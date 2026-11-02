// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Status pane: the device's readouts, firmware and faults, the advice or the readings
//! nothing else explains, the facts the backend lists about the device (its product, profile
//! and signal strength), and, while host audio shows, where the level comes from and what
//! the host is playing.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use svolder_profiles::{Control, ControlKind, EqParam, EqSpec, FlagWords, units};
use svolder_proto::{FunctionId, ValueType};

use crate::tui::app::{App, Reading};
use crate::tui::audio_level::LevelFrom;
use crate::tui::layout_tier::{self, Tier};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::graph::{self, Shape};
use crate::tui::widgets::{self, MISSING};

use super::{cell_window, cells, is_firmware_family, raw_of};

/// The Status pane's lines, fit to a pane `width` columns wide in a frame of `tier`: its own
/// two, then the `Device` row, the `Link` row, the `Level from` row, the `Now playing` row and
/// the `Passphrase` row under them while each is drawn.
pub(super) fn status_lines(
    app: &App,
    theme: &Theme,
    width: usize,
    tier: Tier,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        status_line_one(app, theme, width),
        status_line_two(app, theme, width),
    ];
    if device_drawn(app) {
        lines.push(device_line(app, width));
    }
    if link_drawn(app, tier) {
        lines.push(link_line(app, theme, width));
    }
    if level_from_drawn(app) {
        lines.push(level_from_line(app, theme));
    }
    if now_playing_drawn(app) {
        lines.push(now_playing_line(app, theme, width));
    }
    if let Some(line) = passphrase_line(app, theme, width) {
        lines.push(line);
    }
    lines
}

/// What the `Device` row puts between its facts.
const FACT_GAP: &str = " \u{b7} ";

/// Whether the Status pane draws the `Device` row: once the backend has listed the device.
pub(super) fn device_drawn(app: &App) -> bool {
    app.session.facts.is_some()
}

/// The `Device` row, fit to a pane `width` columns wide: the product with its id, the profile
/// the device is read through, and the signal strength as `Signal::describe` words it, with the
/// facts a backend has not got left out. Facts that do not fit are dropped whole from the right,
/// the signal first; what is left of the product alone is cut with `…` when even it is too wide.
fn device_line(app: &App, width: usize) -> Line<'static> {
    let Some(facts) = app.session.facts.as_ref() else {
        return Line::default();
    };
    let product = facts
        .product
        .clone()
        .unwrap_or_else(|| app.profile.display.clone());
    let mut kept = vec![match facts.product_id {
        Some(id) => format!("{product} (product {id})"),
        None => product,
    }];
    let profile = facts.profile.as_deref().unwrap_or(&app.profile.name);
    kept.push(format!("profile {profile}"));
    if let Some(signal) = facts.signal.as_ref() {
        kept.push(signal.describe(graph::wall_ms(), svolder_ipc::SIGNAL_MINUS));
    }
    let room = width.saturating_sub(1);
    while kept.len() > 1 && widgets::text::cells(&kept.join(FACT_GAP)) > room {
        kept.pop();
    }
    Line::raw(format!(" {}", fit(kept.join(FACT_GAP), room)))
}

/// What the `Passphrase` row opens on.
const PASSPHRASE: &str = " Passphrase: ";

/// Whether the Status pane draws the `Passphrase` row: while the backend holds a note that the
/// configured passphrase does not match the one the device advertises.
pub(super) fn passphrase_drawn(app: &App) -> bool {
    app.session.passphrase_note.is_some()
}

/// The `Passphrase` row, fit to a pane `width` columns wide: what the note says of the
/// configured passphrase, in the warning role, since the device will refuse what svolder sends
/// under it. A best-effort note — the advertisement can be older than the device's code — so it
/// says that the codes do not match, never which one is wrong, and never a digit.
fn passphrase_line(app: &App, theme: &Theme, width: usize) -> Option<Line<'static>> {
    let note = app.session.passphrase_note?;
    let room = width.saturating_sub(widgets::text::cells(PASSPHRASE));
    Some(Line::from(vec![
        Span::raw(PASSPHRASE),
        Span::styled(fit(note.short().to_string(), room), theme.style(Role::Warn)),
    ]))
}

/// What the `Link` row opens on.
const LINK: &str = " Link  ";
/// Columns between the strip and the span's name after it.
const LINK_GAP: usize = 2;
/// The same gap as text, before the link's state and flag.
const LINK_GAP_TEXT: &str = "  ";

