/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Whether the page's viewer may change nothing: shared by the dash's keys and the mountains.

import { mayWrite } from '../../core/actions.js';

/** A guest, a page whose writes are off (the core's rule), or a device the model marks inert. */
export function inertOf(page) {
  return !mayWrite(page) || Boolean(page?.device?.inert);
}
