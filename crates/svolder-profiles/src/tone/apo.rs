// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Equalizer APO parametric text: the format AutoEq and squig.link write a headphone
//! correction in.
//!
//! [`parse`] reads a `Preamp:` line and `Filter n: ON PK Fc 100 Hz Gain 3 dB Q 1` lines into a
//! [`Correction`]: peaking and shelf filters only. A filter of any other kind is skipped with a
//! note, a line that cannot be read refuses the file with its line number, and
//! [`validate`] holds the bounds every correction obeys wherever it came from.

use std::fmt;
use std::str::FromStr;

/// The most bytes of text [`parse`] reads.
pub const MAX_BYTES: usize = 64 * 1024;
/// The most filters a correction holds: a PipeWire `param_eq` takes 64 biquads per channel and
/// the preamp is one of them.
pub const MAX_FILTERS: usize = 63;

/// The lowest `Fc` a filter takes, in Hz.
const FC_MIN: f64 = 10.0;
/// The highest `Fc` a filter takes, in Hz: half the 48 kHz sample rate.
const FC_MAX: f64 = 24_000.0;
/// The largest gain magnitude a filter takes, in dB.
const GAIN_MAX: f64 = 30.0;
/// The largest preamp magnitude a correction takes, in dB.
const PREAMP_MAX: f64 = 30.0;
/// The highest Q a filter takes.
const Q_MAX: f64 = 20.0;

/// The kind of one filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FilterKind {
    /// A peaking filter: a bump or a notch around `f`.
    Peaking,
    /// A low shelf: gain below `f`.
    LowShelf,
    /// A high shelf: gain above `f`.
    HighShelf,
}

impl FilterKind {
    /// The keyword Equalizer APO writes it as: `PK`, `LSC` or `HSC`.
    pub fn code(self) -> &'static str {
        match self {
            FilterKind::Peaking => "PK",
            FilterKind::LowShelf => "LSC",
            FilterKind::HighShelf => "HSC",
        }
    }
}

impl FromStr for FilterKind {
    type Err = ApoError;

    /// `PK`, `PEQ` or `Modal` for a peaking filter, `LSC` or `LS` for a low shelf and `HSC` or
    /// `HS` for a high shelf, in any case.
    fn from_str(word: &str) -> Result<Self, ApoError> {
        match word.to_ascii_lowercase().as_str() {
            "pk" | "peq" | "modal" => Ok(FilterKind::Peaking),
            "lsc" | "ls" => Ok(FilterKind::LowShelf),
            "hsc" | "hs" => Ok(FilterKind::HighShelf),
            _ => Err(ApoError::bare(format!("{word} is not a filter type"))),
        }
    }
}

/// One filter of a correction, in physical units.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceFilter {
    /// What kind of filter it is.
    pub kind: FilterKind,
    /// Centre or corner frequency in Hz.
    pub f: f64,
    /// Gain in dB.
    pub g: f64,
    /// Q.
    pub q: f64,
}

/// A headphone correction: a preamp and the filters it is made of.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Correction {
    /// The preamp in dB, 0 when the file gives none.
    pub preamp_db: f64,
    /// The filters, in file order.
    pub filters: Vec<SourceFilter>,
}

/// A correction read from text, and what was skipped on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The correction.
    pub correction: Correction,
    /// One note per filter that was skipped.
    pub notes: Vec<String>,
}

/// Why text or a correction was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApoError {
    line: Option<usize>,
    what: String,
}

impl ApoError {
    fn at(line: usize, what: impl Into<String>) -> Self {
        ApoError {
            line: Some(line),
            what: what.into(),
        }
    }

    fn bare(what: impl Into<String>) -> Self {
        ApoError {
            line: None,
            what: what.into(),
        }
    }

    /// The line the refusal names, counted from 1, when it names one.
    pub fn line(&self) -> Option<usize> {
        self.line
    }
}

impl fmt::Display for ApoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.what),
            None => f.write_str(&self.what),
        }
    }
}

impl std::error::Error for ApoError {}

