// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wizard as plain prompts on a terminal.
//!
//! One step at a time: the title, whatever the step needs the reader to know, the choices as
//! numbers, and a line read back. It is what `svolder add` with no unit id and the `svolder
//! setup` fix run, and what the wizard falls back to where a TUI cannot be started.
//!
//! Everything it says goes to standard error, where every other question this binary asks goes:
//! a conversation is not a verb's output, and a run whose standard output is being read still
//! asks where the user can see it.

use std::io::Write;
use std::process::ExitCode;

use super::{Effects, Prompt, Step, Wizard};

/// What a typed line turned out to be.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// The choice at this index, which an empty line takes the default of.
    Choice(usize),
    /// The line itself, for a step that is answered by typing rather than by choosing.
    Text(String),
    /// The step before this one.
    Back,
    /// Nothing this prompt can take.
    Unreadable,
}

/// Walk the wizard to its last step, or to wherever the user stops answering.
///
/// The run ends with success when the wizard reaches its last step, with 1 when standard input
/// ends first — a wizard that was left half-walked has done everything it said it had, but it
/// did not finish — and with an effect's own code when the machine would not do what a step
/// asked of it. A refused answer is said and the same step is asked again, so a name another
/// device carries or a passphrase the device turned away costs the answer and nothing else.
pub async fn run<E: Effects>(wizard: Wizard<E>) -> ExitCode {
    run_reading(wizard, read_line).await
}

/// [`run`], reading each answer from `read`.
async fn run_reading<E: Effects>(
    mut wizard: Wizard<E>,
    mut read: impl FnMut() -> Option<String>,
) -> ExitCode {
    let code = walk(&mut wizard, &mut read).await;
    // However the walk ended, a device it wrote is the daemon's to know of.
    wizard.tell_the_daemon().await;
    code
}

/// The loop over the wizard's steps.
async fn walk<E: Effects>(
    wizard: &mut Wizard<E>,
    read: &mut impl FnMut() -> Option<String>,
) -> ExitCode {
    loop {
        // What the next step spends its seconds on is said before they are spent, so a wait is
        // explained while it is being waited out rather than after.
        if let Some(busy) = wizard.busy() {
            eprintln!("svolder: {busy}…");
        }
        let Some(prompt) = wizard.prompt().await else {
            break;
        };
        if prompt.step == Step::Done {
            // The last step says what was written; where to go from here is this front end's
            // own to add, since it is handing the terminal back rather than opening anything.
            // It reads as the last thing the step says, above the choice that ends the run.
            let mut ending = prompt;
            ending
                .body
                .push(super::next_command(wizard.added().is_some()).to_string());
            show(&ending);
            return ExitCode::SUCCESS;
        }
        show(&prompt);
        let Some(line) = read() else {
            eprintln!("svolder: nothing left to read; setup stopped");
            return ExitCode::from(1);
        };
        let answered = match answer_for(&prompt, &line) {
            Answer::Choice(at) => wizard.answer(at, None).await,
            Answer::Text(text) => wizard.answer(0, Some(text)).await,
            Answer::Back => {
                wizard.back();
                continue;
            }
            Answer::Unreadable => {
                eprintln!(
                    "svolder: type the number of one of the choices above, or `b` to go back"
                );
                continue;
            }
        };
        if let Err(failure) = answered {
            eprintln!("svolder: {failure}");
        }
    }
    if wizard.is_later() {
        eprintln!("{}", super::OPEN_AGAIN);
        return ExitCode::SUCCESS;
    }
    // A run the machine stopped leaves with the code that effect earned; one that reached the
    // last step leaves with success.
    ExitCode::from(wizard.stopped().unwrap_or(0))
}

/// Put one prompt on the screen: the title, its body, and the choices as numbers.
fn show(prompt: &Prompt) {
    eprintln!();
    eprintln!("{}", prompt.title);
    for line in &prompt.body {
        eprintln!("{line}");
    }
    for (at, choice) in prompt.choices.iter().enumerate() {
        eprintln!("  {}) {choice}", at + 1);
    }
}

