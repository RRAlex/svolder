/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Devices popup (core/popups/devices/draw.js, wire.js). First the draws, as
// strings: the side list (Overview, a break, a dot per device, the tag on the shown one), a
// device's state in words, the order of the key graphs, a graph with its text alternative, a
// series with one reading or none said in words, a device that is not connected dimmed by a
// class, the Overview and a device's page. Then the wire on the real core over a jsdom page
// (dom.mjs), every case with another device on show than the one the page is about: each request
// names the page's device; the stream's address follows the listed devices and the store forgets
// them at the close; a device the fleet drops while its page stands; a device that connects or
// appears keeps the focus and what stands; *Show on the page* switches the shown device with no
// entry pushed and the popup standing; the timed re-read; a reader has the popup read-only.
// Run: node core-devices.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { readFileSync } = await import('node:fs');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { popupHtml } = await import('../../../bins/svolderd/web/core/popups/frame.js');
const { frameRows } = await import('../../../bins/svolderd/web/core/menu-rows.js');
const { DEVICES, POPUPS } = await import('../../../bins/svolderd/web/core/popups/listed.js');
const draw = await import('../../../bins/svolderd/web/core/popups/devices/draw.js');
const { OWNER, REREAD_MS } = await import('../../../bins/svolderd/web/core/popups/devices/wire.js');

let n = 0;
// WEB_CHECK_CASE=<part of a name> runs the cases whose name holds it (while working on one)
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};
console.warn = () => {};

const HOSTILE = '<img src=x onerror=alert(1)> "Q"';
const SAFE = '&lt;img src=x onerror=alert(1)&gt; &quot;Q&quot;';
const NOW = Date.UTC(2026, 10, 11, 14, 30, 0);
const MIN = 60000;

const volume = { key: 'volume', label: 'Volume', lo: 0, hi: 3, raw: 2, texts: ['mute', '-40 dB', '-20 dB', '0 dB'] };
const model = (name, over = {}) => ({
  name, url: name, link: { word: 'connected', state: 'on' }, volume,
  controls: [
    { kind: 'toggle', key: 'spdif_locked', label: 'Lock', on: true },
    { kind: 'readout', key: 'sample_rate', label: 'Sample rate', text: '44.1 kHz' },
  ],
  meters: null, ...over,
});
const answer = (over = {}) => ({
  now_ms: NOW, bin_ms: 1000, secs: 1800, refused: [], link: [],
  series: [
    { id: 'sample_rate', aggregate: 'last', points: [[NOW - 40 * MIN, 44100]] },
    { id: 'volume', aggregate: 'max', points: [[NOW - 40 * MIN, 1], [NOW - 20 * MIN, 3], [NOW - 5 * MIN, 2]] },
    { id: 'signal', unit: 'dBm', aggregate: 'last', points: [[NOW - 9 * MIN, -61]] },
    { id: 'spdif_locked', aggregate: 'last', points: [] },
  ],
  ...over,
});
const dev = (name, over = {}) => ({
  url: name, name, connected: true, since: null, shown: false, model: model(name), series: answer(),
  ...over,
});

// ---- the draws ----

await ok('the_side_list_is_overview_a_break_then_each_device_with_its_dot_and_the_shown_tag', () => {
  const entries = draw.entriesOf([
    dev('A', { shown: true }), dev('B', { connected: false }), dev(HOSTILE),
  ]);
  assert.deepEqual(entries.map((one) => [one.id, one.label, one.dot, one.dotWords, one.tag, one.group]), [
    ['overview', 'Overview', undefined, undefined, undefined, undefined],
    ['d-A', 'A', 'on', 'connected', 'Shown', true],
    ['d-B', 'B', 'off', 'not connected', undefined, undefined],
    [`d-${HOSTILE}`, HOSTILE, 'on', 'connected', undefined, undefined],
    ['add', '+ Add a device', undefined, undefined, undefined, true],
  ]);
  const html = popupHtml({ name: 'devices', title: 'Devices', entries, page: 'd-B' });
  assert.ok(html.includes('<hr class="fp-rule"><button type="button" class="fp-entry" id="popup-devices-entry-d-A"'), html);
  assert.ok(html.includes('<span class="fp-dot on" role="img" aria-label="connected"></span>A<span class="fp-tag">Shown</span>'));
  assert.ok(html.includes('<span class="fp-dot off" role="img" aria-label="not connected"></span>B</button>'));
  assert.ok(html.includes(SAFE) && !html.includes('<img'), 'a hostile name is escaped');
  assert.equal(html.split('fp-rule').length, 3, 'a break after Overview and one before Add');
  assert.equal(draw.entryInner(entries[1]),
    '<span class="fp-dot on" role="img" aria-label="connected"></span>A<span class="fp-tag">Shown</span>',
    'what the wire puts into a standing entry is what the frame draws in it');
  assert.deepEqual(draw.entriesOf([]).map((one) => one.id), ['overview', 'add'],
    'no device: Overview and the way to add one');
  assert.equal(draw.deviceOf(draw.pageOf('Gungnir%202')), 'Gungnir%202');
  assert.equal(draw.deviceOf('overview'), null);
  assert.equal(draw.deviceOf(null), null);
});

await ok('a_devices_state_is_said_in_words_with_since_when_the_daemon_knows', () => {
  assert.equal(draw.stateWords(dev('A')), 'connected');
  assert.equal(draw.stateWords(dev('A', { connected: false, since: NOW - 28 * MIN })),
    'not connected since 14:02');
  assert.equal(draw.stateWords(dev('A', { connected: false, since: null })), 'not connected');
  assert.equal(draw.clockWords(Date.UTC(2026, 0, 1, 3, 4)), '03:04');
  assert.equal(draw.stateHtml(dev('A', { connected: false, since: NOW - 28 * MIN })),
    '<span class="fp-dot off" aria-hidden="true"></span> not connected since 14:02',
    'the dot is an ornament beside the words');
});

