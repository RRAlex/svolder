// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen: the control rows, the tone-control summary and the status frame.
//!
//! The control panes take whatever the header and the Status pane leave, so a
//! taller terminal shows more rows; a device with no tone control gives its Controls pane the
//! whole width. The focused pane's title is the one drawn in the selected role.
//!
//! - [`controls`]: the Controls pane: a row per control, the meters, and the Host group.
//! - [`header`]: the header every screen draws, over the rule that is the tab strip.
//! - [`status`]: the Status pane: readouts, firmware, faults, advice and the host's audio.
//! - [`tone_pane`]: the Tone pane: the mode, the loudness strength and a row per band.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use svolder_proto::FunctionId;

use crate::tui::app::{App, Holding, Pane, Screen};
use crate::tui::keys::Scene;
use crate::tui::layout_tier::{self, Tier};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render::FrameId;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::actions::Action;

mod controls;
mod header;
mod status;
mod tone_pane;

pub(crate) use header::{Rule, header_height, logo_of};
pub(super) use header::{
    draw_header, header_built, header_text, host_eq_words, mark_rect, marked, mouse_span,
    push_marks,
};
pub(crate) use status::{has_fault_controls, now_playing_room};
pub(super) use tone_pane::{
    BANDS_APPLY_IN_EQ, ModeHalves, band_dim, band_ids, band_values, bands_apply, mode_line,
};

#[cfg(test)]
use controls::path_row;
use controls::{Controls, controls_pane, height, room};
use status::{
    device_drawn, faults_fit, level_from_drawn, link_drawn, now_playing_drawn, passphrase_drawn,
    status_lines,
};
use tone_pane::{LOUDNESS_BAR_AT, TonePane, tone_lines};

/// Columns the Controls pane takes when a Tone pane sits beside it.
const CONTROLS_WIDTH: u16 = 38;
/// Columns it takes there in the wide tier.
const CONTROLS_WIDTH_WIDE: u16 = 50;
/// Lines the Status pane takes: two of its own inside a frame. The tall tier's blank top row
/// comes on top of these, added where the pane is laid out.
const STATUS_HEIGHT: u16 = 4;

/// The interior line the Tone pane draws the mode row on.
const MODE_ROW: u16 = 0;
/// The interior line it draws the loudness row on.
const LOUDNESS_ROW: u16 = 1;
/// How dark a pane's breathing contour is drawn with the meters at their quietest, as a
/// position along the accent's own shading.
const BREATH_FLOOR: f64 = 0.35;
/// How much of that shading the level carries the contour up: the rest of the way to the
/// accent itself.
const BREATH_SPAN: f64 = 0.65;
/// How far a beat of the daemon's frames lifts the level the contour is shaded by as it lands,
/// fading with the pulse ([`App::pulse`]): enough to be seen, little enough to stay a pulse.
const BEAT_LIFT: f64 = 0.3;
/// The function address the firmware build in the header comes from.
const FIRMWARE: FunctionId = FunctionId::three(210, 2, 1);

/// Whether `id` is in `FIRMWARE`'s own family (`210.2.x`, any third component): the firmware
/// query itself, and every build or version reading that family covers.
pub(super) fn is_firmware_family(id: FunctionId) -> bool {
    id.first() == FIRMWARE.first() && id.second() == FIRMWARE.second()
}

/// The words in the Controls pane's top border.
const CONTROLS_ACTIONS: [Action; 3] = [
    Action {
        code: KeyCode::Char('m'),
        word: "mute",
    },
    Action {
        code: KeyCode::Char('0'),
        word: "0 centre",
    },
    Action {
        code: KeyCode::Char('r'),
        word: "refresh",
    },
];

/// The word in the border of the panel that stands in for the panes while no device is open.
const HOLDING_ACTIONS: [Action; 1] = [Action {
    code: KeyCode::Char('r'),
    word: "refresh",
}];

