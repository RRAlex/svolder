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
- **A linear preamp.** The preamp is a `linear` node whose `Mult` changes live (a 0 Hz high shelf
  was the alternative).
- **One held `pw-cli`.** The session (`eq/session.rs`) keeps one `pw-cli` open and writes commands
  to its stdin; it did 1000 changes in 40 ms and stayed healthy at 20 a second. A change is
  `pw-cli set-param <node> Props { params = [ "<slot>:<name>" <value> ... ] }`, and `pw-dump` reads
  the values back.
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
sent in the same call as every live update, so the signal never passes unity, mid-drag included: a
falling preamp goes before the slots and a rising one after. What is measured and refused, the EQ
turned off by a rate change and an older svolder's chain:
[host-control-preamp.md](host-control-preamp.md). The line `7 of 16 points · preamp −6.0 dB` says
how much quieter a boost makes the signal. A hand edit of a preset names it `custom`.

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
  new node or a new rate marks every slot dirty and they are sent again at the new rate; a chain
  built from the config alone holds 48 kHz coefficients until then.
- **Failure.** Thirty failed sends in 5 s make the controls unavailable (`computer controls
  unavailable: <why>`, as a missing `pw-cli` or `pipewire` does). One restart of `pw-cli` is ridden out.
- **Never on its own.** Nothing applies at start; `set_on(true)` is the user's action. Only in a
  container does the daemon apply the kept points once at start, when the state says on. A reload
  never stops the controls or their chain; only the task that follows the players restarts.

## The daemon's requests and events

The socket carries `HostVolume { pct }`, `HostMute { muted }` and `HostEq { op }`, where `op` is
`Mode { on }`, `Preset { name, device?, confirm? }`, `Apply { name, origin?, correction, sink? }`,
the point edits (`Set { index, slot?, kind?, f?, g?, q?, done }`, `Add`, `Remove { index, slot? }`)
and `SaveAs { name }`; the index is the point's place in the list the view sent, `slot` (1 to 16) the
point itself: with one the daemon changes the point in that slot whatever place it has by now and
ignores the index, and a slot no point holds is refused (`point in slot N has moved or is gone`; a 409
on the web, a 422 for a slot outside 1 to 16) with nothing changed. With none (an older client) the
index is used. Every front end that holds a slot sends it, so a re-sort between a read and a request
never lands a value on a neighbour. A recall that a connected device plays too answers
`Response::Ask` and changes nothing. The computer's volume and `EqView` ride `HostView.volume` and
`HostView.eq`, in the `Host` answer and in `Event::Host`, each point with its slot. The sink is read
only while a page or a TUI watches the computer, every 2 s, and a read never writes.

**A drag follows its point.** A page or TUI holds the dragged point by its slot. Every tick and the
release resolve that slot to the point's current place, so another client's release that re-sorts the
list does not move the drag onto another point; a point removed elsewhere, a preset recall or a
slot renumbering ends the drag with no write. For 2 s after a hold ends a client reads the model the
daemon sent before the release and a re-sort reaches it with the next read.

**What the path says.** With a live chain in the path, flat or not, the daemon says `host EQ on` and the path changes the sound.

**The fake.** `SVOLDER_FAKE_HOST_EQ` and `SVOLDER_FAKE_COMPUTER_VOLUME` give the simulated daemon a
computer side whose route follows the fake volume and mute; `make demo-computer` runs it with the page.

## The web routes

Every route is a `POST` through the web's writer: a guest is refused 401 and a page with
`[web] writes` off 403, as for any write (`web/host/computer.rs`).

| Route | Body | Does |
| --- | --- | --- |
| `api/v1/host/volume` | `{"pct"}` | the sink's volume, 0 to 100 |
| `api/v1/host/mute` | `{"muted"}` | mute or unmute |
| `api/v1/host/eq` | `{"on"}` | the EQ on or off; off keeps the points |
| `api/v1/host/eq/preset` | `{"name", "device"?, "confirm"?}` | a preset on the computer; `device` is the shown device's name (the URL segment, decoded), whose built-ins are used |
| `api/v1/host/eq/points` | `{"type", "f", "g", "q"}` | add a point |
| `api/v1/host/eq/points/{n}` | `{"type"?, "f"?, "g"?, "q"?, "done"?, "slot"?}` | change point `n`, or the point in `slot` |
| `api/v1/host/eq/points/{n}/remove` | `{"slot"?}` or none | remove point `n`, or the one in `slot` |
| `api/v1/devices/{device}/presets` | `{"name", "from": "computer"}` | save every point as a preset |

