/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The smoke: the page's live background. The effects module asks what it can do, starts it, and
// drives it through its handle. This is the thin runtime: the page's listeners and the animation
// loop, which hand each event and frame to the director (smoke-director.js, the rules, with
// smoke-model.js's pure helpers) and each plan it answers to the engine (smoke-engine.js, the
// GPU's work). The notes' glyphs are in smoke-notes.js.

import { TUNING } from './smoke-model.js';
import { Director } from './smoke-director.js';
import { createEngine, supported } from './smoke-engine.js';

export { supported };

/** The time now in seconds, on the animation frame callbacks' clock. */
const clock = () => performance.now() / 1000;

/** The director a smoke gets unless `opts.director` names another: `random` and `now` as given,
 * with the page's `shade`, the forced `roll`, whether `notes` can be stamped, the page's `open`
 * test for a note's place and the engine's `density` of the dye there. */
export const makeDirector = (given) => new Director(given);

/**
 * Start the smoke on `canvas` with the page's look (`theme`: `shade`, the muted shade the page
 * drew, a fresh one when missing; `random`, the generator of every random draw, `Math.random`
 * when missing); the handle the page drives, or `null` when it cannot run. The handle takes the
 * glow's level (`level`), a volume step (`step`) and the amp's meters (`meters`, their fractions
 * in the anchor's meter order), the away look (`away`), where the dial or meters sit and the
 * content column's box (`anchor`, in viewport pixels), the page's scroll since the last call
 * (`scroll`) and `stop`, which frees everything. The pointer stirs it too, through listeners on
 * the window that `stop` removes. `opts.director` is a factory `({ random, now, shade, roll,
 * notes, open, density, tuning }) -> Director` and `opts.tuning` a `TUNING`-shaped object; both
 * default to the smoke's. `opts.tuning` is the engine's only (the grids' sizes, the passes'
 * constants): the rules read `TUNING` inside the Director, so a theme hands over its own rules
 * through `opts.director` (the factory is given `tuning` to use if it wants it).
 */
export function start(canvas, theme, opts = {}) {
  const tuning = opts.tuning ?? TUNING;
  const engine = createEngine(canvas, tuning);
  if (!engine) return null;
  try {
    return run(canvas, theme, engine, opts.director ?? makeDirector, tuning);
  } catch (error) {
    // a throw after the context exists: the context goes now, not at garbage collection
    engine.stop();
    throw error;
  }
}

