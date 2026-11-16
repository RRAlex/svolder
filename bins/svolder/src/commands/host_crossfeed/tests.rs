// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Each verb's request and printed answer against the stub daemon, with a view of the test's own
//! words: the CLI takes its words and ranges from the view, so none of these names is the real
//! table's.

use svolder_host::eq::{CrossfeedView, EqView};
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, HostEqOp};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};

/// A view with two models of invented names, the first the current one, off.
fn view() -> CrossfeedView {
    use svolder_host::eq::crossfeed::{ModelView, PresetView, Range};
    let model = |word: &str, name: &str, presets: &[&str]| ModelView {
        word: word.to_string(),
        name: name.to_string(),
        presets: presets
            .iter()
            .map(|p| PresetView {
                name: (*p).to_string(),
                ..PresetView::default()
            })
            .collect(),
        crossover_hz: Range {
            min: 300.0,
            max: 900.0,
            step: 10.0,
        },
        amount_pct: Range {
            min: 10.0,
            max: 50.0,
            step: 1.0,
        },
        delay_max_ms: 1.0,
        delay_step_ms: 0.02,
        delay_settable: true,
        ..ModelView::default()
    };
    CrossfeedView {
        on: false,
        model: "alpha".to_string(),
        preset: Some("Fancy Name".to_string()),
        amount_pct: 37.4,
        feed_db: 4.5,
        amount_db: -8.6,
        crossover_hz: 700.0,
        delay_ms: 0.26,
        delay_floor_ms: 0.26,
        delay_settable: true,
        delay_own: true,
        output: Some("Test DAC".to_string()),
        models: vec![
            model("alpha", "Alpha", &["Fancy Name", "Plain"]),
            model("beta", "Beta", &["Other"]),
        ],
        ..CrossfeedView::default()
    }
}

/// A daemon that serves the computer EQ with `crossfeed` in it.
async fn serving(crossfeed: Option<CrossfeedView>) -> StubServer {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: true,
        eq: Some(EqView {
            crossfeed,
            ..EqView::default()
        }),
        ..HostView::default()
    });
    daemon
}

fn globals(socket: &str) -> Globals<'_> {
    Globals {
        transport: "daemon",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    }
}

/// Run `verb` against `daemon`.
async fn verb(
    daemon: &StubServer,
    json: bool,
    command: Option<HostCrossfeedCommand>,
) -> Result<Answer, Failure> {
    let socket = daemon.socket.display().to_string();
    answer(
        &globals(&socket),
        Ok(&Config::default()),
        None,
        false,
        json,
        command,
    )
    .await
}

/// The one request a verb sent.
fn sent(daemon: &StubServer) -> HostEqOp {
    let mut ops = daemon.host_eq_ops();
    assert_eq!(ops.len(), 1, "{ops:?}");
    ops.remove(0)
}

fn op(
    on: Option<bool>,
    model: Option<&str>,
    preset: Option<&str>,
    amount_pct: Option<f64>,
    crossover_hz: Option<f64>,
    delay_ms: Option<f64>,
) -> HostEqOp {
    HostEqOp::Crossfeed {
        on,
        model: model.map(str::to_string),
        preset: preset.map(str::to_string),
        amount_pct,
        crossover_hz,
        delay_ms,
        done: true,
    }
}

/// The show lines carry the state, the model's name, the preset, the level with its dB,
/// the crossover, the delay with its floor, the output and the note.
#[tokio::test]
async fn show_prints_the_state_in_the_views_words() {
    let mut shown = view();
    shown.on = true;
    shown.note = Some("crossfeed turned off: the output changed".to_string());
    let daemon = serving(Some(shown)).await;
    let said = verb(&daemon, false, None).await.expect("shown");
    assert_eq!(
        said.out,
        [
            "crossfeed: on",
            "model: Alpha",
            "preset: Fancy Name",
            "level: 37 % \u{b7} \u{2212}8.6 dB",
            "crossover: 700 Hz",
            "delay: 0.26 ms (own, floor 0.26 ms)",
            "output: Test DAC",
            "note: crossfeed turned off: the output changed",
        ]
    );
    assert!(said.err.is_empty());
    assert!(daemon.host_eq_ops().is_empty(), "show changes nothing");
}

