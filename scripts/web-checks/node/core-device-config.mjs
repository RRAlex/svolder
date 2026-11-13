/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/device-config-actions.js, the writes to a configured device's settings
// (rename, profile, passphrase, default, remove) as core/actions.js gives them: each request's
// method, path and body with another device on show than the one named, no request when none is
// named, the devices answer handed back, every refusal as a reason, and a passphrase that is in
// the request's body and nowhere else. The actions post with the real `post` of core/api.js over
// a fetch that answers what the test says. Run: node core-device-config.mjs
import assert from 'node:assert/strict';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { post } from '../../../bins/svolderd/web/core/api.js';
import { DEVICE_REFUSALS } from '../../../bins/svolderd/web/core/device-config-actions.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };
const OFF = { signed_in: true, guest: false, writes: false, read_only: 'Read-only here' };
const dac = () => ({
  name: 'Gungnir 2', url: 'Gungnir%202', volume: null, controls: [], tone: null, facts: {},
});
const amp = () => ({ name: 'Desk amp', url: 'Desk%20amp', volume: null, controls: [], tone: null });
// The page shows the DAC; every case names the amp.
const SHOWN = 'Gungnir%202';
const NAMED = 'Desk%20amp';
const SECRET = '8642';
const DEVICES = {
  devices: [{ name: 'Desk amp', unit: '0011223344556677', passphrase: 'set', default: true }],
  profiles: [{ name: 'lyr-plus', display: 'Lyr+' }, { name: 'amp-common', base: true }],
};

// The actions over a page that shows the DAC. `reply(url, init)` answers a request: `{ status,
// body }` (an object is sent as JSON, a string as it is, nothing as an empty body), or throws.
function rig({ viewer = SIGNED, reply = () => ({ status: 200, body: DEVICES }) } = {}) {
  const sent = [];
  const gets = [];
  const toasts = [];
  const pushed = [];
  const assigned = [];
  const store = createStore({
    v: 1, viewer, fleet: { devices: [], hops: [] }, shown: SHOWN, device: dac(),
    followed: { [NAMED]: amp() }, card: null, host: null, strip: null,
  }, {
    signedOut: () => assigned.push('./'),
    fetch: async (path) => { gets.push(path); return { status: 200, json: async () => ({}) }; },
  });
  const io = {
    signedOut: () => assigned.push('./'),
    fetch: async (url, init) => {
      sent.push({ url, ...init });
      const { status, body } = reply(url, init);
      return {
        status,
        json: async () => {
          if (body === undefined || typeof body === 'string') throw new SyntaxError('no JSON');
          return body;
        },
      };
    },
  };
  const actions = createActions({
    store,
    post: (path, body) => post(path, body, io),
    toast: (text, action) => toasts.push(action ? [text, action] : [text]),
    history: { pushState: (...args) => pushed.push(args) },
    location: { assign: (to) => assigned.push(to) },
  });
  return { store, actions, sent, gets, toasts, pushed, assigned };
}

// Each write: its name, its call on the amp, the path's last segment and the body it sends.
const WRITES = [
  ['renameDevice', (a, o) => a.renameDevice('Desk', o), 'edit', { name: 'Desk' }],
  ['setProfile', (a, o) => a.setProfile('lyr-plus', o), 'edit', { profile: 'lyr-plus' }],
  ['setProfile(null)', (a, o) => a.setProfile(null, o), 'edit', { profile: null }],
  ['setPassphrase', (a, o) => a.setPassphrase(SECRET, o), 'edit', { passphrase: SECRET }],
  ['makeDefault', (a, o) => a.makeDefault(o), 'default', {}],
  ['removeDevice', (a, o) => a.removeDevice(o), 'remove', {}],
];

