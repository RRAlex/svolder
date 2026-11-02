// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's stderr log: one level, read once from `RUST_LOG` or `-v`.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::SystemTime;

use svolder_proto::Message;

/// How much the daemon says. Ordered from quietest to loudest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Only failures.
    Error = 0,
    /// Failures and things that look wrong but are survivable.
    Warn = 1,
    /// The default: lifecycle lines a user would want in a journal.
    Info = 2,
    /// Every command sent and status received.
    Debug = 3,
    /// Everything `Debug` says, reserved for finer detail later.
    Trace = 4,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Whether each line carries its own timestamp: not when the journal stamps it.
static STAMPED: AtomicBool = AtomicBool::new(true);

/// Where stderr goes, as the `version` reply says: a file's path, or `journal`.
static TARGET: OnceLock<Option<String>> = OnceLock::new();

/// What the reply's `log` field names for the `journal`.
const JOURNAL: &str = "journal";

/// Set the level from `RUST_LOG`, raised to `Debug` when `-v` was given, and read where
/// stderr goes.
pub fn init(verbose: bool) {
    let journal = stderr_is_journal(
        std::env::var("JOURNAL_STREAM").ok().as_deref(),
        stderr_identity(),
    );
    STAMPED.store(!journal, Ordering::Relaxed);
    let file = std::fs::read_link("/proc/self/fd/2")
        .ok()
        .filter(|path| path.is_absolute() && path.is_file());
    let _ = TARGET.set(target_from(journal, file));
    // The transport crate prints through the same stamp.
    svolder_ble::set_logger(ble_line);
    let from_env = level_from(&std::env::var("RUST_LOG").unwrap_or_default());
    let level = if verbose {
        from_env.max(Level::Debug)
    } else {
        from_env
    };
    LEVEL.store(level as u8, Ordering::Relaxed);
}

/// The level a `RUST_LOG` value names: a bare level, or the level after `=` in a
/// `target=level` form such as `svolderd=debug`. Anything else is `Info`.
fn level_from(text: &str) -> Level {
    let word = text.rsplit('=').next().unwrap_or(text).trim();
    match word.to_ascii_lowercase().as_str() {
        "error" => Level::Error,
        "warn" => Level::Warn,
        "debug" => Level::Debug,
        "trace" => Level::Trace,
        _ => Level::Info,
    }
}

/// Whether a message at `level` is printed.
fn enabled(level: Level) -> bool {
    LEVEL.load(Ordering::Relaxed) >= level as u8
}

/// Where this daemon's stderr goes, for the `version` reply: a file's path, `journal`, or
/// nothing when it is a terminal or a pipe. Read once at start.
pub fn target() -> Option<String> {
    TARGET.get().cloned().flatten()
}

/// What the reply names for stderr: the journal when `journal`, else the file `file` that
/// stderr is open on.
fn target_from(journal: bool, file: Option<PathBuf>) -> Option<String> {
    match journal {
        true => Some(JOURNAL.to_string()),
        false => file.map(|path| path.display().to_string()),
    }
}

/// The device and inode of stderr, as `fstat(2)` reports them.
fn stderr_identity() -> Option<(u64, u64)> {
    let mut st = unsafe { std::mem::zeroed::<libc::stat>() };
    // SAFETY: `fstat` writes only into the `stat` we own.
    let rc = unsafe { libc::fstat(libc::STDERR_FILENO, &mut st) };
    // The field types differ between targets; `u64` is what `JOURNAL_STREAM` is read as.
    #[allow(clippy::unnecessary_cast)]
    let identity = (st.st_dev as u64, st.st_ino as u64);
    (rc == 0).then_some(identity)
}

/// Whether stderr is the systemd journal: `stream` is `JOURNAL_STREAM`, `device:inode` of the
/// stream systemd connected, and `stderr` is the identity of this process's stderr.
fn stderr_is_journal(stream: Option<&str>, stderr: Option<(u64, u64)>) -> bool {
    let named = stream.and_then(|text| {
        let (device, inode) = text.split_once(':')?;
        Some((device.parse::<u64>().ok()?, inode.parse::<u64>().ok()?))
    });
    named.is_some() && named == stderr
}

