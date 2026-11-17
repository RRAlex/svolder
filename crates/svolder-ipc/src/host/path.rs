// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The path verdict: the hops from the player to the DAC, judged against what the device reports.

use svolder_host::{PlayerMatch, RouteFacts, SinkFacts, StreamFacts};
use svolder_profiles::{EqMode, Profile, roles, safety, units};
use svolder_proto::FunctionId;

/// The USB vendor id Schiit's devices carry.
pub const SCHIIT_VENDOR_ID: u16 = 0x30be;

/// What a front end says of the path while the route has not been read.
pub const ROUTE_NOT_READ: &str = "route not read";

/// The word for a path that nothing alters.
const UNTOUCHED: &str = "untouched";
/// What follows the words of an untouched path in its tooltip: the verdict cannot see inside
/// the player.
const UNTOUCHED_NOTE: &str = "the player may still change the sound itself (loudness \
                              normalization, its own EQ)";

/// What the device's own stage does to the sound it receives: its volume below 0 dB, its tone
/// control when it shapes the sound, its balance off centre.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DacStage {
    /// The device's volume in dB, from the profile's own table; negative infinity for silence.
    /// `None` when the device has no volume in dB or has not said.
    pub volume_db: Option<f64>,
    /// Whether the tone control is on EQ or loudness.
    pub tone: bool,
    /// Whether the balance is off its centre.
    pub balance: bool,
    /// Whether the device's mute is on.
    pub muted: bool,
    /// Whether its phase invert is on.
    pub phase: bool,
    /// Whether its NOS (non-oversampling) filter is on.
    pub nos: bool,
}

impl DacStage {
    /// The volume as a drop worth naming: below 0 dB by at least what one decimal shows.
    fn lowers(&self) -> Option<f64> {
        self.volume_db.filter(|db| *db < -0.05)
    }

    /// Whether the stage changes the sound at all.
    pub fn changes(&self) -> bool {
        self.lowers().is_some() || self.tone || self.balance || self.muted || self.phase || self.nos
    }

    /// The stage as short hop words: `volume -3.3 dB`, `tone control`, `balance off centre`.
    fn hop_words(&self) -> Vec<String> {
        let mut words = Vec::new();
        if let Some(db) = self.lowers() {
            words.push(match db.is_finite() {
                true => format!(
                    "volume {}",
                    units::amount_text(units::Amount::of(db), Some("dB"))
                ),
                false => "volume silent".to_string(),
            });
        }
        if self.tone {
            words.push("tone control".to_string());
        }
        if self.balance {
            words.push("balance off centre".to_string());
        }
        if self.muted {
            words.push("muted".to_string());
        }
        if self.phase {
            words.push("phase inverted".to_string());
        }
        if self.nos {
            words.push("NOS filter".to_string());
        }
        words
    }

    /// The stage as a clause that follows `the sound is untouched`: `the Bifrost 3 then lowers it
    /// by 3.3 dB, shapes it with its tone control and shifts the balance`. `None` while the
    /// stage changes nothing.
    fn clause(&self, name: &str) -> Option<String> {
        let mut doings = Vec::new();
        if let Some(db) = self.lowers() {
            doings.push(match db.is_finite() {
                true => format!("lowers it by {:.1} dB", -db),
                false => "silences it".to_string(),
            });
        }
        if self.tone {
            doings.push("shapes it with its tone control".to_string());
        }
        if self.balance {
            doings.push("shifts the balance".to_string());
        }
        if self.phase {
            doings.push("inverts the phase".to_string());
        }
        if self.nos {
            doings.push("uses its NOS filter".to_string());
        }
        let who = match name.is_empty() {
            true => "the DAC".to_string(),
            false => format!("the {name}"),
        };
        let mut clauses = Vec::new();
        if let Some(last) = doings.pop() {
            let doings = match doings.is_empty() {
                true => last,
                false => format!("{} and {last}", doings.join(", ")),
            };
            clauses.push(format!("{who} then {doings}"));
        }
        if self.muted {
            clauses.push(match clauses.is_empty() {
                true => format!("{who} is muted"),
                false => "it is muted".to_string(),
            });
        }
        (!clauses.is_empty()).then(|| clauses.join("; "))
    }
}

