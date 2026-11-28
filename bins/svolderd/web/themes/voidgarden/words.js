/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme's fixed words: the HUD's modules and rows, the audio panel's titles and traces. The
// words that come from the daemon (device names, the path's stage words, the link's word) are never
// written here; a row says them as they arrive.

/** The HUD: each module's title and each row's label (shown in capitals by the style sheet). */
export const HUD = Object.freeze({
  host: 'Host',
  dac: 'DAC',
  amp: 'Amp',
  output: 'Output',
  format: 'Format',
  eq: 'EQ',
  xfeed: 'XFeed',
  speakers: 'V.Spkr',
  loudness: 'Loudness',
  vuL: 'VU L',
  vuR: 'VU R',
  path: 'Path',
  volume: 'Volume',
  input: 'Input',
  link: 'Ctrl',
  state: 'State',
  power: 'Power',
  tone: 'Tone',
  gain: 'Gain',
  on: 'on',
  off: 'off',
  muted: 'muted',
  unconfirmed: 'unconfirmed',
  silence: '−∞',
  changes: 'changes the sound',
  weak: 'weak link',
  noReading: '—',
  unit: 'dBFS',
  powerOn: 'On',
  standby: 'Standby',
  from: (rate) => `from ${rate}`,
  points: (n) => `${n} points`,
  depth: (db) => `depth ${db} dB`,
  strength: (s) => `strength ${s}`,
  hostEq: 'host EQ on',
  over: 'over',
  chipOpens: (chip) => `${chip}: open the PipeWire config`,
});

/** The audio panel: its title, its mode words and each feature's label (`key` is the feature's). */
export const PANEL = Object.freeze({
  title: 'Audio features',
  now: 'now',
  modes: Object.freeze({ panel: 'Panel', spread: 'Spread', off: 'Off' }),
  features: Object.freeze({
    bass: 'Bass',
    mid: 'Mids',
    high: 'Highs',
    l: 'L RMS',
    r: 'R RMS',
    width: 'Width',
    loud: 'Loud',
    flux: 'Novelty',
    cent: 'Centroid',
    flat: 'Flatness',
  }),
  over: 'Over',
  link: 'Link',
  tone: 'Tonal',
  noise: 'Noisy',
});
