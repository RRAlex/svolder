/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Sound popup's Crossfeed tab (core/popups/crossfeed/{draw,wire}.js, the tabs
// of core/popups/sound/, and the lane of core/computer-actions.js) on the real core over a jsdom
// page and a small fake daemon whose crossfeed answers the way the host's contract says
// (`CrossfeedView`, `HostEqOp::Crossfeed`). First the draws, as strings, from views: off, Bauer ·
// Chu Moy, Custom after an edit, Mid/Side bass (delay none), the "turned off" note, a view that is
// unavailable, none at all; the steppers (range and step from the view, a held value laid over
// it); then the wire: the menu row, the tabs, a press -> a request -> the view that comes back, a
// hold that repeats with `done: false` and ends with `done: true`, the newest value wins, the
// answer of a write and the stream's update in either order, a refusal in the tab's own words, a
// 404, the guest with no way in, Escape and the keyboard. Run: node core-crossfeed.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { CROSSFEED, POPUPS, SOUND } = await import(
  '../../../bins/svolderd/web/core/popups/listed.js');
const draw = await import('../../../bins/svolderd/web/core/popups/crossfeed/draw.js');
const { HOLD_MS, REPEAT_MS } = await import(
  '../../../bins/svolderd/web/core/popups/crossfeed/wire.js');
const sound = await import('../../../bins/svolderd/web/core/popups/sound/draw.js');

let n = 0;
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};
console.warn = () => {};

// ---- the views, as the host gives them ----

const curve = (leak = 1) => {
  const hz = [];
  for (let i = 0; i < 61; i++) hz.push(Math.round(20 * 10 ** (i / 20) * 100) / 100);
  return {
    hz,
    same_db: hz.map((f) => -0.2 * Math.log10(f / 20)),
    other_db: hz.map((f) => -8.6 - leak * 12 * Math.log10(f / 20)),
  };
};
const range = (min, max, step) => ({ min, max, step });
const MODELS = [
  {
    word: 'bauer', name: 'Bauer', description: 'Feeds a low-passed, delayed copy of each channel into the other channel at a set level.',
    presets: [
      { name: 'Jan Meier', crossover_hz: 650, amount_pct: 25, feed_db: 9.5 },
      { name: 'Chu Moy', crossover_hz: 700, amount_pct: 33, feed_db: 6 },
      { name: 'bs2b default', crossover_hz: 700, amount_pct: 37, feed_db: 4.5 },
    ],
    default_preset: 'bs2b default',
    crossover_hz: range(300, 2000, 10), amount_pct: range(5, 50, 1), delay_max_ms: 1, delay_step_ms: 0.02,
    delay_settable: true,
  },
  {
    word: 'mid-side', name: 'Mid/Side bass', description: 'Blends the bass toward mono.',
    presets: [{ name: 'Mono bass', crossover_hz: 120, amount_pct: 50 }], default_preset: 'Mono bass',
    crossover_hz: range(60, 300, 5), amount_pct: range(5, 50, 1), delay_max_ms: 0, delay_step_ms: 0,
    delay_settable: false,
  },
];
const VIEW = (over = {}) => ({
  on: true, model: 'bauer', preset: 'bs2b default', edited_from: null, amount_pct: 37, amount_db: -8.6,
  feed_db: 4.5, crossover_hz: 700, delay_ms: 0.26, delay_floor_ms: 0.26, delay_settable: true,
  delay_own: true, output: 'Gungnir 2', note: null, off_reason: null, unavailable: null,
  curve: curve(), models: MODELS, ...over,
});
const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();

// ---- part 1: the draws ----

await ok('the_page_draws_the_views_numbers_and_words_and_nothing_of_its_own', async () => {
  const html = draw.pageHtml({ cf: VIEW() });
  const words = text(html);
  for (const part of ['Bauer', 'bs2b default', '37 % · −8.6 dB', '700 Hz', '0.26 ms',
    'For Gungnir 2', 'Feeds a low-passed, delayed copy', 'Same ear', 'Other ear',
    'Cross path: 37 % (−8.6 dB), below 700 Hz, 0.26 ms behind.']) {
    assert.ok(words.includes(part), `${part} is on the page: ${words}`);
  }
  assert.ok(!html.includes('fpx-off'), 'on is not dimmed');
  assert.ok(html.includes('fpx-svg') && html.includes('fpx-cross') && html.includes('fpx-dot'),
    'the picture, with the crossover marked');
  assert.match(words, /own/, 'the delay is the model\'s own');
});

