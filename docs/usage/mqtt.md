<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# MQTT and Home Assistant

Publishing the daemon's devices to an MQTT broker and controlling them from Home Assistant. Back to
the [usage index](README.md).

## MQTT and Home Assistant

MQTT is a small publish/subscribe protocol. A **broker** (Mosquitto, or the one Home Assistant ships
as an add-on) relays messages; clients **publish** to named **topics** (`svolder/…/state`) and
**subscribe** to the topics they care about. A **retained** message stays on the broker, so a client
that subscribes later still gets the last one. A **last will** is a message the broker publishes for
a client that disappears without saying goodbye. Home Assistant's **MQTT discovery** watches
`homeassistant/…/config` topics: a client that publishes a config there makes an entity appear, with
no YAML.

`svolderd` can publish every device it holds to a broker, announced so Home Assistant finds each one
on its own, and — only when you allow it — take commands back. Only the daemon connects, since it
holds the devices; a direct session (`--transport ble` or `fake`) publishes nothing, and the CLI and
the TUI see the bridge through `svolder daemon status`. It speaks MQTT 3.1.1 over plain TCP, with a
username and password when the broker wants them; there is no TLS. For a broker that only speaks
TLS, see "A TLS-only broker" below.

### Turning it on

The TUI's Settings box has a Connect category for the bridge: it turns the bridge and its commands
on and off, types the broker's host and port, shows whether a password is set (never the password)
and the daemon's status line (see ["Connect"](tui.md#connect) under "Settings box"). The bridge is
off unless `config.toml` turns it on: without an `[mqtt]` section, or with `enabled = false`, the
daemon opens no connection to anything. Every key but `enabled` and `host` is optional:

```toml
[mqtt]
enabled = true
host = "192.168.1.10"      # the broker
port = 1883                # default
username = "svolder"       # optional
password = "…"             # optional; SVOLDER_MQTT_PASSWORD overrides it
client_id = "svolder-myhost" # default: "svolder-" + the host name + a hash of the socket
base_topic = "svolder"     # default
discovery_prefix = "homeassistant"  # default; "" turns discovery off
commands = false           # default
max_volume_db = -10.0      # optional cap on a command's volume
```

`enabled = true` with no `host`, a `port` outside 1–65535, a `max_volume_db` that is not a finite
number, an empty `base_topic`, and a `base_topic` or `discovery_prefix` that holds a `+` or a `#`
(the MQTT wildcards) or begins or ends with `/` are config errors naming the key, so `svolderd`
refuses to start on them as on any other. A blank `host`, `username`, `password` or `client_id` is
no value at all. The default client id is `svolder-`, the host name and six hex digits that stand
for the socket the daemon serves on — `svolder-myhost-b08577` for `/run/user/1000/svolder.sock` — so
two daemons on one host never present the same id: a broker drops a connection when another client
presents its id, and two daemons sharing one would take the connection from each other every time
either reconnected.

The daemon's log names the id it connects as. `SVOLDER_MQTT_PASSWORD`, when set and not empty, is
the password in place of the file's, so the password need not sit in the file; a password with no
username is not sent (MQTT 3.1.1 carries one only with a username), and the daemon's log says so. A
reload — SIGHUP, or the `reload` request `add` and `remove` send — re-reads the section: the same
section keeps its connection and publishes the device set the file now names, a changed one stops
the bridge and starts the one it now describes, and a section taken out stops it.

### Topics

For a device with unit id `U` (its sixteen hex characters) and the default `base_topic`:

- `svolder/bridge/availability` — `online` / `offline`, retained. `offline` is the bridge's last
  will, and what a stop publishes.
- `svolder/U/availability` — `online` while the daemon holds the device's link, else `offline`,
  retained.
- `svolder/U/state` — one retained JSON object with a key per control (the profile's key: `volume`,
  `mute`, `input`, `balance`, …) in human units — volume in dB, balance in steps from centre (`0`
  centred, below 0 to the left, above to the right: -63 to 63 on a DAC), toggles `"ON"`/`"OFF"`,
  selects by their label — published when something changes, at most every 100 ms. A control the
  device has not reported yet is left out. A device with a tone control also carries `eq_preset`,
  `eq_loudness` and `eq_band_<b>_gain`, `eq_band_<b>_frequency` and `eq_band_<b>_q` for each band
  `b` (see "The tone control" below).
