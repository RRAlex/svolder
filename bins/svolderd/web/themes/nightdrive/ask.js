/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Night drive's inline ask: the core's form for a name and question that asks once, dressed in
// Night drive's classes and worded by its sheet. The sheet scrolls inside itself, so an ask is
// brought into view. The page's keys reach an ask through the core's `listen`.

import {
  askForm as coreForm, askQuestion as coreQuestion, closeAsk, listen,
} from '../../core/inline-ask.js';
import { SHEET } from './words.js';

const CLASSES = {
  box: 'nd-askbox', form: 'nd-form', input: 'nd-input', button: 'nd-btn nd-pill', set: 'nd-set',
  question: 'nd-question', danger: 'danger',
};

export { closeAsk };

/** The form for a name (the core's `askForm`) in Night drive's look and words. */
export function askForm(options) {
  const ask = coreForm({ ...options, classes: CLASSES, words: SHEET, scroll: true });
  listen(ask);
  return ask;
}

/** The question (the core's `askQuestion`, its sentence given as `words`) in Night drive's look. */
export function askQuestion({ words, ...options }) {
  const ask = coreQuestion({
    ...options, text: words, classes: CLASSES, words: SHEET, scroll: true,
  });
  listen(ask);
  return ask;
}
