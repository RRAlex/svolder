/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Device controls popup (core/popups/controls/draw.js, wire.js) and the core's
// driver for it (`core.driver.controls`, core/driver.js). First the draws, as strings: the markup of
// each kind of device (a DAC, an amp, a five-meter amp, a mono amp, the Loki Max, a player with a
// track) holds a control for every target `targetsOf` lists, and the computer's side one for every
// target `computerTargetsOf` lists; a viewer who may not write sees every control inert; words are
// the model's and escaped. Then the wire on the real core over a jsdom page (dom.mjs) with the
// core's actions faked (each call recorded): opening, each tap calling the right action of the core
// with the device the popup is about, the asks in place, the flip (which writes nothing), a guest's
// tap and a refused write saying their words, Escape and Back, a change of the shown device, a
// stepper that goes on from the step asked, a model change that waits for an ask. Then the driver.
// Run: node core-popups-controls.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { computerTargetsOf, targetsOf } from '../../../bins/svolderd/web/core/contract.js';
import { openAsks } from '../../../bins/svolderd/web/core/inline-ask.js';
import {
  IDS, WORDS, deviceSections, pageHtml, sayHtml, sideHtml, volumeWords,
} from '../../../bins/svolderd/web/core/popups/controls/draw.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const byName = Object.fromEntries(fixture.cases.map((c) => [c.name, c]));
const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };
const HOSTILE = '<img src=x onerror=alert(1)> "Q" & \'co\'';

/** The computer's model, as the host event carries it. */
const point = (type, f, g, q) => ({ type, f, g, q });
const computerModel = (over = {}) => ({
  volume: { pct: 42, muted: false, sink: 'Schiit' },
  eq: {
    on: true, running_unknown: false, preamp_db: -6, preset: 'Warm', live: true, note: null, max: 16,
    points: [point('PK', 1000, 3, 1), point('LSC', 100, -2, 0.7)], response: [],
    loudness: { on: true, strength_pct: 60 }, crossfeed: { on: false }, speakers: { on: false },
    levels: {
      eq_db: -7.8, space_db: 0, total_db: -7.8, climb_to_db: null, passthrough: false, untouched: false,
      words: 'plays −7.8 dB in all',
    },
    bypass: { on: false, held: { eq: false, loudness: false, crossfeed: false, speakers: false } },
  },
  presets: [
    { name: 'Warm', built_in: true, on: true, playable: true, why: null },
    { name: 'Huge', built_in: false, on: false, playable: false, why: '17 filters' },
  ],
  unavailable: null,
  ...over,
});

/** The page of the fixture's case `name`, with the computer's model in its host, for `viewer`. */
function pageOf(name, { computer = null, viewer } = {}) {
  const base = structuredClone(byName[name].page);
  return {
    ...base,
    viewer: viewer ?? base.viewer,
    host: computer ? { track: null, route: null, computer } : base.host,
    fleet: { devices: [{ url: base.shown, name: base.device?.name ?? 'Device' }], hops: [] },
  };
}

const jsdom = page('');
/** `html` parsed, to be searched by selector. */
const parsed = (html) => {
  const box = jsdom.document.createElement('div');
  box.innerHTML = html;
  return box;
};
const quoted = (text) => `"${String(text).replace(/["\\]/g, '\\$&')}"`;

/** The selector of the control a target is, written here from the attributes the draw documents
 * (draw.js's header) and not from the driver's. */
function selectorOf(target, device) {
  if ('computer' in target) {
    switch (target.computer) {
      case 'flip': return '[data-act="side"]';
      case 'volume': return '[data-act="c-volume-up"]';
      case 'mute': return '[data-act="c-mute"]';
      case 'mode': return '[data-act="c-mode"]';
      case 'bypass': return '[data-act="c-bypass"]';
      case 'add': return '[data-act="c-add"]';
      case 'remove': return `[data-act="c-remove"][data-n="${target.n}"]`;
      case 'point': return `[data-act="c-point"][data-n="${target.n}"][data-param="${target.param}"]`;
      case 'preset': return `[data-act="c-preset"][data-name=${quoted(target.name)}]`;
      default: throw new Error(JSON.stringify(target));
    }
  }
  if ('control' in target) {
    const key = target.control;
    if (key === device.volume?.key) return '[data-act="volume-up"]';
    if (key === device.volume?.mute) return '[data-act="mute"]';
    if (key === device.power?.key) return '[data-act="power"]';
    if ((device.power?.toggles ?? []).some((t) => t.key === key)) {
      return `[data-act="power-toggle"][data-key=${quoted(key)}]`;
    }
    if ((device.power?.readouts ?? []).some((r) => r.key === key)) return `[data-readout=${quoted(key)}]`;
    const control = device.controls.find((c) => c.key === key);
    return {
      choice: `[data-act="choice"][data-key=${quoted(key)}]`,
      toggle: `[data-act="toggle"][data-key=${quoted(key)}]`,
      slider: `[data-act="slider"][data-key=${quoted(key)}]`,
      readout: `[data-readout=${quoted(key)}]`,
    }[control.kind];
  }
  if ('band' in target) return `[data-act="band"][data-band="${target.band}"][data-param="${target.param}"]`;
  if ('mode' in target) return `[data-act="mode"][data-mode=${quoted(target.mode)}]`;
  if ('loudness' in target) return '[data-act="loudness"]';
  if ('preset' in target) return `[data-act="preset"][data-name=${quoted(target.preset)}]`;
  if ('slot' in target) return `[data-act="slot"][data-slot="${target.slot}"]`;
  if ('setting' in target) return `[data-act="setting"][data-name=${quoted(target.setting)}]`;
  if ('transport' in target) return `[data-act="transport"][data-button=${quoted(target.transport)}]`;
  throw new Error(JSON.stringify(target));
}

