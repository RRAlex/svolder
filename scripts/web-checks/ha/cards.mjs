/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node check of svolder's Home Assistant cards (contrib/home-assistant/svolder/cards/) over a mocked
// `hass` in jsdom: what each card shows, the ONE write per gesture (callService calls are counted),
// commands off (sensors, no write control moves anything), the scenes card's rows, chips and answer,
// and an older daemon. Run: node cards.mjs
import assert from 'node:assert/strict';
import { page } from '../node/dom.mjs';

const { window } = page('');
Object.assign(globalThis, {
  window, document: window.document, HTMLElement: window.HTMLElement, customElements: window.customElements,
});
const root = '../../../contrib/home-assistant/svolder/cards/';
await import(`${root}index.js`);
const { ageWord } = await import(`${root}shared.js`);

/** A `hass` over `states` (`{ id: [state, attributes] }`) that records every service call. */
function mock(states, now = '2026-11-19T10:00:00Z') {
  const calls = [];
  const hass = { states: {}, calls, callService: (domain, service, data) => calls.push([domain, service, data]) };
  const put = (id, [state, attributes = {}]) => {
    hass.states[id] = { entity_id: id, state, attributes, last_updated: now, last_changed: now };
  };
  for (const [id, v] of Object.entries(states)) put(id, v);
  hass.put = put;
  return hass;
}
const mount = (tag, config, hass) => {
  const el = document.createElement(tag);
  document.body.appendChild(el);
  el.setConfig({ type: `custom:${tag}`, ...config });
  el.hass = hass;
  return el;
};
const $ = (el, sel) => el.shadowRoot.querySelector(sel);
const $$ = (el, sel) => [...el.shadowRoot.querySelectorAll(sel)];
const text = (el, sel) => $(el, sel).textContent.replace(/\s+/g, ' ').trim();
const ptr = (target, type, x = 0, y = 0) => {
  const e = new window.Event(type, { bubbles: true, cancelable: true });
  Object.assign(e, { clientX: x, clientY: y, pointerId: 1 });
  target.dispatchEvent(e);
};
const click = (el) => el.dispatchEvent(new window.Event('click', { bubbles: true }));

const cases = [];
const test = (name, fn) => cases.push([name, fn]);

// ---- the faceplate ----
const FACE = {
  'number.dac_volume': ['-40', { min: -96.33, max: 0, step: 0.334, unit_of_measurement: 'dB' }],
  'switch.dac_mute': ['off'], 'switch.dac_phase_invert': ['on'],
  'select.dac_input': ['optical', { options: ['usb', 'optical', 'coax'] }],
  'select.dac_eq_preset': ['open ref', { options: ['flat', 'open ref', 'warm'] }],
  'sensor.dac_sample_rate': ['48', { unit_of_measurement: 'kHz' }],
  'binary_sensor.dac_s_pdif_locked': ['on'], 'binary_sensor.dac_usb_1_connected': ['on'], 'binary_sensor.dac_usb_2_connected': ['off'],
  'binary_sensor.dac_link': ['on'], 'sensor.dac_signal': ['-58', { unit_of_measurement: 'dBm' }],
  'sensor.dac_firmware': ['3'],
};
const FACE_CFG = {
  name: 'Desk DAC',
  entities: {
    volume: 'number.dac_volume', mute: 'switch.dac_mute', phase: 'switch.dac_phase_invert', input: 'select.dac_input',
    preset: 'select.dac_eq_preset', sample_rate: 'sensor.dac_sample_rate', spdif: 'binary_sensor.dac_s_pdif_locked',
    usb1: 'binary_sensor.dac_usb_1_connected', usb2: 'binary_sensor.dac_usb_2_connected', link: 'binary_sensor.dac_link',
    signal: 'sensor.dac_signal', firmware: 'sensor.dac_firmware',
  },
};
const readOnly = (states) => Object.fromEntries(Object.entries(states).map(([id, [state, attrs = {}]]) => {
  const { options, ...rest } = attrs; // a sensor carries no option list
  return [id.replace(/^(number|select)\./, 'sensor.').replace(/^(switch|text)\./, 'binary_sensor.'), [state, rest]];
}));
const roCfg = (cfg) => JSON.parse(JSON.stringify(cfg).replace(/"(number|select)\./g, '"sensor.').replace(/"switch\./g, '"binary_sensor.'));

test('faceplate: header, dial, keys, readouts, chips and the link strip', () => {
  const el = mount('svolder-faceplate-card', FACE_CFG, mock(FACE));
  assert.equal(text(el, '.nm'), 'Desk DAC');
  assert.equal(text(el, '.sub'), 'fw 3 · 48 kHz');
  assert.match(text(el, '.link'), /connected/);
  assert.equal(text(el, '.dial .db'), '−40.0 dB');
  assert.equal(text(el, '.dial .raw'), 'raw 74/127');
  const keys = $$(el, '.key');
  assert.deepEqual(keys.map((k) => [k.textContent, k.getAttribute('aria-pressed')]), [['Mute', 'false'], ['Phase invert', 'true']]);
  const rw = $$(el, '.rw').map((r) => r.textContent.replace(/\s+/g, ' ').trim());
  assert.deepEqual(rw, ['Sample rate48 kHz', 'S/PDIFlocked', 'USB 1connected', 'USB 2not connected']);
  assert.deepEqual($$(el, '[data-group=input] .chip').map((c) => [c.dataset.option, c.getAttribute('aria-pressed')]),
    [['usb', 'false'], ['optical', 'true'], ['coax', 'false']]);
  assert.equal($$(el, '[data-group=preset] .chip').length, 3);
  assert.match(text(el, '.strip'), /Link -58 dBm|Link −58 dBm|Link -58/);
});

test('faceplate: a turn of the dial is ONE set_value when the gesture ends', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const dial = $(el, '.dial');
  ptr(dial, 'pointerdown', -1, 1);
  for (const [x, y] of [[-1, 0], [-1, -1], [0, -1]]) ptr(dial, 'pointermove', x, y);
  assert.deepEqual(hass.calls, [], 'no write while the pointer moves');
  const preview = text(el, '.dial .db');
  assert.notEqual(preview, '−40.0 dB', 'the dial shows the preview');
  ptr(dial, 'pointerup', 0, -1);
  assert.equal(hass.calls.length, 1);
  const [domain, service, data] = hass.calls[0];
  assert.deepEqual([domain, service, data.entity_id], ['number', 'set_value', 'number.dac_volume']);
  assert.ok(data.value > -40 && data.value <= 0, `value ${data.value}`);
  assert.equal(`${data.value < 0 ? '−' : ''}${Math.abs(data.value).toFixed(1)} dB`, preview);
});

test('faceplate: a tap on the dial writes nothing; the value never leaves its range', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const dial = $(el, '.dial');
  ptr(dial, 'pointerdown', -1, 1);
  ptr(dial, 'pointerup', -1, 1);
  assert.deepEqual(hass.calls, []);
  ptr(dial, 'pointerdown', -1, 1);
  for (const [x, y] of [[-1, 0], [-1, -1], [0, -1], [1, -1], [1, 0], [1, 1]]) ptr(dial, 'pointermove', x, y);
  ptr(dial, 'pointerup', 1, 1);
  assert.equal(hass.calls.length, 1);
  assert.ok(hass.calls[0][2].value <= 0 && hass.calls[0][2].value >= -96.33);
});

