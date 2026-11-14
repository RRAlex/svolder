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

use super::{Effects, Prompt, Step, Takes, Wizard};

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
///
/// A signal that leaves a walk the daemon runs (Ctrl-C here, a closed terminal) cancels this run's
/// own walk and exits 130: the line being read sits in a thread that cannot be stopped, so the
/// process leaves by itself rather than wait for it.
pub async fn run<E: Effects>(wizard: Wizard<E>) -> ExitCode {
    let (code, interrupted) = run_reading(wizard, |secret| async move {
        tokio::task::spawn_blocking(move || read_line(secret))
            .await
            .ok()
            .flatten()
    })
    .await;
    if interrupted {
        std::process::exit(i32::from(INTERRUPTED));
    }
    code
}

/// The code of a run a signal ended.
const INTERRUPTED: u8 = 130;

/// [`run`], reading each answer from `read` (which is told whether the answer is a secret, to be
/// typed without echo), and whether a signal ended it.
async fn run_reading<E: Effects, F: Future<Output = Option<String>>>(
    mut wizard: Wizard<E>,
    mut read: impl FnMut(bool) -> F,
) -> (ExitCode, bool) {
    let presses = wizard.presses();
    let pressed = async {
        match &presses {
            Some(presses) => presses.next().await,
            None => std::future::pending().await,
        }
    };
    let (code, interrupted) = tokio::select! {
        code = walk(&mut wizard, &mut read) => (code, false),
        () = pressed => {
            eprintln!();
            eprintln!("svolder: interrupted; the walk is cancelled");
            // The thread reading a hidden line cannot be stopped and the process leaves without
            // running its destructors, so the terminal's echo is put back here.
            Echo::restore_held();
            (ExitCode::from(INTERRUPTED), true)
        }
    };
    // However the walk ended, a walk the daemon still runs for it is cancelled, and a device it
    // wrote is the daemon's to know of.
    wizard.leave().await;
    (code, interrupted)
}

