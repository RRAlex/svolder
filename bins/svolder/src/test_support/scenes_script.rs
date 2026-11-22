// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes family as a daemon a case scripts: the stub daemon's `scene` requests and the
//! `scenes` event every subscription opens with.
//!
//! A case says which view the daemon shows and what each request that follows is answered with,
//! and reads back the ops that arrived. Nothing here keeps a scene: it is what a front end's
//! side of the socket is tested against.

use std::collections::VecDeque;
use std::sync::Mutex;

use svolder_ipc::{CODE_USAGE, Event, Response, SceneOp, ScenesView};

/// What the scripted scenes family has been asked and what it will say.
#[derive(Default)]
struct Inner {
    /// The view each subscription opens with; none sends no `scenes` event.
    view: Option<ScenesView>,
    /// The answers the next requests get, in order.
    replies: VecDeque<Response>,
    /// The ops that arrived, in order.
    ops: Vec<SceneOp>,
}

/// The scenes family of the stub daemon.
#[derive(Default)]
pub(crate) struct ScenesScript {
    inner: Mutex<Inner>,
}

impl ScenesScript {
    /// Show `view` to every subscription from now on.
    pub(crate) fn shows(&self, view: ScenesView) {
        self.inner.lock().expect("the script").view = Some(view);
    }

    /// Answer the next request that has no answer queued before it with `reply`.
    pub(crate) fn answers(&self, reply: Response) {
        self.inner
            .lock()
            .expect("the script")
            .replies
            .push_back(reply);
    }

    /// The ops that have arrived, in order.
    pub(crate) fn ops(&self) -> Vec<SceneOp> {
        self.inner.lock().expect("the script").ops.clone()
    }

    /// The `scenes` event a subscription opens with, when a case gave a view.
    pub(super) fn opening(&self) -> Option<Event> {
        self.inner
            .lock()
            .expect("the script")
            .view
            .clone()
            .map(Event::Scenes)
    }

    /// Record `op` and give the next queued answer; with none queued, a refusal that says so.
    pub(super) fn handle(&self, op: SceneOp) -> Response {
        let mut inner = self.inner.lock().expect("the script");
        inner.ops.push(op);
        inner.replies.pop_front().unwrap_or_else(|| {
            Response::error(CODE_USAGE, "the stub daemon has no scripted scene answer")
        })
    }
}
