<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The GNOME top-bar indicator

How the GNOME Shell extension is built and why. What it shows and how to install it is in
[the usage page](../usage/gnome-shell.md).

## Shape

A thin extension in `contrib/gnome-shell/svolder-indicator@rralex.github.io/` that does two things:

- it reads one push stream, `svolder panel --json`, and redraws from every line;
- it runs one `svolder` call per click (volume, mute, input, DAC preset, computer EQ, scene).

Everything that is a judgement lives in Rust. The extension draws.

```
svolderd ──socket──▶ svolder panel --json ──lines──▶ extension (draws)
                     ◀── svolder --no-start --transport daemon <args> ◀── a click
```

The extension supports GNOME Shell 50 and 51, which changed shell APIs between them; it is written
to the common subset (ES modules only, no `vertical:` on St widgets, `open()` and `close()` without
an animation argument, a synchronous `disable()`).

## Decisions

1. **UUID `svolder-indicator@rralex.github.io`.** The usual form of a UUID: a name, `@`, and a
   domain the author controls (the author's GitHub Pages one), so a later submission to
   extensions.gnome.org needs no rename.
2. **One model, built in Rust: `PanelView`** (in `svolder-ipc`), printed as JSON by `svolder panel
   --json`. The extension never re-judges (the same rule as the Path row in the TUI and the web
   page): the Path strip's tones and the off-only text come from the Path judge (`path_row` and
   `PathLine`), the signal's bars, weak and old flags from one rule in `svolder-ipc`, and every
   choice list comes with the CLI arguments that select it. The JavaScript keeps only drawing, the
   sample-rate tint table and the timer that turns `heard_at_ms` into "3 min ago" when the popup
   opens. The model is built in Rust so that one judge serves every front end and so that a golden
   file written by a Rust test is what the JavaScript tests read: a drift in the model fails a test
   on one side or the other.
3. **`svolder panel --json`** subscribes with `host: true` and prints one whole `PanelView` object
   per line: the first as soon as the snapshot is assembled (readings, devices, host view and
   scenes), then one per change, coalesced to at most four lines a second and never two identical
   lines in a row. The signal is not pushed by the daemon, so a `devices` call every 15 seconds
   supplies it. The verb talks to a running daemon and nothing else, whatever the global
   transport says: it never starts a daemon and never opens a direct Bluetooth session (exit 3 when
   no daemon answers, the existing no-daemon text). The daemon stopping ends the stream with the
   same exit 3. The device is chosen as the CLI does elsewhere: `-d`, else `default_device`, else
   the only device; with several and none chosen it exits 2 with the candidate list. The stream
   only connects to the daemon's socket. If `svolderd.socket` is installed, systemd starts the
   daemon on that first connect: the verb itself starts nothing. A DAC that is
   away, or no device at all, is not a failure: the stream goes on with `present: false` or
   `device: null`; a stream that began with no device ends with exit 0 when one appears, and the
   extension restarts it at once.
4. **The signal rule**, in `svolder-ipc` next to `LIVE_FRESH_SECS`: 4 bars at -55 dBm or stronger, 3
   at -67, 2 at -80, 1 below that (which is "weak"); the reading is `old` when older than 120
   seconds, and the stream prints a new line when `old` flips. While a Bluetooth LE link is up
   BlueZ reports no strength, so the reading is the last advertisement before connecting and the
   bars go dim two minutes after connecting. The popup header then reads `−58 dBm · 3 min ago`.
5. **Clicks** run `svolder --no-start --transport daemon <args>` with the arguments the model
   gives, and only while the stream is live. The global `--no-start` (or `SVOLDER_NO_START`) uses
   only a daemon that already answers: no socket unit, no daemon, no direct Bluetooth session, so
   the indicator's own calls start nothing. Open svolder is the exception, by design: it runs
   `svolder tui`, which follows the normal launcher chain.
   - Scrolling the bar is `set volume up|down`: the device's own step and the daemon's volume
     rule. Notches that arrive while a call runs are added to the next call's `--step N`.
   - The slider writes once, on release (`set volume <dB>dB`), never while dragging.
   - The extension never passes `--yes`. A verb that needs an answer and has no terminal exits 1
     with a line naming `--yes`; that shows one line in the popup, `needs a confirmation: open
     svolder`. Exit 4, the device not confirming in time, shows the CLI's own line.
   - Mute toggles from the current state (`set mute on|off`).
   - The computer EQ switch and preset use `host eq on`, `host eq off` and `host eq preset NAME`,
     which map onto the daemon's host EQ operations, the same ones the TUI uses.
   - The DAC preset row is the list the TUI's DAC preset row uses: tone presets (`eq load NAME`)
     on a profile with a tone control, otherwise the preset slots (`set recall N`) and the
     built-in settings (`preset apply NAME`).
   - Scenes load with `scene load NAME`, as the TUI does.
6. **Open svolder** runs `svolder tui`, with no flags, in the user's terminal (an application
   launched with the needs-a-terminal flag), as an explicit user action. It is always there; the
   web UI needs a port and a token.
