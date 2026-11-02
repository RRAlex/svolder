// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An in-process fake Forkbeard device.
//!
//! [`FakeDevice`] decodes a TX frame with [`svolder_proto`] and returns the RX
//! notifications a real device would send, so the session and CLI can be tested without
//! hardware. It is synchronous and holds no I/O; `svolder-ble` wraps it in a transport.
//!
//! [`Signal`] is the stand-in for music a fake device's meters play, so a front end drawing
//! to the music has something to draw without an amplifier on the bench.
//!
//! - [`device`]: the fake device: an in-memory control map that answers protocol frames.
//! - [`signal`]: a stand-in for music: a level per channel, for the fake device's meters.
#![warn(missing_docs)]

pub mod device;
pub mod signal;

pub use device::{FakeDevice, SAMPLE_RATE_HZ};
pub use signal::{SIGNAL_PERIOD, Signal};