await ok('off_is_dimmed_and_still_editable_and_says_why_it_turned_off', async () => {
  const html = draw.pageHtml({ cf: VIEW({ on: false, note: 'crossfeed turned off: the output changed', off_reason: 'output-changed' }) });
  assert.ok(html.includes('fpx fpx-off'));
  assert.ok(text(html).includes('crossfeed turned off: the output changed'));
  assert.ok(!html.includes('aria-disabled="true"><span class="fpx-switch-track"'));
  const stuck = html.match(/id="popup-sound-xf-[a-z_]+-(?:less|more)"[^>]*aria-disabled/g) ?? [];
  assert.deepEqual(stuck.map((one) => one.slice(4, -1).split('"')[0]), ['popup-sound-xf-delay_ms-less'],
    'only the delay\'s `−` (it is at the model\'s own) does not move');
  const on = draw.pageHtml({ cf: VIEW({ note: 'old note' }) });
  assert.ok(!text(on).includes('old note'), 'a note belongs to off');
});

await ok('custom_says_what_it_was_edited_from_and_marks_the_preset_menu', async () => {
  const html = draw.pageHtml({ cf: VIEW({ preset: null, edited_from: 'bs2b default', amount_pct: 40 }) });
  assert.ok(text(html).includes('Edited from bs2b default'));
  assert.ok(html.includes('fpx-custom'));
  assert.match(text(html), /Custom/);
});

await ok('mid_side_greys_the_delay_and_says_none_and_never_steps_it', async () => {
  const cf = VIEW({
    model: 'mid-side', preset: 'Mono bass', delay_settable: false, delay_ms: 0, delay_floor_ms: 0,
    delay_own: true, crossover_hz: 120,
  });
  const html = draw.pageHtml({ cf });
  assert.ok(html.includes('fpx-grey'));
  assert.ok(text(html).includes('none'));
  assert.equal(draw.stepped(cf, 'delay_ms', 1), null);
  assert.equal(draw.rangeOf(cf, 'delay_ms'), null);
});

await ok('a_view_that_is_unavailable_says_so_and_dims_the_controls_and_a_missing_one_says_none', async () => {
  const html = draw.pageHtml({ cf: VIEW({ unavailable: 'the chain cannot hold this curve' }) });
  assert.ok(text(html).includes('the chain cannot hold this curve'));
  assert.ok(html.includes('data-act="model"') && /data-act="model"[^>]*aria-disabled="true"/.test(
    html.replace(/\s+/g, ' ')) || html.includes('aria-disabled="true"'));
  assert.ok(text(draw.pageHtml({ cf: null })).includes('not offered'));
  assert.ok(text(draw.pageHtml({ cf: null, computer: false })).includes('not on'));
});

await ok('a_hostile_name_is_escaped_everywhere_it_is_drawn', async () => {
  const cf = VIEW({ output: '<img src=x onerror=alert(1)>', note: null });
  cf.models = [{ ...MODELS[0], name: '<b>x</b>', description: '"><script>' }];
  const html = draw.pageHtml({ cf });
  assert.ok(!/<img|<script|<b>x/.test(html), html);
});

await ok('steps_come_from_the_views_range_and_step_and_stop_at_its_ends', async () => {
  const cf = VIEW();
  assert.equal(draw.stepped(cf, 'amount_pct', 1), 38);
  assert.equal(draw.stepped(cf, 'amount_pct', -1), 36);
  assert.equal(draw.stepped(cf, 'crossover_hz', 1), 710);
  assert.equal(draw.stepped({ ...cf, amount_pct: 50 }, 'amount_pct', 1), null);
  assert.equal(draw.stepped({ ...cf, amount_pct: 5 }, 'amount_pct', -1), null);
  assert.equal(draw.stepped(cf, 'delay_ms', -1), null, 'not below the model\'s own');
  assert.equal(draw.stepped(cf, 'delay_ms', 1), 0.28);
  assert.equal(draw.stepped({ ...cf, delay_ms: 1 }, 'delay_ms', 1), null);
});

