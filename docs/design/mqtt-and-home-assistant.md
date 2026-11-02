<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# MQTT and Home Assistant

The daemon can publish every device it holds to an MQTT broker, announced so that Home Assistant
creates an entity for each control with no YAML, and, when the owner allows it, take commands back.
This page is how the bridge is built, how a profile becomes entities, and the decisions that keep a
network front end from becoming a way to hurt a speaker. The map's [crate
table](../architecture.md#the-crates) places `svolder-mqtt`; the writer every command goes through
is in [daemon.md](daemon.md); the settings and the entity table a user needs are in the [MQTT
reference](../usage/mqtt.md) and `contrib/home-assistant/README.md`.

## Two halves

**`svolder-mqtt` is pure, apart from one client.** `topics.rs` names every topic, `payload.rs`
builds the state and meter JSON, `discovery.rs` and `eq.rs` build the discovery configs,
`command.rs` and `eq.rs` parse a command payload, and `client.rs` is the one thing that touches a
network: a `rumqttc` event loop over plain TCP. None of the pure parts reads a clock or a socket, so
each is tested with fixtures and golden files (`crates/svolder-mqtt/tests/discovery_goldens.rs`,
`tests/goldens/`).

**`bins/svolderd/src/mqtt_bridge.rs` is the daemon's side.** One task, `Bridge`, started and
stopped by the daemon's reload through a slot (`Daemon::bridge_to`), reads each device only
through its actor's `Feed` and writes only through the device's `Writer` as `Origin::Mqtt`. It never
touches a `Session`.

## What is published

For a device with unit id `U` (sixteen hex characters) and base topic `B` (`topics.rs`):

- `B/bridge/availability`: `online` or `offline`, retained, with `offline` as the connection's last
  will.
- `B/U/availability`: `online` while the daemon holds the device's link, retained.
- `B/U/state`: one retained JSON object with a key per exposed control (`payload::state`), in the
  units every other front end shows: volume in dB, toggles `ON` or `OFF`, selects and readouts by
  their label. A control nothing has reported is left out, never guessed. A burst of readings is one
  publish, at most every `STATE_EVERY` (100 ms).
- `B/U/signal/state`, `B/U/signal/attributes`, `B/U/firmware/state`, `B/U/dsp_firmware/state`: the
  diagnostics, retained, each its own plain text or JSON and each published only when its payload
  differs from the last one (`publish_diagnostics`, `mqtt_bridge/state.rs`). They are not in
  `state`, because none is a profile control; see "Diagnostics" below.
- `B/U/meters`: the meters (`payload::meters`), not retained, at most every `METERS_EVERY` (1 s). A
  meter with a scale is a whole percentage of it, and 0 while the amp's meters are off.
- `B/U/<key>/set`: where a command arrives; subscribed only with `commands = true`.
- Discovery: one retained config per entity at `<prefix>/<component>/svolder_<U>/<key>/config`,
  unless the prefix is empty, which turns discovery off. A device is one Home Assistant device
  holding one entity per control, a connectivity entity for the link, the diagnostics and, with
  commands, the identify button.

### From profile to entity

`exposed_controls` (`crates/svolder-mqtt/src/discovery.rs`) is the list of what a device exposes:
every profile control whose function id is of the `Normal` class, in profile order, with the
tone-control mode appended as a select. `component_for` maps a kind to a Home Assistant platform:
with commands on, a level is a `number` (a slider whose bounds and step come from `units::bounds`),
a toggle a `switch`, a select a `select`; without commands they are a `sensor` or `binary_sensor`,
which is what makes read-only the default. A meter or readout is a sensor, a boolean readout a
binary sensor, a fault a binary sensor. So a control added to a profile appears in Home Assistant
with no Rust: this list is the one the bridge also publishes from.

The tone control is not a profile control per band, so `eq.rs` models it separately: a preset select
(`eq_preset`, offering the presets that fit the device and `custom` last), the loudness strength
(`eq_loudness`) and, per band, `eq_band_<b>_gain|frequency|q`. The select's state is `preset_state`,
the same `on` rule the other front ends use. See
[tone-control-and-presets.md](tone-control-and-presets.md). A device with presets of its own and no
tone control (a Loki Max) gets one select on the same key, `device_presets.rs`: its slots, its
built-in settings and `custom`. It reuses `eq_preset` because the key is already reserved, so no
profile can clash with it, and a dashboard finds the preset select under one key on every device. Its
state is the built-in setting the bands read, `custom` otherwise, and none until every band is read;
a slot is never the state, since the device does not say.

### Diagnostics

Four entities are not controls and are not in `exposed_controls`, so its function-class guard is
untouched: the `signal` sensor, the `firmware` and `dsp_firmware` sensors, and the `identify`
button. `diagnostic_entries` (`discovery.rs`) builds the signal sensor for every device, each
firmware sensor only where the profile's `firmware_subsystems` holds 1 (`firmware`) or 2
(`dsp_firmware`), and the button only with `commands` and a profile whose `identify` is true, and
`payload.rs` holds their pure parts: the strength as text, the attributes JSON and
`rfc3339_utc`, which words a unix-millisecond stamp without a date library.

The signal comes from the device's feed (`Feed::signal`, set where the actor takes in an
advertisement a listen heard) so the bridge still reads the device only through its feed. It is
published as two retained topics, the attributes before the strength so a sensor that updates has
fresh attributes. With no signal known nothing is published and Home Assistant shows the sensor as
unknown; a device that is down is `unavailable` through the availability every entity has, and its
last strength, source and stamp are kept as the daemon holds them (a `live` one never refreshed
once the link is gone, so its age is honest). The firmware builds are ordinary readings, functions
`210.2.1` and `210.2.2`, read from the feed's readings when the device has reported them. Neither
is the `sw_version` of the `device` block, which stays the advertised build.

