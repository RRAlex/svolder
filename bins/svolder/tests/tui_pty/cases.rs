// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first cases: start-up, the Web UI box, Now playing, the mouse, a resize and quitting, at
//! the three widths the TUI is checked at. Each is `#[ignore]`d (see `main.rs`).

use crate::harness::*;
use crate::tap::*;

/// The terminal sizes of the cases, `(cols, rows)`: the smallest the TUI draws a screen on, a wide
/// one, and one too small for any screen.
const SMALL: (u16, u16) = (80, 24);
const WIDE: (u16, u16) = (120, 40);
const NARROW: (u16, u16) = (60, 20);

const RESPONDERS: [Responder; 4] = [
    Responder::None,
    Responder::Plain,
    Responder::Sixel,
    Responder::Kitty,
];

/// Every overlay a stray key could open.
fn no_overlay(text: &str) {
    for overlay in ["Quit svolder?", "Keys ·", "┌ Settings"] {
        assert!(!text.contains(overlay), "{overlay:?} is open:\n{text}");
    }
}

fn assert_clean_device_screen(tui: &Tui) {
    let text = tui.screen_text();
    assert!(
        text.contains("┤ Controls ├"),
        "not the device screen:\n{text}"
    );
    no_overlay(&text);
    for stray in STRAY {
        assert!(!text.contains(stray), "{stray:?} was drawn:\n{text}");
    }
}

/// The static rows of the device screen: from under the tab strip to the Balance row. The
/// animation layer dissolves a closing box over several frames, so a wait for its last words to
/// go would read a screen with rows of it still on, while these rows read back as they were.
fn static_rows(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let strip = lines.iter().position(|l| l.contains("┤ Controls ├"));
    let balance = lines.iter().position(|l| l.contains("Balance"));
    match (strip, balance) {
        (Some(strip), Some(balance)) if strip < balance => lines[strip + 1..=balance].join("\n"),
        _ => String::new(),
    }
}

/// Wait until the device screen reads as `before` did, an overlay gone.
fn wait_settled_as(tui: &Tui, before: &str) {
    let want = static_rows(before);
    tui.wait_for("the device screen back as it was", WAIT, |s| {
        let text = s.contents();
        text.contains("Balance") && static_rows(&text) == want
    });
}

/// The device screen with the fake device's values read: a snapshot taken any earlier still has
/// rows to fill in.
fn wait_values_read(tui: &Tui) {
    tui.wait_ready();
    tui.wait_for("the fake device's values", WAIT, |s| {
        let text = s.contents();
        ["63/127", "‹ Flat ›", "Phase invert [ ] off", "10000 Hz"]
            .iter()
            .all(|row| text.contains(row))
    });
}

/// Wait until the Settings box and everything in it is gone.
fn wait_settings_closed(tui: &Tui) {
    tui.wait_for("the Settings box closed", WAIT, |s| {
        let text = s.contents();
        !text.contains("┌ Settings") && text.contains("Balance")
    });
}

/// What the start-up probe asks a terminal, in order: the kitty graphics query, primary device
/// attributes (sixel), the cell size and the status report that ends the answers.
fn probe_queries() -> Vec<Query> {
    vec![
        Query::KittyGraphics("31".into()),
        Query::Da1,
        Query::CellSize,
        Query::Status,
    ]
}

/// How many answers a terminal gives the probe: none for silence, the device attributes and the
/// status report for a plain one, and the cell size and the kitty answer where it has them.
fn probe_answers(responder: Responder) -> usize {
    match responder {
        Responder::None => 0,
        Responder::Plain => 2,
        Responder::Sixel => 3,
        Responder::Kitty => 4,
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn start_up_draws_no_stray_bytes_and_reads_no_key_under_every_responder() {
    for responder in RESPONDERS {
        for (cols, rows) in [SMALL, WIDE] {
            let tui = Tui::start(cols, rows, responder);
            tui.wait_ready();
            // No answer to the probe is on the screen, and none was read as a key: no box is
            // open, nothing flipped, and the values are as the fake device has them.
            assert_clean_device_screen(&tui);
            wait_values_read(&tui);
            // The probe asked once, in the order the picker asks, and was answered as the
            // terminal does.
            assert_eq!(
                tui.queries(),
                probe_queries(),
                "{responder:?} at {cols}x{rows}"
            );
            assert_eq!(
                tui.answers().len(),
                probe_answers(responder),
                "{responder:?} at {cols}x{rows}"
            );
            // The program asked for the alternate screen (`finish` checks it gave it back).
            let raw = tui.raw();
            let entered = raw.windows(8).position(|w| w == b"\x1b[?1049h");
            assert!(entered.is_some(), "the alternate screen was never entered");
            // The probe came after it, on that screen.
            let probe = raw.windows(4).position(|w| w == b"\x1b_Gi");
            assert!(
                entered < probe,
                "the probe was asked before the alternate screen"
            );
            tui.quit();
        }
    }
}

/// `cover_art = "off"` asks the terminal nothing at all, and `"auto"` asks it as the probe does,
/// the same TUI, direct on the fake device, either way.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn cover_art_off_sends_no_query_and_auto_sends_the_probe() {
    for responder in RESPONDERS {
        let off = Tui::start_plain(SMALL.0, SMALL.1, responder, "cover_art = \"off\"");
        off.wait_ready();
        assert_clean_device_screen(&off);
        assert_eq!(off.queries(), [], "{responder:?}: off asked something");
        assert_eq!(off.answers(), Vec::<Vec<u8>>::new());
        off.quit();

        let auto = Tui::start_plain(SMALL.0, SMALL.1, responder, "cover_art = \"auto\"");
        auto.wait_ready();
        assert_clean_device_screen(&auto);
        assert_eq!(auto.queries(), probe_queries(), "{responder:?}");
        auto.quit();
    }
}

/// The wait for a terminal that never answers is the probe's own: `cover_art = "auto"` starts
/// about 100 ms after the start with the art off (the probe's wait; 101 ms measured), measured as
/// the time to the first cell the TUI draws, the fastest of five starts each (a busy machine
/// delays some starts, but a probe that really waits shows in every one); both are said. The
/// bound is the wait plus 100 ms.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_silent_terminal_costs_the_probes_wait_and_no_more_at_start_up() {
    let first_cell = |lines: &str| {
        let began = std::time::Instant::now();
        let tui = Tui::start_plain(SMALL.0, SMALL.1, Responder::None, lines);
        tui.wait_for("the first cell", WAIT, |s| {
            s.contents().chars().any(|c| !c.is_whitespace())
        });
        let took = began.elapsed();
        tui.wait_ready();
        tui.quit();
        took
    };
    let fastest = |lines: &str| {
        (0..5)
            .map(|_| first_cell(lines))
            .min()
            .expect("five starts")
    };
    let off = fastest("cover_art = \"off\"");
    let auto = fastest("cover_art = \"auto\"");
    eprintln!("start-up to the first cell, silent terminal: off {off:?}, auto {auto:?}");
    assert!(
        auto < off + std::time::Duration::from_millis(200),
        "auto {auto:?} is slower than off {off:?} by {:?}, more than the probe's wait plus 100 ms",
        auto.saturating_sub(off)
    );
}

/// Each of `replies`, sent to a fresh TUI the way a terminal would send it unasked, leaves the
/// device screen as it was: keys are read in order, so once the help box the `?` after it opens
/// is up, the reply has been read, and when the box is gone the screen must read as before.
fn replies_leave_the_screen_alone(replies: &[&str], settle: std::time::Duration) {
    for reply in replies {
        let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
        wait_values_read(&tui);
        let before = tui.screen_text();
        tui.send_keys(reply);
        tui.send_keys("?");
        tui.wait_for_text("Everywhere");
        tui.send_keys("\x1b");
        let want = static_rows(&before);
        tui.wait_for(
            &format!("the screen as it was after the reply {reply:?}"),
            settle,
            |s| {
                let text = s.contents();
                text.contains("Balance") && static_rows(&text) == want
            },
        );
        assert_clean_device_screen(&tui);
        tui.send_keys("\x03");
        tui.finish();
    }
}

/// Replies a terminal sends to queries the TUI did not make (or made and gave up on) are not
/// keys.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn late_terminal_replies_are_not_read_as_keys() {
    replies_leave_the_screen_alone(
        &[
            "\x1b[?62;4;c",
            "\x1b[0n",
            "\x1b[12;40R",
            "\x1b[6;16;8t",
            "\x1b[4;384;640t",
            "\x1b[?0u",
            // The kitty graphics answer (an APC string) and the terminal's name and version (a
            // DCS string): the input reader reads their letters as keys, and the TUI takes them
            // out.
            "\x1b_Gi=31;OK\x1b\\",
            "\x1bP>|xterm(388)\x1b\\",
        ],
        WAIT,
    );
}

