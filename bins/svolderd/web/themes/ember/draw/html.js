/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Ember draws its parts as strings from the page's model. Every word the model carries goes
// through esc: it holds the user's own text (a device's name, a preset's name).

const ESCAPES = {
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
};

/** `text` as text or as a quoted attribute value. */
export const esc = (text) => String(text).replace(/[&<>"']/g, (c) => ESCAPES[c]);

/**
 * The attribute `name` as ` name="value"`, with its leading space: a true value is the bare
 * attribute, and no value (`null`, `undefined`, false) is none.
 */
export function attr(name, value) {
  if (value === null || value === undefined || value === false) return '';
  return value === true ? ` ${name}` : ` ${name}="${esc(value)}"`;
}
