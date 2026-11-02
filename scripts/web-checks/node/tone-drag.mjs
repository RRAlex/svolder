/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for tone-drag.js, the tone curve's gestures as a model: which point a press grabs,
// the axis hold, the gain's zone at 0 dB, the snap to the nearest step past the plot's edges, the
// right drag's and the pinch's Q, the wheel's steps, one write in flight per parameter with the
// newest waiting, the release, Esc, a lost stream, a refusal, the mode toast and the label's place.
// The writes are fakes that answer when the test says, so the order in flight is the test's.
// Run: node tone-drag.mjs
import assert from 'node:assert/strict';
import {
  AXIS_HOLD_PX, BAND_PARAM, LABEL_GAP_PX, Q_HALVES_PX, REACH, WHEEL_STEP_PX, ZERO_HOLD_PX,
  createDrag, grab,
  bandSteps, bandWriter, labelAt, wheelSteps,
} from '../../../bins/svolderd/web/core/tone-drag.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = () => new Promise((resolve) => setImmediate(resolve));

// 128 steps: frequency log-spaced 20 Hz to 20 kHz (3.15 px a step across a 400 px plot), gain from
// -20 dB in quarters (0 dB at 80), Q doubling every 16 steps from 0.5 (8 at 64).
const STEPS = {
  f: Array.from({ length: 128 }, (_, i) => 20 * 1000 ** (i / 127)),
  g: Array.from({ length: 128 }, (_, i) => -20 + i / 4),
  q: Array.from({ length: 128 }, (_, i) => 0.5 * 2 ** (i / 16)),
};
const PLOT = { width: 400, height: 172 };
const START = { f: 40, g: 80, q: 64 };
const px = (raw) => 400 * raw / 127;                       // x of a frequency step
const py = (raw, height = 172) => height * 10 * (13 - STEPS.g[raw]) / 260;  // y of a gain step
const zeroY = (height) => height / 2;                       // y of 0 dB: 13 of the 26 dB drawn

// A drag over fake writes: `calls` are the writes sent, each answering when `answer` says.
function rig(gesture, over = {}) {
  const calls = [];
  const log = { held: [], answers: [], toasts: [], ended: 0 };
  const press = over.press ?? { x: px(START.f), y: py(START.g) };
  const drag = createDrag({
    band: 2,
    gesture,
    start: over.start ?? START,
    press,
    plot: over.plot ?? PLOT,
    map: over.map,
    zeroHoldPx: over.zeroHoldPx,
    steps: over.steps ?? STEPS,
    write: (param, raw) => new Promise((resolve) => calls.push({ param, raw, resolve })),
    on: {
      held: (raws) => log.held.push(raws),
      answer: (param, text) => log.answers.push([param, text]),
      toast: (words) => log.toasts.push(words),
      ended: () => { log.ended++; },
    },
  });
  const sent = (param) => calls.filter((c) => c.param === param).map((c) => c.raw);
  const answer = async (call, body = { text: `${call.param}${call.raw}` }) => {
    call.resolve(body);
    await flush();
  };
  return { drag, calls, log, press, sent, answer };
}

await ok('grab_takes_the_nearer_point_in_reach_and_the_picked_on_a_tie', () => {
  const points = [{ band: 0, x: 100, y: 100 }, { band: 1, x: 110, y: 100 }];
  const mouse = { reach: REACH.mouse };
  assert.equal(grab(points, { x: 102, y: 100 }, { ...mouse, picked: 1 }), 0, 'nearer beats picked');
  assert.equal(grab(points, { x: 108, y: 100 }, { ...mouse, picked: 0 }), 1);
  assert.equal(grab(points, { x: 105, y: 100 }, { ...mouse, picked: 1 }), 1, 'tie: picked');
  assert.equal(grab(points, { x: 105, y: 100 }, { ...mouse, picked: 0 }), 0);
  assert.equal(grab(points, { x: 105, y: 100 }, { ...mouse, picked: null }), 0, 'tie: lower');
  assert.equal(grab(points, { x: 105, y: 100 }, { ...mouse, picked: 7 }), 0, 'picked not tied');
  // The reach is a distance, not a box: 12 px away on the diagonal is out of reach.
  assert.equal(grab(points, { x: 100 + 9, y: 100 + 9 }, { ...mouse, picked: null }), 1);
  assert.equal(grab([points[0]], { x: 109, y: 109 }, mouse), null);
  assert.equal(grab([points[0]], { x: 112, y: 100 }, mouse), 0, 'at the reach itself');
  assert.equal(grab([points[0]], { x: 113, y: 100 }, mouse), null);
  assert.equal(grab([points[0]], { x: 120, y: 100 }, { reach: REACH.touch }), 0,
    'a finger reaches wider');
  assert.equal(grab([], { x: 1, y: 1 }, mouse), null);
  assert.deepEqual(REACH, { mouse: 12, touch: 24, wheel: 20 });
  // the wheel reaches further than a press: 18 px away is out of a press's reach, in the wheel's
  const away = { x: 118, y: 100 };
  assert.equal(grab([points[0]], away, mouse), null);
  assert.equal(grab([points[0]], away, { reach: REACH.wheel }), 0);
  assert.equal(grab([points[0]], { x: 120, y: 100 }, { reach: REACH.wheel }), 0, 'at the reach');
  assert.equal(grab([points[0]], { x: 121, y: 100 }, { reach: REACH.wheel }), null);
  assert.equal(grab(points, { x: 106, y: 100 }, { reach: REACH.wheel }), 1, 'the nearer still wins');
});

