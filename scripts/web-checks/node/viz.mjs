/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the visualizer's pure rules (viz-colour.js, viz-model.js): the base colour
// scale and the stereo tint at the design's exact values, a frame decoded from the wire shape,
// the smoothing and the 2 s max hold, the frame drop rule, the beats scheduled ahead from the
// tempo and phase so they land on time behind a steady delay, the clock offset, the delay's
// median and the band and pixel maps. Run: node viz.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  ALPHA_GAIN, BASE_STOPS, TINT_LEFT, TINT_RIGHT, alpha, base, pixel, tint,
} from '../../../bins/svolderd/web/themes/ember/viz-colour.js';
import {
  BANDS, BeatClock, DelayMedian, VIZ, VizModel, bandCentreHz, bandPos, decode, freqPos, posBand,
  posFreq, rowBands,
} from '../../../bins/svolderd/web/themes/ember/viz-model.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

// --- the colours --------------------------------------------------------------------------------

ok('base_scale_matches_the_design_stops', () => {
  const want = [[0, [12, 12, 12]], [0.35, [45, 43, 44]], [0.65, [123, 119, 116]],
    [0.85, [202, 198, 191]], [1, [252, 248, 240]]];
  assert.deepEqual(BASE_STOPS.map(([at, rgb]) => [at, [...rgb]]), want);
  for (const [at, rgb] of want) assert.deepEqual(base(at), rgb, `stop ${at}`);
  // Halfway between two stops is the mean of their colours.
  assert.deepEqual(base(0.5), [84, 81, 80]);
  assert.deepEqual(base(0.75), [163, 159, 154]);
  // Past either end it stays on the end colour; nothing that is not a number is brighter than 0.
  assert.deepEqual(base(-3), [12, 12, 12]);
  assert.deepEqual(base(7), [252, 248, 240]);
  assert.deepEqual(base(NaN), [12, 12, 12]);
});

ok('the_cards_are_translucent_alike_and_the_canvas_paints_no_ground', () => {
  const web = (name) => new URL(`../../../bins/svolderd/web/${name}`, import.meta.url);
  const css = readFileSync(web('themes/ember/style.css'), 'utf8');
  const rule = (selector) => {
    const found = css.match(new RegExp(`\\n${selector.replace('.', '\\.')} \\{([^}]*)\\}`));
    assert.ok(found, `a ${selector} rule`);
    return found[1];
  };
  // The one token the tone panel, Now playing and the visualizer card all take their ground from.
  assert.match(css, /--glass:\s*color-mix\(in srgb, var\(--card\) 70%, transparent\)/);
  assert.match(rule('.p-viz'), /background:\s*var\(--glass\)/);
  assert.match(rule('.p-playing'), /background:\s*var\(--glass\)/);
  assert.match(rule('.eqg'), /background:\s*var\(--glass\)/);
  const code = readFileSync(web('themes/ember/visualizer.js'), 'utf8');
  assert.ok(!/alpha:\s*false/.test(code), 'no canvas is opaque');
});

ok('a_spectrogram_point_is_transparent_in_silence_and_opaque_by_a_quarter_scale', () => {
  assert.equal(ALPHA_GAIN, 4);
  assert.equal(alpha(0, 0), 0);
  assert.equal(alpha(NaN, undefined), 0);
  assert.equal(alpha(0.1, 0), 0.4);
  assert.equal(alpha(0.1, 0.2), 0.8, 'the louder channel sets it');
  assert.equal(alpha(0.25, 0), 1);
  assert.equal(alpha(0.9, 0.9), 1);
  assert.equal(alpha(-1, -1), 0);
});

ok('tint_is_cyan_when_left_leads_and_orange_when_right_leads', () => {
  assert.deepEqual([...TINT_LEFT], [40, 220, 255]);
  assert.deepEqual([...TINT_RIGHT], [255, 135, 27]);
  // A difference of 1/6 or more at a loud level is the whole tint.
  assert.deepEqual(pixel(0.8, 0.4), [40, 220, 255]);
  assert.deepEqual(pixel(0.4, 0.8), [255, 135, 27]);
  assert.deepEqual(tint(0.8, 0.4), { to: TINT_LEFT, amount: 1 });
  assert.equal(tint(0.4, 0.8).to, TINT_RIGHT);
  // A small difference at a middle level is a part of it, worked out here from the formula:
  // d = (0.5 - 0.45) * 6 = 0.3, amount = 0.3 * min(1, 0.5 * 1.5) = 0.225, base(0.5 * 1.12).
  const amount = 0.3 * 0.75;
  const from = base(0.56);
  const mixed = from.map((c, i) => c + (TINT_LEFT[i] - c) * amount);
  pixel(0.5, 0.45).forEach((c, i) => near(c, mixed[i], 1, `channel ${i}`));
  near(tint(0.5, 0.45).amount, amount, 1e-12, 'amount');
  // Quiet sound is tinted less: the same difference at a quarter of the level.
  near(tint(0.1, 0.05).amount, 0.3 * 0.15, 1e-12, 'quiet amount');
});

