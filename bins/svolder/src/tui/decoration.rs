// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The animation and ornament the TUI draws around its content.
//!
//! Decoration never carries the only copy of a fact: a frame's text is the same with every
//! effect off, so a reader adapting the controls can skip this folder.
//!
//! - [`effects`]: the animation layer drawn over the screens after the widgets finish.
//! - [`intro`]: the wordmark on screen while the device is being opened.
//! - [`intro_sound`]: the sound that plays with the intro.
//! - [`mascot`]: Ratatoskr, the half-block pixel-art squirrel.
//! - [`transition`]: how one screen gives way to the next.
//! - [`title`]: the terminal's title as a small VU meter.
//! - [`wordmark`]: the Svolder wordmark.

pub(super) mod effects;
pub(super) mod intro;
pub(super) mod intro_sound;
pub(super) mod mascot;
pub(super) mod title;
pub(super) mod transition;
pub(super) mod wordmark;