- `svolder/U/meters` — a JSON object of the scaled meters (`meter_1` …), each a whole percentage of
  its meter's scale, and 0 while the meters are switched off or the amp is in standby or warming up,
  not retained, at most once a second, for a device whose meters stream. A meter without a scale
  keeps its raw reading.
- `svolder/U/signal/state` and `svolder/U/signal/attributes` — the signal strength in dBm as a
  plain number, and its attributes as JSON, `{"source":"advert","heard_at":"2026-10-25T12:34:56Z"}`:
  where it was measured (`advert`, or `live`) and when the daemon heard it, in UTC. Retained, and
  published again when the strength, the source or the time changes. Nothing is published while no
  signal is known.
- `svolder/U/firmware/state` and `svolder/U/dsp_firmware/state` — the main and the DSP firmware
  build the device reported, each a plain number (`3`), retained, published once the device has
  reported it.
- `svolder/U/<key>/set` — the command topics, subscribed to only with `commands = true`: one per
  writable control, one per tone-control entity and `identify`, the button's.
- `homeassistant/<component>/svolder_U/<key>/config` — one retained discovery config per control,
  one per tone-control entity, one for the link, the diagnostic sensors and, with
  `commands = true`, the identify button.

### Watching it

`mosquitto_sub` (in Debian's and Ubuntu's `mosquitto-clients`) prints what the broker holds and then
everything published after, one `topic payload` line each. Against a broker on this machine, with a
daemon holding one device — the `gungnir` of the configuration file above, a Gungnir 2 — its lines
include the bridge's and the device's availability and the device's state:

```
$ mosquitto_sub -h 127.0.0.1 -t 'svolder/#' -t 'homeassistant/#' -v
svolder/bridge/availability online
svolder/0102030405060708/availability online
svolder/0102030405060708/state {"balance":0,"eq_mode":"flat","input":"USB 1","mute":"OFF","nos":"OFF","phase":"OFF","sample_rate":"48 kHz","spdif_locked":"OFF","usb1_connected":"ON","usb2_connected":"OFF","volume":-26.089}
```

The state lines in this section leave out the tone-control keys (`eq_preset`, `eq_loudness` and the
`eq_band_…` ones, which sort among the others) for length; see "The tone control" below. Among the
retained messages, in whatever order the broker keeps them, are the thirty-two discovery configs a
Gungnir 2 announces read-only, twelve for its controls and the link, seventeen for the tone control
and three diagnostic sensors (a thirty-third, the identify button, with `commands = true`). The
twelve are: `homeassistant/sensor/svolder_0102030405060708/volume/config` and its like
for `balance`, `input`, `eq_mode` and `sample_rate`, and `homeassistant/binary_sensor/…/config` for
`mute`, `nos`, `phase`, `usb1_connected`, `usb2_connected`, `spdif_locked` and `link`; the seventeen
are sensors, listed under "The tone control"; the three are `signal`, `firmware` and
`dsp_firmware`, under "Signal, firmware and Identify".

Each is one JSON object: the entity's `name` and `unique_id` (`svolder_0102030405060708_volume`),
the state topic and a `value_template` that reads its key out of the state
(`{{ value_json.volume }}`), both availability topics with `availability_mode` `all` (the link's
names the bridge's alone, and reads the device's availability as its state), a `device` block — the
configured name, `Schiit Audio`, the profile's model, the firmware build — and an `origin` block
naming svolder and its version. A change the device confirms — from the TUI, `svolder set`, or the
device's own knob — is a new `svolder/0102030405060708/state` line within moments. Stopping the
daemon prints `offline` for the device and then for the bridge; a daemon killed outright prints the
bridge's `offline` alone, the broker's last will for it.

### In Home Assistant

