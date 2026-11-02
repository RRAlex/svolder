/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Now playing card's page side: the track's elapsed time and bar, counted forward once a
// second from the position the player gave, the transport buttons, and the phone's compact line
// that a tap opens into the full card (kept open across the swaps of its part). The theme draws
// the whole card; nothing here draws a title or a cover. The elapsed time is worked out against
// the daemon's clock, which the part carries as `data-now`, so a browser whose own clock is
// wrong still shows the player's time.
//
// The folded line's title is one line: when it is cut, it scrolls left at a slow steady pace in a
// loop, a copy of the text following it so that the lap wraps without a jump, and rests at its
// start for a moment at each lap; its right edge fades in place of "...". The setting of the
// system for less motion does not stop it (the page's small motions ignore it); opening the line
// does.
//
// The band's card has a fixed height, so a path with more to say than fits (a long verdict, a long
// name, a narrow card) would be cut off at its foot, mid-chip. `fitPath` ends the row of chips with
// a chip of three dots instead: the verdict stays first, the hops that do not fit stay in the page
// for a screen reader but out of sight, and the dots' title lists them.

import { ember, mine } from './bind.js';
import {
  clockOffset, daemonClock, elapsedUs, fraction, timeWords,
} from '../../core/clock.js';

/** A number a `data-` attribute holds, or null when it holds none. */
const number = (text) => (text === undefined || text === '' ? null : Number(text));

/** Whether the phone's compact line is open: kept here, since a swap of the part replaces the
 * element that showed it. */
let open = false;

/** The daemon-clock offset of each part, noted when this page first saw it. */
const offsets = new WeakMap();

/** The part as the page holds it now, or nothing. */
const part = () => document.getElementById('part-playing');

/** Show the compact line open or folded, on the part and for a screen reader. A card with no
 * player has nothing to open: it folds, and the line stays folded when a player returns. */
function show(root) {
  if (root.classList.contains('idle')) open = false;
  root.toggleAttribute('data-expanded', open);
  root.querySelector('.np-fold')?.setAttribute('aria-expanded', String(open));
}

/** The title's loop: its pace in pixels a second, the gap between the text and its copy in pixels
 * and how long each lap waits at its start (ms). */
const PACE = 30;
const GAP = 40;
const REST = 2000;

/**
 * The loop of a title `textWidth` pixels wide: how far a lap travels (the text and the gap, which
 * puts the copy where the text started), how long the move takes at the pace, the whole lap in
 * milliseconds (the rest at the start, then the move) and its keyframes: wait, then move
 * linearly to the end.
 */
export function loopPlan(textWidth) {
  const dist = textWidth + GAP;
  const move = (dist / PACE) * 1000;
  const total = REST + move;
  const x = (px) => `translateX(${-px}px)`;
  return {
    dist, move, total,
    frames: [
      { transform: x(0), offset: 0 },
      { transform: x(0), offset: REST / total },
      { transform: x(dist), offset: 1 },
    ],
  };
}

/** Whether the compact line's fold button is on show: it is on a phone, not on a desktop. */
const folds = (root) => {
  const fold = root.querySelector('.np-fold');
  return fold !== null && fold.getClientRects().length > 0;
};

/** The title while the line is the phone's folded one, else nothing. */
function foldedTitle() {
  const root = part();
  const title = root?.querySelector('.np-title');
  return title?.firstElementChild && folds(root) && !root.hasAttribute('data-expanded')
    ? title : null;
}

/**
 * Mark the folded title `cut` when its text runs past its box (the style then fades its right
 * edge in place of "...") and give it its copy, for the loop to follow the text with; unmark it
 * and take the copy away when it fits, or when the line is open or a desktop's, and stop its
 * loop. Returns how far the text runs past, in pixels (0 when it does not).
 */
