// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The volume and unmute guards of the tools: the floors a rise is measured from, the questions a
//! change asks, the answers a retry carries and the checks on a gain select.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use rmcp::model::{
    CallToolRequestParams, ElicitRequest, ElicitRequestParams, InputRequest, InputRequests,
    InputRequiredResult,
};
use serde_json::{Value, json};
use svolder_ipc::{DeviceInfo, Event};
use svolder_profiles::safety::{self, FLOOR_WINDOW};
use svolder_profiles::units::{self, Outside, Refusal};
use svolder_profiles::{Control, device_presets, roles};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{ASK, Caller, Stop};
use crate::fleet::Fleet;
use crate::locks::locked;

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
    pub(super) fn record(&self, device: &str, key: &str, level: f64) {
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
    pub(super) fn floor(&self, device: &str, key: &str) -> Option<f64> {
        let now = Instant::now();
        let seen = locked(&self.seen);
        seen.get(&(device.to_ascii_lowercase(), key.to_ascii_lowercase()))?
            .iter()
            .filter(|(at, _)| now.duration_since(*at) <= FLOOR_WINDOW)
            .map(|(_, level)| *level)
            .min_by(f64::total_cmp)
    }

    /// Start the window over at `level`, after the user agreed to a rise to it.
    pub(super) fn reset(&self, device: &str, key: &str, level: f64) {
        locked(&self.seen).remove(&(device.to_ascii_lowercase(), key.to_ascii_lowercase()));
        self.record(device, key, level);
    }

    /// Note every level in dB the devices report, until `stop`: a knob turned down is a lower
    /// floor whether or not a tool saw it.
    pub(super) async fn watch(self: Arc<Self>, fleet: Arc<dyn Fleet>, stop: CancellationToken) {
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
pub(super) enum LetThrough {
    /// A mute released.
    Unmute,
    /// A device powered on.
    PowerOn,
}

impl LetThrough {
    pub(super) fn gerund(self) -> &'static str {
        match self {
            LetThrough::Unmute => "unmuting",
            LetThrough::PowerOn => "powering on",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            LetThrough::Unmute => "Unmute",
            LetThrough::PowerOn => "Power on",
        }
    }

    /// The first field of the question's state.
    pub(super) fn tag(self) -> &'static str {
        match self {
            LetThrough::Unmute => "unmute",
            LetThrough::PowerOn => "poweron",
        }
    }
}

/// The question a gain select raised to raw value `new` from `current` asks.
pub(super) fn gain_guard(
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
pub(super) fn level_raw(control: &Control, value: &Value) -> Result<i64, String> {
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
pub(super) fn plain_input(value: &Value) -> Result<String, String> {
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
pub(super) struct Guard {
    pub(super) level: Option<f64>,
    /// The lowest level of the last minute the check judged a level against, when it judged one.
    pub(super) floor_db: Option<f64>,
    pub(super) ask: Option<Ask>,
    /// The raw step a volume is set to through the device's writer, which judges on its own
    /// reading whether it lands at once or climbs; `None` for every other control.
    pub(super) glide: Option<i64>,
}

/// A question the user must answer before a change goes through.
pub(super) struct Ask {
    /// The change in a sentence, which a client that cannot ask is told too.
    pub(super) message: String,
    /// The short title of the question.
    pub(super) title: String,
    /// What the retry must echo for its answer to count for this very change.
    pub(super) state: String,
}

/// Whether `name` is the save action's key, alone or followed by a slot (`save`, `save 2`,
/// `Save Preset 2`), blanks and case aside.
pub(super) fn names_a_save(name: &str) -> bool {
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
pub(super) fn permission(
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
