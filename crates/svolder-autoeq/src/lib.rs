// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AutoEq catalogue: the index of headphone corrections published at
//! `github.com/jaakkopasanen/AutoEq`, searched by name, cached on disk, and downloaded one
//! `ParametricEQ.txt` at a time.
//!
//! [`parse_index`] and [`search`] work on text and reach no network. With the feature `net`
//! (off by default), `Client` fetches and caches it over HTTP(S), blocking, within the byte and
//! time limits of `Limits`. Text taken from the index only ever becomes a component of a URL,
//! percent-encoded, and never a local path. TLS verifies against the operating system's
//! certificate store.
//!
//! - `index`: the index text, parsed into entries, the search over them, the group key and
//!   measurement words, and the refresh policy.
//! - `client` (feature `net`): the HTTP side: fetching the index into a cache and downloading one
//!   correction.
//! - `test_server` (feature `test-server`): a loopback HTTP server for tests that need a base
//!   URL to give a `Client`.

#[cfg(feature = "net")]
mod client;
mod index;
#[cfg(any(test, feature = "test-server"))]
pub mod test_server;

#[cfg(feature = "net")]
pub use client::{AutoEqError, Client, Limits, missing_file_words};
pub use index::{Entry, Index, Refresh, exact, parse_index, search};