/// What the device itself reports of its end of the path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DacFacts {
    /// The device's display name, such as `Gungnir 2`.
    pub display: String,
    /// The input the device reads, as its label, when it says.
    pub input: Option<String>,
    /// The sample rate the device reads, when it says.
    pub rate_hz: Option<u32>,
    /// Whether the USB 1 and the USB 2 port each read connected, for a device whose profile has a
    /// reading for each; `None` for one with a single USB port or no such readings. A port whose
    /// reading has not been said is `None` inside.
    pub usb_ports: Option<[Option<bool>; 2]>,
    /// What the device's own volume, tone control and balance do to the sound.
    pub stage: DacStage,
}

/// Which USB port the computer feeds, judged from the device's own port readings.
enum UsbFeed {
    /// The input is the port the computer is on, or the device has no per-port readings to
    /// say otherwise.
    Computer,
    /// The input is one USB port and the computer is on the other.
    Other {
        /// The input's label, such as `USB 1`.
        input: String,
        /// The number of the port the computer is on.
        computer: usize,
    },
    /// The readings do not say which port the computer is on, in the words that name why.
    Unsure(&'static str),
}

impl DacFacts {
    /// What the device `profile` describes reports of its end of the path, as far as the
    /// profile names the readings and `raw` has them (the raw value last read of a function, a
    /// boolean as 0 or 1).
    /// A DAC whose profile has no `input` control (the Eitr 2, the Mesh Card) takes USB alone,
    /// so its input reads `USB`; one that has the control and has not reported it reads none.
    pub fn read(profile: &Profile, raw: impl Fn(FunctionId) -> Option<i64>) -> Self {
        let reading = |key: &str| {
            let control = profile.control_by_key(key)?;
            let raw = u8::try_from(raw(control.id)?).ok()?;
            control.value_name(raw).map(str::to_string)
        };
        let input = match profile.control_by_key("input") {
            Some(_) => reading("input"),
            None => Some("USB".to_string()),
        };
        let connected = |key: &str| {
            let control = profile.control_by_key(key)?;
            Some(raw(control.id).map(|raw| raw == 1))
        };
        let usb_ports = connected("usb1_connected").zip(connected("usb2_connected"));
        let volume_db = roles::volume(profile)
            .filter(|control| safety::in_db(control))
            .and_then(|control| {
                let amount = units::amount(control, raw(control.id)?)?;
                Some(amount.number().unwrap_or(f64::NEG_INFINITY))
            });
        let tone = profile.eq.as_ref().is_some_and(|eq| {
            raw(eq.mode_id())
                .and_then(|raw| u8::try_from(raw).ok())
                .and_then(EqMode::from_raw)
                .is_some_and(|mode| mode != EqMode::Flat)
        });
        let balance = profile.control_by_key("balance").is_some_and(|control| {
            let (Some(center), Some(raw)) = (control.center, raw(control.id)) else {
                return false;
            };
            raw != center
        });
        let toggle = |key: &str| {
            profile
                .control_by_key(key)
                .is_some_and(|control| raw(control.id) == Some(1))
        };
        DacFacts {
            display: profile.display.clone(),
            input,
            rate_hz: reading("sample_rate").as_deref().and_then(rate_hz),
            usb_ports: usb_ports.map(|(one, two)| [one, two]),
            stage: DacStage {
                volume_db,
                tone,
                balance,
                muted: toggle("mute"),
                phase: toggle("phase"),
                nos: toggle("nos"),
            },
        }
    }

