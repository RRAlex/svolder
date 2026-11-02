/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The words of the theme Paper: its fixed words in one place, and the few lines it works out from
// the model (the title's line about the device, the footer's date). Pure: the model in, text out.
// Paper writes only in characters its two faces carry (`CHARS`): no arrow (a note's `ARROW` marker
// is drawn as a stroke), no tick, no transport glyph, no caret; the true minus sign is in both
// faces.

/** The characters Paper's fonts carry: what the Latin 1 subsets of Crimson Pro and Caveat cover. */
export const CHARS = new RegExp(
  '^[\\u0020-\\u007E\\u00A0-\\u00FF\\u0131\\u0152\\u0153\\u02C6\\u02DA\\u02DC\\u2013\\u2014'
  + '\\u2018-\\u201A\\u201C-\\u201E\\u2020-\\u2022\\u2026\\u2030\\u2039\\u203A\\u2044\\u20AC'
  + '\\u2122\\u2212\\uFB01\\uFB02]*$',
);

const PARAM_WORD = { f: 'frequency', g: 'gain', q: 'Q' };

/** Where a note's words say "to" between two values: a marker the notes' drawing turns into a
 * hand-drawn arrow (the faces have no arrow glyph) followed by the word `to` for a reader who
 * copies the note or listens to it. A private-use character, never U+2192. */
export const ARROW = '\uE000';

/** The words of the path note that the notes' drawing follows with an ink tick. */
export const RATE_MATCHES = 'rate matches all the way';

/** The words the page says about itself, as Paper writes them. */
export const WORDS = {
  reconnecting: 'Reconnecting to svolder…',
  waiting: 'reading the device again…',
  also: 'also here:',
  signIn: 'sign in',
  signOut: 'sign out',
  themeName: 'Paper',
  program: 'svolder',
  readOnly: 'read only:',
  signInToChange: 'sign in to change this',
  /** The volume's written controls and the words it writes beside the needle. */
  quieter: '\u2212 quieter',
  louder: 'louder +',
  mute: 'mute',
  unmute: 'unmute',
  muted: 'muted',
  unread: 'not read yet',
  cap: 'cap',
  risk: 'may clip',
  /** The tone: its label, the preset when none is on, the loudness strength and the band table's
   * heads. */
  toneLabel: 'Tone',
  noPreset: 'no preset',
  strength: 'strength',
  bandHeads: ['band', 'frequency', 'gain', 'Q'],
  modeList: 'Tone mode',
  presetList: 'Presets',
  lowerLoudness: 'Lower the loudness',
  raiseLoudness: 'Raise the loudness',
  /** The words of a band parameter's two steppers (`param` is `f`, `g` or `q`). */
  lower: (band, param) => `Lower the ${PARAM_WORD[param]} of band ${band}`,
  raise: (band, param) => `Raise the ${PARAM_WORD[param]} of band ${band}`,
  /** A band's point on the graph: its number, frequency and gain as the device words them. */
  point: (band, f, g) => `Band ${band}: ${f}, ${g}`,
  /** The controls: a toggle's three states, a list's name, and the readout's and scale's words. */
  onWord: 'on',
  offWord: 'off',
  unknownWord: '\u2014',
  controlsLabel: 'Controls',
  /** The presets: the headings, the words that write, and the lists they open. */
  presetsLabel: 'Presets',
  onDevice: 'On the device',
  builtIn: 'Built in',
  saveAs: 'save as\u2026',
  editPresets: 'rename or delete\u2026',
  saveBands: 'save the bands to\u2026',
  editList: 'Rename or delete',
  saveList: 'Save the bands to',
  rename: (name) => `Rename \u201C${name}\u201D`,
  remove: (name) => `Delete \u201C${name}\u201D`,
  /** The forms and questions: their fields' names, their answers and their sentences. */
  presetName: 'Preset name',
  newName: 'New name',
  answers: {
    save: 'save', rename: 'rename', cancel: 'cancel', delete: 'delete', keep: 'keep',
  },
  askDelete: (name) => `Delete \u201C${name}\u201D? The bands on the device stay as they are.`,
  askSlot: (slot) => `Save the bands to ${slot}? What the device holds there is replaced.`,
  /** Now playing: the heading, the transport's words and what it says with no player. */
  playingLabel: 'Playing',
  nothingPlaying: 'nothing playing',
  back: 'back',
  pause: 'pause',
  play: 'play',
  next: 'next',
  /** The amp: the power line's lead, the meters' heading. */
  powerLead: 'Power: ',
  metersLabel: 'Meters',
  /** The device's facts and the card's words. */
  deviceLabel: 'The device',
  readingsLabel: 'All readings',
  identify: 'identify',
  identifyNote: 'Asks the unit to show itself, so you can tell it from the others.',
  tryNow: 'try now',
  retryIn: (secs) => (secs > 0 ? `trying again in ${secs} s` : 'trying again now'),
  product: (name, id) => name ?? `product ${id}`,
  ago: (age) => `${age} ago`,
  /** The key list's line: `T for themes`. */
  keyHint: (k) => `${k.key} for ${k.words.toLowerCase()}`,
  /** What the hand writes where an arrow is drawn (for the ear), and its minus sign. */
  handArrow: 'to',
  handMinus: '−',
};

/** The first rate (`48 kHz`, `44.1 kHz`) in a hop's detail, or nothing. */
const rateIn = (detail) => /\d+(?:\.\d+)? kHz/.exec(detail ?? '')?.[0] ?? null;

/** The words after the device's kind in the title: its link, the input it plays from and the
 * rate it plays at. */
function deviceLine(page, device) {
  const parts = [device.family_word ?? ''];
  if (device.link?.word) parts.push(`, ${device.link.word.toLowerCase()}`);
  const input = (device.controls ?? []).find((c) => c.key === 'input' && c.kind === 'choice');
  const chosen = input?.options?.find((o) => o.on)?.label;
  if (chosen) parts.push(` over ${chosen}`);
  const readout = (device.controls ?? []).find((c) => c.key === 'sample_rate'
    && c.kind === 'readout');
  const hop = device.playing?.path?.hops?.find((h) => h.label === 'DAC');
  const rate = readout?.text || rateIn(hop?.detail);
  if (rate) parts.push(` at ${rate}`);
  return parts.join('');
}

/**
 * The title: the device's name, and a line in italics under or beside it: its kind, link, input
 * and rate, then what the viewer may not do (`read only: sign in to change this` for a guest,
 * the daemon's own words for a page whose writes are off). Without a device the title is the
 * program's name.
 */
export function titleWords(page, device) {
  const viewer = page?.viewer ?? {};
  const note = viewer.guest ? `${WORDS.readOnly} ${WORDS.signInToChange}` : viewer.read_only;
  const line = device ? deviceLine(page, device) : '';
  return {
    name: device?.name ?? WORDS.program,
    sub: [line, note].filter(Boolean).join(' · '),
  };
}

/** A time as the footer dates it, by the reader's own calendar: the day and the month in words,
 * `30 October`. */
export function dateWords(ms) {
  return new Date(ms).toLocaleDateString('en-GB', { day: 'numeric', month: 'long' });
}
