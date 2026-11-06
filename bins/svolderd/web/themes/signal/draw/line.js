/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The line of the theme Signal, drawn: the wire and its dots as SVG, each hop's label, the box
// round the chosen one. Hops stand evenly between 10 % and 90 % of the band's width, so every
// coordinate is a percentage and nothing here needs the band's size. No markup carries a `style`
// attribute: the labels' places and the box's go through the CSSOM (`place`). The moving dashes
// are moved by `createFlow`, which only sets `stroke-dashoffset`.

import { LABEL_LIMIT, NOT_WATCHED } from '../words.js';
import { esc } from './esc.js';

/** The band's height and the wire's height in it (px). */
export const BAND_PX = 140;
export const WIRE_Y = 60;

/** The width a label may have, and the box round the chosen one (px). */
const LABEL_PX = 190;
const BOX_PX = 200;

/** How far a dash travels in one turn of its loop, and how long a turn takes before the DAC and
 * after it (seconds). */
const STEP_PX = 16;
const PERIOD_S = { before: 1.1, after: 1.6 };

/** Where hop `i` of `n` stands, in percent of the band's width. */
export const xOf = (i, n) => (n === 1 ? 50 : 10 + (80 * i) / (n - 1));

const pct = (x) => `${x.toFixed(2)}%`;

/** Where the wire is drawn on a desktop: hops in percent of the band's width, the wire at `WIRE_Y`. */
const DESK = {
  at: (i, n) => pct(xOf(i, n)),
  edge: pct(0),
  y: WIRE_Y,
  svg: () => `width="100%" height="${BAND_PX}"`,
};

/** The strip a phone has (P7b): hops a fixed step apart in px along a track, the wire's picture
 * starting `STRIP_BLEED` px to the left of the first hop so the wire runs on past both ends. */
export const HOP_PX = 110;
export const STRIP_PX = 92;
const STRIP_BLEED = 200;
const STRIP_WIRE_Y = 30;
const STRIP = {
  at: (i) => String(STRIP_BLEED + i * HOP_PX),
  edge: '0',
  y: STRIP_WIRE_Y,
  svg: (n) => `width="${2 * STRIP_BLEED + (n - 1) * HOP_PX}" height="${STRIP_PX}"`,
};

/** A stretch of the wire from `a` to `b` (coordinates as strings, from `where`); `flow` names the
 * loop its dashes move in. */
const stretch = (where, a, b, cls, flow = '') => `<line class="${cls}" x1="${a}" x2="${b}" `
  + `y1="${where.y}" y2="${where.y}"${flow ? ` data-flow="${flow}"` : ''}/>`;

/** The wire: the green stretch to the shown device (yellow where the computer changes the sound),
 * the thin one after it, the dots and rings on them. A stand-alone amp has the greyed way in. */
function wire(hops, shownId, where = DESK) {
  const n = hops.length;
  const at = (i) => where.at(i, n);
  const d = hops.findIndex((hop) => hop.id === shownId);
  const alone = hops.some((hop) => hop.id === 'standin-dac');
  // Before a linked amp the DAC stands in the line: the way in to it is as unknown as an amp's alone.
  const via = hops.findIndex((hop) => hop.via);
  const lead = alone ? d : via;
  const c = hops.findIndex((hop) => hop.kind === 'computer');
  let marks = '';
  if (d === 0) marks += stretch(where, where.edge, at(0), 'sg-grey');
  else if (lead >= 0) marks += stretch(where, at(0), at(lead), 'sg-grey');
  else if (d > 0) marks += stretch(where, at(0), at(d), 'sg-before sg-flow', 'before');
  if (lead < 0 && c >= 0 && c < d && hops[c].changes) {
    marks += stretch(where, at(c), at(d), 'sg-changes sg-flow', 'before');
  }
  // At a shown device in trouble the wire ends in a hollow ring and what comes after it stands still.
  const trouble = d >= 0 ? hops[d].trouble : null;
  const broken = trouble === 'bad' || trouble === 'wait';
  // The thin stretch runs from the DAC before a linked amp, else from the shown device.
  const from = via >= 0 ? via : d;
  if (d >= 0 && from < n - 1) {
    marks += broken ? stretch(where, at(from), at(n - 1), 'sg-after sg-still')
      : stretch(where, at(from), at(n - 1), 'sg-after sg-flow', 'after');
  }
  const dots = hops.map((hop, i) => {
    const grey = lead >= 0 && i < lead && hop.kind !== 'standin';
    const ring = !hop.live;
    const lost = trouble && i === d;
    const cls = lost ? `sg-lostring sg-t-${trouble === 'fault' ? 'bad' : trouble}`
      : ring ? 'sg-ring' : `sg-dot sg-d-${hop.kind}${hop.id === shownId ? ' sg-shown' : ''}`;
    const r = hop.id === shownId ? 9 : ring ? 6 : 7;
    return `<circle class="${cls}${grey ? ' sg-grey-dot' : ''}${broken && i > d ? ' sg-dim' : ''}" cx="${at(i)}" cy="${where.y}" r="${r}"/>`;
  }).join('');
  return `<svg class="sg-wire" ${where.svg(n)} aria-hidden="true" `
    + `xmlns="http://www.w3.org/2000/svg">${marks}${dots}</svg>`;
}