    /// Which USB port the computer feeds. Where the profile has a reading per port, the input
    /// is the computer's only when its own port reads connected and the other does not.
    fn usb_feed(&self) -> UsbFeed {
        let (Some(input), Some(ports)) = (&self.input, self.usb_ports) else {
            return UsbFeed::Computer;
        };
        let Some(port) = usb_port_of(input) else {
            return UsbFeed::Computer;
        };
        match (ports[port - 1], ports[2 - port]) {
            (Some(true), Some(false)) => UsbFeed::Computer,
            (Some(false), Some(true)) => UsbFeed::Other {
                input: input.clone(),
                computer: 3 - port,
            },
            (Some(true), Some(true)) => UsbFeed::Unsure("both USB ports connected"),
            (Some(false), Some(false)) => UsbFeed::Unsure("no USB port reads connected"),
            _ => UsbFeed::Unsure("USB ports not read"),
        }
    }
}

/// What the path as a whole does to the signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every hop is known to leave the signal alone.
    Untouched,
    /// At least one hop changes the signal; the words name each.
    Changed,
    /// The sound does not go to this device: the computer, or the player, plays to `sink`.
    NotThisDac {
        /// What the other sink is called.
        sink: String,
        /// Whether it is the player, not the computer's default sink, that plays there.
        by_player: bool,
    },
    /// The DAC listens to an input the computer does not feed (Optical, Coaxial, AES, BNC), or
    /// to a USB port the computer is not on: the computer's stream says nothing of what it plays.
    OtherInput {
        /// The input's label, such as `Optical` or `USB 1`.
        input: String,
    },
    /// Nothing is known to change the signal, but something could not be read; the words
    /// name what.
    Unknown,
}

/// One hop of the path: the player, the computer, or the DAC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    /// Which hop: `player`, `computer` or `DAC`.
    pub label: String,
    /// What is known of the signal there: its rate, format and volume.
    pub detail: String,
    /// Whether this hop alters the signal.
    pub changes: bool,
    /// Whether something of this hop was not read; its words are in the path's `unknown` list.
    pub unknown: bool,
}

/// The path from the player to the DAC, judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathLine {
    /// The hops in order; empty when the verdict is [`Verdict::NotThisDac`] or
    /// [`Verdict::OtherInput`], or the sink is not known.
    pub hops: Vec<Hop>,
    /// What the path does to the signal.
    pub verdict: Verdict,
    /// What changes the signal, or what is not known, in hop order; `untouched` for an
    /// untouched path (with `player assumed` after it when the stream was only assumed); the
    /// short words (`→ SINK`, `player → SINK`) for a sound that goes elsewhere.
    pub words: Vec<String>,
    /// What the device's own stage does after the path (`the Bifrost 3 then lowers it by 3.3
    /// dB`); `None` while it changes nothing, and for a sound that does not reach the device.
    pub dac_stage: Option<String>,
}

impl PathLine {
    /// The words on one line, joined by ` · ` as both front ends show them.
    pub fn words_line(&self) -> String {
        self.words.join(" · ")
    }

    /// The whole sentence behind the short words, for a tooltip: for a sound that goes elsewhere
    /// (`the computer plays to DELL S2721QS`), and for an untouched path, which says where the
    /// verdict starts (the player's output) and that the player may still change the sound
    /// itself, with what the device's own stage then does when it changes the sound; for a
    /// changed path, the changes it names and that stage, when there is one. `None` for any other
    /// verdict, and for a changed path whose DAC stage changes nothing.
    pub fn sentence(&self) -> Option<String> {
        match &self.verdict {
            Verdict::NotThisDac { sink, by_player } => {
                let who = if *by_player { "player" } else { "computer" };
                Some(format!("the {who} plays to {sink}"))
            }
            Verdict::Untouched => {
                let extra = match self.words.get(1..) {
                    Some(rest) if !rest.is_empty() => format!(" ({})", rest.join(", ")),
                    _ => String::new(),
                };
                let stage = self
                    .dac_stage
                    .as_ref()
                    .map(|stage| format!("; {stage}"))
                    .unwrap_or_default();
                Some(format!(
                    "{UNTOUCHED} from the player's output on{extra}{stage}; {UNTOUCHED_NOTE}"
                ))
            }
            Verdict::Changed => self
                .dac_stage
                .as_ref()
                .map(|stage| format!("{}; {stage}", self.words.join(", "))),
            _ => None,
        }
    }
}

