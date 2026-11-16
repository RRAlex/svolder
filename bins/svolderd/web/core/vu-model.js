/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The VU meter's rules without a drawing: the needle's ballistics, the scale it reads, the
// reference that sets which level is 0 VU, the over lamp, and the levels a frame carries. They are
// the rules of the engine's `vu` module (crates/svolder-host/src/vu.rs) written again for the
// page, and the node harness (`scripts/web-checks/node/vu.mjs`) holds this needle to the engine's
// golden path (`scripts/web-checks/fixtures/vu-ballistics.json`) so the two cannot drift. Pure: no
// DOM, no timers of its own, no import; the time is passed in (`performance.now()`-like
// milliseconds on the page's clock).
//
// A level is an amplitude (1.0 is full scale) divided by the reference's amplitude: a level equal
// to the reference is a ratio of 1.0, 0 VU. The needle is a second-order system,
// `x'' + 2ζω x' + ω² x = ω² u`, `u` being the latest ratio held until the next one comes, integrated
// in whole-millisecond steps, so its path depends only on what was pushed and when, never on how
// often a frame arrives or the screen is drawn. It rests on a stop at each end of the scale and
// leaves it at once; a gap of more than `MAX_CATCH_UP_MS` between two calls snaps it to its
// steady state. The needle follows the RMS level, not an average rectifier (docs/design/vu-meter.md).
//
// The levels on the wire (`lv`, in a full frame of `GET api/v1/host/frames` and in the whole of a
// levels-only one, `?only=levels`: `{ t, lv }`) are four bytes `[rmsL, rmsR, peakL, peakR]`, a
// byte a level of -72 .. 0 dBFS over 0 .. 255. A peak of 255 is an over (a sample at 32767/32768
// of full scale or more); a peak below it is never 255.
//
//   decodeLevels(wire)   `{ t, levels }` (`levels` a `Uint8Array(4)`) from a frame's parsed object
//                        or its JSON text, full or levels-only, or null for anything else
//   byteAmplitude(byte), peakAmplitude(byte)   the amplitude a byte stands for (0 is silence)
//   Scale                `deflection(ratio)` 0 (-20 VU) to 1 (+3 VU), clamped to 0 .. `Scale.STOP`;
//                        `readingDb(ratio)` the reading in VU (-Infinity at rest)
//   new Needle(x, ms)    one needle at rest at `x` on a clock that reads `ms`: `set(u)`,
//                        `advanceTo(ms)`, `.x`, `.v`, `.u`, `.deflection`
//   new Lamp()           `push(peak, ms)`, `lit(ms)`
//   new VuModel({ ref }) what a theme draws, fed with the stream's events:
//     .push(lv, now)     the four bytes of a frame read at `now`; false when they are not four
//                        bytes (nothing changes then)
//     .step(now)         once per draw: `{ L, R }`, each `{ x, f, db, lamp }`: the needle's ratio,
//                        its deflection, its reading in VU, the lamp. The object is the model's own
//                        and is rewritten by the next `step` (no allocation per draw): read it, do
//                        not keep it
//     .settled(now)      the same shape (its own object), the needle where it will come to rest on
//                        the level held (for reduced motion: jump there each draw) and the lamp as
//                        `step(now)` gives it; the model does not move
//     .setRef(db)        the reference in dBFS, rounded and held to `REF_MIN` .. `REF_MAX`; not a
//                        number is `REF_DEFAULT`; acts on the level held at once
//     .ref               the reference now
//     .clear()           back to rest and no lamp (the capture ended, the stream stopped, the page
//                        is hidden), the reference kept; the clock starts afresh at the next call

/** The reference's range and default, in dBFS. */
export const REF_MIN = -30;
export const REF_MAX = -6;
export const REF_DEFAULT = -18;

/** How long, in milliseconds, the lamp stays lit after the last over. */
export const HOLD_MS = 1200;

/** The needle's integration step, in milliseconds. */
export const STEP_MS = 1;

/** The longest gap, in milliseconds, a call integrates through; a longer one snaps. */
export const MAX_CATCH_UP_MS = 2000;

/** The needle's natural frequency (radians a second) and damping ratio. */
export const OMEGA = 14.2;
export const ZETA = 0.83;

/** The floor of a byte, in dBFS: byte 0 is silence, byte 1 the first step above it. */
const BYTE_FLOOR_DB = -72;

