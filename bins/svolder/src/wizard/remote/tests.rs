// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk as the daemon's client, against a daemon that scripts what the walk says.

use std::time::Duration;

use svolder_walk::{Given, RefusalKind, Step, Takes};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};
use crate::test_support::walk_script::{
    JOB, choice_prompt, done, refusing, text_prompt, waiting, working,
};
use crate::wizard::Wizard;
use crate::wizard::fake::{FakeEffects, seen_product};

/// A case ends in seconds or it is stuck.
const BOUND: Duration = Duration::from_secs(10);

/// A daemon that lists the `walk` feature.
async fn walking_daemon() -> StubServer {
    let daemon = stub_daemon().await;
    daemon.lists_features(&["walk", "device_crud"]);
    daemon
}

fn scan() -> Prompt {
    choice_prompt(Step::Scan, "Scan", &["Scan"])
}

fn pick() -> Prompt {
    choice_prompt(Step::Pick, "Which device", &["Bifrost 3", "Gungnir 2"])
}

fn name() -> Prompt {
    text_prompt(Step::Name, "What to call it", "Bifrost 3", Takes::Text)
}

fn linked() -> Prompt {
    choice_prompt(Step::Linked, "A device is linked", &["Go on", "Cancel"])
}

/// A walk is started, followed and answered to its end, and a walk that has ended is not
/// cancelled on the way out.
#[tokio::test]
async fn a_walk_is_started_followed_answered_and_left_alone_once_over() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(scan(), false)]);
        script.then(vec![working(scan()), waiting(pick(), true)]);
        script.then(vec![done("desk", "0102030405060708")]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());

        assert_eq!(walk.prompt().await.unwrap().step, Step::Scan);
        assert_eq!(walk.answer(0, None).await.unwrap(), Answered::On);
        assert_eq!(walk.prompt().await.unwrap().step, Step::Pick);
        let Answered::Added(device) = walk.answer(0, None).await.unwrap() else {
            panic!("the walk wrote a device");
        };
        assert_eq!(
            (device.name.as_str(), device.udid.as_str()),
            ("desk", "0102030405060708")
        );
        walk.leave().await;

        assert_eq!(script.follows(), 1);
        assert!(
            script.cancelled().is_empty(),
            "a finished walk was cancelled"
        );
        assert_eq!(
            script.ops(),
            [
                "Start",
                "Answer { job: 7, choice: 0, text: None }",
                "Answer { job: 7, choice: 0, text: None }"
            ]
        );
    })
    .await
    .expect("the case ends");
}

/// Only the walk this client started is read or cancelled: a job another front end runs, in the
/// follow's replay, is neither adopted nor stopped by leaving.
#[tokio::test]
async fn another_clients_walk_is_never_adopted_or_cancelled() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.foreign(
            3,
            waiting(choice_prompt(Step::Scan, "theirs", &["Scan"]), false),
        );
        script.starts_with(vec![waiting(scan(), false)]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());

        let prompt = walk.prompt().await.unwrap();
        assert_eq!(prompt.title, "Scan", "the other walk's prompt was taken");
        walk.leave().await;

        assert_eq!(
            script.cancelled(),
            [JOB],
            "only the walk this client started is cancelled"
        );
    })
    .await
    .expect("the case ends");
}

/// A start refused as busy shows the daemon's words, which say where the running walk was
/// started, and leaving cancels nothing: there is no walk of this client's.
#[tokio::test]
async fn a_busy_start_shows_the_daemons_words_and_cancels_nothing() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.busy_with("the web page");
        let mut walk = RemoteWalk::new(daemon.socket.clone());

        let failure = walk.prompt().await.unwrap_err();
        assert!(
            failure.what.contains("started by the web page"),
            "{failure}"
        );
        walk.leave().await;
        assert!(script.cancelled().is_empty());
    })
    .await
    .expect("the case ends");
}

/// A refusal comes back once, with the daemon's words, and the same prompt is asked again.
#[tokio::test]
async fn a_refusal_comes_back_once_and_the_prompt_is_asked_again() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(name(), true)]);
        script.then(vec![
            working(name()),
            refusing(name(), RefusalKind::Taken, "another device is called that"),
        ]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        let asked = walk.prompt().await.unwrap();
        let Answered::Refused(refusal) = walk.answer(0, Some("desk".into())).await.unwrap() else {
            panic!("the name was refused");
        };
        assert_eq!(refusal.message, "another device is called that");
        assert_eq!(walk.prompt().await.unwrap(), asked);
        walk.leave().await;
    })
    .await
    .expect("the case ends");
}

