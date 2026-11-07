<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The MCP server

The MCP server that lets an AI assistant read and set devices, and the `mcp` verbs that manage its
token. Back to the [usage index](README.md).

## MCP server

The Model Context Protocol (MCP) is how an AI assistant such as Claude Code or Claude Desktop calls
tools. `svolderd` can serve it over HTTP, so an assistant can read your devices ("what is the DAC's
input?") and, only when you allow it, set them ("lower the amp 3 dB", "load my HD650 preset"). It
speaks revision 2026-07-28, which is stateless — no `initialize`, no session, one JSON-RPC POST per
request — and answers clients of the 2025-11-25 revision too. This section is the guide; "mcp" below
is the reference for every `[mcp]` key, the refusals and the `svolder mcp token` flags.

### Turning it on

The server is off unless `config.toml` turns it on; without an `[mcp]` section, or with
`enabled = false`, the daemon opens no port:

```toml
[mcp]
enabled = true
```

The defaults are `listen = "127.0.0.1:8737"` (this machine only), `auth = "token"`, `writes = false`
(read-only), no `max_volume_db`, and empty `origins` and `hosts`. Run `svolder daemon restart`, or
reload the running daemon (SIGHUP) to have it read the section; a reload that changes `[mcp]`
restarts only the listener, and one that leaves it alone keeps the listener, its port and the last
minute of levels the volume rule uses. `svolder daemon status` says where it stands, on the line
after the bridge's, and the TUI's Settings box has a Connect category that switches the server, its
listen address and its writes, shows the token masked and what the daemon says (see
["Connect"](tui.md#connect) under "Settings box"):

```
mcp: listening on 127.0.0.1:8737 (token, read-only)
```

`writes` replaces `read-only` when the write tools are offered, and an address that could not be
bound reads `mcp: cannot listen on ADDR: WHY`; the daemon keeps its socket and its devices either
way. `svolder doctor` has an "MCP server" row for the same state.

### The token

Every request carries `Authorization: Bearer <token>`, from this machine too: any program on it, and
any web page in your browser, can reach `127.0.0.1`, so the token is what tells your assistant from
them. The first start with `[mcp]` enabled generates one, 32 random bytes in base64url (43
characters), in `~/.local/state/svolder/mcp-token` with mode 0600. `svolder mcp token` prints it and
nothing else, so `$(svolder mcp token)` is the token; `svolder mcp token --rotate` replaces it and
tells a running daemon to reload, so the old token stops working and the new one works at once
(`svolder: the daemon now accepts only the new token`); with no daemon running it says none uses the
new token yet, and when the reload fails it says so and names `svolder daemon restart`.

Another source replaces the generated file: `token_file = "/run/secrets/mcp"` in `[mcp]` reads the
token from that file, and the `SVOLDER_MCP_TOKEN` environment variable, when not empty, outranks
both. With either set, svolder generates nothing and `--rotate` refuses, naming the source. A token
file that group or others can read is refused. The token is never logged and never shown by
`config show` or `daemon status`. `svolder mcp token` also asks a daemon that answers which token it
uses, and warns on standard error when it is not the one printed (the daemon has `SVOLDER_MCP_TOKEN`
in its environment, or another state home, as a container does); `--rotate` then refuses, naming
where the daemon's token lives.

`auth = "none"` turns the check off, and only for a loopback `listen` address; anywhere else it is a
config error.

### Writes and the volume rule

With `writes = false` the server offers five read tools (`get_host` reads the computer) and nothing
can change. `writes = true` adds seven more: three that change a device, `identify`, which only asks
the unit to show itself, and three for the computer's volume and EQ. The
rule below stands between a model and your amplifier. Set `max_volume_db` too:
`svolder doctor` warns when writes are on without it.

- A volume change is measured against the lowest level the daemon saw for that control in the last
  60 seconds, from the device's own readings and from earlier writes by any client. A request that
  ends more than 6 dB above that floor asks you first; a staircase of small steps asks once the
  steps add up. The same holds for every level with a dB conversion, such as the Loki Max's bands. A
  request that lowers the level or leaves it where it is never asks. A number outside the control's
  range is refused, not clamped.
- `max_volume_db` is a cap. A volume that rises to a level above it is refused, naming the cap,
  whatever you answer. Lowering is always allowed, even to a level that is still above the cap.
- Above three quarters of the volume's range a rise climbs rather than jumps, as the TUI's bar and
  Home Assistant's slider do: after your yes, or with no question for a rise of 6 dB or less, from
  below three quarters the volume is written three quarters of its range at once and then climbs a
  tenth of the range a second; from above, nothing is written at once and the climb starts from the
  reading. The daemon judges this on its own reading of the volume when the write reaches it. The
  call waits for the climb, up to 5 seconds, and answers with the level the device reached; another
  front end's write, a mute or the knob can end the climb sooner.
- A gain or mode select that a profile marks as setting the output level (the Hati's and the Skoll's
  gain, the Saga 2's mode) asks when set to a higher value and allows a lower one; one whose present
  setting cannot be read asks.
- Unmuting always asks, and is refused while the volume is above the cap. A device with no dB volume
  (a fixed-output DAC, a phono stage, a power amp) asks too, saying the unmute lets sound through at
  its full output. A volume the device has not reported yet is worded as such ("at a volume the
  device has not reported yet"), never as silence. An unreported volume never causes a refusal, with
  `max_volume_db` set or not: a volume set at or below the cap, an unmute and a power-on ask, in
  those words, and only a target above the cap is refused.
- Powering a device on asks the same way: a `power` toggle a profile marks `powers_output` (the
  standby amplifiers, Tyr, the Hati, the Stjarna and the Freya 2) set to true says it lets sound
  through at its full output, or at its current volume where the device has a dB volume, and is
  refused while that volume is above the cap. A power amp has no mute, so this is its unmute.
  Powering off never asks.
- A control a profile marks `asks_on_change` (the Lyr 5's pre-out, the Loki Max's mode) asks on
  every change.

The question reaches you through the assistant: a 2026-07-28 client that declared form elicitation
gets an `input_required` answer, shows you a form naming the device and the levels or settings, and
retries only with your yes. A decline or cancel ends the call with an error, and the model is not
asked again. A client that cannot ask — an older one, or one with no form support — gets a tool
error that tells the model to ask you and repeat the call with `"confirm": true`; that flag is
honoured only for such a client, and never replaces the question for one that can ask.

What is never exposed, whatever the settings: raw frames and raw writes, the passphrase, firmware
and anything in the update family, the factory reset, scanning, adding and removing devices, edits
to the configuration, and anything from the host's audio (a track's title or artist is text a third
party writes, which is the way a prompt injection arrives). Tool results carry device data only: the
names you configured, the profile's labels and the readings.

### Using it from another machine

A private address in `listen` — `192.0.2.10:8737` — serves the LAN, which also reaches a container's
network. The token is required there, and a wildcard such as `0.0.0.0` names no host, so with no
`hosts` every `Host` header is let through and the token is the only check; naming `hosts` turns
that into a list. TLS is a reverse proxy's job, as the listener speaks plain HTTP: put Caddy, nginx
or Traefik in front, terminate TLS there, and forward `/mcp` to the listener. A request through the
proxy carries the proxy's host name in `Host`, so name it:

```toml
[mcp]
enabled = true
listen  = "127.0.0.1:8737"
hosts   = ["mcp.example.com"]      # an entry without a port matches any port
```

A browser is refused by default: a request that carries an `Origin` header unless `origins` names it
is answered 403 once the token checks out (a request without a valid token gets 401 first), and a
client that is not a browser sends none. `origins = ["https://app.example.com:443"]` lets a page of
that origin through; each entry needs a scheme and a port (`:*` for any), and `"null"` is refused.

### Connecting Claude Code

With the server on, one command registers it with Claude Code; paste the token `svolder mcp token`
prints in place of `<token>`:

```
claude mcp add --transport http svolder http://127.0.0.1:8737/mcp --header "Authorization: Bearer <token>"
```

Claude Code keeps the token in its own configuration, so a rotated token needs the command again,
and that file is as secret as `mcp-token` itself. Then ask it something: "what is the volume on my
DAC?" runs `get_state`. Another client takes the same URL and header.

### The tools

Every tool but `list_devices` takes a `device`, the name from `list_devices`, which may be left out
when one device is configured. Results are structured JSON.

- `list_devices` — no arguments; the configured devices, each with `name`, `product` and
  `connected`, and `signal` once a strength is known: `dbm`, `source` (`advert`, or `live`) and
  `heard_at_ms`, the daemon's clock in unix milliseconds when it heard it.
- `get_state` — `device`; the device's `product` and `connected`, and every control with its `key`,
  `label`, `kind` (`level`, `toggle`, `select`, `readout`, `meter` or `fault`), `writable`, `value`,
  `unit` (the volume is in dB), the `values` a select accepts and the `range` of a level. A balance
  counts from its centre, as in MQTT: `0` centred, below 0 to the left, above to the right (-63 to
  63 on a DAC, -100 to 100 on a Freya 2). The keys are the ones `set_control` takes. A readout that
  an `only_while` gates reads true only while its gate does, as in the TUI and MQTT.
- `get_link_history` — `device`, `hours` (above 0, at most 24; 24 when left out); the Bluetooth
  link's transitions in that span, each with `connected`, `at_ms` and `seconds_ago`.
- `list_presets` — `device`; the tone-control presets that fit its equalizer, each with `name`,
  `bands`, `built_in` and `on` (whether it is the one on: the preset last recalled by any front end
  while the device still matches it, else the first that matches). A Loki Max has no tone control
  but presets of its own, and lists those: its three device slots (`Preset 1` to `Preset 3`, each
  with its `slot`, `built_in` false and `on` always false, since the device does not say which one
  it was last set from) and its five built-in settings (`built_in` true, `on` for the one its six
  bands read). Any other device with no tone control is an error.
- `set_control` (writes) — `device`, `key`, `value`, optional `confirm`; sets one control (a level
  as a number in its unit, a balance in steps from its centre, a toggle as `true` or `false`, a
  select by name) and returns the reading the device confirmed. The volume rule applies: a rise of
  more than 6 dB, a higher gain or mode, an unmute and powering a device on (`power` set to true)
  ask first; powering off never does.
- `load_preset` (writes) — `device`, `name`; recalls a preset onto the equalizer and returns how
  many writes it took and any value it clamped. It does not touch the volume, but each band boosts
  at most 6 dB and bands that overlap add up, and a preset may set loudness, so it can make some
  frequencies louder. Its writes go out as one batch: a band, loudness or mode write another front
  end sends meanwhile waits until it is done. On a Loki Max, `name` is a slot (`Preset 2`, or `2`)
  or a built-in setting, and the call takes `confirm` as `set_control` does. A **built-in setting**
  is six band writes and is judged as they are: it asks once, naming each band that would rise more
  than 6 dB above its lowest level of the last minute (`raising 20 Hz on loki from 0.0 dB to 10.4
  dB is more than 6 dB above ...`), and goes through unasked when none does. A band that already
  reads its value counts too when it was more than 6 dB lower within the last minute, since another
  front end may lower it before the setting is written. A setting the bands already read is answered
  `NAME is already on` and writes nothing. A **slot recall** always asks, because the bands it sets
  are whatever the device saved there, as high as +12 dB on a band (`recalling Preset 2 on loki sets
  its bands to what the device saved there, which can raise a band to as much as 12.0 dB`). A client
  that can ask is sent the question; any other gets the question as a tool error and repeats the
  call with `"confirm": true` once the user agreed. Saving to a slot is not offered: the name
  `save`, alone or with a slot (`save 2`), is refused, and `set_control` has no `recall` or `save`
  to set.
- `mute` (writes) — `device`, `on`, optional `confirm`; mutes or unmutes. Unmuting asks.
- `identify` (writes) — `device`; asks the unit to show itself, so you can tell it from the others
  (a device whose profile offers no identify, the Mesh Card, is a tool error saying so),
  and answers `{"device", "sent"}`. The unit is asked at most once every two seconds, whoever asks:
  a call inside that gap sends nothing and answers `sent: false`, which is not an error. A device
  that is not connected is an error in the daemon's words, with the wait to its next attempt where
  there is one. It changes nothing on the device, and the tool is listed only with `writes = true`
  like the others, so a client with an older list that calls it is refused naming the setting.
- `get_host` — no arguments; answers `{"volume": {percent, muted, sink} | null, "eq": {on, preset,
  preamp_db, points: [{type, frequency_hz, gain_db, q}]} | null, "player": {title, artist, album,
  status} | null}`. Each is read on its own: `volume` and `player` are null while there is nothing to
  read, and `eq` is null with `unavailable` saying why when the computer EQ cannot be used. An EQ whose state the daemon could not verify carries `"unverified":
  true`: `on` is then only what the saved state says.
- `set_host_volume` (writes) — `percent`, a whole number from 0 to 100, optional `confirm`. Above
  100 is refused whatever is answered. The rise rule is the one above, on the computer's own scale
  (the cube of the percent, `60 * log10(percent / 100)` dB): a rise of more than 6 dB over the lowest
  level of the last minute asks, and lowering never does. A volume not read yet counts as 1 %, so a
  rise from it asks. `max_volume_db` belongs to the devices and does not cover the computer, whose
  cap is 100 %.
- `host_mute` (writes) — `muted`, optional `confirm`; mutes or unmutes the computer's output.
  Unmuting always asks, as the device's `mute` does; muting never does.
- `set_host_eq` (writes) — exactly one of `preset` (a name from `list_presets`) and `off: true`,
  optional `device`, optional `confirm`. A built-in preset is shaped to a device's tone control, so
  it needs one: the named device, or the only one. A preset a connected device plays too asks, since
  the correction would play twice; repeat with `"confirm": true` once the user agreed, which
  flattens that device first.

## mcp

svolderd can serve the Model Context Protocol over HTTP for an AI assistant, on `/mcp` (revision
2026-07-28, stateless: no session, one JSON-RPC POST per request; JSON answers, no event streams).
The `[mcp]` table is off until enabled ("MCP server" above is the guide):

```toml
[mcp]
enabled       = false                   # no listener at all unless true
listen        = "127.0.0.1:8737"        # a socket address; loopback by default
auth          = "token"                 # "token", or "none" for a loopback listen address only
writes        = false                   # whether the tools that change a device are offered
# max_volume_db = -10.0                 # optional cap, in dB, on a volume the server will set
origins       = []                      # browser origins allowed to call the server
hosts         = []                      # extra Host names clients may use, e.g. a proxy's
# token_file  = "/run/secrets/mcp"      # optional; read the token from this file
```

`auth = "none"` with a `listen` address that is not loopback is a config error:
`mcp.auth = "none" needs a loopback listen address`. So are an unparsable `listen` and a
`max_volume_db` that is not a finite number, each naming its key. The token is not part of the
configuration, so `config show` has none to hide.

The token comes from the first of these that is set: the `SVOLDER_MCP_TOKEN` environment variable
(when not empty), the file `token_file` names, and a file svolder generates,
`$XDG_STATE_HOME/svolder/mcp-token` (`~/.local/state/svolder/mcp-token`; 32 random bytes, base64url
without padding, 43 characters, mode 0600). A token file, generated or named, that group or others
can read is refused with its mode named. With the environment variable or `token_file` set, svolder
generates nothing.

Every request needs `Authorization: Bearer <token>`, even from the same machine, unless
`auth = "none"` on a loopback `listen` address. A request without it, with a wrong one, or with a
header over 512 bytes (or with more than one `Authorization` header) is answered `401` with
`WWW-Authenticate: Bearer` and an empty body, whatever path, method or body it carries.

After that the server checks the `Host` header against the address it listens on (with its port; on
a loopback address also `localhost`, `127.0.0.1` and `[::1]`; any `Host` where `listen` is `0.0.0.0`
or `::`, which names no host, so the token is the check there; `hosts = ["mcp.home.lan", ...]` adds
names to the list, also on a wildcard bind, where a non-empty `hosts` turns the any-Host rule into
this list plus the loopback names; an entry without a port matches any port, one with a port that
port only) and refuses a request carrying an `Origin` header unless `origins` names it (each entry
needs a scheme and a port, `:*` for any; `"null"` is refused) (`403`; a request with no `Origin`,
which is every client but a browser page, goes on). A request body over 64 KiB is answered `413`;
any path but `/mcp` is `404`. TLS is a reverse proxy's job: the listener speaks plain HTTP.

The server follows a reload (SIGHUP, or the `reload` request that the TUI's Connect category, `add`,
`remove` and `svolder mcp token --rotate` send; `svolder daemon restart` starts the daemon afresh
instead) like the MQTT bridge: an unchanged `[mcp]` keeps its port, whatever its auth, and a server
with a token reads it again, so a rotated token takes effect at once; a changed one restarts the
listener; `enabled = false` closes the port. An address that cannot be bound, or a token that cannot
be had, is logged without the token and shown by `svolder daemon status` as
`mcp: cannot listen on ADDR: WHY`; the daemon keeps serving its socket and its devices, and the next
reload tries again.

### mcp token

Prints the token and nothing else on standard output, so `$(svolder mcp token)` is the token;
messages go to standard error. The first call creates the generated file. `--rotate` replaces the
generated token with a fresh one, prints it, and sends a running daemon a reload: standard error
then says `svolder: the daemon now accepts only the new token`,
`svolder: no daemon is running, so none uses the new token yet`, or `svolder: the daemon did not
reload (WHY): it still accepts the old token until you run svolder daemon restart`. With the token
coming from `SVOLDER_MCP_TOKEN` or `token_file` it refuses, naming that source, and it refuses too
when the daemon uses another token than this one (`the daemon uses another token, which lives WHERE,
so replacing this one would change nothing it accepts`).

A daemon that answers is asked which token it uses; one that names this file but has not read it
again is told so (`svolder: the daemon has not read PATH again yet: svolder daemon restart (or
SIGHUP)`, and `--rotate` goes ahead, its reload settles it), and any other mismatch is a warning on
standard error, `svolder: warning: the daemon uses another token, which lives WHERE; the one printed
is not the one it accepts`, where WHERE is `in the daemon's environment (SVOLDER_MCP_TOKEN)` or
`at PATH`. A config error or a refused file exits non-zero with nothing on standard output.

```
Print the bearer token a client of the MCP server sends, and nothing else

Usage: svolder mcp token [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --rotate                 Replace the generated token with a fresh one before printing it
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```
