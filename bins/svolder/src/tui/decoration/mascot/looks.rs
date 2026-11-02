// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the squirrel wears and how it is coloured: the costume a palette picks, the season the
//! local date falls in, the layers the two put on, and the colour of each sheet letter.

use std::time::Duration;

use crate::tui::theme::{NEON, Rgb};

/// A set of fur colours and props, one for each palette (and god mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Costume {
    Amber,
    Ice,
    Red,
    Neon,
    God,
}

impl Costume {
    /// The costume the palette `name` wears; any palette without one of its own wears amber.
    pub(crate) fn of_palette(name: &str) -> Costume {
        match name {
            "ice" => Costume::Ice,
            "red" => Costume::Red,
            "neon" => Costume::Neon,
            _ => Costume::Amber,
        }
    }
}

/// A day of the year the squirrel dresses for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Season {
    NewYear,
    Groundhog,
    Pi,
    Alien,
    Towel,
    Midsummer,
    SaintJean,
    Pride,
    Canada,
    Independence,
    Bastille,
    CatInfinity,
    Judgment,
    Pirate,
    Truth,
    Halloween,
    Remembrance,
    Yule,
}

impl Season {
    /// The season `month`/`day` falls in, the first match in the table's order.
    pub(crate) fn on(month: u8, day: u8) -> Option<Season> {
        let season = match (month, day) {
            (6, 24) => Season::SaintJean,
            (12, 30..=31) | (1, 1..=2) => Season::NewYear,
            (2, 2) => Season::Groundhog,
            (3, 14) => Season::Pi,
            (4, 26) => Season::Alien,
            (5, 25) => Season::Towel,
            (6, 19..=25) => Season::Midsummer,
            (6, 28) => Season::Pride,
            (7, 1) => Season::Canada,
            (7, 4) => Season::Independence,
            (7, 14) => Season::Bastille,
            (8, 8) => Season::CatInfinity,
            (8, 29) => Season::Judgment,
            (9, 19) => Season::Pirate,
            (9, 30) => Season::Truth,
            (10, 24..=31) => Season::Halloween,
            (11, 11) => Season::Remembrance,
            (12, 13..=26) => Season::Yule,
            _ => return None,
        };
        Some(season)
    }

    /// Whether the day is a quiet one, with no jokes.
    pub(crate) fn quiet(self) -> bool {
        matches!(self, Season::Truth | Season::Remembrance)
    }
}

/// The layers a pose is dressed in, besides the body and the head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dress {
    /// The hat layer, if the squirrel wears one.
    pub hat: Option<&'static str>,
    /// The layers it carries.
    pub carry: Vec<&'static str>,
    /// The layers it wears: a season's and the costume's.
    pub wear: Vec<&'static str>,
    /// An eyes layer that replaces the default gaze.
    pub eyes: Option<&'static str>,
}

/// `frames`' layer at `elapsed`, the frames looping with their durations in milliseconds.
fn frame<T: Copy>(frames: &[(T, u64)], elapsed: Duration) -> T {
    let total: u64 = frames.iter().map(|(_, ms)| ms).sum();
    let mut at = elapsed.as_millis() as u64 % total;
    for (layer, ms) in frames {
        if at < *ms {
            return *layer;
        }
        at -= ms;
    }
    frames[0].0
}

/// The steam rising from a hot dish at `elapsed`.
fn steam(elapsed: Duration) -> &'static str {
    frame(
        &[
            ("steam1", 250),
            ("steam2", 250),
            ("steam3", 250),
            ("steam4", 250),
        ],
        elapsed,
    )
}

