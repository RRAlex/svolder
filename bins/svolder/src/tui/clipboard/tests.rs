// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;

use super::*;

/// An environment that holds `set` and nothing else.
fn env(set: &'static [&'static str]) -> impl Fn(&str) -> Option<OsString> {
    move |name| set.contains(&name).then(|| OsString::from("x"))
}

const SEQUENCE: &str = "\x1b]52;c;aGk=\x07";

/// With neither multiplexer set the sequence is as it was; tmux gets the plain sequence (which
/// `set-clipboard on` acts on) and then its DCS passthrough with the escapes doubled (which
/// `allow-passthrough on` lets through); screen gets a DCS alone; tmux wins where both are set
/// (screen inside tmux is still tmux's terminal), and an empty variable is not set.
#[test]
fn the_sequence_is_wrapped_for_tmux_and_screen() {
    assert_eq!(wrap(SEQUENCE, &env(&[])), SEQUENCE);
    assert_eq!(
        wrap(SEQUENCE, &env(&["TMUX"])),
        "\x1b]52;c;aGk=\x07\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\"
    );
    assert_eq!(
        wrap(SEQUENCE, &env(&["STY"])),
        "\x1bP\x1b]52;c;aGk=\x07\x1b\\"
    );
    assert_eq!(
        wrap(SEQUENCE, &env(&["TMUX", "STY"])),
        wrap(SEQUENCE, &env(&["TMUX"]))
    );
    assert_eq!(wrap(SEQUENCE, &|_| Some(OsString::new())), SEQUENCE);
}

/// Wayland names `wl-copy`; X names `xclip` then `xsel` with the arguments that read standard
/// input; both display variables name all three, Wayland's first; neither names none.
#[test]
fn the_display_picks_the_tools() {
    let names = |set| {
        candidates(&env(set))
            .iter()
            .map(|tool| (tool.program, tool.args.join(" ")))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&["WAYLAND_DISPLAY"]), [("wl-copy", String::new())]);
    assert_eq!(
        names(&["DISPLAY"]),
        [
            ("xclip", "-selection clipboard".to_string()),
            ("xsel", "--clipboard --input".to_string())
        ]
    );
    assert_eq!(names(&["DISPLAY", "WAYLAND_DISPLAY"]).len(), 3);
    assert_eq!(names(&["DISPLAY", "WAYLAND_DISPLAY"])[0].0, "wl-copy");
    assert!(names(&[]).is_empty());
}

/// A stub `run` that records what it was asked and answers `answer`.
fn tried(
    set: &'static [&'static str],
    installed: &'static [&'static str],
    answer: Result<Ran, String>,
) -> (Tried, Vec<(String, String)>) {
    let asked = RefCell::new(Vec::new());
    let tried = copy_with_tool(
        &env(set),
        &|program| installed.contains(&program),
        &|tool, text| {
            asked.borrow_mut().push((
                format!("{} {}", tool.program, tool.args.join(" ")),
                text.to_string(),
            ));
            answer.clone()
        },
        "the text",
    );
    (tried, asked.into_inner())
}

/// The first tool installed gets the text and the outcome names it; the tools after it are not
/// tried; one that is missing is passed over for the next.
#[test]
fn the_first_installed_tool_gets_the_text() {
    let (out, asked) = tried(&["WAYLAND_DISPLAY"], &["wl-copy"], Ok(Ran::Exited));
    assert_eq!(out, Tried::Copied("wl-copy"));
    assert_eq!(asked, [("wl-copy ".to_string(), "the text".to_string())]);
    let (out, asked) = tried(&["DISPLAY"], &["xclip", "xsel"], Ok(Ran::Exited));
    assert_eq!(out, Tried::Copied("xclip"));
    assert_eq!(asked.len(), 1);
    let (out, asked) = tried(&["DISPLAY"], &["xsel"], Ok(Ran::Exited));
    assert_eq!(out, Tried::Copied("xsel"));
    assert_eq!(asked[0].0, "xsel --clipboard --input");
}