The eight keys the bridge's own entities take (`link`, `signal`, `firmware`, `dsp_firmware`,
`identify`, and the tone control's `eq_mode`, `eq_preset` and `eq_loudness`) are `RESERVED_KEYS`
in `svolder-profiles` (`registry.rs`): the loader refuses a profile control with one, naming the
key, so a control can neither replace an entity, duplicate its unique id nor have its commands
swallowed by the identify topic. A test in `discovery/tests.rs` holds the discovery constants and
that list equal.

The identify button exists only with `commands` and a profile that offers identify, which makes
identify a write wherever a write gate exists (the web page, MCP's `writes`, MQTT's `commands`): one
rule, no special case. A press is the payload `PRESS` on `B/U/identify/set`; `BridgeTask::command`
recognises the key before the profile lookup, since the button is not a control, drops anything
else, and hands `Command::Identify` to the device's writer as `Origin::Mqtt`. The two-second gap
between cues lives in the BLE session, so the bridge adds no limit of its own. A button has no
state, so the answer (sent, or inside the gap) is only logged.

### Retained topics and the client

Retained messages are the bridge's memory on the broker, and `Client` replays them. It keeps the
latest payload of every retained topic it was handed and every subscription it was asked for, and on
each connect, the first and every one after a drop, replays them in order: `online` for the bridge,
every discovery config, every other retained topic, then the subscriptions. The bridge just hands
over what changed, whether or not the broker is there. A message that is not retained (the meters)
is sent only while connected and dropped otherwise. A failed connection retries after `retry_delay`:
2 s doubling to 60 s (`RETRY_MIN`, `RETRY_MAX`), logged at `warn` the first time and when the reason
changes, and at `debug` after.

The client id defaults to `svolder-<host>-<tag>`, the tag being a hash of the socket path
(`socket_tag`). A broker drops a connection when another client presents its id, so two daemons on
one host that shared an id would take it from each other on every reconnect; each daemon has a
socket of its own, and that makes the id its own.

### Clearing what is no longer there

A device removed from the configuration has every retained topic of its own cleared, including a
discovery config an earlier run left under its node that no control announces any more
(`BridgeTask::leftover`): the bridge subscribes to `<prefix>/+/svolder_<U>/+/config` for each
device, and clears any config it did not publish once it knows the device's own set. A stop (the
daemon's, or a reload that disables the bridge or changes its section) publishes `offline` for
every device and for the bridge before disconnecting, within `STOP_WITHIN` (3 s); a daemon that
dies without one leaves the broker to publish the last will.

## Commands

With `commands = true` the bridge subscribes to each device's `B/U/+/set`. A command
(`BridgeTask::command`):

1. is dropped, with a log line naming the device and the reason, if the payload is not text, the
   device has not been seen, or the key is not a control that discovery announces as writable.
   Nothing restricted is ever on that list, so nothing in the firmware-update family can be
   addressed.
