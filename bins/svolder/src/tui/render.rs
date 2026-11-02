// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which screen one frame shows, and the one message a terminal too small for any of them
//! gets instead.

use ratatui::Frame;
use ratatui::buffer::Cell;
use ratatui::layout::{Alignment, Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use svolder_proto::FunctionId;

use super::app::{App, Pane, Screen, ToneRow};
use super::boxes::toast;
use super::boxes::tooltip;
use super::decoration::mascot;
use super::layout_tier;
use super::mouse::{HitMap, Selection};
use super::screens;
use super::theme::{Role, Theme};
use super::widgets;

/// The narrowest terminal a screen is drawn in.
const MIN_WIDTH: u16 = 80;
/// The shortest terminal a screen is drawn in.
const MIN_HEIGHT: u16 = 24;

/// Which bordered block of a screen a frame is.
///
/// Every block a screen draws has a name of its own here, so a frame is known by what it is
/// rather than by where it is or by what it stands next to: the frame carrying the same name
/// on two drawn frames running is the one block, wherever the layout has since put it, and
/// [`FrameAreas::focused`] names the block the keys are in rather than pointing at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameId {
    /// The block round one pane of the device screen.
    Pane(Pane),
    /// The tone-control screen's response chart.
    Response,
    /// The tone-control screen's band table, which its keys move through.
    Bands,
    /// The one block a screen draws round everything it shows.
    Screen,
}

/// Where a frame's header drew svolder's own logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logo {
    /// The small wordmark, on a screen wide enough for it.
    Mark(Rect),
    /// The bold word that opens the header's text row on a narrower one.
    Word(Rect),
}

/// Where one frame put the pieces the animation layer works over.
///
/// Most of these areas are the frame's own, taken after it is drawn, so an effect covers exactly
/// what was painted rather than a layout worked out a second time elsewhere. `curve` and
/// `faults` are the two exceptions: [`frame_areas`] asks the tone screen and device screen's own
/// layout rules for them afresh, rather than reading either back from what the screen just drew,
/// so they can never disagree with it (at the cost of working them out twice).
#[derive(Debug, Clone, Default)]
pub struct FrameAreas {
    /// The header's text row and the accent rule under it.
    pub header: Rect,
    /// Everything below the header.
    pub body: Rect,
    /// Where the header drew svolder's logo; `None` on a frame with no header or under a modal
    /// box.
    pub logo: Option<Logo>,
    /// The cells the tone-control curve was drawn in, on a frame that drew one: a shaped curve
    /// (EQ mode, a band off 0 dB), a flat curve in EQ mode, or Flat mode's own curve, whatever
    /// the bands hold. `None` only in Loudness mode and on a device with no tone control.
    pub curve: Option<Rect>,
    /// Whether the curve above is muted (a flat curve in EQ mode or Flat mode), so
    /// [`effects::spectrum`](super::decoration::effects::spectrum) desaturates it and rests it red
    /// to violet rather than colouring the curve the bands are shaping. Meaningless while `curve`
    /// is `None`.
    pub curve_muted: bool,
    /// The line an active fault was written on, on a frame that drew one; `None` on a screen
    /// with no fault line and whenever the device reports nothing wrong.
    pub faults: Option<Rect>,
    /// The line the Host group's Spectrum row was drawn on, on a frame that put it inside the
    /// device screen's Controls pane; `None` on every other frame. What the capture of the
    /// host's sink runs for, and what the loop redraws fast for while the row moves.
    pub spectrum: Option<Rect>,
    /// Where each row landed, by the function it shows: the control rows of the device
    /// screen, and on the tone-control screen the mode and the loudness its header draws and
    /// the band rows, each band under all three of its own functions. A screen whose rows no
    /// function names reports none.
    pub rows: Vec<(FunctionId, Rect)>,
    /// The row the keys select in the list they are in, for what has no function to name its
    /// row by: the Telemetry rows, the Devices list, the menu's entries, the Settings box's
    /// rows, the preset list and the AutoEq results. `None` where no such list has the keys,
    /// under the help overlay and a text-entry line included, and on the Setup wizard.
    pub cursor: Option<Selection>,
    /// The bordered blocks the frame drew, each under its own name: what the arrival traces
    /// round when one of them appears. Their order says nothing, since each is known by name.
    /// A screen that draws none — the wizard, a terminal too small for any
    /// screen — reports none, and the overlays drawn over a screen are no frames of its.
    pub frames: Vec<(FrameId, Rect)>,
    /// Which of those frames the keys are going to, for the light that runs round it, and
    /// `None` wherever the keys are not in a frame at all: a pane the screen
    /// did not draw, a screen with no block round what it shows, and anything under an
    /// overlay.
    pub focused: Option<FrameId>,
    /// The modal box drawn over the screen, on a frame that drew one: the help overlay, the
    /// menu, the Settings box, the device editor, the question a removal asks, the preset list,
    /// a band's edit line in place of the band's row, or the question `q` asks.
    /// Everything outside it is dimmed, and stays dimmed whatever the animation layer paints
    /// after it, except the squirrel's greys ([`FrameAreas::sprite`], put back over the dimming)
    /// and the menu's join row ([`FrameAreas::join`], left undimmed).
    pub modal: Option<Rect>,
    /// The squirrel's cells under a modal box, in the greys it was painted in; the loop's own
    /// dimming puts them back over itself.
    pub sprite: Option<mascot::draw::Sprite>,
    /// The squirrel's cells, each with what the widgets drew there before it, on a frame with
    /// no box over it: the frame shows those while the pane the squirrel stands in is still
    /// drawing itself, so the squirrel comes in after it, all at once.
    pub behind: Vec<(Position, Cell)>,
    /// The row under the menu's box that holds the demo's join command: outside the box, yet
    /// kept at full strength, so neither dimming touches it nor the squirrel is put over it.
    pub join: Option<Rect>,
    /// The toast drawn over the corner, on a frame that drew one. Nothing the animation layer
    /// paints after the frame changes a cell of it.
    pub toast: Option<Rect>,
    /// The tooltip drawn after the toast, on a frame that drew one. Like the toast, nothing the
    /// animation layer paints after the frame changes a cell of it.
    pub tooltip: Option<Rect>,
    /// Whether the tooltip held has its target under its cell on this frame, drawn or not.
    pub tooltip_stands: bool,
}

