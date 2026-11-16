// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tools the MCP server offers: five that read (four a device, `get_host` the computer) and,
//! with `[mcp] writes`, nine that act: three that change a device, `identify`, which asks it to
//! show itself, and five for the computer (see [`host`]).
//!
//! Every tool goes through the daemon's own request path ([`Fleet`]), the one the socket's
//! clients use, so a value is validated and refused exactly as `svolder set` does. Results
//! carry device data only: names the user configured, profile labels and readings, never
//! anything a third party writes.
//!
//! A model must not be able to raise the volume dangerously. A `set_control` on the volume
//! that raises it by more than [`MAX_RISE_DB`] in one call asks first: a 2026-07-28 client that
//! can elicit is sent an `input_required` naming the device and both levels, and the call goes
//! through when its retry carries the answer yes; any other client gets a tool error saying to
//! repeat the call with `"confirm": true`. A level above `[mcp] max_volume_db` is refused
//! whatever is answered. Every volume write, once the cap and the question allow it, goes
//! through the device's writer, which judges on its own reading whether it lands at once or,
//! above three quarters of the volume's range, climbs there a tenth of the range a second; the
//! tool waits for a climb, up to five seconds, and answers with the level the device reached.
//! The write tools are not listed while `writes` is off, and a call to
//! one is refused naming the setting, since a client may hold an older list.
//!
//! A Loki Max has presets of its own, which `list_presets` and `load_preset` offer in place of a
//! tone control's: three slots the device saved and five built-in settings, never a save. A
//! built-in setting is six band writes, so it is judged as they are: each band that rises more
//! than [`MAX_RISE_DB`] above its floor asks, and one question names every such band. A slot's
//! recall always asks, since the bands it sets are not known until the device reports them and
//! any of them can reach the top of its range.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ErrorData, InputRequiredResult,
    ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
    Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::{Value, json};
use svolder_ipc::{DeviceInfo, Request, Response};
use svolder_profiles::roles;
use svolder_profiles::safety::{self, Rules, Situation, Through};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::units;

use crate::device::Command;
use crate::fleet::Fleet;
use crate::writer::{ClimbView, Glided, Mode, Options, Outcome};
use svolder_ipc::CODE_TIMEOUT;
use svolder_profiles::device_presets;
use svolder_profiles::{Control, ControlKind, Profile, convert, intent};
use svolder_proto::{FunctionClass, FunctionId};

pub use svolder_profiles::safety::MAX_RISE_DB;

mod describe;
mod guard;
mod host;
mod schema;

use describe::{db_text, describe_control, keys_of, label_words, reply};
pub use guard::Floors;
use guard::{Ask, Guard, LetThrough, gain_guard, level_raw, names_a_save, permission, plain_input};
use schema::{Args, TOOLS, definition};

/// The longest span `get_link_history` answers for, in hours: what the daemon keeps.
const MAX_HISTORY_HOURS: f64 = 24.0;

/// The longest a level change waits for the climb it started before the tool answers with the
/// level the device has reached.
const CLIMB_WAIT: Duration = Duration::from_secs(5);

/// How long past [`CLIMB_WAIT`] the reading that answers a climb is given to arrive.
const CLIMB_GRACE: Duration = Duration::from_millis(50);

/// The key `input_requests` carries the volume question under, and the answer comes back under.
const ASK: &str = "confirm_change";

/// What a tool call is told about the client it answers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caller {
    /// Whether the client can be sent an `input_required` asking the user a question: it
    /// negotiated 2026-07-28 or newer and declared that it elicits.
    pub can_ask: bool,
}

/// The tools of one server, as `[mcp]` configured it.
#[derive(Clone)]
pub struct Tools {
    fleet: Arc<dyn Fleet>,
    writes: bool,
    max_volume_db: Option<f64>,
    floors: Arc<Floors>,
}

/// How a tool call ends other than with an answer.
enum Stop {
    /// A tool error, in words for the model.
    Say(String),
    /// The client must answer a question first.
    Ask(InputRequiredResult),
}

impl From<String> for Stop {
    fn from(message: String) -> Self {
        Stop::Say(message)
    }
}

type Done = Result<Value, Stop>;

impl Tools {
    /// The tools over `fleet`: the write tools only when `writes`, volume levels capped at
    /// `max_volume_db` when it is given.
    #[cfg(test)]
    pub fn new(fleet: Arc<dyn Fleet>, writes: bool, max_volume_db: Option<f64>) -> Self {
        Tools::with_floors(fleet, writes, max_volume_db, Arc::default())
    }

