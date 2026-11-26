// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The setup wizard's screens over a walk the daemon runs: what is drawn is the daemon's prompt,
//! what a key answers goes to the daemon, and leaving cancels only this client's own walk.
//!
//! The walk crosses a real socket, so the keys are not timed: each one is released when the frame
//! it answers has been drawn (the terminal here tells a watcher every frame), which is a signal
//! and not a wait.

use std::convert::Infallible;
use std::io;

use crossterm::event::{Event, KeyCode, KeyEvent};
use futures::StreamExt;
use futures::stream::BoxStream;
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use svolder_walk::{Given, Step, Takes};
use tokio::sync::watch;

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};
use crate::test_support::walk_script::{
    JOB, choice_prompt, done, refusing, scanning, text_prompt, waiting, working,
};
use crate::wizard::fake::FakeEffects;

/// A terminal that tells a watcher the text of every frame it flushes.
struct Spy {
    inner: TestBackend,
    frames: watch::Sender<String>,
}

impl Spy {
    /// The text of the last frame.
    fn last(&self) -> String {
        self.frames.borrow().clone()
    }
}

impl Backend for Spy {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Infallible>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Infallible> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Infallible> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Infallible> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Infallible> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Infallible> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Infallible> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Infallible> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Infallible> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Infallible> {
        self.inner.flush()?;
        let buffer = self.inner.buffer();
        let text = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.frames.send_replace(text);
        Ok(())
    }
}

/// One key press.
fn key(code: KeyCode) -> io::Result<Event> {
    Ok(Event::Key(KeyEvent::from(code)))
}

/// Ctrl-C, which leaves from any step.
fn ctrl_c() -> io::Result<Event> {
    Ok(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        crossterm::event::KeyModifiers::CONTROL,
    )))
}

/// Keys that are each pressed once the frame on screen says `shows`, in order: the terminal's
/// whole input, which ends after the last of them.
fn when_shown(
    frames: watch::Receiver<String>,
    script: Vec<(&'static str, io::Result<Event>)>,
) -> BoxStream<'static, io::Result<Event>> {
    futures::stream::unfold(
        (frames, script.into_iter()),
        |(mut frames, mut script)| async move {
            let (shows, event) = script.next()?;
            frames
                .wait_for(|text| text.contains(shows))
                .await
                .expect("the terminal outlives the script");
            Some((event, (frames, script)))
        },
    )
    .boxed()
}

/// Walk a wizard over `daemon` with `script`, and answer with how the run ended.
async fn walked(
    daemon: &StubServer,
    given: Given,
    script: Vec<(&'static str, io::Result<Event>)>,
) -> (WizardEnd, Wizard<FakeEffects>) {
    let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, given);
    wizard
        .attach(&daemon.socket)
        .await
        .expect("the daemon walks");
    let (frames, watcher) = watch::channel(String::new());
    let mut terminal = Terminal::new(Spy {
        inner: TestBackend::new(80, 24),
        frames,
    })
    .expect("a test terminal opens");
    let mut events = when_shown(watcher, script);
    let end = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        event_loop(
            &mut terminal,
            &mut events,
            &mut wizard,
            &Theme::new(false),
            Leaving::Nowhere,
            false,
        ),
    )
    .await
    .unwrap_or_else(|_| panic!("stuck on:\n{}", terminal.backend().last()));
    (end, wizard)
}

fn scan() -> Prompt {
    choice_prompt(Step::Scan, "Listen for devices", &["Scan"])
}

fn pick() -> Prompt {
    choice_prompt(Step::Pick, "Which device to add", &["Bifrost 3"])
}

fn name() -> Prompt {
    text_prompt(Step::Name, "What to call it", "Bifrost 3", Takes::Text)
}

fn code() -> Prompt {
    text_prompt(
        Step::Passphrase,
        "What code it wants",
        "auto",
        Takes::Secret,
    )
}

async fn daemon() -> StubServer {
    let daemon = stub_daemon().await;
    daemon.lists_features(&["walk"]);
    daemon
}