export function fitTitle() {
  const root = part();
  const title = root?.querySelector('.np-title');
  const inner = title?.firstElementChild;
  if (!inner) return 0;
  const shown = foldedTitle();
  const over = shown ? inner.getBoundingClientRect().width - shown.clientWidth : 0;
  title.classList.toggle('cut', over > 1);
  let copy = title.querySelector('.np-copy');
  if (over <= 1) {
    copy?.remove();
    title.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    return 0;
  }
  if (!copy) {
    copy = document.createElement('span');
    copy.className = 'np-copy';
    copy.setAttribute('aria-hidden', 'true');
    copy.style.marginLeft = `${GAP}px`;
    title.append(copy);
  }
  if (copy.textContent !== inner.textContent) copy.textContent = inner.textContent;
  return over;
}

/** What names the track on the card: its title and artist. A track that repeats them (a loop, a
 * live set) is the same track to the page: the part does not say more. */
const trackKey = (root) => [...root.querySelectorAll('.np-title > span:first-child, .np-artist')]
  .map((el) => el.textContent).join('\n');

/**
 * Keep the title's loop as the card is: running while the line is folded, the page shows and the
 * title is cut, stopped and at rest in its start otherwise. A loop already running for the title's
 * width is left to go on; a title whose width changed (a resize, a font coming in) or that has none
 * yet starts a lap `from` milliseconds in (0: at its rest).
 */
function syncLoop(from = 0) {
  const over = fitTitle();
  const title = part()?.querySelector('.np-title');
  const inner = title?.firstElementChild;
  if (!inner) return;
  if (over === 0 || document.hidden) {
    title.getAnimations({ subtree: true }).forEach((a) => a.cancel());
    return;
  }
  const plan = loopPlan(inner.getBoundingClientRect().width);
  const going = inner.getAnimations()[0];
  if (going && from === 0 && Math.abs(going.effect.getTiming().duration - plan.total) < 1) return;
  title.getAnimations({ subtree: true }).forEach((a) => a.cancel());
  const timing = { duration: plan.total, iterations: Infinity, easing: 'linear' };
  // Both start by the same clock, so the copy is exactly a gap behind the text all the time.
  const start = document.timeline.currentTime - (from % plan.total);
  for (const el of [inner, title.querySelector('.np-copy')]) {
    el.animate(plan.frames, timing).startTime = start;
  }
}

/** The title's loop in flight on the part about to be swapped out, as `{ key, time }`: a part
 * swapped in for the same track takes the loop up where it was, not from its rest. */
let carried = null;

/** Write the elapsed time and the bar from the track's data, as of now. */
export function tick() {
  if (document.hidden) return;
  const root = part();
  const track = root?.querySelector('.np[data-status]');
  if (!track) return;
  if (!offsets.has(root)) offsets.set(root, clockOffset(Number(root.dataset.now), Date.now()));
  const length = number(track.dataset.length);
  const known = track.dataset.readAt !== undefined;
  const elapsed = known ? elapsedUs({
    status: track.dataset.status,
    position: number(track.dataset.position) ?? 0,
    rate: number(track.dataset.rate) ?? 1000,
    readAt: number(track.dataset.readAt),
    length,
  }, daemonClock(offsets.get(root), Date.now())) : null;
  const put = (element, text) => {
    if (element && element.textContent !== text) element.textContent = text;
  };
  put(root.querySelector('.np-el'), timeWords(elapsed));
  put(root.querySelector('.np-len'), timeWords(length));
  const fill = `${(fraction(elapsed ?? 0, length) * 100).toFixed(1)}%`;
  const bar = root.querySelector('.np-bar');
  if (bar && bar.style.getPropertyValue('--p') !== fill) bar.style.setProperty('--p', fill);
}

/** The room left under the last chip, in pixels, before the card's foot: the chips keep clear of
 * the edge the card clips at. */
const CHIP_FOOT = 12;

/**
 * End the path's row of chips with a chip of three dots when the row does not fit the card: the
 * hops from the end are put out of sight (kept for a screen reader) until the row, the dots
 * included, fits. The verdict, the first chip, always stays. A row that fits is left whole, and a
 * row that fits again (the card grew, the path got shorter) comes back whole.
 */
