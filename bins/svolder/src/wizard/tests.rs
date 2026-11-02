// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use async_trait::async_trait;
use svolder_ipc::checks::State;
use svolder_profiles::Registry;

use super::fake::{FakeEffects, seen, seen_product, walk_to};
use super::*;
use crate::commands::daemon::install::CONTAINER_DAEMON;
use crate::scanning::Scanned;

#[tokio::test]
async fn the_wizard_walks_every_step_and_writes_the_device_the_adapter_and_the_daemon() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", 10)])
        .with_adapters([("hci0", 2), ("hci1", 5)]);
    let mut wizard = Wizard::new(fx.clone());
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Health);
    wizard.answer(0, None).await.unwrap(); // continue
    // No fixes on a healthy host: Fixes is skipped.
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Scan);
    wizard.answer(0, None).await.unwrap(); // scan
    let pick = wizard.prompt().await.unwrap();
    assert_eq!(pick.step, Step::Pick);
    assert!(pick.choices[0].contains("Bifrost 3"));
    wizard.answer(0, None).await.unwrap();
    let name = wizard.prompt().await.unwrap();
    assert_eq!(name.step, Step::Name);
    assert_eq!(name.body.last().unwrap(), "Bifrost 3"); // the default
    wizard.answer(0, Some("dac".into())).await.unwrap();
    // The passphrase is skipped: the connect was accepted.
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Adapter);
    wizard.answer(0, None).await.unwrap(); // take the graded best
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    wizard.answer(0, None).await.unwrap(); // install
    let done = wizard.prompt().await.unwrap();
    assert_eq!(done.step, Step::Done);
    assert!(
        done.body.iter().all(|line| !line.contains('\n')),
        "a body line is one line: {done:?}"
    );
    assert!(
        done.body.iter().all(|line| !line.contains("svolder tui")),
        "where to go next is the front end's own to add: {done:?}"
    );
    assert_eq!(next_command(true), "Run `svolder tui` to open it.");
    assert_eq!(fx.written_device().unwrap().name, "dac");
    assert_eq!(fx.written_adapter(), Some("hci1".into()));
    assert!(fx.daemon_installed());
    assert_eq!(
        fx.reloads(),
        1,
        "the daemon is told once, with everything this run wrote already in the file"
    );
}

/// The daemon is told to read the file only once everything the run writes is in it, so a
/// daemon that holds the device does not hold it through the adapter grading.
#[tokio::test]
async fn the_daemon_is_told_once_and_only_after_the_adapter_has_been_written() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 2), ("hci1", 5)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Adapter);
    assert_eq!(
        fx.reloads(),
        0,
        "the daemon was told before the adapters were graded"
    );
    assert!(
        fx.written_device().is_some(),
        "the file was written, though"
    );

    wizard.answer(0, None).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.reloads(), 1);

    // A host with one adapter never reaches that step, so it is told as the device is
    // written — and still only once.
    let one = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 5)]);
    let mut wizard = Wizard::new(one.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(one.reloads(), 1);
    wizard.answer(1, None).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Done);
    assert_eq!(one.reloads(), 1, "the daemon was told twice");
}

/// A walk that has written its device does not go back over the steps that wrote it, so
/// the one reload it sent stands.
///
/// The two facts are the same fact: the reload happens only once a device is in the file,
/// and a device in the file is exactly what stops [`Wizard::back`] moving. A walk that did
/// return to the adapter step would send another reload when it reached the daemon step
/// again, carrying whichever adapter was written the second time.
#[tokio::test]
async fn a_written_device_is_not_walked_back_over_and_its_reload_stands() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 2), ("hci1", 5)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();

    assert_eq!(wizard.prompt().await.unwrap().step, Step::Adapter);
    wizard.back();
    assert_eq!(
        wizard.prompt().await.unwrap().step,
        Step::Adapter,
        "a written device was walked back over"
    );
    wizard.answer(1, None).await.unwrap(); // the second-best adapter
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.written_adapter(), Some("hci0".into()));
    assert_eq!(fx.reloads(), 1);

    wizard.back();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.reloads(), 1, "a step nobody walked again sent a reload");
}

