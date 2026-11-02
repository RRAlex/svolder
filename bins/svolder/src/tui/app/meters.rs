// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The meters and the level the animations follow: each meter's peak marker, the level
//! smoothed from the meters or from the host's capture, and when that capture runs.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use svolder_host::{Block, Frame};
use svolder_profiles::{Control, ControlKind, Placed};
use svolder_proto::FunctionId;

use crate::tui::audio_level::{LevelFrom, LevelSource, MeterLevel};
use crate::tui::layout_tier::Tier;

use super::{App, Reading};

/// How long a meter's peak marker stays where the loudest reading put it, in milliseconds.
const PEAK_HOLD_MS: u64 = 400;
/// How long the marker then takes to fall back onto the reading behind it, in milliseconds.
const PEAK_DECAY_MS: u64 = 800;
/// How much of the way to a new meter reading the smoothed level moves in one
/// [`METER_PERIOD`]. A handful of readings settle on a new level, so the borders that breathe
/// with it follow the music rather than flickering with every sample.
pub(super) const LEVEL_ALPHA: f64 = 0.35;
/// How long after a scaled meter's last live reading the meters still count as streaming: the
/// level comes from them while they do, and from the host's capture once they have been silent
/// this long — long enough that a meter reporting every few seconds never hands the level over
/// between two of its readings.
pub(super) const STREAMING_FOR: Duration = Duration::from_secs(10);

/// How often a streaming device's meters report — ten times a second, as the fake amp and the
/// simulator stream them — which is the span one [`LEVEL_ALPHA`] step of the meters' level
/// answers. The host's level is smoothed against real time at that same pace, so a level from
/// the host's capture follows the music as a level from the meters does, however many blocks
/// it arrives in, and the loop redraws what follows it at that pace too
/// ([`App::redraws_host_level`]).
pub(crate) const METER_PERIOD: Duration = svolder_ble::SIGNAL_PERIOD;

/// How much of the way to a fresh level a smoothed level moves in `elapsed`: `1 - exp(-dt/τ)`,
/// with τ the time constant that makes it [`LEVEL_ALPHA`] over one [`METER_PERIOD`]. Steps over
/// consecutive spans compose into the step over their sum, so the level stands in the same
/// place after the same time however often it is updated.
fn level_step(elapsed: Duration) -> f64 {
    let tau = METER_PERIOD.as_secs_f64() / -(1.0 - LEVEL_ALPHA).ln();
    1.0 - (-elapsed.as_secs_f64() / tau).exp()
}

/// One step of that smoothing: `held` moved `step` of the way onto `fresh`, and left where it
/// is for a side this reading said nothing about.
fn smoothed(held: f64, fresh: Option<f64>, step: f64) -> f64 {
    match fresh {
        Some(level) => held + step * (level - held),
        None => held,
    }
}

/// Where one meter's peak marker stands: the loudest reading of the last moment, when it had
/// it, and the reading behind it that the marker falls onto.
#[derive(Debug, Clone, Copy)]
pub(super) struct Peak {
    /// The loudest reading.
    held: i64,
    /// When it arrived.
    since: Instant,
    /// The reading standing when the hold ended, or any quieter one after it: where the
    /// marker lands. A reading that climbs back after the fall began never lifts it.
    behind: i64,
}

