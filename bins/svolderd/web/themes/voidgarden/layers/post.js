/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's post layer: it installs the post chain (tsl/post.js) as the frame's render and moves
// its strengths each frame from the rig, the director's mix, the signal cues and the accents. The
// accents (the barcode strips, the datamosh, the pixel sort, the VHS tracking, and the anamorphic
// flare) are short bursts the section's music calls for; every one of them goes through the flash
// limiter before it fires, none fires in Calm or while muted, and two never start within 0.4 s of
// each other.
//
//   FAMILIES                      the accents a section draws from
//   createAccents()               `fire(kind, options)`, `onEvents(events, options)`, `step(dt)`,
//                                 `levels()`: the accents' gate and envelopes
//   postLayer(ctx)                the layer factory; the layer carries `levels()`, `accents`,
//                                 `setEffectsOff(names)` and `post` for the checks. The frame it is
//                                 handed carries the director's `events`, `director`, `section`,
//                                 `cues` and `flash` (its limiter); without a `flash` the layers
//                                 of a scene share one limiter, `ctx.fx` (the checks').

import { FlashLimiter } from '../director.js';
import {
  EFFECTS, createMix, createPost, eventsOf, flashOf, grooveOf, levelsOf,
} from '../tsl/post.js';

/** The accents a section draws from, in turn (a drop draws two). */
export const FAMILIES = Object.freeze({
  intro: ['flare'],
  build: ['vhs', 'mosh'],
  drop: ['mosh', 'vhs', 'sort', 'flare'],
  break: ['vhs', 'flare'],
});

/** The least time between two accents, and the length bounds of one, seconds. */
export const ACCENT_GAP_S = 0.4;
const DUR = { min: 0.3, max: 2 };
/** How fast the chromatic kick of a peak decays, per second. */
const CA_DECAY = 3;
/** The envelope's rise, seconds. */
const RISE_S = 0.09;

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const ease = (dt, rate) => 1 - Math.exp(-dt * rate);

/**
 * The accents' gate and envelopes. A request must pass, in order: not Calm, not muted, 0.4 s since
 * the last accent, then the flash limiter (`flash(t)` in the options, else the gate's own
 * limiter): so the accents and the director's flashes together never pass the limiter's count.
 */
export function createAccents() {
  const own = new FlashLimiter();
  const active = {};
  const state = { lastAt: -Infinity, turn: 0, fires: 0, ca: 0 };

  function fire(kind, {
    t, dur = 1, k = 1, calm = false, mute = false, flash = null,
  }) {
    if (calm || mute || t - state.lastAt < ACCENT_GAP_S) return false;
    if (!(flash ? flash(t) : own.grant(t))) return false;
    state.lastAt = t;
    state.fires++;
    active[kind] = { age: 0, dur: clamp(dur, DUR.min, DUR.max), k };
    return true;
  }

  /** The next accent of `section`'s family. */
  function next(section, { entry = false, bpm = 120, ...rest }) {
    const family = FAMILIES[section] ?? FAMILIES.build;
    const kind = family[state.turn++ % family.length];
    const beats = section === 'drop' ? (entry ? 2 : 1) : section === 'build' ? 2 : 4;
    return fire(kind, { ...rest, dur: (beats * 60) / bpm, k: entry ? 1 : 0.8 });
  }

  return {
    fire,
    state,
    /** Nothing active, nothing counted: a seed's start. */
    reset() {
      for (const kind of Object.keys(active)) delete active[kind];
      Object.assign(state, { lastAt: -Infinity, turn: 0, fires: 0, ca: 0 });
      own.clear();
    },
    /** The director's events of a step: a section's entry, a peak, a bar's start and a drop's
     * off-beats call for an accent from the section's family; a hat in a drop for the barcode;
     * a peak that was granted a flash kicks the chromatic split. */
    onEvents(events, { section = 'build', bpm = 120, ...rest } = {}) {
      for (const e of events) {
        if (e.type === 'section') {
          // a drop's entry was granted (or refused) by the director: the accent uses that grant
          const granted = e.section === 'drop' && e.flash !== undefined;
          const own = granted ? () => e.flash === true : rest.flash;
          next(e.section ?? section, { entry: true, bpm, ...rest, flash: own });
        } else if (e.type === 'peak') {
          if (e.flash === true && !rest.calm) state.ca = 1;
          // one ask per peak: the accent on it uses the peak's own grant when the director gave one
          const own = e.flash === undefined ? rest.flash : () => e.flash === true;
          next(section, { bpm, ...rest, flash: own });
        } else if (e.type === 'beat') {
          const beat = e.beat ?? 0;
          const bar = Math.floor(beat / 4);
          const start = beat % 4 === 0
            && (section === 'drop' || bar % 4 === 0
              || (section === 'build' && (bar % 2 === 0 || bar % 16 >= 12)));
          const offbeat = section === 'drop' && beat % 4 === 2 && bar % 2 === 1;
          if (start || offbeat) next(section, { bpm, ...rest });
        } else if (e.type === 'hat' && section !== 'intro' && e.amp > 0.5) {
          fire('barcode', { ...rest, dur: 0.6, k: 0.8 });
        }
      }
    },
    /** Move the envelopes `dt` seconds on. */
    step(dt) {
      for (const kind of Object.keys(active)) {
        active[kind].age += dt;
        if (active[kind].age > active[kind].dur) delete active[kind];
      }
      state.ca *= Math.exp(-dt * CA_DECAY);
    },
    /** The level of each accent now (0 to 1): a rise of 0.09 s, a fall over 40 % of its length. */
    levels() {
      const out = {
        barcode: 0, mosh: 0, vhs: 0, sort: 0, flare: 0, ca: state.ca,
      };
      for (const [kind, a] of Object.entries(active)) {
        out[kind] = Math.min(1, a.age / RISE_S)
          * Math.min(1, (a.dur - a.age) / Math.max(0.12, a.dur * 0.4)) * a.k;
      }
      return out;
    },
    /** A number in [0, 1) that changes with each accent, for the barcode's and the sort's
     * pattern. */
    get seed() { return (state.fires * 0.61803398875) % 1; },
  };
}