/// Reads Equalizer APO parametric text.
///
/// An optional UTF-8 BOM, LF or CRLF line ends, blank lines, `#` comments, keywords in any case
/// and `Filter n: OFF` lines (skipped silently) are all accepted. `Channel:` naming anything
/// but `all`, `GraphicEQ:`, `Include:`, `Device:`, a second `Preamp:`, a line that cannot be read
/// and a value out of [`validate`]'s bounds refuse the text, naming the line. A filter that is
/// neither peaking nor a shelf with a Q is skipped with a note.
pub fn parse(text: &str) -> Result<Parsed, ApoError> {
    if text.len() > MAX_BYTES {
        return Err(ApoError::bare(format!(
            "the text is {} bytes, more than the {MAX_BYTES} read",
            text.len()
        )));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut preamp: Option<f64> = None;
    let mut filters = Vec::new();
    let mut notes = Vec::new();
    let mut seen = 0usize;
    for (index, raw_line) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let Some((head, body)) = lower.split_once(':') else {
            return Err(ApoError::at(number, format!("cannot read {line:?}")));
        };
        let head = head.trim();
        let body = body.trim();
        match head {
            "preamp" => {
                if preamp.is_some() {
                    return Err(ApoError::at(number, "a second Preamp line"));
                }
                let value = preamp_value(body).ok_or_else(|| {
                    ApoError::at(number, format!("cannot read the preamp {body:?}"))
                })?;
                if value.abs() > PREAMP_MAX {
                    return Err(ApoError::at(
                        number,
                        format!("Preamp {value} dB is outside -30 to +30 dB"),
                    ));
                }
                preamp = Some(value);
            }
            "channel" => {
                if body != "all" {
                    return Err(ApoError::at(
                        number,
                        format!("Channel {body:?} is not read (only all)"),
                    ));
                }
            }
            "graphiceq" => return Err(ApoError::at(number, "GraphicEQ is not read")),
            "include" => return Err(ApoError::at(number, "Include is not read")),
            "device" => return Err(ApoError::at(number, "Device is not read")),
            _ => {
                let Some(written) = filter_number(head) else {
                    return Err(ApoError::at(number, format!("cannot read {line:?}")));
                };
                seen += 1;
                let label = written.unwrap_or(seen);
                match filter(number, label, body)? {
                    Line::Off => {}
                    Line::Skipped(note) => notes.push(note),
                    Line::Kept(kept) => {
                        if filters.len() == MAX_FILTERS {
                            return Err(ApoError::at(
                                number,
                                format!("more than {MAX_FILTERS} filters"),
                            ));
                        }
                        filters.push(kept);
                    }
                }
            }
        }
    }
    if filters.is_empty() {
        return Err(ApoError::bare("no filter to read"));
    }
    Ok(Parsed {
        correction: Correction {
            preamp_db: preamp.unwrap_or(0.0) + 0.0,
            filters,
        },
        notes,
    })
}

/// Whether `correction` obeys the bounds the parser holds: 1 to [`MAX_FILTERS`] filters, a
/// preamp within ±30 dB, `f` from 10 Hz to 24 kHz, `g` within ±30 dB and `q` above 0 and at
/// most 20.
///
/// The error names the filter by its place in the list, counted from 1.
pub fn validate(correction: &Correction) -> Result<(), ApoError> {
    if !correction.preamp_db.is_finite() {
        return Err(ApoError::bare("the preamp is not a number"));
    }
    if correction.preamp_db.abs() > PREAMP_MAX {
        return Err(ApoError::bare(format!(
            "Preamp {} dB is outside -30 to +30 dB",
            correction.preamp_db
        )));
    }
    if correction.filters.is_empty() {
        return Err(ApoError::bare("no filter"));
    }
    if correction.filters.len() > MAX_FILTERS {
        return Err(ApoError::bare(format!(
            "{} filters, more than {MAX_FILTERS}",
            correction.filters.len()
        )));
    }
    for (index, f) in correction.filters.iter().enumerate() {
        bounds(f.f, f.g, f.q)
            .map_err(|what| ApoError::bare(format!("filter {}: {what}", index + 1)))?;
    }
    Ok(())
}

/// The bound a filter's values break, worded for a message, if any.
fn bounds(f: f64, g: f64, q: f64) -> Result<(), String> {
    if !(f.is_finite() && (FC_MIN..=FC_MAX).contains(&f)) {
        return Err(format!("Fc {f} Hz is outside 10 Hz to 24 kHz"));
    }
    if !(g.is_finite() && g.abs() <= GAIN_MAX) {
        return Err(format!("Gain {g} dB is outside -30 to +30 dB"));
    }
    if !(q.is_finite() && q > 0.0 && q <= Q_MAX) {
        return Err(format!("Q {q} is outside 0 (excluded) to 20"));
    }
    Ok(())
}

