// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's tools: `get_host` reads its volume, EQ, crossfeed, virtual speakers and player;
//! `set_host_volume`, `host_mute`, `set_host_eq`, `set_host_crossfeed`, `set_host_speakers` and
//! `set_host_loudness` and `set_host_bypass` change them.
//!
//! The volume follows the rules the device's tools follow. It never goes above 100 %. A rise of
//! more than [`MAX_RISE_DB`] over its lowest level of the last minute asks first, the level in
//! dB being `wpctl`'s cubic scale, `60 * log10(percent / 100)`. A recall of a preset that a
//! connected device plays too asks, since the correction would play twice, and is repeated with
//! the answer.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, Tool, ToolAnnotations};
use serde_json::{Value, json};
use svolder_host::eq::SpeakersView;
use svolder_host::eq::crossfeed::{CrossfeedView, Model, ModelView};
use svolder_host::eq::loudness::{LoudnessRanges, LoudnessView};
use svolder_host::eq::speakers::Room;
use svolder_ipc::host::{
    DacFacts, DacStage, HostView, PathLine, SETUP_LIMIT, computer_line, path_line, setup_check,
    sink_is_dac,
};
use svolder_ipc::{HostEqOp, Request, Response};
use svolder_profiles::Family;
use svolder_profiles::tone::preset;

use super::{
    Args, Ask, Caller, Done, LetThrough, MAX_RISE_DB, Stop, Tools, answer_error, db_text,
    permission,
};

/// The device the floors of the computer's volume are noted under.
const FLOOR_DEVICE: &str = "computer";

/// The key the computer's volume is noted under in the floors.
const FLOOR_KEY: &str = "host:volume";

/// The top of the computer's volume, in percent.
const MAX_PERCENT: f64 = 100.0;

/// The level in dB `percent` is on `wpctl`'s cubic scale; 0 % reads as 1 %.
fn level_db(percent: f64) -> f64 {
    60.0 * (percent.max(1.0) / 100.0).log10()
}

impl Tools {
    /// The host's view, read fresh.
    async fn host_view(&self) -> Result<HostView, Stop> {
        match self.fleet.request(Request::Host).await {
            Response::Host(view) => Ok(view),
            other => Err(answer_error(other).into()),
        }
    }

    /// Send a request for the computer and turn an error answer into the tool's error.
    async fn host_send(&self, request: Request) -> Result<(), Stop> {
        match self.fleet.request(request).await {
            Response::Ack => Ok(()),
            Response::Error { message, .. } => Err(message.into()),
            other => Err(answer_error(other).into()),
        }
    }

    pub(super) async fn get_host(&self, request: &CallToolRequestParams) -> Done {
        Args::new(request, &[])?;
        let view = self.host_view().await?;
        Ok(host_json(&view, &self.dacs().await))
    }

    /// What each configured DAC reports of its end of the path: every device whose profile is a
    /// DAC and that the fleet follows, in the daemon's order.
    async fn dacs(&self) -> Vec<DacFacts> {
        let Response::Devices { devices } = self.fleet.request(Request::Devices).await else {
            return Vec::new();
        };
        devices
            .iter()
            .filter_map(|device| {
                let profile = self.fleet.profile(device)?;
                if profile.family != Family::Dac {
                    return None;
                }
                let watched = self.fleet.watch(device)?;
                let feed = watched.feed.borrow();
                let mut facts = DacFacts::read(&profile, |id| {
                    let value = &feed.readings.iter().find(|m| m.id == id)?.value;
                    value.as_i64().or_else(|| match value {
                        svolder_proto::Value::Bool(on) => Some(i64::from(*on)),
                        _ => None,
                    })
                });
                // The readings held while the link is down are from before the drop: the
                // device's own stage is not claimed from them.
                if !feed.connected {
                    facts.stage = DacStage::default();
                }
                Some(facts)
            })
            .collect()
    }

