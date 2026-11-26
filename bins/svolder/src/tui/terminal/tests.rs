// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::tui::test_support;

type Log = Rc<RefCell<Vec<&'static str>>>;

struct LoggedMouse(Log);

impl MouseCapture for LoggedMouse {
    fn set_mouse(&mut self, on: bool) -> std::io::Result<()> {
        self.0
            .borrow_mut()
            .push(if on { "mouse on" } else { "mouse off" });
        Ok(())
    }
}

struct LoggedFlush(Log);

impl InputFlush for LoggedFlush {
    fn flush_input(&mut self) {
        self.0.borrow_mut().push("flush");
    }
}

#[test]
fn input_is_flushed_after_the_mouse_goes_off() {
    let log = Log::default();
    mouse_off_then_flush(&mut LoggedMouse(log.clone()), &mut LoggedFlush(log.clone()));
    assert_eq!(*log.borrow(), ["mouse off", "flush"]);
}

#[test]
fn input_is_flushed_even_when_the_terminal_refuses_the_mouse() {
    struct Refusing;
    impl MouseCapture for Refusing {
        fn set_mouse(&mut self, _on: bool) -> std::io::Result<()> {
            Err(std::io::Error::other("refused"))
        }
    }
    let log = Log::default();
    mouse_off_then_flush(&mut Refusing, &mut LoggedFlush(log.clone()));
    assert_eq!(*log.borrow(), ["flush"]);
}

/// An input queue on a clock of its own: each `readable` says yes until `until`, and takes the
/// time it was given when it says no, or `step` when it says yes.
struct Scripted {
    now: Duration,
    /// The time input stops arriving at.
    until: Duration,
    /// What a read takes.
    step: Duration,
    log: Vec<String>,
}

impl Scripted {
    fn arriving_until(until_ms: u64) -> Scripted {
        Scripted {
            now: Duration::ZERO,
            until: Duration::from_millis(until_ms),
            step: Duration::from_millis(10),
            log: Vec::new(),
        }
    }
}

impl InputQueue for Scripted {
    fn now(&mut self) -> Duration {
        self.now
    }

    fn readable(&mut self, within: Duration) -> bool {
        if self.now < self.until {
            self.log.push("readable: yes".into());
            true
        } else {
            self.now += within;
            self.log
                .push(format!("readable: quiet {}", within.as_millis()));
            false
        }
    }

    fn discard(&mut self) -> bool {
        self.now += self.step;
        self.log.push("discard".into());
        true
    }

    fn flush(&mut self) {
        self.log.push("flush".into());
    }
}

/// Reports sent while the terminal catches up are read and dropped until stdin has been quiet
/// for 30 ms, and only then is what is left flushed.
#[test]
fn the_drain_reads_until_a_quiet_window_and_then_flushes() {
    let mut queue = Scripted::arriving_until(40);
    drain_then_flush(&mut queue);
    assert_eq!(
        queue.log,
        [
            "readable: yes",
            "discard",
            "readable: yes",
            "discard",
            "readable: yes",
            "discard",
            "readable: yes",
            "discard",
            "readable: quiet 30",
            "flush"
        ]
    );
}

/// Input that never stops is cut off at 150 ms in all, and the flush still follows.
#[test]
fn the_drain_stops_at_the_cap() {
    let mut queue = Scripted::arriving_until(u64::MAX / 4);
    drain_then_flush(&mut queue);
    assert_eq!(queue.now, DRAIN_CAP, "fifteen reads of ten milliseconds");
    assert_eq!(queue.log.last().map(String::as_str), Some("flush"));
    assert_eq!(queue.log.iter().filter(|l| *l == "flush").count(), 1);
}

/// With nothing waiting, one quiet window passes and the flush follows at once.
#[test]
fn a_quiet_stdin_is_flushed_after_one_window() {
    let mut queue = Scripted::arriving_until(0);
    drain_then_flush(&mut queue);
    assert_eq!(queue.log, ["readable: quiet 30", "flush"]);
}

