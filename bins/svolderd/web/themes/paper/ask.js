/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Paper's inline ask: the core's form for a name and question that asks once, drawn as ruled lines
// and words (`p-` classes, a middle dot between the two words) with Paper's words. The page's keys
// reach an ask through the core's `listen`.

import {
  askForm as coreForm, askQuestion as coreQuestion, closeAsk, listen,
} from '../../core/inline-ask.js';
import { WORDS } from './words.js';

const CLASSES = {
  box: 'p-askbox', form: 'p-form', input: 'p-input', button: 'p-w', question: 'p-question',
};
const BETWEEN = ' &middot; ';

export { closeAsk };

/** The form for a name (the core's `askForm`) in Paper's look and words. */
export function askForm(options) {
  const ask = coreForm({ ...options, classes: CLASSES, words: WORDS, between: BETWEEN });
  listen(ask);
  return ask;
}

/** The question (the core's `askQuestion`, its sentence given as `words`) in Paper's look. */
export function askQuestion({ words, ...options }) {
  const ask = coreQuestion({
    ...options, text: words, classes: CLASSES, words: WORDS, between: BETWEEN,
  });
  listen(ask);
  return ask;
}
