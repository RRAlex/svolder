// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a writer changes the file: read it as a TOML document, edit it, check it still
//! parses and put it back atomically; and the key-level edits that keep every comment,
//! blank line and commented-out default round the keys they touch.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use toml_edit::{ArrayOfTables, Decor, DocumentMut, Item, Key, Table, TableLike, Value};

use super::flock::{self, Wait};
use super::template::commented_line;
use super::{Config, ConfigError};

/// How many temporary names a save tries before giving up on finding a free one.
const TMP_TRIES: u32 = 16;

/// How long a writer waits for another svolder writing the same file before it gives up.
const LOCK_WAIT: Duration = Duration::from_secs(2);

/// Counts the temporary names this process has used, so two saves in one process never
/// pick the same one.
static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Read the file at `path`, edit it as a TOML document, and write it back.
///
/// An absent file starts from [`Config::template`], so the first thing to write a
/// configuration leaves a whole commented one behind rather than a bare `schema` key, and the
/// parent directory is made if it is not there. What `edit` produced is parsed again with
/// [`Config::parse`] before it replaces the file, so a writer can never leave behind a
/// configuration the next run refuses to load. An existing file whose edited text equals what
/// was read is not written at all: its mtime stays and a symbolic link stays one.
///
/// The read, the edit and the write hold an exclusive lock on `<path>.lock` beside the file
/// ([`flock::lock_beside`]), waiting up to two seconds for another svolder's, so two writers take
/// turns and neither loses the other's change; a wait that runs out fails naming the file. Where
/// no lock file can be had (a directory the user cannot write, a read-only filesystem), the write
/// goes on without one and fails, or not, as it would anyway. The wait is a plain sleep: the TUI
/// calls the writers on its own loop, so while another svolder holds the lock its input and
/// frames stop for up to those two seconds, a bound its callers accept. A file its owner cannot
/// write is refused ([`ConfigError::ReadOnly`]), unless the edit would not change it.
pub(super) fn edit_file(
    path: &Path,
    edit: impl FnOnce(&mut DocumentMut) -> Result<(), ConfigError>,
) -> Result<(), ConfigError> {
    make_parent(path)?;
    let _lock = lock(path)?;
    let (text, existed) = match std::fs::read_to_string(path) {
        Ok(text) => (text, true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::template(), false),
        Err(e) => return Err(ConfigError::Io(path.to_path_buf(), e)),
    };
    #[cfg(test)]
    super::tests::after_read(path);
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::Parse(e.to_string()))?;
    edit(&mut doc)?;
    let written = doc.to_string();
    Config::parse(&written)?;
    if existed && written == text {
        return Ok(());
    }
    refuse_read_only(path)?;
    write_atomically(path, &written)
}

/// Refuse a file whose owner cannot write it (`chmod 444`): a configuration made read-only is
/// read-only on purpose, and a writer leaves it as it is rather than replace it.
fn refuse_read_only(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) if meta.permissions().mode() & 0o200 == 0 => {
            Err(ConfigError::ReadOnly(path.to_path_buf()))
        }
        _ => Ok(()),
    }
}

/// Make the directory `path` sits in, at 0700, as [`write_atomically`] would: the lock file goes
/// there before anything is read.
pub(super) fn make_parent(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::DirBuilderExt;
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| ConfigError::Io(parent.to_path_buf(), e))
}

/// The lock `path`'s writers take turns on, waiting up to [`LOCK_WAIT`]; `None` where no lock
/// file can be had, and the write goes on without one.
pub(super) fn lock(path: &Path) -> Result<Option<flock::Held>, ConfigError> {
    match flock::lock_beside(path, Wait::Until(LOCK_WAIT)) {
        Ok(held) => Ok(Some(held)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            Err(ConfigError::Io(path.to_path_buf(), e))
        }
        Err(_) => Ok(None),
    }
}

