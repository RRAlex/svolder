// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A machine the wizard can be walked over without touching this one.
//!
//! Everything [`super::Effects`] can do to a host is answered here from what a case set before the
//! run, and what the wizard did is read back from the record afterwards: no radio, no
//! configuration file, no daemon and no units. It lives beside the core rather than inside one
//! module's tests because both front ends are tested against a real [`Wizard`]: the wizard's
//! frames are rendered from the prompts a wizard over this produced, not from hand-written ones.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use svolder_ipc::checks::{Fix, Outcome, State};
use svolder_ipc::{DeviceConfig, ScanEntry};
use svolder_profiles::Registry;

use super::{Connect, Files, Host, Platform, Radio, Step, Wizard};
use crate::recovery::Failure;
use crate::scanning::{Grade, Scanned};

/// What one run of the wizard did to the machine.
#[derive(Default)]
pub(crate) struct Record {
    /// What applying the fixes said, in order.
    applied: Vec<String>,
    /// How many scans have run.
    scans: usize,
    /// The passphrases a connection was tried with, in order.
    tried: Vec<String>,
    /// The device that was written.
    device: Option<DeviceConfig>,
    /// The adapter that was written.
    adapter: Option<String>,
    /// Whether the daemon was installed.
    daemon: bool,
    /// How many times a running daemon was told to read the file again.
    reloads: usize,
}

/// A machine the wizard can walk over: what it answers is set before the run, and what the
/// wizard did to it is read back after.
///
/// Clones share the record, so a test can hand one to the wizard and keep one to read.
#[derive(Clone)]
pub(crate) struct FakeEffects {
    /// What the checks answer.
    outcomes: Vec<Outcome>,
    /// What a scan hears.
    seen: Vec<ScanEntry>,
    /// Why a scan cannot run at all on this host, for a case about one that never will.
    scan_fails: Option<String>,
    /// What opening a backend for that scan had to say.
    notices: Vec<String>,
    /// How well each adapter hears the device.
    grades: Vec<Grade>,
    /// The only passphrase a connection is accepted with, or `None` to accept any.
    accepts: Option<String>,
    /// The names the configuration already carries.
    taken: Vec<String>,
    /// What this machine makes of a background service.
    platform: Platform,
    /// Whether writing the device, the adapter or the units works on this machine.
    writes: bool,
    /// How long a scan takes to answer, for a case about what is on the screen while one runs.
    scan_takes: Duration,
    /// How long writing the device takes, for a case about a wait over a write.
    write_takes: Duration,
    /// The configuration file the device is written into, for the cases that read what a walk
    /// left behind through the very writers the machine uses.
    path: Option<std::path::PathBuf>,
    /// The profiles this build reads devices through.
    registry: Option<Registry>,
    /// What the run did.
    record: Arc<Mutex<Record>>,
}

impl FakeEffects {
    /// A host where every check passes, nothing is configured, and systemd is there.
    pub(crate) fn healthy() -> FakeEffects {
        FakeEffects {
            outcomes: vec![
                Outcome {
                    check: "Bluetooth service",
                    state: State::Pass,
                    reason: "bluetoothd is running".into(),
                    fix: None,
                },
                Outcome {
                    check: "Adapters",
                    state: State::Pass,
                    reason: "hci0 is powered".into(),
                    fix: None,
                },
            ],
            seen: Vec::new(),
            scan_fails: None,
            notices: Vec::new(),
            grades: Vec::new(),
            accepts: None,
            taken: Vec::new(),
            platform: Platform {
                systemd: true,
                nixos: false,
                container: false,
            },
            writes: true,
            scan_takes: Duration::ZERO,
            write_takes: Duration::ZERO,
            path: None,
            registry: Some(Registry::builtin().expect("the built-in profiles load")),
            record: Arc::new(Mutex::new(Record::default())),
        }
    }

    /// The same host, with `seen` on the air.
    pub(crate) fn with_scan(mut self, seen: Vec<ScanEntry>) -> FakeEffects {
        self.seen = seen;
        self
    }

    /// The same host, with what opening a backend for a scan had to say.
    pub(crate) fn with_notices<const N: usize>(mut self, notices: [&str; N]) -> FakeEffects {
        self.notices = notices.iter().map(|line| (*line).to_string()).collect();
        self
    }

