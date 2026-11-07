// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's tools: `get_host` reads its volume, EQ and player; `set_host_volume`,
//! `host_mute` and `set_host_eq` change them.
//!
//! The volume follows the rules the device's tools follow. It never goes above 100 %. A rise of
//! more than [`MAX_RISE_DB`] over its lowest level of the last minute asks first, the level in
//! dB being `wpctl`'s cubic scale, `60 * log10(percent / 100)`. A recall of a preset that a
//! connected device plays too asks, since the correction would play twice, and is repeated with
//! the answer.

use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};
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
    let mut out = json!({ "volume": volume_json(view), "eq": eq, "player": player });
    if let Some(why) = view.eq.as_ref().and_then(|eq| eq.unavailable.as_ref()) {
        out["unavailable"] = json!(why);
    } else if !view.available {
        out["unavailable"] = json!("computer controls unavailable: host audio is off");
    }
    out
}
