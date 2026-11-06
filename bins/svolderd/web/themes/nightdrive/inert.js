/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Whether the page's viewer may change nothing: shared by the dash's keys and the mountains.

/** A guest, a page whose writes are off, or a device the model marks inert. */
export function inertOf(page) {
  const viewer = page?.viewer ?? {};
  return viewer.guest === true || viewer.writes === false || Boolean(page?.device?.inert);
}
