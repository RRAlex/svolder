// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How often the intro and its exit draw a frame: sixty a second where the terminal keeps up,
//! thirty for the rest of the run once three frames in a row have taken too long, and thirty
//! from the start over SSH. Each frame is drawn at the instant it is due, so a drop costs
//! smoothness, never timing.

use std::ffi::OsString;
use std::time::Duration;

/// The period while the terminal keeps up: about sixty frames a second.
pub(in crate::tui) const FAST: Duration = Duration::from_millis(16);
/// The period once it does not, and over SSH: about thirty frames a second.
pub(in crate::tui) const SLOW: Duration = Duration::from_millis(33);
/// A frame whose draw and write take longer than this is a slow one.
const SLOW_FRAME: Duration = Duration::from_millis(12);
/// How many slow frames in a row drop the pace for good.
const STRIKES: u8 = 3;

/// The intro's frame pace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacer {
    /// How long one frame waits for the next.
    period: Duration,
    /// How many slow frames have come in a row.
    strikes: u8,
}

impl Pacer {
    /// A pacer at sixty frames a second, or at thirty for a `remote` run.
    pub fn new(remote: bool) -> Pacer {
        Pacer {
            period: if remote { SLOW } else { FAST },
            strikes: 0,
        }
    }

    /// The pacer this process runs at: remote when `SSH_CONNECTION` or `SSH_TTY` says so.
    pub fn for_session() -> Pacer {
        Pacer::new(remote(|key| std::env::var_os(key)))
    }

    /// How long one frame waits for the next.
    pub fn period(&self) -> Duration {
        self.period
    }

    /// Take what the frame just drawn cost, from the start of its draw to the end of its write.
    pub fn record(&mut self, cost: Duration) {
        if self.period == SLOW {
            return;
        }
        match cost > SLOW_FRAME {
            true => {
                self.strikes += 1;
                if self.strikes >= STRIKES {
                    self.period = SLOW;
                }
            }
            false => self.strikes = 0,
        }
    }
}

/// Whether `var` says this process runs over SSH: either variable set and not empty.
fn remote(var: impl Fn(&str) -> Option<OsString>) -> bool {
    ["SSH_CONNECTION", "SSH_TTY"]
        .into_iter()
        .any(|key| var(key).is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOW_ONE: Duration = Duration::from_millis(13);
    const QUICK_ONE: Duration = Duration::from_millis(5);

    #[test]
    fn a_local_run_starts_at_sixty_frames_and_a_remote_one_at_thirty() {
        assert_eq!(Pacer::new(false).period(), FAST);
        assert_eq!(Pacer::new(true).period(), SLOW);
    }

    #[test]
    fn three_slow_frames_in_a_row_drop_it_to_thirty_for_good() {
        let mut pacer = Pacer::new(false);
        pacer.record(SLOW_ONE);
        pacer.record(SLOW_ONE);
        assert_eq!(pacer.period(), FAST);
        pacer.record(SLOW_ONE);
        assert_eq!(pacer.period(), SLOW);
        for _ in 0..10 {
            pacer.record(QUICK_ONE);
        }
        assert_eq!(pacer.period(), SLOW, "it never climbs back");
    }

    #[test]
    fn a_quick_frame_between_slow_ones_starts_the_count_again() {
        let mut pacer = Pacer::new(false);
        for cost in [SLOW_ONE, SLOW_ONE, QUICK_ONE, SLOW_ONE, SLOW_ONE] {
            pacer.record(cost);
        }
        assert_eq!(pacer.period(), FAST);
    }

    #[test]
    fn either_ssh_variable_set_and_not_empty_makes_a_run_remote() {
        assert!(remote(|key| (key == "SSH_TTY").then(|| "/dev/pts/1".into())));
        assert!(remote(
            |key| (key == "SSH_CONNECTION").then(|| "10.0.0.2 5 10.0.0.1 22".into())
        ));
        assert!(!remote(|key| (key == "SSH_CONNECTION").then(OsString::new)));
        assert!(!remote(|_| None));
    }
}
