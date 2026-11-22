// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes the daemon keeps: one [`ScenesView`] for every front end, and save, recall, rename
//! and delete as `Request::Scene`.
//!
//! A scene is a snapshot of sound settings in the `[[scene]]` blocks of `config.toml`: the
//! computer's part and one part for each device that was connected when it was saved. The daemon
//! judges everything a front end shows: which scenes there are, which parts of each can be
//! recalled now, and which scene is on (the one `svolder_ipc::scenes::on` names).
//!
//! - `memory`: the scene last recalled, in memory and in `scenes.toml` in the state home, so the
//!   tie rule of "which scene is on" holds across a restart.
//! - `recall`: plan every part, ask the both-sides question, then change the computer and every
//!   device at once, each by its own safe order, and say how each part fared.
//! - `save`: capture the computer and every connected device into a new or an existing scene.
//! - `upkeep`: rename and delete.
//! - `view`: the [`ScenesView`] from what the daemon holds now.
//! - `words`: what a refusal or a note says.
//!
//! The view is a `watch` channel the daemon owns. A task started with the first reader recomputes
//! it when a device's feed changes, the computer's view changes, or the daemon says the config or
//! the device set changed ([`Scenes::poke`]), and sends only a value that differs.

mod memory;
mod recall;
mod save;
mod upkeep;
mod view;
mod words;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use svolder_ipc::socket::protocol::ScenesView;
use svolder_profiles::tone::preset::same_name;
use tokio::sync::{Notify, watch};

use crate::handler::Daemon;
use crate::locks::locked;

pub(crate) use recall::recall;
pub(crate) use save::save;
pub(crate) use upkeep::{delete, rename};

/// The scenes service the daemon holds.
pub(crate) struct Scenes {
    /// The view every reader follows.
    view: watch::Sender<ScenesView>,
    /// The scene last recalled, and the file it is kept in.
    memory: Mutex<memory::Memory>,
    /// Held while the memory changes and its file is written, so two changes land in order.
    keep: tokio::sync::Mutex<()>,
    /// Held by a recall from its arrival until its writes are in the writers' lines, so recalls
    /// reach the devices in the order they arrived, whatever each waits for before.
    turn: tokio::sync::Mutex<()>,
    /// Woken when the config or the device set changed.
    poke: Arc<Notify>,
    /// Whether the task that follows the daemon runs.
    following: AtomicBool,
}

impl Scenes {
    /// A service with no scene recalled and no file to keep the memory in.
    pub(crate) fn new() -> Scenes {
        Scenes {
            view: watch::channel(ScenesView {
                scenes: Vec::new(),
                on: None,
                recalled: None,
                known: true,
            })
            .0,
            memory: Mutex::new(memory::Memory::default()),
            turn: tokio::sync::Mutex::new(()),
            keep: tokio::sync::Mutex::new(()),
            poke: Arc::new(Notify::new()),
            following: AtomicBool::new(false),
        }
    }

    /// The service keeping its memory in the file at `path`, and reading the scene recalled
    /// before the last stop from it. A file that is missing, unreadable or garbled is no memory.
    pub(crate) fn with_file(self, path: PathBuf) -> Scenes {
        let recalled = memory::read(&path);
        *locked(&self.memory) = memory::Memory {
            recalled,
            path: Some(path),
        };
        self
    }

    /// The default place of the memory's file: `scenes.toml` in svolder's state home.
    pub(crate) fn state_path() -> PathBuf {
        svolder_ipc::config::state_home()
            .join("svolder")
            .join("scenes.toml")
    }

    /// The scene last recalled, as the file spells its name.
    pub(crate) fn recalled(&self) -> Option<String> {
        locked(&self.memory).recalled.clone()
    }