    pub(super) async fn set_host_volume(
        &self,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let args = Args::new(request, &["percent", "confirm"])?;
        let percent = args
            .number("percent")?
            .ok_or_else(|| "the argument \"percent\" is required: 0 to 100".to_string())?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        if !(0.0..=MAX_PERCENT).contains(&percent) || percent.fract() != 0.0 {
            return Err(if percent > MAX_PERCENT {
                "the computer's volume goes up to 100 %; choose a whole number from 0 to 100"
            } else {
                "percent must be a whole number from 0 to 100"
            }
            .to_string()
            .into());
        }
        let target = level_db(percent);
        let view = self.host_view().await?;
        if let Some(volume) = &view.volume {
            self.floors
                .record(FLOOR_DEVICE, FLOOR_KEY, level_db(f64::from(volume.pct)));
        }
        // A volume not reported yet counts as the lowest there is, so a rise from it is the
        // largest.
        let floor = match view.volume {
            Some(_) => self.floors.floor(FLOOR_DEVICE, FLOOR_KEY),
            None => Some(level_db(1.0)),
        };
        if let Some(floor) = floor.filter(|floor| target - floor > MAX_RISE_DB + 1e-9) {
            let ask = Ask {
                message: format!(
                    "raising the computer's volume to {percent:.0} % ({}) is more than \
                     {MAX_RISE_DB} dB above its lowest level in the last minute ({})",
                    db_text(target),
                    db_text(floor)
                ),
                title: "Raise the computer's volume".to_string(),
                state: format!("host_volume|{percent:.0}"),
            };
            permission(
                &ask,
                request,
                caller,
                confirmed,
                "the computer's volume is left as it is",
            )?;
            self.floors.reset(FLOOR_DEVICE, FLOOR_KEY, target);
        }
        self.host_send(Request::HostVolume { pct: percent as u8 })
            .await?;
        self.floors.record(FLOOR_DEVICE, FLOOR_KEY, target);
        let view = self.host_view().await?;
        Ok(json!({ "volume": volume_json(&view) }))
    }