const model = (name, side = 'dac', { viewer, computer } = {}) => {
  const pg = pageOf(name, { viewer, computer });
  const inert = !(pg.viewer?.writes !== false && !pg.viewer?.guest) || Boolean(pg.device?.inert);
  return {
    device: pg.device, side, inert, writer: !inert,
    computer: computer ? { sw: { dac: pg.device.name, computer: 'This computer', side, unavailable: null } } : null,
  };
};

// ---- the draws ----

await ok('the_markup_of_each_kind_of_device_holds_a_control_for_every_target_of_targetsOf', () => {
  let checked = 0;
  for (const name of ['dac', 'dac-loudness', 'dac-user-preset', 'dac-presets-off', 'amp', 'five',
    'mono', 'lokimax', 'lokimax-flat', 'phono', 'playing', 'dac-muted']) {
    const { device } = byName[name].page;
    const box = parsed(pageHtml(model(name)));
    for (const target of targetsOf(device)) {
      checked++;
      const found = box.querySelectorAll(selectorOf(target, device));
      assert.ok(found.length >= 1, `${name}: no control for ${JSON.stringify(target)}`);
    }
  }
  assert.ok(checked > 150, `${checked} targets`);
  // the kinds are all there: the DAC, an amp (power), the Loki Max (slots, sliders), a player
  const dac = parsed(pageHtml(model('dac')));
  for (const act of ['volume-down', 'mute', 'volume-up', 'toggle', 'choice', 'slider', 'mode', 'band',
    'preset']) {
    assert.ok(dac.querySelector(`[data-act="${act}"]`), `the DAC has a ${act}`);
  }
  assert.ok(parsed(pageHtml(model('amp'))).querySelector('[data-act="power"]'), 'the amp has a power key');
  assert.ok(parsed(pageHtml(model('lokimax'))).querySelector('[data-act="slot-save"]'));
  assert.ok(parsed(pageHtml(model('playing'))).querySelector('[data-act="transport"][data-button="pause"]'));
});

await ok('a_section_a_device_does_not_offer_is_left_out', () => {
  const ids = (name) => deviceSections(byName[name].page.device, false).map((s) => s.id);
  assert.ok(!ids('amp').includes('ctl-mode'), 'an amp has no tone modes');
  assert.ok(!ids('amp').includes('ctl-bands'));
  assert.ok(!ids('mono').includes('ctl-volume'), 'a mono amp has no volume');
  assert.ok(ids('amp').includes('ctl-power') && !ids('amp').includes('ctl-volume'));
  // a device with nothing to operate says so, in a section of its own
  assert.deepEqual(ids('mono'), []);
  const empty = parsed(pageHtml(model('mono')));
  assert.equal(empty.querySelector('#ctl-body').textContent, WORDS.noControls);
  assert.equal(empty.querySelectorAll('button[data-act]').length, 0);
  assert.ok(!ids('dac').includes('ctl-loudness'), 'no strength outside Loudness');
  assert.ok(ids('dac-loudness').includes('ctl-loudness'));
  assert.ok(!ids('dac').includes('ctl-slots') && ids('lokimax').includes('ctl-slots'));
  assert.ok(!ids('dac').includes('ctl-transport'), 'no track, nothing to press');
  assert.ok(ids('playing').includes('ctl-transport'));
  assert.ok(!ids('no-player').includes('ctl-transport'), 'buttons but no track');
  assert.ok(!ids('playing-guest-other-sink').includes('ctl-transport'), 'a track but no buttons');
  assert.ok(!ids('dac-presets-off').includes('ctl-presets') === false, 'presets with writes off are still shown');
  const none = parsed(pageHtml({ device: null, computer: null, side: 'dac', inert: false, writer: false }));
  assert.equal(none.querySelector('#ctl-body').textContent, WORDS.noDevice);
});