/// An answer that comes after the probe's wait, while the intro is still up, is no key either:
/// a key would end the intro, and its letters would flip a control.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn an_answer_after_the_probes_wait_is_swallowed_during_the_intro() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    // The first cell is drawn after the probe has given up.
    tui.wait_for("the first cell", WAIT, |s| {
        s.contents().chars().any(|c| !c.is_whitespace())
    });
    tui.send_keys("\x1b_Gi=31;OK\x1b\\\x1bP>|xterm(388)\x1b\\");
    wait_values_read(&tui);
    assert_clean_device_screen(&tui);
    let text = tui.screen_text();
    assert!(
        text.contains("Phase invert [ ] off"),
        "a letter flipped phase:\n{text}"
    );
    tui.quit();
}

/// The harness's own checks of what `finish` proves, on small shell programs.
#[test]
fn finish_passes_a_program_that_leaves_the_terminal_clean() {
    Tui::run_program(
        SMALL.0,
        SMALL.1,
        std::path::Path::new("/bin/sh"),
        &["-c", "exit 0"],
    )
    .finish();
}

/// A terminal answer that reaches the pty before the program has raw mode is echoed onto the main
/// screen (`^[[?62;4;c`): the stray-bytes check must see it there, as the review's control did
/// against the TUI.
#[test]
#[should_panic(expected = "is on the main screen after the program left")]
fn finish_sees_a_reply_echoed_onto_the_main_screen() {
    let tui = Tui::run_program(
        SMALL.0,
        SMALL.1,
        std::path::Path::new("/bin/sh"),
        &["-c", "read line; exit 0"],
    );
    tui.send_keys("\x1b[?62;4;c\n");
    tui.finish();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_terminal_too_small_says_so_and_recovers_when_it_grows() {
    let tui = Tui::start(NARROW.0, NARROW.1, Responder::None);
    tui.wait_for_text("terminal too small");
    tui.wait_for_text("svolder needs 80×24, this one is 60×20");
    let text = tui.screen_text();
    for stray in STRAY {
        assert!(!text.contains(stray), "{stray:?} was drawn:\n{text}");
    }
    tui.resize(SMALL.0, SMALL.1);
    tui.wait_ready();
    assert_clean_device_screen(&tui);
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn ctrl_c_quits_a_terminal_too_small_to_draw_on() {
    let tui = Tui::start(NARROW.0, NARROW.1, Responder::None);
    tui.wait_for_text("terminal too small");
    tui.send_keys("\x03");
    tui.finish();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_three_key_rules_hold_on_the_real_terminal() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        tui.wait_ready();
        // `?` is the same on every screen; its box opens with the three rules.
        tui.send_keys("?");
        tui.wait_for_text("Everywhere");
        let help = tui.screen_text();
        for rule in [
            "Lowercase letters act",
            "Uppercase letters toggle",
            "Symbols and navigation keys",
        ] {
            assert!(
                help.contains(rule),
                "{rule:?} missing at {cols}x{rows}:\n{help}"
            );
        }
        tui.send_keys("\x1b");
        tui.wait_for("help closed", WAIT, |s| {
            !s.contents().contains("Everywhere")
        });

        // Lowercase letters act on the device or open a screen: `t` opens Telemetry.
        tui.send_keys("t");
        tui.wait_for_text("┤ Telemetry ├");
        tui.send_keys("?");
        tui.wait_for_text("Everywhere");
        tui.send_keys("\x1b");
        tui.wait_for("help closed", WAIT, |s| {
            !s.contents().contains("Everywhere")
        });
        tui.send_keys("c");
        tui.wait_for_text("┤ Controls ├");

        // Uppercase letters toggle a TUI mode: `M` turns mouse capture off, in the terminal too.
        tui.send_keys("M");
        tui.wait_for("mouse capture off", WAIT, |s| {
            s.mouse_protocol_mode() == vt100::MouseProtocolMode::None
                && s.contents().contains("mouse:off")
        });
        tui.send_keys("M");
        tui.wait_for("mouse capture on", WAIT, |s| {
            s.mouse_protocol_mode() != vt100::MouseProtocolMode::None
                && s.contents().contains("mouse:on")
        });
        tui.quit();
    }
}

/// Settings (`s`), Connect (`5`), Enter on the Web UI row.
fn open_web_box(tui: &Tui) {
    tui.send_keys("s");
    tui.wait_for_text("┤ ¹Look ├");
    tui.send_keys("5");
    tui.wait_for_text("┤ ⁵Connect ├");
    tui.send_keys("\r");
    tui.wait_for_text("Pairing code");
}

fn close_web_box(tui: &Tui) {
    tui.send_keys("\x1b");
    tui.wait_for("the Web UI box closed", WAIT, |s| {
        !s.contents().contains("Pairing code")
    });
    tui.send_keys("\x1b");
    wait_settings_closed(tui);
}

/// How many rows below the link have nine or more cells of the QR's half blocks.
fn qr_rows(screen: &vt100::Screen) -> usize {
    let (rows, cols) = (screen.size().0.get(), screen.size().1.get());
    let Some(link) = screen
        .contents()
        .lines()
        .position(|l| l.contains("#login="))
    else {
        return 0;
    };
    (link as u16..rows)
        .filter(|row| {
            (0..cols)
                .filter(|col| {
                    screen
                        .cell(*row, *col)
                        .is_some_and(|c| matches!(c.contents(), "█" | "▀" | "▄"))
                })
                .count()
                >= 9
        })
        .count()
}

/// A demo with the web UI, narrowed to 127.0.0.1 as soon as the box is open (on loopback the
/// box draws no QR, only a note that a phone cannot reach it). `--web` makes the
/// demo serve every interface (its config says `0.0.0.0`, which only the demo's code could
/// change), so the case moves the Reach row to "this computer" first thing: the web UI is on the
/// network for the start-up and a moment, behind its token, as `make demo-computer` has it for
/// as long as it runs. The box is open and the daemon says it listens on loopback when this
/// returns.
fn start_with_web(cols: u16, rows: u16, loopback: bool) -> Tui {
    let tui = Tui::start_with(cols, rows, Responder::None, &["--demo", "--web"], &[]);
    wait_values_read(&tui);
    open_web_box(&tui);
    if !loopback {
        tui.wait_for_text("State         listening on 0.0.0.0:");
        return tui;
    }
    // The cursor starts on the Web UI row; Down is Reach, and an arrow moves it to this computer.
    tui.send_keys("\x1b[B");
    tui.send_keys("\x1b[D");
    tui.wait_for_text("State         listening on 127.0.0.1:");
    tui.wait_for_text("Reach         this computer");
    tui
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_web_ui_box_draws_its_link_and_sign_in_qr_and_closes() {
    for (cols, rows) in [WIDE, (80, 40)] {
        // The QR is for a phone, so this case needs the network reach the demo starts with: the
        // one place a case has the web UI on every interface, for its length, behind its token.
        let tui = start_with_web(cols, rows, false);
        // The box makes the sign-in link itself, whether the daemon's status came in before
        // the box opened or after: no `l` is pressed.
        // The QR is drawn in half blocks below the link, inside the box, over several frames of
        // the animation layer: wait for all of it.
        tui.wait_for("the link, its timer and the whole QR", WAIT, |s| {
            let text = s.contents();
            text.contains("left") && text.contains("█▀▀▀▀▀█") && qr_rows(s) >= 8
        });
        close_web_box(&tui);
        assert_clean_device_screen(&tui);
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn g_turns_the_guest_qr_on_and_off_beside_the_sign_in_qr() {
    // Wide and tall enough for both codes side by side.
    let tui = start_with_web(200, 50, false);
    tui.wait_for("the sign-in QR", WAIT, |s| {
        s.contents().contains("sign in") && qr_rows(s) >= 8
    });
    assert!(!tui.screen_text().contains("guest: read-only"));
    let before = tui
        .with_screen(shown_link)
        .expect("a sign-in link on show before g");
    tui.send_keys("g");
    // Both captions on one row: the codes sit side by side at this width, not stacked.
    tui.wait_for("both captions on one row", WAIT, |s| {
        s.contents().lines().any(|row| {
            row.contains("sign in") && row.contains("guest: read-only, anyone on your network")
        }) && s.contents().contains("Guest view    read-only")
    });
    // The server restarted and dropped the code that was on show: a different one replaces it.
    tui.wait_for("a fresh sign-in link, not the dead one", WAIT, |s| {
        shown_link(s).is_some_and(|now| now != before)
    });
    let fresh = tui.with_screen(shown_link).unwrap();
    tui.send_keys("g");
    tui.wait_for("the guest caption gone", WAIT, |s| {
        let text = s.contents();
        !text.contains("guest: read-only") && text.contains("Guest view    off")
    });
    tui.wait_for(
        "a link, not the one the restart killed, with its QR",
        WAIT,
        |s| shown_link(s).is_some_and(|now| now != fresh) && qr_rows(s) >= 8,
    );
    close_web_box(&tui);
    assert_clean_device_screen(&tui);
    tui.quit();
}

/// The sign-in link on the screen, whole, if one shows.
fn shown_link(screen: &vt100::Screen) -> Option<String> {
    let contents = screen.contents();
    let row = contents.lines().find(|row| row.contains("#login="))?;
    let at = row.find("http")?;
    Some(row[at..].split_whitespace().next()?.to_string())
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_web_ui_box_opens_and_closes_at_the_smallest_size() {
    let tui = start_with_web(SMALL.0, SMALL.1, true);
    for row in ["Web UI        on", "Reach", "Link", "Token"] {
        tui.wait_for_text(row);
    }
    tui.send_keys("l");
    tui.wait_for_text("http://127.0.0.1:");
    tui.wait_for_text("a phone cannot reach");
    close_web_box(&tui);
    assert_clean_device_screen(&tui);
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn now_playing_is_drawn_on_the_fake_devices_screen() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        // The demo's simulated player plays one fake track.
        tui.wait_for_text("Now playing: Test Pressing");
        if cols >= 100 {
            tui.wait_for_text("Now playing: Test Pressing — The Fakes");
        }
        tui.quit();
    }
}

/// The kitty protocol's placeholder cell, which is where a kitty picture is on the screen.
const PLACEHOLDER: char = '\u{10EEEE}';

/// How many cells of the art `screen` has: the kitty placeholders, or the half-block cells in
/// the rows of the Status pane (which the art fills at its right, however tall the pane is).
fn art_cells(screen: &vt100::Screen) -> usize {
    let text = screen.contents();
    let placeholders = text.chars().filter(|c| *c == PLACEHOLDER).count();
    let Some(top) = text.lines().position(|line| line.contains("┌ Status")) else {
        return placeholders;
    };
    let halves: usize = text
        .lines()
        .skip(top)
        .map(|line| line.chars().filter(|c| matches!(c, '▀' | '▄')).count())
        .sum();
    placeholders + halves
}

/// The art's cell count once it has stood unchanged for a second (the TUI draws whole frames, but
/// a picture can still be arriving after its first one), so a picture still being drawn is not
/// taken for the finished one. Each change restarts the second, all under [`WAIT`] per change.
fn settled_art_cells(tui: &Tui) -> usize {
    tui.wait_for("art cells", WAIT, |screen| art_cells(screen) > 0);
    let deadline = std::time::Instant::now() + WAIT;
    let mut counts = vec![tui.with_screen(art_cells)];
    while tui
        .try_wait_for(std::time::Duration::from_secs(1), |s| {
            Some(art_cells(s)) != counts.last().copied()
        })
        .is_ok()
    {
        counts.push(tui.with_screen(art_cells));
        assert!(
            std::time::Instant::now() < deadline,
            "the art's cell count never settled; the counts were {counts:?}"
        );
    }
    counts[counts.len() - 1]
}

/// Whether the art is on the screen, drawn as `responder`'s terminal draws it: the kitty
/// protocol's picture and its placeholders, a sixel picture, or half-blocks.
fn art_drawn(tap: &Tap, responder: Responder) -> bool {
    match responder {
        Responder::Kitty => {
            count_bytes(&tap.raw, b"a=T,U=1") > 0 && art_cells(tap.parser.screen()) > 0
        }
        Responder::Sixel => sixels_in(&tap.raw) > 0,
        Responder::None | Responder::Plain => art_cells(tap.parser.screen()) > 0,
    }
}

fn wait_art(tui: &Tui, responder: Responder) {
    tui.wait_tap(&format!("the art, drawn for {responder:?}"), WAIT, |tap| {
        art_drawn(tap, responder)
    });
}

/// The colours of the cells the art is drawn in, in the rows the Now playing row ends.
fn art_styles(screen: &vt100::Screen) -> Vec<(String, vt100::Color, vt100::Color)> {
    let text = screen.contents();
    let Some(row) = text.lines().position(|line| line.contains("Now playing")) else {
        return Vec::new();
    };
    let row = row as u16;
    let mut cells = Vec::new();
    for y in row.saturating_sub(4)..=row {
        for x in 65..79 {
            if let Some(cell) = screen.cell(y, x) {
                cells.push((cell.contents().to_string(), cell.fgcolor(), cell.bgcolor()));
            }
        }
    }
    cells
}

/// The cover of the demo's track is drawn the way the terminal draws pictures: with the kitty
/// protocol where it answers the graphics query, as sixel where it says it has it, as half-blocks
/// beside the title where it says nothing or little. No other protocol's bytes are written.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_cover_is_drawn_with_the_protocol_the_terminal_answers_for() {
    for responder in RESPONDERS {
        for (cols, rows) in [SMALL, WIDE] {
            let tui = Tui::start(cols, rows, responder);
            tui.wait_for_text("Now playing: Test Pressing");
            wait_art(&tui, responder);
            assert_clean_device_screen(&tui);
            let (kitty, sixel) = (tui.count_in_raw(b"a=T,U=1"), tui.sixel_count());
            match responder {
                Responder::Kitty => assert_eq!((kitty, sixel), (1, 0), "{responder:?}"),
                Responder::Sixel => assert_eq!((kitty, sixel), (0, 1), "{responder:?}"),
                Responder::None | Responder::Plain => {
                    assert_eq!((kitty, sixel), (0, 0), "{responder:?}");
                }
            }
            // The words keep their room: the title is whole, and the pane's border is there.
            let text = tui.screen_text();
            assert!(text.contains("Now playing: Test Pressing"), "{text}");
            let (_, row) = tui.find("Now playing");
            let line = text.lines().nth(usize::from(row)).expect("the row");
            assert!(
                line.trim_end().ends_with('│') || tui.with_screen(art_cells) > 0,
                "{line:?}"
            );
            tui.quit();
        }
    }
}

/// A track change, which is the next track with its own cover here, draws the new cover: a new
/// kitty picture is sent, a new sixel picture is sent after the screen was cleared, and the
/// half-blocks change.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_track_change_redraws_the_cover() {
    for responder in RESPONDERS {
        let cue = crate::common::TempDir::new("cover-next");
        let file = cue.path().join("next");
        let file_text = file.display().to_string();
        let tui = Tui::start_with(
            SMALL.0,
            SMALL.1,
            responder,
            &["--demo"],
            &[("SVOLDER_FAKE_COVER_NEXT_FILE", file_text.as_str())],
        );
        tui.wait_for_text("Now playing: Test Pressing");
        wait_art(&tui, responder);
        let before_cells = tui.with_screen(art_styles);
        let raw_before = tui.raw().len();

        std::fs::write(&file, b"").expect("the cue");
        tui.wait_for_text("Now playing: Second Pressing");
        match responder {
            Responder::Kitty => tui.wait_tap("the second picture", WAIT, |tap| {
                count_bytes(&tap.raw, b"a=T,U=1") == 2
            }),
            Responder::Sixel => {
                tui.wait_tap("the second picture", WAIT, |tap| sixels_in(&tap.raw) >= 2)
            }
            Responder::None | Responder::Plain => tui.wait_for("other cells", WAIT, |screen| {
                art_cells(screen) > 0 && art_styles(screen) != before_cells
            }),
        }
        assert_clean_device_screen(&tui);
        if responder == Responder::Sixel {
            // What the terminal keeps of a sixel picture goes with a repaint of every cell.
            let raw = tui.raw();
            assert!(
                raw[raw_before..].windows(4).any(|w| w == b"\x1b[2J"),
                "the screen was not cleared under the old sixel picture: {:?}",
                String::from_utf8_lossy(&raw[raw_before..raw.len().min(raw_before + 1500)])
            );
        }
        tui.quit();
    }
}

/// A cover that arrives while the Settings box is open is uploaded when the box closes: the
/// kitty protocol sends the picture once, in the art's first cell, which a box's Clear would
/// wipe in the same frame.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_cover_that_arrives_under_a_box_is_uploaded_when_the_box_closes() {
    let cue = crate::common::TempDir::new("cover-box");
    let file = cue.path().join("next");
    let file_text = file.display().to_string();
    let tui = Tui::start_with(
        WIDE.0,
        WIDE.1,
        Responder::Kitty,
        &["--demo"],
        &[("SVOLDER_FAKE_COVER_NEXT_FILE", file_text.as_str())],
    );
    tui.wait_for_text("Now playing: Test Pressing");
    wait_art(&tui, Responder::Kitty);
    tui.send_keys("s");
    tui.wait_for_text("┌ Settings");
    std::fs::write(&file, b"").expect("the cue");
    // The box hides the track, so nothing on the screen says when the cover has been built; what
    // is checked is where the second upload lies in the bytes: after the box closed.
    let closing = tui.raw().len();
    tui.send_keys("\x1b");
    tui.wait_tap("the second upload", WAIT, |tap| {
        count_bytes(&tap.raw, b"a=T,U=1") == 2
    });
    let raw = tui.raw();
    assert_eq!(
        count_bytes(&raw[..closing], b"a=T,U=1"),
        1,
        "the second picture was written while the box was open"
    );
    tui.wait_for("art cells again", WAIT, |screen| art_cells(screen) > 0);
    assert_clean_device_screen(&tui);
    tui.quit();
}

/// A box over a sixel picture leaves text on its pixels when it closes unless the screen is
/// repainted: the picture is drawn again after the box closes, the whole screen repainted.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_box_over_a_sixel_picture_repaints_the_screen_as_it_closes() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::Sixel);
    tui.wait_for_text("Now playing: Test Pressing");
    wait_art(&tui, Responder::Sixel);
    tui.send_keys("s");
    tui.wait_for_text("┌ Settings");
    let opened = tui.raw().len();
    tui.send_keys("\x1b");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("┌ Settings")
    });
    tui.wait_tap("a repaint and the picture again", WAIT, |tap| {
        count_bytes(&tap.raw[opened..], b"\x1b[2J") > 0 && sixels_in(&tap.raw[opened..]) > 0
    });
    tui.quit();
}

