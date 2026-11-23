<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Device verbs

The device verbs: `doctor`, `setup`, `devices`, `add`, `remove`, `hops`, `passphrase`, `scan`,
`inspect`, `status`, `get`, `set`, `preset`, `scene`, `watch`, `panel`, `host`, `raw`, `decode` and
`profiles`. Back to the [usage index](README.md).

## doctor

Check the host, the running daemon and every configured device, printing one line per check and,
under a line that warns or fails, the fix — a root fix printed as the exact command and never run,
an unprivileged one collected and offered at the end. It observes rather than arranges: no step of
the launcher chain runs from here, so a daemon that is not answering is reported rather than
started. The one thing it does do is connect to the socket, which on a host whose socket unit is
listening is itself what starts the service behind it — that is systemd's arrangement, not a step
this verb takes.

```
Check the host and each configured device, and offer fixes

Usage: svolder doctor [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per check instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --yes                    Apply the fixes this verb can apply itself without asking
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --dev                    Also check what a second adapter and a patched bluetoothd give
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

**Checks**, one snapshot of the host answering all of them so every line is about the same moment,
in this order — `✓` pass, `!` warn, `✗` fail, `·` a fact worth recording either way:

| check | says |
|---|---|
| Bluetooth service | whether `org.bluez` answers on the system D-Bus |
| Adapters | which adapters BlueZ reports, and whether the configured `adapter` is missing or unpowered |
| Adapter grade | (a device configured, and more than one adapter present) each adapter's catch rate against the first configured device, best first as the setup wizard lists them, with adapters that tied left in the order the host reports them; warns when the configured `adapter` is not the best of them |
| Runtime | `$XDG_RUNTIME_DIR`, and whether the socket's own directory can be written to |
| Daemon | whether the systemd units are installed, enabled, and pointed at the resolved socket; a note, not a failure, where there is no systemd at all — autospawn or a direct session serve instead. A daemon answering on the socket with a version other than this svolder's grades `!` (`daemon vX.Y.Z is older than this svolder vA.B.C`, or `newer than` when the daemon is the later build, compared in semver order) and, for an older daemon, gives the fix `svolder daemon restart`, which doctor prints and never runs; a newer daemon has no fix to print, since restarting it changes nothing; a daemon that says its `config.toml` changed since its last reload (a hand edit it has not taken) grades `!` with its words (`config: PATH changed since the last reload: svolder daemon reload (or SIGHUP)`) and, where the check has no fix of its own, the fix `svolder daemon reload`, printed and never run, since a reload changes the devices; a daemon whose `config.toml` is gone grades `!` with `config: PATH is missing: the daemon keeps its running configuration; restore the file, then svolder daemon reload` and no fix, since a reload of a missing file is refused |
| Other daemons | one row per other svolderd of this user's listening on a socket this session does not reach — neither the one it resolves nor the one the systemd unit listens on — whoever started it and whatever network namespace it runs in: its pid, where it listens (the listening socket it holds, or its `--socket`/`SVOLDER_SOCKET` when its descriptors or its namespace's socket table cannot be read; for a daemon in another mount namespace, such as a container, that path reached through `/proc/<pid>/root`), and the command that stops that one, which doctor prints and never runs |
| Config | whether the file parses, whether every device names a profile that exists, and whether any device is configured at all; a profile directory beside the file that cannot be read is named, with the reason, and a device whose profile may be in it is not called missing |
| Config file | (no file yet) offers to create one from the template |
| Devices | (devices configured) one row per device: connected where a daemon holds it — no scan is run for one, since a device a central holds stops advertising — else seen in a scan and how loud, or not seen in the windows this verb listened |
| MCP server | (`[mcp]` on, or a daemon reporting one) the address the daemon listens on and how requests prove themselves (`listening on 127.0.0.1:8737 (token, read-only)`); fails when the daemon could not bind the address or read the token, when a token file group or others can read, or when a named `token_file` is missing; reads no token and creates none |
| Web UI | (`[web]` on, or a daemon reporting one) the address the daemon listens on and how a browser proves itself (`listening on 127.0.0.1:8738 (token, writes)`); fails when the daemon could not bind the address or read the token, when `auth = "none"` is on an address that is not loopback, and when a token file group or others can read, or a named `token_file` is missing; warns when the section is on and the daemon says its web UI is off; notes a listen address off loopback, which serves plain HTTP on the network; reads no token and creates none |
| PipeWire config | (a daemon answering with a route) the PipeWire config check's summary: `✓` when it lets the sound through untouched, a note otherwise, pointing to `svolder host check` for the lines |
| Container | (inside one) the D-Bus socket bind-mounted, an adapter visible, no systemd expected |
| Extended advertising, bluetoothd plugins, WirePlumber (`--dev` only) | fewer than two adapters report extended advertising; bluetoothd running with the `midi` and LE Audio plugins excluded; WirePlumber's BLE MIDI monitor disabled |

Example, on a host with Bluetooth working but no config file yet and no daemon installed:

```
$ svolder doctor
✓ Bluetooth service — org.bluez answers on the system bus
✓ Adapters — present: hci0 (on), hci1 (on)
✓ Runtime — $XDG_RUNTIME_DIR is /run/user/1000; the socket is /run/user/1000/svolder.sock
✗ Daemon — systemd is here and no svolderd units are installed
    fix: svolder daemon install
! Config — there is no config file yet, so no device is configured
    fix: svolder setup
! Config file — there is no config file yet
    fix: create /home/user/.config/svolder/config.toml from the template
```

Exit code: 0 when every check passes, 1 when something warns, 2 when something fails — not affected
by what a fix went on to do. `--json` prints the outcomes as one array of
`{check, state, reason, fix, applies_itself}` instead of lines, `state` one of `"pass"`, `"warn"`,
`"fail"`, `"note"`, and `fix` the fix's command or `null`. On a terminal, without `--json`, the
fixes this verb can apply itself are offered once at the end:

```
Apply the 2 fixes doctor can do itself?
  svolder daemon install
  create /home/user/.config/svolder/config.toml from the template
[y/N]
```

`--yes` applies them without asking, and applies `svolder setup` last of all if it was among them —
every other fix is a command, but setup is a conversation, so it runs the wizard (see "setup" below)
once everything else is done, on a terminal; off one, `--yes` prints `svolder: run \`svolder
setup\` on a terminal to add the first device` instead of trying. `--dev` adds the three checks only
someone running the simulator or `make e2e` needs.

## setup

The guided first run: checks the host, offers to fix what it safely can, scans for a device, and
writes it, its passphrase, the best adapter and the daemon into the configuration — re-runnable at
any time to add another device or revisit a step.

```
The guided first run, safe to run again

Usage: svolder setup [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

On a terminal, `svolder setup` and bare `svolder` with no device configured (see
["Getting started"](getting-started.md#getting-started)) walk the same nine steps as screens of a
TUI wizard — see ["tui"](tui.md#tui)'s ["Setup wizard"](tui.md#setup-wizard) section below for the
keys and the mockups. `svolder add` with no `<UDID>` on a terminal runs the same wizard starting
from its Scan step, with `--name`, `--profile` and `--passphrase` (below) pre-filling its
answers. With a daemon running the scan, the passphrase test and the write are the daemon's walk,
which the wizard only draws: there is no Adapter step, a warning names the devices that hold a
link (the scan may briefly disturb them), `--name` is what an empty name line answers, `--profile`
and `--passphrase` answer their steps (or are written once the device is),
and leaving cancels the walk this run started and no other. A walk already running elsewhere (the
web's *+ Add a device*, say) is refused with where it was started from. A daemon that is too old
to walk is told to `svolder daemon restart`. A `--passphrase` written after the walk (the device
took `auto`) goes into the device's record as given and is never tried on the device, as with
`svolder add UNIT`. The steps:

1. **Health** — the `doctor` checks above and their summary, with `Continue` and
   `Later: open svolder without a device`. Later (or `Esc` on this step) writes nothing and leaves
   the wizard: the TUI opens on an empty Devices screen (`No devices configured - n adds one`),
   where `n` walks again and `q` quits. The plain prompts list Later too, and end with exit code 0
   and `Run svolder again to open the interface.`
2. **Fixes** — the unprivileged fixes as one confirmation, skipped where there are none; root fixes
   are printed under the Health step, never applied here.
3. **Scan** — up to three five-second windows, stopping at the first that hears a device; a rescan
   or skip choice when nothing is heard, with "power the device on and close the phone app, which
   holds its only connection".
4. **Pick** — one device by number, when more than one answered: product, unit id, signal, and the
   profile the advertisement matches (a unit that is already configured is left out, with a line
   naming its device); an unknown product offers the profile list instead.
5. **Name** — defaults to the product name; a name already configured is refused.
6. **Passphrase** — asked only after the first connect is refused: the four digits the phone app
   shows, `auto` to have svolder read the code from the device, or `none` — typed, or left blank —
   for a device with no passphrase. The TUI wizard asks it as the Devices editor's Passphrase row
   (below).
7. **Adapter** — shown only with two or more adapters present; grades each one against the device
   just picked and writes the best into `adapter`.
8. **Daemon** — offers `svolder daemon install` where systemd is present; the NixOS home-manager
   block, "the daemon is the container's own process", or — with no systemd at all — that svolder
   starts a daemon of its own when a command needs one (see ["daemon"](daemon.md#daemon) and
   ["Running in a container"](daemon.md#running-in-a-container)). The device and the adapter are
   already in the file by here; a running daemon is told to re-read it once, on the way to this
   step, or as the walk ends when it ends on the Adapter step with its device written.
9. **Done** — what was written, and the command that opens it.

Off a terminal, `svolder setup` runs the Health and Scan steps and prints the same summary as bare
`svolder` off a terminal (see ["Getting started"](getting-started.md#getting-started)): the health
lines, then every device heard with its ready-to-run `add` line, then what the daemon step would
offer — and reads nothing, prompting for no answer. Example, on a host with no config file, no
daemon installed, and one device in range:

```
$ svolder setup
✓ Bluetooth service — org.bluez answers on the system bus
✓ Adapters — present: hci0 (on), hci1 (on), hci2 (on)
✓ Runtime — $XDG_RUNTIME_DIR is /run/user/1000; the socket is /run/user/1000/svolder.sock
✗ Daemon — systemd is here and no svolderd units are installed
! Config — there is no config file yet, so no device is configured
! Config file — there is no config file yet
    fix: create /home/user/.config/svolder/config.toml from the template

0102030405060708  Gungnir 2  fw 3  passphrase set  -50 dBm
  svolder add 0102030405060708 --name "Gungnir 2"

The daemon holds each device's link, so a command need not connect first.
  svolder daemon install
