/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Paper's key table (core/keys.js says what an entry is): every key Paper answers to,
// as data. Paper binds no key of the whole page: each entry names the widget that handles the
// key itself (the needle, a printed scale, a point of the graph, a cell of the table, a paper
// list, a question), so none has a `run`. Pure, with no DOM.
//
// The one fact a `when` reads is the core's `canWrite`: a viewer who may not write has the keys
// that move the view and none that changes a device.

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
const PAGES = ['PageUp', 'PageDown'];
const writes = (ctx) => ctx.canWrite === true;

export const KEYS = [
  // ---- the needle of the volume
  {
    id: 'needle-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '#p-needle',
    words: 'Volume one step', when: writes,
  },
  {
    id: 'needle-page', key: ['Page Up', 'Page Down'], keys: PAGES, zone: '#p-needle',
    words: 'Volume ten steps', when: writes,
  },
  {
    id: 'needle-home', key: 'Home', zone: '#p-needle', words: 'Volume to its lowest',
    when: writes,
  },

  // ---- a printed scale
  {
    id: 'scale-step', key: ['↑', '↓', '←', '→'], keys: ARROWS, zone: '.p-scale',
    words: 'The scale one step', when: writes,
  },
  {
    id: 'scale-page', key: ['Page Up', 'Page Down'], keys: PAGES, zone: '.p-scale',
    words: 'The scale ten steps', when: writes,
  },
  {
    id: 'scale-home', key: 'Home', zone: '.p-scale', words: 'The scale to its lowest',
    when: writes,
  },

  // ---- a point of the graph
  {
    id: 'point-frequency', key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'], zone: '.pt',
    words: 'Frequency one step', when: writes,
  },
  {
    id: 'point-gain', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.pt',
    words: 'Gain one step', when: writes,
  },

  // ---- a cell of the points' table
  {
    id: 'cell-write', key: 'Enter', zone: '.p-cell', words: 'Apply the typed value',
    when: writes,
  },
  {
    id: 'cell-back', key: 'Esc', keys: ['Escape'], zone: '.p-cell', words: 'Put the old value back',
    when: writes,
  },
  {
    id: 'cell-step', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.p-cell',
    words: 'The value one step', when: writes,
  },

  // ---- a paper list a word opens
  {
    id: 'list-move', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.plist',
    words: 'The option above or below',
  },
  {
    id: 'list-ends', key: ['Home', 'End'], zone: '.plist',
    words: 'The first or the last option',
  },
  { id: 'list-pick', key: 'Enter', zone: '.plist', words: 'Choose the option' },
  {
    id: 'list-close', key: 'Esc', keys: ['Escape'], zone: '.plist', words: 'Close the list',
  },

  // ---- a name asked for, a question asked once
  {
    id: 'ask-close', key: 'Esc', keys: ['Escape'], zone: '.p-askbox',
    words: 'Cancel and change nothing',
  },
];

/** What a person calls each zone of the table: the headings of the Help popup. */
export const ZONES = {
  '#p-needle': 'The volume needle',
  '.p-scale': 'A scale',
  '.pt': 'A point of the graph',
  '.p-cell': 'A cell of the points\' table',
  '.plist': 'A list',
  '.p-askbox': 'A question',
};
