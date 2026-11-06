// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon tells its front ends about the computer's music player, and the one pure
//! judgement they share of the path the sound takes from the player to the DAC.
//!
//! - [`HostView`] and [`PlayingView`] are the wire types: what is playing, and what
//!   [`svolder_host::RouteFacts`] read out of PipeWire. They ride a `host` answer and the
//!   `host` event.
//! - [`path_line`] judges that route against the device's own readings ([`DacFacts`]) and
//!   gives a [`PathLine`]: the hops from the player to the DAC, a [`Verdict`] and the words
//!   that name what changes the signal. [`sink_is_dac`] says whether the computer's sink is
//!   the device at all, and [`rate_hz`] reads the sample rate a device's label names.
//!
//! - [`FrameView`] and [`CaptureView`] are the wire types of the daemon's capture: what a
//!   joined TUI reads on a `host_frames` stream (`Event::HostFrame`, `Event::HostCapture`).
//!
//! The verdict is untouched only when every hop is known to leave the signal alone. What the
//! daemon cannot see (the bits themselves) is never claimed: the words never say more than
//! the rates, the volumes, the mixing and the EQ show.

use serde::{Deserialize, Serialize};
use svolder_host::{PlayerMatch, Progress, RouteFacts, SinkFacts, StreamFacts};
use svolder_profiles::Profile;
use svolder_proto::FunctionId;

/// The USB vendor id Schiit's devices carry.
pub const SCHIIT_VENDOR_ID: u16 = 0x30be;

/// What a front end says of the path while the route has not been read.
pub const ROUTE_NOT_READ: &str = "route not read";

/// The word for a path that nothing alters.
const UNTOUCHED: &str = "untouched";

/// What the daemon knows of the host's player and route when it answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostView {
    /// Whether the daemon watches the host at all: false with `[host] enabled = false`,
    /// `SVOLDER_HOST=off`, or no session bus. Everything else is empty then.
    #[serde(default)]
    pub available: bool,
    /// The followed player and its track; absent while there is no player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playing: Option<PlayingView>,
    /// The route to the default sink, as the last read of the graph gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteFacts>,
    /// The daemon's clock when this was sent, in unix milliseconds: what the progress is
    /// counted forward from.
    #[serde(default)]
    pub now_ms: u64,
}

/// The followed player and its track, as a page or a TUI shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayingView {
    /// The track's title.
    #[serde(default)]
    pub title: String,
    /// The track's artist.
    #[serde(default)]
    pub artist: String,
    /// The track's album.
    #[serde(default)]
    pub album: String,
    /// The player's name, as it calls itself.
    #[serde(default)]
    pub player: String,
    /// What the player is doing: `playing`, `paused` or `stopped`.
    #[serde(default)]
    pub status: String,
    /// The track's length in microseconds, when the player says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_us: Option<u64>,
    /// Where the player was in the track when last asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    /// The hash of the cover the daemon holds, when it read one; the page asks for the cover
    /// by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
}

/// One analysed frame of what the computer plays, as a joined TUI is sent it
/// (`Event::HostFrame`): the daemon's [`svolder_host::Frame`] with its bands as plain arrays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameView {
    /// When the frame was captured, in unix milliseconds on the daemon's clock.
    pub t: u64,
    /// The left channel's bands, 240 bytes on the scale [`svolder_host::analysis::to_byte`]
    /// describes (a tuning, not a promise).
    pub left: Vec<u8>,
    /// The right channel's bands, 240 bytes.
    pub right: Vec<u8>,
    /// Left RMS, right RMS, left peak and right peak, as bytes on the same scale.
    pub levels: [u8; 4],
    /// Whether a beat starts in this frame.
    #[serde(default)]
    pub beat: bool,
    /// The tempo, once the last few seconds hold one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u16>,
    /// How far along the beat period the frame is, 0 at the beat to 255 just before the next.
    #[serde(default)]
    pub phase: u8,
}

impl From<&svolder_host::Frame> for FrameView {
    fn from(frame: &svolder_host::Frame) -> Self {
        FrameView {
            t: frame.capture_ms,
            left: frame.left.to_vec(),
            right: frame.right.to_vec(),
            levels: frame.levels,
            beat: frame.beat,
            bpm: frame.bpm,
            phase: frame.phase,
        }
    }
}

impl FrameView {
    /// The frame this view was made from; `None` when a channel does not hold exactly the
    /// bands a frame has (a peer that analyses differently).
    pub fn to_frame(&self) -> Option<svolder_host::Frame> {
        Some(svolder_host::Frame {
            capture_ms: self.t,
            left: self.left.as_slice().try_into().ok()?,
            right: self.right.as_slice().try_into().ok()?,
            levels: self.levels,
            beat: self.beat,
            bpm: self.bpm,
            phase: self.phase,
        })
    }
}

/// Where the daemon's capture stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    /// Nothing is captured yet: the capture is about to start, or waits for a first viewer.
    Starting,
    /// Frames are coming.
    Running,
    /// No capture is possible now (no `pw-record`, no sink, the host is off); `why` says.
    Unavailable,
}

/// The state of the daemon's capture, sent once when a frames stream opens and on each change
/// (`Event::HostCapture`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureView {
    /// Where the capture stands.
    pub state: CaptureKind,
    /// While it runs, what the captured sink is called for people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,
    /// While it is unavailable, why, in a plain sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What the device itself reports of its end of the path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        DacFacts {
            display: profile.display.clone(),
            input,
            rate_hz: reading("sample_rate").as_deref().and_then(rate_hz),
            usb_ports: usb_ports.map(|(one, two)| [one, two]),
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
    /// one plain sentence for a sound that goes elsewhere.
    pub words: Vec<String>,
}

impl PathLine {
    /// The words on one line, joined by ` · ` as both front ends show them.
    pub fn words_line(&self) -> String {
        self.words.join(" · ")
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
        };
    };
    if !sink_is_dac(sink, dac) {
        let name = sink_name(sink);
        return elsewhere(format!("the computer plays to {name}"), name);
    }
    if let PlayerMatch::Elsewhere { sink: other } = &route.player_match {
        return elsewhere(format!("the player plays to {other}"), other.clone());
    }
    if let Some(input) = dac.input.as_ref().filter(|input| !is_usb_input(input)) {
        return PathLine {
            hops: Vec::new(),
            verdict: Verdict::OtherInput {
                input: input.clone(),
            },
            words: vec![format!("the DAC listens to {input}")],
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
    }
}

/// The line for a sound that goes to a sink other than the device.
fn elsewhere(sentence: String, sink: String) -> PathLine {
    PathLine {
        hops: Vec::new(),
        verdict: Verdict::NotThisDac { sink },
        words: vec![sentence],
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
        changes.push("host EQ on".to_string());
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
    Hop {
        label: "DAC".to_string(),
        detail: detail.join(", "),
        changes: changes.len() > before,
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
mod tests;