await ok('what_is_asked_is_laid_over_the_view_and_the_level_goes_until_it_is_answered', async () => {
  const shown = draw.shownView(VIEW(), { amount_pct: 40 });
  assert.equal(shown.amount_pct, 40);
  assert.equal(shown.amount_db, null);
  assert.equal(shown.preset, null, 'an edit is Custom');
  assert.ok(draw.pageHtml({ cf: shown }).includes('40 %'));
  assert.ok(!text(draw.pageHtml({ cf: shown })).includes('dB'.concat(' ·')));
  assert.equal(draw.shownView(VIEW(), { on: false }).on, false);
  assert.equal(draw.shownView(VIEW(), { delay_ms: 0.4 }).delay_own, false);
  assert.equal(draw.shownView(VIEW(), { delay_ms: 0.1 }).delay_own, true);
  assert.equal(draw.shownView(null, { on: true }), null);
});

// ---- part 2: the wire on the real core ----

class FakeSource {
  addEventListener() {}

  close() {}
}
const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });
const seg = (name) => encodeURIComponent(name);
const FLAT = new Array(181).fill(0);
const deviceModel = (name) => ({
  name, url: seg(name), link: link(true), volume: null, controls: [], meters: null,
  tone: { response: FLAT, mode: 'eq' },
  presets: { status: { lead: '', name: null, tail: '' }, editable: true, recall: true, note: null, rows: [] },
});

/** The real core; the daemon is the page's `fetch`, its crossfeed a little state that applies the
 * op's fields the way the host does (a preset or a number makes it Custom). `daemon.answer` may
 * give `{ status, body }` (or a promise) in place of the default for a call; `daemon.view` is the
 * crossfeed it holds. */