/// A run that wrote nothing tells no daemon anything: there is nothing for it to read.
#[tokio::test]
async fn a_run_that_added_no_device_tells_no_daemon() {
    let fx = FakeEffects::healthy().with_scan(vec![]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.unwrap();
    wizard.answer(1, None).await.unwrap(); // skip
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.reloads(), 0);
}

#[tokio::test]
async fn a_refused_connect_asks_for_the_passphrase_and_a_taken_name_is_refused() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen("0102030405060708")])
        .refusing_without("1234")
        .with_taken(["amp"]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    assert!(wizard.answer(0, Some("amp".into())).await.is_err()); // taken
    wizard.answer(0, Some("dac".into())).await.unwrap();
    let code = wizard.prompt().await.unwrap();
    assert_eq!(code.step, Step::Passphrase);
    assert_eq!(
        code.body.last().map(String::as_str),
        Some(NONE),
        "the last line of a typed step is what an empty line answers with: {code:?}"
    );
    wizard.answer(0, Some("1234".into())).await.unwrap();
    assert_eq!(fx.written_device().unwrap().passphrase, "1234");
}

/// The plain prompt's passphrase step takes the typed answers it always did and `auto` as well:
/// `auto` is tried like any code, and a device that turns it away is said to have, with the step
/// asked again; a blank line is a device with no passphrase.
#[tokio::test]
async fn the_passphrase_step_takes_auto_and_a_blank_line_as_typed_answers() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen("0102030405060708")])
        .refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Passphrase);

    let refused = wizard.answer(0, Some("auto".into())).await.unwrap_err();
    assert_eq!(refused.what, "the device did not accept auto");
    assert_eq!(
        refused.next,
        crate::recovery::REFUSED_UNDER_AUTO,
        "`auto` was refused: there is no `auto` left to offer"
    );
    assert!(
        refused
            .next
            .contains("already read the code afresh once, so power-cycle the device"),
        "{refused}"
    );
    assert_eq!(fx.tried().last().map(String::as_str), Some("auto"));
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Passphrase);

    // A code that is refused still sends the user to `auto` first.
    let wrong = wizard.answer(0, Some("9999".into())).await.unwrap_err();
    assert!(wrong.next.contains("choose `auto`"), "{wrong}");

    let short = wizard.answer(0, Some("12".into())).await.unwrap_err();
    assert!(
        short.to_string().starts_with("a code is four digits"),
        "{short}"
    );
    let tries = fx.tried().len();
    wizard.answer(0, Some("1234".into())).await.unwrap();
    assert_eq!(
        fx.tried().len(),
        tries + 1,
        "the short answer opened no session"
    );
    assert_eq!(fx.written_device().unwrap().passphrase, "1234");
}

