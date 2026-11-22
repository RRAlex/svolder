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

/// How often each piece of a frame's work ran on this thread: the pane builders and layouts a
/// frame should do once say so here, and a case pins the count a frame costs.
pub(crate) mod work {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    thread_local! {
        static DONE: RefCell<BTreeMap<&'static str, usize>> = const { RefCell::new(BTreeMap::new()) };
    }

    /// Count one run of `what`.
    pub(crate) fn tick(what: &'static str) {
        DONE.with(|done| *done.borrow_mut().entry(what).or_default() += 1);
    }

    /// `work` done without counting it, for a check that redoes what the code under test did.
    pub(crate) fn uncounted<T>(work: impl FnOnce() -> T) -> T {
        let kept = take();
        let done = work();
        DONE.with(|done| *done.borrow_mut() = kept);
        done
    }

    /// Every count since the last call, which starts them again from nothing.
    pub(crate) fn take() -> BTreeMap<&'static str, usize> {
        DONE.with(|done| std::mem::take(&mut *done.borrow_mut()))
    }
}

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
        key_reading: false,
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
        Screen::Device => app.set_selected(1),
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

/// A computer EQ as a daemon's view says it: `points` as (type, Hz, dB, Q), on or off, the
/// most it holds 16.
pub(crate) fn computer_eq(
    on: bool,
    points: &[(&str, f64, f64, f64)],
) -> Box<svolder_host::eq::EqView> {
    Box::new(svolder_host::eq::EqView {
        on,
        points: points
            .iter()
            .enumerate()
            .map(|(at, &(kind, f, g, q))| svolder_host::eq::PointView {
                slot: u8::try_from(at + 1).unwrap_or(u8::MAX),
                kind: kind.to_string(),
                f,
                g,
                q,
            })
            .collect(),
        preamp_db: -6.0,
        stated_preamp_db: 0.0,
        preset: Some("custom".to_string()),
        live: true,
        note: None,
        unavailable: None,
        max: 16,
        running_unknown: false,
        loads: 0,
        crossfeed: None,
        speakers: None,
        loudness: None,
    })
}

/// A computer EQ with two points whose view carries the crossfeed of `model`'s preset `preset`
/// (`None` for the model's default), playing or not, as the controller's own view builds it.
pub(crate) fn computer_eq_with_crossfeed(
    xf_on: bool,
    model: &str,
    preset: Option<&str>,
) -> Box<svolder_host::eq::EqView> {
    use svolder_host::eq::crossfeed::{CrossfeedSettings, CrossfeedView, Model};
    let model = Model::from_word(model).expect("a model word");
    let settings = match preset {
        Some(name) => CrossfeedSettings::from_preset(model, name).expect("a preset"),
        None => CrossfeedSettings::for_model(model),
    };
    let mut eq = computer_eq(true, &[("PK", 1000.0, 0.0, 1.0), ("LSC", 100.0, 1.0, 1.0)]);
    eq.crossfeed = Some(CrossfeedView::of(&settings, xf_on, 48_000, None, None));
    eq
}

/// A computer EQ with crossfeed (off, Bauer's default) whose view carries the loudness the
/// controller builds for `strength_pct`, the reference `reference_pct`, `on` or not, the volume
/// `volume_pct` and the depth `played_d_e` it plays.
pub(crate) fn computer_eq_with_loudness(
    on: bool,
    (strength_pct, reference_pct): (f64, f64),
    volume_pct: Option<f64>,
    played_d_e: f64,
) -> Box<svolder_host::eq::EqView> {
    use svolder_host::eq::loudness::{Level, LoudnessSettings, LoudnessView};
    let settings = LoudnessSettings {
        on,
        strength_pct,
        reference_pct,
    };
    let level = volume_pct.map(|pct| Level {
        linear: (pct / 100.0).powi(3),
        muted: false,
    });
    let mut eq = computer_eq_with_crossfeed(false, "bauer", None);
    eq.loudness = Some(LoudnessView::of(
        &settings, level, played_d_e, 48_000, None, None,
    ));
    eq
}

/// A computer EQ with crossfeed (off, Bauer's default) and virtual speakers as the controller's
/// view carries them: playing or not, `chosen` the id of the chosen set, `fetched` the fetched
/// sets as `(id, room word, label)`. The chosen set is a no room set unless it is listed `with`.
pub(crate) fn computer_eq_with_speakers(
    on: bool,
    chosen: Option<&str>,
    fetched: &[(&str, &str, &str)],
) -> Box<svolder_host::eq::EqView> {
    use svolder_host::eq::speakers::{SetView, SpeakersCurve};
    let mut eq = computer_eq_with_crossfeed(false, "bauer", None);
    let sets: Vec<SetView> = fetched
        .iter()
        .map(|&(id, room, label)| SetView {
            id: id.to_string(),
            room: room.to_string(),
            room_name: if room == "with" {
                "With room"
            } else {
                "No room"
            }
            .to_string(),
            term: if room == "with" { "BRIR" } else { "HRTF" }.to_string(),
            label: label.to_string(),
            licence: "Apache-2.0".to_string(),
            credit: Some("sample credit words".to_string()),
            source_url: "https://sofacoustics.example/data/sadie/".to_string(),
        })
        .collect();
    let mut view = svolder_host::eq::SpeakersView {
        on,
        sets: sets.clone(),
        ..svolder_host::eq::SpeakersView::default()
    };
    if let Some(set) = chosen.and_then(|id| sets.iter().find(|set| set.id == id)) {
        view.set = Some(set.id.clone());
        view.room = Some(set.room.clone());
        view.room_name = Some(set.room_name.clone());
        view.term = Some(set.term.clone());
        view.label = set.label.clone();
        view.licence = set.licence.clone();
        view.credit = set.credit.clone();
        view.source_url = set.source_url.clone();
        view.angles = "left 30.0 deg, right 330.0 deg, 1.5 m".to_string();
        view.taps_ms = 5.3;
        view.loudness_db = -7.2;
        let hz: Vec<f64> = (0..61)
            .map(|i| 20.0 * 10f64.powf(f64::from(i) / 20.0))
            .collect();
        let fall = |from: f64| -> Vec<f64> { (0..61).map(|i| from - f64::from(i) * 0.4).collect() };
        view.curve = SpeakersCurve {
            hz,
            ll_db: fall(-0.5),
            rr_db: fall(-0.5),
            lr_db: fall(-9.0),
            rl_db: fall(-9.0),
        };
    }
    eq.speakers = Some(view);
    eq
}

