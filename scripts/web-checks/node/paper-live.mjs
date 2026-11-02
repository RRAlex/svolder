/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's sheet and what keeps it up to date (themes/paper/draw/sheet.js,
// live.js, act.js, place.js): the title, the card and the footer drawn from every fixture page
// with no `style` attribute and every word escaped; which sections a change of the store draws
// again; a held section waiting for the newest model; the stream's state fading and stilling the
// sheet; readings heard until the mount ends; the log of what this page asked; the marks put in
// place through the style object. A small fake page stands in for the browser. Run: node paper-live.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import { MINE_MS, createAct } from '../../../bins/svolderd/web/themes/paper/act.js';
import { SECTIONS, esc, sheet } from '../../../bins/svolderd/web/themes/paper/draw/sheet.js';
import { PARTS_OF, createLive, keeper } from '../../../bins/svolderd/web/themes/paper/live.js';
import { place } from '../../../bins/svolderd/web/themes/paper/place.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const KEYS = [{ key: 'T', words: 'Themes' }];
const view = (extra = {}) => ({
  phone: false, coarse: false, state: 'open', keys: KEYS, now: Date.UTC(2026, 9, 30, 12), ...extra,
});

/** The sheet's parts as a fake page holds them: an element for each `#p-<name>`. */
function fakeRoot() {
  const slots = [];
  const boxes = {};
  const marks = [];
  const sheetEl = {
    named: new Set(),
    inert: false,
    classList: {
      toggle(name, on) {
        if (on) sheetEl.named.add(name);
        else sheetEl.named.delete(name);
      },
    },
  };
  return {
    boxes,
    sheetEl,
    slots,
    marks,
    querySelector(selector) {
      if (selector === '#paper') return sheetEl;
      const found = /^#p-(.+)$/.exec(selector);
      // The marks' layer is for the browser checks (the notes' places need real boxes).
      if (!found || found[1] === 'marks') return null;
      boxes[found[1]] ??= {
        innerHTML: '',
        querySelector: (selector) => {
          if (selector !== '.paper-slot') return null;
          const slot = { slot: slots.length };
          slots.push(slot);
          return slot;
        },
      };
      return boxes[found[1]];
    },
    querySelectorAll: () => marks,
  };
}

/** A live sheet over the fixture page `name`: the store, the fake frame and its calls. */
function rig(name = 'dac') {
  const store = createStore(pageOf(name), { fetch: async () => ({ status: 200, json: async () => ({}) }) });
  const calls = { realign: 0, anchored: [] };
  const core = {
    model: store,
    frame: { realign: () => { calls.realign++; }, anchor: (el) => calls.anchored.push(el) },
  };
  const root = fakeRoot();
  const controller = new AbortController();
  const state = view();
  const live = createLive({ core, root, signal: controller.signal, view: state });
  return { store, core, root, live, calls, controller, state };
}

/** Make `SECTIONS[name]` count its draws and say which model it was drawn from. */
function spy(names) {
  const drawn = Object.fromEntries(names.map((name) => [name, []]));
  const real = Object.fromEntries(names.map((name) => [name, SECTIONS[name]]));
  for (const name of names) {
    SECTIONS[name] = (page, device) => {
      drawn[name].push(device?.tone ?? null);
      return `<p>${name}</p>`;
    };
  }
  const done = () => names.forEach((name) => {
    if (real[name]) SECTIONS[name] = real[name];
    else delete SECTIONS[name];
  });
  return { drawn, done };
}