/// The words in the Tone pane's top border.
const TONE_ACTIONS: [Action; 2] = [
    Action {
        code: KeyCode::Char('f'),
        word: "flat",
    },
    Action {
        code: KeyCode::Char('p'),
        word: "presets",
    },
];

/// The word in the Status pane's top border: `a` always does something.
const STATUS_ACTIONS: [Action; 1] = [Action {
    code: KeyCode::Char('a'),
    word: "advice",
}];

/// One bordered pane of the screen.
struct PaneSpec<'a> {
    /// Where it is drawn.
    area: Rect,
    /// The name in its top border.
    title: &'a str,
    /// What it draws inside.
    lines: Vec<Line<'static>>,
    /// Which pane it is, for whether its title draws in the selected role.
    focus: Pane,
    /// What a click on its title asks for: the Tone pane's own title leads to the full
    /// tone-control screen, the Controls title focuses its pane, and the Status title, on a
    /// pane that is never focused, asks for nothing.
    title_hit: Option<Hit>,
    /// The words in its top border, right-aligned.
    actions: &'a [Action],
}

/// Draw the device screen into `area`, and answer with where each control row landed, and
/// where the Spectrum row did on a frame that drew it inside the Controls pane — a pane with
/// more rows than lines leaves it out of sight, and then it is not on the screen.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> (Vec<(FunctionId, Rect)>, Option<Rect>) {
    let (tier, [header, panes, status]) = rows_of(area, app);

    let (line, marks) = header_built(app, theme, header);
    draw_header(
        frame,
        header,
        theme,
        line,
        &Rule::Tabs {
            current: Screen::Device,
            has_eq: app.profile.eq.is_some(),
        },
        hits,
    );
    push_marks(hits, header, marks);

    if let Some(holding) = &app.session.holding {
        let (tier, [_, body]) = layout_tier::split(area);
        draw_holding(frame, body, holding, app, theme, tier, hits);
        return (Vec::new(), None);
    }

    let (controls_area, tone_area) = panes_of(panes, app.profile.eq.is_some(), tier);
    let controls = controls_pane(
        app,
        theme,
        room(controls_area, tier),
        tier,
        height(controls_area, tier),
    );
    let tone = match (app.profile.eq.as_ref(), tone_area) {
        (Some(eq), Some(area)) => {
            let max_width = usize::from(tier.block().inner(area).width);
            Some(tone_lines(app, eq, theme, max_width, controls.bar))
        }
        _ => None,
    };
    let (mut rows, spectrum) =
        draw_controls(frame, controls_area, controls, app, theme, tier, hits);
    if let (Some(area), Some(tone)) = (tone_area, tone) {
        rows.extend(draw_tone(frame, area, tone, app, theme, tier, hits));
    }
    draw_status(frame, status, app, theme, tier, hits);
    (rows, spectrum)
}

/// Draw the Controls pane into `area` and push the hits of its rows, the Host group's rows and
/// the bars and links beside them. Answers with where each control row landed, and where the
/// Spectrum row did when it is in sight.
fn draw_controls(
    frame: &mut Frame,
    area: Rect,
    controls: Controls,
    app: &App,
    theme: &Theme,
    tier: Tier,
    hits: &mut HitMap,
) -> (Vec<(FunctionId, Rect)>, Option<Rect>) {
    let inner = pane(
        frame,
        PaneSpec {
            area,
            title: "Controls",
            lines: controls.lines,
            focus: Pane::Controls,
            title_hit: Some(Hit::PaneHeader(Pane::Controls)),
            actions: &CONTROLS_ACTIONS,
        },
        app,
        theme,
        tier,
        hits,
    );
    let mut rows = Vec::with_capacity(controls.rows.len());
    for (index, (id, top, height)) in controls.rows.iter().enumerate() {
        let row = Rect {
            x: inner.x,
            y: inner.y + top,
            width: inner.width,
            height: *height,
        }
        .intersection(inner);
        hits.push(row, Hit::Row(Pane::Controls, index));
        rows.push((*id, row));
    }
    // The Host group's rows come after the device's own, and a click focuses them under the
    // index the keys reach them by.
    for (offset, top) in controls.host.iter().enumerate() {
        let row = pane_row(inner, *top);
        hits.push(row, Hit::Row(Pane::Controls, controls.rows.len() + offset));
    }
    let spectrum = controls
        .spectrum
        .map(|top| pane_row(inner, top))
        .filter(|row| !row.is_empty());
    for (top, x, width, hit) in &controls.extras {
        let extra = Rect {
            x: inner.x + x,
            y: inner.y + top,
            width: *width,
            height: 1,
        };
        match hit {
            Hit::LevelBar(_) => hits.push_bar(extra.intersection(inner), extra, *hit),
            _ => hits.push(extra.intersection(inner), *hit),
        }
    }
    (rows, spectrum)
}