/// The Target line of a frame: its text, and the modifiers and foreground of the first cell of
/// the device's `name` and of `Computer`, for the tests of which side the line makes stand out.
#[cfg(test)]
pub(crate) fn target_sides(
    buffer: &Buffer,
    name: &str,
) -> (String, (Modifier, Color), (Modifier, Color)) {
    let area = buffer.area;
    for y in 0..area.height {
        let row: Vec<&str> = (0..area.width)
            .map(|x| buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()))
            .collect();
        let line = row.concat();
        if !line.contains("Target ‹ ") {
            continue;
        }
        let at = |word: &str| {
            let byte = line.find(word).expect("a side on the Target line");
            let x = u16::try_from(line[..byte].chars().count()).expect("a column");
            let cell = buffer.cell((x, y)).expect("a cell");
            (cell.modifier, cell.fg)
        };
        return (line.trim_end().to_string(), at(name), at("Computer"));
    }
    panic!("no Target line in the frame");
}

/// What one cell of a drawn frame is styled with: foreground, background and modifiers.
#[cfg(test)]
pub(crate) type CellStyle = (Color, Color, Modifier);

/// The style of the first cell of `word` on the first row that holds `row_has`, for the tests
/// that compare how two rows of a frame draw their words. `word` is looked for from the start of
/// `row_has` in the row.
#[cfg(test)]
pub(crate) fn style_of(buffer: &Buffer, row_has: &str, word: &str) -> CellStyle {
    style_at(buffer, row_has, word, 0)
}

/// The style of the cell `past` cells after the first cell of `word` (see [`style_of`]); with the
/// word's width, the cell right after it.
#[cfg(test)]
pub(crate) fn style_at(buffer: &Buffer, row_has: &str, word: &str, past: usize) -> CellStyle {
    let area = buffer.area;
    for y in 0..area.height {
        let line: String = (0..area.width)
            .map(|x| buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()))
            .collect();
        let Some(start) = line.find(row_has) else {
            continue;
        };
        let rest = line[start..]
            .find(word)
            .unwrap_or_else(|| panic!("{word:?} not on {line:?}"))
            + start;
        let x = u16::try_from(line[..rest].chars().count() + past).expect("a column");
        let cell = buffer.cell((x, y)).expect("a cell");
        return (cell.fg, cell.bg, cell.modifier);
    }
    panic!(
        "no row with {row_has:?} in the frame: {:#?}",
        crate::tui::test_support::text(buffer)
    );
}

/// The styles of a two-sided or many-worded choice drawn on the row holding `row_has`: its
/// chosen word, one other word, its label, the cell right after the label and its `‹`.
#[cfg(test)]
pub(crate) fn choice_styles(
    buffer: &Buffer,
    row_has: &str,
    (label, chosen, other): (&str, &str, &str),
) -> [CellStyle; 5] {
    [
        style_of(buffer, row_has, chosen),
        style_of(buffer, row_has, other),
        style_of(buffer, row_has, label),
        style_at(buffer, row_has, label, label.chars().count()),
        style_of(buffer, row_has, "‹"),
    ]
}

/// A scenes view of `rows` (each a name and its parts: who, present), with `on` lit and the
/// daemon having settled which scene is on or not (`known`).
pub(crate) fn scenes_view(
    rows: &[(&str, &[(&str, bool)])],
    on: Option<&str>,
    known: bool,
) -> svolder_ipc::socket::protocol::ScenesView {
    use svolder_ipc::socket::protocol::{PartRow, SceneRow, ScenesView};
    ScenesView {
        scenes: rows
            .iter()
            .map(|(name, parts)| SceneRow {
                name: (*name).to_string(),
                parts: parts
                    .iter()
                    .map(|(who, present)| PartRow {
                        who: (*who).to_string(),
                        present: *present,
                    })
                    .collect(),
            })
            .collect(),
        on: on.map(str::to_string),
        recalled: None,
        known,
    }
}

/// The three scenes the mockups show: Late night (on), Desk and Movie, which has a part on a
/// device that is not connected.
pub(crate) fn mockup_scenes() -> svolder_ipc::socket::protocol::ScenesView {
    scenes_view(
        &[
            ("Late night", &[("computer", true), ("Desk DAC", true)]),
            ("Desk", &[("computer", true), ("Desk DAC", true)]),
            (
                "Movie",
                &[("computer", true), ("Desk DAC", true), ("Hall", false)],
            ),
        ],
        Some("Late night"),
        true,
    )
}

/// `app` on a daemon that keeps scenes and has sent `view`.
pub(crate) fn offering_scenes(
    mut app: App,
    view: svolder_ipc::socket::protocol::ScenesView,
) -> App {
    app.session.daemon = true;
    app.scenes_served(true);
    app.scenes_view(view);
    app
}
