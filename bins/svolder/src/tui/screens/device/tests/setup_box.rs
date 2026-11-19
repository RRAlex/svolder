// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config chip on the Path row, `k` and the box it opens: the judge's words laid
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

/// The Path row's first line of a frame: what follows its label, up to the pane's border.
fn path_words(rows: &[String]) -> String {
    let row = rows
        .iter()
        .find(|row| row.contains(" Path "))
        .expect("a Path row");
    let pane = row.split('│').nth(1).unwrap_or("");
    pane.split_once("Path").unwrap().1.trim().to_string()
}

#[test]
fn the_chip_follows_the_verdict_on_the_path_row() {
    let app = daemon_hosting(route_with(|_| {}));
    let (rows, _, _) = drawn(&app, 200, 30);
    assert_eq!(
        path_words(&rows),
        "untouched · config ✓ · player 48 kHz S16LE, volume 100 %"
    );
    let app = daemon_hosting(route_with(two_fail));
    let (rows, _, _) = drawn(&app, 200, 30);
    assert!(
        path_words(&rows).contains("a filter inside the sink · config 2"),
        "{}",
        path_words(&rows)
    );
    // Config facts the judge could not read are a `?`. The unread words run past 200
    // columns, so the row is cut for the chip (scrolling off) to see it.
    let mut app = daemon_hosting(gungnir_route(|route| {
        route.setup = Some(SetupFacts::default())
    }));
    app.tui.scroll_long_lines = false;
    let (rows, _, _) = drawn(&app, 200, 30);
    assert!(
        path_words(&rows).contains(" · config ?"),
        "{}",
        path_words(&rows)
    );
}

/// The chip follows the sink, as on the web: a player playing to another sink while the default
/// sink is this DAC still wears the chip; a default sink that is no DAC of this screen wears none.
#[test]
fn the_chip_shows_while_the_default_sink_is_the_dac_whatever_plays() {
    let mut app = daemon_hosting(route_with(|_| {}));
    app.host.daemon.as_mut().unwrap().route = Some(Box::new(gungnir_route(|route| {
        route.setup = Some(clean_setup());
        route.player_match = PlayerMatch::Elsewhere {
            sink: "LG TV".into(),
        };
    })));
    assert_eq!(
        app.path_chip().map(|chip| chip.words).as_deref(),
        Some("config ✓")
    );
    app.host.daemon.as_mut().unwrap().route = Some(Box::new(gungnir_route(|route| {
        route.setup = Some(clean_setup());
        route.sink.as_mut().unwrap().usb_product = Some("Other".into());
    })));
    assert!(app.path_chip().is_none());
}

#[test]
fn an_older_daemons_route_has_no_chip() {
    let app = daemon_hosting(gungnir_route(|_| {}));
    let (rows, hits, _) = drawn(&app, 200, 30);
    assert!(!path_words(&rows).contains("config"));
    assert!(!rows.iter().any(|row| row.contains("config")));
    assert!(!(0..200).any(|x| (0..30).any(|y| hits.hit_at(x, y) == Some(&Hit::Config))));
}

#[test]
fn a_cut_row_keeps_the_chip_in_sight() {
    let app = daemon_hosting(route_with(|_| {}));
    for width in [80, 100, 120] {
        let (rows, _, _) = drawn(&app, width, 24);
        let words = path_words(&rows);
        assert!(
            words.starts_with("untouched · config ✓"),
            "{width}: {words}"
        );
    }
}

