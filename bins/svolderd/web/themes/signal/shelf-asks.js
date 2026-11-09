/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the shelf's presets share, for the DAC's and for the computer's: where a form or a question
// opens (the ask of the section the word is in), the shelf held while one is open, a list of the
// person's own presets to choose from, and the two flows that rename or delete one. The writes are
// each side's own: they are given as `submit(text)` and `go()`.

import { askForm, askQuestion } from './ask.js';
import { closeOf, openList } from './lists.js';
import { SHELF } from './words.js';

/** The asks of the shelf `shelf` (its holds are `keep`, `release()` gives one back once the press
 * that closed a list is over) until `signal` aborts. */
export function shelfAsks({
  shelf, keep, release, signal,
}) {
  /** The place a form or a question opens in: the ask of the section the word is in, found now (the
   * shelf may have been drawn again since the word was). */
  const within = (el) => el.closest('.sg-box2')?.querySelector('.sg-ask')
    ?? shelf.querySelector('.sg-ask');

  /** Hold the shelf while an ask is open; the ask's `onClose` gives the hold back. */
  const ask = (open) => keep.around(() => open(() => keep.dropLater()));

  /** Open `list` under `opener` holding the shelf; `then` hears the value chosen. */
  function pick(opener, label, options, then) {
    if (closeOf(opener)) return;
    keep.around(() => openList({
      opener, label, options, choose: then, signal, onClose: release,
    }));
  }

  return {
    within,
    ask,
    pick,
    /** Rename or delete: the preset the curve is on if it is the person's own, else a list of the
     * person's own (`own`, `[{ name }]`) to choose from. */
    ownPreset(el, own, current, label, then) {
      if (own.some((p) => p.name === current)) {
        then(current);
        return;
      }
      pick(el, label, own.map((p) => ({ value: p.name, text: p.name, on: false })), then);
    },
    /** Ask for a new name of the preset `from`; `submit(text)` renames it. */
    rename(el, from, submit) {
      ask((onClose) => askForm({
        within: within(el), kind: 'rename', value: from, signal, opener: el, onClose, submit,
      }));
    },
    /** Ask whether to delete the preset `name`; `go()` deletes it, only on the answer. */
    remove(el, name, go) {
      ask((onClose) => askQuestion({
        within: within(el), words: SHELF.askDelete(name), yes: 'delete', no: 'keep',
        signal, opener: el, onClose, go,
      }));
    },
  };
}
