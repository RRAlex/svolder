<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# IPC and configuration

`svolder-ipc` is what the front ends and the daemon share besides the device itself: the socket
protocol they speak, the configuration file they all read and some of them write, the paths and
tokens that locate and guard them, and the host health checks. This page is how those work and what
keeps them from breaking each other. The map's [crate table](../architecture.md#the-crates) says
where the crate sits; the daemon that serves the socket is in [daemon.md](daemon.md); what each
setting does for a user is in the [configuration reference](../usage/config.md).

## The socket

Everything that is not a device reading is one JSON object per line, in both directions
(`crates/svolder-ipc/src/socket/protocol.rs`). A front end sends a `RequestLine` (an id and a
`Request`, tagged by `cmd`) and gets back a `Reply::Response` carrying the same id. A
`Request::Subscribe` turns the connection into a stream of `Reply::Event`s: `Status`, `Connection`,
`Fault`, `Error`, `Recalled`, `Climb` and `Host`; a `HostFrames` request turns its connection into
a stream of `HostCapture` and `HostFrame` instead. The requests are the verbs: `Devices`, `State`,
`Get`, `Set`, `Raw`, the `Eq*` family, `Passphrase`, `Identify`, `Scan`, `History`, `Readings`,
`Recalled`, `WebPair`, `WebLink`, `WebLinkOpen`, `Reload`, `Version`, `Stop`, `Host`, `HostPress`
and `HostFrames`.

The host's view crosses the wire as `HostView` (`crates/svolder-ipc/src/host.rs`): whether the
daemon watches the host at all, the followed player's `PlayingView` and the `RouteFacts` read out of
PipeWire. `Response::Host` answers `Host`, and `HostPress` is answered `Ack` or an error;
`Event::Host` rides a `Subscribe` stream that asked for it (`host: true`), whatever its device
filter, as `Event::Error` does, since it belongs to the daemon and not to a device; a plain
`Subscribe` gets none, and `svolder watch` does not ask. `HostPress` keeps its button as the plain
word on the wire, so an unknown button is the daemon's error to give and not a decode failure.
Lengths and positions are microseconds, rates thousandths, stamps unix milliseconds: whole numbers
throughout, so the types stay comparable. The route's types decode leniently, so a later daemon's
view still reaches this build: a field that is missing reads as its default, and a way of finding
the player that this build has no name for reads as `NotFound` (the player is not identified).

The capture crosses the wire on its own request, `HostFrames`, answered with events and not with a
`Response`: `Event::HostCapture(CaptureView)` (`state` `starting`, `running` or `unavailable`, with
`sink` or `why` when they apply) once and on each change, and `Event::HostFrame(FrameView)` (`t`,
`left` and `right` as 240 bytes each, `levels`, `beat`, `bpm`, `phase`) while it runs; the
connection holds the daemon's capture for its life ([host-capture.md](host-capture.md)). A handler
that has no frames (`Handler::host_frames` defaults to an error, as does a daemon from before the
request) answers an error `Response` and the connection serves on; `Client::host_frames` returns it
as an error. A client from before the events skips them as events it does not know. Both new types
are in `crates/svolder-ipc/src/host.rs`; none of their fields is required of a later peer.

The verdict on the route is one pure function, `path_line`, in the same module, called by whoever
knows the device: it takes the daemon's `RouteFacts` and the device's own readings (`DacFacts`: its
display name, input, sample rate and, for a DAC with a reading per USB port, whether each port is
connected) and gives the hops from the player to the DAC, a `Verdict` and
the words that name what changes the signal. `sink_is_dac` decides first whether the computer's sink
is the device at all (Schiit's vendor id `0x30be` and a product name that is the display name or
holds it as whole words, not followed by a model number: `Modi` is not `Modius`, `Bifrost` is not
`Bifrost 3`); if not, the line says where the sound goes and gives no verdict. So does a DAC
whose input is not a USB one (`Verdict::OtherInput`: its first label word is not `USB`), since the
computer's stream is not what it plays. On a DAC whose profile reads each USB port (`usb1_connected`
and `usb2_connected`: the Gungnir 2 and the Yggdrasil) a `USB 1` or `USB 2` input is the computer's
only when its own port reads connected and the other does not: with only the other port connected
the line is `OtherInput` too (`the DAC listens to USB 1 · the computer is on USB 2`), and with both
connected, or a reading missing, the path is `Unknown` (`both USB ports connected`, `USB ports not
read`). A DAC with one USB port (the Bifrost 3) or no port readings keeps the rule that its USB
input is the computer's. The verdict is `Untouched` only when every hop is known
clean (any volume but 100 % is a change, a boost included; an assumed player is said after it),
`Unknown` when nothing changes the signal but something could not be read (the DAC's input
included), and the words never claim more than the rates, volumes, mixing and EQ show. Both front
ends join the words with ` · `.