    pub(super) async fn host_mute(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["muted", "confirm"])?;
        let muted = args.flag("muted")?.ok_or_else(|| {
            "the argument \"muted\" is required: true to mute, false to unmute".to_string()
        })?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        if !muted {
            // An unmute always asks, as the device's does.
            let at = match self.host_view().await?.volume {
                Some(volume) => format!("a volume of {} %", volume.pct),
                None => "a volume it has not reported yet".to_string(),
            };
            let by = LetThrough::Unmute;
            let ask = Ask {
                message: format!("{} the computer lets sound through at {at}", by.gerund()),
                title: by.title().to_string(),
                state: format!("{}|{FLOOR_DEVICE}", by.tag()),
            };
            permission(&ask, request, caller, confirmed, "the computer stays muted")?;
        }
        self.host_send(Request::HostMute { muted }).await?;
        let view = self.host_view().await?;
        Ok(json!({ "volume": volume_json(&view) }))
    }

    pub(super) async fn set_host_eq(
        &self,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let args = Args::new(request, &["preset", "off", "device", "confirm"])?;
        let preset = args.text("preset")?;
        let off = args.flag("off")?.unwrap_or(false);
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        match (preset, off) {
            (None, false) | (Some(_), true) => {
                return Err(
                    "give exactly one of \"preset\" (a name from list_presets) or \"off\": true"
                        .to_string()
                        .into(),
                );
            }
            (None, true) => {
                self.host_send(Request::HostEq {
                    op: HostEqOp::Mode { on: false },
                })
                .await?;
            }
            (Some(name), false) => {
                let device = self.preset_device(name, args.text("device")?)?;
                let recall = |confirm| Request::HostEq {
                    op: HostEqOp::Preset {
                        name: name.to_string(),
                        device: device.clone(),
                        confirm,
                    },
                };
                match self.fleet.request(recall(false)).await {
                    Response::Ask { message, .. } => {
                        let ask = Ask {
                            message,
                            title: "Play the preset on both".to_string(),
                            state: format!("host_eq|{name}"),
                        };
                        permission(
                            &ask,
                            request,
                            caller,
                            confirmed,
                            "the computer's EQ is left as it is",
                        )?;
                        self.host_send(recall(true)).await?;
                    }
                    Response::Error { message, .. } => return Err(message.into()),
                    Response::Ack => {}
                    other => return Err(answer_error(other).into()),
                }
            }
        }
        let view = self.host_view().await?;
        Ok(match view.eq.as_ref() {
            Some(eq) => json!({ "eq": eq_json(eq) }),
            None => json!({ "eq": null }),
        })
    }

    /// Change the loudness: one [`HostEqOp::Loudness`]. The controller snaps the values to their
    /// steps, holds them to their ranges and words every refusal; `reference: "now"` asks it to
    /// take the computer's volume as the reference.
    pub(super) async fn set_host_loudness(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, LOUDNESS_ARGS)?;
        let on = args.flag("on")?;
        let strength_pct = args.number("strength_pct")?;
        let reference_pct = args.number("reference_pct")?;
        let reference_now = match args.text("reference")? {
            None => None,
            Some(NOW) => Some(true),
            Some(word) => {
                return Err(format!(
                    "reference must be {NOW:?} (got {word:?}); give a number as reference_pct"
                )
                .into());
            }
        };
        if reference_now.is_some() && reference_pct.is_some() {
            return Err("give reference_pct or reference \"now\", not both"
                .to_string()
                .into());
        }
        if on.is_none()
            && strength_pct.is_none()
            && reference_pct.is_none()
            && reference_now.is_none()
        {
            return Err(format!("give at least one of {}", LOUDNESS_ARGS.join(", ")).into());
        }
        self.host_send(Request::HostEq {
            op: HostEqOp::Loudness {
                on,
                strength_pct,
                reference_pct,
                reference_now,
                done: true,
            },
        })
        .await?;
        let view = self.host_view().await?;
        Ok(json!({ "loudness": loudness_of(&view) }))
    }

    /// Turn the bypass on or off: one [`HostEqOp::Bypass`]. It never asks: the preamp falls first
    /// and a rise climbs, so it cannot make the output suddenly louder. The controller words every
    /// refusal.
    pub(super) async fn set_host_bypass(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["on"])?;
        let Some(on) = args.flag("on")? else {
            return Err(
                "give \"on\": true to bypass the computer's stages, false to bring them back"
                    .to_string()
                    .into(),
            );
        };
        self.host_send(Request::HostEq {
            op: HostEqOp::Bypass { on },
        })
        .await?;
        let view = self.host_view().await?;
        Ok(json!({ "bypass": bypass_of(&view), "levels": levels_of(&view) }))
    }

    /// Change the crossfeed. Turning it on never asks: the preamp rule lowers the level first, so
    /// it cannot make the output louder.
    pub(super) async fn set_host_crossfeed(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, CROSSFEED_ARGS)?;
        let model = args.text("model")?;
        if let Some(word) = model.filter(|word| Model::from_word(word).is_none()) {
            let words: Vec<&str> = Model::ALL.iter().map(|m| m.word()).collect();
            return Err(format!(
                "no crossfeed model {word:?}; the models are {}",
                words.join(", ")
            )
            .into());
        }
        let delay_ms = match args.value("delay_ms") {
            Some(Value::String(word)) if word == OWN => Some(0.0),
            Some(Value::String(_)) => {
                return Err(format!("delay_ms must be a number of ms or {OWN:?}").into());
            }
            _ => args.number("delay_ms")?,
        };
        let on = args.flag("on")?;
        let preset = args.text("preset")?.map(str::to_string);
        let amount_pct = args.number("level_pct")?;
        let crossover_hz = args.number("crossover_hz")?;
        if on.is_none()
            && model.is_none()
            && preset.is_none()
            && amount_pct.is_none()
            && crossover_hz.is_none()
            && delay_ms.is_none()
        {
            return Err(format!("give at least one of {}", CROSSFEED_ARGS.join(", ")).into());
        }
        self.host_send(Request::HostEq {
            op: HostEqOp::Crossfeed {
                on,
                model: model.map(str::to_string),
                preset,
                amount_pct,
                crossover_hz,
                delay_ms,
                done: true,
            },
        })
        .await?;
        let view = self.host_view().await?;
        Ok(json!({ "crossfeed": crossfeed_of(&view), "speakers": speakers_of(&view) }))
    }

    /// Change the virtual speakers: choose a fetched set, turn them on or off, or both. A set is
    /// named by its id from `get_host`; nothing is fetched here. Turning them on turns crossfeed
    /// off in the same update, and the daemon refuses what cannot be (no set chosen, a set not
    /// fetched, the EQ unavailable) in words the model reads.
    pub(super) async fn set_host_speakers(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, SPEAKERS_ARGS)?;
        let on = args.flag("on")?;
        let set = args.text("set")?.map(str::to_string);
        if set.as_deref().is_some_and(str::is_empty) {
            return Err("\"set\" is the id of a fetched set from get_host"
                .to_string()
                .into());
        }
        if on.is_none() && set.is_none() {
            return Err(format!("give at least one of {}", SPEAKERS_ARGS.join(", ")).into());
        }
        if let Some(id) = &set {
            let view = self.host_view().await?;
            if let Some(why) = view.eq.as_ref().and_then(|eq| eq.unavailable.as_ref()) {
                return Err(why.clone().into());
            }
            let fetched: Vec<&str> = view
                .eq
                .as_ref()
                .and_then(|eq| eq.speakers.as_ref())
                .map(|sp| sp.sets.iter().map(|set| set.id.as_str()).collect())
                .ok_or_else(|| "virtual speakers are not available on this computer".to_string())?;
            if !fetched.contains(&id.as_str()) {
                return Err(format!(
                    "no fetched set {id:?}; the fetched sets are: {}. Sets are fetched with \
                     `svolder host speakers`, the TUI or the web UI, not through MCP",
                    if fetched.is_empty() {
                        "none".to_string()
                    } else {
                        fetched.join(", ")
                    }
                )
                .into());
            }
        }
        self.host_send(Request::HostEq {
            op: HostEqOp::Speakers {
                on,
                set,
                done: true,
            },
        })
        .await?;
        let view = self.host_view().await?;
        Ok(json!({ "crossfeed": crossfeed_of(&view), "speakers": speakers_of(&view) }))
    }

    /// The device a preset recall is shaped to: the named one, else for a built-in the only or
    /// default one, which must have a tone control; a saved preset needs none.
    fn preset_device(&self, name: &str, named: Option<&str>) -> Result<Option<String>, Stop> {
        if !preset::is_builtin(name) {
            return Ok(named.map(str::to_string));
        }
        let shaped = self.device(named).ok().filter(|(_, p)| p.eq.is_some());
        match shaped {
            Some((info, _)) => Ok(Some(info.name)),
            None => Err(format!(
                "{name} is a built-in preset, shaped to a device's tone control, and {}; \
                 name a device that has one in \"device\", or recall a saved preset",
                match named {
                    Some(device) => format!("{device} has none or is not configured"),
                    None =>
                        "no device with a tone control is the only or the default one".to_string(),
                }
            )
            .into()),
        }
    }
}