/// No display tries nothing; a display with no tool installed, and a tool that fails, say why.
#[test]
fn no_display_no_tool_and_a_failing_tool_are_told_apart() {
    let (out, asked) = tried(&[], &["wl-copy", "xclip"], Ok(Ran::Exited));
    assert_eq!(out, Tried::NoDisplay);
    assert!(asked.is_empty(), "a tool was run with no display");
    let (out, asked) = tried(&["DISPLAY"], &[], Ok(Ran::Exited));
    assert_eq!(
        out,
        Tried::Failed("no clipboard tool is installed (xclip, xsel)".to_string())
    );
    assert!(asked.is_empty());
    let (out, _) = tried(
        &["WAYLAND_DISPLAY"],
        &["wl-copy"],
        Err("exited with 1".into()),
    );
    assert_eq!(
        out,
        Tried::Failed("wl-copy did not run: exited with 1".to_string())
    );
}

/// The note says copied only for a tool that ran; the sequence alone is "sent", with the way
/// round it where no tool was tried, and a failed tool's reason after it.
#[test]
fn the_note_never_claims_a_copy_the_sequence_alone_made() {
    let hand = "Ctrl+click or right-click the link";
    assert_eq!(
        note("link", &Tried::Copied("wl-copy"), None, hand),
        "link copied with wl-copy"
    );
    let alone = note("link", &Tried::NoDisplay, None, hand);
    assert_eq!(
        alone,
        "link sent to the terminal's clipboard (OSC 52) — if your terminal ignores it, \
         Ctrl+click or right-click the link"
    );
    assert!(!alone.contains("copied"), "{alone}");
    let failed = note(
        "code",
        &Tried::Failed("xclip did not run: boom".into()),
        None,
        "",
    );
    assert_eq!(
        failed,
        "code sent to the terminal's clipboard (OSC 52); xclip did not run: boom"
    );
    assert!(!failed.contains("copied"));
    let unwritten = note(
        "code",
        &Tried::NoDisplay,
        Some("broken pipe"),
        "read it off the box",
    );
    assert!(unwritten.starts_with("code could not be sent to the terminal (broken pipe)"));
}

/// A program is on the path when an executable file of its name is in one of its directories.
#[test]
fn on_path_finds_an_executable_file_in_a_directory_of_the_path() {
    let path =
        |value: &'static str| move |name: &str| (name == "PATH").then(|| OsString::from(value));
    assert!(on_path(&path("/nonexistent:/bin:/usr/bin"), "sh"));
    assert!(!on_path(&path("/nonexistent"), "sh"));
    assert!(!on_path(&path("/bin:/usr/bin"), "no-such-program-here"));
    assert!(!on_path(&|_| None, "sh"));
}

/// A tool is given the text on its input, a tool that fails is reported with its status, a
/// missing one says it could not be run, and one that keeps running is taken to hold the
/// selection.
#[test]
fn a_tool_is_run_with_the_text_on_its_input() {
    let moment = Duration::from_secs(5);
    let dir = std::env::temp_dir().join(format!("svolder-clip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");
    let script = format!("cat > {}", out.display());
    assert_eq!(
        run_program(Path::new("/bin/sh"), &["-c", &script], "hello", moment),
        Ok(Ran::Exited)
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "hello");
    let why = run_program(Path::new("/bin/sh"), &["-c", "exit 3"], "x", moment).unwrap_err();
    assert!(why.contains("exited with"), "{why}");
    let why = run_program(Path::new("/nonexistent/tool"), &[], "x", moment).unwrap_err();
    assert!(why.contains("could not be run"), "{why}");
    let started = Instant::now();
    let held = run_program(
        Path::new("/bin/sh"),
        &["-c", "cat >/dev/null; sleep 3"],
        "x",
        Duration::from_millis(100),
    );
    assert_eq!(held, Ok(Ran::Running));
    assert!(started.elapsed() < Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A tool still running when the wait ends is handed the text, not reported as having copied it.
#[test]
fn a_tool_still_running_is_handed_the_text_not_credited_with_a_copy() {
    let (out, _) = tried(&["DISPLAY"], &["xclip"], Ok(Ran::Running));
    assert_eq!(out, Tried::Handed("xclip"));
    let said = note("link", &out, None, "");
    assert_eq!(said, "link handed to xclip, which has not said it took it");
    assert!(!said.contains("copied"), "{said}");
}