await ok('the_computers_side_holds_a_control_for_every_target_of_computerTargetsOf', () => {
  const computer = computerModel();
  const pg = pageOf('dac', { computer });
  const targets = computerTargetsOf(pg);
  assert.ok(targets.length > 15, `${targets.length} targets`);
  const store = createCore({ page: structuredClone(pg), themes: [], window: jsdomWindow() });
  const view = store.computer.view(store.model.page());
  const html = pageHtml({
    device: pg.device, side: 'computer', inert: false, writer: true,
    computer: {
      sw: view.sw, volume: store.computer.volumeOf(store.model.page()), tone: store.computer.toneOf(store.model.page()),
      levels: store.computer.levelsOf(store.model.page()), bypass: store.computer.bypassOf(store.model.page()),
    },
  });
  const box = parsed(html);
  for (const target of targets) {
    assert.ok(box.querySelector(selectorOf(target, pg.device)), `no control for ${JSON.stringify(target)}`);
  }
  assert.equal(box.querySelectorAll('[data-stage]').length, 4, 'EQ, Loudness, Space and the total');
  assert.ok(box.querySelector('[data-act="sound"]'), 'a writer has the link to Sound');
  assert.ok(box.textContent.includes('plays −7.8 dB in all'), "the daemon's words for the levels");
  assert.equal(box.querySelectorAll('select[data-act="c-point"]').length, 2);
  // a reader has no link to Sound, and an unavailable computer says why and draws nothing more
  const reader = parsed(pageHtml({
    device: pg.device, side: 'computer', inert: true, writer: false,
    computer: { sw: view.sw, volume: null, tone: null, levels: null, bypass: null },
  }));
  assert.equal(reader.querySelector('[data-act="sound"]'), null);
  const off = parsed(pageHtml({
    device: pg.device, side: 'computer', inert: false, writer: true,
    computer: { sw: { ...view.sw, unavailable: 'computer controls unavailable: no pw-cli' } },
  }));
  assert.equal(off.querySelector('#ctl-body').textContent, 'computer controls unavailable: no pw-cli');
  assert.equal(off.querySelector('[data-act="side"][data-side="computer"]').getAttribute('aria-disabled'), 'true');
  store.stream?.close?.();
});

await ok('a_viewer_who_may_not_write_sees_every_control_inert', () => {
  for (const [name, side] of [['dac-guest', 'dac'], ['lokimax-guest', 'dac'], ['amp-guest', 'dac'],
    ['dac-readonly', 'dac']]) {
    const m = model(name, side);
    assert.equal(m.inert, true, name);
    const box = parsed(pageHtml(m));
    const controls = [...box.querySelectorAll('button[data-act]')]
      .filter((one) => !['side', 'sound'].includes(one.dataset.act));
    assert.ok(controls.length >= 2, `${name}: ${controls.length} controls`);
    for (const one of controls) {
      assert.equal(one.getAttribute('aria-disabled'), 'true', `${name}: ${one.id} is inert`);
      assert.equal(one.hasAttribute('disabled'), false, `${name}: ${one.id} takes a tap (to say why)`);
    }
  }
  const computer = computerModel();
  const pg = pageOf('dac-guest', { computer });
  const core = createCore({ page: structuredClone(pg), themes: [], window: jsdomWindow() });
  const html = pageHtml({
    device: pg.device, side: 'computer', inert: true, writer: false,
    computer: {
      sw: core.computer.view(core.model.page()).sw, volume: core.computer.volumeOf(core.model.page()),
      tone: core.computer.toneOf(core.model.page()), levels: null, bypass: core.computer.bypassOf(core.model.page()),
    },
  });
  const box = parsed(html);
  for (const one of box.querySelectorAll('input[data-act="c-point"]')) {
    assert.ok(one.hasAttribute('readonly') && one.getAttribute('aria-disabled') === 'true');
  }
  for (const one of box.querySelectorAll('select, button[data-act^="c-"]')) {
    assert.equal(one.getAttribute('aria-disabled'), 'true', one.outerHTML.slice(0, 80));
  }
});

await ok('the_volume_reads_the_models_words_and_minus_infinity_while_muted', () => {
  const { volume } = byName.dac.page.device;
  assert.equal(volumeWords(volume), volume.text);
  assert.equal(volumeWords({ ...volume, muted: true }), '−∞ dB');
  const box = (name) => parsed(pageHtml(model(name))).querySelector('[data-shown="volume"]').textContent;
  assert.equal(box('dac'), byName.dac.page.device.volume.text);
  assert.equal(box('dac-muted'), '−∞ dB', 'a muted DAC');
  const muted = parsed(pageHtml(model('dac-muted'))).querySelector('[data-act="mute"]');
  assert.equal(muted.getAttribute('aria-pressed'), 'true');
});

await ok('the_models_words_are_escaped_in_every_place_they_reach', () => {
  const device = structuredClone(byName['dac-user-preset'].page.device);
  device.presets.rows[device.presets.rows.length - 1].name = HOSTILE;
  device.name = HOSTILE;
  device.controls[0].label = HOSTILE;
  const html = pageHtml({ device, computer: null, side: 'dac', inert: false, writer: true });
  assert.ok(!html.includes('<img'), 'no tag of the words reaches the markup');
  const box = parsed(html);
  const own = box.querySelector('[data-act="preset"][aria-pressed="true"]:last-of-type, [data-act="preset-rename"]');
  assert.equal(own.dataset.name, HOSTILE, 'the name round-trips through its attribute');
  assert.equal(sayHtml({ text: HOSTILE, failed: true }).includes('<img'), false);
  assert.equal(sideHtml({ sw: { unavailable: null } }, 'dac', HOSTILE).includes('<img'), false);
});

await ok('the_say_line_is_an_alert_for_a_refusal_and_nothing_without_words', () => {
  assert.equal(sayHtml(null), '');
  assert.match(sayHtml({ text: 'No.', failed: true }), /role="alert"/);
  assert.match(sayHtml({ text: 'Done', failed: false }), /role="status"/);
  assert.ok(pageHtml(model('dac'), { text: 'The device did not answer.', failed: true })
    .includes(`id="${IDS.say}" aria-live="polite"><p class="fp-note" role="alert">The device did not answer.`));
});

// ---- the wire ----

function jsdomWindow() {
  const p = page('<div id="toasts"></div>');
  globalThis.window = p.window;
  globalThis.document = p.document;
  // jsdom's listeners take only its own signals
  globalThis.AbortController = p.window.AbortController;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  return p.window;
}

