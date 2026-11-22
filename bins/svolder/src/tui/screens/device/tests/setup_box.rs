// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Path row's config tag, `k` and the box it opens: the judge's words laid
//! out at 80x24 and 120x40, the keys, the click and the tooltip.

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use svolder_host::{
    AlsaFacts, ClockFacts, Dither, HwState, KernelOffer, Layout, PlayerMatch, Rates, RouteFacts,
    SetupFacts, SinkOffer,
};

use super::host_group::{daemon_hosting, gungnir_route};
use super::*;
use crate::tui::app::Side;
use crate::tui::boxes::tooltip;
use crate::tui::mouse::{self, MouseAction};
use crate::tui::test_support::app_showing_on;

fn layout(positions: &[&str]) -> Layout {
    Layout {
        channels: positions.len() as u32,
        positions: positions.iter().map(|p| p.to_string()).collect(),
    }
}

/// Config facts under which the Gungnir route stays untouched: a wide offer, a clock that allows
/// the sink's rate, stereo on both sides, no dither, no filter, an ALSA `hw:` device.
fn clean_setup() -> SetupFacts {
    SetupFacts {
        offer: Some(SinkOffer {
            formats: vec!["S32LE".into(), "S16LE".into()],
            rates: Some(Rates::Range {
                min: 44_100,
                max: 384_000,
            }),
            layout: Some(layout(&["FL", "FR"])),
        }),
        forced_format: None,
        node_rates: Vec::new(),
        clock: Some(ClockFacts {
            rate: Some(48_000),
            allowed_rates: vec![44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
            force_rate: None,
        }),
        sink_layout: Some(layout(&["FL", "FR"])),
        stream_layout: Some(layout(&["FL", "FR"])),
        dither: Some(Dither {
            method: "none".into(),
            noise: 0,
        }),
        sink_filter: Some(false),
        stream_filter: Some(false),
        alsa: Some(AlsaFacts {
            path: Some("hw:Audio".into()),
            card: Some(3),
            device: Some(0),
            subdevice: Some(0),
            hw: HwState::NotRead,
            kernel: Some(KernelOffer {
                formats: vec!["S32_LE".into()],
                rates: vec![44_100, 48_000],
            }),
        }),
    }
}

/// The Gungnir route with `edit` applied to its config facts.
fn route_with(edit: impl FnOnce(&mut SetupFacts)) -> RouteFacts {
    gungnir_route(|route| {
        let mut setup = clean_setup();
        edit(&mut setup);
        route.setup = Some(setup);
    })
}

/// Two failing lines: the converter dithers and a filter sits in the sink.
fn two_fail(setup: &mut SetupFacts) {
    setup.dither = Some(Dither {
        method: "triangular".into(),
        noise: 0,
    });
    setup.sink_filter = Some(true);
}

fn press(app: &mut App, code: KeyCode) -> Vec<crate::tui::app::Intent> {
    app.key(KeyEvent::from(code))
}

/// One frame of `app` at `width` by `height`: its rows, its hits and where the modal box is.
fn drawn(app: &App, width: u16, height: u16) -> (Vec<String>, HitMap, Option<Rect>) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut modal = None;
    let theme = Theme::new(false);
    terminal
        .draw(|f| modal = render::draw(f, app, &theme, &mut hits).modal)
        .expect("the frame draws");
    (text(terminal.backend().buffer()), hits, modal)
}

/// The text inside the box `modal` drew, one string per row, its border dropped.
fn boxed(rows: &[String], modal: Rect) -> Vec<String> {
    rows.iter()
        .skip(usize::from(modal.y))
        .take(usize::from(modal.height))
        .map(|row| {
            row.chars()
                .skip(usize::from(modal.x))
                .take(usize::from(modal.width))
                .collect()
        })
        .collect()
}

/// The words of the box's inner lines, trimmed, blanks dropped, without the limit that ends the
/// box (see [`limit`]).
fn inner(rows: &[String], modal: Rect) -> Vec<String> {
    let mut all = inner_all(rows, modal);
    let at = all.iter().position(|row| row.starts_with("svolder judges"));
    all.truncate(at.unwrap_or(all.len()));
    all
}

/// The limit sentence as the box wraps it: its lines, joined by blanks.
fn limit(rows: &[String], modal: Rect) -> String {
    let all = inner_all(rows, modal);
    let at = all.iter().position(|row| row.starts_with("svolder judges"));
    all[at.expect("the limit is in the box")..].join(" ")
}

