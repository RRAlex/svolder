/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the popup skeleton (core/popups/frame.js) and the sample popup that fills it
// (core/popups/sample/draw.js, wire.js). First the draws, as strings: a hostile title and a
// hostile device name are escaped, the side list marks its current entry, the box says whether
// the router is at the list or at a page (the class the phone layout turns on), the loading and
// failed states. Then the wire, on the real core over a jsdom page (dom.mjs) with the real modal
// stack and router: opening draws, a redraw keeps the focus by id, closing releases the stream
// want, Escape inside the inline ask cancels the ask and leaves the popup, a second Escape closes
// it, Back closes it, and none of these changes the shown device. The sample is no popup of the
// page: a core that nothing registered it with opens nothing.
// Run: node core-popups.mjs
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import {
  barId, closeId, createPopupFrame, currentOf, dot, entryId, heading, keycap, popupHtml, popupId,
  popupPanelId, redraw, rows, tag, titleId,
} from '../../../bins/svolderd/web/core/popups/frame.js';
import { tabsHtml } from '../../../bins/svolderd/web/core/popups/sound/draw.js';
import { frameRows } from '../../../bins/svolderd/web/core/menu-rows.js';
import { HELP, POPUPS, rowId } from '../../../bins/svolderd/web/core/popups/listed.js';
import { openAsks } from '../../../bins/svolderd/web/core/inline-ask.js';
import { openLists } from '../../../bins/svolderd/web/core/list.js';
import {
  ENTRIES, IDS, TITLE, VOICES, liveHtml, pageHtml,
} from '../../../bins/svolderd/web/core/popups/sample/draw.js';
import { wire } from '../../../bins/svolderd/web/core/popups/sample/wire.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const HOSTILE = '<img src=x onerror=alert(1)> "Q" & \'co\'';
const SAFE = '&lt;img src=x onerror=alert(1)&gt; &quot;Q&quot; &amp; &#39;co&#39;';
const TWO = [{ id: 'one', label: 'One' }, { id: 'two', label: 'Two <b>' }];

// ---- the draws ----

await ok('a_hostile_title_is_escaped_in_the_title_and_the_lists_label', () => {
  const html = popupHtml({ name: 'x', title: HOSTILE, entries: TWO });
  assert.ok(html.includes(`<h2 class="fp-title" id="popup-x-title">${SAFE}</h2>`), html);
  assert.ok(html.includes(`<nav class="fp-side" aria-label="${SAFE}">`), html);
  assert.ok(!html.includes('<img'), 'no tag of the title reaches the markup');
  assert.ok(html.includes('>Two &lt;b&gt;</button>'), "an entry's label too");
});

await ok('the_side_list_marks_its_current_entry', () => {
  const at = (html) => [...html.matchAll(/<button type="button" class="fp-entry( on)?" id="([^"]+)" data-popup-page="([^"]+)"( aria-current="page")?>/g)]
    .map((m) => [m[3], Boolean(m[1]), Boolean(m[4]), m[2]]);
  assert.deepEqual(at(popupHtml({ name: 'x', title: 'T', entries: TWO, page: 'two' })), [
    ['one', false, false, 'popup-x-entry-one'], ['two', true, true, 'popup-x-entry-two'],
  ]);
  assert.deepEqual(at(popupHtml({ name: 'x', title: 'T', entries: TWO, page: null })), [
    ['one', true, true, 'popup-x-entry-one'], ['two', false, false, 'popup-x-entry-two'],
  ], 'at the list the first entry is the one whose page a wide window shows');
  assert.equal(currentOf(TWO, null), 'one');
  assert.equal(currentOf(TWO, 'two'), 'two');
  assert.equal(currentOf([], null), null);
});

await ok('the_box_says_list_or_page_which_is_what_the_phone_layout_turns_on', () => {
  const box = (model) => /<div class="(fp-box [a-z-]+)" data-popup="x" data-state="([a-z]+)">/
    .exec(popupHtml({ name: 'x', title: 'T', ...model })).slice(1);
  assert.deepEqual(box({ entries: TWO, page: null }), ['fp-box fp-at-list', 'ready']);
  assert.deepEqual(box({ entries: TWO, page: 'one' }), ['fp-box fp-at-page', 'ready']);
  assert.deepEqual(box({ entries: [], page: null }), ['fp-box fp-single', 'ready']);
  assert.ok(!popupHtml({ name: 'x', title: 'T' }).includes('fp-side'), 'no entries: no side list');
});

await ok('the_head_has_the_title_and_the_close_button_and_a_page_its_way_back', () => {
  const html = popupHtml({ name: 'x', title: 'T', entries: TWO, body: '<p>hello</p>' });
  assert.ok(!html.includes('fp-back'), 'at the list there is no way back to it');
  const paged = popupHtml({ name: 'x', title: 'T', entries: TWO, page: 'two', body: '<p>hello</p>' });
  assert.ok(paged.includes('<section class="fp-page" data-list-place><button type="button" class="fp-back" '
    + 'id="popup-x-back" data-popup-back>‹ Back to the list</button><p>hello</p></section>'), paged);
  assert.ok(!popupHtml({ name: 'x', title: 'T', page: 'two' }).includes('fp-back'),
    'nor in a popup with no list');
  assert.ok(html.indexOf('fp-title') < html.indexOf('fp-close')
    && html.indexOf('fp-close') < html.indexOf('fp-side'), 'the head comes first');
  assert.ok(html.includes('<button type="button" class="fp-close" id="popup-x-close" data-popup-close aria-label="Close">'));
  assert.ok(html.includes('<section class="fp-page" data-list-place><p>hello</p></section>'), 'the body is markup, as given');
  assert.equal(titleId('x'), 'popup-x-title');
  assert.equal(closeId('x'), 'popup-x-close');
  assert.equal(popupId('x'), 'popup-x');
  assert.equal(entryId('x', 'one'), 'popup-x-entry-one');
});

await ok('loading_and_failed_put_the_frames_line_in_place_of_the_body', () => {
  const loading = popupHtml({ name: 'x', title: 'T', body: '<p>hello</p>', state: 'loading' });
  assert.ok(loading.includes('data-state="loading"'));
  assert.ok(loading.includes('<section class="fp-page" data-list-place><p class="fp-note" role="status">Loading…</p></section>'));
  const failed = popupHtml({ name: 'x', title: 'T', body: '<p>hello</p>', state: 'failed' });
  assert.ok(failed.includes('<p class="fp-note fp-failed" role="alert">This could not be loaded.'));
  assert.ok(!failed.includes('hello') && !loading.includes('hello'));
  assert.ok(failed.includes('data-popup-close'), 'the close button is there in both');
});

await ok('an_entry_carries_a_dot_a_tag_and_a_group_break_and_a_plain_one_is_as_it_was', () => {
  const html = popupHtml({
    name: 'x', title: 'T', page: 'b',
    entries: [
      { id: 'a', label: 'Bifrost', dot: 'on', dotWords: 'Connected', tag: 'Shown <1>' },
      { id: 'b', label: 'Kara', dot: 'off' },
      { id: 'c', label: 'Add a device', group: true },
    ],
  });
  assert.ok(html.includes('<button type="button" class="fp-entry" id="popup-x-entry-a" data-popup-page="a">'
    + '<span class="fp-dot on" role="img" aria-label="Connected"></span>Bifrost'
    + '<span class="fp-tag">Shown &lt;1&gt;</span></button>'), html);
  assert.ok(html.includes('<button type="button" class="fp-entry on" id="popup-x-entry-b" data-popup-page="b" '
    + 'aria-current="page"><span class="fp-dot off" aria-hidden="true"></span>Kara</button>'), html);
  assert.ok(html.includes('<hr class="fp-rule">'
    + '<button type="button" class="fp-entry" id="popup-x-entry-c" data-popup-page="c">Add a device</button>'),
  html);
  assert.equal(html.split('fp-rule').length, 2, 'one break, before the entry that asks for it');
  assert.ok(!popupHtml({ name: 'x', title: 'T', entries: [{ id: 'a', label: 'A', group: true }] })
    .includes('fp-rule'), 'the first entry has nothing to break from');
  assert.equal(dot('bogus'), '<span class="fp-dot off" aria-hidden="true"></span>',
    'a state the sheet does not know is off');
});

