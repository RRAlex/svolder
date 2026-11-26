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
use svolder_ipc::socket::daemon_version::reason_of;
use svolder_ipc::{
    CODE_LINK, CODE_NOT_FOUND, CODE_TIMEOUT, CODE_UNKNOWN_REQUEST, CODE_USAGE, Reason,
};

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
    /// The daemon is up and older than this svolder: it does not know what is asked of it.
    DaemonTooOld,
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
    /// The daemon that was answering went away under the command: the device is not at fault.
    DaemonStopped,
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
    pub const ALL: [Kind; 15] = [
        Kind::NoAdapter,
        Kind::AdapterMissing,
        Kind::NothingInRange,
        Kind::AmbiguousDevice,
        Kind::Refused,
        Kind::Dropped,
        Kind::DaemonError,
        Kind::DaemonTooOld,
        Kind::DaemonHasNoDevice,
        Kind::AutospawnFailed,
        Kind::ConfigUnreadable,
        Kind::UnknownProduct,
        Kind::NoDaemon,
        Kind::DaemonStopped,
        Kind::Usage,
    ];

    /// The exit code a failure of this kind earns.
    ///
    /// The same three numbers [`code_for`] gives the [`BleError`] each kind stands for: 1 for
    /// something the user can retype, 2 for a device that was not found, 3 for a link or a
    /// daemon that is not there; 7 is a daemon that is there and too old for the request. A
    /// daemon that answered with a code of its own keeps that
    /// one instead, through [`Failure::from_daemon`].
    pub fn code(self) -> u8 {
        match self {
            Kind::ConfigUnreadable | Kind::UnknownProduct | Kind::Usage => CODE_USAGE,
            Kind::DaemonTooOld => CODE_UNKNOWN_REQUEST,
            Kind::NothingInRange | Kind::AmbiguousDevice | Kind::DaemonHasNoDevice => {
                CODE_NOT_FOUND
            }
            Kind::NoAdapter
            | Kind::AdapterMissing
            | Kind::Refused
            | Kind::Dropped
            | Kind::DaemonError
            | Kind::AutospawnFailed
            | Kind::NoDaemon
            | Kind::DaemonStopped => CODE_LINK,
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
        Kind::DaemonTooOld => "Run `svolder daemon restart`",
        Kind::DaemonHasNoDevice => NO_DEVICE_STEP,
        // The failure's own words name the log file; this names what prints it.
        Kind::AutospawnFailed => "See the log `svolder daemon logs` prints",
        Kind::ConfigUnreadable => "Run `svolder config path`",
        Kind::UnknownProduct => {
            "Run `svolder profiles list`; user profiles live in ~/.config/svolder/profiles"
        }
        Kind::NoDaemon => "Run `svolder daemon install`, or drop `--transport daemon`",
        Kind::DaemonStopped => DAEMON_STOPPED_STEP,
        Kind::Usage => "See `svolder --help`",
    }
}

/// What to do once the daemon has stopped: start it again.
const DAEMON_STOPPED_STEP: &str = "Start it again with `svolder daemon restart`";

/// What is said when the daemon went away, in the words of [`svolder_ipc::IpcError::DaemonGone`].
pub const DAEMON_STOPPED: &str = "The svolder daemon stopped.";

/// The step after a write that was sent and not confirmed: the device may have applied it.
const UNCONFIRMED_STEP: &str =
    "Read it back with `svolder get KEY` (or `svolder status`); the device may have applied it";

/// The step after the daemon's "not found yet": it is still looking, so the device is what to
/// check.
const NOT_FOUND_STEP: &str =
    "svolderd keeps looking: check that it is on and in range (`svolder daemon logs`)";

/// The step an error the daemon answered with calls for: a usage step for a request it refused
/// with code 1 (a value it cannot read, a read-only control), the one for a device it has not
/// found yet, else the one every daemon error gets.
pub fn daemon_step(code: u8, message: &str, reason: Option<Reason>) -> &'static str {
    match (code, reason_of(code, message, reason)) {
        (CODE_USAGE, _) => next_step(Kind::Usage),
        (_, Some(Reason::NotFoundYet)) => NOT_FOUND_STEP,
        (CODE_TIMEOUT, _) if message.starts_with("sent, not confirmed") => UNCONFIRMED_STEP,
        (CODE_TIMEOUT, _) if message.starts_with("not sent") => next_step(Kind::Dropped),
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
    /// Why the daemon says it happened, for a failure that is its answer: the reason it sent, or
    /// the one its words give a 0.32.x daemon ([`reason_of`]).
    pub reason: Option<Reason>,
    /// For a connect that never came up on one adapter while another hears the device: what each
    /// hears and the adapter to switch to. A front end offers the switch; the words of `what`
    /// carry it for one that does not.
    pub adapter_hint: Option<Box<svolder_ipc::AdapterHint>>,
}

