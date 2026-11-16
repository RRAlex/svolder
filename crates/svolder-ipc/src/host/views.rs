// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wire types of the host: what is playing, the route read out of PipeWire, and the
//! daemon's capture frames.

use serde::{Deserialize, Serialize};
use svolder_host::{Progress, RouteFacts};

/// What the daemon knows of the host's player and route when it answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostView {
    /// Whether the daemon watches the host at all: false with `[host] enabled = false`,
    /// `SVOLDER_HOST=off`, or no session bus. Everything else is empty then.
    #[serde(default)]
    pub available: bool,
    /// The followed player and its track; absent while there is no player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playing: Option<PlayingView>,
    /// The route to the default sink, as the last read of the graph gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteFacts>,
    /// The daemon's clock when this was sent, in unix milliseconds: what the progress is
    /// counted forward from.
    #[serde(default)]
    pub now_ms: u64,
    /// The computer's volume, as the last read of its default sink gave it; absent while it has
    /// not been read, and from a daemon that does not serve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<HostVolume>,
    /// The computer's EQ; absent from a daemon that does not serve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<svolder_host::eq::EqView>,
}

/// The computer's default sink as a front end shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostVolume {
    /// The volume in percent: above 100 when another program boosted the sink, which is shown as
    /// it is read and never written.
    pub pct: u16,
    /// Whether the sink is muted.
    pub muted: bool,
    /// What the sink is called for people.
    #[serde(default)]
    pub sink: String,
}

/// The followed player and its track, as a page or a TUI shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayingView {
    /// The track's title.
    #[serde(default)]
    pub title: String,
    /// The track's artist.
    #[serde(default)]
    pub artist: String,
    /// The track's album.
    #[serde(default)]
    pub album: String,
    /// The player's name, as it calls itself.
    #[serde(default)]
    pub player: String,
    /// What the player is doing: `playing`, `paused` or `stopped`.
    #[serde(default)]
    pub status: String,
    /// The track's length in microseconds, when the player says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_us: Option<u64>,
    /// Where the player was in the track when last asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    /// The hash of the cover the daemon holds, when it read one; the page asks for the cover
    /// by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
}

/// One analysed frame of what the computer plays, as a joined TUI is sent it
/// (`Event::HostFrame`): the daemon's [`svolder_host::Frame`] with its bands as plain arrays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameView {
    /// When the frame was captured, in unix milliseconds on the daemon's clock.
    pub t: u64,
    /// The left channel's bands, 240 bytes on the scale [`svolder_host::analysis::to_byte`]
    /// describes (a tuning, not a promise).
    pub left: Vec<u8>,
    /// The right channel's bands, 240 bytes.
    pub right: Vec<u8>,
    /// Left RMS, right RMS, left peak and right peak, as bytes on the same scale. A peak byte of
    /// 255 means an over (a sample at or above 16-bit full scale); below that, 254 is the top.
    pub levels: [u8; 4],
    /// Whether a beat starts in this frame.
    #[serde(default)]
    pub beat: bool,
    /// The tempo, once the last few seconds hold one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u16>,
    /// How far along the beat period the frame is, 0 at the beat to 255 just before the next.
    #[serde(default)]
    pub phase: u8,
}

impl From<&svolder_host::Frame> for FrameView {
    fn from(frame: &svolder_host::Frame) -> Self {
        FrameView {
            t: frame.capture_ms,
            left: frame.left.to_vec(),
            right: frame.right.to_vec(),
            levels: frame.levels,
            beat: frame.beat,
            bpm: frame.bpm,
            phase: frame.phase,
        }
    }
}

impl FrameView {
    /// The frame this view was made from; `None` when a channel does not hold exactly the
    /// bands a frame has (a peer that analyses differently).
    pub fn to_frame(&self) -> Option<svolder_host::Frame> {
        Some(svolder_host::Frame {
            capture_ms: self.t,
            left: self.left.as_slice().try_into().ok()?,
            right: self.right.as_slice().try_into().ok()?,
            levels: self.levels,
            beat: self.beat,
            bpm: self.bpm,
            phase: self.phase,
        })
    }
}

/// Where the daemon's capture stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    /// Nothing is captured yet: the capture is about to start, or waits for a first viewer.
    Starting,
    /// Frames are coming.
    Running,
    /// No capture is possible now (no `pw-record`, no sink, the host is off); `why` says.
    Unavailable,
}

/// The state of the daemon's capture, sent once when a frames stream opens and on each change
/// (`Event::HostCapture`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureView {
    /// Where the capture stands.
    pub state: CaptureKind,
    /// While it runs, what the captured sink is called for people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,
    /// While it is unavailable, why, in a plain sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[cfg(test)]
mod tests;
