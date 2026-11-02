/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The controls section: each of the device's controls written the way its kind is. A choice of a
// few values is its label and the values in a line, the current one underlined in pencil; a longer
// one is its label and the current value, a word that opens a list; a switch reads `on` or `off`;
// a level (a balance) is a printed scale with a pencil mark and its value written beside it, a tap
// on which writes the centre; a readout is its label and its words. Pure: the model in, markup out.

import { fractionOf } from '../ruler.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

export const SCALE_BOX = 1000;
/** About how many ticks a scale prints. */
const TICKED = 32;

/** An id made from a control's key and a suffix: what keeps the focus when the section is drawn
 * again. */
const idOf = (prefix, key, more = '') => `${prefix}-${String(key).replace(/[^\w-]/g, '_')}${more}`;

/** The printed ticks of a scale from `lo` to `hi`: one a few steps, the centre's taller. */
export function ticksOf(c) {
  const span = c.hi - c.lo;
  const every = Math.max(1, Math.ceil(span / TICKED));
  const at = (raw) => (((raw - c.lo) / span) * SCALE_BOX).toFixed(2);
  const minor = [];
  for (let raw = c.lo; raw <= c.hi; raw += every) {
    if (raw !== c.centre) minor.push(`M${at(raw)} 0V6`);
  }
  const centre = typeof c.centre === 'number' && c.centre >= c.lo && c.centre <= c.hi
    ? `<path class="major" d="M${at(c.centre)} 0V12"/>` : '';
  return `<path class="minor" d="${minor.join('')}"/>${centre}`;
}

/** A choice of a few values: the label, then the values joined by dots. */
function segments(c, inert) {
  const options = (c.options ?? []).map((o, i) => word('choice', o.label, inert,
    ` id="${idOf('p-c', c.key, `-${i}`)}" data-key="${esc(c.key)}" data-value="${esc(o.label)}"`,
    o.on ? 'on' : ''));
  return `<div class="p-row"><h2 class="p-label">${esc(c.label)}</h2>`
    + `<p class="p-choice">${options.join(' &middot; ')}</p></div>`;
}

/** A choice of more values: the label and the current value, which opens the list. */
function list(c, inert) {
  const now = (c.options ?? []).find((o) => o.on)?.label ?? WORDS.unknownWord;
  return `<p class="p-row p-line">${esc(c.label)}: ${word('choice-list', now, inert,
    ` id="${idOf('p-l', c.key)}" data-key="${esc(c.key)}" aria-haspopup="listbox"`)}</p>`;
}

/** A switch: the label and `on`, `off` or a dash where the device has not said. */
function toggle(c, inert) {
  const said = c.on === true ? WORDS.onWord : c.on === false ? WORDS.offWord : WORDS.unknownWord;
  return `<p class="p-row p-line">${esc(c.label)}: ${word('toggle', said, inert,
    ` id="${idOf('p-t', c.key)}" data-key="${esc(c.key)}" aria-pressed="${c.on === true}"`,
    c.on === true ? 'on' : '')}</p>`;
}

/** A level: its label, a scale with the pencil mark where the device reads, the words of its ends
 * and its value (which writes the centre when the level has one). */
function slider(c, inert) {
  const read = typeof c.raw === 'number';
  const [left, right] = c.ends ?? [];
  const mark = read
    ? `<span class="p-mark" data-key="${esc(c.key)}" data-at="${fractionOf(c, c.raw).toFixed(5)}"></span>`
    : '';
  const value = typeof c.centre === 'number'
    ? word('centre', c.text, inert, ` id="${idOf('p-ce', c.key)}" data-key="${esc(c.key)}"`)
    : `<span class="p-val">${esc(c.text)}</span>`;
  return `<div class="p-row p-level"><p class="p-line">${esc(c.label)}: ${value}</p>`
    + `<div class="p-scalebox">${left === undefined ? '' : `<span class="p-end">${esc(left)}</span>`}`
    + `<div id="${idOf('p-s', c.key)}" class="p-scale${inert ? ' quiet' : ''}" role="slider" tabindex="0"`
    + ` data-key="${esc(c.key)}" aria-label="${esc(c.label)}" aria-valuemin="${esc(c.lo)}"`
    + ` aria-valuemax="${esc(c.hi)}"${read ? ` aria-valuenow="${esc(c.raw)}"` : ''}`
    + ` aria-valuetext="${esc(c.text)}"${inert ? ' aria-disabled="true"' : ''}>`
    + `<svg viewBox="0 0 ${SCALE_BOX} 12" preserveAspectRatio="none" aria-hidden="true">`
    + `${ticksOf(c)}</svg>${mark}</div>`
    + `${right === undefined ? '' : `<span class="p-end">${esc(right)}</span>`}</div></div>`;
}

/** A readout: its label and its words, faint where the device says it is of no account. */
function readout(c) {
  return `<p class="p-row p-line">${esc(c.label)}: <span data-readout="${esc(c.key)}"`
    + `${c.tone === 'faint' ? ' class="faint"' : ''}>${esc(c.text)}</span></p>`;
}

/** One control, by its kind. */
export function control(c, inert) {
  switch (c.kind) {
    case 'choice': return c.style === 'list' ? list(c, inert) : segments(c, inert);
    case 'toggle': return toggle(c, inert);
    case 'slider': return slider(c, inert);
    default: return readout(c);
  }
}

/** The controls section for the shown device; nothing for a device with none. */
export function controls(_page, device, _view) {
  const all = device?.controls ?? [];
  if (all.length === 0) return '';
  const inert = Boolean(device.inert);
  return `<h2 class="p-label p-head">${esc(WORDS.controlsLabel)}</h2>`
    + `${all.map((c) => control(c, inert)).join('')}`;
}
