// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::tui::palette::detect_depth_from;
use svolder_ipc::{Config, FamilyAccents};

/// Every role a screen draws with, in the order [`Role`] declares them.
const ALL_ROLES: [Role; 34] = [
    Role::Text,
    Role::Dim,
    Role::Faint,
    Role::Good,
    Role::Warn,
    Role::Alert,
    Role::Rule,
    Role::Badge,
    Role::Header,
    Role::Connected,
    Role::Disconnected,
    Role::Stale,
    Role::Selected,
    Role::Cursor,
    Role::ToggleOn,
    Role::SelectName,
    Role::Bar,
    Role::MeterOk,
    Role::MeterHot,
    Role::MeterClip,
    Role::Fault,
    Role::CurveSum,
    Role::CurveSelected,
    Role::CurvePoint,
    Role::CurveTarget,
    Role::CurveGap,
    Role::Axis,
    Role::Unknown,
    Role::RateFast,
    Role::Notice,
    Role::KeyHint,
    Role::KeyDesc,
    Role::Title,
    Role::ModeArrow,
];

/// Monochrome drops the colours and keeps the attributes, whatever depth it draws at:
/// there is nothing but weight left to tell the roles apart with.
fn assert_every_role_is_colourless(theme: &Theme) {
    for role in ALL_ROLES {
        let style = theme.style(role);
        assert_eq!(style.fg, None, "{role:?} carries a foreground colour");
        assert_eq!(style.bg, None, "{role:?} carries a background colour");
    }
    assert!(
        theme
            .style(Role::Selected)
            .add_modifier
            .contains(Modifier::REVERSED),
        "monochrome marks the selected row by reverse video"
    );
    for role in [Role::Header, Role::SelectName] {
        assert!(
            theme.style(role).add_modifier.contains(Modifier::BOLD),
            "{role:?} lost the weight that is all monochrome has to stand it out with"
        );
    }
    for role in [
        Role::Dim,
        Role::Disconnected,
        Role::Stale,
        Role::Axis,
        Role::CurveTarget,
        Role::CurveGap,
    ] {
        assert!(
            theme.style(role).add_modifier.contains(Modifier::DIM),
            "{role:?} lost the dim that is all monochrome has to hold it back with"
        );
    }
}

#[test]
fn presets_are_valid_and_the_accent_follows_the_family() {
    for name in ["amber", "ice", "red", "neon"] {
        let p = preset(name).expect(name);
        assert_eq!(p.name, name);
        for accent in [p.dac, p.amp, p.preamp] {
            assert!(
                accent != p.good && accent != p.warn && accent != p.alert,
                "{name}: a family accent dresses as a semantic colour"
            );
        }
    }
    assert!(preset("sepia").is_none());

    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    theme.set_family(Family::Dac);
    assert_eq!(theme.accent_rgb(), (0xe0, 0xa0, 0x30));
    theme.set_family(Family::Amp);
    assert_eq!(theme.accent_rgb(), (0xb8, 0x6a, 0x2a));
    theme.set_family(Family::Preamp);
    assert_eq!(theme.accent_rgb(), (0xf6, 0xcc, 0x7a));
}

/// The accents amber, ice and red give a DAC, an amplifier and a preamplifier, in that
/// order: three shades of the one hue each preset is named for.
const SHADES: [(&str, [Rgb; 3]); 3] = [
    (
        "amber",
        [(0xe0, 0xa0, 0x30), (0xb8, 0x6a, 0x2a), (0xf6, 0xcc, 0x7a)],
    ),
    (
        "ice",
        [(0x5f, 0xc9, 0xc9), (0x1f, 0x6a, 0xa8), (0xa8, 0xe6, 0xf0)],
    ),
    (
        "red",
        [(0xe0, 0x34, 0x1c), (0xb0, 0x1c, 0x3c), (0xff, 0xa8, 0x90)],
    ),
];

