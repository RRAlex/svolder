// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// An asker that fails the test when it is asked.
fn never(_: &str) -> std::io::Result<bool> {
    panic!("nobody was to be asked");
}

#[test]
fn yes_answers_without_asking_and_without_a_terminal() {
    assert!(decide("Move it? [y/N]", true, false, never).unwrap());
    assert!(decide("Move it? [y/N]", true, true, never).unwrap());
}

#[test]
fn without_a_terminal_and_without_yes_it_refuses_and_names_the_flag() {
    let failure = decide("Move it? [y/N]", false, false, never).unwrap_err();
    assert_eq!(failure.code, 1);
    assert!(failure.what.contains("Move it? [y/N]"), "{}", failure.what);
    assert!(failure.what.contains("--yes"), "{}", failure.what);
}

#[test]
fn a_terminal_is_asked_and_its_answer_is_the_answer() {
    let mut asked = Vec::new();
    let said = |question: &str| {
        asked.push(question.to_string());
        Ok(true)
    };
    assert!(decide("Move it? [y/N]", false, true, said).unwrap());
    assert_eq!(asked, ["Move it? [y/N]"]);
    assert!(!decide("Move it? [y/N]", false, true, |_| Ok(false)).unwrap());
    let failure = decide("Move it? [y/N]", false, true, |_| {
        Err(std::io::Error::other("the terminal went away"))
    })
    .unwrap_err();
    assert!(failure.what.contains("went away"), "{}", failure.what);
}
