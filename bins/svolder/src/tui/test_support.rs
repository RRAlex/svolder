// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers several of the TUI's own test modules share: reading a rendered buffer back as
//! text, asserting it against a row-by-row golden, building the `ValueObject` a backend hands
//! over for one reading, and the small pieces of input and setup a case scripts (a key press,
//! a loaded profile, a drawn frame). It also holds the apps a test starts from, the readings it
//! feeds them, the drawing and theme helpers, and the assertions several widgets' tests share.
//!
//! - `presets`: the fixtures of the preset box's tests and of the screens that open it.

pub(super) mod presets;

use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use serde_json::{Value, json};
use svolder_ipc::{ColorDepth, DeviceConfig, TuiConfig, ValueObject};
use svolder_profiles::{EqSpec, Profile, Registry};

use crate::backend::{DeviceRow, Update};
use crate::tui::app::{App, Intent, Screen};
use crate::tui::decoration::effects::Effects;
use crate::tui::decoration::mascot::{Code, View};
use crate::tui::mouse::HitMap;
use crate::tui::render;
use crate::tui::terminal::{Claim, Guard};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::surfaces::fade;
use crate::wizard::fake::{FakeEffects, seen_product};

/// Every cell of `buffer`, one string per row.
pub(crate) fn text(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .expect("a cell inside the buffer")
                        .symbol()
                })
                .collect()
        })
        .collect()
}

/// Assert that `buffer` reads as `expected`, one row per string, reporting every row that
/// differs rather than stopping at the first.
pub(crate) fn assert_rows(buffer: &Buffer, expected: &[&str]) {
    let actual = text(buffer);
    assert_eq!(actual.len(), expected.len(), "row count");
    let differences: Vec<String> = actual
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (drawn, wanted))| drawn.as_str() != **wanted)
        .map(|(y, (drawn, wanted))| {
            format!("row {y}:\n  expected |{wanted}|\n  actual   |{drawn}|")
        })
        .collect();
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// Assert that docs/usage/tui.md carries `rows` as a fenced block, byte for byte: the check that a
/// mockup in the manual is the frame this build draws.
pub(crate) fn assert_usage_md_shows(rows: &[String], what: &str) {
    let doc = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage/tui.md");
    let doc = std::fs::read_to_string(&doc).expect("docs/usage/tui.md reads");
    let block = rows.join("\n");
    assert!(
        doc.contains(&format!("```\n{block}\n```")),
        "docs/usage/tui.md's {what} does not read what this build draws"
    );
}

/// One value reading as a backend hands it over.
pub(crate) fn vo(id: &str, key: Option<&str>, raw: Value, display: &str) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: key.map(str::to_string),
        label: key.unwrap_or("?").to_string(),
        value_type: if raw.is_boolean() { "bool" } else { "u8" }.into(),
        raw,
        display: display.into(),
        value: None,
        unit: None,
    }
}

/// One reading as a backend hands it over, with no key of its own.
pub(crate) fn plain_vo(id: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "?".into(),
        value_type: "u8".into(),
        raw: Value::from(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

/// Report every reading at one moment.
pub(crate) fn hear(app: &mut App, values: Vec<ValueObject>) {
    let now = Instant::now();
    for value in values {
        app.apply(Update::Status(value), now);
    }
}

/// Report one reading, with no key of its own.
pub(crate) fn hear_one(app: &mut App, id: &str, raw: i64) {
    app.apply(Update::Status(plain_vo(id, raw)), Instant::now());
}

/// The built-in Gungnir 2 profile: five bands at 40, 160, 632, 2515 and 10000 Hz, gains of
/// -20 to +6 dB with 98 at 0 dB, and Q 0.5 to 8.0 with 64 at 4.30.
pub(crate) fn profile() -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2 is built in")
        .clone()
}

/// The built-in profile `name`.
pub(crate) fn profile_named(name: &str) -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .unwrap_or_else(|| panic!("{name} is built in"))
        .clone()
}

/// The Aegir 2's meter scale.
pub(crate) const AEGIR_SCALE: (i64, i64, f64) = (160, 350, 0.25);

