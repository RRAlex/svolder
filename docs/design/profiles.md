<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Profiles

A profile is a device model written as data: which functions it has, what each is called and what
kind of control it is, how raw steps map to units a person reads, and which controls can make sound
louder. Every front end builds its screens from the profile, so a device is added by writing a TOML
file, not Rust. This page is how a profile is loaded, resolved and used, and the decisions that keep
it that way. The tone control and the presets built on it are in
[tone-control-and-presets.md](tone-control-and-presets.md); the words (profile, control, key,
readout, family) are in the [glossary](../glossary.md); the place of `svolder-profiles` among the
crates is in [the map](../architecture.md#the-crates).
The format key by key and the steps to add a device are in
[profiles/README.md](../../profiles/README.md).

## The file

A profile file (`profiles/*.toml`, parsed into `ProfileFile` in
`crates/svolder-profiles/src/schema.rs`) has `schema = 1`, a `name`, and optionally `display`,
`product_id`, `min_firmware`, `extends`, `family`, `firmware_subsystems`, `identify`, a list of
`[[control]]` tables and one `[eq]` or one `[builtin_settings]` table. Every table in the schema
denies unknown fields, so a misspelt key is an error, never a silently ignored setting.

`firmware_subsystems` lists the numbers `n` the session asks `210.2.<n>` for after each connect
(`Profile::firmware_subsystems`). An unset key inherits the parent's list and a list replaces it, as
`family` does; the built-in profiles name `[1, 2]` for the Yggdrasil, Gungnir 2 and Bifrost 3 and
`[1]` for the Mimir, Eitr 2 and Mesh Card, the devices the protocol says have subsystems. The loader
refuses a list holding 0, a repeat or more than four numbers (`check_firmware_subsystems`). The ask
itself is in the session ([daemon](daemon.md), "The session underneath").

`identify` says whether svolder offers the identify cue (function 200) for the device
(`Profile::identify`); unset inherits the parent's, and a root offers it. The Mesh Card profile sets
it `false`, as the app offers none there and svolder follows the app: the actor and a direct
session refuse the request in words (a usage error, link or not), and the TUI, the web page, Home
Assistant discovery and the MCP tool each leave the cue out, so no front end offers a button that
sends nothing. `svolder raw 200` still sends the frame, for an owner who wants to see.

The eight keys the MQTT bridge's own entities take are reserved: `build_control` refuses a control
keyed one of them, in any letter case, naming the key ([the bridge](mqtt-and-home-assistant.md)
says why). It also refuses a control whose function id is not of the `Normal` class
(`FunctionId::class`): the passphrase function (250 and below it) and the firmware-update family
(210 but `210.2.x`, the version query), with an error that names the profile, the key and the id.
A control is listed with its value to every viewer a front end serves, a guest on the web page
included, and a write to either function is refused anyway, so the loader is where the passphrase
and the update functions stop being something a user file can show. The web table and the MCP state
leave the passphrase function out as well, and the bridge exposes only `Normal` ids, so each
remote view holds on its own.

A `[[control]]` is one function:

- `id`: the function id as a list of one to three bytes (`[21, 2]`), `key` (its stable name),
  `label`, and `kind`: `select`, `toggle`, `level`, `readout`, `meter` or `fault`.
- `value_type` overrides the wire type a kind carries by default (`ControlKind::default_value_type`:
  toggles and faults are bool, meters `u16`, the rest `u8`).
- A `select` or a named `readout` has `values`, a table from raw number to name.
- A `level` has a `range` of raw steps, an optional `center` (for balance), a `unit`, and a
  `convert`: `{ table = "dac-attenuation-db" }` (an index into a table in `db_tables.rs`) or `{
  linear = [min, max] }`.
- A `meter` has either a `range` (with optional `floor` and `tick`) or a logarithmic `scale` (`zero`
  or `zero_from`, `full`, `class_a`), never both, and may name a `channel`.
- A `fault` has `advice`: what to do about it, shown by every front end.
- `only_while` ties a boolean readout to another boolean control, `auto_off` makes the session write
  false whenever the device reports the readout true.
- The four safety flags, described below.
- `reads` lists further function ids a status is read as the control from (below).
- `kind = "action"` is a command, not a control (below).

The `[eq]` table describes the tone control: the function family `id`, the band centres, the
frequency scale of each side of a centre, the gain and Q ranges. Its use is in
[tone-control-and-presets.md](tone-control-and-presets.md).

An `action` is a Command with empty data on its `id` plus a slot byte (`7.2` and slot 2 send
`7.2.2`): recall or save one of the device's own slots. It sits in `[[control]]` tables because it
is one function of the device, but the loader puts it in `Profile::actions`, never in
`Profile::controls`. Every front end lists, reads and sets controls, and an action is never read and
never set to a value, so a separate list makes that structural: no screen, table or command topic
that walks the controls can show it or reach it, and each front end that offers it does so by name
(`roles::recall_action`, `roles::save_action`, the actions keyed `recall` and `save`). It has
`slots` (1 to 255), `overwrites` (a front end asks before sending it) and `changes` (the keys of the
controls whose readings the device sets afterwards). It takes no `value_type`, `range`, `values`,
`convert`, `center`, `unit`, meter field, safety flag or `reads`, and `slots`, `overwrites` and
`changes` take no other kind. `Action::command(slot)` builds the frame.

`reads = [[6, 1]]` makes a status on `6.1`, or on any id under it (`6.1.3`), read as that control.
The Loki Max writes a band as `6.<band>.3` from its slider and as `6.<band>` from its built-in
settings and reports either, so each band reads its two-id form. `Profile::read_as(id)` is the one
function that decides: the control at `id` itself, else the control whose `reads` entry `id` equals
or lies under, else `id` unchanged.

`[builtin_settings]` is a device's table of named settings: `keys` (the level controls it sets),
`ids` (the id each is written to) and `settings`, each a `name` and one raw step per key. It is a
profile's one preset list, so a profile has it or an `[eq]`; a child's table replaces its parent's
whole. The tone control's presets are the configuration's, and these are the device's, as profile
data; how each front end offers them, and what asks, is in
[tone-control-and-presets.md](tone-control-and-presets.md#device-presets-the-loki-max).

## Loading and resolving

`Registry` (`crates/svolder-profiles/src/registry.rs`) is every profile a front end can use.
`Registry::builtin` resolves the files compiled in (`BUILTIN` in `builtin.rs`, one `profile!` line
per file, by convention after its parents); `Registry::with_user_dir` adds every `*.toml` in a
directory, which the daemon takes as `profiles/` beside the configuration file. `from_sources`
parses each `(name, text)`, in order (the order sets which source wins a tie), and resolves them:

- A later source with the same name merges over the earlier one (`merge_defs`): its set fields win,
  its controls replace those of the same key and append otherwise, its `[eq]` and
  `[builtin_settings]` replace. A user file therefore overrides a built-in profile by name, or adds
  a new device.
- `extends` names a parent (`resolve`, at most `MAX_EXTENDS_DEPTH` deep; a loop is
  `ProfileError::ExtendsCycle`). The child starts from the resolved parent and `merge` lays its
  controls over it, again by key, so a child changes one control without restating the rest. A
  control's function id must stay unique across the profile. A key names one thing: a child's
  entry replaces the parent's of that key whether the parent had it as a control or as an action,
  and the unique-id rule runs over both lists.
- Only a concrete profile (one with a `product_id`) is checked for `only_while` and `zero_from`
  targets, for `reads`, for the names an action's `changes` lists and for `[builtin_settings]`,
  because a base may name a key only its children define.

`Registry::select(product_id, firmware)` picks the profile for an advertisement: among the profiles
for that product id whose `min_firmware` is not above the device's firmware, the highest
`min_firmware`; ties go to the later source, so a user file beats a built-in, then to the later name
so the answer never depends on iteration order. This is how one product with two firmware
generations gets two files. `Registry::product_name` is the one place a product id becomes a name,
and `Registry::generic` is the profile with no controls an unknown product gets, so a new device
still connects and shows its raw functions.

A device's entry can also name its inputs (`[device.inputs]`); `apply_input_labels`
(`crates/svolder-profiles/src/codec.rs`) lays those names over the profile's `input` control,
replacing the label of a value it lists and adding one it does not.

## What a profile is used for

**Reading.** `describe` (`codec.rs`) turns a status message into a `Reading`: the key, label, raw
value, a display string and the converted number and unit. A function the profile does not name is
still described, by its id, with the label `?`. A `readout` or `fault` appears everywhere with no
code written for it.

**Writing.** `intent` (`codec.rs`) builds the message that sets `key` from forgiving text (`-30dB`,
`min`, `usb`, `on`), and `relative` builds a nudge from the current raw value. The device's own
words never reach the wire without passing the profile's range and value checks.

**Values.** `units.rs` is the one place a level's raw step becomes a person's number and back
(`amount`, `text`, `bounds`, `raw_for`). The TUI, the CLI, MQTT and MCP all read a level through it,
built from the raw value and never from a converted value a transport carried, so silence reads the
same through the daemon as on a direct session.

**Meters.** `Profile::placed` (`meter.rs`) settles a meter's scale against the readings held, and
every front end reads a meter through the `Placed` it returns. A log scale has a zero (a number, or
the key of a readout that reports the amp's floor), a full scale, and a class-A block over the start
of the bar.

**The volume.** `roles::is_volume` (`roles.rs`) says which control is the output volume: the level
keyed `volume`. The cap, the glide and the TUI's pacing all hang on that key, which is why every
profile calls its volume `volume`.

## Safety data

What may make sound louder is declared by the profile, not guessed by a front end. Four flags exist
(`crates/svolder-profiles/src/model.rs`), and `safety::lets_sound_through`
(`crates/svolder-profiles/src/safety.rs`) classifies a change by them:

- `output_gain` on a select whose values, low to high, set how loud the output is: a change to a
  higher one is `Through::GainUp`, and one with either side unknown counts as a rise.
- `mutes_output` on a toggle: anything but on lets sound through (`Through::Unmute`).
- `powers_output` on a toggle: anything but off does (`Through::PowerOn`).
- `asks_on_change` on a select or toggle whose effect no order describes: any change asks.

The classification is a fact about the control; whether to ask, and the words, belong to the front
end. The writer reads the same flags to know when a device reads itself muted or off (`silent`,
`silences` in `bins/svolderd/src/writer.rs`). The volume's own rule (cap, question, glide) is in
[the map](../architecture.md#where-the-volume-safety-rule-lives-and-why).

## Principles

**A device is data.** Keep a device's behaviour in its file. A special case in Rust for one model is
a case every front end must learn separately, and the next device will want another.

**Unknown is shown, not hidden.** An unnamed function is described by its id and a generic profile
still connects. A front end that dropped what it cannot name would hide a device's behaviour from
the person trying to understand it.

**A profile is checked when it loads, not when it is used.** Unknown fields, duplicate ids, cycles,
a meter with both scales, a bad `only_while` fail at load with the profile's name, and so do these:
two controls that read one id, a `reads` entry on a restricted function, an action whose commands
overlap a control, an action whose `changes` name the volume or a control that asks before it
changes, a built-in setting that names a control that is not a level, the volume, or an id that does
not read as its control, a profile that has both an `[eq]` and built-in settings, and an action
keyed `save` without `overwrites = true`, which would send with no question. A mistake in a
user file must stop the reload, not corrupt a running device, and the daemon keeps the devices as
they were when a reload fails.

**Inheritance is by key.** A child replaces a control by key and never edits its parent. Shared
files (`dac-common`, `mesh-eq-5`, `amp-common`) stay valid for every child, and a bug fixed in a
parent reaches them all.

**Safety flags are declared, never inferred.** A new gain or power control must say how it lets
sound through. A front end that guessed from a label would be wrong on the one device where it
matters; one flag in the file is read the same by the TUI, the web page, MQTT and MCP.

**One reader per number.** Conversion, wording and bounds live in `units.rs` and `convert.rs` and
nowhere else. Two readers of the same step drift by a rounding or a unit.

**A user file wins.** Selection and merging prefer the later source on purpose: a person with a
device the built-ins get wrong can fix it in their own directory without a rebuild.

## Where the code and tests live

- Schema, model, registry: `crates/svolder-profiles/src/schema.rs`, `model.rs`, `registry.rs`,
  `builtin.rs`; tests in `crates/svolder-profiles/src/registry/tests.rs` and
  `crates/svolder-profiles/tests/builtin.rs` (every built-in loads and covers its products).
- Conversions and readings: `convert.rs`, `units.rs`, `db_tables.rs`, `meter.rs` and `codec.rs`
  (statuses to named readings, a user's words to commands); tests in the `tests.rs` beside each.
- Device rules and safety: `roles.rs`, `safety.rs`; tests in `roles/tests.rs`, `safety/tests.rs` and
  the list in `scripts/safety-tests.list`.
- The profiles themselves: `profiles/*.toml`; the product names the advertisement decodes to are in
  `crates/svolder-proto/src/advert.rs` (`product_name`).
