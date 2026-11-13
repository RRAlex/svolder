/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Device fold-out at the foot of a device's screen: whether it is open (kept in this
// browser's storage, if the browser allows it), the visualizer's delay row it holds for playing,
// and *Device settings…* (*Devices…* for a reader) at its foot, which opens the device's page in
// the Devices popup. The theme draws the whole part (draw/parts.js); the device's facts and
// readings are drawn by the core, on that page.

import { ember, mine } from './bind.js';

/** The storage key the fold-out's open state is kept under. */
export const KEY = 'svolder.facts';

/** The fold-out as the page holds it now, or nothing. */
const fold = () => document.querySelector('#part-facts .device-facts');

/** The state this page last kept, for a browser that refuses storage. */
let current = null;

/** Whether the fold-out was left open: what this page last kept, else what storage holds. */
function wasOpen() {
  if (current !== null) return current;
  try {
    return window.localStorage.getItem(KEY) === 'open';
  } catch {
    return false;
  }
}

/** Keep the fold-out's state, in memory and where the browser lets it. */
function keep(open) {
  current = open;
  try {
    window.localStorage.setItem(KEY, open ? 'open' : 'closed');
  } catch {
    // The page works without it.
  }
}

/** The visualizer's delay behind the sound in milliseconds, or null while the card does not run:
 * the page measures it, so it is a row the page adds to the part and keeps across its swaps. */
let vizDelay = null;

/** Write the delay row into the fold-out's list, or take it out
 * when the card does not run. */
function paintDelay() {
  const facts = document.querySelector('#part-facts .facts');
  if (!facts) return;
  let row = facts.querySelector('[data-viz-delay]');
  if (vizDelay === null) {
    row?.remove();
    return;
  }
  if (!row) {
    row = document.createElement('dl');
    row.className = 'readout';
    row.dataset.vizDelay = '';
    row.append(document.createElement('dt'), document.createElement('dd'));
    row.firstChild.textContent = 'Visualizer delay';
    facts.append(row);
  }
  const text = `${vizDelay} ms`;
  if (row.lastChild.textContent !== text) row.lastChild.textContent = text;
}

/** The visualizer says how far behind the sound its frames are (`ms`, rounded here), or null when
 * the card has stopped. */
export function setVizDelay(ms) {
  vizDelay = ms === null || ms === undefined ? null : Math.round(ms);
  paintDelay();
}

/** The fold-out is in place (first paint, a switch, a redrawn screen): open it if it was left
 * open. */
export function syncFacts() {
  const details = fold();
  if (details && wasOpen()) details.open = true;
  paintDelay();
}

/** Listen for the fold-out opening and closing, for its part's swaps and for the foot;
 * the listeners go with the mount's signal. */
export function installFacts() {
  // `toggle` does not bubble: it is caught on the way down.
  document.addEventListener('toggle', (event) => {
    if (!event.target.matches?.('.device-facts')) return;
    keep(event.target.open);
  }, mine({ capture: true }));
  // A part that comes in over an open fold-out comes in open, so nothing blinks or jumps.
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (event.detail.target?.id !== 'part-facts') return;
    const next = event.detail.fragment?.querySelector?.('.device-facts');
    if (next && event.detail.target.querySelector('.device-facts')?.open) next.open = true;
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id !== 'part-facts') return;
    syncFacts();
  }, mine());
  // *Device settings…* opens the Devices popup at the page of the device the screen is about.
  document.addEventListener('click', (event) => {
    const button = event.target.closest?.('[data-device-settings]');
    if (!button) return;
    const url = button.closest('.view')?.dataset.device;
    if (url) ember.core.popups.openDevice(url);
  }, mine());
}
