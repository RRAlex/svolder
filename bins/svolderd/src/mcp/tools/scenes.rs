// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scene tools: `list_scenes` reads the daemon's view of the scenes; `load_scene`,
//! `save_scene`, `rename_scene` and `delete_scene` send one scene request each and answer in the
//! daemon's words.
//!
//! A scene never holds a volume, a gain select, a mute or a power switch, so no tool here can
//! write one. A recall asks only where a change of sound can surprise:
//!
//! - the daemon's own question, that the scene would play one preset on a device and on the
//!   computer at once, which it answers before changing anything;
//! - a built-in band of a device (a Loki Max's) that the scene raises more than [`MAX_RISE_DB`]
//!   above its lowest level of the last minute, judged by the same rule `set_control` and
//!   `load_preset` use, before any request goes out.
//!
//! A recall that asks nothing of either kind goes straight through. `save_scene` over a name that
//! is taken needs `replace: true`, which asks once; `delete_scene` asks once; `rename_scene`
//! never asks.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, Tool, ToolAnnotations};
use serde_json::{Value, json};
use svolder_ipc::{PartOutcome, Reason, Request, Response, SceneOp};
use svolder_profiles::safety;
use svolder_profiles::tone::preset::same_name;

use super::{
    ASK, Args, Ask, Caller, Done, Guard, MAX_RISE_DB, Stop, Tools, answer_error, permission,
};

/// One built-in band a recall sets, judged.
struct Band {
    device: String,
    key: String,
    guard: Guard,
}

impl Tools {
    /// The scenes as the daemon's view lists them: each with its parts and whether each can be
    /// recalled now, which one is on and which was last recalled.
    pub(super) async fn list_scenes(&self, request: &CallToolRequestParams) -> Done {
        Args::new(request, &[])?;
        let Some(view) = self.fleet.scenes() else {
            // A daemon on its way out says so in its own words.
            self.fleet.config()?;
            return Err("this daemon keeps no scenes".to_string().into());
        };
        let view = view.borrow().clone();
        serde_json::to_value(view).map_err(|e| format!("could not list the scenes: {e}").into())
    }

