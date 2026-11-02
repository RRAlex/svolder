// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A daemon that holds other devices than the configuration file names.
//!
//! A daemon reads its file when it starts, so a file edited or moved away since leaves it
//! holding devices the file no longer lists. A front end opening at start asks it to read the
//! file again, once, and only when the daemon reads this very file. A daemon that reads
//! another file that exists here is left alone and named in a note.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use svolder_ipc::{Client, Config, Request, Response};

use super::{ASK_WAIT, Globals, asks_a_daemon, launch, remote};

/// Tell a running daemon to read `path` again when it reads that file and holds other devices
/// than `config` names, and answer with the note to show for it. A daemon that reads another
/// file is never told to reload; when that file exists on this machine the note says which
/// file it reads instead, from the answer the version probe already gave.
///
/// No launcher is run to be asked: a daemon that is not already answering on the resolved
/// socket, one not saying which file it reads, and one holding the same device names are all
/// left alone, and nothing comes back for them. The ask is a plain connect, so
/// a host whose socket unit is listening starts the daemon behind it, which reads the current
/// file and so holds the same names. The whole check shares one [`ASK_WAIT`]: a daemon that
/// has not finished its answers by then is given up on quietly, with no reload and no note,
/// so the start goes on.
pub async fn reload_if_stale(g: &Globals<'_>, config: &Config, path: &Path) -> Option<String> {
    if !asks_a_daemon(g.transport) {
        return None;
    }
    tokio::time::timeout(ASK_WAIT, check(g, config, path))
        .await
        .ok()?
}

/// The check [`reload_if_stale`] bounds: the version, the device list, then the reload.
async fn check(g: &Globals<'_>, config: &Config, path: &Path) -> Option<String> {
    let socket = launch::probe(g, config).await?;
    let info = svolder_ipc::socket::daemon_version::version_within(&socket, ASK_WAIT).await?;
    let reads = Path::new(info.config.as_deref()?);
    if !same_file(reads, path) {
        // A path that is not there is another namespace's view of its own file (a container's
        // `/config/...`), which is no news worth a note on every start.
        if !reads.exists() {
            return None;
        }
        let home = std::env::var_os("HOME");
        return Some(format!(
            "svolderd reads {}, not {}",
            tilde(reads, home.as_deref().map(Path::new)),
            tilde(path, home.as_deref().map(Path::new))
        ));
    }
    let held = remote::device_infos(&socket).await.ok()?;
    let held: Vec<&str> = held.iter().map(|d| d.name.as_str()).collect();
    let named: Vec<&str> = config.devices.iter().map(|d| d.name.as_str()).collect();
    if lowered(&held) == lowered(&named) {
        return None;
    }
    let mut client = Client::connect(&socket).await.ok()?;
    let shown = path.display();
    let reply = client.request(Request::Reload).await;
    Some(match reply {
        Ok(Response::Reloaded { .. }) => format!("svolderd re-read {shown}"),
        Ok(Response::Error { message, .. }) => {
            format!("svolderd did not re-read {shown}: {message}")
        }
        _ => format!("svolderd did not re-read {shown}"),
    })
}

/// Tell the daemon listening on `socket` to read its configuration file again, but only when
/// its `version` reply names `path`, the file just written. A daemon that reads another file, one
/// that names none, one that does not answer in time and a reload that fails are all left
/// alone, and nothing is said of any of them: the preset writers use this, and a daemon that
/// reads their file is the only one their change reaches.
pub async fn reload_if_reads(socket: PathBuf, path: &Path) {
    let Some(info) = svolder_ipc::socket::daemon_version::version_within(&socket, ASK_WAIT).await
    else {
        return;
    };
    let Some(reads) = info.config.as_deref() else {
        return;
    };
    if !same_file(Path::new(reads), path) {
        return;
    }
    if let Ok(mut client) = Client::connect(&socket).await {
        let _ = client.request(Request::Reload).await;
    }
}

/// `path` as the note writes it: `~` for the home directory it is under, the path as it is
/// otherwise.
fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.filter(|home| !home.as_os_str().is_empty()) {
        Some(home) => match path.strip_prefix(home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
}

/// The device names as a set, case aside.
fn lowered(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_lowercase()).collect()
}

/// Whether two paths name the same file: each is made canonical, and a path that is not there
/// stands as it was written.
fn same_file(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(a) == canonical(b)
}

#[cfg(test)]
mod tests;
