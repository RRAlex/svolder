<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Profiles

A profile is a device model written as data: which functions the device has, what each is called,
what kind of control it is, how raw steps become units, and which controls can make sound louder.
Every front end builds its screens from the profile, so a new device is a TOML file. This folder
holds the profiles compiled into the binaries; a person can put the same files in
`~/.config/svolder/profiles/` to add a device or to override a built-in one by `name`, with no
rebuild.

[docs/design/profiles.md](../docs/design/profiles.md) says why profiles work this way and how they
load and resolve; this page is the format and the steps to add a device. The words (profile,
control, key, readout, family) are in the [glossary](../docs/glossary.md).

## Reading the folder

A file with no `product_id` is a base that only holds what its children share: the `*-common`
files (`dac-common`, `preamp-common`, `amp-common`, `phono-common`), the `mesh-eq-*` files and the
`amp-standby` and `amp-stereo` files. A file with a `product_id` is a device.
A device usually extends a base:

    dac-common -> mesh-eq-5 -> bifrost3

`profiles/bifrost3.toml` is a short, complete example: it names itself, extends `mesh-eq-5` and adds
the three controls only it has. `svolder profiles list` names every profile, and `svolder profiles
show <name>` prints one fully resolved.

## The file

A profile file is parsed into `ProfileFile` in `crates/svolder-profiles/src/schema.rs`. Every table
in the schema denies unknown fields, so a misspelt key fails the load with the profile's name.

### Top level

| Key | Meaning |
| --- | --- |
| `schema` | Required. The format version; the only one is `1`. |
| `name` | Required. The profile's identity, and what `extends` and a user file's override refer to. A file's `name` is what counts, not its file name. |
| `display` | The device's name for people. A device (a profile with a `product_id`) needs one that differs from its `name`. |
| `product_id` | The number the device advertises. Setting it makes the profile concrete: one a device can select. |
| `min_firmware` | The lowest firmware build this file describes; `0` when it covers every build. |
| `extends` | The name of a parent profile, at most eight deep. |
| `family` | `dac`, `amp` or `preamp`. Unset inherits the parent's, and a root defaults to `dac`. The TUI picks its accent from it. |
| `firmware_subsystems` | The firmware subsystems svolder asks for their version after each connect, as a list of numbers, such as `[1, 2]` (main and DSP). Unset inherits the parent's, a list replaces it, and none means nothing extra is sent. Each number is at least 1, listed once, and at most four are allowed. The answers (`210.2.<n>`) show as the device's firmware versions. |
| `identify` | `true` or `false`: whether svolder offers the device's identify cue (function 200). Unset inherits the parent's, and a root offers it. `false` where the app offers none (the Mesh Card): the daemon refuses the request in words, the TUI dims its key, the web has no button and Home Assistant no entity; `svolder raw 200` can still send it. |
| `[[control]]` | One table per function (below). |
| `[eq]` | The tone control, for a device that has one. |
| `[builtin_settings]` | The device's own named settings, for a device that has them and no `[eq]` (below). |

### `[[control]]`

