// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ratatoskr, the half-block pixel-art squirrel: the sprite sheet ([`sheet`]), and the costumes,
//! seasons and colours ([`looks`]) a pose is dressed in. The squirrel lives in a box of
//! [`BOX_W`] × [`BOX_H`] pixels, one pixel to a column and two to a cell row. The brain
//! ([`Mascot`]) watches what the app shows through a [`View`] and says what the squirrel is
//! doing; it never draws and never reads a clock. [`draw`] paints it in half blocks, at home on
//! the Tone pane or floating over the screen's body.
//!
//! - [`brain`]: what the squirrel is doing and where, from the views it is shown.
//! - [`codes`]: the codes typed into the help overlay and the Esc menu, and the matcher.
//! - [`draw`]: drawing the squirrel in half blocks.
//! - [`looks`]: what the squirrel wears and how it is coloured.
//! - [`sheet`]: the sprite sheet: layers of one letter per pixel.
//! - [`tricks`]: the squirrel's tricks, states and dances as poses over time.

mod brain;
pub(crate) mod codes;
pub(crate) mod draw;
pub(crate) mod looks;
pub(crate) mod sheet;
pub(crate) mod tricks;

use std::collections::BTreeMap;
use std::time::Duration;

use svolder_proto::FunctionId;

pub(crate) use brain::Mascot;
#[cfg(test)]
pub(crate) use brain::What;
pub(crate) use codes::Code;
use looks::Paint;
use tricks::{Dance, Pose};

/// The dance a player's genre words ask for: the first word, in order, that holds a keyword
/// (any case) picks it, and no keyword gives the level bob.
pub(crate) fn dance_of(genre: &[String]) -> Dance {
    const KEYWORDS: [(Dance, &[&str]); 4] = [
        (Dance::Headbang, &["rock", "metal", "punk", "grunge"]),
        (
            Dance::Bounce,
            &[
                "electronic",
                "dance",
                "house",
                "techno",
                "trance",
                "drum",
                "dubstep",
            ],
        ),
        (
            Dance::Sway,
            &[
                "classical",
                "jazz",
                "ambient",
                "baroque",
                "opera",
                "chamber",
            ],
        ),
        (
            Dance::Nod,
            &["hip-hop", "hip hop", "rap", "funk", "soul", "r&b"],
        ),
    ];
    genre
        .iter()
        .find_map(|word| {
            let word = word.to_lowercase();
            KEYWORDS
                .iter()
                .find(|(_, keys)| keys.iter().any(|key| word.contains(key)))
                .map(|(dance, _)| *dance)
        })
        .unwrap_or(Dance::Bob)
}

/// The box's width in pixels, which is columns.
pub(crate) const BOX_W: usize = 27;
/// The box's height in pixels, which is twice the cell rows.
pub(crate) const BOX_H: usize = 12;
/// The box pixel the squirrel's own top-left sits at: room for the carried item on its left and
/// the headroom above.
pub(crate) const ORIGIN: (i32, i32) = (8, 2);

/// Where the device's link stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Link {
    Connected,
    /// Down, with a feed that may still bring it back.
    Reconnecting,
    /// Down, with nothing left to bring it back; a held session counts here.
    Offline,
}

/// What the tone control stands at: the loaded preset's name and the raw value of every EQ
/// reading heard, by function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EqSig {
    pub preset: Option<String>,
    pub raws: BTreeMap<FunctionId, i64>,
}

/// What the brain looks at each drawn turn. A field the device does not have is `None`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct View {
    /// The volume's raw value, lowest raw value and highest.
    pub volume: Option<(i64, i64, i64)>,
    /// Whether the output is muted: the device's own switch, else the host's sink.
    pub muted: Option<bool>,
    /// Whether the device is on (raw 0 is off or standby).
    pub power: Option<bool>,
    /// The raw value of the input select.
    pub input: Option<i64>,
    pub eq: Option<EqSig>,
    pub link: Link,
    /// Whether a fault is standing.
    pub fault: bool,
    /// Whether a fault's advice is being shown.
    pub advice_shown: bool,
    /// How loud what plays is, `0.0..=1.0`.
    pub level: Option<f64>,
    /// What the followed player does and the genre words it gave.
    pub playing: Option<(svolder_host::players::Status, Vec<String>)>,
    /// How many times `a` has said it was all clear.
    pub presses: u32,
    /// Whether the screen shown is the one the squirrel lives on.
    pub screen_is_home: bool,
}

impl View {
    /// A view of a device with none of the controls, connected, on its home screen.
    #[cfg(test)]
    pub(crate) fn bare() -> View {
        View {
            volume: None,
            muted: None,
            power: None,
            input: None,
            eq: None,
            link: Link::Connected,
            fault: false,
            advice_shown: false,
            level: None,
            playing: None,
            presses: 0,
            screen_is_home: true,
        }
    }
}

/// Where a showing is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// At home, on the screen the squirrel lives on.
    Home,
    /// Floating over the bottom right of whatever screen is shown.
    Float,
}

/// What the squirrel looks like now and where it is drawn.
#[derive(Debug, Clone)]
pub(crate) struct Showing {
    pub pose: Pose,
    pub place: Place,
    pub paint: Paint,
    /// The time since the session began, which the costume's animated colours turn on.
    pub elapsed: Duration,
}

#[cfg(test)]
mod tests;
