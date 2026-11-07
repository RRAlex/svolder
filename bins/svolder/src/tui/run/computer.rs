// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's controls as the loop reaches them.
//!
//! Joined to a daemon, every change is a request to it, and its answer says if it refused; the
//! view comes back on the update feed. With no daemon the loop owns a controller of its own
//! over the process's one host, ticked every 50 ms, and its view comes back on its watch
//! channel. Either way the changes run one at a time, in the order they were asked for, so the
//! last value of a drag is the last one that lands. Making the link writes nothing.

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use svolder_host::eq::{Edit, EqControl, EqFailure, EqView};
use svolder_ipc::{HostEqOp, Request, Response};
use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
use tokio::sync::{Mutex, mpsc, watch};

use super::host::Aborting;
use super::*;

/// How often the loop's own controller is ticked: the period its live updates are counted in.
const TICK: Duration = Duration::from_millis(50);
/// How many ticks apart the controller reads its state file again: about a second.
const RELOAD_EVERY: u64 = 20;
/// What a daemon that did not answer is reported as.
const UNANSWERED: &str = "the daemon did not answer";

/// What a change the computer was asked for ended with, when it is not simply done.
#[derive(Debug, PartialEq)]
pub(in crate::tui) enum Answer {
    /// It was refused, in these words.
    Said(String),
    /// The computer EQ is off, as a move of its preset to the DAC asked: the DAC's load can go out.
    Moved(String),
    /// A connected device plays the preset too: the question to ask before flattening it.
    Asked {
        /// The preset.
        name: String,
        /// The daemon's words.
        message: String,
        /// The devices that play it.
        devices: Vec<String>,
    },
}

/// One change on its way.
type Work = BoxFuture<'static, Option<Answer>>;

/// The most changes that wait their turn: past it the oldest live value is dropped.
const QUEUE_MAX: usize = 64;

/// The point a change names: by its slot when the change carries one (the point is its own for
/// as long as it exists), else by the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Named {
    Slot(u8),
    Index(u8),
}

/// What a queued change is, for the ones a newer change supersedes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    /// A live value of this point, which a newer value of it replaces while it waits.
    Live(Named),
    /// The last value of this point's drag: always kept, and the live values before it go.
    Last(Named),
    /// Anything else.
    Other,
}

fn named(index: u8, slot: Option<u8>) -> Named {
    slot.map_or(Named::Index(index), Named::Slot)
}

/// What `op` is to the queue.
fn tag_of(op: &HostEqOp) -> Tag {
    match op {
        HostEqOp::Set {
            index,
            slot,
            done: false,
            ..
        } => Tag::Live(named(*index, *slot)),
        HostEqOp::Set {
            index,
            slot,
            done: true,
            ..
        } => Tag::Last(named(*index, *slot)),
        _ => Tag::Other,
    }
}

/// The changes waiting, oldest first, and what wakes the task that runs them.
#[derive(Default)]
struct Waiting {
    items: std::sync::Mutex<std::collections::VecDeque<(Tag, Work)>>,
    wake: tokio::sync::Notify,
}

/// The computer's controls, as the loop reaches them.
#[derive(Default)]
pub(in crate::tui) struct ComputerLink {
    /// The loop's own controller, with no daemon.
    control: Option<Arc<Mutex<EqControl>>>,
    /// What ticks it.
    _tick: Option<Aborting<()>>,
    /// Its view, as it changes.
    pub(super) views: Option<watch::Receiver<EqView>>,
    /// Where the changes wait their turn.
    queue: Option<Arc<Waiting>>,
    /// The task that runs them.
    _worker: Option<Aborting<()>>,
    /// What they ended with.
    pub(super) answers: Option<mpsc::UnboundedReceiver<Answer>>,
    /// Why there is no controller of the loop's own, where there is none to make.
    pub(super) why: Option<String>,
}

impl ComputerLink {
    /// A link that reaches nothing.
    pub(in crate::tui) fn none() -> ComputerLink {
        ComputerLink::default()
    }

    /// The loop's own controller over `control`, ticked every 50 ms until the link is dropped.
    /// Made inside the runtime the loop runs on.
    pub(in crate::tui) fn over(control: EqControl) -> ComputerLink {
        let views = control.watcher();
        let control = Arc::new(Mutex::new(control));
        let ticking = Arc::clone(&control);
        let tick = tokio::spawn(async move {
            // Once, a read only: a saved state that says on while no chain runs is shown as off.
            ticking.lock().await.sync_running().await;
            let mut every = tokio::time::interval(TICK);
            every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut ticks = 0u64;
            loop {
                every.tick().await;
                ticks += 1;
                let mut control = ticking.lock().await;
                control.tick().await;
                if ticks.is_multiple_of(RELOAD_EVERY) {
                    control.reload_if_changed();
                }
            }
        });
        ComputerLink {
            control: Some(control),
            _tick: Some(Aborting(tick)),
            views: Some(views),
            ..ComputerLink::default()
        }
    }

