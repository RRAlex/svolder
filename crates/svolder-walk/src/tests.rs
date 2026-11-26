// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The walk's own rules, over a fake machine. The front end's first run walks the same walk
//! through its wizard tests (bins/svolder's wizard/tests.rs); these cover what only a daemon's
//! walk does: the linked step, marked units, a write the writer refuses, a test that got no
//! answer, and the prompt that says what answer it takes.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use svolder_ipc::{Connect, ConnectFailure, DeviceConfig, ScanEntry};
use svolder_profiles::Registry;

use super::*;

/// A machine the walk can be walked over: what each scan hears, what each test answers, what
/// each write answers, and what is configured.
struct Fake {
    registry: Registry,
    scans: RefCell<VecDeque<Result<Heard, String>>>,
    connects: RefCell<VecDeque<Connect>>,
    writes: RefCell<VecDeque<Result<(), WriteError<String>>>>,
    tried: RefCell<Vec<String>>,
    written: RefCell<Vec<DeviceConfig>>,
    taken: Vec<(String, String)>,
    linked: Vec<String>,
    switches: RefCell<Vec<(String, String)>>,
    switch_fails: Option<String>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            registry: Registry::builtin().expect("the built-in profiles load"),
            scans: RefCell::default(),
            connects: RefCell::default(),
            writes: RefCell::default(),
            tried: RefCell::default(),
            written: RefCell::default(),
            taken: Vec::new(),
            linked: Vec::new(),
            switches: RefCell::default(),
            switch_fails: None,
        }
    }

    fn hearing(self, units: &[&str]) -> Fake {
        let entries = units.iter().map(|unit| entry(unit)).collect();
        self.scans.borrow_mut().push_back(Ok(Heard {
            entries,
            notices: Vec::new(),
            adapters: None,
        }));
        self
    }

    /// The next scan has also been heard on every adapter, as `listened` says.
    fn listened_on(self, listened: Listened) -> Fake {
        if let Some(Ok(heard)) = self.scans.borrow_mut().back_mut() {
            heard.adapters = Some(listened);
        }
        self
    }

    fn answering(self, connects: &[Connect]) -> Fake {
        self.connects.borrow_mut().extend(connects.iter().copied());
        self
    }
}

/// A row with this unit id, advertising a product a built-in profile describes.
fn entry(unit: &str) -> ScanEntry {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let product_id = registry
        .names()
        .iter()
        .filter_map(|name| registry.get(name))
        .find_map(|profile| profile.product_id)
        .expect("a built-in profile names a product");
    ScanEntry {
        unit: unit.to_string(),
        product_id,
        product: Some("Gungnir 2".into()),
        firmware: 3,
        passphrase: true,
        rssi: Some(-60),
    }
}

impl WalkEffects for Fake {
    type Failure = String;

    async fn scan(&self) -> Result<Heard, String> {
        self.scans
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok(Heard::default()))
    }

    fn scan_window(&self) -> Duration {
        Duration::from_secs(5)
    }

    async fn try_connect(&self, _: &ScanEntry, passphrase: &str) -> Result<Connect, String> {
        self.tried.borrow_mut().push(passphrase.to_string());
        Ok(self
            .connects
            .borrow_mut()
            .pop_front()
            .unwrap_or(Connect::Accepted))
    }

    async fn write_device(&self, device: &DeviceConfig) -> Result<(), WriteError<String>> {
        let answer = self.writes.borrow_mut().pop_front().unwrap_or(Ok(()));
        if answer.is_ok() {
            self.written.borrow_mut().push(device.clone());
        }
        answer
    }

    fn taken_names(&self) -> Vec<String> {
        self.taken.iter().map(|(name, _)| name.clone()).collect()
    }

    fn taken_units(&self) -> Vec<(String, String)> {
        self.taken.clone()
    }

    fn registry(&self) -> Option<&Registry> {
        Some(&self.registry)
    }

    fn linked(&self) -> Vec<String> {
        self.linked.clone()
    }

    async fn switch_adapter(&self, address: &str, name: &str) -> Result<(), String> {
        if let Some(why) = &self.switch_fails {
            return Err(why.clone());
        }
        self.switches
            .borrow_mut()
            .push((address.to_string(), name.to_string()));
        Ok(())
    }
}

const UNIT: &str = "1112131415161718";
const OTHER: &str = "0102030405060708";

