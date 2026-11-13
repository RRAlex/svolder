/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the device walk on the page: core/walk-actions.js (start, answer, back and
// cancel as core/actions.js gives them: each request's method, path and body, the job a call
// names and no other, every refusal as a reason, a passphrase in the body and nowhere else). The
// walk's draw cases follow, then the wire's, on the real core over a jsdom page: the add page's
// entry, the states drawn in place, the answer and the state in either order, a job the daemon no
// longer knows, a walk of another origin, the cancel on leaving, the first device of a fresh
// install. The actions post with the real `post` of core/api.js
// over a fetch that answers what the test says. Run: node core-device-walk.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { post } from '../../../bins/svolderd/web/core/api.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import {
  AGAIN_ID, CONTROLS_ID, FIELD_ID, HEAD_ID, OPEN_ID, REFUSAL_ID, REVEAL_ID, ROWS_ID, STATUS_ID,
  WALK_WORDS, controlsHtml, headHtml, refusalHtml, refusalWords, rowsHtml, statusHtml, walkHtml,
} from '../../../bins/svolderd/web/core/popups/devices/walk.js';
import { WALK_REFUSALS } from '../../../bins/svolderd/web/core/walk-actions.js';

let n = 0;
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};

const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };
const OFF = { signed_in: true, guest: false, writes: false, read_only: 'Read-only here' };
const SECRET = '8642';

// The actions over a page. `reply(url, init)` answers a request: `{ status, body }` (an object is
// sent as JSON, a string as it is, nothing as an empty body), or throws.
function rig({ viewer = SIGNED, reply = () => ({ status: 204 }) } = {}) {
  const sent = [];
  const toasts = [];
  const assigned = [];
  const store = createStore({
    v: 1, viewer, fleet: { devices: [], hops: [] }, shown: 'Gungnir%202',
    device: { name: 'Gungnir 2', url: 'Gungnir%202', volume: null, controls: [], tone: null },
    followed: {}, card: null, host: null, strip: null,
  }, { signedOut: () => assigned.push('./'), fetch: async () => ({ status: 200, json: async () => ({}) }) });
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
    history: { pushState() {} },
    location: { assign: (to) => assigned.push(to) },
  });
  return { store, actions, sent, toasts, assigned };
}

// Each call that names a job: its name, the call, the path's last segment and the body it sends.
const CALLS = [
  ['answer(choice)', (a, o) => a.walk.answer({ choice: 1 }, o), 'answer', { choice: 1 }],
  ['answer(text)', (a, o) => a.walk.answer({ text: 'Desk' }, o), 'answer', { text: 'Desk' }],
  ['back', (a, o) => a.walk.back(o), 'back', {}],
  ['cancel', (a, o) => a.walk.cancel(o), 'cancel', {}],
];

await ok('start_is_a_post_that_answers_the_job', async () => {
  const r = rig({ reply: () => ({ status: 200, body: { job: 7 } }) });
  const answer = await r.actions.walk.start();
  assert.deepEqual(answer, { ok: true, status: 200, job: 7 });
  assert.equal(r.sent.length, 1);
  assert.equal(r.sent[0].url, 'api/v1/walk');
  assert.equal(r.sent[0].method, 'POST');
  assert.equal(r.sent[0].credentials, 'same-origin');
});

await ok('each_call_is_a_post_to_the_job_it_names', async () => {
  for (const [name, run, tail, body] of CALLS) {
    const r = rig();
    const answer = await run(r.actions, { job: 12 });
    assert.deepEqual(answer, { ok: true, status: 204 }, name);
    assert.equal(r.sent.length, 1, name);
    assert.equal(r.sent[0].url, `api/v1/walk/12/${tail}`, name);
    assert.equal(r.sent[0].method, 'POST', name);
    assert.deepEqual(JSON.parse(r.sent[0].body), body, name);
    assert.equal(r.sent[0].headers['Content-Type'], 'application/json', name);
  }
});

await ok('a_call_names_the_run_its_job_was_told_in_when_it_has_one', async () => {
  for (const [name, run, tail, body] of CALLS) {
    const r = rig();
    await run(r.actions, { job: 12, run: 1700000000123 });
    assert.deepEqual(JSON.parse(r.sent[0].body), { ...body, run: 1700000000123 }, name);
    const bare = rig();
    await run(bare.actions, { job: 12 });
    assert.deepEqual(JSON.parse(bare.sent[0].body), body, `${name}: no run, no key`);
    assert.equal(r.sent[0].url, `api/v1/walk/12/${tail}`);
  }
});

await ok('a_call_that_names_no_job_sends_nothing', async () => {
  for (const [name, run] of CALLS) {
    for (const options of [undefined, {}, { job: undefined }, { job: null }, { job: '1' },
      { job: -1 }, { job: 1.5 }]) {
      const r = rig();
      const answer = await run(r.actions, options);
      assert.deepEqual(r.sent, [], name);
      assert.deepEqual(answer, {
        ok: false, status: 0, reason: 'no_job', error: 'No walk is named.',
      }, name);
      assert.deepEqual(r.toasts, [], name);
    }
  }
});

const said = (status, reason, error = `the daemon's words for ${reason}`) => (
  { status, body: { error, reason } });
const REFUSALS = [
  ['busy', said(409, 'busy')],
  ['not_waiting', said(409, 'not_waiting')],
  ['no_such_job', said(404, 'no_such_job')],
  ['stopping', said(503, 'stopping')],
  ['invalid', said(422, 'invalid')],
  ['signed_out', { status: 401 }],
  ['forbidden', { status: 403, body: { error: 'Read-only: [web] writes is off' } }],
  ['no_such_job', { status: 404 }],
  ['failed', { status: 400, body: { error: 'the request is not what this takes: no choice' } }],
  ['failed', { status: 413, body: 'length limit exceeded' }],
  ['failed', said(500, 'internal')],
  ['failed', { status: 422, body: { error: 'odd', reason: 7 } }],
];

await ok('each_refusal_is_a_reason_of_the_closed_list_with_the_daemons_words_beside', async () => {
  const all = [['start', (a) => a.walk.start()], ...CALLS.map(([name, run]) => [name,
    (a) => run(a, { job: 3 })])];
  for (const [name, run] of all) {
    for (const [reason, reply] of REFUSALS) {
      const r = rig({ reply: () => reply });
      const answer = await run(r.actions);
      const what = `${name} ${reply.status} ${reason}`;
      assert.equal(answer.ok, false, what);
      assert.equal(answer.status, reply.status, what);
      assert.equal(answer.reason, reason, what);
      assert.ok(WALK_REFUSALS.includes(answer.reason), what);
      const words = typeof reply.body === 'object' ? reply.body.error : undefined;
      if (words) assert.equal(answer.error, words, what);
      else assert.ok(typeof answer.error === 'string' && answer.error.length > 0, what);
      assert.deepEqual(Object.keys(answer).sort(), ['error', 'ok', 'reason', 'status'], what);
      assert.deepEqual(r.toasts, [], what);
      assert.deepEqual(r.assigned, reply.status === 401 ? ['./'] : [], what);
    }
  }
});

