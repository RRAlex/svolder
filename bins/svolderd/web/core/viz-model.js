/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The visualizer's rules, without a canvas: a frame decoded from the wire, the smoothed curves
// and their max hold, the rule for which frame is drawn, the clock offset to the daemon and the
// delay behind it, the beats scheduled ahead, and the maps between bands and pixels. Pure: no
// DOM, no timers of its own, no import; the time is passed in (`performance.now()`-like
// milliseconds on the page's clock), so the canvas layer and the node harness
// (`scripts/web-checks/node/viz.mjs`) run the same code. The colours are in `viz-colour.js`.
//
// A frame on the wire (`GET api/v1/host/frames`, event `frame`) is
// `{ t, l, r, lv, beat, bpm, phase }`: the capture's unix ms on the daemon's clock, the bands of
// the left and right channels as 240 bytes in base64 (a byte is a level of 0 to 255 over
// -72 .. 0 dBFS), the left and right RMS and the left and right peak as bytes, a beat onset flag
// (0 or 1), the tempo (a number or null) and the beat's phase (0 to 255 of the period since the
// last beat).
//
//   decode(wire)         the frame as `{ t, left, right, levels, beat, bpm, phase }` (`left`,
//                        `right` and `levels` are `Uint8Array`s, `beat` a boolean), or null for
//                        anything that is not a frame; `wire` is the parsed object or its text
//   new VizModel()       what the card draws, fed with the stream's events:
//     .hello(daemonNowMs, pageNowMs)  the stream's `hello` event, read at `pageNowMs`: sets the
//                        clock offset and starts the model afresh (the picture stays)
//     .push(wire, now)   a `frame` event read at `now`; false when it is not a frame or is not
//                        newer than the newest seen
//     .step(now)         once per draw: `{ frame, dropped }`, the newest frame waiting (or null)
//                        and how many older ones were left out; updates the curves and the hold
//     .clear()           back to silence (the capture ended, or the card closed)
//     .left, .right      the smoothed bands, `Float32Array(240)` of 0 to 1
//     .holdLeft, .holdRight  the max hold of each band, 0 to 1, as of the last `step`
//     .levels            `[rmsL, rmsR, peakL, peakR]`, 0 to 1, of the frame last drawn
//     .beatDue(now)      true once for each beat that is due on the page clock at `now`
//     .delayMs(now)      the median delay of the last 2 s, or null
//     .offsetMs          the daemon's clock minus the page's, or null before a hello
//     .daemonClock(pageMs), .pageClock(daemonMs)   the same offset, applied
//   new BeatClock()      the beat schedule alone (`observe`, `due`, `clear`)
//   new DelayMedian()    the delay window alone (`add`, `median`, `clear`)
//   freqPos(hz, size), posFreq(pos, size)   a frequency and a place along an axis of `size`
//                        pixels, on a log scale from 20 Hz to 20 kHz (place 0 is 20 Hz)
//   bandPos(i, size), posBand(pos, size)    the place of band `i`'s centre, and the band a place
//                        falls in (clamped to the first and the last)
//   bandEdgeHz(i), bandCentreHz(i)          a band's lower edge and its centre, in Hz
//   rowBands(height)     a spectrogram column's band for each row, row 0 (the top) the highest
//   levelY(level, height) a level's row in a bar graph: 0 at the bottom, 1 at the top
//   FREQ_MARKS           the frequencies a faint scale marks
//   VIZ                  the numbers (hold, smoothing, windows) as one frozen object
//
// The offset comes from `hello` alone. A frame's own time does not correct it: that would make
// the fastest frame count as no delay and so hide a steady one, which is what the delay row and
// the beats ahead of it are for.

/** How many bands a channel has: 1/24 octave from 20 Hz. */
export const BANDS = 240;

