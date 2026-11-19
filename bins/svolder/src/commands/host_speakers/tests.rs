// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Each verb's request and printed answer against the stub daemon, with sets of the test's own
//! words: the CLI takes its labels, ids and licences from the daemon's views and lists, so none of
//! these names is the real catalogue's.

use svolder_host::eq::speakers::Room;
use svolder_host::eq::speakers::SetView;
use svolder_host::eq::{CrossfeedView, EqView, SpeakersView};
use svolder_ipc::host::HostView;
use svolder_ipc::{Config, HostEqOp, Reason, SpeakersOp, SpeakersRoom};

use super::*;
use crate::test_support::speakers_script::set;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};

/// A view with a chosen set of invented words, playing.
fn view() -> SpeakersView {
    SpeakersView {
        on: true,
        set: Some("alpha".to_string()),
        room: Some("with".to_string()),
        room_name: Some("With room".to_string()),
        term: Some("BRIR".to_string()),
        label: "Alpha hall, seat 2".to_string(),
        licence: "CC-BY-4.0".to_string(),
        credit: Some("Credit words of the alpha set".to_string()),
        source_url: "https://example.invalid/alpha".to_string(),
        angles: "left 30.0 deg, right 330.0 deg, 1.5 m".to_string(),
        taps_ms: 480.0,
        loudness_db: -7.04,
        sets: vec![
            SetView {
                id: "alpha".to_string(),
                ..SetView::default()
            },
            SetView {
                id: "beta".to_string(),
                ..SetView::default()
            },
        ],
        ..SpeakersView::default()
    }
}

