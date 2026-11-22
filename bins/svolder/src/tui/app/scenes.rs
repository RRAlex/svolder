// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scenes in the TUI: what the app does with the daemon's `scenes` event and with a request's
//! answer, and the key that opens the box. The box itself is
//! [`boxes::scenes`](crate::tui::boxes::scenes).

use svolder_ipc::socket::protocol::ScenesView;

use crate::tui::boxes::scenes::{self, SceneDone};

use super::{App, Intent};

impl App {
    /// Whether scenes are on offer: the daemon lists them and has sent its view. The Esc menu's
    /// Scenes row and the Status pane's `Scene` line are drawn only then.
    pub(crate) fn scenes_offered(&self) -> bool {
        self.scenes.offered()
    }

    /// What `z` does: open the box, or say why it cannot be opened.
    pub(super) fn open_scenes(&mut self) -> Vec<Intent> {
        scenes::open(self)
    }

    /// Take the view the daemon's feed carried.
    pub(crate) fn scenes_view(&mut self, view: ScenesView) {
        scenes::viewed(self, view);
    }

    /// Take what the daemon said to a scene request.
    pub(crate) fn scenes_done(&mut self, done: SceneDone) -> Vec<Intent> {
        scenes::done(self, done)
    }

    /// Note whether the daemon lists the scenes feature. A daemon that does not has no view to
    /// keep, and an open box closes.
    pub(crate) fn scenes_served(&mut self, served: bool) {
        self.scenes.served = Some(served);
        if !served {
            self.scenes.view = None;
            self.scenes.list = None;
            self.scenes.pending = None;
        }
        // The menu's cursor stays on a row when the Scenes row goes.
        let rows = crate::tui::screens::menu::entries(self).len();
        if let Some(at) = self.overlays.menu {
            self.overlays.menu = Some(at.min(rows.saturating_sub(1)));
        }
    }
}
