/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Signal's key table (core/keys.js says what an entry is): every key Signal answers
// to, as data. Signal binds no key of the whole page: each entry names the widget that handles
// the key itself (the line, the volume's track, a scale, a point of the curve, a typed value, a
// list, a question, a hop's edit form, the Add to the chain sheet), so none has a `run`. Pure,
// with no DOM.
//
// The one fact a `when` reads is the core's `canWrite`: a viewer who may not write has the keys
// that move the view and none that changes a device.

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
const PAGES = ['PageUp', 'PageDown'];
const writes = (ctx) => ctx.canWrite === true;

export const KEYS = [
  // ---- the line: the hops the signal goes through
  {
    id: 'line-move', key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'], zone: '#sg-line',
    words: 'The hop before or after',
  },
  {
    id: 'line-ends', key: ['Home', 'End'], zone: '#sg-line',
    words: 'The first or the last hop',
  },
  {
    id: 'line-open', key: ['Enter', 'Space'], keys: ['Enter', ' '], zone: '#sg-line',
    words: 'Choose the hop',
  },

  // ---- the volume's track, the device's and the computer's
  {
    id: 'volume-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '#sg-vtrack, #sg-cvtrack',
    words: 'Volume one step', when: writes,
  },
  {
    id: 'volume-page', key: ['Page Up', 'Page Down'], keys: PAGES, zone: '#sg-vtrack, #sg-cvtrack',
    words: 'Volume ten steps', when: writes,
  },
  {
    id: 'volume-home', key: 'Home', zone: '#sg-vtrack, #sg-cvtrack',
    words: 'Volume to its lowest', when: writes,
  },

  // ---- a scale of the shelf
  {
    id: 'scale-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '.sg-scale',
    words: 'The scale one step', when: writes,
  },
  {
    id: 'scale-page', key: ['Page Up', 'Page Down'], keys: PAGES, zone: '.sg-scale',
    words: 'The scale ten steps', when: writes,
  },
  {
    id: 'scale-home', key: 'Home', zone: '.sg-scale', words: 'The scale to its lowest',
    when: writes,
  },

  // ---- a point of the curve
  {
    id: 'point-frequency', key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'], zone: '.sg-pt',
    words: 'Frequency one step', when: writes,
  },
  {
    id: 'point-gain', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.sg-pt',
    words: 'Gain one step', when: writes,
  },
  {
    id: 'point-end', key: 'Esc', keys: ['Escape'], zone: '.sg-pt[data-n]',
    words: 'Let go of the point where it is', when: writes,
  },

  // ---- a value typed in place
  {
    id: 'value-write', key: 'Enter', zone: 'input.sg-val, input.sg-pval',
    words: 'Apply the typed value', when: writes,
  },
  {
    id: 'value-back', key: 'Esc', keys: ['Escape'], zone: 'input.sg-val, input.sg-pval',
    words: 'Put the old value back', when: writes,
  },

  // ---- a list a chip opens
  {
    id: 'list-move', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.sg-list',
    words: 'The option above or below',
  },
  {
    id: 'list-ends', key: ['Home', 'End'], zone: '.sg-list',
    words: 'The first or the last option',
  },
  { id: 'list-pick', key: 'Enter', zone: '.sg-list', words: 'Choose the option' },
  { id: 'list-close', key: 'Esc', keys: ['Escape'], zone: '.sg-list', words: 'Close the list' },

  // ---- a name asked for, a question asked once
  {
    id: 'ask-close', key: 'Esc', keys: ['Escape'], zone: '.sg-askbox',
    words: 'Cancel and change nothing',
  },

  // ---- a hop's edit form
  {
    id: 'edit-close', key: 'Esc', keys: ['Escape'], zone: '.sg-editform',
    words: 'Cancel and change nothing', when: writes,
  },
  // ---- the Add to the chain sheet
  {
    id: 'sheet-close', key: 'Esc', keys: ['Escape'], zone: '#sg-sheet-form',
    words: 'Close the sheet and send nothing',
  },
];

/** What a person calls each zone of the table: the headings of the Help popup. Two zones that
 * can sit on one element (a point, and a point the hand holds) have one name. */
export const ZONES = {
  '#sg-line': 'The line',
  '#sg-vtrack, #sg-cvtrack': 'The volume',
  '.sg-scale': 'A scale',
  '.sg-pt': 'A point of the curve',
  '.sg-pt[data-n]': 'A point of the curve',
  'input.sg-val, input.sg-pval': 'A typed value',
  '.sg-list': 'A list',
  '.sg-askbox': 'A question',
  '.sg-editform': 'A hop\'s edit form',
  '#sg-sheet-form': 'The Add to the chain sheet',
};
