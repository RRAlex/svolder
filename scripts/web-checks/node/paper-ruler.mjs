/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's ruler (themes/paper/ruler.js): where a step stands along
// the ruler, which step a place stands for, the labels at each multiple of ten dB read off the
// device's own table, the ticks and the two zones; and the volume section drawn from every
// fixture (draw/volume.js). Run: node paper-ruler.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { SECTIONS } from '../../../bins/svolderd/web/themes/paper/draw/sheet.js';
import { ASK_MS, shownRaw } from '../../../bins/svolderd/web/themes/paper/draw/volume.js';
import { computerView, modelOf, pageWith } from './paper-rig.mjs';
import {
  LABEL_DB, LABEL_GAP, NARROW_LABEL_DB, NARROW_PX, fractionOf, marks, percentVolume, rawAt, ticks, zones,
} from '../../../bins/svolderd/web/themes/paper/ruler.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const volumeOf = (name) => pageOf(name).device.volume;
const MINUS = '−';

ok('the_needle_fraction_follows_the_steps', () => {
  const v = { lo: 0, hi: 127 };
  assert.equal(fractionOf(v, 0), 0);
  assert.equal(fractionOf(v, 127), 1);
  assert.equal(fractionOf(v, 64), 64 / 127);
  assert.equal(fractionOf({ lo: 1, hi: 1 }, 1), 0, 'a range of nothing is at its start');
  assert.equal(fractionOf(v, 500), 1);
  assert.equal(fractionOf(v, -5), 0);
});

ok('raw_at_a_place_rounds_and_clamps_to_the_range', () => {
  const v = { lo: 0, hi: 127 };
  assert.equal(rawAt(v, -0.2), 0);
  assert.equal(rawAt(v, 1.4), 127);
  assert.equal(rawAt(v, 0.5), 64);
  assert.equal(rawAt({ lo: 1, hi: 11 }, 0), 1);
});

ok('a_dac_ruler_is_labelled_every_10_db_from_its_real_bottom', () => {
  const db = Array.from({ length: 161 }, (_, i) => -80 + i * 0.5);
  const v = { lo: 0, hi: 160, db, marks: [{ at: 0, text: `${MINUS}80` }], texts: [] };
  const got = marks(v, 1000);
  assert.deepEqual(got.map((m) => m.text), [`${MINUS}80`, `${MINUS}70`, `${MINUS}60`, `${MINUS}50`,
    `${MINUS}40`, `${MINUS}30`, `${MINUS}20`, `${MINUS}10`, '0']);
  got.forEach((m, i) => assert.ok(Math.abs(m.at - (i === 8 ? 1 : (i * 20) / 160)) < 1e-9, m.text));
  assert.equal(LABEL_DB, 10);
});

ok('a_label_keeps_its_gap_from_the_one_before_on_a_wide_ruler', () => {
  // The 90 dB step stands a sixtieth of the way along: nearer the bottom mark than the gap allows.
  const db = Array.from({ length: 301 }, (_, i) => (i <= 5 ? -100 + 2 * i : -90 + ((i - 5) * 90) / 295));
  const v = { lo: 0, hi: 300, db, marks: [{ at: 0, text: `${MINUS}100` }], texts: [] };
  const got = marks(v, 2000);
  for (let i = 1; i < got.length; i++) {
    assert.ok(got[i].at - got[i - 1].at >= LABEL_GAP, `${got[i - 1].text} and ${got[i].text}`);
  }
  assert.equal(LABEL_GAP, 0.03);
});

ok('the_gungnir_ruler_reads_its_true_places', () => {
  const v = volumeOf('dac');
  const got = marks(v, 1000);
  assert.deepEqual(got[0], { at: 0, text: `${MINUS}∞` });
  assert.ok(!got.some((m) => m.text === `${MINUS}90`), 'minus 90 stands within 3% of the bottom');
  for (const m of got.slice(1)) {
    const level = m.text === '0' ? 0 : -Number(m.text.slice(1));
    const i = v.db.findIndex((x) => x !== null && x >= level);
    const step = i - 1 + (level - v.db[i - 1]) / (v.db[i] - v.db[i - 1]);
    assert.ok(Math.abs(m.at - step / 127) < 1e-3, `${m.text} at ${m.at}`);
  }
  assert.equal(got.at(-1).text, '0');
  assert.ok(Math.abs(got.at(-1).at - 1) < 1e-9);
  for (let i = 1; i < got.length; i++) assert.ok(got[i].at - got[i - 1].at >= LABEL_GAP);
});

