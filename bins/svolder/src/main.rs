// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder`: Schiit Forkbeard control from the terminal.
//!
//! - [`backend`]: where a device verb runs: a daemon that holds it, or a session of its own.
//! - [`cli`]: the command line's definition: every flag, verb and sub-verb clap parses.
//! - [`clock`]: a moment as UTC text.
//! - [`commands`]: what each verb does, one module per verb or family of verbs.
//! - [`env_vars`]: the `SVOLDER_*` environment variables in one table, and `svolder help env`.
//! - [`mod@format`]: rendering one reading as a status line, a JSON object or a bare value.
//! - [`front_door`]: what `svolder` with no verb does.
//! - [`qr`]: a QR code as text, in half blocks.
//! - [`recovery`]: what went wrong, and the one step that follows from it.
//! - [`scanning`]: finding a device on the air, and how well each adapter hears it.
//! - [`state`]: what svolder remembers about the devices it has connected to, between runs.
//! - [`summary`]: what bare `svolder` prints when its output is being read rather than watched.
//! - [`tui`]: `svolder tui`, the interactive terminal UI.
//! - [`which`]: finding a program to run, by path or on `PATH`.
//! - [`wizard`]: the guided first run, one state machine and the front ends that drive it.

mod backend;
mod cli;
mod clock;
mod commands;
mod env_vars;
mod format;
mod front_door;
mod qr;
mod recovery;
mod scanning;
mod state;
mod summary;
#[cfg(test)]
mod test_support;
mod tui;
mod which;
mod wizard;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use backend::Globals;
use cli::{Cli, Command};
use commands::{
    capture, config, daemon, decode, demo, device, doctor, hops, host_eq, identify, profiles,
    token, web,
};
use recovery::Failure;

/// `$XDG_CONFIG_HOME/svolder` or `~/.config/svolder`, the home being the account's own when
/// `$HOME` is unset.
pub fn config_dir() -> PathBuf {
    svolder_ipc::config::config_home().join("svolder")
}

/// Built-in profiles plus the user's override directory.
pub fn registry() -> Result<svolder_profiles::Registry, String> {
    svolder_profiles::Registry::with_user_dir(&config_dir().join("profiles"))
        .map_err(|e| e.to_string())
}

/// Whether the colours are off: `--no-color` on the command line, or `NO_COLOR` in the
/// environment.
fn monochrome(no_color: bool) -> bool {
    no_color || std::env::var_os("NO_COLOR").is_some()
}

/// What a verb that either worked or did not leaves with: nothing said on the way out, or the
/// failure's own words and the code it earns.
fn done(outcome: Result<(), Failure>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}

/// Parse the command line, make the demo's daemon and environment when `tui --demo` asks for
/// them, and only then start the runtime.
///
/// The demo points this process's environment at a private directory, which is sound only while
/// no other thread exists, so it is made here, before the runtime's workers are. The demo is held
/// outside the runtime, so a panic in it unwinds through the guard as well.
fn main() -> ExitCode {
    // `help env` and `help env --markdown` are intercepted before clap ever sees the command
    // line: clap's built-in `help` subcommand rejects a topic it has no subcommand for, which
    // `env` is not, so parsing it as usual would only error. Everything else (bare `help`,
    // `help <command>`, `<command> --help`) reaches `Cli::try_parse` exactly as before.
    match env_vars::requested(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Some(env_vars::Format::Text) => {
            print!("{}", env_vars::render_text(env_vars::terminal_width()));
            return ExitCode::SUCCESS;
        }
        Some(env_vars::Format::Markdown) => {
            print!("{}", env_vars::render_markdown());
            return ExitCode::SUCCESS;
        }
        None => {}
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    let demo = match cli.command {
        Some(Command::Tui { demo: true, .. }) => match demo::Demo::start() {
            Ok(demo) => Some(demo),
            Err(failure) => return failure.report(),
        },
        _ => None,
    };
    let socket = demo.as_ref().map(demo::Demo::socket);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("svolder: the async runtime: {e}");
            return ExitCode::from(1);
        }
    };
    let code = match runtime.block_on(run(cli, socket)) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("svolder: {message}");
            ExitCode::from(1)
        }
    };
    // The runtime's tasks are over before the daemon they talk to is stopped.
    drop(runtime);
    drop(demo);
    code
}

