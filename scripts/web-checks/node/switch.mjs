/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/switch.js, the theme on show and the way to another: whichever theme
// ends up mounted (the fallback included) has its style sheets, meta tags and scripts in place
// first, a sheet that never answers times out and releases the switch, and a page where not even
// Ember starts says so in a popup that stays. A small fake document, head and storage stand in for
// the browser's; a policy says what each file added to the head does (loads, fails, never
// answers). Run: node switch.mjs
import assert from 'node:assert/strict';
import { bootScheme } from '../../../bins/svolderd/web/core/prefs.js';
import { createRegistry } from '../../../bins/svolderd/web/core/registry.js';
import { createSwitcher } from '../../../bins/svolderd/web/core/switch.js';

let n = 0;
const warned = [];
console.warn = (...args) => warned.push(args);
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const tick = (ms = 5) => new Promise((resolve) => setTimeout(resolve, ms));

/** A node: attributes, a dataset, listeners and a parent. */
function node(tag, doc) {
  const el = {
    tag, attrs: {}, dataset: {}, listeners: {}, parent: null,
    setAttribute(name, value) { el.attrs[name] = String(value); },
    getAttribute(name) { return name in el.attrs ? el.attrs[name] : null; },
    addEventListener(type, fn) { (el.listeners[type] ??= []).push(fn); },
    remove() { if (el.parent) el.parent.children = el.parent.children.filter((c) => c !== el); el.parent = null; },
    fire(type) { for (const fn of el.listeners[type] ?? []) fn(); },
    children: [],
    append(...kids) {
      for (const kid of kids) {
        kid.parent = el;
        el.children.push(kid);
        if (el === doc.head) doc.policy(kid);
      }
    },
  };
  for (const name of ['href', 'src', 'rel']) {
    Object.defineProperty(el, name, { get: () => el.attrs[name], set: (v) => { el.attrs[name] = String(v); } });
  }
  return el;
}

/** A page with a head holding `rendered`, and `policy(el)` deciding what a file put in the head
 * does: 'load', 'error' or 'never'. */
function page({ rendered = [], search = '' } = {}) {
  const doc = {
    cookie: '',
    policy: (el) => {
      const how = doc.how(el);
      if (how === 'never') return;
      setTimeout(() => el.fire(how), 0);
    },
    how: () => 'load',
    documentElement: { dataset: {} },
    createElement: (tag) => node(tag, doc),
    getElementById: () => null,
    querySelector: () => null,
    querySelectorAll(selector) {
      const [tag] = selector.split('[');
      return doc.head.children.filter((el) => el.tag === tag);
    },
  };
  doc.head = node('head', doc);
  for (const [tag, attrs, sheet] of rendered) {
    const el = node(tag, doc);
    Object.assign(el.attrs, attrs);
    if (sheet) el.dataset.sheet = sheet;
    doc.head.children.push(el);
    el.parent = doc.head;
  }
  const store = new Map();
  const window = {
    document: doc,
    location: { search, pathname: '/', href: 'http://x/' },
    localStorage: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) },
  };
  return { doc, window, store };
}

const EMBER = {
  id: 'ember', label: 'Ember', blurb: 'warm', module: 'themes/ember/ember.js', schemes: ['dark'],
  styles: ['themes/ember/style.css'], scripts: ['themes/ember/htmx.min.js', 'themes/ember/viz-pref.js'],
  metas: [{ name: 'htmx-config', content: '{}' }],
};
const PAPER = {
  id: 'paper', label: 'Paper', blurb: 'light', module: 'themes/paper/paper.js', schemes: ['light', 'dark'],
  styles: ['themes/paper/style.css'], scripts: [], metas: [],
};

/** The rig: a page that was painted for `first` (its own files in the head), a registry with both
 * themes, a core of fakes, and the switcher. */