/// A daemon that serves the computer EQ with `speakers` in it.
async fn serving(speakers: Option<SpeakersView>) -> StubServer {
    let daemon = stub_daemon().await;
    daemon.hosts(HostView {
        available: true,
        eq: Some(EqView {
            speakers,
            crossfeed: Some(CrossfeedView::default()),
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

/// Run `verb` against `daemon`, with the progress lines it printed.
async fn verb_with(
    daemon: &StubServer,
    json: bool,
    command: Option<HostSpeakersCommand>,
) -> (Result<Answer, Failure>, Vec<String>) {
    let socket = daemon.socket.display().to_string();
    let mut lines = Vec::new();
    let answered = answer(
        &globals(&socket),
        Ok(&Config::default()),
        Saved::default(),
        json,
        command,
        None,
        &mut |line| lines.push(line.to_string()),
    )
    .await;
    (answered, lines)
}

async fn verb(
    daemon: &StubServer,
    json: bool,
    command: Option<HostSpeakersCommand>,
) -> Result<Answer, Failure> {
    verb_with(daemon, json, command).await.0
}

/// The one request a verb sent to the computer EQ.
fn sent(daemon: &StubServer) -> HostEqOp {
    let mut ops = daemon.host_eq_ops();
    assert_eq!(ops.len(), 1, "{ops:?}");
    ops.remove(0)
}

fn on(on: Option<bool>, set: Option<&str>) -> HostEqOp {
    HostEqOp::Speakers {
        on,
        set: set.map(str::to_string),
        done: true,
    }
}

#[tokio::test]
async fn show_prints_the_state_the_choice_with_its_term_the_licence_and_the_loudness() {
    let mut shown = view();
    shown.note = Some("virtual speakers off: the output changed".to_string());
    let daemon = serving(Some(shown)).await;
    let said = verb(&daemon, false, None).await.expect("shown");
    assert_eq!(
        said.out,
        [
            "virtual speakers: on",
            "set: Alpha hall, seat 2 (alpha)",
            "choice: With room \u{b7} BRIR",
            "licence: CC-BY-4.0",
            "credit: Credit words of the alpha set",
            "source: https://example.invalid/alpha",
            "angles: left 30.0 deg, right 330.0 deg, 1.5 m",
            "length: 480 ms",
            "loudness: 7.0 dB quieter than with them off",
            "note: virtual speakers off: the output changed",
        ]
    );
    assert!(said.err.is_empty());
    assert!(daemon.host_eq_ops().is_empty());
}

#[tokio::test]
async fn the_show_verb_is_the_same_as_no_verb() {
    let daemon = serving(Some(view())).await;
    let bare = verb(&daemon, false, None).await.expect("bare");
    let named = verb(&daemon, false, Some(HostSpeakersCommand::Show))
        .await
        .expect("named");
    assert_eq!(bare, named);
}

#[tokio::test]
async fn show_with_no_set_chosen_says_so_and_names_the_no_room_choice_by_its_term_when_chosen() {
    let mut none = view();
    none.on = false;
    none.set = None;
    none.room = None;
    none.room_name = None;
    none.term = None;
    let daemon = serving(Some(none)).await;
    let said = verb(&daemon, false, None).await.expect("shown");
    assert_eq!(said.out[0], "virtual speakers: off");
    assert_eq!(said.out[1], "set: none chosen");
    assert_eq!(said.out.len(), 2, "{:?}", said.out);

    let mut free = view();
    free.room_name = Some("No room".to_string());
    free.term = Some("HRTF".to_string());
    free.loudness_db = 0.0;
    free.cut = true;
    free.taps_ms = 500.0;
    let daemon = serving(Some(free)).await;
    let said = verb(&daemon, false, None).await.expect("shown");
    assert!(
        said.out
            .contains(&"choice: No room \u{b7} HRTF".to_string())
    );
    assert!(said.out.contains(&"length: cut to 0.5 s".to_string()));
    assert!(
        said.out
            .contains(&"loudness: the same as with them off".to_string())
    );
}

#[tokio::test]
async fn show_json_is_the_view_as_the_daemon_sends_it() {
    let daemon = serving(Some(view())).await;
    let said = verb(&daemon, true, None).await.expect("shown");
    assert_eq!(said.out.len(), 1);
    let back: SpeakersView = serde_json::from_str(&said.out[0]).expect("a view");
    assert_eq!(back, view());
}

#[tokio::test]
async fn show_without_a_daemon_reads_the_saved_state_and_says_it_is_unknown() {
    let socket = crate::test_support::stub_daemon::fresh_socket();
    let socket = socket.display().to_string();
    let said = answer(
        &globals(&socket),
        Ok(&Config::default()),
        Saved {
            set: Some("alpha".to_string()),
            on: true,
        },
        false,
        None,
        None,
        &mut |_| {},
    )
    .await
    .expect("shown");
    assert!(said.out[0].starts_with("virtual speakers: unknown: no daemon is running"));
    assert!(
        said.out[0].ends_with("(the saved state says on)"),
        "{:?}",
        said.out
    );
    assert_eq!(said.out[1], "set: alpha");
    // Nothing else is claimed of a set the daemon did not describe (no loudness, no length).
    assert_eq!(said.out.len(), 2, "{:?}", said.out);
}

#[tokio::test]
async fn a_verb_without_a_daemon_says_the_daemon_is_needed_in_the_speakers_own_words() {
    let socket = crate::test_support::stub_daemon::fresh_socket();
    let socket = socket.display().to_string();
    for (command, what) in [
        (
            HostSpeakersCommand::On,
            "virtual speakers need svolderd, and none serves the computer EQ",
        ),
        (
            HostSpeakersCommand::Off,
            "virtual speakers need svolderd, and none serves the computer EQ",
        ),
        (
            HostSpeakersCommand::Use {
                id: "a".into(),
                on: false,
            },
            "virtual speakers need svolderd, and none serves the computer EQ",
        ),
        (
            HostSpeakersCommand::List { room: None },
            "the sets are fetched and kept by svolderd, and none is running",
        ),
        (
            HostSpeakersCommand::Fetch { id: "a".into() },
            "the sets are fetched and kept by svolderd, and none is running",
        ),
        (
            HostSpeakersCommand::Forget { id: "a".into() },
            "the sets are fetched and kept by svolderd, and none is running",
        ),
    ] {
        let failure = answer(
            &globals(&socket),
            Ok(&Config::default()),
            Saved::default(),
            false,
            Some(command),
            None,
            &mut |_| {},
        )
        .await
        .expect_err("refused");
        assert_eq!(failure.code, Kind::NoDaemon.code());
        assert_eq!(failure.what, what);
        assert!(!failure.what.contains("crossfeed"));
    }
}

#[tokio::test]
async fn a_daemon_that_does_not_serve_the_speakers_is_too_old() {
    let daemon = serving(None).await;
    let failure = verb(&daemon, false, None).await.expect_err("too old");
    assert_eq!(failure.code, Kind::DaemonTooOld.code());
}

#[tokio::test]
async fn on_and_off_send_the_op_and_say_what_the_daemon_shows_after() {
    let daemon = serving(Some(view())).await;
    let said = verb(&daemon, false, Some(HostSpeakersCommand::On))
        .await
        .expect("on");
    assert_eq!(sent(&daemon), on(Some(true), None));
    assert_eq!(said.err, ["virtual speakers on"]);
    assert!(said.out.is_empty());

    let mut off = view();
    off.on = false;
    off.note = Some("a note".to_string());
    let daemon = serving(Some(off)).await;
    let said = verb(&daemon, false, Some(HostSpeakersCommand::Off))
        .await
        .expect("off");
    assert_eq!(sent(&daemon), on(Some(false), None));
    assert_eq!(said.err, ["virtual speakers off", "note: a note"]);
}

#[tokio::test]
async fn the_daemons_refusal_of_a_verb_comes_back_in_its_words() {
    let daemon = serving(Some(view())).await;
    daemon.refuses_eq("no set is chosen: fetch one first");
    let failure = verb(&daemon, false, Some(HostSpeakersCommand::On))
        .await
        .expect_err("refused");
    assert_eq!(failure.what, "no set is chosen: fetch one first");
}

#[tokio::test]
async fn use_chooses_a_fetched_set_and_with_on_turns_them_on_too() {
    let daemon = serving(Some(view())).await;
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Use {
            id: "beta".into(),
            on: false,
        }),
    )
    .await
    .expect("chosen");
    assert_eq!(sent(&daemon), on(None, Some("beta")));
    assert_eq!(
        said.err,
        ["virtual speakers set: Alpha hall, seat 2 (alpha) \u{b7} With room \u{b7} BRIR"]
    );
    let daemon = serving(Some(view())).await;
    verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Use {
            id: "beta".into(),
            on: true,
        }),
    )
    .await
    .expect("chosen");
    assert_eq!(sent(&daemon), on(Some(true), Some("beta")));
}

#[tokio::test]
async fn use_of_a_set_that_is_not_fetched_sends_nothing_and_names_the_fetched_ones() {
    let daemon = serving(Some(view())).await;
    let failure = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Use {
            id: "gamma".into(),
            on: false,
        }),
    )
    .await
    .expect_err("refused");
    assert_eq!(failure.code, Kind::Usage.code());
    assert!(failure.what.contains("gamma"), "{}", failure.what);
    assert!(failure.next.contains("alpha, beta"), "{}", failure.next);
    assert!(daemon.host_eq_ops().is_empty());

    let mut empty = view();
    empty.sets.clear();
    let daemon = serving(Some(empty)).await;
    let failure = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Use {
            id: "gamma".into(),
            on: false,
        }),
    )
    .await
    .expect_err("refused");
    assert!(
        failure.next.contains("svolder host speakers fetch"),
        "{}",
        failure.next
    );
}