/// The hue of `rgb` on the colour circle, in degrees.
fn hue(rgb: Rgb) -> f32 {
    let (r, g, b) = (
        f32::from(rgb.0) / 255.0,
        f32::from(rgb.1) / 255.0,
        f32::from(rgb.2) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    let sector = match max {
        _ if chroma == 0.0 => 0.0,
        m if m == r => ((g - b) / chroma).rem_euclid(6.0),
        m if m == g => (b - r) / chroma + 2.0,
        _ => (r - g) / chroma + 4.0,
    };
    sector * 60.0
}

/// No accent any preset gives a family is one of the preset's own spectrum stops other than
/// the resting stop it takes the place of: a stop the accent repeats would leave the glow with
/// nothing to travel between the two, frozen over that stretch.
///
/// And every family accent of every preset stays clear of the preset's alert colour by
/// more than [`ALERT_NEAR`], so a fault is told apart by colour alone — all but the red
/// preset's DAC, the one accent that is its alert colour's own neighbour, whose faults
/// take the inverse row instead.
#[test]
fn no_family_accent_repeats_a_stop_or_stands_beside_the_alert_colour() {
    for name in ["amber", "ice", "red", "neon"] {
        let p = preset(name).expect(name);
        for (family, accent) in [("DAC", p.dac), ("amp", p.amp), ("preamp", p.preamp)] {
            for (at, stop) in p.spectrum.iter().enumerate() {
                assert!(
                    at == p.rest || *stop != accent,
                    "{name}: the {family} accent is stop {at} of the spectrum"
                );
            }
        }
    }
    for name in ["amber", "ice", "red", "neon"] {
        let p = preset(name).expect(name);
        for (family, accent) in [("DAC", p.dac), ("amp", p.amp), ("preamp", p.preamp)] {
            let apart = palette::distance(accent, p.alert);
            let beside = (name, family) == ("red", "DAC");
            assert_eq!(
                apart < ALERT_NEAR,
                beside,
                "{name}: the {family} accent is {apart:.1} from the alert colour"
            );
        }
    }
}

/// Amber, ice and red each draw every device family in a shade of their own hue, so the
/// preset's name describes whatever device is on the screen: each family's accent is the
/// one the table gives it, the three are three different colours, and none of them strays
/// further round the colour circle from the DAC's than the width of one hue. Neon is the
/// one preset whose families are three hues, which is what its name promises.
#[test]
fn a_presets_families_are_shades_of_its_own_hue_but_neons() {
    const ONE_HUE: f32 = 30.0;
    for (name, [dac, amp, preamp]) in SHADES {
        let p = preset(name).expect(name);
        assert_eq!([p.dac, p.amp, p.preamp], [dac, amp, preamp], "{name}");
        assert!(
            dac != amp && dac != preamp && amp != preamp,
            "{name}: two families share one accent"
        );
        for (family, accent) in [("amp", amp), ("preamp", preamp)] {
            let apart = (hue(accent) - hue(dac)).abs();
            let apart = apart.min(360.0 - apart);
            assert!(
                apart <= ONE_HUE,
                "{name}: the {family} accent is {apart}° round from the DAC's"
            );
        }
    }
    let neon = preset("neon").expect("the neon preset");
    assert_eq!(
        [neon.dac, neon.amp, neon.preamp],
        [(0xff, 0x51, 0xfd), (0x2d, 0xe2, 0xe6), (0xa3, 0x6b, 0xff)],
        "neon keeps its three hues"
    );
}

/// The roles a fault, a warning and a good state are drawn in, which no glow may move.
const SEMANTIC: [Role; 4] = [Role::Good, Role::Warn, Role::Alert, Role::Fault];
/// How many places along the glow a sweep of it is sampled at.
const STEPS: u32 = 36;

/// No place along the glow changes the accent, a semantic colour, or the attributes one
/// carries: not the good, warning and alert colours, and not the reverse video a fault
/// takes where the palette draws it near the accent, which is a property of the palette
/// and not of where the glow has been carried.
#[test]
fn no_step_of_the_glow_moves_the_accent_or_a_semantic_colour() {
    for name in ["amber", "ice", "red", "neon"] {
        let config = TuiConfig {
            palette: name.into(),
            ..TuiConfig::default()
        };
        let mut theme = Theme::from_config(&config, false, ColorDepth::TrueColor);
        for family in [Family::Dac, Family::Amp, Family::Preamp] {
            theme.set_family(family);
            theme.set_glow(0.0);
            let at_rest = SEMANTIC.map(|role| theme.style(role));
            let accent = theme.accent();
            for step in 0..=STEPS {
                let along = -1.0 + 2.0 * step as f32 / STEPS as f32;
                theme.set_glow(along);
                for (role, was) in SEMANTIC.into_iter().zip(&at_rest) {
                    assert_eq!(
                        theme.style(role),
                        *was,
                        "{name}/{family:?}: {role:?} moved at {along}"
                    );
                }
                assert_eq!(
                    theme.accent(),
                    accent,
                    "{name}/{family:?}: the accent moved at {along}"
                );
            }
        }
    }
}

/// A named override is the colour the glow rests on, and it draws as its own ratatui
/// colour only while the glow is resting there: carried off it, the glow is somewhere
/// between two stops, and every decorated piece of a frame has to be on the same colour.
#[test]
fn a_named_accent_rests_on_its_slot_while_the_glow_leaves_it() {
    let cfg = Config::parse("schema = 1\n[tui]\n[tui.accents]\ndac = \"red\"\n").unwrap();
    let named = palette::named("red").expect("the red slot").1;
    let mut theme = Theme::from_config(&cfg.tui, false, ColorDepth::TrueColor);
    theme.set_family(Family::Dac);
    assert_eq!(theme.accent(), Color::Red, "at rest it is its own slot");
    assert_eq!(theme.glow(), Color::Red, "and so is the glow resting on it");
    assert_eq!(
        theme.glow_rgb(),
        named,
        "the glow rests on the override rather than on the palette's own stop"
    );

    theme.set_glow(1.0);
    let (r, g, b) = theme.glow_rgb();
    assert_eq!(theme.accent(), Color::Red, "the accent left its slot");
    assert_eq!(
        theme.glow(),
        Color::Rgb(r, g, b),
        "the glow stayed behind on the slot it left"
    );

    theme.set_glow(0.0);
    assert_eq!(theme.glow(), Color::Red, "it did not come back to its slot");
}

#[test]
fn an_unknown_palette_is_a_config_error_and_an_override_wins() {
    assert!(Config::parse("schema = 1\n[tui]\npalette = \"sepia\"\n").is_err());
    let cfg =
        Config::parse("schema = 1\n[tui]\npalette = \"neon\"\n[tui.accents]\namp = \"#123456\"\n")
            .unwrap();
    let mut theme = Theme::from_config(&cfg.tui, false, ColorDepth::TrueColor);
    theme.set_family(Family::Amp);
    assert_eq!(theme.accent_rgb(), (0x12, 0x34, 0x56));
    theme.set_family(Family::Dac);
    assert_eq!(theme.accent_rgb(), (0xff, 0x51, 0xfd));
}

#[test]
fn the_single_accent_key_overrides_every_family() {
    let cfg =
        Config::parse("schema = 1\n[tui]\naccent = \"magenta\"\npalette = \"ice\"\n").unwrap();
    let mut theme = Theme::from_config(&cfg.tui, false, ColorDepth::TrueColor);
    for family in [Family::Dac, Family::Amp, Family::Preamp] {
        theme.set_family(family);
        assert_eq!(theme.accent(), Color::Magenta, "{family:?}");
    }
}

#[test]
fn sixteen_colours_map_the_accents_to_yellow_cyan_and_red() {
    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    theme.set_family(Family::Dac);
    assert_eq!(theme.accent(), Color::Yellow);
    theme.set_family(Family::Amp);
    assert_eq!(theme.accent(), Color::Cyan);
    theme.set_family(Family::Preamp);
    assert_eq!(theme.accent(), Color::Red);

    // The mapping is the preset's own fallback, not a reduction of whatever colour is
    // named: `neon` keeps the same three slots.
    let cfg = Config::parse("schema = 1\n[tui]\npalette = \"neon\"\n").unwrap();
    let mut neon = Theme::from_config(&cfg.tui, false, ColorDepth::Ansi16);
    neon.set_family(Family::Amp);
    assert_eq!(neon.accent(), Color::Cyan);
}

#[test]
fn every_role_carries_a_colour_or_an_attribute_at_every_depth() {
    for palette in ["amber", "ice", "red", "neon"] {
        let text = format!("schema = 1\n[tui]\npalette = \"{palette}\"\n");
        let config = Config::parse(&text).expect(palette).tui;
        for depth in [
            ColorDepth::Ansi16,
            ColorDepth::Ansi256,
            ColorDepth::TrueColor,
        ] {
            let theme = Theme::from_config(&config, false, depth);
            for role in ALL_ROLES {
                let style = theme.style(role);
                assert!(
                    style.fg.is_some() || !style.add_modifier.is_empty(),
                    "{palette} at {depth:?}: {role:?} draws as nothing at all"
                );
            }
        }
    }
}

/// A theme on `palette`, drawing at `depth` for a device of `family`.
fn theme_on(palette: &str, family: Family, depth: ColorDepth) -> Theme {
    let config = TuiConfig {
        palette: palette.into(),
        ..TuiConfig::default()
    };
    let mut theme = Theme::from_config(&config, false, depth);
    theme.set_family(family);
    theme
}

/// A theme on `palette` drawing in true colour, where a blend of two stops has the depth
/// to show, for the device family a theme starts on.
fn true_colour(palette: &str) -> Theme {
    let config = TuiConfig {
        palette: palette.into(),
        ..TuiConfig::default()
    };
    Theme::from_config(&config, false, ColorDepth::TrueColor)
}

/// Every palette carries the stops its glow travels, and one of them is the accent it
/// gives a DAC: the glow leaves from the colour the screen is already drawing in.
#[test]
fn every_preset_rests_on_one_stop_of_its_own_spectrum() {
    for name in ["amber", "ice", "red", "neon"] {
        let p = preset(name).expect(name);
        assert!(
            (3..=5).contains(&p.spectrum.len()),
            "{name} has {} stops",
            p.spectrum.len()
        );
        assert_eq!(
            p.spectrum[p.rest], p.dac,
            "{name}: the resting stop is the DAC accent"
        );
    }
}

/// The glow runs from the resting stop to the cool end below it and to the hot end above
/// it, and lands between two stops anywhere in between.
#[test]
fn the_glow_travels_cool_below_rest_and_hot_above_it() {
    let amber = preset("amber").expect("the amber preset");
    let mut theme = true_colour("amber");
    theme.set_glow(0.0);
    assert_eq!(theme.glow_rgb(), amber.dac);
    theme.set_glow(-1.0);
    assert_eq!(theme.glow_rgb(), amber.spectrum[0], "the cool end");
    theme.set_glow(1.0);
    let hot = *amber.spectrum.last().expect("a hot end");
    assert_eq!(theme.glow_rgb(), hot, "the hot end");
    theme.set_glow(0.5);
    let mid = theme.glow_rgb();
    assert!(mid != hot && mid != amber.dac, "half way is a blend");
}

/// The stop the glow rests on is the accent of the family on the screen, whatever colour
/// that is: an amplifier rests on the copper it accents with and still reaches the hot end
/// of the palette's own stops.
#[test]
fn the_glow_rests_on_the_family_the_screen_is_showing() {
    let amber = preset("amber").expect("the amber preset");
    let hot = *amber.spectrum.last().expect("a hot end");
    let mut theme = true_colour("amber");
    for (family, resting) in [
        (Family::Dac, amber.dac),
        (Family::Amp, amber.amp),
        (Family::Preamp, amber.preamp),
    ] {
        theme.set_family(family);
        theme.set_glow(0.0);
        assert_eq!(
            theme.glow_rgb(),
            resting,
            "{family:?} does not rest on its own accent"
        );
        theme.set_glow(1.0);
        assert_eq!(
            theme.glow_rgb(),
            hot,
            "{family:?} does not reach the hot end"
        );
    }
}

/// The decoration takes the glow and the roles that carry a value keep the accent, so a
/// bar's length and a cursor's place read the same however far the glow has travelled.
#[test]
fn the_accent_rests_while_the_glow_moves() {
    let mut theme = true_colour("amber");
    let at_rest = theme.accent();
    theme.set_glow(0.8);
    assert_eq!(
        theme.accent(),
        at_rest,
        "bars, cursor and selection keep the resting accent"
    );
    assert_ne!(theme.glow(), at_rest);
    assert_eq!(theme.style(Role::Rule).fg, Some(theme.glow()));
    assert_eq!(theme.style(Role::KeyHint).fg, Some(theme.glow()));
    assert_eq!(theme.style(Role::Bar).fg, Some(at_rest));
}

/// A `[tui.spectrum]` of a user's own is the whole spectrum, and it rests on the stop
/// nearest the accent a DAC is drawn in — the config's own override for that family
/// where it names one, so a spectrum named around a user's accent rests on it rather than
/// on whichever stop comes nearest the palette's gold.
#[test]
fn a_user_spectrum_replaces_the_presets_and_rests_nearest_the_accent() {
    let config = TuiConfig {
        palette: "amber".into(),
        spectrum: vec!["#000000".into(), "#e0a030".into(), "#ffffff".into()],
        ..TuiConfig::default()
    };
    let mut theme = Theme::from_config(&config, false, ColorDepth::TrueColor);
    theme.set_glow(0.0);
    assert_eq!(theme.glow_rgb(), (0xe0, 0xa0, 0x30));
    theme.set_glow(1.0);
    assert_eq!(theme.glow_rgb(), (0xff, 0xff, 0xff));
    theme.set_glow(-1.0);
    assert_eq!(theme.glow_rgb(), (0x00, 0x00, 0x00));

    // A user's own DAC accent with stops named around it: the deep blue is what the
    // spectrum rests on, and the palette's gold decides nothing.
    let mine = TuiConfig {
        palette: "amber".into(),
        accents: FamilyAccents {
            dac: Some("#204060".into()),
            ..FamilyAccents::default()
        },
        spectrum: vec!["#102030".into(), "#204060".into(), "#e0a030".into()],
        ..TuiConfig::default()
    };
    let mut theme = Theme::from_config(&mine, false, ColorDepth::TrueColor);
    theme.set_glow(0.0);
    assert_eq!(
        theme.glow_rgb(),
        (0x20, 0x40, 0x60),
        "the spectrum rested on the stop nearest the palette's own gold rather than on \
             the accent the config draws a DAC in"
    );
    theme.set_glow(1.0);
    assert_eq!(theme.glow_rgb(), (0xe0, 0xa0, 0x30));
    theme.set_glow(-1.0);
    assert_eq!(theme.glow_rgb(), (0x10, 0x20, 0x30));
}

/// The glow travels only where there is room to travel: a screen of sixteen colours draws
/// it as the accent itself however far along the spectrum it is, and a position past
/// either end of the spectrum is held at that end rather than carried off it.
#[test]
fn the_glow_rests_on_the_accent_at_sixteen_colours_and_stops_at_the_spectrums_ends() {
    let mut sixteen = Theme::new(false);
    sixteen.set_glow(0.7);
    assert_eq!(
        sixteen.glow(),
        sixteen.accent(),
        "sixteen colours drew a glow on its way between two of them"
    );

    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    theme.set_glow(1.0);
    let hot = theme.glow_rgb();
    theme.set_glow(3.0);
    assert_eq!(theme.glow_rgb(), hot, "the glow travelled past the hot end");
    theme.set_glow(-1.0);
    let cool = theme.glow_rgb();
    theme.set_glow(-3.0);
    assert_eq!(
        theme.glow_rgb(),
        cool,
        "the glow travelled past the cool end"
    );
    assert_ne!(hot, cool, "the two ends of the spectrum are one colour");
}

/// The lifted glow is the spectrum's own colour at the position asked for, brighter: well
/// clear of what a contour drawn in the glow itself carries, short of white, and
/// travelling with the position rather than with the glow the screen is drawing at. A
/// screen of sixteen colours has no brighter form to lift into and draws the accent.
#[test]
fn the_lifted_glow_is_the_glow_itself_brighter() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let Color::Rgb(r, g, b) = theme.glow_lift_at(0.0) else {
        panic!("the lifted glow is not a colour at true colour");
    };
    let rest = theme.accent_rgb();
    assert!(
        palette::distance((r, g, b), rest) > 100.0,
        "the light would not part from the contour it runs over: {r},{g},{b} on {rest:?}"
    );
    assert_ne!(
        (r, g, b),
        WHITE,
        "the light is drawn in white, not in colour"
    );
    assert!(
        r >= rest.0 && g >= rest.1 && b >= rest.2,
        "the lift made the glow darker: {r},{g},{b} from {rest:?}"
    );

    assert_ne!(
        theme.glow_lift_at(-1.0),
        theme.glow_lift_at(1.0),
        "the lift does not follow the position it is taken at"
    );
    assert_eq!(
        theme.glow_lift_at(3.0),
        theme.glow_lift_at(1.0),
        "the lift travelled past the hot end"
    );
    let mut travelled = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    travelled.set_glow(1.0);
    assert_eq!(
        travelled.glow_lift_at(0.0),
        theme.glow_lift_at(0.0),
        "the lift followed the glow the screen is drawing at rather than its own position"
    );

    let sixteen = Theme::new(false);
    assert_eq!(
        sixteen.glow_lift_at(1.0),
        sixteen.accent(),
        "sixteen colours drew a lifted glow they have no colour for"
    );
}

