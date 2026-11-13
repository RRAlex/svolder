/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The key table: one list per theme of every key the theme answers to, which the page's global
// keys are bound from and the Help popup is drawn from, so a key that is listed is bound and a
// global key that is bound is listed.
//
// An entry is `{ id, key, zone, words, when?(ctx), run?(event), own?, keys?, repeat?, gesture? }`:
//   id      the entry's name in its table, which no other entry of the table has
//   key     what a person reads on the key: one keycap's name (`'m'`, `'Home'`, `'Page Up'`), or
//           a list of keycaps for an entry several keys share (`['←', '→']`, `['Home', 'End']`).
//           One string is one keycap, never several: a name holds no space, but for `Page Up`
//           and `Page Down`
//   zone    `'page'`, or a selector naming the widget the key belongs to (never `body`, `html`,
//           `:root` or `*`: that is the page)
//   words   what the key does, in the theme's own words (sentence case, no full stop)
//   when    whether the entry applies now, from `ctx` (the viewer's rights, what the page
//           shows); without one it always does
//   run     with one, the entry is a global key of the page (its zone is `'page'`): `bind`
//           calls it with the event. Without one, the entry documents a key its widget handles
//           itself
//   own     true for a key that a listener of the theme's own on the window or the document
//           answers, because its rule is not `bind`'s (it acts from a field, or in the capture
//           phase): `bind` does not dispatch it. A key of the page has a `run` or is `own`
//   keys    the `event.key` values the entry answers to, when they are not its keycaps
//           (`['ArrowLeft', 'ArrowRight']` for `['←', '→']`, `['A', 'a']` for `'A'`), one at
//           least
//   repeat  true for a key that acts again while it is held; any other key acts once a press
//   gesture true for an entry that is no key: `key` is then the name of what the pointer does
//           (`'Curve'`, any words), drawn as plain text, and the entry has no `run` and no
//           `keys`, of the page's zone too
// A theme may add fields of its own to an entry; the core reads only those above.
//
//   listed(table, ctx)   the entries that apply now, the very objects, in table order: what the
//                        Help popup draws
//   zonesOf(table, el)   every zone that holds `el` (its selector matches `el` or an ancestor),
//                        the outermost first: the widgets whose keys act where `el` is
//   zoneOf(table, el)    the innermost of them, else `'page'`
//   capsOf(entry)        the entry's keycaps, as a list
//   keysOf(entry)        the `event.key` values an entry answers to
// and, on the core (`core.keys`, from `createKeys`):
//   frame                the frame's own keys, `FRAME_KEYS`: a table like a theme's, never bound
//                        (the frame's listener answers them), for whoever lists them
//   bind(table, { signal, ctx })   bind the table's `run` entries until `signal` aborts (a
//                        theme passes its mount's). `ctx()` gives the theme's own facts, laid
//                        over the core's (`{ canWrite }`); it is asked at each key
//   listed(table, ctx)   as above; without `ctx`, with the facts the table was bound with
//
// A bound key reaches its `run` only when nothing else owns it: the frame's listener comes first
// (capture phase, on the window) and stops every key while a modal is open; a key with Ctrl, Alt
// or Meta, a key in a field that takes typing (an input, a text area, a select, an editable
// element) and a key the menu owns (`menuOwns`) are left alone; and a widget that stops the key
// it takes keeps it. The first entry that names the key and applies takes it. A table that binds
// a key the frame reserves (`RESERVED`), holds an entry the core cannot read, or gives a `run`
// to a key that could never fire (a keycap's name that is no `event.key`, with no `keys`), is
// refused when it is bound, with an error that names the entry.
//
// `listed`, `zonesOf`, `zoneOf`, `capsOf`, `keysOf`, `FRAME_KEYS`, `RESERVED` and `HELP` (the id
// of the frame's `?` entry, which is the Help popup's name) are pure: a theme may import them.

import { HELP } from './popups/listed.js';

export { HELP };

/** What a key typed into is not the page's: it types. */
export const TYPING = 'input, textarea, select, [contenteditable]';

/** The `event.key` values the frame keeps for itself, which no table may bind: the picker's `T`,
 * the `?` of the Help popup and Space, which plays and pauses. */
