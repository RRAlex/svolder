/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Scenes popup (core/scenes.js, core/scenes-actions.js, core/popups/scenes/
// {draw,wire}.js, the `scenes` event of core/stream.js) on the real core over a jsdom page and a
// small fake daemon that answers the routes of the contract (`api/v1/scenes/...`) the way the
// daemon does: 200, 409 with an `ask`, 422 for `bad_name` and `taken`, 409 with a `reason` for the
// rest, and a 404 for a scene that was taken away. First the plain data (the view, the words), then
// the actions on fakes, then the popup: who has it (a guest and a read-only page have none), the
// list and its lamp (lit only when the view says it is settled), a recall's answer and the stream's
// `scenes` event in either order (the row never goes back), the asks in place, the refusals by
// reason, a scene deleted elsewhere. Run: node core-scenes.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { createStore } = await import('../../../bins/svolderd/web/core/store.js');
const { createStream } = await import('../../../bins/svolderd/web/core/stream.js');
const { createWrites } = await import('../../../bins/svolderd/web/core/write.js');
const { frameRows } = await import('../../../bins/svolderd/web/core/menu-rows.js');
const { POPUPS, SCENES, seenBy } = await import(
  '../../../bins/svolderd/web/core/popups/listed.js');
const scenes = await import('../../../bins/svolderd/web/core/scenes.js');
const { createScenesActions, GONE } = await import(
  '../../../bins/svolderd/web/core/scenes-actions.js');
const draw = await import('../../../bins/svolderd/web/core/popups/scenes/draw.js');

let n = 0;
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};
console.warn = () => {};

const text = (html) => html.replace(/<[^>]*>/g, ' ').replaceAll('&#39;', "'").replace(/\s+/g, ' ').trim();

// ---- the daemon's view ----

const PART = (who, present = true) => ({ who, present });
const SCENE = (name, ...parts) => ({ name, parts: parts.length > 0 ? parts : [PART('computer'), PART('Desk DAC')] });
/** Three scenes; Movie leaves Hall, which is not connected. */
const VIEW = (over = {}) => ({
  scenes: [
    SCENE('Late night'), SCENE('Desk'),
    SCENE('Movie', PART('computer'), PART('Desk DAC'), PART('Hall', false)),
  ],
  on: null, recalled: null, known: true, ...over,
});

// ---- part 1: the data and the words ----

await ok('the_view_is_read_as_the_daemon_gives_it_and_a_page_without_one_has_none', () => {
  assert.equal(scenes.viewOf(undefined), null);
  assert.equal(scenes.viewOf(null), null);
  assert.equal(scenes.viewOf({}), null, 'no list, no view');
  const view = scenes.viewOf(VIEW({ on: 'Desk', recalled: 'Movie' }));
  assert.equal(view.on, 'Desk');
  assert.equal(view.recalled, 'Movie');
  assert.equal(view.known, true);
  assert.deepEqual(view.scenes[2].parts, [PART('computer'), PART('Desk DAC'), PART('Hall', false)]);
  assert.equal(scenes.viewOf({ scenes: [] }).known, false, 'a view that does not say is unsettled');
});

await ok('on_is_a_scene_only_while_the_view_is_settled', () => {
  assert.equal(scenes.onNow(scenes.viewOf(VIEW({ on: 'Desk' }))), 'Desk');
  assert.equal(scenes.onNow(scenes.viewOf(VIEW({ on: 'Desk', known: false }))), null);
  assert.equal(scenes.onNow(scenes.viewOf(VIEW())), null);
  assert.equal(scenes.onNow(null), null);
});

await ok('a_part_that_is_not_connected_is_a_dashed_chip_with_its_words', () => {
  const chips = scenes.chipsOf(scenes.viewOf(VIEW()).scenes[2]);
  assert.deepEqual(chips.map((c) => c.label), ['computer', 'Desk DAC', 'Hall · not connected']);
  assert.deepEqual(chips.map((c) => c.present), [true, true, false]);
});

await ok('the_line_under_the_field_says_what_a_save_takes_and_leaves_out', () => {
  const fleet = (...rows) => ({
    host: { computer: {} }, fleet: { devices: rows.map(([name, on]) => ({
      name, link: { state: on ? 'on' : 'off' },
    })) },
  });
  assert.equal(scenes.saveHint(fleet(['Desk DAC', true], ['Hall', false])),
    'Saves the computer and Desk DAC. Hall is not connected and is left out. '
    + 'No volume, gain, mute or power is saved.');
  assert.equal(scenes.saveHint(fleet(['Desk DAC', true], ['Hall', false], ['Attic', false])),
    'Saves the computer and Desk DAC. Hall and Attic are not connected and are left out. '
    + 'No volume, gain, mute or power is saved.');
  assert.equal(scenes.saveHint({ host: null, fleet: { devices: [] } }),
    'Nothing is connected to save. No volume, gain, mute or power is saved.');
  assert.equal(scenes.saveHint({ host: null, fleet: { devices: [{ name: 'Kara', link: { state: 'on' } }] } }),
    'Saves Kara. No volume, gain, mute or power is saved.');
});