async fn run(cli: Cli, demo_socket: Option<PathBuf>) -> Result<ExitCode, String> {
    let g = Globals {
        transport: &cli.transport,
        adapter: cli.adapter.as_deref(),
        socket: cli.socket.as_deref(),
        device: cli.device.as_deref(),
        verbose: cli.verbose,
    };
    let Some(command) = cli.command else {
        return front_door::front_door(&g).await;
    };
    match command {
        Command::Setup => Ok(wizard::setup(&g, None).await),
        Command::Doctor { json, yes, dev } => {
            Ok(doctor::run(&g, doctor::DoctorOpts { json, yes, dev }).await)
        }
        Command::Scan { timeout, json } => device::scan(&g, timeout, json).await,
        Command::Tui {
            no_color,
            demo: true,
            ..
        } => match demo_socket {
            Some(socket) => Ok(demo::run(&g, monochrome(no_color), socket).await),
            None => Err("the demo was not started".to_string()),
        },
        Command::Tui {
            no_color,
            join: Some(socket),
            ..
        } => Ok(demo::join(&g, monochrome(no_color), socket).await),
        Command::Tui {
            no_color,
            demo: false,
            join: None,
        } => Ok(tui::run(
            &g,
            tui::TuiOptions {
                no_color: monochrome(no_color),
                demo: None,
                join: None,
                reloaded: None,
                checked: false,
            },
        )
        .await),
        Command::Status { json } => device::status(&g, json).await,
        Command::Get { key, json } => device::get(&g, &key, json).await,
        Command::Set {
            key,
            value,
            step,
            no_wait,
            yes,
            json,
        } => device::set(&g, &key, &value, step, (no_wait, yes), json).await,
        Command::Watch { json } => device::watch(&g, json).await,
        Command::Eq { command } => device::eq(&g, command).await,
        Command::Preset { command } => device::preset(&g, command).await,
        Command::Host { command } => Ok(host_eq::run(&g, command).await),
        Command::Devices { json } => device::devices(&g, json).await,
        Command::Add {
            udid,
            name,
            profile,
            passphrase,
        } => Ok(device::add(
            &g,
            udid.as_deref(),
            name.as_deref(),
            profile.as_deref(),
            &passphrase,
        )
        .await),
        Command::Remove { name } => Ok(device::remove(&g, &name).await),
        Command::Hops { json, command } => Ok(hops::run(&g, json, command).await),
        Command::Passphrase { command } => device::passphrase(&g, command).await,
        Command::Identify => Ok(identify::run(&g).await),
        Command::Daemon(command) => Ok(daemon::run(&g, command).await),
        Command::Inspect { json } => device::inspect(&g, json).await,
        Command::Capture {
            seconds,
            keep_ids,
            yes,
            out,
        } => Ok(capture::run(
            &g,
            capture::Options {
                seconds,
                keep_ids,
                out,
            },
            yes,
        )
        .await),
        Command::Raw {
            id,
            value_type,
            value,
            no_wait,
            json,
        } => device::raw(&g, &id, &value_type, &value, no_wait, json).await,
        Command::Decode { hex, profile, json } => {
            decode::run(&hex, profile.as_deref(), json).map(|()| ExitCode::SUCCESS)
        }
        Command::Profiles { command } => profiles::run(command),
        Command::Mcp { command } => token::mcp(&g, command).await,
        Command::Web { qr, command } => web::run(&g, qr, command).await,
        Command::Config { command } => config::run(command),
    }
}

#[cfg(test)]
mod tests;

/// Unit tests run with scratch homes, as `make test` gives them, so none reads or writes the
/// user's own config, state, data, cache or runtime directory, however the tests were started.
#[cfg(test)]
#[used]
#[unsafe(link_section = ".init_array")]
static SCRATCH_HOMES: extern "C" fn() = {
    extern "C" fn pin() {
        // SAFETY: the ELF initialisers run before `main`, while the process has one thread.
        unsafe { svolder_ipc::config::pin_scratch_homes() }
    }
    pin
};