await ok('grab_misses_away_from_every_point_but_a_wide_press_takes_the_nearest_in_x', () => {
  const points = [{ band: 0, x: 100, y: 20 }, { band: 3, x: 250, y: 150 }];
  const at = { x: 190, y: 80 };
  assert.equal(grab(points, at, { reach: REACH.mouse }), null);
  assert.equal(grab(points, at, { reach: REACH.mouse, wide: true }), 3, 'nearest in x, y ignored');
  assert.equal(grab(points, { x: 160, y: 80 }, { reach: REACH.mouse, wide: true }), 0);
  assert.equal(grab(points, { x: 175, y: 80 }, { reach: 12, wide: true, picked: 3 }), 3,
    'x tie: picked');
  assert.equal(grab(points, { x: 175, y: 80 }, { reach: 12, wide: true }), 0, 'x tie: lower');
  assert.equal(grab([], at, { reach: 12, wide: true }), null);
  // A point in reach wins over a nearer one in x that is out of reach.
  const apart = [{ band: 1, x: 110, y: 200 }, { band: 2, x: 125, y: 100 }];
  assert.equal(grab(apart, { x: 110, y: 100 }, { reach: REACH.touch, wide: true }), 2);
});

await ok('a_drag_across_holds_the_gain_until_8_px_up_or_down', async () => {
  assert.equal(AXIS_HOLD_PX, 8);
  const { drag, press, log, calls } = rig('move');
  drag.move({ x: press.x + 30, y: press.y + 7 });
  assert.equal(drag.held.g, 80, '7 px down leaves the gain');
  assert.ok(drag.held.f > 40, 'the frequency follows');
  assert.deepEqual(calls.map((c) => c.param), ['f'], 'only the frequency written');
  drag.move({ x: press.x + 30, y: press.y + 8 });
  assert.ok(drag.held.g < 80, '8 px down moves the gain');
  assert.deepEqual(log.held.at(-1), drag.held);
  // Mirror: the frequency holds while the pointer goes up and down.
  const up = rig('move');
  up.drag.move({ x: up.press.x + 7, y: up.press.y - 30 });
  assert.equal(up.drag.held.f, 40, '7 px across leaves the frequency');
  assert.ok(up.drag.held.g > 80, 'the gain follows up');
  up.drag.move({ x: up.press.x - 8, y: up.press.y - 30 });
  assert.ok(up.drag.held.f < 40, '8 px left moves the frequency');
  // An axis once free stays free: back within the hold the value is the pointer's own.
  const back = rig('move', { press: { x: px(40) + 1.5, y: py(80) } });
  back.drag.move({ x: back.press.x + 30, y: back.press.y });
  back.drag.move({ x: back.press.x + 5, y: back.press.y });
  assert.equal(back.drag.held.f, 42);
});

await ok('values_snap_to_the_nearest_step_and_carry_on_past_the_plot', () => {
  const plot = { width: 400, height: 100 };
  const r = rig('move', { plot, press: { x: px(40), y: py(80, 100) } });
  r.drag.move({ x: px(50) + 1, y: plot.height + 40 });
  assert.equal(r.drag.held.f, 50, '1 px off a step snaps to it');
  assert.equal(r.drag.held.g, 0, '40 px below the plot is the lowest step, -20 dB');
  r.drag.move({ x: px(50) + 1, y: -40 });
  assert.equal(r.drag.held.g, 127, '40 px above the plot is the highest');
  r.drag.move({ x: 400 + 60, y: py(80, 100) });
  assert.equal(r.drag.held.f, 127, 'past the right edge');
  r.drag.move({ x: -60, y: py(80, 100) });
  assert.equal(r.drag.held.f, 0, 'past the left edge');
  r.drag.move({ x: px(60) + 1.4, y: py(90, 100) });
  assert.deepEqual([r.drag.held.f, r.drag.held.g], [60, 90]);
  // The same ground inside the plot, with the table's own 0 dB row.
  const zero = rig('move', { press: { x: px(40), y: py(80) } });
  zero.drag.move({ x: px(40), y: py(80) + 20 });
  zero.drag.move({ x: px(40), y: py(80) + 0.2 });
  assert.equal(zero.drag.held.g, 80, 'a pointer on the 0 dB row is 0 dB');
});