/** The numbers the rules use. */
export const VIZ = Object.freeze({
  BANDS,
  /** The lowest edge of the axis and of the first band, Hz. */
  F_MIN: 20,
  /** The highest frequency of the axis, Hz. */
  F_MAX: 20000,
  /** The frame rate the daemon sends at, as the smoothing counts it. */
  FRAME_MS: 1000 / 30,
  /** What a frame keeps of the one before it (the reference setting 0.4). */
  SMOOTHING: 0.4,
  /** How long a band's peak is held, ms. */
  HOLD_MS: 2000,
  /** How fast a held peak falls once its hold is over, in full scales per second. */
  FALL_PER_S: 1,
  /** The window the delay is the median of, ms. */
  DELAY_WINDOW_MS: 2000,
  /** The longest a frame may wait for a draw and still count for the smoothing, ms. */
  MAX_DT_MS: 250,
  /** How far behind its beat a pulse may still be drawn, ms (the continuous visuals' tolerance). */
  LATE_BEAT_MS: 120,
  /** The least between two pulses from flags alone, ms. */
  MIN_PULSE_GAP_MS: 200,
  /** How long without a frame ends the beat schedule, ms. */
  STALE_MS: 1000,
  /** How many frames wait for a draw before the oldest are let go. */
  QUEUE: 64,
});

/** The frequencies a faint scale marks, Hz. */
export const FREQ_MARKS = Object.freeze([50, 100, 200, 500, 1000, 2000, 5000, 10000]);

/** How many beat flags wait for a draw before the oldest is let go. */
const MAX_FLAGS = 8;

const BANDS_PER_OCTAVE = 24;
const OCTAVES = Math.log2(VIZ.F_MAX / VIZ.F_MIN);

/** What a draw applies once the frames have stopped: the curves fall as in silence. */
const SILENCE = Object.freeze({
  left: new Uint8Array(BANDS), right: new Uint8Array(BANDS), levels: new Uint8Array(4),
});

/** A level byte, 0 to 255, as 0 to 1. */
const unit = (byte) => byte / 255;

/** `text` (base64) as `BANDS` bytes, or null when it is not that. */
function bytes(text) {
  if (typeof text !== 'string' || text.length === 0) return null;
  let raw;
  try {
    raw = atob(text);
  } catch {
    return null;
  }
  if (raw.length !== BANDS) return null;
  const out = new Uint8Array(BANDS);
  for (let i = 0; i < BANDS; i++) out[i] = raw.charCodeAt(i);
  return out;
}

/**
 * A frame from its wire shape, an object or its JSON text: `{ t, left, right, levels, beat, bpm,
 * phase }`, or null for anything that is not a whole frame (a band list that is not 240 bytes,
 * four levels, a time that is not a number).
 */
export function decode(wire) {
  let w = wire;
  if (typeof w === 'string') {
    try {
      w = JSON.parse(w);
    } catch {
      return null;
    }
  }
  if (!w || typeof w !== 'object' || !Number.isFinite(w.t)) return null;
  const left = bytes(w.l);
  const right = bytes(w.r);
  if (!left || !right) return null;
  if (!Array.isArray(w.lv) || w.lv.length !== 4 || !w.lv.every(Number.isFinite)) return null;
  const bpm = Number.isFinite(w.bpm) && w.bpm > 0 ? w.bpm : null;
  return {
    t: w.t,
    left,
    right,
    levels: Uint8Array.from(w.lv),
    beat: w.beat === 1 || w.beat === true,
    bpm,
    phase: Number.isFinite(w.phase) ? w.phase : 0,
  };
}

/** The median of the delays read in the last `DELAY_WINDOW_MS`. */
export class DelayMedian {
  constructor(windowMs = VIZ.DELAY_WINDOW_MS) {
    this.windowMs = windowMs;
    this.samples = [];
  }

  /** Note a delay of `delayMs` read at `now`. */
  add(now, delayMs) {
    this.samples.push([now, delayMs]);
    this.drop(now);
  }

  /** The median of what was read in the window ending at `now` (the mean of the two in the
   * middle for an even count), or null when nothing was. */
  median(now) {
    this.drop(now);
    if (this.samples.length === 0) return null;
    const sorted = this.samples.map(([, d]) => d).sort((a, b) => a - b);
    const mid = sorted.length >> 1;
    return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
  }

