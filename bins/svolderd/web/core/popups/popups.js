/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The popups of the page, `core.popups`: each is added once with its title and a loader, and its
// code is fetched when a person first opens it, never before. So a page whose viewer never opens
// a popup loads none of its files.
//
//   add(name, { title, load, entries, stays })   make the popup's frame (frame.js) and register it
//                    with the router. `load()` is `() => import('./<name>/wire.js')`: it
//                    resolves to a module whose `wire({ core, frame })` gives the popup's part,
//                    `{ show(page), hide(), onKey(event) }`. `entries` (optional) is the side
//                    list, `[{ id, label }]`, for a popup whose list is the same for everyone:
//                    the loading line then stands beside the list. `stays: true` is a popup
//                    that stands through a change of the shown device. Returns the frame
//   names()          the popups added, in order
//   openDevice(url)  open the Devices popup at the page of the device at `url`: the way a theme's
//                    foot goes (*Device settings…*, or *Devices…* for a reader)
//   listed()         the entries of the page's list (listed.js) as the core was made with it
//                    (a copy that cannot be changed), each added as the core is made: what the
//                    menu reads its popup rows from
//
// A listed popup is its viewer's or nobody's: one whose row the viewer does not see (`seenBy` of
// listed.js: a writer's popup, for a guest or a page whose writes are off) opens by no path. The
// router asks at every open and at every step of the history, so nothing is pushed, drawn or
// loaded, and an open says what a write of that viewer's says (`core.actions.refuse()`: `Sign in
// to change this` with the way to sign in, or the page's read-only words). A viewer who loses
// the right while such a popup stands (the page's model replaced with another viewer) loses the
// popup: it closes.
//
// The popup is opened, stepped and closed through the router (`core.router.open(name, page)`).
// Opening shows the frame at once with its loading line; the part draws when the module has
// arrived, if the popup still stands. A module that does not come leaves the frame's failed
// line, which Escape, Back and the close button close like any popup, and the next open asks
// for the module again.
//
// While a popup stands, the browser tab's title names it after the device's words, with the
// label of its page at a page (`Bifrost 3 · connected · Devices · Kara — svolder`): the history
// has an entry for the popup and one for its page, and the title is how the browser's list of
// them tells one from another. The words go when the popup closes, however it closes.

import { createPopupFrame } from './frame.js';
import { DEVICES, checkListed, devicePage, seenBy } from './listed.js';

/** The popups over `core` (its router) in `document`; `listed` is the page's list of popups
 * (listed.js), each added here. */
export function createPopups({ core, document, listed: given = [] }) {
  checkListed(given);
  /** The list as it stood when the core was made, each entry and its row copied and frozen: a
   * later change to the array it came from adds no popup, so it must add no row, and who sees a
   * popup cannot be changed through what `listed()` hands out. */
  const listed = Object.freeze(given.map((entry) => Object.freeze({
    ...entry, row: Object.freeze({ ...entry.row }),
  })));
  /** The frames by the popup's name. */
  const frames = new Map();
  /** Whether the page's viewer may have the listed popup `entry` now. */
  const sees = (entry) => seenBy(entry, core.model.page().viewer);

  // the open popup's words in the tab's title, read from the router each time the title is made
  core.title?.add(() => {
    const { popup, page } = core.router.state();
    return frames.get(popup)?.titleWords(page) ?? null;
  });

  const popups = {
    add(name, {
      title, load, entries = [], stays = false,
    } = {}, entry = null) {
      if (frames.has(name)) throw new Error(`svolder: the popup ${name} is added already`);
      if (typeof load !== 'function') {
        throw new Error(`svolder: the popup ${name} is added with its loader, \`load\``);
      }
      const frame = createPopupFrame({
        document, router: core.router, name, title, entries, drawn: () => core.title?.refresh(),
      });
      core.router.register(name, {
        // a listed popup is for the viewers its row is for; one added by hand is anyone's
        ...(entry ? { allowed: () => sees(entry), refused: () => core.actions.refuse() } : {}),
        el: frame.el,
        labelledBy: frame.labelledBy,
        focus: frame.focus,
        stays: stays === true,
        loading: (page) => frame.loading(page),
        failed: (page) => frame.failed(page),
        load: async () => {
          const module = await load();
          if (typeof module?.wire !== 'function') {
            throw new Error(`the module of the popup ${name} exports no wire`);
          }
          const part = module.wire({ core, frame });
          return {
            show(page) {
              // the stack put the focus where it could while the popup was loading (the close
              // button, when the list was not known yet): once it is drawn, the focus goes
              // where a popup that opens drawn has it
              const waited = frame.state() === 'loading'
                && frame.el.contains(document.activeElement);
              part.show(page);
              if (waited) frame.focus()?.focus({ preventScroll: true });
            },
            hide: () => part.hide?.(),
            onKey: (event) => part.onKey?.(event) === true,
          };
        },
      });
      frames.set(name, frame);
      return frame;
    },

    names: () => [...frames.keys()],

    /** Open the Devices popup at the page of the device at `url` (its URL segment), whichever
     * device the page shows. A theme calls it from a person's gesture (its *Device settings…*
     * or, for a reader, *Devices…*), and the router asks who may have the popup as it does
     * for every way in. */
    openDevice: (url) => core.router.open(DEVICES, devicePage(url)),

    listed: () => listed,
  };

  for (const entry of listed) popups.add(entry.name, entry, entry);
  // the viewer changed under a popup that is no longer theirs
  core.model.on('model', () => {
    const entry = listed.find((one) => one.name === core.router.state().popup);
    if (entry && !sees(entry)) core.router.close();
  });
  return popups;
}