await ok('the_graphs_stand_signal_first_then_in_the_profiles_order', () => {
  const ids = (list) => draw.ordered(list).map((one) => one.id);
  assert.deepEqual(ids(answer().series), ['signal', 'sample_rate', 'volume', 'spdif_locked']);
  assert.deepEqual(ids([
    { id: 'warming_up', aggregate: 'last' }, { id: 'left', aggregate: 'max' },
    { id: 'input', aggregate: 'last' }, { id: 'right', aggregate: 'max' },
  ]), ['warming_up', 'left', 'input', 'right'],
  'the profile decides: a reading joined by its largest goes before no state');
  assert.deepEqual(ids(undefined), []);
});

await ok('a_raw_reading_is_worded_through_the_models_controls', () => {
  const m = model('A');
  assert.equal(draw.readingWords({ id: 'volume' }, 3, m), '0 dB', "the volume's own words for the step");
  assert.equal(draw.readingWords({ id: 'volume' }, 7, m), '7', 'a step the model has no words for');
  assert.equal(draw.readingWords({ id: 'spdif_locked' }, 1, m), 'on');
  assert.equal(draw.readingWords({ id: 'spdif_locked' }, 0, m), 'off');
  assert.equal(draw.readingWords({ id: 'signal', unit: 'dBm' }, -61, m), '-61 dBm');
  assert.equal(draw.readingWords({ id: 'sample_rate' }, 44100, m), '44100', 'a readout keeps its number');
  assert.equal(draw.readingWords({ id: 'volume' }, 2, null), '2', 'no model: the number');
  assert.equal(draw.readingWords({ id: 'sample_rate' }, 44100, m, { current: true }), '44.1 kHz',
    "the reading now is the readout's own text");
  const picks = model('A', { controls: [{ kind: 'choice', key: 'input', label: 'Input',
    options: [{ label: 'USB', on: false }, { label: 'Coax', on: true }] }] });
  assert.equal(draw.readingWords({ id: 'input' }, 1, picks, { current: true }), 'Coax', 'and the option that is on');
  assert.equal(draw.readingWords({ id: 'input' }, 1, picks), '1', 'an earlier value of a choice has no words');
  assert.equal(draw.seriesName({ id: 'signal' }, m), 'Signal strength');
  assert.equal(draw.seriesName({ id: 'sample_rate' }, m), 'Sample rate');
  assert.equal(draw.seriesName({ id: 'unknown_key' }, m), 'unknown_key');
  const amp = model('Amp', { volume: null, meters: { meters: [{ key: 'left', channel: 'Left' }] } });
  assert.equal(draw.seriesName({ id: 'left' }, amp), 'Left');
});

await ok('a_controls_line_is_drawn_in_steps_from_the_spans_start_to_now', () => {
  const from = NOW - 30 * MIN;
  const one = answer().series[1];
  assert.deepEqual(draw.pointsOf(one, { now: NOW, from, connected: true }), [
    { t: from, v: 1 }, { t: NOW - 20 * MIN, v: 1 }, { t: NOW - 20 * MIN, v: 3 },
    { t: NOW - 5 * MIN, v: 3 }, { t: NOW - 5 * MIN, v: 2 }, { t: NOW, v: 2 },
  ]);
  assert.equal(draw.pointsOf(one, { now: NOW, from, connected: false }).at(-1).t, NOW - 5 * MIN,
    'a device that is not connected reads nothing on to now');
  assert.deepEqual(draw.pointsOf({ id: 'signal', points: [[from - 1, -70], [NOW - MIN, -60]] },
    { now: NOW, from, connected: true }), [{ t: NOW - MIN, v: -60 }], 'the signal is what was heard');
});

await ok('a_graph_has_its_name_its_last_reading_and_a_text_alternative', () => {
  const html = draw.graphHtml(answer().series[1], dev('A'), { now: NOW, secs: 1800 });
  assert.ok(html.startsWith('<figure class="fpd-graph" data-series="volume"><figcaption class="fpd-graph-head">'
    + '<span class="fpd-graph-name">Volume</span><span class="fpd-graph-last">-20 dB</span></figcaption>'), html);
  assert.ok(html.includes('<div class="fp-graph fpd-plot" role="img" '
    + 'aria-label="Volume, last 30 minutes: now -20 dB, between -40 dB and 0 dB">'), html);
  assert.ok(html.includes('<svg class="fpd-spark"') && html.includes('aria-hidden="true"'),
    'the plot is core/plot.js\'s, hidden from a screen reader: the box says it');
  assert.ok(html.includes('class="fpd-spark-line"'));
  assert.ok(html.includes('<p class="fpd-graph-range">-40 dB to 0 dB</p>'), 'the range in sight too');
  const away = draw.graphHtml(answer().series[1], dev('A', { connected: false }), { now: NOW, secs: 1800 });
  assert.ok(away.includes('last -20 dB'), 'a device that is not connected has a last reading, not a now');
  const hostile = draw.graphHtml({ id: HOSTILE, aggregate: 'last', points: [[NOW - 2, 1], [NOW - 1, 2]] },
    dev('A'), { now: NOW, secs: 1800 });
  assert.ok(hostile.includes(SAFE) && !hostile.includes('<img'));
});

