// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon install` and `daemon uninstall`: the systemd user units, written for this machine and
//! enabled, or taken away again.
//!
//! What a machine makes of a background service decides what an install can do: NixOS generates
//! its units, so it is shown a home-manager block instead; a container's daemon is the
//! container's own process; root writes the units for every user and leaves each to enable
//! them. The result comes back as lines rather than as printing, so the wizard and `doctor` can
//! put them where their own words go.

use std::path::{Path, PathBuf};

use svolder_ipc::checks::SystemdFacts;
use svolder_ipc::{Config, checks, socket};

use crate::recovery::{Failure, Kind};

use super::lifecycle::{ask_to_stop, wait_for_the_socket_to_close};
use super::{Ctl, RUNTIME_LISTEN_STREAM, SERVICE_UNIT, SOCKET_UNIT, trouble, units_stopped};

/// The unit files, as the repository writes them, with `__BINDIR__` and `__SOCKET__` still in
/// them: [`install`] fills both in for the machine it is installing on.
const SOCKET_TEMPLATE: &str = include_str!("../../../../../systemd/svolderd.socket");
/// The service unit's own template, read the same way.
const SERVICE_TEMPLATE: &str = include_str!("../../../../../systemd/svolderd.service");

/// The path the socket unit listens on now, when it is running there and that is not `socket`:
/// a running socket unit keeps the path it was started on until it is stopped, so rewriting its
/// file moves nothing by itself.
///
/// The paths are compared as written: a runtime directory that is a symlink can read as
/// "elsewhere" when it is the same place, which costs one needless restart.
fn listening_elsewhere(units: &SystemdFacts, socket: &Path) -> Option<PathBuf> {
    units.socket_unit.filter(|unit| unit.running)?;
    units
        .listen_stream
        .clone()
        .filter(|listen| listen != socket)
}

/// What an install did.
#[derive(Debug, PartialEq, Eq)]
pub enum Installed {
    /// The unit files were written and enabled.
    Units,
    /// The unit files were written and nothing was enabled: root has no user session, so each
    /// user enables them.
    Written,
    /// Nothing was written, and why: a machine that generates its units, or a container, which
    /// is a machine where the daemon is the process the container runs.
    Skipped(String),
}

/// Write the systemd user units for this machine and enable them, so the daemon is there at
/// the next login and holds its device sessions.
///
/// `on_demand` enables the socket alone: the daemon then starts with the first command that
/// needs it and the machine runs nothing until then.
///
/// The units are written for the socket the daemon itself resolves — `SVOLDER_SOCKET`, the
/// config's `socket` key, else the session's runtime directory — because that is the path it
/// will listen on however this command was typed. Where they are written is
/// `$SVOLDER_UNITDIR`, else (as root) `/etc/systemd/user`, else `$XDG_CONFIG_HOME/systemd/user`,
/// else `~/.config/systemd/user`.
pub async fn install(on_demand: bool) -> Result<Installed, Failure> {
    let install = install_quietly(on_demand).await?;
    install.say();
    install.done
}

/// [`install`] with nothing printed: what it did and the lines that say so come back instead.
///
/// A front end that draws its own screen has no terminal to print on while it is drawing, so it
/// takes the lines and puts them where its own words go. The `Err` here is a machine that could
/// not be looked at at all; one that stopped partway through the install says so in
/// [`Install::done`], beside the lines saying how far it got.
pub async fn install_quietly(on_demand: bool) -> Result<Install, Failure> {
    Ok(install_into(&Ctl::live(), &Target::live()?, on_demand).await)
}

/// What an install did, and what there is to say about it.
pub struct Install {
    /// Whether the units were written, or what stopped it partway.
    pub done: Result<Installed, Failure>,
    /// What happened as far as it got, one line each, without the `svolder: ` a verb prints
    /// them behind.
    pub said: Vec<String>,
    /// The home-manager block a machine that generates its unit files is shown instead of
    /// having them written, when it is one.
    pub block: Option<String>,
}

