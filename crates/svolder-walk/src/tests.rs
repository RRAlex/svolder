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
        }
    }

    fn hearing(self, units: &[&str]) -> Fake {
        let entries = units.iter().map(|unit| entry(unit)).collect();
        self.scans.borrow_mut().push_back(Ok(Heard {
            entries,
            notices: Vec::new(),
        }));
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
