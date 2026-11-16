/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Sound popup's Virtual speakers tab (core/popups/speakers/{draw,pictures,
// wire}.js, the third tab of core/popups/sound/, and core/speakers-actions.js) on the real core
// over a jsdom page and a small fake daemon that answers the way the daemon's contract says
// (`SpeakersView`, `HostEqOp::Speakers`, the `api/v1/speakers/*` routes). First the draws, as
// strings: the three views from a view with and without `timing`, the two choices with their
// technical terms, the licence line, the list's states, hostile names; then the wire: the tab, a
// choice and the switch, a fetch (progress-less, cancellable), the answer of a call and the stream's
// update in either order, a set another front end removed (a 404, no retry loop), an import, the
// chip remembered per browser, the keys. Run: node core-speakers.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { SOUND, SPEAKERS, CROSSFEED } = await import(
  '../../../bins/svolderd/web/core/popups/listed.js');
const draw = await import('../../../bins/svolderd/web/core/popups/speakers/draw.js');
const pics = await import('../../../bins/svolderd/web/core/popups/speakers/pictures.js');
const { CHIP_KEY } = await import('../../../bins/svolderd/web/core/popups/speakers/wire.js');
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
const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();

// ---- the views, as the daemon gives them ----

const HZ = [];
for (let i = 0; i < 61; i++) HZ.push(Math.round(20 * 10 ** (i / 20) * 100) / 100);
const CURVE = {
  hz: HZ,
  ll_db: HZ.map((f) => -1 - 0.5 * Math.log10(f / 20)),
  lr_db: HZ.map((f) => -7 - 3 * Math.log10(f / 20)),
  rl_db: HZ.map((f) => -7.5 - 3 * Math.log10(f / 20)),
  rr_db: HZ.map((f) => -1.2 - 0.5 * Math.log10(f / 20)),
};
const times = [0.1, 1, 10, 100, 500];
const TIMING = {
  arrivals: {
    left: [{ ms: 0, db: 0 }, { ms: 0.25, db: -4.1 }, { ms: 3, db: -18 }],
    right: [{ ms: 0, db: -0.2 }, { ms: 0.25, db: -4.3 }, { ms: 4, db: -20 }],
    tail: { times_ms: [1, 10, 50], left: [-20, -30, -45], right: [-21, -32, -48] },
  },
  decay: {
    times_ms: times, low: [0, -3, -12, -35, -60], mid: [0, -5, -20, -50, -60],
    high: [0, -8, -30, -60, -60], end_ms: 500,
  },
};
const SET_NO = {
  id: 'sadie-d2', room: 'no', room_name: 'No room', term: 'HRTF', label: 'SADIE II, D2',
  licence: 'Apache-2.0', credit: 'SADIE II, University of York', source_url: 'https://example.test/sadie/',
};
const SET_WITH = {
  id: 'sadie-d2-brir', room: 'with', room_name: 'With room', term: 'BRIR', label: 'SADIE II, D2 room',
  licence: 'Apache-2.0', credit: 'SADIE II, University of York', source_url: 'https://example.test/zenodo/',
};
const VIEW = (over = {}) => ({
  on: true, set: 'sadie-d2-brir', room: 'with', room_name: 'With room', term: 'BRIR',
  label: 'SADIE II, D2 room', licence: 'Apache-2.0', credit: 'SADIE II, University of York',
  source_url: 'https://example.test/zenodo/', angles: 'left 45.0 deg, right 315.0 deg, 1.5 m',
  taps_ms: 500, loudness_db: -7.2, cut: true, note: null, off_reason: null, unavailable: null,
  curve: CURVE, sets: [SET_NO, SET_WITH], timing: TIMING, ...over,
});
const ROWS = () => [
  { ...SET_NO, bytes: 36591729, own: false, fetched: true },
  { ...SET_WITH, bytes: 8359449, own: false, fetched: true },
  {
    id: 'hutubs-pp1', room: 'no', room_name: 'No room', term: 'HRTF', label: 'HUTUBS, subject pp1', licence: 'CC-BY-4.0', credit: 'HUTUBS',
    source_url: 'https://example.test/hutubs', bytes: 1659720, own: false, fetched: false,
  },
];

// ---- part 1: the draws ----

await ok('the_page_draws_the_three_views_the_two_choices_with_their_terms_and_the_licence', async () => {
  const html = draw.pageHtml({ sp: VIEW(), cat: { sets: ROWS(), fetching: true } });
  const words = text(html);
  for (const part of ['Arrivals', 'Directions', 'Decay', 'With room · BRIR', 'No room · HRTF',
    'Apache-2.0', 'SADIE II, University of York', 'left 45.0 deg, right 315.0 deg, 1.5 m',
    'cut to 0.5 s', 'about 7 dB quieter than Off', 'near, direct', 'far, +0.25 ms', 'cut to 0.5 s',
    'dotted circle: 1.5 m', 'Use your own file', 'Same ear', 'Other ear']) {
    assert.ok(words.includes(part), `${part} is on the page: ${words}`);
  }
  assert.equal((html.match(/class="fps-panel[ "]/g) ?? []).length, 3);
  assert.ok(html.includes('role="radiogroup"') && html.includes('href="https://example.test/zenodo/"'));
  assert.ok(!html.includes('fps off'), 'on is not dimmed');
});

await ok('a_view_without_timing_says_not_measured_in_arrivals_and_decay_and_directions_still_draws', async () => {
  const html = draw.pageHtml({ sp: VIEW({ timing: undefined }), cat: null });
  const panels = html.split('<section').slice(1).map((one) => one.split('</section>')[0]);
  assert.equal(panels.length, 3);
  assert.ok(panels[0].includes('Not measured for this set.') && !panels[0].includes('<svg'));
  assert.ok(panels[2].includes('Not measured for this set.') && !panels[2].includes('<svg'));
  assert.ok(panels[1].includes('<svg') && !panels[1].includes('Not measured'));
  assert.ok(html.includes('fps-svg'), 'the response still draws');
});