/// A cover that goes (the player drops its art, the track staying) takes the picture off the
/// screen, and the words have their row back.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_cover_that_goes_takes_the_picture_off_the_screen() {
    for responder in RESPONDERS {
        let cue = crate::common::TempDir::new("cover-gone");
        let file = cue.path().join("gone");
        let file_text = file.display().to_string();
        let tui = Tui::start_with(
            SMALL.0,
            SMALL.1,
            responder,
            &["--demo"],
            &[("SVOLDER_FAKE_COVER_GONE_FILE", file_text.as_str())],
        );
        tui.wait_for_text("Now playing: Test Pressing");
        wait_art(&tui, responder);
        let raw_before = tui.raw().len();

        std::fs::write(&file, b"").expect("the cue");
        tui.wait_for("no art on the screen", WAIT, |screen| {
            art_cells(screen) == 0
        });
        tui.wait_for_text("Now playing: Test Pressing — The Fakes");
        assert_clean_device_screen(&tui);
        if responder == Responder::Sixel {
            let raw = tui.raw();
            assert!(
                raw[raw_before..].windows(4).any(|w| w == b"\x1b[2J"),
                "the screen was not cleared under the old sixel picture: {:?}",
                String::from_utf8_lossy(&raw[raw_before..raw.len().min(raw_before + 1500)])
            );
        }
        tui.quit();
    }
}

