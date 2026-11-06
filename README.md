# svolder

Control Schiit Audio devices that carry the Forkbeard Bluetooth LE module from Linux: a CLI, a TUI,
a background daemon, a web UI, Home Assistant over MQTT and an MCP server for AI assistants. Named
after the Battle of Svolder, won by Sweyn Forkbeard, son of Harald Bluetooth.

<img src="docs/images/svolder.avif" width="100%" alt="svolder starting in a terminal: sparks build
the svolder wordmark and fly into the header of a Gungnir 2's screen, where So What by Miles Davis
plays; the volume steps up, then the tone-control screen shows the Sennheiser HD 800 S correction
as one band rises">

<sub>[Text version](docs/images/svolder.txt)</sub>

We come in peace: svolder is a fan's homage to Schiit's engineering, and Schiit Audio has not made,
endorsed or reviewed it (see [LEGAL.md](LEGAL.md)). If you find it useful, the best way to say
thanks is to buy Schiit. It knows Schiit's Forkbeard DACs, preamps, amplifiers and phono stages
through a profile per model.

## Before you plug in

svolder is beta software. It has been tested against a fake device and a Bluetooth simulator that
speaks the Forkbeard protocol, and has not yet driven real Schiit hardware. Until it has, treat
every first run as a test:

- Turn the volume down before svolder connects, on the device or on the amplifier after it, and
  start low.
- Try it with cheap headphones or speakers first, and keep headphones off your ears while you try a
  new device, a new version of svolder or a new feature.
- Cap the ways in from other devices: `max_volume_db` under `[web]`, `[mqtt]` and `[mcp]` in
  `config.toml` refuses any volume above it.
- If something sounds or looks wrong, take the headphones off, turn the volume down on the device,
  and stop svolder with `svolder daemon stop`.