await ok('a_recall_says_how_each_part_fared_in_the_daemons_words', () => {
  const part = (who, status, ...notes) => ({ who, status, notes });
  assert.deepEqual(scenes.outcomeOf('Movie', [part('computer', 'done'), part('Desk DAC', 'done')]),
    { headline: 'Movie recalled.', set: 'The computer and Desk DAC are set.', problems: [] });
  const left = scenes.outcomeOf('Movie', [
    part('computer', 'done'), part('Desk DAC', 'done'),
    part('Hall', 'left', 'Hall is not connected: its part was left as it is'),
  ]);
  assert.equal(left.headline, 'Movie recalled in part.', 'a part left with no word that it is away');
  assert.deepEqual(left.problems, ['Hall is not connected: its part was left as it is.'],
    'a note ends with a full stop');
  const away = scenes.outcomeOf('Movie', [
    part('computer', 'done'), part('Desk DAC', 'done'),
    part('Hall', 'left', 'Hall is not connected: its part was left as it is'),
  ], ['Hall']);
  assert.equal(away.headline, 'Movie recalled.', 'a device that is only not connected: recalled');
  assert.deepEqual(away.problems, ['Hall is not connected: its part was left as it is.'], 'and its note');
  const failedToo = scenes.outcomeOf('Movie', [
    part('computer', 'done'), part('Desk DAC', 'failed', 'It said no', 'Try again.'),
    part('Hall', 'left'),
  ], ['Hall']);
  assert.equal(failedToo.headline, 'Movie recalled in part.', 'a part that failed makes it partial');
  assert.deepEqual(failedToo.problems, ['It said no. Try again.', 'Hall was left as it was.']);
  assert.equal(scenes.outcomeOf('Movie', [part('Hall', 'left')], ['Hall']).headline,
    'Movie was not recalled.', 'nothing was set');
  assert.equal(scenes.outcomeOf('Movie', [part('Hall', 'left')]).headline, 'Movie was not recalled.');
  assert.deepEqual(scenes.outcomeOf('Movie', [part('Hall', 'left')]).problems, ['Hall was left as it was.']);
  assert.deepEqual(scenes.outcomeOf('Movie', [part('Hall', 'failed')]).problems, ['Hall could not be set.']);
  assert.deepEqual(scenes.outcomeOf('Movie', [part('Hall', 'partial')]).problems, ['Hall was set in part.']);
  assert.equal(scenes.outcomeOf('Movie', [part('Hall', 'partial')]).headline, 'Movie recalled in part.');
  assert.equal(scenes.outcomeOf('Movie', []).headline, 'Movie recalled.');
  assert.equal(scenes.outcomeOf('Movie', [part('Hall', 'unknown')]).problems[0], 'Hall gave no answer.');
});

await ok('the_list_lights_the_scene_on_only_when_settled_and_says_reading_otherwise', () => {
  const lit = (html) => [...html.matchAll(/sc-row on[^>]*data-name="([^"]*)"/g)].map((m) => m[1]);
  assert.deepEqual(lit(draw.listHtml(scenes.viewOf(VIEW({ on: 'Desk' })))), ['Desk']);
  const html = draw.listHtml(scenes.viewOf(VIEW({ on: 'Desk' })));
  assert.equal((html.match(/On now/g) ?? []).length, 1);
  assert.match(html, /aria-current="true"/);
  const unsettled = scenes.viewOf(VIEW({ on: 'Desk', known: false }));
  assert.deepEqual(lit(draw.listHtml(unsettled)), []);
  assert.ok(!draw.listHtml(unsettled).includes('On now'), 'no lamp is lit on a guess');
  assert.match(draw.readingHtml(unsettled), /reading…/);
  assert.equal(draw.readingHtml(scenes.viewOf(VIEW())), '');
  assert.match(text(draw.listHtml(scenes.viewOf(VIEW({ scenes: [] })))), /No scenes yet/);
  const row = draw.listHtml(scenes.viewOf(VIEW()));
  assert.ok(row.includes('sc-off'), 'the part not connected is dashed');
  assert.ok(row.includes('aria-label="Recall Late night"') && row.includes('aria-label="Rename Movie"')
    && row.includes('aria-label="Delete Desk"'));
});

await ok('a_name_with_markup_is_text_in_the_list_and_the_questions', () => {
  const view = scenes.viewOf({ scenes: [SCENE('<b>x</b> "q"')], known: true });
  const html = draw.listHtml(view);
  assert.ok(!html.includes('<b>x</b>'));
  assert.ok(html.includes('&lt;b&gt;x&lt;/b&gt; &quot;q&quot;'));
});

await ok('the_popup_is_listed_after_sound_for_a_writer_and_stays', () => {
  assert.deepEqual(POPUPS.map((p) => p.name), ['devices', 'sound', 'scenes', 'setup', 'controls', 'help']);
  const entry = POPUPS.find((p) => p.name === SCENES);
  assert.deepEqual([entry.title, entry.row, entry.stays], ['Scenes', { label: 'Scenes…', sees: 'writer' }, true]);
  const rows = (viewer) => frameRows({ viewer, run: { popup() {} }, popups: POPUPS }).map((r) => r.label);
  assert.ok(rows({ writes: true }).includes('Scenes…'));
  assert.ok(!rows({ guest: true }).includes('Scenes…'), 'a guest has no row');
  assert.ok(!rows({ writes: false }).includes('Scenes…'), 'a read-only page has none');
  assert.equal(seenBy(entry, { writes: true }), true);
  assert.equal(seenBy(entry, { guest: true }), false);
  assert.equal(seenBy(entry, { writes: false }), false);
});

// ---- part 2: the actions on fakes ----

const fakeActions = (answer, { viewer = { writes: true } } = {}) => {
  const posts = [];
  const gets = [];
  const store = createStore({
    v: 1, viewer, fleet: { devices: [], hops: [] }, shown: null, device: null, followed: {},
    card: null, host: null, strip: null,
  }, {
    fetch: async () => ({ status: 200, json: async () => ({}) }),
  });
  const seen = [];
  store.on('scenes', (view) => seen.push(view));
  const toasts = [];
  const post = async (path, body) => {
    posts.push([path, body]);
    return answer(path, body);
  };
  const writes = createWrites({ store, post, toast: (words) => toasts.push(words) });
  const fetch = async (path) => {
    gets.push(path);
    return { status: 200, json: async () => ({ ...VIEW({ scenes: [SCENE('Late night')] }) }) };
  };
  const actions = createScenesActions({ store, post, writes, fetch, signedOut() {} });
  return { actions, posts, gets, seen, toasts, store };
};