A reading always travels as a `ValueObject`: the function id, the profile's key and label, the wire
type and raw value, a display string, and the converted number and unit when the control has one. A
front end therefore never converts a value itself, and a TUI on the socket and a session of its own
hand the same object to the same screen. It carries `heard_at_ms`, when the daemon last heard that
reading from the device, when it knows: it is on the wire, rather than kept by a front end, so an
age survives a page or a TUI that reconnects. `DeviceInfo.signal` is a `Signal`: the device's
strength in dBm, whether it came from an advertisement or the live link, and when it was heard;
`Signal::describe` (`crates/svolder-ipc/src/socket/signal.rs`) is the one way the CLI, the TUI and
the web word it, and `SignalSource::word` the one word for a source. A source a newer daemon adds
decodes as `SignalSource::Unknown` (`#[serde(other)]`), worded `unknown`, so an older client still
reads the whole answer.

The server (`crates/svolder-ipc/src/socket/server.rs`) is generic over a `Handler` trait with two
methods, `handle` and `subscribe`; the daemon's `Daemon` implements it. `bind` creates the socket's
directory, refuses to start if a live daemon answers on the path, clears a stale socket file, and
leaves the file at mode 0600. `serve_bound` removes the file at the end only when its inode is still
the one it bound; `serve_on` serves a listener a service manager handed over and never removes the
file. `Client` (`socket/client.rs`) is the other half: `connect`, one `request` at a time, and
`subscribe`. `is_running` is a connect and nothing more.

Both ends read lines through `LineReader` (`socket/lines.rs`), which holds at most `MAX_LINE` (1
MiB) of one. A longer line is answered as a bad request and read no further, and a line that is not
text is the handler's to answer, not an I/O error that ends the connection.

### Where the socket is

`socket::path::resolve` (`crates/svolder-ipc/src/socket/path.rs`) picks the path the same way in the
daemon and every client: `--socket`, then `SVOLDER_SOCKET`, then `socket` in the configuration, then
`svolder.sock` in `$XDG_RUNTIME_DIR`, then in `/run/user/<uid>` if that directory exists, and last
in a private `/tmp/svolder-<uid>` that `ensure_parent` creates at mode 0700. A blank override is no
override: an empty string falls through, because an empty path would bind an anonymous abstract
socket no client can reach. Only the `/tmp` fallback directory is ever created; any other parent is
the user's to provide.

### Versions that differ

A daemon and a front end of different builds talk to each other: a package upgrade leaves the old
daemon running until it is restarted. So the protocol tolerates what it does not know. Every field
added after the first release is `#[serde(default)]` (and `skip_serializing_if` where absence keeps
the old meaning); a peer ignores a field it does not know; a reply or event kind it does not know
decodes as `Unknown` and is skipped; a request kind a daemon does not know is answered with an error
and the connection stays open. There is no handshake. `Request::Version` and `daemon_version::skew`
(`crates/svolder-ipc/src/socket/daemon_version.rs`) let a front end notice the difference and say
`svolder daemon restart` (`RESTART_COMMAND`) rather than fail.

## The configuration file

`~/.config/svolder/config.toml` (`Config::path`, honouring `$XDG_CONFIG_HOME`) is read by every
program and written by the CLI's `config` and `device` verbs, the TUI's Settings, and the web UI's
preset and device edits. `Config` (`crates/svolder-ipc/src/config.rs`) holds the checked result;
`Config::parse` (`config/parse.rs`) is the one seam every test and every loader uses. An absent file
is the default configuration, not an error, so a bare machine runs.

What parsing checks, and why it is strict where it is:

- `schema` must be 1 (`ConfigError::Schema`). A file from a future layout is refused rather than
  half understood.
- Each `[[device]]` must carry a parseable 8-byte `udid` and a valid `passphrase` (`auto`, four
  digits or `none`). A device is found again by its unit id, so a rename or a moved radio address
  changes nothing.
- A `[[preset]]` block is validated by `parse_preset` from raw values so that every error names the
  preset, and a `loudness` or `bands` out of range is refused. An unknown key in a preset, a band,
  or `[tui.effects]` is an error that names itself and the keys that are valid. Unknown keys at the
  top level of the file are ignored, so a file written by a newer svolder still loads.
- A `[[hop]]` block is validated by `parse_hop` from raw values, so every error names the hop. It
  holds exactly one of `name` (which needs a `kind` of four words) and `device`; hops are listed in
  file order and no two share an id (the name, or the linked device), compared as preset names are.
  A `device` or `after` naming no configured device is not an error but a warning, so removing a
  device never bricks the file. `add_hop`, `update_hop`, `remove_hop` and `move_hop`
  (`crates/svolder-ipc/src/config/hops.rs`) write them like the preset writers; `update_hop` takes
  a whole `Hop`, so a caller merges a partial edit first.