fn volume_json(view: &HostView) -> Value {
    view.volume.as_ref().map_or(
        Value::Null,
        |volume| json!({ "percent": volume.pct, "muted": volume.muted, "sink": volume.sink }),
    )
}

fn eq_json(eq: &svolder_host::eq::EqView) -> Value {
    let mut out = json!({
        "on": eq.on,
        "preset": eq.preset,
        "preamp_db": eq.preamp_db,
        "points": eq.points.iter().map(|point| json!({
            "type": point.kind,
            "frequency_hz": point.f,
            "gain_db": point.g,
            "q": point.q,
        })).collect::<Vec<_>>(),
    });
    if eq.running_unknown {
        out["unverified"] = json!(true);
    }
    out
}

/// The bypass of `view`, null when the EQ is unavailable or the daemon has none.
fn bypass_of(view: &HostView) -> Value {
    view.eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .and_then(|eq| eq.bypass.as_ref())
        .map_or(Value::Null, |bypass| {
            json!({
                "on": bypass.on,
                "held": {
                    "eq": bypass.held.eq,
                    "loudness": bypass.held.loudness,
                    "crossfeed": bypass.held.crossfeed,
                    "speakers": bypass.held.speakers,
                },
            })
        })
}

/// The levels of `view` (what the chain plays at, in dB, and its words), null when the EQ is
/// unavailable or the daemon has none.
fn levels_of(view: &HostView) -> Value {
    view.eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .and_then(|eq| eq.levels.as_ref())
        .map_or(Value::Null, |levels| {
            json!({
                "eq_db": levels.eq_db,
                "space_db": levels.space_db,
                "total_db": levels.total_db,
                "climb_to_db": levels.climb_to_db,
                "passthrough": levels.passthrough,
                "untouched": levels.untouched,
                "words": levels.words,
            })
        })
}