impl Install {
    /// An install that stopped partway, with what it had done by then.
    ///
    /// The steps before the one that stopped still happened — the files are written whether or
    /// not systemd was told about them — so they are still said.
    fn stopped(said: Vec<String>, why: Failure) -> Install {
        Install {
            done: Err(why),
            said,
            block: None,
        }
    }

    /// Put what happened on standard error, where this verb's own words go.
    ///
    /// The block goes first and unprefixed, because it is meant to be copied into a
    /// configuration file rather than read as a message.
    pub fn say(&self) {
        if let Some(block) = self.block.as_deref() {
            eprint!("{block}");
        }
        for line in &self.said {
            eprintln!("svolder: {line}");
        }
    }
}

/// The home-manager block this machine's units would be generated from.
///
/// It is what [`install`] prints on a machine that keeps no unit files of its own, so the setup
/// wizard can show the same block under its own question instead of sending the reader to a verb
/// that would only print it.
pub fn home_manager_block() -> Result<String, Failure> {
    Target::live().map(|target| home_manager_snippet(&target))
}

/// Take the systemd user units out again, leaving the binaries and the configuration alone.
pub fn uninstall() -> Result<(), Failure> {
    uninstall_from(&Ctl::live(), &Target::live()?)
}

/// The environment variable that names where the unit files go, which is what lets `make
/// install-service UNITDIR=<dir>` and this verb write the same one pair of files.
use svolder_ipc::config::UNITDIR_ENV;

/// What a machine makes of a background service, and so what the daemon can be offered.
///
/// The order is the one `install` reads them in: a machine that generates its units is that
/// before it is a container, and a container before a machine with systemd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Service {
    /// The units are generated from a home-manager block instead.
    Snippet,
    /// The daemon is the container's own process.
    Container,
    /// Units can be written and enabled.
    Install,
    /// There is no systemd here to install anything into.
    None,
}

impl Service {
    /// Which of those a machine with these facts is.
    pub(crate) fn of(nixos: bool, container: bool, systemd: bool) -> Service {
        if nixos {
            Service::Snippet
        } else if container {
            Service::Container
        } else if systemd {
            Service::Install
        } else {
            Service::None
        }
    }
}

/// What a container's daemon is: the sentence the wizard and `install` both say.
pub(crate) const CONTAINER_DAEMON: &str =
    "the daemon is the container's own process; run svolderd as the image's command";

/// Where the units go and what the machine makes of them.
struct Target {
    /// The directory the unit files are written into.
    dir: PathBuf,
    /// The directory the service starts `svolderd` from, which is where this binary lives.
    bindir: PathBuf,
    /// The socket the units listen on.
    socket: PathBuf,
    /// The socket `%t/svolder.sock` means on this machine, when it has a runtime directory: a
    /// socket equal to it is written as the specifier rather than as a path.
    runtime_socket: Option<PathBuf>,
    /// Whether this machine generates its unit files rather than keeping them (NixOS).
    nixos: bool,
    /// Whether this process runs in a container, where the daemon is the container itself.
    container: bool,
    /// Whether this process runs as root, which has no user session to enable units in.
    root: bool,
    /// Whether the socket was named, by `SVOLDER_SOCKET` or the config's `socket` key, rather
    /// than left to the runtime directory.
    named_socket: bool,
}