/// A breathing contour is shaded in the colour the glow is at: the resting accent's own
/// ramp while the glow rests, and the travelled colour's once it has moved.
#[test]
fn a_shade_of_the_glow_follows_it_along_the_spectrum() {
    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    assert_eq!(
        theme.glow_shade(1.0),
        theme.accent_shade(1.0),
        "a glow at rest shaded something other than the accent"
    );
    theme.set_glow(1.0);
    assert_eq!(
        theme.glow_shade(1.0),
        Color::Rgb(0xf6, 0xdc, 0xa0),
        "the full shade of the glow is not the spectrum's hot end"
    );
    assert_ne!(
        theme.glow_shade(1.0),
        theme.accent_shade(1.0),
        "the shade stayed on the accent the glow had left"
    );
    assert_ne!(
        theme.glow_shade(0.0),
        theme.glow_shade(1.0),
        "the shade does not ramp"
    );
}

#[test]
fn a_fault_takes_the_inverse_row_where_the_accent_comes_near_the_alert_colour() {
    let reversed = |theme: &Theme| {
        theme
            .style(Role::Fault)
            .add_modifier
            .contains(Modifier::REVERSED)
    };
    // The `red` preset accents a DAC in a red a fault's own red sits right beside.
    assert!(reversed(&theme_on(
        "red",
        Family::Dac,
        ColorDepth::TrueColor
    )));
    // The `amber` preset's gold is far enough from it to tell apart by colour alone.
    assert!(!reversed(&theme_on(
        "amber",
        Family::Dac,
        ColorDepth::TrueColor
    )));
    // So are the other shades amber and red draw their other families in: the red DAC is
    // the one accent that stands beside its own alert colour.
    assert!(!reversed(&theme_on(
        "amber",
        Family::Amp,
        ColorDepth::TrueColor
    )));
    assert!(!reversed(&theme_on(
        "red",
        Family::Preamp,
        ColorDepth::TrueColor
    )));
    // Every shade of ice is far from red whichever device it is on.
    for family in [Family::Dac, Family::Amp, Family::Preamp] {
        assert!(!reversed(&theme_on("ice", family, ColorDepth::TrueColor)));
    }
    // At 16 colours a preamplifier's accent is the very slot a fault draws in.
    assert!(reversed(&theme_on(
        "amber",
        Family::Preamp,
        ColorDepth::Ansi16
    )));
    assert!(!reversed(&theme_on(
        "amber",
        Family::Amp,
        ColorDepth::Ansi16
    )));
    // The colour is the alert colour either way: the row is what changes.
    for theme in [
        theme_on("red", Family::Dac, ColorDepth::TrueColor),
        theme_on("amber", Family::Dac, ColorDepth::TrueColor),
    ] {
        assert_eq!(theme.style(Role::Fault).fg, theme.style(Role::Alert).fg);
    }
}