/// The definition of `set_host_bypass`.
pub(super) fn bypass_definition() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "on": {
                "type": "boolean",
                "description": "true to turn off every stage that plays (EQ, loudness, crossfeed, speakers) and hold them, false to bring back exactly those"
            }
        },
        "required": ["on"],
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(
        "set_host_bypass",
        "Bypass the computer's stages: on turns off every stage that plays and remembers which, off brings back exactly those as they are now; it never asks, because the level falls first and a rise climbs. Answers with {bypass, levels} as they are after the change; get_host carries both.",
        Arc::new(schema),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(false));
    tool
}

/// The arguments of `set_host_loudness`.
const LOUDNESS_ARGS: &[&str] = &["on", "strength_pct", "reference_pct", "reference"];

/// The word of `reference` for the computer's volume now.
const NOW: &str = "now";

/// The loudness as the tools show it. `curve` (plot data, large) is left out on purpose.
fn loudness_json(ld: &LoudnessView) -> Value {
    let range = |r: &svolder_host::eq::crossfeed::Range| json!({ "min": r.min, "max": r.max, "step": r.step });
    let mut out = json!({
        "on": ld.on,
        "strength_pct": ld.strength_pct,
        "reference_pct": ld.reference_pct,
        "volume_pct": ld.volume_pct,
        "depth_db": ld.depth_db,
        "lift_db": ld.lift_db,
        "mids_db": ld.mids_db,
        "treble_db": ld.treble_db,
        "following": ld.following,
        "note": ld.note,
        "ranges": {
            "strength_pct": range(&ld.ranges.strength_pct),
            "reference_pct": range(&ld.ranges.reference_pct),
        },
    });
    if let Some(why) = &ld.unavailable {
        out["unavailable"] = json!(why);
    }
    out
}

/// The loudness of `view`, null when the EQ is unavailable or the daemon has none.
fn loudness_of(view: &HostView) -> Value {
    view.eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .and_then(|eq| eq.loudness.as_ref())
        .map_or(Value::Null, loudness_json)
}

/// The definition of `set_host_loudness`; the ranges and steps are the view's own.
pub(super) fn loudness_definition() -> Tool {
    let ranges = LoudnessRanges::default();
    let (strength, reference) = (ranges.strength_pct, ranges.reference_pct);
    let properties = json!({
        "on": { "type": "boolean", "description": "true to turn the loudness on, false to turn it off" },
        "strength_pct": {
            "type": "number", "minimum": strength.min, "maximum": strength.max,
            "description": format!("How much of the correction is given back, in percent, held to {} to {} and snapped in steps of {}", strength.min, strength.max, strength.step)
        },
        "reference_pct": {
            "type": "number", "minimum": reference.min, "maximum": reference.max,
            "description": format!("The volume at and above which the stage is flat, in percent of the volume scale, held to {} to {} and snapped in steps of {}", reference.min, reference.max, reference.step)
        },
        "reference": {
            "type": "string", "enum": [NOW],
            "description": "\"now\" takes the computer's volume now as the reference; not with reference_pct"
        }
    });
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": [],
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(
        "set_host_loudness",
        "Change the computer's loudness, which lifts the bass and the treble against the mids as the volume falls below its reference: give at least one of on, strength_pct, reference_pct and reference, and the call answers with {loudness} as it is after the change. It only ever cuts, never raises the level.",
        Arc::new(schema),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(false));
    tool
}

