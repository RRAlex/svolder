/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The words the daemon's readings are told in, for any theme: how old a reading is, the signal's
// line and the Identify answer's toast; and the computer's points as a table writes them (a
// frequency, a gain, a Q) and as a person types them. Pure: numbers and answers in, text out.

/**
 * A span of `secs` seconds in the largest unit that keeps it readable, as the daemon words one:
 * seconds under 90 s, minutes under 90 min, hours under 36 h, then days; each count rounded down.
 */
export function ageWords(secs) {
  const s = Math.max(0, Math.floor(secs));
  if (s < 90) return `${s} s`;
  if (s < 90 * 60) return `${Math.floor(s / 60)} min`;
  if (s < 36 * 3600) return `${Math.floor(s / 3600)} h`;
  const days = Math.floor(s / 86400);
  return `${days} ${days === 1 ? 'day' : 'days'}`;
}

/** The signal as the daemon words it: `head` (`−62 dBm advert`), with its age after it; a live
 * signal's only once it is half a minute old. */
export function signalText(head, live, secs) {
  if (live && secs < 30) return head;
  return `${head}, ${ageWords(secs)} ago`;
}

/** The words an Identify answer is toasted with; `gap` is the daemon's wait between two cues, in
 * seconds, as the button carries it. */
export function identifyWords(answer, gap) {
  if (answer.status === 200) {
    return answer.sent ? 'Identify sent' : `Just sent — wait ${gap} s`;
  }
  return answer.error || 'The device did not answer.';
}

/** The number a typed value means: `2.9k`, `2900 Hz`, `+3,2 dB` and `−2` all read; `null` when it
 * is none. A comma is the decimal mark (a dot always is). */
export function parse(text) {
  const clean = String(text).trim().replace('−', '-').replace(',', '.').replace(/\s*(hz|db)$/i, '');
  const kilo = /k$/i.test(clean);
  const body = kilo ? clean.slice(0, -1) : clean;
  const number = Number(body);
  return body !== '' && Number.isFinite(number) ? number * (kilo ? 1000 : 1) : null;
}

/** The other reading of a typed value whose comma may be a thousands separator (`1,000` is 1000 as
 * well as 1): the number with its commas taken out, `null` when the text is not groups of exactly
 * three digits after a comma, the first group starting 1-9 (`1,5k`, `20,5`, `0,500` and `1.000,5` have none). */
export function thousandsReading(text) {
  const bare = String(text).trim().replace('−', '-').replace(/\s*(hz|db)$/i, '');
  return /^[+-]?[1-9]\d{0,2}(,\d{3})+$/.test(bare) ? Number(bare.replaceAll(',', '')) : null;
}

/** dB with one decimal and a true minus; no sign on zero. */
export function dbText(db) {
  const text = Math.abs(db).toFixed(1);
  return db < 0 && text !== '0.0' ? `−${text}` : text;
}

/** A frequency in Hz: whole above 100, one decimal below, no trailing zero. */
export const hzText = (f) => `${Number(f.toFixed(f < 100 ? 1 : 0))} Hz`;

/** The frequency in words: kHz from 1000 (`6.3 kHz`), else `hzText`. */
export const hzWords = (hz) => (hz >= 1000 ? `${fixedEven(hz / 1000, 1)} kHz` : hzText(hz));

/** A gain with its sign and a true minus; no sign on zero. */
export const gainText = (g) => `${g > 0 && g.toFixed(1) !== '0.0' ? '+' : ''}${dbText(g)} dB`;

/** A Q with two decimals. */
export const qText = (q) => q.toFixed(2);

/**
 * `value` with `digits` decimals as the daemon's `format!("{:.N}")` writes it (Rust's exact float
 * printing): the double's exact value is rounded, and a value that is exactly halfway goes to the
 * even digit (0.125 is `0.12`, 0.375 is `0.38`); `toFixed` takes such a tie up. The sign of a
 * negative zero is kept, as Rust keeps it.
 */
export function fixedEven(value, digits) {
  const negative = value < 0 || Object.is(value, -0);
  const x = Math.abs(value);
  const sign = negative ? '-' : '';
  if (!Number.isFinite(x)) return String(value);
  // toFixed gives up above 1e21, where a double is a whole number: print it whole.
  if (x >= 1e21) return `${sign}${BigInt(x)}${digits > 0 ? `.${'0'.repeat(digits)}` : ''}`;
  // A tie ends in a 5 at the next decimal with nothing after it, which a double's exact value
  // shows within 30 more digits (it has 53 bits).
  const long = x.toFixed(Math.min(digits + 30, 100));
  const cut = long.length - (Math.min(digits + 30, 100) - digits);
  const tail = long.slice(cut);
  const tie = /^50*$/.test(tail);
  const up = x.toFixed(digits);
  if (!tie) return sign + up;
  const down = long.slice(0, digits > 0 ? cut : cut - 1);
  const last = down.at(-1);
  return sign + (Number(last) % 2 === 0 ? down : up);
}

/** The words of the step `raw` of a level (`{ lo, texts }`: the volume, a slider): the model's
 * own words for it, its number where the model has none. */
export const stepWords = (level, raw) => level?.texts?.[raw - level.lo] ?? String(raw);
