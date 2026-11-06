/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for what the theme Night drive draws of the neon sheet (themes/nightdrive/draw/):
// the order of its sections, the Loudness chip with its round steppers and strength, a field and
// two steppers for every band value, what every drawn control owes a guest or a read-only viewer,
// no `style` attribute anywhere. The markup is read with a small tag reader, no DOM.
// Run: node nightdrive-draw.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { drawSheet, sheetFrame } from '../../../bins/svolderd/web/themes/nightdrive/draw/sheet.js';
import { inputRow, modeRow } from '../../../bins/svolderd/web/themes/nightdrive/draw/modes.js';
import { bandTable } from '../../../bins/svolderd/web/themes/nightdrive/draw/bands.js';
import { drawControls } from '../../../bins/svolderd/web/themes/nightdrive/draw/controls.js';
import { drawFacts, drawFaults } from '../../../bins/svolderd/web/themes/nightdrive/draw/facts.js';
import { drawGauges } from '../../../bins/svolderd/web/themes/nightdrive/draw/gauges.js';
import { drawMeters } from '../../../bins/svolderd/web/themes/nightdrive/draw/meters.js';
import { drawSlots } from '../../../bins/svolderd/web/themes/nightdrive/draw/slots.js';
import { tunable } from '../../../bins/svolderd/web/themes/nightdrive/tune.js';
import { topLine } from '../../../bins/svolderd/web/themes/nightdrive/hud.js';
import { drawDash } from '../../../bins/svolderd/web/themes/nightdrive/draw/dash.js';
import { presetBox } from '../../../bins/svolderd/web/themes/nightdrive/draw/presets.js';
import { PALETTE } from '../../../bins/svolderd/web/themes/nightdrive/draw/palette.js';
import { preview } from '../../../bins/svolderd/web/themes/nightdrive/preview.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const view = { phone: () => false };

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
const classes = (el) => (el.class ?? '').split(' ');
const everything = (page) => [drawSheet(page, view), sheetFrame(), modeRow(page.device), inputRow(page.device),
  bandTable(page.device), presetBox(page.device), drawSlots(page.device), drawControls(page.device), drawFacts(page.device, true),
  drawFaults(page.device), drawGauges(page.device), drawMeters(page.device)].join('');

ok('the_sheet_runs_mode_input_then_bands_beside_presets', () => {
  const html = drawSheet(pageOf('dac'), view);
  const at = (id) => html.indexOf(`id="${id}"`);
  assert.ok(at('nd-mode') >= 0 && at('nd-mode') < at('nd-input'), 'Mode, then Input');
  assert.ok(at('nd-input') < at('nd-bands'), 'the band table under them');
  assert.ok(at('nd-bands') < at('nd-side') && at('nd-side') < at('nd-presets'), 'the presets beside the table, in the side column');
  assert.deepEqual(tags(html, 'section').map((s) => s.id), ['nd-mode', 'nd-input', 'nd-bands', 'nd-presets', 'nd-controls', 'nd-facts']);
  const frame = tags(sheetFrame(), 'div').find((d) => d.id === 'nd-dialog');
  assert.equal(frame.role, 'dialog');
  assert.equal(frame['aria-modal'], 'true');
  assert.ok(tags(sheetFrame(), 'button').some((b) => b['data-act'] === 'sheet-close'), 'a close key');
  const modes = tags(html, 'button').filter((b) => b['data-act'] === 'mode').map((b) => b['data-mode']);
  assert.deepEqual(modes, ['flat', 'eq', 'loudness']);
  const input = tags(html, 'button').filter((b) => b['data-act'] === 'input');
  assert.ok(input.length >= 2 && input.some((b) => b['aria-pressed'] === 'true'), 'the input as chips, one on');
  assert.equal(drawSheet({ ...pageOf('dac'), device: null }, view), '', 'nothing for no device');
});

ok('a_device_with_no_tone_draws_no_mode_bands_or_presets', () => {
  const page = pageOf('amp');
  const html = drawSheet(page, view);
  for (const id of ['nd-mode', 'nd-bands', 'nd-presets']) assert.ok(!html.includes(`id="${id}"`), id);
});

