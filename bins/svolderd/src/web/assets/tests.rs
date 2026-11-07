// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Only listed names are served, and a development directory is read afresh, its ETag
/// following its bytes.
#[test]
fn only_listed_names_are_served_and_a_directory_is_read_afresh() {
    assert!(Assets::Embedded.get(EMBER_CSS).is_some());
    for name in [
        "",
        "nope.css",
        "../themes/ember/style.css",
        "fonts/OFL.txt",
        "OFL.txt",
        "OFL-Geist.txt",
        "OFL-CrimsonPro.txt",
        "OFL-Caveat.txt",
        "fonts/OFL-Caveat.txt",
        "OFL-RussoOne.txt",
        "OFL-MrDafoe.txt",
        "fonts/OFL-MrDafoe.txt",
    ] {
        assert!(Assets::Embedded.get(name).is_none(), "{name}");
    }
    let dir = std::env::temp_dir().join(format!("svolderd-assets-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("themes/ember")).unwrap();
    std::fs::write(dir.join(EMBER_CSS), "a{}").unwrap();
    let assets = Assets::Dir(dir.clone());
    let first = assets.get(EMBER_CSS).expect("the file");
    std::fs::write(dir.join(EMBER_CSS), "b{}").unwrap();
    let second = assets.get(EMBER_CSS).expect("the file");
    assert_eq!(&*second.bytes, b"b{}");
    assert_ne!(first.etag, second.etag);
    assert!(
        assets.get("fonts/IBMPlexSans-Light-Latin1.woff2").is_none(),
        "not in the directory"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// The page's script and its modules, its three components, the effects modules and htmx are
/// served as JavaScript; htmx's licence file lies beside it in the tree and is not served.
#[test]
fn the_pages_scripts_are_served_and_the_licence_is_not() {
    for name in [
        "themes/ember/ember.js",
        "themes/ember/live.js",
        "themes/ember/press.js",
        "themes/ember/bind.js",
        "core/core.js",
        "core/boot.js",
        "themes/ember/knob.js",
        "themes/ember/level-write.js",
        "themes/ember/slider.js",
        "themes/ember/meters.js",
        "themes/ember/keep.js",
        "themes/ember/order.js",
        "themes/ember/sections.js",
        "themes/ember/arrange.js",
        "themes/ember/driver.js",
        "themes/ember/tone-motion.js",
        "core/tone-curve.js",
        "core/tone-drag.js",
        "themes/ember/tone-graph.js",
        "themes/ember/keys.js",
        "themes/ember/facts.js",
        "themes/ember/playing.js",
        "themes/ember/tone-editor.js",
        "themes/ember/strip.js",
        "core/toast.js",
        "core/api.js",
        "core/store.js",
        "core/stream.js",
        "core/actions.js",
        "core/prefs.js",
        "core/scheme.js",
        "core/lifecycle.js",
        "core/analysis.js",
        "core/hold.js",
        "core/title.js",
        "core/viz-model.js",
        "core/registry.js",
        "core/climb.js",
        "core/tone-note.js",
        "core/words.js",
        "core/clock.js",
        "themes/ember/effects.js",
        "themes/ember/smoke.js",
        "themes/ember/smoke-model.js",
        "themes/ember/smoke-notes.js",
        "core/visible.js",
        "core/contract.js",
        "core/computer.js",
        "core/ask.js",
        "themes/ember/visualizer.js",
        "themes/ember/viz-colour.js",
        "themes/ember/viz-pref.js",
        "themes/ember/htmx.min.js",
    ] {
        let asset = Assets::Embedded.get(name).expect(name);
        assert_eq!(asset.mime, "text/javascript; charset=utf-8", "{name}");
        assert!(!asset.bytes.is_empty(), "{name}");
    }
    assert!(Assets::Embedded.get("themes/ember/htmx.LICENSE").is_none());
    assert!(Assets::Embedded.get("htmx.LICENSE").is_none());
}

/// The core's files, Ember's files and the fonts are served by their paths under `web/`, and by
/// no other name.
#[test]
fn the_core_and_embers_files_are_served_by_their_paths() {
    for name in [
        "core/api.js",
        "themes/ember/smoke.js",
        "fonts/Geist-Medium-Latin1.woff2",
    ] {
        assert!(Assets::Embedded.get(name).is_some(), "{name}");
    }
    for name in ["smoke.js", "themes/ember/../../core/api.js"] {
        assert!(Assets::Embedded.get(name).is_none(), "{name}");
    }
}

/// The analysis rules are the core's, so any theme may read them; Ember no longer has its own copy.
#[test]
fn the_analysis_rules_are_served_from_the_core() {
    let asset = Assets::Embedded
        .get("core/viz-model.js")
        .expect("core/viz-model.js");
    assert_eq!(asset.mime, "text/javascript; charset=utf-8");
    assert!(!asset.bytes.is_empty());
    assert!(Assets::Embedded.get("themes/ember/viz-model.js").is_none());
}

/// The icons are served with their types.
#[test]
fn the_icons_are_served() {
    for (name, mime) in [
        ("icon.svg", "image/svg+xml"),
        ("icon-192.png", "image/png"),
        ("icon-512.png", "image/png"),
    ] {
        assert_eq!(
            Assets::Embedded.get(name).map(|a| a.mime),
            Some(mime),
            "{name}"
        );
    }
}

/// The section titles' font, Geist Medium in two Latin subsets, is served as WOFF2 and the
/// core's stylesheet names each file.
#[test]
fn the_title_font_is_served_and_the_stylesheet_names_it() {
    let css = String::from_utf8(
        Assets::Embedded
            .get("core/frame.css")
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    for name in ["Geist-Medium-Latin1.woff2", "Geist-Medium-Latin2.woff2"] {
        let asset = Assets::Embedded.get(&format!("fonts/{name}")).expect(name);
        assert_eq!(asset.mime, "font/woff2", "{name}");
        assert_eq!(&asset.bytes[..4], b"wOF2", "{name}");
        assert!(css.contains(&format!("url(\"../fonts/{name}\")")), "{name}");
    }
}

/// Paper's fonts, Crimson Pro (the print) and Caveat (the hand), each in two Latin subsets, are
/// served as WOFF2 under 60 000 bytes and the core's stylesheet names each file, declaring the
/// same ranges as Geist's two (none of the faces has an arrow, so none claims one).
#[test]
fn the_paper_fonts_are_served_and_the_core_names_them() {
    let css = String::from_utf8(
        Assets::Embedded
            .get("core/frame.css")
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    for name in [
        "CrimsonPro-Regular-Latin1.woff2",
        "CrimsonPro-Regular-Latin2.woff2",
        "CrimsonPro-Italic-Latin1.woff2",
        "CrimsonPro-Italic-Latin2.woff2",
        "Caveat-Medium-Latin1.woff2",
        "Caveat-Medium-Latin2.woff2",
    ] {
        let asset = Assets::Embedded.get(&format!("fonts/{name}")).expect(name);
        assert_eq!(asset.mime, "font/woff2", "{name}");
        assert_eq!(&asset.bytes[..4], b"wOF2", "{name}");
        assert!(asset.bytes.len() < 60_000, "{name}");
        let url = format!("url(\"../fonts/{name}\")");
        let at = css.find(&url).unwrap_or_else(|| panic!("{name}"));
        let rule = &css[at..at + css[at..].find('}').unwrap()];
        let range = |rule: &str| rule[rule.find("unicode-range").unwrap()..].to_string();
        let geist = format!(
            "Geist-Medium-{}.woff2",
            if name.contains("Latin1") {
                "Latin1"
            } else {
                "Latin2"
            }
        );
        let at = css.find(&format!("url(\"../fonts/{geist}\")")).unwrap();
        let geist_rule = &css[at..at + css[at..].find('}').unwrap()];
        assert_eq!(range(rule), range(geist_rule), "{name}");
        assert!(!rule.contains("U+2192"), "{name}");
    }
}

/// Night drive's fonts, Russo One (the figures) and Mr Dafoe (the script), carry Reserved Font
/// Names, so each is served as its Original Version, one TrueType file under 60 000 bytes, and
/// the core's stylesheet names it.
#[test]
fn the_night_drive_fonts_are_served_and_the_core_names_them() {
    let css = String::from_utf8(
        Assets::Embedded
            .get("core/frame.css")
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/fonts");
    for name in ["RussoOne-Regular.ttf", "MrDafoe-Regular.ttf"] {
        let asset = Assets::Embedded.get(&format!("fonts/{name}")).expect(name);
        assert_eq!(asset.mime, "font/ttf", "{name}");
        assert_eq!(&asset.bytes[..4], &[0, 1, 0, 0], "{name}");
        assert!(asset.bytes.len() < 60_000, "{name}");
        assert_eq!(
            &asset.bytes[..],
            std::fs::read(dir.join(name)).unwrap(),
            "{name}"
        );
        assert!(css.contains(&format!("url(\"../fonts/{name}\")")), "{name}");
    }
}

/// The core's stylesheet draws in tokens only: outside its font faces and its custom-property
/// declarations it names no colour of its own, so a theme dresses every part of the frame.
#[test]
fn the_frame_draws_in_its_tokens_only() {
    let css = String::from_utf8(
        Assets::Embedded
            .get("core/frame.css")
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap();
    let mut rest = String::new();
    let mut text = css.as_str();
    while let Some(at) = text.find("@font-face") {
        rest.push_str(&text[..at]);
        let close = text[at..].find('}').expect("a font face closes");
        text = &text[at + close + 1..];
    }
    rest.push_str(text);
    let mut kept = String::new();
    for declaration in rest.split(';') {
        // A custom property's declaration starts after the last `{` or `}` before its name.
        let start = declaration.rfind(['{', '}']).map_or(0, |i| i + 1);
        if declaration[start..].trim_start().starts_with("--") {
            kept.push_str(&declaration[..start]);
        } else {
            kept.push_str(declaration);
        }
        kept.push(';');
    }
    if let Some(at) = kept.find("rgb") {
        panic!(
            "a colour function outside a token: {}",
            &kept[at..kept.len().min(at + 40)]
        );
    }
    if let Some(found) = stray_hex(&kept) {
        panic!("a hex colour outside a token: {found}");
    }
}

/// The first hex colour in `css` (`#` and 3, 4, 6 or 8 hex digits), as the text from its `#`.
/// The digits must end the word, so an id such as `#face-button` or `#added` is no colour; nor
/// is an id selector, which a block follows (`#add {`).
fn stray_hex(css: &str) -> Option<&str> {
    let bytes = css.as_bytes();
    (0..bytes.len()).find_map(|at| {
        if bytes[at] != b'#' {
            return None;
        }
        let hex = bytes[at + 1..]
            .iter()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        let end = at + 1 + hex;
        let word_ends = bytes
            .get(end)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')));
        let block_follows = css[end..].trim_start().starts_with('{');
        (matches!(hex, 3 | 4 | 6 | 8) && word_ends && !block_follows)
            .then(|| &css[at..css.len().min(at + 20)])
    })
}

/// A hex colour is found as a value, and an id is not mistaken for one.
#[test]
fn the_hex_scan_finds_colours_and_not_ids() {
    for colour in [
        "color: #fff;",
        "border: 1px solid #1e1f22;",
        "background: #abcd",
        "x: linear-gradient(#fff, #000);",
        "x: #12345678 !important;",
    ] {
        assert!(stray_hex(colour).is_some(), "{colour}");
    }
    for id in [
        "#add { color: var(--a); }",
        "#face-button { x: y }",
        "a #added, b { x: y }",
        "#c0ffee_x;",
        "#abcdefg",
        "#ab",
        "x: y",
    ] {
        assert_eq!(stray_hex(id), None, "{id}");
    }
}

/// Ember's stylesheet, by its path.
const EMBER_CSS: &str = "themes/ember/style.css";

/// The embedded stylesheet's text.
fn stylesheet() -> String {
    String::from_utf8(Assets::Embedded.get(EMBER_CSS).unwrap().bytes.to_vec()).unwrap()
}

/// The embedded strip script's text.
fn strip_script() -> String {
    String::from_utf8(
        Assets::Embedded
            .get("themes/ember/strip.js")
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap()
}

/// The small motions do not follow the system's wish for less: the stylesheet has no
/// `prefers-reduced-motion` block, so nothing is stilled, the glow is not pinned and the strip is
/// shown and scrolls as usual (the effects menu's Off stills the page).
#[test]
fn the_stylesheet_does_not_follow_reduced_motion() {
    let css = stylesheet();
    assert!(!css.contains("prefers-reduced-motion"));
    assert!(!css.contains("animation: none !important"));
    assert!(!css.contains("--lvl: .6 !important"));
    let script = strip_script();
    let frame = &script[script.find("function frame(").expect("the frame")..];
    let frame = &frame[..frame.find("\n}").unwrap()];
    assert!(!frame.contains("reduce"), "{frame}");
    assert!(frame.contains("strip.draw"), "{frame}");
}

/// Only the first visit's default level reads the system's setting (`effects.js`); every other
/// script that moves something ignores it.
#[test]
fn only_the_default_effects_level_reads_reduced_motion() {
    for name in [
        "themes/ember/knob.js",
        "themes/ember/meters.js",
        "themes/ember/tone-motion.js",
        "themes/ember/playing.js",
    ] {
        let bytes = Assets::Embedded.get(name).unwrap().bytes.to_vec();
        let script = String::from_utf8(bytes).unwrap();
        assert!(!script.contains("prefers-reduced-motion"), "{name}");
        assert!(!script.contains("reduce.matches"), "{name}");
    }
    let bytes = Assets::Embedded
        .get("themes/ember/effects.js")
        .unwrap()
        .bytes
        .to_vec();
    let effects = String::from_utf8(bytes).unwrap();
    assert_eq!(effects.matches("prefers-reduced-motion").count(), 1);
}

/// The glow is about 1.75 times stronger than the look reference (21, 17.5 and 12 per cent for
/// its three gradients) and turns once every 35 seconds.
#[test]
fn the_glow_is_stronger_and_turns_every_35_seconds() {
    let css = stylesheet();
    let flat = css.split_whitespace().collect::<Vec<_>>().join(" ");
    for (token, percent) in [("glow-1", "21%"), ("glow-2", "17.5%"), ("glow-3", "12%")] {
        let at = format!("color-mix(in srgb, var(--{token}) {percent}, transparent)");
        assert!(flat.contains(&at), "{at}");
    }
    assert!(flat.contains("opacity: calc(.22 + .78 * var(--lvl))"));
    assert!(flat.contains("animation: glow-turn 35s linear infinite"));
}

/// The knob eases between two reported readings over less than the 84 ms a climb's steps are
/// apart (measured), whatever the system says about motion.
#[test]
fn the_knob_eases_between_readings_for_less_than_a_step() {
    let bytes = Assets::Embedded
        .get("themes/ember/knob.js")
        .unwrap()
        .bytes
        .to_vec();
    let knob = String::from_utf8(bytes).unwrap();
    let ms: u32 = knob
        .split("const EASE_MS = ")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .and_then(|n| n.parse().ok())
        .expect("EASE_MS");
    assert!((40..=80).contains(&ms), "{ms}");
    let glide = &knob[knob.find("  glide(level) {").expect("glide")..];
    let glide = &glide[..glide.find("\n  }\n").unwrap()];
    assert!(!glide.contains("reduce"), "{glide}");
    assert!(
        glide.contains("this.shown = level"),
        "ends on the reported level: {glide}"
    );
}

/// The strip is drawn at the bottom of the page, behind the content, in the band `.app` keeps
/// free under the footer; `.app` makes its own stacking context for the `z-index: -1` layers.
#[test]
fn the_strip_is_at_the_bottom_of_the_page_and_the_footer_clears_it() {
    let css = stylesheet();
    let rule = &css[css.find("\n.hist {").expect("the strip's rule")..];
    let rule = &rule[..rule.find('}').unwrap()];
    assert!(rule.contains("position: absolute"), "{rule}");
    assert!(
        rule.contains("z-index: -1") && rule.contains("bottom: 0"),
        "{rule}"
    );
    assert!(css.contains(".app { position: relative; isolation: isolate; }"));
    let app = &css[css.find("\n.app {\n  container").expect("the app's rule")..];
    let app = &app[..app.find('}').unwrap()];
    assert!(
        app.contains("var(--hist-h)"),
        "the footer clears the strip: {app}"
    );
}

/// The curve's peak words sit above the plot, in the margin the plot leaves, never at the foot
/// where the frequency labels are.
#[test]
fn the_peak_words_clear_the_axis_labels() {
    let css = stylesheet();
    let rule = &css[css.find(".eqg .pkt {").expect("the peak's rule")..];
    let rule = &rule[..rule.find('}').unwrap()];
    assert!(
        rule.contains("top: 1px") && !rule.contains("bottom"),
        "{rule}"
    );
    let plot = &css[css.find(".eqg .plot {").expect("the plot's rule")..];
    let plot = &plot[..plot.find('}').unwrap()];
    assert!(plot.contains("top: 12px"), "room above the plot: {plot}");
}