ok('a_narrow_ruler_is_labelled_every_20_db', () => {
  const v = volumeOf('dac');
  const wide = marks(v, NARROW_PX);
  const narrow = marks(v, NARROW_PX - 1);
  assert.equal(NARROW_LABEL_DB, 20);
  assert.ok(narrow.length < wide.length);
  assert.deepEqual(narrow.slice(1).map((m) => m.text), [`${MINUS}80`, `${MINUS}60`, `${MINUS}40`,
    `${MINUS}20`, '0'].filter((t) => narrow.some((m) => m.text === t)));
  assert.ok(narrow.slice(1).every((m) => Number(m.text.replace(MINUS, '-')) % 20 === 0));
});

ok('a_level_without_db_is_labelled_from_its_words', () => {
  const texts = Array.from({ length: 33 }, (_, i) => `step ${i}`);
  const v = { lo: 0, hi: 32, db: [], marks: [], texts };
  const got = marks(v, 1000);
  assert.deepEqual(got.map((m) => m.text), ['step 0', 'step 4', 'step 8', 'step 12', 'step 16',
    'step 20', 'step 24', 'step 28', 'step 32']);
  assert.equal(got[1].at, 4 / 32);
  assert.equal(ticks(v).length, 33, 'a short range has a tick a step');
  assert.deepEqual(ticks({ ...v, hi: 200, texts: [] }), [], 'a long one has none');
});

ok('the_ticks_stand_at_every_2_db', () => {
  const t = ticks(volumeOf('dac'));
  assert.ok(t.length > 30 && t.every((x, i) => x >= 0 && x <= 1 && (i === 0 || x > t[i - 1])));
});

ok('the_risk_and_the_cap_stand_at_their_steps', () => {
  const v = volumeOf('dac-climb-cap-risk');
  const z = zones(v);
  assert.equal(z.risk, 115 / 127);
  assert.equal(z.cap, 97 / 127);
  assert.deepEqual(zones(volumeOf('dac')), { risk: 118 / 127, cap: null });
});

ok('the_volume_is_drawn_for_every_fixture_without_style_and_with_its_parts', () => {
  const view = { phone: false, coarse: false, state: 'open', keys: [], now: 0, rulerPx: 1000 };
  for (const { name, page } of fixture.cases) {
    const html = SECTIONS.volume(page, page.device, view);
    assert.ok(!/\sstyle=/i.test(html), name);
    assert.ok(!/→/.test(html), name);
    if (!page.device?.volume) {
      assert.equal(html, '', `${name}: no volume, nothing drawn`);
      continue;
    }
    for (const id of ['p-ruler', 'p-needle', 'p-volume-text', 'p-big']) {
      assert.ok(html.includes(`id="${id}"`), `${name} ${id}`);
    }
    assert.ok(html.includes('role="slider"') && html.includes('aria-label="Volume"'), name);
  }
});

ok('the_volume_says_its_words_and_the_mute_word_follows_the_state', () => {
  const view = { phone: false, state: 'open', now: 0, rulerPx: 1000 };
  const dac = pageOf('dac');
  const html = SECTIONS.volume(dac, dac.device, view);
  assert.ok(html.includes('data-act="volume-down"') && html.includes(`${MINUS} quieter`));
  assert.ok(html.includes('data-act="volume-up"') && html.includes('louder +'));
  assert.ok(html.includes('data-act="mute" data-key="mute"'), html);
  assert.match(html, /<span id="p-volume-text"[^>]*>−26\.1 dB<\/span>/);
  const muted = pageOf('dac-muted');
  muted.device.volume.muted = true;
  const said = SECTIONS.volume(muted, muted.device, view);
  assert.ok(said.includes('>unmute<') && said.includes('>muted<'), said);
  const saga = pageOf('saga');
  assert.ok(!SECTIONS.volume(saga, saga.device, view).includes('data-act="mute"'),
    'a device with no mute key has no mute word');
  const unread = pageOf('dac-unread');
  assert.ok(SECTIONS.volume(unread, unread.device, view).includes('not read yet'));
});

