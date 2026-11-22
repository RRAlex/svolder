// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recalling a scene: every part is planned first, the both-sides question is asked once, and
//! then the computer and every device are changed at once, each by its own safe order.
//!
//! - The computer's part goes through the controller's one composite change
//!   ([`HostService::scene_recall`](crate::host::HostService::scene_recall)); a refusal leaves it
//!   as it was.
//! - A device's part goes through that device's writer, in arrival order: the tone as a recall
//!   batch of one preset (the writer re-plans it when it starts and says what it clamped), the
//!   built-in bands of a device with no tone control as a set of values, then each held control
//!   as an ordered write that the writer leaves unsent only when the device reads the step and
//!   nothing is in the line for the control. What a part holds is handed over whole, never
//!   cut down to what differs from the readings of the moment: a write already in the line
//!   could otherwise land after the recall and leave the device on another state than the
//!   scene's, with the part answered done. No volume, gain select, mute or power is among them
//!   ([`svolder_profiles::scene::held_controls`] is the one rule).
//! - A part of a device that is not connected is left as it is, in words.
//!
//! Nothing is rolled back: a scene whose parts did not all land says so part by part, and is
//! then not the scene that is on.

use std::sync::Arc;
use std::time::Duration;

use svolder_host::eq::{ComputerPart, LEVEL_HELD_NOTE};
use svolder_ipc::socket::protocol::{PartOutcome, PartStatus, Reason};
use svolder_ipc::{Config, Response};
use svolder_profiles::Profile;
use svolder_profiles::scene::{self, DevicePart, DevicePlan};
use svolder_profiles::tone::preset::{self, BothSides, same_name};
use svolder_proto::Message;
use tokio::time::Instant;

use super::save::{refuse, usable};
use super::view::raw_of;
use super::words::{self, custom_cased};
use crate::device::DeviceHandle;
use crate::handler::{ACTOR_DEADLINE, Daemon, actor_gone, actor_silent};
use crate::writer::{Origin, Outcome, Planned, Recalling};

/// One part of a recall, planned.
enum Slot {
    /// A part that is not recalled: already answered.
    Left(PartOutcome),
    /// The computer's part.
    Computer(Box<ComputerPart>),
    /// A connected device's part.
    Device(Box<Work>),
}

/// A device's part and what recalling it does to the device now.
struct Work {
    /// The device's configured name.
    who: String,
    device: Arc<DeviceHandle>,
    profile: Arc<Profile>,
    /// The readings the plan was made from.
    readings: Vec<Message>,
    part: DevicePart,
    plan: DevicePlan,
}

/// Recall the scene `name`. Answers with the scene's name as the config spells it and how each
/// part fared: the computer's first, then the devices in the scene's order.
///
/// A scene that would play one preset on a device and on the computer at once is answered with
/// the question ([`Response::Ask`]) and changes nothing until it is asked again with `confirm`,
/// which recalls the scene as it was saved.
pub(crate) async fn recall(
    daemon: &Daemon,
    origin: Origin,
    name: String,
    confirm: bool,
) -> Response {
    if daemon.is_stopping() {
        return refuse("the daemon is stopping", Reason::Stopping);
    }
    // The recall's place in the line is taken on arrival, before the question's wait for a fit.
    let scenes = daemon.scenes();
    let turn = scenes.take_turn().await;
    let config = daemon.config_snapshot();
    let Some(scene) = config.scene(&name).cloned() else {
        let words = format!("no scene named {:?}", name.trim());
        return refuse(words, Reason::Missing);
    };
    let devices = daemon.devices();
    let mut slots = Vec::new();
    if let Some(computer) = &scene.computer {
        slots.push(Slot::Computer(Box::new(computer.clone())));
    }
    for part in &scene.devices {
        slots.push(device_slot(&config, &devices, part));
    }
    // A computer part that cannot be applied plays nothing twice, so it asks nothing.
    if !confirm
        && let Some(view) = daemon.host().scene_eq()
        && let Some((preset, held)) =
            both_sides(&config, scene.computer.as_ref(), &view, &devices, &slots).await
    {
        return Response::Ask {
            message: words::plays_twice(&scene.name, &preset, &held),
            name: preset,
            devices: held,
        };
    }
    // The memory is set before anything is awaited and the asks are all made in the first poll of
    // the parts, while this recall still holds its turn, so they come in the order the recalls
    // arrived; only the file's write follows them.
    scenes.set_recalled(Some(scene.name.clone()));
    let mut parts = std::pin::pin!(futures::future::join_all(
        slots.into_iter().map(|slot| run(daemon, origin, slot))
    ));
    let first = futures::poll!(parts.as_mut());
    drop(turn);
    let (outcome, ()) = futures::join!(
        async {
            match first {
                std::task::Poll::Ready(outcome) => outcome,
                std::task::Poll::Pending => parts.await,
            }
        },
        scenes.flush()
    );
    scenes.changed(daemon);
    Response::Scene {
        name: scene.name,
        outcome,
    }
}