    /// The tools over `fleet`: the write tools only when `writes`, volume levels capped at
    /// `max_volume_db` when it is given, rises measured from `floors`, which the daemon keeps
    /// across the servers it starts, so a restart of the listener keeps the last minute.
    pub fn with_floors(
        fleet: Arc<dyn Fleet>,
        writes: bool,
        max_volume_db: Option<f64>,
        floors: Arc<Floors>,
    ) -> Self {
        Tools {
            fleet,
            writes,
            max_volume_db,
            floors,
        }
    }

    /// Follow the devices' own readings for the floors, until `stop`.
    pub fn watch(&self, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(Arc::clone(&self.floors).watch(Arc::clone(&self.fleet), stop))
    }

    /// The tools a client is told about.
    pub fn list(&self) -> Vec<Tool> {
        TOOLS
            .iter()
            .filter(|(_, write)| self.writes || !write)
            .map(|(name, write)| definition(name, *write))
            .collect()
    }

    /// Run the tool `request` names. A refusal or failure of the device is a tool error the
    /// model reads; only a tool that does not exist is a protocol error.
    pub async fn call(
        &self,
        request: CallToolRequestParams,
        caller: Caller,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.as_ref();
        let Some((_, write)) = TOOLS.iter().find(|(known, _)| *known == name) else {
            return Err(ErrorData::invalid_params(
                format!(
                    "no tool named {name:?}; the tools are: {}",
                    self.list()
                        .iter()
                        .map(|t| t.name.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None,
            ));
        };
        if *write && !self.writes {
            return Ok(failure(&format!(
                "{name} is not available: this server is read-only; the owner can allow it with \
                 `[mcp] writes = true` in the svolder configuration"
            )));
        }
        let outcome = match name {
            "list_devices" => self.list_devices(&request).await,
            "get_state" => self.get_state(&request).await,
            "get_link_history" => self.get_link_history(&request).await,
            "list_presets" => self.list_presets(&request).await,
            "set_control" => self.set_control(&request, caller).await,
            "load_preset" => self.load_preset(&request, caller).await,
            "identify" => self.identify(&request).await,
            "get_host" => self.get_host(&request).await,
            "set_host_volume" => self.set_host_volume(&request, caller).await,
            "host_mute" => self.host_mute(&request, caller).await,
            "set_host_eq" => self.set_host_eq(&request, caller).await,
            "set_host_crossfeed" => self.set_host_crossfeed(&request).await,
            "set_host_speakers" => self.set_host_speakers(&request).await,
            _ => self.mute(&request, caller).await,
        };
        Ok(match outcome {
            Ok(value) => CallToolResult::structured(value).into(),
            Err(Stop::Say(message)) => failure(&message),
            Err(Stop::Ask(ask)) => ask.into(),
        })
    }

    async fn list_devices(&self, request: &CallToolRequestParams) -> Done {
        Args::new(request, &[])?;
        let Response::Devices { devices } = self.fleet.request(Request::Devices).await else {
            return Err("the daemon did not list its devices".to_string().into());
        };
        let devices: Vec<Value> = devices
            .iter()
            .map(|d| {
                let mut row = json!({
                    "name": d.name,
                    "product": d.product,
                    "connected": d.connected,
                });
                if let Some(signal) = d.signal {
                    row["signal"] = json!(signal);
                }
                row
            })
            .collect();
        Ok(json!({ "devices": devices }))
    }

    async fn get_state(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["device"])?;
        let (info, profile) = self.device(args.text("device")?)?;
        let values = match self
            .fleet
            .request(Request::State {
                device: Some(info.name.clone()),
            })
            .await
        {
            Response::Values { values, .. } => values,
            other => return Err(answer_error(other).into()),
        };
        let controls: Vec<Value> = profile
            .controls
            .iter()
            .filter(|control| control.id.class() != FunctionClass::Sensitive)
            .map(|control| describe_control(&profile, control, &values))
            .collect();
        Ok(json!({
            "device": info.name,
            "product": info.product,
            "connected": info.connected,
            "controls": controls,
        }))
    }

    async fn get_link_history(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["device", "hours"])?;
        let hours = args.number("hours")?.unwrap_or(MAX_HISTORY_HOURS);
        if !hours.is_finite() || hours <= 0.0 || hours > MAX_HISTORY_HOURS {
            return Err(format!(
                "hours must be above 0 and at most {MAX_HISTORY_HOURS}: the daemon keeps one day \
                 of link history"
            )
            .into());
        }
        let info = self.info(args.text("device")?)?;
        let span_secs = (hours * 3600.0).round() as u64;
        match self
            .fleet
            .request(Request::History {
                device: Some(info.name.clone()),
                span_secs,
            })
            .await
        {
            Response::History {
                device,
                now_ms,
                transitions,
            } => {
                let transitions: Vec<Value> = transitions
                    .iter()
                    .map(|t| {
                        json!({
                            "at_ms": t.at_ms,
                            "seconds_ago": now_ms.saturating_sub(t.at_ms) / 1000,
                            "connected": t.connected,
                        })
                    })
                    .collect();
                Ok(json!({
                    "device": device,
                    "now_ms": now_ms,
                    "hours": hours,
                    "transitions": transitions,
                }))
            }
            other => Err(answer_error(other).into()),
        }
    }

