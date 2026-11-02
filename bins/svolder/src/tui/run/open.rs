// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What opens a device for the TUI: the backend an open answers with, the traits a test supplies
//! its own implementations of, the launcher chain and the joined connect, and the struct that
//! holds what opening needs.

use super::*;

/// What opening answers with: a backend on one device, and the devices there are to switch
/// between.
pub(super) type Opened = (Backend, Vec<DeviceRow>);

/// What opens a backend for the TUI.
///
/// A trait rather than a call, so a test can supply its own backend: opening for real reads the
/// user's configuration and reaches for a radio, neither of which belongs in a test of what the
/// loop does with what it opened.
pub(in crate::tui) trait Open {
    /// Open a backend on the device `globals` names, with the configuration in `config`, and
    /// list the devices to switch between.
    fn open(
        &self,
        globals: OwnedGlobals,
        config: &Path,
    ) -> impl Future<Output = Result<Opened, Failure>>;

    /// A backend that reads no device, over the daemon on `socket` or else the one that answers
    /// ([`Backend::hold`]): on the device `wanted` names when the daemon lists it as not
    /// connected, and on none for `None`. Nothing is started and the device is not asked.
    /// Required, like [`Open::open_on`], so an opener cannot forget it.
    fn hold(
        &self,
        socket: Option<PathBuf>,
        globals: OwnedGlobals,
        wanted: Option<String>,
        config: &Path,
    ) -> impl Future<Output = Option<Held>>;

    /// Open a backend on the device `globals` names over the daemon listening on `socket` and
    /// nothing else: it starts no daemon and falls back to no direct session. Required, so an
    /// opener that forgets it does not compile rather than walk the chain here.
    fn open_on(
        &self,
        socket: PathBuf,
        globals: OwnedGlobals,
    ) -> impl Future<Output = Result<Opened, Failure>>;
}

/// What the TUI opens through: the launcher chain, and for a daemon that came back, a connect
/// to its socket alone.
///
/// In the demo, and on a daemon `svolder tui --join` named, every open is that connect, to that
/// daemon's socket: the initial open, `r`, a device switch and the Devices screen all come
/// through [`Opener::reopen_via`], and this is where it chooses, so no path of the TUI can walk
/// the chain and start a daemon on, or a session beside, that socket.
pub(in crate::tui) struct Live {
    /// The only daemon's socket, when this is the demo or a joined daemon.
    pub(in crate::tui) only: Option<PathBuf>,
}

impl Open for Live {
    async fn open(&self, globals: OwnedGlobals, config: &Path) -> Result<Opened, Failure> {
        match &self.only {
            Some(socket) => joined(socket, globals.device.as_deref()).await,
            None => open_device(globals, config).await,
        }
    }

    async fn hold(
        &self,
        socket: Option<PathBuf>,
        globals: OwnedGlobals,
        wanted: Option<String>,
        config: &Path,
    ) -> Option<Held> {
        // The demo and a joined daemon are asked on their own socket, whatever the chain says.
        let socket = socket.or_else(|| self.only.clone());
        Backend::hold(
            &globals.borrow(),
            socket.as_deref(),
            wanted.as_deref(),
            config,
        )
        .await
    }

    async fn open_on(&self, socket: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        // A daemon that came back is looked for where it was, which for the demo and a joined
        // daemon is the same socket as ever.
        joined(&socket, globals.device.as_deref()).await
    }
}

#[cfg(test)]
impl<O, F> Open for O
where
    O: Fn(OwnedGlobals) -> F,
    F: Future<Output = Result<Opened, Failure>>,
{
    fn open(
        &self,
        globals: OwnedGlobals,
        _config: &Path,
    ) -> impl Future<Output = Result<Opened, Failure>> {
        self(globals)
    }

    // Test closures only: they hold no daemon, so closure openers never wait. A case that waits
    // on one names a stand-in opener of its own.
    fn hold(
        &self,
        _socket: Option<PathBuf>,
        _globals: OwnedGlobals,
        _wanted: Option<String>,
        _config: &Path,
    ) -> impl Future<Output = Option<Held>> {
        std::future::ready(None)
    }

    // Test closures only: they open a stand-in backend and have no chain to leave out.
    fn open_on(
        &self,
        _socket: PathBuf,
        globals: OwnedGlobals,
    ) -> impl Future<Output = Result<Opened, Failure>> {
        self(globals)
    }
}

/// Open a session on the daemon on `socket` and nothing else. With no daemon answering there the
/// failure says so, and that nothing was started, which is where a joined start lands.
async fn joined(socket: &std::path::Path, device: Option<&str>) -> Result<Opened, Failure> {
    match Backend::open_on(socket, device).await {
        Err(_) if !svolder_ipc::is_running(socket).await => Err(Failure {
            code: Kind::NoDaemon.code(),
            what: format!(
                "no daemon answers on {}; nothing was started",
                socket.display()
            ),
            next: "Start the daemon, or `svolder tui --demo`, and join its socket".to_string(),
        }),
        other => other,
    }
}

/// Open a backend the way every other verb does, beside the devices there are to switch between.
///
/// A device list that cannot be read is no reason not to run: the TUI works on the device it
/// has, with nothing to switch to.
///
/// The list is asked of whatever daemon answers by then rather than through the launcher chain a
/// second time: the open above already walked it, and walking it again would, on a machine where
/// the daemon won't start, mean two attempts to start it for a single open.
pub(super) async fn open_device(globals: OwnedGlobals, config: &Path) -> Result<Opened, Failure> {
    let g = globals.borrow();
    crate::backend::known_transport(g.transport)?;
    let config = Backend::load_config_for(&g, config)?;
    open_device_with(globals, &config).await
}

/// [`open_device`] over a configuration already read. A daemon's own device list, which its
/// open read, is the list; only a direct session asks for one.
pub(super) async fn open_device_with(
    globals: OwnedGlobals,
    config: &Config,
) -> Result<Opened, Failure> {
    let g = globals.borrow();
    let backend = Backend::open_with(&g, config, true).await?;
    let devices = match backend.listed_rows() {
        Some(rows) => rows,
        None => Backend::devices_in(&g, config).await.unwrap_or_default(),
    };
    Ok((backend, devices))
}

/// How the loop walks someone through adding a device.
///
/// A trait rather than a call, for the same reason [`Open`] is one: building the wizard looks the
/// host over (its checks, its adapters, the configuration file it is about to write), none of
/// which belongs in a test of what the loop does with the walk.
pub(in crate::tui) trait Adding {
    /// What the walk applies to the machine it runs on.
    type Effects: crate::wizard::Effects;

    /// A wizard standing at the scan: where adding a device to a configuration that already works
    /// begins, since the host has been running this TUI and so needs no checks.
    fn wizard(&self) -> impl Future<Output = Result<Wizard<Self::Effects>, Failure>>;
}

/// How the TUI opens a device: what it opens under, how the app it fills draws, the
/// configuration file it reads the devices from, and the opening itself.
pub(in crate::tui) struct Opener<O> {
    /// The command line, owned, so an open inside the loop can name another device. `-v` is
    /// kept as it was typed and dropped per open, by [`Opener::reopen`].
    pub(super) globals: OwnedGlobals,
    /// How the app it fills draws.
    pub(super) tui: TuiConfig,
    /// The configuration file the device list is read from and the editor writes to.
    pub(super) path: PathBuf,
    /// What opens a backend.
    pub(super) open: O,
    /// How long a daemon's session waits for the daemon to come back.
    pub(super) daemon_wait: Duration,
}

#[cfg(test)]
mod tests;