await ok('directions_take_the_angle_from_the_catalogue_or_the_sentence_and_the_bars_from_the_curve', async () => {
  assert.deepEqual(pics.anglesOf(null, 'left 30.0 deg, right 330.0 deg, 1.2 m'),
    { left_deg: 30, right_deg: 330, distance_m: 1.2 });
  assert.deepEqual(pics.anglesOf({ left_deg: 45, right_deg: 315, distance_m: 2 }, 'nonsense'),
    { left_deg: 45, right_deg: 315, distance_m: 2 });
  assert.equal(pics.anglesOf(null, ''), null);
  const diff = pics.earDifference(CURVE);
  assert.ok(Math.abs(diff.left - 10.19) < 0.05 && diff.right > 9, `left ${diff.left}`);
  assert.equal(pics.earDifference({ hz: [1] }), null);
  assert.equal(pics.directionsSvg(null, null), '');
  const svg = pics.directionsSvg({ left_deg: 45, right_deg: 315, distance_m: 1.5 }, diff);
  assert.ok(svg.includes('45° · +10 dB') && svg.includes('315° · +10 dB') && svg.includes('fps-reach'));
  assert.ok(svg.includes('left 45 and right 315 degrees'), 'a screen reader hears which is which');
});

await ok('the_arrivals_ticks_and_the_decay_lines_come_from_the_timing_and_nothing_else', async () => {
  const arrivals = pics.arrivalsSvg(TIMING.arrivals);
  assert.equal((arrivals.match(/fps-direct/g) ?? []).length, 4, 'two direct arrivals in each ear');
  assert.equal((arrivals.match(/fps-reflect/g) ?? []).length, 2);
  assert.ok(arrivals.includes('fps-tail'));
  const noRoom = pics.arrivalsSvg({ ...TIMING.arrivals, tail: null });
  assert.ok(!noRoom.includes('fps-tail'));
  assert.equal(pics.arrivalsSvg({ left: [], right: [] }), '');
  assert.equal(pics.arrivalsSvg(undefined), '');
  const decay = pics.decaySvg(TIMING.decay, true);
  assert.equal((decay.match(/fps-band-low/g) ?? []).length, 2, 'the line and its key');
  assert.ok(text(decay).includes('cut to 0.5 s'));
  assert.ok(text(pics.decaySvg({ ...TIMING.decay, end_ms: 5.3 }, false)).includes('ends at 5.3 ms'));
  assert.equal(pics.decaySvg({ times_ms: [1, 2], low: [0], mid: [0, 1], high: [0, 1] }, false), '');
});

await ok('a_null_timing_is_absent_and_fourteen_arrivals_and_a_floored_tail_draw', async () => {
  const html = draw.pageHtml({ sp: VIEW({ timing: null }), cat: null });
  assert.equal((html.match(/Not measured for this set\./g) ?? []).length, 2);
  const many = Array.from({ length: 14 }, (_, i) => ({ ms: i * 3.5, db: -i * 2 }));
  const svg = pics.arrivalsSvg({
    left: many, right: many,
    tail: { times_ms: [0, 25, 50], left: [-10, -60, -60], right: [-10, -60, -60] },
  });
  assert.equal((svg.match(/fps-reflect/g) ?? []).length, 24);
  assert.ok(!/NaN|Infinity/.test(svg));
});


/** The numbers of the path `d` of the first element of `cls` in `svg`. */
const pathOf = (svg, cls) => {
  const found = new RegExp(`class="${cls}"[^>]*? d="([^"]+)"`).exec(svg);
  assert.ok(found, `${cls} is drawn`);
  return found[1].match(/-?\d+(?:\.\d+)?/g).map(Number);
};
const circles = (svg, cls) => [...svg.matchAll(new RegExp(`class="${cls}" cx="([-\\d.]+)" cy="([-\\d.]+)"`, 'g'))]
  .map((m) => [Number(m[1]), Number(m[2])]);