ok('equal_channels_get_no_tint', () => {
  for (const v of [0, 0.1, 0.35, 0.6, 0.9, 1]) {
    assert.equal(tint(v, v).amount, 0, `v ${v}`);
    assert.deepEqual(pixel(v, v), base(v * 1.12), `v ${v}`);
  }
  // A difference too small to see (under 1/600 of the range) changes the pixel by under a unit.
  const a = pixel(0.6, 0.6);
  const b = pixel(0.6005, 0.6);
  for (let i = 0; i < 3; i++) assert.ok(Math.abs(a[i] - b[i]) <= 1);
});

// --- helpers for frames -------------------------------------------------------------------------

const wireFrame = ({ t, l = 0, r = 0, lv = [0, 0, 0, 0], beat = 0, bpm = null, phase = 0 }) => {
  const bytes = (v) => Buffer.from(typeof v === 'number' ? new Uint8Array(BANDS).fill(v) : v);
  return {
    t, l: bytes(l).toString('base64'), r: bytes(r).toString('base64'), lv, beat, bpm, phase,
  };
};

// --- decode -------------------------------------------------------------------------------------

ok('a_frame_decodes_from_the_wire_shape', () => {
  const left = Uint8Array.from({ length: BANDS }, (_, i) => i);
  const right = Uint8Array.from({ length: BANDS }, (_, i) => 255 - i);
  const wire = wireFrame({ t: 1_700_000_000_123, l: left, r: right, lv: [10, 20, 30, 255],
    beat: 1, bpm: 121, phase: 77 });
  assert.equal(wire.l.length, 320);
  for (const input of [wire, JSON.stringify(wire)]) {
    const f = decode(input);
    assert.equal(f.t, 1_700_000_000_123);
    assert.ok(f.left instanceof Uint8Array && f.left.length === BANDS);
    assert.deepEqual([...f.left], [...left]);
    assert.deepEqual([...f.right], [...right]);
    assert.deepEqual([...f.levels], [10, 20, 30, 255]);
    assert.equal(f.beat, true);
    assert.equal(f.bpm, 121);
    assert.equal(f.phase, 77);
  }
  const quiet = decode(wireFrame({ t: 5 }));
  assert.equal(quiet.beat, false);
  assert.equal(quiet.bpm, null);
  // What is not a frame decodes to nothing, never to a half frame.
  assert.equal(decode('{'), null);
  assert.equal(decode(null), null);
  assert.equal(decode({ ...wire, l: wire.l.slice(4) }), null);
  assert.equal(decode({ ...wire, r: '' }), null);
  assert.equal(decode({ ...wire, t: 'now' }), null);
  assert.equal(decode({ ...wire, lv: [1, 2] }), null);
  assert.equal(decode({ ...wire, l: '!'.repeat(320) }), null);
});

// --- silence ------------------------------------------------------------------------------------

ok('silence_draws_base_zero', () => {
  const m = new VizModel();
  m.hello(5000, 1000);
  let now = 1000;
  for (let i = 0; i < 90; i++, now += 33) {
    m.push(wireFrame({ t: now + 4000 }), now);
    const { frame } = m.step(now);
    assert.ok(frame);
    assert.equal(m.beatDue(now), false);
  }
  assert.ok(m.left.every((v) => v === 0) && m.right.every((v) => v === 0));
  assert.ok(m.holdLeft.every((v) => v === 0) && m.holdRight.every((v) => v === 0));
  assert.deepEqual([...m.levels], [0, 0, 0, 0]);
  assert.deepEqual(pixel(m.left[100], m.right[100]), [12, 12, 12]);
  assert.deepEqual(base(0), [12, 12, 12]);
  // Cleared, a model draws the same.
  m.push(wireFrame({ t: now + 4000, l: 255, r: 255 }), now);
  m.step(now);
  m.clear();
  assert.ok(m.left.every((v) => v === 0) && m.holdLeft.every((v) => v === 0));
});

