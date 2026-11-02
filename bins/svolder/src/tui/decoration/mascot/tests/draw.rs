// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing the squirrel: where its box stands, what the painter writes, the room check, the
//! switches, the toast and the click.

use std::time::{Duration, Instant};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use svolder_ipc::ColorDepth;

use super::super::draw::{self, fits_home, home_box};
use super::super::tricks::{State, Trick};
use super::super::{BOX_H, BOX_W, Code, Place, Showing, View, brain::What};
use crate::tui::app::{App, Pane, Screen};
use crate::tui::mouse::{self, Hit, HitMap, MouseAction};
use crate::tui::render::{self, FrameId};
use crate::tui::screens;
use crate::tui::test_support::{app_showing, called_app, frame_with_theme, ms, theme_for};
use crate::tui::theme::Theme;

fn truecolour() -> Theme {
    theme_for("amber", ColorDepth::TrueColor)
}

/// A hidden squirrel visiting the Telemetry screen: a changed input sends the messenger.
fn visiting_app() -> App {
    let mut app = app_showing(Screen::Telemetry);
    app.mascot.capable = true;
    app.mascot.palette = "amber".into();
    let t0 = Instant::now();
    let mut view = View {
        input: Some(1),
        screen_is_home: false,
        ..View::bare()
    };
    app.mascot.observe(&view, t0);
    view.input = Some(2);
    app.mascot.observe(&view, t0 + ms(1));
    app
}

fn render_with(app: &App, width: u16, height: u16, theme: &Theme) -> (Buffer, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, app, theme, &mut hits);
        })
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), hits)
}

fn showing(app: &App) -> Showing {
    app.mascot.showing().expect("the squirrel shows")
}

/// The cell the painter should write at box cell `(col, row)`: the glyph, fg and bg the sheet's
/// two pixels make, with `under` for a see-through half; `None` for two see-through pixels.
fn expected(
    app: &App,
    theme: &Theme,
    col: usize,
    row: usize,
    under: Color,
) -> Option<(&'static str, Color, Color)> {
    let showing = showing(app);
    let pixels = showing.pose.pixels();
    let colour = |letter: char, y: usize| {
        theme.quantised(showing.paint.rgb(letter, col, y, showing.elapsed))
    };
    match (pixels[2 * row][col], pixels[2 * row + 1][col]) {
        (Some(t), Some(b)) => Some(("▀", colour(t, 2 * row), colour(b, 2 * row + 1))),
        (Some(t), None) => Some(("▀", colour(t, 2 * row), under)),
        (None, Some(b)) => Some(("▄", colour(b, 2 * row + 1), under)),
        (None, None) => None,
    }
}

fn cell(buffer: &Buffer, x: u16, y: u16) -> &ratatui::buffer::Cell {
    buffer.cell((x, y)).expect("a cell in the frame")
}