    /// This machine's computer EQ for a session with no daemon; nothing at all under
    /// `SVOLDER_HOST=off`, and a reason where `pw-cli` is not there.
    pub(in crate::tui) fn live() -> ComputerLink {
        match crate::commands::host_eq::session_control() {
            Ok(control) => ComputerLink::over(control),
            Err(why) => ComputerLink {
                why: Some(why),
                ..ComputerLink::default()
            },
        }
    }

    /// The controller's view now, for the app to start from.
    pub(in crate::tui) fn view(&self) -> Option<EqView> {
        self.views.as_ref().map(|views| views.borrow().clone())
    }

    /// Change the loop's own controller as `op` says, after the changes before it; why not, where
    /// there is no controller.
    pub(in crate::tui) fn direct(&mut self, op: HostEqOp) -> Result<(), String> {
        let control = self.control.clone().ok_or_else(|| {
            self.why
                .clone()
                .unwrap_or_else(|| "computer controls unavailable".to_string())
        })?;
        let tag = tag_of(&op);
        self.queue(Box::pin(async move { run(control, op).await }), tag);
        Ok(())
    }

    /// Run `work` after the changes before it. A live value of a point replaces the one before
    /// it that still waits, the last value of a drag drops the live ones before it, and the queue
    /// is bounded by dropping the oldest live values: a daemon that does not answer holds up the
    /// one change in flight, never a backlog in front of a drag's last value.
    fn queue(&mut self, work: Work, tag: Tag) {
        let queue = self.queue.get_or_insert_with(|| {
            let waiting = Arc::new(Waiting::default());
            let (told, answers) = mpsc::unbounded_channel();
            self.answers = Some(answers);
            let run = Arc::clone(&waiting);
            self._worker = Some(Aborting(tokio::spawn(async move {
                loop {
                    let next = run.items.lock().expect("the queue").pop_front();
                    match next {
                        Some((_, work)) => {
                            if let Some(answer) = work.await
                                && told.send(answer).is_err()
                            {
                                return;
                            }
                        }
                        None => run.wake.notified().await,
                    }
                }
            })));
            waiting
        });
        {
            let mut items = queue.items.lock().expect("the queue");
            if let Tag::Live(point) | Tag::Last(point) = tag {
                items.retain(|(held, _)| *held != Tag::Live(point));
            }
            while items.len() >= QUEUE_MAX {
                match items
                    .iter()
                    .position(|(held, _)| matches!(held, Tag::Live(_)))
                {
                    Some(at) => {
                        items.remove(at);
                    }
                    None => break,
                }
            }
            items.push_back((tag, work));
        }
        queue.wake.notify_one();
    }

    /// Take the computer EQ off, then say so: the DAC's load of the preset it held goes after.
    pub(in crate::tui) fn move_off(
        &mut self,
        name: String,
        joined: Option<BoxFuture<'static, Option<Response>>>,
    ) -> Result<(), String> {
        let off = HostEqOp::Mode { on: false };
        let work: Work = match joined {
            Some(ask) => Box::pin(async move {
                match answer_of(ask.await) {
                    None => Some(Answer::Moved(name)),
                    refused => refused,
                }
            }),
            None => {
                let control = self.control.clone().ok_or_else(|| {
                    self.why
                        .clone()
                        .unwrap_or_else(|| "computer controls unavailable".to_string())
                })?;
                Box::pin(async move {
                    match run(control, off).await {
                        None => Some(Answer::Moved(name)),
                        refused => refused,
                    }
                })
            }
        };
        self.queue(work, Tag::Other);
        Ok(())
    }
}

/// Carry out `op`: a request to the daemon this loop is joined to, or a change of the loop's own
/// controller; with neither, the toast says why.
pub(super) fn eq(watching: &mut Watching, op: HostEqOp) {
    let joined = watching
        .backend
        .as_ref()
        .and_then(|backend| backend.daemon_request(Request::HostEq { op: op.clone() }));
    // Anything but the apply that follows a flatten ends that flatten's advice.
    if !matches!(op, HostEqOp::Apply { .. }) {
        watching.app.presets.computer_flattened = None;
    }
    if let Some(ask) = joined {
        let tag = tag_of(&op);
        watching
            .computer
            .queue(Box::pin(async move { answer_of(ask.await) }), tag);
        return;
    }
    if let Err(why) = watching.computer.direct(op) {
        watching.app.warn(why);
    }
}

/// Set the computer's volume to `pct`: a request to the daemon when joined to one, else the
/// loop's own sink. The volume asked for stands until it is read back.
pub(super) fn volume(watching: &mut Watching, pct: u8) {
    let joined = watching
        .backend
        .as_ref()
        .and_then(|backend| backend.daemon_request(Request::HostVolume { pct }));
    match joined {
        Some(ask) => {
            watching.app.host.asked = Some(pct);
            watching
                .computer
                .queue(Box::pin(async move { answer_of(ask.await) }), Tag::Other);
        }
        None => {
            if watching.host.send(super::host::SinkOp::Volume(pct)) {
                watching.app.host.asked = Some(pct);
            }
        }
    }
}

