// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer EQ's manager: an equalizer on what the computer plays, as a PipeWire filter
//! chain in front of the sink that feeds the DAC.
//!
//! [`Host::apply`] writes the chain's config and starts it; [`Host::remove`] stops it and
//! deletes what apply wrote; [`Host::status`] reads the state file and checks that the chain
//! runs. Something has
//! to keep the chain running, and [`Manager`] says what: a systemd user unit when the user
//! manager answers, else a detached `pipewire` process with an autostart entry; in a container
//! the chain is a child of the program that applied it, and ends with it.
//!
//! Apart from PipeWire and WirePlumber's own runtime state, which the running chain changes
//! the way any client does, svolder writes four kinds of file and nothing else:
//! `svolder/host-eq.conf` in the config directory, the unit `svolder-host-eq.service` or the
//! autostart entry `autostart/svolder-host-eq.desktop`, the state file `svolder/host-eq.toml`
//! and, for a process, its pid file in the private runtime directory. A child writes only the
//! config and the state file. `remove` deletes what
//! `apply` wrote. PipeWire's own config directories, `filter-chain.service` and every unit
//! but svolder's own are left alone.
//!
//! The programs it runs are `systemctl` (`SVOLDER_SYSTEMCTL`), `pipewire` (`SVOLDER_PIPEWIRE`),
//! `wpctl` (`SVOLDER_WPCTL`) and `wireplumber` (`SVOLDER_WIREPLUMBER`, for its version alone),
//! each looked up the way the environment says.
//! `SVOLDER_HOST=off` refuses everything that would run one.
//!
//! The process manager's plumbing, finding, starting and stopping the chain, is in
//! [`super::process`]; the verbs of `svolder host eq` are in the `svolder` binary.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use super::live::{self, CONFIG_RATE_HZ, SLOTS, render_live_conf};
use super::paths::{EqPaths, absolute, is_executable, on_path};
use super::points::Points;
use super::process::{Argv, Held, desktop_program, gone, parse_pid_file, spawn_detached};
use super::systemd::UserSystemd;
use super::{
    HostEqState, Manager, NODE_NAME, Route, Staged, WIREPLUMBER_ENV, render_desktop, render_unit,
    wireplumber_version, write_atomically,
};
use crate::locked;
use crate::sink::{DEFAULT_SINK, Wpctl, parse_inspect};
use svolder_profiles::tone::apo::{self, Correction};
use svolder_profiles::tone::response;

/// The systemd unit the chain runs as.
pub const UNIT: &str = "svolder-host-eq.service";

/// The autostart entry's file name.
const DESKTOP: &str = "svolder-host-eq.desktop";

/// The environment variable that names the `pipewire` to run.
const PIPEWIRE_ENV: &str = "SVOLDER_PIPEWIRE";

/// The environment variable that forces a manager.
const MANAGER_ENV: &str = "SVOLDER_HOST_EQ_MANAGER";

/// How long a stopped process is given to leave before it is killed.
const STOP_WAIT: Duration = Duration::from_secs(3);

/// How long a killed process is given to leave.
const KILL_WAIT: Duration = Duration::from_secs(1);

/// How long a new process is watched for an early exit.
const START_WATCH: Duration = Duration::from_millis(300);

/// How long the chain's sink is waited for after the chain started, before it is made the
/// default output.
const SINK_WAIT: Duration = Duration::from_secs(5);

/// How often a thing waited for is looked at again.
const POLL: Duration = Duration::from_millis(100);

/// What `apply` is asked to put in front of the sink.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyRequest {
    /// The preset's name, or a file's name.
    pub name: String,
    /// Where the correction came from, for people to read.
    pub origin: Option<String>,
    /// What to apply.
    pub correction: Correction,
    /// The sink to put it in front of, as a `node.name` or a wpctl id; the default output
    /// when absent.
    pub sink: Option<String>,
    /// The slot of the live chain (1 to 16) each filter sits in, in the filters' order; empty
    /// puts them in slots 1 to n.
    pub slots: Vec<u8>,
    /// The rate the sink runs at, when it is known: the config's coefficients are worked out
    /// for it (48 kHz when absent).
    pub rate_hz: Option<u32>,
}

/// What `host eq status` knows: the state file, and whether the chain runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// The state file, `None` when nothing was ever applied or the file cannot be read.
    pub state: Option<HostEqState>,
    /// Whether the unit is active or the process alive, `None` when that could not be told.
    pub running: Option<bool>,
    /// Why the state file cannot be read, when it cannot.
    pub unreadable: Option<String>,
}

impl Status {
    /// The words that name a mismatch between the state file and what runs, with the command
    /// that fixes it; `None` when they agree or the answer is not known.
    pub fn problem(&self) -> Option<String> {
        let state = self.state.as_ref()?;
        let (what, unit) = match state.manager {
            Manager::Systemd => ("the unit", true),
            Manager::Process | Manager::Child => ("the process", false),
        };
        match (state.on, self.running?) {
            (true, false) => Some(match unit {
                true => format!(
                    "{what} is not running: systemctl --user status {}",
                    UNIT.trim_end_matches(".service")
                ),
                false => {
                    format!("{what} is not running: apply it again with svolder host eq apply")
                }
            }),
            (false, true) => Some(format!("{what} is still running: svolder host eq remove")),
            _ => None,
        }
    }

    /// The status as the text `host eq status` prints, one line or two.
    pub fn lines(&self) -> Vec<String> {
        if let Some(why) = &self.unreadable {
            return vec![
                format!("host EQ: unknown: {why}"),
                "delete the file, then apply again".to_string(),
            ];
        }
        let mut lines = Vec::new();
        match &self.state {
            None => lines.push("host EQ: off".to_string()),
            Some(state) if !state.on => {
                lines.push(format!("host EQ: off (last: {})", state.name));
            }
            Some(state) => {
                let word = match self.running {
                    Some(false) => "stopped",
                    _ => "on",
                };
                lines.push(format!(
                    "host EQ: {} · {word} · in front of {} · {}",
                    state.name,
                    state.target,
                    state.manager.word()
                ));
            }
        }
        lines.extend(self.problem());
        lines
    }

