// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What went wrong, and the one step that follows from it.
//!
//! Every failure a device path reports carries both: [`Failure::what`] says what happened and
//! [`Failure::next`] says what to do about it. One table, [`next_step`], answers for the step,
//! so a CLI verb printing `svolder: <what>. <next>` and a TUI showing the two on separate lines
//! tell the user the same thing.
//!
//! The exit code travels with the words: from the [`Kind`] for a failure this binary raises
//! itself, from the daemon's own error response for one it relays, and from [`code_for`] for a
//! [`BleError`] (which keeps a timeout exiting 4 while every other dropped link exits 3).

use std::path::Path;
use std::process::ExitCode;

use svolder_ble::BleError;
use svolder_ipc::NOT_FOUND_YET;

/// What kind of thing went wrong.
///
/// The list covers every failure a device path reports, and a mistyped command, so that every
/// message a device path can emit has a kind to take its step from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The machine has no Bluetooth adapter at all.
    NoAdapter,
    /// The adapter that was named is not one the machine has.
    AdapterMissing,
    /// No device answered within the scan window.
    NothingInRange,
    /// Several devices answered and nothing said which one to take.
    AmbiguousDevice,
    /// The device turned a connection or a write away.
    Refused,
    /// A link that was up went down.
    Dropped,
    /// The daemon answered with an error of its own.
    DaemonError,
    /// The daemon is up, and its configuration names no device for it to serve.
    DaemonHasNoDevice,
    /// A daemon this process tried to start did not come up.
    AutospawnFailed,
    /// The configuration file could not be read.
    ConfigUnreadable,
    /// No profile covers the product that was found.
    UnknownProduct,
    /// Nothing is listening on the daemon's socket.
    NoDaemon,
    /// A flag, a value or an environment knob the command line got wrong.
    Usage,
}

impl Kind {
    /// Every kind, for a caller that walks them all: the table's own test does, and so does
    /// anything that prints the steps rather than one step.
    #[allow(
        dead_code,
        reason = "the table is walked by the test that checks every step"
    )]
    pub const ALL: [Kind; 13] = [
        Kind::NoAdapter,
        Kind::AdapterMissing,
        Kind::NothingInRange,
        Kind::AmbiguousDevice,
        Kind::Refused,
        Kind::Dropped,
        Kind::DaemonError,
        Kind::DaemonHasNoDevice,
        Kind::AutospawnFailed,
        Kind::ConfigUnreadable,
        Kind::UnknownProduct,
        Kind::NoDaemon,
        Kind::Usage,
    ];

    /// The exit code a failure of this kind earns.
    ///
    /// The same three numbers [`code_for`] gives the [`BleError`] each kind stands for: 1 for
    /// something the user can retype, 2 for a device that was not found, 3 for a link or a
    /// daemon that is not there. A daemon that answered with a code of its own keeps that one
    /// instead, through [`Failure::from_daemon`].
    pub fn code(self) -> u8 {
        match self {
            Kind::ConfigUnreadable | Kind::UnknownProduct | Kind::Usage => 1,
            Kind::NothingInRange | Kind::AmbiguousDevice | Kind::DaemonHasNoDevice => 2,
            Kind::NoAdapter
            | Kind::AdapterMissing
            | Kind::Refused
            | Kind::Dropped
            | Kind::DaemonError
            | Kind::AutospawnFailed
            | Kind::NoDaemon => 3,
        }
    }
}

/// How to factory-reset a device, as a clause that follows a colon: the last resort for a lost
/// passphrase, named wherever a refusal says so.
macro_rules! factory_reset {
    () => {
        "power-cycle it while holding its only button, else input-select, else the physically \
         largest (a Mesh Card's is behind the pinhole on its back), which clears every setting, \
         not only the passcode"
    };
}

/// The step after a refusal: give svolder the code, or, when it is lost, `auto`, which has it
/// read the code from the device, afresh once when the first reading is refused; the power
/// cycle and then the factory reset come after that.
const REFUSED: &str = concat!(
    "Give svolder the device's passcode, the one set when the device was added to the app: \
     `svolder setup`, the Devices screen's `u`, or `passphrase` in the config; if it is lost, \
     choose `auto` there and svolder reads the code from the device, afresh once if the first \
     reading is refused; if it is refused still, power-cycle the device, which resets nothing, \
     and only then factory-reset the device: ",
    factory_reset!()
);