/// Every inner line of the box, trimmed, blanks dropped.
fn inner_all(rows: &[String], modal: Rect) -> Vec<String> {
    let all = boxed(rows, modal);
    all[1..all.len() - 1]
        .iter()
        .map(|row| {
            let chars: Vec<char> = row.chars().collect();
            chars[1..chars.len() - 1]
                .iter()
                .collect::<String>()
                .trim()
                .to_string()
        })
        .filter(|row| !row.is_empty())
        .collect()
}

/// The Path row's two lines of a frame: what follows its label (the hops), and what the line under
/// it carries (the values), each up to the pane's border.
fn path_lines(rows: &[String]) -> (String, String) {
    let at = rows
        .iter()
        .position(|row| row.contains(" Path "))
        .expect("a Path row");
    let pane = |row: &String| row.split('\u{2502}').nth(1).unwrap_or("").to_string();
    let first = pane(&rows[at]);
    let hops = first.split_once("Path").unwrap().1.trim().to_string();
    (hops, pane(&rows[at + 1]).trim().to_string())
}

/// The config check shows in the row only as `config N` among the values, only when it fails.
#[test]
fn the_config_tag_shows_only_on_a_failing_check() {
    let app = daemon_hosting(route_with(|_| {}));
    let (rows, _, _) = drawn(&app, 200, 30);
    let (hops, values) = path_lines(&rows);
    assert_eq!(hops, "player \u{2192} computer \u{2192} DAC USB 1");
    assert!(values.starts_with("48 kHz"), "{values}");
    assert!(!rows.iter().any(|row| row.contains("config")), "{rows:#?}");
    let app = daemon_hosting(route_with(two_fail));
    let (rows, _, _) = drawn(&app, 200, 30);
    let (_, values) = path_lines(&rows);
    assert!(values.ends_with("config 2"), "{values}");
}

/// The check follows the sink, as on the web: a player playing to another sink while the default
/// sink is this DAC still has its check; a default sink that is no DAC of this screen has none.
#[test]
fn the_check_follows_the_default_sink_whatever_plays() {
    let mut app = daemon_hosting(route_with(|_| {}));
    app.host.daemon.as_mut().unwrap().route = Some(Box::new(gungnir_route(|route| {
        route.setup = Some(clean_setup());
        route.player_match = PlayerMatch::Elsewhere {
            sink: "LG TV".into(),
        };
    })));
    assert_eq!(
        app.path_setup().map(|check| check.chip()).as_deref(),
        Some("config \u{2713}")
    );
    app.host.daemon.as_mut().unwrap().route = Some(Box::new(gungnir_route(|route| {
        route.setup = Some(clean_setup());
        route.sink.as_mut().unwrap().usb_product = Some("Other".into());
    })));
    assert!(app.path_setup().is_none());
}

#[test]
fn an_older_daemons_route_has_no_check() {
    let app = daemon_hosting(gungnir_route(|_| {}));
    let (rows, _, _) = drawn(&app, 200, 30);
    assert!(app.path_setup().is_none());
    assert!(!rows.iter().any(|row| row.contains("config")));
}

/// A failing check's tag is a value of the row and stays in sight at every width the screen is
/// used at, 80 columns included, at the scroll's start and with scrolling off.
#[test]
fn the_failing_tag_is_in_sight_at_every_width() {
    let mut app = daemon_hosting(route_with(two_fail));
    for scroll in [true, false] {
        app.tui.scroll_long_lines = scroll;
        for width in [80, 100, 120, 160, 200] {
            let (rows, _, _) = drawn(&app, width, 40);
            assert!(
                rows.iter().any(|row| row.contains("config 2")),
                "{width} scroll {scroll}: {rows:#?}"
            );
        }
    }
}

