/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The line at the foot of the sheet that opens the device's page in the Devices popup (its facts
// and readings are that page's), and the card for a device out of reach or at fault, or for a
// page with no device. Pure: the model in, markup out.

import { mayWrite } from '../../../core/write.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

/** *Device settings…*, the one line the sheet keeps of the device's facts: it opens the device's
 * page in the Devices popup, where the core draws the facts, the readings and, for a writer,
 * *Identify*. A viewer who may not write reads *Devices…*: the same page, read-only. */
export function device(page, dev) {
  if (!dev) return '';
  const words = mayWrite(page) ? WORDS.deviceSettings : WORDS.deviceFacts;
  return `<p class="p-devset">${word('device-settings', words, false)}</p>`;
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
