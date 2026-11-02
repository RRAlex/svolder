// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use crossterm::event::{KeyModifiers, MouseEvent};
use futures::stream::BoxStream;
use ratatui::backend::TestBackend;
use tokio::time::Instant;

use crate::tui::test_support::{Refusing, host_hearing_one};
use crate::wizard::fake::{FakeEffects, seen, seen_product, walk_to};
use crate::wizard::{Given, Step};

use super::*;

/// How long a scan takes on the host a case about waiting runs on.
const SCAN_TAKES: Duration = Duration::from_secs(5);

/// How long writing the device takes on the host a case about a write runs on.
const WRITE_TAKES: Duration = Duration::from_secs(2);

/// One key press as a terminal reports it.
fn key(code: KeyCode) -> io::Result<Event> {
    Ok(Event::Key(KeyEvent::from(code)))
}

/// `text` typed one key at a time.
fn typing(text: &str) -> Vec<io::Result<Event>> {
    text.chars().map(|c| key(KeyCode::Char(c))).collect()
}

/// Ctrl-C, which the terminal reports as a key while it is in raw mode.
fn ctrl_c() -> io::Result<Event> {
    Ok(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )))
}

/// One left click at `(column, row)`, as a terminal reports it.
fn click(column: u16, row: u16) -> io::Result<Event> {
    Ok(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
}

/// How long a case leaves between one key and the next: a person reads the screen a key
/// drew before pressing the next one, and with the clock paused it costs nothing.
const BETWEEN: Duration = Duration::from_millis(10);

/// Run the wizard over `wizard` with `script` as the terminal's input, and answer with how it
/// ended and the last frame it drew.
///
/// The keys arrive a moment apart, the way a person presses them; the script is all the
/// input there is, so a run that asks for more ends rather than waiting on a terminal no
/// case is going to type into.
async fn walked(
    wizard: &mut Wizard<FakeEffects>,
    script: Vec<io::Result<Event>>,
) -> (WizardEnd, Vec<String>) {
    let (end, buffer) = walked_buffer(wizard, script).await;
    let rows = (0..buffer.area.height)
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
        .collect();
    (end, rows)
}

/// [`walked`], answering with the last frame's cells rather than its text.
async fn walked_buffer(
    wizard: &mut Wizard<FakeEffects>,
    script: Vec<io::Result<Event>>,
) -> (WizardEnd, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut events = pressed(script);
    let end = event_loop(
        &mut terminal,
        &mut events,
        wizard,
        &Theme::new(false),
        Leaving::Nowhere,
    )
    .await;
    (end, terminal.backend().buffer().clone())
}

/// `script` as a terminal reports it: one event every [`BETWEEN`], and nothing after the
/// last of them.
fn pressed(script: Vec<io::Result<Event>>) -> BoxStream<'static, io::Result<Event>> {
    futures::stream::unfold(script.into_iter(), |mut left| async move {
        tokio::time::sleep(BETWEEN).await;
        left.next().map(|event| (event, left))
    })
    .boxed()
}

/// The device a run that added one ended with.
fn added(end: &WizardEnd) -> Option<&DeviceConfig> {
    match end {
        WizardEnd::Added(device) => Some(device),
        _ => None,
    }
}

/// `Enter` takes the choice the cursor is on, and the arrows are what move it: the same
/// walk twice, once with the cursor left where the step put it and once moved one row on.
#[tokio::test(start_paused = true)]
async fn the_cursor_moves_between_the_choices_and_enter_takes_the_one_it_lands_on() {
    let installing = host_hearing_one();
    let mut wizard = Wizard::new(installing.clone());
    walk_to(&mut wizard, Step::Daemon).await;
    let (end, _) = walked(&mut wizard, vec![key(KeyCode::Enter), key(KeyCode::Enter)]).await;
    assert!(added(&end).is_some(), "the run did not end on the device");
    assert!(installing.daemon_installed());

    let skipping = host_hearing_one();
    let mut wizard = Wizard::new(skipping.clone());
    walk_to(&mut wizard, Step::Daemon).await;
    let (end, _) = walked(
        &mut wizard,
        vec![key(KeyCode::Down), key(KeyCode::Enter), key(KeyCode::Enter)],
    )
    .await;
    assert!(added(&end).is_some());
    assert!(!skipping.daemon_installed(), "the skipped step ran anyway");
}

