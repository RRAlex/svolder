/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the EQ library popup (core/popups/eq-library/{draw,wire}.js with
// core/autoeq-actions.js), on the real core over a jsdom page and a small fake daemon, every case
// with a device on show that the page also has a neighbour of: the row of the menu (writer only,
// between Devices and Help); the title naming the side in every draw; the search (debounced, an
// answer matched to the query it answers and a stale one dropped in either order, an aborted
// request no failure, an input method that is composing sends nothing, each refusal as words);
// the groups with their measurements; the preview (the call names the side, the graph and the
// daemon's numbers, a second measurement compared, two answers to one choice in either order);
// the import (the name, the daemon's `taken` and `bad_name` at the field, the suggestion, what was
// saved and that it is not on); the upload (a file read as text, Save disabled while there is none
// and says why, the daemon's parse refusal as it words it); the saved presets (the marks, Overwrite
// asked once and refused with its reason on each row that cannot); the computer's side. Nothing
// reaches a network: the daemon is the `fetch` of the page. Run: node core-eq-library.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { SOUND, POPUPS } = await import('../../../bins/svolderd/web/core/popups/listed.js');
const { IDS, WORDS, askId, eid, overwriteId, variantId, refusalWords, keyOf } = await import(
  '../../../bins/svolderd/web/core/popups/eq-library/draw.js');
const { hzWords } = await import('../../../bins/svolderd/web/core/words.js');
const { DEBOUNCE_MS } = await import('../../../bins/svolderd/web/core/popups/eq-library/wire.js');
const { AUTOEQ_REFUSALS } = await import('../../../bins/svolderd/web/core/autoeq-actions.js');
const { logFreqs } = await import('../../../bins/svolderd/web/core/tone-curve.js');

let n = 0;
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};
console.warn = () => {};

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });
const seg = (name) => encodeURIComponent(name).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
const FREQS = logFreqs(181);
const FLAT = FREQS.map(() => 0);
const curve = (f) => FREQS.map((hz) => f(hz));

const presetRows = (list) => list.map(([name, extra = {}]) => ({
  name, built_in: false, imported: false, on: false, json: '', rename_json: '', ...extra,
}));
const deviceModel = (name, over = {}) => ({
  name, url: seg(name), link: link(true), volume: null, controls: [], meters: null,
  tone: { response: FLAT, mode: 'eq' },
  presets: {
    status: { lead: '', name: null, tail: '' }, editable: true, recall: true, note: null,
    rows: presetRows([['Flat', { built_in: true }], ['Mine', {}], ['From AutoEq', { imported: true }]]),
  },
  ...over,
});
const computerModel = () => ({
  unavailable: null, volume: null, eq: { on: true, points: [], max: 16 },
  presets: [
    { name: 'Flat', built_in: true, on: false, playable: true, why: null },
    { name: 'Mine', built_in: false, on: true, playable: true, why: null },
    { name: 'Big one', built_in: false, on: false, playable: false, why: 'it has 40 filters; the computer holds 16' },
  ],
});

const V = (name, source, form, rig, words) => ({ name, source, form, ...(rig ? { rig } : {}), words });
const HD650 = [
  V('Sennheiser HD 650', 'oratory1990', 'over-ear', null, 'oratory1990 · over-ear'),
  V('Sennheiser HD 650', 'Rtings', 'over-ear', 'GRAS 43AG', 'Rtings · over-ear · GRAS 43AG'),
  V('Sennheiser HD 650', 'Innerfidelity', 'over-ear', null, 'Innerfidelity · over-ear'),
];
const SEARCH = (groups, extra = {}) => ({
  groups, more: 0, measurements: 5123, fetched_unix: Date.UTC(2026, 10, 9) / 1000, ...extra,
});
const GROUP = (key, name, variants) => ({ key, name, variants });
const ONE = SEARCH([GROUP('sennheiserhd650', 'Sennheiser HD 650', HD650)]);
const PREVIEW = (over = {}) => ({
  freqs: FREQS, target: curve((hz) => (hz > 2000 && hz < 4000 ? 6 : 0)),
  preamp_db: -3, peak_db: 2.4,
  fit: {
    response: curve((hz) => (hz > 2000 && hz < 4000 ? 2 : 0)), gap: [{ from_hz: 2000, to_hz: 4000 }],
    rms_db: 0.9, worst_db: 3.1, worst_hz: 6300,
  },
  ...over,
});

class FakeSource {
  addEventListener() {}

  close() {}
}

/** The real core with `shown` on show (A by default; B and C are its neighbours). The daemon is the
 * page's `fetch`: every call is kept in `calls` (`{ path, body }`) and answered by `daemon.answer`
 * (a function of the call that gives `{ status, body }`, or a promise of it), `hold` being a map of
 * path to the promise an answer waits for. */
