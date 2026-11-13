/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Help popup (core/popups/help/draw.js, wire.js). First the draws, as
// strings: a key's row (its keycaps, a gesture's name as plain text, hostile words escaped), the
// groups of Here (the widget that has the focus first, two zones on one element as one group,
// an `own` entry of another widget, then the whole page with the frame's keys), the line that
// says the focus is nowhere, Everywhere, and About (the version, the words a guest reads in its
// place, the daemon, links that open in a new tab and are the ones the repository ships).
// Then that Help cannot drift, on the four themes' real tables, for a writer and a guest and
// with the focus in each zone in turn: every row Help draws is an entry of the theme's table or
// of the frame's list as `listed` gives them (the tables `keys.py` proves bound in a browser),
// and Help draws every listed entry of each zone it names; every zone has a name a person reads.
// Last the wire and the `?` key, on the real core over a jsdom page (dom.mjs): the popup is
// listed with its row, it opens on Here, Here is read at each show and never before, a guest
// opens it, it takes no stream want and adds no key listener, `?` opens it through the router,
// types in a field, does nothing while Help is open and closes the menu first.
// Run: node core-help.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { FRAME_KEYS, listed } from '../../../bins/svolderd/web/core/keys.js';
import { HELP, POPUPS, rowId } from '../../../bins/svolderd/web/core/popups/listed.js';
import {
  LINKS, WORDS, aboutHtml, everywhereGroups, groupsHtml, hereGroups, pageHtml,
} from '../../../bins/svolderd/web/core/popups/help/draw.js';
import * as ember from '../../../bins/svolderd/web/themes/ember/key-table.js';
import * as nightdrive from '../../../bins/svolderd/web/themes/nightdrive/key-table.js';
import * as paper from '../../../bins/svolderd/web/themes/paper/key-table.js';
import * as signal from '../../../bins/svolderd/web/themes/signal/key-table.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const HOSTILE = '<img src=x onerror=alert(1)> "Q" & \'co\'';
const SAFE = '&lt;img src=x onerror=alert(1)&gt; &quot;Q&quot; &amp; &#39;co&#39;';

/** A small table: two keys of the page, a dial with a scale on it (two zones, one element in the
 * cases below), a list, and a widget whose Escape a listener of the theme's own answers. */
const TABLE = [
  { id: 'mute', key: 'm', zone: 'page', words: 'Mute', run() {} },
  { id: 'look', key: 'v', zone: 'page', words: 'Change the look', run() {} },
  {
    id: 'dial-step', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.dial',
    words: 'Volume one step',
  },
  { id: 'dial-home', key: 'Home', zone: '.dial[data-low]', words: 'Volume to its lowest' },
  { id: 'list-pick', key: 'Enter', zone: '.list', words: 'Choose the option' },
  { id: 'drag', key: 'Curve', gesture: true, zone: '.curve', words: 'Drag a point' },
  {
    id: 'drag-end', key: 'Esc', keys: ['Escape'], zone: '.curve', words: 'Let go of the point',
    own: true,
  },
];
const NAMES = {
  '.dial': 'The dial', '.dial[data-low]': 'The dial', '.list': 'A list', '.curve': 'The curve',
};
const FRAME = listed(FRAME_KEYS, { canWrite: true, help: true });
const ids = (groups) => groups
  .map((group) => [group.heading, group.entries.map((entry) => entry.id)]);

// ---- the draws ----

await ok('a_row_is_the_keycaps_and_the_words_and_carries_the_entrys_id', () => {
  const html = groupsHtml([
    { zones: ['.dial'], heading: 'The dial', entries: [TABLE[2], TABLE[3]] },
  ]);
  assert.equal(html, '<h3 class="fp-heading">The dial</h3><dl class="fp-rows">'
    + '<div class="fp-row" data-key="dial-step" data-zone=".dial"><dt class="fp-row-label">'
    + '<kbd class="fp-key">↑</kbd> <kbd class="fp-key">↓</kbd></dt>'
    + '<dd class="fp-row-value">Volume one step</dd></div>'
    + '<div class="fp-row" data-key="dial-home" data-zone=".dial[data-low]">'
    + '<dt class="fp-row-label"><kbd class="fp-key">Home</kbd></dt>'
    + '<dd class="fp-row-value">Volume to its lowest</dd></div></dl>');
});