/// The daemon's prompts are what is drawn, each key answers the daemon, and the walk ends with
/// the device the daemon wrote; the daemon's job is not cancelled after it ended.
#[tokio::test]
async fn the_daemons_prompts_are_drawn_and_answered_to_the_end() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.starts_with(vec![waiting(scan(), false)]);
    script.then(vec![working(scan()), waiting(pick(), true)]);
    script.then(vec![waiting(name(), true)]);
    script.then(vec![done("desk", "0102030405060708")]);
    let (end, _) = walked(
        &daemon,
        Given::default(),
        vec![
            ("Listen for devices", key(KeyCode::Enter)),
            ("Which device to add", key(KeyCode::Enter)),
            ("What to call it", key(KeyCode::Enter)),
            ("The background service", key(KeyCode::Enter)),
            ("Added desk", key(KeyCode::Enter)),
            ("Done", key(KeyCode::Enter)),
        ],
    )
    .await;
    match end {
        WizardEnd::Added(device) => assert_eq!(device.name, "desk"),
        _ => panic!("the walk did not end with the device"),
    }
    assert!(
        script.cancelled().is_empty(),
        "a finished walk was cancelled"
    );
}

/// Leaving mid-walk cancels the walk this client started and no other.
#[tokio::test]
async fn leaving_mid_walk_cancels_only_this_clients_walk() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.foreign(
        3,
        waiting(choice_prompt(Step::Scan, "Theirs", &["Scan"]), false),
    );
    script.starts_with(vec![waiting(scan(), false)]);
    let (end, _) = walked(
        &daemon,
        Given::default(),
        vec![("Listen for devices", key(KeyCode::Char('q')))],
    )
    .await;
    assert!(matches!(end, WizardEnd::Quit));
    assert_eq!(script.cancelled(), [JOB]);
}

/// A line typed at one step is not in the field of another after going back, even when the
/// step asked again says exactly what the first said, and a code typed on the Passphrase step
/// is not carried to the next time it is asked.
#[tokio::test]
async fn going_back_carries_nothing_typed_into_another_step() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.starts_with(vec![waiting(name(), true)]);
    // Back to the pick, forward to the same name prompt again; then on to the code.
    script.then(vec![waiting(pick(), true)]);
    script.then(vec![working(pick()), waiting(name(), true)]);
    script.then(vec![working(name()), waiting(code(), true)]);
    script.then(vec![waiting(name(), true)]);
    script.then(vec![working(name()), waiting(code(), true)]);
    let typed = vec![
        ("What to call it", key(KeyCode::Char('x'))),
        ("x", key(KeyCode::Esc)),
        ("Which device to add", key(KeyCode::Enter)),
        // Asked again: the field is empty, so the answer is the empty line.
        ("What to call it", key(KeyCode::Enter)),
        ("What code it wants", key(KeyCode::Char('1'))),
        ("‹ •___ ›", key(KeyCode::Esc)),
        ("What to call it", key(KeyCode::Enter)),
        // The code step again: an empty row, which an Enter refuses without asking the daemon.
        ("What code it wants", key(KeyCode::Enter)),
        ("a code is four digits", ctrl_c()),
    ];
    let (_, _) = walked(&daemon, Given::default(), typed).await;
    assert_eq!(
        script.answers(),
        [
            (0, None),
            (0, Some(String::new())),
            (0, Some(String::new())),
        ],
        "a typed line reached another step, or a half-typed code reached the daemon"
    );
}

/// A code is not kept once it is sent: a refused one is asked for on an empty row, and the
/// daemon is not sent it twice.
#[tokio::test]
async fn a_code_that_was_sent_is_not_kept() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.starts_with(vec![waiting(code(), true)]);
    script.then(vec![
        working(code()),
        refusing(
            code(),
            svolder_walk::RefusalKind::Passphrase,
            "the device turned the code away",
        ),
    ]);
    let mut typed = vec![];
    for digit in ['1', '2', '3', '4'] {
        typed.push(("What code it wants", key(KeyCode::Char(digit))));
    }
    typed.push(("‹ •••• ›", key(KeyCode::Enter)));
    typed.push(("turned the code away", key(KeyCode::Enter)));
    typed.push(("a code is four digits", ctrl_c()));
    let (end, _) = walked(&daemon, Given::default(), typed).await;
    assert!(
        matches!(end, WizardEnd::Quit),
        "the run did not end on the key"
    );
    assert_eq!(script.answers(), [(0, Some("1234".to_string()))]);
}