await ok('every_write_goes_to_its_route_with_the_body_the_contract_gives', async () => {
  const r = fakeActions(async (path, body) => ({ status: 200, name: body.name ?? body.to }));
  assert.deepEqual((await r.actions.save('Late night')).name, 'Late night');
  await r.actions.save('Late night', { force: true });
  await r.actions.rename('Desk', 'Desk 2');
  await r.actions.delete('Movie');
  assert.deepEqual(r.posts, [
    ['api/v1/scenes/save', { name: 'Late night' }],
    ['api/v1/scenes/save', { name: 'Late night', force: true }],
    ['api/v1/scenes/rename', { from: 'Desk', to: 'Desk 2' }],
    ['api/v1/scenes/delete', { name: 'Movie' }],
  ]);
  const recall = fakeActions(async () => ({ status: 200, name: 'Movie', outcome: [] }));
  assert.equal((await recall.actions.recall('Movie')).ok, true);
  assert.deepEqual(recall.posts, [['api/v1/scenes/recall', { name: 'Movie' }]]);
});

await ok('a_refusal_carries_its_words_and_reason_and_is_not_toasted', async () => {
  const r = fakeActions(async () => ({
    status: 422, error: 'A scene named Desk is there already.', reason: 'taken',
  }));
  const answer = await r.actions.save('Desk');
  assert.equal(answer.ok, false);
  assert.equal(answer.reason, 'taken');
  assert.equal(answer.error, 'A scene named Desk is there already.');
  assert.deepEqual(r.toasts, [], 'the popup says it in place');
  const busy = fakeActions(async () => ({ status: 409, error: 'A device is still reading.', reason: 'busy' }));
  assert.equal((await busy.actions.save('x')).reason, 'busy');
  const bare = fakeActions(async () => ({ status: 409 }));
  assert.equal((await bare.actions.delete('x')).error, 'Refused.');
});

await ok('the_both_sides_question_is_asked_and_a_yes_sends_confirm_a_no_sends_nothing', async () => {
  const question = 'Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway?';
  const daemon = async (path, body) => (body.confirm
    ? { status: 200, name: 'Late night', outcome: [] }
    : {
      status: 409, error: question,
      ask: { kind: 'both_sides', name: 'Late night', devices: ['Desk DAC'], question },
    });
  const yes = fakeActions(daemon);
  const asked = [];
  const done = await yes.actions.recall('Late night', { asker: async (q) => { asked.push(q); return true; } });
  assert.deepEqual(asked, [question]);
  assert.equal(done.ok, true);
  assert.deepEqual(yes.posts.map(([, body]) => body), [{ name: 'Late night' }, { name: 'Late night', confirm: true }]);
  const no = fakeActions(daemon);
  const declined = await no.actions.recall('Late night', { asker: async () => false });
  assert.equal(declined.declined, true);
  assert.equal(no.posts.length, 1, 'a no sends nothing more');
  const none = fakeActions(daemon);
  const handed = await none.actions.recall('Late night');
  assert.equal(handed.ok, false);
  assert.equal(handed.ask.kind, 'both_sides', 'with no asker the question is handed back');
});

await ok('a_scene_taken_away_elsewhere_is_gone_by_404_or_missing_and_the_view_is_read_once', async () => {
  for (const reply of [{ status: 404, error: 'No such scene.' }, { status: 409, reason: 'missing', error: 'x' }]) {
    const r = fakeActions(async () => reply);
    const answer = await r.actions.delete('Movie');
    assert.equal(answer.ok, false);
    assert.equal(answer.gone, true);
    assert.equal(answer.error, GONE);
    assert.deepEqual(r.gets, ['api/v1/scenes'], 'one read, no retry of the write');
    assert.equal(r.posts.length, 1);
    assert.equal(r.seen.length, 1, 'the page is told the view the daemon holds');
    assert.equal(r.store.page().scenes.scenes.length, 1);
  }
});

await ok('a_guest_and_a_read_only_page_send_nothing', async () => {
  for (const viewer of [{ guest: true }, { writes: false, read_only: 'Read-only: writes are off' }]) {
    const r = fakeActions(async () => ({ status: 200 }), { viewer });
    for (const call of [() => r.actions.save('x'), () => r.actions.recall('x'),
      () => r.actions.rename('x', 'y'), () => r.actions.delete('x')]) {
      const answer = await call();
      assert.equal(answer.ok, false);
    }
    assert.deepEqual(r.posts, []);
  }
});

// ---- part 3: the stream's event ----

await ok('the_stream_hands_the_scenes_event_to_the_store_and_the_events_address_is_unchanged', async () => {
  const sources = [];
  class Source {
    constructor(url) { this.url = url; this.handlers = {}; sources.push(this); }

    addEventListener(name, fn) { (this.handlers[name] ??= []).push(fn); }

    close() {}
  }
  const store = createStore({
    v: 1, viewer: { writes: true }, fleet: { devices: [], hops: [] }, shown: 'A', device: {},
    followed: {}, card: null, host: null, strip: null, scenes: VIEW(),
  }, { fetch: async () => ({ status: 200, json: async () => ({}) }) });
  const stream = createStream({
    store, EventSource: Source, fetch: async () => ({ json: async () => ({}) }),
    watchVisible: () => ({ stop() {}, observe() {} }), signedOut() {}, reload() {}, show() {},
    doc: { visibilityState: 'visible', body: { classList: { toggle() {} } }, dispatchEvent() {} },
  });
  stream.want('theme', { needs: ['meters', 'host'] });
  stream.open();
  const [source] = sources;
  assert.equal(source.url, 'api/v1/events?device=A&needs=meters,host', 'no needs word for scenes');
  const heard = [];
  store.on('scenes', (view) => heard.push(view));
  const next = VIEW({ on: 'Desk' });
  for (const fn of source.handlers.scenes) fn({ data: JSON.stringify(next) });
  assert.deepEqual(heard, [next]);
  assert.deepEqual(store.page().scenes, next, 'the store keeps it for a popup opened later');
  stream.close();
});

// ---- part 4: the popup on the real core ----

class FakeSource {
  addEventListener() {}

  close() {}
}
const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });

const QUESTION = 'Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway?';