#[tokio::test]
async fn nothing_found_offers_a_rescan_and_an_unknown_product_offers_the_profiles() {
    let fx = FakeEffects::healthy().with_scan(vec![]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.unwrap();
    let again = wizard.prompt().await.unwrap();
    assert_eq!(again.step, Step::Scan);
    assert!(
        again.body.iter().any(|l| l.contains("phone app"))
            && again.choices.contains(&"Scan again".to_string())
    );
}

/// Giving up on a scan that hears nothing leaves no device behind and goes on to the
/// daemon, which is worth installing whether or not there was anything to add.
#[tokio::test]
async fn giving_up_on_a_quiet_scan_goes_on_to_the_daemon() {
    let fx = FakeEffects::healthy().with_scan(vec![]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.unwrap();
    wizard.answer(1, None).await.unwrap(); // skip
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    wizard.answer(1, None).await.unwrap(); // skip the install too
    let done = wizard.prompt().await.unwrap();
    assert_eq!(done.step, Step::Done);
    assert!(
        done.body.iter().any(|l| l.contains("No device")),
        "{done:?}"
    );
    assert!(
        next_command(false).contains("svolder setup"),
        "a run that added nothing is sent back to the verb that adds one"
    );
    assert!(fx.written_device().is_none());
    assert!(!fx.daemon_installed());
}

/// A device whose product id the codec's table does not know, heard with no product name,
/// is offered under the name of the profile made for it — the fake amp as `Fake amp` — and
/// that is the name it is written with by default.
#[tokio::test]
async fn the_fake_amp_is_offered_under_the_name_of_its_profile() {
    let amp = ScanEntry {
        product_id: 65534,
        ..seen("0102030405060708")
    };
    let fx = FakeEffects::healthy().with_scan(vec![amp]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Pick).await;
    let pick = wizard.prompt().await.unwrap();
    assert!(
        pick.choices[0].starts_with("Fake amp  0102030405060708"),
        "{pick:?}"
    );
    assert!(pick.choices[0].ends_with("fake-amp"), "{pick:?}");
    wizard.answer(0, None).await.unwrap();
    assert_eq!(wizard.default_name(), "Fake amp");
}

/// A product no profile covers says so on its line and asks which profile to read it
/// through; the one chosen is what the device is written with, and names it by default.
#[tokio::test]
async fn an_unknown_product_offers_the_profile_list_and_writes_the_one_chosen() {
    let fx = FakeEffects::healthy().with_scan(vec![seen("0102030405060708")]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Pick).await;
    let pick = wizard.prompt().await.unwrap();
    assert!(pick.choices[0].contains(UNKNOWN_PRODUCT), "{pick:?}");
    wizard.answer(0, None).await.unwrap();

    let profiles = wizard.prompt().await.unwrap();
    assert_eq!(profiles.step, Step::Pick);
    assert!(
        profiles
            .body
            .iter()
            .any(|l| l.contains("no profile covers")),
        "{profiles:?}"
    );
    // Going back from the profiles is the device list again, not the step before it.
    wizard.back();
    assert_eq!(wizard.prompt().await.unwrap().choices, pick.choices);
    wizard.answer(0, None).await.unwrap();

    let profiles = wizard.prompt().await.unwrap();
    let at = profiles
        .choices
        .iter()
        .position(|name| name == "bifrost3")
        .expect("the built-in profiles are offered");
    wizard.answer(at, None).await.unwrap();
    let name = wizard.prompt().await.unwrap();
    assert_eq!(name.step, Step::Name);
    assert_eq!(
        name.body.last().unwrap(),
        "Bifrost 3",
        "the profile names it"
    );
    wizard.answer(0, None).await.unwrap();
    let written = fx.written_device().expect("the device is written");
    assert_eq!(written.profile.as_deref(), Some("bifrost3"));
    assert_eq!(written.name, "Bifrost 3");
}

/// The fixes step offers only what svolder may apply itself, applies them when the answer
/// asks for it, and says what they did on the way to the scan.
///
/// The two fixes the wizard is itself are not among them: this run is the guided first run,
/// and the daemon has a step of its own further down.
#[tokio::test]
async fn the_fixes_step_applies_what_it_offers_and_leaves_the_wizard_s_own_steps_out() {
    let template = "create /tmp/nowhere/config.toml from the template";
    let fx = FakeEffects::healthy()
        .with_fix(template, true)
        .with_fix(checks::DAEMON_INSTALL, true)
        .with_fix(checks::SETUP, true)
        .with_fix("sudo systemctl enable --now bluetooth", false);
    let mut wizard = Wizard::new(fx.clone());
    let health = wizard.prompt().await.unwrap();
    assert!(
        health.body.iter().any(|l| l.contains("sudo systemctl")),
        "a root fix is printed to be typed: {health:?}"
    );
    wizard.answer(0, None).await.unwrap();

    let fixes = wizard.prompt().await.unwrap();
    assert_eq!(fixes.step, Step::Fixes);
    assert_eq!(
        fixes.choices,
        vec!["Apply 1 fix".to_string(), "Skip".into()]
    );
    assert_eq!(
        fixes.choices.get(fixes.default).map(String::as_str),
        Some("Skip"),
        "changing the machine is not what an answer nobody typed asks for"
    );
    assert!(fixes.body.iter().any(|l| l.contains(template)), "{fixes:?}");
    wizard.answer(0, None).await.unwrap();
    assert_eq!(fx.applied(), vec![template.to_string()]);
    let scan = wizard.prompt().await.unwrap();
    assert_eq!(scan.step, Step::Scan);
    assert!(
        scan.body.iter().any(|l| l.contains(template)),
        "what was applied is said: {scan:?}"
    );
}

/// Skipping the fixes applies none of them.
#[tokio::test]
async fn skipping_the_fixes_changes_nothing() {
    let fx = FakeEffects::healthy().with_fix("create it from the template", true);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Fixes).await;
    wizard.answer(1, None).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Scan);
    assert!(fx.applied().is_empty());
}

