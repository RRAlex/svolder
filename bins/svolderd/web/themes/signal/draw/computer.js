/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the computer's hop opens (P5): the facts of the route the sound takes through the computer,
// read only. Nothing here acts: no button, nothing focusable. Pure: the model in, markup out.

import { COMPUTER, rateWords } from '../words.js';
import { esc } from './esc.js';

/** The row `label`: `text`, in the class `cls` when given. */
const row = (label, text, cls = '') => `<dt>${esc(label)}</dt><dd${cls ? ` class="${cls}"` : ''}>${esc(text)}</dd>`;

/** The sink's rate and format in words: `96 kHz S32LE`. */
const formatWords = (sink) => [rateWords(sink.rate_hz), sink.format].filter(Boolean).join(' ');

/** The host EQ's words: `off`, `on`, or `on · Harman` when the preset is named. */
function eqWords(route) {
  if (!route.host_eq) return COMPUTER.eqOff;
  return route.eq_preset ? `${COMPUTER.eqOn} · ${route.eq_preset}` : COMPUTER.eqOn;
}

/** The path verdict, from the path the shown device reads (its hops' `computer` entry and the words),
 * else nothing. */
function verdict(path) {
  if (!path?.words) return '';
  return row(COMPUTER.path, path.words, path.class === 'ok' ? 'sg-ok' : 'sg-warn');
}

/** The open area of the computer hop for `page`: the route's facts, or `route not read`. */
export function computerScreen(page) {
  const route = page.host?.route;
  const path = page.device?.playing?.path ?? page.host?.path ?? null;
  const head = `<h2>${esc(COMPUTER.title)}</h2>`;
  if (!route) {
    return `<section id="sg-computer" class="sg-plain">${head}<p class="sg-detail">${esc(COMPUTER.unread)}</p></section>`;
  }
  const sink = route.sink;
  const rows = sink
    ? [row(COMPUTER.sink, sink.words),
      row(COMPUTER.format, formatWords(sink) || '—'),
      row(COMPUTER.volume, sink.volume_pct === null || sink.volume_pct === undefined ? '—' : `${sink.volume_pct} %`),
      row(COMPUTER.muted, sink.muted ? COMPUTER.yes : COMPUTER.no)]
    : [row(COMPUTER.sink, COMPUTER.noSink)];
  rows.push(row(COMPUTER.hostEq, eqWords(route), route.host_eq ? 'sg-warn' : ''));
  rows.push(row(COMPUTER.others, route.others_mixing > 0 ? COMPUTER.mixing(route.others_mixing) : COMPUTER.alone));
  const way = verdict(path);
  return `<section id="sg-computer" class="sg-plain">${head}<dl class="sg-facts">${rows.join('')}${way}</dl>`
    + `<p class="sg-note">${esc(COMPUTER.note)}</p></section>`;
}