// --- smoothing and max hold ---------------------------------------------------------------------

ok('smoothing_blends_a_frame_with_the_one_before_by_0_4', () => {
  const m = new VizModel();
  m.hello(0, 0);
  m.push(wireFrame({ t: 0, l: 255, r: 0 }), 10);
  m.step(10);
  // From silence, one frame at full scale reaches 0.6 of it (a 0.4 share of what was).
  near(m.left[7], 0.6, 1e-6, 'first');
  assert.equal(m.right[7], 0);
  m.push(wireFrame({ t: 33, l: 255, r: 0 }), 10 + 1000 / 30);
  m.step(10 + 1000 / 30);
  near(m.left[7], 0.6 * 0.4 + 0.6, 1e-3, 'second');
  // A long gap between frames (a tab that was hidden) leaves nothing of the old picture.
  m.push(wireFrame({ t: 9000, l: 0, r: 0 }), 9010);
  m.step(9010);
  near(m.left[7], 0, 1e-3, 'after a gap');
});

ok('max_hold_keeps_a_peak_2_s_then_falls', () => {
  const m = new VizModel();
  m.hello(0, 0);
  let now = 0;
  const frame = (v) => { m.push(wireFrame({ t: now, l: v, r: 0 }), now); m.step(now); };
  for (let i = 0; i < 12; i++, now += 33) frame(255);
  const peak = m.holdLeft[40];
  assert.ok(peak > 0.99, `peak ${peak}`);
  const peakAt = now - 33;
  // Silence follows; the curve drops at once, the hold stays for 2 s since the last peak.
  for (; now < peakAt + 1900; now += 33) frame(0);
  assert.ok(m.left[40] < 0.01);
  assert.equal(m.holdLeft[40], peak, 'held at 1.9 s');
  assert.equal(m.holdRight[40], 0);
  // Past 2 s it falls, and keeps falling until it reaches the curve.
  for (; now < peakAt + 2600; now += 33) frame(0);
  const falling = m.holdLeft[40];
  assert.ok(falling < peak && falling > 0, `falling ${falling}`);
  for (; now < peakAt + 2900; now += 33) frame(0);
  assert.ok(m.holdLeft[40] < falling);
  for (; now < peakAt + 6000; now += 33) frame(0);
  assert.equal(m.holdLeft[40], 0);
  // A new peak restarts the 2 s, and a lower reading inside them does not lower the hold.
  for (let i = 0; i < 12; i++, now += 33) frame(200);
  const second = m.holdLeft[40];
  assert.ok(second > 0.7 && second < 0.8, `second ${second}`);
  for (let i = 0; i < 20; i++, now += 33) frame(100);
  assert.equal(m.holdLeft[40], second);
  // The hold is per band: another band never moved.
  assert.equal(m.holdLeft[41], second);
});

ok('with_no_frame_for_a_second_the_curves_fall_and_the_hold_runs_out', () => {
  const m = new VizModel();
  m.hello(0, 0);
  for (let i = 0; i < 12; i++) { m.push(wireFrame({ t: i * 33, l: 255 }), i * 33); m.step(i * 33); }
  const peak = m.holdLeft[3];
  m.step(500);
  assert.equal(m.left[3], peak, 'under a second the picture holds');
  m.step(1500);
  assert.ok(m.left[3] < peak, 'the curve falls');
  m.step(1800);
  m.step(2100);
  assert.ok(m.holdLeft[3] <= peak);
  for (let now = 2400; now < 9000; now += 300) m.step(now);
  assert.ok(m.left[3] < 0.001);
  assert.ok(m.holdLeft[3] < 0.001);
});