/// A read that takes nothing (stdin closed, say) ends the drain instead of spinning to the cap.
#[test]
fn a_read_that_takes_nothing_ends_the_drain() {
    struct Closed(Vec<&'static str>);
    impl InputQueue for Closed {
        fn now(&mut self) -> Duration {
            Duration::ZERO
        }
        fn readable(&mut self, _within: Duration) -> bool {
            true
        }
        fn discard(&mut self) -> bool {
            self.0.push("discard");
            false
        }
        fn flush(&mut self) {
            self.0.push("flush");
        }
    }
    let mut queue = Closed(Vec::new());
    drain_then_flush(&mut queue);
    assert_eq!(queue.0, ["discard", "flush"]);
}

/// A reader that finds nothing to take (would-block) is polled again rather than waited on,
/// and the drain ends once stdin is quiet, well inside the cap.
#[test]
fn a_would_block_read_never_stalls_the_drain() {
    struct Blocking {
        now: Duration,
        polls: u32,
    }
    impl InputQueue for Blocking {
        fn now(&mut self) -> Duration {
            self.now
        }
        fn readable(&mut self, within: Duration) -> bool {
            self.polls += 1;
            self.now += within.min(Duration::from_millis(5));
            self.polls <= 2
        }
        fn discard(&mut self) -> bool {
            true
        }
        fn flush(&mut self) {
            self.polls += 100;
        }
    }
    let mut queue = Blocking {
        now: Duration::ZERO,
        polls: 0,
    };
    drain_then_flush(&mut queue);
    assert_eq!(queue.polls, 103, "polled until quiet, then flushed once");
    assert!(queue.now < DRAIN_CAP);
}

fn unreadable() -> Result<Config, svolder_ipc::ConfigError> {
    Err(svolder_ipc::ConfigError::Parse("bad toml".into()))
}

/// A run whose output is being read says so before it looks at the file.
#[test]
fn preflight_refuses_a_missing_terminal_before_the_file() {
    let failure = preflight(&test_support::Refusing::new(false), unreadable())
        .expect_err("no terminal is a refusal");
    assert_eq!(failure.code, Kind::Usage.code());
    assert!(
        failure.what.contains("needs a terminal"),
        "{}",
        failure.what
    );
}

#[test]
fn preflight_refuses_an_unreadable_file_on_a_terminal() {
    let failure = preflight(&test_support::Refusing::new(true), unreadable())
        .expect_err("an unreadable file is a refusal");
    assert_eq!(failure.code, Kind::ConfigUnreadable.code());
}

#[test]
fn preflight_hands_back_a_readable_file() {
    assert!(preflight(&test_support::Refusing::new(true), Ok(Config::default())).is_ok());
}

#[test]
fn a_terminal_that_cannot_be_claimed_is_a_usage_failure() {
    let failure = claimed(&test_support::Refusing::new(true), false)
        .err()
        .expect("the claim is refused");
    assert_eq!(failure.what, "the terminal: no tty");
    assert_eq!(failure.code, Kind::Usage.code());
}

/// A writer that counts the writes it takes.
#[derive(Default, Clone)]
struct Counted(Rc<RefCell<usize>>);

impl std::io::Write for Counted {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        *self.0.borrow_mut() += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A whole frame of a large terminal reaches the terminal in one write, not one per escape
/// sequence and cell.
#[test]
fn a_frame_goes_out_in_one_write() {
    let writes = Counted::default();
    let backend = ratatui::backend::CrosstermBackend::new(frame_writer(writes.clone()));
    let mut terminal = ratatui::Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 200, 60)),
        },
    )
    .expect("a terminal");
    *writes.0.borrow_mut() = 0;
    terminal
        .draw(|frame| {
            let area = frame.area();
            for y in 0..area.height {
                let colour = ratatui::style::Color::Rgb(0, 0, y as u8);
                let line = ratatui::text::Span::styled(
                    "spectrum ".repeat(22),
                    ratatui::style::Style::new().fg(colour),
                );
                frame.render_widget(
                    line,
                    ratatui::layout::Rect {
                        y,
                        height: 1,
                        ..area
                    },
                );
            }
        })
        .expect("the frame draws");
    assert_eq!(*writes.0.borrow(), 1);
}

/// A writer that logs what reaches it, each write as text, into a log the case shares.
struct Logged(Rc<RefCell<Vec<String>>>);

impl std::io::Write for Logged {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .borrow_mut()
            .push(String::from_utf8_lossy(buf).into_owned());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a frame left unsent and the cursor reach the alternate screen before it is left: none of
/// it lands on the screen the shell gets back.
#[test]
fn a_frames_leftovers_go_out_before_the_screen_is_left() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let backend = ratatui::backend::CrosstermBackend::new(frame_writer(Logged(log.clone())));
    let mut terminal = ratatui::Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 20, 2)),
        },
    )
    .expect("a terminal");
    std::io::Write::write_all(terminal.backend_mut(), b"left over").expect("buffered");
    log.borrow_mut().clear();
    leave_screen(&mut terminal, || log.borrow_mut().push("left".into()));
    let log = log.borrow();
    let left = log
        .iter()
        .position(|w| w == "left")
        .expect("the screen was left");
    let sent = log
        .iter()
        .position(|w| w.contains("left over"))
        .expect("the leftover went out");
    assert!(sent < left, "{log:?}");
    assert!(
        log[..left].iter().any(|w| w.contains("\x1b[?25h")),
        "the cursor showed only after the screen was left: {log:?}"
    );
}

