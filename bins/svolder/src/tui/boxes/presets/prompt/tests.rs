// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use crossterm::event::KeyCode;
use svolder_profiles::EqMode;

use crate::test_support::{TempDir, temp_dir};
use crate::tui::test_support::presets::{PEQ, imported, resting, step};
use crate::tui::test_support::{press_from as press, type_text};

use super::*;

/// A home folder of this case's own, holding what the cases complete and import.
fn home() -> TempDir {
    let dir = temp_dir("prompt");
    let root = &dir.0;
    for file in [
        "HD650 ParametricEQ.txt",
        "HD600 ParametricEQ.txt",
        "Loud.TXT",
        "notes.md",
        ".hidden.txt",
        "Docs/inside.txt",
        "Music/a.txt",
        "Music2/b.txt",
        ".config/c.txt",
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder is made");
        std::fs::write(path, PEQ).expect("a file is written");
    }
    for n in 1..=9 {
        let many = root.join("many");
        std::fs::create_dir_all(&many).expect("a folder is made");
        std::fs::write(many.join(format!("f{n}.txt")), PEQ).expect("a file is written");
    }
    dir
}

/// The list over `dir` as `~`, open on the file prompt.
fn prompting(dir: &Path) -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('i'));
    let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_mut()
    else {
        panic!("the prompt did not open");
    };
    prompt.home = Some(dir.to_path_buf());
    app
}

/// The prompt, which the step must be.
fn prompt_of(app: &App) -> &FilePrompt {
    match step(app) {
        Some(Step::Prompting(prompt)) => prompt,
        other => panic!("the list is not prompting: {other:?}"),
    }
}

/// Replace the path with `text` and press `Tab`.
fn tab_on(app: &mut App, text: &str) {
    if let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        prompt.text = text.to_string();
        prompt.tab = None;
    }
    press(app, KeyCode::Tab);
}

/// `i` opens the prompt at `~/`, `Esc` goes back to the list, and the box swallows every key.
#[test]
fn i_opens_the_prompt_at_the_home_folder_and_esc_goes_back() {
    let dir = home();
    let mut app = prompting(&dir.0);
    assert_eq!(prompt_of(&app).text, "~/");
    type_text(&mut app, "ab");
    assert_eq!(prompt_of(&app).text, "~/ab");
    press(&mut app, KeyCode::Backspace);
    assert_eq!(prompt_of(&app).text, "~/a");
    assert!(press(&mut app, KeyCode::Up).is_empty());
    press(&mut app, KeyCode::Esc);
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// One match completes in full, a folder gaining its `/`; the match is of the file name as it is
/// typed, and the folder's contents are completed through the next `Tab`.
#[test]
fn one_match_completes_fully_and_a_folder_gains_a_slash() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/HD65");
    assert_eq!(prompt_of(&app).text, "~/HD650 ParametricEQ.txt");
    assert!(prompt_of(&app).listing().is_none());
    tab_on(&mut app, "~/Do");
    assert_eq!(prompt_of(&app).text, "~/Docs/");
    press(&mut app, KeyCode::Tab);
    assert_eq!(prompt_of(&app).text, "~/Docs/inside.txt");
    tab_on(&mut app, "~/Lo");
    assert_eq!(
        prompt_of(&app).text,
        "~/Loud.TXT",
        "an upper-case extension is offered"
    );
}