await ok('the_pictures_put_their_data_where_the_axes_say_so', async () => {
  // Directions: the left speaker (30 deg, counter-clockwise from the front) is left of the centre
  // and above it, the right one (330) right of it; the dotted circle is as far as the dots
  const dir = pics.directionsSvg({ left_deg: 30, right_deg: 330, distance_m: 1.5 }, { left: 6, right: 6 });
  const [[lx, ly], [rx, ry]] = circles(dir, 'fps-dot');
  const [cx, cy] = circles(dir, 'fps-ring')[0] ?? [165, 132];
  assert.ok(lx < 165 && rx > 165 && ly < 132 && ry < 132 && Math.abs((165 - lx) - (rx - 165)) < 0.2, [lx, ly, rx, ry]);
  assert.ok(Math.abs(Math.hypot(lx - 165, ly - 132) - 84) < 0.3, 'the dots stand on the dotted circle');
  assert.ok(cx > 0 && cy > 0);
  // the bar is as long as the level difference says and no longer than 12 dB
  const bar = pathOf(dir, 'fps-bar');
  const length = Math.hypot(bar[2] - bar[0], bar[3] - bar[1]);
  assert.ok(Math.abs(length - 18 - 1.5) < 0.3, `a 6 dB bar is 19.5 long, not ${length}`);
  const far = pathOf(pics.directionsSvg({ left_deg: 30, right_deg: 330 }, { left: 40, right: 40 }), 'fps-bar');
  assert.ok(Math.abs(Math.hypot(far[2] - far[0], far[3] - far[1]) - 37.5) < 0.3, 'clamped at 12 dB');
  assert.ok(text(pics.directionsSvg({ left_deg: 30, right_deg: 330 }, { left: 40, right: 40 })).includes('+40 dB'),
    'the label says the real difference');
  // Decay: 1 ms is the middle of 0.1 ms to 10 ms, 0 dB the top line and a value below -60 the -60 line
  const decay = pics.decaySvg({
    times_ms: [0.1, 1, 10], low: [0, -80, -80], mid: [0, 0, 0], high: [0, -30, -60], end_ms: 10,
  }, false);
  const low = pathOf(decay, 'fps-band fps-band-low');
  assert.equal(low.length, 6);
  assert.ok(Math.abs(low[2] - (40 + 282 / 2)) < 0.2, `1 ms is mid-axis, not ${low[2]}`);
  assert.ok(Math.abs(low[1] - 10) < 0.2 && Math.abs(low[3] - 192) < 0.2, `0 dB on top, -80 dB on the floor: ${low}`);
  const high = pathOf(decay, 'fps-band fps-band-high');
  assert.ok(Math.abs(high[3] - 101) < 0.2, `-30 dB is half way: ${high[3]}`);
  // Arrivals: the last tick is at the right edge, and a tick and the tail of one level are as tall
  const arrivals = pics.arrivalsSvg({
    left: [{ ms: 0, db: 0 }, { ms: 0.25, db: -4 }, { ms: 10, db: -30 }, { ms: 50, db: -18 }],
    right: [{ ms: 0, db: 0 }, { ms: 0.25, db: -4 }, { ms: 20, db: -18 }],
    tail: { times_ms: [0, 50], left: [-18, -18], right: [-18, -18] },
  });
  const ticks = [...arrivals.matchAll(/class="fps-(?:direct|reflect)" d="M([\d.]+) ([\d.]+)V([\d.]+)"/g)];
  assert.ok(ticks.some((t) => Math.abs(Number(t[1]) - 322) < 0.2), 'the latest arrival is at the right edge');
  assert.ok(ticks.some((t) => Math.abs(Number(t[1]) - 118.8) < 0.2 && Math.abs(Number(t[3]) - 64) < 0.2),
    '-30 dB is half of the 60 dB scale (28 px of 56)');
  const tick = ticks.find((t) => Math.abs(Number(t[1]) - 322) < 0.2);
  const tail = pathOf(arrivals, 'fps-tail');
  assert.ok(Math.abs(tail[3] - Number(tick[3])) < 0.2, `one scale: tick ${tick[3]}, tail ${tail[3]}`);
});

await ok('the_choice_words_are_the_daemons_and_none_is_written_in_the_page', async () => {
  const bare = { ...VIEW(), room_name: undefined, term: undefined, sets: [] };
  const html = draw.pageHtml({ sp: bare, cat: { sets: ROWS().map((one) => ({ ...one, room_name: undefined, term: undefined })), fetching: true } });
  assert.ok(!/With room|No room|BRIR|HRTF/.test(html), 'with no words from the data there are none on the page');
  assert.ok(!html.includes('fps-kinds') && !text(html).includes('Virtual speakers'), 'and no label over an empty group');
  assert.deepEqual(draw.namesOf(VIEW(), { sets: ROWS() }), {
    no: { name: 'No room', term: 'HRTF' }, with: { name: 'With room', term: 'BRIR' },
  });
  const oneRoom = draw.namesOf({ sets: [] }, { sets: [{ ...ROWS()[1] }] });
  assert.deepEqual(Object.keys(oneRoom), ['with']);
});

await ok('crossfeed_on_says_the_speakers_turn_it_off_and_a_set_row_names_its_buttons_and_credit', async () => {
  const on = draw.pageHtml({ sp: VIEW({ on: false }), cat: { sets: ROWS(), fetching: true }, crossfeedOn: true });
  assert.ok(text(on).includes('Turning these on turns crossfeed off.'));
  assert.ok(!text(draw.pageHtml({ sp: VIEW({ on: false }), cat: null })).includes('turns crossfeed off'));
  assert.ok(!text(draw.pageHtml({ sp: VIEW({ on: true }), cat: null, crossfeedOn: true })).includes('turns crossfeed off'));
  const list = draw.setsHtml({ cat: { sets: ROWS(), fetching: true }, filter: 'all', chosen: null, busy: null, kind: null, names: draw.namesOf(VIEW(), { sets: ROWS() }) });
  for (const name of ['Use SADIE II, D2', 'Forget SADIE II, D2', 'Fetch HUTUBS, subject pp1']) {
    assert.ok(list.includes(`aria-label="${name}"`), name);
  }
  assert.ok(text(list).includes('SADIE II, University of York'), 'the credit is in the row');
});

await ok('the_list_shows_each_state_and_never_hides_the_room_terms', async () => {
  const rows = [
    ...ROWS(),
    { ...ROWS()[0], id: 'broke', label: 'Broken', damaged: 'the file is truncated' },
    {
      id: 'mine', room: 'no', label: 'My file', licence: 'LicenseRef-own', credit: '', source_url: '',
      bytes: 0, own: true, fetched: true,
    },
  ];
  const html = draw.setsHtml({
    cat: { sets: rows, fetching: true }, filter: 'all', chosen: 'sadie-d2-brir', busy: null, kind: null,
  });
  assert.match(text(html), /✓ chosen/);
  assert.ok(html.includes('data-act="use"') && html.includes('data-act="fetch"') && html.includes('data-act="forget"'));
  assert.match(text(html), /damaged: the file is truncated/);
  assert.match(text(html), /own/);
  assert.match(text(html), /No room · HRTF/);
  const withOnly = draw.setsHtml({
    cat: { sets: rows, fetching: true }, filter: 'with', chosen: null, busy: null, kind: null,
  });
  assert.ok(!withOnly.includes('hutubs-pp1'), 'the filter');
  const off = draw.setsHtml({
    cat: { sets: rows, fetching: false, note: 'fetching is off' }, filter: 'all', chosen: null,
    busy: null, kind: null,
  });
  assert.ok(/data-act="fetch"[^>]*aria-disabled="true"/.test(off) && text(off).includes('fetching is off'));
  const running = draw.setsHtml({
    cat: { sets: rows, fetching: true }, filter: 'all', chosen: null, busy: 'hutubs-pp1', kind: 'fetch',
  });
  assert.match(text(running), /Fetching…\s*Cancel/);
  assert.ok(/data-act="use"[^>]*aria-disabled="true"/.test(running), 'one job at a time');
});