/// `Esc` is the step before this one: the device chosen on the step it went back from is
/// not the one the run ends up writing.
#[tokio::test(start_paused = true)]
async fn esc_goes_back_a_step_and_what_it_went_back_over_is_answered_afresh() {
    let fx = FakeEffects::healthy().with_scan(vec![
        seen_product("0102030405060708", "Bifrost 3", -47),
        seen_product("0807060504030201", "Gungnir 2", -60),
    ]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Pick).await;
    let (end, _) = walked(
        &mut wizard,
        vec![
            key(KeyCode::Down),  // the second device
            key(KeyCode::Enter), // which asks what to call it
            key(KeyCode::Esc),   // and back to the list
            key(KeyCode::Enter), // the first device this time
            key(KeyCode::Enter), // the name it advertises
            key(KeyCode::Enter), // the daemon
            key(KeyCode::Enter), // done
        ],
    )
    .await;
    assert_eq!(
        added(&end).map(|device| device.name.clone()),
        Some("Bifrost 3".to_string())
    );
    assert_eq!(
        fx.written_device().expect("a device was written").udid,
        "0102030405060708"
    );
}

/// A step that takes a line takes the letters as they are typed, `Backspace` takes one back,
/// and `Enter` answers with what is left — `q` among the letters, where it is a letter
/// rather than the way out.
#[tokio::test(start_paused = true)]
async fn typing_names_the_device_and_backspace_takes_a_letter_back() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let mut script = typing("quax");
    script.push(key(KeyCode::Backspace));
    script.push(key(KeyCode::Enter));
    script.push(key(KeyCode::Enter)); // the daemon
    script.push(key(KeyCode::Enter)); // done
    let (end, _) = walked(&mut wizard, script).await;
    assert_eq!(
        added(&end).map(|device| device.name.clone()),
        Some("qua".to_string())
    );
}

/// `q` leaves, and a walk that was left half-way has written nothing it had not already
/// written: the step it was on is the step it was leaving.
#[tokio::test(start_paused = true)]
async fn q_leaves_the_wizard_with_nothing_written() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    let (end, _) = walked(&mut wizard, vec![key(KeyCode::Char('q'))]).await;
    assert!(matches!(end, WizardEnd::Quit), "the run did not leave");
    assert!(fx.written_device().is_none());
    assert!(!fx.daemon_installed());
    assert_eq!(fx.scans(), 0);
    assert_eq!(fx.reloads(), 0, "a walk that wrote nothing told the daemon");
}

/// `q` on the Adapter step, after the device was written, tells a running daemon to read the
/// file as the wizard ends: the daemon step that would have told it is never reached.
#[tokio::test(start_paused = true)]
async fn q_on_the_adapter_step_tells_the_daemon_what_was_written() {
    let fx = host_hearing_one().with_adapters([("hci0", 2), ("hci1", 5)]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    wizard
        .answer(0, Some("dac".into()))
        .await
        .expect("the device is written");
    let (end, _) = walked(&mut wizard, vec![key(KeyCode::Char('q'))]).await;
    assert!(matches!(end, WizardEnd::Quit), "the run did not leave");
    assert!(fx.written_device().is_some());
    assert_eq!(fx.reloads(), 1, "the daemon was not told of the device");
}

/// The last step hands the device over, which is what the caller opens the device screen
/// on.
#[tokio::test(start_paused = true)]
async fn enter_on_the_last_step_hands_over_the_device_that_was_added() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    let (end, _) = walked(&mut wizard, (0..6).map(|_| key(KeyCode::Enter)).collect()).await;
    let device = added(&end).expect("the run ended on the device it added");
    assert_eq!(device.name, "Bifrost 3");
    assert_eq!(device.udid, "0102030405060708");
    assert_eq!(
        fx.written_device().map(|written| written.name),
        Some(device.name.clone())
    );
}

/// After the device is written there is nothing to go back to: the footer's `Esc` is drawn
/// dimmed, in `Role::Faint`, while `q`, which still quits, keeps `Role::KeyHint`'s colour.
#[tokio::test(start_paused = true)]
async fn the_footer_dims_esc_on_the_written_step_and_keeps_q_lit() {
    let theme = Theme::new(false);
    let mut wizard = Wizard::new(host_hearing_one());
    let (_, buffer) =
        walked_buffer(&mut wizard, (0..6).map(|_| key(KeyCode::Enter)).collect()).await;
    assert_eq!(Some(footer_fg(&buffer, "Esc")), theme.style(Role::Faint).fg);
    assert_eq!(Some(footer_fg(&buffer, "q")), theme.style(Role::KeyHint).fg);
    assert_ne!(theme.style(Role::Faint).fg, theme.style(Role::KeyHint).fg);
}

