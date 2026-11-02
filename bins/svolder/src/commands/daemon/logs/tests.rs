// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon logs` against fake `journalctl` and `tail` programs: where it reads from, the banner
//! it prints first and the colour it puts on each line.

use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use svolder_ipc::{Event, Handler, Request, Response};

use super::*;
use crate::commands::daemon::test_support::{Tools, installed_units, no_units};
use crate::commands::daemon::{POLL, RUNTIME_LISTEN_STREAM};
use crate::test_support::{install_script, temp_dir};

/// `logs` reads the journal where systemd keeps it, following it when asked, and tells
/// `journalctl` to page nothing and stamp in ISO form.
#[test]
fn logs_reads_the_journal_when_systemd_manages_the_daemon() {
    let tools = Tools::new(&installed_units(RUNTIME_LISTEN_STREAM));
    logs_from(
        &tools.ctl(),
        Path::new("/nowhere/svolderd.log"),
        &LogSaid::Unsaid,
        true,
        None,
    )
    .expect("the journal");
    assert!(
        tools
            .calls()
            .contains(&"journalctl --user -u svolderd --no-pager -o short-iso -f".to_string()),
        "{:?}",
        tools.calls()
    );
}

/// Without units it reads the file a daemon nobody installed writes.
#[test]
fn logs_reads_the_autospawn_log_when_nothing_manages_the_daemon() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-logs");
    let log = dir.0.join("svolderd.log");
    std::fs::write(&log, "started\n").expect("the log is written");

    logs_from(&tools.ctl(), &log, &LogSaid::Unsaid, false, None).expect("the log file");

    assert!(
        tools
            .calls()
            .contains(&format!("tail -n {LOG_LINES} {}", log.display())),
        "{:?}",
        tools.calls()
    );
}

/// A log file that is not there yet is said to be missing rather than tailed.
#[test]
fn logs_with_nothing_logged_yet_says_so() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-logs-empty");
    let log = dir.0.join("svolderd.log");

    logs_from(&tools.ctl(), &log, &LogSaid::Unsaid, false, None).expect("nothing to read");

    assert!(
        !tools.calls().iter().any(|call| call.starts_with("tail")),
        "{:?}",
        tools.calls()
    );
    let said = read_plan(&tools.ctl(), &log, &LogSaid::Unsaid, false)
        .err()
        .expect("nothing to read");
    assert_eq!(
        said,
        format!(
            "svolder: no daemon has logged anything to {} yet",
            log.display()
        )
    );
}

/// What a running daemon reports wins over the fallback: a path is tailed even where systemd
/// manages the unit, and `journal` is read even where it does not.
#[test]
fn logs_reads_the_source_a_daemon_reports() {
    let dir = temp_dir("daemon-logs-reported");
    let reported = dir.0.join("elsewhere.log");
    std::fs::write(&reported, "x\n").expect("the log is written");

    let managed = Tools::new(&installed_units(RUNTIME_LISTEN_STREAM));
    let path = reported.display().to_string();
    logs_from(
        &managed.ctl(),
        Path::new("/nowhere/a.log"),
        &LogSaid::Path(path.clone()),
        false,
        None,
    )
    .expect("the reported file");
    assert_eq!(
        managed.calls().last().unwrap(),
        &format!("tail -n {LOG_LINES} {path}")
    );

    let bare = Tools::new(&no_units());
    logs_from(
        &bare.ctl(),
        Path::new("/nowhere/a.log"),
        &LogSaid::Path("journal".into()),
        false,
        None,
    )
    .expect("the journal");
    assert!(
        bare.calls()
            .contains(&"journalctl --user -u svolderd --no-pager -o short-iso".to_string()),
        "{:?}",
        bare.calls()
    );
}

/// The banner names the file and when it was last written, or the journal.
#[test]
fn logs_names_its_source_first() {
    let dir = temp_dir("daemon-logs-banner");
    let log = dir.0.join("svolderd.log");
    std::fs::write(&log, "x\n").expect("the log is written");
    let tools = Tools::new(&no_units());

    let plan = read_plan(&tools.ctl(), &log, &LogSaid::Unsaid, false).expect("a file to read");
    let written =
        svolder_ipc::clock::local_stamp(std::fs::metadata(&log).unwrap().modified().unwrap());
    assert_eq!(
        plan.banner,
        format!(
            "svolder: reading {} (last written {})",
            log.display(),
            &written[..16]
        )
    );
    // `YYYY-MM-DD HH:MM`: sixteen characters, no seconds.
    assert!(plan.banner.ends_with(&format!("{})", &written[..16])));

    let plan = read_plan(&tools.ctl(), &log, &LogSaid::Path("journal".into()), false)
        .expect("the journal");
    assert_eq!(plan.banner, "svolder: reading the journal");
}

/// Colour only on a terminal with `NO_COLOR` unset.
#[test]
fn logs_colour_only_on_a_terminal_without_no_color() {
    assert!(wants_color(true, None));
    assert!(!wants_color(false, None));
    assert!(!wants_color(true, Some(OsStr::new("1"))));
    assert!(!wants_color(true, Some(OsStr::new(""))));
}