await ok('a_series_with_one_reading_or_none_says_so_in_words_and_draws_no_box', () => {
  const span = { now: NOW, secs: 1800 };
  const none = draw.graphHtml(answer().series[3], dev('A'), span);
  assert.ok(none.includes('<span class="fpd-graph-name">Lock</span></figcaption>'
    + '<p class="fp-note">No reading in the last 30 minutes.</p>'), none);
  assert.ok(!none.includes('fp-graph') && !none.includes('<svg'));
  const late = draw.graphHtml({ id: 'signal', unit: 'dBm', aggregate: 'last',
    points: [[NOW - 20 * MIN, null], [NOW - 9 * MIN, -61]] }, dev('A'), span);
  assert.ok(late.includes('at 14:21.'), 'the time is the reading\'s, not an empty point\'s');
  const signal = draw.graphHtml(answer().series[2], dev('A'), span);
  assert.ok(signal.includes('<span class="fpd-graph-last">-61 dBm</span>'), signal);
  assert.ok(signal.includes('<p class="fp-note">One reading in the last 30 minutes, at 14:21.</p>'), signal);
  assert.ok(!signal.includes('<svg'));
  const held = draw.graphHtml(answer().series[0], dev('A'), span);
  assert.ok(held.includes('<span class="fpd-graph-last">44.1 kHz</span>'), held);
  assert.ok(draw.graphHtml(answer().series[0], dev('A', { connected: false }), span)
    .includes('<span class="fpd-graph-last">44100</span>'), 'not connected: the number that was read');
  assert.ok(held.includes('<p class="fp-note">No change in the last 30 minutes.</p>'), held);
});

await ok('a_devices_block_names_it_says_its_state_and_dims_one_that_is_not_connected', () => {
  const on = draw.deviceHtml(dev('A'));
  assert.ok(on.startsWith(`<section class="fpd-device" id="${draw.blockId('A')}" data-device="A">`
    + '<div class="fpd-head"><h3 class="fpd-name">A</h3></div>'
    + `<p class="fpd-state" id="${draw.stateId('A')}"><span class="fp-dot on" aria-hidden="true"></span> connected</p>`
    + `<div class="fpd-graphs" id="${draw.graphsId('A')}">`), on);
  const order = [...on.matchAll(/data-series="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(order, ['signal', 'sample_rate', 'volume', 'spdif_locked']);
  const off = draw.deviceHtml(dev('B', { connected: false, since: NOW - MIN }));
  assert.ok(off.startsWith('<section class="fpd-device fpd-away"'), 'dimmed by a class the sheet dresses');
  assert.ok(!/opacity|style=/.test(off), 'never by an opacity');
  assert.ok(draw.deviceHtml(dev('A', { series: null })).includes('Reading the last 30 minutes…'));
  assert.ok(draw.deviceHtml(dev('A', { series: { error: 'svolder did not answer.' } }))
    .includes('<p class="fp-note fp-failed">svolder did not answer.</p>'));
  assert.ok(draw.deviceHtml(dev('A', { series: answer({ series: [] }) })).includes('no key readings'));
  assert.notEqual(draw.domOf('a b'), draw.domOf('a%20b'));
  assert.match(draw.domOf(HOSTILE), /^[0-9a-f]+$/);
  const css = readFileSync(
    new URL('../../../bins/svolderd/web/core/frame.css', import.meta.url), 'utf8');
  for (const rule of css.matchAll(/^\.fpd-[^{]*\{([^}]*)\}/gm)) {
    assert.ok(!/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(|opacity/i.test(rule[1]),
      `a rule of the popup names no colour and no opacity of its own: ${rule[0]}`);
  }
  assert.match(css, /^\.fpd-away \.fpd-spark-line \{ stroke: var\(--frame-offline\); \}/m);
});

await ok('the_overview_is_every_device_in_order_and_a_page_is_one_with_show_on_the_page', () => {
  const devices = [dev('A', { shown: true }), dev('B'), dev('C', { connected: false })];
  const config = `<div id="${draw.CONFIG_ID}" role="status"></div>`;
  assert.ok(draw.pageHtml({ page: null, devices }).startsWith(`<div id="${draw.BODY_ID}">${config}<section`)
    && draw.pageHtml({ page: null, devices }).endsWith('</section></div>'),
  'the pane holds one place, which the wire puts when the devices in it change, and opens with the'
  + ' standing place of the config line');
  for (const at of [null, 'overview']) {
    const html = draw.pageHtml({ page: at, devices });
    assert.deepEqual([...html.matchAll(/data-device="([^"]+)"/g)].map((m) => m[1]), ['A', 'B', 'C']);
    assert.ok(!html.includes('data-show'), 'the Overview offers no switch');
  }
  const b = draw.pageHtml({ page: 'd-B', devices });
  assert.deepEqual([...b.matchAll(/data-device="([^"]+)"/g)].map((m) => m[1]), ['B']);
  assert.ok(b.includes(`<div class="fpd-show" id="${draw.SHOW_ID}">`
    + `<button type="button" class="fi-btn" id="${draw.SHOW_BUTTON_ID}" data-show="B">Show on the page</button>`
    + '</div>'), b);
  const a = draw.pageHtml({ page: 'd-A', devices });
  assert.ok(!a.includes('data-show') && a.includes('This is the device the page shows.'), a);
  assert.ok(draw.pageHtml({ page: 'd-Z', devices }).includes('no longer configured'));
  assert.ok(draw.pageHtml({ page: null, devices: [] }).includes('No device is configured.'));
  assert.ok(draw.bodyHtml({ page: null, devices, error: HOSTILE })
    .startsWith(`<p class="fp-note fp-failed" role="alert">${SAFE}</p>`));
});

await ok('the_config_line_is_the_daemons_words_in_a_place_of_its_own_on_the_overview_only', () => {
  const devices = [dev('A', { shown: true }), dev('B')];
  const words = 'config: /x/<b>.toml changed since the last reload: svolder daemon reload (or SIGHUP)';
  const on = draw.pageHtml({ page: 'overview', devices, configChanged: words });
  assert.ok(on.includes(`<div id="${draw.CONFIG_ID}" role="status"><p class="fp-note fp-config">`
    + 'config: /x/&lt;b&gt;.toml changed since the last reload: svolder daemon reload (or SIGHUP)</p></div>'),
  'the words as given, escaped, in the place');
  assert.equal(draw.configHtml(null), '');
  assert.equal(draw.configHtml(''), '');
  assert.ok(!draw.pageHtml({ page: 'd-B', devices, configChanged: words }).includes(draw.CONFIG_ID),
    'a device\'s page has no such place');
  const css = readFileSync(new URL('../../../bins/svolderd/web/core/frame.css', import.meta.url), 'utf8');
  const rule = /^\.fp-note\.fp-config \{([^}]*)\}/m.exec(css)?.[1] ?? '';
  assert.ok(rule && !/#[0-9a-f]{3,8}\b|rgba?\(/i.test(rule) && /var\(--frame-ink\)/.test(rule),
    'the line is coloured by the frame tokens alone');
});

// ---- the wire ----

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });
const fleet = (list) => ({
  devices: list.map(([name, on = true]) => ({ url: name, name, link: link(on) })), hops: [],
});
const rows = (list) => list.map(([name, on = true, unit = `unit-${name}`]) => ({
  unit, name, passphrase: 'auto', connected: on,
  ...(on ? {} : { not_connected_since_ms: NOW - 28 * MIN }),
}));
const THREE = [['A'], ['B'], ['C', false]];

class FakeSource {
  static all = [];

  constructor(url) {
    this.url = url;
    this.closed = false;
    this.listeners = {};
    FakeSource.all.push(this);
  }

  addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); }

  close() { this.closed = true; }

  send(name, data) {
    for (const fn of this.listeners[name] ?? []) fn({ data: JSON.stringify(data) });
  }
}