await ok('the_skeleton_holds_every_section_in_its_place', () => {
  const html = sheet(pageOf('dac'));
  const ids = [...html.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(ids, ['paper', 'p-title', 'p-card', 'p-volume', 'p-left', 'p-tone', 'p-power',
    'p-meters', 'p-right', 'p-controls', 'p-playing', 'p-presets', 'p-device-presets', 'p-notes',
    'p-device', 'p-foot', 'p-marks']);
  assert.ok(html.startsWith('<div id="paper" class="paper"><header id="p-title">'));
  assert.ok(html.includes('<footer id="p-foot"></footer>'));
  assert.deepEqual(Object.keys(SECTIONS), ['title', 'card', 'volume', 'tone', 'power', 'meters',
    'controls', 'playing', 'presets', 'device-presets', 'device', 'notes', 'foot']);
  // Every name the live module draws has an element in the skeleton.
  for (const name of Object.keys(PARTS_OF)) assert.ok(html.includes(` id="p-${name}"`), name);
});

await ok('the_title_names_the_device_its_line_and_the_slot_for_the_button', () => {
  const page = pageOf('dac');
  const html = SECTIONS.title(page, page.device, view());
  assert.match(html, /^<h1>Gungnir 2<\/h1><small class="p-sub">DAC, connected over USB 1 at 48 kHz<\/small>/);
  assert.ok(html.endsWith('<span class="paper-slot"></span>'), html);
  assert.ok(!html.includes('p-also'), 'one device: nobody else is here');
  const fleet = pageOf('two-devices');
  const two = SECTIONS.title(fleet, fleet.device, view());
  assert.match(two, /<p class="p-also">also here: <button type="button" class="p-w" data-go="device" data-device="Aegir%202">Aegir 2<\/button><\/p>/);
});

await ok('the_state_rewrites_the_titles_line', () => {
  const page = pageOf('dac');
  const lost = SECTIONS.title(page, page.device, view({ state: 'lost' }));
  assert.ok(lost.includes('<small class="p-sub lost">Reconnecting to svolder…</small>'), lost);
  const waiting = SECTIONS.title(page, page.device, view({ state: 'waiting' }));
  assert.ok(waiting.includes('>reading the device again…<'), waiting);
  const resting = SECTIONS.title(page, page.device, view({ state: 'resting' }));
  assert.ok(resting.includes('DAC, connected'), resting);
});

await ok('the_footer_says_version_date_keys_and_the_way_in_or_out', () => {
  const page = pageOf('dac-live-signal-token');
  const out = SECTIONS.foot(page, page.device, view());
  assert.ok(out.includes('svolder 0.30.1 &middot; 30 October'), out);
  assert.ok(out.includes('<span class="p-keys">T for themes</span>'), out);
  assert.ok(out.includes('<button type="button" class="p-w" data-go="sign-out">sign out</button> &middot; Paper'), out);
  const plain = pageOf('dac');
  assert.ok(SECTIONS.foot(plain, plain.device, view()).endsWith('<span class="p-right">Paper</span>'));
  const guest = pageOf('dac-guest');
  const visitor = SECTIONS.foot(guest, guest.device, view());
  assert.ok(visitor.includes('<a class="p-w" href="./?sign-in">sign in</a> &middot; Paper'), visitor);
  assert.ok(visitor.startsWith('<span class="p-left">svolder &middot; 30 October</span>'), visitor);
});

await ok('the_card_is_the_red_note_under_the_title', () => {
  const lost = pageOf('lost');
  const html = SECTIONS.card(lost, lost.device, view());
  assert.ok(html.includes('<p class="pn red card">Gungnir 2 is out of reach</p>'), html);
  assert.ok(html.includes('Not connected. Next attempt in 8 s.'), html);
  const none = pageOf('no-devices');
  assert.ok(SECTIONS.card(none, null, view()).includes('No devices'));
  assert.equal(SECTIONS.card(pageOf('dac'), pageOf('dac').device, view()), '');
});

await ok('no_markup_carries_a_style_attribute_and_every_word_is_escaped', () => {
  for (const { name, page } of fixture.cases) {
    for (const [section, draw] of Object.entries(SECTIONS)) {
      const html = draw(page, page.device, view());
      assert.ok(!/\sstyle=/i.test(html), `${name} ${section}`);
      assert.ok(!/<script|<style|\son\w+=/i.test(html), `${name} ${section}`);
    }
  }
  const page = pageOf('dac');
  page.device.name = '<b>"x"</b> & \'y\'';
  const html = SECTIONS.title(page, page.device, view());
  assert.ok(html.startsWith('<h1>&lt;b&gt;&quot;x&quot;&lt;/b&gt; &amp; &#39;y&#39;</h1>'), html);
  assert.equal(esc(null), '');
});

await ok('the_sheet_is_drawn_from_the_model_and_the_title_anchors_the_button', () => {
  const { live, root, calls } = rig();
  live.drawAll();
  assert.ok(root.boxes.title.innerHTML.startsWith('<h1>Gungnir 2</h1>'));
  assert.ok(root.boxes.foot.innerHTML.includes('Paper'));
  assert.ok(root.boxes.volume.innerHTML.includes('id="p-ruler"'), 'the volume is drawn');
  assert.ok(root.boxes.tone.innerHTML.includes('id="p-graph"'), 'the tone is drawn');
  assert.equal(root.boxes.power.innerHTML, '', 'a section nobody wrote draws nothing');
  assert.equal(calls.anchored.length, 1);
  assert.ok(calls.realign >= 1);
});

await ok('a_part_draws_only_the_sections_that_show_it', () => {
  const { drawn, done } = spy(['volume', 'tone', 'notes', 'controls']);
  try {
    const { live, store } = rig();
    live.drawAll();
    for (const list of Object.values(drawn)) list.length = 0;
    store.applyPart(store.shown(), 'volume', { ...store.part('volume'), raw: 40 });
    assert.deepEqual(Object.fromEntries(Object.entries(drawn).map(([k, v]) => [k, v.length])),
      { volume: 1, tone: 0, notes: 1, controls: 0 });
    store.applyPart(store.shown(), 'presets', null);
    assert.equal(drawn.tone.length, 1, 'the presets are shown by the tone section');
    assert.equal(drawn.notes.length, 2, 'the notes show every part');
    assert.equal(drawn.volume.length, 1);
  } finally {
    done();
  }
});

await ok('a_part_of_another_device_changes_nothing', () => {
  const { drawn, done } = spy(['volume']);
  try {
    const { live, store } = rig('two-devices');
    live.drawAll();
    drawn.volume.length = 0;
    store.applyPart('Aegir%202', 'volume', { raw: 3 });
    assert.equal(drawn.volume.length, 0);
  } finally {
    done();
  }
});

await ok('a_model_or_the_shown_device_replaced_draws_every_section', () => {
  const { drawn, done } = spy(['volume', 'tone', 'meters', 'notes']);
  try {
    const { live, store } = rig();
    live.drawAll();
    for (const list of Object.values(drawn)) list.length = 0;
    store.replace(pageOf('dac'));
    assert.deepEqual(Object.values(drawn).map((v) => v.length), [1, 1, 1, 1]);
    store.setDevice(store.shown(), pageOf('dac').device);
    assert.deepEqual(Object.values(drawn).map((v) => v.length), [2, 2, 2, 2]);
    store.setDevice('someone-else', pageOf('dac').device);
    assert.deepEqual(Object.values(drawn).map((v) => v.length), [2, 2, 2, 2]);
  } finally {
    done();
  }
});

await ok('a_held_section_waits_and_is_drawn_from_the_newest_model_when_released', () => {
  const { drawn, done } = spy(['tone']);
  try {
    const { live, store } = rig();
    live.drawAll();
    drawn.tone.length = 0;
    live.hold('tone', true);
    assert.equal(live.held('tone'), true);
    store.applyPart(store.shown(), 'tone', { mode: 'first' });
    store.applyPart(store.shown(), 'tone', { mode: 'second' });
    store.replace({ ...store.page(), device: { ...store.device(), tone: { mode: 'third' } } });
    assert.equal(drawn.tone.length, 0, 'nothing is drawn under the hand');
    live.hold('tone', false);
    assert.equal(live.held('tone'), false);
    assert.deepEqual(drawn.tone, [{ mode: 'third' }], 'once, from the model as it is now');
    live.hold('tone', true);
    live.hold('tone', false);
    assert.equal(drawn.tone.length, 1, 'a hold with nothing missed draws nothing');
  } finally {
    done();
  }
});

const pause = (ms) => new Promise((resolve) => { setTimeout(resolve, ms); });

await ok('a_slow_answer_does_not_release_a_hold_the_next_gesture_took', async () => {
  const { live } = rig();
  const keep = keeper(live, 'controls');
  let answer;
  let pending = new Promise((resolve) => { answer = resolve; });
  const hand = keep.hand({ pending: () => pending, ms: 5 });
  hand.grip();
  hand.letGo();
  await pause(25);
  assert.equal(live.held('controls'), true, 'the write is unanswered: still held');
  // a new drag starts while the first release waits for its answer
  hand.grip();
  answer();
  await pause(25);
  assert.equal(live.held('controls'), true, 'the first answer must not release the new hand');
  pending = Promise.resolve();
  hand.letGo();
  await pause(25);
  assert.equal(live.held('controls'), false, 'the last hand let go: released once');
  // a release waiting when the hold was reset from outside does nothing
  hand.grip();
  hand.letGo();
  keep.reset();
  hand.forget();
  await pause(25);
  assert.equal(live.held('controls'), false);
  keep.take();
  await pause(25);
  assert.equal(live.held('controls'), true, 'a later hold is not undone by the forgotten release');
  keep.drop();
});

await ok('a_hold_taken_around_a_call_is_given_back_if_it_throws', () => {
  const { live } = rig();
  const keep = keeper(live, 'tone');
  assert.equal(keep.around(() => 7), 7);
  assert.equal(live.held('tone'), true);
  keep.drop();
  assert.throws(() => keep.around(() => { throw new Error('no list'); }), /no list/);
  assert.equal(live.held('tone'), false);
});

await ok('another_device_shown_tells_the_switchers_once_and_lets_a_reset_release_the_holds', () => {
  const { live, store } = rig();
  const keep = keeper(live, 'presets');
  let told = 0;
  live.onSwitch(() => {
    told++;
    keep.reset();
  });
  keep.take();
  keep.take();
  store.replace(pageOf('dac'));
  assert.equal(told, 0, 'the same device replaced is not a switch');
  assert.equal(live.held('presets'), true);
  store.replace({ ...pageOf('dac'), shown: 'another-one' });
  assert.equal(told, 1);
  assert.equal(live.held('presets'), false, 'the old device\'s holds are gone');
  store.replace({ ...pageOf('dac'), shown: 'another-one' });
  assert.equal(told, 1, 'and it is told only once');
});

await ok('the_state_fades_or_stills_the_sheet_and_rewrites_the_title', () => {
  const { live, store, root, state } = rig();
  live.drawAll();
  store.setState('lost');
  assert.ok(root.sheetEl.named.has('faded') && !root.sheetEl.named.has('waiting'));
  assert.equal(root.sheetEl.inert, true);
  assert.equal(state.state, 'lost');
  assert.ok(root.boxes.title.innerHTML.includes('Reconnecting to svolder'));
  store.setState('waiting');
  assert.ok(root.sheetEl.named.has('waiting') && !root.sheetEl.named.has('faded'));
  assert.equal(root.sheetEl.inert, true);
  assert.ok(root.boxes.title.innerHTML.includes('reading the device again'));
  store.setState('resting');
  assert.equal(root.sheetEl.named.size, 0);
  assert.equal(root.sheetEl.inert, false);
  assert.ok(root.boxes.title.innerHTML.includes('DAC, connected'));
  store.setState('open');
  assert.equal(root.sheetEl.inert, false);
});

await ok('a_state_change_during_a_hold_neither_stills_the_sheet_nor_leaves_the_section_stale', () => {
  const { live, store, root } = rig();
  live.drawAll();
  const before = root.boxes.volume.innerHTML;
  live.hold('volume', true);
  store.setState('waiting');
  store.emit('volume', { raw: 99 });
  assert.equal(root.sheetEl.inert, false, 'a gesture in hand is not cut short by the sheet turning inert');
  assert.ok(root.sheetEl.named.has('waiting'), 'the sheet still shows it waits');
  assert.equal(root.boxes.volume.innerHTML, before, 'nothing is drawn under the hand');
  store.setState('resting');
  live.hold('volume', false);
  assert.equal(root.sheetEl.inert, false);
  assert.ok(root.boxes.volume.innerHTML.includes('aria-valuenow="99"'), 'the newest reading is shown');
  // The sheet that turned inert during the hold is inert as soon as the hold ends.
  live.hold('volume', true);
  store.setState('lost');
  assert.equal(root.sheetEl.inert, false);
  live.hold('volume', false);
  assert.equal(root.sheetEl.inert, true, 'the sheet is out of reach once the hand lets go');
});

await ok('a_reading_of_the_volume_is_drawn_and_one_that_fits_the_ruler_is_put_in_place', () => {
  const { live, store, root } = rig();
  live.drawAll();
  const set = [];
  const needle = { dataset: {}, setAttribute: (k, v) => set.push([k, v]), removeAttribute() {} };
  const element = () => ({ dataset: {}, classList: { toggle() {} }, querySelector: () => element() });
  const text = element();
  const box = root.boxes.volume;
  const html = box.innerHTML;
  box.querySelector = (selector) => ({
    '#p-ruler': { dataset: { sig: /data-sig="([^"]*)"/.exec(html)[1].replace(/&quot;/g, '"') } },
    '#p-needle': needle, '#p-volume-text': text,
  })[selector] ?? element();
  box.querySelectorAll = () => [];
  const errors = [];
  const log = console.error;
  console.error = (...a) => errors.push(a);
  store.emit('volume', { raw: 70 });
  console.error = log;
  assert.deepEqual(errors, [], 'no listener threw');
  assert.equal(box.innerHTML, html, 'the ruler was not drawn again');
  assert.equal(needle.dataset.at, (70 / 127).toFixed(5));
  assert.deepEqual(set.find(([k]) => k === 'aria-valuenow'), ['aria-valuenow', '70']);
});

await ok('the_host_and_the_fleet_draw_now_playing_and_the_title', () => {
  const { drawn, done } = spy(['playing']);
  try {
    const { live, store, root } = rig();
    live.drawAll();
    drawn.playing.length = 0;
    const before = root.slots.length;
    store.emit('host', { track: null });
    store.emit('fleet', { devices: [], hops: [] });
    assert.equal(drawn.playing.length, 2);
    assert.equal(root.slots.length, before + 2, 'the title is drawn each time and anchors again');
  } finally {
    done();
  }
});

await ok('readings_are_heard_until_the_signal_aborts', () => {
  const { live, store, controller } = rig();
  const heard = [];
  live.onReading('volume', (data) => heard.push(data.raw));
  const off = live.onReading('meters', () => heard.push('m'));
  store.emit('volume', { raw: 12 });
  off();
  store.emit('meters', { x: 1 });
  assert.deepEqual(heard, [12]);
  controller.abort();
  store.emit('volume', { raw: 13 });
  assert.deepEqual(heard, [12], 'nothing is heard after the mount ends');
});

await ok('a_stopped_sheet_listens_to_nothing', () => {
  const { drawn, done } = spy(['volume']);
  try {
    const { live, store } = rig();
    live.drawAll();
    live.stop();
    drawn.volume.length = 0;
    store.applyPart(store.shown(), 'volume', { raw: 1 });
    store.replace(pageOf('dac'));
    assert.equal(drawn.volume.length, 0);
  } finally {
    done();
  }
});

await ok('a_change_this_page_asked_for_is_logged_with_its_answer', async () => {
  let t = 1000;
  const core = { actions: { setVolume: async (raw) => ({ ok: raw < 50, status: 200 }) } };
  const { act, log } = createAct(core, { now: () => t });
  assert.equal(MINE_MS, 3000);
  const asked = act('volume', 40, (actions) => actions.setVolume(40));
  t = 1250;
  assert.deepEqual(log(), [{ field: 'volume', value: 40, at: 1000, answeredAt: null, ok: null }]);
  assert.equal((await asked).ok, true);
  assert.deepEqual(log(), [{ field: 'volume', value: 40, at: 1000, answeredAt: 1250, ok: true }]);
  await act('volume', 60, (actions) => actions.setVolume(60));
  assert.equal(log()[1].ok, false, 'a refused write is logged as refused');
  await assert.rejects(act('mute', true, () => { throw new Error('no'); }), /no/);
  assert.equal(log()[2].ok, false);
  assert.notEqual(log()[0], log()[0], 'the log hands out copies');
});

await ok('the_log_keeps_the_last_fifty', async () => {
  const { act, log } = createAct({ actions: {} }, { now: () => 0 });
  for (let i = 0; i < 60; i++) await act('x', i, async () => ({ ok: true }));
  assert.equal(log().length, 50);
  assert.equal(log()[0].value, 10);
  assert.equal(log()[49].value, 59);
});

await ok('place_sets_marks_through_the_style_object', () => {
  const el = (dataset, classes = []) => ({ dataset, style: {}, classList: { contains: (c) => classes.includes(c) } });
  const mark = el({ at: '0.25' });
  const share = el({ at: '0.4' }, ['p-zone', 'classa']);
  const point = el({ x: '0.5', y: '0.1' });
  const lone = el({ x: '0.5' });
  const bad = el({ at: 'nope' });
  const empty = el({ at: '' });
  const root = {
    querySelectorAll: (selector) => ({
      '[data-at]': [mark, bad, empty, share],
      '[data-x][data-y]': [point],
    })[selector] ?? [],
  };
  place(root);
  assert.equal(mark.style.left, '25.00%');
  assert.deepEqual(share.style, { width: '40.00%' }, 'the class-A share is a width from the start');
  assert.deepEqual(point.style, { left: '50.00%', top: '10.00%' });
  assert.deepEqual(lone.style, {}, 'an x without a y is left alone');
  assert.deepEqual(bad.style, {});
  assert.deepEqual(empty.style, {});
});

console.log(`paper-live: ${n} passed`);