await ok('the_gain_sticks_at_the_0_db_step_within_10_px_of_the_0_db_line', async () => {
  assert.equal(ZERO_HOLD_PX, 10);
  // 520 px tall: 20 px a dB, so the zone is the full 10 px and the steps beside 0 dB 5 px apart.
  const tall = { width: 400, height: 520 };
  const mid = zeroY(520);
  const below = { f: 40, g: 60, q: 64 };
  const at = (y, over = {}) => {
    const r = rig('move', { plot: tall, start: below, press: { x: px(40), y: 0 }, ...over });
    r.drag.move({ x: px(40), y });
    return r;
  };
  for (const y of [mid - 10, mid - 9, mid - 5, mid, mid + 5, mid + 9, mid + 10]) {
    assert.equal(at(y).drag.held.g, 80, `${y - mid} px from the line is 0 dB`);
  }
  assert.equal(at(mid - 11).drag.held.g, 82, '11 px above: the nearest step, 0.55 dB up');
  assert.equal(at(mid + 11).drag.held.g, 78, '11 px below: the nearest step, 0.55 dB down');
  assert.equal(at(mid - 10.5).drag.held.g, 82, 'just past the edge');
  assert.equal(at(mid + 10.5).drag.held.g, 78, 'just past the edge, below');
  // Out of the zone the map is the pointer's own again, however far on.
  assert.equal(at(mid + 60).drag.held.g, 68, '3 dB down');
  // The frequency is the pointer's own inside the zone.
  const sideways = rig('move', { plot: tall, start: below, press: { x: px(40), y: 0 } });
  sideways.drag.move({ x: px(60) + 1, y: mid });
  assert.deepEqual([sideways.drag.held.f, sideways.drag.held.g], [60, 80]);
  // The zone is the 0 dB step of the table, wherever the table has it.
  const late = { ...STEPS, g: Array.from({ length: 128 }, (_, i) => -25 + i / 4) };
  assert.equal(late.g[100], 0);
  const r = at(mid + 4, { steps: late });
  assert.equal(r.drag.held.g, 100, 'the table\'s own 0 dB step');
  // A table with no 0 dB step has no zone: the nearest step, here -0.35 dB.
  const none = { ...STEPS, g: Array.from({ length: 128 }, (_, i) => -20.1 + i / 4) };
  assert.equal(at(mid + 9, { steps: none }).drag.held.g, 79);
  assert.equal(at(mid + 9, { steps: none }).calls.length, 1, 'only the gain asked');
});

await ok('the_zero_zone_is_never_wider_than_one_db_of_the_plot', () => {
  // 172 px tall: 6.6 px a dB, the zone 6.6 px a side and not 10.
  const mid = zeroY(172);
  const below = { f: 40, g: 60, q: 64 };
  const at = (y, plot = PLOT) => {
    const r = rig('move', { plot, start: below, press: { x: px(40), y: 0 } });
    r.drag.move({ x: px(40), y });
    return r.drag.held.g;
  };
  assert.equal(at(mid + 6), 80, 'inside');
  assert.equal(at(mid - 6), 80);
  assert.equal(at(mid + 7.5), 75, '7.5 px down is -1.1 dB: the nearest step, -1.25');
  assert.equal(at(mid - 7.5), 85);
  // 60 px tall: 2.3 px a dB.
  const short = { width: 400, height: 60 };
  assert.equal(at(30 + 2, short), 80);
  assert.equal(at(30 - 2, short), 80);
  assert.equal(at(30 + 3, short), 75, '3 px is -1.3 dB: the nearest step, -1.25');
});

await ok('the_zero_zone_is_for_a_moves_gain_only_and_waits_for_the_y_axis', async () => {
  const mid = zeroY(520);
  const tall = { width: 400, height: 520 };
  const below = { f: 40, g: 60, q: 64 };
  // Q gestures never ask for a gain, in the zone or out of it.
  for (const gesture of ['width', 'pinch']) {
    const r = rig(gesture, {
      plot: tall, start: below, press: gesture === 'pinch' ? { distance: 100 } : { x: 0, y: 0 },
    });
    if (gesture === 'pinch') r.drag.spread(200);
    r.drag.move({ x: 120, y: mid });
    r.drag.move({ x: 120, y: mid + 3 });
    assert.equal(r.drag.held.g, 60, `${gesture} leaves the gain`);
    assert.ok(r.calls.every((c) => c.param === 'q'), `${gesture} writes only the Q`);
  }
  // The y axis not yet free (7 px from the press, in the zone): the gain stays where it was.
  const held = rig('move', { plot: tall, start: below, press: { x: px(40), y: mid - 7 } });
  held.drag.move({ x: px(60), y: mid });
  assert.equal(held.drag.held.g, 60, 'an axis held is held, zone or not');
  assert.ok(held.drag.held.f > 40);
  assert.deepEqual(held.calls.map((c) => c.param), ['f']);
  held.drag.move({ x: px(60), y: mid + 1 });
  assert.equal(held.drag.held.g, 80, '8 px from the press the axis is free and the zone applies');
});

