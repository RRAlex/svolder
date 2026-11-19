// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The setup check: whether the PipeWire setup lets a single player reach the DAC untouched,
//! one pass, fail or unknown line per condition, whatever plays. The one judge of these lines:
//! every front end shows the words it is given.

use super::path::sink_name;
use super::sample::{AlsaName, HwJudge, Sample, alsa_name, hw_judge, pipewire_name, sample_of};
use serde::{Deserialize, Serialize};
use svolder_host::{HwState, RouteFacts, SetupFacts, SinkFacts};

/// The highest rate the check judges or lists, in hertz.
const RATE_CEILING: u32 = 384_000;

/// The rates a DAC is taken to take inside a range of rates it offers, in hertz.
const STANDARD_RATES: [u32; 8] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000,
];

/// What the check cannot judge, as one sentence every front end shows (and the JSON carries as
/// `limit`).
pub const SETUP_LIMIT: &str = "svolder judges the PipeWire config from the player’s stream on. \
It cannot see inside a player, nor past the sink’s converter (ALSA hw, USB, the DAC), and it \
judges at most 24 bits. It only reads: it never changes a setting.";

/// How one line of the check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupOutcome {
    /// The condition holds.
    Pass,
    /// The condition does not hold: the PipeWire config changes the sound.
    Fail,
    /// The condition could not be read or judged.
    Unknown,
}

impl SetupOutcome {
    /// Every key, in the order of the variants.
    pub const KEYS: [&'static str; 3] = ["pass", "fail", "unknown"];

    /// The stable word JSON carries for the outcome (the same as its serialized form).
    pub fn key(self) -> &'static str {
        match self {
            SetupOutcome::Pass => "pass",
            SetupOutcome::Fail => "fail",
            SetupOutcome::Unknown => "unknown",
        }
    }
}

/// One condition of the check and what was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupLine {
    /// A stable word for the condition: `rates`, `forced_rate`, `dither`, `format`, `plug`,
    /// `stereo` or `filter`.
    pub key: String,
    /// How the condition came out.
    pub outcome: SetupOutcome,
    /// What was found, in words (`44.1-384 kHz all allowed`).
    pub words: String,
}

impl SetupLine {
    /// The condition as a name for a row: `24-bit output`, `no plug`.
    pub fn label(&self) -> &'static str {
        match self.key.as_str() {
            "rates" => "rates",
            "forced_rate" => "forced rate",
            "dither" => "dither",
            "format" => "24-bit output",
            "plug" => "no plug",
            "stereo" => "stereo to stereo",
            "filter" => "no filter in the sink",
            _ => "",
        }
    }
}

/// What the lines of a check add up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupSummary {
    /// Every line passes.
    Untouched,
    /// This many lines fail.
    Changes(u32),
    /// No line fails and this many could not be read.
    Unread(u32),
    /// The setup was not read at all.
    NotRead,
    /// The route came with a sink but no config facts this build reads: an older svolderd sent
    /// it, or a newer one sent facts this build cannot decode (the words name no direction).
    OtherVersion,
}

impl SetupSummary {
    /// Every key, in the order of the variants.
    pub const KEYS: [&'static str; 5] = [
        "untouched",
        "changes",
        "unread",
        "not_read",
        "other_version",
    ];

    /// The stable word JSON carries for the summary, whatever its count.
    pub fn key(self) -> &'static str {
        match self {
            SetupSummary::Untouched => "untouched",
            SetupSummary::Changes(_) => "changes",
            SetupSummary::Unread(_) => "unread",
            SetupSummary::NotRead => "not_read",
            SetupSummary::OtherVersion => "other_version",
        }
    }
}

/// The setup check of one route: the sink judged and its lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupCheck {
    /// The sink the check judges, as people name it; empty when the route names none.
    pub sink: String,
    /// The seven conditions, in a fixed order.
    pub lines: Vec<SetupLine>,
    /// What the lines add up to.
    pub summary: SetupSummary,
}