/// An answer the wizard refuses is said where it was refused, the step stays where it was,
/// and what was typed stays there to be corrected.
#[tokio::test(start_paused = true)]
async fn a_refused_answer_is_said_on_the_step_it_was_refused_on() {
    let fx = host_hearing_one().with_taken(["dac"]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let mut script = typing("dac");
    script.push(key(KeyCode::Enter));
    let (end, rows) = walked(&mut wizard, script).await;
    assert!(
        matches!(end, WizardEnd::Stopped { .. }),
        "the script ran out on the step that was refused"
    );
    assert!(
        rows[22].contains("already configured"),
        "the refusal was not said: |{}|",
        rows[22]
    );
    assert!(
        rows.iter().any(|row| row.starts_with(" > dac")),
        "what was typed is not there to be corrected: {rows:?}"
    );
    assert!(fx.written_device().is_none());
}

/// An effect the machine would not carry out ends the run with the code it earned: asking
/// the same step again would ask the same unwritable file the same thing.
#[tokio::test(start_paused = true)]
async fn a_write_the_machine_refuses_ends_the_run_with_the_code_it_earned() {
    let fx = host_hearing_one().unwritable();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let (end, _) = walked(&mut wizard, vec![key(KeyCode::Enter)]).await;
    match end {
        WizardEnd::Stopped { code, why } => {
            assert_eq!(code, 1);
            assert!(why.contains("read-only"), "{why}");
        }
        _ => panic!("the run did not stop"),
    }
}

/// The seconds a scan takes are spent under a frame that says so, with the keys that give
/// up read throughout: one press leaves, without waiting the scan out.
#[tokio::test(start_paused = true)]
async fn a_wait_is_named_while_it_is_spent_and_a_key_ends_it() {
    for ending in [ctrl_c(), key(KeyCode::Char('q'))] {
        let fx = host_hearing_one().scanning_for(SCAN_TAKES);
        let mut wizard = Wizard::new(fx.clone());
        walk_to(&mut wizard, Step::Scan).await;
        let started = Instant::now();
        let (end, rows) = walked(&mut wizard, vec![key(KeyCode::Enter), ending]).await;
        assert!(matches!(end, WizardEnd::Quit), "the key did not leave");
        assert!(
            started.elapsed() < SCAN_TAKES,
            "the scan was waited out: {:?}",
            started.elapsed()
        );
        assert!(
            rows.iter().any(|row| row.contains("⠋ listening…")),
            "the spinner is not in front of the wait: {rows:?}"
        );
        assert!(fx.written_device().is_none());
    }
}

/// The spinner turns while a wait actually runs, and not only at the moment it began: a
/// wait several of its own steps long ends on a glyph other than the one it opened on.
#[tokio::test(start_paused = true)]
async fn the_spinner_turns_while_a_wait_runs() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let theme = Theme::new(false);
    let mut canvas = Canvas::new(&mut terminal, &mut hits, &theme);
    let mut view = ShownStep::waiting_on("Adding", "listening…");
    assert_eq!(
        view.spinner,
        Some('⠋'),
        "a wait does not open on its first glyph"
    );
    let mut reading = true;
    let mut events = pressed(vec![]);

    let waited = wait_on(
        &mut canvas,
        &mut events,
        &mut reading,
        &mut view,
        true,
        async {
            tokio::time::sleep(Duration::from_millis(250)).await;
            7
        },
    )
    .await;

    assert!(
        matches!(waited, Waited::Done(7)),
        "the work was not seen through"
    );
    assert_ne!(
        view.spinner,
        Some('⠋'),
        "the spinner did not turn at all over a wait several of its own steps long"
    );
}

