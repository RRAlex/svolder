// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row: one model every front end draws. [`path_row`] turns a judged [`PathLine`] and
//! the config check of its sink into the label's state and words, each hop's words with their
//! tone, and the values line, so no front end re-judges, diffs texts or picks a colour itself.

use super::path::{DacStage, Hop, PathLine, ROUTE_NOT_READ, Verdict};
use super::sample::khz;
use super::setup::{SetupCheck, SetupSummary};
use serde::Serialize;

/// How a word of the row is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// Normal text.
    Plain,
    /// Quiet text: shared values and what could not be read.
    Dim,
    /// Green: the label of a path that is untouched end to end.
    Good,
    /// Yellow: something the judge named as a change.
    Warn,
}

/// What the whole path does to the sound, the DAC's own stage included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndToEnd {
    /// The path is untouched and the DAC's stage changes nothing.
    Untouched,
    /// A hop changes the sound, or the DAC's stage does.
    Changed,
    /// The sound goes to another sink.
    NotThisDac,
    /// The DAC listens to an input the computer does not feed.
    OtherInput,
    /// Something could not be read.
    Unknown,
}

impl EndToEnd {
    /// The stable word every JSON surface carries: `untouched`, `changed`, `not_this_dac`,
    /// `other_input` or `unknown`.
    pub fn key(self) -> &'static str {
        match self {
            EndToEnd::Untouched => "untouched",
            EndToEnd::Changed => "changed",
            EndToEnd::NotThisDac => "not_this_dac",
            EndToEnd::OtherInput => "other_input",
            EndToEnd::Unknown => "unknown",
        }
    }
}

/// The row's label (`Path`): its tone and the words behind it for a tooltip or a screen reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowLabel {
    /// Good (green) only for a path untouched end to end whose config check does not fail.
    pub tone: Tone,
    /// `Path: untouched end to end`, `Path: the DAC changes the sound`, ...
    pub words: String,
}

/// One word of a hop or of the values line, with its tone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowWord {
    /// The word as drawn.
    pub text: String,
    /// How it is drawn.
    pub tone: Tone,
}

/// One hop of the row: its label and the words that say what it changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowHop {
    /// `player`, `computer` or `DAC USB 1`.
    pub label: String,
    /// The hop's changes in Warn (`resampled`), and a Dim `?` when something was not read.
    pub words: Vec<RowWord>,
}

/// The Path row of one device's screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathRow {
    /// The label's state and words.
    pub label: RowLabel,
    /// The hops, player first; empty for another sink, another input or no route.
    pub hops: Vec<RowHop>,
    /// The values line: the rate, the DAC's volume, the DAC stage's changes, `config N` on a
    /// failing config check.
    pub values: Vec<RowWord>,
    /// The short words for a row with no hops (`→ DELL S2721QS`).
    pub words: Option<String>,
}

impl PathRow {
    /// The row while the route has not been read.
    pub fn not_read() -> Self {
        PathRow {
            label: RowLabel {
                tone: Tone::Plain,
                words: format!("Path: {ROUTE_NOT_READ}"),
            },
            hops: Vec::new(),
            values: Vec::new(),
            words: Some(ROUTE_NOT_READ.to_string()),
        }
    }
}

impl PathLine {
    /// What the path does to the sound end to end: the verdict, with the DAC's own stage added.
    /// An untouched or unknown path whose DAC lowers the volume, shapes the sound with its tone
    /// control, shifts the balance, mutes, inverts the phase or uses its NOS filter has changed;
    /// the other verdicts pass through.
    pub fn end_to_end(&self) -> EndToEnd {
        match &self.verdict {
            Verdict::Untouched if self.dac_stage_changes() => EndToEnd::Changed,
            Verdict::Untouched => EndToEnd::Untouched,
            Verdict::Changed => EndToEnd::Changed,
            Verdict::NotThisDac { .. } => EndToEnd::NotThisDac,
            Verdict::OtherInput { .. } => EndToEnd::OtherInput,
            // A known change outranks something not read, as the judge's own words do.
            Verdict::Unknown if self.dac_stage_changes() => EndToEnd::Changed,
            Verdict::Unknown => EndToEnd::Unknown,
        }
    }

    /// Whether the DAC hop's own stage changes the sound.
    fn dac_stage_changes(&self) -> bool {
        self.hops
            .iter()
            .find_map(|hop| hop.stage)
            .is_some_and(|stage| stage.changes())
    }
}