ok('loudness_carries_its_round_minus_and_plus_and_strength', () => {
  const html = modeRow(pageOf('dac-loudness-strength').device);
  const buttons = tags(html, 'button');
  const steps = buttons.filter((b) => b['data-act'] === 'loudness');
  assert.deepEqual(steps.map((b) => [b.id, b['data-step'], b.text]), [['nd-loudm', '-1', '−'], ['nd-loudp', '1', '+']]);
  assert.ok(steps.every((b) => classes(b).includes('nd-rb')), 'round');
  const chip = buttons.find((b) => b['data-mode'] === 'loudness');
  assert.equal(chip['aria-pressed'], 'true');
  assert.match(html, /<i>strength 40<\/i>/, 'the strength under the name');
  const order = [html.indexOf('id="nd-loudm"'), html.indexOf('id="nd-mode-loudness"'), html.indexOf('id="nd-loudp"')];
  assert.deepEqual(order, [...order].sort((a, b) => a - b), '− the chip +');
  const eq = modeRow(pageOf('dac').device);
  assert.ok(!eq.includes('nd-rb'), 'no round keys while Loudness is off');
});

ok('every_band_value_has_a_field_and_steppers', () => {
  const page = pageOf('dac');
  const html = bandTable(page.device);
  const rows = page.device.tone.bands.rows;
  assert.equal(rows.length, 5);
  const fields = tags(html, 'input');
  const steps = tags(html, 'button');
  for (const r of rows) {
    for (const p of ['f', 'g', 'q']) {
      const f = fields.find((x) => x.id === `nd-b${r.band}${p}`);
      assert.ok(f, `a field for band ${r.band} ${p}`);
      assert.equal(f.value, r[p]);
      assert.equal(f['data-band'], String(r.band));
      assert.ok(f['aria-label'], 'named');
      const down = steps.find((x) => x.id === `nd-b${r.band}${p}m`);
      const up = steps.find((x) => x.id === `nd-b${r.band}${p}p`);
      assert.deepEqual([down['data-step'], up['data-step']], ['-1', '1']);
      assert.ok(down['aria-label'] && up['aria-label']);
      assert.ok(down['data-band'] === String(r.band) && down['data-param'] === p);
    }
  }
  assert.equal(fields.length, 15);
  assert.equal(steps.length, 30);
});

ok('a_guests_sheet_is_drawn_inert', () => {
  for (const name of ['dac-guest', 'dac-readonly', 'dac-loudness-guest']) {
    const page = pageOf(name);
    const html = drawSheet(page, view);
    const buttons = tags(html, 'button');
    assert.ok(buttons.length > 20, name);
    // the fold of the facts only shows or hides what is read: it works for a guest too
    for (const b of buttons.filter((x) => x['data-act'] !== 'facts-fold')) {
      assert.equal(b['aria-disabled'], 'true', `${name}: ${b.id} is aria-disabled`);
      assert.ok(classes(b).includes('quiet'), `${name}: ${b.id} is quiet`);
    }
    const inputs = [...html.matchAll(/<input[^>]*>/g)].map((m) => m[0]);
    assert.equal(inputs.length, 15, `${name}: its fields`);
    assert.ok(inputs.every((i) => i.includes(' readonly')), `${name}: fields read only`);
    assert.ok(!html.includes('data-act="preset-save"'), `${name}: nothing to save with`);
  }
  // a viewer whose writes are off is inert though the device is not marked
  const page = pageOf('dac');
  page.viewer = { ...page.viewer, writes: false };
  page.device.inert = false;
  assert.ok(tags(drawSheet(page, view), 'button').filter((b) => b['data-act'] !== 'facts-fold').every((b) => b['aria-disabled'] === 'true'), 'writes off');
  const live = drawSheet(pageOf('dac'), view);
  assert.ok(!tags(live, 'button').some((b) => b['aria-disabled'] === 'true'), 'a writer\'s sheet is not inert');
});

ok('the_sheet_lists_every_control_the_model_has', () => {
  let seen = 0;
  for (const c of fixture.cases) {
    const device = c.page.device;
    if (!device) continue;
    const html = drawSheet(c.page, view);
    for (const control of device.controls ?? []) {
      if (control.key === 'input' && control.kind === 'choice') {
        assert.ok(html.includes('data-act="input') && html.includes('id="nd-input'), `${c.name}: the input`);
      } else if (control.kind === 'readout') {
        assert.ok(html.includes(`data-readout="${control.key}"`), `${c.name}: readout ${control.key}`);
      } else {
        assert.ok(html.includes(`data-key="${control.key}"`), `${c.name}: ${control.kind} ${control.key}`);
      }
      seen++;
    }
    if (device.power) assert.ok(html.includes(`data-act="power" id="nd-power-key" data-key="${device.power.key}"`), `${c.name}: power`);
    for (const t of device.power?.toggles ?? []) assert.ok(html.includes(`data-key="${t.key}"`), `${c.name}: switch ${t.key}`);
  }
  assert.ok(seen > 100, `${seen} controls looked at`);
  // by kind: a toggle is a pill that reads On or Off, a slider is a scale with a knob, a readout its words
  const html = drawControls(pageOf('dac').device);
  const toggle = tags(html, 'button').find((b) => b['data-key'] === 'phase');
  assert.equal(toggle['aria-pressed'], 'true');
  assert.equal(toggle.text, 'On');
  const scale = tags(html, 'div').find((d) => d.role === 'slider');
  assert.equal(scale['data-key'], 'balance');
  assert.ok(html.includes('class="nd-knob"') && html.includes('class="nd-ctr"'), 'a knob and its centre tick');
  assert.ok(html.includes('data-act="centre"'), 'a tap on the value goes to the centre');
});

