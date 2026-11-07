/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Night drive's motion levels: Full (the screen's rate, the road and the sky's events), Calm (the
// road and the events stopped, the mountains following the music slowly, ten frames a second) and
// Still (about one frame a second).

/** The levels, in the menu's order. */
export const MOTIONS = ['full', 'calm', 'still'];

/** The least ms between draws at each level (0 is every frame). */
const BASE_PACE = { full: 0, calm: 100, still: 1000 };

/** The least ms between draws while the music is silent or the player is not playing, on a phone (a coarse
 * pointer): a desktop has no battery to save, and its scene keeps its level's own pace. The road, the embers and the
 * sky go on moving then (Full), so the pace is one a moving scene still looks smooth at, 30 a second; Calm and Still
 * are slower than that already, and nothing in them rolls. */
export const IDLE_PACE = 1000 / 30;

/** The time constant of the mountains in Calm, in seconds. */
export const CALM_RIDGE = 0.6;

/** The level in use: the kept one, else Calm under reduced motion and Full otherwise. */
export function motionOf(prefs, reduced) {
  const kept = prefs?.get('motion', null);
  if (MOTIONS.includes(kept)) return kept;
  return reduced ? 'calm' : 'full';
}

/** The least ms between two draws at `motion`; on a `coarse` pointer a silent or paused scene is drawn at 30 a second at most. */
export function paceOf({
  motion, silent, playing, coarse = false,
}) {
  const base = BASE_PACE[motion] ?? 0;
  return coarse && (silent || !playing) ? Math.max(base, IDLE_PACE) : base;
}

/** Whether a frame the screen offers is drawn under a pace of `paceMs` (a pace the loop does not time itself: under
 * 100 ms): the one whose time is nearest the pace's, so a screen at 72 Hz draws every second frame (36 a second)
 * and not every third, whatever the screen's rate. `sinceMs` is the time since the last draw, `frameMs` the time
 * since the last frame was offered. */
export const keepFrame = ({
  sinceMs, frameMs, paceMs, atMost = false,
}) => !(paceMs > 0) || (atMost ? sinceMs >= paceMs * AT_MOST : sinceMs + frameMs / 2 >= paceMs);

/** `atMost` keeps a pace as a ceiling: a draw once 90 % of the pace has passed (frames that jitter by a
 * millisecond and a half still fall on the pace), never the nearest frame, which on a 144 Hz screen is 72 a second. */
export const AT_MOST = 0.9;
