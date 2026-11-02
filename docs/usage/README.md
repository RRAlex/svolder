<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# svolder usage

The `svolder` CLI provides verbs to scan for devices, inspect their status, and control them —
through `svolderd`, the background daemon, when one is running, or directly over a BlueZ adapter
otherwise. Pass `--transport fake` (or set `SVOLDER_TRANSPORT=fake`) to talk to an in-process fake
Bluetooth Low Energy device instead of real hardware: a Gungnir 2 (profile `gungnir2-v2`), unit id
`0102030405060708`, firmware 3, with passphrase `1234` set. See
["Transport and Defaults"](bluetooth.md#transport-and-defaults) for how a daemon or a direct session
is chosen, ["Daemon and config"](daemon.md#daemon-and-config) for `svolderd` and the configuration
file both sides read, and ["MQTT and Home Assistant"](mqtt.md#mqtt-and-home-assistant) for
publishing the daemon's devices to a broker, and ["MCP server"](mcp.md#mcp-server) for letting an AI
assistant read and set them.

## The files

- [Getting started](getting-started.md): the guided walk from the first launch to a connected
  device.
- [Transports and Bluetooth](bluetooth.md): how a verb picks the daemon or a direct session, real
  Bluetooth, the simulator and the adapter choice.
- [The daemon](daemon.md): running `svolderd`, release builds and packages, containers, logging and
  the `daemon` verbs.
- [The configuration file](config.md): the file both sides read, its template and the `config`
  verbs.
- [MQTT and Home Assistant](mqtt.md): publishing the daemon's devices to a broker.
- [The MCP server](mcp.md): letting an AI assistant read and set devices, and the `mcp` verbs.
- [The web UI](web-ui.md): the browser front end, signing in, and the `web` verbs.
- [Device verbs](cli.md): `doctor`, `setup`, `devices`, `add`, `remove`, `hops`, `passphrase`,
  `scan`, `inspect`, `capture`, `status`, `get`, `set`, `preset`, `watch`, `host`, `raw`, `decode`
  and `profiles`.
- [Equaliser and presets](eq.md): the `eq` verbs and the preset library.
- [The terminal UI](tui.md): every screen and key of `svolder tui`.
- [Exit codes](exit-codes.md): what each verb returns.
- [Environment variables](environment.md): every `SVOLDER_*` variable and what reads it.