/// A passphrase answer is the four digits the app shows, `auto`, or nothing at all — typed as an
/// empty line, or as the word the step itself shows for one; anything else is refused, in the
/// words the TUI uses for a code short of four digits, before a session is opened with it.
#[test]
fn a_passphrase_is_four_digits_auto_or_nothing_at_all() {
    assert_eq!(passphrase_text("1234").expect("four digits"), "1234");
    assert_eq!(passphrase_text("  ").expect("nothing at all"), NONE);
    for word in ["none", "None", " NONE "] {
        assert_eq!(
            passphrase_text(word).expect("the word the field shows"),
            NONE,
            "{word}"
        );
    }
    for word in ["auto", "Auto", " AUTO "] {
        assert_eq!(
            passphrase_text(word).expect("the advertised code"),
            AUTO,
            "{word}"
        );
    }
    for wrong in ["12", "12345", "abcd", "12 4"] {
        let refused = passphrase_text(wrong).expect_err(wrong);
        assert!(
            refused.to_string().starts_with("a code is four digits"),
            "{wrong}: {refused}"
        );
    }
}

/// A scan that cannot run at all is tried a few times and then given up on: answering the
/// same step again would ask the same adapter the same thing.
#[tokio::test]
async fn a_scan_that_can_never_run_ends_the_run_rather_than_being_asked_forever() {
    let fx = FakeEffects::healthy().scan_failing("no adapter named hci9");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Scan).await;
    for _ in 1..SCAN_ATTEMPTS {
        let again = wizard.answer(0, None).await.expect_err("the scan failed");
        assert!(again.what.contains("hci9"), "{}", again.what);
        assert_eq!(wizard.stopped(), None, "one failure is worth another try");
        assert_eq!(wizard.prompt().await.map(|p| p.step), Some(Step::Scan));
    }
    let last = wizard.answer(0, None).await.expect_err("the scan failed");
    assert_eq!(wizard.stopped(), Some(last.code));
    assert!(
        wizard.prompt().await.is_none(),
        "there is nothing left to ask"
    );
    assert_eq!(fx.scans(), SCAN_ATTEMPTS as usize);
}

/// A machine whose adapters went away between being counted and being graded has nothing
/// to choose between, and is offered the one thing there is to do about it.
#[tokio::test]
async fn an_adapter_step_with_nothing_to_grade_offers_a_way_on() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let mut wizard = Wizard::starting_at(fx.clone(), Step::Adapter, Given::default());
    let prompt = wizard.prompt().await.expect("the adapter step");
    assert_eq!(prompt.choices, vec!["Continue".to_string()]);
    wizard.answer(0, None).await.expect("the one way on");
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.written_adapter(), None, "nothing was graded to write");
}

