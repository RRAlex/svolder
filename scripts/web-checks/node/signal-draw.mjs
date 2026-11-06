/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for what the theme Signal draws of a DAC's first screen (themes/signal/draw/): the
// volume with its unit, the tone curve with a point on each band, the choices as chips and the Now
// playing bar; what every drawn control owes a guest or a read-only viewer; no `style` attribute
// anywhere. The markup is read with a small tag reader, no DOM. Run: node signal-draw.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { targetsOf } from '../../../bins/svolderd/web/core/contract.js';
import { yOf } from '../../../bins/svolderd/web/core/tone-curve.js';
import { ampScreen, metersInPlace } from '../../../bins/svolderd/web/themes/signal/draw/amp.js';
import { declaredScreen, editForm, hopTools, moves, plusSheet, unlinkedAmps } from '../../../bins/svolderd/web/themes/signal/draw/hop.js';
import { choices, modeRow } from '../../../bins/svolderd/web/themes/signal/draw/choices.js';
import { HOP_PX, line, slide, strip } from '../../../bins/svolderd/web/themes/signal/draw/line.js';
import { drawShelf } from '../../../bins/svolderd/web/themes/signal/draw/shelf.js';
import { createFold } from '../../../bins/svolderd/web/themes/signal/fold.js';
import { keeper } from '../../../bins/svolderd/web/themes/signal/live.js';
import { drawSlim, volumeWords } from '../../../bins/svolderd/web/themes/signal/slim.js';
import { card, stateCard } from '../../../bins/svolderd/web/themes/signal/draw/card.js';
import { computerScreen } from '../../../bins/svolderd/web/themes/signal/draw/computer.js';
import { playerScreen } from '../../../bins/svolderd/web/themes/signal/draw/player.js';
import { drawPlaying, transport } from '../../../bins/svolderd/web/themes/signal/draw/playing.js';
import { curvePath, heldGraph, pointsOf, toneBox } from '../../../bins/svolderd/web/themes/signal/draw/tone.js';
import { volumeBox } from '../../../bins/svolderd/web/themes/signal/draw/volume.js';
import { hopsOf, shelves, shownHopId } from '../../../bins/svolderd/web/themes/signal/line.js';
import { bound } from '../../../bins/svolderd/web/themes/signal/driver.js';
import { openers, theme } from '../../../bins/svolderd/web/themes/signal/signal.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const view = { asked: null, now: 0, facts: { open: false }, phone: () => false };

/** The start tags of `html` named `tag`, each as its attributes (and `text`: what follows up to the
 * next tag). */
function tags(html, tag) {
  const out = [];
  const open = new RegExp(`<${tag}(\\s[^>]*)?>([^<]*)`, 'g');
  for (const m of html.matchAll(open)) {
    const attrs = { text: m[2] };
    for (const a of (m[1] ?? '').matchAll(/([\w-]+)="([^"]*)"/g)) attrs[a[1]] = a[2];
    out.push(attrs);
  }
  return out;
}
const count = (html, name, value) => (html.match(new RegExp(` ${name}="${value}"`, 'g')) ?? []).length;
const classes = (el) => (el.class ?? '').split(' ');
const everything = (page) => {
  const d = page.device;
  const hop = { id: `device:${page.shown}`, kind: 'device', url: page.shown };
  return [volumeBox(d, view), toneBox(d), choices(d), modeRow(d), drawPlaying(page, 1_700_000_100_000),
    openers.device(page, hop, view), drawShelf(page, d, view), drawShelf(page, d, { facts: { open: true } }),
    drawSlim([{ id: hop.id, name: d.name, live: true }, { id: 'hop:1', name: 'HD 650', live: false }],
      hop.id, hop.id, volumeWords(d))].join('');
};

ok('the_volume_reads_the_devices_db_with_its_unit', () => {
  const page = pageOf('dac');
  const html = volumeBox(page.device, view);
  assert.match(html, /<span class="sg-num">−26\.1<\/span><small>dB<\/small>/);
  const track = tags(html, 'div').find((d) => d.id === 'sg-vtrack');
  assert.equal(track.role, 'slider');
  assert.equal(track['aria-valuenow'], '63');
  assert.equal(track['aria-valuemin'], '0');
  assert.equal(track['aria-valuemax'], '127');
  assert.equal(track['aria-valuetext'], '−26.1 dB');
  const acts = tags(html, 'button').map((b) => b['data-act']);
  assert.deepEqual(acts, ['volume-down', 'mute', 'volume-up'], 'Ember\'s order');
  assert.ok(!html.includes('sg-vcap'), 'no cap, no mark');
  const muted = volumeBox(pageOf('dac-muted').device, view);
  assert.match(muted, /<span class="sg-num">\u2212\u221e<\/span><small>dB<\/small>/, 'muted reads minus infinity');
  assert.ok(/data-act="mute"[^>]*aria-pressed="true">Unmute</.test(muted), muted);
  assert.equal(tags(muted, 'div').find((d) => d.id === 'sg-vtrack')['aria-valuetext'], 'muted');
  const noMute = pageOf('dac');
  delete noMute.device.volume.mute;
  assert.deepEqual(tags(volumeBox(noMute.device, view), 'button').map((b) => b['data-act']),
    ['volume-down', 'volume-up'], 'no mute key, no Mute');
});

ok('the_volume_shows_the_step_asked_for_until_it_is_read_back', () => {
  const { device } = pageOf('dac');
  const asked = { raw: 64, at: 0, answered: false };
  const html = volumeBox(device, { asked, now: 100 });
  assert.match(html, new RegExp(`<small>dB</small>`));
  assert.ok(html.includes('aria-valuenow="64"'), html);
  assert.ok(volumeBox(device, { asked, now: 5000 }).includes('aria-valuenow="63"'), 'it gives way');
});

ok('the_cap_is_marked_on_the_track', () => {
  const { device } = pageOf('dac');
  device.volume.cap = 97;
  const html = volumeBox(device, view);
  const mark = tags(html, 'span').find((s) => classes(s).includes('sg-vcap'));
  assert.ok(Math.abs(Number(mark['data-at']) - 97 / 127) < 1e-4, mark['data-at']);
});

ok('a_device_without_a_volume_draws_none', () => {
  const { device } = pageOf('dac');
  delete device.volume;
  assert.equal(volumeBox(device, view), '');
});

ok('the_curve_has_one_point_per_band', () => {
  const page = pageOf('dac');
  const html = toneBox(page.device);
  const points = tags(html, 'button').filter((b) => classes(b).includes('sg-pt'));
  const tone = page.device.tone;
  assert.equal(points.length, tone.markers.length);
  const want = pointsOf(tone);
  points.forEach((p, i) => {
    assert.equal(Number(p['data-band']), want[i].band);
    assert.ok(Math.abs(Number(p['data-x']) - want[i].x) < 1e-5, `x of ${i}`);
    assert.ok(Math.abs(Number(p['data-y']) - want[i].y) < 1e-5, `y of ${i}`);
    assert.equal(p.role, 'slider');
    assert.match(p['aria-label'], /^Band \d: /);
  });
  assert.ok(points[0]['aria-label'].includes('40.0 Hz') && points[0]['aria-label'].includes('3.10 dB'));
  assert.ok(Math.abs(want[1].y - yOf(tone.markers[1].db) / 260) < 1e-9);
  const curve = tags(html, 'path').find((p) => classes(p).includes('sg-curve'));
  assert.equal(curve.d, curvePath(tone.response));
  assert.equal(curve.d.split('L').length, 181);
});

ok('a_band_not_read_in_full_draws_no_point_and_flat_dashes_the_curve', () => {
  const html = toneBox(pageOf('dac-band-unread').device);
  const points = tags(html, 'button').filter((b) => classes(b).includes('sg-pt'));
  assert.ok(!points.some((p) => p['data-band'] === '3'), 'band 3 has no Q reading');
  assert.ok(/<path class="sg-curve faint"/.test(toneBox(pageOf('dac-flat').device)));
  assert.ok(/<path class="sg-curve"/.test(toneBox(pageOf('dac').device)));
  assert.ok(!tags(toneBox(pageOf('dac-loudness').device), 'button').some((b) => classes(b).includes('sg-pt')),
    'Loudness has no points of its own');
});

ok('the_held_graph_moves_only_the_held_point', () => {
  const page = pageOf('dac');
  const tone = page.device.tone;
  const read = tone.readings.bands[1];
  const held = heldGraph(tone, page.device.tone_steps, 2, { f: read[0], g: read[1] + 5, q: read[2] });
  const was = pointsOf(tone)[1];
  assert.ok(held.point.y < was.y, 'more gain, higher up');
  assert.equal(heldGraph(pageOf('dac-band-unread').device.tone, page.device.tone_steps, 2, { f: 1, g: 1, q: 1 }), null);
});