- Tell us what happened, with the output of `svolder doctor`, in an
  [issue](https://github.com/RRAlex/svolder/issues).

[LEGAL.md](LEGAL.md) says the same, and you use svolder at your own risk.

## Screenshots

### Terminal

<img src="docs/images/dac.webp" width="100%" alt="The device screen of a Gungnir 2: its controls
with the volume bar's clipping-risk zone, the host's volume, transport and spectrum, the tone
control, and the status with the link history, the level's source and Now playing So What by Miles
Davis">

<sub>[Text version](docs/images/dac.txt)</sub>

<img src="docs/images/tone.webp" width="100%" alt="The tone-control screen of a Gungnir 2 with the
Sennheiser HD 800 S correction from AutoEq, measured by oratory1990, fitted to its five bands: the
response curve, the correction's target and its filters">

<sub>[Text version](docs/images/tone.txt)</sub>

<img src="docs/images/amp.webp" width="100%" alt="The device screen of an Aegir 2 amplifier,
powered on: its controls, its L and R meters as bars with a class-A block, the live part filled and
a percentage beside each, five minutes of their mirrored history, and the host's audio">

<sub>[Text version](docs/images/amp.txt)</sub>

<img src="docs/images/telemetry.webp" width="49%" alt="The Telemetry screen: every reading an Aegir
2 sends, with its type, value, age, rate and recent history"> <img src="docs/images/devices.webp"
width="49%" alt="The Devices screen listing a Gungnir and an Aegir 2, both connected">

<sub>Text versions: [telemetry](docs/images/telemetry.txt), [devices](docs/images/devices.txt)</sub>

### Web

<img src="docs/images/web-phone.webp" width="32%" alt="The web UI on a phone, the first screen: the
svolder wordmark and the connected pill, the folded Now playing line (cover, title and artist, a
pause button, a next button and a thin progress bar), a Gungnir 2's knob at −24.1 dB with its −,
Mute and + buttons and the clipping-risk note, the visualizer card with its bars, and the top of its
controls, Phase invert"> <img src="docs/images/web-desktop.webp" width="66%" alt="The web UI on a
desktop: the Now playing band across the top, with the cover, the title, the transport, the time and
the signal path on the left and the visualizer on the right; below it the same Gungnir 2 with the
knob, its buttons and its controls on the left and the tone control in EQ mode on the right with its
curve, the band table and the preset list; and under the footer a strip of recent volume bars with
the marks of a Warm and a Sennheiser HD 800 S recall">

### Web themes on a phone

<img src="docs/images/web-phone-paper.webp" width="32%" alt="Paper on a phone: squared paper with a
serif Gungnir 2 title, the volume −26.1 dB in red with a ruler, written controls (− quieter, mute,
louder +), the tone graph with its five bands and the first rows of the band table"> <img
src="docs/images/web-phone-signal.webp" width="32%" alt="Signal on a phone, scrolled: the signal path as a
strip along the top with PipeWire, the Gungnir 2 chosen and an amp, then the tone graph, the mode,
the input and the presets, with the Now playing bar pinned at the bottom"> <img
src="docs/images/web-phone-nightdrive.webp" width="32%" alt="Night drive on a phone: a striped sun
over a dusk sky, the live spectrum as cyan mountains and a floor of lit cells rolling toward you, the
volume −26.1 dB as a large neon number under the device line and the EQ gains, and under the scene
the strip with − Mute +, the input, the track, the transport and Device">

## Install

Get the current version from the [release page](https://github.com/RRAlex/svolder/releases/latest),
for x86_64 and arm64, each file listed in `SHA256SUMS`:

- Debian, Ubuntu, Raspberry Pi OS: `sudo apt install ./svolder_<version>_amd64.deb`
  (`_arm64.deb` on arm64, Raspberry Pi OS 64-bit)
- Fedora, RHEL: `sudo dnf install ./svolder-<version>-1.x86_64.rpm`
- Arch: `PKGBUILD` and the tarball in one folder, then `makepkg -si`
- Nix: `nix run github:RRAlex/svolder/v<version>`
- Any Linux: the `x86_64-linux` or `aarch64-linux` tarball; static binaries, BlueZ only
- Container: `docker pull ghcr.io/rralex/svolder:<version>`

A package enables nothing: turn the daemon on with
`systemctl --user enable --now svolderd.socket svolderd.service` (from a tarball:
`svolder daemon install`). [Getting started](docs/usage/getting-started.md) and
[The daemon](docs/usage/daemon.md) have every file, a machine nobody logs in to and the container.
To build from source, run `make deps` then `make build` (Rust 1.93 or later);
[CONTRIBUTING.md](CONTRIBUTING.md#build) lists the tools.

## First steps

    svolder setup                 # scan, name the device, offer to install the daemon
    svolder                       # the TUI, or the setup wizard the first time
    svolder doctor                # check the host and the daemon, and offer fixes
    svolder status                # every control's value
    svolder set volume -20dB      # set a control by its key; `svolder set mute on`
    svolder eq show               # the tone control; `eq load`, `eq set`, `eq search` AutoEq
    svolder web                   # the web UI, signed in; `web --qr` for a phone
    svolder daemon install        # keep every device connected for the whole login

No device at hand: `svolder tui --demo` runs the TUI on a fake device and a throwaway daemon, and
touches nothing of yours. `make demo` adds `--host --web`: the same fake device with your computer's
real audio (volume, player, EQ) and the web UI on a free port, for trying the themes with your own
music; `make demo-safe` is the plain one.

### Home Assistant

The daemon publishes each device to an MQTT broker, and Home Assistant's MQTT integration finds it
on its own. Add a section to `~/.config/svolder/config.toml`:

```toml
[mqtt]
enabled = true
host = "192.168.1.10"     # the broker, such as Home Assistant's Mosquitto add-on
username = "svolder"      # and SVOLDER_MQTT_PASSWORD in the daemon's environment
commands = true           # let Home Assistant change things; leave it out for read-only
max_volume_db = -20.0     # Home Assistant never sets the volume above this
```

Then run `svolder daemon restart`; `svolder daemon status` says when the bridge is connected. With
the MQTT integration on the same broker, each device appears with its controls.
[contrib/home-assistant](contrib/home-assistant/README.md) has the broker and login steps and a
dashboard to paste in, and [MQTT and Home Assistant](docs/usage/mqtt.md) has every entity and
topic.

<img src="docs/images/ha-dashboard.webp" width="78%" alt="The svolder dashboard in Home Assistant
for a Gungnir 2, in the svolder theme: the volume slider, the input with its balance gauge, the tone
control on the Warm preset, the input readouts and output switches, and the five bands' gain,
frequency and Q"> <img src="docs/images/ha-phone.webp" width="20%" alt="The same dashboard on a
phone: the volume, the source, the tone control, the inputs and the output switches in one column">

### AI assistants (MCP)

The daemon can serve the Model Context Protocol on this machine, read-only unless you allow writes:

```toml
[mcp]
enabled = true            # 127.0.0.1:8737; add writes = true to let it change things
```

Run `svolder daemon restart`, then register it with Claude Code, pasting the token that
`svolder mcp token` prints:

    claude mcp add --transport http svolder http://127.0.0.1:8737/mcp --header "Authorization: Bearer <token>"

[The MCP server](docs/usage/mcp.md) covers other clients, the volume rule and the tools.

## Where the details live

- [Usage index](docs/usage/README.md): every page below, with the transports in one paragraph.
- [Getting started](docs/usage/getting-started.md): install, the front door and the setup wizard.
- [Device verbs](docs/usage/cli.md) and [Equaliser and presets](docs/usage/eq.md): the CLI.
- [The terminal UI](docs/usage/tui.md): every screen and key, themes, effects and the mouse.
- [The web UI](docs/usage/web-ui.md): turning it on, signing in, a phone, its limits, its themes
  (Ember, Paper, Signal and Night drive), the theme menu and Arrange.
- [MQTT and Home Assistant](docs/usage/mqtt.md) and [The MCP server](docs/usage/mcp.md).
- [The daemon](docs/usage/daemon.md), [the configuration file](docs/usage/config.md) and
  [Transports and Bluetooth](docs/usage/bluetooth.md): the packages, containers, the simulator.
- [Exit codes](docs/usage/exit-codes.md) and [environment variables](docs/usage/environment.md).
- [Known issues](docs/known-issues.md), [the protocol](docs/protocol.md) and
  [the architecture](docs/architecture.md): limits, the wire format and how the code fits.
- [CONTRIBUTING.md](CONTRIBUTING.md): build, test and propose a change; add a device.

## Licence

svolder is free software, under the GNU General Public License, version 3 or (at your option) any
later version, and comes with no warranty: see [LICENSE](LICENSE); [AUTHORS](AUTHORS) says who holds
the copyright. [LEGAL.md](LEGAL.md) explains how svolder was developed, how it stands with Schiit
Audio (it is independent of it), and why a new device or version should be tried at a low volume.