    /// The status as the one JSON object `host eq status --json` prints.
    pub fn json(&self) -> serde_json::Value {
        if let Some(why) = &self.unreadable {
            return serde_json::json!({ "on": null, "running": null, "problem": why });
        }
        let Some(state) = &self.state else {
            return serde_json::json!({ "on": false, "running": null });
        };
        let filters: Vec<_> = state
            .filters
            .iter()
            .map(|f| serde_json::json!({ "type": f.kind.code(), "f": f.f, "g": f.g, "q": f.q }))
            .collect();
        serde_json::json!({
            "on": state.on,
            "running": self.running,
            "name": state.name,
            "origin": state.origin,
            "target": state.target,
            "route": state.route.word(),
            "manager": state.manager.word(),
            "previous_default": state.previous_default,
            "preamp_db": state.preamp_db,
            "filters": filters,
            "problem": self.problem(),
        })
    }
}

/// The preamp a correction is applied with: its stated preamp, or minus its peak when that is
/// lower. The peak is the highest the filters rise anywhere the chain plays, DC to Nyquist, floored at 0 and
/// rounded up to 0.1 dB, so the computer EQ never goes above unity.
pub fn safe_preamp(correction: &Correction) -> f64 {
    played_preamp(correction, CONFIG_RATE_HZ)
}

/// [`safe_preamp`] of `correction` as it plays at `rate_hz`: the filters are held to what the
/// biquads are worked out for (`live::held`) and the peak is read at that rate, so the preamp
/// is measured on the very curve that comes out of the chain.
pub fn played_preamp(correction: &Correction, rate_hz: u32) -> f64 {
    let (sets, centres) = live::played_sets(correction, rate_hz);
    let peak = played_peak(&sets, &centres, rate_hz);
    // A peak that is not a number is a filter that is not a filter: [`check_playable`] refuses
    // it, and the preamp then is as deep as it gets.
    let peak = match peak.is_nan() {
        true => f64::INFINITY,
        false => peak.max(0.0),
    };
    // Rounded up to a tenth of a dB, but a peak within 0.008 dB over a tenth (what
    // f32 rounding of the coefficients leaves) does not cost the next one.
    let limit = -((peak * 10.0 - PEAK_TOLERANCE_TENTHS).ceil().max(0.0) / 10.0);
    correction.preamp_db.min(limit).max(f64::MIN) + 0.0
}

/// [`response::played_peak_db_of`], remembering the newest answer: a drag asks for the same
/// curve several times an update (the check, the view, the send, the save), and one answer
/// costs about a millisecond for sixteen filters.
fn played_peak(sets: &[[f64; 5]], centres: &[f64], rate_hz: u32) -> f64 {
    use std::cell::RefCell;
    thread_local! {
        static NEWEST: RefCell<Option<(Vec<u64>, f64)>> = const { RefCell::new(None) };
    }
    let key: Vec<u64> = std::iter::once(u64::from(rate_hz))
        .chain(sets.iter().flatten().map(|c| c.to_bits()))
        .chain(centres.iter().map(|c| c.to_bits()))
        .collect();
    if let Some(peak) = NEWEST.with(|newest| {
        newest
            .borrow()
            .as_ref()
            .and_then(|(held, peak)| (*held == key).then_some(*peak))
    }) {
        return peak;
    }
    let peak = response::played_peak_db_of(sets, centres, rate_hz);
    NEWEST.with(|newest| *newest.borrow_mut() = Some((key, peak)));
    peak
}

/// The peak over a tenth of a dB the preamp's rounding forgives, in tenths of a dB: 0.008 dB.
/// The preamp is measured on the f32 coefficients that play; the tolerance only keeps a tenth
/// that the curve passes by a hair from costing the next one.
const PEAK_TOLERANCE_TENTHS: f64 = 0.08;
/// The deepest preamp the computer EQ plays, in dB: a curve that needs less is silence with a
/// resonance on top. A policy floor, the same as a typed biquad's own gain minimum.
const PREAMP_FLOOR_DB: f64 = -120.0;
/// The largest a coefficient can be: PipeWire clamps a `bq_raw`'s coefficient ports to this,
/// either way, so a filter past it would play as another filter.
const COEFFICIENT_LIMIT: f64 = 10.0;
/// The margin of the f32 stability test: the second Jury condition, `1 + a2 - |a1|`, must stay
/// above it, so the filter is stable in the coefficients as the chain runs them. It guards
/// stability and nothing about fidelity or noise: every hand-range point at every rate up to
/// 192 kHz passes.
const STABLE_MARGIN: f64 = 1e-7;

/// Whether `correction` can be played at `rate_hz` as it stands, `Ok` when it can: every filter
/// is strictly stable in the f32 coefficients the chain runs, and the curve needs a preamp
/// within the floor. The refusal says which and what to do; nothing has changed when it is
/// asked for.
pub fn check_playable(correction: &Correction, rate_hz: u32) -> Result<(), EqFailure> {
    let (sets, centres) = live::played_sets(correction, rate_hz);
    for (index, set) in sets.iter().enumerate() {
        if set.iter().any(|c| c.abs() > COEFFICIENT_LIMIT) {
            return Err(trouble(
                format!(
                    "point {} boosts more than the computer EQ's chain can hold at {rate_hz} Hz",
                    index + 1
                ),
                "Lower its gain",
            ));
        }
        let [b0, b1, b2, a1, a2] = *set;
        // A filter whose numerator is its denominator (0 dB) is the identity whatever its
        // poles: it is never refused, so a point can always be flattened and removed.
        if b0 == 1.0 && b1 == a1 && b2 == a2 {
            continue;
        }
        let stable = a2.abs() < 1.0 - STABLE_MARGIN && a1.abs() < 1.0 + a2 - STABLE_MARGIN;
        if !stable {
            return Err(trouble(
                format!(
                    "point {} would not be stable at {rate_hz} Hz: its frequency is too low for its shape at this rate",
                    index + 1
                ),
                "Raise its frequency, lower its Q or its gain, or use a lower sample rate",
            ));
        }
    }
    let peak = played_peak(&sets, &centres, rate_hz);
    if !peak.is_finite() || peak > -PREAMP_FLOOR_DB {
        return Err(trouble(
            match peak.is_finite() {
                true => format!(
                    "this curve peaks {peak:.0} dB above unity; the computer EQ plays curves up to {} dB",
                    -PREAMP_FLOOR_DB
                ),
                false => format!(
                    "this curve peaks without bound; the computer EQ plays curves up to {} dB",
                    -PREAMP_FLOOR_DB
                ),
            },
            "Lower the gains or the Q of the sharpest filters",
        ));
    }
    Ok(())
}

