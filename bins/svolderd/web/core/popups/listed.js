/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The popups of the page, as one list: the one place a popup is added. Each entry is the popup
// and its row in the frame's menu together, so a row cannot stand without its popup, nor a
// popup without its row:
//
//   { name, title, entries?, stays?, load, row: { label, sees } }
//
//   name      the popup's name: the router's (`core.router.open(name)`) and its folder's
//   title     its title, until a draw names another
//   entries   the side list, when it is the same for everyone (popups.js)
//   stays     `true` for a popup that is about the devices and not about the one on show: it
//             stands through a change of the shown device (router.js), where every other popup
//             closes
//   load      `() => import('./<name>/wire.js')`: the popup's code, fetched at its first opening
//   row       its row in the menu: `label`, the row's words, and `sees`, who has the popup:
//             `'everyone'`, or `'writer'` (a viewer who may write: no guest, and not on a page
//             whose writes are off; `mayWrite` of write.js, the core's one rule)
//
// `createCore` adds every entry to `core.popups` (popups.js), and the menu's rows
// (menu-rows.js) are read from the same entries, in this order. `sees` is the popup's, not only
// the row's: a viewer who does not see the row cannot open the popup by any other way either
// (popups.js gives the router the same answer). Pure data: no page, no core.

import { mayWrite } from '../write.js';

/** The name of the Help popup, which is also the id of the frame's key entry that opens it
 * (`?` in `FRAME_KEYS`, keys.js): the key is listed once a popup of this name is added. */
export const HELP = 'help';

/** The name of the Devices popup. */
export const DEVICES = 'devices';

/** The name of the EQ library popup. */
export const EQ_LIBRARY = 'eq-library';

/** The page of the Devices popup that is about the device at `url` (its URL segment): the page's
 * id in the router, whichever device the page behind shows. */
export const devicePage = (url) => `d-${url}`;

/** Every popup of the page, in the order of their rows in the menu. Help is the last. A popup's
 * title and its side list are written here and not read from its folder, which loads at its
 * first opening. Devices is everyone's: a writer's draws the configured devices and their
 * settings, a reader's (a guest, or a page with writes off) the fleet and each device's facts,
 * signal and readings with nothing that writes. It stays through a change of the shown device,
 * which it makes itself. The EQ library is a writer's only (the daemon reaches the internet for
 * it, which only a writer asks) and closes with a change of the shown device: it is about that
 * device. */
export const POPUPS = [
  {
    name: DEVICES,
    title: 'Devices',
    entries: [{ id: 'overview', label: 'Overview' }],
    stays: true,
    load: () => import('./devices/wire.js'),
    row: { label: 'Devices…', sees: 'everyone' },
  },
  {
    name: EQ_LIBRARY,
    title: 'EQ library',
    entries: [
      { id: 'autoeq', label: 'AutoEq' },
      { id: 'upload', label: 'Upload a file' },
      { id: 'saved', label: 'Saved presets' },
    ],
    load: () => import('./eq-library/wire.js'),
    row: { label: 'EQ library…', sees: 'writer' },
  },
  {
    name: HELP,
    title: 'Help',
    entries: [
      { id: 'here', label: 'Here' },
      { id: 'everywhere', label: 'Everywhere' },
      { id: 'about', label: 'About' },
    ],
    load: () => import('./help/wire.js'),
    row: { label: 'Help…', sees: 'everyone' },
  },
];

/** Who may have a popup's row. */
export const SEES = ['everyone', 'writer'];

/** The id of the menu row of the popup `name`. */
export const rowId = (name) => `menu-popup-${name}`;

/** Whether `viewer` (the page's: `guest`, `writes`) has `popup`: its row, and the popup. */
export function seenBy(popup, viewer) {
  return popup.row.sees === 'everyone' || mayWrite({ viewer });
}

/** Throw unless `list` is a list of popups the core can add, each with its row. */
export function checkListed(list) {
  if (!Array.isArray(list)) throw new Error('svolder: the popups are a list');
  const text = (value) => typeof value === 'string' && value !== '';
  const names = new Set();
  for (const popup of list) {
    if (!text(popup?.name)) throw new Error('svolder: a listed popup has a name, a string');
    const bad = (why) => new Error(`svolder: the listed popup ${popup.name}: ${why}`);
    if (names.has(popup.name)) throw bad('it is listed twice');
    names.add(popup.name);
    if (!text(popup.title)) throw bad('`title` is its title, a string');
    if (typeof popup.load !== 'function') throw bad('`load` imports its wire');
    if (popup.stays !== undefined && typeof popup.stays !== 'boolean') {
      throw bad('`stays` is true or false');
    }
    if (!popup.row) throw bad('`row` is its row in the menu: no popup is listed without one');
    if (!text(popup.row.label)) throw bad('its row has a `label`, the row\'s words');
    if (!SEES.includes(popup.row.sees)) {
      throw bad(`its row says who \`sees\` it: ${SEES.map((one) => `'${one}'`).join(' or ')}`);
    }
  }
}
