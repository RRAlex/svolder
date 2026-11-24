/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node check of the Home Assistant strategy (contrib/home-assistant/svolder/): one view per
// computer from fixtures/*.json, which hold `hass.devices`, `hass.entities` and `hass.states` in the
// frontend's shapes. Run: node strategy.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { buildDashboard } from '../../../contrib/home-assistant/svolder/strategy.js';

// Each fixture is named by its path, so the touched-file map sees which check reads it.
const FIXTURES = {
  'older-single': './fixtures/older-single.json',
  'stray-entity': './fixtures/stray-entity.json',
  'no-scenes': './fixtures/no-scenes.json',
  'renamed-ids': './fixtures/renamed-ids.json',
  // Made by the daemon's own discovery code (bins/svolderd, mqtt_bridge/tests/ha_fixtures.rs); the hand
  // fixtures above stay for what the daemon cannot announce: a stray or hidden entity, a renamed entity id,
  // an older daemon's single computer, a computer with no scenes, a disabled device.
  'generated-on': './fixtures/generated/commands-on.json',
  'generated-off': './fixtures/generated/commands-off.json',
};
const load = (name) => {
  const hass = JSON.parse(readFileSync(fileURLToPath(new URL(FIXTURES[name], import.meta.url)), 'utf8')).hass;
  const calls = [];
  for (const fn of ['callService', 'callWS', 'callApi', 'connection']) {
    hass[fn] = (...args) => { calls.push([fn, ...args]); throw new Error(`${fn} called`); };
  }
  return { hass, calls };
};

/** The entity ids in a svolder card's `entities` (ids as strings, bands as lists of them). */
function cardEntityIds(node, out) {
  if (typeof node === 'string') { if (/^[a-z_]+\.[a-z0-9_]+$/.test(node)) out.add(node); }
  else if (Array.isArray(node)) node.forEach((n) => cardEntityIds(n, out));
  else if (node && typeof node === 'object') Object.values(node).forEach((n) => cardEntityIds(n, out));
}

/** Every entity id a config names, anywhere. */
function entityIds(node, out = new Set()) {
  if (Array.isArray(node)) node.forEach((n) => entityIds(n, out));
  else if (node && typeof node === 'object') {
    for (const [k, v] of Object.entries(node)) {
      if (k === 'entity' && typeof v === 'string') out.add(v);
      else if (k === 'entities' && node.type && node.type.startsWith('custom:svolder-')) cardEntityIds(v, out);
      else entityIds(v, out);
    }
  }
  return out;
}
const cardsOf = (node, type, out = []) => {
  if (Array.isArray(node)) node.forEach((n) => cardsOf(n, type, out));
  else if (node && typeof node === 'object') {
    if (node.type === type) out.push(node);
    Object.values(node).forEach((v) => cardsOf(v, type, out));
  }
  return out;
};
const headings = (view) => cardsOf(view, 'heading').map((h) => h.heading);

const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('a: two computers, a view each, each device under its own computer', () => {
  const { hass } = load('generated-on');
  const dash = buildDashboard(hass);
  assert.deepEqual(dash.views.map((v) => v.title), ['Living-Room', 'reframe']);
  const [living, reframe] = dash.views;
  for (const name of ['Gungnir', 'Lyr 5']) assert.ok(headings(living).includes(name) && !headings(reframe).includes(name), name);
  assert.ok(headings(reframe).includes('Bifrost 3') && !headings(living).includes('Bifrost 3'));
  assert.deepEqual([...new Set(dash.views.map((v) => v.path))].length, 2);
  for (const view of dash.views) assert.ok(view.sections.length >= 3);
});

test('a: the scene pair and the loudness tiles are on the computer section', () => {
  const { hass } = load('generated-on');
  const view = buildDashboard(hass).views.find((v) => v.title === 'reframe');
  const ids = entityIds(view);
  assert.ok(ids.has('select.reframe_scene') && ids.has('text.reframe_scene_save'));
  assert.ok(ids.has('switch.reframe_loudness') && ids.has('number.reframe_loudness_strength'));
  assert.ok(ids.has('number.bifrost_3_loudness_strength'));
});