/** The real core, a writer's unless `viewer` says otherwise, with A on show: every case of the
 * wire is about B or C, never about the shown device. */
function rig({ viewer = { guest: false, writes: true }, url = 'http://svolder.test/?device=A' } = {}) {
  const p = page(BODY, { url });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  FakeSource.all = [];
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const state = {
    listed: THREE, series: () => ({ status: 200, body: answer() }), devices: 200, changed: null,
  };
  const asked = [];
  p.window.fetch = async (path, init = {}) => {
    asked.push(`${init.method ?? 'GET'} ${path}`);
    const reply = (status, body) => ({ status, ok: status === 200, json: async () => body });
    const one = /^api\/v1\/devices\/([^/]+)\/(model|series)/.exec(path);
    if (one?.[2] === 'model') return reply(200, { device: model(one[1]) });
    if (one?.[2] === 'series') {
      const said = state.series(one[1]);
      return reply(said.status, said.body);
    }
    if (path === 'api/v1/devices') {
      return state.devices === 200
        ? reply(200, { devices: rows(state.listed), config_changed: state.changed })
        : reply(state.devices, { error: 'Sign in to change this' });
    }
    return reply(200, {});
  };
  const ticks = [];
  const cleared = [];
  // the popup's reread timer is the one the cases step; the core's ticker of the ages is kept apart
  const agers = [];
  p.window.setInterval = (fn, ms) => {
    const id = ticks.length + agers.length + 1;
    (ms === REREAD_MS ? ticks : agers).push({ fn, ms, id });
    return id;
  };
  p.window.clearInterval = (id) => { cleared.push(id); };
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
      v: 1, viewer, fleet: fleet(THREE), shown: 'A', device: model('A'), followed: {},
      card: null, host: null, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  keyListeners.length = 0;
  const $ = (id) => p.document.getElementById(id);
  const landed = (act) => new Promise((resolve) => {
    p.window.addEventListener('popstate', () => p.window.setTimeout(resolve, 0), { once: true });
    act();
  });
  const calm = async () => { for (let i = 0; i < 6; i++) await p.settle(); };
  const open = async (to = null) => {
    $('knob').focus();
    core.router.open(DEVICES, to);
    await calm();
  };
  const source = () => FakeSource.all.at(-1);
  /** The daemon lists `list` from now on and says so on the stream. */
  const relist = async (list) => {
    state.listed = list;
    source().send('fleet', fleet(list));
    await calm();
  };
  const entries = () => [...p.document.querySelectorAll('#popup-devices .fp-entry')].map((el) => [
    el.dataset.popupPage, el.textContent,
    el.querySelector('.fp-dot')?.classList.contains('on') ?? null,
  ]);
  return {
    ...p, core, state, asked, ticks, agers, cleared, keyListeners, $, landed, calm, open, source, relist,
    entries,
    at: () => p.document.activeElement?.id ?? null,
    seriesAsked: () => asked.filter((one) => one.includes('/series')),
  };
}

await ok('the_list_is_the_configured_devices_and_every_series_request_names_its_device', async () => {
  const r = rig();
  await r.open();
  assert.deepEqual(r.entries(), [
    ['overview', 'Overview', null], ['d-A', 'AShown', true], ['d-B', 'B', true], ['d-C', 'C', false],
    ['add', '+ Add a device', null],
  ]);
  assert.equal(r.asked.filter((one) => one === 'GET api/v1/devices').length, 1, 'one read of the list');
  assert.deepEqual(r.seriesAsked().sort(), ['GET api/v1/devices/A/series', 'GET api/v1/devices/B/series',
    'GET api/v1/devices/C/series'], 'the Overview reads each device once, by its name');
  assert.ok(r.asked.every((one) => one.startsWith('GET ')), 'the popup writes nothing');
  assert.equal(r.document.querySelectorAll('#popup-devices .fpd-device').length, 3);
  assert.ok(r.$(draw.blockId('C')).classList.contains('fpd-away'));
  assert.equal(r.$(draw.stateId('C')).textContent.trim(), 'not connected since 14:02');
  assert.ok(r.$(draw.graphsId('B')).querySelector('.fp-graph[role="img"]'), 'the graphs arrived');
  assert.equal(r.core.router.state().device, 'A');
  assert.deepEqual(r.keyListeners, [], 'the wire attaches no key listener');
  r.window.close();
});