/// The catalogue of the tests: a fetched set with room, one not fetched (with its size), an
/// imported one, a damaged one.
fn lists(daemon: &StubServer) {
    let mut fetched = set("alpha", SpeakersRoom::With);
    fetched.label = "Alpha hall, seat 2".to_string();
    let mut absent = set("beta", SpeakersRoom::No);
    absent.fetched = false;
    absent.bytes = 36_700_160;
    let mut own = set("mine", SpeakersRoom::No);
    own.own = true;
    own.licence = String::new();
    let mut damaged = set("broken", SpeakersRoom::No);
    damaged.damaged = Some("its checksum differs".to_string());
    daemon.speakers().lists(vec![fetched, absent, own, damaged]);
}

#[tokio::test]
async fn list_shows_each_set_with_its_choice_term_licence_and_state() {
    let daemon = serving(Some(view())).await;
    lists(&daemon);
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::List { room: None }),
    )
    .await
    .expect("listed");
    let cells = |line: &String| line.split_whitespace().collect::<Vec<_>>().join(" ");
    let rows: Vec<String> = said.out.iter().map(cells).collect();
    assert_eq!(
        rows,
        [
            "ID CHOICE LABEL LICENCE STATE",
            "alpha With room \u{b7} BRIR Alpha hall, seat 2 CC-BY-4.0 fetched",
            "beta No room \u{b7} HRTF Label of beta CC-BY-4.0 not fetched (35.0 MB)",
            "mine No room \u{b7} HRTF Label of mine imported",
            "broken No room \u{b7} HRTF Label of broken CC-BY-4.0 damaged: its checksum differs",
        ]
    );
    assert_eq!(daemon.speakers().ops(), [SpeakersOp::Catalogue]);
}