/// `profile` with every meter given the logarithmic scale `(zero, full, class_a)`.
pub(crate) fn with_log_scale(
    mut profile: Profile,
    (zero, full, class_a): (i64, i64, f64),
) -> Profile {
    for control in &mut profile.controls {
        if control.kind == svolder_profiles::ControlKind::Meter {
            control.range = None;
            control.floor = None;
            control.tick = None;
            control.log_scale = Some(svolder_profiles::LogScale {
                zero: svolder_profiles::Zero::Fixed(zero),
                full,
                class_a,
            });
        }
    }
    profile
}

/// The built-in `tyr` with its meter reading from its floor readout: zero from `meter_floor`,
/// full scale 1000, a class-A block of a tenth.
pub(crate) fn tyr_following_its_floor() -> Profile {
    let mut profile = profile_named("tyr");
    for control in &mut profile.controls {
        if control.key == "meter_1" {
            control.log_scale = Some(svolder_profiles::LogScale {
                zero: svolder_profiles::Zero::Floor("meter_floor".into()),
                full: 1000,
                class_a: 0.10,
            });
        }
    }
    profile
}

/// `profile` with every meter stripped of its scale: no `range`, `floor`, `tick` or `scale`, as
/// a user's profile may leave a meter.
pub(crate) fn without_scale(mut profile: Profile) -> Profile {
    for control in &mut profile.controls {
        if control.kind == svolder_profiles::ControlKind::Meter {
            control.range = None;
            control.floor = None;
            control.tick = None;
            control.log_scale = None;
        }
    }
    profile
}

/// `profile` with every meter given the linear scale `range`, one of the two scales that give
/// a meter a bar and a level to carry.
pub(crate) fn with_meter_range(mut profile: Profile, range: (i64, i64)) -> Profile {
    for control in &mut profile.controls {
        if control.kind == svolder_profiles::ControlKind::Meter {
            control.range = Some(range);
            control.log_scale = None;
        }
    }
    profile
}

/// `profile` with only the control keyed `key` given the scale `range`.
pub(crate) fn with_range_on(mut profile: Profile, key: &str, range: (i64, i64)) -> Profile {
    for control in &mut profile.controls {
        if control.key == key {
            control.range = Some(range);
            control.log_scale = None;
        }
    }
    profile
}

/// One meter reading as a backend hands it over: `raw` on function `id`, as a `u16`.
pub(crate) fn meter_reading(id: &str, raw: i64) -> ValueObject {
    ValueObject {
        value_type: "u16".into(),
        ..vo(id, None, serde_json::json!(raw), &raw.to_string())
    }
}

/// `profile` with the over-current fault of the amps, for the cases that need a fault to
/// show: no DAC profile has one.
pub(crate) fn with_overcurrent(mut profile: Profile) -> Profile {
    profile.controls.push(svolder_profiles::Control {
        id: "230.1".parse().expect("a function id"),
        key: "fault_overcurrent".into(),
        label: "Over-current".into(),
        kind: svolder_profiles::ControlKind::Fault,
        value_type: svolder_proto::ValueType::Bool,
        values: Vec::new(),
        range: None,
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: Some(
            "Power-cycle the device. Check for shorts in speakers or headphones if it continues."
                .into(),
        ),
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    });
    profile
}

/// The Gungnir 2's tone control, to convert the steps a case expects.
pub(crate) fn eq() -> EqSpec {
    profile().eq.expect("the Gungnir 2 has a tone control")
}

/// Press one key with no modifiers.
pub(crate) fn press(app: &mut App, code: KeyCode) -> Vec<Intent> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Press one key, built the way [`KeyEvent::from`] builds it: the same event `press` sends,
/// spelled the other way its own callers already used.
pub(crate) fn press_from(app: &mut App, code: KeyCode) -> Vec<Intent> {
    app.key(KeyEvent::from(code))
}

/// Type every character of `text`, pressed one at a time.
pub(crate) fn type_text(app: &mut App, text: &str) {
    for letter in text.chars() {
        press_from(app, KeyCode::Char(letter));
    }
}