/// The slot for a device's part: its plan when the device is connected, else the part left as it
/// is.
fn device_slot(config: &Config, devices: &[Arc<DeviceHandle>], part: &DevicePart) -> Slot {
    let device = devices
        .iter()
        .find(|device| device.udid.eq_ignore_ascii_case(&part.udid));
    let Some(device) = device.filter(|device| usable(device)) else {
        let who = config
            .devices
            .iter()
            .find(|device| device.udid.eq_ignore_ascii_case(&part.udid))
            .map_or_else(|| part.name.clone(), |device| device.name.clone());
        return Slot::Left(PartOutcome {
            notes: vec![words::not_connected(&who)],
            who,
            status: PartStatus::Left,
        });
    };
    let feed = device.feed.borrow().clone();
    let profile = feed.profile.clone().expect("a usable device has a profile");
    let plan = scene::plan(&profile, part, |id| raw_of(&feed.readings, id));
    Slot::Device(Box::new(Work {
        who: device.name.clone(),
        device: Arc::clone(device),
        profile,
        readings: feed.readings,
        part: part.clone(),
        plan,
    }))
}

/// The preset the scene would play on the computer and on a device at once, and the devices that
/// would play it, judged on the state the recall leaves.
///
/// The computer plays the part's EQ when the scene has one (on, never `custom`), else what it
/// plays now (`view`). A device plays its part's tone when it can play it; otherwise it keeps
/// what it plays now: a device the scene has no part for, a part with no tone, or a tone it
/// cannot play. A part that sets raw bands leaves a state this does not judge. The preset asks
/// when it is the same non-neutral one on both sides.
async fn both_sides(
    config: &Config,
    computer: Option<&ComputerPart>,
    view: &svolder_host::eq::EqView,
    devices: &[Arc<DeviceHandle>],
    slots: &[Slot],
) -> Option<(String, Vec<String>)> {
    let named = match computer {
        Some(part) => part.eq_on.then(|| part.name.clone()),
        None => view.on.then(|| view.preset.clone()).flatten(),
    }
    .filter(|name| !same_name(name, preset::CUSTOM))?;
    let mut held = Vec::new();
    let mut name = None;
    let mut ask = |preset: &str, who: &str| {
        name.get_or_insert_with(|| preset.to_string());
        held.push(who.to_string());
    };
    let mut seen: Vec<&Arc<DeviceHandle>> = Vec::new();
    for slot in slots {
        let Slot::Device(work) = slot else {
            continue;
        };
        seen.push(&work.device);
        let Some(eq) = work.profile.eq.as_ref() else {
            continue;
        };
        let plays_tone =
            work.part.tone.as_ref().filter(|tone| {
                preset::plan_recall(eq, |id| raw_of(&work.readings, id), tone).is_ok()
            });
        match plays_tone {
            Some(tone) => {
                if !same_name(&tone.name, preset::CUSTOM)
                    && same_name(&tone.name, &named)
                    && matches!(
                        preset::both_sides(tone, Some(&named)),
                        BothSides::Ask { .. }
                    )
                {
                    ask(&tone.name, &work.who);
                }
            }
            None if work
                .part
                .raw_bands
                .as_ref()
                .is_some_and(|bands| !bands.is_empty()) => {}
            None => {
                if let Some(found) = keeps(config, &work.profile, &work.readings, &named).await {
                    ask(&found, &work.who);
                }
            }
        }
    }
    for device in devices.iter().filter(|device| usable(device)) {
        if seen.iter().any(|done| Arc::ptr_eq(done, device)) {
            continue;
        }
        let feed = device.feed.borrow().clone();
        let Some(profile) = feed.profile.clone() else {
            continue;
        };
        if let Some(found) = keeps(config, &profile, &feed.readings, &named).await {
            ask(&found, &device.name);
        }
    }
    name.map(|name| (name, held))
}