    async fn list_presets(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["device"])?;
        let (info, profile) = self.device(args.text("device")?)?;
        let Some(eq) = profile.eq.as_ref() else {
            if device_presets::offered(&profile) {
                return self.list_device_presets(&info, &profile).await;
            }
            return Err(format!("{} has no tone control, so no presets", info.name).into());
        };
        // An imported preset has no band count of its own: it is fitted to this device's.
        let bands = eq.bands.len();
        let fitting = self.fleet.config()?.fitting_presets(&profile);
        let tone = match self
            .fleet
            .request(Request::EqShow {
                device: Some(info.name.clone()),
            })
            .await
        {
            Response::Values { values, .. } => values,
            _ => Vec::new(),
        };
        let raw = |id: FunctionId| {
            tone.iter()
                .find(|vo| vo.id.parse::<FunctionId>().ok() == Some(id))
                .and_then(|vo| vo.raw.as_i64())
        };
        let listed: Vec<Preset> = fitting.iter().map(|(p, _)| p.clone()).collect();
        crate::recall::warm(eq, &listed).await;
        let on = match preset::on(eq, raw, &listed, info.recalled.as_deref()) {
            preset::On::Preset(found) => Some(found.name.clone()),
            preset::On::Unread | preset::On::Custom => None,
        };
        let presets: Vec<Value> = fitting
            .into_iter()
            .map(|(p, built_in)| {
                let bands = if p.is_imported() {
                    bands
                } else {
                    usize::from(p.bands)
                };
                let is_on = on
                    .as_deref()
                    .is_some_and(|name| preset::same_name(name, &p.name));
                json!({ "name": p.name, "bands": bands, "built_in": built_in, "on": is_on })
            })
            .collect();
        Ok(json!({ "device": info.name, "presets": presets }))
    }

    async fn load_preset(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["device", "name", "confirm"])?;
        let name = args.required_text("name")?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        let (info, profile) = self.device(args.text("device")?)?;
        if profile.eq.is_none() {
            if device_presets::offered(&profile) {
                return self
                    .load_device_preset(&info, &profile, name, confirmed, request, caller)
                    .await;
            }
            return Err(format!("{} has no tone control, so no presets", info.name).into());
        }
        let fitting: Vec<Preset> = self
            .fleet
            .config()?
            .fitting_presets(&profile)
            .into_iter()
            .map(|(preset, _)| preset)
            .collect();
        let (recalled, response) = self.fleet.recall(&info, name, fitting).await?;
        if let Response::Error { message, .. } = response {
            return Err(message.into());
        }
        Ok(json!({
            "device": info.name,
            "preset": recalled.preset.name,
            "writes": recalled.recall.commands.len(),
            "clamped": recalled
                .recall
                .plan
                .clamped
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        }))
    }

    /// A device's own presets as `list_presets` shows them: its slots, which the device never
    /// says are on, then its built-in settings, the one its bands read marked on.
    async fn list_device_presets(&self, info: &DeviceInfo, profile: &Profile) -> Done {
        let raws = self.band_raws(info, profile).await?;
        let on = device_presets::matching(profile, |id| raws.get(&id).copied())
            .map(|setting| setting.name.clone());
        let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
        let mut presets: Vec<Value> = (1..=slots)
            .map(|slot| {
                json!({
                    "name": device_presets::slot_name(slot),
                    "slot": slot,
                    "built_in": false,
                    "on": false,
                })
            })
            .collect();
        for setting in profile
            .builtin_settings
            .iter()
            .flat_map(|table| &table.settings)
        {
            presets.push(json!({
                "name": setting.name,
                "built_in": true,
                "on": on.as_ref() == Some(&setting.name),
            }));
        }
        Ok(json!({ "device": info.name, "presets": presets }))
    }

    /// Put a device on the slot or the built-in setting `name` names: a slot recall always asks,
    /// a built-in setting asks when one of its band writes would, and a save is never offered.
    async fn load_device_preset(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
        name: &str,
        confirmed: bool,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let name = name.trim();
        if names_a_save(name) {
            return Err(format!(
                "saving a preset is not offered here, because it overwrites what the device holds; \
                 {} can be saved to from its own front ends",
                info.name
            )
            .into());
        }
        let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
        if let Some(slot) = device_presets::parse_slot(name, slots) {
            return self
                .recall_slot(info, profile, slot, confirmed, request, caller)
                .await;
        }
        if device_presets::find(profile, name).is_some() {
            return self
                .apply_setting(info, profile, name, confirmed, request, caller)
                .await;
        }
        let listed: Vec<String> = (1..=slots)
            .map(device_presets::slot_name)
            .chain(
                profile
                    .builtin_settings
                    .iter()
                    .flat_map(|table| table.settings.iter().map(|s| s.name.clone())),
            )
            .collect();
        Err(format!(
            "{} has no preset {name:?}; its presets are: {}",
            info.name,
            listed.join(", ")
        )
        .into())
    }

    /// Recall slot `slot` once the user has agreed to it: the bands it sets are the device's to
    /// say, so the question names the highest level any of them can reach.
    async fn recall_slot(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
        slot: u8,
        confirmed: bool,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let Some(action) = roles::recall_action(profile) else {
            return Err(format!("{} has no slots to recall", info.name).into());
        };
        let slot_name = device_presets::slot_name(slot);
        let message = match device_presets::recall_ceiling_db(profile) {
            Some(ceiling) => format!(
                "recalling {slot_name} on {} sets its bands to what the device saved there, which \
                 can raise a band to as much as {}",
                info.name,
                db_text(ceiling)
            ),
            None => format!(
                "recalling {slot_name} on {} sets what the device saved there",
                info.name
            ),
        };
        let ask = Ask {
            message,
            title: format!("Recall {slot_name}"),
            state: format!("recall|{}|{slot}", info.name),
        };
        permission(
            &ask,
            request,
            caller,
            confirmed,
            &format!("{} stays as it is", info.name),
        )?;
        let command = Command::Set {
            key: action.key.clone(),
            value: slot.to_string(),
            step: 1,
            no_wait: false,
        };
        match self
            .fleet
            .submit(info, command, Options::from(Mode::Ordered))
            .await
        {
            Outcome::Done(Response::Error { message, .. }) => Err(message.into()),
            Outcome::Done(_) => Ok(json!({
                "device": info.name,
                "preset": slot_name,
                "slot": slot,
            })),
            Outcome::Superseded => {
                Err(format!("a newer write to {} took this one's place", info.name).into())
            }
            Outcome::Cancelled(why) => Err(why.to_string().into()),
        }
    }

    /// Apply the built-in setting `name` once the rules allow it. Its writes are planned from the
    /// readings and each is judged as a band write by [`Tools::level_guard`]; one question names
    /// every band that asks. Once it is applied, each band that asked starts its window over
    /// at the level agreed to and every other band notes its new level.
    async fn apply_setting(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
        name: &str,
        confirmed: bool,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let raws = self.band_raws(info, profile).await?;
        let plan = device_presets::plan(profile, name, |id| raws.get(&id).copied())
            .map_err(|e| e.to_string())?;
        if plan.writes.is_empty() {
            return Ok(json!({
                "device": info.name,
                "preset": plan.name,
                "writes": 0,
                "message": format!("{} is already on", plan.name),
            }));
        }
        // The writer plans again from its own readings, so a band another front end lowers after
        // this check is written too: every band the setting sets is judged, the ones already on
        // target against their floor.
        let setting = device_presets::find(profile, name);
        let table = profile.builtin_settings.as_ref();
        let mut guards = Vec::new();
        for (key, &target) in table
            .iter()
            .flat_map(|table| &table.keys)
            .zip(setting.iter().flat_map(|setting| &setting.raw))
        {
            let Some(control) = profile.control_by_key(key) else {
                continue;
            };
            if safety::in_db(control)
                && let Some(guard) = self
                    .level_guard(info, control, target, &Value::Null, true)
                    .await?
            {
                guards.push((control, guard));
            }
        }
        let questions: Vec<&str> = guards
            .iter()
            .filter_map(|(_, guard)| guard.ask.as_ref())
            .map(|ask| ask.message.as_str())
            .collect();
        if !questions.is_empty() {
            // The state names each band that asks with its target and the floor it was judged
            // against, so a yes given under other floors does not answer this question.
            let judged: Vec<String> = guards
                .iter()
                .filter(|(_, guard)| guard.ask.is_some())
                .map(|(control, guard)| {
                    let level =
                        |db: Option<f64>| db.map_or("none".to_string(), |db| format!("{db:.1}"));
                    format!(
                        "{}={}@{}",
                        control.key,
                        level(guard.level),
                        level(guard.floor_db)
                    )
                })
                .collect();
            let ask = Ask {
                message: questions.join("; "),
                title: format!("Apply {}", plan.name),
                state: format!("setting|{}|{}|{}", info.name, plan.name, judged.join(",")),
            };
            permission(
                &ask,
                request,
                caller,
                confirmed,
                &format!("the bands of {} stay as they are", info.name),
            )?;
        }
        let (applied, response) = self.fleet.apply(info, &plan.name).await?;
        if let Response::Error { message, .. } = response {
            return Err(message.into());
        }
        for (control, guard) in guards {
            if let Some(target) = guard.level {
                match guard.ask {
                    Some(_) => self.floors.reset(&info.name, &control.key, target),
                    None => self.floors.record(&info.name, &control.key, target),
                }
            }
        }
        Ok(json!({
            "device": info.name,
            "preset": applied.name,
            "writes": applied.writes.len(),
        }))
    }

    /// The raw reading of each control a built-in setting sets, by the control's id; a control
    /// the device has not reported is left out.
    async fn band_raws(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
    ) -> Result<HashMap<FunctionId, i64>, Stop> {
        let mut raws = HashMap::new();
        for key in profile
            .builtin_settings
            .iter()
            .flat_map(|table| &table.keys)
        {
            let Some(control) = profile.control_by_key(key) else {
                continue;
            };
            if let Some(raw) = self.read_raw(info, key).await? {
                raws.insert(control.id, raw);
            }
        }
        Ok(raws)
    }

    /// Ask the device to show itself, through the session's one send per gap. Answers
    /// `{"device", "sent"}`; `sent` is false inside the gap, when nothing went out. A device
    /// whose profile offers no identify is a tool error saying so, and the daemon is not asked. A
    /// device that is down is a tool error in the daemon's words, with the wait to its next attempt
    /// where the row has one.
    async fn identify(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["device"])?;
        let info = self.info(args.text("device")?)?;
        if self.fleet.profile(&info).is_some_and(|p| !p.identify) {
            return Err(format!("{} offers no identify", info.name).into());
        }
        let asked = Request::Identify {
            device: Some(info.name.clone()),
        };
        match self.fleet.request(asked).await {
            Response::Identify { device, sent } => Ok(json!({ "device": device, "sent": sent })),
            Response::Error {
                message,
                reason: Some(svolder_ipc::Reason::NotConnected),
                ..
            } => {
                let wait = info
                    .retry_in_secs
                    .map(|secs| format!(", next attempt in {secs} s"))
                    .unwrap_or_default();
                Err(format!("{}: {message}{wait}", info.name).into())
            }
            other => Err(answer_error(other).into()),
        }
    }

    async fn mute(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["device", "on", "confirm"])?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        let on = args.flag("on")?.ok_or_else(|| {
            "the argument \"on\" is required: true to mute, false to unmute".to_string()
        })?;
        let (info, profile) = self.device(args.text("device")?)?;
        let control = profile
            .controls
            .iter()
            .find(|c| c.mutes_output && c.key.eq_ignore_ascii_case("mute"))
            .or_else(|| profile.controls.iter().find(|c| c.mutes_output))
            .ok_or_else(|| {
                format!(
                    "{} has no mute control; its toggles are: {}",
                    info.name,
                    keys_of(&profile, |c| c.kind == ControlKind::Toggle)
                )
            })?;
        self.apply(
            &info,
            &profile,
            control,
            &json!(on),
            confirmed,
            request,
            caller,
        )
        .await
    }

    async fn set_control(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["device", "key", "value", "confirm"])?;
        let key = args.required_text("key")?;
        let value = args
            .value("value")
            .ok_or_else(|| "the argument \"value\" is required".to_string())?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        let (info, profile) = self.device(args.text("device")?)?;
        let Some(control) = profile.control_by_key(key) else {
            return Err(format!(
                "{} has no control {key:?}; the controls you can set are: {}",
                info.name,
                keys_of(&profile, |c| c.kind.writable())
            )
            .into());
        };
        if !control.kind.writable() {
            return Err(format!(
                "{} is read-only on {}; the controls you can set are: {}",
                control.key,
                info.name,
                keys_of(&profile, |c| c.kind.writable())
            )
            .into());
        }
        self.apply(&info, &profile, control, value, confirmed, request, caller)
            .await
    }

    /// Set `control` to `value` once the rules allow it: a level in dB that rises by more than
    /// [`MAX_RISE_DB`] above the lowest level of the last minute, a gain select moved up, a
    /// mute released and a device powered on all ask first; a volume that rises above
    /// `max_volume_db`, or an unmute or a power-on while the volume is above it, is refused.
    #[allow(clippy::too_many_arguments)]
    async fn apply(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
        control: &Control,
        value: &Value,
        confirmed: bool,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let (send, guard) = match control.kind {
            ControlKind::Level => {
                let raw = level_raw(control, value)?;
                let guard = if safety::in_db(control) {
                    self.level_guard(info, control, raw, value, false).await?
                } else {
                    None
                };
                (raw.to_string(), guard)
            }
            _ => {
                let text = plain_input(value)?;
                let message = intent(profile, &control.key, &text).map_err(|e| e.to_string())?;
                let new = match control.kind {
                    ControlKind::Toggle => Some(i64::from(message.value.bool_value())),
                    _ => message.value.as_i64(),
                };
                let current = match control.output_gain {
                    true => self.read_raw(info, &control.key).await?,
                    false => None,
                };
                let guard = match safety::lets_sound_through(control, new, current) {
                    Some(Through::GainUp) => Some(gain_guard(info, control, current, new)),
                    Some(Through::Unmute) => {
                        self.unmute_guard(info, profile, LetThrough::Unmute).await?
                    }
                    Some(Through::PowerOn) => {
                        self.unmute_guard(info, profile, LetThrough::PowerOn)
                            .await?
                    }
                    Some(Through::AnyChange) => Some(Guard {
                        level: None,
                        floor_db: None,
                        glide: None,
                        ask: Some(Ask {
                            message: format!(
                                "changing {} on {} may make the output louder",
                                control.label.to_ascii_lowercase(),
                                info.name
                            ),
                            title: format!("Change the {}", control.label.to_ascii_lowercase()),
                            state: format!("change|{}|{}|{text}", info.name, control.key),
                        }),
                    }),
                    None => None,
                };
                (text, guard)
            }
        };
        if let Some(guard) = &guard
            && let Some(ask) = &guard.ask
        {
            permission(
                ask,
                request,
                caller,
                confirmed,
                &format!("{} stays as it is", control.label.to_ascii_lowercase()),
            )?;
        }
        let done = match guard.as_ref().and_then(|guard| guard.glide) {
            Some(target) => self.glide_set(info, control, target).await?,
            None => self.send_set(info, control, send).await?,
        };
        if let Some(Guard {
            level: Some(target),
            ask,
            ..
        }) = guard
        {
            match ask {
                Some(_) => self.floors.reset(&info.name, &control.key, target),
                None => self.floors.record(&info.name, &control.key, target),
            }
        }
        Ok(done)
    }

    /// The checks of a level in dB set to raw step `raw`, by the shared rule ([`safety::judge`]):
    /// a rise past `max_volume_db` on the volume is refused, a rise of more than [`MAX_RISE_DB`]
    /// above the lowest level of the last minute asks.
    async fn level_guard(
        &self,
        info: &DeviceInfo,
        control: &Control,
        raw: i64,
        asked: &Value,
        from_floor: bool,
    ) -> Result<Option<Guard>, Stop> {
        let target = convert::raw_to_display(control, raw)
            .ok_or_else(|| format!("{} step {raw} has no level in dB", control.key))?;
        let reading_raw = self.read_raw(info, &control.key).await?;
        // A level not reported yet counts as the lowest there is, so a rise from it is the
        // largest; the rule keeps negative infinity in its arithmetic.
        let reading = reading_raw.and_then(|raw| convert::raw_to_display(control, raw));
        if let Some(level) = reading {
            self.floors.record(&info.name, &control.key, level);
        }
        let requested = asked.as_f64().or_else(|| {
            asked
                .as_str()
                .and_then(|s| units::number_in(s, control.unit.as_deref()))
        });
        let situation = Situation {
            reading: reading_raw,
            base: reading_raw,
            floor_db: self.floors.floor(&info.name, &control.key),
            asked_db: requested,
        };
        let judged = safety::judge(control, &situation, raw, &Rules::mcp(self.max_volume_db));
        if let Some(capped) = judged.capped.filter(|capped| capped.refused) {
            return Err(Stop::Say(format!(
                "raising the volume to {} is above this server's limit of {} (max_volume_db); \
                 choose a level at or below it{}",
                db_text(requested.unwrap_or(target)),
                db_text(capped.cap_db),
                if requested.is_some_and(|r| r <= capped.cap_db) {
                    format!(" (the nearest step to your request is {})", db_text(target))
                } else {
                    String::new()
                }
            )));
        }
        // A write that goes out whatever the reading now (the writer plans from its own) is also
        // judged against the floor alone: a level that is later lowered must not be written back
        // up past the rise a question guards.
        let by_floor = judged.question.is_none() && from_floor;
        let question = judged.question.or_else(|| {
            let floor = situation.floor_db?;
            (by_floor && target - floor > MAX_RISE_DB + 1e-9).then_some(safety::Rise {
                from_db: reading,
                to_db: target,
                floor_db: floor,
            })
        });
        let by_floor = by_floor && question.is_some();
        let Some(rise) = question else {
            return Ok(Some(Guard {
                level: Some(target),
                floor_db: situation.floor_db,
                ask: None,
                glide: roles::is_volume(control).then_some(raw),
            }));
        };
        let message = match rise.from_db {
            _ if by_floor => format!(
                "writing {} on {} at {} is more than {MAX_RISE_DB} dB above its lowest level in the last minute ({})",
                label_words(control),
                info.name,
                db_text(rise.to_db),
                db_text(rise.floor_db)
            ),
            Some(level) => format!(
                "raising {} on {} from {} to {} is more than {MAX_RISE_DB} dB above its lowest level in the last minute ({})",
                label_words(control),
                info.name,
                db_text(level),
                db_text(rise.to_db),
                db_text(rise.floor_db)
            ),
            None => format!(
                "raising {} on {} from {} the device has not reported yet to {}",
                label_words(control),
                info.name,
                match roles::is_volume(control) {
                    true => "a volume",
                    false => "a level",
                },
                db_text(rise.to_db)
            ),
        };
        let state = format!(
            "level|{}|{}|{raw}|{}",
            info.name,
            control.key,
            reading_raw.map_or("none".to_string(), |r| r.to_string())
        );
        Ok(Some(Guard {
            level: Some(target),
            floor_db: situation.floor_db,
            ask: Some(Ask {
                message,
                title: format!("Raise the {}", control.label.to_ascii_lowercase()),
                state,
            }),
            glide: roles::is_volume(control).then_some(raw),
        }))
    }

    /// The check of letting sound through, by an unmute or by powering on: refused while the
    /// volume is above `max_volume_db` or, with a limit set, not yet reported, and asked about
    /// otherwise. A device with no volume in dB has nothing to compare, so it asks about its
    /// full output.
    async fn unmute_guard(
        &self,
        info: &DeviceInfo,
        profile: &Profile,
        by: LetThrough,
    ) -> Result<Option<Guard>, Stop> {
        let Some(volume) = profile
            .controls
            .iter()
            .find(|c| roles::is_volume(c) && safety::in_db(c))
        else {
            return Ok(Some(Guard {
                level: None,
                floor_db: None,
                glide: None,
                ask: Some(Ask {
                    message: format!(
                        "{} {} lets sound through at its full output",
                        by.gerund(),
                        info.name
                    ),
                    title: by.title().to_string(),
                    state: format!("{}|{}", by.tag(), info.name),
                }),
            }));
        };
        let reading = self.read_level(info, volume).await?;
        // A volume not reported yet is no ground for a refusal: it is asked about.
        if let Some(cap) = self.max_volume_db
            && let Some(level) = reading
            && level > cap
        {
            return Err(Stop::Say(format!(
                "{} {} would play at {}, above this server's limit of {} (max_volume_db); \
                 lower the volume first",
                by.gerund(),
                info.name,
                db_text(level),
                db_text(cap)
            )));
        }
        let at = match (reading, by) {
            (None, _) => "a volume the device has not reported yet".to_string(),
            (Some(level), LetThrough::Unmute) => format!("a volume of {}", db_text(level)),
            (Some(level), LetThrough::PowerOn) => {
                format!("its current volume of {}", db_text(level))
            }
        };
        Ok(Some(Guard {
            level: None,
            floor_db: None,
            glide: None,
            ask: Some(Ask {
                message: format!("{} {} lets sound through at {at}", by.gerund(), info.name),
                title: by.title().to_string(),
                state: format!("{}|{}|{}", by.tag(), info.name, volume.key),
            }),
        }))
    }

    /// A level's current value in dB, `None` when the device has reported none. Silence is a
    /// reading of its own: the lowest step reads negative infinity.
    async fn read_level(&self, info: &DeviceInfo, control: &Control) -> Result<Option<f64>, Stop> {
        Ok(self
            .read_raw(info, &control.key)
            .await?
            .and_then(|raw| convert::raw_to_display(control, raw)))
    }

    /// A control's current raw reading, or `None` when the device has reported none.
    async fn read_raw(&self, info: &DeviceInfo, key: &str) -> Result<Option<i64>, Stop> {
        match self
            .fleet
            .request(Request::Get {
                device: Some(info.name.clone()),
                key: key.to_string(),
            })
            .await
        {
            Response::Value(vo) => Ok(vo.raw.as_i64()),
            Response::Error {
                code: CODE_TIMEOUT, ..
            } => Ok(None),
            other => Err(answer_error(other).into()),
        }
    }

    /// Send one `set` and report the reading the device confirmed, a level counted the way
    /// `get_state` counts it.
    async fn send_set(&self, info: &DeviceInfo, control: &Control, value: String) -> Done {
        match self
            .fleet
            .request(Request::Set {
                device: Some(info.name.clone()),
                key: control.key.clone(),
                value,
                step: 1,
                no_wait: false,
            })
            .await
        {
            Response::Value(vo) => Ok(reply(info, control, &vo)),
            other => Err(answer_error(other).into()),
        }
    }

    /// Set `control` to raw step `target` through the device's writer, which judges on its own
    /// reading whether it lands at once or climbs, and report where it got to: a landing with its
    /// echo; a climb by waiting on the writer's climb watch until the climb ends or [`CLIMB_WAIT`]
    /// has passed, then answering with one reading — another front end, a mute or the knob may
    /// end a climb short of its target, and one still going when the wait runs out is answered
    /// where it has got to. No read holds the answer past the wait, however long the daemon takes
    /// over it.
    async fn glide_set(&self, info: &DeviceInfo, control: &Control, target: i64) -> Done {
        match self.fleet.glide(info, &control.key, target).await {
            Glided::Landed(Response::Value(vo)) => Ok(reply(info, control, &vo)),
            Glided::Climbing { .. } => {
                let until = Instant::now() + CLIMB_WAIT;
                if let Some(watched) = self.fleet.watch(info) {
                    let over = self.climb_over(info, control, target, watched.climb);
                    if let Ok(over) = tokio::time::timeout_at(until, over).await {
                        over?;
                    }
                }
                let last = self.fleet.request(Request::Get {
                    device: Some(info.name.clone()),
                    key: control.key.clone(),
                });
                match tokio::time::timeout_at(until + CLIMB_GRACE, last).await {
                    Ok(Response::Value(vo)) => Ok(reply(info, control, &vo)),
                    Ok(other) => Err(answer_error(other).into()),
                    Err(_) => {
                        Err(format!("{} did not answer within {CLIMB_WAIT:?}", info.name).into())
                    }
                }
            }
            Glided::Landed(other) | Glided::Refused(other) => Err(answer_error(other).into()),
            Glided::Superseded => Err(format!(
                "a newer write to {} on {} took this one's place",
                control.key, info.name
            )
            .into()),
        }
    }

    /// Wait until the climb of `control` is over on `climb`. The writer shows a climb a moment
    /// after it answers that one began, so a watch that shows none yet is told apart from a
    /// climb already over by one reading: at `target` it is over; short of it, the climb is
    /// awaited when it shows.
    async fn climb_over(
        &self,
        info: &DeviceInfo,
        control: &Control,
        target: i64,
        mut climb: tokio::sync::watch::Receiver<Option<ClimbView>>,
    ) -> Result<(), Stop> {
        let shows = |view: &Option<ClimbView>| view.as_ref().is_some_and(|v| v.key == control.key);
        if !shows(&climb.borrow_and_update()) {
            if self.read_raw(info, &control.key).await? == Some(target) {
                return Ok(());
            }
            if climb.has_changed().unwrap_or(true) && !shows(&climb.borrow_and_update()) {
                // It showed and ended while the reading was out.
                return Ok(());
            }
            if climb.wait_for(shows).await.is_err() {
                return Ok(());
            }
        }
        let _ = climb.wait_for(|view| !shows(view)).await;
        Ok(())
    }

    /// The device `selector` names and its profile.
    fn device(&self, selector: Option<&str>) -> Result<(DeviceInfo, Profile), String> {
        let info = self.info(selector)?;
        let profile = self.fleet.profile(&info).ok_or_else(|| {
            format!(
                "the profile {:?} of {} is not known",
                info.profile, info.name
            )
        })?;
        Ok((info, profile))
    }

    fn info(&self, selector: Option<&str>) -> Result<DeviceInfo, String> {
        self.fleet.resolve(selector)
    }
}

/// A refused or failed request in words.
fn answer_error(response: Response) -> String {
    match response {
        Response::Error { message, .. } => message,
        other => format!("the daemon answered unexpectedly: {other:?}"),
    }
}

/// A tool error: the model reads the words and may try again.
fn failure(message: &str) -> CallToolResponse {
    CallToolResult::error(vec![rmcp::model::ContentBlock::text(message)]).into()
}

/// What the server answers `initialize` and `server/discover` with, and how it lists and runs
/// the tools.
#[derive(Clone)]
pub struct Handler {
    pub tools: Tools,
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            rmcp::model::Implementation::new("svolder", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let newer = context
            .protocol_version()
            .is_some_and(|v| v.as_str() >= ProtocolVersion::V_2026_07_28.as_str());
        let elicits = context.client_capabilities().is_some_and(|c| {
            // An empty declaration means forms; one that names only URLs cannot show one.
            c.elicitation
                .as_ref()
                .is_some_and(|e| e.form.is_some() || e.url.is_none())
        });
        self.tools
            .call(
                request,
                Caller {
                    can_ask: newer && elicits,
                },
            )
            .await
    }
}

#[cfg(test)]
mod tests;