test('a2: the bypass switch and the three level sensors are on the computer section', () => {
  const { hass } = load('generated-on');
  const view = buildDashboard(hass).views.find((v) => v.title === 'reframe');
  const ids = entityIds(view);
  for (const id of ['switch.reframe_bypass', 'sensor.reframe_chain_level', 'sensor.reframe_eq_level', 'sensor.reframe_space_level']) {
    assert.ok(ids.has(id), id);
  }
  const off = entityIds(buildDashboard(load('generated-off').hass).views.find((v) => v.title === 'reframe'));
  assert.ok(off.has('binary_sensor.reframe_bypass'));
});

test('b: the older single "This computer" is one view', () => {
  const { hass } = load('older-single');
  const dash = buildDashboard(hass);
  assert.equal(dash.views.length, 1);
  assert.equal(dash.views[0].title, 'This computer');
  assert.ok(headings(dash.views[0]).includes('Bifrost 3'));
});

test('c: an entity no role takes is in the entities card', () => {
  const { hass } = load('stray-entity');
  const view = buildDashboard(hass).views[0];
  const cards = cardsOf(view, 'entities').filter((c) => c.entities.some((e) => e.entity === 'sensor.bifrost_3_gadget_reading'));
  assert.equal(cards.length, 1);
});

test('c: every entity of every fixture is shown somewhere', () => {
  for (const f of Object.keys(FIXTURES)) {
    const { hass } = load(f);
    const shown = entityIds(buildDashboard(hass));
    for (const [id, e] of Object.entries(hass.entities)) {
      assert.equal(shown.has(id), !e.hidden, `${f}: ${id} ${e.hidden ? 'is shown but hidden' : 'is missing'}`);
    }
  }
});

test('c: diagnostics come last in their device section', () => {
  const { hass } = load('generated-on');
  const sections = buildDashboard(hass).views[1].sections;
  const last = sections[sections.length - 1];
  assert.ok(last.cards.some((c) => c.heading && /diagnostics/.test(c.heading)));
  assert.ok(entityIds(last).has('sensor.bifrost_3_firmware'));
});

test('d: no scene entities, no scene card, no error', () => {
  const { hass } = load('no-scenes');
  const view = buildDashboard(hass).views[0];
  assert.ok(!headings(view).includes('Scenes'));
  assert.ok(![...entityIds(view)].some((id) => /scene/.test(id)));
});

test('e: renamed entity ids: roles are found by the registry name, nothing is lost', () => {
  const { hass } = load('renamed-ids');
  const view = buildDashboard(hass).views[0];
  const ids = entityIds(view);
  for (const id of Object.keys(hass.entities)) assert.ok(ids.has(id), id);
    assert.ok(headings(view).includes('Scenes'));
  assert.ok(headings(view).includes('Bifrost 3: EQ'));
  assert.equal(cardsOf(view, 'custom:svolder-eq-card').length, 1);
  assert.equal(cardsOf(view, 'custom:svolder-scenes-card').length, 1);
});

