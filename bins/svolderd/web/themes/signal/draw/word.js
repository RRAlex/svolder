/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A control that acts: a button of the open area. For a viewer who may not change the device (a
// guest, a page whose writes are off) it is drawn `quiet` with `aria-disabled`; a tap on it still
// reaches the core, which says why nothing happens. Pure: strings in, markup out.

import { esc } from './esc.js';

/** The markup of a button that does `act`: `inert` for a viewer who may not write, `attrs` the rest
 * of its attributes (already escaped, each beginning with a space), `cls` its classes. `text` is
 * escaped unless `markup` is true (then it is markup the caller drew, such as an icon). */
export function button(act, text, inert, attrs = '', cls = '', markup = false) {
  return `<button type="button" class="${cls}${inert ? ' quiet' : ''}" data-act="${act}"${attrs}`
    + `${inert ? ' aria-disabled="true"' : ''}>${markup ? text : esc(text)}</button>`;
}

/** Whether the page's viewer may not change anything: a guest, or a page whose writes are off. */
export function inertPage(page) {
  const viewer = page.viewer ?? {};
  return viewer.guest === true || viewer.writes === false;
}
