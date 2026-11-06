<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The terminal UI

`svolder tui`: every screen, key and setting of the terminal UI. Back to the
[usage index](README.md).

## tui

Open a full-screen terminal UI: a `subscribe` client that also `set`s, redrawing every control, the
tone control, and telemetry as the device reports them. `-d`, `--transport`, `--adapter`,
`--socket`, and `-v` apply exactly as they do for every other verb — `auto` runs against a daemon
when one answers the resolved socket, else opens a direct BlueZ session, and `--transport fake` (or
`SVOLDER_TRANSPORT=fake`) talks to the in-process fake device instead of either.

```
Open the interactive terminal UI

Usage: svolder tui [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --no-color               Force monochrome rendering; also set by the `NO_COLOR` environment variable
      --demo                   Try the TUI against a fake device on a daemon of its own, in a private directory that is removed on exit; nothing of yours is read or written
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --host                   With --demo, leave the computer's audio on: the real default sink's volume and meters, the real media player and the computer EQ, on the fake device. A volume or player change stays after the demo; the computer EQ the demo applied is removed
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
      --web                    With --demo, serve the web UI on a port of its own on every interface, signed in by token; the TUI's Web box and standard error show the link
      --join <SOCKET>          Open the TUI on the daemon listening on SOCKET and nothing else: no daemon is started and no direct session stands in, so a second front end can join a demo or any private daemon
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

`svolder tui --demo` tries the TUI with no device, no configuration and no daemon of your own
(`make demo-safe` builds and runs it from a checkout; `make demo` is the variant below that uses your
computer's real audio). It makes a private directory (mode 0700, `sd-` and
six random hex digits, under the system's temporary directory), starts the `svolderd` that stands
beside `svolder` (there is no search of `PATH`; it says what to build when that one is missing) in
it over the fake device, and opens the TUI on that daemon. The fake device honours
`SVOLDER_FAKE_PROFILE`, `SVOLDER_FAKE_FAULTS` and `SVOLDER_FAKE_SIGNAL`:
`SVOLDER_FAKE_PROFILE=fake-amp SVOLDER_FAKE_SIGNAL=1 svolder tui --demo` tries an amplifier whose
meters play a music-like signal.

The daemon and the TUI read their configuration and state from the private directory, which is also
their runtime directory, and the daemon's `systemctl` and unit directory are not there either, with
MQTT off, and `SVOLDER_HOST` is set to `off` for the TUI (and the daemon runs a fake host), since the host controls would change your own
speakers' volume and your media players: nothing of yours is read, written or acted on, not your
configuration, your state, your socket, your daemon or your audio. The TUI only ever connects to
that daemon's socket, so no daemon is started beside it and `--transport`, `--adapter` and
`--socket` do not apply (`-d` and `-v` do); adding a device from the Devices screen says it is not
part of the demo.

Once the intro is over a toast, which stays six seconds, names the command for a second front end on
the same daemon, `svolder tui --join <directory>/sv.sock`, to see two front ends on one daemon or
the daemon's version; the Esc menu keeps it on a `Join with:` line under the box for as long as the
demo runs. A demo whose daemon does not start keeps its directory and the message names the daemon's
own log in it (the next demo removes it); Ctrl-C while it starts prints
`svolder: interrupted before the device answered` and exits 0, as it does while the plain TUI opens.
Quitting (`q`, Ctrl-C), an error, a panic and a signal stop the daemon and remove the directory; a
TUI killed outright leaves the directory behind but not the daemon, which ends with it, and the next
demo removes any such directory that is over a minute old and has no daemon holding it.

`--host` and `--web` are opt-in extras of `--demo` and are refused without it (`make demo` runs
`svolder tui --demo --host --web`; `make demo-dac` and `make demo-amp` pick the fake DAC or the fake
amplifier with its meters moving; `PROFILE=<fake profile>` and `SIGNAL=1` reach the fake device as
`SVOLDER_FAKE_PROFILE` and `SVOLDER_FAKE_SIGNAL`). The fake device list has one device per run, so
there is no DAC-with-an-amplifier demo.

`--host` leaves the host on for the daemon and the TUI, so the fake device sits beside your computer's
real audio: the spectrum and meters follow the default sink, the Now playing card and its transport
drive your real media player, the Host volume row turns your real sink, and `h` in the preset list
puts the computer EQ on. These are your real ones: a volume or player change made in the demo stays
after it. The demo says so once on standard error before the TUI opens and once in the TUI,
`demo: fake devices, your computer's real audio (volume, player, EQ)`. The private directory stays the
runtime directory, since the computer EQ's pid file goes in the runtime directory and must not sit
beside one of yours, so the host tools are pointed at your session instead: `PIPEWIRE_RUNTIME_DIR` is
the `XDG_RUNTIME_DIR` you ran it from (else `/run/user/<uid>`, or `$PIPEWIRE_RUNTIME_DIR` when you
set one; `wpctl`, `pw-record`, `pw-dump`, `pw-play` and the computer EQ's `pipewire` all read it) and
`DBUS_SESSION_BUS_ADDRESS` is yours (else the `bus` socket in that directory), which the player
watcher reads. With no PipeWire socket there the demo does not start and says so. The computer EQ is
kept by the `process` manager (`SVOLDER_HOST_EQ_MANAGER=process`), never a systemd unit, so nothing
reaches your unit directory; its configuration, state, pid file and autostart entry are all in the
private directory. A computer EQ the demo applied is removed when the demo ends (`q`, an error, a
panic or a signal); one of yours that was already on is left as it was, since the demo reads and
writes only its own state file, so turn yours off before trying `h` (two chains would stack). A TUI
killed outright cannot remove its chain: `pkill -f '^[^ ]*pipewire -c /[^ ]*/sd-[0-9a-f]{6}/config/svolder/host-eq\.conf$'` stops the demo's, and only the demo's. MQTT stays off, and the
daemon's `systemctl` and unit directory stay out as before.

`--web` writes a `[web]` block into the demo's own configuration: enabled, listening on every
interface (`0.0.0.0`) on a free port the kernel picked (never 8737 or 8738), `auth = "token"`, writes
on. The page is plain HTTP, so anyone on your network who copies a sign-in link or token could use
it while the demo runs. Standard error shows a link that signs this computer in, once and for five
minutes, as `svolder web` does, before the TUI opens; the TUI's Web box (Settings) shows the link and
the QR code for a phone, as it does for any daemon.

`svolder tui --join <socket>` opens the TUI on the daemon listening on that socket and nothing else:
it never walks the launcher chain, starts no daemon, opens no direct session and adds no device, so
it is the front end to run in a second terminal on a demo or any private daemon. With no daemon
answering there the TUI still opens, on the Devices screen, with
`no daemon answers on <socket>; nothing was started` and the step to start one in the panel above
the list; nothing is started, and `r` tries the socket again. `--join` cannot be combined with
`--demo`, and `-d`, `-v` and `--no-color` apply; `--transport`, `--adapter` and `--socket` do not.

`svolder tui` needs a real terminal: with stdout not a TTY, or a `config.toml` that fails to load,
it exits 1 before opening anything. Past that it always opens: nothing about the device or the
daemon ends a start. Only `q` or Ctrl-C, a terminal that cannot be claimed and a configuration file
that cannot be parsed do. Where it lands depends on what the open came to:

- The device opened: the device screen.
- The daemon holds the device and has not found it yet (the open is answered with code 3 and the
  daemon's own device list has the device, not connected): the device screen shows the waiting panel
  (see "Device screen" below), and opens the device in full by itself the moment the daemon finds
  it.
- Anything else (no daemon with `--transport daemon`, nothing in range for a direct session, a
  refused connect): the Devices screen, listing the configured devices, with a panel above them that
  gives the failure's own words and the step that follows. `r` tries the whole start again, `Enter`
  on a row opens that device and `n` adds one.

Switching to a device the daemon has not found yet, from the Devices screen, shows the same waiting
panel on that device. The menu, Devices, Settings and help work on every landing. Every keystroke
sends immediately; a row shows the last value the device confirmed, so a dropped command is visible
as a row that doesn't move. A lost connection hollows the header's connection dot, turns a small
spinner in front of it, and greys every control out at its last-known value; the screen wakes back
up on the first status the device sends after it reconnects, and the spinner with it.

The terminal is claimed and the Svolder wordmark intro begins (see "Palette and the wordmark" below)
before the device is even opened, so the seconds a connect takes are spent on the mark rather than
on an empty screen; every open the running session makes afterwards — switching device from the `d`
list included — happens with the alternate screen already up.

Because of that, `-v` says only one thing about a `tui` run, printed once before the screen is
claimed: `svolder: tui starting at HH:MM (<depth> colour)`, rather than the
`daemon at …`/`direct (…)` line every other verb prints. Ctrl-C or `q`, pressed while the device is
still opening — over the wordmark or over the wait that follows it, one press either way — gives up
on the wait: the terminal is restored, `svolder: interrupted before the device answered` is printed,
and the command exits 0.

`svolder tui` opens on the device screen. Every screen's keys follow three rules, printed as the
help overlay's own first lines:

- Lowercase letters act on the device, or open a screen or the Settings box.
- Uppercase letters toggle a TUI mode and save it to the config.
- Symbols and navigation keys are the same on every screen.

The rule under the header is the tab strip: `Controls`, `EQ`, `Telemetry` and `Devices`, the screen
showing between `┤ ├` with its name bold in the accent. Each other tab carries the key that shows it
as its first letter, bold (`c`, `e`, `t`, `d`); `Controls` is the device screen, which `Esc` leads
back to as well. A device with no tone control draws `EQ` faint, and the tab does nothing there, as
`e` does nothing. At the right end the strip names two keys, `? help` and `esc menu` on the device
screen, where `Esc` opens the menu, or `esc back` on the others, where it goes back to the device
screen. Every tab and both marks are clickable, and none is a `Tab` stop. The setup wizard keeps a
plain rule.

These keys work on every screen:

- `q` quit — asking first, in a box of its own over the screen: `Quit svolder? [Y/n]`. `Enter`, `y`
  or a second `q` quits (so `qq` is still one gesture), `Esc` or `n` stays; on a daemon's session
  the box adds `the daemon keeps the device connected`. With `confirm_quit = false` under `[tui]`,
  or the Settings box's **Confirm quit** row off, `q` quits at once, and the help overlay writes it
  `q quit` rather than `q quit…`
- Ctrl-C quit at once, from anywhere — over the help overlay, the device editor, the preset list, a
  band's edit line or the quit question included — without asking; it is the one Ctrl or Alt chord
  the TUI answers
- `r` refresh — read every value again
- `b` identify — ask the unit to show itself (function 200), on the Devices list the one under the
  cursor. The toast says `identify sent to NAME`, or `identify just sent to NAME; wait 2 s` for a
  second press inside two seconds, which sends nothing; a device that is down, or a daemon too old
  to know the request, is said in its own words. It says `sent`: nothing can see the light. On a
  device whose profile offers no identify (the Mesh Card) the key is dimmed, and the help overlay
  says `this device offers no identify`
- `m` mute, on a device whose profile has one
- `i` flip phase, on a device whose profile has a phase toggle
- `o` flip NOS, on a device whose profile has a NOS toggle
- `c` open the Controls screen, from every other screen
- `e` open the tone-control screen, on a device whose profile has one
- `t` open the telemetry screen
- `d` open the device list — where a device is added, changed and removed as well as switched to, so
  it is worth reaching whatever is configured
- `s` open the Settings box over the screen (see "Settings box" below)
- `M` toggle mouse capture (see "Mouse" below); a terminal that refuses leaves it as it was and says
  so in a toast
- `E` toggle the animation layer, `H` toggle hue drift, `R` toggle the rainbow scroll, `F` toggle
  the focus runner, `V` toggle the title meter (see "Title meter" below) — the same five switches
  the Settings box shows
- `a` with a fault standing toggles that fault's advice text, which the device screen's Status pane
  draws; with none standing it first shows the step the last failure named, once per failure, and
  then posts a six-second toast saying how long all has been clear
  (`All clear for 2 h 14 min · tip: V · toggle the title meter`; `Connected for …` on a profile with
  no fault controls), with a tip made from the key table and the tone screen's mouse lines, one more
  each press and wrapping, and every fifth press a quip for the device's family in its place
  (`Tubes warm, nothing to report.` where the Mode reads a tube). The help overlay names it under
  `Controls` as `a  advice, or all clear`, and the Status pane's border carries `advice` for it on
  every device
- `?` open the help overlay listing every key of every screen with what it does (see "Help, tabs and
  borders" below); `?` or `Esc` closes it
- `Esc` closes what is open, else goes back to the device screen, and on the device screen opens the
  menu (see "Menu (`Esc`)" below)

What a key, a click or the device came to is said in a toast: a small bordered box over the bottom
right corner of the screen, its bottom border on the terminal's last row and its right border two
cells from the right edge of the body (the terminal's own edge below 120 columns; in the wide tier,
the capped, centred body's). There are three kinds. An **Info** toast, with an accent border, stays
three seconds (`no player`, `switched to EQ`). A **Failure** toast, with a warn-coloured border,
reports a refusal, an error or a warning and stays six seconds (`settings not saved: …`,
`link lost; press q and reopen`, `band 2: f must be 82.0–312 Hz`).

A **Wait** toast, with a dim border, carries the spinner and stays as long as the wait does
(`⠋ opening Kraken…`). One toast shows at a time and a newer one replaces it. It is as wide as its
words plus four cells, at most 60% of the body's width; longer words wrap at spaces onto up to three
lines, the third ending in `…` if it still does not fit. It is drawn over everything, a box
included, at full strength and never animated. Keys go to the screen while a toast shows (`Esc` is
not taken by it), and a click on it dismisses it and reaches nothing under it.

### Device screen

Every writable control as a row — the selects and toggles first, then the levels, each group in the
profile's address order, and an amp's meters under them — with the Host group under them, the
tone-control summary and its five bands in a second pane on a device that has one, and a status pane
below with the firmware line, sample rate, active faults (a boolean readout such as `S/PDIF locked`
is drawn as its bare label while it reads true and as `S/PDIF unlocked` while it does not),
any raw reading no control names, where the level comes from and what the host is playing
(`↑↓ ←→ ␣/Enter +/=/- 0 m f p`), an 80×24 terminal on a Gungnir 2 that is also the host's default
sink:

```
 svolder  Gungnir 2 · fw 3 · 0102…08  ● connected                 mouse:on 14:02
─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─
┌ Controls ───────── mute ─ 0 centre ┐┌ Tone control ────────── flat ─ presets ┐
│ ▶ Input        ‹ 3 · Optical ›     ││   Mode      ‹ EQ ›                     │
│   Phase invert [ ] off             ││   Loudness  ░░░░░░░░░░ 0               │
│   NOS filter   [x] on              ││   Preset    custom                     │
│   Mute         [ ] off             ││                                        │
│                                    ││ Band   Freq      Gain      Q           │
│   Volume       █109/127┃░  -6.0 dB ││ ▶ 1   40.0 Hz    +0.0 dB   4.3         │
│                                    ││   2    160 Hz   -2.45 dB   2.1         │
│   Balance      L ────┃───── R  0   ││   3    632 Hz    +0.0 dB   4.3         │
│                                    ││   4   2515 Hz   +1.45 dB   1.0         │
│   Host                             ││   5  10000 Hz    +0.0 dB   4.3         │
│   Host volume  ███54%░░░░  Gungni… ││                                        │
│   Transport    ◀◀  ▶  ▮▮  ▶▶       ││                                        │
│   Host EQ      off                 ││                                        │
│                                    ││                                        │
└────────────────────────────────────┘└────────────────────────────────────────┘
┌ Status ────────────────────────────────────────────────────────────── advice ┐
│ Sample rate 48 kHz   fw 3                                                    │
│ Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                   │
│ Level from: host                                                             │
│ Now playing: So What — Miles Davis   mpv                                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

The tab strip under the header (its dashes and `┤ ├`), the `▶` cursor, and the key letters of a
border's words are all drawn in the accent the device's family draws in. At 100 columns and wider
the header grows to two rows — the small wordmark on the first, the device name and the rest of the
header sharing the second, the device name cut short with `…` where it would push the connection
words, the faults, `mouse:on` or the clock off the row — with the tab strip beneath both; the mockup
above, at 80 columns, is the one-row header most terminals show. The Host group and the `Level from`
and `Now playing` rows are the host's own audio (see "Host audio" below); with `host_audio = false`,
or `SVOLDER_HOST=off`, the screen is drawn without any of them.

A pane's top border carries the keys that act on what it shows, right-aligned after its title:
Controls has `mute`, `0 centre` and `refresh`, Tone control `flat` and `presets`, and Status
`advice`. A word's key is its bold first letter (or the key itself, as in `0 centre`); a word whose
key does nothing on this device is dim; a border too short for them all drops the last words, which
their keys still reach. A click on a word presses its key.

A terminal larger than 80×24 is laid out in one of two tiers on each axis rather than stretched.
**Wide**, from 120 columns, gives the Controls pane 50 columns and its bars up to 25 cells, sets
panes side by side a column apart, keeps two cells of blank inside every pane's border, and holds
everything below the header rule to 256 columns in the middle of the terminal — the header and its
rule keep the full width. **Tall**, from 40 rows, adds a blank row at the top of every pane, a row
between panes stacked one over the other (the device screen's panes and its Status pane, the
tone-control screen's Response and Bands), and draws a meter's bar on two rows, its label and
reading on the first.

Either tier also ends the Host group on its Spectrum row (see "Host audio" below), which the 80×24
layout leaves out. Anything short of both, up to 119×39, is drawn as the 80×24 mockups here show;
the mouse reaches whatever the tier drew, where it drew it. At any size, the Controls pane sets its
groups a blank row apart — the selects and toggles, the levels, the balance, the meters, and the
Host group — wherever it has the rows for them all and the blanks; a pane a row short of that packs
them, so no row goes out of sight.

A level row is one line, ending at least one blank cell short of the pane's border: its bar carries
the raw value over its range — `109/127` above — centred on the bar's own cells, each character over
the filled part drawn reversed on that cell's shade so it reads as a hole in the bar and each over
the empty part in plain text, with the converted value after the bar; a bar too short for the text
(a range running to seven digits) is drawn without it. On the focused row a bar or a track keeps its
own colours — the filled cells their accent shades, the empty cells and the balance track the
accent's darkest shade instead of the faint grey — while the cursor, the label and the value take
the selected row's inverse video, which runs on across the pane to its right edge; the loudness bar
does the same on the Tone control pane and in the tone-control screen's header.

With mouse capture on, a level's bar takes the mouse (see "Mouse" below): a click sets the value the
clicked cell stands for and a drag follows the pointer. The Volume does not jump up loudly, though:
a click or drag that raises it above 75 % of its range writes 75 % at once and then glides up toward
the target at a tenth of the range a second, so the top quarter takes about 2.5 s on a responsive
device, longer on a slow one, and the target's cell is drawn dim on the bar while it does. Any key
stops the glide where it is; `m` mutes at once, and a click or drag that lowers the volume is
instant.

While the tone control's curve boosts in EQ mode, the volume bar marks where clipping becomes
possible, the way Schiit's app marks its volume wedge red: the device does no headroom management of
its own, so a band's positive gain can clip digitally on a loud master, and lowering the DAC's own
volume is what prevents it. The zone starts past the loudest volume step whose attenuation (the
profile's `dac-attenuation-db` table) is at or below minus the curve's peak — the peak of the summed
curve the tone-control screen draws (each band at the step it is on its way to, or at the line's
preview while the edit line is open), found by sampling it at 1024 points from 20 Hz to 20 kHz and
searching round each band's centre, so two overlapping bands are measured where they add.

The threshold step's own cell — the last the bar fills at it — carries the tick `┃`, the glyph a
meter marks its class-A tick with, whether the bar has filled it or not: in the warn colour while
the volume is at or below the threshold, and in the alert red, bold, once it is past it. A cell of
the bar holds a dozen steps at 80 columns, so the first steps past the threshold share the tick's
cell and fill no red cell yet: the tick turning red is what says they are past it. Every cell the
bar only reaches past the threshold is the zone: filled in the alert red instead of the accent's
shade, and drawn with the medium shade `▓` instead of `█` beside an accent as red as the alert and
with colour off, so it reads as a pattern next to the bar's solid cells, empty in a dim red.

The raw value written over the bar keeps its own rule there, reversed on a filled red cell and plain
on an empty one, and moves aside for the tick where the bar has room for the whole value on one side
of it, giving way to it on the one cell they share where it has not: at volume 63 on a ten-cell bar,
with the tick on step 109's cell, the value moves a cell left, `█63/127┃░░`. The tick reads by its
shape where the colours do not: on the `red` palette or beside an accent as red as the alert, and
with colour off, where the zone's reds drop out but its pattern still shows and the tick's weight
says the volume is past it.

The mockup above has one: band 4's +1.4 dB puts the tick on the volume bar's tenth cell and the zone
on its last. With band 2 at +6 dB the curve peaks at +6.0 dB, step 109 (−6.02 dB) is the loudest
clear of it, and on an 80-column Controls pane at full volume the bar reads `██127/127┃██`, the tick
and the last two cells red. There is no tick and no zone in Flat mode, where no band applies, in
Loudness mode, whose boost is the device's own contour and has not been measured, or for a curve
that is flat or only cuts; at 16 colours the zone is the red and dim red of the terminal's own
palette and the tick its yellow, or its red once the volume is past the threshold. svolder never
lowers the volume by itself.

The Tone control pane's Mode and Loudness are two rows the cursor reaches like any other: `↑↓` moves
onto them from the bands, and `←→` steps the mode through flat, EQ and loudness, or nudges the
loudness strength, whichever row is focused. The Mode select's `‹ ›` arrows are in the accent
whether or not the row is focused.

Below the Loudness row, the pane names the preset the tone control is on, as the tone-control
screen's header does (see below): `Preset    NAME`, `NAME*` once the curve has moved off the one
loaded last, `custom` for a curve no preset matches, and `loading NAME` while a recall is still
being confirmed; no cursor reaches that line. In Flat and Loudness modes the pane's band rows are
dimmed and a line under them says `bands apply in EQ mode`, except the row the cursor is on, which
keeps its selected style rather than dimming; a band nudged from the pane in Flat mode switches the
tone control to EQ, as on the tone-control screen (below).

Below the writable controls, an amp's own pane also lists its `Meter` controls — not selectable,
since a meter is a reading rather than a setting. An amp's meters read on its own logarithmic scale,
and a bar draws it in two parts. The first is the class-A block, drawn `▓` in the faint grey over
the first part of the bar (a quarter of it on an Aegir 2, a tenth on the other amps), its last cell
the `┃` glyph; it is a fixed part of the bar, not a reading.

The second is the live part, the rest of the bar, which fills from the scale's zero (a reading at or
below it draws no fill) to its full scale, in steps that are small near the zero and large near the
top. After the bar comes the percentage of the live part that is filled, `0%` at the zero and `100%`
at full scale; before the first reading the bar is drawn empty with `—` in its place. A Tyr and a
Wotan send a floor reading along with their meters, and the zero sits half again above it, and at
least 10 above it; until a floor has arrived the zero is 0. A floor too high for the bar leaves it
empty at `0%`.

The floor readings set the zero and are not drawn in the Status pane; the Telemetry screen, `get`,
`watch` and the MCP server still list them. In the tall tier the bar takes two rows. The colours run
on the live part: at 16 colours the fill is yellow from 70% and red from 90%; at 256 colours and
truecolor it blends green through yellow to red along the live part. On the `red` palette the fill
is its off-white at every depth, and the peak marker is what says where it stands. `svolder get`,
`watch` and the MCP server keep showing the raw reading; the percentage is the TUI's, MQTT's and
Home Assistant's.

While the meters are not live, the bar is drawn dim with an empty live part and `off` in place of
the percentage, and the peak marker is gone. That is while the `Power meters` switch reads off,
while the amp is in standby, and while it is warming up. Only a reported state counts: an amp that
has not yet reported its meters' switch draws what it reads. A reading that arrives while the meters
are off is kept but gives no level, so the title meter, the effects and `Level from` follow the host
as with no meters.

A profile gives a `Meter` control one of two ways to read it. A `scale` table, on the amps' own
meters, has `full`, the raw reading at full scale, `class_a`, the block's part of the bar (0 up to,
not including, 1), and either `zero`, the raw reading at the live part's start, or `zero_from`, the
key of a `u16` readout of the same profile (a floor reading) from which the zero is worked out by
the rule above. `full` must be above a fixed `zero`, and above 0 otherwise. Alternatively a linear
meter gives a `range`, with two optional integer fields, `floor`, the raw value the meter reads at
idle, and `tick`, the raw value where the amplifier crosses into class-A operation.

Either field needs the `range` it is a position on: `tick` must sit inside it, `floor` must be below
its top, and a `tick` given with a `floor` must be at or above that floor, since a tick below it
would mark a spot the bar keeps empty. With a `floor`, the bar fills from that idle reading rather
than from `range`'s minimum, so a reading at or below the floor draws an empty bar; with a `tick`,
the `┃` glyph marks its position on the bar, and at 16 colours the bar is yellow at or above it (or
above 70% of full scale with none) and red at or above 90%. A linear meter draws the percentage too.
`scale`, `range`, `floor` and `tick` on a control of any other kind, `scale` together with `range`,
`floor` or `tick`, and a `scale` that breaks the rules above are profile errors. A `Meter` with
neither draws its bare number.

- `↑↓` move the selection, down past the device's own rows onto the Host group's
- `←→` change the focused control — step a select to its neighbouring value, nudge a level control's
  raw value by one, or flip a toggle — or, on the Host group, step the host's volume or move between
  the transport buttons
- `␣`/`Enter` flip a toggle at the focused row, or, on the Host group, turn the host's mute or press
  the transport button under the cursor
- `+`/`-` nudge volume up or down, from any row; `=`, the same key as `+` without Shift on most
  layouts, raises it too. Above 75 % of the Volume's range a key held down raises it no faster than
  a bar's glide, a tenth of the range a second: presses that come faster go nowhere. `→` on the
  Volume row is paced the same way
- `m` flip mute, from any row
- `i` flip phase and `o` flip NOS, from any row, on a device whose profile has the toggle; dimmed on
  one that has none, like `m`
- `0` centre the balance — set it to the value the profile names its centre — from any row, on a
  device whose profile has a balance; dimmed on one that has none
- `f` toggle the tone control's flat mode, from any row, on a device that has a Tone pane — the same
  key as on the tone-control screen (below); dimmed on a device with no tone control
- `p` open the preset list over the device screen: on a device that has a Tone pane the same list
  as the tone-control screen's (see "The preset list" below), on a Loki Max its own list of device
  slots and built-in settings (see "The Loki Max's preset list" below); dimmed on a device with
  neither
- `Tab` cycle the focused pane between Controls and Tone control, on a device that has one;
  Shift-Tab (`⇧Tab`) cycles the other way. The Status pane has no rows for a key to move through and
  is never focused: its `a` works from any pane

The Status pane's first line writes the readouts, then, on a device whose profile has a fault
control (an amp or preamp, not a DAC), `Faults:` and the active faults. A boolean readout is written
in both states, in the words the web page uses: its label while it reads true, and while it reads
false the label with `not connected` (a key ending `_connected`: `USB not connected`), `unlocked`
(`_locked`: `S/PDIF unlocked`) or `no` after it, dim. A pane too narrow to leave room for any fault
text writes neither the word nor the fault-presence pulse there; the header's fault mark carries the
faults.

Under its two lines the pane has a `Device` row once the backend has listed the device, in every
tier (the mockups in this guide are drawn with no backend behind them, so they have none; the
pictures in the README, taken against a daemon's device, do): the product with its id, the profile
the device is read through, and the signal strength, as
`Bifrost 3 (product 10) · profile bifrost3 · −62 dBm advert, 3 min ago`. The signal is the strength
of the advert the daemon heard before connecting: `advert` says it came from an advertisement, with
its age (BlueZ gives no strength for a connected device), and `live` would say it came from the link
itself. A device with no signal heard leaves it out. A row too wide for the pane drops what is on
its right whole, the signal first, then the profile, and cuts the product with `…` only when it
alone does not fit. A direct session has the strength of the advertisement it opened on, stamped
when it opened and again when a reconnect listens and hears it afresh; an advertisement the adapter
held from before, which no listen heard, has no strength listed. The row's facts, and the `Link`
row's flags below, are read when the device opens, whenever its link comes or goes or `refresh` is
pressed, and, while the Devices screen shows, with each of its 2-second refreshes; nothing asks the
daemon on a timer while the device screen itself is showing.

In the wide or tall tier, with `[tui] graphs` on, the Status pane draws a `Link` row under its two
lines: the connection strip, how the device's link has come and gone over `link_span` (12 hours
unless `[tui] link_span` says otherwise) and ending now, with the span named at its end. A dot at
the top is connected, one at the bottom is down (failed, waiting or reconnecting), and a stretch
before the history begins is blank. While the device has refused the passphrase on the link the
daemon holds, or the readings held are older than the link's last drop, the row also says so after
the span, in the warning colour: `connected · passphrase refused` or `disconnected · readings
stale`. The Devices list words them the same way.

A column is down if the link was down at any moment inside it, so a drop shorter than a column still
shows, as a meter's column keeps its peak. Through a daemon the strip starts from the history the
daemon keeps, up to a day of link changes that outlast the TUI; an older daemon that keeps none
leaves the strip to what this session has seen since it opened, which is all a direct session has.
The row's line is not drawn at 80×24, short of both tiers, or with `graphs = false`, which gives it
back to the panes above. At 80×40:

```
┌ Status ────────────────────────────────────────────────────────────── advice ┐
│                                                                              │
│ Sample rate 48 kHz   fw 3                                                    │
│ Raw: 9=u8 64   9.1=u8 0   9.2=u8 1   (3 unknown functions)                   │
│ Link  ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⣀⣀⣀⣀⣀⣀⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉  12 h│
└──────────────────────────────────────────────────────────────────────────────┘
```

In the tall tier, with `[tui] graphs` on, the Controls pane draws the level history under the
device's meters while any of them reports, else under the Host group while the host's capture runs:
the last `graph_span` of it, ending now. The `Level from` row reads `meters` while a scaled meter
streams, and `host` while the capture runs and the meters do not stream. The graph rows follow the
same source: while a scaled meter is not live (see the meter bars above) they draw the host's
history, or none when no capture runs, and a meter with no scale draws its own history while it
reports. Meters that report a left and a right (`L` and `R`) share two rows, the left filling up
from the midline and the right filling down, so a louder side reads as the taller; any other meter
takes a row of its own, labelled by its channel (`M`, or `1` to `5`).

The host's two channels are mirrored the same way, labelled `host`. Each column draws the loudest
reading inside it, so a short peak is never lost. A scaled meter graphs on its own fixed scale, the
zero at the bottom of the rows and full scale at the top, so a quiet passage looks quiet whatever
the window holds; each channel uses its own scale, and a floor reading that moves the zero moves the
whole history with it. A meter with no scale is scaled between its window's smallest and largest
value, with the channels on one shared scale. The 80×24 and 120×24 layouts have no room for it, and
`graphs = false` removes the rows.

```
│   L/R  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿ │
│        ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉ │
```

### Waiting for a device

While the daemon holds the device and has not found it yet, the device screen shows one frame in
place of the Controls and Status panes, with the three lines below and the same `refresh` word on
its border, since `r` tries to open the device again. `since` is the time this session began
waiting, as `HH:MM`, and no control of a placeholder profile is drawn. The header's spinner turns
while the wait lasts. An 80×24 terminal waiting for the device `amp`:

```
 svolder  amp  ○ disconnected                                     mouse:on 14:02
─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─
┌ Waiting ──────────────────────────────────────────────────────────── refresh ┐
│ amp not found yet                                                            │
│ svolderd keeps looking (since 14:02).                                        │
│ Is it on and in range?                                                       │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

The TUI listens for the daemon's events for that device; on the first one that says its link is up
it opens the device in full, as a reconnect does, so the real profile, the readings and the link
history replace the panel without a key. The Devices screen's 2-second relist keeps running
meanwhile.

### Host audio

With `[tui] host_audio` on, the default, the TUI reaches the host's own audio as well as the device:
what the host's media player plays, the volume of the host's default sink, the player's transport,
and what the sink plays — captured from its monitor as a spectrum and a level. The device screen's
mockup above shows all but the spectrum, which the 80×24 layout leaves out.

- **`Now playing`**, the last of the Status pane's host lines: the followed player's track as
  `Title — Artist`, and the player's own name dim after it where there is room. Words longer than
  the row scroll along it a cell every 250 ms, pausing a second at each end, for as long as they are
  too long, and start again with each new track. With a session bus but no player on it the row
  reads `—`, dim; with no session bus it is not drawn at all.

  The players are the session bus's `org.mpris.MediaPlayer2.*` names, and the one followed is the
  one playing (the one that started last, when several are), else the one that played last, else the
  first by name, chosen again whenever a player joins or leaves the bus or says its playback status
  changed, never by polling. The bus is `DBUS_SESSION_BUS_ADDRESS`, else `$XDG_RUNTIME_DIR/bus`; it
  is looked for the first time host audio is on, and again every 10 s once a bus that was found has
  gone away or one that took the connection has not answered yet (not while host audio is off: the
  look that came due is made the moment it is on again), until it answers, whatever the looks in
  between find; svolder never starts one.
  On a TUI joined to a daemon (the usual case: `svolder tui` finds the running daemon), the daemon
  reads the players, not the TUI: it follows the player once for every front end, so this row, the
  transport and the web page's Now playing card always agree. A daemon whose host side is off,
  or that is too old to have one (`SVOLDER_HOST=off` in its environment, `[host] enabled = false`,
  no session bus where it runs), leaves the TUI on its own watcher, exactly as in a direct session;
  `host_audio = false` turns it all off whatever the daemon says, and the Settings box's **Host
  audio** row does so at once: off, the TUI stops asking the daemon for the host (which then stops
  reading the computer's audio graph for it); on, it asks again and shows the daemon's view as it
  stands then. When the daemon goes away the TUI's own watcher takes over until a daemon that
  speaks for the host is back. The host volume, the computer EQ and the capture are always the
  TUI's own, on the computer it runs on.
- **The Host group**, at the bottom of the Controls pane under a `Host` heading of its own, three
  rows the Controls pane's cursor reaches under the device's own, and rows it does not reach (the
  Spectrum row in the wide and tall tiers, the Path row on a DAC joined to a daemon):
  - `Host volume`: the host's default sink, whichever device that is, as a bar from 0 to 100 % with
    the volume written inside it, `muted` while it is, and the sink's own name (its
    `node.description`) dim after them, cut to what the row leaves — so a default sink other than
    the DAC is named, and the row says which one it changes. `←→`, and the wheel over the bar, step
    it 2 %, never past 100 %: a sink something else boosted comes down to 100 % on a step down and
    goes no higher. `␣`/`Enter` turn its mute.

    The sink is read with `wpctl get-volume @DEFAULT_AUDIO_SINK@` once a second while the device
    screen shows, and at once after every change, and named with `wpctl inspect` only when it may
    have changed: on the first read, after a read that failed, when its volume or its mute moved,
    and once the name it holds is five seconds old, which catches a new default sink at the same
    volume; each call is given a second before it is given up on, and none of them holds up a frame
    or a key. Before the first read, and on a host with no `wpctl`, the row reads `—`, dim.
  - `Transport`: `◀◀  ▶  ▮▮  ▶▶` — previous, play, pause and next. `←→` move between the four,
    `␣`/`Enter` press the one under the cursor, which the focused row draws out of its highlight,
    and a click presses the one clicked. `▶` is drawn in the accent, bold, while the player plays,
    and `▮▮` while it is paused. With no player the row is dim, and a press sends nothing and says
    `no player` in a toast.
  - `Path`, on a DAC joined to a daemon that speaks for the host, under `Transport`: where the sound
    goes from the player to the DAC, which no key or click reaches. The row is there for as long as
    the daemon speaks for the host, so the group does not change height as playback starts and
    stops: with no player it reads `nothing playing`, and with a player the daemon has not read the
    route for yet, `route not read`, both dim.
    The verdict comes first in its own colour — `untouched` (green), what changes the sound (amber:
    `resampled 44.1 → 48 kHz`, `computer volume 54 %`, `player muted`, `host EQ on`,
    `2 other sounds mixing`, `DAC reads 44.1 kHz`), or, dim, `the computer plays to NAME` when the
    default sink is not this DAC, `the DAC listens to Optical` when the DAC is on an input the
    computer does not feed (`the DAC listens to USB 1 · the computer is on USB 2` for a DAC on one
    USB port while the computer is on the other), or what could not be read (`DAC input not read`,
    `both USB ports connected`); several words are
    joined by ` · `, as on the web page — then each hop from the player to the DAC with what is
    known of the signal there (`player 44.1 kHz S16LE, volume 100 % → computer 48 kHz … → DAC
    USB 1, 48 kHz`), a hop that changes the sound in amber. The row is as wide as
    the pane, and is cut with `…` where the words run out of room, so the verdict is what shows
    first. svolder cannot see the bits, so it never says more than the rates, volumes, mixing and
    EQ show. An amp or a preamp has no row. The daemon reads the route (`pw-dump`, every 2 s) only
    while a TUI or a web page that shows it is joined.
  - `Host EQ`: the computer EQ that `h` in the preset list and `svolder host eq apply` put in front
    of the host's default sink (see "The preset list" below). It reads `NAME · on` while the
    computer EQ is on, with the preset's name cut to the row, and `off`, dim, while it is not.
    `␣`/`Enter` take it off when it is on; when it is off they put the last one back, built from the
    filters its state file kept, so no preset needs to be in the list, and with none to put back
    they say `p then h on a preset` in a toast.

    They run off the UI loop as `h` does, so a second press while one is still changing says
    `the computer EQ is still changing`; a computer where the EQ cannot run says why instead. The
    TUI looks at the state file's modification time once a second, so an apply or a remove from the
    CLI shows without a key. In the wide tier the header also names it while it is on, as
    `Host EQ NAME`.
  - `Spectrum`, in the wide and tall tiers only and while `visualizer` is on: what the default sink
    plays, in its sixteen bands from 40 Hz to 16 kHz spread over the pane's width to the edge the
    Volume row reaches (between neighbouring bands, on the same log axis; a pane with no more room
    than sixteen cells draws the lowest bands as they are), each cell a block glyph (`▁▂▃▄▅▆▇█`)
    as tall as the band is loud, `▁` for silence, as `Spectrum     ▁▃▅▇█▆▅▃▂▂▁▁▁▁▁▁`, coloured
    from red at the bass to violet at the treble where the theme has a spectrum. Each block the
    capture reads, every 21⅓ ms, takes a band straight up to the block's own value where that is
    higher than the band stands; a quieter block does not drop the band to its value but leaves it
    falling back at its one rate, the whole scale in 750 ms, so the row settles once the music
    stops. With no capture running the row reads `—`, dim. It is a reading, as a meter's row is:
    neither the keys nor the mouse reach it.
- **`Level from`**, the Status pane's line above `Now playing`: where the level the glow, the
  breathing borders, the focus runner and the title meter follow comes from — `meters` while the
  device's scaled meters stream (one of them has sent a reading within the last ten seconds — the
  values a refresh or a reconnect reads the device for do not count), else `host` while the capture
  of the sink's monitor runs, else `—`, dim. It is one source at a time, never a mix of the two: an
  amp whose meters stream keeps their level, and a DAC, which streams none, takes the host's.

  The host's level is the louder of the capture's two channels, smoothed against time at the pace
  the meters' level moves at (a meter reporting ten times a second) however many blocks it arrives
  in, and while it drives the breathing borders or the hue drift the screen is redrawn ten times a
  second, the pace an amp's meter readings redraw it at, so the two breathe alike on a DAC and on an
  amp; and the title meter draws its left and right channels as its facing pair (see "Title meter"
  below).
- **`Passphrase`**, the Status pane's last line while it has something to say, host audio or not:
  that the passphrase the entry configures `does not match the code the device advertises`,
  `is none, but the device advertises a code`, or `is a code, but the device advertises none`, in
  the warning colour. It is the best-effort note "Configuration" describes, so it never says which
  code is wrong and never shows a digit, and it goes away when the entry is corrected or the device
  shows the code works. The TUI asks for it when a device opens, whenever the link comes or goes, on
  `r`, and after it writes the configuration.

The spectrum and the host level come from `pw-record --raw --format f32 --rate 48000 --channels 2 -P
'{ stream.capture.sink = true node.passive = true node.rate = 0 }' --target <sink> -`, where
`<sink>` is the default sink's own node, its `node.name` as `wpctl inspect` gives it: the stream
property has PipeWire link the capture to that sink's monitor, so it only ever records what the sink
plays and never a microphone, and without a sink named it does not start at all; the stream is
passive, so it does not keep the sink awake, and it asks for no rate of its own, so it does not pin
the graph's. The samples stay in memory only as long as it takes to turn each block of 1024 frames
into its sixteen bands and two levels; nothing is written, kept or sent.

Joined to a daemon whose host side is available (`svolderd` with `[host]` on and its capture
possible), the TUI does not record the host itself: it asks the daemon for its analysed frames over
a connection of its own, which the daemon holds its one capture for, and draws the Spectrum row,
the level and the beats from them (the frames' 240 bands taken down to the same sixteen). That
connection is open only while the same need holds as for the TUI's own capture, and is let go with
it; the TUI's own `pw-record` keeps feeding the row until the daemon's first frame lands, then
stops. A daemon that cannot give frames (one
from before them, a host with no capture tool or `[host]` off) leaves the TUI recording for itself
as it always has, and so does one whose capture stops: the TUI asks again after half a minute, and
after five seconds if the connection broke. A beat in the frames pulses what is drawn for a moment
(see "Breathing borders" below): the Spectrum row's bars go bold and the borders' level lifts. A
session on no daemon (`--transport` direct) never asks.

The capture runs only while something needs it: the Spectrum row on the screen — the device screen,
in the wide or tall tier, the visualizer on, the row inside the Controls pane and no box open over
it — or, while no meter streams, something following the level: the title meter, the breathing
borders, or, with the animation layer running, the hue drift or the focus runner. It is stopped, its
`pw-record` ended, the moment none of that holds, when host audio is turned off, while the
add-device wizard runs or a device switch opens the next device (the first frame drawn after it
starts it again where it is still wanted), and when the TUI quits; one that cannot start, or stops
by itself, is left alone for five seconds before it is tried again. `SVOLDER_PW_RECORD` names the
`pw-record` to run in place of the one on `PATH` (see
["Environment variables"](environment.md#environment-variables)).

Nothing reaches the host but a key, a click or the wheel on the Host group: nothing is sent when the
TUI starts, when a row is focused, or on a timer, the once-a-second read only reads, and the capture
only listens. There is no global key and no media key for any of it; the Controls pane's own keys
are the ones its border words and the `?` overlay name. `host_audio = false`, or the Settings box's
**Host audio** row off, draws none of it and reaches nothing of the host; `SVOLDER_HOST=off` in the
environment does the same for that one process whatever the file says, and the Settings row then
reads `on (SVOLDER_HOST=off)`. `SVOLDER_WPCTL` names the `wpctl` to run in place of the one on
`PATH` (see ["Environment variables"](environment.md#environment-variables)).

### Tone-control screen (`e`)

The five parametric bands, the tone-control mode and loudness strength, and the preset the device
carries, on a device whose profile has a tone control
(`↑↓ ←→ Tab/⇧Tab 1-5 , . ⇧←→ [ ] ⇧↑↓ Enter 0 f p`):

```
 svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│
│    │                                                                         │
│    │                                                                         │
│ -6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│-12 │                                                                         │
│    └─────────────────────────────────────────────────────────────────────────│
│    20        50     100    200       500    1k     2k        5k           20k│
└──────────────────────────────────────────────────────────────────────────────┘
┌ Bands ──────────────────────────────────────────────────────────── 0 flatten ┐
│   #   Freq        Gain       Q        raw (f/g/q)                            │
│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   2    160 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

The Response chart's top border carries `flat`, `presets` and `refresh`, and the Bands block's
`0 flatten`, as the device screen's panes carry theirs; a click on a word presses its key.

This screen follows the same rule the device screen's Tone pane does: `↑↓` move between Mode,
Loudness and the bands, and `←→` change whichever of them the cursor is on — the mode through flat,
EQ and loudness, the loudness strength by one step, a band's gain by one of the device's own steps.
The header has no cursor column, so the row the arrows are on is picked out by colour instead of
`▶`, its whole half — label and value — in the selected style with the Mode select's `‹ ›` arrows in
the accent, which they are at every rest too. `Tab` (Shift-Tab the other way) moves between the
header and the band table, the header opening on whichever half the cursor was last on — Mode the
first time — and the band table on the band it was on; with only the two stops, either direction
goes to the other one.

A band's gain moves one raw step a press, whatever size the profile's steps are: about 0.2 dB on a
Gungnir 2, whose gain runs from -20 dB at step 0 through 0 dB at step 98 to +6 dB at step 127, so
`←` from 0 dB reads `-0.20` and `→` from there reads `+0.0` again. The key sends the value its step
stands for, so the device lands on that step and no neighbour of it, and past either end of the
range it does nothing; the wheel over a band's row, and `←→` on the device screen's Tone pane, step
it the same way.

Until the device has answered a write, the band keys — `←→`, the wheel, `,` `.` `[` `]` — step that
parameter from the value last asked for rather than from the reading, so two presses before the
first echo move two steps and a key held down moves at its own repeat rate rather than the device's;
the device answering the last write, a refresh (`r`), a failed write, or 5 s passing since the step
was asked for with no answer ends that, the keys then stepping from the reading again.

The same holds for a preset's load, whose writes — each band parameter, the mode and the loudness
strength it sends — count as asked for as it is sent: a band key, `0`, the edit line and another
load used while the device is still answering it work from the curve the load asked for, so `←` on
band 1 just after loading Flat over Bass lands one step below 0 dB, not one step below the +4.0 dB
band 1 still reads, and a band edit switches the mode to EQ as the mode the load asked for says (see
"What the graph draws" below).

The Gain column reads to the hundredth of a dB a step lands on (`-0.20`, `+1.45`), a whole number of
dB to the tenth (`+0.0`, `+6.0`), and the device screen's Tone pane reads a band's gain and
frequency the same way, as does a preset recall's clamp text
(`band 1 frequency 10 Hz clamped to 20.7 Hz`). The Freq column reads a frequency below 100 Hz to the
tenth of a hertz, where one of a band's steps is less than a hertz (`40.0`, `40.4`), and from 100 Hz
up in whole hertz, so two neighbouring steps never read the same, as `svolder eq show` writes them.

The header names the preset the device carries: `Preset Flat` above, the device being in EQ mode
with every band on its centre at 0 dB. A preset is on when the device's mode is the preset's and
every band reads the steps the preset's values convert to on this device — a band at 0 dB matching
whatever its frequency and Q — and, for a preset in Loudness mode alone, the loudness strength is
the preset's too. After a change from the preset loaded last the header reads `Preset NAME*`, until
the curve matches it again or another preset is loaded; with no preset matching and none loaded it
reads `Preset custom`; and while a load's writes are on their way it reads `loading NAME`, until the
device answers the last of them, or for 5 s at most, when the load's steps give up with it.

The name loaded is told to the daemon, which keeps it for every front end while it runs and writes
it nowhere (see ["Presets"](eq.md#presets)). A name longer than the room the rest of the header
leaves it is cut short with `…`, its `*` kept. How much room that is depends on the mode the header
names beside it: at 80 columns a Gungnir 2's header has room for ten characters of the name in EQ
mode, eight in Flat mode and four in Loudness mode, the `*` among them. A wider header has more room
for it, and a preset's name is at most 40 characters.

- `↑↓` move between Mode, Loudness and the bands
- `←→` change the row the cursor is on: step the mode, nudge the loudness, or move the focused
  band's gain one step
- `Tab`/`⇧Tab` move between the header and the band table, either direction, opening the header on
  the half it was last on and the bands on the band they were on
- `1`-`N` pick a band by number, `N` being how many bands the profile has — the help overlay reads
  `1-5` on a five-band tone control and `1-3` on a three-band one, and a digit past `N` picks
  nothing
- `,`/`.` step the focused band's frequency down or up by a semitone; Shift+`←`/Shift+`→` do the
  same
- `[`/`]` step the focused band's Q down or up — `[` widens the bell, `]` narrows it; Shift+`↑` is
  `[` and Shift+`↓` is `]`, for keyboard layouts that reach the brackets only through AltGr. Some
  terminals (Konsole, by default) scroll their own history on Shift+`↑`/`↓` rather than passing it
  on, and `[ ]` stay for those
- `Enter` open the focused band's edit line, to type its frequency, gain and Q (see "The edit line"
  below); nothing on the Mode and Loudness rows. A double-click on one of a band's values opens the
  line on that value
- `0` flatten the curve: put every band back on its centre frequency and centre Q at 0 dB, the
  values of the Flat preset, so the header reads `Preset Flat`. Only the steps that differ are
  written, the boosts brought down before the frequencies and Q move and the cuts come up. In Flat
  mode it also switches the tone control to EQ, after the bands, so the flat curve is what plays
  (see below); in Loudness mode it leaves the mode alone, and on a curve already on Flat's values it
  writes nothing at all. It is not the flat mode `f` toggles
- `f` toggle the tone control's flat mode — the device's own bypass, as the Mode row's `flat`: the
  bands keep their frequency, gain and Q, so stepping the mode back to EQ brings them back. Pressed
  again from Flat it asks for the mode before — the last non-Flat mode this device session saw or
  asked for (a reading, a Mode-select step, a preset recall, `f` itself, or a band edit's switch to
  EQ), or EQ with none of those yet. It is not a gain of 0 dB on every band, which is `0`
- `p` open the preset list (below)
- `m` mute, on a device whose profile has one
- `i` flip phase, on a device whose profile has a phase toggle
- `o` flip NOS, on a device whose profile has a NOS toggle

A band edit made while the tone control is in Flat mode — `←→` on a band, `,` `.` `[` `]` and their
Shift+arrow twins, the edit line's `Enter`, `0`, the wheel over a band's row or over the curve, or a
drag of a band's point on the curve (see "Mouse" below) — also writes the mode to EQ, after the
band, so the change is heard, and a toast says `switched to EQ`. In Loudness mode a band edit leaves
the mode alone, until it is known whether the bands apply there. A preset's recall writes its own
mode instead.

#### What the graph draws

The Response graph draws what the device applies in EQ mode, and — since nothing is applied outside
it — the curve the bands would shape in Flat mode and a line of text in Loudness mode instead, which
depends on the mode as much as on the bands. The curve and its points draw each band at the step it
is on its way to — the one a band key, a drag of its point or a preset's load has asked for, before
the device has answered — so a point follows the keys and the pointer as they move it, while the
band table reads what the device confirmed. While a band's edit line is open the band is drawn at
the steps the line's fields hold, typed or stepped, before anything is written (see "The edit line"
below):

- **EQ mode, a band off 0 dB:** the summed curve, coloured as a spectrum by frequency, each band's
  centre marked on it and the focused band's in a colour of its own.
- **EQ mode, every band at 0 dB** (a flat curve, as `0` or the Flat preset leaves it): the flat
  line, muted — it IS the applied curve. Muted keeps its hue but desaturates it toward its own grey
  and darkens it a little, at true colour and 256 colours, rather than the flat dim grey; at 16
  colours, where there is no spectrum to draw in at all, it is the flat dim grey. The rainbow scroll
  never moves it: it rests red to violet (on `red`, dark red to off-white) whatever the drift is
  doing.
- **Flat mode** (the device's bypass, `f`): nothing is applied, so the graph draws the stored curve
  instead — what the bands would shape if the bypass were not standing between them and what plays —
  muted the same way, with their points on it; a boosted band's peak rises above the 0 dB row the
  way it would in EQ mode, so the points are never left floating apart from a flat line that is not
  theirs. The band table is dimmed and its bottom border says `bands apply in EQ mode`, except the
  row the cursor is on, which keeps its selected style rather than dimming.
- **Loudness mode:** no curve and no points, since the device's loudness contour is not known until
  it is measured; a line on the 0 dB row says what the DAC's own volume leaves it —
  `no effect at full volume` at the top step, and below it
  `loudness active — the boost grows as the volume goes down · strength 40`, with the strength the
  contour is set to. The band table is dimmed as in Flat mode, except the row the cursor is on.

A Gungnir 2 in Flat mode with band 2 kept at +6 dB — the stored curve rises for it even though Flat
mode applies none of it — on an 80×24 terminal:

```
 svolder  Gungnir 2  Preset custom    Mode ‹ Flat ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │                     ⡰•                                                  │
│    │                    ⢠⠃⠈⡆                                                 │
│    │                  ⢀⡠⠊  ⠘⢄⡀                                               │
│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠉⠉⠁      ⠈⠉⠉⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│
│    │                                                                         │
│    │                                                                         │
│ -6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│-12 │                                                                         │
│    └─────────────────────────────────────────────────────────────────────────│
│    20        50     100    200       500    1k     2k        5k           20k│
└──────────────────────────────────────────────────────────────────────────────┘
┌ Bands ──────────────────────────────────────────────────────────── 0 flatten ┐
│   #   Freq        Gain       Q        raw (f/g/q)                            │
│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   2    160 Hz    +6.0 dB    4.30      64 / 127 / 64                          │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└ bands apply in EQ mode ──────────────────────────────────────────────────────┘
```

While the curve boosts in EQ mode, the band table's bottom border carries a note in the dim grey
saying how far it peaks and how far down that keeps the volume — the same peak, and the same test
for whether one is due, as the volume bar's zone, and as `svolder eq show`'s `peak` line once the
device has confirmed the bands; while a write is pending, or the edit line previews a value, it is
the peak of the curve the graph draws: `peaks at +4.2 dB — keep the volume at or below -4.2 dB to
avoid clipping on loud masters`.

A border too short for the whole of it, as at 80 columns, leaves off the last three words: `└ peaks
at +6.0 dB — keep the volume at or below -6.0 dB to avoid clipping ────┘`. The note is gone once the
curve is flat or only cuts — or peaks by less than 0.05 dB, which would read as `+0.0` — and in Flat
and Loudness modes, where the border says `bands apply in EQ mode` instead.

#### An imported preset on the graph

While the preset on the device, or the one loaded with changes (`NAME*`), is an imported one (see
"The AutoEq search" below), the Response graph draws it against what it was fitted from:

- the **target**, the imported correction's own curve — its peaking filters and shelves summed,
  without the preamp — as a dotted line in the dim role, in quadrant cells (`▖▗▘▝`) rather than the
  Braille the fit is drawn in, so the spectrum colouring never recolours it;
- the **fit**, the summed curve of the bands exactly as above, with its handles;
- the **gap**, the cells between the two curves filled with the light shade `░` in the theme's
  warning colour, drawn under both, in every column where the fit misses the target by more than 2
  dB — and in monochrome, dim. The gap is left out while the curve is muted (Flat mode, or a flat
  curve), which is not what the device applies; values past the graph's -12 to +6 dB clip as the
  curve does.

The header adds `fit 0.9 dB`, the RMS error of the fit over 20 Hz to 10 kHz, after the preset's name
where the line has room for it whole; at 80 columns it usually has not. A band preset draws none of
this. A Gungnir 2 with band 2 lifted to +6 dB after loading an imported preset with a +5 dB bass
shelf and a -3 dB notch at 3 kHz, on an 80×24 terminal:

```
 svolder  Gungnir 2  Preset Phones*     Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │▖▖▖▖▖▖▖▖▖▖▖          ⡰•                                                  │
│    │░░░░░░░░░░░▘▘▘▘▖▖   ⢠⠃⠈⡆                                                 │
│    │░░░░░░░░░░░░░░░░░▘⢀⡠⠊▖░⠘⢄⡀                                               │
│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠉⠉⠁   ▘▘▘⠈⠉⠉⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│
│    │                                                 ▝▝▗▗▗▝▝                 │
│    │                                                                         │
│ -6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│-12 │                                                                         │
│    └─────────────────────────────────────────────────────────────────────────│
│    20        50     100    200       500    1k     2k        5k           20k│
└──────────────────────────────────────────────────────────────────────────────┘
┌ Bands ──────────────────────────────────────────────────────────── 0 flatten ┐
│   #   Freq        Gain       Q        raw (f/g/q)                            │
│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   2    160 Hz    +6.0 dB    4.30      64 / 127 / 64                          │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└ peaks at +6.0 dB — keep the volume at or below -6.0 dB to avoid clipping ────┘
```

In the wide tier (from 120 columns) a side pane, `Correction`, stands to the right of the graph: the
correction's source (the `origin` the file records, else `Imported`) and how many filters it has,
one row per filter — its type, frequency, gain and Q, as `LSC   105  +5.0 0.70` — then the preamp
and the fit line (`fit 0.1 dB RMS · worst 0.7 dB at 20.0 Hz`, with `lower volume` after it where the
fitted bands boost). Filters the pane has no room for end in `… N more`. The pane takes its columns
from the graph, so the graph is that much narrower there. Its top:

```
┌ Correction ────────────────────┐
│ Imported · 2 filters           │
│ LSC   105  +5.0 0.70           │
│ PK   3000  -3.0 2.00           │
│ Preamp -3.0 dB                 │
│                                │
│ fit 0.1 dB RMS · worst 0.7 dB  │
│ at 20.0 Hz · lower volume 5.2  │
│ dB                             │
```

The target, the fit and their strings are worked out once, when the preset or the presets file
changes, and kept; a frame only reads them.

#### The edit line (`Enter`)

`Enter` on a band's row opens a line in the row's place for typing the band's frequency, gain and Q
as numbers — `f 160 Hz  g +0.0 dB  q 4.30` — each field holding what the row showed; a double-click
on one of the row's values opens it with that value's field focused (see "Mouse" below). Everything
around it is dimmed as it is around the preset list, and the line names its own keys,
`Tab ←→ ↑↓ · ↵ writes · Esc cancels` on an 80-column row; where the row has room it also says what
each of them does, `Tab/⇧Tab ←→ moves · ↑↓ steps · Enter writes · Esc cancels`. An 80×24 terminal on
a Gungnir 2, band 2's gain being typed as `1.5`, the graph already showing the curve it makes:

```
 svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │                                                                         │
│    │                                                                         │
│    │                     ⡠•⡀                                                 │
│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠊⠉  ⠈⠉⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│
│    │                                                                         │
│    │                                                                         │
│ -6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│-12 │                                                                         │
│    └─────────────────────────────────────────────────────────────────────────│
│    20        50     100    200       500    1k     2k        5k           20k│
└──────────────────────────────────────────────────────────────────────────────┘
┌ Bands ──────────────────────────────────────────────────────────── 0 flatten ┐
│   #   Freq        Gain       Q        raw (f/g/q)                            │
│   1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│ ▶ 2 f 160 Hz     g 1.5 dB     q 4.30      Tab ←→ ↑↓ · ↵ writes · Esc cancels │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└ peaks at +1.4 dB — keep the volume at or below -1.4 dB to avoid clipping ────┘
```

- `Tab`/`⇧Tab`, and `→`/`←`, move to the next or the previous field, round the ends
- `↑`/`↓` step the focused field one of the device's own steps up or down — a gain about 0.2 dB on a
  Gungnir 2, a frequency and a Q one raw step — the field then reading the new step as the band's
  row does, ready to be typed over: `↑` from `-0.20` reads `+0.0`, and `Enter` then writes 0 dB
  exactly. A number typed in is stepped from the step nearest it; at either end of the steps the
  arrow does nothing, and a field that is no number the band takes is refused as `Enter` would
  refuse it
- the digits, `-`, `+` and one decimal separator — `.` or `,`, whichever the keyboard's keypad
  sends: `0,5` is `0.5` — type a number: the first character typed into a field replaces what it
  shows — the field the cursor lands on is drawn selected until then, and in the accent once it is
  being typed into, the terminal's cursor after its text — and `Backspace` takes the last character
  off instead, to edit the value shown. Any other character, a second separator, or a thirteenth
  character is refused on the line (`only digits, one . or , and a sign`,
  `a number takes at most 12 characters`) and not typed. A longer number than the field shows
  scrolls inside it with the cursor
- `Enter` write every field typed into or stepped, each at the device step nearest its number — or
  the very step `↑↓` put it on — and close the line; where every number typed lands on the step the
  band is on already, nothing is written and the line says so — `already at +0.0 dB`, in the notice
  colour — until the next key, the next `Enter` then closing it
- `Esc` close the line with nothing written, the graph back as it was

While the line is open the Response graph follows its values: the curve, the band's point on it and
the peak note under the band table are drawn as the line's fields would leave the band, and the
volume bar's clipping zone and tick are worked out from the same steps. A field is drawn at the
device step `Enter` would write for it, so a number typed in previews at the step nearest it; while
a field's text is refused or incomplete (`-`, empty, out of range) the graph keeps the last step
that field previewed, and a field left as it opened is drawn at the band's own step, so a report for
the band that arrives meanwhile moves it.

Only the graph previews: the band table keeps showing what the device confirmed, and nothing is sent
before `Enter`. `Esc` puts the graph back as it was, and on `Enter` it does not jump, the step
written being the one previewed. In Flat mode the muted curve moves the same way; Loudness mode
draws no curve to move.

A field left as it opened is not written, so a value the row rounds for showing stays where it was
when only the field beside it is typed, and a number that lands on the step the band is on already
writes nothing. The row then reads the step the device took: `1.5` dB is step 105, `+1.45`. The
writes go in the order a preset's recall writes a band in, from the same planner: a gain coming down
before the frequency and Q and one going up after them — a gain the device has not reported yet
counting as coming down to a cut and going up to a boost — so the band never passes through a louder
curve than either end of the edit; in Flat mode the writes also switch the tone control to EQ, as
every band edit does (see above).

A field that is not a number, or lies outside what the band takes, is refused with the range it does
take — `f must be 82.0–312 Hz`, `g must be -20.0–+6.0 dB`, `q must be 0.50–8.00` on band 2 — and
nothing is written: the line stays up on that field, with what was typed still in it, ready to be
typed over. The refusal stands on the line itself, in the alert red where its keys were and at full
strength over the dimmed screen, until the next key brings the keys back; a Failure toast says it
too, with the band, `band 2: f must be 82.0–312 Hz`. Band 2 refusing `5` Hz:

```
│ ▶ 2 f 5 Hz       g +0.0 dB    q 4.30                   f must be 82.0–312 Hz │
```

A frequency is taken anywhere from the band's lowest step to its highest as the refusal writes them,
a number between a written end and the step itself going to that step. While the device is still
answering a preset's load or a band key, the line opens on the steps they asked for and compares
what is typed with those, as the band keys step from them: typing the gain the load asked for says
`already at`, and typing the gain the row still reads writes it. Every other key that is no
character — `Home`, `End` and the rest — is swallowed while the line is open, and the mouse reaches
nothing under it: a left click outside the line presses `Esc`, which cancels the edit.

#### The preset list (`p`)

`p` opens a box over the tone-control screen, or over the device screen, listing the presets that
fit the device's tone control: the four built-ins first — Flat, Warm, Bass and Presence, each marked
`built-in` and built on this tone control's own band centres — then the configuration file's own
`[[preset]]` blocks with the same band count and every imported one, in the file's order. The one
that is on is marked `●`, and the list opens on it. The box names its own keys, and everything
around it is dimmed as it is around the device editor; an 80×24 terminal on a Gungnir 2 whose
configuration file holds one preset, `HD650 warm`:

```
 svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│
│    │  ┌ Presets · 5 bands ──────────────────────────────────────────┐        │
│    │  │ ▶ ● Flat        built-in                                    │        │
│ -6 │  │     Warm        built-in                                    │        │
│    │  │     Bass        built-in                                    │        │
│    │  │     Presence    built-in                                    │        │
│-12 │  │     HD650 warm                                              │        │
│    └──│                                                             │────────│
│    20 │ Enter loads · n saves new · Esc closes                      │     20k│
└───────│ u updates · c renames · x deletes · built-ins are read-only │────────┘
┌ Bands │ a AutoEq · i from file · h computer                         │flatten ┐
│   #   └─────────────────────────────────────────────────────────────┘        │
│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   2    160 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

- `↑↓` move between the presets; a list longer than the box shows the rows round the focused one and
  says `more ↑` or `more ↓` on the border the rest are past
- `Enter` load the focused preset and close the list: the writes a recall needs and only those, sent
  the way every key's writes are — the gains coming down first, then the frequencies and Q, then the
  gains going up, and the mode last when it becomes EQ, first otherwise. A value outside this
  device's band window or range goes to the nearest step, said in a Failure toast
  (`band 5 frequency 22000 Hz clamped to 19497 Hz`)
- `n` save the device's current curve — its mode, its loudness strength, and every band's frequency,
  gain and Q — as a new preset: a one-line prompt asks for its name, prefilled with the first
  `custom N` the file does not have yet; `Enter` saves and `Esc` goes back to the list
- `u` write the device's current curve over the focused preset, after asking
  `Update NAME with the curve on the device? [y/N]`; an imported preset refuses it with
  `NAME is imported: save the curve under a new name`
- `c` rename the focused preset, through the same prompt prefilled with its name
- `x` delete the focused preset, after asking `Delete NAME from the configuration? [y/N]`
- `a` search AutoEq's headphones and import one (below)
- `i` import a parametric EQ file (below)
- `h` put the focused preset on the computer, or take it off when it is the one there; a built-in is
  refused, `NAME is built in: save a curve as a new preset to put it on the computer`, and where the
  computer EQ cannot run — host audio off, a container, no PipeWire — the box says why and does
  nothing. The preset on the computer is marked `computer` in the accent, beside the `●` of the one
  on the device
- `Esc` close the list

An imported preset's row says `imported` and, where the box is wide enough, how well the tone
control fits it, `fit 0.9 dB`; the fit is worked out the first time a screen draws it or the list
opens (on the screen's own thread, so ten or more imports pause it for a moment once), then kept.
After an import the new row is focused and nothing is loaded.

The same preset is never left on both the device and the computer. `h` on the preset that is on the
device asks `NAME is on the DAC. Move it to the computer? [y/N]`, and `y` flattens the device (every
band back on 0 dB at its centre) and then applies it (if the apply fails, the message says the DAC
was flattened and that `Enter` on the preset in this box, or `svolder eq load NAME`, puts the curve
back); `Enter` on the preset that is on the computer asks
`NAME is on the computer. Move it to the DAC? [y/N]`, and `y` takes it off the computer and then
loads it. Both are answered like the other questions.

A question is answered with `y`; `n`, `Esc` and `Enter` — no is the default — go back to the list.
`u`, `c` and `x` do nothing on a built-in: the box dims them, with the reason, while one is focused,
and pressing one says `NAME is built in; n saves the curve as a new preset`. The prompt takes at
most 40 characters, a preset's longest name, scrolling a name longer than its field with the cursor,
drops the blanks round the name typed, and refuses a blank name, one holding a control character, a
built-in's and another preset's, in any case, with the prompt still up; a name too long for the list
or a question is cut short there with `…`.

A preset saved from the device becomes the one loaded, for the header's `*`. While a load is still
being confirmed, `n` and `u` are refused with `still loading NAME`: each saves the curve the device
reads, which is still the one from before the load or part of the way to it. `Enter` loads another
preset meanwhile, planned from the steps the first load asked for, as the tone screen's band keys
and `0` are. The list reads the configuration file again as it opens, so it shows what `svolder eq`
or another front end has written since.

Every change is written into the configuration file the session reads, through the writers
`svolder eq save`, `eq rename` and `eq delete` use; a daemon that reads the same configuration file
is then told to read it again, silently, so its MQTT bridge offers the new list (a daemon that reads
another file is left alone); and the list is drawn from what the file says afterwards. Every key
that is not the box's own is swallowed while it is open, and the mouse reaches nothing under it: a
left click outside the box presses `Esc`, which closes it.

##### The AutoEq search (`a`)

`a` opens a box over the list with a query line over the matching measurements, one row each — the
headphone, then who measured it and on what, the recommended measurement of each headphone first.
Typing filters live, so `hd 6` finds every HD 6xx; `↑↓` move; `Enter` downloads the focused
measurement and imports it as a preset that remembers where it came from; `Esc` goes back to the
list. The status line says where the index is from (`index from 2026-07-01 · 3 measurements`), or
that it is being fetched, or why it could not be.

`Ctrl-R` fetches the index again whatever its age; otherwise it is read from the cache and fetched
when it is older than `[autoeq] refresh_weeks`. Fetching and downloading run off the loop, so typing
never waits on the network. A headphone's name is the preset's name, as `eq import --autoeq` names
it; a name another preset already carries opens the name prompt, prefilled with `NAME (source)`.

```
 svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on
─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─
┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐
│ +6 │                                                                         │
│    │                                                                         │
│    │                                                                         │
│ ┌ AutoEq ──────────────────────────────────────────────────────────────────┐⠒│
│ │ Search AutoEq  hd 6                                                      │ │
│ │                                                                          │ │
│ │   Sennheiser HD 600  oratory1990 · over-ear                              │ │
│ │   Sennheiser HD 650  oratory1990 · over-ear                              │ │
│ │ ▶ Sennheiser HD 650  crinacle · over-ear · GRAS 43AG-7                   │ │
│-│                                                                          │ │
│ │                                                                          │─│
│ │                                                                          │k│
└─│                                                                          │─┘
┌ │                                                                          │ ┐
│ │ index from 2026-07-01 · 3 measurements                                   │ │
│ │ ↑↓ move · Enter imports · Ctrl-R refreshes · Esc back                    │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

##### The file prompt (`i`)

`i` opens a one-line prompt, `File  ~/`. `Enter` imports the file, named for the file without its
extension and its ` ParametricEQ`; a name another preset carries opens the name prompt prefilled
with `NAME 2`; a file the parser refuses is a warning toast with its line. `Tab` completes the path
as a shell does: one match completes in full, adding `/` after a folder; several complete to their
common prefix and are listed under the prompt, up to six lines and then `… N more`, and a further
`Tab` steps through them. Only folders and `.txt` files are offered, and hidden entries only once
the part typed starts with a dot; a name that is not UTF-8, which the prompt cannot hold, is never
offered; a folder that cannot be read is said under the prompt. `Esc` goes back to the list.

#### The Loki Max's preset list (`p`)

A Loki Max has no tone control, so `p` opens its own list instead, over the device screen: the
device's three slots, `Preset 1` to `Preset 3`, then the five settings the device ships with under a
`Built in` header — Flat, Bass Boost, Treble Boost, Smiley and Loudness. The header takes no focus.
The built-in the six bands read exactly is marked `●`, and the list opens on it; nothing marks a slot,
since the device does not say which it holds. The box names its own keys, and everything around it is
dimmed as around the other list; an 80×24 terminal on a Loki Max whose bands read Bass Boost:

```
 svolder  Loki Max  ● connected                                        mouse:on 
─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─
┌ Controls ───────────────────────────────────────── mute ─ 0 centre ─ refresh ┐
│ ▶ Input  —                                                                   │
│   Mode   —                                                                   │
│                                                                              │
│   20 Hz  █████28/30███┌ Presets · Loki Max ─────────┐                        │
│   120 Hz █████21/30░░░│     Preset 1                │                        │
│   550 Hz █████17/30░░░│     Preset 2                │                        │
│   2 kHz  █████15/30░░░│     Preset 3                │                        │
│   6 kHz  █████15/30░░░│     Built in                │                        │
│   20 kHz █████15/30░░░│     Flat                    │                        │
│                       │ ▶ ● Bass Boost              │                        │
│                       │     Treble Boost            │                        │
│                       │     Smiley                  │                        │
│                       │     Loudness                │                        │
│                       │                             │                        │
│                       │ Enter loads · Esc closes    │                        │
│                       │ u saves · device slots only │                        │
└───────────────────────└─────────────────────────────┘────────────────────────┘
┌ Status ────────────────────────────────────────────────────────────── advice ┐
│                                                                              │
│ Raw: none                                                                    │
└──────────────────────────────────────────────────────────────────────────────┘
```

- `↑↓` move between the rows, over the header; a list longer than the box shows the rows round the
  focused one and says `more ↑` or `more ↓` on the border the rest are past
- `Enter` on a slot recall it and close the list, with `Recalled preset 2`. The device then reports
  its six bands; until a band does, its old reading is not trusted: the `●` goes out, and a built-in
  applied meanwhile writes that band whatever it last read
- `Enter` on a built-in apply it and close the list, with `Bass Boost on`: the bands that differ are
  written, every band going down first, then every band going up, each as the raw step the setting
  names; a band the device has not reported yet is written too. A setting the bands already read
  sends nothing and says `Bass Boost is already on`
- `u` on a slot write the device's current bands over the slot, after asking
  `Save the bands to preset 2? [y/N]`; `y` sends it and says `Saved to preset 2`. The box asks
  because a save overwrites the slot on the device and cannot be undone
- `u` on a built-in does nothing, and says `Bass Boost is built in; u saves on a device slot`; the
  box dims the key, with the reason, while a built-in is focused
- `Esc` close the list

A question is answered with `y`; `n`, `Esc` and `Enter` — no is the default — go back to the list,
and every other key is swallowed. Every key that is not the box's own is swallowed while it is
open, and the mouse reaches nothing under it: a left click outside the box presses `Esc`, which
closes it. `svolder preset list`, `preset apply` and `set recall|save` do the same from the shell.

### Telemetry screen (`t`)

Every reading the device has sent, labelled from the profile or `?`, with its recent history, its
age and update rate (`↑↓ ←→`). A list longer than the pane is a window that follows the selection,
and says so on the pane's right border: a dim `▲` beside the first row it shows while readings are
hidden above it, and a `▼` beside the last while readings are hidden below:

```
 svolder  Gungnir 2 · fw 3  ● connected                           mouse:on 14:02
─ Controls ── EQ ──┤ Telemetry ├── Devices ───────────────── ? help ─ esc back ─
┌ Telemetry ──────────────────────────────────────────── refresh ─ 6 functions ┐
│   ID       Type  Value       Label           History             Age  Rate   │
│ ▶ 6        u8    2           Sample rate (4… ⣀⣀⣀⣀⣀⣀⣀⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿   1s   1/s   │
│   7.3      u16   3           ?               ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣿⣿⣿⣿⣿⣿   1s   1/s   │
│   9        u16   734         ?               ⣴⣴⣴⣴⣧⣦⣴⣤⣄⣄⣾⣼⣴⣤⣦⣤⣠⣤   1s   1/s   │
│   12.1.2   u8    64          EQ band 1 freq  ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀   1s   1/s   │
│   210.2.1  u8    3           Firmware        ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀   1s   1/s   │
│   240.1    bool  false       ?               ⣀⣀⣀⣀⣀⣀⣀⣀⣀⠉⢁⣀⣀⣀⣀⣀⣀⣀   1s   1/s   │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

The pane's top border carries `refresh`, left of the count of readings.

A TUI joined to a daemon starts its graphs from the daemon's last 30 minutes of readings (the
socket's `readings`), so they are not blank when it opens.

The `History` column draws each numeric reading's recent past in braille, over `graph_span` (5
minutes unless `[tui] graph_span` says otherwise) and ending now; the newest value is at the right
edge. A number fills its column from the bottom up to its level, scaled through the control's scale
or range where the profile gives one (a meter's, fixed from its zero to its full scale) and
otherwise between the smallest and largest value the window holds; a present reading always draws at
least its bottom dots, and a blank stretch is time with no reading yet.

A meter or level control draws the loudest value of each column, so a peak never drops out; any
other number draws its latest. A true or false reading is a line of dots, high for true and low for
false. A string has no graph. The column takes what is left of the row once the label has 16 cells,
up to 40 cells, and only where at least 16 are left, so a narrow terminal draws the screen without
it; `graphs = false` draws it without the column at any width. The selected row's highlight runs
across the graph.

- `↑↓` move the selection
- `←→` nudge the focused reading's raw value by one, within its wire type's range — only for a
  reading no writable control already handles from the device screen

### Devices screen (`d`)

The configured devices, to switch between, add to, change and remove — always reachable, whether one
device is configured or several (`↑↓ Enter n c x r`):

```
 svolder  Gungnir 2  ● connected                                  mouse:on 14:02
─ Controls ── EQ ── Telemetry ──┤ Devices ├───────────────── ? help ─ esc back ─
┌ Devices ────────────────────────────────── new ─ update ─ x remove ─ refresh ┐
│ ▶ bifrost   Bifrost 3   connected                                            │
│   gungnir   Gungnir 2   disconnected (current)                               │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

A start that opened no device lands here, with one more frame above the list titled ` Not opened `.
It holds the failure's own words and the step that follows them (with `--transport daemon` and no
daemon: `no daemon …`, then the step `svolder daemon install`, or drop `--transport daemon`), and is
never more than half the screen. The list under it is the configuration's. The device screen reads
`No device is open` and how to pick one until a device opens.

The list's top border carries `new`, `update`, `x remove` and `refresh`, left of the `more ↑` mark
while rows are hidden above.

With no device configured the list says so and names the key that adds one. A device that is open
without an entry in the configuration (the fake transport's one device, say) is listed under the
message as a row that says it is not configured:

```
┌ Devices ────────────────────────────────── new ─ update ─ x remove ─ refresh ┐
│   No devices configured - n adds one                                         │
│   Gungnir 2   not configured (current)                                       │
```

With a daemon holding the devices, each row's product and link are the daemon's, asked for again
every 2 seconds while the screen shows, and a device the daemon is waiting to try again for carries
the wait — `disconnected, next attempt in 15 s` — as `svolder daemon status` does. A link the device
has refused the passphrase on says `connected · passphrase refused`, and a device whose readings
have gone stale says `disconnected · readings stale`, in the warning colour after the state. Once
any row has a signal strength the list gains a `signal` column between the product and the state, in
the short form: `−58 dBm` while a live strength is current, `−62 dBm 3 min` with an age, and `—` for
a device with none heard; a list too narrow for the column beside its states (a device waiting to be
tried again has the longest) leaves it out first. On a direct session the rows come from the
configuration file alone, and draw `-` for what it cannot say.

- `↑↓` move the selection; a list longer than the block scrolls to keep the selected device in view,
  and the border says `more ↑` or `more ↓` while devices lie past it
- `Enter` watch the selected device instead. The old device is released and the new one scanned for
  and connected before its screen draws, which takes a few seconds and up to about 40 when the
  device does not answer; a Wait toast says `⠋ opening NAME…`, with the spinner turning, for as long
  as it lasts. Ctrl-C or `q` pressed meanwhile gives up and quits: during the open it stops at once,
  and during the release of the old device it lets the release finish first, skips the open and then
  quits, so the old device is never left half released. Any other key pressed meanwhile is dropped
- `n` runs the same guided walk `setup` does (see ["setup"](cli.md#setup)), on its own screen, from
  its Scan step; the host is looked over first, under a frame that says so, with `q` and Ctrl-C read
  throughout; adding a device switches to it, leaving without adding one (`q`, or `Esc` on the Scan
  step) comes back to the list, an empty Devices screen included
- `u` opens a small editor over the list, for the selected device's name, passphrase, profile, and
  whether it is the default, with everything around it dimmed:

  ```
  │           ┌ Edit bifrost ────────────────────────────────────────┐           │
  │           │ ▶ Name        bifrost                                │           │
  │           │   Passphrase  ‹ auto ›                               │           │
  │           │   Profile     ‹ automatic ›                          │           │
  │           │   Default     [ ] no                                 │           │
  │           │                                                      │           │
  │           │ Enter saves · Esc cancels                            │           │
  │           └──────────────────────────────────────────────────────┘           │
  ```

  `↑↓` move between the four rows; `←→` move the caret along the Name — a name longer than its row
  scrolls through it with the caret, `…` standing for what is out of sight — and step Passphrase,
  Profile and Default; `Space` also flips Default; `Backspace`/`Home`/`End` edit the name being
  typed into; `Enter` saves — refusing a blank name, a name another configured device already
  carries, or a code of fewer than four digits (`a code is four digits`), in a Failure toast with
  the editor still open — and `Esc` cancels, changing nothing.

  Passphrase is a stepper like Profile, round three answers: `←→` go `none` → `auto` → a code →
  `none`, wrapping at both ends. `none` is a device with no passphrase, `auto` is the code the
  device's advertisement carries, and the code is four digits drawn as they are typed, with `_` for
  each still to come (`‹ 12__ ›`). In the code only digits are typed, in order, with no caret — so
  `←→` stay free to cycle — and `Backspace` takes back the last one. A digit typed on `none` or
  `auto` switches to the code and starts it with that digit. Stepping away from the code keeps the
  digits typed until the editor saves or `Esc` cancels, so stepping back finds them again. The row
  opens on what the file says: `auto`, `none`, or the code with its four digits.

  A save that wrote a passphrase the device does not advertise says so in a toast right after
  (`gungnir: the passphrase does not match the code the device advertises`), the best-effort note
  ["Configuration file"](daemon.md#configuration-file) describes: the editor has no advertisement to
  judge a code against as it is typed, so the entry is judged once it is saved, by the daemon that
  took it in or, for a direct session, which reads the file only when it opens, by the session
  itself: it takes the saved entry for its own device in at that moment, as the daemon does
  (re-keying to the code it means against the advertisement it holds, which also lifts a refusal a
  corrected code answers), and judges it.
- `x` asks before removing the selected device, dimming everything around the question as the editor
  does:

  ```
  │           ┌ Remove ──────────────────────────────────────────────┐           │
  │           │ Remove bifrost from the configuration?               │           │
  │           │                                                      │           │
  │           │ Enter removes · Esc cancels                          │           │
  │           └──────────────────────────────────────────────────────┘           │
  ```

Every write from this screen — adding, editing, or removing — goes through the same config writers
`add`/`remove` (above) use, tells a running daemon to `reload`, and re-reads the file before drawing
the list again; a rename or removal of the device being watched reopens the session under its new
name, or under the first device still configured, or sends the loop to the same guided walk `n` runs
when none is left; leaving that walk comes back to the empty list while a device is still on screen.

### Menu (`Esc`)

`Esc` goes up one level. With a box or a line open (the quit question, the help overlay, the menu,
the Settings box, the preset list, the device editor, the removal question, a band's edit line) it
closes that; during a curve drag it ends the drag. On the tone-control, telemetry and devices
screens it goes back to the device screen. On the device screen with nothing open it opens the menu:
a box over the dimmed screen, drawn the way the help overlay is, 36 columns by 11 rows in the middle
of the terminal at every size. It holds the small wordmark in the accent gradient, the version, and
three entries:

```
┌──────────────────────────────────┐
│                                  │
│  █▀▀▀ █  █ █▀▀█ █ ▄▄▄█ ████ █▀▀  │
│  ▄▄▄█  ██  █▄▄█ █ █▄▄█ █▄▄▄ █    │
│                          v0.31.5 │
│                                  │
│             Settings             │
│               Help               │
│               Quit               │
│                                  │
└──────────────────────────────────┘
```

The version row reads `v` and this svolder's version, dim, right-aligned one cell inside the border.
Over a daemon session the daemon's own version joins it as ` · daemon vX.Y.Z` once the daemon has
answered, dim where it matches this svolder and in the warn colour where the two differ, since a
daemon left running from before an upgrade is the case that exists for. The daemon is asked once,
over a connection of its own so the draw loop never waits on it, and one that has not answered
within 3 s is not asked again on that connection; a direct session, which has no daemon to ask,
shows its own version alone.

A daemon of another version also earns one warning toast, once for each version met:
`daemon vX.Y.Z is older than this svolder vA.B.C: svolder daemon restart` when the daemon is the
older build, and `daemon vX.Y.Z is newer than this svolder vA.B.C: restart svolder` when it is the
newer one (a daemon restarted by `make install` under a TUI that is still the old binary), the two
compared in semver order: opening another device on the same daemon does not repeat it, a restart
onto the same version stays quiet, a restart onto another differing version warns again, and a
direct session never shows it.

When the daemon goes away under a TUI (`svolder daemon restart`, `make install`), the TUI does not
give the device up at once. The wait starts when the old daemon's connection closes, which is when
that process exits (a few seconds after `restart` asked it to stop, longer while it closes a device
mid-connect), not when the command ran. The TUI then looks at the daemon's socket, after 0.1 s and
then after pauses doubling up to 1 s, for up to 30 seconds, drawing the link down meanwhile. It only
looks: it never starts a daemon of its own on the socket, so what answers is the one the restart
started.

When one answers, the TUI opens the device it had open by connecting to that socket and nothing else
(no daemon is started and no direct session stands in for it) and says `reconnected to the daemon`
in a toast. If nothing answers within the 30 seconds, or the daemon is gone again by the time the
TUI connects, it says `device did not come back`, and `r` reopens the device through the usual
launcher chain whenever a daemon is back. Where systemd restarts the service and the socket unit
keeps listening, the socket answers at once, and the reopen waits in its own "opening" wait until
the new service accepts.

The first letter of each entry is bold in the key-hint accent, and the entry under the cursor is
drawn selected. `↑↓` move the cursor, stopping at the ends, and `Enter` takes the entry under it.
`s`, `h` or `?`, and `q` take theirs at once: Settings opens the Settings box, Help the help
overlay, and Quit asks first unless `confirm_quit` is off, as `q` does. `Esc` closes the menu and
`Ctrl-C` quits at once. Every other key does nothing and never reaches the screen under the menu, so
`m`, `i` and `o` do not flip and `r` does not refresh. A left click on an entry takes it, and a
click anywhere outside the box closes it. Shrinking the terminal below 80×24 shows the too-small
notice and growing it back shows the menu again, still open.

### Settings box (`s`)

`s` opens Settings from any screen, as a box over the screen you were on, which is drawn dimmed
behind it; `Esc` closes it onto the same screen. The box fills the body, from the row under the
header down to the terminal's last row, no wider than the wide tier's body is and in the middle of
it, and it opens on the category and row it was closed on (the first time, `Look`, row 0). Every row
but the AutoEq refresh and the last category's is one key of the `[tui]` table (the refresh is
`[autoeq] refresh_weeks`), in five categories named on the box's top border with superscript digits,
the current one between `┤ ├`:

- **Look**: the palette preset, an accent override for each device family (DAC, amp, preamp), the
  colour depth, monochrome, the theme background, the graphs switch, and the two spans of history
  the graphs cover.
- **Effects**: the animation layer, then indented beneath it each of its switches (eased values,
  selection glow, shimmer, breathing borders, the focus runner, fault presence, peak hold, list
  fade, the Mascot switch), then the hue-drift mode, the rainbow-scroll switch and the transition
  between screens. The Effects row reads `on (inactive)` where the switch is on but the terminal
  cannot show the animation layer (below truecolor, or in monochrome), since nothing animates there.
- **Behaviour**: mouse capture, whether `q` asks before it quits, the title meter, whether the intro
  plays on launch, which effect builds it and whether a short sound plays with it, and how often the
  AutoEq index is fetched again (`[autoeq] refresh_weeks`).
- **Host audio**: whether the TUI reaches the host's own audio, and its visualizer.
- **Connect** (the Connections category): the daemon's `[web]`, `[mcp]` and `[mqtt]` sections and
  what the daemon says of them, described under "Connect" below.

The box on an 80×24 terminal, on Look:

```
┌ Settings ─ tab→ ─┤ ¹Look ├─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─ ⁵Connect ──┐
│ ▶ Palette             ‹ amber ›           │                                  │
│   Accent · DAC          (preset)          │                                  │
│   Accent · amp          (preset)          │  The colour preset every screen  │
│   Accent · preamp       (preset)          │  draws from: amber, ice, red or  │
│   Colours               auto              │  neon. Each names an accent per  │
│   Monochrome            off               │  device family and the good,     │
│   Theme background      on                │  warn and alert colours.         │
│   Graphs                on                │                                  │
│   Graph span            5m                │  [tui] palette = "amber"         │
│   Link span             12h               │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
│                                           │                                  │
└ ~/.config/svolder/config.toml ──────────────────────────────────── esc close ┘
```

The left column is 43 cells and lists the category's rows: the cursor, the label (an animation's two
cells in), and the value, cut to 12 cells and `…` where it is longer than 13. Only the selected row
draws `‹` before its value and ` ›` after it, whatever kind of row it is, and a row that one of the
uppercase keys also turns from any screen names that key, dimmed, in a column of its own after the
value: `E` on the animation layer, `F` on the focus runner, `H` on hue drift, `R` on the rainbow
scroll, `M` on mouse capture, and `V` on the title meter. A category too tall for the box scrolls
with the cursor, a dim `▲` or `▼` on the rule between the columns saying which way the rest are.

The right column, from the rule on, wraps its text two cells in from the rule and from the border,
starting two rows below the top border, and explains the selected row: what it does and the values
it takes, a blank line, the row's key in `config.toml` with the value it holds now
(`[tui] palette = "amber"`, `[tui.effects] glow = true`, `[tui.accents] dac = "#e0341c"` or
`[tui.accents] dac  (not set)`), and, for a row an uppercase key also turns, a line such as
`E from any screen`.

A row's own accent reads `(preset)` while nothing overrides it, which means the palette's own colour
for that family draws; the arrows step it from `(preset)` through the colours the palette gives the
other two families, and a colour of the row's own that the palette does not offer — written by hand,
another palette's, or a bare `accent`'s — is a stop just before `(preset)`, which the row keeps once
it steps away, for the rest of the session, so the arrows lead back to it; while a bare `accent` is
on file all three rows read that colour instead, marked `(all)` — `blue (all)`, say, and its config
line names `[tui] accent` — since one key is drawing every family, and stepping any of the three
clears it.

The bottom border carries the configuration file's own path, `~` standing for the home directory; a
path too long for the border gives up its leading directories to `…` — `…/svolder/config.toml` — so
the file name always shows. `esc close` ends the border at its right end, and a click on it, or
anywhere outside the box, the greyed header and its logo included, presses `Esc`; a Connect
category's edit line or token question over the rows takes the same click as its own `Esc`, which
cancels it and leaves the box open. The box draws neither the version nor the licence notice: the
notice is on the intro's last two rows (see "Palette and the wordmark" below).

Keys, which the box takes whole, so a key it has no use for never reaches the screen under it (`m`,
`i` and `o` do not flip and `r` does not refresh behind it):

- `↑↓` move the selection within the category
- `←→` change the row: either arrow flips a switch the same as `␣`/`Enter` does, and steps a row
  that chooses among several values (the transition, the palette, an accent, or the colour depth) to
  its neighbour, wrapping round at the ends; the AutoEq refresh row steps `never`, `1 week`,
  `2 weeks` … `99 weeks` one at a time and stops at both ends instead of wrapping, and `␣`/`Enter`
  leave it alone
- `␣`/`Enter` turn a switch row the other way
- `Tab`/`⇧Tab` move to the next or previous category, wrapping round and starting at its first row,
  and `1`–`5` jump to one
- `M`, `E`, `H`, `R`, `F` and `V` turn their rows, and the box shows the change
- `Esc` closes the box
- `?` opens the help overlay over the box, its `Settings` section first; `Esc` closes it back onto
  the box
- `q` quits, asking first as it does elsewhere

With mouse capture on, a click on a row selects it, a click on the `‹` or `›` of the selected row
steps it as `←` and `→` do, and a click on a category's name on the top border shows that category.
A click anywhere else does nothing, not even on the dimmed screen.

A change applies to the running session on the next frame (the one exception is a mouse toggle the
terminal refuses, which changes nothing, writes nothing and shows the toast
`mouse capture did not change: the terminal refused`) and is written to `config.toml` immediately:
the writer reads the file's own `[tui]` table first and rewrites only the keys the change alters,
leaving every other byte of the file as it was (a hand edit made while the TUI runs stays, comments
and quote style included), and a deleted file comes back as the template with that one change; it
touches only the `[tui]` table — or, for the AutoEq refresh row, `[autoeq]` — writing each key it
changes in the shape a hand edit would use (`effects` as one word while every switch is on or every
switch is off, an accent only for a family that overrides one, as dotted `accents.<family>` keys
when the table has to open one of its own rather than a `[tui.accents]` header).

The box is drawn in the theme's own colours, so it shows the change on the next frame and plays no
screen sweep for it. The new text goes to a uniquely named `config.toml.<pid>.<n>.tmp` beside the
file, is fsynced and is renamed over it, so an interrupted save leaves the old file whole rather
than a truncated one, and a `config.toml` that is a symbolic link is replaced by the regular file
the rename leaves there; a config made for the first time is created at mode 0600, and a file
already there keeps its own mode.

Every writer of `config.toml` — the TUI, `svolder eq save` and the other verbs that change it —
holds an exclusive lock on `config.toml.lock` beside it (mode 0600, left in place) from its read to
its rename, so two svolders writing at once each keep the other's change; one that waits more than 2
s fails with `config io …: another svolder is writing this file`. A `config.toml` made read-only
(`chmod 444`) is read-only on purpose: every writer refuses it with `config … is read-only` and
leaves it as it is. A write that fails is reported in a Failure toast with its reason
(`settings not saved: …`), drawn over the box at full strength, and the change still applies for the
rest of the session.

#### Connect

The fifth category edits the daemon's servers instead of `[tui]`, and shows how each stands. Its
rows, top to bottom:

- **Web UI** reads the daemon's state alone (`off`, `no daemon`, `listening on …`, cut to the column
  like any value), and `␣`, `Enter` or an arrow opens the Web UI box over the rows, described under
  "The Web UI box" below.
- **MCP server**, **MCP writes** (`read-only` or `read/write`), **MQTT bridge** and **MQTT
  commands** (`read-only` or `allowed`) are switches: `␣`, `Enter` or either arrow flips one.
- **MCP listen**, **MQTT broker host** and **MQTT broker port** open an edit line over the box on
  `␣`, `Enter` or an arrow: type, `←→`/`Home`/`End`/`Backspace` edit, `Enter` saves and `Esc`
  cancels, and a value that is not an address, a host or a port 1–65535 is refused in a toast with
  the line still up. The listen row has no list of the machine's addresses to cycle through: it is
  typed. It reads `loopback:8737` for the default `127.0.0.1:8737`, and the right column has the
  address whole.
- **MCP token** reads masked, `••••••3fQ`: six dots and the token's last three characters (a token
  shorter than 16 characters shows only the dots). `v` shows the whole token in the right column for
  10 seconds, and `v` again hides it sooner; `c` copies it the way the Web UI box copies its link
  (see "The Web UI box"): an OSC 52 sequence to the terminal (wrapped for tmux and GNU screen) and,
  under a display, the token piped on standard input to `wl-copy`, `xclip` or `xsel`; the notice
  says what happened (`token copied with wl-copy`, or
  `token sent to the terminal's clipboard (OSC 52) — if your terminal ignores it, svolder mcp token
  prints it`, or that with why a tool did not run),
  never `copied` for the sequence alone, and never holds the token; `R` asks
  `Replace the MCP token? [y/N]` (`n`, `Esc`, `Enter` or `q` cancels) and on `y` writes a fresh
  token and has the daemon reload.

  `R` refuses a token from `SVOLDER_MCP_TOKEN` or `token_file`, naming the source, as
  `svolder mcp token --rotate` does. The token is shown whole only while revealed, and never in a
  toast or the config line; the three trailing characters of the mask only say which token is in
  use.

  The row compares what the daemon reports of its token (the source, its path and those three
  characters, never the token) with what this session reads: when they differ — the daemon has
  `SVOLDER_MCP_TOKEN` in its environment, or a container has its own state directory — the row shows
  the daemon's tail and the right column says where the daemon's token lives
  (`The daemon's token lives in the daemon's environment (SVOLDER_MCP_TOKEN).`, or
  `The daemon's token lives at PATH, not the file this session reads.`), and `v`, `c` and `R` refuse
  with that reason.

  When the daemon names the file this session reads but has another tail (the file changed and the
  daemon has not read it again, after a reload that failed or a hand edit), the row is this
  session's own and the right column says
  `The daemon has not read PATH again yet: svolder daemon restart (or SIGHUP).`; `v`, `c` and `R`
  work, and `R`'s reload settles it. With no daemon running the row shows this session's own token,
  marked `(daemon not running)`, and `v`, `c` and `R` work on it. `v`, `c` and `R` are listed in the
  help overlay's `Settings` section. If the daemon cannot be told to reload after a rotation, the
  notice says so and names `svolder daemon restart`; with no daemon it says none uses the token yet.
- **MCP status** and **MQTT status** are the daemon's own answers, as `svolder daemon status` words
  them (`listening`, `failed`, `connecting`, `connected`, `off`; the right column has the whole
  line). They are asked again every two seconds while the category shows, so a bridge that connects
  after a change shows it.
- **MQTT password** reads `set` or `not set` (the file's `password` or `SVOLDER_MQTT_PASSWORD`) and
  nothing else: it is never shown and never edited here. It reflects this session's environment and
  config file; the daemon's own environment may differ.

A change writes only its own key of `[web]`, `[mcp]` or `[mqtt]` into `config.toml`, as the `[tui]`
writer does for its keys (comments and every other byte stay; a value the file would refuse, such as
turning the bridge on with no host, is reported as `settings not saved: …` and writes nothing), and
then sends the daemon a `reload`. A daemon that reads another configuration file than this session's
— a container's — makes every Connect row read-only: a key reports
`read-only here: the daemon reads PATH`, and the right column names that path. The Connect category
needs a daemon for its status rows; with none they read `no daemon`.

#### The Web UI box

`Enter` on the Connect category's Web UI row opens a box of six rows over the Settings box, titled
`Web UI`, with its keys on the last two lines. `↑↓` move the cursor among the rows that take a key
(the first, State, only reports); `Esc` or `q` closes the box and forgets its codes (`q` here closes
the box, it does not ask to quit).

- **State** is the daemon's own answer, as `daemon status` words it: `listening on ADDR`, `off`,
  `failed: WHY`, `unknown to this daemon`, or `no daemon`.
- **Web UI** reads `on` or `off`; `Enter` or either arrow turns it, writing `[web] enabled` and
  sending the daemon a reload.
- **Reach** reads `this computer` (a loopback listen address), `your network` (a wildcard one) or
  `custom (ADDR)`; either arrow moves it between the first two and keeps the port, writing
  `[web] listen`, and a custom address moves to this computer.
- **Link** is a link that signs one browser in, made when the box opens on a daemon whose web UI is
  listening and again by `l`: the minutes and seconds it has left on the label's line (`1:52 left`,
  then `ended`), and the whole link under it on lines of its own, as wide as the box's text area,
  wrapped with nothing after its last piece; under the pointer every line of it underlines. The link
  is also a terminal hyperlink (OSC 8, one link across the wrapped lines): a terminal that supports
  them (VTE: Ptyxis, GNOME Terminal; others do too) opens it in the browser on your own machine on
  Ctrl+click, and offers Copy Link in its own menu, which works over SSH when OSC 52 does not.

  While an application tracks the mouse, as this one does, that menu is the terminal's own only on
  Shift+right-click (a plain right-click goes to the application, which does nothing with it); a
  terminal may want Shift held with the click too, and its own documentation says. The box says
  `Ctrl+click opens it; Shift+right-click copies it` under the link, and a terminal without
  hyperlinks shows the same text.

  `c` copies the link by every way that can reach you: it sends the terminal an OSC 52 sequence
  (under tmux, `TMUX` set, both the plain sequence, which needs `set-clipboard on`, and the wrapped
  one, which needs `allow-passthrough on`; under GNU screen, `STY` set, the wrapped one), and where
  the TUI has a display it also pipes the link to `wl-copy` (`WAYLAND_DISPLAY`) or to
  `xclip -selection clipboard`, else `xsel --clipboard --input` (`DISPLAY`, which `ssh -X` sets),
  run off the screen's loop with its streams going nowhere.

  The box says what happened, and never `copied` for the sequence alone, which a terminal cannot
  confirm (VTE ignores it): `link copied with wl-copy` (a tool still running after two seconds is
  worded `link handed to xclip, which has not said it took it`); with no display, `link sent to the
  terminal's clipboard (OSC 52) — if your terminal ignores it, Ctrl+click the link to open it,
  Shift+right-click it to copy it`; with a tool that is missing or fails, the same sentence with why
  the tool did not run after a semicolon.

  A copy made while another is on its way is sent to the terminal at once, and the tool (and, for a
  click, the browser) gets it when the last one ends, and the notice says so; only the newest waits.
  `Enter` on the Link row is `l`, and a left click on the link's text (every line of it, where it
  wraps) does what `c` does and also hands the link to `xdg-open`, which never takes the terminal
  (its input and output go nowhere); the box says `copying the link and opening the browser…` at
  once (the copy and the opener run off the screen's loop, one at a time), then the copy's note
  followed by `; opened in the browser`, or `; no browser was opened (WHY)`, as when neither
  `DISPLAY` nor `WAYLAND_DISPLAY` is set (`no desktop`: no opener is started then) or `xdg-open`
  fails.

  Below the rows the box draws the link as a QR code in black on white, for a phone to scan, where
  the terminal has the room, else `widen the terminal to see the QR code`. On a loopback listen with
  no `[web] url` it says `a phone cannot reach this computer's loopback: choose your network`, and
  where no address a phone reaches was found, `a phone cannot open this link: no address a phone can
  reach was found, so set [web] url`.

  A link signs one browser in, and a phone's camera preview of the QR code can use it up before the
  phone's browser opens it. While a link shows, the box asks the daemon about once a second (off the
  screen's loop) whether it is still open; once a browser has used it, or it has ended another way
  (a daemon restart, a token replaced elsewhere) before its five minutes, the box replaces it with
  a fresh one (the text, the hyperlink and the QR code) and says `the last link no longer works; a
  fresh one is shown` under the hint, so the next scan works. A daemon that cannot say (one that
  does not know the question, or a web UI turned off) is not asked again about that link. A link
  that reaches its five minutes is replaced as well, without the note, and the pairing code is
  never replaced by itself.
- **Pairing code** is six digits in two threes with the time left (`482 913 · 1:52 left`), made by
  `p` or by `Enter` on its row, and copied when it arrives the way the link is (the OSC 52 sequence
  and the display's tool, no hyperlink: a code is not an address), with the same note
  (`code copied with wl-copy`, or `code sent to the terminal's clipboard (OSC 52)` followed by why a
  tool did not run, or, with no display, `read it off the box`); the code stays shown whatever the
  tool does; under it the box says that guesses from the network can use a code up and that the link
  is the fallback.
- **Token** reads masked as the MCP token does; `r` asks `Replace the web token? [Y/n]` (every
  signed-in browser is signed out), with its answers on the last line (`Enter/y replaces`,
  `Esc/n/q cancels`), and on `y` or `Enter` writes a fresh token and has the daemon reload, which
  refuses a token from `SVOLDER_WEB_TOKEN` or `token_file` as `svolder web token --rotate` does.
  Once the daemon has reloaded, the link and the pairing code on show are gone (the old token ended
  them), and `l` or `p` makes new ones; a refused replace leaves them.

`l` and `p` need a running daemon, which serves the page and makes the codes; without one the rows
read `the daemon serves the web UI` and the keys say to start it. The two writes need the daemon to
read this session's configuration file, as every Connect row does. The box is 64 columns wide, or
the terminal's width where that is less.

### Setup wizard

`svolder setup` and bare `svolder` with no device configured (see
["Getting started"](getting-started.md#getting-started) and ["setup"](cli.md#setup)), on a terminal,
walk the wizard's nine steps as screens of their own — one step per screen, under the small wordmark
where the frame is wide enough, its title on the mark's second row and repeated alone below it.

`↑↓` move the cursor among a step's choices, `Enter` takes the one it is on; a step with no choices
takes a typed line instead, its last body line the value an empty line stands for, shown dimmed as
the field's placeholder, with `←→`/`Home`/`End`/`Backspace` editing it (except the Passphrase
step's, which is a row `←→` step between `none`, `auto` and a code, see its row of the table below).

`Esc` goes back to the step before; on the first step it leaves a walk the TUI opened (back to the
screen `n` was pressed on) and leaves a standalone run (`svolder setup`, or bare `svolder` with
nothing configured) for later, the TUI opening on an empty Devices screen; it is dimmed in the key
line on a standalone walk that starts past the Health step and once the device has been written,
when there is nothing to go back to.

`PgUp`/`PgDn` scroll the step's own words where there are more of them than the screen holds, while
a choice list too long to draw whole is a window the cursor moves through, with a
`↑ N more`/`↓ N more` row at either end of it; `q` quits (a letter like any other while a line is
being typed, where Ctrl-C still leaves).

Leaving mid-write — Ctrl-C or `q` during the Fixes, Name, Passphrase, Adapter, or Daemon step's own
wait — finishes that one write before the wizard closes, so a device it was already writing is never
left half-configured; the Scan wait, which writes nothing, stops as soon as asked. The wizard's own
keys:

```
 Setup  ↑↓ choose ←→ move PgUp/PgDn scroll Enter confirm  Esc back  q quit      
```

The Name step, an 80×24 terminal on a Bifrost 3 the Scan step just heard:

```
 What to call it                                                                
────────────────────────────────────────────────────────────────────────────────
 Enter takes the name below; type another to use that instead.                 
                                                                                
 > Bifrost 3                                                                    


















 Setup  ↑↓ choose ←→ move PgUp/PgDn scroll Enter confirm  Esc back  q quit      
```

The nine steps, each a screen of the wizard:

| step | title | asks |
|---|---|---|
| Health | How this host looks | the `doctor` checks above and their summary, `▶ Continue` the way on, and `Later: open svolder without a device` to leave the wizard for the interface; a fix's own `svolder setup`/`svolder daemon install` line never appears here, since this run is the one or leads to the other |
| Fixes | What svolder can put right itself | one confirmation for every unprivileged fix the Health step found (`Apply N fixes` / `Skip`, defaulting to Skip); skipped outright on a host with none |
| Scan | Find the device | up to three five-second windows, stopping at the first that hears one; nothing heard offers `Scan again` / `Skip` and says to power the device on and close the phone app; three scans that could not run at all — no adapter, a bus that will not answer — end the run on the last reason given |
| Pick | Which device | one device by number, when more than one answered — product, unit id, signal, and the profile the advertisement matches; an unknown product offers the profile list instead |
| Name | What to call it | a typed name, defaulting to the product; a name already configured is refused and asked again |
| Passphrase | The device wants its passphrase | shown only after the first connect is refused, as a row like the Devices editor's: `←→` step it `none` → `auto` → a code, `‹ ____ ›` opening on the code, where only digits are typed, in order, and `Backspace` takes back the last; `Enter` answers with what the row is on, and a code of fewer than four digits is refused on the step (`a code is four digits`) without troubling the wizard. Where the terminal cannot be claimed the plain prompt takes a typed line: four digits, `auto`, `none` or blank for none |
| Adapter | Which adapter to use | shown only with two or more adapters present: each one's catch rate against the device just picked, best first, written into `adapter` |
| Daemon | The background service | `Install the user service` / `Skip` where systemd is present; the NixOS or container line, or — with no systemd at all — that svolder starts a daemon of its own when a command needs one, each with a single `Continue` |
| Done | Done | what was written, and `Enter opens <name>`; `▶ Finish` is that Enter — the wizard hands the device it added to the device screen without leaving the alternate screen, and `q` here is what leaves without opening anything |

The device goes into the file at the Name step (or at the Passphrase step, where one was wanted) and
the adapter at the Adapter step, but a running daemon is told to re-read the file only once both are
written — one `reload` for a run — because a daemon told earlier would connect the device while the
adapters were still being graded against it, and a device a central holds stops advertising.

A refused answer — a taken name, a code of fewer than four digits, a choice that was never offered —
is said on the row above the key line, the step's own keys still on the screen beside it so it can
be answered again. Where the terminal cannot be claimed, both `setup` and bare `svolder` fall back
to the plain numbered prompts `wizard::cli` prints on stderr, asking the same nine steps one at a
time.

### Palette and the wordmark

Four presets ship: `amber` (the default), `ice`, `red`, and `neon`, each one naming an accent for
every device family (DAC, amp, preamp), the three semantic colours (good, warn, alert), and the two
text greys. In `amber`, `ice` and `red` the three family accents are shades of the colour the preset
is named for, so its name describes whatever device is on the screen — `amber` draws a DAC in
`#e0a030`, an amp in `#b86a2a` and a preamp in `#f6cc7a`; `ice` in `#5fc9c9`, `#1f6aa8` and
`#a8e6f0`; `red` in `#e0341c`, `#b01c3c` and `#ffa890` — while `neon` gives the three families three
hues of their own, `#ff51fd`, `#2de2e6` and `#a36bff`, on purple-black greys (see
["Configuration file"](daemon.md#configuration-file) for the `palette` and `accent`/`[tui.accents]`
keys, and "Color" below for how depth reduces them).

The accent follows the device on screen: a profile's `family` — `dac`, `amp`, or `preamp`, inherited
through `extends` from the `dac-common`/`amp-common`/`preamp-common`/`phono-common` roots,
overridable by the profile's own `family` key, and defaulting to `dac` where neither says otherwise
— picks which of the preset's three accents draws that device's level bars, meter gradients,
selection, header rule, wordmark and key hints (on `red`, its header rule, wordmark, key hints, `▶`
marker and focused frame: see below). A stack with a DAC and an amp on the devices screen reads as
two colours.

A fault is drawn in the preset's alert red, and where the accent on the screen is too close to that
red to tell the two apart, the fault's row takes inverse video as well, so it never reads as one
more accented row. That happens for `red`'s own scarlet DAC; at 16 colours for any accent that lands
on the terminal's red or bright red, which includes the `amber` and `ice` preamps there and every
family of `red`; and for an `accent` or `[tui.accents]` value set near the alert red on any preset —
`#ff4a3a`, say, or `bright-red` at 16 colours.

`red` is Schiit-styled: off-white text on near-black (`#0c0c0c` under the screen with the theme
background on, else the terminal's own black), labels and secondary values in a dim grey
(`#7a7a7a`), green (`#6fbf73`) for good states and amber (`#d9a441`) for warnings, and nothing blue
or cyan anywhere. Its accent, the family's red, keeps the rule under the header, the key hints, the
`▶` marker and the frame the keys are in, and glows along the preset's spectrum as every accent
does; the level and meter bars, the selected row's inverse video and every pane's title are drawn in
the off-white instead, and every other frame in the faint grey, so the red marks where the keys are.

Its tone curve runs from dark red at 20 Hz through the scarlet and the amber to the off-white at 20
kHz rather than round the hue circle (see "Effects" below). At 16 colours its accents are the
terminal's red for a DAC and an amp and its bright red for a preamp, and the off-white is the
terminal's white. `amber`, `ice` and `neon` draw their bars, selection and titles in the accent. At
16 colours `amber` and `ice` accent a DAC, an amp and a preamp in the terminal's yellow, cyan and
red, and `neon` in its bright magenta, cyan and magenta, none of which is the slot of a state
colour. `neon`'s tone curve runs a sunset of its own, from violet at 20 Hz through hot pink and
orange to gold at 20 kHz, rather than round the hue circle.

`svolder tui` opens with the Svolder wordmark: a block-letter mark drawn in a two-tone gradient of
the active accent. `intro_effect` under `[tui]` (the Settings box's **Intro effect** row, in
Behaviour) says how it comes in.
- `particles` builds it from sparks, about 3.6 seconds in all. They gather along the edges of the
  terminal, are thrown in on gentle curves, and land on the letters along a front that rises from
  the bottom left. Each letter flashes and cools as it fills, and its shadow slides out from under
  it. A glint runs along the letters and the notice fades in.
- `fade` fades the mark in over about 1.2 seconds and holds it for 0.4 more before the device screen
  comes in, its panes traced round under the default transition (see "Effects" below).
- `random`, the default, picks one of the effects that build the mark at each launch, never `fade`.

After a full `particles` intro the mark breaks back into sparks that fly, letter by letter, into the
small mark in the header, or the word `svolder` on a terminal under 100 columns. Each cell of it
lights up as its sparks land. Meanwhile the panes come in: traced round under the default
transition, there at once under the others. When both are done, the light runs once round the
focused pane. A key, a click or a resize ends it at once, and a key or a click does what it always
does. A skipped intro goes straight to the screen. There is no exit after a resize, or while the
device is still being waited for, or on a terminal under 80×24, which draws only its too-small
notice.

Below truecolor, or with the animation layer off, the mark is drawn once and held for half a second
whatever `intro_effect` says. A terminal smaller than 70×12 gets the fade on the small mark. The
sparks run at sixty frames a second, dropping to thirty for the rest of the intro on a terminal slow
to keep up, and over SSH from the start. Each frame is sent as one synchronized update, which a
terminal that does not know the mode ignores. Any key skips it: while the device is still opening,
Ctrl-C or `q` gives up on it instead — the terminal is restored and the TUI exits; once the device
has answered, a key over the mark only ends the intro, and the device screen opens, where `q` quits.

The device connects in the background from the moment the TUI starts, so the device screen is
already populated when the intro ends — and so the mark is drawn in the default family's accent,
since it plays before any device has answered for its own; the small mark in the header takes the
device's colour once it has. `intro = false` under `[tui]` removes it outright, and it never shows
when stdout is not a terminal. The copyright and licence notice stands on the terminal's last two
rows, centred and dim: it fades in with the mark under `fade`, at the end of the build under
`particles`, and is drawn at once where the mark is static. With the intro off the TUI draws no
notice, and `svolder --version` still carries it. A terminal at least 100 columns wide carries the
small, two-row form of the mark in the header beside the device name on every screen; a narrower one
shows the device name alone.

With `intro_sound = true` under `[tui]` (off by default, and the Settings box's **Intro sound** row,
in Behaviour), a short sound of about five and a half seconds plays with the wordmark, through
PipeWire's `pw-play`. It plays once per `svolder tui` start, and only when the intro is shown, host
audio is allowed (`host_audio` on, and `SVOLDER_HOST=off` not set, so `svolder tui --demo` without `--host`
never plays it) and `pw-play` is on `PATH`. It starts the moment the opening device first answers while
the wordmark is still up, because that is when its sample rate is known: a start that ends without a
device answering during the intro, on the waiting panel, the Devices screen or the setup wizard, or
with the intro skipped first, plays nothing.

It follows the DAC's own sample rate, read from its `sample_rate` control, so that a Schiit DAC's
relay does not click on a rate change; a device with no such control, or one that reports no rate
yet, leaves the rate to PipeWire. Any key that skips the intro stops the sound, and so does leaving
the TUI; otherwise it plays on, under the sparks' flight into the header after `particles`, to its
end over the device screen. A missing `pw-play` or a failure to play is silent. Three limits: it
plays on PipeWire's default output, which may not be the Schiit; PipeWire changes its rate only
while nothing else is playing and only to a rate in its `default.clock.allowed-rates`, which is 48
kHz alone by default, so on a default setup no rate change happens anyway; and a DAC whose output
was suspended may still click as it locks onto the signal again, at any rate.

### Color

Colour depth is detected from the environment and can be overridden by `colors` under `[tui]` (see
["Configuration file"](daemon.md#configuration-file)): `COLORTERM` of `truecolor` or `24bit`, or a
`TERM` ending in `-direct`, means truecolor; a `TERM` containing `256color` means 256 colours;
anything else means the 16 ANSI colours. `--no-color`, the `NO_COLOR` environment variable, or
`monochrome = true` under `[tui]` drop every colour and mark the selected row by reverse video
alone.

Every role — connected, stale, a fault, an enabled toggle, a warning — uses an ANSI colour name at
every depth, so the terminal's own theme decides what red and green look like. The extra depth goes
into: the accent, when it is a `#rrggbb` value (exact at truecolor, the nearest 256-colour cube
entry at 256, the nearest of the 16 ANSI colours at 16); the level bar and meter, which shade from a
darker accent to the accent, and blend green through yellow to red, at 256 and truecolor (one flat
colour at 16; on `red` both are its off-white at every depth); and the grey ramps that stale rows,
the curve's axis, and the header's dim parts draw in at 256 and truecolor, in place of a plain dim
attribute at 16.

### Theme background

While `theme_background = true` under `[tui]` (the default), each palette paints three dark greys,
warm or cool with its accent, so svolder looks the same on any terminal: the ground under the whole
frame, the pane grey inside every pane and on its border, and a raised grey under every box (the
menu, help, the quit question, the preset list, the device editor, the removal question, a band's
edit line) and every toast. The Settings box sits on the pane grey instead, so a palette picked
there reads in it as the screens draw it; a Connect edit line or the token question over its rows is
raised as the other boxes are.

A cell that has a background of its own, the selected row's inverse video among them, keeps it. The
greys are exact at true colour and, at 256 colours, taken from the 240 entries above the ANSI
sixteen (the colour cube and the grey ramp together): the raised grey's nearest, then the pane's
nearest among the entries darker than that, then the ground's among those darker still, so the three
stay three different greys in their own order (on `amber` and `ice`, black and the ramp's two
darkest steps; on `red` and `neon`, the ramp's three darkest); at 16 colours and in monochrome
nothing is painted and the terminal's own background shows, as does `theme_background = false`,
which the Settings box's **Theme background** row (Look) sets.

The dimming round a box changes the text only, so the ground and pane greys stay under it, and the
animation layer leaves every background as painted. The intro (the fade, and the sparks of
`particles`) and a screen change's sweep fade in from the ground, and a pane's trace from the pane
grey. While it is on svolder stays dark on a light terminal; with it off, a light terminal shows
svolder's light-on-dark colours over its own background, and the intro, the sweep and the trace fade
text in from a dim grey, which reads on a light terminal as on a dark one, with the terminal's own
background behind it throughout.

### Mouse

Mouse capture starts on (`mouse = true` under `[tui]`) and `M` turns it off or back on, because
capture disables the terminal's own text selection and copy; a terminal that refuses the change
leaves capture as it was and says `mouse capture did not change: the terminal refused` in a toast.
The header shows `mouse:on` or `mouse:off` just left of the clock, or at the header's far right on
the tone-control screen, which has no clock. With capture on:

- click a row to focus it; click a toggle's box to flip it; click `‹` or `›` on a select to step it
- click a tab of the strip under the header to show that screen (the current tab and a faint `EQ` do
  nothing), `? help` to open the help overlay, `esc menu` or `esc back` to press `Esc`, and a word
  of a pane's top border (`mute`, `0 centre`, `presets`, `refresh`, `new`…) to press its key, a
  faint word included
- on the device screen, click the Controls pane's title to focus it, or the Tone control pane's
  title to focus that pane (the tone-control screen is the `EQ` tab and `e`); the Status pane's
  title does nothing, the pane never being focused; on the tone, telemetry, and devices screens,
  click the main block's title to return to the device screen
- click a band row on the tone-control screen to select it; double-click its frequency, gain or Q —
  two left presses on the same cell within 400 ms — to open the band's edit line with that value's
  field focused (see "The edit line" above); a press a box over the screen takes, such as the click
  that closes the help overlay, makes no double-click with the next one
- on the tone-control screen's Response curve — in every mode but Loudness, which draws no curve —
  press within a cell of a band's point to grab it, focusing the band; where two points are that
  close, the band the cursor is on wins, and otherwise the nearer one. Drag with the left button to
  move the point: across sets the band's frequency on the curve's log axis and up and down its gain,
  each at the device step nearest what the cell under the pointer stands for — the frequency held
  inside the band's window, the gain inside the chart's -12 to +6 dB, and the row the 0 dB line is
  drawn on standing for 0 dB exactly, so a point dragged back onto that line is flat.

  Each axis works on its own: the frequency stays where the press found it until the pointer leaves
  the column the press landed in, and the gain until it leaves the press's row, so a drag straight
  across leaves the gain alone and one straight up or down leaves the frequency alone, whether the
  press landed on the point or a cell off it. Drag with the right button sideways to set the band's
  Q, one device step per cell from where the press landed: right widens the bell (the Q comes down)
  and left narrows it; a right press away from every point grabs the band whose centre is nearest in
  log frequency. Letting go of the button ends the drag, and `Esc` ends it at once, doing nothing
  else then; so do a screen change and a box opening over the screen.

  A release the TUI never sees — the button let go outside the window, say — is made up for: the
  next press of that button, and `M` turning capture off, let the drag go where the pointer last
  dragged it, so a later click elsewhere never moves the band. A press of the other button while a
  drag is held is ignored. The point follows the pointer as it goes: while the device has not
  answered a write of the band, only the latest position is kept and sent once it has, and letting
  go always writes where the pointer let go, so a drag never queues one write per motion. A cell is
  a coarse step: the keys and the edit line are the fine ones
- the wheel over the curve steps the Q of the band under the pointer one device step a notch — up
  narrows the bell, down widens it — the band whose centre is nearest in log frequency where the
  pointer is off every point
- a drag on the curve and the wheel over it are band edits like the keys': in Flat mode each
  switches the tone control to EQ
- click the Mode half (the tone-control screen's header, or the device screen's Tone pane) anywhere
  but its `‹ ›` to toggle the flat mode, focusing it — the same as `f`; click `‹` or `›` to step the
  select instead, like every select's arrows
- click anywhere on the Loudness half but its bar — the word, the value — to toggle the loudness
  mode, focusing it; the wheel over it steps the strength
- click a level's bar — the Controls pane's Volume and Balance, the Loudness bar on the Tone control
  pane and in the tone-control screen's header — to focus its row and set the value the clicked cell
  stands for (the cell's middle, read against the bar's range as the bar draws it, as the nearest
  raw value); a click on the row's label only focuses it. Press on a bar and drag to follow the
  pointer, which may leave the row and the pane: the value under the pointer is set as it moves,
  held at the range's ends, until the button is let go; the bar draws the dragged value at once.

  A drag paces its writes as the curve's does: while the device has not answered a write of the
  control, only the latest value is kept and sent once it has, and letting go always writes where
  the pointer let go. The Volume alone glides: a raise above 75 % of its range writes 75 % at once
  and then climbs a tenth of the range a second toward the click or drag's target, which is drawn
  dim on the bar, each step one raw step above the lower of the last volume the device reported and
  the last one sent to it, so the glide waits for the device: a slow or silent device slows it, and
  a late answer resumes it from the answered value rather than catching up.

  A raise above 75 % is never held for later, it only moves the target. Lowering, and raising to 75
  % or below, is instant; a click or drag below where the glide has reached lowers at once and ends
  it, one above sets a new target and the glide goes on; any key stops it where it is, and `m` stops
  it and mutes; so does a mouse press anywhere but on the Volume bar, a wheel step anywhere (over a
  level it then steps one notch), a change of screen, and a box opening over the screen. Balance and
  Loudness have no glide
- on the device screen's Tone control pane, click a band row to select it, or its frequency, gain or
  Q to select that value's band as on the tone-control screen; a double-click there only selects,
  the edit line being drawn over the tone-control screen alone (`e` shows that screen), and the
  wheel over a value steps the band's gain as it does there. A click on any of the Tone pane's rows
  — Mode, Loudness or a band — also takes the keys to that pane, as `Tab` would, so `←→` then step
  what was clicked
- the wheel over a level (volume, balance), the loudness bar, or a band's row nudges it one step
  (above 75 % of the Volume's range no faster than the glide, as the keys) — a band's gain one of
  the device's own steps, as `←→` do, and in Flat mode switching the tone control to EQ, as the keys
  do; the wheel over the Host volume bar steps the host's volume 2 %, as `←→` there do
- on the device screen's Transport row, click a button — its glyphs or the cell either side of them
  — to press it on the host's player, focusing the row on it, as `Enter` there would
- the device editor, a removal question, the preset list, a band's edit line and the quit question
  are answered from the keyboard: while one is open the mouse reaches nothing under it, and a left
  click outside it presses `Esc`, which closes it or cancels what it asks — the same click closes
  the Settings box

Resting the pointer on anything clickable for 0.7 seconds shows a tooltip: one or two lines, at most
40 cells wide, saying what the target is and how to use it. It appears under the pointer's cell,
above it near the last row, on the pointer's other side where it would meet a toast (and not at all
where both sides do), and shifted left to stay on the screen, in the raised shade with no border
(reverse video where no raised shade is painted: at 16 colours, in monochrome and with the theme
background off). While one shows, moving straight onto another target shows that one at once; a key,
a click, a drag, the wheel, a resize, the pointer leaving its target or something else drawing over
it hides it, and none shows while a box is open or capture is off.

The targets are the tabs, `? help` and `esc …`, the words of a pane's border (the key table's
description and key: `mute · m`), the rows of the Controls pane (with the control's label and how to
use its kind), the Mode and Loudness halves, the bands, the Response curve, the Transport buttons,
the rows of the Telemetry and Devices lists, and the header's connection words,
`mouse:on`/`mouse:off` and fault glyphs; menu entries, the Settings box's rows and toasts have none.
The Telemetry tab's, on an 80×24 terminal:

```
 svolder  Gungnir 2  ● connected                                       mouse:on 
─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─
┌ Controls ───────── mut Telemetry · every reading · t ──────── flat ─ presets ┐
│   Input        —                   ││   Mode      ‹ — ›                      │
│ ▶ Phase invert [ ] —               ││   Loudness  —                          │
```

### Title meter

While a device streams its meters, the terminal's title — the window name, or the tab's — carries
one small bar per scaled meter, in the profile's own order, with the device's name behind them. One
channel draws a single wide bar filling from the left edge, ten cells: `(██████░░░░) svolder · Tyr`.
Two draw a pair facing each other: `(██░░░░░███) svolder · bifrost`, the first filling from the left
edge toward the centre and the second mirroring it from the right edge, the two touching with no gap
between them. Three to five each draw their own five-cell bar filling from the left, a single space
between each pair: `(████░ ░░░░░ █████) svolder · Kraken`.

Whatever the count, every cell of every bar is either a full block `█` or a light shade `░`, never a
space or a partial block within a bar, and every bar's level is rounded to the nearest whole cell,
so the title keeps the same width whatever it reads and the device's name behind the bars never
moves. The title is rewritten at most about three times a second (every 300 ms) so a terminal that
redraws its whole tab bar for every title is not hammered by a device reporting several readings a
second. The bars are decoration over a reading and never a reading themselves: nothing about them
delays or smooths what a meter said, and they follow the same levels the meter rows draw. While the
level comes from the host's capture instead (see "Host audio" above) — a DAC, which streams no
meters — the title carries the capture's left and right channels as that same facing pair:
`(███░░░░███) svolder · gungnir`.

Every scaled meter — one whose profile names a `scale` or a `range` to read it against — feeds its
own bar, filled to its place on the live part of its scale; a meter with neither streams no level
and carries none. While the meters are not live (switched off, or the amp in standby or warming up)
they carry none either. A device with no scaled meter at all streams no level, and with no capture
of the host's sink running either the title is the plain `svolder · <device name>`.

`title_meter = true` under `[tui]` is the default and `V` turns it off or back on from any screen,
as the Settings box's own **Title meter** row does; with it off the title is that plain form. On
exit the title svolder found is put back: it goes onto the terminal's title stack on the way in and
comes off it on the way out. A terminal that does not support titles at all simply shows nothing of
this, and inside tmux the outer terminal follows only with `set-titles on` in `tmux.conf` (tmux's
own window name is set by its `automatic-rename` rules, not by this).

### Help, tabs and borders (`?`)

Nothing is drawn along the bottom of a screen: the body runs down to the terminal's last row, and
the screen says where you are and what it takes in three places.

- **The tab strip** under the header names the four screens and marks the one showing, with `? help`
  and `esc menu` or `esc back` at its right end (see the top of "tui" above). Each tab carries the
  key that shows it as its first letter, bold.
- **The words on a pane's top border** name the keys that act on what the pane shows (see "Device
  screen" above): `mute`, `0 centre` and `refresh` on Controls, `flat` and `presets` on Tone control
  and the Response block, `advice` on Status, `0 flatten` on Bands, `refresh` on Telemetry, `new`,
  `update`, `x remove` and `refresh` on the Devices list. A word's key is its bold first letter, or
  the key itself where it is not a letter.

  A key that only sometimes does something on the screen showing (`f` and `p` with no tone control,
  `m` on a device whose profile names no mute, `0` on one with no balance) keeps its word, dim
  rather than dropped; the help overlay says why. The Devices screen's `d` is always worth pressing,
  whatever is configured, since it is where a device is added, changed, and removed as well as
  switched to.
- **`?`** names every other key. The arrows, `Tab`, `Enter` and `␣`, `+`/`=`/`-`, `0`, `1`-`N`, `,`
  `.`, `[` `]`, the shifted arrows and the six uppercase toggles `M`, `E`, `H`, `R`, `F` and `V` are
  drawn on no screen; the overlay lists them, and the uppercase toggles are also rows of the
  Settings box, whose right column names the key that turns the selected row from any screen.
  Nothing that works stops working for want of a word.

Tabs and words are mouse targets: clicking one is the same as pressing its key, and none of them is
a `Tab` stop — `Tab` and Shift-Tab move only between the screen's own panes — so a key with no word
is reached by its own letter.

`?` opens a full-screen overlay listing every key of every screen with a one-line description of
what it does, scrolled with the arrows, the page keys, `Home`/`End`, or the wheel when it holds more
than the terminal shows; `?` or `Esc` closes it. Its first three lines are the three key rules given
above, wrapped where the box is narrower than a rule.

Each key is named once: the keys more than one screen takes come first under **Everywhere** — the
six uppercase toggles one line each, named as their Settings rows name them (`M  mouse capture`,
`E  effects`, `H  hue drift`, `R  rainbow scroll`, `F  focus runner`, `V  title meter`), and last
`Ctrl-C  quit at once`, which the key table does not hold since its keys are pressed without a
modifier — then **Telemetry**, which ends the first column, and the rest is a section per screen,
the screen showing first (Telemetry itself, when it is the one showing, takes its place right after
Everywhere), heading the second column, each headed with the name of its tab (`Controls`, `EQ`,
`Telemetry`, `Devices`) and then `Settings` for the box's own keys, holding only the keys that
screen alone takes. The `Settings` section lists `Tab/⇧Tab  category` and `1-5  category` beside the
box's `␣/Enter  toggle`.

The `EQ` section ends on what the mouse does there (see "Mouse" above) — `L-drag  point freq/gain`,
`R-drag  Q, right widens` and `wheel  Q on the curve` on the curve, and `2×click  edit band value`
on the band table — dimmed with `mouse capture off` beside them while capture is off. A screen whose
keys are all in Everywhere gets no section of its own. Everything around the box — the header and
the screen under it — is drawn dimmed while it is open, every glyph kept but in the palette's grey
at 256 colours and truecolor, or the terminal's dim attribute at 16 colours and in monochrome, so
the box is the one thing at full strength.

The box itself is drawn in the theme's own colours, as a focused pane is: its border and title as a
focused pane's, the rules and what each key does in the text colour, and the keys in the accent; the
one grey left inside it is a key that does nothing on this device, beside the reason. The device
editor and the removal question do the same over the Devices screen, the preset list over the
tone-control and device screens, a band's edit line over the tone-control screen, and the quit
question `q` asks over any screen, and no animation plays over what they dim.

The sections run down two columns wherever two fit across the terminal, filling the first before the
second and keeping the two about as long as each other, and take more columns only when the terminal
is too short for two to hold every key; a section stays in one column unless it is longer than a
column, and a heading always has its first key under it. A key for a control a device lacks (`i`
phase, `o` NOS) is left out of the overlay on that device rather than dimmed. The overlay writes a
one-word description in full where the key row abbreviates it (`volume`, `frequency`, `telemetry`,
`devices`). The text keeps one blank cell inside the box's border on either side. The box on an
80×24 terminal, where the keys run past it:

```
┌ Keys · ? or Esc closes ────────────────────────────────────────────────────┐
│ Lowercase letters act on the device, or open a screen or the Settings box. │
│ Uppercase letters toggle a TUI mode and save it to the config.             │
│ Symbols and navigation keys are the same on every screen.                  │
│                                                                            │
│ Everywhere                        Controls                                 │
│ ↑↓        move                    ␣/Enter   toggle/press                   │
│ ←→        change                  +/=/-     volume                         │
│ m         mute                    0         centre balance                 │
│ i         phase                   a         advice, or all clear           │
│ o         NOS                                                              │
│ Tab/⇧Tab  pane                    EQ                                       │
│ f         flat on/off             1-5       band                           │
│ p         presets (a i h inside)  , . ⇧←→   frequency                      │
│ b         identify                [ ] ⇧↑↓   Q                              │
│ r         refresh                 Enter     edit band                      │
│ Esc       back, or the menu       0         flatten                        │
│ c         controls                L-drag    point freq/gain                │
│ e         tone                    R-drag    Q, right widens                │
│ t         telemetry               wheel     Q on the curve                 │
│ d         devices                 2×click   edit band value                │
└──────────────────────────────────────────────────────────────────── more ↓ ┘
```

`a` toggles the advice text under a fault in the device screen's Status pane, or with no fault
standing shows the step a failure named (once per failure) and then posts the all-clear toast. The
device editor and the removal question the Devices screen's `u` and `x` open (see "Devices screen"
above) take their own few keys, named on the overlay box itself rather than in the key table: `↑↓`
row, `←→` caret / step, `Space` flip the default, `Backspace`/`Home`/`End` edit, `Enter` save or
confirm, `Esc` cancel. The preset list `p` opens names its own keys on its box the same way (see
"The preset list" above), and a band's edit line names its own on the line itself (see "The edit
line" above).

### Effects

Animation is a polish layer, never the only carrier of a fact — every fact it decorates also shows
as plain text (a fault's row, say). It is on by default (`effects = true` under `[tui]`) and runs
only at truecolor, and only with colour on at all (never at 256, 16, or monochrome, since it draws
with RGB colours a lower depth cannot show faithfully) — all but the `noise` and `melt` transitions
below, which are drawn at every depth and in monochrome. With effects off, no effect code runs, no
transition plays, and the redraw rate falls back to the slow tick that keeps ages and the clock
current; `effects` under `[tui]` can also switch on or off one animation at a time (see
["Configuration file"](daemon.md#configuration-file)).

A small spinner is information rather than an effect, so it runs whatever `effects` says: the
braille cycle `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, one glyph every hundred milliseconds, in front of the words of a wait
the TUI states — the header's connection dot while the link is down, a device's `next attempt in Ns`
on the Devices screen, and the setup wizard's own waits (`connecting…`, `listening…`, and the rest).
It shows only while that wait lasts, turns at every colour depth and in monochrome, and keeps the
redraw rate at a hundred milliseconds or faster for as long as one is on the screen, whatever
`effects` says. The CLI's own output — `svolder devices`, `daemon status`, the summary — stays
plain, since scripts read it.

`transition` under `[tui]`, or the Settings box's **Transition** row, picks how one screen gives way
to the next:

- `trace` (the default): a pane's frame traces itself once, from its top-left corner round to the
  start over about 300 ms, when it first appears — on a screen change and on the first frame alike
  (after a full `particles` intro, a third of a second after it, while the sparks fly) — with its
  content sweeping in behind it; a screen with no frame to trace (the help overlay as it opens)
  sweeps its body in from the left instead, and a palette or a colour-depth change earns the same as
  a screen change.

  The squirrel, if it stands in a pane that is drawing itself, waits for it: the pane traces and
  sweeps in as if the squirrel were not there, and the squirrel appears whole, feet and all, once
  the pane is in, about 600 ms after the screen appears. It runs where the rest of the layer does:
  at truecolor only.
- `noise`: on a screen change only, the new screen shows through a field of shade glyphs (`░▒▓`) in
  the palette's spectrum colours, the covered share of cells falling from all to none over about 250
  ms, each cell's moment a hash of where it is, so it plays the same way every time; at 16 colours
  and in monochrome the glyphs are dim instead.
- `melt`: on a screen change only, the last frame the terminal showed slides down off the bottom
  column by column, the Doom wipe, revealing the new screen above it: each column starts after a
  small delay of its own, then falls at the speed every column falls at, and the whole takes about
  600 ms at any terminal size.
- `none`: the new screen is simply there.

A transition never holds anything back: the new screen is drawn in full beneath the noise or the
melt every frame, a key pressed while one plays acts on the new screen at once, and a reading
arrives as it arrives. A second screen change while one plays cuts it short and starts its own — a
melt then melting the frame the first had got to. A modal box opening or closing, a resize and a
switch to another device are not screen changes and play no noise or melt, and a resize while one
plays ends it.

Always on when the layer runs: a red flash of the header on a new fault, and a fade-in for the
readings that arrive after a reconnect. Each of the following has its own switch, all defaulting to
on except the last two:

- *Eased values* (`effects.eased`): a changed bar glides to its new length over about 200 ms, and
  the row it is on glows briefly in the accent.
- *Selection glow* (`effects.glow`): the row the cursor just left keeps a 300 ms fading trail.
- *Shimmer* (`effects.shimmer`): a bright band crosses a row at a steady 80 cells per second (a wide
  row takes longer than a narrow one), as feedback on what the user just did — the row a key of
  theirs wrote to, or the row the cursor moved onto, whichever selection the keys are moving: a
  control row, a Telemetry row, a device on the Devices list, an entry of the menu, a row of the
  Settings box, a preset or an AutoEq result. A reading the device pushed by itself never sweeps:
  the row shows it moving already, and a resize sweeps nothing.

  While the keyboard rests, the selected row is swept once more 2 s, 10 s, 30 s and 2 minutes after
  the last key and then left alone until the next one, so an untouched screen goes quiet; the rows
  under the menu, the Settings box or the preset list never sweep, only the selection of the box
  itself, and nothing sweeps under the help overlay, the device editor, a removal question or a
  band's edit line, which have no selection, nor on the setup wizard.
- *Breathing borders* (`effects.breathing`): the contours of every screen's frames, the Controls,
  EQ, Telemetry and Devices screens alike (on `red`, only the one the keys are in, the others being
  faint) pulse with the level — the amp meters' smoothed level while they stream, else the host
  capture's (see "Host audio" above) — and stay still with neither: no capture of the host's sink
  running, and a device that reports no meters, whose meters have no scale, or whose meters are not
  live. With the daemon's frames (see "Host audio" above) each beat also lifts the level the
  contours are shaded by for about 0.2 s, and the Spectrum row's bars go bold for as long. To see an
  amp's meters move them with no amp, run `--profile fake-amp --signal` on the simulator, or
  `SVOLDER_FAKE_SIGNAL=1` with `SVOLDER_FAKE_PROFILE=fake-amp` on the in-process fake (see
  ["The simulator's signal"](bluetooth.md#the-simulators-signal)).
- *Focus runner* (`effects.frames`, the key's name unchanged): a three-cell band of light runs once
  round the frame of the pane that has the focus, on a focus change, and once round the frame the
  keys are in when a screen is arrived on, starting when the way in has played out (the trace's 0.3
  s, the noise or the melt, at once with no transition); never on a modal overlay or the Setup
  wizard; how a pane arrives is the transition's, whichever way this switch is. Its brightness and
  pace follow the level where there is one, the meters' or the host's: the spectrum's hot end and
  about 0.4 s a lap at the loudest, and with none, the lift of the frame's own colour and about 1.2
  s a lap.
- *Fault presence* (`effects.fault_presence`): while a fault stays active, its row and the header
  border pulse slowly.
- *Peak hold* (`effects.peak_hold`): a marker lingers at a meter's recent peak and decays over about
  1 s — inert on meters with no scale and while the meters are not live, as breathing is.
- *List fade* (`effects.list_fade`): the rows of a list dim a little with their distance from the
  selected row, as btop does — the Telemetry, Devices and preset lists and the Settings box's rows.
  A row within two of the selection keeps its colours; further out, every colour on it blends toward
  the palette's dim grey by a third of the way at most, growing evenly to that at ten rows away. The
  selected row never fades, and a list scrolled past its window counts distance from the selected
  row's own place in the list.
- *Hue drift* (`effects.hue_drift`, a mode rather than a fixed decoration, off by default): the pane
  borders and titles, the rule under the header, and the key hints drift along the palette's
  spectrum — cool to hot — in place of the resting accent; the selection and every value stay in the
  resting accent throughout. On `red`, whose titles and unfocused frames are off-white and faint
  rather than in the accent, the frame the keys are in, the rule and the key hints drift. With no
  level the drift stays in the cool half, one round trip every 20 s; with one — the meters', or the
  host's — it moves toward the hot end as the level rises and back as it fades. `[tui.spectrum]`'s
  `stops` (see ["Configuration file"](daemon.md#configuration-file)) give a palette its own spectrum
  in place of the built-in one.
- *Rainbow scroll* (`effects.spectrum_drift`, a mode rather than a fixed decoration, off by
  default): the tone-control curve's own colours scroll slowly round the hue circle, one round trip
  every 20 s, rather than resting red at 20 Hz through to violet at 20 kHz — on `red` and `neon`,
  whose curves are ramps rather than the circle, the ramp slides along its stops and back instead,
  once each way per round trip; independent of hue drift, which is the decoration's own drift rather
  than the curve's.

The tone-control curve itself is coloured as a spectrum by frequency — red at 20 Hz through to
violet at 20 kHz while the rainbow scroll is off, on `red` a warm ramp from dark red through scarlet
and amber to off-white, and on `neon` a sunset from violet through hot pink and orange to gold —
whether or not the layer runs: the colour names the frequency the column draws, so it is part of
what the curve says, and the layer's own part, with the rainbow scroll on, is the slow drift of that
spectrum round the hue circle, or along the ramp on `red` and `neon`.

A curve the bands shape (EQ mode, a band off 0 dB) takes the whole of that: the drift carries it
round the circle, or along the ramp. A muted curve — a flat curve in EQ mode, or Flat mode, whatever
the bands hold — is coloured too, its hue kept but desaturated toward its own grey and darkened a
little rather than blended toward a fixed one, and it rests red to violet, or on its ramp on `red`
and `neon`, whatever the rainbow scroll is doing: nothing carries a curve the bands are not applying
round the circle or along the ramp. A terminal at 16 colours, or with colour off, has no spectrum to
draw in at all and keeps the curve — shaped or muted — in whichever role the widgets drew it in.

A bar never glides for a meter: a meter's reading is a fact the device reports moment to moment, not
a value it confirmed changing, so its fill always tracks the raw reading directly.

A squirrel, Ratatoskr, lives in svolder. Now and then it looks in when something happens, and once
it is called it sits in the corner of the Tone pane (the Controls pane on a device without tone
control), beside any rows short enough to leave that corner free; where the pane's text reaches the
corner, it waits elsewhere. `mascot = false` under `[tui.effects]` keeps it away; so do effects off,
16 colours and monochrome. Under a box that dims the screen it keeps its shading, in greys.

### Exit codes

`q` exits 0. Before opening a backend, `svolder tui` exits 1 if stdout is not a terminal or the
configuration file fails to load; once running, it exits 1 if a frame fails to draw or the
terminal's input stream ends or errs. Opening the device at startup follows the same exit codes as
every other verb (see "Exit Codes" above) for the kind of failure: 2 not found, 3 connection failed,
4 timeout, 1 a usage or protocol error. Ctrl-C or `q`, pressed while the device is still opening,
also exits 0, after printing `svolder: interrupted before the device answered` (see above).

A failure switching device from the `d` list does not exit the TUI: it reopens the device that was
showing and reports the failure in a Failure toast instead. The Setup wizard (see "Setup wizard"
above) leaves with 0 wherever it is quit, with the exception of a write the machine itself refuses —
a taken configuration write, or an install the host would not let happen — which leaves with that
failure's own code and words on stderr once the terminal is restored.
