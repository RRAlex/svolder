/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The device's facts: a spec table, every reading the device holds with how long ago it was heard,
// and Identify, under a title that folds them away (folded unless the person opened them). Pure:
// the model in, markup out; the ages are written once here and kept current by facts-wire.js.

import { ageWords } from '../../../core/words.js';
import { SHELF } from '../words.js';
import { esc } from './esc.js';
import { button } from './word.js';

/** A row of the spec table: its label, then its value in the cell that names the fact. */
const row = (label, value, attrs = '', cls = '') => `<tr><th scope="row">${esc(label)}</th>`
  + `<td data-fact="${esc(label)}"${cls ? ` class="${cls}"` : ''}${attrs}>${value}</td></tr>`;

/** The seconds from `heardAt` to `nowMs`, both by the daemon's clock. */
const since = (nowMs, heardAt) => (nowMs - heardAt) / 1000;

/** The facts of `device`, folded unless `view.facts.open`; nothing for a device that holds none. `own.identify`
 * leaves Identify out (the open area has it). */
export function facts(_page, device, view, own = {}) {
  const f = device?.facts;
  if (!f) return '';
  const age = f.signal_age;
  const heard = age
    ? ` data-head="${esc(age.head)}" data-heard-at="${esc(age.heard_at_ms)}"${age.live ? ' data-live' : ''}`
    : '';
  const spec = [
    row('Product', esc(SHELF.product(f.product, f.product_id))),
    f.unit ? row('Unit', esc(f.unit), '', 'mono') : '',
    row('Firmware', esc(f.firmware)),
    row('DSP', f.dsp == null ? SHELF.unknown : esc(f.dsp)),
    row('Profile', esc(f.profile)),
    row('Link', esc(f.link)),
    row('Signal', esc(f.signal), heard),
    f.passphrase_note == null ? '' : row('Passphrase', esc(f.passphrase_note), '', 'warn'),
  ].join('');
  const readings = (f.rows ?? []).map((r) => {
    const when = r.heard_at_ms == null
      ? `<td class="sg-age">${SHELF.unknown}</td>`
      : `<td class="sg-age" data-heard-at="${esc(r.heard_at_ms)}">${esc(SHELF.ago(
        ageWords(since(f.now_ms, r.heard_at_ms))))}</td>`;
    return `<tr><th scope="row">${esc(r.label)}</th><td data-fact="${esc(r.id)}">${esc(r.value)}</td>${when}</tr>`;
  }).join('');
  const identify = f.identify_offered && !own.identify
    ? `<div class="sg-set">${button('identify', SHELF.identify, !f.identify || Boolean(device.inert),
      ' id="sg-identify"', 'sg-btn sg-pill')}</div><p class="sg-note">${esc(SHELF.identifyNote)}</p>`
    : '';
  const open = Boolean(view?.facts?.open);
  return `<h2 class="sg-lbl sg-foldhead"><button type="button" id="sg-facts-fold" class="sg-fold" `
    + `data-act="facts-fold" aria-expanded="${open}" aria-controls="sg-facts-body">`
    + `<span class="sg-caret" aria-hidden="true">${open ? '▾' : '▸'}</span>`
    + `${esc(SHELF.factsTitle(device.name))}</button> <span class="sg-note">`
    + `${esc(open ? SHELF.factsHide : SHELF.factsShow)}</span></h2>`
    + `<div id="sg-facts-body"${open ? '' : ' hidden'}>`
    + `<table id="sg-spec" class="sg-spec" data-now="${esc(f.now_ms)}"><tbody>${spec}</tbody></table>`
    + `<h2 class="sg-lbl">${esc(SHELF.readings)}</h2>`
    + `<table id="sg-readings" class="sg-spec"><tbody>${readings}</tbody></table>${identify}</div>`;
}