await ok('the_shared_pieces_are_strings_that_escape_their_words', () => {
  assert.equal(keycap('<'), '<kbd class="fp-key">&lt;</kbd>');
  assert.equal(dot('warn', HOSTILE), `<span class="fp-dot warn" role="img" aria-label="${SAFE}"></span>`);
  assert.equal(tag(HOSTILE), `<span class="fp-tag">${SAFE}</span>`);
  assert.equal(heading(HOSTILE), `<h3 class="fp-heading">${SAFE}</h3>`);
  assert.equal(rows([{ label: '<b>a</b>', value: '<i>41</i>', attrs: ' data-x="1"' }, { label: 'b', value: '' }]),
    '<dl class="fp-rows"><div class="fp-row" data-x="1"><dt class="fp-row-label"><b>a</b></dt>'
    + '<dd class="fp-row-value"><i>41</i></dd></div><div class="fp-row"><dt class="fp-row-label">b</dt>'
    + '<dd class="fp-row-value"></dd></div></dl>', 'the label and the value are markup the draw made safe');
});

await ok('every_shared_class_has_a_rule_dressed_by_the_frames_tokens_alone', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/core/frame.css', import.meta.url), 'utf8');
  assert.match(css, /^\.fi-input:user-invalid \{ border-color: var\(--frame-danger\); \}/m,
    'a name field left wrong takes the danger border once the person has acted on it');
  const defaults = new Set([...css.matchAll(/(--frame-[a-z-]+):/g)].map((m) => m[1]));
  for (const name of ['fp-key', 'fp-dot', 'fp-dot.on', 'fp-dot.warn', 'fp-tag', 'fp-heading', 'fp-row',
    'fp-row-label', 'fp-row-value', 'fp-graph', 'fp-graph-grid', 'fp-graph-series',
    'fp-graph-target', 'fp-rule']) {
    const rule = new RegExp(`^\\.${name.replace('.', '\\.')} \\{([^}]*)\\}`, 'm').exec(css);
    assert.ok(rule, `frame.css has a rule for .${name}`);
    const used = [...rule[1].matchAll(/var\((--[a-z-]+)/g)].map((m) => m[1]);
    assert.ok(used.length > 0, `.${name} is dressed by a token`);
    for (const token of used) {
      assert.ok(defaults.has(token), `.${name} reads ${token}, which frame.css has no default for`);
    }
    assert.ok(!/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i.test(rule[1]), `.${name} names no colour of its own`);
  }
});

await ok('the_box_has_an_entry_animation_hook_that_no_theme_sets', () => {
  const web = new URL('../../../bins/svolderd/web/', import.meta.url);
  const css = readFileSync(new URL('core/frame.css', web), 'utf8');
  const box = /^\.fp-box \{([^}]*)\}/m.exec(css)[1];
  assert.match(box, /animation: var\(--frame-popup-in, none\);/, 'off unless a theme sets the token');
  assert.ok(!/--frame-popup-in\s*:/.test(css), 'the frame gives it no value');
  assert.ok(!css.includes('fp-again'), 'the box is one element: nothing has to switch the animation off');
  for (const theme of readdirSync(new URL('themes/', web), { withFileTypes: true })) {
    if (!theme.isDirectory()) continue;
    for (const file of readdirSync(new URL(`themes/${theme.name}/`, web))) {
      if (!file.endsWith('.css')) continue;
      assert.ok(!readFileSync(new URL(`themes/${theme.name}/${file}`, web), 'utf8').includes('--frame-popup-in'),
        `${theme.name}/${file} sets no entry animation here`);
    }
  }
});

await ok('the_samples_draws_escape_the_device_and_name_the_device_they_follow', () => {
  const live = liveHtml({ device: { url: 'A', name: HOSTILE }, volume: { raw: 41 } });
  assert.equal(live, `${SAFE}: volume 41`);
  assert.equal(liveHtml({ device: { url: 'A', name: 'Bifrost' }, volume: null }),
    'Bifrost: no reading yet');
  assert.equal(liveHtml({ device: null, volume: null }), 'No device');
  const one = pageHtml({ page: null, device: { url: 'A', name: HOSTILE }, volume: null, name: '' });
  assert.ok(one.includes(`<p class="fp-live" id="${IDS.live}" aria-live="off">${SAFE}: no reading yet</p>`), one);
  assert.ok(one.includes(`id="${IDS.askOpen}"`) && one.includes(`id="${IDS.formOpen}"`));
  assert.ok(one.includes(`<div id="${IDS.ask}"></div>`), 'the place the inline ask is drawn in');
  assert.ok(!one.includes('<img'));
  const two = pageHtml({ page: 'two', device: { url: 'A', name: 'B' }, volume: null, name: HOSTILE });
  assert.ok(two.includes(SAFE) && !two.includes('<img'), 'the name the form gave, escaped');
  assert.ok(!two.includes(IDS.askOpen), 'page two has no ask');
  assert.deepEqual(ENTRIES.map((entry) => entry.id), ['one', 'two']);
  assert.equal(TITLE, 'Sample popup');
});

// ---- the wire ----

const MODEL = {
  v: 1, viewer: { guest: false, writes: true },
  fleet: { devices: [{ url: 'A', name: 'Bifrost' }], hops: [] }, shown: 'A',
  device: { volume: { raw: 40 } }, followed: {}, card: null, host: null, strip: null,
};
const BODY = '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';

/** The real core over a jsdom page; with `sample`, the sample popup is added to its popups,
 * loaded by `load` (at once by default; the cases of the lazy loading pass their own). */
function rig({ sample = true, load = async () => ({ wire }), popups, model = MODEL } = {}) {
  const p = page(BODY);
  globalThis.window = p.window;
  globalThis.document = p.document;
  // jsdom's listeners take only its own signals (list.js makes one)
  globalThis.AbortController = p.window.AbortController;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  const core = createCore({
    page: structuredClone(model), themes: [], window: p.window, ...(popups ? { popups } : {}),
  });
  const shows = [];
  const show = core.actions.showDevice;
  core.actions.showDevice = (...args) => { shows.push(args[0]); return show(...args); };
  const frame = sample ? core.popups.add('sample', { title: TITLE, load }) : null;
  const $ = (id) => p.document.getElementById(id);
  const at = () => p.document.activeElement?.id ?? null;
  /** Open the sample from the theme's knob and let the history's step land. */
  const open = async (to = null) => {
    $('knob').focus();
    core.router.open('sample', to);
    await p.settle();
  };
  /** Do `act` and wait until the step of the history it starts has landed. */
  const landed = (act) => new Promise((resolve) => {
    p.window.addEventListener('popstate', () => p.window.setTimeout(resolve, 0), { once: true });
    act();
  });
  const back = () => landed(() => p.window.history.back());
  return { ...p, core, frame, shows, $, at, open, back, landed };
}

await ok('a_core_nothing_registered_the_sample_with_opens_nothing', async () => {
  const r = rig({ sample: false });
  r.core.router.open('sample');
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.$('popup-sample'), null);
  assert.equal(r.window.history.length, 1, 'and pushes no entry');
  r.window.close();
});

