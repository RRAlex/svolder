// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's tools: `get_host` reads its volume, EQ, crossfeed, virtual speakers and player;
//! `set_host_volume`, `host_mute`, `set_host_eq`, `set_host_crossfeed` and `set_host_speakers`
//! change them.
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
use svolder_host::eq::speakers::Room;
use svolder_ipc::host::HostView;
use svolder_ipc::{HostEqOp, Request, Response};
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
        Ok(host_json(&view))
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
        "Read the computer's volume (percent, muted, the sink), its EQ (on or off, the preset it came from or custom, the preamp in dB and each point's type, frequency, gain and Q), its crossfeed (on or off, model, preset or custom, amount in percent, feed level, crossover, delay and its floor, the output, a note when it turned itself off), its virtual speakers (on or off, the chosen set and its room as {}, its label and licence, the loudness change, a note when they turned themselves off, and the fetched sets a call can choose from; the set fields are null while no set is chosen) and the player (title, artist, album, status); each is null while it is not available, and `unavailable` says why.",
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

fn host_json(view: &HostView) -> Value {
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
        "player": player,
    });
    if let Some(why) = view.eq.as_ref().and_then(|eq| eq.unavailable.as_ref()) {
        out["unavailable"] = json!(why);
    } else if !view.available {
        out["unavailable"] = json!("computer controls unavailable: host audio is off");
    }
    out
}