await ok('a_page_reads_its_own_device_and_never_the_shown_one', async () => {
  const r = rig();
  await r.open('d-B');
  assert.equal(r.core.model.shown(), 'A', 'another device is on show than the page is about');
  assert.deepEqual(r.seriesAsked(), ['GET api/v1/devices/B/series']);
  assert.ok(r.asked.every((one) => !one.includes('/A/')), `nothing asks for the shown device: ${r.asked}`);
  assert.equal(r.document.querySelector('#popup-devices .fpd-name').textContent, 'B');
  assert.equal(r.$(draw.stateId('B')).textContent.trim(), 'connected');
  assert.equal(r.$(draw.SHOW_ID).querySelector('button').dataset.show, 'B');
  assert.equal(r.document.title.includes('Devices · B'), true, `the tab's title names the page: ${r.document.title}`);
  // a step to another device's page reads that one
  r.asked.length = 0;
  r.core.router.step('d-C');
  await r.calm();
  assert.deepEqual(r.seriesAsked(), ['GET api/v1/devices/C/series']);
  // the page of the shown device offers no switch and says why
  r.core.router.step('d-A');
  await r.calm();
  assert.equal(r.$(draw.SHOW_ID).querySelector('button'), null);
  assert.equal(r.$(draw.SHOW_ID).textContent, 'This is the device the page shows.');
  r.window.close();
});

await ok('the_stream_follows_the_listed_devices_and_the_store_forgets_them_at_the_close', async () => {
  const r = rig();
  assert.equal(r.source().url, 'api/v1/events?device=A', 'before the popup: the shown device alone');
  await r.open('d-B');
  assert.equal(r.source().url, 'api/v1/events?device=A&needs=walk&follow=A,B,C',
    'the first real follow: the address names every listed device');
  assert.deepEqual(r.core.stream.followed(), ['A', 'B', 'C']);
  assert.deepEqual(r.core.stream.needed(), ['walk'],
    'it asks for no feed of its own, no meters, no host, but the add page\'s walk states');
  r.source().send('device', { device: 'B', data: model('B', { link: link(false) }) });
  assert.equal(r.core.model.device('B').name, 'B', 'the store holds the followed device');
  assert.equal(r.$(draw.stateId('B')).textContent.trim().startsWith('not connected'), true,
    'and its link is the state line, without a request');
  const opened = FakeSource.all.length;
  await r.landed(() => r.core.router.close());
  assert.equal(r.core.model.device('B'), null, 'store.forget at the release');
  assert.deepEqual(r.core.stream.followed(), []);
  assert.equal(FakeSource.all.length, opened + 1);
  assert.equal(r.source().url, 'api/v1/events?device=A', 'and the stream is back on the shown device alone');
  assert.equal(OWNER, 'popup:devices');
  r.window.close();
});

await ok('a_device_the_fleet_drops_while_its_page_stands_is_forgotten_and_the_page_says_so', async () => {
  const r = rig();
  await r.open('d-C');
  r.source().send('device', { device: 'C', data: model('C') });
  assert.ok(r.core.model.device('C'));
  r.$('popup-devices-entry-d-C').focus();
  await r.relist([['A'], ['B']]);
  assert.equal(r.core.model.device('C'), null, 'the stream forgot it (core/stream.js, the fleet event)');
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'add']);
  assert.equal(r.document.querySelector('#popup-devices .fp-page').textContent.includes('no longer configured'), true);
  assert.equal(r.$('popup-devices').isConnected, true, 'the popup stands');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'devices', page: 'd-C' });
  assert.ok(r.$('popup-devices').contains(r.document.activeElement), 'the focus stays in the popup');
  assert.equal(r.source().url, 'api/v1/events?device=A&needs=walk&follow=A,B');
  r.window.close();
});

await ok('a_device_that_connects_or_drops_changes_its_dot_and_its_line_and_nothing_else', async () => {
  const r = rig();
  await r.open();
  const entry = r.$('popup-devices-entry-d-C');
  entry.focus();
  const block = r.$(draw.blockId('B'));
  const graphs = r.$(draw.graphsId('B')).firstElementChild;
  r.asked.length = 0;
  await r.relist([['A'], ['B'], ['C', true]]);
  assert.equal(r.$('popup-devices-entry-d-C'), entry, 'the entry is the same element: nothing was drawn again');
  assert.equal(r.document.activeElement, entry, 'the focus is where it was');
  assert.equal(entry.querySelector('.fp-dot').classList.contains('on'), true);
  assert.equal(entry.querySelector('.fp-dot').getAttribute('aria-label'), 'connected');
  assert.equal(r.$(draw.stateId('C')).textContent.trim(), 'connected');
  assert.equal(r.$(draw.blockId('C')).classList.contains('fpd-away'), false);
  assert.equal(r.$(draw.blockId('B')), block, 'the pane was not drawn again (its scroll stays)');
  assert.equal(r.$(draw.graphsId('B')).firstElementChild, graphs, "nor another device's graphs");
  assert.deepEqual(r.asked, ['GET api/v1/devices'], 'the list is read again, no series');
  await r.relist([['A'], ['B', false], ['C', true]]);
  assert.equal(r.$('popup-devices-entry-d-B').querySelector('.fp-dot').classList.contains('on'), false);
  assert.equal(r.$(draw.stateId('B')).textContent.trim(), 'not connected since 14:02');
  assert.equal(r.$(draw.blockId('B')).classList.contains('fpd-away'), true);
  assert.equal(r.document.activeElement, entry);
  r.window.close();
});

