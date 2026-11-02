/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/prefs.js and core/scheme.js: a theme's preferences under its own key, the
// storage that refuses, the theme choice written to storage and cookie together and read back,
// and light and dark following the system until a person says otherwise. Storage, cookie and the
// system's setting are fakes. Run: node core-prefs.mjs
import assert from 'node:assert/strict';
import {
  CHOICE_KEY, COOKIE, bootScheme, createPrefs, guarded, readChoice, writeChoice,
} from '../../../bins/svolderd/web/core/prefs.js';
import { chromeOf, createScheme, paintChrome } from '../../../bins/svolderd/web/core/scheme.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const memory = () => {
  const map = new Map();
  return {
    map,
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, text) => { map.set(key, String(text)); },
    removeItem: (key) => { map.delete(key); },
  };
};
const refusing = () => ({
  getItem() { throw new Error('blocked'); },
  setItem() { throw new Error('blocked'); },
  removeItem() { throw new Error('blocked'); },
});

ok('prefs_live_under_the_themes_key', () => {
  assert.equal(CHOICE_KEY, 'svolder.theme');
  assert.equal(COOKIE, 'svolder_theme');
  const storage = memory();
  const prefs = createPrefs('paper', storage);
  assert.equal(prefs.get('lines', 3), 3, 'the fallback until it is set');
  assert.equal(prefs.set('lines', 5), true);
  assert.equal(storage.map.get('svolder.theme.paper.lines'), '5');
  assert.equal(prefs.get('lines', 3), 5);
  prefs.set('look', { a: [1, 2], on: true });
  assert.deepEqual(prefs.get('look'), { a: [1, 2], on: true });
  assert.equal(createPrefs('ember', storage).get('lines', 'none'), 'none', 'another theme\'s own');
  prefs.remove('lines');
  assert.equal(storage.map.has('svolder.theme.paper.lines'), false);
  storage.map.set('svolder.theme.paper.broken', '{not json');
  assert.equal(prefs.get('broken', 'fine'), 'fine', 'text that is not JSON reads as the fallback');
});

ok('storage_that_throws_reads_null_and_writes_false', () => {
  const shut = guarded(refusing());
  assert.equal(shut.read('k'), null);
  assert.equal(shut.write('k', 'v'), false);
  assert.doesNotThrow(() => shut.remove('k'));
  const none = guarded(undefined);
  assert.equal(none.read('k'), null);
  assert.equal(none.write('k', 'v'), false);
  const open = guarded(memory());
  assert.equal(open.write('k', 'v'), true);
  assert.equal(open.read('k'), 'v');
  open.remove('k');
  assert.equal(open.read('k'), null);
  const prefs = createPrefs('ember', refusing());
  assert.equal(prefs.get('x', 7), 7);
  assert.equal(prefs.set('x', 1), false);
  assert.doesNotThrow(() => prefs.remove('x'));
  assert.equal(readChoice({ storage: refusing(), cookie: '' }), null);
  assert.doesNotThrow(() => writeChoice({ id: 'ember', scheme: null },
    { storage: refusing(), document: { cookie: '' }, path: '/' }));
});

ok('the_choice_and_the_cookie_are_written_together', () => {
  const storage = memory();
  const document = { cookie: '' };
  writeChoice({ id: 'ember', scheme: null }, { storage, document, path: '/svolder/' });
  assert.equal(storage.map.get('svolder.theme'), 'ember');
  assert.equal(document.cookie,
    'svolder_theme=ember; Path=/svolder/; Max-Age=31536000; SameSite=Lax');
  writeChoice({ id: 'paper', scheme: 'dark' }, { storage, document, path: '/' });
  assert.equal(storage.map.get('svolder.theme'), 'paper.dark');
  assert.ok(document.cookie.startsWith('svolder_theme=paper.dark; Path=/;'), document.cookie);
  assert.deepEqual(readChoice({ storage, cookie: '' }), { id: 'paper', scheme: 'dark' });
  assert.deepEqual(readChoice({ storage: memory(), cookie: 'a=b; svolder_theme=paper.light; c=d' }),
    { id: 'paper', scheme: 'light' });
  assert.deepEqual(readChoice({ storage: memory(), cookie: 'svolder_theme=ember' }),
    { id: 'ember', scheme: null });
  assert.equal(readChoice({ storage: memory(), cookie: 'a=b' }), null);
  assert.equal(readChoice({ storage: memory(), cookie: 'svolder_theme=<b>' }), null);
  // What the page was rendered with wins over what storage holds.
  assert.deepEqual(readChoice({ storage, cookie: 'svolder_theme=ember' }),
    { id: 'ember', scheme: null });
});

// The system's colour setting as a browser gives it: `matches` and a change listener.
function system(dark) {
  const mql = {
    matches: dark,
    listeners: new Set(),
    addEventListener(_, fn) { mql.listeners.add(fn); },
    removeEventListener(_, fn) { mql.listeners.delete(fn); },
    flip(next) { mql.matches = next; for (const fn of [...mql.listeners]) fn({ matches: next }); },
  };
  return mql;
}
function schemeRig({ dark = false, schemes = ['light', 'dark'], stored } = {}) {
  const mql = system(dark);
  const storage = memory();
  if (stored) storage.map.set('svolder.theme.probe.scheme', JSON.stringify(stored));
  const root = { dataset: {} };
  const cookies = [];
  const scheme = createScheme({
    matchMedia: (query) => {
      assert.equal(query, '(prefers-color-scheme: dark)');
      return mql;
    },
    root,
    prefs: createPrefs('probe', storage),
    schemes,
    onCookie: (value) => cookies.push(value),
  });
  return { mql, root, scheme, cookies, storage };
}