#[tokio::test]
async fn a_walk_with_a_linked_device_warns_first_and_names_it() {
    let mut fake = Fake::new().hearing(&[UNIT]);
    fake.linked = vec!["amp".into(), "dac".into()];
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    let prompt = walk.prompt();
    assert_eq!(prompt.step, Step::Linked);
    assert_eq!(prompt.takes, Takes::Choice);
    assert_eq!(prompt.choices, ["Go on", "Cancel"]);
    assert!(
        prompt.body[0].starts_with("amp, dac have a link"),
        "{prompt:?}"
    );
    assert!(
        prompt.body[0].contains("may briefly disturb them"),
        "{prompt:?}"
    );
    assert_eq!(walk.answer(0, None).await.ok(), Some(Walked::On));
    assert_eq!(walk.step(), Step::Scan);
    assert!(walk.can_go_back(), "the warning is behind the scan");
}

#[tokio::test]
async fn cancel_at_the_linked_step_ends_the_walk() {
    let mut fake = Fake::new();
    fake.linked = vec!["amp".into()];
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    assert!(walk.prompt().body[0].starts_with("amp has a link"));
    assert_eq!(walk.answer(1, None).await.ok(), Some(Walked::Cancelled));
}

#[tokio::test]
async fn nothing_linked_starts_at_the_scan() {
    let walk = DeviceWalk::new(Fake::new(), Given::default(), Configured::Mark);
    assert_eq!(walk.prompt().step, Step::Scan);
    assert!(!walk.can_go_back());
}

#[tokio::test]
async fn a_configured_unit_is_marked_and_picking_it_names_the_device() {
    let mut fake = Fake::new().hearing(&[OTHER, UNIT]);
    fake.taken = vec![("amp".into(), OTHER.into())];
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("the scan runs");
    let prompt = walk.prompt();
    assert_eq!(prompt.step, Step::Pick);
    assert!(
        prompt.choices[0].ends_with("configured as amp"),
        "{prompt:?}"
    );
    assert!(!prompt.choices[1].contains("configured"), "{prompt:?}");
    assert_eq!(walk.rows()[0].configured.as_deref(), Some("amp"));
    match walk.answer(0, None).await {
        Err(WalkError::Configured { name, unit }) => {
            assert_eq!((name.as_str(), unit.as_str()), ("amp", OTHER));
        }
        other => panic!("picking a configured unit is refused: {other:?}"),
    }
    assert_eq!(walk.step(), Step::Pick, "the pick is asked again");
    walk.answer(1, None).await.expect("the other unit is taken");
    assert_eq!(walk.step(), Step::Name);
}

#[tokio::test]
async fn each_prompt_says_what_answer_it_takes() {
    let fake = Fake::new().hearing(&[UNIT]).answering(&[Connect::Refused]);
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    assert_eq!(walk.prompt().takes, Takes::Choice);
    walk.answer(0, None).await.expect("scan");
    assert_eq!(walk.prompt().takes, Takes::Choice);
    walk.answer(0, None).await.expect("pick");
    assert_eq!(walk.prompt().takes, Takes::Text);
    walk.answer(0, Some("desk".into())).await.expect("name");
    let prompt = walk.prompt();
    assert_eq!(
        (prompt.step, prompt.takes),
        (Step::Passphrase, Takes::Secret)
    );
}

#[tokio::test]
async fn a_wrong_code_is_refused_without_quoting_it_and_the_right_one_writes() {
    let fake = Fake::new().hearing(&[UNIT]).answering(&[
        Connect::Refused,
        Connect::Refused,
        Connect::Accepted,
    ]);
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("pick");
    walk.answer(0, Some("desk".into())).await.expect("name");
    let refused = walk
        .answer(0, Some("4321".into()))
        .await
        .expect_err("a wrong code is refused");
    let words = refused.words(|f: &String| f.clone());
    assert!(!words.contains("4321"), "{words}");
    assert!(!format!("{refused:?}").contains("4321"), "{refused:?}");
    assert_eq!(walk.step(), Step::Passphrase);
    assert_eq!(walk.connect(), Some(Connect::Refused));
    let done = walk.answer(0, Some("1234".into())).await.ok();
    assert_eq!(done, Some(Walked::Added));
    let added = walk.added().expect("the device is written");
    assert_eq!(
        (
            added.name.as_str(),
            added.udid.as_str(),
            added.passphrase.as_str()
        ),
        ("desk", UNIT, "1234")
    );
    assert_eq!(*walk.effects().tried.borrow(), ["auto", "4321", "1234"]);
}