7. **Failures** show as one line in the popup and the bar draws the away state: the CLI is missing
   (spawning fails: "svolder is not on PATH"; the shell's own `PATH` is used, so a CLI only on a
   terminal's `PATH` is not found), a model of another version ("this svolder speaks another panel
   version"), the wait for the first line ("waiting for svolder"), no daemon (exit 3, with the CLI's
   next-step line from standard error), several devices (exit 2, the CLI's first line), a daemon too
   old (exit 7). The stream restarts with a backoff of 1, 2, 4, 8, 16, then 30 seconds (it stays at
   30) and the schedule resets after the stream has been healthy for 60 seconds. Every signal, timer
   source and the child process is removed in `disable()`, which is synchronous because GNOME 51
   throws on an asynchronous one.
8. **Tests.** Every check is shown to fail on a broken build of what it guards.
   - Rust: `PanelView` built from fixture readings (untouched end to end, the DAC at -24.1 dB with
     tone control, resampled, a failing config check, away, no device, a sound for another output or
     input, a player not read, muted, weak and old signal, no computer EQ, several scenes), with
     golden JSON Lines files under `contrib/gnome-shell/tests/fixtures/` that the Rust test writes
     (`SVOLDER_BLESS=1` rewrites them) and otherwise fails on any drift. The stream loop runs on
     tokio's paused clock against the fake transport: coalescing, no duplicate line, a new line when
     `old` flips, the exit codes.
   - JavaScript, with `gjs -m` and no shell imports: `model.js` turns each fixture line into the
     bar's and the popup's view data, and the argument building, the backoff schedule, the scroll
     accumulator and the markup escaping of device and scene names are tested.
   - A real load in `gnome-shell-test-tool --headless` under `dbus-run-session`, with a temporary
     home and a fake `svolder` first on the `PATH` that prints fixture lines and logs its
     arguments: the indicator is in the panel, the bar shows the fixture's figure, the menu opens
     with its rows, a click on a scene logs the scene's arguments, and after `disable()` the
     fake's process is gone. It never touches a user's session, dconf or daemon.
   - `make gnome-extension-check` runs all of it and skips the load, with a message, when
     `gnome-shell-test-tool` is absent.
9. **Looking.** Screenshots of the headless shell's panel and menu serve for review; the
   look on a real session needs a log out and in on Wayland.

## Drawing facts

- **The box**, 18 by 14: an outline of a rounded rectangle (x 1.5, y 3, 15 wide, 8 high, radius 2.2,
  stroke 1.8) stroked in the rate tint; the light is a filled circle (centre 13, 7; radius 1.7) in
  green `#8ff0a4` (the Path row's label is green) or yellow `#f8e45c` (it is yellow: something
  changes the sound or the config check fails). Away, or a plain label (no verdict): a hollow ring
  (radius 1.3, stroke `#8a8a8a` 1.1) and the outline in the plain foreground colour.
- **The tint**, one colour per rate, saturation rising with the rate, lightness kept high, and no
  green or yellow (they are the light's verdict colours): 44.1 `#cfcfcf`, 48 `#ffffff`, 88.2
  `#b8d8ff`, 96 `#86b4ff`, 176.4 `#a58aff`, 192 `#d476f0`, 352.8 `#f27aa8`, 384 `#ff8c72`. Any
  other rate is untinted.
- **The signal**, 14 by 12: four rounded bars of rising height (x = 4i, y = 9 - 3i, width 3, height
  3 + 3i, radius 1); filled bars at full opacity, the rest at 0.3 (an old reading: 0.5 and 0.15).
  Fine is white; weak is one yellow bar `#f8e45c`; away is no filled bar, in grey `#8a8a8a`.
- **The figure**: bold 12 px, `−24` and a `dB` at 0.72 em lowered 0.22 em below the baseline with a
  1 px gap, in the same font (Pango markup, not a CJK compatibility glyph); the minus is U+2212.
  Muted: the muted-speaker glyph replaces the figure (no strike-through); the light still
  follows the Path verdict, which counts a mute as a change at every hop, the DAC's included, so
  it turns yellow. Away: no figure.
- **The popup header**: the box, the bold name, the rate in grey with a small `kHz`, then
  right-aligned grey bars and `−58` with a small `dBm`.
- **The volume row**: the speaker button (the mute toggle), the slider (it shrinks), and the value
  `−24.1 dB` right-aligned, tabular, at least 72 px, never wrapping.
- **The rows**: Input, DAC preset, Computer EQ with its preset and a switch, then the Path row.
- **The Path row**: three 22 by 12 segments (player, computer, DAC; green, yellow or grey) and the
  off-only text in yellow.
- **Then** a separator, `Scenes` with the scene that is on at the right (it is marked with a green
  dot in the list), and `Open svolder`.

No hover tooltips (not a top-bar idiom), no now-playing, no transport and no graphs.

## What it leaves out

- **extensions.gnome.org.** Not yet; the zip `make gnome-extension-check` builds is what such a
  submission would start from.
- **Starting a daemon.** The extension never autospawns one: a panel that started a background
  daemon and a Bluetooth session on log-in would surprise people.
- **`--yes`.** A confirmation is a decision the person makes in the TUI.
- **Another model.** The web page, the TUI and MCP keep their own models; `PanelView` is the
  indicator's, and any later top-bar or applet front end can read the same stream.