#[test]
fn the_focused_curve_point_goes_white_at_sixteen_colours() {
    let sixteen = theme_on("amber", Family::Dac, ColorDepth::Ansi16);
    let style = sixteen.style(Role::CurveSelected);
    assert_eq!(
        style.fg,
        Some(Color::White),
        "no other role draws white, so the focused band parts from the accent and from warn"
    );
    assert!(style.add_modifier.contains(Modifier::BOLD));
    assert_ne!(style.fg, sixteen.style(Role::CurvePoint).fg);

    let true_colour = theme_on("amber", Family::Dac, ColorDepth::TrueColor);
    assert_eq!(
        true_colour.style(Role::CurveSelected).fg,
        true_colour.style(Role::Warn).fg,
        "above 16 colours the focused band keeps the warn colour"
    );
}

#[test]
fn the_semantic_colours_keep_their_ansi_slots_at_sixteen_colours() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    assert_eq!(theme.style(Role::Good).fg, Some(Color::Green));
    assert_eq!(theme.style(Role::Warn).fg, Some(Color::Yellow));
    assert_eq!(theme.style(Role::Alert).fg, Some(Color::Red));
}

#[test]
fn selected_takes_the_accent_and_is_bold_and_inverse_at_sixteen_colours() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let style = theme.style(Role::Selected);
    assert_eq!(style.fg, Some(Color::Yellow));
    assert!(
        style
            .add_modifier
            .contains(Modifier::BOLD | Modifier::REVERSED)
    );
}

#[test]
fn no_color_forces_monochrome_over_a_colour_config() {
    let theme = Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor);
    assert_every_role_is_colourless(&theme);
}

#[test]
fn config_monochrome_forces_monochrome_without_no_color() {
    let config = TuiConfig {
        monochrome: true,
        ..TuiConfig::default()
    };
    let theme = Theme::from_config(&config, false, ColorDepth::Ansi256);
    assert_every_role_is_colourless(&theme);
}

#[test]
fn a_named_accent_draws_as_its_ratatui_slot_at_every_depth() {
    let config = TuiConfig {
        accent: Some("bright-cyan".into()),
        ..TuiConfig::default()
    };
    let sixteen = Theme::from_config(&config, false, ColorDepth::Ansi16);
    let true_colour = Theme::from_config(&config, false, ColorDepth::TrueColor);
    assert_eq!(sixteen.style(Role::Bar).fg, Some(Color::LightCyan));
    assert_eq!(true_colour.style(Role::Bar).fg, Some(Color::LightCyan));
}

#[test]
fn a_hex_accent_reduces_to_the_nearest_colour_at_each_depth() {
    let config = TuiConfig {
        accent: Some("#5fafff".into()),
        ..TuiConfig::default()
    };
    let true_colour = Theme::from_config(&config, false, ColorDepth::TrueColor);
    let ansi256 = Theme::from_config(&config, false, ColorDepth::Ansi256);
    let ansi16 = Theme::from_config(&config, false, ColorDepth::Ansi16);
    assert_eq!(
        true_colour.style(Role::Bar).fg,
        Some(Color::Rgb(0x5f, 0xaf, 0xff))
    );
    assert_eq!(ansi256.style(Role::Bar).fg, Some(Color::Indexed(75)));
    assert_eq!(ansi16.style(Role::Bar).fg, Some(Color::LightBlue));
}

#[test]
fn config_colours_override_a_higher_detected_depth() {
    let config = TuiConfig {
        colors: ColorDepth::Ansi16,
        ..TuiConfig::default()
    };
    let theme = Theme::from_config(&config, false, ColorDepth::TrueColor);
    assert_eq!(theme.depth(), ColorDepth::Ansi16);
}

#[test]
fn detect_depth_from_reads_colorterm_before_term() {
    assert_eq!(
        detect_depth_from(Some("truecolor"), None),
        ColorDepth::TrueColor
    );
    assert_eq!(
        detect_depth_from(None, Some("xterm-256color")),
        ColorDepth::Ansi256
    );
    assert_eq!(detect_depth_from(None, Some("xterm")), ColorDepth::Ansi16);
    assert_eq!(
        detect_depth_from(None, Some("foo-direct")),
        ColorDepth::TrueColor
    );
}

#[test]
fn gradient_blends_in_true_colour_and_picks_a_stop_at_sixteen() {
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    match true_colour.gradient(0.5, &[Color::Green, Color::Red]) {
        Color::Rgb(r, g, b) => {
            // Halfway between pure green and pure red, red and green land on the same
            // channel value: neither stop is leaned on more than the other.
            assert_eq!(r, g, "a mid gradient between green and red is not even");
            assert!(r > 0, "a mid gradient leans on both stops");
            assert_eq!(b, 0, "green and red never carry blue");
        }
        other => panic!("expected an RGB blend, got {other:?}"),
    }

    let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let picked = sixteen.gradient(0.5, &[Color::Green, Color::Red]);
    assert!(
        picked == Color::Green || picked == Color::Red,
        "16 colours has no blend to draw, only the nearer stop: got {picked:?}"
    );

    // Stops always spread evenly: three stops switch at 1/3 and 2/3, not at whatever
    // fractions a caller (the meter, say) treats as meaningful.
    assert_eq!(
        sixteen.gradient(0.9, &[Color::Green, Color::Yellow, Color::Red]),
        Color::Red,
        "round(t * (n - 1)) picks the nearest of n evenly spread stops generically"
    );
}