/// Custom values say what they were edited from, a raised delay says its floor, and a model with
/// no delay to set says so.
#[tokio::test]
async fn show_words_custom_values_a_raised_delay_and_a_fixed_delay() {
    let mut custom = view();
    custom.preset = None;
    custom.edited_from = Some("Plain".to_string());
    custom.delay_own = false;
    custom.delay_ms = 0.5;
    let text = lines(&custom);
    assert!(
        text.contains(&"preset: Custom (edited from Plain)".to_string()),
        "{text:?}"
    );
    assert!(
        text.contains(&"delay: 0.50 ms (floor 0.26 ms)".to_string()),
        "{text:?}"
    );

    let mut fixed = view();
    fixed.delay_settable = false;
    fixed.delay_floor_ms = 0.0;
    fixed.delay_ms = 0.0;
    let text = lines(&fixed);
    assert!(
        text.contains(&"delay: the model's own (not settable)".to_string()),
        "{text:?}"
    );
}

/// `--json` prints the view as the daemon sent it.
#[tokio::test]
async fn json_is_the_view_as_the_daemon_sends_it() {
    let daemon = serving(Some(view())).await;
    let said = verb(&daemon, true, None).await.expect("shown");
    assert_eq!(said.out.len(), 1);
    let back: CrossfeedView = serde_json::from_str(&said.out[0]).expect("one JSON object");
    assert_eq!(back, view());
}

/// Each verb is one `Crossfeed` request with its own field, and the answer says what the daemon
/// holds afterwards.
#[tokio::test]
async fn each_verb_sends_its_one_request() {
    use HostCrossfeedCommand as C;
    let cases: Vec<(C, HostEqOp)> = vec![
        (C::On, op(Some(true), None, None, None, None, None)),
        (C::Off, op(Some(false), None, None, None, None, None)),
        (
            C::Model {
                word: "BETA".into(),
            },
            op(None, Some("beta"), None, None, None, None),
        ),
        (
            C::Preset {
                name: vec!["plain".into()],
            },
            op(None, None, Some("Plain"), None, None, None),
        ),
        (
            C::Preset {
                name: vec!["fancy".into(), "NAME".into()],
            },
            op(None, None, Some("Fancy Name"), None, None, None),
        ),
        (
            C::Level {
                percent: "30".into(),
            },
            op(None, None, None, Some(30.0), None, None),
        ),
        (
            C::Crossover { hz: "650".into() },
            op(None, None, None, None, Some(650.0), None),
        ),
        (
            C::Delay {
                value: "0.5".into(),
            },
            op(None, None, None, None, None, Some(0.5)),
        ),
        (
            C::Delay {
                value: "own".into(),
            },
            op(None, None, None, None, None, Some(0.0)),
        ),
    ];
    for (command, expected) in cases {
        let daemon = serving(Some(view())).await;
        let said = verb(&daemon, false, Some(command)).await.expect("taken");
        assert_eq!(sent(&daemon), expected);
        assert_eq!(said.err.len(), 1, "{said:?}");
        assert!(said.out.is_empty());
    }
}

/// What the answer says is what the daemon holds after the change, so a value it held to its range
/// is printed as held.
#[tokio::test]
async fn a_held_value_is_printed_as_set() {
    let mut after = view();
    after.amount_pct = 50.0;
    after.feed_db = 6.0;
    after.amount_db = -6.0;
    after.preset = None;
    // The stub keeps one view: it stands for what the daemon holds once the change is done.
    let daemon = serving(Some(after)).await;
    let said = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Level {
            percent: "80".into(),
        }),
    )
    .await
    .expect("taken");
    assert_eq!(sent(&daemon), op(None, None, None, Some(80.0), None, None));
    assert_eq!(said.err, ["crossfeed level: 50 % \u{b7} \u{2212}6.0 dB"]);

    let mut after = view();
    after.crossover_hz = 900.0;
    let daemon = serving(Some(after)).await;
    let said = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Crossover { hz: "5000".into() }),
    )
    .await
    .expect("taken");
    assert_eq!(said.err, ["crossfeed crossover: 900 Hz"]);

    let mut after = view();
    after.on = true;
    after.model = "beta".into();
    let daemon = serving(Some(after)).await;
    let said = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Model {
            word: "beta".into(),
        }),
    )
    .await
    .expect("taken");
    assert_eq!(said.err, ["crossfeed model: Beta"]);
}

