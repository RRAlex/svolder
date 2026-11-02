/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for effects.js's pure rules: stored, keep, pick. Run: node effects.mjs
import assert from 'node:assert/strict';
import {
  KEY, LEVELS, MUSIC, backgroundAlpha, beat, glowLevel, keep, level, loudness, music, pick, stored,
} from '../../../bins/svolderd/web/themes/ember/effects.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fake = (initial, { readThrows = false, writeThrows = false } = {}) => {
  const data = new Map(initial === undefined ? [] : [[KEY, initial]]);
  return {
    data,
    getItem(k) {
      if (readThrows) throw new Error('denied');
      return data.has(k) ? data.get(k) : null;
    },
    setItem(k, v) { if (writeThrows) throw new Error('quota'); data.set(k, v); },
  };
};

ok('constants', () => {
  assert.deepEqual(LEVELS, ['smoke', 'glow', 'off']);
  assert.equal(KEY, 'svolder.effects');
});
ok('stored: each level reads back', () => {
  for (const l of LEVELS) assert.equal(stored(fake(l)), l);
});
ok('stored: absent, unknown, throwing, no storage', () => {
  assert.equal(stored(fake()), null);
  assert.equal(stored(fake('banana')), null);
  assert.equal(stored(fake('')), null);
  assert.equal(stored(fake('Glow')), null);
  assert.equal(stored(fake('glow', { readThrows: true })), null);
  assert.equal(stored(null), null);
  assert.equal(stored(undefined), null);
});
ok('keep: writes, and says false when refused', () => {
  const s = fake();
  assert.equal(keep(s, 'off'), true);
  assert.equal(s.data.get(KEY), 'off');
  assert.equal(keep(fake(undefined, { writeThrows: true }), 'glow'), false);
  assert.equal(keep(null, 'glow'), false);
});
ok('keep: refuses a level that is not one', () => {
  const s = fake();
  assert.equal(keep(s, 'banana'), false);
  assert.equal(s.data.size, 0);
});

const yes = { ok: true, why: null };
const no = { ok: false, why: 'because' };
ok('pick: every row of asked x stored x reduced x can', () => {
  const opts = [null, ...LEVELS];
  for (const asked of opts) for (const st of opts) for (const reduced of [false, true]) {
    for (const can of [yes, no]) {
      const chosen = asked ?? st ?? (reduced ? 'glow' : 'smoke');
      const level = chosen === 'smoke' && !can.ok ? 'glow' : chosen;
      const got = pick({ stored: st, asked, reduced, can });
      assert.deepEqual(got, { level, chosen, smokeOff: can.ok ? null : 'because' },
        JSON.stringify({ asked, st, reduced, can: can.ok }));
    }
  }
});
ok('pick: the defaults spelled out', () => {
  assert.deepEqual(pick({ stored: null, asked: null, reduced: false, can: yes }),
    { level: 'smoke', chosen: 'smoke', smokeOff: null });
  assert.deepEqual(pick({ stored: null, asked: null, reduced: true, can: yes }),
    { level: 'glow', chosen: 'glow', smokeOff: null });
  assert.deepEqual(pick({ stored: 'smoke', asked: null, reduced: true, can: yes }),
    { level: 'smoke', chosen: 'smoke', smokeOff: null });
  assert.deepEqual(pick({ stored: 'smoke', asked: null, reduced: false, can: no }),
    { level: 'glow', chosen: 'smoke', smokeOff: 'because' });
  assert.deepEqual(pick({ stored: 'off', asked: 'glow', reduced: false, can: no }),
    { level: 'glow', chosen: 'glow', smokeOff: 'because' });
});
ok('backgroundAlpha: the alpha of any colour serialisation, others opaque', () => {
  const cases = [
    ['rgba(0, 0, 0, 0)', 0], ['transparent', 0], ['rgb(10, 20, 30)', 1],
    ['rgba(10, 20, 30, 0.7)', 0.7], ['rgb(10 20 30 / 0.35)', 0.35],
    ['rgb(10 20 30 / 50%)', 0.5],
    // what a color-mix() background (the EQ graph, a warn or fault card) computes to
    ['color(srgb 0.1 0.2 0.3 / 0.7)', 0.7], ['color(srgb 0.1 0.2 0.3)', 1],
    ['color(srgb 0.1 0.2 0.3 / 0)', 0], ['oklab(0.5 0.1 0.1 / 0.4)', 0.4],
    ['lab(50 10 10 / 25%)', 0.25], ['color-mix(in srgb, red 70%, transparent)', 1],
    ['red', 1], ['', 1],
  ];
  for (const [css, want] of cases) assert.equal(backgroundAlpha(css), want, css);
});
ok('loudness: the frame\'s two RMS levels on a scale from the floor to the top, 0 to 1', () => {
  assert.equal(loudness([0, 0, 0, 0]), 0);
  assert.equal(loudness([MUSIC.FLOOR, MUSIC.FLOOR, 1, 1]), 0);
  assert.equal(loudness([MUSIC.FLOOR + MUSIC.SPAN, MUSIC.FLOOR + MUSIC.SPAN, 1, 1]), 1);
  assert.equal(loudness([1, 1, 1, 1]), 1);
  // the louder channel counts, not the mean: a hard-panned track is as loud as it is
  assert.equal(loudness([0.9, 0, 1, 0]), loudness([0.9, 0.9, 1, 1]));
  const quiet = loudness([0.5, 0.5, 0.6, 0.6]);
  const loud = loudness([0.8, 0.8, 0.9, 0.9]);
  assert.ok(quiet > 0 && quiet < loud && loud < 1, `${quiet} ${loud}`);
  // anything that is not a frame's levels is silence
  for (const bad of [null, undefined, [], [NaN, NaN, 0, 0], 'x']) assert.equal(loudness(bad), 0);
});
ok('glowLevel: the volume plus the music\'s share, never over 1; no music is the volume', () => {
  assert.equal(glowLevel(0.4, 0), 0.4);
  assert.equal(glowLevel(0.4, null), 0.4);
  assert.ok(Math.abs(glowLevel(0.4, 1) - (0.4 + MUSIC.GAIN)) < 1e-12);
  assert.equal(glowLevel(0.9, 1), 1);
  assert.equal(glowLevel(0.06, 0.5) > 0.06, true);
  assert.equal(glowLevel(0.4, -3), 0.4);
});