/// The tag is no target of its own: a click on it is the row's, which opens the box.
#[test]
fn a_click_on_the_config_tag_is_the_rows() {
    let app = daemon_hosting(route_with(two_fail));
    let (rows, hits, _) = drawn(&app, 200, 30);
    let y = rows
        .iter()
        .position(|row| row.contains("config 2"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    let at = rows[y].find("config 2").unwrap();
    let x = u16::try_from(rows[y][..at].chars().count()).unwrap();
    assert_eq!(hits.hit_at(x, u16::try_from(y).unwrap()), Some(&Hit::Path));
}

/// With `[tui] host_audio` off nothing of the host is shown or read: no `k check`, no key, no box.
#[test]
fn with_host_audio_off_the_check_is_not_offered() {
    let mut app = daemon_hosting(route_with(|_| {}));
    app.tui.host_audio = false;
    assert!(!app.setup_offered());
    let (rows, _, _) = drawn(&app, 120, 40);
    assert!(!rows.iter().any(|row| row.contains("k check")));
    assert!(press(&mut app, KeyCode::Char('k')).is_empty());
    assert!(app.overlays.setup.is_none());
    let mut off = daemon_hosting(route_with(|_| {}));
    off.host.linked = false;
    assert!(!off.setup_offered());
    press(&mut off, KeyCode::Char('k'));
    assert!(off.overlays.setup.is_none());
}

/// The chip is no target of its own: the click that opens the box is the row's, and the row's
/// tooltip ends in how many config lines pass.
#[test]
fn the_row_says_how_many_config_lines_pass() {
    let mut app = daemon_hosting(route_with(two_fail));
    app.mouse = true;
    let (rows, hits, _) = drawn(&app, 200, 30);
    let y = rows
        .iter()
        .position(|row| row.contains("config 2"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    let chip = rows[y].find("config 2").unwrap();
    let (x, y) = (
        u16::try_from(rows[y][..chip].chars().count()).unwrap() + 2,
        u16::try_from(y).unwrap(),
    );
    assert_eq!(hits.hit_at(x, y), Some(&Hit::Path));
    app.pointer.tooltip = Some(tooltip::Tooltip {
        hit: Hit::Path,
        at: (x, y),
        since: Instant::now(),
        shown: true,
    });
    let lines = tooltip::standing(&app, &hits).expect("a tooltip");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("config: 5 of 7 pass · k")
    );
}

#[test]
fn k_opens_the_box_on_a_dacs_screen_at_80x24() {
    let mut app = daemon_hosting(route_with(|_| {}));
    assert!(press(&mut app, KeyCode::Char('k')).is_empty());
    assert!(app.overlays.setup.is_some());
    let (rows, _, modal) = drawn(&app, 80, 24);
    let modal = modal.expect("the box is the modal");
    assert_eq!(
        inner(&rows, modal),
        [
            "Sink  Gungnir 2 Analog Stereo",
            "✓ rates                  44.1-48 kHz all allowed",
            "✓ forced rate            none",
            "✓ dither                 off",
            "✓ 24-bit output          S32LE in use",
            "✓ no plug                hw:Audio",
            "✓ stereo to stereo       FL FR",
            "✓ no filter in the sink  none",
            "PipeWire config lets it through untouched",
        ]
    );
    assert!(boxed(&rows, modal)[0].contains("PipeWire config"));
    assert!(boxed(&rows, modal)[0].contains("k or esc close"));
    // The judge's limit sentence ends the box, whole, and the box still fits.
    assert_eq!(limit(&rows, modal), svolder_ipc::host::SETUP_LIMIT);
    assert_eq!(
        boxed(&rows, modal).len(),
        17,
        "sink, 7 lines, summary, limit x3, 3 blanks, border"
    );
}

#[test]
fn a_failing_line_says_so_in_full_at_120x40() {
    let mut app = daemon_hosting(route_with(two_fail));
    press(&mut app, KeyCode::Char('k'));
    let (rows, _, modal) = drawn(&app, 120, 40);
    let lines = inner(&rows, modal.unwrap());
    assert_eq!(lines[3], "✗ dither                 triangular");
    assert_eq!(
        lines[7],
        "✗ no filter in the sink  a filter inside the sink"
    );
    assert_eq!(
        lines.last().unwrap(),
        "PipeWire config changes the sound: 2"
    );
}

#[test]
fn the_box_wraps_a_long_line_under_the_words_column_at_80x24() {
    let mut app = daemon_hosting(route_with(|setup| {
        setup.clock.as_mut().unwrap().allowed_rates = vec![48_000];
        setup.node_rates = vec![48_000];
        setup.clock.as_mut().unwrap().force_rate = Some(48_000);
    }));
    press(&mut app, KeyCode::Char('k'));
    let (rows, _, modal) = drawn(&app, 80, 24);
    let modal = modal.unwrap();
    let all = boxed(&rows, modal);
    assert!(all.len() <= 22, "the box fits the screen: {all:#?}");
    for row in &all {
        assert!(row.chars().count() <= 80);
    }
    let words = inner(&rows, modal).join("\n");
    assert!(words.contains("✗ rates"), "{words}");
    assert!(
        words.contains("PipeWire config changes the sound: "),
        "{words}"
    );
    // The box is capped at the screen, and the limit still ends it, whole.
    assert_eq!(limit(&rows, modal), svolder_ipc::host::SETUP_LIMIT);
}

#[test]
fn esc_and_k_close_the_box_and_every_other_key_is_swallowed() {
    let mut app = daemon_hosting(route_with(|_| {}));
    press(&mut app, KeyCode::Char('k'));
    for code in [
        KeyCode::Char('q'),
        KeyCode::Char('r'),
        KeyCode::Char('m'),
        KeyCode::Tab,
    ] {
        assert!(press(&mut app, code).is_empty(), "{code:?}");
        assert!(app.overlays.setup.is_some(), "{code:?}");
        assert!(!app.overlays.quitting, "{code:?}");
    }
    press(&mut app, KeyCode::Esc);
    assert!(app.overlays.setup.is_none());
    assert!(
        app.overlays.menu.is_none(),
        "Esc closed the box, not opened the menu"
    );
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('k'));
    assert!(app.overlays.setup.is_none());
}

#[test]
fn a_click_outside_the_box_closes_it() {
    let mut app = daemon_hosting(route_with(|_| {}));
    app.mouse = true;
    press(&mut app, KeyCode::Char('k'));
    let (_, hits, modal) = drawn(&app, 120, 40);
    let modal = modal.unwrap();
    let event = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    assert_eq!(
        mouse::resolve(&hits, event(0, 39), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
    assert_eq!(
        mouse::resolve(&hits, event(modal.x + 3, modal.y + 3), &app),
        None,
        "a click inside the box asks for nothing"
    );
}

#[test]
fn k_is_offered_on_a_dac_and_on_the_computers_side_only() {
    let dac = daemon_hosting(route_with(|_| {}));
    assert!(dac.setup_offered());
    let mut amp = app_showing_on("aegir2", Screen::Device);
    amp.host.linked = true;
    assert!(!amp.setup_offered());
    assert!(press(&mut amp, KeyCode::Char('k')).is_empty());
    assert!(amp.overlays.setup.is_none(), "no box on an amp's own side");
    amp.host.computer.why = Some("computer controls unavailable".into());
    amp.side = Side::Computer;
    assert!(amp.setup_offered());
    press(&mut amp, KeyCode::Char('k'));
    assert!(amp.overlays.setup.is_some());
}

#[test]
fn the_controls_border_names_the_key_where_it_is_on_offer() {
    let app = daemon_hosting(route_with(|_| {}));
    let (rows, _, _) = drawn(&app, 120, 40);
    assert!(
        rows.iter().any(|row| row.contains("refresh ─ k check")),
        "{:#?}",
        &rows[..6]
    );
    let mut amp = app_showing_on("aegir2", Screen::Device);
    amp.host.linked = true;
    let (rows, _, _) = drawn(&amp, 120, 40);
    assert!(!rows.iter().any(|row| row.contains("k check")));
}

#[test]
fn the_box_says_not_read_with_no_route_and_what_the_loop_found_without_a_daemon() {
    // No daemon: the box holds the loop's own read, or says it is on its way, or that it failed.
    let mut app = daemon_hosting(route_with(|_| {}));
    app.host.daemon = None;
    press(&mut app, KeyCode::Char('k'));
    let (rows, _, modal) = drawn(&app, 120, 40);
    let lines = inner(&rows, modal.unwrap());
    assert_eq!(lines[0], "Sink  not read");
    assert_eq!(lines[1], "? rates                  not read");
    assert_eq!(lines[8], "PipeWire config not read");
    assert_eq!(lines[9], "reading the PipeWire graph…");
    // The loop's read lands: the judge's words for it.
    let own = route_with(|_| {});
    app.overlays.setup.as_mut().unwrap().own = Some(Box::new(own));
    let (rows, _, modal) = drawn(&app, 120, 40);
    let lines = inner(&rows, modal.unwrap());
    assert_eq!(lines[0], "Sink  Gungnir 2 Analog Stereo");
    assert_eq!(
        lines.last().unwrap(),
        "PipeWire config lets it through untouched"
    );
    // A later read that found no graph shows every line as not read and the summary `config ?`
    // (unread is unknown, never the last route's clean marks), and says so.
    app.overlays.setup.as_mut().unwrap().failed = true;
    let (rows, _, modal) = drawn(&app, 120, 40);
    let lines = inner(&rows, modal.unwrap());
    assert_eq!(
        lines.last().unwrap(),
        "svolder could not read the PipeWire graph (pw-dump)"
    );
    assert_eq!(lines[0], "Sink  not read");
    assert_eq!(lines[8], "PipeWire config not read");
    assert!(
        !lines.iter().any(|line| line.contains('✓')),
        "no old marks: {lines:?}"
    );
    assert_eq!(app.setup_check().chip(), "config ?");
}

#[test]
fn an_older_daemon_is_named_in_the_box() {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    press(&mut app, KeyCode::Char('k'));
    let (rows, _, modal) = drawn(&app, 120, 40);
    let lines = inner(&rows, modal.unwrap());
    assert_eq!(
        lines.last().unwrap(),
        "PipeWire config not read (svolderd's version differs)"
    );
}

/// A row too long for the narrow pane scrolls with the setting on, and `k` still opens the box.
#[test]
fn with_scrolling_on_a_long_row_keeps_scrolling() {
    let mut app = daemon_hosting(route_with(two_fail));
    assert!(app.tui.scroll_long_lines);
    let (rows, _, _) = drawn(&app, 80, 40);
    let row = rows.iter().find(|row| row.contains(" Path ")).unwrap();
    assert!(row.contains("player \u{2192} computer"), "{row}");
    assert!(app.drawn.path_travel.get().is_some(), "the row scrolls");
    press(&mut app, KeyCode::Char('k'));
    assert!(app.overlays.setup.is_some());
}

/// A click anywhere on the Path row, the label and every line it wraps to included, opens the
/// config box as `k` does; the line above the row and the one below it do not.
#[test]
fn a_click_anywhere_on_the_path_row_opens_the_box() {
    let mut failing = daemon_hosting(route_with(two_fail));
    let mut clean = daemon_hosting(route_with(|_| {}));
    failing.mouse = true;
    clean.mouse = true;
    // The clean route at 120x45 is two lines: the hops, then the values.
    let cases = [
        (&failing, 80, 24, 0),
        (&failing, 100, 45, 0),
        (&failing, 120, 40, 0),
        (&failing, 200, 30, 0),
        (&clean, 120, 45, 1),
    ];
    for (app, width, height, lines) in cases {
        let (rows, hits, _) = drawn(app, width, height);
        let top = rows
            .iter()
            .position(|row| row.contains(" Path "))
            .unwrap_or_else(|| panic!("{width}x{height}: {rows:#?}"));
        let inner = rows[top].trim_start_matches('│');
        let first = u16::try_from(rows[top].chars().count() - inner.chars().count()).unwrap();
        // the row's lines: its own and the lines that carry on under its words, which start
        // in or past the words' column (a hop, or a piece after a comma)
        let pane = |row: &str| row.split('│').nth(1).unwrap_or("").to_string();
        let column = pane(&rows[top]).find(char::is_alphabetic).map_or(0, |at| {
            // the label, then the words after it
            let label = pane(&rows[top]);
            label[at..].find("  ").map_or(at, |gap| {
                at + gap + label[at + gap..].find(|c: char| c != ' ').unwrap_or(0)
            })
        });
        let mut bottom = top;
        while rows.get(bottom + 1).is_some_and(|row| {
            let words = pane(row);
            !words.trim().is_empty() && words.chars().take_while(|c| *c == ' ').count() >= column
        }) {
            bottom += 1;
        }
        let click = |column, row| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        for y in top..=bottom {
            let y = u16::try_from(y).unwrap();
            for x in [first + 2, first + 12, first + 30] {
                assert_eq!(
                    mouse::resolve(&hits, click(x, y), app),
                    Some(MouseAction::Key(KeyCode::Char('k'))),
                    "{width}x{height}: ({x}, {y})"
                );
            }
        }
        let above = u16::try_from(top - 1).unwrap();
        assert_ne!(
            hits.hit_at(first + 12, above),
            Some(&Hit::Path),
            "{width}x{height}: the row above"
        );
        let below = u16::try_from(bottom + 1).unwrap();
        assert_ne!(
            hits.hit_at(first + 12, below),
            Some(&Hit::Path),
            "{width}x{height}: the row below"
        );
        assert!(
            bottom - top >= lines,
            "{width}x{height}: the row's lines: {}",
            bottom - top
        );
    }
}
