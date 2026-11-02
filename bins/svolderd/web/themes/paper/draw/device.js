/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The device's facts as a spec table, every reading it holds with how long ago it was heard, and
// the Identify word, under a heading that folds them away (folded unless the person opened them).
// The card for a device out of reach or at fault, or for a page with no device, is here too. Pure:
// the model in, markup out; the ages are written once here and kept current by device-wire.js.

import { ageWords } from '../../../core/words.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

/** A row of the spec table: its label, then its value in the cell that names the fact. */
const row = (label, value, attrs = '', cls = '') => `<tr><th scope="row">${esc(label)}</th>`
  + `<td data-fact="${esc(label)}"${cls ? ` class="${cls}"` : ''}${attrs}>${value}</td></tr>`;

/** The seconds from `heardAt` to `nowMs`, both by the daemon's clock. */
const since = (nowMs, heardAt) => (nowMs - heardAt) / 1000;

/** The device's facts: the spec table, all the readings, and Identify where it is offered. */
export function device(_page, dev, view) {
  const f = dev?.facts;
  if (!f) return '';
  const age = f.signal_age;
  const signal = age
    ? ` data-head="${esc(age.head)}" data-heard-at="${esc(age.heard_at_ms)}"${age.live ? ' data-live' : ''}`
    : '';
  const spec = [
    row('Product', esc(WORDS.product(f.product, f.product_id))),
    f.unit ? row('Unit', esc(f.unit), '', 'mono') : '',
    row('Firmware', esc(f.firmware)),
    row('DSP', f.dsp == null ? WORDS.unknownWord : esc(f.dsp)),
    row('Profile', esc(f.profile)),
    row('Link', esc(f.link)),
    row('Signal', esc(f.signal), signal),
    f.passphrase_note == null ? '' : row('Passphrase', esc(f.passphrase_note), '', 'warn'),
  ].join('');
  const readings = (f.rows ?? []).map((r) => {
    const heard = r.heard_at_ms == null
      ? `<td class="p-age">${WORDS.unknownWord}</td>`
      : `<td class="p-age" data-heard-at="${esc(r.heard_at_ms)}">${esc(WORDS.ago(
        ageWords(since(f.now_ms, r.heard_at_ms))))}</td>`;
    return `<tr><th scope="row">${esc(r.label)}</th><td data-fact="${esc(r.id)}">${esc(r.value)}</td>${heard}</tr>`;
  }).join('');
  const identify = f.identify_offered
    ? `<p class="p-identify">${word('identify', WORDS.identify, !f.identify || Boolean(dev.inert))}`
      + ` <span class="p-note">${esc(WORDS.identifyNote)}</span></p>`
    : '';
  const open = Boolean(view?.deviceOpen);
  return `<h2 class="p-label p-head"><button type="button" class="p-w p-fold" data-act="device-fold"`
    + ` aria-expanded="${open}" aria-controls="p-device-body">${esc(WORDS.deviceLabel)}</button>`
    + ` <span class="p-note">${esc(open ? WORDS.deviceHide : WORDS.deviceShow)}</span></h2>`
    + `<div id="p-device-body"${open ? '' : ' hidden'}>`
    + `<table id="p-spec" class="p-spec" data-now="${esc(f.now_ms)}"><tbody>${spec}</tbody></table>`
    + `<h2 class="p-label p-head">${esc(WORDS.readingsLabel)}</h2>`
    + `<table id="p-readings" class="p-spec"><tbody>${readings}</tbody></table>${identify}</div>`;
}

/** The card's words from its spans (code in mono), or its plain detail. */
function detailOf(held) {
  if ((held.spans ?? []).length > 0) {
    return held.spans.map((s) => (s.code ? `<code>${esc(s.text)}</code>` : esc(s.text))).join('');
  }
  return esc(held.detail);
}

/** The card under the title: what the page has to say before the device can be used (not
 * connected, a fault, no device yet): its title as the red note, its words in print, the faults
 * with their advice, `try now` where offered and the seconds to the next try. */
export function card(page, dev, _view) {
  const held = dev?.card ?? page.card;
  if (!held) return '';
  const quiet = dev ? Boolean(dev.inert) : page.viewer?.writes === false;
  const faults = held.faults ?? [];
  const body = faults.length === 0
    ? (held.spans?.length || held.detail ? `<p class="p-detail">${detailOf(held)}</p>` : '')
    : `<ul class="p-faults">${faults.map((x) => `<li><b>${esc(x.label)}</b>`
      + `${x.advice ? ` ${esc(x.advice)}` : ''}</li>`).join('')}</ul>`;
  const retry = typeof held.retry_in === 'number'
    ? ` <span class="p-retry" data-secs="${esc(held.retry_in)}">${esc(WORDS.retryIn(held.retry_in))}</span>` : '';
  const act = held.try_now ? word('try', WORDS.tryNow, quiet, ' id="p-try"') : '';
  return `<div class="p-card"><p class="pn red card">${esc(held.title)}</p>${body}`
    + `${act || retry ? `<p class="p-detail p-tryline">${act}${retry}</p>` : ''}</div>`;
}
