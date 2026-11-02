// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound that plays with the wordmark intro (`[tui] intro_sound`).
//!
//! The sound is a short clip embedded in the binary, played through PipeWire's `pw-play` at the
//! sample rate the device already runs at, so that a Schiit DAC's relay does not click on a rate
//! change. It starts once, the moment the device first answers while the wordmark is up; a key
//! that skips the intro stops it, and leaving the TUI stops it and removes the file it played
//! from. Every failure is silent: the sound is a courtesy, never a reason to say anything.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use svolder_ipc::TuiConfig;
use svolder_proto::FunctionId;

use crate::tui::app::Reading;
use crate::which::on_path;

/// The clip, Opus in an Ogg container.
const CLIP: &[u8] = include_bytes!("../../../assets/intro.ogg");

/// The profile key of the control that reads the sample rate the device runs at.
const RATE_KEY: &str = "sample_rate";

/// How often the reaper looks at a playing clip to see whether it has ended.
const REAP_EVERY: Duration = Duration::from_millis(250);

/// The sample rate in hertz a raw `sample_rate` reading stands for, by raw value and never by
/// label; `None` for no reading, for 0 and for anything outside 1 to 8, so that no rate is
/// asked for.
pub(super) fn hz_of(raw: Option<i64>) -> Option<u32> {
    match raw? {
        1 => Some(44_100),
        2 => Some(48_000),
        3 => Some(88_200),
        4 => Some(96_000),
        5 => Some(176_400),
        6 => Some(192_000),
        7 => Some(352_800),
        8 => Some(384_000),
        _ => None,
    }
}

/// The rate the device runs at, from the readings the app holds.
pub(in crate::tui) fn rate_of(readings: &BTreeMap<FunctionId, Reading>) -> Option<u32> {
    let reading = readings
        .values()
        .find(|reading| reading.value.key.as_deref() == Some(RATE_KEY))?;
    hz_of(reading.value.raw.as_i64())
}

/// The arguments `pw-play` is run with to play `file`, at `rate` hertz when one is known.
pub(super) fn arguments(rate: Option<u32>, file: &Path) -> Vec<OsString> {
    let mut properties =
        String::from("{ media.name = \"svolder intro\" application.name = \"svolder\"");
    if let Some(rate) = rate {
        properties.push_str(&format!(" node.rate = 1/{rate}"));
    }
    properties.push_str(" }");
    vec!["-P".into(), properties.into(), file.into()]
}

/// A clip that is playing.
pub(super) trait Playing {
    /// End the clip now and reap it.
    fn stop(&mut self);
}

/// Something that starts `pw-play`.
pub(super) trait Player {
    /// Run the player with `args`, with nothing attached to its standard streams.
    fn play(&self, args: &[OsString]) -> io::Result<Box<dyn Playing>>;
}

/// The real `pw-play`, at the path `PATH` found it.
struct PwPlay(PathBuf);

impl PwPlay {
    /// Run the player with `args`, with nothing attached to its standard streams.
    fn spawn(&self, args: &[OsString]) -> io::Result<Child> {
        Command::new(&self.0)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

impl Player for PwPlay {
    fn play(&self, args: &[OsString]) -> io::Result<Box<dyn Playing>> {
        let child = Arc::new(Mutex::new(self.spawn(args)?));
        spawn_reaper(Arc::clone(&child));
        Ok(Box::new(Running(child)))
    }
}

/// Reap `child` from a thread of its own once it has ended, so a clip that plays to its end
/// never lingers as a zombie while the TUI runs on.
fn spawn_reaper(child: Arc<Mutex<Child>>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(REAP_EVERY);
            if !matches!(locked(&child).try_wait(), Ok(None)) {
                return;
            }
        }
    });
}

/// `child` locked, whatever a thread that panicked while holding it left behind.
fn locked(child: &Mutex<Child>) -> MutexGuard<'_, Child> {
    child.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A `pw-play` child that is running.
struct Running(Arc<Mutex<Child>>);

impl Playing for Running {
    fn stop(&mut self) {
        let mut child = locked(&self.0);
        // A child that has already ended refuses the kill, and has nothing left to reap.
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The sound for one `svolder tui` run: started at most once, stopped by a skip or by leaving.
pub(in crate::tui) struct IntroSound {
    player: Box<dyn Player>,
    /// The private file the clip is written to before each play.
    file: PathBuf,
    playing: Option<Box<dyn Playing>>,
    started: bool,
}

impl IntroSound {
    /// The sound for a run, or `None` when this run plays none: the intro and the sound are
    /// both on, host audio is allowed (on in `[tui]` and not held off for the process),
    /// `pw_play` is the path `pw-play` was found at, and `dir` is where the private file goes.
    pub(super) fn planned(
        tui: &TuiConfig,
        host_off: bool,
        pw_play: Option<PathBuf>,
        dir: &Path,
    ) -> Option<IntroSound> {
        if !tui.intro || !tui.intro_sound || !tui.host_audio || host_off {
            return None;
        }
        let player = PwPlay(pw_play?);
        Some(IntroSound::new(Box::new(player), dir))
    }

    /// The sound for this run, from the environment: `SVOLDER_HOST`, `PATH`, and the private
    /// directory.
    pub(in crate::tui) fn for_run(tui: &TuiConfig) -> Option<IntroSound> {
        // Checked here as well as in `planned`, so a run with no sound never creates the
        // private directory.
        if !tui.intro || !tui.intro_sound {
            return None;
        }
        let dir = svolder_ipc::socket::path::private_dir().ok()?;
        IntroSound::planned(tui, svolder_host::host_off(), on_path("pw-play"), &dir)
    }

    /// A sound that plays through `player`, from a file in `dir`.
    pub(super) fn new(player: Box<dyn Player>, dir: &Path) -> IntroSound {
        IntroSound {
            player,
            file: dir.join(format!("svolder-intro-{}.ogg", std::process::id())),
            playing: None,
            started: false,
        }
    }

    /// Start the clip, at `rate` hertz when one is known. Only the first call does anything,
    /// and a failure to write the file or to run the player is silent.
    pub(in crate::tui) fn start(&mut self, rate: Option<u32>) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        if write_private(&self.file, CLIP).is_err() {
            return;
        }
        self.playing = self.player.play(&arguments(rate, &self.file)).ok();
    }

    /// Stop the clip if it is playing, and remove the file.
    pub(in crate::tui) fn stop(&mut self) {
        if let Some(mut playing) = self.playing.take() {
            playing.stop();
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

impl Drop for IntroSound {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Write `bytes` to `path`, readable by this user alone. A file or link already there is
/// removed first and the new file is created exclusively, without following a link, so
/// nothing but this file is ever written.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests;