/** The smallest peak that is an over: the 16-bit positive full scale, 32767/32768. */
export const OVER = 32767 / 32768;

/** How near its input (a ratio) and how slow (ratio per second) a needle is when it counts as at rest. */
const REST_X = 1e-5;
const REST_V = 1e-3;

const LOW = 0.1;
const TOP = 10 ** (3 / 20);

/** The scale a needle reads: -20 VU at the left end, +3 VU at the right, linear in amplitude. */
export const Scale = Object.freeze({
  /** The deflection past which the needle rests on its stop. */
  STOP: 1.03,
  /** The deflection of an amplitude ratio: 0 to `Scale.STOP`; not a number is 0. */
  deflection(ratio) {
    const f = (ratio - LOW) / (TOP - LOW);
    if (Number.isNaN(f)) return 0;
    return Math.min(Math.max(f, 0), 1.03);
  },
  /** The reading in VU of an amplitude ratio; 0, a negative ratio or not a number is -Infinity. */
  readingDb(ratio) {
    if (!(ratio > 0)) return -Infinity;
    return 20 * Math.log10(ratio);
  },
});

/** The amplitude ratio at the right stop: the needle's state never goes past it. */
export const STOP_RATIO = LOW + 1.03 * (TOP - LOW);

/** The amplitude (1.0 = full scale) a level byte stands for; 0 is silence, not the floor. */
export function byteAmplitude(byte) {
  if (byte === 0) return 0;
  return 10 ** (((byte / 255) * -BYTE_FLOOR_DB + BYTE_FLOOR_DB) / 20);
}

/** The amplitude a peak byte stands for: 255, an over, is exactly `OVER`. */
export function peakAmplitude(byte) {
  return byte === 255 ? OVER : byteAmplitude(byte);
}

/** Whether `lv` is four whole numbers from 0 to 255 (an array, typed or not; not a string). */
function isFourBytes(lv) {
  if (!lv || typeof lv === 'string' || lv.length !== 4) return false;
  for (let i = 0; i < 4; i++) {
    const b = lv[i];
    if (typeof b !== 'number' || !Number.isInteger(b) || b < 0 || b > 255) return false;
  }
  return true;
}

/** `{ t, levels }` from a frame's wire shape (the object or its JSON text), full or levels-only,
 * or null for anything that is not a time and four level bytes. */
export function decodeLevels(wire) {
  let w = wire;
  if (typeof w === 'string') {
    try {
      w = JSON.parse(w);
    } catch {
      return null;
    }
  }
  if (!w || typeof w !== 'object' || !Number.isFinite(w.t)) return null;
  return Array.isArray(w.lv) && isFourBytes(w.lv) ? { t: w.t, levels: Uint8Array.from(w.lv) } : null;
}

/** A needle: the state of the second-order system and the millisecond it was last advanced to. */
export class Needle {
  /** A needle at rest at ratio `x` on a clock that reads `ms`, with `x` as its input. */
  constructor(x = 0, ms = 0) {
    this.x = x;
    this.v = 0;
    this.u = x;
    this.ms = Math.floor(ms);
  }

  /** The latest input ratio, held until the next call; not a number or negative is 0. */
  set(u) {
    this.u = Number.isFinite(u) ? Math.max(u, 0) : 0;
  }

  /** Integrate up to `ms` on the caller's clock (a time not after the last one does nothing). A
   * gap over `MAX_CATCH_UP_MS` puts the needle at its input, held inside the stops, at rest. */
  advanceTo(ms) {
    const to = Math.floor(ms);
    if (!(to > this.ms)) return;
    if (to - this.ms > MAX_CATCH_UP_MS) {
      this.x = Math.min(Math.max(this.u, 0), STOP_RATIO);
      this.v = 0;
      this.ms = to;
      return;
    }
    const dt = STEP_MS / 1000;
    while (this.ms + STEP_MS <= to) {
      this.v += dt * (OMEGA * OMEGA * (this.u - this.x) - 2 * ZETA * OMEGA * this.v);
      this.x += dt * this.v;
      if (this.x < 0 || this.x > STOP_RATIO) {
        this.x = Math.min(Math.max(this.x, 0), STOP_RATIO);
        this.v = 0;
      }
      this.ms += STEP_MS;
    }
  }

  /** The needle's deflection on the scale. */
  get deflection() {
    return Scale.deflection(this.x);
  }
}