/// A step with more to say than the screen shows carries its words on a screenful at a
/// time, and the key line names the keys that do it.
#[tokio::test(start_paused = true)]
async fn the_page_keys_carry_the_step_s_own_words_on() {
    let mut fx = FakeEffects::healthy();
    for at in 0..12 {
        fx = fx.with_fix(&format!("sudo fix number {at}"), false);
    }
    let mut wizard = Wizard::new(fx);
    let (_, first) = walked(&mut wizard, vec![key(KeyCode::Char('q'))]).await;
    assert!(
        !first.iter().any(|row| row.contains("fix number 11")),
        "the last row is on the first screenful: {first:?}"
    );
    assert!(
        first.iter().any(|row| row.contains("more lines")),
        "the frame does not say there is more: {first:?}"
    );
    assert!(
        first[23].contains("PgUp/PgDn scroll"),
        "the key line does not name the keys that scroll: |{}|",
        first[23]
    );

    let mut fx = FakeEffects::healthy();
    for at in 0..12 {
        fx = fx.with_fix(&format!("sudo fix number {at}"), false);
    }
    let mut wizard = Wizard::new(fx);
    let (_, carried) = walked(&mut wizard, vec![key(KeyCode::PageDown)]).await;
    assert!(
        carried.iter().any(|row| row.contains("fix number 11")),
        "a page down did not carry the words on: {carried:?}"
    );
    assert!(
        carried.iter().any(|row| row.contains("▶ Continue")),
        "what answers the step went off the screen: {carried:?}"
    );
}

/// The caret moves along the line being typed, and a letter goes in where it stands.
#[tokio::test(start_paused = true)]
async fn the_caret_keys_move_along_the_line_and_type_where_they_stand() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let mut script = typing("dc");
    script.push(key(KeyCode::Left));
    script.extend(typing("a")); // dac
    script.push(key(KeyCode::Home));
    script.extend(typing("x")); // xdac
    script.push(key(KeyCode::End));
    script.extend(typing("y")); // xdacy
    script.push(key(KeyCode::Enter));
    script.push(key(KeyCode::Enter)); // the daemon
    script.push(key(KeyCode::Enter)); // done
    let (end, _) = walked(&mut wizard, script).await;
    assert_eq!(
        added(&end).map(|device| device.name.clone()),
        Some("xdacy".to_string())
    );
}

/// A letter held down with Ctrl is a key combination rather than a letter: it types
/// nothing into the line.
#[tokio::test(start_paused = true)]
async fn a_ctrl_combination_types_nothing() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let mut script = typing("dac");
    script.push(Ok(Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    ))));
    script.push(key(KeyCode::Enter));
    script.push(key(KeyCode::Enter)); // the daemon
    script.push(key(KeyCode::Enter)); // done
    let (end, _) = walked(&mut wizard, script).await;
    assert_eq!(
        added(&end).map(|device| device.name.clone()),
        Some("dac".to_string())
    );
}

/// A key that gives up while a write is under way is honoured once the write is through,
/// not part-way: the device is in the file, and the run leaves with success.
#[tokio::test(start_paused = true)]
async fn a_write_under_way_is_seen_through_before_the_run_leaves() {
    for ending in [ctrl_c(), key(KeyCode::Char('q'))] {
        let fx = host_hearing_one().writing_for(WRITE_TAKES);
        let mut wizard = Wizard::new(fx.clone());
        walk_to(&mut wizard, Step::Name).await;
        let started = Instant::now();
        let (end, rows) = walked(&mut wizard, vec![key(KeyCode::Enter), ending]).await;
        assert!(matches!(end, WizardEnd::Quit), "the run did not leave");
        assert_eq!(
            fx.written_device().map(|device| device.name),
            Some("Bifrost 3".to_string()),
            "the write was dropped part-way"
        );
        assert!(
            started.elapsed() >= WRITE_TAKES,
            "the write was not waited out: {:?}",
            started.elapsed()
        );
        assert!(
            rows.iter().any(|row| row.contains("finishing…")),
            "the frame did not say the write was being seen through: {rows:?}"
        );
    }
}

/// A click on a choice takes the cursor there, and a second on the row it is already on
/// answers with it — the two presses a keyboard makes, in the order it makes them.
#[tokio::test(start_paused = true)]
async fn a_click_takes_the_cursor_to_a_choice_and_a_second_answers_with_it() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Daemon).await;
    // The daemon step draws its two choices on rows five and six.
    let (end, _) = walked(
        &mut wizard,
        vec![click(4, 6), click(4, 6), key(KeyCode::Enter)],
    )
    .await;
    assert!(added(&end).is_some());
    assert!(!fx.daemon_installed(), "the click took the wrong choice");
}