fn sprite_cells(buffer: &Buffer, area: Rect) -> usize {
    let mut n = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if matches!(cell(buffer, x, y).symbol(), "▀" | "▄")
                && matches!(cell(buffer, x, y).fg, Color::Rgb(..))
            {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn an_uncalled_squirrel_draws_nothing() {
    let theme = truecolour();
    let mut app = app_showing(Screen::Device);
    app.mascot.capable = true;
    let shown = frame_with_theme(&app, 80, 24, &theme);
    app.mascot.capable = false;
    let off = frame_with_theme(&app, 80, 24, &theme);
    assert_eq!(shown, off);
}

#[test]
fn a_called_squirrel_stands_at_home_at_80_by_24() {
    let theme = truecolour();
    let app = called_app();
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    assert_eq!(home_box(Rect::new(38, 2, 42, 18)), Rect::new(51, 14, 27, 6));
    let mut painted = 0;
    for row in 0..6 {
        for col in 0..BOX_W {
            let c = cell(&buffer, 51 + col as u16, 14 + row as u16);
            match expected(&app, &theme, col, row, c.bg) {
                Some(want) => {
                    painted += 1;
                    assert_eq!((c.symbol(), c.fg, c.bg), want, "({col},{row})");
                    assert!(
                        matches!(c.fg, Color::Rgb(..)),
                        "({col},{row}) is truecolour"
                    );
                }
                None => assert!(
                    !matches!(c.symbol(), "▀" | "▄"),
                    "({col},{row}) is see-through"
                ),
            }
        }
    }
    assert!(painted > 20, "the squirrel is there: {painted} cells");
    // The eye: a pupil over the eye white, composed from the sheet itself.
    let eye = cell(&buffer, 61, 17);
    assert_eq!(eye.symbol(), "▀");
    assert_eq!(eye.fg, Color::Rgb(0x12, 0x0d, 0x08), "the pupil");
    assert_eq!(eye.bg, Color::Rgb(0xf3, 0xee, 0xe2), "the eye white");
    // A box column with no sprite pixel in its bottom cell row keeps the border.
    let bare = (0..BOX_W)
        .find(|col| {
            let pixels = showing(&app).pose.pixels();
            pixels[BOX_H - 2][*col].is_none() && pixels[BOX_H - 1][*col].is_none()
        })
        .expect("a column with a bare foot row");
    assert_eq!(cell(&buffer, 51 + bare as u16, 19).symbol(), "─");
    for x in 0..80 {
        assert!(
            !matches!(cell(&buffer, x, 13).symbol(), "▀" | "▄"),
            "row 13"
        );
    }
    for y in 0..24 {
        assert!(
            !matches!(cell(&buffer, 78, y).symbol(), "▀" | "▄"),
            "column 78"
        );
    }
    let area = hits.area_of(Hit::Mascot).expect("the hit is recorded");
    assert!(Rect::new(51, 14, 27, 6).contains(area.as_position()));
    assert_eq!(sprite_cells(&buffer, Rect::new(0, 0, 80, 24)), painted);
}

#[test]
fn the_box_starts_at_91_27_at_120_by_40() {
    let theme = truecolour();
    let app = called_app();
    let tone = screens::device::frames_of(Rect::new(0, 0, 120, 40), &app)
        .into_iter()
        .find(|(id, _)| *id == FrameId::Pane(Pane::Tone))
        .expect("the Tone pane")
        .1;
    assert_eq!(home_box(tone), Rect::new(91, 27, 27, 6));
    let (buffer, hits) = render_with(&app, 120, 40, &theme);
    let area = hits.area_of(Hit::Mascot).expect("the hit is recorded");
    assert!(area.x >= 91 && area.y >= 27, "{area:?}");
    assert!(sprite_cells(&buffer, Rect::new(91, 27, 27, 6)) > 20);
}

/// The room check looks at where each line's text ends in the rows the box covers: lines ending
/// a blank column short of the box leave it room however many there are, a line reaching that
/// blank takes it, and a line above the box never does. The tier's padding moves the lines down.
#[test]
fn the_room_check_looks_at_the_columns_the_box_covers() {
    let pane = Rect::new(38, 2, 42, 18);
    let home = home_box(pane);
    let clear = vec![home.x - 1; usize::from(home.bottom() - 3)];
    assert!(fits_home(pane, 3, &clear), "lines short of the box");
    let mut touching = clear.clone();
    touching[usize::from(home.y - 3)] = home.x;
    assert!(!fits_home(pane, 3, &touching), "no blank before the box");
    let mut above = clear.clone();
    above[usize::from(home.y - 4)] = pane.right();
    assert!(fits_home(pane, 3, &above), "a whole line above the box");
    assert!(
        !fits_home(pane, 4, &above),
        "padded down into the box's first row"
    );
}

/// A called squirrel on `name`, at home with nothing to do, as [`called_app`] is on a Gungnir.
fn called_on(name: &str) -> App {
    let profile = svolder_profiles::Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("the profile is built in")
        .clone();
    let tui = svolder_ipc::TuiConfig::default();
    let mut app = App::new(profile, name.into(), &tui, Vec::new());
    app.mascot.capable = true;
    app.mascot.palette = "amber".into();
    let t0 = Instant::now();
    app.mascot.code(Code::Konami, t0);
    app.mascot.observe(&View::bare(), t0 + ms(10_000));
    app
}

/// On a device with no tone control the squirrel lives on the Controls pane, as wide as the
/// screen, beside rows that end well short of it: a Kraken with the Host group at 80×24 has
/// text in the rows the box covers, and none in its columns.
#[test]
fn the_squirrel_stands_beside_rows_that_end_short_of_it() {
    let theme = truecolour();
    let mut app = called_on("kraken");
    app.host.linked = true;
    let controls = screens::device::frames_of(Rect::new(0, 0, 80, 24), &app)
        .into_iter()
        .find(|(id, _)| *id == FrameId::Pane(Pane::Controls))
        .expect("the Controls pane")
        .1;
    let home = home_box(controls);
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    let text_beside = (home.y..home.bottom() - 1)
        .any(|y| (controls.x + 1..home.x - 1).any(|x| buffer[(x, y)].symbol() != " "));
    assert!(
        text_beside,
        "no row reaches the box's rows: the case is not a tight one"
    );
    assert!(
        sprite_cells(&buffer, home) > 20,
        "the squirrel is not at home"
    );
    assert!(hits.area_of(Hit::Mascot).is_some());
}

#[test]
fn a_pane_with_no_room_paints_no_sprite() {
    let theme = truecolour();
    let mut app = called_app();
    // A Tone pane too short for the box: a 24-row terminal is the smallest, so take the room
    // away with more bands.
    let eq = app
        .profile
        .eq
        .as_mut()
        .expect("the profile has a tone control");
    let band = eq.bands[0].clone();
    eq.bands.extend(std::iter::repeat_n(band, 3));
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    assert_eq!(sprite_cells(&buffer, Rect::new(0, 0, 80, 24)), 0);
    assert!(hits.area_of(Hit::Mascot).is_none());
}

#[test]
fn only_truecolour_and_256_colours_draw_and_256_uses_the_cube() {
    let app = called_app();
    let (buffer, _) = render_with(&app, 80, 24, &theme_for("amber", ColorDepth::Ansi256));
    let indexed = (0..BOX_W as u16)
        .filter(|col| matches!(cell(&buffer, 51 + col, 17).fg, Color::Indexed(_)))
        .count();
    assert!(indexed > 0, "the squirrel is drawn from the cube");

    for switch in 0..3 {
        let mut app = called_app();
        match switch {
            0 => app.mascot.capable = false, // 16 colours and monochrome
            1 => app.tui.effects.enabled = false,
            _ => app.tui.effects.mascot = false,
        }
        let (buffer, hits) = render_with(&app, 80, 24, &truecolour());
        assert_eq!(
            sprite_cells(&buffer, Rect::new(0, 0, 80, 24)),
            0,
            "{switch}"
        );
        assert!(hits.area_of(Hit::Mascot).is_none());
    }
}

#[test]
fn a_visit_floats_at_the_bottom_right_of_the_body() {
    let theme = truecolour();
    let app = visiting_app();
    assert_eq!(
        app.mascot.what(),
        Some((What::Trick(Trick::Messenger), Place::Float))
    );
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    let body = Rect::new(0, 2, 80, 22);
    let float = home_box(body);
    assert_eq!(float, Rect::new(51, 18, 27, 6));
    assert!(
        hits.area_of(Hit::Mascot).is_none(),
        "a visitor takes no click"
    );
    assert!(sprite_cells(&buffer, float) > 10);
}

/// The first and last rows of `buffer` that hold sprite cells, or none when it holds none.
fn sprite_rows(buffer: &Buffer) -> Option<(u16, u16)> {
    let rows: Vec<u16> = (0..buffer.area.height)
        .filter(|y| sprite_cells(buffer, Rect::new(0, *y, buffer.area.width, 1)) > 0)
        .collect();
    Some((*rows.first()?, *rows.last()?))
}

#[test]
fn a_toast_over_a_visitor_lifts_it_above_the_toast() {
    let theme = truecolour();
    let mut app = visiting_app();
    app.notify("Input changed to something long enough to reach the box");
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    let toast = hits.area_of(Hit::Toast).expect("the toast is drawn");
    let (first, last) = sprite_rows(&buffer).expect("the visitor still shows");
    assert_eq!(last, toast.y - 1, "its feet stand just above the toast");
    assert!(first >= 2, "it stays inside the body");
    assert!(hits.area_of(Hit::Mascot).is_none());
    assert!(!toast.is_empty());
}

#[test]
fn the_hour_toast_and_its_stretching_visitor_show_together() {
    let theme = truecolour();
    let mut app = app_showing(Screen::Telemetry);
    app.mascot.capable = true;
    app.mascot.palette = "amber".into();
    let t0 = Instant::now();
    app.observe_mascot(t0, (3, 3));
    app.observe_mascot(t0 + Duration::from_secs(3600), (3, 3));
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    let toast = hits.area_of(Hit::Toast).expect("the hour is said");
    let (_, last) = sprite_rows(&buffer).expect("the stretch shows");
    assert_eq!(last, toast.y - 1);
}

#[test]
fn a_toast_over_the_box_at_home_hides_the_squirrel() {
    let theme = truecolour();
    let mut app = called_app();
    // Three lines at 80 columns, which reach up into the box's bottom row.
    app.notify("a line of words that wraps onto several lines of the toast ".repeat(3));
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    assert!(hits.area_of(Hit::Toast).is_some());
    assert_eq!(sprite_cells(&buffer, Rect::new(0, 0, 80, 24)), 0);
    assert!(hits.area_of(Hit::Mascot).is_none());
}

/// Under a box that dims the screen the squirrel keeps its shading: each pixel is the grey of
/// its own luma, darkened as the dimmed text is, its see-through halves keep what is under them,
/// and the box's own dimming does not flatten it.
#[test]
fn under_a_box_that_dims_the_screen_the_squirrel_keeps_its_shading_in_greys() {
    let theme = truecolour();
    let mut app = called_app();
    app.overlays.menu = Some(0);
    // At 120 by 40 the menu stands clear of the squirrel's box.
    let (buffer, _) = render_with(&app, 120, 40, &theme);
    let mut bare = called_app();
    bare.overlays.menu = None;
    let (bare, _) = render_with(&bare, 120, 40, &theme);
    let greys = shaded_greys(&app, &theme, &buffer, &bare);
    assert!(greys.len() >= 3, "the shading went flat: {greys:?}");
}

/// The demo's join command under the menu's box is whole: the squirrel, whose cells reach that
/// row, is not put back over it.
#[test]
fn the_squirrel_is_not_put_back_over_the_menus_join_row() {
    let theme = truecolour();
    let mut app = called_app();
    app.overlays.menu = Some(0);
    app.session.join_hint = Some("svolder tui --join /tmp/sd-abcdef/sv.sock".into());
    let (buffer, _) = render_with(&app, 80, 24, &theme);
    let row: String = (0..80).map(|x| cell(&buffer, x, 17).symbol()).collect();
    assert!(
        row.contains("Join with: svolder tui --join /tmp/sd-abcdef/sv.sock"),
        "{row:?}"
    );
}

/// The squirrel's cells under a box, checked against the greys its pixels make; the distinct
/// foregrounds seen.
fn shaded_greys(
    app: &App,
    theme: &Theme,
    buffer: &Buffer,
    bare: &Buffer,
) -> std::collections::BTreeSet<String> {
    let showing = showing(app);
    let pixels = showing.pose.pixels();
    let grey = |letter: char, col: usize, y: usize| {
        theme.dimmed(showing.paint.rgb(letter, col, y, showing.elapsed))
    };
    let mut greys = std::collections::BTreeSet::new();
    let mut seen = 0;
    for row in 0..6 {
        for (col, (upper, lower)) in pixels[2 * row].iter().zip(&pixels[2 * row + 1]).enumerate() {
            let (x, y) = (91 + col as u16, 27 + row as u16);
            let c = cell(buffer, x, y);
            match (upper, lower) {
                (None, None) => continue,
                (Some(t), Some(b)) => assert_eq!(
                    (c.fg, c.bg),
                    (grey(*t, col, 2 * row), grey(*b, col, 2 * row + 1)),
                    "({x},{y})"
                ),
                (Some(t), None) => {
                    assert_eq!(c.fg, grey(*t, col, 2 * row), "({x},{y})");
                    assert_eq!(
                        c.bg,
                        cell(bare, x, y).bg,
                        "({x},{y}) keeps what is under it"
                    );
                }
                (None, Some(b)) => {
                    assert_eq!(c.fg, grey(*b, col, 2 * row + 1), "({x},{y})");
                    assert_eq!(
                        c.bg,
                        cell(bare, x, y).bg,
                        "({x},{y}) keeps what is under it"
                    );
                }
            }
            greys.insert(format!("{:?}", c.fg));
            seen += 1;
        }
    }
    assert!(seen > 20, "{seen} cells");
    greys
}

/// The squirrel is never over a box: the Settings box fills the body, so none of the squirrel is
/// put back, and every cell there is the box's own.
#[test]
fn the_squirrel_is_never_over_the_settings_box() {
    let theme = truecolour();
    let mut app = called_app();
    app.overlays.settings = Some(crate::tui::app::SettingsBox::default());
    let (buffer, _) = render_with(&app, 120, 40, &theme);
    let mut bare = app_showing(Screen::Device);
    bare.overlays.settings = Some(crate::tui::app::SettingsBox::default());
    let (bare, _) = render_with(&bare, 120, 40, &theme);
    let (alone, _) = render_with(&called_app(), 120, 40, &theme);
    let covered: Vec<(u16, u16)> = (0..120)
        .flat_map(|x| (0..40).map(move |y| (x, y)))
        .filter(|&(x, y)| matches!(cell(&alone, x, y).symbol(), "▀" | "▄"))
        .filter(|&(x, y)| cell(&bare, x, y).symbol() != cell(&alone, x, y).symbol())
        .collect();
    assert!(
        covered.len() > 20,
        "the squirrel is under the box: {}",
        covered.len()
    );
    for y in 0..40 {
        for x in 0..120 {
            assert_eq!(cell(&buffer, x, y), cell(&bare, x, y), "({x},{y})");
        }
    }
}

/// The box a modal draws in a frame of `width` by `height`.
fn box_of(
    app: &App,
    theme: &Theme,
    width: u16,
    height: u16,
    draw: fn(&mut ratatui::Frame, Rect, &App, &Theme) -> Rect,
) -> Rect {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal opens");
    let mut drawn = Rect::default();
    terminal
        .draw(|f| drawn = draw(f, f.area(), app, theme))
        .expect("the frame draws");
    drawn
}

/// With `open` applied to a called squirrel and to a bare app, every cell inside the modal's
/// box is the same; with `overlap` the squirrel also stands under part of that box (the quit
/// question is centred, and the squirrel's cells never reach it at a size that shows the squirrel).
fn assert_modal_clear_of_the_squirrel(
    open: fn(&mut App),
    draw: fn(&mut ratatui::Frame, Rect, &App, &Theme) -> Rect,
    (width, height): (u16, u16),
    overlap: bool,
) {
    let theme = truecolour();
    let mut called = called_app();
    open(&mut called);
    let mut bare = app_showing(Screen::Device);
    open(&mut bare);
    let area = box_of(&bare, &theme, width, height, draw);
    let (with, _) = render_with(&called, width, height, &theme);
    let (without, _) = render_with(&bare, width, height, &theme);
    let (alone, _) = render_with(&called_app(), width, height, &theme);
    let under = (area.x..area.right())
        .flat_map(|x| (area.y..area.bottom()).map(move |y| (x, y)))
        .filter(|&(x, y)| matches!(cell(&alone, x, y).symbol(), "▀" | "▄"))
        .count();
    assert!(
        !overlap || under > 5,
        "the squirrel is under the box {area:?}: {under} cells"
    );
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            assert_eq!(cell(&with, x, y), cell(&without, x, y), "({x},{y})");
        }
    }
}