await ok('moving_within_the_zero_zone_writes_the_0_db_step_once', async () => {
  const mid = zeroY(520);
  const r = rig('move', {
    plot: { width: 400, height: 520 }, start: { f: 40, g: 60, q: 64 }, press: { x: px(40), y: 0 },
  });
  for (const y of [mid - 10, mid - 4, mid, mid + 6, mid + 10, mid - 2]) {
    r.drag.move({ x: px(40), y });
  }
  assert.deepEqual(r.sent('g'), [80], 'one write');
  assert.equal(r.log.held.length, 1, 'and the page told once');
  assert.deepEqual(r.calls.map((c) => c.param), ['g']);
  await r.answer(r.calls[0]);
  assert.equal(r.calls.length, 1);
  // Out and back in: the step goes out again after the one sent from outside the zone.
  r.drag.move({ x: px(40), y: mid + 40 });
  r.drag.move({ x: px(40), y: mid });
  await r.answer(r.calls[1]);
  assert.deepEqual(r.sent('g'), [80, 72, 80]);
});

await ok('a_right_drag_halves_q_every_120_px_right_and_doubles_it_left', () => {
  assert.equal(Q_HALVES_PX, 120);
  const { drag, press, calls } = rig('width');
  drag.move({ x: press.x + 120, y: press.y - 80 });
  assert.equal(drag.held.q, 48, 'q 8 → 4');
  assert.deepEqual([drag.held.f, drag.held.g], [40, 80], 'only the Q');
  drag.move({ x: press.x + 240, y: press.y });
  assert.equal(drag.held.q, 32, 'q 8 → 2');
  drag.move({ x: press.x - 120, y: press.y });
  assert.equal(drag.held.q, 80, 'q 8 → 16');
  drag.move({ x: press.x - 5000, y: press.y });
  assert.equal(drag.held.q, 127, 'past the table');
  drag.move({ x: press.x + 5000, y: press.y });
  assert.equal(drag.held.q, 0);
  assert.ok(calls.every((c) => c.param === 'q'));
  drag.move({ x: press.x + 10, y: press.y });
  assert.equal(drag.held.q, 63, 'travel counts from the press, in a ratio');
});

await ok('a_pinch_twice_as_wide_halves_q', () => {
  const { drag, calls } = rig('pinch', { press: { distance: 100 } });
  drag.spread(200);
  assert.equal(drag.held.q, 48, 'twice as far apart: q 8 → 4');
  drag.spread(50);
  assert.equal(drag.held.q, 80, 'half as far: q 8 → 16');
  drag.spread(100);
  assert.equal(drag.held.q, 64, 'back to the press');
  drag.spread(0);
  drag.spread(NaN);
  assert.equal(drag.held.q, 64, 'no distance changes nothing');
  drag.move({ x: 300, y: 20 });
  assert.deepEqual(drag.held, START, 'a pinch ignores the pointer');
  assert.ok(calls.every((c) => c.param === 'q'));
  const other = rig('move');
  other.drag.spread(300);
  assert.deepEqual(other.drag.held, START, 'a drag ignores the fingers');
});

await ok('wheel_a_notch_is_one_step_whatever_its_pixels_and_small_deltas_add_up', () => {
  assert.equal(WHEEL_STEP_PX, 40);
  assert.deepEqual(wheelSteps(0, -3, 1), { steps: 1, rest: 0 }, 'a notch up narrows');
  assert.deepEqual(wheelSteps(0, 3, 1), { steps: -1, rest: 0 }, 'a notch down widens');
  assert.deepEqual(wheelSteps(0, -1, 2), { steps: 1, rest: 0 }, 'a page is a notch');
  assert.deepEqual(wheelSteps(0, 0, 1), { steps: 0, rest: 0 });
  assert.deepEqual(wheelSteps(0, -30, 0), { steps: 0, rest: -30 });
  assert.deepEqual(wheelSteps(-30, -30, 0), { steps: 1, rest: -20 }, 'the rest carried');
  assert.deepEqual(wheelSteps(0, 100, 0), { steps: -1, rest: 0 }, 'a 100 px notch is one step');
  assert.deepEqual(wheelSteps(0, -40, 0), { steps: 1, rest: 0 });
  // A mouse's notch arrives as 53 to 120 px: one step each, the rest dropped.
  for (const px of [53, 100, 120, 240]) {
    assert.deepEqual(wheelSteps(0, -px, 0), { steps: 1, rest: 0 }, `-${px} px`);
    assert.deepEqual(wheelSteps(0, px, 0), { steps: -1, rest: 0 }, `${px} px`);
  }
  assert.deepEqual(wheelSteps(-30, -100, 0), { steps: 1, rest: 0 }, 'a notch resets the rest');
  assert.deepEqual(wheelSteps(0, -3, 1), { steps: 1, rest: 0 }, 'lines: -3 is one step');
  let tick = { steps: 0, rest: 0 };
  let ticks = 0;
  for (let i = 0; i < 4; i++) {
    tick = wheelSteps(tick.rest, -10, 0);
    ticks += tick.steps;
  }
  assert.equal(ticks, 1, 'four -10 px events add up to one step');
  assert.deepEqual(wheelSteps(20, -30, 0), { steps: 0, rest: -10 }, 'opposite deltas cancel');
  let rest = 0;
  let total = 0;
  for (let i = 0; i < 10; i++) {
    const got = wheelSteps(rest, -13, 0);
    total += got.steps;
    rest = got.rest;
  }
  assert.equal(total, 3, '130 px of small deltas is three steps');
  assert.equal(rest, -10);
});

