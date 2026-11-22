<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Scenes

A scene is a named snapshot of the sound settings of the whole config: the computer's EQ, loudness,
crossfeed and virtual speakers, and the tone control and plain sound settings of every device that
was connected when it was saved. Saving one, recalling one, renaming one and deleting one are the
four things a front end asks of the daemon, and every front end (the terminal UI, the web page, the
command line, the MCP server and Home Assistant) asks the same daemon the same way. This page is
what a scene holds and why, how a recall keeps every side safe, how "which scene is on" is judged,
where scenes are stored, and the contract the front ends share. What a user sees and does is in
the usage pages ([tui.md](../usage/tui.md), [web-ui.md](../usage/web-ui.md),
[cli.md](../usage/cli.md), [mcp.md](../usage/mcp.md), [mqtt.md](../usage/mqtt.md),
[config.md](../usage/config.md)); the words are in [the glossary](../glossary.md).

## What a scene is not

- Not a **preset** (a named tone curve, `[[preset]]`; see
  [tone-control-and-presets.md](tone-control-and-presets.md)). A scene may hold a preset's values
  and may share its name.
- Not a device **profile** (a model's TOML; see [profiles.md](profiles.md)).
- Not a **device slot** (Loki Max's Preset 1 to 3, which live on the device).
- Not Home Assistant's own scene: HA's is written "a Home Assistant scene" in words.
- Not Night drive's `scene` (the canvases of that web theme, local to its JavaScript).

In Rust, `scene` and `scenes` mean only this feature; the TUI's key-hint context is `KeyContext`.

## What a scene holds

A scene is a snapshot of values, not references. A preset renamed, edited or deleted later never
changes a scene; the preset's name rides along as a label so the screens can show it again after a
recall. Scenes are global: one list for the whole config, and each part belongs to the computer or
to a device by its unit id. A device is listed under its configured name of now, so renaming it
costs nothing.

**The computer part** (one per scene; `ComputerPart` in `crates/svolder-host/src/eq/scene.rs`): the
EQ on or off with its curve as the controller keeps it (name, origin, the stated preamp and the
filters), loudness (on, strength, reference), crossfeed (on, with its model, preset and numbers) and
virtual speakers (on, with the set). The chain's slots, route, manager and target are not sound
and stay the chain's. The stated preamp is the one the curve asks for, saved even while the level is
held down for a playing stream. The EQ's own word `custom` names a hand-made curve (lower case, the
`CUSTOM` constant); anything a user reads shows it as `Custom`.

**A device part** (one per device connected at save, keyed by unit id; `DevicePart` in
`crates/svolder-profiles/src/scene.rs`):

- Its tone control as a preset snapshot in Hz, dB and Q (mode, bands, loudness strength), named
  after the preset that was on, else `Custom`. Capture takes the fitting presets to find that name.
- For a device with built-in settings and no EQ block of its own (Loki Max), its bands as raw steps.
  A part holds a tone or raw bands, never both (refused when the config loads).
- Its plain sound settings: every `select`, `toggle` or `level` control of the profile that is not
  the volume role, an output gain, a power or mute switch, a control that asks on change, an
  auto-off, nor a readout, meter, fault or the amps' `power_meters` switch (reporting, not sound).
  On a Bifrost 3 that is phase, NOS, balance and input (profile order); on a Loki Max, the input
  (its Mode asks on change, so it is out). `held_controls` in the same file is the one rule; no
  front end lists keys itself.

**Never in a scene** (no volume, gain select, mute or power on any side, and the computer's sink
volume): a scene never writes one, and a test pins that on every surface. That is not a promise
that nothing gets louder: a held input, a phono load or a Lyr 5 mode can change the level, so the
words everywhere say "no volume, gain, mute or power is saved", never "never louder".

Not held either: the output device (the daemon puts the chain in front of the default sink, so the
computer part plays on whatever output is current), and anything of the web, TUI, MQTT or MCP
configuration.

## Names

The preset rule (trimmed, 40 characters at most, case-blind `same_name`) in a namespace of its own:
a scene may share a preset's name, built-ins included. `custom` is reserved in any case (it is the
Home Assistant select's "none" option): save and rename refuse it as a bad name.

## Where scenes are stored

As `[[scene]]` blocks in `config.toml`, beside `[[preset]]`, written by the daemon through
`Config::{add,update,rename,remove}_scene` under the file's lock with the comments and other blocks
kept, and through `edit_then_reload`, so the snapshot, the MQTT client's select options and every
front end follow the reload. The shape is `[[scene]]` with `name`, then `[scene.computer]` (the EQ's
`filter` array as a preset's inline array, `loudness` and `crossfeed` inline tables, `speakers`),
and `[[scene.device]]` with `udid`, `name`, `controls` and `raw_bands` as `{ key, step }` lists and
`[scene.device.tone]` with the preset's `mode`, `loudness` and `band` array. A key repeated in one
part, or a tone with raw bands, is refused at load. A scene is the user's own library, not something
the machine learned, so it is not in the state home.

The one thing the state home keeps is which scene was recalled last: `scenes.toml` there holds
`recalled = "<name>"`, so the tie rule below holds across a restart. A missing, unreadable or
garbled file is no memory and never a start failure. A rename moves the memory and a delete forgets
it.

## Without a daemon

Scenes span several devices and the computer, and a daemon-less command-line run holds one link, so
they need the daemon. `svolder scene list` and `show` read the file and say so; every other verb,
and the TUI's box, say that scenes are kept by the daemon and how to start it.

## Save

A save captures every connected device and the computer's part, writes one block through
`edit_then_reload` and answers with a `left` part for each configured device that is not connected
("Kara is not connected: it is not in the scene"). It is refused in words while a device part
cannot be read whole (`Bifrost 3 is still reading its bands`, the preset list's own rule) or while
a device's writer runs or waits a recall or setting (reason `busy`), and with `nothing_to_save`
when no device is connected and the computer's controls are not there. It takes what plays: the
computer's stated values, never a drag's unsent one. A taken name is refused unless forced, which
saves over that scene.

## Recall

Every side changes at once, each by its own existing safe order; a scene adds no ordering rule.

**The computer** changes through `EqControl::recall` (`eq/control/scene.rs`) only, as one change:
every stage's settings are set together and then there is at most one apply, or one live line.

- One apply when the chain must be built or rebuilt: no chain yet, live updates off, another virtual
  speakers set, a stage the chain lacks, or a config that holds a stage going off. Otherwise one
  live line carries the slots, the preamp and the stages. A second apply happens only when a stage's
  live send fails after an apply that did not hold the stage; that failure itself goes to one apply
  with the stage in its config.
- The preamp rules apply unchanged. A curve needing more preamp than the running chain's cap goes
  live at the cap while a stream plays, says `LEVEL_HELD_NOTE` and is lifted after the idle time
  ([host-control-preamp.md](host-control-preamp.md)); a crossfeed's cost is in the preamp before the
  stage plays; loudness is cut-only. A crossfeed or speakers-only scene (the EQ off) recalled over
  a hotter chain while a stream plays keeps the held level until the next apply, as turning the EQ
  off does.
- Crossfeed and virtual speakers on together are refused. A part the controller cannot take (a
  speakers set that is not fetched, a legacy chain) is refused whole, in words, before anything
  changes. "No filter left hanging" holds: a recall that turns everything off removes the chain, and
  there is never a second one.
- `Recalled.notes` carries `LEVEL_HELD_NOTE`, a stage it turned on that did not stay on (`turned
  off: ...`) and a kept speakers set that is no longer fetched (the choice was left as it was).
- Changes older than scenes came with the work: a live line that changes points and a stage that
  costs less now puts the stage first and, when the node may hold the loaded config's slots after an
  unseen rate change, holds the preamp for both orders; a stage the node plays and the controller
  has no record of is taken to the bypass in a line of its own first
  ([host-control-preamp.md](host-control-preamp.md)).

**Each device** goes through its writer, in arrival order: the tone as a recall batch of one preset
(the snapshot is the one fitting preset: gains down, shape, gains up, mode last), a Loki's raw
bands by `Writer::apply_values`, then the held controls in profile order as ordered writes through
`set_if_needed`, which leaves one unsent only when the device reads the step and nothing is in the
line for it (a toggle's boolean reads as step 0 or 1). A part is handed to the writer whole, never
cut to what differs, so a write already in the line cannot land after the recall and leave the
device on another state with the part answered done. A tone named `Custom` clears the writer's
recall memory (no `Recalled` event). A device's own intermediate tone states are bounded by its
old and new curves, as in any preset recall.

**The both-sides rule.** `preset::both_sides` is judged on the state the recall leaves: if one
non-neutral preset would play on a device and on the computer at once, the daemon answers `Ask`
and changes nothing until the same request comes again with `confirm`, which recalls the scene as
it was saved (no flattening: the user saved that state, and the question says so). With no
computer controls there is nothing to play twice and no ask.

**The answer** lists each part's outcome: `done`, `partial` (some settings left out, the notes name
them: a key the profile no longer has, a select value it does not list, a level out of range, a tone
with another band count, a computer note other than held-level or chain-rebuilt), `left` (a device
not connected, or a part refused before anything changed) or `failed` (started and could not
finish: a device's writer batch failed or was replaced, a computer recall that failed after it began
to change things). Nothing is rolled back; a partial scene reads "not on". The writer's clamp words
and the computer's notes are passed on in the daemon's words.

**An edit in progress.** There is no lock: arrival order, as a preset recall today. A scene recall
behind a running recall waits its turn; a computer drag from another front end finds the new curve
or is refused when no point holds its slot, as after a preset load.

## Which scene is on

`svolder_ipc::scenes::on` is the one pure function, over the scenes and what the settings read now.
A scene matches when every part that counts agrees: a device's tone (`matches`) and each held
control's raw step for a device that is present, and the computer part, compared on what is heard
(the EQ's points and its stated preamp while it is on, each stage's settings only while it is on;
`ComputerPart::matches` against `EqView`, which carries `stated_preamp_db` for it). A part of a
device that is not connected is not counted, and a scene with no part that counts is never on.

The scenes are ranked: the recalled one first, then the file's order. The first that matches is on.
`known` is false only while an unread scene ranks above the first match, or while nothing matches
and one is unread; then `on` is absent and a front end shows "reading..." and never "none".
Crossfeed and the speakers never stay on across a restart, so a scene with either on reads "no
scene" after a daemon restart until recalled.

## The daemon

`bins/svolderd/src/scenes.rs` and `scenes/` own a `watch` of the `ScenesView`, recomputed when a
device's feed changes, the host view changes (the view follows the host's change watch, not one
computer, which a host reload would orphan) or the config or device set changes, and sent only when
it differs. A recall issues the computer and every device at once. The computer is reached through
`Computer::eq_op` and a door in `bins/svolderd/src/host.rs`.

## The contract

**Socket** (`crates/svolder-ipc/src/socket/protocol/scenes.rs`). Feature `scenes`
(`FEATURE_SCENES`): a front end offers scenes only while its daemon lists it, and a daemon without
it gets no scene request. `Request::Scene { op }` with `op` one of `save {name, force}`,
`recall {name, confirm}`, `rename {from, to}`, `delete {name}`. The answer is `Response::Scene
{ name, outcome }` (the name as the file spells it; one `PartOutcome { who, status, notes }` per
part for a recall and for a save's `left` parts, else empty). A refusal is `Response::Error` with a
`Reason`: `taken`, `bad_name`, `missing`, `read_only`, `file_busy`, `invalid`, `stopping`,
`internal`, `busy`, `nothing_to_save`. The both-sides question is `Response::Ask { message, name,
devices }`.

`Event::Scenes(ScenesView)` goes on every `Subscribe` stream whatever its device filter, once after
the snapshot and then each time the view differs; an older peer reads it as `Unknown`.
`ScenesView { scenes: [{ name, parts: [{ who, present }] }], on, recalled, known }`: `who` is
`computer` or the device's configured name now.

**Web** (writer only: a guest and a read-only page have no row, no `scenes` in the page model and no
`scenes` event; the routes answer 401 or 403 like every write): `GET /api/v1/scenes`; `POST
/api/v1/scenes/save`, `/recall`, `/rename`, `/delete`. A recall's both-sides question is a 409 with
`ask`; a refusal is 422 for `bad_name` and `taken`, else 409, with `error` (words) and `reason`
(what a page acts on, never the words). The `scenes` event is sent like `fleet`, with no `needs`
word, so the events URL is unchanged. The popup is `core/popups/scenes/`, with `core/scenes.js` (the
view and its words) and `core/scenes-actions.js` (the four routes); the store keeps `scenes` from
the page model and the event (`stream.js`); see [web-ui.md](web-ui.md).

**Home Assistant** (per computer; [mqtt-and-home-assistant.md](mqtt-and-home-assistant.md)): a
`select` `scene` (the scene names plus `custom`; the state is the scene that is on, else `custom`,
and unchanged while the daemon has not read which is on) and a `text` `scene_save` on the computer's
own device. HA recalls and saves only: rename and delete are library upkeep with no HA idiom, and a
stray automation could delete. A both-sides recall is refused (logged) because HA cannot answer.

**MCP** ([mcp-server.md](mcp-server.md)): five tools, `list_scenes` and, under `[mcp] writes`,
`load_scene`, `save_scene`, `rename_scene`, `delete_scene`. A plain recall asks nothing.

## Rules every surface keeps

- The daemon owns scenes. A front end sends `Request::Scene` and reads `ScenesView`, and judges
  nothing itself: which controls a scene holds is `scene::held_controls` and nowhere else, which
  scene is on is `scenes::on` and nowhere else.
- Refusals and notes are the daemon's own words, with no front-end rewording. A scene that is gone
  elsewhere answers `missing` and the list is read again.
- A scene never writes a volume, gain select, mute or power.
- The computer part changes only through `EqControl::recall`; the device parts through each
  device's writer; no new path to the actor.
- A click on a scene row only focuses it in the TUI, and a recall (which changes the sound) needs a
  second click on the focused row or `Enter`; the presets box differs and recalls on one click.

## Proof

Each layer has its own tests, red first: the profiles' tables over every shipped profile, the
config's round trips and writers, the controller's counts of applies and live lines under a fake
chain, the daemon's races (a no-wait write whose echo is late, a toggle already there, a hand-made
part naming the volume) and the writer lab's scenes variant over a fake device. The preamp lab has
a scene profile that mixes recalls with the faults of `event_for` and checks the unity rule after
every event. Host end to end has phase SC: scenes saved through the socket and recalled on a live
chain under a pulse player, with the chain's node and process staying, no sink removed or added, the
1 kHz tone never above the EQ-only reference and the view's `on` naming the scene until one point
moves ([host-e2e.md](host-e2e.md)).