#[test]
fn accent_shade_ramps_up_at_true_colour_and_is_flat_at_sixteen() {
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    assert_eq!(true_colour.accent_shade(1.0), Color::Rgb(0xe0, 0xa0, 0x30));
    assert_eq!(
        true_colour.accent_shade(0.0),
        Color::Rgb(0x5a, 0x40, 0x13),
        "the cold end is about 40% of the accent's own brightness"
    );

    let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    assert_eq!(sixteen.accent_shade(0.0), Color::Yellow);
    assert_eq!(sixteen.accent_shade(1.0), Color::Yellow);
}

#[test]
fn stale_dims_at_sixteen_colours_and_greys_at_256() {
    let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let style = sixteen.style(Role::Stale);
    assert_eq!(style.fg, None);
    assert!(style.add_modifier.contains(Modifier::DIM));

    let ansi256 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi256);
    assert_eq!(
        ansi256.style(Role::Stale).fg,
        Some(palette::nearest_cube(AMBER.dim)),
        "at 256 colours a held-back reading takes the preset's own grey"
    );

    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    assert_eq!(
        true_colour.style(Role::Stale).fg,
        Some(Color::Rgb(0x8a, 0x8a, 0x8a))
    );
    assert_eq!(
        true_colour.style(Role::Faint).fg,
        Some(Color::Rgb(0x5a, 0x5a, 0x5a)),
        "an empty bar segment sits a step below a held-back reading"
    );
}

/// At 256 colours the spectrum comes through the colour cube: the hue the tone curve
/// starts at lands on the cube's red and the one it ends at on a violet, so the curve
/// reads as a run of frequencies there as it does at true colour.
#[test]
fn the_spectrum_runs_red_to_violet_through_the_cube_at_256_colours() {
    let ansi256 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi256);
    assert_eq!(
        ansi256.curve_colour(0.0, 0.0, false),
        Some(Color::Indexed(196)),
        "the low end of the curve is not the cube's red"
    );
    assert_eq!(
        ansi256.curve_colour(0.0, 270.0, false),
        Some(Color::Indexed(93)),
        "the high end of the curve is not a cube violet"
    );
}

/// A theme with no spectrum to draw in answers none at every hue, and the curve keeps the
/// role it would have drawn in: 16 colours land a whole hue on one of six slots, and
/// monochrome has no colour at all to land it on, whatever depth it draws at.
#[test]
fn a_theme_with_no_spectrum_answers_none_at_every_hue() {
    let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let monochrome = |depth| Theme::from_config(&TuiConfig::default(), true, depth);
    for degrees in [0.0, 90.0, 180.0, 270.0, 359.0] {
        assert_eq!(
            sixteen.curve_colour(0.0, degrees, false),
            None,
            "16 colours at {degrees}°"
        );
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            assert_eq!(
                monochrome(depth).curve_colour(0.0, degrees, false),
                None,
                "monochrome at {depth:?}, {degrees}°"
            );
        }
    }
}

/// The muted spectrum keeps each hue's own colour rather than blending toward a fixed
/// grey: it stays less saturated than [`Theme::curve_colour`]'s unmuted colour at the same hue, and
/// is not the flat dim grey — the low end stays reddish and the high end blue-violet, both
/// just less so.
#[test]
fn the_muted_spectrum_keeps_the_hue_desaturated_and_darker() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let dim = theme.style(Role::Dim).fg.expect("the dim role's colour");
    let spread = |r: u8, g: u8, b: u8| r.max(g).max(b) - r.min(g).min(b);
    for degrees in [0.0, 90.0, 180.0, 270.0] {
        let Some(Color::Rgb(pr, pg, pb)) = theme.curve_colour(0.0, degrees, false) else {
            panic!("no spectrum at {degrees}°");
        };
        let Some(muted @ Color::Rgb(mr, mg, mb)) = theme.curve_colour(0.0, degrees, true) else {
            panic!("no muted spectrum at {degrees}°");
        };
        assert!(
            spread(mr, mg, mb) < spread(pr, pg, pb),
            "{degrees}°: muted {mr},{mg},{mb} is not less saturated than {pr},{pg},{pb}"
        );
        assert_ne!(
            muted, dim,
            "{degrees}°: the muted spectrum is the flat dim grey"
        );
    }
    let Some(Color::Rgb(r, g, b)) = theme.curve_colour(0.0, 0.0, true) else {
        panic!("no muted spectrum at the low end");
    };
    assert!(
        r > g && r > b,
        "the low end is not still reddish: {r},{g},{b}"
    );
    let Some(Color::Rgb(r, g, b)) = theme.curve_colour(0.0, 270.0, true) else {
        panic!("no muted spectrum at the high end");
    };
    assert!(
        b > g && b >= r,
        "the high end is not still blue-violet: {r},{g},{b}"
    );
}

/// A theme with no spectrum to draw in has no muted one either, for the same reasons.
#[test]
fn a_theme_with_no_spectrum_has_no_muted_one_either() {
    let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let monochrome = Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor);
    for degrees in [0.0, 90.0, 270.0] {
        assert_eq!(sixteen.curve_colour(0.0, degrees, true), None, "{degrees}°");
        assert_eq!(
            monochrome.curve_colour(0.0, degrees, true),
            None,
            "{degrees}°"
        );
    }
}

/// `red` takes its accents from the two ANSI reds at 16 colours — scarlet and crimson the red,
/// salmon the bright red — and draws its emphasis in the text's own white, while the other
/// presets take yellow, cyan and red and have no emphasis but the accent.
#[test]
fn red_accents_in_the_two_reds_at_sixteen_colours_and_emphasises_in_white() {
    for (family, slot) in [
        (Family::Dac, Color::Red),
        (Family::Amp, Color::Red),
        (Family::Preamp, Color::LightRed),
    ] {
        let red = theme_on("red", family, ColorDepth::Ansi16);
        assert_eq!(red.accent(), slot, "red's {family:?} accent");
        assert_eq!(red.emphasis(), Color::Gray, "red's {family:?} emphasis");
        assert_eq!(red.style(Role::Selected).fg, Some(Color::Gray));
        assert_eq!(red.style(Role::Bar).fg, Some(Color::Gray));
    }
    for (name, slots) in [
        ("amber", [Color::Yellow, Color::Cyan, Color::Red]),
        ("ice", [Color::Yellow, Color::Cyan, Color::Red]),
        ("neon", [Color::LightMagenta, Color::Cyan, Color::Magenta]),
    ] {
        for (family, slot) in [Family::Dac, Family::Amp, Family::Preamp]
            .into_iter()
            .zip(slots)
        {
            let theme = theme_on(name, family, ColorDepth::Ansi16);
            assert_eq!(theme.accent(), slot, "{name}'s {family:?} accent");
            assert_eq!(theme.emphasis(), slot, "{name} has an emphasis of its own");
        }
    }
}