await ok('one_write_in_flight_per_parameter_and_the_newest_waits', async () => {
  const { drag, press, calls, sent, answer } = rig('move');
  for (let i = 1; i <= 6; i++) drag.move({ x: press.x + 10 * i, y: press.y - 10 * i });
  assert.deepEqual(calls.map((c) => c.param), ['f', 'g'], 'one in flight each, at once');
  const lastF = drag.held.f;
  const lastG = drag.held.g;
  await answer(calls[0]);
  assert.equal(sent('f').length, 2, 'the answer sends the one waiting');
  assert.equal(sent('f')[1], lastF, 'the newest, none of the between');
  assert.equal(sent('g').length, 1, 'the other parameter is still waiting on its own');
  await answer(calls[1]);
  assert.equal(sent('g').at(-1), lastG);
  await answer(calls[2]);
  await answer(calls[3]);
  assert.equal(calls.length, 4);
});

await ok('the_release_writes_where_the_pointer_let_go_unless_already_sent', async () => {
  const a = rig('move');
  a.drag.move({ x: a.press.x + 20, y: a.press.y });
  a.drag.move({ x: a.press.x + 60, y: a.press.y });
  const where = a.drag.held.f;
  a.drag.release();
  assert.equal(a.log.ended, 1);
  await a.answer(a.calls[0]);
  assert.deepEqual(a.sent('f').slice(-1), [where],
    'the let-go place is written after the one in flight');
  await a.answer(a.calls[1]);
  assert.equal(a.calls.length, 2, 'nothing twice');
  // Already sent and answered: the release writes nothing more.
  const b = rig('move');
  b.drag.move({ x: b.press.x + 60, y: b.press.y });
  await b.answer(b.calls[0]);
  b.drag.release();
  await flush();
  assert.equal(b.calls.length, 1);
  // A drag back to where the band started writes that step: the device holds the one before it.
  const c = rig('move');
  c.drag.move({ x: c.press.x + 60, y: c.press.y });
  await c.answer(c.calls[0]);
  c.drag.move({ x: c.press.x + 1, y: c.press.y });
  assert.equal(c.drag.held.f, 40);
  assert.deepEqual(c.sent('f'), [c.sent('f')[0], 40]);
});

await ok('a_fling_then_lost_release_writes_the_last_place_once', async () => {
  const { drag, press, calls, sent, answer, log } = rig('move');
  for (let i = 1; i <= 30; i++) drag.move({ x: press.x + 9 * i, y: press.y });
  const last = drag.held.f;
  drag.release();                      // the button went up where the page never heard it
  drag.release();
  assert.equal(log.ended, 1);
  await answer(calls[0]);
  await flush();
  assert.equal(sent('f').filter((raw) => raw === last).length, 1, 'the last place, once');
  assert.deepEqual(sent('f').slice(-1), [last]);
  assert.equal(calls.length, 2);
  await answer(calls[1]);
  drag.move({ x: press.x - 100, y: press.y });
  await flush();
  assert.equal(calls.length, 2, 'a move after the end writes nothing');
});

await ok('esc_and_switch_write_the_held_value_lost_writes_nothing_more', async () => {
  for (const why of ['esc', 'switch']) {
    const r = rig('move');
    r.drag.move({ x: r.press.x + 20, y: r.press.y });
    r.drag.move({ x: r.press.x + 80, y: r.press.y });
    const where = r.drag.held.f;
    r.drag.cancel(why);
    assert.equal(r.log.ended, 1, why);
    await r.answer(r.calls[0]);
    assert.deepEqual(r.sent('f').slice(-1), [where], `${why} writes where it was`);
    await r.answer(r.calls[1]);
    assert.deepEqual(r.drag.held.f, where, 'not put back');
  }
  const lost = rig('move');
  lost.drag.move({ x: lost.press.x + 20, y: lost.press.y });
  lost.drag.move({ x: lost.press.x + 80, y: lost.press.y });
  lost.drag.cancel('lost');
  assert.equal(lost.log.ended, 1);
  await lost.answer(lost.calls[0]);
  assert.equal(lost.calls.length, 1, 'the waiting write is dropped');
  assert.equal(lost.log.toasts.length, 0);
  await lost.drag.settled;
});