/// The sentence that says an apply lowered the stated preamp, `None` when it did not.
pub fn lowered_note(stated: f64, applied: f64) -> Option<String> {
    (applied < stated - 1e-9).then(|| {
        format!(
            "preamp lowered from {stated:.1} dB to {applied:.1} dB so the correction never clips"
        )
    })
}

/// Something the computer EQ could not do, in its own words and with the step that follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqFailure {
    /// What happened, without a program's name in front of it.
    pub what: String,
    /// The one step that follows.
    pub next: String,
}

impl std::fmt::Display for EqFailure {
    /// What happened and then what to do, as one line: `what. next`. A `what` that already
    /// ends a sentence (`.`, `?` or `!`) gains no full stop.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.what.ends_with(['.', '?', '!']) {
            true => write!(f, "{} {}", self.what, self.next),
            false => write!(f, "{}. {}", self.what, self.next),
        }
    }
}

impl std::error::Error for EqFailure {}

/// An [`EqFailure`] in the words given.
fn trouble(what: impl Into<String>, next: impl Into<String>) -> EqFailure {
    EqFailure {
        what: what.into(),
        next: next.into(),
    }
}

/// A file this verb needed to write and could not.
fn cannot_write(path: &Path, e: impl std::fmt::Display) -> EqFailure {
    trouble(
        format!("{}: {e}", path.display()),
        "Check that the directory can be written to",
    )
}

/// The path as text for a config, a unit and an autostart entry, or the refusal that says why
/// it cannot be: those files cannot carry a path that is not UTF-8 or that holds a newline or
/// another control character.
fn plain(path: &Path) -> Result<&str, EqFailure> {
    match path.to_str() {
        Some(text) if !text.chars().any(char::is_control) => Ok(text),
        _ => Err(trouble(
            format!(
                "the path {} has a character the computer EQ's files cannot carry",
                path.display()
            ),
            "Use a path of plain text, with no control characters",
        )),
    }
}

/// The `pipewire` this machine runs: `$SVOLDER_PIPEWIRE`, else `pipewire` on `PATH`, made
/// absolute; `None` when there is none to run.
fn find_pipewire() -> Option<PathBuf> {
    let named = std::env::var_os(PIPEWIRE_ENV).filter(|named| !named.is_empty());
    let found = match named {
        Some(path) => Some(PathBuf::from(path)).filter(|path| is_executable(path)),
        None => on_path("pipewire"),
    };
    found.map(|path| absolute(&path))
}

/// Whether the words `systemctl` refused with say the unit or its file is not there, which
/// for a stop or a disable is the state asked for already.
fn is_absent(why: &str) -> bool {
    let why = why.to_lowercase();
    ["not loaded", "not found", "does not exist", "no such file"]
        .iter()
        .any(|phrase| why.contains(phrase))
}

/// The machine the computer EQ runs on: where its files go, what runs it and which programs
/// it asks.
pub struct Host {
    /// Whether `SVOLDER_HOST=off` turns every part of the host off.
    off: bool,
    /// Whether this process runs in a container.
    container: bool,
    /// The `wpctl` to run.
    wpctl: OsString,
    /// The `wireplumber` whose `--version` names the WirePlumber this machine has.
    wireplumber: OsString,
    /// The `pipewire` to run, when there is one.
    pipewire: Option<PathBuf>,
    /// What runs `systemctl`.
    ctl: UserSystemd,
    /// A manager a test or the live check forces.
    forced: Option<Manager>,
    /// What `SVOLDER_HOST_EQ_MANAGER` said when it names no manager.
    bad_manager: Option<String>,
    /// `svolder/host-eq.conf`.
    conf: PathBuf,
    /// `svolder/host-eq.toml`.
    state: PathBuf,
    /// Where the unit goes.
    unit_dir: PathBuf,
    /// Where the autostart entry goes.
    autostart_dir: PathBuf,
    /// The pid file of a process, `None` where the private runtime directory cannot be used.
    pid_file: Option<PathBuf>,
    /// The chain the child manager started, which ends when this value is dropped.
    child: Mutex<Option<tokio::process::Child>>,
    /// Held by an apply or a remove from its start to its end, so two never interleave.
    gate: tokio::sync::Mutex<()>,
}

impl Host {
    /// This machine, as the environment describes it, with the files at `paths`. Every path is
    /// made absolute, since the unit, the autostart entry and the process's command line
    /// carry them. `container` is whether this process runs in a container.
    pub fn live(paths: EqPaths, container: bool) -> Host {
        let word = std::env::var(MANAGER_ENV)
            .ok()
            .filter(|word| !word.trim().is_empty());
        let forced = word.as_deref().and_then(Manager::parse);
        let bad_manager = word.filter(|_| forced.is_none());
        Host {
            off: crate::host_off(),
            container,
            wpctl: std::env::var_os(crate::sink::WPCTL_ENV)
                .filter(|named| !named.is_empty())
                .unwrap_or_else(|| "wpctl".into()),
            wireplumber: std::env::var_os(WIREPLUMBER_ENV)
                .filter(|named| !named.is_empty())
                .unwrap_or_else(|| "wireplumber".into()),
            pipewire: find_pipewire(),
            ctl: UserSystemd::from_env(),
            forced,
            bad_manager,
            conf: absolute(&paths.conf),
            state: absolute(&paths.state),
            unit_dir: absolute(&paths.unit_dir),
            autostart_dir: absolute(&paths.autostart_dir),
            pid_file: paths.pid_file.map(|file| absolute(&file)),
            child: Mutex::new(None),
            gate: tokio::sync::Mutex::new(()),
        }
    }