ok('each_band_holds_and_falls_on_its_own', () => {
  const m = new VizModel();
  m.hello(0, 0);
  let now = 0;
  const bands = (spec) => Uint8Array.from({ length: BANDS }, (_, i) => spec[i] ?? 0);
  const tick = (spec) => {
    m.push(wireFrame({ t: now, l: bands(spec), r: bands({}) }), now);
    m.step(now);
    now += 33;
  };
  // Three bands peak at different levels and times: 10 at once, 20 about 1 s on, 30 about 1.5 s on.
  const peaks = {};
  const phases = [[10, 255, 0], [20, 200, 1000], [30, 100, 1500]];
  const zeroAt = {};
  for (const [band, value, start] of phases) {
    for (; now < start; ) tick({});
    for (let i = 0; i < 12; i++) tick({ [band]: value });
    peaks[band] = { at: now - 33, level: m.holdLeft[band] };
  }
  assert.ok(peaks[10].level > 0.99 && peaks[20].level > 0.77 && peaks[30].level > 0.38);
  assert.ok(peaks[10].level > peaks[20].level && peaks[20].level > peaks[30].level);
  // 2.3 s after band 10's peak: it is falling, the others still hold their own peak, and the
  // bands nothing touched are at 0.
  for (; now < peaks[10].at + 2300; ) tick({});
  assert.ok(m.holdLeft[10] < peaks[10].level && m.holdLeft[10] > 0, 'band 10 falls');
  assert.equal(m.holdLeft[20], peaks[20].level, 'band 20 holds');
  assert.equal(m.holdLeft[30], peaks[30].level, 'band 30 holds');
  assert.equal(m.holdLeft[11], 0);
  assert.ok(m.holdRight.every((v) => v === 0));
  // Each is gone 2 s plus its own level after its own peak (a full scale falls in 1 s).
  for (; now < peaks[30].at + 4000; ) {
    tick({});
    for (const band of [10, 20, 30]) {
      if (zeroAt[band] === undefined && m.holdLeft[band] < 0.001) zeroAt[band] = now - 33;
    }
  }
  for (const band of [10, 20, 30]) {
    const want = peaks[band].at + 2000 + peaks[band].level * 1000;
    near(zeroAt[band], want, 120, `band ${band} runs out`);
  }
  assert.ok(zeroAt[10] < zeroAt[20] && zeroAt[20] < zeroAt[30]);
});

ok('a_stalled_stream_falls_at_the_normal_rate_not_in_one_step', () => {
  const m = new VizModel();
  m.hello(0, 0);
  for (let i = 0; i < 12; i++) {
    m.push(wireFrame({ t: i * 33, l: 255 }), i * 33);
    m.step(i * 33);
  }
  const full = m.left[5];
  // Draws every 16.7 ms; frames stop at 363 ms. Up to a second nothing changes.
  let now = 363;
  for (; now < 1350; now += 1000 / 60) m.step(now);
  assert.equal(m.left[5], full, 'a second of quiet holds the picture');
  // Then each draw keeps 0.4 ^ (16.7 / 33.3) of the curve, about 0.63: a fall, not a drop.
  let before = m.left[5];
  for (;;) {
    before = m.left[5];
    m.step(now);
    if (m.left[5] !== before) break;
    now += 1000 / 60;
  }
  near(m.left[5] / before, 0.4 ** 0.5, 0.01, 'one draw');
  let last = m.left[5];
  for (let i = 0; i < 10; i++) {
    now += 1000 / 60;
    m.step(now);
    assert.ok(m.left[5] < last, 'always falling');
    last = m.left[5];
  }
  // Ten more draws (167 ms) leave about 0.4 ^ 5 of it.
  near(last / before, 0.4 ** 5.5, 0.01, 'after eleven draws');
  // A draw after a long wait (a hidden tab) is one clamped step, not a jump to some other value.
  m.step(now + 60_000);
  assert.ok(m.left[5] < last);
  // Levels are what the last frame said until frames come again; a stalled stream shows none.
  assert.ok(m.levels.every((v) => v === 0));
});

// A passive capture gives no samples while its sink is suspended: the capture still reads running,
// the frames stop, and the card goes on drawing silence.
ok('frames_that_stop_while_the_capture_runs_end_in_the_colour_of_silence', () => {
  const m = new VizModel();
  m.hello(0, 0);
  const loud = { l: 255, r: 200, lv: [200, 180, 255, 255], bpm: 120, beat: 1 };
  for (let i = 0; i < 30; i++) {
    m.push(wireFrame({ t: i * 33, ...loud }), i * 33);
    m.step(i * 33);
  }
  assert.ok(m.left[7] > 0.5 && m.holdLeft[7] > 0.5, 'it was loud');
  // Draws go on at 60 Hz with no frame: five seconds.
  let now = 30 * 33;
  const quiet = now + 5000;
  let pulses = 0;
  for (; now < quiet; now += 1000 / 60) {
    m.step(now);
    if (m.beatDue(now) && now - 30 * 33 > VIZ.STALE_MS) pulses++;
  }
  assert.equal(pulses, 0, 'no beat pulse once the frames are stale');
  for (let i = 0; i < BANDS; i++) {
    assert.deepEqual(pixel(m.left[i], m.right[i]), base(0), `band ${i}: the waterfall's colour`);
    assert.ok(alpha(m.left[i], m.right[i]) < 0.001, `band ${i}: the waterfall is transparent`);
    assert.ok(m.holdLeft[i] < 0.001 && m.holdRight[i] < 0.001, `band ${i}: the hold has fallen`);
  }
  assert.ok(m.levels.every((v) => v === 0), 'levels fall to none');
  // And it listens again the moment frames come back.
  m.push(wireFrame({ t: 9000, ...loud }), now);
  m.step(now + 16);
  assert.ok(m.left[7] > 0.3);
});

