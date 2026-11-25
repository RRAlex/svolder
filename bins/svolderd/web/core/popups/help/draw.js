/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Help popup's draws: a model in, a string out. Pure, with no page and no core.
//
// Help holds no key list of its own. Its model is made of the entries of the theme's key table
// and of the frame's own list that apply now (`core.keys.listed`, keys.js), the very objects,
// and the draw reads `key` (through `capsOf`), `gesture`, `words`, `zone`, `own` and `id` of
// each. The model:
//
//   table    the entries of the theme's table that apply now, in table order
//   frame    the entries of the frame's own list that apply now (`T`, Space, `?`)
//   focus    where the focus was last, as the zones that hold it: a list, the innermost widget
//            first, of the zones that sit on one element (`[['.cell'], ['.sheet', '.sheet.open']]`)
//   names    the theme's words for its zones, `{ '<zone>': 'The dial' }`
//   version  the daemon's version, or `null` (a guest is not told)
//   daemon   where the daemon that serves the page is reached (`box.lan:8737`)
//
// A row carries its entry's id and zone (`data-key`, `data-zone`), so a check reads what Help
// shows against the tables.

import { attr, esc } from '../../html.js';
import { capsOf } from '../../keys.js';
import { heading, keycap, rows } from '../frame.js';

/** Help's own words. */
export const WORDS = {
  page: 'The whole page',
  // the heading of a zone its theme gave no name
  widget: 'This control',
  nowhere: 'Nothing with keys of its own has the focus. These keys work on the whole page.',
  everywhere: 'These keys work on every screen of this theme, wherever the focus is.',
  about: 'svolder',
  version: 'Version',
  versionGuest: 'Shown once you are signed in',
  daemon: 'Daemon',
  links: 'Links',
  newTab: 'Each link opens in a new tab.',
};

/** Where About leads: the project's page, which holds the documentation, and two places of it. */
export const LINKS = [
  { label: 'Project and documentation', href: 'https://github.com/RRAlex/svolder' },
  { label: 'Report a problem', href: 'https://github.com/RRAlex/svolder/issues' },
  { label: 'Releases', href: 'https://github.com/RRAlex/svolder/releases/latest' },
];

/** The keys of the whole page: the theme's, then the frame's. */
const pageGroup = ({ table, frame }) => ({
  zones: ['page'],
  heading: WORDS.page,
  entries: [...table.filter((entry) => entry.zone === 'page'), ...frame],
});

/** A group of the entries of `zones` (given in any order), named after the first of them in
 * table order: `keep` says which of their entries it holds. */
function widgetGroup(table, zones, names, keep = () => true) {
  const entries = table.filter((entry) => zones.includes(entry.zone) && keep(entry));
  const ordered = [...new Set(table.map((entry) => entry.zone))]
    .filter((zone) => zones.includes(zone));
  return { zones: ordered, heading: names[ordered[0]] ?? WORDS.widget, entries };
}

/**
 * Here: the keys that act where the focus was last. In order: a group for each widget that
 * holds the focus, the innermost first (zones on one element are one group); a group for each
 * other widget with a key that a listener of the theme's own answers on the whole page (`own`:
 * the Escape that leaves a mode), holding those keys alone; then the whole page. Returns `{
 * focused, groups }`, a group being `{ zones, heading, entries }`; `focused` is false when no
 * widget with keys holds the focus.
 */
export function hereGroups({
  table, frame, focus = [], names = {},
}) {
  const held = focus.map((zones) => widgetGroup(table, zones, names))
    .filter((group) => group.entries.length > 0);
  const claimed = new Set(held.flatMap((group) => group.zones));
  const others = [...new Set(table
    .filter((entry) => entry.own && entry.zone !== 'page' && !claimed.has(entry.zone))
    .map((entry) => entry.zone))];
  const own = others
    .map((zone) => widgetGroup(table, [zone], names, (entry) => entry.own === true));
  const page = pageGroup({ table, frame });
  return {
    focused: held.length > 0,
    groups: [...held, ...own, ...(page.entries.length > 0 ? [page] : [])],
  };
}

/** Everywhere: the keys of the whole page, the theme's and the frame's, as one group. */
export function everywhereGroups({ table, frame }) {
  const page = pageGroup({ table, frame });
  return page.entries.length > 0 ? [page] : [];
}

/** One key's row: its keycaps (a gesture's name is plain text), then what it does. */
const keyRow = (entry) => ({
  attrs: `${attr('data-key', entry.id)}${attr('data-zone', entry.zone)}`,
  label: entry.gesture ? esc(entry.key) : capsOf(entry).map(keycap).join(' '),
  value: esc(entry.words),
});

/** Groups as markup: each its heading, then its keys as a list. */
export function groupsHtml(groups) {
  return groups.map((group) => heading(group.heading) + rows(group.entries.map(keyRow))).join('');
}

const note = (words) => `<p class="fp-note">${esc(words)}</p>`;

/** The page Here. */
export function hereHtml(model) {
  const { focused, groups } = hereGroups(model);
  return `${focused ? '' : note(WORDS.nowhere)}${groupsHtml(groups)}`;
}

/** The page Everywhere. */
export const everywhereHtml = (model) => note(WORDS.everywhere)
  + groupsHtml(everywhereGroups(model));

/** A link that leaves the page: it opens in a new tab, and its words say where it leads. */
const link = (href) => `<a${attr('href', href)} target="_blank" rel="noopener">`
  + `${esc(href.replace(/^https:\/\//, ''))}</a>`;

/** The page About: the version, the daemon the page talks to, the links. */
export function aboutHtml({ version = null, daemon = '' }) {
  return heading(WORDS.about)
    + rows([
      { label: esc(WORDS.version), value: esc(version ?? WORDS.versionGuest) },
      { label: esc(WORDS.daemon), value: esc(daemon) },
    ])
    + heading(WORDS.links)
    + rows(LINKS.map((one) => ({ label: esc(one.label), value: link(one.href) })))
    + note(WORDS.newTab);
}

/** The page pane at `page` (`null`, the popup's list, shows Here). */
export function pageHtml({ page = null, ...model }) {
  if (page === 'about') return aboutHtml(model);
  if (page === 'everywhere') return everywhereHtml(model);
  return hereHtml(model);
}