ok('the_choices_are_chips_with_the_one_on_marked', () => {
  const html = choices(pageOf('dac').device);
  const chips = tags(html, 'button');
  const input = chips.filter((c) => c['data-act'] === 'choice');
  assert.deepEqual(input.map((c) => c['data-value']), ['USB 1', 'USB 2', 'Optical', 'Coaxial', 'AES']);
  assert.ok(input[0]['aria-pressed'] === 'true' && classes(input[0]).includes('on'));
  assert.ok(input.slice(1).every((c) => c['aria-pressed'] === 'false'));
  assert.ok(!html.includes('data-act="mode"'), 'the mode is not among the shelf\'s choices');
  const mode = tags(modeRow(pageOf('dac').device), 'button').filter((c) => c['data-act'] === 'mode');
  assert.deepEqual(mode.map((c) => c['data-mode']), ['flat', 'eq', 'loudness']);
  assert.equal(mode.filter((c) => classes(c).includes('on')).length, 1);
  assert.ok(!html.includes('preset-list') && !html.includes('sg-preset"'), 'no Preset chip: the shelf has the presets');
  assert.equal(modeRow(pageOf('amp').device), '', 'no tone, no mode row');
});

ok('the_loudness_chip_carries_round_steppers_while_loudness_is_on', () => {
  const on = pageOf('dac-loudness').device;
  const html = modeRow(on);
  assert.equal(count(html, 'id', 'sg-loud'), 1);
  assert.match(html, /<span id="sg-loud" class="sg-loudw">/);
  const steps = tags(html, 'button').filter((b) => b['data-act'] === 'loudness');
  assert.deepEqual(steps.map((b) => [b.id, b['data-step']]), [['sg-loudm', '-1'], ['sg-loudp', '1']]);
  assert.ok(steps.every((b) => classes(b).includes('sg-rb') && classes(b).includes('sg-st')), 'round buttons');
  assert.ok(html.indexOf('sg-loudm') < html.indexOf('data-mode="loudness"')
    && html.indexOf('data-mode="loudness"') < html.indexOf('sg-loudp'), '- over the chip\'s start, + its end');
  assert.match(html, /<small class="sg-lv">[^<]+<\/small>/, 'the strength under the name');
  const off = pageOf('dac').device;
  assert.ok(!modeRow(off).includes('sg-loudm'), 'no steppers while Loudness is off');
  const shelf = drawShelf(pageOf('dac-loudness'), on, view, { choices: true });
  assert.ok(!shelf.includes('class="sg-box2"><div class="sg-ctl"><span class="sg-lbl">Loudness'), 'no separate loudness box in a DAC\'s shelf');
  assert.equal(count(shelf, 'id', 'sg-loud'), 1, 'one Loudness wrap');
  const plain = drawShelf(pageOf('dac-loudness'), on, view);
  const mid = plain.slice(plain.indexOf('id="sg-mid"'));
  assert.ok(plain.indexOf('id="sg-loud"') > plain.indexOf('id="sg-mid"'), 'the box is in the middle column');
  assert.ok(mid.indexOf('id="sg-loud"') > mid.indexOf('id="sg-controls"')
    && mid.indexOf('id="sg-loud"') < mid.indexOf('id="sg-facts"'), 'after the controls, as before');
  assert.ok(!plain.slice(0, plain.indexOf('id="sg-mid"')).includes('id="sg-loud"'), 'not under the band table');
  const noInput = pageOf('dac-loudness');
  noInput.device.controls = noInput.device.controls.filter((c) => c.key !== 'input');
  assert.ok(!drawShelf(noInput, noInput.device, view, { choices: true }).includes('id="sg-choices"'),
    'no input, no empty choices section');
});

ok('the_mode_sits_where_the_screen_puts_it', () => {
  const page = pageOf('dac-loudness');
  const hop = { id: `device:${page.shown}`, kind: 'device', url: page.shown };
  const cols = (html) => [...html.matchAll(/<section id="(sg-[a-z]+)"/g)].map((m) => m[1]);
  // a desktop: the mode heads the shelf's middle column, the input is under the band table
  const desk = shelves.device(page, hop, view);
  assert.deepEqual(cols(desk).slice(0, 4), ['sg-bands', 'sg-choices', 'sg-mid', 'sg-facts']);
  assert.ok(desk.indexOf('id="sg-mode"') > desk.indexOf('id="sg-mid"')
    && desk.indexOf('id="sg-mode"') < desk.indexOf('id="sg-presets"'), 'the mode first in the middle column');
  assert.ok(!openers.device(page, hop, view).includes('id="sg-mode"'), 'not in the first screen');
  // a phone: the mode ends the first screen; the shelf opens with the input, then the presets
  const phone = { ...view, phone: () => true };
  assert.ok(openers.device(page, hop, phone).includes('id="sg-mode"'), 'under the curve');
  const shelf = shelves.device(page, hop, phone);
  assert.deepEqual(cols(shelf).slice(0, 4), ['sg-up', 'sg-choices', 'sg-bands', 'sg-mid']);
  assert.ok(!shelf.includes('id="sg-mode"'), 'drawn once');
  assert.ok(shelf.indexOf('id="sg-choices"') < shelf.indexOf('id="sg-presets"') && shelf.indexOf('id="sg-presets"') < shelf.indexOf('id="sg-bands"'),
    'input, presets, then the band table');
});

ok('a_guests_controls_are_drawn_inert', () => {
  const guests = fixture.cases.filter((c) => c.page.device && (c.page.viewer.guest || c.page.viewer.writes === false));
  assert.ok(guests.length >= 4, `guest and read-only fixtures: ${guests.length}`);
  for (const c of guests) {
    const page = pageOf(c.name);
    const html = everything(page);
    // the fold of the facts and the slim bar's hops only read and move about the page: a guest has them
    const buttons = tags(html, 'button').filter((b) => b['data-act'] !== 'facts-fold' && !('data-slim' in b));
    for (const b of buttons) {
      assert.equal(b['aria-disabled'], 'true', `${c.name}: ${b['data-act'] ?? b.id} is inert`);
      assert.ok(classes(b).includes('quiet'), `${c.name}: ${b['data-act'] ?? b.id} is quiet`);
    }
    for (const s of [...tags(html, 'div'), ...tags(html, 'button')].filter((e) => e.role === 'slider')) {
      assert.equal(s['aria-disabled'], 'true', `${c.name}: a slider is inert`);
    }
  }
  // and the ones of a viewer who may write are not
  const html = everything(pageOf('playing'));
  assert.ok(tags(html, 'button').length > 8);
  assert.ok(!html.includes('aria-disabled'), 'a writer\'s controls are live');
});

ok('no_markup_carries_a_style_attribute', () => {
  for (const c of fixture.cases.filter((x) => x.page.device)) {
    const html = everything(pageOf(c.name));
    assert.ok(!/\sstyle=/.test(html), `${c.name} has a style attribute`);
  }
  // a linked amp's area and shelf, with its Unlink and its name made of markup
  const { page, hop } = linkedAmp();
  page.followed[hop.url].name = '<i onclick=1>Aegir</i>';
  const html = openers.device(page, hop, view) + shelves.device(page, hop, view);
  assert.ok(!/\sstyle=/.test(html), 'a linked amp has a style attribute');
  assert.ok(!html.includes('<i onclick') && html.includes('&lt;i onclick=1&gt;'), 'the amp\'s name is escaped');
});

ok('words_from_the_model_are_escaped', () => {
  const page = pageOf('playing');
  page.device.controls.find((c) => c.key === 'input').options[1].label = '<img src=x onerror=1>';
  page.device.playing.track.headline = '<b>x</b>';
  const html = everything(page);
  assert.ok(!html.includes('<img src=x') && !html.includes('<b>x</b>'), html);
});

ok('the_bar_names_the_track_and_the_transport', () => {
  const page = pageOf('playing');
  const html = drawPlaying(page, 1_700_000_100_000);
  const buttons = tags(html, 'button').map((b) => [b['data-button'], b['aria-label']]);
  assert.deepEqual(buttons, [['previous', 'Previous'], ['pause', 'Pause'], ['next', 'Next']]);
  assert.match(html, /<b>Test Pressing<\/b> · The Fakes/);
  // 30 s at the read + 3 s since (now_ms 100000 vs read_at 97000) of a 20 min track
  assert.match(html, /<span class="sg-time">0:33 \/ 20:00<\/span>/);
  assert.match(html, /class="sg-fill" data-at="0\.02750"/);
  page.device.playing.track.status = 'paused';
  const paused = drawPlaying(page, 1_700_000_100_000);
  assert.deepEqual(tags(paused, 'button').map((b) => b['data-button']), ['previous', 'play', 'next']);
  assert.match(paused, /0:30 \/ 20:00/, 'a paused track stands still');
});

