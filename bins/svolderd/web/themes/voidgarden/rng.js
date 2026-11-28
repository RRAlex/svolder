/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme's only randomness: a text hashed to a seed, and a seed made into a stream of numbers.
// Pure: the same text or seed always gives the same numbers, on every machine.

/** FNV-1a of `text`'s UTF-16 units, as an unsigned 32-bit number. */
export function hashStr(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** A generator of numbers in [0, 1) from `seed` (mulberry32): call it for each next number. */
export function mulberry(seed) {
  let a = seed | 0;
  return () => {
    a = (a + 0x6D2B79F5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
