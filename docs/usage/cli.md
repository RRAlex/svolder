<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Device verbs

The device verbs: `doctor`, `setup`, `devices`, `add`, `remove`, `hops`, `passphrase`, `scan`,
`inspect`, `status`, `get`, `set`, `preset`, `watch`, `host`, `raw`, `decode` and `profiles`. Back
to the [usage index](README.md).

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
| Daemon | whether the systemd units are installed, enabled, and pointed at the resolved socket; a note, not a failure, where there is no systemd at all — autospawn or a direct session serve instead. A daemon answering on the socket with a version other than this svolder's grades `!` (`daemon vX.Y.Z is older than this svolder vA.B.C`, or `newer than` when the daemon is the later build, compared in semver order) and, for an older daemon, gives the fix `svolder daemon restart`, which doctor prints and never runs; a newer daemon has no fix to print, since restarting it changes nothing |
| Other daemons | one row per other svolderd of this user's listening on a socket this session does not reach — neither the one it resolves nor the one the systemd unit listens on — whoever started it and whatever network namespace it runs in: its pid, where it listens (the listening socket it holds, or its `--socket`/`SVOLDER_SOCKET` when its descriptors or its namespace's socket table cannot be read; for a daemon in another mount namespace, such as a container, that path reached through `/proc/<pid>/root`), and the command that stops that one, which doctor prints and never runs |
| Config | whether the file parses, whether every device names a profile that exists, and whether any device is configured at all; a profile directory beside the file that cannot be read is named, with the reason, and a device whose profile may be in it is not called missing |
| Config file | (no file yet) offers to create one from the template |
| Devices | (devices configured) one row per device: connected where a daemon holds it — no scan is run for one, since a device a central holds stops advertising — else seen in a scan and how loud, or not seen in the windows this verb listened |
| MCP server | (`[mcp]` on, or a daemon reporting one) the address the daemon listens on and how requests prove themselves (`listening on 127.0.0.1:8737 (token, read-only)`); fails when the daemon could not bind the address or read the token, when a token file group or others can read, or when a named `token_file` is missing; reads no token and creates none |
| Web UI | (`[web]` on, or a daemon reporting one) the address the daemon listens on and how a browser proves itself (`listening on 127.0.0.1:8738 (token, writes)`); fails when the daemon could not bind the address or read the token, when `auth = "none"` is on an address that is not loopback, and when a token file group or others can read, or a named `token_file` is missing; warns when the section is on and the daemon says its web UI is off; notes a listen address off loopback, which serves plain HTTP on the network; reads no token and creates none |
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
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

On a terminal, `svolder setup` and bare `svolder` with no device configured (see
["Getting started"](getting-started.md#getting-started)) walk the same nine steps as screens of a
TUI wizard — see ["tui"](tui.md#tui)'s ["Setup wizard"](tui.md#setup-wizard) section below for the
keys and the mockups. `svolder add` with no `<UDID>` on a terminal runs the same wizard starting
from its Scan step, with `--name`, `--profile` and `--passphrase` (below) pre-filling its
answers. The steps:

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
   profile the advertisement matches; an unknown product offers the profile list instead.
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
refused; the file is re-parsed after writing, before this reports success, so a config that would
not load is never left behind. Without a `<UDID>`, on a terminal, `add` runs the same guided walk
`setup` does (see "setup" above), starting from its Scan step, with `--name`, `--profile` and
`--passphrase` pre-filling its answers; off a terminal it prints the hint toward `svolder scan`
instead:

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
or the TUI's Devices screen run right after never shows a device the file no longer names.

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
answers `this daemon is too old to identify; restart it`, exit 1. A device whose profile offers no
identify (the Mesh Card, where the app offers none) is refused in words, `NAME offers no identify`,
exit 1; `svolder raw 200` can still send the function.

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
by steps. Without `--no-wait`, the command waits for the device's confirmation (exit code 4 if
timeout).

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

and through the daemon, it is the subscription going quiet:

```
svolder: connection failed: the daemon stopped sending events. If it does not come back, run `svolder doctor`
```

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
  eq    An equalizer on what the computer plays
  help  Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### host eq

The group's one subcommand, with `apply`, `remove` and `status` below it.

```
An equalizer on what the computer plays

Usage: svolder host eq [OPTIONS] <COMMAND>

Commands:
  apply   Put a correction in front of the sink that feeds the DAC and keep it there
  remove  Stop the computer EQ and delete what was written for it
  status  Print what is applied and whether it runs
  help    Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
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

The built-in presets are the DAC's own and are refused. The sink is `--sink`, a `node.name` or a
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

`SVOLDER_HOST_EQ_MANAGER=systemd|process` forces one. An unrecognised value of
`SVOLDER_HOST_EQ_MANAGER` is refused, naming the two. It refuses in a container
(`host EQ is not available in a container`, with
`run svolder host eq on the machine that plays the sound`) and with `SVOLDER_HOST=off`. The
`pipewire` program is `SVOLDER_PIPEWIRE`, else `pipewire` on `PATH`; the WirePlumber version is read
from `SVOLDER_WIREPLUMBER`, else `wireplumber` on `PATH`, as `wireplumber --version`.

svolder writes these files for the computer EQ and nothing else: `~/.config/svolder/host-eq.conf`
(the complete PipeWire config, run as `pipewire -c`), the unit or the autostart entry, the pid file
of a process, and `~/.local/state/svolder/host-eq.toml`. `host eq remove` deletes what apply wrote.
PipeWire's own configuration directories, `filter-chain.service` and every unit but svolder's own
are never touched.

### host eq remove

Stop the computer EQ and delete what apply wrote (the config, the unit or the autostart entry and
the pid file); the state file stays, with `on = false`, so the last correction can be applied again.
On the default route the default output goes back to what it was. A chain that is already gone,
after a reboot or a kill, is removed all the same, and a remove with nothing applied does nothing.
`svolder: host EQ removed` says it was on, `svolder: host EQ was not on` that it was not.

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

A state file that says on while nothing runs reads `stopped` and names the fix on a second line,
`the unit is not running: systemctl --user status svolder-host-eq` (for a process,
`the process is not running: apply it again with svolder host eq apply`); one that says off while
the chain runs reads `the unit is still running: svolder host eq remove`. `--json` prints one object
with `on`, `running` (`null` when it could not be told), `name`, `origin`, `target`, `route`,
`manager`, `previous_default`, `preamp_db`, `filters` (each `type`, `f`, `g`, `q`) and `problem`;
with nothing ever applied it prints `{"on":false,"running":null}`; a state file that cannot be read
says so, `host EQ: unknown: …`, instead of reading as off.

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
svolder 0.31.3
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