test('faceplate: a tap after a drag the daemon has not answered writes nothing (the preview is not a value)', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const dial = $(el, '.dial');
  ptr(dial, 'pointerdown', -1, 1);
  ptr(dial, 'pointermove', 0, -1);
  ptr(dial, 'pointerup', 0, -1);
  assert.equal(hass.calls.length, 1);
  ptr(dial, 'pointerdown', -1, 1);
  ptr(dial, 'pointerup', -1, 1);
  assert.equal(hass.calls.length, 1, 'the second gesture moved nothing');
});

test('faceplate: the keys toggle and the chips select, one call each', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  click($$(el, '.key')[0]);
  click($$(el, '.key')[1]);
  click($(el, '[data-group=input] [data-option=coax]'));
  click($(el, '[data-group=input] [data-option=optical]'));
  assert.deepEqual(hass.calls, [
    ['switch', 'turn_on', { entity_id: 'switch.dac_mute' }],
    ['switch', 'turn_off', { entity_id: 'switch.dac_phase_invert' }],
    ['select', 'select_option', { entity_id: 'select.dac_input', option: 'coax' }],
  ]);
});

const BAL = { 'number.dac_balance': ['0', { min: -127, max: 127, step: 1 }] };
const BAL_CFG = { ...FACE_CFG, entities: { ...FACE_CFG.entities, balance: 'number.dac_balance' } };
const change = (el, value) => { el.value = String(value); el.dispatchEvent(new window.Event('input', { bubbles: true })); el.dispatchEvent(new window.Event('change', { bubbles: true })); };

test('faceplate: the balance slider shows the entity range and writes once, when let go', () => {
  const hass = mock({ ...FACE, ...BAL });
  const el = mount('svolder-faceplate-card', BAL_CFG, hass);
  const r = $(el, '.bal input');
  assert.deepEqual([r.min, r.max, r.step, r.value, r.disabled], ['-127', '127', '1', '0', false]);
  change(r, -12);
  assert.deepEqual(hass.calls, [['number', 'set_value', { entity_id: 'number.dac_balance', value: -12 }]]);
  assert.equal($$(el, '.rw').filter((x) => !x.classList.contains('bal')).length, 4, 'the readout rows are unchanged');
});

test('faceplate: balance with commands off and no range is a readout line, no slider', () => {
  const hass = mock(readOnly({ ...FACE, 'number.dac_balance': ['63'] }));
  const el = mount('svolder-faceplate-card', roCfg(BAL_CFG), hass);
  assert.equal($(el, '.bal input'), null);
  assert.ok($$(el, '.rw').some((r) => /^Balance\s*63$/.test(r.textContent.replace(/\s+/g, ' ').trim())));
  assert.deepEqual(hass.calls, []);
});

const tick = () => new Promise((r) => setTimeout(r, 0));

test('faceplate: input alone writes nothing; the one write comes on change', async () => {
  const hass = mock({ ...FACE, ...BAL });
  const el = mount('svolder-faceplate-card', BAL_CFG, hass);
  const r = $(el, '.bal input');
  r.value = '10'; r.dispatchEvent(new window.Event('input', { bubbles: true }));
  r.value = '20'; r.dispatchEvent(new window.Event('input', { bubbles: true }));
  assert.deepEqual(hass.calls, []);
  r.dispatchEvent(new window.Event('pointerup', { bubbles: true }));
  r.dispatchEvent(new window.Event('change', { bubbles: true }));
  await tick();
  assert.deepEqual(hass.calls, [['number', 'set_value', { entity_id: 'number.dac_balance', value: 20 }]]);
});

test('faceplate: a drag that ends where it began releases the hold, the dial follows the next state', async () => {
  const hass = mock({ ...FACE, ...BAL });
  const el = mount('svolder-faceplate-card', BAL_CFG, hass);
  const r = $(el, '.bal input');
  r.value = '5'; r.dispatchEvent(new window.Event('input', { bubbles: true }));
  r.value = '0'; r.dispatchEvent(new window.Event('input', { bubbles: true }));
  r.dispatchEvent(new window.Event('pointerup', { bubbles: true })); // no change event: the value did not move
  await tick();
  assert.deepEqual(hass.calls, []);
  el.hass = mock({ ...FACE, ...BAL, 'number.dac_volume': ['-20', { min: -96.33, max: 0, step: 0.334 }] });
  assert.equal(text(el, '.dial .db'), '−20.0 dB');
});

