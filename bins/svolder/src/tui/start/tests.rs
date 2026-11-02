// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::future::pending;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers};
use tokio::time::Instant;

use super::*;
use crate::tui::decoration::intro::test_event_stream;
use crate::tui::only_socket;
use crate::tui::test_support::{ANSWERS_IN, key_press};

/// The demo's toast is timed from the moment it is raised, which is after the intro: an app
/// built long before still shows it for its whole six seconds.
#[test]
fn the_join_toast_starts_when_it_is_raised() {
    let mut app = crate::tui::test_support::app_showing(app::Screen::Device);
    let later = std::time::Instant::now() + Duration::from_secs(10);
    advise_join(
        &mut app,
        std::path::Path::new("/tmp/sd-abcdef/sv.sock"),
        later,
    );
    assert_eq!(
        app.session.join_hint.as_deref(),
        Some("svolder tui --join /tmp/sd-abcdef/sv.sock")
    );
    app.tick(later + Duration::from_secs(5));
    assert!(
        app.toast()
            .is_some_and(|t| t.text.starts_with("svolder tui --join "))
    );
    app.tick(later + Duration::from_secs(7));
    assert!(app.toast().is_none());
}

/// A start, or the setup hand-over, whose open failed does not end the run: the failure is
/// where it lands, which is the Devices screen saying it.
#[tokio::test]
async fn a_failed_open_lands_on_the_devices_screen_instead_of_ending_the_run() {
    let opener = |_: crate::backend::OwnedGlobals| async {
        Err::<(crate::backend::Backend, Vec<crate::backend::DeviceRow>), _>(Failure::of(
            Kind::NothingInRange,
            "nothing answered",
        ))
    };
    let globals = crate::backend::OwnedGlobals {
        transport: "fake".into(),
        adapter: None,
        socket: None,
        device: None,
        verbose: false,
    };
    let path = std::env::temp_dir().join("svolder-tui-no-such-config.toml");
    let opener = Opener::new(globals, svolder_ipc::TuiConfig::default(), path, opener);
    let opened = opener.reopen(Some("amp"), None).await;
    let watching = landed(&opener, Some("amp"), opened).await;
    assert_eq!(watching.app.screen, app::Screen::Devices);
    assert!(matches!(
        watching.app.session.holding,
        Some(app::Holding::Nothing { why: Some(_) })
    ));
}

/// `svolder tui --join <dead socket>` asks that socket and nothing else: `demo::join` puts
/// the socket where every open reaches only it, and a start, a hold and a landing over a
/// socket nothing listens on make neither the socket nor its `.lock`.
#[tokio::test]
async fn joining_a_dead_socket_makes_neither_the_socket_nor_its_lock() {
    use run::Open;

    let dir = std::env::temp_dir().join(format!("svolder-join-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory is made");
    let dead = dir.join("none.sock");
    let lock = dir.join("none.sock.lock");

    let options = crate::commands::demo::join_options(false, dead.clone());
    assert_eq!(options.join.as_deref(), Some(dead.as_path()));
    assert_eq!(options.demo, None);
    assert_eq!(only_socket(&options), Some(dead.clone()));

    let shown = dead.to_string_lossy().into_owned();
    let globals = crate::backend::Globals {
        transport: "auto",
        adapter: None,
        socket: Some(&shown),
        device: None,
        verbose: false,
    }
    .owned();
    let live = run::Live {
        only: only_socket(&options),
    };
    let failure = live
        .open(globals.clone(), &dead.with_file_name("config.toml"))
        .await
        .err()
        .expect("nothing answers there");
    assert!(failure.what.contains("none.sock"), "{failure}");
    assert!(
        live.hold(None, globals, None, &dead.with_file_name("config.toml"))
            .await
            .is_none()
    );

    assert!(!dead.exists(), "the socket was made");
    assert!(!lock.exists(), "the lock was made");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A start over a terminal that draws nowhere, with a mark of 500 ms.
async fn started<T>(
    show_intro: bool,
    keys: &[char],
    opening: impl Future<Output = Result<T, Failure>>,
) -> (
    Begun<T>,
    Duration,
    ratatui::Terminal<ratatui::backend::TestBackend>,
) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))
        .expect("a test terminal");
    let (mut events, tx) = test_event_stream();
    for key in keys {
        tx.send(key_press(KeyCode::Char(*key), KeyModifiers::NONE))
            .expect("the stream is open");
    }
    let theme = theme::Theme::new(false);
    let mut plan = intro::IntroPlan::fade(false);
    let mut opening = Box::pin(opening);
    let started = Instant::now();
    let begun = tokio::time::timeout(
        Duration::from_secs(60),
        begin(
            &mut terminal,
            &mut events,
            &theme,
            &mut plan,
            show_intro,
            &mut opening,
            None,
            |_: &T| None,
        ),
    )
    .await
    .expect("the start ended");
    (begun, started.elapsed(), terminal)
}

