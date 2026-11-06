// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `SVOLDER_*` environment variables, in one table, and what `svolder help env` does with
//! it: the terminal listing `main` prints, and the GitHub Markdown table
//! `docs/usage/environment.md` carries between its
//! `<!-- env-table:begin -->`/`<!-- env-table:end -->` markers (kept in sync by `make docs-env`,
//! checked by a test that fails naming it when the two drift).

/// One `SVOLDER_*` variable: its name, who reads it, whether it is for tests and development
/// only, and one sentence of what it does.
struct EnvVar {
    /// The variable's name, e.g. `SVOLDER_TRANSPORT`.
    pub name: &'static str,
    /// Which programs read it, e.g. `"svolder, svolderd"`.
    pub read_by: &'static str,
    /// Whether this is for tests and development rather than everyday use.
    pub dev_only: bool,
    /// What it does: the values it takes, and the default when unset.
    pub what: &'static str,
}

/// Every `SVOLDER_*` variable the workspace reads, in the order `svolder help env` lists them:
/// everyday use first, then the tests-and-development ones. A test in `env_vars.rs`'s own test
/// module scans `bins/`, `crates/`, `scripts/*.sh`, `Makefile` and the Docker files for
/// `SVOLDER_[A-Z0-9_]+` and fails on a name missing here or a row nothing names, so this list
/// cannot drift from the code that reads these variables.
const ENV_VARS: &[EnvVar] = &[
    EnvVar {
        name: "SVOLDER_TRANSPORT",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Where a device verb runs: `auto` (the daemon when it is running, else a direct \
               BlueZ session), `daemon`, `ble`, or `fake`. Default `auto` for `svolder`, `ble` \
               for `svolderd`.",
    },
    EnvVar {
        name: "SVOLDER_ADAPTER",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Bluetooth adapter for a direct BlueZ session, e.g. `hci0`. Defaults to the \
               configured `adapter`, else the first adapter BlueZ reports.",
    },
    EnvVar {
        name: "SVOLDER_SOCKET",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Unix socket path to serve or connect to. Defaults to \
               `$XDG_RUNTIME_DIR/svolder.sock`, then `/run/user/<uid>/svolder.sock`, then \
               `/tmp/svolder-<uid>/svolder.sock`.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_PROFILE",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Profile the in-process fake device (`--transport fake`) uses. Default \
               `gungnir2-v2`.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_UNIT",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Unit id (8 bytes, hex) the fake device reports. Default `0102030405060708`.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_MORE",
        read_by: "svolderd",
        dev_only: true,
        what: "More live fake devices beside the first, for `svolderd --transport fake`: \
               `<profile>@<UNIT>`, comma-separated, each answering its own unit id (16 hex \
               digits) with the code `1234` and firmware `3`. The other `SVOLDER_FAKE_*` \
               knobs describe the first device alone, except `SVOLDER_FAKE_SIGNAL`, which \
               plays on every device. The daemon lists a device only when a `[[device]]` \
               block names its unit. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_FIRMWARE",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Firmware version the fake device reports. Default `3`.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_PASSPHRASE",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Passphrase the fake device requires: four digits, or `none` for no passphrase. \
               Default `1234`.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_FAULTS",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "`1` seeds every `Fault` control the chosen profile has as active. Unset by \
               default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_SIGNAL",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "`1` plays a signal on the meters; needs a profile whose meters have a scale, \
               such as `fake-amp`. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_LOKI_REPORTS",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Which id the fake Loki Max reports a band on: `full` (the default) is its \
               three-id form, `6.<band>.3`, and `short` the two-id form, `6.<band>`, which a \
               front end reads as the same band. Any other value is refused.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_SILENT_RECALL",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "`1` makes the fake Loki Max set its bands on a device-slot recall without \
               reporting them, as a unit that pushes nothing after a recall would. Unset by \
               default: a recall reports the bands it changed.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_QUIET_SAME",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "`1` makes the fake device report a change only: a Command equal to the value a \
               control holds gets no Status, on every control. Unset by default: only the \
               tone-control mode is silent on a repeat, and every other control is reported.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_SEVER_AFTER_MS",
        read_by: "svolder, svolderd",
        dev_only: true,
        what: "Cuts the in-process fake device's first link this many milliseconds after it \
               comes up, the way a device dropping the connection would, so a test can watch \
               a reconnect. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_GONE_AFTER_MS",
        read_by: "svolder, svolderd",
        dev_only: true,
        what: "Takes the in-process fake device off the air this many milliseconds after it \
               starts: its link is cut and it is neither heard nor reached again, so a test can \
               watch a device out of reach. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_GONE_FILE",
        read_by: "svolder, svolderd",
        dev_only: true,
        what: "A path: the in-process fake device goes off the air, as with \
               `SVOLDER_FAKE_GONE_AFTER_MS`, the moment a file appears there, so a test drops \
               it on its own cue. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_COVER_GONE_AFTER_MS",
        read_by: "svolderd",
        dev_only: true,
        what: "With `SVOLDER_HOST=fake`, drops the fake player's cover art this many \
               milliseconds after the host service starts, the track staying, so a check can \
               watch a card lose its picture. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_PLAYER_GONE_AFTER_MS",
        read_by: "svolderd",
        dev_only: true,
        what: "With `SVOLDER_HOST=fake`, makes the fake player quit this many milliseconds \
               after the host service starts, so a check can watch a card go to no player. \
               Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_COVER_GONE_FILE",
        read_by: "svolderd",
        dev_only: true,
        what: "A path: with `SVOLDER_HOST=fake`, the fake player's cover art is dropped, as \
               with `SVOLDER_FAKE_COVER_GONE_AFTER_MS`, the moment a file appears there, so a \
               test drops it on its own cue. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_PLAYER_GONE_FILE",
        read_by: "svolderd",
        dev_only: true,
        what: "A path: with `SVOLDER_HOST=fake`, the fake player quits, as with \
               `SVOLDER_FAKE_PLAYER_GONE_AFTER_MS`, the moment a file appears there, so a test \
               drops it on its own cue. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_ROUTE",
        read_by: "svolderd",
        dev_only: true,
        what: "With `SVOLDER_HOST=fake`, `changed` makes the fake route alter the signal in every \
               way a route can (the player at 91 % and 44.1 kHz into a sink at 48 kHz and 54 %, \
               the host EQ on, two other sounds mixing), so a check can see a changed path \
               line. Unset by default: the route is untouched.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_CAPTURE",
        read_by: "svolderd",
        dev_only: true,
        what: "With `SVOLDER_HOST=fake`, `off` gives the fake host no capture: the visualizer's \
               stream reports it unavailable, so a check can see the card say so. Unset by \
               default: the fake host captures synthetic music while a page or a TUI watches.",
    },
    EnvVar {
        name: "SVOLDER_FAKE_TITLE",
        read_by: "svolderd",
        dev_only: true,
        what: "With `SVOLDER_HOST=fake`, names the fake track in place of `Test Pressing`, so a \
               check (or a look at the phone's folded line) can see a title too long for its \
               room. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_BLE_TRACE",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "Traces a real BlueZ connect's steps to stderr, one line per step with its \
               elapsed time. Set to anything but empty or `0`; unset by default.",
    },
    EnvVar {
        name: "SVOLDER_HOST",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "`off` keeps `svolder tui` away from the host's own audio for that process, \
               whatever `[tui] host_audio` says: no `wpctl`, no `pw-record`, no `pw-play`, no session bus. \
               `svolder host eq apply` and `remove` refuse with it set. For `svolderd`, `off` \
               turns the host service off (no player watcher, no `pw-dump`, no cover reads), \
               whatever `[host] enabled` says, and `fake` runs a fake player, a fake route and a \
               bundled cover in its place, for demos and checks. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_MQTT_PASSWORD",
        read_by: "svolderd",
        dev_only: false,
        what: "The MQTT broker's password, in place of `[mqtt] password`, so the password need \
               not sit in the config file. Unset by default; an empty value counts as unset.",
    },
    EnvVar {
        name: "SVOLDER_MCP_TOKEN",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "The MCP server's bearer token, outranking `[mcp] token_file` and the generated \
               token file; with it set svolder generates nothing and `svolder mcp token --rotate` \
               refuses. Unset by default; an empty value counts as unset.",
    },
    EnvVar {
        name: "SVOLDER_WEB_TOKEN",
        read_by: "svolder, svolderd",
        dev_only: false,
        what: "The web UI's token, outranking `[web] token_file` and the generated token file; \
               with it set svolder generates nothing and `svolder web token --rotate` refuses. \
               Unset by default; an empty value counts as unset.",
    },
    EnvVar {
        name: "SVOLDER_WEB_ASSETS",
        read_by: "svolderd",
        dev_only: true,
        what: "A directory laid out as `bins/svolderd/web/` that the web UI reads its stylesheet, \
               scripts and fonts from on every request, in place of the copies built in: for \
               working on the page without a rebuild (templates still need one). Unset by \
               default.",
    },
    EnvVar {
        name: "SVOLDER_SYSTEMCTL",
        read_by: "svolder, svolderd, make",
        dev_only: true,
        what: "The `systemctl` binary to run, for `daemon install`/`uninstall`, `doctor`'s \
               Daemon check, and the daemon's own startup checks alike. Default `systemctl`.",
    },
    EnvVar {
        name: "SVOLDER_AUTOEQ_URL",
        read_by: "svolder",
        dev_only: true,
        what: "The `results/` directory `eq search` and `eq import --autoeq` read AutoEq's index \
               and files from, in place of AutoEq's repository on raw.githubusercontent.com: a \
               mirror, or a local server in tests. Default is AutoEq's own.",
    },
    EnvVar {
        name: "SVOLDER_SCREENSHOTS_AUTOEQ_URL",
        read_by: "make",
        dev_only: true,
        what: "The `results/` directory `make screenshots` downloads its one AutoEq file from, \
               in place of AutoEq's repository on raw.githubusercontent.com: a mirror. Not \
               `SVOLDER_AUTOEQ_URL`, which every test binary points at a dead address. Default \
               is AutoEq's own.",
    },
    EnvVar {
        name: "SVOLDER_WPCTL",
        read_by: "svolder, svolderd",
        dev_only: true,
        what: "The `wpctl` binary `svolder tui` reads and sets the host's default sink \
               through (its volume, its mute and its name), `svolder host eq` reads the \
               sinks through, and the daemon reads the default sink through to follow it with \
               its capture (it never sets it). Default `wpctl` on `PATH`.",
    },
    EnvVar {
        name: "SVOLDER_WIREPLUMBER",
        read_by: "svolder",
        dev_only: true,
        what: "The `wireplumber` binary whose `--version` tells `svolder host eq` and \
               `svolder doctor` which WirePlumber this machine has. Default `wireplumber` on \
               `PATH`.",
    },
    EnvVar {
        name: "SVOLDER_PIPEWIRE",
        read_by: "svolder",
        dev_only: true,
        what: "The `pipewire` binary `svolder host eq` runs the computer EQ's filter chain \
               with. Default `pipewire` on `PATH`.",
    },
    EnvVar {
        name: "SVOLDER_HOST_EQ_MANAGER",
        read_by: "svolder",
        dev_only: true,
        what: "What keeps the computer EQ running: `systemd` (a user unit) or `process` (a \
               detached `pipewire` and an autostart entry). Default `systemd` when the user \
               manager answers, else `process`.",
    },
    EnvVar {
        name: "SVOLDER_PW_RECORD",
        read_by: "svolder, svolderd",
        dev_only: true,
        what: "The `pw-record` binary `svolder tui` and the daemon capture the host's default \
               sink's monitor through, for the spectrum and the host level and for the web \
               visualizer; it only ever runs with that sink's node as its target. Default \
               `pw-record` on `PATH`.",
    },
    EnvVar {
        name: "SVOLDER_PW_DUMP",
        read_by: "svolderd",
        dev_only: true,
        what: "The `pw-dump` binary the daemon reads the PipeWire graph through, to find the \
               default sink, the followed player's stream and what else plays into the sink; \
               it is run read-only, only while a front end that shows the path is connected \
               (a `subscribe` that asked for the host, the event stream of a DAC's web page), \
               once at once and then every two seconds. Default `pw-dump` on `PATH`.",
    },
    EnvVar {
        name: "SVOLDER_UNITDIR",
        read_by: "svolder, make",
        dev_only: true,
        what: "Where `daemon install` writes and `daemon uninstall` removes the systemd user \
               units. Default `$XDG_CONFIG_HOME/systemd/user`, else `~/.config/systemd/user`; as root, `/etc/systemd/user`.",
    },
    EnvVar {
        name: "SVOLDER_TEST_PREBUILT",
        read_by: "svolder test suite, make",
        dev_only: true,
        what: "`1` tells the daemon-spawning test helper that `make test`/`make check` already \
               built every workspace binary, so it runs the one on disk. Unset, the helper builds \
               the daemon into `target/test-daemon`, apart from `target/debug`.",
    },
    EnvVar {
        name: "SVOLDER_WRITE_FIXTURES",
        read_by: "svolderd test suite",
        dev_only: true,
        what: "Set to anything, makes the web view's fixture tests rewrite their files in \
               `scripts/web-checks/node/fixtures/` instead of comparing with them: \
               `tone-curve.json` from the core's numbers (run `SVOLDER_WRITE_FIXTURES=1 cargo \
               test -p svolderd curve_fixture` after the response maths changes). Unset, a stale \
               file fails the test.",
    },
    EnvVar {
        name: "SVOLDER_E2E_BIN_DIR",
        read_by: "e2e.sh",
        dev_only: true,
        what: "Directory holding prebuilt `svolder` and `svolderd` for `make e2e` to drive instead \
               of its own cargo build, such as `target/dist/aarch64`. Unset, the run builds and \
               uses `target/debug`.",
    },
    EnvVar {
        name: "SVOLDER_E2E_ADV_ADAPTER",
        read_by: "e2e.sh",
        dev_only: true,
        what: "Controller `make e2e` advertises the peripheral from; set together with \
               `SVOLDER_E2E_SCAN_ADAPTER`, or leave both unset to auto-detect two qualifying \
               controllers.",
    },
    EnvVar {
        name: "SVOLDER_E2E_SCAN_ADAPTER",
        read_by: "e2e.sh, svolder-ble tests",
        dev_only: true,
        what: "Controller `make e2e` scans and connects from; set together with \
               `SVOLDER_E2E_ADV_ADAPTER`, or leave both unset to auto-detect two qualifying \
               controllers.",
    },
    EnvVar {
        name: "SVOLDER_E2E_UNIT",
        read_by: "e2e.sh, svolder-ble tests",
        dev_only: true,
        what: "Unit id `make e2e` starts the peripheral with. Default `0102030405060708`.",
    },
    EnvVar {
        name: "SVOLDER_E2E_SIGNAL_UNIT",
        read_by: "e2e.sh",
        dev_only: true,
        what: "Unit id `make e2e` starts the signal section's own peripheral with, distinct \
               from `SVOLDER_E2E_UNIT` so a stale BlueZ device object from the peripheral that \
               section stops is never mistaken for the new one. Default `0102030405060709`.",
    },
    EnvVar {
        name: "SVOLDER_E2E_LEGACY",
        read_by: "e2e.sh",
        dev_only: true,
        what: "`1` adds a step to `make e2e` that runs the simulator in its legacy layout and \
               checks it advertises again after a link closes; needs bluetoothd started with \
               `--experimental`. Unset by default.",
    },
    EnvVar {
        name: "SVOLDER_E2E_LEGACY_UNIT",
        read_by: "e2e.sh",
        dev_only: true,
        what: "Unit id that step's peripheral advertises, distinct from the others for the \
               reason `SVOLDER_E2E_SIGNAL_UNIT` gives. Default `010203040506070b`.",
    },
    EnvVar {
        name: "SVOLDER_E2E_KEEP_SIM",
        read_by: "e2e.sh",
        dev_only: true,
        what: "`1` leaves `svolder-sim` running after a failing `make e2e` run instead of \
               stopping it, and prints its pid to kill before the next run.",
    },
    EnvVar {
        name: "SVOLDER_E2E_SIM_PID",
        read_by: "svolder-ble tests",
        dev_only: true,
        what: "The running `svolder-sim`'s pid, exported by `make e2e` so a live test can tell \
               the peripheral apart from other devices on the air.",
    },
];

