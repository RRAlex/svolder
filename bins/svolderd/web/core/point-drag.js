/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hand on the curve of the computer's EQ, for every theme: what a press, a drag, a tap, two
// fingers, the wheel and the arrow keys do to the points. A theme gives the geometry (the plot's
// rectangle and where each point's marker is, in CSS px) and hears what happens (`on`); the
// session, the writes and the follow of a re-sort are here, so every theme does them alike.
//
// A press on a point drags its frequency and gain (a right press its Q), two fingers set the Q,
// the wheel steps the Q and the arrow keys step a frequency or a gain; a tap on an empty spot adds
// a point. A drag is one session: its writes go out through `pointWriter` (at most 20 a second, the
// newest waiting), its last value is written once as the final one when every write has been
// answered, and the page's hold on the point goes with that. Flipping to the other side, another
// device or a lost stream ends it (`computer.track`). The wheel and the arrow keys are bursts: each
// notch or key is a live update, and the burst's last value is written once as the final one,
// `WHEEL_END_MS` after the last notch.
//
// The point a session or a burst is on is followed to the place the table lists it in once an edit
// moved it (`computer.on` `'moved'`): the label, the lit marker and a pinch's start read `s.n`,
// which is the point's number as the table has it now. Writes keep the number the session began
// with (the actions name the point by its slot).
//
// The events are the DOM's own and are given to the methods by the theme (it owns the listeners,
// which go with its signal): a pointer event needs `clientX`, `clientY`, `pointerId`,
// `pointerType`, `button`, `buttons`, `isPrimary` and `preventDefault`; a touch event `touches`,
// `changedTouches`, `type`, `cancelable` and `preventDefault`.

import {
  TAP_MS, TAP_PX, WHEEL_END_MS, fullAnswer,
} from './computer.js';
import {
  REACH, computerSteps, createDrag, grab, plotMap, pointWriter, startOf, stepAgainst, wheelSteps,
} from './tone-drag.js';
import { VIEW_HEIGHT, VIEW_WIDTH } from './tone-curve.js';
import { gainText, hzText, qText } from './words.js';

/** The arrow keys on a point: the field each steps, and which way. */
export const POINT_KEYS = {
  ArrowLeft: ['f', -1], ArrowRight: ['f', 1], ArrowDown: ['g', -1], ArrowUp: ['g', 1],
};

/** The fields of a point a step can change. */
const STEP_PARAMS = ['f', 'g', 'q'];

/** The three values of a point as the label beside a dragged point says them; `qPrefix` goes before
 * the Q (`'Q '` where the label has no other word for it). */
export const labelWords = (point, { qPrefix = '' } = {}) => (
  `${hzText(point.f)} · ${gainText(point.g)} · ${qPrefix}${qText(point.q)}`
);

/** The values at the raw steps `raws` (`{ f, g, q }`, indexes into `steps`, the hand's grid by
 * default): what a drag's `held` asks for. */
export const pointValues = (raws, steps = computerSteps()) => ({
  f: steps.f[raws.f], g: steps.g[raws.g], q: steps.q[raws.q],
});

/** The markers of the tone (`toneOf(...).markers`, in the graph's viewBox) as the points a press
 * can grab, in CSS px within a plot of `rect`. */
export const markersIn = (markers, rect) => (markers ?? []).map((m) => ({
  band: m.n,
  x: (m.x / VIEW_WIDTH) * rect.width,
  y: (m.y / VIEW_HEIGHT) * rect.height,
}));

/**
 * The listeners of a touch on an element for as long as it lasts: `watch(el, true)` adds `move`
 * (not passive: it may stop the page's scroll, which a listener left on the graph would stop
 * always) and `end` for `touchend` and for `touchcancel`; `watch(el, false)` takes exactly those
 * off again, and `watch.stop()` takes them off every element still watched. An element it never
 * watched is left alone. The handlers are the ones given, one function each, so removing finds
 * what was added.
 */
export function touchWatch({ move, end }) {
  const watched = new Set();
  const watch = (el, on = true) => {
    if (!el) return;
    if (!on && !watched.has(el)) return;
    const call = on ? 'addEventListener' : 'removeEventListener';
    el[call]('touchmove', move, { passive: false });
    el[call]('touchend', end);
    el[call]('touchcancel', end);
    watched[on ? 'add' : 'delete'](el);
  };
  watch.stop = () => [...watched].forEach((el) => watch(el, false));
  return watch;
}