export const RESERVED = ['T', 't', '?', ' '];

/** The frame's own keys as a table, for a theme that lists them beside its own. `?` applies on
 * a page that has a Help popup for it to open (`ctx.help`). */
export const FRAME_KEYS = [
  { id: 'themes', key: 'T', zone: 'page', words: 'Themes' },
  {
    id: 'play', key: 'Space', zone: 'page', words: 'Play or pause',
    when: (ctx) => ctx.canWrite === true,
  },
  {
    id: HELP, key: '?', zone: 'page', words: 'Help',
    when: (ctx) => ctx.help === true,
  },
];

/** What a key press can be called (`event.key`): one character that is no drawn arrow, or the
 * browser's name for a key that types nothing. */
const NAMED = ['Enter', 'Escape', 'Tab', 'Backspace', 'Delete', 'Insert', 'Home', 'End', 'PageUp',
  'PageDown', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
const pressed = (key) => /^[^←→↑↓]$/u.test(key) || NAMED.includes(key) || /^F\d{1,2}$/.test(key);

/** The keycaps whose name is two words; any other string with a space in it is several keycaps
 * packed into one, which `bind` refuses. */
const TWO_WORD_CAPS = ['Page Up', 'Page Down'];

/** The selectors that are the whole page, which no widget's zone may be. */
const WHOLE_PAGE = ['body', 'html', ':root', '*'];

/** The keycaps of `entry`, one name each: what a person reads. */
export const capsOf = (entry) => [].concat(entry.key);

/** The `event.key` values `entry` answers to. */
export const keysOf = (entry) => entry.keys ?? capsOf(entry);

/** Whether `entry` applies with the facts `ctx`; a `when` that throws is said and does not. */
function applies(entry, ctx) {
  if (!entry.when) return true;
  try {
    return Boolean(entry.when(ctx));
  } catch (error) {
    console.warn(`svolder: the key table's when of ${entry.id} threw`, error);
    return false;
  }
}

/** The entries of `table` that apply with the facts `ctx`, in table order. */
export function listed(table, ctx = {}) {
  return table.filter((entry) => applies(entry, ctx));
}

/**
 * Every zone of `table` that holds `element`, the outermost first: a scale in a sheet is in the
 * sheet's zone and in its own. Each zone once; none for an element outside every widget. Two
 * zones on one element stand as the table has them, the first the inner one.
 */
export function zonesOf(table, element) {
  if (!element?.closest) return [];
  const depth = (el) => {
    let n = 0;
    for (let at = el; at.parentElement; at = at.parentElement) n++;
    return n;
  };
  const zones = [...new Set(table.map((entry) => entry.zone))].filter((zone) => zone !== 'page');
  return zones.map((zone, at) => ({ zone, at, held: element.closest(zone) }))
    .filter((one) => one.held)
    .map((one) => ({ ...one, depth: depth(one.held) }))
    .sort((a, b) => a.depth - b.depth || b.at - a.at)
    .map((one) => one.zone);
}

/** The innermost zone of `table` that `element` falls in, else `'page'`. */
export function zoneOf(table, element) {
  return zonesOf(table, element).at(-1) ?? 'page';
}

/** Throw unless `table` is one the core can read and bind. */
function check(table) {
  if (!Array.isArray(table)) throw new Error('svolder: a key table is an array of entries');
  const text = (value) => typeof value === 'string' && value !== '';
  // several keycaps in one string: a keycap's name holds no space, but for the few of two words
  const packed = (cap) => cap.length > 1 && /\s/.test(cap) && !TWO_WORD_CAPS.includes(cap);
  const ids = new Set();
  table.forEach((entry, at) => {
    const bad = (why) => new Error(`svolder: key table entry ${at} (${entry?.id ?? '?'}): ${why}`);
    if (!text(entry?.id)) throw bad('`id` is the entry\'s name, a string');
    if (ids.has(entry.id)) throw bad('`id` is the entry\'s own: another entry has it');
    ids.add(entry.id);
    const caps = Array.isArray(entry.key) ? entry.key : [entry.key];
    if (caps.length === 0 || !caps.every(text)) {
      throw bad('`key` is a keycap\'s name or a list of them');
    }
    if (entry.gesture !== undefined && typeof entry.gesture !== 'boolean') {
      throw bad('`gesture` is true for an entry that is no key');
    }
    if (entry.gesture && (entry.run !== undefined || entry.keys !== undefined)) {
      throw bad('a `gesture` is no key: it has no `run` and no `keys`');
    }
    if (!entry.gesture && caps.some(packed)) {
      throw bad('`key` holds one keycap in a string; several keycaps are a list');
    }
    if (!text(entry.zone)) throw bad('`zone` is \'page\' or a selector');
    if (entry.zone !== entry.zone.trim() || WHOLE_PAGE.includes(entry.zone)) {
      throw bad('`zone` is \'page\' or a selector that names a widget: not the whole page under '
        + 'another name, and no space around it');
    }
    if (!text(entry.words)) throw bad('`words` say what the key does');
    if (entry.when !== undefined && typeof entry.when !== 'function') {
      throw bad('`when` is a function of the ctx');
    }
    if (entry.keys !== undefined
      && !(Array.isArray(entry.keys) && entry.keys.length > 0 && entry.keys.every(text))) {
      throw bad('`keys` is a list of event.key values, one at least');
    }
    if (entry.own !== undefined && typeof entry.own !== 'boolean') {
      throw bad('`own` is true for a key the theme\'s own listener answers');
    }
    if (entry.own && entry.gesture) throw bad('a `gesture` is no key: no listener owns it');
    if (entry.own && entry.run !== undefined) {
      throw bad('`own` says the theme\'s own listener answers the key: it has no `run`');
    }
    if (entry.run === undefined) {
      if (entry.zone === 'page' && !entry.own && !entry.gesture) {
        throw bad('a key of the whole page has a `run`, or says `own: true` for a listener of '
          + 'the theme\'s own');
      }
      return;
    }
    if (typeof entry.run !== 'function') throw bad('`run` is a function of the event');
    if (entry.zone !== 'page') throw bad('an entry with `run` is a key of the whole page');
    const taken = keysOf(entry).find((key) => RESERVED.includes(key));
    if (taken !== undefined) {
      throw bad(`the frame reserves ${taken === ' ' ? 'Space' : `\`${taken}\``}`);
    }
    const dead = keysOf(entry).find((key) => !pressed(key));
    if (dead !== undefined) {
      throw bad(`\`${dead}\` is no \`event.key\`, so the key would never act: \`keys\` says `
        + 'which keys answer when the keycap is not the key');
    }
  });
}