await ok('a_gestures_name_is_plain_text_and_hostile_words_are_escaped', () => {
  const gesture = groupsHtml([{ zones: ['.curve'], heading: 'The curve', entries: [TABLE[5]] }]);
  assert.ok(gesture.includes('<dt class="fp-row-label">Curve</dt>'), gesture);
  assert.ok(!gesture.includes('<kbd'), 'a gesture is no keycap');
  const hostile = groupsHtml([{
    zones: [HOSTILE], heading: HOSTILE,
    entries: [{ id: HOSTILE, key: HOSTILE, zone: HOSTILE, words: HOSTILE }],
  }]);
  assert.ok(!hostile.includes('<img'), hostile);
  assert.ok(hostile.includes(`<h3 class="fp-heading">${SAFE}</h3>`));
  assert.ok(hostile.includes(`data-zone="${SAFE}"`) && hostile.includes(`data-key="${SAFE}"`));
  assert.ok(hostile.includes(`<kbd class="fp-key">${SAFE}</kbd>`)
    && hostile.includes(`<dd class="fp-row-value">${SAFE}</dd>`));
  // a draw names no colour and no class but the frame's
  assert.deepEqual([...new Set([...hostile.matchAll(/class="([^"]+)"/g)].map((m) => m[1]))].sort(),
    ['fp-heading', 'fp-key', 'fp-row', 'fp-row-label', 'fp-row-value', 'fp-rows']);
});

await ok('here_is_the_widget_with_the_focus_then_the_own_keys_of_another_then_the_page', () => {
  const here = hereGroups({
    table: TABLE, frame: FRAME, focus: [['.list'], ['.dial']], names: NAMES,
  });
  assert.equal(here.focused, true);
  assert.deepEqual(ids(here.groups), [
    ['A list', ['list-pick']],
    ['The dial', ['dial-step']],
    ['The curve', ['drag-end']],
    [WORDS.page, ['mute', 'look', 'themes', 'play', HELP]],
  ]);
  // the entries are the table's own objects: Help holds no list of its own
  assert.equal(here.groups[1].entries[0], TABLE[2]);
  assert.equal(here.groups[3].entries[2], FRAME[0]);
});

await ok('two_zones_on_one_element_are_one_group_with_one_heading', () => {
  const here = hereGroups({
    table: TABLE, frame: FRAME, focus: [['.dial[data-low]', '.dial']], names: NAMES,
  });
  assert.deepEqual(ids(here.groups).slice(0, 1), [['The dial', ['dial-step', 'dial-home']]]);
  assert.deepEqual(here.groups[0].zones, ['.dial', '.dial[data-low]'], 'in table order');
});

await ok('a_focused_widget_shows_all_its_keys_and_is_not_listed_twice_for_its_own_one', () => {
  const here = hereGroups({ table: TABLE, frame: FRAME, focus: [['.curve']], names: NAMES });
  assert.deepEqual(ids(here.groups), [
    ['The curve', ['drag', 'drag-end']],
    [WORDS.page, ['mute', 'look', 'themes', 'play', HELP]],
  ]);
});

await ok('with_the_focus_nowhere_here_is_the_page_and_the_own_keys_and_says_so', () => {
  const here = hereGroups({ table: TABLE, frame: FRAME, focus: [], names: NAMES });
  assert.equal(here.focused, false);
  assert.deepEqual(ids(here.groups), [
    ['The curve', ['drag-end']],
    [WORDS.page, ['mute', 'look', 'themes', 'play', HELP]],
  ]);
  const html = pageHtml({ page: 'here', table: TABLE, frame: FRAME, focus: [], names: NAMES });
  assert.ok(html.startsWith(`<p class="fp-note">${WORDS.nowhere}</p>`), html);
  const focused = pageHtml({
    page: 'here', table: TABLE, frame: FRAME, focus: [['.list']], names: NAMES,
  });
  assert.ok(!focused.includes(WORDS.nowhere));
  assert.ok(focused.startsWith('<h3 class="fp-heading">A list</h3>'));
  // a zone the focus is in whose keys do not apply now is no group: the focus counts as nowhere
  const none = hereGroups({
    table: TABLE.slice(0, 2), frame: FRAME, focus: [['.list']], names: NAMES,
  });
  assert.equal(none.focused, false);
});

await ok('a_zone_with_no_name_still_has_a_heading_a_person_reads', () => {
  const here = hereGroups({ table: TABLE, frame: FRAME, focus: [['.list']], names: {} });
  assert.equal(here.groups[0].heading, WORDS.widget);
  assert.doesNotMatch(WORDS.widget, /[.#[\]]/);
});

await ok('everywhere_is_the_pages_keys_and_the_frames_as_they_apply_now', () => {
  assert.deepEqual(ids(everywhereGroups({ table: TABLE, frame: FRAME })),
    [[WORDS.page, ['mute', 'look', 'themes', 'play', HELP]]]);
  const guest = listed(FRAME_KEYS, { canWrite: false, help: true });
  assert.deepEqual(ids(everywhereGroups({ table: [], frame: guest })),
    [[WORDS.page, ['themes', HELP]]]);
  const html = pageHtml({
    page: 'everywhere', table: TABLE, frame: FRAME, focus: [['.list']], names: NAMES,
  });
  assert.ok(html.startsWith(`<p class="fp-note">${WORDS.everywhere}</p>`), html);
  assert.ok(!html.includes('list-pick'), 'no widget\'s key');
  assert.ok(html.includes('data-key="mute"') && html.includes(`data-key="${HELP}"`));
});

await ok('the_page_with_no_name_is_here', () => {
  const model = { table: TABLE, frame: FRAME, focus: [['.list']], names: NAMES };
  assert.equal(pageHtml({ page: null, ...model }), pageHtml({ page: 'here', ...model }));
});

await ok('about_has_the_version_the_daemon_and_links_that_open_in_a_new_tab', () => {
  const html = aboutHtml({ version: '0.33.0', daemon: 'box.lan:9000' });
  const pair = (label, value) => `<dt class="fp-row-label">${label}</dt>`
    + `<dd class="fp-row-value">${value}</dd>`;
  assert.ok(html.includes(pair('Version', '0.33.0')), html);
  assert.ok(html.includes(pair('Daemon', 'box.lan:9000')), html);
  const links = [...html.matchAll(/<a href="([^"]+)" target="_blank" rel="noopener">([^<]+)<\/a>/g)]
    .map((m) => [m[1], m[2]]);
  assert.deepEqual(links.map((one) => one[0]), LINKS.map((link) => link.href));
  assert.equal([...html.matchAll(/<a /g)].length, LINKS.length, 'every link opens in a new tab');
  assert.ok(html.includes(WORDS.newTab), 'and the page says so');
  // a link's words say where it leads
  for (const [href, words] of links) assert.equal(`https://${words}`, href);
  const hostile = aboutHtml({ version: HOSTILE, daemon: HOSTILE });
  assert.ok(!hostile.includes('<img') && hostile.includes(SAFE));
});

await ok('a_guest_reads_why_there_is_no_version', () => {
  const html = aboutHtml({ version: null, daemon: 'box.lan:9000' });
  assert.ok(html.includes(`<dd class="fp-row-value">${WORDS.versionGuest}</dd>`), html);
});

await ok('the_links_are_the_ones_the_repository_ships', () => {
  const root = new URL('../../../', import.meta.url);
  const cargo = readFileSync(new URL('Cargo.toml', root), 'utf8');
  const repository = /^repository = "([^"]+)"$/m.exec(cargo)[1];
  const readme = readFileSync(new URL('README.md', root), 'utf8');
  assert.equal(LINKS[0].href, repository);
  for (const link of LINKS.slice(1)) {
    assert.ok(link.href.startsWith(`${repository}/`), link.href);
    assert.ok(readme.includes(`(${link.href})`), link.href);
  }
});

// ---- Help cannot drift from the tables ----

const THEMES = { ember, paper, signal, nightdrive };
const VIEWERS = { writer: { canWrite: true, help: true }, guest: { canWrite: false, help: true } };

for (const [theme, { KEYS, ZONES }] of Object.entries(THEMES)) {
  await ok(`${theme}_every_zone_has_a_name_a_person_reads_and_no_name_is_left_over`, () => {
    const zones = [...new Set(KEYS.map((entry) => entry.zone))].filter((zone) => zone !== 'page');
    assert.deepEqual(Object.keys(ZONES).sort(), [...zones].sort());
    for (const words of Object.values(ZONES)) {
      assert.match(words, /^[A-Z][^.#[\]<>]*$/, words);
      assert.ok(words.length <= 40, words);
    }
  });

  await ok(`${theme}_help_shows_the_listed_entries_of_the_zones_it_names_and_no_other`, () => {
    const zones = [...new Set(KEYS.map((entry) => entry.zone))].filter((zone) => zone !== 'page');
    for (const [who, ctx] of Object.entries(VIEWERS)) {
      const table = listed(KEYS, ctx);
      const frame = listed(FRAME_KEYS, ctx);
      const known = new Set([...table, ...frame]);
      for (const focus of [[], ...zones.map((zone) => [[zone]])]) {
        const tag = `${theme}, ${who}, the focus in ${focus[0]?.[0] ?? 'nothing'}`;
        const here = hereGroups({ table, frame, focus, names: ZONES });
        const everywhere = everywhereGroups({ table, frame });
        for (const group of [...here.groups, ...everywhere]) {
          assert.ok(group.entries.length > 0, `${tag}: no empty group`);
          for (const entry of group.entries) {
            assert.ok(known.has(entry), `${tag}: ${entry.id} is listed`);
          }
        }
        const shown = here.groups.flatMap((group) => group.entries);
        assert.equal(new Set(shown).size, shown.length, `${tag}: no entry twice`);
        // a zone that holds the focus and has an entry that applies has its group, a zone of one
        // entry too
        for (const zone of focus.flat()) {
          const has = table.some((entry) => entry.zone === zone);
          const grouped = here.groups.some((group) => group.zones.includes(zone));
          assert.equal(grouped, has, `${tag}: ${zone}`);
        }
        // every listed entry of a zone Here names, the page included
        for (const group of here.groups) {
          const want = group.zones.includes('page')
            ? [...table.filter((entry) => entry.zone === 'page'), ...frame]
            : table.filter((entry) => group.zones.includes(entry.zone));
          const whole = focus.some((held) => held.some((zone) => group.zones.includes(zone)))
            || group.zones.includes('page');
          assert.deepEqual(group.entries, whole ? want : want.filter((entry) => entry.own), tag);
          assert.equal(group.heading,
            group.zones.includes('page') ? WORDS.page : ZONES[group.zones[0]], tag);
        }
        // the page's keys and the frame's are in Here and are the whole of Everywhere
        assert.deepEqual(everywhere.flatMap((group) => group.entries),
          [...table.filter((entry) => entry.zone === 'page'), ...frame], tag);
        assert.deepEqual(here.groups.at(-1).entries, everywhere[0].entries, tag);
        // what is drawn is what the groups hold, by id
        const html = pageHtml({ page: 'here', table, frame, focus, names: ZONES });
        assert.deepEqual([...html.matchAll(/data-key="([^"]+)"/g)].map((m) => m[1]),
          shown.map((entry) => entry.id), tag);
      }
    }
  });
}

// ---- the wire and the key, on the real core ----

const MODEL = {
  v: 1, viewer: { guest: false, writes: true, version: '0.33.0' },
  fleet: { devices: [{ url: 'A', name: 'Bifrost' }], hops: [] }, shown: 'A',
  device: { volume: { raw: 40 } }, followed: {}, card: null, host: null, strip: null,
};
const GUEST = { ...MODEL, viewer: { guest: true, writes: false } };
const BODY = '<div id="theme"><button id="plain">Plain</button>'
  + '<div class="dial" data-low tabindex="0" id="dial"></div>'
  + '<div class="list"><button id="option">One</button></div><input id="name"></div>'
  + '<div id="toasts"></div>';

/** The real core over a jsdom page, with the page's own list of popups and a theme on show
 * that gives `TABLE` and `NAMES`. `keys` counts the key listeners added to the window and the
 * document from then on. */
function rig({ model = MODEL } = {}) {
  const p = page(BODY, { url: 'http://box.lan:9000/' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  const core = createCore({ page: structuredClone(model), themes: [], window: p.window });
  const stop = new p.window.AbortController();
  core.keys.bind(TABLE, { signal: stop.signal });
  core.mounted = { keys: TABLE, zones: NAMES, unmount() {} };
  const keys = [];
  for (const target of [p.window, p.document]) {
    const add = target.addEventListener.bind(target);
    target.addEventListener = (kind, ...rest) => {
      if (/^key/.test(kind)) keys.push(kind);
      return add(kind, ...rest);
    };
  }
  const $ = (id) => p.document.getElementById(id);
  /** Wait for the popup's module and for the history's step. */
  const drawn = async () => {
    const state = () => $('popup-help')?.querySelector('.fp-box')?.dataset.state;
    for (let i = 0; i < 50 && state() !== 'ready'; i++) {
      await p.settle();
    }
    await p.settle();
  };
  const open = async (to = null) => {
    core.router.open(HELP, to);
    await drawn();
  };
  const close = async () => {
    core.router.close();
    await p.settle();
    await p.settle();
  };
  const all = (selector) => [...p.document.querySelectorAll(`#popup-help ${selector}`)];
  const shown = () => all('[data-key]').map((row) => row.dataset.key);
  const headings = () => all('.fp-heading').map((h) => h.textContent);
  return {
    ...p, core, keys, $, open, close, drawn, shown, headings,
  };
}

await ok('help_is_listed_last_with_its_row_for_everyone', async () => {
  assert.equal(POPUPS.at(-1).name, HELP);
  assert.deepEqual(POPUPS.at(-1).row, { label: 'Help…', sees: 'everyone' });
  assert.equal(POPUPS.at(-1).title, 'Help');
  assert.deepEqual(POPUPS.at(-1).entries, [
    { id: 'here', label: 'Here' }, { id: 'everywhere', label: 'Everywhere' },
    { id: 'about', label: 'About' },
  ]);
  assert.equal(rowId(HELP), 'menu-popup-help');
  const r = rig();
  assert.deepEqual(r.core.keys.listed(r.core.keys.frame).map((entry) => entry.id),
    ['themes', 'play', HELP]);
  r.window.close();
});

await ok('it_opens_on_here_with_the_keys_of_what_had_the_focus', async () => {
  const r = rig();
  r.$('dial').focus();
  await r.open();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: null });
  assert.equal(r.$('popup-help-title').textContent, 'Help');
  assert.equal(r.document.querySelector('#popup-help .fp-entry.on').textContent, 'Here');
  assert.deepEqual(r.headings(), ['The dial', 'The curve', WORDS.page]);
  assert.deepEqual(r.shown(),
    ['dial-step', 'dial-home', 'drag-end', 'mute', 'look', 'themes', 'play', HELP]);
  assert.equal(r.document.activeElement.id, 'popup-help-entry-here', 'the focus is on the list');
  await r.close();
  assert.equal(r.document.activeElement.id, 'dial', 'and back where it was');
  r.window.close();
});

await ok('here_is_read_at_each_show_and_not_when_the_popup_is_added', async () => {
  const r = rig();
  await r.open();
  assert.deepEqual(r.headings(), ['The curve', WORDS.page], 'nothing had the focus');
  assert.ok(r.$('popup-help').textContent.includes(WORDS.nowhere));
  await r.close();
  r.$('option').focus();
  await r.open();
  assert.deepEqual(r.headings(), ['A list', 'The curve', WORDS.page]);
  assert.ok(!r.$('popup-help').textContent.includes(WORDS.nowhere));
  await r.close();
  // a control that is in no zone
  r.$('plain').focus();
  await r.open();
  assert.deepEqual(r.headings(), ['The curve', WORDS.page]);
  r.window.close();
});

await ok('the_three_pages_step_and_the_tab_title_names_each', async () => {
  const r = rig();
  r.$('option').focus();
  await r.open();
  r.$('popup-help-entry-everywhere').click();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: 'everywhere' });
  assert.deepEqual(r.shown(), ['mute', 'look', 'themes', 'play', HELP]);
  assert.match(r.document.title, /Help · Everywhere/);
  r.$('popup-help-entry-about').click();
  await r.settle();
  assert.match(r.document.title, /Help · About/);
  const about = r.$('popup-help').querySelector('.fp-page').textContent;
  assert.ok(about.includes('0.33.0') && about.includes('box.lan:9000'), about);
  assert.deepEqual(r.shown(), []);
  r.$('popup-help-entry-here').click();
  await r.settle();
  assert.deepEqual(r.headings(), ['A list', 'The curve', WORDS.page], 'Here again, as it was');
  await r.close();
  assert.doesNotMatch(r.document.title, /Help/);
  r.window.close();
});

await ok('a_guest_opens_it_and_has_the_keys_a_guest_has', async () => {
  const r = rig({ model: GUEST });
  await r.open('everywhere');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: 'everywhere' });
  assert.deepEqual(r.shown(), ['mute', 'look', 'themes', HELP]);
  r.$('popup-help-entry-about').click();
  await r.settle();
  assert.ok(r.$('popup-help').textContent.includes(WORDS.versionGuest));
  r.window.close();
});

