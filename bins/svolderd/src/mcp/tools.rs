// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tools the MCP server offers: five that read (four a device, `get_host` the computer) and,
//! with `[mcp] writes`, seven that act: three that change a device, `identify`, which asks it to
//! show itself, and three for the computer (see [`host`]).
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

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ElicitRequest, ElicitRequestParams,
    ErrorData, InputRequest, InputRequests, InputRequiredResult, JsonObject, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::{Map, Value, json};
use svolder_ipc::{DeviceInfo, Event, Request, Response, ValueObject};
use svolder_profiles::roles;
use svolder_profiles::safety::{self, FLOOR_WINDOW, Rules, Situation, Through};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::units::{self, Amount, Outside, Refusal};

use crate::device::Command;
use crate::fleet::Fleet;
use crate::locks::locked;
use crate::writer::{Glided, Mode, Options, Outcome};
use svolder_profiles::device_presets;
use svolder_profiles::{Control, ControlKind, Profile, convert, intent};
use svolder_proto::{FunctionClass, FunctionId};

pub use svolder_profiles::safety::MAX_RISE_DB;

mod host;

/// The longest span `get_link_history` answers for, in hours: what the daemon keeps.
const MAX_HISTORY_HOURS: f64 = 24.0;

/// The longest a level change waits for the climb it started before the tool answers with the
/// level the device has reached.
const CLIMB_WAIT: Duration = Duration::from_secs(5);

/// How often the level is read while the tool waits for a climb.
const CLIMB_POLL: Duration = Duration::from_millis(50);

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

/// The levels one control was noted at, oldest first.
type Window = VecDeque<(Instant, f64)>;

/// The levels in dB each device's controls were seen at in the last [`FLOOR_WINDOW`], shared by
/// every request: the lowest of them is what a rise is measured from, so a staircase of small
/// steps adds up to one big rise instead of passing each as a small one.
#[derive(Default)]
pub struct Floors {
    seen: Mutex<HashMap<(String, String), Window>>,
}

impl Floors {
    /// Note that `key` on `device` is at `level` now.
    fn record(&self, device: &str, key: &str, level: f64) {
        let now = Instant::now();
        let mut seen = locked(&self.seen);
        let window = seen
            .entry((device.to_ascii_lowercase(), key.to_ascii_lowercase()))
            .or_default();
        while window
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > FLOOR_WINDOW)
        {
            window.pop_front();
        }
        window.push_back((now, level));
    }

    /// The lowest level noted for `key` on `device` within the window.
    fn floor(&self, device: &str, key: &str) -> Option<f64> {
        let now = Instant::now();
        let seen = locked(&self.seen);
        seen.get(&(device.to_ascii_lowercase(), key.to_ascii_lowercase()))?
            .iter()
            .filter(|(at, _)| now.duration_since(*at) <= FLOOR_WINDOW)
            .map(|(_, level)| *level)
            .min_by(f64::total_cmp)
    }

    /// Start the window over at `level`, after the user agreed to a rise to it.
    fn reset(&self, device: &str, key: &str, level: f64) {
        locked(&self.seen).remove(&(device.to_ascii_lowercase(), key.to_ascii_lowercase()));
        self.record(device, key, level);
    }

    /// Note every level in dB the devices report, until `stop`: a knob turned down is a lower
    /// floor whether or not a tool saw it.
    async fn watch(self: Arc<Self>, fleet: Arc<dyn Fleet>, stop: CancellationToken) {
        let mut events = fleet.events().await;
        loop {
            let event = tokio::select! {
                () = stop.cancelled() => return,
                event = events.next() => event,
            };
            let Some(Event::Status { device, reading }) = event else {
                if event.is_none() {
                    return;
                }
                continue;
            };
            if let Some(key) = reading.key
                && reading
                    .unit
                    .as_deref()
                    .is_some_and(|u| u.eq_ignore_ascii_case("dB"))
            {
                self.record(&device, &key, reading.value.unwrap_or(f64::NEG_INFINITY));
            }
        }
    }
}

/// What lets sound through that a guard asks about.
#[derive(Clone, Copy)]
enum LetThrough {
    /// A mute released.
    Unmute,
    /// A device powered on.
    PowerOn,
}