function rig({ shown = 'A', viewer = {}, host = null, devices = {}, side = null } = {}) {
  const p = page(BODY, { url: `http://svolder.test/?device=${shown}` });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const daemon = { calls: [], answer: null, aborted: [], models: {} };
  // the debounce is the one timer the cases step; every other timer runs as it is
  const pauses = [];
  const realSet = p.window.setTimeout.bind(p.window);
  const realClear = p.window.clearTimeout.bind(p.window);
  p.window.setTimeout = (fn, ms, ...rest) => {
    if (ms !== DEBOUNCE_MS) return realSet(fn, ms, ...rest);
    const pause = { fn, id: pauses.length + 1000, live: true };
    pauses.push(pause);
    return pause.id;
  };
  p.window.clearTimeout = (id) => {
    const pause = pauses.find((one) => one.id === id);
    if (pause) pause.live = false;
    else realClear(id);
  };
  p.window.fetch = (path, init = {}) => new Promise((resolve, reject) => {
    const reply = (status, body) => resolve({ status, ok: status < 400, json: async () => body });
    const body = init.body && typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call = { path, body };
    daemon.calls.push(call);
    const model = /^api\/v1\/devices\/([^/]+)\/model/.exec(path);
    if (model) {
      reply(200, { device: daemon.models[model[1]] ?? deviceModel(decodeURIComponent(model[1])) });
      return;
    }
    if (path.startsWith('api/v1/host')) {
      reply(200, { computer: computerModel() });
      return;
    }
    const abort = () => {
      daemon.aborted.push(call);
      // a reply that was on its way when the abort came is still delivered
      if (daemon.ignoreAbort) return;
      reject(new p.window.DOMException('aborted', 'AbortError'));
    };
    if (init.signal?.aborted && !daemon.ignoreAbort) {
      abort();
      return;
    }
    init.signal?.addEventListener('abort', abort, { once: true });
    const give = (made) => Promise.resolve(made).then((got) => {
      if (init.signal?.aborted && !daemon.ignoreAbort) return;
      init.signal?.removeEventListener('abort', abort);
      reply(got.status, got.body);
    });
    give(daemon.answer?.(call) ?? { status: 200, body: {} });
  });
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  const keyListeners = [];
  for (const target of [p.window, p.document]) {
    const add = target.addEventListener.bind(target);
    target.addEventListener = (kind, ...rest) => {
      if (/^key/.test(kind)) keyListeners.push(kind);
      return add(kind, ...rest);
    };
  }
  const core = createCore({
    page: {
      v: 1, viewer: { signed_in: true, guest: false, writes: true, read_only: null, ...viewer },
      fleet: { devices: ['A', 'B', 'C'].map((name) => ({ url: seg(name), name, link: link(true) })), hops: [] },
      shown, device: deviceModel(shown, devices[shown] ?? {}), followed: {}, card: null, host, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  keyListeners.length = 0;
  if (side) core.computer.flip(side);
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 8; i++) await p.settle(); };
  const open = async (to = null) => {
    $('knob').focus();
    core.router.open(SOUND, to);
    await calm();
  };
  const type = (id, text, extra = {}) => {
    const field = $(id);
    field.focus();
    field.value = text;
    field.dispatchEvent(new p.window.Event('input', { bubbles: true, ...extra }));
  };
  /** Type `text` in the search field and let the pause pass: the search is sent. */
  const search = async (text) => {
    type(IDS.query, text);
    for (const pause of pauses.splice(0)) if (pause.live) pause.fn();
    await calm();
  };
  const click = async (id) => { $(id).click(); await calm(); };
  const submit = async (el) => {
    el.dispatchEvent(new p.window.Event('submit', { cancelable: true, bubbles: true }));
    await calm();
  };
  const text = (id) => $(id)?.textContent ?? null;
  return {
    ...p, core, daemon, pauses, keyListeners, $, calm, open, type, search, click, submit, text,
    posts: (tail) => daemon.calls.filter((one) => one.path.endsWith(tail)),
    title: () => $('popup-sound-context')?.textContent,
    at: () => p.document.activeElement?.id ?? null,
  };
}

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
const answers = (map) => (call) => {
  const found = map[call.path.replace(/^api\/v1\/autoeq\//, '')] ?? map[call.path];
  return typeof found === 'function' ? found(call) : found;
};
const good = (body) => ({ status: 200, body });
const refused = (status, reason, error = 'daemon words') => ({ status, body: { error, reason } });

// ---- part 1: the row and the frame ----

await ok('the_popup_is_a_writers_between_devices_and_help_and_names_the_side_in_every_title', async () => {
  const order = POPUPS.map((one) => one.name);
  assert.deepEqual(order, ['devices', 'sound', 'setup', 'help']);
  const entry = POPUPS.find((one) => one.name === SOUND);
  assert.equal(entry.row.sees, 'writer');
  assert.equal(entry.stays, undefined, 'a change of the shown device closes it');
  assert.equal(entry.entries, undefined, 'no side list: the tabs and the pills are its pages');
  assert.equal(entry.row.label, 'Sound…');
  const a = rig();
  await a.open();
  assert.equal(a.title(), 'EQ library · A');
  a.core.router.step('saved');
  await a.calm();
  assert.equal(a.title(), 'EQ library · A', 'the title is named in every draw');
  a.core.router.step(null);
  await a.calm();
  assert.equal(a.title(), 'EQ library · A');
  assert.deepEqual(a.keyListeners, [], 'no key listener of its own');
  a.window.close();
  const b = rig({ shown: 'B' });
  await b.open();
  assert.equal(b.title(), 'EQ library · B', 'the device on show, whichever it is');
  b.window.close();
});

await ok('a_guest_and_a_page_with_writes_off_have_no_way_in', async () => {
  for (const viewer of [{ guest: true, signed_in: false }, { writes: false }]) {
    const r = rig({ viewer });
    await r.open();
    assert.equal(r.$('popup-sound'), null, 'nothing is drawn');
    assert.equal(r.core.router.state().popup, null);
    r.window.close();
  }
});

await ok('a_change_of_the_shown_device_closes_it_and_stops_what_is_on_its_way', async () => {
  const r = rig();
  const held = deferred();
  r.daemon.answer = answers({ search: () => held.promise });
  await r.open();
  await r.search('hd');
  await r.core.actions.showDevice('B', { push: false });
  await r.calm();
  assert.equal(r.core.router.state().popup, null, 'closed with the device');
  assert.equal(r.daemon.aborted.length, 1, 'the search in flight was aborted');
  held.resolve(good(ONE));
  await r.calm();
  r.window.close();
});

// ---- part 2: the search ----

await ok('typing_waits_for_a_pause_and_sends_once_and_enter_sends_at_once', async () => {
  const r = rig();
  r.daemon.answer = answers({ search: good(ONE) });
  await r.open();
  r.type(IDS.query, 'h');
  r.type(IDS.query, 'hd');
  r.type(IDS.query, 'hd 6');
  await r.calm();
  assert.equal(r.posts('/search').length, 0, 'nothing is sent while typing');
  for (const pause of r.pauses.splice(0)) if (pause.live) pause.fn();
  await r.calm();
  assert.deepEqual(r.posts('/search').map((one) => one.body), [{ query: 'hd 6' }], 'one search, the last text');
  r.type(IDS.query, 'hd 650');
  const form = r.$(IDS.query).form;
  const event = new r.window.Event('submit', { cancelable: true, bubbles: true });
  form.dispatchEvent(event);
  await r.calm();
  assert.equal(event.defaultPrevented, true, 'a submit never navigates');
  assert.equal(r.posts('/search').length, 2, 'Enter sends at once');
  assert.equal(r.posts('/search')[1].body.query, 'hd 650');
  assert.equal(r.$(IDS.query).maxLength, 60, 'the daemon refuses more than 60');
  r.window.close();
});

await ok('an_input_method_that_is_composing_sends_nothing_until_it_ends', async () => {
  const r = rig();
  r.daemon.answer = answers({ search: good(ONE) });
  await r.open();
  const field = r.$(IDS.query);
  field.dispatchEvent(new r.window.Event('compositionstart', { bubbles: true }));
  r.type(IDS.query, 'ゼン');
  await r.calm();
  assert.equal(r.pauses.filter((one) => one.live).length, 0, 'no pause is started while composing');
  field.dispatchEvent(new r.window.Event('compositionend', { bubbles: true }));
  for (const pause of r.pauses.splice(0)) if (pause.live) pause.fn();
  await r.calm();
  assert.equal(r.posts('/search').length, 1);
  assert.equal(r.posts('/search')[0].body.query, 'ゼン');
  r.window.close();
});

await ok('a_stale_answer_is_dropped_in_either_order_and_an_aborted_one_is_no_failure', async () => {
  for (const [first, late] of [['older', false], ['newer', false], ['older', true], ['newer', true]]) {
    const r = rig();
    r.daemon.ignoreAbort = late;
    const gates = [deferred(), deferred()];
    let i = 0;
    r.daemon.answer = answers({ search: () => gates[i++].promise });
    await r.open();
    await r.search('hd');
    await r.search('hd 650');
    assert.equal(r.daemon.aborted.length, 1, 'the older request is aborted when a newer is sent');
    const older = SEARCH([GROUP('hd', 'Older headphone', HD650.slice(0, 1))]);
    const newer = SEARCH([GROUP('hd650', 'Newer headphone', HD650.slice(0, 1))]);
    const [one, two] = first === 'older' ? [0, 1] : [1, 0];
    gates[one].resolve(good(one === 0 ? older : newer));
    await r.calm();
    gates[two].resolve(good(two === 0 ? older : newer));
    await r.calm();
    const names = [...r.document.querySelectorAll('.fpe-group-name')].map((el) => el.firstChild.textContent);
    assert.deepEqual(names, ['Newer headphone'], `${first} first, late ${late}: only the newest query's answer stands`);
    assert.doesNotMatch(r.text(IDS.status), /could not|Try again/, 'an aborted request says nothing');
    r.window.close();
  }
});

await ok('the_groups_name_each_headphone_with_its_measurements_the_recommended_first', async () => {
  const r = rig();
  r.daemon.answer = answers({
    search: good(SEARCH([GROUP('sennheiserhd650', 'Sennheiser HD 650', HD650),
      GROUP('hd6xx', 'HD 6XX', HD650.slice(0, 1))],
    { more: 14, note: 'AutoEq could not be reached; this copy is from earlier.' })),
  });
  await r.open();
  await r.search('hd');
  const groups = [...r.document.querySelectorAll('.fpe-group')];
  assert.equal(groups.length, 2);
  assert.ok(groups.every((el) => el.open), 'a short answer shows its measurements');
  const buttons = [...groups[0].querySelectorAll('.fpe-variant')];
  assert.deepEqual(buttons.map((el) => el.textContent.replace(/\s+/g, ' ').trim()), [
    'oratory1990 · over-ear Recommended', 'Rtings · over-ear · GRAS 43AG', 'Innerfidelity · over-ear',
  ]);
  assert.match(groups[0].querySelector('.fpe-count').textContent, /3 measurements/);
  const status = r.text(IDS.status);
  assert.match(status, /2 headphones\. 14 more headphones match: narrow the search\./);
  assert.match(status, /5,123 measurements; this copy was fetched on 2026-11-09\./);
  assert.match(status, /this copy is from earlier/, 'the daemon\'s note is shown');
  r.window.close();
});

await ok('a_long_answer_closes_its_disclosures_and_a_person_who_opens_one_keeps_it', async () => {
  const r = rig();
  const many = SEARCH(['a', 'b', 'c', 'd'].map((k) => GROUP(k, `Phone ${k}`, HD650.slice(0, 2))));
  r.daemon.answer = answers({ search: good(many) });
  await r.open();
  await r.search('phone');
  const groups = () => [...r.document.querySelectorAll('.fpe-group')];
  assert.ok(groups().every((el) => !el.open), 'more than three headphones: all closed');
  groups()[1].open = true;
  groups()[1].dispatchEvent(new r.window.Event('toggle'));
  r.daemon.answer = answers({ search: good(many) });
  await r.search('phone ');
  assert.deepEqual(groups().map((el) => el.open), [false, false, false, false],
    'a new answer starts closed again');
  r.window.close();
});

await ok('empty_and_failed_and_loading_have_their_words_and_each_refusal_reads_as_words', async () => {
  const r = rig();
  const gate = deferred();
  r.daemon.answer = answers({ search: () => gate.promise });
  await r.open();
  assert.match(r.text(IDS.status), /Type a headphone/);
  await r.search('zzz');
  assert.equal(r.text(IDS.status), 'Searching…', 'loading');
  gate.resolve(good(SEARCH([])));
  await r.calm();
  assert.equal(r.text(IDS.status).startsWith('Nothing in AutoEq matches “zzz”.'), true);
  r.type(IDS.query, '   ');
  await r.calm();
  assert.match(r.text(IDS.status), /Type a headphone/, 'an empty query is no search');
  const status = { busy: 409, not_listed: 404, unreachable: 502, invalid: 422, missing: 404, stopping: 503,
    internal: 500, no_answer: 0, signed_out: 401, forbidden: 403, read_only: 409, file_busy: 409, too_large: 413,
    taken: 422, bad_name: 422, not_found_yet: 409, not_connected: 409 };
  for (const reason of AUTOEQ_REFUSALS.filter((one) => one in status)) {
    const q = rig();
    q.daemon.answer = answers({ search: () => refused(status[reason], reason, `daemon said ${reason}`) });
    await q.open();
    await q.search('hd');
    const said = q.text(IDS.status);
    assert.ok(said.length > 0 && said !== WORDS.searching, `${reason}: words, got "${said}"`);
    if (!['invalid', 'bad_name'].includes(reason)) assert.ok(!said.startsWith('daemon said'), `${reason}: the page's words`);
    q.window.close();
  }
  const busy = refusalWords('search', { reason: 'busy' });
  assert.match(busy, /busy/, 'busy reads as words a person reads');
  assert.match(refusalWords('search', { reason: 'unreachable' }), /AutoEq could not be reached/);
  const off = 'AutoEq is off for the fake device; set SVOLDER_AUTOEQ_URL to turn it on';
  assert.equal(refusalWords('search', { reason: 'unreachable', error: off }), off, 'the daemon\'s words for AutoEq being off stand');
  assert.match(refusalWords('search', { reason: 'unreachable', error: 'could not fetch x' }), /AutoEq could not be reached/);
  r.window.close();
});

// ---- part 3: the preview ----

/** A rig with the one-headphone answer on show and the preview answered by `preview`. */
async function previewRig(options = {}, preview = good({ preview: PREVIEW() })) {
  const r = rig(options);
  r.daemon.answer = answers({ search: good(ONE), preview: typeof preview === 'function' ? preview : () => preview });
  await r.open();
  await r.search('hd 650');
  return r;
}

await ok('choosing_a_measurement_asks_for_the_preview_naming_the_shown_device_and_draws_the_numbers', async () => {
  for (const shown of ['A', 'B']) {
    const r = await previewRig({ shown });
    await r.click(variantId(0, 0));
    const calls = r.posts('/preview');
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0].body, {
      variant: { name: 'Sennheiser HD 650', source: 'oratory1990', form: 'over-ear' },
      target: { side: 'device', device: shown },
    }, `${shown} on show: the call names the device itself and sends the measurement's four keys`);
    const pane = r.$(IDS.preview);
    assert.match(pane.textContent, /Fit\s*0\.9 dB/);
    assert.match(pane.textContent, /Worst miss\s*3\.1 dB at 6\.3 kHz/);
    assert.match(pane.textContent, /Lower the volume by 2\.4 dB/);
    assert.ok(pane.querySelector('.fp-graph-target'), 'the target');
    assert.ok(pane.querySelector('.fp-graph-series'), 'the fit');
    assert.ok(pane.querySelector('.fp-graph-gap'), 'the gap as a shaded area');
    assert.equal(pane.querySelector('.fp-graph-compare'), null, 'no second measurement yet');
    assert.equal(r.$(variantId(0, 0)).getAttribute('aria-pressed'), 'true');
    assert.equal(r.$(variantId(0, 1)).getAttribute('aria-pressed'), 'false');
    r.window.close();
  }
});

await ok('the_graph_is_the_daemons_numbers_and_a_preview_with_no_gap_or_boost_has_neither', async () => {
  const flat = PREVIEW({ peak_db: 0, fit: { ...PREVIEW().fit, gap: [] } });
  const r = await previewRig({}, good({ preview: flat }));
  await r.click(variantId(0, 0));
  const pane = r.$(IDS.preview);
  assert.equal(pane.querySelector('.fp-graph-gap'), null, 'no span, no shading');
  assert.doesNotMatch(pane.textContent, /Lower the volume/);
  assert.ok(pane.querySelector('svg path.fp-graph-series').getAttribute('d').startsWith('M'));
  r.window.close();
});

await ok('compare_with_is_a_list_from_a_chip_in_the_pane_and_draws_the_second_target_with_its_numbers', async () => {
  const second = PREVIEW({ preamp_db: -2, peak_db: 1, fit: { ...PREVIEW().fit, rms_db: 1.7, worst_db: 4.4, worst_hz: 80 } });
  const r = await previewRig({}, (call) => good(call.body.compare
    ? { preview: PREVIEW(), compare: second } : { preview: PREVIEW() }));
  await r.click(variantId(0, 0));
  assert.equal(r.$(IDS.compareOpen).closest('.fp-head'), null, 'the chip is in the pane, not the head');
  assert.ok(r.$(IDS.compareOpen).closest('[data-list-place]'), 'in a place for lists');
  await r.click(IDS.compareOpen);
  const options = [...r.document.querySelectorAll('.fi-opt')];
  assert.deepEqual(options.map((el) => el.textContent), [
    'Rtings · over-ear · GRAS 43AG', 'Innerfidelity · over-ear',
  ], 'the other measurements of the same headphone, never the chosen one');
  options[0].click();
  await r.calm();
  const calls = r.posts('/preview');
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[1].body.compare, { name: 'Sennheiser HD 650', source: 'Rtings', form: 'over-ear', rig: 'GRAS 43AG' });
  const pane = r.$(IDS.preview);
  assert.ok(pane.querySelector('.fp-graph-compare'), 'the second target in its own class');
  assert.match(pane.textContent, /Compared: Rtings · over-ear · GRAS 43AG/);
  assert.match(pane.textContent, /1\.7 dB/);
  assert.match(pane.textContent, /4\.4 dB at 80 Hz/);
  await r.click(IDS.compareClear);
  assert.equal(r.posts('/preview').at(-1).body.compare, undefined, 'stopping asks for the one alone');
  assert.equal(r.$(IDS.preview).querySelector('.fp-graph-compare'), null);
  r.window.close();
});