| Key | Meaning |
| --- | --- |
| `id` | Required. The function id as a list of one to three bytes: `[10]`, `[21, 2]`. Unique across the resolved profile. Function 250 (the passphrase) and the firmware-update family `210` (but `210.2.x`, the version readout) cannot be a control: the profile fails to load, naming the id. |
| `key` | Required. The control's stable name, used on the command line, in MQTT and by the web page. Case does not matter when keys are compared. Eight keys are the bridge's own and fail the load, naming the key: `link`, `signal`, `firmware`, `dsp_firmware`, `identify`, `eq_mode`, `eq_preset` and `eq_loudness`. |
| `label` | Required. What a person reads. |
| `kind` | Required. `select`, `toggle`, `level`, `readout`, `meter`, `fault` or `action`. |
| `value_type` | Overrides the wire type a kind carries by default: `u8`, `u16`, `u32`, `i8`, `i16`, `i32`, `bool` or `data`. A toggle or fault is `bool`, a meter `u16`, the rest `u8`. |
| `values` | A `select`'s choices, or a named `readout`'s: a table from the raw number (0 to 255, written as a TOML key) to its name. |
| `range` | A `level`'s raw steps, `[low, high]`, inside the `value_type`'s domain. A level needs one. A linear `meter` may have one. |
| `center` | A `level`'s resting step, inside `range` (a balance). |
| `unit` | The unit a converted level shows, such as `dB`. |
| `convert` | A level's raw-to-unit conversion: `{ table = "dac-attenuation-db" }` or `{ linear = [min, max] }`. |
| `channel` | A meter's channel name, such as `L` or `R`. |
| `floor`, `tick` | A linear meter's idle reading and the reading where class-A operation starts. |
| `scale` | A power amp's logarithmic meter, in place of `range`, `floor` and `tick`: `{ zero = 160, full = 350, class_a = 0.25 }`, or `zero_from = "<key>"` for a zero that follows a `u16` readout's reading. |
| `advice` | A `fault`'s required text: what to do about it. Every front end shows it. |
| `only_while` | A boolean readout that reads true only while the named boolean control also does. |
| `auto_off` | A boolean readout the session writes false whenever the device reports it true. |
| `reads` | Further ids a status is read as this control from, a list of one to three bytes each: `reads = [[6, 1]]`. A status on an id that equals an entry or lies under it reads as this control. An entry may not meet another control's `id` or `reads`, an action's `id`, the `[eq]` family or a restricted function; the control's own `id` may lie under its own entry. |
| `slots` | An `action`'s slot count, 1 to 255 (required). |
| `overwrites` | An `action` that overwrites what the device holds, so a front end asks before it. |
| `changes` | An `action`'s list of control keys whose readings the device sets after it. Each must be a control of the resolved profile, and not the volume or a control with `asks_on_change`. |

Each `kind` is a different shape of function:

- `select` picks one of several named values.
- `toggle` is on or off.
- `level` is a number in a range, optionally converted to a unit.
- `readout` is a value the device reports and a person cannot set.
- `meter` is a live reading, drawn as a bar.
- `fault` is a flag the device raises, with its advice.
- `action` is a command with empty data on its `id` plus a slot (`7.2` and slot 2 send `7.2.2`).
  It is not a control: the loader keeps it in a list of its own, so it has no reading and is never
  set to a value, and every front end that offers it does so by name (`recall` and `save`). It
  takes `slots`, `overwrites` and `changes`, and no `value_type`, `range`, `values`, `convert`,
  `center`, `unit`, meter field, safety flag or `reads`. Its `id` is one or two bytes, outside
  the passphrase and firmware families, and its commands may not meet any control's `id` or
  `reads`. The action keyed `save` must set `overwrites = true`, so a front end always asks before
  it sends. A child's entry of the same `key` replaces the parent's, control or action.

### Safety flags

Four flags say how a control can make sound louder. A front end reads the flag; it never guesses
from a label. A new control that raises the output and carries no flag is a defect.

| Flag | On | Meaning |
| --- | --- | --- |
| `output_gain` | `select` | The values, low to high, set how loud the output is. A change to a higher one is a rise. |
| `mutes_output` | `toggle` | True silences the output, so setting it false lets sound through. |
| `powers_output` | `toggle` | True powers the device on and lets sound through, so setting it true is as much a rise as an unmute. |
| `asks_on_change` | `select` or `toggle` | A change raises or may raise the output in a way no order describes, so it asks first. |

The volume's own rules (the cap, the question, the glide) hang on the level whose key is `volume`,
so a device's output volume is called `volume`.

### `[eq]`

| Key | Meaning |
| --- | --- |
| `id` | The function id the bands live under. |
| `bands` | The centre frequency of each band, in hertz, in band order. |
| `fscale` | One `[below_centre, above_centre]` pair per band; a count that differs from `bands` fails the load. |
| `range` | The frequency range factor all bands share. |
| `gain` | `{ range = [low, high], zero = <raw> }`: the gain range in dB and the raw value that means 0 dB. |
| `q` | `{ range = [low, high], center = <q> }`: the Q range, starting above 0, and the Q at raw value 64. |
| `mode_sub`, `loudness_sub` | The second byte of the function ids of the tone mode and the loudness strength, under `id`; they default to `50` and `52`. |

A three-band device and a five-band one differ here and nowhere else (`mesh-eq-3` and
`mesh-eq-5`). The tone control is in
[docs/design/tone-control-and-presets.md](../docs/design/tone-control-and-presets.md).

### `[builtin_settings]`