#[tokio::test]
async fn list_keeps_only_the_sets_of_the_room_asked_for() {
    let daemon = serving(Some(view())).await;
    lists(&daemon);
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::List {
            room: Some(crate::cli::RoomArg::With),
        }),
    )
    .await
    .expect("listed");
    assert_eq!(said.out.len(), 2, "{:?}", said.out);
    assert!(said.out[1].starts_with("alpha"));
    let said = verb(
        &daemon,
        true,
        Some(HostSpeakersCommand::List {
            room: Some(crate::cli::RoomArg::No),
        }),
    )
    .await
    .expect("listed");
    let back: svolder_ipc::SpeakersCatalogue = serde_json::from_str(&said.out[0]).expect("json");
    let ids: Vec<_> = back.sets.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["beta", "mine", "broken"]);
}

#[tokio::test]
async fn list_says_when_the_daemon_fetches_nothing_and_when_there_are_no_sets() {
    let daemon = serving(Some(view())).await;
    daemon.speakers().can_fetch(false);
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::List { room: None }),
    )
    .await
    .expect("listed");
    assert_eq!(said.out, ["no sets"]);
    assert_eq!(
        said.err,
        [
            "this daemon is set up to fetch nothing: import your own file with `svolder host speakers import FILE`"
        ]
    );
}

#[tokio::test]
async fn list_says_the_fetching_is_off_once_in_the_daemons_words_when_it_has_them() {
    let daemon = serving(Some(view())).await;
    daemon.speakers().can_fetch(false);
    daemon
        .speakers()
        .notes("fetching sets is off: the base is x");
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::List { room: None }),
    )
    .await
    .expect("listed");
    assert_eq!(
        said.err,
        [
            "note: fetching sets is off: the base is x",
            "import your own file with `svolder host speakers import FILE`"
        ]
    );
}

#[test]
fn the_speakers_summary_fits_a_help_line() {
    use clap::CommandFactory;
    let mut cli = crate::cli::Cli::command();
    let host = cli.find_subcommand_mut("host").expect("host");
    let speakers = host.find_subcommand_mut("speakers").expect("speakers");
    let about = speakers.get_about().expect("about").to_string();
    assert!(about.chars().count() <= 60, "{about}");
}

#[tokio::test]
async fn fetch_waits_for_the_job_says_what_was_kept_and_prints_progress_in_steps() {
    let daemon = serving(Some(view())).await;
    let mut saved = set("alpha", SpeakersRoom::With);
    saved.loudness_db = -6.0;
    daemon.speakers().saves(saved);
    daemon
        .speakers()
        .reports(&[(1, 100), (12, 100), (13, 100), (55, 100), (100, 100)]);
    let (said, lines) = verb_with(
        &daemon,
        false,
        Some(HostSpeakersCommand::Fetch { id: "alpha".into() }),
    )
    .await;
    let said = said.expect("fetched");
    assert_eq!(
        daemon.speakers().ops(),
        [SpeakersOp::Fetch { id: "alpha".into() }]
    );
    assert_eq!(
        said.err,
        ["fetched Label of alpha (alpha): With room \u{b7} BRIR, 6.0 dB quieter"]
    );
    assert_eq!(
        lines,
        [
            "fetching alpha (1 kB)",
            "fetching alpha: 10 %",
            "fetching alpha: 50 %",
            "fetching alpha: 100 %"
        ]
    );
}

#[tokio::test]
async fn fetch_json_prints_the_saved_set() {
    let daemon = serving(Some(view())).await;
    daemon.speakers().saves(set("alpha", SpeakersRoom::No));
    let said = verb(
        &daemon,
        true,
        Some(HostSpeakersCommand::Fetch { id: "alpha".into() }),
    )
    .await
    .expect("fetched");
    let back: svolder_ipc::SpeakersSaved = serde_json::from_str(&said.out[0]).expect("json");
    assert_eq!(back.set.id, "alpha");
}

