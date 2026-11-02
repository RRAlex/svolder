/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The smoke's director: every rule of the smoke that needs no GPU. It takes the page's events
// and the time from its caller and a `random` function (never a clock, the DOM or WebGL), and
// answers, frame by frame, a plan of what the engine is to do: whether to simulate, wait or stop,
// the step's length, the splats to apply, the note to stamp, the band fade, the grid shift and how
// to draw. Same events, same times and same `random` give the same plans.

import {
  Haze, HazeClock, NoteGate, SettleWatch, TUNING, beatPuff, clickPuff, dialDot, drawShade, dyeOf,
  ease, frameDt, hazeBands, lighten, meterAgitation, meterEmission, noteCell, notePlacement,
  notePosition, peakOf, pointerAgitation, pointerSplat, radiusUv, restSplat, rhythm, scrollStir,
  shadePick, shiftUv, sourceStrength, stepPuff, stepStir, uvOf, wander,
} from './smoke-model.js';

/**
 * What the engine does for one animation frame.
 *
 * @typedef {object} FramePlan
 * @property {'run'|'skip'|'stop'} action `run` simulates this frame and asks for the next, `skip`
 *   only waits for the next, `stop` asks for no more until something wakes the loop.
 * @property {number} dt The step's length in seconds (0 unless `run`).
 * @property {boolean} away Whether the away fade is on.
 * @property {boolean} vorticity Whether this step adds vorticity confinement (not while away).
 * @property {number} velocityLoss What the velocity loses a second on top of its usual loss (the
 *   away fade's, else 0).
 * @property {Array<object>} splats In the order they are applied: `{ x, y, radius }` in uv and
 *   units of the canvas's height as `uvOf` and `radiusUv` give them, `rest` (counts as calm),
 *   `kind` (`pointer`, `puff`, `meter` or `scroll`, none for the resting and haze sources),
 *   `add`, `pull` and `rate` for the velocity and `dye` for the dye, as `splatsFor` always gave.
 * @property {[number, number]|null} shift The grids' move in uv before the frame, if any.
 * @property {{lo: number, hi: number, keep: number}|null} bandFade The dye fade in the content's
 *   place: the band's uv bounds and the share of the dye kept this step.
 * @property {{x: number, y: number, uv: {x: number, y: number}, angle: number, aspect: number,
 *   size: number, cell: number, glyph: number, tilt: number, tint: number[]}|null} note A note to
 *   stamp (`x`, `y` and `size` in CSS px, `angle` in radians, `tint` as rgb 0 to 1).
 * @property {{bright: number, sat: number}} draw The brightness and saturation to draw with.
 * @property {boolean} redraw Whether to draw the dye on the canvas.
 */

/** The longest canvas side the director plans for (CSS px); a larger one is not a canvas. */
const MAX_SIDE = 1e7;

/** The shortest canvas side the director plans for (CSS px); a smaller one is not a canvas. */
const MIN_SIDE = 1;

/** A size `[width, height]` that is two finite numbers from `MIN_SIDE` to `MAX_SIDE`. */
const usable = (size) => Array.isArray(size) && size.length === 2
  && size.every((v) => Number.isFinite(v) && v >= MIN_SIDE && v <= MAX_SIDE);

