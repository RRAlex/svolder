<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's controls

One switch on each front end flips its volume and its tone control to the computer: the computer's
sink volume and mute, and the computer EQ, a parametric correction of up to 16 points. The daemon
serves them (`host_volume`, `host_mute` and `host_eq`); this page is how the web UI reaches them.
The sink, the chain and the state file are in [host-audio.md](host-audio.md); the presets both sides
share are in [tone-control-and-presets.md](tone-control-and-presets.md).

The reason is the couch: the music comes from the computer and the person holds a phone. One switch
flips the volume and tone control that drive the DAC to the computer, so nothing new is drawn.
Flipping writes nothing; the sound changes only by a key, a click, a drag or a wheel.

## The live chain

The computer EQ is a PipeWire filter chain of a preamp and sixteen biquad slots per channel, `lpre`,
`l01` to `l16` and the same for the right channel (`crates/svolder-host/src/eq/live.rs`). A point is
a slot's parameters, so adding, moving, retyping or removing a point is a `set-param` on the running
node and no rebuild: nothing is heard but the change. A spike on PipeWire 1.6.2 and a private
WirePlumber settled the shape:

- **Raw slots.** Every slot is a `bq_raw` biquad set by its coefficients, which svolder works out
  for the rate the sink runs at (the cookbook forms in `svolder-profiles`'s `tone/response.rs`). Any
  type fits any slot, so a change of type is live too. A typed slot (a builtin peaking, low shelf or
  high shelf biquad fixed by the node's label) takes a gain live but rebuilds for a new type, and is
  the alternative the spike measured and did not choose.
- **A linear preamp.** The preamp is a `linear` node whose `Mult` changes live (no 0 Hz shelf).
- **One held `pw-cli`.** The session (`eq/session.rs`) keeps one `pw-cli` open and writes commands
  to its stdin; it did 1000 changes in 40 ms and stayed healthy at 20 a second. A change is `pw-cli
  set-param <node> Props { params = [ "<slot>:<name>" <value> ... ] }`, and `pw-dump` reads them
  back.
- **No gap.** A 5 s drag at 20 changes a second left no gap and no xrun: the end-to-end check
  measures 100 updates, 0.02 ms of near-silence and no step over 1.0 dB in a millisecond (0.5
  measured). Putting the chain in or out of the path costs one short gap, 43 ms typical and 85 ms
  worst seen, while WirePlumber relinks; Off to EQ and EQ to Off are the only edits that do.
- **The route.** On WirePlumber 0.5 the chain is a smart filter and the default sink stays the real
  one; older WirePlumber gets the chain as a sink of its own ([host-audio.md](host-audio.md)).

**Points and slots.** A point is `{type, f, g, q}` with a type `PK`, `LSC` or `HSC`. Point 1 is
always there: a flat curve is one peak at 1 kHz, 0 dB, Q 1. At most 16 points. A point keeps its
slot for its life, so moving one past another changes no slot and a live update touches the one slot
that moved. The table lists the points by frequency: the daemon puts them in that order when an edit
ends (a drag's release, a typed value, an add, a recall) and never while a drag goes on, so a row
never jumps under a finger. The wire carries each point's slot, which is what a drag holds on to
when another front end's release re-sorts the list. Removing the last point sets it to 0 dB and
keeps its frequency, type and Q. A hand sets frequency 20 Hz to 20 kHz, gain ±12 dB and Q 0.1 to 10:
the controller holds a set and an add to them (the web refuses an add outside them, 422). A preset
inside the correction format's ranges plays as written and an edit of one field leaves the others
alone; one outside them is refused.

**The preamp is automatic.** It is the lower of a preset's stated preamp and minus the curve's peak,
sent in the same call as every live update, so no whole update passes unity, mid-drag included
(PipeWire's half-applied update can, rarely; see the limit below): a falling preamp goes before the
slots and a rising one after. What is measured and refused, the EQ turned off by a rate change and
an older svolder's chain: [host-control-preamp.md](host-control-preamp.md). The line
`7 of 16 points · preamp −6.0 dB` says how much quieter a boost makes the signal. A hand edit of a
preset names it `custom`.

## The controller

`EqControl` (`eq/control.rs`) is the one owner. It holds the points, the chain and the session, and
publishes its `EqView` on a watch channel that readers take without waiting for it.

- **Ticks.** A 50 ms tick is the coalescer: an edit with `done: false` marks its slot dirty and the
  tick sends the dirty slots and the preamp in one call, so a point gets at most 20 sends a second
  and the last value is always sent. `done: true` sends at once and saves the state file; nothing
  else is written to disk per live update.
- **Off and on.** Off removes the chain from the path and keeps the points; EQ puts it back. An edit
  made while Off turns the EQ on first, as a band write on a DAC turns Flat into EQ. The first EQ
  applies the chain with the kept points, one short gap on that click.
- **Read back.** After the first live send one value is read back. Three misses in a row mean this
  PipeWire does not take live changes: the controller says so once (`this computer's PipeWire takes
  EQ changes when a drag ends`) and applies on release, a rewrite and a restart, from then on.
- **The rate.** Every 20th tick (about a second) the node and its running sample rate are read. A
  new node (a new `object.serial`, whatever its id) or a new rate marks every slot dirty and they
  are sent again at the new rate; a chain built from the config alone holds 48 kHz coefficients
  until then.
- **Scenes.** `EqControl::computer_part` captures what the computer plays as a `ComputerPart` (the
  EQ with its stated preamp even while the level is held, loudness, crossfeed, virtual speakers) and
  `EqControl::recall` sets all of one as a single change: one apply, or one live line, never an
  intermediate apply ([scenes.md](scenes.md#recall)). `ComputerPart::matches` compares what is heard
  against the view (`EqView.stated_preamp_db` carries the stated preamp it needs).
- **Bypass.** `EqControl::bypass(on)` (`eq/control/bypass.rs`) is the all-off. On records which of
  the EQ, loudness, crossfeed and virtual speakers play (`HeldStages`) and turns them all off as ONE
  change through the scene path: `computer_part()` with every switch off, `recall`, so one apply or
  one live line, and a stream that plays keeps the chain as a pass-through that is not applied
  again. Off recalls the current part with the held switches on, their settings as they are now. On
  with nothing playing and nothing held is refused (`nothing plays on the computer to bypass`); a
  restore that is refused (a set no longer fetched) leaves the bypass on, with the words. Any stage
  turned on by another path (a switch, a preset load, an edit that turns the EQ on, loudness,
  crossfeed, the speakers, a scene recall) ends the bypass and forgets the held set: each public
  entry point ends with one helper that does it. The held set is saved as `[bypass]` (`eq`,
  `loudness`, `crossfeed`, `speakers`) in `host-eq.toml`, present only while the bypass holds, and
  is adopted at start while no stage is on, so a restart can still bring the stages back.
- **Levels.** `EqView.levels` (`eq/levels.rs`, `ChainLevels`) is the chain's level in dB, made once
  in svolder-host: `eq_db` is the EQ's share of the preamp sent, `space_db` is crossfeed's share
  (its stage cost) or the chosen set's level against Off while the virtual speakers play, `total_db`
  is the preamp sent plus the speakers' level (loudness is left out: it follows the volume), and
  `passthrough` says the chain plays flat with no stage. `words` is the one sentence every front end
  shows: nothing with no chain, `plays −14.8 dB in all`, `pass-through −7.6 dB until the music
  pauses`; while a climb runs (see [host-control-preamp.md](host-control-preamp.md#the-climb))
  `climb_to_db` holds where it ends and `words` says `climbing to 0.0 dB`; `untouched` is true only
  for a pass-through that reached exactly 0 dB on a chain started from an identity config with every
  port read at its identity value, and then `words` is `pass-through, untouched`. `EqView.bypass`
  (`BypassView { on, held }`) carries the bypass; a peer without either sends none and then no front
  end offers a bypass.
- **Idle.** A chain kept as a pass-through for a stream (the last stage or the EQ went off while it
  played) is removed once nothing has played through it for `IDLE_KEEP` ticks, 60 (3 s): the watcher
  looks every second, and three looks in a row that find no `running` stream remove it, so a seek, a
  track change or a pause of two seconds keeps it. The held level's lift (a chain whose config holds
  the curve, applied again with the curve's own preamp) waits for the same time.
  `SVOLDER_EQ_IDLE_TICKS` sets it.
- **Failure.** Thirty failed sends in 5 s make the controls unavailable (`computer controls
  unavailable: <why>`, as a missing `pw-cli` or `pipewire` does). A `pw-cli` restart is ridden out.
- **Never on its own.** Nothing applies at start; `set_on(true)` is the user's action. Only in a
  container does the daemon apply the kept points once at start, when the state says on. A reload
  never stops the controls or their chain; only the task that follows the players restarts.

## The daemon's requests and events

The socket carries `HostVolume { pct }`, `HostMute { muted }` and `HostEq { op }`, where `op` is
`Mode { on }`, `Preset { name, device?, confirm? }`, `Apply { name, origin?, correction, sink? }`,
the point edits (`Set { index, slot?, kind?, f?, g?, q?, done }`, `Add`, `Remove { index, slot? }`)
and `SaveAs { name }`; the index is the point's place in the list the view sent, `slot` (1 to 16)
the point itself: with one the daemon changes the point in that slot whatever place it has by now
and ignores the index, and a slot no point holds is refused (`point in slot N has moved or is gone`;
a 409 on the web, a 422 for a slot outside 1 to 16) with nothing changed. With none (an older
client) the index is used. Every front end that holds a slot sends it, so a re-sort between a read
and a request never lands a value on a neighbour. A recall that a connected device plays too answers
`Response::Ask` and changes nothing. The computer's volume and `EqView` ride `HostView.volume` and
`HostView.eq`, in the `Host` answer and in `Event::Host`, each point with its slot. The sink is read
only while a page or a TUI watches the computer, every 2 s, and a read never writes.

**A drag follows its point.** A page or TUI holds the dragged point by its slot. Every tick and the
release resolve that slot to the point's current place, so another client's release that re-sorts
the list does not move the drag onto another point; a point removed elsewhere, a preset recall or a
slot renumbering ends the drag with no write. For 2 s after a hold ends a client reads the model the
daemon sent before the release and a re-sort reaches it with the next read.

**What the path says.** With a live chain in the path the path changes the sound, and it names the
stages the daemon's EQ view says play: `host EQ on`, `loudness on`, `crossfeed on`, `virtual
speakers on`, or `EQ pass-through` for the flat chain kept while a stream plays; a pass-through the
daemon proved untouched (`ChainStages.untouched`) is told no stage word and the computer hop is
green. `RouteFacts.stages` carries them; when the view cannot vouch for them (controls unavailable,
an older svolder's chain) there is none and the path says `host EQ on`.

**The fake.** `SVOLDER_FAKE_HOST_EQ` and `SVOLDER_FAKE_COMPUTER_VOLUME` give the simulated daemon a
computer side whose route follows the fake volume and mute; `make demo-computer` runs it.

## The web routes

Every route is a `POST` through the web's writer: a guest is refused 401 and a page with
`[web] writes` off 403, as for any write (`web/host/computer.rs`).

| Route | Body | Does |
| --- | --- | --- |
| `api/v1/host/volume` | `{"pct"}` | the sink's volume, 0 to 100 |
| `api/v1/host/mute` | `{"muted"}` | mute or unmute |
| `api/v1/host/eq` | `{"on"}` | the EQ on or off; off keeps the points |
| `api/v1/host/bypass` | `{"on"}` | the bypass on or off (every stage off as one change, brought back as it was) |
| `api/v1/host/eq/preset` | `{"name", "device"?, "confirm"?}` | a preset on the computer; `device` is the shown device's name (the URL segment, decoded), whose built-ins are used |
| `api/v1/host/eq/points` | `{"type", "f", "g", "q"}` | add a point |
| `api/v1/host/eq/points/{n}` | `{"type"?, "f"?, "g"?, "q"?, "done"?, "slot"?}` | change point `n`, or the point in `slot` |
| `api/v1/host/eq/points/{n}/remove` | `{"slot"?}` or none | remove point `n`, or the one in `slot` |
| `api/v1/devices/{device}/presets` | `{"name", "from": "computer"}` | save every point as a preset |

Points count from 1 and a type is `PK`, `LSC` or `HSC`. `done` is false while a drag goes on and,
when absent, true: the daemon coalesces what a drag sends, so the page sends at most 20 a second per
point and always the last value. The answer is `{"ok": true}`; 409 with the daemon's words when it
refused (a `slot` no point holds included; the bypass with nothing playing; a restore it cannot
make); 422 for a value out of its range (a volume outside 0 to 100, a bypass body that is not
`{"on": bool}`, a point type that is not one of the three, point 0, a slot outside 1 to 16, an
added point outside the hand ranges); 503 with `computer controls unavailable: <why>` while the
controls are unusable.

### The question

A preset that a connected device's tone control plays too would play twice. The daemon answers the
recall without `confirm` with `Response::Ask { message, name, devices }`, never changing anything
(any other refusal is a `Response::Error`; an unavailable computer has code 6,
`CODE_COMPUTER_UNAVAILABLE`), and the route answers 409 and

```json
{
  "error": "Warm is on the dac too: the correction would play twice",
  "ask": {
    "kind": "both_sides",
    "name": "Warm",
    "devices": ["dac"],
    "question": "Warm is on the dac too: flatten the dac and apply on the computer?"
  }
}
```

The page puts `ask.question` to the user. Yes sends the same request again with `"confirm": true`,
which flattens the devices first; no sends nothing. A preset that changes nothing is not asked.

## The model

`HostModel.computer` rides the `host` event and `api/v1/host` (`?device=<url>` names the shown
device, whose own built-in presets the computer lists). It is present while the daemon serves the
computer, for any shown device, and `null` with the host off.

- `volume`: `{pct, muted, sink}`, absent until the daemon has read the sink, which it does only
  while a page's stream is open; `pct` is above 100 when another program boosted the sink, and the
  page then writes no more than 100.
- `eq`: `{on, running_unknown, points[{type, f, g, q}], preamp_db, preset, live, note, max, loads,
  response}`. `response` is the points' curve in dB at the tone control's 181 log-spaced frequencies
  (20 Hz to 20 kHz), the preamp left out. `running_unknown`: `on` is the state file's. `loads`
  counts loads and reloads (a held value ends when it moves). `live` is false with no chain.
  `levels` is the host's `ChainLevels` (`eq_db`, `space_db`, `total_db`, `climb_to_db`,
  `passthrough`, `untouched`, `words`) and `bypass` its `BypassView` (`on`, `held` with `eq`,
  `loudness`, `crossfeed`, `speakers`); both are `null` from a host that has none. The page's core
  reads them with `core.computer.levelsOf` (the EQ, Loudness and Space stages with their levels,
  the total and the daemon's words; numbers are only formatted there) and `bypassOf` (`on`, `can`,
  `held`), and writes with `actions.computer.setBypass(on)`; the contract lists a `bypass` target
  while the model has one.
- `presets`: `[{name, built_in, on, playable, why}]`, the built-ins of the shown device's tone
  control and the user's presets. One of more than 16 filters is listed with `playable: false` and
  its reason. On a page whose device has no tone control (an amp) there is no band layout to
  shape the built-ins with, so only the saved presets are listed.
- `unavailable`: the whole `computer controls unavailable: <why>` sentence.

## The page's core

Every theme draws the computer's side from `core.computer` and writes through
`core.actions.computer`, so the rules are kept once.

- **The side** is the viewer's own, kept in the browser (`svolder.side`, `dac` or `computer`, `dac`
  by default) and never sent to the daemon. The switch is offered for any shown device, an amp too,
  while the model holds the computer. With the computer unavailable the DAC side is shown whatever
  was kept, and the sentence of why takes the switch's place. A flip writes nothing, nor does a load
  with the side kept; it ends any drag or hold a theme registered (`computer.track`).
- **What the viewer is changing is held.** A volume, mute, mode or a point's value asked for is
  drawn as asked, a host update meanwhile does not move it, and once the writes have settled the
  computer's model is read again and the held values go. A point's live updates are sent at most
  20 a second, the last value always at release.
- **The first paint** has no volume yet: it reads `—` and is inert, never a live 0 %. An EQ whose
  state is only what the state file says is never drawn as verified.
- **The question** (`core.ask(question) -> Promise<boolean>`) is a card with Yes and No, both
  at touch size, the focus on No, `y`, `n` and Escape answering. The same preset on both sides
  is put to the person through it (Ember's card; Signal, Paper and Night drive pass their own `asker` and put the question in their own card, a line or a sheet): yes repeats the recall with `confirm`, no sends nothing.
- **The targets** a theme's test driver reaches are listed by `computerTargetsOf(page)` in
  `core/contract.js`: the switch (`flip`), the volume, mute and mode, `add`, each point's
  `type`, `f`, `g` and `q`, `remove` once there is more than one point, and each playable preset.

## The TUI

One switch, `App::side` (`Side::Dac` or `Side::Computer`), flips every screen that has one: the EQ
tab's `Target ‹ NAME | Computer ›` row, with the mode `‹ Off | EQ ›`, the Points table and the
Correction pane's "nothing is fitted"; and the Controls tab's Volume row, under the same `Target` row (a focusable row on both screens). Flipping writes nothing
but the `done` of a drag still held, for the value it already plays. The state the screens draw is
`Host::computer` (the EQ view, the volume, why the controls cannot be used), filled from the
daemon's `HostView` when joined and from the loop's own controller when not.

Every change is an `Intent` the loop carries out in `run/computer.rs`. Joined to a daemon it is a
request (`HostEq { op }`, `HostVolume`, `HostMute`), run one at a time so a drag's last value lands
last; the daemon's both-sides question comes back as `Response::Ask` and the preset list asks it, a
yes repeating the recall with `confirm`. With no daemon the loop owns an `EqControl` over the
process's one `Host`, ticked every 50 ms, and its watch channel feeds the view. A drag keeps its own
values on screen while it runs and sends at most 20 a second, the last always.

## The flip in each theme

Each web theme draws the flip and the computer's controls in its own way (Ember, Paper, Signal and
Night drive): see [the second page](host-control-themes.md).

## MCP

`get_host` reads the computer (volume, mute, the EQ and its points, the player, the loudness, the
crossfeed and the virtual speakers). `set_host_volume` takes a whole percent from 0 to 100; above
100 is refused (the cap is 100 %, not `max_volume_db`). A rise of more than 6 dB on the volume's
cubic scale asks the client, counted from the lowest level this server saw, so a staircase of small
steps asks at the sum; a level not read yet counts as 1 %. `host_mute` unmuting asks, muting does
not. `set_host_eq` takes exactly one of a preset name or `off`; a built-in needs a device with a
tone control, a saved preset needs none, and a preset on a device too asks the client, repeated with
`confirm`. `set_host_loudness` ([loudness](loudness.md)) takes the strength and the reference and
never asks, since it only cuts. `set_host_crossfeed` ([crossfeed](crossfeed.md)) and
[`set_host_speakers`](virtual-speakers.md) each answer with both, since one on turns the other off.
The writes are listed only while `[mcp] writes` is on.

## MQTT

Home Assistant gets one device for each computer, named by its host name (`svolder_computer_<host>`,
[mqtt-and-home-assistant.md](mqtt-and-home-assistant.md)): a switch (a binary sensor with commands
off) that turns the computer EQ on with its kept points and off, and a sensor naming the applied
preset (`custom`, or nothing); a loudness switch and strength number; a crossfeed and a speakers
switch and select; and the scene select and text ([scenes.md](scenes.md)). No computer volume, mute
or point is offered; HA's own media integrations do volume better. A retained command is never acted
on; ON with nothing kept (the first point flat, no preset) logs that and stays off. With the
computer not served the EQ entities are not announced and the retained ones are cleared at the
bridge's start.

## Packages and Docker

The deb and rpm recommend the PipeWire tools (`pipewire-bin`, `wireplumber`; `pipewire-utils` on
rpm) without depending on them, the AUR lists them as optional with what each is for, and the Nix
package wraps `svolderd` and `svolder` with the tools on their PATH. The image installs the client
tools; the opt-in `docker-compose.host-audio.yml` (which `make docker` writes into `.env` when a
PipeWire socket is there) mounts the host's runtime directory read-only, runs as the host's uid, and
points `PIPEWIRE_RUNTIME_DIR` at it. In a container the chain is the daemon's child (`pipewire -c`),
which ends with the daemon and puts the path back to unfiltered; at start the daemon applies it
again if the state says on. Outside Docker the chain stays a user unit, or a detached process.

## The checks

- Rust: tests on fakes in `make check` for the controller, the session, the daemon's requests, MCP,
  MQTT and the TUI; the volume rule's tests are on the safety list.
- The page: `core-computer.mjs` (the core), a `computer_*.py` driver for each theme in both browsers
  on the fake daemon, and the contract and completeness checks, which list every computer target.
  The Night drive timing check has a computer-side case with every cell lit (4.5 ms in Chromium).
- `make host-e2e`: the real daemon and chain on a private PipeWire with a null sink, never the
  session. It plays a tone, drags and retypes points at 20 updates a second, forces a sample-rate
  change (again after a live softening: never over unity), turns the EQ off, checks the speakers
  (V0-V11), volume and mute with `wpctl`, and that every process it started is gone
  ([host-e2e.md](host-e2e.md)).
- `make demo-computer`, `make demo-computer-amp`: the page with a simulated computer, audio left
  alone.

## Known limits

- **A live update can play a half-applied filter for one quantum.** PipeWire's filter chain applies
  a live control update one value at a time, and nothing stops the audio thread from playing in
  between. Rarely (measured about 1 in 6,000 risky updates on PipeWire 1.6.2; at ten glide ends a
  day, roughly once every year or two) one quantum, about 1/50 s at 1024 samples and 48 kHz (a
  smaller quantum makes it likelier), plays a half-applied filter. A small step is harmless (a
  loudness glide step: about +0.3 dB). These are loud:
  - the daemon stopping gracefully, a TUI with no daemon quitting, a reconfigure that halts the
    computer's side, or loudness turned off, while loudness plays at a depth: +31 to +43 dB in the
    bass measured, up to +50 dB worked out;
  - the end of a loudness glide back to flat: +29 dB in the bass measured;
  - adding or removing a computer EQ point, a live preset recall that changes the number of points,
    the EQ turned on or off live: an unstable filter or full scale measured;
  - dragging an existing point: up to about +17 dB worked out, not measured;
  - switching the virtual speakers or crossfeed: up to about 6 dB worked out.

  After a bad quantum the chain recovers by itself. Live edits of the computer EQ have had this
  since they began (0.35), so v0.36.7 has it. svolder cannot close it without giving up live updates
  or a much larger redesign; the fix belongs in PipeWire. See
  [host-control-preamp.md](host-control-preamp.md#what-the-lab-cannot-see) and
  [known-issues.md](../known-issues.md).
- **A sample-rate change leaves the sound up to about 2 s shifted, never over unity.** The chain is
  rebuilt with the coefficients it was applied with until the controller reads the rate (about a
  second) and sends every slot again. After a curve was softened by hand the level stays at the
  applied curve's preamp until the next apply (see
  [host-control-preamp.md](host-control-preamp.md)).
- **A dragged computer point dips the level by up to about 0.6 dB per update.** The level
  moves with the preamp as the curve's peak moves; the end-to-end check's net level over a drag was
  within −0.12 dB of the start and a 1 ms step at most 1.1 dB.
- **A model older than 2 s re-sorts a row on its next read.** The 2 s follow after a hold covers
  every observed case; a stale model costs a missed re-sort, never a wrong write (a point write
  carries its slot, and a slot that is gone is refused).
- **A jank during the Night drive's Full probe can keep a lower tier.** The ladder walks back up,
  the wet road coming back within about 3 minutes.
