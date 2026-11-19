// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the judges need to read of a sample format name and an ALSA device name: shared by the
//! path verdict and the setup check, so both say the same thing of the same facts.

use svolder_host::{HwParams, SinkFacts};

/// A sample format as the judges read it: an integer of some width, or a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sample {
    /// An integer format of `bits` bits, signed or not.
    Int { signed: bool, bits: u32 },
    /// A 32- or 64-bit float.
    Float,
}

impl Sample {
    /// The integer width the judges count, up to the 24 bits PipeWire's float mix holds; `None`
    /// for a float.
    pub(super) fn counted_bits(self) -> Option<u32> {
        match self {
            Self::Int { bits, .. } => Some(bits.min(24)),
            Self::Float => None,
        }
    }
}

/// The name of `name` without its endian (`LE`, `BE`, ALSA's `_LE`) and planar (`P`) suffix:
/// `S32LE` and `S32P` are `S32`, ALSA's `S24_3LE` is `S24_3`.
pub(super) fn base_name(name: &str) -> &str {
    let name = name
        .strip_suffix("LE")
        .or_else(|| name.strip_suffix("BE"))
        .unwrap_or(name);
    let name = name.strip_suffix('_').unwrap_or(name);
    name.strip_suffix('P').unwrap_or(name)
}

/// The format `name` names, in PipeWire's or ALSA's spelling; `None` for a name the judges do
/// not know (`S8X`, `DSD`).
pub(super) fn sample_of(name: &str) -> Option<Sample> {
    let base = base_name(name);
    let kind = base.chars().next()?;
    let rest = &base[kind.len_utf8()..];
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (number, tail) = rest.split_at(digits);
    let bits: u32 = number.parse().ok()?;
    let tail_ok = tail.is_empty()
        || tail
            .strip_prefix('_')
            .is_some_and(|more| !more.is_empty() && more.chars().all(|c| c.is_ascii_digit()));
    if !tail_ok {
        return None;
    }
    match (kind, bits) {
        ('F', 32 | 64) if tail.is_empty() => Some(Sample::Float),
        ('S' | 'U', 8 | 16 | 20 | 24 | 32) => Some(Sample::Int {
            signed: kind == 'S',
            bits,
        }),
        _ => None,
    }
}

/// What an ALSA device name says of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AlsaName {
    /// `plug:` or `plughw:`: ALSA converts whatever it is given.
    Plug,
    /// `dmix`: ALSA mixes the players into one stream.
    Dmix,
    /// `hw:`, `front:`, `hdmi:`, `iec958:`: the device as it is.
    Direct,
    /// Any other name (`default`, `sysdefault`, a PCM of the user's own).
    Other,
}

/// What the device name `path` says of the device.
pub(super) fn alsa_name(path: &str) -> AlsaName {
    let prefix = path.split(':').next().unwrap_or(path).to_lowercase();
    match prefix.as_str() {
        "plug" | "plughw" => AlsaName::Plug,
        "dmix" => AlsaName::Dmix,
        "hw" | "front" | "hdmi" | "iec958" => AlsaName::Direct,
        _ => AlsaName::Other,
    }
}

/// What the card's `hw_params` say against the sink's negotiated format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HwJudge {
    /// The sink has none of format, rate and channels to compare.
    NotComparable,
    /// What the sink says of all three agrees with the card.
    Same,
    /// What differs, as the parts of a sentence (`S32LE → S16_LE`, `48 → 44.1 kHz`,
    /// `channels 2 → 6`).
    Differs(Vec<String>),
    /// Nothing else differs, but a format name is one the judges cannot read: the card's (said
    /// `ALSA format X not judged`) or the sink's (`format X not judged`). Equal is never assumed.
    Unreadable(String),
}

/// Compares the sink's negotiated format, rate and channels with the card's `hw_params`.
pub(super) fn hw_judge(sink: &SinkFacts, sink_channels: Option<u32>, hw: &HwParams) -> HwJudge {
    let mut compared = false;
    let mut parts = Vec::new();
    let mut unreadable = None;
    if let Some(format) = sink.format.as_deref() {
        compared = true;
        match (sample_of(format), sample_of(&hw.format)) {
            (Some(ours), Some(theirs)) => {
                if ours != theirs {
                    parts.push(format!("{format} → {}", hw.format));
                }
            }
            (Some(_), None) => unreadable = Some(unreadable_words(&hw.format)),
            (None, _) => unreadable = Some(format!("format {format} not judged")),
        }
    }
    if let Some(rate) = sink.rate_hz {
        compared = true;
        if rate != hw.rate {
            parts.push(format!("{} → {} kHz", khz(rate), khz(hw.rate)));
        }
    }
    if let Some(channels) = sink_channels {
        compared = true;
        if channels != hw.channels {
            parts.push(format!("channels {channels} → {}", hw.channels));
        }
    }
    match (compared, parts.is_empty(), unreadable) {
        (false, ..) => HwJudge::NotComparable,
        (true, false, _) => HwJudge::Differs(parts),
        (true, true, Some(name)) => HwJudge::Unreadable(name),
        (true, true, None) => HwJudge::Same,
    }
}

/// The words for a format name the judges cannot read: `ALSA format FLOAT_LE not judged`, or
/// `ALSA format not read` for none.
pub(super) fn unreadable_words(name: &str) -> String {
    if name.is_empty() {
        "ALSA format not read".to_string()
    } else {
        format!("ALSA format {name} not judged")
    }
}

/// A rate in kilohertz: `44.1`, `48`, `176.4`; a decimal only when the rate needs one.
pub(super) fn khz(rate_hz: u32) -> String {
    let (whole, rest) = (rate_hz / 1000, rate_hz % 1000);
    if rest == 0 {
        return whole.to_string();
    }
    let decimals = format!("{rest:03}");
    format!("{whole}.{}", decimals.trim_end_matches('0'))
}

/// `name` as PipeWire spells it: ALSA's `S32_LE` is `S32LE`, its `S24_LE` (24 bits in 32) is
/// `S24_32LE` and its packed `S24_3LE` is `S24LE`.
pub(super) fn pipewire_name(name: &str) -> String {
    for suffix in ["LE", "BE"] {
        let Some(head) = name.strip_suffix(suffix) else {
            continue;
        };
        return match head {
            "S24_3" => format!("S24{suffix}"),
            "S24_" => format!("S24_32{suffix}"),
            _ => match head.strip_suffix('_') {
                Some(head) => format!("{head}{suffix}"),
                None => name.to_string(),
            },
        };
    }
    name.to_string()
}
