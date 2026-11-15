// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Web UI box of the Settings modal: its lines, QR codes and mouse targets.

use super::*;

/// Columns the Web UI box takes, borders and all.
const WEB_WIDTH: u16 = 64;
/// Columns the Web UI box's labels take, the space after them included.
const WEB_LABEL: usize = 14;
/// What the box says under a link on show: the terminal's own link handling, where it has any.
const WEB_LINK_HINT: &str = "Ctrl+click opens it; Shift+right-click copies it";
/// What the box says under a link that replaced one a browser used.
const WEB_LINK_REPLACED: &str = "the last link no longer works; a fresh one is shown";

/// The Web UI box: seven `label · value` lines, the QR codes below them where the box has room
/// (the sign-in link's, and the guest page's while the guest view is on), the question before
/// the token is replaced while it is up, and its keys. Answers the cells it fills.
pub(super) fn draw_web_box(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    web: &WebBox,
    theme: &Theme,
    live_hits: &mut HitMap,
) -> Rect {
    let (width, lines, link_lines) = web_box_lines(app, web, area, theme);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let block = widgets::surfaces::modal_block(Block::bordered(), " Web UI ", app, theme);
    let inner = block.inner(box_area);
    for (at, cells) in link_lines {
        let text = Rect {
            x: inner.x + 1,
            y: inner
                .y
                .saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: cells,
            height: 1,
        };
        let seen = text.intersection(inner);
        if !seen.is_empty() {
            live_hits.push(seen, Hit::WebLink);
        }
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    box_area
}

/// The Web UI box's width, its lines, and the lines the link's text fills with each one's width
/// in cells: what a click opens. The box is wider than usual when two QR codes sit side by side
/// and `area` holds that width and the lines under it.
fn web_box_lines(
    app: &App,
    web: &WebBox,
    area: Rect,
    theme: &Theme,
) -> (u16, Vec<Line<'static>>, Vec<(usize, u16)>) {
    let qrs = web_qrs(app, web);
    if let Some(width) = side_by_side_width(&qrs).filter(|width| *width <= area.width) {
        let (lines, link_lines, fits) = web_box_at(app, web, area, theme, width, &qrs, true);
        if fits {
            return (width, lines, link_lines);
        }
    }
    let width = WEB_WIDTH.min(area.width);
    let (lines, link_lines, _) = web_box_at(app, web, area, theme, width, &qrs, false);
    (width, lines, link_lines)
}

/// The Web UI box's lines at `width`, with the QR codes `qrs` side by side or stacked, and
/// whether the codes (or none) fit.
fn web_box_at(
    app: &App,
    web: &WebBox,
    area: Rect,
    theme: &Theme,
    width: u16,
    qrs: &[Qr],
    side: bool,
) -> (Vec<Line<'static>>, Vec<(usize, u16)>, bool) {
    let across = usize::from(width.saturating_sub(2));
    let room = across.saturating_sub(WEB_LABEL + 1);
    let values = web_values(app, web);
    let mut lines: Vec<Line<'static>> = Vec::new();
    // The lines the link's text fills, with each one's width in cells: what a click opens.
    let mut link_lines: Vec<(usize, u16)> = Vec::new();
    // The link's lines take the box's width less a cell of margin each side, so the text of a
    // wrapped link is whole on its lines and a selection of them is the link.
    let link_room = across.saturating_sub(2).max(1);
    let underline = match app.pointer.over_link {
        true => Modifier::UNDERLINED,
        false => Modifier::empty(),
    };
    for (row, value) in values {
        let style = match web.row == row {
            true => theme.style(Role::Selected),
            false => theme.style(Role::Text),
        };
        let live_link = web
            .link
            .as_ref()
            .filter(|c| c.until > app.now)
            .filter(|_| row == WebRow::Link);
        let mut chunks = value.chars().collect::<Vec<_>>();
        let first: String = chunks.drain(..room.min(chunks.len())).collect();
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                widgets::rows::label(row.label(), WEB_LABEL)
                    .content
                    .to_string(),
                style,
            ),
            Span::styled(first, style),
        ]));
        // A value longer than the box is wide goes on under its label.
        while !chunks.is_empty() {
            let part: String = chunks.drain(..room.min(chunks.len())).collect();
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(WEB_LABEL + 1)),
                Span::styled(part, style),
            ]));
        }
        // The link on lines of its own, as wide as they go, with nothing after its last piece.
        if let Some(link) = live_link {
            let text: Vec<char> = link.text.chars().collect();
            for part in text.chunks(link_room) {
                let part: String = part.iter().collect();
                link_lines.push((lines.len(), cell_count(&part)));
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(part, style.add_modifier(underline)),
                ]));
            }
            lines.extend(dim_wrapped(WEB_LINK_HINT, across, theme));
            if web.link_replaced {
                lines.extend(dim_wrapped(WEB_LINK_REPLACED, across, theme));
            }
        }
        if row == WebRow::Pairing && web.pair.is_some() {
            lines.extend(pairing_hint_lines(theme));
        }
    }
    if web.rotating {
        lines.extend(rotate_prompt_lines(theme));
    }
    let room = QrRoom {
        height: usize::from(area.height),
        across,
        above: lines.len(),
    };
    let (qr, fits) = web_qr_lines(app, web, qrs, side, &room, theme);
    lines.extend(qr);
    lines.extend(web_key_lines(theme));
    (lines, link_lines, fits)
}