impl SetupCheck {
    /// The summary in words: `PipeWire config lets it through untouched`, `PipeWire config
    /// changes the sound: 2`, `PipeWire config not fully read: 1`, `PipeWire config not read`
    /// (with `(svolderd's version differs)` when the route has a sink and no config facts this
    /// build reads: this build always sends them).
    pub fn summary_words(&self) -> String {
        match self.summary {
            SetupSummary::Untouched => "PipeWire config lets it through untouched".to_string(),
            SetupSummary::Changes(fails) => format!("PipeWire config changes the sound: {fails}"),
            SetupSummary::Unread(unknown) => format!("PipeWire config not fully read: {unknown}"),
            SetupSummary::NotRead => "PipeWire config not read".to_string(),
            SetupSummary::OtherVersion => {
                "PipeWire config not read (svolderd's version differs)".to_string()
            }
        }
    }

    /// The short chip for a row beside the verdict: `config ✓`, `config 2` (lines that fail) or
    /// `config ?` (something not read).
    pub fn chip(&self) -> String {
        match self.summary {
            SetupSummary::Untouched => "config ✓".to_string(),
            SetupSummary::Changes(fails) => format!("config {fails}"),
            SetupSummary::Unread(_) | SetupSummary::NotRead | SetupSummary::OtherVersion => {
                "config ?".to_string()
            }
        }
    }
}

/// The keys of the lines, in the order they are listed.
const KEYS: [&str; 7] = [
    "rates",
    "forced_rate",
    "dither",
    "format",
    "plug",
    "stereo",
    "filter",
];

/// The setup check of `route`: its sink's lines. A route with no setup facts (an older svolderd)
/// or no sink reads as not read, with every line unknown.
pub fn setup_check(route: &RouteFacts) -> SetupCheck {
    let (Some(sink), Some(setup)) = (&route.sink, &route.setup) else {
        return SetupCheck {
            sink: route.sink.as_ref().map(sink_name).unwrap_or_default(),
            lines: KEYS
                .iter()
                .map(|key| line(key, SetupOutcome::Unknown, "not read"))
                .collect(),
            summary: match (&route.sink, &route.setup) {
                (Some(_), None) => SetupSummary::OtherVersion,
                _ => SetupSummary::NotRead,
            },
        };
    };
    let lines = vec![
        rates_line(setup),
        forced_rate_line(setup),
        dither_line(setup),
        format_line(sink, setup),
        plug_line(sink, setup),
        stereo_line(setup),
        filter_line(setup),
    ];
    let count = |wanted| lines.iter().filter(|l| l.outcome == wanted).count() as u32;
    let (fails, unknown) = (count(SetupOutcome::Fail), count(SetupOutcome::Unknown));
    let summary = if fails > 0 {
        SetupSummary::Changes(fails)
    } else if unknown > 0 {
        SetupSummary::Unread(unknown)
    } else {
        SetupSummary::Untouched
    };
    SetupCheck {
        sink: sink_name(sink),
        lines,
        summary,
    }
}

fn line(key: &str, outcome: SetupOutcome, words: impl Into<String>) -> SetupLine {
    SetupLine {
        key: key.to_string(),
        outcome,
        words: words.into(),
    }
}

/// Rates as kilohertz, comma separated: `44.1, 88.2`.
fn khz_list(rates: &[u32]) -> String {
    let each: Vec<String> = rates.iter().map(|rate| super::sample::khz(*rate)).collect();
    each.join(", ")
}

/// The rates the DAC takes, up to the ceiling: the kernel's list when the route has it, else the
/// sink's offer (a range as the standard rates inside it); `None` when neither says.
fn dac_rates(setup: &SetupFacts) -> Option<Vec<u32>> {
    let kernel = setup
        .alsa
        .as_ref()
        .and_then(|alsa| alsa.kernel.as_ref())
        .map(|kernel| kernel.rates.clone())
        .filter(|rates| !rates.is_empty());
    let mut rates = match kernel {
        Some(rates) => rates,
        None => match setup.offer.as_ref()?.rates.as_ref()? {
            svolder_host::Rates::One(rate) => vec![*rate],
            svolder_host::Rates::Range { min, max } => STANDARD_RATES
                .into_iter()
                .filter(|rate| (*min..=*max).contains(rate))
                .collect(),
            svolder_host::Rates::List(list) => list.clone(),
        },
    };
    rates.retain(|rate| *rate <= RATE_CEILING);
    rates.sort_unstable();
    rates.dedup();
    (!rates.is_empty()).then_some(rates)
}