/// What the meters hold: each meter's peak marker, the level smoothed from the readings, and
/// when the meters last streamed.
#[derive(Default)]
pub struct Meters {
    /// The loudest reading each meter has had lately and when it had it, by function
    /// address: what [`App::peak`] holds a marker at and then lets fall.
    pub(super) peaks: BTreeMap<FunctionId, Peak>,
    /// How loud the meters read, smoothed over the readings that have arrived; `None` until a
    /// meter with a scale of its own has reported. What [`App::level`] answers with while the
    /// meters stream. Follows the meters only: with readings stale or the link down it stays
    /// where the last reading left it, as the meter bars themselves do.
    pub(super) meter_level: Option<f64>,
    /// When the readings last moved [`Meters::meter_level`], which the next smoothing step
    /// measures its time from.
    pub(super) meter_level_at: Option<Instant>,
    /// How loud each scaled meter reads, smoothed the same way over the same readings, in the
    /// profile's order, each with the channel label its profile gives it (`M`, `L`/`R`, `1`-`5`);
    /// `None` until a meter with a scale of its own has reported. What [`App::levels`] answers
    /// with while the meters stream.
    pub(super) channel_levels: Option<Vec<(String, f64)>>,
    /// When a scaled meter last reported on the device's feed: says the meters are streaming
    /// ([`App::meters_streaming`]). A refresh or reconnect reading the whole device does not
    /// count as streaming and leaves this where it was.
    pub(super) meters_heard: Option<Instant>,
}

impl App {
    /// Fold the reading `id` has just carried into what the meters say: the peak its own
    /// marker rests at, and the smoothed level every meter together comes to.
    ///
    /// A reading at or above the marker takes it there and starts its hold again; a quieter
    /// one leaves it where it was, for [`App::peak`] to let fall. A reading heard while the
    /// meters are dead gives no peak.
    pub(super) fn metered(&mut self, id: FunctionId, now: Instant) {
        let raw = self.readings.values.get(&id).and_then(Reading::raw_i64);
        if let Some(raw) = raw.filter(|_| self.meters_live()) {
            match self.meters.peaks.get_mut(&id) {
                Some(peak) if raw < peak.held => {
                    let holding = now.saturating_duration_since(peak.since)
                        <= Duration::from_millis(PEAK_HOLD_MS);
                    peak.behind = match holding {
                        true => raw,
                        false => peak.behind.min(raw),
                    };
                }
                _ => {
                    self.meters.peaks.insert(
                        id,
                        Peak {
                            held: raw,
                            since: now,
                            behind: raw,
                        },
                    );
                }
            }
        }
        let Some(level) = (MeterLevel { app: self }).level() else {
            return;
        };
        // One step of exponential smoothing over the time since the last reading, so the level
        // settles in the same wall time whatever rate the device reports at; the first one
        // there is a level at all is the level itself, so the borders start where the meters
        // are.
        let step = self
            .meters
            .meter_level_at
            .map(|before| level_step(now.saturating_duration_since(before)));
        self.meters.meter_level_at = Some(now);
        self.meters.meter_level = Some(match (self.meters.meter_level, step) {
            (Some(held), Some(step)) => held + step * (level - held),
            _ => level,
        });
        let fresh = self.channel_readings();
        self.meters.channel_levels = Some(match &self.meters.channel_levels {
            Some(held) if held.len() == fresh.len() => held
                .iter()
                .zip(&fresh)
                .map(|((label, held), (_, level))| {
                    (label.clone(), smoothed(*held, *level, step.unwrap_or(1.0)))
                })
                .collect(),
            _ => fresh
                .into_iter()
                .map(|(label, level)| (label, level.unwrap_or(0.0)))
                .collect(),
        });
    }

    /// How loud each meter with a scale reads this moment, on its own scale, in the profile's
    /// own order, alongside the channel label its profile gives it (`M`, `L`/`R`, `1`-`5`, or
    /// empty where it names none). `None` for a meter nothing has reported a reading for yet,
    /// and for every meter while the meters are dead.
    ///
    /// This is the rule the smoothed channel levels read every meter by, one entry per scaled
    /// meter — [`MeterLevel`] folds the same readings down to the loudest of them instead.
    pub(super) fn channel_readings(&self) -> Vec<(String, Option<f64>)> {
        let live = self.meters_live();
        self.profile
            .controls
            .iter()
            .filter(|control| control.kind == ControlKind::Meter)
            .filter_map(|control| Some((control, self.placed(control)?)))
            .map(|(control, placed)| {
                let level = self
                    .readings
                    .values
                    .get(&control.id)
                    .and_then(Reading::raw_i64)
                    .filter(|_| live)
                    .map(|raw| placed.fraction(raw as f64));
                (control.channel.clone().unwrap_or_default(), level)
            })
            .collect()
    }

