// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Virtual speakers: two speakers at about 30 degrees to either side, heard on headphones.
//!
//! The crate turns a measured set (a SOFA file, or a WAV of four or fourteen channels) into the
//! one 4-channel WAV the filter chain's convolvers play, keeps those WAVs, and fetches the
//! catalogue's sets on request. It reaches no PipeWire; only the `net` feature reaches a network.
//!
//! - `wav`: the own reader and writer of the 4-channel float WAV (and the WAVs imported).
//! - `ir`: the four impulse responses (left speaker to each ear, right speaker to each ear):
//!   lead-in fit, length cap, level, the loudness change and the curve.
//! - `sofa`: SOFA files read once through the `hdf` module of `sofar`, and the choice of the two
//!   speakers' measurements.
//! - `convert`: any source to a [`Converted`] set.
//! - `catalogue`: the curated list of sets that can be fetched.
//! - `store`: the converted sets on disk: sidecar, checksum, limit.
//! - `fetch` (feature `net`): the download of one catalogue set into the store.

pub mod analysis;
mod catalogue;
mod convert;
mod error;
#[cfg(feature = "net")]
mod fetch;
mod import;
pub mod ir;
mod sha;
mod sofa;
mod store;
pub mod wav;

pub use catalogue::{Container, Entry, Room, entries, find};
pub use convert::{Converted, MAX_SOURCE_BYTES, Recipe, convert, convert_wav_pair};
pub use error::ConvertError;
#[cfg(feature = "net")]
pub use fetch::{Cancel, Client, FetchError, Limits, OFF_WORDS};
pub use import::{ImportError, OWN_LICENCE, Saved, import, import_pair};
pub use sha::sha256_hex;
pub use store::{Health, MAX_SETS, SetMeta, SetStatus, Store, StoreError, valid_id};