    /// Why the computer EQ cannot be applied here, when it cannot: host audio is off or there is
    /// no `pipewire`.
    pub fn unavailable(&self) -> Option<EqFailure> {
        if self.off {
            return Some(trouble(
                "host EQ is off: SVOLDER_HOST=off",
                "Unset SVOLDER_HOST to use the computer EQ",
            ));
        }
        if self.pipewire.is_none() {
            return Some(trouble(
                "PipeWire is not installed: no `pipewire` program was found",
                "Install PipeWire, or name the program in SVOLDER_PIPEWIRE",
            ));
        }
        None
    }

    /// Whether this host would keep the chain as a child of its own, which ends with the host:
    /// the manager is forced to `child`, or this is a container and none is forced.
    pub fn runs_as_child(&self) -> bool {
        match (self.forced, &self.bad_manager) {
            (Some(forced), _) => forced == Manager::Child,
            (None, None) => self.container,
            (None, Some(_)) => false,
        }
    }

    /// The state file, `None` when there is none or it cannot be read.
    pub fn state(&self) -> Option<HostEqState> {
        HostEqState::load(&self.state).ok().flatten()
    }

    /// The manager this machine uses: the one forced, else a child in a container, else
    /// systemd when its user manager answers, else a process. A forced manager that is none of
    /// the three is refused.
    fn manager(&self) -> Result<Manager, EqFailure> {
        if let Some(word) = &self.bad_manager {
            return Err(trouble(
                format!("{MANAGER_ENV}={word:?} names no manager"),
                "Use `systemd`, `process` or `child`",
            ));
        }
        Ok(self.forced.unwrap_or_else(|| match self.container {
            true => Manager::Child,
            false => match self.ctl.user_manager_answers() {
                true => Manager::Systemd,
                false => Manager::Process,
            },
        }))
    }

    fn unit_path(&self) -> PathBuf {
        self.unit_dir.join(UNIT)
    }

    /// The link `enable` makes in the unit directory for the unit's `WantedBy=`.
    fn wants_path(&self) -> PathBuf {
        self.unit_dir.join("pipewire.service.wants").join(UNIT)
    }

    fn desktop_path(&self) -> PathBuf {
        self.autostart_dir.join(DESKTOP)
    }