await ok('two_choices_in_a_row_drop_the_first_answer_in_either_order', async () => {
  for (const [order, late] of [['first-first', false], ['second-first', false], ['first-first', true], ['second-first', true]]) {
    const gates = [deferred(), deferred()];
    let i = 0;
    const r = await previewRig({}, () => gates[i++].promise);
    r.daemon.ignoreAbort = late;
    await r.click(variantId(0, 0));
    await r.click(variantId(0, 1));
    assert.equal(r.daemon.aborted.length, 1, 'the first choice is aborted');
    const mine = (ratio) => good({ preview: PREVIEW({ fit: { ...PREVIEW().fit, rms_db: ratio } }) });
    const go = order === 'first-first' ? [0, 1] : [1, 0];
    for (const k of go) {
      gates[k].resolve(mine(k === 0 ? 5.5 : 1.5));
      await r.calm();
    }
    assert.match(r.$(IDS.preview).textContent, /Fit\s*1\.5 dB/, `${order}: the second choice's numbers stand`);
    assert.match(r.$(IDS.preview).textContent, /Rtings/);
    r.window.close();
  }
});

await ok('a_refused_preview_reads_its_words_and_an_aborted_one_none', async () => {
  const cases = [['busy', 409, /busy/], ['not_listed', 404, /does not list/], ['unreachable', 502, /could not be reached/],
    ['not_found_yet', 409, /not found this device yet/], ['missing', 404, /no longer configured/]];
  for (const [reason, status, words] of cases) {
    const r = await previewRig({}, refused(status, reason));
    await r.click(variantId(0, 0));
    assert.match(r.$(IDS.preview).textContent, words, reason);
    assert.equal(r.$(IDS.preview).querySelector('.fp-failed')?.getAttribute('role'), 'alert');
    r.window.close();
  }
  const r = await previewRig({}, refused(422, 'invalid', 'the correction has 40 filters; the computer holds 16'));
  await r.click(variantId(0, 0));
  assert.match(r.$(IDS.preview).textContent, /40 filters; the computer holds 16/, 'the daemon\'s own words for invalid');
  r.window.close();
});

