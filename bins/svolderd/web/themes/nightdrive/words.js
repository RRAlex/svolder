/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The fixed words of Night drive that are not worked out from the model.

export const SCENE = Object.freeze({
  label: 'Night drive',
  scene: 'The music as an outrun road',
  menu: 'Menu',
  motion: 'Motion',
  device: 'Device…',
});

/** The motion levels' words in the menu. */
export const MOTION_WORDS = Object.freeze({ full: 'Full', calm: 'Calm', still: 'Still' });

/** The HUD's fixed words: the path's verdict, the EQ line's names and the amp's note. */
export const HUD = Object.freeze({
  hostEq: 'host EQ on',
  changes: 'changes the sound',
  eq: 'EQ',
  off: 'OFF',
  unconfirmed: 'unconfirmed',
  flat: 'FLAT',
  loudness: 'LOUDNESS',
  silence: '−∞',
  defaultUnit: 'dB',
  noVolume: (dac) => (dac
    ? `This amp has no volume of its own: the ${dac}’s volume is the one that sets the level.`
    : 'This amp has no volume of its own.'),
});

/** The switch's words: its group's name and why it cannot be used. */
export const SWITCH = Object.freeze({
  label: 'Show the controls of',
  unavailable: (why) => `computer controls unavailable: ${why}`,
});

/** The dash's fixed words: the keys, the input, the transport and the way to the sheet. */
export const DASH = Object.freeze({
  label: 'Controls',
  quieter: 'Quieter',
  louder: 'Louder',
  mute: 'MUTE',
  input: 'Input',
  power: 'Power',
  previous: 'Previous',
  next: 'Next',
  pause: 'Pause',
  play: 'Play',
  device: 'DEVICE ▴',
  deviceLabel: 'Device settings',
  nothing: 'Nothing playing',
});

/** The page's states: the stream lost, the page reading again or resting, the card of a device out of reach and the
 * music's analysis missing. */
export const STATES = Object.freeze({
  reconnecting: 'Reconnecting to svolder…',
  waitingLine: 'reading the device again…',
  restingLine: 'Paused while the page is hidden; it reads the device again when shown.',
  tryNow: 'Try now',
  retryIn: (secs) => (secs > 0 ? `next try in ${secs} s` : 'trying again now'),
  noCapture: (why) => `no music analysis: ${why || 'the computer’s audio cannot be read'}`,
});

/** The sheet's fixed words: its headings, the band table, the presets and what a question asks. */
const PARAM_WORD = Object.freeze({ f: 'frequency', g: 'gain', q: 'Q' });
export const SHEET = Object.freeze({
  label: 'Device settings',
  close: 'Close',
  mode: 'Mode',
  input: 'Input',
  bands: 'Bands',
  bandHeads: ['#', 'Freq', 'Gain', 'Q'],
  bandNote: 'Type a value and press Enter, or step it; the mountains take drags.',
  field: (band, param) => `${PARAM_WORD[param][0].toUpperCase()}${PARAM_WORD[param].slice(1)} of band ${band}`,
  lower: (band, param) => `Lower the ${PARAM_WORD[param]} of band ${band}`,
  raise: (band, param) => `Raise the ${PARAM_WORD[param]} of band ${band}`,
  minus: '−',
  plus: '+',
  strength: (value) => `strength ${value ?? '—'}`,
  lowerLoudness: 'Lower the loudness',
  raiseLoudness: 'Raise the loudness',
  points: 'Points',
  pointHeads: ['#', 'Type', 'Freq', 'Gain', 'Q'],
  addPoint: '+ Add point',
  removePoint: (n) => `Remove point ${n}`,
  pointType: (n) => `Type of point ${n}`,
  pointField: (n, param) => `${PARAM_WORD[param][0].toUpperCase()}${PARAM_WORD[param].slice(1)} of point ${n}`,
  moreBelow: (hidden) => (hidden > 0 ? `${hidden} more below ↓` : ''),
  notNumber: 'Not a number.',
  unconfirmed: 'not confirmed yet',
  presets: 'Presets',
  saveAs: 'Save as…',
  rename: 'Rename',
  remove: 'Delete',
  whichRename: 'Rename which preset',
  whichDelete: 'Delete which preset',
  presetName: 'Preset name',
  newName: 'New name',
  answers: { save: 'Save', rename: 'Rename', cancel: 'Cancel', delete: 'Delete', keep: 'Keep' },
  askDelete: (name) => `Delete “${name}”? The bands on the device stay as they are.`,
  askSlot: (slot) => `Save the bands to ${slot}? What the device holds there is replaced.`,
  controls: 'Controls',
  power: 'Power',
  on: 'On',
  off: 'Off',
  unknown: '—',
  onDevice: 'On the device',
  saveBands: 'Save the bands to…',
  saveList: 'Save the bands to',
  builtIn: 'Built in',
  faults: 'Faults',
  factsTitle: (name) => `The ${name}`,
  factsShow: 'show the facts',
  factsHide: 'hide',
  readings: 'All readings',
  identify: 'Identify',
  identifyNote: 'Asks the unit to show itself, so you can tell it from the others.',
  ago: (age) => `${age} ago`,
  product: (name, id) => name ?? `product ${id}`,
});

/** The amp's gauges in the dash. */
export const GAUGE = Object.freeze({
  power: 'Power',
  temperature: 'Temperature',
});

/** The amp's meters on the sheet. */
export const METERS = Object.freeze({
  title: 'Output',
  note: 'Inside the shaded part the amp runs in class A.',
  channel: (c) => `${c} channel`,
});