/// Several matches complete to their common prefix and are listed; a further `Tab` steps through
/// them, round to the first again, and an edit forgets them.
#[test]
fn several_matches_complete_to_the_common_prefix_and_tab_steps_through_them() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/HD");
    assert_eq!(prompt_of(&app).text, "~/HD6");
    let (shown, more, at) = prompt_of(&app)
        .listing()
        .expect("the candidates are listed");
    assert_eq!(shown, ["HD600 ParametricEQ.txt", "HD650 ParametricEQ.txt"]);
    assert_eq!((more, at), (0, None));

    press(&mut app, KeyCode::Tab);
    assert_eq!(prompt_of(&app).text, "~/HD600 ParametricEQ.txt");
    assert_eq!(
        prompt_of(&app).listing().map(|(_, _, at)| at),
        Some(Some(0))
    );
    press(&mut app, KeyCode::Tab);
    assert_eq!(prompt_of(&app).text, "~/HD650 ParametricEQ.txt");
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        prompt_of(&app).text,
        "~/HD600 ParametricEQ.txt",
        "the steps did not wrap"
    );

    press(&mut app, KeyCode::Backspace);
    assert!(
        prompt_of(&app).listing().is_none(),
        "an edit kept the candidates"
    );

    tab_on(&mut app, "~/Mu");
    assert_eq!(prompt_of(&app).text, "~/Music");
    let (shown, _, _) = prompt_of(&app).listing().expect("two folders");
    assert_eq!(shown, ["Music/", "Music2/"]);
}

/// The candidates are six lines, then `… N more`; stepping past the sixth scrolls the window to
/// keep the one stepped to in view.
#[test]
fn the_list_is_six_lines_and_then_how_many_more() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/many/");
    let (shown, more, _) = prompt_of(&app).listing().expect("nine candidates");
    assert_eq!(
        shown,
        ["f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt", "f6.txt"]
    );
    assert_eq!(more, 3);
    for _ in 0..7 {
        press(&mut app, KeyCode::Tab);
    }
    assert_eq!(prompt_of(&app).text, "~/many/f7.txt");
    let (shown, more, at) = prompt_of(&app).listing().expect("still listed");
    assert_eq!(shown.len(), LISTED);
    assert!(shown.contains(&"f7.txt"));
    assert_eq!(shown[at.expect("a candidate is stepped to")], "f7.txt");
    assert_eq!(more, 2);
}

/// Only folders and `.txt` files are offered, and hidden entries only once the typed part starts
/// with a dot.
#[test]
fn only_folders_and_txt_files_are_offered_and_hidden_ones_after_a_dot() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/");
    let (shown, more, _) = prompt_of(&app).listing().expect("candidates");
    assert_eq!(more, 1);
    assert_eq!(
        shown,
        [
            "Docs/",
            "HD600 ParametricEQ.txt",
            "HD650 ParametricEQ.txt",
            "Loud.TXT",
            "many/",
            "Music/"
        ]
    );
    let all = &prompt_of(&app).tab.as_ref().expect("candidates").names;
    assert!(
        all.iter()
            .all(|name| !name.starts_with('.') && name != "notes.md"),
        "{all:?}"
    );

    tab_on(&mut app, "~/.");
    let (shown, _, _) = prompt_of(&app).listing().expect("hidden candidates");
    assert_eq!(shown, [".config/", ".hidden.txt"]);
}

/// A file name that is not UTF-8, which the prompt's line cannot hold, is never offered: the
/// name beside it completes on its own, and one with nothing else beside it matches nothing.
#[test]
fn a_name_that_is_not_utf8_is_not_offered() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = home();
    for name in [&b"Bad\xff.txt"[..], b"Weird\xfe.txt"] {
        std::fs::write(dir.0.join(OsStr::from_bytes(name)), PEQ).expect("a file is written");
    }
    std::fs::write(dir.0.join("Bad ok.txt"), PEQ).expect("a file is written");
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/Bad");
    assert_eq!(prompt_of(&app).text, "~/Bad ok.txt");
    assert!(prompt_of(&app).listing().is_none());
    tab_on(&mut app, "~/Weird");
    assert_eq!(prompt_of(&app).text, "~/Weird");
    assert_eq!(prompt_of(&app).note.as_deref(), Some("no match"));
}