const clamp = (value, low, high) => Math.min(Math.max(value, low), high);
const within = (event, rect) => ({ x: event.clientX - rect.left, y: event.clientY - rect.top });
const touchOf = (list, id) => [...list].find((touch) => touch.identifier === id) ?? null;
const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);

/**
 * The hand over a plot. `actions` and `computer` are the core's; `toast(words)` says a drag's
 * refusal; `points(plot, rect)` gives the points a press can grab (`[{ band, x, y }]`, CSS px
 * within `rect`, the plot's rectangle: `markersIn` makes them from the tone); `rectOf(plot)` the
 * plot's rectangle (its own `getBoundingClientRect()` by default); `usable()` says whether the
 * computer's controls may be used now (a press that is not usable does nothing).
 *
 * `on` hears, each optional: `pick(n)` a point is put in hand, or `null` for none (a theme lights
 * its row and dot and scrolls the table; the core also says it when a remove took the point out of
 * hand or renumbered it); `begin(s)` a session starts (the label, the hold on the drawn part);
 * `held(s)` the values the drag asks changed, and once at its start; `ended(s)` the pointer let go
 * (the label goes); `finished(s)` every write was answered and the last value written;
 * `moved({ from, to })` a point was listed elsewhere; `burst(b, on)` a wheel or key burst began
 * (`true`) or ended (`false`); `added(answer)` a tap added a point, or was refused (a full table:
 * `{ ok: false, full: true, max }`).
 */
