/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's conductor, from the features: the beat (the frame's flags, tempo and phase through
// the core's BeatClock), groove hits from band onsets, the section (intro, build, drop, break) with
// hysteresis and a 3.5 s pending rule, morph and peak triggers, the track change's fade and new
// seed, and the flash limiter. Pure: no DOM, no timers, no randomness; the clock is the sum of the
// `dt` seconds handed to `step`, so the same input gives the same events at any step rate.
//
//   createDirector()       the conductor: `state` (live, read by the layers), `step(dt, input)`
//                          returns the events of this step, `track(title, artist)` starts a
//                          reseed, `flash()` asks the limiter, `reset()` starts afresh
//   new FlashLimiter()     at most `MAX_FLASHES` grants in any `WINDOW_S` seconds
//
// `step` input: `{ frame, raw, values, now, offsetMs, mute, pause }`: `frame` the newly decoded
// frame or null, `raw` and `values` the features' raw and smoothed numbers, `now` the page clock in
// ms and `offsetMs` the daemon's minus the page's (as `VizModel` has them).
// Events: `{ type: 'beat' | 'kick' | 'snare' | 'hat' | 'peak' | 'morph' | 'section' | 'reseed', ... }`.

import { BeatClock } from '../../core/viz-model.js';
import { hashStr } from './rng.js';

/** The most full-frame luminance flashes in any `WINDOW_S` seconds (WCAG 2.3.1). */
export const MAX_FLASHES = 3;
export const WINDOW_S = 1;

/** The section's loudness thresholds: a candidate is entered past `ENTER` and left past `LEAVE`
 * (the gap is the hysteresis). */
export const SECTION = Object.freeze({
  DROP_ENTER: 0.5, DROP_LEAVE: 0.42, BREAK_ENTER: 0.22, BREAK_LEAVE: 0.28, BUILD_FLUX: 0.45,
  /** How long a different section must hold before it takes over, seconds. */
  PENDING_S: 3.5,
});

/** The least between two hits of a kind, seconds (0.2; hats 0.07), and the onset that makes one. */
const HIT_GAP_S = [0.2, 0.2, 0.07];
const HIT_ONSET = 0.09;
/** The time constant of the slow mean an onset is measured against, seconds (it eases 3 a second). */
const ONSET_MEAN_S = 1 / 3;
/** Decay rates of a hit's envelope (per second; the kick's follows the tempo). */
const SNARE_DECAY = 6;
const HAT_DECAY = 16;

/** The least between two peaks, and between two morphs, seconds. */
const PEAK_GAP_S = 0.4;
const MORPH_GAP_S = 3.5;
const MORPH_FLUX = 0.55;

/** How many worlds `explore('scene')` walks through before the track's own comes round again. */
export const WORLDS = 6;

/** The track change's fade out and back in, per second. */
const FADE_OUT = 2.2;
const FADE_IN = 0.9;

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const smooth = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

/** Grants flashes: at most `MAX_FLASHES` in any `WINDOW_S` seconds of the times asked at. */
export class FlashLimiter {
  constructor() {
    this.grants = [];
  }

  /** Whether a flash at time `t` seconds (not before the last asked) is allowed; if so, it counts. */
  grant(t) {
    // a grant exactly `WINDOW_S` ago is outside the window (the epsilon keeps that true when the
    // clock is a sum of steps)
    this.grants = this.grants.filter((g) => t - g < WINDOW_S - 1e-9);
    if (this.grants.length >= MAX_FLASHES) return false;
    this.grants.push(t);
    return true;
  }

  clear() {
    this.grants = [];
  }
}

/** The section's seeds as text: a track's title and artist, the same text the same seed. */
export function seedOf(title, artist) {
  return (hashStr(`${title ?? ''}\u0000${artist ?? ''}`) % 10000) / 100;
}