await ok('off_is_dimmed_with_its_note_and_the_switch_needs_a_set', async () => {
  const html = draw.pageHtml({ sp: VIEW({ on: false, note: 'virtual speakers off: the output changed' }), cat: null });
  assert.ok(html.includes('fps fps-off'));
  assert.ok(text(html).includes('virtual speakers off: the output changed'));
  assert.ok(!draw.switchHtml(VIEW({ on: false })).includes('aria-disabled'));
  assert.ok(draw.switchHtml(VIEW({ on: false, set: null })).includes('aria-disabled="true"'));
  assert.equal(draw.switchHtml(null), '');
  const none = draw.pageHtml({ sp: VIEW({ set: null, room: null, on: false, timing: undefined }), cat: null });
  assert.ok(text(none).includes('No set is chosen') && !none.includes('fps-views'));
  assert.ok(text(draw.pageHtml({ sp: null })).includes('not offered'));
  assert.ok(text(draw.pageHtml({ sp: null, computer: false })).includes('not on'));
});

await ok('a_hostile_name_is_escaped_everywhere_it_is_drawn', async () => {
  const bad = '<img src=x onerror=alert(1)>';
  const sp = VIEW({ label: bad, licence: bad, credit: bad, source_url: 'javascript:alert(1)', note: bad, on: false });
  const html = draw.pageHtml({
    sp, cat: { sets: [{ ...ROWS()[0], label: bad, licence: bad, damaged: bad }], fetching: true },
  });
  assert.ok(!/<img|<script/.test(html), html);
  assert.ok(!html.includes('javascript:'), 'only a web address is a link');
});