/** The director of one smoke. */
export class Director {
  /**
   * @param {object} options
   * @param {() => number} options.random Gives [0, 1); the only source of chance.
   * @param {number} options.now The creation time in seconds.
   * @param {object} [options.shade] The shade to use (one is drawn from `random` when absent).
   * @param {number|null} [options.roll] Make every settle's roll come out as this (`null`:
   *   `random`'s).
   * @param {boolean} [options.notes] Whether notes can be stamped (false without an atlas).
   * @param {((x: number, y: number, half: number) => boolean)|null} [options.open] Whether the
   *   box of `half` px around `x`, `y` (viewport px) shows the smoke rather than a control.
   * @param {(() => ((x: number, y: number, half: number) => number)|null)|null} [options.density]
   *   Called once when a note is to be placed: a function giving the mean dye (amount) in the
   *   box of `half` px around `x`, `y` (viewport px), or `null` when the dye cannot be read (every
   *   place then counts as dense). A note forms at the nearest place that is open and dense.
   */
  constructor({ random, now, shade, roll = null, notes = true, open = null, density = null }) {
    this.random = random;
    this.shade = shade ?? drawShade(random);
    this.haze_ = new Haze(random, this.shade);
    this.canNote = notes;
    this.open = typeof open === 'function' ? open : null;
    this.density = typeof density === 'function' ? density : null;
    this.forcedRoll = roll ?? null;
    this.clockAt = Number.isFinite(now) ? now : 0;
    this.target = 0;
    this.lvl = 0;
    this.sat = 1;
    this.dim = 1;
    this.isAway = false;
    this.fadedAt = null;
    this.isLost = false;
    this.isVisible = true;
    this.last = this.clockAt;
    this.lastFrame = -Infinity;
    this.lastStir = -Infinity;
    this.dirty = true;
    this.settled = false;
    this.shiftX = 0;
    this.shiftY = 0;
    this.scrollDy = 0;
    this.scrollV = 0;
    this.restAngle = 2 * Math.PI * random();
    this.meterAngles = [];
    this.stirRgb = this.shade.base;
    this.stirUntil = 0;
    this.anchorAt = null;
    this.columnAt = null;
    this.innerSeen = null;
    this.fadeUntil = -Infinity;
    this.hazeOn = true;
    this.hazeClock = new HazeClock();
    this.pointerQueue = [];
    this.puffQueue = [];
    this.meterTargets = [];
    this.meterShown = [];
    this.watch = new SettleWatch();
    this.gate = new NoteGate();
    this.noted = { stamped: 0, last: null };
    this.lastPlace = null;
    this.pointerSeen = new Map();
    this.counts = { frames: 0, dt: 0, splats: { pointer: 0, puff: 0, meter: 0, scroll: 0 } };
  }

  /** The time to use for an event: `at` when it is a finite number, else the last time seen. */
  time(at) {
    if (Number.isFinite(at)) this.clockAt = Math.max(this.clockAt, at);
    return Number.isFinite(at) ? at : this.clockAt;
  }

  /** The level the smoke eases to, 0 to 1 (anything else is ignored). Returns `true`: wake. */
  level(x) {
    if (Number.isFinite(x)) this.target = Math.min(1, Math.max(0, x));
    return true;
  }

  /** Set the anchor (the dial's place, its meters and the content column) or clear it with
   * `null`; a different count of meters forgets their readings. Returns `true`: wake. */
  anchor(a) {
    const before = this.anchorAt;
    this.column(a?.column);
    this.anchorAt = a && Number.isFinite(a.x) && Number.isFinite(a.y)
      ? {
        x: a.x,
        y: a.y,
        radius: Number.isFinite(a.radius) ? a.radius : 0,
        meters: (a.meters ?? []).map((m) => (m && Number.isFinite(m.x) && Number.isFinite(m.y)
          ? { x: m.x, y: m.y } : null)),
      }
      : null;
    // readings belong to the anchor's meters: a different count, or none, forgets them
    if (before && (!this.anchorAt || this.anchorAt.meters.length !== before.meters.length)) {
      this.meterTargets = [];
      this.meterShown = [];
      this.meterAngles = [];
    }
    return true;
  }

  /** Set the content column `{ left, right }` (viewport px) the haze keeps clear, or none. */
  column(c) {
    this.columnAt = c && Number.isFinite(c.left) && Number.isFinite(c.right)
      ? { left: c.left, right: c.right } : null;
    return true;
  }

  /** A volume step of `size` in `direction` at the dial position `at`, `big` when it is part of
   * a burst over `bigRiseDb`. Returns `true` when the step was taken (wake). */
  step(direction, size, at, big = false, now = undefined) {
    if (this.isAway || !this.anchorAt || !(this.anchorAt.radius > 0)) return false;
    if (![direction, size, at].every(Number.isFinite)) return false;
    const t = this.time(now);
    const dot = dialDot(this.anchorAt, at);
    const odds = big ? TUNING.bigRiseOdds : TUNING.settleOdds;
    this.watch.stir(stepStir(size), dot.x, dot.y, t, odds);
    this.puffQueue.push({
      ...stepPuff(dot, direction, size, this.target), rgb: shadePick(this.shade, this.random),
    });
    this.trimPuffs();
    this.lastStir = t;
    return true;
  }