/// The squirrel is never over the help box: every cell inside it is the box's own.
#[test]
fn the_squirrel_is_never_over_the_help_box() {
    assert_modal_clear_of_the_squirrel(
        |app| app.overlays.help_open = true,
        screens::help::draw,
        (80, 24),
        true,
    );
}

/// The squirrel is never over the quit question: every cell inside it is the box's own.
#[test]
fn the_squirrel_is_never_over_the_quit_question() {
    assert_modal_clear_of_the_squirrel(
        |app| {
            app.overlays.quitting = true;
            app.session.daemon = true;
        },
        screens::quit::draw,
        (80, 24),
        false,
    );
}

/// Under the help overlay the squirrel outside the box keeps its shading in greys.
#[test]
fn under_the_help_overlay_the_squirrel_keeps_its_shading_outside_the_box() {
    let theme = truecolour();
    let mut app = called_app();
    app.overlays.help_open = true;
    let (buffer, _) = render_with(&app, 120, 40, &theme);
    let mut greys = std::collections::BTreeSet::new();
    for y in 27..33 {
        for x in 91..91 + BOX_W as u16 {
            let c = cell(&buffer, x, y);
            if matches!(c.symbol(), "▀" | "▄") {
                greys.insert(format!("{:?}", c.fg));
            }
        }
    }
    assert!(greys.len() >= 3, "the shading went flat: {greys:?}");
}