impl std::fmt::Display for Failure {
    /// What happened and then what to do, as one line: `what. next`. A `what` that already ends
    /// a sentence (`.`, `?` or `!`) gains no full stop.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.what.ends_with(['.', '?', '!']) {
            _ if self.next.is_empty() => f.write_str(&self.what),
            true => write!(f, "{} {}", self.what, self.next),
            false => write!(f, "{}. {}", self.what, self.next),
        }
    }
}

impl From<svolder_host::eq::EqFailure> for Failure {
    /// The computer EQ's refusal, as a failure with code 1.
    fn from(failure: svolder_host::eq::EqFailure) -> Failure {
        Failure {
            code: CODE_USAGE,
            what: failure.what,
            next: failure.next,
            reason: None,
            adapter_hint: None,
        }
    }
}

impl Failure {
    /// Whether this is the daemon's answer for a device it has never found.
    pub fn is_not_found_yet(&self) -> bool {
        self.code == CODE_LINK && self.reason == Some(Reason::NotFoundYet)
    }

    /// A failure of `kind`, in the words `detail` gives it, with the code and the step that
    /// kind earns.
    pub fn of(kind: Kind, detail: impl Into<String>) -> Failure {
        Failure {
            code: kind.code(),
            what: detail.into(),
            next: next_step(kind).to_string(),
            reason: None,
            adapter_hint: None,
        }
    }

    /// A failure of the conversation with the daemon: the daemon going away says so and names
    /// the step of starting it again, every other error is a daemon error in its own words.
    pub fn from_ipc(err: &svolder_ipc::IpcError) -> Failure {
        match err.daemon_gone() {
            true => Failure::of(Kind::DaemonStopped, DAEMON_STOPPED),
            false => Failure::of(Kind::DaemonError, err.to_string()),
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
            // The daemon's going away stands alone, without the link's "connection failed".
            what: match err {
                _ if daemon_stopped(err) => DAEMON_STOPPED.to_string(),
                // The adapter's busy words alone; the daemon's own log keeps the prefix.
                BleError::Adapter(words) if words == svolder_ble::error::ADAPTER_BUSY => {
                    words.clone()
                }
                _ => err.to_string(),
            },
            next: ble_next_step(err).to_string(),
            reason: None,
            adapter_hint: None,
        }
    }

    /// A failure the daemon answered with: its own code and its own words, and the step a
    /// daemon error calls for ([`daemon_step`]).
    fn from_daemon(code: u8, message: impl Into<String>, reason: Option<Reason>) -> Failure {
        let what = message.into();
        // The words for a missing configuration file give the step themselves.
        let next = match svolder_ipc::socket::daemon_version::is_config_missing_refusal(&what) {
            true => String::new(),
            false => daemon_step(code, &what, reason).to_string(),
        };
        let reason = reason_of(code, &what, reason);
        Failure {
            code,
            what,
            next,
            reason,
            adapter_hint: None,
        }
    }

    /// A failure the daemon on `socket` answered with.
    ///
    /// A daemon whose configuration names no device answers every device request with
    /// [`svolder_ipc::NO_DEVICES_CONFIGURED`], which says nothing of which daemon it is or what
    /// to do; this names the daemon by its socket instead, with the step of adding a device to
    /// it (the `add` in that step carries the same socket, so what it writes is what that daemon
    /// is told to reload). Every other answer is [`Failure::from_daemon`]'s. The code is the
    /// daemon's own either way.
    pub fn from_daemon_on(
        socket: &Path,
        code: u8,
        message: impl Into<String>,
        reason: Option<Reason>,
    ) -> Failure {
        let message = message.into();
        if message != svolder_ipc::NO_DEVICES_CONFIGURED {
            return Failure::from_daemon(code, message, reason);
        }
        Failure {
            code,
            reason: None,
            what: format!(
                "the daemon on {} serves no device: its configuration names none",
                socket.display()
            ),
            next: NO_DEVICE_STEP.replace("SOCKET", &shell_word(socket)),
            adapter_hint: None,
        }
    }

