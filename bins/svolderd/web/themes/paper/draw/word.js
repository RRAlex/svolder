/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A written word that acts: a button underlined by the sheet's style. For a viewer who may not
// change the device (a guest, a page whose writes are off) it is drawn `quiet`, with no underline
// and `aria-disabled`; a tap on it still reaches the core, which says why it does nothing.

import { esc } from './esc.js';

/** The markup of the word `text` that does `act`: `inert` for a viewer who may not write, `attrs`
 * the rest of its attributes (already escaped, each beginning with a space), `cls` more classes. */
export function word(act, text, inert, attrs = '', cls = '') {
  return `<button type="button" class="p-w${cls ? ` ${cls}` : ''}${inert ? ' quiet' : ''}"`
    + ` data-act="${act}"${attrs}${inert ? ' aria-disabled="true"' : ''}>${esc(text)}</button>`;
}