test('f: generate writes nothing', () => {
  for (const f of Object.keys(FIXTURES)) {
    const { hass, calls } = load(f);
    buildDashboard(hass, {});
    assert.deepEqual(calls, []);
  }
  const src = ['svolder.js', 'svolder/find.js', 'svolder/layout.js', 'svolder/strategy.js']
    .map((p) => readFileSync(fileURLToPath(new URL(`../../../contrib/home-assistant/${p}`, import.meta.url)), 'utf8'))
    .join('\n');
  assert.ok(!/callService|callWS|callApi|fetch\(/.test(src));
});

test('a: balance is on the faceplate and the tone control mode on the EQ card, neither in "other"', () => {
  const dash = buildDashboard(load('generated-on').hass);
  const view = dash.views.find((v) => v.title === 'reframe');
  const face = cardsOf(view, 'custom:svolder-faceplate-card').find((c) => c.name === 'Bifrost 3');
  const eq = cardsOf(view, 'custom:svolder-eq-card').find((c) => c.entities.mode === 'select.bifrost_3_tone_control_mode');
  assert.equal(face.entities.balance, 'number.bifrost_3_balance');
  assert.ok(eq, 'the EQ card is handed the mode select');
  const other = view.sections.find((s) => s.cards.some((c) => c.heading === 'Bifrost 3: other'));
  assert.ok(other, 'the other list exists');
  const listed = entityIds(other.cards.filter((c) => c.type === 'entities'));
  assert.ok(!listed.has('number.bifrost_3_balance') && !listed.has('select.bifrost_3_tone_control_mode'), [...listed].join());
});

const SEQ = ['Bifrost 3', 'Scenes', 'Bifrost 3: diagnostics'];
const shape = (view) => view.sections.map((s) => s.cards.filter((c) => c.type === 'heading')[0].heading);

test('a: the approved layout order: device faceplate and EQ, scenes then chain, other under the chain, diagnostics last', () => {
  const view = buildDashboard(load('generated-on').hass).views[1];
  const names = (section) => section.cards.map((c) => c.heading || c.type.replace('custom:svolder-', ''));
  assert.deepEqual(names(view.sections[0]), ['Bifrost 3', 'faceplate-card', 'Bifrost 3: EQ', 'eq-card']);
  assert.deepEqual(names(view.sections[1]), ['Scenes', 'scenes-card', 'reframe', 'chain-card', 'Bifrost 3: other', 'entities']);
  assert.deepEqual(view.sections.slice(2).map((s) => s.cards[0].heading), ['Bifrost 3: diagnostics']);
});

test('a: view paths and section order are pinned', () => {
  const dash = buildDashboard(load('generated-on').hass);
  assert.deepEqual(dash.views.map((v) => v.path), ['living_room', 'reframe']);
  assert.deepEqual(shape(dash.views[1]), SEQ);
  assert.deepEqual(shape(dash.views[0]).slice(0, 3), ['Gungnir', 'Lyr 5', 'Scenes']);
});

test('F1: commands off (sensors only): the same sections, no write controls', () => {
  const { hass } = load('generated-off');
  const dash = buildDashboard(hass);
  assert.deepEqual(shape(dash.views[1]), SEQ);
  const shown = entityIds(dash);
  for (const id of Object.keys(hass.entities)) assert.ok(shown.has(id) || hass.entities[id].hidden, id);
  for (const name of ['faceplate', 'eq', 'chain', 'scenes']) {
    assert.ok(cardsOf(dash.views[1], `custom:svolder-${name}-card`).length >= 1, name);
  }
  for (const card of cardsOf(dash, 'tile')) {
    assert.ok(!card.features && !card.tap_action, `${card.entity} carries a control it cannot write`);
  }
  const scenes = cardsOf(dash.views[1], 'custom:svolder-scenes-card')[0];
  assert.equal(scenes.entities.scene, 'sensor.reframe_scene');
  assert.equal(scenes.entities.save, undefined, 'no text entity when commands are off');
});

test('F1: commands on: the cards are handed the writable entities', () => {
  const dash = buildDashboard(load('generated-on').hass);
  const face = cardsOf(dash.views[1], 'custom:svolder-faceplate-card')[0];
  assert.equal(face.entities.volume, 'number.bifrost_3_volume');
  assert.equal(face.entities.mute, 'switch.bifrost_3_mute');
  assert.equal(face.entities.input, 'select.bifrost_3_input');
  const scenes = cardsOf(dash.views[1], 'custom:svolder-scenes-card')[0];
  assert.deepEqual(scenes.entities, { scene: 'select.reframe_scene', save: 'text.reframe_scene_save' });
  const eq = cardsOf(dash.views[1], 'custom:svolder-eq-card')[0];
  assert.equal(eq.entities.bands.length, 5);
  assert.deepEqual(eq.entities.bands[0], {
    n: 1, gain: 'number.bifrost_3_band_1_gain', frequency: 'number.bifrost_3_band_1_frequency', q: 'number.bifrost_3_band_1_q',
  });
});

test('F2: a hidden entity is left out, a disabled device has no section', () => {
  const dash = buildDashboard(load('stray-entity').hass);
  assert.ok(!entityIds(dash).has('sensor.bifrost_3_hidden_reading'));
  assert.ok(!headings(dash.views[0]).includes('Retired DAC'));
});

test('F5: the path is shown once on the computer section', () => {
  const dash = buildDashboard(load('generated-on').hass);
  const hits = JSON.stringify(dash.views[1]).split('sensor.reframe_path').length - 1;
  assert.equal(hits, 1);
});

test('F6: a near-miss entity is not taken for a role', () => {
  const hass = load('generated-on').hass;
  delete hass.entities['number.bifrost_3_volume'];
  hass.entities['number.bifrost_3_max_volume'] = { entity_id: 'number.bifrost_3_max_volume', name: 'Max volume', device_id: 'd1' };
  const dash = buildDashboard(hass);
  const tiles = cardsOf(dash.views[1], 'tile').filter((t) => t.entity === 'number.bifrost_3_max_volume');
  assert.equal(tiles.length, 0);
  assert.ok(entityIds(dash.views[1]).has('number.bifrost_3_max_volume'));
});

test('F8: every card of a view names only that computer\'s own entities and its devices\'', () => {
  const { hass } = load('generated-on');
  const dash = buildDashboard(hass);
  for (const view of dash.views) {
    const computer = Object.values(hass.devices).find((d) => d.name === view.title);
    const own = new Set(Object.keys(hass.entities).filter((id) => {
      const dev = hass.devices[hass.entities[id].device_id];
      return dev && (dev.id === computer.id || dev.via_device_id === computer.id);
    }));
    const named = [];
    for (const card of cardsOf(view, 'custom:svolder-chain-card')
      .concat(cardsOf(view, 'custom:svolder-scenes-card'), cardsOf(view, 'custom:svolder-faceplate-card'), cardsOf(view, 'custom:svolder-eq-card'))) {
      const ids = new Set();
      cardEntityIds(card.entities, ids);
      named.push(...ids);
    }
    assert.ok(named.length > 20, `${view.title} has its cards`);
    for (const id of named) assert.ok(own.has(id), `${view.title}: ${id} belongs to another computer`);
  }
});

test('theme svolder is set only when installed', () => {
  const { hass } = load('generated-on');
  assert.equal(buildDashboard(hass).views[0].theme, undefined);
  hass.themes = { themes: { svolder: {} } };
  assert.equal(buildDashboard(hass).views[0].theme, 'svolder');
});

test('a device with no computer goes to the only computer, else to "Devices"', () => {
  const { hass } = load('generated-on');
  hass.devices.d1.via_device_id = null;
  const dash = buildDashboard(hass);
  assert.deepEqual(dash.views.map((v) => v.title), ['Living-Room', 'reframe', 'Devices']);
  assert.ok(headings(dash.views[2]).includes('Bifrost 3'));
  const one = load('older-single').hass;
  assert.equal(buildDashboard(one).views.length, 1);
});

// ---- the fixtures the daemon's discovery code made ----
const GENERATED = ['generated-on', 'generated-off'];

/** The ids of every "<device>: other" list of `view` (the entities card after that heading). */
function otherIds(view) {
  const out = new Set();
  for (const sec of view.sections) {
    sec.cards.forEach((c, i) => {
      if (c.type === 'entities' && /: other$/.test((sec.cards[i - 1] || {}).heading || '')) c.entities.forEach((e) => out.add(e.entity));
    });
  }
  return out;
}

/** Fails when an entity a svolder card was handed is also in an "other" list; the strength is shown by
 *  its card and kept in the list on purpose (layout.js `peek`), as is the firmware. */
function assertNoneOwnedInOther(view, label) {
  const handed = new Set();
  for (const type of ['faceplate', 'eq', 'chain', 'scenes']) {
    for (const card of cardsOf(view, `custom:svolder-${type}-card`)) {
      const { strength, firmware, ...own } = card.entities;
      cardEntityIds(own, handed);
    }
  }
  const left = [...otherIds(view)].filter((id) => handed.has(id));
  assert.deepEqual(left, [], `${label}: ${view.title} lists what a card owns`);
}

test('generated: the registry holds the identifiers the daemon publishes', () => {
  for (const f of GENERATED) {
    const ids = Object.values(load(f).hass.devices).map((d) => d.identifiers[0][1]);
    for (const id of ['svolder_computer_reframe', 'svolder_computer_living_room', 'svolder_0A1B2C3D4E5F6071', 'svolder_2C3D4E5F6071A0B1']) {
      assert.ok(ids.includes(id), `${f}: ${id}`);
    }
  }
});

test('generated: the approved order, device faceplate and EQ, then scenes, chain and other, then diagnostics', () => {
  for (const f of GENERATED) {
    const [living, reframe] = buildDashboard(load(f).hass).views;
    const names = (section) => section.cards.map((c) => c.heading || c.type.replace('custom:svolder-', ''));
    assert.deepEqual(names(reframe.sections[0]), ['Bifrost 3', 'faceplate-card', 'Bifrost 3: EQ', 'eq-card'], f);
    assert.deepEqual(names(reframe.sections[1]), ['Scenes', 'scenes-card', 'reframe', 'chain-card', 'Bifrost 3: other', 'entities'], f);
    assert.deepEqual(reframe.sections.slice(2).map((s) => s.cards[0].heading), ['Bifrost 3: diagnostics'], f);
    assert.deepEqual(names(living.sections[0]), ['Gungnir', 'faceplate-card', 'Gungnir: EQ', 'eq-card'], f);
    assert.deepEqual(names(living.sections[1]), ['Lyr 5', 'faceplate-card'], f);
    assert.deepEqual(names(living.sections[2]).slice(0, 4), ['Scenes', 'scenes-card', 'Living-Room', 'chain-card'], f);
    assert.deepEqual(living.sections.slice(3).map((s) => s.cards[0].heading), ['Gungnir: diagnostics'], f);
  }
});

test('generated: the tone mode chips and the balance are on their cards, and a card-owned entity is not in "other"', () => {
  for (const f of GENERATED) {
    const { hass } = load(f);
    const view = buildDashboard(hass).views.find((v) => v.title === 'reframe');
    const mode = Object.keys(hass.entities).find((id) => id.endsWith('bifrost_3_tone_control_mode'));
    const balance = Object.keys(hass.entities).find((id) => id.endsWith('bifrost_3_balance'));
    assert.equal(cardsOf(view, 'custom:svolder-eq-card')[0].entities.mode, mode, f);
    assert.equal(cardsOf(view, 'custom:svolder-faceplate-card')[0].entities.balance, balance, f);
    assert.equal(Array.isArray(hass.states[mode].attributes.options), f === 'generated-on', f);
    for (const view of buildDashboard(hass).views) assertNoneOwnedInOther(view, f);
  }
});

test('generated: one USB input is a readout on the faceplate, two are two, none is left in "other"', () => {
  for (const f of GENERATED) {
    const [living, reframe] = buildDashboard(load(f).hass).views;
    const face = (view, name) => cardsOf(view, 'custom:svolder-faceplate-card').find((c) => c.name === name).entities;
    assert.equal(face(reframe, 'Bifrost 3').usb, 'binary_sensor.bifrost_3_usb_connected', f);
    assert.equal(face(reframe, 'Bifrost 3').usb1, undefined, f);
    assert.deepEqual([face(living, 'Gungnir').usb1, face(living, 'Gungnir').usb2],
      ['binary_sensor.gungnir_usb_1_connected', 'binary_sensor.gungnir_usb_2_connected'], f);
    for (const view of [living, reframe]) assert.ok(![...otherIds(view)].some((id) => /usb/.test(id)), `${f}: USB left in other`);
  }
});

test('generated: the cards are handed the writable entities with commands on, sensors with them off', () => {
  const on = buildDashboard(load('generated-on').hass).views[1];
  const off = buildDashboard(load('generated-off').hass).views[1];
  assert.equal(cardsOf(on, 'custom:svolder-faceplate-card')[0].entities.volume, 'number.bifrost_3_volume');
  assert.equal(cardsOf(off, 'custom:svolder-faceplate-card')[0].entities.volume, 'sensor.bifrost_3_volume');
  assert.deepEqual(cardsOf(on, 'custom:svolder-scenes-card')[0].entities, { scene: 'select.reframe_scene', save: 'text.reframe_scene_save' });
  assert.deepEqual(cardsOf(off, 'custom:svolder-scenes-card')[0].entities, { scene: 'sensor.reframe_scene' });
  assert.equal(cardsOf(on, 'custom:svolder-eq-card')[0].entities.bands.length, 5);
});

let failed = 0;
for (const [name, fn] of cases) {
  try { fn(); console.log(`ok   ${name}`); } catch (e) { failed += 1; console.log(`FAIL ${name}\n     ${e.message}`); }
}
if (failed) process.exit(1);
console.log(`${cases.length} cases passed`);