/// At 256 colours the squirrel under the menu's box is greys of the palette's cube, in several
/// steps.
#[test]
fn at_256_colours_the_squirrel_under_a_box_is_indexed_greys_of_several_steps() {
    let theme = theme_for("amber", ColorDepth::Ansi256);
    let mut app = called_app();
    app.overlays.menu = Some(0);
    let (buffer, _) = render_with(&app, 120, 40, &theme);
    let mut greys = std::collections::BTreeSet::new();
    for y in 27..33 {
        for x in 91..91 + BOX_W as u16 {
            let c = cell(&buffer, x, y);
            if matches!(c.symbol(), "▀" | "▄") {
                assert!(
                    matches!(
                        c.fg,
                        Color::Indexed(232..=255 | 16 | 59 | 102 | 145 | 188 | 231)
                    ),
                    "({x},{y}) {:?}",
                    c.fg
                );
                greys.insert(format!("{:?}", c.fg));
            }
        }
    }
    assert!(greys.len() >= 3, "the shading went flat: {greys:?}");
}

#[test]
fn a_click_on_the_squirrel_makes_it_hop() {
    let theme = truecolour();
    let mut app = called_app();
    let (_, hits) = render_with(&app, 80, 24, &theme);
    let area = hits.area_of(Hit::Mascot).expect("the hit is recorded");
    // A cell with no sprite is not the squirrel: find one it painted.
    let (buffer, _) = render_with(&app, 80, 24, &theme);
    let (x, y) = (area.x..area.right())
        .flat_map(|x| (area.y..area.bottom()).map(move |y| (x, y)))
        .find(|(x, y)| matches!(cell(&buffer, *x, *y).symbol(), "▀" | "▄"))
        .expect("a painted cell");
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        mouse::resolve(&hits, click, &app),
        Some(MouseAction::Mascot)
    );
    app.mascot.click(Instant::now() + ms(20_000));
    assert_eq!(
        app.mascot.what(),
        Some((What::Trick(Trick::Hop), Place::Home))
    );
}