/** [`start`]'s work on the engine that `createEngine` made for `canvas`. */
function run(canvas, theme, engine, makeDirector, tuning) {
  const random = theme.random ?? Math.random;
  const director = makeDirector({
    random, now: clock(), shade: theme.shade, roll: theme.roll ?? null, notes: engine.hasNotes,
    open: theme.open ?? null, density: () => engine.density(), tuning,
  });

  let dead = false;
  let lost = false;
  let raf = 0;
  /** Whether the page is hidden: nothing runs then. */
  const hidden = () => document.visibilityState === 'hidden';
  director.visible(!hidden(), clock());

  /** The canvas's size in CSS pixels, from the layer it fills: read when the window or the layer
   * resizes (reading it asks the browser for a layout), not by the frames or the pointer. */
  let sized = [0, 0];
  const measure = () => {
    sized = [canvas.clientWidth, canvas.clientHeight];
  };
  const cssSize = () => sized;

  /** One animation frame: do what the director plans: simulate and draw, wait, or stop asking
   * for frames. */
  const frame = (t) => {
    raf = 0;
    if (dead || lost || hidden()) return;
    const resized = engine.fit(cssSize());
    const [cw, ch] = cssSize();
    // the observer wakes the loop at a real size
    if (!engine.ready || !(cw > 0 && ch > 0)) return;
    const plan = director.frame(t / 1000, [cw, ch], resized);
    if (plan.shift) engine.shift(plan.shift[0], plan.shift[1]);
    if (plan.action === 'stop') {
      if (plan.redraw) engine.draw(plan.draw);
      return;
    }
    if (plan.action === 'run') {
      engine.run(plan);
      engine.draw(plan.draw);
    } else if (plan.redraw) {
      engine.draw(plan.draw);
    }
    raf = requestAnimationFrame(frame);
  };

  /** Ask for a frame when the loop is not running; time spent stopped is not simulated. */
  const wake = () => {
    if (raf || dead || lost || hidden()) return;
    director.wake(clock());
    raf = requestAnimationFrame(frame);
  };

  /** The page became visible or hidden: time spent hidden is never simulated, and the loop
   * resumes when it shows. */
  const onVisible = () => {
    director.visible(!hidden(), clock());
    wake();
  };
  /** The browser took the context: nothing more is drawn or freed. */
  engine.onLost = () => {
    lost = true;
    director.lost(true);
  };
  /** A pointer move (mouse, pen): each of its coalesced events becomes a push along the motion,
   * and a dab of dye when a button is down. A finger's pointer moves are left to `onTouch`: a
   * browser that starts to pan cancels the finger's pointer stream but keeps sending touch
   * moves. Never prevented, never captured; the controls receive what they always did. */
  const onPointer = (event) => {
    if (dead || lost || !engine.ready || event.pointerType === 'touch') return;
    const size = cssSize();
    const moves = event.getCoalescedEvents?.() ?? [];
    let queued = false;
    for (const move of moves.length > 0 ? moves : [event]) {
      queued = director.pointer(event.pointerId,
        { x: move.clientX, y: move.clientY, t: move.timeStamp / 1000 }, event.buttons !== 0, size,
        clock()) || queued;
    }
    if (queued) wake();
  };
  /** A finger lands: its place and time, so its first move is measured from where it began. */
  const onTouchStart = (event) => {
    if (dead || lost || !engine.ready) return;
    for (const touch of event.changedTouches) {
      director.touchStart(`touch${touch.identifier}`,
        { x: touch.clientX, y: touch.clientY, t: event.timeStamp / 1000 });
    }
  };
  /** A touch move: each changed finger, kept apart by its identifier, is a pressed pointer. Never
   * prevented, so the page scrolls as it always did. */
  const onTouch = (event) => {
    if (dead || lost || !engine.ready) return;
    const size = cssSize();
    let queued = false;
    for (const touch of event.changedTouches) {
      queued = director.pointer(`touch${touch.identifier}`,
        { x: touch.clientX, y: touch.clientY, t: event.timeStamp / 1000 }, true, size,
        clock()) || queued;
    }
    if (queued) wake();
  };
  /** The pointer left the page, or a finger lifted: a later scroll has no place to stir at. */
  const onLeave = () => {
    director.leave();
  };
  /** The window or the layer changed size: measure again, and draw. */
  const onResize = () => {
    measure();
    director.resized();
    wake();
  };
  const observer = new ResizeObserver(onResize);
  window.addEventListener('resize', onResize, { passive: true });
  document.addEventListener('visibilitychange', onVisible);
  window.addEventListener('pointermove', onPointer, { passive: true });
  window.addEventListener('touchstart', onTouchStart, { passive: true });
  window.addEventListener('touchmove', onTouch, { passive: true });
  window.addEventListener('touchend', onLeave, { passive: true });
  window.addEventListener('touchcancel', onLeave, { passive: true });
  document.documentElement.addEventListener('pointerleave', onLeave, { passive: true });
  observer.observe(canvas);
  measure();
  engine.fit(cssSize());
  wake();

  return {
    level(x) {
      director.level(x);
      wake();
    },
    step(direction, size, at, big = false) {
      if (director.step(direction, size, at, big, clock())) wake();
    },
    /** A beat of the music, `strength` 0 to 1, at the dial position `at`: a puff from the dial,
     * no stir. */
    beat(strength, at) {
      if (dead || !engine.ready) return;
      if (director.beat?.(strength, clock(), at)) wake();
    },
    /** A control was clicked at `x`, `y` (viewport px): a small puff and a stir whose settle
     * makes a note at the click's odds. */
    click(x, y) {
      if (dead || !engine.ready) return;
      if (director.click(x, y, clock())) wake();
    },
    meters(fractions) {
      if (director.meters(fractions)) wake();
    },
    away(on) {
      if (director.away(on, clock())) wake();
    },
    anchor(a) {
      director.anchor(a);
      wake();
    },
    scroll(dx, dy) {
      director.scroll(dx, dy, clock());
      wake();
    },
    /** Make every settle's roll come out as `v` (`null`: random); read by the browser checks
     * only, no page code calls it. */
    roll(v) {
      director.roll(v);
    },
    /** Pause (`false`, which also empties the grids of the haze already laid down) or resume the
     * haze's feeding; read by the browser checks only, no page code calls it: the checks of the
     * dial's cloud, a stroke and the scroll want the cloud alone. */
    haze(on) {
      if (director.haze(on)) engine.clear();
    },
    /** The frames simulated, the last step's length, the splats applied by source and the notes
     * (`stamped` in all, `live` within their life now, `last` the latest one's place, size (the
     * glyph's height), cell, glyph, tilt and tint, or `null`; `rolls` the settles rolled and
     * `wins` those that rolled well, before the gate); read by the browser checks only, no
     * page code calls it. */
    stats: () => director.stats(clock()),
    /** The velocity and dye grids as floats (`{ w, h, data }` each, RGBA, y up) and the notes'
     * atlas as bytes (`atlas`, the same shape, red the glyph, green the halo, blue 0, `null`
     * without notes); read by the browser checks only, no page code calls it. `null` when there
     * are no grids or the browser cannot read float targets (no error is left behind). */
    readback: () => engine.readback(),
    stop() {
      if (dead) return;
      dead = true;
      cancelAnimationFrame(raf);
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onPointer);
      window.removeEventListener('touchstart', onTouchStart);
      window.removeEventListener('touchmove', onTouch);
      window.removeEventListener('touchend', onLeave);
      window.removeEventListener('touchcancel', onLeave);
      document.documentElement.removeEventListener('pointerleave', onLeave);
      observer.disconnect();
      engine.stop();
    },
  };
}
