// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is open over the screen: the Settings box, the Esc menu, the help overlay and the quit
//! question, and what the Settings box's Connect category knows.

use crate::tui::boxes::connections::Connections;

use super::SettingsBox;

/// What is open over the screen: the Settings box, the Esc menu, the help overlay and the quit
/// question, and what the Settings box's Connect category knows.
pub struct Overlays {
    /// The Settings box, while it is open over the screen: the category and row it is on.
    pub settings: Option<SettingsBox>,
    /// The category and row the Settings box was last closed on, which it opens on again.
    pub(crate) settings_memory: SettingsBox,
    /// An accent colour of the user's own per family — one the palette's stops lack, read off
    /// the row the first time it is stepped — so the row keeps it as a stop to step back to.
    pub(crate) held_accents: svolder_ipc::FamilyAccents,
    /// What the Settings box's Connect category knows of `[mcp]`, `[mqtt]`, the token and the
    /// daemon's two servers.
    pub conn: Connections,
    /// The menu `Esc` opens on the device screen, while it is open: the cursor over
    /// [`menu::ENTRIES`](crate::tui::screens::menu::ENTRIES).
    pub menu: Option<usize>,
    /// Whether the help overlay is open, which `?` opens and `?`, `Esc` or a click closes.
    /// While it is open every key but its own is swallowed.
    pub help_open: bool,
    /// How many rows of keys the help overlay has been scrolled past, for the terminals it
    /// holds more keys than. Starts again at the top every time the overlay opens.
    pub help_scroll: usize,
    /// Whether the question `q` asks before the TUI quits is open.
    pub quitting: bool,
}