await ok('a_refusal_toasts_once_ends_the_drag_and_writes_nothing_more', async () => {
  const r = rig('move');
  r.drag.move({ x: r.press.x + 40, y: r.press.y - 40 });
  r.drag.move({ x: r.press.x + 80, y: r.press.y - 80 });
  assert.equal(r.calls.length, 2, 'f and g in flight, both waiting a newer one');
  await r.answer(r.calls[0], { refused: 'Refused: the amp is off.' });
  assert.deepEqual(r.log.toasts, ['Refused: the amp is off.']);
  assert.equal(r.log.ended, 1);
  await r.answer(r.calls[1], { refused: 'Refused: the amp is off.' });
  assert.equal(r.log.toasts.length, 1, 'once for the drag, not once a parameter');
  assert.equal(r.calls.length, 2, 'the waiting writes were dropped');
  r.drag.move({ x: r.press.x + 140, y: r.press.y });
  r.drag.release();
  await flush();
  assert.equal(r.calls.length, 2, 'nothing after it');
  assert.equal(r.log.ended, 1);
  // A write that never answers (it throws) is a refusal too.
  const calls = [];
  const log = { toasts: [], ended: 0 };
  const drag = createDrag({
    band: 0, gesture: 'width', start: START, press: { x: 0, y: 0 }, plot: PLOT, steps: STEPS,
    write: (param, raw) => { calls.push([param, raw]); return Promise.reject(new Error('down')); },
    on: { held() {}, answer() {}, toast: (w) => log.toasts.push(w), ended: () => { log.ended++; } },
  });
  drag.move({ x: 120, y: 0 });
  await drag.settled;
  assert.equal(log.toasts.length, 1);
  assert.equal(log.ended, 1);
});

await ok('mode_set_to_eq_toasts_once_per_drag', async () => {
  const r = rig('move');
  r.drag.move({ x: r.press.x + 40, y: r.press.y - 40 });
  r.drag.move({ x: r.press.x + 80, y: r.press.y - 80 });
  await r.answer(r.calls[0], { text: 'a', modeSetToEq: true });
  await r.answer(r.calls[1], { text: 'b', modeSetToEq: true });
  await r.answer(r.calls[2], { text: 'c', modeSetToEq: true });
  assert.deepEqual(r.log.toasts, ['Mode set to EQ']);
  assert.deepEqual(r.log.answers, [['f', 'a'], ['g', 'b'], ['f', 'c']]);
  assert.equal(r.log.ended, 0, 'a toast does not end the drag');
  const quiet = rig('width');
  quiet.drag.move({ x: quiet.press.x + 120, y: 0 });
  await quiet.answer(quiet.calls[0], { text: 'q' });
  assert.deepEqual(quiet.log.toasts, []);
  assert.deepEqual(quiet.log.answers, [['q', 'q']]);
});

await ok('settled_waits_for_every_answer', async () => {
  const r = rig('move');
  let done = false;
  await r.drag.settled;                          // nothing sent: settled at once
  r.drag.move({ x: r.press.x + 40, y: r.press.y - 40 });
  r.drag.move({ x: r.press.x + 80, y: r.press.y - 80 });
  r.drag.release();
  r.drag.settled.then(() => { done = true; });
  await flush();
  assert.equal(done, false, 'two in flight and two waiting');
  await r.answer(r.calls[0]);
  await r.answer(r.calls[1]);
  assert.equal(done, false, 'the waiting ones were sent in their turn');
  assert.equal(r.calls.length, 4);
  await r.answer(r.calls[2]);
  assert.equal(done, false);
  await r.answer(r.calls[3]);
  assert.equal(done, true);
});

await ok('labelAt_keeps_the_label_inside_the_plot_at_every_edge', () => {
  const plot = { width: 400, height: 172 };
  const size = { width: 120, height: 20 };
  assert.deepEqual(labelAt({ x: 200, y: 100 }, plot, size),
    { x: 140, y: 100 - LABEL_GAP_PX - 20 }, 'centred above the point');
  for (const x of [-30, 0, 10, 60, 200, 340, 399, 400, 430]) {
    for (const y of [-30, 0, 5, 30, 90, 171, 172, 200]) {
      const at = labelAt({ x, y }, plot, size);
      assert.ok(at.x >= 0 && at.x + size.width <= plot.width, `x of ${x},${y}: ${at.x}`);
      assert.ok(at.y >= 0 && at.y + size.height <= plot.height, `y of ${x},${y}: ${at.y}`);
    }
  }
  assert.equal(labelAt({ x: 5, y: 100 }, plot, size).x, 0, 'left edge');
  assert.equal(labelAt({ x: 395, y: 100 }, plot, size).x, 280, 'right edge');
  assert.equal(labelAt({ x: 200, y: 5 }, plot, size).y, 0, 'top edge');
  assert.equal(labelAt({ x: 200, y: 172 }, plot, size).y, 172 - LABEL_GAP_PX - 20, 'bottom edge');
  const wide = labelAt({ x: 10, y: 10 }, { width: 100, height: 10 }, size);
  assert.deepEqual(wide, { x: 0, y: 0 }, 'a label larger than the plot sits at its corner');
});