Points count from 1 and a type is `PK`, `LSC` or `HSC`. `done` is false while a drag goes on and,
when absent, true: the daemon coalesces what a drag sends, so the page sends at most 20 a second per
point and always the last value. The answer is `{"ok": true}`; 409 with the daemon's words when it
refused (a `slot` no point holds included); 422 for a value out of its range (a volume outside 0 to
100, a point type that is not one of the three, point 0, a slot outside 1 to 16, an added point
outside the hand ranges); 503 with `computer controls unavailable: <why>` while the controls cannot
be used.

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
- `eq`: `{on, running_unknown, points[{type, f, g, q}], preamp_db, preset, live, note, max,
  response}`. `response` is the points' curve in dB at the tone control's 181 log-spaced
  frequencies (20 Hz to 20 kHz), the preamp left out. `running_unknown`: `on` is the state file's.
- `presets`: `[{name, built_in, on, playable, why}]`, the built-ins of the shown device's tone
  control and the user's presets. One of more than 16 filters is listed with `playable: false` and
  its reason. On a page whose device has no tone control (an amp) there is no band layout to
  shape the built-ins with, so only the saved presets are listed.
- `unavailable`: the whole `computer controls unavailable: <why>` sentence.

## The page's core

Every theme draws the computer's side from `core.computer` and writes through
`core.actions.computer`, so the rules are kept once.

- **The side** is the viewer's own, kept in the browser (`svolder.side`, `dac` or `computer`,
  `dac` by default) and never sent to the daemon. The switch is offered for any shown device, an
  amp too, while the model holds the computer. With the computer unavailable the DAC side is
  shown whatever was kept, and the sentence of why takes the switch's place. A flip writes
  nothing, nor does a load with the side kept; it ends any drag or hold a theme registered with
  `computer.track`.
- **What the viewer is changing is held.** A volume, mute, mode or a point's value asked for is
  drawn as asked, a host update meanwhile does not move it, and once the writes have settled the
  computer's model is read again and the held values go. A point's live updates are sent at most
  20 a second, the last value always at release.
- **The first paint** has no volume yet: it reads `—` and is inert, never a live 0 %. An EQ whose
  state is only what the state file says is never drawn as verified.
- **The question** (`core.ask(question) -> Promise<boolean>`) is a card with Yes and No, both
  at touch size, the focus on No, `y`, `n` and Escape answering. The same preset on both sides
  is put to the person through it: yes repeats the recall with `confirm`, no sends nothing.
- **The targets** a theme's test driver reaches are listed by `computerTargetsOf(page)` in
  `core/contract.js`: the switch (`flip`), the volume, mute and mode, `add`, each point's
  `type`, `f`, `g` and `q`, `remove` once there is more than one point, and each preset the
  computer can play.

## The TUI

One switch, `App::side` (`Side::Dac` or `Side::Computer`), flips every screen that has one: the EQ
tab's `EQ for ‹ NAME | Computer ›` line, with the mode `‹ Off | EQ ›`, the Points table and the
Correction pane's "nothing is fitted"; and the Controls tab's Volume row. Flipping writes nothing
but the `done` of a drag still held, for the value it already plays. The state the screens draw is
`Host::computer` (the EQ view, the volume, why the controls cannot be used), filled from the daemon's
`HostView` when joined and from the loop's own controller when not.

Every change is an `Intent` the loop carries out in `run/computer.rs`. Joined to a daemon it is a
request (`HostEq { op }`, `HostVolume`, `HostMute`), run one at a time so a drag's last value lands
last; the daemon's both-sides question comes back as `Response::Ask` and the preset list asks it, a
yes repeating the recall with `confirm`. With no daemon the loop owns an `EqControl` over the
process's one `Host`, ticked every 50 ms, and its watch channel feeds the view. A drag keeps its own
values on screen while it runs and sends at most 20 a second, the last always.

## The flip in each theme

### Ember

