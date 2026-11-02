// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the capture file never holds without `--keep-ids`: unit ids, passphrase bytes, the home
//! directory and Bluetooth addresses.
//!
//! A [`Scrub`] numbers the units it is shown in the order it sees them, and a unit becomes
//! `00000000000000NN` wherever it appears, in a record's bytes and in text alike, so a scrubbed
//! advertisement still parses. A frame loses its passphrase field and the value of every
//! function-250 message, with the checksum put right so it still decodes.

use svolder_ble::{FrameDirection, ManufacturerRecord};
use svolder_proto::{FunctionId, Header, UnitId, checksum};

/// Where a unit id sits in a manufacturer record's bytes.
const UNIT_AT: std::ops::Range<usize> = 6..14;
/// Where the encrypted passphrase sits in a manufacturer record's bytes.
const PASSPHRASE_AT: std::ops::Range<usize> = 14..18;
/// The function whose messages carry a passphrase.
const PASSPHRASE_FUNCTION: u8 = 250;

/// The scrub of one capture: what it has been shown, and whether it is to change anything.
pub(crate) struct Scrub {
    /// Whether `--keep-ids` was given: everything passes through unchanged.
    keep: bool,
    /// The units seen so far, in order; a unit's placeholder number is its place here.
    units: Vec<UnitId>,
    /// The Bluetooth addresses seen so far, in order, as upper-case text.
    addresses: Vec<String>,
    /// The home directory to turn into `~`, when there is one.
    home: Option<String>,
}

impl Scrub {
    /// A scrub that changes nothing when `keep_ids`, else one that scrubs; its home directory is
    /// `$HOME`.
    pub(crate) fn new(keep_ids: bool) -> Scrub {
        Scrub {
            keep: keep_ids,
            units: Vec::new(),
            addresses: Vec::new(),
            home: std::env::var("HOME").ok(),
        }
    }

    /// The same scrub with `home` as the directory that becomes `~`.
    #[cfg(test)]
    pub(crate) fn with_home(mut self, home: impl Into<String>) -> Scrub {
        self.home = Some(home.into());
        self
    }

    /// Whether this scrub changes nothing.
    pub(crate) fn keeps(&self) -> bool {
        self.keep
    }

    /// The placeholder for `unit`, numbered by the order the units were first shown; the unit
    /// itself when keeping.
    pub(crate) fn unit(&mut self, unit: UnitId) -> UnitId {
        if self.keep {
            return unit;
        }
        let place = match self.units.iter().position(|seen| *seen == unit) {
            Some(place) => place,
            None => {
                self.units.push(unit);
                self.units.len() - 1
            }
        };
        UnitId::new((place as u64 + 1).to_be_bytes())
    }

    /// `record` with its unit id (bytes 6..14) as the placeholder and its encrypted passphrase
    /// (bytes 14..18) zeroed; every other byte as heard. A record too short to hold the unit id
    /// loses every byte from 6 on.
    pub(crate) fn record(&mut self, record: &ManufacturerRecord) -> ManufacturerRecord {
        let mut scrubbed = record.clone();
        if self.keep {
            return scrubbed;
        }
        let len = scrubbed.bytes.len();
        if let Ok(unit) = UnitId::from_slice(record.bytes.get(UNIT_AT).unwrap_or_default()) {
            scrubbed.bytes[UNIT_AT].copy_from_slice(&self.unit(unit).bytes());
            scrubbed.bytes[PASSPHRASE_AT.start.min(len)..PASSPHRASE_AT.end.min(len)].fill(0);
        } else {
            // A record too short to hold a whole unit id loses whatever of it is there.
            scrubbed.bytes[UNIT_AT.start.min(len)..PASSPHRASE_AT.end.min(len)].fill(0);
        }
        scrubbed
    }