    /// Put `correction` in front of a sink and keep it there, replacing a computer EQ that is
    /// already on. Returns the state it saved.
    ///
    /// The preamp saved and written is the stated one or minus the correction's peak,
    /// whichever is lower ([`safe_preamp`]). The correction, the paths and the sink are
    /// checked, every directory it deletes a file from or keeps the pid in is tried, and the
    /// config and the unit or autostart entry are written beside their places ([`Staged`]),
    /// before anything is stopped, so a request that cannot be served, or whose files cannot be
    /// written or deleted, leaves a computer EQ that is on as it was.
    pub async fn apply(&self, mut request: ApplyRequest) -> Result<HostEqState, EqFailure> {
        let _gate = self.gate.lock().await;
        if let Some(failure) = self.unavailable() {
            return Err(failure);
        }
        let manager = self.manager()?;
        // The stated preamp is bounded where it is read (a file, the configuration); the one
        // saved may be lower than -30 dB for a correction that peaks high, and is applied again
        // as it stands. Here the filters are checked, and a preamp that is not a number or
        // boosts beyond +30 dB is refused.
        let stated = request.correction.preamp_db;
        if !stated.is_finite() || stated > 30.0 {
            return Err(trouble(
                format!(
                    "the correction cannot be applied: Preamp {stated} dB is not within +30 dB"
                ),
                "Fix the preamp",
            ));
        }
        let held = Points::from_state(&HostEqState {
            on: true,
            name: String::new(),
            origin: None,
            target: String::new(),
            route: Route::Smart,
            manager,
            previous_default: None,
            preamp_db: 0.0,
            filters: request.correction.filters.clone(),
            slots: request.slots.clone(),
        });
        if request.correction.filters.len() > SLOTS {
            return Err(trouble(
                format!(
                    "{} filters; the computer EQ holds {SLOTS}",
                    request.correction.filters.len()
                ),
                "Use a curve of 16 filters or fewer",
            ));
        }
        let mut shape = request.correction.clone();
        shape.preamp_db = 0.0;
        apo::validate(&shape).map_err(|e| {
            trouble(
                format!("the correction cannot be applied: {e}"),
                "Fix the band or the filter it names",
            )
        })?;
        check_playable(
            &request.correction,
            request.rate_hz.unwrap_or(CONFIG_RATE_HZ),
        )?;
        // Whatever the correction states, it is never applied above unity.
        request.correction.preamp_db = played_preamp(
            &request.correction,
            request.rate_hz.unwrap_or(CONFIG_RATE_HZ),
        );
        let pipewire = self.pipewire.clone().expect("unavailable() checked it");
        plain(&pipewire)?;
        plain(&self.conf)?;
        let wpctl = Wpctl::new(&self.wpctl);
        let previous = HostEqState::load(&self.state).ok().flatten();

        let route = self.route().await?;
        let default = default_sink(&wpctl).await;
        let target = self
            .target(
                &wpctl,
                request.sink.as_deref(),
                default.as_deref(),
                previous.as_ref(),
            )
            .await?;
        let previous_default = match route {
            Route::Smart => None,
            Route::Default => match default.as_deref() {
                Some(name) if name != NODE_NAME => Some(name.to_string()),
                _ => previous.as_ref().and_then(|s| s.previous_default.clone()),
            },
        };
        let state = HostEqState {
            on: true,
            name: request.name,
            origin: request.origin,
            target,
            route,
            manager,
            previous_default,
            preamp_db: request.correction.preamp_db,
            filters: held.filters.clone(),
            slots: held.slots.clone(),
        };
        // The process manager keeps its pid in the runtime directory, so one that has none is
        // refused here, before anything changes.
        if manager == Manager::Process && self.pid_file.is_none() {
            return Err(no_runtime_dir());
        }
        // Every directory this apply deletes a file from, keeps the pid in or saves the state in
        // is tried before anything is stopped, so one that cannot be written fails the apply
        // here rather than after the chain that is on has gone: the files of the manager a
        // switch leaves, the runtime directory the process manager writes its pid to, and the
        // state's.
        let mut touched: Vec<PathBuf> = vec![self.state.clone()];
        match manager {
            Manager::Process => {
                touched.extend(self.pid_file.clone());
                touched.extend(Some(self.unit_path()).filter(|unit| unit.exists()));
                touched.extend(Some(self.wants_path()).filter(|wants| wants.is_symlink()));
            }
            Manager::Systemd => {
                touched.extend(Some(self.desktop_path()).filter(|desktop| desktop.exists()));
                touched.extend(self.pid_file.clone().filter(|pid| pid.exists()));
            }
            // A child keeps no file of its own.
            Manager::Child => {}
        }
        for path in &touched {
            drop(stage(path, "")?);
        }
        // The files are written beside their places before anything of a computer EQ that is on
        // is touched: a write that is refused (a full disk, a directory that cannot be written)
        // fails the apply here, and the chain that is on runs on, on its own files.
        let manager_file = match manager {
            Manager::Systemd => Some(self.unit_path()),
            Manager::Process => Some(self.desktop_path()),
            Manager::Child => None,
        };
        let files = Files {
            conf: stage(
                &self.conf,
                &render_live_conf(
                    &held,
                    state.preamp_db,
                    &state.target,
                    state.route,
                    request.rate_hz.unwrap_or(CONFIG_RATE_HZ),
                ),
            )?,
            manager: match manager_file {
                Some(file) => Some(stage(
                    &file,
                    &match manager {
                        Manager::Systemd => render_unit(&pipewire, &self.conf),
                        _ => render_desktop(&pipewire, &self.conf),
                    },
                )?),
                None => None,
            },
        };

        // A chain the other manager runs, or left files of, is stopped and removed first, so
        // two never run at once, whether or not the state file names it. Under systemd a chain
        // that belongs to the active unit is the unit's, and is left to `systemctl restart`;
        // only the process manager's own leftovers are stopped.
        match manager {
            Manager::Systemd => {
                let unit_runs = self.unit_active() == Some(true);
                self.stop_process_with(!unit_runs).await?;
                self.stop_child().await;
            }
            Manager::Process => {
                self.stop_unit()?;
                self.stop_child().await;
            }
            // A child replaces the child this host started; what another manager left is not
            // its to stop.
            Manager::Child => {}
        }
        if let Err(failure) = self.start(&state, files, &pipewire, &wpctl).await {
            // The output that was the default goes back, what was written is taken away
            // again, and a computer EQ that was on is recorded as off.
            if state.route == Route::Default {
                self.restore_default(&state).await;
            }
            let _ = self.stop_manager(manager).await;
            self.delete_files();
            if let Some(mut old) = previous {
                old.on = false;
                let _ = old.save(&self.state);
            }
            return Err(failure);
        }
        state
            .save(&self.state)
            .map_err(|e| cannot_write(&self.state, e))?;
        Ok(state)
    }

    /// Write the config the chain starts from again, for `request`'s points and its safe
    /// preamp, without stopping or starting anything: a live edit's values are what a restart
    /// of the chain finds. Nothing is written when no computer EQ is on.
    pub async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure> {
        let _gate = self.gate.lock().await;
        let Some(state) = self.state().filter(|state| state.on) else {
            return Ok(());
        };
        let mut correction = request.correction;
        correction.preamp_db =
            played_preamp(&correction, request.rate_hz.unwrap_or(CONFIG_RATE_HZ));
        let points = Points::from_state(&HostEqState {
            filters: correction.filters.clone(),
            slots: request.slots.clone(),
            ..state.clone()
        });
        plain(&self.conf)?;
        let conf = render_live_conf(
            &points,
            correction.preamp_db,
            &state.target,
            state.route,
            request.rate_hz.unwrap_or(CONFIG_RATE_HZ),
        );
        write_atomically(&self.conf, conf.as_bytes()).map_err(|e| cannot_write(&self.conf, e))
    }

    /// The route this WirePlumber takes, from `wireplumber --version`: the daemon's own program
    /// prints the version banner, which `wpctl` has no option for.
    async fn route(&self) -> Result<Route, EqFailure> {
        let banner =
            crate::sink::output_within(&self.wireplumber, &["--version"], crate::sink::TIMEOUT)
                .await
                .ok_or_else(|| {
                    trouble(
                        format!(
                            "`{} --version` did not answer: WirePlumber is needed",
                            self.wireplumber.to_string_lossy()
                        ),
                        "Install WirePlumber, or name its program in SVOLDER_WIREPLUMBER",
                    )
                })?;
        wireplumber_version(&banner)
            .map(Route::for_wireplumber)
            .ok_or_else(|| {
                trouble(
                    "wireplumber does not say which libwireplumber it is linked with",
                    "Update WirePlumber",
                )
            })
    }

