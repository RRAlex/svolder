// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The screens the TUI draws, one module each.
//!
//! - [`device`]: the device screen: the control rows, the tone-control summary, the status.
//! - [`devices`]: the device list.
//! - [`device_presets`]: the box of a device's own presets.
//! - [`help`]: the help overlay: every key of every screen.
//! - [`menu`]: the menu `Esc` opens on the device screen.
//! - [`presets`]: the preset list's box.
//! - [`quit`]: the question `q` asks before the TUI quits.
//! - [`settings`]: the Settings box: every choice the `[tui]` table holds.
//! - [`telemetry`]: every reading the device has sent, with its wire type and raw value.
//! - [`tone`]: the tone-control screen: the response the bands add up to, and the bands.
//! - [`wizard`]: one step of the setup wizard.

pub mod device;
pub mod device_presets;
pub mod devices;
pub mod help;
pub mod menu;
pub mod presets;
pub mod quit;
pub mod settings;
pub mod telemetry;
pub mod tone;
pub mod wizard;