```

Exit 0 either way, reading nothing off a terminal.

## devices

List configured devices and their connection state, never opening a Bluetooth transport itself. With
a daemon answering on the resolved socket and `--transport` at `auto` or `daemon`, the listing comes
from the daemon and shows each device's product, firmware, and whether it is connected — a device
the daemon is waiting to try again for carries the wait, as in
`gungnir  Gungnir 2  fw 3  disconnected, next attempt in 15 s`, and its JSON object the same wait as
`retry_in_secs`; a device whose configured passphrase does not match the one it advertises carries
the note as `passphrase_note` (see ["Configuration file"](daemon.md#configuration-file)). A link
the device has refused the passphrase on, or readings kept from before it dropped, are said after
the state as in the TUI and the web: `connected · passphrase refused`, `disconnected · readings
stale`.

Each row ends with the device's signal strength: `signal -62 dBm advert, 3 min ago`, or
`signal —` when the daemon has never heard the device advertise with one. The strength is the one
the device's advertisement carried when the daemon last listened for it, so its age says how old
that is; a connected device shows the strength from just before the connect, since BlueZ gives an
unprivileged user none for a link that is up. The JSON object carries it as
`signal: {"dbm": -62, "source": "advert", "heard_at_ms": 1700000000000}` (absent while none has been
heard, and from an older daemon).

A device whose link the device has refused the passphrase on carries `refused: true` in its JSON
object (absent otherwise). Under `--transport ble` or `fake` no daemon is asked; otherwise, with
none answering, it lists the devices named in the configuration file, each reported as `no daemon`;
an empty configuration prints a hint toward `add` instead of an empty list.

```
List the configured devices and their state

Usage: svolder devices [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per device instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, with two devices configured and no daemon running:

```
$ svolder devices
bifrost  1A2B3C4D5E6F7A8B  no daemon
gungnir  0102030405060708  no daemon
```

With no config file, or one with no `[[device]]` block:

```
$ svolder devices
svolder: no devices configured; add one with `svolder add <udid> --name NAME`
```

## add

Add a `[[device]]` entry to the configuration file, creating the file from the template (and its
directory) first if either is absent. A `name` is required, and a duplicate `name` or `udid` is
refused (a unit is configured once, so a second name for a unit another device carries is
refused, naming that device); the file is re-parsed after writing, before this reports success, so a config that would
not load is never left behind. Without a `<UDID>`, on a terminal, `add` runs the same guided walk
`setup` does (see "setup" above), starting from its Scan step, with `--name`, `--profile` and
`--passphrase` pre-filling its answers; off a terminal it prints the hint toward `svolder scan`
instead. While a daemon runs, `add` with a unit id is the daemon's own write (a `device_config` op,
saying `through the daemon`) and never touches the file from this side; a daemon too old to take it
is told to `svolder daemon restart`, and with none running the file is written here as before:

```
$ svolder add --name bifrost
svolder: add needs the unit id of a device. Run `svolder scan` and use the `add` line it prints under the device
```

```
Add a device to the configuration

Usage: svolder add [OPTIONS] [UDID]

Arguments:
  [UDID]  The device's 8-byte unit id, as sixteen hex characters

Options:
  -d, --device <DEVICE>          Device selector: a unit-id prefix or a product name. Optional with one device
      --name <NAME>              The name to select this device by
      --profile <PROFILE>        The profile to use for this device, when it differs from the detected one
      --transport <TRANSPORT>    Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>        Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --passphrase <PASSPHRASE>  `auto`, `none`, or four digits [default: auto]
      --socket <SOCKET>          Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start                 Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                  Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                     Print help
```

Example:

```
$ svolder add 1A2B3C4D5E6F7A8B --name bifrost --profile bifrost3
svolder: added bifrost (1A2B3C4D5E6F7A8B) to /home/user/.config/svolder/config.toml
```

## remove

