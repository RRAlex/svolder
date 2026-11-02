/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/contract.js, what every theme must present: the targets (controls and
// actions) and the facts a device's model holds, read from the fixture's pages (every device kind
// the page draws, a guest's included). Every control of the model is of a kind the core knows,
// every target maps to one action request with today's path and body, and a followed device's
// targets reach that device by its name. The post is a fake that answers 200. Run: node contract.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import * as contract from '../../../bins/svolderd/web/core/contract.js';

const { factsOf, targetsOf } = contract;

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const cases = fixture.cases.filter((c) => c.page.device);
const byName = Object.fromEntries(fixture.cases.map((c) => [c.name, c]));

/** The kinds of control the core knows: what a theme draws by kind, not by device. */
const KINDS = ['choice', 'toggle', 'slider', 'readout'];
/** The members of each object of a device the core knows. */
const POWER = ['json', 'key', 'on', 'readouts', 'toggles', 'warming', 'word'];

/** The page of `device`, which the actions work on, with `followed` beside it. */
const pageOf = (device, viewer, followed = {}) => ({
  v: 1, viewer, fleet: { devices: [], hops: [] }, shown: device.url, device, followed,
  card: null, host: null, strip: null,
});

/** Everything the actions touch, faked: the posts they make, the toasts they say. */
function rig(page) {
  const posts = [];
  const toasts = [];
  const store = createStore(page, {
    signedOut: () => {},
    fetch: async () => ({ status: 200, json: async () => ({}) }),
  });
  const actions = createActions({
    store,
    post: async (path, body) => { posts.push([path, body]); return { status: 200 }; },
    toast: (text) => toasts.push(text),
    history: { pushState: () => {} },
    location: { assign: () => {} },
  });
  return { actions, posts, toasts };
}

/** The request a target should make, as today's page makes it, and the call that makes it:
 * `[path, body]` under the device (`null` when a target is only read, `[null, body]` for a
 * request that names no device), and `run(actions, opts)`. `op` picks what a slot does. */
function planOf(device, target, op = 'recall') {
  const { volume, power, tone } = device;
  const toggled = (control) => ({ value: !control.on });
  if ('control' in target) {
    const key = target.control;
    if (volume && key === volume.key) {
      const raw = volume.lo;
      return {
        want: ['volume', { raw }], run: (a, o) => a.setVolume(raw, o),
      };
    }
    if (volume && key === volume.mute) {
      const body = { value: !volume.muted };
      return { want: [`controls/${key}`, body], run: (a, o) => a.setControl(key, body, o) };
    }
    const control = (device.controls ?? []).find((c) => c.key === key)
      ?? (power && key === power.key ? { kind: 'power', ...power } : null)
      ?? [...(power?.toggles ?? []), ...(power?.readouts ?? [])].find((c) => c.key === key);
    assert.ok(control, `no control ${key}`);
    let body;
    switch (control.kind) {
      case 'readout': return { want: null, run: async () => ({ ok: true }) };
      case 'toggle': case 'power': body = toggled(control); break;
      case 'choice': body = { value: control.options[0].label }; break;
      case 'slider': body = { raw: control.lo }; break;
      default: assert.fail(`control ${key} is of kind ${control.kind}`);
    }
    return { want: [`controls/${key}`, body], run: (a, o) => a.setControl(key, body, o) };
  }
  if ('band' in target) {
    const body = { param: target.param, step: 1 };
    return {
      want: [`eq/bands/${target.band}`, body],
      run: (a, o) => a.setBand(target.band, target.param, { step: 1 }, o),
    };
  }
  if ('mode' in target) {
    return { want: ['eq/mode', { mode: target.mode }], run: (a, o) => a.setMode(target.mode, o) };
  }
  if ('loudness' in target) {
    return { want: ['eq/loudness', { step: 1 }], run: (a, o) => a.setLoudness({ step: 1 }, o) };
  }
  if ('preset' in target) {
    return {
      want: ['presets/recall', { name: target.preset }],
      run: (a, o) => a.recallPreset(target.preset, o),
    };
  }
  if ('slot' in target) {
    if (op === 'save') {
      return {
        want: ['device-presets/save', { slot: target.slot, confirm: true }],
        run: (a, o) => a.saveSlot(target.slot, true, o),
      };
    }
    return {
      want: ['device-presets/recall', { slot: target.slot }],
      run: (a, o) => a.recallSlot(target.slot, o),
    };
  }
  if ('setting' in target) {
    return {
      want: ['device-presets/apply', { name: target.setting }],
      run: (a, o) => a.applySetting(target.setting, o),
    };
  }
  if ('transport' in target) {
    return {
      want: [null, { button: target.transport }], run: (a) => a.press(target.transport),
    };
  }
  assert.fail(`a target the test does not know: ${JSON.stringify(target)}`);
  return null;
}

/** The viewer of a page that may write. */
const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };

