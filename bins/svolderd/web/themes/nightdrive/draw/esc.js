/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Words from the model made safe to put into Night drive's markup.

const ESCAPES = {
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', '\'': '&#39;',
};

/** `text` made safe to put into markup, in text or in a quoted attribute. */
export const esc = (text) => String(text ?? '').replace(/[&<>"']/g, (c) => ESCAPES[c]);