The device's named settings, written to controls as raw steps. The Loki Max has one:

    [builtin_settings]
    keys = ["band1", "band2", "band3", "band4", "band5", "band6"]
    ids = [[6, 1], [6, 2], [6, 3], [6, 4], [6, 5], [6, 6]]
    settings = [
      { name = "Flat",       raw = [15, 15, 15, 15, 15, 15] },
      { name = "Bass Boost", raw = [28, 21, 17, 15, 15, 15] },
    ]

| Key | Meaning |
| --- | --- |
| `keys` | The controls a setting sets: level controls only, each once, and never the `volume`: a built-in writes outside the volume's rules, so the profile fails to load if one names it. |
| `ids` | The id each is written to, in the order of `keys`; each must read as its control (the Loki Max writes `6.<band>` and its control's id is `6.<band>.3`, which `reads = [[6, <band>]]` joins). |
| `settings` | A list of `{ name, raw }`: a name that is not blank and not another's (case aside), and one raw step per key, inside that control's `range`. At least one. |

A profile has these or an `[eq]`, not both. Every refusal names the profile and the reason.

### `extends`

A child starts from the resolved parent and lays its own fields over it. A control whose `key`
matches one of the parent's replaces it whole, control or action; any other is appended; an `[eq]`
or `[builtin_settings]` replaces the parent's. A child therefore changes one control without
restating the rest, and never edits the parent. A `product_id`, `min_firmware`, `display`,
`family`, `firmware_subsystems` and `identify` set in the child win.

### Firmware

A device advertises its firmware build. Among the concrete profiles for its `product_id` whose
`min_firmware` is not above that build, the one with the highest `min_firmware` is used. One product
with two generations of firmware is two files: `gungnir2-v1` (`min_firmware = 0`) and `gungnir2-v2`
(a higher one). A product no profile covers still connects as a generic profile with no controls,
so its raw functions show.

## Add a device

Work from a profile that is already close: `extends` a base and write only what differs.

1. **Write the profile.** Create `profiles/<name>.toml` with the SPDX comment block, `schema = 1`,
   `name`, `display`, `product_id`, `min_firmware` and `extends`, then the controls the device has
   that the base does not. Give every control that can raise the output its safety flag. A device
   that needs two firmware generations gets two files, each with its own `min_firmware`.
2. **Add its line to the built-in list.** In `crates/svolder-profiles/src/builtin.rs`, add
   `profile!("<name>"),` to `BUILTIN`, by convention after its parents. A parent is found by name,
   so the order does not decide whether a profile resolves; it decides which of two sources with the
   same `name` wins, and which of two profiles for one product id and firmware `select` prefers.
3. **Name the product.** `product_name` in `crates/svolder-proto/src/advert.rs` is the one table of
   product names an advertisement is decoded with; add `<product_id> => "<Name>"` to it, and the
   same pair to the `names_every_known_product` test beside it. The advertisement's own method of
   that name, the BLE crate's wrapper and `Registry::product_name` in
   `crates/svolder-profiles/src/registry.rs` all end in this table, so no other function needs
   touching. A product the table does not know is still named: `Registry::product_name` falls back
   to the `display` of the profile for that id, which is how a user's profile names a device with
   no Rust change. A device svolder ships belongs in the table.
4. **Tell the tests.** In `crates/svolder-profiles/tests/builtin.rs`, add the profile's exact
   control set to `EXPECTED`, and the product id to the list in
   `every_released_product_has_a_profile`.
   `concrete_profiles_are_well_formed` and `all_builtin_profiles_resolve` check the rest on their
   own. A test for a control that is the device's own, such as a power state or a load setting,
   goes beside the others in that file.
5. **List the product in the protocol's product table**, in [docs/protocol.md](../docs/protocol.md),
   in the same commit.
6. **Run the tests**:

       cargo test -p svolder-profiles
       cargo test -p svolder-proto advert
       make check

   `make check` also runs the safety list; a control with a gain, mute or power flag needs a test
   in `crates/svolder-profiles/src/safety/tests.rs` that shows the flag does what it says.

To try a profile before it is built in, copy it to a throwaway configuration directory's `profiles/`
folder and run `svolder profiles show <name>` with `XDG_CONFIG_HOME` pointing there. A mistake
stops the load and names the profile, the control and the reason.
