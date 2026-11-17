// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who a session is talking to and how it stands: the device's name and unit, its link, what
//! stands in for it while none is open and the daemon behind it.

use super::Holding;

/// Who a session is talking to and how it stands: the device's name and unit, its link, what
/// stands in for it while none is open and the daemon behind it.
pub struct Session {
    /// The product name shown in the header.
    pub product: String,
    /// The device's configured name, which a request selects it by.
    pub device_name: String,
    /// The device's unit id, once something has read it.
    pub unit: Option<String>,
    /// Whether the link is up.
    pub connected: bool,
    /// Whether this session's own feed could still bring the link back on its own, without a
    /// switch or a refresh. False only once the backend says nothing is left to read (a direct
    /// session past its one connect deadline, or every backend the loop tried giving up). A
    /// daemon session's indefinite retry never says that while the link is merely down, which
    /// keeps the header's spinner turning for as long as the daemon keeps trying.
    pub feed_open: bool,
    /// What stands in for the device while none is open, which the device screen draws as a
    /// panel and the Devices screen explains above its list; `None` with a device open.
    pub holding: Option<Holding>,
    /// Whether the backend is a daemon's, which keeps the device connected after the TUI
    /// quits: the quit question says so.
    pub daemon: bool,
    /// The daemon's own version, once the running loop has asked it and it has answered. Asked
    /// once per daemon connection, over a connection of its own, so the draw loop never waits on
    /// it. `None` on a direct session (no daemon to ask) and until a daemon session's answer
    /// arrives.
    pub daemon_version: Option<String>,
    /// The daemon version the last warning was about: a daemon of that version is not warned
    /// about again, however many devices this session opens on it or times it restarts.
    pub daemon_version_warned: Option<String>,
    /// Whether the daemon's `config changed` words were toasted since it last said the file was
    /// unchanged, and for a file it found missing (`Some(true)`) or changed (`Some(false)`): each
    /// of the two is told once (a hand edit, then the file's deletion, are two), and a daemon
    /// that says unchanged again (a reload took it) lets the next change be told.
    pub config_changed_told: Option<bool>,
    /// The command that joins this run's daemon from a second terminal, in the demo; the Esc menu
    /// shows it.
    pub join_hint: Option<String>,
    /// When the demo's daemon ending was last said, which is said again every
    /// [`crate::tui::start::GONE_REPEAT`] while it stays gone.
    pub gone_told: Option<std::time::Instant>,
    /// Whether the demo's daemon has ended for good: nothing serves the host's frames any more,
    /// so the loop's own capture does not run in its place.
    pub daemon_gone: bool,
    /// How the device's configured passphrase disagrees with the advertisement, as the backend
    /// last judged it: the Status pane says so while it is `Some`. A best-effort note, never
    /// the digits of a code.
    pub passphrase_note: Option<svolder_ipc::PassphraseNote>,
    /// The device's own row as the backend last listed it: the product, product id, profile,
    /// signal strength, and whether its readings are stale or its passphrase refused. The
    /// Status pane draws its `Device` row from it and words the link with its flags; `None`
    /// until a backend has said.
    pub facts: Option<crate::backend::DeviceRow>,
    /// Whether the link dropped and no status has come since it: what is shown is from before
    /// the drop, even once the link is back, until the first fresh reading.
    pub awaiting_fresh: bool,
}
