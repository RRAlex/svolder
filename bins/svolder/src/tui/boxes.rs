// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The boxes the TUI opens over a screen: each one's state and logic live here, and its drawing
//! lives in [`screens`](super::screens), except the toast and the tooltip, which take no key and
//! draw themselves.
//!
//! - [`editor`]: the device editor, one `[[device]]` block as rows over the device list.
//! - [`edit_line`]: one band's frequency, gain and Q typed as numbers.
//! - [`presets`]: the named tone curves that fit the device's tone control.
//! - [`device_presets`]: the presets a device keeps itself: its slots and its built-in settings.
//! - [`connections`]: the Settings box's Connections category, the daemon's `[mcp]` and `[mqtt]`.
//! - [`web_box`]: the Settings box's Web UI box, its link, pairing code and token tail.
//! - [`toast`]: what the last thing asked for came to, in a small box over a corner.
//! - [`tooltip`]: what a pointer resting on something clickable says it is.

pub(super) mod connections;
pub(super) mod device_presets;
pub(super) mod edit_line;
pub(super) mod editor;
pub(super) mod presets;
pub(super) mod toast;
pub(super) mod tooltip;
pub(super) mod web_box;