impl LetThrough {
    fn gerund(self) -> &'static str {
        match self {
            LetThrough::Unmute => "unmuting",
            LetThrough::PowerOn => "powering on",
        }
    }

    fn title(self) -> &'static str {
        match self {
            LetThrough::Unmute => "Unmute",
            LetThrough::PowerOn => "Power on",
        }
    }

    /// The first field of the question's state.
    fn tag(self) -> &'static str {
        match self {
            LetThrough::Unmute => "unmute",
            LetThrough::PowerOn => "poweron",
        }
    }
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

/// The tool names, each with whether it changes a device.
const TOOLS: [(&str, bool); 12] = [
    ("list_devices", false),
    ("get_state", false),
    ("get_link_history", false),
    ("list_presets", false),
    ("get_host", false),
    ("set_control", true),
    ("load_preset", true),
    ("mute", true),
    ("identify", true),
    ("set_host_volume", true),
    ("host_mute", true),
    ("set_host_eq", true),
];

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
            Response::Error { message, .. } if message == svolder_ipc::NOT_CONNECTED => {
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
            Response::Error { code: 4, .. } => Ok(None),
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
    /// echo; a climb by reading the level until it is `target` or [`CLIMB_WAIT`] has passed, then
    /// answering with the reading — another front end, a mute or the knob may end a climb short
    /// of its target, and one still going when the wait runs out is answered where it has got to.
    /// No read holds the answer past the wait, however long the daemon takes over it.
    async fn glide_set(&self, info: &DeviceInfo, control: &Control, target: i64) -> Done {
        match self.fleet.glide(info, &control.key, target).await {
            Glided::Landed(Response::Value(vo)) => Ok(reply(info, control, &vo)),
            Glided::Climbing { .. } => {
                let until = Instant::now() + CLIMB_WAIT;
                while Instant::now() < until {
                    match tokio::time::timeout_at(until, self.read_raw(info, &control.key)).await {
                        Ok(read) => match read? {
                            Some(raw) if raw == target => break,
                            _ => tokio::time::sleep(CLIMB_POLL).await,
                        },
                        Err(_) => break,
                    }
                }
                let last = self.fleet.request(Request::Get {
                    device: Some(info.name.clone()),
                    key: control.key.clone(),
                });
                match tokio::time::timeout_at(until + CLIMB_POLL, last).await {
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

/// What a set reports: the reading the device confirmed, a level counted the way `get_state`
/// counts it.
fn reply(info: &DeviceInfo, control: &Control, vo: &ValueObject) -> Value {
    let origin = roles::origin(control);
    let display = match vo.raw.as_i64() {
        Some(raw) if control.kind == ControlKind::Level && origin != 0 => {
            units::text(control, raw).unwrap_or_else(|| vo.display.clone())
        }
        _ => vo.display.clone(),
    };
    let value = match control.kind {
        ControlKind::Level => value_of(control, vo),
        _ => json!(vo.value),
    };
    json!({
        "device": info.name,
        "key": vo.key,
        "label": vo.label,
        "value": value,
        "unit": vo.unit,
        "display": display,
    })
}

/// The question a gain select raised to raw value `new` from `current` asks.
fn gain_guard(
    info: &DeviceInfo,
    control: &Control,
    current: Option<i64>,
    new: Option<i64>,
) -> Guard {
    let name = |raw: Option<i64>| {
        raw.and_then(|r| u8::try_from(r).ok())
            .and_then(|r| control.value_name(r))
            .map_or("an unknown setting".to_string(), str::to_string)
    };
    Guard {
        level: None,
        floor_db: None,
        glide: None,
        ask: Some(Ask {
            message: format!(
                "changing {} on {} from {} to {} raises the output",
                control.label.to_ascii_lowercase(),
                info.name,
                name(current),
                name(new)
            ),
            title: format!("Change the {}", control.label.to_ascii_lowercase()),
            state: format!(
                "gain|{}|{}|{}|{}",
                info.name,
                control.key,
                new.map_or("none".to_string(), |n| n.to_string()),
                current.map_or("none".to_string(), |n| n.to_string())
            ),
        }),
    }
}

/// A level's raw step for the number the model gave, in the control's own unit (dB for the
/// volume; steps from the centre for a balance), snapped to the nearest step the device has. A
/// number outside the control's range is refused, not clamped.
fn level_raw(control: &Control, value: &Value) -> Result<i64, String> {
    let bad = || {
        format!(
            "{} takes a number{}",
            control.key,
            control
                .unit
                .as_deref()
                .map(|u| format!(" in {u}"))
                .unwrap_or_default()
        )
    };
    let number = match value {
        Value::Number(n) => n.as_f64().filter(|n| n.is_finite()),
        Value::String(s) => units::number_in(s, control.unit.as_deref()),
        _ => None,
    }
    .ok_or_else(bad)?;
    if roles::is_volume(control) && !safety::in_db(control) {
        return Err(
            "the volume of this device has no level in dB, so it cannot be set here".to_string(),
        );
    }
    let unit = control
        .unit
        .as_deref()
        .map(|u| format!(" {u}"))
        .unwrap_or_default();
    match units::raw_for(control, number, Outside::Refuse) {
        Ok(raw) => Ok(raw),
        Err(Refusal::Outside { min, max }) => Err(format!(
            "{} is between {min} and {max}{unit}; {number} is outside that, and the value is a \
             level, not a change",
            control.key
        )),
        Err(Refusal::NotFinite | Refusal::NoRange) => Err(bad()),
    }
}

/// A toggle's or select's value as text for the set path, which reads on/off and names.
fn plain_input(value: &Value) -> Result<String, String> {
    match value {
        Value::Bool(true) => Ok("on".to_string()),
        Value::Bool(false) => Ok("off".to_string()),
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err("value must be a boolean, a string or a number".to_string()),
    }
}

/// What a check found: the level the write lands on, when it is a level, and a question the
/// user must answer first, when there is one.
struct Guard {
    level: Option<f64>,
    /// The lowest level of the last minute the check judged a level against, when it judged one.
    floor_db: Option<f64>,
    ask: Option<Ask>,
    /// The raw step a volume is set to through the device's writer, which judges on its own
    /// reading whether it lands at once or climbs; `None` for every other control.
    glide: Option<i64>,
}

/// A question the user must answer before a change goes through.
struct Ask {
    /// The change in a sentence, which a client that cannot ask is told too.
    message: String,
    /// The short title of the question.
    title: String,
    /// What the retry must echo for its answer to count for this very change.
    state: String,
}

/// Whether `name` is the save action's key, alone or followed by a slot (`save`, `save 2`,
/// `Save Preset 2`), blanks and case aside.
fn names_a_save(name: &str) -> bool {
    let name = name.trim();
    match name.get(..roles::SAVE_KEY.len()) {
        Some(head) if head.eq_ignore_ascii_case(roles::SAVE_KEY) => {
            let rest = name[roles::SAVE_KEY.len()..].trim();
            rest.is_empty() || device_presets::parse_slot(rest, u8::MAX).is_some()
        }
        _ => false,
    }
}

/// Whether the user has agreed to what `ask` asks: a client that can ask is asked, and goes
/// ahead on an accepted answer to this very question; any other goes ahead on `confirm`. A no is
/// final and says `stays` is left as it is.
fn permission(
    ask: &Ask,
    request: &CallToolRequestParams,
    caller: Caller,
    confirmed: bool,
    stays: &str,
) -> Result<(), Stop> {
    if caller.can_ask {
        match answer(request, &ask.state) {
            Answer::Yes => Ok(()),
            Answer::No => Err(Stop::Say(format!(
                "the user declined: {stays}; do not ask again unless the user does"
            ))),
            Answer::Unasked => Err(Stop::Ask(question(ask)?)),
        }
    } else if !confirmed {
        Err(Stop::Say(format!(
            "{}; ask the user whether to do it, and if they agree repeat this call with \"confirm\": true",
            ask.message
        )))
    } else {
        Ok(())
    }
}

/// What a retry says about the question.
enum Answer {
    Yes,
    No,
    Unasked,
}

/// The user's answer carried by the retry, when it answers this very question: the state the
/// retry echoes must be the one asked with, and the answer an accept with `confirm` true; a
/// decline or a cancel is a no.
fn answer(request: &CallToolRequestParams, state: &str) -> Answer {
    if request.request_state.as_deref() != Some(state) {
        return Answer::Unasked;
    }
    let Some(reply) = request
        .input_responses
        .as_ref()
        .and_then(|answers| answers.get(ASK))
    else {
        return Answer::Unasked;
    };
    match reply["action"].as_str() {
        Some("accept") if reply["content"]["confirm"] == json!(true) => Answer::Yes,
        Some("accept") => Answer::No,
        Some("decline" | "cancel") => Answer::No,
        _ => Answer::Unasked,
    }
}

/// The question a 2026-07-28 client is sent, with the state the retry must echo.
fn question(ask: &Ask) -> Result<InputRequiredResult, Stop> {
    let schema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "confirm": {
                "type": "boolean",
                "title": ask.title,
                "description": "Allow this change"
            }
        },
        "required": ["confirm"]
    }))
    .map_err(|e| Stop::Say(format!("could not build the question: {e}")))?;
    let mut requests = InputRequests::new();
    requests.insert(
        ASK.to_string(),
        InputRequest::Elicitation(ElicitRequest::new(
            ElicitRequestParams::FormElicitationParams {
                meta: None,
                message: format!("{}. Allow it?", ask.message),
                requested_schema: schema,
            },
        )),
    );
    Ok(InputRequiredResult::new(
        Some(requests),
        Some(ask.state.clone()),
    ))
}

/// A control's label inside a sentence: lower case, except a label that starts with a number
/// (`20 Hz`), whose unit keeps its capitals.
fn label_words(control: &Control) -> String {
    match control.label.starts_with(|c: char| c.is_ascii_digit()) {
        true => control.label.clone(),
        false => control.label.to_ascii_lowercase(),
    }
}

/// A level in dB for a sentence: `-6.0 dB`, and `silence` at the lowest step.
fn db_text(db: f64) -> String {
    units::amount_text(Amount::of(db), Some("dB"))
}

/// The keys of the controls `keep` accepts, for a message that names the valid ones.
fn keys_of(profile: &Profile, keep: impl Fn(&Control) -> bool) -> String {
    profile
        .controls
        .iter()
        .filter(|c| keep(c))
        .map(|c| c.key.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A refused or failed request in words.
fn answer_error(response: Response) -> String {
    match response {
        Response::Error { message, .. } => message,
        other => format!("the daemon answered unexpectedly: {other:?}"),
    }
}

/// One control as `get_state` shows it: its key, label and kind, its reading, and what it can
/// be set to.
fn describe_control(profile: &Profile, control: &Control, values: &[ValueObject]) -> Value {
    let id = control.id.to_string();
    let reading = values.iter().find(|v| v.id == id);
    let mut out = Map::new();
    out.insert("key".into(), json!(control.key));
    out.insert("label".into(), json!(control.label));
    out.insert("kind".into(), json!(kind_word(control.kind)));
    out.insert("writable".into(), json!(control.kind.writable()));
    out.insert(
        "value".into(),
        reading.map_or(Value::Null, |r| match profile.gate_of(control) {
            Some(_) => json!(gated(profile, control, r, values)),
            None => value_of(control, r),
        }),
    );
    if let Some(unit) = &control.unit {
        out.insert("unit".into(), json!(unit));
    }
    if !control.values.is_empty() {
        let names: Vec<&str> = control.values.iter().map(|(_, n)| n.as_str()).collect();
        out.insert("values".into(), json!(names));
    }
    if control.kind == ControlKind::Level
        && let Some(range) = level_range(control)
    {
        out.insert("range".into(), range);
    }
    Value::Object(out)
}

/// A level's range as `get_state` shows it: in its unit when it has a conversion, the silent step
/// of a volume left out, else in whole steps counted from its centre, if it has one. `None` for a
/// level with no range.
fn level_range(control: &Control) -> Option<Value> {
    let bounds = units::bounds(control)?;
    Some(match control.convert {
        None => json!({ "min": bounds.min as i64, "max": bounds.max as i64 }),
        Some(_) => json!({ "min": bounds.min, "max": bounds.max }),
    })
}

fn kind_word(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Select => "select",
        ControlKind::Toggle => "toggle",
        ControlKind::Level => "level",
        ControlKind::Readout => "readout",
        ControlKind::Meter => "meter",
        ControlKind::Fault => "fault",
    }
}

/// Whether a readout an `only_while` gates reads true: it and its gate both read true in
/// `values`, a gate not reported counting as closed ([`units::shows_true`]).
fn gated(
    profile: &Profile,
    control: &Control,
    reading: &ValueObject,
    values: &[ValueObject],
) -> bool {
    units::shows_true(profile, control, reads_true(&reading.raw), |gate| {
        let id = gate.id.to_string();
        values
            .iter()
            .find(|v| v.id == id)
            .map(|v| reads_true(&v.raw))
    })
}

/// Whether a raw reading reads true: a boolean's own bit, or an integer 1.
fn reads_true(raw: &Value) -> bool {
    match raw {
        Value::Bool(on) => *on,
        other => other.as_i64() == Some(1),
    }
}

/// A reading's value in the shape the control's kind suggests: a boolean for a toggle or
/// fault, for a level the value built from its raw step (the converted number for a level with a
/// conversion, `null` at silence, the step counted from the centre for a centred one), the name
/// for a select. A meter is read as its raw value: it is never converted here.
fn value_of(control: &Control, reading: &ValueObject) -> Value {
    match control.kind {
        ControlKind::Toggle | ControlKind::Fault => json!(reads_true(&reading.raw)),
        ControlKind::Level | ControlKind::Meter => {
            let origin = units::origin(control);
            let amount = reading
                .raw
                .as_i64()
                .and_then(|raw| units::amount(control, raw).map(|amount| (raw, amount)));
            match amount {
                Some((_, Amount::Value(value))) if control.convert.is_some() => json!(value),
                Some((_, Amount::Silence)) => Value::Null,
                Some((raw, Amount::Value(_))) if origin != 0 => json!(raw - origin),
                _ => reading.raw.clone(),
            }
        }
        ControlKind::Select | ControlKind::Readout => reading
            .raw
            .as_u64()
            .and_then(|r| u8::try_from(r).ok())
            .and_then(|r| control.value_name(r))
            .map_or_else(|| reading.raw.clone(), |name| json!(name)),
    }
}

/// A tool error: the model reads the words and may try again.
fn failure(message: &str) -> CallToolResponse {
    CallToolResult::error(vec![rmcp::model::ContentBlock::text(message)]).into()
}

/// A call's arguments, checked: a name the tool does not take is refused, and each is read
/// as the type the tool wants.
struct Args<'a> {
    map: &'a JsonObject,
}

impl<'a> Args<'a> {
    fn new(request: &'a CallToolRequestParams, allowed: &[&str]) -> Result<Args<'a>, String> {
        static EMPTY: std::sync::OnceLock<JsonObject> = std::sync::OnceLock::new();
        let map = request
            .arguments
            .as_ref()
            .unwrap_or_else(|| EMPTY.get_or_init(JsonObject::new));
        if let Some(extra) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!(
                "{} takes no argument {extra:?}; its arguments are: {}",
                request.name,
                if allowed.is_empty() {
                    "none".to_string()
                } else {
                    allowed.join(", ")
                }
            ));
        }
        Ok(Args { map })
    }

    fn value(&self, key: &str) -> Option<&Value> {
        self.map.get(key).filter(|v| !v.is_null())
    }

    fn text(&self, key: &str) -> Result<Option<&str>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(format!("{key} must be a string")),
        }
    }

    fn required_text(&self, key: &str) -> Result<&str, String> {
        self.text(key)?
            .ok_or_else(|| format!("the argument {key:?} is required"))
    }

    fn number(&self, key: &str) -> Result<Option<f64>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| format!("{key} must be a number")),
        }
    }

    fn flag(&self, key: &str) -> Result<Option<bool>, String> {
        match self.value(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(format!("{key} must be true or false")),
        }
    }
}

