/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Night drive's key table (core/keys.js says what an entry is): every key Night drive
// answers to, as data. One key is the whole page's: `↑` and `↓` step the volume from the scene
// (volume-keys.js; nightdrive.js gives the entry its `run` by its `id` and binds the table). The
// rest name the widget that handles the key itself: a scale of the sheet, a typed value, a list,
// a question, the sheet. Pure, with no DOM.
//
// The one fact a `when` reads is the core's `canWrite`: a viewer who may not write has the keys
// that move the view and none that changes a device, the volume's arrows included.

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
const PAGES = ['PageUp', 'PageDown'];
const writes = (ctx) => ctx.canWrite === true;

export const KEYS = [
  // ---- the page's key
  {
    id: 'volume', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: 'page',
    words: 'Volume one step', repeat: true, when: writes,
  },

  // ---- a scale of the sheet
  {
    id: 'scale-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '#nd-controls .nd-scale',
    words: 'The scale one step', when: writes,
  },
  {
    id: 'scale-page', key: ['Page Up', 'Page Down'], keys: PAGES, zone: '#nd-controls .nd-scale',
    words: 'The scale ten steps', when: writes,
  },
  {
    id: 'scale-home', key: 'Home', zone: '#nd-controls .nd-scale', words: 'The scale to its lowest',
    when: writes,
  },

  // ---- a value typed in place
  {
    id: 'value-write', key: 'Enter', zone: 'input.nd-v', words: 'Apply the typed value',
    when: writes,
  },
  {
    id: 'value-back', key: 'Esc', keys: ['Escape'], zone: 'input.nd-v',
    words: 'Put the old value back', when: writes,
  },

  // ---- a list a chip opens
  {
    id: 'list-move', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.nd-list',
    words: 'The option above or below',
  },
  {
    id: 'list-ends', key: ['Home', 'End'], zone: '.nd-list',
    words: 'The first or the last option',
  },
  { id: 'list-pick', key: 'Enter', zone: '.nd-list', words: 'Choose the option' },
  { id: 'list-close', key: 'Esc', keys: ['Escape'], zone: '.nd-list', words: 'Close the list' },

  // ---- a name asked for, a question asked once
  {
    id: 'ask-close', key: 'Esc', keys: ['Escape'], zone: '.nd-askbox',
    words: 'Cancel and change nothing',
  },

  // ---- the device's sheet
  { id: 'sheet-close', key: 'Esc', keys: ['Escape'], zone: '#nd-dialog', words: 'Close the sheet' },
];

/** What a person calls each zone of the table: the headings of the Help popup. */
export const ZONES = {
  '#nd-controls .nd-scale': 'A scale',
  'input.nd-v': 'A typed value',
  '.nd-list': 'A list',
  '.nd-askbox': 'A question',
  '#nd-dialog': 'The device\'s sheet',
};