await ok('a_press_released_without_moving_writes_nothing', async () => {
  for (const gesture of ['move', 'width', 'pinch']) {
    const press = gesture === 'pinch' ? { distance: 100 } : { x: px(40), y: py(80) };
    const r = rig(gesture, { press });
    if (gesture === 'pinch') r.drag.spread(100);
    else r.drag.move({ x: press.x + 3, y: press.y + 3 });
    assert.deepEqual(r.log.held, [], gesture);
    r.drag.release();
    await r.drag.settled;
    assert.equal(r.calls.length, 0, gesture);
    assert.equal(r.log.ended, 1, gesture);
  }
});

await ok('a_pointer_that_is_not_a_number_keeps_the_held_value_and_writes_nothing', async () => {
  const r = rig('move');
  r.drag.move({ x: NaN, y: NaN });
  r.drag.move({ x: NaN, y: py(80) });
  assert.deepEqual(r.drag.held, START);
  r.drag.move({ x: r.press.x + 40, y: r.press.y + 30 });
  const moved = { ...r.drag.held };
  r.drag.move({ x: NaN, y: NaN });
  assert.deepEqual(r.drag.held, moved, 'a lost coordinate leaves what was asked');
  const sentBefore = r.calls.length;
  r.drag.move({ x: NaN, y: NaN });
  assert.equal(r.calls.length, sentBefore, 'and sends nothing');
  const wide = rig('width', { press: { x: 0, y: 0 } });
  wide.drag.move({ x: NaN, y: 0 });
  assert.equal(wide.drag.held.q, 64, 'a right drag too');
  assert.equal(wide.calls.length, 0);
  assert.ok(r.calls.every((c) => c.raw >= 0 && c.raw <= 127), 'no step of -1');
});

await ok('a_refusal_after_the_release_toasts_once_and_ends_cleanly', async () => {
  const r = rig('move');
  r.drag.move({ x: r.press.x + 40, y: r.press.y });
  r.drag.release();
  assert.equal(r.log.ended, 1);
  let done = false;
  r.drag.settled.then(() => { done = true; });
  await r.answer(r.calls[0], { refused: 'Refused: the amp is off.' });
  assert.deepEqual(r.log.toasts, ['Refused: the amp is off.']);
  assert.equal(r.log.ended, 1, 'ended once, by the release');
  await flush();
  assert.equal(done, true, 'settles');
  assert.equal(r.calls.length, 1, 'writes nothing more');
});

await ok('settled_holds_for_a_write_sent_the_moment_a_loop_ended', async () => {
  // A write answered, then a new value sent after some microtask hops: whether the send lands
  // inside the loop that is ending or just after it, `settled` waits for the write it makes.
  for (let hops = 0; hops < 12; hops++) {
    const r = rig('move');
    r.drag.move({ x: r.press.x + 20, y: r.press.y });
    r.calls[0].resolve({ text: 'first' });
    for (let i = 0; i < hops; i++) await Promise.resolve();
    r.drag.move({ x: r.press.x + 80, y: r.press.y });
    r.drag.release();
    let done = false;
    r.drag.settled.then(() => { done = true; });
    await flush();
    assert.equal(r.calls.length, 2, `hops ${hops}: the second value is in flight`);
    assert.equal(done, false, `hops ${hops}: settled waits for the write in flight`);
    await r.answer(r.calls[1]);
    assert.equal(done, true, `hops ${hops}: and settles with its answer`);
  }
});

await ok('a_value_waiting_that_is_the_one_in_flight_is_not_written_again', async () => {
  const r = rig('move');
  const at = (dx) => ({ x: r.press.x + dx, y: r.press.y });
  r.drag.move(at(20));                       // A in flight
  const a = r.drag.held.f;
  r.drag.move(at(80));                       // B waits
  assert.notEqual(r.drag.held.f, a);
  r.drag.move(at(20));                       // the pointer is back at A: A waits
  assert.equal(r.drag.held.f, a);
  r.drag.release();
  await r.answer(r.calls[0]);
  assert.deepEqual(r.sent('f'), [a], 'A is not written a second time');
  let done = false;
  r.drag.settled.then(() => { done = true; });
  await flush();
  assert.equal(done, true, 'nothing is left to wait for');
  // Not sent twice in a row, but B then A after B was written is two writes.
  const t = rig('move');
  t.drag.move({ x: t.press.x + 20, y: t.press.y });
  const first = t.drag.held.f;
  await t.answer(t.calls[0]);
  t.drag.move({ x: t.press.x + 80, y: t.press.y });
  await t.answer(t.calls[1]);
  t.drag.move({ x: t.press.x + 20, y: t.press.y });
  await t.answer(t.calls[2]);
  assert.equal(t.sent('f').length, 3);
  assert.equal(t.sent('f')[2], first, 'back at A after B was written: A is written');
});