/// Put `text` at `path`, through a temporary file beside it.
///
/// The text goes to a `<file name>.<pid>.<n>.tmp` in the same directory, is fsynced, and is
/// renamed over the target; the directory is fsynced after the rename, and an error from that
/// last step is ignored since the rename has already happened. A new file is created at 0600
/// whatever the umask, and one already there keeps the mode it had: the temporary file is
/// *opened* with it, so a config locked to 0600 over the four digits in it is never briefly
/// world-readable. A directory the write has to create is made at 0700. The temporary name is
/// unique to the process and the call and is created exclusively, so a concurrent save never
/// shares it and a file or symbolic link already at that name is never written through. A
/// failure before the rename removes the temporary file and reports `path`, not the
/// temporary one. A symbolic `path` is replaced by the regular file the rename leaves there,
/// not written through.
pub(super) fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigError> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| ConfigError::Io(parent.to_path_buf(), e))?;
    }
    // A file already there can carry a mode of its own, and it is read before the temporary
    // file is made rather than set on it afterwards: between the two there would be a window
    // where a config the user locked down is readable by anyone.
    let mode = match std::fs::metadata(path) {
        Ok(meta) => Some(meta.permissions().mode()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(ConfigError::Io(path.to_path_buf(), e)),
    };
    let io_error = |e| ConfigError::Io(path.to_path_buf(), e);
    let (tmp, mut file) = create_tmp(path, mode).map_err(io_error)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&tmp, path));
    drop(file);
    if let Err(e) = written {
        // The file is still the one that was there; the half-written one beside it is
        // not left behind to be read as a configuration of its own.
        let _ = std::fs::remove_file(&tmp);
        return Err(io_error(e));
    }
    if let Some(parent) = parent.or(Some(Path::new("."))) {
        let _ = std::fs::File::open(parent).and_then(|dir| dir.sync_all());
    }
    Ok(())
}

/// Create the temporary file for a write to `path`, and give back its name and the open file.
///
/// The name is `<file name>.<pid>.<n>.tmp` beside `path`, with `n` from a per-process
/// counter; the file is created with `create_new`, so a name already taken moves the counter
/// on, up to 16 tries, and the `AlreadyExists` of the last one is what is reported.
fn create_tmp(path: &Path, mode: Option<u32>) -> std::io::Result<(PathBuf, std::fs::File)> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut last = None;
    for _ in 0..TMP_TRIES {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = path.with_file_name(format!("{name}.{}.{n}.tmp", std::process::id()));
        match open_tmp(&tmp, mode) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.expect("at least one try was made"))
}

/// The temporary file a write goes through, created exclusively and open for writing.
///
/// `mode` is the target's when it has one and 0600 when it does not. It is asked for on the
/// way in rather than set afterwards, and then set outright so the umask cannot narrow it: a
/// config a user locked to 0600 over the four digits in it would otherwise be a
/// world-readable file for as long as the write took. `create_new` fails with `AlreadyExists`
/// on any file or symbolic link already at `tmp`, and never writes through it. A file it
/// created but could not set the mode of is removed again.
fn open_tmp(tmp: &Path, mode: Option<u32>) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mode = mode.unwrap_or(0o600);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(tmp)?;
    if let Err(e) = file.set_permissions(std::fs::Permissions::from_mode(mode)) {
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    Ok(file)
}

/// The array of tables `key` of `doc` (`device`, `preset`, `hop`), for a writer that has already
/// found a block in it.
pub(super) fn blocks_mut<'d>(
    doc: &'d mut DocumentMut,
    key: &str,
) -> Result<&'d mut ArrayOfTables, ConfigError> {
    doc.as_table_mut()
        .get_mut(key)
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| ConfigError::Parse(format!("{key} is not an array of tables")))
}

/// Add `table`, already filled in, to `doc` as the last `[[key]]` block.
///
/// A file with no such block yet gets the array made by this write, so its table is the last one
/// the document prints. Whatever the file ended with — the comments under its last table — is
/// taken along as the block's own leading decoration, rather than being left to print after it
/// and read as the new block's own.
pub(super) fn push_block(
    doc: &mut DocumentMut,
    key: &str,
    mut table: Table,
) -> Result<(), ConfigError> {
    let root = doc.as_table_mut();
    if !root.contains_key(key) {
        root.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
        let trailing = doc.trailing().as_str().unwrap_or_default().to_string();
        doc.set_trailing("");
        table.decor_mut().set_prefix(format!("{trailing}\n"));
    }
    blocks_mut(doc, key)?.push(table);
    Ok(())
}

/// Put `carried`, the comments a removed `[[key]]` block left to the file, in front of whatever
/// the document prints where the block at `at` was: the block that follows it, the table that
/// follows the whole array, or the end of the file.
pub(super) fn hand_on(
    doc: &mut DocumentMut,
    key: &str,
    at: usize,
    carried: String,
) -> Result<(), ConfigError> {
    if let Some(next) = blocks_mut(doc, key)?.get_mut(at) {
        let own = leading(next.decor());
        next.decor_mut().set_prefix(format!("{carried}{own}"));
        return Ok(());
    }
    if let Some(step) = nested(doc.as_table(), Some(key))
        && let Some(decor) = decor_at_mut(doc.as_table_mut(), &[step])
    {
        let own = leading(decor);
        decor.set_prefix(format!("{carried}{own}"));
        return Ok(());
    }
    let trailing = doc.trailing().as_str().unwrap_or_default().to_string();
    doc.set_trailing(format!("{carried}{trailing}"));
    Ok(())
}