/// Whether the Status pane draws the `Link` row: with graphs on, in a wide or tall frame. The
/// smallest terminal has no room for it.
pub(super) fn link_drawn(app: &App, tier: Tier) -> bool {
    app.tui.graphs && (tier.wide || tier.tall)
}

/// The `Link` row, fit to a pane `width` columns wide: the link's history over the link span as
/// a braille strip, top dots for connected and bottom dots for down (a column is down if the
/// link was down at any moment in it), and the span's name after it, like `12 h`. While the
/// device has refused the passphrase or its readings are stale, the link's state and that flag
/// follow the name, in the warning role (`connected · passphrase refused`).
fn link_line(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    let span = app.tui.link_span;
    let name = format!("{} h", span.name().trim_end_matches('h'));
    let flags = app
        .session
        .facts
        .as_ref()
        .and_then(|facts| crate::format::link_flags(facts.stale, facts.refused))
        .map(|flags| {
            let state = match app.session.connected {
                true => "connected",
                false => "disconnected",
            };
            format!("{LINK_GAP_TEXT}{state}{FACT_GAP}{flags}")
        });
    let flags_room = flags.as_deref().map_or(0, widgets::text::cells);
    let room = width.saturating_sub(
        widgets::text::cells(LINK) + LINK_GAP + widgets::text::cells(&name) + flags_room,
    );
    let columns = app
        .readings
        .link
        .columns(graph::wall_ms(), span.duration(), room * 2);
    let mut spans = vec![Span::raw(LINK)];
    if let Some(strip) = graph::draw(Shape::Link, &columns, &[], theme).pop() {
        spans.extend(strip);
    }
    spans.push(Span::raw(format!("{}{name}", " ".repeat(LINK_GAP))));
    if let Some(flags) = flags {
        spans.push(Span::styled(flags, theme.style(Role::Warn)));
    }
    Line::from(spans)
}

/// What the `Level from` row opens on.
const LEVEL_FROM: &str = " Level from: ";

/// Whether the Status pane draws the `Level from` row: while host audio shows, which is when
/// the level can come from either of two places. With host audio off it can only be the
/// device's own meters, and there is no row.
pub(super) fn level_from_drawn(app: &App) -> bool {
    app.host_shown()
}

/// The `Level from` row: where the level the glow, the breathing borders and the title meter
/// read comes from now — `meters`, `host`, or `—`, dim, from nowhere.
fn level_from_line(app: &App, theme: &Theme) -> Line<'static> {
    let from = match app.level_from() {
        LevelFrom::Meters => Span::raw("meters"),
        LevelFrom::Host => Span::raw("host"),
        LevelFrom::Nowhere => Span::styled(MISSING, theme.style(Role::Dim)),
    };
    Line::from(vec![Span::raw(LEVEL_FROM), from])
}

/// What the `Now playing` row opens on.
const NOW_PLAYING: &str = " Now playing: ";
/// Columns between the track's words and the player's own name after them.
const IDENTITY_GAP: usize = 3;

/// Whether the Status pane draws the `Now playing` row: while host audio shows and the session
/// bus has answered. With no bus, or host audio off, there is no row at all.
pub(super) fn now_playing_drawn(app: &App) -> bool {
    app.host_shown() && app.host.bus
}

/// The cells the `Now playing` row gives the track's words on a terminal `size` large: the
/// Status pane's width inside its border and padding, less what the row opens on. The row's
/// marquee carries words longer than this.
pub(crate) fn now_playing_room(size: (u16, u16)) -> usize {
    let (tier, [_, body]) = layout_tier::split(Rect::new(0, 0, size.0, size.1));
    usize::from(tier.block().inner(body).width)
        .saturating_sub(tier.trail())
        .saturating_sub(widgets::text::cells(NOW_PLAYING))
}