await ok('nothing_of_the_page_names_the_sample', () => {
  // no menu row, no key, no list of popups: outside its own folder no shipped file names it
  const web = new URL('../../../bins/svolderd/web/', import.meta.url);
  const hits = [];
  const text = (rel) => readFileSync(new URL(rel, web), 'utf8');
  const walk = (dir) => {
    for (const entry of readdirSync(new URL(dir, web), { withFileTypes: true })) {
      const rel = `${dir}${entry.name}`;
      if (entry.isDirectory()) {
        if (rel !== 'core/popups/sample') walk(`${rel}/`);
      } else if (/\.(js|css|html|json)$/.test(entry.name)
        && (/popups\/sample|sample\/(wire|draw)|popup-sample|popup:sample/.test(text(rel))
          // the name as a word of its own, in the core (a theme adds no popup and may use the
          // word for its own things)
          || (rel.startsWith('core/') && /['"`]sample['"`]/.test(text(rel))))) {
        hits.push(rel);
      }
    }
  };
  walk('');
  assert.deepEqual(hits, []);
});

await ok('opening_draws_the_frame_the_list_and_the_live_line', async () => {
  const r = rig();
  await r.open();
  const el = r.$('popup-sample');
  assert.equal(el.parentElement, r.document.body);
  assert.equal(el.getAttribute('role'), 'dialog');
  assert.equal(el.getAttribute('aria-labelledby'), 'popup-sample-title');
  assert.equal(r.$('popup-sample-title').textContent, 'Sample popup');
  assert.deepEqual([...el.querySelectorAll('.fp-entry')].map((one) => [one.textContent, one.className]),
    [['Page one', 'fp-entry on'], ['Page two', 'fp-entry']]);
  assert.ok(el.querySelector('.fp-box').classList.contains('fp-at-list'));
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40', 'the live line names its device');
  assert.equal(r.$('theme').inert, true, 'the page behind is inert');
  assert.equal(r.at(), 'popup-sample-entry-one', 'the focus opens on the current entry');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  r.window.close();
});

await ok('a_reading_redraws_the_live_line_alone', async () => {
  const r = rig();
  await r.open();
  const para = r.$('popup-sample').querySelector('.fp-page p');
  r.core.model.applyParts('A', { volume: { raw: 43 } });
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 43');
  assert.equal(r.$('popup-sample').querySelector('.fp-page p'), para, 'the page was not drawn again');
  r.window.close();
});

await ok('a_redraw_keeps_the_focus_by_id', async () => {
  const r = rig();
  await r.open();
  const two = r.$('popup-sample-entry-two');
  two.focus();
  two.click();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: 'two' });
  assert.notEqual(r.$('popup-sample-entry-two'), two, 'the box was drawn again');
  assert.equal(r.at(), 'popup-sample-entry-two', 'and the focus is on the twin of the entry');
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-page'));
  // a control that is gone after the redraw: the focus goes to the popup, never to the page
  const place = r.$('popup-sample').querySelector('.fp-page');
  place.innerHTML = '<button id="gone">x</button>';
  r.$('gone').focus();
  redraw(place, '<p>nothing to focus</p>', r.$('popup-sample'));
  assert.equal(r.at(), 'popup-sample');
  // a focus outside the place is left alone
  r.$('popup-sample-close').focus();
  redraw(place, '<button id="gone">x</button>', r.$('popup-sample'));
  assert.equal(r.at(), 'popup-sample-close');
  r.window.close();
});

/** Make the page `r` lay out as a sheet would: what matches `unrendered` takes no focus and says
 * it is not drawn, and neither does an element that is not in the page. jsdom lays nothing out,
 * so the two things a browser does are put on the elements' prototype. */
function layout(r, unrendered) {
  const hidden = (el) => !el.isConnected || Boolean(el.closest(unrendered));
  const proto = r.window.HTMLElement.prototype;
  const focus = proto.focus;
  proto.focus = function laid(...args) {
    if (!hidden(this)) focus.apply(this, args);
  };
  proto.checkVisibility = function laid() {
    return !hidden(this);
  };
}
/** The phone layout: no side list at a page, no page pane at the list. */
const phone = (r) => layout(r, '.fp-at-page .fp-side, .fp-at-list .fp-page');
/** A wide window: the way back from a page is not shown. */
const wide = (r) => layout(r, '.fp-back');

await ok('a_pane_that_holds_more_than_it_shows_is_a_named_stop_of_the_keyboard', async () => {
  const r = rig();
  // jsdom lays nothing out: the pane's sizes are put on the prototype, and the observer of the
  // box's size is one the case calls
  let tall = 900;
  const sized = [];
  r.window.ResizeObserver = class {
    constructor(fn) { sized.push(fn); }

    observe() {}

    disconnect() {}
  };
  const proto = r.window.HTMLElement.prototype;
  Object.defineProperty(proto, 'scrollHeight', {
    configurable: true, get() { return this.classList.contains('fp-page') ? tall : 0; },
  });
  Object.defineProperty(proto, 'clientHeight', {
    configurable: true, get() { return this.classList.contains('fp-page') ? 300 : 0; },
  });
  // a wide window: the way back, a control in the pane's markup, is not shown
  wide(r);
  const frame = createPopupFrame({
    document: r.document, router: r.core.router, name: 'long', title: 'Long',
  });
  r.document.body.append(frame.el);
  const pane = () => frame.el.querySelector('.fp-page');
  const marks = () => ['tabindex', 'role', 'aria-labelledby'].map((name) => pane().getAttribute(name));
  frame.draw({ entries: TWO, page: 'two', body: '<p>a long page with no control</p>' });
  assert.deepEqual(marks(), ['0', 'region', 'popup-long-entry-two'],
    'it takes the focus, so the keyboard scrolls it, and is named after its entry');
  pane().focus();
  assert.equal(r.document.activeElement, pane());
  frame.draw({ entries: [], body: '<p>long</p>' });
  assert.deepEqual(marks(), ['0', 'region', 'popup-long-title'], 'with no list, after the title');
  // tabs dropped while the pane is a long region that has the focus: its region marks stay
  frame.draw({ entries: [], body: '<p>long</p>', bar: '<div class="fp-tabs"><button class="fp-tab on" id="t1">a</button></div>' });
  assert.equal(marks()[1], 'tabpanel');
  pane().focus();
  frame.draw({ entries: [], body: '<p>long</p>' });
  assert.deepEqual(marks(), ['0', 'region', 'popup-long-title'], 'marks of a region are kept');
  // a pane that shows all it holds is no stop
  tall = 300;
  frame.draw({ entries: TWO, page: 'two', body: '<p>short</p>' });
  assert.deepEqual(marks(), [null, null, null]);
  // the window made lower: the box's size changed, nothing was drawn
  tall = 900;
  assert.ok(sized.length >= 1, 'the frame watches the size of its box');
  for (const fn of sized) fn([]);
  assert.deepEqual(marks(), ['0', 'region', 'popup-long-entry-two']);
  // one place drawn again (a live line) may make the page longer or shorter
  tall = 300;
  frame.put('nowhere', 'x');
  frame.el.querySelector('.fp-page').innerHTML = '<p id="live">a</p>';
  frame.put('live', 'b');
  assert.deepEqual(marks(), [null, null, null]);
  // a pane with a control that is shown scrolls from that control: no stop of its own
  tall = 900;
  frame.draw({ entries: TWO, page: 'two', body: '<p>long</p><button id="in-pane">a control</button>' });
  assert.deepEqual(marks(), [null, null, null]);
  frame.draw({ entries: TWO, page: 'two', body: '<p>long</p><button hidden>not shown</button>' });
  frame.el.querySelector('.fp-page button[hidden]').checkVisibility = () => false;
  for (const fn of sized) fn([]);
  assert.deepEqual(marks(), ['0', 'region', 'popup-long-entry-two'], 'one that is not shown is none');
  frame.draw({ entries: TWO, page: 'two', body: '<p>long</p>' });
  // it keeps the focus it has, even once it shows everything
  tall = 900;
  for (const fn of sized) fn([]);
  pane().focus();
  tall = 300;
  for (const fn of sized) fn([]);
  assert.equal(r.document.activeElement, pane(), 'the focus is not dropped onto the page behind');
  assert.equal(pane().getAttribute('tabindex'), '0');
  // a caret left on the page behind goes as the pane takes the focus; selected words stay
  const knob = r.$('knob');
  const selection = r.window.getSelection();
  knob.focus();
  selection.collapse(knob.firstChild, 2);
  pane().focus();
  assert.equal(selection.rangeCount, 0, 'End and Home are the pane\'s, not a caret\'s on the page behind');
  knob.focus();
  selection.selectAllChildren(knob);
  pane().focus();
  assert.equal(String(selection), 'Volume', 'a selection is the person\'s and is kept');
  frame.el.remove();
  r.window.close();
});

await ok('tab_goes_round_a_popup_past_what_its_layout_does_not_show', async () => {
  // a wide window at a page with no control of its own: the way back, which is not shown, is the
  // last control in the markup
  let r = rig();
  wide(r);
  await r.open('two');
  assert.equal(r.at(), 'popup-sample-entry-two');
  assert.equal(r.key('Tab').defaultPrevented, true, 'wide: Tab from the last control that is shown');
  assert.equal(r.at(), 'popup-sample-close', 'wide: goes round to the close button');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, true);
  assert.equal(r.at(), 'popup-sample-entry-two', 'wide: and Shift+Tab back to the last shown');
  r.window.close();
  // a phone at the list: the page pane, which is not shown, holds the last controls
  r = rig();
  phone(r);
  await r.open();
  r.$('popup-sample-entry-two').focus();
  assert.equal(r.key('Tab').defaultPrevented, true, 'phone, list: Tab from the last entry');
  assert.equal(r.at(), 'popup-sample-close');
  assert.equal(r.key('Tab', { shiftKey: true }).defaultPrevented, true);
  assert.equal(r.at(), 'popup-sample-entry-two', 'phone, list: Shift+Tab from the close button');
  r.window.close();
  // a phone at a page: the side list, which is not shown, comes before the way back
  r = rig();
  phone(r);
  await r.open('two');
  assert.equal(r.at(), 'popup-sample-back');
  assert.equal(r.key('Tab').defaultPrevented, true, 'phone, page: the way back is the last shown');
  assert.equal(r.at(), 'popup-sample-close');
  assert.equal(r.key('Tab').defaultPrevented, false, 'phone, page: close to the way back is the browser\'s');
  r.window.close();
});