/// A folder that cannot be read is a line under the prompt, and a path that matches nothing
/// says so; neither panics nor changes the path.
#[test]
fn a_folder_that_cannot_be_read_is_said_under_the_prompt() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/missing/x");
    let note = prompt_of(&app).note.clone().expect("a note");
    assert!(note.starts_with("cannot read ~/missing/: "), "{note}");
    assert_eq!(prompt_of(&app).text, "~/missing/x");

    tab_on(&mut app, "~/zzz");
    assert_eq!(prompt_of(&app).note.as_deref(), Some("no match"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = dir.0.join("locked");
        std::fs::create_dir_all(&locked).expect("a folder");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
            .expect("the mode is set");
        tab_on(&mut app, "~/locked/");
        // A superuser reads it anyway; nobody panics either way.
        let unreadable = std::fs::read_dir(&locked).is_err();
        assert_eq!(prompt_of(&app).note.is_some(), unreadable);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))
            .expect("the mode is put back");
    }

    // With no home folder at all, `~` says so instead of reading the current folder.
    if let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        prompt.home = None;
    }
    tab_on(&mut app, "~/a");
    assert_eq!(
        prompt_of(&app).note.as_deref(),
        Some("no home folder to put ~ in")
    );
}

/// `Enter` imports the file as a preset named for it, without its extension and its
/// ` ParametricEQ`, written and not loaded; a name taken opens the prompt with `NAME 2`.
#[test]
fn enter_imports_the_file_and_a_taken_name_opens_the_prompt_with_a_number() {
    let dir = home();
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/HD650 P");
    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::SavePreset(saved)] = intents.as_slice() else {
        panic!("expected one preset to be saved, got {intents:?}");
    };
    assert_eq!(saved.name, "HD650");
    assert_eq!(
        saved.source.as_ref().map(|source| source.origin.clone()),
        Some(None)
    );
    assert_eq!(step(&app), Some(&Step::Choosing));
    assert_eq!(app.presets.loaded_preset, None);
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(4)
    );

    let mut app = prompting(&dir.0);
    app.presets.saved = vec![imported("HD650"), imported("HD650 2")];
    tab_on(&mut app, "~/HD650 P");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    let Some(Step::Naming(line)) = step(&app) else {
        panic!("the name prompt did not open");
    };
    assert_eq!(line.text, "HD650 3");
}

/// A file whose name is a preset's already and as long as a name may be opens the name prompt,
/// prefilled with `NAME 2` cut to fit: the search for a free number stops at the first candidate
/// no preset goes by, however long.
#[test]
fn a_taken_name_as_long_as_a_name_may_be_opens_the_prompt_prefilled() {
    let dir = home();
    let long = "L".repeat(svolder_profiles::tone::preset::NAME_MAX);
    std::fs::write(dir.0.join(format!("{long} ParametricEQ.txt")), PEQ).expect("a file is written");
    let mut app = prompting(&dir.0);
    app.presets.saved = vec![imported(&long)];
    tab_on(&mut app, &format!("~/{long} P"));
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    let Some(Step::Naming(line)) = step(&app) else {
        panic!("the name prompt did not open");
    };
    assert_eq!(
        line.text, long,
        "NAME 2 cut to the longest name a preset takes"
    );
}

/// A file the parser refuses is a warning toast with the line; so is a path that is no file.
#[test]
fn a_parse_refusal_is_a_warning_toast_with_the_line() {
    let dir = home();
    std::fs::write(
        dir.0.join("bad.txt"),
        "Preamp: -3 dB\nFilter 1: ON PK Fc loud Hz Gain 2 dB Q 1\n",
    )
    .expect("a file is written");
    let mut app = prompting(&dir.0);
    tab_on(&mut app, "~/bad.txt");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    let said = app.toast().map(|t| t.text.clone()).expect("a toast");
    assert!(
        said.contains("bad.txt") && said.contains("line 2"),
        "{said}"
    );
    assert!(
        matches!(step(&app), Some(Step::Prompting(_))),
        "the prompt closed"
    );

    type_text(&mut app, "x");
    if let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        prompt.text = "~/Docs".to_string();
    }
    press(&mut app, KeyCode::Enter);
    let said = app.toast().map(|t| t.text.clone()).expect("a toast");
    assert!(said.contains("cannot read"), "{said}");
}