/// The `Now playing` row, fit to a pane `width` columns wide: the followed player's track as
/// `Title — Artist`, and the player's own name dim after it where there is room for both. Words
/// too long for the row are a window the row's marquee carries along them, the name left out.
/// With no player on the bus the row reads `—`, dim. Everything is measured in terminal cells,
/// so a wide character counts for the two it draws in.
fn now_playing_line(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    let room = width.saturating_sub(widgets::text::cells(NOW_PLAYING));
    let mut spans = vec![Span::raw(NOW_PLAYING)];
    let (Some(playing), Some(words)) = (app.host.playing.as_ref(), app.host.now_playing()) else {
        spans.push(Span::styled(MISSING, theme.style(Role::Dim)));
        return Line::from(spans);
    };
    let said = cells(&words);
    let identity = playing.identity.as_str();
    let named = !identity.is_empty() && said + IDENTITY_GAP + cells(identity) <= room;
    let shown = match said <= room {
        true => words,
        false => cell_window(&words, app.host.marquee.at(said - room), room),
    };
    let shown = match shown.is_empty() {
        true => Span::styled(MISSING, theme.style(Role::Dim)),
        false => Span::raw(shown),
    };
    spans.push(shown);
    if named {
        spans.push(Span::raw(" ".repeat(IDENTITY_GAP)));
        spans.push(Span::styled(identity.to_string(), theme.style(Role::Dim)));
    }
    Line::from(spans)
}

/// The Status pane's first line: the device's readouts, the firmware builds it reports, and
/// the faults it has active, in that order; a profile with no fault control writes no faults.
///
/// The faults themselves take the colour of what they say: nothing active reads as a good
/// state, and a fault the device reports as an alert. A boolean readout that reads false is
/// dim.
fn status_line_one(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    let segments = line_one_segments(app, width);
    if !has_fault_controls(app) {
        return fit_spans(body_spans(&segments, theme, false), width);
    }
    let head = faults_head(&body_of(&segments));
    let role = match app.faults().is_empty() {
        true => Role::Good,
        false => Role::Alert,
    };
    // What the readouts leave of the line is what the faults are written in. A pane too narrow
    // to have left any writes neither the `Faults: ` word nor the faults: the header's fault
    // mark carries them there.
    let room = width.saturating_sub(widgets::text::cells(&head));
    if room == 0 {
        return fit_spans(body_spans(&segments, theme, true), width);
    }
    let mut spans = body_spans(&segments, theme, true);
    spans.push(Span::raw(FAULTS));
    spans.push(Span::styled(
        fit(faults_summary(app), room),
        theme.style(role),
    ));
    Line::from(spans)
}

/// One thing the first line writes before the faults: a readout or a firmware build, as words,
/// and whether it is a boolean readout that reads false, which is drawn dim.
struct Segment {
    /// the words
    text: String,
    /// whether the words are a false boolean readout
    dim: bool,
}

/// What opens the faults on the first line.
const FAULTS: &str = "Faults: ";
/// What the first line puts between one segment and the next.
const SEGMENT_GAP: &str = "   ";

/// The first line's segments in function-address order: the readouts, then the firmware builds,
/// sorted together.
fn first_line_segments(app: &App) -> Vec<Segment> {
    let mut segments = readout_segments(app);
    segments.extend(firmware_segments(app));
    segments.sort_unstable_by_key(|(id, _)| *id);
    segments.into_iter().map(|(_, segment)| segment).collect()
}

/// The first line's segments as a pane `width` columns wide has room for them. The dim false
/// readouts are the first to go: while the line (the faults' own words included) is wider than
/// the pane, the last of them is dropped whole, so the readouts that read true, the firmware
/// builds and the faults keep the room they would have had with no false readout written.
fn line_one_segments(app: &App, width: usize) -> Vec<Segment> {
    let mut segments = first_line_segments(app);
    let wanted = |segments: &[Segment]| match has_fault_controls(app) {
        true => {
            widgets::text::cells(&faults_head(&body_of(segments)))
                + widgets::text::cells(&faults_summary(app))
        }
        false => 1 + widgets::text::cells(&body_of(segments)),
    };
    while wanted(&segments) > width
        && let Some(last) = segments.iter().rposition(|segment| segment.dim)
    {
        segments.remove(last);
    }
    segments
}

/// The segments as the first line's text writes them before the faults, without the line's own
/// leading space.
fn body_of(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(SEGMENT_GAP)
}

/// Whether the profile has any fault control. A device with none, such as a DAC, writes no
/// `Faults:` segment at all.
pub(crate) fn has_fault_controls(app: &App) -> bool {
    app.profile
        .controls
        .iter()
        .any(|c| c.kind == svolder_profiles::ControlKind::Fault)
}

/// The segments as spans: the line's own leading space, each segment in its own style, and,
/// with `trailing`, the gap that would lead into the `Faults: ` word. Nothing before the faults
/// has only the leading space, not a second gap where a segment would otherwise have been.
fn body_spans(segments: &[Segment], theme: &Theme, trailing: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw(" ")];
    for (at, segment) in segments.iter().enumerate() {
        if at > 0 {
            spans.push(Span::raw(SEGMENT_GAP));
        }
        spans.push(match segment.dim {
            true => Span::styled(segment.text.clone(), theme.style(Role::Dim)),
            false => Span::raw(segment.text.clone()),
        });
    }
    if trailing && !segments.is_empty() {
        spans.push(Span::raw(SEGMENT_GAP));
    }
    spans
}

