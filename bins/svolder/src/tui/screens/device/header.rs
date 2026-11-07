// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header every screen draws: the small wordmark where the screen is wide enough, the
//! device, its link, its faults, the mouse indicator and the clock, over the rule, which on
//! the screens of the device is the tab strip.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::app::{App, Screen};
use crate::tui::decoration::wordmark::{self, Size};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render::Logo;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::spinner;

use super::{FIRMWARE, cells, cut_cells, raw_of};

/// Rows every screen's header takes on a screen `width` columns wide: one row of text and
/// the accent rule under it, or the small wordmark's two rows and the rule on a screen wide
/// enough to carry the mark.
pub(crate) fn header_height(width: u16) -> u16 {
    match marked(width) {
        true => wordmark::height(Size::Small) + 1,
        false => 2,
    }
}

/// What the header's bottom row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    /// A plain accent rule, across the setup wizard's header.
    Plain,
    /// The tab strip of a screen of the device: the four tabs with `current` marked, the EQ tab
    /// faint where the device has no tone control (`has_eq` false), and the help and Esc marks
    /// at the right end.
    Tabs {
        /// The screen being drawn, whose tab is marked.
        current: Screen,
        /// Whether the device has a tone control.
        has_eq: bool,
    },
}

/// Draw a header of `line` into `area`: the small wordmark first on a screen wide enough for
/// it, the text on the row above the rule, and `rule` across the bottom row, recording in
/// `hits` what a click on the strip's tabs and marks asks for.
///
/// Every screen draws its header through this, so the tone screen's own header line lands on
/// the same row as the device screen's.
pub(in crate::tui::screens) fn draw_header(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    line: Line<'static>,
    rule: &Rule,
    hits: &mut HitMap,
) {
    if marked(area.width) {
        frame.render_widget(
            Paragraph::new(wordmark::lines(Size::Small, theme)),
            Rect {
                x: area.x + MARK_LEFT,
                y: area.y,
                width: wordmark::width(Size::Small),
                height: wordmark::height(Size::Small).min(area.height),
            },
        );
    }
    frame.render_widget(Paragraph::new(line), header_text(area));
    let row = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1.min(area.height),
        ..area
    };
    let (rule_line, marks) = header_rule(theme, area.width, rule);
    frame.render_widget(Paragraph::new(rule_line), row);
    for (at, width, hit) in marks {
        hits.push(
            Rect {
                x: row.x + at,
                width,
                ..row
            }
            .intersection(row),
            hit,
        );
    }
}

/// Where the header's text line goes inside `area`: the row directly above the rule, which on
/// a marked header is the mark's second row, in the columns the mark leaves.
pub(in crate::tui::screens) fn header_text(area: Rect) -> Rect {
    let left = match marked(area.width) {
        true => MARK_LEFT + wordmark::width(Size::Small) + MARK_GAP,
        false => 0,
    };
    Rect {
        x: area.x + left,
        y: area.y + area.height.saturating_sub(2),
        width: area.width.saturating_sub(left),
        height: 1.min(area.height),
    }
}

/// Whether a screen `width` columns wide carries the wordmark in its header.
pub(in crate::tui::screens) fn marked(width: u16) -> bool {
    width >= MARK_FLOOR
}

/// The word the header's text row opens on where the screen is too narrow for the mark.
const WORD: &str = "svolder";

/// Where a header in `area` draws svolder's own logo: the small wordmark on a screen wide
/// enough for it, else the bold word that opens the text row.
pub(crate) fn logo_of(area: Rect) -> Option<Logo> {
    if area.height < 2 {
        return None;
    }
    Some(match marked(area.width) {
        true => Logo::Mark(Rect {
            x: area.x + MARK_LEFT,
            y: area.y,
            width: wordmark::width(Size::Small),
            height: wordmark::height(Size::Small).min(area.height),
        }),
        false => {
            let text = header_text(area);
            Logo::Word(Rect {
                x: text.x + 1,
                y: text.y,
                width: WORD.len() as u16,
                height: 1,
            })
        }
    })
}

/// Columns a screen must have before its header carries the small wordmark.
const MARK_FLOOR: u16 = 100;
/// Columns before the mark, which the text row's own leading space matches.
const MARK_LEFT: u16 = 1;
/// Columns between the mark and the header's text.
const MARK_GAP: u16 = 2;

/// The tabs of the strip in order, each with the screen it shows, its name and the key that
/// shows it.
const TABS: [(Screen, &str, Option<char>); 4] = [
    (Screen::Device, "Controls", Some('c')),
    (Screen::Tone, "EQ", Some('e')),
    (Screen::Telemetry, "Telemetry", Some('t')),
    (Screen::Devices, "Devices", Some('d')),
];