/// Mute the computer, or unmute it, the other way from how it reads now.
pub(super) fn mute(watching: &mut Watching) {
    let Some(volume) = watching.app.host.volume() else {
        return;
    };
    let joined = watching.backend.as_ref().and_then(|backend| {
        backend.daemon_request(Request::HostMute {
            muted: !volume.muted,
        })
    });
    match joined {
        Some(ask) => watching
            .computer
            .queue(Box::pin(async move { answer_of(ask.await) }), Tag::Other),
        None => {
            watching.host.send(super::host::SinkOp::ToggleMute);
        }
    }
}

/// Move the preset `name` from the computer to the DAC: the computer EQ goes off first, and the
/// answer that says so is what sends the DAC's load.
pub(super) fn move_to_dac(watching: &mut Watching, name: String) {
    let joined = watching
        .backend
        .as_ref()
        .and_then(|backend| {
            backend.daemon_request(Request::HostEq {
                op: HostEqOp::Mode { on: false },
            })
        })
        .map(|ask| Box::pin(ask) as BoxFuture<'static, Option<Response>>);
    if let Err(why) = watching.computer.move_off(name, joined) {
        watching.app.warn(why);
    }
}

/// What the daemon's reply to a change is, when it is not an acknowledgement.
fn answer_of(response: Option<Response>) -> Option<Answer> {
    match response {
        Some(Response::Ack) => None,
        Some(Response::Ask {
            message,
            name,
            devices,
        }) => Some(Answer::Asked {
            name,
            message,
            devices,
        }),
        Some(Response::Error { message, .. }) => Some(Answer::Said(message)),
        Some(_) | None => Some(Answer::Said(UNANSWERED.to_string())),
    }
}

/// A point's index from the wire, which counts from 1.
fn zero_based(index: u8) -> Result<usize, EqFailure> {
    usize::from(index).checked_sub(1).ok_or_else(|| EqFailure {
        what: "points count from 1".to_string(),
        next: "Choose a point that is there".to_string(),
    })
}

/// The 0-based index a change means: with a slot the controller finds the point by the slot
/// and the index is not looked at, else it counts from 1.
fn index_for(index: u8, slot: Option<u8>) -> Result<usize, EqFailure> {
    match slot {
        Some(_) => Ok(usize::from(index.max(1)) - 1),
        None => zero_based(index),
    }
}

/// A point's type from its word.
fn kind_of(word: &str) -> Result<FilterKind, EqFailure> {
    word.parse().map_err(|_| EqFailure {
        what: format!("{word:?} is not a point type"),
        next: "Use PK, LSC or HSC".to_string(),
    })
}

/// Change the loop's own controller as `op` says.
async fn run(control: Arc<Mutex<EqControl>>, op: HostEqOp) -> Option<Answer> {
    let result = change(&mut *control.lock().await, op).await;
    result.err().map(|why| Answer::Said(why.to_string()))
}

async fn change(control: &mut EqControl, op: HostEqOp) -> Result<(), EqFailure> {
    match op {
        HostEqOp::Mode { on } => control.set_on(on).await,
        HostEqOp::Apply {
            name,
            origin,
            correction,
            ..
        } => control.load(name, origin, correction).await,
        HostEqOp::Set {
            index,
            slot,
            kind,
            f,
            g,
            q,
            done,
        } => {
            let kind = kind.as_deref().map(kind_of).transpose()?;
            let index = index_for(index, slot)?;
            control
                .edit(Edit::Set {
                    index,
                    slot,
                    kind,
                    f,
                    g,
                    q,
                    done,
                })
                .await
        }
        HostEqOp::Add { kind, f, g, q } => {
            let kind = kind_of(&kind)?;
            control
                .edit(Edit::Add(SourceFilter { kind, f, g, q }))
                .await
        }
        HostEqOp::Remove { index, slot } => {
            let index = index_for(index, slot)?;
            control.edit(Edit::Remove { index, slot }).await
        }
        HostEqOp::Preset { .. } | HostEqOp::SaveAs { .. } => Err(EqFailure {
            what: "this session keeps no presets of its own for the computer".to_string(),
            next: "Recall it from the preset list".to_string(),
        }),
    }
}

/// What the changes ended with next; never while none is in flight.
pub(super) async fn answered(answers: &mut Option<mpsc::UnboundedReceiver<Answer>>) -> Box<Answer> {
    match answers {
        Some(answers) => match answers.recv().await {
            Some(answer) => Box::new(answer),
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// The controller's next view; never with no controller.
pub(super) async fn next_view(views: &mut Option<watch::Receiver<EqView>>) -> Box<EqView> {
    match views {
        Some(views) => match views.changed().await {
            Ok(()) => Box::new(views.borrow_and_update().clone()),
            Err(_) => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
