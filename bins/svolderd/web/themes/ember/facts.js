/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Device fold-out at the foot of a device's screen: whether it is open (kept in this
// browser's storage, if the browser allows it), the ages the part carries as stamps (worked out
// here against the daemon's clock, so a part that only grew older is never sent again) and the
// Identify button. The theme draws the whole part (draw/parts.js); nothing here draws a fact.

import { ember, mine, shown } from './bind.js';
import { ageWords, signalText } from '../../core/words.js';

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

/** When each part of the fold-out was first looked at here, in this page's own clock: the
 * daemon's clock the part carries, plus the time since, is the daemon's clock now. */
const seen = new WeakMap();

/** Note when `root` came in, unless it has been noted. */
function arrived(root) {
  if (!seen.has(root)) seen.set(root, performance.now());
}

/** Write every age of the fold-out in place; nothing while it is closed (opening it does). */
export function tick() {
  const root = document.getElementById('part-facts');
  if (!root || !fold()?.open) return;
  arrived(root);
  const now = Number(root.dataset.now) + (performance.now() - seen.get(root));
  const put = (element, text) => {
    if (element.textContent !== text) element.textContent = text;
  };
  const secs = (stamp) => (now - Number(stamp)) / 1000;
  for (const cell of root.querySelectorAll('td.age[data-heard-at]')) {
    put(cell, `${ageWords(secs(cell.dataset.heardAt))} ago`);
  }
  const signal = root.querySelector('[data-signal][data-heard-at]');
  if (signal) {
    put(signal, signalText(signal.dataset.head, 'live' in signal.dataset,
      secs(signal.dataset.heardAt)));
  }
}

/** The visualizer's delay behind the sound in milliseconds, or null while the card does not run:
 * the page measures it, so it is a row the page adds to the part and keeps across its swaps. */
let vizDelay = null;

/** Write the delay row into the fold-out's readings (last, after the daemon's), or take it out
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
 * open, and age it. */
export function syncFacts() {
  const root = document.getElementById('part-facts');
  if (root) arrived(root);
  const details = fold();
  if (details && wasOpen()) details.open = true;
  paintDelay();
  tick();
}

/** How many times the stream has swapped the part in; every swap is newer than a read begun
 * before it. */
let swaps = 0;

/** The part as the page holds it now: the element in place and the swaps so far. */
const placed = () => ({ root: document.getElementById('part-facts'), swaps });

/**
 * Whether a read of the part begun at `began` has been overtaken: the stream swapped the part in
 * meanwhile, or a redrawn screen put another one in its place. What it brings is older than what
 * the page holds, so it is dropped.
 */
export function overtaken(began, now) {
  return began.root !== now.root || began.swaps !== now.swaps;
}

/**
 * Read the device's facts afresh and put them in place, open: the part is sent again only when a
 * value changes, so what was left on the page may be old when someone looks. A fold-out that was
 * closed meanwhile, a device switched, or a read that a newer swap overtook is left alone; an
 * ended sign-in loads the sign-in page, as every other read does. The read is the model's, of
 * which the facts are the part published.
 */
async function refresh() {
  const name = shown();
  if (!name) return;
  const began = placed();
  const { core, signal } = ember;
  const answer = await core.model.read(name);
  if (signal.aborted || answer.status !== 200 || !answer.device?.facts) return;
  const root = document.getElementById('part-facts');
  if (!root || !fold()?.open || shown() !== name || overtaken(began, placed())) return;
  core.model.applyPart(name, 'facts', answer.device.facts);
}

/** Listen for the fold-out opening and closing, for its part's swaps, for the clock and for the
 * Identify button; the listeners go with the mount's signal. */
export function installFacts() {
  // `toggle` does not bubble: it is caught on the way down.
  document.addEventListener('toggle', (event) => {
    if (!event.target.matches?.('.device-facts')) return;
    const open = event.target.open;
    // Only a change of state is the person's: the page opening it again after a swap is not.
    const changed = open !== wasOpen();
    keep(open);
    if (open && changed) refresh();
    tick();
  }, mine({ capture: true }));
  // A part that comes in over an open fold-out comes in open, so nothing blinks or jumps.
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (event.detail.target?.id !== 'part-facts') return;
    const next = event.detail.fragment?.querySelector?.('.device-facts');
    if (next && event.detail.target.querySelector('.device-facts')?.open) next.open = true;
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id !== 'part-facts') return;
    swaps++;
    syncFacts();
  }, mine());
  const timer = setInterval(tick, 1000);
  ember.signal?.addEventListener('abort', () => clearInterval(timer));
  document.addEventListener('visibilitychange', tick, mine());
  // Identify says what became of it, with the gap the device's facts carry.
  document.addEventListener('click', (event) => {
    const button = event.target.closest?.('[data-identify]');
    if (!button || button.getAttribute('aria-disabled') === 'true') return;
    ember.core.actions.identify();
  }, mine());
}
