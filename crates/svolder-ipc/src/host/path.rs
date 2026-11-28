// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The path verdict: the hops from the player to the DAC, judged against what the device reports.

use super::sample::{AlsaName, HwJudge, Sample, alsa_name, base_name, hw_judge, khz, sample_of};
use svolder_host::{HwState, Layout, PlayerMatch, RouteFacts, SetupFacts, SinkFacts, StreamFacts};
use svolder_profiles::{EqMode, Profile, roles, safety, units};
use svolder_proto::FunctionId;

/// The USB vendor id Schiit's devices carry.
pub const SCHIIT_VENDOR_ID: u16 = 0x30be;

/// What a front end says of the path while the route has not been read.
pub const ROUTE_NOT_READ: &str = "route not read";

/// The player hop's label for a stream with no MPRIS player behind it.
pub(crate) const NO_MPRIS_HOP: &str = "player (no MPRIS)";

/// What the Path says when several streams play and no MPRIS player tells them apart.
pub const NO_MPRIS_PLAYER: &str = "no MPRIS player";

/// What the Path says when no stream plays into the sink.
pub const NOTHING_PLAYING: &str = "nothing playing";

/// Whether the route has a sound playing into the sink: a stream the route took (the player's, or
/// the lone one with no MPRIS player) or several running that none can be told from. Front ends
/// ask it while no MPRIS player is followed, to tell a sound that plays from silence.
pub fn plays_without_player(route: &RouteFacts) -> bool {
    route.player.is_some() || route.others_mixing > 0
}

/// The word for a path that nothing alters.
const UNTOUCHED: &str = "untouched";
/// What follows the words of an untouched path in its tooltip: the verdict cannot see inside
/// the player.
const UNTOUCHED_NOTE: &str = "the player may still change the sound itself (loudness \
                              normalization, ReplayGain, its own EQ); svolder judges up to 24 \
                              bits";

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
    pub(super) fn lowers(&self) -> Option<f64> {
        self.volume_db.filter(|db| *db < -0.05)
    }

    /// Whether the stage changes the sound at all.
    pub fn changes(&self) -> bool {
        self.lowers().is_some() || self.tone || self.balance || self.muted || self.phase || self.nos
    }

    /// The volume `db` as hop words: `volume -3.3 dB`, or `volume silent` for negative infinity.
    pub(super) fn volume_word(db: f64) -> String {
        match db.is_finite() {
            true => format!(
                "volume {}",
                units::amount_text(units::Amount::of(db), Some("dB"))
            ),
            false => "volume silent".to_string(),
        }
    }

    /// What the stage changes besides the volume, as short hop words: `tone control`, `balance
    /// off centre`, `muted`, `phase inverted`, `NOS filter`.
    pub(super) fn other_words(&self) -> Vec<String> {
        let mut words = Vec::new();
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

    /// The stage as short hop words: `volume -3.3 dB`, `tone control`, `balance off centre`.
    pub(super) fn hop_words(&self) -> Vec<String> {
        let mut words = Vec::new();
        if let Some(db) = self.lowers() {
            words.push(Self::volume_word(db));
        }
        words.extend(self.other_words());
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

impl Verdict {
    /// Every key, in the order of the variants.
    pub const KEYS: [&'static str; 5] = [
        "untouched",
        "changed",
        "not_this_dac",
        "other_input",
        "unknown",
    ];

    /// The stable word every JSON surface carries for the verdict (the web card, Home Assistant,
    /// MCP, the CLI's `--json`).
    pub fn key(&self) -> &'static str {
        match self {
            Verdict::Untouched => "untouched",
            Verdict::Changed => "changed",
            Verdict::NotThisDac { .. } => "not_this_dac",
            Verdict::OtherInput { .. } => "other_input",
            Verdict::Unknown => "unknown",
        }
    }
}

/// One thing a hop does to the signal: its full words, as the path's words say it, and the short
/// form the Path row shows at the hop (the hop's label is never repeated in it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The full words, such as `resampled 44.1 → 48 kHz` or `player volume 50 %`.
    pub words: String,
    /// The short form, such as `resampled` or `volume 50 %`.
    pub short: String,
}