const BODY = '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const DEVICE_ACTIONS = ['setVolume', 'stopClimb', 'setControl', 'setMode', 'setBand', 'setLoudness',
  'recallPreset', 'savePreset', 'renamePreset', 'deletePreset', 'recallSlot', 'saveSlot',
  'applySetting', 'press'];
const COMPUTER_ACTIONS = ['step', 'toggleMute', 'setMode', 'setBypass', 'addNext', 'removePoint',
  'recallPreset', 'cell', 'setPoint'];

/** The real core over a jsdom page whose actions are faked: each call is recorded in `calls`
 * (`[name, ...arguments]`, the computer's as `computer.<name>`) and answers `answer()`. */
function rig({ name = 'dac', computer = null, viewer, answer = () => ({ ok: true, status: 200 }) } = {}) {
  const p = page(BODY);
  globalThis.window = p.window;
  globalThis.document = p.document;
  // jsdom's listeners take only its own signals
  globalThis.AbortController = p.window.AbortController;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  const core = createCore({ page: pageOf(name, { computer, viewer }), themes: [], window: p.window });
  const calls = [];
  for (const one of DEVICE_ACTIONS) {
    core.actions[one] = (...args) => { calls.push([one, ...args]); return Promise.resolve(answer(one, args)); };
  }
  for (const one of COMPUTER_ACTIONS) {
    core.actions.computer[one] = (...args) => {
      calls.push([`computer.${one}`, ...args]);
      return Promise.resolve(answer(`computer.${one}`, args));
    };
  }
  const $ = (id) => p.document.getElementById(id);
  const q = (selector) => p.document.querySelector(`#popup-controls ${selector}`);
  const landed = (act) => new Promise((resolve) => {
    p.window.addEventListener('popstate', () => p.window.setTimeout(resolve, 0), { once: true });
    act();
  });
  const open = async () => {
    $('knob').focus();
    core.router.open('controls');
    await p.settle();
    await p.settle();
  };
  /** Tap the control matching `selector` (a click with no pointer: a key's) and let the answer land. */
  const tap = async (selector) => {
    const el = q(selector);
    assert.ok(el, `no control ${selector}`);
    el.click();
    await p.settle();
    return el;
  };
  const state = () => core.router.state();
  return { ...p, core, calls, $, q, open, tap, landed, state };
}

const DEVICE = { device: byName.dac.page.shown };

await ok('opening_draws_the_page_the_title_and_the_volume_with_its_figure', async () => {
  const r = rig();
  await r.open();
  assert.deepEqual(r.state(), { device: byName.dac.page.shown, popup: 'controls', page: null });
  assert.equal(r.$('popup-controls-title').textContent, 'Device controls');
  assert.equal(r.$('popup-controls').getAttribute('aria-labelledby'), 'popup-controls-title');
  assert.ok(r.q('.fp-box').classList.contains('fp-single'), 'one pane, no side list');
  assert.equal(r.q('[data-shown="volume"]').textContent, byName.dac.page.device.volume.text);
  assert.ok(r.q('#ctl-side') && r.q('#ctl-say') && r.q('#ctl-body'), 'its places are there');
  assert.equal(r.$('theme').inert, true, 'the page behind is inert');
  assert.deepEqual(r.calls, [], 'drawing writes nothing');
  r.window.close();
});

await ok('each_tap_calls_the_right_action_of_the_core_naming_the_device', async () => {
  const r = rig();
  await r.open();
  const device = byName.dac.page.device;
  const raw = device.volume.raw;
  const want = [];
  const check = async (selector, expected) => {
    await r.tap(selector);
    want.push(expected);
    assert.deepEqual(r.calls, want, selector);
  };
  await check('[data-act="volume-up"]', ['setVolume', raw + 1, DEVICE]);
  await check('[data-act="mute"]', ['setControl', 'mute', { value: true }, DEVICE]);
  await check('[data-act="toggle"][data-key="phase"]', ['setControl', 'phase', { value: false }, DEVICE]);
  await check('[data-act="choice"][data-value="USB 2"]', ['setControl', 'input', { value: 'USB 2' }, DEVICE]);
  await check('[data-act="slider"][data-key="balance"][data-step="1"]',
    ['setControl', 'balance', { raw: 71 }, DEVICE]);
  await check('[data-act="mode"][data-mode="flat"]', ['setMode', 'flat', DEVICE]);
  await check('[data-act="band"][data-band="2"][data-param="g"][data-step="-1"]',
    ['setBand', 2, 'g', { step: -1 }, DEVICE]);
  await check('[data-act="preset"][data-name="Bass"]', ['recallPreset', 'Bass', DEVICE]);
  r.window.close();
});