/** The real core with a daemon behind `fetch`: the scenes it holds (`daemon.view`), each route
 * answering by `daemon.answer(call)` or by the default, which changes the view the way the daemon
 * does. `daemon.hold` is a promise a route waits for, to put an answer after an event. */
function rig({ viewer = {}, scenes: given = VIEW() } = {}) {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.Element.prototype.scrollIntoView = () => {};
  const daemon = { calls: [], answer: null, view: structuredClone(given ?? VIEW()), hold: null };
  const reply = (resolve) => (status, body) => resolve({ status, ok: status < 400, json: async () => body });
  const byName = (name) => daemon.view.scenes.findIndex((s) => s.name === name);
  const apply = (path, body) => {
    if (path.endsWith('/save')) {
      const at = byName(body.name);
      if (at >= 0 && !body.force) {
        return { status: 422, body: { error: `A scene named ${body.name} is there already.`, reason: 'taken' } };
      }
      if (body.name.trim() === '') return { status: 422, body: { error: 'A scene needs a name.', reason: 'bad_name' } };
      if (at < 0) daemon.view.scenes.push(SCENE(body.name));
      return {
        status: 200,
        body: {
          name: body.name,
          outcome: [{ who: 'Hall', status: 'left', notes: ['Hall is not connected: it is not in the scene'] }],
        },
        changed: true,
      };
    }
    if (path.endsWith('/recall')) {
      if (byName(body.name) < 0) return { status: 404, body: { error: 'No such scene.' } };
      const both = body.name === 'Late night' && !body.confirm && daemon.both;
      if (both) {
        return {
          status: 409,
          body: { error: QUESTION, ask: { kind: 'both_sides', name: body.name, devices: ['Desk DAC'], question: QUESTION } },
        };
      }
      daemon.view.on = body.name;
      daemon.view.recalled = body.name;
      return {
        status: 200,
        body: {
          name: body.name,
          outcome: daemon.view.scenes[byName(body.name)].parts.map((part) => (part.present
            ? { who: part.who, status: 'done', notes: [] }
            : { who: part.who, status: 'left', notes: [`${part.who} is not connected: its part was left as it is`] })),
        },
        changed: true,
      };
    }
    if (path.endsWith('/rename')) {
      const at = byName(body.from);
      if (at < 0) return { status: 404, body: { error: 'No such scene.' } };
      if (byName(body.to) >= 0) return { status: 422, body: { error: `A scene named ${body.to} is there already.`, reason: 'taken' } };
      daemon.view.scenes[at].name = body.to;
      if (daemon.view.on === body.from) daemon.view.on = body.to;
      return { status: 200, body: { name: body.to }, changed: true };
    }
    if (path.endsWith('/delete')) {
      const at = byName(body.name);
      if (at < 0) return { status: 404, body: { error: 'No such scene.' } };
      daemon.view.scenes.splice(at, 1);
      if (daemon.view.on === body.name) daemon.view.on = null;
      return { status: 200, body: { name: body.name }, changed: true };
    }
    return { status: 404, body: {} };
  };
  p.window.fetch = (path, init = {}) => new Promise((resolve) => {
    const done = reply(resolve);
    const body = init.body && typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    const call = { path, body };
    daemon.calls.push(call);
    if (path === 'api/v1/scenes') {
      done(200, structuredClone(daemon.view));
      return;
    }
    if (path.startsWith('api/v1/scenes/')) {
      const got = daemon.answer?.(call) ?? apply(path, body);
      Promise.resolve(daemon.hold).then(() => {
        if (got.changed) daemon.changed?.();
        done(got.status, got.body);
      });
      return;
    }
    done(200, {});
  });
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  const core = createCore({
    page: {
      v: 1, viewer: { signed_in: true, guest: false, writes: true, read_only: null, ...viewer },
      fleet: {
        devices: [{ url: 'A', name: 'Desk DAC', link: link(true) },
          { url: 'H', name: 'Hall', link: link(false) }],
        hops: [],
      },
      shown: 'A', device: { name: 'Desk DAC', url: 'A', link: link(true), volume: null, controls: [], meters: null },
      followed: {}, card: null,
      host: { track: null, route: null, computer: { unavailable: null, volume: null, presets: [], eq: { on: true, points: [] } } },
      strip: null,
      ...(viewer.guest || viewer.writes === false || given === null
        ? {} : { scenes: structuredClone(given) }),
    },
    themes: [],
    window: p.window,
  });
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 12; i++) await p.settle(); };
  const open = async () => {
    $('knob').focus();
    core.router.open(SCENES);
    await calm();
  };
  /** What the stream tells the page: the view as the daemon holds it now. */
  const event = async (view = daemon.view) => {
    core.model.emit('scenes', structuredClone(view));
    await calm();
  };
  const click = async (el) => { el.click(); await calm(); };
  const rows = () => [...p.document.querySelectorAll('#popup-scenes-list .sc-row')];
  const names = () => rows().map((row) => row.dataset.name);
  const lit = () => rows().filter((row) => row.classList.contains('on')).map((row) => row.dataset.name);
  const say = () => ($('popup-scenes-say')?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const posts = () => daemon.calls.filter((c) => c.path !== 'api/v1/scenes' && c.body);
  const rowOf = (name) => rows().find((row) => row.dataset.name === name);
  const act = (name, what) => rowOf(name).querySelector(`[data-act="${what}"]`);
  return {
    ...p, core, daemon, $, calm, open, event, click, rows, names, lit, say, posts, rowOf, act,
  };
}

const type = (r, id, value) => {
  const input = r.$(id);
  input.value = value;
  input.dispatchEvent(new r.window.Event('input', { bubbles: true }));
};
const submit = async (r) => {
  r.$(draw.IDS.save).dispatchEvent(new r.window.Event('submit', { bubbles: true, cancelable: true }));
  await r.calm();
};

