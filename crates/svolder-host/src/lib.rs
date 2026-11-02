// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host's own audio, as svolder's TUI and its daemon reach it: the sink feeding the DAC, the
//! media players on the session bus, and what the sink plays, drawn as a spectrum and a level or
//! analysed band by band.
//! The daemon's host service follows the player, reads its cover and reads the route for every
//! front end; the TUI reaches the rest itself.
//!
//! Each source sits behind a trait so the tests of what uses them can put a fake in its place
//! (see [`fake`]), and each real one goes on quietly without what it talks to: a missing tool, a
//! missing bus or a player that went away leaves an absent answer, never an error to handle.
//!
//! - [`sink`]: the default audio sink's volume, mute and name, through `wpctl`.
//! - [`players`]: the player being followed and what it plays, over MPRIS on the session bus.
//! - [`capture`]: the sink's monitor, read through `pw-record`, as blocks of bands and levels or
//!   as analysed frames.
//! - [`analysis`]: the frames: 240 bands per channel, levels, beats and tempo.
//! - [`cover`]: the followed player's cover, read from a local file or Spotify's own cache.
//! - [`mod@route`]: the default sink, the player's stream and what sits between, from `pw-dump`.
//! - [`fft`]: the transform and the scales the captured blocks are drawn on.
//! - [`eq`]: the computer EQ's PipeWire config, unit, autostart entry and state file, as text.
//! - [`fake`]: stand-ins for the sources, for tests.
//!
//! Nothing here links PipeWire: the sink and the capture run PipeWire's own command-line
//! tools, and MPRIS goes through the same D-Bus bindings the rest of the workspace uses.
//! [`HOST_ENV`] set to `off` turns every part off for the process.
#![warn(missing_docs)]

use std::ffi::OsStr;
use std::sync::{Mutex, MutexGuard, PoisonError};

pub mod analysis;
pub mod capture;
pub mod cover;
pub mod eq;
pub mod fake;
pub mod fft;
pub mod players;
pub mod route;
pub mod sink;

pub use analysis::{Analyser, Frame};
pub use capture::{Block, Capture, FrameCapture, PwFrames, PwRecord, pw_record_args};
pub use players::{Button, Mpris, NoBus, NowPlaying, Players, Progress, Status, unix_ms};
pub use route::{PlayerMatch, PwDump, RouteFacts, SinkFacts, StreamFacts, route};
pub use sink::{Sink, SinkState, Wpctl};

/// The environment variable that turns every host part off for the process when it reads
/// `off`, whatever the config says.
pub const HOST_ENV: &str = "SVOLDER_HOST";

/// Whether [`HOST_ENV`] turns host audio off for this process.
///
/// The constructors that read the environment ([`Wpctl::from_env`], [`Mpris::session`],
/// [`PwRecord::from_env`]) ask this themselves and give nothing back when it says so, so a
/// process run with `SVOLDER_HOST=off` never reaches the host's tools or its session bus.
pub fn host_off() -> bool {
    is_off(std::env::var_os(HOST_ENV).as_deref())
}

/// Whether a value of [`HOST_ENV`] turns host audio off: `off`, in any case and with any
/// surrounding blanks. Unset, empty or anything else leaves it on.
pub fn is_off(value: Option<&OsStr>) -> bool {
    value
        .and_then(OsStr::to_str)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("off"))
}

/// `mutex` locked, whatever a thread that panicked while holding it left behind: what the
/// crate keeps behind a lock is a name or a note it overwrites whole, so a panic elsewhere
/// never makes a caller panic too.
pub(crate) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_in_any_case_turns_host_audio_off() {
        for value in ["off", "OFF", "Off", " off\n"] {
            assert!(is_off(Some(OsStr::new(value))), "{value:?}");
        }
    }

    /// A lock a panicking thread left poisoned is still taken, with what it held.
    #[test]
    fn a_poisoned_lock_is_taken_all_the_same() {
        let mutex = std::sync::Arc::new(Mutex::new(Some("hdmi-out".to_string())));
        let held = std::sync::Arc::clone(&mutex);
        let panicked = std::thread::spawn(move || {
            let _guard = held.lock().expect("the lock is free");
            panic!("a panic while holding the lock");
        })
        .join();
        assert!(
            panicked.is_err() && mutex.is_poisoned(),
            "the lock was not poisoned"
        );
        assert_eq!(locked(&mutex).as_deref(), Some("hdmi-out"));
        *locked(&mutex) = None;
        assert_eq!(*locked(&mutex), None);
    }

    #[test]
    fn unset_empty_or_anything_else_leaves_it_on() {
        assert!(!is_off(None));
        for value in ["", "on", "0", "false", "offline", "no"] {
            assert!(!is_off(Some(OsStr::new(value))), "{value:?}");
        }
    }
}
