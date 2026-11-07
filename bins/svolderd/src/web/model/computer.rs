// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's own controls as the page's model: its volume, its live EQ with the curve the
//! points make, and the presets the computer side lists. Built per page for the shown device,
//! whose tone control gives the built-in presets their bands.

use serde::Serialize;
use svolder_host::eq::EqView;
use svolder_ipc::{DeviceInfo, HostView};
use svolder_profiles::tone::apo::{Correction, SourceFilter};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::tone::response;

use crate::web::Web;

/// How many points the curve has, as the tone control's does.
const POINTS: usize = 181;

/// The computer's volume as the page draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct VolumeModel {
    /// Percent; above 100 when another program boosted the sink, which is shown as read.
    pub(crate) pct: u16,
    pub(crate) muted: bool,
    pub(crate) sink: String,
}

/// One point of the computer's EQ.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PointModel {
    /// The slot the point sits in: its identity for as long as it exists, whatever place the
    /// table lists it in.
    pub(crate) slot: u8,
    /// `PK`, `LSC` or `HSC`.
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) f: f64,
    pub(crate) g: f64,
    pub(crate) q: f64,
}

/// The computer's EQ.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct EqModel {
    pub(crate) on: bool,
    /// Whether it could not be told if the chain runs: `on` is only what the state file says.
    pub(crate) running_unknown: bool,
    pub(crate) points: Vec<PointModel>,
    pub(crate) preamp_db: f64,
    /// The preset the points came from, `custom` once a hand changed them.
    pub(crate) preset: Option<String>,
    /// Whether a drag is heard as it moves.
    pub(crate) live: bool,
    /// Why it is not.
    pub(crate) note: Option<String>,
    /// The most points there can be.
    pub(crate) max: u8,
    /// The points' summed curve in dB at 181 log-spaced frequencies from 20 Hz to 20 kHz, the
    /// preamp left out.
    pub(crate) response: Vec<f64>,
}

/// One preset the computer side lists.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PresetModel {
    pub(crate) name: String,
    pub(crate) built_in: bool,
    /// Whether the computer EQ is on with this preset.
    pub(crate) on: bool,
    /// Whether the computer can play it.
    pub(crate) playable: bool,
    /// Why not, when it cannot.
    pub(crate) why: Option<String>,
}

/// The computer's controls.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ComputerModel {
    /// Absent while the sink has not been read, which the daemon does only while someone watches.
    pub(crate) volume: Option<VolumeModel>,
    pub(crate) eq: EqModel,
    pub(crate) presets: Vec<PresetModel>,
    /// Why the controls cannot be used: a whole `computer controls unavailable: <why>` sentence.
    pub(crate) unavailable: Option<String>,
}

/// The correction `points` make; a point whose type is not one the core knows is left out.
fn correction_of(points: &[PointModel], preamp_db: f64) -> Correction {
    Correction {
        preamp_db,
        filters: points
            .iter()
            .filter_map(|p| {
                Some(SourceFilter {
                    kind: p.kind.parse().ok()?,
                    f: p.f,
                    g: p.g,
                    q: p.q,
                })
            })
            .collect(),
    }
}

/// The curve of `points`, 181 values.
pub(crate) fn response_of(points: &[PointModel]) -> Vec<f64> {
    let freqs = response::log_freqs(POINTS);
    response::correction_db(&correction_of(points, 0.0), &freqs)
}

fn eq_model(view: &EqView) -> EqModel {
    let points: Vec<PointModel> = view
        .points
        .iter()
        .map(|p| PointModel {
            slot: p.slot,
            kind: p.kind.clone(),
            f: p.f,
            g: p.g,
            q: p.q,
        })
        .collect();
    EqModel {
        on: view.on,
        running_unknown: view.running_unknown,
        preamp_db: view.preamp_db,
        preset: view.preset.clone(),
        live: view.live,
        note: view.note.clone(),
        max: view.max,
        response: response_of(&points),
        points,
    }
}

/// The presets the computer lists, `saved` and the built-ins of `shown`'s tone control among
/// them, each with whether the computer plays it.
fn presets_model(
    web: &Web,
    shown: Option<&DeviceInfo>,
    on: Option<&str>,
    saved: &[Preset],
) -> Vec<PresetModel> {
    let eq = shown
        .and_then(|info| web.fleet.profile(info))
        .and_then(|profile| profile.eq.clone());
    let listed: Vec<(Preset, bool)> = match &eq {
        Some(eq) => preset::fitting(eq, saved),
        None => saved.iter().map(|p| (p.clone(), false)).collect(),
    };
    listed
        .into_iter()
        .map(|(p, built_in)| {
            let why = preset::host_correction(&p).err();
            PresetModel {
                built_in,
                on: on.is_some_and(|name| preset::same_name(name, &p.name)),
                playable: why.is_none(),
                why,
                name: p.name,
            }
        })
        .collect()
}

/// What the configuration file's path, modification time and length were when the presets were
/// read, and the presets.
type Kept = (
    std::path::PathBuf,
    Option<(std::time::SystemTime, u64)>,
    Vec<Preset>,
);

/// The saved presets, as the configuration file last read, kept by the file's path, modification
/// time and length: a host event (up to ten a second while an EQ drag runs) reads nothing, and a
/// file that does not parse is read, and logged, once per change.
#[derive(Default)]
pub(crate) struct SavedPresets(std::sync::Mutex<Option<Kept>>);

/// The saved presets of `web`'s configuration. A fleet with no file of its own is read each time.
fn saved_presets(web: &Web) -> Vec<Preset> {
    let read = || {
        web.fleet
            .config()
            .map(|config| config.presets)
            .unwrap_or_default()
    };
    let Some(path) = web.fleet.config_path() else {
        return read();
    };
    let stamp = std::fs::metadata(&path)
        .ok()
        .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
    let mut kept = web
        .saved_presets
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((kept_path, kept_stamp, presets)) = kept.as_ref()
        && *kept_path == path
        && stamp.is_some()
        && *kept_stamp == stamp
    {
        return presets.clone();
    }
    let presets = read();
    *kept = Some((path, stamp, presets.clone()));
    presets
}

/// The computer's controls for a page showing `shown`, from `view`; `None` while the daemon
/// does not serve them (the host is off).
pub(crate) fn computer_model(
    web: &Web,
    view: &HostView,
    shown: Option<&DeviceInfo>,
) -> Option<ComputerModel> {
    if !view.available {
        return None;
    }
    let eq = view.eq.as_ref()?;
    let saved = saved_presets(web);
    let on = eq.on.then_some(eq.preset.as_deref()).flatten();
    Some(ComputerModel {
        volume: view.volume.as_ref().map(|v| VolumeModel {
            pct: v.pct,
            muted: v.muted,
            sink: v.sink.clone(),
        }),
        eq: eq_model(eq),
        presets: presets_model(web, shown, on, &saved),
        unavailable: eq.unavailable.clone(),
    })
}

#[cfg(test)]
mod tests;