    /// The same host, where a scan cannot run at all, for the reason it gives.
    pub(crate) fn scan_failing(mut self, why: &str) -> FakeEffects {
        self.scan_fails = Some(why.to_string());
        self
    }

    /// The same host, with nothing on it that can be written to.
    pub(crate) fn unwritable(mut self) -> FakeEffects {
        self.writes = false;
        self
    }

    /// The same host, with a scan that takes `takes` to answer.
    pub(crate) fn scanning_for(mut self, takes: Duration) -> FakeEffects {
        self.scan_takes = takes;
        self
    }

    /// The same host, where writing the device takes `takes` to go through.
    pub(crate) fn writing_for(mut self, takes: Duration) -> FakeEffects {
        self.write_takes = takes;
        self
    }

    /// The same host, whose device writer puts the block in the file at `path`, through the
    /// very writer the live machine uses: what a case that reads the file a walk left behind
    /// needs, rather than the record alone.
    pub(crate) fn writing_into(mut self, path: std::path::PathBuf) -> FakeEffects {
        self.path = Some(path);
        self
    }

    /// The same host, with these adapters and how many of five windows each caught the
    /// device in.
    pub(crate) fn with_adapters<const N: usize>(
        mut self,
        adapters: [(&str, u8); N],
    ) -> FakeEffects {
        self.grades = adapters
            .into_iter()
            .map(|(adapter, seen)| Grade {
                adapter: adapter.to_string(),
                seen,
                of: 5,
                note: None,
            })
            .collect();
        self
    }

    /// The same host, with a device that turns away every passphrase but this one.
    pub(crate) fn refusing_without(mut self, code: &str) -> FakeEffects {
        self.accepts = Some(code.to_string());
        self
    }

    /// The same host, with these device names already configured.
    pub(crate) fn with_taken<const N: usize>(mut self, names: [&str; N]) -> FakeEffects {
        self.taken = names.iter().map(|name| (*name).to_string()).collect();
        self
    }

    /// The same host, with the config-file check warning and offering this fix.
    pub(crate) fn with_fix(self, command: &str, applies_itself: bool) -> FakeEffects {
        self.with_fix_on(
            "Config file",
            "there is no config file yet",
            command,
            applies_itself,
        )
    }

    /// The same host, with `check` warning for `reason` and offering this fix.
    ///
    /// Every row a frame draws names the check it is about, so a case that wants two of them
    /// gives each its own name rather than showing the same row twice.
    pub(crate) fn with_fix_on(
        mut self,
        check: &'static str,
        reason: &str,
        command: &str,
        applies_itself: bool,
    ) -> FakeEffects {
        self.outcomes.push(Outcome {
            check,
            state: State::Warn,
            reason: reason.to_string(),
            fix: Some(Fix {
                command: command.to_string(),
                applies_itself,
            }),
        });
        self
    }

    /// The same host, as this kind of machine.
    pub(crate) fn on(mut self, platform: Platform) -> FakeEffects {
        self.platform = platform;
        self
    }

    /// The fixes that were applied, in order.
    pub(crate) fn applied(&self) -> Vec<String> {
        self.record.lock().expect("the record").applied.clone()
    }

    /// How many scans have run.
    pub(crate) fn scans(&self) -> usize {
        self.record.lock().expect("the record").scans
    }

    /// The passphrases a connection was tried with, in order.
    pub(crate) fn tried(&self) -> Vec<String> {
        self.record.lock().expect("the record").tried.clone()
    }

    /// The device that was written.
    pub(crate) fn written_device(&self) -> Option<DeviceConfig> {
        self.record.lock().expect("the record").device.clone()
    }

    /// The adapter that was written.
    pub(crate) fn written_adapter(&self) -> Option<String> {
        self.record.lock().expect("the record").adapter.clone()
    }

    /// Whether the daemon was installed.
    pub(crate) fn daemon_installed(&self) -> bool {
        self.record.lock().expect("the record").daemon
    }

    /// How many times a running daemon was told to read the file again.
    pub(crate) fn reloads(&self) -> usize {
        self.record.lock().expect("the record").reloads
    }
}

#[async_trait(?Send)]
impl Host for FakeEffects {
    async fn checks(&self) -> Vec<Outcome> {
        self.outcomes.clone()
    }