test('eq: the tone control mode is a row of chips, the on one marked; a chip selects it', () => {
  const hass = mock(bandStates());
  const el = eqMount(hass);
  assert.deepEqual($$(el, '.modes .chip').map((c) => [c.dataset.mode, c.getAttribute('aria-pressed')]),
    [['flat', 'false'], ['eq', 'true'], ['loudness', 'false']]);
  assert.equal(text(el, '.modes .leg'), 'Mode');
  click($$(el, '.modes .chip')[0]);
  click($$(el, '.modes .chip')[1]);
  assert.deepEqual(hass.calls, [['select', 'select_option', { entity_id: 'select.dac_tone_control_mode', option: 'flat' }]]);
});

test('eq: mode chips with commands off show the value and move nothing', () => {
  const hass = mock(readOnly(bandStates()));
  const el = eqMount(hass, roCfg(eqCfg()));
  assert.deepEqual($$(el, '.modes .chip').map((c) => [c.dataset.mode, c.getAttribute('aria-pressed')]), [['eq', 'true']]);
  assert.ok($$(el, '.modes .chip').every((c) => c.disabled));
  for (const c of $$(el, '.modes .chip')) click(c);
  assert.deepEqual(hass.calls, []);
});

test('faceplate: commands off, every control is a sensor and nothing writes', () => {
  const hass = mock(readOnly(FACE));
  const el = mount('svolder-faceplate-card', roCfg(FACE_CFG), hass);
  assert.ok($$(el, '.key').every((k) => k.disabled));
  assert.ok($$(el, '.chip').every((k) => k.disabled));
  assert.equal($(el, '.dial').getAttribute('aria-readonly'), 'true');
  for (const k of $$(el, '.key, .chip')) click(k);
  const dial = $(el, '.dial');
  ptr(dial, 'pointerdown', -1, 1);
  ptr(dial, 'pointermove', 0, -1);
  ptr(dial, 'pointerup', 0, -1);
  assert.deepEqual(hass.calls, []);
  assert.equal(text(el, '.dial .db'), '−40.0 dB');
  assert.equal($$(el, '[data-group=input] .chip').length, 1, 'the current input only: a sensor has no option list');
});

test('faceplate: a new state is not painted under a dial gesture', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const dial = $(el, '.dial');
  ptr(dial, 'pointerdown', -1, 1);
  hass.put('sensor.dac_signal', ['-70', { unit_of_measurement: 'dBm' }]);
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal($(el, '.dial'), dial, 'the same element, not rebuilt');
  ptr(dial, 'pointerup', -1, 1);
  assert.notEqual($(el, '.dial'), dial, 'painted once the gesture ended');
  assert.match(text(el, '.strip'), /70/);
});

test('faceplate: unavailable states read as dashes, a bare config draws', () => {
  const hass = mock({ ...FACE, 'number.dac_volume': ['unavailable', {}], 'binary_sensor.dac_link': ['unavailable'] });
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  assert.equal(text(el, '.dial .db'), '—');
  assert.match(text(el, '.link'), /unavailable/);
  const bare = mount('svolder-faceplate-card', { entities: {} }, mock({}));
  assert.ok($(bare, '.card'));
  assert.throws(() => document.createElement('svolder-faceplate-card').setConfig({}), /entities/);
});

test('age words', () => {
  const at = (s) => ageWord({ last_updated: '2026-11-19T10:00:00Z' }, Date.parse('2026-11-19T10:00:00Z') + s * 1000);
  assert.deepEqual([at(12), at(1800), at(7200), at(200000)], ['12 s', '30 min', '2 h', '2 d']);
});

// ---- the EQ card ----
const bandStates = () => {
  const s = {};
  const set = [[31, 6, 0.5], [213, -2.6, 1], [1232, -2.2, 0.5], [1736, 4.8, 0.9], [14801, -4.3, 0.5]];
  set.forEach(([f, g, q], i) => {
    const n = i + 1;
    s[`number.dac_band_${n}_frequency`] = [String(f), { min: 20, max: 20000, step: 1 }];
    s[`number.dac_band_${n}_gain`] = [String(g), { min: -12, max: 12, step: 0.1 }];
    s[`number.dac_band_${n}_q`] = [String(q), { min: 0.1, max: 10, step: 0.1 }];
  });
  s['select.dac_eq_preset'] = ['open ref', { options: ['flat', 'open ref', 'warm'] }];
  s['select.dac_tone_control_mode'] = ['eq', { options: ['flat', 'eq', 'loudness'] }];
  s['number.dac_loudness_strength'] = ['0', { min: 0, max: 127, step: 1 }];
  return s;
};
const eqCfg = (dom = 'number') => ({
  name: 'EQ',
  entities: {
    preset: 'select.dac_eq_preset', mode: 'select.dac_tone_control_mode', strength: 'number.dac_loudness_strength',
    bands: [1, 2, 3, 4, 5].map((n) => ({
      n, gain: `${dom}.dac_band_${n}_gain`, frequency: `${dom}.dac_band_${n}_frequency`, q: `${dom}.dac_band_${n}_q`,
    })),
  },
});
const eqMount = (hass, cfg = eqCfg()) => {
  const el = mount('svolder-eq-card', cfg, hass);
  $(el, '.plot').getBoundingClientRect = () => ({ left: 0, top: 0, width: 300, height: 96 });
  return el;
};