await ok('on_a_phone_a_step_keeps_the_focus_inside_the_popup', async () => {
  const r = rig();
  phone(r);
  await r.open();
  assert.equal(r.at(), 'popup-sample-entry-one', 'at the list the focus opens on the current entry');
  const two = r.$('popup-sample-entry-two');
  two.focus();
  two.click();
  await r.settle();
  // the entry's twin is in the side list, which a phone does not show at a page
  assert.equal(r.at(), 'popup-sample-back', 'list to page: the focus is on the way back');
  r.$('popup-sample-back').click();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.equal(r.at(), 'popup-sample-entry-one', 'page to list: the focus is on the list');
  await r.landed(() => r.key('Escape'));
  // opened straight at a page: the focus opens on the way back, not on a hidden entry
  await r.open('two');
  assert.equal(r.at(), 'popup-sample-back');
  r.window.close();
});

await ok('the_tab_title_names_the_open_popup_and_its_page_until_it_closes_any_way', async () => {
  const r = rig();
  const base = r.document.title;
  assert.match(base, /^Bifrost · .+ — svolder$/);
  const withWords = (words) => base.replace(' — svolder', ` · ${words} — svolder`);
  await r.open();
  assert.equal(r.document.title, withWords('Sample popup'), 'at its list, the popup alone');
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(r.document.title, withWords('Sample popup · Page two'));
  await r.back();
  assert.equal(r.document.title, withWords('Sample popup'), 'Back to the list');
  await r.landed(() => r.key('Escape'));
  assert.equal(r.document.title, base, 'Escape');
  await r.open('two');
  assert.equal(r.document.title, withWords('Sample popup · Page two'), 'opened straight at a page');
  await r.back();
  assert.equal(r.document.title, base, 'Back');
  await r.open();
  await r.landed(() => r.$('popup-sample-close').click());
  assert.equal(r.document.title, base, 'the close button');
  await r.open();
  await r.landed(() => r.core.modal.closeAll());
  assert.equal(r.document.title, base, 'the stack emptied');
  // a draw that names another title is in the tab at once
  await r.open();
  r.frame.draw({ title: 'Sample for Bifrost', entries: ENTRIES, page: null, body: '' });
  assert.equal(r.document.title, withWords('Sample for Bifrost'));
  await r.landed(() => r.key('Escape'));
  assert.equal(r.document.title, base);
  r.window.close();
});

await ok('the_box_is_one_element_through_every_draw_so_it_comes_in_once_an_opening', async () => {
  // an animation runs when its element enters the page: the box that is drawn as the popup
  // opens is the box of the loading line, of the part that arrives and of every step
  let arrive;
  const r = rig({ load: () => new Promise((resolve) => { arrive = () => resolve({ wire }); }) });
  await r.open();
  const el = r.$('popup-sample');
  const box = el.querySelector('.fp-box');
  assert.equal(box.dataset.state, 'loading');
  arrive();
  await r.settle();
  assert.equal(el.querySelector('.fp-box'), box, 'the part arriving draws into the same box');
  assert.equal(box.dataset.state, 'ready');
  assert.ok(box.classList.contains('fp-at-list'));
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(el.querySelector('.fp-box'), box, 'and so does a step');
  assert.ok(box.classList.contains('fp-at-page') && !box.classList.contains('fp-at-list'));
  assert.equal(el.querySelectorAll('.fp-box').length, 1);
  assert.equal(el.children.length, 1, 'the popup holds its box and nothing else');
  assert.ok(!el.classList.contains('fp-again'));
  await r.landed(() => r.key('Escape'));
  r.window.close();
});

await ok('a_click_on_the_entry_whose_page_the_pane_shows_pushes_no_entry', async () => {
  const r = rig();
  await r.open();
  const depth = r.window.history.length;
  // a wide window at the list: the pane holds the first entry's page already
  r.$('popup-sample-entry-one').click();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.equal(r.window.history.length, depth, 'no entry: Back still closes the popup in one step');
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-list'));
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(r.core.router.state().page, 'two', 'another entry steps to its page');
  assert.equal(r.window.history.length, depth + 1);
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(r.window.history.length, depth + 1, 'the entry of the page on show: nothing');
  r.$('popup-sample-entry-one').click();
  await r.settle();
  assert.equal(r.core.router.state().page, 'one', 'from a page, the first entry is a step in place');
  assert.equal(r.window.history.length, depth + 1);
  await r.landed(() => r.key('Escape'));
  r.window.close();
  // a phone at the list shows no pane: the first entry opens its page
  const narrow = rig();
  phone(narrow);
  await narrow.open();
  narrow.$('popup-sample-entry-one').click();
  await narrow.settle();
  assert.equal(narrow.core.router.state().page, 'one');
  narrow.window.close();
});

await ok('a_twin_that_takes_no_focus_gives_way_to_the_next_choice', () => {
  const p = page('<div id="place"><button id="a">a</button></div><button id="out">out</button>');
  const place = p.document.getElementById('place');
  p.document.getElementById('a').focus();
  redraw(place, '<button id="a" disabled>a</button><button id="b">b</button>',
    () => [null, p.document.getElementById('b'), place]);
  assert.equal(p.document.activeElement.id, 'b', 'the twin is disabled: the first choice that takes it');
  redraw(place, '<p>nothing</p>', () => [p.document.getElementById('out')]);
  assert.equal(p.document.activeElement.id, 'out');
  p.window.close();
});

await ok('closing_releases_the_stream_want_and_gives_the_focus_back', async () => {
  const r = rig();
  assert.deepEqual(r.core.stream.needed(), []);
  await r.open();
  assert.deepEqual(r.core.stream.needed(), ['meters'], 'the want is held while the popup is open');
  r.$('popup-sample-close').click();
  await r.settle();
  assert.deepEqual(r.core.stream.needed(), [], 'and released at the close');
  assert.equal(r.$('popup-sample'), null);
  assert.equal(r.at(), 'knob', 'the focus is back where it was');
  assert.equal(r.$('theme').inert, false);
  // a reading after the close reaches no listener of the sample's: it puts nothing
  let puts = 0;
  const put = r.frame.put;
  r.frame.put = (...args) => { puts += 1; return put(...args); };
  r.core.model.applyParts('A', { volume: { raw: 50 } });
  assert.equal(puts, 0, 'the store listeners went with the popup');
  r.frame.put = put;
  // and it opens again, with the reading of now
  await r.open();
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 50');
  assert.deepEqual(r.core.stream.needed(), ['meters']);
  r.window.close();
});

await ok('a_popup_opened_from_a_control_drawn_again_meanwhile_returns_the_focus_to_its_twin', async () => {
  const r = rig();
  await r.open();
  const knob = r.$('knob');
  // a reading redraws the theme's control while the popup stands
  r.$('theme').innerHTML = '<button id="knob">Volume</button>';
  await r.landed(() => r.key('Escape'));
  assert.equal(r.$('popup-sample'), null);
  assert.equal(r.at(), 'knob');
  assert.notEqual(r.document.activeElement, knob, 'the twin by id, not the control that left the page');
  r.window.close();
});

