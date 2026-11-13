/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Signal's list: the core's list of options next to the chip that opened it, dressed in
// Signal's classes and kept inside its page, under it.
// The page's keys reach a list through the core's `listen`.

import {
  closeList, closeOf, listen, openList as coreOpen,
} from '../../core/list.js';

const CLASSES = { list: 'sg-list', option: 'sg-opt' };

export { closeList, closeOf };

/** Open a list next to `opener` (the core's `openList`) in Signal's look. */
export function openList(options) {
  const list = coreOpen({ ...options, classes: CLASSES, root: '.sg', place: 'below' });
  listen(list);
  return list;
}
