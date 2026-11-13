/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Paper's list: the core's list of options next to the word that opened it, dressed in
// Paper's classes and kept inside its page, under it.
// The page's keys reach a list through the core's `listen`.

import {
  closeList, closeOf, listen, openList as coreOpen,
} from '../../core/list.js';

const CLASSES = { list: 'plist', option: 'p-opt' };

export { closeList, closeOf };

/** Open a list next to `opener` (the core's `openList`) in Paper's look. */
export function openList(options) {
  const list = coreOpen({ ...options, classes: CLASSES, root: '.paper', place: 'below' });
  listen(list);
  return list;
}