With Home Assistant's MQTT integration set up against the same broker (Settings → Devices & services
→ MQTT), each configured device appears as one Home Assistant device — its configured name,
`Schiit Audio` as the manufacturer, the profile's model, its firmware build as the software version
— holding one entity per control: a sensor for each level (volume and balance, volume in dB), a
binary sensor for each toggle (mute, NOS, phase), a sensor for each select (input, filter, the
tone-control mode) reading its label, a sensor for each numeric readout (the sample rate), a binary
sensor for each boolean readout (USB 1 / 2 connected, S/PDIF locked), a problem binary sensor for
each fault, a measurement sensor per meter (in `%` for a scaled one), a `Link` connectivity binary
sensor for the device's link, and the diagnostics of "Signal, firmware and Identify" below.

Home Assistant's MQTT integration has no media player, which is why a device is a set of entities.
Every entity but the link is unavailable while either the bridge or its device is offline. A device
with a tone control also gets the preset, the loudness strength and every band's gain, frequency and
Q (see "The tone control"). Nothing in the firmware-update family or the passphrase function can be
written or commanded, and the only things published from that family are the two version readings
(the `Firmware` and `DSP firmware` sensors of "Signal, firmware and Identify"), read-only. The
eight keys `link`, `signal`, `firmware`, `dsp_firmware`, `identify`, `eq_mode`, `eq_preset` and
`eq_loudness` belong to the bridge's own entities, so a profile file whose control takes one fails
to load, naming the key.

A device taken out of the configuration has its discovery configs, its state and its availability
cleared at the next reload, so its entities leave Home Assistant. A discovery config the broker
holds under one of the daemon's devices that no control announces now — a read-only sensor left by a
run before `commands` was switched on, say — is cleared as the bridge connects, so one control is
never two entities.

`contrib/home-assistant/` in the source tree has the broker and login steps, a dashboard to paste
into Home Assistant's raw configuration editor, the entity names a Gungnir 2 gets, and a recorder
exclusion for streaming meters.

### Signal, firmware and Identify

