// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The last half hour of one device's readings: every status the actor publishes and the whole
//! state recorded at each connect, binned by the
//! second ([`Window::DAEMON`]), which a front end's graphs and the web UI's strip start from,
//! and the presets recalled on it.

use std::time::{Duration, Instant};

use svolder_ipc::{RecallOut, SeriesOut};
use svolder_profiles::Profile;
use svolder_profiles::history::{Readings, Window};
use svolder_proto::{Message, Value};

/// One device's recent readings.
#[derive(Debug)]
pub struct Ring {
    readings: Readings,
    /// The presets recalled, oldest first, and when.
    recalls: std::collections::VecDeque<(Instant, String)>,
}

impl Default for Ring {
    fn default() -> Self {
        Ring {
            readings: Readings::new(Window::DAEMON),
            recalls: std::collections::VecDeque::new(),
        }
    }
}

impl Ring {
    /// Note that the preset `name` was recalled at `at`; recalls older than half an hour go, and
    /// the oldest past the window's capacity.
    pub fn recalled(&mut self, name: &str, at: Instant) {
        self.recalls.push_back((at, name.to_string()));
        while self.recalls.len() > Window::DAEMON.capacity
            || self
                .recalls
                .front()
                .is_some_and(|(then, _)| at.saturating_duration_since(*then) > Window::DAEMON.keep)
        {
            self.recalls.pop_front();
        }
    }

    /// The recalls inside the `span` before `now`, at the wall clock `now_ms` stands for at
    /// `now`, oldest first.
    pub fn recalls_out(&self, now: Instant, now_ms: u64, span: Duration) -> Vec<RecallOut> {
        self.recalls
            .iter()
            .filter_map(|(at, name)| {
                let age = now.checked_duration_since(*at)?;
                (age <= span).then(|| RecallOut {
                    at_ms: now_ms
                        .saturating_sub(u64::try_from(age.as_millis()).unwrap_or(u64::MAX)),
                    name: name.clone(),
                })
            })
            .collect()
    }

    /// Record `message` as of `at`: its number, a bool as 0 or 1; a data payload is no number
    /// and is left out. `profile` decides how its series joins the samples of one bin.
    pub fn record(&mut self, profile: &Profile, message: &Message, at: Instant) {
        if let Some(value) = number(&message.value) {
            self.readings.record(profile, message.id, at, value);
        }
    }

    /// Record every number of `messages` as of `at`, as if each had been reported then: the
    /// session's whole state after a connect's refresh.
    pub fn record_all<'a>(
        &mut self,
        profile: &Profile,
        messages: impl IntoIterator<Item = &'a Message>,
        at: Instant,
    ) {
        for message in messages {
            self.record(profile, message, at);
        }
    }

    /// Every series with a sample at or before the `span` before `now`, at the wall clock
    /// `now_ms` stands for at `now`, keyed through `profile` when there is one. A series
    /// starts with its newest sample from before the span, at its own time, which says what
    /// held when the span began; the samples inside the span follow.
    pub fn series_out(
        &self,
        profile: Option<&Profile>,
        now: Instant,
        now_ms: u64,
        span: Duration,
    ) -> Vec<SeriesOut> {
        self.readings
            .iter()
            .filter_map(|(id, series)| {
                let mut points: Vec<(u64, f64)> = Vec::new();
                let mut carried = None;
                for (at, value) in series.samples() {
                    let Some(age) = now.checked_duration_since(at) else {
                        continue;
                    };
                    let ms = u64::try_from(age.as_millis()).unwrap_or(u64::MAX);
                    let point = (now_ms.saturating_sub(ms), value);
                    if age <= span {
                        points.push(point);
                    } else {
                        carried = Some(point);
                    }
                }
                points.splice(0..0, carried);
                (!points.is_empty()).then(|| SeriesOut {
                    id: id.to_string(),
                    key: profile
                        .and_then(|p| p.control_by_id(id))
                        .map(|c| c.key.clone()),
                    aggregate: series.aggregate(),
                    points,
                })
            })
            .collect()
    }
}

/// A status's number: a bool as 0 or 1, an integer as itself; `None` for a data payload.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(on) => Some(f64::from(u8::from(*on))),
        other => other.as_i64().map(|raw| raw as f64),
    }
}

#[cfg(test)]
mod tests;