/// The value each row of the Web UI box shows, in row order.
fn web_values(app: &App, web: &WebBox) -> [(WebRow, String); 7] {
    let conn = &app.overlays.conn;
    let direct = conn.daemon_up != Some(true);
    let ask = |code: &Option<web_state::Code>, pairing: bool, makes: &str| match (code, direct) {
        (Some(code), _) if pairing => web_state::code_line(code, app.now),
        (Some(code), _) => web_state::timer(code, app.now),
        (None, true) if app.session.daemon && conn.daemon_up.is_none() => "…".to_string(),
        (None, true) => "the daemon serves the web UI".to_string(),
        (None, false) => makes.to_string(),
    };
    let token = match &conn.web_token {
        TokenView::Present { masked, .. } => masked.clone(),
        TokenView::Missing => "not made yet".to_string(),
        TokenView::Unreadable(_) => "unreadable".to_string(),
        TokenView::Unread => "…".to_string(),
    };
    [
        (
            WebRow::State,
            web_state::state_word(conn.web_status.as_ref(), conn.daemon_up, app.session.daemon),
        ),
        (
            WebRow::WebUi,
            match conn.web.enabled {
                true => "on".to_string(),
                false => "off".to_string(),
            },
        ),
        (WebRow::Reach, web_state::reach(conn.web.listen).word()),
        (
            WebRow::Guest,
            match conn.web.guest {
                svolder_ipc::config::WebGuest::Read => "read-only".to_string(),
                svolder_ipc::config::WebGuest::None => "off".to_string(),
            },
        ),
        (WebRow::Link, ask(&web.link, false, "l makes one")),
        (WebRow::Pairing, ask(&web.pair, true, "p makes one")),
        (WebRow::Token, token),
    ]
}

/// `text` wrapped to the Web UI box's width less a cell of margin each side, dimmed, with one
/// cell of margin on the left.
fn dim_wrapped(text: &str, across: usize, theme: &Theme) -> Vec<Line<'static>> {
    widgets::text::wrap(text, across.saturating_sub(2))
        .into_iter()
        .map(|part| Line::from(Span::styled(format!(" {part}"), theme.style(Role::Dim))))
        .collect()
}

/// The two dim lines under a pairing code, indented under the row's label.
fn pairing_hint_lines(theme: &Theme) -> Vec<Line<'static>> {
    [
        "guesses from the network can use a code up:",
        "the link is the fallback",
    ]
    .into_iter()
    .map(|part| {
        Line::from(vec![
            Span::raw(" ".repeat(WEB_LABEL + 1)),
            Span::styled(part, theme.style(Role::Dim)),
        ])
    })
    .collect()
}

/// The question the Web UI box asks before it replaces the token, with its keys.
fn rotate_prompt_lines(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(
            " Replace the web token? [Y/n]",
            theme.style(Role::Warn),
        )),
        Line::from(Span::styled(
            " every signed-in browser is signed out",
            theme.style(Role::Warn),
        )),
        widgets::surfaces::modal_keys(&[("Enter/y", "replaces"), ("Esc/n/q", "cancels")], theme),
    ]
}