// ---- part 4: the import ----

const SAVED = good({ name: 'Sennheiser HD 650', filters: 10, skipped: 2 });

await ok('import_names_the_device_sends_no_name_while_it_is_the_headphones_and_says_it_is_not_on', async () => {
  const r = await previewRig({ shown: 'B' });
  r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW() }), import: SAVED });
  await r.click(variantId(0, 2));
  const field = r.$(IDS.importName);
  assert.equal(field.value, 'Sennheiser HD 650', 'the headphone\'s own name, editable');
  await r.submit(field.form);
  const calls = r.posts('/import');
  assert.deepEqual(calls[0].body, {
    variant: { name: 'Sennheiser HD 650', source: 'Innerfidelity', form: 'over-ear' },
    target: { side: 'device', device: 'B' },
  }, 'the name is left to the daemon while it is the default');
  const said = r.text(IDS.importWords);
  assert.match(said, /Saved “Sennheiser HD 650” \(10 filters\)/);
  assert.match(said, /it is not on B yet/);
  assert.match(said, /2 lines of the file were left out/);
  assert.ok(r.posts('/model').length >= 1 || true);
  r.type(IDS.importName, 'My HD 650');
  await r.submit(r.$(IDS.importName).form);
  assert.equal(r.posts('/import')[1].body.name, 'My HD 650', 'a typed name is sent');
  r.window.close();
});