/// Two clicks on one row of a list too long to draw whole answer with the choice that row
/// carries: the window stays where it is until the cursor would leave it, so the second
/// click lands on the same choice the first one took the cursor to.
#[tokio::test(start_paused = true)]
async fn two_clicks_on_a_row_of_a_windowed_list_answer_with_it() {
    // A product no profile covers, which is what offers the whole profile list — more
    // choices than an eighty-by-twenty-four screen can draw at once.
    let fx = FakeEffects::healthy().with_scan(vec![seen("0807060504030201")]);
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Pick).await;
    wizard.answer(0, None).await.expect("the device is taken");
    let offered = wizard
        .prompt()
        .await
        .expect("the profile list")
        .choices
        .len();
    assert!(offered > 20, "this case wants a windowed list: {offered}");

    // A row well below the middle of the window, which is where a list that recentred on
    // every move would have slid another choice under the second click.
    let row = 14;
    let (end, rows) = walked(
        &mut wizard,
        vec![
            click(4, row),
            click(4, row),
            key(KeyCode::Enter), // the name the profile gives it
            key(KeyCode::Enter), // the daemon
            key(KeyCode::Enter), // done
        ],
    )
    .await;
    assert!(
        added(&end).is_some(),
        "the second click did not answer the step: {rows:?}"
    );
    assert!(
        fx.written_device()
            .and_then(|device| device.profile)
            .is_some(),
        "no profile was chosen"
    );
}

/// A click on a key of the key line is that key, exactly as it is on every other screen.
#[tokio::test(start_paused = true)]
async fn a_click_on_the_key_line_presses_the_key_it_names() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    let (end, _) = walked(&mut wizard, vec![click(68, 23)]).await;
    assert!(matches!(end, WizardEnd::Quit), "the q of the key line");
    assert!(fx.written_device().is_none());
}

/// A terminal with nothing left to say ends the run rather than leaving it waiting on a
/// screen nobody is reading.
#[tokio::test(start_paused = true)]
async fn a_terminal_that_stops_answering_ends_the_run() {
    let mut wizard = Wizard::new(host_hearing_one());
    let (end, _) = walked(&mut wizard, Vec::new()).await;
    match end {
        WizardEnd::Stopped { code, why } => {
            assert_eq!(code, 1);
            assert_eq!(why, super::super::run::INPUT_ENDED);
        }
        _ => panic!("the run did not end"),
    }
}

/// The foreground of the first cell of `word` on the last row of `buffer`.
fn footer_fg(buffer: &ratatui::buffer::Buffer, word: &str) -> ratatui::style::Color {
    let text: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 23)).expect("a cell").symbol())
        .collect();
    let at = text
        .find(word)
        .unwrap_or_else(|| panic!("no {word:?} in |{text}|"));
    let column = u16::try_from(text[..at].chars().count()).expect("a column");
    buffer.cell((column, 23)).expect("a cell").fg
}

/// [`walked_buffer`] with the walk's `leaving` given.
async fn walked_leaving(
    wizard: &mut Wizard<FakeEffects>,
    script: Vec<io::Result<Event>>,
    leaving: Leaving,
) -> (WizardEnd, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut events = pressed(script);
    let end = event_loop(
        &mut terminal,
        &mut events,
        wizard,
        &Theme::new(false),
        leaving,
    )
    .await;
    (end, terminal.backend().buffer().clone())
}

/// A walk the TUI opened leaves on `Esc` from its first step, writing nothing, and its key
/// row says so with `Esc` lit.
#[tokio::test(start_paused = true)]
async fn esc_on_the_first_step_leaves_a_walk_the_tui_opened() {
    let theme = Theme::new(false);
    let fx = host_hearing_one();
    let mut wizard = Wizard::starting_at(fx.clone(), Step::Scan, Given::default());
    let (end, _) = walked_leaving(&mut wizard, vec![key(KeyCode::Esc)], Leaving::ToTui).await;
    assert!(matches!(end, WizardEnd::Quit), "the walk was not left");
    assert!(fx.written_device().is_none());

    // The key row, drawn before the key arrives, has Esc lit.
    let mut wizard = Wizard::starting_at(host_hearing_one(), Step::Scan, Given::default());
    let (_, buffer) = walked_leaving(&mut wizard, vec![], Leaving::ToTui).await;
    assert_eq!(
        Some(footer_fg(&buffer, "Esc")),
        theme.style(Role::KeyHint).fg
    );
}

