/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where things stand in the open area. Markup carries no `style` attribute (the page's content
// policy forbids it), so a mark on a track, a knob or a point on the graph names its place as a
// fraction of its box, and this sets it through the style object, which the policy allows.

/** A fraction of a box (0 to 1) as a percentage, or nothing for a value that is not a number. */
function percent(text) {
  const value = Number(text);
  return text === '' || text === null || text === undefined || !Number.isFinite(value)
    ? null : `${(value * 100).toFixed(2)}%`;
}

/** Put every `[data-at]` of `root` at that fraction of its box's width (a `.sg-fill` takes it as
 * its width from the start) and every `[data-x]` with a `[data-y]` at those fractions of its
 * box's width and height. */
export function placeOpen(root) {
  for (const el of root.querySelectorAll('[data-at]')) {
    const at = percent(el.dataset.at);
    if (at === null) continue;
    if (el.classList.contains('sg-fill')) el.style.width = at;
    else el.style.left = at;
  }
  for (const el of root.querySelectorAll('[data-x][data-y]')) {
    const left = percent(el.dataset.x);
    const top = percent(el.dataset.y);
    if (left !== null) el.style.left = left;
    if (top !== null) el.style.top = top;
  }
}
