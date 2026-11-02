// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything that talks over the daemon's Unix socket: the wire protocol, its line framing, the
//! server and client, where the socket lives, and the version check a front end makes of a
//! daemon.
//!
//! Both front ends and the daemon reach the socket through this group, so a message is defined
//! once and framed, sent and read the same way on each side.
//!
//! | Module | What it holds |
//! | --- | --- |
//! | [`protocol`] | The requests, replies and events, and their line encoding |
//! | [`signal`] | A device's signal strength on the wire, and the one way to word it |
//! | [`lines`] | Reading newline-framed lines with a length limit |
//! | [`server`] | Binding the socket and serving a handler on it |
//! | [`client`] | Connecting to a daemon and exchanging requests |
//! | [`path`] | Resolving and preparing the socket path |
//! | [`daemon_version`] | Asking a daemon its version and judging how it compares with ours |

pub mod client;
pub mod daemon_version;
pub mod lines;
pub mod path;
pub mod protocol;
pub mod server;
pub mod signal;
