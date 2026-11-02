// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `capture`: one text file of everything svolder learns about a device, for its owner to send to
//! the developers.
//!
//! The pieces the verb rests on are in the children:
//!
//! - [`scrub`]: unit ids, passphrase bytes, the home directory and Bluetooth addresses taken out
//!   of everything the file holds, unless the owner asked to keep them.
//! - [`handover`]: the daemon that holds the device stopped for the capture and always brought
//!   back.
//! - [`mod@file`]: the text of the file, built from what the run gathered.

mod file;
pub(crate) mod handover;
pub(crate) mod scrub;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures::StreamExt;
use svolder_ble::{Advertisement, FrameTap, TappedTransport, Transport};
use svolder_ipc::Config;
use svolder_ipc::checks::Outcome;
use svolder_proto::UnitId;
use tokio::time::Instant;

use crate::backend::launch::{Host, Live};
use crate::backend::{Backend, Globals, Wire, known_transport, transport_for};
use crate::commands::daemon::Ctl;
use crate::commands::device::confirm;
use crate::commands::doctor;
use crate::commands::interrupts::{self, Presses};
use crate::recovery::{Failure, Kind};
use crate::{clock, scanning};

use file::{Capture, render};
use handover::Holder;
use scrub::Scrub;

/// What the command line asked of a capture.
pub(crate) struct Options {
    /// How long the window of every frame lasts, in seconds; 0 skips it.
    pub seconds: u32,
    /// Whether unit ids and passphrase bytes stay in the file.
    pub keep_ids: bool,
    /// The file to write, instead of one named from the product and the time.
    pub out: Option<PathBuf>,
}

/// What a capture came to: the exit code, and the file it wrote.
pub(crate) struct Report {
    /// The exit code the command leaves with.
    pub code: u8,
    /// The file that was written, when one was.
    pub file: Option<PathBuf>,
}

/// A question put to the owner, answered yes or no.
pub(crate) type Ask<'a> = Box<dyn FnOnce(&str) -> Result<bool, Failure> + 'a>;

/// What a capture asks of the world around it, so a test can put its own there.
pub(crate) struct Hands<'a> {
    /// The `systemctl` the daemon is stopped and started through.
    pub ctl: &'a Ctl,
    /// The host a spawned daemon is started on.
    pub host: &'a dyn Host,
    /// Asks the owner a question and says what they answered.
    pub ask: Ask<'a>,
    /// Starts counting Ctrl-C.
    pub listen: Box<dyn FnOnce() -> Arc<Presses> + 'a>,
    /// A transport to talk through instead of opening the one the command line names.
    pub transport: Option<Arc<dyn Transport>>,
}

/// How a capture ended.
enum End {
    /// It ran to its end.
    Done,
    /// Ctrl-C ended it before the frame window.
    Pressed,
    /// Something failed part way.
    Failed(Failure),
}

/// What the body gathers, and the product the file is named for.
struct Draft {
    /// The data of the file.
    data: Capture,
    /// The product of the device that was reached, as a file name part.
    product: Option<String>,
}

impl Draft {
    /// A draft with nothing gathered yet, for a capture taken at `taken` with a window of
    /// `window` seconds.
    fn new(taken: &str, window: u32) -> Draft {
        Draft {
            data: Capture {
                version: format!("svolder {}", env!("CARGO_PKG_VERSION")),
                taken: taken.to_string(),
                window,
                ended_early: None,
                window_from: None,
                doctor: None,
                adverts: None,
                readings: None,
                profile: None,
                frames: None,
                ended: None,
            },
            product: None,
        }
    }
}

/// The wire a capture opens, or why it cannot: it talks to the device directly, so a daemon is
/// not an option.
fn wire_for(transport: &str) -> Result<Wire, Failure> {
    known_transport(transport)?;
    match transport {
        "daemon" => Err(Failure::of(
            Kind::Usage,
            "capture talks to the device directly, so it cannot run with --transport daemon",
        )),
        "fake" => Ok(Wire::Fake),
        _ => Ok(Wire::Ble),
    }
}

/// `-o FILE` checked before anything runs: not there yet, in a directory that is.
fn check_out(out: &Path) -> Result<(), Failure> {
    if out.exists() {
        return Err(Failure::of(
            Kind::Usage,
            format!(
                "{} exists already; pick another file with -o",
                out.display()
            ),
        ));
    }
    match out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.is_dir() => Err(Failure::of(
            Kind::Usage,
            format!(
                "{} is not a directory; capture does not make it",
                parent.display()
            ),
        )),
        _ => Ok(()),
    }
}

/// Run `work` until it ends or Ctrl-C is pressed, which is `None`.
async fn stage<T>(presses: &Presses, work: impl Future<Output = T>) -> Option<T> {
    tokio::select! {
        biased;
        () = presses.next() => None,
        done = work => Some(done),
    }
}

