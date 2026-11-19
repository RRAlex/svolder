// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's EQ, crossfeed, virtual speakers and loudness on the bridge: following the
//! host's view, announcing the EQ's switch and preset sensor, the crossfeed's switch and preset
//! select, the speakers' switch and set select and the loudness's switch and strength number,
//! publishing their states (and the chosen set's attributes), and turning the EQ, the crossfeed,
//! the speakers or the loudness on or off, or choosing a crossfeed preset, a fetched set or a
//! loudness strength, when one is commanded. The crossfeed's, the speakers' and the loudness's
//! entities are announced only while the view carries them.
//!
//! Two read-only entities say what the path to the DAC does to the sound: the path sensor and the
//! PipeWire config's problem sensor ([`computer::path`]). They are judged here, from the view's
//! route and the DACs the bridge publishes, by the IPC crate's judges; a route read needs a hold
//! on the daemon's route reads, which the bridge keeps (slowly) while it is connected and the
//! entities are published.
//!
//! Nothing is announced while the host reports no EQ at all (host audio off): what an earlier
//! announcement left is cleared, once the host has had [`START_SETTLES`] from the start to report
//! one. While it reports the computer's controls unavailable the
//! entities stay and the computer's availability says `offline`. Home Assistant is only ever
//! told what the view says: an EQ whose chain the daemon could not verify reads as the saved
//! state says.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use svolder_host::RouteFacts;
use svolder_ipc::host::{
    DacFacts, DacStage, HostView, ROUTE_NOT_READ, SETUP_LIMIT, SetupSummary, path_line,
    setup_check, sink_is_dac,
};
use svolder_mqtt::computer::path::{self, ConfigLine, ConfigState, PathState};
use svolder_mqtt::{computer, payload, topics};
use svolder_profiles::Family;
use svolder_proto::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::{BridgeTask, ComputerLink, PublishedDevice, WriteOutcome};
use crate::host::{EqSwitch, SlowHolds, Watcher};
use crate::log;

/// How long a start waits for the host to report the computer's EQ before it takes down what an
/// earlier run left: the host's first view comes a moment after the bridge's, and clearing an
/// entity that is about to be announced makes Home Assistant remove and add it again.
pub(super) const START_SETTLES: Duration = Duration::from_secs(3);