/// A signal that leaves the run (SIGHUP, SIGTERM: the terminal closed) ends the walk with the
/// signalled end, and cancels this client's own walk once, never another's.
#[tokio::test]
async fn a_signal_ends_the_run_and_cancels_only_this_clients_walk() {
    use crate::commands::interrupts::Presses;

    let daemon = daemon().await;
    let script = daemon.walk();
    script.foreign(
        3,
        waiting(choice_prompt(Step::Scan, "Theirs", &["Scan"]), false),
    );
    script.starts_with(vec![waiting(scan(), false)]);
    let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
    wizard
        .attach(&daemon.socket)
        .await
        .expect("the daemon walks");
    let presses = std::sync::Arc::new(Presses::new());
    wizard.leaving_by(std::sync::Arc::clone(&presses));

    let (frames, mut watcher) = watch::channel(String::new());
    let mut terminal = Terminal::new(Spy {
        inner: TestBackend::new(80, 24),
        frames,
    })
    .expect("a test terminal opens");
    // The press comes once the walk's prompt is on the screen.
    let signal = tokio::spawn(async move {
        watcher
            .wait_for(|text| text.contains("Listen for devices"))
            .await
            .expect("the terminal outlives the case");
        presses.press();
    });
    let mut events = futures::stream::pending::<io::Result<Event>>();
    let end = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        event_loop(
            &mut terminal,
            &mut events,
            &mut wizard,
            &Theme::new(false),
            Leaving::Nowhere,
            false,
        ),
    )
    .await
    .expect("the run ends on the press");
    signal.await.expect("the press was made");
    assert!(matches!(end, WizardEnd::Signalled));
    assert_eq!(
        script.cancelled(),
        [JOB],
        "once, and only this client's job"
    );
    drop(wizard);
    assert_eq!(script.cancelled(), [JOB], "the drop cancelled it again");
}

/// What a walk over two devices shows, as the real walk words it: the warning prompt of a daemon
/// with a linked device, and the pick after a scan with rows.
struct Shots(svolder_profiles::Registry);

impl svolder_walk::WalkEffects for Shots {
    type Failure = String;

    async fn scan(&self) -> Result<svolder_walk::Heard, String> {
        Ok(svolder_walk::Heard {
            entries: vec![
                crate::wizard::fake::seen_product("0102030405060708", "Bifrost 3", -41),
                crate::wizard::fake::seen_product("0807060504030201", "Gungnir 2", -58),
            ],
            notices: Vec::new(),
            adapters: None,
        })
    }

    fn scan_window(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }

    async fn try_connect(
        &self,
        _seen: &svolder_ipc::ScanEntry,
        _passphrase: &str,
    ) -> Result<svolder_ipc::Connect, String> {
        Ok(svolder_ipc::Connect::Accepted)
    }

    async fn switch_adapter(&self, _address: &str, _name: &str) -> Result<(), String> {
        Ok(())
    }

    async fn write_device(
        &self,
        _device: &svolder_ipc::DeviceConfig,
    ) -> Result<(), svolder_walk::WriteError<String>> {
        Ok(())
    }

    fn taken_names(&self) -> Vec<String> {
        vec!["desk".into()]
    }

    fn taken_units(&self) -> Vec<(String, String)> {
        vec![("desk".into(), "1112131415161718".into())]
    }

    fn registry(&self) -> Option<&svolder_profiles::Registry> {
        Some(&self.0)
    }

    fn linked(&self) -> Vec<String> {
        vec!["desk".into()]
    }
}

/// The frame the wizard draws for a daemon's walk standing at `state`, left with Ctrl-C.
async fn shown(state: svolder_ipc::JobState, expecting: &'static str) -> String {
    let daemon = daemon().await;
    daemon.walk().starts_with(vec![state]);
    let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
    wizard
        .attach(&daemon.socket)
        .await
        .expect("the daemon walks");
    let (frames, watcher) = watch::channel(String::new());
    let mut terminal = Terminal::new(Spy {
        inner: TestBackend::new(80, 24),
        frames,
    })
    .expect("a test terminal opens");
    let mut events = when_shown(watcher, vec![(expecting, ctrl_c())]);
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        event_loop(
            &mut terminal,
            &mut events,
            &mut wizard,
            &Theme::new(false),
            Leaving::ToTui,
            false,
        ),
    )
    .await
    .expect("the run ends");
    terminal.backend().last()
}