await ok('escape_in_the_inline_ask_cancels_the_ask_and_a_second_escape_closes_the_popup', async () => {
  const r = rig();
  await r.open();
  r.$(IDS.askOpen).focus();
  r.$(IDS.askOpen).click();
  const box = r.$(IDS.ask).querySelector('.fi-askbox');
  assert.ok(box, 'the question is drawn in its place');
  assert.equal(r.document.activeElement.dataset.answer, 'keep', 'the safe answer has the focus');
  r.key('Escape');
  await r.settle();
  assert.equal(r.$(IDS.ask).querySelector('.fi-askbox'), null, 'the ask is gone');
  assert.ok(r.$('popup-sample'), 'the popup is still open');
  assert.equal(r.at(), IDS.askOpen, 'the focus is back on the word that asked');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null, 'the second Escape closes the popup');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.at(), 'knob');
  assert.deepEqual(r.shows, [], 'no device was shown');
  assert.equal(r.core.model.shown(), 'A');
  r.window.close();
});

await ok('the_list_takes_its_keys_from_the_popup_and_a_pick_or_escape_closes_the_list_alone', async () => {
  const r = rig();
  await r.open();
  const chip = r.$(IDS.listOpen);
  const panel = () => r.$('popup-sample').querySelector('.fi-list');
  chip.focus();
  chip.click();
  assert.equal(panel().parentElement, r.$('popup-sample').querySelector('.fp-page'),
    'the panel is in the page pane, the popup\'s place for a list');
  assert.equal(chip.getAttribute('aria-expanded'), 'true');
  assert.equal(r.document.activeElement.textContent, VOICES[0], 'the voice in use has the focus');
  assert.equal(r.key('ArrowDown').defaultPrevented, true, 'the arrow is the list\'s, through onKey');
  assert.equal(r.document.activeElement.textContent, VOICES[1]);
  r.key('Escape');
  await r.settle();
  assert.equal(panel(), null, 'Escape closes the list');
  assert.ok(r.$('popup-sample'), 'and leaves the popup open');
  assert.equal(r.at(), IDS.listOpen, 'the focus is back on the chip');
  // the chip toggles its list
  chip.click();
  assert.ok(panel());
  chip.click();
  assert.equal(panel(), null);
  // a pick closes the list, keeps the popup and is shown
  chip.click();
  panel().children[2].click();
  assert.equal(panel(), null);
  assert.ok(r.$('popup-sample'));
  assert.equal(r.$(IDS.voice).textContent, VOICES[2]);
  assert.equal(r.at(), IDS.listOpen);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null, 'with no list open Escape closes the popup');
  assert.equal(openLists(), 0);
  r.window.close();
});

await ok('a_put_over_the_place_of_an_open_lists_opener_or_an_asks_place_closes_it', async () => {
  const r = rig();
  await r.open();
  const pop = () => r.$('popup-sample');
  const chip = r.$(IDS.listOpen);
  chip.parentElement.id = 'chip-place';
  chip.click();
  assert.equal(openLists(), 1);
  // a put elsewhere leaves the list alone
  r.frame.put(IDS.live, '<span>other</span>');
  assert.equal(openLists(), 1, 'a put over another place keeps the list');
  assert.ok(pop().querySelector('.fi-opt:focus'), 'the focus is on an option');
  r.frame.put('chip-place', '<p>drawn again</p>');
  assert.equal(openLists(), 0, 'the opener left with its place: the list is closed');
  assert.equal(pop().querySelector('.fi-list'), null, 'and its panel is gone');
  assert.ok(pop().contains(r.document.activeElement), 'the focus stays in the popup');
  r.window.close();
  // the ask: its place is put over
  const q = rig();
  await q.open();
  q.$(IDS.askOpen).click();
  assert.equal(openAsks(), 1);
  q.frame.put(IDS.live, '<span>other</span>');
  assert.equal(openAsks(), 1, 'a put over another place keeps the ask');
  q.frame.put(IDS.ask, '<p>drawn again</p>');
  assert.equal(openAsks(), 0, 'the ask\'s own place was put over: the ask is closed');
  assert.ok(q.$('popup-sample').contains(q.document.activeElement), 'the focus stays in the popup');
  q.key('Escape');
  await q.settle();
  assert.equal(q.$('popup-sample'), null, 'no ghost ask takes the Escape: it closes the popup');
  q.window.close();
});

await ok('a_bar_of_tabs_makes_the_page_pane_the_tabpanel_of_the_tab_shown', async () => {
  const r = rig();
  await r.open();
  const pane = () => r.$('popup-sample').querySelector('.fp-page');
  const bar = (on) => '<div class="fp-tabs" role="tablist">'
    + ['a', 'b'].map((id) => `<button type="button" role="tab" id="t-${id}" class="fp-tab${id === on ? ' on' : ''}"`
      + ` aria-controls="${popupPanelId('sample')}">${id}</button>`).join('') + '</div>';
  assert.equal(pane().getAttribute('role'), null, 'with no tabs the pane has no role');
  r.frame.draw({ entries: [], page: null, body: '<p>x</p>', bar: bar('a') });
  assert.equal(pane().getAttribute('role'), 'tabpanel');
  assert.equal(pane().getAttribute('aria-labelledby'), 't-a', 'named by the tab shown');
  assert.equal(pane().id, popupPanelId('sample'), 'the id the tabs control');
  r.frame.put(barId('sample'), bar('b'));
  assert.equal(pane().getAttribute('aria-labelledby'), 't-b', 'a put of the bar follows the tab');
  r.frame.draw({ entries: [], page: null, body: '<p>x</p>' });
  assert.equal(pane().getAttribute('role'), null, 'a draw with no tabs drops the role');
  assert.equal(pane().id, '', 'and the id');
  // the Sound popup's tabs name the pane by the same id
  assert.ok(tabsHtml({ page: null }).includes(`aria-controls="${popupPanelId('sound')}"`));
  r.window.close();
});

await ok('a_reading_leaves_an_open_ask_and_its_focus_alone', async () => {
  const r = rig();
  await r.open();
  r.$(IDS.formOpen).click();
  const input = r.$(IDS.ask).querySelector('input');
  assert.equal(r.document.activeElement, input);
  input.value = 'half typed';
  r.core.model.applyParts('A', { volume: { raw: 44 } });
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 44');
  assert.equal(r.document.activeElement, input, 'the field keeps the focus');
  assert.equal(input.value, 'half typed');
  r.window.close();
});

await ok('the_form_names_the_sample_and_escape_in_it_cancels', async () => {
  const r = rig();
  await r.open();
  r.$(IDS.formOpen).focus();
  r.$(IDS.formOpen).click();
  r.key('Escape');
  assert.equal(r.$(IDS.ask).querySelector('form'), null, 'Escape cancels the form');
  assert.ok(r.$('popup-sample'));
  r.$(IDS.formOpen).click();
  const form = r.$(IDS.ask).querySelector('form');
  form.querySelector('input').value = 'Evening <1>';
  form.dispatchEvent(new r.window.Event('submit', { cancelable: true }));
  await r.settle();
  assert.equal(r.$(IDS.ask).querySelector('form'), null, 'a name that is taken closes the form');
  assert.equal(r.$(IDS.name).textContent, 'Evening <1>');
  r.window.close();
});

await ok('the_questions_yes_counts_and_closes_it', async () => {
  const r = rig();
  await r.open();
  r.$(IDS.askOpen).click();
  r.$(IDS.ask).querySelector('[data-answer=save]').click();
  await r.settle();
  assert.equal(r.$(IDS.ask).querySelector('.fi-askbox'), null);
  assert.equal(r.$(IDS.count).textContent, '1');
  r.window.close();
});

await ok('back_closes_it_and_never_changes_the_device', async () => {
  const r = rig();
  await r.open('two');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: 'two' });
  await r.back();
  assert.equal(r.$('popup-sample'), null);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.deepEqual(r.core.stream.needed(), []);
  assert.deepEqual(r.shows, []);
  assert.equal(r.at(), 'knob');
  r.window.close();
});