#[tokio::test]
async fn a_fetch_that_fails_gives_the_daemons_words_and_a_step_by_reason() {
    for (reason, step) in [
        (Reason::NotListed, "svolder host speakers list"),
        (Reason::Unreachable, "svolder host speakers import FILE"),
    ] {
        let daemon = serving(Some(view())).await;
        daemon.speakers().fails("the words of the daemon", reason);
        let failure = verb(
            &daemon,
            false,
            Some(HostSpeakersCommand::Fetch { id: "alpha".into() }),
        )
        .await
        .expect_err("fails");
        assert_eq!(failure.what, "the words of the daemon");
        assert!(failure.next.contains(step), "{reason:?}: {}", failure.next);
    }
}

#[tokio::test]
async fn a_fetch_refused_because_fetching_is_off_offers_the_import_and_no_retry() {
    let daemon = serving(Some(view())).await;
    daemon.speakers().fails(
        &format!(
            "{}: the sets' base address is x",
            svolder_ipc::SPEAKERS_OFF_PREFIX
        ),
        Reason::Unreachable,
    );
    let failure = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Fetch { id: "alpha".into() }),
    )
    .await
    .expect_err("fails");
    assert!(failure.next.contains("svolder host speakers import FILE"));
    assert!(!failure.next.contains("Retry"), "{}", failure.next);
}

#[tokio::test]
async fn a_press_ends_a_fetch_as_interrupted_with_the_shell_s_code_for_it() {
    let daemon = serving(Some(view())).await;
    daemon.speakers().holds_jobs();
    let presses = Arc::new(crate::commands::interrupts::Presses::new());
    presses.press();
    let socket = daemon.socket.display().to_string();
    let failure = answer(
        &globals(&socket),
        Ok(&Config::default()),
        Saved::default(),
        false,
        Some(HostSpeakersCommand::Fetch { id: "alpha".into() }),
        Some(presses),
        &mut |_| {},
    )
    .await
    .expect_err("interrupted");
    assert_eq!(failure.code, 130);
    assert_eq!(daemon.speakers().cancelled().len(), 1);
}

#[tokio::test]
async fn import_sends_an_absolute_path_with_the_label_and_the_room() {
    let daemon = serving(Some(view())).await;
    let mut saved = set("mine", SpeakersRoom::With);
    saved.own = true;
    daemon.speakers().saves(saved);
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Import {
            file: "relative/room.wav".into(),
            label: Some("Mine".into()),
            room: Some(crate::cli::RoomArg::With),
        }),
    )
    .await
    .expect("imported");
    let [SpeakersOp::Import { path, label, room }] = &daemon.speakers().ops()[..] else {
        panic!("{:?}", daemon.speakers().ops());
    };
    assert!(std::path::Path::new(path).is_absolute(), "{path}");
    assert!(path.ends_with("relative/room.wav"), "{path}");
    assert_eq!(label.as_deref(), Some("Mine"));
    assert_eq!(*room, Some(SpeakersRoom::With));
    assert_eq!(
        said.err,
        [
            "imported Label of mine (mine): With room \u{b7} BRIR, the same loudness as with them off"
        ]
    );
}