await ok('the_loudness_strength_the_amps_power_and_the_players_buttons_call_their_actions', async () => {
  const loud = rig({ name: 'dac-loudness' });
  await loud.open();
  await loud.tap('[data-act="loudness"][data-step="1"]');
  await loud.tap('[data-act="loudness"][data-step="-1"]');
  assert.deepEqual(loud.calls, [['setLoudness', { step: 1 }, DEVICE], ['setLoudness', { step: -1 }, DEVICE]]);
  loud.window.close();

  const amp = rig({ name: 'amp' });
  await amp.open();
  const dev = byName.amp.page.device;
  await amp.tap('[data-act="power"]');
  await amp.tap('[data-act="power-toggle"][data-key="power_meters"]');
  const name = dev.name;
  assert.deepEqual(amp.calls, [
    ['setControl', 'power', { value: false }, { device: dev.url }],
    ['setControl', 'power_meters', { value: true }, { device: dev.url }],
  ], name);
  assert.ok(amp.q('[data-readout="warming_up"]'), 'a readout is a row, not a button');
  amp.window.close();

  const player = rig({ name: 'playing' });
  await player.open();
  await player.tap('[data-act="transport"][data-button="next"]');
  await player.tap('[data-act="transport"][data-button="pause"]');
  assert.deepEqual(player.calls, [['press', 'next'], ['press', 'pause']]);
  player.window.close();
});

await ok('the_loki_maxs_slots_settings_and_a_slot_saved_after_its_question', async () => {
  const r = rig({ name: 'lokimax' });
  await r.open();
  const { url } = byName.lokimax.page.device;
  await r.tap('[data-act="slot"][data-slot="2"]');
  await r.tap('[data-act="setting"][data-name="Smiley"]');
  assert.deepEqual(r.calls, [['recallSlot', 2, { device: url }], ['applySetting', 'Smiley', { device: url }]]);
  r.calls.length = 0;
  await r.tap('[data-act="slot-save"][data-slot="3"]');
  assert.equal(r.calls.length, 0, 'a save writes nothing until it is answered');
  const ask = r.q('.fi-askbox');
  assert.ok(ask, 'the question stands in place');
  assert.equal(ask.parentElement.id, IDS.cask);
  assert.ok(ask.textContent.includes('Preset 3'));
  ask.querySelector('[data-answer="keep"]').click();
  await r.settle();
  assert.equal(r.calls.length, 0, 'keep sends nothing');
  assert.equal(r.q('.fi-askbox'), null);
  await r.tap('[data-act="slot-save"][data-slot="3"]');
  r.q('.fi-askbox [data-answer="save"]').click();
  await r.settle();
  assert.deepEqual(r.calls, [['saveSlot', 3, true, { device: url }]]);
  r.window.close();
});

await ok('presets_save_rename_and_delete_ask_in_place_and_name_the_device', async () => {
  const r = rig({ name: 'dac-user-preset' });
  await r.open();
  const { url, presets } = byName['dac-user-preset'].page.device;
  const own = presets.rows.find((row) => !row.built_in).name;
  // save as…: a name field
  await r.tap('[data-act="preset-save"]');
  let form = r.q(`#${IDS.ask} form`);
  assert.ok(form, 'the name form is in the ask place');
  form.querySelector('input').value = 'Night';
  form.requestSubmit?.() ?? form.dispatchEvent(new r.window.Event('submit', { cancelable: true, bubbles: true }));
  await r.settle();
  assert.deepEqual(r.calls, [['savePreset', 'Night', { device: url }]]);
  await r.settle();
  assert.equal(r.q(`#${IDS.ask} form`), null, 'closed on ok');
  // rename the person's own
  r.calls.length = 0;
  await r.tap('[data-act="preset-rename"]');
  form = r.q(`#${IDS.ask} form`);
  assert.equal(form.querySelector('input').value, own, 'the field holds the old name');
  form.querySelector('input').value = 'Day';
  form.requestSubmit?.();
  await r.settle();
  assert.deepEqual(r.calls, [['renamePreset', own, 'Day', { device: url }]]);
  // delete: a question first
  r.calls.length = 0;
  await r.tap('[data-act="preset-delete"]');
  assert.equal(r.calls.length, 0);
  assert.ok(r.q('.fi-askbox').textContent.includes(own));
  r.q('.fi-askbox [data-answer="delete"]').click();
  await r.settle();
  assert.deepEqual(r.calls, [['deletePreset', own, { device: url }]]);
  // built-in presets carry no rename or remove; a device with saving off has no save
  assert.equal(parsed(pageHtml(model('dac'))).querySelector('[data-act="preset-rename"]'), null);
  const off = parsed(pageHtml(model('dac-presets-off')));
  assert.equal(off.querySelector('[data-act="preset-save"]'), null);
  assert.ok(off.textContent.includes('Saving presets is off'));
  r.window.close();
});

await ok('a_key_press_on_a_stepper_steps_once_and_a_pointer_press_steps_once_not_twice', async () => {
  const r = rig();
  await r.open();
  const raw = byName.dac.page.device.volume.raw;
  const up = r.q('[data-act="volume-up"]');
  // a pointer press steps (the core's repeat) and the click after it does not step again
  const press = new r.window.MouseEvent('pointerdown', { bubbles: true, cancelable: true, button: 0 });
  Object.defineProperty(press, 'pointerType', { value: 'mouse' });
  Object.defineProperty(press, 'pointerId', { value: 1 });
  up.dispatchEvent(press);
  up.dispatchEvent(new r.window.MouseEvent('pointerup', { bubbles: true }));
  up.dispatchEvent(new r.window.MouseEvent('click', { bubbles: true, detail: 1 }));
  await r.settle();
  assert.deepEqual(r.calls, [['setVolume', raw + 1, DEVICE]], 'one step for one press');
  // a click with no pointer (a key, a screen reader) steps once
  up.click();
  await r.settle();
  assert.equal(r.calls.length, 2);
  r.window.close();
});