await ok('back_from_a_page_entered_from_the_list_is_the_list_and_the_back_button_too', async () => {
  const r = rig();
  await r.open();
  r.$('popup-sample-entry-two').click();
  await r.settle();
  r.$('popup-sample').querySelector('[data-popup-back]').click();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-list'));
  r.$('popup-sample-entry-two').click();
  await r.settle();
  await r.back();
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.deepEqual(r.shows, []);
  r.window.close();
});

await ok('the_veil_closes_it_only_when_the_press_and_the_release_are_both_on_it', async () => {
  const r = rig();
  await r.open();
  const el = r.$('popup-sample');
  const pane = el.querySelector('.fp-page');
  /** A pointer pressed on `from` and let go on `to`: the click is their common ancestor's. */
  const pressed = async (from, to) => {
    from.dispatchEvent(new r.window.Event('pointerdown', { bubbles: true }));
    to.dispatchEvent(new r.window.Event('pointerup', { bubbles: true }));
    (from === to ? to : el).click();
    await r.settle();
  };
  await pressed(pane, pane);
  assert.ok(r.$('popup-sample'), 'a click inside the box');
  // a drag that starts in the box and ends on the veil: the click is the veil's
  await pressed(pane, el);
  assert.ok(r.$('popup-sample'), 'pressed in the box, let go on the veil: it stands');
  await pressed(el, pane);
  assert.ok(r.$('popup-sample'), 'pressed on the veil, let go in the box: it stands');
  el.click();
  await r.settle();
  assert.ok(r.$('popup-sample'), 'a press is spent by its click: a click with no press closes nothing');
  await pressed(el, el);
  assert.equal(r.$('popup-sample'), null, 'pressed and let go on the veil');
  r.window.close();
});

await ok('an_open_ask_goes_with_a_step_to_another_page', async () => {
  const r = rig();
  await r.open();
  r.$(IDS.askOpen).click();
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(r.$('popup-sample').querySelector('.fi-askbox'), null);
  // Escape now closes the popup: no ask is left to take it
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null);
  r.window.close();
});

// ---- the lazy loading ----

await ok('a_redraw_over_an_open_ask_or_list_closes_it_and_the_first_escape_closes_the_popup', async () => {
  // the frame's own rule, whatever the wire does: `draw`, `loading` and `failed` replace the box.
  // The counts are over every page this harness made, so each is read against the one before.
  const asks = openAsks();
  const lists = openLists();
  for (const again of ['draw', 'failed', 'loading']) {
    const r = rig();
    await r.open();
    r.$(IDS.askOpen).focus();
    r.$(IDS.askOpen).click();
    assert.equal(openAsks(), asks + 1);
    if (again === 'draw') r.frame.draw({ entries: ENTRIES, page: null, body: '<p>drawn again</p>' });
    else r.frame[again](null);
    assert.equal(openAsks(), asks, `${again}: the ask went with the markup it stood in`);
    assert.equal(r.at(), 'popup-sample-entry-one', `${again}: the focus stays in the popup`);
    r.key('Escape');
    await r.settle();
    assert.equal(r.$('popup-sample'), null, `${again}: the first Escape closes the popup, no ghost eats it`);
    r.window.close();
  }
  const r = rig();
  await r.open();
  r.$(IDS.listOpen).focus();
  r.$(IDS.listOpen).click();
  assert.equal(openLists(), lists + 1);
  r.frame.draw({ entries: ENTRIES, page: null, body: '<p>drawn again</p>' });
  assert.equal(openLists(), lists, 'a list goes the same way');
  assert.equal(r.at(), 'popup-sample-entry-one');
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null);
  // a put leaves an ask in another place alone
  const q = rig();
  await q.open();
  q.$(IDS.askOpen).click();
  q.frame.put(IDS.live, 'a reading');
  assert.equal(openAsks(), asks + 1, 'a put is not a redraw of the box');
  q.key('Escape');
  await q.landed(() => q.key('Escape'));
  assert.equal(openAsks(), asks);
  r.window.close();
  q.window.close();
});

/** A loader the case resolves or rejects itself; `calls` counts how often it ran. */
function loader() {
  const hand = { calls: 0, resolve: null, reject: null };
  hand.load = () => {
    hand.calls += 1;
    return new Promise((resolve, reject) => {
      hand.resolve = () => resolve({ wire });
      hand.reject = () => reject(new Error('404'));
    });
  };
  return hand;
}

await ok('a_page_has_the_listed_popups_until_one_is_added_and_a_name_is_added_once', async () => {
  const r = rig({ sample: false });
  assert.deepEqual(r.core.popups.names(), ['devices', 'sound', HELP], 'the page\'s own list and no other');
  r.core.popups.add('sample', { title: TITLE, load: async () => ({ wire }) });
  assert.deepEqual(r.core.popups.names(), ['devices', 'sound', HELP, 'sample']);
  assert.throws(() => r.core.popups.add('sample', { title: TITLE, load: async () => ({ wire }) }),
    /sample is added already/);
  assert.throws(() => r.core.popups.add('other', { title: 'T' }), /loader/);
  r.window.close();
});

await ok('a_listed_popup_is_added_with_the_core_and_its_row_opens_it', async () => {
  let loads = 0;
  const popups = [
    {
      name: 'guide', title: 'Guide', row: { label: 'Guide', sees: 'everyone' },
      load: async () => { loads += 1; return { wire: () => ({ show() {} }) }; },
    },
    {
      name: 'gear', title: 'Gear', row: { label: 'Gear', sees: 'writer' },
      load: async () => ({ wire: () => ({ show() {} }) }),
    },
  ];
  const r = rig({ sample: false, popups });
  assert.deepEqual(r.core.popups.names(), ['guide', 'gear']);
  assert.deepEqual(r.core.popups.listed(), popups, 'what the menu reads its rows from');
  popups.push({ ...popups[0], name: 'late' });
  assert.deepEqual(r.core.popups.listed().map((popup) => popup.name), ['guide', 'gear'],
    'the list as the core was made with it: a later entry has no popup, so it gets no row');
  assert.throws(() => r.core.popups.listed().push(popups[0]), TypeError);
  assert.throws(() => { r.core.popups.listed()[1].row.sees = 'everyone'; }, TypeError,
    'who sees a popup cannot be changed through the list');
  assert.throws(() => { r.core.popups.listed()[1].row = { label: 'x', sees: 'everyone' }; }, TypeError);
  assert.equal(loads, 0, 'nothing is fetched as the core is made');
  const run = { popup: (name) => r.core.router.open(name) };
  const rows = (viewer) => frameRows({
    viewer, devices: [], shown: null, run, popups: r.core.popups.listed(),
  }).filter((def) => def.elementId.startsWith('menu-popup-'));
  // every row is a popup the router knows, for every kind of viewer
  for (const viewer of [{ guest: false, writes: true }, { guest: false, writes: false }, { guest: true }]) {
    for (const def of rows(viewer)) {
      assert.ok(r.core.popups.names().some((name) => rowId(name) === def.elementId), def.elementId);
    }
  }
  assert.deepEqual(rows({ guest: true }).map((def) => def.elementId), ['menu-popup-guide']);
  rows({ guest: false, writes: true })[0].run();
  await r.settle();
  assert.equal(r.core.router.state().popup, 'guide');
  assert.equal(r.$('popup-guide-title').textContent, 'Guide');
  assert.equal(loads, 1);
  r.window.close();
  assert.throws(() => rig({ sample: false, popups: [{ ...popups[0], row: undefined }] }), /guide.*row/,
    'a popup with no row is refused as the core is made');
});