/// The arguments of `set_host_crossfeed`.
const CROSSFEED_ARGS: &[&str] = &[
    "on",
    "model",
    "preset",
    "level_pct",
    "crossover_hz",
    "delay_ms",
];

/// The word of `delay_ms` for the model's own lag. A null cannot say it: every argument of
/// these tools reads a null as not given.
const OWN: &str = "own";

/// The crossfeed as the tools show it. `curve` (plot data, large) and `models` (the schema of
/// `set_host_crossfeed` carries their words and ranges) are left out on purpose.
fn crossfeed_json(xf: &CrossfeedView) -> Value {
    let mut out = json!({
        "on": xf.on,
        "model": xf.model,
        "preset": xf.preset,
        "edited_from": xf.edited_from,
        "level_pct": xf.amount_pct,
        "level_db": xf.amount_db,
        "feed_db": xf.feed_db,
        "crossover_hz": xf.crossover_hz,
        "delay_ms": xf.delay_ms,
        "delay_floor_ms": xf.delay_floor_ms,
        "delay_own": xf.delay_own,
        "delay_settable": xf.delay_settable,
        "output": xf.output,
        "note": xf.note,
        "off_reason": xf.off_reason,
    });
    if let Some(why) = &xf.unavailable {
        out["unavailable"] = json!(why);
    }
    out
}

/// The arguments of `set_host_speakers`.
const SPEAKERS_ARGS: &[&str] = &["on", "set"];

/// The virtual speakers as the tools show them. `curve` (plot data, large) is left out; the
/// fetched sets are listed, since `set_host_speakers` names one by its id.
/// The set fields are null while no set is chosen: a 0 dB change would read as a measured one.
fn speakers_json(sp: &SpeakersView) -> Value {
    let chosen = sp.set.is_some();
    let mut out = json!({
        "on": sp.on,
        "set": sp.set,
        "room": sp.room,
        "room_name": sp.room_name,
        "term": sp.term,
        "label": chosen.then_some(&sp.label),
        "licence": chosen.then_some(&sp.licence),
        "credit": sp.credit,
        "source_url": chosen.then_some(&sp.source_url),
        "angles": chosen.then_some(&sp.angles),
        "taps_ms": chosen.then_some(sp.taps_ms),
        "loudness_db": chosen.then_some(sp.loudness_db),
        "cut": sp.cut,
        "note": sp.note,
        "off_reason": sp.off_reason,
        "sets": sp.sets.iter().map(|set| json!({
            "id": set.id,
            "room": set.room,
            "room_name": set.room_name,
            "term": set.term,
            "label": set.label,
            "licence": set.licence,
            "credit": set.credit,
            "source_url": set.source_url,
        })).collect::<Vec<_>>(),
    });
    if let Some(why) = &sp.unavailable {
        out["unavailable"] = json!(why);
    }
    out
}

/// The virtual speakers of `view`, null when the EQ is unavailable or the daemon has none.
fn speakers_of(view: &HostView) -> Value {
    view.eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .and_then(|eq| eq.speakers.as_ref())
        .map_or(Value::Null, speakers_json)
}

/// The two choices as the descriptions quote them, from the table's own words.
pub(super) fn room_words() -> String {
    [Room::With, Room::No]
        .map(|room| format!("\"{} \u{b7} {}\"", room.name(), room.term()))
        .join(" or ")
}