The switch is `#part-switch`, a part of its own beside the shown device's name in the header: the
device's name and `This computer` in the mode track's style (`.seg`), swapped out of band as the
link's dot is. Where the computer cannot be used the same place carries the reason. It is drawn for
any shown device, an amp too, while the model has the computer's controls (`draw/computer.js`).

On the computer side `computerView(core)` (`computer-view.js`) hands the drawing the core's
`volumeOf` and `toneOf`; the screen is the DAC page's layout, so an amp page flipped to the computer
has the same knob, tone section and presets as a DAC page, and flipping back restores its own.

- **Volume.** The same `sv-knob` over a model of 0 to 100 % (labels 0, 25, 50, 75, 100; no zone, no
  cap tick); `data-computer` sends its steps and mute through `actions.computer`. The first paint
  reads `—`; a level above 100 % shows as read with the dial at its end.
- **Tone control.** Mode `Off | EQ`. The curve's points are tapped on (an empty spot adds a PK),
  dragged (frequency and gain) and set for Q by the wheel, a right drag or a pinch: `computer.js`
  makes the gestures with `createDrag` over `computerSteps()` and `pointWriter`, and a flip ends them
  (`computer.track`). While a point is dragged the part's swap waits (`keep.js`) and the curve, the
  point and the row's cells follow what the core holds; the part is drawn again from the host's
  model when the writes are answered.
- **The order** of the points is the daemon's: `HostEqState.filters` is kept sorted by frequency (a
  stable sort) when an edit ends (a drag's last value, a typed value, an add, a preset recalled or
  loaded) and never during a drag. The wire's 1-based point numbers are that order; the point's `slot`
  is its identity: a front end that holds a point across an edit, its own or another's, finds it by
  the slot, and sends it with every write. The web core keeps a drag's holds by slot, looks the
  number up again before every write and ends the drag with no write when the point is gone; it tells
  the themes where a held point went with the `moved` event (`from`, `to`). The TUI does the same for
  a pointer drag and for its focus.
- **The table** shows six rows of Type, Frequency, Gain, Q and `×` (labelled `Remove point N`; on a
  lone point it sets the point to 0 dB and keeps its frequency, type and Q) and scrolls; `N more ↓
  scroll` counts the rows below. The dragged point's row is lit and scrolled into view. A value is
  typed into its cell (`2.5k`, `+3,2`), the type is a choice of PK, LSC, HSC. `+ Add point` adds a
  PK at the widest gap; the line reads `7 of 16 points · preamp −6.0 dB`. A table of 16 rows is as
  tall as one of 7.
- **Presets** are the shared list, with `Save as preset` (the points on show), and rename and delete
  for a viewer's own; one the EQ cannot play is disabled with its reason. A preset a connected
  device plays too is put as the core's question (`core.ask`).
- The switch's halves have stable ids (`side-dac`, `side-computer`): a flip from the keyboard keeps
  the focus on the chosen half across the redraw. A point's marker sits on the summed curve at its
  frequency (`toneOf`, shared by every theme), so a shelf's dot is on the line.
- Every computer control carries `data-comp` and no `data-json`, so the DAC side's handlers leave it
  alone; for a guest each is `aria-disabled`, and the switch alone works.

### Paper