/// `svolder setup` has no screen behind its first step: `Esc` changes nothing and is drawn
/// dimmed.
#[tokio::test(start_paused = true)]
async fn esc_on_the_first_step_of_svolder_setup_does_nothing_and_is_dimmed() {
    let theme = Theme::new(false);
    let mut wizard = Wizard::starting_at(host_hearing_one(), Step::Scan, Given::default());
    let (_, before) = walked_leaving(&mut wizard, vec![], Leaving::Nowhere).await;
    let mut wizard = Wizard::starting_at(host_hearing_one(), Step::Scan, Given::default());
    let (end, after) = walked_leaving(&mut wizard, vec![key(KeyCode::Esc)], Leaving::Nowhere).await;
    assert!(
        matches!(end, WizardEnd::Stopped { .. }),
        "the script ran out on the same step"
    );
    assert_eq!(before, after, "Esc changed the screen");
    assert_eq!(Some(footer_fg(&after, "Esc")), theme.style(Role::Faint).fg);
}

/// The first step's `Later` choice, and `Esc` on that step under `svolder setup`, end the walk
/// with nothing written and hand over to the interface.
#[tokio::test(start_paused = true)]
async fn later_and_esc_on_the_first_step_leave_svolder_setup_for_later() {
    let fx = host_hearing_one();
    let mut wizard = Wizard::new(fx.clone());
    let (end, _) = walked_leaving(
        &mut wizard,
        vec![key(KeyCode::Down), key(KeyCode::Enter)],
        Leaving::Nowhere,
    )
    .await;
    assert!(matches!(end, WizardEnd::Later), "the choice did not leave");
    assert!(fx.written_device().is_none());

    let mut wizard = Wizard::new(host_hearing_one());
    let (end, buffer) = walked_leaving(&mut wizard, vec![], Leaving::Nowhere).await;
    assert!(matches!(end, WizardEnd::Stopped { .. }));
    assert_eq!(
        Some(footer_fg(&buffer, "Esc")),
        Theme::new(false).style(Role::KeyHint).fg,
        "Esc is lit on a first step that can be left"
    );
    let mut wizard = Wizard::new(host_hearing_one());
    let (end, _) = walked_leaving(&mut wizard, vec![key(KeyCode::Esc)], Leaving::Nowhere).await;
    assert!(matches!(end, WizardEnd::Later), "Esc did not leave");
}

/// `q` still quits, and `Esc` on a later step still goes back rather than leaving.
#[tokio::test(start_paused = true)]
async fn q_quits_and_esc_goes_back_from_a_later_step() {
    let mut wizard = Wizard::new(host_hearing_one());
    let (end, _) =
        walked_leaving(&mut wizard, vec![key(KeyCode::Char('q'))], Leaving::Nowhere).await;
    assert!(matches!(end, WizardEnd::Quit));
    let mut wizard = Wizard::new(host_hearing_one());
    let (end, _) = walked_leaving(
        &mut wizard,
        vec![key(KeyCode::Enter), key(KeyCode::Esc), key(KeyCode::Esc)],
        Leaving::Nowhere,
    )
    .await;
    assert!(
        matches!(end, WizardEnd::Later),
        "back to the first step, then out"
    );
}

/// The Passphrase step is a row of its own: it opens on a code with no digits, digits typed on
/// it go in order, and `Enter` answers with them.
#[tokio::test(start_paused = true)]
async fn the_passphrase_step_takes_a_code_typed_a_digit_at_a_time() {
    let fx = host_hearing_one().refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let mut script = vec![key(KeyCode::Enter)]; // the name, which the device turns away
    script.extend(typing("1x2-34"));
    script.push(key(KeyCode::Enter));
    script.push(key(KeyCode::Enter)); // the daemon
    script.push(key(KeyCode::Enter)); // done
    let (end, _) = walked(&mut wizard, script).await;
    assert_eq!(
        added(&end).map(|device| device.passphrase.clone()),
        Some("1234".to_string()),
        "the digits, and only the digits, were the answer"
    );
    assert_eq!(fx.tried().last().map(String::as_str), Some("1234"));
}

