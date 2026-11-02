/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for tone-curve.js, the page's copy of the tone control's curve maths: every case
// of fixtures/tone-curve.json (the core's own numbers, written by the Rust test
// `the_curve_fixture_is_current`) must come out of the script within a tolerance. Rewrite the
// fixture with: SVOLDER_WRITE_FIXTURES=1 cargo test -p svolderd curve_fixture
// Run: node tone-curve.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  POINTS, RANGE_DB, F_MIN, F_MAX, PEAK_SHOWN_DB, clippingPeak, dbAt, hzAt, linePath, logFreqs,
  markerPoint, peakDb, peakWords, responseDb, xOf, yOf,
} from '../../../bins/svolderd/web/core/tone-curve.js';

const fixture = JSON.parse(readFileSync(
  fileURLToPath(new URL('./fixtures/tone-curve.json', import.meta.url)), 'utf8'));

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (got, want, tol, what) =>
  assert.ok(Math.abs(got - want) <= tol, `${what}: ${got} against ${want}`);
const bandsOf = (c) => c.bands.map((b) => ({ f0: b.f0, gainDb: b.gain_db, q: b.q }));

ok('the fixture has the cases the maths is held to', () => {
  assert.equal(fixture.freqs.length, 181);
  assert.ok(fixture.cases.length >= 20, `${fixture.cases.length} cases`);
  const names = fixture.cases.map((c) => c.name);
  for (const want of ['flat', 'one boost', 'one cut', 'two overlapping boosts', 'gain -20',
    'gain +6', 'wide boost and a narrow one between samples', 'q 0.5', 'q 8', 'tiny peak',
    'peak just over', 'peak away from every band centre', 'band 1 low edge', 'band 5 high edge',
    'flat mode with shaped bands', 'loudness mode with shaped bands']) {
    assert.ok(names.includes(want), `no case "${want}"`);
  }
});

ok('constants are the core\'s', () => {
  assert.deepEqual([POINTS, RANGE_DB, F_MIN, F_MAX, PEAK_SHOWN_DB], [181, 13, 20, 20000, 0.05]);
});

ok('logFreqs: the core\'s 181 frequencies', () => {
  const got = logFreqs(POINTS);
  assert.equal(got.length, 181);
  got.forEach((f, i) => near(f, fixture.freqs[i], 1e-9 * f, `freq ${i}`));
  assert.deepEqual(logFreqs(1), [20]);
  assert.deepEqual(logFreqs(0), []);
});

ok('responseDb: every case\'s curve, as drawn, within 1e-6', () => {
  for (const c of fixture.cases) {
    const ys = responseDb(bandsOf(c), fixture.freqs).map(yOf);
    assert.equal(ys.length, c.ys.length, c.name);
    ys.forEach((y, i) => near(y, c.ys[i], 1e-6, `${c.name} y[${i}]`));
  }
});

ok('clippingPeak: the core\'s peak, or none, in every case', () => {
  for (const c of fixture.cases) {
    const got = clippingPeak(c.mode, bandsOf(c));
    if (c.peak === null) assert.equal(got, null, `${c.name}: ${got}`);
    else near(got, c.peak, 1e-6, `${c.name} peak`);
  }
});

ok('peakDb: every case\'s raw peak, shown or not, within 1e-6', () => {
  for (const c of fixture.cases) near(peakDb(bandsOf(c)), c.peak_db, 1e-6, `${c.name} peak_db`);
});

ok('peakDb: a peak away from every band\'s centre needs the best sample\'s window', () => {
  // The case's summed curve peaks outside every band's half-octave window: the samples alone
  // fall short of the peak by more than the tolerance, so a script without the refinement either
  // side of the highest sample would fail the check above.
  const c = fixture.cases.find((x) => x.name === 'peak away from every band centre');
  const sampled = Math.max(...responseDb(bandsOf(c), logFreqs(1024)));
  assert.ok(c.peak_db - sampled > 5e-5, `${c.peak_db - sampled}`);
  near(peakDb(bandsOf(c)), c.peak_db, 1e-6, 'the refined peak');
});