/// The preset called `named` as a device plays it now when it is the same non-neutral preset
/// the computer is left on, under the name the device's own list gives it.
async fn keeps(
    config: &Config,
    profile: &Profile,
    readings: &[Message],
    named: &str,
) -> Option<String> {
    let eq = profile.eq.as_ref()?;
    let fitting = config.fitting_presets(profile);
    let (found, _) = fitting
        .iter()
        .find(|(preset, _)| same_name(&preset.name, named))?;
    crate::recall::warm(eq, std::slice::from_ref(found)).await;
    let plays = preset::matches(eq, |id| raw_of(readings, id), found);
    let asks = matches!(
        preset::both_sides(found, Some(named)),
        BothSides::Ask { .. }
    );
    (plays && asks).then(|| found.name.clone())
}

/// Carry one part out and say how it fared.
async fn run(daemon: &Daemon, origin: Origin, slot: Slot) -> PartOutcome {
    match slot {
        Slot::Left(outcome) => outcome,
        Slot::Computer(part) => computer(daemon, *part).await,
        Slot::Device(work) => device(origin, *work).await,
    }
}

/// The notes of a recall that mean the part was set whole.
fn is_information(note: &str) -> bool {
    note == LEVEL_HELD_NOTE || note.starts_with("the computer EQ chain was rebuilt")
}

/// What a refused computer part counts as: left as it is when nothing of it changed, failed when
/// the recall began to change things and the computer is not as it was.
fn refused_status(changed: bool) -> PartStatus {
    match changed {
        true => PartStatus::Failed,
        false => PartStatus::Left,
    }
}

/// The computer's part, as one change. A refusal before anything changed leaves the part as it
/// is; one that came after the controller began to change things and could not put it all back is
/// a part that failed. The notes quote the part's name as `Custom` where it is `custom`.
async fn computer(daemon: &Daemon, part: ComputerPart) -> PartOutcome {
    let who = "computer".to_string();
    let named_custom = same_name(&part.name, preset::CUSTOM);
    let cased = |text: &str| match named_custom {
        true => custom_cased(text),
        false => text.to_string(),
    };
    match daemon.host().scene_recall(part).await {
        Ok(recalled) => {
            let notes: Vec<String> = recalled.notes.iter().map(|n| cased(n)).collect();
            let status = match recalled.notes.iter().all(|note| is_information(note)) {
                true => PartStatus::Done,
                false => PartStatus::Partial,
            };
            PartOutcome { who, status, notes }
        }
        Err(refusal) => PartOutcome {
            who,
            status: refused_status(refusal.changed),
            notes: vec![cased(&words::computer_away(&refusal.words))],
        },
    }
}

/// The words of a refusal answer.
fn said(response: &Response) -> String {
    match response {
        Response::Error { message, .. } => message.clone(),
        other => format!("{other:?}"),
    }
}

/// How long one device's part has in all, for every wait it makes: the daemon's worst case for an
/// answer ([`ACTOR_DEADLINE`] twice) less a margin, so the answer is always in time for the front
/// ends' wait.
const PART_DEADLINE: Duration = ACTOR_DEADLINE
    .saturating_mul(2)
    .saturating_sub(Duration::from_secs(5));

/// The words of a part that ran out of its [`PART_DEADLINE`].
fn part_silence(who: &str) -> String {
    match actor_silent(who, PART_DEADLINE) {
        Response::Error { message, .. } => message,
        other => format!("{other:?}"),
    }
}

/// Wait for a batch the writer plans and ends, both before `end`; the clamp words of its plan on
/// success, why it did not go through otherwise.
async fn batch(
    who: &str,
    recalling: Recalling,
    what: &str,
    end: Instant,
) -> Result<Vec<String>, String> {
    let Recalling { planned, done } = recalling;
    let clamps = match tokio::time::timeout_at(end, planned).await {
        Ok(Ok(Planned::Plan(recalled))) => recalled
            .recall
            .plan
            .clamped
            .iter()
            .map(ToString::to_string)
            .collect(),
        Ok(Ok(Planned::Setting(_))) => Vec::new(),
        Ok(Ok(Planned::Refused(why))) => return Err(why),
        Ok(Err(_)) => {
            return Err(format!(
                "a later {what} on {who} took this one's place before it started"
            ));
        }
        Err(_) => return Err(part_silence(who)),
    };
    match tokio::time::timeout_at(end, done).await {
        Ok(Ok(Outcome::Done(response @ Response::Error { .. }))) => Err(said(&response)),
        Ok(Ok(Outcome::Done(_))) => Ok(clamps),
        Ok(Ok(Outcome::Superseded)) => {
            Err(format!("a later {what} on {who} took this one's place"))
        }
        Ok(Ok(Outcome::Cancelled(why))) => Err(why.to_string()),
        Ok(Err(_)) => Err(said(&actor_gone(who))),
        Err(_) => Err(part_silence(who)),
    }
}