/// The foreground one cell of `buffer` carries.
pub(in crate::tui) fn fg(buffer: &Buffer, x: u16, y: u16) -> Color {
    buffer.cell((x, y)).expect("a cell inside the buffer").fg
}

/// One frame of `app` in a terminal `width` by `height`, drawn with `theme` rather than the
/// default 16-colour one, for the cases that check what a colour depth or accent draws.
pub(crate) fn frame_with_theme(app: &App, width: u16, height: u16, theme: &Theme) -> Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, app, theme, &mut hits);
        })
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// One frame of `app` in a terminal `width` by `height`, drawn with the default 16-colour
/// theme.
pub(crate) fn frame(app: &App, width: u16, height: u16) -> Buffer {
    frame_with_theme(app, width, height, &Theme::new(false))
}

/// A span of `n` milliseconds.
pub(super) fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// `palette` at `depth`, with every other `[tui]` setting at its default.
pub(crate) fn theme_for(palette: &str, depth: ColorDepth) -> Theme {
    let config = TuiConfig {
        palette: palette.into(),
        ..TuiConfig::default()
    };
    Theme::from_config(&config, false, depth)
}

/// How long the device takes to answer in a case that lets it.
pub(super) const ANSWERS_IN: Duration = Duration::from_secs(2);

/// One key press as a terminal reports it.
pub(super) fn key_press(code: KeyCode, modifiers: KeyModifiers) -> std::io::Result<Event> {
    Ok(Event::Key(KeyEvent::new(code, modifiers)))
}

/// A claim over a terminal that is, or is not, there, and that cannot be entered.
pub(super) struct Refusing {
    tty: bool,
    pub(super) calls: std::cell::Cell<u32>,
}

impl Refusing {
    pub(super) fn new(tty: bool) -> Refusing {
        Refusing {
            tty,
            calls: std::cell::Cell::new(0),
        }
    }
}

impl Claim for Refusing {
    fn is_terminal(&self) -> bool {
        self.tty
    }

    fn enter(&self, _mouse: bool) -> std::io::Result<Guard> {
        self.calls.set(self.calls.get() + 1);
        Err(std::io::Error::other("no tty"))
    }
}

/// A Gungnir 2 (which has an EQ) among two configured devices, showing `screen`.
///
/// The cursor is put somewhere every one of the screen's own hints can act from: a toggle
/// row on the device screen, since its first row is a select that only the arrows step
/// through; a band with a frequency, a gain and a Q already read, on the tone screen; and
/// two readings no control names, on the telemetry screen, so its own arrows have rows to
/// move through and a raw value to nudge.
pub(super) fn app_showing(screen: Screen) -> App {
    app_showing_on("gungnir2-v2", screen)
}

/// [`app_showing`] on the built-in profile `profile` instead of the Gungnir 2's.
pub(super) fn app_showing_on(profile: &str, screen: Screen) -> App {
    let devices = vec![
        DeviceRow::for_test("gungnir"),
        DeviceRow::for_test("bifrost"),
    ];
    let mut app = App::new(
        profile_named(profile),
        "gungnir".into(),
        &TuiConfig::default(),
        devices,
    );
    // The device list edits what the configuration file says, so an app showing it knows
    // what the file says about the devices its rows name.
    app.devices.configured = vec![
        configured("gungnir", "0102030405060708"),
        configured("bifrost", "1112131415161718"),
    ];
    app.show(screen);
    match screen {
        Screen::Device => app.selected = 1,
        Screen::Tone => hear_band_zero(&mut app),
        Screen::Telemetry => hear_two_raws(&mut app),
        Screen::Devices | Screen::Settings | Screen::Setup => {}
    }
    app
}

/// One device as the configuration file has it.
pub(super) fn configured(name: &str, udid: &str) -> DeviceConfig {
    DeviceConfig {
        name: name.into(),
        udid: udid.into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: std::collections::BTreeMap::new(),
    }
}