/// On `red` the bars, the selected row's inverse, a pane's title and the curve's own role
/// draw in the off-white text, which stays put while the glow travels, and the accent is
/// left the rule, the key hints, the `▶` marker and the band points, glow and all.
#[test]
fn reds_emphasis_takes_the_bars_the_selection_and_the_titles_from_the_accent() {
    let white = Color::Rgb(0xf2, 0xf0, 0xea);
    for family in [Family::Dac, Family::Amp, Family::Preamp] {
        let mut red = theme_on("red", family, ColorDepth::TrueColor);
        for glow in [0.0, 0.7] {
            red.set_glow(glow);
            let drawn = format!("{family:?} at glow {glow}");
            assert_eq!(red.emphasis(), white, "{drawn}");
            assert_eq!(red.style(Role::Bar).fg, Some(white), "{drawn}: a bar");
            assert_eq!(red.style(Role::CurveSum).fg, Some(white), "{drawn}");
            let selected = red.style(Role::Selected);
            assert_eq!(selected.fg, Some(white), "{drawn}: the selected row");
            assert!(
                selected
                    .add_modifier
                    .contains(Modifier::BOLD | Modifier::REVERSED),
                "{drawn}: the selected row is not bold inverse video"
            );
            let title = red.style(Role::Title);
            assert_eq!(title.fg, Some(white), "{drawn}: a pane's title");
            assert!(title.add_modifier.contains(Modifier::BOLD), "{drawn}");
            for t in [0.0, 0.5, 1.0] {
                assert_eq!(red.bar_shade(t), white, "{drawn}: a bar's cell at {t}");
            }
            assert_eq!(
                red.bar_dim(),
                Style::default().fg(Color::Rgb(0x61, 0x60, 0x5e)),
                "{drawn}"
            );

            let accent = red.accent();
            assert_ne!(accent, white, "{drawn}");
            for role in [Role::Cursor, Role::CurvePoint] {
                assert_eq!(red.style(role).fg, Some(accent), "{drawn}: {role:?}");
            }
            for role in [Role::Rule, Role::KeyHint] {
                assert_eq!(red.style(role).fg, Some(red.glow()), "{drawn}: {role:?}");
            }
        }
    }
}

/// A palette with no emphasis colour draws its bars, its selection, its titles and its
/// curve in the accent and the hue circle: the emphasis is the accent, a bar shades along the
/// accent's own ramp, a title takes the glow, bold, and on a palette with no curve stops of its
/// own (amber, ice) the curve runs round the hue circle.
#[test]
fn a_palette_without_an_emphasis_keeps_its_bars_selection_and_titles_in_the_accent() {
    for name in ["amber", "ice", "neon"] {
        for depth in [
            ColorDepth::Ansi16,
            ColorDepth::Ansi256,
            ColorDepth::TrueColor,
        ] {
            for monochrome in [false, true] {
                let config = TuiConfig {
                    palette: name.into(),
                    ..TuiConfig::default()
                };
                let mut theme = Theme::from_config(&config, monochrome, depth);
                theme.set_glow(0.6);
                let drawn = format!("{name} at {depth:?}, monochrome {monochrome}");
                assert!(!theme.emphasised(), "{drawn}");
                assert_eq!(theme.emphasis(), theme.accent(), "{drawn}");
                assert_eq!(
                    theme.style(Role::Title),
                    theme.style(Role::Rule).patch(theme.style(Role::Header)),
                    "{drawn}: a title"
                );
                assert_eq!(
                    theme.style(Role::CurveSum),
                    theme.style(Role::Text),
                    "{drawn}"
                );
                for t in [0.0, 0.5, 1.0] {
                    assert_eq!(theme.bar_shade(t), theme.accent_shade(t), "{drawn}");
                }
                let dim = match (monochrome, depth) {
                    (true, _) => Style::default(),
                    (false, ColorDepth::Ansi16) => Style::default()
                        .fg(theme.accent())
                        .add_modifier(Modifier::DIM),
                    (false, _) => Style::default().fg(theme.accent_shade(0.0)),
                };
                assert_eq!(theme.bar_dim(), dim, "{drawn}");
                if preset(name).expect(name).curve.is_some() {
                    continue;
                }
                for along in [0.0, 0.5, 1.0] {
                    let degrees = 45.0 + SPECTRUM_ARC * along;
                    assert_eq!(
                        theme.curve_colour(along, 45.0, false),
                        theme.curve_colour(0.0, degrees, false),
                        "{drawn}: the curve left the hue circle"
                    );
                    assert_eq!(
                        theme.curve_colour(along, 45.0, true),
                        theme.curve_colour(0.0, degrees, true),
                        "{drawn}: the muted curve left the hue circle"
                    );
                }
            }
        }
    }
}

/// A `red` preamplifier accents in the bright red at 16 colours, which is the fault's own red
/// but for its brightness: a fault takes the inverse row there, as it does on every `red`
/// device at 16 colours, while an accent a whole hue away is still told apart by colour.
#[test]
fn a_fault_beside_a_bright_red_accent_takes_the_inverse_row_at_sixteen_colours() {
    let reversed = |theme: &Theme| {
        theme
            .style(Role::Fault)
            .add_modifier
            .contains(Modifier::REVERSED)
    };
    for family in [Family::Dac, Family::Amp, Family::Preamp] {
        assert!(
            reversed(&theme_on("red", family, ColorDepth::Ansi16)),
            "a red {family:?}'s fault at 16 colours"
        );
    }
    for family in [Family::Dac, Family::Amp] {
        assert!(!reversed(&theme_on("amber", family, ColorDepth::Ansi16)));
    }
}

/// Whether `theme` draws a fault on the inverse row.
fn fault_reversed(theme: &Theme) -> bool {
    theme
        .style(Role::Fault)
        .add_modifier
        .contains(Modifier::REVERSED)
}

/// Amber, ice and neon draw their own accents' faults by colour alone at every depth, but for
/// an amber or ice preamplifier's at 16 colours, whose slot is the alert's own red and whose
/// faults take the inverse row there; neon's preamplifier is the magenta there.
#[test]
fn a_presets_own_accents_leave_their_faults_off_the_inverse_row_but_on_the_red_slot() {
    for (name, preamp_on_red) in [("amber", true), ("ice", true), ("neon", false)] {
        for (family, at_sixteen) in [
            (Family::Dac, false),
            (Family::Amp, false),
            (Family::Preamp, preamp_on_red),
        ] {
            let sixteen = theme_on(name, family, ColorDepth::Ansi16);
            assert_eq!(
                fault_reversed(&sixteen),
                at_sixteen,
                "{name}'s {family:?} at 16 colours"
            );
            for depth in [ColorDepth::Ansi256, ColorDepth::TrueColor] {
                assert!(
                    !fault_reversed(&theme_on(name, family, depth)),
                    "{name}'s {family:?} at {depth:?}"
                );
            }
        }
    }
}