    /// `bytes`, a frame as sent or as received, without the passphrase.
    ///
    /// A sent frame's first four bytes are zeroed. Every frame, decoding or not, has the value of
    /// every function-250 message the length-prefixed walk finds zeroed in place; the checksum is
    /// computed again where the original matched, so a frame that decodes still decodes and
    /// differs from the original only there. A bad checksum on a frame whose passphrase value was
    /// zeroed is replaced by one that is wrong for the scrubbed bytes, since the heard one still
    /// carries the value's byte sum; the frame still fails to decode. A 250 message whose length
    /// runs past the end (a notification cut short) loses everything from its value to the end of
    /// the buffer. Where the walk finds no message at all (too short, a zero length) the bytes are
    /// left as heard.
    pub(crate) fn frame(&self, direction: FrameDirection, bytes: &[u8]) -> Vec<u8> {
        let mut frame = bytes.to_vec();
        if self.keep {
            return frame;
        }
        let skip = match direction {
            FrameDirection::Sent => {
                let field = frame.len().min(4);
                frame[..field].fill(0);
                field
            }
            FrameDirection::Received => 0,
        };
        zero_passphrase_values(&mut frame[skip..]);
        frame
    }

    /// Whether the value of a message to `function` is withheld: a passphrase function's, unless
    /// keeping.
    pub(crate) fn hides(&self, function: FunctionId) -> bool {
        !self.keep && function.first() == PASSPHRASE_FUNCTION
    }

    /// Learn every unit id that stands in `text` as a word of its own (16 hex digits, with no
    /// letter or digit against either end), so a unit a message names that nothing else showed
    /// this scrub gets its placeholder from [`Scrub::text`].
    pub(crate) fn learn_units(&mut self, text: &str) {
        if self.keep {
            return;
        }
        for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
            if let Ok(unit) = word.parse::<UnitId>()
                && word.len() == 16
            {
                self.unit(unit);
            }
        }
    }

    /// `text` with every unit id this scrub knows as its placeholder (in either case), the home
    /// directory as `~` and each Bluetooth address as a placeholder.
    pub(crate) fn text(&mut self, text: &str) -> String {
        if self.keep {
            return text.to_string();
        }
        let text = match &self.home {
            Some(home) => replace_home(text, home),
            None => text.to_string(),
        };
        let text = self.replace_addresses(&text);
        self.replace_units(&text)
    }

    /// Each known unit's hex, in either case, as its placeholder, in one pass so a placeholder is
    /// never itself replaced.
    fn replace_units(&self, text: &str) -> String {
        let lower = text.to_ascii_lowercase();
        let known: Vec<String> = self
            .units
            .iter()
            .map(|unit| unit.to_string().to_ascii_lowercase())
            .collect();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            let hit = known
                .iter()
                .position(|hex| lower[at..].starts_with(hex.as_str()));
            match hit {
                Some(place) => {
                    out.push_str(&UnitId::new((place as u64 + 1).to_be_bytes()).to_string());
                    at += known[place].len();
                }
                None => {
                    let step = text[at..].chars().next().map_or(1, char::len_utf8);
                    out.push_str(&text[at..at + step]);
                    at += step;
                }
            }
        }
        out
    }

    /// Each Bluetooth address (six `:`-separated hex pairs, or the `_`-separated ones after
    /// `dev_` in a BlueZ object path) as `00:00:00:00:00:NN`, numbered in the order the addresses
    /// were first met.
    fn replace_addresses(&mut self, text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            // BlueZ names a device in an object path as `dev_` and its address with underscores.
            let in_path = text[at..].starts_with("dev_") && is_address_at(bytes, at + 4, b'_');
            if in_path || is_address_at(bytes, at, b':') {
                let start = if in_path { at + 4 } else { at };
                let address = text[start..start + 17]
                    .replace('_', ":")
                    .to_ascii_uppercase();
                let place = match self.addresses.iter().position(|seen| *seen == address) {
                    Some(place) => place,
                    None => {
                        self.addresses.push(address);
                        self.addresses.len() - 1
                    }
                };
                match in_path {
                    true => out.push_str(&format!("dev_00_00_00_00_00_{:02X}", place + 1)),
                    false => out.push_str(&format!("00:00:00:00:00:{:02X}", place + 1)),
                }
                at = start + 17;
            } else {
                let step = text[at..].chars().next().map_or(1, char::len_utf8);
                out.push_str(&text[at..at + step]);
                at += step;
            }
        }
        out
    }
}

