// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The index text (`results/INDEX.md`): one line per measurement, parsed into [`Entry`]s, and
//! the search over them.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

/// The characters left as they are in a URL path component: the unreserved set. Everything else,
/// `/`, `?`, `#`, `%` and `&` among them, is encoded, so one name is always one component.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The forms a measurement folder ends in.
const FORMS: [&str; 3] = ["over-ear", "in-ear", "earbud"];

/// One measurement of one headphone: a line of the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The headphone's name, as AutoEq writes it.
    pub name: String,
    /// Who measured it (`oratory1990`, `crinacle`, ...).
    pub source: String,
    /// `over-ear`, `in-ear` or `earbud`.
    pub form: String,
    /// The measurement rig the folder names in front of the form, when it does.
    pub rig: Option<String>,
}

impl Entry {
    /// What a preset made from this entry remembers about its origin:
    /// `AutoEq · <source> · <form>[ · <rig>]`.
    pub fn origin(&self) -> String {
        match &self.rig {
            Some(rig) => format!("AutoEq · {} · {} · {rig}", self.source, self.form),
            None => format!("AutoEq · {} · {}", self.source, self.form),
        }
    }

    /// The URL of this entry's `ParametricEQ.txt` under `base` (the `results/` directory):
    /// `<base><source>/<form-rig>/<name>/<name> ParametricEQ.txt`, each component
    /// percent-encoded.
    pub fn peq_url(&self, base: &str) -> String {
        let folder = match &self.rig {
            Some(rig) => format!("{rig} {}", self.form),
            None => self.form.clone(),
        };
        let file = format!("{} ParametricEQ.txt", self.name);
        let mut url = base.to_string();
        if !url.ends_with('/') {
            url.push('/');
        }
        for (i, component) in [&self.source, &folder, &self.name, &file]
            .into_iter()
            .enumerate()
        {
            if i > 0 {
                url.push('/');
            }
            url.push_str(&encode(component));
        }
        url
    }
}

/// `component` as one URL path component. A component made of dots only is written with its dots
/// encoded, so it never reads as a `.` or `..` step.
fn encode(component: &str) -> String {
    let encoded = utf8_percent_encode(component, COMPONENT).to_string();
    if !encoded.is_empty() && encoded.bytes().all(|b| b == b'.') {
        return encoded.replace('.', "%2E");
    }
    encoded
}

/// Every entry of the index `text`, in the file's order. A line that is not an entry, or whose
/// path does not have the shape of one, is skipped.
///
/// A line reads `- [<name>](./<source>/<form-rig>/<name>) by <source>[ on <rig>]`. The link
/// target holds literal parentheses, so it is matched with balanced ones; each of its three
/// components is percent-decoded on its own.
pub fn parse_index(text: &str) -> Vec<Entry> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Entry> {
    let rest = line.trim_end().strip_prefix("- [")?;
    let (_label, rest) = rest.split_once("](./")?;
    let mut depth = 0usize;
    let mut end = None;
    for (at, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => {
                end = Some(at);
                break;
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    let end = end?;
    rest[end + 1..].strip_prefix(" by ")?;
    let mut parts = rest[..end].split('/');
    let (source, folder, name) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let decode = |part: &str| -> Option<String> {
        let text = percent_decode_str(part).decode_utf8().ok()?.into_owned();
        let usable = !text.trim().is_empty()
            && text != "."
            && text != ".."
            && !text.chars().any(char::is_control);
        usable.then_some(text)
    };
    let (source, folder, name) = (decode(source)?, decode(folder)?, decode(name)?);
    let (form, rig) = split_folder(&folder)?;
    Some(Entry {
        name,
        source,
        form,
        rig,
    })
}

/// A form folder (`over-ear`, `GRAS 43AG-7 over-ear`) as its form and its rig.
fn split_folder(folder: &str) -> Option<(String, Option<String>)> {
    for form in FORMS {
        if folder == form {
            return Some((form.to_string(), None));
        }
        if let Some(rig) = folder.strip_suffix(form).and_then(|r| r.strip_suffix(' '))
            && !rig.trim().is_empty()
        {
            return Some((form.to_string(), Some(rig.trim().to_string())));
        }
    }
    None
}

/// `text` as lowercase letters and digits only.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The words of `query` as lowercase letters and digits, the empty ones dropped.
fn query_words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(plain)
        .filter(|word| !word.is_empty())
        .collect()
}

/// The entries whose name is exactly `query`, compared as lowercase letters and digits, in
/// `entries`' order. A query with no letter or digit finds nothing.
pub fn exact<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let whole = query_words(query).concat();
    if whole.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|entry| plain(&entry.name) == whole)
        .collect()
}

/// Whether `word` (already plain) occurs in `name` starting where one of the name's own words
/// starts (`hd600` in `Sennheiser HD 600`, not in `Denon AH-D600`).
fn starts_a_word(name: &str, word: &str) -> bool {
    let full = plain(name);
    let mut at = 0;
    for part in name.split(|c: char| !c.is_alphanumeric()) {
        let part = plain(part);
        if full[at..].starts_with(word) && !part.is_empty() {
            return true;
        }
        at += part.len();
    }
    false
}

/// The entries whose name holds every word of `query`, compared as lowercase letters and
/// digits (`hd650` finds `HD 650`, `athm50x` finds `ATH-M50x`).
///
/// Names are ranked exact match first, then names starting with the query, then names where
/// the query's first word starts one of the name's words, then the rest, each tier
/// alphabetically; the entries of one name stay together in `entries`' own order, which is
/// AutoEq's priority, so the first of each name is the recommended one. A query with no letter
/// or digit finds nothing.
pub fn search<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let words = query_words(query);
    if words.is_empty() {
        return Vec::new();
    }
    let whole = words.concat();
    let mut found: Vec<(u8, String, &Entry)> = entries
        .iter()
        .filter_map(|entry| {
            let name = plain(&entry.name);
            if !words.iter().all(|word| name.contains(word.as_str())) {
                return None;
            }
            let rank = if name == whole {
                0
            } else if name.starts_with(&whole) {
                1
            } else if starts_a_word(&entry.name, &words[0]) {
                2
            } else {
                3
            };
            Some((rank, entry.name.to_lowercase(), entry))
        })
        .collect();
    found.sort_by(|a, b| (a.0, &a.1, &a.2.name).cmp(&(b.0, &b.1, &b.2.name)));
    found.into_iter().map(|(_, _, entry)| entry).collect()
}

#[cfg(test)]
mod tests;
