// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Each verb's request and printed answer against the stub daemon, with a view of the test's own
//! numbers: the CLI prints what the view says and works none out.

use svolder_host::eq::{EqView, LoudnessView};
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, HostEqOp};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};

/// A view at volume 35 %, strength 35, reference 100: the plan's worked example.
fn view() -> LoudnessView {
    LoudnessView {
        on: true,
        strength_pct: 35.0,
        reference_pct: 100.0,
        volume_pct: Some(35.0),
        depth_db: 13.7,
        lift_db: 7.3,
        mids_db: -7.3,
        treble_db: 2.1,
        following: true,
        ..LoudnessView::default()
    }
}

async fn serving(loudness: Option<LoudnessView>) -> StubServer {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: true,
        eq: Some(EqView {
            loudness,
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

async fn verb(
    daemon: &StubServer,
    json: bool,
    command: Option<HostLoudnessCommand>,
) -> Result<Answer, Failure> {
    let socket = daemon.socket.display().to_string();
    answer(
        &globals(&socket),
        Ok(&Config::default()),
        None,
        json,
        command,
    )
    .await
}

fn sent(daemon: &StubServer) -> HostEqOp {
    let mut ops = daemon.host_eq_ops();
    assert_eq!(ops.len(), 1, "{ops:?}");
    ops.remove(0)
}

fn op(
    on: Option<bool>,
    strength_pct: Option<f64>,
    reference_pct: Option<f64>,
    reference_now: Option<bool>,
) -> HostEqOp {
    HostEqOp::Loudness {
        on,
        strength_pct,
        reference_pct,
        reference_now,
        done: true,
    }
}

/// `show` prints the state, the settings and the view's readout, and changes nothing.
#[tokio::test]
async fn show_prints_the_readout_from_the_view() {
    let mut shown = view();
    shown.note = Some("muted: the loudness holds".to_string());
    let daemon = serving(Some(shown)).await;
    for command in [None, Some(HostLoudnessCommand::Show)] {
        let said = verb(&daemon, false, command).await.expect("shown");
        assert_eq!(
            said.out,
            [
                "loudness: on",
                "strength: 35 %",
                "reference: 100 %",
                "volume: 35.0 %",
                "depth: 13.7 dB",
                "bass: +7.3 dB",
                "mids: \u{2212}7.3 dB",
                "treble: +2.1 dB",
                "note: muted: the loudness holds",
            ]
        );
        assert!(said.err.is_empty());
    }
    assert!(daemon.host_eq_ops().is_empty(), "show changes nothing");
}

/// At the reference the readout is all zeros, with no minus sign; with no volume read there is no
/// readout.
#[test]
fn a_flat_readout_has_no_sign_and_an_unread_volume_has_none() {
    let flat = LoudnessView {
        depth_db: 0.0,
        lift_db: 0.0,
        mids_db: 0.0,
        treble_db: 0.0,
        ..view()
    };
    let text = lines(&flat);
    assert!(text.contains(&"mids: 0.0 dB".to_string()), "{text:?}");
    assert!(text.contains(&"bass: 0.0 dB".to_string()), "{text:?}");
    let unread = LoudnessView {
        volume_pct: None,
        ..view()
    };
    assert_eq!(lines(&unread).len(), 3, "{:?}", lines(&unread));
}

#[tokio::test]
async fn json_is_the_view_as_the_daemon_sends_it() {
    let daemon = serving(Some(view())).await;
    let said = verb(&daemon, true, None).await.expect("shown");
    assert_eq!(said.out.len(), 1);
    let back: LoudnessView = serde_json::from_str(&said.out[0]).expect("one JSON object");
    assert_eq!(back, view());
}

/// Each verb is one `Loudness` request with its own field.
#[tokio::test]
async fn each_verb_sends_its_one_request() {
    use HostLoudnessCommand as C;
    let cases: Vec<(C, HostEqOp)> = vec![
        (C::On, op(Some(true), None, None, None)),
        (C::Off, op(Some(false), None, None, None)),
        (
            C::Strength {
                percent: "40".into(),
            },
            op(None, Some(40.0), None, None),
        ),
        (
            C::Strength {
                percent: " 40 % ".into(),
            },
            op(None, Some(40.0), None, None),
        ),
        (
            C::Reference { value: "80".into() },
            op(None, None, Some(80.0), None),
        ),
        (
            C::Reference {
                value: "NOW".into(),
            },
            op(None, None, None, Some(true)),
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

/// `strength 37` is sent as 37 and printed as the 35 the daemon holds.
#[tokio::test]
async fn a_snapped_value_is_printed_as_held() {
    let daemon = serving(Some(view())).await;
    let said = verb(
        &daemon,
        false,
        Some(HostLoudnessCommand::Strength {
            percent: "37".into(),
        }),
    )
    .await
    .expect("taken");
    assert_eq!(sent(&daemon), op(None, Some(37.0), None, None));
    assert_eq!(said.err, ["loudness strength: 35 %"]);

    let mut after = view();
    after.reference_pct = 42.0;
    after.note = Some("reference is the current volume".to_string());
    let daemon = serving(Some(after)).await;
    let said = verb(
        &daemon,
        false,
        Some(HostLoudnessCommand::Reference {
            value: "now".into(),
        }),
    )
    .await
    .expect("taken");
    assert_eq!(
        said.err,
        [
            "loudness reference: 42 % (the current volume)",
            "note: reference is the current volume"
        ]
    );
}

/// A word that is not a number is refused in svolder's words before anything is sent.
#[tokio::test]
async fn a_value_that_is_no_number_is_refused_before_it_is_sent() {
    let daemon = serving(Some(view())).await;
    for bad in ["x", "NaN", "inf", "-5", "%", ""] {
        for command in [
            HostLoudnessCommand::Strength {
                percent: bad.into(),
            },
            HostLoudnessCommand::Reference { value: bad.into() },
        ] {
            let failure = verb(&daemon, false, Some(command)).await.unwrap_err();
            assert_eq!(failure.code, 1, "{bad}");
            assert!(failure.what.contains("is not a"), "{failure}");
        }
    }
    assert!(daemon.host_eq_ops().is_empty());
}

/// The daemon's own refusal comes back in its words with its code.
#[tokio::test]
async fn the_daemons_refusal_comes_back_in_its_words() {
    let daemon = serving(Some(view())).await;
    daemon.refuses_eq("the computer's volume has not been read yet");
    let failure = verb(
        &daemon,
        false,
        Some(HostLoudnessCommand::Reference {
            value: "now".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(failure.what.contains("has not been read"), "{failure}");
}

/// With no daemon every verb is refused with words that say what starts it; a daemon that serves
/// the EQ but knows no loudness is too old.
#[tokio::test]
async fn no_daemon_refuses_every_verb_and_an_old_daemon_says_so() {
    use HostLoudnessCommand as C;
    let direct = Globals {
        transport: "fake",
        ..globals("/nonexistent")
    };
    for command in [
        C::On,
        C::Off,
        C::Strength {
            percent: "5".into(),
        },
        C::Reference {
            value: "now".into(),
        },
    ] {
        let failure = answer(&direct, Ok(&Config::default()), None, false, Some(command))
            .await
            .unwrap_err();
        assert_eq!(failure.code, 3);
        assert_eq!(
            failure.to_string(),
            "loudness needs svolderd, and none serves the computer EQ. \
             Start it with `svolder daemon install`, then try again"
        );
    }
    let old = serving(None).await;
    let failure = verb(&old, false, Some(HostLoudnessCommand::On))
        .await
        .unwrap_err();
    assert_eq!(failure.code, 7);
    let failure = verb(&old, false, None).await.unwrap_err();
    assert_eq!(failure.code, 7);
}

/// With no daemon `show` reads the saved settings, says the live state is unknown and that
/// nothing follows the volume without a daemon or a TUI, and prints no readout.
#[tokio::test]
async fn show_without_a_daemon_reads_the_saved_settings() {
    let direct = Globals {
        transport: "fake",
        ..globals("/nonexistent")
    };
    let saved = LoudnessSettings {
        on: true,
        strength_pct: 60.0,
        reference_pct: 80.0,
    };
    let said = answer(&direct, Ok(&Config::default()), Some(saved), false, None)
        .await
        .expect("shown");
    assert_eq!(
        said.out,
        [
            "loudness: unknown: no daemon is running, so nothing follows the volume unless a \
             TUI does (the saved state says on)",
            "strength: 60 %",
            "reference: 80 %",
        ]
    );
    let said = answer(&direct, Ok(&Config::default()), None, false, None)
        .await
        .expect("shown");
    assert!(
        said.out[0].ends_with("(the saved state says off)"),
        "{said:?}"
    );
    assert_eq!(said.out[1], "strength: 50 %");

    let json = answer(&direct, Ok(&Config::default()), Some(saved), true, None)
        .await
        .expect("shown");
    let back: LoudnessView = serde_json::from_str(&json.out[0]).expect("json");
    assert!(back.on && back.unavailable.is_some() && back.volume_pct.is_none());
}

/// An unreadable configuration fails a verb but not `show`.
#[tokio::test]
async fn an_unreadable_config_fails_a_verb_but_not_show() {
    let daemon = serving(Some(view())).await;
    let socket = daemon.socket.display().to_string();
    let failure = answer(
        &globals(&socket),
        Err("config.toml: expected `=`"),
        None,
        false,
        Some(HostLoudnessCommand::On),
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(daemon.host_eq_ops().is_empty());
    let said = answer(&globals(&socket), Err("bad"), None, false, None)
        .await
        .expect("shown");
    assert!(said.out[0].starts_with("loudness: unknown"), "{said:?}");
}

/// A daemon that answers with its host audio off is not "no daemon": a verb exits 6, and `show`
/// says off whatever the saved state holds.
#[tokio::test]
async fn a_daemon_with_its_host_off_shows_off_and_refuses_a_verb_with_6() {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: false,
        eq: None,
        ..HostView::default()
    });
    let socket = daemon.socket.display().to_string();
    let saved = LoudnessSettings {
        on: true,
        strength_pct: 60.0,
        reference_pct: 80.0,
    };
    let said = answer(
        &globals(&socket),
        Ok(&Config::default()),
        Some(saved),
        false,
        None,
    )
    .await
    .expect("shown");
    assert_eq!(said.out[0], "loudness: off: the daemon's host audio is off");
    let json = answer(
        &globals(&socket),
        Ok(&Config::default()),
        Some(saved),
        true,
        None,
    )
    .await
    .expect("shown");
    let back: serde_json::Value = serde_json::from_str(&json.out[0]).expect("json");
    assert_eq!(back["on"], false);
    let failure = verb(&daemon, false, Some(HostLoudnessCommand::On))
        .await
        .unwrap_err();
    assert_eq!(failure.code, 6);
}

#[test]
fn show_with_no_daemon_reads_the_settings_kept_before_any_state_existed() {
    let dir = std::env::temp_dir().join(format!("svolder-ld-saved-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let state = dir.join("host-eq.toml");
    std::fs::write(
        dir.join("host-eq-settings.toml"),
        "[loudness]\non = false\nstrength_pct = 40\n",
    )
    .unwrap();
    let saved = super::saved_at(&state);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(saved.map(|s| s.strength_pct), Some(40.0));
}
