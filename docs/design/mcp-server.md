<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The MCP server

The daemon can serve the Model Context Protocol over HTTP so an AI assistant can read a device and,
when the owner allows it, change one. The danger is plain: a model that can set a volume can set it
loud. This page is how the server is built and the rules that make that safe to offer. It sits in
`bins/svolderd`; the map's [web layers](../architecture.md#the-web-uis-layers) describe the HTTP
loop it shares with the web UI; the settings and the tool list a user needs are in the [MCP
reference](../usage/mcp.md).

## How it is built

`McpServer` (`bins/svolderd/src/mcp.rs`) is one task beside the device actors, held by a `Slot` in
the daemon (`Daemon::mcp_to`) so a reload brings it in line with `[mcp]` without restarting the
daemon. It serves `/mcp` through rmcp's streamable-HTTP service, stateless: there is no session
(`NeverSessionManager`), every request carries what it needs, and a call that has to ask the user a
question returns an `input_required` result that the client answers by repeating the call with the
answer attached.

The accept loop, with its limits, is `bins/svolderd/src/http.rs`, shared with the web UI:
`MAX_CONNECTIONS` (64), `HEADER_TIMEOUT` (5 s) and `MAX_HEAD_BYTES` (16 KiB) per request head, and
`SETUP_TIMEOUT` (5 s) for the token and the bind, run off the runtime's workers by `blocking` so a
token file on a hung mount fails only this server. Every request meets, in order:

1. `McpAuth::allows` (`TokenAuth`): exactly one `Authorization: Bearer` header of at most
   `MAX_AUTHORIZATION` (512) bytes, compared by `token::token_matches`, in time that does not depend
   on where they differ. A failure is a 401 with an empty body, so a caller learns nothing, not even
   which paths exist. `[mcp] auth = "none"` swaps in `NoAuth`, which `prepare` allows only on a
   loopback address.
2. The path, `/mcp`.
3. rmcp's checks: the `Host` header against `allowed_hosts` (the address the server is bound to, the
   loopback names on a loopback bind, and the `hosts` the owner lists), the `Origin` header against
   `[mcp] origins` (empty allows none, so a web page cannot call it), and a body of at most
   `MAX_BODY` (64 KiB).

A server that cannot start, for an address that will not bind or a token that cannot be had, is left
in the failed state: the daemon goes on, `version` reports the failure, and the next reload tries
again. The token is never logged, printed or put in a status; only where it came from and its last
characters are (`TokenInfo`, see [ipc-and-configuration.md](ipc-and-configuration.md)). A reload
reads the token again (`Refresher`), and a server that cannot is stopped rather than left serving on
a secret the owner has replaced.

## The tools

`Tools` (`bins/svolderd/src/mcp/tools.rs`) offers twelve, listed in `TOOLS`: `list_devices`,
`get_state`, `get_link_history`, `list_presets` and `get_host` read; `set_control`, `load_preset`,
`mute`, `identify`, `set_host_volume`, `host_mute` and `set_host_eq` write. `get_host`,
`set_host_volume`, `host_mute` and `set_host_eq` are the computer's
(`bins/svolderd/src/mcp/tools/host.rs`): a percent from 0 to 100 becomes dB as
`60 * log10(percent / 100)`, the floors hold it under `host:volume`, and a rise past `MAX_RISE_DB`
asks as the device's does; a volume not read yet counts as 1 %. An unmute always asks, as the
device's does. `max_volume_db` does not cover the computer, whose cap is 100 %. The floors see
only this server's own computer writes and reads. A preset another device plays too asks, and
repeats with `confirm`. The write tools are not listed while `[mcp] writes` is off, and a call to
one is refused naming the setting, because a client may hold an older list. A device refusal is a
tool error the model reads; only a tool that does not exist is a protocol error.

Every tool goes through `Fleet`, the same request path the socket's clients use
(`bins/svolderd/src/fleet.rs`), so a value is validated and refused exactly as `svolder set` refuses
it, and every write is a write from `Origin::Mcp` through the device's writer. Results carry device
data only: names the owner configured, profile labels and readings, never anything a third party
writes, so a tool result is not a channel for instructions into the model.

`list_presets` and `load_preset` branch on the device: one with a tone control lists and recalls its
tone-control presets, and one with presets of its own and none of that (a Loki Max, see
`svolder_profiles::device_presets`) lists its slots and built-in settings, recalls a slot with
`Fleet::submit` (`Mode::Ordered`) and applies a built-in setting with `Fleet::apply`, which the
writer sends as one batch. A save is never offered: `load_preset` refuses the name `save`, alone or
followed by a slot, and an action is not a control, so `set_control` cannot reach `recall` or
`save`.

`get_state` leaves out any control on a function of the `Sensitive` class, the passphrase's, even
one a profile were to declare, so an agent never reads the encoded code.

