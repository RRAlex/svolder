// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a picture says as text: a still's twin, the animation's twin, and the guard that refuses
//! a frame carrying the home directory, the user or host name, or an absolute path. The twins
//! are committed beside the pictures, so the privacy checks, which cannot read pixels, read
//! every drawn character there.

use std::collections::HashSet;

use ratatui::buffer::Buffer;

use crate::tui::test_support::text;

/// The one path a frame may carry: the configuration file, written from `~`.
pub(super) const PINNED_PATH: &str = "~/.config/svolder/config.toml";

/// `buffer` as text: one line per row, trailing blanks trimmed, each ended by a newline.
pub(super) fn still_twin(buffer: &Buffer) -> String {
    text(buffer)
        .iter()
        .map(|row| format!("{}\n", row.trim_end()))
        .collect()
}

/// Every distinct line the animation drew, in the order first drawn. Braille cells are blank, so
/// the sparks and curves that change every frame do not each make a line of their own: braille
/// spells nothing a privacy check could match.
#[derive(Debug, Default)]
pub(super) struct AnimationTwin {
    seen: HashSet<String>,
    lines: Vec<String>,
}

impl AnimationTwin {
    /// Take every line of `buffer` not seen before.
    pub(super) fn add(&mut self, buffer: &Buffer) {
        for row in text(buffer) {
            let line: String = row
                .chars()
                .map(|c| match ('\u{2800}'..='\u{28ff}').contains(&c) {
                    true => ' ',
                    false => c,
                })
                .collect();
            let line = line.trim_end().to_string();
            if !line.is_empty() && self.seen.insert(line.clone()) {
                self.lines.push(line);
            }
        }
    }

    /// The lines, each ended by a newline.
    pub(super) fn text(&self) -> String {
        self.lines.iter().map(|line| format!("{line}\n")).collect()
    }
}

/// The details of this machine a frame must not carry.
#[derive(Debug, Clone, Default)]
pub(super) struct HostDetails {
    home: Option<String>,
    user: Option<String>,
    hostname: Option<String>,
}

impl HostDetails {
    /// This machine's: `$HOME`, `$USER` and the kernel's host name.
    pub(super) fn of_this_machine() -> HostDetails {
        HostDetails::new(
            std::env::var("HOME").ok(),
            std::env::var("USER").ok(),
            std::fs::read_to_string("/proc/sys/kernel/hostname").ok(),
        )
    }

    /// The details given, trimmed; a home under 2 characters, a user under 3 and a host name
    /// under 2 are not looked for, as they would match ordinary text.
    pub(super) fn new(
        home: Option<String>,
        user: Option<String>,
        hostname: Option<String>,
    ) -> HostDetails {
        let keep = |value: Option<String>, min: usize| {
            value
                .map(|value| value.trim().to_string())
                .filter(|value| value.chars().count() >= min)
        };
        HostDetails {
            home: keep(home, 2),
            user: keep(user, 3),
            hostname: keep(hostname, 2),
        }
    }
}

/// Refuse `buffer`, drawn for `place` (a scene, and a frame where there are several), when a
/// line of it carries a detail of this machine; the error names the place, the line and why.
pub(super) fn guard(buffer: &Buffer, host: &HostDetails, place: &str) -> Result<(), String> {
    for (y, row) in text(buffer).iter().enumerate() {
        if let Some(why) = refused(row, host) {
            return Err(format!(
                "{place}, line {}: {why}: {}",
                y + 1,
                row.trim_end()
            ));
        }
    }
    Ok(())
}

/// Why `line` is refused, or `None`: the home directory anywhere in it, the user name or the host
/// name as a whole word (not inside a longer run of letters and digits), all in any case, or an
/// absolute path other than [`PINNED_PATH`].
pub(super) fn refused(line: &str, host: &HostDetails) -> Option<&'static str> {
    let lower = line.to_lowercase();
    let holds = |detail: &Option<String>| {
        detail
            .as_ref()
            .is_some_and(|detail| lower.contains(&detail.to_lowercase()))
    };
    let holds_word = |detail: &Option<String>| {
        detail
            .as_ref()
            .is_some_and(|detail| has_word(&lower, &detail.to_lowercase()))
    };
    if holds(&host.home) {
        return Some("the home directory");
    }
    if holds_word(&host.user) {
        return Some("the user name");
    }
    if holds_word(&host.hostname) {
        return Some("the host name");
    }
    absolute_path(&line.replace(PINNED_PATH, "")).then_some("an absolute path")
}

/// Whether `word` occurs in `line` with no ASCII letter or digit right before or after it.
fn has_word(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_alphanumeric())
            && !after.is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

/// Whether `line` holds a `/` that starts a word (the line's start, a blank, or one of
/// `( [ " ' = :` before it) and is followed by a path character. `L/R`, `10/s` and `109/127`
/// start no word.
fn absolute_path(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        c == '/'
            && (i == 0 || chars[i - 1].is_whitespace() || "([\"'=:".contains(chars[i - 1]))
            && chars
                .get(i + 1)
                .is_some_and(|&next| next.is_ascii_alphanumeric() || ".-_~".contains(next))
    })
}