function rig({ viewer = {}, view = VIEW(), computer = true, steps = {} } = {}) {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const daemon = { calls: [], answer: null, view, hold: null };
  // the stepper's hold: its timers are the cases', the rest run as they are
  const timers = [];
  const realSet = p.window.setTimeout.bind(p.window);
  const realClear = p.window.clearTimeout.bind(p.window);
  p.window.setTimeout = (fn, ms, ...rest) => {
    if (ms !== HOLD_MS && ms !== REPEAT_MS) return realSet(fn, ms, ...rest);
    const timer = { fn, id: 5000 + timers.length, live: true, ms };
    timers.push(timer);
    return timer.id;
  };
  p.window.clearTimeout = (id) => {
    const timer = timers.find((one) => one.id === id);
    if (timer) timer.live = false;
    else realClear(id);
  };
  const computerNow = () => ({
    unavailable: null, volume: null, presets: [],
    eq: { on: true, points: [], max: 16, crossfeed: daemon.view },
  });
  const apply = (op) => {
    const next = { ...daemon.view };
    if (op.model && op.model !== next.model) {
      const model = MODELS.find((one) => one.word === op.model);
      Object.assign(next, { model: op.model, preset: model.presets[0].name, amount_pct: model.presets[0].amount_pct, crossover_hz: model.presets[0].crossover_hz });
      if (!model.delay_settable) Object.assign(next, { delay_settable: false, delay_ms: 0, delay_floor_ms: 0 });
    }
    if (op.preset) {
      const preset = MODELS.find((one) => one.word === next.model).presets.find((one) => one.name === op.preset);
      Object.assign(next, { preset: preset.name, amount_pct: preset.amount_pct, crossover_hz: preset.crossover_hz });
    }
    for (const field of ['amount_pct', 'crossover_hz', 'delay_ms']) {
      if (typeof op[field] === 'number') Object.assign(next, { [field]: op[field], edited_from: next.edited_from ?? next.preset, preset: null });
    }
    if (typeof op.on === 'boolean') next.on = op.on;
    next.amount_db = Math.round(20 * Math.log10(next.amount_pct / 100) * 10) / 10;
    daemon.view = next;
  };
  p.window.fetch = (path, init = {}) => new Promise((resolve) => {
    const reply = (status, body) => resolve({ status, ok: status < 400, json: async () => body });
    const body = init.body && typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call = { path, body };
    daemon.calls.push(call);
    const model = /^api\/v1\/devices\/([^/]+)\/model/.exec(path);
    if (model) {
      reply(200, { device: deviceModel(decodeURIComponent(model[1])) });
      return;
    }
    const given = daemon.answer?.(call);
    if (path === 'api/v1/host/crossfeed') {
      Promise.resolve(given ?? { status: 200, body: {}, apply: true }).then((got) => {
        if (got.apply && got.status === 200) apply(body);
        reply(got.status, got.body);
      });
      return;
    }
    if (path.startsWith('api/v1/host')) {
      Promise.resolve(daemon.hold).then(() => reply(200, { computer: computerNow() }));
      return;
    }
    reply(200, {});
  });
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  const core = createCore({
    page: {
      v: 1, viewer: { signed_in: true, guest: false, writes: true, read_only: null, ...viewer },
      fleet: { devices: [{ url: 'A', name: 'A', link: link(true) }], hops: [] },
      shown: 'A', device: deviceModel('A'), followed: {}, card: null,
      host: computer ? { track: null, route: null, computer: computerNow() } : null, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 10; i++) await p.settle(); };
  const open = async (to = CROSSFEED) => {
    $('knob').focus();
    core.router.open(SOUND, to);
    await calm();
  };
  const click = async (id) => { $(id).click(); await calm(); };
  /** The stream tells the page the computer's model as the daemon holds it now. */
  const stream = async () => {
    core.model.emit('host', { ...core.model.page().host, computer: computerNow() });
    await calm();
  };
  const sent = () => daemon.calls.filter((c) => c.path === 'api/v1/host/crossfeed').map((c) => c.body);
  const fire = (ms) => { for (const t of timers.splice(0)) if (t.live && t.ms === ms) t.fn(); };
  return {
    ...p, core, daemon, $, calm, open, click, stream, sent, fire, timers, steps, apply,
    words: (id) => ($(id)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
  };
}

const press = (r, id, type) => r.$(id).dispatchEvent(new r.window.MouseEvent(type, {
  bubbles: true, button: 0,
}));

await ok('the_row_is_sound_a_writers_between_devices_and_help_with_no_side_list', async () => {
  assert.deepEqual(POPUPS.map((one) => one.name), ['devices', SOUND, 'setup', 'help']);
  const entry = POPUPS.find((one) => one.name === SOUND);
  assert.equal(entry.row.label, 'Sound…');
  assert.equal(entry.row.sees, 'writer');
  assert.equal(entry.entries, undefined);
});

await ok('opened_at_crossfeed_it_draws_the_tab_the_switch_and_the_view', async () => {
  const r = rig();
  await r.open();
  const tabs = [...r.document.querySelectorAll('.fp-tab')].map((t) => t.textContent.trim());
  assert.deepEqual(tabs, ['EQ library', 'Crossfeed', 'Virtual speakers']);
  assert.equal(r.$(sound.tabId('xf')).getAttribute('aria-selected'), 'true');
  assert.equal(r.document.querySelector('.fp-pills'), null, 'the pills are the EQ library tab\'s');
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true');
  assert.equal(r.$('popup-sound-title').textContent, 'Sound');
  assert.match(r.words(draw.IDS.amount), /37 % · −8\.6 dB/);
  assert.equal(r.words(draw.IDS.model), 'Bauer');
  r.window.close();
});

await ok('the_eq_library_tab_has_its_pills_and_the_tabs_step_the_router', async () => {
  const r = rig();
  await r.open(null);
  assert.equal(r.$(sound.tabId('eq')).getAttribute('aria-selected'), 'true');
  assert.deepEqual([...r.document.querySelectorAll('.fp-pill')].map((p) => p.textContent),
    ['AutoEq', 'Upload a file', 'Saved presets']);
  assert.equal(r.$(draw.IDS.switch), null, 'no switch on the other tab');
  await r.click(sound.tabId('xf'));
  assert.equal(r.core.router.state().page, CROSSFEED);
  assert.equal(r.document.querySelector('.fp-pills'), null);
  assert.ok(r.$(draw.IDS.switch));
  await r.click(sound.tabId('eq'));
  assert.equal(r.core.router.state().page, 'autoeq');
  await r.click(sound.pillId('saved'));
  assert.equal(r.core.router.state().page, 'saved');
  await r.click(sound.tabId('xf'));
  await r.click(sound.tabId('eq'));
  assert.equal(r.core.router.state().page, 'saved', 'the tab comes back to the page it was on');
  r.window.close();
});

await ok('the_arrows_move_between_the_tabs', async () => {
  const r = rig();
  await r.open(null);
  r.$(sound.tabId('eq')).focus();
  r.key('ArrowRight');
  await r.calm();
  assert.equal(r.core.router.state().page, CROSSFEED);
  assert.equal(r.document.activeElement?.id, sound.tabId('xf'));
  r.key('ArrowLeft');
  await r.calm();
  assert.equal(r.document.activeElement?.id, sound.tabId('eq'));
  r.window.close();
});

await ok('a_press_is_one_request_and_the_view_that_comes_back_is_shown', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.stepId('amount_pct', 1));
  assert.deepEqual(r.sent(), [{ amount_pct: 38, done: true }]);
  assert.match(r.words(draw.IDS.amount), /38 %/);
  assert.equal(r.words(draw.IDS.preset), 'Custom');
  assert.match(text(r.$('popup-sound').innerHTML), /Edited from bs2b default/);
  r.window.close();
});