/// A code of fewer than four digits is refused where it is typed: the wizard is not asked, the
/// row stays as it is, and finishing the code answers.
#[tokio::test(start_paused = true)]
async fn a_partial_code_is_refused_on_the_step_without_asking_the_wizard() {
    let fx = host_hearing_one().refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let tried = fx.tried().len();
    let mut script = vec![key(KeyCode::Enter)];
    script.extend(typing("12"));
    script.push(key(KeyCode::Enter));
    let (end, rows) = walked(&mut wizard, script).await;
    assert!(
        matches!(end, WizardEnd::Stopped { .. }),
        "the script ran out on the step that was refused"
    );
    assert!(
        rows[22].contains("a code is four digits"),
        "the refusal was not said: |{}|",
        rows[22]
    );
    assert!(
        rows.iter().any(|row| row.starts_with(" > ‹ 12__ ›")),
        "the digits typed are not there to go on from: {rows:?}"
    );
    assert_eq!(
        fx.tried().len(),
        tried + 1,
        "only the name's connect was tried"
    );
}

/// `←→` step the row round `none`, `auto` and the code, and `Enter` answers with what it is on:
/// `auto` is tried like any code, and a device that turns it away is said to have.
#[tokio::test(start_paused = true)]
async fn the_arrows_step_the_passphrase_row_and_enter_answers_with_it() {
    let fx = host_hearing_one().refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    // From the code, one step on is none and two are auto.
    let script = vec![
        key(KeyCode::Enter),
        key(KeyCode::Right),
        key(KeyCode::Right),
        key(KeyCode::Enter),
    ];
    let (_, rows) = walked(&mut wizard, script).await;
    assert_eq!(fx.tried().last().map(String::as_str), Some("auto"));
    assert!(
        rows[22].contains("the device did not accept auto"),
        "|{}|",
        rows[22]
    );
    assert!(
        rows.iter().any(|row| row.starts_with(" > ‹ auto ›")),
        "the row did not stay on what was refused: {rows:?}"
    );

    // A digit typed on auto starts the code, with the digits of no earlier visit.
    let fx = host_hearing_one().refusing_without("1234");
    let mut wizard = Wizard::new(fx.clone());
    walk_to(&mut wizard, Step::Name).await;
    let script = vec![
        key(KeyCode::Enter),
        key(KeyCode::Left), // the code, back to auto
        key(KeyCode::Char('7')),
    ];
    let (_, rows) = walked(&mut wizard, script).await;
    assert!(
        rows.iter().any(|row| row.starts_with(" > ‹ 7___ ›")),
        "{rows:?}"
    );
}

fn globals() -> Globals<'static> {
    Globals {
        transport: "fake",
        adapter: None,
        socket: None,
        device: None,
        verbose: false,
    }
}

fn options() -> TuiOptions {
    TuiOptions {
        no_color: true,
        demo: None,
        join: None,
        reloaded: None,
        checked: false,
    }
}

fn a_wizard() -> Wizard<crate::wizard::fake::FakeEffects> {
    Wizard::starting_at(
        crate::wizard::fake::FakeEffects::healthy(),
        Step::Scan,
        Given::default(),
    )
}

/// Without a terminal the wizard gives the walk back at once, and says nothing.
#[tokio::test]
async fn the_wizard_gives_the_walk_back_where_there_is_no_terminal() {
    let claim = Refusing::new(false);
    let mut said = Vec::new();
    let back = setup_on(
        &claim,
        &mut said,
        Config::default(),
        "/nonexistent/config.toml".into(),
        &globals(),
        a_wizard(),
        options(),
    )
    .await;
    assert!(back.is_err());
    assert_eq!(claim.calls.get(), 0);
    assert!(said.is_empty());
}

/// A terminal that cannot be claimed gives the walk back with the reason said first.
#[tokio::test]
async fn the_wizard_says_why_when_the_terminal_is_refused() {
    let claim = Refusing::new(true);
    let mut said = Vec::new();
    let back = setup_on(
        &claim,
        &mut said,
        Config::default(),
        "/nonexistent/config.toml".into(),
        &globals(),
        a_wizard(),
        options(),
    )
    .await;
    assert!(back.is_err());
    assert_eq!(claim.calls.get(), 1);
    let line = format!(
        "svolder: {}\n",
        Failure::of(Kind::Usage, "the terminal: no tty; asking in plain prompts")
    );
    assert_eq!(String::from_utf8(said).unwrap(), line);
}