test('eq: header, five band boxes on one line, Q in the readout line, preamp and loudness', () => {
  const el = eqMount(mock(bandStates()));
  assert.equal(text(el, '.eqh .nm'), 'EQ');
  assert.equal(text(el, '.eqh .sub'), 'open ref');
  assert.deepEqual($$(el, '.eqh .key').map((k) => k.textContent), ['Flat', 'Preset…']);
  const boxes = $$(el, '.band').map((b) => b.textContent);
  assert.deepEqual(boxes, ['31 Hz+6.0 dB', '213 Hz−2.6 dB', '1.23 kHz−2.2 dB', '1.74 kHz+4.8 dB', '14.8 kHz−4.3 dB']);
  assert.match($(el, '.bands').getAttribute('style'), /repeat\(5/);
  assert.match(text(el, '.read'), /Band 1: 31 Hz \+6\.0 dB\s?Q 0\.5/);
  assert.equal($$(el, '.pt').length, 5);
  assert.match(text(el, '.foot'), /Loudness off/);
  click($$(el, '.band')[1]);
  assert.match(text(el, '.read'), /Band 2: 213 Hz −2\.6 dB\s?Q 1\.0/);
});

test('eq: a diagonal drag is two writes on release, a vertical drag one, never per move', () => {
  const hass = mock(bandStates());
  const el = eqMount(hass);
  const hit = () => $$(el, '.hit')[1];
  ptr(hit(), 'pointerdown', 100, 40);
  const svg = $(el, '.plot');
  for (const [x, y] of [[110, 40], [140, 30], [160, 20]]) ptr(svg, 'pointermove', x, y);
  assert.deepEqual(hass.calls, [], 'nothing while dragging');
  assert.ok(!/^213 Hz/.test(text(el, '.band[data-i="1"]')), 'the box shows the preview');
  ptr(svg, 'pointerup', 160, 20);
  assert.equal(hass.calls.length, 2);
  assert.deepEqual(hass.calls.map((c) => [c[0], c[1], c[2].entity_id]), [
    ['number', 'set_value', 'number.dac_band_2_frequency'], ['number', 'set_value', 'number.dac_band_2_gain']]);
  hass.calls.length = 0;
  const el2 = eqMount(mock(bandStates()));
  const h2 = el2.hass;
  const svg2 = $(el2, '.plot');
  ptr($$(el2, '.hit')[2], 'pointerdown', 150, 40);
  ptr(svg2, 'pointermove', 150, 20);
  ptr(svg2, 'pointerup', 150, 20);
  assert.deepEqual(h2.calls.map((c) => c[2].entity_id), ['number.dac_band_3_gain']);
});

test('eq: a press on a point with no move writes nothing; the other controls write once per tap', () => {
  const hass = mock(bandStates());
  const el = eqMount(hass);
  ptr($$(el, '.hit')[0], 'pointerdown', 40, 30);
  ptr($(el, '.plot'), 'pointerup', 40, 30);
  assert.deepEqual(hass.calls, []);
  click($(el, '[data-flat]'));
  click($(el, '[data-open]'));
  click($(el, '[data-preset=warm]'));
  click($(el, '[data-q="1"]'));
  assert.deepEqual(hass.calls.map((c) => [c[1], c[2].entity_id, c[2].option ?? c[2].value]), [
    ['select_option', 'select.dac_eq_preset', 'flat'], ['select_option', 'select.dac_eq_preset', 'warm'],
    ['set_value', 'number.dac_band_1_q', 0.6]]);
});

test('eq: commands off, the curve is drawn and nothing can be dragged or written', () => {
  const states = readOnly(bandStates());
  const hass = mock(states);
  const cfg = roCfg(eqCfg());
  const el = eqMount(hass, cfg);
  assert.ok($(el, '.plot').classList.contains('ro'));
  ptr($$(el, '.hit')[1], 'pointerdown', 100, 40);
  ptr($(el, '.plot'), 'pointermove', 160, 20);
  ptr($(el, '.plot'), 'pointerup', 160, 20);
  for (const b of $$(el, '.eqh .key')) click(b);
  assert.ok($$(el, '.eqh .key').every((b) => b.disabled));
  assert.equal($$(el, '[data-q]').length, 0);
  assert.deepEqual(hass.calls, []);
  assert.equal($$(el, '.band').length, 5);
});

test('eq: the bands say they wait for eq mode when the mode is another', () => {
  const s = bandStates();
  s['select.dac_tone_control_mode'] = ['flat', { options: ['flat', 'eq', 'loudness'] }];
  const el = eqMount(mock(s));
  assert.match(text(el, '.read'), /Bands apply in eq mode \(now flat\)/);
  assert.ok($(el, '.plot').classList.contains('off'));
});

// ---- the chain card ----
const CHAIN = {
  'switch.pc_eq': ['on'], 'sensor.pc_eq_preset': ['open ref'],
  'switch.pc_loudness': ['off'], 'number.pc_loudness_strength': ['64', { min: 0, max: 127, step: 1 }],
  'switch.pc_crossfeed': ['on'], 'select.pc_crossfeed_preset': ['mild', { options: ['mild', 'strong'] }],
  'switch.pc_virtual_speakers': ['off'], 'select.pc_virtual_speakers_set': ['near', { options: ['near', 'far'] }],
  'sensor.pc_path': ['direct'], 'binary_sensor.pc_pipewire_config': ['off'],
};
const CHAIN_CFG = {
  name: 'pc',
  entities: {
    eq: 'switch.pc_eq', eq_preset: 'sensor.pc_eq_preset', loudness: 'switch.pc_loudness', loudness_strength: 'number.pc_loudness_strength',
    crossfeed: 'switch.pc_crossfeed', crossfeed_preset: 'select.pc_crossfeed_preset', speakers: 'switch.pc_virtual_speakers',
    speakers_set: 'select.pc_virtual_speakers_set', path: 'sensor.pc_path', config: 'binary_sensor.pc_pipewire_config',
  },
};

test('chain: four numbered modules with their switch, one setting, a picture and a why', () => {
  const el = mount('svolder-chain-card', CHAIN_CFG, mock(CHAIN));
  const mods = $$(el, '.mod');
  assert.deepEqual(mods.map((m) => m.querySelector('.no').textContent + m.querySelector('.nm').textContent),
    ['1EQ', '2Loudness', '3Crossfeed', '4Speakers']);
  assert.deepEqual(mods.map((m) => m.classList.contains('on')), [true, false, true, false]);
  assert.ok(mods.every((m) => m.querySelector('svg')));
  assert.match(text(el, '[data-mod="2"] .val'), /Strength 50%/);
  assert.equal(text(el, '[data-mod="3"] .why'), 'exclusive with speakers');
  assert.match(text(el, '.foot'), /Path direct/);
});

test('chain: a switch tap is one call; a slider is one write when it is released; a list is one write per choice', () => {
  const hass = mock(CHAIN);
  const el = mount('svolder-chain-card', CHAIN_CFG, hass);
  click($(el, '[data-sw="2"]'));
  const range = $(el, 'input[type=range]');
  ptr(range, 'pointerdown');
  range.value = '70';
  range.dispatchEvent(new window.Event('input', { bubbles: true }));
  range.value = '90';
  range.dispatchEvent(new window.Event('input', { bubbles: true }));
  assert.equal(hass.calls.length, 1, 'only the switch so far');
  range.dispatchEvent(new window.Event('change', { bubbles: true }));
  const sel = $(el, '[data-sel=xf]');
  sel.value = 'strong';
  sel.dispatchEvent(new window.Event('change', { bubbles: true }));
  assert.deepEqual(hass.calls, [
    ['switch', 'turn_on', { entity_id: 'switch.pc_loudness' }],
    ['number', 'set_value', { entity_id: 'number.pc_loudness_strength', value: 90 }],
    ['select', 'select_option', { entity_id: 'select.pc_crossfeed_preset', option: 'strong' }],
  ]);
});

test('chain: both exclusive modules on is flagged; commands off writes nothing', () => {
  const on = { ...CHAIN, 'switch.pc_virtual_speakers': ['on'] };
  const el = mount('svolder-chain-card', CHAIN_CFG, mock(on));
  assert.ok($(el, '[data-mod="3"]').classList.contains('clash'));
  assert.match(text(el, '[data-mod="4"] .why'), /pick one/);
  const hass = mock(readOnly(CHAIN));
  const ro = mount('svolder-chain-card', roCfg(CHAIN_CFG), hass);
  assert.ok($$(ro, '.key').every((k) => k.disabled));
  assert.equal($$(ro, 'input, select').length, 0);
  for (const k of $$(ro, '.key')) click(k);
  assert.deepEqual(hass.calls, []);
});

// ---- the scenes card ----
const SCENES = {
  scenes: [
    { name: 'Late night', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }] },
    { name: 'Desk', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }] },
    { name: 'Movie', parts: [{ who: 'computer', present: true }, { who: 'Desk DAC', present: true }, { who: 'Hall', present: false }] },
  ],
  known: true, recalled: null,
};
const sceneHass = (attrs = {}, state = 'Late night', extra = {}) => mock({
  'select.pc_scene': [state, { options: ['Late night', 'Desk', 'Movie', 'custom'], ...SCENES, ...attrs }],
  'text.pc_scene_save': [''], ...extra,
});
const SCFG = { entities: { scene: 'select.pc_scene', save: 'text.pc_scene_save' } };