/// A code the device turns away too is said to have been, and the step is asked again
/// rather than written into the file.
#[tokio::test]
async fn a_code_the_device_turns_away_is_asked_again() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Passphrase);

    let refused = wizard
        .answer(0, Some("9999".into()))
        .await
        .expect_err("the device turned it away");
    assert_eq!(refused.code, Kind::Refused.code());
    assert!(refused.what.contains("9999"), "{}", refused.what);
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Passphrase);
    assert!(fx.written_device().is_none());

    let mistyped = wizard
        .answer(0, Some("12".into()))
        .await
        .expect_err("two digits are not four");
    assert_eq!(mistyped.code, Kind::Usage.code());
    assert_eq!(fx.tried(), vec!["auto".to_string(), "9999".into()]);
}

/// One adapter is no choice to offer, so the step is not walked and nothing is written.
#[tokio::test]
async fn one_adapter_is_no_choice_at_all() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 1)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
    assert_eq!(fx.written_adapter(), None);
}

/// The adapters are offered best first, and adapters that heard the device equally often
/// are a tie the first of them wins — the same adapter `doctor`'s own grade row names.
#[tokio::test]
async fn the_adapters_are_offered_best_first_and_a_tie_goes_to_the_first() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 3), ("hci1", 5), ("hci2", 5)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    let adapter = wizard.prompt().await.unwrap();
    assert_eq!(adapter.step, Step::Adapter);
    let named: Vec<&str> = adapter
        .choices
        .iter()
        .map(|choice| choice.split_whitespace().next().unwrap_or_default())
        .collect();
    assert_eq!(named, ["hci1", "hci2", "hci0"]);
    wizard.answer(0, None).await.unwrap();
    assert_eq!(fx.written_adapter(), Some("hci1".into()));
}

/// A machine that generates its unit files is shown the block to put in its configuration,
/// and a container is told that the daemon is its own process; neither is offered an
/// install, and neither has one run.
#[tokio::test]
async fn a_generated_machine_gets_the_block_and_a_container_gets_its_sentence() {
    let nixos = FakeEffects::healthy().on(Platform {
        systemd: true,
        nixos: true,
        container: false,
    });
    let mut wizard = Wizard::starting_at(nixos.clone(), Step::Daemon, Given::default());
    let prompt = wizard.prompt().await.unwrap();
    assert_eq!(prompt.choices, vec!["Continue".to_string()]);
    assert!(
        prompt
            .body
            .iter()
            .any(|l| l.contains("systemd.user.sockets.svolderd")),
        "{prompt:?}"
    );
    wizard.answer(0, None).await.unwrap();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Done);
    assert!(!nixos.daemon_installed());

    let boxed = FakeEffects::healthy().on(Platform {
        systemd: true,
        nixos: false,
        container: true,
    });
    let mut wizard = Wizard::starting_at(boxed.clone(), Step::Daemon, Given::default());
    let prompt = wizard.prompt().await.unwrap();
    assert_eq!(prompt.choices, vec!["Continue".to_string()]);
    assert!(
        prompt.body.iter().any(|l| l.contains("container's own")),
        "{prompt:?}"
    );
    wizard.answer(0, None).await.unwrap();
    assert!(!boxed.daemon_installed());
}

/// What the command line already answered is what the wizard offers: the name it defaults
/// to, the profile it writes, and the passphrase it tries first.
#[tokio::test]
async fn what_the_command_line_already_answered_is_what_the_wizard_offers() {
    let fx = FakeEffects::healthy().with_scan(vec![seen("0102030405060708")]);
    let given = Given {
        name: Some("amp".into()),
        profile: Some("bifrost3".into()),
        passphrase: Some("1234".into()),
    };
    let mut wizard = Wizard::starting_at(fx.clone(), Step::Scan, given);
    wizard.answer(0, None).await.unwrap(); // scan
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Pick);
    // The profile was given, so the product no profile covers asks nothing about one.
    wizard.answer(0, None).await.unwrap();
    let name = wizard.prompt().await.unwrap();
    assert_eq!(name.step, Step::Name);
    assert_eq!(name.body.last().unwrap(), "amp");
    wizard.answer(0, None).await.unwrap();

    let written = fx.written_device().expect("the device is written");
    assert_eq!(written.name, "amp");
    assert_eq!(written.profile.as_deref(), Some("bifrost3"));
    assert_eq!(written.passphrase, "1234");
    assert_eq!(fx.tried(), vec!["1234".to_string()]);
}