// --- the frame drop rule ------------------------------------------------------------------------

ok('a_late_frame_is_dropped_only_when_a_newer_one_arrived', () => {
  const m = new VizModel();
  m.hello(0, 0);
  // Alone, a frame 400 ms behind is drawn: late is not a reason to drop it.
  assert.equal(m.push(wireFrame({ t: 100, l: 255 }), 500), true);
  let s = m.step(510);
  assert.equal(s.frame.t, 100);
  assert.equal(s.dropped, 0);
  // Two frames waiting for one draw: only the newest is drawn, the older one counts as dropped.
  m.push(wireFrame({ t: 133, l: 10 }), 520);
  m.push(wireFrame({ t: 166, l: 20 }), 521);
  s = m.step(530);
  assert.equal(s.frame.t, 166);
  assert.equal(s.dropped, 1);
  // Nothing waiting: nothing to draw, nothing dropped.
  s = m.step(540);
  assert.equal(s.frame, null);
  assert.equal(s.dropped, 0);
  // A frame older than the one already drawn is dropped on arrival, and so is a repeat.
  assert.equal(m.push(wireFrame({ t: 150 }), 550), false);
  assert.equal(m.push(wireFrame({ t: 166 }), 551), false);
  assert.equal(m.step(560).frame, null);
  // Out of order while waiting: the older one never replaces the newer.
  m.push(wireFrame({ t: 233 }), 600);
  assert.equal(m.push(wireFrame({ t: 200 }), 601), false);
  assert.equal(m.step(610).frame.t, 233);
  // Something that is not a frame is not taken.
  assert.equal(m.push('{', 700), false);
});

// --- the clock offset and the delay -------------------------------------------------------------

ok('the_clock_offset_comes_from_hello_and_frames_do_not_hide_the_delay', () => {
  const m = new VizModel();
  assert.equal(m.offsetMs, null);
  m.hello(1_000_500, 1_000_000);
  assert.equal(m.offsetMs, 500);
  assert.equal(m.daemonClock(1_000_100), 1_000_600);
  assert.equal(m.pageClock(1_000_600), 1_000_100);
  // A frame captured at daemon 1_000_700 that arrives at page 1_000_350 (350 ms behind: it was
  // captured at page time 1_000_200) leaves the offset alone.
  m.push(wireFrame({ t: 1_000_700 }), 1_000_350);
  assert.equal(m.offsetMs, 500);
  assert.equal(m.delayMs(1_000_350), 150);
  // A frame stamped later than the page's clock allows (a daemon clock that jumped, or a hello
  // read late) is no delay, and moves nothing either.
  m.push(wireFrame({ t: 1_001_050 }), 1_000_360);
  assert.equal(m.offsetMs, 500);
  assert.equal(m.delayMs(1_000_360), 75, 'the median of 150 and 0, never negative');
  // A second hello (a new stream) takes the daemon's clock afresh.
  m.hello(2_000_000, 1_500_000);
  assert.equal(m.offsetMs, 500_000);
});