/// The step after a refusal of `auto` itself, where there is no `auto` left to offer: svolder
/// has already released the link and read the code afresh once, so the device is power-cycled,
/// and the factory reset is what remains when the code is lost for good.
pub const REFUSED_UNDER_AUTO: &str = concat!(
    "svolder has already read the code afresh once, so power-cycle the device, which resets \
     nothing, and try again; if the code is lost for good, factory-reset the device: ",
    factory_reset!()
);

/// The one step that follows a failure of this kind.
///
/// Each step is a command to run or something to do to the device itself, and each is the only
/// step for its kind: a message that offered two would leave the user choosing between them.
pub fn next_step(kind: Kind) -> &'static str {
    match kind {
        Kind::NoAdapter | Kind::AdapterMissing => "Run `svolder doctor`",
        Kind::NothingInRange => {
            "Power the device on and close the phone app, which holds its only connection, \
             then run `svolder scan` again"
        }
        Kind::AmbiguousDevice => "Pick one with `-d NAME` or set `default_device` in the config",
        Kind::Refused => REFUSED,
        Kind::Dropped => "If it does not come back, run `svolder doctor`",
        Kind::DaemonError => "See `svolder daemon logs`",
        Kind::DaemonHasNoDevice => NO_DEVICE_STEP,
        // The failure's own words name the log file; this names what prints it.
        Kind::AutospawnFailed => "See the log `svolder daemon logs` prints",
        Kind::ConfigUnreadable => "Run `svolder config path`",
        Kind::UnknownProduct => {
            "Run `svolder profiles list`; user profiles live in ~/.config/svolder/profiles"
        }
        Kind::NoDaemon => "Run `svolder daemon install`, or drop `--transport daemon`",
        Kind::Usage => "See `svolder --help`",
    }
}

/// The step after the daemon's "not found yet": it is still looking, so the device is what to
/// check.
const NOT_FOUND_STEP: &str =
    "svolderd keeps looking: check that it is on and in range (`svolder daemon logs`)";

/// The step an error the daemon answered with calls for: a usage step for a request it refused
/// with code 1 (a value it cannot read, a read-only control), the one for a device it has not
/// found yet, else the one every daemon error gets.
pub fn daemon_step(code: u8, message: &str) -> &'static str {
    match code {
        1 => next_step(Kind::Usage),
        3 if message.ends_with(NOT_FOUND_YET) => NOT_FOUND_STEP,
        _ => next_step(Kind::DaemonError),
    }
}

/// The step for a daemon whose configuration names no device, with `SOCKET` standing for the
/// socket that daemon listens on: [`Failure::from_daemon_on`] writes the real one in.
///
/// `add` given that socket writes the device and tells that very daemon to reload, rather than
/// whichever daemon its own environment resolves. What it writes is the configuration under
/// its own `XDG_CONFIG_HOME`, and a daemon reads the file its `--config` or its own
/// `XDG_CONFIG_HOME` names, so the two have to agree.
const NO_DEVICE_STEP: &str = "Add one with `svolder --socket SOCKET add <udid> --name NAME`, \
     which also tells that daemon to reload; a daemon started with `--config` or an \
     `XDG_CONFIG_HOME` of its own reads that file, and `add` writes the one under its own \
     `XDG_CONFIG_HOME`, so give it the daemon's";

/// `path` as a shell reads it back: as it is where every character is one a shell leaves
/// alone, and in single quotes otherwise.
fn shell_word(path: &Path) -> String {
    let text = path.display().to_string();
    let plain = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%=,".contains(c));
    match plain {
        true => text,
        false => format!("'{}'", text.replace('\'', "'\\''")),
    }
}

/// What to do about a frame this build could not read, or a reading the device has never sent:
/// a dropped link's step is no use, since the link is up and the device is talking.
pub const INSPECT: &str = "Run `svolder inspect` to see what the device sends";