/// One reading no control names, as a backend hands it over.
pub(super) fn raw_reading(id: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "?".into(),
        value_type: "u8".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

/// Band zero's frequency, gain and Q, so the tone screen's arrows and brackets have a
/// reading to move: the built-in bands centre at 40 Hz, 98 is 0 dB and 64 is a Q of 4.3.
fn hear_band_zero(app: &mut App) {
    let now = Instant::now();
    for (id, raw) in [("12.1.2", 64), ("12.1.3", 98), ("12.1.1", 64)] {
        app.apply(Update::Status(raw_reading(id, raw)), now);
    }
}

/// Two readings no control names, so the telemetry screen has rows to move through and a
/// raw value to nudge.
fn hear_two_raws(app: &mut App) {
    let now = Instant::now();
    for id in ["7.1", "7.2"] {
        app.apply(Update::Status(raw_reading(id, 0)), now);
    }
}

/// The two themes a modal box is checked at: true colour, where the greys are colours of
/// the preset's own, and the 16 ANSI colours, where the dim role is the dim attribute.
pub(super) fn modal_themes() -> [(&'static str, Theme); 2] {
    [
        (
            "true colour",
            Theme::from_config(
                &svolder_ipc::TuiConfig::default(),
                false,
                ColorDepth::TrueColor,
            ),
        ),
        ("16 colours", Theme::new(false)),
    ]
}

/// The cells inside `area` that draw a glyph in one of the theme's two greys, each with
/// the grey it is: `"dim"` for [`Role::Dim`] — the colour or the attribute everything
/// around a modal box is dimmed to — and `"faint"` for [`Role::Faint`].
pub(super) fn greyed(buffer: &Buffer, area: Rect, theme: &Theme) -> Vec<(u16, u16, &'static str)> {
    let dim = theme.style(Role::Dim);
    let faint = theme.style(Role::Faint).fg;
    let mut greyed = Vec::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            if cell.symbol().trim().is_empty() {
                continue;
            }
            let is_dim = match dim.fg {
                Some(grey) => cell.fg == grey,
                None => cell.modifier.contains(Modifier::DIM),
            };
            if is_dim {
                greyed.push((x, y, "dim"));
            } else if faint.is_some_and(|grey| cell.fg == grey) {
                greyed.push((x, y, "faint"));
            }
        }
    }
    greyed
}

/// Assert that the box at `area` is drawn as a focused pane is: every cell of its border
/// in the accent rule's colour, the first cell of its title in the selected role, and no
/// glyph inside it in the dim grey.
pub(super) fn assert_drawn_in_focus(buffer: &Buffer, area: Rect, theme: &Theme, what: &str) {
    let rule = theme.style(Role::Rule).fg;
    for x in area.left()..area.right() {
        let bottom = buffer.cell((x, area.bottom() - 1)).expect("a border cell");
        if matches!(bottom.symbol(), "─" | "└" | "┘") {
            assert_eq!(Some(bottom.fg), rule, "{what}: bottom border at column {x}");
        }
    }
    for y in area.top()..area.bottom() {
        for x in [area.left(), area.right() - 1] {
            let side = buffer.cell((x, y)).expect("a border cell");
            assert_eq!(Some(side.fg), rule, "{what}: side border at ({x}, {y})");
        }
    }
    let title = buffer
        .cell((area.x + 1, area.y))
        .expect("the title's first cell");
    let selected = theme.style(Role::Selected);
    assert_eq!(
        (
            Some(title.fg),
            title.modifier.contains(selected.add_modifier)
        ),
        (selected.fg, true),
        "{what}: the title is not drawn as a focused pane's"
    );
    let dim: Vec<_> = greyed(buffer, area, theme)
        .into_iter()
        .filter(|(_, _, grey)| *grey == "dim")
        .collect();
    assert!(dim.is_empty(), "{what}: dim cells inside the box: {dim:?}");
}

/// The 16-colour theme the line cases draw with, where the depth is not what they check.
pub(super) fn sixteen_colour() -> Theme {
    Theme::new(false)
}