/** The class of a line of a hop's words: what it says about the sound, in colour. */
function tone(hop, text, last, shownId) {
  if (hop.trouble && hop.id === shownId) return hop.trouble === 'wait' ? ' sg-wait' : ' sg-bad';
  if (!last || hop.kind === 'declared' || hop.kind === 'standin') return '';
  if (hop.changes && hop.kind === 'computer') return ' sg-warn';
  if (hop.kind === 'player' && text === 'playing') return ' sg-ok';
  if (hop.id === shownId && hop.words.length > 1) return hop.changes ? ' sg-warn' : ' sg-ok';
  return '';
}

/** One hop's label: its name and its words, a dim italic for a ring. On a phone (`strip`) the
 * hop's place is its number along the strip and it says one line of words. */
function label(hop, i, n, shownId, chosen, canList, after, strip = false) {
  const cur = hop.id === chosen;
  // The chosen shown device's name opens the list of the devices when there is another to show.
  const lists = cur && hop.id === shownId && canList;
  const ring = !hop.live;
  const cls = `${strip ? 'sg-sop' : 'sg-hop'} sg-k-${hop.kind}${ring ? ' sg-decl' : ''}${cur ? ' sg-cur' : ''}${after ? ' sg-dimmed' : ''}`;
  // The shown device keeps its volume written under its name while another hop is chosen.
  const volume = hop.id === shownId && !cur && hop.volume ? hop.volume : null;
  const spoken = volume ? [...hop.words.slice(0, 1), volume] : hop.words;
  const said = strip ? spoken.slice(0, 1) : spoken;
  const words = !strip && n > LABEL_LIMIT ? '' : said.map((text, k) => {
    const cls2 = volume && k === said.length - 1 && !strip ? ' sg-v' : tone(hop, text, k === said.length - 1, shownId);
    return `<span class="sg-w${cls2}">${esc(text)}</span>`;
  }).join('');
  const where = strip ? `data-i="${i}"` : `data-x="${xOf(i, n).toFixed(2)}"`;
  return `<div class="${cls}" role="option" id="sg-hop-${i}" data-hop="${esc(hop.id)}" `
    + `${where} aria-selected="${cur}"${lists ? ' aria-haspopup="listbox"' : ''} tabindex="${cur ? 0 : -1}" `
    + `title="${esc(hop.name)}"><b>${esc(hop.name)}${lists ? ' <span class="sg-more" aria-hidden="true">\u25BE</span>' : ''}</b>${words}</div>`;
}

/** The inside of the line's band: the wire, the box and the hops' labels. */
export function line(hops, shownId, chosen, canList = false) {
  if (hops.length === 0) return '';
  // A box round a hop the person declared (a ring) is dashed, like the ring.
  const ring = hops.some((hop) => hop.id === chosen && !hop.live);
  // What comes after a shown device in trouble is dimmed.
  const d = hops.findIndex((hop) => hop.id === shownId);
  const trouble = d >= 0 && (hops[d].trouble === 'bad' || hops[d].trouble === 'wait');
  return `${wire(hops, shownId)}<div class="sg-box${ring ? ' sg-boxdecl' : ''}" aria-hidden="true"></div>`
    + hops.map((hop, i) => label(hop, i, hops.length, shownId, chosen, canList, trouble && i > d)).join('');
}

