/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Frames for Void Garden's feed harnesses: the recorded frames of the fake host (fixtures/
// voidgarden-frames.json, wire objects as `GET api/v1/host/frames` sent them) decoded, and frames
// made by hand from band levels in dB. Not a test: voidgarden-feed.mjs and voidgarden-director.mjs
// import it.
import { readFileSync } from 'node:fs';
import { BANDS, bandCentreHz, decode } from '../../../bins/svolderd/web/core/viz-model.js';

export const recorded = () => JSON.parse(
  readFileSync(new URL('./fixtures/voidgarden-frames.json', import.meta.url)),
).frames;

/** The recorded frames, decoded. */
export const recordedFrames = () => recorded().map((wire) => decode(wire));

/** The byte of a level in dB (-72 .. 0 over 0 .. 255; below -72 is 0). */
export const byteOf = (db) => (db <= -72 ? 0 : Math.round(((Math.min(db, 0) + 72) / 72) * 255));

/** `Uint8Array(BANDS)` of `db(hz)` at each band's centre. */
export const bandsOf = (db) => Uint8Array.from({ length: BANDS }, (_, i) => byteOf(db(bandCentreHz(i))));

/** A decoded frame at `t` ms from the bands' dB on each side (a function of Hz or a number for a
 * flat floor), the levels' bytes, and the beat fields. */
export function frame({
  t = 0, left = -200, right = left, levels = [0, 0, 0, 0], beat = false, bpm = null, phase = 0,
} = {}) {
  const side = (v) => bandsOf(typeof v === 'function' ? v : () => v);
  return {
    t, left: side(left), right: side(right), levels: Uint8Array.from(levels), beat, bpm, phase,
  };
}

/** `db` inside [lo, hi] Hz, nothing outside. */
export const within = (lo, hi, db) => (hz) => (hz >= lo && hz <= hi ? db : -200);

/** An RMS byte that reads as amplitude `a` (0 to 1 of full scale), the inverse of vu-model's. */
export const rmsByte = (a) => (a <= 0 ? 0 : Math.min(255, Math.round(((20 * Math.log10(a) + 72) / 72) * 255)));