/// Whether `sink` is the device `dac` describes: a USB sink of Schiit's vendor id whose
/// product name is the device's display name, or holds it as a run of whole words not followed
/// by a model number (`Bifrost` is not `Bifrost 3`), ignoring case, punctuation and a leading
/// `Schiit `.
pub fn sink_is_dac(sink: &SinkFacts, dac: &DacFacts) -> bool {
    if sink.usb_vendor_id != Some(SCHIIT_VENDOR_ID) {
        return false;
    }
    let Some(product) = sink.usb_product.as_deref() else {
        return false;
    };
    let (product, display) = (words_of(product), words_of(&dac.display));
    if display.is_empty() || product.len() < display.len() {
        return false;
    }
    (0..=product.len() - display.len()).any(|start| {
        let end = start + display.len();
        product[start..end] == display[..]
            && product
                .get(end)
                .is_none_or(|next| !next.starts_with(char::is_numeric))
    })
}

/// A sample rate's label in hertz: `44.1 kHz` is 44100; a label that is no rate (`—`, which a
/// device that locks to nothing reads) is none.
pub fn rate_hz(label: &str) -> Option<u32> {
    let (number, unit) = label.split_once(' ')?;
    if unit.trim() != "kHz" {
        return None;
    }
    let khz: f64 = number.parse().ok()?;
    // Rounded to the hertz: 176.4 kHz is 176400 whatever its binary form.
    let hz = (khz * 1000.0).round();
    (hz > 0.0 && hz < f64::from(u32::MAX)).then_some(hz as u32)
}

/// The lower-case words of `name`, without a leading `schiit`.
fn words_of(name: &str) -> Vec<String> {
    let mut words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.first().is_some_and(|word| word == "schiit") {
        words.remove(0);
    }
    words
}

/// Whether the input labelled `label` is one the computer feeds: every DAC profile names its
/// USB inputs `USB`, `USB 1` and `USB 2`.
fn is_usb_input(label: &str) -> bool {
    label
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("usb"))
}

/// The number of the USB port the input labelled `label` names (`USB 1`, `USB 2`), when it
/// names one.
fn usb_port_of(label: &str) -> Option<usize> {
    let mut words = label.split_whitespace().skip(1);
    match (words.next()?, words.next()) {
        ("1", None) => Some(1),
        ("2", None) => Some(2),
        _ => None,
    }
}

/// The path `route` describes judged against what the device `dac` reports. A device that
/// listens to an input other than USB, or to a USB port the computer is not on, gets no
/// verdict, and one whose input is not read, or whose port is not told from the other, can at
/// best be unknown.
pub fn path_line(route: &RouteFacts, dac: &DacFacts) -> PathLine {
    let Some(sink) = &route.sink else {
        return PathLine {
            hops: Vec::new(),
            verdict: Verdict::Unknown,
            words: vec!["the computer's sink was not read".to_string()],
            dac_stage: None,
        };
    };
    if !sink_is_dac(sink, dac) {
        let name = sink_name(sink);
        return elsewhere(format!("→ {name}"), name, false);
    }
    if let PlayerMatch::Elsewhere { sink: other } = &route.player_match {
        return elsewhere(format!("player → {other}"), other.clone(), true);
    }
    if let Some(input) = dac.input.as_ref().filter(|input| !is_usb_input(input)) {
        return PathLine {
            hops: Vec::new(),
            verdict: Verdict::OtherInput {
                input: input.clone(),
            },
            words: vec![format!("the DAC listens to {input}")],
            dac_stage: None,
        };
    }
    if let UsbFeed::Other { input, computer } = dac.usb_feed() {
        return PathLine {
            hops: Vec::new(),
            verdict: Verdict::OtherInput {
                input: input.clone(),
            },
            words: vec![
                format!("the DAC listens to {input}"),
                format!("the computer is on USB {computer}"),
            ],
            dac_stage: None,
        };
    }

    let mut changes = Vec::new();
    let mut unknown = Vec::new();
    let player = player_hop(route, &mut changes, &mut unknown);
    let computer = computer_hop(route, sink, &mut changes, &mut unknown);
    let device = dac_hop(sink, dac, &mut changes, &mut unknown);

    let (verdict, words) = if !changes.is_empty() {
        (Verdict::Changed, changes)
    } else if !unknown.is_empty() {
        (Verdict::Unknown, unknown)
    } else if route.player_match == PlayerMatch::Assumed {
        (
            Verdict::Untouched,
            vec![UNTOUCHED.to_string(), "player assumed".to_string()],
        )
    } else {
        (Verdict::Untouched, vec![UNTOUCHED.to_string()])
    };
    PathLine {
        hops: vec![player, computer, device],
        verdict,
        words,
        dac_stage: dac.stage.clause(&dac.display),
    }
}