await ok('every_control_has_a_kind_the_core_knows', async () => {
  let controls = 0;
  for (const { name, page } of cases) {
    const { device } = page;
    for (const control of [...device.controls, ...(device.power?.toggles ?? []),
      ...(device.power?.readouts ?? [])]) {
      controls++;
      assert.ok(KINDS.includes(control.kind), `${name}: ${control.key} is of kind ${control.kind}`);
      assert.equal(typeof control.key, 'string', name);
      assert.equal(typeof control.label, 'string', `${name}: ${control.key}`);
      if (control.kind === 'choice') {
        assert.ok(control.options.length > 0, `${name}: ${control.key} has no options`);
      }
    }
    if (device.volume) {
      for (const member of ['key', 'lo', 'hi', 'raw', 'text', 'muted', 'mute']) {
        assert.ok(member in device.volume, `${name}: the volume has no ${member}`);
      }
    }
    if (device.power) {
      assert.deepEqual(Object.keys(device.power).sort(), POWER, `${name}: the power object`);
    }
    if (device.tone) {
      for (const member of ['modes', 'loud', 'bands', 'readings', 'strength']) {
        assert.ok(member in device.tone, `${name}: the tone has no ${member}`);
      }
    }
    if (device.presets) {
      for (const member of ['rows', 'recall', 'editable', 'status']) {
        assert.ok(member in device.presets, `${name}: the presets have no ${member}`);
      }
    }
    if (device.device_presets) {
      for (const member of ['slots', 'builtins', 'recall', 'save']) {
        assert.ok(member in device.device_presets, `${name}: the device presets have no ${member}`);
      }
    }
  }
  assert.ok(controls > 20, 'the fixture holds controls of every kind');
  const kinds = new Set(cases.flatMap(({ page }) => page.device.controls.map((c) => c.kind)));
  assert.deepEqual([...kinds].sort(), [...KINDS].sort(), 'every kind is in the fixture');
});

await ok('targets_name_every_control_and_action_the_model_offers', async () => {
  const dac = targetsOf(byName.dac.page.device);
  assert.deepEqual(dac.slice(0, 2), [{ control: 'volume' }, { control: 'mute' }]);
  for (const key of ['phase', 'nos', 'sample_rate', 'balance', 'input', 'usb1_connected']) {
    assert.ok(dac.some((t) => t.control === key), key);
  }
  const bands = dac.filter((t) => 'band' in t);
  assert.equal(bands.length, 15, 'five bands, three parameters each');
  assert.deepEqual(bands.slice(0, 3), [{ band: 1, param: 'f' }, { band: 1, param: 'g' },
    { band: 1, param: 'q' }]);
  assert.deepEqual(dac.filter((t) => 'mode' in t).map((t) => t.mode), ['flat', 'eq', 'loudness']);
  assert.equal(dac.some((t) => 'loudness' in t), false, 'no strength outside Loudness');
  assert.deepEqual(dac.filter((t) => 'preset' in t).map((t) => t.preset),
    ['Flat', 'Warm', 'Bass', 'Presence']);
  const loud = targetsOf(byName['dac-loudness'].page.device);
  assert.deepEqual(loud.filter((t) => 'loudness' in t), [{ loudness: true }]);
  const loki = targetsOf(byName.lokimax.page.device);
  assert.deepEqual(loki.filter((t) => 'slot' in t).map((t) => t.slot), [1, 2, 3]);
  assert.deepEqual(loki.filter((t) => 'setting' in t).map((t) => t.setting),
    ['Flat', 'Bass Boost', 'Treble Boost', 'Smiley', 'Loudness']);
  assert.equal(loki.some((t) => 'band' in t), false, 'the Loki Max has sliders, no tone curve');
  for (const key of ['band1', 'band2', 'band3', 'band4', 'band5', 'band6']) {
    assert.ok(loki.some((t) => t.control === key), `the Loki Max slider ${key} is a target`);
  }
  // Every power readout is a target, by name.
  for (const { name, page } of cases) {
    const listed = targetsOf(page.device).map((t) => t.control);
    for (const readout of page.device.power?.readouts ?? []) {
      assert.ok(listed.includes(readout.key), `${name}: the power readout ${readout.key} is a target`);
    }
  }
  assert.ok(targetsOf(byName.amp.page.device).some((t) => t.control === 'warming_up'));
  assert.deepEqual(targetsOf(byName.amp.page.device).slice(0, 3),
    [{ control: 'power' }, { control: 'power_meters' }, { control: 'warming_up' }]);
  assert.deepEqual(targetsOf(byName.playing.page.device).filter((t) => 'transport' in t),
    [{ transport: 'previous' }, { transport: 'pause' }, { transport: 'next' }]);
  // Buttons the viewer may press but no track: nothing to operate, no transport target.
  const bare = byName['no-player'].page.device;
  assert.equal(bare.playing.buttons, true, 'the case has buttons');
  assert.equal(Boolean(bare.playing.track), false, 'and no track');
  assert.deepEqual(targetsOf(bare).filter((t) => 'transport' in t), []);
  // A track but no buttons (a guest on another sink): nothing either.
  const noButtons = byName['playing-guest-other-sink'].page.device;
  assert.equal(noButtons.playing.buttons, false);
  assert.deepEqual(targetsOf(noButtons).filter((t) => 'transport' in t), []);
  assert.deepEqual(targetsOf(null), []);
  // Every target is told apart from every other.
  for (const { name, page } of cases) {
    const seen = targetsOf(page.device).map((t) => JSON.stringify(t));
    assert.equal(new Set(seen).size, seen.length, `${name}: a target is listed twice`);
  }
});

