// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The strip along the bottom of the page: one bar a second of what the device reported, from
//! the daemon's ring, and marks for what happened. A DAC's bar is its volume's place in its
//! range, tinted by the sample rate, red where the curve of that second could clip, dim
//! while muted; an amp's is its louder meter on its scale. A second without a link has no bar.

use std::collections::HashMap;

use serde::Serialize;
use svolder_ipc::{LinkChange, RecallOut, SeriesOut};
use svolder_profiles::{ControlKind, Profile, roles, tone::response};
use svolder_proto::FunctionId;

use super::meters::RED_FROM;
use super::{knob_controls, risk_step};

/// The longest a label runs before an ellipsis.
const LABEL_MAX: usize = 14;

/// What the ring answered, as the strip reads it.
pub(crate) struct Past<'a> {
    pub(crate) now_ms: u64,
    pub(crate) series: &'a [SeriesOut],
    pub(crate) recalls: &'a [RecallOut],
    pub(crate) link: &'a [LinkChange],
}

/// The strip's bars and marks.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Strip {
    /// `dac` or `amp`: how the page reads the bars.
    pub(crate) kind: &'static str,
    pub(crate) bars: Vec<Bar>,
    pub(crate) marks: Vec<Mark>,
}

#[cfg(test)]
impl Strip {
    /// A strip with no bars and one mark labelled `text`.
    pub(crate) fn for_test_with_mark(text: &str) -> Strip {
        Strip {
            kind: "dac",
            bars: Vec::new(),
            marks: vec![Mark {
                t: 1,
                text: text.to_string(),
                tint: "acc",
            }],
        }
    }
}

/// One second's bar.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Bar {
    /// The second, in unix seconds.
    pub(crate) t: u64,
    /// Its height, 0 to 1.
    pub(crate) h: f64,
    /// Its colour: `ramp`, `gold`, `target`, `glow`, `red` or `brass`.
    pub(crate) tint: &'static str,
    /// Whether it is dim: muted.
    pub(crate) dim: bool,
    /// Whether it is the stub silence draws.
    pub(crate) stub: bool,
}

/// One mark: a line at its second and a label.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Mark {
    pub(crate) t: u64,
    pub(crate) text: String,
    /// Its colour: `dim`, `acc`, `gold`, `target`, `glow`, `ramp`, `good` or `warn`.
    pub(crate) tint: &'static str,
}

/// The value of `series` in second `t`: its last point before the second ends.
fn at(series: Option<&SeriesOut>, t: u64) -> Option<f64> {
    let end = (t + 1) * 1000;
    series?
        .points
        .iter()
        .rev()
        .find(|(ms, _)| *ms < end)
        .map(|(_, v)| *v)
}

/// Whether the link was up in second `t`: its last change before the second ends; up when
/// nothing says otherwise.
fn linked(link: &[LinkChange], t: u64) -> bool {
    let end = (t + 1) * 1000;
    link.iter()
        .rev()
        .find(|c| c.at_ms < end)
        .is_none_or(|c| c.connected)
}

/// The tint of a sample rate's family: 44.1/48 kHz the ramp, 88.2/96 gold, 176.4/192 the
/// target's, 352.8/384 the glow's (the profile's values 1–8).
fn rate_tint(rate: Option<f64>) -> &'static str {
    match rate.map(|r| r as i64) {
        Some(3 | 4) => "gold",
        Some(5 | 6) => "target",
        Some(7 | 8) => "glow",
        _ => "ramp",
    }
}

/// `text` cut to [`LABEL_MAX`] characters with an ellipsis.
fn label(text: &str) -> String {
    match text.chars().count() > LABEL_MAX {
        true => text.chars().take(LABEL_MAX - 1).chain(['…']).collect(),
        false => text.to_string(),
    }
}

/// The last whole second before `now_ms`, in unix seconds: the newest second a strip covers.
pub(crate) fn last_second(now_ms: u64) -> u64 {
    (now_ms / 1000).saturating_sub(1)
}