/// The line for a sound that goes to a sink other than the device.
fn elsewhere(words: String, sink: String, by_player: bool) -> PathLine {
    PathLine {
        hops: Vec::new(),
        verdict: Verdict::NotThisDac { sink, by_player },
        words: vec![words],
        dac_stage: None,
    }
}

/// What a sink is called for people: its description, else its node name.
fn sink_name(sink: &SinkFacts) -> String {
    if sink.description.is_empty() {
        sink.name.clone()
    } else {
        sink.description.clone()
    }
}

/// The player's stream, when the route took it as the player's own.
fn identified_stream(route: &RouteFacts) -> Option<&StreamFacts> {
    let identified = matches!(
        route.player_match,
        PlayerMatch::Pid | PlayerMatch::Name | PlayerMatch::Assumed
    );
    route.player.as_ref().filter(|_| identified)
}

/// The player's hop; what it changes and what it leaves unknown go to the two lists.
fn player_hop(route: &RouteFacts, changes: &mut Vec<String>, unknown: &mut Vec<String>) -> Hop {
    let Some(stream) = identified_stream(route) else {
        unknown.push(match route.player_match {
            PlayerMatch::NoPlayer => "no player".to_string(),
            _ => "player not identified".to_string(),
        });
        return Hop {
            label: "player".to_string(),
            detail: "not identified".to_string(),
            changes: false,
            unknown: true,
        };
    };
    let mut detail = signal_detail(
        stream.rate_hz,
        stream.format.as_deref(),
        stream.volume_milli,
    );
    let before = changes.len();
    let unknown_before = unknown.len();
    if stream.rate_hz.is_none() {
        unknown.push("player rate not read".to_string());
    }
    stream_volume(stream, changes, unknown);
    if stream.muted {
        detail.push("muted".to_string());
    }
    if route.player_match == PlayerMatch::Assumed {
        detail.push("assumed".to_string());
    }
    Hop {
        label: "player".to_string(),
        detail: detail.join(", "),
        changes: changes.len() > before,
        unknown: unknown.len() > unknown_before,
    }
}

/// Names a stream's volume below 100 % and its mute; a volume not read is unknown.
fn stream_volume(stream: &StreamFacts, changes: &mut Vec<String>, unknown: &mut Vec<String>) {
    match stream.volume_milli {
        Some(milli) if milli != 1000 => changes.push(format!("player volume {}", percent(milli))),
        Some(_) => {}
        None => unknown.push("player volume not read".to_string()),
    }
    if stream.muted {
        changes.push("player muted".to_string());
    }
}

/// The computer's hop: the sink's own rate and volume, the EQ and the other sounds.
fn computer_hop(
    route: &RouteFacts,
    sink: &SinkFacts,
    changes: &mut Vec<String>,
    unknown: &mut Vec<String>,
) -> Hop {
    let before = changes.len();
    let unknown_before = unknown.len();
    let detail = signal_detail(sink.rate_hz, sink.format.as_deref(), sink.volume_milli);
    match (
        identified_stream(route).and_then(|stream| stream.rate_hz),
        sink.rate_hz,
    ) {
        (Some(from), Some(to)) if from != to => {
            changes.push(format!("resampled {} → {} kHz", khz(from), khz(to)));
        }
        (_, None) => unknown.push("computer rate not read".to_string()),
        _ => {}
    }
    match sink.volume_milli {
        Some(milli) if milli != 1000 => changes.push(format!("computer volume {}", percent(milli))),
        Some(_) => {}
        None => unknown.push("computer volume not read".to_string()),
    }
    if sink.muted {
        changes.push("computer muted".to_string());
    }
    if route.host_eq {
        changes.extend(chain_words(route.stages));
    }
    match route.others_mixing {
        0 => {}
        1 => changes.push("1 other sound mixing".to_string()),
        many => changes.push(format!("{many} other sounds mixing")),
    }
    Hop {
        label: "computer".to_string(),
        detail: detail.join(", "),
        changes: changes.len() > before,
        unknown: unknown.len() > unknown_before,
    }
}