await ok('a_device_that_appears_is_listed_and_the_selection_and_the_focus_stay', async () => {
  const r = rig();
  await r.open('d-B');
  r.$('popup-devices-entry-d-B').focus();
  r.asked.length = 0;
  await r.relist([['A'], ['B'], ['C', false], ['D']]);
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-C', 'd-D', 'add']);
  assert.equal(r.at(), 'popup-devices-entry-d-B', 'the focus is on the same entry');
  assert.equal(r.$('popup-devices-entry-d-B').getAttribute('aria-current'), 'page', 'the selection stays');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'devices', page: 'd-B' });
  assert.equal(r.source().url, 'api/v1/events?device=A&needs=walk&follow=A,B,C,D', 'and it is followed');
  assert.deepEqual(r.seriesAsked(), [], 'the page in view is still B, whose series is held');
  r.window.close();
});

await ok('a_device_that_appears_or_leaves_takes_nothing_typed_on_another_devices_page', async () => {
  const r = rig();
  await r.open();
  r.core.router.step('d-B');
  await r.calm();
  const pane = r.document.querySelector('#popup-devices .fp-page');
  const block = r.$(draw.blockId('B'));
  const field = r.document.createElement('input');
  field.id = 'typed';
  r.$(draw.BODY_ID).append(field);
  field.value = 'half a name';
  field.focus();
  await r.relist([...THREE, ['D']]);
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-C', 'd-D', 'add']);
  assert.equal(r.$('typed'), field, 'the field is the same element');
  assert.equal(field.value, 'half a name');
  assert.equal(r.document.activeElement, field, 'with the focus');
  assert.equal(r.$(draw.blockId('B')), block, "and the device's block was not drawn again");
  assert.equal(r.document.querySelector('#popup-devices .fp-page'), pane);
  await r.relist([['A'], ['B'], ['D']]);
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-D', 'add']);
  assert.equal(r.document.activeElement, field, 'a device that leaves takes nothing either');
  assert.equal(field.value, 'half a name');
  assert.equal(r.$('popup-devices-entry-d-B').getAttribute('aria-current'), 'page');
  assert.ok(r.document.title.includes('Devices · B'), r.document.title);
  // on the Overview the pane's body does change: the new device has a block
  r.core.router.step('overview');
  await r.calm();
  const b = r.$(draw.blockId('B'));
  const stepped = r.document.querySelector('#popup-devices .fp-page');
  await r.relist([['A'], ['B'], ['D'], ['E']]);
  assert.ok(r.$(draw.blockId('E')), 'the Overview lists the device that appeared');
  assert.notEqual(r.$(draw.blockId('B')), b, 'its body was put again');
  assert.equal(r.document.querySelector('#popup-devices .fp-page'), stepped, 'in the same pane');
  r.window.close();
});

await ok('a_rename_of_the_pages_device_moves_the_page_in_place', async () => {
  const r = rig();
  await r.open();
  r.core.router.step('d-B');
  await r.calm();
  const before = r.window.history.length;
  await r.relist([['A'], ['B2', true, 'unit-B'], ['C', false]]);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'devices', page: 'd-B2' },
    'the page follows the device by its unit');
  assert.equal(r.window.history.length, before, 'in place: no entry is pushed');
  assert.equal(r.document.querySelector('#popup-devices .fpd-name').textContent, 'B2');
  assert.equal(r.core.model.shown(), 'A');
  r.window.close();
});

await ok('show_on_the_page_switches_the_device_keeps_the_popup_and_pushes_nothing', async () => {
  const r = rig();
  await r.open();
  r.core.router.step('d-B');
  await r.calm();
  const before = r.window.history.length;
  const button = r.$(draw.SHOW_ID).querySelector('button');
  button.focus();
  r.asked.length = 0;
  button.click();
  await r.calm();
  const reads = r.asked.filter((one) => one.includes('/model'));
  assert.ok(reads.length >= 1 && reads.every((one) => one === 'GET api/v1/devices/B/model'),
    'the device the page is about is the one read (the switch, and the page\'s facts once B\'s model is in)');
  assert.equal(r.core.model.shown(), 'B', 'the page behind changes');
  assert.equal(r.$('popup-devices').isConnected, true, 'the popup stays');
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'devices', page: 'd-B' });
  assert.equal(r.window.history.length, before, 'no entry of the history is pushed');
  assert.equal(r.window.location.search, '?device=B');
  assert.deepEqual(r.entries().map((one) => one[1]), ['Overview', 'A', 'BShown', 'C', '+ Add a device'], 'the tag moved');
  assert.equal(r.$(draw.SHOW_ID).querySelector('button'), null, 'the shown device offers no switch');
  assert.equal(r.$(draw.SHOW_ID).textContent, 'This is the device the page shows.');
  assert.ok(r.$('popup-devices').contains(r.document.activeElement), 'the focus stays in the popup');
  assert.equal(r.source().url, 'api/v1/events?device=B&needs=walk&follow=A,B,C', 'the stream is on the new device');
  assert.ok(r.asked.every((one) => one.startsWith('GET ')), 'the switch is no write');
  // Back: the list, then closed, on B
  await r.landed(() => r.window.history.back());
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'devices', page: null });
  await r.landed(() => r.window.history.back());
  await r.calm();
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.core.model.shown(), 'B');
  r.window.close();
});

