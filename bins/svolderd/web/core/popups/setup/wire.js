/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The PipeWire config popup's wire: thin. It draws the check of the shown device (draw.js) into the
// popup's frame at each `show`, and again when the model, a device's shape or its `playing` part
// changes under it (the check rides the card's path). It writes nothing, takes no stream want and
// attaches no key listener; the popup closes with a change of the shown device (it is about that
// card).

import { WORDS, pageHtml, changesOf, setupOf } from './draw.js';

/** The popup's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`, drawn
 * into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const store = core.model;
  let offs = [];

  const draw = () => frame.draw({
    title: WORDS.title, body: pageHtml({ setup: setupOf(store.page()), changes: changesOf(store.page()) }),
  });

  return {
    show() {
      if (offs.length === 0) {
        offs = [
          store.on('model', draw),
          store.on('part', (data) => { if (data.name === 'playing') draw(); }),
          store.on('device', draw),
        ];
      }
      draw();
    },
    hide() {
      for (const off of offs) off();
      offs = [];
    },
    onKey: () => false,
  };
}