await ok('the_closed_list_is_frozen_and_lists_every_reason_once', () => {
  assert.deepEqual([...WALK_REFUSALS].sort(), ['busy', 'failed', 'forbidden', 'invalid',
    'no_answer', 'no_job', 'no_such_job', 'not_waiting', 'signed_out', 'stopping']);
  assert.ok(Object.isFrozen(WALK_REFUSALS));
});

await ok('no_answer_at_all_is_a_reason_of_its_own', async () => {
  const r = rig({ reply: () => { throw new TypeError('network'); } });
  assert.deepEqual(await r.actions.walk.start(), {
    ok: false, status: 0, reason: 'no_answer', error: 'svolder did not answer.',
  });
  assert.deepEqual(await r.actions.walk.cancel({ job: 1 }), {
    ok: false, status: 0, reason: 'no_answer', error: 'svolder did not answer.',
  });
});

await ok('a_guest_and_a_page_with_writes_off_send_nothing_and_say_why', async () => {
  const all = [['start', (a) => a.walk.start()], ...CALLS.map(([name, run]) => [name,
    (a) => run(a, { job: 3 })])];
  for (const [name, run] of all) {
    const guest = rig({ viewer: GUEST });
    assert.deepEqual(await run(guest.actions), {
      ok: false, status: 0, reason: 'signed_out', error: 'Sign in to change this',
    }, name);
    assert.deepEqual(guest.sent, [], name);
    const off = rig({ viewer: OFF });
    assert.deepEqual(await run(off.actions), {
      ok: false, status: 0, reason: 'forbidden', error: 'Read-only here',
    }, name);
    assert.deepEqual(off.sent, [], name);
  }
});

await ok('a_passphrase_is_in_the_body_only', async () => {
  const r = rig();
  await r.actions.walk.answer({ text: SECRET }, { job: 4 });
  assert.ok(!r.sent[0].url.includes(SECRET));
  assert.equal(JSON.parse(r.sent[0].body).text, SECRET);
  const quoting = rig({ reply: () => said(422, 'invalid', `${SECRET} is not a code`) });
  const answer = await quoting.actions.walk.answer({ text: SECRET }, { job: 4 });
  assert.ok(!JSON.stringify(answer).includes(SECRET), 'the answer does not repeat it');
  assert.equal(answer.reason, 'invalid');
  assert.equal(quoting.toasts.length, 0);
});


// ---- the draw: the daemon's own states (fixtures/device-walk.json, copied from svolderd's `walk`
// events on the fake transport), each as the page draws it ----

const FIXTURE = JSON.parse(readFileSync(new URL('./fixtures/device-walk.json', import.meta.url)));
const progress = (name) => FIXTURE[name].state.payload;
// the daemon's passphrase prompt as the walk crate words it (`passphrase_prompt`): the walk's `auto`
// is accepted by the fake, so no real event of it was captured
const PASSPHRASE_PROMPT = {
  step: 'passphrase', title: 'The device wants its passphrase', default: 0, takes: 'secret',
  body: ['It turned the connection away, which is what a device with a passphrase does.',
    'The four digits the phone app shows, `auto` to have svolder read the code from the device, '
      + 'or `none` for a device with no passphrase.', 'none'],
  choices: [],
};
const live = (state, more = {}) => ({
  mode: 'live', state, end: null, sending: false, words: null, revealed: false, device: null, ...more,
});
const tag = (html, pattern) => [...html.matchAll(pattern)].map((m) => m[0]);
const buttons = (html) => tag(html, /<button[^>]*>[^<]*<\/button>/g);
const choicesOf = (html) => buttons(html).filter((b) => b.includes('data-walk-choice'));
const textOf = (html) => html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();

await ok('the_warning_is_the_daemons_prompt_and_the_safe_answer_takes_the_focus', () => {
  const state = progress('linked');
  const html = controlsHtml(live(state));
  const head = headHtml(live(state));
  assert.match(head, /A device is linked/);
  assert.ok(head.includes('have a link: the scan and the passphrase test may briefly disturb them.'));
  const choices = choicesOf(html);
  assert.equal(choices.length, 2);
  assert.match(choices[0], /data-walk-choice="0"[^>]*>Go on</);
  assert.match(choices[1], /data-walk-choice="1"[^>]*>Cancel</);
  assert.ok(!choices[0].includes('data-walk-focus'), 'go on does not take the focus');
  assert.ok(choices[1].includes('data-walk-focus'), 'cancel takes the focus');
  assert.ok(!choices.some((c) => c.includes('aria-disabled')));
  assert.ok(html.includes('id="popup-devices-walk-stop"'));
  assert.ok(!html.includes('id="popup-devices-walk-back"'), 'the first step cannot go back');
});

await ok('the_scan_prompt_is_one_button_and_the_rows_come_live_keyed_by_unit', () => {
  const waiting = live(progress('scan'));
  assert.equal(choicesOf(controlsHtml(waiting)).length, 1);
  assert.ok(choicesOf(controlsHtml(waiting))[0].includes('data-walk-focus'));
  assert.equal(rowsHtml(waiting), '');
  const heard = live(progress('scanning'));
  const rows = rowsHtml(heard);
  assert.equal(tag(rows, /<li /g).length, 2);
  assert.match(rows, /data-unit="0102030405060708"[^>]*data-configured="Gungnir 2"/);
  assert.match(rows, /data-unit="1122334455667788"/);
  assert.ok(rows.includes('-50 dBm'));
  assert.ok(rows.includes('Configured as Gungnir 2'));
  assert.ok(rows.includes('Bifrost 3'));
  // nothing heard yet, while it listens
  const none = live({ ...progress('scanning'), rows: [] });
  assert.match(rowsHtml(none), /No device heard yet/);
  // the rows are the scan's: no list at another step
  assert.equal(rowsHtml(live(progress('pick'))), '');
});

const STOP_NAME = 'popup-devices-walk-stop';

await ok('a_walk_that_works_is_read_only_and_busy_with_the_words_of_what_it_does', () => {
  for (const [name, words] of [['scanning', 'Listening for devices'], ['testing', 'Testing the'],
    ['writing', 'Saving']]) {
    const view = live(progress(name));
    const html = controlsHtml(view);
    assert.ok(html.includes('aria-busy="true"'), name);
    assert.ok(statusHtml(view).includes(words), name);
    for (const b of buttons(html)) {
      assert.ok(b.includes('aria-disabled="true"') || b.includes(STOP_NAME), `${name}: ${b}`);
    }
  }
  const name = controlsHtml(live(progress('testing')));
  assert.ok(name.includes('readonly'), 'the field cannot be typed in while it works');
  // a call of the page's own on its way does the same
  const sending = live(progress('name'), { sending: true });
  assert.ok(controlsHtml(sending).includes('aria-busy="true"'));
  assert.match(statusHtml(sending), /Working/);
  assert.match(controlsHtml(live(progress('name'))), /aria-busy="false"/);
});