/// A verdict of two failing lines is wider than the Controls pane at 80 and 120 columns: the
/// verdict is cut (scrolling off) and the chip after it stays whole and clickable; with scrolling
/// on, the row scrolls as it always did and `k` still opens the box.
#[test]
fn a_long_verdict_is_cut_so_that_the_chip_stays_whole_with_scrolling_off() {
    let mut app = daemon_hosting(route_with(two_fail));
    app.tui.scroll_long_lines = false;
    for width in [80, 120] {
        let (rows, hits, _) = drawn(&app, width, 40);
        let y = rows
            .iter()
            .position(|row| row.contains("… · config 2"))
            .unwrap_or_else(|| panic!("{width}: {rows:#?}"));
        let at = rows[y].find("config 2").unwrap();
        let x = u16::try_from(rows[y][..at].chars().count()).unwrap();
        assert_eq!(
            hits.hit_at(x, u16::try_from(y).unwrap()),
            Some(&Hit::Config),
            "{width}"
        );
    }
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

#[test]
fn the_chip_is_a_target_that_opens_the_box_and_says_how_many_pass() {
    let mut app = daemon_hosting(route_with(two_fail));
    app.mouse = true;
    let (rows, hits, _) = drawn(&app, 200, 30);
    let y = rows
        .iter()
        .position(|row| row.contains("config 2"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    let chip = rows[y].find("config 2").unwrap();
    let chip_x = u16::try_from(rows[y][..chip].chars().count()).unwrap();
    let (x, y) = (chip_x + 2, u16::try_from(y).unwrap());
    assert_eq!(hits.hit_at(x, y), Some(&Hit::Config));
    assert_eq!(hits.hit_at(chip_x - 3, y), Some(&Hit::Path), "off the chip");
    let click = |column| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    assert_eq!(
        mouse::resolve(&hits, click(x), &app),
        Some(MouseAction::Key(KeyCode::Char('k')))
    );
    assert_eq!(mouse::resolve(&hits, click(chip_x - 3), &app), None);
    app.pointer.tooltip = Some(tooltip::Tooltip {
        hit: Hit::Config,
        at: (x, y),
        since: Instant::now(),
        shown: true,
    });
    assert_eq!(
        tooltip::standing(&app, &hits).expect("a tooltip"),
        ["config: 5 of 7 pass · k"]
    );
}

#[test]
fn the_chip_is_found_on_a_continuation_line_and_off_a_scrolled_or_cut_row() {
    use super::super::controls::chip_at;
    // A row that wrapped carries the chip at the start of its second line, after the `· `.
    let lines = vec![
        Line::from(vec![
            Span::raw("Path    "),
            Span::raw("dither on (triangular)"),
        ]),
        Line::from(vec![
            Span::raw("        "),
            Span::raw("· config 1"),
            Span::raw(" · player"),
        ]),
    ];
    let at = chip_at(&lines, "config 1").expect("the chip");
    assert_eq!((at.line, at.x, at.width), (1, 10, 8));
    // After the verdict on the first line it starts after the ` · ` that joins it.
    let lines = vec![Line::from(vec![
        Span::raw("Path    "),
        Span::raw("untouched"),
        Span::raw(" · config ✓"),
    ])];
    let at = chip_at(&lines, "config ✓").expect("the chip");
    assert_eq!((at.line, at.x, at.width), (0, 20, 8));
    // A chip cut at the right edge or scrolled partly out is not a target.
    let cut = vec![Line::from(vec![
        Span::raw("Path    "),
        Span::raw(" · config…"),
    ])];
    assert_eq!(chip_at(&cut, "config ✓"), None);
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

#[test]
fn with_scrolling_on_a_long_verdict_keeps_the_row_scrolling() {
    let mut app = daemon_hosting(route_with(two_fail));
    assert!(app.tui.scroll_long_lines);
    for width in [80, 120] {
        let (rows, _, _) = drawn(&app, width, 40);
        let row = rows.iter().find(|row| row.contains(" Path ")).unwrap();
        assert!(row.contains("dither on (triangula"), "{width}: {row}");
        assert!(!rows.iter().any(|row| row.contains("… · config 2")));
    }
    assert!(app.drawn.path_travel.get().is_some(), "the row scrolls");
    press(&mut app, KeyCode::Char('k'));
    assert!(app.overlays.setup.is_some());
}