/// What the bridge keeps of the computer's EQ.
pub(super) struct Published {
    pub(super) view: watch::Receiver<HostView>,
    pub(super) switch: Arc<dyn EqSwitch>,
    /// Tells the task each time the view changes.
    watcher: JoinHandle<()>,
    /// Whether the view changed since it was last published.
    pub(super) dirty: bool,
    /// The discovery configs announced, by topic.
    pub(super) configs: BTreeMap<String, String>,
    /// The availability last published: `true` for `online`.
    pub(super) available: Option<bool>,
    /// The switch's state and the preset sensor's last published.
    pub(super) state: Option<(&'static str, String)>,
    /// The crossfeed switch's state and preset's last published.
    pub(super) crossfeed_state: Option<(&'static str, String)>,
    /// The loudness switch's state and strength's last published.
    pub(super) loudness_state: Option<(&'static str, String)>,
    /// The speakers' switch, chosen set and attributes last published.
    pub(super) speakers_state: Option<computer::speakers::State>,
    /// The path sensor's state and attributes last published.
    pub(super) path_state: Option<PathState>,
    /// The config sensor's state and attributes last published.
    pub(super) config_state: Option<ConfigState>,
    /// Makes the slow hold on the route reads; `None` where nothing reads a route.
    slow: Option<SlowHolds>,
    /// The hold on the route reads, while the broker is connected and the entities published.
    hold: Option<Watcher>,
    /// Whether the broker is connected.
    connected: bool,
    /// Whether what an earlier run left retained has been cleared.
    pub(super) swept: bool,
    /// When the bridge began following the computer.
    since: Instant,
}

impl Drop for Published {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

impl Published {
    /// Hold the route reads while the broker is connected and the path and config states are
    /// published (whether or not discovery announces them), and let go otherwise.
    fn keep_hold(&mut self) {
        let wanted = self.connected && self.path_state.is_some();
        match (wanted, self.hold.is_some()) {
            (true, false) => self.hold = self.slow.as_ref().map(SlowHolds::hold),
            (false, true) => self.hold = None,
            _ => {}
        }
    }
}

impl BridgeTask {
    /// Follow the computer's EQ through `link`, from the first publish on, and take its commands
    /// when commands are on.
    pub(super) fn follow_computer(&mut self, link: ComputerLink) {
        let watcher = tokio::spawn(watch_view(link.view.clone(), self.changes_tx.clone()));
        if self.config.commands {
            self.client.subscribe(topics::command(
                &self.config.base_topic,
                computer::UNIT,
                "+",
            ));
        }
        self.computer = Some(Published {
            view: link.view,
            switch: link.switch,
            watcher,
            dirty: true,
            configs: BTreeMap::new(),
            available: None,
            state: None,
            crossfeed_state: None,
            loudness_state: None,
            speakers_state: None,
            path_state: None,
            config_state: None,
            slow: link.slow,
            hold: None,
            connected: false,
            swept: false,
            since: Instant::now(),
        });
        self.changed(computer::UNIT);
    }

    /// Publish what changed for the computer: the entities, their availability and their states.
    pub(super) fn publish_computer(&mut self) {
        let base = self.config.base_topic.clone();
        let prefix = self.config.discovery_prefix.clone();
        let commands = self.config.commands;
        let client = &self.client;
        let Some(published) = self.computer.as_mut().filter(|c| c.dirty) else {
            return;
        };
        published.dirty = false;
        let view = published.view.borrow().clone();
        let eq = view.eq.as_ref().filter(|_| view.available);
        let availability = topics::device_availability(&base, computer::UNIT);
        let state_topic = topics::diagnostic(&base, computer::UNIT, computer::EQ_KEY);
        let preset_topic = topics::diagnostic(&base, computer::UNIT, computer::PRESET_KEY);
        let xf_topic = topics::diagnostic(&base, computer::UNIT, computer::crossfeed::SWITCH_KEY);
        let xf_preset_topic =
            topics::diagnostic(&base, computer::UNIT, computer::crossfeed::PRESET_KEY);
        let ld_topic = topics::diagnostic(&base, computer::UNIT, computer::loudness::SWITCH_KEY);
        let ld_strength_topic =
            topics::diagnostic(&base, computer::UNIT, computer::loudness::STRENGTH_KEY);
        let sp_topic = topics::diagnostic(&base, computer::UNIT, computer::speakers::SWITCH_KEY);
        let sp_set_topic = topics::diagnostic(&base, computer::UNIT, computer::speakers::SET_KEY);
        let sp_attrs_topic = computer::speakers::attributes_topic(&base);
        let path_topic = topics::diagnostic(&base, computer::UNIT, path::PATH_KEY);
        let path_attrs_topic = path::path_attributes_topic(&base);
        let config_topic = topics::diagnostic(&base, computer::UNIT, path::CONFIG_KEY);
        let config_attrs_topic = path::config_attributes_topic(&base);
        let dacs = dacs_of(&self.devices);

        // The first publish takes down what an earlier run left retained, whichever entities it
        // announced and whether or not this run serves the computer. While the host has not yet
        // reported an EQ it waits, so a computer about to be announced is never cleared first;
        // a host that reports none within [`START_SETTLES`] is swept then.
        if !published.swept && eq.is_none() {
            let due = published.since + START_SETTLES;
            if Instant::now() < due {
                published.dirty = true;
                // `flush_at` is an absolute time, so the earlier of two owed flushes is kept.
                self.flush_at = Some(self.flush_at.map_or(due, |at| at.min(due)));
                return;
            }
        }
        if !published.swept {
            published.swept = true;
            let wanted: Vec<String> = match eq {
                Some(eq) => computer::entries_of(&base, &prefix, commands, eq)
                    .into_iter()
                    .map(|entry| entry.topic)
                    .collect(),
                None => Vec::new(),
            };
            for with_commands in [true, false] {
                for topic in computer::topics_of(&prefix, with_commands) {
                    if !wanted.contains(&topic) {
                        client.clear(topic);
                    }
                }
            }
            if eq.is_none() {
                client.clear(availability.clone());
                client.clear(state_topic.clone());
                client.clear(preset_topic.clone());
                client.clear(xf_topic.clone());
                client.clear(xf_preset_topic.clone());
                client.clear(ld_topic.clone());
                client.clear(ld_strength_topic.clone());
                client.clear(sp_topic.clone());
                client.clear(sp_set_topic.clone());
                client.clear(sp_attrs_topic.clone());
                client.clear(path_topic.clone());
                client.clear(path_attrs_topic.clone());
                client.clear(config_topic.clone());
                client.clear(config_attrs_topic.clone());
            }
        }

        let Some(eq) = eq else {
            // No computer EQ: take down what was announced.
            for topic in published.configs.keys() {
                client.clear(topic.clone());
            }
            if published.available.is_some() || !published.configs.is_empty() {
                client.clear(availability);
                client.clear(state_topic);
                client.clear(preset_topic);
                client.clear(xf_topic);
                client.clear(xf_preset_topic);
                client.clear(ld_topic);
                client.clear(ld_strength_topic);
                client.clear(sp_topic);
                client.clear(sp_set_topic);
                client.clear(sp_attrs_topic);
                client.clear(path_topic);
                client.clear(path_attrs_topic);
                client.clear(config_topic);
                client.clear(config_attrs_topic);
            }
            published.configs.clear();
            published.available = None;
            published.state = None;
            published.crossfeed_state = None;
            published.loudness_state = None;
            published.speakers_state = None;
            published.path_state = None;
            published.config_state = None;
            published.keep_hold();
            return;
        };

        let next: BTreeMap<String, String> = computer::entries_of(&base, &prefix, commands, eq)
            .into_iter()
            .map(|entry| (entry.topic, entry.payload.to_string()))
            .collect();
        for (topic, body) in &next {
            if published.configs.get(topic) != Some(body) {
                client.announce(topic.clone(), body.clone());
            }
        }
        for topic in published.configs.keys().filter(|t| !next.contains_key(*t)) {
            client.clear(topic.clone());
        }
        published.configs = next;

        let usable = eq.unavailable.is_none();
        if published.available != Some(usable) {
            let word = match usable {
                true => payload::ONLINE,
                false => payload::OFFLINE,
            };
            client.retain(availability, word);
            published.available = Some(usable);
        }
        if !usable {
            return;
        }
        let (on, preset) = computer::state(Some(eq));
        if published.state.as_ref() != Some(&(on, preset.clone())) {
            let kept_preset = published.state.as_ref().map(|(_, p)| p.clone());
            client.retain(state_topic, on);
            if preset.is_empty() {
                client.clear(preset_topic);
            } else if kept_preset.as_deref() != Some(preset.as_str()) {
                client.retain(preset_topic, preset.clone());
            }
            published.state = Some((on, preset));
        }
        match eq.crossfeed.as_ref() {
            Some(crossfeed) => {
                let (on, preset) = computer::crossfeed::state(crossfeed);
                let was = published.crossfeed_state.clone();
                if was.as_ref().map(|s| s.0) != Some(on) {
                    client.retain(xf_topic, on);
                }
                if was.as_ref().map(|s| s.1.as_str()) != Some(preset.as_str()) {
                    client.retain(xf_preset_topic, preset.clone());
                }
                published.crossfeed_state = Some((on, preset));
            }
            None if published.crossfeed_state.is_some() => {
                client.clear(xf_topic);
                client.clear(xf_preset_topic);
                published.crossfeed_state = None;
            }
            None => {}
        }
        match eq.loudness.as_ref() {
            Some(loudness) => {
                let (on, strength) = computer::loudness::state(loudness);
                let was = published.loudness_state.clone();
                if was.as_ref().map(|s| s.0) != Some(on) {
                    client.retain(ld_topic, on);
                }
                if was.as_ref().map(|s| s.1.as_str()) != Some(strength.as_str()) {
                    client.retain(ld_strength_topic, strength.clone());
                }
                published.loudness_state = Some((on, strength));
            }
            None if published.loudness_state.is_some() => {
                client.clear(ld_topic);
                client.clear(ld_strength_topic);
                published.loudness_state = None;
            }
            None => {}
        }
        match eq.speakers.as_ref() {
            Some(speakers) => {
                let now = computer::speakers::state(speakers);
                let was = published.speakers_state.clone();
                if was.as_ref().map(|s| s.on) != Some(now.on) {
                    client.retain(sp_topic, now.on);
                }
                if was.as_ref().map(|s| &s.set) != Some(&now.set) {
                    client.retain(sp_set_topic, now.set_payload());
                }
                if was.as_ref().map(|s| &s.attributes) != Some(&now.attributes) {
                    client.retain(sp_attrs_topic, now.attributes_payload());
                }
                published.speakers_state = Some(now);
            }
            None if published.speakers_state.is_some() => {
                client.clear(sp_topic);
                client.clear(sp_set_topic);
                client.clear(sp_attrs_topic);
                published.speakers_state = None;
            }
            None => {}
        }
        let (now_path, now_config) = judge(view.route.as_ref(), &dacs);
        let was = published.path_state.as_ref();
        if was.map(|p| p.state) != Some(now_path.state) {
            client.retain(path_topic, now_path.state);
        }
        if was.map(|p| &p.attributes) != Some(&now_path.attributes) {
            client.retain(path_attrs_topic, now_path.attributes.clone());
        }
        published.path_state = Some(now_path);
        let was = published.config_state.as_ref();
        if was.map(|c| c.state) != Some(now_config.state) {
            client.retain(config_topic, now_config.payload());
        }
        if was.map(|c| &c.attributes) != Some(&now_config.attributes) {
            client.retain(config_attrs_topic, now_config.attributes.clone());
        }
        published.config_state = Some(now_config);
        published.keep_hold();
    }

    /// A command on one of the computer's topics: the EQ, crossfeed, speakers and loudness
    /// switches take `ON` or `OFF`, the crossfeed preset select takes one of its options, the
    /// loudness strength takes a number in percent. EQ ON with nothing kept to apply leaves the
    /// EQ off. `Custom` is no command.
    pub(super) fn computer_command(&mut self, key: &str, body: &[u8]) {
        let drop = |why: &str| {
            log::warn(&format!(
                "mqtt: This computer: a command for {key} dropped: {why}"
            ))
        };
        let Some(published) = self.computer.as_ref() else {
            return;
        };
        let eq_key = key.eq_ignore_ascii_case(computer::EQ_KEY);
        let xf_key = key.eq_ignore_ascii_case(computer::crossfeed::SWITCH_KEY);
        let preset_key = key.eq_ignore_ascii_case(computer::crossfeed::PRESET_KEY);
        let sp_key = key.eq_ignore_ascii_case(computer::speakers::SWITCH_KEY);
        let set_key = key.eq_ignore_ascii_case(computer::speakers::SET_KEY);
        let ld_key = key.eq_ignore_ascii_case(computer::loudness::SWITCH_KEY);
        let strength_key = key.eq_ignore_ascii_case(computer::loudness::STRENGTH_KEY);
        if !(eq_key || xf_key || preset_key || sp_key || set_key || ld_key || strength_key) {
            return drop(
                "nothing of the computer but its EQ, crossfeed, virtual speakers and loudness \
                 takes commands",
            );
        }
        let Ok(text) = std::str::from_utf8(body) else {
            return drop("the payload is not text");
        };
        let on = match preset_key || set_key || strength_key {
            true => None,
            false => match computer::parse(text) {
                Some(on) => Some(on),
                None => return drop(&format!("{:?} is not ON or OFF", text.trim())),
            },
        };
        let view = published.view.borrow().clone();
        let Some(eq) = view.eq.as_ref().filter(|_| view.available) else {
            return drop("computer controls unavailable: host audio is off");
        };
        if let Some(why) = &eq.unavailable {
            return drop(why);
        }
        let switch = Arc::clone(&published.switch);
        if eq_key {
            let on = on.unwrap_or_default();
            if on && !eq.on && computer::nothing_kept(eq) {
                log::info(
                    "mqtt: This computer: ON left the EQ off: no EQ points are saved yet; set the \
                     EQ up first",
                );
                return;
            }
            let what = format!("turning the EQ {}", if on { "on" } else { "off" });
            self.writes
                .spawn(async move { WriteOutcome::Computer(what, switch.set_on(on).await) });
            return;
        }
        if ld_key || strength_key {
            if eq.loudness.is_none() {
                return drop("this computer serves no loudness");
            }
            // While loudness cannot follow the volume, a change is still taken and saved; it
            // acts once loudness can follow again.
            let (what, on, strength) = match on {
                Some(on) => (
                    format!("turning loudness {}", if on { "on" } else { "off" }),
                    Some(on),
                    None,
                ),
                None => match computer::loudness::parse_strength(text) {
                    None => {
                        return drop(&format!("{:?} is not a strength in percent", text.trim()));
                    }
                    Some(strength) => (
                        format!("setting the loudness strength to {}", text.trim()),
                        None,
                        Some(strength),
                    ),
                },
            };
            self.writes.spawn(async move {
                WriteOutcome::Computer(what, switch.loudness(on, strength).await)
            });
            return;
        }
        if sp_key || set_key {
            let Some(speakers) = eq.speakers.as_ref() else {
                return drop("this computer serves no virtual speakers");
            };
            if let Some(why) = &speakers.unavailable {
                return drop(why);
            }
            let (what, set) = match on {
                Some(true) if computer::speakers::state(speakers).set.is_empty() => {
                    log::info(
                        "mqtt: This computer: ON left the virtual speakers off: no fetched set \
                         is chosen; choose one first",
                    );
                    return;
                }
                Some(on) => (
                    format!(
                        "turning the virtual speakers {}",
                        if on { "on" } else { "off" }
                    ),
                    None,
                ),
                None => match computer::speakers::parse_set(speakers, text) {
                    None => return drop(&format!("{:?} is not a fetched set", text.trim())),
                    Some(id) => (
                        format!("choosing the virtual speakers set {:?}", text.trim()),
                        Some(id),
                    ),
                },
            };
            self.writes
                .spawn(async move { WriteOutcome::Computer(what, switch.speakers(on, set).await) });
            return;
        }
        let Some(crossfeed) = eq.crossfeed.as_ref() else {
            return drop("this computer serves no crossfeed");
        };
        if let Some(why) = &crossfeed.unavailable {
            return drop(why);
        }
        let (what, on, choice) = match on {
            Some(on) => (
                format!("turning crossfeed {}", if on { "on" } else { "off" }),
                Some(on),
                None,
            ),
            None => match computer::crossfeed::parse_preset(crossfeed, text) {
                None => return drop(&format!("{:?} is not a crossfeed preset", text.trim())),
                Some(computer::crossfeed::Choice::Custom) => {
                    log::info(
                        "mqtt: This computer: Custom is a crossfeed state, not a choice; the \
                         settings stay as they are",
                    );
                    return;
                }
                Some(computer::crossfeed::Choice::Preset { model, preset }) => (
                    format!("choosing the crossfeed preset {:?}", text.trim()),
                    None,
                    Some((model, preset)),
                ),
            },
        };
        self.writes
            .spawn(async move { WriteOutcome::Computer(what, switch.crossfeed(on, choice).await) });
    }

    /// The end of a command to the computer: say so when it was refused.
    pub(super) fn computer_written(&mut self, what: &str, result: Result<(), String>) {
        if let Err(why) = result {
            log::warn(&format!("mqtt: This computer: {what} failed: {why}"));
        }
        // The state is whatever the view says now.
        self.changed(computer::UNIT);
    }

    /// The broker was reached or lost: the route reads are held only while it is connected.
    pub(super) fn computer_link(&mut self, connected: bool) {
        let Some(computer) = self.computer.as_mut() else {
            return;
        };
        computer.connected = connected;
        if connected {
            self.changed(computer::UNIT);
        } else {
            computer.keep_hold();
        }
    }

    /// Say `offline` for the computer on the way out, when it was announced, and let go of the
    /// route reads.
    pub(super) fn computer_farewell(&mut self) {
        if let Some(computer) = self.computer.as_mut() {
            computer.hold = None;
        }
        if self
            .computer
            .as_ref()
            .is_some_and(|c| c.available.is_some())
        {
            self.client.retain(
                topics::device_availability(&self.config.base_topic, computer::UNIT),
                payload::OFFLINE,
            );
        }
    }
}

/// Tell the bridge each time the view changes, until the view or the bridge goes.
async fn watch_view(mut view: watch::Receiver<HostView>, changes: mpsc::UnboundedSender<String>) {
    while view.changed().await.is_ok() {
        if changes.send(computer::UNIT.to_string()).is_err() {
            return;
        }
    }
}

/// The DACs the bridge publishes, as the page reads them: what each one reports of its end of the
/// path. Its own stage is not claimed while its link is down: the readings held then are from
/// before the drop.
fn dacs_of(devices: &BTreeMap<String, PublishedDevice>) -> Vec<DacFacts> {
    devices
        .values()
        .filter_map(|published| {
            let feed = published.feed.borrow();
            let profile = feed.profile.as_ref().filter(|p| p.family == Family::Dac)?;
            let raw = |id| {
                let value = &feed.readings.iter().find(|m| m.id == id)?.value;
                value.as_i64().or_else(|| match value {
                    Value::Bool(on) => Some(i64::from(*on)),
                    _ => None,
                })
            };
            let mut facts = DacFacts::read(profile, raw);
            if !feed.connected {
                facts.stage = DacStage::default();
            }
            Some(facts)
        })
        .collect()
}

/// What the path sensor and the config sensor say for `route` against the DAC that is its sink:
/// the path judged by the IPC crate's `path_line` (a sink that is no configured DAC reads
/// `not_this_dac` and is named), the config by its `setup_check`, and only while the sink is a
/// DAC. A config that was not fully read says nothing, never `OFF`.
fn judge(route: Option<&RouteFacts>, dacs: &[DacFacts]) -> (PathState, ConfigState) {
    let Some(route) = route else {
        return (
            path::path_state(path::NOT_READ, &[ROUTE_NOT_READ.to_string()], None, None),
            path::config_unjudged(),
        );
    };
    let dac = route
        .sink
        .as_ref()
        .and_then(|sink| dacs.iter().find(|dac| sink_is_dac(sink, dac)));
    let line = path_line(route, dac.unwrap_or(&DacFacts::default()));
    let state = match (&route.sink, &line.verdict) {
        (None, _) => path::NOT_READ,
        (_, verdict) => verdict.key(),
    };
    let said = path::path_state(
        state,
        &line.words,
        line.sentence().as_deref(),
        dac.map(|dac| dac.display.as_str()),
    );
    if dac.is_none() {
        return (said, path::config_unjudged());
    }
    let check = setup_check(route);
    let problem = match check.summary {
        SetupSummary::Untouched => Some(false),
        SetupSummary::Changes(_) => Some(true),
        SetupSummary::Unread(_) | SetupSummary::NotRead | SetupSummary::OtherVersion => None,
    };
    let lines: Vec<ConfigLine> = check
        .lines
        .iter()
        .map(|line| ConfigLine {
            key: &line.key,
            outcome: line.outcome.key(),
            words: &line.words,
        })
        .collect();
    let config = path::config_state(
        problem,
        check.summary.key(),
        &check.summary_words(),
        &check.sink,
        SETUP_LIMIT,
        &lines,
    );
    (said, config)
}
