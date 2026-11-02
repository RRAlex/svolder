// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer EQ, as text: the PipeWire config of a filter chain that corrects what the
//! computer plays, the systemd unit and the autostart entry that keep it running, and the
//! state file that remembers what is applied.
//!
//! Everything here is pure except [`HostEqState::load`] and [`HostEqState::save`]: no program
//! is run and nothing is written beside the state file. `svolder host eq` (in the `svolder`
//! binary) decides what to do with the text.
//!
//! The config follows what a live check on PipeWire 1.6 with WirePlumber 0.5 confirmed.
//!
//! - **Smart route** (WirePlumber 0.5 and newer): the chain is a smart filter in front of the
//!   target sink. WirePlumber links every stream to the filter and the filter to the sink, and
//!   again when the sink goes away and returns. `node.dont-fallback` and `node.linger` ship
//!   together: with `node.dont-fallback` alone the filter's nodes are destroyed when the sink
//!   is unplugged and never come back while the process lives. `priority.session = 0` is
//!   insurance for older 0.5 releases; it does not keep the filter from being elected as the
//!   default when it is the only sink left.
//! - **Default route** (older WirePlumber): the chain is a sink of its own that plays into
//!   the target through `target.object`, and the caller makes it the default output. When the
//!   target is unplugged the filter stays the default and plays into the next sink, filtered.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};

/// How the filter chain is put in front of the target sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    /// A WirePlumber smart filter targeting the sink (WirePlumber 0.5 and newer).
    Smart,
    /// A sink of its own that plays into the target, made the default output.
    Default,
}

impl Route {
    /// The word the state file and `host eq status` use for the route.
    pub fn word(self) -> &'static str {
        match self {
            Route::Smart => "smart",
            Route::Default => "default",
        }
    }

    /// The route a WirePlumber of `version` takes: smart from 0.5, default before.
    pub fn for_wireplumber(version: (u32, u32, u32)) -> Route {
        match (version.0, version.1) >= (0, 5) {
            true => Route::Smart,
            false => Route::Default,
        }
    }
}

/// What keeps the filter chain running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Manager {
    /// A systemd user unit.
    Systemd,
    /// A detached `pipewire` process and an autostart entry.
    Process,
}

impl Manager {
    /// The word the state file, `host eq status` and `SVOLDER_HOST_EQ_MANAGER` use.
    pub fn word(self) -> &'static str {
        match self {
            Manager::Systemd => "systemd",
            Manager::Process => "process",
        }
    }

    /// The manager `word` names, in any case and with blanks round it.
    pub fn parse(word: &str) -> Option<Manager> {
        match word.trim().to_ascii_lowercase().as_str() {
            "systemd" => Some(Manager::Systemd),
            "process" => Some(Manager::Process),
            _ => None,
        }
    }
}

/// The name of the filter chain's sink, which is also the node name of its capture side.
pub const NODE_NAME: &str = "svolder_eq";

/// The environment variable that names the `wireplumber` whose version is read; `wireplumber`
/// on `PATH` when it is unset or empty.
pub const WIREPLUMBER_ENV: &str = "SVOLDER_WIREPLUMBER";

/// The WirePlumber version `wireplumber --version` reports as linked, from its banner:
/// `Linked with libwireplumber 0.5.13` gives `(0, 5, 13)`. A missing patch number reads as 0.
/// `None` when no such line is there or its version does not parse.
pub fn wireplumber_version(banner: &str) -> Option<(u32, u32, u32)> {
    let line = banner
        .lines()
        .find(|line| line.contains("Linked with libwireplumber"))?;
    let version = line
        .split("libwireplumber")
        .nth(1)?
        .split_whitespace()
        .next()?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = match parts.next() {
        Some(patch) => patch.parse().ok()?,
        None => 0,
    };
    Some((major, minor, patch))
}

/// A number as the PipeWire config reads it: no exponent, and `-0` written as `0`.
///
/// Every number is checked to be finite before a config is rendered (`apo::validate`), so a
/// value that is not is a bug of the caller and stops here rather than being written as
/// something else.
fn num(value: f64) -> String {
    assert!(
        value.is_finite(),
        "a correction is validated before it is rendered"
    );
    format!("{}", value + 0.0)
}

/// `text` as a quoted string of the PipeWire config: control characters dropped, `\` and `"`
/// escaped.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars().filter(|c| !c.is_control()) {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The `type` a filter kind has in `param_eq`.
fn builtin_type(kind: FilterKind) -> &'static str {
    match kind {
        FilterKind::Peaking => "bq_peaking",
        FilterKind::LowShelf => "bq_lowshelf",
        FilterKind::HighShelf => "bq_highshelf",
    }
}

