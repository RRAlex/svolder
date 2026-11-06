/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the scene's draws have cost and how often they come: the times of the last 60 and the draws
// of the last second, from the clock handed in. No DOM here.

/** The draws the mean and the 95th percentile are taken over, and the span the rate is counted over (ms). */
export const STATS = Object.freeze({ draws: 60, spanMs: 1000 });

/** The draws' record: `add(ms, atMs)` for each draw (its time and the clock), `read(atMs)` for
 * `{ fps, mean, p95 }`: draws in the last second (measured, not asked), the mean and the 95th percentile of the
 * last 60 times in ms. */
export function createStats() {
  const times = [];
  const stamps = [];

  const trim = (now) => {
    while (stamps.length && stamps[0] < now - STATS.spanMs) stamps.shift();
  };

  return {
    add(ms, at) {
      times.push(ms);
      if (times.length > STATS.draws) times.shift();
      stamps.push(at);
      trim(at);
    },

    read(now) {
      trim(now);
      if (!times.length) return { fps: 0, mean: 0, p95: 0 };
      const sorted = [...times].sort((a, b) => a - b);
      return {
        fps: (stamps.length * 1000) / STATS.spanMs,
        mean: times.reduce((a, b) => a + b, 0) / times.length,
        p95: sorted[Math.ceil(0.95 * sorted.length) - 1],
      };
    },
  };
}
