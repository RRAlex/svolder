/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Now playing card's arithmetic, for any theme: the daemon's clock against this browser's, how
// far into the track it is, the bar's fill and the clock words. Pure: numbers in, numbers or text
// out, so a browser whose own clock is wrong still shows the player's time.

/** Microseconds in a second. */
const SEC = 1_000_000;

/** The daemon's clock minus this browser's, in milliseconds, for a part that said it was
 * `dataNowMs` by the daemon's clock when this browser's own read `localMs`. */
export function clockOffset(dataNowMs, localMs) {
  return dataNowMs - localMs;
}

/** The daemon's clock now, in milliseconds, when this browser's reads `localMs`. */
export function daemonClock(offsetMs, localMs) {
  return localMs + offsetMs;
}

/**
 * How far into the track it is, in microseconds, at `daemonNowMs` by the daemon's clock: the
 * position the player gave plus the time since it was read times the rate (in thousandths of
 * normal speed) while it plays, the position as it was while it does not; never below 0 or past
 * the length, which a player (the fake one) can run on beyond.
 * `track` is `{ status, position, rate, readAt, length }`: µs, thousandths, ms, µs or null.
 */
export function elapsedUs(track, daemonNowMs) {
  const moving = track.status === 'playing' ? Math.max(0, daemonNowMs - track.readAt) : 0;
  // A millisecond at the rate in thousandths is exactly `rate` microseconds.
  const at = track.position + moving * track.rate;
  const capped = track.length ? Math.min(at, track.length) : at;
  return Math.max(0, capped);
}

/** The bar's fill, 0 to 1: the elapsed part of the length, empty without a length. */
export function fraction(elapsed, length) {
  if (!length) return 0;
  return Math.min(1, Math.max(0, elapsed / length));
}

/** A time in microseconds as `m:ss`, or `h:mm:ss` from an hour; seconds rounded down; nothing
 * for a time that is not known. */
export function timeWords(us) {
  if (us === null || us === undefined) return '';
  const total = Math.max(0, Math.floor(us / SEC));
  const [h, m, s] = [Math.floor(total / 3600), Math.floor((total % 3600) / 60), total % 60];
  const two = (n) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}