/// Draw the Tone pane into `area` and push the hits of its rows. Answers with the function each
/// row shows and where it landed.
fn draw_tone(
    frame: &mut Frame,
    area: Rect,
    tone: TonePane,
    app: &App,
    theme: &Theme,
    tier: Tier,
    hits: &mut HitMap,
) -> Vec<(FunctionId, Rect)> {
    let mut rows = Vec::new();
    let Some(eq) = app.profile.eq.as_ref() else {
        return rows;
    };
    let inner = pane(
        frame,
        PaneSpec {
            area,
            title: "Tone control",
            lines: tone.lines,
            focus: Pane::Tone,
            title_hit: Some(Hit::PaneHeader(Pane::Tone)),
            actions: &TONE_ACTIONS,
        },
        app,
        theme,
        tier,
        hits,
    );
    // Every row of this pane is one the cursor reaches, so each is reported under the
    // function it shows, the way the tone-control screen reports its bands. A click on the
    // Mode row toggles the flat mode anywhere but its own `‹ ›`, which still steps the
    // select; a click anywhere on the Loudness row toggles the loudness mode,
    // and the wheel over it steps the strength.
    let mode_row = pane_row(inner, MODE_ROW);
    hits.push(mode_row, Hit::Mode);
    let arrow = |at: usize| Rect {
        x: inner.x + u16::try_from(at).unwrap_or(0),
        y: inner.y + MODE_ROW,
        width: 1,
        height: 1,
    };
    hits.push(arrow(tone.mode_arrows.0).intersection(inner), Hit::ModePrev);
    hits.push(arrow(tone.mode_arrows.1).intersection(inner), Hit::ModeNext);
    if app.on_computer() {
        rows.push((eq.mode_id(), mode_row));
        for (point, top) in tone.bands.iter().enumerate() {
            hits.push(pane_row(inner, *top), Hit::Point(point));
        }
        return rows;
    }
    let loudness_row = pane_row(inner, LOUDNESS_ROW);
    hits.push(loudness_row, Hit::Loudness);
    // The bar inside the row sets the strength under a press and a drag.
    if app.raw_of(eq.loudness_id()).is_some() {
        let bar = Rect {
            x: inner.x + u16::try_from(LOUDNESS_BAR_AT).unwrap_or(0),
            width: u16::try_from(tone.loudness_bar).unwrap_or(0),
            ..loudness_row
        };
        hits.push_bar(bar.intersection(loudness_row), bar, Hit::LoudnessBar);
    }
    rows.push((eq.mode_id(), mode_row));
    rows.push((eq.loudness_id(), loudness_row));
    for (band, top) in tone.bands.iter().enumerate() {
        let row = pane_row(inner, *top);
        hits.push(row, Hit::Band(band));
        for (field, (start, width)) in tone.fields[band].iter().enumerate() {
            let cells = Rect {
                x: row
                    .x
                    .saturating_add(u16::try_from(*start).unwrap_or(u16::MAX)),
                width: u16::try_from(*width).unwrap_or(0),
                ..row
            }
            .intersection(row);
            hits.push(cells, Hit::BandField(band, field));
        }
        rows.extend(band_ids(eq, band).map(|id| (id, row)));
    }
    rows
}