await ok('each_request_is_a_post_to_the_named_device_with_another_on_show', async () => {
  for (const [name, run, tail, body] of WRITES) {
    const r = rig();
    assert.equal(r.store.shown(), SHOWN);
    await run(r.actions, { device: NAMED });
    assert.equal(r.sent.length, 1, name);
    const [request] = r.sent;
    assert.equal(request.method, 'POST', name);
    assert.equal(request.url, `api/v1/devices/${NAMED}/${tail}`, name);
    assert.deepEqual(JSON.parse(request.body), body, name);
    assert.equal(request.headers['Content-Type'], 'application/json', name);
    assert.equal(request.credentials, 'same-origin', name);
    assert.ok(!request.url.includes(SHOWN), `${name}: the shown device is not in the address`);
  }
});

await ok('a_call_that_names_no_device_sends_nothing_and_never_falls_back_to_the_shown_one',
  async () => {
    for (const [name, run] of WRITES) {
      for (const options of [undefined, {}, { device: undefined }, { device: null },
        { device: '' }]) {
        const r = rig();
        const answer = await run(r.actions, options);
        assert.deepEqual(r.sent, [], name);
        assert.equal(answer.ok, false, name);
        assert.equal(answer.status, 0, name);
        assert.equal(answer.reason, 'no_device', name);
        assert.ok(answer.error, name);
        assert.deepEqual(r.toasts, [], name);
      }
    }
  });

await ok('a_write_answers_the_devices_and_the_profiles_and_touches_nothing_else', async () => {
  for (const [name, run] of WRITES) {
    const r = rig();
    const answer = await run(r.actions, { device: NAMED });
    assert.deepEqual(answer, { ok: true, status: 200, ...DEVICES }, name);
    assert.deepEqual(r.toasts, [], name);
    assert.deepEqual(r.gets, [], `${name}: no model is read again`);
    assert.deepEqual(r.pushed, [], name);
    assert.deepEqual(r.assigned, [], name);
  }
});

// What the routes refuse with, and the reason each reads as.
const said = (status, reason, error = `the daemon's words for ${reason}`) => (
  { status, body: { error, reason } });
const REFUSALS = [
  ['taken', said(422, 'taken')],
  ['missing', said(404, 'missing')],
  ['invalid', said(422, 'invalid')],
  ['read_only', said(409, 'read_only')],
  ['file_busy', said(409, 'file_busy')],
  ['stopping', said(503, 'stopping')],
  ['signed_out', { status: 401 }],
  ['signed_out', { status: 401, body: { error: 'Sign in to change this' } }],
  ['forbidden', { status: 403, body: { error: 'Read-only: [web] writes is off' } }],
  ['missing', { status: 404 }],
  ['missing', { status: 404, body: { error: 'no configured device matches "Desk amp"' } }],
  ['failed', { status: 400, body: { error: 'the body is not what this takes: it is not JSON' } }],
  ['failed', { status: 413, body: 'length limit exceeded' }],
  ['failed', said(422, 'bad_name')],
  ['failed', said(500, 'internal')],
  ['failed', { status: 503, body: { error: 'no daemon' } }],
  ['failed', { status: 422, body: { error: 'odd', reason: 7 } }],
];

await ok('each_refusal_is_a_reason_the_page_can_word_with_the_daemons_words_beside', async () => {
  for (const [name, run] of WRITES) {
    for (const [reason, reply] of REFUSALS) {
      const r = rig({ reply: () => reply });
      const answer = await run(r.actions, { device: NAMED });
      const what = `${name} ${reply.status} ${reason}`;
      assert.equal(answer.ok, false, what);
      assert.equal(answer.status, reply.status, what);
      assert.equal(answer.reason, reason, what);
      assert.ok(DEVICE_REFUSALS.includes(answer.reason), what);
      const words = typeof reply.body === 'object' ? reply.body.error : undefined;
      if (words) assert.equal(answer.error, words, what);
      else assert.ok(typeof answer.error === 'string' && answer.error.length > 0, what);
      assert.deepEqual(Object.keys(answer).sort(), ['error', 'ok', 'reason', 'status'], what);
      // the page words the refusal at its control: nothing is toasted, no model is read again
      assert.deepEqual(r.toasts, [], what);
      assert.deepEqual(r.gets, [], what);
      assert.deepEqual(r.assigned, reply.status === 401 ? ['./'] : [], what);
    }
  }
});