    /// `control`'s scale with its zero settled from this app's readings: a floor it follows is
    /// read from the last value that function reported.
    pub fn placed(&self, control: &Control) -> Option<Placed> {
        self.profile.placed(control, |id| {
            self.readings.values.get(&id).and_then(Reading::raw_i64)
        })
    }

    /// Whether the device's meters are live: its meter switch does not read off, and it neither
    /// reads asleep nor warming up ([`svolder_profiles::Profile::meters_live`]). A dead meter
    /// draws dim and empty, holds no peak and gives no level.
    pub fn meters_live(&self) -> bool {
        self.profile.meters_live(|id| {
            self.readings
                .values
                .get(&id)
                .map(|reading| reading.value.reads_true())
        })
    }

    /// Where the peak marker on one meter's bar sits as of `now`, as a fraction of the bar's
    /// live part: the loudest reading of the last moment, held for [`PEAK_HOLD_MS`] and then
    /// falling evenly in that fraction onto the reading behind it over [`PEAK_DECAY_MS`].
    ///
    /// `None` once it has met the current reading, for a function that is not a meter with a
    /// scale, while the meters are dead, and wherever the peak-hold animation is switched off:
    /// a marker is never drawn at or below the bar's own fill, where it would mark nothing the
    /// bar does not already show.
    pub fn peak(&self, id: FunctionId, now: Instant) -> Option<f64> {
        if !self.peak_hold || !self.meters_live() {
            return None;
        }
        let placed = self.placed(self.profile.control_by_id(id)?)?;
        let peak = *self.meters.peaks.get(&id)?;
        let raw = self.readings.values.get(&id)?.raw_i64()?;
        let elapsed = u64::try_from(now.saturating_duration_since(peak.since).as_millis()).ok()?;
        let falling = elapsed.saturating_sub(PEAK_HOLD_MS);
        if falling >= PEAK_DECAY_MS {
            return None;
        }
        let held = placed.fraction(peak.held as f64);
        let behind = placed.fraction(peak.behind as f64);
        let at = held - (held - behind) * falling as f64 / PEAK_DECAY_MS as f64;
        (at > placed.fraction(raw as f64)).then_some(at)
    }

    /// Whether any marker a bar can show is still falling as of `now`, which is what the loop
    /// redraws fast for.
    ///
    /// A meter whose profile names no scale has no bar to carry a marker, so its peak moves
    /// nothing on the screen and asks for no redraw: an amp that streams readings for a
    /// meter with no scale leaves the loop at its slow tick.
    pub fn peaking(&self, now: Instant) -> bool {
        self.meters
            .peaks
            .keys()
            .any(|id| self.peak(*id, now).is_some())
    }

    /// How loud what is playing is, `0.0..=1.0`, from where [`App::level_from`] says: the
    /// meters' smoothed level, or the capture's, the louder of its two channels, each smoothed
    /// the same way. `None` from nowhere, and before the source chosen has read anything.
    ///
    /// This is what the glow and the breathing borders follow.
    pub fn level(&self) -> Option<f64> {
        match self.level_from() {
            LevelFrom::Meters => self.meters.meter_level,
            LevelFrom::Host => self.host.level,
            LevelFrom::Nowhere => None,
        }
    }

    /// How loud each channel of what is playing is, from where [`App::level_from`] says: one
    /// level per scaled meter, in the profile's own order, or the capture's left and right.
    /// `None` from nowhere, and before the source chosen has read anything.
    ///
    /// This is what the terminal's title draws its bars from.
    pub fn levels(&self) -> Option<Vec<f64>> {
        match self.level_from() {
            LevelFrom::Meters => self
                .meters
                .channel_levels
                .as_ref()
                .map(|levels| levels.iter().map(|(_, level)| *level).collect()),
            LevelFrom::Host => self.host.channels.map(Vec::from),
            LevelFrom::Nowhere => None,
        }
    }