ok('the_transport_is_drawn_as_icons_no_font_can_swap', () => {
  for (const status of ['playing', 'paused']) {
    const html = transport(status, false);
    assert.ok(!/[\u23ee\u23f8\u23ed\u25b6]/u.test(html), 'no glyph text in the buttons');
    const buttons = html.match(/<button[^>]*>.*?<\/button>/g);
    assert.equal(buttons.length, 3);
    for (const b of buttons) {
      assert.match(b, /<svg [^>]*fill="currentColor"[^>]*aria-hidden="true"[^>]*>/);
      assert.ok(!b.includes('style='), 'no style attribute');
      assert.match(b, /aria-label="[A-Z][a-z]+"/, 'the button keeps its label');
      assert.equal(b.replace(/<svg.*<\/svg>/, '').replace(/<[^>]*>/g, ''), '', 'nothing but the svg inside');
    }
  }
  assert.deepEqual(tags(transport('playing', true), 'button').map((b) => b['aria-disabled']),
    ['true', 'true', 'true']);
});

ok('the_bar_says_nothing_playing_and_draws_nothing_without_a_player', () => {
  assert.match(drawPlaying(pageOf('no-player')), /Nothing playing/);
  const bare = pageOf('dac');
  bare.host = null;
  bare.device.playing = null;
  assert.equal(drawPlaying(bare), '');
});

ok('the_device_opener_draws_a_dacs_first_screen_for_the_shown_device_only', () => {
  const page = pageOf('dac');
  const hop = { id: `device:${page.shown}`, kind: 'device', url: page.shown };
  const html = openers.device(page, hop, view);
  for (const id of ['sg-volume', 'sg-tone']) assert.ok(html.includes(`id="${id}"`), id);
  assert.equal(openers.device(page, { ...hop, url: 'Elsewhere' }, view), '');
  const amp = pageOf('amp');
  const ampHtml = openers.device(amp, { id: 'x', kind: 'device', url: amp.shown }, view);
  assert.ok(!ampHtml.includes('id="sg-volume"') && ampHtml.includes('id="sg-amp-power"'), 'an amp opens its own area');
});


/** How many elements of `html` have the attribute `name` equal to `value`. */
const escaped = (text) => String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;');

