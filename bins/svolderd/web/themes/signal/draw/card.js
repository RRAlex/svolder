/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The card of a device that cannot be used now (out of reach, looking, refused, faulted) and of a
// page that waits or rests (P6): what is wrong in words, the faults with their advice, `Try now`
// where the daemon offers it and the seconds to the next try. Pure: the model in, markup out; the
// seconds are counted down by card-wire.js from `data-secs` and `data-drawn`.

import { STATES } from '../words.js';
import { esc } from './esc.js';
import { button } from './word.js';

/** The card's words from its spans (code in mono), or its plain detail. */
function detailOf(held) {
  if ((held.spans ?? []).length > 0) {
    return held.spans.map((s) => (s.code ? `<code>${esc(s.text)}</code>` : esc(s.text))).join('');
  }
  return esc(held.detail ?? '');
}

/** The card the page has to show for the device: the device's own, else the page's (no device yet). */
export const heldCard = (page) => page.device?.card ?? page.card ?? null;

/** The card of a page that reads the device again (`waiting`) or rests (`resting`): its own words. */
export function stateCard(state) {
  if (state === 'waiting') return { title: STATES.waiting, detail: '', kind: 'wait' };
  if (state === 'resting') return { title: STATES.resting, detail: STATES.restingLine, kind: 'wait' };
  return null;
}

/**
 * The card `held` as markup: the title, the words or the faults with their advice, `Try now` and the
 * seconds to the next try. `kind` `wait` is amber, any other red; `quiet` draws `Try now` for a viewer
 * who may not write; `now` is the clock the seconds were worked out against (ms).
 */
export function card(held, { quiet = false, now = 0 } = {}) {
  if (!held) return '';
  const faults = held.faults ?? [];
  const body = faults.length === 0
    ? (held.spans?.length || held.detail ? `<p class="sg-detail">${detailOf(held)}</p>` : '')
    : `<ul class="sg-advice">${faults.map((x) => `<li><b>${esc(x.label)}</b>`
      + `${x.advice ? ` ${esc(x.advice)}` : ''}</li>`).join('')}</ul>`;
  const retry = typeof held.retry_in === 'number'
    ? `<span class="sg-retry" data-secs="${esc(held.retry_in)}" data-drawn="${esc(now)}">`
      + `${esc(STATES.retryIn(held.retry_in))}</span>` : '';
  const act = held.try_now ? button('try', STATES.tryNow, quiet, ' id="sg-try"', 'sg-btn sg-pill') : '';
  const tone = held.kind === 'wait' ? 'sg-wait' : 'sg-bad';
  return `<section id="sg-card" class="sg-card ${tone}" role="status"><h2>${esc(held.title)}</h2>${body}`
    + `${act || retry ? `<p class="sg-tryline">${act}${retry}</p>` : ''}</section>`;
}