/// The last `secs` whole seconds before `past.now_ms`, as bars and marks, for `profile`.
pub(crate) fn strip(profile: &Profile, past: &Past, secs: u64) -> Strip {
    let find = |key: &str| {
        let id = profile.control_by_key(key)?.id.to_string();
        past.series.iter().find(|s| s.id == id)
    };
    let last = last_second(past.now_ms);
    let first = last.saturating_sub(secs.saturating_sub(1));
    let volume = roles::volume(profile);
    let kind = if volume.is_some() { "dac" } else { "amp" };
    let mut risks: HashMap<Vec<i64>, Option<i64>> = HashMap::new();
    let mut bars = Vec::new();
    for t in first..=last {
        if !linked(past.link, t) {
            continue;
        }
        let bar = match volume {
            Some(control) => dac_bar(profile, control, past, t, &find, &mut risks),
            None => amp_bar(profile, past, t),
        };
        bars.extend(bar);
    }
    let mut marks: Vec<Mark> = Vec::new();
    for (key, change) in [
        ("input", Change::Name),
        ("sample_rate", Change::Rate),
        ("spdif_locked", Change::Lock),
    ] {
        let (Some(series), Some(control)) = (find(key), profile.control_by_key(key)) else {
            continue;
        };
        for pair in series.points.windows(2) {
            let ((_, before), (ms, now)) = (pair[0], pair[1]);
            let t = ms / 1000;
            if before == now || t < first || t > last {
                continue;
            }
            let name = || {
                let step = u8::try_from(now as i64).ok();
                step.and_then(|step| control.value_name(step))
                    .map_or_else(|| format!("{now}"), str::to_string)
            };
            let (text, tint) = match change {
                Change::Name => (name(), "dim"),
                Change::Rate => (name(), rate_tint(Some(now))),
                Change::Lock if now >= 0.5 => ("locked".to_string(), "good"),
                Change::Lock => ("unlocked".to_string(), "warn"),
            };
            marks.push(Mark {
                t,
                text: label(&text),
                tint,
            });
        }
    }
    for recall in past.recalls {
        let t = recall.at_ms / 1000;
        if (first..=last).contains(&t) {
            marks.push(Mark {
                t,
                text: label(&recall.name),
                tint: "acc",
            });
        }
    }
    marks.sort_by_key(|m| m.t);
    Strip { kind, bars, marks }
}

/// What a series' value change marks.
#[derive(Clone, Copy)]
enum Change {
    Name,
    Rate,
    Lock,
}

/// A DAC's bar in second `t`.
fn dac_bar<'a>(
    profile: &Profile,
    control: &svolder_profiles::Control,
    past: &'a Past,
    t: u64,
    find: &dyn Fn(&str) -> Option<&'a SeriesOut>,
    risks: &mut HashMap<Vec<i64>, Option<i64>>,
) -> Option<Bar> {
    let (lo, hi) = control.range?;
    let raw = at(find("volume"), t)? as i64;
    let h = ((raw - lo) as f64 / (hi - lo).max(1) as f64).clamp(0.0, 1.0);
    let silence = raw == lo
        && matches!(
            svolder_profiles::units::amount(control, lo),
            Some(svolder_profiles::units::Amount::Silence)
        );
    let muted = knob_controls(profile)
        .1
        .and_then(|m| at(past.series.iter().find(|s| s.id == m.id.to_string()), t))
        .is_some_and(|v| v >= 0.5);
    let risk = profile.eq.as_ref().and_then(|eq| {
        let ids: Vec<FunctionId> = std::iter::once(eq.mode_id())
            .chain((0..eq.bands.len()).flat_map(|b| {
                [
                    svolder_profiles::EqParam::Frequency,
                    svolder_profiles::EqParam::Gain,
                    svolder_profiles::EqParam::Q,
                ]
                .into_iter()
                .filter_map(move |p| eq.band_id(b, p))
            }))
            .collect();
        let values: Vec<i64> = ids
            .iter()
            .map(|id| {
                at(past.series.iter().find(|s| s.id == id.to_string()), t).map_or(-1, |v| v as i64)
            })
            .collect();
        *risks.entry(values.clone()).or_insert_with(|| {
            let raw = |id: FunctionId| {
                let at = ids.iter().position(|i| *i == id)?;
                (values[at] >= 0).then_some(values[at])
            };
            let peak = response::clipping_peak(eq, raw)?;
            risk_step(control, peak)
        })
    });
    let rate = at(find("sample_rate"), t);
    let tint = match risk.is_some_and(|r| raw >= r) {
        true => "red",
        false => rate_tint(rate),
    };
    Some(Bar {
        t,
        h,
        tint,
        dim: muted,
        stub: silence,
    })
}

/// An amp's bar in second `t`: its louder meter on its scale, brass below class A, red from
/// [`RED_FROM`].
fn amp_bar(profile: &Profile, past: &Past, t: u64) -> Option<Bar> {
    let read = |id: FunctionId| {
        at(past.series.iter().find(|s| s.id == id.to_string()), t).map(|v| v as i64)
    };
    let (h, class_a) = profile
        .controls
        .iter()
        .filter(|c| c.kind == ControlKind::Meter)
        .filter_map(|c| {
            let placed = profile.placed(c, read)?;
            let raw = read(c.id)?;
            Some((placed.fraction(raw as f64), placed.class_a()))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    let tint = match () {
        _ if h >= RED_FROM => "red",
        _ if h < class_a => "brass",
        _ => "ramp",
    };
    Some(Bar {
        t,
        h,
        tint,
        dim: false,
        stub: false,
    })
}
