// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scene last recalled, and the file that keeps it across a restart.
//!
//! The file is `scenes.toml` in the state home, one line: `recalled = "<name>"`. It is written
//! whole to a temporary file in the same directory (mode 0600) and renamed over the old one, so
//! a reader never sees half a name. A file that is missing, unreadable or garbled is no memory;
//! the garbled one is said once in the log, and nothing fails.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::log;

/// What the service remembers of its last recall.
#[derive(Default)]
pub(super) struct Memory {
    /// The scene last recalled, as the config spells its name.
    pub(super) recalled: Option<String>,
    /// Where it is kept; `None` keeps it in memory alone.
    pub(super) path: Option<PathBuf>,
}

/// The scene the file at `path` names; `None` when there is none or the file cannot be used.
pub(super) fn read(path: &Path) -> Option<String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn(&format!(
                "scenes: {} could not be read, so no scene counts as recalled: {e}",
                path.display()
            ));
            return None;
        }
    };
    match parse(&text) {
        Some(recalled) => recalled,
        None => {
            log::warn(&format!(
                "scenes: {} is not what the daemon writes, so no scene counts as recalled",
                path.display()
            ));
            None
        }
    }
}

/// Keep `name` (`None` forgets it) in the file at `path`, off the async threads; a failure is
/// logged and leaves the memory in the process as it is.
pub(super) async fn keep(path: PathBuf, name: Option<String>) {
    let kept = tokio::task::spawn_blocking(move || write(&path, name.as_deref())).await;
    match kept {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::warn(&format!(
            "scenes: the recalled scene could not be kept across a restart: {e}"
        )),
        Err(_) => log::warn("scenes: the write of the recalled scene stopped"),
    }
}

/// The file's text for `name`.
fn text_of(name: &str) -> String {
    let mut quoted = String::with_capacity(name.len() + 2);
    quoted.push('"');
    for c in name.chars() {
        if c == '"' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!("recalled = {quoted}\n")
}

/// What `text` says: `Some(Some(name))`, `Some(None)` for a file that says nothing, and `None`
/// for text that is not the line the daemon writes.
fn parse(text: &str) -> Option<Option<String>> {
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value = line
            .strip_prefix("recalled")?
            .trim_start()
            .strip_prefix('=')?
            .trim();
        if found.is_some() {
            return None;
        }
        found = Some(unquoted(value)?);
    }
    Some(found)
}

/// The text a quoted TOML string holds, when it is a basic string with only the two escapes the
/// daemon writes.
fn unquoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut text = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                e @ ('"' | '\\') => text.push(e),
                _ => return None,
            },
            '"' => return None,
            c => text.push(c),
        }
    }
    Some(text)
}

/// Write the memory: `name` to the file, or, for `None`, take the file away.
fn write(path: &Path, name: Option<&str>) -> std::io::Result<()> {
    let Some(name) = name else {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    };
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("the file has no directory"))?;
    create_private_dir(dir)?;
    let temp = dir.join(format!(".scenes.toml.{}.tmp", std::process::id()));
    let result = write_new(&temp, &text_of(name)).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// `dir`, with its parents, made as a private directory when it is not there.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// A file at `path`, readable by its owner alone, holding `text`.
fn write_new(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_round_trips_through_the_file_text() {
        for name in ["Late night", "Say \"hi\"", "back\\slash", "ü and 夜"] {
            assert_eq!(
                parse(&text_of(name)),
                Some(Some(name.to_string())),
                "{name}"
            );
        }
    }

    #[test]
    fn text_that_is_not_the_daemons_line_is_garbled() {
        for text in [
            "recalled = Late night\n",
            "recalled = \"open\n",
            "recalled = \"a\" extra\n",
            "recalled = \"a\\u0041\"\n",
            "other = 1\n",
            "recalled = \"a\"\nrecalled = \"b\"\n",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
        assert_eq!(parse(""), Some(None), "an empty file says nothing");
        assert_eq!(parse("# kept\n"), Some(None));
    }
}