/// Going back returns to the step before this one, and stops once the device has been
/// written: the steps behind it have all been applied.
#[tokio::test]
async fn going_back_returns_to_the_step_before_and_stops_once_the_device_is_written() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard.back();
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Pick);
    wizard.answer(0, None).await.unwrap();
    wizard.answer(0, Some("dac".into())).await.unwrap();

    let after = wizard.prompt().await.unwrap().step;
    wizard.back();
    assert_eq!(
        wizard.prompt().await.unwrap().step,
        after,
        "a written device is not walked back over"
    );
    assert_eq!(
        wizard.added().map(|device| device.name.clone()),
        Some("dac".into())
    );
}

/// A choice no prompt offered is refused, and the step stays where it was.
#[tokio::test]
async fn a_choice_that_was_never_offered_is_refused() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Pick).await;
    let refused = wizard
        .answer(7, None)
        .await
        .expect_err("there is no choice 8");
    assert_eq!(refused.code, Kind::Usage.code());
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Pick);
}

/// What opening a backend for the scan had to say is shown where the user is looking,
/// rather than printed over whatever a front end has on the screen.
#[tokio::test]
async fn what_opening_the_backend_said_is_shown_rather_than_printed() {
    let notice = "the daemon did not answer in 2s; using a direct session";
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_notices([notice]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.unwrap();
    let pick = wizard.prompt().await.unwrap();
    assert_eq!(pick.step, Step::Pick);
    assert!(pick.body.iter().any(|line| line == notice), "{pick:?}");

    // A scan that heard nothing shows it beside the reason a device stays quiet.
    let quiet = FakeEffects::healthy()
        .with_scan(vec![])
        .with_notices([notice]);
    let mut wizard = Wizard::new(quiet);
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.unwrap();
    let again = wizard.prompt().await.unwrap();
    assert!(again.body.iter().any(|line| line == notice), "{again:?}");
}

/// The health rows carry the fixes that are commands to type, and not the two the wizard is
/// itself: a step that told the reader to run what is running would be telling them to run
/// this.
#[test]
fn the_health_rows_leave_out_the_fixes_the_wizard_walks_itself() {
    let row = |check: &'static str, command: &str, applies_itself: bool| Outcome {
        check,
        state: State::Warn,
        reason: "because".into(),
        fix: Some(Fix {
            command: command.to_string(),
            applies_itself,
        }),
    };
    let lines = health_lines(&[
        row("Config", checks::SETUP, true),
        row("Daemon", checks::DAEMON_INSTALL, true),
        row(
            "Bluetooth service",
            "sudo systemctl enable --now bluetooth",
            false,
        ),
    ]);
    assert_eq!(
        lines.iter().filter(|line| line.contains("fix:")).count(),
        1,
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("sudo systemctl")),
        "a fix to type stands under its row: {lines:?}"
    );
    assert_eq!(lines.len(), 4, "every row is still shown: {lines:?}");
}

/// A machine that will not take what a step writes stops the run: answering the same step
/// again would ask the same unwritable file the same thing.
#[tokio::test]
async fn a_write_the_machine_refuses_stops_the_run() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .unwritable();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let refused = wizard
        .answer(0, Some("dac".into()))
        .await
        .expect_err("the file will not take it");
    assert_eq!(refused.code, Kind::ConfigUnreadable.code());
    assert_eq!(wizard.stopped(), Some(Kind::ConfigUnreadable.code()));
    assert!(
        wizard.prompt().await.is_none(),
        "there is nothing left to ask"
    );
    assert!(wizard.busy().is_none());
    assert!(fx.written_device().is_none());
}