await ok('the_graphs_are_read_again_on_a_timer_for_the_devices_in_view_only', async () => {
  const r = rig();
  await r.open('d-C');
  assert.deepEqual(r.ticks.map((one) => one.ms), [REREAD_MS], 'one timer an opening');
  r.asked.length = 0;
  r.ticks[0].fn();
  await r.calm();
  assert.deepEqual(r.asked, ['GET api/v1/devices/C/series', 'GET api/v1/devices'],
    "one request for the device in view, and the list again: a setting changed elsewhere reaches the page");
  r.core.model.setState('resting');
  r.ticks[0].fn();
  await r.calm();
  assert.equal(r.seriesAsked().length, 1, 'a page that rests reads nothing');
  await r.landed(() => r.core.router.close());
  assert.ok(r.cleared.includes(r.ticks[0].id), 'the timer goes with the popup');
  assert.ok(r.cleared.includes(r.agers[0].id), 'and the core\'s ticker of the ages with it');
  await r.open();
  assert.equal(r.ticks.length, 2, 'and comes back with it');
  r.window.close();
});

await ok('a_series_or_a_list_that_cannot_be_read_says_so_in_the_daemons_words', async () => {
  const r = rig();
  r.state.series = (url) => (url === 'B' ? { status: 503, body: { error: 'the daemon is stopping' } }
    : { status: 200, body: answer() });
  await r.open();
  assert.equal(r.$(draw.graphsId('B')).textContent, 'the daemon is stopping');
  assert.ok(r.$(draw.graphsId('C')).querySelector('.fp-graph'), 'the others are drawn');
  await r.landed(() => r.core.router.close());
  r.state.devices = 500;
  await r.open();
  assert.equal(r.document.querySelector('#popup-devices .fp-page [role="alert"]').textContent,
    'Sign in to change this');
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-C', 'add'],
    "the fleet's devices stand in for the list");
  r.window.close();
});

await ok('an_event_of_another_device_leaves_the_focus_on_show_on_the_page', async () => {
  const r = rig();
  await r.open();
  r.core.router.step('d-B');
  await r.calm();
  const button = r.$(draw.SHOW_ID).querySelector('button');
  button.focus();
  r.source().send('device', { device: 'C', data: model('C', { link: link(false) }) });
  await r.calm();
  assert.equal(r.document.activeElement, button, `a device event of C: the focus is on #${r.at()}`);
  r.source().send('part', { device: 'C', name: 'link', data: link(true) });
  await r.calm();
  assert.equal(r.document.activeElement, button, `a link part of C: the focus is on #${r.at()}`);
  assert.equal(r.$(draw.SHOW_ID).querySelector('button'), button, 'the button was not drawn again');
  r.source().send('model', { ...r.core.model.page(), followed: { B: model('B'), C: model('C') } });
  await r.calm();
  assert.equal(r.document.activeElement, button, `the stream's model: the focus is on #${r.at()}`);
  assert.equal(button.id, draw.SHOW_BUTTON_ID, 'and it has an id, so a put that does change it finds its twin');
  r.window.close();
});

await ok('graphs_that_came_before_the_devices_model_are_worded_when_it_arrives', async () => {
  const r = rig();
  await r.open();
  const heads = () => [...r.$(draw.graphsId('B')).querySelectorAll('.fpd-graph-name, .fpd-graph-last')]
    .map((one) => one.textContent);
  // the series answered first: B is followed, and the stream has not sent its model yet
  assert.equal(r.core.model.device('B'), null);
  assert.deepEqual(heads(), ['Signal strength', '-61 dBm', 'sample_rate', '44100', 'volume', '2', 'spdif_locked'],
    'with no model the ids and the numbers stand');
  r.asked.length = 0;
  r.source().send('device', { device: 'B', data: model('B') });
  await r.calm();
  assert.deepEqual(heads(), ['Signal strength', '-61 dBm', 'Sample rate', '44.1 kHz', 'Volume', '-20 dB', 'Lock'],
    'the model words them at once');
  assert.deepEqual(r.seriesAsked(), [], 'with no request');
  // and through the stream's whole model, as after a switch of the shown device
  r.source().send('model', { ...r.core.model.page(), followed: { B: model('B'), C: model('C') } });
  await r.calm();
  assert.equal([...r.$(draw.graphsId('C')).querySelectorAll('.fpd-graph-name')][1].textContent, 'Sample rate');
  r.window.close();
});

await ok('an_older_answer_of_the_list_never_overwrites_a_newer_one', async () => {
  const r = rig();
  await r.open();
  const real = r.window.fetch;
  /** The list requests held back, each answering with the list as it stood when it was sent. */
  const held = [];
  r.window.fetch = (path, init) => {
    if (path !== 'api/v1/devices') return real(path, init);
    const then = r.state.listed;
    return new Promise((resolve) => {
      held.push(() => resolve({ status: 200, ok: true, json: async () => ({ devices: rows(then) }) }));
    });
  };
  r.state.listed = [['A'], ['B']];
  r.source().send('fleet', fleet(r.state.listed));
  r.state.listed = [['A'], ['B'], ['D']];
  r.source().send('fleet', fleet(r.state.listed));
  await r.calm();
  assert.equal(held.length, 2);
  held[1]();
  await r.calm();
  held[0]();
  await r.calm();
  assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-D', 'add'],
    'the newest list stands, whichever answer came last');
  assert.ok(r.$(draw.graphsId('D')).querySelector('.fpd-graph'), "and the new device's graphs are held");
  r.window.close();
});