test('scenes: one row per scene, custom never listed, the lamp on the state', () => {
  const el = mount('svolder-scenes-card', SCFG, sceneHass());
  assert.deepEqual($$(el, '.row .name').map((n) => n.textContent), ['Late night', 'Desk', 'Movie']);
  assert.deepEqual($$(el, '.row').map((r) => r.classList.contains('on')), [true, false, false]);
  assert.deepEqual($$(el, '.row .lamp').map((l) => l.classList.contains('on')), [true, false, false]);
  const custom = mount('svolder-scenes-card', SCFG, sceneHass({}, 'custom'));
  assert.ok($$(custom, '.row.on').length === 0);
});

test('scenes: the common chips are one column; only the extra chip goes under its row', () => {
  const el = mount('svolder-scenes-card', SCFG, sceneHass());
  const rows = $$(el, '.row');
  const common = rows.map((r) => [...r.querySelector('.parts').children].map((c) => c.textContent));
  assert.deepEqual(common, [['computer', 'Desk DAC'], ['computer', 'Desk DAC'], ['computer', 'Desk DAC']]);
  assert.equal($$(el, '.parts.extra').length, 1);
  assert.equal(rows[2].querySelector('.parts.extra').textContent, 'Hall · not connected');
  assert.ok(rows[2].querySelector('.parts.extra .part.off'));
});

test('scenes: a tap recalls with one select_option; Save writes the text entity', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  click($$(el, '.row')[2]);
  const input = $(el, '.saver input');
  input.value = '  Quiet  ';
  input.dispatchEvent(new window.Event('input', { bubbles: true }));
  click($(el, '[data-save]'));
  assert.deepEqual(hass.calls, [
    ['select', 'select_option', { entity_id: 'select.pc_scene', option: 'Movie' }],
    ['text', 'set_value', { entity_id: 'text.pc_scene_save', value: 'Quiet' }],
  ]);
  click($(el, '[data-save]'));
  assert.equal(hass.calls.length, 2, 'an empty name writes nothing');
});

test('scenes: the answer shows once the outcome names what was sent', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  assert.equal(text(el, '.say'), '');
  click($$(el, '.row')[2]);
  const outcome = { name: 'Movie', parts: [{ who: 'computer', status: 'done' }, { who: 'Desk DAC', status: 'done' },
    { who: 'Hall', status: 'left', notes: ['not connected'] }] };
  const other = { name: 'Desk', parts: [{ who: 'computer', status: 'done' }] };
  hass.put('select.pc_scene', ['Movie', { options: ['Late night', 'Desk', 'Movie', 'custom'], ...SCENES, outcome: other }]);
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal(text(el, '.say'), '', "another scene's outcome is not ours");
  hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'], last_updated: 'later',
    attributes: { ...hass.states['select.pc_scene'].attributes, outcome } };
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal(text(el, '.say'), 'Movie recalled. Hall skipped: not connected.');
});

test('scenes: a refused or asked recall shows the daemon words, and several notes join', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  click($$(el, '.row')[2]);
  const put = (outcome) => {
    hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'],
      attributes: { ...hass.states['select.pc_scene'].attributes, outcome } };
    el.hass = { ...hass, states: { ...hass.states } };
  };
  put({ name: 'Movie', error: 'Movie plays twice. Recall it anyway?', seq: 1 });
  assert.equal(text(el, '.say'), 'Movie not recalled: Movie plays twice. Recall it anyway?');
  click($$(el, '.row')[2]);
  put({ name: 'Movie', parts: [{ who: 'Hall', status: 'partial', notes: ['a', 'b'] }], seq: 2 });
  assert.equal(text(el, '.say'), 'Movie recalled. Hall partial: a; b.');
});