ok('the_delay_is_the_median_of_the_last_2_s', () => {
  const d = new DelayMedian();
  assert.equal(d.median(0), null);
  for (let t = 0; t < 1000; t += 100) d.add(t, 100);
  // One slow frame does not move a median.
  d.add(1000, 900);
  assert.equal(d.median(1000), 100);
  // An even count takes the mean of the middle two.
  const e = new DelayMedian();
  e.add(0, 100);
  e.add(10, 200);
  e.add(20, 300);
  e.add(30, 400);
  assert.equal(e.median(30), 250);
  // A reading leaves after 2 s: at 2.5 s only what came since 0.5 s counts.
  const f = new DelayMedian();
  for (let t = 0; t < 500; t += 50) f.add(t, 50);
  for (let t = 500; t <= 2500; t += 50) f.add(t, 200);
  assert.equal(f.median(2500), 200);
  assert.equal(f.median(10_000), null);
  // The window is 2 s exactly: a reading 2 s old is in, one older is out.
  const g = new DelayMedian();
  g.add(0, 10);
  g.add(2000, 30);
  assert.equal(g.median(2000), 20);
  assert.equal(g.median(2001), 30);
  // Through the model: frames each 150 ms behind, one at 700 ms, read at the end.
  const m = new VizModel();
  m.hello(5000, 0);
  let now = 0;
  for (let i = 0; i < 80; i++, now += 33) {
    const behind = i === 40 ? 700 : 150;
    m.push(wireFrame({ t: now - behind + 5000 }), now);
    m.step(now);
  }
  assert.equal(m.delayMs(now), 150);
  assert.equal(VIZ.DELAY_WINDOW_MS, 2000);
});

// --- beats --------------------------------------------------------------------------------------

// A song at `bpm` whose beats fall at daemon time `first + k * period`: the frames the daemon
// would send, 30 a second, with the flag on the frame a beat falls in and the phase counted from
// the beat before.
const song = (bpm, first, from, to) => {
  const period = 60000 / bpm;
  const frames = [];
  for (let t = from; t < to; t += 1000 / 30) {
    const since = ((t - first) % period + period) % period;
    const beat = since < 1000 / 30 ? 1 : 0;
    frames.push({ t, beat, bpm, phase: Math.floor(since / period * 256) });
  }
  return { frames, beats: (k) => first + k * period, period };
};

// Runs a page at 60 Hz over `frames`, each arriving `delay` ms after its capture; the page's clock
// is `offset` behind the daemon's. Returns the page times a pulse was due.
const runPage = (m, frames, delay, offset, { holdBack = () => false } = {}) => {
  const pulses = [];
  const queue = frames.map((f) => ({ f, at: f.t - offset + delay }));
  const end = queue[queue.length - 1].at + 100;
  for (let now = queue[0].at; now < end; now += 1000 / 60) {
    while (queue.length && queue[0].at <= now && !holdBack(now)) {
      m.push(wireFrame(queue.shift().f), now);
    }
    m.step(now);
    if (m.beatDue(now)) pulses.push(now);
  }
  return pulses;
};

ok('beats_land_on_time_with_a_steady_150_ms_delay', () => {
  const offset = 7_000;
  const s = song(120, 10_000, 10_000, 20_000);
  const m = new VizModel();
  m.hello(5_000 + offset, 5_000);
  const pulses = runPage(m, s.frames, 150, offset);
  // The beat the first frames show is already 150 ms old: too late to pulse. After it, one pulse
  // per beat, each on the beat (the page tick is 16.7 ms) and not 150 ms after it.
  assert.ok(pulses.length >= 15, `${pulses.length} pulses`);
  const beatsAt = (k) => s.beats(k) - offset;
  const late = pulses.map((p) => {
    const k = Math.round((p - (s.beats(0) - offset)) / s.period);
    return p - beatsAt(k);
  });
  for (const l of late) assert.ok(l >= 0 && l <= 17, `a pulse ${l} ms after its beat`);
  const ks = pulses.map((p) => Math.round((p - (s.beats(0) - offset)) / s.period));
  assert.deepEqual(ks, ks.map((_, i) => ks[0] + i), 'one pulse for each beat, none twice');
  // Without a delay it is just as on time; with 400 ms behind (over the tolerance) too, as it is
  // scheduled ahead of the frames.
  for (const delay of [0, 60, 400]) {
    const m2 = new VizModel();
    m2.hello(5_000 + offset, 5_000);
    const got = runPage(m2, s.frames, delay, offset);
    assert.ok(got.length >= 15, `${delay} ms: ${got.length} pulses`);
    for (const p of got.slice(2)) {
      const k = Math.round((p - (s.beats(0) - offset)) / s.period);
      const l = p - beatsAt(k);
      assert.ok(l >= 0 && l <= 17, `${delay} ms: a pulse ${l} ms after its beat`);
    }
  }
});

