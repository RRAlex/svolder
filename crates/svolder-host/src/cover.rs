// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cover art for the followed player, read from the disk and never from the network.
//!
//! [`cover`] takes the `mpris:artUrl` a player published and the places on disk it may look in
//! ([`CoverPlaces`]). A `file://` address names one file, read only if it resolves, symbolic
//! links followed, to a regular file under one of the roots. Spotify's own
//! `https://i.scdn.co/image/<id>` address is looked up in the picture cache Spotify's embedded
//! browser keeps on disk. Every other address, and every file that is missing, too large or
//! not a JPEG, PNG or WebP picture by its first bytes, is no cover. Nothing here opens a
//! socket or runs a program, so an address a player chose can cost at most a bounded read of
//! a local file.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use ring::digest;

/// The largest cover, in bytes, that is read, from a file or from Spotify's cache.
pub const MAX_COVER_BYTES: u64 = 4 * 1024 * 1024;

/// The marker Chromium's simple cache writes after each stream of an entry, as it is stored.
const EOF_MAGIC: [u8; 8] = [0xd8, 0x41, 0x0d, 0x97, 0x45, 0x6f, 0xfa, 0xf4];
/// The length of a simple cache entry's header; the key follows it.
const CACHE_HEADER: usize = 24;
/// Where the header holds the key's length, as a little-endian `u32`.
const CACHE_KEY_LEN_AT: usize = 12;
/// Where a Spotify cover lives on the image server; nothing else is looked up in the cache.
const SPOTIFY_IMAGE_PREFIX: &str = "https://i.scdn.co/image/";
/// Where the cache entries are, below one of Spotify's cache directories.
const CACHE_DATA: &str = "Browser/Cache/Cache_Data";

/// A cover picture: its bytes as read, and what they are by their first bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    /// The picture, at most [`MAX_COVER_BYTES`] long.
    pub bytes: Vec<u8>,
    /// `image/jpeg`, `image/png` or `image/webp`.
    pub mime: &'static str,
}

/// Where covers may be read from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverPlaces {
    /// Directories a `file://` cover must be inside, once its symbolic links are resolved.
    pub roots: Vec<PathBuf>,
    /// Directories holding Spotify's cache entries (`<name>_0`), tried in order.
    pub spotify: Vec<PathBuf>,
}

impl CoverPlaces {
    /// The places for this process: `$HOME`, `/tmp` and `$XDG_RUNTIME_DIR` (those that are set
    /// to an absolute path) as roots, and Spotify's cache under `$HOME` as the snap, deb and
    /// Flatpak installs lay it out.
    pub fn from_env() -> CoverPlaces {
        places_from(
            std::env::var_os("HOME").as_deref(),
            std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        )
    }
}

/// [`CoverPlaces::from_env`] with the two variables passed in.
pub(crate) fn places_from(home: Option<&OsStr>, runtime: Option<&OsStr>) -> CoverPlaces {
    let absolute =
        |value: Option<&OsStr>| value.map(PathBuf::from).filter(|path| path.is_absolute());
    let home = absolute(home);
    let mut roots: Vec<PathBuf> = home.iter().cloned().collect();
    roots.push(PathBuf::from("/tmp"));
    roots.extend(absolute(runtime));
    let spotify = home
        .iter()
        .flat_map(|home| {
            [
                "snap/spotify/common/.cache/spotify",
                ".cache/spotify",
                ".var/app/com.spotify.Client/cache/spotify",
            ]
            .map(|cache| home.join(cache).join(CACHE_DATA))
        })
        .collect();
    CoverPlaces { roots, spotify }
}

/// The cover `art_url` names, if it is one that can be read from `places`: a `file://` picture
/// under a root, or a Spotify image in Spotify's cache. Anything else is `None`.
pub fn cover(art_url: &str, places: &CoverPlaces) -> Option<Cover> {
    if let Some(rest) = art_url.strip_prefix("file://") {
        file_cover(rest, places)
    } else if let Some(id) = art_url.strip_prefix(SPOTIFY_IMAGE_PREFIX) {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        spotify_cover(art_url, places)
    } else {
        None
    }
}