await ok('a_writer_opens_the_list_with_the_chips_the_lamp_and_the_save_line', async () => {
  const r = rig();
  r.daemon.view.on = 'Desk';
  await r.event();
  await r.open();
  assert.equal(r.core.router.state().popup, SCENES);
  assert.equal(r.$('popup-scenes-title').textContent, 'Scenes');
  assert.deepEqual(r.names(), ['Late night', 'Desk', 'Movie']);
  assert.deepEqual(r.lit(), ['Desk']);
  assert.match(r.rowOf('Desk').textContent, /On now/);
  assert.deepEqual([...r.rowOf('Movie').querySelectorAll('.fp-tag')].map((t) => t.textContent),
    ['computer', 'Desk DAC', 'Hall · not connected']);
  assert.ok(r.rowOf('Movie').querySelector('.sc-off'));
  assert.equal(r.$('popup-scenes-hint').textContent,
    'Saves the computer and Desk DAC. Hall is not connected and is left out. '
    + 'No volume, gain, mute or power is saved.');
  assert.equal(r.$(draw.IDS.field).getAttribute('aria-label'), 'Scene name');
  r.window.close();
});

await ok('a_guest_and_a_read_only_page_have_no_popup_no_scenes_in_the_model_and_open_nothing', async () => {
  for (const viewer of [{ guest: true }, { writes: false }]) {
    const r = rig({ viewer });
    assert.equal(r.core.model.page().scenes, undefined, 'no scenes in the page model');
    await r.open();
    assert.equal(r.core.router.state().popup, null, 'the popup does not open');
    assert.deepEqual(r.daemon.calls, []);
    r.window.close();
  }
});

await ok('known_false_lights_no_lamp_and_says_reading', async () => {
  const r = rig({ scenes: VIEW({ on: 'Desk', known: false }) });
  await r.open();
  assert.deepEqual(r.lit(), []);
  assert.match(r.$(draw.IDS.reading).textContent, /reading…/);
  r.daemon.view.known = true;
  r.daemon.view.on = 'Desk';
  await r.event();
  assert.deepEqual(r.lit(), ['Desk']);
  assert.equal(r.$(draw.IDS.reading).textContent, '');
  r.window.close();
});

await ok('a_tap_on_the_name_recalls_and_the_outcome_says_what_was_left', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Movie', 'recall'));
  assert.deepEqual(r.posts().map((c) => [c.path, c.body]), [['api/v1/scenes/recall', { name: 'Movie' }]]);
  assert.match(r.say(), /^Movie recalled\. The computer and Desk DAC are set\./);
  assert.match(r.say(), /Hall is not connected: its part was left as it is\./);
  await r.event();
  assert.deepEqual(r.lit(), ['Movie']);
  assert.match(r.say(), /^Movie recalled\./, 'an event does not redraw the answer');
  await r.click(r.act('Desk', 'recall'));
  assert.match(r.say(), /^Desk recalled\. The computer and Desk DAC are set\.$/);
  r.window.close();
});

await ok('the_answer_then_the_event_never_flickers_the_row_back', async () => {
  const r = rig();
  await r.open();
  const seen = [];
  await r.click(r.act('Movie', 'recall'));
  seen.push(r.lit().join(','));
  assert.match(r.say(), /^Movie recalled/);
  // an older view that was on its way arrives first (the daemon sent it before the recall), then
  // the one the recall made
  await r.event(VIEW({ on: null }));
  seen.push(r.lit().join(','));
  await r.event();
  seen.push(r.lit().join(','));
  assert.deepEqual(seen, ['', '', 'Movie'], 'a lamp lit by the view only, never lit and put out');
  assert.match(r.say(), /^Movie recalled/, 'and the answer stays through both events');
  r.window.close();
});

await ok('the_event_then_the_answer_keeps_the_row_lit_and_the_answer_still_shows', async () => {
  const r = rig();
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  const tap = r.act('Movie', 'recall');
  tap.click();
  await r.calm();
  assert.match(r.say(), /Recalling/);
  // the stream's event comes first
  r.daemon.view.on = 'Movie';
  r.daemon.view.recalled = 'Movie';
  const seen = [];
  r.core.model.on('scenes', () => seen.push(r.lit().join(',')));
  await r.event();
  assert.deepEqual(r.lit(), ['Movie']);
  release();
  await r.calm();
  seen.push(r.lit().join(','));
  assert.deepEqual(seen, ['Movie', 'Movie'], 'lit before the answer and after it');
  assert.match(r.say(), /^Movie recalled\./);
  r.window.close();
});

await ok('a_second_tap_while_a_recall_is_on_its_way_sends_nothing_more', async () => {
  const r = rig();
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.open();
  r.act('Movie', 'recall').click();
  r.act('Desk', 'recall').click();
  await r.calm();
  release();
  await r.calm();
  assert.equal(r.posts().length, 1);
  r.window.close();
});

await ok('the_both_sides_question_is_asked_in_place_a_yes_recalls_as_saved_and_a_no_sends_nothing', async () => {
  const r = rig();
  r.daemon.both = true;
  await r.open();
  await r.click(r.act('Late night', 'recall'));
  const ask = r.$(draw.IDS.ask);
  assert.equal(ask.querySelector('.fi-question').textContent, QUESTION);
  assert.equal(r.posts().length, 1);
  await r.click(ask.querySelector('[data-answer="cancel"]'));
  assert.equal(ask.textContent, '');
  assert.equal(r.posts().length, 1, 'a no sends nothing more');
  assert.equal(r.say(), '', 'and says nothing');
  await r.click(r.act('Late night', 'recall'));
  await r.click(r.$(draw.IDS.ask).querySelector('[data-answer="recall"]'));
  assert.deepEqual(r.posts().map((c) => c.body), [{ name: 'Late night' }, { name: 'Late night' }, { name: 'Late night', confirm: true }]);
  assert.match(r.say(), /^Late night recalled\./);
  assert.equal(r.$(draw.IDS.ask).textContent, '');
  r.window.close();
});

