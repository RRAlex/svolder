/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone curve's motion: the curve eases from the old values to the new, the changed band
// values flash and the lamp of a newly lit preset rings. ember.js calls `installToneMotion` where
// its listeners stand; `once` plays a one-shot class on an element, for the page's other flashes.

import { mine } from './bind.js';

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

const TWEEN_MS = { recall: 480, mode: 320, step: 180 };
let tweenToken = 0;
// The values on screen while a tween runs, so a swap that lands mid-way eases on from them.
let tweenNow = null;
// Puts the curve in its final shape: set while a tween runs, so `settle` can end it.
let landing = null;
// When a preset's button was last pressed: its answer swaps only the tone part, and the lamp
// follows with the stream's next parts, so the press itself says the curve is a recall.
let recalledAt = -Infinity;

/** The y values a `data-y` attribute holds, or none. */
const ysOf = (text) => (text ? text.split(',').map(Number).filter(Number.isFinite) : []);

/** Ease the curve of the tone part just swapped in from `from` to the values it carries. */
function tweenCurve(from, ms) {
  const line = document.querySelector('#part-tone .eqg .line');
  const area = document.querySelector('#part-tone .eqg .area');
  const to = ysOf(line?.dataset.y);
  // A mode with no line (Loudness) has no values: nothing is drawn and nothing is tweened to.
  if (!line || !from.length || from.length !== to.length) return false;
  const finalLine = line.getAttribute('d');
  const finalArea = area?.getAttribute('d');
  const xs = [...finalLine.matchAll(/[ML]([\d.]+) [\d.]+/g)].map((m) => m[1]);
  if (xs.length !== to.length || from.every((v, i) => v === to[i])) return false;
  const token = ++tweenToken;
  const start = performance.now();
  const set = (d, a) => {
    line.setAttribute('d', d);
    area?.setAttribute('d', a);
  };
  landing = () => set(finalLine, finalArea);
  const step = (now) => {
    if (token !== tweenToken) return;
    const t = clamp((now - start) / ms, 0, 1);
    const ease = 1 - (1 - t) ** 3;
    tweenNow = from.map((v, i) => v + (to[i] - v) * ease);
    const d = xs.map((x, i) => `${i ? 'L' : 'M'}${x} ${tweenNow[i].toFixed(1)}`).join('');
    set(d, `${d}L1000.0 130.0L0.0 130.0Z`);
    if (t < 1) requestAnimationFrame(step);
    else {
      tweenNow = null;
      landing = null;
    }
  };
  requestAnimationFrame(step);
  // Frames stop in a background tab; the timer lands the final values.
  setTimeout(() => {
    if (token !== tweenToken) return;
    set(finalLine, finalArea);
    tweenNow = null;
    landing = null;
  }, ms + 40);
  return true;
}

/** End a curve tween that is running, the curve at its final shape: what draws the curve itself
 * (a drag) takes over from there, and nothing eases or lands over it later. */
export function settle() {
  tweenToken++;
  landing?.();
  landing = null;
  tweenNow = null;
}

/** Play `class` once on `element`: it comes off when the animation ends, or after 1.2 s. */
export function once(element, name) {
  if (!element) return;
  element.classList.remove(name);
  void element.getBoundingClientRect();
  element.classList.add(name);
  const done = () => element.classList.remove(name);
  element.addEventListener('animationend', done, { once: true });
  setTimeout(done, 1200);
}

/** What the tone part, the band table and the preset list show, for the next swap to compare. */
function look() {
  const line = document.querySelector('#part-tone .eqg .line');
  return {
    ys: tweenNow ?? ysOf(line?.dataset.y),
    mode: document.querySelector('#part-tone .seg .opt[aria-pressed="true"]')?.dataset.mode,
    preset: document.querySelector('.pre[aria-pressed="true"]')?.dataset.name,
    cells: [...document.querySelectorAll('#part-bands .band span')].map((c) => c.textContent),
  };
}

let noted = null;

/**
 * Note what the page shows before the first swap of a tone, band or preset part; once the swaps
 * that were under way have all landed (a microtask later) [`landed`] compares and animates.
 */
export function note() {
  if (noted) return;
  noted = look();
  queueMicrotask(landed);
}

/** The curve eases, the changed band values flash, the lamp of a newly lit preset rings. */
function landed() {
  const before = noted;
  noted = null;
  const now = look();
  const recalled = now.preset !== before.preset || performance.now() - recalledAt < 2500;
  const ms = recalled ? TWEEN_MS.recall : now.mode !== before.mode ? TWEEN_MS.mode : TWEEN_MS.step;
  if (tweenCurve(before.ys, ms)) recalledAt = -Infinity;
  const cells = document.querySelectorAll('#part-bands .band span');
  if (before.cells.length === cells.length) {
    cells.forEach((cell, i) => {
      if (cell.textContent !== before.cells[i]) once(cell, 'flash');
    });
  }
  if (now.preset && now.preset !== before.preset) {
    once(document.querySelector('.pre[aria-pressed="true"] .lamp'), 'ring');
  }
}

// The parts the curve, the band values and the lamps live in are noted before they swap.
const NOTED = ['part-tone', 'part-bands', 'part-presets'];

/** Attach the swap listeners that note the curve before a swap, and the press that marks a
 * recall. */
export function installToneMotion() {
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (NOTED.includes(event.detail.target?.id)) note();
  }, mine());
  document.addEventListener('click', (event) => {
    if (event.target.closest('.pre[data-json]')) recalledAt = performance.now();
  }, mine());
}
