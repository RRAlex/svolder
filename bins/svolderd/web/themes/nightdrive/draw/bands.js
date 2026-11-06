/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The band table of the sheet: each band's frequency, gain and Q between a `−` and a `+`, the value
// a field that takes a typed one (the keyboard's way to every band: the bubbles on the mountains are
// not focusable). Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { button } from './modes.js';
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
