// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use crate::tui::test_support::{host_hearing_one, text};
use crate::tui::theme::Theme;
use crate::tui::wizard::ShownStep;
use crate::wizard::fake::{FakeEffects, seen, seen_product, walk_to};
use crate::wizard::{Step, Wizard};

use super::*;

/// The step on show after a wizard over `fx` reaches `step`, taking the default answer to
/// every step before it.
async fn showing(fx: FakeEffects, step: Step) -> ShownStep {
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, step).await;
    showing_now(&mut wizard).await
}

/// The wizard showing whatever `wizard` is asking now.
async fn showing_now(wizard: &mut Wizard<FakeEffects>) -> ShownStep {
    let prompt = wizard.prompt().await.expect("a step to show");
    let mut step = ShownStep::opening();
    step.show(prompt);
    step.hands_over_to(wizard.added());
    step
}

/// One frame of `step` in a terminal `width` by `height`.
fn frame(step: &ShownStep, width: u16, height: u16) -> Buffer {
    frame_with_hits(step, width, height).0
}

/// One frame of `step`, and the rows a click can reach on it.
fn frame_with_hits(step: &ShownStep, width: u16, height: u16) -> (Buffer, HitMap) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            draw(f, f.area(), step, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), hits)
}

/// Compare a frame with the rows it drew from the top and the rows it drew at the foot,
/// with every row between the two blank.
fn assert_frame(buffer: &Buffer, top: &[&str], bottom: &[&str]) {
    let rows = text(buffer);
    let blank = " ".repeat(usize::from(buffer.area.width));
    let expected: Vec<String> = top
        .iter()
        .map(|row| (*row).to_string())
        .chain(std::iter::repeat_n(
            blank,
            rows.len() - top.len() - bottom.len(),
        ))
        .chain(bottom.iter().map(|row| (*row).to_string()))
        .collect();
    let differences: Vec<String> = rows
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (drawn, wanted))| drawn != wanted)
        .map(|(y, (drawn, wanted))| {
            format!("row {y}:\n  expected |{wanted}|\n  actual   |{drawn}|")
        })
        .collect();
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// The rule under the header, across an eighty-column screen.
const RULE: &str =
    "────────────────────────────────────────────────────────────────────────────────";

/// The key line on the Passphrase step, whose `←→` cycle a row rather than move along a line.
const FOOTER_STEPPING: &str =
    " Setup  ↑↓ choose ←→ cycle PgUp/PgDn scroll Enter confirm  Esc back  q quit     ";
/// The key line every other step of the wizard draws, on an eighty-column screen.
const FOOTER: &str =
    " Setup  ↑↓ choose ←→ move PgUp/PgDn scroll Enter confirm  Esc back  q quit      ";

/// A host with two rows that warn: one fix svolder can apply itself, and one to type.
fn warning_host() -> FakeEffects {
    FakeEffects::healthy()
        .with_fix(
            "create /home/u/.config/svolder/config.toml from the template",
            true,
        )
        .with_fix_on(
            "bluetoothd plugins",
            "bluetoothd runs with the midi plugin loaded",
            "sudo systemctl daemon-reload && sudo systemctl restart bluetooth",
            false,
        )
}

#[tokio::test]
async fn the_health_step_draws_every_row_and_the_one_way_on() {
    let step = showing(warning_host(), Step::Health).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " How this host looks                                                            ",
            RULE,
            " ✓ Bluetooth service — bluetoothd is running                                    ",
            " ✓ Adapters — hci0 is powered                                                   ",
            " ! Config file — there is no config file yet                                    ",
            "     fix: create /home/u/.config/svolder/config.toml from the template          ",
            " ! bluetoothd plugins — bluetoothd runs with the midi plugin loaded             ",
            "     fix: sudo systemctl daemon-reload && sudo systemctl restart bluetooth      ",
            "                                                                                ",
            " ▶ Continue                                                                     ",
            "   Later: open svolder without a device                                         ",
        ],
        &[FOOTER],
    );
}

