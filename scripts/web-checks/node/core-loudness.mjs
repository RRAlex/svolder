/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Sound popup's Loudness tab (core/popups/loudness/{draw,wire}.js, the tabs
// of core/popups/sound/, and the loudness lane (`setComputerLoudness`) of core/computer-actions.js) on the real core over
// a jsdom page and a small fake daemon whose loudness answers the way the host's contract says
// (`LoudnessView`, `HostEqOp::Loudness`). First the draws, as strings, from views: the worked
// example (35 % of a 100 % reference: depth 13.7, bass +7.3, mids −7.3, treble +2.1), at the
// reference, off, the faint combined line (only while the EQ plays), unavailable, none at all;
// the steppers (range and step from the view, a held value laid over it); then the wire: the tab
// order (EQ library, Loudness, Crossfeed, Virtual speakers, Loudness only while the host offers
// it), the slider (`done: false` while dragged, `done: true` on release, never drawn again under
// the hand), the reference (a press, a hold, "Use current volume"), the answer of a write and the
// stream's update in either order, a refusal in the tab's own words, a 404 from an older daemon,
// the DAC's own loudness hint, the guest with no way in. Run: node core-loudness.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { LOUDNESS, SOUND } = await import('../../../bins/svolderd/web/core/popups/listed.js');
const draw = await import('../../../bins/svolderd/web/core/popups/loudness/draw.js');
const { HOLD_MS, REPEAT_MS } = await import(
  '../../../bins/svolderd/web/core/popups/loudness/wire.js');
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

const HZ = [];
for (let i = 0; i < 61; i++) HZ.push(Math.round(20 * 10 ** (i / 20) * 100) / 100);
/** A cut-only curve in the shape of the stage: the bass held, the middle cut, the top between. */
const curveFor = (lift, mids, treble) => HZ.map((f) => {
  const low = 1 / (1 + (f / 150) ** 2);
  const high = 1 - 1 / (1 + (f / 4000) ** 2);
  return Math.round((mids + lift * low + treble * high) * 100) / 100;
});
const range = (min, max, step) => ({ min, max, step });
/** The worked example: reference 100 %, strength 35 %, volume 35 %. */
const VIEW = (over = {}) => ({
  on: true, strength_pct: 35, reference_pct: 100, volume_pct: 35, depth_db: 13.7, lift_db: 7.3,
  mids_db: -7.3, treble_db: 2.1, following: true, note: null, unavailable: null,
  curve: { hz: HZ, db: curveFor(7.3, -7.3, 2.1) },
  ranges: { strength_pct: range(0, 100, 5), reference_pct: range(1, 100, 1) }, ...over,
});
const FLAT_VIEW = (over = {}) => VIEW({
  volume_pct: 100, depth_db: 0, lift_db: 0, mids_db: 0, treble_db: 0,
  curve: { hz: HZ, db: HZ.map(() => 0) }, ...over,
});
const text = (html) => html.replace(/<[^>]*>/g, ' ').replaceAll('&#39;', "'").replace(/\s+/g, ' ').trim();
const EQ = (over = {}) => ({
  on: true, points: [{ type: 'PK', f: 1000, g: 4, q: 1 }],
  response: Array.from({ length: 181 }, (_, i) => (i > 90 && i < 115 ? 4 : 0)), ...over,
});

// ---- part 1: the draws ----

await ok('the_page_draws_the_views_numbers_and_words_and_works_out_none_of_its_own', async () => {
  const html = draw.pageHtml({ ld: VIEW() });
  const words = text(html);
  for (const part of ['35 %', '100 %', 'Use current volume (35 %)', 'Volume now', 'Depth', '13.7 dB',
    'Bass', '+7.3 dB', 'Mids', '−7.3 dB', 'Treble, against the mids', '+2.1 dB',
    'Playing at 35 % of a 100 % reference: following the volume', 'It only cuts']) {
    assert.ok(words.includes(part), `${part} is on the page: ${words}`);
  }
  assert.ok(!html.includes('fpl-off'), 'on is not dimmed');
  assert.ok(html.includes('type="range"') && html.includes('min="0" max="100" step="5"'),
    'the slider\'s range is the view\'s');
  assert.ok(html.includes('fpl-line') && html.includes('fpl-zero'), 'the curve and the 0 dB line');
  assert.ok(!html.includes('fpl-with'), 'no second line while there is no EQ');
});