/// An accent the config sets close to the alert red — the bright red by name, a hex that
/// comes down to it at 16 colours, or a hex beside the alert colour at true colour — puts a
/// fault on the inverse row, whatever palette it is set over, so the fault still parts from
/// the accent beside it; one a whole hue away leaves the fault to its colour.
#[test]
fn an_accent_set_beside_the_alert_red_puts_faults_on_the_inverse_row() {
    let theme = |text: &str, depth| {
        let config = Config::parse(&format!("schema = 1\n[tui]\n{text}\n")).expect(text);
        let mut theme = Theme::from_config(&config.tui, false, depth);
        theme.set_family(Family::Dac);
        theme
    };
    for (text, depth, reversed) in [
        ("accent = \"bright-red\"", ColorDepth::Ansi16, true),
        ("accent = \"#ff1010\"", ColorDepth::Ansi16, true),
        ("accent = \"#ff4a3a\"", ColorDepth::TrueColor, true),
        ("accent = \"bright-red\"", ColorDepth::TrueColor, true),
        (
            "palette = \"ice\"\n[tui.accents]\ndac = \"bright-red\"",
            ColorDepth::Ansi16,
            true,
        ),
        ("accent = \"magenta\"", ColorDepth::Ansi16, false),
        ("accent = \"#5fafff\"", ColorDepth::TrueColor, false),
    ] {
        let drawn = theme(text, depth);
        assert_eq!(
            fault_reversed(&drawn),
            reversed,
            "{text} at {depth:?}, the accent drawn {:?}",
            drawn.accent()
        );
    }
    assert_eq!(
        theme("accent = \"#ff1010\"", ColorDepth::Ansi16).accent(),
        Color::LightRed,
        "the hex does not come down to the bright red"
    );
}

/// Every colour `red` draws — every role's, every shade and ramp's, the glow and its lift,
/// the transition's spread of the spectrum and the tone curve at every place and drift,
/// shaped and muted — is clear of blue and cyan, for every family at every depth.
#[test]
fn nothing_red_draws_is_blue_or_cyan() {
    let steps = |n: u32| (0..=n).map(move |i| i as f32 / n as f32);
    for depth in [
        ColorDepth::Ansi16,
        ColorDepth::Ansi256,
        ColorDepth::TrueColor,
    ] {
        for family in [Family::Dac, Family::Amp, Family::Preamp] {
            let mut red = theme_on("red", family, depth);
            let mut colours = Vec::new();
            for glow in steps(8).map(|t| 2.0 * t - 1.0) {
                red.set_glow(glow);
                for role in ALL_ROLES {
                    let style = red.style(role);
                    colours.extend([style.fg, style.bg].into_iter().flatten());
                }
                colours.push(red.glow());
                colours.push(red.glow_shade(0.5));
            }
            for t in steps(20) {
                let t64 = f64::from(t);
                colours.extend([
                    red.bar_shade(t64),
                    red.accent_shade(t64),
                    red.glow_lift_at(2.0 * t64 - 1.0),
                ]);
                colours.extend(red.along_spectrum(2.0 * t - 1.0));
                for drift in [0.0, 90.0, 180.0, 270.0] {
                    colours.extend(red.curve_colour(t, drift, false));
                    colours.extend(red.curve_colour(t, drift, true));
                }
            }
            colours.extend(red.bar_dim().fg);
            colours.extend(red.alert_ground().fg);
            for colour in colours {
                assert!(
                    !bluish(palette::rgb_of(colour)),
                    "red's {family:?} at {depth:?} draws {colour:?}"
                );
            }
        }
    }
}

/// `red`'s tone curve runs a warm ramp — dark red at the lowest frequency, the scarlet a
/// third of the way, the warning amber two thirds, the off-white at the highest — and the
/// rainbow scroll slides it along those stops and back, a half turn carrying the far end
/// to the near one and the turn closing on where it started, never jumping. Muted, it is
/// the same ramp desaturated and darker. There is no curve colour to draw at 16 colours.
#[test]
fn reds_curve_runs_a_warm_ramp_that_the_scroll_slides_and_brings_back() {
    let red = true_colour("red");
    let at = |along: f32, drift: f32| red.curve_colour(along, drift, false);
    assert_eq!(at(0.0, 0.0), Some(Color::Rgb(0xa0, 0x24, 0x14)), "20 Hz");
    assert_eq!(at(1.0 / 3.0, 0.0), Some(Color::Rgb(0xe0, 0x34, 0x1c)));
    assert_eq!(at(2.0 / 3.0, 0.0), Some(Color::Rgb(0xd9, 0xa4, 0x41)));
    assert_eq!(at(1.0, 0.0), Some(Color::Rgb(0xf2, 0xf0, 0xea)), "20 kHz");
    assert_eq!(at(0.0, 180.0), at(1.0, 0.0), "a half turn is the far end");
    assert_eq!(
        at(0.25, 90.0),
        at(0.75, 0.0),
        "a quarter turn slides half way"
    );
    assert_ne!(at(0.25, 90.0), at(0.25, 0.0), "the scroll moved nothing");
    let near = |a: Option<Color>, b: Option<Color>| {
        let (a, b) = (palette::rgb_of(a.unwrap()), palette::rgb_of(b.unwrap()));
        palette::distance(a, b) < 4.0
    };
    assert!(
        near(at(0.4, 359.9), at(0.4, 0.0)),
        "the turn jumped as it closed"
    );
    assert!(near(at(0.4, 180.1), at(0.4, 179.9)), "the far end jumped");

    let spread = |c: Option<Color>| {
        let (r, g, b) = palette::rgb_of(c.expect("a colour"));
        r.max(g).max(b) - r.min(g).min(b)
    };
    let bright = |c: Option<Color>| {
        let (r, g, b) = palette::rgb_of(c.expect("a colour"));
        u16::from(r) + u16::from(g) + u16::from(b)
    };
    for along in [0.0, 0.3, 0.6, 0.9] {
        let (shaped, muted) = (at(along, 0.0), red.curve_colour(along, 0.0, true));
        assert!(
            spread(muted) < spread(shaped),
            "{along}: muted is as saturated"
        );
        assert!(
            bright(muted) < bright(shaped),
            "{along}: muted is as bright"
        );
    }
    let sixteen = theme_on("red", Family::Dac, ColorDepth::Ansi16);
    assert_eq!(sixteen.curve_colour(0.5, 0.0, false), None);
    let monochrome = Theme::from_config(
        &TuiConfig {
            palette: "red".into(),
            ..TuiConfig::default()
        },
        true,
        ColorDepth::TrueColor,
    );
    assert_eq!(monochrome.curve_colour(0.5, 0.0, true), None);
}

