/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The PipeWire config popup's draws: a model in, a string out. Pure, with no page and no core.
//
// The popup prints the daemon's words and judges nothing: the outcome of the check, the summary,
// the limit, the sink and every line (its label, its outcome and its words) come from the card's
// `path.config` (`SetupModel`, the one judge of svolder-ipc), and the draw only sets them out.
// All seven lines are always listed, the sink first, then the summary, the lines and the limit.
// A card without `config` (a sink that is not the DAC, or no route) says `config not read`.
// The model:
//
//   setup   `{ outcome, chip, summary, limit, sink, lines: [{ key, label, outcome, words }] }`, or
//           `null`
//
// A line's row carries its key and outcome (`data-key`, `data-outcome`), so a check reads what
// the popup shows against the card.

import { esc } from '../../html.js';
import { dot, heading, rows, tag } from '../frame.js';

/** The popup's own words: what the daemon did not send. */
export const WORDS = {
  title: 'PipeWire config',
  sink: 'Sink',
  unread: 'config not read',
  fails: 'fails',
  passes: 'passes',
};

/** The config check the page holds for the shown device, or `null`: the card's path's. */
export const setupOf = (page) => page?.device?.playing?.path?.config ?? null;

/** The dot of a check's outcome: `untouched` is on, `changes` warns, anything else is off. */
const SUMMARY_DOT = { untouched: 'on', changes: 'warn' };

/** The dot of a line's outcome, and the words a screen reader reads for it: only a passing line's
 * dot speaks, a failing line carries the tag `fails` and an unread one says so in its words, so
 * their dots are ornaments. */
const LINE_DOT = {
  pass: ['on', WORDS.passes],
  fail: ['warn', null],
};

/** One line as a row: the dot and the label, `fails` as a tag, then the daemon's words. */
function lineRow(line) {
  const [state, said] = LINE_DOT[line.outcome] ?? ['off', null];
  const fail = line.outcome === 'fail';
  return {
    attrs: ` data-key="${esc(line.key)}" data-outcome="${esc(line.outcome)}"`,
    label: `<span class="fpk-lab">${dot(state, said)}${esc(line.label)}${fail ? tag(WORDS.fails) : ''}</span>`,
    value: fail ? `<span class="fpk-fail">${esc(line.words)}</span>` : esc(line.words),
  };
}

/** The popup's page: the sink, the summary, the seven lines, the limit; or `config not read`. */
export function pageHtml({ setup = null }) {
  if (!setup) return `<p class="fp-note" data-setup="none">${esc(WORDS.unread)}</p>`;
  const summary = `<p class="fpk-sum" data-outcome="${esc(setup.outcome)}">`
    + `${dot(SUMMARY_DOT[setup.outcome] ?? 'off')}${esc(setup.summary)}</p>`;
  return `${heading(WORDS.sink)}<p class="fpk-sink">${esc(setup.sink)}</p>${summary}`
    + `${rows((setup.lines ?? []).map(lineRow))}${setup.limit ? `<p class="fp-note">${esc(setup.limit)}</p>` : ''}`;
}
