// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::web::assets::Assets;

/// Ember is the first theme, with the values the page carries for it.
#[test]
fn ember_is_the_first_theme_with_its_values() {
    let ember = theme("ember").expect("Ember is shipped");
    assert!(std::ptr::eq(ember, &THEMES[0]));
    assert_eq!(ember.label, "Ember");
    assert_eq!(ember.blurb, "warm, smoke and glow");
    assert_eq!(ember.module, "themes/ember/ember.js");
    assert_eq!(ember.styles, ["themes/ember/style.css"]);
    assert_eq!(ember.color_scheme, "dark");
    assert_eq!(ember.theme_color, "#0f0d0b");
    assert_eq!(ember.schemes, ["dark"]);
    assert!(theme("nobody").is_none());
}

/// Paper is the second theme: light and dark paper, with its browser colour for each.
#[test]
fn paper_is_listed_in_light_and_dark() {
    let paper = theme("paper").expect("Paper is shipped");
    assert!(std::ptr::eq(&THEMES[0], theme("ember").unwrap()));
    assert!(std::ptr::eq(paper, &THEMES[1]));
    assert_eq!(paper.label, "Paper");
    assert_eq!(paper.blurb, "light, ink and notes");
    assert_eq!(paper.module, "themes/paper/paper.js");
    assert_eq!(paper.styles, ["themes/paper/style.css"]);
    assert_eq!(paper.schemes, ["light", "dark"]);
    assert_eq!(paper.color_scheme, "light dark");
    assert_eq!(paper.theme_color, "#f3efe6");
    assert_eq!(paper.colour_for(Some("dark")), "#1e1f22");
    assert_eq!(paper.colour_for(Some("light")), "#f3efe6");
    assert_eq!(paper.colour_for(None), "#f3efe6");
    assert!(paper.scripts.is_empty() && paper.metas.is_empty());
}

/// A theme's module and styles are among its files, every file is served by its name, and no
/// two themes bring the same name.
#[test]
fn a_themes_module_styles_and_files_are_served() {
    let mut seen = std::collections::BTreeSet::new();
    for theme in THEMES {
        let named = |name: &str| theme.files.iter().any(|(listed, ..)| *listed == name);
        assert!(named(theme.module), "{}: its module", theme.id);
        for style in theme.styles {
            assert!(named(style), "{}: {style}", theme.id);
        }
        for script in theme.scripts {
            assert!(named(script), "{}: {script}", theme.id);
        }
        for (name, mime, bytes) in theme.files {
            assert!(name.starts_with(&format!("themes/{}/", theme.id)), "{name}");
            assert!(seen.insert(*name), "{name} is listed twice");
            let asset = Assets::Embedded.get(name).expect(name);
            assert_eq!(asset.mime, *mime, "{name}");
            assert_eq!(&*asset.bytes, *bytes, "{name}");
        }
    }
}

/// The list the page carries has what the picker and the registry read, and can sit in a
/// `<script>`.
#[test]
fn the_list_is_the_pickers_and_the_registrys_data() {
    let list: serde_json::Value = serde_json::from_str(&list_json()).expect("JSON");
    assert_eq!(
        list,
        serde_json::json!([{
            "id": "ember",
            "label": "Ember",
            "blurb": "warm, smoke and glow",
            "module": "themes/ember/ember.js",
            "styles": ["themes/ember/style.css"],
            "scripts": ["themes/ember/htmx.min.js", "themes/ember/viz-pref.js"],
            "metas": [{
                "name": "htmx-config",
                "content": r#"{"includeIndicatorStyles":false,"allowEval":false,"allowScriptTags":false,"selfRequestsOnly":true,"historyEnabled":false}"#,
            }],
            "color_scheme": "dark",
            "theme_color": "#0f0d0b",
            "theme_color_dark": null,
            "schemes": ["dark"],
        }, {
            "id": "paper",
            "label": "Paper",
            "blurb": "light, ink and notes",
            "module": "themes/paper/paper.js",
            "styles": ["themes/paper/style.css"],
            "scripts": [],
            "metas": [],
            "color_scheme": "light dark",
            "theme_color": "#f3efe6",
            "theme_color_dark": "#1e1f22",
            "schemes": ["light", "dark"],
        }])
    );
    assert!(!list_json().contains('<'));
}

/// A theme can name a browser colour for its dark scheme; the scheme painted picks the colour.
#[test]
fn a_theme_names_its_browser_colour_per_scheme() {
    let probe = theme("probe").expect("the test theme");
    assert_eq!(probe.colour_for(Some("dark")), "#000000");
    assert_eq!(probe.colour_for(Some("light")), "#ffffff");
    assert_eq!(probe.colour_for(None), "#ffffff");
    assert_eq!(THEMES[0].colour_for(Some("dark")), "#0f0d0b");
    assert!(list_json().contains(r#""theme_color_dark":null"#));
}

/// The choice chooses a theme the page has, and a scheme only where the theme has more than one;
/// anything else is Ember.
#[test]
fn the_choice_chooses_a_theme_and_what_is_not_shipped_is_ember() {
    let with = |cookie: &str, query: Option<&str>| {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, cookie.parse().expect("a header"));
        chosen(&headers, query)
    };
    let ember = &THEMES[0];
    assert!(std::ptr::eq(chosen(&HeaderMap::new(), None).0, ember));
    for cookie in [
        "svolder_theme=ember",
        "a=b; svolder_theme=ember.dark; c=d",
        "svolder_theme=nobody.light",
        "svolder_theme=",
        "svolder_theme",
        "other=ember",
    ] {
        let (theme, scheme) = with(cookie, None);
        assert!(std::ptr::eq(theme, ember), "{cookie}");
        assert_eq!(scheme, None, "{cookie}: Ember has one scheme");
    }
    // The test theme has two schemes: the one asked for stays, one it lacks goes.
    let (theme, scheme) = with("svolder_theme=probe.light", None);
    assert_eq!((theme.id, scheme), ("probe", Some("light")));
    let (theme, scheme) = with("svolder_theme=probe.sepia", None);
    assert_eq!((theme.id, scheme), ("probe", None));
    // The address beats the cookie, and what it names that nobody ships does not.
    let (theme, _) = with("svolder_theme=probe", Some("ember"));
    assert_eq!(theme.id, "ember");
    let (theme, _) = with("svolder_theme=probe", Some("nope"));
    assert_eq!(theme.id, "probe");
}