#[tokio::test]
async fn a_test_that_got_no_answer_keeps_the_step() {
    let fake = Fake::new().hearing(&[UNIT]).answering(&[Connect::Failed {
        kind: ConnectFailure::NotInRange,
    }]);
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("pick");
    match walk.answer(0, Some("desk".into())).await {
        Err(WalkError::Connect(ConnectFailure::NotInRange)) => {}
        other => panic!("a test with no answer is its own refusal: {other:?}"),
    }
    assert_eq!(walk.step(), Step::Name);
    assert!(!walk.stopped());
}

#[tokio::test]
async fn a_name_taken_at_the_write_goes_back_to_the_name_and_a_unit_to_the_pick() {
    let fake = Fake::new().hearing(&[UNIT]).answering(&[
        Connect::Refused,
        Connect::Accepted,
        Connect::Accepted,
    ]);
    fake.writes
        .borrow_mut()
        .push_back(Err(WriteError::NameTaken("desk".into())));
    fake.writes
        .borrow_mut()
        .push_back(Err(WriteError::UnitConfigured(
            "unit 1112131415161718 is configured already, as amp".into(),
        )));
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("pick");
    walk.answer(0, Some("desk".into())).await.expect("name");
    assert!(matches!(
        walk.answer(0, Some("1234".into())).await,
        Err(WalkError::NameTaken(name)) if name == "desk"
    ));
    assert_eq!(walk.step(), Step::Name);
    assert!(matches!(
        walk.answer(0, Some("study".into())).await,
        Err(WalkError::UnitConfigured(_))
    ));
    assert_eq!(walk.step(), Step::Pick);
    assert!(!walk.stopped());
}

#[tokio::test]
async fn a_file_that_cannot_take_the_write_stops_the_walk() {
    let fake = Fake::new().hearing(&[UNIT]);
    fake.writes
        .borrow_mut()
        .push_back(Err(WriteError::Stop("read-only".into())));
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("pick");
    assert!(matches!(
        walk.answer(0, Some("desk".into())).await,
        Err(WalkError::Effect(why)) if why == "read-only"
    ));
    assert!(walk.stopped());
}

#[test]
fn a_secret_and_a_given_passphrase_never_print() {
    assert_eq!(format!("{:?}", Secret::new("4321")), "<hidden>");
    let given = Given {
        passphrase: Some("4321".into()),
        ..Given::default()
    };
    assert!(!format!("{given:?}").contains("4321"));
}

#[test]
fn a_prompt_and_a_row_travel_as_json() {
    let prompt = Prompt {
        step: Step::Passphrase,
        title: "t".into(),
        body: vec!["b".into()],
        choices: Vec::new(),
        default: 0,
        takes: Takes::Secret,
    };
    let json = serde_json::to_value(&prompt).expect("a prompt is JSON");
    assert_eq!(json["step"], "passphrase");
    assert_eq!(json["takes"], "secret");
    let row = Row {
        entry: entry(UNIT),
        configured: Some("amp".into()),
    };
    let json = serde_json::to_value(&row).expect("a row is JSON");
    assert_eq!(
        (json["unit"].as_str(), json["configured"].as_str()),
        (Some(UNIT), Some("amp"))
    );
    let back: Row = serde_json::from_value(json).expect("and back");
    assert_eq!(back, row);
}

/// A profile chosen for a product no profile covers belongs to that pick: back at the pick, a
/// device the registry matches is offered and written with no profile pinned.
#[tokio::test]
async fn a_profile_chosen_for_an_unknown_product_does_not_stick_to_the_next_pick() {
    let unknown = ScanEntry {
        product_id: 0,
        product: None,
        ..entry(OTHER)
    };
    let fake = Fake::new();
    fake.scans.borrow_mut().push_back(Ok(Heard {
        entries: vec![unknown, entry(UNIT)],
        notices: Vec::new(),
        adapters: None,
    }));
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("the unknown product");
    assert_eq!(walk.prompt().title, "Which profile");
    walk.answer(1, None).await.expect("a profile");
    assert_eq!(walk.step(), Step::Name);
    assert!(walk.back());
    assert_eq!(walk.step(), Step::Pick);
    let pick = walk.prompt();
    assert!(
        pick.choices[0].ends_with(UNKNOWN_PRODUCT),
        "the unknown product's line shows no profile: {pick:?}"
    );
    walk.answer(1, None).await.expect("the known device");
    let done = walk.answer(0, Some("desk".into())).await.expect("name");
    assert_eq!(done, Walked::Added);
    let added = walk.added().expect("written");
    assert_eq!(added.udid, UNIT);
    assert_eq!(added.profile, None, "the other pick's profile was written");
}