Take a `[[device]]` entry out of the configuration file. The file is re-parsed after writing, before
this reports success; a `default_device` naming the device just removed is cleared in the same
write, so the file never ends up naming a default that no longer exists. A running daemon is told to
`reload` (see ["IPC protocol"](daemon.md#ipc-protocol)) once the write lands, so a `svolder devices`
or the TUI's Devices screen run right after never shows a device the file no longer names. While a
daemon runs the removal is the daemon's own write (saying `through the daemon`, no reload to send),
and a name it does not know is answered as it is without one.

```
Take a device out of the configuration

Usage: svolder remove [OPTIONS] <NAME>

Arguments:
  <NAME>  The configured name of the device to take out

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder remove bifrost
svolder: removed bifrost from /home/user/.config/svolder/config.toml
```

## hops

List and change the hops of the configuration: what the sound goes through after the DAC, such as a
headphone amplifier, headphones or speakers. Each hop is a `[[hop]]` block of the configuration file
(see ["Hops"](config.md#hops)), either declared by hand with a kind or linked to a configured
device, and the list keeps the file's order, which is the order the sound takes. Like `add` and
`remove`, none of these verbs opens a transport, and a running daemon is told to `reload` once a
write lands. The file is re-parsed after each write, before it is reported.

While a daemon says the file it reads is gone, a write is refused in the daemon's words (`PATH is
missing: the daemon keeps its running configuration; restore the file, then try again`, exit 1): a
write would start a new file from the template, and the reload after it would drop every
device. Restore the file, then run the verb again.

```
List or change what the sound goes through after the DAC

Usage: svolder hops [OPTIONS]
       svolder hops <COMMAND>

Commands:
  list    List the hops in the order the sound takes through them
  add     Declare a piece of equipment the sound goes through
  link    Link a configured device as a hop
  edit    Change a hop; what the flags leave out stays as it is
  remove  Take a hop out of the configuration
  move    Put a hop at another position in the list
  help    Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per hop instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

- `hops` or `hops list [--json]` prints the hops, numbered from 1. `--json` prints one object a line:
  `{"id","kind","name","note","after","device","live"}`. `id` is what every other verb picks the hop
  by: the declared name, or the linked device's. `kind` of a linked hop is `amp` when the device's
  profile is an amplifier and `other` otherwise; `live` is true when the linked device is
  configured.
- `hops add <NAME> --kind <KIND> [--note <TEXT>] [--after <DAC>]` declares a piece of equipment. The
  kind is `amp`, `headphones`, `speakers` or `other`.
- `hops link <DEVICE> [--note <TEXT>] [--after <DAC>]` links a configured device, which the hop is
  then named after.
- `hops edit <ID> [--name <NAME>] [--kind <KIND>] [--note <TEXT> | --no-note] [--after <DAC>]`
  changes the flags it is given and nothing else; `--name` and `--kind` are for a declared hop, and
  `--after ""` takes the `after` away.
- `hops remove <ID>` takes the hop out, and the comments written right above it with it.
- `hops move <ID> <POSITION>` puts the hop at a position counted from 1; past the end puts it last.

A name another hop already carries, compared with case and the blanks round it aside, is refused,
as is an `<ID>` that names no hop. An `--after` that names no configured device is accepted and
warned about whenever the file is read.

Example:

```
$ svolder hops add "HD 650" --kind headphones --note balanced
svolder: added hop "HD 650" in /home/user/.config/svolder/config.toml
$ svolder hops link vali --after bifrost
svolder: linked "Vali" as a hop in /home/user/.config/svolder/config.toml
$ svolder hops
1. HD 650 (headphones): balanced
2. Vali (linked amp) after bifrost
$ svolder hops move Vali 1
svolder: moved hop "Vali" to 1 in /home/user/.config/svolder/config.toml
```

## identify

Ask a device to show itself, so one unit of several can be told from the rest: function `200`, a
visual cue on the unit. The request goes out over the link the daemon (or this process, with
`--transport ble` or `fake`) already holds, sends no refresh, and goes out on a link the device has
refused the passphrase on too, as the official app's identify does. At most one request goes out
every 2 seconds per device, however many front ends ask: a request inside that gap sends nothing
and says so. A device whose link is down is the usual exit 3, with the wait until the daemon tries
it again, as in `device not connected, next attempt in 12 s`. A daemon older than this verb
answers `the daemon cannot identify a device, which this svolder asks it to`, exit 1. A device
whose profile offers no identify (the Mesh Card, where the app offers none) is refused in words,
`NAME offers no identify`, exit 1; `svolder raw 200` can still send the function.

The words say `sent`, never that the unit blinked: nothing here can see the light. What the unit
does (one blink, a timed cue, whether a repeat extends it) has not been seen on a real unit yet.
The TUI's `b` key asks the same for the device on show, or for the row the Devices list is on.

```
Ask the device to show itself, so you can tell which it is

Usage: svolder identify [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder identify -d bifrost
identify sent to bifrost
$ svolder identify -d bifrost
identify just sent to bifrost; wait 2 s
```

## passphrase

Set or clear a device's passphrase, sending function 250 — the only sanctioned path to it (see "raw"
below). On a TTY, without `--yes`, either subcommand asks for confirmation, naming the effect;
anything but `y`/`yes` cancels with exit 1. Without a TTY, `--yes` is required, or the command
refuses outright — so a script cannot change the passphrase by accident. Either way, tell the
official Forkbeard app the same code afterwards; `svolder` does not. A device configured with
`passphrase = "auto"` needs nothing else done to the file: both front ends recover the new code from
the device's next advertisement, and a running daemon picks it up on its next connect attempt.

```
Set or clear the device passphrase

Usage: svolder passphrase [OPTIONS] <COMMAND>

Commands:
  set    Set the device passphrase to four digits (sends function 250)
  clear  Clear the device passphrase (sends function 250)
  help   Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### passphrase set

`<CODE>` is four digits.

```
Set the device passphrase to four digits (sends function 250)

Usage: svolder passphrase set [OPTIONS] <CODE>

Arguments:
  <CODE>  Four digits, or `none` to clear the passphrase

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --yes                    Skip the confirmation prompt
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder passphrase set 4321 --yes
svolder: passphrase set; tell the Forkbeard app the same code
```

### passphrase clear

Sends a raw zero, the protocol's documented clear, through the same call `passphrase set` uses — so
a cleared passphrase behaves exactly as `passphrase set none` above always has.

```
Clear the device passphrase (sends function 250)

Usage: svolder passphrase clear [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --yes                    Skip the confirmation prompt
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder passphrase clear --yes
svolder: passphrase cleared
```

A device configured with `passphrase = "auto"` needs nothing else: both front ends recover the new
code from the device's next advertisement, and a running daemon picks it up on its next connect
attempt. A device configured with literal digits does not — edit `passphrase` in `config.toml` to
the new code by hand, or the next session will encode its frames with the old one.

### Forgot the passphrase

Every setting on a Forkbeard-capable device is saved on the device itself, the passphrase included:
`svolder` and the app both read it back from there, and power-cycling the device resets none of it.

If the device's passphrase is known but `svolder` is refused, `svolder`'s own copy is the one that
is wrong: give it the device's code with `svolder setup`, the Devices screen's `u`, or `passphrase`
in `config.toml` (`svolder passphrase set` changes the device's code, and needs the right one to be
accepted). A changed code that lifts a refusal on a link that is up is followed by a refresh, so
what the refused connect never read is read at once; a code that is still wrong is refused again,
and one handed back unchanged does nothing. If it is not known, the device itself says it: set
`passphrase` to `auto` (the default), and `svolder` encodes its frames with the code the device's
advertisement carries.

Once it is connected, `svolder passphrase set` chooses a code the owner will remember, and the
Forkbeard app has to be told the same one. Under `auto`, `svolder` also corrects an advertisement
older than the device's code by itself, once: the advertisement `svolder` holds can be older than
the code (BlueZ may hand back one from its cache, and a device a central holds does not advertise at
all), so when a device on `auto` refuses on a live link, at the connect's refresh or later on a
write, `svolder` releases the link, listens for up to one scan window (5 seconds) for the device's
next advertisement, encodes with the code that one carries and connects again. Only an advertisement
heard after the release is used to re-key, not one the adapter held from before; when none is heard,
the connect goes on with what svolder already had.

This is the one drop a refusal earns: a fresh advertisement that carries the code that was refused,
or none heard in the window, leaves the connect as it would have been, and a link refused again is
kept as before, nothing written to it and the refusal reported. The same refusal earns no second
drop until it is over: the device shows another code works, the `passphrase` entry changes, or a
connect reads another code than the refused one from the device's advertisement, after which a new
refusal gets its own; a link `svolder` kept that drops by itself and comes back refused under the
same code is the same refusal.

The daemon logs one line for it, and `devices` and `watch` show the link going down and up as they
show any other drop (a daemon reads a refusal at the connect before it shows the link as up, so that
one shows nothing; a session of `svolder`'s own that is watched shows the link back, then down, then
back); the waits between the daemon's connect attempts are unchanged. A verb that opens its own
session (`--transport ble`, or `svolder setup`'s check of a code) does the same once before it
reports the refusal, and a verb that reads or writes the device then exits 3 with the refusal and
its step, through a daemon as through a session of its own, rather than printing nothing; `watch`
and the TUI keep the refused link. A code typed as digits, and `none`, are never dropped over: the
entry is what has to change.

If the device is refused under `auto` after that, power-cycle the device: it then advertises afresh,
`svolder` reads its code, and nothing is reset. Only if the code is lost for good is the last resort
a factory reset: power-cycle the device while holding its only button, if it has one; else the
input-select button; else the physically largest button. On a device with a Mesh Card installed,
that button is behind the pinhole on the card's back — hold it with a paper clip while power-cycling
the device. A factory reset clears every setting on the device, not only the passphrase: a device
ships with no passphrase set; setting one is the owner's own choice, against neighbours taking over
the system.

## scan

List devices in range.

```
List every device in range with its ready-to-run `add` line

Usage: svolder scan [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --timeout <TIMEOUT>      How long to listen for advertisements [default: 5s]
      --json                   Print one JSON object per device instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake scan --timeout 2s
0102030405060708  Gungnir 2  fw 3  passphrase set  -50 dBm
  svolder add 0102030405060708 --name "Gungnir 2"
```

Under each device stands the `add` line that configures it, ready to paste. A product is named from
the codec's table of Schiit product ids, and a product id that table does not know is named after
the `display` of the profile made for it — the simulator's `fake-amp` reads `Fake amp` — here, in
the daemon's answers, and everywhere else a device is named; with neither, the product reads
`unknown` and the `add` line leaves `--name NAME` for you to fill in.

The signal reads whenever the adapter reported it — `svolder doctor`'s Devices row and the setup
wizard's Pick step show the same figure for the same device. `--json` prints one object per device
and nothing else. A daemon listens for at most 30 seconds on a `scan` request: over a daemon, a
longer `--timeout` is cut to 30 seconds before it is asked for, and
`svolder: the daemon scans for at most 30 s` is printed to stderr once.

## inspect

Connect to a device, refresh its state by asking for every reading, and print every function the
device reported — including ids the profile does not name, which print with their raw value and no
label.

```
Connect, refresh, and print everything the device sends

Usage: svolder inspect [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per reading instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake inspect
Status 2 u8 1   Input = 1 (USB 1)
Status 3 bool false   Phase invert = off
Status 4 bool false   NOS filter = off
Status 5 bool false   Mute = off
Status 6 u8 2   Sample rate = 2 (48 kHz)
Status 7.1 bool true   USB 1 connected = true
Status 7.2 bool false   USB 2 connected = false
Status 8 bool false   S/PDIF locked = false
Status 10 u8 63   Volume = 63 (-26.1 dB)
Status 11 u8 64   Balance = 0
Status 12.1.1 u8 64   EQ band 1 Q = 4.30
Status 12.1.2 u8 64   EQ band 1 frequency = 40 Hz
Status 12.1.3 u8 98   EQ band 1 gain = +0.0 dB
Status 12.2.1 u8 64   EQ band 2 Q = 4.30
Status 12.2.2 u8 64   EQ band 2 frequency = 160 Hz
Status 12.2.3 u8 98   EQ band 2 gain = +0.0 dB
Status 12.3.1 u8 64   EQ band 3 Q = 4.30
Status 12.3.2 u8 64   EQ band 3 frequency = 632 Hz
Status 12.3.3 u8 98   EQ band 3 gain = +0.0 dB
Status 12.4.1 u8 64   EQ band 4 Q = 4.30
Status 12.4.2 u8 64   EQ band 4 frequency = 2515 Hz
Status 12.4.3 u8 98   EQ band 4 gain = +0.0 dB
Status 12.5.1 u8 64   EQ band 5 Q = 4.30
Status 12.5.2 u8 64   EQ band 5 frequency = 10000 Hz
Status 12.5.3 u8 98   EQ band 5 gain = +0.0 dB
Status 12.50 u8 1   Tone control mode = flat
Status 12.52 u8 0   Loudness = 0
```

## status

Connect to a device, refresh it, and print the same readings `inspect` prints; both verbs connect
and refresh before printing. What `status` adds is the fault check: it exits 5 when any fault
control reads true.

```
Connect to a device and print every control's current value

Usage: svolder status [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per control instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake status
Status 2 u8 1   Input = 1 (USB 1)
Status 3 bool false   Phase invert = off
Status 4 bool false   NOS filter = off
Status 5 bool false   Mute = off
Status 6 u8 2   Sample rate = 2 (48 kHz)
Status 7.1 bool true   USB 1 connected = true
Status 7.2 bool false   USB 2 connected = false
Status 8 bool false   S/PDIF locked = false
Status 10 u8 63   Volume = 63 (-26.1 dB)
Status 11 u8 64   Balance = 0
Status 12.1.1 u8 64   EQ band 1 Q = 4.30
Status 12.1.2 u8 64   EQ band 1 frequency = 40 Hz
Status 12.1.3 u8 98   EQ band 1 gain = +0.0 dB
Status 12.2.1 u8 64   EQ band 2 Q = 4.30
Status 12.2.2 u8 64   EQ band 2 frequency = 160 Hz
Status 12.2.3 u8 98   EQ band 2 gain = +0.0 dB
Status 12.3.1 u8 64   EQ band 3 Q = 4.30
Status 12.3.2 u8 64   EQ band 3 frequency = 632 Hz
Status 12.3.3 u8 98   EQ band 3 gain = +0.0 dB
Status 12.4.1 u8 64   EQ band 4 Q = 4.30
Status 12.4.2 u8 64   EQ band 4 frequency = 2515 Hz
Status 12.4.3 u8 98   EQ band 4 gain = +0.0 dB
Status 12.5.1 u8 64   EQ band 5 Q = 4.30
Status 12.5.2 u8 64   EQ band 5 frequency = 10000 Hz
Status 12.5.3 u8 98   EQ band 5 gain = +0.0 dB
Status 12.50 u8 1   Tone control mode = flat
Status 12.52 u8 0   Loudness = 0
```

Exit code 5 if any fault control is active:

```
$ SVOLDER_FAKE_FAULTS=1 SVOLDER_FAKE_PROFILE=fake-amp svolder --transport fake status
Status 4 bool false   Mute = off
Status 20.1 u16 0   Meter L = 0
Status 20.2 u16 0   Meter R = 0
Status 21.1 bool false   Temperature reporting = false
Status 21.2 bool true   Power meters = on
Status 230.1 bool true   Over-current = active
Status 230.2 bool true   Over-temperature = active
```

## get

Read one control's current value.

```
Read one control's current value

Usage: svolder get [OPTIONS] <KEY>

Arguments:
  <KEY>  The control to read, by its profile key

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake get volume
-26.1 dB
```

At silence it prints `-inf dB`.

An action, such as a Loki Max's `recall` and `save`, is sent and never read, so `get` refuses it,
exit 1, naming the verb that sends it:

```
$ svolder --transport fake get recall
svolder: recall is an action: it is sent, never read; svolder set recall 1-3 sends it
```

## set

Set one control's value, or recall or save one of the device's own presets. The value can be a
number, a name (for named controls), a unit-suffixed number, `min`, `max`, or `up`/`down` to adjust
by steps. Without `--no-wait`, the command waits for the device's own report of that control
received after the write; when none arrives within 2 seconds it says `sent, not confirmed` with the
device's last report and exits with code 4 (`svolder get KEY` or `svolder status` reads it back).

```
Set one control's value, or recall or save a preset

Usage: svolder set [OPTIONS] <KEY> <VALUE>

Arguments:
  <KEY>    The control to set, by its profile key, or an action: `recall` or `save`
  <VALUE>  A literal value (a number, name, unit-suffixed number, `min`/`max`), or `up`/`down`; for an action, the slot: `2` or `Preset 2`

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --step <STEP>            Raw-value step for `up`/`down` [default: 1]
      --no-wait                Send without waiting for the device to confirm
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --yes                    Answer yes to the question a save asks
      --json                   Print one JSON object instead of text
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake set volume 100
Status 10 u8 100   Volume = 100 (-9.0 dB)
```

A centred level, the balance, takes a whole number counted from its centre, as every other front end
counts it: `set balance 0` centres it, `-5` is five steps left, and a number past either end is
refused (`balance is from -63 to 63, counted from its centre; 64 is outside that`). `get balance`
reads the same way.

A key that names an action of the profile takes a slot for its value, `2` or `Preset 2`, and sends
that action's command. An action has no echo and no reading, so nothing is waited for and nothing is
printed on standard output: `--no-wait` means nothing to it, and standard error says what went out.
With `--json` one object says the same on standard output. A slot the device does not have is
refused, exit 1, before anything is sent. The Loki Max has two, `recall` and `save`, each with
slots 1 to 3:

```
$ svolder --transport fake set recall 2
svolder: recalled preset 2
$ svolder --transport fake set recall 2 --json
{"key":"recall","slot":2,"sent":true}
$ svolder --transport fake set recall 4
svolder: recall takes a slot from 1 to 3
```

A recall sets the bands to what the device saved in the slot, which can be up to 12 dB above 0 on a
band, and sends at once as the app does. A save writes the current bands over the slot and cannot
be undone, so it asks first: on a terminal, `Save the bands to preset 2 on Loki Max? [y/N]`, where
a no, or an Enter, exits 0 with `svolder: nothing changed`. Off a terminal it is refused, exit 1,
before anything is sent, unless `--yes` answers for you:

```
$ svolder --transport fake set save 2 </dev/null
svolder: saving to preset 2 overwrites what Loki Max holds there; add --yes to save without asking
$ svolder --transport fake set save 2 --yes
svolder: saved the bands to preset 2
```

With `--json` that refusal is also one object on standard output,
`{"key":"save","slot":2,"sent":false,"error":"…"}` with the same words, and the exit is 1.

`svolder raw 7.1.2 data ""` sends the same save with no question: `raw` is outside every rule.

A key the device's profile does not have is refused with the keys it does have, and, when one is
near, the key that was probably meant: a key an app release renamed (`standby` is now `power`,
`preout_mute` is now `preout`) names its successor, and any other key within two edits of one, when
two edits are fewer than half the typed key's length (a short key such as `ns` for `nos` gets no
hint), names that one. Exit 1:

```
$ svolder --transport fake set volme 5
svolder: no control named volme; this device has: phase, mute, nos, sample_rate, volume, balance, input, usb1_connected, usb2_connected, spdif_locked; did you mean volume? See `svolder --help`
```

## preset

List or apply the device's own presets: what the device keeps itself, apart from the tone-control
presets of [`eq`](eq.md). A Loki Max has three slots, which `svolder set recall N` brings back and
`svolder set save N` writes, and five built-in settings, which `preset apply` puts on its bands.
A device with a tone control but no presets of its own is told to use `eq list` instead, and the
`eq` verbs on a device with presets and no tone control point here.

```
List or apply the device's own presets

Usage: svolder preset [OPTIONS] <COMMAND>

Commands:
  list   List the device's presets: its slots, then its built-in settings
  apply  Put the device on one of its built-in settings
  help   Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### preset list

List the device's presets, one line or JSON object each: the slots, marked `slot`, then the
built-in settings, marked `built-in`. The profile is the one the configured device names; with a
daemon that holds the device, it is the daemon's, and the built-in setting the bands read is marked
`on`. A slot is never marked: the device does not say which it holds. A text listing ends with the
two verbs that use it. With `--json`, a row is `{"name", "slot", "built_in", "on"}`: `slot` is a
number for a slot and `null` for a setting, and `on` is `null` for a slot and for every row when no
daemon gave the readings.

```
List the device's presets: its slots, then its built-in settings

Usage: svolder preset list [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per preset instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder preset list
Preset 1      slot
Preset 2      slot
Preset 3      slot
Flat          built-in
Bass Boost    built-in
Treble Boost  built-in
Smiley        built-in
Loudness      built-in
svolder set recall N recalls a slot; svolder preset apply NAME applies a built-in
```

A device with a tone control and no presets of its own exits 1:

```
svolder: Gungnir 2 has no device presets; svolder eq list lists its tone-control presets
```

### preset apply

Put the device on a built-in setting, found by name without regard to case or the blanks round it.
The bands the setting changes are written as a batch in an order that never sounds a boost first:
every band going down, then every band going up. A band already on its step is left out, and each
write waits for the device's echo. Nothing asks first: `preset apply` is outside the rise rule, as
`set` is. A failure part-way says how far it got, as `eq load` does, and exits with the failure's
code. A name the device does not list is refused with the names it does:

```
Put the device on one of its built-in settings

Usage: svolder preset apply [OPTIONS] <NAME>

Arguments:
  <NAME>  The setting's name, as `preset list` shows it

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder preset apply "bass boost"
svolder: Bass Boost on (3 writes)
$ svolder preset apply Nope
svolder: no built-in setting named Nope; the settings are: Flat, Bass Boost, Treble Boost, Smiley, Loudness
```

A device already on the setting sends nothing and says `svolder: Flat is already on`.

## scene

Save, recall, list, rename and delete scenes: the sound settings of the whole config under a name.
A scene holds the computer's EQ, loudness, crossfeed and virtual speakers and, for each device that
was connected when it was saved, its tone control and plain sound settings (input, phase, NOS,
balance and the like). No volume, gain, mute or power is saved or recalled. The daemon keeps scenes
([the design](../design/scenes.md)), so every verb but `list` and `show` needs a running daemon.
`svolder scene` with no verb is `scene list`.

```
List, save, load, rename or delete scenes of sound settings

Usage: svolder scene [OPTIONS] [COMMAND]

Commands:
  list    List the scenes and which one is on (what no verb does)
  show    Show one scene: which parts it holds and which one is on
  save    Save the sound settings as they are now as a scene: the computer's and every connected device's. No volume, gain, mute or power is saved
  load    Recall a scene onto the computer and every connected device that has a part in it
  rename  Rename a scene, leaving what it holds as it is
  delete  Delete a scene
  help    Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

An old daemon that does not know scenes exits 7 with a line that says so (no request is sent to it).

### scene list

List the scenes, one line each: a `*` marks the one that is on, and the parts follow, `computer` and
each device by its configured name, with `(not connected)` after a device that is away. With
`--json` the answer is one object, `{"scenes":[{"name","parts":[{"who","present"}]}],"on","recalled",
"known"}`: `on` is absent while no scene is on or while that is not known yet, `recalled` is the scene
last recalled, and `known` is false while a device is still reading a setting that could change
which scene is on.

```
List the scenes and which one is on (what no verb does)

Usage: svolder scene list [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, over the fake device:

```
$ svolder scene list
* Late night  computer · Desk DAC
  Desk  computer · Desk DAC
```

With no daemon running, `list` and `show` read the config instead, and say so on stderr: the parts
are named as the daemon names them, no scene is marked on, no `(not connected)` is said (nothing is
connected), and `--json` gives `{"scenes":[{"name","parts":[{"who"}]}],"known":false}`:

```
$ svolder scene list
Late night  computer · Desk DAC
svolder: read from the config, with no daemon running: which scene is on is not known
```

### scene show

Show one scene by name, without regard to case: the parts it holds, and `(on)` after the name while
it is the scene that is on. A line says so while which scene is on is not known yet. With `--json`
the answer is the scene's row plus `on` and `known` (the bare row, with neither, when read from the
config with no daemon), which is also how one scene is read out for a script.

```
Show one scene: which parts it holds and which one is on

Usage: svolder scene show [OPTIONS] <NAME>

Arguments:
  <NAME>  The scene's name, in any case

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder scene show "late night"
Late night (on)
  computer
  Desk DAC
```

### scene save

Save the sound settings as they are now, the computer's and those of every connected device. A name
the config already has is refused (exit 1, `a scene named "Desk" is already configured`) unless
`--force`, which saves over that scene. The names follow the preset rule: trimmed, at most 40
characters, compared without regard to case, and `custom` is reserved. A device that is configured
but not connected is not in the scene, and the answer names it as a note
(`Hall is not connected: it is not in the scene`). The daemon refuses a save in words while a
device is still reading its settings or applying a change.

```
Save the sound settings as they are now as a scene: the computer's and every connected device's. No volume, gain, mute or power is saved

Usage: svolder scene save [OPTIONS] <NAME>

Arguments:
  <NAME>  The name to save it under

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --force                  Save over a scene of that name
      --json                   Print the answer as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder scene save "Late night"
Saved "Late night".
$ svolder scene save "Late night"
svolder: config: a scene named "Late night" is already configured. See `svolder --help`
$ svolder scene save "Late night" --force
Saved "Late night".
```

### scene load

Recall a scene by name: the computer's part and each connected device's, at once, each in its own
safe order. A recall prints one line per part, `who: status`, with the daemon's notes under it, and
the statuses are `done` (every setting was set), `partial` (some were not: the notes name them),
`left` (a device that is not connected keeps what it has, or a part was refused before anything
changed) and `failed` (it started and did not finish). Nothing is rolled back. The exit code is 0
when every part is `done` or `left`, and 3 when any part is `partial` or `failed`.

A scene that would play one preset on a device and on the computer at once asks first. At a terminal
the question is printed and answered `[y/N]`; `--yes` answers it and prints the question as a note;
with no terminal and no `--yes` the recall is refused with the question and `Pass --yes`, and a no
says `nothing changed` and exits 0. Yes recalls the scene as it was saved.

```
Recall a scene onto the computer and every connected device that has a part in it

Usage: svolder scene load [OPTIONS] <NAME>

Arguments:
  <NAME>  The scene's name, in any case

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --yes                    Answer yes to the question asked when the scene plays a preset on a device and on the computer at once
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder scene load "late night"
Recalled "Late night".
computer: done
Desk DAC: done
$ svolder scene load Nope
svolder: no scene named "Nope". See `svolder --help`
```

### scene rename

Rename a scene, leaving what it holds as it is. The new name follows the same rule as a save's, and
a name another scene has is refused. If the scene is the one last recalled, the daemon's memory of it
follows the new name.

```
Rename a scene, leaving what it holds as it is

Usage: svolder scene rename [OPTIONS] <OLD> <NEW>

Arguments:
  <OLD>  Its name now
  <NEW>  The name it takes

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder scene rename Desk Work
Renamed "Desk" to "Work".
```

### scene delete

Delete a scene. The name is looked up first, then at a terminal it asks `Delete scene "Movie"? [y/N]`
and a no exits 1. `--yes` skips the question; with no terminal and no `--yes` it refuses (exit 1,
`refusing to delete a scene without confirmation; pass --yes`).

```
Delete a scene

Usage: svolder scene delete [OPTIONS] <NAME>

Arguments:
  <NAME>  The scene's name

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the answer as one JSON object instead of text
      --yes                    Skip the confirmation prompt
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder scene delete Work
svolder: refusing to delete a scene without confirmation; pass --yes
$ svolder scene delete Work --yes
Deleted "Work".
```

The exit codes of the scene verbs are the device verbs' ([exit-codes.md](exit-codes.md)): 1 for a
refusal (a taken or missing name, a bad name, no confirmation), 3 for a daemon that is not running
(``scenes are kept by the daemon, and none is running. Start it with `svolder daemon install`, then
try again``) and for a recall with a `partial` or `failed` part, 7 for a daemon without scenes.

## watch

Connect to a device, print the refresh snapshot, then print every status the device reports as it
arrives.

```
Connect to a device and print every status it reports until Ctrl-C

Usage: svolder watch [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per reading instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Ctrl-C ends the command with exit 0, and is answered at once, even while the snapshot reprinted
after a reconnect is still being read. A direct session's release that takes over half a second
first prints `svolder: releasing NAME…` on stderr, once. A Ctrl-C during the release (a second one
when Ctrl-C ended the loop) stops waiting for it:
`svolder: not waiting for the release; the device may stay held for a moment` on stderr, and exit
130, whatever the command would otherwise have exited with.

A dropped link prints `svolder: device disconnected` on stderr, and a restored one prints
`svolder: reconnected` followed by a fresh snapshot, so a value that changed while the device was
away is visible. Both markers appear on either path: through the daemon they are the events it
publishes, and on a direct session the first is printed as soon as the link drops, with the
session's own reconnect running between the two.

A device that does not come back exits 3, naming what happened and then the one step that follows
(see ["Exit Codes"](exit-codes.md#exit-codes)). On a direct session, that is the connect deadline
(20s) running out:

```
svolder: connection failed: device did not reconnect within 20s. If it does not come back, run `svolder doctor`
```

and through the daemon, a daemon that stopped is said so:

```
svolder: The svolder daemon stopped. Start it again with `svolder daemon restart`
```

(a daemon that still answers but stopped sending prints `connection failed: the daemon stopped
sending events` with the same `svolder doctor` step).

Example (interrupted with Ctrl-C; here shown with `timeout` to end the demonstration):

```
$ timeout 2 svolder --transport fake watch
Status 2 u8 1   Input = 1 (USB 1)
Status 3 bool false   Phase invert = off
Status 4 bool false   NOS filter = off
Status 5 bool false   Mute = off
Status 6 u8 2   Sample rate = 2 (48 kHz)
Status 7.1 bool true   USB 1 connected = true
Status 7.2 bool false   USB 2 connected = false
Status 8 bool false   S/PDIF locked = false
Status 10 u8 63   Volume = 63 (-26.1 dB)
Status 11 u8 64   Balance = 0
Status 12.1.1 u8 64   EQ band 1 Q = 4.30
Status 12.1.2 u8 64   EQ band 1 frequency = 40 Hz
Status 12.1.3 u8 98   EQ band 1 gain = +0.0 dB
Status 12.2.1 u8 64   EQ band 2 Q = 4.30
Status 12.2.2 u8 64   EQ band 2 frequency = 160 Hz
Status 12.2.3 u8 98   EQ band 2 gain = +0.0 dB
Status 12.3.1 u8 64   EQ band 3 Q = 4.30
Status 12.3.2 u8 64   EQ band 3 frequency = 632 Hz
Status 12.3.3 u8 98   EQ band 3 gain = +0.0 dB
Status 12.4.1 u8 64   EQ band 4 Q = 4.30
Status 12.4.2 u8 64   EQ band 4 frequency = 2515 Hz
Status 12.4.3 u8 98   EQ band 4 gain = +0.0 dB
Status 12.5.1 u8 64   EQ band 5 Q = 4.30
Status 12.5.2 u8 64   EQ band 5 frequency = 10000 Hz
Status 12.5.3 u8 98   EQ band 5 gain = +0.0 dB
Status 12.50 u8 1   Tone control mode = flat
Status 12.52 u8 0   Loudness = 0
```

The command then streams new statuses from the device until Ctrl-C.

## `--no-start`

A global flag (and `SVOLDER_NO_START`, read as `1`, `true`, `0` or `false`) for a caller that must
never start anything: the verb uses a daemon that already answers on the socket, and starts no
socket unit, no daemon of its own and no direct BlueZ session, whatever `--transport` says. With
none answering it exits 3 (`no daemon answers on SOCKET`, with the step
`Start it with svolder daemon restart`). `panel` and `host eq` never start a daemon;
`--no-start` gives `set`, `eq load`, `preset apply` and `scene load` the same guarantee, as every
verb that talks to a device or a daemon has it. The verbs that cannot keep to it say so: `capture`
talks to the device directly and refuses, `doctor` leaves out the rows that look at devices when no
daemon answers, and the guided wizard (`svolder setup`, `svolder add` on a terminal and a bare
`svolder`) starts the daemon and opens BlueZ itself, so it is exempt. The GNOME indicator runs
every click with it.

## panel

Print the model the GNOME top-bar indicator draws, one JSON object per line. It is the one source
the indicator reads: the device and its link, the volume, the input, the DAC's presets, the signal
bars, the computer EQ, the Path strip (the same judge as the `Path` row of the TUI and the web
page), the scenes, and the arguments of every action the bar and the popup run. Nothing in it is
for a person to read; there is no text form, so without `--json` the verb is a usage error (exit 1).

```
Print what the GNOME top-bar indicator shows, one JSON line per change

Usage: svolder panel [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per line (the only form: the verb is for programs)
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

The first line comes as soon as the snapshot is whole (the device's readings, the daemon's device
list, the host view and the scenes; a daemon that sends no host view or scenes gets its first line
after two seconds), then one line per change: at most four a second, and never the same line twice
in a row. The daemon does not push the signal strength, so the device list is read again every 15
seconds, and a line is printed when a signal reading turns old (older than 120 s: while a BLE link
is up BlueZ reports no strength, so the reading is the last advertisement heard before connecting).

The verb talks to a running daemon and nothing else: it looks at the socket, starts no daemon and
opens no BLE session, whatever `--transport` says. With no daemon answering it exits 3 (`no daemon
answers on SOCKET`); a daemon that stops ends the stream with exit 3 and `The svolder daemon
stopped.`; several devices, no `-d` and no `default_device` exit 2 with the candidate list; a daemon
too old to know a request exits 7 (see ["Exit Codes"](exit-codes.md#exit-codes)). A configured
device that is away, not found yet, or no device at all is not a failure: the stream goes on with
`present: false` or `device: null`, and the computer's EQ and the scenes stay usable. A stream that
began with `device: null` ends with exit 0 and no message once a device appears: start it again.

The `run` vectors put `--` before every name, so a scene or preset whose name starts with `-` can
be selected. A caller that must not start anything runs them with the global `--no-start` (see
"`--no-start`" above).

Every line is one object with these members (each `null` when there is nothing to say):

- `v`: the shape's version, 1.
- `device`: `name`, `present` (the link is up), `rate_hz` and `rate_text` (`44.1`), `volume`
  (`db`, `min_db`, `max_db`, `muted`, and the strings `text` `−24.1` and `bar_text` `−24`), `input`
  and `preset` (`on` and `choices`), and `signal` (`dbm`, `dbm_text`, `heard_at_ms`, `bars` 1 to 4,
  `weak`, `old`). The DAC preset list is the TUI's: the tone control's presets (loaded with
  `eq load`) on a device that has a tone control, else the device's own slots (`set recall N`) and
  built-in settings (`preset apply NAME`).
- `computer_eq`: `on`, `preset` and `choices`; `null` while the computer's controls are unavailable.
- `path`: `end_to_end` (`untouched`, `changed`, `not_this_dac`, `other_input` or `unknown`), `tone`
  (`good`, `warn` or `plain`: how the Path row's label is drawn, which is what the indicator's LED
  follows: a failing PipeWire config check is `warn` while `end_to_end` still reads `untouched`),
  `label` (the words for a screen reader), `segments` (the player, the computer and the DAC, each
  `good`, `warn` or `unknown`) and `off`, the words for what is off grouped by hop (`DAC: volume
  −24.1 dB · tone control`; empty when untouched end to end; `config N` is added for a failing
  PipeWire config check; for a sound that goes to another sink or input there are no hops, and the
  row's words stand here, such as `→ Monitor HDMI`). The path is judged whenever the computer's
  route is read, with or without a player playing (the TUI says "nothing playing" and the web page
  shows no path then; the indicator keeps its strip and the DAC's own changes in view).
- `scenes`: `on` and `choices`.
- `actions`: the arguments, after the program name and the global flags, of `volume_up` and
  `volume_down` (a caller adds `--step N` for N notches), `mute_on`, `mute_off`, `volume_set` (the
  literal `{db}` in `{db}dB` stands for the number), `eq_on` and `eq_off`. Every choice carries its
  own `run`, such as `["scene", "load", "--", "Desk"]`.

Every number a person reads in a string has the proper minus sign (U+2212); the JSON numbers stay
numbers.

## host

Correct what the computer plays, before it reaches the DAC. `host eq` puts a parametric equalizer, a
PipeWire filter chain, in front of the sink that feeds the DAC, so music from any program is
corrected on the way out; it is the computer-side twin of the DAC's own tone control, and the same
AutoEq corrections go on either. It needs PipeWire and WirePlumber (`wpctl`), and says so when
either is missing.

```
Correct what the computer plays, before it reaches the DAC

Usage: svolder host [OPTIONS] <COMMAND>

Commands:
  eq         An equalizer on what the computer plays
  check      Check what the computer and its PipeWire config do to the sound
  crossfeed  Cross a little of each channel into the other for headphones, through the daemon
  loudness   Give the bass and the treble back at a low volume, through the daemon
  speakers   Two virtual speakers on headphones, through the daemon
  help       Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host eq

The group's first subcommand, with `apply`, `remove`, `on`, `off`, `preset` and `status` below
it. (`host crossfeed`,
`host loudness` and `host speakers`, the group's other ones, are described in their own sections.)

```
An equalizer on what the computer plays

Usage: svolder host eq [OPTIONS] <COMMAND>

Commands:
  apply   Put a correction in front of the sink that feeds the DAC and keep it there
  remove  Stop the computer EQ and delete what was written for it
  on      Turn the computer EQ on, with the points it has, through the daemon
  off     Turn the computer EQ off and keep its points, through the daemon
  preset  Put a preset on the computer, through the daemon
  status  Print what is applied and whether it runs
  help    Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host eq apply

Put a correction in front of a sink and keep it there. The argument is a file when it names an
existing file, otherwise a preset's name (matched as `eq load` matches it). What goes on the
computer:

- an imported preset: its filters and preamp;
- a band preset: its bands as peaking filters, with a preamp of minus the curve's peak rounded up to
  0.1 dB, so the correction never clips;
- a file (an Equalizer APO parametric EQ file): its filters and preamp; nothing is saved, and the
  name shown is the file's name, as `eq import` names it.

The computer EQ never goes above unity: whatever the preamp states (a file or a preset may state
anything from −30 to +30 dB), the one applied is the lower of it and minus the correction's peak
between 20 Hz and 20 kHz, rounded up to 0.1 dB. When that lowers it, the verb says so on stderr,
`preamp lowered from -2.0 dB to -6.4 dB so the correction never clips`. A band preset the DAC keeps
flat or in loudness mode has no curve to put on the computer and is refused, with the reason.

A built-in preset goes as its bands, like a band preset. This command has no device to ask, so it
shapes the built-ins on the five-band tone control of the Gungnir 2 (their frequencies are fixed). A
preset of more than 16 filters is refused (`17 filters; the computer EQ holds 16`), never cut. The
sink is `--sink`, a `node.name` or a
wpctl id (read through `wpctl inspect`), else the default output at the moment of the apply.
Applying while a computer EQ is on replaces it; the correction is checked first (every filter's
frequency, gain and Q, `filter 2: Q 0 is outside 0 (excluded) to 20`), and a request that cannot be
served leaves the chain that is on running.

So does one whose files cannot be written or deleted (a full disk, a directory that cannot be
written): every directory the apply writes in or deletes from — the config's, the unit's or the
autostart entry's, the runtime directory the pid goes in, the state file's, and on a switch those of
the manager it leaves — is tried before anything of the chain that is on is touched, and the new
files are written beside their places and put in place only once they all are.

```
Put a correction in front of the sink that feeds the DAC and keep it there

Usage: svolder host eq apply [OPTIONS] <TARGET>

Arguments:
  <TARGET>  A preset's name, or an Equalizer APO parametric EQ file

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --sink <NODE>            The sink to put it in front of, by node name or wpctl id; the default output when left out
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --yes                    Answer yes to the question asked when the preset is on the DAC
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

With a running daemon, the devices it serves are checked: where the preset is on one of them (the
same preset name, and the device reads the curve), the question
`NAME is on the DAC. Move it to the computer? [y/N]` is asked, and a yes flattens that device (as
`eq flatten` does) before the correction goes on the computer. A no or an Enter changes nothing and
exits 0 with `nothing changed`. Questions need a terminal: without one the verb refuses and names
`--yes`, which answers yes. With no daemon running nothing is asked and no device is reached. The
device is flattened first, so the correction is never on both; when the apply then fails, the
failure says the DAC was flattened and that `svolder eq load NAME` puts the curve back.

```
$ svolder host eq apply "Sennheiser HD 650"
svolder: host EQ: Sennheiser HD 650 · on · in front of alsa_output.usb-Example_DAC-00.analog-stereo · systemd
```

How the chain is put in front of the sink depends on WirePlumber:

- **Smart route** (WirePlumber 0.5 and newer, read from `wireplumber --version`): the chain is a
  WirePlumber smart filter targeting the sink. Every stream is linked to the filter and the filter
  to the sink, again after the sink goes away and comes back; while the DAC is gone, streams play to
  another output without the correction, and return through the filter when the DAC does. The
  default output is not changed.
- **Default route** (older WirePlumber): the chain is a sink of its own, `svolder_eq`, that plays
  into the target; apply makes it the default output and remembers the previous default, and
  `remove` restores that default when it still exists. With the DAC unplugged the chain stays the
  default and its output goes to the next sink, still corrected: that is this route's limit.

What keeps the chain running is the **manager**, found by asking the systemd user manager:

- **systemd**, when the user manager answers: a unit `svolder-host-eq.service` in the unit directory
  `daemon install` uses (`SVOLDER_UNITDIR`, else `~/.config/systemd/user`), modelled on PipeWire's
  `filter-chain.service`, started with `daemon-reload`, `enable` and `restart`;
- **process** otherwise: a detached `pipewire -c <config>` in a session of its own, its pid kept in
  the private runtime directory, and an autostart entry
  `~/.config/autostart/svolder-host-eq.desktop` that starts it at login. The chain is recognised by
  its command line alone, exactly `pipewire -c <config>` among your own processes, so the one the
  autostart entry starts at login (when no pid file is left) is seen by `status`, replaced by
  `apply` and stopped by `remove`; a process is signalled only when its command line is that, so a
  pid that is gone, or that another program has since been given, is never signalled.

`SVOLDER_HOST_EQ_MANAGER=systemd|process|child` forces one. An unrecognised value of
`SVOLDER_HOST_EQ_MANAGER` is refused, naming the three. In a container the chain is a `child`: a
`pipewire` started by the program that applied it, which ends with that program and writes no unit,
autostart entry or pid file. So `svolder host eq apply` and `remove` refuse there (`in a container
the computer EQ runs with the daemon`, `start svolderd, then try again`) and write nothing; the TUI
keeps the chain it starts until it quits. The computer EQ refuses with `SVOLDER_HOST=off`. The
`pipewire` program is `SVOLDER_PIPEWIRE`, else `pipewire` on `PATH`; the WirePlumber version is read
from `SVOLDER_WIREPLUMBER`, else `wireplumber` on `PATH`, as `wireplumber --version`.

svolder writes these files for the computer EQ and nothing else: `~/.config/svolder/host-eq.conf`
(the complete PipeWire config, run as `pipewire -c`), the unit or the autostart entry, the pid file
of a process, `~/.local/state/svolder/host-eq.toml`, and `~/.local/state/svolder/host-eq-settings.toml`
(only the crossfeed, speakers and loudness settings changed before any chain was applied; the
controller writes and deletes it). `host eq remove` deletes what apply wrote and leaves that file.
PipeWire's own configuration directories, `filter-chain.service` and every unit but svolder's own
are never touched.

### host eq remove

Stop the computer EQ and delete what apply wrote (the config, the unit or the autostart entry and
the pid file); the state file stays, with `on = false`, so the last correction can be applied again.
On the default route the default output goes back to what it was. A chain that is already gone,
after a reboot or a kill, is removed all the same, and a remove with nothing applied does nothing.
`svolder: host EQ removed` says it was on, `svolder: host EQ was not on` that it was not.

When crossfeed plays, a `remove` that finds the daemon running only turns the EQ off and says:
`svolder: host EQ off; the chain stays while crossfeed plays`. Without a daemon, `remove` also
turns the saved crossfeed switch off; the model and numbers are kept.

Applying or removing the computer EQ adds or takes away the node what is playing goes through, so
WirePlumber links each stream again: a player can stall for a moment, and a browser can pause as it
does when headphones are unplugged — press play again. svolder sends nothing to the player.

```
Stop the computer EQ and delete what was written for it

Usage: svolder host eq remove [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host eq status

Print what is applied and whether it runs: the unit is active, or the process alive.

```
Print what is applied and whether it runs

Usage: svolder host eq status [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

```
$ svolder host eq status
host EQ: Sennheiser HD 650 · on · in front of alsa_output.usb-Example_DAC-00.analog-stereo · systemd
$ svolder host eq remove
svolder: host EQ removed
$ svolder host eq status
host EQ: off (last: Sennheiser HD 650)
```

While the chain runs for crossfeed alone (the EQ is off), `status` says so:
`host EQ: off · the chain is running for crossfeed alone · in front of SINK · MANAGER`. While both
are off and the chain is kept as a flat pass-through for a stream still playing, `status` says the
chain is kept as a pass-through until idle, and `remove` leaves it, saying so.

A state file that says on while nothing runs reads `stopped` and names the fix on a second line,
`the unit is not running: systemctl --user status svolder-host-eq` (for a process,
`the process is not running: apply it again with svolder host eq apply`); one that says off while
the chain runs reads `the unit is still running: svolder host eq remove`. `--json` prints one object
with `on`, `running` (`null` when it could not be told), `name`, `origin`, `target`, `route`,
`manager`, `previous_default`, `preamp_db`, `filters` (each `type`, `f`, `g`, `q`) and `problem`;
with nothing ever applied it prints `{"on":false,"running":null}`; a state file that cannot be read
says so, `host EQ: unknown: …`, instead of reading as off.

### host eq on

Turn the computer EQ on with the points it has, through the daemon (the daemon keeps the chain, so
there is no one-shot form: with no daemon answering it exits 3, and with a daemon whose host audio
is off, 6). `svolder: host EQ on` says it was done; `--json` prints instead
`{"on":true,"preset":"NAME or null","ok":true}`, read back from the daemon after the change
(`on` and `preset` are `null` when that read failed; `ok` says the change itself was made). The
same object follows `host eq off` and `host eq preset`.

```
Turn the computer EQ on, with the points it has, through the daemon

Usage: svolder host eq on [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the computer EQ's state as one JSON object
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host eq off

Turn the computer EQ off and keep its points, through the daemon, as the `off` switch of the TUI
does (`host eq remove` also deletes what apply wrote). Same exits and `--json` as `host eq on`.

### host eq preset

Put a preset on the computer, through the daemon: a saved one or a built-in, found as `eq load`
finds one. A preset that a connected device plays too would play twice: the daemon asks first, and
`--yes` answers it (that device is flattened). With no terminal and no `--yes` the question is
refused with exit 1 and the flag that answers it. `svolder: host EQ: NAME · on` says it was done;
`--json` prints the state as `host eq on` does.

```
Put a preset on the computer, through the daemon

Usage: svolder host eq preset [OPTIONS] <NAME>

Arguments:
  <NAME>  The preset's name: a saved one, or a built-in

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --yes                    Answer yes to the question asked when a device plays the preset too
      --json                   Print the computer EQ's state as one JSON object
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host check

Say whether the computer lets a player's sound through to the DAC untouched, and why not when it
does not. It prints the path from the player through the computer to the sink, with the verdict
(`untouched`, or what changes the sound), then the **PipeWire config** check: seven conditions of
the PipeWire config (rates, forced rate, dither, 24-bit output, no plug, stereo to stereo, no
filter in the sink), each `pass`, `fail` or `unknown`, and what they add up to. It only reads, and
the only thing it runs is `pw-dump`.

```
$ svolder host check --player "Fixture Player"
player → computer → bits S16LE triangular dither: resampled 48 → 96 kHz (rate forced) · dither on (triangular)
  player: 48 kHz S16LE, volume 100 %
  computer: 48 kHz S16LE, volume 100 %
PipeWire config changes the sound: 3
  rates                  pass     48 kHz allowed
  forced rate            fail     96 kHz forced: every other rate is resampled
  dither                 fail     triangular
  24-bit output          fail     S16LE in use
  no plug                pass     not an ALSA sink
  stereo to stereo       pass     FL FR
  no filter in the sink  pass     none
  svolder judges the PipeWire config from the player’s stream on. It cannot see inside a
  player, nor past the sink’s converter (ALSA hw, USB, the DAC), and it judges at most 24
  bits. It only reads: it never changes a setting.
```

The output above is from a test graph (the default sink is a pipe sink that dithers, with the
clock forced to 96 kHz, and the player plays into it), so the seven lines judge the sink the first
line names. When the path has no hops (another sink, another input) a `config judged on:` line
names it. A failing line is not an error: the command exits 0 and the summary says `PipeWire
config changes the sound: 3`. A line that could not be read says `unknown`, never `pass`
(`PipeWire config not fully read: 1`, or `PipeWire config not read` when there is no sink to
judge). The limit sentence closes the text.
The check is of what enters the system's audio pipeline from the player and every hop after it. It
cannot see inside the player (its own normalization, ReplayGain, volume or EQ), whether the music
is a local file or a stream, what ALSA does past the sink's converter, the USB driver or the DAC,
and it judges at most 24 bits.

With a daemon answering, `check` uses the daemon's route (the player it follows) and its DACs'
readings: when the sink is one of your DACs the verdict is the DAC's, with its input and rate; for
any other sink it is the computer's side alone (`path.dac` is null, and the path may read
`untouched` up to that sink: a Bluetooth or virtual sink changes the sound past it, which the
check cannot see). The TUI, the web page and Home Assistant judge against your DACs only, and
read `not_this_dac` for a sink that is none of them. With no daemon the player is not known, so the
verdict reads `unknown` until `--player NAME` names it (an application name or a process id), which
also makes the command read the graph itself instead of using the daemon's route. When it reads the
graph itself (no daemon route, or `--player`), `SVOLDER_HOST=off` or `fake` gives exit 6 with "host
audio is off for this process", and a graph `pw-dump` cannot print gives exit 6 too ([exit
codes](exit-codes.md)); with a daemon's route printed the exit is 0.

`--json` prints one object:

```
{"sink":"Bifrost 3",
 "path":{"verdict":"untouched","end_to_end":"untouched","words":["untouched"],"sentence":"...",
         "dac":"Bifrost 3"},
 "config":{"sink":"Bifrost 3","outcome":"untouched",
           "summary":"PipeWire config lets it through untouched",
           "limit":"...","lines":[{"key":"rates","outcome":"pass","words":"..."}, ...]}}
```

The keys are the same on every JSON surface of svolder but Home Assistant, which has no
`end_to_end` in 0.38.1 (the web card, MCP; the top-level `sink` is kept for scripts, and `path.dac`
is null when only the computer's side was judged, see above): the path `verdict` is `untouched`,
`changed`, `not_this_dac`, `other_input` or `unknown`, and judges the path up to the DAC's input,
as it always did; `end_to_end` takes the same five words and adds the DAC's own stage to the
verdict, so a path that is `untouched` up to a DAC at -3 dB or with its tone control on reads
`changed` end to end, and one that could not be read but has a known change at the DAC reads
`changed` too; it is what the Path row's label is coloured from; `words` is a list; the config `outcome` is `untouched`, `changes`, `unread`, `not_read`
or `other_version`; the lines' `key` is `rates`, `forced_rate`, `dither`, `format`, `plug`, `stereo`
or `filter`, and each line's `outcome` is `pass`, `fail` or `unknown`. `limit` is the sentence of
what the check cannot judge. `svolder doctor` shows the same check as a `PipeWire config` row, only
while a daemon answers with a route: a `✓` when the config lets the sound through, a note (`·`)
otherwise, never a warning.

### host crossfeed

Cross a little of each channel into the other for headphones: the computer's
[crossfeed](eq.md#crossfeed) (what it does, the four models, the ranges). With no verb it prints
the state; a verb changes it. Every verb goes through `svolderd`, which owns the filter chain and
every rule that keeps a crossfeed from being left on, so the CLI never starts one by itself
(`--transport` does not change that).

```
Cross a little of each channel into the other for headphones, through the daemon

Usage: svolder host crossfeed [OPTIONS] [COMMAND]

Commands:
  on         Turn the crossfeed on, with the numbers it has
  off        Turn the crossfeed off
  model      Choose the model (the words `svolder host crossfeed --json` lists under `models`) and load its default preset
  preset     Load one of the model's presets, by the name the view lists
  level     Set how much of the other channel is crossed, in percent
  crossover  Set the crossover, in Hz
  delay      Set the total lag of the crossed signal, in ms, or `own` for the model's own
  help       Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the crossfeed as the daemon sends it, as one JSON object
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help (see more with '--help')
```

```
$ svolder host crossfeed
crossfeed: off
model: Bauer
preset: bs2b default
level: 37 % · −8.6 dB
crossover: 700 Hz
delay: 0.26 ms (own, floor 0.26 ms)
output: Example DAC
$ svolder host crossfeed preset chu moy
$ svolder host crossfeed level 40
$ svolder host crossfeed delay own
$ svolder host crossfeed on
crossfeed: on
```

The lines are: `crossfeed:` on or off (with the reason after a colon when it cannot be changed now),
the `model`, the `preset` (`Custom (edited from Chu Moy)` after a number was changed), the `level`
as percent and the crossed signal's level in dB (`20·log10` of the share, as the TUI and the web
show it; `--json` also carries bs2b's feed level as `feed_db`), the `crossover`, the `delay` (the
total lag, `own` when it is the model's own, and its floor), the `output` it plays into when known
and the `note` of the last reset ([the four resets](eq.md#it-turns-itself-off)). `--json` prints the
daemon's view as one object, also after a verb: `on`, `model`, `preset`, `edited_from`,
`amount_pct`, `amount_db`, `feed_db`, `crossover_hz`, `delay_ms`, `delay_floor_ms`,
`delay_settable`, `delay_own`, `output`, `note`, `off_reason`, `unavailable`, `curve` (the two level
curves, 61 points) and `models` (every model's words, presets and ranges).

The verbs: `on` and `off`; `model WORD` (`bauer`, `linkwitz`, `meier`, `mid-side`; loads the model's
default preset); `preset NAME` (matched case-insensitively against the current model's presets,
several words need no quotes); `level PERCENT`; `crossover HZ`; `delay MS` or `delay own` (back to
the model's own lag; Mid/Side bass takes no delay). Numbers are plain (`level 37`, `crossover 700`);
a trailing unit is accepted but never needed, which keeps `%` out of scripts and crontabs. A value
past a range is held to it by the daemon, and the answer prints what it holds. Turning crossfeed on
never asks: the preamp is lowered first.

With no daemon running, or one that does not answer, `show` cannot know whether crossfeed plays
(a TUI with no daemon may be playing it): its first line is `crossfeed: unknown: …` with what the
saved state says (`on` or `off`), and the other lines are the saved settings (the delay floor at an
assumed 48 kHz); `--json` carries the saved switch in `on` and the reason in `unavailable`. A
daemon that answers with host audio off shows them as off. Every verb with no daemon exits 3, naming
`svolder daemon install`; a daemon that does not offer crossfeed (an older one) exits 7
(`svolder daemon restart`). See [exit codes](exit-codes.md).

### host loudness

Give the bass and the treble back at a low volume: the computer's [loudness](eq.md#loudness) (what
it does, strength and reference). With no verb it prints the state; a verb changes it. Every verb
goes through `svolderd`, which owns the filter chain and follows the computer's volume, so the CLI
never starts one by itself (`--transport` does not change that).

```
Give the bass and the treble back at a low volume, through the daemon

Usage: svolder host loudness [OPTIONS] [COMMAND]

Commands:
  show       Print the loudness (what no verb does)
  on         Turn the loudness on, with the numbers it has
  off        Turn the loudness off
  strength   Set how much of the correction is given back, in percent
  reference  Set the volume at and above which the loudness is flat, in percent, or `now` for the current volume
  help       Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the loudness as the daemon sends it, as one JSON object
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help (see more with '--help')
```

```
$ svolder host loudness
loudness: on
strength: 50 %
reference: 100 %
volume: 35.0 %
depth: 13.7 dB
bass: +7.3 dB
mids: −7.3 dB
treble: +2.1 dB
$ svolder host loudness strength 70
loudness strength: 70 %
$ svolder host loudness reference now
loudness reference: 35 % (the current volume)
$ svolder host loudness off
loudness off
```

The lines are: `loudness:` on or off (with the reason after a colon when it cannot follow the volume
now), the `strength` and the `reference` in percent, and, once the daemon has read the computer's
volume, the `volume`, the `depth` (how far under the reference the stage plays, in dB) and the shelf
gains it plays at that volume: the `bass` (how far it stands above the middle), the `mids` (how far
the middle sits under it) and the `treble` (how far the top stands above the middle), then the
`note` the daemon left, if any. The three gains are the stage's own, not the response at three
frequencies. `--json` prints the daemon's view as one object, also after a verb: `on`,
`strength_pct`, `reference_pct`, `volume_pct`, `depth_db`, `lift_db`, `mids_db`, `treble_db`,
`following`, `note`, `unavailable`, `curve` (the stage as played now, 61 points) and `ranges`.

The verbs: `on` and `off`; `strength PERCENT` (0 to 100, snapped to 5); `reference PERCENT` (1 to
100) or `reference now` (the volume as it is). Numbers are plain (`strength 70`); a trailing `%` is
accepted but never needed. A value past a range is held to it by the daemon, and the answer says
what it holds. Turning loudness on never asks: it only cuts.

With no daemon running, or one that does not answer, `show` cannot know whether loudness plays (a
TUI with no daemon may be following the volume): its first line is `loudness: unknown: …` with what
the saved settings say (`on` or `off`), read from the state file, else from the settings kept before
any state existed, and the other lines are those settings. The offline `--json` holds no volume
(`volume_pct` is null), `following` false, the reason in `unavailable` and a flat readout and curve,
since what the stage plays is not known. A daemon that answers with host audio off shows loudness as
off, whatever the saved state holds. Every verb with no daemon exits 3, naming
`svolder daemon install`; one with host audio off exits 6; a daemon that does not offer loudness (an
older one) exits 7 (`svolder daemon restart`); a refusal or a bad value exits 1. See [exit
codes](exit-codes.md).

### host speakers

Hear the computer's sound as from two speakers in front of you, on headphones: the computer's
virtual speakers. There are two choices, each shown with
its technical term: **With room · BRIR** (the speakers in a measured room) and **No room · HRTF**
(the same speakers in the open). A set is fetched once from a curated catalogue, or imported from
your own file, and kept on this computer. With no verb the command prints the state; a verb changes
it. Every verb goes through `svolderd`, which owns the filter chain, the sets and every rule that
keeps the speakers from being left on, so the CLI never starts a chain by itself (`--transport`
does not change that). Turning them on turns the crossfeed off, and the answer says so.

```
Two virtual speakers on headphones, through the daemon

Usage: svolder host speakers [OPTIONS] [COMMAND]

Commands:
  show    Print the state: whether they play, the chosen set, its licence and its loudness change
  on      Turn the virtual speakers on, with the chosen set
  off     Turn the virtual speakers off
  list    List the sets: the catalogue's, and the ones you imported, with which are fetched
  fetch   Download a catalogue set, check it and keep it on this computer
  use     Choose a fetched set (a rebuild of the chain, with one short gap when they play)
  import  Import a SOFA file, or a WAV of 4 or 14 channels, as your own set
  forget  Remove a fetched or imported set from this computer
  help    Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the state, the list or the saved set as the daemon sends it, as JSON
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help (see more with '--help')
```

```
$ svolder host speakers list --room with
ID        CHOICE              LABEL         LICENCE     STATE
example   With room · BRIR    Example hall  CC-BY-4.0   not fetched (8.1 MB)
$ svolder host speakers fetch example
svolder: fetching example (8.1 MB)
svolder: fetching example: 50 %
svolder: fetched Example hall (example): With room · BRIR, 7.0 dB quieter
$ svolder host speakers use example --on
svolder: virtual speakers set: Example hall (example) · With room · BRIR
svolder: virtual speakers on
$ svolder host speakers
virtual speakers: on
set: Example hall (example)
choice: With room · BRIR
licence: CC-BY-4.0
angles: left 30.0 deg, right 330.0 deg, 1.5 m
length: 480 ms
loudness: 7.0 dB quieter than with them off
```

The lines `show` prints are: `virtual speakers:` on or off (with the reason after a colon when it
cannot be changed now), the `set` (label and id, or `none chosen`), its `choice`, `licence`,
`credit` (where the licence asks for one), `source` page, the `angles` the speakers were measured
at, the `length` of the responses (`cut to 0.5 s` when they were shortened), the `loudness` change
against off (a quieter sound is the usual price) and the `note` of the last reset. `--json` prints
the daemon's view as one object, also after `on`, `off` and `use`.

The verbs: `on` and `off`; `list [--room with|no]` (the catalogue's sets and your own, with which
are fetched; `--json` prints the daemon's list); `fetch ID` (downloads, checks and converts one,
printing progress to standard error; Ctrl-C, SIGTERM or a closed terminal cancels the download on
the daemon and exits 130); `use ID` (chooses a fetched set, which rebuilds the chain once, with one
short gap, only while they play; `--on` also turns them on); `import FILE [--label NAME] [--room
with|no]` (a SOFA file, or a WAV of 4 or 14 channels, as your own set: the daemon reads the file, so
it must be on the computer the daemon runs on, and the path is made absolute first); `forget ID`
(removes a set; refused while it plays or a fetch is running). `fetch`, `import` and `forget` print
`--json` as the saved set, or `{"forgot":"ID"}`. A daemon set up to fetch nothing says so in `list`
and refuses `fetch` with the step of importing a file.

With no daemon running, or one that does not answer, `show` cannot know whether they play: its
first line is `virtual speakers: unknown: …` with what the saved state says (`on` or `off`), then
the saved set's id; nothing else is claimed of the set. Every verb with no daemon exits 3, naming
`svolder daemon install`; a daemon that does not offer the speakers (an older one) exits 7
(`svolder daemon restart`). See [exit codes](exit-codes.md).

## capture

Record everything svolder learns about a device into one plain text file, to send to the
developers when a device behaves in a way svolder does not understand. A new product is the usual
reason: the file shows what it advertises, what it reports and what it sends when its buttons are
pressed.

```
Record everything about a device into one file to send

Usage: svolder capture [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --seconds <SECONDS>      How long to record every frame while you press the device's buttons, in seconds [default: 60]
      --keep-ids               Keep unit ids, passphrase bytes, the home directory and Bluetooth addresses in the file
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --yes                    Stop the daemon that holds the device, and start it again after, without asking
  -o, --out <FILE>             Write the file here instead of a name made from the product and the time
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

The run, in order:

1. Doctor's checks are collected, as `svolder doctor --json` prints them; nothing is fixed.
2. The scan: every advertisement heard in one 5 s window, with its manufacturer record in hex.
   Other people's devices in range are in it too.
3. The device is chosen as every direct verb chooses it (`-d`, the one configured, the one in
   range) and connected, and every reading it reports is listed, as `inspect` lists them.
4. For `--seconds` seconds (60 by default, 0 to 3600), the frames: press the device's buttons, turn
   its knob, change its input, and every frame either way is recorded with its time; a line
   `-- frame window from +12.345 s --` marks where this step began. `--seconds 0` skips this step.
   Ctrl-C ends the step early and the file is still written.

The file is `svolder-capture-<product>-<date>-<time>.txt` in the current directory (UTC), or the
file `-o` names; an existing file, or a directory that is not there, is refused before anything
runs. The path is printed when it is done. Each section is headed `== doctor ==`, `== adverts ==`,
`== inspect ==` and `== frames ==`; a frame is its time, its direction and its bytes in hex, which
`svolder decode` reads, then what it says. A frame that does not decode is there in hex with the
reason, never left out. The file is readable by you alone (mode 0600).

Unless `--keep-ids` is given, the file holds no unit id, no passphrase byte, no home directory and
no Bluetooth address. Unit ids become `0000000000000001`, `0000000000000002` and so on, the same
everywhere in the file, so an advertisement still parses; the passphrase field of every frame sent,
the value of every passphrase message either way and the passphrase in each advertisement are
zeroed; the home directory is `~` and an address is `00:00:00:00:00:01`. Every frame, one that does
not decode too, is walked message by message and the value of every passphrase message found is
zeroed (a cut one loses everything from its value on, and a bad checksum over a zeroed value is
replaced); only bytes that cannot be placed as a message stay as heard. A sent frame that carried a
passphrase reads `passphrase field set (scrubbed)`, and a decoded passphrase message ends
`(scrubbed)`. Names you gave to devices, listen addresses, adapter names, the time and signal
strengths stay in the file, and so does every reading of a function other than the passphrase; the
names you gave to inputs do not (the readings show the profile's own). Read the file before
sending it.

Capture talks to the device directly, so `--transport daemon` is refused. A daemon that holds the
device has to let go of it: svolder asks first (`--yes` answers for you), stops it, and starts it
again when the capture is over, whatever ends the capture, Ctrl-C included. Under systemd its
socket and service units are stopped and started again if they were running; a daemon that was
started by hand comes back the way svolder starts one, which may not be with the flags it had. If
it cannot be started again, the exact command that does it is printed and the exit code is the
daemon's (3), even after a good capture. Answer no and nothing is captured: `svolder: nothing
captured`, exit 0.

Exit codes: 0 for a capture that ran to its end or was ended in the frame window; 130 for Ctrl-C
before the window; a failure part way (nothing in range, a refused passphrase) writes the file
with what was gathered, ends it with a line `[capture ended: …]` and exits with the failure's
code (see [exit codes](exit-codes.md)).

Example, over the fake device:

```
$ svolder --transport fake capture --seconds 2 -o capture.txt
svolder: checking the host
svolder: recording every frame for 2 s; press the device's buttons now (Ctrl-C ends early)
capture.txt
$ head -4 capture.txt
svolder capture
svolder 0.39.0
taken 2026-10-23 07:33:02 UTC, frame window 2 s
unit ids, passphrase bytes, the home directory and Bluetooth addresses are scrubbed (--keep-ids keeps them)
$ grep -A5 '== adverts' capture.txt
== adverts ==
company 0xffff  020000000000000000000000000100000000030001070000000000
  product 2 Gungnir 2
  unit 0000000000000001
  passphrase present
  firmware 3  module hardware 1 firmware 7  incomplete dfu no
```

## raw

Send a command straight to a function id, bypassing the profile. Useful for testing and diagnosis.

A value type that does not exist, and a value that does not fit its type, are refused before any
device is looked for, whichever way the command would have run. Two families are refused before
anything is sent. Function `250` changes the passphrase. In the firmware-update family, `210.1` and
`210.3.*` cannot be sent; `210.2.<sub>`, the firmware-version query, can.

`data` takes hex bytes, or `""` for a command that carries nothing (a Loki Max preset's save,
`7.1.<p>`, or recall, `7.2.<p>`).

```
Send a command straight to a function id, bypassing the profile

Usage: svolder raw [OPTIONS] <ID> <VALUE_TYPE> <VALUE>

Arguments:
  <ID>          The function address, e.g. `2` or `210.2.1`
  <VALUE_TYPE>  The wire type: u8, u16, u32, u64, i8, i16, i32, i64, bool, or data
  <VALUE>       The value: a number, `on`/`off`/`true`/`false`/`1`/`0`, or hex bytes for `data` (empty for none)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --no-wait                Send without waiting for the device to confirm
      --json                   Print one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --no-start               Use only a daemon that already answers on the socket: start no socket unit, no daemon and no direct BlueZ session, and exit 3 when none answers. Overrides `--transport` [env: SVOLDER_NO_START=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake raw 11 u8 100
Status 11 u8 100   Balance = 36
```

The firmware-version query, the one sendable member of the 210 family:

```
$ svolder --transport fake raw 210.2.1 u8 0
Status 210.2.1 u16 3
```

Refused functions:

```
$ svolder --transport fake raw 11 u8 300
svolder: 300 does not fit in a u8. See `svolder --help`

$ svolder --transport fake raw 11 nosuch 1
svolder: unknown value type "nosuch". See `svolder --help`

$ svolder --transport fake raw 210.1 u8 0
svolder: function 210.1 belongs to the firmware-update family and cannot be sent

$ svolder --transport fake raw 210.3.1 u8 0
svolder: function 210.3.1 belongs to the firmware-update family and cannot be sent

$ svolder --transport fake raw 250 u32 1
svolder: function 250 changes the passphrase; use `svolder passphrase set`
```

## decode

Decode a frame captured from the app or the device, given as hex (spaces allowed), without a device
or a daemon. `--profile <name>` names the functions with that profile, and `--json` prints one JSON
object. A capture file's frames (see "capture") are the usual input.

```
$ svolder decode --profile gungnir2-v2 "03 00 c1 02 03 02 00 b1 05 7e fe"
Status 2 u8 3   Input = 3 (Optical)
Status 5 bool true   Mute = on
```

## profiles

`svolder profiles list` names every built-in and user profile with its product id and firmware
floor, and `svolder profiles show <name>` prints one fully resolved, such as `freya`. User profiles
in `~/.config/svolder/profiles/*.toml` add devices or override a built-in by `name`; a profile's
`family` (`dac`, `amp` or `preamp`) picks which of the TUI's palette accents it draws with. The
format is in [profiles/README.md](../../profiles/README.md).

`profiles show` lists every control with its id, key, kind and label, and under it what the profile
says of it: its values, its range, how a meter scales, and the ids a control is also read from
(`reads 6.1` on a Loki Max band, which the device may report on either id). An action follows the
controls: its id with the slots it takes in angle brackets, `overwrites what the device holds` when
a front end asks before it, and the controls whose readings the device sets after it (`changes`). A
device with built-in settings ends with them, the bands they set and the raw steps of each:

```
7.2.<1-3> recall                 action   Recall preset
                                            changes band1, band2, band3, band4, band5, band6
7.1.<1-3> save                   action   Save preset
                                            overwrites what the device holds

built-in settings of band1, band2, band3, band4, band5, band6
  written to 6.1, 6.2, 6.3, 6.4, 6.5, 6.6
  Flat          15 15 15 15 15 15
  Bass Boost    28 21 17 15 15 15
```