  clear() {
    this.samples = [];
  }

  drop(now) {
    let at = 0;
    while (at < this.samples.length && now - this.samples[at][0] > this.windowMs) at++;
    if (at > 0) this.samples.splice(0, at);
  }
}

/**
 * When the beats are, kept on the daemon's clock and read on the page's. From a frame's tempo and
 * phase the last beat is `t - phase / 256 * period`, and the beats after it fall a period apart:
 * `due` says when the page clock reaches one, so a pulse is drawn on the beat though the frame
 * that carries it arrives late. A beat a pulse was already drawn for is not drawn again when a
 * frame for it arrives late. A beat that is already more than `LATE_BEAT_MS` behind is skipped
 * rather than drawn late, however many beats a gap has left behind. Without a tempo only the
 * flags pulse, once each, at least `MIN_PULSE_GAP_MS` apart and, as a beat of the grid, only
 * while not more than `LATE_BEAT_MS` behind; without frames (`STALE_MS`) nothing does.
 */
export class BeatClock {
  constructor() {
    this.clear();
  }

  clear() {
    this.period = null;
    this.anchor = 0;
    this.fired = null;
    this.flags = [];
    this.seenAt = -Infinity;
  }

  /** Take a decoded frame read at `now` on the page's clock. A flag no pulse was drawn for yet
   * stays until it is drawn or too late, whatever frames come after it. */
  observe(frame, now) {
    this.seenAt = now;
    if (frame.bpm) {
      this.period = 60000 / frame.bpm;
      this.anchor = frame.t - (frame.phase / 256) * this.period;
      this.flags = [];
    } else {
      this.period = null;
      if (frame.beat) {
        this.flags.push(frame.t);
        if (this.flags.length > MAX_FLAGS) this.flags.shift();
      }
    }
  }

  /** Whether a pulse is due at page time `now`, given the daemon's clock minus the page's;
   * each beat is due once. */
  due(now, offsetMs) {
    if (now - this.seenAt > VIZ.STALE_MS) return false;
    if (this.period === null) return this.dueFlag(now, offsetMs);
    const at = this.nextBeat();
    if (at - offsetMs > now) return false;
    // The beats already too late are passed over in one step, however many there are: the first
    // beat that is not is `first`, due now or still to come.
    const horizon = now + offsetMs - VIZ.LATE_BEAT_MS;
    const first = at >= horizon ? at
      : this.anchor + Math.ceil((horizon - this.anchor) / this.period) * this.period;
    if (first - offsetMs > now) {
      this.fired = first - this.period;
      return false;
    }
    this.fired = first;
    return true;
  }

  /** The next beat of the grid, on the daemon's clock: the first not within half a period of
   * the one last pulsed. */
  nextBeat() {
    if (this.fired === null) return this.anchor;
    const first = this.fired + this.period / 2;
    return this.anchor + Math.ceil((first - this.anchor) / this.period) * this.period;
  }

  /** Without a tempo: the flags that are waiting, oldest first; one more than `LATE_BEAT_MS`
   * behind, or within `MIN_PULSE_GAP_MS` of the last pulse, is let go; the first other pulses. */
  dueFlag(now, offsetMs) {
    while (this.flags.length > 0) {
      const at = this.flags[0];
      if (at - offsetMs > now) return false;
      this.flags.shift();
      if (now - (at - offsetMs) > VIZ.LATE_BEAT_MS) continue;
      if (this.fired !== null && at - this.fired < VIZ.MIN_PULSE_GAP_MS) continue;
      this.fired = at;
      return true;
    }
    return false;
  }
}

/**
 * What the card draws. Frames are pushed as they arrive and one is taken per draw (`step`):
 * the newest waiting, the older ones left out, none left out when only one waits however late it
 * is. The curves are smoothed with the share `SMOOTHING` of the last frame kept per frame
 * interval (so a slower draw does not change the look; with no frame for `STALE_MS` the curves
 * fall as in silence, so a stream that stalls does not freeze the picture), the max hold keeps
 * each band's highest smoothed level for `HOLD_MS` and then lets it fall `FALL_PER_S` until it
 * meets the curve.
 */