await ok('at_the_reference_it_says_flat_and_with_a_zero_depth_no_minus_is_drawn', async () => {
  const words = text(draw.pageHtml({ ld: FLAT_VIEW() }));
  assert.ok(words.includes('Playing at the reference: flat, nothing is cut'), words);
  assert.ok(words.includes('Mids 0.0 dB') && !words.includes('−0.0'), words);
  assert.ok(words.includes('Depth 0.0 dB'));
});

await ok('off_is_dimmed_and_still_editable_and_says_so', async () => {
  const html = draw.pageHtml({ ld: VIEW({ on: false, following: false }) });
  assert.ok(html.includes('fpl fpl-off'));
  assert.ok(text(html).includes('Off: the sound is not changed.'));
  assert.ok(!/type="range"[^>]*disabled/.test(html), 'the slider still moves');
  assert.ok(!/id="popup-sound-ld-reference_pct-less"[^>]*aria-disabled/.test(html));
});

await ok('a_stage_that_does_not_follow_says_the_hosts_note_else_its_own_words', async () => {
  assert.ok(text(draw.pageHtml({ ld: VIEW({ following: false, note: 'waiting for the volume' }) }))
    .includes('waiting for the volume'));
  assert.ok(text(draw.pageHtml({ ld: VIEW({ following: false, volume_pct: null }) }))
    .includes('Not following the volume now.'));
  const html = draw.pageHtml({ ld: VIEW({ following: false, volume_pct: null }) });
  assert.ok(text(html).includes('not read yet'), 'the volume row says it is unread');
  assert.match(html, /data-act="now"[^>]*aria-disabled="true"/, 'no volume to take');
  assert.ok(text(html).includes('Use current volume'));
});

await ok('the_faint_line_is_the_stage_and_the_eq_together_and_only_while_the_eq_plays', async () => {
  const eq = EQ();
  const both = draw.combined(VIEW(), eq);
  assert.equal(both.length, 61);
  const at1k = HZ.findIndex((f) => f >= 1000);
  assert.ok(Math.abs(both[at1k] - (VIEW().curve.db[at1k] + 4)) < 1e-9, 'the sum at 1 kHz');
  const html = draw.pageHtml({ ld: VIEW(), eq });
  assert.ok(html.includes('fpl-with') && html.includes('With the EQ'));
  assert.equal(draw.combined(VIEW(), EQ({ on: false })), null);
  assert.equal(draw.combined(VIEW(), EQ({ points: [] })), null);
  assert.equal(draw.combined(VIEW(), EQ({ response: [] })), null);
  assert.ok(!draw.pageHtml({ ld: VIEW(), eq: EQ({ on: false }) }).includes('fpl-with'));
});

await ok('a_boosting_eq_widens_the_picture_instead_of_leaving_it_and_the_stage_stays_put', async () => {
  const flat = draw.pictureHtml(VIEW(), null);
  const boost = draw.pictureHtml(VIEW(), EQ({ response: new Array(181).fill(10) }));
  const zero = (html) => /fpl-zero" d="M0 ([\d.]+)L/.exec(html)[1];
  assert.notEqual(zero(flat), zero(boost), 'the 0 dB line moves down for a +10 dB line');
  assert.ok(Number(zero(boost)) > Number(zero(flat)));
});

await ok('a_view_that_is_unavailable_says_so_and_keeps_the_controls_live_and_a_missing_one_says_none', async () => {
  const html = draw.pageHtml({ ld: VIEW({ unavailable: 'computer controls unavailable: no sink' }) });
  assert.ok(text(html).includes('computer controls unavailable: no sink'));
  assert.ok(!/type="range"[^>]*disabled/.test(html), 'the slider stays live');
  assert.ok(!/id="popup-sound-ld-reference_pct-less"[^>]*aria-disabled/.test(html), 'a step stays live');
  const header = draw.switchHtml(VIEW({ unavailable: 'computer controls unavailable: no sink' }));
  assert.ok(/data-act="on"/.test(header) && !/aria-disabled/.test(header), 'the header switch stays live');
  assert.ok(text(draw.pageHtml({ ld: null })).includes('not offered'));
  assert.ok(text(draw.pageHtml({ ld: null, computer: false })).includes('not on'));
});

