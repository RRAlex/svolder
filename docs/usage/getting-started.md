<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Getting started

The guided walk from bare `svolder` to a connected device: what the front door does on a terminal
and off one, the setup wizard and the first commands. Back to the [usage index](README.md).

## Install

The [release page](https://github.com/RRAlex/svolder/releases/latest) carries every file for x86_64
and arm64 with its checksum in `SHA256SUMS`. Pick the one for the machine:

| Machine | File | Command |
| --- | --- | --- |
| Debian, Ubuntu, Raspberry Pi OS (64-bit) | `svolder_<version>_amd64.deb` or `_arm64.deb` | `sudo apt install ./<file>` |
| Fedora, RHEL | `svolder-<version>-1.x86_64.rpm` | `sudo dnf install ./<file>` |
| Arch | `PKGBUILD` and the tarball for the machine | `makepkg -si` in a folder holding both |
| Nix | | `nix run github:RRAlex/svolder/v<version>` |
| Any Linux | `svolder-<version>-x86_64-linux.tar.gz` or `-aarch64-linux.tar.gz` | unpack; static binaries, BlueZ only |
| Container | | `docker pull ghcr.io/rralex/svolder:<version>`; see [Running in a container](daemon.md#running-in-a-container) |

A package enables nothing. Each user turns the daemon on with
`systemctl --user enable --now svolderd.socket svolderd.service` (from a tarball: put the binaries
on `PATH` and run `svolder daemon install`), then runs `svolder`. On a machine nobody logs in to,
such as a Raspberry Pi beside the DAC, `loginctl enable-linger $USER` keeps the units running after
a reboot. Check a download with `sha256sum -c SHA256SUMS`. To build from source instead, see the
"Build" section of [CONTRIBUTING.md](../../CONTRIBUTING.md#build).

## Getting started

Bare `svolder` is the front door. On a terminal: the TUI when at least one device is configured, the
setup wizard (see ["setup"](cli.md#setup)) when none is. The TUI always opens, whatever the state of
the device or the daemon: a device the daemon has not found yet gives a waiting panel that fills in
by itself when it is found, and a device or a daemon that cannot be reached gives the Devices screen
with the failure and its next step above the configured devices (see ["tui"](tui.md#tui)).

Before it chooses, the front door (and `svolder tui`) looks at a daemon that is already answering:
when that daemon reads this same configuration file and holds other device names than the file
lists, it is told once to read the file again, and a toast says `svolderd re-read <path>` (on the
wizard path the note heads its first screen). A daemon reading another file is left alone and named:
a toast says `svolderd reads <its path>, not <this path>`, with `~` for the home directory (a daemon
too old to say which file it reads gets no note, and neither does one whose path does not exist on
this machine, such as a container's `/config/...`).

The look is a plain connect to the socket, with no launcher run and no daemon spawned; a host whose
socket unit is listening starts the daemon behind it, which reads the current file and so needs no
reload. A daemon that does not answer within two seconds is left alone, with no note. Off a terminal
— a script, or a shell over SSH — it prints one summary instead: the host's health in one line, each
configured device and its state (a device the daemon is waiting to try again for carries the wait,
as in `not connected, next attempt in 15 s`), and the next command to run, then exits 0 without
asking anything. Prompts appear only on a terminal, on every verb this reference documents.

`svolder --help` groups every verb by what a new user needs first, what everyone runs daily, what
changes the configured devices, what looks after the daemon, and what takes something apart:

```
Control a Schiit Forkbeard device from a Linux terminal

Usage: svolder [OPTIONS] [COMMAND]

Getting started:
  setup       The guided first run, safe to run again
  doctor      Check the host and each configured device, and offer fixes
  scan        List every device in range with its ready-to-run `add` line

Everyday:
  tui         Open the interactive terminal UI
  status      Connect to a device and print every control's current value
  get         Read one control's current value
  set         Set one control's value
  watch       Connect to a device and print every status it reports until Ctrl-C
  eq          Inspect or change the tone control
  host        Correct what the computer plays, before it reaches the DAC

Devices:
  devices     List the configured devices and their state
  add         Add a device to the configuration
  remove      Take a device out of the configuration
  identify    Ask the device to show itself, so you can tell which it is
  passphrase  Set or clear the device passphrase

Daemon:
  daemon      Check, install or control the background service
  mcp         The MCP server's bearer token
  web         Open the web UI, or print a link, a code or its token

Diagnostics:
  inspect     Connect, refresh, and print everything the device sends
  capture     Record everything about a device into one file to send
  raw         Send a command straight to a function id, bypassing the profile
  decode      Decode a captured frame (command or notification) given as hex
  profiles    Inspect device profiles
  config      Configuration paths, settings and the template

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
  -V, --version                Print version

Run `svolder help env` to list the environment variables it reads.
```

`svolder -V` prints the one-line version; `svolder --version` (like `svolderd --version` and
`svolder-sim --version`) prints it with the copyright and licence notice.

None of these verbs has an alias for another name. See ["doctor"](cli.md#doctor) and
["setup"](cli.md#setup) for the guided checks and the guided first run, ["daemon"](daemon.md#daemon)
for the background service's own verbs, and ["Daemon and config"](daemon.md#daemon-and-config) for
the launcher chain `--transport auto` walks to find or start one.