ok('the_facts_start_folded', () => {
  const device = pageOf('dac').device;
  const folded = drawFacts(device, false);
  assert.match(folded, /aria-expanded="false"/);
  assert.match(folded, /<div id="nd-facts-body" hidden>/);
  assert.ok(folded.includes('show the facts'));
  const open = drawFacts(device, true);
  assert.match(open, /aria-expanded="true"/);
  assert.ok(!/id="nd-facts-body" hidden/.test(open));
  assert.ok(open.includes('data-fact="Firmware"') && open.includes('data-act="identify"'), 'the table and Identify');
  assert.equal(drawFacts(null, true), '');
  assert.ok(drawSheet(pageOf('dac'), { facts: { open: false } }).includes('id="nd-facts-body" hidden'));
  assert.ok(!drawSheet(pageOf('dac'), { facts: { open: true } }).includes('id="nd-facts-body" hidden'));
});

ok('a_loki_max_shows_its_slots_and_settings', () => {
  const page = pageOf('lokimax');
  const html = drawSheet(page, view);
  assert.deepEqual(tags(html, 'button').filter((b) => b['data-act'] === 'slot').map((b) => b['data-slot']), ['1', '2', '3']);
  assert.ok(html.includes('data-act="slot-save-list"'), 'a way to save the bands');
  assert.deepEqual(tags(html, 'button').filter((b) => b['data-act'] === 'setting').map((b) => b['data-name']),
    ['Flat', 'Bass Boost', 'Treble Boost', 'Smiley', 'Loudness']);
  assert.equal(tags(html, 'div').filter((d) => d.role === 'slider').length, 6, 'its six band levels as scales');
  assert.ok(html.indexOf('id="nd-slots"') < html.indexOf('id="nd-controls"'));
  assert.equal(drawSlots(pageOf('dac').device), '', 'a device with none draws none');
  // a guest recalls nothing and saves nothing, but the words are there
  const guest = drawSlots(pageOf('lokimax-guest').device, { inert: true });
  assert.ok(!guest.includes('slot-save-list'));
  assert.ok(tags(guest, 'button').every((b) => b['aria-disabled'] === 'true'));
});

ok('an_amps_dash_has_power_and_temperature_gauges', () => {
  const page = pageOf('amp');
  const device = { ...page.device, power: { ...page.device.power, readouts: [{ key: 'temp', label: 'Temperature', text: '41 \u00b0C' }] } };
  assert.equal(drawGauges({ ...device, power: undefined }), '');
  const gauges = tags(drawGauges(device), 'span').filter((s) => s['data-gauge']);
  assert.deepEqual(gauges.map((g) => g['data-gauge']), ['power', 'temperature']);
  assert.ok(gauges.every((g) => g.role === 'img' && g['aria-label']), 'each named for a screen reader');
  assert.ok(drawGauges(device).includes('stroke-dasharray="100 100"'), 'power on fills its arc');
  assert.ok(drawGauges(device).includes('stroke-dasharray="41 100"'), '41 degrees fills 41 of 100');
  const warming = drawGauges({ ...device, power: { ...device.power, on: true, word: 'Warming up', warming: true } });
  assert.ok(warming.includes('stroke-dasharray="50 100"'), 'warming up is half');
  const standby = drawGauges({ ...device, power: { ...device.power, on: false, word: 'Standby' } });
  assert.ok(standby.includes('stroke-dasharray="0 100"'));
  // a negative reading in the model's minus is a number too
  assert.ok(drawGauges({ ...device, power: { ...device.power, readouts: [{ key: 'temp', label: 'Temperature', text: '\u221210 \u00b0C' }] } }).includes('stroke-dasharray="0 100"'));
  const dash = drawDash({ ...page, device, shown: page.shown, fleet: { devices: [{ url: page.shown, family: 'amp' }] } }, view, 1000);
  assert.ok(dash.includes('data-gauge="power"') && dash.includes('data-gauge="temperature"'));
  assert.ok(!dash.includes('data-act="input"'), 'the gauges stand where the input would');
  assert.ok(dash.includes('data-act="power"'));
});

