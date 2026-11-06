/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The controls the model lists, as the sheet draws them: toggles as pills, choices as chips (a long
// list as one chip that opens it), balance and the other levels as scales with a knob, readouts as
// their words; and the device's power, with its switches and readouts, which the dash keeps only the
// key of. The input is the sheet's own row, so it is not drawn here. Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { esc } from './esc.js';
import { button, chip, idOf } from './modes.js';

/** Whether the sheet draws the control `c` in its own row (the input), so Controls does not. */
export const drawnAbove = (c) => c.key === 'input' && c.kind === 'choice';

/** The fraction (0 to 1) of the scale `c` that step `raw` stands at. */
export const fractionOf = (c, raw) => (c.hi > c.lo ? Math.min(1, Math.max(0, (raw - c.lo) / (c.hi - c.lo))) : 0);

/** The step a fraction of the scale `c` is, within its range. */
export const rawAt = (c, fraction) => Math.min(c.hi, Math.max(c.lo, Math.round(c.lo + fraction * (c.hi - c.lo))));

/** A switch: its label and a pill that reads `On`, `Off` or a dash where the device has not said. */
function toggle(c, inert) {
  const said = c.on === true ? SHEET.on : c.on === false ? SHEET.off : SHEET.unknown;
  return `<div class="nd-ctl"><span class="nd-name">${esc(c.label)}</span><div class="nd-set">`
    + `${chip('toggle', said, inert, ` id="${idOf('nd-t', c.key)}" data-key="${esc(c.key)}"`, c.on === true)}`
    + '</div></div>';
}

/** A choice: its label and its options as chips, or the one that is on as a chip that opens the list. */
function choice(c, inert) {
  const options = c.options ?? [];
  const body = c.style === 'list'
    ? chip('choice-list', `${options.find((o) => o.on)?.label ?? '-'} ▾`, inert,
      ` id="${idOf('nd-l', c.key)}" data-key="${esc(c.key)}" aria-haspopup="listbox"`)
    : options.map((o, i) => chip('choice', o.label, inert,
      ` id="${idOf('nd-c', c.key, `-${i}`)}" data-key="${esc(c.key)}" data-value="${esc(o.label)}"`, o.on)).join('');
  return `<div class="nd-ctl"><span class="nd-name">${esc(c.label)}</span><div class="nd-set">${body}</div></div>`;
}

/** A level: its label and value, a scale with a knob where the device reads and a tick at its centre. A
 * tap on the value writes the centre where the level has one. */
function slider(c, inert) {
  const read = typeof c.raw === 'number';
  const [left, right] = c.ends ?? [];
  const knob = read ? `<span class="nd-knob" data-at="${fractionOf(c, c.raw).toFixed(5)}"></span>` : '';
  const centre = typeof c.centre === 'number'
    ? `<span class="nd-ctr" data-at="${fractionOf(c, c.centre).toFixed(5)}"></span>` : '';
  const value = typeof c.centre === 'number'
    ? button('centre', c.text, inert, ` id="${idOf('nd-ce', c.key)}" data-key="${esc(c.key)}"`, 'nd-link nd-val')
    : `<span class="nd-val">${esc(c.text)}</span>`;
  const end = (text) => (text === undefined ? '' : `<span class="nd-end">${esc(text)}</span>`);
  return `<div class="nd-level"><div class="nd-lrow"><span class="nd-name">${esc(c.label)}</span>${value}</div>`
    + `<div class="nd-sbox">${end(left)}`
    + `<div id="${idOf('nd-s', c.key)}" class="nd-scale${inert ? ' quiet' : ''}" role="slider" tabindex="0" `
    + `data-key="${esc(c.key)}" aria-label="${esc(c.label)}" aria-valuemin="${esc(c.lo)}" `
    + `aria-valuemax="${esc(c.hi)}"${read ? ` aria-valuenow="${esc(c.raw)}"` : ''} `
    + `aria-valuetext="${esc(c.text)}"${inert ? ' aria-disabled="true"' : ''}>${centre}${knob}</div>`
    + `${end(right)}</div></div>`;
}

/** A readout: its label and its words, faint where the device says it is of no account. */
function readout(c) {
  return `<div class="nd-ctl nd-read"><span class="nd-name">${esc(c.label)}</span>`
    + `<span data-readout="${esc(c.key)}" class="nd-rv${c.tone === 'faint' ? ' faint' : ''}">${esc(c.text)}</span></div>`;
}

/** One control, by its kind. */
export function control(c, inert) {
  switch (c.kind) {
    case 'choice': return choice(c, inert);
    case 'toggle': return toggle(c, inert);
    case 'slider': return slider(c, inert);
    default: return readout(c);
  }
}

/** The device's power: the switch with the word the device says (`On`, `Standby`, `Warming up`), then its
 * switches as pills and its readouts. Nothing for a device without power. */
function power(device, inert) {
  const p = device.power;
  if (!p) return '';
  const said = p.word ?? (p.on === true ? SHEET.on : p.on === false ? SHEET.off : SHEET.unknown);
  const main = `<div class="nd-ctl"><span class="nd-name">${esc(SHEET.power)}</span><div class="nd-set">`
    + `${chip('power', said, inert, ` id="nd-power-key" data-key="${esc(p.key)}"`, p.on === true)}</div></div>`;
  const switches = (p.toggles ?? []).map((c) => `<div class="nd-ctl"><span class="nd-name">${esc(c.label)}</span>`
    + `<div class="nd-set">${chip('power-toggle', c.on === true ? SHEET.on : c.on === false ? SHEET.off : SHEET.unknown,
      inert, ` id="${idOf('nd-pt', c.key)}" data-key="${esc(c.key)}"`, c.on === true)}</div></div>`).join('');
  return `${main}${switches}${(p.readouts ?? []).map(readout).join('')}`;
}

/** The sheet's Controls: the device's power, then every control the model lists but the input. Nothing for a
 * device with none. `inert` (a guest, a page whose writes are off) draws the words quiet. */
export function drawControls(device, view = {}) {
  if (!device) return '';
  const inert = Boolean(view.inert ?? device.inert);
  const all = (device.controls ?? []).filter((c) => !drawnAbove(c));
  const body = power(device, inert) + all.map((c) => control(c, inert)).join('');
  return body ? `<h2 class="nd-lbl">${esc(SHEET.controls)}</h2>${body}` : '';
}