- `Config::warnings` lists what is legal but probably wrong: a `default_device` naming no device, a
  hop linking or following a device that is not configured, a web guest setting that cannot apply or
  leaves a wildcard listen open to DNS rebinding, and a web UI and an MCP server on one port. The
  daemon logs them at every load.

The tables are `[daemon]` (`autospawn`), `[tui]`, `[mqtt]`, `[mcp]`, `[web]`, `[autoeq]`
(`refresh_weeks`), `[host]` (`enabled`, on by default) and the repeated `[[device]]`, `[[preset]]`
and `[[hop]]`. `MqttConfig` serialises its password as `***` (`redacted`), so `config show --json`
and a debug print never leak it, and `SVOLDER_MQTT_PASSWORD` overrides it at connect time so it need
not sit in the file.

`[host]` has one key, `enabled`, true unless set otherwise: the daemon is the per-user service on
the computer that plays, so following the player and reading the route is its job. `enabled =
false`, or `SVOLDER_HOST=off` in the daemon's environment, makes its `HostView` say `available =
false`, and a joined TUI then keeps its own watcher; `[tui] host_audio = false` turns the host off
in the TUI whatever the daemon says. A reload applies a changed `[host]` without a restart.

### The template

`Config::template` (`config/template.rs`) is the whole file with every key commented out at the
value it has while absent, one line above each saying what it does, and an example device block.
`schema = 1` is the only live line. Because the values come from the same `Default` implementations
the parser falls back on, uncommenting any line changes nothing, and the template cannot drift from
the parser. `Config::create_from_template` writes it for a first configuration (under the lock, only
if the file does not exist), and `config template` prints it.

The plain `[tui]` keys are one table entry each (`TUI_SETTINGS` in `config/tui_settings.rs`): the
name in the file, what the template says, and how the Settings box labels it. The writer and the
template walk the table, so a key added there is written, commented and read back the same way
everywhere; `config/tui_settings.rs` lists the four places a new key touches, and tests walk the
table and fail for a key that skips one.

### Writing the file

Every writer goes through `edit_file` (`config/edit.rs`):

1. make the parent directory at 0700;
2. take an exclusive `flock` on `<path>.lock` (`flock::lock_beside`), waiting at most two seconds;
3. read the file as a TOML document (or start from the template if it is absent) and edit it with
   `toml_edit`, which keeps every comment, blank line and commented-out default round the keys it
   touches;
4. parse the result with `Config::parse`;
5. write it through a temporary file in the same directory, fsync, rename over the target, and fsync
   the directory.

A file whose edited text equals what was read is not written, so its mtime stays and a symbolic link
stays one. A new file is created at 0600 whatever the umask, and one that exists keeps its mode (the
temporary file is opened with it), so a configuration locked down because it holds a passphrase is
never briefly world-readable. A file its owner cannot write is refused (`ConfigError::ReadOnly`),
because a configuration made read-only is read-only on purpose; `Config::writable` says so in words
before a screen offers to save. A lock file that cannot be opened means no lock, and the write goes
on. The connection writers (`config/connections.rs`) name the keys they write one by one and never
write the broker password or the token file.

## Starting a daemon: the launcher chain

A verb run with `--transport auto` (`bins/svolder/src/backend/launch.rs`, `reach`) walks four steps
and stops at the first that works:

1. a daemon answers on the resolved socket (a listening systemd socket unit counts: connecting to it
   starts the service behind it);
2. a socket unit that is installed but not listening is started, and the socket is tried again;
3. the front end starts a `svolderd` of its own (`autospawn`), unless `[daemon] autospawn` is false
   or systemd owns the socket unit;
4. a session of the front end's own (`Reached::Direct`). `--transport daemon` walks 1 to 3 and fails
   instead of taking this step.

`autospawn` finds the binary beside the running `svolder`, then on `PATH`; starts it in a session of
its own with output appended to the log in the state directory; hands it only the environment that
matters (`daemon_env_from`: a `SVOLDER_TRANSPORT` the daemon understands, and `SVOLDER_ADAPTER`) and
removes `LISTEN_FDS` and `LISTEN_PID`; and waits up to `AUTOSPAWN_WAIT` (two seconds) for the
socket. Two front ends that start one at once end with one daemon: the second finds the lock taken
and refuses, and its client reaches the first on the next look. A daemon that will not start is
reported (`AutospawnFailed`, in the TUI as a notice) and the verb still runs on a direct session.
`probe` is step 1 alone, for `doctor`, which must not change the machine it is checking.

## Tokens

The web UI and the MCP server each have a bearer token (`crates/svolder-ipc/src/token.rs`,
`Store::WEB`, `Store::MCP`). `resolve` takes the first of: the environment variable
(`SVOLDER_WEB_TOKEN`, `SVOLDER_MCP_TOKEN`) when not empty, the configuration's `token_file`, and a
file svolder generates under `$XDG_STATE_HOME/svolder/` with 32 random bytes. A file read from disk
must not be readable by group or others, is at most 4096 bytes, and a token is at most
`MAX_TOKEN_LEN` (505) characters, so `Bearer ` and the token fit the 512-byte header limit the
servers enforce. `rotate` replaces only a token svolder generated; it refuses one that comes from
the environment or a named file, because svolder does not own those. `peek` reads without making
one, so looking never creates a secret. `token_matches` compares in time that does not depend on
where the two differ, and no error or `Debug` print contains a token. The daemon reports where each
token comes from and its last characters (`TokenInfo`), so a front end can say which is in use
without showing it.

## Host checks

`checks::run` (`crates/svolder-ipc/src/checks.rs`) turns a `Probes` snapshot of what BlueZ, systemd,
the environment and the configuration say into one `Outcome` per check, in reading order. Nothing in
`checks` touches the system: `checks::live` takes the snapshot, so `doctor` prints the outcomes and
the daemon logs them (`report_health`) with no probes of their own, and tests drive every branch
from a fixture. The checks that need a radio, grading adapters and scanning for devices, belong to
the front end that has a session.

## Principles

**The protocol tolerates the other side's age.** New fields default, unknown fields and kinds are
skipped, unknown requests are answered. Make a field required or reject an unknown kind and an
upgrade breaks the front end that was left running.

**A value object, never a raw reading.** The daemon converts through the profile once; front ends
display. Add a front end that converts on its own and the same reading shows two ways.

**A writer proves its output parses.** `edit_file` parses what it produced before it replaces the
file, so no writer can leave a configuration the next run refuses. Skip that and a TUI save can
brick the daemon's next start.

**Writers take turns, and keep the user's file.** The lock beside the file makes two writers (a TUI
and the daemon, or two TUIs) add different presets without one losing the other's, and `toml_edit`
plus atomic rename keep comments, modes and links. Rewrite the file from a struct and every hand
edit is lost.

**The template is generated from the defaults.** A hand-maintained template would drift from the
parser until uncommenting a line changed behaviour. Keep it derived.

**The wire carries facts, the verdict is drawn where the device is known.** The daemon sends what it
read of the player and the graph, and each front end judges it against its own device's readings
with the one `path_line`, so a page of one DAC and a TUI on another can say different things of the
same computer, and a later daemon's richer view still decodes.

**A secret never travels in an error.** Passwords serialise as `***`, tokens are absent from errors
and `Debug`, and key files must be private. Anything new that holds a secret follows suit.

**Looking does not change the machine.** `probe`, `peek` and `doctor` never start a daemon or create
a token. A check that altered what it checked would report on its own work.

**The chain always ends somewhere that works.** With systemd, without, and with autospawn off,
`auto` reaches a daemon or a direct session. Add a step only if it keeps that.

## Where the code and tests live

- Protocol, server, client, lines: `crates/svolder-ipc/src/socket/protocol.rs`, `server.rs`,
  `client.rs`, `lines.rs`; tests in the `protocol/tests.rs` and `lines/tests.rs` beside them and in
  `server.rs`.
- Socket paths and adoption: `crates/svolder-ipc/src/socket/path.rs`; tests in
  `crates/svolder-ipc/tests/adoption.rs`.
- Configuration: `crates/svolder-ipc/src/config.rs` and `crates/svolder-ipc/src/config/`; tests in
  `crates/svolder-ipc/src/config/tests/`, and the table tests under `config/tui_settings/`.
- Lock: `crates/svolder-ipc/src/config/flock.rs`, tests in `config/flock/tests.rs`.
- Tokens: `crates/svolder-ipc/src/token.rs`, tests in `token/tests.rs`.
- Signal strength and its wording: `crates/svolder-ipc/src/socket/signal.rs`, tests in
  `socket/signal/tests.rs`.
- Version skew: `crates/svolder-ipc/src/socket/daemon_version.rs`, tests in
  `socket/daemon_version/tests.rs`.
- Host checks: `crates/svolder-ipc/src/checks.rs`, `checks/live.rs`, tests in `checks/tests.rs`.
- The launcher chain: `bins/svolder/src/backend/launch.rs`, tests in `backend/launch/`.