/// The terminal-width fallback `render_text` wraps to when the crate cannot ask a real
/// terminal — stdout piped or redirected, or the query itself failing.
const FALLBACK_WIDTH: usize = 100;

/// The terminal's column count, from `crossterm`'s own query (already a dependency of this
/// binary, for the TUI), else [`FALLBACK_WIDTH`].
pub fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(columns, _rows)| columns as usize)
        .unwrap_or(FALLBACK_WIDTH)
}

/// Which form `svolder help env` was asked to print.
#[derive(Debug, PartialEq, Eq)]
pub enum Format {
    /// The terminal listing.
    Text,
    /// The GitHub Markdown table.
    Markdown,
}

/// Whether `args` (the command line after the program name) asks for the environment-variable
/// table, and in which form.
///
/// Recognizes exactly `help env` and `help env --markdown`; everything else, including a bare
/// `help`, `help <command>`, and any `help env` followed by something other than `--markdown`
/// alone, is left for clap to parse as it does today. Clap's built-in `help` subcommand rejects
/// a topic it has no subcommand for, which `env` is not, so this table's two forms are
/// intercepted here rather than added to the command tree.
pub fn requested(args: &[String]) -> Option<Format> {
    match args {
        [help, env] if help == "help" && env == "env" => Some(Format::Text),
        [help, env, markdown] if help == "help" && env == "env" && markdown == "--markdown" => {
            Some(Format::Markdown)
        }
        _ => None,
    }
}