/// The fixes step lists what svolder can put right itself — and only that, the one to type
/// having stayed under its own row on the step before — with the cursor on `Skip`, since
/// changing the machine is not what an answer nobody typed asks for.
#[tokio::test]
async fn the_fixes_step_lists_what_it_would_apply_and_rests_on_skip() {
    let step = showing(warning_host(), Step::Fixes).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " What svolder can put right itself                                              ",
            RULE,
            "   create /home/u/.config/svolder/config.toml from the template                 ",
            "                                                                                ",
            "   Apply 1 fix                                                                  ",
            " ▶ Skip                                                                         ",
        ],
        &[FOOTER],
    );
}

#[tokio::test]
async fn a_scan_that_heard_nothing_says_why_and_offers_another() {
    let mut wizard = Wizard::new(FakeEffects::healthy().with_scan(vec![]));
    walk_to(&mut wizard, Step::Scan).await;
    wizard.answer(0, None).await.expect("the scan runs");
    let step = showing_now(&mut wizard).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " Find the device                                                                ",
            RULE,
            " Nothing answered: power the device on and close the phone app, which holds its ",
            " only connection.                                                               ",
            "                                                                                ",
            " ▶ Scan again                                                                   ",
            "   Skip                                                                         ",
        ],
        &[FOOTER],
    );
}

#[tokio::test]
async fn the_pick_step_names_every_device_it_heard() {
    let fx = FakeEffects::healthy().with_scan(vec![
        seen_product("0102030405060708", "Bifrost 3", -47),
        seen("0807060504030201"),
    ]);
    let step = showing(fx, Step::Pick).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " Which device                                                                   ",
            RULE,
            " ▶ Bifrost 3  0102030405060708  -47 dBm  bifrost3                               ",
            "   unknown  0807060504030201  -50 dBm  unknown product — choose a profile       ",
        ],
        &[FOOTER],
    );
}

#[tokio::test]
async fn the_name_step_shows_the_default_in_the_line_it_is_typed_into() {
    let step = showing(host_hearing_one(), Step::Name).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " What to call it                                                                ",
            RULE,
            " Enter takes the name below; type another to use that instead.                  ",
            "                                                                                ",
            " > Bifrost 3                                                                    ",
        ],
        &[FOOTER],
    );
}

#[tokio::test]
async fn the_passphrase_step_is_a_row_that_opens_on_the_code() {
    let mut wizard = Wizard::new(host_hearing_one().refusing_without("1234"));
    walk_to(&mut wizard, Step::Name).await;
    wizard.answer(0, None).await.expect("the name is taken");
    let step = showing_now(&mut wizard).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " The device wants its passphrase                                                ",
            RULE,
            " It turned the connection away, which is what a device with a passphrase does.  ",
            " The four digits the phone app shows, `auto` to have svolder read the code from ",
            " the device, or `none` for a device with no passphrase.                         ",
            "                                                                                ",
            " > ‹ ____ ›                                                                     ",
        ],
        &[FOOTER_STEPPING],
    );
}

#[tokio::test]
async fn the_adapter_step_offers_the_graded_adapters_best_first() {
    let fx = host_hearing_one().with_adapters([("hci0", 2), ("hci1", 5)]);
    let step = showing(fx, Step::Adapter).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " Which adapter to use                                                           ",
            RULE,
            " The one picked here is written into the config as `adapter`.                   ",
            "                                                                                ",
            " ▶ hci1  5/5                                                                    ",
            "   hci0  2/5                                                                    ",
        ],
        &[FOOTER],
    );
}