ok('peakDb: the same refinement, above and below what is shown', () => {
  const byName = (name) => fixture.cases.find((c) => c.name === name);
  const tiny = peakDb(bandsOf(byName('tiny peak')));
  assert.ok(tiny > 0 && tiny < PEAK_SHOWN_DB, `${tiny}`);
  const over = peakDb(bandsOf(byName('peak just over')));
  assert.ok(over >= PEAK_SHOWN_DB && over < 0.06, `${over}`);
  assert.equal(clippingPeak('eq', bandsOf(byName('tiny peak'))), null);
  assert.notEqual(clippingPeak('eq', bandsOf(byName('peak just over'))), null);
  assert.equal(peakDb([]), 0);
  // A cut-only curve peaks at 0 or below, never above.
  assert.ok(peakDb(bandsOf(byName('one cut'))) <= 1e-9);
});

ok('clippingPeak: only the EQ mode warns', () => {
  const boosted = bandsOf(fixture.cases.find((c) => c.name === 'one boost'));
  assert.notEqual(clippingPeak('eq', boosted), null);
  for (const mode of ['flat', 'loudness', null, undefined]) {
    assert.equal(clippingPeak(mode, boosted), null, `${mode}`);
  }
});

ok('peakWords: the core\'s words, with U+2212 for a minus', () => {
  for (const c of fixture.cases) {
    if (c.peak !== null) assert.equal(peakWords(c.peak), c.peak_words, c.name);
  }
  assert.equal(peakWords(1.5), 'peak +1.5 dB');
  assert.equal(peakWords(-0.3), 'peak −0.3 dB');
  assert.equal(peakWords(12), 'peak +12.0 dB');
});

ok('markerPoint: x of the band\'s frequency, y of its own gain in EQ and Flat, 0 dB else', () => {
  for (const c of fixture.cases) {
    c.bands.forEach((b, i) => {
      const got = markerPoint(c.mode, { f0: b.f0, gainDb: b.gain_db });
      near(got.x, c.markers[i].x, 1e-9, `${c.name} marker ${i} x`);
      near(got.y, c.markers[i].y, 1e-9, `${c.name} marker ${i} y`);
    });
  }
  const shaped = fixture.cases.find((c) => c.name === 'loudness mode with shaped bands');
  assert.ok(shaped.markers.every((m) => m.y === 130), 'Loudness sits every marker at 0 dB');
});

ok('xOf and yOf: the core\'s maps, y clamped to the range', () => {
  for (const [hz, x] of fixture.maps.x) near(xOf(hz), x, 1e-9, `x(${hz})`);
  for (const [db, y] of fixture.maps.y) near(yOf(db), y, 1e-9, `y(${db})`);
  assert.equal(yOf(20), 0);
  assert.equal(yOf(-20), 260);
  assert.equal(yOf(0), 130);
});

ok('hzAt and dbAt: inverses of the maps, not clamped', () => {
  for (const [hz] of fixture.maps.x) near(hzAt(xOf(hz)), hz, 1e-9 * hz, `hzAt(xOf(${hz}))`);
  for (let d = -12.9; d < 13; d += 0.7) near(dbAt(yOf(d)), d, 1e-9, `dbAt(yOf(${d}))`);
  near(hzAt(-100), 20 * Math.exp(-0.1 * Math.log(1000)), 1e-9, 'left of the plot');
  near(dbAt(300), -17, 1e-9, 'below the plot');
  near(dbAt(-50), 18, 1e-9, 'above the plot');
});

ok('linePath: every case\'s line is the core\'s path, point for point, within 0.051', () => {
  // Rust rounds a tie to even and the script up: a number may differ by one in the last digit.
  const numbers = (path) => path.split(/[ML]/).slice(1)
    .flatMap((pair) => pair.split(' ').map(Number));
  for (const c of fixture.cases) {
    const ys = responseDb(bandsOf(c), fixture.freqs).map(yOf);
    const got = numbers(linePath(ys));
    const want = numbers(c.line);
    assert.equal(got.length, want.length, `${c.name}: ${got.length} numbers`);
    assert.equal(got.length, 2 * 181, c.name);
    got.forEach((v, i) => near(v, want[i], 0.051, `${c.name} number ${i}`));
  }
});

ok('linePath: 181 points over the log frequencies', () => {
  const ys = fixture.cases[1].ys;
  const path = linePath(ys);
  assert.equal(path.split(/(?=[ML])/).length, 181);
  assert.ok(path.startsWith('M0.0 130.0L'), path.slice(0, 30));
  const end = path.slice(path.lastIndexOf('L'));
  assert.equal(end, `L1000.0 ${ys[180].toFixed(1)}`);
  const flat = linePath(new Array(181).fill(130));
  assert.ok(flat.startsWith('M0.0 130.0L5.6 130.0L'), flat.slice(0, 30));
});

console.log(`tone-curve.mjs: ${n} checks passed`);