await ok('a_stepper_pressed_again_before_the_model_answers_goes_on_from_the_step_asked', async () => {
  const r = rig();
  await r.open();
  const raw = byName.dac.page.device.volume.raw;
  await r.tap('[data-act="volume-up"]');
  await r.tap('[data-act="volume-up"]');
  await r.tap('[data-act="volume-down"]');
  assert.deepEqual(r.calls.map((c) => c[1]), [raw + 1, raw + 2, raw + 1]);
  // the model has moved: the next step starts from what it says
  const device = structuredClone(byName.dac.page.device);
  device.volume = { ...device.volume, raw: raw + 5 };
  r.core.model.setDevice(device.url, device);
  await r.tap('[data-act="volume-up"]');
  assert.equal(r.calls.at(-1)[1], raw + 6);
  // held to the range: at the top a step up asks for the top
  const top = structuredClone(device);
  top.volume = { ...top.volume, raw: top.volume.hi };
  r.core.model.setDevice(top.url, top);
  await r.tap('[data-act="volume-up"]');
  assert.equal(r.calls.at(-1)[1], top.volume.hi);
  r.window.close();
});

await ok('the_flip_writes_nothing_and_each_computer_tap_calls_the_computers_action', async () => {
  const r = rig({ computer: computerModel() });
  await r.open();
  assert.equal(r.q('[data-act="side"][aria-pressed="true"]').dataset.side, 'dac');
  assert.equal(r.q('[data-act="c-volume-up"]'), null, 'the computer side is not drawn yet');
  await r.tap('[data-act="side"][data-side="computer"]');
  assert.deepEqual(r.calls, [], 'a flip writes nothing');
  assert.equal(r.core.computer.side(), 'computer');
  assert.equal(r.q('[data-act="side"][aria-pressed="true"]').dataset.side, 'computer');
  assert.ok(r.q('[data-act="c-volume-up"]'), 'the computer side is drawn');
  assert.equal(r.q('[data-act="volume-up"]'), null, 'and the device side is not');
  await r.tap('[data-act="c-volume-up"]');
  await r.tap('[data-act="c-volume-down"]');
  await r.tap('[data-act="c-mute"]');
  await r.tap('[data-act="c-mode"][data-mode="off"]');
  await r.tap('[data-act="c-bypass"]');
  await r.tap('[data-act="c-add"]');
  await r.tap('[data-act="c-remove"][data-n="2"]');
  await r.tap('[data-act="c-preset"][data-name="Warm"]');
  assert.deepEqual(r.calls, [
    ['computer.step', 1], ['computer.step', -1], ['computer.toggleMute'],
    ['computer.setMode', 'off'], ['computer.setBypass', true], ['computer.addNext'],
    ['computer.removePoint', 2], ['computer.recallPreset', 'Warm', DEVICE],
  ]);
  // a point's type from its menu, a field's value on Enter, as it was shown
  const select = r.q('select[data-act="c-point"][data-n="2"]');
  select.value = 'HSC';
  select.dispatchEvent(new r.window.Event('change', { bubbles: true }));
  const field = r.q('input[data-act="c-point"][data-n="1"][data-param="f"]');
  const shown = field.value;
  field.focus();
  field.value = '2500';
  r.key('Enter', { on: field });
  await r.settle();
  assert.deepEqual(r.calls.slice(8), [
    ['computer.setPoint', 2, { type: 'HSC' }, { done: true }],
    ['computer.cell', 1, 'f', '2500', shown],
  ]);
  // Escape in a field that holds what was typed puts it back and leaves the popup standing
  const gain = r.q('input[data-act="c-point"][data-n="1"][data-param="g"]');
  gain.focus();
  const was = gain.value;
  gain.value = '9';
  r.key('Escape', { on: gain });
  assert.equal(gain.value, was);
  assert.equal(r.state().popup, 'controls');
  // leaving a field puts back what was typed and not sent, and sends nothing
  const q = r.q('input[data-act="c-point"][data-n="1"][data-param="q"]');
  q.focus();
  const qWas = q.value;
  q.value = '3';
  q.blur();
  assert.equal(q.value, qWas);
  assert.equal(r.calls.length, 10, 'leaving a field sends nothing');
  r.window.close();
});

await ok('the_sound_link_opens_the_sound_popup_for_a_writer', async () => {
  const r = rig({ computer: computerModel() });
  await r.open();
  await r.tap('[data-act="side"][data-side="computer"]');
  await r.tap('[data-act="sound"]');
  await r.settle();
  assert.equal(r.state().popup, 'sound');
  assert.equal(r.calls.length, 0);
  r.window.close();
});