await ok('a_viewer_who_does_not_see_a_popups_row_cannot_open_it_and_nothing_loads', async () => {
  let loads = 0;
  const made = () => [{
    name: 'gear', title: 'Gear', row: { label: 'Gear', sees: 'writer' },
    load: async () => { loads += 1; return { wire: () => ({ show() {} }) }; },
  }];
  const GUEST = { ...MODEL, viewer: { guest: true, writes: false } };
  // the toast is shown on the next frame, which jsdom's window has and node has not
  globalThis.requestAnimationFrame = (fn) => setTimeout(fn, 0);
  const r = rig({ sample: false, popups: made(), model: GUEST });
  const toasts = () => [...r.document.querySelectorAll('#toasts .toast')].map((one) => one.textContent);
  r.core.router.open('gear');
  await r.settle();
  assert.equal(r.core.router.state().popup, null, 'a guest opens no popup of a writer');
  assert.equal(r.$('popup-gear'), null);
  assert.equal(loads, 0, 'and its files are not asked for');
  assert.equal(r.window.history.length, 1);
  assert.ok(toasts().some((words) => words.startsWith('Sign in to change this')), toasts());
  r.window.close();
  // a page whose writes are off: the core's read-only words
  const off = rig({
    sample: false, popups: made(),
    model: { ...MODEL, viewer: { guest: false, writes: false, read_only: 'Read-only here' } },
  });
  off.core.router.open('gear');
  await off.settle();
  assert.equal(off.core.router.state().popup, null);
  assert.ok([...off.document.querySelectorAll('#toasts .toast')]
    .some((one) => one.textContent.startsWith('Read-only here')));
  assert.equal(loads, 0);
  off.window.close();
  // a writer opens it; when the page then comes back as a guest's, the popup closes
  const w = rig({ sample: false, popups: made() });
  w.$('knob').focus();
  w.core.router.open('gear');
  await w.settle();
  assert.equal(w.core.router.state().popup, 'gear');
  assert.equal(loads, 1);
  await w.landed(() => w.core.model.replace(structuredClone(GUEST)));
  assert.equal(w.core.router.state().popup, null, 'a viewer who lost the right loses the popup');
  assert.equal(w.$('popup-gear'), null);
  w.window.close();
});

await ok('a_viewer_who_loses_the_right_by_another_event_loses_the_popup_too', async () => {
  const made = () => [{
    name: 'gear', title: 'Gear', row: { label: 'Gear', sees: 'writer' },
    load: async () => ({ wire: () => ({ show() {} }) }),
  }];
  for (const [event, data] of [
    ['fleet', { devices: [] }], ['host', null], ['state', 'open'], ['opened', null],
  ]) {
    const w = rig({ sample: false, popups: made() });
    w.$('knob').focus();
    w.core.router.open('gear');
    await w.settle();
    assert.equal(w.core.router.state().popup, 'gear');
    // the rights are read from the page's model: this one changed in place and an event other
    // than the page being replaced is what the page hears next
    w.core.model.page().viewer = { guest: true, writes: false };
    w.core.model.emit(event, data);
    await w.settle();
    assert.equal(w.core.router.state().popup, null, `${event}: the popup of a writer is closed`);
    w.window.close();
  }
});

await ok('the_question_mark_is_listed_once_the_page_has_the_help_popup', () => {
  const none = rig({ sample: false, popups: [] });
  assert.deepEqual(none.core.keys.listed(none.core.keys.frame).map((entry) => entry.id), ['themes', 'play']);
  none.window.close();
  const help = rig({
    sample: false,
    popups: [{ name: HELP, title: 'Help', load: async () => ({}), row: { label: 'Help', sees: 'everyone' } }],
  });
  assert.deepEqual(help.core.keys.listed(help.core.keys.frame).map((entry) => entry.id),
    ['themes', 'play', HELP]);
  help.window.close();
  const other = rig({
    sample: false,
    popups: [{ name: 'guide', title: 'G', load: async () => ({}), row: { label: 'G', sees: 'everyone' } }],
  });
  assert.deepEqual(other.core.keys.listed(other.core.keys.frame).map((entry) => entry.id), ['themes', 'play'],
    'another popup is not Help');
  other.window.close();
});

await ok('the_page_as_shipped_adds_the_popups_of_its_list_and_no_other', () => {
  const r = rig({ sample: false });
  assert.deepEqual(r.core.popups.names(), POPUPS.map((popup) => popup.name));
  assert.ok(!POPUPS.some((popup) => popup.name === 'sample'), 'the sample stays unlisted');
  r.window.close();
});

await ok('the_frame_shows_at_once_with_the_loading_line_and_draws_when_the_module_arrives', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  assert.equal(hand.calls, 0, 'nothing is loaded before the popup is opened');
  await r.open();
  const el = r.$('popup-sample');
  assert.ok(el, 'the popup is on the stack before its module has arrived');
  assert.equal(el.querySelector('.fp-box').dataset.state, 'loading');
  assert.equal(el.querySelector('.fp-note').textContent, 'Loading…');
  assert.equal(r.$('popup-sample-title').textContent, 'Sample popup');
  assert.equal(r.at(), 'popup-sample-close', 'the focus is on the close button meanwhile');
  assert.equal(r.$('theme').inert, true);
  assert.deepEqual(r.core.stream.needed(), [], 'no want before the wire is there');
  hand.resolve();
  await r.settle();
  assert.equal(el.querySelector('.fp-box').dataset.state, 'ready');
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40');
  assert.equal(r.at(), 'popup-sample-entry-one', 'the focus goes where a drawn popup opens');
  assert.deepEqual(r.core.stream.needed(), ['meters']);
  r.window.close();
});

await ok('entries_given_as_the_popup_is_added_stand_beside_the_loading_line', async () => {
  const hand = loader();
  const r = rig({ sample: false });
  r.core.popups.add('sample', { title: TITLE, load: hand.load, entries: ENTRIES });
  await r.open();
  const box = r.$('popup-sample').querySelector('.fp-box');
  assert.equal(box.dataset.state, 'loading');
  assert.ok(box.classList.contains('fp-at-list'), 'the box has its two panes from the start');
  assert.deepEqual([...box.querySelectorAll('.fp-entry')].map((one) => one.textContent),
    ['Page one', 'Page two']);
  assert.equal(r.at(), 'popup-sample-entry-one', 'and the focus opens on the current entry');
  hand.resolve();
  await r.settle();
  assert.equal(r.at(), 'popup-sample-entry-one', 'where it stays, by id, when the module arrives');
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-list'));
  r.window.close();
});

await ok('the_loader_runs_once_for_two_opens', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  await r.open();
  hand.resolve();
  await r.settle();
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null);
  await r.open('two');
  assert.equal(hand.calls, 1);
  assert.equal(r.$('popup-sample').querySelector('.fp-box').dataset.state, 'ready',
    'the second open draws at once');
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-page'));
  r.window.close();
});

await ok('open_and_close_before_the_import_resolves_draws_nothing_and_holds_no_want', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  await r.open();
  r.key('Escape');
  await r.settle();
  assert.equal(r.$('popup-sample'), null, 'Escape closes a popup that is still loading');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.at(), 'knob');
  hand.resolve();
  await r.settle();
  assert.equal(r.$('popup-sample'), null, 'the module arriving opens nothing');
  assert.equal(r.frame.el.querySelector('.fp-box').dataset.state, 'loading', 'and draws nothing');
  assert.deepEqual(r.core.stream.needed(), [], 'no want is held');
  // the next open draws at once, from the module that arrived meanwhile
  await r.open();
  assert.equal(hand.calls, 1);
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40');
  r.window.close();
});

await ok('opened_again_before_the_import_resolves_it_is_drawn_once_at_the_page_of_then', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  await r.open();
  r.$('popup-sample-close').click();
  await r.settle();
  await r.open('two');
  assert.equal(hand.calls, 1, 'the import under way is the one waited for');
  hand.resolve();
  await r.settle();
  assert.ok(r.$('popup-sample').querySelector('.fp-box').classList.contains('fp-at-page'));
  assert.equal(r.$(IDS.name) !== null, true);
  r.window.close();
});

await ok('a_rejected_import_shows_a_line_and_escape_still_closes', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  await r.open();
  hand.reject();
  await r.settle();
  const el = r.$('popup-sample');
  assert.equal(el.querySelector('.fp-box').dataset.state, 'failed');
  assert.equal(el.querySelector('.fp-failed').getAttribute('role'), 'alert');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.equal(r.$('theme').inert, true, 'the stack is as it was');
  await r.landed(() => r.key('Escape'));
  assert.equal(r.$('popup-sample'), null);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.window.history.state.popup, null, 'the history is on the entry it was opened from');
  assert.equal(r.$('theme').inert, false);
  assert.equal(r.at(), 'knob');
  assert.deepEqual(r.shows, []);
  // the next open asks for the module again, and this time it comes
  await r.open();
  assert.equal(hand.calls, 2);
  assert.equal(r.$('popup-sample').querySelector('.fp-box').dataset.state, 'loading');
  hand.resolve();
  await r.settle();
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40');
  r.window.close();
});