    /// The `node.name` of the sink to put the chain in front of: the one asked for, else the
    /// default output, and when the default output is the chain itself (a re-apply on the
    /// default route) the sink the last apply kept.
    async fn target(
        &self,
        wpctl: &Wpctl,
        asked: Option<&str>,
        default: Option<&str>,
        previous: Option<&HostEqState>,
    ) -> Result<String, EqFailure> {
        let name = match asked {
            Some(sink) => resolve_sink(wpctl, sink).await?,
            None => match default {
                Some(name) if name == NODE_NAME => previous.map(|s| s.target.clone()).ok_or_else(|| {
                    trouble(
                        "the computer EQ is the default output and there is no sink to put it in front of",
                        "Name the sink with --sink",
                    )
                })?,
                Some(name) => name.to_string(),
                None => {
                    return Err(trouble(
                        "there is no default output to put the EQ in front of",
                        "Name the sink with --sink; `wpctl status` lists them",
                    ));
                }
            },
        };
        match name == NODE_NAME || name.starts_with(&format!("{NODE_NAME}.")) {
            true => Err(trouble(
                "the sink is the computer EQ itself",
                "Name the sink the EQ goes in front of with --sink",
            )),
            false => Ok(name),
        }
    }

    /// Put the files for `state` in place and start the chain, then, on the default route, make
    /// it the default output.
    async fn start(
        &self,
        state: &HostEqState,
        files: Files,
        pipewire: &Path,
        wpctl: &Wpctl,
    ) -> Result<(), EqFailure> {
        files
            .conf
            .commit()
            .map_err(|e| cannot_write(&self.conf, e))?;
        match (state.manager, files.manager) {
            (Manager::Systemd, Some(staged)) => self.start_unit(staged).await?,
            (Manager::Process, Some(staged)) => self.start_process(pipewire, staged).await?,
            _ => self.start_child(pipewire).await?,
        }
        if state.route == Route::Default {
            let id = wpctl
                .wait_for_sink(NODE_NAME, SINK_WAIT, POLL)
                .await
                .ok_or_else(|| {
                    trouble(
                        format!("the sink {NODE_NAME} did not appear"),
                        "Check that PipeWire and WirePlumber are running",
                    )
                })?;
            wpctl
                .run(&["set-default", &id.to_string()])
                .await
                .ok_or_else(|| {
                    trouble(
                        format!("wpctl could not make {NODE_NAME} the default output"),
                        format!("Run `wpctl set-default {id}`"),
                    )
                })?;
        }
        Ok(())
    }

    /// Put the unit in place and have systemd (re)start it.
    async fn start_unit(&self, staged: Staged) -> Result<(), EqFailure> {
        let unit = self.unit_path();
        staged.commit().map_err(|e| cannot_write(&unit, e))?;
        let next = format!(
            "Run `systemctl --user status {}`",
            UNIT.trim_end_matches(".service")
        );
        for args in [
            vec!["daemon-reload"],
            vec!["enable", UNIT],
            vec!["restart", UNIT],
        ] {
            self.ctl
                .systemctl(&args)
                .map_err(|why| trouble(why, &next))?;
        }
        // A unit that fails at once is said, rather than left for `status` to find.
        for _ in 0..15 {
            match self.unit_active() {
                Some(true) | None => return Ok(()),
                Some(false) => tokio::time::sleep(POLL).await,
            }
        }
        match self.unit_active() {
            Some(false) => Err(trouble(format!("{UNIT} did not start"), next)),
            _ => Ok(()),
        }
    }

    /// Whether the unit is active, `None` when systemd cannot be asked.
    fn unit_active(&self) -> Option<bool> {
        let text = self.ctl.show(&["show", UNIT, "-p", "ActiveState"])?;
        Some(text.lines().any(|line| line.trim() == "ActiveState=active"))
    }

    /// Stop the chain's process, if any is running, then put the autostart entry in place and
    /// start `pipewire -c conf` detached. The entry goes in after the stop, which deletes the
    /// old one.
    async fn start_process(&self, pipewire: &Path, staged: Staged) -> Result<(), EqFailure> {
        let pid_file = self.pid_file.clone().ok_or_else(no_runtime_dir)?;
        self.stop_process().await?;
        let desktop = self.desktop_path();
        staged.commit().map_err(|e| cannot_write(&desktop, e))?;
        let argv = self.argv(pipewire);
        let held = spawn_detached(pipewire, &self.conf).map_err(|e| {
            trouble(
                format!("{}: {e}", pipewire.display()),
                "Check that PipeWire is installed",
            )
        })?;
        let record = format!("{}\n{}\n{}\n", held.pid(), argv.program(), argv.conf());
        if let Err(e) = write_atomically(&pid_file, record.as_bytes()) {
            // A process nobody can find again is not left running.
            held.signal(libc::SIGTERM);
            return Err(cannot_write(&pid_file, e));
        }
        tokio::time::sleep(START_WATCH).await;
        match argv.runs(held.pid()) {
            true => Ok(()),
            false => Err(trouble(
                "pipewire exited right after it started",
                format!("Run `{} -c {}` to see why", argv.program(), argv.conf()),
            )),
        }
    }

