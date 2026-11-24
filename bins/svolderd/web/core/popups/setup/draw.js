/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The PipeWire config popup's draws: a model in, a string out. Pure, with no page and no core.
//
// The popup prints the daemon's words and judges nothing: the outcome of the check, the summary,
// the limit, the sink and every line (its label, its outcome and its words) come from the card's
// `path.config` (`SetupModel`, the one judge of svolder-ipc), and the draw only sets them out.
// All seven lines are always listed, the sink first, then the summary, the lines and the limit.
// A card without `config` (a sink that is not the DAC, or no route) says `config not read`.
// Under the lines, before the limit, a section `What changes the sound` names every hop the Path
// row turns yellow, from the row's `changes` (the same model the row draws, so the two cannot
// disagree): one row per hop, its label and each word in the failing line's yellow when its tone
// is `warn` (dim for `dim`, plain otherwise), the separators dim. It is drawn only when the row
// carries a hop; its wrapper has `data-changes` and each row `data-hop` from the entry's `key`
// (`player`, `computer` or `dac`).
// The model:
//
//   setup   `{ outcome, chip, summary, limit, sink, from_player, to_dac,
//             lines: [{ key, label, outcome, words }] }`, or
//           `null`
//   changes `[{ key, label, words: [{ text, tone }] }]` (the row's `changes`), in path order
//
// Under the sink two rows give what comes in and what goes out, the daemon's words as they are:
// `From player` (the stream's rate, format and channels as PipeWire negotiated them) and `To DAC`
// (what the card is open at, `not open` or `not read`); their rows carry `data-io="from_player"`
// and `data-io="to_dac"`, and a missing one (an older daemon) is left out.
// A line's row carries its key and outcome (`data-key`, `data-outcome`), so a check reads what
// the popup shows against the card.

import { esc } from '../../html.js';
import { dot, heading, rows, tag } from '../frame.js';

/** The popup's own words: what the daemon did not send. */
export const WORDS = {
  title: 'PipeWire config',
  sink: 'Sink',
  fromPlayer: 'From player',
  toDac: 'To DAC',
  unread: 'config not read',
  fails: 'fails',
  passes: 'passes',
  changes: 'What changes the sound',
};

/** The config check the page holds for the shown device, or `null`: the card's path's. */
export const setupOf = (page) => page?.device?.playing?.path?.config ?? null;

/** The Path row's `changes` the page holds for the shown device, or an empty list. */
export const changesOf = (page) => page?.device?.playing?.path?.row?.changes ?? [];

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

/** The `From player` and `To DAC` rows under the sink; a line the daemon did not send is left
 * out. */
function ioHtml(setup) {
  const list = [['from_player', WORDS.fromPlayer], ['to_dac', WORDS.toDac]]
    .filter(([key]) => setup[key])
    .map(([key, label]) => ({
      attrs: ` data-io="${key}"`,
      label: `<span class="fpk-lab">${esc(label)}</span>`,
      value: esc(setup[key]),
    }));
  return list.length ? rows(list) : '';
}

/** One word in its tone: `warn` in the failing line's yellow, `dim` dimmed, any other plain. */
function wordHtml(word) {
  const cls = { warn: 'fpk-fail', dim: 'fpk-dim' }[word.tone];
  return cls ? `<span class="${cls}">${esc(word.text)}</span>` : esc(word.text);
}

/** The section of what changes the sound: its heading and one row per hop, the hop's label then
 * its words, a `warn` word in the failing line's yellow; nothing without a hop. */
function changesHtml(changes) {
  if (!changes?.length) return '';
  const list = rows(changes.map((hop) => ({
    attrs: ` data-hop="${esc(hop.key)}"`,
    label: `<span class="fpk-lab">${esc(hop.label)}</span>`,
    value: (hop.words ?? []).map(wordHtml).join('<span class="fpk-dim"> · </span>'),
  })));
  return `<div data-changes>${heading(WORDS.changes)}${list}</div>`;
}

/** The popup's page: the sink with what comes in and goes out, the summary, the seven lines, what
 * changes the sound, the limit; or `config not read`. */
export function pageHtml({ setup = null, changes = [] }) {
  if (!setup) return `<p class="fp-note" data-setup="none">${esc(WORDS.unread)}</p>`;
  const summary = `<p class="fpk-sum" data-outcome="${esc(setup.outcome)}">`
    + `${dot(SUMMARY_DOT[setup.outcome] ?? 'off')}${esc(setup.summary)}</p>`;
  const sink = `${heading(WORDS.sink)}<p class="fpk-sink">${esc(setup.sink)}</p>`;
  return `${sink}${ioHtml(setup)}${summary}`
    + `${rows((setup.lines ?? []).map(lineRow))}${changesHtml(changes)}${setup.limit ? `<p class="fp-note">${esc(setup.limit)}</p>` : ''}`;
}