test('scenes: a save answer, and the daemon refusal in its words', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  const type = (name) => {
    const input = $(el, '.saver input');
    input.value = name;
    input.dispatchEvent(new window.Event('input', { bubbles: true }));
    click($(el, '[data-save]'));
  };
  const answer = (saved, stamp) => {
    hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'], last_updated: stamp,
      attributes: { ...hass.states['select.pc_scene'].attributes, saved } };
    el.hass = { ...hass, states: { ...hass.states } };
  };
  type('Quiet');
  answer({ name: 'Quiet' }, 't1');
  assert.equal(text(el, '.say'), 'Quiet saved.');
  type('Late night');
  answer({ name: 'Late night', error: 'a scene with that name exists' }, 't2');
  assert.equal(text(el, '.say'), 'Late night not saved: a scene with that name exists');
});

test('scenes: a new state while the name is typed keeps the draft, the focus and the caret', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  const input = $(el, '.saver input');
  input.focus();
  input.value = 'Qui';
  input.setSelectionRange(1, 2);
  input.dispatchEvent(new window.Event('input', { bubbles: true }));
  hass.put('select.pc_scene', ['Desk', { options: ['Late night', 'Desk', 'Movie', 'custom'], ...SCENES }]);
  el.hass = { ...hass, states: { ...hass.states } };
  assert.ok($$(el, '.row')[1].classList.contains('on'), 'painted');
  const now = $(el, '.saver input');
  assert.equal(now.value, 'Qui');
  assert.equal(el.shadowRoot.activeElement, now, 'focus kept');
  assert.deepEqual([now.selectionStart, now.selectionEnd], [1, 2]);
});

test('scenes: Enter saves, the field keeps focus, and the answer is painted', () => {
  const hass = sceneHass();
  const el = mount('svolder-scenes-card', SCFG, hass);
  const input = $(el, '.saver input');
  input.focus();
  input.value = 'Quiet';
  input.dispatchEvent(new window.Event('input', { bubbles: true }));
  input.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  assert.deepEqual(hass.calls, [['text', 'set_value', { entity_id: 'text.pc_scene_save', value: 'Quiet' }]]);
  hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'],
    attributes: { ...hass.states['select.pc_scene'].attributes, saved: { name: 'Quiet' } } };
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal(text(el, '.say'), 'Quiet saved.');
  assert.equal(el.shadowRoot.activeElement, $(el, '.saver input'));
});

test('scenes: a stale outcome is not the answer (state moved first, or only last_updated moved)', () => {
  const hass = sceneHass();
  const old = { name: 'Movie', parts: [{ who: 'Hall', status: 'left', notes: ['not connected'] }] };
  hass.states['select.pc_scene'].attributes.outcome = old;
  const el = mount('svolder-scenes-card', SCFG, hass);
  click($$(el, '.row')[2]);
  hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'], state: 'Movie', last_updated: 'later' };
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal(text(el, '.say'), '', 'the select moved, the outcome of the earlier recall is not republished yet');
  const fresh = { name: 'Movie', parts: [{ who: 'Hall', status: 'done' }], seq: 2 };
  hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'],
    attributes: { ...hass.states['select.pc_scene'].attributes, outcome: fresh } };
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal(text(el, '.say'), 'Movie recalled.');
});

test('scenes: an older daemon (no scenes attribute) lists the options alone, no chips', () => {
  const hass = mock({ 'select.pc_scene': ['Desk', { options: ['Late night', 'Desk', 'custom'] }], 'text.pc_scene_save': [''] });
  const el = mount('svolder-scenes-card', SCFG, hass);
  assert.deepEqual($$(el, '.row .name').map((n) => n.textContent), ['Late night', 'Desk']);
  assert.equal($$(el, '.part').length, 0);
  assert.ok($$(el, '.row')[1].classList.contains('on'));
});

test('scenes: commands off, the scene is a sensor: rows without a tap, no save field', () => {
  const hass = mock({ 'sensor.pc_scene': ['Late night', { ...SCENES }] });
  const el = mount('svolder-scenes-card', { entities: { scene: 'sensor.pc_scene' } }, hass);
  assert.deepEqual($$(el, '.row .name').map((n) => n.textContent), ['Late night', 'Desk', 'Movie']);
  assert.equal($$(el, 'button').length, 0);
  assert.equal($$(el, '.saver').length, 0);
  for (const r of $$(el, '.row')) click(r);
  assert.deepEqual(hass.calls, []);
});

test('scenes: the empty text is not a cell of the rows grid, so it wraps at the card width', () => {
  const hass = sceneHass({ scenes: [], known: true }, 'custom');
  const el = mount('svolder-scenes-card', SCFG, hass);
  const none = $(el, '.none');
  assert.equal(none.textContent, 'No scenes saved yet.');
  assert.equal(none.closest('.rows'), null, 'the 14px lamp column of .rows would hold the text one word per line');
});

test('scenes: a scene still being read says so, never "none"', () => {
  const hass = sceneHass({ known: false }, 'custom');
  const el = mount('svolder-scenes-card', SCFG, hass);
  assert.match(el.shadowRoot.textContent, /reading/);
  assert.ok(!/No scenes saved/.test(el.shadowRoot.textContent));
});

test('every card names its stub config and its grid size', () => {
  const hass = mock({ ...FACE, ...bandStates(), ...CHAIN, 'select.pc_scene': ['custom', { options: ['custom'] }] });
  for (const tag of ['svolder-faceplate-card', 'svolder-eq-card', 'svolder-chain-card', 'svolder-scenes-card']) {
    const cls = customElements.get(tag);
    const stub = cls.getStubConfig(hass);
    assert.equal(stub.type, `custom:${tag}`);
    const el = new cls();
    el.setConfig(stub);
    assert.ok(el.getCardSize() > 0);
    assert.equal(el.getGridOptions().columns, 12);
  }
  assert.deepEqual(window.customCards.map((c) => c.type).sort(),
    ['svolder-chain-card', 'svolder-eq-card', 'svolder-faceplate-card', 'svolder-scenes-card']);
});

