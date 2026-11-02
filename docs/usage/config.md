<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The configuration file

The `config` verbs: where the configuration file lives, how to show it, and the commented template
that `config template` prints. Back to the [usage index](README.md).

## config

Inspect the configuration file, or print the template a new one starts from. None of the three
subcommands opens a Bluetooth transport or a daemon connection.

### config path

Print the configuration directory.

```
Print the configuration directory

Usage: svolder config path [OPTIONS]

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
$ svolder config path
/home/user/.config/svolder
```

### Hops

A `[[hop]]` block declares one stage the sound goes through after the DAC. It carries exactly one of
`name` (which needs a `kind`: `amp`, `headphones`, `speakers` or `other`, in lower case only) and
`device` (the name of a configured `[[device]]`, which gives the hop its name), and optionally a
free `note` and `after`, the configured device whose output it follows. The blocks are listed in
file order, and no two may share a name or a device, compared with case and the blanks round them
aside. A `device` or an `after` that names no configured device loads with a warning, so removing a
device never breaks the file. `svolder hops` (see [Device verbs](cli.md#hops)) lists, adds, links,
edits, removes and moves them.

### config show

Print the loaded configuration: the file's path, then either `no config file` or a readable
rendering of every top-level setting, the `[mqtt]`, `[mcp]` and `[web]` blocks when the file sets
anything in them, every device block and every hop block. `--json` prints the loaded config as one
JSON object instead. The MQTT broker's password is printed as `***` in both, so a configuration
pasted into a bug report never hands out the broker login.

```
Print the loaded configuration

Usage: svolder config show [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print the loaded config as one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, with two devices configured:

```
$ svolder config show
/home/user/.config/svolder/config.toml

[[device]]
name = bifrost
udid = 1A2B3C4D5E6F7A8B
profile = bifrost3
passphrase = auto

[[device]]
name = gungnir
udid = 0102030405060708
passphrase = auto
```

`--json` carries a `daemon` object alongside `tui` — `{"daemon":{"autospawn":true}, ...}` — the same
key the file's `[daemon]` table (see ["Configuration file"](daemon.md#configuration-file)) is read
from.

### config template

Print a whole configuration file with every key commented out at the value svolder uses while the
key is absent — the same text `add`, the wizard and a Settings save start a new file from.
Uncommenting a line on its own changes nothing, but for the three keys whose absence has no spelling
to show: `[mqtt]`'s, `[mcp]`'s and `[web]`'s `max_volume_db`, shown at an example cap, and
`[tui.spectrum]`'s `stops`, shown at an example spectrum.

```
Print a configuration file with every key commented out at its default

Usage: svolder config template [OPTIONS]

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
$ svolder config template
# The svolder configuration. Every key below is commented out at the value svolder
# uses while the key is absent, so uncommenting a line on its own changes nothing.

schema = 1

# The Bluetooth adapter to talk through, named the way BlueZ names it.
# adapter = ""

# The device a command acts on when it names none; one of the devices below.
# default_device = ""

# The socket the front ends and the daemon meet on, overriding where svolder looks.
# socket = ""

# How a command that needs a daemon gets one.
[daemon]

# Whether a command that finds no daemon listening starts one of its own.
# autospawn = true

# svolderd's bridge to an MQTT broker for Home Assistant, off until enabled with a host.
[mqtt]

# Whether the bridge runs at all.
# enabled = false

# The broker's host.
# host = ""

# The broker's port.
# port = 1883

# The broker username, when it wants one.
# username = ""

# The broker password, when it wants one; SVOLDER_MQTT_PASSWORD overrides this.
# password = ""

# The client id to connect with; the default is svolder-, the host name and a hash of the socket.
# client_id = ""

# The topic prefix every topic is built under.
# base_topic = "svolder"

# The discovery prefix Home Assistant watches; empty turns discovery off.
# discovery_prefix = "homeassistant"

# Whether the bridge takes commands back, through the same validation `svolder set` uses.
# commands = false

# An optional cap, in dB, on a volume command's value; no cap while absent.
# max_volume_db = -10.0

# svolderd's MCP server for AI assistants, over HTTP, off until enabled.
[mcp]

# Whether the server runs at all.
# enabled = false

# The address it listens on; the default is loopback only.
# listen = "127.0.0.1:8737"

# "token" asks for a bearer token (`svolder mcp token` prints it); "none" is for a loopback listen only.
# auth = "token"

# Whether the tools that change a device are offered.
# writes = false

# An optional cap, in dB, on a volume the server will set; no cap while absent.
# max_volume_db = -10.0

# Browser origins allowed to call the server, such as "https://claude.ai:443" (a port, or :*, is required); none while empty.
# origins = []

# Host names clients reach the server by (a reverse proxy's, say), such as "mcp.home.lan"; an entry without a port matches any port. Added to the listen address and the loopback names; with none set, a wildcard listen address accepts any Host.
# hosts = []

# A file holding the token, in place of the generated one; SVOLDER_MCP_TOKEN outranks it.
# token_file = ""

# svolderd's web UI for a browser, off until enabled.
# To reach it from a phone, listen on "0.0.0.0:8738": that is plain HTTP, and anyone who can watch your network can copy a sign-in.
[web]

# Whether the daemon serves the web UI.
# enabled = false

# The address it listens on; the default is this computer only.
# listen = "127.0.0.1:8738"

# "token" asks a browser to sign in (`svolder web` opens one signed in); "none" is for a loopback listen only.
# auth = "token"

# Whether the page may change a device; false makes a read-only dashboard.
# writes = true

# An optional cap, in dB, on a volume the page sets; no cap while absent.
# max_volume_db = -10.0

# "read" lets anyone who reaches the page look, read-only; "none" shows them the sign-in page.
# guest = "none"

# The address links and QR codes carry, such as "https://svolder.home.lan/"; worked out from listen while empty.
# url = ""

# Host names browsers reach the page by (a reverse proxy's, say); with none set, a wildcard listen address accepts any Host.
# hosts = []

# Origins a reverse proxy presents besides the page's own, such as "https://svolder.home.lan".
# origins = []

# A file holding the token, in place of the generated one; SVOLDER_WEB_TOKEN outranks it.
# token_file = ""

# One block per device. `svolder setup` and `svolder add` write these for you.
# [[device]]
# The name the front ends pick this device out by.
# name = "bifrost"
# The device's 8-byte unit id, as sixteen hex characters.
# udid = "1A2B3C4D5E6F7A8B"
# The profile to drive it with, when it differs from the one svolder detects.
# profile = "bifrost3"
# The pairing passphrase: auto, none, or four digits.
# passphrase = "auto"

# One block per preset. `svolder eq save` writes these for you.
# [[preset]]
# The name the tone screen and `svolder eq` pick it by, at most 40 characters.
# name = "HD650 warm"
# How many bands the tone controls it fits have: 3 or 5.
# bands = 5
# The mode it puts the tone control in: flat, eq or loudness.
# mode = "eq"
# The loudness strength, 0-127.
# loudness = 0
# One entry per band, in band order: frequency in Hz, gain in dB and Q.
# band = [
#   { f = 40, g = 3.0, q = 0.7 },
#   { f = 160, g = 1.5, q = 0.7 },
#   { f = 632, g = 0.0, q = 4.3 },
#   { f = 2515, g = 0.0, q = 4.3 },
#   { f = 10000, g = 2.0, q = 0.7 },
# ]

# An imported correction (`svolder eq import`) holds filters instead of bands, and
# fits every tone control: type is PK, LSC or HSC, f in Hz, g in dB and q.
# [[preset]]
# name = "Sennheiser HD 650"
# origin = "AutoEq"
# preamp = -6.1
# filter = [
#   { type = "LSC", f = 105.0, g = 6.0, q = 0.7 },
#   { type = "PK", f = 180.0, g = -2.1, q = 0.9 },
# ]

# What the sound goes through after the DAC, in order. `svolder hops` writes these.
# A hop is either named by hand, with a kind (amp, headphones, speakers or other), or
# linked to a configured device; `note` and `after` (the device it follows) are optional.
# [[hop]]
# name = "HD 650"
# kind = "headphones"
# note = "balanced"
# after = "bifrost"

# [[hop]]
# device = "vali"

# The AutoEq index `svolder eq search` reads, kept in the cache directory.
[autoeq]

# Weeks (1 to 99) before the cached index is fetched again, or "never".
# refresh_weeks = 13

# Whether svolderd watches the computer's music player and the route its sound takes to the DAC.
[host]

# Whether the daemon reads the player and the route.
# enabled = true

# How `svolder tui` looks and behaves.
[tui]

# The colour preset the screens draw from: amber, ice, red or neon.
# palette = "amber"

# One accent for every device family at once, as an ANSI colour name or #rrggbb.
# accent = ""

# The colour depth to render at: auto, 16, 256 or true.
# colors = "auto"

# Whether to draw without colour, marking the selection by reverse video alone.
# monochrome = false

# Whether each palette paints its own dark greys under the screen, its panes and its boxes.
# theme_background = true

# Whether mouse capture starts on.
# mouse = true

# Whether the wordmark intro plays while the device opens.
# intro = true

# Which effect builds the wordmark intro: random, fade or particles.
# intro_effect = "random"

# Whether a short sound plays with the wordmark intro, through PipeWire's pw-play.
# intro_sound = false

# How one screen gives way to the next: none, trace, noise or melt.
# transition = "trace"

# Whether the terminal's title carries one bar per channel of the device's meters, else of what the host plays.
# title_meter = true

# Whether q asks before svolder tui quits; Ctrl-C quits at once either way.
# confirm_quit = true

# Whether svolder tui shows what the host plays, with its volume and transport.
# host_audio = true

# Whether the device screen draws a spectrum of what the host plays.
# visualizer = true

# Whether the screens draw their graphs: Telemetry, the level history and the connection strip.
# graphs = true

# How much history the Telemetry and level graphs span: 1m, 2m, 3m, 5m, 10m, 15m, 30m or 60m.
# graph_span = "5m"

# How much history the connection strip spans: 1h, 3h, 6h, 12h or 24h.
# link_span = "12h"

# Which of the animations run.
[tui.effects]

# Whether the animation layer runs at all.
# enabled = true

# Whether a changed value's bar glides to its new length and its row glows.
# eased = true

# Whether the row the cursor left keeps a fading trail.
# glow = true

# Whether a highlight sweeps the row you change, and the selected row while you rest.
# shimmer = true

# Whether panel contours pulse with the level: the amp's meters, else what the host plays.
# breathing = true

# Whether a light runs once round the frame the keys move or arrive to.
# frames = true

# Whether a standing fault's row and the header border pulse.
# fault_presence = true

# Whether a marker lingers at a meter's recent peak.
# peak_hold = true

# Whether the decoration drifts along the palette's spectrum, glowing hotter as the level rises.
# hue_drift = false

# Whether the tone curve's colours scroll: round the hue circle, or along red's and neon's ramps.
# spectrum_drift = false

# Whether the rows of a list dim with their distance from the selected row.
# list_fade = true

# mascot: Ratatoskr, the squirrel that comes out when called.
# mascot = true

# An accent for one device family at a time, which `accent` overrides.
[tui.accents]

# The accent a DAC draws with.
# dac = ""

# The accent an amplifier draws with.
# amp = ""

# The accent a preamplifier draws with.
# preamp = ""

# The stops the glow travels, cool end to hot end, in place of the palette's own.
[tui.spectrum]

# Three to five #rrggbb colours; the glow rests on the one nearest the DAC accent.
# stops = ["#5a1a08", "#c04a10", "#e0a030", "#f6dca0"]
```