/// The words for the filter chain in the path: each stage that plays, in chain order, or the
/// flat pass-through kept while a stream plays; `host EQ on` where the stages were not told. The
/// Night drive's top line (`STAGE_WORDS` in bins/svolderd/web/themes/nightdrive/hud.js) picks these
/// words out of the path by their text: change both together.
fn chain_words(stages: Option<svolder_host::ChainStages>) -> Vec<String> {
    let Some(stages) = stages else {
        return vec!["host EQ on".to_string()];
    };
    let words: Vec<String> = [
        (stages.eq, "host EQ on"),
        (stages.crossfeed, "crossfeed on"),
        (stages.speakers, "virtual speakers on"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, word)| word.to_string())
    .collect();
    if words.is_empty() {
        vec!["EQ pass-through".to_string()]
    } else {
        words
    }
}

/// The DAC's hop: what the device itself reads against the sink's rate.
fn dac_hop(
    sink: &SinkFacts,
    dac: &DacFacts,
    changes: &mut Vec<String>,
    unknown: &mut Vec<String>,
) -> Hop {
    let before = changes.len();
    let unknown_before = unknown.len();
    let mut detail = Vec::new();
    if let Some(input) = &dac.input {
        detail.push(input.clone());
    }
    if let Some(rate) = dac.rate_hz {
        detail.push(format!("{} kHz", khz(rate)));
    }
    if dac.input.is_none() {
        unknown.push("DAC input not read".to_string());
    }
    if let UsbFeed::Unsure(doubt) = dac.usb_feed() {
        unknown.push(doubt.to_string());
    }
    match (sink.rate_hz, dac.rate_hz) {
        (_, None) => unknown.push("DAC rate not read".to_string()),
        (Some(sent), Some(read)) if sent != read => {
            changes.push(format!("DAC reads {} kHz", khz(read)));
        }
        _ => {}
    }
    detail.extend(dac.stage.hop_words());
    Hop {
        label: "DAC".to_string(),
        detail: detail.join(", "),
        changes: changes.len() > before || dac.stage.changes(),
        unknown: unknown.len() > unknown_before,
    }
}

/// A hop's rate, format and volume, as far as they are known.
fn signal_detail(
    rate_hz: Option<u32>,
    format: Option<&str>,
    volume_milli: Option<u32>,
) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(rate) = rate_hz {
        let mut part = format!("{} kHz", khz(rate));
        if let Some(format) = format {
            part.push(' ');
            part.push_str(format);
        }
        parts.push(part);
    } else if let Some(format) = format {
        parts.push(format.to_string());
    }
    if let Some(milli) = volume_milli {
        parts.push(format!("volume {}", percent(milli)));
    }
    parts
}

/// A rate in kilohertz: `44.1`, `48`, `176.4`; a decimal only when the rate needs one.
fn khz(rate_hz: u32) -> String {
    let (whole, rest) = (rate_hz / 1000, rate_hz % 1000);
    if rest == 0 {
        return whole.to_string();
    }
    let decimals = format!("{rest:03}");
    format!("{whole}.{}", decimals.trim_end_matches('0'))
}

/// A volume in thousandths as a percent with its unit: whole when it is, else with one
/// decimal (thousandths hold no more), so only 1000 ever reads `100 %`.
fn percent(milli: u32) -> String {
    match milli % 10 {
        0 => format!("{} %", milli / 10),
        tenth => format!("{}.{tenth} %", milli / 10),
    }
}

#[cfg(test)]
pub(super) mod tests;
