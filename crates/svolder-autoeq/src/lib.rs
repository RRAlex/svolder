// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq catalogue: the index of headphone corrections published at
//! `github.com/jaakkopasanen/AutoEq`, searched by name, cached on disk, and downloaded one
//! `ParametricEQ.txt` at a time.
//!
//! [`parse_index`] and [`search`] work on text; [`Client`] fetches and caches it over HTTP(S),
//! blocking, within the byte and time limits of [`Limits`]. Text taken from the index only ever
//! becomes a component of a URL, percent-encoded, and never a local path. TLS verifies against
//! the operating system's certificate store.
//!
//! - `client`: the HTTP side: fetching the index into a cache and downloading one correction.
//! - `index`: the index text, parsed into entries, and the search over them.

mod client;
mod index;

pub use client::{AutoEqError, Client, Index, Limits, Refresh};
pub use index::{Entry, exact, parse_index, search};