/// The seconds the adapter step spends grading are named before they are spent, and only
/// when there is grading to do.
#[tokio::test]
async fn the_grading_is_named_before_it_runs() {
    let fx = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 2), ("hci1", 5)]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Name).await;
    assert_eq!(
        wizard.busy(),
        None,
        "nothing is graded before the device is written"
    );
    wizard.answer(0, Some("dac".into())).await.unwrap();
    let busy = wizard.busy().expect("the adapters are about to be graded");
    assert!(busy.contains("2 adapters"), "{busy}");
    assert!(busy.contains("10 s"), "{busy}");
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Adapter);
    assert_eq!(wizard.busy(), None, "the grading has run");

    // One adapter never reaches the step, so there is nothing to wait for.
    let one = FakeEffects::healthy()
        .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
        .with_adapters([("hci0", 5)]);
    let mut wizard = Wizard::new(one);
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert_eq!(wizard.busy(), None);
    assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
}

/// The summary is the wizard with nobody to ask: the health rows, the `add` line under each
/// device in range, and what applies to the daemon here. It writes nothing.
#[tokio::test]
async fn the_summary_prints_the_health_the_add_line_and_the_daemon_line() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let (lines, code) = summary::rendered(&fx).await;
    assert_eq!(code, 0);
    assert!(
        lines.iter().any(|l| l.starts_with("✓ Bluetooth service")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("svolder add 0102030405060708 --name \"Bifrost 3\"")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("svolder daemon install")),
        "{lines:?}"
    );
    assert!(fx.written_device().is_none(), "the summary wrote a device");
}

/// A summary over a host that heard nothing says why a device usually stays quiet.
#[tokio::test]
async fn a_summary_that_heard_nothing_says_what_to_do_about_it() {
    let (lines, code) = summary::rendered(&FakeEffects::healthy().with_scan(vec![])).await;
    assert_eq!(code, 0);
    assert!(lines.iter().any(|l| l.contains("phone app")), "{lines:?}");
}

/// The wizard says it can go back until the device is written, and again never after.
#[tokio::test]
async fn the_wizard_says_when_going_back_does_nothing() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Name).await;
    assert!(wizard.can_go_back());
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert!(!wizard.can_go_back(), "the device is written");
}

/// A walk started at the scan has nothing behind its first step, so going back does nothing
/// there; it can once a step is answered, and cannot again once the device is written.
#[tokio::test]
async fn a_walk_started_at_the_scan_cannot_go_back_from_its_first_step() {
    let fx =
        FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)]);
    let mut wizard = Wizard::starting_at(fx, Step::Scan, Given::default());
    let first = wizard.prompt().await.unwrap();
    assert_eq!(first.step, Step::Scan);
    assert!(!wizard.can_go_back(), "nothing is behind the first step");
    wizard.answer(first.default, None).await.unwrap();
    assert!(wizard.can_go_back(), "one step is behind");
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, Some("dac".into())).await.unwrap();
    assert!(!wizard.can_go_back(), "the device is written");
}

/// The first step ends with a choice that leaves the walk for later, which writes nothing and
/// ends the walk with nothing left to ask.
#[tokio::test]
async fn the_first_step_offers_later_as_its_last_choice_and_choosing_it_ends_the_walk() {
    let fx = FakeEffects::healthy();
    let mut wizard = Wizard::new(fx.clone());
    let health = wizard.prompt().await.unwrap();
    assert_eq!(health.choices.last().map(String::as_str), Some(LATER));
    assert_eq!(LATER, "Later: open svolder without a device");
    assert!(wizard.offers_later());
    wizard.answer(health.choices.len() - 1, None).await.unwrap();
    assert!(wizard.is_later());
    assert!(wizard.prompt().await.is_none(), "nothing is left to ask");
    assert!(wizard.added().is_none());
    assert!(fx.written_device().is_none());
    assert_eq!(OPEN_AGAIN, "Run svolder again to open the interface.");
}