/// The picture a `file://` address names, `rest` being what follows the scheme.
fn file_cover(rest: &str, places: &CoverPlaces) -> Option<Cover> {
    let path = rest
        .strip_prefix("localhost")
        .unwrap_or(rest)
        .strip_prefix('/')?;
    let path = PathBuf::from(OsString::from_vec(percent_decode(path)?));
    let path = Path::new("/").join(path);
    let resolved = std::fs::canonicalize(&path).ok()?;
    let inside = places
        .roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| resolved.starts_with(root));
    if !inside {
        return None;
    }
    let meta = std::fs::metadata(&resolved).ok()?;
    if !meta.is_file() || meta.len() > MAX_COVER_BYTES {
        return None;
    }
    let file = open_inside(&resolved, &places.roots)?;
    if file.metadata().ok()?.len() > MAX_COVER_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_COVER_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    image(bytes)
}

/// Open `path`, if the file that opens is a regular file under one of `roots`. The check is on
/// the open file itself (the path it was opened as, as the kernel resolved it), so a path that
/// was swapped for a link after it was looked at gives nothing.
fn open_inside(path: &Path, roots: &[PathBuf]) -> Option<File> {
    let file = File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let real = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()?;
    roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| real.starts_with(root))
        .then_some(file)
}

/// `text` with its `%XX` escapes decoded, or `None` for a broken escape or a NUL byte.
fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let high = hex_digit(bytes.next()?)?;
        let low = hex_digit(bytes.next()?)?;
        out.push(high << 4 | low);
    }
    (!out.contains(&0)).then_some(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

/// The Spotify picture for `art_url`, from the first cache directory that has it.
fn spotify_cover(art_url: &str, places: &CoverPlaces) -> Option<Cover> {
    let name = format!("{}_0", spotify_cache_name(art_url));
    let key = spotify_key(art_url);
    places
        .spotify
        .iter()
        .find_map(|dir| cache_entry(&dir.join(&name), dir, key.as_bytes()))
}

/// The key Spotify's browser files its cache entry under.
fn spotify_key(art_url: &str) -> String {
    format!("1/0/_dk_https://spotify.com https://spotify.com {art_url}")
}

/// The name of the cache entry file (without its `_0`) for `art_url`: the first 8 bytes of the
/// SHA-1 of the entry's key, as a little-endian number in 16 lowercase hex digits. SHA-1 only
/// names the file here; the key stored inside it is compared in full before anything is used.
pub(crate) fn spotify_cache_name(art_url: &str) -> String {
    let sum = digest::digest(
        &digest::SHA1_FOR_LEGACY_USE_ONLY,
        spotify_key(art_url).as_bytes(),
    );
    let mut first = [0u8; 8];
    first.copy_from_slice(&sum.as_ref()[..8]);
    format!("{:016x}", u64::from_le_bytes(first))
}

/// The picture in the cache entry at `path` in the cache directory `dir`, if its stored key is
/// exactly `key`: the bytes between the key and the first end marker after it, when they are a
/// picture.
fn cache_entry(path: &Path, dir: &Path, key: &[u8]) -> Option<Cover> {
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = open_inside(path, &[dir.to_path_buf()])?;
    let mut header = [0u8; CACHE_HEADER];
    file.read_exact(&mut header).ok()?;
    let stored = u32::from_le_bytes(
        header[CACHE_KEY_LEN_AT..CACHE_KEY_LEN_AT + 4]
            .try_into()
            .ok()?,
    );
    if usize::try_from(stored).ok()? != key.len() {
        return None;
    }
    let mut stored_key = vec![0u8; key.len()];
    file.read_exact(&mut stored_key).ok()?;
    if stored_key != key {
        return None;
    }
    let window = MAX_COVER_BYTES + EOF_MAGIC.len() as u64;
    let mut rest = Vec::new();
    file.take(window).read_to_end(&mut rest).ok()?;
    let end = rest.windows(EOF_MAGIC.len()).position(|w| w == EOF_MAGIC)?;
    rest.truncate(end);
    image(rest)
}

/// `bytes` as a cover if they are within the size cap and start like a JPEG, PNG or WebP.
fn image(bytes: Vec<u8>) -> Option<Cover> {
    if bytes.len() as u64 > MAX_COVER_BYTES {
        return None;
    }
    let mime = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return None;
    };
    Some(Cover { bytes, mime })
}

#[cfg(test)]
mod tests;