test('the cards never write on their own: setConfig and hass calls make no service call', () => {
  const hass = mock({ ...FACE, ...bandStates(), ...CHAIN, 'select.pc_scene': ['custom', { options: ['custom'] }] });
  mount('svolder-faceplate-card', FACE_CFG, hass);
  eqMount(hass);
  mount('svolder-chain-card', CHAIN_CFG, hass);
  mount('svolder-scenes-card', { entities: { scene: 'select.pc_scene' } }, hass);
  assert.deepEqual(hass.calls, []);
});

// ---- fix 1: lifecycle, focus, keys, Q, hostile strings ----
test('chain: a press on the slider that moves nothing releases the hold; a detached card lets go too', () => {
  const hass = mock(CHAIN);
  const el = mount('svolder-chain-card', CHAIN_CFG, hass);
  const range = $(el, 'input[type=range]');
  ptr(range, 'pointerdown');
  ptr(range, 'pointerup');
  hass.put('switch.pc_loudness', ['on']);
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal($(el, '[data-sw="2"]').textContent, 'On', 'painted again');
  const el2 = mount('svolder-chain-card', CHAIN_CFG, hass);
  ptr($(el2, 'input[type=range]'), 'pointerdown');
  el2.remove();
  document.body.appendChild(el2);
  hass.put('switch.pc_loudness', ['off']);
  el2.hass = { ...hass, states: { ...hass.states } };
  assert.equal($(el2, '[data-sw="2"]').textContent, 'Off', 'a card put back after a detach is not stuck');
});

test('focus: a chip keeps focus across a repaint; the ticking link age is patched in place', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const chip = $(el, '[data-group=input] [data-option=coax]');
  chip.focus();
  assert.equal(el.shadowRoot.activeElement, chip);
  hass.put('select.dac_input', ['usb', { options: ['usb', 'optical', 'coax'] }]);
  el.hass = { ...hass, states: { ...hass.states } };
  const again = $(el, '[data-group=input] [data-option=coax]');
  assert.notEqual(again, chip, 'the card was rebuilt');
  assert.equal(el.shadowRoot.activeElement, again, 'and focus went to the same chip');
  const strip = $(el, '.strip');
  hass.states['sensor.dac_signal'] = { ...hass.states['sensor.dac_signal'], last_updated: new Date(Date.now() - 600000).toISOString() };
  const before = text(el, '.strip .age');
  el.hass = { ...hass, states: { ...hass.states } };
  assert.equal($(el, '.strip'), strip, 'an age change does not rebuild the card');
  assert.notEqual(text(el, '.strip .age'), before);
});

test('faceplate: a dial key press is one write on key up; PageUp is ten steps; End is not a key', () => {
  const hass = mock(FACE);
  const el = mount('svolder-faceplate-card', FACE_CFG, hass);
  const dial = $(el, '.dial');
  const key = (type, k) => dial.dispatchEvent(new window.KeyboardEvent(type, { key: k, bubbles: true, cancelable: true }));
  key('keydown', 'End');
  key('keyup', 'End');
  assert.deepEqual(hass.calls, []);
  key('keydown', 'PageUp');
  key('keyup', 'PageUp');
  assert.equal(hass.calls.length, 1);
  assert.ok(hass.calls[0][2].value > -40 && hass.calls[0][2].value < -30, hass.calls[0][2].value);
});

test('faceplate: a volume with no range of its own shows no raw step and no arc', () => {
  const s = readOnly(FACE);
  s['sensor.dac_volume'] = ['-40', { unit_of_measurement: 'dB' }];
  const el = mount('svolder-faceplate-card', roCfg(FACE_CFG), mock(s));
  assert.equal(text(el, '.dial .raw'), '');
  assert.equal($(el, '.dial .val').getAttribute('d'), '');
});

test('eq: arrow keys on a band box move frequency and gain by a step; one write per moved value on key up', () => {
  const hass = mock(bandStates());
  const el = eqMount(hass);
  const box = () => $$(el, '.band')[1];
  box().focus();
  const key = (type, k) => box().dispatchEvent(new window.KeyboardEvent(type, { key: k, bubbles: true, cancelable: true }));
  key('keydown', 'ArrowUp');
  key('keydown', 'ArrowUp');
  key('keydown', 'ArrowRight');
  assert.deepEqual(hass.calls, [], 'nothing until the key is released');
  assert.equal(el.shadowRoot.activeElement, box(), 'focus stays on the box while the preview redraws');
  assert.match(box().getAttribute('aria-label'), /^Band 2, \d+ Hz, /);
  key('keyup', 'ArrowRight');
  assert.deepEqual(hass.calls.map((c) => [c[2].entity_id, c[2].value]), [
    ['number.dac_band_2_frequency', 219], ['number.dac_band_2_gain', -2.4]]);
  const ro = eqMount(mock(readOnly(bandStates())), roCfg(eqCfg()));
  const rb = $$(ro, '.band')[0];
  rb.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
  rb.dispatchEvent(new window.KeyboardEvent('keyup', { key: 'ArrowUp', bubbles: true }));
  assert.deepEqual(ro.hass.calls, []);
});

test('eq: Q steps by the device (real Band Q: 0.5 to 8, step 0.01), two decimals shown', () => {
  const s = bandStates();
  s['number.dac_band_1_q'] = ['2', { min: 0.5, max: 8, step: 0.01 }];
  s['number.dac_band_1_gain'] = ['6', { min: -20, max: 6, step: 0.204 }];
  s['number.dac_band_1_frequency'] = ['31', { min: 20, max: 78.038, step: 1 }];
  const hass = mock(s);
  const el = eqMount(hass);
  assert.match(text(el, '.read'), /Q 2\.00/);
  click($(el, '[data-q="1"]'));
  click($(el, '[data-q="-1"]'));
  const [up, down] = hass.calls.map((c) => c[2].value);
  assert.ok(up - 2 >= 0.1 && 2 - down >= 0.1, [up, down]);
  assert.ok(up <= 8 && down >= 0.5);
});

