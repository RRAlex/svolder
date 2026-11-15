// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which writer a front end uses: the daemon's, whenever one runs, and its own only with none.
//!
//! The rule is one: while a daemon runs it is the one writer of the devices and the one that
//! walks a new one, so a front end that wrote the file itself would be a second authority. A
//! front end asks here, with the feature the work needs. No socket and nothing listening on it
//! is no daemon (the front end's own path stands); a socket that cannot be tried, a daemon that
//! does not say what it is and a daemon without the feature are each a refusal in words, and
//! nothing is written behind any of them.

use std::path::Path;

use svolder_ipc::socket::daemon_version::{VERSION_WAIT, version_on};
use svolder_ipc::{
    Client, Config, DEFAULT_REQUEST_TIMEOUT, DeviceConfigOp, FEATURE_DEVICE_CRUD,
    FEATURE_PRESET_WRITE, IpcError, PresetOp, Reason, Request, Response, request_on_own_connection,
};

use crate::recovery::{Failure, Kind};

/// Whether a daemon answers on `socket`, and has `feature`.
///
/// `Ok(false)`, no daemon, is the only answer that lets a front end use its own writer. A daemon
/// that lacks `feature` is refused with the step that brings it up to date.
pub(crate) async fn daemon_runs(socket: &Path, feature: &str, what: &str) -> Result<bool, Failure> {
    let mut client = match Client::connect_if_any(socket).await {
        Ok(Some(client)) => client,
        Ok(None) => return Ok(false),
        // A daemon may run behind a socket that cannot be tried.
        Err(why) => {
            return Err(Failure::of(
                Kind::Usage,
                format!(
                    "cannot reach the daemon's socket {}: {why}",
                    socket.display()
                ),
            ));
        }
    };
    let info = match tokio::time::timeout(VERSION_WAIT, version_on(&mut client)).await {
        Ok(Ok(reply)) => reply.info,
        Ok(Err(_)) | Err(_) => {
            return Err(Failure::of(
                Kind::DaemonError,
                format!(
                    "the daemon on {} did not say what it is: svolder daemon status",
                    socket.display()
                ),
            ));
        }
    };
    if !info.supports(feature) {
        return Err(Failure::of(
            Kind::DaemonTooOld,
            too_old_words(Some(&info.version), what),
        ));
    }
    Ok(true)
}

/// Send a `device_config` op to the daemon on `socket`, when one that has the family runs:
/// `Ok(None)` for no daemon, so the caller writes through its own path; the daemon's answer
/// (its refusal included, see [`refusal_words`]) otherwise.
pub(crate) async fn device_request(
    socket: &Path,
    op: DeviceConfigOp,
) -> Result<Option<Response>, Failure> {
    if !daemon_runs(socket, FEATURE_DEVICE_CRUD, "edit the devices").await? {
        return Ok(None);
    }
    let request = Request::DeviceConfig { op };
    match request_on_own_connection(socket, request, Some(DEFAULT_REQUEST_TIMEOUT)).await {
        Ok(answer) => Ok(Some(answer)),
        Err(IpcError::Refused(answer)) => Ok(Some(*answer)),
        Err(e) => Err(Failure::of(Kind::DaemonError, e.to_string())),
    }
}

/// Send a `preset_write` op to the daemon on `socket`, when one that has the family runs:
/// `Ok(None)` for no daemon, so the caller writes through its own path; the daemon's answer
/// (its refusal included) otherwise. A daemon without the family is refused in words, and
/// nothing is written behind it.
pub(crate) async fn preset_request(
    socket: &Path,
    op: PresetOp,
) -> Result<Option<Response>, Failure> {
    if !daemon_runs(socket, FEATURE_PRESET_WRITE, "write presets").await? {
        return Ok(None);
    }
    let request = Request::PresetWrite { op };
    match request_on_own_connection(socket, request, Some(DEFAULT_REQUEST_TIMEOUT)).await {
        Ok(answer) => Ok(Some(answer)),
        Err(IpcError::Refused(answer)) => Ok(Some(*answer)),
        Err(e) => Err(Failure::of(Kind::DaemonError, e.to_string())),
    }
}

/// Carry out one write of the preset library: through the daemon on `socket`, which judges the
/// name and says so in the words every surface uses, or, where no daemon answers there, through
/// the file's own writer. A daemon that cannot take the op is refused in words and nothing is
/// written.
pub(crate) async fn preset_write(socket: &Path, file: &Path, op: PresetOp) -> Result<(), Failure> {
    match preset_request(socket, op.clone()).await? {
        Some(Response::PresetWritten { .. }) => Ok(()),
        Some(Response::Error {
            message, reason, ..
        }) => Err(Failure::of(
            Kind::Usage,
            preset_refusal_words(reason, &message),
        )),
        Some(other) => Err(Failure::of(
            Kind::DaemonError,
            format!("the daemon sent an unexpected reply: {other:?}"),
        )),
        None => {
            let file = file.to_path_buf();
            tokio::task::spawn_blocking(move || preset_in_process(&file, op))
                .await
                .unwrap_or_else(|_| Err(Failure::of(Kind::DaemonError, "the preset write stopped")))
        }
    }
}