/** The inside of the line's band on a phone: a track of hops a step apart, with the wire under
 * them, and the box that stays at the strip's middle while the track slides under it. */
export function strip(hops, shownId, chosen, canList = false) {
  if (hops.length === 0) return '';
  const ring = hops.some((hop) => hop.id === chosen && !hop.live);
  const d = hops.findIndex((hop) => hop.id === shownId);
  const trouble = d >= 0 && (hops[d].trouble === 'bad' || hops[d].trouble === 'wait');
  return `<div class="sg-slide">${wire(hops, shownId, STRIP)}`
    + hops.map((hop, i) => label(hop, i, hops.length, shownId, chosen, canList, trouble && i > d, true)).join('')
    + `</div><div class="sg-pbox${ring ? ' sg-boxdecl' : ''}" aria-hidden="true"></div>`;
}

/** The `transform` that brings hop number `at` under the box; `drag` is how far a finger has pulled it. */
export const slide = (at, drag = 0) => `translateX(${drag - at * HOP_PX}px)`;

/** Put the strip's hops where they stand and slide the track to the chosen one; `from` is the
 * `transform` the old track was at, which the new one moves on from (the CSS makes that a glide). */
export function placeStrip(band, from = '') {
  const track = band.querySelector('.sg-slide');
  if (!track) return;
  const options = [...track.querySelectorAll('.sg-sop')];
  for (const el of options) el.style.left = `calc(50% + ${Number(el.dataset.i) * HOP_PX}px)`;
  const at = Math.max(0, options.findIndex((el) => el.classList.contains('sg-cur')));
  track.dataset.at = String(at);
  if (from) {
    track.style.transform = from;
    track.getBoundingClientRect();
  }
  track.style.transform = slide(at);
}

/** The quiet line on the left when the computer is not watched, else nothing. */
export const stub = (page) => (page.host ? '' : NOT_WATCHED);

/** Put the labels and the box where they stand (the CSSOM; the markup has no style). */
export function place(band) {
  const labels = band.querySelectorAll('.sg-hop');
  const n = labels.length;
  const width = n > 1 ? 80 / (n - 1) : 80;
  for (const el of labels) {
    el.style.left = `${el.dataset.x}%`;
    el.style.width = `min(${LABEL_PX}px, ${width.toFixed(2)}%)`;
  }
  const chosen = band.querySelector('.sg-hop.sg-cur');
  const box = band.querySelector('.sg-box');
  if (box) {
    box.hidden = !chosen;
    if (chosen) {
      box.style.left = chosen.style.left;
      // The box stays inside its slot, so it never runs through a neighbour's name.
      box.style.width = `min(${BOX_PX}px, calc(${width.toFixed(2)}% - 8px))`;
    }
  }
}

/** Mark `id` chosen in place: its class and aria state, the roving tab stop, the box under it. */
export function mark(band, id) {
  for (const el of band.querySelectorAll('.sg-hop')) {
    const on = el.dataset.hop === id;
    el.classList.toggle('sg-cur', on);
    el.setAttribute('aria-selected', String(on));
    el.tabIndex = on ? 0 : -1;
  }
  place(band);
}

/**
 * The moving dashes of the wire in `band`: the turn each loop is at, in seconds, and where each
 * dash stands for it. `step(dt)` moves them on by `dt` seconds (0 leaves them); `apply()` sets the
 * places again (after the wire was drawn anew).
 */
export function createFlow(band) {
  const turn = { before: 0, after: 0 };
  function apply() {
    for (const el of band.querySelectorAll('[data-flow]')) {
      const kind = el.getAttribute('data-flow');
      el.setAttribute('stroke-dashoffset', (-(turn[kind] / PERIOD_S[kind]) * STEP_PX).toFixed(2));
    }
  }
  return {
    step(dt) {
      for (const kind of Object.keys(turn)) turn[kind] = (turn[kind] + dt) % PERIOD_S[kind];
      apply();
    },
    apply,
  };
}
