// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded histories of readings: a series per function, binned and aged by a [`Window`], its
//! sampling into columns over a span of time, a device's link changes, and the scaling of
//! columns onto 0 to 1. Nothing here draws: the TUI's braille graphs and the daemon's ring of
//! readings both build on it.
//!
//! A [`Series`] keeps at most one sample per bin, at most the window's capacity and nothing
//! older than its keep. [`columns`] turns it into `count` columns covering the span that ends at
//! `now`; [`normalise`] puts them on a 0 to 1 scale.
//!
//! Time is `std::time::Instant` throughout and every subtraction is checked: a `now` earlier
//! than the newest sample (the clock went back) reads that sample as not yet happened, and
//! never panics or reports a negative span.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use svolder_proto::FunctionId;

use crate::{ControlKind, Placed, Profile};

/// How a series bins and ages its samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// The width of the time bin one sample stands for.
    pub bin: Duration,
    /// The age past which a sample is dropped.
    pub keep: Duration,
    /// The most samples a series holds.
    pub capacity: usize,
}

impl Window {
    /// The TUI's graphs: 250 ms bins for 60 minutes, 14,400 samples.
    pub const TUI: Window = Window {
        bin: Duration::from_millis(250),
        keep: Duration::from_secs(60 * 60),
        capacity: 14_400,
    };

    /// The daemon's ring: 1 s bins for 30 minutes, 1,800 samples.
    pub const DAEMON: Window = Window {
        bin: Duration::from_secs(1),
        keep: Duration::from_secs(30 * 60),
        capacity: 1_800,
    };
}

/// How the samples inside one bin or one column become a single value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Aggregate {
    /// The largest, so a peak never vanishes (meters, levels and faults).
    Max,
    /// The latest, for everything else.
    Last,
    /// A kind a later daemon sent that this build does not know; read as [`Aggregate::Last`].
    #[cfg_attr(feature = "serde", serde(other))]
    Unknown,
}

/// A bounded history of `(when, value)`.
#[derive(Debug, Clone)]
pub struct Series {
    /// The samples, oldest first.
    samples: VecDeque<(Instant, f64)>,
    /// When the bin the newest sample stands for began.
    bin_start: Option<Instant>,
    /// What a sample landing in the newest bin does to it.
    aggregate: Aggregate,
    /// How the samples are binned and aged.
    window: Window,
}

impl Series {
    /// An empty series whose bins keep their `aggregate`, binned and aged by `window`.
    pub fn new(aggregate: Aggregate, window: Window) -> Series {
        Series {
            samples: VecDeque::new(),
            bin_start: None,
            aggregate,
            window,
        }
    }

    /// Records `value` at `at`: into the newest sample's bin when it lands within the window's
    /// bin of that bin's start, as a new sample otherwise. A value that is not finite is
    /// ignored. A sample is dropped once the one after it is at least the window's keep old at
    /// `at`, so the newest sample at or before the keep stays to say what held before the next;
    /// the capacity is kept too.
    pub fn push(&mut self, at: Instant, value: f64) {
        if !value.is_finite() {
            return;
        }
        let same_bin = match (self.bin_start, self.samples.back_mut()) {
            (Some(start), Some(_)) => at.saturating_duration_since(start) < self.window.bin,
            _ => false,
        };
        match self.samples.back_mut() {
            Some(newest) if same_bin => {
                newest.1 = match self.aggregate {
                    Aggregate::Max => newest.1.max(value),
                    Aggregate::Last | Aggregate::Unknown => value,
                };
            }
            _ => {
                self.samples.push_back((at, value));
                self.bin_start = Some(at);
            }
        }
        // The newest sample at or before the keep stays: it says what held before the next one.
        while self.samples.len() > self.window.capacity
            || self.samples.get(1).is_some_and(|&(next, _)| {
                at.checked_duration_since(next)
                    .is_some_and(|age| age >= self.window.keep)
            })
        {
            self.samples.pop_front();
        }
    }

    /// The samples, oldest first.
    pub fn samples(&self) -> impl Iterator<Item = (Instant, f64)> + '_ {
        self.samples.iter().copied()
    }

    /// Puts `samples` (oldest first) before the samples the series holds: one at or after its
    /// oldest, and one that is not finite, is dropped, so a seed never doubles what the series
    /// saw itself. The capacity holds as for [`Series::push`], the newest kept.
    pub fn seed(&mut self, samples: impl IntoIterator<Item = (Instant, f64)>) {
        let oldest = self.samples.front().map(|&(at, _)| at);
        let earlier: Vec<(Instant, f64)> = samples
            .into_iter()
            .filter(|(at, value)| value.is_finite() && oldest.is_none_or(|first| *at < first))
            .collect();
        for sample in earlier.into_iter().rev() {
            self.samples.push_front(sample);
        }
        if self.bin_start.is_none() {
            self.bin_start = self.samples.back().map(|&(at, _)| at);
        }
        while self.samples.len() > self.window.capacity {
            self.samples.pop_front();
        }
    }

    /// What a sample landing in a bin does to it.
    pub fn aggregate(&self) -> Aggregate {
        self.aggregate
    }

    /// Forgets every sample.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.bin_start = None;
    }

    /// How many samples it holds.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// One series per function, by address, created on a function's first numeric reading.