/// A resize draws the picture again for the new size: the art is sized for the box the new
/// frame has.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resize_sizes_the_cover_again() {
    for responder in [Responder::Kitty, Responder::None] {
        let tui = Tui::start(SMALL.0, SMALL.1, responder);
        tui.wait_for_text("Now playing: Test Pressing");
        wait_art(&tui, responder);
        let small = settled_art_cells(&tui);
        tui.resize(WIDE.0, WIDE.1);
        tui.wait_for("a bigger picture", WAIT, |screen| art_cells(screen) > small);
        assert_clean_device_screen(&tui);
        tui.resize(SMALL.0, SMALL.1);
        tui.wait_for("the small picture again", WAIT, |screen| {
            let cells = art_cells(screen);
            cells > 0 && cells <= small
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_mouse_clicks_tabs_and_marks_and_the_wheel_steps_the_volume() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        wait_values_read(&tui);

        // A click on a control row moves the cursor there, as `↓` does.
        let (col, row) = tui.find("Phase invert");
        tui.send_mouse(Mouse::Click, col + 2, row);
        tui.wait_for_text("▶ Phase invert");
        let before = tui.screen_text();

        // The `? help` mark and the tabs are clickable.
        let (col, row) = tui.find("? help");
        tui.send_mouse(Mouse::Click, col, row);
        tui.wait_for_text("Everywhere");
        tui.send_keys("\x1b");
        wait_settled_as(&tui, &before);
        let (col, row) = tui.find("Telemetry");
        tui.send_mouse(Mouse::Click, col + 1, row);
        tui.wait_for_text("┤ Telemetry ├");
        let (col, row) = tui.find("Controls");
        tui.send_mouse(Mouse::Click, col + 1, row);
        tui.wait_for_text("┤ Controls ├");

        // The wheel over the volume bar steps the volume one notch a click.
        tui.wait_for_text("63/127");
        let (col, row) = tui.find("63/127");
        tui.send_mouse(Mouse::WheelUp, col, row);
        tui.wait_for_text("64/127");
        tui.send_mouse(Mouse::WheelDown, col, row);
        tui.wait_for_text("63/127");
        tui.quit();
    }
}

/// The value on a Settings line whose label is `Tooltips`: the word after the label, without
/// the arrows the selected row puts round it.
fn tooltips_value(line: &str) -> Option<&str> {
    let after = line.split("Tooltips").nth(1)?;
    after
        .split_whitespace()
        .find(|word| !matches!(*word, "‹" | "›"))
}

/// The words of the Telemetry tab's tooltip, which nothing else on a screen says.
const TAB_TIP: &str = "every reading";

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resting_pointer_shows_a_tooltip_and_leaving_the_target_hides_it() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        wait_values_read(&tui);

        // Resting on the Telemetry tab shows its tip, made from the key table.
        let (col, row) = tui.find("Telemetry");
        tui.send_mouse(Mouse::Move, col + 1, row);
        tui.wait_for_text(TAB_TIP);

        // Moving off the target, to a cell that is nothing, hides it.
        tui.send_mouse(Mouse::Move, 0, rows - 1);
        tui.wait_for("the tooltip gone", WAIT, |s| {
            !s.contents().contains(TAB_TIP)
        });

        // A key hides a tooltip that shows.
        tui.send_mouse(Mouse::Move, col + 1, row);
        tui.wait_for_text(TAB_TIP);
        tui.send_keys("\x1b[B");
        tui.wait_for("the tooltip gone after a key", WAIT, |s| {
            !s.contents().contains(TAB_TIP)
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_settings_tooltips_row_turns_the_tooltips_off() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    wait_values_read(&tui);
    // Settings (`s`), Behaviour (`3`), down to the row after Mouse capture, Space.
    tui.send_keys("s3");
    tui.wait_for_text("Mouse capture");
    tui.send_keys("\x1b[B");
    tui.send_keys(" ");
    tui.wait_for("the Tooltips row off", WAIT, |s| {
        s.contents()
            .lines()
            .any(|l| tooltips_value(l) == Some("off"))
    });
    tui.send_keys("\x1b");
    wait_settings_closed(&tui);

    // Resting on the tab shows nothing now: the wait outlasts the delay several times over.
    let (col, row) = tui.find("Telemetry");
    tui.send_mouse(Mouse::Move, col + 1, row);
    let shown = tui.try_wait_for(std::time::Duration::from_secs(3), |s| {
        s.contents().contains(TAB_TIP)
    });
    assert!(shown.is_err(), "a tooltip showed with the setting off");

    // Back on, the same rest shows it.
    tui.send_keys("s3\x1b[B ");
    tui.wait_for("the Tooltips row on", WAIT, |s| {
        s.contents()
            .lines()
            .any(|l| tooltips_value(l) == Some("on"))
    });
    tui.send_keys("\x1b");
    wait_settings_closed(&tui);
    tui.send_mouse(Mouse::Move, col + 1, row);
    tui.wait_for_text(TAB_TIP);
    tui.quit();
}

/// The words of the setup wizard's `q` tooltip: what the key does, then the key.
const SETUP_Q_TIP: &str = "quit · q";

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resting_pointer_on_the_setup_wizards_key_line_shows_a_tooltip() {
    let tui = Tui::run_program(
        SMALL.0,
        SMALL.1,
        &crate::common::svolder_with_daemon(),
        &["setup"],
    );
    tui.wait_for("the alternate screen", WAIT, |s| s.alternate_screen());
    // The key line is the last row; its `q` is the one key that is there on every step.
    tui.wait_for("the key line", WAIT, |s| {
        s.contents()
            .lines()
            .last()
            .is_some_and(|l| l.contains("quit"))
    });
    let (col, row) = tui.find("q quit");
    tui.send_mouse(Mouse::Move, col, row);
    tui.wait_for_text(SETUP_Q_TIP);
    // Off the key, the tip goes.
    tui.send_mouse(Mouse::Move, 0, 0);
    tui.wait_for("the tooltip gone", WAIT, |s| {
        !s.contents().contains(SETUP_Q_TIP)
    });
    // Quitting gives the terminal back: mouse reporting off, alternate screen left.
    tui.send_keys("q");
    tui.finish();
}

/// The tab strip's line when it is a whole frame wide: `cols` characters, ending in the
/// Esc menu's mark.
fn strip_of_width(text: &str, cols: u16) -> Option<usize> {
    text.lines().position(|l| {
        l.starts_with("─┤ Controls ├")
            && l.ends_with("esc menu ─")
            && l.chars().count() == usize::from(cols)
    })
}

/// Whether the screen holds a frame the size of the terminal, whole, with nothing of an earlier
/// size left in it: the tab strip is `cols` wide and the first rows of the panes end in a border
/// at both edges. A frame the animation layer is still drawing is not one yet.
fn frame_fits(screen: &vt100::Screen, cols: u16) -> bool {
    let Some(strip) = strip_of_width(&screen.contents(), cols) else {
        return false;
    };
    let edge = |row: u16, col: u16| {
        screen
            .cell(row, col)
            .is_some_and(|cell| cell.contents() == "│")
    };
    ((strip as u16 + 2)..(strip as u16 + 10)).all(|row| edge(row, 0) && edge(row, cols - 1))
}

fn wait_for_frame(tui: &Tui, cols: u16, rows: u16) {
    tui.wait_for(&format!("a whole frame at {cols}x{rows}"), WAIT, |s| {
        s.size().0.get() == rows && s.size().1.get() == cols && frame_fits(s, cols)
    });
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resize_mid_run_redraws_the_whole_frame() {
    let tui = Tui::start(WIDE.0, WIDE.1, Responder::None);
    tui.wait_ready();
    wait_for_frame(&tui, WIDE.0, WIDE.1);
    for (cols, rows) in [SMALL, WIDE, SMALL] {
        tui.resize(cols, rows);
        wait_for_frame(&tui, cols, rows);
    }
    // Too small for any screen says so, and the frame comes back whole when the terminal grows.
    tui.resize(NARROW.0, NARROW.1);
    tui.wait_for_text("svolder needs 80×24, this one is 60×20");
    tui.resize(SMALL.0, SMALL.1);
    wait_for_frame(&tui, SMALL.0, SMALL.1);
    assert_clean_device_screen(&tui);
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn quitting_gives_the_terminal_back() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        tui.wait_ready();
        // The terminal is the TUI's while it runs: alternate screen, mouse reporting, no cursor.
        tui.with_screen(|s| {
            assert!(s.alternate_screen());
            assert_ne!(s.mouse_protocol_mode(), vt100::MouseProtocolMode::None);
            assert!(s.hide_cursor());
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn ctrl_c_quits_at_once_and_gives_the_terminal_back() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    tui.wait_ready();
    tui.send_keys("\x03");
    tui.finish();
}

/// Every cell of the screen as text and colours.
fn grid(tui: &Tui) -> Look {
    tui.with_screen(look_of)
}

/// Every cell of `screen`: its text and colours.
fn look_of(screen: &vt100::Screen) -> Look {
    let (rows, cols) = screen.size();
    (0..rows.get())
        .map(|row| {
            (0..cols.get())
                .map(|col| {
                    let cell = screen.cell(row, col).expect("a cell on the screen");
                    (cell.contents().to_string(), cell.fgcolor(), cell.bgcolor())
                })
                .collect()
        })
        .collect()
}

/// The squirrel's box on the Tone pane: 27 columns by 6 rows, its right column three inside the
/// pane's right edge and its bottom row on the pane's bottom border. `(x0, y0, x1, y1)`, both
/// ends included.
fn squirrel_box(tui: &Tui) -> (usize, usize, usize, usize) {
    let text = tui.screen_text();
    let lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
    let (left, top) = tui.find("┌ Tone control");
    let (left, top) = (usize::from(left), usize::from(top));
    let right = lines[top]
        .iter()
        .rposition(|c| *c == '┐')
        .expect("the Tone pane's corner");
    let bottom = (top..lines.len())
        .find(|row| lines[*row].get(left) == Some(&'└'))
        .expect("the Tone pane's bottom border");
    (right - 2 - 26, bottom - 5, right - 2, bottom)
}

type Look = Vec<Vec<(String, vt100::Color, vt100::Color)>>;

/// The cells below the header that differ between two screens, outside `(x0, y0, x1, y1)`.
fn changed_outside(
    was: &Look,
    now: &Look,
    (x0, y0, x1, y1): (usize, usize, usize, usize),
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (y, (a, b)) in was.iter().zip(now).enumerate().skip(2) {
        for (x, (a, b)) in a.iter().zip(b).enumerate() {
            let inside = (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
            // A digit that became another digit is a counter ticking, not the squirrel.
            let ticking = |cell: &(String, vt100::Color, vt100::Color)| {
                cell.0.len() == 1 && cell.0.chars().all(|c| c.is_ascii_digit())
            };
            if a != b && !inside && !(ticking(a) && ticking(b)) {
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_mascots_entrance_and_exit_leave_no_stray_cells_outside_its_box() {
    let (cols, rows) = WIDE;
    let tui = Tui::start_with(
        cols,
        rows,
        Responder::None,
        &["--demo"],
        &[("COLORTERM", "truecolor")],
    );
    wait_values_read(&tui);
    // The demo's toast on how to join it comes and goes; the squirrel stands aside for a toast.
    tui.wait_for_text("svolder tui --join");
    tui.wait_for("the join toast gone", WAIT, |s| {
        !s.contents().contains("svolder tui --join")
    });
    // The intro and the first boxes have finished when two looks half a second apart agree.
    let mut before = grid(&tui);
    let settle_deadline = std::time::Instant::now() + WAIT;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let again = grid(&tui);
        let same = changed_outside(&before, &again, (0, 0, 0, 0)).is_empty();
        before = again;
        if same {
            break;
        }
        assert!(
            std::time::Instant::now() < settle_deadline,
            "the screen never settled"
        );
    }
    let home = squirrel_box(&tui);
    // Cells that move on their own (the cursor's pulse) are not the squirrel's strays.
    let mut idle = Vec::new();
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        idle.extend(changed_outside(&before, &grid(&tui), home));
    }
    // The cursor row's highlight sweeps along it, a different cell each time it is looked at.
    // So does the marquee of the Now playing row.
    let moving = [
        usize::from(tui.find("▶ Input").1),
        usize::from(tui.find("Now playing").1),
    ];
    let stray_of = |look: &Look| -> Vec<(usize, usize)> {
        let mut stray = changed_outside(&before, look, home);
        stray.retain(|cell| !idle.contains(cell) && !moving.contains(&cell.1));
        stray
    };
    let konami = "\x1b[A\x1b[A\x1b[B\x1b[B\x1b[D\x1b[C\x1b[D\x1b[Cba";
    // The code is typed into the help box, which closes on the last key; the squirrel enters
    // then, by an entrance that takes up to two seconds, and hops once it has landed.
    tui.send_keys("?");
    tui.wait_for_text("Everywhere");
    tui.send_keys(konami);
    // The help box dissolves over the first second; the entrance runs under it and past it.
    std::thread::sleep(std::time::Duration::from_millis(1200));
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let stray = stray_of(&grid(&tui));
        assert!(
            stray.is_empty(),
            "{} cells outside the box during the entrance, from {:?}",
            stray.len(),
            &stray[..stray.len().min(5)]
        );
    }
    // The entrance's length depends on the machine: the squirrel has landed when its box differs
    // from the resting screen in more than a few cells.
    tui.wait_for("the squirrel landed in its box", WAIT, |screen| {
        let landed = look_of(screen);
        let inside = (home.0..=home.2)
            .flat_map(|x| (home.1..=home.3).map(move |y| (x, y)))
            .filter(|(x, y)| before[*y][*x] != landed[*y][*x])
            .count();
        inside > 20
    });
    // The dismissal leaves by an exit and takes every cell with it.
    tui.send_keys("?");
    tui.wait_for_text("Everywhere");
    tui.send_keys(konami);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let stray = stray_of(&grid(&tui));
        assert!(
            stray.is_empty(),
            "{} cells outside the box during the exit, from {:?}",
            stray.len(),
            &stray[..stray.len().min(5)]
        );
    }
    tui.quit();
}

/// Where the two panes meet on the device screen: the columns of the Controls pane's right
/// border and of the Tone pane's left border, read off the top border row.
fn pane_edges(text: &str) -> Option<(u16, u16)> {
    let row = text.lines().find(|l| l.contains("┌ Controls"))?;
    let cells: Vec<char> = row.chars().collect();
    let controls = cells.iter().position(|c| *c == '┐')?;
    let tone_open = cells
        .iter()
        .enumerate()
        .skip(controls + 1)
        .find(|(_, c)| **c == '┌')?
        .0;
    Some((
        u16::try_from(controls).ok()?,
        u16::try_from(tone_open).ok()?,
    ))
}

/// Wait until the panes meet at `controls_border` (the Controls pane's right border column).
fn wait_for_bar_at(tui: &Tui, controls_border: u16) {
    tui.wait_for(&format!("the bar at column {controls_border}"), WAIT, |s| {
        pane_edges(&s.contents()).is_some_and(|(border, _)| border == controls_border)
    });
}

/// The configuration file the demo's run reads and saves, in its private directory.
fn demo_config(tui: &Tui) -> std::path::PathBuf {
    std::fs::read_dir(tui.home())
        .expect("read the run's home")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sd-"))
        })
        .expect("the demo's private directory")
        .join("config")
        .join("svolder")
        .join("config.toml")
}

/// What the file says `device_split` is, once it says it.
fn saved_split(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("device_split = ").map(str::to_string))
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_bar_between_the_panes_drags_clamps_nudges_and_resets() {
    let (cols, rows) = WIDE;
    let tui = Tui::start(cols, rows, Responder::None);
    wait_values_read(&tui);

    // Even by default: 60 and 59 columns beside their gap, the Controls pane's border on column 59.
    wait_for_bar_at(&tui, 59);
    let config = demo_config(&tui);
    assert_eq!(
        saved_split(&config),
        None,
        "nothing is written by opening the screen"
    );

    // Drag the bar from the Controls pane's border to column 69: the Controls pane is 70 wide,
    // and the split is saved once the button is up.
    tui.send_mouse(Mouse::DragTo(69, 10), 59, 10);
    wait_for_bar_at(&tui, 69);
    tui.wait_for("the split saved", WAIT, |_| saved_split(&config).is_some());
    assert_eq!(saved_split(&config).as_deref(), Some("0.588"));

    // Dragged past either limit it stops 40 columns from the other pane's edge.
    tui.send_mouse(Mouse::DragTo(2, 10), 69, 10);
    wait_for_bar_at(&tui, 39);
    tui.send_mouse(Mouse::DragTo(118, 10), 39, 10);
    wait_for_bar_at(&tui, 78);
    let (_, tone) = pane_edges(&tui.screen_text()).expect("both panes");
    assert_eq!(tone, 80, "the Tone pane keeps 40 columns: 80 to 119");

    // `<` and `>` nudge it four columns; `|` and a double-click put it back to an even split.
    tui.send_keys("<");
    wait_for_bar_at(&tui, 74);
    tui.send_keys(">>");
    wait_for_bar_at(&tui, 78);
    tui.send_keys("<<<");
    wait_for_bar_at(&tui, 66);
    tui.send_keys("|");
    wait_for_bar_at(&tui, 59);
    tui.wait_for("the even split saved", WAIT, |_| {
        saved_split(&config).as_deref() == Some("0.5")
    });
    tui.send_keys(">");
    wait_for_bar_at(&tui, 63);
    // Two clicks on the bar, on one cell and close together, are a double-click.
    // The window is real time, so a loaded machine can read the pair too far apart: the pair is
    // written in one go and asked again, a few times, before the wait on the result.
    for _ in 0..5 {
        tui.send_mouse(Mouse::DoubleClick, 63, 10);
        let reset = tui
            .try_wait_for(std::time::Duration::from_secs(3), |s| {
                pane_edges(&s.contents()).is_some_and(|(border, _)| border == 59)
            })
            .is_ok();
        if reset {
            break;
        }
    }
    wait_for_bar_at(&tui, 59);

    // A resize keeps the fraction and re-clamps it: 79 of 119 columns is 0.664, which a 100-column
    // terminal reads as 66 and holds to 60 for the Tone pane's 40.
    tui.send_mouse(Mouse::DragTo(79, 10), 59, 10);
    wait_for_bar_at(&tui, 78);
    tui.resize(100, 40);
    tui.wait_for("the bar re-clamped to the narrower terminal", WAIT, |s| {
        pane_edges(&s.contents()).is_some_and(|(border, _)| border == 59)
    });
    tui.resize(cols, rows);
    wait_for_bar_at(&tui, 78);
    assert_clean_device_screen(&tui);
    tui.quit();
}

/// The column the Volume row's reading starts on, which is where its bar ends.
fn volume_reading_column(text: &str) -> Option<usize> {
    let row = text
        .lines()
        .find(|line| line.contains("Volume") && !line.contains('┌'))?;
    let end = row.find("dB").or_else(|| row.find("silence"))?;
    let before = &row[..end];
    let reading = before.rfind("  ")? + 2;
    Some(row[..reading].chars().count())
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn dragging_the_bar_between_the_panes_moves_the_volume_bars_right_end() {
    let (cols, rows) = WIDE;
    let tui = Tui::start(cols, rows, Responder::None);
    wait_values_read(&tui);
    wait_for_bar_at(&tui, 59);
    let even = volume_reading_column(&tui.screen_text()).expect("the volume row");
    // The bar runs to the pane's edge: the reading ends inside the Controls pane's border.
    assert!(
        even > 40 && even < 59,
        "the reading starts at column {even}"
    );

    tui.send_mouse(Mouse::DragTo(79, 10), 59, 10);
    wait_for_bar_at(&tui, 78);
    // The frame is read once it has settled on the wider pane's bar: the bar grew by the 19
    // columns the pane gained.
    tui.wait_for("the bar grown with the pane", WAIT, |s| {
        volume_reading_column(&s.contents()) == Some(even + 19)
    });

    tui.send_mouse(Mouse::DragTo(49, 10), 78, 10);
    wait_for_bar_at(&tui, 49);
    tui.wait_for("the bar shrunk with the pane", WAIT, |s| {
        volume_reading_column(&s.contents()) == Some(even - 10)
    });
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_saved_split_is_where_the_next_run_draws_the_bar() {
    let (cols, rows) = WIDE;
    let config = "schema = 1\n[[device]]\nname = \"demo\"\nudid = \"0102030405060708\"\n\
                  [tui]\ndevice_split = 0.7\n";
    let tui = Tui::start_with_config(cols, rows, config);
    wait_values_read(&tui);
    // 0.7 of the 119 columns beside the gap is 83, which leaves the Tone pane 36: held to 79
    // columns for the Controls pane, its border on column 78.
    wait_for_bar_at(&tui, 78);
    tui.quit();
}

/// Settings (`s`), Connect (`5`), then down `rows` rows from the first.
fn connect_row(tui: &Tui, rows: usize) {
    tui.send_keys("s");
    tui.wait_for_text("┤ ¹Look ├");
    tui.send_keys("5");
    tui.wait_for_text("┤ ⁵Connect ├");
    for _ in 0..rows {
        tui.send_keys("\x1b[B");
    }
}

/// The Connect rows after Web UI, counted from it: where each MQTT row sits.
const MQTT_BRIDGE_ROW: usize = 6;
const MQTT_HOST_ROW: usize = 7;
const MQTT_PORT_ROW: usize = 8;
const MQTT_USERNAME_ROW: usize = 10;

/// The line of the row `label` is on, from its label to the end of the left column.
fn row_of(screen: &vt100::Screen, label: &str) -> Option<String> {
    screen
        .contents()
        .lines()
        .find(|line| line.contains(label) && !line.contains('┌'))
        .map(str::to_string)
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_mqtt_username_and_password_rows_edit_with_the_password_masked() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    wait_values_read(&tui);

    // The username: Enter types it, Enter saves, and the row says it.
    connect_row(&tui, MQTT_USERNAME_ROW);
    tui.wait_for_text("MQTT username");
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT username");
    tui.send_keys("svolder\r");
    tui.wait_for("the username row says it", WAIT, |s| {
        row_of(s, "MQTT username").is_some_and(|row| row.contains("svolder"))
    });

    // The password: typed masked, shown with Tab, masked again, saved, never on the screen after.
    tui.send_keys("\x1b[B");
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT password");
    tui.send_keys("hunter2");
    tui.wait_for("seven bullets", WAIT, |s| s.contents().contains("•••••••"));
    assert!(
        !tui.screen_text().contains("hunter2"),
        "the password showed while it was typed"
    );
    tui.send_keys("\t");
    tui.wait_for("the password shown", WAIT, |s| {
        s.contents().contains("hunter2")
    });
    tui.send_keys("\t");
    tui.wait_for("the password masked again", WAIT, |s| {
        !s.contents().contains("hunter2")
    });
    tui.send_keys("\r");
    tui.wait_for("the password row says set", WAIT, |s| {
        !s.contents().contains("┌ MQTT password")
            && row_of(s, "MQTT password")
                .is_some_and(|row| row.contains("set") && !row.contains("not set"))
    });
    assert!(!tui.screen_text().contains("hunter2"));

    // An empty line keeps it; Delete twice clears it.
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT password");
    tui.send_keys("\r");
    tui.wait_for("the empty line closed and the password kept", WAIT, |s| {
        !s.contents().contains("┌ MQTT password")
            && row_of(s, "MQTT password").is_some_and(|row| !row.contains("not set"))
    });
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT password");
    tui.send_keys("\x1b[3~\x1b[3~");
    tui.wait_for("the password row says not set", WAIT, |s| {
        row_of(s, "MQTT password").is_some_and(|row| row.contains("not set"))
    });
    tui.send_keys("\x1b");
    tui.send_keys("\x1b");
    wait_settings_closed(&tui);
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_mqtt_status_row_says_the_broker_is_not_reachable() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    wait_values_read(&tui);
    // A host on this machine and port 1, where nothing listens: no broker is ever reached.
    connect_row(&tui, MQTT_HOST_ROW);
    tui.wait_for_text("MQTT broker host");
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT broker host");
    tui.send_keys("127.0.0.1\r");
    tui.wait_for("the host row says it", WAIT, |s| {
        row_of(s, "MQTT broker host").is_some_and(|row| row.contains("127.0.0.1"))
    });
    tui.send_keys("\x1b[B\r");
    tui.wait_for_text("┌ MQTT broker port");
    tui.send_keys("\x7f\x7f\x7f\x7f1\r");
    tui.wait_for("the port row says 1", WAIT, |s| {
        row_of(s, "MQTT broker port")
            .is_some_and(|row| row.trim_end().ends_with('1') || row.contains(" 1 "))
    });
    // Back up to the bridge row and turn it on.
    for _ in 0..(MQTT_PORT_ROW - MQTT_BRIDGE_ROW) {
        tui.send_keys("\x1b[A");
    }
    tui.send_keys(" ");
    tui.wait_for("the status row says not reachable", WAIT, |s| {
        row_of(s, "MQTT status").is_some_and(|row| row.contains("not reachable"))
    });
    tui.send_keys("\x1b");
    wait_settings_closed(&tui);
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn ctrl_c_at_the_mqtt_password_prompt_exits_130() {
    let mut tui = Tui::run_program(
        SMALL.0,
        SMALL.1,
        &crate::common::svolder_with_daemon(),
        &["mqtt", "password"],
    );
    tui.wait_for_text("MQTT password:");
    tui.send_keys("\x03");
    let status = tui
        .wait_exit_for(WAIT)
        .expect("the command did not exit on Ctrl-C");
    assert_eq!(status.code(), Some(130), "{status:?}");
    // That the echo is back is the guard's unit test (the pty is closed with the program).
}

/// The preset list opens from its word on the EQ page by a click; inside it a click picks a
/// row, the wheel moves the selection, and a click outside closes it.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_preset_list_takes_clicks_the_wheel_and_an_outside_click() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start(cols, rows, Responder::None);
        wait_values_read(&tui);
        tui.send_keys("e");
        tui.wait_for_text("┤ EQ ├");
        // Open by a click on the `presets` word of the Response border.
        tui.wait_for_text("presets");
        let (col, row) = tui.find("presets");
        tui.send_mouse(Mouse::Click, col + 1, row);
        tui.wait_for_text("Presets ·");
        tui.wait_for_text("▶   Flat");
        tui.wait_for_text("Warm");
        // The wheel moves the selection a row at a time, and back.
        let (col, row) = tui.find("Warm");
        tui.send_mouse(Mouse::WheelDown, col, row);
        tui.wait_for_text("▶   Warm");
        tui.send_mouse(Mouse::WheelUp, col, row);
        tui.wait_for_text("▶   Flat");
        // A click outside the box closes it.
        tui.send_mouse(Mouse::Click, 0, rows - 1);
        tui.wait_for("the list closed", WAIT, |screen| {
            !screen.contents().contains("Presets ·")
        });
        // Open it again and pick a row by a click: the preset loads and the list closes.
        tui.wait_for_text("presets");
        let (col, row) = tui.find("presets");
        tui.send_mouse(Mouse::Click, col + 1, row);
        tui.wait_for_text("Presets ·");
        tui.wait_for_text("Warm");
        let (col, row) = tui.find("Warm");
        tui.send_mouse(Mouse::Click, col, row);
        tui.wait_for("the list closed on the pick", WAIT, |screen| {
            !screen.contents().contains("Presets ·")
        });
        tui.wait_for_text("Preset Warm");
        tui.quit();
    }
}

/// A Loki Max's own preset list takes the mouse as the tone control's does: a click on a row
/// picks it, the wheel moves the selection, and a click outside closes it.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_device_preset_list_takes_clicks_the_wheel_and_an_outside_click() {
    for (cols, rows) in [SMALL, WIDE] {
        let tui = Tui::start_with(
            cols,
            rows,
            Responder::None,
            &["--demo"],
            &[("SVOLDER_FAKE_PROFILE", "lokimax")],
        );
        tui.wait_ready();
        tui.wait_for_text("Loki Max");
        tui.send_keys("p");
        tui.wait_for_text("Presets · Loki Max");
        // The list opens on the built-in the bands read.
        tui.wait_for_text("▶ ● Flat");
        // The wheel moves the selection a row at a time, and back.
        let (col, row) = tui.find("Bass Boost");
        tui.send_mouse(Mouse::WheelDown, col, row);
        tui.wait_for_text("▶   Bass Boost");
        tui.send_mouse(Mouse::WheelUp, col, row);
        tui.wait_for_text("▶ ● Flat");
        // A click outside the box closes it.
        tui.send_mouse(Mouse::Click, 0, rows - 1);
        tui.wait_for("the list closed", WAIT, |screen| {
            !screen.contents().contains("Presets ·")
        });
        // Open it again and pick a built-in by a click: it applies and the list closes.
        tui.send_keys("p");
        tui.wait_for_text("Presets · Loki Max");
        let (col, row) = tui.find("Bass Boost");
        tui.send_mouse(Mouse::Click, col, row);
        tui.wait_for("the list closed on the pick", WAIT, |screen| {
            !screen.contents().contains("Presets ·")
        });
        tui.wait_for_text("Bass Boost on");
        tui.quit();
    }
}

/// The preset list's import by `i` from a file, a rename by `c` and a delete by `x` each reach
/// the list through the demo's daemon, and a name another front end took after the list was
/// read (written into the demo's file behind the TUI) is refused on screen in the daemon's
/// answer, since the TUI's own check does not know it.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_preset_list_imports_renames_and_deletes_through_the_daemon() {
    let tui = Tui::start(WIDE.0, WIDE.1, Responder::None);
    wait_values_read(&tui);
    std::fs::write(
        tui.home().join("cans.txt"),
        "Preamp: -3.0 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2.0 dB Q 1.0\n",
    )
    .expect("the file to import is written");
    tui.send_keys("e");
    tui.wait_for_text("┤ EQ ├");
    tui.wait_for_text("presets");
    tui.send_keys("p");
    tui.wait_for_text("Presets ·");
    // Import: the file prompt starts at `~/`.
    tui.send_keys("i");
    tui.wait_for_text("Import a file");
    tui.send_keys("cans.txt\r");
    tui.wait_for_text("▶   cans");
    // Rename: the name line is prefilled with the name.
    tui.send_keys("c");
    tui.send_keys(" two\r");
    tui.wait_for_text("▶   cans two");
    // Another front end saves a preset after the list was read: the TUI's own copy does not
    // have it, so the name goes to the daemon, whose writer refuses it under the file's lock.
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    let theirs = svolder_profiles::tone::preset::from_computer(
        "theirs",
        Correction {
            preamp_db: -1.0,
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 500.0,
                g: 1.0,
                q: 1.0,
            }],
        },
    );
    svolder_ipc::Config::add_preset(&demo_config(&tui), &theirs)
        .expect("another front end writes a preset");
    tui.send_keys("n");
    tui.send_keys(&"\x7f".repeat(8));
    tui.send_keys("theirs\r");
    tui.wait_for_text("already exists");
    // Delete asks, and `y` takes the preset out.
    tui.send_keys("\x1b[A");
    tui.wait_for_text("▶   cans two");
    tui.send_keys("x");
    tui.wait_for_text("[y/N]");
    tui.send_keys("y");
    tui.wait_for("the preset gone from the list", WAIT, |screen| {
        !screen.contents().contains("cans two")
    });
    tui.send_keys("\x1b");
    tui.wait_for("the list closed", WAIT, |screen| {
        !screen.contents().contains("Presets ·")
    });
    tui.quit();
}

/// Hold the lock of the configuration file of `tui` for `seconds` (under the two the writer
/// waits), from another process.
fn hold_config_lock(tui: &Tui, seconds: &str) -> std::process::Child {
    use std::process::{Command, Stdio};
    let folder = tui.home().join("config").join("svolder");
    let lock = folder.join("config.toml.lock");
    std::fs::File::create(&lock).expect("the lock file is made");
    let holder = Command::new("flock")
        .arg(&lock)
        .args(["sleep", seconds])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("flock holds the lock");
    // Held once a try for it fails.
    let deadline = std::time::Instant::now() + WAIT;
    while Command::new("flock")
        .arg("-n")
        .arg(&lock)
        .arg("true")
        .status()
        .expect("flock tries the lock")
        .success()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the lock was never held"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    holder
}

/// The configuration of a direct session on the fake device.
const DIRECT_CONFIG: &str = "schema = 1\n[daemon]\nautospawn = false\n[[device]]\nname = \"demo\"\n\
                             udid = \"0102030405060708\"\n";

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_screen_stays_live_while_a_settings_write_waits_for_the_config_lock() {
    let tui = Tui::start_with_config(WIDE.0, WIDE.1, DIRECT_CONFIG);
    wait_values_read(&tui);
    let file = tui
        .home()
        .join("config")
        .join("svolder")
        .join("config.toml");

    // Settings, Behaviour: the box is open before the lock is taken, so the time the lock is
    // held (under the two seconds the writer waits) is for the write alone.
    tui.send_keys("s3");
    tui.wait_for_text("Mouse capture");
    let mut holder = hold_config_lock(&tui, "1.9");
    tui.send_keys("\x1b[B");
    tui.send_keys(" ");
    // The row turns over, and the box closes on Esc, while the write still waits.
    tui.wait_for("the Tooltips row off", WAIT, |s| {
        s.contents()
            .lines()
            .any(|l| tooltips_value(l) == Some("off"))
    });
    tui.send_keys("\x1b");
    wait_settings_closed(&tui);
    assert!(
        holder.try_wait().expect("the holder is polled").is_none(),
        "the screen answered only after the lock was let go"
    );

    // The write lands once the lock is let go.
    holder.wait().expect("the holder ends");
    let deadline = std::time::Instant::now() + WAIT;
    while !std::fs::read_to_string(&file).is_ok_and(|text| text.contains("tooltips")) {
        assert!(
            std::time::Instant::now() < deadline,
            "the write never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_quit_during_a_slow_connect_write_ends_the_run_and_loses_no_write() {
    let mut tui = Tui::start_with_config(SMALL.0, SMALL.1, DIRECT_CONFIG);
    wait_values_read(&tui);
    let file = tui
        .home()
        .join("config")
        .join("svolder")
        .join("config.toml");
    connect_row(&tui, MQTT_USERNAME_ROW);
    tui.wait_for_text("MQTT username");
    tui.send_keys("\r");
    tui.wait_for_text("┌ MQTT username");
    tui.send_keys("svolder");
    let mut holder = hold_config_lock(&tui, "1.5");
    tui.send_keys("\r");
    tui.wait_for_text("saving");
    // The wait reads this `q`: the write is seen through, then the run ends.
    tui.send_keys("q");
    let status = tui
        .wait_exit_for(WAIT)
        .expect("the run did not end after the quit");
    assert!(status.success(), "{status:?}");
    holder.wait().expect("the holder ends");
    let text = std::fs::read_to_string(&file).expect("the file is there");
    assert!(text.contains("svolder"), "the write was lost:\n{text}");
}

/// The rows of the Status pane that carry picture cells (kitty placeholders or half-blocks) on
/// the screen as text, and the pane's own height from its top border to the bottom one.
fn status_art_rows(text: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    // A screen caught between two frames has no pane yet: no pane, no art.
    let Some(top) = lines.iter().position(|line| line.contains("┌ Status")) else {
        return (0, 0);
    };
    let pane = &lines[top..];
    let art = pane
        .iter()
        .filter(|line| {
            line.chars()
                .filter(|c| *c == PLACEHOLDER || matches!(c, '▀' | '▄'))
                .count()
                >= 4
        })
        .count();
    let bottom = pane.iter().position(|line| line.starts_with('└'));
    (art, bottom.map_or(0, |bottom| bottom + 1))
}

/// From 100 by 45 the Status pane grows upward and its cover with it, the kitty picture and the
/// half-block one alike; one row or column short of that the pane and the cover are as they
/// were.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_cover_grows_with_the_status_pane_from_100_by_45() {
    for responder in [Responder::None, Responder::Kitty] {
        let drawn = |cols: u16, rows: u16| {
            let tui = Tui::start(cols, rows, responder);
            tui.wait_for_text("Now playing: Test Pressing");
            wait_art(&tui, responder);
            // The kitty picture's placeholder rows and the half-blocks arrive with the frame
            // that shows them, so a count is taken once the cell count has settled.
            settled_art_cells(&tui);
            assert_clean_device_screen(&tui);
            let counts = status_art_rows(&tui.screen_text());
            tui.quit();
            counts
        };
        let (small_art, small_pane) = drawn(100, 44);
        let (grown_art, grown_pane) = drawn(100, 45);
        assert!(
            grown_pane > small_pane + 4,
            "{responder:?}: {small_pane} {grown_pane}"
        );
        assert!(small_art <= small_pane - 2, "{responder:?}: {small_art}");
        assert!(
            grown_art > small_art + 4,
            "{responder:?}: {small_art} {grown_art}"
        );
        assert!(grown_art <= grown_pane - 2, "{responder:?}: {grown_art}");
    }
}

/// A resize across the 100 by 45 line, both ways, grows the Status pane and its cover and then
/// takes them back, leaving no picture above the shrunk pane.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resize_across_100_by_45_grows_and_shrinks_the_cover() {
    for responder in [Responder::Kitty, Responder::None] {
        let tui = Tui::start(100, 44, responder);
        tui.wait_for_text("Now playing: Test Pressing");
        wait_art(&tui, responder);
        let (small_art, small_pane) = status_art_rows(&tui.screen_text());
        tui.resize(100, 45);
        tui.wait_for("the cover in the grown pane", WAIT, |screen| {
            let (art, pane) = status_art_rows(&screen.contents());
            pane > small_pane + 4 && art > small_art + 4
        });
        assert_clean_device_screen(&tui);
        tui.resize(100, 44);
        tui.wait_for("the small cover again", WAIT, |screen| {
            status_art_rows(&screen.contents()) == (small_art, small_pane)
        });
        // Nothing of the grown picture stands between the Controls pane's top and the small pane
        // (the wordmark above them is made of half-blocks too).
        let text = tui.screen_text();
        let lines: Vec<&str> = text.lines().collect();
        let controls = lines
            .iter()
            .position(|line| line.contains("┌ Controls"))
            .expect("the Controls pane");
        let top = lines
            .iter()
            .position(|line| line.contains("┌ Status"))
            .expect("the pane");
        let above: usize = lines[controls..top]
            .iter()
            .map(|line| {
                line.chars()
                    .filter(|c| *c == PLACEHOLDER || matches!(c, '▀' | '▄'))
                    .count()
            })
            .sum();
        assert_eq!(above, 0, "{responder:?}:\n{text}");
        tui.quit();
    }
}