/**
 * The core's keys over `document`: `{ frame, bind, listed, zoneOf, zonesOf }`, `frame` being the
 * frame's own list (`FRAME_KEYS`). `menuOwns(event)` says whether a key is the menu's own and
 * `base()` gives the core's facts for a `when` (`{ canWrite, help }`).
 */
export function createKeys({ document, menuOwns = () => false, base = () => ({}) }) {
  /** The facts each bound table was bound with, while it is bound. */
  const facts = new WeakMap();

  /** The facts for `table` now: the core's, then its theme's. */
  const now = (table) => ({ ...base(), ...(facts.get(table)?.() ?? {}) });

  function bind(table, { signal, ctx = () => ({}) } = {}) {
    check(table);
    if (!signal) throw new Error('svolder: a key table is bound with the signal of its mount');
    const runs = table.filter((entry) => entry.run);
    facts.set(table, ctx);
    signal.addEventListener('abort', () => {
      if (facts.get(table) === ctx) facts.delete(table);
    }, { once: true });
    if (runs.length === 0) return;
    document.addEventListener('keydown', (event) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.target?.closest?.(TYPING) || menuOwns(event)) return;
      const named = runs.filter((entry) => keysOf(entry).includes(event.key)
        && (!event.repeat || entry.repeat === true));
      if (named.length === 0) return;
      const at = now(table);
      const entry = named.find((one) => applies(one, at));
      if (!entry) return;
      try {
        entry.run(event);
      } catch (error) {
        console.warn(`svolder: the key ${entry.id} threw`, error);
      }
    }, { signal });
  }

  return {
    frame: FRAME_KEYS,
    bind,
    listed: (table, ctx) => listed(table, ctx ?? now(table)),
    zoneOf,
    zonesOf,
  };
}