The switch is a line by the title, `showing: the DAC · this computer` (`the amp` on an amp's page), the
side shown underlined in pencil as Paper marks every chosen word, the other dotted. When the
computer cannot be used the line is replaced by the sentence of why. The ruler reads 0 to 100 %
(labels at 0, 25, 50, 75, 100, a tick every 2 %, no zone and no cap) with the percent beside the
needle; before the first read the number is `—` and the needle is not drawn. The tone is written
`TONE · off · EQ` with the mode chosen underlined; an EQ whose state is only what its file says is
drawn with a dashed underline and a line saying so. The curve has a pencil point for each point;
under it a table (`#`, `type`, `frequency`, `gain`, `Q`, `×`) shows six rows and a half and scrolls
(`N more below`, counting what is not fully in view); below it `+ Add point` and the line
`7 of 16 points · preamp −6.0 dB`. The table keeps its height, so sixteen points never grow the sheet: the add
word's place stays when it goes.

The hands are `computer-wire.js` (the drawing is `draw/computer.js`, `draw/volume.js` reuses the
DAC's ruler through `percentVolume` in `ruler.js`). A point is dragged on the graph with the core's
`createDrag` over `computerSteps()` and `pointWriter`; a tap on an empty spot adds a PK at that
place; the arrow keys step a focused point and the wheel its Q; a cell takes typed text (held to the
hand ranges) or the up and down keys; the type word opens Paper's list. The row and the mark of the
point in hand are drawn selected. A flip ends a drag through `computer.track` (the last value is
sent, nothing else) and runs the same closing of lists and holds as another device shown.

On an amp's page the switch is drawn too, and the computer's side is the DAC page's own volume, tone
and presets sections (the amp's power and meters stay); the device's notes are left out. Paper's `×`
and `−` are in its faces, and the table's down hint is a drawn stroke, not an arrow glyph.

### Signal

The flip is the line's own hop: the computer's hop is named `This computer`, says `PipeWire · <rate>`
and, while it is the hop chosen, `controls below` (or why the controls cannot be used). Choosing it
is the flip: the side is the computer's exactly while that hop is chosen, any other hop writes
`dac`, and a load with the computer kept (and usable) chooses the hop (`keepChoice`, `sideOf` in `line.js`). On an
amp's page the line has the hop too while the computer is served.

The area below is the DAC's usual screen acting on the computer (`draw/computer.js`,
`computer-wire.js`): the level in % with `−`, Mute, `+` and the track (`—` until read), the curve with
a dot on each point (on the curve, at the summed response at the point's frequency), the mode
`Off | EQ` as chips, the points' table (six rows in view, `×` labelled `Remove point N`,
`+ Add point`, the line `7 of 16 points · preamp −6.0 dB`), the presets with Save as, Rename and
Delete, and, last, the facts of the route that are not controls: the sound card and its format, the
other streams and the verdict on the sound's way (the daemon's words name PipeWire there, not the computer). Nothing below the line says `computer`, but for the path's own words (its `computer: …` hop line, `computer volume N %`, `computer muted`). A preset
the DAC plays too asks the core's question.

### Night drive

The switch is two keys, the shown device's name and `This computer`, in a slot of their own left of
the menu button (`#nd-switch`, by `#nd-slot`); on a phone it is in the scene's top column, under the
device line (held sideways, at the left). They are drawn for any shown device, an amp too, while the model has the computer's controls, and where the
computer cannot be used the sentence stands under them, in the switch's own place. They are not the
dash's: the dash fades to a fifth with the HUD after 15 s on a desktop and on a phone held sideways,
and the keys stay whole; the right of the top line (the verdict, the status words) stands under the keys and
the EQ line under it. The keys keep their ids (`nd-side-dac`, `nd-side-computer`), so the focus stays on
the key a keyboard flip chose across the redraw.

`computerView(core)` (`computer-view.js`) hands the drawing the core's `volumeOf` and `toneOf` on the
computer side only. The scene, the top line, the dash's transport and the fade do not flip; the rest
reads the computer's, in the DAC page's own parts, so an amp page flipped to the computer has the
volume keys, figure, mode, points and presets of a DAC page and its own power key and gauges still.

- **Volume.** The outrun figure reads `42` with `%`; muted keeps the number, dimmed; before the first
  read it is a dash and the keys are quiet, never a live 0 %. The dash's `−` `MUTE` `+` step and mute
  the computer (`actions.computer`), a held key repeats as the device's does.
- **EQ line.** `OFF`, or `EQ` with the preset's name, and no gains list; a state only the EQ's file
  says is marked `unconfirmed`.
- **Mountains.** The bubbles are the points, each on the curve the points sum to (a shelf's too); the
  lit point has the bell's diamonds, a drag of one sets its Q. A press grabs a bubble and moves its
  frequency and gain, the wheel and two fingers set the Q, and a tap on an empty spot adds a PK
  while the curve is up (the morph at one half or more); a first touch only shows the curve. The held
  bubble is on the curve all along a drag; a flip ends the drag and writes only its last value, done. With the
  EQ off the curve, the bubbles and the diamonds are drawn at 45 %: the points it would play, not playing. The
  computer's tone is worked out once per model change or held value (`tune-wire.js`), not per frame.
- **Sheet.** Mode (`Off | EQ`), Points (the table of six rows that scrolls, `N more below ↓`, a typed
  value in each field, the type as a choice, `×` labelled `Remove point N`, `+ Add point`, the line
  `7 of 16 points · preamp −6.0 dB`) and the shared presets with `Save as…`, rename and delete. The
  phone's rows are 44 px. A preset a connected device plays too is the core's question (`core.ask`,
  dressed by Night drive's frame tokens).
- Nothing below the switch says whose controls these are. For a guest every control is quiet and
  `aria-disabled`; the switch alone works.

## MCP

`get_host` reads the computer (volume, mute, the EQ and its points, the player). `set_host_volume`
takes a whole percent from 0 to 100; above 100 is refused as the cap. A rise of more than 6 dB on
the volume's cubic scale asks the client, counted from the lowest level this server saw, so a
staircase of small steps asks at the sum; a level not read yet counts as 1 %. `host_mute` unmuting
asks, muting does not. `set_host_eq` takes exactly one of a preset name or `off`; a built-in needs a
device with a tone control, a saved preset needs none, and the question a preset on a device too
asks is put to the client and repeated with `confirm`. The three writes are listed only while
`[mcp] writes` is on. The computer's cap is 100 %, not `max_volume_db`.

## MQTT

Home Assistant gets one device, `This computer`: a switch (a binary sensor with commands off) that
turns the computer EQ on with its kept points and off, and a sensor naming the applied preset
(`custom`, or nothing). No computer volume, mute or point is offered; HA's own media integrations do
volume better. A retained command is never acted on; ON with nothing kept (the first point flat, no
preset) logs that and stays off. With the computer not served nothing is announced and the retained
entities are cleared at the bridge's start.

## Packages and Docker

The deb and rpm recommend the PipeWire tools (`pipewire-bin`, `wireplumber`; `pipewire-utils` on
rpm) without depending on them, the AUR lists them as optional with what each is for, and the Nix
package wraps `svolderd` and `svolder` with the tools on their PATH. The image installs the client
tools; the opt-in `docker-compose.host-audio.yml` (which `make docker` writes into `.env` when a
PipeWire socket is there) mounts the host's runtime directory read-only, runs as the host's uid, and
points `PIPEWIRE_RUNTIME_DIR` at it. In a container the chain is the daemon's child (`pipewire -c`),
which ends with the daemon and puts the path back to unfiltered; at start the daemon applies it again
if the state says on. Outside Docker the chain stays a user unit, or a detached process, independent
of the daemon.

## The checks

- Rust: tests on fakes in `make check` for the controller, the session, the daemon's requests, MCP,
  MQTT and the TUI; the volume rule's tests are on the safety list.
- The page: `core-computer.mjs` (the core), a `computer_*.py` driver for each theme in both browsers
  on the fake daemon, and the contract and completeness checks, which list every computer target.
  The Night drive timing check has a computer-side case with every cell lit (4.5 ms in Chromium).
- `make host-e2e`: the real daemon and the real chain on a private PipeWire and WirePlumber with a
  null sink, never the session. It plays a tone, drags and retypes points at 20 updates a second,
  forces a sample-rate change (again after a live softening: never over unity), turns the EQ off,
  checks the volume and mute with `wpctl`, and that every process it started is gone (testing.md).
- `make demo-computer` and `make demo-computer-amp`: the page with a simulated computer, your audio
  untouched, for looking.

## Known limits

- **A sample-rate change leaves the sound up to about 2 s shifted, never over unity.** The chain
  is rebuilt with the coefficients it was applied with until the controller reads the rate (about a
  second) and sends every slot again. After a curve was softened by hand the level stays at the
  applied curve's preamp until the next apply (see [host-control-preamp.md](host-control-preamp.md)).
- **A dragged computer point dips the level by up to about 0.6 dB per update.** The level
  moves with the preamp as the curve's peak moves; the end-to-end check's net level over a drag was
  within −0.12 dB of the start and a 1 ms step at most 1.1 dB.
- **A model older than 2 s re-sorts a row on its next read.** The 2 s follow after a hold covers
  every observed case; a stale model costs a missed re-sort, never a wrong write (a point write
  carries its slot, and a slot that is gone is refused).
- **A jank during the Night drive's Full probe can keep a lower tier.** The ladder walks back up, the
  wet road coming back within about 3 minutes.