await ok('facts_name_what_the_device_section_shows', async () => {
  const dac = factsOf(byName.dac.page.device);
  assert.deepEqual(dac.slice(0, 7), ['Product', 'Unit', 'Firmware', 'DSP', 'Profile', 'Link',
    'Signal']);
  assert.ok(dac.includes('12.3.3'), 'a reading row is named by its id');
  assert.equal(dac.length, 7 + byName.dac.page.device.facts.rows.length);
  assert.equal(dac.includes('Passphrase'), false);
  const noUnit = factsOf({ facts: { ...byName.dac.page.device.facts, unit: undefined } });
  assert.equal(noUnit.includes('Unit'), false, 'a guest sees no unit id');
  const noted = factsOf({ facts: { ...byName.dac.page.device.facts, passphrase_note: 'Wrong code' } });
  assert.ok(noted.includes('Passphrase'));
  assert.deepEqual(factsOf(null), []);
  assert.deepEqual(factsOf({}), []);
  const guest = factsOf(byName['dac-guest'].page.device);
  assert.equal(guest.includes('Unit'), byName['dac-guest'].page.device.facts.unit !== undefined);
});

await ok('every_target_maps_to_one_action_request', async () => {
  let targets = 0;
  for (const { name, page } of cases) {
    const { device, viewer } = page;
    if (viewer.guest || viewer.writes === false) continue;
    for (const target of targetsOf(device)) {
      const ops = 'slot' in target ? ['recall', 'save'] : [null];
      for (const op of ops) {
        targets++;
        const plan = planOf(device, target, op);
        const { actions, posts } = rig(pageOf(device, SIGNED));
        await plan.run(actions);
        const label = `${name} ${JSON.stringify(target)} ${op ?? ''}`;
        if (plan.want === null) {
          assert.deepEqual(posts, [], `${label}: a readout writes nothing`);
          continue;
        }
        const [path, body] = plan.want;
        const full = path === null ? 'api/v1/host/press' : `api/v1/devices/${device.url}/${path}`;
        assert.deepEqual(posts, [[full, body]], label);
        assert.equal(JSON.stringify(posts[0][1]), JSON.stringify(body), `${label}: the body's order`);
      }
    }
  }
  assert.ok(targets > 200, `${targets} targets in the fixture`);
});

await ok('a_followed_devices_targets_carry_its_name', async () => {
  const shown = byName.dac.page.device;
  let targets = 0;
  for (const { name, page } of cases) {
    if (page.viewer.guest) continue;
    const followed = { ...page.device, url: 'Amp' };
    for (const target of targetsOf(followed)) {
      const plan = planOf(followed, target);
      if (plan.want === null) continue;
      targets++;
      const { actions, posts } = rig(pageOf(shown, SIGNED, { Amp: followed }));
      await plan.run(actions, { device: 'Amp' });
      const [path, body] = plan.want;
      const full = path === null ? 'api/v1/host/press' : `api/v1/devices/Amp/${path}`;
      assert.deepEqual(posts, [[full, body]], `${name} ${JSON.stringify(target)}`);
    }
  }
  assert.ok(targets > 100, `${targets} targets of followed devices`);
});

await ok('a_guests_targets_send_nothing_and_say_why', async () => {
  const guest = byName['dac-guest'];
  const { actions, posts, toasts } = rig(pageOf(guest.page.device, guest.page.viewer));
  let tried = 0;
  for (const target of targetsOf(guest.page.device)) {
    const plan = planOf(guest.page.device, target);
    if (plan.want === null) continue;
    tried++;
    await plan.run(actions);
  }
  assert.deepEqual(posts, []);
  assert.equal(toasts.length, tried);
  assert.ok(toasts.every((t) => t === 'Sign in to change this'), toasts[0]);
});

console.log(`contract.mjs: ${n} passed`);
