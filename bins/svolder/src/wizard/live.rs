// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wizard against the machine it is running on.
//!
//! Everything [`super::Effects`] can do is done here by the code the verbs already do it with: the
//! checks and their fixes are `doctor`'s, the windows are [`scanning`]'s, the writes are the
//! configuration file's own, and the units are `daemon install`'s. Nothing is its own second
//! implementation of any of them.
//!
//! The host is looked at once, when this is built, and every answer about the machine comes
//! from that one snapshot, so the rows the wizard shows, the adapters it grades and what it says
//! about the daemon are all about the same moment.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use svolder_ipc::checks::live::LiveSnapshot;
use svolder_ipc::checks::{self, Fix, Outcome, Probes};
use svolder_ipc::{Config, DeviceConfig, ScanEntry, socket};

use svolder_profiles::Registry;

use super::{Connect, Files, Host, Platform, Radio};
use crate::backend::{Globals, OwnedGlobals};
use crate::recovery::{Failure, Kind, next_step};
use crate::scanning::{self, Grade, Scanned};

/// The machine the wizard is walking over.
pub struct LiveEffects {
    /// What the run was given, kept because a session is opened long after the command line
    /// that named it has been parsed.
    g: OwnedGlobals,
    /// The configuration as it stood when the wizard started, for the socket it names and the
    /// device names it already carries.
    config: Config,
    /// The configuration file the wizard writes into.
    path: PathBuf,
    /// The daemon socket this run resolves to.
    socket: PathBuf,
    /// What the host said when it was asked, once.
    snapshot: LiveSnapshot,
    /// The profiles this build reads devices through, absent when they could not be read.
    registry: Option<Registry>,
}

impl LiveEffects {
    /// Look the host over and answer with the effects a wizard over it has.
    ///
    /// The wizard reads and writes the configuration at `path`.
    ///
    /// A configuration that does not parse stops this: every step after the first writes into
    /// that file, and the writers refuse a file they cannot read, so there is nothing to be
    /// gained by walking up to the refusal.
    pub async fn live(g: &Globals<'_>, path: &Path) -> Result<LiveEffects, Failure> {
        let path = path.to_path_buf();
        let config = Config::load_from(&path)
            .map_err(|e| Failure::of(Kind::ConfigUnreadable, e.to_string()))?;
        let socket = socket::path::resolve(g.socket, config.socket.as_deref());
        let snapshot = checks::live::live(&socket, &path).await;
        Ok(LiveEffects {
            g: g.owned(),
            config,
            path,
            socket,
            snapshot,
            // A profile directory that cannot be read leaves every product unmatched, which is
            // the wizard's own question to ask rather than a reason not to start.
            registry: crate::backend::registry().ok(),
        })
    }
}

#[async_trait(?Send)]
impl Host for LiveEffects {
    /// The rows `doctor` shows that need no radio: the grades are the adapter step's own
    /// question, and the devices are what this run is about to add its first one to.
    async fn checks(&self) -> Vec<Outcome> {
        crate::commands::doctor::rows(
            &self.snapshot,
            &self.socket,
            &self.path,
            false,
            None,
            Vec::new(),
        )
    }

    async fn apply(&self, fix: &Fix) -> Result<String, String> {
        crate::commands::doctor::apply(fix, &self.path).await
    }

    fn platform(&self) -> Platform {
        let env = self.snapshot.env();
        Platform {
            systemd: self.snapshot.systemd().present,
            nixos: env.nixos,
            container: env.container,
        }
    }

