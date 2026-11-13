/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The daemon's clock on the page, in two parts.
//
// The Now playing card's arithmetic, for any theme: the daemon's clock against this browser's, how
// far into the track it is, the bar's fill and the clock words (`clockOffset`, `daemonClock`,
// `elapsedUs`, `fraction`, `timeWords`). These are pure: numbers in, numbers or text out, so a
// browser whose own clock is wrong still shows the player's time, and a theme's drawing may import
// them from this file.
//
// The one ticker of the ages a device's page carries as stamps (`tickAges`, with `AGES_EVERY_MS`).
// It is not pure: it reads and writes the page under the root it is given, holds a timer and
// watches whether the page is on show. The Devices popup reaches it as `core.clock.tickAges`.

import { ageWords, signalText } from './words.js';
import { watchVisible as watchPage } from './visible.js';

/** Microseconds in a second. */
const SEC = 1_000_000;

/** How often the ages are written again (ms). */
export const AGES_EVERY_MS = 1000;

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

/**
 * Keep the ages of a Device section current while the page is on show: every `AGES_EVERY_MS`, and
 * at once when the page comes back on show, each age is worked out again on the daemon's clock and
 * written where its words changed. A part that only grew older is so never sent again.
 *
 * `root` is what the section is looked for in at every tick (an element, or the document), so a
 * section drawn again, a row taken away and a section that is not there are followed with nothing
 * kept. The three selectors say what the theme drew:
 *   stamp    the element that carries the daemon's clock when the part was made (`data-now`, ms);
 *            the first tick that sees a stamp value notes when, and the daemon's clock now is
 *            that stamp plus the time since (an element drawn again with it keeps the note)
 *   ages     the cells of the readings' ages (`data-heard-at`, ms by the daemon's clock), written
 *            as `ago(ageWords(secs))`
 *   signal   the cell of the Bluetooth strength (`data-head`, `data-heard-at`, `data-live` when it
 *            is a live reading), written as `signalText` words it
 *
 * Options: `ago(age)` words an age (`<age> ago` by default); `now()` is this browser's clock in ms
 * (`Date.now` by default; any clock that only runs forward serves); `when()`, when given, holds
 * the writing while it says no (a fold-out that is closed) without holding the note of a part's
 * arrival; `signal` is an AbortSignal that stops the ticker (a theme passes its mount's, so its
 * unmount leaves no timer); `watchVisible` and `timers` (`{ setInterval, clearInterval }`) are the
 * page's by default.
 *
 * Returns `{ tick(), stop() }`: `tick()` writes now (after a draw, or when a fold-out opens),
 * `stop()` ends the timer and the watch of the page; a stopped ticker writes nothing.
 */
export function tickAges(root, { stamp, ages, signal: strength }, {
  ago = (age) => `${age} ago`,
  now = () => Date.now(),
  when = null,
  signal = null,
  watchVisible = watchPage,
  timers = globalThis,
} = {}) {
  /** When this page first saw each daemon clock value a carrier holds (`data-now`), by `now()`:
   * an element drawn again with the same stamp is aged from the first sight, not from the draw. */
  const arrived = new Map();
  let timer = null;
  let watch = null;
  let stopped = Boolean(signal?.aborted);

  const put = (element, text) => {
    if (element.textContent !== text) element.textContent = text;
  };

  function tick() {
    if (stopped) return;
    const carrier = root.querySelector(stamp);
    if (!carrier) return;
    const seen = carrier.dataset.now;
    if (!arrived.has(seen)) arrived.set(seen, now());
    if (when && !when()) return;
    const offset = clockOffset(Number(seen), arrived.get(seen));
    const secs = (heardAt) => (daemonClock(offset, now()) - Number(heardAt)) / 1000;
    for (const cell of root.querySelectorAll(ages)) {
      put(cell, ago(ageWords(secs(cell.dataset.heardAt))));
    }
    const cell = strength ? root.querySelector(strength) : null;
    if (cell) {
      put(cell, signalText(cell.dataset.head, 'live' in cell.dataset, secs(cell.dataset.heardAt)));
    }
  }

  const rest = () => {
    if (timer !== null) timers.clearInterval(timer);
    timer = null;
  };

  function stop() {
    stopped = true;
    rest();
    watch?.stop();
  }

  if (!stopped) {
    watch = watchVisible({
      onVisible(shown) {
        rest();
        if (stopped || !shown) return;
        tick();
        timer = timers.setInterval(tick, AGES_EVERY_MS);
      },
    });
    signal?.addEventListener('abort', stop, { once: true });
  }

  return { tick, stop };
}