ok('a_beat_frame_that_arrives_late_is_not_pulsed_twice', () => {
  const offset = 3_000;
  const s = song(120, 10_000, 10_000, 16_000);
  const m = new VizModel();
  m.hello(offset, 0);
  // The page stalls from 12 100 to 12 700 (page clock): the frames of the beat at daemon 12 000
  // + 500 k wait and arrive together.
  const stall = (now) => now > 12_100 - 0 && now < 12_700;
  const pulses = runPage(m, s.frames, 100, offset, { holdBack: stall });
  for (let i = 1; i < pulses.length; i++) {
    assert.ok(pulses[i] - pulses[i - 1] > 400, `pulses ${pulses[i - 1]} and ${pulses[i]}`);
  }
  assert.ok(pulses.length >= 8);
});

ok('without_a_tempo_a_beat_flag_pulses_once_and_silence_pulses_nothing', () => {
  const m = new VizModel();
  m.hello(0, 0);
  m.push(wireFrame({ t: 100, beat: 1 }), 120);
  assert.equal(m.beatDue(120), true);
  assert.equal(m.beatDue(121), false, 'once');
  // A flag 100 ms after is a double of the same onset; one 300 ms after is a new one.
  m.push(wireFrame({ t: 200, beat: 1 }), 220);
  assert.equal(m.beatDue(220), false);
  m.push(wireFrame({ t: 400, beat: 1 }), 420);
  assert.equal(m.beatDue(420), true);
  // No flags, no tempo: nothing, however long.
  for (let now = 500; now < 5000; now += 33) {
    m.push(wireFrame({ t: now }), now);
    assert.equal(m.beatDue(now), false);
  }
});

ok('the_grid_stops_when_the_tempo_clears_or_the_frames_stop', () => {
  const offset = 0;
  const s = song(120, 0, 0, 4_000);
  const m = new VizModel();
  m.hello(0, 0);
  const pulses = runPage(m, s.frames, 50, offset);
  assert.ok(pulses.length >= 6);
  // The last frame was at ~4 s: a second later nothing more is due, though the grid goes on.
  assert.equal(m.beatDue(5_500), false);
  assert.equal(m.beatDue(7_000), false);
  // The daemon clears the tempo in silence: frames without a bpm end the grid at once.
  const q = new VizModel();
  q.hello(0, 0);
  const t = song(120, 0, 0, 3_000);
  runPage(q, t.frames, 50, 0);
  q.push(wireFrame({ t: 3_100, bpm: null }), 3_150);
  assert.equal(q.beatDue(3_400), false);
  assert.equal(q.beatDue(3_600), false);
});

ok('after_a_long_gap_the_next_beat_is_found_at_once', () => {
  const clock = new BeatClock();
  const beat = (t, now) => clock.observe({ t, beat: true, bpm: 120, phase: 0 }, now);
  beat(0, 0);
  assert.equal(clock.due(0, 0), true);
  assert.equal(clock.due(1, 0), false);
  // A tab comes back after 60 s: 120 beats were passed. The frame that carries a beat at 60 000
  // is drawn on that beat, in the first call, and the beat is not pulsed twice.
  beat(60_000, 60_000);
  assert.equal(clock.due(60_000, 0), true);
  assert.equal(clock.due(60_001, 0), false);
  // Back after 10 min, the frame a beat and a bit late (30 ms): still drawn, the beat is within
  // the tolerance.
  const late = new BeatClock();
  late.observe({ t: 0, beat: true, bpm: 120, phase: 0 }, 0);
  assert.equal(late.due(0, 0), true);
  late.observe({ t: 600_030, beat: false, bpm: 120, phase: 15 }, 600_030);
  assert.equal(late.due(600_030, 0), true);
  // One 300 ms late is passed over and the next beat, 200 ms ahead, waits for its time.
  const missed = new BeatClock();
  missed.observe({ t: 0, beat: true, bpm: 120, phase: 0 }, 0);
  assert.equal(missed.due(0, 0), true);
  missed.observe({ t: 600_300, beat: false, bpm: 120, phase: 154 }, 600_300);
  assert.equal(missed.due(600_300, 0), false);
  assert.equal(missed.due(600_498, 0), false);
  assert.equal(missed.due(600_500, 0), true);
  assert.equal(missed.due(600_501, 0), false);
  // A very long gap costs one call, not one per beat: a million beats behind.
  const far = new BeatClock();
  far.observe({ t: 0, beat: true, bpm: 120, phase: 0 }, 0);
  assert.equal(far.due(0, 0), true);
  far.observe({ t: 500_000_000, beat: true, bpm: 120, phase: 0 }, 500_000_000);
  assert.equal(far.due(500_000_000, 0), true);
});