await ok('the_switch_turns_it_on_and_off_and_the_header_follows_the_view', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.switch);
  assert.deepEqual(r.sent(), [{ on: false, done: true }]);
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'false');
  assert.ok(r.$(draw.IDS.page).classList.contains('fpx-off'));
  assert.equal(r.$(sound.tabId('xf')).querySelector('.fp-dot'), null, 'no dot on the tab when off');
  await r.click(draw.IDS.switch);
  assert.deepEqual(r.sent().at(-1), { on: true, done: true });
  assert.ok(r.$(sound.tabId('xf')).querySelector('.fp-dot.on'));
  r.window.close();
});

// Clicks the open menu's option shown as `name`; fails loudly when it is absent.
function pick(r, name) {
  const option = [...r.document.querySelectorAll('.fi-opt')].find((o) => o.textContent.trim() === name);
  assert.ok(option, `the menu offers ${name}`);
  option.click();
}

await ok('the_model_and_preset_menus_are_lists_of_the_views_models_and_presets', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.preset);
  let options = [...r.document.querySelectorAll('.fi-opt')].map((o) => o.textContent);
  assert.deepEqual(options, ['Jan Meier', 'Chu Moy', 'bs2b default']);
  pick(r, 'Chu Moy');
  await r.calm();
  assert.deepEqual(r.sent(), [{ preset: 'Chu Moy', done: true }]);
  assert.equal(r.words(draw.IDS.preset), 'Chu Moy');
  assert.match(r.words(draw.IDS.amount), /33 %/);
  assert.equal(r.document.activeElement?.id, draw.IDS.preset, 'the focus goes back to the menu');
  await r.click(draw.IDS.model);
  options = [...r.document.querySelectorAll('.fi-opt')].map((o) => o.textContent);
  assert.deepEqual(options, ['Bauer', 'Mid/Side bass']);
  pick(r, 'Mid/Side bass');
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { model: 'mid-side', done: true });
  assert.equal(r.words(draw.IDS.delay), 'none');
  assert.ok(r.$(draw.IDS.page).innerHTML.includes('fpx-grey'));
  r.window.close();
});

await ok('a_hold_repeats_with_done_false_and_ends_with_one_done_true', async () => {
  const r = rig();
  await r.open();
  const id = draw.stepId('amount_pct', 1);
  press(r, id, 'pointerdown');
  assert.deepEqual(r.sent(), [], 'nothing before the hold begins');
  r.fire(HOLD_MS);
  await r.calm();
  r.fire(REPEAT_MS);
  await r.calm();
  r.fire(REPEAT_MS);
  await r.calm();
  const during = r.sent();
  assert.ok(during.length >= 3 && during.every((body) => body.done === false), JSON.stringify(during));
  assert.deepEqual(during.map((body) => body.amount_pct), during.map((_, i) => 38 + i));
  press(r, id, 'pointerup');
  r.$(id).click();
  await r.calm();
  const all = r.sent();
  assert.equal(all.at(-1).done, true, 'the last is done');
  assert.equal(all.filter((body) => body.done).length, 1, 'only the last');
  assert.equal(all.at(-1).amount_pct, all.at(-2).amount_pct, 'at the value reached');
  assert.equal(r.timers.filter((t) => t.live).length, 0, 'no timer is left');
  r.window.close();
});