function rig({
  first = 'paper', mounts = {}, search = '', timeoutMs = 30, painted = null, cookie = '', kept = {},
} = {}) {
  const own = first === 'paper' ? PAPER : EMBER;
  const rendered = [['link', { href: `assets/${own.styles[0]}`, rel: 'stylesheet' }, own.id]];
  for (const m of own.metas) rendered.push(['meta', { name: m.name, content: m.content }, own.id]);
  for (const s of own.scripts) rendered.push(['script', { src: `assets/${s}` }, null]);
  const { doc, window, store } = page({ rendered, search });
  doc.cookie = cookie;
  for (const [key, value] of Object.entries(kept)) store.set(key, value);
  const core = {
    theme: null, scheme: null,
    frame: {
      toasts: [],
      toast: (text, action, options) => core.frame.toasts.push({ text, action, options }),
      closeMenu() {}, closePicker() {}, fade() {}, anchor() {},
    },
    stream: { opened: 0, open() { core.stream.opened++; } },
  };
  const log = { mounted: [], unmounted: [] };
  const theme = (list) => ({
    id: list.id, label: list.label, needs: [], schemes: list.schemes,
    mount: mounts[list.id] ?? (() => {
      log.mounted.push(list.id);
      return { unmount() { log.unmounted.push(list.id); } };
    }),
  });
  // What the core's bind does: the scheme of the theme mounted, the page's painted one the choice
  // at the first bind.
  let bound = false;
  core.registry = createRegistry([EMBER, PAPER], {
    beforeMount: (t) => {
      core.theme = t;
      const key = `svolder.theme.${t.id}.scheme`;
      if (!bound && t.schemes.length > 1) store.set(key, JSON.stringify(bootScheme(painted)));
      bound = true;
      const mode = () => JSON.parse(store.get(key) ?? '"auto"');
      core.scheme = { mode, effective: () => (mode() === 'auto' ? t.schemes[0] : mode()) };
    },
  });
  core.registry.add(theme(EMBER));
  core.registry.add(theme(PAPER));
  const root = { replaceChildren() { root.empty = true; }, empty: false };
  const switcher = createSwitcher({ core, window, root, timeoutMs });
  const files = () => doc.head.children.map((el) => el.attrs.href ?? el.attrs.src ?? `meta:${el.attrs.name}`);
  return { core, doc, window, store, switcher, log, root, files, toasts: core.frame.toasts };
}

const EMBER_FILES = ['assets/themes/ember/style.css', 'assets/themes/ember/htmx.min.js', 'assets/themes/ember/viz-pref.js'];

await ok('a_first_paint_that_falls_back_loads_embers_sheet_metas_and_scripts', async () => {
  const r = rig({ first: 'paper', mounts: { paper: () => { throw new Error('no canvas'); } } });
  const mounted = await r.switcher.start('paper');
  assert.equal(mounted.id, 'ember');
  assert.deepEqual(r.toasts.map((t) => t.text), ['Paper could not start here; showing Ember']);
  const files = r.files();
  for (const wanted of [...EMBER_FILES, 'meta:htmx-config']) assert.ok(files.includes(wanted), `${wanted} in ${files}`);
  assert.ok(files.indexOf('meta:htmx-config') < files.indexOf('assets/themes/ember/htmx.min.js'), 'the meta first');
  assert.ok(files.indexOf('assets/themes/ember/htmx.min.js') < files.indexOf('assets/themes/ember/viz-pref.js'), 'scripts in order');
  assert.ok(!files.includes('assets/themes/paper/style.css'), 'the failed theme\'s sheet is out');
  assert.equal(r.doc.documentElement.dataset.theme, 'ember');
  assert.match(r.doc.cookie, /svolder_theme=ember/);
});

await ok('a_pick_of_ember_from_a_paper_first_paint_loads_embers_files_and_drops_papers_sheet', async () => {
  const r = rig({ first: 'paper' });
  await r.switcher.start('paper');
  assert.deepEqual(r.files(), ['assets/themes/paper/style.css']);
  await r.switcher.to('ember');
  const files = r.files();
  assert.deepEqual(files.filter((f) => !f.startsWith('meta:')).sort(), [...EMBER_FILES].sort());
  assert.ok(files.includes('meta:htmx-config'));
  assert.deepEqual(r.log.mounted, ['paper', 'ember']);
  assert.deepEqual(r.log.unmounted, ['paper']);
  assert.equal(r.switcher.current().id, 'ember');
  assert.equal(r.doc.documentElement.dataset.theme, 'ember');
});

await ok('files_the_page_already_has_are_not_added_again', async () => {
  const r = rig({ first: 'ember' });
  await r.switcher.start('ember');
  assert.equal(r.files().length, 4, 'a first paint of Ember adds nothing');
  await r.switcher.to('paper');
  await r.switcher.to('ember');
  assert.equal(r.files().filter((f) => f.endsWith('htmx.min.js')).length, 1, 'a script stays once');
});

await ok('a_sheet_that_never_answers_times_out_and_the_switch_is_released', async () => {
  const r = rig({ first: 'paper' });
  await r.switcher.start('paper');
  r.doc.how = (el) => (el.attrs.href?.includes('ember') ? 'never' : 'load');
  await r.switcher.to('ember');
  assert.equal(r.switcher.current().id, 'paper', 'the theme on show stays');
  assert.deepEqual(r.toasts.map((t) => t.text), ['Ember could not start here; showing Paper']);
  assert.deepEqual(r.files(), ['assets/themes/paper/style.css'], 'what was added is taken out');
  assert.deepEqual(r.log.unmounted, [], 'nothing was unmounted');
  r.doc.how = () => 'load';
  await r.switcher.to('ember');
  assert.equal(r.switcher.current().id, 'ember', 'the switch was released');
  assert.match(r.doc.cookie, /svolder_theme=ember/);
});