await ok('a_guests_tap_says_the_cores_words_in_place_and_sends_nothing', async () => {
  globalThis.requestAnimationFrame = (fn) => setTimeout(fn, 0);
  const r = rig({ name: 'dac-guest', computer: computerModel() });
  await r.open();
  for (const selector of ['[data-act="volume-up"]', '[data-act="mute"]', '[data-act="mode"]',
    '[data-act="preset"]', '[data-act="band"]']) {
    r.core.actions.refuse = r.core.actions.refuse.bind(r.core.actions);
    await r.tap(selector);
    assert.equal(r.q(`#${IDS.say}`).textContent.trim(), 'Sign in to change this', selector);
  }
  assert.deepEqual(r.calls, [], 'nothing was sent');
  assert.ok([...r.document.querySelectorAll('#toasts .toast')].some((t) => t.textContent.startsWith('Sign in to change this')));
  // the flip is free to a guest (it writes nothing), and the computer's controls are inert too
  await r.tap('[data-act="side"][data-side="computer"]');
  assert.equal(r.core.computer.side(), 'computer');
  await r.tap('[data-act="c-volume-up"]');
  await r.tap('[data-act="c-add"]');
  assert.deepEqual(r.calls, []);
  assert.equal(r.q(`#${IDS.say}`).textContent.trim(), 'Sign in to change this');
  // a page with writes off says its own words
  const off = rig({ name: 'dac-readonly' });
  await off.open();
  await off.tap('[data-act="volume-up"]');
  assert.ok(off.q(`#${IDS.say}`).textContent.trim().length > 0, 'the read-only words');
  assert.deepEqual(off.calls, []);
  r.window.close();
  off.window.close();
});

await ok('a_refused_write_shows_the_daemons_words_and_the_next_landed_write_clears_them', async () => {
  let words = 'The device did not answer.';
  const r = rig({
    answer: (name) => (name === 'setMode' ? { ok: false, status: 409, error: words } : { ok: true, status: 200 }),
  });
  await r.open();
  await r.tap('[data-act="mode"][data-mode="flat"]');
  await r.settle();
  const say = r.q(`#${IDS.say}`);
  assert.equal(say.textContent.trim(), words);
  assert.equal(say.querySelector('p').getAttribute('role'), 'alert');
  // a refusal with no words (a raise dropped while the page was not live) says nothing new
  words = '';
  await r.tap('[data-act="mode"][data-mode="eq"]');
  await r.settle();
  assert.equal(r.q(`#${IDS.say}`).textContent.trim(), 'The device did not answer.', 'words stay until a write lands');
  await r.tap('[data-act="preset"][data-name="Bass"]');
  await r.settle();
  assert.equal(r.q(`#${IDS.say}`).textContent.trim(), '', 'a landed write clears them');
  r.window.close();
});

await ok('escape_and_back_close_it_and_a_change_of_the_shown_device_closes_it', async () => {
  const r = rig();
  await r.open();
  await r.landed(() => r.key('Escape'));
  assert.equal(r.state().popup, null);
  assert.equal(r.$('popup-controls'), null);
  await r.open();
  await r.landed(() => r.window.history.back());
  assert.equal(r.state().popup, null);
  // another device on show closes it (it is about the one on show)
  await r.open();
  assert.equal(r.state().popup, 'controls');
  const other = pageOf('amp');
  await r.landed(() => r.core.model.replace(other));
  assert.equal(r.state().popup, null, 'no stays: a change of the shown device closes it');
  // and no write went out meanwhile
  assert.deepEqual(r.calls, []);
  r.window.close();
});

await ok('a_change_of_the_model_waits_for_an_ask_and_is_drawn_when_it_closes', async () => {
  const r = rig({ name: 'dac-user-preset' });
  await r.open();
  await r.tap('[data-act="preset-save"]');
  assert.equal(openAsks() > 0, true);
  const device = structuredClone(byName['dac-user-preset'].page.device);
  device.volume = { ...device.volume, text: '−12.3 dB' };
  r.core.model.setDevice(device.url, device);
  assert.ok(r.q('.fi-askbox'), 'the ask stands through a change of the model');
  assert.notEqual(r.q('[data-shown="volume"]').textContent, '−12.3 dB', 'the page waits');
  r.q('.fi-askbox [data-answer="cancel"]').click();
  await r.settle();
  assert.equal(r.q('[data-shown="volume"]').textContent, '−12.3 dB', 'drawn once the ask is closed');
  // without an ask the model is drawn at once, and only the section that changed is put
  const volumeBox = r.q('#ctl-volume');
  const presets = r.q('#ctl-presets');
  device.volume = { ...device.volume, text: '−12.0 dB' };
  r.core.model.setDevice(device.url, device);
  assert.equal(r.q('[data-shown="volume"]').textContent, '−12.0 dB');
  assert.equal(r.q('#ctl-presets'), presets, 'a section that did not change is not drawn again');
  assert.equal(r.q('#ctl-volume'), volumeBox, 'the place stays, its words are put');
  r.window.close();
});

await ok('a_mode_that_brings_a_strength_draws_the_whole_page_and_keeps_the_focus_by_id', async () => {
  const r = rig();
  await r.open();
  assert.equal(r.q('#ctl-loudness'), null);
  r.q('#ctl-mode-eq').focus();
  const device = structuredClone(byName['dac-loudness'].page.device);
  device.url = byName.dac.page.device.url;
  r.core.model.setDevice(device.url, device);
  assert.ok(r.q('#ctl-loudness'), 'the strength section appears');
  assert.equal(r.document.activeElement.id, 'ctl-mode-eq', 'the focus stayed on its control');
  r.window.close();
});

// ---- the core's driver ----

/** A window that lays things out: every element has a box inside the window. */
function laidOut(r) {
  r.window.Element.prototype.getBoundingClientRect = () => ({
    width: 40, height: 20, top: 10, bottom: 30, left: 0, right: 40,
  });
}

