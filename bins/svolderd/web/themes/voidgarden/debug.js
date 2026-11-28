/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's readout and its bisect switches, asked for on the address: `?vg-debug` shows the
// readout, `?vg-debug=nosky,nodash` also turns the named parts off, to find on a phone what costs
// the most. The query rides through the core's own routing; a fragment does not (the core drops it
// at once). Pure: nothing here touches the page.

/** The switches that turn off one layer of the scene (its name in layers.js). */
export const LAYER_SWITCHES = Object.freeze({
  nosky: 'sky',
  noland: 'land',
  noriver: 'river',
  noflora: 'flora',
  nobeacons: 'beacons',
  nohero: 'hero',
  noparticles: 'particles',
  nopost: 'post',
});

/** The switches: each turns off one layer or one part of the page, with what it takes away. */
export const SWITCHES = Object.freeze({
  nosky: 'the sky',
  noland: 'the land',
  noriver: 'the river',
  noflora: 'the plants',
  nobeacons: 'the beacons',
  nohero: 'the hero',
  noparticles: 'the particles',
  nopost: 'the post chain: bloom, grain and the rest',
  nodash: 'the dash along the bottom',
  nohud: 'the HUD',
  nopanel: 'the audio panel',
});

/** What a query string asks: `null` for no readout, else `{ off: Set of layer names, parts: Set of
 * the page's parts (`dash`, `hud`, `panel`), switches: [the switches in force], unknown:
 * [words] }`. */
export function debugOf(search) {
  const params = new URLSearchParams(search ?? '');
  if (!params.has('vg-debug')) return null;
  const words = params.get('vg-debug').split(',')
    .map((w) => w.trim().toLowerCase()).filter(Boolean);
  const known = words.filter((w) => Object.hasOwn(SWITCHES, w));
  return {
    off: new Set(known.filter((w) => Object.hasOwn(LAYER_SWITCHES, w))
      .map((w) => LAYER_SWITCHES[w])),
    parts: new Set(known.filter((w) => !Object.hasOwn(LAYER_SWITCHES, w)).map((w) => w.slice(2))),
    switches: known,
    unknown: words.filter((w) => !Object.hasOwn(SWITCHES, w)),
  };
}

/** What this page was opened with (read once, so a step to another device, which drops the query
 * from the address, leaves the readout on). */
export const ASKED = debugOf(globalThis.location?.search);

/** `create` (layers.js `createLayers`) with the layers the address's `asked` switched off left
 * out: the maker the scene's wire is handed. */
export const withSwitches = (create, asked) => (gpu, options) => create(
  gpu, { ...options, off: asked?.off ?? new Set() },
);

/** `on` or `off` for a switch that is only `off` when it is false. */
const onOff = (value) => (value === false ? 'off' : 'on');

/** `value` to `digits` places, or `n/a` for no number. */
const num = (value, digits = 1) => (Number.isFinite(value) ? value.toFixed(digits) : 'n/a');

/**
 * The readout's lines from `read`: `{ perf, fps, canvas, dpr, switches }`. `perf` is the scene's
 * (`driver.perf()`, null before the scene is mounted), `fps` the draws in the last second, `canvas`
 * the backing size `[w, h]`. The first line names the tier as the picker's card does.
 */
export function readoutLines(read) {
  const {
    perf, fps, canvas, dpr, switches,
  } = read;
  const gpu = Number.isFinite(perf?.gpuMs) ? `${num(perf.gpuMs)} ms` : 'n/a';
  const tier = perf?.words ?? (perf?.tier === 'none' ? 'no GPU scene' : 'starting');
  return [
    tier,
    `${num(fps, 0)} draws/s  asked ${perf?.asked ? `${num(perf.asked, 0)} ms` : 'every frame'}`,
    `draw ${num(perf?.ms)} ms  gpu ${gpu}`,
    `canvas ${canvas[0]}x${canvas[1]}  dpr ${dpr}`,
    `scale ${num(perf?.scale, 2)}  back ${num(perf?.back, 2)}  capped ${perf?.capped ? 'yes' : 'no'}  `
      + `step ${perf?.step ?? 0}/${perf?.rungs ?? 0}`,
    `sky ${num(perf?.sky, 1)}  water ${num(perf?.water, 1)}  det ${num(perf?.det, 1)}  `
      + `rings ${num(perf?.rings, 2)}  flora ${num(perf?.flora, 2)}`,
    `q ${num(perf?.q, 2)}  post ${num(perf?.post, 1)}  grit ${onOff(perf?.grit)}  `
      + `bloom2 ${onOff(perf?.bloom2)}  hero ${num(perf?.hero, 2)}`,
    `motion ${perf?.motion ?? 'n/a'}  detail ${perf?.detail ?? 'n/a'}`,
    switches.length ? `off: ${switches.join(' ')}` : 'off: nothing',
  ];
}
