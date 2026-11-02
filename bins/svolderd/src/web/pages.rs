// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's pages, as askama templates under `bins/svolderd/templates/`, checked when the
//! daemon is built. Every value is escaped but the wordmark, which is the binary's own SVG.

use askama::Template;

use crate::web::themes::Theme;

/// The wordmark, inline so the page's stylesheet colours its three layers.
pub(crate) const WORDMARK: &str = include_str!("../../web/wordmark.svg");

/// The sign-in page.
#[derive(Template)]
#[template(path = "signin.html")]
pub(crate) struct SignIn {
    /// The `Host` the browser asked for, as the line under the title names it.
    pub(crate) host: String,
    /// Whether the browser's sign-in ended after three years.
    pub(crate) ended: bool,
}

/// The page's frame: the root a theme draws into and the data it draws from, the first snapshot of
/// the page's model and the list of themes.
#[derive(Template)]
#[template(path = "page.html")]
pub(crate) struct Page {
    /// The theme the page opens in.
    pub(crate) theme: &'static Theme,
    /// The scheme to open in, for a theme with more than one and a choice already made.
    pub(crate) scheme: Option<&'static str>,
    /// Whether the viewer is a guest.
    pub(crate) guest: bool,
    /// Whether the viewer may change a device.
    pub(crate) writes: bool,
    /// The page's model as JSON that cannot close its `<script>`: the first snapshot.
    pub(crate) page: String,
    /// The themes the page ships, as JSON that cannot close its `<script>`.
    pub(crate) themes: String,
}

#[cfg(test)]
mod tests;
