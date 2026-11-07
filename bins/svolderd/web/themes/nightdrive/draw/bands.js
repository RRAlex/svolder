/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The band table of the sheet: each band's frequency, gain and Q between a `−` and a `+`, the value
// a field that takes a typed one (the keyboard's way to every band: the bubbles on the mountains are
// not focusable); and, for the computer's EQ, its points: a type, a frequency, a gain and a Q each in a
// field, a `×` to remove it, `+ Add point` and the line that counts them. Pure: the model in, markup out.

import { TYPES, WORDS } from '../points.js';
import { SHEET } from '../words.js';
import { button, chip } from './modes.js';
import { esc } from './esc.js';

const PARAMS = ['f', 'g', 'q'];

/** One value between its steppers. */
function cell(inert, r, param) {
  const at = `data-band="${r.band}" data-param="${param}"`;
  const id = `nd-b${r.band}${param}`;
  const step = (delta, sign, words) => button('band', sign, inert,
    ` id="${id}${delta < 0 ? 'm' : 'p'}" data-step="${delta}" ${at} aria-label="${esc(words)}"`, 'nd-st');
  return `<td data-head="${esc(SHEET.bandHeads[1 + PARAMS.indexOf(param)])}"><span class="nd-stepper">`
    + `${step(-1, SHEET.minus, SHEET.lower(r.band, param))}`
    + `<input id="${id}" class="nd-v${inert ? ' quiet' : ''}" type="text" inputmode="decimal" autocomplete="off" `
    + `data-act="band-type" ${at} value="${esc(r[param])}" aria-label="${esc(SHEET.field(r.band, param))}"`
    + `${inert ? ' readonly aria-disabled="true"' : ''}>`
    + `${step(1, SHEET.plus, SHEET.raise(r.band, param))}</span></td>`;
}

/** The band table of `device`; nothing for a device with no bands. */
export function bandTable(device, inert = false) {
  const t = device?.tone;
  if (!t?.bands?.rows?.length) return '';
  const mode = t.modes?.find((m) => m.on)?.label ?? '';
  const heads = SHEET.bandHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('');
  const rows = t.bands.rows.map((r) => `<tr data-band="${r.band}"><th scope="row">${esc(r.band)}</th>`
    + `${PARAMS.map((p) => cell(inert, r, p)).join('')}</tr>`).join('');
  return `<h2 class="nd-lbl">${esc(SHEET.bands)}${mode ? ` · ${esc(mode)}` : ''}</h2>`
    + `<table id="nd-bands-table" class="nd-bandtable${t.bands.faint ? ' faint' : ''}">`
    + `<thead><tr>${heads}</tr></thead><tbody>${rows}</tbody></table>`
    + `<p class="nd-note">${esc(SHEET.bandNote)}</p>`;
}

/** The rows of the points' table in view before it scrolls. */
export const ROWS_SHOWN = 6;

/** One row of the points' table: its type, its three values as fields, and the `×` that removes it. */
function pointRow(point, inert) {
  const n = point.n;
  const options = TYPES.map((t) => `<option${t === point.type ? ' selected' : ''}>${t}</option>`).join('');
  const type = `<select id="nd-p${n}t" class="nd-ptype" data-act="point-type" data-point="${n}" aria-label="${esc(SHEET.pointType(n))}"`
    + `${inert ? ' disabled aria-disabled="true"' : ''}>${options}</select>`;
  const value = (param) => `<td data-head="${esc(SHEET.pointHeads[2 + ['f', 'g', 'q'].indexOf(param)])}">`
    + `<input id="nd-p${n}${param}" class="nd-v nd-pv${inert ? ' quiet' : ''}" type="text" inputmode="decimal" autocomplete="off" `
    + `data-act="point-field" data-point="${n}" data-param="${param}" value="${esc(WORDS[param](point[param]))}" `
    + `aria-label="${esc(SHEET.pointField(n, param))}"${inert ? ' readonly aria-disabled="true"' : ''}></td>`;
  const remove = button('point-remove', '×', inert, ` id="nd-px-${n}" data-point="${n}" aria-label="${esc(SHEET.removePoint(n))}"`, 'nd-px');
  return `<tr data-point="${n}"><th scope="row">${n}</th><td data-head="${esc(SHEET.pointHeads[1])}">${type}</td>`
    + `${value('f')}${value('g')}${value('q')}<td class="nd-pxc">${remove}</td></tr>`;
}

/** The points of the computer's EQ (core `toneOf`): the table, which shows `ROWS_SHOWN` rows and scrolls, the words
 * for the rows beyond them, `+ Add point` and the line that counts the points and the preamp. */
export function pointsTable(tone, inert = false) {
  if (!tone) return '';
  const heads = SHEET.pointHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('');
  const rows = tone.points.map((p) => pointRow(p, inert)).join('');
  const long = tone.points.length > ROWS_SHOWN;
  const add = chip('point-add', SHEET.addPoint, inert || !tone.canAdd, ' id="nd-point-add"');
  const unconfirmed = tone.verified ? '' : ` <span class="nd-unsure">${esc(SHEET.unconfirmed)}</span>`;
  return `<h2 class="nd-lbl">${esc(SHEET.points)}</h2>`
    + `<div id="nd-pscroll" class="nd-pscroll${long ? ' long' : ''}"><table id="nd-points-table" class="nd-ptable">`
    + `<thead><tr>${heads}<th></th></tr></thead><tbody>${rows}</tbody></table></div>`
    + `<p id="nd-more" class="nd-more">${esc(SHEET.moreBelow(tone.points.length - ROWS_SHOWN))}</p>`
    + `<div class="nd-set">${add}</div>`
    + `<p class="nd-note nd-pline">${esc(tone.line)}${unconfirmed}</p>`;
}

