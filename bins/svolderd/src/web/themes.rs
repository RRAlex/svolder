// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The themes the page can wear: each one's name and blurb, the module that draws it, its styles,
//! the scripts and meta tags it needs in the page's head and the files it brings. A theme is one folder under
//! `bins/svolderd/web/themes/` and one entry in [`THEMES`]; the frame, the core and the picker
//! know a theme by this list only.

use hyper::HeaderMap;
use hyper::header;

use super::api::embed;
use super::assets::Entry;

/// One theme.
pub(crate) struct Theme {
    /// Its id: the folder's name, and what `data-theme` and the choice's cookie say.
    pub(crate) id: &'static str,
    /// Its name for a person.
    pub(crate) label: &'static str,
    /// A few words on what it is like.
    pub(crate) blurb: &'static str,
    /// The module that registers it, as a request names it under `assets/`.
    pub(crate) module: &'static str,
    /// Its style sheets, as requests name them under `assets/`.
    pub(crate) styles: &'static [&'static str],
    /// The scripts it needs in the page's head before its module runs, as requests name them
    /// under `assets/`, in order (plain scripts of the page's own origin, never inline).
    pub(crate) scripts: &'static [&'static str],
    /// The meta tags it reads before its module runs: `(name, content)`, put in the head before
    /// its scripts.
    pub(crate) metas: &'static [(&'static str, &'static str)],
    /// The `color-scheme` of its page: `dark`, `light` or `light dark`.
    pub(crate) color_scheme: &'static str,
    /// The `theme-color` of the page: the browser's own chrome.
    pub(crate) theme_color: &'static str,
    /// The `theme-color` for its dark scheme, when that differs from `theme_color`.
    pub(crate) theme_color_dark: Option<&'static str>,
    /// The schemes it has: `dark`, `light`, or both.
    pub(crate) schemes: &'static [&'static str],
    /// Its files, by the name a request asks for.
    pub(crate) files: &'static [Entry],
}

impl Theme {
    /// The browser colour for a painted scheme: the dark one for `dark` when the theme has one,
    /// else its `theme_color`.
    pub(crate) fn colour_for(&self, scheme: Option<&str>) -> &'static str {
        match (scheme, self.theme_color_dark) {
            (Some("dark"), Some(dark)) => dark,
            _ => self.theme_color,
        }
    }
}

/// The files of a theme, in the folder `$id`: `js "a.js"` and `css "a.css"` name one each.
macro_rules! theme_files {
    ($id:literal: $($kind:ident $name:literal),* $(,)?) => {
        &[$((
            concat!("themes/", $id, "/", $name),
            theme_files!(@mime $kind),
            include_bytes!(concat!("../../web/themes/", $id, "/", $name)),
        )),*]
    };
    (@mime js) => { "text/javascript; charset=utf-8" };
    (@mime css) => { "text/css; charset=utf-8" };
}

/// A theme of the folder `$id`: its module is `<id>.js`.
macro_rules! theme {
    (
        id: $id:literal, label: $label:literal, blurb: $blurb:literal,
        styles: [$($style:literal),*], color_scheme: $color_scheme:literal,
        theme_color: $theme_color:literal, theme_color_dark: $theme_color_dark:expr,
        schemes: [$($scheme:literal),*],
        scripts: [$($script:literal),*], metas: [$(($meta:literal, $content:literal)),*],
        files: [$($kind:ident $name:literal),* $(,)?]
    ) => {
        Theme {
            id: $id,
            label: $label,
            blurb: $blurb,
            module: concat!("themes/", $id, "/", $id, ".js"),
            styles: &[$(concat!("themes/", $id, "/", $style)),*],
            scripts: &[$(concat!("themes/", $id, "/", $script)),*],
            metas: &[$(($meta, $content)),*],
            color_scheme: $color_scheme,
            theme_color: $theme_color,
            theme_color_dark: $theme_color_dark,
            schemes: &[$($scheme),*],
            files: theme_files!($id: $($kind $name),*),
        }
    };
}

