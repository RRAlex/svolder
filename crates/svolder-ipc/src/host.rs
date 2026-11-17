// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon tells its front ends about the computer's music player, and the one pure
//! judgement they share of the path the sound takes from the player to the DAC.
//!
//! - [`HostView`] and [`PlayingView`] are the wire types: what is playing, and what
//!   [`svolder_host::RouteFacts`] read out of PipeWire. They ride a `host` answer and the
//!   `host` event.
//! - [`path_line`] judges that route against the device's own readings ([`DacFacts`]) and
//!   gives a [`PathLine`]: the hops from the player to the DAC, a [`Verdict`] and the words
//!   that name what changes the signal. [`sink_is_dac`] says whether the computer's sink is
//!   the device at all, and [`rate_hz`] reads the sample rate a device's label names.
//! - [`run_eq_op`] carries out a computer-EQ request on an `EqControl`, for the daemon and for a
//!   TUI with no daemon alike.
//! - [`FrameView`] and [`CaptureView`] are the wire types of the daemon's capture: what a
//!   joined TUI reads on a `host_frames` stream (`Event::HostFrame`, `Event::HostCapture`).
//!
//! The verdict is untouched only when every hop is known to leave the signal alone. What the
//! daemon cannot see (the bits themselves) is never claimed: the words never say more than
//! the rates, the volumes, the mixing and the EQ show.

mod eq_op;
mod path;
mod views;

pub use eq_op::run_eq_op;
pub use path::{
    DacFacts, DacStage, Hop, PathLine, ROUTE_NOT_READ, SCHIIT_VENDOR_ID, Verdict, path_line,
    rate_hz, sink_is_dac,
};
pub use views::{CaptureKind, CaptureView, FrameView, HostView, HostVolume, PlayingView};