impl Change {
    fn new(words: impl Into<String>, short: impl Into<String>) -> Self {
        Change {
            words: words.into(),
            short: short.into(),
        }
    }
}

/// One hop of the path: the player, the computer, or the DAC.
#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    /// Which hop: `player`, `computer` or `DAC`.
    pub label: String,
    /// What is known of the signal there: its rate, format and volume.
    pub detail: String,
    /// Whether this hop alters the signal.
    pub changes: bool,
    /// Whether something of this hop was not read; its words are in the path's `unknown` list.
    pub unknown: bool,
    /// Whether this hop cannot be named or controlled (a stream with no MPRIS player behind
    /// it); a row draws it with a dim `?` though nothing of it was missed.
    pub unnamed: bool,
    /// What this hop changes, each change recorded at the hop that makes it, in the order the
    /// path's words name them. Empty when `changes` is only the DAC's own stage.
    pub changed: Vec<Change>,
    /// The sample rate at this hop: the player's stream rate, the sink's rate, the rate the DAC
    /// reads; `None` when not read.
    pub rate_hz: Option<u32>,
    /// For the computer hop when it resamples: the rate the sound ends at (the forced rate when
    /// the clock is forced, else the sink's).
    pub resamples_to: Option<u32>,
    /// For the DAC hop: the input it reads, as its label.
    pub input: Option<String>,
    /// For the DAC hop: what the device's own stage does.
    pub stage: Option<DacStage>,
}

/// The path from the player to the DAC, judged.
#[derive(Debug, Clone, PartialEq)]
pub struct PathLine {
    /// The hops in order; empty when the verdict is [`Verdict::NotThisDac`] or
    /// [`Verdict::OtherInput`], or the sink is not known.
    pub hops: Vec<Hop>,
    /// What the path does to the signal.
    pub verdict: Verdict,
    /// What changes the signal, or what is not known, in hop order; `untouched` for an
    /// untouched path (with `player assumed` after it when the stream was only assumed, `no MPRIS
    /// player` when no player was followed); the
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
        return sink_not_read();
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

    if is_several_streams(route) {
        return several_streams();
    }
    let mut side = computer_side(route, sink);
    let device = dac_hop(sink, dac, &mut side.unknown);
    let hops = vec![side.player, side.computer, device];
    let (verdict, words) = verdict_of(route, change_words(&hops), side.unknown);
    PathLine {
        hops,
        verdict,
        words,
        dac_stage: dac.stage.clause(&dac.display),
    }
}

/// The path from the player to the computer's sink, with no DAC hop: the verdict the computer
/// alone earns, for a sink that is no configured DAC and for the checks that judge the
/// computer's side only. A route with no sink is unknown, and a player that plays to another
/// sink is a sound that goes elsewhere.
pub fn computer_line(route: &RouteFacts) -> PathLine {
    let Some(sink) = &route.sink else {
        return sink_not_read();
    };
    if let PlayerMatch::Elsewhere { sink: other } = &route.player_match {
        return elsewhere(format!("player → {other}"), other.clone(), true);
    }
    if is_several_streams(route) {
        return several_streams();
    }
    let side = computer_side(route, sink);
    let hops = vec![side.player, side.computer];
    let (verdict, words) = verdict_of(route, change_words(&hops), side.unknown);
    PathLine {
        hops,
        verdict,
        words,
        dac_stage: None,
    }
}

/// The line for several running streams and no MPRIS player: not judged, with no hops, as the
/// sound's source cannot be told from the others.
fn several_streams() -> PathLine {
    PathLine {
        hops: Vec::new(),
        verdict: Verdict::Unknown,
        words: vec![NO_MPRIS_PLAYER.to_string()],
        dac_stage: None,
    }
}