/** The layer factory. */
export const postLayer = (ctx) => {
  const {
    T, S, scene, camera, gpu, lib, u,
  } = ctx;
  const object = new T.Group();
  const post = createPost({
    T, S, lib, u, scene, camera, renderer: gpu.renderer,
  });
  const accents = createAccents();
  const mix = createMix();
  const seen = { peak: 0 };
  const signal = { vhs: 0, block: 0, sort: 0 };
  let off = new Set();
  let levels = null;

  ctx.render = () => {
    if (object.visible) post.render();
    else gpu.render(scene, camera);
  };

  return {
    name: 'post',
    group: 'post',
    object,
    post,
    accents,
    /** A seed's start: nothing carried over from the frames before it. */
    build() {
      accents.reset();
      mix.reset();
      seen.peak = 0;
      Object.assign(signal, { vhs: 0, block: 0, sort: 0 });
    },
    /** The effects (names of `EFFECTS`) left out, for the bisect. */
    setEffectsOff(names) {
      off = new Set(names);
    },
    /** The strengths of the last frame: `{ strengths, exposure }`. */
    levels: () => levels,
    update(frame) {
      const { u: rig, dt = 0, budget } = frame;
      const groove = grooveOf(frame);
      const calm = rig.calm > 0.5;
      const events = eventsOf(ctx, frame, seen);
      accents.onEvents(events, {
        section: groove.section,
        bpm: groove.bpm,
        t: frame.t,
        calm,
        mute: frame.cues?.mute ?? rig.frost > 0.5,
        flash: flashOf(ctx, frame),
      });
      accents.step(dt);
      mix.step(dt, groove.section);
      // the weak link's slow level, and the link drop's block and sort that fade after it
      signal.vhs += ((rig.static > 0.05 ? 1 : 0) - signal.vhs) * ease(dt, 2);
      if (rig.tear > 0.5) {
        signal.block = 1;
        signal.sort = Math.max(signal.sort, 0.8);
      } else {
        signal.block *= Math.exp(-dt * 4);
      }
      signal.sort *= Math.exp(-dt * 2.2);
      levels = levelsOf({
        u: rig,
        groove,
        mix: mix.state,
        accents: accents.levels(),
        signal,
        budget: budget ?? { grit: true, bloom2: true },
        calm,
        off,
      });
      // the shares first: the update sizes the bloom's texel steps by them
      post.setBack(budget?.back ?? 1);
      post.setPost(budget?.post ?? 1);
      post.update(levels, accents.seed);
      post.setHistory(Boolean(budget?.grit ?? true) && !off.has('mosh'));
      post.setBloom2(Boolean(budget?.bloom2 ?? true) && !off.has('halation'));

    },
    /** The canvas changed size: the chain's size uniforms and history follow, at the last
     * levels. */
    refit() {
      if (levels) post.update(levels, accents.seed);
    },
    dispose() {
      ctx.render = null;
      post.dispose();
    },
  };
};

export { EFFECTS };