await ok('a_press_that_ends_before_the_hold_is_one_step_and_a_key_press_is_one_too', async () => {
  const r = rig();
  await r.open();
  const id = draw.stepId('crossover_hz', -1);
  press(r, id, 'pointerdown');
  press(r, id, 'pointerup');
  r.$(id).click();
  await r.calm();
  assert.deepEqual(r.sent(), [{ crossover_hz: 690, done: true }]);
  assert.equal(r.timers.filter((t) => t.live).length, 0, 'the hold\'s timer was cleared');
  r.$(id).focus();
  r.key('Enter');
  r.$(id).click();
  await r.calm();
  assert.equal(r.sent().length, 2);
  r.window.close();
});

await ok('the_newest_value_wins_and_fields_asked_meanwhile_go_in_one_request', async () => {
  const r = rig();
  const gate = {};
  gate.promise = new Promise((resolve) => { gate.resolve = resolve; });
  let first = true;
  r.daemon.answer = () => {
    if (first) {
      first = false;
      return gate.promise.then(() => ({ status: 200, body: {}, apply: true }));
    }
    return { status: 200, body: {}, apply: true };
  };
  await r.open();
  r.$(draw.stepId('amount_pct', 1)).click();
  await r.calm();
  assert.equal(r.sent().length, 1, 'one in flight');
  r.core.actions.computer.setCrossfeed({ amount_pct: 41 }, { done: false });
  r.core.actions.computer.setCrossfeed({ amount_pct: 42 }, { done: false });
  r.core.actions.computer.setCrossfeed({ crossover_hz: 900 }, { done: true });
  await r.calm();
  assert.equal(r.sent().length, 1, 'still one: the others wait');
  assert.match(r.words(draw.IDS.amount), /42 %/, 'the page shows what is asked');
  gate.resolve();
  await r.calm();
  assert.deepEqual(r.sent(), [
    { amount_pct: 38, done: true }, { amount_pct: 42, crossover_hz: 900, done: true },
  ]);
  assert.match(r.words(draw.IDS.amount), /42 %/);
  assert.equal(r.words(draw.IDS.crossover), '900 Hz');
  r.window.close();
});

await ok('a_model_or_preset_picked_after_a_queued_number_replaces_it_and_a_number_after_a_pick_goes_on_top', async () => {
  const hold = async (r) => {
    const gate = {};
    gate.promise = new Promise((resolve) => { gate.resolve = resolve; });
    let first = true;
    r.daemon.answer = () => {
      if (!first) return { status: 200, body: {}, apply: true };
      first = false;
      return gate.promise.then(() => ({ status: 200, body: {}, apply: true }));
    };
    await r.open();
    r.$(draw.IDS.switch).click();
    await r.calm();
    return gate;
  };
  // Level + queued, then a preset: the preset is the latest intent
  let r = rig();
  let gate = await hold(r);
  r.core.actions.computer.setCrossfeed({ amount_pct: 38 });
  r.core.actions.computer.setCrossfeed({ preset: 'Chu Moy' });
  gate.resolve();
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { preset: 'Chu Moy', done: true });
  assert.equal(r.words(draw.IDS.preset), 'Chu Moy');
  r.window.close();
  // Delay + queued, then a model that plays none: the model goes through alone
  r = rig();
  gate = await hold(r);
  r.core.actions.computer.setCrossfeed({ delay_ms: 0.3 });
  r.core.actions.computer.setCrossfeed({ model: 'mid-side' });
  gate.resolve();
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { model: 'mid-side', done: true });
  assert.equal(r.words(draw.IDS.model), 'Mid/Side bass');
  r.window.close();
  // a preset after a model replaces the model's default; a number after a pick applies on top
  r = rig();
  gate = await hold(r);
  r.core.actions.computer.setCrossfeed({ model: 'bauer' });
  r.core.actions.computer.setCrossfeed({ preset: 'Chu Moy' });
  r.core.actions.computer.setCrossfeed({ crossover_hz: 900 });
  gate.resolve();
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { model: 'bauer', preset: 'Chu Moy', crossover_hz: 900, done: true });
  r.window.close();
});