await ok('save_takes_the_typed_name_and_clears_the_field_and_the_view_comes_by_the_event', async () => {
  const r = rig();
  await r.open();
  type(r, draw.IDS.field, 'Evening');
  await submit(r);
  assert.deepEqual(r.posts().map((c) => [c.path, c.body]), [['api/v1/scenes/save', { name: 'Evening' }]]);
  assert.equal(r.say(), 'Saved Evening. Hall is not connected: it is not in the scene.');
  assert.equal(r.$(draw.IDS.field).value, '');
  await r.event();
  assert.deepEqual(r.names(), ['Late night', 'Desk', 'Movie', 'Evening']);
  r.window.close();
});

await ok('an_empty_name_is_not_sent_and_a_bad_name_shows_the_daemons_words_and_keeps_the_text', async () => {
  const r = rig();
  await r.open();
  await submit(r);
  assert.equal(r.posts().length, 0, 'the field is required');
  type(r, draw.IDS.field, '   ');
  await submit(r);
  assert.equal(r.say(), 'A scene needs a name.');
  assert.ok(r.$(draw.IDS.say).querySelector('.fp-failed'), 'in the failed ink');
  assert.equal(r.$(draw.IDS.field).value, '   ', 'what was typed stays');
  r.window.close();
});

await ok('a_taken_name_asks_save_over_in_place_and_a_yes_sends_force', async () => {
  const r = rig();
  await r.open();
  type(r, draw.IDS.field, 'Desk');
  await submit(r);
  const ask = r.$(draw.IDS.over);
  assert.equal(ask.querySelector('.fi-question').textContent, 'Save over “Desk”?');
  assert.deepEqual(r.posts().map((c) => c.body), [{ name: 'Desk' }]);
  await r.click(ask.querySelector('[data-answer="keep"]'));
  assert.equal(ask.textContent, '');
  assert.equal(r.posts().length, 1, 'keeping sends nothing');
  assert.equal(r.$(draw.IDS.field).value, 'Desk');
  await submit(r);
  await r.click(r.$(draw.IDS.over).querySelector('[data-answer="overwrite"]'));
  assert.deepEqual(r.posts().map((c) => c.body), [{ name: 'Desk' }, { name: 'Desk' }, { name: 'Desk', force: true }]);
  assert.equal(r.say(), 'Saved Desk. Hall is not connected: it is not in the scene.');
  assert.equal(r.$(draw.IDS.field).value, '');
  r.window.close();
});

await ok('a_busy_refusal_shows_the_words_and_acts_on_nothing', async () => {
  const r = rig();
  r.daemon.answer = ({ path }) => (path.endsWith('/save')
    ? { status: 409, body: { error: 'A device is still reading its settings.', reason: 'busy' } }
    : null);
  await r.open();
  type(r, draw.IDS.field, 'Evening');
  await submit(r);
  assert.equal(r.say(), 'A device is still reading its settings.');
  assert.equal(r.$(draw.IDS.over).textContent, '', 'no question: only taken asks');
  assert.equal(r.$(draw.IDS.field).value, 'Evening');
  r.window.close();
});

await ok('rename_asks_in_place_with_the_name_and_the_new_name_comes_by_the_event', async () => {
  const r = rig();
  r.daemon.view.on = 'Desk';
  await r.event();
  await r.open();
  await r.click(r.act('Desk', 'rename'));
  const form = r.rowOf('Desk').querySelector('.sc-ask form');
  assert.ok(form, 'in the row');
  assert.equal(form.querySelector('input').value, 'Desk');
  assert.equal(form.querySelector('.fi-question') ?? null, null);
  form.querySelector('input').value = 'Desk, speakers';
  form.dispatchEvent(new r.window.Event('submit', { bubbles: true, cancelable: true }));
  await r.calm();
  assert.deepEqual(r.posts().map((c) => [c.path, c.body]), [['api/v1/scenes/rename', { from: 'Desk', to: 'Desk, speakers' }]]);
  assert.equal(r.say(), 'Desk is now Desk, speakers.');
  await r.event();
  assert.deepEqual(r.names(), ['Late night', 'Desk, speakers', 'Movie']);
  assert.deepEqual(r.lit(), ['Desk, speakers']);
  r.window.close();
});

await ok('a_taken_rename_shows_the_words_and_keeps_the_form_open', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Desk', 'rename'));
  const form = r.rowOf('Desk').querySelector('.sc-ask form');
  form.querySelector('input').value = 'Movie';
  form.dispatchEvent(new r.window.Event('submit', { bubbles: true, cancelable: true }));
  await r.calm();
  assert.equal(r.say(), 'A scene named Movie is there already.');
  assert.ok(r.rowOf('Desk').querySelector('.sc-ask form'), 'the form stays, with what was typed');
  assert.equal(r.rowOf('Desk').querySelector('.sc-ask input').value, 'Movie');
  r.window.close();
});

await ok('delete_asks_in_place_and_keep_sends_nothing', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Movie', 'delete'));
  assert.equal(r.rowOf('Movie').querySelector('.fi-question').textContent,
    'Delete “Movie”? The sound now playing does not change.');
  await r.click(r.rowOf('Movie').querySelector('[data-answer="keep"]'));
  assert.equal(r.posts().length, 0);
  await r.click(r.act('Movie', 'delete'));
  await r.click(r.rowOf('Movie').querySelector('[data-answer="delete"]'));
  assert.deepEqual(r.posts().map((c) => [c.path, c.body]), [['api/v1/scenes/delete', { name: 'Movie' }]]);
  assert.equal(r.say(), 'Deleted Movie.');
  await r.event();
  assert.deepEqual(r.names(), ['Late night', 'Desk']);
  r.window.close();
});

