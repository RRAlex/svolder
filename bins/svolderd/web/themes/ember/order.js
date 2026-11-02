/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The order of the theme Ember's sections on a device's screen. A layout has three zones: the
// band across the top and two columns. Each holds sections by id, in the order they are drawn.

/** The sections of each layout, zone by zone, in the order the page sets them. */
export const DEFAULT_ORDER = {
  dac: {
    band: ['playing', 'visualizer'],
    left: ['volume', 'controls', 'power'],
    right: ['tone', 'presets', 'device-presets'],
  },
  amp: {
    band: ['playing', 'visualizer'],
    left: ['meters'],
    right: ['power', 'controls'],
  },
};

/** The zones, in the order a kept id is claimed by the first that holds it. */
export const ZONES = ['band', 'left', 'right'];

/** The sections a person may move: every one a layout draws but the card and the fold-out. */
export const MOVABLE = ['playing', 'visualizer', 'volume', 'controls', 'power', 'tone', 'presets',
  'device-presets', 'meters'];

/**
 * The order a screen of `layout` draws: the sections `kept` names come first in each zone, an id
 * the layout does not have is dropped, and an id `kept` does not name goes after them in the zone
 * the layout sets it in. `kept` is what the viewer saved, or nothing.
 */
export function resolveOrder(kept, layout) {
  const base = DEFAULT_ORDER[layout] ?? DEFAULT_ORDER.dac;
  const known = new Set(ZONES.flatMap((zone) => base[zone]));
  const taken = new Set();
  const given = (zone) => (kept && typeof kept === 'object' && Array.isArray(kept[zone])
    ? kept[zone] : []);
  const first = Object.fromEntries(ZONES.map((zone) => [zone, given(zone).filter((id) => {
    if (!known.has(id) || taken.has(id)) return false;
    taken.add(id);
    return true;
  })]));
  return Object.fromEntries(ZONES.map((zone) => [
    zone,
    [...first[zone], ...base[zone].filter((id) => !taken.has(id))],
  ]));
}

/** The order this browser kept for `layout`, out of the stored value (`{ dac, amp }`, each
 * `{ band, left, right }`), or nothing: an order kept for one layout is not another's. */
export function keptOrder(stored, layout) {
  if (!stored || typeof stored !== 'object' || !Object.hasOwn(stored, layout)) return null;
  const own = stored[layout];
  return own && typeof own === 'object' && !Array.isArray(own) ? own : null;
}

/** The order a screen of `layout` is drawn in for the theme's preferences `prefs`. */
export function orderFor(prefs, layout) {
  return resolveOrder(keptOrder(prefs?.get('arrange', null), layout), layout);
}

/** Whether `order` is the layout's own, counting only the sections `drawn` says (all, by default):
 * a section the screen does not draw may stand anywhere without changing what shows. */
export function isDefault(order, layout, drawn = () => true) {
  const base = DEFAULT_ORDER[layout] ?? DEFAULT_ORDER.dac;
  const full = resolveOrder(order, layout);
  return ZONES.every((zone) => {
    const mine = full[zone].filter(drawn);
    const theirs = base[zone].filter(drawn);
    return mine.length === theirs.length && mine.every((id, at) => id === theirs[at]);
  });
}

/**
 * `order` with `id` put in `zone` at `index` among the sections of that zone that `drawn` says
 * (all, by default), the others not counted; an index past the end puts it last. An id the order
 * lacks and a zone that is none change nothing.
 */
export function moveTo(order, id, zone, index, drawn = () => true) {
  if (!ZONES.includes(zone) || !ZONES.some((z) => order[z]?.includes(id))) return order;
  const out = Object.fromEntries(ZONES.map((z) => [z, (order[z] ?? []).filter((x) => x !== id)]));
  const ids = out[zone];
  const shown = ids.filter(drawn);
  const at = Math.min(shown.length, Math.max(0, index));
  let place = ids.length;
  if (at < shown.length) place = ids.indexOf(shown[at]);
  else if (shown.length > 0) place = ids.indexOf(shown[shown.length - 1]) + 1;
  out[zone] = [...ids.slice(0, place), id, ...ids.slice(place)];
  return out;
}

/**
 * `order` after the arrow `key` moves `id`: up or down within its zone, left or right to the
 * zone before or after it at the same place (or the end, where that zone is shorter). At an edge
 * nothing moves. `drawn` is as for [`moveTo`].
 */
export function moveBy(order, id, key, drawn = () => true) {
  const zone = ZONES.find((z) => order[z]?.includes(id));
  if (!zone) return order;
  const shown = order[zone].filter(drawn);
  const at = shown.indexOf(id);
  if (at < 0) return order;
  if (key === 'ArrowUp') return at > 0 ? moveTo(order, id, zone, at - 1, drawn) : order;
  if (key === 'ArrowDown') {
    return at < shown.length - 1 ? moveTo(order, id, zone, at + 1, drawn) : order;
  }
  const step = { ArrowLeft: -1, ArrowRight: 1 }[key];
  const next = step && ZONES[ZONES.indexOf(zone) + step];
  return next ? moveTo(order, id, next, at, drawn) : order;
}