/// The default helmet and horn, overridden by `season`, plus the costume's own worn layers.
/// Animated props pick their frame from `elapsed`.
pub(crate) fn dress(costume: Costume, season: Option<Season>, elapsed: Duration) -> Dress {
    let mut dress = Dress {
        hat: Some("helmet"),
        carry: vec!["horn"],
        wear: Vec::new(),
        eyes: None,
    };
    let carry = |dress: &mut Dress, layers: &[&'static str]| dress.carry = layers.to_vec();
    match season {
        None | Some(Season::Pride) => {}
        Some(Season::NewYear) => {
            let fw = frame(
                &[
                    (Some("fw1"), 300),
                    (Some("fw2"), 300),
                    (Some("fw3"), 500),
                    (None, 400),
                ],
                elapsed,
            );
            carry(&mut dress, fw.as_slice());
            dress.eyes = Some("eyes_up");
        }
        Some(Season::Groundhog) => {
            carry(&mut dress, &["shadow"]);
            dress.eyes = Some("eyes_left");
        }
        Some(Season::Pi) => carry(&mut dress, &["pi_sign", "pie", steam(elapsed)]),
        Some(Season::Alien) => {
            carry(
                &mut dress,
                &[frame(&[("egg1", 1400), ("egg2", 900)], elapsed)],
            );
            dress.eyes = Some("eyes_left");
        }
        Some(Season::Towel) => carry(
            &mut dress,
            &[frame(&[("towel", 500), ("towel2", 500)], elapsed)],
        ),
        Some(Season::Midsummer) => {
            dress.hat = Some(frame(&[("crown", 400), ("crown2", 400)], elapsed));
        }
        Some(Season::SaintJean) => carry(&mut dress, &["poutine", steam(elapsed)]),
        Some(Season::Canada) => carry(
            &mut dress,
            &[frame(&[("maple", 600), ("maple2", 600)], elapsed)],
        ),
        Some(Season::Independence) => {
            carry(
                &mut dress,
                &[frame(&[("spark1", 160), ("spark2", 160)], elapsed)],
            );
        }
        Some(Season::Bastille) => {
            dress.hat = Some("beret");
            carry(
                &mut dress,
                &[frame(&[("baguette", 500), ("baguette2", 500)], elapsed)],
            );
        }
        Some(Season::CatInfinity) => carry(
            &mut dress,
            &[frame(&[("infinity", 450), ("infinity2", 450)], elapsed)],
        ),
        Some(Season::Judgment) => {
            dress
                .wear
                .push(frame(&[("shades", 1400), ("shades2", 400)], elapsed));
        }
        Some(Season::Pirate) => {
            dress.hat = Some("tricorn");
            dress
                .wear
                .push(frame(&[("eyepatch", 1200), ("eyepatch2", 800)], elapsed));
        }
        Some(Season::Truth) => {
            carry(&mut dress, &[]);
            dress.wear.push("shirt");
        }
        Some(Season::Halloween) => {
            carry(
                &mut dress,
                &[frame(&[("pumpkin1", 300), ("pumpkin2", 300)], elapsed)],
            );
        }
        Some(Season::Remembrance) => {
            carry(&mut dress, &[]);
            dress.wear.push("poppy");
        }
        Some(Season::Yule) => {
            dress.hat = Some("yulecap");
            dress.wear.push("scarf");
        }
    }
    let own: Vec<&'static str> = match costume {
        Costume::Ice => vec![
            "scarf",
            frame(&[("breath1", 700), ("breath2", 700)], elapsed),
        ],
        Costume::Red => vec![frame(&[("ember1", 260), ("ember2", 260)], elapsed)],
        Costume::Amber | Costume::Neon | Costume::God => Vec::new(),
    };
    for layer in own {
        if !dress.wear.contains(&layer) {
            dress.wear.push(layer);
        }
    }
    dress
}

/// The colour of one sheet letter, given the costume, the accent the palette's family carries
/// when the config overrides it, and the season.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Paint {
    pub costume: Costume,
    /// The family's accent when `[tui]` overrides it; tints the amber costume only.
    pub accent: Option<Rgb>,
    pub season: Option<Season>,
}