/// The loop over the wizard's steps.
async fn walk<E: Effects, F: Future<Output = Option<String>>>(
    wizard: &mut Wizard<E>,
    read: &mut impl FnMut(bool) -> F,
) -> ExitCode {
    loop {
        // What the next step spends its seconds on is said before they are spent, so a wait is
        // explained while it is being waited out rather than after.
        if let Some(busy) = wizard.busy() {
            eprintln!("svolder: {busy}…");
        }
        let fresh = wizard.stopped().is_none();
        let Some(prompt) = wizard.prompt().await else {
            // The daemon's walk could not be had, or went away: say why.
            if fresh && let Some(why) = wizard.stopped_why() {
                eprintln!("svolder: {why}");
            }
            break;
        };
        if wizard.has_given_answer(&prompt) {
            if let Err(failure) = wizard.answer_given(&prompt).await {
                eprintln!("svolder: {failure}");
            }
            continue;
        }
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
        let Some(line) = read(prompt.takes == Takes::Secret).await else {
            eprintln!("svolder: nothing left to read; setup stopped");
            return ExitCode::from(1);
        };
        let answered = match answer_for(&prompt, &line) {
            Answer::Choice(at) => wizard.answer(at, None).await,
            Answer::Text(text) => wizard.answer(0, Some(text)).await,
            Answer::Back => {
                wizard.back().await;
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

/// One line from standard input, or nothing when there is no more of it. A `secret` (a typed
/// pairing code) is typed without echo when standard input is a terminal, and the newline the
/// hidden Enter did not show is printed after it.
fn read_line(secret: bool) -> Option<String> {
    // The cursor sits after the marker rather than on the line below it, which is where a
    // prompt leaves it everywhere else in this binary.
    eprint!("> ");
    let _ = std::io::stderr().flush();
    let hidden = if secret { Echo::off(0) } else { None };
    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);
    if hidden.is_some() {
        eprintln!();
    }
    match read {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

/// The terminal whose echo is off now, and its settings from before: held process-wide so the exit
/// path can put them back while the reading thread still stands in `read_line`.
static HELD: std::sync::Mutex<Option<(i32, libc::termios)>> = std::sync::Mutex::new(None);

/// A terminal with its echo off, put back as it was when this is dropped.
pub(crate) struct Echo {
    fd: i32,
    was: libc::termios,
}

impl Echo {
    /// Turn echo off on `fd`; nothing when it is no terminal (a pipe's lines show nowhere).
    pub(crate) fn off(fd: i32) -> Option<Echo> {
        // SAFETY: `was` is only read after tcgetattr filled it in, and both calls take a plain
        // file descriptor and a struct that lives for the call.
        unsafe {
            let mut was = std::mem::zeroed::<libc::termios>();
            if libc::tcgetattr(fd, &mut was) != 0 {
                return None;
            }
            let mut quiet = was;
            quiet.c_lflag &= !libc::ECHO;
            if libc::tcsetattr(fd, libc::TCSANOW, &quiet) != 0 {
                return None;
            }
            *HELD.lock().unwrap_or_else(|e| e.into_inner()) = Some((fd, was));
            Some(Echo { fd, was })
        }
    }

    /// Put back the echo of a terminal that is hidden now, if one is. Safe to call from any thread
    /// and again after the drop, which then finds nothing held.
    pub(crate) fn restore_held() {
        let held = HELD.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((fd, was)) = held {
            // SAFETY: `was` came from tcgetattr on this descriptor.
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &was) };
        }
    }
}

impl Drop for Echo {
    fn drop(&mut self) {
        HELD.lock().unwrap_or_else(|e| e.into_inner()).take();
        // SAFETY: `was` came from tcgetattr on this descriptor.
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.was) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wizard::Takes;
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
        let (code, _) = run_reading(wizard, |_| async { None }).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(1)));
        assert_eq!(fx.reloads(), 1, "the daemon was told of the device once");
    }

    /// A signal at the prompt (Ctrl-C, a closed terminal) ends the run with 130 and cancels this
    /// client's own walk once, though the read of the line never returns.
    #[tokio::test]
    async fn a_signal_at_the_prompt_cancels_the_walk_this_run_started() {
        use crate::commands::interrupts::Presses;
        use crate::test_support::stub_daemon::stub_daemon;
        use crate::test_support::walk_script::{JOB, choice_prompt, waiting};

        let daemon = stub_daemon().await;
        daemon.lists_features(&["walk"]);
        let script = daemon.walk();
        script.starts_with(vec![waiting(
            choice_prompt(Step::Scan, "Listen for devices", &["Scan"]),
            false,
        )]);
        let mut wizard =
            Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Default::default());
        wizard
            .attach(&daemon.socket)
            .await
            .expect("the daemon walks");
        let presses = std::sync::Arc::new(Presses::new());
        wizard.leaving_by(std::sync::Arc::clone(&presses));
        // The press arrives while the run waits for a line that never comes.
        let (code, interrupted) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            presses.press();
            run_reading(wizard, |_| std::future::pending::<Option<String>>()).await
        })
        .await
        .expect("the run ends on the press");
        assert!(interrupted);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(130)));
        assert_eq!(script.cancelled(), [JOB]);
    }

    /// A prompt with `choices` and `default`, for a case about what an answer means.
    fn prompt(choices: &[&str], default: usize) -> Prompt {
        Prompt {
            step: Step::Daemon,
            title: "The background service".into(),
            body: Vec::new(),
            choices: choices.iter().map(|c| (*c).to_string()).collect(),
            default,
            takes: Takes::Choice,
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
            takes: Takes::Text,
        };
        assert_eq!(answer_for(&typed, "dac\n"), Answer::Text("dac".into()));
        assert_eq!(answer_for(&typed, "\n"), Answer::Text(String::new()));
        assert_eq!(answer_for(&typed, "b\n"), Answer::Text("b".into()));
    }

    /// A pty pair: the master's end and the slave's descriptor.
    fn pty() -> (std::fs::File, i32) {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        // SAFETY: openpty fills the two descriptors; the null pointers ask for no name, termios
        // or size.
        let made = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(made, 0, "a pty");
        // SAFETY: `master` is an open descriptor nothing else owns.
        (unsafe { std::fs::File::from_raw_fd(master) }, slave)
    }

    /// What the master has been sent back so far, without waiting for more.
    fn echoed(master: &std::fs::File) -> String {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let mut seen = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            let mut ready = libc::pollfd {
                fd: master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one pollfd that lives for the call, a zero timeout.
            if unsafe { libc::poll(&mut ready, 1, 200) } <= 0 {
                break;
            }
            let Ok(n) = (&*master).read(&mut buf) else {
                break;
            };
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&seen).into_owned()
    }

    /// A typed pairing code is not echoed back to the terminal while the prompt reads it, and the
    /// terminal echoes again once it is read; an ordinary answer is echoed as it always was.
    #[test]
    fn a_secret_is_typed_without_echo_and_the_echo_comes_back() {
        use std::io::Write;
        let (mut master, slave) = pty();
        master.write_all(b"plain\n").unwrap();
        assert!(
            echoed(&master).contains("plain"),
            "the control: a terminal with echo on shows what is typed"
        );
        let hidden = Echo::off(slave).expect("a terminal's echo goes off");
        master.write_all(b"s3cr3t-1234\n").unwrap();
        assert!(
            !echoed(&master).contains("s3cr3t"),
            "the typed code was echoed in clear"
        );
        drop(hidden);
        master.write_all(b"after\n").unwrap();
        assert!(echoed(&master).contains("after"), "the echo is back");
        // SAFETY: `slave` is the pty's own descriptor, closed once.
        unsafe { libc::close(slave) };
    }

    /// The prompt asks for a hidden line at the code's step, and for a plain one at the name's.
    #[tokio::test]
    async fn the_code_step_asks_for_a_hidden_line_and_the_name_step_does_not() {
        let asked_at = async |step: Step| {
            let fx = FakeEffects::healthy()
                .with_scan(vec![seen_product("0102030405060708", "Bifrost 3", -40)])
                .refusing_without("1234");
            let mut wizard = Wizard::new(fx);
            walk_to(&mut wizard, Step::Name).await;
            if step == Step::Passphrase {
                wizard.answer(0, Some("dac".into())).await.unwrap();
            }
            let asked = std::cell::RefCell::new(Vec::new());
            let _ = run_reading(wizard, |secret| {
                asked.borrow_mut().push(secret);
                async { None }
            })
            .await;
            asked.into_inner()
        };
        assert_eq!(asked_at(Step::Name).await, [false]);
        assert_eq!(asked_at(Step::Passphrase).await, [true]);
    }

    /// A press while a thread is reading a hidden line leaves the terminal with its echo back on,
    /// though that thread never returns from the read (the process exits with it still standing).
    #[tokio::test]
    async fn a_press_at_the_hidden_prompt_puts_the_echo_back() {
        use crate::commands::interrupts::Presses;
        use crate::test_support::stub_daemon::stub_daemon;
        use crate::test_support::walk_script::{choice_prompt, waiting};
        use std::io::Write;

        let daemon = stub_daemon().await;
        daemon.lists_features(&["walk"]);
        daemon.walk().starts_with(vec![waiting(
            choice_prompt(Step::Scan, "Listen for devices", &["Scan"]),
            false,
        )]);
        let mut wizard =
            Wizard::starting_at(FakeEffects::healthy(), Step::Scan, Default::default());
        wizard.attach(&daemon.socket).await.unwrap();
        let presses = std::sync::Arc::new(Presses::new());
        wizard.leaving_by(std::sync::Arc::clone(&presses));
        let (mut master, slave) = pty();
        // The reader hides the echo and then never comes back, as a thread blocked in read does.
        let hidden = std::sync::Arc::new(std::sync::Mutex::new(None));
        let held = std::sync::Arc::clone(&hidden);
        let (code, interrupted) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            run_reading(wizard, |_| {
                *held.lock().unwrap() = Echo::off(slave);
                presses.press();
                std::future::pending::<Option<String>>()
            })
            .await
        })
        .await
        .expect("the run ends on the press");
        assert!(interrupted);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(130)));
        master.write_all(b"visible\n").unwrap();
        assert!(
            echoed(&master).contains("visible"),
            "the terminal was left with its echo off"
        );
        // The reader's own guard comes down late and harmlessly.
        drop(hidden.lock().unwrap().take());
        // SAFETY: `slave` is the pty's own descriptor, closed once.
        unsafe { libc::close(slave) };
    }
}