    /// Whether the device's scaled meters are streaming: one of them has reported on the
    /// device's feed within [`STREAMING_FOR`] of this app's clock, and the meters are live. The
    /// values a refresh or a reconnect reads are not a meter streaming, however recent, and a
    /// meter switched off hands the level back at once rather than after [`STREAMING_FOR`].
    pub fn meters_streaming(&self) -> bool {
        self.meters_live()
            && self
                .meters
                .meters_heard
                .is_some_and(|heard| self.now.saturating_duration_since(heard) < STREAMING_FOR)
    }

    /// Whether a meter with no scale has reported within [`STREAMING_FOR`] of this app's clock.
    /// A meter with no scale carries none, so [`App::meters_streaming`] never counts it;
    /// the level history still draws it.
    pub fn unscaled_meters_heard(&self) -> bool {
        self.profile
            .controls
            .iter()
            .filter(|control| control.kind == ControlKind::Meter && self.placed(control).is_none())
            .filter_map(|control| self.readings.values.get(&control.id))
            .any(|reading| self.now.saturating_duration_since(reading.received) < STREAMING_FOR)
    }

    /// Where the level the glow, the breathing borders and the title meter read comes from now:
    /// the device's own meters while they stream, else the capture of the host's sink while
    /// host audio shows and the capture runs, else nowhere. One source at a time, never both.
    pub fn level_from(&self) -> LevelFrom {
        if self.meters_streaming() {
            LevelFrom::Meters
        } else if self.host_shown() && self.host.capturing {
            LevelFrom::Host
        } else {
            LevelFrom::Nowhere
        }
    }

    /// Whether the level comes from the host's capture and a decoration drawn afresh every
    /// frame follows it — the breathing borders, or the hue drift — which the loop redraws at
    /// the meters' own pace for ([`METER_PERIOD`]), as an amp's readings redraw it. The focus
    /// runner reads the level only as a lap starts and the title meter paces itself, so
    /// neither asks for it.
    pub fn redraws_host_level(&self) -> bool {
        self.level_from() == LevelFrom::Host
            && (self.breathing || (self.effects && self.tui.effects.hue_drift))
    }

    /// Whether anything this session draws follows a level: the title meter, the breathing
    /// borders, and — while the animation layer runs — the hue drift and the focus runner's
    /// pace.
    fn wants_level(&self) -> bool {
        let effects = &self.tui.effects;
        self.tui.title_meter
            || self.breathing
            || (self.effects && (effects.hue_drift || effects.frames))
    }

    /// The sink the capture of its monitor runs on now, or `None` while nothing needs one:
    /// while host audio shows, and either the Spectrum row is on the screen or something wants
    /// a level no meter of the device's streams. Only ever the node the host's default sink
    /// named itself by: before the sink has read, and for a sink that named no node, there is
    /// nothing to capture.
    pub fn capture_target(&self) -> Option<&str> {
        if !self.host_shown() {
            return None;
        }
        let target = self.host.sink.as_ref()?.target.as_deref()?;
        let for_level = self.wants_level() && !self.meters_streaming();
        (self.spectrum_shown() || for_level).then_some(target)
    }

    /// Whether the device screen draws the Host group's Spectrum row in a frame of `tier`:
    /// while host audio shows and the visualizer is on, in the wide or the tall tier — the
    /// 80×24 layout leaves it out.
    pub fn spectrum_drawn_in(&self, tier: Tier) -> bool {
        self.host_shown() && self.tui.visualizer && (tier.wide || tier.tall)
    }