/// Every advertisement a scan window hears, in the order heard.
async fn scan_all(transport: &dyn Transport) -> Result<Vec<Advertisement>, Failure> {
    let heard = transport
        .scan(scanning::WINDOW)
        .await
        .map_err(|e| Failure::from_ble(&e))?;
    Ok(heard.collect().await)
}

/// The product as a part of a file name: lower case, words joined by `-`.
fn product_slug(display: &str) -> String {
    let words: Vec<String> = display
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect();
    match words.is_empty() {
        true => "unknown".to_string(),
        false => words.join("-"),
    }
}

/// What happens with the device free of the daemon: the scan, the connect, the readings and the
/// window of every frame, each stage ending at a press.
async fn gather(
    g: &Globals<'_>,
    config: &Config,
    wire: Wire,
    seconds: u32,
    presses: &Presses,
    given: Option<Arc<dyn Transport>>,
    draft: &mut Draft,
) -> End {
    let (inner, adapter) = match given {
        Some(transport) => (transport, None),
        None => match stage(presses, transport_for(g, config, wire)).await {
            None => return End::Pressed,
            Some(Err(failure)) => return End::Failed(failure),
            Some(Ok(opened)) => opened,
        },
    };
    match stage(presses, scan_all(inner.as_ref())).await {
        None => return End::Pressed,
        Some(Err(failure)) => return End::Failed(failure),
        Some(Ok(adverts)) => draft.data.adverts = Some(adverts),
    }
    let tap = FrameTap::new();
    let tapped: Arc<dyn Transport> = Arc::new(TappedTransport::new(inner, tap.clone()));
    let opened = stage(
        presses,
        Backend::direct_over(g, config, tapped, wire, adapter.as_deref()),
    )
    .await;
    let end = match opened {
        None => End::Pressed,
        Some(Err(failure)) => End::Failed(failure),
        Some(Ok(mut backend)) => {
            draft.data.profile = Some(backend.profile().clone());
            draft.product = Some(product_slug(&backend.profile().display));
            let end = read_and_listen(&mut backend, seconds, presses, &tap, draft).await;
            // The link is let go whatever ended the run.
            backend.close().await;
            end
        }
    };
    let frames = tap.frames();
    draft.data.frames = (!frames.is_empty()).then_some(frames);
    end
}

/// The readings, then the window in which the owner presses the device's buttons.
async fn read_and_listen(
    backend: &mut Backend,
    seconds: u32,
    presses: &Presses,
    tap: &FrameTap,
    draft: &mut Draft,
) -> End {
    match stage(presses, backend.state()).await {
        None => return End::Pressed,
        Some(Ok(values)) => draft.data.readings = Some(values),
        Some(Err(failure)) => return End::Failed(failure),
    }
    if seconds == 0 {
        return End::Done;
    }
    eprintln!(
        "svolder: recording every frame for {seconds} s; press the device's buttons now (Ctrl-C ends early)"
    );
    draft.data.window_from = Some(tap.elapsed());
    let started = Instant::now();
    let deadline = started + Duration::from_secs(u64::from(seconds));
    loop {
        tokio::select! {
            biased;
            () = presses.next() => {
                draft.data.ended_early = Some(started.elapsed());
                return End::Done;
            }
            () = tokio::time::sleep_until(deadline) => return End::Done,
            // Notifications only reach the tap while something polls for them.
            heard = backend.next_status() => match heard {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return End::Failed(Failure::of(
                        Kind::Dropped,
                        "the link ended during the frame window",
                    ));
                }
                Err(e) => return End::Failed(Failure::from_ble(&e)),
            },
        }
    }
}

/// The file name a capture gets when none was asked for: the product and the time, in the
/// current directory.
fn default_name(product: Option<&str>, taken: &str) -> PathBuf {
    let stamp = taken.replace(' ', "-").replace(':', "");
    PathBuf::from(format!(
        "svolder-capture-{}-{stamp}.txt",
        product.unwrap_or("unknown")
    ))
}

/// Write `text` to a file that does not exist yet, readable by its owner alone: with `--keep-ids`
/// it holds what others on the machine have no business with.
fn write_new(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()
}

/// A failure said on stderr, as the report it ends the run with.
fn failed(failure: Failure) -> Report {
    eprintln!("svolder: {failure}");
    Report {
        code: failure.code,
        file: None,
    }
}

