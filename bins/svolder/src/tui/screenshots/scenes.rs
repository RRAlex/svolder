// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The five stills: each a device screen of the world, settled for two seconds with the
//! animation layer's arrivals played out, then taken as the frame the last turn drew.
//!
//! - [`record`]: the Particles intro as a first launch plays it, recorded frame by frame.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use svolder_profiles::tone::apo::Correction;

use crate::tui::app::{App, Screen};
use crate::tui::render::FrameAreas;
use crate::tui::run::drawn_turn;
use crate::tui::screenshots::raster::{Palette, ink, mix};
use crate::tui::screenshots::world::{
    ANIMATION_SIGNAL_AT, START_VOLUME_DB, STILL_SIGNAL_AT, STILL_VOLUME_DB, World, aegir2, block,
    feed_host_history, feed_meter, feed_meter_history, gungnir, rise, runtime, set_volume,
};

mod record;

/// The terminal every picture is drawn on, in columns and rows.
const SIZE: (u16, u16) = (120, 40);

/// The time between two frames of a picture: about 30 frames a second.
const FRAME: Duration = Duration::from_millis(33);

/// How many turns a still settles for: two seconds, past every arrival and lap.
const SETTLE_TURNS: u32 = 61;

/// How long before [`STILL_SIGNAL_AT`] the settling turns start reading the signal.
const LEAD: Duration = Duration::from_secs(2);

/// Every how many turns the amp's meters report: every 100 ms.
const METER_TURNS: u32 = 3;

/// A picture of the README.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::screenshots) enum Still {
    /// The Gungnir 2's device screen.
    Dac,
    /// The Aegir 2's device screen.
    Amp,
    /// The Gungnir 2's tone-control screen.
    Tone,
    /// The Aegir 2's telemetry screen.
    Telemetry,
    /// The Devices screen.
    Devices,
}

impl Still {
    /// Every still, in the order the README shows them.
    pub(in crate::tui::screenshots) const ALL: [Still; 5] = [
        Still::Dac,
        Still::Amp,
        Still::Tone,
        Still::Telemetry,
        Still::Devices,
    ];

    /// The name its files and the guard's messages carry.
    pub(in crate::tui::screenshots) fn name(self) -> &'static str {
        match self {
            Still::Dac => "dac",
            Still::Amp => "amp",
            Still::Tone => "tone",
            Still::Telemetry => "telemetry",
            Still::Devices => "devices",
        }
    }

    /// Whether the still is of the amp.
    fn on_the_amp(self) -> bool {
        matches!(self, Still::Amp | Still::Telemetry)
    }

    /// The screen the still shows.
    fn screen(self) -> Screen {
        match self {
            Still::Dac | Still::Amp => Screen::Device,
            Still::Tone => Screen::Tone,
            Still::Telemetry => Screen::Telemetry,
            Still::Devices => Screen::Devices,
        }
    }
}

/// A taken still: the frame, where the frame drew its parts, and what the picture needs to be
/// judged by.
pub(in crate::tui::screenshots) struct Shot {
    /// The last turn's frame.
    pub buffer: Buffer,
    /// Where that frame drew its parts.
    pub areas: FrameAreas,
    /// Whether the animation layer has nothing left running.
    pub at_rest: bool,
    /// The volume step where the tone curve starts to clip, if it boosts.
    pub clip_step: Option<i64>,
}

/// The `still` of a world whose DAC wears `correction`.
pub(in crate::tui::screenshots) fn still(correction: &Correction, still: Still) -> Shot {
    still_with(correction, still, |_| {})
}

/// The `still`, with `tweak` applied to the app after it is built and before it settles.
pub(in crate::tui::screenshots) fn still_with(
    correction: &Correction,
    still: Still,
    tweak: impl FnOnce(&mut App),
) -> Shot {
    runtime().block_on(async {
        let world = World::new(correction.clone());
        let t0 = world.base;
        let signal_start = STILL_SIGNAL_AT - LEAD;
        let mut app = if still.on_the_amp() {
            let mut app = aegir2(&world, t0).await;
            feed_meter_history(&mut app, signal_start, t0);
            app
        } else {
            gungnir(&world, t0, STILL_VOLUME_DB).await
        };
        feed_host_history(&mut app, signal_start, t0);
        app.show(still.screen());
        tweak(&mut app);

        let World { mut look, .. } = world;
        let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a terminal");
        let mut last = None;
        for n in 0..SETTLE_TURNS {
            let now = t0 + FRAME * n;
            let t = still_signal_at(n);
            app.tick(now);
            app.heard_block(&block(t), now);
            if still.on_the_amp() && n % METER_TURNS == 0 {
                feed_meter(&mut app, t, now);
            }
            let since = if n == 0 { Duration::ZERO } else { FRAME };
            last = Some(drawn_turn(&mut terminal, &mut app, &mut look, now, since));
        }
        let (buffer, areas) = last.expect("a turn was drawn");
        Shot {
            buffer,
            areas,
            at_rest: !look.effects.active(),
            clip_step: app.clip_step(),
        }
    })
}

/// The animation's random seed: the Particles intro's scatter and its exit are drawn from it.
const SEED: u64 = 0x5EED_0A11;

/// How long after the loop's first frame the volume steps.
const VOLUME_AFTER: Duration = Duration::from_millis(1700);

/// How long after the loop's first frame the key to the Tone screen is pressed.
const KEY_AFTER: Duration = Duration::from_millis(3000);

/// How long after the loop's first frame one band rises.
const GAIN_AFTER: Duration = Duration::from_millis(4000);

