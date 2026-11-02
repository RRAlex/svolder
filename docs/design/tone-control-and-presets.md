<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The tone control, presets and AutoEq import

Some devices carry a small parametric equaliser, the tone control: a few peaking bands, each with a
frequency, a gain and a Q, and a mode (flat, EQ or loudness). This page is how svolder models that
control, how it names and recalls curves (presets), how it computes what a curve does, and how a
headphone correction from AutoEq becomes a preset the device can hold. The profile that declares the
control is in [profiles.md](profiles.md), and the place of the crates in [the
map](../architecture.md#the-crates); the computer's own EQ, a separate thing, is in
[host-audio.md](host-audio.md); what each command and key does for a user is in the [EQ
reference](../usage/eq.md).

## The model

`EqSpec` (`crates/svolder-profiles/src/model.rs`) is the profile's `[eq]` table resolved: the first
byte of every tone-control function id (`id`), the mode and loudness sub-ids, the band centres, a
frequency scale for each side of a centre, the gain and Q ranges, and the raw gain that means 0 dB.
Every parameter is one raw step from 0 to 127 (`EqSpec::STEP_MAX`), and the conversion between steps
and Hz, dB and Q is in `crates/svolder-profiles/src/convert.rs`: a band's frequency is centred on
step 64 and moves on its own scale either side, so each band has a window it cannot leave.
`band_id`, `mode_id` and `loudness_id` build the function ids; `EqMode` is flat, EQ or loudness;
`EqParam` is frequency, gain or Q. Conversion is always to the nearest step and clamped, so a write
is exactly what the device will hold. `band_text` (`units.rs`), built on `frequency_text`,
`gain_text` and `q_text` (`convert.rs`), is the one wording of a band value that every front end
shows.

A boost can clip at a high enough volume, so the code computes how far a curve boosts
(`tone/response.rs`, below) and `clipping_peak` says, from a device's own readings, whether that can
clip. Every front end asks that one function, so the TUI's red zone, the CLI's `peak` line and the
web page agree.

## The response curve

`response_db` (`crates/svolder-profiles/src/tone/response.rs`) sums one peaking biquad per band, as
the device's own DSP runs it (sampled at 48 kHz), over log-spaced frequencies between `F_MIN` and
`F_MAX`. `bands_of` reads the bands from raw readings and gives nothing until every band has
reported its frequency, gain and Q. `peak_db` finds the highest boost: 1024 samples, refined by a
golden-section search near the peak and near each band's centre, so a narrow boost between two
samples is still measured at its height. `threshold_step` turns that peak into the highest volume
step that stays clear of clipping, from the profile's attenuation table.

Nothing in `tone/response.rs` draws. A screen samples the curve for the columns it has. The same
file sums a headphone correction's peaking and shelf filters (`correction_db`) so a correction and a
fit can be compared on one scale.

### The curve in the page

The web page draws the same curve in JavaScript so that a band can follow the pointer without a
round trip (see [Dragging the curve's points](web-curve-dragging.md)): asking the
server for each frame of a drag would put a Bluetooth round trip between the finger and the line.
`bins/svolderd/web/core/tone-curve.js` is a straight port of `response.rs` (the peaking biquad at 48 kHz,
181 log-spaced points, `peak_db`'s 1024 samples with the same golden-section refinement and 60
steps, `clipping_peak` and `PEAK_SHOWN_DB`) and of the graph's maps in
`bins/svolderd/src/web/view/tone.rs` (x from the frequency, y from the dB clamped to the plot, and
their inverses), with the peak's words and a band's marker point: at the band's own gain in EQ and
Flat, at 0 dB in Loudness and with no mode read, as the page draws it at rest. It has no DOM and no
import, so node loads it as the browser does.

The page converts no number and words none. The tone part carries the tables it needs: every step of
every band as Hz, dB or Q with the words `units::band_text` gives it (`#tone-steps`, once per
screen, since the screen's shape covers the profile) and each band's raw readings (`data-readings`),
so a drag shows the core's own text and writes the step it shows. The band write's `raw` field takes
that step through the same value-to-step path as every other write.

Two languages hold one maths because of a fixture, so that the web and the TUI draw the same curve.
The Rust test `the_curve_fixture_is_current` (`bins/svolderd/src/web/view/tone/curve_fixture.rs`)
computes the core's own curves, line paths, raw and shown peaks, peak words and marker points for a
spread of band sets on the Gungnir 2's tone control (flat, boosts and cuts, overlaps, every band's
window edges, the gain and Q ranges, a peak under and just over what is shown, a peak away from
every band's centre, Flat and Loudness) and compares them with
`scripts/web-checks/node/fixtures/tone-curve.json`, failing when the file is stale. The node harness
`scripts/web-checks/node/tone-curve.mjs` checks the script against that file. After a change to the
response maths, the maps or the fixture's cases, rewrite the file with `SVOLDER_WRITE_FIXTURES=1
cargo test -p svolderd curve_fixture` and run `node scripts/web-checks/node/tone-curve.mjs`.

## Presets

A `Preset` (`crates/svolder-profiles/src/tone/preset.rs`) is a named tone curve in physical units,
Hz, dB and Q, not raw steps, because the same curve converts differently on different devices. It
has a mode, a loudness strength and one `PresetBand` per band.

- **Built-in presets** (`builtins`, names in `BUILTIN_NAMES`) are computed from a device's own
  `EqSpec` and never stored: Flat (every band at 0 dB on its centre and centre Q) for any band
  count, and Warm, Bass and Presence for five and three bands, each Flat with the bands it shapes
  set to fixed values.
- **User presets** are `[[preset]]` blocks in the configuration file, global rather than per device,
  and apply to every device whose tone control has the same number of bands (`fitting`,
  `Config::fitting_presets`). A preset cannot take a built-in's name, in any case (`new_name`);
  names are trimmed, at most `NAME_MAX` (40) characters, with no control characters, and compared
  without regard to case (`same_name`). The writers `Config::add_preset`, `update_preset`,
  `rename_preset` and `remove_preset` (`crates/svolder-ipc/src/config/presets.rs`) go through the
  configuration's locked edit, so two front ends saving at once both survive.

### Recall

`plan_recall` builds the writes that put a preset on a device from its current readings. It writes
only steps that differ, and orders them so a boost never sounds where it was not meant to: every
gain going down first, then frequency and Q changes, then every gain going up. The mode goes last
when the preset is in EQ mode, so the curve is complete before the device applies it, and first
otherwise. The loudness strength is written only for a loudness preset. A value outside its band's
window or the device's range goes to the nearest step and is reported in `Plan::clamped`; a preset
with another band count is an error. `Preset::eq_writes` returns the `Plan` and its `EqCommand`s,
the terms every front end sends.

The daemon recalls through the writer (`Writer::recall`, `bins/svolderd/src/writer.rs`) so the
plan's writes go out as a batch that other tone-control writes wait for, and `recall::plan`
(`bins/svolderd/src/recall.rs`) finds the preset and words a refusal for MQTT and MCP alike. The
writer remembers which preset was last recalled on each device and publishes it, so every front end
shows the same one as the one that is on.

### Which preset is on

`on` (`tone/preset.rs`) is the one rule every front end reads: `Unread` until the mode and every
band's three values have read; then the preset last recalled, while it is listed and still
`matches`; else the first listed preset that matches; else `Custom`. `matches` compares what a
recall would make the device hold: the mode, every band's gain, and frequency and Q except for a
band at zero gain, which shapes nothing, and the loudness strength only for a loudness preset. A
missing reading counts as a difference, and a preset named `custom` is never listed, so nothing is
ever on under that name. The last recalled preset wins a tie, so recalling one never makes another
that happens to match look current.

A band write made while the tone control reads Flat (the device's bypass, under which no band
applies) may be followed by a write of the mode EQ so the edit is heard (`SWITCHED_TO_EQ` is the
words every front end uses; the daemon side is `Options::flat_to_eq`).

## AutoEq import

A headphone correction is a list of filters, usually more than a device has bands, and in unlimited
ranges; the tone control has a few bands, each in a window, on a grid of steps. Import keeps the
original and fits it to each device.

**Reading** (`crates/svolder-profiles/src/tone/apo.rs`). `apo::parse` reads Equalizer APO parametric
text, the format AutoEq and squig.link publish: an optional `Preamp:` line and `Filter n: ON PK Fc
100 Hz Gain 3 dB Q 1` lines. Peaking and shelf filters are read; any other kind is skipped with a
note. A line it cannot read, a `Channel:` other than all, `GraphicEQ:`, `Include:` or `Device:`
refuses the whole file and names the line. `validate` holds the bounds every correction obeys
wherever it came from: at most `MAX_FILTERS` (63), frequencies 10 Hz to 24 kHz, gains and preamp
within 30 dB, Q above 0 and at most 20, a file of at most `MAX_BYTES` (64 KiB). The same bounds
check a `[[preset]]` read from the configuration file.

**Storing.** An imported preset is a `[[preset]]` block with `name`, optional `origin`, optional
`preamp` and a `filter` array (`type`, `f`, `g`, `q`), and none of the keys of a preset of bands
(`parse_imported`); a block with both kinds is refused. It has `bands = 0` and fits every device
whose tone control has bands, whatever the count (`Preset::is_imported`). The source is kept as
imported so it can be fitted again to another device; updating an imported preset with the device's
current curve is refused (`imported_update_refusal`), because that would replace the correction it
came from.

**Fitting** (`crates/svolder-profiles/src/tone/fit.rs`). `fit` finds, for a device's bands, the
frequencies, gains and Qs closest to the correction's summed response on 256 log-spaced points,
weighting 1 up to 10 kHz and 0.2 above (measurements are least trusted there). A correction with no
more peaking filters than bands, each landing in its matching band's window, maps one to one.
Anything else runs Levenberg-Marquardt from three deterministic starts, with each band's values
squeezed through a logistic onto its own window, then snaps to the device's steps and searches the
steps next door. The best weighted error wins; nothing is random, so the same input gives the same
bands on every run. `Fit` carries the RMS and worst error and the fitted curve's boost, which
`Fit::line` words for a front end ("fit 0.9 dB RMS ..."). `fit_for` caches fits per process, keyed
by the correction and the tone control's layout, because every row of a list asks for its fit every
frame.

**Recall** of an imported preset fits first (`Preset::for_device`) and then runs the ordinary plan
on the fitted bands; the fit stays on the device's steps, so no clamp can occur.

**The catalogue** (`crates/svolder-autoeq/src/`). `Client` fetches AutoEq's `results/INDEX.md` and
one `ParametricEQ.txt` per correction, blocking, within `Limits` (connect 10 s, index 60 s and 8
MiB, a file 20 s and 64 KiB). The index is cached on disk and fetched again by `Refresh` (`Never`,
`AfterWeeks`, `Now`; the `[autoeq] refresh_weeks` key); a failed refresh falls back to the cached
copy with a note. Text from the index only ever becomes a percent-encoded component of a URL, never
a local path. TLS verifies against the operating system's certificates; a redirect from an https
base to plain http is refused; `SVOLDER_AUTOEQ_URL` names another base, and a loopback base bypasses
proxies, which is how tests serve it.

## Device presets: the Loki Max

A Loki Max has no tone control (no `[eq]`) but has presets of its own, and svolder offers them
beside the DAC's with a different model. The device keeps three **device slots**, a recall
(`7.2.<p>`) and a save (`7.1.<p>`) each a command with empty data, and the profile lists five
**built-in settings**, each one raw step for each of the six bands (the protocol is in
[protocol.md](../protocol.md#loki-max-23), the profile format in [profiles.md](profiles.md)). A
slot lives on the device and a setting is data of one device, so neither is a `Preset`: nothing
is stored in Hz, dB or Q, nothing is converted per device, and there are no named curves of the
user's own for this device. `svolder_profiles::device_presets` plans and words all of it
(`offered`, `plan`, `matching`, `recall_ceiling_db`, `slot_name`, `parse_slot`), and a profile has
`[builtin_settings]` or `[eq]`, never both, so a device has one preset list in every front end.
`matching` is the one rule for "which setting is on": the first whose six bands all read its raw
steps, and none while any band is unread. A slot is never marked, because the device does not say
which it holds.

### How each front end offers them

- **TUI.** `p` opens a list over the device screen: the three slots, then the settings under a
  `Built in` header (`tui/boxes/device_presets.rs`, `tui/screens/device_presets.rs`). Enter recalls
  a slot or applies a setting, `u` on a slot saves after `Save the bands to preset 2? [y/N]`, `Esc`
  closes. The box takes every key, as the DAC's list does.
- **CLI.** `svolder set recall N` and `set save N [--yes]`, and `svolder preset list|apply NAME`
  (`commands/device/device_presets.rs`). The `eq` verbs stay the tone control's, and the two
  names of a Flat never meet.
- **Web.** One part, `device-presets`, in the place of the Presets part: slots (a tap recalls,
  and with writes on a save icon opens an in-place question) and settings with a lamp. Routes
  `device-presets/recall`, `save` and `apply`; a save needs `"confirm": true` in its body as well
  as the question (`web/view/device_presets.rs`, `web/writes/device_presets.rs`,
  `bins/svolderd/web/themes/ember/draw/parts.js`).
- **MCP.** `list_presets` and `load_preset` branch on the device: slots and settings, no save.
- **MQTT.** One select on the reserved `eq_preset` key: three slots, the settings, `custom`
  (`svolder-mqtt/src/device_presets.rs`). Its state is the setting the bands read, or `custom`.

### The batch, its order and the guard

An **action** is not a control (see profiles.md): it has no reading and no echo, so the session
writes it and waits for nothing, and the actor answers `Ack` at once. A setting is six waited raw
writes on `6.<band>`, planned from the readings by `device_presets::plan` and sent through
`Writer::apply` as one batch like a DAC recall: every band going down before any goes up, none for
a band already on its value, and an empty plan says `<name> is already on`.

The writer keeps the batch safe. A setting waits behind a band write in flight and for the echo of a
band written without waiting, holds the band writes that arrive meanwhile and the tone control's,
and does not touch the recall memory. A slot recall or save is held behind a running setting. A
recall goes out only once no write to its bands is in flight and no echo is awaited for any of them:
a band written without waiting is answered at its `Ack`, so the next write on the band is not held,
but its echo is awaited until it arrives or the set timeout passes, so an echo is never taken for a
report that follows the recall (an echo later than the set timeout is marked unsure, and so is the
band of a waited write answered with the set timeout). A band written again while an echo is
awaited, or after one missed its deadline and may still come, is awaited until the set timeout after
its latest write, since the first echo cannot be told from the second, and if that time passes it is
unreported after the next recall whatever the feed hears meanwhile. A setting's or a DAC recall's
own band writes are tracked the same way once its batch is done: a band step to a band whose echo is
still outstanding counts as a band written again. After a recall the readings are from before it: a
setting planned next treats each band as unread until that band reports anew and writes it whatever
it last read, and the TUI does the same for its own recalls.

The device is presumed to push its bands after a recall, and the session adds one guard for a unit
that does not: a recall that changes bands arms it, a report of any of them cancels it, and if none
comes within one second function `1` is written once. A save arms nothing.

### Safety, per front end

A recall's bands are the device's to say, up to 12 dB above 0 on a band; a setting is six band
writes and is treated as those writes. Nothing here writes the mode (`3`, which asks on every
change): the loader refuses a setting that names a control that is not a level or names the
volume, and an action cannot overlap a control's id. It also refuses an action keyed `save`
without `overwrites = true`.

| Front end | Setting that raises a band more than 6 dB | Other setting | Slot recall | Save |
| --- | --- | --- | --- | --- |
| TUI `p` | applies | applies | sends | `[y/N]`, only `y` sends |
| CLI | `preset apply` applies | applies | `set recall N` sends | `[y/N]` on a terminal; off one, refused without `--yes` |
| Web | applies | applies | sends | writers only; in-page question; server needs `confirm: true` |
| MCP | asks once, naming each band | applies | always asks, naming the 12 dB | not offered; `load_preset` refuses it |
| MQTT, Home Assistant | applies | applies | sends | not offered |
| Socket clients | not applicable | not applicable | `Request::Set` on `recall` sends | `Request::Set` on `save` sends |

The MCP judges every band a setting sets against its floor, not only the bands the plan writes,
because the writer plans again from its own readings; a setting already on answers `<name> is
already on`. The front ends that ask nothing for a band write ask nothing for a setting.
`set_control`, the command topics and the web `controls/{key}` route look keys up among controls,
so none of them reaches an action; `svolder raw 7.1.<p>` does, and `raw` is outside every rule by
design.

## Principles

**Presets are physical, devices are steps.** A preset stores Hz, dB and Q and is converted per
device at recall. Store raw steps and one curve means different sounds on two models.

**A recall is a batch, ordered for safety.** Gains down, then shape, then gains up, mode last when
entering EQ. Reorder it and a half-applied curve can boost somewhere unintended while the device is
playing.

**One rule for "which is on".** `on` and `matches` are the only definition, and every front end
reads them. A second definition would show different presets as current in the TUI and in Home
Assistant.

**Keep the source of an import.** The correction stays as published and is fitted per device on
demand, deterministically and cached. Storing only the fit would lose it for the next device and
make the preset unreproducible.

**Bounds are validated once, wherever the input came from.** A file, a download and a config block
all pass `validate`. A correction that gets past one door reaches the solver unchecked.

**Nothing from the network is a path or a command.** Index text becomes a percent-encoded URL
component, transfers are size- and time-limited, and a failed refresh keeps the cached copy. The
catalogue is a convenience that must never be able to hurt the machine.

**The clipping warning is computed once.** `clipping_peak` decides whether a boost can clip, and
returns nothing in Flat, in Loudness mode (the device's own contour is unmeasured), for a curve not
read in full, and for a peak too small to print. Duplicate the logic and the screens disagree about
whether to warn.

**A slot is never marked, a save always asks.** The device does not say which slot it holds, so no
front end shows one as current, and a save overwrites the slot with no undo, so every front end
that offers it asks first and the others do not offer it. A recall does not ask, as in the app,
except over MCP, which cannot see the user.

**An action is not a control.** Keeping it in its own list makes "never read, never set to a
value" a fact of the type instead of a filter in each front end, and none of the places that walk
the controls can reach it.

**The mode is never written by a device preset.** A setting names level controls only and an
action's ids overlap no control's, so neither can move a control that asks on every change.

## Where the code and tests live

- Model and conversion: `crates/svolder-profiles/src/model.rs`, `convert.rs`, `units.rs`.
- Curve and clipping: `crates/svolder-profiles/src/tone/response.rs`.
- Presets, recall, `on`: `crates/svolder-profiles/src/tone/preset.rs`, tests in
  `tone/preset/tests.rs`.
- Import: `tone/apo.rs` (tests in `tone/apo/tests.rs`), `tone/fit.rs` (tests in
  `tone/fit/tests.rs`), all in `crates/svolder-profiles/src/`.
- The catalogue: `crates/svolder-autoeq/src/client.rs`, `index.rs`, with tests beside them.
- `[[preset]]` in the configuration: `crates/svolder-ipc/src/config/parse.rs`, `config/presets.rs`;
  tests in `config/tests/`.
- The daemon's recall: `bins/svolderd/src/recall.rs`, `bins/svolderd/src/writer.rs`; tests in
  `bins/svolderd/src/writer/tests.rs`.
- Device presets: `crates/svolder-profiles/src/device_presets.rs` (tests in
  `device_presets/tests.rs`), the profile's `actions` and `[builtin_settings]` in `model.rs` and
  `registry.rs`; the session's alias read and guard in `crates/svolder-ble/src/session.rs`.
- Their writer batch: `Writer::apply`, `bins/svolderd/src/recall.rs` (`plan_setting`) and
  `writer.rs`; the fake's slots in `crates/svolder-fake/src/device.rs`.
- Their front ends: `bins/svolder/src/commands/device/device_presets.rs`,
  `bins/svolder/src/tui/boxes/device_presets.rs`, `tui/screens/device_presets.rs`,
  `bins/svolderd/src/mcp/tools.rs`, `crates/svolder-mqtt/src/device_presets.rs`,
  `bins/svolderd/src/web/view/device_presets.rs`, `bins/svolderd/src/web/writes/device_presets.rs`
  and `bins/svolderd/web/themes/ember/draw/parts.js`.