/// Whether the route has several running streams and no MPRIS player to tell them apart.
fn is_several_streams(route: &RouteFacts) -> bool {
    route.player_match == PlayerMatch::NoPlayer && route.others_mixing > 0
}

/// The line for a route whose sink the dump did not name.
fn sink_not_read() -> PathLine {
    PathLine {
        hops: Vec::new(),
        verdict: Verdict::Unknown,
        words: vec!["the computer's sink was not read".to_string()],
        dac_stage: None,
    }
}

/// The player's and the computer's hops with what they leave unknown.
struct Side {
    player: Hop,
    computer: Hop,
    unknown: Vec<String>,
}

/// The full words of every change of `hops`, in hop order.
fn change_words(hops: &[Hop]) -> Vec<String> {
    hops.iter()
        .flat_map(|hop| hop.changed.iter().map(|change| change.words.clone()))
        .collect()
}

/// Judges the player's and the computer's hops against `sink`.
fn computer_side(route: &RouteFacts, sink: &SinkFacts) -> Side {
    let mut unknown = Vec::new();
    let player = player_hop(route, &mut unknown);
    let computer = computer_hop(route, sink, &mut unknown);
    Side {
        player,
        computer,
        unknown,
    }
}

/// The verdict and words of the changes and unknowns found: a change outranks an unknown, and
/// an untouched path says so (with `player assumed` after it for an assumed stream, `no MPRIS
/// player` for one with no player).
fn verdict_of(
    route: &RouteFacts,
    changes: Vec<String>,
    unknown: Vec<String>,
) -> (Verdict, Vec<String>) {
    if !changes.is_empty() {
        (Verdict::Changed, changes)
    } else if !unknown.is_empty() {
        (Verdict::Unknown, unknown)
    } else if route.player_match == PlayerMatch::Assumed {
        (
            Verdict::Untouched,
            vec![UNTOUCHED.to_string(), "player assumed".to_string()],
        )
    } else if route.player_match == PlayerMatch::NoMpris {
        (
            Verdict::Untouched,
            vec![UNTOUCHED.to_string(), NO_MPRIS_PLAYER.to_string()],
        )
    } else {
        (Verdict::Untouched, vec![UNTOUCHED.to_string()])
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
pub(super) fn sink_name(sink: &SinkFacts) -> String {
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
        PlayerMatch::Pid | PlayerMatch::Name | PlayerMatch::Assumed | PlayerMatch::NoMpris
    );
    route.player.as_ref().filter(|_| identified)
}

/// The player's hop; what it changes and what it leaves unknown go to the two lists.
fn player_hop(route: &RouteFacts, unknown: &mut Vec<String>) -> Hop {
    let Some(stream) = identified_stream(route) else {
        unknown.push(match route.player_match {
            PlayerMatch::NoPlayer => NOTHING_PLAYING.to_string(),
            _ => "player not identified".to_string(),
        });
        return Hop {
            label: "player".to_string(),
            detail: "not identified".to_string(),
            changes: false,
            unknown: true,
            unnamed: false,
            changed: Vec::new(),
            rate_hz: None,
            resamples_to: None,
            input: None,
            stage: None,
        };
    };
    let mut detail = signal_detail(
        stream.rate_hz,
        stream.format.as_deref(),
        stream.volume_milli,
    );
    let mut changed = Vec::new();
    let unknown_before = unknown.len();
    if stream.rate_hz.is_none() {
        unknown.push("player rate not read".to_string());
    }
    stream_volume(stream, &mut changed, unknown);
    if stream.muted {
        detail.push("muted".to_string());
    }
    if route.player_match == PlayerMatch::Assumed {
        detail.push("assumed".to_string());
    }
    let no_mpris = route.player_match == PlayerMatch::NoMpris;
    Hop {
        label: if no_mpris { NO_MPRIS_HOP } else { "player" }.to_string(),
        detail: detail.join(", "),
        changes: !changed.is_empty(),
        unknown: unknown.len() > unknown_before,
        unnamed: no_mpris,
        changed,
        rate_hz: stream.rate_hz,
        resamples_to: None,
        input: None,
        stage: None,
    }
}

/// Names a stream's volume below 100 % and its mute; a volume not read is unknown.
fn stream_volume(stream: &StreamFacts, changes: &mut Vec<Change>, unknown: &mut Vec<String>) {
    match stream.volume_milli {
        Some(milli) if milli != 1000 => changes.push(Change::new(
            format!("player volume {}", percent(milli)),
            format!("volume {}", percent(milli)),
        )),
        Some(_) => {}
        None => unknown.push("player volume not read".to_string()),
    }
    if stream.muted {
        changes.push(Change::new("player muted", "muted"));
    }
}

/// The computer's hop: the sink's own rate and volume, the EQ and the other sounds, and, when
/// the route carries its setup, what the sink's converter and ALSA device do to the sound.
fn computer_hop(route: &RouteFacts, sink: &SinkFacts, unknown: &mut Vec<String>) -> Hop {
    let mut changed = Vec::new();
    let changes = &mut changed;
    let mut resamples_to = None;
    let unknown_before = unknown.len();
    let mut detail = signal_detail(sink.rate_hz, sink.format.as_deref(), sink.volume_milli);
    let forced = route
        .setup
        .as_ref()
        .and_then(|setup| setup.clock.as_ref())
        .and_then(|clock| clock.force_rate);
    match (
        identified_stream(route).and_then(|stream| stream.rate_hz),
        sink.rate_hz,
    ) {
        (Some(from), Some(to)) if from != to => {
            let cause = if forced.is_some_and(|force| force != from) {
                " (rate forced)"
            } else {
                ""
            };
            changes.push(Change::new(
                format!("resampled {} → {} kHz{cause}", khz(from), khz(to)),
                "resampled",
            ));
            resamples_to = Some(to);
        }
        (Some(from), Some(_)) => {
            // The sink's Format can still read the stream's rate while the clock is forced to
            // another: the sound is resampled all the same.
            if let Some(force) = forced.filter(|force| *force != from) {
                changes.push(Change::new(
                    format!("resampled {} → {} kHz (rate forced)", khz(from), khz(force)),
                    "resampled",
                ));
                resamples_to = Some(force);
            } else if route
                .setup
                .as_ref()
                .is_some_and(|setup| setup.clock.is_none())
            {
                // Without the clock an equal pair of rates does not prove there is no resample.
                unknown.push("forced rate not read".to_string());
            }
        }
        (_, None) => unknown.push("computer rate not read".to_string()),
        _ => {}
    }
    match sink.volume_milli {
        Some(milli) if milli != 1000 => changes.push(Change::new(
            format!("computer volume {}", percent(milli)),
            format!("volume {}", percent(milli)),
        )),
        Some(_) => {}
        None => unknown.push("computer volume not read".to_string()),
    }
    if sink.muted {
        changes.push(Change::new("computer muted", "muted"));
    }
    if route.host_eq {
        changes.extend(
            chain_words(route.stages)
                .into_iter()
                .map(|words| Change::new(words, "EQ")),
        );
    }
    match route.others_mixing {
        0 => {}
        1 => changes.push(Change::new("1 other sound mixing", "1 other sound mixing")),
        many => {
            let words = format!("{many} other sounds mixing");
            changes.push(Change::new(words.clone(), words));
        }
    }
    if let Some(stream) = identified_stream(route) {
        width_words(stream, sink, changes, unknown);
    }
    if let Some(setup) = &route.setup {
        setup_words(route, sink, setup, &mut detail, changes, unknown);
    }
    Hop {
        label: "computer".to_string(),
        detail: detail.join(", "),
        changes: !changed.is_empty(),
        unknown: unknown.len() > unknown_before,
        unnamed: false,
        changed,
        rate_hz: sink.rate_hz,
        resamples_to,
        input: None,
        stage: None,
    }
}

/// What the setup facts say of the sink's converter and ALSA device, in the order channels,
/// dither, filters, ALSA (the width, which needs only the two formats, comes before them). A
/// fact the route does not carry is skipped by the caller (an older svolderd); one it carries as
/// unread is unknown.
fn setup_words(
    route: &RouteFacts,
    sink: &SinkFacts,
    setup: &SetupFacts,
    detail: &mut Vec<String>,
    changes: &mut Vec<Change>,
    unknown: &mut Vec<String>,
) {
    let stream = identified_stream(route);
    if stream.is_some() {
        channel_words(setup, detail, changes, unknown);
    }
    dither_words(setup, changes, unknown);
    filter_words(setup, stream.is_some(), changes, unknown);
    alsa_words(sink, setup, changes, unknown);
}

/// The width rule: an integer stream into a narrower integer sink is cut; a float stream into
/// a sink under 24 bits may or may not hold more than the sink does; a format name that is not
/// known is not judged. Bits count up to 24, where PipeWire's float mix ends.
fn width_words(
    stream: &StreamFacts,
    sink: &SinkFacts,
    changes: &mut Vec<Change>,
    unknown: &mut Vec<String>,
) {
    let Some(to) = sink.format.as_deref() else {
        return;
    };
    let Some(from) = stream.format.as_deref() else {
        // Only a sink of 24 bits or more (or a float one) takes any stream whole.
        if !matches!(
            sample_of(to),
            Some(Sample::Float | Sample::Int { bits: 24.., .. })
        ) {
            unknown.push("player format not read".to_string());
        }
        return;
    };
    let (read_from, read_to) = (sample_of(from), sample_of(to));
    for (name, read) in [(from, read_from), (to, read_to)] {
        let word = format!("format {name} not judged");
        if read.is_none() && !unknown.contains(&word) {
            unknown.push(word);
        }
    }
    let (Some(read_from), Some(read_to)) = (read_from, read_to) else {
        return;
    };
    let Sample::Int {
        bits: sink_bits, ..
    } = read_to
    else {
        return;
    };
    match read_from.counted_bits() {
        Some(counted) if counted > sink_bits => changes.push(Change::new(
            format!(
                "cut to {sink_bits} bits ({} → {})",
                base_name(from),
                base_name(to)
            ),
            format!("cut to {sink_bits} bits"),
        )),
        None if sink_bits < 24 => {
            unknown.push(format!("float stream into a {sink_bits}-bit sink"));
        }
        _ => {}
    }
}

/// The channel rule: mono on both channels of a stereo sink is no change (both channels are
/// exact copies); any other count that differs is mixed or upmixed; the same count at other
/// positions is remapped (positions compare as sets: PipeWire maps by position, not order).
fn channel_words(
    setup: &SetupFacts,
    detail: &mut Vec<String>,
    changes: &mut Vec<Change>,
    unknown: &mut Vec<String>,
) {
    let (Some(from), Some(to)) = (&setup.stream_layout, &setup.sink_layout) else {
        unknown.push("channels not read".to_string());
        return;
    };
    match (from.channels, to.channels) {
        (a, b) if a == b => match same_positions(from, to) {
            Positions::Same => {}
            Positions::Differ => {
                changes.push(Change::new("channels remapped", "channels remapped"))
            }
            Positions::PartlyUnnamed => unknown.push("channels partly unnamed".to_string()),
        },
        (1, 2) => one_into_two(from, to, detail, changes),
        (a, b) if b < a => changes.push(Change::new(
            format!("channels {a} → {b} (mixed)"),
            "channels mixed",
        )),
        (a, b) => changes.push(Change::new(
            format!("channels {a} → {b} (upmixed)"),
            "channels upmixed",
        )),
    }
}

/// A one-channel stream into a stereo sink. PipeWire spreads a channel that names no position
/// (`UNK`, `NA`, `AUXn`) or is `MONO` to both at full level (channelmix-ops.c, `make_matrix`:
/// "distribute UNK", "assign MONO to STEREO (1.0)"). `FC` comes out on both at 1/sqrt(2)
/// (-3 dB: "assign FC to STEREO", L290-294), and `FL` or `FR` alone leaves the other channel
/// silent. The sink's positions decide first: see the body.
fn one_into_two(from: &Layout, to: &Layout, detail: &mut Vec<String>, changes: &mut Vec<Change>) {
    let position = from.positions.first().map_or("", String::as_str);
    // A sink that names none, or a source that does, is distributed at full level (L221-225).
    if names_none(to) || position.is_empty() || is_unnamed(position) {
        detail.push("mono on both channels".to_string());
        return;
    }
    let stereo = {
        let mut sorted = to.positions.clone();
        sorted.sort();
        sorted == ["FL", "FR"]
    };
    if !stereo {
        // MONO into a sink without FL and FR is silent ("can't assign MONO", L279-285).
        changes.push(Change::new("channels remapped", "channels remapped"));
        return;
    }
    match position {
        "MONO" => detail.push("mono on both channels".to_string()),
        "FC" => changes.push(Change::new(
            "mono centre 3 dB lower on both channels",
            "mono centre -3 dB",
        )),
        "FL" | "FR" => changes.push(Change::new("one channel silent", "one channel silent")),
        _ => changes.push(Change::new("channels remapped", "channels remapped")),
    }
}

/// How two layouts of the same count compare.
enum Positions {
    Same,
    Differ,
    /// Equal sets, but some position of a layout names none beside named ones: PipeWire does not
    /// pair the unnamed channel, so the copy cannot be vouched for.
    PartlyUnnamed,
}

/// Whether two layouts of the same count hold the same positions; a layout that names none
/// (empty, only `UNK`, `NA` and `AUXn`, or `MONO` among several channels) is taken as equal to
/// any: PipeWire then pairs the channels one to one, in its sorted position order (channelmix-ops.c
/// L212-215 and L236-240). A layout mixing named and unnamed positions compares as sets like any
/// other, PipeWire dropping the unnamed ones from the match, so a difference may be real and an
/// equal set is no proof of a copy.
fn same_positions(a: &Layout, b: &Layout) -> Positions {
    if names_none(a) || names_none(b) {
        return Positions::Same;
    }
    let sorted = |layout: &Layout| {
        let mut positions = layout.positions.clone();
        positions.sort();
        positions
    };
    if sorted(a) != sorted(b) {
        Positions::Differ
    } else if [a, b]
        .iter()
        .any(|layout| layout.positions.iter().any(|p| is_unnamed(p)))
    {
        Positions::PartlyUnnamed
    } else {
        Positions::Same
    }
}

/// Whether no position of the layout is a named one (`FL`, `FR`, ...); several channels with
/// `MONO` among them count so too.
fn names_none(layout: &Layout) -> bool {
    (layout.channels > 1 && layout.positions.iter().any(|p| p == "MONO"))
        || layout.positions.iter().all(|p| is_unnamed(p))
}

/// `UNK`, `NA` or `AUXn`: a position PipeWire maps to no channel bit.
fn is_unnamed(position: &str) -> bool {
    matches!(position, "UNK" | "NA")
        || position
            .strip_prefix("AUX")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The sink's converter's dither: on, with noise, or not read.
fn dither_words(setup: &SetupFacts, changes: &mut Vec<Change>, unknown: &mut Vec<String>) {
    let Some(dither) = &setup.dither else {
        unknown.push("dither not read".to_string());
        return;
    };
    if !matches!(dither.method.as_str(), "" | "none") {
        changes.push(Change::new(
            format!("dither on ({})", dither.method),
            "dither",
        ));
    }
    if dither.noise != 0 {
        changes.push(Change::new(
            format!("dither noise {}", dither.noise),
            "dither",
        ));
    }
}

/// A filter graph inside the sink's converter or the player's stream's.
fn filter_words(
    setup: &SetupFacts,
    stream_known: bool,
    changes: &mut Vec<Change>,
    unknown: &mut Vec<String>,
) {
    let stream = stream_known.then_some(setup.stream_filter);
    if setup.sink_filter == Some(true) {
        changes.push(Change::new("a filter inside the sink", "sink filter"));
    }
    if stream == Some(Some(true)) {
        changes.push(Change::new(
            "a filter on the player's stream",
            "stream filter",
        ));
    }
    if setup.sink_filter.is_none() || stream == Some(None) {
        unknown.push("filter not read".to_string());
    }
}

/// The ALSA device: `dmix` always mixes (other ALSA clients can share the card, which neither
/// `hw_params` nor the dump shows); when the card's `hw_params` are open, what differs from the
/// sink's format is what ALSA converts, and a format the judges cannot read is unknown, never
/// equal; otherwise the device name says (`plug` converts, `hw` and its kin do not, any other
/// name is not judged). A sink that is not an ALSA sink is not judged.
fn alsa_words(
    sink: &SinkFacts,
    setup: &SetupFacts,
    changes: &mut Vec<Change>,
    unknown: &mut Vec<String>,
) {
    let Some(alsa) = &setup.alsa else {
        return;
    };
    let name = alsa.path.as_deref().map(|path| (path, alsa_name(path)));
    if let Some((path, AlsaName::Dmix)) = name {
        changes.push(Change::new(
            format!("ALSA dmix mixes ({path})"),
            "ALSA dmix",
        ));
    }
    if let HwState::Open(hw) = &alsa.hw {
        let channels = setup.sink_layout.as_ref().map(|layout| layout.channels);
        match hw_judge(sink, channels, hw) {
            HwJudge::Differs(parts) => {
                changes.push(Change::new(
                    format!("ALSA converts {}", parts.join(", ")),
                    "ALSA converts",
                ));
                return;
            }
            HwJudge::Same => return,
            HwJudge::Unreadable(words) if unknown.contains(&words) => {}
            HwJudge::Unreadable(words) => unknown.push(words),
            HwJudge::NotComparable => {}
        }
    }
    match name {
        None => unknown.push("ALSA device not read".to_string()),
        Some((path, AlsaName::Plug)) => changes.push(Change::new(
            format!("ALSA plug converts ({path})"),
            "ALSA plug",
        )),
        Some((_, AlsaName::Dmix | AlsaName::Direct)) => {}
        Some((path, AlsaName::Other)) => unknown.push(format!("ALSA device {path} not judged")),
    }
}

/// The words for the filter chain in the path: each stage that plays, in chain order, or the
/// flat pass-through kept while a stream plays (none for one proved untouched); `host EQ on`
/// where the stages were not told.
/// The web themes' path words (`STAGE_WORDS` in bins/svolderd/web/core/path-words.js) pick these
/// words out of the path by their text: change both together.
fn chain_words(stages: Option<svolder_host::ChainStages>) -> Vec<String> {
    let Some(stages) = stages else {
        return vec!["host EQ on".to_string()];
    };
    // A pass-through the daemon proved untouched changes nothing: no word, so the hop is green.
    if stages.untouched && !(stages.eq || stages.loudness || stages.crossfeed || stages.speakers) {
        return Vec::new();
    }
    let words: Vec<String> = [
        (stages.eq, "host EQ on"),
        (stages.loudness, "loudness on"),
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
fn dac_hop(sink: &SinkFacts, dac: &DacFacts, unknown: &mut Vec<String>) -> Hop {
    let mut changed = Vec::new();
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
            changed.push(Change::new(
                format!("DAC reads {} kHz", khz(read)),
                format!("reads {} kHz", khz(read)),
            ));
        }
        _ => {}
    }
    detail.extend(dac.stage.hop_words());
    Hop {
        label: "DAC".to_string(),
        detail: detail.join(", "),
        changes: !changed.is_empty() || dac.stage.changes(),
        unknown: unknown.len() > unknown_before,
        unnamed: false,
        changed,
        rate_hz: dac.rate_hz,
        resamples_to: None,
        input: dac.input.clone(),
        stage: Some(dac.stage),
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