/// The two frames the walk's look check is made of, from the real walk's own prompts.
async fn walk_frames() -> (String, String) {
    let mut walk = svolder_walk::DeviceWalk::new(
        Shots(svolder_profiles::Registry::builtin().expect("the built-in profiles load")),
        Given::default(),
        svolder_walk::Configured::Mark,
    );
    let linked = walk.prompt();
    assert_eq!(linked.step, Step::Linked);
    walk.answer(0, None).await.expect("go on");
    walk.answer(0, None).await.expect("the scan runs");
    let pick = walk.prompt();
    assert_eq!(pick.step, Step::Pick);
    let warning = shown(waiting(linked, false), "A device is linked").await;
    let scan = shown(waiting(pick, true), "Which device").await;
    (warning, scan)
}

/// The warning prompt (a device holds a link) and the pick after a scan are drawn from the
/// daemon's own prompts.
#[tokio::test]
async fn the_warning_and_the_scan_are_drawn_from_the_daemons_prompts() {
    let (warning, scan) = walk_frames().await;
    assert!(warning.contains("desk"), "{warning}");
    assert!(warning.contains("Go on"), "{warning}");
    assert!(scan.contains("Bifrost 3"), "{scan}");
    assert!(scan.contains("Gungnir 2"), "{scan}");
}

/// Write the two frames as text under `target/d4w-shots`, for the look check
/// (`cargo test -p svolder write_the_walk_shots -- --ignored`).
#[tokio::test]
#[ignore = "writes the look check's text renders under target/"]
async fn write_the_walk_shots() {
    let (warning, scan) = walk_frames().await;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/d4w-shots");
    std::fs::create_dir_all(&dir).expect("the folder is made");
    for (name, frame) in [
        ("tui-walk-warning.txt", warning),
        ("tui-walk-scan-rows.txt", scan),
    ] {
        let text = frame
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(dir.join(name), format!("{text}\n")).expect("the shot is written");
    }
}

/// With a daemon, a given `--passphrase` answers the passphrase step in the screens too: the
/// answer is sent under the wait, not dropped by the poll that looks for a prompt already there.
#[tokio::test]
async fn a_given_code_answers_the_passphrase_step_in_the_screens() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.starts_with(vec![waiting(name(), true)]);
    script.then(vec![working(name()), waiting(code(), true)]);
    script.then(vec![done("desk", "0102030405060708")]);
    let given = Given {
        passphrase: Some("4821".into()),
        ..Given::default()
    };
    let (end, _) = walked(
        &daemon,
        given,
        vec![
            ("What to call it", key(KeyCode::Enter)),
            ("The background service", key(KeyCode::Enter)),
            ("Added desk", key(KeyCode::Enter)),
        ],
    )
    .await;
    assert!(matches!(end, WizardEnd::Added(_)));
    assert_eq!(
        script.answers(),
        [(0, Some(String::new())), (0, Some("4821".to_string()))]
    );
}

/// While the daemon's scan listens, the devices it has heard so far are drawn under the wait.
#[tokio::test]
async fn the_scan_draws_the_devices_heard_so_far() {
    let daemon = daemon().await;
    let script = daemon.walk();
    script.starts_with(vec![waiting(scan(), false)]);
    let heard = |unit: &str, product: &str, rssi: i16| svolder_walk::Row {
        entry: crate::wizard::fake::seen_product(unit, product, rssi),
        configured: None,
    };
    script.then(vec![scanning(
        scan(),
        vec![
            heard("0102030405060708", "Bifrost 3", -41),
            heard("0807060504030201", "Gungnir 2", -58),
        ],
    )]);
    let mut wizard = Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Given::default());
    wizard
        .attach(&daemon.socket)
        .await
        .expect("the daemon walks");
    let (frames, watcher) = watch::channel(String::new());
    let mut terminal = Terminal::new(Spy {
        inner: TestBackend::new(80, 24),
        frames,
    })
    .expect("a test terminal opens");
    let mut events = when_shown(
        watcher,
        vec![
            ("Listen for devices", key(KeyCode::Enter)),
            ("Gungnir 2", ctrl_c()),
        ],
    );
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        event_loop(
            &mut terminal,
            &mut events,
            &mut wizard,
            &Theme::new(false),
            Leaving::ToTui,
            false,
        ),
    )
    .await
    .unwrap_or_else(|_| panic!("stuck on:\n{}", terminal.backend().last()));
    let frame = terminal.backend().last();
    assert!(frame.contains("listening…"), "{frame}");
    assert!(
        frame.contains("Bifrost 3  0102030405060708  -41 dBm"),
        "{frame}"
    );
}