/// The preset write with no daemon to make it: the core's writers on the file, with the
/// daemon's meaning for each op (a `save` with `force` adds a preset that is not there).
pub(crate) fn preset_in_process(file: &Path, op: PresetOp) -> Result<(), Failure> {
    guarded(file, || in_process(file, op)).map_err(crate::commands::device::config_failure)
}

/// Run `write`, a call of the core's file writers on `file`, refusing a file that goes missing
/// before the writer reads it ([`ConfigError::Gone`](svolder_ipc::ConfigError::Gone)) when it
/// was there as the write began: a deletion in that window never starts a template file that a
/// daemon's next reload would take for the whole configuration. A file already absent is written
/// as it always was.
pub(crate) fn guarded<T>(
    file: &Path,
    write: impl FnOnce() -> Result<T, svolder_ipc::ConfigError>,
) -> Result<T, svolder_ipc::ConfigError> {
    match file.exists() {
        true => Config::while_file_must_exist(write),
        false => write(),
    }
}

/// [`preset_in_process`]'s writer calls.
fn in_process(file: &Path, op: PresetOp) -> Result<(), svolder_ipc::ConfigError> {
    let exists = |name: &str| Config::load_from(file).is_ok_and(|c| c.preset(name).is_some());
    match op {
        PresetOp::Save { preset, force } if force && exists(&preset.name) => {
            Config::update_preset(file, &preset.name, &preset)
        }
        PresetOp::Save { preset, .. } => Config::add_preset(file, &preset),
        PresetOp::Update { preset } => Config::update_preset(file, &preset.name, &preset),
        PresetOp::Rename { from, to } => Config::rename_preset(file, &from, &to),
        PresetOp::Delete { name } => Config::remove_preset(file, &name),
    }
}

/// What a refused preset write says: the daemon's own words, which are the core's for a name
/// (taken, blank, a built-in's) and the same ones the web page shows; a busy file and a daemon
/// that is stopping are worded as for a device write.
pub(crate) fn preset_refusal_words(reason: Option<Reason>, message: &str) -> String {
    match reason {
        Some(Reason::FileBusy | Reason::Stopping) => refusal_words(reason, message),
        _ => message.to_string(),
    }
}

/// What a refused device write says, worded from the typed reason at the control: the name
/// that is taken, the device that is gone, and for the rest the daemon's own words, which name
/// the file or the unit at fault and never a passphrase.
pub(crate) fn refusal_words(reason: Option<Reason>, message: &str) -> String {
    match reason {
        Some(Reason::Taken) => "another device already has that name".to_string(),
        Some(Reason::Missing) => "that device is no longer configured".to_string(),
        Some(Reason::FileBusy) => {
            "the configuration file is busy with another writer: try again".to_string()
        }
        Some(Reason::Stopping) => "the daemon is stopping: nothing was written".to_string(),
        _ => message.to_string(),
    }
}

/// The words for a daemon that lacks a feature this svolder asks of it: one wording for every
/// front end. `version` is the daemon's when it said it.
pub(crate) fn too_old_words(version: Option<&str>, what: &str) -> String {
    let daemon = match version {
        Some(version) => format!("the daemon ({version})"),
        None => "the daemon".to_string(),
    };
    format!("{daemon} cannot {what}, which this svolder asks it to")
}

/// Whether `failure` is a daemon that lacks the feature asked for ([`Kind::DaemonTooOld`]), rather
/// than one that could not be reached or answered.
pub(crate) fn is_too_old(failure: &Failure) -> bool {
    failure.code == Kind::DaemonTooOld.code()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file that is there as the write begins and gone before the writer reads it is refused
    /// as a daemon's missing file is, and no template file is made; a file already absent is
    /// written as it always was.
    #[test]
    fn a_file_deleted_under_a_cli_write_is_not_made_again() {
        let dir = crate::test_support::temp_dir("reach-guard");
        let path = dir.0.join("config.toml");
        std::fs::write(&path, "schema = 1\n").unwrap();
        let hop = svolder_ipc::config::Hop {
            what: svolder_ipc::config::HopWhat::Declared {
                kind: svolder_ipc::config::HopKind::Amp,
                name: "x".into(),
            },
            note: None,
            after: None,
        };
        let err = guarded(&path, || {
            std::fs::remove_file(&path).unwrap();
            Config::add_hop(&path, &hop)
        })
        .unwrap_err();
        assert!(matches!(err, svolder_ipc::ConfigError::Gone(_)), "{err:?}");
        assert!(!path.exists(), "the file was made again");
        let failure = crate::commands::device::config_failure(err);
        assert_eq!(failure.code, 1);
        assert!(failure.what.contains("is missing"), "{}", failure.what);

        guarded(&path, || Config::add_hop(&path, &hop)).expect("an absent file is started");
        assert!(path.exists());
    }
}