/// The rates the setup lets through: the node's own list, else the clock's.
fn allowed_rates(setup: &SetupFacts) -> Option<Vec<u32>> {
    let listed = if setup.node_rates.is_empty() {
        setup
            .clock
            .as_ref()
            .map(|clock| clock.allowed_rates.clone())?
    } else {
        setup.node_rates.clone()
    };
    let mut rates: Vec<u32> = listed.into_iter().filter(|r| *r <= RATE_CEILING).collect();
    rates.sort_unstable();
    rates.dedup();
    (!rates.is_empty()).then_some(rates)
}

fn rates_line(setup: &SetupFacts) -> SetupLine {
    let Some(dac) = dac_rates(setup) else {
        return line("rates", SetupOutcome::Unknown, "the DAC's rates not read");
    };
    let Some(allowed) = allowed_rates(setup) else {
        return line("rates", SetupOutcome::Unknown, "allowed rates not read");
    };
    let resampled: Vec<u32> = dac
        .iter()
        .copied()
        .filter(|rate| !allowed.contains(rate))
        .collect();
    if resampled.is_empty() {
        let words = match dac.as_slice() {
            [only] => format!("{} kHz allowed", super::sample::khz(*only)),
            [first, .., last] => format!(
                "{}-{} kHz all allowed",
                super::sample::khz(*first),
                super::sample::khz(*last)
            ),
            [] => String::new(),
        };
        return line("rates", SetupOutcome::Pass, words);
    }
    line(
        "rates",
        SetupOutcome::Fail,
        format!(
            "{} kHz resampled (allowed: {})",
            khz_list(&resampled),
            khz_list(&allowed)
        ),
    )
}

fn forced_rate_line(setup: &SetupFacts) -> SetupLine {
    match setup.clock.as_ref() {
        None => line("forced_rate", SetupOutcome::Unknown, "settings not read"),
        Some(clock) => match clock.force_rate {
            None => line("forced_rate", SetupOutcome::Pass, "none"),
            Some(rate) => line(
                "forced_rate",
                SetupOutcome::Fail,
                format!(
                    "{} kHz forced: every other rate is resampled",
                    super::sample::khz(rate)
                ),
            ),
        },
    }
}

fn dither_line(setup: &SetupFacts) -> SetupLine {
    let Some(dither) = &setup.dither else {
        return line("dither", SetupOutcome::Unknown, "not read");
    };
    let mut found = Vec::new();
    if !matches!(dither.method.as_str(), "" | "none") {
        found.push(dither.method.clone());
    }
    if dither.noise != 0 {
        found.push(format!("noise {}", dither.noise));
    }
    if found.is_empty() {
        line("dither", SetupOutcome::Pass, "off")
    } else {
        line("dither", SetupOutcome::Fail, found.join(", "))
    }
}

/// The 24-bit line: the format in use (the negotiated one while it plays, else the forced one,
/// else the offer's default) judged at 24 bits or more, with the widest the DAC takes named (in
/// PipeWire's spelling) when the one in use is narrower.
fn format_line(sink: &SinkFacts, setup: &SetupFacts) -> SetupLine {
    let playing = sink.format.is_some();
    let used = sink
        .format
        .clone()
        .or_else(|| setup.forced_format.clone())
        .or_else(|| {
            setup
                .offer
                .as_ref()
                .and_then(|o| o.formats.first().cloned())
        });
    let Some(used) = used else {
        return line("format", SetupOutcome::Unknown, "format not read");
    };
    let Some(read) = sample_of(&used) else {
        return line(
            "format",
            SetupOutcome::Unknown,
            format!("format {used} not judged"),
        );
    };
    let name = if playing {
        format!("{used} in use")
    } else {
        format!("{used} (when it plays)")
    };
    let wide = match read {
        Sample::Float => true,
        Sample::Int { bits, .. } => bits >= 24,
    };
    if wide {
        return line("format", SetupOutcome::Pass, name);
    }
    let kernel = setup
        .alsa
        .as_ref()
        .and_then(|alsa| alsa.kernel.as_ref())
        .map(|kernel| kernel.formats.clone())
        .filter(|formats| !formats.is_empty());
    let offered = kernel.or_else(|| setup.offer.as_ref().map(|offer| offer.formats.clone()));
    let mut widest: Option<(u32, String)> = None;
    for format in offered.unwrap_or_default() {
        let bits = match sample_of(&format) {
            Some(Sample::Float) => 33,
            Some(Sample::Int { bits, .. }) if bits >= 24 => bits,
            _ => continue,
        };
        if widest.as_ref().is_none_or(|(best, _)| bits > *best) {
            widest = Some((bits, format));
        }
    }
    match widest.map(|(_, format)| format) {
        Some(format) => line(
            "format",
            SetupOutcome::Fail,
            format!("{name}, {} available", pipewire_name(&format)),
        ),
        None => line("format", SetupOutcome::Fail, name),
    }
}