/// Word-wrap `text` to `width` columns, never splitting a word; one line, unwrapped, when
/// `width` is `0` (nothing sensible to wrap to).
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// One row, `NAME  read by  what it does`, both columns aligned to `name_width`/`read_width`
/// across the whole table and `what` wrapped to `width`, continuation lines indented under its
/// own column.
fn push_row(out: &mut String, var: &EnvVar, name_width: usize, read_width: usize, width: usize) {
    let indent = name_width + 2 + read_width + 2;
    let wrap_width = width.saturating_sub(indent);
    let wrapped = wrap(var.what, wrap_width);
    let mut lines = wrapped.into_iter();
    let name = var.name;
    let read_by = var.read_by;
    let first = lines.next().unwrap_or_default();
    out.push_str(&format!(
        "{name:name_width$}  {read_by:read_width$}  {first}\n"
    ));
    let blank = "";
    for cont in lines {
        out.push_str(&format!("{blank:indent$}{cont}\n"));
    }
}

/// The terminal listing `svolder help env` prints: the everyday variables, then a blank line, a
/// `For tests and development:` heading, and the rest — wrapped to `width` columns.
pub fn render_text(width: usize) -> String {
    let name_width = ENV_VARS.iter().map(|v| v.name.len()).max().unwrap_or(0);
    let read_width = ENV_VARS.iter().map(|v| v.read_by.len()).max().unwrap_or(0);
    let mut out = String::new();
    for var in ENV_VARS.iter().filter(|v| !v.dev_only) {
        push_row(&mut out, var, name_width, read_width, width);
    }
    out.push('\n');
    out.push_str("For tests and development:\n");
    for var in ENV_VARS.iter().filter(|v| v.dev_only) {
        push_row(&mut out, var, name_width, read_width, width);
    }
    out
}

