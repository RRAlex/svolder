// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture's run: the helper that starts the capture of the default sink with the first
//! [`FramesHold`](super::FramesHold), keeps it on the sink the computer plays to, and stops it
//! 2 s after the last hold is let go.
//!
//! The helper belongs to the run that started it: halting the host ends it (the capture, and so
//! its `pw-record`, is dropped with it), and a step of a run that was halted publishes nothing
//! (see `Shared::current`). While nobody holds, it waits and does nothing: no sink is read, no
//! sample is taken, no frame is analysed. While somebody does, the sink is read (and only read)
//! every [`LOOK_EVERY`] to follow it, on a task of its own so a slow `wpctl` never holds up
//! the frames; a capture that cannot start, or ended on its own, is tried again at the next look.
//!
//! The capture's frames go to every holder on one broadcast channel, so a page that reads slowly
//! loses the frames it was too late for and nothing else.

use std::sync::Arc;
use std::time::Duration;

use svolder_host::SinkState;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::{CaptureState, Frames, Shared};

/// How long the capture goes on after the last hold is let go, so a page that is reloaded or
/// switches tab and back does not start it twice.
pub(super) const LINGER: Duration = Duration::from_secs(2);

/// How often the default sink is read while capturing.
pub(super) const LOOK_EVERY: Duration = Duration::from_secs(2);

/// A read of the sink under way, ended when dropped.
struct Looking(JoinHandle<Option<SinkState>>);

