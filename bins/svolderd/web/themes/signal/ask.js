/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Signal's inline ask: the core's form for a name and question that asks once, dressed in Signal's
// classes and worded by its shelf. The shelf is long, so an ask is brought into view (clear of the
// Now playing bar, by its scroll margin). The page's keys reach an ask through the core's `listen`.

import {
  askForm as coreForm, askQuestion as coreQuestion, closeAsk, listen,
} from '../../core/inline-ask.js';
import { SHELF } from './words.js';

const CLASSES = {
  box: 'sg-askbox', form: 'sg-form', input: 'sg-input', button: 'sg-btn sg-pill', set: 'sg-set',
  question: 'sg-question', danger: 'danger',
};

export { closeAsk };

/** The form for a name (the core's `askForm`) in Signal's look and words. */
export function askForm(options) {
  const ask = coreForm({ ...options, classes: CLASSES, words: SHELF, scroll: true });
  listen(ask);
  return ask;
}

/** The question (the core's `askQuestion`, its sentence given as `words`) in Signal's look. */
export function askQuestion({ words, ...options }) {
  const ask = coreQuestion({
    ...options, text: words, classes: CLASSES, words: SHELF, scroll: true,
  });
  listen(ask);
  return ask;
}