/// Only the first step can be left for later: a walk that starts at the scan has no such
/// choice, and neither does one that has gone on.
#[tokio::test]
async fn later_is_the_first_steps_alone() {
    let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
    assert!(!wizard.offers_later());
    assert!(
        !wizard
            .prompt()
            .await
            .unwrap()
            .choices
            .contains(&LATER.to_string())
    );
}

/// A note given to the walk is the first line of its first step.
#[tokio::test]
async fn a_note_stands_above_the_first_steps_own_lines() {
    let mut wizard =
        Wizard::new(FakeEffects::healthy()).noting(Some("svolderd re-read ~/config.toml".into()));
    let health = wizard.prompt().await.unwrap();
    assert_eq!(health.body[0], "svolderd re-read ~/config.toml");
}

/// A wizard that gives the walk back is followed by the plain prompts, once, and their code is
/// the walk's.
#[tokio::test]
async fn plain_prompts_take_the_walk_the_wizard_gives_back() {
    let calls = std::cell::Cell::new(0);
    let code = walked_with(
        Wizard::new(FakeEffects::healthy()),
        |wizard| async move { Err(wizard) },
        |_wizard| async {
            calls.set(calls.get() + 1);
            std::process::ExitCode::from(7)
        },
    )
    .await;
    assert_eq!(calls.get(), 1);
    assert_eq!(code, std::process::ExitCode::from(7));
}

/// A wizard that took the walk is the only front end that runs.
#[tokio::test]
async fn plain_prompts_stay_unused_when_the_wizard_took_the_walk() {
    let calls = std::cell::Cell::new(0);
    let code = walked_with(
        Wizard::new(FakeEffects::healthy()),
        |_wizard| async { Ok(std::process::ExitCode::from(3)) },
        |_wizard| async {
            calls.set(calls.get() + 1);
            std::process::ExitCode::SUCCESS
        },
    )
    .await;
    assert_eq!(calls.get(), 0);
    assert_eq!(code, std::process::ExitCode::from(3));
}

/// A host that implements only what the summary reads: no files, no writes.
struct SummaryOnly;

#[async_trait(?Send)]
impl Host for SummaryOnly {
    async fn checks(&self) -> Vec<Outcome> {
        Vec::new()
    }

    async fn apply(&self, _: &Fix) -> Result<String, String> {
        Err("unused".into())
    }

    fn platform(&self) -> Platform {
        Platform {
            container: true,
            ..Platform::default()
        }
    }

    fn nixos_snippet(&self) -> Vec<String> {
        Vec::new()
    }

    fn registry(&self) -> Option<&Registry> {
        None
    }
}

#[async_trait(?Send)]
impl Radio for SummaryOnly {
    async fn scan(&self) -> Result<Scanned, Failure> {
        Ok(Scanned {
            entries: Vec::new(),
            notices: Vec::new(),
        })
    }

    fn adapters(&self) -> Vec<String> {
        Vec::new()
    }

    async fn grade(&self, _: &str) -> Vec<Grade> {
        Vec::new()
    }

    async fn try_connect(&self, _: &ScanEntry, _: &str) -> Result<Connect, Failure> {
        Ok(Connect::Accepted)
    }
}

/// The summary needs a host and a radio and nothing that writes.
#[tokio::test]
async fn the_summary_runs_over_a_host_and_a_radio_alone() {
    let (lines, code) = summary::rendered(&SummaryOnly).await;
    assert_eq!(code, 0);
    assert!(
        lines.iter().any(|l| l.contains("container's own")),
        "{lines:?}"
    );
}

/// The daemon step says the container's daemon with the sentence `install` shares.
#[test]
fn daemon_lines_in_a_container_say_the_shared_container_sentence() {
    let boxed = FakeEffects::healthy().on(Platform {
        container: true,
        ..Default::default()
    });
    let lines = daemon_lines(&boxed);
    assert!(
        lines.iter().any(|l| l.contains(CONTAINER_DAEMON)),
        "{lines:?}"
    );
}
