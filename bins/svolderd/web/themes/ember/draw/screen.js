/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember's screen: a device's sections laid out in the band and two columns the order
// sets, each drawn as it first appears (none asks htmx to swap it out of band). A section the
// order names that Ember does not have is left out. A page with no device to show draws its card
// alone. Pure, with no DOM.

import { attr } from './html.js';
import { PARTS } from './parts.js';
import { DEFAULT_ORDER, isDefault, resolveOrder } from '../order.js';

/**
 * The JSON `value` as the text of a data block: no `<` in it, so nothing in it can close the
 * script it sits in.
 */
const block = (value) => JSON.stringify(value).replace(/</g, '\\u003c');

/** Every section the layouts name: the ids an order may place. */
const SECTIONS = new Set(Object.values(DEFAULT_ORDER)
  .flatMap((layout) => Object.values(layout).flat()));

/** The sections that stand apart from what follows them in a column once they are drawn. */
const TRAILING = new Set(['volume', 'meters', 'power', 'tone', 'presets', 'device-presets']);

/**
 * The section `id` as it stands in its zone: a space before it, and one after for the sections
 * that stand apart from what follows. The stylesheet and the page's scripts lay out those spaces.
 */
const included = (html, id) => ` ${html}${html !== '' && TRAILING.has(id) ? ' ' : ''}`;

/**
 * Whether the screen of `device` draws the section `id`: Now playing and the visualizer only where
 * there is a player to follow, the volume, the meters and the power button only where the device
 * has them, the rest as the layout has them (the tone control, the presets and the device presets
 * draw their own emptiness when the device has none).
 */
function drawn(id, device) {
  switch (id) {
    case 'playing':
    case 'visualizer': return device.playing != null;
    case 'volume': return device.volume != null;
    case 'meters': return device.meters != null;
    case 'power': return device.power != null;
    default: return true;
  }
}

/** The ids of `zone` that Ember draws for `device`, in order; a zone that is no list has none. */
const drawnIn = (zone, device) => (Array.isArray(zone) ? zone : [])
  .filter((id) => SECTIONS.has(id) && PARTS[id] && drawn(id, device));

/** The sections `ids` of one zone, in order, each as the first paint has it. */
const sections = (ids, device, page) => ids
  .map((id) => included(PARTS[id](device, page, { oob: false }), id))
  .join('');

/** The band across the top, where it holds a section: Now playing and the visualizer by default. */
function band(zone, device, page) {
  const ids = drawnIn(zone, device);
  if (ids.length === 0) return '';
  return `<div class="p-band">${sections(ids, device, page)}</div>`;
}

/** The class words of the view: its layout, and `arranged` where the order is not the layout's own
 * (the style sheet's phone order and the visualizer's place then leave the sections where they
 * stand). */
const kind = (device, order) => {
  const base = device.layout === 'amp' ? `amp${device.lone ? ' lone' : ''}` : 'dac';
  const shown = (id) => SECTIONS.has(id) && PARTS[id] && drawn(id, device);
  return order && !isDefault(order, device.layout, shown) ? `${base} arranged` : base;
};

/**
 * The `#screen` of `page`: the shown device's view, the strip's data and the tone's steps,
 * `order` setting where each section stands (the default order where it is not given).
 */
export function screen(page, order) {
  const device = page.device;
  if (!device) return PARTS.card(null, page, { oob: false });
  const zones = order ?? resolveOrder(null, device.layout);
  const inert = device.locked ? ' inert' : '';
  const column = (zone) => `<div class="col"${inert}>${sections(drawnIn(zone, device), device, page)}</div>`;
  const card = device.card ? included(PARTS.card(device, page, { oob: false })) : '';
  const view = `<div role="group"${attr('aria-label', `${device.name}, ${device.family_word}`)} class="view ${kind(device, order)}${device.away ? ' away' : ''}"${attr('data-device', device.url)}${attr('data-name', device.name)}${attr('data-family', device.family)}${attr('data-glow', device.glow.toFixed(2))}${attr('data-shape', device.shape)}> ${card} ${band(zones.band, device, page)} ${column(zones.left)} ${column(zones.right)} ${included(PARTS.facts(device, page, { oob: false }))} </div>`;
  const strip = `<script type="application/json" id="strip">${block(page.strip ?? {})}</script>`;
  const steps = device.tone_steps != null
    ? ` <script type="application/json" id="tone-steps">${block(device.tone_steps)}</script>`
    : '';
  return `${view} ${strip}${steps}`;
}