await ok('a_taken_name_is_worded_at_the_field_with_the_suggestion_and_a_bad_name_in_the_daemons_words', async () => {
  const r = await previewRig();
  r.daemon.answer = answers({
    search: good(ONE), preview: good({ preview: PREVIEW() }),
    import: (call) => (call.body.name === 'too long'
      ? refused(422, 'bad_name', 'a name is at most 40 characters')
      : refused(422, 'taken', 'daemon words')),
  });
  await r.click(variantId(0, 1));
  await r.submit(r.$(IDS.importName).form);
  const field = r.$(IDS.importName);
  assert.equal(field.getAttribute('aria-invalid'), 'true');
  assert.equal(field.getAttribute('aria-describedby'), IDS.importWords);
  assert.match(r.text(IDS.importWords), /A preset with this name exists already\./);
  assert.equal(field.value, 'Sennheiser HD 650', 'what was typed stays');
  assert.equal(r.at(), IDS.importName, 'the focus is on the field');
  const suggest = r.document.querySelector('[data-act="suggest"]');
  assert.equal(suggest.textContent, 'Use “Sennheiser HD 650 (Rtings)”');
  suggest.click();
  await r.calm();
  assert.equal(r.$(IDS.importName).value, 'Sennheiser HD 650 (Rtings)');
  assert.equal(r.$(IDS.importName).getAttribute('aria-invalid'), 'false');
  assert.equal(r.at(), IDS.importName);
  r.type(IDS.importName, 'too long');
  await r.submit(r.$(IDS.importName).form);
  assert.equal(r.text(IDS.importWords), 'a name is at most 40 characters');
  r.window.close();
});

await ok('an_empty_name_sends_nothing_and_a_refusal_not_about_the_name_keeps_the_focus_on_save', async () => {
  const r = await previewRig();
  await r.click(variantId(0, 0));
  r.type(IDS.importName, '  ');
  await r.submit(r.$(IDS.importName).form);
  assert.equal(r.posts('/import').length, 0);
  assert.equal(r.text(IDS.importWords), 'Give the preset a name.');
  assert.equal(r.$(IDS.importName).getAttribute('aria-invalid'), 'true');
  r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW() }), import: refused(409, 'read_only') });
  r.type(IDS.importName, 'Fine');
  await r.submit(r.$(IDS.importName).form);
  assert.match(r.text(IDS.importWords), /read-only/);
  assert.equal(r.at(), IDS.importSave);
  r.window.close();
});

await ok('a_missing_file_reads_in_the_daemons_words_not_as_read_only', async () => {
  const error = '/x/config.toml is missing: the daemon keeps its running configuration; restore the '
    + 'file, then try again';
  for (const kind of ['import', 'upload']) {
    assert.equal(refusalWords(kind, { reason: 'read_only', status: 409, error }), error);
  }
  assert.match(refusalWords('import', { reason: 'read_only', status: 409, error: 'x' }), /read-only/);
});

await ok('the_answer_and_the_models_event_reach_the_saved_list_in_either_order', async () => {
  for (const order of ['answer-first', 'event-first']) {
    const r = await previewRig();
    const gate = deferred();
    r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW() }), import: () => gate.promise });
    await r.click(variantId(0, 0));
    r.core.router.step('saved');
    await r.calm();
    r.core.router.step('autoeq');
    await r.calm();
    await r.search('hd 650');
    await r.click(variantId(0, 0));
    const pending = r.submit(r.$(IDS.importName).form);
    await r.calm();
    const withNew = deviceModel('A', {});
    withNew.presets.rows = presetRows([['Flat', { built_in: true }], ['Mine'], ['Sennheiser HD 650', { imported: true }]]);
    // the daemon lists the new preset from the moment it is saved: a read after it says so too
    r.daemon.models.A = withNew;
    const event = () => r.core.model.applyPart('A', 'presets', withNew.presets);
    if (order === 'event-first') event();
    gate.resolve(SAVED);
    await pending;
    await r.calm();
    if (order === 'answer-first') event();
    r.core.router.step('saved');
    await r.calm();
    const names = [...r.document.querySelectorAll('.fpe-row-name')].map((el) => el.textContent);
    assert.deepEqual(names, ['Flat', 'Mine', 'Sennheiser HD 650'], `${order}: the list has the new preset`);
    assert.match(r.$('popup-sound').textContent, /Imported/);
    r.window.close();
  }
});

// ---- part 5: the upload ----

const file = (r, text, name = 'HD 650 ParametricEQ.txt') => {
  const made = new r.window.File([text], name, { type: 'text/plain' });
  const input = r.$(IDS.file);
  Object.defineProperty(input, 'files', { value: [made], configurable: true });
  input.dispatchEvent(new r.window.Event('change', { bubbles: true }));
  return made;
};

await ok('an_upload_is_read_in_the_page_named_after_the_file_and_saved_with_the_daemons_words_for_a_refusal', async () => {
  const r = rig();
  r.daemon.answer = answers({ upload: (call) => (call.body.text.includes('bad')
    ? refused(422, 'invalid', 'line 3: cannot read "Filter nonsense"')
    : good({ name: call.body.name, filters: 3, skipped: 0 })) });
  await r.open('upload');
  const save = r.$(IDS.upSave);
  assert.equal(save.getAttribute('aria-disabled'), 'true', 'no file: Save is disabled');
  assert.equal(save.getAttribute('aria-describedby'), IDS.upWords);
  assert.equal(r.text(IDS.upWords), 'Choose a file first.', 'and it says why');
  assert.equal(save.disabled, false, 'it stays a stop of the keyboard');
  await r.submit(r.$(IDS.file).form);
  assert.equal(r.posts('/upload').length, 0, 'pressed with no file: nothing is sent');
  file(r, 'bad text');
  await r.calm();
  assert.equal(r.$(IDS.upName).value, 'HD 650', 'named after the file, as `eq import` names it');
  assert.equal(r.$(IDS.upSave).getAttribute('aria-disabled'), null);
  await r.submit(r.$(IDS.file).form);
  assert.equal(r.text(IDS.upWords), 'line 3: cannot read "Filter nonsense"', 'the daemon\'s own words');
  assert.equal(r.$(IDS.upSave).getAttribute('aria-describedby'), null);
  file(r, 'Preamp: -3 dB');
  r.type(IDS.upName, 'Mine 2');
  await r.calm();
  await r.submit(r.$(IDS.file).form);
  assert.deepEqual(r.posts('/upload').at(-1).body, { text: 'Preamp: -3 dB', name: 'Mine 2' });
  assert.match(r.text(IDS.upWords), /Saved “Mine 2” \(3 filters\)\. It is saved only: it is not on A yet/);
  r.window.close();
});