  /** A control was clicked at `x`, `y` (viewport px): a small puff and a stir whose settle makes
   * a note at the click's odds. Returns `true` when taken (wake). */
  click(x, y, now = undefined) {
    if (this.isLost || this.isAway || !Number.isFinite(x) || !Number.isFinite(y)) return false;
    const t = this.time(now);
    this.puffQueue.push({
      ...clickPuff({ x, y }, this.random() * 2 * Math.PI), rgb: shadePick(this.shade, this.random),
    });
    this.trimPuffs();
    this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.clickOdds);
    this.lastStir = t;
    return true;
  }

  /** A beat of the music of `strength` (0 to 1): a puff from the dial's track outward, with no
   * stir (a beat never arms a note), at the dial position `at` as a step takes it (the knob's
   * fraction), else at the glow's level. Returns `true` when taken (wake). */
  beat(strength, now = undefined, at = undefined) {
    if (this.isLost || this.isAway || !this.anchorAt || !(this.anchorAt.radius > 0)) return false;
    if (!Number.isFinite(strength)) return false;
    this.time(now);
    const dot = dialDot(this.anchorAt, Number.isFinite(at) ? at : this.target);
    this.puffQueue.push({
      ...beatPuff(dot, this.anchorAt, strength), rgb: shadePick(this.shade, this.random),
    });
    this.trimPuffs();
    return true;
  }

  /** The puffs waiting are bounded. */
  trimPuffs() {
    if (this.puffQueue.length > TUNING.maxPuffs) {
      this.puffQueue.splice(0, this.puffQueue.length - TUNING.maxPuffs);
    }
  }

  /** One move of the pointer `id` to `here` (`x`, `y` in viewport px, `t` in seconds): a push
   * along the motion from where that pointer was last seen, and a dab of dye when `pressed`. A
   * pause longer than the stroke gap starts a fresh stroke. `size` is the canvas's `[w, h]`.
   * Returns `true` when pushes are waiting (wake). */
  pointer(id, here, pressed, size, now = undefined) {
    if (this.isLost || this.isAway || !usable(size)) return false;
    if (![here?.x, here?.y, here?.t].every(Number.isFinite)) return false;
    const t = this.time(now);
    const before = this.pointerSeen.get(id);
    // a move of zero length still replaces the previous position (and its time), so the next
    // move is measured from now
    this.pointerSeen.delete(id);
    this.pointerSeen.set(id, here);
    if (this.pointerSeen.size > 10) this.pointerSeen.delete(this.pointerSeen.keys().next().value);
    const short = Math.min(size[0], size[1]);
    if (before && here.t - before.t <= TUNING.pointerGapSecs) {
      const push = pointerSplat({
        dx: here.x - before.x, dy: here.y - before.y, dt: here.t - before.t,
        shortSide: short, pressed,
      });
      if (push.fx !== 0 || push.fy !== 0 || push.dye !== 0) {
        this.pointerQueue.push({ x: here.x, y: here.y, ...push });
        this.lastPlace = { x: here.x, y: here.y };
        this.watch.stir(pointerAgitation({
          dist: Math.hypot(here.x - before.x, here.y - before.y), dt: here.t - before.t,
          shortSide: short,
        }), here.x, here.y, t, TUNING.settleOdds);
      }
    }
    if (this.pointerQueue.length > TUNING.maxPointerSplats) {
      this.pointerQueue.splice(0, this.pointerQueue.length - TUNING.maxPointerSplats);
    }
    if (this.pointerQueue.length > 0) {
      this.lastStir = t;
      return true;
    }
    return false;
  }

  /** A finger landed at `here` (`x`, `y`, `t`): its first move is measured from there. */
  touchStart(id, here) {
    if (this.isLost || this.isAway) return;
    if (![here?.x, here?.y, here?.t].every(Number.isFinite)) return;
    this.pointerSeen.delete(id);
    this.pointerSeen.set(id, here);
  }

  /** The pointer left the page, or a finger lifted: a later scroll has no place to stir at. */
  leave() {
    this.lastPlace = null;
  }

  /** The page scrolled by `dx`, `dy` CSS pixels at `now`: the grids move with the content and the
   * scroll stirs. Returns `true`: wake. */
  scroll(dx, dy, now = undefined) {
    if (Number.isFinite(dx) && Number.isFinite(dy)) {
      const t = this.time(now);
      const x = this.shiftX + dx;
      const y = this.shiftY + dy;
      if (Number.isFinite(x) && Number.isFinite(y)) {
        this.shiftX = x;
        this.shiftY = y;
      }
      const d = this.scrollDy + dy;
      this.scrollDy = Number.isFinite(d) ? d : 0;
      if (dy !== 0 && !this.isAway) this.lastStir = t;
    }
    return true;
  }

  /** The meters' readings (fractions 0 to 1, anything else is 0). Returns `true` when taken. */
  meters(fractions) {
    if (!Array.isArray(fractions)) return false;
    this.meterTargets = fractions.map((f) => (Number.isFinite(f) ? Math.min(1, Math.max(0, f))
      : 0));
    return true;
  }

  /** Go away (the fade to grey and rest) or come back at `now`. Returns `true` when it changed
   * (wake). */
  away(on, now = undefined) {
    const next = Boolean(on);
    if (next === this.isAway) return false;
    const t = this.time(now);
    this.isAway = next;
    if (next) {
      this.meterTargets = [];
      this.meterShown = [];
      this.meterAngles = [];
    }
    this.watch.reset();
    this.fadedAt = next ? t + TUNING.fadeSecs : null;
    this.settled = false;
    return true;
  }

  /** The browser took the context (`true`) or gave it back: a pending settle is forgotten. */
  lost(on) {
    this.isLost = Boolean(on);
    if (this.isLost) this.watch.reset();
  }

  /** Pause (`false`) or resume the haze's feeding. Returns whether the engine is to empty its
   * grids (when paused). */
  haze(on) {
    this.hazeOn = Boolean(on);
    return !this.hazeOn;
  }

  /** Make every settle's roll come out as `v` (`null`, or anything not a number: random). */
  roll(v) {
    this.forcedRoll = Number.isFinite(v) ? v : null;
  }

  /** The page became visible (`true`) or hidden at `now`: time hidden is never simulated. A
   * pending settle is kept: it rolls on return. */
  visible(on, now = undefined) {
    this.isVisible = Boolean(on);
    if (this.isVisible) this.last = this.time(now);
  }

  /** The loop is asked for again at `now`: time spent stopped is not simulated. */
  wake(now = undefined) {
    this.last = this.time(now);
    this.settled = false;
  }

  /** The canvas changed size or content: the next frame draws. */
  resized() {
    this.dirty = true;
  }

  /** Whether the director has nothing to plan: the context is lost or the page hidden. */
  idle() {
    return this.isLost || !this.isVisible;
  }

  /** The time (seconds) of the next frame that would simulate, or `null` when none is due. */
  wakeAt() {
    if (this.idle()) return null;
    if (this.isAway && this.fadedAt !== null && this.clockAt >= this.fadedAt) return null;
    const stirredUntil = this.lastStir + TUNING.stirredSecs;
    if (this.clockAt < stirredUntil) return this.clockAt;
    return Math.max(this.clockAt, this.lastFrame + 1 / TUNING.calmFps - TUNING.calmSlack);
  }

  /** The director's part of the handle's `stats()`: the frames simulated, the last step's length,
   * the splats applied by source and the notes. `now` is the time for the notes still live. */
  stats(now = undefined) {
    const t = Number.isFinite(now) ? now : this.clockAt;
    return {
      ...this.counts,
      splats: { ...this.counts.splats },
      notes: {
        stamped: this.noted.stamped,
        live: this.gate.live(t),
        last: this.noted.last && { ...this.noted.last },
        rolls: this.watch.rolls,
        wins: this.watch.wins,
      },
    };
  }

  /** The plan for the animation frame at `now` (seconds) on a canvas of `size` `[w, h]` CSS px.
   * `resized` is whether the engine changed the canvas or the grids since the last frame. */
  frame(now, size, resized = false) {
    const plan = {
      action: 'stop', dt: 0, away: this.isAway, vorticity: true, velocityLoss: 0, splats: [],
      shift: null, bandFade: null,
      note: null, draw: { bright: this.dim, sat: this.sat }, redraw: false,
    };
    if (this.idle() || !Number.isFinite(now) || !usable(size)) return plan;
    this.clockAt = Math.max(this.clockAt, now);
    if (resized) this.dirty = true;
    const by = shiftUv(this.shiftX, this.shiftY, size);
    this.shiftX = 0;
    this.shiftY = 0;
    if (Number.isFinite(by[0]) && Number.isFinite(by[1]) && (by[0] !== 0 || by[1] !== 0)) {
      plan.shift = by;
      this.dirty = true;
    }
    const action = rhythm({
      now, lastStir: this.lastStir, lastFrame: this.lastFrame, away: this.isAway,
      fadedAt: this.fadedAt, hidden: !this.isVisible,
    });
    if (action === 'stop') {
      if (!this.settled || this.dirty) {
        this.sat = this.isAway ? 0 : this.sat;
        this.dim = this.isAway ? TUNING.awayOpacity : this.dim;
        plan.redraw = true;
        this.settled = true;
        this.dirty = false;
      }
    } else if (action === 'skip') {
      plan.action = 'skip';
      if (this.dirty) {
        plan.redraw = true;
        this.dirty = false;
      }
    } else {
      plan.action = 'run';
      this.run(plan, now, size);
    }
    plan.draw = { bright: this.dim, sat: this.sat };
    return plan;
  }

  /** The part of a frame that simulates: fills `plan` for the step at `now`. */
  run(plan, now, size) {
    this.watchBands(now, size);
    const dt = frameDt(now - this.last);
    this.last = now;
    this.lastFrame = now;
    this.lvl = ease(this.lvl, this.target, dt, TUNING.fadeSecs);
    this.sat = ease(this.sat, this.isAway ? 0 : 1, dt, TUNING.fadeSecs);
    this.dim = ease(this.dim, this.isAway ? TUNING.awayOpacity : 1, dt, TUNING.fadeSecs);
    plan.dt = dt;
    plan.away = this.isAway;
    plan.vorticity = !this.isAway;
    plan.velocityLoss = this.isAway ? TUNING.awayVelocityLoss / TUNING.fadeSecs : 0;
    plan.splats = this.splatsFor(dt, now, size);
    for (const s of plan.splats) {
      if (!s.rest) this.lastStir = now;
      if (s.kind) this.counts.splats[s.kind]++;
    }
    plan.note = this.noteAt(now, size);
    if (now < this.fadeUntil) {
      const inner = hazeBands(size, this.columnAt);
      plan.bandFade = {
        lo: inner.left / size[0], hi: 1 - inner.right / size[0],
        keep: Math.exp(-TUNING.hazeFadeRate * dt),
      };
    }
    plan.redraw = true;
    this.dirty = false;
    this.settled = false;
    this.counts.frames++;
    this.counts.dt = dt;
  }

  /** Start the fade of the dye in the content's place when the haze's bands moved: the old dye
   * was stretched into the new grid and would sit where the content is now. */
  watchBands(now, size) {
    const [cw] = size;
    const b = hazeBands(size, this.columnAt);
    const seen = [b.left / cw, b.right / cw];
    if (this.innerSeen && (Math.abs(seen[0] - this.innerSeen[0]) > TUNING.hazeFadeShare
      || Math.abs(seen[1] - this.innerSeen[1]) > TUNING.hazeFadeShare)) {
      this.fadeUntil = now + TUNING.hazeFadeSecs;
    }
    this.innerSeen = seen;
  }

  /** The scroll's splats for this frame: the page's scroll since the last frame, eased into a
   * speed so a wheel's bursts push steadily, as a few broad pulls along the content's motion
   * over the visible page; none while the page is still. */
  scrollSplats(size, dt, now) {
    const measured = dt > 0 ? this.scrollDy / dt : 0;
    this.scrollDy = 0;
    this.scrollV += (measured - this.scrollV) * (1 - Math.exp(-dt / TUNING.scrollEaseSecs));
    const { push, agit } = scrollStir({
      dy: this.scrollV * dt, dt, shortSide: Math.min(size[0], size[1]),
    });
    // a scroll stirs at the last pointer or finger place, else somewhere in the visible page
    if (agit > 0) {
      const place = this.lastPlace
        ?? { x: this.random() * size[0], y: this.random() * size[1] };
      this.watch.stir(agit, place.x, place.y, now, TUNING.settleOdds);
    }
    if (Math.abs(this.scrollV) < TUNING.scrollFloor * Math.min(size[0], size[1]) || push === 0) {
      this.scrollV = 0;
      return [];
    }
    const out = [];
    const { scrollSplatRows: rows, scrollSplatCols: cols } = TUNING;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        out.push({
          kind: 'scroll',
          ...uvOf({ x: ((c + 0.5) / cols) * size[0], y: ((r + 0.5) / rows) * size[1] }, size),
          radius: radiusUv(TUNING.scrollRadius, size),
          pull: [0, push, 0, 0],
          rate: 1 - Math.exp(-TUNING.scrollPull * dt),
        });
      }
    }
    return out;
  }

  /** This frame's splats, in the order they are applied: the haze, the resting source at the
   * anchor (none without one), the scroll's, the pointer's moves since the last frame, the puffs
   * and the meters'. Nothing stirs while away. */
  splatsFor(dt, now, size) {
    const list = [];
    if (this.isAway) {
      this.pointerQueue = [];
      this.puffQueue = [];
      this.scrollDy = 0;
      this.scrollV = 0;
      return list;
    }
    let fed = [];
    if (this.hazeOn) {
      const owed = this.hazeClock.step(dt);
      if (owed !== null) fed = this.haze_.splats({ size, column: this.columnAt, now, dt: owed });
    }
    for (const h of fed) {
      list.push({
        rest: true,
        ...uvOf(h, size),
        radius: h.radius / size[1], // px to units of the canvas's height
        add: [h.add[0], h.add[1], 0, 0],
        dye: h.dye,
      });
    }
    if (now >= this.stirUntil) {
      this.stirRgb = shadePick(this.shade, this.random);
      this.stirUntil = now + TUNING.restPickSecs;
    }
    this.restAngle = wander(this.restAngle, dt, this.random);
    if (this.anchorAt) {
      list.push(restSplat(this.anchorAt, size, sourceStrength(this.lvl), dt, this.shade.dial,
        this.restAngle));
    }
    list.push(...this.scrollSplats(size, dt, now));
    // each push adds its share of the force, the frame's pushes together one 1/60 s's worth per
    // frame's length, so the mouse's event rate and the frame rate do not change the total
    const share = (TUNING.pointerAdd * 60 * dt) / Math.max(1, this.pointerQueue.length);
    for (const p of this.pointerQueue) {
      list.push({
        kind: 'pointer',
        ...uvOf(p, size),
        radius: radiusUv(TUNING.pointerRadius, size),
        add: [p.fx * share, -p.fy * share, 0, 0],
        dye: p.dye > 0 ? dyeOf(this.stirRgb, p.dye) : null,
      });
    }
    this.pointerQueue = [];
    for (const p of this.puffQueue) {
      list.push({
        kind: 'puff',
        ...uvOf(p, size),
        radius: radiusUv(TUNING.puffRadius, size),
        add: [p.fx * TUNING.puffAdd, -p.fy * TUNING.puffAdd, 0, 0],
        dye: dyeOf(p.rgb, p.dye),
      });
    }
    this.puffQueue = [];
    this.meterSplats(list, dt, now, size);
    return list;
  }

  /** Add the meters' splats to `list`: each meter's centre emits by its reading, eased; a list
   * longer or shorter than the anchor's meters uses the common prefix. The loudest reading
   * (before easing, so a music peak counts) stirs the notes' measure once a frame. */
  meterSplats(list, dt, now, size) {
    const meters = this.anchorAt?.meters ?? [];
    const peak = peakOf(this.meterTargets, meters.length);
    const peakAt = peak && meters[peak.i];
    if (peakAt) {
      this.watch.stir(meterAgitation(peak.fraction, TUNING.meterRed, dt), peakAt.x, peakAt.y, now,
        TUNING.settleOdds);
    }
    meters.forEach((at, i) => {
      if (!at) return;
      this.meterShown[i] = ease(this.meterShown[i] ?? 0, this.meterTargets[i] ?? 0, dt,
        TUNING.meterEaseSecs);
      if (!(this.meterShown[i] > TUNING.meterFloor)) return;
      const emit = meterEmission(this.meterShown[i], TUNING.meterRed);
      const angle = wander(this.meterAngles[i] ?? 2 * Math.PI * this.random(), dt, this.random);
      this.meterAngles[i] = angle;
      list.push({
        kind: 'meter',
        // a meter's emission is a source like the resting one; only its red-zone kick stirs
        rest: !(this.meterShown[i] >= TUNING.meterRed),
        ...uvOf(at, size),
        radius: radiusUv(TUNING.meterRadius, size),
        pull: [Math.cos(angle) * emit.drift, Math.sin(angle) * emit.drift, 0, 0],
        rate: 1 - Math.exp(-TUNING.meterPull * dt),
        dye: dyeOf(this.shade.light, emit.dye * dt),
      });
    });
  }

  /** The place for a note of `cell` px near `place`: `place` itself when the page shows the smoke
   * there (`open`) and it holds enough dye (`density`) to carve a gap in, else the nearest point
   * of a lattice laid over the whole canvas of `size` that does, else `null`: a note is not left
   * under a control, nor in an empty sky. The lattice is anchored at `place`, so it holds
   * `place`; points the same distance away come in row order, so no random draw is made. */
  noteSpot(place, cell, size) {
    const half = cell * TUNING.noteOpenHalf;
    let dense = () => true;
    try {
      const read = this.density?.();
      if (typeof read === 'function') {
        dense = (p) => read(p.x, p.y, cell / 2) >= TUNING.noteDenseMin;
      }
    } catch {
      dense = () => true;
    }
    const shown = (p) => {
      try {
        return !this.open || this.open(p.x, p.y, half) === true;
      } catch {
        return true;
      }
    };
    const reach = cell * 0.71;
    const step = cell * TUNING.noteFindStep;
    const left = Math.max(0, Math.floor((place.x - reach) / step));
    const right = Math.max(0, Math.floor((size[0] - reach - place.x) / step));
    const up = Math.max(0, Math.floor((place.y - reach) / step));
    const down = Math.max(0, Math.floor((size[1] - reach - place.y) / step));
    const lattice = [];
    for (let j = -up; j <= down; j++) {
      for (let i = -left; i <= right; i++) {
        lattice.push({ x: place.x + i * step, y: place.y + j * step, d: Math.hypot(i, j) });
      }
    }
    lattice.sort((a, b) => a.d - b.d);
    return lattice.find((p) => dense(p) && shown(p)) ?? null;
  }

  /** The note to stamp when a stir has settled and the gate allows: at where the stir was when
   * the smoke there is visible and dense enough, else at the nearest place on the canvas that is
   * (`noteSpot`). A settle is rolled once, win or lose. Never while away. */
  noteAt(now, size) {
    if (this.isAway || !this.canNote) return null;
    const at = this.watch.poll(now, this.forcedRoll === null ? this.random : () => this.forcedRoll);
    if (!at) return null;
    if (!this.gate.allows(now)) return null;
    const [cw, ch] = size;
    const { glyph, tilt, size: height } = notePlacement(this.random, Math.min(cw, ch));
    const cell = noteCell(height);
    // half the cell's diagonal, so the turned cell stays on the canvas
    const spot = this.noteSpot(notePosition(at, cell * 0.71, size), cell, size);
    // nowhere to be seen: no note, no gap spent, no cap slot (the roll stays spent)
    if (!spot) return null;
    const { x, y } = spot;
    const tint = lighten(this.shade.light, TUNING.noteLight);
    this.gate.record(now);
    this.noted.stamped++;
    this.noted.last = { x, y, size: height, cell, glyph, tilt, tint, at: now };
    this.lastStir = now;
    return {
      x, y, uv: uvOf({ x, y }, size), angle: (tilt * Math.PI) / 180, aspect: cw / ch,
      size: height, cell, glyph, tilt, tint,
    };
  }
}