await ok('an_event_while_a_row_asks_waits_for_the_ask_and_then_draws', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Desk', 'delete'));
  r.daemon.view.on = 'Late night';
  await r.event();
  assert.ok(r.rowOf('Desk').querySelector('.fi-question'), 'the question stands through the event');
  assert.deepEqual(r.lit(), [], 'the list is drawn when the ask closes');
  r.core.router.state();
  r.key('Escape', { on: r.rowOf('Desk').querySelector('[data-answer="keep"]') });
  await r.calm();
  assert.equal(r.rowOf('Desk').querySelector('.fi-question'), null);
  assert.deepEqual(r.lit(), ['Late night'], 'and now it shows what the event said');
  assert.equal(r.core.router.state().popup, SCENES, 'Escape closed the ask, not the popup');
  r.window.close();
});

await ok('a_scene_deleted_elsewhere_answers_gone_and_the_list_is_read_once_with_no_retry', async () => {
  for (const kind of ['404', 'missing']) {
    const r = rig();
    await r.open();
    // another front end deletes Movie; this page has not heard yet
    r.daemon.view.scenes.splice(2, 1);
    if (kind === 'missing') {
      r.daemon.answer = ({ path }) => (path.endsWith('/recall')
        ? { status: 409, body: { error: 'No scene is called Movie.', reason: 'missing' } } : null);
    }
    await r.click(r.act('Movie', 'recall'));
    assert.equal(r.say(), 'That scene is gone.');
    assert.deepEqual(r.daemon.calls.map((c) => c.path), ['api/v1/scenes/recall', 'api/v1/scenes'],
      'the write once, then one read');
    assert.deepEqual(r.names(), ['Late night', 'Desk'], 'the list is what the daemon holds');
    r.window.close();
  }
  const r = rig();
  await r.open();
  await r.click(r.act('Movie', 'delete'));
  r.daemon.view.scenes.splice(2, 1);
  await r.click(r.rowOf('Movie').querySelector('[data-answer="delete"]'));
  assert.equal(r.say(), 'That scene is gone.');
  assert.equal(r.daemon.calls.filter((c) => c.path === 'api/v1/scenes/delete').length, 1);
  assert.deepEqual(r.names(), ['Late night', 'Desk']);
  r.window.close();
});

await ok('the_popup_stays_through_a_change_of_the_shown_device_and_closes_cleanly', async () => {
  const r = rig();
  await r.open();
  const before = r.core.router.state().popup;
  r.core.model.emit('fleet', { devices: [{ url: 'A', name: 'Desk DAC', link: link(true) }, { url: 'H', name: 'Hall', link: link(true) }], hops: [] });
  await r.calm();
  assert.equal(r.core.router.state().popup, before);
  assert.equal(r.$('popup-scenes-hint').textContent.startsWith('Saves the computer, Desk DAC and Hall.'), true);
  r.key('Escape');
  await r.calm();
  assert.equal(r.core.router.state().popup, null);
  // an event after the close draws nothing and throws nothing
  await r.event(VIEW({ on: 'Desk' }));
  r.window.close();
});

// ---- fix 1 ----

await ok('a_tap_takes_the_name_of_its_row_not_its_place_while_a_row_ask_freezes_the_list', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Movie', 'rename'));
  // another front end deletes Late night: the list stands (the ask is in it), the view moves
  r.daemon.view.scenes.splice(0, 1);
  await r.event();
  assert.deepEqual(r.names(), ['Late night', 'Desk', 'Movie'], 'the list is frozen');
  await r.click(r.act('Desk', 'recall'));
  assert.deepEqual(r.posts().map((c) => c.body), [{ name: 'Desk' }], 'Desk, not Movie');
  r.window.close();
});

await ok('a_row_whose_scene_the_view_has_lost_says_gone_and_sends_nothing', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Movie', 'rename'));
  r.daemon.view.scenes.splice(1, 1);
  await r.event();
  await r.click(r.act('Desk', 'delete'));
  assert.equal(r.say(), 'That scene is gone.');
  assert.equal(r.posts().length, 0);
  assert.deepEqual(r.names(), ['Late night', 'Movie'], 'the ask closed and the list is the view');
  r.window.close();
});

await ok('a_second_row_ask_after_the_view_changed_shows_and_the_list_follows_the_next_event', async () => {
  const r = rig();
  await r.open();
  await r.click(r.act('Desk', 'rename'));
  r.daemon.view.on = 'Late night';
  await r.event();
  await r.click(r.act('Movie', 'delete'));
  const asks = [...r.document.querySelectorAll('#popup-scenes-list .fi-question')];
  assert.equal(asks.length, 1, 'one question stands');
  assert.ok(r.rowOf('Movie').contains(asks[0]), 'in the row of Movie');
  assert.equal(r.rowOf('Desk').querySelector('form'), null, 'the first ask went');
  assert.deepEqual(r.lit(), ['Late night'], 'the list was drawn when the first closed');
  await r.click(r.rowOf('Movie').querySelector('[data-answer="keep"]'));
  r.daemon.view.on = 'Desk';
  await r.event();
  assert.deepEqual(r.lit(), ['Desk'], 'and follows the next event');
  r.window.close();
});

await ok('after_the_both_sides_question_either_answer_gives_the_focus_back', async () => {
  for (const answer of ['cancel', 'recall']) {
    const r = rig();
    r.daemon.both = true;
    await r.open();
    const main = r.act('Late night', 'recall');
    main.focus();
    await r.click(main);
    await r.click(r.$(draw.IDS.ask).querySelector(`[data-answer="${answer}"]`));
    assert.equal(r.document.activeElement?.id, main.id, `after ${answer}`);
    r.window.close();
  }
});

await ok('a_writer_page_with_no_scenes_draws_one_line_and_no_form_until_the_view_comes', async () => {
  const r = rig({ scenes: null });
  assert.equal(r.core.model.page().scenes, undefined);
  await r.open();
  assert.equal(r.core.router.state().popup, SCENES);
  assert.equal(r.$(draw.IDS.field), null, 'no save field');
  assert.match(r.$(draw.IDS.page).textContent, /Scenes are not available here\./);
  await r.event(VIEW());
  assert.ok(r.$(draw.IDS.field), 'the form comes with the view');
  assert.deepEqual(r.names(), ['Late night', 'Desk', 'Movie']);
  r.window.close();
});