/// A device's part: its tone, its built-in bands and each held control, handed to its writer
/// whole and together. The writer re-plans the tone and the bands when their batch starts, and
/// decides each control as it takes the ask in arrival order, so nothing the part holds is
/// dropped because the device read it already when the recall was planned.
async fn device(origin: Origin, work: Work) -> PartOutcome {
    let Work {
        who,
        device,
        profile,
        readings,
        part,
        plan,
    } = work;
    let end = Instant::now() + PART_DEADLINE;
    let writer = &device.writer;
    let skipped = |key: &str| {
        let lead = format!("{key}:");
        plan.skipped.iter().any(|skip| {
            skip.get(..lead.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(&lead))
        })
    };
    let mut notes: Vec<String> = plan.skipped.clone();
    // The tone, whenever the device can play it; a curve it cannot is already in the notes.
    let tone =
        part.tone
            .clone()
            .filter(|tone| {
                profile.eq.as_ref().is_some_and(|eq| {
                    preset::plan_recall(eq, |id| raw_of(&readings, id), tone).is_ok()
                })
            })
            .map(|tone| writer.recall(origin, tone.name.clone(), vec![tone]));
    // The built-in bands the plan did not skip, all of them: the writer plans which still change.
    let values: Vec<(String, i64)> = part
        .raw_bands
        .iter()
        .flatten()
        .filter(|(key, _)| !skipped(key))
        .cloned()
        .collect();
    let bands =
        (!values.is_empty()).then(|| writer.apply_values(origin, "scene".to_string(), values));
    // Every held control the plan did not skip.
    let held: Vec<String> = scene::held_controls(&profile)
        .iter()
        .map(|control| control.key.clone())
        .collect();
    let sets: Vec<_> = part
        .controls
        .iter()
        .filter(|(key, _)| !skipped(key))
        // Whatever a hand-edited part names, only the controls a scene holds are ever written:
        // the filter is on `held_controls`, not on how the plan words what it left out.
        .filter(|(key, _)| held.iter().any(|own| own.eq_ignore_ascii_case(key)))
        .map(|(key, raw)| (key, writer.set_if_needed(origin, key.clone(), *raw)))
        .collect();
    let mut failed = false;
    if let Some(tone) = tone {
        match batch(&who, tone, "preset recall", end).await {
            Ok(clamps) => notes.extend(clamps),
            Err(why) => {
                failed = true;
                notes.push(why);
            }
        }
    }
    if let Some(bands) = bands
        && let Err(why) = batch(&who, bands, "setting", end).await
    {
        failed = true;
        notes.push(why);
    }
    for (key, answer) in sets {
        let why = match tokio::time::timeout_at(end, answer).await {
            Ok(Ok(Outcome::Done(response @ Response::Error { .. }))) => Some(said(&response)),
            Ok(Ok(Outcome::Done(_))) => None,
            Ok(Ok(Outcome::Superseded)) => Some(format!("a newer write took {key}'s place")),
            Ok(Ok(Outcome::Cancelled(why))) => Some(why.to_string()),
            Ok(Err(_)) => Some(said(&actor_gone(&who))),
            Err(_) => Some(part_silence(&who)),
        };
        if let Some(why) = why {
            failed = true;
            notes.push(format!("{key}: {why}"));
        }
    }
    let status = match (failed, notes.is_empty()) {
        (true, _) => PartStatus::Failed,
        (false, true) => PartStatus::Done,
        (false, false) => PartStatus::Partial,
    };
    PartOutcome { who, status, notes }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A computer part that was changed and not put back is failed; one refused whole is left.
    #[test]
    fn a_refused_computer_part_is_failed_only_when_it_changed() {
        assert_eq!(refused_status(true), PartStatus::Failed);
        assert_eq!(refused_status(false), PartStatus::Left);
    }
}