/// The rule under the header, across a screen `width` columns wide, and the cells of it a click
/// reaches: each as its offset from the left, its width and what it asks for.
///
/// A strip is a dash, the tabs joined by `──` (the current one as `┤ Name ├`, the others as
/// ` Name `), dashes filling to ` ? help ─ esc menu ` (`esc back` off the device screen) and
/// one more dash.
fn header_rule(theme: &Theme, width: u16, rule: &Rule) -> (Line<'static>, Vec<(u16, u16, Hit)>) {
    let rule_style = theme.style(Role::Rule);
    let Rule::Tabs { current, has_eq } = *rule else {
        let plain = Span::styled("─".repeat(usize::from(width)), rule_style);
        return (Line::from(plain), Vec::new());
    };
    let text = theme.style(Role::Text);
    let key = theme.style(Role::KeyHint).add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled("─", rule_style)];
    let mut marks = Vec::new();
    let mut at = 1usize;
    for (n, (screen, name, letter)) in TABS.into_iter().enumerate() {
        if n > 0 {
            spans.push(Span::styled("──", rule_style));
            at += 2;
        }
        let faint = screen == Screen::Tone && !has_eq;
        if screen == current {
            let accent = theme.style(Role::Cursor).add_modifier(Modifier::BOLD);
            spans.push(Span::styled("┤ ", rule_style));
            spans.push(Span::styled(name, accent));
            spans.push(Span::styled(" ├", rule_style));
            at += 2 + name.chars().count() + 2;
            continue;
        }
        spans.push(Span::styled(" ", rule_style));
        at += 1;
        match (faint, letter) {
            (true, _) => spans.push(Span::styled(name, theme.style(Role::Faint))),
            (false, Some(_)) => {
                let (first, rest) = name.split_at(1);
                spans.push(Span::styled(first, key));
                spans.push(Span::styled(rest, text));
            }
            (false, None) => spans.push(Span::styled(name, text)),
        }
        if !faint {
            marks.push((at, name.chars().count(), Hit::Tab(screen)));
        }
        at += name.chars().count();
        spans.push(Span::styled(" ", rule_style));
        at += 1;
    }
    let esc = match current {
        Screen::Device => "menu",
        _ => "back",
    };
    // ` ? help ─ esc <word> ` and the closing dash.
    let tail = 1 + "? help".len() + 1 + 1 + 1 + "esc ".len() + esc.len() + 1 + 1;
    let fill = usize::from(width).saturating_sub(at + tail);
    spans.push(Span::styled("─".repeat(fill), rule_style));
    at += fill;
    spans.push(Span::styled(" ", rule_style));
    at += 1;
    marks.push((at, "? help".len(), Hit::HelpMark));
    spans.push(Span::styled("?", key));
    spans.push(Span::styled(" help", text));
    spans.push(Span::styled(" ─ ", rule_style));
    at += "? help".len() + 3;
    marks.push((at, "esc ".len() + esc.len(), Hit::EscMark));
    spans.push(Span::styled("esc", key));
    spans.push(Span::styled(format!(" {esc}"), text));
    spans.push(Span::styled(" ─", rule_style));
    let marks = marks
        .into_iter()
        .map(|(at, width, hit)| {
            (
                u16::try_from(at).unwrap_or(u16::MAX),
                u16::try_from(width).unwrap_or(u16::MAX),
                hit,
            )
        })
        .collect();
    (Line::from(spans), marks)
}

/// Record the cells of the header's connection words, the faults and the mouse indicator from
/// the `marks` [`header_built`] answered, which
/// [`Hit::Connection`], [`Hit::FaultMark`] and [`Hit::MouseMark`] answer on. A click on any of
/// them does nothing; a pointer resting on one says what it is.
pub(in crate::tui::screens) fn push_marks(
    hits: &mut HitMap,
    area: Rect,
    marks: Vec<(usize, usize, Hit)>,
) {
    let text = header_text(area);
    for (at, width, hit) in marks {
        let (rect, hit) = mark_rect(text, at, width, hit);
        hits.push(rect, hit);
    }
}

/// The rectangle of a mark `width` cells wide, `at` cells in from the left of the header's
/// text row `text`, cut to that row.
pub(in crate::tui::screens) fn mark_rect(
    text: Rect,
    at: usize,
    width: usize,
    hit: Hit,
) -> (Rect, Hit) {
    let rect = Rect {
        x: text.x.saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
        width: u16::try_from(width).unwrap_or(u16::MAX),
        ..text
    };
    (rect.intersection(text), hit)
}