/// The row for `line`, with the config check of its sink when there is one. The label is Good
/// only for a path untouched end to end whose config check does not fail, Warn when a hop or the
/// DAC's stage changes the sound or the check fails, and Plain for what could not be read, for
/// another sink and for another input.
pub fn path_row(line: &PathLine, setup: Option<&SetupCheck>) -> PathRow {
    if line.hops.is_empty() {
        return PathRow {
            label: RowLabel {
                tone: Tone::Plain,
                words: format!("Path: {}", elsewhere_words(line)),
            },
            hops: Vec::new(),
            values: Vec::new(),
            words: Some(line.words_line()),
        };
    }
    let fails = match setup.map(|check| check.summary) {
        Some(SetupSummary::Changes(fails)) => Some(fails),
        _ => None,
    };
    PathRow {
        label: label(line, fails.is_some()),
        hops: line.hops.iter().map(row_hop).collect(),
        values: values(line, fails),
        words: None,
    }
}

/// The words of a path with no hops, for the label: the sentence of a sound that goes elsewhere,
/// what the DAC listens to, or what was not read.
fn elsewhere_words(line: &PathLine) -> String {
    match &line.verdict {
        Verdict::NotThisDac { .. } => line.sentence().unwrap_or_else(|| line.words_line()),
        _ => line.words.join("; "),
    }
}

/// The label of a row with hops.
fn label(line: &PathLine, config_fails: bool) -> RowLabel {
    let end_to_end = line.end_to_end();
    let (tone, words) = match end_to_end {
        EndToEnd::Changed => (Tone::Warn, changing_hops(line)),
        _ if config_fails => (Tone::Warn, "the PipeWire config check failed".to_string()),
        EndToEnd::Untouched => (Tone::Good, "untouched end to end".to_string()),
        _ => (Tone::Plain, "not everything could be read".to_string()),
    };
    RowLabel {
        tone,
        words: format!("Path: {words}"),
    }
}

/// `the DAC changes the sound`, `the computer and the DAC change the sound`: the hops that
/// change it, joined `A, B and C`.
fn changing_hops(line: &PathLine) -> String {
    let names: Vec<String> = line
        .hops
        .iter()
        .filter(|hop| hop.changes)
        .map(|hop| format!("the {}", hop.label))
        .collect();
    let (verb, who) = match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            ("change", format!("{} and {last}", rest.join(", ")))
        }
        Some((only, _)) => ("changes", only.clone()),
        None => ("changes", "the path".to_string()),
    };
    format!("{who} {verb} the sound")
}

/// A hop as a row shows it: its label (the DAC's with its input) and its changes in Warn, each
/// short form once, then a Dim `?` when something of it was not read.
fn row_hop(hop: &Hop) -> RowHop {
    let label = match (&hop.input, hop.stage.is_some()) {
        (Some(input), true) => format!("{} {input}", hop.label),
        _ => hop.label.clone(),
    };
    let mut words: Vec<RowWord> = Vec::new();
    for change in &hop.changed {
        if !words.iter().any(|word| word.text == change.short) {
            words.push(RowWord {
                text: change.short.clone(),
                tone: Tone::Warn,
            });
        }
    }
    if hop.unknown {
        words.push(RowWord {
            text: "?".to_string(),
            tone: Tone::Dim,
        });
    }
    RowHop { label, words }
}

fn word(text: impl Into<String>, tone: Tone) -> RowWord {
    RowWord {
        text: text.into(),
        tone,
    }
}

/// The values line: the rate once (or `from → to kHz` when the computer resamples), the DAC's
/// volume in dB, the DAC stage's other changes, and `config N` on a failing config check.
fn values(line: &PathLine, config_fails: Option<u32>) -> Vec<RowWord> {
    let mut values = Vec::new();
    let (player, computer, dac) = (line.hops.first(), line.hops.get(1), line.hops.get(2));
    let rate = match (
        player.and_then(|hop| hop.rate_hz),
        computer.and_then(|hop| hop.resamples_to),
    ) {
        (Some(from), Some(to)) => Some(format!("{} → {} kHz", khz(from), khz(to))),
        _ => computer
            .and_then(|hop| hop.rate_hz)
            .or_else(|| player.and_then(|hop| hop.rate_hz))
            .or_else(|| dac.and_then(|hop| hop.rate_hz))
            .map(|rate| format!("{} kHz", khz(rate))),
    };
    values.extend(rate.map(|rate| word(rate, Tone::Dim)));
    if let Some(stage) = dac.and_then(|hop| hop.stage) {
        if let Some(db) = stage.volume_db {
            let (text, tone) = match stage.lowers() {
                Some(_) => (DacStage::volume_word(db), Tone::Warn),
                // A volume that reads 0.0 dB is said `0 dB`.
                None => ("volume 0 dB".to_string(), Tone::Dim),
            };
            values.push(word(text, tone));
        }
        values.extend(
            stage
                .other_words()
                .into_iter()
                .map(|text| word(text, Tone::Warn)),
        );
    }
    if let Some(fails) = config_fails {
        values.push(word(format!("config {fails}"), Tone::Warn));
    }
    values
}

#[cfg(test)]
mod tests;