/// The line for `message` at `level`: the stamp and the daemon's name when there is a stamp, and
/// `warning:` or `error:` for the two levels a reader may colour. Under the journal there is
/// neither stamp nor name, since the journal tags the unit and the pid itself.
fn render(level: Level, message: &str, stamp: Option<&str>) -> String {
    let label = match level {
        Level::Error => "error: ",
        Level::Warn => "warning: ",
        _ => "",
    };
    match stamp {
        Some(stamp) => format!("{stamp} svolderd: {label}{message}"),
        None => format!("{label}{message}"),
    }
}

/// Print `line`, preceded by the local time unless the journal stamps it.
fn print(line: impl FnOnce(Option<&str>) -> String) {
    let stamp = STAMPED
        .load(Ordering::Relaxed)
        .then(|| svolder_ipc::clock::local_stamp(SystemTime::now()));
    eprintln!("{}", line(stamp.as_deref()));
}

/// Print one line, prefixed with the daemon's name, when `level` is enabled.
fn emit(level: Level, message: &str) {
    #[cfg(test)]
    if hold(level, message) {
        return;
    }
    if enabled(level) {
        print(|stamp| render(level, message, stamp));
    }
}

/// A line from the transport crate, which names itself; a warning carries `warning:`.
fn ble_line(level: svolder_ble::LogLevel, text: &str) {
    let label = match level {
        svolder_ble::LogLevel::Warn => "warning: ",
        svolder_ble::LogLevel::Info => "",
    };
    print(|stamp| match stamp {
        Some(stamp) => format!("{stamp} svolder-ble: {label}{text}"),
        None => format!("svolder-ble: {label}{text}"),
    });
}

#[cfg(test)]
thread_local! {
    /// The lines this thread's log has been asked to keep instead of print, with the level each
    /// was said at. It is per thread: a test that reads it runs on a current-thread runtime,
    /// where every task it spawns runs on that thread too.
    static HELD: std::cell::RefCell<Option<Vec<(Level, String)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Keep what this thread's log says from now on, instead of printing it.
#[cfg(test)]
pub fn hold_lines() {
    HELD.with(|held| *held.borrow_mut() = Some(Vec::new()));
}

/// Every line this thread's log has kept since [`hold_lines`], with its level.
#[cfg(test)]
pub fn held_lines() -> Vec<(Level, String)> {
    HELD.with(|held| held.borrow().clone().unwrap_or_default())
}

/// Keep `message` when this thread holds lines; says whether it did.
#[cfg(test)]
fn hold(level: Level, message: &str) -> bool {
    HELD.with(|held| match held.borrow_mut().as_mut() {
        Some(lines) => {
            lines.push((level, message.to_string()));
            true
        }
        None => false,
    })
}

/// Report a failure.
pub fn error(message: &str) {
    emit(Level::Error, message);
}

/// Report something survivable that looks wrong.
pub fn warn(message: &str) {
    emit(Level::Warn, message);
}

/// Report a lifecycle event.
pub fn info(message: &str) {
    emit(Level::Info, message);
}

/// Report a detail useful when something is being debugged.
pub fn debug(message: &str) {
    emit(Level::Debug, message);
}

/// Log one decoded message at `Debug`, as its id, wire type and value.
///
/// `direction` reads `->` for a command the daemon sent and `<-` for a status it received.
/// The frame bytes themselves are not visible outside `Session`, so they are not logged.
pub fn message(direction: &str, device: &str, m: &Message) {
    if enabled(Level::Debug) {
        print(|stamp| {
            render(
                Level::Debug,
                &format!(
                    "{device} {direction} {} {} {}",
                    m.id,
                    m.value.value_type(),
                    m.value
                ),
                stamp,
            )
        });
    }
}

#[cfg(test)]
mod tests;