/// The leading decoration of `table`, as the file wrote it.
pub(super) fn prefix_of(table: &Table) -> &str {
    table
        .decor()
        .prefix()
        .and_then(toml_edit::RawString::as_str)
        .unwrap_or_default()
}

/// The part of a block's leading decoration that is not the block's own: the comments a blank
/// line sets apart from its header, which explain the file rather than the block.
///
/// A run of nothing but whitespace is nobody's, and goes with the block.
pub(super) fn detached(prefix: &str) -> String {
    let mut at = 0;
    let mut above = 0;
    for line in prefix.split_inclusive('\n') {
        at += line.len();
        if line.trim().is_empty() {
            above = at;
        }
    }
    let above = &prefix[..above];
    if !above.contains('#') {
        return String::new();
    }
    format!("{}\n", above.trim_end_matches('\n'))
}

/// What `decor` leads its table with, or the blank line `toml_edit` writes for a table that
/// says nothing about it.
pub(super) fn leading(decor: &Decor) -> String {
    decor
        .prefix()
        .and_then(toml_edit::RawString::as_str)
        .unwrap_or("\n")
        .to_string()
}

/// Set `key` to `new` in the table of `doc` that `path` names.
///
/// This is [`set_key`] with the run of comments the document prints after that table's own
/// keys handed to it, which is where a key the table carries commented out at its default is
/// found. `toml_edit` keeps that run as the leading decoration of whatever the document prints
/// next, so it is read out of there, edited, and put back.
pub(super) fn set_key_at(
    doc: &mut DocumentMut,
    path: &[&str],
    key: &str,
    new: Value,
) -> Result<(), ConfigError> {
    let after = after(doc, path);
    let mut following = read_after(doc, &after);
    let table = table_at_mut(doc.as_table_mut(), path)
        .ok_or_else(|| ConfigError::Parse(format!("{} is not a table", path.join("."))))?;
    if !set_key(table, &mut following, key, new) {
        return Ok(());
    }
    write_after(doc, &after, following);
    Ok(())
}

/// The run of comments and blank lines `after` names, as text.
fn read_after(doc: &mut DocumentMut, after: &After) -> String {
    match after {
        After::Table(steps) => decor_at_mut(doc.as_table_mut(), steps)
            .and_then(|decor| decor.prefix())
            .and_then(toml_edit::RawString::as_str)
            .unwrap_or_default()
            .to_string(),
        After::Trailing => doc.trailing().as_str().unwrap_or_default().to_string(),
        After::Nothing => String::new(),
    }
}

/// Put `following` back where `after` names.
fn write_after(doc: &mut DocumentMut, after: &After, following: String) {
    match after {
        After::Table(steps) => {
            if let Some(decor) = decor_at_mut(doc.as_table_mut(), steps) {
                decor.set_prefix(following);
            }
        }
        After::Trailing => doc.set_trailing(following),
        After::Nothing => {}
    }
}

/// Take `key` out of the table of `doc` that `path` names, and leave it documented at `value`.
///
/// Clearing a key the configuration template writes is the other half of setting it: the live
/// key goes, and the commented line the template shows it at takes its place, under the comment
/// that explains it. Setting a key and clearing it again leaves the file as it was found.
///
/// A key the table does not carry live is left exactly as it is, comment and all.
pub(super) fn clear_key_at(
    doc: &mut DocumentMut,
    path: &[&str],
    key: &str,
    value: impl Into<Value>,
) -> Result<(), ConfigError> {
    let after = after(doc, path);
    let mut following = read_after(doc, &after);
    let table = table_at_mut(doc.as_table_mut(), path)
        .ok_or_else(|| ConfigError::Parse(format!("{} is not a table", path.join("."))))?;
    let Some(head) = table
        .key(key)
        .and_then(|found| found.leaf_decor().prefix())
        .and_then(toml_edit::RawString::as_str)
        .map(str::to_string)
    else {
        table.remove(key);
        return Ok(());
    };
    let next = next_value(table, key);
    table.remove(key);
    // Everything that led the live key leads the commented line that replaces it, and what the
    // document printed next follows it, so the line lands exactly where the key was.
    let restored = format!("{head}{}", commented_line(key, value));
    match next {
        Some(next) => {
            if let Some(mut found) = table.key_mut(&next) {
                let own = found
                    .leaf_decor()
                    .prefix()
                    .and_then(toml_edit::RawString::as_str)
                    .unwrap_or_default()
                    .to_string();
                found
                    .leaf_decor_mut()
                    .set_prefix(format!("{restored}{own}"));
            }
            Ok(())
        }
        None => {
            following.insert_str(0, &restored);
            write_after(doc, &after, following);
            Ok(())
        }
    }
}

