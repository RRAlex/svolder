/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where the theme Ember's sections stand in a screen: the element of each section by its id in
// the order (order.js), the box of each zone and the moves that put the sections in an order. A
// section is moved in the page, never drawn again, so what it holds (a dial's state, a focus, a
// pointer capture) goes with it; the browser's `moveBefore` does that where it has it.

import { moves } from './keep.js';
import { ZONES } from './order.js';

/** The selector of each section's element in a screen, by the section's id in the order. */
const SELECTOR = {
  playing: '#part-playing',
  visualizer: '.p-viz',
  volume: '#part-volume',
  controls: '#part-controls',
  power: '#part-power',
  tone: '#part-tone',
  presets: '#part-presets',
  'device-presets': '#part-device-presets',
  meters: '#part-meters',
};

/** The element of the section `id` in `view`, or nothing where the screen does not have it. */
export const sectionEl = (view, id) => (SELECTOR[id] ? view.querySelector(SELECTOR[id]) : null);

/**
 * The element holding the zone `zone` of `view`: the band, or one of the two columns. The band is
 * drawn only where it has a section, so with `make` a missing one is made (empty) and put before
 * the columns.
 */
export function boxOf(view, zone, make = false) {
  if (zone !== 'band') {
    return view.querySelectorAll(':scope > .col')[zone === 'left' ? 0 : 1] ?? null;
  }
  let band = view.querySelector(':scope > .p-band');
  if (!band && make) {
    band = document.createElement('div');
    band.className = 'p-band';
    const column = view.querySelector(':scope > .col');
    if (column) column.before(band);
    else view.append(band);
  }
  return band;
}

/** Put `node` in `parent` before `before` (the end, when there is none), moving it where the
 * browser can. */
function put(parent, node, before) {
  if (moves() && node.isConnected) {
    try {
      parent.moveBefore(node, before);
      return;
    } catch {
      // A browser that refuses this move takes the plain one below.
    }
  }
  parent.insertBefore(node, before);
}

/**
 * Put the sections of `view` in `order`, each zone's box marked with its name (`data-zone`). A
 * section the screen does not draw is left out; a zone with nothing to hold is not made unless
 * `make` says so.
 */
export function lay(view, order, make = false) {
  for (const zone of ZONES) {
    const els = (order[zone] ?? []).map((id) => sectionEl(view, id)).filter(Boolean);
    const box = boxOf(view, zone, make || els.length > 0);
    if (!box) continue;
    box.dataset.zone = zone;
    els.forEach((el, at) => {
      const now = box.children[at];
      if (now !== el) put(box, el, now ?? null);
    });
  }
}

/**
 * Put one section's element `el` in the place `order` gives the section `id`: before the first
 * later section of its zone that stands in that zone's box, else last in it.
 */
export function seat(view, el, id, order) {
  const zone = ZONES.find((z) => order[z]?.includes(id)) ?? 'band';
  const box = boxOf(view, zone, true);
  const later = order[zone].slice(order[zone].indexOf(id) + 1)
    .map((other) => sectionEl(view, other))
    .find((other) => other && other.parentElement === box);
  put(box, el, later ?? null);
}