export function createDirector() {
  const clock = new BeatClock();
  const limiter = new FlashLimiter();
  const hits = [0, 1, 2].map(() => ({ since: 9, mean: 0, prev: 0, amp: 0 }));
  let state;
  let pendingSeed;
  let frameGap = 1 / 30;
  let lastFrameAt = null;
  // the explore keys: the track's own seed, the world shown of it and the effect asked for
  let base = null;
  let salt = 0;
  let world = 0;
  let effects = 0;
  let asked = null;

  function reset() {
    base = null;
    salt = 0;
    world = 0;
    effects = 0;
    asked = null;
    clock.clear();
    limiter.clear();
    for (const h of hits) Object.assign(h, { since: 9, mean: 0, prev: 0, amp: 0 });
    pendingSeed = null;
    frameGap = 1 / 30;
    lastFrameAt = null;
    state = {
      time: 0,
      bpm: 120,
      beat: 0,
      beats: 0,
      groove: { kick: 0, snare: 0, hat: 0, energy: 0.3, bar: 0, down: 0 },
      hits: [0, 0, 0],
      section: 'intro',
      pending: null,
      pendingFor: 0,
      sawDrop: false,
      morph: 0,
      morphs: 0,
      lastMorph: -9,
      lastPeak: -9,
      peak: 0,
      over: 0,
      seed: 0,
      fade: 1,
      fadeDir: 0,
    };
  }
  reset();

  /** What the loudness and flux call for now, given the section the conductor is in (hysteresis). */
  function candidate(values) {
    const { loud, flux } = values;
    const now = state.section;
    const dropAt = now === 'drop' ? SECTION.DROP_LEAVE : SECTION.DROP_ENTER;
    if (loud > dropAt) return 'drop';
    const quietAt = now === 'break' || now === 'intro' ? SECTION.BREAK_LEAVE : SECTION.BREAK_ENTER;
    if (loud < quietAt && flux < SECTION.BUILD_FLUX) return state.sawDrop ? 'break' : 'intro';
    return 'build';
  }

  function sectionStep(dt, values, events) {
    const want = candidate(values);
    if (want === state.section) {
      state.pending = null;
      state.pendingFor = 0;
      return;
    }
    if (want !== state.pending) {
      state.pending = want;
      state.pendingFor = 0;
    }
    state.pendingFor += dt;
    if (state.pendingFor <= SECTION.PENDING_S) return;
    state.section = want;
    state.pending = null;
    state.pendingFor = 0;
    if (want === 'drop') state.sawDrop = true;
    events.push({ type: 'section', section: want, flash: want === 'drop' && limiter.grant(state.time) });
  }

  function grooveStep(dt, frame, raw, still, events) {
    const src = [raw.bass, raw.mid, raw.high];
    for (let i = 0; i < 3; i++) {
      const h = hits[i];
      h.since += dt;
      if (!frame) continue;
      const v = still ? 0 : src[i];
      const onset = v - h.mean;
      // the slow mean is measured per frame, over the time since the last
      h.mean += (v - h.mean) * (1 - Math.exp(-frameGap / ONSET_MEAN_S));
      if (onset > HIT_ONSET && h.since > HIT_GAP_S[i]) {
        h.since = 0;
        h.amp = clamp(onset * 3);
        state.hits[i]++;
        events.push({ type: ['kick', 'snare', 'hat'][i], amp: h.amp });
      }
    }
    const g = state.groove;
    const decay = (h, k) => h.amp * smooth(0, 0.035, h.since) * Math.exp(-h.since * k);
    g.kick = decay(hits[0], (3 * state.bpm) / 60);
    g.snare = decay(hits[1], SNARE_DECAY);
    g.hat = decay(hits[2], HAT_DECAY);
  }

  function step(dt, {
    frame = null, raw, values, now = 0, offsetMs = 0, mute = false, pause = false,
  }) {
    const events = [];
    const still = pause || mute;
    state.time += dt;
    if (frame) {
      frameGap = lastFrameAt === null ? 1 / 30 : Math.max(state.time - lastFrameAt, 1 / 240);
      lastFrameAt = state.time;
    }

    // the beat: the clock takes the frame, and each beat that is due is one event
    if (frame) {
      clock.observe(frame, now);
      if (frame.bpm) state.bpm = frame.bpm;
    }
    state.beat += (dt * state.bpm) / 60;
    if (!pause && clock.due(now, offsetMs)) {
      state.beat += (Math.round(state.beat) - state.beat) * 0.35;
      state.beats++;
      events.push({ type: 'beat', beat: state.beats, bpm: state.bpm });
    }

    grooveStep(dt, frame, raw, still, events);
    const g = state.groove;
    const energyTo = still ? 0.1 : clamp(values.loud * 1.6);
    g.energy += (energyTo - g.energy) * (1 - Math.exp(-dt * (energyTo > g.energy ? 0.9 : 0.5)));
    g.bar = (state.beat / 4) % 1;
    g.down = Math.exp(-g.bar * 14);

    if (!still) sectionStep(dt, values, events);

    // morph and peak
    if (!mute) {
      if (frame && raw.peak && !pause && state.time - state.lastPeak > PEAK_GAP_S) {
        state.lastPeak = state.time;
        state.peak = 1;
        state.over = 1;
        events.push({ type: 'peak', flash: limiter.grant(state.time) });
      }
      if (values.flux > MORPH_FLUX && state.time - state.lastMorph > MORPH_GAP_S) {
        state.lastMorph = state.time;
        state.morph = 1 - state.morph;
        state.morphs++;
        events.push({ type: 'morph', morph: state.morph });
      }
    }
    // an effect asked for from the page: one of the scene's two (a peak, a morph), the peak's flash
    // through the same limiter as any other
    if (asked) {
      const { flash } = asked;
      asked = null;
      if (effects++ % 2 === 0) {
        state.peak = 1;
        events.push({ type: 'peak', flash: flash && limiter.grant(state.time), explore: true });
      } else {
        state.morph = 1 - state.morph;
        state.morphs++;
        events.push({ type: 'morph', morph: state.morph, explore: true });
      }
    }
    state.over = Math.max(0, state.over - dt * 2.5);
    state.peak *= Math.exp(-dt * 3);

    // the track change: fade out, take the new seed at black, fade back in
    if (state.fadeDir < 0) {
      state.fade -= dt * FADE_OUT;
      if (state.fade <= 0) {
        state.fade = 0;
        state.fadeDir = 1;
        if (pendingSeed !== null) {
          state.seed = pendingSeed;
          pendingSeed = null;
          state.section = 'intro';
          state.sawDrop = false;
          state.pending = null;
          events.push({ type: 'reseed', seed: state.seed });
        }
      }
    } else if (state.fadeDir > 0) {
      state.fade += dt * FADE_IN;
      if (state.fade >= 1) {
        state.fade = 1;
        state.fadeDir = 0;
      }
    }
    return events;
  }

  return {
    get state() { return state; },
    step,
    reset,
    /** A new track: fade out, reseed from its title and artist, fade back in. */
    track(title, artist) {
      pendingSeed = seedOf(title, artist);
      base = pendingSeed;
      salt = 0;
      world = 0;
      state.fadeDir = -1;
    },
    /**
     * An explore key of the page: `seed` a new set of worlds for the track (reseeded through the
     * fade), `scene` the next world of the set (the track's own comes round again after
     * `WORLDS`), `effect` one of the scene's effects at the next step. The seeds are a hash of the
     * track's seed, the set and the world: no randomness, so a seed's frame is the same whichever
     * key led to it. `flash` says whether a flash may be drawn (the Motion level is Full).
     */
    explore(kind, { flash = true } = {}) {
      if (kind === 'effect') {
        asked = { flash };
        return;
      }
      if (kind !== 'seed' && kind !== 'scene') return;
      base ??= state.seed;
      if (kind === 'seed') {
        salt++;
        world = 0;
      } else {
        world = (world + 1) % WORLDS;
      }
      pendingSeed = salt === 0 && world === 0 ? base
        : (hashStr(`${base}\u0000${salt}\u0000${world}`) % 10000) / 100;
      state.fadeDir = -1;
    },
    /** Whether a full-frame flash may be drawn now (the limiter's, whichever the Motion level). */
    flash() {
      return limiter.grant(state.time);
    },
  };
}