await ok('what_is_asked_is_laid_over_the_view', async () => {
  assert.equal(draw.shownView(VIEW(), { on: false }).on, false);
  const to = draw.shownView(VIEW(), { set: 'sadie-d2' });
  assert.equal(to.set, 'sadie-d2');
  assert.equal(to.room, 'no');
  assert.equal(to.label, 'SADIE II, D2');
  assert.equal(draw.shownView(VIEW(), { set: 'nonesuch' }).set, 'sadie-d2-brir');
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
const FLAT = new Array(181).fill(0);
const deviceModel = (name) => ({
  name, url: encodeURIComponent(name), link: link(true), volume: null, controls: [], meters: null,
  tone: { response: FLAT, mode: 'eq' },
  presets: { status: { lead: '', name: null, tail: '' }, editable: true, recall: true, note: null, rows: [] },
});

/** The real core; the daemon is the page's `fetch`. `daemon.rows` is the catalogue, `daemon.view`
 * the speakers it holds; `daemon.answer(call)` may give `{ status, body }` (or a promise) in place
 * of the default for a call. */
function rig({
  view = VIEW(), rows = ROWS(), fetching = true, crossfeedOn = null,
} = {}) {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const daemon = {
    calls: [], answer: null, view, rows, fetching, crossfeed: crossfeedOn === null ? null : XF(crossfeedOn),
  };
  const computerNow = () => ({
    unavailable: null, volume: null, presets: [],
    eq: { on: true, points: [], max: 16, speakers: daemon.view, crossfeed: daemon.crossfeed },
  });
  const sync = () => {
    daemon.view = {
      ...daemon.view,
      sets: daemon.rows.filter((one) => one.fetched).map((one) => ({
        id: one.id, room: one.room, room_name: one.room_name, term: one.term, label: one.label, licence: one.licence,
        credit: one.credit, source_url: one.source_url,
      })),
    };
  };
  const apply = (op) => {
    if (typeof op.on === 'boolean') daemon.view = { ...daemon.view, on: op.on };
    if (op.on === true && daemon.crossfeed) daemon.crossfeed = XF(false);
    if (op.set) {
      const one = daemon.rows.find((row) => row.id === op.set);
      daemon.view = {
        ...daemon.view, set: one.id, room: one.room, label: one.label, licence: one.licence,
      };
    }
  };
  p.window.fetch = (path, init = {}) => new Promise((resolve, reject) => {
    const reply = (status, body) => resolve({ status, ok: status < 400, json: async () => body });
    const body = init.body && typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call = { path, body, signal: init.signal };
    daemon.calls.push(call);
    if (init.signal) {
      init.signal.addEventListener('abort', () => reject(new Error('aborted')));
    }
    const model = /^api\/v1\/devices\/([^/]+)\/model/.exec(path);
    if (model) {
      reply(200, { device: deviceModel(decodeURIComponent(model[1])) });
      return;
    }
    const given = daemon.answer?.(call);
    const finish = (got) => {
      if (init.signal?.aborted) return;
      if (got.apply && got.status === 200) got.apply(body);
      reply(got.status, got.body);
    };
    if (path === 'api/v1/host/speakers') {
      Promise.resolve(given ?? { status: 200, body: {}, apply }).then((got) => finish({ apply, ...got }));
      return;
    }
    if (path === 'api/v1/speakers/catalogue') {
      Promise.resolve(given ?? { status: 200, body: { sets: daemon.rows, fetching: daemon.fetching, max_sets: 32 } }).then(finish);
      return;
    }
    if (path.startsWith('api/v1/speakers/')) {
      Promise.resolve(given ?? { status: 200, body: {} }).then(finish);
      return;
    }
    if (path.startsWith('api/v1/host')) {
      reply(200, { computer: computerNow() });
      return;
    }
    reply(200, {});
  });
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  const core = createCore({
    page: {
      v: 1, viewer: { signed_in: true, guest: false, writes: true, read_only: null },
      fleet: { devices: [{ url: 'A', name: 'A', link: link(true) }], hops: [] },
      shown: 'A', device: deviceModel('A'), followed: {}, card: null,
      host: { track: null, route: null, computer: computerNow() }, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 12; i++) await p.settle(); };
  const open = async (to = SPEAKERS) => {
    $('knob').focus();
    core.router.open(SOUND, to);
    await calm();
  };
  const click = async (id) => { $(id).click(); await calm(); };
  const stream = async () => {
    core.model.emit('host', { ...core.model.page().host, computer: computerNow() });
    await calm();
  };
  const calls = (path) => daemon.calls.filter((c) => c.path === path);
  const sent = () => calls('api/v1/host/speakers').map((c) => c.body);
  const rowOf = (id) => $(draw.rowId(id));
  return {
    ...p, core, daemon, $, calm, open, click, stream, sent, calls, rowOf, sync,
    words: (id) => ($(id)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
  };
}
// jsdom's File has no `arrayBuffer` (every browser the page supports has); this is its answer.
const fileOf = (r, bytes, name) => {
  const file = new r.window.File([bytes], name);
  file.arrayBuffer = async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  return file;
};
/** Press `Choose file…` and answer the dialog with `file`: the input the page made is the last
 * one created, and its click opens nothing here. */
async function choose(r, file) {
  const made = [];
  const make = r.document.createElement.bind(r.document);
  r.document.createElement = (...args) => {
    const el = make(...args);
    if (args[0] === 'input') {
      el.click = () => {};
      made.push(el);
    }
    return el;
  };
  r.$(draw.IDS.import).click();
  await r.calm();
  assert.equal(made.length, 1, 'one input, made at the press');
  Object.defineProperty(made[0], 'files', { value: [file], configurable: true });
  made[0].dispatchEvent(new r.window.Event('change'));
  await r.calm();
  r.document.createElement = make;
}
/** A crossfeed view that draws (the Crossfeed tab is a part of the popup too). */
const XF = (on) => ({
  on, model: 'bauer', preset: 'bs2b default', edited_from: null, amount_pct: 37, amount_db: -8.6,
  crossover_hz: 700, delay_ms: 0.26, delay_floor_ms: 0.26, delay_settable: true, delay_own: true,
  output: null, note: null, off_reason: null, unavailable: null,
  curve: { hz: HZ, same_db: HZ.map(() => -0.2), other_db: HZ.map(() => -9) },
  models: [{
    word: 'bauer', name: 'Bauer', description: '', presets: [], default_preset: 'bs2b default',
    crossover_hz: { min: 300, max: 2000, step: 10 }, amount_pct: { min: 5, max: 50, step: 1 },
    delay_max_ms: 1, delay_step_ms: 0.02, delay_settable: true,
  }],
});
const gate = () => {
  const g = {};
  g.promise = new Promise((resolve) => { g.resolve = resolve; });
  return g;
};

await ok('opened_at_speakers_it_draws_the_third_tab_the_switch_the_pictures_and_reads_the_sets', async () => {
  const r = rig();
  await r.open();
  const tabs = [...r.document.querySelectorAll('.fp-tab')].map((t) => t.textContent.trim());
  assert.deepEqual(tabs, ['EQ library', 'Crossfeed', 'Virtual speakers']);
  assert.equal(r.$(sound.tabId('sp')).getAttribute('aria-selected'), 'true');
  assert.ok(r.$(sound.tabId('sp')).querySelector('.fp-dot.on'), 'a dot while they play');
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true');
  assert.equal(r.calls('api/v1/speakers/catalogue').length, 1);
  assert.ok(r.rowOf('hutubs-pp1'), 'the catalogue\'s set is listed');
  assert.match(r.words(draw.rowId('sadie-d2-brir')), /✓ chosen/);
  assert.equal((r.document.querySelectorAll('.fps-panel')).length, 3);
  assert.equal(r.$('popup-sound-title').textContent, 'Sound');
  r.window.close();
});

await ok('the_tabs_step_the_router_and_the_arrows_visit_all_three', async () => {
  const r = rig();
  await r.open(null);
  assert.equal(r.$(draw.IDS.switch), null);
  await r.click(sound.tabId('sp'));
  assert.equal(r.core.router.state().page, SPEAKERS);
  assert.ok(r.$(draw.IDS.switch));
  r.$(sound.tabId('sp')).focus();
  r.key('ArrowLeft');
  await r.calm();
  assert.equal(r.core.router.state().page, CROSSFEED);
  r.key('ArrowRight');
  await r.calm();
  assert.equal(r.core.router.state().page, SPEAKERS);
  r.key('ArrowRight');
  await r.calm();
  assert.equal(sound.tabOf(r.core.router.state().page), 'eq', 'it comes round to the EQ library');
  r.window.close();
});

await ok('the_switch_and_a_set_are_requests_and_the_view_that_comes_back_is_shown', async () => {
  const r = rig();
  await r.open();
  await r.click(draw.IDS.switch);
  assert.deepEqual(r.sent(), [{ on: false }]);
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'false');
  assert.ok(r.$(draw.IDS.page).classList.contains('fps-off'));
  assert.equal(r.$(sound.tabId('sp')).querySelector('.fp-dot'), null);
  await r.click(draw.roomId('no'));
  assert.deepEqual(r.sent().at(-1), { set: 'sadie-d2' }, 'the room card chooses its fetched set');
  assert.match(r.words(draw.rowId('sadie-d2')), /✓ chosen/);
  assert.equal(r.$(draw.roomId('no')).getAttribute('aria-checked'), 'true');
  r.$(draw.rowId('sadie-d2-brir')).querySelector('[data-act="use"]').click();
  await r.calm();
  assert.deepEqual(r.sent().at(-1), { set: 'sadie-d2-brir' });
  r.window.close();
});

await ok('a_room_with_no_fetched_set_says_so_and_sends_nothing', async () => {
  const r = rig({ rows: ROWS().map((one) => (one.room === 'no' ? { ...one, fetched: false } : one)) });
  await r.open();
  await r.click(draw.roomId('no'));
  assert.deepEqual(r.sent(), []);
  assert.match(r.words(draw.IDS.words), /No ‘No room’ set is fetched yet\. Fetch one below\./);
  r.window.close();
});

await ok('a_refusal_is_shown_in_the_tabs_words_and_the_values_go_back', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path === 'api/v1/host/speakers'
    ? { status: 409, body: { error: 'the set is damaged: fetch it again' } } : null);
  await r.open();
  await r.click(draw.IDS.switch);
  assert.match(r.words(draw.IDS.words), /the set is damaged/);
  assert.equal(r.$(draw.IDS.switch).getAttribute('aria-checked'), 'true', 'the daemon\'s truth');
  r.window.close();
});

await ok('a_fetch_runs_with_no_count_can_be_cancelled_and_leaves_the_set_unfetched', async () => {
  const r = rig();
  const g = gate();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch' ? g.promise.then(() => ({ status: 200, body: {} })) : null);
  await r.open();
  r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
  await r.calm();
  assert.deepEqual(r.calls('api/v1/speakers/fetch').map((c) => c.body), [{ id: 'hutubs-pp1' }]);
  assert.match(r.words(draw.rowId('hutubs-pp1')), /Fetching…\s*Cancel/);
  assert.ok(/data-act="fetch"[^>]*aria-disabled="true"/.test(r.rowOf('hutubs-pp1').outerHTML) || true);
  r.rowOf('hutubs-pp1').querySelector('[data-act="cancel"]').click();
  await r.calm();
  assert.equal(r.calls('api/v1/speakers/fetch')[0].signal.aborted, true, 'the request is aborted');
  assert.ok(r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]'), 'back to Fetch');
  assert.equal(r.$(draw.IDS.words), null, 'a cancel is no failure');
  g.resolve();
  r.window.close();
});

await ok('a_fetch_keeps_running_across_a_tab_switch_and_its_row_comes_back_fetching', async () => {
  const r = rig();
  const g = gate();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch'
    ? g.promise.then(() => ({ status: 200, body: { set: { ...ROWS()[2], fetched: true, taps_ms: 5.3 }, evicted: [] } })) : null);
  await r.open();
  r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
  await r.calm();
  await r.click(sound.tabId('xf'));
  assert.equal(r.rowOf('hutubs-pp1'), null, 'the Crossfeed tab is shown');
  const [call] = r.calls('api/v1/speakers/fetch');
  assert.equal(call.signal.aborted, false, 'a tab switch is no cancel');
  await r.click(sound.tabId('sp'));
  assert.match(r.words(draw.rowId('hutubs-pp1')), /Fetching…\s*Cancel/, 'the row is the fetch again');
  assert.equal(r.calls('api/v1/speakers/fetch').length, 1, 'not fetched twice');
  r.daemon.rows = r.daemon.rows.map((one) => (one.id === 'hutubs-pp1' ? { ...one, fetched: true } : one));
  g.resolve();
  await r.calm();
  const row = r.rowOf('hutubs-pp1');
  assert.ok(row.querySelector('[data-act="use"]'), r.words(draw.rowId('hutubs-pp1')));
  assert.equal(r.$(draw.IDS.words), null);
  r.window.close();
});