    /// Stop the child this host started, if any, and start `pipewire -c conf` as a child that
    /// ends with this host. Nothing but the config is written.
    async fn start_child(&self, pipewire: &Path) -> Result<(), EqFailure> {
        self.stop_child().await;
        let mut command = Command::new(pipewire);
        command
            .arg("-c")
            .arg(&self.conf)
            .current_dir("/")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        // SAFETY: `prctl` is async-signal-safe and the closure does nothing else between fork
        // and exec. The chain is killed when this process dies, even without a clean drop.
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|e| {
            trouble(
                format!("{}: {e}", pipewire.display()),
                "Check that PipeWire is installed",
            )
        })?;
        tokio::time::sleep(START_WATCH).await;
        if let Ok(Some(_)) | Err(_) = child.try_wait() {
            return Err(trouble(
                "pipewire exited right after it started",
                format!(
                    "Run `{} -c {}` to see why",
                    pipewire.display(),
                    self.conf.display()
                ),
            ));
        }
        *locked(&self.child) = Some(child);
        Ok(())
    }

    /// Kill the child this host started and wait for it to go; nothing when there is none.
    async fn stop_child(&self) {
        let child = locked(&self.child).take();
        if let Some(mut child) = child {
            let _ = child.kill().await;
        }
    }

    /// Whether the child this host started still runs.
    fn child_running(&self) -> bool {
        locked(&self.child)
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    /// The command line the chain runs as with `pipewire` as its program.
    fn argv(&self, pipewire: &Path) -> Argv {
        Argv::new(&pipewire.to_string_lossy(), &self.conf.to_string_lossy())
    }

    /// Stop what `manager` runs and delete its own files; the config and the state stay.
    async fn stop_manager(&self, manager: Manager) -> Result<(), EqFailure> {
        match manager {
            Manager::Systemd => self.stop_unit(),
            Manager::Process => self.stop_process().await,
            Manager::Child => {
                self.stop_child().await;
                Ok(())
            }
        }
    }

    /// Stop and disable the unit and delete svolder's files for it: its file and the link
    /// `enable` made. A unit systemd no longer knows, or whose file was deleted by hand while
    /// systemd still has it loaded, is as good as stopped: what `systemctl` says is not there
    /// is tolerated, and anything else it refuses with is a failure.
    fn stop_unit(&self) -> Result<(), EqFailure> {
        let unit = self.unit_path();
        let wants = self.wants_path();
        let file = unit.exists();
        let link = wants.symlink_metadata().is_ok();
        let loaded = self.ctl.show(&["show", UNIT, "-p", "LoadState"]);
        let known = loaded
            .as_deref()
            .is_some_and(|text| !text.contains("LoadState=not-found"));
        if loaded.is_none() && file {
            return Err(trouble(
                "systemctl did not answer, so the unit could not be stopped",
                "Run `systemctl --user disable --now svolder-host-eq`",
            ));
        }
        if known {
            for verb in ["stop", "disable"] {
                if let Err(why) = self.ctl.systemctl(&[verb, UNIT])
                    && !is_absent(&why)
                {
                    return Err(trouble(
                        why,
                        "Run `systemctl --user status svolder-host-eq`",
                    ));
                }
            }
        }
        if file {
            std::fs::remove_file(&unit).map_err(|e| cannot_write(&unit, e))?;
        }
        // The link is svolder's own and is removed only as the link it is.
        if link && wants.is_symlink() {
            std::fs::remove_file(&wants).map_err(|e| cannot_write(&wants, e))?;
        }
        if known || file || link {
            // The manager forgets the unit; a failure here leaves nothing running.
            let _ = self.ctl.systemctl(&["daemon-reload"]);
        }
        Ok(())
    }

    /// The processes that run the chain, with the command line each was found by.
    ///
    /// The pipewire may be any of the programs this machine could have started the chain
    /// with: the one it would start now, the one the pid file recorded and the one the
    /// autostart entry names, and the autostart entry is what starts the chain at login,
    /// when no pid file is left. Each is looked for by its exact command line among this
    /// user's processes.
    fn chains(&self) -> Vec<(u32, Argv)> {
        let conf = self.conf.to_string_lossy();
        let recorded = self
            .pid_file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| parse_pid_file(&text));
        let mut programs: Vec<String> = Vec::new();
        let named = self
            .pipewire
            .iter()
            .map(|p| p.to_string_lossy().into_owned());
        let from_entry = desktop_program(&self.desktop_path());
        let from_file = recorded
            .as_ref()
            .map(|(_, argv)| argv.program().to_string());
        for program in named.chain(from_file).chain(from_entry) {
            if !programs.contains(&program) {
                programs.push(program);
            }
        }
        let mut found: Vec<(u32, Argv)> = Vec::new();
        for program in programs {
            let argv = Argv::new(&program, &conf);
            let mut pids = argv.find();
            if let Some((pid, _)) = recorded.as_ref().filter(|(pid, _)| argv.runs(*pid)) {
                pids.push(*pid);
            }
            for pid in pids {
                if !found.iter().any(|(seen, _)| *seen == pid) {
                    found.push((pid, argv.clone()));
                }
            }
        }
        found
    }

    /// Stop the processes that run the chain, then delete the pid file and the autostart
    /// entry. What runs is found before either is deleted, since both say which program to
    /// look for.
    ///
    /// A pid is signalled only when its command line is the chain's exactly, through a pidfd
    /// opened before the command line is checked; a pid that is gone, or that another program
    /// has since been given, is never signalled.
    async fn stop_process(&self) -> Result<(), EqFailure> {
        self.stop_process_with(true).await
    }

    /// [`Host::stop_process`], signalling the chains it finds only when `signal_chains`; the
    /// pid file and the autostart entry are deleted either way.
    async fn stop_process_with(&self, signal_chains: bool) -> Result<(), EqFailure> {
        let chains: Vec<(Held, Argv)> = match signal_chains {
            true => self
                .chains()
                .into_iter()
                .filter_map(|(pid, argv)| Some((Held::open(pid, &argv)?, argv)))
                .collect(),
            false => Vec::new(),
        };
        let desktop = self.desktop_path();
        if desktop.exists() {
            std::fs::remove_file(&desktop).map_err(|e| cannot_write(&desktop, e))?;
        }
        for (held, _) in &chains {
            held.signal(libc::SIGTERM);
        }
        for (held, argv) in &chains {
            if !gone(held.pid(), argv, STOP_WAIT).await {
                held.signal(libc::SIGKILL);
                gone(held.pid(), argv, KILL_WAIT).await;
            }
        }
        match &self.pid_file {
            Some(file) => match std::fs::remove_file(file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(cannot_write(file, e)),
                _ => Ok(()),
            },
            None => Ok(()),
        }
    }

    /// Delete the config and the files of both managers, whatever is there.
    fn delete_files(&self) {
        for path in [self.conf.clone(), self.unit_path(), self.desktop_path()]
            .into_iter()
            .chain(self.pid_file.clone())
        {
            let _ = std::fs::remove_file(path);
        }
        let wants = self.wants_path();
        if wants.is_symlink() {
            let _ = std::fs::remove_file(wants);
        }
    }

    /// Stop the computer EQ and delete what [`Host::apply`] wrote; the state file stays, with
    /// `on` false, so the last correction can be applied again. On the default route the
    /// default output goes back to what it was, when that still exists.
    ///
    /// A chain that is already gone (a reboot, a kill) is removed all the same, and one that
    /// runs with no pid file (the autostart entry started it) is found and stopped.
    pub async fn remove(&self) -> Result<(), EqFailure> {
        let _gate = self.gate.lock().await;
        if self.off {
            return Err(self.unavailable().expect("host audio is off"));
        }
        let state = HostEqState::load(&self.state).map_err(|e| {
            trouble(
                e,
                "Delete the file, then run `svolder host eq remove` again",
            )
        })?;
        if let Some(state) = state.as_ref().filter(|s| s.route == Route::Default && s.on) {
            self.restore_default(state).await;
        }
        // Both managers are looked at: the one the state names, and any file another left.
        self.stop_unit()?;
        self.stop_process().await?;
        self.stop_child().await;
        self.delete_files();
        if let Some(mut state) = state {
            state.on = false;
            state
                .save(&self.state)
                .map_err(|e| cannot_write(&self.state, e))?;
        }
        Ok(())
    }

    /// Make the output the chain took over the default again, when it still exists.
    async fn restore_default(&self, state: &HostEqState) {
        let Some(previous) = state.previous_default.as_deref() else {
            return;
        };
        let wpctl = Wpctl::new(&self.wpctl);
        if let Some(id) = wpctl.find_sink(previous).await {
            let _ = wpctl.run(&["set-default", &id.to_string()]).await;
        }
    }

    /// What is applied and whether it runs.
    pub async fn status(&self) -> Status {
        let (state, unreadable) = match HostEqState::load(&self.state) {
            Ok(state) => (state, None),
            Err(why) => (None, Some(why)),
        };
        // With the host off nothing is asked of systemd or the process table.
        let running = match &state {
            Some(state) if !self.off => match state.manager {
                Manager::Systemd => self.unit_active(),
                Manager::Process => Some(self.process_running()),
                Manager::Child => Some(self.child_running()),
            },
            _ => None,
        };
        Status {
            state,
            running,
            unreadable,
        }
    }

    /// Whether a process runs the chain.
    fn process_running(&self) -> bool {
        !self.chains().is_empty()
    }
}