Up to three sensors sit under the device's Diagnostic heading in Home Assistant (`entity_category:
diagnostic`): `Signal` for every device, and each firmware sensor only for a device whose profile
has the session ask for that build (its `firmware_subsystems` lists 1 for `Firmware`, 2 for
`DSP firmware`: a Gungnir 2 has both, a Mimir only `Firmware`, a Freya neither):

- `Signal` (`sensor.gungnir_signal`) — device class `signal_strength`, in `dBm`, a measurement. Its
  attributes say where the strength was measured, `source` (`advert` for the advertisement the
  daemon heard before the link; `live` when the link itself gave it, which a BlueZ daemon never
  does for an unprivileged user) and `heard_at`, an RFC 3339 time in UTC. The strength is kept,
  with its own source and time, while the link is down. The sensor reads unknown until a strength
  is known, and unavailable while the device is, like every other entity of the device.
- `Firmware` (`sensor.gungnir_firmware`) and `DSP firmware` (`sensor.gungnir_dsp_firmware`) — the
  build numbers the device reports for its main controller and for its DSP, read from the device
  itself and not from its advertisement (the device's software version in Home Assistant stays the
  advertised build). A sensor reads unknown until the device has reported its build, and stays so
  for a device that never reports one.

With `commands = true` the device also has an `Identify` button (`button.gungnir_identify`, in Home
Assistant's Configuration group), except for a device whose profile offers no identify, the Mesh
Card. A press publishes `PRESS` on `svolder/U/identify/set`, and the daemon asks the unit to show
itself, as `svolder identify` does. The unit answers at most once every two seconds, so a press
inside that gap sends nothing. A button has no state, so the answer is in the daemon's log: `mqtt:
gungnir: identify sent`, or `identify not sent: one went out within the last two seconds`, or the
reason when the device is down. Anything but `PRESS` on that topic is dropped. Read-only, the button
is not offered, since a press would be a command.

### Commands

The bridge is read-only unless `commands = true`: it publishes and never subscribes to a command
topic. Remote control of a DAC's volume over the network is opt-in for the volume-safety reasons
LEGAL.md gives. With `commands = true`, the level entities are sliders (`number`, with the control's
minimum, maximum and step), the toggles are switches and the selects are selects, and each writes to
its own `svolder/U/<key>/set` topic:

- a level takes a number in its own units, snapped to the nearest step the device has: `-20` for the
  volume in dB, `0` for a centred balance;
- a toggle takes `ON` or `OFF`;
- a select takes one of its labels, `Coaxial` say, and the tone-control mode `flat`, `eq` or
  `loudness`;
- the tone control's preset, loudness strength and bands take what "The tone control" below says;
- the `Identify` button's topic takes `PRESS` (see "Signal, firmware and Identify").

A command is acted on only as it is published: one someone published retained, which the broker
hands to the bridge's subscription on every connect, is never written to the device, and the
daemon's log says so once per topic — the message itself is left on the broker, since it is not the
bridge's to clear. Each command goes through the daemon's own set path — the one `svolder set`
takes, with its validation and its refusal of every protected function id — so nothing reaches the
device that `svolder set` would refuse; anything else is logged and dropped. A burst on one topic —
a slider dragged in Home Assistant — coalesces: one write per control is in flight at a time, and
the latest value is the one sent next.

Commands for different controls reach the device in the order they arrive, and a command that has to
wait for the one in flight on its control is not overtaken by a later command for another control: a
volume and a `mute ON` published after it are written in that order, even while an earlier volume is
still in flight, and a newer slider value takes the place of the one it replaces in that order. A
preset recall is the exception: it can start ahead of a command waiting on another control. Above
three quarters of its range the volume climbs rather than jumps, as the TUI's bar does: from below
three quarters, a command above it writes three quarters of the range at once — at most that, for a
volume the daemon has not read yet — and then climbs; from above it nothing is written at once and
the climb starts from the reading.

The climb is a raw step at a time, a tenth of the range a second, so a slider dragged to the top
takes about 2.5 s from there. A newer command steers the climb, one below where it has reached lands
at once, and a write from any other front end, a mute, a power-off, the knob or a lost link ends it.
`max_volume_db` applies first, so a climb never passes the cap. While a device reports its output
muted or off, a glide does not climb (it lands its first step); on a Lyr 5 a muted headphone output
ends the climb even when the pre-out is in use.

While a preset recall from Home Assistant or the MCP server runs, the tone-control writes every
front end sends wait for it to finish: a TUI band drag or an `svolder eq set` during a Home
Assistant recall goes out after it. With `max_volume_db` set, the slider still reaches the device's
maximum — the device's volume moves by its own knob and every other front end too, and Home
Assistant ignores a state beyond a `number`'s range — and a volume command above `max_volume_db`, or
one whose nearest step lies above it, is set to the cap: the step at or below it, which the daemon's
log says (a cap below every step of the device, which no step is at or below, sends the quietest
step instead, and the log says so):

```
svolderd: mqtt: gungnir: volume -5 dB capped at max_volume_db, -10 dB
```

With `commands = true` the volume's config moves to
`homeassistant/number/svolder_0102030405060708/volume/config` and gains a `command_topic`,
`svolder/0102030405060708/volume/set`, a `min` of -96.33, a `max` of 0.0 — the device's own maximum,
`max_volume_db` or not — a `step` of 0.334 and `mode` `slider`. A command published on that topic —
`mosquitto_pub -h 127.0.0.1 -t svolder/0102030405060708/volume/set -m -20` — is followed by the
state it left the device in, as the `mosquitto_sub` above prints them:

```
svolder/0102030405060708/volume/set -20
svolder/0102030405060708/state {"balance":0,"eq_mode":"flat","input":"USB 1","mute":"OFF","nos":"OFF","phase":"OFF","sample_rate":"48 kHz","spdif_locked":"OFF","usb1_connected":"ON","usb2_connected":"OFF","volume":-20.069}
```

### The tone control

A device whose profile has a tone control (the one the TUI's tone-control screen edits) announces,
besides `eq_mode`, one `eq_preset` entity, one `eq_loudness` and three entities for each of its
bands. A Gungnir 2 has five bands, so these are seventeen entities; a three-band device has eleven.
A profile without a tone control has none. With `commands = true` they are a `select` and `number`s,
and read-only they are all `sensor`s that show the same state:

| Key | Name | Unit | Bounds | Step | Mode |
|---|---|---|---|---|---|
| `eq_preset` | EQ preset | | the presets that fit the device, then `custom` | | select |
| `eq_loudness` | Loudness strength | | 0 to 127 | 1 | slider |
| `eq_band_<b>_gain` | Band b gain | `dB` | the band's lowest to highest gain (-20 to 6 on a Gungnir 2) | the finest step between two gains (0.204) | slider |
| `eq_band_<b>_frequency` | Band b frequency | `Hz` | the band's own window (band 2 of a Gungnir 2: 81.961 to 311.957) | 1 | box |
| `eq_band_<b>_q` | Band b Q | | the band's Q range (0.5 to 8) | 0.01 | box |

The unique ids are `svolder_U_<key>`, and every entity carries the same availability, device and
origin blocks as the others. The state holds each band parameter in those units, rounded to three
decimals, and the loudness strength as its number from 0 to 127; a parameter the device has not
reported yet is left out. The loudness strength is what the tone-control screen's loudness row sets;
it applies in `loudness` mode.

A band command is a number in the entity's unit, converted and snapped to the nearest step the band
has exactly as `svolder eq set` does: a value outside the band's window or the device's range is set
to its nearest end rather than refused, and a payload that is not a finite number, like `loud`, or a
frequency of 0 or below (the log names the band's window), is logged and dropped. A loudness command
is a number from 0 to 127, rounded to a whole step; one outside that range is dropped.
`max_volume_db` caps the volume alone: it does not apply to the tone control, and an EQ boost can
make the output louder than the volume says.

A tone control in `flat` mode is the device's bypass, under which no band applies, so a band command
the device takes while the tone control reads `flat` is followed by a write of the mode `eq`, as the
TUI's band edits are, and the log says `switching to EQ`. The mode goes after the band, so the curve
is whole when the device applies it; a burst of band commands does not queue a mode write per band.

In `loudness` mode, and before the device has reported its mode, the mode is left alone, and so it
is when the band command is refused or a preset recall follows it (the recall writes its own mode).
A band command sent while a recall runs waits for it, and the mode the recall left decides. A mode
command sent after a band wins over that band's switch, so a command for `flat` that comes after the
band leaves the tone control in `flat`; a scene that sets `eq_mode: flat` together with changed band
values sends them in no fixed order, so it can end in `eq`. `svolder eq set` leaves the mode alone.

The preset select offers the presets that fit the device, in the order the TUI's preset list shows
them — the built-ins, then the `[[preset]]` blocks of `config.toml` with the same band count — and
`custom` last. Its state is chosen in this order:

1. the preset last recalled on that device by any front end, while the device still matches it;
2. else the first preset of the list that the device matches, as `svolder eq show` decides it;
3. else `custom`.

Until the device has reported its mode and every band parameter, the state has no `eq_preset`. A
command for a preset recalls it as `svolder eq load` and the MCP server's `load_preset` do: the
writes it takes are planned from the device's current readings and sent one after the other through
the daemon's own path, as one write under `eq_preset`, so a burst of selections ends on the latest.

A value the plan has to move to a step the device has is logged, `svolderd: mqtt: gungnir: preset
HD650 warm: band 5 frequency 22000 Hz clamped to 19497 Hz` say, and a recall that fails partway is
logged with how many of its writes went through. A command of `custom`, or of a name that is not in
the list (matched without regard to case or the blanks round it), is logged and dropped; a preset
the file names `custom` is left out of the options for the same reason.

A preset saved from the TUI's preset list (`n`, `u`, `c`, `x`) or by `svolder eq save`, `rename` or
`delete` reaches Home Assistant without a restart: after writing the file, each asks a running
daemon to reload, but only when that daemon's `version` reply names the same configuration file, and
says nothing where there is no daemon, where it reads another file, or where the reload fails. A
direct session has no daemon to tell. On a reload the bridge announces the select's discovery config
again with the new options, and publishes the device's state. A preset written by hand, or by a
front end that does not send the reload, is offered at the next reload or restart.

While a preset is being recalled, or waiting for a tone-control write already in flight, a band,
loudness or mode command for the same device is held and sent after the recall finishes (the latest
one per entity). A change held before a later preset pick is dropped by that pick when the pick
rewrites it: a preset rewrites every band parameter and the mode, and the loudness strength only
when it is in Loudness mode. A change held after the pick goes out after it. The device so ends on
what was last asked for, so a script that picks a preset and then sets a band ends on its band. A
preset picked while one of those writes is in flight waits for it, then plans from the readings it
left; a later pick replaces one that has not started yet. Volume, mute and the other controls are
never held.

### A Loki Max's presets

A Loki Max has no tone control, so it has no `eq_mode`, `eq_loudness` or `eq_band_*` entities. Its
six bands are the controls `band1` to `band6`, and its presets are one more `eq_preset` entity,
named Preset: a `select` with `commands = true` (a `sensor` read-only) whose options are `Preset 1`,
`Preset 2`, `Preset 3`, the five built-in settings (`Flat`, `Bass Boost`, `Treble Boost`, `Smiley`,
`Loudness`) and `custom` last. Its state is the built-in setting the six bands read, `custom` when
they match none, and absent until every band has been read. A slot is never the state, because the
device does not say which slot it was last set from.

A pick of a slot recalls it, and the device reports its bands, which the state then follows. A pick
of a built-in setting writes the bands it changes as one batch, cuts before boosts. The bridge asks
about neither, as it asks about no band write. A command of `custom`, of a slot the device does not
have, or of a save is logged and dropped, and so is a command on the topic of `recall` or `save`:
saving to a slot is not offered. A retained command is never acted on.

### The computer's EQ

One more Home Assistant device, `This computer`, carries the computer's EQ and nothing else of the
computer: no volume, no mute and no points.

| Entity | Topics | What |
|---|---|---|
| `switch.this_computer_eq` | `B/computer/computer_eq/state` (`ON`/`OFF`) and `.../set` | the EQ on or off; a `binary_sensor` while `commands` is off |
| `sensor.this_computer_eq_preset` | `B/computer/computer_eq_preset/state` | the preset the points came from, `custom` after a hand edit, empty while the EQ is off |

`ON` turns the EQ on with the points the daemon keeps; `OFF` removes it and keeps them. With no
points kept yet (nothing applied or edited since the daemon first ran), `ON` leaves the EQ off and
logs a line. A retained command is never acted on. The entities appear while the daemon serves the
computer's controls; when it cannot (for example no `pw-cli`), they say unavailable, and with host
audio off nothing is announced. When the daemon cannot tell whether the chain runs, the switch
reports the state the daemon saved, which is what it last applied.

### The connection

The daemon connects when it starts and whenever the connection drops, waiting 2 s after a failure
and doubling the wait each time up to 60 s; the first connect and every reconnect publish `online`,
every discovery config and every device's state and availability again, then subscribe to the
command topics if commands are on. `svolder daemon status` names where it stands, on the line after
the daemon's own: `mqtt: off`; `mqtt: connecting to HOST:PORT` while an attempt is under way, and
`mqtt: connecting to HOST:PORT (retry in N s)` while it waits between attempts; or
`mqtt: connected to HOST:PORT (read-only)` or `(commands on)` (see
["daemon status"](daemon.md#daemon-status)).

The daemon's log names the broker and the client id once, as the bridge starts —
`mqtt: connecting to HOST:PORT as ID, read-only` or `taking commands` — then says
`mqtt: connected to HOST:PORT` on every connect, and the reason each time the broker could not be
reached: once at the default level, and at `debug` for every attempt after that fails the same way.
A subscription the broker refuses — an ACL that denies the command topics, say — is logged at warn,
naming the filter. The daemon's stop — `svolder daemon stop`, the `stop` request, SIGTERM or SIGINT
— publishes `offline` for every device and for the bridge before it disconnects; a daemon killed
outright leaves the broker to publish the bridge's last will.

### A TLS-only broker

The username and password travel in the clear, which suits a broker on the same host or on a trusted
LAN (Home Assistant's Mosquitto add-on listens on plain port 1883 with a login by default), and the
broker needs a plaintext listener. For a broker that accepts only TLS (usually port 8883), a local
TLS client carries the connection: svolder connects to a port on 127.0.0.1 and the client to the
broker. With stunnel, `/etc/stunnel/mqtt.conf`:

```
[mqtt]
client = yes
accept = 127.0.0.1:1884
connect = broker.example:8883
verifyChain = yes
CAfile = /etc/ssl/certs/ca-certificates.crt
checkHost = broker.example
```

and `host = "127.0.0.1"`, `port = 1884` in `config.toml`'s `[mqtt]`.
