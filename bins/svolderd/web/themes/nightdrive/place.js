/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where things stand on the sheet. Markup carries no `style` attribute (the page's content policy
// forbids it), so a knob on a scale names its place as a fraction of its box and this sets it through
// the style object, which the policy allows.

import { SHEET } from './words.js';

/** Put every `.nd-knob` and `.nd-ctr` of `root` that has a `data-at` at that fraction of its scale's
 * width. */
export function placeSheet(root) {
  for (const el of root.querySelectorAll('.nd-knob[data-at], .nd-ctr[data-at], .nd-needle[data-at]')) {
    const value = Number(el.dataset.at);
    if (el.dataset.at !== '' && Number.isFinite(value)) el.style.left = `${(value * 100).toFixed(2)}%`;
  }
  // the class A share is the width of the shaded part
  for (const el of root.querySelectorAll('.nd-zone[data-at]')) {
    const value = Number(el.dataset.at);
    if (Number.isFinite(value)) el.style.width = `${(value * 100).toFixed(2)}%`;
  }
}

/** Put the words under the points' table (`#nd-more`) right for how far its box (`#nd-pscroll`) is scrolled: the
 * rows still below the ones in view. */
export function placeMore(root) {
  const box = root.querySelector('#nd-pscroll');
  const more = root.querySelector('#nd-more');
  if (!box || !more) return;
  const rows = Math.max(box.querySelectorAll('tbody tr').length, 1);
  const hidden = Math.ceil((box.scrollHeight - box.scrollTop - box.clientHeight - 1) / (box.scrollHeight / rows));
  more.textContent = box.classList.contains('long') ? SHEET.moreBelow(hidden) : '';
}