await ok('a_rejected_import_and_back_still_closes', async () => {
  const hand = loader();
  const r = rig({ load: hand.load });
  await r.open();
  hand.reject();
  await r.settle();
  await r.back();
  assert.equal(r.$('popup-sample'), null);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.deepEqual(r.shows, []);
  r.window.close();
});

await ok('a_module_without_a_wire_and_a_wire_that_throws_are_the_failed_line_too', async () => {
  const r = rig({ load: async () => ({}) });
  await r.open();
  assert.equal(r.$('popup-sample').querySelector('.fp-box').dataset.state, 'failed');
  r.window.close();
  const thrower = rig({ load: async () => ({ wire: () => ({ show() { throw new Error('no'); } }) }) });
  await thrower.open();
  assert.equal(thrower.$('popup-sample').querySelector('.fp-box').dataset.state, 'failed');
  thrower.key('Escape');
  await thrower.settle();
  assert.equal(thrower.$('popup-sample'), null);
  thrower.window.close();
});

/** The sample's wire with a `show` that throws while `hand.bad` is set, after the real one has
 * run (so the want is taken); `hand.wires` counts how often the wire was made. */
function flaky() {
  const hand = { bad: false, wires: 0 };
  hand.load = async () => ({
    wire(given) {
      hand.wires += 1;
      const part = wire(given);
      return {
        ...part,
        show(to) {
          part.show(to);
          if (hand.bad) throw new Error('a draw that throws');
        },
      };
    },
  });
  return hand;
}

await ok('a_part_that_throws_on_a_later_open_shows_the_failed_line_and_closes_like_any_popup', async () => {
  const hand = flaky();
  const r = rig({ load: hand.load });
  await r.open();
  await r.landed(() => r.key('Escape'));
  hand.bad = true;
  r.$('knob').focus();
  assert.doesNotThrow(() => r.core.router.open('sample'), 'the open does not throw to its caller');
  await r.settle();
  const el = r.$('popup-sample');
  assert.ok(el, 'the popup is on the stack, as the history says');
  assert.equal(el.querySelector('.fp-box').dataset.state, 'failed');
  assert.equal(r.$('theme').inert, true);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'sample', page: null });
  assert.deepEqual(r.core.stream.needed(), [], 'what the part took before it threw is given back');
  await r.landed(() => r.key('Escape'));
  assert.equal(r.$('popup-sample'), null, 'Escape closes it');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.at(), 'knob');
  // a step that throws leaves the failed line too, and the popup still closes
  hand.bad = false;
  await r.open();
  hand.bad = true;
  r.$('popup-sample-entry-two').click();
  await r.settle();
  assert.equal(r.$('popup-sample').querySelector('.fp-box').dataset.state, 'failed');
  await r.landed(() => r.$('popup-sample-close').click());
  assert.equal(r.$('popup-sample'), null);
  // and it opens again once its draw holds
  hand.bad = false;
  await r.open();
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40');
  assert.equal(hand.wires, 1, 'the wire was made once');
  r.window.close();
});

await ok('a_part_that_throws_as_it_is_first_shown_is_hidden_and_kept', async () => {
  const hand = flaky();
  hand.bad = true;
  const r = rig({ load: hand.load });
  await r.open();
  assert.equal(r.$('popup-sample').querySelector('.fp-box').dataset.state, 'failed');
  assert.deepEqual(r.core.stream.needed(), [], 'its want is released at once: hide() ran');
  await r.landed(() => r.key('Escape'));
  assert.deepEqual(r.core.stream.needed(), []);
  hand.bad = false;
  await r.open();
  assert.equal(r.$(IDS.live).textContent, 'Bifrost: volume 40', 'the next open draws');
  assert.equal(hand.wires, 1, 'with the part it has: the wire is not made a second time');
  // one click listener, not two: a question's yes counts once
  r.$(IDS.askOpen).click();
  assert.equal(r.$('popup-sample').querySelectorAll('.fi-askbox').length, 1);
  r.$(IDS.ask).querySelector('[data-answer=save]').click();
  await r.settle();
  assert.equal(r.$(IDS.count).textContent, '1');
  r.window.close();
});

await ok('a_popup_without_a_loader_whose_show_throws_still_stands_and_closes', async () => {
  const r = rig({ sample: false });
  const el = r.document.createElement('div');
  el.innerHTML = '<button id="plain-x">x</button>';
  let bad = true;
  r.core.router.register('plain', { el, show() { if (bad) throw new Error('no'); } });
  r.$('knob').focus();
  assert.doesNotThrow(() => r.core.router.open('plain'));
  await r.settle();
  assert.equal(el.isConnected, true, 'it is on the stack, as the history says');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'plain', page: null });
  await r.landed(() => r.key('Escape'));
  assert.equal(el.isConnected, false);
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: null, page: null });
  bad = false;
  r.window.close();
});

await ok('a_draw_can_change_the_title_and_the_entries_and_the_loading_line_keeps_them', () => {
  const p = page('<div id="theme"></div>');
  const frame = createPopupFrame({
    document: p.document, router: { close() {}, step() {} },
    name: 'x', title: 'Presets', entries: TWO,
  });
  p.document.body.append(frame.el);
  const title = () => p.document.getElementById('popup-x-title').textContent;
  const labels = () => [...frame.el.querySelectorAll('.fp-entry')].map((one) => one.textContent);
  frame.draw({ entries: TWO, body: '' });
  assert.equal(title(), 'Presets', 'a draw that names no title keeps the one it was made with');
  frame.draw({ title: 'Presets for Bifrost', entries: [{ id: 'k', label: 'Kara', tag: 'Shown' }], body: '' });
  assert.equal(title(), 'Presets for Bifrost');
  assert.equal(frame.el.querySelector('.fp-side').getAttribute('aria-label'), 'Presets for Bifrost');
  assert.deepEqual(labels(), ['KaraShown']);
  frame.loading(null);
  assert.equal(title(), 'Presets for Bifrost', 'the loading line stands under the last title');
  assert.deepEqual(labels(), ['KaraShown']);
  frame.draw({ entries: TWO, body: '' });
  assert.equal(title(), 'Presets for Bifrost');
  frame.draw({ page: 'two', body: '<p>again</p>' });
  assert.deepEqual(labels(), ['One', 'Two <b>'], 'a draw that names no entries keeps the list');
  assert.equal(frame.titleWords('two'), 'Presets for Bifrost · Two <b>');
  frame.draw({ entries: [], body: '' });
  assert.deepEqual(labels(), [], 'an empty list is a list');
  p.window.close();
});

await ok('the_frame_alone_draws_and_puts_by_id', () => {
  const p = page('');
  const steps = [];
  const frame = createPopupFrame({
    document: p.document, name: 'x', title: 'T',
    router: { close: () => steps.push('close'), step: (to) => steps.push(to) },
  });
  p.document.body.append(frame.el);
  frame.draw({ entries: TWO, page: null, body: '<p id="spot">a</p>' });
  frame.put('spot', 'b');
  assert.equal(p.document.getElementById('spot').textContent, 'b');
  frame.put('nowhere', 'c');
  frame.loading();
  assert.equal(frame.el.querySelector('.fp-note').textContent, 'Loading…');
  assert.equal(frame.el.querySelectorAll('.fp-entry').length, 2, 'the list stays while it loads');
  frame.failed();
  assert.equal(frame.el.querySelector('.fp-failed').getAttribute('role'), 'alert');
  frame.el.querySelector('[data-popup-page=two]').click();
  frame.draw({ entries: TWO, page: 'two', body: '' });
  frame.el.querySelector('[data-popup-back]').click();
  frame.el.querySelector('[data-popup-close]').click();
  assert.deepEqual(steps, ['two', null, 'close']);
  assert.equal(frame.labelledBy, 'popup-x-title');
  p.window.close();
});

console.log(`core-popups.mjs: ${n} passed`);