/// The ALSA line: `dmix` always fails (other ALSA clients can share the card); what the card's
/// `hw_params` show of a conversion when they are open, a format they cannot read being unknown
/// (the device name still fails a `plug`); otherwise what the device name says.
fn plug_line(sink: &SinkFacts, setup: &SetupFacts) -> SetupLine {
    let Some(alsa) = &setup.alsa else {
        return line("plug", SetupOutcome::Pass, "not an ALSA sink");
    };
    let path = alsa.path.as_deref();
    let kind = path.map(alsa_name);
    let mut unread = None;
    if let HwState::Open(hw) = &alsa.hw {
        let channels = setup.sink_layout.as_ref().map(|layout| layout.channels);
        match hw_judge(sink, channels, hw) {
            HwJudge::Differs(parts) => {
                return line(
                    "plug",
                    SetupOutcome::Fail,
                    format!("ALSA converts {}", parts.join(", ")),
                );
            }
            HwJudge::Same if kind != Some(AlsaName::Dmix) => {
                let words = match path {
                    Some(path) if kind == Some(AlsaName::Plug) => {
                        format!("{path}, the card gets the sink's format")
                    }
                    Some(path) => path.to_string(),
                    None => "the card gets the sink's format".to_string(),
                };
                return line("plug", SetupOutcome::Pass, words);
            }
            HwJudge::Unreadable(words) => unread = Some(words),
            _ => {}
        }
    }
    let Some(path) = path else {
        return line("plug", SetupOutcome::Unknown, "ALSA device not read");
    };
    match alsa_name(path) {
        AlsaName::Plug => line("plug", SetupOutcome::Fail, format!("{path} converts")),
        AlsaName::Dmix => line("plug", SetupOutcome::Fail, format!("{path} mixes")),
        _ if unread.is_some() => line("plug", SetupOutcome::Unknown, unread.unwrap_or_default()),
        AlsaName::Direct => line("plug", SetupOutcome::Pass, path),
        AlsaName::Other => line("plug", SetupOutcome::Unknown, format!("{path} not judged")),
    }
}

/// The stereo line: the sink has two channels (what it will have while it plays, when idle).
fn stereo_line(setup: &SetupFacts) -> SetupLine {
    let (layout, idle) = match (&setup.sink_layout, setup.offer.as_ref()) {
        (Some(layout), _) => (layout, false),
        (None, Some(offer)) => match &offer.layout {
            Some(layout) => (layout, true),
            None => return line("stereo", SetupOutcome::Unknown, "sink channels not read"),
        },
        (None, None) => return line("stereo", SetupOutcome::Unknown, "sink channels not read"),
    };
    let later = if idle { " (when it plays)" } else { "" };
    if layout.channels == 2 {
        let words = if layout.positions.is_empty() {
            "2 channels".to_string()
        } else {
            layout.positions.join(" ")
        };
        return line("stereo", SetupOutcome::Pass, format!("{words}{later}"));
    }
    let plural = if layout.channels == 1 { "" } else { "s" };
    line(
        "stereo",
        SetupOutcome::Fail,
        format!("the sink has {} channel{plural}{later}", layout.channels),
    )
}

fn filter_line(setup: &SetupFacts) -> SetupLine {
    match setup.sink_filter {
        None => line("filter", SetupOutcome::Unknown, "not read"),
        Some(false) => line("filter", SetupOutcome::Pass, "none"),
        Some(true) => line("filter", SetupOutcome::Fail, "a filter inside the sink"),
    }
}

#[cfg(test)]
mod tests;