/** The over lamp: lit from a peak of `OVER` or more until `HOLD_MS` after the last such one. */
export class Lamp {
  constructor() {
    this.last = null;
  }

  /** A frame's sample peak (amplitude, 1.0 = full scale) at `ms`. */
  push(peak, ms) {
    if (peak >= OVER) this.last = Math.floor(ms);
  }

  /** Whether the lamp is lit at `ms`. */
  lit(ms) {
    if (this.last === null) return false;
    const at = Math.floor(ms);
    return at >= this.last && at - this.last <= HOLD_MS;
  }

  clear() {
    this.last = null;
  }
}

/** The reference of `db` dBFS: a whole number in range, or the default for anything else. */
function refOf(db) {
  if (typeof db !== 'number' || Number.isNaN(db)) return REF_DEFAULT;
  return Math.min(Math.max(Math.round(db), REF_MIN), REF_MAX);
}

/** Write what one channel shows into `out`, for a needle at ratio `x` and a lamp `lamp`. */
function reading(out, x, lamp) {
  out.x = x;
  out.f = Scale.deflection(x);
  out.db = Scale.readingDb(x);
  out.lamp = lamp;
}

/** The ratio a needle settles at: its input, held inside the stops. */
const held = (needle) => Math.min(Math.max(needle.u, 0), STOP_RATIO);

const shape = () => ({
  L: { x: 0, f: 0, db: -Infinity, lamp: false },
  R: { x: 0, f: 0, db: -Infinity, lamp: false },
});

/** The two channels' needles and lamps over a reference. */
export class VuModel {
  constructor({ ref = REF_DEFAULT } = {}) {
    this.ref = refOf(ref);
    this.shown = shape();
    this.rested = shape();
    this.clear();
  }

  /** The ratio of an RMS amplitude to the reference's amplitude. */
  ratio(rms) {
    return rms / 10 ** (this.ref / 20);
  }

  /** Make the needles and lamps at `ms`, on the first sight of a clock. */
  start(ms) {
    if (this.needles) return;
    this.needles = [new Needle(0, ms), new Needle(0, ms)];
  }

  push(lv, now) {
    if (!isFourBytes(lv) || !Number.isFinite(now)) return false;
    this.start(now);
    for (let i = 0; i < 2; i++) {
      this.needles[i].advanceTo(now);
      this.rms[i] = byteAmplitude(lv[i]);
      this.needles[i].set(this.ratio(this.rms[i]));
      this.lamps[i].push(peakAmplitude(lv[2 + i]), now);
    }
    return true;
  }

  /** Fill `out` for channel `i` from `x`, the lamp as it stands at `now`. */
  fill(out, i, x, now) {
    const clock = this.needles !== null && Number.isFinite(now);
    reading(out, x, clock && this.lamps[i].lit(now));
  }

  step(now) {
    if (this.needles && Number.isFinite(now)) {
      this.needles[0].advanceTo(now);
      this.needles[1].advanceTo(now);
    }
    this.fill(this.shown.L, 0, this.needles ? this.needles[0].x : 0, now);
    this.fill(this.shown.R, 1, this.needles ? this.needles[1].x : 0, now);
    return this.shown;
  }

  settled(now) {
    this.fill(this.rested.L, 0, this.needles ? held(this.needles[0]) : 0, now);
    this.fill(this.rested.R, 1, this.needles ? held(this.needles[1]) : 0, now);
    return this.rested;
  }

  /**
   * Whether nothing on the dials will change before the next frame: no lamp is lit and, unless `still`
   * (the needles jump to their value), each needle is on its input within `REST_X` and slower than
   * `REST_V`, or its deflection and its input's are both 0.
   */
  resting(now, still = false) {
    if (!this.needles) return true;
    for (const lamp of this.lamps) if (lamp.lit(now)) return false;
    if (still) return true;
    return this.needles.every((n) => {
      const goal = held(n);
      if (n.deflection === 0 && Scale.deflection(goal) === 0) return true;
      return Math.abs(goal - n.x) < REST_X && Math.abs(n.v) < REST_V;
    });
  }

  setRef(db) {
    this.ref = refOf(db);
    if (this.needles) {
      for (let i = 0; i < 2; i++) this.needles[i].set(this.ratio(this.rms[i]));
    }
  }

  clear() {
    this.needles = null;
    this.rms = [0, 0];
    this.lamps = [new Lamp(), new Lamp()];
  }
}