await ok('no_answer_at_all_is_a_reason_of_its_own', async () => {
  for (const [name, run] of WRITES) {
    const r = rig({ reply: () => { throw new TypeError('network'); } });
    const answer = await run(r.actions, { device: NAMED });
    assert.deepEqual(answer, {
      ok: false, status: 0, reason: 'no_answer', error: 'svolder did not answer.',
    }, name);
  }
});

await ok('a_guest_and_a_page_with_writes_off_send_nothing_and_say_why', async () => {
  for (const [name, run] of WRITES) {
    const guest = rig({ viewer: GUEST });
    const turned = await run(guest.actions, { device: NAMED });
    assert.deepEqual(guest.sent, [], name);
    assert.deepEqual(turned, {
      ok: false, status: 0, reason: 'signed_out', error: 'Sign in to change this',
    }, name);
    assert.equal(guest.toasts.length, 1, name);
    const off = rig({ viewer: OFF });
    const refused = await run(off.actions, { device: NAMED });
    assert.deepEqual(off.sent, [], name);
    assert.deepEqual(refused, {
      ok: false, status: 0, reason: 'forbidden', error: 'Read-only here',
    }, name);
    assert.deepEqual(off.toasts, [['Read-only here']], name);
  }
});

await ok('every_reason_a_write_can_answer_is_listed_once', () => {
  assert.deepEqual([...DEVICE_REFUSALS].sort(), ['busy', 'failed', 'file_busy', 'forbidden',
    'invalid', 'missing', 'no_answer', 'no_device', 'no_such_job', 'not_waiting', 'read_only',
    'signed_out', 'stopping', 'taken']);
  assert.ok(Object.isFrozen(DEVICE_REFUSALS));
});

// A passphrase is in the request's body and nowhere else: not in the address, a header, a toast,
// a console line, the history, the store, or the answer, whatever the daemon says back.
await ok('a_passphrase_travels_in_the_body_only', async () => {
  const echo = `passphrase "${SECRET}" is not four digits`;
  const replies = [
    { status: 200, body: DEVICES },
    ...REFUSALS.map(([, reply]) => reply),
    { status: 422, body: { error: echo, reason: 'invalid' } },
    { status: 409, body: { error: `invalid value: integer \`${SECRET}\``, reason: 'read_only' } },
    { status: 500, body: { error: echo } },
  ];
  const lines = [];
  const kept = {};
  for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
    kept[level] = console[level];
    console[level] = (...args) => lines.push(args);
  }
  try {
    for (const reply of replies) {
      for (const viewer of [SIGNED, GUEST, OFF]) {
        for (const options of [{ device: NAMED }, undefined]) {
          const r = rig({ viewer, reply: () => reply });
          const answer = await r.actions.setPassphrase(SECRET, options);
          for (const request of r.sent) {
            const { body, ...rest } = request;
            assert.deepEqual(JSON.parse(body), { passphrase: SECRET });
            assert.ok(!JSON.stringify(rest).includes(SECRET), 'the address and the headers');
          }
          const elsewhere = JSON.stringify([answer, r.toasts, r.pushed, r.assigned, r.gets,
            r.store.page(), lines]);
          assert.ok(!elsewhere.includes(SECRET), `${reply.status}: ${elsewhere}`);
          if (answer.ok === false) assert.ok(answer.error.length > 0);
        }
      }
    }
    // the daemon's words stand when they quote no passphrase, and for the two words that are none
    const plain = rig({ reply: () => said(422, 'invalid', 'a passphrase is auto, none or four digits') });
    assert.equal((await plain.actions.setPassphrase('none', { device: NAMED })).error,
      'a passphrase is auto, none or four digits');
    // a rename's words are never taken for a passphrase
    const rename = rig({ reply: () => said(422, 'taken', 'the name "Desk" is taken') });
    assert.equal((await rename.actions.renameDevice('Desk', { device: NAMED })).error,
      'the name "Desk" is taken');
  } finally {
    Object.assign(console, kept);
  }
  assert.deepEqual(lines, []);
});

console.log(`${n} core-device-config checks passed`);