/// Two adapters, `hci2` in use, hearing `UNIT` at the readings given (`None`: not heard).
fn two_adapters(on_hci0: Option<Option<i16>>, on_hci2: Option<Option<i16>>) -> Listened {
    let on = |name: &str, address: &str, reading: Option<Option<i16>>| ListenedOn {
        name: name.into(),
        address: address.into(),
        heard: reading
            .map(|rssi| (UNIT.to_string(), rssi))
            .into_iter()
            .collect(),
        adverts: Vec::new(),
    };
    Listened {
        in_use: "hci2".into(),
        adapters: vec![
            on("hci0", "AA:BB:CC:DD:EE:01", on_hci0),
            on("hci2", "AA:BB:CC:DD:EE:02", on_hci2),
        ],
    }
}

/// Walk to the pick over a machine with two adapters and take the device.
async fn picked_over(fake: Fake) -> DeviceWalk<Fake> {
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    walk.answer(0, None).await.expect("pick");
    walk
}

/// The pick shows each adapter's reading per device, and when the adapter in use is not the one
/// that hears the device best the walk asks before the name: the table of readings, the sentence,
/// and the two ways to add it.
#[tokio::test]
async fn the_adapter_question_is_asked_when_another_adapter_hears_the_device_better() {
    let fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80))));
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    let pick = walk.prompt();
    assert_eq!(pick.step, Step::Pick);
    assert!(
        pick.body
            .iter()
            .any(|l| l == "Heard on 2 adapters; svolder uses hci2."),
        "{pick:?}"
    );
    assert!(
        pick.choices[0].contains("hci0 \u{2212}74 dBm  hci2 \u{2212}80 dBm"),
        "{pick:?}"
    );
    walk.answer(0, None).await.expect("pick");
    let ask = walk.prompt();
    assert_eq!(ask.step, Step::Adapter);
    assert_eq!(ask.title, "Which adapter");
    assert_eq!(
        ask.choices,
        ["Add it and switch to hci0", "Add it, keep hci2"]
    );
    assert_eq!(ask.default, 0);
    assert!(
        ask.body[0].starts_with("Device") && ask.body[0].contains("hci0"),
        "{ask:?}"
    );
    assert!(
        ask.body[1].contains("\u{2212}74 dBm") && ask.body[1].contains("\u{2212}80 dBm"),
        "{ask:?}"
    );
    assert_eq!(
        ask.body[2],
        "hci0 hears the Gungnir 2 best; svolder uses hci2."
    );
}

/// A device only another adapter hears is offered all the same, and picking it asks the question:
/// the adapter in use hears nothing, so the other one hears it better (`better_for`'s first arm).
#[tokio::test]
async fn a_device_only_another_adapter_hears_is_listed_and_asks_the_question() {
    let mut listened = two_adapters(Some(Some(-74)), None);
    listened.adapters[0].adverts = vec![svolder_proto::ManufacturerData {
        product_id: 2,
        incomplete_dfu: false,
        unit_id: UNIT.parse().unwrap(),
        encrypted_passphrase: 0,
        firmware_build: 3,
        module_hardware: 0,
        module_firmware: 0,
    }];
    // The scan of the adapter in use heard nothing at all.
    let fake = Fake::new().hearing(&[]).listened_on(listened);
    let mut walk = DeviceWalk::new(fake, Given::default(), Configured::Mark);
    walk.answer(0, None).await.expect("scan");
    let pick = walk.prompt();
    assert_eq!(
        pick.step,
        Step::Pick,
        "nothing answered, though hci0 hears it"
    );
    assert!(pick.choices[0].contains("hci0 \u{2212}74 dBm"), "{pick:?}");
    walk.answer(0, None).await.expect("pick");
    let ask = walk.prompt();
    assert_eq!(ask.step, Step::Adapter);
    assert_eq!(
        ask.choices,
        ["Add it and switch to hci0", "Add it, keep hci2"]
    );
}