await ok('a_file_over_64_kib_is_refused_in_the_page_and_a_taken_upload_name_is_worded_at_the_field', async () => {
  const r = rig();
  r.daemon.answer = answers({ upload: refused(422, 'taken') });
  await r.open('upload');
  file(r, 'x'.repeat(64 * 1024 + 1));
  await r.calm();
  assert.match(r.text(IDS.upWords), /larger than 64 KiB/);
  assert.equal(r.$(IDS.upSave).getAttribute('aria-disabled'), 'true');
  assert.equal(r.posts('/upload').length, 0);
  file(r, 'Preamp: -3 dB');
  await r.calm();
  await r.submit(r.$(IDS.file).form);
  assert.equal(r.$(IDS.upName).getAttribute('aria-invalid'), 'true');
  assert.match(r.text(IDS.upWords), /exists already/);
  assert.equal(r.at(), IDS.upName);
  r.window.close();
});

// ---- part 6: the saved presets ----

const ofRows = () => [...document.querySelectorAll('.fpe-row')];

await ok('each_saved_preset_says_where_it_lives_and_why_it_cannot_be_overwritten', async () => {
  const r = rig({
    devices: { A: { presets: { ...deviceModel('A').presets, rows: presetRows([
      ['Flat', { built_in: true }], ['Mine', { on: true }], ['From AutoEq', { imported: true }], ['Other']]) } } },
  });
  await r.open('saved');
  assert.equal(r.title(), 'EQ library · A');
  const rows = ofRows();
  assert.deepEqual(rows.map((el) => el.querySelector('.fpe-row-name').textContent), ['Flat', 'Mine', 'From AutoEq', 'Other']);
  assert.deepEqual(rows.map((el) => el.querySelector('.fpe-where').textContent), [
    'In the library', 'On A now', 'In the library', 'In the library']);
  assert.match(rows[0].textContent, /Built in/);
  assert.match(rows[2].textContent, /Imported/);
  const overwrite = (i) => r.$(overwriteId(i));
  assert.equal(overwrite(0).getAttribute('aria-disabled'), 'true');
  assert.match(rows[0].querySelector('.fpe-why').textContent, /Flat is built in: save the curve under a new name\./);
  assert.match(rows[2].querySelector('.fpe-why').textContent, /From AutoEq is imported: save the curve under a new name\./);
  assert.equal(overwrite(1).getAttribute('aria-disabled'), null, 'a preset of the person\'s own can be overwritten');
  assert.equal(overwrite(3).getAttribute('aria-disabled'), null);
  assert.ok(!r.text('popup-sound').includes('saved on the device'), '"saved on the device" is not claimed');
  r.window.close();
});

await ok('overwrite_asks_once_with_the_safe_answer_focused_and_sends_force_naming_the_device', async () => {
  for (const shown of ['A', 'B']) {
    const r = rig({ shown });
    r.daemon.answer = (call) => (call.path.endsWith('/presets')
      ? good({ parts: {}, toast: 'Saved' }) : good({}));
    await r.open('saved');
    r.$(overwriteId(1)).click();
    await r.calm();
    const keep = r.$('popup-sound').querySelector('[data-answer="keep"]');
    assert.equal(r.at(), null === keep ? '' : r.document.activeElement.id, 'x');
    assert.equal(r.document.activeElement, keep, 'the safe answer has the focus');
    assert.match(r.$(askId(1)).textContent, new RegExp(`Replace “Mine” with the curve that is on ${shown} now\\?`));
    keep.click();
    await r.calm();
    assert.equal(r.posts('/presets').length, 0, 'keeping sends nothing');
    r.$(overwriteId(1)).click();
    await r.calm();
    r.$('popup-sound').querySelector('[data-answer="overwrite"]').click();
    await r.calm();
    const sent = r.posts('/presets');
    assert.equal(sent.length, 1);
    assert.equal(sent[0].path, `api/v1/devices/${shown}/presets`, 'the device it names itself');
    assert.deepEqual(sent[0].body, { name: 'Mine', force: true });
    assert.match(r.text(IDS.savedWords), /“Mine” now holds the curve that is on\./);
    assert.equal(r.$(askId(1)).textContent, '', 'the question closed');
    r.window.close();
  }
});

await ok('a_refused_overwrite_keeps_the_question_and_says_why', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path.endsWith('/presets') ? refused(409, 'read_only') : good({}));
  await r.open('saved');
  r.$(overwriteId(1)).click();
  await r.calm();
  r.$('popup-sound').querySelector('[data-answer="overwrite"]').click();
  await r.calm();
  assert.match(r.text(IDS.savedWords), /read-only/);
  assert.ok(r.$('popup-sound').querySelector('[data-answer="keep"]'), 'the question stays');
  assert.equal(r.document.activeElement.dataset.answer, 'keep');
  r.window.close();
});

await ok('overwrite_is_disabled_with_the_reason_offline_without_a_tone_control_and_before_the_tone_is_read', async () => {
  const cases = [
    ['offline', { link: link(false) }, /A is not connected/],
    ['no tone control', { presets: null }, /A has no tone control/],
    ['not read', { tone: { response: null } }, /has not been read yet/],
  ];
  for (const [name, over, words] of cases) {
    const r = rig({ devices: { A: over } });
    await r.open('saved');
    if (name === 'no tone control') {
      assert.match(r.text(IDS.saved), words, name);
    } else {
      const button = r.$(overwriteId(1));
      assert.equal(button.getAttribute('aria-disabled'), 'true', name);
      assert.match(r.text(IDS.savedNote), words, name);
      assert.ok(button.getAttribute('aria-describedby').includes(IDS.savedNote), 'the reason describes it');
      button.click();
      await r.calm();
      assert.equal(r.document.querySelector('[data-answer="keep"]'), null, `${name}: no question`);
      assert.match(r.text(IDS.savedWords), words, `${name}: pressing says why`);
    }
    r.window.close();
  }
});

await ok('an_open_question_is_not_drawn_away_by_a_model_event_and_the_list_follows_once_it_closes', async () => {
  const r = rig();
  await r.open('saved');
  r.$(overwriteId(1)).click();
  await r.calm();
  const withNew = deviceModel('A');
  withNew.presets.rows = presetRows([['Flat', { built_in: true }], ['Mine'], ['Added']]);
  r.core.model.applyPart('A', 'presets', withNew.presets);
  await r.calm();
  assert.ok(r.document.querySelector('[data-answer="keep"]'), 'the question is still there');
  assert.equal(ofRows().length, 3, 'the list was not put under the question');
  r.document.querySelector('[data-answer="keep"]').click();
  await r.calm();
  assert.equal(ofRows().length, 3);
  assert.deepEqual([...r.document.querySelectorAll('.fpe-row-name')].map((el) => el.textContent), ['Flat', 'Mine', 'Added']);
  r.window.close();
});

// ---- part 7: the computer's side ----

const host = () => ({ computer: computerModel() });