ok('the_shelf_lists_every_control_the_model_has', () => {
  for (const c of fixture.cases.filter((x) => x.page.device)) {
    const page = pageOf(c.name);
    const device = page.device;
    const html = drawShelf(page, device, view);
    for (const control of device.controls ?? []) {
      const key = control.key;
      const above = key === 'input' && control.kind === 'choice';
      const found = {
        toggle: count(html, 'id', `sg-t-${key}`),
        slider: count(html, 'id', `sg-s-${key}`),
        readout: count(html, 'data-readout', key),
        choice: control.style === 'list' ? count(html, 'id', `sg-l-${key}`)
          : (control.options ?? []).reduce((sum, _o, i) => sum + count(html, 'id', `sg-c-${key}-${i}`), 0),
      }[control.kind];
      const want = above ? 0 : control.kind === 'choice' && control.style !== 'list' ? (control.options ?? []).length : 1;
      assert.equal(found, want, `${c.name}: ${key} (${control.kind}) drawn ${found} times, wanted ${want}`);
    }
    // the power: the switch, its switches and its readouts, whatever the family
    const pw = device.power;
    if (pw) {
      assert.equal(count(html, 'id', 'sg-power'), 1, `${c.name}: power`);
      for (const t of pw.toggles ?? []) assert.equal(count(html, 'id', `sg-pt-${t.key}`), 1, `${c.name}: ${t.key}`);
      for (const r of pw.readouts ?? []) assert.equal(count(html, 'data-readout', r.key), 1, `${c.name}: ${r.key}`);
    } else assert.equal(count(html, 'id', 'sg-power'), 0);
    // every control target the contract lists is drawn once on the first screen and the shelf together
    const whole = html + choices(device) + volumeBox(device, view) + toneBox(device) + modeRow(device)
      + (device.family === 'amp' ? ampScreen(device, { dac: null, linkId: null }) : '');
    for (const t of targetsOf(device)) {
      if (t.control !== undefined) {
        const key = t.control;
        const has = [`id="sg-t-${key}"`, `id="sg-s-${key}"`, `data-readout="${key}"`, `id="sg-power"`, `id="sg-pt-${key}"`,
          `data-key="${key}"`, `id="sg-mute"`].some((m) => whole.includes(m));
        assert.ok(has || key === device.volume?.key, `${c.name}: control ${key} is drawn`);
      } else if (t.band !== undefined) {
        assert.ok(html.includes(`id="sg-b${t.band}${t.param}"`), `${c.name}: band ${t.band}${t.param}`);
      } else if (t.mode !== undefined) {
        assert.ok(modeRow(device).includes(`data-act="mode" id="sg-mode-${t.mode}" data-mode="${t.mode}"`), `${c.name}: mode ${t.mode}`);
      } else if (t.loudness !== undefined) {
        assert.ok(whole.includes('data-act="loudness"') && whole.includes('id="sg-loudm"') && whole.includes('id="sg-loudp"'),
          `${c.name}: the loudness steppers`);
      } else if (t.preset !== undefined) {
        assert.ok(tags(html, 'button').some((b) => b['data-act'] === 'preset' && b.id?.startsWith('sg-pr-')
          && b['data-name'] === escaped(t.preset)), `${c.name}: preset ${t.preset}`);
      } else if (t.slot !== undefined) {
        assert.ok(html.includes(`data-act="slot" id="sg-sl-${t.slot}" data-slot="${t.slot}"`), `${c.name}: slot ${t.slot}`);
      } else if (t.setting !== undefined) {
        assert.ok(tags(html, 'button').some((b) => b['data-act'] === 'setting' && b.id?.startsWith('sg-st-')
          && b['data-name'] === escaped(t.setting)), `${c.name}: setting ${t.setting}`);
      } else if (t.transport !== undefined) {
        assert.ok(drawPlaying(page, 1_700_000_100_000).includes(`data-button="${t.transport}"`),
          `${c.name}: transport ${t.transport}`);
      } else {
        assert.fail(`${c.name}: a target the test does not know: ${JSON.stringify(t)}`);
      }
    }
    // the tone: three values a band, each with its two steppers; the loudness and every preset
    for (const row of device.tone?.bands?.rows ?? []) {
      for (const p of ['f', 'g', 'q']) {
        assert.equal(count(html, 'id', `sg-b${row.band}${p}`), 1, `${c.name}: band ${row.band} ${p}`);
        assert.equal(count(html, 'id', `sg-b${row.band}${p}m`) + count(html, 'id', `sg-b${row.band}${p}p`), 2);
      }
    }
    assert.equal(count(html, 'id', 'sg-loudm'), device.tone?.loud ? 1 : 0, `${c.name}: loudness`);
    const loudOn = (device.tone?.modes ?? []).some((m) => m.name === 'loudness' && m.on === true) && device.tone?.loud;
    assert.equal(count(modeRow(device), 'id', 'sg-loudm'), loudOn ? 1 : 0, `${c.name}: loudness on the mode`);
    for (const preset of device.presets?.rows ?? []) {
      assert.equal(count(html, 'data-name', preset.name.replace(/&/g, '&amp;').replace(/</g, '&lt;')
        .replace(/>/g, '&gt;').replace(/"/g, '&quot;')), 1, `${c.name}: preset ${preset.name}`);
    }
    for (const slot of device.device_presets?.slots ?? []) {
      assert.equal(count(html, 'data-slot', slot.slot), 1, `${c.name}: slot ${slot.slot}`);
    }
    for (const built of device.device_presets?.builtins ?? []) {
      assert.equal(count(html, 'data-name', built.name), 1, `${c.name}: setting ${built.name}`);
    }
  }
});

ok('the_band_table_has_typed_values_and_steppers', () => {
  const page = pageOf('dac');
  const html = drawShelf(page, page.device, view);
  const inputs = tags(html, 'input').filter((i) => i['data-act'] === 'band-type');
  assert.equal(inputs.length, 15);
  const first = inputs[0];
  assert.deepEqual([first['data-band'], first['data-param'], first.value], ['1', 'f', '40.0 Hz']);
  assert.ok(!html.includes('readonly'), 'a writer types');
  const steps = tags(html, 'button').filter((b) => b['data-act'] === 'band');
  assert.equal(steps.length, 30);
  assert.deepEqual([steps[0]['data-step'], steps[1]['data-step']], ['-1', '1']);
  const guest = pageOf('dac-guest');
  const inert = drawShelf(guest, guest.device, view);
  assert.ok(tags(inert, 'input').filter((i) => i['data-act'] === 'band-type').every((i) => i['aria-disabled'] === 'true'));
  assert.equal((inert.match(/ readonly /g) ?? []).length, 15, 'a guest cannot type');
  assert.ok(tags(inert, 'button').filter((b) => b['data-act'] === 'band').every((b) => b['aria-disabled'] === 'true'));
});

ok('the_facts_start_folded', () => {
  const page = pageOf('dac');
  const folded = drawShelf(page, page.device, view);
  const toggle = tags(folded, 'button').find((b) => b.id === 'sg-facts-fold');
  assert.equal(toggle['aria-expanded'], 'false');
  assert.equal(toggle['aria-controls'], 'sg-facts-body');
  assert.match(folded, /<div id="sg-facts-body" hidden>/, 'the body is hidden');
  assert.ok(folded.includes('data-fact="Firmware"'), 'the facts are drawn, folded');
  assert.ok(tags(folded, 'button').some((b) => b.id === 'sg-identify'), 'Identify is under it');
  const open = drawShelf(page, page.device, { ...view, facts: { open: true } });
  assert.equal(tags(open, 'button').find((b) => b.id === 'sg-facts-fold')['aria-expanded'], 'true');
  assert.match(open, /<div id="sg-facts-body">/);
});

ok('the_fold_is_kept_as_open_or_closed_and_works_without_storage', () => {
  const kept = new Map();
  const prefs = { get: (name, fallback) => kept.get(name) ?? fallback, set: (name, value) => kept.set(name, value) };
  const fold = createFold(() => prefs);
  assert.equal(fold.open, false);
  assert.equal(fold.toggle(), true);
  assert.equal(kept.get('facts'), 'open');
  assert.equal(createFold(() => prefs).open, true, 'a reload reads it');
  fold.toggle();
  assert.equal(kept.get('facts'), 'closed');
  const refusing = { get() { throw new Error('no storage'); }, set() { throw new Error('no storage'); } };
  const bare = createFold(() => refusing);
  assert.equal(bare.open, false);
  assert.equal(bare.toggle(), true);
  assert.equal(bare.open, true, 'kept in memory');
  assert.equal(createFold(() => null).open, false);
});

ok('a_loki_max_shows_its_slots_and_settings', () => {
  const page = pageOf('lokimax');
  const html = drawShelf(page, page.device, view);
  const slots = tags(html, 'button').filter((b) => b['data-act'] === 'slot');
  assert.deepEqual(slots.map((b) => b['data-slot']), ['1', '2', '3']);
  const settings = tags(html, 'button').filter((b) => b['data-act'] === 'setting').map((b) => b['data-name']);
  assert.deepEqual(settings, ['Flat', 'Bass Boost', 'Treble Boost', 'Smiley', 'Loudness']);
  assert.ok(tags(html, 'button').some((b) => b['data-act'] === 'slot-save-list'));
  assert.ok(html.indexOf('sg-dpresets') > html.indexOf('id="sg-mid"') && html.indexOf('sg-dpresets') < html.indexOf('id="sg-facts"'),
    'in the middle column');
  assert.ok(!html.includes('id="sg-bands-table"'), 'no tone control, no band table');
  const guest = pageOf('lokimax-guest');
  const inert = drawShelf(guest, guest.device, view);
  assert.ok(!inert.includes('slot-save-list'), 'a guest cannot save');
  assert.ok(tags(inert, 'button').filter((b) => b['data-act'] === 'slot').every((b) => b['aria-disabled'] === 'true'));
});

ok('the_presets_have_save_as_rename_and_delete_for_a_writer_only', () => {
  const page = pageOf('dac-user-preset');
  const acts = (p) => tags(drawShelf(p, p.device, view), 'button').map((b) => b['data-act']);
  const writer = acts(page);
  for (const act of ['preset-save', 'preset-rename', 'preset-delete']) assert.ok(writer.includes(act), act);
  const plain = acts(pageOf('dac'));
  assert.ok(plain.includes('preset-save') && !plain.includes('preset-rename'), 'nothing of the person\'s own to rename');
  const guest = acts(pageOf('dac-guest'));
  assert.ok(!guest.some((a) => a.startsWith('preset-')), 'a guest cannot write presets');
});

ok('a_shelf_without_a_device_is_nothing', () => {
  assert.equal(drawShelf(pageOf('dac'), null, view), '');
});

ok('the_slim_bar_names_the_hops_and_rings_the_chosen_one', () => {
  const hops = [{ id: 'player', name: 'Strawberry', live: true }, { id: 'device:B', name: 'Bifrost 3', live: true },
    { id: 'hop:1', name: 'HD <650>', live: false }];
  const html = drawSlim(hops, 'device:B', 'device:B', '−23.5 dB');
  const found = tags(html, 'button');
  assert.deepEqual(found.map((b) => b['data-slim']), ['player', 'device:B', 'hop:1']);
  assert.ok(html.includes('Bifrost 3 · −23.5 dB</button>'));
  assert.deepEqual(found.map((b) => b['aria-current']), ['false', 'true', 'false']);
  assert.ok(classes(found[1]).includes('sg-cur') && !classes(found[0]).includes('sg-cur'));
  assert.ok(classes(found[2]).includes('sg-decl'));
  assert.ok(html.includes('HD &lt;650&gt;') && !html.includes('<650>'));
  assert.ok(!drawSlim(hops, 'player', 'device:B', '').includes('·'), 'no volume, no dot');
  assert.equal(volumeWords({ volume: { text: '−26.1 dB', muted: false } }), '−26.1 dB');
  assert.equal(volumeWords({ volume: { text: '−26.1 dB', muted: true } }), '\u2212\u221e dB');
  assert.equal(volumeWords({}), '');
});

ok('a_hold_given_back_before_a_reset_does_not_count_after_it', () => {
  const queued = [];
  const real = globalThis.setTimeout;
  globalThis.setTimeout = (fn) => { queued.push(fn); return 0; };
  try {
    const calls = [];
    const keep = keeper({ hold: (name, on) => calls.push([name, on]) }, 'shelf');
    keep.take();
    keep.dropLater();
    keep.reset();
    keep.take();
    queued.splice(0).forEach((fn) => fn());
    assert.deepEqual(calls.at(-1), ['shelf', true], 'the hold taken after the reset is still held');
    keep.dropLater();
    queued.splice(0).forEach((fn) => fn());
    assert.deepEqual(calls.at(-1), ['shelf', false], 'a give-back with no reset between counts');
  } finally {
    globalThis.setTimeout = real;
  }
});

ok('the_slim_bar_sits_at_the_safe_area_and_the_focus_keeps_clear_of_both_bars', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/signal/style.css', import.meta.url), 'utf8');
  const slim = css.match(/\.sg-slim \{([^}]*)\}/)[1];
  assert.match(slim, /top: env\(safe-area-inset-top, 0px\)/);
  assert.match(slim, /height: 64px/);
  assert.match(css, /\.sg-shelf :is\(button, input, \[role=slider\], \[tabindex\]\) \{ scroll-margin: 72px 0 96px; \}/);
  assert.match(css, /\.sg-askbox \{ scroll-margin: 72px 0 96px;/);
});


/** An amp's page with the DAC `Bifrost 3` before it, linked by a hop of the configuration. */
function linkedAmp(name = 'amp') {
  const page = pageOf(name);
  const amp = page.fleet.devices[0];
  const dac = { name: 'Bifrost 3', url: 'Bifrost%203', family: 'dac', family_word: 'DAC', link: amp.link };
  page.fleet.devices.unshift(dac);
  page.fleet.hops = [{ id: 'link-1', kind: 'amp', name: amp.name, note: '', after: dac.url, device: amp.url, live: true }];
  page.shown = dac.url;
  page.followed = { [amp.url]: page.device };
  page.device = { ...clone(page.device), name: 'Bifrost 3', url: dac.url, family: 'dac', volume: { text: '\u221223.5 dB', muted: false } };
  return { page, amp, dac, hop: { id: shownHopId(amp.url), kind: 'device', url: amp.url } };
}

ok('an_amp_opens_power_meters_and_faults', () => {
  const { page, hop } = linkedAmp();
  const html = openers.device(page, hop, view);
  // power first, with the switch, the amp's switches and its readouts
  assert.ok(html.indexOf('id="sg-amp-power"') < html.indexOf('id="sg-amp-meters"')
    && html.indexOf('id="sg-amp-meters"') < html.indexOf('id="sg-amp-about"'));
  const power = tags(html, 'button').find((b) => b.id === 'sg-power');
  assert.deepEqual([power['data-act'], power['data-key'], power['aria-pressed'], power.text], ['power', 'power', 'true', 'On']);
  const toggle = tags(html, 'button').find((b) => b.id === 'sg-pt-power_meters');
  assert.equal(toggle['data-act'], 'power-toggle');
  assert.ok(html.includes('data-readout="warming_up"'), 'the readouts are facts of the area');
  // the meters: a scale per channel, the class A share shaded, the needle at the fraction
  const scales = tags(html, 'div').filter((d) => classes(d).includes('sg-mscale'));
  assert.deepEqual(scales.map((d) => d['data-key']), ['meter_1', 'meter_2']);
  const zones = tags(html, 'span').filter((x) => classes(x).includes('sg-zone'));
  assert.deepEqual(zones.map((z) => z['data-at']), ['0.25000', '0.25000']);
  const needles = tags(html, 'span').filter((x) => classes(x).includes('sg-needle'));
  assert.deepEqual(needles.map((x) => x['data-at']), ['1.00000', '0.70704']);
  assert.ok(html.includes('data-meter="meter_1">100<'));
  // the amp itself: its name, Identify, Unlink with the hop's id, and the DAC's volume as the level
  assert.ok(html.includes('The Aegir 2'));
  const identify = tags(html, 'button').find((b) => b.id === 'sg-amp-identify');
  assert.equal(identify['data-act'], 'amp-identify');
  const unlink = tags(html, 'button').find((b) => b.id === 'sg-amp-unlink');
  assert.deepEqual([unlink['data-act'], unlink['data-hop-id']], ['amp-unlink', 'link-1']);
  assert.ok(html.includes('the Bifrost 3\u2019s volume is the one that sets the level'));
  // an amp not read yet: the needles are hidden, the state is said; standby and warming up too
  const standby = pageOf('amp-standby');
  const quiet = openers.device(standby, { id: 'x', kind: 'device', url: standby.shown }, view);
  assert.equal(tags(quiet, 'span').filter((x) => classes(x).includes('off')).length, 2);
  assert.ok(quiet.includes('<i>Standby: no readings</i>') && !quiet.includes('sg-amp-unlink'), 'nothing links the amp on its own');
  const warming = pageOf('amp-warming');
  const warm = openers.device(warming, { id: 'x', kind: 'device', url: warming.shown }, view);
  assert.equal(tags(warm, 'button').find((b) => b.id === 'sg-power').text, 'Warming up');
  assert.ok(warm.includes('data-readout="warming_up" class="sg-rv">yes<'));
  // faults: the fault fixture's rows (the fault family's ids, 230.x) and advice; a row is a fault by its id, not its words
  const bad = pageOf('fault');
  const faulty = openers.device(bad, { id: 'x', kind: 'device', url: bad.shown }, view);
  for (const label of ['Over-temperature', 'Over-current']) {
    assert.match(faulty, new RegExp(`<dt>${label}</dt><dd data-fact="${label}" class="sg-bad">active</dd>`), label);
  }
  assert.ok(faulty.includes('Switch the device off and let it cool down') && faulty.includes('Power-cycle the device.'), 'the advice');
  const renamed = pageOf('fault');
  renamed.device.facts.rows = renamed.device.facts.rows.map((r) => (r.id.startsWith('230.') ? { ...r, label: `Renamed ${r.id}` } : r));
  const still = openers.device(renamed, { id: 'x', kind: 'device', url: renamed.shown }, view);
  assert.ok(still.includes('<dt>Renamed 230.1</dt>') && still.includes('<dt>Renamed 230.2</dt>'), 'a renamed fault is still drawn');
  const hot = pageOf('amp');
  hot.device.facts.rows.push({ id: '21.3', label: 'Over-heated', value: 'no', heard_at_ms: null },
    { id: '21.1', label: 'Temperature reporting', value: 'false', heard_at_ms: null });
  const warm41 = openers.device(hot, { id: 'x', kind: 'device', url: hot.shown }, view);
  assert.ok(!warm41.includes('Over-heated') && !warm41.includes('reporting'), 'a row outside the fault family is not a fault');
  assert.ok(!html.includes('sg-faults') && !html.includes('Over-'), 'no fault rows, none drawn');
  // a followed amp not read yet is a quiet word, nothing to press
  delete page.followed[hop.url];
  const waiting = openers.device(page, hop, view);
  assert.ok(waiting.includes('Reading the amp') && !waiting.includes('<button'), waiting);
});

ok('the_chosen_dacs_name_shows_that_it_opens_the_device_list', () => {
  const hops = [{ id: 'device:A', kind: 'device', name: 'Bifrost 3', words: [], live: true, url: 'A' },
    { id: 'device:B', kind: 'device', name: 'Amp', words: [], live: true, url: 'B' }];
  const popup = (html) => (html.match(/aria-haspopup="listbox"/g) ?? []).length;
  assert.equal(popup(line(hops, 'device:A', 'device:A', true)), 1);
  assert.match(line(hops, 'device:A', 'device:A', true), /Bifrost 3 <span class="sg-more" aria-hidden="true">\u25BE<\/span>/);
  assert.equal(popup(line(hops, 'device:A', 'device:A', false)), 0, 'no other device, nothing to open');
  assert.equal(popup(line(hops, 'device:A', 'device:B', true)), 0, 'not chosen: a tap chooses');
});

ok('the_amp_has_its_power_once_per_page_and_the_shelf_leaves_it_out', () => {
  const { page, hop } = linkedAmp();
  const device = page.followed[hop.url];
  const opened = openers.device(page, hop, view);
  const shelf = drawShelf(page, device, view, { power: true, identify: true });
  assert.equal(count(opened + shelf, 'id', 'sg-power'), 1);
  assert.equal(count(opened + shelf, 'id', 'sg-pt-power_meters'), 1);
  assert.equal(count(opened + shelf, 'data-readout', 'warming_up'), 1);
  assert.equal(count(opened + shelf, 'id', 'sg-identify') + count(opened + shelf, 'id', 'sg-amp-identify'), 1);
  assert.ok(shelf.includes('id="sg-facts-fold"'), 'the facts stay on the shelf');
  assert.ok(drawShelf(page, device, view).includes('id="sg-power"'), 'a shelf without the option draws it');
});

ok('an_amps_controls_are_inert_for_a_viewer_who_may_not_write', () => {
  for (const mine of [(p) => { p.viewer.guest = true; p.viewer.writes = false; }, (p) => { p.viewer.writes = false; }]) {
    const { page, hop } = linkedAmp();
    mine(page);
    const html = openers.device(page, hop, view) + shelvesOf(page, hop);
    for (const b of tags(html, 'button').filter((x) => x['data-act'] !== 'facts-fold')) {
      assert.equal(b['aria-disabled'], 'true', `${b['data-act']} is inert`);
    }
    assert.ok(tags(html, 'button').some((b) => b['data-act'] === 'amp-unlink'), 'Unlink is drawn, inert');
  }
});

ok('the_meters_are_put_in_place_without_drawing_the_area_again', () => {
  const { page, hop } = linkedAmp();
  const device = page.followed[hop.url];
  const needles = [{ at: '1.00000' }, { at: '0.70704' }];
  const texts = new Map();
  const box = {
    querySelectorAll: () => [0, 1].map((i) => ({
      dataset: { key: device.meters.meters[i].key },
      querySelector: () => ({
        dataset: needles[i], style: {}, classList: { toggle() {} },
      }),
    })),
    querySelector: (selector) => (selector.startsWith('[data-meter')
      ? { get textContent() { return texts.get(selector) ?? ''; }, set textContent(v) { texts.set(selector, v); } }
      : { innerHTML: '' }),
  };
  global.CSS = { escape: (x) => x };
  const next = clone(device);
  next.meters.meters[0].fraction = 0.5;
  next.meters.meters[0].text = '50';
  assert.equal(metersInPlace(box, next), true);
  assert.equal(needles[0].at, '0.50000');
  assert.equal(texts.get('[data-meter="meter_1"]'), '50');
  const other = clone(device);
  other.meters.meters[1].key = 'meter_9';
  assert.equal(metersInPlace(box, other), false, 'other meters: the area is drawn again');
  assert.equal(metersInPlace({ querySelectorAll: () => [] }, device), false, 'no scale drawn: draw');
  delete global.CSS;
});

ok('the_dac_keeps_its_volume_written_while_the_amp_is_chosen', () => {
  const { page, amp, dac } = linkedAmp();
  const hops = hopsOf(page, dac.url);
  const words = (html, id) => [...html.matchAll(new RegExp(`data-hop="${id.replace(/[%]/g, '%')}"[^>]*>(.*?)</div>`, 'g'))][0][1];
  const chosenDac = line(hops, shownHopId(dac.url), shownHopId(dac.url));
  assert.ok(!words(chosenDac, shownHopId(dac.url)).includes('23.5 dB'), 'the DAC chosen: its own screen has the volume');
  const chosenAmp = line(hops, shownHopId(dac.url), shownHopId(amp.url));
  const dacLabel = words(chosenAmp, shownHopId(dac.url));
  assert.ok(dacLabel.includes('<b>Bifrost 3</b>') && dacLabel.includes('class="sg-w sg-v">\u221223.5 dB</span>'), dacLabel);
  const muted = clone(page);
  muted.device.volume = { text: '\u221223.5 dB', muted: true };
  assert.ok(words(line(hopsOf(muted, dac.url), shownHopId(dac.url), 'player'), shownHopId(dac.url)).includes('>\u2212\u221e dB<'));
  const mute = clone(page);
  delete mute.device.volume;
  assert.ok(!words(line(hopsOf(mute, dac.url), shownHopId(dac.url), shownHopId(amp.url)), shownHopId(dac.url)).includes('sg-v'),
    'no volume read, none written');
});

/** What the shelf draws for the hop's device. */
function shelvesOf(page, hop) {
  return shelves.device(page, hop, view);
}

ok('an_amps_shelf_is_its_facts_and_not_its_power', () => {
  const { page, hop } = linkedAmp();
  const html = shelvesOf(page, hop);
  assert.ok(html.includes('id="sg-facts-fold"') && !html.includes('id="sg-power"') && !html.includes('id="sg-identify"'));
  assert.equal(shelvesOf({ ...page, followed: {} }, hop), '', 'a followed amp not read yet has no shelf');
});

/** A DAC's page with one amp svolder controls that no hop links, and three declared hops after the DAC
 * (and one on another DAC's line, between them in the file). */
function declaredPage() {
  const { page, amp, dac } = linkedAmp();
  const other = { name: 'Gungnir 2', url: 'Gungnir%202', family: 'dac', family_word: 'DAC' };
  page.fleet.devices.push(other);
  const row = (id, kind, after, more = {}) => ({ id, kind, name: id, note: null, after, device: null, live: false, ...more });
  page.fleet.hops = [row('Asgard 3', 'amp', dac.url, { note: 'gain low' }), row('Elsewhere', 'speakers', other.url),
    row('HD 650', 'headphones', dac.url), row('Cans', 'other', dac.url, { note: '<b>x</b>' })];
  return { page, amp, dac, rows: page.fleet.hops };
}
const declaredHop = (page, id) => ({ id: `hop:${id}`, kind: 'declared', hopId: id, name: id });

ok('the_sheet_offers_unlinked_amps_first', () => {
  const { page, amp, dac } = declaredPage();
  assert.deepEqual(unlinkedAmps(page).map((d) => d.url), [amp.url], 'the amp no hop links');
  const html = plusSheet(page);
  const chips = tags(html, 'button').filter((b) => b['data-act'] === 'hop-kind');
  assert.deepEqual(chips.map((c) => c.text), [`${amp.name} (connected)`, 'Amp', 'Headphones', 'Speakers', 'Other']);
  assert.deepEqual(chips.map((c) => c['aria-checked']), ['true', 'false', 'false', 'false', 'false'], 'the connected amp first and chosen');
  assert.equal(chips[0]['data-device'], amp.url);
  assert.ok(html.includes(`After the ${dac.name}`) && html.includes('id="sg-add-name"') && html.includes('id="sg-add-note"'));
  assert.match(html, /id="sg-add-name"[^>]* disabled/, 'a connected amp has its own name');
  assert.ok(html.includes('data-act="sheet-cancel"') && html.includes('id="sg-sheet-go">Add<'));
  // a linked amp is not offered; with none left the kinds come first, an Amp, declared
  page.fleet.hops.push({ id: 'l', kind: 'amp', name: amp.name, note: null, after: dac.url, device: amp.url, live: true });
  const plain = plusSheet(page);
  const kinds = tags(plain, 'button').filter((b) => b['data-act'] === 'hop-kind');
  assert.deepEqual(kinds.map((c) => c.text), ['Amp', 'Headphones', 'Speakers', 'Other']);
  assert.equal(kinds[0]['aria-checked'], 'true');
  assert.ok(!/id="sg-add-name"[^>]* disabled/.test(plain) && plain.includes('shows as declared'));
});

ok('a_linked_hop_edits_only_its_note', () => {
  const { page, rows } = declaredPage();
  const linked = { id: 'Aegir', kind: 'amp', name: 'Aegir 2', note: 'on the left', after: 'Bifrost%203', device: 'Amp', live: false };
  rows.push(linked);
  const form = editForm(linked);
  assert.ok(form.includes('id="sg-edit-note"') && form.includes('value="on the left"'));
  assert.ok(!form.includes('sg-edit-name') && !form.includes('hop-kind'), 'no name, no kind of its own');
  const declared = editForm(rows[0]);
  assert.ok(declared.includes('id="sg-edit-name"') && declared.includes('value="Asgard 3"') && declared.includes('value="gain low"'));
  assert.equal(tags(declared, 'button').filter((b) => b['data-act'] === 'hop-kind' && b['aria-checked'] === 'true')[0]['data-kind'], 'amp');
  // its open area (a linked hop whose device is not listed) offers the note, moves and Remove; no name field
  const html = declaredScreen(page, declaredHop(page, 'Aegir'), false);
  assert.equal(tags(html, 'button').find((b) => b.id === 'sg-hop-edit-open').text, 'Edit the note');
  assert.ok(html.includes('Linked to a device') && html.includes('which svolder does not list now'));
  // a live linked amp's tools: Edit the note, the moves, no Remove (Unlink is the amp's own)
  const tools = hopTools(linked, rows, false, { remove: false, form: true });
  assert.ok(tools.includes('sg-hop-edit-open') && tools.includes('sg-hop-later') && !tools.includes('hop-remove') && tools.includes('id="sg-hop-edit"'));
});

ok('a_declared_hop_says_where_it_sits_and_moves_by_the_whole_lists_places', () => {
  const { page, rows } = declaredPage();
  // the list: 0 Asgard 3 (line A), 1 Elsewhere (line B), 2 HD 650 (A), 3 Cans (A)
  assert.deepEqual(moves(rows[0], rows), { earlier: null, later: 2 });
  assert.deepEqual(moves(rows[2], rows), { earlier: 0, later: 3 });
  assert.deepEqual(moves(rows[3], rows), { earlier: 2, later: null });
  assert.deepEqual(moves(rows[1], rows), { earlier: null, later: null }, 'alone on its line');
  const html = declaredScreen(page, declaredHop(page, 'HD 650'), false);
  const by = (id) => tags(html, 'button').find((b) => b.id === id);
  assert.deepEqual([by('sg-hop-earlier')['data-to'], by('sg-hop-later')['data-to']], ['0', '3']);
  assert.deepEqual([by('sg-hop-earlier')['data-hop-id'], by('sg-hop-remove')['data-hop-id']], ['HD 650', 'HD 650']);
  assert.ok(html.includes('Declared by you · headphones') && html.includes('HD 650'));
  assert.ok(html.includes('It sits after the Asgard 3, before the Cans.'), html);
  const first = declaredScreen(page, declaredHop(page, 'Asgard 3'), false);
  assert.ok(first.includes('gain low') && first.includes(`It sits after the ${page.fleet.devices.find((d) => d.url === page.shown).name}, before the HD 650.`));
  assert.match(first, /id="sg-hop-earlier"[^>]* disabled/, 'first on its line: no Move earlier');
  assert.ok(!/id="sg-hop-later"[^>]* disabled/.test(first));
  const last = declaredScreen(page, declaredHop(page, 'Cans'), false);
  assert.match(last, /id="sg-hop-later"[^>]* disabled/);
  assert.ok(last.includes('at the end of the line') && last.includes('&lt;b&gt;x&lt;/b&gt;'), 'a note is escaped');
  assert.equal(declaredScreen(page, declaredHop(page, 'Nobody'), false), '', 'a hop the list no longer has draws nothing');
});

ok('the_sheet_and_the_declared_area_are_inert_for_a_viewer_who_may_not_write', () => {
  const { page } = declaredPage();
  const html = declaredScreen(page, declaredHop(page, 'HD 650'), true);
  for (const b of tags(html, 'button').filter((x) => !x.disabled)) assert.equal(b['aria-disabled'], 'true', `${b['data-act']} is inert`);
  const live = declaredScreen(page, declaredHop(page, 'HD 650'), false);
  assert.ok(!tags(live, 'button').some((b) => b['aria-disabled']));
});

ok('no_style_attribute_in_the_sheet_or_the_declared_area', () => {
  const { page } = declaredPage();
  page.fleet.hops[0].name = '<i onclick=1>x</i>';
  page.fleet.hops[0].note = '"><script>';
  const html = plusSheet(page) + editForm(page.fleet.hops[0]) + declaredScreen(page, declaredHop(page, page.fleet.hops[0].id), false);
  assert.ok(!/\sstyle=/.test(html), 'a style attribute');
  assert.ok(!html.includes('<i onclick') && !html.includes('<script>') && html.includes('&lt;i onclick=1&gt;'));
});

// ---- the player, the computer and the states (P4, P5, P6) ----

const ROUTE = {
  sink: { words: 'Bifrost 3 USB', rate_hz: 96000, format: 'S32LE', volume_pct: 80, muted: true },
  player_rate_hz: 48000,
  host_eq: true,
  eq_preset: 'Harman <b>',
  eq_autoeq: true,
  others_mixing: 2,
};
/** The `playing` fixture with the host the page watches: a track and a route, as the daemon sends. */
function hosted(route = ROUTE) {
  const page = pageOf('playing');
  page.host = { now_ms: 1_700_000_100_000, sink_label: false, buttons: true, path: null, route,
    track: page.device.playing.track };
  return page;
}

ok('the_player_opens_the_track_and_the_verdict', () => {
  const page = hosted();
  const html = playerScreen(page, 1_700_000_100_000);
  assert.match(html, /<h2 class="sg-title">Test Pressing<\/h2>/);
  assert.ok(html.includes('The Fakes'));
  assert.deepEqual(tags(html, 'button').map((b) => b['data-button']), ['previous', 'pause', 'next']);
  assert.ok(tags(html, 'button').every((b) => b['data-act'] === 'transport'));
  assert.ok(html.includes('id="sg-pp"') && html.includes('data-drawn="1700000100000"'), 'progress moves');
  assert.ok(html.includes('0:33 / 20:00'), html);
  for (const hop of page.device.playing.path.hops) assert.ok(html.includes(`<b>${hop.label}</b>`), hop.label);
  assert.ok(html.includes('untouched') && html.includes('sg-verdict sg-ok'), 'the verdict, in its class');
  const changed = pageOf('playing-changed-route');
  changed.host = hosted().host;
  const words = playerScreen(changed, 1_700_000_100_000);
  assert.ok(words.includes('resampled 48 → 96 kHz') && words.includes('sg-verdict sg-warn'), words);
  const none = pageOf('playing');
  none.host = { ...hosted().host, track: null };
  none.device.playing = { ...none.device.playing, track: null };
  assert.ok(playerScreen(none).includes('Nothing is playing.'));
  const paused = hosted();
  paused.host.track.status = 'paused';
  paused.device.playing.track.status = 'paused';
  assert.deepEqual(tags(playerScreen(paused, 1), 'button').map((b) => b['data-button']), ['previous', 'play', 'next']);
});

ok('the_player_goes_inert_for_a_viewer_who_may_not_write', () => {
  const page = hosted();
  page.viewer.guest = true;
  const buttons = tags(playerScreen(page, 1), 'button');
  assert.ok(buttons.length === 3 && buttons.every((b) => b['aria-disabled'] === 'true'));
});

ok('the_computer_opens_the_routes_facts_read_only', () => {
  const page = hosted();
  const html = computerScreen(page);
  const facts = Object.fromEntries([...html.matchAll(/<dt>([^<]*)<\/dt><dd[^>]*>([^<]*)<\/dd>/g)].map((m) => [m[1], m[2]]));
  assert.equal(facts['Sound card'], 'Bifrost 3 USB');
  assert.equal(facts['Rate and format'], '96 kHz S32LE');
  assert.equal(facts['Volume in PipeWire'], '80 %');
  assert.equal(facts.Muted, 'yes');
  assert.equal(facts['Host EQ'], 'on · Harman &lt;b&gt;', 'the preset is named, escaped');
  assert.equal(facts['Other streams'], '2 other streams mix in');
  assert.equal(facts['The sound’s way'], 'untouched');
  assert.ok(!html.includes('<b>'), 'a name is not markup');
  // read only: nothing acts, nothing takes the focus
  assert.ok(!/<(button|input|select|textarea|a)[\s>]/.test(html) && !/tabindex|data-act|role="(slider|option)"/.test(html), html);
  const off = computerScreen(hosted({ ...ROUTE, host_eq: false, eq_preset: null, others_mixing: 0,
    sink: { ...ROUTE.sink, muted: false, volume_pct: null } }));
  assert.ok(off.includes('<dt>Host EQ</dt><dd>off</dd>') && off.includes('<dt>Other streams</dt><dd>none</dd>'));
  assert.ok(off.includes('<dt>Muted</dt><dd>no</dd>') && off.includes('<dt>Volume in PipeWire</dt><dd>—</dd>'));
  const byHand = computerScreen(hosted({ ...ROUTE, eq_preset: null }));
  assert.ok(byHand.includes('<dt>Host EQ</dt><dd class="sg-warn">on</dd>'), 'on with no name');
  assert.ok(computerScreen(hosted({ ...ROUTE, sink: null })).includes('no sound card found'));
});

ok('the_computer_says_route_not_read_before_the_route_arrives', () => {
  const html = computerScreen(hosted(null));
  assert.ok(html.includes('route not read') && !html.includes('<dl'), html);
});

ok('a_lost_link_breaks_the_line_at_the_dac', () => {
  const page = hosted();
  page.device.card = pageOf('lost').device.card;
  page.fleet.hops = [{ id: 'HD 650', kind: 'headphones', name: 'HD 650', note: null, after: page.shown, device: null, live: false }];
  const hops = hopsOf(page, page.shown, 'open');
  const dac = hops.find((h) => h.id === shownHopId(page.shown));
  assert.equal(dac.trouble, 'bad');
  assert.deepEqual(dac.words, ['Gungnir 2 is out of reach']);
  const html = line(hops, dac.id, dac.id);
  assert.match(html, /<circle class="sg-lostring sg-t-bad"/, 'a hollow ring at the DAC');
  assert.match(html, /<line class="sg-after sg-still"/, 'after it, still');
  assert.ok(!html.includes('data-flow="after"'), 'no dashes move after the DAC');
  assert.ok(html.includes('data-flow="before"'), 'the player and the computer keep flowing');
  assert.equal(html.match(/sg-dimmed/g).length, 1, 'the hop after the DAC is dimmed, the others not');
  assert.ok(html.includes('sg-w sg-bad'));
  // the same line with the device well
  delete page.device.card;
  const well = line(hopsOf(page, page.shown, 'open'), dac.id, dac.id);
  assert.ok(!well.includes('sg-lostring') && well.includes('data-flow="after"'));
  // waiting and resting: an amber ring
  for (const state of ['waiting', 'resting']) {
    const wait = hopsOf(page, page.shown, state).find((h) => h.id === dac.id);
    assert.equal(wait.trouble, 'wait', state);
    assert.match(line(hopsOf(page, page.shown, state), dac.id, dac.id), /sg-lostring sg-t-wait/);
  }
  assert.equal(hopsOf(page, page.shown, 'lost').find((h) => h.id === dac.id).trouble, null, 'a lost stream greys the page, not the ring');
});

ok('the_lost_card_offers_try_now_and_the_seconds', () => {
  const held = pageOf('lost').device.card;
  const html = card(held, { quiet: false, now: 5000 });
  assert.ok(html.includes('Gungnir 2 is out of reach') && html.includes('role="status"'));
  assert.match(html, /<button type="button" class="sg-btn sg-pill" data-act="try" id="sg-try">Try now<\/button>/);
  assert.match(html, /<span class="sg-retry" data-secs="8" data-drawn="5000">next try in 8 s<\/span>/);
  assert.ok(card({ ...held, retry_in: 0 }).includes('trying again now'));
  const refused = card(pageOf('refused').device.card);
  assert.ok(!refused.includes('data-act="try"') && !refused.includes('sg-retry'), 'no Try now, no seconds');
  assert.ok(refused.includes('<code>svolder doctor</code>'));
  const fault = card(pageOf('fault').device.card);
  assert.ok(fault.includes('Over-current') && fault.includes('Power-cycle the device'), fault);
  assert.ok(card(held, { quiet: true }).includes('aria-disabled="true"'), 'a viewer who may not write gets it inert');
  assert.equal(card(null), '');
  for (const state of ['waiting', 'resting']) {
    const html2 = card(stateCard(state));
    assert.ok(html2.includes('sg-card sg-wait') && !html2.includes('data-act'), state);
  }
  assert.ok(stateCard('waiting').title === 'Reading the device again…' && stateCard('open') === null);
  assert.ok(!/\sstyle=/.test(html + refused + fault), 'no style attribute');
});

ok('a_device_in_trouble_keeps_what_was_last_heard_dimmed_and_inert', () => {
  const page = pageOf('lost');
  const hop = { id: shownHopId(page.shown), kind: 'device', url: page.shown };
  const html = openers.device(page, hop, { ...view, state: 'open' });
  assert.ok(html.startsWith('<section id="sg-card"'), 'the card comes first');
  assert.equal(html.match(/class="sg-(vol|tone) sg-stale" inert/g).length, 2, 'the volume and the curve, dimmed and inert');
  for (const b of tags(html.slice(html.indexOf('id="sg-volume"')), 'button')) assert.equal(b['aria-disabled'], 'true', `${b['data-act']} is quiet`);
  assert.ok(!/\sstyle=/.test(html));
  const well = openers.device(pageOf('dac'), hop, { ...view, state: 'open' });
  assert.ok(!well.includes('sg-card') && !well.includes('sg-stale'));
  const waiting = openers.device(pageOf('dac'), hop, { ...view, state: 'waiting' });
  assert.ok(waiting.includes('sg-card sg-wait') && waiting.includes('sg-stale'));
  const none = pageOf('unavailable');
  assert.ok(openers.device(none, { ...hop, url: none.shown }, { ...view, state: 'open' }).includes('is not available'), 'a card and no device model');
});

ok('a_fault_on_a_connected_device_keeps_its_controls_live', () => {
  const page = pageOf('dac');
  page.device.card = pageOf('fault').device.card;
  const hop = { id: shownHopId(page.shown), kind: 'device', url: page.shown };
  const html = openers.device(page, hop, { ...view, state: 'open' });
  assert.ok(html.startsWith('<section id="sg-card"') && html.includes('Over-current'), 'the card, with its faults');
  assert.ok(!html.includes('sg-stale') && !/ inert/.test(html), 'the columns are not dimmed or inert');
  assert.ok(!html.slice(html.indexOf('id="sg-volume"')).includes('aria-disabled'), 'a writer\'s controls stay live');
  const hops = hopsOf({ ...page, fleet: { ...page.fleet, hops: [{ id: 'HD', kind: 'headphones', name: 'HD 650', note: null,
    after: page.shown, device: null, live: false }] } }, page.shown, 'open');
  const dac = hops.find((h) => h.id === hop.id);
  assert.equal(dac.trouble, 'fault');
  const wire = line(hops, hop.id, hop.id);
  assert.match(wire, /sg-lostring sg-t-bad/, 'a red ring at the device');
  assert.ok(wire.includes('data-flow="after"') && !wire.includes('sg-still') && !wire.includes('sg-dimmed'), 'the line goes on');
});

ok('player_and_computer_hops_open_their_areas', () => {
  const page = hosted();
  assert.ok(openers.player(page, { id: 'player' }, { ...view, now: 1 }).includes('id="sg-player"'));
  assert.ok(openers.computer(page, { id: 'computer' }, view).includes('id="sg-computer"'));
  assert.ok(!/\sstyle=/.test(openers.player(page, {}, view) + openers.computer(page, {}, view)), 'no style attribute');
});

ok('the_strip_is_the_same_hops_a_step_apart_with_the_wire_under_them', () => {
  const hops = [
    { id: 'player', kind: 'player', name: 'Player', words: ['96 kHz', 'playing'], live: true, changes: false },
    { id: 'computer', kind: 'computer', name: 'PipeWire', words: ['EQ on'], live: true, changes: true },
    { id: 'device:A', kind: 'device', name: 'Bifrost 3', words: ['USB'], live: true, url: 'A', volume: '-20 dB' },
    { id: 'hop:Cans', kind: 'declared', name: 'Cans', words: ['your cans'], live: false },
  ];
  const html = strip(hops, 'device:A', 'device:A', true);
  const ids = [...html.matchAll(/data-hop="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(ids, hops.map((h) => h.id), 'the hops of the line, in its order, and no other');
  assert.deepEqual([...html.matchAll(/data-i="(\d+)"/g)].map((m) => Number(m[1])), [0, 1, 2, 3]);
  assert.equal([...html.matchAll(/role="option"/g)].length, 4);
  assert.equal((html.match(/aria-selected="true"/g) ?? []).length, 1);
  assert.match(html, /aria-haspopup="listbox"/, 'the chosen shown device still opens the list');
  // one line of words under a name (the strip is narrow); the line draws them all
  assert.ok(html.includes('class="sg-sop sg-k-device sg-cur"') && !html.includes('class="sg-hop'));
  assert.equal((html.match(/class="sg-w[ "]/g) ?? []).length, 4, 'one line each');
  // the wire: dots a step apart, from where the track starts, and the same dash loops as the line
  const xs = [...html.matchAll(/<circle[^>]* cx="(\d+)"/g)].map((m) => Number(m[1]));
  assert.deepEqual(xs.map((x, i) => x - xs[0] - i * HOP_PX), [0, 0, 0, 0]);
  assert.ok(html.includes('data-flow="before"') && html.includes('data-flow="after"'));
  assert.ok(html.includes('sg-changes'), 'where the computer changes the sound');
  assert.match(html, /<svg class="sg-wire" width="\d+" height="92"/);
  assert.match(html, /<div class="sg-pbox" aria-hidden="true"><\/div>$/, 'the box at the middle');
  assert.ok(strip(hops, 'device:A', 'hop:Cans').includes('class="sg-pbox sg-boxdecl"'), 'a ring chosen is boxed dashed');
  assert.equal(strip([], null, null), '');
  assert.equal(slide(2), 'translateX(-220px)');
  assert.equal(slide(0, 30), 'translateX(30px)');
  // a name made of markup is escaped here too, and nothing carries a style attribute
  const odd = strip([{ ...hops[0], name: '<i onclick=1>x</i>' }], 'player', 'player');
  assert.ok(!odd.includes('<i onclick') && odd.includes('&lt;i onclick=1&gt;'));
  assert.ok(!/\sstyle=/.test(html + odd), 'no style attribute');
});

ok('a_lost_link_on_the_strip_draws_the_same_ring_and_dims_what_follows', () => {
  const hops = [
    { id: 'device:A', kind: 'device', name: 'A', words: ['A is out of reach'], live: true, url: 'A', trouble: 'bad' },
    { id: 'hop:B', kind: 'declared', name: 'B', words: ['amp'], live: false },
  ];
  const html = strip(hops, 'device:A', 'device:A');
  assert.match(html, /<circle class="sg-lostring sg-t-bad"/);
  assert.match(html, /<line class="sg-after sg-still"/);
  assert.equal((html.match(/sg-dimmed/g) ?? []).length, 1);
  assert.ok(!html.includes('data-flow="after"'));
});

ok('each_value_of_the_band_table_names_its_column_for_the_phone_rows', () => {
  const page = pageOf('playing');
  const html = drawShelf(page, page.device, view);
  for (const head of ['Frequency', 'Gain', 'Q']) {
    assert.ok(html.includes(`<td data-head="${head}">`), head);
  }
});

ok('a_linked_amp_shown_alone_draws_a_grey_way_in_then_a_thin_stretch_from_the_dac', () => {
  const hop = (id, kind, name, more = {}) => ({ id, kind, name, words: [], live: true, changes: false, url: null, ...more });
  const hops = [hop('player', 'player', 'Strawberry'), hop('computer', 'computer', 'This computer', { changes: true }),
    hop('device:A', 'device', 'Bifrost 3', { url: 'A', via: true }), hop('device:B', 'device', 'Asgard 3', { url: 'B' })];
  const html = line(hops, 'device:B', 'device:B', true);
  const lines = html.match(/<line [^>]*>/g);
  const grey = lines.filter((l) => l.includes('class="sg-grey"'));
  assert.equal(grey.length, 1, 'one grey stretch');
  assert.match(grey[0], /x1="10\.00%" x2="63\.33%"/, 'from the player to the DAC');
  const after = lines.filter((l) => l.includes('sg-after'));
  assert.equal(after.length, 1);
  assert.match(after[0], /x1="63\.33%" x2="90\.00%"/, 'thin from the DAC to the amp');
  assert.ok(after[0].includes('data-flow="after"'));
  assert.ok(!html.includes('data-flow="before"') && !html.includes('sg-changes'), 'nothing flows before the DAC');
  assert.equal((html.match(/sg-grey-dot/g) ?? []).length, 2, 'the player and the computer are greyed');
  // a DAC's own line is unchanged: green before it, thin after it
  const own = line([hops[0], hops[1], { ...hops[2], via: undefined }], 'device:A', 'device:A', true);
  assert.ok(own.includes('data-flow="before"') && !own.includes('sg-grey'));
});


ok('the_menu_row_adds_a_device_while_a_device_is_shown', () => {
  const [row] = theme.menuRows();
  assert.deepEqual([row.id, row.elementId, row.kind, row.label], ['add-device', 'menu-add-device', 'action', 'Add device…']);
  assert.equal(row.available(), false, 'not mounted');
  const was = { ...bound };
  try {
    let ran = 0;
    bound.add = () => { ran++; };
    bound.core = { model: { shown: () => null } };
    assert.equal(row.available(), false, 'no device shown');
    bound.core = { model: { shown: () => 'Gungnir 2' } };
    bound.canAdd = () => false;
    assert.equal(row.available(), false, 'the sheet cannot open');
    bound.canAdd = () => true;
    assert.equal(row.available(), true, 'a device shown');
    row.run();
    assert.equal(ran, 1);
    bound.add = null;
    assert.equal(row.available(), false, 'unmounted');
  } finally {
    Object.assign(bound, was);
  }
  assert.ok(!theme.menuButton.fixed, 'the theme itself does not name the placing');
});

console.log(`signal-draw.mjs: ${n} passed`);