/// The key a table prints after `key`, which is the next of its own keys there is: a table
/// prints every one of its values before the first table nested under it.
fn next_value(table: &dyn TableLike, key: &str) -> Option<String> {
    let mut reached = false;
    for (name, item) in table.iter() {
        if !reached {
            reached = name == key;
            continue;
        }
        if item.is_value() {
            return Some(name.to_string());
        }
    }
    None
}

/// Set `key` in `table` to `new`, keeping whatever the file writes around it.
///
/// A key the file already carries is written through in place: the key, and so the comment and
/// blank lines above it, are never replaced, and the value's own decoration (spaces around the
/// `=`, the trailing comment) carries onto the new value — replacing the item outright, as
/// `insert` does, would drop all of that.
///
/// A key the file carries only as a commented-out line, the way [`Config::template`] writes
/// every key, takes that line's place: the comment is dropped and the live key goes where it
/// stood, under the line that explains it. That line is looked for in the decoration above each
/// of the table's own keys, then in `following` (the run the document prints after them); the
/// return value says whether `following` changed, for the caller to put back.
///
/// A key the file says nothing about at all is appended with no decoration of its own.
pub(super) fn set_key(
    table: &mut dyn TableLike,
    following: &mut String,
    key: &str,
    new: Value,
) -> bool {
    if let Some(old) = table.get_mut(key).and_then(Item::as_value_mut) {
        let decor = old.decor().clone();
        *old = new;
        *old.decor_mut() = decor;
        return false;
    }
    if matches!(table.entry(key), toml_edit::Entry::Occupied(_)) {
        // The key is taken by something no value can be written over — a table of the same
        // name — so nothing is written and nothing around it is disturbed either.
        return false;
    }
    let Some((slot, head, tail)) = commented_key(table, following, key) else {
        table.insert(key, Item::Value(new));
        return false;
    };
    // The key takes over the comment line's place: everything the decoration held above that
    // line leads the live key, and everything below it leads whatever came next.
    let formatted = Key::new(key).with_leaf_decor(Decor::new(head, " "));
    table.entry_format(&formatted).or_insert(Item::Value(new));
    match slot {
        Slot::Before(next) => {
            if let Some(mut following) = table.key_mut(&next) {
                following.leaf_decor_mut().set_prefix(tail);
            }
            move_before(table, key, &next);
            false
        }
        Slot::Following => {
            *following = tail;
            true
        }
    }
}

/// Where the commented-out line for a key was found.
pub(super) enum Slot {
    /// Above one of the table's own keys, which the live key has to be written before.
    Before(String),
    /// In the run the document prints after the table's keys, which the live key is appended
    /// in front of.
    Following,
}

/// The commented-out line for `key`, as the decoration before it and the decoration after it.
///
/// The table's own keys are looked through in the order they are printed — every value of a
/// table is printed before the first table nested under it — and then `following`.
pub(super) fn commented_key(
    table: &dyn TableLike,
    following: &str,
    key: &str,
) -> Option<(Slot, String, String)> {
    for (name, item) in table.iter() {
        if !item.is_value() {
            continue;
        }
        if let Some(prefix) = table
            .key(name)
            .and_then(|k| k.leaf_decor().prefix())
            .and_then(toml_edit::RawString::as_str)
            && let Some((head, tail)) = split_at_commented(prefix, key)
        {
            return Some((Slot::Before(name.to_string()), head, tail));
        }
    }
    let (head, tail) = split_at_commented(following, key)?;
    Some((Slot::Following, head, tail))
}

/// `text` without the line that comments `key` out, as what stood above it and what stood
/// below it, or `None` when it holds no such line.
fn split_at_commented(text: &str, key: &str) -> Option<(String, String)> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if comments_out(line, key) {
            return Some((text[..at].to_string(), text[at + line.len()..].to_string()));
        }
        at += line.len();
    }
    None
}

