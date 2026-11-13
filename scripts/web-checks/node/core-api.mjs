/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/api.js, the page's two calls to the daemon (`post`, `get`): each
// answers `{ status, ...json }`, a 401 loads the sign-in page, no answer at all is status 0 with
// the words "svolder did not answer.", a body that is no JSON leaves the status alone with no
// fields (never a rejection), and what is sent is the call's own request. The fetch is a fake.
// Run: node core-api.mjs
import assert from 'node:assert/strict';
import {
  NO_ANSWER, get, post,
} from '../../../bins/svolderd/web/core/api.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

// A fetch that answers `status` and `body` (a string of the body's text), and notes its calls.
function fake({ status = 200, body = '{}', fail = false } = {}) {
  const calls = [];
  const out = { out: 0 };
  const io = {
    fetch: async (path, init) => {
      calls.push([path, init]);
      if (fail) throw new TypeError('network');
      return { status, json: async () => JSON.parse(body) };
    },
    signedOut: () => { out.out++; },
  };
  return { io, calls, out };
}

const CALLS = [
  ['post', (io) => post('api/v1/x', { a: 1 }, io)],
  ['get', (io) => get('api/v1/x', io)],
];

for (const [name, call] of CALLS) {
  await ok(`${name}_answers_the_status_with_the_json_body`, async () => {
    const { io, out } = fake({ status: 200, body: '{"ok":true,"n":3}' });
    assert.deepEqual(await call(io), { status: 200, ok: true, n: 3 });
    assert.equal(out.out, 0);
  });

  await ok(`${name}_a_401_loads_the_sign_in_page_and_still_answers`, async () => {
    const { io, out } = fake({ status: 401, body: '{"error":"Sign in"}' });
    assert.deepEqual(await call(io), { status: 401, error: 'Sign in' });
    assert.equal(out.out, 1);
  });

  await ok(`${name}_no_answer_is_status_0_never_a_rejection`, async () => {
    const { io, out } = fake({ fail: true });
    assert.deepEqual(await call(io), { status: 0, error: NO_ANSWER });
    assert.equal(out.out, 0);
  });

  await ok(`${name}_an_error_body_that_is_no_json_keeps_the_status`, async () => {
    const { io } = fake({ status: 502, body: '<html>bad gateway' });
    assert.deepEqual(await call(io), { status: 502 });
  });
}

await ok('each_call_sends_its_own_request_with_the_session_cookie', async () => {
  const sent = fake();
  await post('api/v1/x', { a: 1 }, sent.io);
  assert.deepEqual(sent.calls[0], ['api/v1/x', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{"a":1}',
    credentials: 'same-origin',
  }]);
  const read = fake();
  await get('api/v1/y?z=1', read.io);
  assert.deepEqual(read.calls[0], ['api/v1/y?z=1', { credentials: 'same-origin' }]);
});

await ok('the_calls_use_the_pages_own_fetch_and_sign_in_by_default', async () => {
  const seen = [];
  globalThis.fetch = async (path, init) => {
    seen.push([path, init]);
    return { status: 401, json: async () => ({}) };
  };
  const assigned = [];
  globalThis.location = { assign: (to) => assigned.push(to) };
  assert.deepEqual(await get('api/v1/x'), { status: 401 });
  assert.deepEqual(await post('api/v1/x', {}), { status: 401 });
  assert.deepEqual(assigned, ['./', './']);
  assert.equal(seen.length, 2);
});

await ok('an_aborted_request_is_status_0_marked_aborted_and_the_signal_is_passed_on', async () => {
  const controller = new AbortController();
  const seen = [];
  const io = {
    fetch: async (path, init) => {
      seen.push(init.signal);
      controller.abort();
      throw new DOMException('aborted', 'AbortError');
    },
    signedOut() {},
    signal: controller.signal,
  };
  assert.deepEqual(await get('api/v1/x', io),
    { status: 0, error: NO_ANSWER, aborted: true });
  assert.equal(seen[0], controller.signal);
  const dead = fake({ fail: true });
  assert.equal((await get('api/v1/x', dead.io)).aborted, undefined, 'a dead link is not an abort');
});

console.log(`${n} core-api checks passed`);