/// The Spectrum row draws the curve's rainbow at half the colourfulness: the hue kept, less
/// saturated than the curve and more than the muted curve, on a preset with its own stops too,
/// and none at 16 colours or monochrome.
#[test]
fn the_spectrum_row_is_a_softer_rainbow() {
    let spread = |c: Option<Color>| {
        let (r, g, b) = palette::rgb_of(c.expect("a colour"));
        i32::from(r.max(g).max(b)) - i32::from(r.min(g).min(b))
    };
    for name in ["red", "neon", "ember"] {
        let theme = true_colour(name);
        for along in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (row, full, muted) = (
                theme.spectrum_row_colour(along),
                theme.curve_colour(along, 0.0, false),
                theme.curve_colour(along, 0.0, true),
            );
            assert!(spread(row) < spread(full), "{name} {along}: not softer");
            assert!(
                spread(row) > spread(muted),
                "{name} {along}: not more than muted"
            );
        }
    }
    let default = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let hue = |c: Option<Color>| {
        let (r, g, b) = palette::rgb_of(c.expect("a colour"));
        let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let degrees = if max == r {
            (g - b) / (max - min) * 60.0
        } else if max == g {
            120.0 + (b - r) / (max - min) * 60.0
        } else {
            240.0 + (r - g) / (max - min) * 60.0
        };
        degrees.rem_euclid(360.0)
    };
    for along in [0.0, 0.3, 0.6, 0.9] {
        let (row, full) = (
            hue(default.spectrum_row_colour(along)),
            hue(default.curve_colour(along, 0.0, false)),
        );
        assert!(
            (row - full).abs() < 6.0,
            "{along}: hue {row} against {full}"
        );
    }
    let red = true_colour("red");
    for along in [0.1, 0.4, 0.7] {
        let (row, full) = (
            hue(red.spectrum_row_colour(along)),
            hue(red.curve_colour(along, 0.0, false)),
        );
        assert!(
            (row - full).abs() < 6.0,
            "red {along}: hue {row} against {full}"
        );
    }
    let sixteen = theme_on("red", Family::Dac, ColorDepth::Ansi16);
    assert_eq!(sixteen.spectrum_row_colour(0.5), None);
    let monochrome = Theme::from_config(
        &TuiConfig {
            palette: "red".into(),
            ..TuiConfig::default()
        },
        true,
        ColorDepth::TrueColor,
    );
    assert_eq!(monochrome.spectrum_row_colour(0.5), None);
}

/// `neon`'s tone curve runs a sunset of its own — violet at the lowest frequency, hot pink a
/// third of the way, orange two thirds, gold at the highest — and has no curve colour to draw
/// at 16 colours.
#[test]
fn neons_curve_runs_a_sunset() {
    let neon = true_colour("neon");
    let at = |along: f32| neon.curve_colour(along, 0.0, false);
    assert_eq!(at(0.0), Some(Color::Rgb(0x7b, 0x2f, 0xff)), "20 Hz");
    assert_eq!(at(1.0 / 3.0), Some(Color::Rgb(0xff, 0x3e, 0xa5)));
    assert_eq!(at(2.0 / 3.0), Some(Color::Rgb(0xff, 0x8a, 0x4c)));
    assert_eq!(at(1.0), Some(Color::Rgb(0xff, 0xd3, 0x6b)), "20 kHz");
    let sixteen = theme_on("neon", Family::Dac, ColorDepth::Ansi16);
    assert_eq!(sixteen.curve_colour(0.5, 0.0, false), None);
}

/// A theme on `palette` at `depth`.
fn themed(palette: &str, depth: ColorDepth, monochrome: bool, background: bool) -> Theme {
    let config = TuiConfig {
        palette: palette.into(),
        theme_background: background,
        ..TuiConfig::default()
    };
    Theme::from_config(&config, monochrome, depth)
}

/// At 256 colours each palette's three surfaces are three different indices, so a pane and
/// a box show against the ground.
#[test]
fn the_three_surfaces_are_three_distinct_indices_at_256_colours() {
    for name in ["amber", "ice", "red", "neon"] {
        let theme = themed(name, ColorDepth::Ansi256, false, true);
        let [ground, pane, raised] = [Surface::Ground, Surface::Pane, Surface::Raised]
            .map(|s| theme.surface(s).expect("a grey at 256 colours"));
        assert!(
            ground != pane && pane != raised && ground != raised,
            "{name}: {ground:?} {pane:?} {raised:?}"
        );
    }
}

/// Each palette paints its three greys exactly at true colour, as three entries in their own
/// order at 256 colours (the black and the two darkest steps of the grey ramp for the halved
/// greys of amber and ice, whose nearest entries would merge the ground and the pane, and the
/// ramp's three darkest steps for red and neon), and none at 16 colours, in monochrome or with
/// the switch off.
#[test]
fn each_palette_paints_its_own_greys_where_the_depth_allows() {
    let table = [
        (
            "amber",
            [(0x07, 0x06, 0x05), (0x0b, 0x0a, 0x08), (0x10, 0x0e, 0x0b)],
            [16, 232, 233],
        ),
        (
            "ice",
            [(0x05, 0x06, 0x07), (0x08, 0x0b, 0x0d), (0x0c, 0x10, 0x15)],
            [16, 232, 233],
        ),
        (
            "red",
            [(0x0c, 0x0c, 0x0c), (0x14, 0x14, 0x14), (0x1e, 0x1e, 0x1e)],
            [232, 233, 234],
        ),
        (
            "neon",
            [(0x0a, 0x06, 0x12), (0x10, 0x0a, 0x1c), (0x17, 0x10, 0x26)],
            [232, 233, 234],
        ),
    ];
    let surfaces = [Surface::Ground, Surface::Pane, Surface::Raised];
    for (name, greys, indices) in table {
        let truecolor = themed(name, ColorDepth::TrueColor, false, true);
        let cube = themed(name, ColorDepth::Ansi256, false, true);
        for ((surface, (r, g, b)), index) in surfaces.into_iter().zip(greys).zip(indices) {
            assert_eq!(
                truecolor.surface(surface),
                Some(Color::Rgb(r, g, b)),
                "{name} {surface:?}"
            );
            assert_eq!(
                cube.surface(surface),
                Some(Color::Indexed(index)),
                "{name} {surface:?}"
            );
            for none in [
                themed(name, ColorDepth::Ansi16, false, true),
                themed(name, ColorDepth::TrueColor, true, true),
                themed(name, ColorDepth::TrueColor, false, false),
                themed(name, ColorDepth::Ansi256, false, false),
            ] {
                assert_eq!(none.surface(surface), None, "{name} {surface:?}");
            }
        }
    }
}

/// A palette name no preset carries draws as amber, like every other bad `[tui]` value the parser
/// would have refused: the theme is total, the parser is strict.
#[test]
fn an_unknown_palette_draws_as_amber() {
    let bad = TuiConfig {
        palette: "no-such-palette".into(),
        ..TuiConfig::default()
    };
    let theme = Theme::from_config(&bad, false, ColorDepth::TrueColor);
    let amber = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    assert_eq!(theme.style(Role::Cursor), amber.style(Role::Cursor));
}

#[test]
fn dimmed_keeps_a_pictures_shading_in_greys_darkened_as_the_dim_text_is() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let near = |a: Color, b: (u8, u8, u8)| match a {
        Color::Rgb(r, g, bl) => r.abs_diff(b.0).max(g.abs_diff(b.1)).max(bl.abs_diff(b.2)) <= 1,
        _ => false,
    };
    assert!(near(theme.dimmed(theme.preset.text), theme.preset.dim));
    assert_eq!(theme.dimmed((0, 0, 0)), Color::Rgb(0, 0, 0));
    for rgb in [(200, 120, 20), (30, 90, 220), (255, 255, 255)] {
        let Color::Rgb(r, g, b) = theme.dimmed(rgb) else {
            panic!("true colour");
        };
        assert!(r == g && g == b, "{r} {g} {b}");
    }
    assert_ne!(theme.dimmed((250, 200, 90)), theme.dimmed((120, 60, 20)));
    let ansi256 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi256);
    assert!(
        matches!(ansi256.dimmed((200, 200, 200)), Color::Indexed(232..=255)),
        "{:?}",
        ansi256.dimmed((200, 200, 200))
    );
}