export class VizModel {
  constructor() {
    this.left = new Float32Array(BANDS);
    this.right = new Float32Array(BANDS);
    this.holdLeft = new Float32Array(BANDS);
    this.holdRight = new Float32Array(BANDS);
    this.peakLeft = new Float32Array(BANDS);
    this.peakRight = new Float32Array(BANDS);
    this.peakAtLeft = new Float64Array(BANDS);
    this.peakAtRight = new Float64Array(BANDS);
    this.levels = new Float32Array(4);
    this.offsetMs = null;
    this.delays = new DelayMedian();
    this.beats = new BeatClock();
    this.clear();
  }

  /** Back to silence: the curves, the hold, the levels and everything waiting or scheduled. */
  clear() {
    for (const a of [this.left, this.right, this.holdLeft, this.holdRight, this.peakLeft,
      this.peakRight, this.levels]) a.fill(0);
    this.queue = [];
    this.newest = -Infinity;
    this.lastFrameAt = null;
    this.lastStepAt = null;
    this.delays.clear();
    this.beats.clear();
  }

  /** The stream said it was `daemonNowMs` on the daemon's clock when the page's read `pageNowMs`;
   * a new stream starts what waits and what is scheduled afresh, the picture stays. */
  hello(daemonNowMs, pageNowMs) {
    this.offsetMs = daemonNowMs - pageNowMs;
    this.queue = [];
    this.newest = -Infinity;
    this.delays.clear();
    this.beats.clear();
  }

  /** The daemon's clock when the page's reads `pageMs`. */
  daemonClock(pageMs) {
    return pageMs + (this.offsetMs ?? 0);
  }

  /** The page's clock when the daemon's reads `daemonMs`. */
  pageClock(daemonMs) {
    return daemonMs - (this.offsetMs ?? 0);
  }

  /** A `frame` event read at page time `now`. Returns whether it was taken: not when it is not a
   * frame, and not when a frame at least as new was already taken (a late one that arrives after
   * a newer one is of no use). Without a `hello` the first frame is taken to have no delay. */
  push(wire, now) {
    const frame = decode(wire);
    if (!frame || frame.t <= this.newest) return false;
    if (this.offsetMs === null) this.offsetMs = frame.t - now;
    this.newest = frame.t;
    this.queue.push(frame);
    if (this.queue.length > VIZ.QUEUE) this.queue.splice(0, this.queue.length - VIZ.QUEUE);
    this.delays.add(now, Math.max(0, now - this.pageClock(frame.t)));
    this.beats.observe(frame, now);
    return true;
  }

  /** One draw at page time `now`: the newest frame waiting is applied (`frame`, or null when none
   * waits) and `dropped` older ones are left out. The hold moves with the time either way. */
  step(now) {
    const frame = this.queue.length ? this.queue[this.queue.length - 1] : null;
    const dropped = Math.max(0, this.queue.length - 1);
    this.queue = [];
    const sinceStep = this.lastStepAt === null ? VIZ.FRAME_MS : now - this.lastStepAt;
    this.lastStepAt = now;
    if (frame) {
      const dt = this.lastFrameAt === null ? VIZ.FRAME_MS : this.span(now - this.lastFrameAt);
      this.lastFrameAt = now;
      this.apply(frame, dt);
    } else if (this.lastFrameAt !== null && now - this.lastFrameAt > VIZ.STALE_MS) {
      // Silence is applied at every draw, by the time since the last draw: the curves fall at
      // the pace they do between frames, not in one step.
      this.apply(SILENCE, this.span(sinceStep));
    }
    this.hold(now, this.left, this.holdLeft, this.peakLeft, this.peakAtLeft);
    this.hold(now, this.right, this.holdRight, this.peakRight, this.peakAtRight);
    return { frame, dropped };
  }