/// Draw the Status pane into `area`.
fn draw_status(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    tier: Tier,
    hits: &mut HitMap,
) {
    let status_width = usize::from(tier.block().inner(area).width).saturating_sub(tier.trail());
    pane(
        frame,
        PaneSpec {
            area,
            title: "Status",
            lines: status_lines(app, theme, status_width, tier),
            focus: Pane::Status,
            title_hit: None,
            actions: &STATUS_ACTIONS,
        },
        app,
        theme,
        tier,
        hits,
    );
}

/// What the panel says while the daemon has not found the device yet: its name, how long
/// svolderd has been looking, and what to check.
pub(crate) fn waiting_lines(name: &str, since: &str) -> [String; 3] {
    [
        format!("{name} not found yet"),
        format!("svolderd keeps looking (since {since})."),
        "Is it on and in range?".to_string(),
    ]
}

/// What the panel says with no device open: where to pick one.
pub(crate) const NOTHING_OPEN: [&str; 2] = [
    "No device is open",
    "Press d for the Devices screen: Enter opens a device, n adds one.",
];

/// Draw the panel that stands in for the Controls and Status panes while no device is open: one
/// frame over the whole body, with the words [`waiting_lines`] or [`NOTHING_OPEN`] give, and the
/// refresh action the Controls pane has.
fn draw_holding(
    frame: &mut Frame,
    body: Rect,
    holding: &Holding,
    app: &App,
    theme: &Theme,
    tier: Tier,
    hits: &mut HitMap,
) {
    let (title, words) = match holding {
        Holding::Waiting { since } => (
            " Waiting ",
            waiting_lines(
                &app.session.device_name,
                &svolder_ipc::clock::local_hhmm(*since),
            )
            .to_vec(),
        ),
        Holding::Nothing { .. } => (" No device ", NOTHING_OPEN.map(str::to_string).to_vec()),
    };
    let (actions, word_hits) = widgets::actions::actions_title(
        &HOLDING_ACTIONS,
        None,
        widgets::actions::action_room(body.width, cells(title)),
        Screen::Device,
        &Scene::of(app),
        theme,
    );
    let block = tier
        .block()
        .border_style(contour(app, theme, true, border_style(app, theme)))
        .title(Span::styled(title, theme.style(Role::Selected)))
        .title(actions.clone().right_aligned());
    let inner = block.inner(body);
    let room = usize::from(inner.width).saturating_sub(1);
    let lines: Vec<Line<'static>> = words
        .iter()
        .flat_map(|text| widgets::text::wrap(text, room))
        .map(|text| Line::from(format!(" {text}")))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), body);
    hits.push(
        Rect {
            height: 1.min(body.height),
            ..body
        },
        Hit::PaneHeader(Pane::Controls),
    );
    widgets::actions::push_action_hits(hits, body, &actions, &word_hits);
}

/// The tiers `area` falls in, and the three bands this screen is laid out in over it: the
/// header, the panes between, and the Status frame — a row under the panes in the tall tier,
/// and a line taller for each of the `Link`, `Level from`, `Now playing` and `Passphrase` rows
/// it draws — down to the terminal's last row.
fn rows_of(area: Rect, app: &App) -> (Tier, [Rect; 3]) {
    let (tier, [header, body]) = layout_tier::split(area);
    let extra_lines = u16::from(device_drawn(app))
        + u16::from(link_drawn(app, tier))
        + u16::from(level_from_drawn(app))
        + u16::from(now_playing_drawn(app))
        + u16::from(passphrase_drawn(app));
    let [panes, _, status] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(tier.row_gap()),
        Constraint::Length(STATUS_HEIGHT + extra_lines + tier.top()),
    ])
    .areas(body);
    (tier, [header, panes, status])
}