await ok('a_viewer_who_lost_the_right_gets_the_read_only_popup_from_forward_onto_an_entry_of_this_load', async () => {
  const r = rig();
  await r.open('d-B');
  await r.landed(() => r.window.history.back());
  await r.calm();
  assert.equal(r.$('popup-devices'), null);
  // Forward shows it again while the viewer is a writer: the entry is one of this load's
  await r.landed(() => r.window.history.forward());
  await r.calm();
  assert.equal(r.$('popup-devices').isConnected, true, 'a writer has it back by Forward');
  await r.landed(() => r.window.history.back());
  await r.calm();
  r.core.model.replace({ ...r.core.model.page(), viewer: { guest: true, writes: false } });
  r.asked.length = 0;
  await r.landed(() => r.window.history.forward());
  await r.calm();
  assert.equal(r.$('popup-devices').isConnected, true, 'the same entry opens the popup for a guest');
  assert.equal(r.core.router.state().popup, DEVICES);
  assert.equal(r.document.querySelectorAll('#popup-devices [id^="popup-devices-set-"]').length, 0,
    'and it is read-only: read, not written');
  assert.deepEqual(r.asked.filter((one) => one.includes('api/v1/devices') && !one.includes('/series')
    && !one.includes('/model')), [], 'the writer\'s list is not asked');
  r.window.close();
});

await ok('a_reader_has_the_row_and_the_popup_by_the_router_and_by_the_address_read_only', async () => {
  for (const viewer of [{ guest: true, writes: false }, { guest: false, writes: false }]) {
    const r = rig({ viewer, url: 'http://svolder.test/?device=A&popup=devices#devices' });
    const row = frameRows({
      viewer, devices: r.core.model.page().fleet.devices, shown: 'A', run: {},
      popups: r.core.popups.listed(),
    }).map((one) => one.elementId);
    assert.ok(row.includes('menu-popup-devices') && row.includes('menu-popup-help'), `${row}`);
    r.core.router.open(DEVICES);
    await r.calm();
    assert.equal(r.$('popup-devices').isConnected, true, 'the popup opens');
    r.core.router.step('d-B');
    await r.calm();
    assert.equal(r.document.querySelector('#popup-devices .fpd-name').textContent, 'B');
    assert.deepEqual(r.entries().map((one) => one[0]), ['overview', 'd-A', 'd-B', 'd-C'],
      'the fleet\'s list: no add entry');
    assert.equal(r.document.querySelectorAll('#popup-devices [id^="popup-devices-set-"]').length, 0);
    assert.deepEqual(r.asked.filter((one) => one === 'GET api/v1/devices' || !one.startsWith('GET ')), [],
      'no read of the configured list and no write');
    r.window.close();
  }
  const listed = POPUPS.find((one) => one.name === DEVICES);
  assert.deepEqual([listed.row.sees, listed.stays, POPUPS.indexOf(listed)], ['everyone', true, 0]);
  assert.equal(POPUPS.at(-1).name, 'help', 'Help stays the last row');
});


const WORDS = 'config: /home/u/config.toml changed since the last reload: svolder daemon reload (or SIGHUP)';

await ok('the_overview_says_the_config_changed_in_the_daemons_words_and_the_reread_clears_it', async () => {
  const r = rig();
  r.state.changed = WORDS;
  await r.open('overview');
  assert.equal(r.$(draw.CONFIG_ID).textContent, WORDS, 'the words as given');
  assert.equal(r.$(draw.CONFIG_ID).getAttribute('role'), 'status');
  const lists = () => r.asked.filter((one) => one === 'GET api/v1/devices').length;
  // a reload clears it at the next read: the Overview is read again on the timer while the line
  // stands
  const before = lists();
  r.state.changed = null;
  r.ticks[0].fn();
  await r.calm();
  assert.equal(lists(), before + 1, 'the Overview read the list again while the line stood');
  assert.equal(r.$(draw.CONFIG_ID).textContent, '', 'the line went');
  // with no line standing, the Overview is not read on the timer
  r.ticks[0].fn();
  await r.calm();
  assert.equal(lists(), before + 1, 'no re-read of an Overview that shows no line');
  // a hand edit made now is told by the next read the page makes anyway (a fleet event)
  r.state.changed = WORDS;
  r.source().send('fleet', fleet(THREE));
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, WORDS);
  r.window.close();
});

await ok('a_put_of_the_config_line_leaves_the_focus_where_it_is_and_the_rest_of_the_pane_alone', async () => {
  const r = rig();
  await r.open('overview');
  const entry = r.document.querySelector('#popup-devices .fp-entry[data-popup-page="d-B"]');
  entry.focus();
  const block = r.$('popup-devices-device-B') ?? r.document.querySelector('[data-device="B"]');
  r.state.changed = WORDS;
  r.ticks[0].fn();
  r.source().send('fleet', fleet(THREE));
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, WORDS);
  assert.equal(r.document.activeElement, entry, 'the focus did not move');
  assert.equal(r.document.querySelector('[data-device="B"]'), block, 'the blocks were not drawn again');
  r.state.changed = null;
  r.source().send('fleet', fleet(THREE));
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, '');
  assert.equal(r.document.activeElement, entry);
  r.window.close();
});

await ok('a_device_page_has_no_config_line_and_the_overview_after_it_has_the_words', async () => {
  const r = rig();
  r.state.changed = WORDS;
  await r.open('d-B');
  assert.equal(r.$(draw.CONFIG_ID), null, 'a device page has no such place');
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, WORDS);
  r.window.close();
});

await ok('stepping_back_to_the_overview_reads_the_config_line_afresh', async () => {
  const r = rig();
  await r.open('d-B');
  assert.equal(r.$(draw.CONFIG_ID), null);
  r.state.changed = WORDS;
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, WORDS, 'a hand edit made on the page is told');
  // the Overview opened from the list's own Overview entry again, with no line: none is made up
  r.state.changed = null;
  r.core.router.step('d-C');
  await r.calm();
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$(draw.CONFIG_ID).textContent, '');
  r.window.close();
});

console.log(`core-devices: ${n} cases passed`);