/// The header of a screen whose header area is `area`, and the marks on it: what the device is,
/// whether the link is up, and the local time; each mark as its offset from the left of the
/// line, its width and the hit it answers.
///
/// Every screen draws the same header, so the other screens call this rather than building
/// their own. The wordmark says svolder's own name, so a screen wide enough to draw it opens
/// the line on the device instead of on the word. The marks go to [`push_marks`] once the
/// header is drawn.
pub(in crate::tui::screens) fn header_built(
    app: &App,
    theme: &Theme,
    area: Rect,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let width = usize::from(header_text(area).width);
    let lead = match marked(area.width) {
        true => None,
        false => Some(Span::styled(
            format!(" {WORD}  "),
            theme.style(Role::Header),
        )),
    };
    let mut after = Vec::new();
    if let Some(build) = firmware(app) {
        after.push(Span::raw(format!(" · fw {build}")));
    }
    if let Some(unit) = app.session.unit.as_deref() {
        after.push(Span::raw(format!(" · {unit}")));
    }
    if let Some(words) = host_eq_words(app, area) {
        after.push(Span::raw(format!(" · {words}")));
    }
    after.push(Span::raw("  "));
    let mut marks = Vec::new();
    let (dot, word, role) = match app.session.connected {
        true => ("●", "connected", Role::Connected),
        false => ("○", "disconnected", Role::Disconnected),
    };
    let words = format!("{dot} {word}");
    let words = match !app.session.connected && app.session.feed_open {
        true => spinner::before(app.spinner(), &words),
        false => words,
    };
    let words = Span::styled(words, theme.style(role));
    let faults = fault_spans(app, theme);
    let mouse = mouse_span(app, theme);
    let clock = Span::raw(app.clock.clone());

    // The device's name gives way to everything else on the line: what follows it, the mouse
    // indicator and the clock stay whole, with a blank between, however long the name is.
    let kept: usize = lead.iter().map(Span::width).sum::<usize>()
        + after.iter().map(Span::width).sum::<usize>()
        + words.width()
        + faults.iter().map(Span::width).sum::<usize>()
        + mouse.width()
        + 1
        + clock.width();
    let name = match kept + cells(&app.session.product) > width {
        true => cut_cells(&app.session.product, width.saturating_sub(kept + 1)),
        false => app.session.product.clone(),
    };
    let mut spans: Vec<Span<'static>> = lead.into_iter().collect();
    spans.push(Span::styled(name, theme.style(Role::Header)));
    spans.extend(after);
    marks.push((
        Line::from(spans.clone()).width(),
        words.width(),
        Hit::Connection,
    ));
    spans.push(words);
    if !faults.is_empty() {
        let before = Line::from(spans.clone()).width();
        marks.push((before, faults.iter().map(Span::width).sum(), Hit::FaultMark));
    }
    spans.extend(faults);

    let mut line = Line::from(spans);
    let tail = mouse.width() + 1 + clock.width();
    let gap = width.saturating_sub(line.width() + tail);
    line.push_span(Span::raw(" ".repeat(gap)));
    marks.push((line.width(), mouse.width(), Hit::MouseMark));
    line.push_span(mouse);
    line.push_span(Span::raw(" "));
    line.push_span(clock);
    (line, marks)
}

/// The most cells of the computer EQ's name the header keeps.
const HOST_EQ_NAME_MAX: usize = 24;

/// What the wide tier's headers say while the computer EQ is on — `Host EQ NAME`, the name cut
/// to [`HOST_EQ_NAME_MAX`] cells — or `None` below the wide tier and while it is off.
pub(in crate::tui::screens) fn host_eq_words(app: &App, area: Rect) -> Option<String> {
    let name = app.computer_preset()?;
    Tier::of(area)
        .wide
        .then(|| format!("Host EQ {}", cut_cells(&name, HOST_EQ_NAME_MAX)))
}

/// The mouse-capture indicator every header shows, dim: `mouse:on` with capture on, else
/// `mouse:off`.
pub(in crate::tui::screens) fn mouse_span(app: &App, theme: &Theme) -> Span<'static> {
    Span::styled(
        format!("mouse:{}", if app.mouse { "on" } else { "off" }),
        theme.style(Role::Stale),
    )
}

/// The firmware build the device reports, once it has reported one.
fn firmware(app: &App) -> Option<i64> {
    raw_of(app, FIRMWARE)
}

/// The header's fault glyphs: one span per active fault, each the glyph its key earns
/// followed by the control's label in lower case, all in the fault role.
fn fault_spans(app: &App, theme: &Theme) -> Vec<Span<'static>> {
    app.faults()
        .into_iter()
        .map(|fault| {
            Span::styled(
                format!(
                    " {} {}",
                    fault_glyph(&fault.key),
                    fault.label.to_lowercase()
                ),
                theme.style(Role::Fault),
            )
        })
        .collect()
}

/// The glyph one fault's key earns: the lightning bolt for over-current, the thermometer
/// for anything that names heat, and a plain warning for the rest.
fn fault_glyph(key: &str) -> &'static str {
    if key == "fault_overcurrent" {
        "⚡"
    } else if key.contains("temp") || key.contains("thermal") {
        "♨"
    } else {
        "⚠"
    }
}
