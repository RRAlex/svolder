/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The card in the middle of the page while the shown device is out of reach: what is wrong in
// words, `Try now` where the daemon offers it and the seconds to the next try. Pure: the model's
// card in, markup out; dash-wire.js counts the seconds down from `data-secs` and `data-drawn`.

import { esc } from '../../../core/html.js';
import { STATES } from '../states.js';

/** The card's words from its spans (a command in mono), or its plain detail. */
function detailOf(held) {
  if ((held.spans ?? []).length > 0) {
    return held.spans.map((s) => (s.code ? `<code>${esc(s.text)}</code>` : esc(s.text))).join('');
  }
  return esc(held.detail ?? '');
}

/**
 * The card `held` as markup: its title, its words, `Try now` and the seconds to the next try
 * (`retry_in`). `quiet` draws `Try now` for a viewer who may not write (a tap still reaches the
 * core, which says why nothing happens); `now` is the clock (ms) the seconds were worked out
 * against.
 */
export function drawCard(held, { quiet = false, now = 0 } = {}) {
  if (!held) return '';
  const body = held.spans?.length || held.detail
    ? `<p class="vg-detail">${detailOf(held)}</p>` : '';
  const retry = typeof held.retry_in === 'number'
    ? `<span class="vg-retry" data-secs="${esc(held.retry_in)}" data-drawn="${esc(now)}">`
      + `${esc(STATES.retryIn(held.retry_in))}</span>` : '';
  const act = held.try_now
    ? `<button type="button" id="vg-try" class="vg-k cy${quiet ? ' quiet' : ''}" data-act="try"`
      + `${quiet ? ' aria-disabled="true"' : ''}>${esc(STATES.tryNow)}</button>` : '';
  return `<section id="vg-cardbox" class="vg-cardbox" role="status"><h2>${esc(held.title)}</h2>`
    + body
    + `${act || retry ? `<p class="vg-tryline">${act}${retry}</p>` : ''}</section>`;
}
