/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Ember draws its parts as strings from the page's model. Every word the model carries goes
// through esc: it holds the user's own text (a device's name, a preset's name). The helpers are the
// core's, kept at this path for the draws.
export { attr, esc } from '../../../core/html.js';