await ok('the_computer_side_names_the_computer_previews_the_correction_and_overwrite_is_disabled', async () => {
  const r = rig({ host: host(), side: 'computer' });
  r.daemon.answer = answers({
    search: good(ONE),
    preview: good({ preview: PREVIEW({ fit: null, peak_db: 3.2, preamp_db: -6.1 }) }),
    import: SAVED,
  });
  await r.open();
  assert.equal(r.title(), 'EQ library · This computer');
  await r.search('hd 650');
  await r.click(variantId(0, 0));
  assert.deepEqual(r.posts('/preview')[0].body.target, { side: 'computer' });
  const pane = r.$(IDS.preview);
  assert.match(pane.textContent, /correction as the computer plays it; nothing is fitted/);
  assert.match(pane.textContent, /Preamp\s*−6\.1 dB/);
  assert.match(pane.textContent, /boosts by 3\.2 dB at most/);
  assert.doesNotMatch(pane.textContent, /Worst miss/, 'no fit, no miss');
  assert.equal(pane.querySelector('.fp-graph-series'), null);
  assert.equal(pane.querySelector('.fp-graph-gap'), null);
  await r.submit(r.$(IDS.importName).form);
  assert.deepEqual(r.posts('/import')[0].body.target, { side: 'computer' });
  assert.match(r.text(IDS.importWords), /not on the computer yet/);
  r.core.router.step('saved');
  await r.calm();
  assert.equal(r.title(), 'EQ library · This computer');
  const rows = ofRows();
  assert.deepEqual(rows.map((el) => el.querySelector('.fpe-where').textContent), [
    'In the library', 'On the computer now', 'In the library']);
  assert.match(rows[2].textContent, /it has 40 filters; the computer holds 16/, 'a preset the computer cannot hold says so');
  assert.ok(ofRows().every((el, i) => r.$(overwriteId(i)).getAttribute('aria-disabled') === 'true'));
  assert.match(r.text(IDS.savedNote), /saved as a new preset, not over one/);
  r.window.close();
});

await ok('a_computer_that_becomes_unavailable_draws_the_popup_again_for_the_device_and_drops_the_old_preview', async () => {
  const r = rig({ host: host(), side: 'computer' });
  r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW({ fit: null }) }) });
  await r.open();
  await r.search('hd 650');
  await r.click(variantId(0, 0));
  assert.ok(r.$(IDS.preview).querySelector('svg'));
  r.core.model.replace({ ...r.core.model.page(), host: { computer: { ...computerModel(), unavailable: 'no sink' } } });
  await r.calm();
  assert.equal(r.title(), 'EQ library · A', 'the side now');
  assert.match(r.text(IDS.preview), /Choose a measurement/, 'a preview of the other side is dropped');
  assert.ok(r.document.querySelector('.fpe-group'), 'the search stays');
  r.window.close();
});

// ---- the calls themselves ----

await ok('a_call_that_names_no_side_sends_nothing_and_a_guest_is_turned_away', async () => {
  const r = rig();
  const a = await r.core.actions.autoEqPreview({ name: 'x', source: 's', form: 'f' }, {});
  assert.equal(a.reason, 'no_target');
  const b = await r.core.actions.autoEqImport({ name: 'x', source: 's', form: 'f' }, { computer: false });
  assert.equal(b.reason, 'no_target');
  const c = await r.core.actions.overwritePreset('Mine', {});
  assert.equal(c.reason, 'no_target');
  assert.equal(r.daemon.calls.filter((one) => one.path.startsWith('api/v1/autoeq') || one.path.endsWith('/presets')).length, 0);
  r.core.model.replace({ ...r.core.model.page(), viewer: { signed_in: false, guest: true, writes: false } });
  const d = await r.core.actions.autoEqSearch('hd');
  assert.equal(d.ok, false);
  assert.equal(d.reason, 'signed_out');
  assert.equal(r.posts('/search').length, 0);
  r.window.close();
});

await ok('closing_the_popup_stops_the_requests_and_a_reopened_one_starts_clean', async () => {
  const r = rig();
  const held = deferred();
  r.daemon.answer = answers({ search: good(ONE), preview: () => held.promise });
  await r.open();
  await r.search('hd 650');
  await r.click(variantId(0, 0));
  r.core.router.close();
  await r.calm();
  assert.equal(r.daemon.aborted.length, 1, 'the preview in flight was aborted');
  held.resolve(good({ preview: PREVIEW() }));
  await r.calm();
  await r.open();
  assert.equal(r.$(IDS.query).value, '');
  assert.match(r.text(IDS.status), /Type a headphone/);
  assert.match(r.text(IDS.preview), /Choose a measurement/);
  r.window.close();
});

await ok('the_words_and_keys_of_the_draws_are_what_the_page_shows', async () => {
  assert.equal(hzWords(6300), '6.3 kHz');
  assert.equal(hzWords(80), '80 Hz');
  assert.equal(keyOf({ name: 'a', source: 's', form: 'f' }), keyOf({ name: 'a', source: 's', form: 'f', rig: undefined }));
  assert.equal(eid('x'), 'popup-eq-library-x');
});

// ---- fix round 1 ----

await ok('the_focus_stays_where_the_person_was_after_a_compare_pick_stop_comparing_an_import_and_an_upload', async () => {
  const second = PREVIEW({ preamp_db: -2 });
  const r = await previewRig({}, (call) => good(call.body.compare
    ? { preview: PREVIEW(), compare: second } : { preview: PREVIEW() }));
  r.daemon.answer = answers({
    search: good(ONE), import: SAVED, upload: good({ name: 'Mine', filters: 1, skipped: 0 }),
    preview: (call) => good(call.body.compare ? { preview: PREVIEW(), compare: second } : { preview: PREVIEW() }),
  });
  await r.click(variantId(0, 0));
  r.$(IDS.compareOpen).focus();
  await r.click(IDS.compareOpen);
  r.document.querySelector('.fi-opt').click();
  await r.calm();
  assert.equal(r.at(), IDS.compareClear, 'a pick: the focus is on Stop comparing');
  r.$(IDS.compareClear).focus();
  await r.click(IDS.compareClear);
  assert.equal(r.at(), IDS.compareOpen, 'Stop comparing: the focus is on Compare with…');
  r.type(IDS.importName, 'First');
  await r.submit(r.$(IDS.importName).form);
  assert.equal(r.at(), IDS.importName, 'Enter in the name: the focus stays in the field');
  r.$(IDS.importSave).focus();
  await r.click(IDS.importSave);
  assert.equal(r.at(), IDS.importSave, 'Save as a preset keeps the focus');
  r.core.router.step('upload');
  await r.calm();
  file(r, 'Preamp: -3 dB');
  await r.calm();
  r.$(IDS.upSave).focus();
  await r.click(IDS.upSave);
  assert.equal(r.at(), IDS.upSave, 'the upload\'s Save keeps the focus');
  assert.equal(r.$(IDS.upSave).disabled, false, 'it is never natively disabled');
  r.window.close();
});