/// Why something did not happen, and what to do about it.
///
/// Nothing is printed where a failure is raised: a front end drawing on the alternate screen
/// has no terminal to print on, and one that does prints at its own boundary through
/// [`Failure::report`]. The code travels with the words so that whichever of them a caller
/// uses, the other is the one this failure earned.
#[derive(Debug)]
pub struct Failure {
    /// The exit code this failure earns.
    pub code: u8,
    /// What happened, without the `svolder: ` a CLI verb prints it behind.
    pub what: String,
    /// The one step that follows.
    pub next: String,
}

impl std::fmt::Display for Failure {
    /// What happened and then what to do, as one line: `what. next`. A `what` that already ends
    /// a sentence (`.`, `?` or `!`) gains no full stop.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.what.ends_with(['.', '?', '!']) {
            true => write!(f, "{} {}", self.what, self.next),
            false => write!(f, "{}. {}", self.what, self.next),
        }
    }
}

impl From<svolder_host::eq::EqFailure> for Failure {
    /// The computer EQ's refusal, as a failure with code 1.
    fn from(failure: svolder_host::eq::EqFailure) -> Failure {
        Failure {
            code: 1,
            what: failure.what,
            next: failure.next,
        }
    }
}

impl Failure {
    /// Whether this is the daemon's answer for a device it has never found: code 3, in words
    /// that end with [`NOT_FOUND_YET`].
    pub fn is_not_found_yet(&self) -> bool {
        self.code == 3 && self.what.ends_with(NOT_FOUND_YET)
    }

    /// A failure of `kind`, in the words `detail` gives it, with the code and the step that
    /// kind earns.
    pub fn of(kind: Kind, detail: impl Into<String>) -> Failure {
        Failure {
            code: kind.code(),
            what: detail.into(),
            next: next_step(kind).to_string(),
        }
    }

    /// A direct session's own failure, with the exit code its kind earns and the step that
    /// kind of trouble calls for.
    ///
    /// The code is the error's own rather than the [`Kind`]'s, so a timeout still exits 4 where
    /// every other dropped link exits 3.
    pub fn from_ble(err: &BleError) -> Failure {
        Failure {
            code: code_for(err),
            what: err.to_string(),
            next: ble_next_step(err).to_string(),
        }
    }

    /// A failure the daemon answered with: its own code and its own words, and the step a
    /// daemon error calls for ([`daemon_step`]).
    fn from_daemon(code: u8, message: impl Into<String>) -> Failure {
        let what = message.into();
        let next = daemon_step(code, &what).to_string();
        Failure { code, what, next }
    }

    /// A failure the daemon on `socket` answered with.
    ///
    /// A daemon whose configuration names no device answers every device request with
    /// [`svolder_ipc::NO_DEVICES_CONFIGURED`], which says nothing of which daemon it is or what
    /// to do; this names the daemon by its socket instead, with the step of adding a device to
    /// it (the `add` in that step carries the same socket, so what it writes is what that daemon
    /// is told to reload). Every other answer is [`Failure::from_daemon`]'s. The code is the
    /// daemon's own either way.
    pub fn from_daemon_on(socket: &Path, code: u8, message: impl Into<String>) -> Failure {
        let message = message.into();
        if message != svolder_ipc::NO_DEVICES_CONFIGURED {
            return Failure::from_daemon(code, message);
        }
        Failure {
            code,
            what: format!(
                "the daemon on {} serves no device: its configuration names none",
                socket.display()
            ),
            next: NO_DEVICE_STEP.replace("SOCKET", &shell_word(socket)),
        }
    }

    /// Say this on stderr, as a CLI verb does, and hand back the exit code to leave with.
    pub fn report(self) -> ExitCode {
        eprintln!("svolder: {self}");
        ExitCode::from(self.code)
    }
}

/// The step a [`BleError`] calls for, by what the error says went wrong.
///
/// A frame that could not be encoded or decoded is the one departure from the table: the link
/// is up and the device is talking, so the step is to look at what it is saying rather than at
/// the link.
fn ble_next_step(err: &BleError) -> &'static str {
    match err {
        BleError::NotFound => next_step(Kind::NothingInRange),
        BleError::Adapter(_) => next_step(Kind::AdapterMissing),
        BleError::PassphraseRejected => next_step(Kind::Refused),
        BleError::Connection(_) | BleError::Timeout(_) | BleError::Bootloader => {
            next_step(Kind::Dropped)
        }
        BleError::Proto(_) | BleError::Profile(_) => INSPECT,
    }
}