await ok('it_takes_no_stream_want_and_adds_no_key_listener', async () => {
  const r = rig();
  const before = JSON.stringify(r.core.stream.needed());
  await r.open();
  r.$('popup-help-entry-about').click();
  await r.settle();
  assert.equal(JSON.stringify(r.core.stream.needed()), before);
  assert.deepEqual(r.keys, []);
  r.key('Escape');
  await r.settle();
  await r.settle();
  assert.equal(r.$('popup-help'), null, 'Escape closes it');
  r.window.close();
});

await ok('the_question_mark_opens_help_through_the_router', async () => {
  const r = rig();
  r.$('dial').focus();
  const event = r.key('?', { shiftKey: true });
  assert.equal(event.defaultPrevented, true);
  await r.drawn();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: null });
  assert.deepEqual(r.headings()[0], 'The dial', 'Here is about what had the focus at the key');
  assert.equal(r.window.history.length, 2, 'one entry of the history');
  // with Help open the key is the modal's and does nothing
  r.key('?');
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: null });
  assert.equal(r.window.history.length, 2);
  await r.close();
  assert.equal(r.document.activeElement.id, 'dial');
  r.window.close();
});

await ok('the_question_mark_types_in_a_field_and_does_nothing_held_or_modified', async () => {
  const r = rig();
  r.$('name').focus();
  const typed = r.key('?');
  await r.settle();
  assert.equal(typed.defaultPrevented, false);
  assert.equal(r.core.router.state().popup, null, 'in a field it types');
  r.$('dial').focus();
  for (const init of [{ repeat: true }, { ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
    r.key('?', init);
    await r.settle();
    assert.equal(r.core.router.state().popup, null, JSON.stringify(init));
  }
  r.window.close();
});

await ok('the_question_mark_closes_the_menu_first_and_a_second_one_does_nothing', async () => {
  const r = rig();
  r.$('dial').focus();
  r.core.frame.openMenu();
  assert.equal(r.$('fx-menu').hidden, false);
  assert.ok(r.$('menu-popup-help'), 'the menu has the Help row');
  assert.equal(r.$('menu-popup-help').textContent.trim().replace(/\s*›$/, ''), 'Help…');
  r.key('?');
  await r.drawn();
  assert.equal(r.$('fx-menu').hidden, true, 'the menu closed');
  assert.equal(r.core.router.state().popup, HELP);
  assert.deepEqual(r.headings()[0], 'The dial',
    'the menu was not the theme: the dial had the focus last');
  r.key('?');
  await r.settle();
  assert.equal(r.core.router.state().popup, HELP, 'a second ? is the popup\'s own key: nothing');
  assert.equal(r.$('fx-menu').hidden, true);
  r.window.close();
});

await ok('the_menus_row_opens_it', async () => {
  const r = rig({ model: GUEST });
  r.core.frame.openMenu();
  r.$('menu-popup-help').click();
  await r.drawn();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: HELP, page: null });
  r.window.close();
});

console.log(`core-help.mjs: ${n} passed`);