/// How the band between the header and the Status pane is split: the Controls pane alone on a
/// device with no tone control, and the Tone pane beside it on one that has it — a column
/// apart in the wide tier, where the Controls pane is wider too.
fn panes_of(panes: Rect, tone: bool, tier: Tier) -> (Rect, Option<Rect>) {
    let width = match tier.wide {
        true => CONTROLS_WIDTH_WIDE,
        false => CONTROLS_WIDTH,
    };
    match tone {
        true => {
            let [left, _, right] = Layout::horizontal([
                Constraint::Length(width),
                Constraint::Length(tier.gap()),
                Constraint::Min(0),
            ])
            .areas(panes);
            (left, Some(right))
        }
        false => (panes, None),
    }
}

/// The bordered panes this screen draws over `area`, in the order it lays them out: the
/// Controls pane, the Tone pane beside it on a device with tone control, and the Status pane
/// under them both.
///
/// This is the same layout the draw works from, so what the animation layer traces is where
/// the contours were painted.
pub(crate) fn frames_of(area: Rect, app: &App) -> Vec<(FrameId, Rect)> {
    if app.session.holding.is_some() {
        let (_, [_, body]) = layout_tier::split(area);
        return vec![(FrameId::Pane(Pane::Controls), body)];
    }
    let (tier, [_, panes, status]) = rows_of(area, app);
    let (controls, tone) = panes_of(panes, app.profile.eq.is_some(), tier);
    let mut frames = vec![(FrameId::Pane(Pane::Controls), controls)];
    frames.extend(tone.map(|area| (FrameId::Pane(Pane::Tone), area)));
    frames.push((FrameId::Pane(Pane::Status), status));
    frames
}

/// The pane the squirrel's home is on `area`, and where the pane's own content lies in it.
pub(crate) struct HomePane {
    /// The pane's frame.
    pub rect: Rect,
    /// The row the pane's content starts on: under its top border and the tier's padding.
    pub top: u16,
    /// Each line of content the pane draws, from `top` down: the column just past its text. A
    /// focused row's highlight running on past the text is no text.
    pub ends: Vec<u16>,
}

/// The pane the squirrel lives on: the Tone pane, or the Controls pane on a device with no tone
/// control. A screen with no device open has none.
pub(crate) fn home_of(area: Rect, app: &App, theme: &Theme) -> Option<HomePane> {
    if app.session.holding.is_some() {
        return None;
    }
    let (tier, [_, panes, _]) = rows_of(area, app);
    let (controls, tone) = panes_of(panes, app.profile.eq.is_some(), tier);
    let (rect, lines) = match (app.profile.eq.as_ref(), tone) {
        (Some(eq), Some(tone)) => {
            let max_width = usize::from(tier.block().inner(tone).width);
            let bar = controls_pane(
                app,
                theme,
                room(controls, tier),
                tier,
                height(controls, tier),
            )
            .bar;
            (tone, tone_lines(app, eq, theme, max_width, bar).lines)
        }
        _ => (
            controls,
            controls_pane(
                app,
                theme,
                room(controls, tier),
                tier,
                height(controls, tier),
            )
            .lines,
        ),
    };
    let inner = tier.block().inner(rect);
    let ends = lines
        .iter()
        .map(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            let width = widgets::text::cells(text.trim_end());
            inner
                .x
                .saturating_add(u16::try_from(width).unwrap_or(u16::MAX))
        })
        .collect();
    Some(HomePane {
        rect,
        top: inner.y,
        ends,
    })
}

/// The Status pane's first line over `area` — the one the faults are written on — for a frame
/// of `app` that has a fault to write there, and nothing at all otherwise.
///
/// This is where the animation layer pulses while a fault stands, so it answers for a frame
/// rather than for a layout a caller has to work out again.
pub(crate) fn fault_line(area: Rect, app: &App) -> Option<Rect> {
    if app.faults().is_empty() {
        return None;
    }
    let (tier, [_, _, status]) = rows_of(area, app);
    let inner = tier.block().inner(status);
    // The same width the line is written in, so a line with no room for the faults has none
    // for the pulse either.
    let width = usize::from(inner.width).saturating_sub(tier.trail());
    (!inner.is_empty() && faults_fit(app, width)).then_some(Rect { height: 1, ..inner })
}

