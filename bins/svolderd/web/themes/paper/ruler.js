/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume's ruler, as numbers: where a step, a label, a tick and the two zones stand along it,
// as fractions of its length (0 at the quietest step, 1 at the loudest). The ruler is linear in
// the device's steps, so the needle moves one equal distance a step; the labels sit where the dB
// crosses each multiple of ten, read off the device's own table (`volume.db`, one entry per step,
// the first `null` where the device is silent), so on a device whose steps are not an even number
// of dB apart the labels still tell the truth and are spaced unevenly. Pure: no page needed.

/** A label at each multiple of this many dB; at each multiple of the narrow one on a short ruler. */
export const LABEL_DB = 10;
export const NARROW_LABEL_DB = 20;
/** A ruler shorter than this many pixels is narrow. */
export const NARROW_PX = 600;
/** The least a label keeps from the one before it, as a fraction of the ruler, and in pixels on a
 * ruler too short for that fraction to be as many (a phone's, where the bottom mark and the next
 * label would touch). */
export const LABEL_GAP = 0.03;
const LABEL_GAP_PX = 30;
/** A tick at each multiple of this many dB. */
const TICK_DB = 2;
/** The most steps a ruler without dB draws a tick for each of. */
const TICKED_STEPS = 60;
/** How many labels a ruler without dB aims for. */
const WORDED = 8;

const span = (volume) => volume.hi - volume.lo;

/** The place of step `raw` as a fraction of the ruler, held between 0 and 1. */
export function fractionOf(volume, raw) {
  if (!(span(volume) > 0) || !Number.isFinite(raw)) return 0;
  return Math.min(1, Math.max(0, (raw - volume.lo) / span(volume)));
}

/** The step at fraction `fraction` of the ruler: the nearest, held to the device's range. */
export function rawAt(volume, fraction) {
  const step = Math.round(volume.lo + fraction * span(volume));
  return Math.min(volume.hi, Math.max(volume.lo, step));
}

/** Whether the table says anything about dB: it has at least one finite entry. */
const hasDb = (volume) => (volume.db ?? []).some(Number.isFinite);

/** The places (fractions) where the dB crosses each multiple of `every`, with the multiple. */
function crossings(volume, every) {
  const db = volume.db ?? [];
  const finite = db.filter(Number.isFinite);
  if (finite.length < 2 || !(span(volume) > 0)) return [];
  const found = [];
  const first = Math.ceil(Math.min(...finite) / every);
  const last = Math.floor(Math.max(...finite) / every);
  for (let multiple = first; multiple <= last; multiple++) {
    const level = multiple * every;
    const i = db.findIndex((v) => Number.isFinite(v) && v >= level);
    if (i < 0) continue;
    const before = i > 0 && Number.isFinite(db[i - 1]) ? db[i - 1] : null;
    const index = before === null || db[i] === before
      ? i
      : i - 1 + (level - before) / (db[i] - before);
    found.push({ at: Math.min(1, Math.max(0, index / span(volume))), level });
  }
  return found;
}

/** A level as the ruler writes it: the true minus sign, a plain `0`. */
const written = (level) => (level < 0 ? `−${-level}` : String(level));

/** The words of the quietest step: the device's own mark for it, else its table's. */
function bottomWords(volume) {
  const given = volume.marks?.[0];
  if (given) return given.text;
  const first = volume.texts?.[0];
  if (first !== undefined) return first;
  const level = (volume.db ?? []).find(Number.isFinite);
  return level === undefined ? '' : written(Math.round(level));
}

/**
 * The labels along a ruler `widthPx` wide, `[{ at, text }]`: the bottom mark first, then one at
 * each multiple of ten dB (twenty on a narrow ruler), each dropped when it would stand within
 * `LABEL_GAP` of one kept. Without any dB, every few steps, in the device's own words.
 */
export function marks(volume, widthPx) {
  if (!(span(volume) > 0)) return [];
  const list = [{ at: fractionOf(volume, volume.marks?.[0]?.at ?? volume.lo), text: bottomWords(volume) }];
  const gap = Math.max(LABEL_GAP, LABEL_GAP_PX / widthPx);
  const keep = (at, text) => {
    if (list.every((m) => Math.abs(m.at - at) >= gap)) list.push({ at, text });
  };
  if (hasDb(volume)) {
    const every = widthPx < NARROW_PX ? NARROW_LABEL_DB : LABEL_DB;
    for (const { at, level } of crossings(volume, every)) keep(at, written(level));
  } else {
    const each = Math.ceil(span(volume) / WORDED);
    for (let i = each; i <= span(volume); i += each) {
      keep(i / span(volume), volume.texts?.[i] ?? '');
    }
  }
  return list;
}

/** The ticks along the ruler, as fractions: each two dB; without dB each step of a short range. */
export function ticks(volume) {
  if (hasDb(volume)) return crossings(volume, TICK_DB).map((c) => c.at);
  if (!(span(volume) > 0) || span(volume) > TICKED_STEPS) return [];
  return Array.from({ length: span(volume) + 1 }, (_, i) => i / span(volume));
}

/** Where the risk zone starts and the cap stands, as fractions (`null` for one the device lacks). */
export function zones(volume) {
  const at = (raw) => (raw === null || raw === undefined ? null : fractionOf(volume, raw));
  return { risk: at(volume.risk), cap: at(volume.cap) };
}