/// What a typed line means to this prompt: the choice it numbers, the text it is, or the step
/// before this one.
///
/// A prompt with no choices takes a line of text, empty or not (an empty one is a valid answer
/// there, e.g. the name the device advertises, or a device with no passphrase), so only a
/// prompt that offers choices reads `b` as going back. A prompt with choices takes a number, or
/// an empty line for its default; anything else is not an answer to it.
fn answer_for(prompt: &Prompt, line: &str) -> Answer {
    if prompt.choices.is_empty() {
        return Answer::Text(line.trim_end_matches('\n').to_string());
    }
    let typed = line.trim();
    if typed.is_empty() {
        return Answer::Choice(prompt.default);
    }
    if typed.eq_ignore_ascii_case("b") || typed.eq_ignore_ascii_case("back") {
        return Answer::Back;
    }
    match typed.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
        Some(at) if at < prompt.choices.len() => Answer::Choice(at),
        _ => Answer::Unreadable,
    }
}

/// One line from standard input, or nothing when there is no more of it.
fn read_line() -> Option<String> {
    // The cursor sits after the marker rather than on the line below it, which is where a
    // prompt leaves it everywhere else in this binary.
    eprint!("> ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wizard::fake::{FakeEffects, seen_product, walk_to};

    /// Input that ends on the Adapter step, after the device was written, still tells a running
    /// daemon of it as the run ends.
    #[tokio::test]
    async fn input_ending_on_the_adapter_step_tells_the_daemon() {
        let fx = FakeEffects::healthy()
            .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
            .with_adapters([("hci0", 2), ("hci1", 5)]);
        let mut wizard = Wizard::new(fx.clone());
        walk_to(&mut wizard, Step::Name).await;
        wizard
            .answer(0, Some("dac".into()))
            .await
            .expect("the device is written");
        let code = run_reading(wizard, || None).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(1)));
        assert_eq!(fx.reloads(), 1, "the daemon was told of the device once");
    }

    /// A prompt with `choices` and `default`, for a case about what an answer means.
    fn prompt(choices: &[&str], default: usize) -> Prompt {
        Prompt {
            step: Step::Daemon,
            title: "The background service".into(),
            body: Vec::new(),
            choices: choices.iter().map(|c| (*c).to_string()).collect(),
            default,
        }
    }

    /// An empty line takes the default, a number takes the choice it names, `b` goes back, and
    /// a number that names no choice is not an answer at all.
    #[test]
    fn a_number_takes_the_choice_it_names_and_an_empty_line_takes_the_default() {
        let offered = prompt(&["Install the user service", "Skip"], 1);
        assert_eq!(answer_for(&offered, "\n"), Answer::Choice(1));
        assert_eq!(answer_for(&offered, "1\n"), Answer::Choice(0));
        assert_eq!(answer_for(&offered, " 2 \n"), Answer::Choice(1));
        assert_eq!(answer_for(&offered, "b\n"), Answer::Back);
        assert_eq!(answer_for(&offered, "Back\n"), Answer::Back);
        assert_eq!(answer_for(&offered, "3\n"), Answer::Unreadable);
        assert_eq!(answer_for(&offered, "0\n"), Answer::Unreadable);
        assert_eq!(answer_for(&offered, "yes\n"), Answer::Unreadable);
    }

    /// A prompt that offers no choices takes the line as it was typed, whatever it says: an
    /// empty line is an answer there rather than a refusal to give one, and so is a device
    /// someone wants to call `b`.
    #[test]
    fn a_prompt_with_no_choices_takes_the_line_itself() {
        let typed = Prompt {
            step: Step::Name,
            title: "What to call it".into(),
            body: vec!["Gungnir 2".into()],
            choices: Vec::new(),
            default: 0,
        };
        assert_eq!(answer_for(&typed, "dac\n"), Answer::Text("dac".into()));
        assert_eq!(answer_for(&typed, "\n"), Answer::Text(String::new()));
        assert_eq!(answer_for(&typed, "b\n"), Answer::Text("b".into()));
    }
}