/// The stamp is dim, a warning line amber and an error line red, each span reset after, in the
/// file's shape and in the two shapes `journalctl -o short-iso` prints.
#[test]
fn colorize_dims_the_stamp_and_colours_the_level() {
    assert_eq!(
        colorize("2026-10-04 18:04:26 svolderd: listening"),
        "\x1b[2m2026-10-04 18:04:26\x1b[0m svolderd: listening"
    );
    assert_eq!(
        colorize("2026-10-04 18:04:26 svolderd: warning: slow"),
        "\x1b[2m2026-10-04 18:04:26\x1b[0m\x1b[33m svolderd: warning: slow\x1b[0m"
    );
    // The journal's own line, which the daemon writes without a stamp or a name of its own.
    assert_eq!(
        colorize("2026-10-04T18:04:26+0200 myhost svolderd[812]: error: gone"),
        "\x1b[2m2026-10-04T18:04:26+0200\x1b[0m\x1b[31m myhost svolderd[812]: error: gone\x1b[0m"
    );
    assert_eq!(
        colorize(
            "2026-10-04T18:04:26+0200 myhost svolderd[812]: listening on /run/user/1000/svolder.sock"
        ),
        "\x1b[2m2026-10-04T18:04:26+0200\x1b[0m myhost svolderd[812]: listening on /run/user/1000/svolder.sock"
    );
    // A journal line from a daemon that still wrote its own name, and the transport's lines.
    assert_eq!(
        colorize("2026-10-04T18:04:26+0200 myhost svolderd[812]: svolderd: warning: slow"),
        "\x1b[2m2026-10-04T18:04:26+0200\x1b[0m\x1b[33m myhost svolderd[812]: svolderd: warning: slow\x1b[0m"
    );
    assert_eq!(
        colorize("2026-10-04 18:04:26 svolder-ble: warning: 64 unread messages are queued"),
        "\x1b[2m2026-10-04 18:04:26\x1b[0m\x1b[33m svolder-ble: warning: 64 unread messages are queued\x1b[0m"
    );
    // A line from before stamps, and one that says nothing of a level, keep what they say.
    assert_eq!(
        colorize("svolderd: error: old"),
        "\x1b[31msvolderd: error: old\x1b[0m"
    );
    assert_eq!(colorize("svolder-ble: 1 ok"), "svolder-ble: 1 ok");
    assert_eq!(colorize(""), "");
}

/// A reader's output comes through the colourer line by line, and an uncoloured line is not
/// touched.
#[test]
fn a_reader_streams_through_the_colourer() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-logs-colour");
    let log = dir.0.join("svolderd.log");
    std::fs::write(&log, "x\n").expect("the log is written");
    let tail = tools.dir.0.join("tail");
    install_script(
        &tail,
        "#!/bin/sh\n\
         printf '%s\\n' '2026-10-04 18:04:26 svolderd: error: gone' 'plain'\n",
    );

    let mut out = Vec::new();
    logs_from(&tools.ctl(), &log, &LogSaid::Unsaid, false, Some(&mut out)).expect("the log file");
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "\x1b[2m2026-10-04 18:04:26\x1b[0m\x1b[31m svolderd: error: gone\x1b[0m\nplain\n"
    );
}

/// The words `status` prints for a daemon that writes no file are the ones the usage page
/// shows, and `logs` says the same before it reads the fallback.
#[test]
fn logs_says_a_daemon_that_writes_no_file_before_it_reads_the_fallback() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-logs-nofile");
    let log = dir.0.join("svolderd.log");
    std::fs::write(&log, "x\n").expect("the log is written");
    let plan = read_plan(&tools.ctl(), &log, &LogSaid::NoFile, false).expect("the fallback");
    assert_eq!(
        plan.note.as_deref(),
        Some(
            format!(
                "svolder: the daemon does not log to a file (stderr is a terminal or a pipe); \
                 reading {} instead",
                log.display()
            )
            .as_str()
        )
    );
    assert!(
        plan.banner
            .starts_with(&format!("svolder: reading {}", log.display()))
    );
    let older = read_plan(&tools.ctl(), &log, &LogSaid::Unsaid, false).expect("the fallback");
    assert_eq!(older.note, None, "an older daemon keeps today's line");
}

/// A daemon that takes the connection and never answers is given [`ASK_WAIT`], then read as
/// one that said nothing, so `logs` falls back instead of hanging.
#[tokio::test]
async fn a_daemon_that_never_answers_does_not_hold_logs_up() {
    struct Mute;

    #[async_trait::async_trait]
    impl Handler for Mute {
        async fn handle(&self, _request: Request) -> Response {
            std::future::pending().await
        }

        async fn subscribe(
            &self,
            _device: Option<String>,
        ) -> futures::stream::BoxStream<'static, Event> {
            futures::stream::pending().boxed()
        }
    }

    let dir = temp_dir("daemon-logs-mute");
    let socket = dir.0.join("svolder.sock");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.clone();
        async move {
            let _ = svolder_ipc::serve(&socket, Arc::new(Mute), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..100 {
        if svolder_ipc::is_running(&socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    let start = Instant::now();
    let said = tokio::time::timeout(ASK_WAIT * 10, reported_log(&socket))
        .await
        .expect("the ask is bounded");
    assert_eq!(said, LogSaid::Unsaid);
    assert!(start.elapsed() < ASK_WAIT * 5, "{:?}", start.elapsed());
    let _ = stop.send(());
    let _ = served.await;
}