await ok('the_driver_reaches_operates_reads_and_states_the_devices_targets', async () => {
  const r = rig();
  laidOut(r);
  const c = r.core.driver.controls;
  const raw = byName.dac.page.device.volume.raw;
  assert.equal(await c.reach({ control: 'volume' }), true);
  assert.equal(r.state().popup, 'controls', 'it opened the popup through the router');
  assert.equal(c.shown({ control: 'volume' }), byName.dac.page.device.volume.text);
  assert.equal(await c.operate({ control: 'volume' }, { step: 1 }), true);
  assert.deepEqual(r.calls, [['setVolume', raw + 1, DEVICE]]);
  assert.equal(await c.operate({ control: 'volume' }, { step: -1 }), true);
  assert.deepEqual(r.calls.at(-1), ['setVolume', raw, DEVICE], 'a step down is a step down');
  assert.equal(await c.operate({ control: 'mute' }, { on: true }), true);
  assert.deepEqual(r.calls.at(-1), ['setControl', 'mute', { value: true }, DEVICE]);
  assert.equal(await c.operate({ control: 'input' }, { value: 'USB 2' }), true);
  assert.deepEqual(r.calls.at(-1), ['setControl', 'input', { value: 'USB 2' }, DEVICE]);
  assert.equal(await c.operate({ band: 3, param: 'f' }, { step: 1 }), true);
  assert.deepEqual(r.calls.at(-1), ['setBand', 3, 'f', { step: 1 }, DEVICE]);
  assert.equal(await c.operate({ preset: 'Warm' }, { apply: true }), true);
  assert.deepEqual(r.calls.at(-1), ['recallPreset', 'Warm', DEVICE]);
  assert.equal(c.shown({ band: 2, param: 'g' }), byName.dac.page.device.tone.bands.rows[1].g);
  assert.equal(c.shown({ mode: 'eq' }), 'EQ');
  assert.equal(c.shown({ control: 'phase' }), 'on');
  assert.deepEqual(Object.keys(c.state({ control: 'phase' })).sort(), ['inert', 'pressed', 'said', 'toasts']);
  assert.equal(c.state({ control: 'phase' }).pressed, true);
  assert.equal(c.state({ control: 'phase' }).inert, false);
  // refused without a throw: a target the page does not list, a device the page does not hold
  assert.equal(await c.reach({ control: 'nope' }), false);
  assert.equal(await c.operate({ band: 9, param: 'f' }, { step: 1 }), false);
  assert.equal(c.shown({ control: 'nope' }), null);
  assert.equal(c.state({ control: 'nope' }), null);
  assert.equal(await c.reach(null), false);
  r.window.close();
});

await ok('the_driver_flips_to_the_computer_and_back_and_reaches_its_targets', async () => {
  const r = rig({ computer: computerModel() });
  laidOut(r);
  const c = r.core.driver.controls;
  assert.equal(await c.reach({ computer: 'flip' }), true);
  assert.equal(c.shown({ computer: 'flip' }), byName.dac.page.device.name);
  assert.equal(await c.reach({ computer: 'volume' }), true, 'reaching a computer target flips');
  assert.equal(r.core.computer.side(), 'computer');
  assert.deepEqual(r.calls, [], 'the flip wrote nothing');
  assert.equal(await c.operate({ computer: 'volume' }, { step: 1 }), true);
  assert.deepEqual(r.calls.at(-1), ['computer.step', 1]);
  assert.equal(await c.operate({ computer: 'point', n: 2, param: 'type' }, { value: 'HSC' }), true);
  assert.deepEqual(r.calls.at(-1), ['computer.setPoint', 2, { type: 'HSC' }, { done: true }]);
  assert.equal(await c.operate({ computer: 'point', n: 1, param: 'g' }, { value: '4' }), true);
  assert.deepEqual(r.calls.at(-1).slice(0, 4), ['computer.cell', 1, 'g', '4']);
  assert.equal(await c.operate({ computer: 'flip' }, { side: 'dac' }), true);
  assert.equal(r.core.computer.side(), 'dac');
  assert.equal(await c.reach({ control: 'volume' }), true, 'a device target flips back');
  assert.equal(await c.reach({ computer: 'preset', name: 'Huge' }), false, 'a preset the computer cannot play is no target');
  r.window.close();
});

await ok('the_driver_refuses_with_no_device_and_for_a_viewer_who_may_not_have_the_popup', async () => {
  const none = rig({ name: 'no-devices' });
  laidOut(none);
  assert.equal(await none.core.driver.controls.reach({ control: 'volume' }), false);
  assert.equal(await none.core.driver.controls.operate({ control: 'volume' }, { step: 1 }), false);
  assert.equal(none.core.driver.controls.shown({ control: 'volume' }), null);
  assert.equal(none.state().popup, null, 'no popup opened for nothing');
  none.window.close();
  // the guest may have the popup (everyone's), and its operate says the core's words
  const g = rig({ name: 'dac-guest' });
  laidOut(g);
  assert.equal(await g.core.driver.controls.operate({ control: 'volume' }, { step: 1 }), true);
  assert.deepEqual(g.calls, []);
  assert.equal(g.core.driver.controls.state({ control: 'volume' }).said, 'Sign in to change this');
  assert.equal(g.core.driver.controls.state({ control: 'volume' }).inert, true);
  g.window.close();
});

console.log(`core-popups-controls.mjs: ${n} passed`);