mod panics {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::tui::app::Screen;

    /// The panic hook is the process's: the tests that install one take turns. Other tests'
    /// panics (`should_panic`, `catch_unwind`) are not held back by this, so each test owns its
    /// router, claimed by its own thread alone, and reads only what carries its own message: a
    /// foreign panic lands in the list or the earlier hook's count, and is not looked at.
    static HOOK: Mutex<()> = Mutex::new(());

    /// A hook of the test's own for as long as it lives, the old one back after. What the hook's
    /// three outputs did is logged in order: `restore`, `print: <panic>`, `left: <line>`.
    struct Installed {
        saved: Option<PanicHook>,
        _turn: std::sync::MutexGuard<'static, ()>,
        events: Arc<Mutex<Vec<String>>>,
        router: &'static PanicRouter,
    }

    impl Installed {
        fn new() -> Installed {
            let turn = HOOK.lock().unwrap_or_else(|e| e.into_inner());
            let router: &'static PanicRouter = Box::leak(Box::new(PanicRouter::new()));
            let events = Arc::new(Mutex::new(Vec::new()));
            let saved = std::panic::take_hook();
            let (r, p, l) = (events.clone(), events.clone(), events.clone());
            std::panic::set_hook(routing_hook(
                router,
                move || r.lock().unwrap().push("restore".into()),
                Box::new(move |info| p.lock().unwrap().push(format!("print: {info}"))),
                move |lines| {
                    for line in lines {
                        l.lock().unwrap().push(format!("left: {line}"));
                    }
                },
            ));
            Installed {
                saved: Some(saved),
                _turn: turn,
                events,
                router,
            }
        }

        /// The kinds of output, in order, for the panics that said `marker`, and every restore (only this test's
        /// thread, or a fatal build's, can restore through this router).
        fn log(&self, marker: &str) -> Vec<String> {
            let log = self.events.lock().unwrap();
            log.iter()
                .filter(|e| e.as_str() == "restore" || e.contains(marker))
                .map(|e| e.split([' ', ':']).next().unwrap_or_default().to_string())
                .collect()
        }
    }

    impl Drop for Installed {
        fn drop(&mut self) {
            // The hook cannot be changed from a thread that is panicking: a failed test leaves
            // its own hook, which only records, for the rest of the run.
            if std::thread::panicking() {
                return;
            }
            let _ = std::panic::take_hook();
            if let Some(saved) = self.saved.take() {
                std::panic::set_hook(saved);
            }
        }
    }

    /// The router's background list, with only the entries that carry `marker`.
    fn kept(router: &PanicRouter, marker: &str) -> Vec<String> {
        router
            .take()
            .into_iter()
            .filter(|e| e.contains(marker))
            .collect()
    }

    /// A task the runtime catches the panic of, while this thread holds the terminal, leaves the
    /// terminal alone and prints nothing; the run goes on and the loop says it in a toast.
    #[test]
    fn a_panic_in_a_spawned_task_leaves_the_terminal_alone() {
        let hook = Installed::new();
        hook.router.claim();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("svolder-tokio")
            .build()
            .expect("a runtime");
        let ended = runtime.block_on(async {
            tokio::spawn(async { panic!("remove_match failed") })
                .await
                .is_err()
        });
        assert!(ended, "the runtime caught the panic");
        assert!(
            hook.log("remove_match failed").is_empty(),
            "no restore, nothing printed: {:?}",
            hook.log("remove_match failed")
        );
        // The run goes on: the runtime still runs tasks, and the loop puts the words in a toast.
        assert_eq!(
            runtime
                .block_on(async { tokio::spawn(async { 7 }).await })
                .ok(),
            Some(7)
        );
        let mut app = test_support::app_showing(Screen::Device);
        hook.router.say_to(&mut app);
        let toast = app.toast().expect("the panic is told");
        assert!(toast.text.contains("remove_match failed"), "{}", toast.text);
        assert!(toast.text.contains("svolder-tokio"), "{}", toast.text);
        assert!(
            kept(hook.router, "remove_match failed").is_empty(),
            "told once"
        );
    }

