// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The real `svolder tui` binary in a pseudo-terminal.
//!
//! Every case is `#[ignore]`d so that `cargo test` stays fast; `make tui-harness` runs them.

#[path = "../common/mod.rs"]
mod common;
mod harness;
mod pty;
mod tap;

mod cases;
mod crossfeed;