impl Target {
    /// This machine.
    fn live() -> Result<Target, Failure> {
        let config =
            Config::load().map_err(|e| Failure::of(Kind::ConfigUnreadable, e.to_string()))?;
        let socket = socket::path::resolve(None, config.socket.as_deref());
        let env = checks::live::env_facts(&socket);
        let named = |value: Option<&str>| value.is_some_and(|v| !v.trim().is_empty());
        let named_socket = named(std::env::var("SVOLDER_SOCKET").ok().as_deref())
            || named(config.socket.as_deref());
        let bindir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .ok_or_else(|| {
                trouble(
                    "this binary's own directory could not be read".to_string(),
                    "Run `svolder daemon install` from an installed svolder",
                )
            })?;
        Ok(Target {
            // SAFETY: geteuid has no preconditions and cannot fail.
            dir: unit_dir(unsafe { libc::geteuid() } == 0),
            bindir,
            runtime_socket: env.runtime_dir.map(|dir| dir.join("svolder.sock")),
            socket,
            nixos: env.nixos,
            container: env.container,
            // SAFETY: geteuid has no preconditions and cannot fail.
            root: unsafe { libc::geteuid() } == 0,
            named_socket,
        })
    }

    /// The two unit files, each with its name and the text this machine gets.
    fn units(&self) -> [(&'static str, String); 2] {
        [
            (SOCKET_UNIT, self.render(SOCKET_TEMPLATE)),
            (SERVICE_UNIT, self.render(SERVICE_TEMPLATE)),
        ]
    }

    /// One unit file, with what the repository leaves to the machine filled in.
    fn render(&self, template: &str) -> String {
        template
            .replace("__BINDIR__", &self.bindir.display().to_string())
            .replace("__SOCKET__", &self.listen_stream())
    }

    /// What `ListenStream` says: systemd's specifier for the runtime directory when that is
    /// where the socket is, and the path itself when the user put it somewhere else.
    ///
    /// Units written by root are for every user, so unless a socket was named they get the
    /// specifier: the path root itself resolves belongs to root alone.
    fn listen_stream(&self) -> String {
        if self.root && !self.named_socket {
            return RUNTIME_LISTEN_STREAM.to_string();
        }
        match self.runtime_socket.as_deref() == Some(self.socket.as_path()) {
            true => RUNTIME_LISTEN_STREAM.to_string(),
            false => self.socket.display().to_string(),
        }
    }
}

/// [`install`] into a given target, with given programs, saying nothing itself.
///
/// What it did comes back as lines for the caller to put where its own words go: the verb
/// prints them on standard error, the home-manager block included, so that a caller whose
/// standard output is a JSON array — `doctor --json --yes` — still gets one.
async fn install_into(ctl: &Ctl, target: &Target, on_demand: bool) -> Install {
    // `install` runs the systemd work whenever the machine is neither of the first two, so
    // systemd is taken as present.
    match Service::of(target.nixos, target.container, true) {
        Service::Snippet => {
            let why = "NixOS generates its unit files, so nothing was written; put the block \
                       above in your home-manager configuration and apply it";
            return Install {
                done: Ok(Installed::Skipped(why.to_string())),
                said: vec![why.to_string()],
                block: Some(home_manager_snippet(target)),
            };
        }
        Service::Container => {
            let why =
                format!("in a container {CONTAINER_DAEMON}, rather than installing user units");
            return Install {
                done: Ok(Installed::Skipped(why.clone())),
                said: vec![why],
                block: None,
            };
        }
        Service::Install | Service::None => {}
    }

    // Whether the daemon this machine has is systemd's, read before the unit files are written
    // and systemd is told about them: one that answers while neither unit runs (none installed,
    // or both stopped) is one a front end started, nobody's to stop but this verb's. A
    // `systemctl` that will not run at all means there is no unit to take the socket and this
    // install is about to fail without ever asking for one, so the daemon that is serving is
    // left serving. Root asks systemd nothing: it has no user session to ask. A socket unit about
    // to move to this path is the same case: the daemon answering here is not the units', which
    // listen elsewhere until they are stopped.
    let (take_the_socket, moved_from) = match target.root {
        true => (false, None),
        false => {
            let (units, systemd_answered) = ctl.asked_units();
            let moved_from =
                listening_elsewhere(&units, &target.socket).filter(|_| systemd_answered);
            (
                systemd_answered && (units_stopped(&units) || moved_from.is_some()),
                moved_from,
            )
        }
    };

    let mut said = Vec::new();
    if let Err(e) = std::fs::create_dir_all(&target.dir) {
        return Install::stopped(said, cannot_write(&target.dir.join(SOCKET_UNIT), e));
    }
    for (name, text) in target.units() {
        let path = target.dir.join(name);
        if let Err(e) = std::fs::write(&path, text) {
            return Install::stopped(said, cannot_write(&path, e));
        }
        if !target.root {
            said.push(format!("wrote {}", path.display()));
        }
    }
    if target.root {
        said.push(format!(
            "wrote {SOCKET_UNIT} and {SERVICE_UNIT} into {}, for every user",
            target.dir.display()
        ));
        said.push(format!(
            "each user enables them with: systemctl --user enable --now {}",
            enabled_units(on_demand)
        ));
        return Install {
            done: Ok(Installed::Written),
            said,
            block: None,
        };
    }

    // One daemon holds one device. A daemon a front end started is listening on the very path
    // the socket unit is about to take: systemd unlinks the file and binds its own listener,
    // leaving that daemon running, holding the device, answering to nobody — and the service
    // systemd then starts finds the device taken. So it is asked to stop first.
    if take_the_socket && svolder_ipc::is_running(&target.socket).await {
        if let Err(failure) = ask_to_stop(&target.socket).await {
            return Install::stopped(said, failure);
        }
        if let Err(failure) = wait_for_the_socket_to_close(&target.socket).await {
            return Install::stopped(said, failure);
        }
        said.push(format!(
            "stopped the daemon svolder had started on {}",
            target.socket.display()
        ));
    }

    let ask = |args: &[&str]| -> Result<(), Failure> {
        ctl.systemctl(args)
            .map_err(|why| trouble(why, "Run `svolder daemon status`"))
    };
    let mut asked = vec![vec!["daemon-reload"]];
    // Both go down so `enable --now` starts the socket on the path just written and the service
    // on the listener it hands over; the service would refuse to let a socket start under it.
    if moved_from.is_some() {
        asked.push(vec!["stop", SERVICE_UNIT, SOCKET_UNIT]);
    }
    asked.push(vec!["enable", "--now", SOCKET_UNIT]);
    if !on_demand {
        asked.push(vec!["enable", "--now", SERVICE_UNIT]);
    }
    for args in asked {
        if let Err(failure) = ask(&args) {
            return Install::stopped(said, failure);
        }
        // Said as soon as the stop is done, so a later step that fails still reports it.
        if let (Some("stop"), Some(old)) = (args.first().copied(), &moved_from) {
            said.push(format!(
                "stopped {SOCKET_UNIT} and {SERVICE_UNIT}, which listened on {}",
                old.display()
            ));
        }
    }
    said.push(format!(
        "enabled {}",
        match on_demand {
            true =>
                format!("{SOCKET_UNIT}; the daemon starts with the first command that needs it"),
            false => format!("{SOCKET_UNIT} and {SERVICE_UNIT}"),
        }
    ));
    said.push(format!(
        "to keep the daemon running after logout: loginctl enable-linger {}",
        std::env::var("USER").unwrap_or_else(|_| "$USER".to_string())
    ));
    Install {
        done: Ok(Installed::Units),
        said,
        block: None,
    }
}

/// The units `enable --now` names: the socket alone when the daemon starts on demand.
fn enabled_units(on_demand: bool) -> String {
    match on_demand {
        true => SOCKET_UNIT.to_string(),
        false => format!("{SOCKET_UNIT} {SERVICE_UNIT}"),
    }
}

/// [`uninstall`] from a given target, with given programs.
///
/// Every step is taken whatever the ones before it did: a unit systemd has already forgotten
/// still has a file to remove, and a file that is gone is one less thing to disable.
fn uninstall_from(ctl: &Ctl, target: &Target) -> Result<(), Failure> {
    if target.nixos {
        eprintln!(
            "svolder: NixOS generates its unit files; take the svolderd block out of your \
             home-manager configuration and apply it"
        );
        return Ok(());
    }
    // Root has no user session: it removes the files and leaves each user to disable the units.
    if !target.root {
        for unit in [SERVICE_UNIT, SOCKET_UNIT] {
            if let Err(why) = ctl.systemctl(&["disable", "--now", unit]) {
                eprintln!("svolder: {why}");
            }
        }
    }
    let mut removed = Vec::new();
    for (name, _) in target.units() {
        let path = target.dir.join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => removed.push(path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(cannot_write(&path, e)),
        }
    }
    if !target.root
        && let Err(why) = ctl.systemctl(&["daemon-reload"])
    {
        eprintln!("svolder: {why}");
    }
    match removed.is_empty() {
        true => eprintln!("svolder: no unit files in {}", target.dir.display()),
        false => {
            for path in removed {
                eprintln!("svolder: removed {}", path.display());
            }
        }
    }
    if target.root {
        eprintln!(
            "svolder: each user who enabled them runs: systemctl --user disable --now {SOCKET_UNIT} {SERVICE_UNIT}"
        );
    }
    Ok(())
}

/// The home-manager block a NixOS machine puts the daemon in instead of unit files.
///
/// It says what the two unit files say, in the shape home-manager generates them from, so a
/// machine that writes no unit files still gets the socket and the service this project runs on.
fn home_manager_snippet(target: &Target) -> String {
    format!(
        "systemd.user.sockets.svolderd = {{\n\
         \x20 Socket = {{ ListenStream = \"{listen}\"; SocketMode = \"0600\"; }};\n\
         \x20 Install = {{ WantedBy = [ \"sockets.target\" ]; }};\n\
         }};\n\
         systemd.user.services.svolderd = {{\n\
         \x20 Unit = {{ Description = \"svolder Forkbeard daemon\"; \
         Requires = [ \"svolderd.socket\" ]; \
         After = [ \"svolderd.socket\" \"bluetooth.target\" ]; }};\n\
         \x20 Service = {{ ExecStart = \"{bindir}/svolderd\"; \
         ExecReload = \"/bin/kill -HUP $MAINPID\"; Restart = \"on-failure\"; RestartSec = 5; }};\n\
         \x20 Install = {{ WantedBy = [ \"default.target\" ]; }};\n\
         }};\n",
        listen = target.listen_stream(),
        bindir = target.bindir.display(),
    )
}

/// The directory systemd reads every user's units from, which is where root's units go.
const SYSTEM_WIDE_UNIT_DIR: &str = "/etc/systemd/user";

/// Where the unit files go: `$SVOLDER_UNITDIR` when something says so — `make install-service`
/// passes its own `UNITDIR` that way — else, for root, `/etc/systemd/user`, else the directory
/// systemd reads a user's own units from.
fn unit_dir(root: bool) -> PathBuf {
    unit_dir_named(root, std::env::var_os(UNITDIR_ENV))
}

/// [`unit_dir`] with the value of `$SVOLDER_UNITDIR` given rather than read.
fn unit_dir_named(root: bool, named: Option<std::ffi::OsString>) -> PathBuf {
    named
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| match root {
            true => PathBuf::from(SYSTEM_WIDE_UNIT_DIR),
            false => config_home().join("systemd").join("user"),
        })
}

/// `$XDG_CONFIG_HOME`, or `~/.config` (the account's home when `$HOME` is unset): the directory systemd reads a user's own units from.
fn config_home() -> PathBuf {
    svolder_ipc::config::config_home()
}

/// A file this verb needed to write and could not.
fn cannot_write(path: &Path, e: std::io::Error) -> Failure {
    trouble(
        format!("{}: {e}", path.display()),
        "Check that the directory can be written to",
    )
}

#[cfg(test)]
mod tests;
