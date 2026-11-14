/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's glow (themes/nightdrive/draw/): it is drawn, never blurred
// (`shadowBlur` is slow where a canvas is drawn in software): a gradient round the sun, the ridge's edge
// and the bubbles and diamonds drawn wider and fainter under themselves. With `geo.glow` false none of it is
// drawn. The canvas is a recorder, no DOM. Run: node nightdrive-glow.mjs
import assert from 'node:assert/strict';
import { BANDS } from '../../../bins/svolderd/web/core/viz-model.js';
import { drawRidge, drawRoad } from '../../../bins/svolderd/web/themes/nightdrive/draw/road.js';
import { farRow } from '../../../bins/svolderd/web/themes/nightdrive/camera.js';
import { drawWet, WET } from '../../../bins/svolderd/web/themes/nightdrive/draw/mood.js';
import { drawSky } from '../../../bins/svolderd/web/themes/nightdrive/draw/sky.js';
import { drawBubbles, drawDiamonds } from '../../../bins/svolderd/web/themes/nightdrive/draw/tune.js';
import { createFloor } from '../../../bins/svolderd/web/themes/nightdrive/floor.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** A canvas that keeps every property set and every call that matters here. */
function recorder() {
  const sets = [];
  const calls = [];
  const state = { globalAlpha: 1, lineWidth: 1 };
  const g = new Proxy(state, {
    get(target, name) {
      if (name in target) return target[name];
      if (name === 'createLinearGradient' || name === 'createRadialGradient') {
        return (...args) => {
          calls.push({ op: name, args });
          return { addColorStop() {} };
        };
      }
      if (name === 'drawImage') return (...args) => calls.push({ op: 'drawImage', args });
      if (name === 'stroke') return () => calls.push({ op: 'stroke', width: target.lineWidth, alpha: target.globalAlpha });
      return () => {};
    },
    set(target, name, value) {
      sets.push([name, value]);
      target[name] = value;
      return true;
    },
  });
  return { g, sets, calls };
}

const W = 1176;
const H = 800;
const tune = {
  m: 1,
  curve: () => 0.5 * H,
  held: 2,
  bubbles: [{ band: 1, x: 200, y: 300 }, { band: 2, x: 500, y: 320 }],
  bell: { held: 'lo', diamonds: [{ side: 'lo', x: 400, y: 330 }, { side: 'hi', x: 600, y: 330 }] },
};

/** Draw the whole scene once with `glow` and `dull`; the recorder. */
function scene(glow, dull = false) {
  const r = recorder();
  const geo = { w: W, h: H, H: 0.56 * H, dpr: 2, glow };
  const frame = { t: 1, kick: 0.5, dull, flat: false, bass: 0, rays: 0, stars: [], tune, swell: 0.2 };
  drawSky(r.g, geo, frame);
  drawRidge(r.g, geo, new Float32Array(BANDS).fill(0.3), frame);
  drawRoad(r.g, geo, createFloor({}), frame);
  drawDiamonds(r.g, geo, tune);
  drawBubbles(r.g, geo, tune);
  return r;
}

ok('nothing_in_the_scene_is_blurred', () => {
  for (const glow of [true, false]) {
    for (const dull of [false, true]) {
      const { sets } = scene(glow, dull);
      const blurred = sets.filter(([name, v]) => name === 'shadowBlur' && v !== 0);
      assert.deepEqual(blurred, [], `glow ${glow} dull ${dull}`);
      assert.deepEqual(sets.filter(([name]) => name === 'shadowColor'), [], 'no shadow colour either');
    }
  }
});

ok('the_suns_glow_is_a_gradient_round_it_and_is_left_out_when_the_glow_is_off', () => {
  const radial = (r) => r.calls.filter((c) => c.op === 'createRadialGradient');
  assert.equal(radial(scene(true)).length, 1);
  assert.equal(radial(scene(false)).length, 0);
  const [one] = radial(scene(true));
  assert.ok(one.args[5] > one.args[2] + 20, 'it reaches well beyond the sun\'s edge');
});