#[derive(Debug, Clone)]
pub struct Readings {
    series: BTreeMap<FunctionId, Series>,
    window: Window,
}

impl Readings {
    /// No series yet; each one made is binned and aged by `window`.
    pub fn new(window: Window) -> Readings {
        Readings {
            series: BTreeMap::new(),
            window,
        }
    }

    /// Records `value` for function `id` at `at`. The series is created on the first reading,
    /// aggregating by maximum for a meter, a level or a fault control (a peak, or a fault that
    /// came and went, shows in its column) and by the latest value for every other function,
    /// those `profile` does not name included.
    pub fn record(&mut self, profile: &Profile, id: FunctionId, at: Instant, value: f64) {
        let window = self.window;
        self.series
            .entry(id)
            .or_insert_with(|| Series::new(aggregate_of(profile, id), window))
            .push(at, value);
    }

    /// The series of function `id`, once it has had a numeric reading.
    pub fn series(&self, id: FunctionId) -> Option<&Series> {
        self.series.get(&id)
    }

    /// Every series, by function address.
    pub fn iter(&self) -> impl Iterator<Item = (FunctionId, &Series)> {
        self.series.iter().map(|(id, series)| (*id, series))
    }

    /// Puts `samples` before function `id`'s series ([`Series::seed`]), making it with
    /// `aggregate` when there is none yet.
    pub fn seed(
        &mut self,
        id: FunctionId,
        aggregate: Aggregate,
        samples: impl IntoIterator<Item = (Instant, f64)>,
    ) {
        let window = self.window;
        self.series
            .entry(id)
            .or_insert_with(|| Series::new(aggregate, window))
            .seed(samples);
    }
}

/// How the series of function `id` aggregates: by maximum for a meter, a level or a fault.
fn aggregate_of(profile: &Profile, id: FunctionId) -> Aggregate {
    let peaks = profile.control_by_id(id).is_some_and(|c| {
        matches!(
            c.kind,
            ControlKind::Meter | ControlKind::Level | ControlKind::Fault
        )
    });
    match peaks {
        true => Aggregate::Max,
        false => Aggregate::Last,
    }
}

/// `count` half-cell columns covering `[now - span, now)`: each the series' own aggregate of the
/// samples inside it, the previous value carried through a column with none, `None` before the
/// first sample. A sample older than the span still carries into the columns after it; one at or
/// after `now` is not counted.
pub fn columns(series: &Series, now: Instant, span: Duration, count: usize) -> Vec<Option<f64>> {
    let mut out: Vec<Option<f64>> = vec![None; count];
    if count == 0 || span.is_zero() {
        return out;
    }
    let span_ns = span.as_nanos() as f64;
    // The samples older than the span come first: the newest of them carries into the window.
    let first = series
        .samples
        .partition_point(|&(at, _)| now.checked_duration_since(at).is_some_and(|age| age > span));
    let before = first.checked_sub(1).map(|i| series.samples[i].1);
    for &(at, value) in series.samples.iter().skip(first) {
        let Some(age) = now.checked_duration_since(at).filter(|age| !age.is_zero()) else {
            continue;
        };
        let from_start = span_ns - age.as_nanos() as f64;
        let index = ((from_start / span_ns * count as f64) as usize).min(count - 1);
        out[index] = Some(match (out[index], series.aggregate) {
            (Some(held), Aggregate::Max) => held.max(value),
            _ => value,
        });
    }
    let mut carried = before;
    for column in &mut out {
        match column {
            Some(value) => carried = Some(*value),
            None => *column = carried,
        }
    }
    out
}

/// The most link changes the history holds.
const LINK_CAP: usize = 4096;

/// The age past which a link change is dropped, in milliseconds: a day, the longest link span.
const LINK_KEEP_MS: u64 = 24 * 3600 * 1000;

/// The changes of the device's link, oldest first, in wall-clock unix milliseconds: seeded from
/// the daemon's history and extended by what its holder sees itself.
#[derive(Debug, Clone, Default)]
pub struct Link {
    changes: VecDeque<(u64, bool)>,
}