ok('a_flag_without_a_tempo_is_dropped_when_late_and_kept_when_not_yet_drawn', () => {
  // A flag read 200 ms after its sound is too late to pulse, as a beat of the grid would be.
  const m = new VizModel();
  m.hello(5_000, 0);
  m.push(wireFrame({ t: 5_100, beat: 1 }), 300);
  assert.equal(m.beatDue(300), false);
  // One 100 ms behind is drawn.
  m.push(wireFrame({ t: 5_500, beat: 1 }), 600);
  assert.equal(m.beatDue(600), true);
  // A flag waiting for a draw is not lost to the frames after it (a page that drew late).
  m.push(wireFrame({ t: 6_000, beat: 1 }), 1_010);
  m.push(wireFrame({ t: 6_033 }), 1_043);
  m.push(wireFrame({ t: 6_066 }), 1_076);
  assert.equal(m.beatDue(1_080), true);
  assert.equal(m.beatDue(1_081), false);
  // Two flags waiting 250 ms apart: after a stall the older is too late and the newer, 50 ms
  // behind, pulses.
  m.push(wireFrame({ t: 7_000, beat: 1 }), 2_010);
  m.push(wireFrame({ t: 7_250, beat: 1 }), 2_260);
  assert.equal(m.beatDue(2_300), true);
  assert.equal(m.beatDue(2_301), false);
});

// --- band and pixel maps ------------------------------------------------------------------------

ok('bands_map_to_pixels_on_a_log_axis_from_20_hz_to_20_khz', () => {
  assert.equal(BANDS, 240);
  assert.equal(freqPos(20, 1000), 0);
  assert.equal(freqPos(20000, 1000), 1000);
  near(freqPos(632.4555, 1000), 500, 0.01, 'the middle');
  near(posFreq(500, 1000), 632.4555, 0.01, 'the middle back');
  for (const hz of [20, 55, 440, 1000, 9999, 20000]) {
    near(posFreq(freqPos(hz, 777), 777), hz, 1e-6, 'round trip');
  }
  // Band i spans 20 * 2^(i/24); its centre is half a band up.
  near(bandCentreHz(0), 20 * 2 ** (0.5 / 24), 1e-9, 'band 0 centre');
  near(bandCentreHz(239), 20 * 2 ** (239.5 / 24), 1e-6, 'band 239 centre');
  // 1/24 octave is 1/239.2 of the axis: bands are about 4.18 px wide on 1000 px.
  near(bandPos(1, 1000) - bandPos(0, 1000), 1000 / (24 * Math.log2(1000)), 1e-6, 'one band');
  for (const i of [0, 1, 50, 120, 200, 239]) assert.equal(posBand(bandPos(i, 1000), 1000), i);
  // Off the ends: clamped to the first and last band.
  assert.equal(posBand(-5, 1000), 0);
  assert.equal(posBand(2000, 1000), 239);
  // A spectrogram column: the top row is the highest band, the bottom the lowest.
  const rows = rowBands(200);
  assert.equal(rows.length, 200);
  assert.equal(rows[199], 0);
  assert.ok(rows[0] >= 238, `top row ${rows[0]}`);
  for (let y = 1; y < 200; y++) assert.ok(rows[y] <= rows[y - 1], `row ${y}`);
  // On a phone card 360 px tall, one row never skips more than 2 bands, so none are lost.
  const tall = rowBands(360);
  for (let y = 1; y < 360; y++) assert.ok(tall[y - 1] - tall[y] <= 2);
  assert.ok(VIZ.F_MIN === 20 && VIZ.F_MAX === 20000);
});
ok('the_head_script_reads_the_key_the_card_keeps_its_choices_under', () => {
  const web = '../../../bins/svolderd/web/';
  const read = (name) => readFileSync(new URL(web + name, import.meta.url), 'utf8');
  const card = read('themes/ember/visualizer.js').match(/export const KEY = '([^']+)'/);
  const head = read('themes/ember/viz-pref.js').match(/getItem\('([^']+)'\)/);
  assert.ok(card && head, 'both files name a key');
  assert.equal(head[1], card[1], 'viz-pref.js and visualizer.js use one key');
  assert.match(read('themes/ember/viz-pref.js'), /kept\.on === false/);
});

console.log(`viz.mjs: ${n} checks passed`);
