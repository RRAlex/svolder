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