ok('the_ruler_reads_0_to_100_percent', () => {
  const read = { pct: 42, muted: false, text: '42', unit: '%', inert: false, boosted: false };
  const v = percentVolume(read);
  assert.deepEqual(marks(v, 1000).map((m) => [m.at, m.text]),
    [[0, '0'], [0.25, '25'], [0.5, '50'], [0.75, '75'], [1, '100']]);
  assert.deepEqual(marks(v, 300).map((m) => m.text), ['0', '25', '50', '75', '100'], 'the same on a phone');
  const at = ticks(v);
  assert.equal(at.length, 51);
  assert.ok(at.every((t, i) => Math.abs(t - i / 50) < 1e-9));
  assert.deepEqual(zones(v), { risk: null, cap: null });
  assert.equal(fractionOf(v, 42), 0.42);
  assert.equal(rawAt(v, 0.424), 42);
  assert.equal(rawAt(v, 1.2), 100);
  // drawn: the number, the percent, no zone, the needle at 42 %
  const { view } = computerView(pageWith('dac', modelOf({ pct: 42 })), 'computer');
  const page = pageWith('dac', modelOf({ pct: 42 }));
  const html = SECTIONS.volume(page, page.device, view);
  assert.match(html, /<span class="p-num">42<\/span><small>%<\/small>/);
  assert.match(html, /id="p-volume-text"[^>]*>42 %<\/span>/);
  assert.ok(!html.includes('p-zone'), 'no red zone, no cap');
  assert.match(html, /id="p-needle"[^>]*aria-valuemin="0" aria-valuemax="100" aria-valuenow="42"[^>]*data-at="0\.42000"/);
  assert.deepEqual([...html.matchAll(/<span class="p-mark" data-at="[\d.]+">([^<]*)<\/span>/g)].map((m) => m[1]),
    ['0', '25', '50', '75', '100']);
  // a sink another program put above 100 % is read as it is, at the ruler's end
  const loud = pageWith('dac', modelOf({ pct: 130 }));
  const boosted = SECTIONS.volume(loud, loud.device, computerView(loud, 'computer').view);
  assert.match(boosted, /<span class="p-num">130<\/span>/);
  assert.match(boosted, /aria-valuenow="100"[^>]*data-at="1\.00000"/);
  // the written words are the computer's own, and mute follows the state
  assert.ok(boosted.includes('data-act="c-down"') && boosted.includes('data-act="c-up"')
    && boosted.includes('data-act="c-mute"'));
  assert.ok(!boosted.includes('data-act="volume-up"'));
});

ok('a_guests_words_are_quiet_and_the_cap_and_risk_are_drawn', () => {
  const view = { phone: false, state: 'open', now: 0, rulerPx: 1000 };
  const guest = pageOf('dac-guest');
  const html = SECTIONS.volume(guest, guest.device, view);
  assert.ok(!/class="p-w"/.test(html) && html.includes('p-w quiet'), html);
  const hot = pageOf('dac-climb-cap-risk');
  const zoned = SECTIONS.volume(hot, hot.device, view);
  assert.ok(zoned.includes('p-zone risk') && zoned.includes('p-zone cap'), zoned);
  assert.ok(zoned.includes('may clip') && zoned.includes('>cap<'), zoned);
});

ok('shown_raw_is_the_ask_until_the_device_answers_or_the_ask_is_old', () => {
  const v = (raw, target = null) => ({ raw, target });
  const asked = (raw, at, answered = false) => ({ raw, at, answered });
  assert.equal(shownRaw(v(50), { asked: null, now: 0 }), 50, 'no ask: the reading');
  assert.equal(shownRaw(v(50), { asked: asked(50, 0), now: 10 }), 50, 'reading equals the ask');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0), now: ASK_MS }), 50, 'an ask older than ASK_MS');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0, true), now: 10 }), 50,
    'answered and not gliding shows the reading');
  assert.equal(shownRaw(v(50, 70), { asked: asked(60, 0, true), now: 10 }), 60,
    'gliding keeps the ask');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0), now: 10 }), 60, 'unanswered keeps the ask');
  assert.equal(shownRaw(v(null), { asked: asked(60, 0), now: 10 }), null, 'unread stays unread');
});

ok('the_drawn_ruler_is_replaced_when_the_label_gap_in_pixels_changes_within_a_band', () => {
  const page = pageOf('dac-facts-heard');
  const sig = (rulerPx) => /data-sig="([^"]*)"/.exec(SECTIONS.volume(page, page.device,
    { phone: false, coarse: false, state: 'open', keys: [], now: 0, rulerPx }))[1];
  assert.notEqual(sig(700), sig(900), 'under 1000 px the pixel floor sets the gap');
  assert.equal(sig(1100), sig(1500), 'wide enough, the gap is the fraction and the sheet is as drawn');
  assert.equal(sig(900), sig(900));
});

console.log(`paper-ruler: ${n} passed`);