/// What the computer EQ answers to: [`Host`] on a real machine, [`crate::fake::FakeChain`] in the
/// tests of what drives it.
#[async_trait]
pub trait Chain: Send + Sync {
    /// Put the correction in front of the sink and keep it there, as [`Host::apply`].
    async fn apply(&self, request: ApplyRequest) -> Result<HostEqState, EqFailure>;

    /// Stop the chain and delete what it wrote, as [`Host::remove`].
    async fn remove(&self) -> Result<(), EqFailure>;

    /// What is applied and whether it runs, as [`Host::status`].
    async fn status(&self) -> Status;

    /// Write the config the running chain restarts from for `request`, touching nothing else,
    /// as [`Host::keep_conf`].
    async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure>;

    /// Why the computer EQ cannot be applied here, when it cannot.
    fn unavailable(&self) -> Option<EqFailure>;

    /// The saved state, `None` when there is none or it cannot be read.
    fn state(&self) -> Option<HostEqState>;
}

#[async_trait]
impl Chain for Host {
    async fn apply(&self, request: ApplyRequest) -> Result<HostEqState, EqFailure> {
        Host::apply(self, request).await
    }

    async fn remove(&self) -> Result<(), EqFailure> {
        Host::remove(self).await
    }

    async fn keep_conf(&self, request: ApplyRequest) -> Result<(), EqFailure> {
        Host::keep_conf(self, request).await
    }

    async fn status(&self) -> Status {
        Host::status(self).await
    }

    fn unavailable(&self) -> Option<EqFailure> {
        Host::unavailable(self)
    }

    fn state(&self) -> Option<HostEqState> {
        Host::state(self)
    }
}

/// The files an apply writes, staged beside their places before anything is changed.
struct Files {
    /// `host-eq.conf`.
    conf: Staged,
    /// The unit, or the autostart entry; a child has none.
    manager: Option<Staged>,
}

/// `text` written beside `path`, or the failure to say when it cannot be.
fn stage(path: &Path, text: &str) -> Result<Staged, EqFailure> {
    Staged::write(path, text.as_bytes()).map_err(|e| cannot_write(path, e))
}

/// The process manager's refusal where there is no runtime directory to keep its pid in.
fn no_runtime_dir() -> EqFailure {
    trouble(
        "there is no private runtime directory to keep the process's pid in",
        "Set XDG_RUNTIME_DIR to a directory only you can write to",
    )
}

/// The `node.name` of the default output, `None` when `wpctl` has none to say.
async fn default_sink(wpctl: &Wpctl) -> Option<String> {
    let text = wpctl.run(&["inspect", DEFAULT_SINK]).await?;
    parse_inspect(&text).1
}

/// The `node.name` of the sink `asked` names: a wpctl id (all digits) read through `wpctl
/// inspect`, else the name itself.
async fn resolve_sink(wpctl: &Wpctl, asked: &str) -> Result<String, EqFailure> {
    let asked = asked.trim();
    if asked.is_empty() || asked.chars().any(char::is_control) {
        return Err(trouble(
            format!("{asked:?} is not a sink name"),
            "Name the sink by its node.name or its wpctl id; `wpctl status` lists them",
        ));
    }
    if !asked.chars().all(|c| c.is_ascii_digit()) {
        return Ok(asked.to_string());
    }
    let inspected = wpctl.run(&["inspect", asked]).await;
    inspected
        .as_deref()
        .and_then(|text| parse_inspect(text).1)
        .ok_or_else(|| {
            trouble(
                format!("wpctl knows no sink with id {asked}"),
                "`wpctl status` lists the sinks and their ids",
            )
        })
}

#[cfg(test)]
mod tests;
