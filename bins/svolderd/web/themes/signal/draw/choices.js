/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The choices of a DAC, as chips: the input in the shelf under its band table (a few values are
// chips of their own, a longer list is one chip that opens a list), and the tone mode under the
// curve, whose Loudness button carries the strength's round `−` and `+` while it is on, as Ember's
// does. Pure: the model in, markup out.

import { SHELF, WORDS } from '../words.js';
import { esc } from './esc.js';
import { button } from './word.js';

/** An id made from a control's key and a suffix: what keeps the focus when the area is drawn
 * again. */
export const idOf = (prefix, key, more = '') => `${prefix}-${String(key).replace(/[^\w-]/g, '_')}${more}`;

export const chip = (act, text, inert, attrs, on) => button(act, text, inert,
  `${attrs}${on === undefined ? '' : ` aria-pressed="${on}"`}`, `sg-chip${on ? ' on' : ''}`);

/** A choice control as chips (a few values) or as one chip that opens a list (a longer one), under
 * its label. */
export function choiceControl(c, inert) {
  const options = c.options ?? [];
  const body = c.style === 'list'
    ? chip('choice-list', `${options.find((o) => o.on)?.label ?? '-'} ▾`, inert,
      ` id="${idOf('sg-l', c.key)}" data-key="${esc(c.key)}" aria-haspopup="listbox"`)
    : options.map((o, i) => chip('choice', o.label, inert,
      ` id="${idOf('sg-c', c.key, `-${i}`)}" data-key="${esc(c.key)}" data-value="${esc(o.label)}"`,
      o.on)).join('');
  return `<div><span class="sg-lbl">${esc(c.label ?? WORDS.input)}</span><div class="sg-set">${body}</div></div>`;
}

/** Whether the first screen draws the control `c` (the input), so the shelf does not. */
export const drawnAbove = (c) => c.key === 'input' && c.kind === 'choice';

/** The input: the control `key` `input`. */
function input(device, inert) {
  const c = (device.controls ?? []).find(drawnAbove);
  return c ? choiceControl(c, inert) : '';
}

/** The Loudness chip while it is on: the round `−` and `+` over its ends, the strength under its
 * name (buttons cannot hold buttons, so the three sit in one wrap). */
function loudChip(m, tone, inert) {
  const step = (delta, sign, words) => button('loudness', sign, inert,
    ` id="sg-loud${delta < 0 ? 'm' : 'p'}" data-step="${delta}" aria-label="${esc(words)}"`, 'sg-st sg-rb');
  const name = `${esc(m.label)}<small class="sg-lv">${esc(tone.strength ?? SHELF.unknown)}</small>`;
  return `<span id="sg-loud" class="sg-loudw">${step(-1, SHELF.minus, SHELF.lowerLoudness)}`
    + `${button('mode', name, inert, ` id="sg-mode-${esc(m.name)}" data-mode="${esc(m.name)}" aria-pressed="true"`,
      'sg-chip on sg-loudc', true)}`
    + `${step(1, SHELF.plus, SHELF.raiseLoudness)}</span>`;
}

/** The tone mode under the curve: a chip for each mode the device has, Loudness with its strength's
 * steppers while it is on. */
export function modeRow(device) {
  const tone = device?.tone;
  if (!tone?.modes?.length) return '';
  const inert = Boolean(device.inert);
  const chips = tone.modes.map((m) => (m.on === true && tone.loud ? loudChip(m, tone, inert)
    : chip('mode', m.label, inert, ` id="sg-mode-${esc(m.name)}" data-mode="${esc(m.name)}"`, m.on === true)))
    .join('');
  return `<div id="sg-mode" class="sg-mode"><span class="sg-lbl">${esc(WORDS.mode)}</span>`
    + `<div class="sg-set">${chips}</div></div>`;
}

/** The choices of the shown device. */
export function choices(device) {
  if (!device) return '';
  const inert = Boolean(device.inert);
  return input(device, inert);
}