/// The complete standalone PipeWire config (`pipewire -c <file>`) of a filter chain applying
/// `c` in front of the sink whose `node.name` is `target`, for `route`.
///
/// The chain is one builtin `param_eq` node whose first filter is the preamp, a high shelf at
/// 0 Hz, followed by the correction's filters in order, with the FL and FR ports named
/// explicitly.
pub fn render_conf(c: &Correction, target: &str, route: Route) -> String {
    let mut filters = String::new();
    let _ = writeln!(
        filters,
        "                                {{ type = bq_highshelf freq = 0 gain = {} q = 1.0 }}",
        num(c.preamp_db)
    );
    for filter in &c.filters {
        let _ = writeln!(
            filters,
            "                                {{ type = {} freq = {} gain = {} q = {} }}",
            builtin_type(filter.kind),
            num(filter.f),
            num(filter.g),
            num(filter.q)
        );
    }
    let target = quoted(target);
    // The lines that differ by route: the capture side's smart-filter properties, and the
    // playback side's `target.object` before `stream.dont-remix` or its `node.dont-fallback`
    // and `node.linger` after it, in the order of the config the live check ran.
    let (capture, playback_head, playback_tail) = match route {
        Route::Smart => (
            format!(
                "                filter.smart = true\n\
                 \x20               filter.smart.name = \"svolder-eq\"\n\
                 \x20               filter.smart.target = {{ node.name = {target} }}\n\
                 \x20               priority.session = 0\n"
            ),
            String::new(),
            "                node.dont-fallback = true\n\
             \x20               node.linger = true\n"
                .to_string(),
        ),
        Route::Default => (
            String::new(),
            format!("                target.object = {target}\n"),
            String::new(),
        ),
    };
    format!(
        "# Written by `svolder host eq apply`; `svolder host eq remove` deletes it.\n\
         context.properties = {{\n\
         \x20   log.level = 2\n\
         }}\n\
         \n\
         context.spa-libs = {{\n\
         \x20   audio.convert.* = audioconvert/libspa-audioconvert\n\
         \x20   support.*       = support/libspa-support\n\
         }}\n\
         \n\
         context.modules = [\n\
         \x20   {{ name = libpipewire-module-rt\n\
         \x20       args = {{ }}\n\
         \x20       flags = [ ifexists nofail ]\n\
         \x20   }}\n\
         \x20   {{ name = libpipewire-module-protocol-native }}\n\
         \x20   {{ name = libpipewire-module-client-node }}\n\
         \x20   {{ name = libpipewire-module-adapter }}\n\
         \x20   {{ name = libpipewire-module-filter-chain\n\
         \x20       args = {{\n\
         \x20           node.description = \"svolder EQ\"\n\
         \x20           media.name       = \"svolder EQ\"\n\
         \x20           filter.graph = {{\n\
         \x20               nodes = [\n\
         \x20                   {{\n\
         \x20                       type  = builtin\n\
         \x20                       name  = eq\n\
         \x20                       label = param_eq\n\
         \x20                       config = {{\n\
         \x20                           filters = [\n\
         {filters}\
         \x20                           ]\n\
         \x20                       }}\n\
         \x20                   }}\n\
         \x20               ]\n\
         \x20               inputs  = [ \"eq:In 1\" \"eq:In 2\" ]\n\
         \x20               outputs = [ \"eq:Out 1\" \"eq:Out 2\" ]\n\
         \x20           }}\n\
         \x20           audio.channels = 2\n\
         \x20           audio.position = [ FL FR ]\n\
         \x20           capture.props = {{\n\
         \x20               node.name = \"{NODE_NAME}\"\n\
         \x20               node.description = \"svolder EQ\"\n\
         \x20               media.class = Audio/Sink\n\
         {capture}\
         \x20           }}\n\
         \x20           playback.props = {{\n\
         \x20               node.name = \"{NODE_NAME}.output\"\n\
         \x20               node.passive = true\n\
         {playback_head}\
         \x20               stream.dont-remix = true\n\
         {playback_tail}\
         \x20           }}\n\
         \x20       }}\n\
         \x20   }}\n\
         ]\n"
    )
}

/// One word of a systemd `ExecStart=` line: quoted when it holds a blank or a quote, with `%`
/// and `$` doubled so neither is expanded.
fn exec_word(word: &str) -> String {
    let plain = word
        .chars()
        .all(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '\\'));
    let mut out = String::new();
    if !plain {
        out.push('"');
    }
    for c in word.chars() {
        match c {
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            '"' | '\\' if !plain => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if !plain {
        out.push('"');
    }
    out
}

/// The systemd user unit that runs `pipewire -c conf`, modelled on PipeWire's own
/// `filter-chain.service`: it follows the PipeWire service, goes when it goes and restarts on
/// failure. `pipewire` and `conf` are absolute paths.
pub fn render_unit(pipewire: &Path, conf: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=svolder host EQ (PipeWire filter chain)\n\
         After=pipewire.service pipewire-session-manager.service\n\
         BindsTo=pipewire.service\n\
         \n\
         [Service]\n\
         LockPersonality=yes\n\
         MemoryDenyWriteExecute=yes\n\
         NoNewPrivileges=yes\n\
         RestrictNamespaces=yes\n\
         SystemCallArchitectures=native\n\
         SystemCallFilter=@system-service\n\
         Type=simple\n\
         ExecStart={} -c {}\n\
         Restart=on-failure\n\
         Slice=session.slice\n\
         \n\
         [Install]\n\
         WantedBy=pipewire.service\n",
        exec_word(&pipewire.to_string_lossy()),
        exec_word(&conf.to_string_lossy()),
    )
}