/// The state an answer was given against, sent again, does not settle the answer: the walk is
/// read on to the state that shows it was taken.
#[tokio::test]
async fn a_state_sent_again_does_not_settle_an_answer() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(scan(), false)]);
        script.then(vec![
            waiting(scan(), false),
            working(scan()),
            waiting(pick(), true),
        ]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        walk.prompt().await.unwrap();
        walk.answer(0, None).await.unwrap();
        assert_eq!(walk.prompt().await.unwrap().step, Step::Pick);
        walk.leave().await;
    })
    .await
    .expect("the case ends");
}

/// Going back asks the daemon and shows the step it went back to.
#[tokio::test]
async fn going_back_shows_the_step_before() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(pick(), true)]);
        script.then(vec![waiting(scan(), false)]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        walk.prompt().await.unwrap();
        assert!(walk.can_go_back());
        walk.back().await.unwrap();
        assert_eq!(walk.prompt().await.unwrap().step, Step::Scan);
        assert!(!walk.can_go_back());
        walk.leave().await;
    })
    .await
    .expect("the case ends");
}

/// A walk dropped while it runs is cancelled all the same, and nothing of an answer's text
/// reaches the ops as `{:?}` prints them.
#[tokio::test]
async fn a_dropped_walk_is_cancelled_and_a_passphrase_is_never_printed() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        let code = choice_prompt(Step::Passphrase, "The code", &[]);
        script.starts_with(vec![waiting(code.clone(), true)]);
        script.then(vec![working(code.clone()), waiting(code, true)]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        walk.prompt().await.unwrap();
        walk.answer(0, Some("4321".into())).await.unwrap();
        drop(walk);
        script.cancel_arrived().await;

        assert_eq!(script.cancelled(), [JOB]);
        assert_eq!(script.answers(), [(0, Some("4321".to_string()))]);
        assert!(
            script.ops().iter().all(|op| !op.contains("4321")),
            "{:?}",
            script.ops()
        );
    })
    .await
    .expect("the case ends");
}

/// A walk cancelled by someone else, the idle bound or the daemon's stop is told as such, not
/// as a device written; one cancelled at the linked step by this client's own answer is a leave.
#[tokio::test]
async fn a_cancel_is_a_leave_only_when_this_client_chose_it() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(linked(), false)]);
        script.then(vec![JobState::Cancelled]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        walk.prompt().await.unwrap();
        assert_eq!(walk.answer(1, None).await.unwrap(), Answered::Cancelled);

        let other = walking_daemon().await;
        let script = other.walk();
        script.starts_with(vec![waiting(scan(), false)]);
        script.then(vec![JobState::Cancelled]);
        let mut walk = RemoteWalk::new(other.socket.clone());
        walk.prompt().await.unwrap();
        let failure = walk.answer(0, None).await.unwrap_err();
        assert!(failure.what.contains("cancelled"), "{failure}");
        assert!(walk.is_over());
    })
    .await
    .expect("the case ends");
}

/// A wizard asks the daemon to walk when one runs, takes no radio of its own, skips the adapter
/// step however many adapters there are, and tells the daemon nothing: it wrote the device.
#[tokio::test]
async fn a_wizard_over_a_daemon_walks_there_and_skips_the_adapter_step() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(scan(), false)]);
        script.then(vec![waiting(name(), true)]);
        script.then(vec![done("desk", "0102030405060708")]);
        let fx = FakeEffects::healthy()
            .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
            .with_adapters([("hci0", 2), ("hci1", 5)]);
        let mut wizard = Wizard::starting_at(fx.clone(), Step::Scan, Given::default());
        wizard.attach(&daemon.socket).await.unwrap();
        assert!(wizard.is_remote());

        assert_eq!(wizard.prompt().await.unwrap().step, Step::Scan);
        wizard.answer(0, None).await.unwrap();
        assert_eq!(wizard.prompt().await.unwrap().step, Step::Name);
        wizard.answer(0, Some("desk".into())).await.unwrap();
        assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
        assert_eq!(
            wizard.added().map(|device| device.name.as_str()),
            Some("desk")
        );
        wizard.leave().await;

        assert_eq!(
            fx.graded(),
            0,
            "the adapters were graded with a daemon running"
        );
        assert_eq!(fx.scans(), 0, "the radio was used with a daemon running");
        assert!(
            fx.written_device().is_none(),
            "the file was written behind the daemon"
        );
        assert_eq!(fx.reloads(), 0);
        assert!(script.cancelled().is_empty());
    })
    .await
    .expect("the case ends");
}