/// The blank line and the two key lines that close the Web UI box.
fn web_key_lines(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        widgets::surfaces::modal_keys(
            &[
                ("↑↓", "rows"),
                ("←→", "change"),
                ("Enter", "link/code"),
                ("l", "link"),
                ("c", "copy"),
            ],
            theme,
        ),
        widgets::surfaces::modal_keys(
            &[
                ("p", "code"),
                ("g", "guest"),
                ("r", "token"),
                ("click", "link opens"),
                ("Esc/q", "closes"),
            ],
            theme,
        ),
    ]
}

/// The caption under the sign-in QR code.
const WEB_QR_SIGN_IN: &str = "sign in";
/// The caption under the guest QR code: the warning, on one line.
const WEB_QR_GUEST: &str = "guest: read-only, anyone on your network";
/// Cells between two QR codes drawn side by side.
const WEB_QR_GAP: usize = 2;

/// A QR code the Web UI box draws, with the caption under it.
struct Qr {
    /// The code's half-block lines, quiet zone included.
    code: Vec<String>,
    /// The one line under it.
    caption: &'static str,
}

impl Qr {
    /// The QR code of `link` with `caption` under it; none for a link that does not fit one.
    fn of(link: &str, caption: &'static str) -> Option<Qr> {
        crate::qr::half_blocks(link)
            .ok()
            .map(|code| Qr { code, caption })
    }

    /// The code's width in cells.
    fn code_width(&self) -> usize {
        self.code.first().map_or(0, |line| line.chars().count())
    }

    /// The cells the code and its caption take, whichever is wider.
    fn column(&self) -> usize {
        self.code_width().max(widgets::text::cells(self.caption))
    }
}

/// The QR codes the box draws, in order: the sign-in link's while one is on show, the guest
/// page's while the guest view is on and the page is known.
fn web_qrs(app: &App, web: &WebBox) -> Vec<Qr> {
    if reach_words(app, web).is_some() {
        return Vec::new();
    }
    let sign_in = web
        .link
        .as_ref()
        .filter(|link| link.until > app.now)
        .and_then(|link| Qr::of(&link.text, WEB_QR_SIGN_IN));
    let guest = web
        .page
        .as_ref()
        .filter(|_| app.overlays.conn.guest_view() == GuestView::Open)
        .and_then(|page| Qr::of(&page.url, WEB_QR_GUEST));
    sign_in.into_iter().chain(guest).collect()
}

/// The words that stand in for the QR codes when a phone cannot reach the page, while a code
/// would be drawn: a loopback listen with no `[web] url`, or no base a phone reaches.
fn reach_words(app: &App, web: &WebBox) -> Option<&'static str> {
    let conn = &app.overlays.conn;
    let link_on_show = web.link.as_ref().is_some_and(|link| link.until > app.now);
    if !link_on_show && conn.guest_view() != GuestView::Open {
        return None;
    }
    if web_state::reach(conn.web.listen) == web_state::Reach::ThisComputer && conn.web.url.is_none()
    {
        return Some("a phone cannot reach this computer's loopback: choose your network");
    }
    if web.link_local || web.page.as_ref().is_some_and(|page| page.local) {
        return Some(
            "a phone cannot open this link: no address a phone can reach was found, so set \
             [web] url",
        );
    }
    None
}

/// The box width two QR codes side by side need, borders and a cell of margin each side included;
/// none unless there are two.
fn side_by_side_width(qrs: &[Qr]) -> Option<u16> {
    let [first, second] = qrs else {
        return None;
    };
    let cells = first.column() + WEB_QR_GAP + second.column() + 4;
    Some(u16::try_from(cells).unwrap_or(u16::MAX).max(WEB_WIDTH))
}

/// A QR code line painted black on white, or `cells` blanks where the code has no line left.
fn qr_row(qr: &Qr, row: usize) -> (Span<'static>, usize) {
    match qr.code.get(row) {
        Some(line) => (
            Span::styled(line.clone(), Style::new().fg(Color::Black).bg(Color::White)),
            qr.code_width(),
        ),
        None => (Span::raw(" ".repeat(qr.code_width())), qr.code_width()),
    }
}

