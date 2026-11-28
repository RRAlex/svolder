/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's palette (themes/voidgarden/palette.js): a two-colour cover gives
// those two hues, a one-colour cover a hue and its complement, a grey, empty or missing cover the
// pair the track's title and artist pick from the scene's eight; every colour is lifted into the
// pairs' lightness and saturation range. Run: node voidgarden-palette.mjs
import assert from 'node:assert/strict';
import {
  LIGHTNESS, PALETTES, SATURATION_MIN, fromCover, hex, palette, seeded,
} from '../../../bins/svolderd/web/themes/voidgarden/palette.js';
import { hashStr, mulberry } from '../../../bins/svolderd/web/themes/voidgarden/rng.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** The hue (0-1), saturation and lightness of `#rrggbb`. */
function hslOf(text) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(text.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  let h;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [(h / 6 + 1) % 1, s, l];
}
const hueGap = (a, b) => Math.abs(((a - b + 1.5) % 1) - 0.5);

/** A 32x32 RGBA cover: `colourAt(x, y)` is `[r, g, b, a?]`. */
const cover = (colourAt) => {
  const px = new Uint8ClampedArray(32 * 32 * 4);
  for (let y = 0; y < 32; y++) {
    for (let x = 0; x < 32; x++) {
      const [r, g, b, a = 255] = colourAt(x, y);
      px.set([r, g, b, a], (y * 32 + x) * 4);
    }
  }
  return px;
};
const RED = [210, 40, 50];
const BLUE = [40, 70, 220];
const GREEN = [50, 200, 70];

ok('a_two_colour_cover_gives_those_two_hues_the_larger_first', () => {
  const [a, b] = palette(cover((x) => (x < 20 ? RED : BLUE)), { title: 't', artist: 'a' });
  const red = hslOf('#d22832')[0];
  const blue = hslOf('#2846dc')[0];
  assert.ok(hueGap(hslOf(a)[0], red) < 0.03, `first ${a}`);
  assert.ok(hueGap(hslOf(b)[0], blue) < 0.03, `second ${b}`);
  const [c, d] = palette(cover((x) => (x < 12 ? RED : GREEN)), {});
  assert.ok(hueGap(hslOf(c)[0], hslOf('#32c846')[0]) < 0.03 && hueGap(hslOf(d)[0], red) < 0.03, 'the bigger area is first');
});

ok('shading_noise_and_a_few_stray_pixels_do_not_move_the_hues', () => {
  const rng = mulberry(5);
  const noisy = (c) => c.map((v) => Math.max(0, Math.min(255, v + Math.round((rng() - 0.5) * 36))));
  const [a, b] = palette(cover((x, y) => (y === 0 && x < 3 ? [240, 220, 20] : noisy(x < 16 ? RED : BLUE))), {});
  const hues = [hslOf(a)[0], hslOf(b)[0]];
  const red = hslOf('#d22832')[0];
  const blue = hslOf('#2846dc')[0];
  assert.ok(hueGap(hues[0], red) < 0.04 || hueGap(hues[0], blue) < 0.04);
  assert.ok(hueGap(hues[1], red) < 0.04 || hueGap(hues[1], blue) < 0.04);
  assert.ok(hueGap(hues[0], hues[1]) > 0.3);
});

ok('colours_are_lifted_into_the_scenes_lightness_and_saturation', () => {
  const dark = [40, 5, 10];
  const dull = [140, 120, 120];
  const pale = [255, 235, 240];
  for (const [p, q] of [[dark, [5, 10, 40]], [dull, [120, 120, 140]], [pale, [235, 240, 255]]]) {
    const got = fromCover(cover((x) => (x < 16 ? p : q)));
    assert.ok(got, 'some colour');
    for (const text of got) {
      const [, s, l] = hslOf(text);
      assert.ok(l >= LIGHTNESS[0] - 0.01 && l <= LIGHTNESS[1] + 0.01, `${text} lightness ${l}`);
      assert.ok(s >= SATURATION_MIN - 0.02, `${text} saturation ${s}`);
    }
  }
  for (const pair of PALETTES) {
    for (const text of pair) {
      const [, s, l] = hslOf(text);
      assert.ok(l >= LIGHTNESS[0] - 0.01 && l <= LIGHTNESS[1] + 0.01 && s >= SATURATION_MIN - 0.01, `own colour ${text}`);
    }
  }
});

ok('a_one_colour_cover_gives_its_hue_and_the_complement', () => {
  const [a, b] = palette(cover(() => RED), {});
  assert.ok(hueGap(hslOf(a)[0], hslOf('#d22832')[0]) < 0.03);
  assert.ok(Math.abs(hueGap(hslOf(a)[0], hslOf(b)[0]) - 0.5) < 0.03, `${a} ${b}`);
});

ok('a_grey_empty_or_missing_cover_gives_the_seeded_pair', () => {
  const grey = cover((x, y) => { const v = (x * 8 + y * 3) % 256; return [v, v, v]; });
  const want = seeded('Song', 'Artist');
  assert.deepEqual(palette(grey, { title: 'Song', artist: 'Artist' }), want);
  assert.deepEqual(palette(null, { title: 'Song', artist: 'Artist' }), want);
  assert.deepEqual(palette(new Uint8ClampedArray(0), { title: 'Song', artist: 'Artist' }), want);
  assert.deepEqual(palette(cover(() => [200, 30, 30, 0]), { title: 'Song', artist: 'Artist' }), want, 'transparent');
  assert.equal(fromCover(grey), null);
  const faint = cover((x) => (x < 16 ? [128, 124, 124] : [124, 124, 128]));
  assert.equal(fromCover(faint), null, 'a faint tint is no colour');
  assert.equal(fromCover(null), null);
  assert.equal(fromCover(cover(() => [0, 0, 0])), null, 'black');
  assert.equal(fromCover(cover(() => [255, 255, 255])), null, 'white');
});

ok('the_seeded_pair_is_one_of_the_eight_and_is_the_same_for_the_same_track', () => {
  const seen = new Set();
  for (let i = 0; i < 60; i++) {
    const pair = seeded(`Track ${i}`, 'The Band');
    assert.ok(PALETTES.some((p) => p[0] === pair[0] && p[1] === pair[1]), 'one of the eight');
    assert.deepEqual(pair, seeded(`Track ${i}`, 'The Band'));
    seen.add(pair[0]);
  }
  assert.equal(seen.size, 8, 'all eight turn up');
  // the rule: seed = hash % 10000 / 100, pair = floor(seed * 7.31) % 8
  const seed = (hashStr('Song\u0000Artist') % 10000) / 100;
  assert.deepEqual(seeded('Song', 'Artist'), [...PALETTES[Math.floor(seed * 7.31) % 8]]);
  assert.deepEqual(seeded(undefined, undefined), seeded('', ''));
});

ok('hex_writes_hsl_as_six_digits', () => {
  assert.equal(hex(0, 1, 0.5), '#ff0000');
  assert.equal(hex(1 / 3, 1, 0.5), '#00ff00');
  assert.equal(hex(2 / 3, 1, 0.5), '#0000ff');
  assert.equal(hex(0, 0, 1), '#ffffff');
  assert.equal(hex(0, 0, 0), '#000000');
});

console.log(`${n} ok`);