await ok('a_script_that_fails_leaves_the_theme_on_show', async () => {
  const r = rig({ first: 'paper' });
  await r.switcher.start('paper');
  r.doc.how = (el) => (el.attrs.src?.includes('htmx') ? 'error' : 'load');
  await r.switcher.to('ember');
  assert.equal(r.switcher.current().id, 'paper');
  assert.deepEqual(r.files(), ['assets/themes/paper/style.css']);
});

await ok('the_theme_and_the_fallback_both_failing_leaves_a_popup_that_stays_not_a_blank_page', async () => {
  const boom = () => { throw new Error('no'); };
  const r = rig({ first: 'paper', mounts: { paper: boom, ember: boom } });
  const mounted = await r.switcher.start('paper');
  assert.equal(mounted, null);
  assert.equal(r.root.empty, true);
  const last = r.toasts.at(-1);
  assert.equal(last.text, 'svolder could not draw the page.');
  assert.equal(last.options.ms, 0, 'it stays');
  assert.equal(last.action.label, 'Reload');
  assert.equal(r.switcher.current(), null);
});

await ok('a_pick_whose_theme_and_fallback_both_fail_says_so_and_the_switch_is_released', async () => {
  const r = rig({ first: 'paper' });
  await r.switcher.start('paper');
  r.core.registry.add({
    id: 'ember', label: 'Ember', needs: [], schemes: ['dark'],
    mount() { throw new Error('no'); },
  });
  await r.switcher.to('ember');
  assert.equal(r.switcher.current(), null);
  assert.equal(r.toasts.at(-1).text, 'svolder could not draw the page.');
  assert.equal(r.root.empty, true);
  // released: another pick runs (nothing is mounted, so it waits for a mount that is not there)
  await r.switcher.to('paper');
});

await ok('a_pick_before_the_first_mount_does_nothing', async () => {
  const r = rig({ first: 'paper' });
  await r.switcher.to('ember');
  assert.equal(r.switcher.current(), null);
  assert.deepEqual(r.toasts, []);
  assert.deepEqual(r.log.mounted, []);
});

await ok('an_empty_theme_query_is_ignored_and_a_wrong_one_is_told', async () => {
  const empty = rig({ first: 'ember', search: '?theme=' });
  await empty.switcher.start('ember');
  assert.deepEqual(empty.toasts, []);
  const wrong = rig({ first: 'ember', search: '?theme=nope' });
  await wrong.switcher.start('ember');
  assert.deepEqual(wrong.toasts.map((t) => t.text), ['There is no theme “nope”.']);
});

await ok('a_switch_keeps_the_new_themes_own_light_or_dark', async () => {
  const r = rig({ first: 'ember', kept: { 'svolder.theme.paper.scheme': '"dark"' } });
  await r.switcher.start('ember');
  assert.match(r.doc.cookie, /svolder_theme=ember(;|$)/);
  const cookies = [];
  let jar = '';
  Object.defineProperty(r.doc, 'cookie', { get: () => jar, set: (v) => { jar = v; cookies.push(v); } });
  await r.switcher.to('paper');
  assert.match(cookies[0], /svolder_theme=paper\.dark;/, 'the write before the mount has the new theme\'s own scheme');
  assert.equal(r.switcher.current().id, 'paper');
  assert.match(r.doc.cookie, /svolder_theme=paper\.dark(;|$)/);
  assert.equal(r.store.get('svolder.theme'), 'paper.dark');
  await r.switcher.to('ember');
  assert.match(r.doc.cookie, /svolder_theme=ember(;|$)/, 'Ember has one scheme and writes none');
  assert.equal(r.store.get('svolder.theme'), 'ember');
});

await ok('the_first_mount_repairs_a_choice_that_lost_its_scheme', async () => {
  const r = rig({ first: 'paper', painted: 'dark', cookie: 'svolder_theme=paper' });
  const mounted = await r.switcher.start('paper');
  assert.equal(mounted.id, 'paper');
  assert.equal(r.core.scheme.mode(), 'dark');
  assert.match(r.doc.cookie, /svolder_theme=paper\.dark(;|$)/);
  assert.equal(r.store.get('svolder.theme'), 'paper.dark');
});

await ok('a_fallback_to_ember_writes_ember_alone', async () => {
  const r = rig({ first: 'paper', painted: 'dark', mounts: { paper: () => { throw new Error('x'); } } });
  const mounted = await r.switcher.start('paper');
  assert.equal(mounted.id, 'ember');
  assert.match(r.doc.cookie, /svolder_theme=ember(;|$)/);
  assert.equal(r.store.get('svolder.theme'), 'ember');
});

console.log(`${n} switch checks passed`);
