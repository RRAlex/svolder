/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The points of the computer's EQ as words: a frequency, a gain and a Q as the table writes them
// and as a person types them, and the hand ranges a typed value is held to. No DOM here.

/** The ranges of a hand, by the parameter the table names. */
export const FIELD = { f: [20, 20000], g: [-12, 12], q: [0.1, 10] };

/** The point types a person may pick. */
export const TYPES = ['PK', 'LSC', 'HSC'];

/** A frequency: whole above 100, one decimal below, no trailing zero. */
export const hzText = (f) => `${Number(f.toFixed(f < 100 ? 1 : 0))} Hz`;

/** A gain with its sign and a true minus; no sign on zero. */
export function gainText(g) {
  const text = Math.abs(g).toFixed(1);
  if (text === '0.0') return '0.0 dB';
  return `${g < 0 ? '−' : '+'}${text} dB`;
}

/** A Q with two decimals. */
export const qText = (q) => q.toFixed(2);

/** The words of one parameter of a point. */
export const WORDS = { f: hzText, g: gainText, q: qText };

/** The words above a held point: `250 Hz · +1.5 dB · Q 1.00`. */
export const labelOf = (point) => `${hzText(point.f)} · ${gainText(point.g)} · Q ${qText(point.q)}`;

/** The number a typed value means: `2.9k`, `2900 Hz`, `+3,2 dB` and `−2` all read; null when it is none. */
export function parse(text) {
  const clean = String(text).trim().replace('−', '-').replace(',', '.').replace(/\s*(hz|db)$/i, '');
  const kilo = /k$/i.test(clean);
  const number = Number(kilo ? clean.slice(0, -1) : clean);
  return clean !== '' && Number.isFinite(number) ? number * (kilo ? 1000 : 1) : null;
}

/** `value` held to the hand range of `param`. */
export const held = (param, value) => Math.min(FIELD[param][1], Math.max(FIELD[param][0], value));
