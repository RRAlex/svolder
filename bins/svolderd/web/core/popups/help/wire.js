/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Help popup's wire: thin. At each `show` it reads what the keys are now (the key table of
// the theme on show and the frame's own list, as `core.keys.listed` gives them, and the zones
// that hold the element of the theme that had the focus last) and draws the page (draw.js)
// into the popup's frame. It reads nothing when the popup is added and holds nothing between
// two shows: Here is read again at every `show` (the opening, each step between the pages, a
// step of the history), so it is about the page as it stands then. The last focus is the
// frame's record and stays through the popup; a theme that draws that control again under the
// popup takes it out of the page, and the next show of Here has the page's keys alone. Help
// writes nothing, follows no device and wants nothing of the stream; it has no key of its own
// and attaches no listener.

import { pageHtml } from './draw.js';

/** Help's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`, drawn into
 * `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const doc = frame.el.ownerDocument;

  /** The zones that hold `el`, the innermost widget first, those on one element together. */
  function held(table, el) {
    const groups = [];
    for (const zone of [...core.keys.zonesOf(table, el)].reverse()) {
      const on = el.closest(zone);
      const same = groups.find((group) => group.on === on);
      if (same) same.zones.push(zone);
      else groups.push({ on, zones: [zone] });
    }
    return groups.map((group) => group.zones);
  }

  function model() {
    const table = core.keys.listed(core.frame.keyTable());
    return {
      table,
      frame: core.keys.listed(core.keys.frame),
      focus: held(table, core.frame.lastFocus()),
      names: core.frame.zoneNames(),
      version: core.model.page().viewer?.version ?? null,
      daemon: doc.defaultView?.location.host ?? '',
    };
  }

  return {
    show(page) {
      frame.draw({ page, body: pageHtml({ page, ...model() }) });
    },
    hide() {},
    onKey: () => false,
  };
}