/// How long after the loop's first frame the Tone screen holds before the fade starts.
const TONE_UNTIL: Duration = Duration::from_millis(5800);

/// How long the fade to the ground colour lasts.
const FADE: Duration = Duration::from_millis(600);

/// How long the intro plays: the loop's clock and the signal's start after it.
const INTRO: Duration = Duration::from_millis(3600);

/// How many turns the animation's loop runs: the Tone screen's hold and the fade.
fn animation_turns() -> u32 {
    (TONE_UNTIL + FADE).div_duration_f64(FRAME).ceil() as u32
}

/// The instant of the signal a still reads on settle turn `n`, `LEAD` before the still's own.
fn still_signal_at(n: u32) -> Duration {
    STILL_SIGNAL_AT - LEAD + FRAME * n
}

/// Every instant at which a scene reads the seeded signal for its levels: the animation's
/// frames and the stills' settle turns.
pub(in crate::tui::screenshots) fn signal_instants() -> impl Iterator<Item = Duration> {
    let animation = (0..animation_turns()).map(|n| ANIMATION_SIGNAL_AT + INTRO + FRAME * n);
    let stills = (0..SETTLE_TURNS).map(still_signal_at);
    animation.chain(stills)
}

/// The animation: the Particles intro, then the loop's frames, and where its moments fall.
///
/// Frame indexes count from the first intro frame.
pub(in crate::tui::screenshots) struct Animation {
    /// Every frame, intro first.
    pub frames: Vec<Buffer>,
    /// The index of the loop's first frame, the first after the intro.
    pub loop_start: usize,
    /// The first frame that shows the new volume.
    pub volume_frame: usize,
    /// The frame drawn right after the key to the Tone screen.
    pub key_frame: usize,
    /// The first frame that shows the raised band.
    pub gain_frame: usize,
    /// The first frame of the fade to the ground colour.
    pub fade_start: usize,
    /// How many frames painted a toast.
    pub toasts: usize,
}

/// The animation of a world whose DAC wears `correction`.
pub(in crate::tui::screenshots) fn animation(correction: &Correction) -> Animation {
    runtime().block_on(async {
        let world = World::new(correction.clone());
        let base = world.base;
        let (mut frames, exit) = record::intro(&world.look, SEED).await;
        let loop_start = frames.len();
        let mut app = gungnir(&world, base, START_VOLUME_DB).await;
        let signal_at = ANIMATION_SIGNAL_AT + INTRO;
        feed_host_history(&mut app, signal_at, base);
        let World { mut look, .. } = world;
        look.effects.hand_over(exit);

        let loop_turns = animation_turns();
        let fade_turns = (0..loop_turns).filter(|&n| FRAME * n >= TONE_UNTIL).count();
        let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a terminal");
        let mut volume_frame = None;
        let mut key_frame = None;
        let mut gain_frame = None;
        let mut fade_start = None;
        let mut toasts = 0;
        for n in 0..loop_turns {
            let since_start = FRAME * n;
            let now = base + since_start;
            app.tick(now);
            app.heard_block(&block(signal_at + since_start), now);
            if volume_frame.is_none() && since_start >= VOLUME_AFTER {
                set_volume(&mut app, STILL_VOLUME_DB, now);
                volume_frame = Some(loop_start + n as usize);
            }
            if key_frame.is_none() && since_start >= KEY_AFTER {
                look.effects.on_key(now);
                let intents = app.key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
                assert!(intents.is_empty(), "the key asks the device for nothing");
                key_frame = Some(loop_start + n as usize);
            }
            if gain_frame.is_none() && since_start >= GAIN_AFTER {
                rise(&mut app, 3.0, now);
                gain_frame = Some(loop_start + n as usize);
            }
            let since = if n == 0 { Duration::ZERO } else { FRAME };
            let (buffer, areas) = drawn_turn(&mut terminal, &mut app, &mut look, now, since);
            toasts += usize::from(areas.toast.is_some());
            if since_start < TONE_UNTIL {
                frames.push(buffer);
                continue;
            }
            let first = *fade_start.get_or_insert(frames.len());
            let k = frames.len() - first;
            let alpha = (k + 1) as f32 / fade_turns as f32;
            let palette = Palette::of(&look.theme);
            frames.push(faded(&buffer, alpha, &palette).unwrap_or_else(|why| panic!("{why}")));
        }
        Animation {
            frames,
            loop_start,
            volume_frame: volume_frame.expect("the volume stepped"),
            key_frame: key_frame.expect("the key was pressed"),
            gain_frame: gain_frame.expect("a band rose"),
            fade_start: fade_start.expect("the fade started"),
            toasts,
        }
    })
}

/// `buffer` with every cell drawn `alpha` of the way to the ground colour: 0 leaves it as it
/// is, 1 makes it bare ground. Dim and reversed are applied to the colours, so what is left of
/// the modifier is bold.
pub(in crate::tui::screenshots) fn faded(
    buffer: &Buffer,
    alpha: f32,
    palette: &Palette,
) -> Result<Buffer, String> {
    let mut out = buffer.clone();
    for (cell, source) in out.content.iter_mut().zip(buffer.content()) {
        let inked = ink(source, palette)?;
        let (fr, fg, fb) = mix(inked.fg, palette.ground, alpha);
        let (br, bg, bb) = mix(inked.bg, palette.ground, alpha);
        cell.fg = Color::Rgb(fr, fg, fb);
        cell.bg = Color::Rgb(br, bg, bb);
        cell.modifier = if inked.bold {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
    }
    Ok(out)
}
