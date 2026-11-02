/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the pure rules of core/clock.js and playing.js: the elapsed time of the track
// from the player's position, its rate and the time it was read, the daemon-clock offset the page
// works it out against, the bar's fraction and the clock words, and the folded title's loop.
// Run: node playing.mjs
import assert from 'node:assert/strict';
import {
  clockOffset, daemonClock, elapsedUs, fraction, timeWords,
} from '../../../bins/svolderd/web/core/clock.js';
import { loopPlan } from '../../../bins/svolderd/web/themes/ember/playing.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const SEC = 1_000_000;
// A track read at 30 s, 1 s into the daemon's day, playing at the normal rate.
const track = (extra = {}) => ({
  status: 'playing', position: 30 * SEC, rate: 1000, readAt: 1_000_000, length: 200 * SEC, ...extra,
});

ok('progress_ticks_from_position_rate_and_read_time', () => {
  // Read at 1 000 000 ms; 2.5 s later the track is 2.5 s further on.
  assert.equal(elapsedUs(track(), 1_000_000), 30 * SEC);
  assert.equal(elapsedUs(track(), 1_002_500), 32.5 * SEC);
  // Double speed counts twice as fast, half speed half as fast (rate in thousandths).
  assert.equal(elapsedUs(track({ rate: 2000 }), 1_002_000), 34 * SEC);
  assert.equal(elapsedUs(track({ rate: 500 }), 1_002_000), 31 * SEC);
});

ok('progress_does_not_move_while_paused_or_stopped', () => {
  for (const status of ['paused', 'stopped']) {
    assert.equal(elapsedUs(track({ status }), 1_060_000), 30 * SEC, status);
  }
});

ok('progress_is_clamped_to_zero_and_the_length', () => {
  // A fake player runs on past the track's length; a read stamped in the future is no negative.
  assert.equal(elapsedUs(track(), 1_000_000 + 500_000), 200 * SEC);
  assert.equal(elapsedUs(track({ position: 0 }), 999_000), 0);
  // Without a length nothing caps it.
  assert.equal(elapsedUs(track({ length: null }), 1_500_000), 530 * SEC);
});

ok('progress_uses_the_daemon_clock_offset', () => {
  // The page's clock runs 7 s behind the daemon's: the part said 1 003 000 ms when this
  // browser's clock read 996 000 ms.
  const offset = clockOffset(1_003_000, 996_000);
  assert.equal(offset, 7000);
  // Three seconds later by the browser it is 1 006 000 by the daemon, 6 s after the read.
  const now = daemonClock(offset, 999_000);
  assert.equal(now, 1_006_000);
  assert.equal(elapsedUs(track(), now), 36 * SEC);
  // A page whose clock is ahead gets the same answer for the same moment.
  const ahead = clockOffset(1_003_000, 1_303_000);
  assert.equal(elapsedUs(track(), daemonClock(ahead, 1_306_000)), 36 * SEC);
});

ok('times_read_m_ss_and_h_mm_ss', () => {
  const cases = [[0, '0:00'], [999_999, '0:00'], [SEC, '0:01'], [59 * SEC, '0:59'],
    [60 * SEC, '1:00'], [192 * SEC, '3:12'], [599 * SEC, '9:59'], [600 * SEC, '10:00'],
    [3599 * SEC, '59:59'], [3600 * SEC, '1:00:00'], [3723 * SEC, '1:02:03'],
    [36000 * SEC, '10:00:00']];
  for (const [us, want] of cases) assert.equal(timeWords(us), want, `${us} us`);
  assert.equal(timeWords(-5 * SEC), '0:00');
  assert.equal(timeWords(null), '');
});

ok('the_bar_fills_by_the_fraction_of_the_length_and_is_empty_without_one', () => {
  assert.equal(fraction(0, 200 * SEC), 0);
  assert.equal(fraction(50 * SEC, 200 * SEC), 0.25);
  assert.equal(fraction(300 * SEC, 200 * SEC), 1);
  assert.equal(fraction(50 * SEC, null), 0);
  assert.equal(fraction(50 * SEC, 0), 0);
});
ok('the_title_loops_at_30_px_a_second_linearly_after_a_two_second_rest', () => {
  for (const width of [2, 20, 72, 150, 468, 5000, 1_000_000]) {
    const plan = loopPlan(width);
    // The lap ends where the copy, a 40 px gap behind the text, stands where the text started.
    assert.equal(plan.dist, width + 40);
    assert.ok(Math.abs(plan.move - ((width + 40) / 30) * 1000) < 1e-6, `${width}: ${plan.move}`);
    assert.equal(plan.total, 2000 + plan.move);
    const [first, rest, last] = plan.frames;
    assert.equal(plan.frames.length, 3);
    assert.equal(first.transform, 'translateX(0px)');
    assert.equal(rest.transform, 'translateX(0px)');
    assert.equal(last.transform, `translateX(${-(width + 40)}px)`);
    // It waits 2 s, then moves with no easing anywhere: the pace is the same at every moment.
    assert.deepEqual(plan.frames.map((f) => f.offset), [0, 2000 / plan.total, 1]);
    for (const f of plan.frames) assert.equal(f.easing, undefined, `${width}`);
    const pace = (width + 40) / ((plan.total - 2000) / 1000);
    assert.ok(Math.abs(pace - 30) < 1e-6, `${width}: ${pace}`);
  }
});

ok('a_long_title_is_not_sped_up_to_fit_a_time', () => {
  // A loop has no time cap: a long title is not sped up.
  const plan = loopPlan(5000);
  assert.ok(plan.total > 5000);
  assert.ok(Math.abs(plan.move - (5040 / 30) * 1000) < 1e-6);
});

console.log(`playing.mjs: ${n} checks passed`);