// A page of one layer, enough for the glow's level to be written and read back.
const page = () => {
  const props = new Map();
  const classes = new Set();
  const writes = [];
  const layer = {
    style: { setProperty: (k, v) => { props.set(k, v); writes.push([k, v]); } },
    classList: {
      toggle: (c, on) => (on ? classes.add(c) : classes.delete(c)),
      add: (c) => classes.add(c),
      contains: (c) => classes.has(c),
    },
    dataset: {},
    append() {},
  };
  globalThis.document = { querySelector: (q) => (q === '.fx' ? layer : null) };
  return { props, classes, writes, lvl: () => props.get('--lvl') };
};

ok('the_glow_follows_the_level: the volume sets it, the music adds while it plays and the '
  + 'old level is back when it stops', () => {
  const p = page();
  level(0.4);
  assert.equal(p.lvl(), '0.40');
  music(1);
  assert.equal(p.lvl(), (0.4 + MUSIC.GAIN).toFixed(2));
  assert.ok(p.classes.has('hearing'), 'the layer says it is following music');
  music(0.5);
  assert.equal(p.lvl(), (0.4 + MUSIC.GAIN / 2).toFixed(2));
  // a volume change while music plays keeps the music's share on top
  level(0.2);
  assert.equal(p.lvl(), (0.2 + MUSIC.GAIN / 2).toFixed(2));
  music(null);
  assert.equal(p.lvl(), '0.20');
  assert.equal(p.classes.has('hearing'), false);
  // levels that are not numbers change nothing; music of 0 is playing, not stopped
  music(Number.NaN);
  assert.equal(p.lvl(), '0.20');
  assert.equal(p.classes.has('hearing'), false);
  music(0);
  assert.equal(p.classes.has('hearing'), true);
  assert.equal(p.lvl(), '0.20');
  music(null);
});
ok('music: a loudness that moves less than the glow can show writes nothing, a change of the '
  + 'two decimals writes once, and the class is toggled only as the music starts and stops', () => {
  const p = page();
  level(0.4);
  music(null);
  music(0.5);
  const after = p.writes.length;
  for (const x of [0.5, 0.502, 0.498, 0.5004]) music(x);
  assert.equal(p.writes.length, after, 'sub-decimal moves wrote the property');
  music(0.7);
  assert.equal(p.writes.length, after + 1);
  assert.equal(p.lvl(), (0.4 + MUSIC.GAIN * 0.7).toFixed(2));
  music(null);
  assert.equal(p.lvl(), '0.40');
  assert.equal(p.classes.has('hearing'), false);
});
ok('a beat with no smoke to puff, or with no music, is nothing', () => {
  page();
  music(null);
  beat();
  music(0.5);
  beat();
  music(null);
});
console.log(`effects.mjs: ${n} checks passed`);