await ok('the_pick_offers_a_configured_unit_marked_and_not_pressable', () => {
  const state = progress('pick');
  const html = controlsHtml(live(state));
  const [first, second] = choicesOf(html);
  assert.ok(first.includes('configured as Gungnir 2'));
  assert.ok(first.includes('aria-disabled="true"'), 'a configured unit is not offered');
  assert.ok(!first.includes('data-walk-focus'));
  assert.ok(!second.includes('aria-disabled'));
  assert.ok(second.includes('data-walk-focus'), 'the first unit that can be chosen takes the focus');
  assert.ok(html.includes('id="popup-devices-walk-back"'), 'the walk can step back here');
});

await ok('the_daemons_refusal_is_worded_at_the_control_it_is_about', () => {
  const state = progress('pick_refused');
  const said = refusalHtml(live(state));
  assert.ok(said.includes('0102030405060708 is configured already, as Gungnir 2'));
  assert.ok(said.includes('fp-failed'));
  assert.ok(!refusalHtml(live(progress('pick'))).includes('fp-failed'));
  // a text prompt that was refused describes its field by the refusal
  const named = { ...progress('name'), refusal: { kind: 'taken', message: 'Another device has that name.' } };
  const html = controlsHtml(live(named));
  assert.ok(html.includes('aria-invalid="true"'));
  assert.ok(html.includes(`aria-describedby="${REFUSAL_ID}"`));
  assert.ok(refusalHtml(live(named)).includes('Another device has that name.'));
  assert.ok(controlsHtml(live(progress('name'))).includes('aria-invalid="false"'));
  // a call of the page's that was refused says it in the same place
  const failed = refusalHtml(live(progress('name'), { words: 'The walk is no longer there.' }));
  assert.ok(failed.includes('The walk is no longer there.') && failed.includes('fp-failed'));
});

await ok('a_text_prompt_is_a_field_that_holds_nothing_and_shows_the_default_as_its_placeholder', () => {
  const state = progress('name');
  const view = live(state);
  const head = headHtml(view);
  assert.ok(head.includes('What to call it'));
  assert.ok(head.includes('Enter takes the name below; type another to use that instead.'));
  assert.match(head, /fpw-default">Bifrost 3</, 'the last body line is the default');
  const html = controlsHtml(view);
  const input = tag(html, /<input[^>]*>/g)[0];
  assert.ok(input.includes('type="text"') && input.includes('placeholder="Bifrost 3"'));
  assert.ok(!/ value=/.test(input), 'the field holds nothing');
  assert.ok(input.includes('data-walk-focus'));
  assert.ok(!html.includes(REVEAL_ID));
  assert.ok(html.includes('data-walk-form'));
  assert.ok(!html.includes('data-walk-choice'));
});

await ok('a_secret_prompt_is_a_password_field_never_filled_with_a_reveal_per_the_viewers_say', () => {
  // the daemon's passphrase step, as its prompt says it takes a secret (`takes`)
  const secret = { ...progress('name'), prompt: PASSPHRASE_PROMPT };
  const hidden = controlsHtml(live(secret));
  const input = tag(hidden, /<input[^>]*>/g)[0];
  assert.ok(input.includes('type="password"') && input.includes('autocomplete="new-password"'));
  assert.ok(!/ value=/.test(input) && !input.includes('placeholder'), 'never filled, no hint of a code');
  assert.ok(hidden.includes(`id="${REVEAL_ID}"`) && hidden.includes('aria-pressed="false"'));
  const shown = controlsHtml(live(secret, { revealed: true }));
  assert.ok(tag(shown, /<input[^>]*>/g)[0].includes('type="text"'));
  assert.ok(shown.includes('aria-pressed="true"'));
  assert.ok(shown.includes(`aria-controls="${FIELD_ID}"`));
  assert.ok(!/ value=/.test(shown));
});

// how a walk ended is read in the status line; the controls hold the buttons
const endedAs = (view) => ({ status: statusHtml({ mode: 'ended', state: null, ...view }),
  controls: controlsHtml({ mode: 'ended', state: null, ...view }) });

await ok('a_done_walk_says_the_device_its_link_and_offers_its_page', () => {
  const end = { kind: 'done', payload: FIXTURE.done.state.payload, message: '' };
  const bare = endedAs({ end, device: null });
  assert.ok(bare.status.includes('Bifrost 3 was added.'));
  assert.ok(!bare.controls.includes(OPEN_ID), 'no page to open until the fleet lists it');
  assert.ok(bare.controls.includes(AGAIN_ID));
  const device = { name: 'Bifrost 3', url: 'Bifrost%203', connected: true, since: null };
  const html = endedAs({ end, device });
  assert.ok(html.status.includes('It is connected.') && html.status.includes('fp-dot on'));
  assert.match(html.controls, /id="popup-devices-walk-open" data-open="Bifrost%203"/);
  const away = endedAs({ end, device: { ...device, connected: false } });
  assert.ok(away.status.includes('It is not connected yet.') && away.status.includes('fp-dot off'));
  // a name is a user's text
  const odd = { ...end, payload: { ...end.payload, name: '<b>x</b>' } };
  assert.ok(!endedAs({ end: odd }).status.includes('<b>'));
});

await ok('a_walk_that_ended_without_a_device_says_so_and_offers_to_start_again', () => {
  const view = (end) => textOf(endedAs({ end }).status);
  assert.match(view({ kind: 'cancelled', payload: null }), /The walk was cancelled\./);
  assert.match(view({ kind: 'done', payload: null }), /No device was added\./);
  assert.match(view({ kind: 'failed', message: 'the scan could not run' }), /the scan could not run/);
  assert.match(view({ kind: 'failed', message: '' }), /The walk stopped\./);
  assert.ok(endedAs({ end: { kind: 'failed', message: 'x' } }).status.includes('fp-failed'));
  assert.equal(buttons(endedAs({ end: { kind: 'cancelled' } }).controls).length, 1);
  // a start that was refused has only the way to try again
  assert.ok(buttons(controlsHtml({ mode: 'idle' })).length === 1);
});

await ok('a_walk_of_another_origin_is_words_only', () => {
  const state = { ...progress('scan'), origin: 'socket' };
  const view = { mode: 'foreign', state, origin: 'socket', end: null, sending: false };
  assert.equal(buttons(controlsHtml(view)).length, 0, 'nothing to press');
  assert.equal(controlsHtml(view), '');
  assert.ok(!/<input|<form/.test(walkHtml(view)));
  const said = textOf(statusHtml(view));
  assert.match(said, /A device walk started from the terminal is running\./);
  assert.ok(!/socket/.test(said), 'the daemon\'s log word is never shown');
  assert.ok(said.includes('Find the device'), "the step's title");
  assert.equal(rowsHtml(view), '');
});

await ok('starting_and_joining_say_what_they_wait_for_and_have_no_controls', () => {
  for (const mode of ['starting', 'joining']) {
    const view = { mode, state: null, end: null };
    assert.equal(controlsHtml(view), '');
    assert.ok(statusHtml(view).length > 40);
  }
  assert.match(statusHtml({ mode: 'starting' }), /Starting/);
});

await ok('the_page_is_five_places_each_by_its_id', () => {
  const html = walkHtml(live(progress('name')));
  for (const id of [HEAD_ID, ROWS_ID, CONTROLS_ID, STATUS_ID, REFUSAL_ID]) {
    assert.equal(html.split(`id="${id}"`).length, 2, id);
  }
  assert.ok(html.includes('data-mode="live"'));
});

await ok('the_walks_words_are_escaped', () => {
  const odd = { ...progress('pick'), prompt: { ...progress('pick').prompt, title: '<i>t</i>',
    choices: ['<u>a</u>  0102030405060708  x', 'b'] }, refusal: { kind: 'answer', message: '<s>no</s>' } };
  const html = walkHtml(live(odd));
  for (const bad of ['<i>', '<u>', '<s>']) assert.ok(!html.includes(bad), bad);
});

await ok('every_refusal_reason_of_a_call_has_words_of_its_own', () => {
  for (const reason of WALK_REFUSALS) {
    const said = refusalWords({ reason, status: 422, error: 'the daemon said so' });
    assert.ok(said.length > 10, reason);
  }
  assert.equal(refusalWords({ reason: 'invalid', error: 'the daemon said so' }), 'the daemon said so');
  assert.equal(refusalWords({ reason: 'failed', error: '' }), 'The walk did not take that.');
  assert.equal(refusalWords({ reason: 'not_waiting', error: 'walk 1 is not waiting' }),
    WALK_WORDS.refusals.not_waiting);
});


// ---- the wire: the real core over a jsdom page, a small fake daemon and the events it sends ----

const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { DEVICES } = await import('../../../bins/svolderd/web/core/popups/listed.js');
const { ADD, STOP_ID, BACK_ID } = await import('../../../bins/svolderd/web/core/popups/devices/walk.js');
const { OWNER, WALK_OWNER } = await import('../../../bins/svolderd/web/core/popups/devices/wire.js');
console.warn = () => {};

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });
const seg = (name) => encodeURIComponent(name);
const modelOf = (name) => ({ name, url: seg(name), link: link(true), volume: null, controls: [], meters: null });
const empty = { now_ms: 0, bin_ms: 1000, secs: 1800, refused: [], link: [], series: [] };