    /// Recall a scene. The built-in bands it raises are judged first; the daemon's both-sides
    /// question is put when it asks it. Answers with the outcome of every part.
    pub(super) async fn load_scene(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["name", "confirm", "confirm_both_sides"])?;
        let name = args.required_text("name")?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        let both_confirmed = args.flag("confirm_both_sides")?.unwrap_or(false);
        let label = self
            .fleet
            .config()?
            .scene(name)
            .map_or_else(|| name.trim().to_string(), |scene| scene.name.clone());
        let bands = self.scene_bands(&label).await?;
        let judged = judged_state(&bands);
        let both_state = format!("scene|{label}|both|{judged}");
        // The daemon's question comes only after the bands were agreed to, and carries what was
        // judged, so a retry that answers it needs no second yes to the bands.
        let carried = request.request_state.as_deref() == Some(both_state.as_str())
            && request
                .input_responses
                .as_ref()
                .is_some_and(|answers| answers.contains_key(ASK));
        let questions: Vec<&str> = bands
            .iter()
            .filter_map(|band| band.guard.ask.as_ref())
            .map(|ask| ask.message.as_str())
            .collect();
        if !questions.is_empty() && !carried {
            let ask = Ask {
                message: questions.join("; "),
                title: format!("Recall {label}"),
                state: format!("scene|{label}|bands|{judged}"),
            };
            permission(
                &ask,
                request,
                caller,
                confirmed,
                "no scene is recalled and the bands stay as they are",
            )?;
        }
        let mut answer = self.scene_request(recall_op(name, false)).await;
        if let Response::Ask { message, .. } = answer {
            // A client that cannot ask answers this question with its own flag: the `confirm` that
            // agreed to a band is no answer to it.
            if !caller.can_ask && !both_confirmed {
                let repeat = match questions.is_empty() {
                    true => "\"confirm_both_sides\": true",
                    false => "\"confirm_both_sides\": true and \"confirm\": true",
                };
                return Err(format!(
                    "{message}; ask the user whether to do it, and if they agree repeat this call \
                     with {repeat}"
                )
                .into());
            }
            let ask = Ask {
                message,
                title: format!("Recall {label}"),
                state: both_state,
            };
            permission(
                &ask,
                request,
                caller,
                both_confirmed,
                "no scene is recalled",
            )?;
            answer = self.scene_request(recall_op(name, true)).await;
        }
        let Response::Scene { name, outcome } = answer else {
            return Err(answer_error(answer).into());
        };
        self.settle_floors(&bands, &outcome);
        Ok(json!({ "name": name, "outcome": outcome }))
    }

    /// Save the settings as they are now under a name. A name that is taken is refused unless
    /// `replace` is true, which asks once.
    pub(super) async fn save_scene(&self, request: &CallToolRequestParams, caller: Caller) -> Done {
        let args = Args::new(request, &["name", "replace", "confirm"])?;
        let name = args.required_text("name")?;
        let replace = args.flag("replace")?.unwrap_or(false);
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        let taken = self
            .fleet
            .config()?
            .scene(name)
            .map(|scene| scene.name.clone());
        let force = match (&taken, replace) {
            (Some(own), true) => {
                let ask = Ask {
                    message: format!(
                        "saving over the scene {own} replaces what {own} holds with the current \
                         settings; no volume, gain, mute or power is saved"
                    ),
                    title: format!("Replace {own}"),
                    state: format!("scene_save|{own}"),
                };
                permission(
                    &ask,
                    request,
                    caller,
                    confirmed,
                    &format!("{own} stays as it is"),
                )?;
                true
            }
            _ => false,
        };
        let op = SceneOp::Save {
            name: name.to_string(),
            force,
        };
        match self.scene_request(op).await {
            Response::Scene { name, outcome } => Ok(json!({ "name": name, "outcome": outcome })),
            Response::Error {
                message,
                reason: Some(Reason::Taken),
                ..
            } => Err(format!(
                "{message}; to save over it, repeat this call with \"replace\": true, which asks \
                 the user first"
            )
            .into()),
            other => Err(answer_error(other).into()),
        }
    }

    /// Rename a scene, leaving what it holds as it is.
    pub(super) async fn rename_scene(&self, request: &CallToolRequestParams) -> Done {
        let args = Args::new(request, &["from", "to"])?;
        let op = SceneOp::Rename {
            from: args.required_text("from")?.to_string(),
            to: args.required_text("to")?.to_string(),
        };
        match self.scene_request(op).await {
            Response::Scene { name, .. } => Ok(json!({ "name": name })),
            other => Err(answer_error(other).into()),
        }
    }

    /// Delete a scene, once the user has agreed. A name no scene has is the daemon's refusal, with
    /// no question.
    pub(super) async fn delete_scene(
        &self,
        request: &CallToolRequestParams,
        caller: Caller,
    ) -> Done {
        let args = Args::new(request, &["name", "confirm"])?;
        let name = args.required_text("name")?;
        let confirmed = args.flag("confirm")?.unwrap_or(false);
        if let Some(own) = self
            .fleet
            .config()?
            .scene(name)
            .map(|scene| scene.name.clone())
        {
            let ask = Ask {
                message: format!(
                    "deleting the scene {own} removes it from the configuration and cannot be undone"
                ),
                title: format!("Delete {own}"),
                state: format!("scene_delete|{own}"),
            };
            permission(&ask, request, caller, confirmed, &format!("{own} is kept"))?;
        }
        let op = SceneOp::Delete {
            name: name.to_string(),
        };
        match self.scene_request(op).await {
            Response::Scene { name, .. } => Ok(json!({ "name": name })),
            other => Err(answer_error(other).into()),
        }
    }

    async fn scene_request(&self, op: SceneOp) -> Response {
        self.fleet.request(Request::Scene { op }).await
    }

    /// Every built-in band the scene `label` sets on a connected device, judged as a band set by
    /// hand is. A scene the config does not hold has none: the daemon refuses it in words.
    async fn scene_bands(&self, label: &str) -> Result<Vec<Band>, Stop> {
        let config = self.fleet.config()?;
        let Some(scene) = config.scene(label) else {
            return Ok(Vec::new());
        };
        let mut judged = Vec::new();
        let devices = match self.fleet.request(Request::Devices).await {
            Response::Devices { devices } => devices,
            other => return Err(answer_error(other).into()),
        };
        for part in &scene.devices {
            let Some(bands) = &part.raw_bands else {
                continue;
            };
            let Some(info) = devices
                .iter()
                .find(|d| d.connected && d.unit.eq_ignore_ascii_case(&part.udid))
            else {
                continue;
            };
            let Some(profile) = self.fleet.profile(info) else {
                continue;
            };
            for (key, raw) in bands {
                let Some(control) = profile.control_by_key(key) else {
                    continue;
                };
                if !safety::in_db(control) {
                    continue;
                }
                if let Some(guard) = self
                    .level_guard(info, control, *raw, &Value::Null, true)
                    .await?
                {
                    judged.push(Band {
                        device: info.name.clone(),
                        key: control.key.clone(),
                        guard,
                    });
                }
            }
        }
        Ok(judged)
    }

    /// After a recall: a band that asked and was set whole starts its window over at the level
    /// agreed to, and a band that did not ask notes its new level. A device whose part was left,
    /// failed or partly set keeps what the readings say.
    fn settle_floors(&self, bands: &[Band], outcome: &[PartOutcome]) {
        for band in bands {
            let whole = outcome.iter().any(|part| {
                same_name(&part.who, &band.device) && part.status == svolder_ipc::PartStatus::Done
            });
            let Some(level) = band.guard.level.filter(|_| whole) else {
                continue;
            };
            match band.guard.ask {
                Some(_) => self.floors.reset(&band.device, &band.key, level),
                None => self.floors.record(&band.device, &band.key, level),
            }
        }
    }
}

