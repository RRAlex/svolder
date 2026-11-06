// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's files: embedded in the binary with their type and an ETag of their bytes, or,
//! with `SVOLDER_WEB_ASSETS=<dir>` (a developer's switch), read from that directory on every
//! request. Only the names listed here are ever served, whatever the request asks for.

use std::borrow::Cow;
use std::path::PathBuf;

use super::themes::THEMES;

/// One file as it is served.
#[derive(Debug, Clone)]
pub(crate) struct Asset {
    /// Its bytes.
    pub(crate) bytes: Cow<'static, [u8]>,
    /// Its `Content-Type`.
    pub(crate) mime: &'static str,
    /// Its `ETag`, quoted: the first 8 bytes of its SHA-256, in hex.
    pub(crate) etag: String,
}

/// Where the files come from.
#[derive(Debug, Clone)]
pub(crate) enum Assets {
    /// The binary's own copies.
    Embedded,
    /// A directory laid out as `bins/svolderd/web/`, read on every request.
    Dir(PathBuf),
}

/// The name a request asks for, which is the file's path under `bins/svolderd/web/`, its type,
/// and its bytes.
pub(crate) type Entry = (&'static str, &'static str, &'static [u8]);

/// A font's entry: `name` is under `fonts/`; a TrueType file is named with `ttf`.
macro_rules! font {
    ($name:literal) => {
        font!($name, "font/woff2")
    };
    (ttf $name:literal) => {
        font!($name, "font/ttf")
    };
    ($name:literal, $mime:literal) => {
        (
            concat!("fonts/", $name),
            $mime,
            include_bytes!(concat!("../../web/fonts/", $name)),
        )
    };
}

/// The files every theme shares: the core's, the icons and the fonts.
const FILES: &[Entry] = &[
    (
        "core/frame.css",
        "text/css; charset=utf-8",
        include_bytes!("../../web/core/frame.css"),
    ),
    (
        "core/login.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/login.js"),
    ),
    (
        "core/core.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/core.js"),
    ),
    (
        "core/boot.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/boot.js"),
    ),
    (
        "core/api.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/api.js"),
    ),
    (
        "core/toast.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/toast.js"),
    ),
    (
        "core/store.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/store.js"),
    ),
    (
        "core/stream.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/stream.js"),
    ),
    (
        "core/actions.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/actions.js"),
    ),
    (
        "core/prefs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/prefs.js"),
    ),
    (
        "core/scheme.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/scheme.js"),
    ),
    (
        "core/lifecycle.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/lifecycle.js"),
    ),
    (
        "core/analysis.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/analysis.js"),
    ),
    (
        "core/hold.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/hold.js"),
    ),
    (
        "core/viz-model.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/viz-model.js"),
    ),
    (
        "core/title.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/title.js"),
    ),
    (
        "core/registry.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/registry.js"),
    ),
    (
        "core/climb.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/climb.js"),
    ),
    (
        "core/tone-note.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/tone-note.js"),
    ),
    (
        "core/words.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/words.js"),
    ),
    (
        "core/clock.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/clock.js"),
    ),
    (
        "core/tone-curve.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/tone-curve.js"),
    ),
    (
        "core/tone-drag.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/tone-drag.js"),
    ),
    (
        "core/frame.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/frame.js"),
    ),
    (
        "core/menu.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/menu.js"),
    ),
    (
        "core/picker.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/picker.js"),
    ),
    (
        "core/switch.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/switch.js"),
    ),
    (
        "core/contract.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/contract.js"),
    ),
    (
        "core/visible.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/core/visible.js"),
    ),
    (
        "icon.svg",
        "image/svg+xml",
        include_bytes!("../../web/icon.svg"),
    ),
    (
        "icon-192.png",
        "image/png",
        include_bytes!("../../web/icon-192.png"),
    ),
    (
        "icon-512.png",
        "image/png",
        include_bytes!("../../web/icon-512.png"),
    ),
    font!("IBMPlexSans-Light-Latin1.woff2"),
    font!("IBMPlexSans-Light-Latin2.woff2"),
    font!("IBMPlexSans-Light-Pi.woff2"),
    font!("IBMPlexSans-Regular-Latin1.woff2"),
    font!("IBMPlexSans-Regular-Latin2.woff2"),
    font!("IBMPlexSans-Regular-Pi.woff2"),
    font!("IBMPlexSans-Medium-Latin1.woff2"),
    font!("IBMPlexSans-Medium-Latin2.woff2"),
    font!("IBMPlexSans-Medium-Pi.woff2"),
    font!("IBMPlexSans-SemiBold-Latin1.woff2"),
    font!("IBMPlexSans-SemiBold-Latin2.woff2"),
    font!("IBMPlexSans-SemiBold-Pi.woff2"),
    font!("IBMPlexMono-Regular-Latin1.woff2"),
    font!("IBMPlexMono-Regular-Latin2.woff2"),
    font!("IBMPlexMono-Regular-Pi.woff2"),
    font!("IBMPlexMono-Medium-Latin1.woff2"),
    font!("IBMPlexMono-Medium-Latin2.woff2"),
    font!("IBMPlexMono-Medium-Pi.woff2"),
    font!("Geist-Medium-Latin1.woff2"),
    font!("Geist-Medium-Latin2.woff2"),
    font!("CrimsonPro-Regular-Latin1.woff2"),
    font!("CrimsonPro-Regular-Latin2.woff2"),
    font!("CrimsonPro-Italic-Latin1.woff2"),
    font!("CrimsonPro-Italic-Latin2.woff2"),
    font!("Caveat-Medium-Latin1.woff2"),
    font!("Caveat-Medium-Latin2.woff2"),
    font!(ttf "RussoOne-Regular.ttf"),
    font!(ttf "MrDafoe-Regular.ttf"),
];

impl Assets {
    /// The binary's own copies, or the directory `SVOLDER_WEB_ASSETS` names.
    pub(crate) fn from_env() -> Assets {
        match std::env::var_os("SVOLDER_WEB_ASSETS").filter(|dir| !dir.is_empty()) {
            Some(dir) => Assets::Dir(PathBuf::from(dir)),
            None => Assets::Embedded,
        }
    }

    /// The file a request names, when it is one of the web UI's; `None` for any other name,
    /// and for a listed file a development directory does not hold.
    pub(crate) fn get(&self, name: &str) -> Option<Asset> {
        let &(_, mime, embedded) = FILES
            .iter()
            .chain(THEMES.iter().flat_map(|theme| theme.files))
            .find(|(listed, ..)| *listed == name)?;
        let bytes: Cow<'static, [u8]> = match self {
            Assets::Embedded => Cow::Borrowed(embedded),
            Assets::Dir(dir) => Cow::Owned(std::fs::read(dir.join(name)).ok()?),
        };
        let etag = etag_of(&bytes);
        Some(Asset { bytes, mime, etag })
    }
}

/// `"<16 hex digits>"`: the first 8 bytes of `bytes`' SHA-256.
fn etag_of(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let hex: String = digest.as_ref()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("\"{hex}\"")
}

#[cfg(test)]
mod tests;
