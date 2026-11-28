/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The audio panel's history: a ring of the ten features' last 18 seconds, 144 buckets of 1/8 s,
// each the time-weighted mean of what was pushed into it, so any step rate keeps the same 18
// seconds; and each feature's peak hold (held 1.2 s, then a slow fall). Pure: the time is the
// `dt` seconds handed to `push`.
//
//   new History(keys)        `push(values, dt)` adds a step; `series(key)` is the ring oldest
//                            first as `Float32Array(BUCKETS)`; `hold(key)` the peak mark;
//                            `filled` how many buckets have been written (at most `BUCKETS`)

import { KEYS } from './features.js';

/** How many seconds the ring keeps, and the buckets it has (1/8 s each). */
export const SECONDS = 18;
export const BUCKETS = 144;

/** How long a peak is held, seconds, and how fast it falls after, per second. */
const HOLD_S = 1.2;
const FALL_PER_S = 0.6;

const BUCKET_S = SECONDS / BUCKETS;

export class History {
  constructor(keys = KEYS) {
    this.keys = [...keys];
    this.ring = {};
    this.peak = {};
    this.heldFor = {};
    this.sum = {};
    for (const k of this.keys) {
      this.ring[k] = new Float32Array(BUCKETS);
      this.peak[k] = 0;
      this.heldFor[k] = 0;
      this.sum[k] = 0;
    }
    this.weight = 0;
    this.acc = 0;
    this.next = 0;
    this.filled = 0;
  }

  /** Add `dt` seconds of `values` (an object of the keys, 0 to 1). */
  push(values, dt) {
    for (const k of this.keys) {
      const v = values[k];
      if (v >= this.peak[k]) {
        this.peak[k] = v;
        this.heldFor[k] = HOLD_S;
      } else if ((this.heldFor[k] -= dt) < 0) {
        this.peak[k] = Math.max(v, this.peak[k] - dt * FALL_PER_S);
      }
      this.sum[k] += v * dt;
    }
    this.weight += dt;
    this.acc += dt;
    while (this.acc >= BUCKET_S - 1e-9) {
      this.acc -= BUCKET_S;
      for (const k of this.keys) {
        this.ring[k][this.next] = this.weight > 0 ? this.sum[k] / this.weight : values[k];
        this.sum[k] = 0;
      }
      this.weight = 0;
      this.next = (this.next + 1) % BUCKETS;
      this.filled = Math.min(BUCKETS, this.filled + 1);
    }
  }

  /** The ring of `key`, oldest bucket first. */
  series(key) {
    const out = new Float32Array(BUCKETS);
    const ring = this.ring[key];
    for (let i = 0; i < BUCKETS; i++) out[i] = ring[(this.next + i) % BUCKETS];
    return out;
  }

  /** The peak mark of `key`. */
  hold(key) {
    return this.peak[key];
  }
}