#[test]
fn a_box_past_the_frame_writes_only_inside_it_and_does_not_panic() {
    let theme = truecolour();
    let app = called_app();
    let showing = showing(&app);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a terminal opens");
    for (right, bottom) in [(90, 30), (10, 3), (-5, -2), (79, 23), (200, 100)] {
        terminal
            .draw(|f| {
                let painted = draw::paint(
                    f,
                    Rect::new(0, 0, 200, 100),
                    right,
                    bottom,
                    BOX_W,
                    &showing,
                    false,
                    &theme,
                );
                let painted: Vec<_> = painted.into_iter().map(|(at, _)| at).collect();
                if let Some(painted) = draw::bounds(&painted) {
                    assert_eq!(painted.intersection(f.area()), painted, "{right},{bottom}");
                }
            })
            .expect("the frame draws");
    }
}

#[test]
fn rest_is_the_pose_the_called_squirrel_holds_after_its_arrival() {
    let app = called_app();
    assert_eq!(
        app.mascot.what(),
        Some((What::State(State::Rest), Place::Home))
    );
}

#[test]
fn a_visitor_stays_under_the_devices_boxes() {
    use crossterm::event::{KeyCode, KeyEvent};

    let theme = truecolour();
    let mut app = visiting_app();
    app.show(Screen::Devices);
    let (buffer, _) = render_with(&app, 80, 24, &theme);
    assert!(
        sprite_cells(&buffer, Rect::new(0, 0, 80, 24)) > 10,
        "the visitor shows over the device list"
    );
    app.key(KeyEvent::from(KeyCode::Char('u')));
    assert!(app.devices.editor.is_some(), "the editor did not open");
    app.notify("a line of words that wraps onto several lines of the toast ".repeat(3));
    let (buffer, hits) = render_with(&app, 80, 24, &theme);
    let editor = hits.top_box().expect("the editor is a box");
    assert_eq!(sprite_cells(&buffer, editor), 0, "the editor is covered");
    assert!(hits.area_of(Hit::Mascot).is_none());
}

/// The Tone pane's band table, set a line apart from the rows above it, still leaves the squirrel
/// its corner at 80×24: in EQ mode with the Host group or without, and in Flat mode, whose note
/// sits right under the bands.
#[test]
fn the_squirrel_keeps_its_corner_under_the_spaced_band_table() {
    for (host, mode) in [(false, 2), (true, 2), (false, 1)] {
        let mut app = called_app();
        app.host.linked = host;
        crate::tui::test_support::hear_one(&mut app, "12.50", mode);
        assert!(
            draw::home_has_room(Rect::new(0, 0, 80, 24), &app, &truecolour()),
            "host {host}, mode {mode}"
        );
    }
}