export function fitPath() {
  const root = part();
  const path = root?.querySelector('.np-path');
  if (!root || !path) return;
  path.querySelector('.np-more')?.remove();
  for (const hop of path.querySelectorAll('.np-cut')) hop.classList.remove('np-cut');
  const limit = () => root.getBoundingClientRect().bottom - CHIP_FOOT;
  const last = () => {
    const shown = [...path.children].filter((chip) => !chip.classList.contains('np-cut'));
    return shown[shown.length - 1];
  };
  const over = () => path.getClientRects().length > 0
    && (last()?.getBoundingClientRect().bottom ?? 0) > limit();
  if (!over()) return;
  const more = document.createElement('span');
  more.className = 'np-more';
  more.setAttribute('aria-hidden', 'true');
  more.textContent = '…';
  path.append(more);
  const hidden = [];
  const hops = [...path.querySelectorAll('.np-hop')];
  while (hops.length > 0 && over()) {
    const hop = hops.pop();
    hop.classList.add('np-cut');
    hidden.unshift(hop.textContent.trim());
    more.title = hidden.join(' → ');
  }
  // The dots say nothing when no hop could be put out of sight, and when every hop is gone and the
  // verdict alone still overflows (nothing is left to name).
  if (hidden.length === 0 || (hops.length === 0 && over())) more.remove();
}

/** Watches the card's size: a path is fitted again when its card is resized. */
let sizing = null;

/** The part is in place (first paint, a switch, a redrawn screen): show it as it was left, fit its
 * path and count its time. */
export function syncPlaying() {
  const root = part();
  if (root) show(root);
  if (!sizing && root && 'ResizeObserver' in window) {
    // An observer that cannot be made leaves the path fitted at each sync only.
    try {
      sizing = new ResizeObserver(() => {
        fitPath();
        syncLoop();
      });
    } catch { sizing = null; }
  }
  sizing?.disconnect();
  if (root) sizing?.observe(root);
  fitPath();
  const same = carried && root?.querySelector('.np-title') && carried.key === trackKey(root);
  syncLoop(same ? carried.time : 0);
  carried = null;
  tick();
}

/** The once-a-second timer, running only while the tab shows. */
let timer = 0;
/** Start the timer when the tab shows, stop it when it does not, and count at once on return. */
function schedule() {
  clearInterval(timer);
  timer = 0;
  if (document.hidden) return;
  timer = setInterval(tick, 1000);
  tick();
}

/** Listen for the transport buttons and the tap that opens or folds the line, for the swaps of
 * the part and for the clock; the listeners and the clock go with the mount's signal. */
export function installPlaying() {
  ember.signal?.addEventListener('abort', () => {
    clearInterval(timer);
    timer = 0;
    sizing?.disconnect();
    sizing = null;
  });
  document.addEventListener('click', (event) => {
    const button = event.target.closest?.('#part-playing [data-press]');
    if (button) {
      ember.core.actions.press(button.dataset.press);
      return;
    }
    const root = part();
    if (!root || !event.target.closest?.('#part-playing .np-head') || !folds(root)) return;
    open = !open;
    show(root);
    syncLoop();
  }, mine());
  // A part that comes in over an open line comes in open, so nothing blinks or jumps.
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (event.detail.target?.id !== 'part-playing') return;
    const lap = part()?.querySelector('.np-title')?.firstElementChild?.getAnimations()[0];
    carried = lap ? { key: trackKey(part()), time: lap.currentTime } : null;
    const next = event.detail.fragment?.querySelector?.('#part-playing');
    if (next?.classList.contains('idle')) open = false;
    next?.toggleAttribute('data-expanded', open);
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id === 'part-playing') syncPlaying();
  }, mine());
  // A cover the daemon no longer holds is not drawn (its box stays: the card does not move).
  document.addEventListener('error', (event) => {
    if (event.target.matches?.('#part-playing img')) event.target.hidden = true;
  }, mine({ capture: true }));
  document.addEventListener('visibilitychange', schedule, mine());
  // The loop stops while the page is hidden and starts again, from its rest, when it shows. A font
  // that comes in later changes the text's width: the loop is fitted again to it.
  document.addEventListener('visibilitychange', () => syncLoop(), mine());
  document.fonts?.addEventListener?.('loadingdone', () => syncLoop(), mine());
  const { signal } = ember;
  document.fonts?.ready?.then(() => {
    if (!signal.aborted) syncLoop();
  });
  schedule();
}