/// Every theme the page ships, the first the one the page falls back to.
pub(crate) static THEMES: &[Theme] = &[
    theme! {
        id: "ember",
        label: "Ember",
        blurb: "warm, smoke and glow",
        styles: ["style.css"],
        color_scheme: "dark",
        theme_color: "#0f0d0b",
        theme_color_dark: None,
        schemes: ["dark"],
        scripts: ["htmx.min.js", "viz-pref.js"],
        metas: [(
            "htmx-config",
            r#"{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false,"selfRequestsOnly":true,"historyEnabled":false}"#
        )],
        files: [
            css "style.css",
            js "ember.js", js "preview.js", js "live.js", js "press.js", js "bind.js", js "order.js",
            js "sections.js", js "arrange.js", js "driver.js",
            js "htmx.min.js", js "viz-pref.js",
            js "knob.js", js "slider.js", js "level-write.js", js "meters.js", js "keep.js",
            js "keys.js", js "facts.js",
            js "playing.js", js "strip.js", js "tone-editor.js", js "tone-graph.js",
            js "tone-motion.js", js "visualizer.js", js "viz-colour.js", js "viz-model.js",
            js "effects.js", js "smoke.js", js "smoke-model.js", js "smoke-director.js",
            js "smoke-engine.js", js "smoke-notes.js",
            js "draw/html.js", js "draw/parts.js", js "draw/meter-arch.js", js "draw/dial.js",
            js "draw/curve.js", js "draw/screen.js", js "draw/shell.js"
        ]
    },
    theme! {
        id: "paper",
        label: "Paper",
        blurb: "light, ink and notes",
        styles: ["style.css"],
        color_scheme: "light dark",
        theme_color: "#f3efe6",
        theme_color_dark: Some("#1e1f22"),
        schemes: ["light", "dark"],
        scripts: [],
        metas: [],
        files: [
            css "style.css",
            js "paper.js", js "preview.js", js "words.js", js "act.js", js "live.js", js "place.js",
            js "ruler.js", js "gestures.js", js "volume-wire.js", js "draw/sheet.js",
            js "draw/esc.js", js "draw/volume.js", js "draw/word.js", js "draw/tone.js",
            js "graph.js", js "lists.js", js "tone-wire.js", js "draw/controls.js",
            js "draw/presets.js", js "ask.js", js "controls-wire.js", js "presets-wire.js",
            js "draw/playing.js", js "draw/amp.js", js "draw/device.js", js "device-wire.js",
            js "changes.js", js "fold.js", js "notes.js", js "draw/notes.js", js "marks.js",
            js "driver.js"
        ]
    },
];

/// A theme that exists only in the tests: it lets a test see the choice go to a theme the page does
/// not ship.
#[cfg(test)]
static TEST_THEMES: &[Theme] = &[Theme {
    id: "probe",
    label: "Probe",
    blurb: "a theme the tests have",
    module: "themes/probe/probe.js",
    styles: &[],
    scripts: &[],
    metas: &[],
    color_scheme: "light dark",
    theme_color: "#ffffff",
    theme_color_dark: Some("#000000"),
    schemes: &["light", "dark"],
    files: &[],
}];

/// The themes `chosen` may name: the ones the page ships (and, in the tests, the test theme).
fn named() -> impl Iterator<Item = &'static Theme> {
    #[cfg(test)]
    let more = TEST_THEMES.iter();
    #[cfg(not(test))]
    let more = [].iter();
    THEMES.iter().chain(more)
}

/// The theme `id`, if the page ships it.
pub(crate) fn theme(id: &str) -> Option<&'static Theme> {
    named().find(|theme| theme.id == id)
}

/// The cookie the page keeps the browser's choice in: `<id>`, or `<id>.light` / `<id>.dark`.
const CHOICE_COOKIE: &str = "svolder_theme";

/// The theme and scheme named by a choice's text, `<id>` or `<id>.<scheme>`: the theme only if the
/// page ships it, the scheme only if that theme has it and has more than one.
fn parse_choice(text: &str) -> Option<(&'static Theme, Option<&'static str>)> {
    let (id, scheme) = text
        .split_once('.')
        .map_or((text, None), |(id, scheme)| (id, Some(scheme)));
    let theme = theme(id)?;
    let scheme = scheme
        .filter(|_| theme.schemes.len() > 1)
        .and_then(|scheme| theme.schemes.iter().find(|have| **have == scheme).copied());
    Some((theme, scheme))
}

/// The theme and scheme a request chooses: `?theme=` when it names a shipped theme, else the
/// cookie's choice, else the first theme (a name nobody ships is passed over); a scheme only for a
/// theme that has more than one, and the scheme the cookie holds for the same theme when the
/// address names none.
pub(crate) fn chosen(
    headers: &HeaderMap,
    query: Option<&str>,
) -> (&'static Theme, Option<&'static str>) {
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == CHOICE_COOKIE)
        .and_then(|(_, value)| parse_choice(value));
    let Some((theme, scheme)) = query.and_then(parse_choice) else {
        return cookie.unwrap_or((&THEMES[0], None));
    };
    let kept = cookie.filter(|(held, _)| std::ptr::eq(*held, theme));
    (
        theme,
        scheme.or_else(|| kept.and_then(|(_, scheme)| scheme)),
    )
}

/// The themes as the page's data block lists them: `[{"id","label","blurb","module","styles",
/// "scripts","metas":[{"name","content"}],"color_scheme","theme_color","theme_color_dark",
/// "schemes"}]`, in a form that cannot close the `<script>` it sits in.
pub(crate) fn list_json() -> String {
    let list: Vec<_> = THEMES
        .iter()
        .map(|theme| {
            serde_json::json!({
                "id": theme.id,
                "label": theme.label,
                "blurb": theme.blurb,
                "module": theme.module,
                "styles": theme.styles,
                "scripts": theme.scripts,
                "metas": theme.metas.iter()
                    .map(|(name, content)| serde_json::json!({"name": name, "content": content}))
                    .collect::<Vec<_>>(),
                "color_scheme": theme.color_scheme,
                "theme_color": theme.theme_color,
                "theme_color_dark": theme.theme_color_dark,
                "schemes": theme.schemes,
            })
        })
        .collect();
    embed(&list)
}

#[cfg(test)]
mod tests;