/// The description of `get_host`.
pub(super) fn get_host_description() -> String {
    format!(
        "Read the computer's volume (percent, muted, the sink), its EQ (on or off, the preset it came from or custom, the preamp in dB and each point's type, frequency, gain and Q), its crossfeed (on or off, model, preset or custom, amount in percent, feed level, crossover, delay and its floor, the output, a note when it turned itself off), its loudness (on or off, strength and reference in percent with their ranges, the volume, the depth, `lift_db` (the bass over the mids), `mids_db` (the mids' cut) and `treble_db` (the top over the mids) in dB, whether it follows the volume now, a note), its virtual speakers (on or off, the chosen set and its room as {}, its label and licence, the set's level change, a note when they turned themselves off, and the fetched sets a call can choose from; the set fields are null while no set is chosen) and the player (title, artist, album, status), `bypass` (`on`, and `held`: which of eq, loudness, crossfeed and speakers it brings back) and `levels` (`eq_db`, `space_db`, `total_db`, `climb_to_db`, `passthrough`, `untouched` and the `words` for what the chain plays at; both null with no chain or from an older daemon), the path of the sound (`path`: the verdict, its words (a list, one per change or thing not read), the sentence behind them when there is one, and `dac`, the name of the DAC it was judged with; the verdict is `untouched`, `changed`, `not_this_dac` (the sound goes to another sink), `other_input` (the DAC listens to an input the computer does not feed) or `unknown`; `end_to_end` has the same keys and adds the DAC's own volume, tone control and the rest to the verdict; when the computer's sink is one of the configured DACs the path is judged to that DAC and `dac` names it, else the computer's side alone is judged and `dac` is null) and the PipeWire config (`config`: the `sink` it judged, its `outcome` (`untouched`, `changes`, `unread`, `not_read` or `other_version`), summary, the `limit` of what it can judge and seven lines, each with a `key`, an `outcome` of `pass`, `fail` or `unknown`, and the `words` of what was found); each is null while it is not available, `path` and `config` while no route is read, and `unavailable` says why.",
        room_words()
    )
}

/// The definition of `set_host_speakers`.
pub(super) fn speakers_definition() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "on": { "type": "boolean", "description": "true to play the virtual speakers, false to stop them; turning them on turns crossfeed off, and needs a chosen set" },
            "set": { "type": "string", "description": format!("The id of a fetched set, from `speakers.sets` in get_host: {} (the first adds a room's own sound, the second is the free-field head only). Nothing is fetched by this call", room_words()) }
        },
        "required": [],
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(
        "set_host_speakers",
        format!(
            "Change the computer's virtual speakers, which play stereo as if from two speakers in front of the listener, through a measured set ({}): give at least one of on and set. Turning them on turns crossfeed off. The call answers with {{crossfeed, speakers}} as they are after the change, so a crossfeed that went off shows its note. Only a set already fetched can be chosen.",
            room_words()
        ),
        Arc::new(schema),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(false));
    tool
}

/// The crossfeed of `view`, null when the EQ is unavailable or the daemon has none.
fn crossfeed_of(view: &HostView) -> Value {
    view.eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .and_then(|eq| eq.crossfeed.as_ref())
        .map_or(Value::Null, crossfeed_json)
}