/// Draw one frame of whatever the app is showing, recording where its rows landed.
///
/// The frame starts as the terminal's own background with the theme's primary text over it, so
/// every piece of a screen that names no role of its own still reads as text rather than as
/// whatever colour the terminal happens to write in.
///
/// The hit map describes one frame only, so it starts empty here and is filled by the screen
/// that draws. A terminal too small for any screen paints only its notice, and reports no
/// areas at all: there is nothing there worth animating.
///
/// A modal box (the help overlay, the menu, the Settings box and the quit question drawn over
/// whatever screen is showing, the
/// device editor and removal question drawn over the device list, the preset list drawn over
/// the tone-control screen or device screen, or a band's edit line drawn over its row on the
/// tone-control screen) dims everything around it and is the whole of the frame the animation
/// layer is given: the sweep a screen change earns plays over the overlay as it opens, and the
/// header, curve, rows, fault line, Spectrum row and frames under the box are reported as gone,
/// so nothing the screen beneath earned (the spectrum, a glow, a trail, the shimmer, a fault's
/// pulse) brightens a dimmed cell, and no capture runs for a row out of sight. Those frames are
/// covered rather than gone, so what reads them keeps its own count of which are on the
/// terminal.
///
/// A toast is drawn last of all, over everything including a modal box, and is neither dimmed
/// nor animated: [`FrameAreas::toast`] says where it is.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme, hits: &mut HitMap) -> FrameAreas {
    hits.clear();
    let area = frame.area();
    frame.buffer_mut().set_style(area, theme.style(Role::Text));
    if !fits(area) {
        too_small(frame, area, theme);
        return FrameAreas::default();
    }
    let (_, [header, body]) = layout_tier::split(area);
    let mut modal = None;
    let mut spectrum = None;
    let rows = match app.screen {
        Screen::Device => {
            let (rows, drawn) = screens::device::draw(frame, area, app, theme, hits);
            spectrum = drawn;
            rows
        }
        Screen::Tone => screens::tone::draw(frame, area, app, theme, hits),
        Screen::Telemetry => {
            screens::telemetry::draw(frame, area, app, theme, hits);
            Vec::new()
        }
        Screen::Devices => {
            modal = screens::devices::draw(frame, area, app, theme, hits);
            if let Some(drawn) = modal {
                hits.push_box(drawn);
                // The editor and the removal question are not a selection: text entry and a
                // question the list under them does not answer.
                hits.deselect();
            }
            Vec::new()
        }
        // The Settings box is a modal over a screen, drawn below; no app shows its name as a
        // screen, and the wizard draws its own steps from the wizard rather than from an app,
        // its loop being over before this one starts.
        Screen::Settings | Screen::Setup => Vec::new(),
    };
    // The theme's backgrounds go under the screen before any box is drawn over it.
    let frames = screen_frames(area, app);
    let panes: Vec<Rect> = frames.iter().map(|(_, rect)| *rect).collect();
    widgets::surfaces::paint_surfaces(frame.buffer_mut(), &panes, theme);
    // The squirrel stands over the panes and under every box and the toast. The device list
    // draws its editor and removal boxes together with the list itself, so the squirrel cannot
    // be laid between them: it stays out while one of them is open.
    let (sprite, behind) = match modal {
        None => mascot::draw::draw(frame, area, app, theme, hits),
        Some(_) => (None, Vec::new()),
    };
    let mut boxes: Vec<Rect> = Vec::new();
    let mut join = None;
    // The preset list is a box over the body of whichever screen opened it, the tone-control
    // screen or the device screen, below the header.
    if app.presets.preset_list.is_some() {
        hits.deselect();
        let drawn = screens::presets::draw(frame, body, app, theme, hits);
        stack_box(drawn, hits, &mut boxes, &mut modal);
    }
    // The device's own presets are a box over the body in the same way.
    if app.presets.device_list.is_some() {
        hits.deselect();
        let drawn = screens::device_presets::draw(frame, body, app, theme, hits);
        stack_box(drawn, hits, &mut boxes, &mut modal);
    }
    // A band's edit line takes the place of the band's row on the tone-control screen.
    if app.tone.edit_line.is_some() && app.screen == Screen::Tone {
        hits.deselect();
        let drawn = screens::tone::draw_edit_line(frame, area, app, theme);
        stack_box(drawn, hits, &mut boxes, &mut modal);
    }
    // The Settings box fills the body, over the screen it was opened on.
    if app.overlays.settings.is_some() {
        hits.deselect();
        let drawn = screens::settings::draw(frame, area, app, theme, hits);
        boxes.push(drawn);
        modal = Some(drawn);
    }
    if app.overlays.menu.is_some() {
        hits.deselect();
        let drawn = screens::menu::draw(frame, area, app, theme, hits);
        join = screens::menu::join_row(area, app);
        boxes.push(drawn);
        boxes.extend(join);
        modal = Some(drawn);
    }
    if app.overlays.help_open {
        hits.deselect();
        let drawn = screens::help::draw(frame, area, app, theme);
        boxes.push(drawn);
        modal = Some(drawn);
    }
    if app.overlays.quitting {
        hits.deselect();
        let drawn = screens::quit::draw(frame, area, app, theme);
        hits.push_box(drawn);
        boxes.push(drawn);
        modal = Some(drawn);
    }
    // The squirrel's greys go back over the flat dim each box laid around itself, except where a
    // box is drawn over them.
    let sprite = sprite.map(|sprite| sprite.outside(&boxes));
    if let Some(sprite) = &sprite {
        sprite.put_back(frame.buffer_mut());
    }
    let cursor = hits.selection();
    // The toast goes on last of all, over any box, at full strength, and its hit is the last
    // pushed.
    let toast = app
        .toast()
        .map(|toast| toast::draw(frame, area, toast, theme, hits));
    // The tooltip goes after the toast and clear of it, and only while its target is still
    // under its cell and no box is open.
    let standing = tooltip::standing(app, hits);
    let tooltip_stands = standing.is_some();
    let tooltip = match (app.pointer.tooltip.filter(|held| held.shown), standing) {
        (Some(held), Some(lines)) => tooltip::draw(frame, area, held.at, &lines, theme, toast),
        _ => None,
    };
    match modal {
        // Everything around a modal box is dimmed, so the box is the whole of the frame the
        // animation layer is given: nothing it plays brightens a dimmed cell.
        Some(body) => FrameAreas {
            header: Rect::default(),
            body,
            logo: None,
            curve: None,
            curve_muted: false,
            faults: None,
            spectrum: None,
            rows: Vec::new(),
            cursor,
            frames: Vec::new(),
            focused: None,
            modal: Some(body),
            sprite,
            behind: Vec::new(),
            join,
            toast,
            tooltip,
            tooltip_stands,
        },
        None => FrameAreas {
            spectrum,
            behind,
            cursor,
            toast,
            tooltip,
            tooltip_stands,
            ..frame_areas(area, header, body, app, rows, frames)
        },
    }
}