/// What one filter line came to.
enum Line {
    Off,
    Skipped(String),
    Kept(SourceFilter),
}

/// The number in a `filter 10` head: `Some(None)` for a bare `filter`, `None` when the head is
/// not a filter's at all.
fn filter_number(head: &str) -> Option<Option<usize>> {
    let rest = head.strip_prefix("filter")?.trim();
    match rest.is_empty() {
        true => Some(None),
        false => rest.parse().ok().map(Some),
    }
}

/// A preamp's value from the text after `Preamp:`: a number, then `dB` alone or attached.
fn preamp_value(body: &str) -> Option<f64> {
    let mut words = body.split_whitespace();
    let value = read_number(words.next()?, "db")?;
    match (words.next(), words.next()) {
        (None, _) | (Some("db"), None) => Some(value + 0.0),
        _ => None,
    }
}

/// A finite number from a word, with the unit it may carry attached cut off.
fn read_number(word: &str, unit: &str) -> Option<f64> {
    let digits = word.strip_suffix(unit).unwrap_or(word);
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// One filter line's body (the part after the colon, lowercased) read for line `number`.
fn filter(number: usize, label: usize, body: &str) -> Result<Line, ApoError> {
    let mut words = body.split_whitespace();
    match words.next() {
        Some("off") => return Ok(Line::Off),
        Some("on") => {}
        _ => return Err(ApoError::at(number, "a filter is ON or OFF")),
    }
    let Some(kind_word) = words.next() else {
        return Err(ApoError::at(number, "the filter has no type"));
    };
    let Ok(kind) = kind_word.parse::<FilterKind>() else {
        return Ok(Line::Skipped(format!(
            "filter {label}: {} skipped (only peaking and shelf filters are read)",
            kind_word.to_ascii_uppercase()
        )));
    };
    let code = kind_word.to_ascii_uppercase();
    // A shelf given as a slope (`12 dB`, `6dB`) has no Q.
    if kind != FilterKind::Peaking
        && words
            .clone()
            .next()
            .is_some_and(|word| read_number(word, "db").is_some())
    {
        return Ok(Line::Skipped(format!(
            "filter {label}: {code} skipped (only peaking and shelf filters are read)"
        )));
    }
    let (mut f, mut g, mut q) = (None, None, None);
    let mut bandwidth = false;
    while let Some(key) = words.next() {
        let slot = match key {
            "fc" => &mut f,
            "gain" => &mut g,
            "q" => &mut q,
            "bw" => {
                bandwidth = true;
                break;
            }
            other => {
                return Err(ApoError::at(
                    number,
                    format!("cannot read {other:?} in a filter"),
                ));
            }
        };
        let unit = if key == "fc" { "hz" } else { "db" };
        let value = words
            .next()
            .and_then(|word| read_number(word, if key == "q" { "" } else { unit }));
        let Some(value) = value else {
            return Err(ApoError::at(number, format!("{key} needs a number")));
        };
        if slot.replace(value).is_some() {
            return Err(ApoError::at(number, format!("{key} given twice")));
        }
        // A separate unit word follows its number.
        if (key != "q") && words.clone().next() == Some(unit) {
            words.next();
        }
    }
    if bandwidth {
        return Ok(Line::Skipped(format!(
            "filter {label}: {code} skipped (a BW Oct width is not read, only a Q)"
        )));
    }
    let (Some(f), Some(g)) = (f, g) else {
        return Err(ApoError::at(number, "a filter needs Fc and Gain"));
    };
    let Some(q) = q else {
        return match kind {
            FilterKind::Peaking => Err(ApoError::at(number, "a peaking filter needs a Q")),
            _ => Ok(Line::Skipped(format!(
                "filter {label}: {code} skipped (a shelf is read only with a Q)"
            ))),
        };
    };
    bounds(f, g, q).map_err(|what| ApoError::at(number, what))?;
    Ok(Line::Kept(SourceFilter {
        kind,
        f,
        g: g + 0.0,
        q,
    }))
}

#[cfg(test)]
mod tests;