/// With no daemon the wizard stays on its own walk; a daemon that cannot walk is refused in
/// words, and never written around.
#[tokio::test]
async fn no_daemon_is_the_local_walk_and_an_old_daemon_is_refused() {
    tokio::time::timeout(BOUND, async {
        let none = crate::test_support::temp_dir("remote-none");
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
        wizard.attach(&none.join("no.sock")).await.unwrap();
        assert!(!wizard.is_remote());

        let old = stub_daemon().await;
        old.lists_features(&["device_crud"]);
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
        let failure = wizard.attach(&old.socket).await.unwrap_err();
        assert!(failure.next.contains("svolder daemon restart"), "{failure}");
        assert_eq!(
            failure.code, 7,
            "a daemon too old is not a lost link: {failure}"
        );
        assert!(
            old.walk().ops().is_empty(),
            "an old daemon was asked to walk"
        );
    })
    .await
    .expect("the case ends");
}

/// A name the command line gave is what an empty line answers at the daemon's name step, and a
/// code typed at one step is not carried to another's after going back.
#[tokio::test]
async fn the_given_name_stands_in_for_an_empty_line() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(name(), true)]);
        script.then(vec![done("studio", "0102030405060708")]);
        let given = Given {
            name: Some("studio".into()),
            ..Given::default()
        };
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given);
        wizard.attach(&daemon.socket).await.unwrap();
        let prompt = wizard.prompt().await.unwrap();
        assert_eq!(prompt.body.last().map(String::as_str), Some("studio"));
        wizard.answer(0, Some(String::new())).await.unwrap();
        assert_eq!(script.answers(), [(0, Some("studio".to_string()))]);
        wizard.leave().await;
    })
    .await
    .expect("the case ends");
}

/// A walk the daemon ends while the client sits at a prompt (the idle bound, a stop) is read
/// from the stream before an answer goes out, and the answer says so, rather than being refused
/// as not waiting for ever.
#[tokio::test]
async fn a_walk_the_daemon_ended_at_a_prompt_is_seen_not_asked_again() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(name(), true)]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        walk.prompt().await.unwrap();
        // The idle bound ends it while the person is away.
        script.foreign(JOB, JobState::Cancelled);
        let failure = walk.answer(0, Some("desk".into())).await.unwrap_err();
        assert!(failure.what.contains("cancelled"), "{failure}");
        assert!(walk.is_over());
    })
    .await
    .expect("the case ends");
}

/// A daemon that stops while the client sits at a prompt ends the stream: the next answer or
/// step back says the daemon went away.
#[tokio::test]
async fn a_daemon_that_went_away_at_a_prompt_is_seen() {
    tokio::time::timeout(BOUND, async {
        for back in [false, true] {
            let daemon = walking_daemon().await;
            let script = daemon.walk();
            script.starts_with(vec![waiting(name(), true)]);
            let mut walk = RemoteWalk::new(daemon.socket.clone());
            walk.prompt().await.unwrap();
            script.hang_up();
            let failure = match back {
                true => walk.back().await.unwrap_err(),
                false => walk.answer(0, None).await.unwrap_err(),
            };
            assert!(failure.what.contains("went away"), "{failure}");
            assert!(walk.is_over());
        }
    })
    .await
    .expect("the case ends");
}

/// A state this build cannot read ends the walk in words that say the daemon is newer.
#[tokio::test]
async fn a_state_that_does_not_parse_is_said() {
    tokio::time::timeout(BOUND, async {
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![JobState::Progress {
            payload: serde_json::json!({"phase": "dancing"}),
        }]);
        let mut walk = RemoteWalk::new(daemon.socket.clone());
        let failure = walk.prompt().await.unwrap_err();
        assert!(failure.what.contains("newer than svolder"), "{failure}");
    })
    .await
    .expect("the case ends");
}