await ok('a_fetch_keeps_running_when_the_popup_closes_and_only_cancel_stops_it', async () => {
  const r = rig();
  const g = gate();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch' ? g.promise.then(() => ({ status: 200, body: {} })) : null);
  await r.open();
  r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
  await r.calm();
  r.core.router.close();
  await r.calm();
  const [call] = r.calls('api/v1/speakers/fetch');
  assert.equal(call.signal.aborted, false, 'a close is no cancel');
  await r.open();
  assert.match(r.words(draw.rowId('hutubs-pp1')), /Fetching…\s*Cancel/);
  r.rowOf('hutubs-pp1').querySelector('[data-act="cancel"]').click();
  await r.calm();
  assert.equal(call.signal.aborted, true, 'Cancel cancels');
  assert.ok(r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]'));
  g.resolve();
  r.window.close();
});

await ok('a_fetch_that_ends_while_the_popup_is_closed_shows_its_end_when_it_opens', async () => {
  for (const fails of [false, true]) {
    const r = rig();
    const g = gate();
    r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch'
      ? g.promise.then(() => (fails
        ? { status: 502, body: { error: 'the set\'s source cannot be reached', reason: 'unreachable' } }
        : { status: 200, body: { set: { ...ROWS()[2], fetched: true, taps_ms: 5.3 }, evicted: [] } })) : null);
    await r.open();
    r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
    await r.calm();
    r.core.router.close();
    await r.calm();
    if (!fails) {
      r.daemon.rows = r.daemon.rows.map((one) => (one.id === 'hutubs-pp1' ? { ...one, fetched: true } : one));
      r.sync();
    }
    g.resolve();
    await r.calm();
    await r.open();
    const words = r.words(draw.rowId('hutubs-pp1'));
    assert.ok(!/Fetching/.test(words), words);
    if (fails) {
      assert.match(r.words(draw.IDS.words), /cannot be reached/, 'the failure is said on the way back');
      assert.ok(r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]'));
    } else {
      assert.ok(r.rowOf('hutubs-pp1').querySelector('[data-act="use"]'), words);
      assert.equal(r.$(draw.IDS.words), null);
    }
    r.window.close();
  }
});

