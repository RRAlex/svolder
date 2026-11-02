// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verbs `svolder` runs, one module per verb group.
//!
//! `main` parses the command line (see [`crate::cli`]) and hands each verb to the module that
//! carries it out. The modules:
//!
//! - [`capture`]: `capture`, one text file of what svolder learns about a device, scrubbed of ids.
//! - [`config`]: `config path|show|template`, the configuration's location and contents.
//! - [`daemon`]: `daemon status|logs|restart|stop|install|uninstall`, the background service.
//! - [`decode`]: `decode`, a captured frame explained.
//! - [`demo`]: `tui --demo` and `tui --join`, the interface on a fake device and a daemon of its
//!   own.
//! - [`device`]: the device-facing verbs, from `status` and `set` to `eq` and `passphrase`, with
//!   `devices`, `add` and `remove`.
//! - [`doctor`]: `doctor`, the checks of the host, the daemon and each device, and their fixes.
//! - [`hops`]: `hops list|add|link|edit|remove|move`, what the sound goes through after the DAC.
//! - [`host_eq`]: `host eq`, an equalizer on what the computer plays.
//! - [`identify`]: `identify`, a device asked to show itself.
//! - [`interrupts`]: Ctrl-C, SIGTERM and SIGHUP counted, for a verb that must clean up before it
//!   leaves.
//! - [`profiles`]: `profiles list|show`, the device profiles.
//! - [`token`]: `mcp token` and `web token`, the bearer tokens of the daemon's servers.
//! - [`web`]: `web` and `web pair`, the web UI's link, code and pairing.

pub(crate) mod capture;
pub(crate) mod config;
pub(crate) mod daemon;
pub(crate) mod decode;
pub(crate) mod demo;
pub(crate) mod device;
pub(crate) mod doctor;
pub(crate) mod hops;
pub(crate) mod host_eq;
pub(crate) mod identify;
pub(crate) mod interrupts;
pub(crate) mod profiles;
pub(crate) mod token;
pub(crate) mod web;