#[tokio::test]
async fn forget_sends_the_id_and_a_refusal_for_a_set_in_use_names_the_step() {
    let daemon = serving(Some(view())).await;
    let said = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Forget { id: "beta".into() }),
    )
    .await
    .expect("forgotten");
    assert_eq!(
        daemon.speakers().ops(),
        [SpeakersOp::Forget { id: "beta".into() }]
    );
    assert_eq!(said.err, ["forgot beta"]);
    let said = verb(
        &daemon,
        true,
        Some(HostSpeakersCommand::Forget { id: "beta".into() }),
    )
    .await
    .expect("forgotten");
    assert_eq!(said.out, [r#"{"forgot":"beta"}"#]);
    assert!(said.err.is_empty());

    let daemon = serving(Some(view())).await;
    daemon.speakers().refuses_forget(
        "alpha plays now; turn virtual speakers off first",
        Reason::Busy,
    );
    let failure = verb(
        &daemon,
        false,
        Some(HostSpeakersCommand::Forget { id: "alpha".into() }),
    )
    .await
    .expect_err("busy");
    assert_eq!(
        failure.what,
        "alpha plays now; turn virtual speakers off first"
    );
    // The daemon's words carry the step.
    assert_eq!(failure.next, "");
}

#[test]
fn progress_starts_with_a_line_then_prints_each_tenth_once() {
    let mut steps = Steps::default();
    let at = |steps: &mut Steps, received, total| {
        steps.line(&svolder_ipc::SpeakersProgress {
            id: "x".into(),
            received,
            total,
        })
    };
    assert_eq!(at(&mut steps, 0, 3 << 20), ["fetching x (3.0 MB)"]);
    assert!(at(&mut steps, 9, 100).is_empty());
    assert_eq!(at(&mut steps, 10, 100), ["fetching x: 10 %"]);
    assert!(at(&mut steps, 19, 100).is_empty());
    assert_eq!(at(&mut steps, 35, 100), ["fetching x: 30 %"]);
    assert!(at(&mut steps, 35, 100).is_empty());
    assert_eq!(at(&mut steps, 200, 100), ["fetching x: 100 %"]);
}

#[test]
fn a_first_report_already_past_a_tenth_prints_the_start_line_and_the_tenth() {
    let mut steps = Steps::default();
    let lines = steps.line(&svolder_ipc::SpeakersProgress {
        id: "x".into(),
        received: 100,
        total: 100,
    });
    assert_eq!(lines, ["fetching x (1 kB)", "fetching x: 100 %"]);
}

#[test]
fn an_unknown_total_gets_the_start_line_and_nothing_after() {
    let mut steps = Steps::default();
    let mut at = |received| {
        steps.line(&svolder_ipc::SpeakersProgress {
            id: "x".into(),
            received,
            total: 0,
        })
    };
    assert_eq!(at(0), ["fetching x"]);
    assert!(at(500).is_empty());
}

#[test]
fn the_crossfeed_giving_way_is_said_once() {
    let on = |on| CrossfeedView {
        on,
        ..CrossfeedView::default()
    };
    assert!(gave_way(Some(&on(true)), Some(&on(false))));
    assert!(!gave_way(Some(&on(false)), Some(&on(false))));
    assert!(!gave_way(Some(&on(true)), Some(&on(true))));
    assert!(!gave_way(None, None));
}

/// The help names each choice with its technical term, spelled by the words the rest of the
/// program takes from one table.
#[test]
fn the_help_shows_each_choice_with_its_technical_term() {
    use clap::CommandFactory;
    let mut cli = crate::cli::Cli::command();
    let host = cli.find_subcommand_mut("host").expect("host");
    let speakers = host.find_subcommand_mut("speakers").expect("speakers");
    let list = speakers.find_subcommand_mut("list").expect("list");
    let help = list.render_long_help().to_string();
    for room in [Room::With, Room::No] {
        let words = format!("{} \u{b7} {}", room.name(), room.term());
        assert!(help.contains(&words), "{words} is not in\n{help}");
    }
    let top = speakers.render_long_help().to_string();
    for room in [Room::With, Room::No] {
        let words = format!("{} \u{b7} {}", room.name(), room.term());
        assert!(top.contains(&words), "{words} is not in\n{top}");
    }
    // The `--room` values: clap needs literals, so each is held to the table here.
    use clap::ValueEnum;
    for (arg, room) in [
        (crate::cli::RoomArg::With, Room::With),
        (crate::cli::RoomArg::No, Room::No),
    ] {
        let value = arg.to_possible_value().expect("a value");
        let help = value
            .get_help()
            .map(ToString::to_string)
            .unwrap_or_default();
        let words = format!("{} \u{b7} {}", room.name(), room.term());
        assert!(
            help.starts_with(&format!("{words}:")),
            "{help:?} does not start with {words:?}"
        );
        assert_eq!(value.get_name(), room.word());
    }
}

#[test]
fn show_with_no_daemon_reads_the_set_kept_before_any_state_existed() {
    let dir = std::env::temp_dir().join(format!("svolder-sp-saved-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let state = dir.join("host-eq.toml");
    std::fs::write(dir.join("host-eq-settings.toml"), "speakers = \"set-a\"\n").unwrap();
    let saved = super::saved_at(&state);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(saved.set.as_deref(), Some("set-a"));
    assert!(!saved.on);
}
