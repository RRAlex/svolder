// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cover art of the track playing, drawn in the Now playing row.
//!
//! The art is a nicety: nothing here ever holds up a frame or a key, and a terminal, a daemon or
//! a picture that does not cooperate leaves the row as it was without art.
//!
//! - [`probe`]: asks the terminal once, as the TUI starts, which graphics it draws.
//! - [`filter`]: the terminal's input with the strings its answers come in taken out of it.
//! - [`engine`]: fetching, decoding and sizing the picture off the loop, one track at a time.

pub(super) mod engine;
pub(super) mod filter;
pub(super) mod probe;

pub(super) use engine::{Art, Picture};

/// Ask the terminal what it draws when `tui` asks for cover art, which is once, as the TUI
/// starts, on the alternate screen and before any input is read; `None` for art that is off,
/// which asks the terminal nothing.
pub(super) fn probe_for(tui: &svolder_ipc::TuiConfig) -> Option<ratatui_image::picker::Picker> {
    (tui.cover_art == svolder_ipc::CoverArt::Auto).then(probe::probe)
}