await ok('exact_values_step_from_the_views_number_and_show_the_snapped_one', async () => {
  const r = rig({ view: VIEW({ amount_pct: 37.33, amount_db: -8.6, delay_ms: 0.265, delay_floor_ms: 0.265 }) });
  await r.open();
  r.$(draw.stepId('amount_pct', 1)).click();
  await r.calm();
  assert.deepEqual(r.sent(), [{ amount_pct: 38.33, done: true }]);
  r.window.close();
  const d = draw.shownView(VIEW({ delay_ms: 0.27, delay_floor_ms: 0.27 }), { delay_ms: 0.29 });
  assert.equal(d.delay_own, false);
  assert.equal(draw.stepped(VIEW({ delay_ms: 0.27, delay_floor_ms: 0.27 }), 'delay_ms', 1), 0.29);
});

await ok('under_a_new_model_the_old_preset_is_not_shown_before_the_answer', async () => {
  const shown = draw.shownView(VIEW(), { model: 'mid-side' });
  assert.equal(shown.preset, 'Mono bass');
  assert.equal(shown.crossover_hz, 120);
  assert.equal(shown.delay_settable, false);
  assert.equal(shown.amount_db, null);
  const same = draw.shownView(VIEW(), { preset: 'Chu Moy' });
  assert.equal(same.amount_pct, 33);
  // the model's default preset, not the first of its list (the list runs mildest to strongest)
  const back = draw.shownView(VIEW({ model: 'mid-side', preset: 'Mono bass' }), { model: 'bauer' });
  assert.equal(back.preset, 'bs2b default');
});

await ok('picking_the_current_model_keeps_queued_numbers_and_a_preset_then_a_number_shows_custom', async () => {
  const r = rig();
  const gate = {};
  gate.promise = new Promise((resolve) => { gate.resolve = resolve; });
  let first = true;
  r.daemon.answer = () => {
    if (!first) return { status: 200, body: {}, apply: true };
    first = false;
    return gate.promise.then(() => ({ status: 200, body: {}, apply: true }));
  };
  await r.open();
  r.$(draw.IDS.switch).click();
  await r.calm();
  r.core.actions.computer.setCrossfeed({ amount_pct: 40 });
  r.core.actions.computer.setCrossfeed({ model: 'bauer' });
  gate.resolve();
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { amount_pct: 40, model: 'bauer', done: true }, 'a same-model pick is no change');
  r.window.close();
  const shown = draw.shownView(VIEW(), { preset: 'Chu Moy', amount_pct: 35 });
  assert.equal(shown.preset, null);
  assert.equal(shown.edited_from, 'Chu Moy');
});

await ok('the_answer_and_the_stream_update_in_either_order_leave_the_views_value', async () => {
  for (const streamFirst of [true, false]) {
    const r = rig();
    const gate = {};
    gate.promise = new Promise((resolve) => { gate.resolve = resolve; });
    r.daemon.answer = () => gate.promise.then(() => ({ status: 200, body: {}, apply: true }));
    await r.open();
    r.$(draw.stepId('amount_pct', 1)).click();
    await r.calm();
    assert.match(r.words(draw.IDS.amount), /38 %/, 'asked, shown at once');
    if (streamFirst) {
      r.apply({ amount_pct: 38 });
      await r.stream();
      gate.resolve();
    } else {
      gate.resolve();
      await r.calm();
      await r.stream();
    }
    await r.calm();
    assert.match(r.words(draw.IDS.amount), /38 % · −8\.4 dB/, `the view's own level, stream first: ${streamFirst}`);
    // another front end changes it meanwhile: the page follows
    r.apply({ amount_pct: 20 });
    await r.stream();
    assert.match(r.words(draw.IDS.amount), /20 %/);
    r.window.close();
  }
});