/// One argument of a desktop entry's `Exec=` line, as the file holds it.
///
/// An argument with a reserved character is quoted, and inside the quotes `"`, `` ` ``, `$`
/// and `\` each take a backslash (the Exec rule). The value of a desktop entry is a string
/// whose own rule, applied first when it is read, turns `\\` into `\`, so every backslash of
/// the Exec rule is written doubled: a quote reads `\\"`, a dollar `\\$` and a backslash
/// `\\\\` in the file. `%` is doubled in any argument.
fn desktop_word(word: &str) -> String {
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
    let quote = word.chars().any(|c| RESERVED.contains(c));
    let mut out = String::new();
    if quote {
        out.push('"');
    }
    for c in word.chars() {
        match c {
            '%' => out.push_str("%%"),
            '\\' if quote => out.push_str("\\\\\\\\"),
            '"' | '`' | '$' if quote => {
                out.push_str("\\\\");
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if quote {
        out.push('"');
    }
    out
}

/// The autostart entry that starts `pipewire -c conf` at login when no systemd user manager
/// keeps it running. `pipewire` and `conf` are absolute paths.
pub fn render_desktop(pipewire: &Path, conf: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=svolder host EQ\n\
         Exec={} -c {}\n\
         NoDisplay=true\n",
        desktop_word(&pipewire.to_string_lossy()),
        desktop_word(&conf.to_string_lossy()),
    )
}

/// What the computer EQ is, and was last, as `host-eq.toml` keeps it.
///
/// Everything a re-apply needs is here, so the last correction can be applied again without
/// the preset it came from. After a remove, `on` is false and the rest stays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEqState {
    /// Whether the filter chain is applied.
    pub on: bool,
    /// The preset's name, or a file's name.
    pub name: String,
    /// Where the correction came from, as free text; absent for a band preset or a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// The `node.name` of the sink the chain sits in front of.
    pub target: String,
    /// How the chain is put in front of it.
    pub route: Route,
    /// What keeps it running.
    pub manager: Manager,
    /// On the default route, the `node.name` of the default output before the chain took
    /// over, which a remove restores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_default: Option<String>,
    /// The preamp in dB.
    pub preamp_db: f64,
    /// The filters, in order.
    #[serde(default)]
    pub filters: Vec<SourceFilter>,
}

impl HostEqState {
    /// The correction this state applies.
    pub fn correction(&self) -> Correction {
        Correction {
            preamp_db: self.preamp_db,
            filters: self.filters.clone(),
        }
    }

    /// The state at `path`: `None` when there is no file, and an error naming the file when it
    /// cannot be read or is not a state.
    pub fn load(path: &Path) -> Result<Option<HostEqState>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        toml::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Write the state to `path`, atomically, creating its directory.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| format!("{}: {e}", path.display()))?;
        write_atomically(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Write `bytes` to `path` through a temporary file beside it that is renamed over it, so a
/// reader sees the old file or the new one and never half of either. The directory is created.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    Staged::write(path, bytes)?.commit()
}

/// A file written beside the one it is to replace and not yet renamed over it, so what a write
/// can refuse (a full disk, a directory that cannot be written) is refused before anything at
/// its path changes. [`Staged::commit`] puts it in place; dropped uncommitted, it is taken away.
#[derive(Debug)]
pub struct Staged {
    /// The temporary file beside `path`, until it is committed.
    temporary: Option<PathBuf>,
    /// Where it goes.
    path: PathBuf,
}

impl Staged {
    /// Write `bytes` to a temporary file beside `path`, creating the directory.
    pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<Staged> {
        let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty());
        if let Some(dir) = dir {
            std::fs::create_dir_all(dir)?;
        }
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".tmp{}", std::process::id()));
        let temporary = path.with_file_name(name);
        // Made before the write, so a write that fails part of the way is taken away by `Drop`.
        let staged = Staged {
            temporary: Some(temporary.clone()),
            path: path.to_path_buf(),
        };
        std::fs::write(&temporary, bytes)?;
        Ok(staged)
    }

    /// Rename the written file over its path.
    pub fn commit(mut self) -> std::io::Result<()> {
        let Some(temporary) = self.temporary.take() else {
            return Ok(());
        };
        let renamed = std::fs::rename(&temporary, &self.path);
        if renamed.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        renamed
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if let Some(temporary) = self.temporary.take() {
            let _ = std::fs::remove_file(temporary);
        }
    }
}

#[cfg(test)]
mod tests;