class FakeSource {
  static all = [];

  constructor(url) { this.url = url; this.listeners = {}; FakeSource.all.push(this); }

  addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); }

  close() {}

  send(name, data) { for (const fn of this.listeners[name] ?? []) fn({ data: JSON.stringify(data) }); }
}

/** A `walk` event of job `job` carrying `payload` as a progress state. */
const progressOf = (job, name, over = {}) => ({
  job, state: { kind: 'progress', payload: { ...FIXTURE[name].state.payload, ...over } },
});

/** The popup over the real core. `none` starts a fresh install (no device configured). The fake
 * daemon keeps every request in `posts`; `start` answers the start (`{ status, body }`), `answer`
 * every other walk call, `held` is a promise a call waits for. */
function wired({ none = false } = {}) {
  const p = page(BODY, { url: 'http://svolder.test/' + (none ? '' : '?device=A') });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  FakeSource.all = [];
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const daemon = {
    rows: none ? [] : [{ unit: 'UNIT-A', name: 'A', passphrase: 'auto', connected: true }],
    posts: [], start: { status: 200, body: { job: 1 } }, answer: { status: 204 }, hold: null,
    gone: [], reads: [],
  };
  p.window.fetch = async (path, init = {}) => {
    const reply = (status, body) => ({ status, ok: status === 200, json: async () => body ?? {} });
    const call = /^api\/v1\/walk(?:\/(\d+)\/(answer|back|cancel))?$/.exec(path);
    if (call) {
      const sent = { path, job: call[1] === undefined ? null : Number(call[1]), tail: call[2] ?? 'start',
        body: JSON.parse(init.body ?? '{}') };
      daemon.posts.push(sent);
      if (daemon.hold) await daemon.hold;
      if (sent.job !== null && daemon.gone.includes(sent.job)) {
        return reply(404, { error: `no walk ${sent.job}`, reason: 'no_such_job' });
      }
      const answer = sent.tail === 'start' ? daemon.start : daemon.answer;
      if (answer.down) throw new TypeError('down');
      return reply(answer.status, answer.body);
    }
    if (path === 'api/v1/devices') {
      daemon.reads.push(path);
      return reply(200, { devices: structuredClone(daemon.rows), profiles: [] });
    }
    const one = /^api\/v1\/devices\/([^/]+)\/(model|series)/.exec(path);
    if (one?.[2] === 'model') return reply(200, { device: modelOf(decodeURIComponent(one[1])) });
    if (one?.[2] === 'series') return reply(200, empty);
    return reply(200, {});
  };
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  p.window.setInterval = () => 1;
  p.window.clearInterval = () => {};
  const keyListeners = [];
  for (const target of [p.window, p.document]) {
    const add = target.addEventListener.bind(target);
    target.addEventListener = (kind, ...rest) => {
      if (/^key/.test(kind)) keyListeners.push(kind);
      return add(kind, ...rest);
    };
  }
  const pushes = [];
  const push = p.window.history.pushState.bind(p.window.history);
  p.window.history.pushState = (...args) => { pushes.push(args[2]); return push(...args); };
  const core = createCore({
    page: {
      v: 1, viewer: { signed_in: true, guest: false, writes: true, read_only: null },
      fleet: { devices: none ? [] : [{ url: 'A', name: 'A', link: link(true) }], hops: [] },
      shown: none ? null : 'A', device: none ? null : modelOf('A'), followed: {},
      card: none ? { kind: 'dim', title: 'No devices', detail: '', spans: [], faults: [], retry_in: null,
        try_now: false } : null,
      host: null, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  keyListeners.length = 0;
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 8; i++) await p.settle(); };
  const source = () => FakeSource.all.at(-1);
  const tell = async (event) => { source().send('walk', event); await calm(); };
  const at = () => p.document.activeElement?.id ?? null;
  const controls = () => $('popup-devices-walk-controls');
  const text = (id) => $(id)?.textContent.replace(/\s+/g, ' ').trim() ?? '';
  const open = async () => {
    $('knob').focus();
    core.router.open(DEVICES, ADD);
    await calm();
  };
  const choice = async (i) => {
    controls().querySelector(`[data-walk-choice="${i}"]`).click();
    await calm();
  };
  const click = async (id) => { $(id).click(); await calm(); };
  const posts = (tail) => daemon.posts.filter((one) => one.tail === tail);
  const submit = async (value) => {
    const field = $('popup-devices-walk-field');
    field.value = value;
    field.closest('form').requestSubmit();
    await calm();
  };
  return {
    ...p, core, daemon, reads: daemon.reads, $, calm, source, tell, at, controls, text, open, choice, click,
    posts, submit,
    pushes, keyListeners,
  };
}

/** A walk the page started, up to the warning (job 1). */
async function started(r) {
  await r.open();
  await r.tell({ job: 1, state: { kind: 'started' } });
  await r.tell(progressOf(1, 'linked', { can_go_back: false }));
}

await ok('the_add_page_is_the_last_entry_and_starts_a_walk_in_place', async () => {
  const r = wired();
  await r.open();
  const entries = [...r.document.querySelectorAll('.fp-entry')].map((e) => e.textContent.trim());
  assert.equal(entries.at(-1), '+ Add a device');
  assert.equal(r.document.querySelector('.fp-entry.on').dataset.popupPage, ADD);
  assert.equal(r.posts('start').length, 1);
  assert.deepEqual(r.posts('start')[0].body, {});
  assert.match(r.text('popup-devices-walk-status'), /Starting/);
  assert.deepEqual(r.keyListeners, [], 'no key listener of the page');
  // the popup wants the walk's states beside its follow, which a fleet event does not wipe
  assert.ok(FakeSource.all.some((s) => s.url.includes('needs=walk')), 'the stream asks for the walk');
  r.window.close();
});

await ok('the_states_the_daemon_causes_change_the_page_in_place_and_push_no_entry', async () => {
  const r = wired();
  await r.open();
  const pushed = r.pushes.length;
  await r.tell({ job: 1, state: { kind: 'started' } });
  await r.tell(progressOf(1, 'linked', { can_go_back: false }));
  assert.match(r.text('popup-devices-walk-head'), /A device is linked/);
  const cancel = r.controls().querySelector('[data-walk-choice="1"]');
  assert.equal(r.document.activeElement, cancel, 'the safe answer has the focus');
  await r.choice(0);
  assert.deepEqual(r.posts('answer')[0], { path: 'api/v1/walk/1/answer', job: 1, tail: 'answer',
    body: { choice: 0 } });
  // shut until the daemon's next state
  assert.equal(r.controls().querySelector('.fpw-controls').getAttribute('aria-busy'), 'true');
  await r.choice(0);
  assert.equal(r.posts('answer').length, 1, 'no second answer to one prompt');
  await r.tell(progressOf(1, 'scan'));
  assert.equal(r.controls().querySelector('.fpw-controls').getAttribute('aria-busy'), 'false');
  await r.tell(progressOf(1, 'scanning'));
  assert.equal(r.$('popup-devices-walk-rows').querySelectorAll('li').length, 2);
  assert.match(r.text('popup-devices-walk-status'), /Listening/);
  await r.tell(progressOf(1, 'pick'));
  assert.equal(r.$('popup-devices-walk-rows').textContent, '');
  assert.equal(r.pushes.length, pushed, 'a step the daemon causes pushes no entry');
  assert.equal(r.core.router.state().page, ADD);
  r.window.close();
});

await ok('the_answer_and_the_state_come_in_either_order', async () => {
  for (const order of ['answer first', 'state first']) {
    const r = wired();
    await started(r);
    let release;
    r.daemon.hold = new Promise((resolve) => { release = resolve; });
    r.controls().querySelector('[data-walk-choice="0"]').click();
    await r.calm();
    const shut = () => r.controls().querySelector('.fpw-controls').getAttribute('aria-busy');
    if (order === 'state first') {
      await r.tell(progressOf(1, 'scan'));
      release();
      await r.calm();
      assert.equal(shut(), 'false', order);
    } else {
      release();
      await r.calm();
      assert.equal(shut(), 'true', `${order}: still shut until the daemon's state`);
      await r.tell(progressOf(1, 'scan'));
      assert.equal(shut(), 'false', order);
    }
    assert.match(r.text('popup-devices-walk-head'), /Find the device/, order);
    assert.equal(r.posts('answer').length, 1, order);
    r.window.close();
  }
});

await ok('states_that_come_before_the_start_answers_are_taken_when_it_does', async () => {
  const r = wired();
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  await r.tell({ job: 1, state: { kind: 'started' } });
  await r.tell(progressOf(1, 'linked', { can_go_back: false }));
  assert.match(r.text('popup-devices-walk-status'), /Starting/, 'not the page\'s until the answer');
  release();
  await r.calm();
  assert.match(r.text('popup-devices-walk-head'), /A device is linked/);
  assert.equal(r.text('popup-devices-walk-status'), '');
  r.window.close();
});

await ok('a_job_the_daemon_no_longer_knows_is_said_once_and_never_retried', async () => {
  const r = wired();
  await started(r);
  r.daemon.gone.push(1);
  await r.choice(0);
  assert.equal(r.posts('answer').length, 1);
  assert.match(r.text('popup-devices-walk-refusal'), /no longer there/);
  assert.ok(r.$('popup-devices-walk-again'), 'the way to start again');
  await r.calm();
  await r.calm();
  assert.equal(r.posts('answer').length, 1, 'no retry loop');
  assert.equal(r.posts('start').length, 1);
  r.daemon.start = { status: 200, body: { job: 2 } };
  await r.click('popup-devices-walk-again');
  assert.equal(r.posts('start').length, 2);
  r.window.close();
});

await ok('a_walk_that_does_not_wait_is_worded_and_the_next_state_goes_on', async () => {
  const r = wired();
  await started(r);
  r.daemon.answer = { status: 409, body: { error: 'walk 1 is not waiting', reason: 'not_waiting' } };
  await r.choice(0);
  assert.match(r.text('popup-devices-walk-refusal'), /not waiting for that/);
  assert.equal(r.controls().querySelector('.fpw-controls').getAttribute('aria-busy'), 'false');
  await r.tell(progressOf(1, 'scan'));
  assert.equal(r.text('popup-devices-walk-refusal'), '', 'the next state clears it');
  r.window.close();
});

await ok('a_refusal_of_the_daemon_is_at_the_field_and_what_was_typed_stays', async () => {
  const r = wired();
  await started(r);
  await r.tell(progressOf(1, 'name'));
  const field = r.$('popup-devices-walk-field');
  assert.equal(r.document.activeElement, field);
  field.value = 'Desk';
  await r.tell(progressOf(1, 'name', { phase: 'working', working: 'testing' }));
  assert.equal(r.$('popup-devices-walk-field').value, 'Desk', 'typed text stays through a put');
  await r.tell(progressOf(1, 'name', { refusal: { kind: 'taken', message: 'Another device has that name.' } }));
  const now = r.$('popup-devices-walk-field');
  assert.equal(now.value, 'Desk');
  assert.equal(now.getAttribute('aria-invalid'), 'true');
  assert.equal(r.document.activeElement, now, 'the focus is on the field');
  assert.match(r.text('popup-devices-walk-refusal'), /Another device has that name/);
  await r.submit('');
  assert.deepEqual(r.posts('answer').at(-1).body, { text: '' }, 'an empty line is the default');
  r.window.close();
});

await ok('the_passphrase_is_a_secret_field_in_the_body_only_dropped_when_refused', async () => {
  const r = wired();
  await started(r);
  const secretOf = (over = {}) => progressOf(1, 'name', { prompt: PASSPHRASE_PROMPT, ...over });
  await r.tell(secretOf());
  const field = () => r.$('popup-devices-walk-field');
  assert.equal(field().type, 'password');
  assert.equal(field().getAttribute('value'), null);
  field().value = SECRET;
  await r.tell(secretOf({ phase: 'working', working: 'testing' }));
  assert.equal(field().value, SECRET, 'kept through a redraw');
  await r.click('popup-devices-walk-reveal');
  assert.equal(field().type, 'text');
  assert.equal(field().value, SECRET, 'and through the reveal');
  await r.click('popup-devices-walk-reveal');
  assert.equal(field().type, 'password');
  await r.tell(secretOf());
  assert.equal(field().value, SECRET);
  await r.submit(SECRET);
  assert.equal(r.posts('answer').at(-1).body.text, SECRET, 'the body carries it');
  assert.equal(field().value, '', 'cleared as it is sent');
  for (const place of [r.window.location.href, r.document.title, r.document.body.innerHTML]) {
    assert.ok(!place.includes(SECRET), 'in no address, title or markup');
  }
  field().value = SECRET;
  await r.tell(secretOf({ refusal: { kind: 'passphrase', message: 'The device turned that code away.' } }));
  assert.equal(field().value, '', 'a refused code is dropped');
  assert.match(r.text('popup-devices-walk-refusal'), /turned that code away/);
  assert.ok(!r.window.document.body.innerHTML.includes(SECRET));
  r.window.close();
});

await ok('a_walk_of_another_origin_is_shown_read_only_and_left_alone', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'a walk is already running (job 4, started by the terminal)',
    reason: 'busy' } };
  await r.open();
  assert.match(r.text('popup-devices-walk-status'), /Looking for where it stands/);
  await r.tell(progressOf(4, 'scan', { origin: 'socket' }));
  assert.match(r.text('popup-devices-walk-status'), /started from the terminal is running/);
  assert.ok(!/socket/.test(r.text('popup-devices-walk-status')), 'never the log word');
  assert.match(r.text('popup-devices-walk-status'), /Find the device/);
  assert.equal(r.controls().querySelectorAll('button').length, 0, 'nothing to press');
  r.core.router.step(null);
  await r.calm();
  assert.deepEqual(r.posts('cancel'), [], 'a walk only watched is not stopped');
  // the terminal's walk ended after the page saw it running
  const again = wired();
  again.daemon.start = r.daemon.start;
  await again.open();
  await again.tell(progressOf(4, 'scan', { origin: 'socket' }));
  await again.tell({ job: 4, state: { kind: 'cancelled' } });
  assert.match(again.text('popup-devices-walk-status'), /The walk was cancelled/);
  r.window.close();
  again.window.close();
});