    /// Whether the Spectrum row is on the screen: the frame last drawn put it there, inside
    /// the Controls pane and under no box ([`Host::spectrum_on_screen`]), and host audio
    /// shows.
    ///
    /// [`Host::spectrum_on_screen`]: crate::tui::host::Host::spectrum_on_screen
    fn spectrum_shown(&self) -> bool {
        self.host_shown() && self.host.spectrum_on_screen
    }

    /// Whether the Spectrum row is on the screen and moving: a band the last block left above
    /// silence is still falling back, which the loop redraws fast for.
    pub fn spectrum_moving(&self) -> bool {
        self.spectrum_shown() && self.host.bands.moving(self.now)
    }

    /// Fold in a block the capture of the host's sink read, as of `now`: its bands land on the
    /// Spectrum row, and its two channels move the capture's levels toward them by the time
    /// since the block before — [`LEVEL_ALPHA`] of the way for each [`METER_PERIOD`], the pace
    /// the meters' level moves at on a device that streams them — so the level stands in the
    /// same place after a quarter of a second whether that quarter came in twelve blocks or in
    /// one. The first block is its levels themselves.
    pub fn heard_block(&mut self, block: &Block, now: Instant) {
        self.now = now;
        self.host.bands.land(&block.bands, now);
        let read = |level: f32| match level.is_finite() {
            true => f64::from(level).clamp(0.0, 1.0),
            false => 0.0,
        };
        let (left, right) = (read(block.left), read(block.right));
        let loudest = left.max(right);
        self.readings.histories.record_host(now, left, right);
        let step = self.host.level_at.map_or(1.0, |before| {
            level_step(now.saturating_duration_since(before))
        });
        let toward = |held: f64, fresh: f64| held + step * (fresh - held);
        self.host.level_at = Some(now);
        self.host.level = Some(match self.host.level {
            Some(held) => toward(held, loudest),
            None => loudest,
        });
        self.host.channels = Some(match self.host.channels {
            Some([held_left, held_right]) => [toward(held_left, left), toward(held_right, right)],
            None => [left, right],
        });
    }

    /// Fold in a frame the daemon analysed as it does a block of its own capture, and let a beat
    /// in it pulse: `frame.block()` lands the bands and the levels, and the pulse is
    /// [`Host::pulse`](crate::tui::host::Host::pulse).
    pub fn heard_frame(&mut self, frame: &Frame, now: Instant) {
        self.heard_block(&frame.block(), now);
        if frame.beat {
            self.host.beat_landed(now);
        }
    }

    /// How strongly the last beat of the daemon's frames pulses the Spectrum row at the app's
    /// clock, `1.0` as it lands down to `0.0`: while the host's capture runs, whatever the level's
    /// source is (the row's bands are the host's even where an amp's meters stream), and `0.0`
    /// while no beat has landed (the loop's own capture finds none).
    pub fn row_pulse(&self) -> f32 {
        match self.host_shown() && self.host.capturing {
            true => self.host.pulse(self.now),
            false => 0.0,
        }
    }

    /// How strongly the last beat pulses the borders: as [`App::row_pulse`], but only while the
    /// host's capture is the level the borders follow, which an amp's own meters are not.
    pub fn pulse(&self) -> f32 {
        match self.level_from() {
            LevelFrom::Host => self.host.pulse(self.now),
            _ => 0.0,
        }
    }

    /// Whether a beat's pulse is on something drawn and still fading, which the loop redraws
    /// fast for: the Spectrum row on the screen, or the borders breathing.
    pub fn pulsing(&self) -> bool {
        (self.spectrum_shown() && self.row_pulse() > 0.0) || (self.breathing && self.pulse() > 0.0)
    }

    /// Whether `id` is a meter the device streams.
    pub(super) fn is_meter(&self, id: FunctionId) -> bool {
        self.profile
            .control_by_id(id)
            .is_some_and(|control| control.kind == ControlKind::Meter)
    }
}