/// `spans` as one line cut to `width` columns, its tail replaced by `…` when it does not fit,
/// each character keeping the style of the span it came from.
fn fit_spans(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let whole: String = spans.iter().map(|span| span.content.as_ref()).collect();
    if widgets::text::cells(&whole) <= width {
        return Line::from(spans);
    }
    let mut cut = fit(whole, width).chars().collect::<Vec<_>>().into_iter();
    let mut fitted = Vec::new();
    for span in spans {
        let piece: String = cut.by_ref().take(span.content.chars().count()).collect();
        if piece.is_empty() {
            break;
        }
        fitted.push(Span::styled(piece, span.style));
    }
    Line::from(fitted)
}

/// What the first line writes before the faults: the line's own leading space, the readouts
/// and firmware `body`, and the `Faults: ` word.
fn faults_head(body: &str) -> String {
    format!("{}{FAULTS}", body_head(body))
}

/// The first line up to where `Faults: ` would begin. A device with no readout or firmware
/// reading yet has nothing to put before it, so only the line's own leading space goes in front
/// of the word, not a second gap where a segment would otherwise have been.
fn body_head(body: &str) -> String {
    match body.is_empty() {
        true => " ".to_string(),
        false => format!(" {body}{SEGMENT_GAP}"),
    }
}

/// Whether the Status pane's first line, `width` columns wide, has room to write the faults
/// after the readouts: what the animation layer's pulse over that line answers to.
pub(super) fn faults_fit(app: &App, width: usize) -> bool {
    has_fault_controls(app)
        && width > widgets::text::cells(&faults_head(&body_of(&line_one_segments(app, width))))
}

/// The Status pane's second line: the advice while `show_advice` asks for it and there is any,
/// or the readings nothing else explains.
fn status_line_two(app: &App, theme: &Theme, width: usize) -> Line<'static> {
    if app.notices.show_advice
        && let Some(advice) = advice_text(app)
    {
        return Line::from(Span::styled(
            fit(format!(" Advice: {advice}"), width),
            theme.style(Role::Warn),
        ));
    }
    let entries = raw_entries(app);
    if entries.is_empty() {
        return Line::raw(fit(" Raw: none".to_string(), width));
    }
    let joined = entries
        .iter()
        .map(|(id, reading)| {
            format!(
                "{id}={} {}",
                reading.value.value_type, reading.value.display
            )
        })
        .collect::<Vec<_>>()
        .join("   ");
    let count = entries.len();
    let word = if count == 1 { "function" } else { "functions" };
    Line::raw(fit(
        format!(" Raw: {joined}   ({count} unknown {word})"),
        width,
    ))
}

/// What the advice line has to say: an active fault's own advice, else the step the last
/// failure the loop reported named, else nothing.
///
/// A fault comes first because it is the device saying something is wrong with it now; the
/// step a failure named is what to do about something that already happened.
fn advice_text(app: &App) -> Option<String> {
    match app.faults().first() {
        Some(fault) => Some(fault.advice.clone().unwrap_or_default()),
        None => app.notices.advice.clone(),
    }
}

/// Each `Readout` control with a reading, as its label and the value's name (or the raw
/// number, when the profile names no value for it), keyed by function address for sorting
/// alongside the firmware entries. A boolean readout is its bare label while it reads true
/// (an integer 1 counts, and so does `only_while` holding) and written out with the web's
/// false word, dim, while it reads false ([`false_words`]). A readout on `210.2.x` is left to
/// [`firmware_segments`], which owns that family's formatting, so it is not drawn twice; a
/// floor a meter's zero follows places the meter's scale and is not listed.
fn readout_segments(app: &App) -> Vec<(FunctionId, Segment)> {
    let mut controls: Vec<&Control> = app
        .profile
        .controls
        .iter()
        .filter(|control| control.kind == ControlKind::Readout)
        .filter(|control| !is_firmware_family(control.id))
        .filter(|control| !app.profile.sets_a_zero(control))
        .collect();
    controls.sort_unstable_by_key(|control| control.id);
    controls
        .into_iter()
        .filter_map(|control| {
            let reading = app.readings.values.get(&control.id)?;
            if control.value_type == ValueType::Bool
                || matches!(reading.value.raw, serde_json::Value::Bool(_))
            {
                let shown =
                    units::shows_true(&app.profile, control, reading.value.reads_true(), |gate| {
                        app.readings
                            .values
                            .get(&gate.id)
                            .map(|gate_reading| gate_reading.value.reads_true())
                    });
                let segment = match shown {
                    true => Segment {
                        text: control.label.clone(),
                        dim: false,
                    },
                    false => Segment {
                        text: false_words(control),
                        dim: true,
                    },
                };
                return Some((control.id, segment));
            }
            let raw = reading.raw_i64()?;
            let name = u8::try_from(raw)
                .ok()
                .and_then(|v| control.value_name(v))
                .map(str::to_string)
                .unwrap_or_else(|| raw.to_string());
            let text = format!("{} {name}", control.label);
            Some((control.id, Segment { text, dim: false }))
        })
        .collect()
}