await ok('a_web_walk_that_runs_is_taken_over_where_it_stands_after_a_reload', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  // this tab started walk 7 before the reload: its session store says so
  r.window.sessionStorage.setItem('svolder-walk-job', '7');
  await r.open();
  // the stream's replay: the walk's last state, from before the reload
  await r.tell(progressOf(7, 'name', { origin: 'web' }));
  assert.match(r.text('popup-devices-walk-head'), /What to call it/);
  assert.ok(r.$('popup-devices-walk-field'), 'answerable');
  await r.submit('Desk');
  assert.deepEqual(r.posts('answer')[0].body, { text: 'Desk' });
  assert.equal(r.posts('answer')[0].job, 7);
  r.window.close();
});

await ok('a_daemon_restart_forgets_the_old_runs_walks_though_a_new_walk_has_the_same_job_id', async () => {
  const r = wired();
  r.daemon.start = { status: 200, body: { job: 1, run: 100 } };
  await r.open();
  await r.tell({ run: 100, ...progressOf(1, 'name', { origin: 'web' }) });
  assert.ok(r.$('popup-devices-walk-field'), 'the page drives its walk 1');
  // the daemon restarted: a walk 1 of the terminal in the new run
  await r.tell({ run: 200, ...progressOf(1, 'scan', { origin: 'socket' }) });
  assert.equal(r.$('popup-devices-walk-field'), null, 'the old walk 1 is not answerable any more');
  assert.equal(r.controls().querySelectorAll('[data-walk-choice]').length, 0, 'no answer to give');
  assert.ok(!/started from the terminal/.test(r.text('popup-devices-walk-status')),
    'the new walk 1 is not taken for the old one');
  assert.equal(r.posts('answer').length, 0);
  r.core.router.step(null);
  await r.calm();
  assert.deepEqual(r.posts('cancel'), [], 'the new run\'s walk 1 is not stopped as the page\'s own');
  r.window.close();
});