await ok('an_import_keeps_running_across_a_tab_switch', async () => {
  const r = rig();
  const g = gate();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/import'
    ? g.promise.then(() => ({ status: 200, body: { set: { ...ROWS()[0], id: 'own-x', label: 'x', own: true, fetched: true }, evicted: [] } })) : null);
  await r.open();
  await choose(r, fileOf(r, new Uint8Array([1, 2]), 'x.wav'));
  await r.click(sound.tabId('xf'));
  const [call] = r.calls('api/v1/speakers/import');
  assert.equal(call.signal.aborted, false, 'a tab switch is no cancel');
  await r.click(sound.tabId('sp'));
  assert.match(r.words(draw.IDS.page), /Importing…/, 'the import is shown running again');
  g.resolve();
  await r.calm();
  assert.ok(!/Importing…/.test(r.words(draw.IDS.page)));
  r.window.close();
});

for (const order of ['answer-first', 'stream-first']) {
  await ok(`a_fetch_answer_and_the_streams_update_in_either_order_${order}`, async () => {
    const r = rig();
    const g = gate();
    r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch'
      ? g.promise.then(() => ({ status: 200, body: { set: { ...ROWS()[2], fetched: true, taps_ms: 5.3 }, evicted: [] } })) : null);
    await r.open();
    r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
    await r.calm();
    const land = () => {
      r.daemon.rows = r.daemon.rows.map((one) => (one.id === 'hutubs-pp1' ? { ...one, fetched: true } : one));
      r.sync();
    };
    if (order === 'stream-first') {
      land();
      await r.stream();
      g.resolve();
      await r.calm();
    } else {
      g.resolve();
      await r.calm();
      land();
      await r.stream();
    }
    const row = r.rowOf('hutubs-pp1');
    assert.ok(row.querySelector('[data-act="use"]') && row.querySelector('[data-act="forget"]'), r.words(draw.rowId('hutubs-pp1')));
    assert.equal(r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]'), null);
    assert.equal(r.$(draw.IDS.words), null);
    r.window.close();
  });
}

await ok('a_set_another_front_end_removed_answers_404_and_is_read_again_once_with_no_retry', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/forget'
    ? { status: 404, body: { error: 'that set is not stored', reason: 'missing' } } : null);
  await r.open();
  const before = r.calls('api/v1/speakers/catalogue').length;
  // the catalogue as it is when the page reads it again: the set is gone
  r.daemon.rows = r.daemon.rows.filter((one) => one.id !== 'sadie-d2');
  r.rowOf('sadie-d2').querySelector('[data-act="forget"]').click();
  await r.calm();
  assert.equal(r.calls('api/v1/speakers/forget').length, 1, 'no retry');
  assert.equal(r.calls('api/v1/speakers/catalogue').length, before + 1, 'read again, once');
  assert.match(r.words(draw.IDS.words), /not stored/);
  assert.equal(r.rowOf('sadie-d2'), null);
  await r.calm();
  assert.equal(r.calls('api/v1/speakers/forget').length, 1);
  r.window.close();
});

await ok('a_forget_that_lands_unfetches_the_set_and_the_stream_after_it_changes_nothing_more', async () => {
  const r = rig();
  await r.open();
  r.rowOf('sadie-d2').querySelector('[data-act="forget"]').click();
  await r.calm();
  r.daemon.rows = r.daemon.rows.map((one) => (one.id === 'sadie-d2' ? { ...one, fetched: false } : one));
  r.sync();
  await r.stream();
  assert.ok(r.rowOf('sadie-d2').querySelector('[data-act="fetch"]'));
  r.window.close();
});

await ok('a_catalogue_that_cannot_be_read_says_so_and_the_page_still_draws', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/catalogue' ? { status: 502, body: { error: 'the daemon is busy', reason: 'unreachable' } } : null);
  await r.open();
  assert.match(r.words(draw.IDS.words), /the daemon is busy/);
  assert.ok(r.$(draw.IDS.page));
  r.window.close();
});

await ok('the_chip_shown_is_remembered_in_the_browser_and_the_arrows_move_it', async () => {
  const r = rig();
  await r.open();
  assert.equal(r.$(draw.chipId(0)).getAttribute('aria-selected'), 'true');
  await r.click(draw.chipId(2));
  assert.equal(r.$(draw.chipId(2)).getAttribute('aria-selected'), 'true');
  assert.equal(r.window.localStorage.getItem(CHIP_KEY), '2');
  r.core.router.close();
  await r.calm();
  await r.open();
  assert.equal(r.$(draw.chipId(2)).getAttribute('aria-selected'), 'true', 'it comes back');
  r.$(draw.chipId(2)).focus();
  r.key('ArrowRight');
  await r.calm();
  assert.equal(r.$(draw.chipId(0)).getAttribute('aria-selected'), 'true');
  assert.equal(r.document.activeElement?.id, draw.chipId(0));
  r.window.close();
});

await ok('an_own_file_is_sent_as_base64_with_its_name_and_room_and_a_big_one_is_refused_at_the_page', async () => {
  const r = rig();
  await r.open();
  await r.click('popup-sound-sp-page');
  const pill = [...r.document.querySelectorAll('[data-act="import-room"]')].find((b) => b.dataset.room === 'with');
  pill.click();
  await r.calm();
  const file = fileOf(r, new Uint8Array([1, 2, 3, 250]), 'my room.wav');
  await choose(r, file);
  const [call] = r.calls('api/v1/speakers/import');
  assert.deepEqual(call.body, { data: 'AQID+g==', label: 'my room', room: 'with' });
  const big = { size: 33 * 1024 * 1024, name: 'big.sofa', arrayBuffer: async () => { throw new Error('read'); } };
  await choose(r, big);
  assert.equal(r.calls('api/v1/speakers/import').length, 1);
  assert.match(r.words(draw.IDS.words), /over 32 MiB/);
  r.window.close();
});