async fn answering(after: Duration) -> Result<u8, Failure> {
    tokio::time::sleep(after).await;
    Ok(7)
}

fn answered(begun: &Begun<u8>) -> bool {
    matches!(begun, Begun::Opened { opened: Ok(7), .. })
}

#[tokio::test(start_paused = true)]
async fn a_device_that_answers_during_the_mark_lets_the_mark_finish() {
    let (begun, elapsed, _) = started(true, &[], answering(Duration::from_millis(100))).await;
    assert!(answered(&begun));
    assert_eq!(elapsed, Duration::from_millis(500));
}

#[tokio::test(start_paused = true)]
async fn a_mark_that_ends_first_waits_for_the_device() {
    let (begun, elapsed, _) = started(true, &[], answering(ANSWERS_IN)).await;
    assert!(answered(&begun));
    assert_eq!(elapsed, ANSWERS_IN);
}

/// An answer at the instant the mark ends is taken, whichever side of the end it lands on.
/// With no sound to start, the result is the same whether it was taken during the intro or
/// right after it, so this cannot tell the two apart.
#[tokio::test(start_paused = true)]
async fn a_device_that_answers_as_the_mark_ends_is_taken() {
    let (begun, elapsed, _) = started(true, &[], answering(Duration::from_millis(500))).await;
    assert!(answered(&begun));
    assert_eq!(elapsed, Duration::from_millis(500));
}

#[tokio::test(start_paused = true)]
async fn with_no_intro_the_start_waits_on_the_device_alone() {
    let (begun, elapsed, terminal) = started(false, &[], answering(ANSWERS_IN)).await;
    assert!(answered(&begun));
    assert_eq!(elapsed, ANSWERS_IN);
    let blank = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 24));
    assert_eq!(terminal.backend().buffer(), &blank, "a frame was drawn");
}

#[tokio::test(start_paused = true)]
async fn q_over_the_mark_gives_up_on_a_device_that_has_not_answered() {
    let (begun, elapsed, _) = started::<u8>(true, &['q'], pending::<Result<u8, Failure>>()).await;
    assert!(matches!(begun, Begun::GaveUp));
    assert_eq!(elapsed, Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn with_no_intro_q_gives_up_on_a_device_that_has_not_answered() {
    let (begun, _, _) = started::<u8>(false, &['q'], pending::<Result<u8, Failure>>()).await;
    assert!(matches!(begun, Begun::GaveUp));
}

#[tokio::test(start_paused = true)]
async fn an_open_that_failed_is_still_an_open_that_answered() {
    let failed = async { Err::<u8, _>(Failure::of(Kind::Dropped, "x")) };
    let (begun, _, _) = started(true, &[], failed).await;
    assert!(matches!(begun, Begun::Opened { opened: Err(_), .. }));
}

/// Records, in order, the terminal being given back and what is written.
#[derive(Clone, Default)]
struct Log(std::sync::Arc<std::sync::Mutex<String>>);

struct Restored(Log);

impl Drop for Restored {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().push_str("[restored]");
    }
}

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn giving_up_gives_the_terminal_back_before_it_speaks_and_leaves_with_success() {
    let log = Log::default();
    let mut writer = log.clone();
    let code = leave_unopened(Restored(log.clone()), &mut writer);
    assert_eq!(
        *log.0.lock().unwrap(),
        "[restored]svolder: interrupted before the device answered\n"
    );
    assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
}

/// The `n` wizard reads the configuration file the TUI shows: a file that does not parse
/// stops it, in words quoting that file.
#[tokio::test]
async fn the_n_wizard_gets_the_path_the_tui_shows() {
    let dir = crate::test_support::temp_dir("tui-n-wizard-path");
    let path = dir.join("config.toml");
    std::fs::write(&path, "this is = not [valid\n").expect("the file is written");
    let g = Globals {
        transport: "fake",
        socket: None,
        adapter: None,
        device: None,
        verbose: false,
    };
    let adder = Live {
        g: &g,
        path,
        refuse: None,
    };
    let failure = adder.wizard().await.err().expect("the file does not parse");
    assert!(failure.what.contains("this is = not"), "{failure}");
}