/// Record the box a screen drew, if it drew one: it takes its mouse hits, joins the drawn boxes and
/// becomes the modal one.
fn stack_box(
    drawn: Option<Rect>,
    hits: &mut HitMap,
    boxes: &mut Vec<Rect>,
    modal: &mut Option<Rect>,
) {
    if let Some(drawn) = drawn {
        hits.push_box(drawn);
    }
    boxes.extend(drawn);
    *modal = drawn.or(*modal);
}

/// The frames the screen `app` shows draws, each under its name.
fn screen_frames(area: Rect, app: &App) -> Vec<(FrameId, Rect)> {
    match app.screen {
        Screen::Device => screens::device::frames_of(area, app),
        Screen::Tone => screens::tone::frames_of(area, app),
        Screen::Telemetry => screens::telemetry::frames_of(area),
        Screen::Devices => screens::devices::frames_of(area, app),
        // No app shows the Settings name as a screen, and the wizard is over before this loop
        // starts: neither has a contour to trace.
        Screen::Settings | Screen::Setup => Vec::new(),
    }
}

/// The areas the frame just drawn put its pieces in: every screen keeps its header and the
/// rule under it on the first rows — the wordmark's own two among them on a wide screen — and
/// fills what is below them down to the last row, as wide as the tier lets it be.
fn frame_areas(
    area: Rect,
    header: Rect,
    body: Rect,
    app: &App,
    rows: Vec<(FunctionId, Rect)>,
    frames: Vec<(FrameId, Rect)>,
) -> FrameAreas {
    let curve = match app.screen {
        Screen::Tone => screens::tone::curve_of(area, app),
        _ => None,
    };
    let curve_muted = app.screen == Screen::Tone && screens::tone::curve_muted(app);
    let faults = match app.screen {
        Screen::Device => screens::device::fault_line(area, app),
        _ => None,
    };
    // The keys go to a pane of the device screen, to the band table of the tone-control
    // screen while the cursor is on it, and to the whole of any other screen, and to nothing at
    // all where what they are in has no frame round it — the tone screen's header among them,
    // which is one line with no border of its own to run a light round.
    let wanted = match app.screen {
        Screen::Device => Some(FrameId::Pane(app.pane)),
        Screen::Tone if app.tone.row == ToneRow::Band => Some(FrameId::Bands),
        Screen::Tone => None,
        _ => Some(FrameId::Screen),
    };
    let focused = wanted.filter(|wanted| frames.iter().any(|(id, _)| id == wanted));
    FrameAreas {
        header,
        body,
        logo: screens::device::logo_of(header),
        curve,
        curve_muted,
        faults,
        spectrum: None,
        rows,
        cursor: None,
        frames,
        focused,
        modal: None,
        sprite: None,
        behind: Vec::new(),
        join: None,
        toast: None,
        tooltip: None,
        tooltip_stands: false,
    }
}