/// A boolean readout's label written for its false state, with the words every front end takes
/// from [`Control::flag_words`]. A label that already ends in the true word (`USB connected`)
/// has it replaced rather than followed, so it reads `USB not connected`, not `USB connected not
/// connected`; any other label has the false word after it (`Headphone output no`).
fn false_words(control: &Control) -> String {
    let label = control.label.as_str();
    let words = control.flag_words();
    let stem = match words {
        FlagWords::Plain => None,
        _ => label.strip_suffix(words.on()),
    };
    match stem {
        Some(stem) if stem.is_empty() || stem.ends_with(' ') => format!("{stem}{}", words.off()),
        _ => format!("{label} {}", words.off()),
    }
}

/// The main firmware build's reading, `210.2.1`.
const FIRMWARE: FunctionId = FunctionId::three(210, 2, 1);
/// The DSP firmware build's reading, `210.2.2`.
const DSP: FunctionId = FunctionId::three(210, 2, 2);

/// The firmware builds the device reports on `210.2.x`: the control's label and the raw
/// build when the profile names one, else, as the web words them, `fw N` for the main build
/// and `DSP N` for the DSP's, and `fw <address> <raw>` for any other subsystem.
fn firmware_segments(app: &App) -> Vec<(FunctionId, Segment)> {
    app.readings
        .values
        .keys()
        .filter(|id| is_firmware_family(**id))
        .filter_map(|id| {
            let raw = raw_of(app, *id)?;
            let text = match app.profile.control_by_id(*id) {
                Some(control) => format!("{} {raw}", control.label),
                None if *id == FIRMWARE => format!("fw {raw}"),
                None if *id == DSP => format!("DSP {raw}"),
                None => format!("fw {id} {raw}"),
            };
            Some((*id, Segment { text, dim: false }))
        })
        .collect()
}

/// The Status pane's `Faults:` word: the active faults' labels as the profile writes them,
/// or `none`.
fn faults_summary(app: &App) -> String {
    let faults = app.faults();
    if faults.is_empty() {
        return "none".to_string();
    }
    faults
        .iter()
        .map(|fault| fault.label.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The readings function-id order, that neither a control, the firmware query nor the tone
/// control account for.
fn raw_entries(app: &App) -> Vec<(FunctionId, &Reading)> {
    app.readings
        .values
        .iter()
        .filter(|(id, _)| !explained(app, **id))
        .map(|(id, reading)| (*id, reading))
        .collect()
}

/// Whether the profile explains `id` some way other than the Status pane's Raw list: a
/// control, the firmware version query, or one of the tone control's own functions.
fn explained(app: &App, id: FunctionId) -> bool {
    app.profile.control_by_id(id).is_some()
        || is_firmware_family(id)
        || app.profile.eq.as_ref().is_some_and(|eq| eq_owns(eq, id))
}

/// Whether the tone control reads or writes `id`: its mode, its loudness, or one band's
/// frequency, gain or Q.
fn eq_owns(eq: &EqSpec, id: FunctionId) -> bool {
    id == eq.mode_id()
        || id == eq.loudness_id()
        || (0..eq.bands.len()).any(|band| {
            [EqParam::Frequency, EqParam::Gain, EqParam::Q]
                .into_iter()
                .any(|param| eq.band_id(band, param) == Some(id))
        })
}

/// `text` cut to `width` columns, its tail replaced by `…` when it does not fit.
fn fit(text: String, width: usize) -> String {
    widgets::text::cut_to_cells(&text, width)
}