await ok('a_hostile_note_is_escaped_and_the_dac_hint_shows_only_when_asked', async () => {
  const html = draw.pageHtml({ ld: VIEW({ following: false, note: '<img src=x onerror=alert(1)>' }) });
  assert.ok(!/<img/.test(html), html);
  assert.ok(!text(draw.pageHtml({ ld: VIEW() })).includes('The DAC\'s loudness is on too.'));
  assert.ok(text(draw.pageHtml({ ld: VIEW(), dac: true })).includes('The DAC\'s loudness is on too.'));
});

await ok('steps_come_from_the_views_range_and_step_and_stop_at_its_ends', async () => {
  const ld = VIEW();
  assert.equal(draw.stepped(ld, 'reference_pct', 1), null, 'at the top');
  assert.equal(draw.stepped(ld, 'reference_pct', -1), 99);
  assert.equal(draw.stepped({ ...ld, reference_pct: 1 }, 'reference_pct', -1), null);
  const wide = VIEW({ ranges: { strength_pct: range(0, 100, 5), reference_pct: range(10, 90, 10) } });
  assert.equal(draw.stepped({ ...wide, reference_pct: 50 }, 'reference_pct', 1), 60);
  assert.equal(draw.stepped(VIEW({ ranges: undefined }), 'reference_pct', 1), null, 'no range, no step');
});