impl Link {
    /// Replaces the history with `changes`.
    pub fn seed(&mut self, changes: impl IntoIterator<Item = (u64, bool)>) {
        self.changes = changes.into_iter().collect();
        self.trim(self.changes.back().map_or(0, |&(at, _)| at));
    }

    /// Joins the daemon's `changes` to the history held: the held changes stamped before the
    /// daemon's first stay, the daemon's follow, and a daemon change to the state the held ones
    /// end in adds nothing. With nothing held it is the daemon's history alone.
    pub fn merge(&mut self, changes: impl IntoIterator<Item = (u64, bool)>) {
        let theirs: Vec<(u64, bool)> = changes.into_iter().collect();
        let Some(&(first, _)) = theirs.first() else {
            return;
        };
        self.changes.retain(|&(at, _)| at < first);
        let newest = theirs.last().map_or(first, |&(at, _)| at);
        for (at, connected) in theirs {
            if self
                .changes
                .back()
                .is_none_or(|&(_, last)| last != connected)
            {
                self.changes.push_back((at, connected));
            }
        }
        self.trim(newest);
    }

    /// Records the link as `connected` from `at_ms` on. A change to the state it is already in
    /// adds nothing. What the cap and a day's age no longer hold is dropped, keeping the change
    /// before that day, which is the state at its start.
    pub fn record(&mut self, at_ms: u64, connected: bool) {
        if self
            .changes
            .back()
            .is_some_and(|&(_, last)| last == connected)
        {
            return;
        }
        self.changes.push_back((at_ms, connected));
        self.trim(at_ms);
    }

    fn trim(&mut self, newest: u64) {
        let oldest = newest.saturating_sub(LINK_KEEP_MS);
        while self.changes.len() > 1 && self.changes[1].0 <= oldest {
            self.changes.pop_front();
        }
        while self.changes.len() > LINK_CAP {
            self.changes.pop_front();
        }
    }

    /// `count` half-cell columns covering `[now_ms - span, now_ms)` as 1.0 for connected and 0.0
    /// for down. A column is down if the link was down at any moment inside it, so a drop
    /// shorter than a column still shows; otherwise it is up, and `None` before the first
    /// change. A change stamped after `now_ms` (the clock stepped back) has not happened yet and
    /// is not counted.
    pub fn columns(&self, now_ms: u64, span: Duration, count: usize) -> Vec<Option<f64>> {
        let mut out: Vec<Option<f64>> = vec![None; count];
        let span_ms = span.as_millis() as i128;
        if count == 0 || span_ms == 0 {
            return out;
        }
        let now = i128::from(now_ms);
        let start = now - span_ms;
        // The column a moment of the window falls in.
        let column = |ms: i128| {
            ((ms - start) * count as i128 / span_ms).clamp(0, count as i128 - 1) as usize
        };
        let seen: Vec<(i128, bool)> = self
            .changes
            .iter()
            .filter(|&&(at, _)| at <= now_ms)
            .map(|&(at, connected)| (i128::from(at), connected))
            .collect();
        for (i, &(at, connected)) in seen.iter().enumerate() {
            let end = seen.get(i + 1).map_or(now, |&(next, _)| next.max(at));
            if at < start && end <= start {
                continue;
            }
            let from = column(at.max(start));
            // The last moment the state held: a zero-width interval still has its own moment.
            let to = column((end - 1).max(at).max(start));
            for slot in &mut out[from..=to] {
                match (connected, *slot) {
                    (false, _) => *slot = Some(0.0),
                    (true, None) => *slot = Some(1.0),
                    (true, Some(_)) => {}
                }
            }
        }
        out
    }
}

/// `columns` on a 0 to 1 scale: through the control's own scale where it has one, else between
/// the smallest and largest of the columns shown. A flat window sits at 0.
pub fn normalise(placed: Option<Placed>, columns: &[Option<f64>]) -> Vec<Option<f64>> {
    normalise_together(placed, &[columns]).remove(0)
}

/// Several windows on one 0 to 1 scale: `placed` where there is one, else the scale they
/// share (the two channels of a mirrored graph).
pub fn normalise_together(
    placed: Option<Placed>,
    windows: &[&[Option<f64>]],
) -> Vec<Vec<Option<f64>>> {
    let (min, max) = windows
        .iter()
        .flat_map(|window| window.iter().flatten())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    windows
        .iter()
        .map(|window| {
            window
                .iter()
                .map(|column| {
                    let value = (*column)?;
                    Some(match placed {
                        Some(placed) => placed.fraction(value),
                        None if max > min => ((value - min) / (max - min)).clamp(0.0, 1.0),
                        None => 0.0,
                    })
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests;