/// "Add it and switch": the adapter is switched to before the passphrase is tested, and named
/// by its address; the device is then written.
#[tokio::test]
async fn adding_and_switching_switches_before_the_test_and_names_the_address() {
    let fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80))));
    let mut walk = picked_over(fake).await;
    assert_eq!(walk.answer(0, None).await.ok(), Some(Walked::On));
    assert_eq!(
        *walk.effects().switches.borrow(),
        [("AA:BB:CC:DD:EE:01".to_string(), "hci0".to_string())]
    );
    assert_eq!(walk.switched(), Some("hci0"));
    assert!(
        walk.effects().tried.borrow().is_empty(),
        "tested before switching"
    );
    assert_eq!(walk.step(), Step::Name);
    assert_eq!(
        walk.answer(0, Some("desk".into())).await.ok(),
        Some(Walked::Added)
    );
    assert_eq!(walk.added().map(|d| d.udid.as_str()), Some(UNIT));
}

/// "Add it, keep hci2": nothing is switched, and the walk goes on to the name.
#[tokio::test]
async fn adding_and_keeping_the_adapter_switches_nothing() {
    let fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80))));
    let mut walk = picked_over(fake).await;
    assert_eq!(walk.answer(1, None).await.ok(), Some(Walked::On));
    assert!(walk.effects().switches.borrow().is_empty());
    assert_eq!(walk.switched(), None);
    assert_eq!(walk.step(), Step::Name);
    // An answer that is none of the two is refused, and the step stays.
    let mut again = picked_over(
        Fake::new()
            .hearing(&[UNIT])
            .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80)))),
    )
    .await;
    assert!(again.answer(2, None).await.is_err());
    assert_eq!(again.step(), Step::Adapter);
}

/// When the adapter in use already hears the device best, nothing is asked: the walk goes from
/// the pick to the name as it did, with the readings shown on the pick all the same.
#[tokio::test]
async fn nothing_is_asked_when_the_adapter_in_use_hears_the_device_best() {
    let fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-80)), Some(Some(-74))));
    let walk = picked_over(fake).await;
    assert_eq!(walk.step(), Step::Name);
    assert!(walk.effects().switches.borrow().is_empty());
    // A tie, and a reading with no signal, are no louder either.
    let tie = picked_over(
        Fake::new()
            .hearing(&[UNIT])
            .listened_on(two_adapters(Some(Some(-74)), Some(Some(-74)))),
    )
    .await;
    assert_eq!(tie.step(), Step::Name);
    let unreadable = picked_over(
        Fake::new()
            .hearing(&[UNIT])
            .listened_on(two_adapters(Some(None), Some(Some(-90)))),
    )
    .await;
    assert_eq!(unreadable.step(), Step::Name);
}

/// An adapter in use that does not hear the device at all is asked about whenever another does;
/// a machine that listened on one adapter, or no adapter list at all, is asked nothing.
#[tokio::test]
async fn an_adapter_in_use_that_hears_nothing_is_asked_about_and_one_adapter_is_not() {
    let deaf = picked_over(
        Fake::new()
            .hearing(&[UNIT])
            .listened_on(two_adapters(Some(Some(-90)), None)),
    )
    .await;
    assert_eq!(deaf.step(), Step::Adapter);
    let plain = picked_over(Fake::new().hearing(&[UNIT])).await;
    assert_eq!(plain.step(), Step::Name);
    assert!(plain.listened().is_none());
}

/// A switch that failed keeps the walk on its question, with the failure for the person, and
/// writes nothing.
#[tokio::test]
async fn a_switch_that_fails_leaves_the_walk_on_the_question() {
    let mut fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80))));
    fake.switch_fails = Some("the daemon is pinned".into());
    let mut walk = picked_over(fake).await;
    assert!(walk.answer(0, None).await.is_err());
    assert_eq!(walk.step(), Step::Adapter);
    assert_eq!(walk.switched(), None);
    assert!(walk.effects().written.borrow().is_empty());
}

/// Back from the name returns to the question, and keeping after a switch goes back to the
/// adapter that was in use.
#[tokio::test]
async fn keeping_after_a_switch_goes_back_to_the_adapter_in_use() {
    let fake = Fake::new()
        .hearing(&[UNIT])
        .listened_on(two_adapters(Some(Some(-74)), Some(Some(-80))));
    let mut walk = picked_over(fake).await;
    walk.answer(0, None).await.expect("switch");
    assert!(walk.back());
    assert_eq!(walk.step(), Step::Adapter);
    walk.answer(1, None).await.expect("keep");
    assert_eq!(
        *walk.effects().switches.borrow(),
        [
            ("AA:BB:CC:DD:EE:01".to_string(), "hci0".to_string()),
            ("AA:BB:CC:DD:EE:02".to_string(), "hci2".to_string())
        ]
    );
    assert_eq!(walk.switched(), None);
}