    /// Say this on stderr, as a CLI verb does, and hand back the exit code to leave with.
    pub fn report(self) -> ExitCode {
        eprintln!("svolder: {self}");
        for line in self.hint_lines() {
            eprintln!("{line}");
        }
        ExitCode::from(self.code)
    }

    /// The lines a command prints under the failure when it carries an adapter hint its own words
    /// do not already say: the first behind `svolder: `, the second under it.
    pub fn hint_lines(&self) -> Vec<String> {
        let Some(hint) = &self.adapter_hint else {
            return Vec::new();
        };
        if self.what.contains(&hint.step()) {
            return Vec::new();
        }
        let [hears, step] = hint.lines();
        vec![format!("svolder: {hears}"), format!("         {step}")]
    }
}

/// The step after an adapter that another scan or connection holds: the svolder daemon, when one
/// runs, is the usual holder.
const ADAPTER_BUSY_STEP: &str = "The svolder daemon, if it runs, holds it: `svolder status` shows \
     it; stop it with `svolder daemon stop` or wait, then try again";

/// Whether `err` is the daemon's going away, which `daemon_link_error` (backend/remote.rs)
/// carries as a [`BleError::Connection`] holding exactly [`DAEMON_STOPPED`]; nothing may wrap
/// those words.
fn daemon_stopped(err: &BleError) -> bool {
    matches!(err, BleError::Connection(words) if words == DAEMON_STOPPED)
}

/// The step a [`BleError`] calls for, by what the error says went wrong.
///
/// A frame that could not be encoded or decoded is the one departure from the table: the link
/// is up and the device is talking, so the step is to look at what it is saying rather than at
/// the link.
fn ble_next_step(err: &BleError) -> &'static str {
    match err {
        BleError::NotFound => next_step(Kind::NothingInRange),
        BleError::Adapter(words) if words == svolder_ble::error::ADAPTER_BUSY => ADAPTER_BUSY_STEP,
        BleError::Adapter(_) => next_step(Kind::AdapterMissing),
        BleError::PassphraseRejected => next_step(Kind::Refused),
        _ if daemon_stopped(err) => next_step(Kind::DaemonStopped),
        BleError::Timeout(_) | BleError::Unconfirmed { .. } => UNCONFIRMED_STEP,
        BleError::Connection(_) | BleError::NotSent(_) | BleError::Bootloader => {
            next_step(Kind::Dropped)
        }
        BleError::Proto(_) | BleError::Profile(_) => INSPECT,
    }
}