/// `0xRRGGBB` as a colour.
const fn hex(value: u32) -> Rgb {
    ((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// The neon costume's LEDs, which chase along the S pixels: the neon palette's three family
/// accents.
const LEDS: [Rgb; 3] = [NEON.dac, NEON.amp, NEON.preamp];

/// The Pride tail's bands, one colour a box pixel row.
const RAINBOW: [Rgb; 12] = [
    hex(0xe40303),
    hex(0xe40303),
    hex(0xe40303),
    hex(0xe40303),
    hex(0xe40303),
    hex(0xff8c00),
    hex(0xffed00),
    hex(0x008026),
    hex(0x004dff),
    hex(0x004dff),
    hex(0x750787),
    hex(0x750787),
];

/// How long an LED holds before the chase steps on.
const LED_STEP: Duration = Duration::from_millis(160);

/// How far the custom accent pulls the amber fur.
const TINT: f64 = 0.3;

/// The letters every costume shares, with their amber-costume colours.
fn base(letter: char) -> Option<Rgb> {
    let value = match letter {
        'F' | 'T' => 0xa65a1d,
        'D' | 'S' => 0x5a3013,
        'C' => 0xc9a07a,
        'W' => 0xf3eee2,
        'K' => 0x120d08,
        'N' => 0x2e1a0e,
        'H' => 0x9a9088,
        'h' => 0x4a433c,
        'A' => 0xe0a030,
        'a' => 0x9c6a14,
        'R' => 0xe0a030,
        'r' => 0xb07818,
        'G' => 0x8f969c,
        'g' => 0x5f666c,
        'Y' | 'U' => 0xe8dcc0,
        'u' => 0xb89a5a,
        'V' => 0xffd060,
        'E' => 0xe8452c,
        'w' => 0x3a2a1a,
        'P' => 0xcfa874,
        'p' => 0x8f6a3e,
        'B' => 0x4f8fe0,
        'I' => 0xdff4fa,
        'L' => 0x8a8560,
        'l' => 0x5a5638,
        'O' => 0xe07a20,
        'Z' => 0xd06a8a,
        'Q' => 0xefe3c4,
        'q' => 0xa8916a,
        '1' => 0xe05040,
        '2' => 0xe8b030,
        '3' => 0x5fbf6f,
        '4' => 0x4f8fe0,
        '5' => 0x9a6ae0,
        '6' => 0x6a2a22,
        '7' => 0x6e5418,
        '8' => 0x2e5a34,
        '9' => 0x26446a,
        '0' => 0x4a3470,
        _ => return None,
    };
    Some(hex(value))
}

/// What `costume` paints `letter` instead of the base colour, if anything.
fn costume_colour(costume: Costume, letter: char) -> Option<Rgb> {
    let value = match (costume, letter) {
        (Costume::Ice, 'F' | 'T') => 0x7d8a94,
        (Costume::Ice, 'D' | 'S') => 0x3e4a54,
        (Costume::Ice, 'C') => 0xd8e2ea,
        (Costume::Ice, 'R') => 0x5fc9c9,
        (Costume::Ice, 'r') => 0x1f6aa8,
        (Costume::Red, 'F' | 'T') => 0xb0341c,
        (Costume::Red, 'D' | 'S') => 0x5c160c,
        (Costume::Red, 'C') => 0xe8c0a0,
        (Costume::Neon, 'F' | 'T') => 0x2a2438,
        (Costume::Neon, 'D') => 0x15121e,
        (Costume::Neon, 'C') => 0x6a6080,
        (Costume::God, 'F' | 'T') => 0xe8b830,
        (Costume::God, 'D' | 'S') => 0x8a5a10,
        (Costume::God, 'C') => 0xfff0b0,
        (Costume::God, 'W') => 0xfff8d8,
        (Costume::God, 'K') => 0xffb000,
        _ => return None,
    };
    Some(hex(value))
}

/// `from` pulled `t` of the way toward `to`.
fn mix(from: Rgb, to: Rgb, t: f64) -> Rgb {
    let channel = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

impl Paint {
    /// The colour of `letter` at box pixel (`x`, `y`) `elapsed` into the animation. Panics on
    /// a letter the sheet does not use.
    pub(crate) fn rgb(&self, letter: char, x: usize, y: usize, elapsed: Duration) -> Rgb {
        if self.season == Some(Season::Pride) && matches!(letter, 'T' | 'S') {
            return RAINBOW[y.min(RAINBOW.len() - 1)];
        }
        if self.season == Some(Season::Yule) {
            match letter {
                'R' => return hex(0xd8322a),
                'r' => return hex(0x8e1c16),
                _ => {}
            }
        }
        if self.costume == Costume::Neon && letter == 'S' {
            let step = (elapsed.as_millis() / LED_STEP.as_millis()) as usize;
            return LEDS[(x + y + step) % LEDS.len()];
        }
        let colour = costume_colour(self.costume, letter)
            .or_else(|| base(letter))
            .unwrap_or_else(|| panic!("no colour for the sheet letter {letter:?}"));
        match (self.costume, self.accent) {
            (Costume::Amber, Some(accent)) if matches!(letter, 'F' | 'T' | 'D' | 'S') => {
                mix(colour, accent, TINT)
            }
            _ => colour,
        }
    }
}