export function pointDrag({
  actions, computer, toast = () => {}, points, rectOf = (plot) => plot.getBoundingClientRect(),
  usable = () => true, on = {}, steps = computerSteps(), clock = {},
}) {
  const time = {
    now: () => Date.now(), later: (fn, ms) => setTimeout(fn, ms), cancel: (id) => clearTimeout(id),
    ...clock,
  };
  const act = actions.computer;
  const tone = () => computer.tone();

  /** The drag under way (or let go of and waiting for its answers), a press that may turn into a
   * tap, and the burst of a wheel or the keys. */
  let session = null;
  let tap = null;
  let burst = null;

  // the touches of a plot, while the theme asks for them (`watchTouch`): the handlers are the
  // hand's own, so one watch is one set of listeners
  const watching = touchWatch({
    move: (event) => touchMove(event), end: (event) => touchEnd(event),
  });

  const busy = () => session !== null && !session.over;
  const pointOf = (n) => tone()?.points[n - 1] ?? null;

  // ---------------------------------------------------------------------------------- the session

  /** Every write of the drag is answered: the last value is written as the final one. */
  async function finish(s) {
    await Promise.all(s.drags.map((drag) => drag.settled));
    if (s.wrote) await s.write.release();
    if (session === s) session = null;
    s.forget();
    on.finished?.(s);
  }

  /** The drag has ended: the pointer and the fingers are let go of, and the plot's touches are no
   * longer watched. */
  function ended(s) {
    s.lastPointer = s.pointerId;
    s.touchIds = null;
    s.pointerId = null;
    s.over = true;
    watching(s.plot, false);
    on.ended?.(s);
    finish(s);
  }

  /** Start the gesture `gesture` of the session's point from `start` (its raw steps), pressed at
   * `press`; a drag it replaces must have been let go of by the caller. */
  function launch(s, gesture, press, start) {
    const rect = rectOf(s.plot);
    const drag = createDrag({
      band: s.n,
      gesture,
      start,
      press,
      plot: { width: rect.width, height: rect.height },
      steps,
      write: s.write,
      on: {
        held: () => {
          s.wrote = true;
          on.held?.(s);
        },
        answer: () => {},
        toast,
        ended: () => {
          if (s.drag === drag) ended(s);
        },
      },
    });
    s.drag = drag;
    s.drags.push(drag);
  }

  /** Begin a drag of point `n` on `plot`, the pointer's (`pointerId`) or the fingers'
   * (`touchIds`). */
  function begin({
    n, plot, gesture, press, pointerId = null, touchIds = null,
  }) {
    const point = pointOf(n);
    if (!point) return null;
    const s = {
      n,
      plot,
      drag: null,
      drags: [],
      pointerId,
      /** The pointer the drag had, kept after it ends so a theme can let go of its capture. */
      lastPointer: pointerId,
      touchIds,
      wrote: false,
      over: false,
      switched: false,
      forget: () => {},
      write: pointWriter({ actions, n, steps }),
      /** The words of the label: the point's values as the table has them, which holds what the
       * drag has asked (`options` as `labelWords` takes them). */
      words: (options) => {
        const now = pointOf(s.n);
        return now ? labelWords(now, options) : '';
      },
      /** The values the drag asks for now, as numbers (`{ f, g, q }`: the grid's values at its raw
       * steps, with no read of the table). */
      values: () => {
        const began = pointOf(s.n);
        return s.drag ? pointValues(s.drag.held, steps) : began && pointValues(startOf(steps, began), steps);
      },
    };
    session = s;
    s.forget = computer.track({ cancel: (why) => s.drag?.cancel(why) });
    computer.pick(n);
    on.pick?.(n);
    on.begin?.(s);
    launch(s, gesture, press, startOf(steps, point));
    on.held?.(s);
    return s;
  }

  /**
   * Begin a session of point `n` on `plot` for a gesture the theme names (`gesture`: `'move'`,
   * `'width'` or `'pinch'`, with the `press` it started from: a spot in CSS px, or for a pinch
   * `{ distance }`), driven by the pointer `pointerId` (or the fingers `touchIds`). The theme feeds
   * a pinch through the session's own `s.drag.spread(distance)`, and lets the pointer's `move`,
   * `up` and `cancel` go to the hand as for a press. Answers the session, `'refused'` when this
   * page may not write (and says why), or `null` when nothing began: a point that is not there, a
   * drag under way, a page not usable now. A burst under way ends first.
   */
  function start(options) {
    if (!options.plot) return null;
    if (busy()) return null;
    if (!usable()) return null;
    const away = refused();
    if (away) return 'refused';
    if (!pointOf(options.n)) return null;
    finishBurst(); // a burst under way ends first
    return begin(options);
  }

  // An edit that ended moved a point in the table's frequency order: the session, the burst and
  // the lit point follow it.
  const stopHearing = computer.on(({ type, from, to, n }) => {
    if (type === 'picked') {
      on.pick?.(n);
      return;
    }
    if (type === 'gone') {
      // the point a burst is on was removed: the burst ends with no final write for it
      if (burst?.n === n) dropBurst();
      return;
    }
    if (type !== 'moved') return;
    if (session?.n === from) session.n = to;
    if (burst?.n === from) burst.n = to;
    on.moved?.({ from, to });
  });

  // ------------------------------------------------------------------------------------ the pointer

  /** Whether a press at `event` on `plot` would grab a point (a mouse's reach). */
  function canGrab(event, plot) {
    const rect = rectOf(plot);
    return usable() && grab(points(plot, rect), within(event, rect), {
      reach: REACH.mouse, picked: computer.picked(),
    }) !== null;
  }

  /** A press that cannot write is turned away with the words that say why; `true` when it was. */
  const refused = () => actions.refuse() !== null;

  /**
   * A pointer press on `plot`. On a point it starts a drag (`wide`: a right press, which sets the Q
   * instead of the frequency and gain; `grabs: false` lets a touch past a point's own area) and
   * answers `'drag'`, or `'refused'` when this page may not write; on empty ground it may become a
   * tap, answers `'tap'`; with a drag under way, or nothing to do, `null`.
   */
  function down(event, plot, { wide = false, grabs = true } = {}) {
    finishBurst();
    if (busy() || !plot || !usable()) return null;
    const rect = rectOf(plot);
    const at = within(event, rect);
    const touch = event.pointerType === 'touch';
    const n = grabs ? grab(points(plot, rect), at, {
      reach: touch ? REACH.touch : REACH.mouse, wide, picked: computer.picked(),
    }) : null;
    if (n === null) {
      tap = null;
      if (wide) return null;
      if (event.isPrimary === false) return null;
      if ((event.button ?? 0) !== 0) return null;
      tap = { plot, at, id: event.pointerId, time: time.now() };
      return 'tap';
    }
    event.preventDefault?.();
    if (refused()) return 'refused';
    try {
      plot.setPointerCapture?.(event.pointerId);
    } catch {
      // Without capture the drag still ends on a move with no button down.
    }
    begin({
      n, plot, gesture: wide ? 'width' : 'move', press: at, pointerId: event.pointerId,
    });
    return 'drag';
  }

  /** The pointer moved: a tap it left is no tap; a drag follows it (`true`); with none, `false`
   * (the theme may show a hover). */
  function move(event) {
    if (tap && event.pointerId === tap.id) {
      const rect = rectOf(tap.plot);
      const now = within(event, rect);
      if (Math.hypot(now.x - tap.at.x, now.y - tap.at.y) > TAP_PX) tap = null;
    }
    const s = session;
    if (!s || s.over || event.pointerId !== s.pointerId) return false;
    if (event.buttons === 0 && event.pointerType === 'mouse') {
      s.drag.release();
      return true;
    }
    s.drag.move(within(event, rectFor(s)));
    return true;
  }

  const rectFor = (s) => rectOf(s.plot);

  /** A press on empty ground is let go of where it began: a point is added there, or the answer
   * says why not (`full: true` with the `max`, or a page that may not write's own words). */
  async function endTap(event) {
    const t = tap;
    tap = null;
    if (!t || event.pointerId !== t.id || time.now() - t.time > TAP_MS) return null;
    const box = rectOf(t.plot);
    const at = within(event, box);
    if (Math.hypot(at.x - t.at.x, at.y - t.at.y) > TAP_PX || !usable()) return null;
    const map = plotMap({ width: box.width, height: box.height });
    const point = computer.addAt(map.hzAt(t.at.x), map.dbAt(t.at.y));
    if (!point) {
      // no EQ to add to says nothing; a full table says so (a page that may not write is turned
      // away with its own words first)
      const now = tone();
      if (!now) return null;
      const refusal = actions.refuse() ?? fullAnswer(now.max);
      on.added?.(refusal);
      return refusal;
    }
    const answer = await act.addPoint(point);
    on.added?.(answer);
    return answer;
  }

  /** The pointer was let go: a drag ends (`true`) or a tap adds its point (resolves the answer). */
  function up(event) {
    const s = session;
    if (s && !s.over && event.pointerId === s.pointerId) {
      s.drag.move(within(event, rectFor(s)));
      s.drag.release();
      return true;
    }
    return endTap(event);
  }

  /** The press that may turn into a tap is no tap (a theme's own gesture took it). */
  const dropTap = () => {
    tap = null;
  };

  /** The pointer was cancelled or lost its capture. */
  function cancel(event) {
    if (event.pointerId === tap?.id) tap = null;
    const s = session;
    if (s && !s.over && event.pointerId === s.pointerId) s.drag.release();
  }

  // ----------------------------------------------------------------------------------------- touch

  /**
   * A finger went down on `plot`. Within reach of a point one finger grabs it (`single: false`
   * leaves that to the pointer's press); a second finger makes a pinch of the drag, or two fingers
   * anywhere pinch the point in hand (else the nearest to their middle). The page does not scroll
   * for a touch that grabbed. A second finger makes a press that waits to be a tap no tap. Answers
   * `'drag'`, `'pinch'`, `'refused'` or `null` (the touch is the page's).
   */
  function touchStart(event, plot, { single = true } = {}) {
    if (!plot || !usable()) return null;
    // a second finger makes the press no tap, whatever it finds to do
    if (event.touches.length >= 2) tap = null;
    const s = session;
    if (s && !s.over) {
      if (s.plot !== plot) return null;
      event.preventDefault?.();
      if (!s.touchIds || s.touchIds.length !== 1 || event.touches.length !== 2) return null;
      const first = touchOf(event.touches, s.touchIds[0]);
      const second = [...event.changedTouches].find((t) => t.identifier !== s.touchIds[0]);
      const point = pointOf(s.n);
      if (!first || !second || !point) return null;
      const old = s.drag;
      s.touchIds = [first.identifier, second.identifier];
      launch(s, 'pinch', { distance: spacing(first, second) }, startOf(steps, point));
      old.release();
      return 'pinch';
    }
    const box = rectOf(plot);
    const marks = points(plot, box);
    if (event.touches.length === 1 && single) {
      const touch = event.touches[0];
      const n = grab(marks, within(touch, box), { reach: REACH.touch, picked: computer.picked() });
      if (n === null) return null;
      event.preventDefault?.();
      if (refused()) return 'refused';
      begin({
        n, plot, gesture: 'move', press: within(touch, box), touchIds: [touch.identifier],
      });
      return 'drag';
    }
    if (event.touches.length === 2) {
      const [a, b] = event.touches;
      const middle = within({
        clientX: (a.clientX + b.clientX) / 2, clientY: (a.clientY + b.clientY) / 2,
      }, box);
      const picked = computer.picked();
      const n = marks.some((p) => p.band === picked) ? picked
        : grab(marks, middle, { reach: REACH.touch, wide: true });
      if (n === null) return null;
      event.preventDefault?.();
      if (refused()) return 'refused';
      tap = null;
      begin({
        n,
        plot,
        gesture: 'pinch',
        press: { distance: spacing(a, b) },
        touchIds: [a.identifier, b.identifier],
      });
      return 'pinch';
    }
    return null;
  }

  /** The fingers of the drag moved. */
  function touchMove(event) {
    const s = session;
    if (!s?.touchIds) return;
    const fingers = s.touchIds.map((id) => touchOf(event.touches, id));
    if (fingers.some((t) => !t)) return;
    if (event.cancelable) event.preventDefault?.();
    if (fingers.length === 1) s.drag.move(within(fingers[0], rectFor(s)));
    else s.drag.spread(spacing(fingers[0], fingers[1]));
  }

  /** A finger of the drag lifted (or the touch was cancelled): the drag ends. */
  function touchEnd(event) {
    const s = session;
    if (!s?.touchIds) return;
    const gone = [...event.changedTouches].filter((t) => s.touchIds.includes(t.identifier));
    if (gone.length === 0) return;
    if (event.type === 'touchend' && s.touchIds.length === 1) s.drag.move(within(gone[0], rectFor(s)));
    s.drag.release();
  }

  // --------------------------------------------------------------------------- the wheel and keys

  /** A burst ends: the value it reached is written as the final one. */
  function finishBurst() {
    const b = burst;
    burst = null;
    if (!b) return;
    time.cancel(b.timer);
    b.forget();
    on.burst?.(b, false);
    act.setPoint(b.writes, {}, { done: true });
  }

  /** A burst ends with nothing more written: its point is gone. */
  function dropBurst() {
    const b = burst;
    burst = null;
    if (!b) return;
    time.cancel(b.timer);
    b.forget();
    on.burst?.(b, false);
  }

  /** A burst of `param` on point `n` is under way (a new one ends the old). */
  function burstOf(n, param) {
    if (burst && burst.n === n && burst.param === param) return burst;
    finishBurst();
    burst = {
      n,
      writes: n,
      param,
      idx: 0,
      rest: 0,
      point: null,
      timer: 0,
      forget: computer.track({ end: finishBurst }),
    };
    on.burst?.(burst, true);
    return burst;
  }

  /** The burst's end is `WHEEL_END_MS` after the last notch or key. */
  function rearm(b) {
    time.cancel(b.timer);
    b.timer = time.later(finishBurst, WHEEL_END_MS);
  }

  /**
   * A wheel notch over `plot`: over a point it steps its Q (up narrows) and the page does not
   * scroll (`true`); elsewhere the page scrolls (`false`); `'refused'` when this page may not
   * write. With `wide` a notch away from every point steps the nearest in x.
   */
  function wheel(event, plot, { wide = false } = {}) {
    if (!plot || event.ctrlKey || !usable()) return false;
    if (busy()) {
      event.preventDefault?.();
      return true;
    }
    const box = rectOf(plot);
    const marks = points(plot, box);
    const n = burst && marks.some((p) => p.band === burst.n) ? burst.n
      : grab(marks, within(event, box), { reach: REACH.wheel, wide, picked: computer.picked() });
    if (n === null) return false;
    event.preventDefault?.();
    if (refused()) return 'refused';
    const point = pointOf(n);
    if (!point) return false;
    const fresh = !burst || burst.n !== n || burst.param !== 'q';
    const b = burstOf(n, 'q');
    if (fresh) b.idx = startOf(steps, point).q;
    const got = wheelSteps(b.rest, event.deltaY, event.deltaMode);
    b.rest = got.rest;
    const idx = clamp(b.idx + got.steps, 0, steps.q.length - 1);
    rearm(b);
    if (idx === b.idx) return true;
    b.idx = idx;
    computer.pick(n);
    on.pick?.(n);
    act.setPoint(b.writes, { q: steps.q[idx] }, { done: false });
    return true;
  }

  /**
   * An arrow key on point `n` (the theme names the point the focused element stands for): left and
   * right step its frequency, down and up its gain, one step of the hand's grid. A key is a live
   * update and the burst of keys ends in one final write. Answers whether the key was the point's
   * (`false` for any other key or one with a modifier), `'refused'` when this page may not write.
   */
  function key(event, n) {
    const mapped = POINT_KEYS[event.key];
    if (!mapped || event.altKey || event.ctrlKey || event.metaKey || !usable()) return false;
    event.preventDefault?.();
    if (busy()) return true;
    if (refused()) return 'refused';
    stepBurst(n, ...mapped, false);
    return true;
  }

  /** One step of `param` of point `n` in its burst (the point is in hand). */
  function stepBurst(n, param, delta, hold) {
    const point = pointOf(n);
    if (!point) return false; // a point that is not there is no step
    computer.pick(n);
    on.pick?.(n);
    const b = burstOf(n, param);
    // a hold ends the burst itself (`finishBurst`): no timer runs between its repeats
    if (hold) time.cancel(b.timer);
    else rearm(b);
    // a step never goes against its key: a point beyond the hand range stays where it is
    // from the burst's own value, which a re-sort by another front end cannot change
    b.point ??= { ...act.current(b.writes) };
    const next = stepAgainst(steps, b.point, param, delta);
    if (next !== null) {
      b.point[param] = next;
      act.setPoint(b.writes, { [param]: next }, { done: false });
    }
    return true;
  }

  /**
   * One step of `param` (`'f'`, `'g'` or `'q'`) of point `n`, `delta` 1 or -1 along the hand's
   * grid, for a theme's own steppers: the keys' burst, so each step is a live update and the
   * burst's last value is written once as the final one, `WHEEL_END_MS` after the last step or at
   * once by `finishBurst()`. A held stepper passes `{ hold: true }` for each repeat: the burst has
   * no timer then (the pause before a hold's first repeat is longer than `WHEEL_END_MS`) and ends
   * at `finishBurst()`, which the hold calls when it ends. A step against the direction (a point
   * beyond the hand range) writes nothing. Answers `true`, `'refused'` when this page may not
   * write, `false` for a step that is none, a point that is not there or a page not usable now.
   */
  function step(n, param, delta, { hold = false } = {}) {
    if (!STEP_PARAMS.includes(param)) return false;
    if (delta !== 1 && delta !== -1) return false;
    if (!usable()) return false;
    const dragging = busy();
    if (dragging) return true;
    const turned = refused();
    if (turned) return 'refused';
    return stepBurst(n, param, delta, hold);
  }

  // ------------------------------------------------------------------------------------- the ends

  return {
    down,
    begin: start,
    dropTap,
    move,
    up,
    cancel,
    canGrab,
    touchStart,
    touchMove,
    touchEnd,
    /** Listen to the touches of `plot` for the length of a touch drag or pinch (`watchTouch(plot,
     * true)` as it begins; a drag that ends takes it off, or `watchTouch(plot, false)`), so no
     * listener stays on the graph to hold up the page's scroll. */
    watchTouch: watching,
    wheel,
    key,
    step,
    /** Escape: the drag ends where it is, what it asked is written. */
    escape() { session?.drag.cancel('esc'); },
    /** The stream was lost: the drag ends and writes nothing more. */
    lost() { session?.drag.cancel('lost'); },
    /** The window lost its focus: the drag ends where it is. */
    blur() { session?.drag.release(); },
    /** Another device is shown: the drag and the burst end. */
    switched() {
      if (session) session.switched = true;
      session?.drag.cancel('switch');
      finishBurst();
    },
    /** Write a burst's last value now. */
    finishBurst,
    /** The session (or `null`), whether or not its pointer is still down. */
    get session() { return session; },
    /** Whether a drag is under way with its pointer or fingers down. */
    get active() { return busy(); },
    get burst() { return burst; },
    /** Stop listening to the core. A drag under way ends as on a lost stream: once its writes are
     * answered, the value it held is written once as the final one and `on.finished` runs, so a
     * theme's `finished` must cope with a page already taken down. A burst under way writes
     * nothing more. */
    destroy() {
      stopHearing();
      watching.stop();
      if (session && !session.over) session.drag.cancel('lost');
      if (burst) {
        time.cancel(burst.timer);
        burst.forget();
      }
      session = null;
      tap = null;
      burst = null;
    },
  };
}
