/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the scene may cost a phone: the wet road, the density its canvases are drawn at, capped at once,
// and a step-down on slow draws that drops the wet road first, then lowers the density, then the rate,
// and brings them back in the other order after a good while.
// Two things step it down: a slow mean draw time, and too few draws a second while the scene asks for
// every frame (a page slow in compositing or in input handling draws fast and seldom). A phone starts at
// the rate's cap. Time-based, from the draws' own times and the clock handed in; no DOM here.

/** The budget's numbers. `dpr`, `megapixels`: the most density and backing pixels a canvas has (the
 * backing store is scaled down, the CSS size is not). `window`: how many draws the mean is taken
 * over. `slowMs`: a mean over it steps down; `calmMs`: a mean under it counts as comfortable.
 * `step`: the factor each step lowers the scale by (it stops at 1 backing px per CSS px). `fps`: the
 * most draws a second once at the floor and still slow. `phoneFps`: the most draws a second on a phone (a coarse
 * pointer) that has come up from `fps`, whatever its screen's rate. `upAfterS`: how long comfortable it stays
 * before a step back up. `gapS`: a pause between two draws this long (a hidden tab) restarts the
 * window and the clock of the way back up, which are not evidence of anything. `minFps`, `lowForS`: fewer
 * draws a second than this, for this long, while the scene asks for every frame, steps down as a slow
 * mean does (at the cap, under 0.7 of the cap's own rate). `heldS`: how long after that the way back up stays shut, since drawing was not what was slow.
 * `wetHeldS`: how long the wet road, the costliest layer, stays off once a step-down has dropped it, so that
 * it does not come and go with every minute of good draws. While the scene is idle (silent or
 * paused: `setIdle`) the wet road comes back as soon as the draws measure under `calmMs`, with no wait, unless it
 * was dropped while idle too; if it pushes the draws over `slowMs` a step-down drops it again, as at any other time. */
export const BUDGET = Object.freeze({
  dpr: 2,
  megapixels: 2.5,
  window: 60,
  slowMs: 12,
  calmMs: 6,
  step: 0.75,
  fps: 30,
  phoneFps: 60,
  upAfterS: 10,
  gapS: 3,
  minFps: 24,
  lowForS: 3,
  heldS: 60,
  wetHeldS: 180,
});

/** The least ms between draws: `asked`, held to the cap's pace while `capped` and, for a `coarse` pointer
 * (a phone) that is not capped, to the phone's pace. */
export function paceUnder({ asked, capped, coarse }) {
  if (capped) return Math.max(asked, 1000 / BUDGET.fps);
  return coarse ? Math.max(asked, 1000 / BUDGET.phoneFps) : asked;
}

/** The scale a canvas of `width` by `height` CSS px is drawn at on a screen of density `density`,
 * before any step: at most `BUDGET.dpr` and at most `BUDGET.megapixels` backing pixels. */
export function baseScale(width, height, density) {
  const area = Math.max(1, width * height);
  return Math.min(BUDGET.dpr, density, Math.sqrt((BUDGET.megapixels * 1e6) / area));
}

/**
 * The step-down for a scene: `size(width, height, density)` once the canvases' CSS size is known,
 * `sample(ms, seconds, full)` with each draw's time, the clock and whether the scene asks for every
 * frame, and then `scale`, `capped` and `step` (how many steps down, 0 for none). `sample` returns
 * true when the scale changed, and the caller sizes the canvases again. `coarse` (a finger for a
 * pointer: a phone) starts at the rate's cap, which the way back up lifts after 10 s under budget.
 */
export function createBudget({ coarse = false } = {}) {
  let base = 1;
  let step = 0;
  let capped = coarse;
  let wet = !coarse;
  let wetHeldUntil = -Infinity;
  let times = [];
  let stamps = [];
  let calmSince = null;
  let lowSince = null;
  let freshAt = null;
  /** Whether the music is silent or the player not playing. */
  let idle = false;
  /** Whether the wet road was last dropped while idle: then the idle way back is shut too. */
  let droppedIdle = false;
  let heldUntil = -Infinity;
  let last = null;

  const floor = () => Math.min(1, base);
  const scaleAt = (n) => Math.max(floor(), base * BUDGET.step ** n);
  const mean = () => times.reduce((a, b) => a + b, 0) / times.length;

  /** One step down (the wet road first, the rate's cap once the scale is at its floor); whether the
   * scale changed. */
  function lower(seconds) {
    calmSince = null;
    lowSince = null;
    times = [];
    stamps = [];
    freshAt = seconds;
    if (wet) {
      wet = false;
      droppedIdle = idle;
      wetHeldUntil = seconds + BUDGET.wetHeldS;
      return false;
    }
    if (scaleAt(step) > floor() + 1e-9) {
      step++;
      return true;
    }
    capped = true;
    return false;
  }

  return {
    size(width, height, density) {
      base = baseScale(width, height, density);
    },

    get scale() { return scaleAt(step); },
    get capped() { return capped; },
    get step() { return step; },
    get wet() { return wet; },

    /** Whether the scene is idle (silent or paused) now. A change starts the measuring again, so the draws it
     * measures are the idle ones. */
    setIdle(quiet) {
      if (quiet === idle) return;
      idle = quiet;
      times = [];
      calmSince = null;
    },

    sample(ms, seconds, full = false) {
      if (last !== null && seconds - last > BUDGET.gapS) {
        times = [];
        stamps = [];
        calmSince = null;
        lowSince = null;
        freshAt = null;
      }
      last = seconds;
      freshAt ??= seconds;
      stamps.push(seconds);
      while (stamps.length && stamps[0] < seconds - 1) stamps.shift();
      times.push(ms);
      // too few draws a second for long enough, while every frame is asked for
      if (full && seconds - freshAt >= 1 && stamps.length < (capped ? 0.7 * BUDGET.fps : BUDGET.minFps)) lowSince ??= seconds;
      else lowSince = null;
      if (lowSince !== null && seconds - lowSince >= BUDGET.lowForS) {
        heldUntil = seconds + BUDGET.heldS;
        return lower(seconds);
      }
      if (times.length < BUDGET.window) return false;
      times = times.slice(-BUDGET.window);
      const average = mean();
      if (average > BUDGET.slowMs) return lower(seconds);
      if (average >= BUDGET.calmMs) {
        calmSince = null;
        return false;
      }
      if (idle && !wet && !droppedIdle) {
        // the idle scene is cheap to draw: the wet road is tried at once, and the step-down drops it if it is not
        wet = true;
        times = [];
        return false;
      }
      calmSince ??= seconds;
      if (seconds - calmSince < BUDGET.upAfterS || seconds < heldUntil) return false;
      calmSince = null;
      times = [];
      if (capped) {
        capped = false;
        return false;
      }
      if (step > 0) {
        step--;
        return true;
      }
      if (!wet && seconds >= wetHeldUntil) {
        wet = true;
        droppedIdle = false;
      }
      return false;
    },
  };
}