await ok('a_job_remembered_through_a_reload_belongs_to_its_run', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  // the stored word is `run:job`; a walk 7 of run 200 is not run 100's (the restart case above is the
  // behaviour's own red)
  r.window.sessionStorage.setItem('svolder-walk-job', '100:7');
  await r.open();
  await r.tell({ run: 200, ...progressOf(7, 'name', { origin: 'web' }) });
  assert.equal(r.$('popup-devices-walk-field'), null, 'a walk 7 of another run is not the tab\'s');
  r.window.close();
  const same = wired();
  same.daemon.start = r.daemon.start;
  same.window.sessionStorage.setItem('svolder-walk-job', '100:7');
  await same.open();
  await same.tell({ run: 100, ...progressOf(7, 'name', { origin: 'web' }) });
  assert.ok(same.$('popup-devices-walk-field'), 'the same run: taken up where it stands');
  same.window.close();
});

await ok('a_busy_start_with_no_walk_seen_says_so_and_starts_again_when_the_stream_is_back', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  // the stream is lost while the start is on its way
  r.core.model.setState('lost');
  r.daemon.hold = null;
  release();
  await r.calm();
  assert.match(r.text('popup-devices-walk-status'), /not reached this page/);
  assert.equal(r.text('popup-devices-walk-refusal'), '', 'nothing failed: no refusal line');
  assert.equal(r.controls().querySelectorAll('button').length, 0);
  assert.equal(r.posts('start').length, 1);
  // the stream is found again; the walk that held the daemon has ended
  r.daemon.start = { status: 200, body: { job: 3 } };
  r.core.model.emit('opened', null);
  await r.calm();
  assert.equal(r.posts('start').length, 2, 'the start is made again');
  await r.tell({ job: 3, state: { kind: 'started' } });
  await r.tell(progressOf(3, 'linked', { can_go_back: false }));
  assert.ok(!/not reached this page/.test(r.text('popup-devices-walk-status')), 'the line is gone');
  assert.ok(r.controls().querySelectorAll('button').length > 0, 'the page drives walk 3');
  r.window.close();
});

await ok('a_busy_start_with_no_walk_seen_follows_the_walk_the_stream_then_tells', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  await r.open();
  await r.tell(progressOf(4, 'scan', { origin: 'socket' }));
  assert.ok(!/not reached this page/.test(r.text('popup-devices-walk-status')), 'the line is gone');
  assert.match(r.text('popup-devices-walk-status'), /started from the terminal is running/);
  r.window.close();
});

