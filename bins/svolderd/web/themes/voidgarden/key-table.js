/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Void Garden's key table (core/keys.js says what an entry is): every key the theme
// answers to, as data. Two keys are the whole page's: `↑` and `↓` step the volume and `A` cycles
// the audio panel (voidgarden.js gives each entry its `run` by its `id` and binds the table). Three
// keys are the scene's, as the dash's explore keys: `S` a new seed, `N` the next scene, `E` an
// effect; none clashes with the frame's (`T`, `?`, Space) or the core's. The frame's own keys (the menu, Help, Space) are the core's. The one fact a `when` reads is the
// core's `canWrite`: `A` changes the view and stays for a viewer who may not write. Pure, no DOM.

const writes = (ctx) => ctx.canWrite === true;

export const KEYS = [
  {
    id: 'volume', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: 'page',
    words: 'Volume one step', repeat: true, when: writes,
  },
  {
    id: 'panel', key: 'A', keys: ['A', 'a'], zone: 'page', words: 'Cycle the audio panel',
  },
  {
    id: 'explore-seed', key: 'S', keys: ['S', 's'], zone: 'page', words: 'New seed: a new world for this track',
  },
  {
    id: 'explore-scene', key: 'N', keys: ['N', 'n'], zone: 'page', words: 'Next scene: another world on the same track',
  },
  {
    id: 'explore-effect', key: 'E', keys: ['E', 'e'], zone: 'page', words: 'Effect: fire one of the scene’s effects',
  },
  {
    id: 'sheet-close', key: 'Esc', keys: ['Escape'], zone: '#vg-dash', words: 'Close the phone\u2019s sheet',
  },
];

/** What a person calls each zone of the table: the headings of the Help popup. */
export const ZONES = { '#vg-dash': 'On the phone\u2019s sheet' };