await ok('a_refusal_shows_in_the_tabs_words_the_values_go_back_and_the_next_change_clears_it', async () => {
  const r = rig();
  r.daemon.answer = () => ({ status: 422, body: { error: 'the crossover is out of range' } });
  await r.open();
  await r.click(draw.stepId('crossover_hz', 1));
  assert.equal(r.words(draw.IDS.words), 'the crossover is out of range');
  assert.equal(r.words(draw.IDS.crossover), '700 Hz', 'what the daemon holds');
  assert.equal(r.document.querySelectorAll('.toast, [data-toast]').length, 0, 'no toast: it is the tab\'s');
  r.daemon.answer = null;
  await r.click(draw.stepId('crossover_hz', 1));
  assert.equal(r.$(draw.IDS.words), null);
  assert.equal(r.words(draw.IDS.crossover), '710 Hz');
  r.window.close();
});

await ok('a_404_and_no_answer_are_refusals_in_words_and_nothing_retries', async () => {
  for (const answer of [{ status: 404, body: {} }, { status: 0, body: {} }]) {
    const r = rig();
    r.daemon.answer = () => answer;
    await r.open();
    await r.click(draw.IDS.switch);
    const said = r.words(draw.IDS.words);
    assert.ok(said.length > 0, `words for ${answer.status}`);
    assert.equal(r.sent().length, 1, 'no retry');
    assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true', 'back to what it holds');
    r.window.close();
  }
});

await ok('a_guest_and_a_page_with_writes_off_have_no_way_in', async () => {
  for (const viewer of [{ guest: true, signed_in: false }, { writes: false }]) {
    const r = rig({ viewer });
    await r.open();
    assert.equal(r.$('popup-sound'), null, 'nothing is drawn');
    assert.equal(r.core.router.state().popup, null);
    assert.deepEqual(r.sent(), []);
    r.window.close();
  }
});

await ok('a_page_whose_host_offers_no_crossfeed_says_so_and_has_no_switch', async () => {
  const r = rig({ view: null });
  r.daemon.view = undefined;
  await r.open();
  assert.match(r.words(draw.IDS.page), /not offered/);
  assert.equal(r.$(draw.IDS.switch), null);
  const none = rig({ computer: false });
  await none.open();
  assert.match(none.words(draw.IDS.page), /not on/);
  r.window.close();
  none.window.close();
});

await ok('escape_closes_the_list_first_then_the_popup_and_a_hold_in_flight_ends', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.model);
  assert.ok(r.document.querySelector('.fi-list'));
  r.key('Escape');
  await r.calm();
  assert.equal(r.document.querySelector('.fi-list'), null);
  assert.equal(r.core.router.state().popup, SOUND);
  const id = draw.stepId('amount_pct', 1);
  press(r, id, 'pointerdown');
  r.fire(HOLD_MS);
  await r.calm();
  r.key('Escape');
  await r.calm();
  assert.equal(r.core.router.state().popup, null);
  assert.equal(r.sent().at(-1).done, true, 'a hold the popup closed on ends with done: true');
  assert.equal(r.timers.filter((t) => t.live).length, 0);
  r.window.close();
});

await ok('the_tab_dot_follows_the_stream_and_a_redraw_keeps_the_focus', async () => {
  const r = rig();
  await r.open();
  r.$(draw.stepId('amount_pct', 1)).focus();
  r.apply({ on: false });
  await r.stream();
  assert.equal(r.$(sound.tabId('xf')).querySelector('.fp-dot'), null);
  assert.equal(r.document.activeElement?.id, draw.stepId('amount_pct', 1));
  r.window.close();
});

await ok('an_open_menu_stays_open_through_a_stream_that_changes_nothing_it_shows', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.model);
  assert.equal(r.$(draw.IDS.model).getAttribute('aria-expanded'), 'true');
  // the stream sends the computer again (its player moved on, say): the crossfeed is the same
  await r.stream();
  assert.deepEqual([...r.document.querySelectorAll('.fi-opt')].map((o) => o.textContent),
    ['Bauer', 'Mid/Side bass'], 'the list is still open');
  pick(r, 'Mid/Side bass');
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { model: 'mid-side', done: true });
  assert.notEqual(r.$(draw.IDS.model).getAttribute('aria-expanded'), 'true', 'the list is closed');
  // and a stream after the list closed does not draw the page again for the opener's state
  const before = r.$(draw.IDS.model);
  await r.stream();
  assert.equal(r.$(draw.IDS.model), before, 'the menu button is the same element');
  r.window.close();
});

console.log(`core-crossfeed: ${n} cases passed`);
