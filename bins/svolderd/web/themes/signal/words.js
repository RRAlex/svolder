/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The fixed words of the theme Signal and the small lines worked out from the model's numbers.

/** The quiet line on a page whose computer the daemon does not watch. */
export const NOT_WATCHED = 'the computer is not watched';

/** The ring before an amp no hop links: what comes before it is not a DAC svolder drives. */
export const NO_DAC = 'a DAC svolder does not drive';

/** The words under the ring the applied AutoEq preset names. */
export const AUTOEQ = 'from your AutoEq preset';

/** What a stretch of the line does to the sound; the DAC says AS_SENT when it reads the rate
 * the computer sends. */
export const CHANGES = 'changes the sound';
export const AS_SENT = 'arrives as sent';

/** The computer's hop on the line is its audio server. */
export const COMPUTER_NAME = 'PipeWire';

/** The player's name when the track says none. */
export const PLAYER_NAME = 'Player';

/** What a declared hop's kind is called, as the person's own. */
export const KIND_WORDS = {
  amp: 'your amp', headphones: 'your headphones', speakers: 'your speakers', other: 'your gear',
};

/** A sample rate in Hz as `96 kHz` or `44.1 kHz`; empty for no rate. */
export function rateWords(hz) {
  if (!Number.isFinite(hz) || hz <= 0) return '';
  return `${Number((hz / 1000).toFixed(1))} kHz`;
}

/** The player's name: what follows the last ` · ` of the track's source, else the plain word. */
export function playerName(source) {
  const name = String(source ?? '').split(' · ').pop().trim();
  return name || PLAYER_NAME;
}

/** More hops than this and a label is its name alone. */
export const LABEL_LIMIT = 7;

/** What the computer's hop says before its route is read. */
export const ROUTE_NOT_READ = 'route not read';

/** The words of the open area's first screen for a DAC and of the Now playing bar. */
export const WORDS = {
  volume: 'Volume',
  louder: 'Louder',
  quieter: 'Quieter',
  mute: 'Mute',
  unmute: 'Unmute',
  muted: 'muted',
  silence: '\u2212\u221e',
  unread: 'not read yet',
  cap: 'cap',
  tone: 'Tone',
  input: 'Input',
  mode: 'Mode',
  preset: 'Preset',
  devices: 'Show another device',
  nothingPlaying: 'Nothing playing',
  previous: 'Previous',
  next: 'Next',
  pause: 'Pause',
  play: 'Play',
  /** A band's point on the graph: its number, frequency and gain as the device words them. */
  point: (band, f, g) => `Band ${band}: ${f}, ${g}`,
};

/** The open area of a live amp linked after the DAC (or standing alone). */
export const AMP = {
  meters: 'Output',
  meterNote: 'Inside the shaded part the amp runs in class A.',
  noVolume: (dac) => (dac
    ? `This amp has no volume of its own: the ${dac}\u2019s volume is the one that sets the level.`
    : 'This amp has no volume of its own.'),
  channel: (c) => `${c} channel`,
  name: (name) => `The ${name}`,
  identify: 'Identify',
  unlink: 'Unlink',
  askUnlink: (name) => `Unlink the ${name}? It stays in svolder\u2019s list and stands alone; nothing on the device changes.`,
  faults: 'Faults',
  reading: 'Reading the amp\u2026',
  showDevice: (name) => `Show ${name}`,
};

/** The rest of a device below the first screen: the band table, presets, controls, the device's
 * own presets and its facts. */