ok('the_ridges_edge_and_the_bubbles_glow_with_wider_fainter_strokes_under_the_line', () => {
  const lit = scene(true).calls.filter((c) => c.op === 'stroke');
  const dark = scene(false).calls.filter((c) => c.op === 'stroke');
  assert.ok(lit.length > dark.length + 6, `${lit.length} strokes with the glow, ${dark.length} without`);
  const wide = lit.filter((c) => c.width >= 6 && c.alpha < 0.4);
  assert.ok(wide.length >= 7, `wide faint strokes: ${wide.length}`);
  assert.equal(dark.filter((c) => c.width >= 6 && c.alpha < 0.4).length, 0);
});

ok('the_wet_roads_slices_sway_as_one_wave', () => {
  const geo = { w: 1280, h: 800, H: 448, dpr: 1 };
  const source = { width: 1280, height: 448 };
  for (let t = 0; t < 12; t += 0.37) {
    const r = recorder();
    drawWet(r.g, source, geo, 1, t);
    const offsets = r.calls.filter((c) => c.op === 'drawImage').map((c) => c.args[5]);
    assert.ok(offsets.length >= WET.slices - 4, `slices drawn: ${offsets.length}`);
    for (let k = 1; k < offsets.length; k++) {
      assert.ok(Math.abs(offsets[k] - offsets[k - 1]) < 1, `t ${t}: slices ${k - 1} and ${k} differ by ${offsets[k] - offsets[k - 1]}`);
    }
    // one wave: every slice leans the same way, the near ones further
    const lean = Math.sign(offsets[offsets.length - 1]);
    assert.ok(offsets.every((x) => Math.sign(x) === lean || Math.abs(x) < 1e-9), `t ${t}: slices lean both ways`);
    assert.ok(Math.abs(offsets[offsets.length - 1]) >= Math.abs(offsets[0]), 'the sway grows with nearness');
  }
});

ok('the_wet_road_lies_only_on_the_floor_grid_never_above_its_far_row', () => {
  // the last is a phone held upright: the scene's height h is more than the picture's (geo.v)
  for (const [w, h, Hh] of [[1280, 800, 448], [1920, 1080, 0.56 * 1080], [390, 844, 0.56 * 700]]) {
    const geo = { w, h, H: Hh, dpr: 2 };
    const source = { width: w * 2, height: Hh * 2 };
    const yFar = farRow(h, Hh);
    const r = recorder();
    drawWet(r.g, source, geo, 1, 1);
    const draws = r.calls.filter((c) => c.op === 'drawImage');
    assert.ok(draws.length > 0, `${w}x${h}: something is drawn`);
    for (const c of draws) assert.ok(c.args[6] >= yFar - 1e-6, `${w}x${h}: slice at ${c.args[6]} above the far row ${yFar}`);
    // below the far row the reflection is neither rescaled nor moved: every slice keeps the scale `reach` and reads
    // the source where the mirror puts it, the cut one included
    const dh = (h - Hh) / WET.slices + 1;
    for (const c of draws) {
      assert.ok(Math.abs(c.args[4] / c.args[8] - WET.reach * 2) < 1e-9, `${w}x${h}: slice at ${c.args[6]} is rescaled`);
      const want = Hh - (c.args[6] + c.args[8] - dh - Hh) * WET.reach;
      assert.ok(Math.abs((c.args[2] + c.args[4]) / 2 - want) < 1e-9, `${w}x${h}: slice at ${c.args[6]} reads the source bottom ${(c.args[2] + c.args[4]) / 2}, not ${want}`);
    }
    const first = Math.min(...draws.map((c) => c.args[6]));
    assert.ok(first < yFar + 1, `${w}x${h}: the reflection starts at the far row (${first} vs ${yFar})`);
  }
});

console.log(`${n} glow checks passed`);