impl Drop for Looking {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The sink a read under way gives, once it is done; a read that failed or was ended gives
/// none. Waits for ever when there is no read.
async fn finished(looking: &mut Option<Looking>) -> Option<SinkState> {
    match looking {
        Some(Looking(read)) => read.await.ok().flatten(),
        None => std::future::pending().await,
    }
}

/// Wait until `at`, or for ever without one.
async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// How long a run has to last to count as a capture that worked: one that ends sooner, or
/// cannot start, backs the next try off.
const STABLE: Duration = Duration::from_secs(10);

/// The longest the next try is put off after capture failures in a row.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// How many reads of the sink in a row may fail before a running capture is given up.
const MISSES: u32 = 2;

/// How long after the `failures`th failure in a row the capture is tried again: the look's own
/// 2 s after the first, then 4 s, 8 s and so on up to [`BACKOFF_MAX`].
fn backoff(failures: u32) -> Duration {
    LOOK_EVERY
        .saturating_mul(1 << failures.saturating_sub(1).min(5))
        .min(BACKOFF_MAX)
}

/// Run the capture of `frames` for as long as the run of `generation` lasts. Everything it
/// publishes is published only while `generation` is the current one.
pub(super) async fn run(shared: Arc<Shared>, generation: u64, frames: Frames) {
    let Frames { mut capture, sink } = frames;
    let say = |state: CaptureState| {
        shared.current(generation, || {
            shared.capture.send_if_modified(|current| {
                let changed = *current != state;
                if changed {
                    *current = state;
                }
                changed
            });
        });
    };
    let mut holds = shared.holds.subscribe();
    // Whether the capture is wanted: from the first hold to 2 s after the last.
    let mut on = false;
    let mut linger: Option<Instant> = None;
    let mut next_look = Instant::now();
    let mut looking: Option<Looking> = None;
    let mut held = *holds.borrow_and_update() > 0;
    // Captures that failed in a row (could not start, or ended within STABLE), when the next
    // may be tried, and when the one running began.
    let mut failures = 0u32;
    let mut not_before = Instant::now();
    let mut began: Option<Instant> = None;
    // Reads of the sink that failed in a row.
    let mut misses = 0u32;
    loop {
        if held {
            linger = None;
            if !on {
                on = true;
                next_look = Instant::now();
            }
        } else if on && linger.is_none() {
            linger = Some(Instant::now() + LINGER);
        }
        tokio::select! {
            changed = holds.changed() => {
                if changed.is_err() {
                    return;
                }
                held = *holds.borrow_and_update() > 0;
            }
            () = sleep_until(linger), if linger.is_some() => {
                // A hold taken at the very moment the linger ran out keeps the capture.
                if *holds.borrow_and_update() > 0 {
                    held = true;
                    continue;
                }
                capture.stop();
                on = false;
                linger = None;
                looking = None;
                (failures, misses, began) = (0, 0, None);
                // A page that comes back is not made to wait out an old back-off.
                not_before = Instant::now();
                say(CaptureState::Starting);
            }
            () = tokio::time::sleep_until(next_look), if on && looking.is_none() => {
                let sink = Arc::clone(&sink);
                // The daemon only ever reads the sink (never its volume or mute).
                looking = Some(Looking(tokio::spawn(async move { sink.read().await })));
            }
            seen = finished(&mut looking), if looking.is_some() => {
                looking = None;
                next_look = Instant::now() + LOOK_EVERY;
                let running = capture.target().is_some();
                misses = if seen.is_some() { 0 } else { misses + 1 };
                if running && misses > 0 && misses < MISSES {
                    // One read that failed does not end a capture that works.
                    continue;
                }
                if !running && Instant::now() < not_before {
                    // The last capture failed lately: wait out the back-off.
                    next_look = not_before;
                    continue;
                }
                let (state, started) = follow(capture.as_mut(), seen);
                match started {
                    Started::Newly => began = Some(Instant::now()),
                    Started::Failed => {
                        // A capture that had run for STABLE before it was moved is a good run,
                        // whatever failed before it.
                        if began.take().is_some_and(|at| at.elapsed() >= STABLE) {
                            failures = 0;
                        }
                        failures += 1;
                        not_before = Instant::now() + backoff(failures);
                        next_look = not_before;
                    }
                    Started::Kept => {}
                }
                say(state);
            }
            frame = capture.next_frame(), if on => match frame {
                Some(frame) => {
                    shared.current(generation, || {
                        // No listener is no error: the next hold hears the next frame.
                        let _ = shared.frames.send(Arc::new(frame));
                    });
                }
                None => {
                    // A capture that lasted was fine until now; one that dies at once is not.
                    if began.is_some_and(|at| at.elapsed() >= STABLE) {
                        failures = 0;
                    }
                    failures += 1;
                    began = None;
                    not_before = Instant::now() + backoff(failures);
                    next_look = next_look.max(not_before);
                    say(CaptureState::Unavailable {
                        why: "the capture ended".to_string(),
                    });
                }
            },
        }
    }
}

/// What [`follow`] did to the capture.
enum Started {
    /// It was started now.
    Newly,
    /// It runs as it did.
    Kept,
    /// It could not be started, or there is nothing to start it on.
    Failed,
}

/// Put the capture on the sink `seen` names (it is left alone when it runs there already) and
/// say where that leaves it.
fn follow(
    capture: &mut dyn svolder_host::FrameCapture,
    seen: Option<SinkState>,
) -> (CaptureState, Started) {
    let (name, target) = match seen {
        Some(SinkState {
            name,
            target: Some(target),
            ..
        }) if !target.is_empty() => (name, target),
        other => {
            capture.stop();
            // A read that gave nothing (no `wpctl`, a bus that does not answer) is not a sink
            // with no node to capture.
            let why = if other.is_none() {
                "the default sink could not be read"
            } else {
                "there is no sink to capture"
            };
            let state = CaptureState::Unavailable {
                why: why.to_string(),
            };
            return (state, Started::Kept);
        }
    };
    let mut started = Started::Kept;
    if capture.target().as_deref() != Some(target.as_str()) {
        if !capture.start(&target) {
            let state = CaptureState::Unavailable {
                why: "pw-record could not be started".to_string(),
            };
            return (state, Started::Failed);
        }
        started = Started::Newly;
    }
    // The label is for people: the sink's description, else the node it is.
    let sink = match name.trim() {
        "" => target.clone(),
        named => named.to_string(),
    };
    (CaptureState::Running { sink, target }, started)
}

#[cfg(test)]
mod tests;
