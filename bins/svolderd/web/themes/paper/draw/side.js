/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Which side of the sheet is drawn: the device's, or the computer's (the core's `computer.side()`,
// handed to the sections as `view.computer`). A sheet drawn without the core's computer (node, a
// theme preview) is the device's. Pure.

/** Whether the computer's side is the one drawn. */
export const onComputer = (view) => view?.computer?.side?.() === 'computer';

/** Whether the viewer may not write (a guest, or a page whose writes are off): the computer's
 * words are drawn quiet for them, as the device's are. */
export function viewerQuiet(page) {
  const viewer = page?.viewer ?? {};
  return Boolean(viewer.guest) || viewer.writes === false;
}