/// One GitHub Markdown table, `Variable | Read by | What it does`.
fn markdown_table(out: &mut String, vars: impl Iterator<Item = &'static EnvVar>) {
    out.push_str("| Variable | Read by | What it does |\n");
    out.push_str("|----------|---------|---------------|\n");
    for var in vars {
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            var.name, var.read_by, var.what
        ));
    }
}

/// `svolder help env --markdown`'s output: the same two groups as [`render_text`], each its own
/// GitHub Markdown table. This is byte-for-byte what belongs between `docs/usage/environment.md`'s
/// `<!-- env-table:begin -->`/`<!-- env-table:end -->` markers; `make docs-env` writes it there
/// and a test fails naming that target when the file has drifted from it.
pub fn render_markdown() -> String {
    let mut out = String::new();
    markdown_table(&mut out, ENV_VARS.iter().filter(|v| !v.dev_only));
    out.push('\n');
    out.push_str("**For tests and development:**\n\n");
    markdown_table(&mut out, ENV_VARS.iter().filter(|v| v.dev_only));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_a_real_svolder_variable_with_every_field_filled_in() {
        for var in ENV_VARS {
            assert!(
                var.name.starts_with("SVOLDER_"),
                "{} does not start with SVOLDER_",
                var.name
            );
            assert!(!var.read_by.is_empty(), "{} names no reader", var.name);
            assert!(
                !var.what.is_empty(),
                "{} says nothing about itself",
                var.name
            );
        }
    }

    #[test]
    fn no_name_repeats() {
        let mut names: Vec<&str> = ENV_VARS.iter().map(|v| v.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "a name is listed more than once");
    }

    #[test]
    fn help_env_is_recognized_and_nothing_else_is() {
        let args = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(requested(&args(&["help", "env"])), Some(Format::Text));
        assert_eq!(
            requested(&args(&["help", "env", "--markdown"])),
            Some(Format::Markdown)
        );
        assert_eq!(requested(&args(&["help"])), None);
        assert_eq!(requested(&args(&["help", "daemon"])), None);
        assert_eq!(requested(&args(&["env"])), None);
        assert_eq!(requested(&args(&["help", "env", "extra"])), None);
        assert_eq!(requested(&args(&[])), None);
        assert_eq!(requested(&args(&["status"])), None);
    }

    #[test]
    fn wrap_never_splits_a_word_and_never_exceeds_the_width_by_more_than_one_word() {
        let text = "a sentence with a few words longer than the width it wraps to";
        for width in [0, 1, 5, 10, 20, 100] {
            let lines = wrap(text, width);
            let joined = lines.join(" ");
            assert_eq!(joined, text);
        }
    }

    #[test]
    fn render_text_lists_everyday_variables_before_the_development_heading() {
        let text = render_text(FALLBACK_WIDTH);
        let heading_at = text
            .find("For tests and development:")
            .expect("the heading is printed");
        assert!(text.find("SVOLDER_TRANSPORT").unwrap() < heading_at);
        assert!(text.find("SVOLDER_SYSTEMCTL").unwrap() > heading_at);
    }

    #[test]
    fn render_text_still_lists_every_variable_at_a_narrow_width() {
        // A width too narrow for even the two aligned columns falls back to an unwrapped
        // `what` column rather than looping or dropping anything.
        let text = render_text(10);
        for var in ENV_VARS {
            assert!(text.contains(var.name), "{} missing at width 10", var.name);
        }
    }

    #[test]
    fn render_markdown_holds_two_tables_split_on_the_same_groups() {
        let markdown = render_markdown();
        let heading_at = markdown
            .find("For tests and development")
            .expect("the heading is printed");
        assert!(markdown.find("SVOLDER_TRANSPORT").unwrap() < heading_at);
        assert!(markdown.find("SVOLDER_SYSTEMCTL").unwrap() > heading_at);
        assert_eq!(markdown.matches("| Variable | Read by").count(), 2);
    }

    /// This workspace's root: `bins/svolder` sits two directories under it.
    fn workspace_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("bins/svolder sits two levels under the workspace root")
            .to_path_buf()
    }

    /// Every `SVOLDER_[A-Z0-9_]+` token `text` holds, added to `found`.
    fn collect_names(text: &str, found: &mut std::collections::BTreeSet<String>) {
        let bytes = text.as_bytes();
        let mut at = 0;
        while let Some(offset) = text[at..].find("SVOLDER_") {
            let start = at + offset;
            let mut end = start + "SVOLDER_".len();
            while end < bytes.len()
                && (bytes[end].is_ascii_uppercase()
                    || bytes[end].is_ascii_digit()
                    || bytes[end] == b'_')
            {
                end += 1;
            }
            found.insert(text[start..end].to_string());
            at = end;
        }
    }

    /// Every file under `dir` whose extension is `ext`, `target/`, `.git/` and `skip` left out,
    /// scanned into `found` by [`collect_names`]. Silent about a directory it cannot read, since not
    /// every workspace checkout has every directory this test asks for (`scripts/`, say, on a
    /// source archive that dropped it).
    fn scan_dir(
        dir: &std::path::Path,
        ext: &str,
        skip: &std::path::Path,
        found: &mut std::collections::BTreeSet<String>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name == "target" || name == ".git" {
                continue;
            }
            if path == skip {
                continue;
            }
            if path.is_dir() {
                scan_dir(&path, ext, skip, found);
            } else if path.extension().and_then(|e| e.to_str()) == Some(ext)
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                collect_names(&text, found);
            }
        }
    }

    /// Every `SVOLDER_*` variable the workspace's own sources read is a row in [`ENV_VARS`], and
    /// every row in [`ENV_VARS`] is a variable some source still reads: scans `bins/`,
    /// `crates/`, `scripts/*.sh`, `Makefile`, `Dockerfile` and `docker-compose.yml` for
    /// `SVOLDER_[A-Z0-9_]+` and compares the names against the table, so the two cannot drift —
    /// a new variable with no row, or a row for one nothing reads any more, fails here by name.
    #[test]
    fn every_variable_the_workspace_reads_is_in_the_table_and_every_row_is_read() {
        let root = workspace_root();
        let mut found = std::collections::BTreeSet::new();
        // This file names every row in the table, so a scan of it would find each row read.
        let table_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/env_vars.rs");
        for dir in ["bins", "crates"] {
            scan_dir(&root.join(dir), "rs", &table_file, &mut found);
        }
        scan_dir(&root.join("scripts"), "sh", &table_file, &mut found);
        for file in ["Makefile", "Dockerfile", "docker-compose.yml"] {
            if let Ok(text) = std::fs::read_to_string(root.join(file)) {
                collect_names(&text, &mut found);
            }
        }

        // Noise the blunt scan cannot tell from a real name: no real variable name ends with
        // `_`, so that catches both this file's own doc comments describing the scan pattern
        // and the `SVOLDER_FAKE_*` wildcard shorthand a few other doc comments use for "every
        // `SVOLDER_FAKE_` variable" — neither names one variable in particular. `SVOLDER_SOCKETS`
        // is a deliberate one-letter-off fixture in
        // `crates/svolder-ipc/src/checks/processes.rs`'s tests, proving the daemon-owner lookup
        // does not match a near-miss name; it is not a variable this workspace reads.
        found.retain(|name| !name.ends_with('_'));
        found.remove("SVOLDER_SOCKETS");

        let table: std::collections::BTreeSet<String> =
            ENV_VARS.iter().map(|v| v.name.to_string()).collect();

        let missing: Vec<&String> = found.difference(&table).collect();
        assert!(
            missing.is_empty(),
            "the workspace reads {missing:?}, which ENV_VARS in bins/svolder/src/env_vars.rs \
             has no row for"
        );

        let unread: Vec<&String> = table.difference(&found).collect();
        assert!(
            unread.is_empty(),
            "ENV_VARS in bins/svolder/src/env_vars.rs has a row for {unread:?}, which nothing \
             in the workspace reads any more"
        );
    }
}
