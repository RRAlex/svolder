/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The ear: what the mood reads from the music. Loud and quiet listening both span the range, because
// a level is read against the loudest of the last minute. Everything is eased by a time constant in
// seconds and counted in seconds, never in frames, so the same music gives the same mood at any
// screen rate. A pure module, fed each draw with the bands and the time since the last draw.

import { BANDS } from '../../core/viz-model.js';

/** The ear's numbers, in seconds and 0 to 1 levels. */
export const EAR = Object.freeze({
  /** The loudest level falls away with this time constant, and never under `peakFloor`. */
  peakFall: 60,
  peakFloor: 0.05,
  /** The time constants of the medium and the long view of the level relative to that peak. */
  med: 1.2,
  long: 8,
  /** Where the ear starts, so a page that has just opened looks as the scene always has until the
   * music says otherwise, and opens on no drop. */
  start: 0.7,
  /** The bands the balance weighs: the first `bassBands` against the last `highBands`. */
  bassBands: 48,
  highBands: 80,
  /** The balance's time constant and where it starts (even), and the level under which the music
   * counts as silent and the balance is held. */
  balance: 6,
  balanceStart: 0.45,
  silent: 0.02,
  /** A moment: the medium view rises this far over the long one while over `momentMin`, and then
   * not again for `momentGap` seconds. */
  momentRise: 0.2,
  momentMin: 0.6,
  momentGap: 15,
  /** A peak: the medium view over `loud` for `emberAfter` seconds brings the embers, which ease
   * with `emberTau`; the count of seconds falls at twice the rate while it is under. */
  loud: 0.78,
  emberAfter: 6,
  emberTau: 1.5,
});

/** Ease `x` toward `target` over `dt` seconds with the time constant `tau`. */
export const ease = (x, target, dt, tau) => x + (target - x) * (1 - Math.exp(-dt / tau));

const mean = (units, from, to) => {
  let sum = 0;
  for (let i = from; i < to; i++) sum += units[i];
  return sum / (to - from);
};

/**
 * The ear. `step(dt, units)` takes the bands' levels (0 to 1, `BANDS` of them, the ridge's units)
 * and the seconds since the last step, and returns the state:
 * `{ level, peak, rel, med, long, balance, moment, peakHeld, embers }`. `level` is the mean band
 * level, `peak` the loudest of the last minute and `rel` the level against it (0 to 1); `med` and
 * `long` are `rel` eased over 1.2 and 8 s (times `slow`, which Calm makes 2: half the rate of change; the
 * balance too); `balance` is the highs against the bass (0 for all bass, 1 for all highs, 0.5 for even); `moment` is true on the step a drop arrives; `peakHeld` counts
 * the seconds with `med` over 0.78 and `embers` eases to 1 after 6 of them. With `moments` false (Still) no moment
 * is found, so none is spent. `read()` is the last
 * state, `clear()` starts again.
 */
export function createEar() {
  const fresh = () => ({
    level: 0, peak: EAR.peakFloor, rel: EAR.start, med: EAR.start, long: EAR.start, balance: EAR.balanceStart,
    moment: false, peakHeld: 0, embers: 0,
  });
  let state = fresh();
  let clock = 0;
  let momentAt = -Infinity;

  return {
    step(dt, units, slow = 1, moments = true) {
      const s = state;
      clock += dt;
      const level = mean(units, 0, BANDS);
      s.level = level;
      s.peak = Math.max(level, s.peak * Math.exp(-dt / EAR.peakFall), EAR.peakFloor);
      s.rel = Math.min(1, Math.max(0, level / s.peak));
      s.med = ease(s.med, s.rel, dt, EAR.med * slow);
      s.long = ease(s.long, s.rel, dt, EAR.long * slow);
      if (level >= EAR.silent) {
        const highs = mean(units, BANDS - EAR.highBands, BANDS);
        const bass = mean(units, 0, EAR.bassBands);
        s.balance = ease(s.balance, highs / (highs + 0.9 * bass + 1e-3), dt, EAR.balance * slow);
      }
      s.moment = moments && s.med - s.long > EAR.momentRise && s.med > EAR.momentMin
        && clock - momentAt > EAR.momentGap;
      if (s.moment) momentAt = clock;
      s.peakHeld = s.med > EAR.loud ? s.peakHeld + dt : Math.max(0, s.peakHeld - 2 * dt);
      s.embers = ease(s.embers, s.peakHeld > EAR.emberAfter ? 1 : 0, dt, EAR.emberTau);
      return s;
    },

    read: () => state,

    clear() {
      state = fresh();
      clock = 0;
      momentAt = -Infinity;
    },
  };
}