/// The whole run, with `doctor` the checks of the host, not started until the run is ready for
/// them; it is told whether it may connect to the daemon's socket, which it may not where
/// systemd's socket unit is listening with its service idle.
///
/// In order: the options checked, the holder of the device found and, when there is one, the
/// owner asked, then Ctrl-C listened for (one pressed at the question ends the process, as
/// nothing is stopped yet); doctor's outcomes collected while the host is as the owner has it;
/// then, with the holder stopped, the scan, the connect, the readings and the frame window; the
/// holder put back; the file written. Ctrl-C ends whatever stage it finds, and a daemon that
/// was stopped is brought back on every path.
pub(crate) async fn capture<Fut>(
    g: &Globals<'_>,
    options: Options,
    mut hands: Hands<'_>,
    doctor: impl FnOnce(bool) -> Fut,
) -> Report
where
    Fut: Future<Output = Vec<Outcome>>,
{
    let wire = match wire_for(g.transport) {
        Ok(wire) => wire,
        Err(failure) => return failed(failure),
    };
    if let Some(out) = &options.out
        && let Err(failure) = check_out(out)
    {
        return failed(failure);
    }
    let config = match Backend::load_config_for(g, &Config::path()) {
        Ok(config) => config,
        Err(failure) => return failed(failure),
    };
    let holder = handover::holder(g, &config, hands.ctl).await;
    if holder != Holder::Nobody {
        match (hands.ask)("stop the daemon for the capture and restart it after? [y/N]") {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("svolder: nothing captured");
                return Report {
                    code: 0,
                    file: None,
                };
            }
            Err(failure) => return failed(failure),
        }
    }
    let presses = (hands.listen)();
    let taken = clock::date_time(SystemTime::now());
    // The devices the owner configured are numbered first, in the order of the file, then a unit
    // id given with `-d`: a failure's message may name it, and no scan need have shown it.
    let mut scrub = Scrub::new(options.keep_ids);
    for device in &config.devices {
        if let Ok(unit) = device.udid.parse::<UnitId>() {
            scrub.unit(unit);
        }
    }
    if let Some(Ok(unit)) = g.device.map(str::parse::<UnitId>) {
        scrub.unit(unit);
    }
    // The readings show the profile's own input names: the ones the owner gave are theirs.
    let mut body_config = config.clone();
    for device in &mut body_config.devices {
        device.inputs.clear();
    }
    let mut draft = Draft::new(&taken, options.seconds);
    // Where systemd's socket unit listens with its service idle, a connect would start the
    // service, which would then be what the doctor reports on.
    let connect = !matches!(holder, Holder::Systemd { service: false, .. });
    eprintln!("svolder: checking the host");
    let mut end = match stage(&presses, doctor(connect)).await {
        None => End::Pressed,
        Some(outcomes) => {
            draft.data.doctor = serde_json::to_string_pretty(&outcomes).ok();
            End::Done
        }
    };
    let mut back = Ok(());
    if matches!(end, End::Done) {
        let stopped = holder != Holder::Nobody;
        let body = async {
            if stopped {
                eprintln!("svolder: stopped the daemon; it comes back after the capture");
            }
            gather(
                g,
                &body_config,
                wire,
                options.seconds,
                &presses,
                hands.transport.take(),
                &mut draft,
            )
            .await
        };
        let (ran, restored) = handover::away(g, hands.ctl, hands.host, &holder, body).await;
        end = ran.unwrap_or_else(End::Failed);
        if stopped && restored.is_ok() {
            eprintln!("svolder: the daemon is running again");
        }
        back = restored;
    }
    draft.data.ended = match &end {
        End::Done => None,
        End::Pressed => Some("interrupted by Ctrl-C".to_string()),
        End::Failed(failure) => Some(failure.to_string()),
    };
    let text = render(&mut scrub, &draft.data);
    let (file, written) = match options.out {
        Some(out) => {
            let result = write_new(&out, &text);
            (out, result)
        }
        None => {
            let name = default_name(draft.product.as_deref(), &taken);
            let result = write_new(&name, &text);
            (name, result)
        }
    };
    let mut code = match &end {
        End::Done => 0,
        End::Pressed => 130,
        End::Failed(failure) => {
            eprintln!("svolder: {failure}");
            failure.code
        }
    };
    let file = match written {
        Ok(()) => Some(file),
        Err(e) => {
            // The sitting is not lost: what could not be saved is printed.
            println!("{text}");
            eprintln!(
                "svolder: the capture could not be written to {}: {e}; it is printed above instead",
                file.display()
            );
            code = 1;
            None
        }
    };
    if let Err(failure) = back {
        eprintln!("svolder: {failure}");
        code = failure.code;
    }
    Report { code, file }
}

/// `svolder capture`, on this machine's own systemd, daemons and terminal.
pub(crate) async fn run(g: &Globals<'_>, options: Options, yes: bool) -> ExitCode {
    let hands = Hands {
        ctl: &Ctl::live().in_own_group(),
        host: &Live,
        ask: Box::new(move |question| confirm(question, yes)),
        listen: Box::new(interrupts::listen),
        transport: None,
    };
    let report = capture(g, options, hands, |connect| {
        doctor::outcomes_with(g, false, connect)
    })
    .await;
    if let Some(file) = &report.file {
        println!("{}", file.display());
    }
    ExitCode::from(report.code)
}

#[cfg(test)]
mod tests;
