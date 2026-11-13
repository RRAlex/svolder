/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember's key table (core/keys.js says what an entry is): every key Ember answers to,
// as data. The keys of the whole page come first; keys.js gives each its `run` by its `id` and
// binds them. The rest say which keys a widget handles itself: the dial, a slider, a section's
// handle in Arrange mode, a typed band value, a cell of the computer's points typed in, the
// curve.
// An entry marked `own` is answered by a listener of Ember's own on the document, whose rule is
// not the binding's: the Escape that leaves Arrange mode (it acts from a field too, arrange.js)
// and the Escape that ends a drag on a curve (in the capture phase, tone-graph.js and
// computer.js).
// `ZONES` names each zone for the Help popup, which is the one list of these keys a person
// reads. Pure, with no DOM.
//
// The facts a `when` reads (`ctx`) are the core's `canWrite` and, from keys.js, what the page
// shows now: `lost` (the link is down), `arranging`, `held` (a point of a curve is held), `viz`
// (the screen has a visualizer card), `knob`, `mute`, `balance`, `flat` and `presets` (the
// screen has that control). A fact that is not given counts as there.

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];

/** The sliders that have a centre to go back to (the balance): the others have no Home. */
const CENTRED = 'sv-slider[data-centre]:not([data-centre=""])';

/** Whether a key that writes to the device has anything to do: the viewer may write, the link is
 * up and the sections are not being arranged. */
const writes = (ctx) => ctx.canWrite === true && ctx.lost !== true && ctx.arranging !== true;

export const KEYS = [
  // ---- the page's keys
  {
    id: 'volume', key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'], zone: 'page',
    words: 'Volume one step', repeat: true, when: (ctx) => writes(ctx) && ctx.knob !== false,
  },
  {
    id: 'mute', key: 'm', zone: 'page', words: 'Mute',
    when: (ctx) => writes(ctx) && ctx.mute !== false,
  },
  {
    id: 'centre', key: '0', zone: 'page', words: 'Centre the balance',
    when: (ctx) => writes(ctx) && ctx.balance !== false,
  },
  {
    id: 'look', key: 'v', zone: 'page', words: 'Change the visualizer\'s look',
    when: (ctx) => ctx.viz !== false,
  },
  {
    id: 'flat', key: 'f', zone: 'page', words: 'Tone control to Flat',
    when: (ctx) => writes(ctx) && ctx.flat !== false,
  },
  {
    id: 'preset', key: 'p', zone: 'page', words: 'Go to the preset that is on',
    when: (ctx) => writes(ctx) && ctx.presets !== false,
  },
  // not a key: what the pointer does on the curve
  {
    id: 'curve', key: 'Curve', gesture: true, zone: '.eqg .plot',
    words: 'Drag a point for frequency and gain; wheel, right drag or pinch for Q',
    when: (ctx) => ctx.canWrite === true,
  },
  {
    id: 'arrange', key: 'A', keys: ['A', 'a'], zone: 'page', words: 'Arrange sections',
  },

  // ---- the keys a widget handles itself
  {
    id: 'dial-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: 'sv-knob',
    words: 'Volume one step', when: writes,
  },
  {
    id: 'dial-page', key: ['Page Up', 'Page Down'], keys: ['PageUp', 'PageDown'], zone: 'sv-knob',
    words: 'Volume eight steps', when: writes,
  },
  {
    id: 'dial-home', key: 'Home', zone: 'sv-knob', words: 'Volume to its lowest', when: writes,
  },
  {
    id: 'slider-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: 'sv-slider',
    words: 'The slider one step', when: writes,
  },
  {
    id: 'slider-home', key: 'Home', zone: CENTRED, words: 'Back to the centre', when: writes,
  },
  {
    id: 'arrange-move', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '.arr-handle',
    words: 'Move the section', when: (ctx) => ctx.arranging !== false,
  },
  {
    id: 'arrange-leave', key: 'Esc', keys: ['Escape'], zone: '.view.arranging',
    words: 'Leave Arrange mode and put the order back', when: (ctx) => ctx.arranging !== false,
    own: true,
  },
  {
    id: 'band-write', key: 'Enter', zone: '#part-bands .bedit .v', words: 'Apply the typed value',
    when: writes,
  },
  {
    id: 'band-back', key: 'Esc', keys: ['Escape'], zone: '#part-bands .bedit .v',
    words: 'Put the old value back', when: writes,
  },
  {
    id: 'cell-write', key: 'Enter', zone: '.pts-table .cell', words: 'Apply the typed value',
    when: writes,
  },
  {
    id: 'cell-back', key: 'Esc', keys: ['Escape'], zone: '.pts-table .cell',
    words: 'Put the old value back', when: writes,
  },
  {
    id: 'curve-end', key: 'Esc', keys: ['Escape'], zone: '.eqg .plot',
    words: 'Let go of the point where it is', own: true,
    when: (ctx) => ctx.canWrite === true && ctx.held !== false,
  },
];

/** What a person calls each zone of the table: the headings of the Help popup. Two zones that
 * can sit on one element (a slider, and a slider with a centre) have one name. */
export const ZONES = {
  '.eqg .plot': 'The tone curve',
  'sv-knob': 'The volume dial',
  'sv-slider': 'A slider',
  [CENTRED]: 'A slider',
  '.arr-handle': 'A section\'s handle',
  '.view.arranging': 'Arrange mode',
  '#part-bands .bedit .v': 'A band\'s typed value',
  '.pts-table .cell': 'A cell of the points\' table',
};