await ok('answer_back_and_cancel_name_the_run_the_page_was_told_in', async () => {
  const r = wired();
  r.daemon.start = { status: 200, body: { job: 1, run: 100 } };
  await r.open();
  await r.tell({ run: 100, job: 1, state: { kind: 'started' } });
  await r.tell({ run: 100, ...progressOf(1, 'linked', { can_go_back: true }) });
  await r.choice(0);
  assert.deepEqual(r.posts('answer')[0].body, { choice: 0, run: 100 });
  await r.tell({ run: 100, ...progressOf(1, 'scan', { can_go_back: true }) });
  await r.click('popup-devices-walk-back');
  assert.deepEqual(r.posts('back')[0].body, { run: 100 });
  // the daemon restarted and nothing of the new run has been told yet: leaving still names run 100
  r.core.router.step(null);
  await r.calm();
  assert.deepEqual(r.posts('cancel').map((c) => [c.job, c.body]), [[1, { run: 100 }]]);
  r.window.close();
});

await ok('walks_that_ended_before_the_page_saw_them_are_no_notice', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  await r.open();
  const before = r.$('popup-devices-walk').outerHTML;
  await r.tell({ job: 2, state: { kind: 'done', payload: { name: 'Old', unit: 'X', passphrase: 'auto' } } });
  await r.tell({ job: 3, state: { kind: 'cancelled' } });
  await r.tell({ job: 5, state: { kind: 'failed', message: 'it failed', reason: 'internal' } });
  assert.equal(r.$('popup-devices-walk').outerHTML, before, 'nothing is drawn of them');
  r.window.close();
});

await ok('leaving_the_page_stops_the_walk_it_drives_by_its_job', async () => {
  const exits = [
    ['another entry', (r) => r.core.router.step('overview')],
    ['back to the list', (r) => r.core.router.step(null)],
    ['the popup closed', (r) => r.core.router.close()],
  ];
  for (const [name, leave] of exits) {
    const r = wired();
    await started(r);
    await r.tell(progressOf(1, 'scan'));
    leave(r);
    await r.calm();
    assert.deepEqual(r.posts('cancel').map((c) => [c.job, c.body]), [[1, {}]], name);
    r.window.close();
  }
  // a walk that ended is not stopped again
  const r = wired();
  await started(r);
  await r.tell({ job: 1, state: { kind: 'cancelled' } });
  r.core.router.close();
  await r.calm();
  assert.deepEqual(r.posts('cancel'), []);
  r.window.close();
});

await ok('a_start_that_answers_after_the_page_was_left_is_stopped', async () => {
  const r = wired();
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  r.core.router.step('overview');
  await r.calm();
  r.daemon.hold = null;
  release();
  await r.calm();
  assert.deepEqual(r.posts('cancel').map((c) => c.job), [1]);
  r.window.close();
});

await ok('stop_and_back_name_their_job', async () => {
  const r = wired();
  await started(r);
  await r.tell(progressOf(1, 'pick'));
  await r.click(BACK_ID);
  assert.deepEqual(r.posts('back').map((c) => c.job), [1]);
  await r.tell(progressOf(1, 'scan'));
  await r.tell(progressOf(1, 'pick'));
  await r.click(STOP_ID);
  assert.deepEqual(r.posts('cancel').map((c) => c.job), [1]);
  await r.tell({ job: 1, state: { kind: 'cancelled' } });
  assert.match(r.text('popup-devices-walk-status'), /The walk was cancelled/);
  assert.deepEqual(r.posts('cancel').length, 1, 'leaving after the end sends nothing more');
  r.core.router.close();
  await r.calm();
  assert.equal(r.posts('cancel').length, 1);
  r.window.close();
});

await ok('done_names_the_device_and_its_link_and_open_its_page_steps_in_place', async () => {
  const r = wired();
  await started(r);
  await r.tell(progressOf(1, 'writing'));
  r.daemon.rows.push({ unit: 'UNIT-B', name: 'Bifrost 3', passphrase: 'auto', connected: true });
  await r.tell({ job: 1, state: FIXTURE.done.state });
  assert.match(r.text('popup-devices-walk-status'), /Bifrost 3 was added/);
  // the popup read its list again: the side list gains the entry, the done card its link
  assert.ok([...r.document.querySelectorAll('.fp-entry')].some((e) => e.textContent.includes('Bifrost 3')));
  assert.match(r.text('popup-devices-walk-status'), /It is connected/);
  assert.ok(r.$('popup-devices-walk-open'));
  assert.equal(r.core.router.state().page, ADD);
  const pushed = r.pushes.length;
  await r.click('popup-devices-walk-open');
  assert.equal(r.core.router.state().page, 'd-Bifrost%203');
  assert.equal(r.pushes.length, pushed, 'a step from a page changes the page in place');
  assert.deepEqual(r.posts('cancel'), [], 'nothing is cancelled after done');
  r.window.close();
});

await ok('the_first_device_of_a_fresh_install_is_shown_without_a_push', async () => {
  const r = wired({ none: true });
  assert.equal(r.core.model.shown(), null);
  await r.open();
  await r.tell({ job: 1, state: { kind: 'started' } });
  await r.tell(progressOf(1, 'writing'));
  r.daemon.rows.push({ unit: 'UNIT-B', name: 'Bifrost 3', passphrase: 'auto', connected: true });
  const pushed = r.pushes.length;
  await r.tell({ job: 1, state: FIXTURE.done.state });
  // the fleet lists it a moment later
  r.source().send('fleet', { devices: [{ url: 'Bifrost%203', name: 'Bifrost 3', link: link(true) }], hops: [] });
  await r.calm();
  assert.equal(r.core.model.shown(), 'Bifrost%203', 'the page behind shows the new device');
  assert.equal(r.pushes.length, pushed, 'no entry is pushed');
  assert.match(r.text('popup-devices-walk-status'), /Bifrost 3 was added/);
  r.window.close();
});

await ok('the_popup_wants_the_walks_states_beside_its_follow', async () => {
  const r = wired();
  await r.open();
  r.source().send('fleet', { devices: [{ url: 'A', name: 'A', link: link(true) }], hops: [] });
  await r.calm();
  assert.ok(r.core.stream.needed().includes('walk'), 'a fleet event does not wipe it');
  assert.ok(OWNER !== WALK_OWNER);
  r.core.router.close();
  await r.calm();
  assert.ok(!r.core.stream.needed().includes('walk'), 'and the close gives it back');
  r.window.close();
});


// ---- review fixes (4.4 fix 1) ----

await ok('typed_text_goes_back_only_into_the_same_field_of_the_same_step', async () => {
  // a code typed at the passphrase step is not carried into the name step by Back
  const a = wired();
  await started(a);
  await a.tell(progressOf(1, 'name', { prompt: PASSPHRASE_PROMPT, can_go_back: true }));
  a.$('popup-devices-walk-field').value = '4821';
  await a.click(BACK_ID);
  await a.tell(progressOf(1, 'name'));
  const name = a.$('popup-devices-walk-field');
  assert.equal(name.type, 'text');
  assert.equal(name.value, '', 'the code is not in the name field');
  assert.ok(!a.document.body.innerHTML.includes('4821'));
  a.window.close();
  // a name typed at the name step is not carried into the secret field
  const b = wired();
  await started(b);
  await b.tell(progressOf(1, 'name'));
  b.$('popup-devices-walk-field').value = 'Desk DAC';
  await b.tell(progressOf(1, 'name', { prompt: PASSPHRASE_PROMPT }));
  assert.equal(b.$('popup-devices-walk-field').type, 'password');
  assert.equal(b.$('popup-devices-walk-field').value, '', 'the name is not in the secret field');
  b.window.close();
});