/// `text` with the directory `home` as `~` wherever it is a whole path or the start of one.
fn replace_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        let after = &rest[at + home.len()..];
        let mut next = after.chars();
        let name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-');
        // A full stop is part of a name only when a name character follows it.
        let ends = match next.next() {
            None => true,
            Some('.') => !next.next().is_some_and(name),
            Some(c) => !name(c),
        };
        out.push_str(&rest[..at]);
        out.push_str(if ends { "~" } else { home });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Whether six hex pairs joined by `sep` start at `at`, with no further hex digit or `sep` run
/// on either side that would make them part of something longer.
fn is_address_at(bytes: &[u8], at: usize, sep: u8) -> bool {
    let Some(window) = bytes.get(at..at + 17) else {
        return false;
    };
    let shaped = window.iter().enumerate().all(|(i, b)| match i % 3 {
        2 => *b == sep,
        _ => b.is_ascii_hexdigit(),
    });
    let hex_pair = |pair: Option<&[u8]>| {
        pair.is_some_and(|p| p.len() == 2 && p.iter().all(u8::is_ascii_hexdigit))
    };
    // A separator next to the address belongs to a longer run only when a hex pair is beside it.
    let before = at.checked_sub(1).map(|i| bytes[i]);
    let longer_before = match before {
        Some(b) if b == sep => at >= 3 && hex_pair(bytes.get(at - 3..at - 1)),
        Some(b) => b.is_ascii_hexdigit(),
        None => false,
    };
    let longer_after = match bytes.get(at + 17).copied() {
        Some(b) if b == sep => hex_pair(bytes.get(at + 18..at + 20)),
        Some(b) => b.is_ascii_hexdigit(),
        None => false,
    };
    shaped && !longer_before && !longer_after
}

/// Zero the value bytes of every function-250 message in `framed` (the length-prefixed messages
/// and the checksum after them, as a received notification is), and put the checksum right.
///
/// The walk is the decoder's: a two-byte length, then that many bytes of header, function id and
/// value, until the length is zero or runs past the end. Messages found before that point are
/// scrubbed whatever the checksum says. A 250 whose length runs past the end is a frame cut
/// short, so its value runs to the end of the buffer and the would-be checksum bytes are part of
/// it. The checksum is written again when the heard one matched; when it did not and a value was
/// zeroed, it becomes the complement of the right one, which depends on nothing the scrub took.
/// A walk that ran off the end of a frame cut short (an overrun) rewrites nothing: its last two
/// bytes are left alone.
fn zero_passphrase_values(framed: &mut [u8]) {
    let Some(end) = framed.len().checked_sub(2) else {
        return;
    };
    let matched = checksum(&framed[..end]).to_le_bytes() == framed[end..];
    let mut zeroed = false;
    let mut overran = false;
    let mut at = 0;
    while at + 2 <= end {
        let declared = usize::from(u16::from_le_bytes([framed[at], framed[at + 1]]));
        let body = at + 2;
        if declared == 0 {
            break;
        }
        let id_len = framed
            .get(body)
            .map_or(0, |b| usize::from(Header::decode(*b).id_len));
        let is_250 = id_len > 0 && framed.get(body + 1) == Some(&PASSPHRASE_FUNCTION);
        if body + declared > end {
            overran = true;
            if is_250 {
                let value = (body + 1 + id_len).min(framed.len());
                framed[value..].fill(0);
                zeroed = true;
            }
            break;
        }
        if is_250 && id_len < declared {
            framed[body + 1 + id_len..body + declared].fill(0);
            zeroed = true;
        }
        at = body + declared;
    }
    // Where the walk ran off the end, what sits in the last two bytes is a value zeroed or a
    // header, not a checksum, so nothing is written there.
    if overran {
        return;
    }
    let right = checksum(&framed[..end]);
    if matched {
        framed[end..].copy_from_slice(&right.to_le_bytes());
    } else if zeroed {
        framed[end..].copy_from_slice(&(!right).to_le_bytes());
    }
}

#[cfg(test)]
mod tests;
