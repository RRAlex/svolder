<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Exit codes

What each verb returns to the shell. Back to the [usage index](README.md).

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Command succeeded |
| 1 | Usage error, or command refused (e.g., restricted function, bad control name) |
| 2 | Device not found (no matching device in scan results, or device selector did not match) |
| 3 | Connection failed |
| 4 | Device did not confirm the command within the timeout |
| 5 | An active fault was reported by the device |
| 130 | `watch` was told not to wait for the release (Ctrl-C during it); `capture` was ended by Ctrl-C before its frame window |

With no `-d` and several devices in range, the candidates are listed on stderr and the command exits
2; name one of them with `-d`:

```
svolder: 2 devices in range: 0102030405060708  Gungnir 2; 1112131415161718  Bifrost 3. Pick one with `-d NAME` or set `default_device` in the config
```

A command answered by the daemon exits with the same code the daemon chose. Every failure a device
path can emit prints as `svolder: <what>. <next>` — what happened, then the one step that follows
from it — from one table keyed by what went wrong, so the CLI's wording and the exit code always
agree:

| what went wrong | code | next step |
|---|---|---|
| no adapter, or a named one missing | 3 | `svolder doctor` |
| nothing in range (`scan`, `add`, the wizard) | 2 | power the device on and close the phone app, then `svolder scan` again |
| several devices and no `-d` | 2 | `-d NAME`, or `default_device` in the config |
| connect refused, or a write rejected by a passphrase | 3 | give svolder the device's passcode (`svolder setup`, the Devices screen's `u`, or `passphrase` in the config); if it is lost, `auto` reads it from the device — see ["Forgot the passphrase"](cli.md#forgot-the-passphrase) |
| a device dropped during `watch` or the TUI | 3 | the reconnect notice, then "if it does not come back, `svolder doctor`" |
| the daemon answered with an error | its own | `svolder daemon logs` |
| a device the daemon holds has not been found yet (`amp has not been found yet`) | 3 | svolderd keeps looking: check that it is on and in range (`svolder daemon logs`) |
| the daemon's configuration names no device (the message names the daemon by its socket) | its own (2) | `svolder --socket <that socket> add <udid> --name NAME`, which also tells that daemon to reload, run with the `XDG_CONFIG_HOME` that makes `add` write the file the daemon reads (its `--config`, or the one under its own `XDG_CONFIG_HOME`) |
| autospawn could not start the daemon | 3 | the log `svolder daemon logs` prints |
| the config unreadable | 1 | `svolder config path` |
| an unknown product in the advertisement | 1 | `svolder profiles list`; user profiles live in `~/.config/svolder/profiles` |
| `--transport daemon` finds nothing | 3 | `svolder daemon install`, or drop `--transport daemon` |
| a flag or value the command line got wrong | 1 | `svolder --help` |

`doctor`'s own exit code is separate from this table: 0 when every check passes, 1 when something
warns, 2 when something fails, whatever `--yes` or the guided first run it may have run went on to
do (see ["doctor"](cli.md#doctor)).

`capture` leaves with 0 when it ran to its end or was ended by Ctrl-C in its frame window, 130 when
Ctrl-C ended it earlier, and the failure's code from the table above when something failed part way,
the file still written with what was gathered. A daemon that was stopped for the capture and could
not be started again makes it 3, whatever else happened (see ["capture"](cli.md#capture)).
