/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's floor as a heat grid (themes/nightdrive/draw/road.js and
// heat.js): the grid's crossings are lifted by their row's level, a loud cell is lifted and hotter than a
// quiet one, a near crossing is lifted more than the same far one, the strips are drawn far first, the rows
// roll toward the viewer and a floor that is not rolling is drawn the same twice, and the wet road thins
// the ground. The canvas is a recorder, no DOM. Run: node nightdrive-floor.mjs
import assert from 'node:assert/strict';
import { COLUMNS } from '../../../bins/svolderd/web/themes/nightdrive/axis.js';
import { CAMERA } from '../../../bins/svolderd/web/themes/nightdrive/camera.js';
import { FLOOR, drawRoad, heatLevel, waveAt } from '../../../bins/svolderd/web/themes/nightdrive/draw/road.js';
import {
  RAMP, RAMP_COOL, RAMP_WARM, STEPS, heatColours, heatIndex, heatOf, rampFor,
} from '../../../bins/svolderd/web/themes/nightdrive/draw/heat.js';
import { PALETTE } from '../../../bins/svolderd/web/themes/nightdrive/draw/palette.js';
import { PEAK_TAU, createFloor } from '../../../bins/svolderd/web/themes/nightdrive/floor.js';
import { NEUTRAL } from '../../../bins/svolderd/web/themes/nightdrive/look.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const GEO = { w: 1280, h: 800, H: 448, dpr: 1 };

/** The strips of the floor drawn: between its row edges, those of the scene's rows and the extra beyond the bottom. */
const STRIPS = CAMERA.rows + FLOOR.extra - 1;

/** A canvas that keeps what is filled and stroked: the path's points, the style and the alpha. */
function recorder() {
  const log = [];
  const state = { globalAlpha: 1, fillStyle: '', strokeStyle: '', lineWidth: 1 };
  let path = [];
  const g = new Proxy(state, {
    get(target, name) {
      if (name in target) return target[name];
      if (name === 'beginPath') return () => { path = []; };
      if (name === 'moveTo' || name === 'lineTo') return (x, y) => { path.push([x, y]); };
      if (name === 'fill') return () => log.push({ op: 'fill', pts: path.slice(), style: target.fillStyle, alpha: target.globalAlpha });
      if (name === 'stroke') return () => log.push({ op: 'stroke', pts: path.slice(), width: target.lineWidth, alpha: target.globalAlpha });
      if (name === 'createLinearGradient') return () => ({ addColorStop() {} });
      return () => {};
    },
    set(target, name, value) { target[name] = value; return true; },
  });
  return { g, log };
}

/** A floor of `rows` rows born from `level(k)` for row `k` counting from the newest. */
function floorOf(level, rows = CAMERA.rows + FLOOR.extra) {
  const floor = createFloor({ rows });
  // the oldest first, so that row 0 ends up the newest
  for (let k = rows - 1; k >= 0; k--) floor.advance(1, 1, new Float32Array(COLUMNS).fill(level(k)), true);
  return floor;
}

const draw = (floor, frame = { dull: false }) => {
  const r = recorder();
  drawRoad(r.g, GEO, floor, frame);
  return r.log;
};

/** The heat of each cell (four corners), strip by strip, `COLUMNS` to a strip: neighbours of one colour and alpha are
 * filled as one path, which this cuts into its cells again. The ground's path is not one of them. */
const cells = (log) => log.filter((e) => e.op === 'fill' && e.style !== PALETTE.ground)
  .flatMap((e) => Array.from({ length: e.pts.length / 4 }, (_, k) => ({ ...e, pts: e.pts.slice(4 * k, 4 * k + 4) })));