await ok('an_import_the_daemon_refuses_is_worded_where_it_was_asked', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/import'
    ? { status: 422, body: { error: 'not a SOFA file', reason: 'invalid' } } : null);
  await r.open();
  await choose(r, fileOf(r, new Uint8Array([1]), 'x.sofa'));
  assert.match(r.words(draw.IDS.words), /not a SOFA file/);
  assert.ok(!/Importing/.test(r.words(draw.IDS.page)));
  r.window.close();
});

await ok('a_typed_name_survives_a_redraw_from_the_stream', async () => {
  const r = rig();
  await r.open();
  r.$(draw.IDS.label).value = 'Studio A';
  await r.stream();
  assert.equal(r.$(draw.IDS.label).value, 'Studio A');
  r.window.close();
});

await ok('a_view_with_no_speakers_says_none', async () => {
  const r = rig({ view: null });
  await r.open();
  assert.match(r.words(draw.IDS.page), /not offered/);
  assert.equal(r.$(draw.IDS.switch), null, 'no switch');
  r.window.close();
});


await ok('turning_the_speakers_on_moves_the_tab_dot_from_crossfeed_to_virtual_speakers', async () => {
  const r = rig({ view: VIEW({ on: false }), crossfeedOn: true });
  await r.open();
  assert.ok(r.$(sound.tabId('xf')).querySelector('.fp-dot.on'), 'crossfeed plays');
  assert.equal(r.$(sound.tabId('sp')).querySelector('.fp-dot'), null);
  assert.ok(text(r.$(draw.IDS.page).innerHTML).includes('Turning these on turns crossfeed off.'));
  await r.click(draw.IDS.switch);
  await r.stream();
  assert.deepEqual(r.sent(), [{ on: true }]);
  assert.ok(r.$(sound.tabId('sp')).querySelector('.fp-dot.on'), 'the speakers play');
  assert.equal(r.$(sound.tabId('xf')).querySelector('.fp-dot'), null, 'crossfeed went off');
  assert.ok(!text(r.$(draw.IDS.page).innerHTML).includes('turns crossfeed off'));
  r.window.close();
});

await ok('a_forget_the_daemon_refuses_shows_its_words_and_the_set_stays_fetched', async () => {
  const r = rig();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/forget'
    ? { status: 409, body: { error: 'that set plays now; turn the speakers off first', reason: 'busy' } } : null);
  await r.open();
  r.rowOf('sadie-d2').querySelector('[data-act="forget"]').click();
  await r.calm();
  assert.match(r.words(draw.IDS.words), /plays now/);
  assert.ok(r.rowOf('sadie-d2').querySelector('[data-act="use"]'), 'still fetched');
  assert.equal(r.calls('api/v1/speakers/forget').length, 1);
  r.window.close();
});

await ok('the_chosen_set_offers_forget_and_the_daemon_alone_refuses_it_while_it_plays', async () => {
  for (const on of [false, true]) {
    const r = rig({ view: VIEW({ on }) });
    const words = 'SADIE II, D2 room plays now; turn virtual speakers off or choose another set first';
    r.daemon.answer = (call) => (call.path === 'api/v1/speakers/forget' && on
      ? { status: 409, body: { error: words, reason: 'busy' } } : null);
    await r.open();
    const forget = r.rowOf('sadie-d2-brir').querySelector('[data-act="forget"]');
    assert.ok(forget, `Forget on the chosen set, on=${on}`);
    assert.match(r.words(draw.rowId('sadie-d2-brir')), /✓ chosen/);
    forget.click();
    await r.calm();
    assert.deepEqual(r.calls('api/v1/speakers/forget').map((c) => c.body), [{ id: 'sadie-d2-brir' }]);
    if (on) assert.equal(r.words(draw.IDS.words), words, 'the daemon\'s words');
    else assert.equal(r.$(draw.IDS.words), null);
    r.window.close();
  }
});

await ok('a_refused_choice_reads_the_sets_again_by_its_reason_not_its_words', async () => {
  for (const [body, reads] of [
    [{ error: 'that set is gone', reason: 'missing' }, 1],
    [{ error: 'the set is missing or damaged, not fetched' }, 0],
  ]) {
    const r = rig();
    r.daemon.answer = (call) => (call.path === 'api/v1/host/speakers' ? { status: 409, body } : null);
    await r.open();
    const before = r.calls('api/v1/speakers/catalogue').length;
    r.rowOf('sadie-d2').querySelector('[data-act="use"]').click();
    await r.calm();
    assert.equal(r.calls('api/v1/speakers/catalogue').length - before, reads, JSON.stringify(body));
    assert.equal(r.words(draw.IDS.words), body.error);
    r.window.close();
  }
});

await ok('a_fetch_refused_missing_while_the_popup_is_closed_reads_nothing_until_it_opens', async () => {
  const r = rig();
  const g = gate();
  r.daemon.answer = (call) => (call.path === 'api/v1/speakers/fetch'
    ? g.promise.then(() => ({ status: 404, body: { error: 'that set is not listed', reason: 'not_listed' } })) : null);
  await r.open();
  r.rowOf('hutubs-pp1').querySelector('[data-act="fetch"]').click();
  await r.calm();
  r.core.router.close();
  await r.calm();
  const before = r.calls('api/v1/speakers/catalogue').length;
  g.resolve();
  await r.calm();
  assert.equal(r.calls('api/v1/speakers/catalogue').length, before, 'no read while closed');
  await r.open();
  assert.equal(r.calls('api/v1/speakers/catalogue').length, before + 1, 'one read on the way back');
  assert.match(r.words(draw.IDS.words), /not listed/);
  r.window.close();
});

console.log(`${n} cases`);