ok('an_amps_sheet_draws_its_meters_a_scale_with_a_needle_and_the_class_a_share_for_each_channel', () => {
  const page = pageOf('amp');
  const html = drawMeters(page.device);
  const scales = tags(html, 'div').filter((d) => classes(d).includes('nd-mscale'));
  assert.deepEqual(scales.map((d) => d['data-key']), page.device.meters.meters.map((m) => m.key));
  assert.equal(tags(html, 'span').filter((s) => classes(s).includes('nd-zone')).length, 2);
  assert.deepEqual(tags(html, 'span').filter((s) => classes(s).includes('nd-needle')).map((s) => s['data-at']),
    page.device.meters.meters.map((m) => m.fraction.toFixed(5)));
  assert.ok(html.includes('data-meter="meter_1"') && html.includes('L channel') && html.includes('class A'));
  // a meter not read yet has its needle hidden; a device with no meters draws none
  const unread = { ...page.device, meters: { ...page.device.meters, meters: [{ ...page.device.meters.meters[0], fraction: null }] } };
  assert.ok(drawMeters(unread).includes('nd-needle off'));
  assert.equal(drawMeters(pageOf('dac').device), '');
  assert.ok(drawSheet(page, view).includes('id="nd-meters"'), 'in the sheet');
});

ok('an_amp_never_morphs', () => {
  for (const name of ['amp', 'amp-standby', 'five', 'mono', 'fault']) {
    assert.equal(tunable(pageOf(name)), false, `${name}: no bubbles, no curve to tune`);
    assert.ok(!drawSheet(pageOf(name), view).includes('id="nd-bands"'), `${name}: no band table`);
  }
  assert.equal(tunable(pageOf('dac')), true);
});

ok('faults_are_listed_with_their_advice_and_turn_the_top_red', () => {
  const page = pageOf('fault');
  const html = drawFaults(page.device);
  assert.ok(html.includes('Over-current') && html.includes('Over-temperature'));
  assert.ok(html.includes('Power-cycle the device') && html.includes('let it cool down'), 'each with its advice');
  assert.equal(tags(html, 'dd').filter((d) => classes(d).includes('bad')).length, 2, 'active ones marked');
  assert.ok(drawSheet(page, view).indexOf('id="nd-faults"') < drawSheet(page, view).indexOf('id="nd-controls"'));
  const top = topLine(page, 'open');
  assert.equal(top.tone, 'bad', JSON.stringify(top));
  assert.notEqual(top.right, '');
  assert.equal(drawFaults(pageOf('amp').device), '', 'no faults, no section');
});

ok('nothing_in_the_sheet_has_a_style_attribute', () => {
  for (const c of fixture.cases) {
    if (!c.page.device) continue;
    const html = everything(c.page);
    assert.ok(!/\sstyle\s*=/i.test(html), `${c.name}: a style attribute`);
    assert.ok(!/<script|<style|\son\w+\s*=/i.test(html), `${c.name}: inline script or style`);
  }
  const named = pageOf('dac-user-preset');
  named.device.presets.rows[4].name = '<img src=x onerror=alert(1)>"';
  const html = everything(named);
  assert.ok(!html.includes('<img'), 'a preset name is escaped');
});

ok('the_thumbnails_sun_stripes_show_at_the_menus_size_in_the_palettes_colour', () => {
  const box = { innerHTML: '' };
  preview(box);
  const rects = [...box.innerHTML.matchAll(/<rect x="56" y="[\d.]+" width="48" height="([\d.]+)" fill="([^"]+)"\/>/g)];
  assert.ok(rects.length >= 2, `${rects.length} stripes`);
  // the menu draws the 160-unit thumbnail 32 px wide (frame.css, `.mthumb`): each stripe must be 1.5 px or more there
  for (const [, height, fill] of rects) {
    assert.ok(Number(height) * (32 / 160) >= 1.5, `a stripe ${height} units tall is ${(Number(height) * 32 / 160).toFixed(2)} px in the menu`);
    assert.equal(fill, PALETTE.sunStripe);
  }
});

console.log(`nightdrive-draw.mjs: ${n} passed`);
