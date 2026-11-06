/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What every theme must present, as data: the controls and actions a device's model offers and
// the facts it holds. The contract checks read a theme's test driver against these lists: for each
// target the driver must reach it and operate it, for each fact it must reach it and show the
// model's words (see the checks' own README). Plain data in and out, no DOM.
//
// A target names one thing a person can use:
//   { control: key }             the volume, its mute, every control of the device (a choice, a
//                                toggle, a slider, a readout), the power switch, its toggles and
//                                readouts
//   { band, param }              one of `f`, `g`, `q` of a band of the tone curve
//   { mode }                     a tone mode (flat, eq, loudness)
//   { loudness: true }           the loudness strength, while the mode is loudness
//   { preset: name }             a tone preset
//   { slot: n }                  a slot of the device's own presets (recall it, save over it)
//   { setting: name }            a built-in setting of the device's own presets
//   { transport: button }        a button of the computer's player: listed only when the viewer has
//                                the buttons and there is a track (no track, nothing to operate)

/** The parameters of a band, as the band writes name them. */
const PARAMS = ['f', 'g', 'q'];

/**
 * Every control and action `device` (a device's model) offers, in the order the model lists
 * them; `[]` for no device.
 */
export function targetsOf(device) {
  const targets = [];
  if (!device) return targets;
  const { volume, power, tone, presets } = device;
  if (volume) {
    targets.push({ control: volume.key });
    if (volume.mute) targets.push({ control: volume.mute });
  }
  for (const control of device.controls ?? []) targets.push({ control: control.key });
  if (power) {
    targets.push({ control: power.key });
    for (const control of [...(power.toggles ?? []), ...(power.readouts ?? [])]) {
      targets.push({ control: control.key });
    }
  }
  if (tone) {
    for (const row of tone.bands?.rows ?? []) {
      for (const param of PARAMS) targets.push({ band: row.band, param });
    }
    for (const mode of tone.modes ?? []) targets.push({ mode: mode.name });
    if (tone.loud) targets.push({ loudness: true });
  }
  for (const row of presets?.rows ?? []) targets.push({ preset: row.name });
  for (const slot of device.device_presets?.slots ?? []) targets.push({ slot: slot.slot });
  for (const built of device.device_presets?.builtins ?? []) targets.push({ setting: built.name });
  // The buttons are the viewer's to press, and there is a player to press them on.
  const track = device.playing?.track;
  if (device.playing?.buttons && track) {
    const playing = track.status === 'playing';
    for (const button of ['previous', playing ? 'pause' : 'play', 'next']) {
      targets.push({ transport: button });
    }
  }
  return targets;
}

/**
 * The facts `device` holds, each by the label the Device section gives it: Product, Unit (when
 * the viewer may see it), Firmware, DSP, Profile, Link, Signal, Passphrase (when the device has
 * a note on it) and the id of every reading row; then Cover, when the computer's player has a
 * cover for the track (shown as a picture whose address carries the cover's version). `[]` for a
 * device with no facts and no cover.
 */
export function factsOf(device) {
  const labels = [];
  const facts = device?.facts;
  if (facts) {
    labels.push('Product');
    if (facts.unit) labels.push('Unit');
    labels.push('Firmware', 'DSP', 'Profile', 'Link', 'Signal');
    if (facts.passphrase_note !== null && facts.passphrase_note !== undefined) {
      labels.push('Passphrase');
    }
    for (const row of facts.rows ?? []) labels.push(row.id);
  }
  const cover = device?.playing?.track?.cover;
  if (cover !== null && cover !== undefined) labels.push('Cover');
  return labels;
}
