/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Night drive's list: the core's list of options next to the chip that opened it, dressed in
// Night drive's classes and kept inside its page, over it in the lower half of the page, else under it.
// The page's keys reach a list through the core's `listen`.

import {
  closeList, closeOf, listen, openList as coreOpen,
} from '../../core/list.js';

const CLASSES = { list: 'nd-list', option: 'nd-opt' };

export { closeList, closeOf };

/** Open a list next to `opener` (the core's `openList`) in Night drive's look. */
export function openList(options) {
  const list = coreOpen({ ...options, classes: CLASSES, root: '.nd', place: 'auto' });
  listen(list);
  return list;
}