await ok('what_is_asked_is_laid_over_the_view_and_the_readout_stays_the_hosts', async () => {
  const shown = draw.shownView(VIEW(), { strength_pct: 60, reference_pct: 80, on: false });
  assert.equal(shown.strength_pct, 60);
  assert.equal(shown.reference_pct, 80);
  assert.equal(shown.on, false);
  assert.equal(shown.depth_db, 13.7, 'a number the host works out is not asked');
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
const deviceModel = (name, mode = 'eq') => ({
  name, url: seg(name), link: link(true), volume: null, controls: [], meters: null,
  tone: { response: FLAT, mode },
  presets: { status: { lead: '', name: null, tail: '' }, editable: true, recall: true, note: null, rows: [] },
});

/** The real core; the daemon is the page's `fetch`, its loudness a little state that applies the
 * op's fields the way the host does (a strength snapped to its step, `reference_now` taking the
 * volume it reads). `daemon.answer` may give `{ status, body }` (or a promise) in place of the
 * default for a call; `daemon.view` is the loudness it holds (`undefined`: a host that has none). */
function rig({
  viewer = {}, view = VIEW(), computer = true, eq = null, crossfeed = false, dac = 'eq',
} = {}) {
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
    eq: {
      on: true, points: [], max: 16, ...(eq ?? {}),
      ...(daemon.view === undefined ? {} : { loudness: daemon.view }),
      ...(crossfeed ? { crossfeed: { on: false } } : {}),
    },
  });
  const apply = (op) => {
    const next = { ...daemon.view };
    if (typeof op.strength_pct === 'number') next.strength_pct = Math.round(op.strength_pct / 5) * 5;
    if (typeof op.reference_pct === 'number') next.reference_pct = op.reference_pct;
    if (op.reference_now) next.reference_pct = Math.round(next.volume_pct ?? next.reference_pct);
    if (typeof op.on === 'boolean') next.on = op.on;
    daemon.view = next;
  };
  p.window.fetch = (path, init = {}) => new Promise((resolve) => {
    const reply = (status, body) => resolve({ status, ok: status < 400, json: async () => body });
    const body = init.body && typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call = { path, body };
    daemon.calls.push(call);
    const model = /^api\/v1\/devices\/([^/]+)\/model/.exec(path);
    if (model) {
      reply(200, { device: deviceModel(decodeURIComponent(model[1]), dac) });
      return;
    }
    const given = daemon.answer?.(call);
    if (path === 'api/v1/host/loudness') {
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
      shown: 'A', device: deviceModel('A', dac), followed: {}, card: null,
      host: computer ? { track: null, route: null, computer: computerNow() } : null, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 10; i++) await p.settle(); };
  const open = async (to = LOUDNESS) => {
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
  const sent = () => daemon.calls.filter((c) => c.path === 'api/v1/host/loudness').map((c) => c.body);
  const fire = (ms) => { for (const t of timers.splice(0)) if (t.live && t.ms === ms) t.fn(); };
  /** The slider at `value`, as a hand moves it: an `input`; its release is a `change`. */
  const slide = async (value, { release = false, press = false } = {}) => {
    const slider = $(draw.IDS.strength);
    if (press) slider.dispatchEvent(new p.window.MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    slider.value = String(value);
    slider.dispatchEvent(new p.window.Event('input', { bubbles: true }));
    if (release) slider.dispatchEvent(new p.window.Event('change', { bubbles: true }));
    await calm();
  };
  return {
    ...p, core, daemon, $, calm, open, click, stream, sent, fire, timers, apply, slide,
    words: (id) => ($(id)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
  };
}

const press = (r, id, type) => r.$(id).dispatchEvent(new r.window.MouseEvent(type, {
  bubbles: true, button: 0,
}));

await ok('the_tabs_follow_the_chain_and_loudness_is_second_and_only_while_the_host_offers_it', async () => {
  const r = rig();
  await r.open();
  assert.deepEqual([...r.document.querySelectorAll('.fp-tab')].map((t) => t.textContent.trim()),
    ['EQ library', 'Loudness', 'Crossfeed', 'Virtual speakers']);
  assert.equal(r.$(sound.tabId('ld')).getAttribute('aria-selected'), 'true');
  assert.equal(r.document.querySelector('.fp-pills'), null, 'the pills are the EQ library tab\'s');
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true');
  assert.equal(r.$('popup-sound-title').textContent, 'Sound');
  assert.match(r.words(draw.IDS.page), /Depth\s*13\.7 dB/);
  r.window.close();
  const none = rig({ view: null });
  none.daemon.view = undefined;
  await none.open(null);
  assert.deepEqual([...none.document.querySelectorAll('.fp-tab')].map((t) => t.textContent.trim()),
    ['EQ library', 'Crossfeed', 'Virtual speakers'], 'a host without loudness has no tab');
  none.window.close();
});

await ok('a_page_opened_at_loudness_on_a_host_without_it_says_so_and_has_no_switch', async () => {
  const r = rig({ view: null });
  r.daemon.view = undefined;
  await r.open();
  assert.match(r.words(draw.IDS.page), /not offered/);
  assert.equal(r.$(draw.IDS.switch), null);
  assert.ok(r.$(sound.tabId('ld')), 'the tab shown stays, so the strip does not lie');
  const none = rig({ computer: false });
  await none.open();
  assert.match(none.words(draw.IDS.page), /not on/);
  r.window.close();
  none.window.close();
});

await ok('the_tabs_step_the_router_and_the_arrows_go_in_chain_order', async () => {
  const r = rig();
  await r.open(null);
  assert.equal(r.$(sound.tabId('eq')).getAttribute('aria-selected'), 'true');
  r.$(sound.tabId('eq')).focus();
  r.key('ArrowRight');
  await r.calm();
  assert.equal(r.core.router.state().page, LOUDNESS);
  assert.equal(r.document.activeElement?.id, sound.tabId('ld'));
  r.key('ArrowRight');
  await r.calm();
  assert.equal(r.core.router.state().page, 'crossfeed');
  r.key('ArrowLeft');
  await r.calm();
  r.key('ArrowLeft');
  await r.calm();
  assert.equal(r.document.activeElement?.id, sound.tabId('eq'));
  await r.click(sound.tabId('ld'));
  assert.equal(r.core.router.state().page, LOUDNESS);
  await r.click(sound.tabId('eq'));
  assert.equal(r.core.router.state().page, 'autoeq');
  r.window.close();
  const without = rig({ view: null });
  without.daemon.view = undefined;
  await without.open(null);
  without.$(sound.tabId('eq')).focus();
  without.key('ArrowRight');
  await without.calm();
  assert.equal(without.core.router.state().page, 'crossfeed', 'no Loudness to stop at');
  without.window.close();
});

await ok('the_switch_turns_it_on_and_off_the_header_and_the_tab_dot_follow_the_view', async () => {
  const r = rig();
  await r.open();
  assert.ok(r.$(sound.tabId('ld')).querySelector('.fp-dot.on'));
  await r.click(draw.IDS.switch);
  assert.deepEqual(r.sent(), [{ on: false, done: true }]);
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'false');
  assert.ok(r.$(draw.IDS.page).classList.contains('fpl-off'));
  assert.equal(r.$(sound.tabId('ld')).querySelector('.fp-dot'), null, 'no dot when off');
  await r.click(draw.IDS.switch);
  assert.deepEqual(r.sent().at(-1), { on: true, done: true });
  assert.ok(r.$(sound.tabId('ld')).querySelector('.fp-dot.on'));
  r.window.close();
});

await ok('a_drag_is_done_false_a_release_is_one_done_true_and_the_slider_is_not_drawn_under_the_hand', async () => {
  const r = rig();
  await r.open();
  const slider = r.$(draw.IDS.strength);
  await r.slide(40, { press: true });
  await r.slide(45);
  await r.slide(50);
  const during = r.sent();
  assert.ok(during.length >= 1 && during.every((b) => b.done === false), JSON.stringify(during));
  assert.equal(r.words(draw.IDS.strengthValue), '50 %', 'the number follows the hand');
  // the host's stream arrives mid-drag with new readings: they are drawn, the slider is the same element
  r.daemon.view = { ...r.daemon.view, volume_pct: 30, depth_db: 15.4 };
  await r.stream();
  assert.equal(r.$(draw.IDS.strength), slider, 'the slider is the hand\'s');
  assert.match(r.words(draw.IDS.readout), /Depth\s*15\.4 dB/, 'the readout follows the host');
  await r.slide(50, { release: true });
  r.window.dispatchEvent(new r.window.MouseEvent('pointerup', { bubbles: true }));
  const all = r.sent();
  assert.equal(all.at(-1).done, true);
  assert.equal(all.filter((b) => b.done).length, 1, 'only the last');
  assert.equal(all.at(-1).strength_pct, 50);
  assert.equal(r.words(draw.IDS.strengthValue), '50 %');
  assert.equal(r.$(draw.IDS.strength).value, '50');
  r.window.close();
});

await ok('a_key_on_the_slider_is_one_request_that_is_done', async () => {
  const r = rig();
  await r.open();
  await r.slide(40, { release: true });
  assert.deepEqual(r.sent(), [{ strength_pct: 40, done: true }], 'exactly one request, no done: false');
  assert.equal(r.words(draw.IDS.strengthValue), '40 %');
  r.window.close();
});

await ok('a_press_on_the_reference_is_one_request_and_a_hold_repeats_done_false_and_ends_done', async () => {
  const r = rig();
  await r.open();
  const less = draw.stepId('reference_pct', -1);
  await r.click(less);
  assert.deepEqual(r.sent(), [{ reference_pct: 99, done: true }]);
  assert.match(r.words(draw.IDS.reference), /99 %/);
  r.daemon.calls.length = 0;
  press(r, less, 'pointerdown');
  assert.deepEqual(r.sent(), [], 'nothing before the hold begins');
  r.fire(HOLD_MS);
  await r.calm();
  r.fire(REPEAT_MS);
  await r.calm();
  const during = r.sent();
  assert.ok(during.length >= 2 && during.every((b) => b.done === false), JSON.stringify(during));
  press(r, less, 'pointerup');
  r.$(less).click();
  await r.calm();
  const all = r.sent();
  assert.equal(all.at(-1).done, true);
  assert.equal(all.filter((b) => b.done).length, 1);
  assert.equal(all.at(-1).reference_pct, all.at(-2).reference_pct, 'at the value reached');
  assert.equal(r.timers.filter((t) => t.live).length, 0, 'no timer is left');
  r.window.close();
});

await ok('use_current_volume_asks_the_host_and_the_reference_shown_is_the_hosts', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.now);
  assert.deepEqual(r.sent(), [{ reference_now: true, done: true }]);
  assert.match(r.words(draw.IDS.reference), /35 %/, 'the volume the host read');
  assert.equal(r.$(draw.IDS.strengthValue) !== null, true);
  r.window.close();
});

await ok('a_reference_asked_replaces_a_waiting_use_current_volume_and_the_reverse', async () => {
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
  r.$(draw.IDS.switch).click();
  await r.calm();
  r.$(draw.IDS.now).click();
  r.$(draw.stepId('reference_pct', -1)).click();
  await r.calm();
  r.$(draw.IDS.now).click();
  await r.calm();
  gate.resolve();
  await r.calm();
  const bodies = r.sent();
  assert.deepEqual(bodies.at(-1), { reference_now: true, done: true }, JSON.stringify(bodies));
  assert.ok(!('reference_pct' in bodies.at(-1)), 'the later intent replaces the earlier one');
  r.window.close();
  // the other way round: a reference asked after a waiting "use current volume" replaces it
  const again = rig();
  const hold = {};
  hold.promise = new Promise((resolve) => { hold.resolve = resolve; });
  let held = true;
  again.daemon.answer = () => {
    if (held) {
      held = false;
      return hold.promise.then(() => ({ status: 200, body: {}, apply: true }));
    }
    return { status: 200, body: {}, apply: true };
  };
  await again.open();
  again.$(draw.IDS.switch).click();
  await again.calm();
  again.$(draw.IDS.now).click();
  again.$(draw.stepId('reference_pct', -1)).click();
  await again.calm();
  hold.resolve();
  await again.calm();
  assert.deepEqual(again.sent().at(-1), { reference_pct: 99, done: true }, JSON.stringify(again.sent()));
  again.window.close();
  // and a field that is not the reference leaves a waiting "use current volume" alone
  const keep = rig();
  const wait = {};
  wait.promise = new Promise((resolve) => { wait.resolve = resolve; });
  let first2 = true;
  keep.daemon.answer = () => {
    if (first2) {
      first2 = false;
      return wait.promise.then(() => ({ status: 200, body: {}, apply: true }));
    }
    return { status: 200, body: {}, apply: true };
  };
  await keep.open();
  keep.$(draw.IDS.switch).click();
  await keep.calm();
  keep.$(draw.IDS.now).click();
  await keep.slide(60, { release: true });
  wait.resolve();
  await keep.calm();
  assert.deepEqual(keep.sent().at(-1), { reference_now: true, strength_pct: 60, done: true },
    JSON.stringify(keep.sent()));
  keep.window.close();
});

await ok('the_answer_and_the_stream_update_in_either_order_leave_the_views_value', async () => {
  for (const streamFirst of [true, false]) {
    const r = rig();
    const gate = {};
    gate.promise = new Promise((resolve) => { gate.resolve = resolve; });
    r.daemon.answer = () => gate.promise.then(() => ({ status: 200, body: {}, apply: true }));
    await r.open();
    r.$(draw.stepId('reference_pct', -1)).click();
    await r.calm();
    assert.match(r.words(draw.IDS.reference), /99 %/, 'asked, shown at once');
    if (streamFirst) {
      r.apply({ reference_pct: 99 });
      await r.stream();
      gate.resolve();
    } else {
      gate.resolve();
      await r.calm();
      await r.stream();
    }
    await r.calm();
    assert.match(r.words(draw.IDS.reference), /99 %/, `stream first: ${streamFirst}`);
    // another front end changes it meanwhile: the page follows
    r.apply({ reference_pct: 70, strength_pct: 80 });
    await r.stream();
    assert.match(r.words(draw.IDS.reference), /70 %/);
    assert.equal(r.words(draw.IDS.strengthValue), '80 %');
    assert.equal(r.$(draw.IDS.strength).value, '80');
    r.window.close();
  }
});

await ok('the_readout_follows_the_streams_volume_and_the_curve_with_it', async () => {
  const r = rig();
  await r.open();
  const before = r.$(draw.IDS.readout).querySelector('.fpl-line').getAttribute('d');
  r.daemon.view = FLAT_VIEW();
  await r.stream();
  assert.match(r.words(draw.IDS.readout), /Playing at the reference: flat/);
  assert.notEqual(r.$(draw.IDS.readout).querySelector('.fpl-line').getAttribute('d'), before);
  r.window.close();
});

await ok('a_refusal_shows_in_the_tabs_words_the_values_go_back_and_the_next_change_clears_it', async () => {
  const r = rig();
  r.daemon.answer = () => ({ status: 409, body: { error: 'the chain is not running, so there is no volume to take' } });
  await r.open();
  await r.click(draw.IDS.now);
  assert.equal(r.words(draw.IDS.words), 'the chain is not running, so there is no volume to take');
  assert.match(r.words(draw.IDS.reference), /100 %/, 'what the daemon holds');
  assert.equal(r.document.querySelectorAll('.toast, [data-toast]').length, 0, 'no toast: it is the tab\'s');
  r.daemon.answer = null;
  await r.click(draw.stepId('reference_pct', -1));
  assert.equal(r.$(draw.IDS.words), null);
  assert.match(r.words(draw.IDS.reference), /99 %/);
  r.window.close();
});

await ok('a_404_from_an_older_daemon_and_no_answer_are_refusals_in_words_and_nothing_retries', async () => {
  for (const answer of [{ status: 404, body: {} }, { status: 0, body: {} }]) {
    const r = rig();
    r.daemon.answer = () => answer;
    await r.open();
    await r.click(draw.IDS.switch);
    assert.ok(r.words(draw.IDS.words).length > 0, `words for ${answer.status}`);
    assert.equal(r.sent().length, 1, 'no retry');
    assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true', 'back to what it holds');
    r.window.close();
  }
});

await ok('the_dac_hint_shows_while_the_shown_devices_tone_is_loudness_and_goes_with_it', async () => {
  const r = rig({ dac: 'loudness' });
  await r.open();
  assert.match(r.words(draw.IDS.page), /The DAC's loudness is on too\./);
  r.window.close();
  const off = rig();
  await off.open();
  assert.ok(!off.words(draw.IDS.page).includes('DAC'));
  off.window.close();
});

await ok('the_faint_line_shows_with_the_eq_on_and_leaves_with_it', async () => {
  const r = rig({ eq: EQ() });
  await r.open();
  assert.ok(r.$(draw.IDS.readout).querySelector('.fpl-with'));
  assert.match(r.words(draw.IDS.readout), /With the EQ/);
  r.window.close();
  const off = rig({ eq: EQ({ on: false }) });
  await off.open();
  assert.equal(off.$(draw.IDS.readout).querySelector('.fpl-with'), null);
  off.window.close();
});

await ok('a_guest_and_a_page_with_writes_off_have_no_way_in', async () => {
  for (const viewer of [{ guest: true, signed_in: false }, { writes: false }]) {
    const r = rig({ viewer });
    await r.open();
    assert.equal(r.$('popup-sound'), null, 'nothing is drawn');
    assert.deepEqual(r.sent(), []);
    r.window.close();
  }
});

await ok('escape_closes_the_popup_and_a_hold_in_flight_ends_with_one_done_true', async () => {
  const r = rig();
  await r.open();
  const id = draw.stepId('reference_pct', -1);
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

await ok('a_stream_that_changes_nothing_it_shows_keeps_the_controls_and_the_focus', async () => {
  const r = rig();
  await r.open();
  r.$(draw.stepId('reference_pct', -1)).focus();
  const slider = r.$(draw.IDS.strength);
  await r.stream();
  assert.equal(r.$(draw.IDS.strength), slider, 'the same element');
  assert.equal(r.document.activeElement?.id, draw.stepId('reference_pct', -1));
  r.apply({ on: false });
  await r.stream();
  assert.equal(r.document.activeElement?.id, draw.stepId('reference_pct', -1), 'a redraw keeps the focus');
  r.window.close();
});

console.log(`core-loudness: ${n} cases passed`);
