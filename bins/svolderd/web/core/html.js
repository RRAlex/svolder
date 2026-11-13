/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Markup helpers for every part that draws strings: the words the model carries hold the user's
// own text (a device's name, a preset's name), so each goes through `esc`. A pure module a theme
// may import directly.

const ESCAPES = {
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', '\'': '&#39;',
};

/** `text` made safe to put into markup, in text or in a quoted attribute; none is empty. */
export const esc = (text) => String(text ?? '').replace(/[&<>"']/g, (c) => ESCAPES[c]);

/**
 * The attribute `name` as ` name="value"`, with its leading space: a true value is the bare
 * attribute, and no value (`null`, `undefined`, false) is none.
 */
export function attr(name, value) {
  if (value === null || value === undefined || value === false) return '';
  return value === true ? ` ${name}` : ` ${name}="${esc(value)}"`;
}