/// The exit code a [`BleError`] earns, as the number a [`Failure`] carries it as.
pub fn code_for(err: &BleError) -> u8 {
    match err {
        BleError::NotFound => CODE_NOT_FOUND,
        BleError::Connection(_)
        | BleError::Bootloader
        | BleError::PassphraseRejected
        | BleError::Adapter(_) => CODE_LINK,
        BleError::Timeout(_) | BleError::Unconfirmed { .. } | BleError::NotSent(_) => CODE_TIMEOUT,
        BleError::Proto(_) | BleError::Profile(_) => CODE_USAGE,
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

    /// A daemon that went away reads as the words and the start command, with the link code,
    /// never as a device fault; any other IPC error stays a daemon error.
    #[test]
    fn a_daemon_that_went_away_is_said_so_and_never_blames_the_device() {
        let gone = Failure::from_ipc(&svolder_ipc::IpcError::from_wire(std::io::Error::from(
            std::io::ErrorKind::BrokenPipe,
        )));
        assert_eq!(gone.code, 3);
        assert_eq!(
            gone.to_string(),
            "The svolder daemon stopped. Start it again with `svolder daemon restart`"
        );
        assert_eq!(gone.what, svolder_ipc::IpcError::DaemonGone.to_string());
        let other = Failure::from_ipc(&svolder_ipc::IpcError::Protocol("odd".into()));
        assert!(other.what.contains("odd") && other.next.contains("daemon logs"));
    }

    /// A busy adapter names the daemon and `svolder status` as the step, and never sends the
    /// user to `svolder doctor`, which is what hit the busy adapter.
    #[test]
    fn a_busy_adapter_is_not_sent_to_doctor() {
        let busy = Failure::from_ble(&BleError::from_bluez_text("Operation already in progress"));
        assert_eq!(busy.what, svolder_ble::error::ADAPTER_BUSY);
        assert!(busy.next.contains("svolder status"), "{}", busy.next);
        assert!(!busy.next.contains("doctor"), "{}", busy.next);
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
        assert_eq!(timeout.next, UNCONFIRMED_STEP);
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
            None,
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
            None,
        );
        assert!(
            spaced
                .next
                .contains("`svolder --socket '/tmp/my daemon/sv.sock' add <udid> --name NAME`"),
            "{spaced}"
        );

        // Any other refusal from the same daemon keeps its own words and the generic step.
        let other = Failure::from_daemon_on(
            std::path::Path::new("/tmp/sv.sock"),
            5,
            "no such device",
            None,
        );
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
        let failure = Failure::from_daemon(1, "cannot interpret \"-30\" for control volume", None);
        assert_eq!(failure.code, 1);
        assert_eq!(failure.next, next_step(Kind::Usage));
        assert!(!failure.to_string().contains("daemon logs"), "{failure}");
    }

    /// An error the daemon answered with keeps the daemon's own code and words.
    #[test]
    fn a_daemon_error_keeps_the_code_it_came_with() {
        let failure = Failure::from_daemon(5, "no such device", None);
        assert_eq!(failure.code, 5);
        assert_eq!(
            failure.to_string(),
            "no such device. See `svolder daemon logs`"
        );
    }

    /// The daemon's refusal for a missing configuration file gives its own step: none is added.
    #[test]
    fn a_missing_config_refusal_gets_no_step_of_its_own() {
        let words = svolder_ipc::socket::daemon_version::config_missing_refusal("/x/config.toml");
        let failure = Failure::from_daemon(1, words.clone(), Some(Reason::ReadOnly));
        assert_eq!(failure.code, 1);
        assert_eq!(failure.to_string(), words);
        assert!(!failure.to_string().contains("--help"), "{failure}");
    }

    /// The daemon's words for a device it has not found yet send the user to the device, not to
    /// the daemon's log alone; code 3 from anything else keeps the generic step.
    #[test]
    fn a_device_not_found_yet_is_told_to_check_the_device() {
        let failure = Failure::from_daemon(3, "amp has not been found yet", None);
        assert_eq!(failure.code, 3);
        assert_eq!(
            failure.to_string(),
            "amp has not been found yet. svolderd keeps looking: check that it is on and in \
             range (`svolder daemon logs`)"
        );
        let lost = Failure::from_daemon(3, "device not connected", None);
        assert_eq!(lost.next, next_step(Kind::DaemonError));
        let other = Failure::from_daemon(5, "amp has not been found yet", None);
        assert_eq!(other.next, next_step(Kind::DaemonError));
    }

    /// Only the daemon's answer for a device it has never found reads as one: its code and its
    /// words, together.
    #[test]
    fn only_code_3_with_the_not_found_yet_words_is_a_device_not_found_yet() {
        assert!(
            Failure::from_daemon(3, svolder_ipc::not_found_yet("amp"), None).is_not_found_yet()
        );
        assert!(Failure::from_daemon(3, "amp has not been found yet", None).is_not_found_yet());
        assert!(!Failure::from_daemon(3, "device not connected", None).is_not_found_yet());
        assert!(!Failure::from_daemon(5, "amp has not been found yet", None).is_not_found_yet());
        assert!(!Failure::of(Kind::Dropped, "the link dropped").is_not_found_yet());
    }

    /// A daemon that sends the reason is read by it, whatever its words say; the words are only
    /// read when it sent none.
    #[test]
    fn the_reason_decides_before_the_words() {
        let by_reason = Failure::from_daemon(3, "some new words", Some(Reason::NotFoundYet));
        assert!(by_reason.is_not_found_yet());
        assert_eq!(by_reason.next, NOT_FOUND_STEP);
        let lost =
            Failure::from_daemon(3, "amp has not been found yet", Some(Reason::NotConnected));
        assert!(!lost.is_not_found_yet());
        let unknown = Failure::from_daemon(3, "x has not been found yet", Some(Reason::Unknown));
        assert!(!unknown.is_not_found_yet());
    }
}