test('chain: an unavailable setting is disabled and shows a dash; a state outside the options is shown', () => {
  const s = { ...CHAIN, 'number.pc_loudness_strength': ['unavailable', { min: 0, max: 127, step: 1 }],
    'select.pc_crossfeed_preset': ['custom thing', { options: ['mild', 'strong'] }] };
  const el = mount('svolder-chain-card', CHAIN_CFG, mock(s));
  assert.ok($(el, 'input[type=range]').disabled);
  assert.match(text(el, '[data-mod="2"] .val'), /—/);
  assert.ok(!/NaN/.test(el.shadowRoot.textContent));
  const first = $(el, '[data-sel=xf] option');
  assert.ok(first.disabled && first.selected && first.textContent === 'custom thing');
  assert.equal($(el, '[data-sw="1"]').getAttribute('aria-label'), 'EQ');
});

test('hostile strings (markup in names, options, units, scenes, parts, notes, refusals) make no element', () => {
  const bad = '<img src=x onerror=alert(1)>';
  const hass = mock({
    ...FACE, ...bandStates(), ...CHAIN,
    'select.dac_input': [bad, { options: [bad, 'usb'] }],
    'select.dac_eq_preset': [bad, { options: [bad, 'flat'] }],
    'sensor.dac_sample_rate': ['48', { unit_of_measurement: bad }],
    'sensor.dac_signal': [bad, { unit_of_measurement: bad }],
    'sensor.dac_firmware': [bad],
    'sensor.pc_path': [bad], 'binary_sensor.pc_pipewire_config': [bad], 'sensor.pc_eq_preset': [bad],
    'select.pc_crossfeed_preset': [bad, { options: [bad] }],
    'select.pc_scene': [bad, { options: [bad, 'custom'], scenes: [{ name: bad, parts: [{ who: bad, present: false }] }],
      outcome: { name: bad, parts: [{ who: bad, status: bad, notes: [bad] }] }, saved: { name: bad, error: bad } }],
    'text.pc_scene_save': [''],
  });
  const cfg = { ...FACE_CFG, name: bad };
  const eq = eqCfg();
  eq.name = bad;
  eq.entities.bands[0].n = bad;
  const cards = [mount('svolder-faceplate-card', cfg, hass), eqMount(hass, eq), mount('svolder-chain-card', { ...CHAIN_CFG, name: bad }, hass),
    mount('svolder-scenes-card', SCFG, hass)];
  click($$(cards[3], '.row')[0]);
  hass.states['select.pc_scene'] = { ...hass.states['select.pc_scene'], attributes: { ...hass.states['select.pc_scene'].attributes } };
  cards[3].hass = { ...hass, states: { ...hass.states } };
  for (const el of cards) {
    assert.equal(el.shadowRoot.querySelectorAll('img').length, 0, el.localName);
    assert.ok(!/onerror=/.test([...el.shadowRoot.querySelectorAll('*')].map((e) => [...e.attributes].map((a) => a.name).join(' ')).join(' ')));
  }
  assert.ok(el0(cards[3]).includes(bad), 'the text itself is shown as text');
});
const el0 = (el) => el.shadowRoot.textContent;

test('eq: focus leaving the card with an arrow held ends the gesture (one write set), a redraw does not', () => {
  const hass = mock(bandStates());
  const el = eqMount(hass);
  const box = () => $$(el, '.band')[1];
  box().focus();
  box().dispatchEvent(new window.KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }));
  assert.deepEqual(hass.calls, [], 'the redraw of the preview is not a focus loss');
  $(el, '.card').dispatchEvent(new window.FocusEvent('focusout', { bubbles: true, relatedTarget: null }));
  assert.deepEqual(hass.calls.map((c) => c[2].entity_id), ['number.dac_band_2_gain']);
  hass.put('switch.pc_eq', ['off']);
  el.hass = { ...hass, states: { ...hass.states } };
  assert.ok($(el, '.card'), 'and the card paints again');
  box().dispatchEvent(new window.KeyboardEvent('keyup', { key: 'ArrowUp', bubbles: true }));
  assert.equal(hass.calls.length, 1, 'the later key up writes nothing more');
});

test('accent: a theme or mode switch in hass.themes works the accent out again, with no repaint', () => {
  const colours = { on: 'rgb(255, 193, 7)', bg: 'rgb(255, 255, 255)', text: 'rgb(33, 33, 33)' };
  const real = globalThis.getComputedStyle;
  globalThis.getComputedStyle = (e) => ({ color: e.localName === 'i' ? colours.on : colours.text, backgroundColor: colours.bg });
  try {
    const hass = mock(FACE);
    hass.themes = { theme: 'default', darkMode: false };
    const el = mount('svolder-faceplate-card', FACE_CFG, hass);
    const card = $(el, '.card');
    assert.match(card.style.getPropertyValue('--sv-on'), /^rgb\(/, 'amber on white is blended toward the text');
    Object.assign(colours, { bg: 'rgb(28, 28, 28)', text: 'rgb(225, 225, 225)' });
    el.hass = { ...hass, states: { ...hass.states }, themes: { theme: 'default', darkMode: true } };
    assert.equal($(el, '.card'), card, 'not repainted');
    assert.equal(card.style.getPropertyValue('--sv-on'), '', 'amber on near-black needs no blend');
  } finally { globalThis.getComputedStyle = real; }
});

let failed = 0;
for (const [name, fn] of cases) {
  try { await fn(); console.log(`ok   ${name}`); } catch (e) { failed += 1; console.log(`FAIL ${name}\n     ${e.message}`); }
}
if (failed) process.exit(1);
console.log(`${cases.length} cases passed`);