/// The style a pane's contour is drawn in: the accent rule's own, shaded by how loud what is
/// playing is while the borders breathe with it (the level [`App::level`] reads from the
/// device's meters while they stream, else from the host sink capture).
///
/// A quiet level dims the contour to [`BREATH_FLOOR`] of the colour the glow is at rather than
/// putting it out, and a loud one takes it to that colour itself, so a breathing contour is
/// shaded in the same glow it has also carried along the palette's spectrum. No level at all (no
/// meter streaming and no capture running, or a meter whose profile names no scale), breathing
/// switched off, and a theme with no colour to shade all leave the contour exactly where the
/// theme drew it, as do the colour depths below true colour, where [`Theme::glow_shade`] is one
/// flat colour.
pub(crate) fn border_style(app: &App, theme: &Theme) -> Style {
    let rule = theme.style(Role::Rule);
    if !app.breathing || theme.monochrome() {
        return rule;
    }
    match app.level() {
        Some(level) => {
            let lifted = (level + f64::from(app.pulse()) * BEAT_LIFT).min(1.0);
            rule.fg(theme.glow_shade(BREATH_FLOOR + BREATH_SPAN * lifted))
        }
        None => rule,
    }
}

/// The style a frame's contour is drawn in, `focused` or not, on a screen whose frames every
/// palette without an emphasis colour draws in `plain`. Every screen's frames pass
/// [`border_style`] for it, so they glow and breathe with the level as the device screen's
/// panes do.
///
/// A palette with an emphasis colour of its own draws the bars and the selection in it, so the
/// accent is left to say where the keys are: the frame they are in keeps [`border_style`], and
/// every other frame is faint, whatever `plain` would have drawn it in.
pub(crate) fn contour(app: &App, theme: &Theme, focused: bool, plain: Style) -> Style {
    match (theme.emphasised(), focused) {
        (false, _) => plain,
        (true, true) => border_style(app, theme),
        (true, false) => theme.style(Role::Faint),
    }
}

/// The style a title takes on a screen with more than one frame, where it tells which of them
/// the keys are in: the selected role on the frame they are in, the title role on every other,
/// as the device screen's panes do.
pub(crate) fn pane_title(theme: &Theme, focused: bool) -> Style {
    match focused {
        true => theme.style(Role::Selected),
        false => theme.style(Role::Title),
    }
}

/// The style a title takes on a frame [`contour`] draws with a `plain` style of
/// `Style::default()`: nothing of its own — the text the frame is drawn over, as the contour
/// itself is — on a palette without an emphasis colour and in monochrome, which has no colour to
/// give it, and the emphasis on a palette with one, whatever colour the contour under it is
/// drawn in, and bold, as every other pane title on that palette is.
pub(crate) fn frame_title(theme: &Theme) -> Style {
    match theme.emphasised() && !theme.monochrome() {
        true => Style::default()
            .fg(theme.emphasis())
            .add_modifier(ratatui::style::Modifier::BOLD),
        false => Style::default(),
    }
}