`identify` is a write for the gate and not for the volume rule: it asks the unit to show itself and
changes nothing, so it never asks and has no cap, but it is listed only with `writes` (one rule
across the web page, MCP and MQTT: identify counts as a write wherever a write gate exists). It
sends `Request::Identify` through the `Fleet` and answers `{"device", "sent"}`; `sent` is false
inside the BLE session's two-second gap, which the tool does not widen or repeat, a device whose
profile's `identify` is false is a tool error (`NAME offers no identify`) before the daemon is
asked, and a device that is down is a tool error in the daemon's words plus the wait to its next
attempt from the resolved `DeviceInfo`. `list_devices` adds `signal` (the strength, its source and
its stamp) to a row once one is known.

## The volume rules

A model must not be able to raise the volume dangerously, and this is what `set_control` and `mute`
enforce, by the shared rule (`safety::judge` with `Rules::mcp`, in
`crates/svolder-profiles/src/safety.rs`):

- **The cap refuses.** A level above `[mcp] max_volume_db` is refused whatever is answered, with the
  limit in the message. (MQTT and the web page clamp to their cap; MCP refuses, so the model is told
  and chooses another level itself.)
- **A rise asks.** A level in dB that rises by more than `MAX_RISE_DB` (6 dB) above the lowest level
  of the last minute asks first. `Floors` keeps that minute: it follows every dB reading the devices
  report (`Tools::watch`), per device and control, so a staircase of small calls adds up to one
  large rise instead of passing each as small, and a knob turned down is a lower floor whether or
  not a tool saw it. The floors outlive a restart of the listener (`Daemon` holds them).
- **Letting sound through asks.** `safety::lets_sound_through` classifies a change a profile marks
  (a gain select raised, a mute released, a device powered on, any change of an `asks_on_change`
  control); each asks. An unmute or power-on while the volume is above the cap is refused.
- **How it asks.** A client that negotiated the stateless revision and declared that it can elicit
  is sent an `input_required` naming the device and both levels (`question`), and the call goes
  through when its retry carries the answer yes (`answer`); a no ends the call with words that tell
  the model not to ask again unless the user does. Any other client gets a tool error saying to ask
  the user and repeat the call with `"confirm": true`. The `confirm` argument is ignored for a
  client that can ask, so a model cannot answer for the user.
- **A built-in setting is judged as its band writes.** `apply_setting` plans the writes from the
  readings (`device_presets::plan`), judges each with `level_guard` and the same `Floors`, and asks
  once with one question that names every band that asks (state `setting|DEVICE|NAME|BANDS`, each
  band with its target and the floor it was judged against, so a yes given under other floors does
  not answer it). Every band the setting sets is judged, not only the ones this plan writes,
  because the writer plans again from its own readings: a band already on target is judged against
  its floor alone. A plan with no write answers `NAME is already on` and sends no batch. Once it
  is applied, each band that asked starts its window over at the level agreed to and each other
  band notes its new level, as a single `set_control` does.
- **A slot recall always asks.** The bands a slot sets are the device's to say, so the question
  names the highest level the recall action's `changes` controls can reach
  (`device_presets::recall_ceiling_db`; state `recall|DEVICE|SLOT`). Nothing is reset by hand: the
  floors follow the readings the device reports (`Floors::watch`).
- **Motion.** Once the cap and the question allow a volume write it goes through the writer's
  `glide`, which lands it or climbs above three quarters of the range a tenth of the range a second;
  the tool waits for a climb up to `CLIMB_WAIT` (5 s) and answers with the level the device reached.

## Principles

**Read-only by default, and the server says so.** `writes = false` lists no write tool and refuses a
stale call by naming the setting, so a client learns what to ask the owner for instead of failing
opaquely.

**The model is not trusted with the volume.** The cap refuses, the rise asks, the floor is
remembered across calls, and a `confirm` flag counts only for a client that cannot ask itself.
Soften any of these and a loop of small calls or one confident guess reaches a level the owner never
agreed to.

**Same door as everyone.** Tools call the `Fleet` and the writer; they hold no session and no second
copy of the rule. The rule is the shared `safety::judge`, so a change to it reaches MCP too.

**Learn nothing without credentials.** A bad token gets an empty 401, a wrong `Host` or `Origin` is
refused before any tool runs, and the body, header and connection limits bound what an
unauthenticated peer can cost. `auth = "none"` is for loopback only, checked by the config and again
by `prepare`.

**A failed listener never stops the daemon.** The devices are the daemon's job; an address in use is
a state to report.

**Data in, data out.** Results are device data and configured names; arguments are validated against
the profile. Nothing a tool returns is something an outside party chose.

## Where the code and tests live

- The server: `bins/svolderd/src/mcp.rs`, tests in `bins/svolderd/src/mcp/tests.rs`.
- The tools: `bins/svolderd/src/mcp/tools.rs`, tests in `bins/svolderd/src/mcp/tools/tests.rs`.
- The shared HTTP loop and host list: `bins/svolderd/src/http.rs`.
- The config: `McpConfig` in `crates/svolder-ipc/src/config.rs`; tokens in
  `crates/svolder-ipc/src/token.rs`.
- End to end over HTTP against the fake transport: `bins/svolderd/tests/mcp.rs`,
  `bins/svolderd/tests/mcp_tools.rs`, `bins/svolderd/tests/mcp_walkthrough.rs`.