const PARAM_WORD = { f: 'frequency', g: 'gain', q: 'Q' };
export const SHELF = {
  bands: 'Bands',
  bandHeads: ['Band', 'Frequency', 'Gain', 'Q'],
  bandNote: 'Type a value and press Enter, or step it; the curve above takes drags.',
  /** A band's value field and its two steppers, named for a screen reader. */
  field: (band, param) => `${PARAM_WORD[param][0].toUpperCase()}${PARAM_WORD[param].slice(1)} of band ${band}`,
  lower: (band, param) => `Lower the ${PARAM_WORD[param]} of band ${band}`,
  raise: (band, param) => `Raise the ${PARAM_WORD[param]} of band ${band}`,
  minus: '\u2212',
  plus: '+',
  presets: 'Presets',
  saveAs: 'Save as\u2026',
  rename: 'Rename',
  remove: 'Delete',
  whichRename: 'Rename which preset',
  whichDelete: 'Delete which preset',
  presetName: 'Preset name',
  newName: 'New name',
  answers: {
    save: 'Save', rename: 'Rename', cancel: 'Cancel', delete: 'Delete', keep: 'Keep', unlink: 'Unlink', remove: 'Remove',
  },
  askDelete: (name) => `Delete \u201C${name}\u201D? The bands on the device stay as they are.`,
  askSlot: (slot) => `Save the bands to ${slot}? What the device holds there is replaced.`,
  controls: 'Controls',
  power: 'Power',
  on: 'On',
  off: 'Off',
  unknown: '\u2014',
  loudness: 'Loudness',
  lowerLoudness: 'Lower the loudness',
  raiseLoudness: 'Raise the loudness',
  onDevice: 'On the device',
  saveBands: 'Save the bands to\u2026',
  saveList: 'Save the bands to',
  builtIn: 'Built in',
  factsTitle: (name) => `The ${name}`,
  factsShow: 'show the facts',
  factsHide: 'hide',
  readings: 'All readings',
  identify: 'Identify',
  identifyNote: 'Asks the unit to show itself, so you can tell it from the others.',
  ago: (age) => `${age} ago`,
  product: (name, id) => name ?? `product ${id}`,
};

/** The `+` of the line, the sheet it opens and the open area of a hop the person declared. */
export const HOPS = {
  addMenu: 'Add device…',
  addNote: 'an amp, headphones, speakers',
  sheetTitle: (dac) => `After the ${dac}`,
  what: 'What is it',
  name: 'Name',
  note: 'A note (optional)',
  connected: (name) => `${name} (connected)`,
  declaredNote: 'svolder cannot see this one, so it shows as declared: no live facts. '
    + 'A Forkbeard amp svolder controls is offered here instead, live.',
  linkedNote: 'svolder controls this amp: it shows live, with its power and meters.',
  cancel: 'Cancel',
  addGo: 'Add',
  save: 'Save',
  kinds: [['amp', 'Amp'], ['headphones', 'Headphones'], ['speakers', 'Speakers'], ['other', 'Other']],
  declaredBy: (kind) => `Declared by you · ${kind}`,
  linkedBy: 'Linked to a device',
  edit: 'Edit',
  editNote: 'Edit the note',
  earlier: '← Move earlier',
  later: 'Move later →',
  remove: 'Remove',
  noNote: 'no note',
  between: (before, after) => (after
    ? `It sits after the ${before}, before the ${after}.` : `It sits after the ${before}, at the end of the line.`),
  declaredLine: 'svolder cannot see this one, so it shows only what you wrote.',
  linkedLine: (device) => `It links ${device}, which svolder does not list now.`,
  askRemove: (name) => `Remove ${name} from the line? Nothing on a device changes.`,
};

/** The open area of the player (P4) and of the computer (P5), and the page's states (P6). */
export const PLAYER = {
  title: 'Now playing',
  path: 'The way to the headphones',
  noPath: 'The path is not read yet.',
  noTrack: 'Nothing is playing.',
};

export const COMPUTER = {
  title: 'The computer',
  sink: 'Sound card',
  format: 'Rate and format',
  volume: 'Volume in PipeWire',
  muted: 'Muted',
  hostEq: 'Host EQ',
  others: 'Other streams',
  path: 'The sound’s way',
  yes: 'yes',
  no: 'no',
  eqOff: 'off',
  eqOn: 'on',
  mixing: (n) => (n === 1 ? '1 other stream mixes in' : `${n} other streams mix in`),
  alone: 'none',
  noSink: 'no sound card found',
  unread: ROUTE_NOT_READ,
  note: 'Read only: this is what the computer does to the sound before it reaches the DAC.',
};

/** The page's states: the stream lost, the page waiting or resting, a device's card. */
export const STATES = {
  reconnecting: 'Reconnecting to svolder…',
  waiting: 'Reading the device again…',
  waitingLine: 'reading the device again…',
  resting: 'Resting',
  restingLine: 'Paused while the page is hidden; it reads the device again when shown.',
  tryNow: 'Try now',
  retryIn: (secs) => (secs > 0 ? `next try in ${secs} s` : 'trying again now'),
  last: 'Last heard, not live',
};