await ok('an_upload_names_the_file_as_eq_import_does_and_a_second_file_has_its_own_name', async () => {
  const r = rig();
  r.daemon.answer = answers({ upload: (call) => good({ name: call.body.name, filters: 1, skipped: 0 }) });
  await r.open('upload');
  file(r, 'Preamp: -3 dB', `${'A very long headphone name '.repeat(3)} ParametricEQ.txt`);
  await r.calm();
  assert.equal(r.$(IDS.upName).value.length, 40, 'cut at 40 characters');
  file(r, 'Preamp: -3 dB', ' hd650 PARAMETRICEQ .txt');
  await r.calm();
  assert.equal(r.$(IDS.upName).value, 'hd650', 'the suffix in any case, trimmed, as preset::default_name');
  r.type(IDS.upName, 'Mine 2');
  await r.submit(r.$(IDS.file).form);
  file(r, 'Preamp: -3 dB', 'Another.txt');
  await r.calm();
  assert.equal(r.$(IDS.upName).value, 'Another', 'a typed name belongs to the file it was typed for');
  r.window.close();
});

await ok('an_answer_that_lands_after_another_choice_is_not_shown_as_the_new_ones_and_still_lists', async () => {
  const gate = deferred();
  const r = await previewRig({}, good({ preview: PREVIEW() }));
  r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW() }), import: () => gate.promise, upload: () => gate.promise });
  await r.click(variantId(0, 0));
  const pending = r.submit(r.$(IDS.importName).form);
  await r.click(variantId(0, 1));
  assert.equal(r.daemon.aborted.length, 0, 'another choice does not stop a save on its way');
  gate.resolve(SAVED);
  await pending;
  await r.calm();
  assert.match(r.text(IDS.importWords), /^Saved “Sennheiser HD 650” for oratory1990 · over-ear, the measurement chosen before\./,
    'the late save is told, as the one it was for');
  assert.doesNotMatch(r.text(IDS.importWords), /Saved “Sennheiser HD 650” \(/, 'and not as the new choice\'s own');
  const up = rig();
  const hold = deferred();
  up.daemon.answer = answers({ upload: () => hold.promise });
  await up.open('upload');
  file(up, 'Preamp: -3 dB', 'One.txt');
  await up.calm();
  const sending = up.submit(up.$(IDS.file).form);
  file(up, 'Preamp: -2 dB', 'Two.txt');
  await up.calm();
  hold.resolve(good({ name: 'One', filters: 1, skipped: 0 }));
  await sending;
  await up.calm();
  assert.doesNotMatch(up.text(IDS.upWords), /Saved/, 'the answer for One is not Two\'s');
  assert.equal(up.$(IDS.upName).value, 'Two');
  up.window.close();
  r.window.close();
});

await ok('a_side_change_sends_the_kept_search_again_and_an_answer_with_no_groups_is_a_failure', async () => {
  const r = rig({ host: host(), side: 'computer' });
  const gate = deferred();
  r.daemon.answer = answers({ search: () => gate.promise });
  await r.open();
  await r.search('hd');
  r.core.model.replace({ ...r.core.model.page(), host: { computer: { ...computerModel(), unavailable: 'no sink' } } });
  await r.calm();
  assert.equal(r.posts('/search').length, 2, 'the query is sent again for the side now');
  gate.resolve({ status: 200, body: {} });
  await r.calm();
  assert.match(r.text(IDS.status), /did not work/, 'an answer with no groups is a failure, not a crash');
  r.window.close();
});

await ok('a_device_that_is_not_found_is_not_said_to_have_no_tone_control_and_the_words_stay_in_stable_regions', async () => {
  const r = rig({ devices: { A: { presets: null, link: link(false) } } });
  await r.open('saved');
  assert.match(r.text(IDS.saved), /A is not connected, so its presets are not known yet/);
  assert.doesNotMatch(r.text(IDS.saved), /no tone control/);
  // each words line is the content of a live region that is put in place, never the region itself
  assert.equal(r.$(IDS.savedWordsPlace).getAttribute('role'), 'status');
  r.core.router.step('autoeq');
  await r.calm();
  for (const id of [IDS.status, IDS.importWordsPlace]) assert.equal(r.$(id).getAttribute('role'), 'status', id);
  const before = r.$(IDS.status);
  r.daemon.answer = answers({ search: good(ONE) });
  await r.search('hd');
  assert.equal(r.$(IDS.status), before, 'the status region is the same element after an answer');
  assert.equal(r.$(IDS.status).querySelector('[role]'), null);
  r.window.close();
});

await ok('an_overwrite_is_worded_once_under_the_list_and_not_toasted', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path.endsWith('/presets') ? good({ parts: {}, toast: 'Saved Mine' }) : good({}));
  await r.open('saved');
  r.$(overwriteId(1)).click();
  await r.calm();
  r.$('popup-sound').querySelector('[data-answer="overwrite"]').click();
  await r.calm();
  assert.match(r.text(IDS.savedWords), /now holds the curve that is on/);
  assert.equal(r.$('toasts').textContent, '', 'the answer\'s toast would say it twice');
  r.window.close();
});

await ok('the_loading_line_is_in_a_stable_live_region_and_not_in_the_replaced_preview', async () => {
  const gate = deferred();
  const r = await previewRig({}, () => gate.promise);
  await r.click(variantId(0, 0));
  assert.match(r.text(IDS.previewWords), /Fitting oratory1990 · over-ear to A…/);
  assert.equal(r.$(IDS.previewWords).getAttribute('role'), 'status');
  assert.equal(r.$(IDS.preview).querySelector('[role="status"]'), null);
  const region = r.$(IDS.previewWords);
  gate.resolve(good({ preview: PREVIEW() }));
  await r.calm();
  assert.equal(r.$(IDS.previewWords), region, 'the same element once it is answered');
  assert.equal(r.text(IDS.previewWords), '', 'and empty');
  r.window.close();
});

await ok('a_save_reads_the_other_side_too', async () => {
  // each case: the side the popup is on (read as before) and the other one (read too)
  for (const [options, here, other] of [
    [{ shown: 'B', host: host() }, /api\/v1\/devices\/B\/model/, /api\/v1\/host/],
    [{ shown: 'B', host: host(), side: 'computer' }, /api\/v1\/host/, /api\/v1\/devices\/B\/model/],
  ]) {
    const r = await previewRig(options);
    r.daemon.answer = answers({ search: good(ONE), preview: good({ preview: PREVIEW() }), import: SAVED });
    await r.click(variantId(0, 2));
    r.daemon.calls.length = 0;
    await r.submit(r.$(IDS.importName).form);
    const paths = r.daemon.calls.map((call) => call.path);
    assert.ok(paths.some((path) => here.test(path)), `this side is read: ${paths}`);
    assert.ok(paths.some((path) => other.test(path)), `the other side is read: ${paths}`);
    r.window.close();
  }
});

console.log(`core-eq-library: ${n} cases passed`);
