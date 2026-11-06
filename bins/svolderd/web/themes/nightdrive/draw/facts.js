/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The device's facts: a spec table, every reading the device holds with how long ago it was heard, and
// Identify, under a title that folds them away (folded unless the person opened them); and the faults an
// amp reports, each with its advice. Pure: the model in, markup out; the ages are written once here and
// kept current by facts-wire.js.

import { ageWords } from '../../../core/words.js';
import { SHEET } from '../words.js';
import { esc } from './esc.js';
import { button } from './modes.js';

/** A row of the spec table: its label, then its value in the cell that names the fact. */
const row = (label, value, attrs = '', cls = '') => `<tr><th scope="row">${esc(label)}</th>`
  + `<td data-fact="${esc(label)}"${cls ? ` class="${cls}"` : ''}${attrs}>${value}</td></tr>`;

/** The seconds from `heardAt` to `nowMs`, both by the daemon's clock. */
const since = (nowMs, heardAt) => (nowMs - heardAt) / 1000;

/** The faults the device reports: the facts' rows of the fault family (ids `230.…`: over-current,
 * over-temperature) with their value, and the advice of the card's faults. Nothing for none. */
export function drawFaults(device) {
  const rows = (device?.facts?.rows ?? []).filter((r) => /^230\./.test(r.id));
  const advice = device?.card?.faults ?? [];
  if (rows.length === 0 && advice.length === 0) return '';
  const list = rows.map((r) => `<dt>${esc(r.label)}</dt><dd data-fact="${esc(r.label)}"`
    + `${r.value === 'active' ? ' class="bad"' : ''}>${esc(r.value)}</dd>`).join('');
  const tips = advice.map((x) => `<li><b>${esc(x.label)}</b> ${esc(x.advice)}</li>`).join('');
  return `<h2 class="nd-lbl bad">${esc(SHEET.faults)}</h2>${list ? `<dl class="nd-faults">${list}</dl>` : ''}`
    + `${tips ? `<ul class="nd-advice">${tips}</ul>` : ''}`;
}

/** The facts of `device`, folded unless `open`; nothing for a device that holds none. `inert` (a guest, a page
 * whose writes are off) draws Identify quiet. */
export function drawFacts(device, open = false, inert = Boolean(device?.inert)) {
  const f = device?.facts;
  if (!f) return '';
  const age = f.signal_age;
  const heard = age
    ? ` data-head="${esc(age.head)}" data-heard-at="${esc(age.heard_at_ms)}"${age.live ? ' data-live' : ''}`
    : '';
  const spec = [
    row('Product', esc(SHEET.product(f.product, f.product_id))),
    f.unit ? row('Unit', esc(f.unit), '', 'mono') : '',
    row('Firmware', esc(f.firmware)),
    row('DSP', f.dsp == null ? SHEET.unknown : esc(f.dsp)),
    row('Profile', esc(f.profile)),
    row('Link', esc(f.link)),
    row('Signal', esc(f.signal), heard),
    f.passphrase_note == null ? '' : row('Passphrase', esc(f.passphrase_note), '', 'warn'),
  ].join('');
  const readings = (f.rows ?? []).map((r) => {
    const when = r.heard_at_ms == null
      ? `<td class="nd-age">${SHEET.unknown}</td>`
      : `<td class="nd-age" data-heard-at="${esc(r.heard_at_ms)}">${esc(SHEET.ago(
        ageWords(since(f.now_ms, r.heard_at_ms))))}</td>`;
    return `<tr><th scope="row">${esc(r.label)}</th><td data-fact="${esc(r.id)}">${esc(r.value)}</td>${when}</tr>`;
  }).join('');
  const identify = f.identify_offered
    ? `<div class="nd-set">${button('identify', SHEET.identify, !f.identify || inert,
      ' id="nd-identify"', 'nd-btn')}</div><p class="nd-note">${esc(SHEET.identifyNote)}</p>`
    : '';
  return `<h2 class="nd-lbl nd-foldhead"><button type="button" id="nd-facts-fold" class="nd-fold" `
    + `data-act="facts-fold" aria-expanded="${open}" aria-controls="nd-facts-body">`
    + `<span class="nd-caret" aria-hidden="true">${open ? '▾' : '▸'}</span>`
    + `${esc(SHEET.factsTitle(device.name))}</button> <span class="nd-note">`
    + `${esc(open ? SHEET.factsHide : SHEET.factsShow)}</span></h2>`
    + `<div id="nd-facts-body"${open ? '' : ' hidden'}>`
    + `<table id="nd-spec" class="nd-spec" data-now="${esc(f.now_ms)}"><tbody>${spec}</tbody></table>`
    + `<h2 class="nd-lbl">${esc(SHEET.readings)}</h2>`
    + `<table id="nd-readings" class="nd-spec"><tbody>${readings}</tbody></table>${identify}</div>`;
}