ok('the_scheme_follows_the_system_until_overridden', () => {
  const { mql, root, scheme, cookies, storage } = schemeRig({ dark: false });
  assert.equal(scheme.mode(), 'auto');
  assert.equal(scheme.effective(), 'light');
  assert.equal(root.dataset.scheme, 'light');
  const heard = [];
  const off = scheme.on((effective) => heard.push(effective));
  mql.flip(true);
  assert.equal(scheme.effective(), 'dark');
  assert.equal(root.dataset.scheme, 'dark');
  assert.deepEqual(heard, ['dark']);
  scheme.set('light');
  assert.equal(scheme.mode(), 'light');
  assert.equal(scheme.effective(), 'light');
  assert.equal(root.dataset.scheme, 'light');
  mql.flip(false);
  mql.flip(true);
  assert.equal(scheme.effective(), 'light', 'a person\'s choice holds against the system');
  assert.deepEqual(cookies, ['light']);
  assert.equal(storage.map.get('svolder.theme.probe.scheme'), '"light"');
  scheme.set('auto');
  assert.equal(scheme.effective(), 'dark');
  assert.deepEqual(cookies, ['light', null], 'auto clears the cookie\'s scheme');
  scheme.set('nonsense');
  assert.equal(scheme.mode(), 'auto', 'a mode that is not one changes nothing');
  off();
  const count = heard.length;
  mql.flip(false);
  assert.equal(scheme.effective(), 'light');
  assert.equal(heard.length, count, 'not heard once it left');
  scheme.stop();
  assert.equal(mql.listeners.size, 0);
  mql.flip(true);
  assert.equal(scheme.effective(), 'dark', 'effective still reads the system');
});

ok('a_kept_choice_is_the_mode_at_boot', () => {
  const { scheme, root } = schemeRig({ dark: false, stored: 'dark' });
  assert.equal(scheme.mode(), 'dark');
  assert.equal(scheme.effective(), 'dark');
  assert.equal(root.dataset.scheme, 'dark');
  const odd = schemeRig({ stored: 'purple' });
  assert.equal(odd.scheme.mode(), 'auto');
});

ok('a_theme_with_one_scheme_ignores_the_system', () => {
  const { mql, scheme, root, cookies } = schemeRig({ dark: false, schemes: ['dark'] });
  assert.equal(scheme.effective(), 'dark');
  assert.equal(root.dataset.scheme, undefined, 'the page names no scheme for a theme with one');
  mql.flip(true);
  mql.flip(false);
  assert.equal(scheme.effective(), 'dark');
  scheme.set('light');
  assert.equal(scheme.effective(), 'dark');
  assert.deepEqual(cookies, [], 'there is nothing to remember');
  const light = schemeRig({ dark: true, schemes: ['light'] });
  assert.equal(light.scheme.effective(), 'light');
});

ok('the_browsers_colours_follow_the_scheme', () => {
  const entry = { color_scheme: 'light dark', theme_color: '#f3efe6', theme_color_dark: '#1e1f22' };
  assert.deepEqual(chromeOf(entry, 'dark', 'auto'), { colorScheme: 'light dark', themeColor: '#1e1f22' });
  assert.deepEqual(chromeOf(entry, 'dark', 'dark'), { colorScheme: 'dark', themeColor: '#1e1f22' });
  assert.deepEqual(chromeOf(entry, 'light', 'light'), { colorScheme: 'light', themeColor: '#f3efe6' });
  assert.deepEqual(chromeOf(entry, 'light', 'auto'), { colorScheme: 'light dark', themeColor: '#f3efe6' });
  const one = { color_scheme: 'dark', theme_color: '#0f0d0b', theme_color_dark: null };
  assert.equal(chromeOf(one, 'dark', 'auto').themeColor, '#0f0d0b', 'no dark colour: the one colour');
});

ok('the_painted_scheme_is_the_choice_at_boot', () => {
  assert.equal(bootScheme('dark'), 'dark');
  assert.equal(bootScheme('light'), 'light');
  assert.equal(bootScheme(null), 'auto');
});

ok('a_mode_change_that_keeps_the_scheme_repaints_the_chrome', () => {
  const { scheme } = schemeRig({ dark: true });
  const entry = { color_scheme: 'light dark', theme_color: '#f3efe6', theme_color_dark: '#1e1f22' };
  const metas = [{ name: 'color-scheme', content: 'light dark' }];
  const document = {
    querySelector: () => ({ setAttribute: (_, v) => { metas[0].content = v; } }),
    querySelectorAll: () => [],
  };
  const themes = [];
  scheme.on((effective) => themes.push(effective));
  scheme.on((effective, mode) => paintChrome(document, entry, effective, mode), { withMode: true });
  scheme.set('dark');
  assert.equal(metas[0].content, 'dark');
  scheme.set('auto');
  assert.equal(metas[0].content, 'light dark', 'dark to auto under a dark system');
  assert.deepEqual(themes, [], 'the theme is not told: its scheme did not change');
});

console.log(`${n} core-prefs checks passed`);