/// A tool's definition: its name, its one-sentence description and its arguments.
fn definition(name: &'static str, write: bool) -> Tool {
    let device = json!({
        "type": "string",
        "description": "The device's name from list_devices; may be left out when there is one device"
    });
    let (description, properties, required): (&str, Value, &[&str]) = match name {
        "list_devices" => (
            "List the configured devices with their product name and whether each is connected.",
            json!({}),
            &[],
        ),
        "get_state" => (
            "Read every control of a device with its key, label, kind, current value, unit (the volume is in dB; a balance counts from its centre: 0 centred, below 0 to the left, above to the right) and the values or range it accepts; the keys are the ones set_control takes.",
            json!({ "device": device }),
            &[],
        ),
        "get_link_history" => (
            "Show when a device's Bluetooth link came up and dropped over the last hours (at most 24).",
            json!({
                "device": device,
                "hours": { "type": "number", "description": "How far back to look, in hours; above 0, at most 24; 24 when left out" }
            }),
            &[],
        ),
        "list_presets" => (
            "List a device's presets, each with whether it is the one on (`on`): the tone-control presets, built in and saved, that fit its equalizer; or, on a Loki Max, its three device slots (`slot`, never marked on, since the device does not say which is current) and its five built-in settings.",
            json!({ "device": device }),
            &[],
        ),
        "set_control" => (
            "Set one control of a device by its key from get_state (a level in its unit, dB for the volume, a balance in steps from its centre: 0 centred, below 0 to the left, above to the right; a toggle as true or false; a select by name); a rise of more than 6 dB, a higher gain or mode, an unmute, or powering a device on (the power toggle set to true) asks the user first, and a volume above the owner's limit is refused; a volume above three quarters of its range climbs there, and the reply gives the level the device had reached when the tool answered, which is short of the target if the climb is still going; powering off never asks.",
            json!({
                "device": device,
                "key": { "type": "string", "description": "The control's key from get_state" },
                "value": { "type": ["number", "string", "boolean"], "description": "The new value; for a level the number in its unit (dB for the volume; a balance counts from its centre)" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the change, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["key", "value"],
        ),
        "load_preset" => (
            "Apply a preset from list_presets: a tone-control preset to a device's equalizer, which does not touch the volume control, but each band boosts at most 6 dB and bands that overlap add up, so it can make some frequencies louder; or, on a Loki Max, a device slot (its recall always asks the user first, because the bands it sets are not known and any of them can be set as high as +12 dB) or a built-in setting (it asks the user first when a band would rise more than 6 dB above its lowest level of the last minute, naming each such band). Saving to a slot is not offered.",
            json!({
                "device": device,
                "name": { "type": "string", "description": "The preset's name from list_presets" },
                "confirm": { "type": "boolean", "description": "Only for a Loki Max, only after the user agreed to the question the call raised, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["name"],
        ),
        "get_host" => (
            "Read the computer's volume (percent, muted, the sink), its EQ (on or off, the preset it came from or custom, the preamp in dB and each point's type, frequency, gain and Q) and the player (title, artist, album, status); each is null while it is not available, and `unavailable` says why.",
            json!({}),
            &[],
        ),
        "set_host_volume" => (
            "Set the computer's volume in whole percent, 0 to 100 (never above 100); a rise of more than 6 dB over its lowest level in the last minute, on the scale the computer's mixer uses (the cube of the percent), asks the user first; lowering never asks.",
            json!({
                "percent": { "type": "integer", "minimum": 0, "maximum": 100, "description": "The new volume in percent, 0 to 100" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the rise, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["percent"],
        ),
        "host_mute" => (
            "Mute or unmute the computer's output; unmuting asks the user first.",
            json!({
                "muted": { "type": "boolean", "description": "true to mute, false to unmute; unmuting asks the user first" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to unmute, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["muted"],
        ),
        "set_host_eq" => (
            "Recall a preset on the computer's EQ (`preset`, a saved preset or a built-in one, which is shaped to a device's tone control: name that device in `device`, or leave it out when there is one) or turn the EQ off (`off`: true); give exactly one of them. A preset a connected device plays too asks the user first, since the correction would play twice.",
            json!({
                "preset": { "type": "string", "description": "The preset's name from list_presets" },
                "off": { "type": "boolean", "description": "true to turn the computer's EQ off" },
                "device": device,
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the question the call raised, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &[],
        ),
        "identify" => (
            "Ask a device to show itself, so it can be told apart from the others; it changes nothing on the device, and a second call within two seconds of one that went out sends nothing and answers sent false.",
            json!({ "device": device }),
            &[],
        ),
        _ => (
            "Mute or unmute a device's output; unmuting asks the user first.",
            json!({
                "device": device,
                "on": { "type": "boolean", "description": "true to mute, false to unmute; unmuting asks the user first" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to unmute, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["on"],
        ),
    };
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    });
    let Value::Object(schema) = schema else {
        unreachable!("an object literal");
    };
    let mut tool = Tool::new(name, description, Arc::new(schema));
    tool.annotations = Some(ToolAnnotations::new().read_only(!write));
    tool
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