    async fn apply(&self, fix: &Fix) -> Result<String, String> {
        let mut record = self.record.lock().expect("the record");
        record.applied.push(fix.command.clone());
        Ok(format!("applied {}", fix.command))
    }

    fn platform(&self) -> Platform {
        self.platform
    }

    fn nixos_snippet(&self) -> Vec<String> {
        vec!["systemd.user.sockets.svolderd = {".to_string()]
    }

    fn registry(&self) -> Option<&Registry> {
        self.registry.as_ref()
    }
}

#[async_trait(?Send)]
impl Radio for FakeEffects {
    async fn scan(&self) -> Result<Scanned, Failure> {
        self.record.lock().expect("the record").scans += 1;
        if !self.scan_takes.is_zero() {
            tokio::time::sleep(self.scan_takes).await;
        }
        if let Some(why) = self.scan_fails.as_deref() {
            return Err(Failure::of(crate::recovery::Kind::AdapterMissing, why));
        }
        Ok(Scanned {
            entries: self.seen.clone(),
            notices: self.notices.clone(),
        })
    }

    fn adapters(&self) -> Vec<String> {
        self.grades
            .iter()
            .map(|grade| grade.adapter.clone())
            .collect()
    }

    async fn grade(&self, _udid: &str) -> Vec<Grade> {
        self.grades.clone()
    }

    async fn try_connect(&self, _seen: &ScanEntry, passphrase: &str) -> Result<Connect, Failure> {
        self.record
            .lock()
            .expect("the record")
            .tried
            .push(passphrase.to_string());
        match self.accepts.as_deref() {
            Some(wanted) if wanted != passphrase => Ok(Connect::Refused),
            _ => Ok(Connect::Accepted),
        }
    }
}

#[async_trait(?Send)]
impl Files for FakeEffects {
    async fn write_device(&self, device: &DeviceConfig) -> Result<(), String> {
        if !self.write_takes.is_zero() {
            tokio::time::sleep(self.write_takes).await;
        }
        if !self.writes {
            return Err("config.toml is read-only".to_string());
        }
        if let Some(path) = self.path.as_deref() {
            svolder_ipc::Config::add_device(path, device).map_err(|e| e.to_string())?;
        }
        self.record.lock().expect("the record").device = Some(device.clone());
        Ok(())
    }

    async fn reload_daemon(&self) {
        self.record.lock().expect("the record").reloads += 1;
    }

    fn write_adapter(&self, adapter: &str) -> Result<(), String> {
        if !self.writes {
            return Err("config.toml is read-only".to_string());
        }
        self.record.lock().expect("the record").adapter = Some(adapter.to_string());
        Ok(())
    }

    async fn install_daemon(&self) -> Result<String, String> {
        if !self.writes {
            return Err("the units were not installed".to_string());
        }
        self.record.lock().expect("the record").daemon = true;
        Ok("installed the systemd user units".to_string())
    }

    fn taken_names(&self) -> Vec<String> {
        self.taken.clone()
    }
}

/// One device heard, advertising the product it is, so the registry has something to match.
pub(crate) fn seen_product(udid: &str, product: &str, rssi: i16) -> ScanEntry {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let product_id = registry
        .names()
        .iter()
        .filter_map(|name| registry.get(name))
        .find(|profile| profile.display == product)
        .and_then(|profile| profile.product_id)
        .unwrap_or_else(|| panic!("no profile describes {product}"));
    ScanEntry {
        unit: udid.to_string(),
        product_id,
        product: Some(product.to_string()),
        firmware: 3,
        passphrase: false,
        rssi: Some(rssi),
    }
}

/// One device heard that names no product, so no profile covers it.
pub(crate) fn seen(udid: &str) -> ScanEntry {
    ScanEntry {
        unit: udid.to_string(),
        product_id: 0,
        product: None,
        firmware: 3,
        passphrase: false,
        rssi: Some(-50),
    }
}

/// Take the default answer to every step until the wizard is on `step`.
pub(crate) async fn walk_to(wizard: &mut Wizard<FakeEffects>, step: Step) {
    for _ in 0..20 {
        let prompt = wizard.prompt().await.expect("a step to answer");
        if prompt.step == step {
            return;
        }
        wizard
            .answer(prompt.default, None)
            .await
            .expect("the default answer");
    }
    panic!("the wizard never reached {step:?}");
}