/// The definition of `set_host_crossfeed`; the words and ranges come from the models' table.
pub(super) fn crossfeed_definition() -> Tool {
    let views: Vec<ModelView> = Model::ALL.into_iter().map(ModelView::of).collect();
    let least = |f: fn(&ModelView) -> f64| views.iter().map(f).fold(f64::INFINITY, f64::min);
    let most = |f: fn(&ModelView) -> f64| views.iter().map(f).fold(0.0, f64::max);
    let per_model = |f: &dyn Fn(&ModelView) -> String| {
        views
            .iter()
            .map(|view| format!("{}: {}", view.word, f(view)))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let presets = per_model(&|view| {
        view.presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let levels = per_model(&|view| {
        format!(
            "{} to {} in steps of {}",
            view.amount_pct.min, view.amount_pct.max, view.amount_pct.step
        )
    });
    let crossovers = per_model(&|view| {
        format!(
            "{} to {} in steps of {}",
            view.crossover_hz.min, view.crossover_hz.max, view.crossover_hz.step
        )
    });
    let delays = per_model(&|view| match view.delay_settable {
        true => format!(
            "up to {} ms in steps of {}",
            view.delay_max_ms, view.delay_step_ms
        ),
        false => "none".to_string(),
    });
    let properties = json!({
        "on": { "type": "boolean", "description": "true to turn the crossfeed on, false to turn it off; turning it on never raises the level and turns the virtual speakers off" },
        "model": { "type": "string", "enum": Model::ALL.map(Model::word), "description": "The model; a different one loads its default preset" },
        "preset": { "type": "string", "description": format!("A preset's name within the model, loaded after a model change; {presets}") },
        "level_pct": {
            "type": "number", "minimum": least(|m| m.amount_pct.min), "maximum": most(|m| m.amount_pct.max),
            "description": format!("The level of the crossed signal as a share of the input, in percent, held to the model's range and snapped to its step ({levels}); makes the values custom")
        },
        "crossover_hz": {
            "type": "number", "minimum": least(|m| m.crossover_hz.min), "maximum": most(|m| m.crossover_hz.max),
            "description": format!("The crossover in Hz, held to the model's range and snapped to its step ({crossovers}); makes the values custom")
        },
        "delay_ms": {
            "anyOf": [
                { "type": "number", "minimum": 0, "maximum": most(|m| m.delay_max_ms) },
                { "const": OWN }
            ],
            "description": format!("The total lag of the other side in ms, snapped to the model's step ({delays}); \"own\" or a number at or below the model's own lag asks for the model's own; makes the values custom; the answer says what was set")
        }
    });
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": [],
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(
        "set_host_crossfeed",
        "Change the computer's crossfeed, which feeds a low-passed, delayed copy of each channel into the other: give at least one of on, model, preset, level_pct, crossover_hz and delay_ms, and the call answers with {crossfeed, speakers} as they are after the change. Turning the crossfeed on turns the virtual speakers off, which the answer's speakers note says.",
        Arc::new(schema),
    );
    tool.annotations = Some(ToolAnnotations::new().read_only(false));
    tool
}

/// The path of `view`'s route judged: the DAC's when the sink is one of `dacs`, else the
/// computer's alone; null with no route.
fn path_of(view: &HostView, dacs: &[DacFacts]) -> Value {
    let Some(route) = &view.route else {
        return Value::Null;
    };
    let dac = route
        .sink
        .as_ref()
        .and_then(|sink| dacs.iter().find(|dac| sink_is_dac(sink, dac)));
    let line: PathLine = match dac {
        Some(dac) => path_line(route, dac),
        None => computer_line(route),
    };
    json!({
        "verdict": line.verdict.key(),
        "end_to_end": line.end_to_end().key(),
        "words": line.words,
        "sentence": line.sentence(),
        "dac": dac.map(|dac| dac.display.clone()),
    })
}

/// The config check of `view`'s route: its sink, summary, `from_player`, `to_dac` and the seven
/// lines; null with no route.
fn config_of(view: &HostView) -> Value {
    let Some(route) = &view.route else {
        return Value::Null;
    };
    let check = setup_check(route);
    let lines: Vec<Value> = check
        .lines
        .iter()
        .map(|line| json!({ "key": line.key, "outcome": line.outcome.key(), "words": line.words }))
        .collect();
    json!({
        "sink": check.sink,
        "outcome": check.summary.key(),
        "summary": check.summary_words(),
        "from_player": check.io.from_player,
        "to_dac": check.io.to_dac,
        "limit": SETUP_LIMIT,
        "lines": lines,
    })
}

fn host_json(view: &HostView, dacs: &[DacFacts]) -> Value {
    let eq = view
        .eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())
        .map_or(Value::Null, eq_json);
    let player = view.playing.as_ref().map_or(
        Value::Null,
        |p| json!({ "title": p.title, "artist": p.artist, "album": p.album, "status": p.status }),
    );
    let mut out = json!({
        "volume": volume_json(view),
        "eq": eq,
        "crossfeed": crossfeed_of(view),
        "speakers": speakers_of(view),
        "loudness": loudness_of(view),
        "bypass": bypass_of(view),
        "levels": levels_of(view),
        "player": player,
        "path": path_of(view, dacs),
        "config": config_of(view),
    });
    if let Some(why) = view.eq.as_ref().and_then(|eq| eq.unavailable.as_ref()) {
        out["unavailable"] = json!(why);
    } else if !view.available {
        out["unavailable"] = json!("computer controls unavailable: host audio is off");
    }
    out
}