/// A word or a name the view does not list is refused before anything is sent, and the refusal
/// lists the words the view has.
#[tokio::test]
async fn an_unknown_model_or_preset_is_refused_with_the_views_words() {
    let daemon = serving(Some(view())).await;
    let failure = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Model {
            word: "gamma".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(failure.to_string().contains("alpha, beta"), "{failure}");
    let failure = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Preset {
            name: vec!["nope".into()],
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(
        failure.to_string().contains("Fancy Name, Plain"),
        "{failure}"
    );
    for bad in ["x", "NaN", "inf", "-1"] {
        let failure = verb(
            &daemon,
            false,
            Some(HostCrossfeedCommand::Delay { value: bad.into() }),
        )
        .await
        .unwrap_err();
        assert_eq!(failure.code, 1, "{bad}: {failure}");
    }
    for bad in ["x", "NaN", "inf", "-5", "%", ""] {
        let failure = verb(
            &daemon,
            false,
            Some(HostCrossfeedCommand::Level {
                percent: bad.into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(failure.code, 1, "{bad}");
        assert!(
            failure.what.contains("is not a"),
            "svolder's words, not clap's: {failure}"
        );
    }
    assert!(daemon.host_eq_ops().is_empty());
}

/// The daemon's own refusal comes back in its words with its code.
#[tokio::test]
async fn the_daemons_refusal_comes_back_in_its_words() {
    let daemon = serving(Some(view())).await;
    daemon.refuses_eq("computer controls unavailable: no pw-cli on this computer");
    let failure = verb(&daemon, false, Some(HostCrossfeedCommand::On))
        .await
        .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(failure.what.contains("no pw-cli"), "{failure}");
}

/// With no daemon serving the computer EQ, every verb fails with words that say what starts the
/// daemon, and the code of no daemon; a daemon that serves the EQ but knows no crossfeed is too
/// old.
#[tokio::test]
async fn no_daemon_refuses_every_verb_and_an_old_daemon_says_so() {
    use HostCrossfeedCommand as C;
    let direct = Globals {
        transport: "fake",
        ..globals("/nonexistent")
    };
    for command in [
        C::On,
        C::Off,
        C::Model {
            word: "alpha".into(),
        },
        C::Preset {
            name: vec!["x".into()],
        },
        C::Level {
            percent: "1".into(),
        },
        C::Crossover { hz: "1".into() },
        C::Delay {
            value: "own".into(),
        },
    ] {
        let failure = answer(
            &direct,
            Ok(&Config::default()),
            None,
            false,
            false,
            Some(command),
        )
        .await
        .unwrap_err();
        assert_eq!(failure.code, 3);
        assert_eq!(
            failure.to_string(),
            "crossfeed needs svolderd, and none serves the computer EQ. \
             Start it with `svolder daemon install`, then try again"
        );
    }

    let old = serving(None).await;
    let failure = verb(&old, false, Some(HostCrossfeedCommand::On))
        .await
        .unwrap_err();
    assert_eq!(failure.code, 7);
    assert!(
        failure.to_string().contains("svolder daemon restart"),
        "{failure}"
    );
    let failure = verb(&old, false, None).await.unwrap_err();
    assert_eq!(failure.code, 7);
}

/// With no daemon, the state file's settings are shown as off, with the reason; the view is the
/// same one `--json` prints.
#[test]
fn the_state_files_settings_are_shown_off_and_say_why() {
    use svolder_host::eq::CrossfeedSettings;
    let settings = CrossfeedSettings {
        amount: 0.3,
        crossover_hz: 650.0,
        ..CrossfeedSettings::default()
    };
    let shown = offline(Some(&settings), false, NEEDS_A_DAEMON);
    assert!(!shown.on);
    assert!(
        (shown.amount_pct - 30.0).abs() < 1e-9,
        "{}",
        shown.amount_pct
    );
    assert_eq!(shown.unavailable.as_deref(), Some(NEEDS_A_DAEMON));
    let text = lines(&shown);
    assert_eq!(text[0], "crossfeed: off: crossfeed needs svolderd");
    assert!(text.contains(&"crossover: 650 Hz".to_string()), "{text:?}");
    assert!(!offline(None, false, NEEDS_A_DAEMON).on);
    assert!(offline(None, true, NEEDS_A_DAEMON).on);
}

/// A unit after the number is accepted, in any case, and means the same as the bare number.
#[tokio::test]
async fn a_unit_after_the_number_is_accepted() {
    use HostCrossfeedCommand as C;
    for (command, expected) in [
        (
            C::Level {
                percent: "37%".into(),
            },
            op(None, None, None, Some(37.0), None, None),
        ),
        (
            C::Level {
                percent: " 37 % ".into(),
            },
            op(None, None, None, Some(37.0), None, None),
        ),
        (
            C::Crossover { hz: "700Hz".into() },
            op(None, None, None, None, Some(700.0), None),
        ),
        (
            C::Crossover {
                hz: "700 hz".into(),
            },
            op(None, None, None, None, Some(700.0), None),
        ),
        (
            C::Delay {
                value: "0.5ms".into(),
            },
            op(None, None, None, None, None, Some(0.5)),
        ),
    ] {
        let daemon = serving(Some(view())).await;
        verb(&daemon, false, Some(command)).await.expect("taken");
        assert_eq!(sent(&daemon), expected);
    }
}

/// An unreadable configuration is the failure of a verb (exit 1), as for `host eq apply`; `show`
/// still reads the saved settings.
#[tokio::test]
async fn an_unreadable_config_fails_a_verb_but_not_show() {
    let daemon = serving(Some(view())).await;
    let socket = daemon.socket.display().to_string();
    let failure = answer(
        &globals(&socket),
        Err("config.toml: expected `=`"),
        None,
        false,
        false,
        Some(HostCrossfeedCommand::On),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(failure.what.contains("config.toml"), "{failure}");
    assert!(daemon.host_eq_ops().is_empty());
    let said = answer(&globals(&socket), Err("bad"), None, false, false, None)
        .await
        .expect("shown");
    assert_eq!(
        said.out[0],
        "crossfeed: unknown: no daemon is running, so whether it plays now is unknown \
         (the saved state says off)"
    );
}

/// With no daemon, `show` never claims the crossfeed is off: a TUI with no daemon may be playing
/// it. The saved state's switch is what it reads, and `--json` carries that and the reason.
#[tokio::test]
async fn no_daemon_says_the_live_state_is_unknown_and_what_the_state_file_holds() {
    let direct = globals("/nonexistent/svolder.sock");
    let said = answer(&direct, Ok(&Config::default()), None, true, true, None)
        .await
        .expect("shown");
    let back: CrossfeedView = serde_json::from_str(&said.out[0]).expect("one object");
    assert!(back.on, "the state file says on");
    assert!(
        back.unavailable
            .as_deref()
            .is_some_and(|why| why.contains("unknown")),
        "{back:?}"
    );
    let said = answer(&direct, Ok(&Config::default()), None, true, false, None)
        .await
        .expect("shown");
    assert!(
        said.out[0].starts_with("crossfeed: unknown: no daemon is running"),
        "{said:?}"
    );
    assert!(
        said.out[0].ends_with("(the saved state says on)"),
        "{said:?}"
    );
}

/// A daemon that answers but shows no computer EQ (its host is off) is not "no daemon": a verb
/// says so with the code of an unavailable computer, and `show` reads the saved settings.
#[tokio::test]
async fn a_daemon_with_its_host_off_gets_its_own_word_and_exit_6() {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: false,
        eq: None,
        ..HostView::default()
    });
    let failure = verb(&daemon, false, Some(HostCrossfeedCommand::On))
        .await
        .unwrap_err();
    assert_eq!(failure.code, 6);
    assert!(failure.what.contains("no computer EQ"), "{failure}");
    assert!(!failure.to_string().contains("daemon install"), "{failure}");
    let said = verb(&daemon, false, None).await.expect("shown");
    assert_eq!(
        said.out[0],
        "crossfeed: off: the daemon's host audio is off"
    );
}

/// A daemon that takes the connection and never answers is "did not answer", which is not the
/// "no computer EQ" of one that answers with none.
#[tokio::test]
async fn a_daemon_that_never_answers_is_told_apart_from_one_with_no_computer_eq() {
    let dir = crate::test_support::TempDir(
        std::path::PathBuf::from("/tmp/svolder").join(format!("xf-silent-{}", std::process::id())),
    );
    std::fs::create_dir_all(&*dir).unwrap();
    let socket = dir.join("s.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let _held = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    assert!(host_eq(&socket).await.is_none(), "no answer in time");

    let answering = stub_daemon().await;
    answering.hosts(HostView {
        available: false,
        eq: None,
        ..HostView::default()
    });
    assert_eq!(host_eq(&answering.socket).await, Some(None));
}

/// A model or preset the view does not list is refused in the daemon's own words.
#[tokio::test]
async fn a_word_the_view_does_not_list_is_refused_in_the_daemons_words() {
    let daemon = serving(Some(view())).await;
    let failure = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Model {
            word: "nope".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.what, "no crossfeed model \"nope\"");
    assert_eq!(failure.next, "The models are alpha, beta");
    let failure = verb(
        &daemon,
        false,
        Some(HostCrossfeedCommand::Preset {
            name: vec!["Nope".into()],
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.what, "Alpha has no preset \"Nope\"");
    assert_eq!(failure.next, "The presets are Fancy Name, Plain");
}