/// With a daemon, `--passphrase` answers the passphrase step once; the default `auto` is not a
/// given answer, so that step is shown to the person.
#[tokio::test]
async fn a_given_code_answers_the_passphrase_step_and_auto_is_not_given() {
    tokio::time::timeout(BOUND, async {
        let code = text_prompt(Step::Passphrase, "The code", "none", Takes::Secret);
        for given in [Some("1234"), Some("auto")] {
            let daemon = walking_daemon().await;
            let script = daemon.walk();
            script.starts_with(vec![waiting(code.clone(), true)]);
            script.then(vec![done("desk", "0102030405060708")]);
            let given = Given {
                passphrase: given.map(str::to_string),
                ..Given::default()
            };
            let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given.clone());
            wizard.attach(&daemon.socket).await.unwrap();
            let mut prompt = wizard.prompt().await.unwrap();
            if wizard.has_given_answer(&prompt) {
                wizard.answer_given(&prompt).await.unwrap();
                prompt = wizard.prompt().await.unwrap();
            }
            match given.passphrase.as_deref() {
                Some("1234") => {
                    assert_eq!(prompt.step, Step::Daemon, "the code was not sent");
                    assert_eq!(script.answers(), [(0, Some("1234".to_string()))]);
                }
                _ => {
                    assert_eq!(prompt.step, Step::Passphrase, "auto answered the step");
                    assert!(script.answers().is_empty());
                }
            }
            wizard.leave().await;
        }
    })
    .await
    .expect("the case ends");
}

/// `--profile` answers the profile question by name, and for a product that has no question the
/// walk's end is followed by an `update` of the profile, so the flag is never dropped.
#[tokio::test]
async fn a_given_profile_is_answered_or_written_after_the_walk() {
    use svolder_ipc::DeviceConfigOp;
    tokio::time::timeout(BOUND, async {
        let given = Given {
            profile: Some("Bifrost3".into()),
            ..Given::default()
        };
        // The question is asked: the choice named answers it, and nothing is written after.
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        let question = choice_prompt(Step::Pick, "Which profile", &["amp", "bifrost3"]);
        script.starts_with(vec![waiting(question, true)]);
        script.then(vec![done("desk", "0102030405060708")]);
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given.clone());
        wizard.attach(&daemon.socket).await.unwrap();
        let asked = wizard.prompt().await.unwrap();
        wizard.answer_given(&asked).await.unwrap();
        assert_eq!(wizard.prompt().await.unwrap().step, Step::Daemon);
        assert_eq!(script.answers(), [(1, None)]);
        assert!(daemon.device_ops().is_empty(), "{:?}", daemon.device_ops());
        wizard.leave().await;

        // No question (the product is known): the profile is written once the device is.
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(name(), true)]);
        script.then(vec![done("desk", "0102030405060708")]);
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given);
        wizard.attach(&daemon.socket).await.unwrap();
        wizard.prompt().await.unwrap();
        wizard.answer(0, Some("desk".into())).await.unwrap();
        assert_eq!(
            daemon.device_ops(),
            [DeviceConfigOp::Update {
                device: "desk".into(),
                name: None,
                profile: Some(Some("Bifrost3".into())),
                passphrase: None,
            }]
        );
        wizard.leave().await;
    })
    .await
    .expect("the case ends");
}

/// A given code the device turns away is said, and the step is then asked of the person.
#[tokio::test]
async fn a_given_code_that_is_turned_away_is_said() {
    tokio::time::timeout(BOUND, async {
        let code = text_prompt(Step::Passphrase, "The code", "none", Takes::Secret);
        let daemon = walking_daemon().await;
        let script = daemon.walk();
        script.starts_with(vec![waiting(code.clone(), true)]);
        script.then(vec![
            working(code.clone()),
            refusing(
                code,
                RefusalKind::Passphrase,
                "the device turned the code away",
            ),
        ]);
        let given = Given {
            passphrase: Some("1234".into()),
            ..Given::default()
        };
        let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given);
        wizard.attach(&daemon.socket).await.unwrap();
        let prompt = wizard.prompt().await.unwrap();
        let failure = wizard.answer_given(&prompt).await.unwrap_err();
        assert!(
            failure.what.contains("passphrase given was not taken"),
            "{failure}"
        );
        let again = wizard.prompt().await.unwrap();
        assert!(
            !wizard.has_given_answer(&again),
            "the given code was sent twice"
        );
        wizard.leave().await;
    })
    .await
    .expect("the case ends");
}

/// A busy refusal tells what a person can do when the walk's holder is gone: it ends by itself
/// within ten minutes of its last answer, or at once with a daemon restart.
#[test]
fn a_busy_start_names_the_two_ways_the_walk_ends() {
    let failure = started_badly(
        std::path::Path::new("/nonexistent/svolder.sock"),
        Ok(Response::Error {
            code: svolder_ipc::CODE_USAGE,
            message: "a walk is already running".to_string(),
            reason: Some(Reason::Busy),
        }),
    );
    assert!(
        failure.next.contains("within 10 minutes"),
        "{}",
        failure.next
    );
    assert!(
        failure.next.contains("svolder daemon restart"),
        "{}",
        failure.next
    );
}