    /// A panic on the thread that draws ends the run: the terminal is restored first, then the
    /// earlier hook prints.
    #[test]
    fn a_panic_on_the_drawing_thread_restores_the_terminal_then_prints() {
        let hook = Installed::new();
        hook.router.claim();
        let caught = std::panic::catch_unwind(|| panic!("a draw went wrong"));
        assert!(caught.is_err());
        assert_eq!(
            hook.log("a draw went wrong"),
            ["restore", "print"],
            "restored first, then printed"
        );
        assert!(
            kept(hook.router, "a draw went wrong").is_empty(),
            "not told as a background one"
        );
    }

    /// With no terminal held, every panic goes to the earlier hook alone.
    #[test]
    fn a_panic_with_no_terminal_held_is_the_earlier_hooks() {
        let hook = Installed::new();
        hook.router.claim();
        hook.router.release();
        let _ = std::panic::catch_unwind(|| panic!("nothing held"));
        assert_eq!(hook.log("nothing held"), ["print"]);
    }

    /// Panics other threads made that were never told are printed once the run ends, after the
    /// terminal is back and the panic that ended it, so a cause that only poisoned a lock or a
    /// location a later panic hides is not lost.
    #[test]
    fn a_background_panic_not_yet_told_is_printed_when_the_run_ends() {
        let hook = Installed::new();
        hook.router.claim();
        std::thread::Builder::new()
            .name("svolder-blocking".into())
            .spawn(|| panic!("the first, in a blocking job"))
            .expect("a thread")
            .join()
            .expect_err("the job panicked");
        let _ = std::panic::catch_unwind(|| panic!("the second, on the drawing thread"));
        let log = hook.events.lock().unwrap().clone();
        // the entries this test's own panics made: another test's panic may land in between
        let at = |kind: &str, marker: &str| {
            log.iter()
                .position(|e| e.contains(kind) && e.contains(marker))
                .unwrap_or_else(|| panic!("{kind}{marker} missing from {log:?}"))
        };
        let restore = log
            .iter()
            .position(|e| e.contains("restore"))
            .unwrap_or_else(|| panic!("restore missing from {log:?}"));
        let print = at("print: ", "the second, on the drawing thread");
        let left = at("left: ", "the first, in a blocking job");
        assert!(restore < print && print < left, "{log:?}");
    }

    /// Where no panic is survived (`panic = "abort"`), a panic on any thread ends the run: it
    /// restores and prints, as the drawing thread's does. Where one is, it only waits to be told.
    #[test]
    fn where_every_panic_is_fatal_a_background_one_restores_too() {
        let router = PanicRouter::new();
        router.claim();
        let other = std::thread::spawn(|| std::thread::current().id())
            .join()
            .expect("a thread");
        let calls = std::cell::RefCell::new(Vec::new());
        let ended = router.route(
            other,
            true,
            || "said".to_string(),
            || calls.borrow_mut().push("restore"),
            || calls.borrow_mut().push("print"),
        );
        assert!(ended);
        assert_eq!(*calls.borrow(), ["restore", "print"]);
        let ended = router.route(
            other,
            false,
            || "said".to_string(),
            || calls.borrow_mut().push("restore"),
            || calls.borrow_mut().push("print"),
        );
        assert!(!ended, "a survivable one is only kept");
        assert_eq!(calls.borrow().len(), 2, "and touches nothing");
        assert_eq!(router.take(), ["said"]);
    }

    /// The terminal is claimed for the thread that opens it, before it opens (a panic while it
    /// opens restores), and let go when it cannot be opened, so the wizard's plain prompts do
    /// not swallow later panics of other threads.
    #[test]
    fn opening_the_terminal_claims_it_and_a_failed_open_lets_go() {
        let holds = |router: &PanicRouter| {
            router.route(
                std::thread::current().id(),
                false,
                String::new,
                || {},
                || {},
            )
        };
        let router = PanicRouter::new();
        let opened = router.hold(|| Ok(1)).expect("opened");
        assert_eq!(opened, 1);
        assert!(holds(&router), "claimed after a good open");
        let router = PanicRouter::new();
        let claimed_during = std::cell::Cell::new(false);
        let failed: std::io::Result<()> = router.hold(|| {
            claimed_during.set(holds(&router));
            Err(std::io::Error::other("no tty"))
        });
        assert!(failed.is_err());
        assert!(claimed_during.get(), "claimed while it opens");
        assert!(!holds(&router), "let go after a failed open");
    }

    /// What was left is written one line each.
    #[test]
    fn left_panics_are_written_a_line_each() {
        let mut out = Vec::new();
        print_left(&["one".to_string(), "two".to_string()], &mut out);
        assert_eq!(String::from_utf8(out).expect("text"), "one\ntwo\n");
    }
}