fn recall_op(name: &str, confirm: bool) -> SceneOp {
    SceneOp::Recall {
        name: name.to_string(),
        confirm,
    }
}

/// What a question about bands states: each band that asks, with its target and the floor it was
/// judged against, so a yes given under other floors does not answer this question.
fn judged_state(bands: &[Band]) -> String {
    let level = |db: Option<f64>| db.map_or("none".to_string(), |db| format!("{db:.1}"));
    bands
        .iter()
        .filter(|band| band.guard.ask.is_some())
        .map(|band| {
            format!(
                "{}.{}={}@{}",
                band.device,
                band.key,
                level(band.guard.level),
                level(band.guard.floor_db)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The definition of a scene tool.
pub(super) fn definition(name: &'static str, write: bool) -> Tool {
    let scene = json!({ "type": "string", "description": "The scene's name from list_scenes" });
    let (description, properties, required): (String, Value, &[&str]) = match name {
        "list_scenes" => (
            "List the scenes: each with its parts (`computer`, or a device's name, with `present` \
             true when the part can be recalled now), the one that is on (`on`, absent while none \
             is or while it is not known yet: `known` false), and the one last recalled. A scene \
             saves sound settings only: no volume, gain, mute or power is saved or recalled."
                .to_string(),
            json!({}),
            &[],
        ),
        "load_scene" => (
            format!(
                "Recall a scene from list_scenes: the computer's EQ, loudness and crossfeed and \
                 each connected device's tone, bands and other sound settings, every part by its \
                 own safe order; a part of a device that is not connected is left as it is. No \
                 volume, gain, mute or power is changed. A plain recall asks nothing. It asks the \
                 user first when the scene would play one preset on a device and on the computer \
                 at once, and when it sets a built-in band of a device more than {MAX_RISE_DB} dB \
                 above that band's lowest level of the last minute (naming each such band). The \
                 answer lists each part with its status (done, partial, left or failed) and the \
                 daemon's notes."
            ),
            json!({
                "name": scene,
                "confirm": { "type": "boolean", "description": "Only after the user agreed to the band question the call raised, and only for a client that cannot ask the user itself; ignored when it can" },
                "confirm_both_sides": { "type": "boolean", "description": "Only after the user agreed to the question that the scene would play one preset on a device and on the computer at once, and only for a client that cannot ask the user itself; ignored when it can; `confirm` does not answer it" }
            }),
            &["name"],
        ),
        "save_scene" => (
            "Save the settings as they are now under a name: the computer's and those of every connected device. No volume, gain, mute or power is saved. A name that is taken is refused unless `replace` is true, which asks the user first and saves over that scene. The answer lists a part left out for each configured device that is not connected."
                .to_string(),
            json!({
                "name": { "type": "string", "description": "The name to save under, for example Late night" },
                "replace": { "type": "boolean", "description": "true to save over a scene of that name; asks the user first" },
                "confirm": { "type": "boolean", "description": "Only after the user agreed to replace the scene, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["name"],
        ),
        "rename_scene" => (
            "Rename a scene, leaving what it holds as it is; it asks nothing.".to_string(),
            json!({
                "from": scene,
                "to": { "type": "string", "description": "The new name" }
            }),
            &["from", "to"],
        ),
        _ => (
            "Delete a scene; it asks the user first and cannot be undone.".to_string(),
            json!({
                "name": scene,
                "confirm": { "type": "boolean", "description": "Only after the user agreed to delete the scene, and only for a client that cannot ask the user itself; ignored when it can" }
            }),
            &["name"],
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
