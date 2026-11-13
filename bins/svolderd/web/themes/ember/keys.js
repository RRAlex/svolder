/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The desktop keys. They are the theme's key table (key-table.js): this module gives each key
// of the page its `run` and binds the table through `core.keys.bind`, so a key acts only when
// nothing else owns it (a modal, a text field, a modifier, the frame's menu) and only while its
// `when` holds (none that writes for a guest, with the link down or while the sections are
// being arranged; `v` only where the screen has a visualizer card). The Help popup lists them.
//
// ember.js calls `installKeys` where its listeners stand and gives `TABLE` to the frame as the
// mounted instance's `keys`.

import { ember } from './bind.js';
import { arranging, openArrange } from './arrange.js';
import { dragging as computerHeld } from './computer.js';
import { KEYS } from './key-table.js';
import { dragging as toneHeld } from './tone-graph.js';
import { cycleLook, hasBand } from './visualizer.js';

/** What each key of the page does, by the `id` of its entry in the table. A key that writes names
 * no device: the action's default target, the device on show, is the one. */
const RUNS = {
  /** One step of the volume, unless the focus is on a control that has arrows of its own. */
  volume(event) {
    if (event.target.closest?.('sv-knob, sv-slider, .seg, .presets, .bands, .bedit')) return;
    document.querySelector('sv-knob')?.step(event.key === 'ArrowRight' ? 1 : -1);
    event.preventDefault();
  },
  mute: () => document.querySelector('sv-knob')?.toggleMute(),
  centre: () => ember.core.actions.setControl('balance', { value: 0 }),
  // The look of the visualizer is the page's own, not a write: a guest and a lost link have it.
  look: () => cycleLook(),
  flat() {
    const flat = document.querySelector('#part-tone .opt[data-mode="flat"]');
    if (flat && flat.getAttribute('aria-pressed') !== 'true') ember.core.actions.setMode('flat');
  },
  preset: () => (document.querySelector('.pre[aria-pressed="true"]')
    ?? document.querySelector('.pre'))?.focus(),
  // Arranging is the page's own too.
  arrange: () => openArrange(),
};

/** Ember's key table as it is bound: the entries of key-table.js, each key of the page with its
 * `run`. */
export const TABLE = KEYS.map((entry) => (
  RUNS[entry.id] ? { ...entry, run: RUNS[entry.id] } : entry));

/** What the page shows now, for the table's `when` (key-table.js names each fact). */
function facts() {
  const knob = document.querySelector('sv-knob');
  return {
    lost: document.body.classList.contains('lost'),
    arranging: arranging(),
    held: toneHeld() || computerHeld(),
    viz: hasBand(),
    knob: knob !== null,
    mute: Boolean(knob?.dataset.mute),
    balance: document.querySelector('sv-slider[data-key="balance"]') !== null,
    flat: document.querySelector('#part-tone .opt[data-mode="flat"]') !== null,
    presets: document.querySelector('.pre') !== null,
  };
}

/** Bind the table: the keys of the page act from now on, until the theme is unmounted. */
export function installKeys() {
  ember.core.keys.bind(TABLE, { signal: ember.signal, ctx: facts });
}