/// The exit code a [`BleError`] earns, as the number a [`Failure`] carries it as.
pub fn code_for(err: &BleError) -> u8 {
    match err {
        BleError::NotFound => 2,
        BleError::Connection(_)
        | BleError::Bootloader
        | BleError::PassphraseRejected
        | BleError::Adapter(_) => 3,
        BleError::Timeout(_) => 4,
        BleError::Proto(_) | BleError::Profile(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Every kind names a step the user can take: a command to run, or something to do to the
    /// device itself.
    #[test]
    fn every_kind_names_a_next_step_that_is_a_command_or_an_action() {
        for kind in Kind::ALL {
            let next = next_step(kind);
            assert!(!next.is_empty(), "{kind:?}");
            assert!(
                next.contains("svolder ") || next.contains("power") || next.contains("-d "),
                "{kind:?}: {next}"
            );
        }
    }

    /// A failure reads as what happened and then what to do, and keeps the code its kind
    /// earns.
    #[test]
    fn a_failure_prints_what_then_next_and_keeps_its_code() {
        let f = Failure::of(
            Kind::AdapterMissing,
            "no adapter named hci9; adapters: hci0, hci1",
        );
        assert_eq!(f.code, 3);
        assert_eq!(
            f.to_string(),
            "no adapter named hci9; adapters: hci0, hci1. Run `svolder doctor`"
        );
    }

    /// A `what` that already ends in a full stop does not gain a second one.
    #[test]
    fn a_sentence_that_ends_in_a_full_stop_keeps_the_one_it_has() {
        let f = Failure::of(Kind::ConfigUnreadable, "line 4 is not a table.");
        assert_eq!(
            f.to_string(),
            "line 4 is not a table. Run `svolder config path`"
        );
    }

    /// A `what` that ends in a question mark gains no full stop either.
    #[test]
    fn a_question_keeps_its_question_mark() {
        let f = Failure::of(Kind::Usage, "no control named volme; did you mean volume?");
        assert_eq!(
            f.to_string(),
            "no control named volme; did you mean volume? See `svolder --help`"
        );
    }

    /// A BLE failure keeps the exit code the error itself earns, so a timeout still exits 4
    /// where the rest of a dropped link exits 3, and takes the step its kind calls for.
    #[test]
    fn a_ble_failure_keeps_its_own_code_and_takes_its_kind_s_step() {
        let timeout = Failure::from_ble(&BleError::Timeout(Duration::from_secs(2)));
        assert_eq!(timeout.code, 4);
        assert_eq!(timeout.next, next_step(Kind::Dropped));
        let adapter = Failure::from_ble(&BleError::Adapter("no adapter named hci9".into()));
        assert_eq!(adapter.code, Kind::AdapterMissing.code());
        assert_eq!(adapter.next, next_step(Kind::AdapterMissing));
        let refused = Failure::from_ble(&BleError::PassphraseRejected);
        assert_eq!(refused.code, Kind::Refused.code());
        assert_eq!(refused.next, next_step(Kind::Refused));
    }

    /// The step after a refusal offers `auto` before the factory reset, and never `passphrase
    /// set`, which changes the device's code and needs the right one to be accepted.
    #[test]
    fn the_refused_step_offers_auto_before_the_factory_reset() {
        let step = next_step(Kind::Refused);
        let auto = step.find("choose `auto`").expect("it offers auto");
        let cycle = step
            .find("power-cycle the device,")
            .expect("it offers a power cycle");
        let reset = step.find("factory-reset").expect("it keeps the reset");
        assert!(auto < cycle && cycle < reset, "{step}");
        assert!(!step.contains("passphrase set"), "{step}");
    }

    /// A frame that could not be read is the device talking, not the link failing: the step
    /// is to look at what it sent.
    #[test]
    fn an_unreadable_frame_is_sent_to_inspect() {
        let err = BleError::Profile(svolder_profiles::ProfileError::NoEq);
        let failure = Failure::from_ble(&err);
        assert_eq!(failure.code, 1);
        assert_eq!(failure.next, INSPECT);
    }

    /// A daemon whose configuration names no device says which daemon it is — the one on the
    /// socket the front end asked — and that its configuration is empty, and the step is to
    /// add a device, to the file that daemon reads; the code is still the daemon's own.
    #[test]
    fn a_daemon_with_no_device_is_named_by_its_socket_and_sends_the_user_to_add() {
        let failure = Failure::from_daemon_on(
            std::path::Path::new("/tmp/sv.sock"),
            2,
            svolder_ipc::NO_DEVICES_CONFIGURED,
        );
        assert_eq!(failure.code, 2);
        assert_eq!(
            failure.to_string(),
            "the daemon on /tmp/sv.sock serves no device: its configuration names none. \
             Add one with `svolder --socket /tmp/sv.sock add <udid> --name NAME`, which also \
             tells that daemon to reload; a daemon started with `--config` or an \
             `XDG_CONFIG_HOME` of its own reads that file, and `add` writes the one under its \
             own `XDG_CONFIG_HOME`, so give it the daemon's"
        );

        // A socket a shell would split is quoted, so the command still runs as printed.
        let spaced = Failure::from_daemon_on(
            std::path::Path::new("/tmp/my daemon/sv.sock"),
            2,
            svolder_ipc::NO_DEVICES_CONFIGURED,
        );
        assert!(
            spaced
                .next
                .contains("`svolder --socket '/tmp/my daemon/sv.sock' add <udid> --name NAME`"),
            "{spaced}"
        );

        // Any other refusal from the same daemon keeps its own words and the generic step.
        let other =
            Failure::from_daemon_on(std::path::Path::new("/tmp/sv.sock"), 5, "no such device");
        assert_eq!(other.code, 5);
        assert_eq!(
            other.to_string(),
            "no such device. See `svolder daemon logs`"
        );
    }

    /// A request the daemon refused as a usage error, such as a value it cannot read, sends the
    /// user to the usage, as the same refusal does on a direct session, not to the daemon's log.
    #[test]
    fn a_usage_refusal_from_the_daemon_takes_the_usage_step() {
        let failure = Failure::from_daemon(1, "cannot interpret \"-30\" for control volume");
        assert_eq!(failure.code, 1);
        assert_eq!(failure.next, next_step(Kind::Usage));
        assert!(!failure.to_string().contains("daemon logs"), "{failure}");
    }

    /// An error the daemon answered with keeps the daemon's own code and words.
    #[test]
    fn a_daemon_error_keeps_the_code_it_came_with() {
        let failure = Failure::from_daemon(5, "no such device");
        assert_eq!(failure.code, 5);
        assert_eq!(
            failure.to_string(),
            "no such device. See `svolder daemon logs`"
        );
    }

    /// The daemon's words for a device it has not found yet send the user to the device, not to
    /// the daemon's log alone; code 3 from anything else keeps the generic step.
    #[test]
    fn a_device_not_found_yet_is_told_to_check_the_device() {
        let failure = Failure::from_daemon(3, "amp has not been found yet");
        assert_eq!(failure.code, 3);
        assert_eq!(
            failure.to_string(),
            "amp has not been found yet. svolderd keeps looking: check that it is on and in \
             range (`svolder daemon logs`)"
        );
        let lost = Failure::from_daemon(3, "device not connected");
        assert_eq!(lost.next, next_step(Kind::DaemonError));
        let other = Failure::from_daemon(5, "amp has not been found yet");
        assert_eq!(other.next, next_step(Kind::DaemonError));
    }

    /// Only the daemon's answer for a device it has never found reads as one: its code and its
    /// words, together.
    #[test]
    fn only_code_3_with_the_not_found_yet_words_is_a_device_not_found_yet() {
        assert!(Failure::from_daemon(3, svolder_ipc::not_found_yet("amp")).is_not_found_yet());
        assert!(Failure::from_daemon(3, "amp has not been found yet").is_not_found_yet());
        assert!(!Failure::from_daemon(3, "device not connected").is_not_found_yet());
        assert!(!Failure::from_daemon(5, "amp has not been found yet").is_not_found_yet());
        assert!(!Failure::of(Kind::Dropped, "the link dropped").is_not_found_yet());
    }
}