2. is parsed against the device's profile the way `svolder set` parses a value
   (`svolder_mqtt::parse_command`, `command.rs`): a number for a level, snapped to the nearest step;
   `on` or `off` for a toggle; a label or raw number for a select, and `flat`, `eq` or `loudness`
   for the tone-control mode. `max_volume_db` caps a volume in dB, and the clamp is logged.
3. goes to the device's writer as `Origin::Mqtt` in `Mode::Latest`, so a slider dragged in Home
   Assistant is one write in flight and one waiting. A volume goes through `Writer::glide`, so a
   raise above three quarters of the range climbs instead of jumping. A band write asks to switch a
   Flat tone control to EQ (`Options::flat_to_eq`). A preset recall goes through `Writer::recall` as
   one batch. On a device with slots, a pick of a slot is a `recall` action write in
   `Mode::Ordered` and a pick of a built-in setting goes through `Writer::apply` as one batch; the
   bridge asks about neither, as it asks about no band write (`custom`, a save and any name the
   device does not list are dropped, and an action has no command topic of its own).

A retained command is never acted on (`BridgeTask::message`): a retained message on a command
topic would be replayed on every subscribe, so a volume set once would be set again on every
reconnect and every daemon restart. A command is acted on only as it is published, and a retained
one is logged once and ignored.

## Principles

**Off, then read-only, then control.** Without `[mqtt] enabled` the daemon opens no socket to any
broker; without `commands = true` it subscribes to no command topic at all. Remote control of a DAC
over a network is opt-in, for the volume-safety reasons in LEGAL.md. A default that listened would
turn every install with a broker into a remote.

**The bridge is a front end like the others.** It reads a feed, writes through the writer, and
inherits ordering, the cap and the climb. A second path to the session would bypass the volume rule
for the one front end that is reachable from the network.

**Parse as the CLI does.** A payload goes through the same control lookup, unit conversion and
refusal as `svolder set`. A re-derived parser would accept what the CLI refuses.

**Never a retained command.** See above: replay on reconnect would turn a past command into a
standing one.

**Pure core, thin shell.** Topics, payloads and discovery are functions; the only I/O is
`client.rs`. That is why goldens can pin the whole discovery output without a broker, and why a
layout change touches one file (`topics.rs`).

**The profile is the schema.** Entities come from `exposed_controls`, so a profile `readout` appears
with no Rust, and the publish list, the discovery list and the command list are the same list. Keep
them one.

**No secrets in the file you can print.** The password serialises as `***`, `Settings` prints it as
`***`, and `SVOLDER_MQTT_PASSWORD` overrides the file so it need not be written at all. The
connection is plain TCP, so a broker that needs protection is reached through a tunnel or a local
broker; TLS is not part of the bridge.

**Reconnect without caller logic.** The client replays its memory on every connect, so the bridge
hands over a change and forgets it; add reconnect handling to the bridge and it will fight the
client.

## Where the code and tests live

- Diagnostics and the identify press: `diagnostic_entries` in `crates/svolder-mqtt/src/discovery.rs`
  (tests in `discovery/tests.rs`), `signal_attributes` and `firmware_state` in `payload.rs`,
  `publish_diagnostics` in `bins/svolderd/src/mqtt_bridge/state.rs` and the press in
  `mqtt_bridge/commands.rs`, tested in `mqtt_bridge/tests.rs` and `bins/svolderd/tests/mqtt.rs`.
- Topics, payloads, discovery, commands, tone-control entities: `crates/svolder-mqtt/src/topics.rs`,
  `payload.rs`, `discovery.rs`, `command.rs`, `eq.rs`; tests beside them (`discovery/tests.rs`,
  `eq/tests.rs`) and in `crates/svolder-mqtt/tests/discovery_goldens.rs`.
- The connection: `crates/svolder-mqtt/src/client.rs`, tests in `client/tests.rs` and
  `crates/svolder-mqtt/tests/client.rs`.
- The bridge: `bins/svolderd/src/mqtt_bridge.rs` and its children in `mqtt_bridge/`, tests in
  `bins/svolderd/src/mqtt_bridge/tests.rs` and `bins/svolderd/tests/mqtt.rs`.
- Configuration: `MqttConfig` in `crates/svolder-ipc/src/config.rs`; the status `daemon status`
  prints is `MqttStatus` in `crates/svolder-ipc/src/socket/protocol.rs`.
- Home Assistant files: `contrib/home-assistant/` (a dashboard and themes).