await ok('a_map_of_plain_coordinates_writes_the_same_steps', async () => {
  // A 400 x 100 canvas: linear in log Hz from 20 Hz to 20 kHz across, +12 dB at the top to -12 dB
  // at the foot.
  const canvas = { width: 400, height: 100 };
  const map = {
    hzAt: (x) => 20 * 1000 ** (x / canvas.width),
    dbAt: (y) => 12 - (24 * y) / canvas.height,
    yOfDb: (db) => ((12 - db) / 24) * canvas.height,
  };
  const xOfHz = (hz) => (canvas.width * Math.log(hz / 20)) / Math.log(1000);
  const onPlot = rig('move');
  onPlot.drag.move({ x: px(90), y: py(100) });
  const onMap = rig('move', {
    plot: canvas,
    map,
    press: { x: xOfHz(STEPS.f[START.f]), y: map.yOfDb(STEPS.g[START.g]) },
  });
  onMap.drag.move({ x: xOfHz(STEPS.f[90]), y: map.yOfDb(STEPS.g[100]) });
  assert.deepEqual(onPlot.sent('f'), [90]);
  assert.deepEqual(onPlot.sent('g'), [100]);
  assert.deepEqual(onMap.sent('f'), onPlot.sent('f'), 'the same frequency step');
  assert.deepEqual(onMap.sent('g'), onPlot.sent('g'), 'the same gain step');
  assert.deepEqual(onMap.drag.held, onPlot.drag.held);
  // The map's own 0 dB line is the one that sticks: 3 px under it (0.72 dB) stays at 0 dB.
  const sticky = rig('move', {
    plot: canvas, map, zeroHoldPx: 4, start: { ...START, g: 100 },
    press: { x: xOfHz(STEPS.f[START.f]), y: map.yOfDb(5) },
  });
  sticky.drag.move({ x: xOfHz(STEPS.f[START.f]), y: map.yOfDb(0) + 3 });
  assert.equal(sticky.drag.held.g, 80);
});

await ok('zero_hold_px_0_turns_the_sticky_0_db_off', async () => {
  const start = { ...START, g: 100 };
  const press = { x: px(START.f), y: py(100) };
  // 5 px under the 0 dB line (86 px) of the 172 px plot: inside today's hold, outside none.
  const at = { x: px(START.f), y: zeroY(172) + 5 };
  const sticks = rig('move', { start, press });
  sticks.drag.move(at);
  assert.equal(sticks.drag.held.g, 80, 'the default holds 0 dB');
  const free = rig('move', { start, press, zeroHoldPx: 0 });
  free.drag.move(at);
  assert.equal(free.drag.held.g, 77, 'no hold: the nearest step, -0.75 dB');
  const wide = rig('move', { start, press, zeroHoldPx: 8 });
  wide.drag.move({ x: px(START.f), y: zeroY(172) + 8 });
  assert.equal(wide.drag.held.g, 80, 'a wider hold');
});

await ok('band_steps_read_a_models_tables', () => {
  const tables = {
    bands: [{ f: [[20], [40]] }, { f: [['off'], ['off']] }],
    gain: [[-20], [-19.75]],
    q: [[0.5], [0.56]],
  };
  assert.deepEqual(BAND_PARAM, { f: 'frequency', g: 'gain', q: 'q' });
  const first = bandSteps(tables, 1);
  assert.deepEqual(first, { f: [20, 40], g: [-20, -19.75], q: [0.5, 0.56] });
  const second = bandSteps(tables, 2);
  assert.ok(Number.isNaN(second.f[0]) && Number.isNaN(second.f[1]), 'a word is NaN');
  assert.deepEqual(second.g, [-20, -19.75]);
});

await ok('the_band_writer_answers_the_drag_from_the_actions', async () => {
  const calls = [];
  let reply = { ok: true, text: '60 Hz', mode_set_to_eq: true };
  const actions = { setBand: async (...args) => { calls.push(args); return reply; } };
  let gate = null;
  let stop = false;
  const write = bandWriter({
    actions, band: 2, device: 'Gungnir%202', before: () => (gate ? [gate] : []), aborted: () => stop,
  });
  assert.deepEqual(await write('f', 7), { text: '60 Hz', modeSetToEq: true });
  assert.deepEqual(calls[0], [2, 'frequency', { raw: 7 }, { device: 'Gungnir%202' }]);
  reply = { ok: false, error: 'Refused: x' };
  assert.deepEqual(await write('g', 3), { refused: 'Refused: x' });
  let open;
  gate = new Promise((resolve) => { open = resolve; });
  const before = calls.length;
  const waiting = write('q', 1);
  await flush();
  assert.equal(calls.length, before, 'it waits for what came before');
  open();
  await waiting;
  assert.equal(calls.length, before + 1);
  gate = null;
  stop = true;
  assert.deepEqual(await write('f', 9), {});
  assert.equal(calls.length, before + 1, 'aborted: no call');
});

console.log(`${n} tone-drag checks passed`);