/// The focused choice on the adapter step is extended by one blank cell in the selected
/// style right after its last character, so the highlight does not stop flush against it;
/// the cell after that one stays plain.
#[tokio::test]
async fn a_focused_choice_gets_one_highlighted_cell_past_its_last_character() {
    let theme = Theme::new(false);
    let selected = theme.style(Role::Selected);
    let fx = host_hearing_one().with_adapters([("hci0", 2), ("hci1", 5)]);
    let step = showing(fx, Step::Adapter).await;
    let buffer = frame(&step, 80, 24);
    // Row 4, " ▶ hci1  5/5", is the choice the step opens on.
    let y = 4;
    let cells: Vec<_> = (0..buffer.area.width)
        .map(|x| buffer.cell((x, y)).expect("a cell").clone())
        .collect();
    let last = cells
        .iter()
        .rposition(|cell| !cell.symbol().trim().is_empty())
        .expect("the row has a last character");
    let after = &cells[last + 1];
    assert_eq!(after.symbol(), " ", "not a blank cell after the choice");
    assert_eq!(
        Some(after.fg),
        selected.fg,
        "the cell right after the choice is not selected"
    );
    assert!(
        after.modifier.contains(selected.add_modifier),
        "the cell right after the choice is not selected"
    );
    let next = &cells[last + 2];
    assert_ne!(
        Some(next.fg),
        selected.fg,
        "the highlight ran two cells past the choice"
    );
}

#[tokio::test]
async fn the_daemon_step_offers_the_install_and_the_command_it_runs() {
    let step = showing(host_hearing_one(), Step::Daemon).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " The background service                                                         ",
            RULE,
            " The daemon holds each device's link, so a command need not connect first.      ",
            "   svolder daemon install                                                       ",
            "                                                                                ",
            " ▶ Install the user service                                                     ",
            "   Skip                                                                         ",
        ],
        &[FOOTER],
    );
}

#[tokio::test]
async fn the_done_step_says_what_was_written() {
    let step = showing(host_hearing_one(), Step::Done).await;
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " Done                                                                           ",
            RULE,
            " Added Bifrost 3 (0102030405060708).                                            ",
            " The daemon:                                                                    ",
            "   installed the systemd user units                                             ",
            " Enter opens Bifrost 3.                                                         ",
            "                                                                                ",
            " ▶ Finish                                                                       ",
        ],
        &[FOOTER],
    );
}

/// A screen wide enough for the small wordmark carries it, and the step's title sits beside
/// it on the mark's own second row, which is where every other screen puts its header text.
#[tokio::test]
async fn a_wide_screen_carries_the_wordmark_beside_the_title() {
    let step = showing(host_hearing_one(), Step::Name).await;
    let rows = text(&frame(&step, 100, 24));
    assert!(
        rows[0].contains('█'),
        "the mark is not on the first row: {:?}",
        rows[0]
    );
    assert!(
        rows[1].contains("What to call it"),
        "the title is not beside the mark: {:?}",
        rows[1]
    );
    assert!(rows[2].starts_with('─'), "{:?}", rows[2]);
}

/// What a refused answer said stands on the row above the key line, so the keys that answer
/// the step again are still on the screen beside it.
#[tokio::test]
async fn a_refused_answer_stands_above_the_keys_that_ask_the_step_again() {
    let mut step = showing(host_hearing_one(), Step::Name).await;
    step.notice = Some("a device named \"dac\" is already configured. See `svolder --help`".into());
    let rows = text(&frame(&step, 80, 24));
    assert_eq!(
        rows[22],
        " a device named \"dac\" is already configured. See `svolder --help`               ",
    );
    assert_eq!(rows[23], FOOTER, "the keys are still on the screen");
}

/// What the wizard is spending its seconds on is named under the step it is spending them
/// from, rather than on a frame that says nothing at all.
#[tokio::test]
async fn what_is_being_waited_for_is_named_under_the_step() {
    let mut step = showing(FakeEffects::healthy().with_scan(vec![]), Step::Scan).await;
    step.wait_for(Some("listening…".into()));
    assert_frame(
        &frame(&step, 80, 24),
        &[
            " Find the device                                                                ",
            RULE,
            " Listening takes up to 15 s.                                                    ",
            "                                                                                ",
            " ▶ Scan                                                                         ",
            "                                                                                ",
            " ⠋ listening…                                                                   ",
        ],
        &[FOOTER],
    );
}