/// Whether `line` is `key` commented out: a `#`, the key, and the `=` that would assign it.
pub(super) fn comments_out(line: &str, key: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

/// Move `key` in `table` to just in front of `before`.
///
/// A table prints its values in the order they were put into it, and a key that takes a
/// commented-out line's place has to print where that line stood. Everything from `before` on
/// is taken out and put back after `key`; this moves it up while every key keeps the
/// decoration it carries.
fn move_before(table: &mut dyn TableLike, key: &str, before: &str) {
    let mut moved: Vec<(Key, Item)> = Vec::new();
    let mut reached = false;
    for (name, item) in table.iter() {
        reached |= name == before;
        if reached
            && name != key
            && item.is_value()
            && let Some(found) = table.key(name)
        {
            moved.push((found.clone(), item.clone()));
        }
    }
    for (name, _) in &moved {
        table.remove(name.get());
    }
    for (name, item) in moved {
        table.entry_format(&name).or_insert(item);
    }
}

/// Where a document keeps the run of comments and blank lines it prints after a table's own
/// keys.
enum After {
    /// The leading decoration of the table this path names — the next header the document
    /// prints, which is where `toml_edit` parks everything written between the two.
    Table(Vec<Step>),
    /// The document's own trailing decoration, for a table nothing is printed after.
    Trailing,
    /// Nowhere of its own: a dotted table's keys are printed among its parent's.
    Nothing,
}

/// One step of the path to a table: the key that names it and, for a `[[table]]` array, which
/// of its blocks.
pub(super) struct Step {
    /// The key the table sits under.
    key: String,
    /// Which block of the array, for a key that names an array of tables.
    index: Option<usize>,
}

/// Where the run of comments the document prints after the keys of the table at `path` is
/// kept.
///
/// A document prints a table's keys, then the tables nested under it, then its parent's next
/// table, and so on out to the end of the file — so the run is the leading decoration of the
/// first of those there is, and the document's trailing decoration when there is none.
fn after(doc: &DocumentMut, path: &[&str]) -> After {
    let Some(table) = table_at(doc.as_table(), path) else {
        return After::Nothing;
    };
    if table.is_dotted() {
        return After::Nothing;
    }
    if let Some(step) = nested(table, None) {
        return After::Table(steps(path, step));
    }
    for depth in (0..path.len()).rev() {
        if let Some(parent) = table_at(doc.as_table(), &path[..depth])
            && let Some(step) = nested(parent, Some(path[depth]))
        {
            return After::Table(steps(&path[..depth], step));
        }
    }
    After::Trailing
}

/// The path `path` names with `last` on the end of it.
fn steps(path: &[&str], last: Step) -> Vec<Step> {
    let mut steps: Vec<Step> = path
        .iter()
        .map(|key| Step {
            key: (*key).to_string(),
            index: None,
        })
        .collect();
    steps.push(last);
    steps
}

/// The first table `table` prints under itself, or the first one it prints after the key
/// `from` when one is named.
///
/// A dotted table is passed over: its keys are printed among this table's own rather than
/// under a header of their own.
pub(super) fn nested(table: &Table, from: Option<&str>) -> Option<Step> {
    let mut reached = from.is_none();
    for (name, item) in table.iter() {
        if !reached {
            reached = Some(name) == from;
            continue;
        }
        match item {
            Item::Table(nested) if !nested.is_dotted() => {
                return Some(Step {
                    key: name.to_string(),
                    index: None,
                });
            }
            Item::ArrayOfTables(array) if !array.is_empty() => {
                return Some(Step {
                    key: name.to_string(),
                    index: Some(0),
                });
            }
            _ => {}
        }
    }
    None
}

/// The table `path` names under `table`.
pub(super) fn table_at<'a>(table: &'a Table, path: &[&str]) -> Option<&'a Table> {
    match path.split_first() {
        None => Some(table),
        Some((key, rest)) => table_at(table.get(key)?.as_table()?, rest),
    }
}

/// The table `path` names under `table`, to write into.
pub(super) fn table_at_mut<'a>(
    table: &'a mut Table,
    path: &[&str],
) -> Option<&'a mut dyn TableLike> {
    fn walk<'a>(table: &'a mut dyn TableLike, path: &[&str]) -> Option<&'a mut dyn TableLike> {
        match path.split_first() {
            None => Some(table),
            Some((key, rest)) => walk(table.get_mut(key)?.as_table_like_mut()?, rest),
        }
    }
    walk(table, path)
}

/// The leading decoration of the table `path` names under `table`.
pub(super) fn decor_at_mut<'a>(table: &'a mut Table, path: &[Step]) -> Option<&'a mut Decor> {
    let (step, rest) = path.split_first()?;
    let item = table.get_mut(&step.key)?;
    let next = match step.index {
        None => item.as_table_mut()?,
        Some(index) => item.as_array_of_tables_mut()?.get_mut(index)?,
    };
    if rest.is_empty() {
        Some(next.decor_mut())
    } else {
        decor_at_mut(next, rest)
    }
}