    /// The block `daemon install` prints on a machine that generates its units, split into the
    /// lines a prompt shows.
    ///
    /// A machine whose own directories cannot be read has no block to show, and says nothing
    /// rather than half of one.
    fn nixos_snippet(&self) -> Vec<String> {
        crate::commands::daemon::install::home_manager_block()
            .map(|block| block.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    fn registry(&self) -> Option<&Registry> {
        self.registry.as_ref()
    }
}

#[async_trait(?Send)]
impl Radio for LiveEffects {
    /// What opening the backend had to say comes back beside what it heard, rather than going
    /// to a terminal a front end may be drawing on.
    async fn scan(&self) -> Result<Scanned, Failure> {
        scanning::find_any(&self.g.borrow()).await
    }

    fn adapters(&self) -> Vec<String> {
        self.snapshot
            .bluez()
            .adapters
            .into_iter()
            .map(|adapter| adapter.name)
            .collect()
    }

    /// The adapters BlueZ reported when the host was looked over, graded one after another.
    async fn grade(&self, udid: &str) -> Vec<Grade> {
        scanning::grade_adapters(&self.g.borrow(), udid, &self.adapters()).await
    }

    /// A device that turns the passphrase away is answered by the wizard's own passphrase step,
    /// so it comes back as a refusal rather than as a failure; anything else that stopped the
    /// session is the failure it was.
    async fn try_connect(&self, seen: &ScanEntry, passphrase: &str) -> Result<Connect, Failure> {
        match crate::backend::try_connect(&self.g.borrow(), seen, passphrase).await {
            Ok(()) => Ok(Connect::Accepted),
            Err(failure) if failure.next == next_step(Kind::Refused) => Ok(Connect::Refused),
            Err(failure) => Err(failure),
        }
    }
}

#[async_trait(?Send)]
impl Files for LiveEffects {
    /// Write the device into the configuration file, through the writer `svolder add` uses.
    ///
    /// Nothing is told about it here: the daemon is told once, when everything this run writes
    /// is in the file, so a daemon that holds the device does not hold it through the adapter
    /// grading. The file is the record either way, and the last step says what was written.
    async fn write_device(&self, device: &DeviceConfig) -> Result<(), String> {
        Config::add_device(&self.path, device).map_err(|e| e.to_string())
    }

    /// Tell a daemon that is running to re-read the file, exactly as `svolder add` does; no
    /// daemon is started to be told.
    ///
    /// What the daemon said is not printed: the front end walking this may be drawing on the
    /// alternate screen, where a line would land in the middle of a frame.
    async fn reload_daemon(&self) {
        crate::commands::device::reload_daemon_quietly(&self.g.borrow(), &self.config).await;
    }

    fn write_adapter(&self, adapter: &str) -> Result<(), String> {
        Config::set_adapter(&self.path, Some(adapter)).map_err(|e| e.to_string())
    }

    /// Install the units and enable both of them, which is what `svolder daemon install` with
    /// no flags does, saying nothing: what it did comes back as the lines that say so, and a
    /// machine that writes none is a failure, since nothing was installed.
    async fn install_daemon(&self) -> Result<String, String> {
        let install = match crate::commands::daemon::install::install_quietly(false).await {
            Ok(install) => install,
            Err(failure) => return Err(failure.to_string()),
        };
        // What it did before it stopped is worth saying either way: the unit files are written
        // whether or not systemd was told about them.
        let so_far = install.said.join("; ");
        match install.done {
            Ok(
                crate::commands::daemon::install::Installed::Units
                | crate::commands::daemon::install::Installed::Written,
            ) => Ok(install.said.join("\n")),
            Ok(crate::commands::daemon::install::Installed::Skipped(why)) => {
                Err(format!("the units were not installed: {why}"))
            }
            Err(failure) if so_far.is_empty() => Err(failure.to_string()),
            Err(failure) => Err(format!("{so_far}; {failure}")),
        }
    }

    fn taken_names(&self) -> Vec<String> {
        self.config
            .devices
            .iter()
            .map(|device| device.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wizard reads the file it is handed, not whatever `Config::path` resolves to: a file
    /// that does not parse stops it, in words quoting that file.
    #[tokio::test]
    async fn the_wizard_reads_the_file_it_is_handed() {
        let dir = crate::test_support::temp_dir("wizard-file");
        let path = dir.join("config.toml");
        std::fs::write(&path, "this is = not [valid\n").expect("the file is written");
        let g = Globals {
            transport: "fake",
            socket: None,
            adapter: None,
            device: None,
            verbose: false,
        };
        let failure = LiveEffects::live(&g, &path)
            .await
            .err()
            .expect("the file does not parse");
        assert!(failure.what.contains("this is = not"), "{failure}");
    }
}