await ok('leaving_before_the_first_state_stops_the_walk_this_page_started', async () => {
  const a = wired();
  await a.open();
  a.core.router.step(null);
  await a.calm();
  assert.deepEqual(a.posts('cancel').map((c) => c.job), [1], 'no state had come');
  a.window.close();
  const b = wired();
  await b.open();
  await b.tell({ job: 1, state: { kind: 'started' } });
  b.core.router.close();
  await b.calm();
  assert.deepEqual(b.posts('cancel').map((c) => c.job), [1], 'only started had come');
  b.window.close();
});

await ok('a_page_never_takes_over_or_stops_another_tabs_walk', async () => {
  const r = wired();
  await started(r);
  await r.tell({ job: 1, state: { kind: 'cancelled' } });
  const card = r.text('popup-devices-walk-controls');
  // another tab starts walk 9: this page stays on its card
  await r.tell(progressOf(9, 'name', { origin: 'web' }));
  assert.equal(r.text('popup-devices-walk-controls'), card, 'the ended page is left on its card');
  assert.ok(!r.$('popup-devices-walk-field'));
  r.core.router.close();
  await r.calm();
  assert.deepEqual(r.posts('cancel'), [], 'a walk it did not start is never stopped');
  r.window.close();
  // a page that asked and got busy shows a web walk it did not start read-only (no memory of it)
  const j = wired();
  j.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  await j.open();
  await j.tell(progressOf(7, 'name', { origin: 'web' }));
  assert.ok(!j.$('popup-devices-walk-field'), 'not this tab\'s walk: read-only');
  assert.match(j.text('popup-devices-walk-status'), /started in another browser tab is running/);
  j.core.router.close();
  await j.calm();
  assert.deepEqual(j.posts('cancel'), []);
  j.window.close();
});

await ok('the_status_and_the_refusal_are_stable_live_regions_and_the_outcome_is_read_there', async () => {
  const r = wired();
  await started(r);
  const status = r.$('popup-devices-walk-status');
  const refusal = r.$('popup-devices-walk-refusal');
  for (const place of [status, refusal]) {
    assert.equal(place.getAttribute('role'), 'status');
    assert.equal(place.getAttribute('aria-atomic'), 'true');
  }
  await r.tell(progressOf(1, 'scanning'));
  await r.tell(progressOf(1, 'pick', { refusal: { kind: 'answer', message: 'Pick one.' } }));
  assert.equal(r.$('popup-devices-walk-status'), status, 'the same element through the states');
  assert.equal(r.$('popup-devices-walk-refusal'), refusal);
  assert.match(refusal.textContent, /Pick one/);
  assert.equal(status.querySelector('[role]'), null, 'no live role is created inside it');
  // the outcome is in the status line
  r.daemon.rows.push({ unit: 'UNIT-B', name: 'Bifrost 3', passphrase: 'auto', connected: true });
  await r.tell({ job: 1, state: FIXTURE.done.state });
  assert.match(status.textContent, /Bifrost 3 was added/);
  assert.ok(!r.$('popup-devices-walk-controls').textContent.includes('was added'), 'buttons only');
  r.window.close();
  for (const [kind, words] of [['cancelled', /cancelled/], ['failed', /it broke/]]) {
    const x = wired();
    await started(x);
    await x.tell({ job: 1, state: kind === 'failed' ? { kind, message: 'it broke', reason: 'internal' }
      : { kind } });
    assert.match(x.$('popup-devices-walk-status').textContent, words, kind);
    x.window.close();
  }
});

await ok('the_warning_body_is_tied_to_its_buttons_and_drawn_as_a_warning', async () => {
  const r = wired();
  await started(r);
  const group = r.controls().querySelector('[role=group]');
  const body = r.$(group.getAttribute('aria-describedby'));
  assert.ok(body && body.textContent.includes('have a link'), 'the body names the device it disturbs');
  assert.ok(r.$('popup-devices-walk-head').querySelector('.fpw-warning'));
  await r.tell(progressOf(1, 'scan'));
  assert.ok(!r.$('popup-devices-walk-head').querySelector('.fpw-warning'));
  r.window.close();
});

await ok('a_replayed_state_changes_nothing', async () => {
  // a done state that is replayed does not read the list again
  const d = wired();
  await started(d);
  await d.tell(progressOf(1, 'writing'));
  const reads = () => d.reads.length;
  await d.tell({ job: 1, state: FIXTURE.done.state });
  const first = reads();
  await d.tell({ job: 1, state: FIXTURE.done.state });
  assert.equal(reads(), first, 'no second read of the list');
  d.window.close();
});


// ---- review fixes (4.4 fix 2) ----

await ok('a_walk_that_ends_before_its_start_answers_shows_its_end', async () => {
  const r = wired();
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  await r.tell({ job: 1, state: { kind: 'started' } });
  await r.tell(progressOf(1, 'linked', { can_go_back: false }));
  await r.tell({ job: 1, state: { kind: 'cancelled' } });
  release();
  await r.calm();
  assert.match(r.text('popup-devices-walk-status'), /The walk was cancelled/);
  assert.equal(r.$('popup-devices-walk').dataset.mode, 'ended');
  assert.ok(r.$('popup-devices-walk-again'));
  r.window.close();
});

await ok('stop_then_leaving_sends_one_cancel', async () => {
  const r = wired();
  await started(r);
  await r.click(STOP_ID);
  r.core.router.close();
  await r.calm();
  assert.deepEqual(r.posts('cancel').map((c) => c.job), [1]);
  r.window.close();
});

await ok('a_state_told_again_after_an_answer_opens_the_controls', async () => {
  const r = wired();
  await started(r);
  await r.tell(progressOf(1, 'pick', { can_go_back: true }));
  await r.click(BACK_ID);
  assert.equal(r.controls().querySelector('.fpw-controls').getAttribute('aria-busy'), 'true');
  // the daemon refused the Back and sends the same state again
  await r.tell(progressOf(1, 'pick', { can_go_back: true }));
  assert.equal(r.controls().querySelector('.fpw-controls').getAttribute('aria-busy'), 'false');
  r.window.close();
});

await ok('another_tabs_walk_is_said_to_be_in_another_browser_tab', async () => {
  const r = wired();
  r.daemon.start = { status: 409, body: { error: 'busy', reason: 'busy' } };
  await r.open();
  await r.tell(progressOf(7, 'name', { origin: 'web' }));
  assert.match(r.text('popup-devices-walk-status'), /started in another browser tab is running/);
  assert.ok(!/started from web/.test(r.text('popup-devices-walk-status')));
  r.window.close();
});

console.log(`core-device-walk: ${n} cases passed`);