/// Assert that `faded` is `plain` with the rows `rows` (each a line and its distance from the
/// selected row) faded by [`fade`]: columns `columns` of every glyph, the selected row and the two
/// either side untouched, a row six away blended by 0.165 and one ten or more away by 0.33, and
/// every other cell of the buffer as it was.
pub(super) fn assert_list_faded(
    plain: &Buffer,
    faded: &Buffer,
    rows: &[(u16, usize)],
    columns: std::ops::Range<u16>,
    theme: &Theme,
) {
    let mut seen = [false; 3];
    for y in 0..plain.area.height {
        for x in 0..plain.area.width {
            let (before, after) = (&plain[(x, y)], &faded[(x, y)]);
            assert_eq!(before.symbol(), after.symbol(), "({x}, {y})");
            assert_eq!(before.bg, after.bg, "({x}, {y})");
            let faded_row = rows.iter().find(|(row, _)| *row == y);
            let expected = match faded_row {
                Some((_, distance))
                    if columns.contains(&x) && !before.symbol().trim().is_empty() =>
                {
                    let own = match before.fg {
                        Color::Reset => Style::default(),
                        fg => Style::default().fg(fg),
                    };
                    fade(own, *distance, theme).fg.unwrap_or(before.fg)
                }
                _ => before.fg,
            };
            assert_eq!(after.fg, expected, "({x}, {y})");
            if let Some((_, distance)) = faded_row
                && columns.contains(&x)
                && !before.symbol().trim().is_empty()
            {
                match distance {
                    0..=2 => {
                        assert_eq!(after.fg, before.fg, "({x}, {y}) is within two");
                        seen[0] = true;
                    }
                    6 => seen[1] |= after.fg != before.fg,
                    10.. => seen[2] |= after.fg != before.fg,
                    _ => {}
                }
            }
        }
    }
    assert_eq!(
        seen, [true; 3],
        "the rows cover distances 0, and 6 and 10+ that blend"
    );
}

/// A Device screen app whose squirrel is called and past its arrival, at rest.
pub(super) fn called_app() -> App {
    let mut app = app_showing(Screen::Device);
    app.mascot.capable = true;
    app.mascot.palette = "amber".into();
    let t0 = Instant::now();
    app.mascot.code(Code::Konami, t0);
    app.mascot.observe(&View::bare(), t0 + ms(10_000));
    app
}

/// The row index of one control key, the same order [`App::rows`] lists them in.
pub(super) fn row_of(app: &App, key: &str) -> usize {
    app.rows()
        .iter()
        .position(|r| r.control.key == key)
        .unwrap_or_else(|| panic!("a {key} row"))
}

/// A mouse event of `kind` at `(x, y)`.
pub(super) fn mouse_at(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// One left click at `(x, y)`.
pub(super) fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// One frame of `app` in a terminal `width` by `height`: the cells it drew and the hit
/// map it filled.
pub(super) fn drawn(app: &App, width: u16, height: u16) -> (Buffer, HitMap) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), hits)
}

/// Run the animations `app`'s own settings ask for, the way the event loop does on a
/// terminal that can show them.
///
/// An app built here has not been through [`App::set_look`], and nothing animates until it
/// has: a configuration says which animations are wanted, and only the layer says whether
/// the terminal can show any of them.
pub(super) fn animated(app: &mut App) {
    let tui = app.tui.clone();
    app.set_look(&tui, &Effects::new(true, &tui.effects));
}

/// Report one reading at `now`.
pub(super) fn feed(app: &mut App, value: ValueObject, now: Instant) {
    app.apply(Update::Status(value), now);
}

/// The device answering every write asked of it so far with the step it asked for.
pub(super) fn answer(app: &mut App) {
    let asked: Vec<(String, u8)> = app
        .readings
        .pending
        .iter()
        .map(|(id, pending)| (id.to_string(), pending.step))
        .collect();
    for (id, step) in asked {
        hear_one(app, &id, i64::from(step));
    }
}

/// The theme at true colour on the default palette.
pub(super) fn true_colour() -> Theme {
    Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor)
}

/// The theme at true colour on the default palette, drawing its background or not.
pub(super) fn true_colour_background(background: bool) -> Theme {
    let config = TuiConfig {
        theme_background: background,
        ..TuiConfig::default()
    };
    Theme::from_config(&config, false, ColorDepth::TrueColor)
}

/// A host with one Bifrost 3 on the air and every check passing.
pub(super) fn host_hearing_one() -> FakeEffects {
    FakeEffects::healthy().with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -47)])
}