/// A step with more to say than the screen has room for says how many rows are not shown,
/// and keeps what answers it on the screen: a step nobody can answer is no step at all.
#[tokio::test]
async fn a_step_that_says_more_than_the_screen_holds_keeps_the_answer_on_it() {
    let mut fx = FakeEffects::healthy();
    for _ in 0..12 {
        fx = fx.with_fix("sudo systemctl enable --now bluetooth", false);
    }
    let step = showing(fx, Step::Health).await;
    let rows = text(&frame(&step, 80, 24));
    assert_eq!(
        rows[19],
        " … 9 more lines                                                                 "
    );
    assert_eq!(
        rows[21],
        " ▶ Continue                                                                     "
    );
    assert_eq!(
        rows[22],
        "   Later: open svolder without a device                                         "
    );
    assert_eq!(rows[23], FOOTER);
}

/// The terminal's own cursor sits where the next letter typed would go, so a step that
/// takes a line looks like the field it is.
#[tokio::test]
async fn the_cursor_sits_where_the_next_letter_would_be_typed() {
    let mut step = showing(host_hearing_one(), Step::Name).await;
    step.typed = "dac".into();
    step.caret = 2;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            draw(f, f.area(), &step, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    assert_eq!(
        terminal
            .get_cursor_position()
            .expect("the terminal answers where its cursor is"),
        Position { x: 5, y: 4 },
        "the cursor is not in the field: |{}|",
        text(terminal.backend().buffer())[4]
    );
}

/// A list of choices too long for the screen is a window that follows the cursor, with the
/// number of choices past each end on the row above and below it.
#[tokio::test]
async fn a_long_choice_list_is_a_window_that_follows_the_cursor() {
    let fx = FakeEffects::healthy().with_scan(vec![seen("0807060504030201")]);
    let mut wizard = Wizard::new(fx);
    walk_to(&mut wizard, Step::Pick).await;
    wizard.answer(0, None).await.expect("the device is taken");
    let mut step = showing_now(&mut wizard).await;
    let offered = step.prompt.choices.len();
    assert!(
        offered > 20,
        "this case wants more profiles than a screen holds: {offered}"
    );

    let rows = text(&frame(&step, 80, 24));
    assert!(
        rows.iter().any(|row| row.starts_with(" ▶ ")),
        "the cursor is not on the screen: {rows:?}"
    );
    let below = rows
        .iter()
        .find(|row| row.contains("↓ "))
        .expect("the row that says how many choices are below");
    assert!(below.contains("more"), "|{below}|");
    assert!(
        !rows.iter().any(|row| row.contains("↑ ")),
        "nothing is above the first choice: {rows:?}"
    );

    // The cursor on the last choice carries the window with it: the row it is on is drawn,
    // and what is above it is counted.
    step.choice = offered - 1;
    let rows = text(&frame(&step, 80, 24));
    let cursor = rows
        .iter()
        .find(|row| row.starts_with(" ▶ "))
        .expect("the cursor is on the screen");
    assert!(
        cursor.contains(&step.prompt.choices[offered - 1]),
        "the cursor is not on the last choice: |{cursor}|"
    );
    assert!(
        rows.iter().any(|row| row.contains("↑ ")),
        "the choices above the window are not counted: {rows:?}"
    );
}

/// The step's own words are carried on by the scroll, and the last row says how many are
/// still below.
#[tokio::test]
async fn a_scrolled_body_starts_where_the_scroll_leaves_it() {
    let mut fx = FakeEffects::healthy();
    for at in 0..12 {
        fx = fx.with_fix(&format!("sudo fix number {at}"), false);
    }
    let mut step = showing(fx, Step::Health).await;
    let first = text(&frame(&step, 80, 24));
    assert_eq!(
        first[2],
        " ✓ Bluetooth service — bluetoothd is running                                    ",
        "the first frame does not start at the first row"
    );
    assert!(
        first[19].starts_with(" … ") && first[19].contains("more lines"),
        "|{}|",
        first[19]
    );

    step.scroll = 19;
    let carried = text(&frame(&step, 80, 24));
    assert_eq!(
        carried[2],
        "     fix: sudo fix number 8                                                     ",
        "the frame did not start where the scroll left it"
    );
    assert!(
        carried.iter().any(|row| row.contains("fix number 11")),
        "the last row is not on the screen: {carried:?}"
    );
    assert!(
        carried[21]
            == " ▶ Continue                                                                     ",
        "what answers the step moved: |{}|",
        carried[21]
    );
}

/// Every choice is a row a click takes, and the keys of the line below are clickable the
/// way they are on every other screen.
#[tokio::test]
async fn every_choice_is_a_row_a_click_can_take() {
    let step = showing(host_hearing_one(), Step::Daemon).await;
    let (_, hits) = frame_with_hits(&step, 80, 24);
    assert_eq!(hits.hit_at(4, 5), Some(&Hit::ListRow(0)), "the install row");
    assert_eq!(hits.hit_at(4, 6), Some(&Hit::ListRow(1)), "the skip row");
    assert!(
        matches!(hits.hit_at(8, 23), Some(Hit::Footer(0))),
        "the first key of the line is not clickable"
    );
}

/// The keys that are not worth pressing on the step being shown are dimmed rather than
/// dropped: on a step that takes a typed line `q` is a letter of it, and the arrows move
/// along the line rather than between choices.
#[tokio::test]
async fn a_step_that_takes_a_line_dims_the_keys_that_are_not_for_it() {
    let faint = Theme::new(false).style(Role::Faint).fg;
    let typing = frame(&showing(host_hearing_one(), Step::Name).await, 80, 24);
    let choosing = frame(&showing(host_hearing_one(), Step::Daemon).await, 80, 24);
    // Column 8 opens `↑↓ choose`, and column 68 is the `q` of `q quit`.
    for (buffer, column, dimmed) in [
        (&typing, 8, true),
        (&typing, 68, true),
        (&choosing, 8, false),
        (&choosing, 68, false),
    ] {
        let cell = buffer.cell((column, 23)).expect("a cell on the key line");
        assert_eq!(
            cell.fg == faint.expect("the faint role carries a colour"),
            dimmed,
            "column {column} ({}) of |{}|",
            cell.symbol(),
            text(buffer)[23]
        );
    }
    assert_eq!(
        text(&typing)[23],
        text(&choosing)[23],
        "the keys themselves are the same either way"
    );
}

/// With the theme background on at true colour, a blank cell of the wizard has the ground
/// behind it, so the wizard looks as dark on a light terminal as the screens do.
#[tokio::test]
async fn the_wizard_paints_the_ground() {
    use crate::tui::theme::Surface;
    use svolder_ipc::ColorDepth;

    let step = showing(warning_host(), Step::Health).await;
    let theme = crate::tui::test_support::theme_for("amber", ColorDepth::TrueColor);
    let ground = theme.surface(Surface::Ground).expect("a ground is painted");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            draw(f, f.area(), &step, &theme, &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    let blank = buffer.cell((79, 10)).expect("a cell");
    assert_eq!(blank.symbol(), " ");
    assert_eq!(blank.bg, ground);
    let unpainted = (0..24)
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .filter(|at| {
            buffer
                .cell(*at)
                .is_some_and(|cell| cell.bg == ratatui::style::Color::Reset)
        })
        .count();
    assert_eq!(unpainted, 0, "every cell has a background");
}