const green = (style) => Number(/rgb\(\d+,(\d+),/.exec(style)[1]);

ok('a_quiet_row_is_flat', () => {
  const silent = cells(draw(floorOf(() => 0)));
  assert.equal(silent.length, STRIPS * COLUMNS);
  // every cell of a strip has its far corners on one line, its near corners on another
  for (let s = 0; s < STRIPS; s++) {
    const strip = silent.slice(s * COLUMNS, (s + 1) * COLUMNS);
    for (const c of strip) {
      assert.ok(Math.abs(c.pts[0][1] - strip[0].pts[0][1]) < 1e-3 && Math.abs(c.pts[3][1] - strip[0].pts[3][1]) < 1e-3, `strip ${s}`);
    }
  }
  const lifted = cells(draw(floorOf(() => 0.9)));
  assert.ok(lifted[100].pts[0][1] < silent[100].pts[0][1] - 1, 'a loud floor is not flat');
  const idle = cells(draw(createFloor({ rows: CAMERA.rows + FLOOR.extra })));
  assert.equal(idle.length, STRIPS * COLUMNS, 'no row born yet: drawn, nearly flat');
});

ok('a_loud_cell_is_lifted_and_hotter_than_a_quiet_one', () => {
  const quiet = cells(draw(floorOf(() => 0.1)));
  const loud = cells(draw(floorOf((k) => (k === 8 ? 0.95 : 0.1))));
  const at = 8 * COLUMNS + 10;
  const rise = quiet[at].pts[0][1] - loud[at].pts[0][1];
  assert.ok(rise > 5, `lifted by ${rise}`);
  // the heat is read against the floor's own peak, so a cell is hotter than the quiet ones beside it in the same floor
  const calm = loud[3 * COLUMNS + 10];
  assert.ok(green(loud[at].style) > green(calm.style) + 40, `${loud[at].style} against ${calm.style}`);
  assert.ok(loud[at].alpha > 1.5 * calm.alpha, `${loud[at].alpha} against ${calm.alpha}`);
  const before = 7 * COLUMNS + 10;
  assert.ok(quiet[before].pts[3][1] - loud[before].pts[3][1] > 5, 'the strip before it shares the lifted edge');
  assert.ok(loud[3 * COLUMNS + 10].pts[0][1] === quiet[3 * COLUMNS + 10].pts[0][1], 'a row two strips away does not move');
});

ok('a_crossing_of_level_1_at_the_nearest_row_is_lifted_by_the_floors_height', () => {
  const quiet = cells(draw(floorOf(() => 0)));
  const loud = cells(draw(floorOf(() => 1)));
  const last = (CAMERA.rows - 1) * COLUMNS + 10;
  const lift = quiet[last].pts[3][1] - loud[last].pts[3][1];
  // the row edge at the bottom of the scene is at depth 1: a crossing of level 1 there is lifted by the floor's height
  assert.ok(Math.abs(lift - FLOOR.height * GEO.h) < 1e-3, `${lift}`);
  const far = quiet[10].pts[0][1] - loud[10].pts[0][1];
  const near = quiet[(CAMERA.rows - 2) * COLUMNS + 10].pts[0][1] - loud[(CAMERA.rows - 2) * COLUMNS + 10].pts[0][1];
  assert.ok(near > 3 * far, `near ${near} far ${far}`);
  assert.equal(FLOOR.height, 0.25);
});

const luma = (style) => {
  const [r, g, b] = /rgb\((\d+),(\d+),(\d+)/.exec(style).slice(1).map(Number);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

ok('the_low_levels_of_real_music_are_five_colours_apart_under_the_floors_own_peak', () => {
  assert.deepEqual([FLOOR.heatFloor, FLOOR.heatCurve], [0.15, 0.6]);
  const colours = heatColours(0);
  const levels = [0.05, 0.1, 0.2, 0.3, 0.5];
  const steps = (at) => levels.map((v) => luma(colours[heatIndex(at(v))]));
  const gaps = (lumas) => lumas.slice(1).map((l, i) => l - lumas[i]);
  const now = gaps(steps((v) => heatLevel(v, 0.54)));
  assert.ok(now.every((d) => d >= FLOOR.lumaStep), `the steps now: ${now}`);
  // the absolute reading of the level (before the floor's own peak and the curve) does not tell the low levels apart
  const before = gaps(steps((v) => v));
  assert.ok(before.some((d) => d < FLOOR.lumaStep), `the steps before: ${before}`);
  assert.equal(new Set(levels.map((v) => heatIndex(heatLevel(v, 0.54)))).size, 5);
});

ok('a_steady_quiet_floor_reaches_mid_ramp_within_a_minute_and_so_does_one_that_was_loud', () => {
  const mid = STEPS / 2;
  const floor = createFloor({});
  const quiet = new Float32Array(COLUMNS).fill(0.1);
  for (let t = 0; t < 60; t++) floor.advance(1, 3, quiet, true);
  assert.ok(heatIndex(heatLevel(0.1, floor.peak)) >= mid, `a quiet floor: ${heatIndex(heatLevel(0.1, floor.peak))}`);
  // one that was loud falls toward its new range with the peak's time constant
  const was = createFloor({});
  was.advance(1, 3, new Float32Array(COLUMNS).fill(0.54), true);
  assert.ok(Math.abs(was.peak - 0.54) < 1e-6);
  for (let t = 0; t < PEAK_TAU; t++) was.advance(1, 3, quiet, true);
  assert.ok(Math.abs(was.peak - (0.1 + 0.44 * Math.exp(-1))) < 0.01, `${was.peak}`);
  assert.ok(heatIndex(heatLevel(0.1, was.peak)) >= mid, `a floor that was loud: ${heatIndex(heatLevel(0.1, was.peak))}`);
  // the peak is the loudest cell as the fill averages it: a lone column of 0.9 makes cells of 0.45, so the hot end is reachable
  const spike = createFloor({});
  const lone = new Float32Array(COLUMNS);
  lone[10] = 0.9;
  spike.advance(1, 1, lone, true);
  assert.ok(Math.abs(spike.peak - 0.45) < 1e-6, `${spike.peak}`);
  // one peak for the whole floor: the bass-left, treble-right tilt of the music is kept
  const tilt = createFloor({});
  const cols = Float32Array.from({ length: COLUMNS }, (_, i) => 0.5 - 0.3 * (i / (COLUMNS - 1)));
  tilt.advance(1, 1, cols, true);
  assert.ok(heatLevel(cols[0], tilt.peak) > heatLevel(cols[COLUMNS - 1], tilt.peak));
  // the lift keeps the absolute level: the same cell is lifted as high under any peak
  const lifted = (peak) => {
    const f = floorOf(() => 0.3);
    f.advance(1, 0, new Float32Array(COLUMNS).fill(peak), false);
    return { top: cells(draw(f))[100].pts[0][1], peak: f.peak };
  };
  const [low, high] = [lifted(0.3), lifted(0.9)];
  assert.ok(high.peak > low.peak + 0.5, 'the peak moved');
  assert.equal(high.top, low.top);
});

ok('strips_are_drawn_far_first', () => {
  const log = draw(floorOf(() => 0.8));
  const feet = cells(log).filter((_, i) => i % COLUMNS === 0).map((c) => c.pts[3][1]);
  assert.equal(feet.length, STRIPS);
  assert.deepEqual(feet, [...feet].sort((a, b) => a - b), 'the near edges go down the screen: far first');
  // each strip: its ground, then its heat (one fill where the cells are alike), then its lines
  const ops = log.map((e) => (e.op === 'fill' ? (e.style === PALETTE.ground ? 'ground' : 'heat') : 'stroke'));
  const first = ops.indexOf('heat');
  assert.equal(ops[first - 1], 'ground', 'the ground comes before the heat');
  assert.equal(ops[ops.indexOf('stroke', first) - 1], 'heat', 'the lines come after the strip\'s cells');
  assert.ok(ops.indexOf('stroke', first) > first && ops.slice(0, ops.indexOf('stroke', first)).filter((o) => o === 'ground').length === 1,
    'the first strip is whole before the second\'s ground');
});

ok('the_rows_roll_toward_the_viewer_and_a_floor_not_rolling_is_drawn_the_same', () => {
  const floor = floorOf(() => 0.5);
  const before = JSON.stringify(draw(floor));
  floor.advance(0.1, 1, new Float32Array(COLUMNS).fill(0.5), false);
  assert.equal(JSON.stringify(draw(floor)), before, 'Still: nothing moved');
  const y0 = cells(draw(floor))[15 * COLUMNS].pts[3][1];
  floor.advance(0.4, 1, new Float32Array(COLUMNS).fill(0.5), true);
  const y1 = cells(draw(floor))[15 * COLUMNS].pts[3][1];
  assert.ok(y1 > y0 + 1, `${y0} then ${y1}`);
});

ok('the_heat_follows_the_ramp_and_leans_warm_for_bass_and_cool_for_highs', () => {
  assert.equal(heatColours(0).length, STEPS);
  assert.deepEqual(heatOf(0, RAMP), RAMP[0]);
  assert.ok(heatOf(1, RAMP)[0] > 250 && heatOf(1, RAMP)[1] > 220, 'the top is a pale yellow');
  assert.deepEqual(rampFor(0), RAMP);
  assert.deepEqual(rampFor(-1), RAMP_WARM.map((c) => [...c]));
  assert.deepEqual(rampFor(1), RAMP_COOL.map((c) => [...c]));
  const warm = heatOf(0.5, rampFor(-1));
  const cool = heatOf(0.5, rampFor(1));
  assert.ok(warm[0] > cool[0] && cool[2] > warm[2], `${warm} ${cool}`);
  assert.equal(heatColours(0.5), heatColours(0.5), 'one array for one lean');
  assert.equal(heatIndex(0), 0);
  assert.equal(heatIndex(1), STEPS - 1);
  assert.notEqual(heatColours(-1)[40], heatColours(1)[40]);
});

ok('cells_alike_in_colour_and_alpha_are_filled_together_and_unlike_ones_apart', () => {
  const fills = (log) => log.filter((e) => e.op === 'fill' && e.style !== PALETTE.ground).length;
  const even = fills(draw(floorOf(() => 0.3)));
  const varied = fills(draw(floorOf((k) => (k % 2 ? 0.1 : 0.9))));
  assert.ok(even <= 3 * STRIPS, `an even floor takes ${even} fills (the road's edge cells, half lifted, are their own)`);
  const ragged = fills(draw((() => {
    const floor = createFloor({});
    for (let k = 0; k < CAMERA.rows + FLOOR.extra; k++) floor.advance(1, 1, Float32Array.from({ length: COLUMNS }, (_, i) => i / (COLUMNS - 1)), true);
    return floor;
  })()));
  assert.ok(ragged > 8 * STRIPS, `cells rising across the road take ${ragged} fills`);
  assert.ok(varied <= 3 * STRIPS, 'rows alike across the road are filled as one however much the rows differ');
});

ok('a_lifted_near_edge_still_has_floor_under_it_to_the_bottom_of_the_scene', () => {
  const low = (log) => Math.max(...cells(log).flatMap((c) => c.pts.map((p) => p[1])));
  assert.ok(low(draw(floorOf(() => 0.7))) >= GEO.h, `${low(draw(floorOf(() => 0.7)))}`);
  const floor = floorOf(() => 0.7);
  floor.advance(0.6, 1, new Float32Array(COLUMNS).fill(0.7), true);
  assert.ok(low(draw(floor)) >= GEO.h, 'rolled by half a row too');
  // at the floor's height of 0.25 a loud floor, at any roll, has every column's last cell down to the bottom
  for (const frac of [0, 0.25, 0.5, 0.75, 0.99]) {
    for (const level of [0.5, 1]) {
      const f = floorOf(() => level);
      f.advance(frac, 1, new Float32Array(COLUMNS).fill(level), true);
      const last = cells(draw(f)).slice(-COLUMNS);
      assert.equal(last.length, COLUMNS);
      assert.ok(last.every((c) => Math.max(...c.pts.map((p) => p[1])) >= GEO.h - 1e-6), `level ${level} rolled ${frac}`);
    }
  }
});

ok('the_near_edge_moves_continuously_while_the_rows_roll_across_a_birth_at_any_level', () => {
  // the y of each strip's near edge at the first column, by the edge's own place in the floor; a birth shifts the
  // strips by one, so the same edge is compared with itself
  const edges = (f) => {
    const out = [];
    for (const c of cells(draw(f)).filter((_, i) => i % COLUMNS === 0)) out.push(c.pts[3][1]);
    return out;
  };
  for (const level of [0.3, 1]) {
    const f = floorOf(() => level);
    const row = new Float32Array(COLUMNS).fill(level);
    let was = edges(f);
    let whole = f.whole;
    let worst = 0;
    let births = 0;
    for (let step = 0; step < 300; step++) {
      f.advance(0.01, 1, row, true);
      const now = edges(f);
      const shift = f.whole - whole;
      births += shift;
      whole = f.whole;
      for (let k = 0; k + shift < now.length && k < was.length; k++) {
        // what is on the screen and one row spacing below it (an edge popping out of sight counts); an edge far below
        // flies by faster than the eye can see
        const reach = GEO.h + ((GEO.h - GEO.H) * CAMERA.K) / CAMERA.rows;
        if (was[k] <= reach || now[k + shift] <= reach) worst = Math.max(worst, Math.abs(now[k + shift] - was[k]));
      }
      was = now;
    }
    assert.ok(births >= 2, 'the rows were born');
    // a row spacing near the bottom is a few hundred px a whole row: 0.01 of it is a few px; a pop is the lift, 100 px or more
    assert.ok(worst < 12, `level ${level}: an edge moved ${worst.toFixed(1)} px in one 0.01 step`);
  }
});

ok('the_wet_road_thins_the_ground_so_the_reflection_shows', () => {
  const ground = (look) => draw(floorOf(() => 0.3), { dull: false, look }).filter((e) => e.op === 'fill' && e.style === PALETTE.ground).map((e) => e.alpha);
  assert.ok(ground({ ...NEUTRAL, wet: 0 }).every((a) => a === 1), 'dry: the ground is whole');
  const wet = ground({ ...NEUTRAL, wet: 0.6 });
  assert.equal(wet.length, STRIPS);
  assert.ok(wet.every((a) => Math.abs(a - (1 - 0.85 * 0.6)) < 1e-9), `${wet[0]}`);
});

ok('the_mood_dims_the_lines_and_the_heat', () => {
  const alphas = (mood) => {
    const log = draw(floorOf(() => 0.6), { dull: false, look: { ...NEUTRAL, mood } });
    return [Math.max(...cells(log).map((e) => e.alpha)), Math.max(...log.filter((e) => e.op === 'stroke').map((e) => e.alpha))];
  };
  const deep = alphas(0.18);
  const warm = alphas(1);
  assert.ok(warm[0] > deep[0] * 1.2 && warm[1] > deep[1] * 1.1, `${deep} ${warm}`);
});

ok('a_wave_runs_down_the_floor_from_the_horizon_and_fades', () => {
  assert.equal(waveAt(3, 5, 0), 0, 'no wave: nothing');
  const early = Array.from({ length: CAMERA.rows }, (_, k) => waveAt(k + 0.5, 0.1, 1));
  const later = Array.from({ length: CAMERA.rows }, (_, k) => waveAt(k + 0.5, 0.5, 1));
  assert.equal(early.indexOf(Math.max(...early)), 1);
  assert.ok(later.indexOf(Math.max(...later)) > 8, 'the front has moved down');
  assert.ok(Math.max(...early) <= FLOOR.waveAlpha + 1e-9);
  // a floor whose peak is well above its cells (a cell at its own peak is already as bright as it gets)
  const under = () => {
    const f = floorOf(() => 0.3);
    f.advance(1, 0, new Float32Array(COLUMNS).fill(0.9), false);
    return f;
  };
  const calm = cells(draw(under(), { dull: false, look: { ...NEUTRAL, wave: 1, since: 0.3 } }));
  const flat = cells(draw(under(), { dull: false, look: NEUTRAL }));
  assert.ok(calm.some((c, i) => c.alpha > flat[i].alpha + 0.1), 'a cell in the front glows');
});

console.log(`${n} floor checks passed`);