  /** Whether a beat pulse is due at page time `now`. */
  beatDue(now) {
    return this.beats.due(now, this.offsetMs ?? 0);
  }

  /** The median delay of the frames of the last 2 s, in ms: how far behind the sound they are
   * when they arrive; null when none came. */
  delayMs(now) {
    return this.delays.median(now);
  }

  /** A time between two draws or frames, ms, kept within 0 and `MAX_DT_MS` for the smoothing. */
  span(ms) {
    return Math.min(VIZ.MAX_DT_MS, Math.max(0, ms));
  }

  /** Smooth the curves toward `frame`, which `dt` ms of frame time after the last one. */
  apply(frame, dt) {
    const keep = VIZ.SMOOTHING ** (dt / VIZ.FRAME_MS);
    for (let i = 0; i < BANDS; i++) {
      this.left[i] = this.left[i] * keep + unit(frame.left[i]) * (1 - keep);
      this.right[i] = this.right[i] * keep + unit(frame.right[i]) * (1 - keep);
    }
    for (let i = 0; i < 4; i++) this.levels[i] = unit(frame.levels[i]);
  }

  /** The hold of one channel at `now`: a band's peak is the highest the curve reached, kept for
   * `HOLD_MS` from when it was reached, then lowered by `FALL_PER_S` for the time since; the
   * hold is that, or the curve when that is higher. */
  hold(now, curve, held, peak, peakAt) {
    for (let i = 0; i < BANDS; i++) {
      const fall = Math.max(0, now - peakAt[i] - VIZ.HOLD_MS) / 1000 * VIZ.FALL_PER_S;
      const value = peak[i] - fall;
      if (curve[i] >= value) {
        peak[i] = curve[i];
        peakAt[i] = now;
        held[i] = curve[i];
      } else {
        held[i] = value;
      }
    }
  }
}

/** The place of `hz` along an axis of `size` pixels, 20 Hz at 0 and 20 kHz at `size`, on a log
 * scale. */
export function freqPos(hz, size) {
  return (Math.log(hz / VIZ.F_MIN) / Math.log(VIZ.F_MAX / VIZ.F_MIN)) * size;
}

/** The frequency at place `pos` of an axis of `size` pixels; the other way of [`freqPos`]. */
export function posFreq(pos, size) {
  return VIZ.F_MIN * (VIZ.F_MAX / VIZ.F_MIN) ** (pos / size);
}

/** Band `i`'s lower edge, Hz. */
export function bandEdgeHz(i) {
  return VIZ.F_MIN * 2 ** (i / BANDS_PER_OCTAVE);
}

/** Band `i`'s centre, Hz (on the log scale, half a band above its lower edge). */
export function bandCentreHz(i) {
  return VIZ.F_MIN * 2 ** ((i + 0.5) / BANDS_PER_OCTAVE);
}

/** The place of band `i`'s centre along an axis of `size` pixels. The top band's centre is a
 * little past 20 kHz, so its place is a little past `size`. */
export function bandPos(i, size) {
  return freqPos(bandCentreHz(i), size);
}

/** The band that place `pos` of an axis of `size` pixels falls in, the first or the last when it
 * is off the ends. */
export function posBand(pos, size) {
  const band = Math.floor(Math.log2(posFreq(pos, size) / VIZ.F_MIN) * BANDS_PER_OCTAVE);
  return Math.min(BANDS - 1, Math.max(0, band));
}

/** A spectrogram column's band for each of `height` rows, taken at the row's middle: row 0 (the
 * top) holds the highest. Between the 20 Hz and 20 kHz edges, so a tall card has several rows
 * for one low band and a short one skips bands. */
export function rowBands(height) {
  return Uint8Array.from({ length: height }, (_, y) => posBand(height - (y + 0.5), height));
}

/** The row of a level `v` of 0 to 1 in a graph `height` rows tall, row 0 at the top. */
export function levelY(v, height) {
  return (1 - Math.min(1, Math.max(0, v))) * height;
}