/// Draw one bordered pane, record where a click on its title lands, and answer with the area
/// its lines went into.
///
/// The title of the pane the keys are going to is drawn in the selected role, which is how a
/// user sees what Tab did. Every other title takes the title role: the glow, bold — it sits on
/// the contour, which breathes with the meters, and a title with no colour of its own would
/// breathe along with it — or the emphasis, bold, where the palette names one.
fn pane(
    frame: &mut Frame,
    spec: PaneSpec<'_>,
    app: &App,
    theme: &Theme,
    tier: Tier,
    hits: &mut HitMap,
) -> Rect {
    let focused = app.pane == spec.focus;
    let title = match focused {
        true => theme.style(Role::Selected),
        false => theme.style(Role::Title),
    };
    let name = format!(" {} ", spec.title);
    let (words, word_hits) = widgets::actions::actions_title(
        spec.actions,
        None,
        widgets::actions::action_room(spec.area.width, cells(&name)),
        Screen::Device,
        &Scene::of(app),
        theme,
    );
    let block = tier
        .block()
        .border_style(contour(app, theme, focused, border_style(app, theme)))
        .title(Span::styled(name, title))
        .title(words.clone().right_aligned());
    let inner = block.inner(spec.area);
    frame.render_widget(Paragraph::new(spec.lines).block(block), spec.area);
    let titles = Rect {
        height: 1.min(spec.area.height),
        ..spec.area
    };
    if let Some(hit) = spec.title_hit {
        hits.push(titles, hit);
    }
    widgets::actions::push_action_hits(hits, spec.area, &words, &word_hits);
    inner
}

/// The whole width of interior line `top` of a pane drawn inside `inner`, which is where one
/// row landed.
fn pane_row(inner: Rect, top: u16) -> Rect {
    Rect {
        x: inner.x,
        y: inner.y + top,
        width: inner.width,
        height: 1,
    }
    .intersection(inner)
}

/// How many terminal cells `text` takes: two for a wide character, as the terminal draws it.
pub(crate) fn cells(text: &str) -> usize {
    text.chars().map(char_cells).sum()
}

/// How many terminal cells `c` takes.
fn char_cells(c: char) -> usize {
    widgets::text::cells(c.encode_utf8(&mut [0; 4]))
}

/// The `room` cells of `text` from its cell `from` on. A wide character the window's left edge
/// cuts leaves its cells inside the window blank, so what follows stays where the marquee put
/// it; one that would run past the right edge is left out.
fn cell_window(text: &str, from: usize, room: usize) -> String {
    let mut shown = String::new();
    let mut at = 0;
    let mut used = 0;
    for c in text.chars() {
        let width = char_cells(c);
        let start = at;
        at += width;
        if start < from && at <= from {
            continue;
        }
        if start < from {
            let blank = (at - from).min(room - used);
            shown.push_str(&" ".repeat(blank));
            used += blank;
            continue;
        }
        if used + width > room {
            break;
        }
        shown.push(c);
        used += width;
    }
    shown
}

/// `text` cut to `room` terminal cells, its tail replaced by `…` where it does not fit.
pub(super) fn cut_cells(text: &str, room: usize) -> String {
    if cells(text) <= room {
        return text.to_string();
    }
    let Some(room) = room.checked_sub(1) else {
        return String::new();
    };
    let mut cut = String::new();
    let mut used = 0;
    for c in text.chars() {
        let width = char_cells(c);
        if used + width > room {
            break;
        }
        cut.push(c);
        used += width;
    }
    cut.push('…');
    cut
}

/// The style a row draws in: the focused row stands out, and a `dim` one is held back in the
/// stale role — a value the device has not confirmed since the link went down, or (a caller's
/// own choice, see [`bands_apply`]) a band the mode the tone control is in does not apply.
///
/// A focused row is extended by one blank cell past its last character, in the same style, so
/// long as that stays short of `max_width` columns — the row's own budget, cursor included — a
/// row that already reaches it gets nothing extra.
pub(super) fn row_style(
    line: Line<'static>,
    focused: bool,
    dim: bool,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let mut style = match focused {
        true => theme.style(Role::Selected),
        false => Style::default(),
    };
    if dim {
        style = style.patch(theme.style(Role::Stale));
    }
    let mut line = line.style(style);
    if focused {
        widgets::text::pad_selected(&mut line.spans, style, max_width);
    }
    line
}

/// The raw value one function reads now, for the values this screen shows as numbers.
fn raw_of(app: &App, id: FunctionId) -> Option<i64> {
    app.readings.values.get(&id)?.raw_i64()
}

#[cfg(test)]
mod tests;