await ok('the_no_volume_sentence_is_on_the_screen_once_and_the_placeholder_is_a_plain_name', async () => {
  const r = rig();
  await r.open();
  const words = r.$(draw.IDS.page).textContent;
  assert.equal(words.split('No volume, gain, mute or power is saved.').length - 1, 1);
  assert.equal(r.$(draw.IDS.field).placeholder, 'Name, for example Late night');
  assert.ok(!/night shift/i.test(r.document.body.innerHTML));
  r.window.close();
});

await ok('a_row_has_the_words_a_theme_may_draw_instead_of_icons', async () => {
  const html = draw.listHtml(scenes.viewOf(VIEW()));
  for (const word of ['Recall', 'Rename…', 'Delete…']) assert.ok(html.includes(`>${word}<`), word);
});

await ok('a_refusal_headline_ends_with_a_full_stop', async () => {
  const r = rig();
  r.daemon.answer = ({ path }) => (path.endsWith('/save')
    ? { status: 409, body: { error: 'A device is still reading', reason: 'busy' } } : null);
  await r.open();
  type(r, draw.IDS.field, 'Evening');
  await submit(r);
  assert.equal(r.say(), 'A device is still reading.');
  r.window.close();
});

// ---- phase B: the daemon's contract as 2.1 built it ----

await ok('a_save_names_the_device_it_left_out_in_the_daemons_words', async () => {
  const r = rig();
  await r.open();
  type(r, draw.IDS.field, 'Evening');
  await submit(r);
  assert.equal(r.say(), 'Saved Evening. Hall is not connected: it is not in the scene.');
  assert.deepEqual(draw.savedSaid('X', undefined).lines, []);
  r.window.close();
});

await ok('nothing_to_save_internal_and_stopping_show_their_words_and_the_form_stays', async () => {
  for (const [reason, error] of [['nothing_to_save', 'Nothing is connected to save'],
    ['internal', 'The scenes could not be saved'], ['stopping', 'The daemon is stopping']]) {
    const r = rig();
    r.daemon.answer = ({ path }) => (path.endsWith('/save')
      ? { status: 409, body: { error, reason } } : null);
    await r.open();
    type(r, draw.IDS.field, 'Evening');
    await submit(r);
    assert.equal(r.say(), `${error}.`, reason);
    assert.equal(r.$(draw.IDS.over).textContent, '', 'only taken asks');
    assert.equal(r.$(draw.IDS.field).value, 'Evening');
    r.window.close();
  }
});

await ok('a_read_while_the_daemon_stops_is_409_stopping_and_changes_nothing', async () => {
  const r = fakeActions(async () => ({ status: 200 }));
  const store = r.store;
  const before = store.page().scenes;
  const asked = [];
  const again = createScenesActions({
    store, post: async () => ({ status: 200 }), writes: createWrites({ store, post: async () => ({}), toast() {} }),
    fetch: async (path) => { asked.push(path); return { status: 409, json: async () => ({ reason: 'stopping', error: 'The daemon is stopping' }) }; },
    signedOut() {},
  });
  const answer = await again.read();
  assert.equal(answer.status, 409);
  assert.equal(answer.reason, 'stopping');
  assert.equal(store.page().scenes, before, 'the view stays as it was');
  assert.deepEqual(asked, ['api/v1/scenes']);
});

await ok('a_computer_part_that_failed_is_said_with_its_notes_and_makes_the_recall_partial', async () => {
  const how = scenes.outcomeOf('Movie', [
    { who: 'computer', status: 'failed', notes: ['The computer was changed and could not be put back'] },
    { who: 'Desk DAC', status: 'done', notes: [] },
  ]);
  assert.equal(how.headline, 'Movie recalled in part.');
  assert.equal(how.set, 'Desk DAC is set.');
  assert.deepEqual(how.problems, ['The computer was changed and could not be put back.']);
  assert.deepEqual(scenes.outcomeOf('Movie', [{ who: 'computer', status: 'failed', notes: [] }]).problems,
    ['The computer could not be set.']);
});

await ok('a_row_has_one_recall_button_for_a_reader_the_word_button_is_for_the_eye', async () => {
  const r = rig();
  await r.open();
  for (const row of r.rows()) {
    const named = [...row.querySelectorAll('button')].filter((b) => b.getAttribute('aria-hidden') !== 'true'
      && b.getAttribute('aria-label') === `Recall ${row.dataset.name}`);
    assert.equal(named.length, 1, row.dataset.name);
    const word = row.querySelector('.sc-recall');
    assert.ok(word.id && word.getAttribute('tabindex') === '-1', 'an id for the focus, out of the tab order');
  }
  r.window.close();
});


await ok('the_daemons_words_start_with_a_capital_whatever_they_came_in', async () => {
  assert.equal(scenes.said('nothing to save: no device is connected'), 'Nothing to save: no device is connected.');
  assert.equal(scenes.said('Already a sentence.'), 'Already a sentence.');
  const how = scenes.outcomeOf('Movie', [
    { who: 'Desk DAC', status: 'partial', notes: ['input: 99 is above the most the device takes', 'phase left as it was'] },
  ]);
  assert.deepEqual(how.problems, ['Input: 99 is above the most the device takes. Phase left as it was.']);
  assert.deepEqual(draw.savedSaid('X', [{ who: 'Hall', status: 'left', notes: ['hall is not connected: it is not in the scene'] }]).lines,
    ['Hall is not connected: it is not in the scene.']);
  const r = rig();
  r.daemon.answer = ({ path }) => (path.endsWith('/save')
    ? { status: 409, body: { error: 'nothing to save: no device is connected', reason: 'nothing_to_save' } } : null);
  await r.open();
  type(r, draw.IDS.field, 'Evening');
  await submit(r);
  assert.equal(r.say(), 'Nothing to save: no device is connected.');
  r.window.close();
});

console.log(`core-scenes: ${n} cases passed`);