/// Whether a frame this size has room for a screen at all.
///
/// The wizard asks this of its own frames, so a terminal too small is told the same thing
/// before there is a device as after it.
pub(super) fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Say what the terminal needs to be, and what it is, on the theme's ground.
pub(super) fn too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from("terminal too small"),
        Line::from(format!(
            "svolder needs {MIN_WIDTH}×{MIN_HEIGHT}, this one is {}×{}",
            area.width, area.height
        )),
    ];
    let height = u16::try_from(lines.len()).unwrap_or(1).min(area.height);
    let middle = Rect {
        x: area.x,
        y: area.y + (area.height - height) / 2,
        width: area.width,
        height,
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), middle);
    widgets::surfaces::paint_surfaces(frame.buffer_mut(), &[], theme);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use crate::tui::app::Pane;
    use crate::tui::mouse::Hit;
    use crate::tui::test_support::{app_showing, raw_reading};
    use crate::tui::theme::Theme;

    use super::*;

    /// The too-small notice is drawn on the theme's ground at true colour.
    #[test]
    fn the_too_small_notice_paints_the_ground() {
        use crate::tui::theme::Surface;
        use svolder_ipc::ColorDepth;

        let theme = crate::tui::test_support::theme_for("amber", ColorDepth::TrueColor);
        let ground = theme.surface(Surface::Ground).expect("a ground is painted");
        let app = app_showing(Screen::Device);
        let mut terminal = Terminal::new(TestBackend::new(60, 10)).expect("a test terminal");
        terminal
            .draw(|frame| {
                draw(frame, &app, &theme, &mut HitMap::default());
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer();
        assert!(
            (0..10)
                .flat_map(|y| (0..60).map(move |x| (x, y)))
                .all(|at| buffer.cell(at).is_some_and(|cell| cell.bg == ground))
        );
    }

    /// What one frame of `app` in an 80×24 terminal put where.
    fn areas_of(app: &App) -> FrameAreas {
        drawn(app).1
    }

    /// One frame of `app` in an 80×24 terminal: the cells it painted, and where it says it
    /// painted them.
    fn drawn(app: &App) -> (Buffer, FrameAreas) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut areas = FrameAreas::default();
        terminal
            .draw(|frame| areas = draw(frame, app, &Theme::new(false), &mut hits))
            .expect("the frame draws");
        (terminal.backend().buffer().clone(), areas)
    }

    /// What `buffer` says at `(x, y)`.
    fn symbol(buffer: &Buffer, x: u16, y: u16) -> &str {
        buffer
            .cell((x, y))
            .expect("a cell inside the buffer")
            .symbol()
    }

    /// Assert that `frame` is where `buffer` drew a bordered block: its four corners carry
    /// the corner glyphs, so a reported frame the trace would run round is the contour the
    /// screen actually drew.
    fn assert_bordered(buffer: &Buffer, frame: Rect, what: &str) {
        let (left, right) = (frame.x, frame.right() - 1);
        let (top, bottom) = (frame.y, frame.bottom() - 1);
        assert_eq!(symbol(buffer, left, top), "┌", "{what}: {frame:?}");
        assert_eq!(symbol(buffer, right, top), "┐", "{what}: {frame:?}");
        assert_eq!(symbol(buffer, left, bottom), "└", "{what}: {frame:?}");
        assert_eq!(symbol(buffer, right, bottom), "┘", "{what}: {frame:?}");
    }

    /// The device screen reports the three panes it draws, each the contour it drew, and says
    /// which of them the keys are in.
    #[test]
    fn the_device_screen_reports_its_three_panes_and_the_one_the_keys_are_in() {
        let mut app = app_showing(Screen::Device);
        let (buffer, areas) = drawn(&app);

        let ids: Vec<FrameId> = areas.frames.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            vec![
                FrameId::Pane(Pane::Controls),
                FrameId::Pane(Pane::Tone),
                FrameId::Pane(Pane::Status),
            ]
        );
        for (id, frame) in &areas.frames {
            assert_bordered(&buffer, *frame, &format!("{id:?}"));
        }
        let [controls, tone, status] = <[(FrameId, Rect); 3]>::try_from(areas.frames.clone())
            .expect("the three panes")
            .map(|(_, frame)| frame);
        assert_eq!(controls.x, 0, "the Controls pane opens the row");
        assert_eq!(tone.x, controls.right(), "the Tone pane is beside it");
        assert_eq!(
            tone.right(),
            80,
            "the Tone pane runs to the edge of the screen"
        );
        assert_eq!(status.y, controls.bottom(), "the Status pane is under them");

        assert_eq!(areas.focused, Some(FrameId::Pane(app.pane)));
        app.pane = Pane::Tone;
        assert_eq!(areas_of(&app).focused, Some(FrameId::Pane(Pane::Tone)));
    }

    /// Every screen but the device one reports the blocks it draws under their own names: the
    /// one block of the device list and of the telemetry screen, both of the tone-control
    /// screen with the keys in its band table, and none at all where there is no block — a
    /// terminal too small for any screen.
    #[test]
    fn the_other_screens_report_the_blocks_they_draw_each_under_its_own_name() {
        let app = app_showing(Screen::Devices);
        let (buffer, areas) = drawn(&app);
        let [(id, frame)] = <[(FrameId, Rect); 1]>::try_from(areas.frames.clone())
            .expect("the device list's one block");
        assert_eq!(id, FrameId::Screen);
        assert_bordered(&buffer, frame, "the device list");
        assert_eq!(areas.focused, Some(FrameId::Screen));

        let telemetry = areas_of(&app_showing(Screen::Telemetry));
        assert_eq!(telemetry.frames.len(), 1, "the telemetry block");
        assert_eq!(telemetry.focused, Some(FrameId::Screen));

        let (chart, tone) = drawn(&app_showing(Screen::Tone));
        // The order is not asserted: `FrameAreas::frames`' own doc says it carries no meaning,
        // since every frame is known by its name rather than by its position in the list.
        let ids: Vec<FrameId> = tone.frames.iter().map(|(id, _)| *id).collect();
        assert!(
            ids.contains(&FrameId::Response) && ids.contains(&FrameId::Bands) && ids.len() == 2,
            "the tone screen's two blocks are not each under a name of its own: {ids:?}"
        );
        for (id, frame) in &tone.frames {
            assert_bordered(&chart, *frame, &format!("{id:?}"));
        }
        assert_eq!(
            tone.focused,
            Some(FrameId::Bands),
            "the keys move through the band table"
        );
        let mut headed = app_showing(Screen::Tone);
        headed.tone.row = crate::tui::app::ToneRow::Mode;
        assert_eq!(
            areas_of(&headed).focused,
            None,
            "the header is one line with no contour to run a light round"
        );

        let mut terminal = Terminal::new(TestBackend::new(40, 10)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut small = FrameAreas::default();
        terminal
            .draw(|frame| small = draw(frame, &app, &Theme::new(false), &mut hits))
            .expect("the frame draws");
        assert!(
            small.frames.is_empty(),
            "a terminal too small draws nothing"
        );
        assert_eq!(small.focused, None);
    }

    #[test]
    fn an_open_help_overlay_leaves_the_animation_layer_nothing_of_the_screen_beneath() {
        let mut app = app_showing(Screen::Tone);
        // Band 1 lifted off 0 dB: a curve the spectrum colours.
        app.apply(
            crate::backend::Update::Status(raw_reading("12.1.3", 110)),
            std::time::Instant::now(),
        );
        let screen = areas_of(&app);
        assert!(
            screen.curve.is_some() && !screen.rows.is_empty(),
            "the tone screen drew no curve or no rows to animate in the first place"
        );

        app.overlays.help_open = true;
        let overlay = areas_of(&app);
        assert_eq!(overlay.curve, None, "the spectrum would play over the keys");
        assert!(overlay.rows.is_empty(), "a row would glow under the keys");
        assert_eq!(overlay.faults, None);
        assert!(
            overlay.frames.is_empty() && overlay.focused.is_none(),
            "a frame under the overlay would arrive, or carry the runner"
        );
        assert!(
            overlay.header.is_empty(),
            "the header is dimmed around the box, and nothing flashes over it"
        );
        assert_ne!(
            overlay.body, screen.body,
            "the sweep plays over the overlay, not the screen under it"
        );
    }

    /// The device editor and the question a removal asks are modal boxes as the help overlay
    /// is: the frame hands the animation layer the box and nothing of the list or the header
    /// under it.
    #[test]
    fn the_device_editor_and_the_removal_leave_the_animation_layer_only_their_box() {
        let mut app = app_showing(Screen::Devices);
        let list = areas_of(&app);
        assert!(!list.frames.is_empty() && !list.header.is_empty());
        for open in [true, false] {
            match open {
                true => crate::tui::boxes::editor::open(&mut app),
                false => {
                    app.devices.editor = None;
                    app.devices.confirm = Some("gungnir".into());
                }
            }
            let modal = areas_of(&app);
            let what = if open { "the editor" } else { "the removal" };
            assert!(modal.header.is_empty(), "{what}: the header was left in");
            assert!(
                modal.frames.is_empty() && modal.focused.is_none(),
                "{what}: the list's frame would arrive or carry the runner"
            );
            assert!(
                modal.body.width < list.body.width && list.body.contains(modal.body.as_position()),
                "{what}: the body is not the box over the list: {:?}",
                modal.body
            );
        }
    }

    /// One frame of `app` in an 80×24 terminal drawn with `theme`.
    fn drawn_with(app: &App, theme: &Theme) -> (Buffer, FrameAreas) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut areas = FrameAreas::default();
        terminal
            .draw(|frame| areas = draw(frame, app, theme, &mut hits))
            .expect("the frame draws");
        (terminal.backend().buffer().clone(), areas)
    }

    /// One frame of `app` in a terminal `width` by `height`: the cells it painted and the hits
    /// it pushed.
    fn drawn_at(app: &App, (width, height): (u16, u16)) -> (Buffer, HitMap, FrameAreas) {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut areas = FrameAreas::default();
        terminal
            .draw(|frame| areas = draw(frame, app, &Theme::new(false), &mut hits))
            .expect("the frame draws");
        (terminal.backend().buffer().clone(), hits, areas)
    }

    /// No screen draws a key row: the terminal's last row is the bottom border of the last
    /// pane, at 80×24 and in the wide and tall tiers, and the body runs down to it.
    #[test]
    fn the_last_row_of_every_screen_is_the_bottom_border_of_its_last_pane() {
        for screen in [
            Screen::Device,
            Screen::Tone,
            Screen::Telemetry,
            Screen::Devices,
        ] {
            for size in [
                (80, 24),
                (126, 30),
                (100, 62),
                (126, 62),
                (200, 60),
                (300, 60),
            ] {
                let app = app_showing(screen);
                let (buffer, hits, areas) = drawn_at(&app, size);
                let last = size.1 - 1;
                let row: Vec<&str> = (0..size.0).map(|x| symbol(&buffer, x, last)).collect();
                let left = row.iter().position(|cell| *cell != " ").expect("a border");
                let right = row.iter().rposition(|cell| *cell != " ").expect("a border");
                assert_eq!(row[left], "└", "{screen:?} at {size:?}: |{}|", row.concat());
                assert_eq!(
                    row[right],
                    "┘",
                    "{screen:?} at {size:?}: |{}|",
                    row.concat()
                );
                assert_eq!(areas.body.bottom(), size.1, "{screen:?} at {size:?}");
                assert!(
                    (0..size.0).all(|x| !matches!(hits.hit_at(x, last), Some(Hit::Footer(_)))),
                    "{screen:?} at {size:?} draws a key row"
                );
            }
        }
    }

    /// While a modal box is open every cell outside it is drawn in the theme's dim role —
    /// the grey at true colour, the dim attribute at 16 colours — with its glyph kept and
    /// its background cleared, and no cell inside the box is: for the help overlay, the device
    /// editor, the question a removal asks, the preset list, the menu and the Settings box alike.
    #[test]
    fn everything_around_a_modal_box_is_dimmed_and_the_box_is_not() {
        use ratatui::style::Modifier;
        use svolder_ipc::{ColorDepth, TuiConfig};

        let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
        let ansi16 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
        let grey = true_colour
            .style(Role::Dim)
            .fg
            .expect("a grey at true colour");
        let mut help = app_showing(Screen::Device);
        help.overlays.help_open = true;
        let mut editing = app_showing(Screen::Devices);
        crate::tui::boxes::editor::open(&mut editing);
        let mut removing = app_showing(Screen::Devices);
        removing.devices.confirm = Some("gungnir".into());
        let mut choosing = app_showing(Screen::Tone);
        crate::tui::boxes::presets::open(&mut choosing);
        let mut menu = app_showing(Screen::Device);
        menu.overlays.menu = Some(0);
        let mut settings = app_showing(Screen::Device);
        settings.overlays.settings = Some(crate::tui::app::SettingsBox::default());
        for (what, app) in [
            ("the overlay", &help),
            ("the editor", &editing),
            ("the removal", &removing),
            ("the preset list", &choosing),
            ("the menu", &menu),
            ("the Settings box", &settings),
        ] {
            for (depth, theme) in [("true colour", &true_colour), ("16 colours", &ansi16)] {
                let (plain, _) = drawn_with(
                    &{
                        let mut closed = app_showing(app.screen);
                        closed.selected = app.selected;
                        closed
                    },
                    theme,
                );
                let (buffer, areas) = drawn_with(app, theme);
                let boxed = areas.body;
                // The header's first letter and the last row's first border cell, each outside
                // the box where the box does not reach it.
                for (x, y) in [(1, 0), (1, 23)] {
                    if boxed.contains(ratatui::layout::Position::new(x, y)) {
                        continue;
                    }
                    let cell = buffer.cell((x, y)).expect("a cell outside the box");
                    assert_eq!(
                        cell.symbol(),
                        plain.cell((x, y)).expect("the same cell").symbol(),
                        "{what} at {depth}: the glyph at {x},{y} changed"
                    );
                    // Dimmed, a cell is neither bold nor reversed.
                    assert!(
                        !cell
                            .modifier
                            .intersects(Modifier::REVERSED | Modifier::BOLD),
                        "{what} at {depth}: {x},{y} keeps {:?}",
                        cell.modifier
                    );
                    match theme.depth() {
                        ColorDepth::TrueColor => {
                            assert_eq!(cell.fg, grey, "{what} at {depth}: {x},{y} is not grey");
                        }
                        _ => assert!(
                            cell.modifier.contains(Modifier::DIM),
                            "{what} at {depth}: {x},{y} is not dim"
                        ),
                    }
                }
                let title = buffer
                    .cell((boxed.x + 2, boxed.y))
                    .expect("the box's title");
                assert!(
                    !title.modifier.contains(Modifier::DIM) && title.fg != grey,
                    "{what} at {depth}: the box itself was dimmed"
                );
            }
        }
    }

    /// A theme on `palette` at `depth` with the theme background `on`.
    fn themed(palette: &str, depth: svolder_ipc::ColorDepth, on: bool) -> Theme {
        let config = svolder_ipc::TuiConfig {
            palette: palette.into(),
            theme_background: on,
            ..svolder_ipc::TuiConfig::default()
        };
        Theme::from_config(&config, false, depth)
    }

    /// The background of the cell at `(x, y)`.
    fn bg(buffer: &Buffer, x: u16, y: u16) -> ratatui::style::Color {
        buffer.cell((x, y)).expect("a cell inside the buffer").bg
    }

    /// Every cell of `area` has background `colour`.
    fn assert_all_bg(buffer: &Buffer, area: Rect, colour: ratatui::style::Color, what: &str) {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                assert_eq!(bg(buffer, x, y), colour, "{what}: cell {x},{y} of {area:?}");
            }
        }
    }

    /// The ground is under the header and the tab strip, and the pane grey is inside every pane
    /// and on its border, on every palette at true colour.
    #[test]
    fn the_ground_is_under_the_screen_and_the_pane_grey_inside_every_pane() {
        use ratatui::style::Color;
        let greys = [
            ("amber", (0x07, 0x06, 0x05), (0x0b, 0x0a, 0x08)),
            ("ice", (0x05, 0x06, 0x07), (0x08, 0x0b, 0x0d)),
            ("red", (0x0c, 0x0c, 0x0c), (0x14, 0x14, 0x14)),
            ("neon", (0x0a, 0x06, 0x12), (0x10, 0x0a, 0x1c)),
        ];
        for (name, ground, pane) in greys {
            let theme = themed(name, svolder_ipc::ColorDepth::TrueColor, true);
            for screen in [
                Screen::Device,
                Screen::Tone,
                Screen::Telemetry,
                Screen::Devices,
            ] {
                let (buffer, areas) = drawn_with(&app_showing(screen), &theme);
                assert_all_bg(
                    &buffer,
                    areas.header,
                    Color::Rgb(ground.0, ground.1, ground.2),
                    &format!("{name} {screen:?} header"),
                );
                for (id, frame) in &areas.frames {
                    assert_all_bg(
                        &buffer,
                        *frame,
                        Color::Rgb(pane.0, pane.1, pane.2),
                        &format!("{name} {screen:?} {id:?}"),
                    );
                }
                assert!(!areas.frames.is_empty(), "{screen:?} has no pane");
            }
        }
    }

    /// At 256 colours the greys are the nearest entries `quantise` gives; the paint leaves a
    /// cell that already has a background of its own alone.
    #[test]
    fn the_paint_goes_through_quantise_and_leaves_a_cell_with_a_background_alone() {
        use ratatui::style::Color;
        let theme = themed("amber", svolder_ipc::ColorDepth::Ansi256, true);
        let ground = theme
            .surface(crate::tui::theme::Surface::Ground)
            .expect("a ground at 256 colours");
        let pane = theme
            .surface(crate::tui::theme::Surface::Pane)
            .expect("a pane grey at 256 colours");
        let app = app_showing(Screen::Device);
        let (buffer, areas) = drawn_with(&app, &theme);
        assert_eq!(bg(&buffer, 1, 0), ground);
        let controls = areas.frames[0].1;
        assert_eq!(bg(&buffer, controls.x, controls.y), pane);

        let mut cells = Buffer::empty(Rect::new(0, 0, 4, 1));
        cells.cell_mut((1, 0)).expect("a cell").bg = Color::Blue;
        widgets::surfaces::paint_surfaces(&mut cells, &[Rect::new(0, 0, 2, 1)], &theme);
        assert_eq!(bg(&cells, 0, 0), pane, "inside a pane");
        assert_eq!(bg(&cells, 1, 0), Color::Blue, "the cell kept its own");
        assert_eq!(bg(&cells, 3, 0), ground, "outside every pane");
    }

    /// The selected row keeps its own inverse colours under the paint: still reversed, with the
    /// colours it has without the theme background.
    #[test]
    fn the_selected_row_keeps_its_inverse_video_under_the_paint() {
        use ratatui::style::Modifier;
        let theme = themed("amber", svolder_ipc::ColorDepth::TrueColor, true);
        let off = themed("amber", svolder_ipc::ColorDepth::TrueColor, false);
        let app = app_showing(Screen::Device);
        let (areas, _) = drawn_with(&app, &theme);
        let (bare, _) = drawn_with(&app, &off);
        let reversed: Vec<(u16, u16)> = (0..24)
            .flat_map(|y| (0..80).map(move |x| (x, y)))
            .filter(|at| {
                bare.cell(*at)
                    .is_some_and(|c| c.modifier.contains(Modifier::REVERSED))
            })
            .collect();
        assert!(!reversed.is_empty(), "no selected row is drawn");
        for at in reversed {
            let (was, now) = (bare.cell(at).expect("cell"), areas.cell(at).expect("cell"));
            assert!(
                now.modifier.contains(Modifier::REVERSED),
                "{at:?} lost the inverse"
            );
            assert_eq!(now.fg, was.fg, "{at:?} changed its colours");
        }
    }

    /// Every box is painted raised, border and inside: the menu, help, the quit question, the
    /// preset list, the device editor, the removal question, a band's edit line and a toast. The
    /// Settings box alone takes the pane grey, so the palette it picks reads in it as the
    /// screens draw it.
    #[test]
    fn every_box_and_toast_is_painted_raised_and_the_cells_around_it_keep_their_greys() {
        let theme = themed("amber", svolder_ipc::ColorDepth::TrueColor, true);
        let raised = theme
            .surface(crate::tui::theme::Surface::Raised)
            .expect("a raised grey");
        let pane = theme
            .surface(crate::tui::theme::Surface::Pane)
            .expect("a pane grey");
        let ground = theme.surface(crate::tui::theme::Surface::Ground);
        let mut help = app_showing(Screen::Device);
        help.overlays.help_open = true;
        let mut quitting = app_showing(Screen::Device);
        quitting.overlays.quitting = true;
        let mut editing = app_showing(Screen::Devices);
        crate::tui::boxes::editor::open(&mut editing);
        let mut removing = app_showing(Screen::Devices);
        removing.devices.confirm = Some("gungnir".into());
        let mut choosing = app_showing(Screen::Tone);
        crate::tui::boxes::presets::open(&mut choosing);
        let mut menu = app_showing(Screen::Device);
        menu.overlays.menu = Some(0);
        let mut settings = app_showing(Screen::Device);
        settings.overlays.settings = Some(crate::tui::app::SettingsBox::default());
        let mut line = app_showing(Screen::Tone);
        line.apply(
            crate::backend::Update::Status(raw_reading("12.1.3", 110)),
            std::time::Instant::now(),
        );
        crate::tui::boxes::edit_line::open(&mut line);
        assert!(line.tone.edit_line.is_some(), "the edit line did not open");
        for (what, app, grey) in [
            ("the overlay", &help, raised),
            ("the quit question", &quitting, raised),
            ("the editor", &editing, raised),
            ("the removal", &removing, raised),
            ("the preset list", &choosing, raised),
            ("the menu", &menu, raised),
            ("the Settings box", &settings, pane),
            ("the edit line", &line, raised),
        ] {
            let (buffer, areas) = drawn_with(app, &theme);
            let modal = areas.modal.unwrap_or_else(|| panic!("{what} is no modal"));
            assert_all_bg(&buffer, modal, grey, what);
            // The dimmed header keeps the ground.
            if let Some(ground) = ground {
                assert_eq!(bg(&buffer, 1, 0), ground, "{what}: the header's ground");
            }
        }
        let mut toasting = app_showing(Screen::Device);
        toasting.notify("Saved");
        let (buffer, areas) = drawn_with(&toasting, &theme);
        assert_all_bg(&buffer, areas.toast.expect("a toast"), raised, "the toast");
        // A toast over a box is raised as well, and the box under it is as it is without the toast.
        settings.notify("Saved");
        let (buffer, areas) = drawn_with(&settings, &theme);
        assert_all_bg(
            &buffer,
            areas.toast.expect("a toast"),
            raised,
            "the toast over a box",
        );
    }

    /// Cells under a dimmed screen keep the pane grey of the pane they are in.
    #[test]
    fn the_dimmed_cells_around_a_box_keep_the_pane_grey() {
        let theme = themed("amber", svolder_ipc::ColorDepth::TrueColor, true);
        let pane = theme
            .surface(crate::tui::theme::Surface::Pane)
            .expect("a pane grey");
        let mut help = app_showing(Screen::Device);
        let (_, open) = drawn_with(&help, &theme);
        let controls = open.frames[0].1;
        help.overlays.quitting = true;
        let (buffer, areas) = drawn_with(&help, &theme);
        let modal = areas.modal.expect("the question");
        let outside = (controls.x..controls.right())
            .flat_map(|x| (controls.y..controls.bottom()).map(move |y| (x, y)))
            .find(|(x, y)| !modal.contains(ratatui::layout::Position::new(*x, *y)))
            .expect("a Controls cell outside the box");
        assert_eq!(bg(&buffer, outside.0, outside.1), pane);
    }

    /// With the switch off, at 16 colours and in monochrome, no cell has a background: boxes and
    /// toasts included.
    #[test]
    fn nothing_is_painted_with_the_switch_off_at_16_colours_or_in_monochrome() {
        use ratatui::style::Color;
        let off = themed("amber", svolder_ipc::ColorDepth::TrueColor, false);
        let sixteen = themed("amber", svolder_ipc::ColorDepth::Ansi16, true);
        let config = svolder_ipc::TuiConfig::default();
        let mono = Theme::from_config(&config, true, svolder_ipc::ColorDepth::TrueColor);
        for (what, theme) in [
            ("off", &off),
            ("16 colours", &sixteen),
            ("monochrome", &mono),
        ] {
            let mut app = app_showing(Screen::Device);
            app.overlays.settings = Some(crate::tui::app::SettingsBox::default());
            app.notify("Saved");
            let (buffer, _) = drawn_with(&app, theme);
            let area = buffer.area;
            assert_all_bg(&buffer, area, Color::Reset, what);
            let (buffer, _) = drawn_with(&app_showing(Screen::Tone), theme);
            assert_all_bg(&buffer, area, Color::Reset, what);
        }
    }
}