/// `columns` (each a span and its width in cells) laid out in cells `column` wide each, `gap`
/// apart, the whole centred in `across`.
fn centred_columns(columns: Vec<(Span<'static>, usize, usize)>, across: usize) -> Line<'static> {
    let total: usize = columns.iter().map(|(_, _, wide)| wide).sum::<usize>()
        + WEB_QR_GAP * columns.len().saturating_sub(1);
    let mut spans = vec![Span::raw(" ".repeat(across.saturating_sub(total) / 2))];
    for (at, (span, cells, wide)) in columns.into_iter().enumerate() {
        if at > 0 {
            spans.push(Span::raw(" ".repeat(WEB_QR_GAP)));
        }
        let before = wide.saturating_sub(cells) / 2;
        spans.push(Span::raw(" ".repeat(before)));
        spans.push(span);
        spans.push(Span::raw(" ".repeat(wide.saturating_sub(cells + before))));
    }
    Line::from(spans)
}

/// The room the Web UI box's QR codes have: the terminal's rows, the box's inner width and the
/// lines the box holds above them.
#[derive(Clone, Copy)]
struct QrRoom {
    /// The terminal's rows.
    height: usize,
    /// The box's inner width in cells.
    across: usize,
    /// The lines above the codes.
    above: usize,
}

/// What the Web UI box shows under its rows: the QR codes `qrs`, black on white with a caption
/// under each, side by side or one above the other, where `area` has room for them beside the
/// `above` lines the box already holds; the words that say to widen the terminal where it has
/// not, and on a loopback listen the words that say a phone cannot reach it (once, for both
/// codes). Nothing with no QR code to draw. Answers the lines and whether they fit.
fn web_qr_lines(
    app: &App,
    web: &WebBox,
    qrs: &[Qr],
    side: bool,
    room: &QrRoom,
    theme: &Theme,
) -> (Vec<Line<'static>>, bool) {
    let QrRoom {
        height,
        across,
        above,
    } = *room;
    let link_on_show = web.link.as_ref().is_some_and(|link| link.until > app.now);
    let guest = app.overlays.conn.guest_view();
    if !link_on_show && guest == GuestView::Closed {
        return (Vec::new(), true);
    }
    let words = |text: &str| -> (Vec<Line<'static>>, bool) {
        let mut lines = vec![Line::default()];
        lines.extend(dim_wrapped(text, across, theme));
        (lines, true)
    };
    if let Some(text) = reach_words(app, web) {
        return words(text);
    }
    // Under `auth = "none"` every visitor is signed in with full access: no code is drawn for a
    // read-only view, and the box says so.
    let note = match guest {
        GuestView::NoEffect => {
            words("no guest view: auth is none, so every visitor has full access").0
        }
        _ => Vec::new(),
    };
    if qrs.is_empty() {
        return (note, true);
    }
    let caption = |qr: &Qr| {
        let cells = widgets::text::cells(qr.caption);
        (
            Span::styled(qr.caption, theme.style(Role::Dim)),
            cells,
            qr.column(),
        )
    };
    let mut lines = vec![Line::default()];
    match side {
        true => {
            let tallest = qrs.iter().map(|qr| qr.code.len()).max().unwrap_or(0);
            for row in 0..tallest {
                let columns = qrs
                    .iter()
                    .map(|qr| {
                        let (span, cells) = qr_row(qr, row);
                        (span, cells, qr.column())
                    })
                    .collect();
                lines.push(centred_columns(columns, across));
            }
            lines.push(centred_columns(qrs.iter().map(caption).collect(), across));
        }
        false => {
            for (at, qr) in qrs.iter().enumerate() {
                if at > 0 {
                    lines.push(Line::default());
                }
                for row in 0..qr.code.len() {
                    let (span, cells) = qr_row(qr, row);
                    lines.push(centred_columns(vec![(span, cells, cells)], across));
                }
                lines.push(centred_columns(vec![caption(qr)], across));
            }
        }
    }
    // The rows above, the codes and captions, and the blank and two key lines, between two
    // borders.
    let wanted = above + lines.len() + note.len() + 3 + 2;
    let wide = |qr: &Qr| qr.column() <= across;
    let fits = wanted <= height && (side || qrs.iter().all(wide));
    let (mut lines, fits) = match fits {
        true => (lines, true),
        false => (words("widen the terminal to see the QR code").0, false),
    };
    lines.extend(note);
    (lines, fits)
}