    /// A recall's place in the line: granted in the order it is asked for.
    pub(crate) async fn take_turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.turn.lock().await
    }

    /// Note `name` as the scene last recalled (`None` forgets it), in the process at once; the
    /// file follows with [`Scenes::flush`].
    pub(crate) fn set_recalled(&self, name: Option<String>) {
        locked(&self.memory).recalled = name;
    }

    /// The memory follows the scene `old` being renamed `new`, when it is the one recalled; the
    /// comparison and the change are one step under the memory's lock, so a recall that lands
    /// beside it is never overwritten. Whether the memory changed (its file then wants a
    /// [`Scenes::flush`]).
    pub(crate) fn renamed(&self, old: &str, new: &str) -> bool {
        let mut memory = locked(&self.memory);
        match memory.recalled.as_deref() {
            Some(recalled) if same_name(recalled, old) => {
                memory.recalled = Some(new.to_string());
                true
            }
            _ => false,
        }
    }

    /// The memory forgets the scene `gone`, when it is the one recalled, as [`Scenes::renamed`]
    /// changes it: one step under the lock. Whether it changed.
    pub(crate) fn forgotten(&self, gone: &str) -> bool {
        let mut memory = locked(&self.memory);
        match memory.recalled.as_deref() {
            Some(recalled) if same_name(recalled, gone) => {
                memory.recalled = None;
                true
            }
            _ => false,
        }
    }

    /// Keep the memory as it is now in its file. Two flushes write in order, each the memory as
    /// it stands when its turn comes, so the file ends as the memory does. A file that cannot be
    /// written is logged: the memory still holds until the daemon stops.
    pub(crate) async fn flush(&self) {
        let _keep = self.keep.lock().await;
        let (path, name) = {
            let memory = locked(&self.memory);
            (memory.path.clone(), memory.recalled.clone())
        };
        if let Some(path) = path {
            memory::keep(path, name).await;
        }
    }

    /// Say that the config or the set of devices changed, so the view is worked out again.
    pub(crate) fn poke(&self) {
        self.poke.notify_one();
    }

    /// Say that a scene was saved, recalled, renamed or deleted: the view is worked out at once,
    /// for a reader that is waiting on it, and again when the daemon's task takes the poke.
    pub(crate) fn changed(&self, daemon: &Daemon) {
        self.refresh(daemon);
        self.poke();
    }

    /// The view as it is now, with every later change; the first reader starts the task that
    /// follows the daemon.
    pub(crate) fn watch(&self, daemon: &Daemon) -> watch::Receiver<ScenesView> {
        self.follow(daemon);
        self.refresh(daemon);
        self.view.subscribe()
    }

    /// Work the view out from what the daemon holds and send it if it differs.
    pub(crate) fn refresh(&self, daemon: &Daemon) {
        let next = view::of(
            &daemon.config_snapshot(),
            &daemon.devices(),
            daemon.host().scene_eq().as_ref(),
            self.recalled().as_deref(),
        );
        self.view.send_if_modified(|shown| {
            let changed = *shown != next;
            if changed {
                *shown = next;
            }
            changed
        });
    }

    /// Start the task that follows the daemon, once, when it runs behind a shared handle in a
    /// runtime; a daemon that does not (a test's) is read through [`Scenes::watch`] alone.
    fn follow(&self, daemon: &Daemon) {
        let (Some(me), Ok(runtime)) = (
            daemon.shared_handle(),
            tokio::runtime::Handle::try_current(),
        ) else {
            return;
        };
        if self.following.swap(true, Ordering::SeqCst) {
            return;
        }
        runtime.spawn(follow(Arc::downgrade(&me), Arc::clone(&self.poke)));
    }
}

/// Recompute the view each time something it is made of changes, until the daemon is gone or
/// stops.
async fn follow(daemon: std::sync::Weak<Daemon>, poke: Arc<Notify>) {
    loop {
        let Some(strong) = daemon.upgrade() else {
            return;
        };
        if strong.is_stopping() {
            return;
        }
        // Marked seen before the view is worked out, so a change after the look wakes the wait.
        let mut feeds: Vec<_> = strong.devices().iter().map(|d| d.feed.clone()).collect();
        for feed in &mut feeds {
            feed.borrow_and_update();
        }
        let mut host = strong.host().changes();
        host.borrow_and_update();
        strong.scenes().refresh(&strong);
        drop(strong);
        // A watch whose sender is gone (a device that was stopped) never wakes the wait: the
        // poke of the reload that dropped it does.
        let feeds_changed = async {
            let waits: Vec<_> = feeds
                .iter_mut()
                .map(|feed| {
                    Box::pin(async {
                        if feed.changed().await.is_err() {
                            std::future::pending::<()>().await;
                        }
                    })
                })
                .collect();
            match waits.is_empty() {
                true => std::future::pending::<()>().await,
                false => {
                    futures::future::select_all(waits).await;
                }
            }
        };
        let host_changed = async {
            if host.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            () = poke.notified() => {}
            () = host_changed => {}
            () = feeds_changed => {}
        }
    }
}

#[cfg(test)]
mod tests;
