/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the tone control on the mountains: the pointer over them turns the ridge into the
// tone curve with a bubble per band, a press near a bubble grabs it and a drag puts its frequency
// and gain where the pointer goes (core/tone-drag.js does the gesture and the writes, this is the
// page around it). Every coordinate here is in CSS px of the scene, whatever the canvas's density.
// Hovering or holding a bubble lights the stretch of the curve its band controls, between the band's
// half-gain points, with a diamond at each end on the curve: a press on a diamond drags the band's Q, the wheel over the mountains steps the Q of the band nearest in x and two
// fingers pinch the band nearest their middle (the core's pinch, as Ember's and Signal's).
// Every write goes through `core.actions`; a viewer who may not write taps the mountains and the
// core says why nothing happens.
//
// With the computer as the side shown (core.computer) the bubbles are its EQ's points, each on the curve the
// points sum to, whatever their type: a press grabs one and a drag moves its frequency and gain, the wheel and
// two fingers set its Q, and a tap on an empty spot adds a PK there. They are written as live updates through
// `core.actions.computer`, the last value at the release; a flip of the side ends a drag or a wheel burst and
// writes only that last value. The lit point has the bell's diamonds, a drag of one setting its Q.
//
// While a drag is on, and until its writes are answered, the curve and the bubbles are drawn from
// the tone as it stood at the press with the values the drag asks for, so a reading that comes
// meanwhile never moves a bubble under the hand; it is drawn from the model afterwards.

import { labelOf as pointLabel } from './points.js';
import {
  bandsOf, bubblesOf, computerTone, createMorph, curveAt, diamondsOf, gMaxOf, heldTone, inBand, labelOf,
  mountainMap, qAtDiamond, tunable, tunableComputer,
} from './tune.js';

/** How near a bubble the pointer lights its bell, CSS px: farther than a press grabs (REACH 12) so
 * a hand drifting toward a diamond keeps the bell, which then stays lit until the curve goes. */
const HOVER_PX = 24;

/** A press that moves less than this (CSS px) and lets go within this long (ms) is a tap. */
const TAP_PX = 6;
const TAP_MS = 600;

/** A burst of wheel notches ends, and its last Q is written as the final value, after this long (ms). */
const WHEEL_END_MS = 300;

/**
 * Wire the mountains of the scene in `root` (its `.nd-scene`) to `core` until `signal` aborts.
 * `geometry()` is the scene's `{ w, h, H }` in CSS px; `live` is `createLive`'s handle (a switch of
 * device ends a drag). Returns `{ step(dt, ms), frame(), active(), read() }`: `step` moves the
 * morph, `frame()` is what the scene draws (`{ m, curve, bubbles, held, bell }`, or null while the
 * scene is the music), `active()` whether the mountains are being tuned now, `read()` what the
 * checks read.
 */
export function wireTune({
  root, core, live, geometry, signal,
}) {
  const scene = root.querySelector('.nd-scene');
  const store = core.model;
  const {
    BAND_PARAM, REACH, bandSteps, bandWriter, computerSteps, createDrag, grab, labelAt, pointWriter, startOf, wheelSteps,
  } = core.drag;
  const morph = createMorph();
  const now = () => globalThis.performance.now();

  /** The drags begun and not finished, oldest first (their writes are waited for in turn). */
  const pending = [];
  /** The raw steps each band last held, by a drag that has not settled. */
  const heldRaws = new Map();
  /** The tone and the steps as they stood at the first press of the drags not yet settled. */
  let snap = null;
  /** The drag the pointer has now. */
  let active = null;
  /** The two fingers of a pinch going, and its drag: `{ ids, entry }`. */
  let fingers = null;
  /** The band whose bell is lit while nothing is held: the last bubble hovered or turned. */
  let lit = null;
  let rest = 0;
  let label = null;
  /** The computer's drags begun and not finished (their last writes are waited for), a press that may turn into
   * a tap `{ id, at, time }`, and the wheel's burst on a point `{ n, idx, timer, untrack }`. */
  const points = new Set();
  let tap = null;
  let wheel = null;

  const steps = () => store.device()?.tone_steps;
  /** Whether the computer is the side shown: the points are its EQ's. */
  const onComputer = () => core.computer.side() === 'computer';

  /** The computer's tone as the mountains draw it, worked out again only after the model or a held value changed. */
  let computerCache = null;
  /** A drag of one of the computer's points that began with the EQ off has moved it: the daemon turns the EQ on
   * with the point, so the curve stops being dim from the first move, not at the next reading of the model. */
  let turnedOn = false;
  const forget = () => {
    computerCache = null;
    if (!active) turnedOn = false;
  };
  for (const event of ['model', 'device', 'host', 'fleet']) live.onReading(event, forget);
  const offComputer = core.computer.on(forget);
  signal.addEventListener('abort', offComputer, { once: true });
  const computerNow = () => {
    computerCache ??= computerTone(core.computer.toneOf(store.page()));
    return computerCache;
  };

  /** The map of the scene as it is now (the scene may have been resized since the press). */
  const mapNow = () => mountainMap({ ...geometry(), gMax: gMaxOf(onComputer() ? null : steps()) });
  const lazyMap = {
    hzAt: (x) => mapNow().hzAt(x),
    dbAt: (y) => mapNow().dbAt(y),
    yOfDb: (db) => mapNow().yOfDb(db),
  };

  /** The curve of `tone` as `x => y` in CSS px, null while it has none. */
  const curveOf = (tone) => (tone?.response?.length ? curveAt(tone.response, mapNow(), geometry().w) : null);

  /** The tone the scene draws: the computer's, or the device's (the model's, or the held one while a drag has not
   * settled). */
  const toneNow = () => {
    if (onComputer()) return computerNow();
    return snap ? heldTone(snap.tone, snap.steps, heldRaws) : store.part('tone');
  };

  /** Whether the curve is drawn dim: the computer's EQ is off, unless a drag of a point has turned it on. */
  const ghosted = (tone) => onComputer() && tone?.mode === 'off' && !turnedOn;

  /** Whether the side shown has a tone with points to turn the mountains into. */
  const toned = () => (onComputer() ? Boolean(computerNow()?.markers.length) : Boolean(store.part('tone')?.markers?.length));

  /** Tell a viewer who may not write, through the core: the mode asked is the one it is in. */
  const turnAway = () => {
    if (onComputer()) core.actions.computer.setMode(computerNow()?.mode === 'eq' ? 'eq' : 'off');
    else core.actions.setMode(store.part('tone')?.mode);
  };

  /** The viewer may not write (a guest, a page whose writes are off): the core says why. */
  const mayNot = () => {
    const viewer = store.page().viewer ?? {};
    return viewer.guest === true || viewer.writes === false;
  };

  /** Whether the mountains may turn into the curve now: tunable, the stream open, no card. */
  function allowed() {
    const page = store.page();
    const tunes = onComputer() ? tunableComputer(page, computerNow()) : tunable(page);
    return tunes && store.state() === 'open' && !(page.device?.card ?? page.card);
  }

  /** The scene's `data-tune`, `on` while the target is the curve. */
  function paint() {
    if (morph.target === 1) scene.dataset.tune = 'on';
    else delete scene.dataset.tune;
  }

  /** Ask whether the page may be tuned (once), then paint. */
  function mark() {
    morph.allow(allowed());
    paint();
  }

  const at = (event) => {
    const rect = scene.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  // ---- the label above the held bubble ----
  function say(band, words) {
    const place = geometry();
    const spot = bubblesOf(toneNow(), mapNow(), place.w).find((b) => b.band === band);
    if (!spot) return;
    if (!label) {
      label = scene.ownerDocument.createElement('span');
      label.className = 'nd-label';
      label.setAttribute('aria-hidden', 'true');
      scene.append(label);
    }
    label.textContent = words;
    const size = { width: label.offsetWidth, height: label.offsetHeight };
    const where = labelAt(spot, { width: place.w, height: place.h }, size);
    label.style.left = `${where.x.toFixed(1)}px`;
    label.style.top = `${where.y.toFixed(1)}px`;
  }

  function unsay() {
    label?.remove();
    label = null;
  }

  // ---- a drag ----
  function begin(band, {
    gesture = 'move', press, pointer, side = null,
  }) {
    if (onComputer()) {
      beginPoint(band, {
        gesture, press, pointer, side,
      });
      return;
    }
    const tone = store.part('tone');
    const device = store.shown();
    const read = tone.readings.bands[band - 1];
    const raws = heldRaws.get(band) ?? { f: read[0], g: read[1], q: read[2] };
    const entry = {
      band, drag: null, pointer, side, untrack: () => {},
    };
    if (pending.length === 0) snap = { tone: structuredClone(tone), steps: structuredClone(steps()) };
    const place = geometry();
    entry.drag = createDrag({
      band,
      gesture,
      start: raws,
      press,
      plot: { width: place.w, height: place.h },
      steps: bandSteps(snap.steps, band),
      map: lazyMap,
      write: bandWriter({
        actions: core.actions,
        band,
        device,
        before: () => pending.slice(0, pending.indexOf(entry)).map((p) => p.drag.settled),
        aborted: () => signal.aborted,
      }),
      on: {
        held: (held) => {
          heldRaws.set(band, held);
          say(band, labelOf(snap.steps, band, held));
        },
        answer: () => {},
        toast: (words) => core.frame.toast(words),
        ended: () => {
          if (active === entry) active = null;
          entry.untrack();
          try {
            if (pointer !== null) scene.releasePointerCapture(pointer);
          } catch {
            // already let go
          }
          unsay();
          morph.held(false, now());
          mark();
          Promise.all(pending.map((p) => p.drag.settled)).then(() => {
            // A switch has forgotten the drag: nothing is left to give back.
            if (entry.switched) return;
            pending.splice(pending.indexOf(entry), 1);
            if (pending.length === 0) {
              heldRaws.clear();
              snap = null;
            }
          });
        },
      },
    });
    // a diamond's drag asks the Q of the step whose half point is under the pointer, through the
    // pinch's spread (q0 / that Q lands exactly on the step); the others move the band's point
    const tables = bandSteps(snap.steps, band);
    const shape = bandsOf(snap.tone, snap.steps, heldRaws)?.[band - 1];
    entry.move = side === null ? (pointer) => entry.drag.move(pointer) : (pointer) => {
      const index = qAtDiamond(tables.q, shape, side, lazyMap.hzAt(pointer.x));
      entry.drag.spread(tables.q[raws.q] / tables.q[index]);
    };
    // a flip of the side lets go of the drag: what it asked of the device stays
    entry.untrack = core.computer.track({ cancel: (why) => entry.drag.cancel(why) });
    pending.push(entry);
    active = entry;
    lit = band;
    morph.held(true, now());
    mark();
    try {
      scene.setPointerCapture(pointer);
    } catch {
      // without capture the drag still ends on the cancel
    }
  }

  // ---- a drag of one of the computer's points ----
  /** Begin the gesture `gesture` of point `n` of the computer's EQ: its values go out as live updates and, once the
   * drag has ended and every one is answered, the last as the final value. A flip of the side ends it. */
  function beginPoint(n, {
    gesture = 'move', press, pointer, side = null,
  }) {
    const point = computerNow()?.points[n - 1];
    if (!point) return;
    const tables = computerSteps();
    const place = geometry();
    const entry = {
      band: n, drag: null, pointer, side, write: pointWriter({ actions: core.actions, n }), wrote: false, untrack: () => {},
    };
    entry.drag = createDrag({
      band: n,
      gesture,
      start: startOf(tables, point),
      press,
      plot: { width: place.w, height: place.h },
      steps: tables,
      map: lazyMap,
      write: entry.write,
      on: {
        held: (raws) => {
          if (!entry.wrote && computerNow()?.mode === 'off') turnedOn = true;
          entry.wrote = true;
          say(n, pointLabel({ f: tables.f[raws.f], g: tables.g[raws.g], q: tables.q[raws.q] }));
        },
        answer: () => {},
        toast: (words) => core.frame.toast(words),
        ended: async () => {
          if (active === entry) active = null;
          try {
            if (pointer !== null) scene.releasePointerCapture(pointer);
          } catch {
            // already let go
          }
          unsay();
          morph.held(false, now());
          mark();
          await entry.drag.settled;
          if (entry.wrote) await entry.write.release();
          if (!active) turnedOn = false;
          entry.untrack();
          points.delete(entry);
        },
      },
    });
    entry.untrack = core.computer.track({ cancel: (why) => entry.drag.cancel(why) });
    // a diamond's drag asks the Q of the step whose half point is under the pointer, as the device's does
    const shape = parsedPoint(n);
    const q0 = startOf(tables, point).q;
    entry.move = side === null ? (spot) => entry.drag.move(spot) : (spot) => {
      const index = qAtDiamond(tables.q, shape, side, lazyMap.hzAt(spot.x));
      entry.drag.spread(tables.q[q0] / tables.q[index]);
    };
    points.add(entry);
    active = entry;
    lit = n;
    morph.held(true, now());
    mark();
    try {
      scene.setPointerCapture(pointer);
    } catch {
      // without capture the drag still ends on the cancel
    }
  }

  /** Point `n` of the computer's EQ as `{ band, f0, gainDb, q, type }`, null when it is not there. */
  function parsedPoint(n) {
    const p = computerNow()?.points[n - 1];
    return p ? {
      band: n, f0: p.f, gainDb: p.g, q: p.q, type: p.type,
    } : null;
  }

  /** The wheel's burst on point `n` ends: the Q it reached is written as the final value. */
  function endWheel() {
    const w = wheel;
    wheel = null;
    if (!w) return;
    clearTimeout(w.timer);
    w.untrack();
    core.actions.computer.setPoint(w.n, {}, { done: true });
  }

  /** `steps` notches of the wheel over point `n`: its Q a step further (up narrows), as live updates. */
  function wheelPoint(n, notches) {
    const point = computerNow()?.points[n - 1];
    if (!point) return;
    const tables = computerSteps();
    if (!wheel || wheel.n !== n) {
      endWheel();
      wheel = {
        n, idx: startOf(tables, point).q, timer: 0, untrack: core.computer.track({ end: endWheel }),
      };
    }
    clearTimeout(wheel.timer);
    wheel.timer = setTimeout(endWheel, WHEEL_END_MS);
    const idx = Math.min(tables.q.length - 1, Math.max(0, wheel.idx + notches));
    if (idx === wheel.idx) return;
    wheel.idx = idx;
    core.actions.computer.setPoint(n, { q: tables.q[idx] }, { done: false });
  }

  /** A press let go of where it began, soon enough, on an empty spot: a PK is added there. */
  function endTap(event) {
    const t = tap;
    tap = null;
    if (!t || event.pointerId !== t.id || now() - t.time > TAP_MS) return;
    const spot = at(event);
    if (Math.hypot(spot.x - t.at.x, spot.y - t.at.y) > TAP_PX || !onComputer() || !allowed()) return;
    const map = mapNow();
    const point = core.computer.addAt(map.hzAt(t.at.x), map.dbAt(t.at.y));
    if (point) core.actions.computer.addPoint(point);
  }

  scene.addEventListener('pointerdown', (event) => {
    tap = null;
    if (active || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const press = at(event);
    const place = geometry();
    if (!inBand(place, press.y)) return;
    const touch = event.pointerType === 'touch';
    mark();
    if (!allowed()) {
      // the core turns a viewer who may not write away, with its words
      if (mayNot() && toned()) turnAway();
      return;
    }
    morph.at(now());
    if (!touch) morph.over(true, now());
    // a touch (or a press as the pointer arrives) on the mountains that are still the music only
    // shows the curve; the bubbles are grabbed from the next touch
    if (morph.m < 0.5) {
      if (touch) morph.touch(now());
      mark();
      return;
    }
    const reach = touch ? REACH.touch : REACH.mouse;
    // the lit bell's diamonds and every bubble are one set: the nearest point within the reach is
    // taken, a bubble before a diamond at the same distance
    const bubbles = bubblesOf(toneNow(), mapNow(), place.w);
    const bell = litParsed();
    const qRead = onComputer() || Number.isInteger(store.part('tone')?.readings?.bands?.[bell?.band - 1]?.[2]);
    const diamonds = bell && qRead ? diamondsOf(bell, mapNow(), place.w, curveOf(toneNow())) : [];
    const away = (p) => Math.hypot(p.x - press.x, p.y - press.y);
    const bubbleBand = grab(bubbles, press, { reach });
    const nearBubble = bubbleBand === null ? Infinity : away(bubbles.find((p) => p.band === bubbleBand));
    const nearDiamond = diamonds.filter((p) => away(p) <= reach).sort((a, b) => away(a) - away(b))[0];
    if (nearDiamond && away(nearDiamond) < nearBubble) {
      event.preventDefault();
      begin(bell.band, {
        gesture: 'pinch', press: { distance: 1 }, pointer: event.pointerId, side: nearDiamond.side,
      });
      return;
    }
    const band = bubbleBand;
    if (band === null) {
      if (touch) morph.touch(now());
      if (onComputer()) tap = { id: event.pointerId, at: press, time: now() };
      mark();
      return;
    }
    if (onComputer()) {
      event.preventDefault();
      beginPoint(band, { press, pointer: event.pointerId });
      return;
    }
    const read = store.part('tone').readings?.bands?.[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    event.preventDefault();
    begin(band, { press, pointer: event.pointerId });
  }, { signal });

  scene.addEventListener('pointermove', (event) => {
    if (tap && event.pointerId === tap.id) {
      const spot = at(event);
      if (Math.hypot(spot.x - tap.at.x, spot.y - tap.at.y) > TAP_PX) tap = null;
    }
    if (active) {
      if (event.pointerId === active.pointer) {
        const spot = at(event);
        // a held drag keeps the curve with the pointer out of the band; the pointer's place still counts
        if (event.pointerType !== 'touch') morph.over(inBand(geometry(), spot.y), now());
        active.move(spot);
      }
      return;
    }
    if (event.pointerType === 'touch') return;
    const spot = at(event);
    const over = inBand(geometry(), spot.y);
    morph.at(now());
    morph.over(over, now());
    if (over) {
      const near = grab(bubblesOf(toneNow(), mapNow(), geometry().w), spot, { reach: HOVER_PX });
      if (near !== null) lit = near;
    }
    mark();
  }, { signal });

  scene.addEventListener('pointerleave', (event) => {
    if (active || event.pointerType === 'touch') return;
    morph.over(false);
    mark();
  }, { signal });

  const finish = (cancelled) => (event) => {
    if (!active || event.pointerId !== active.pointer) return;
    if (!cancelled) active.move(at(event));
    active.drag.release();
    if (event.pointerType === 'touch') morph.touch(now());
  };
  const lifted = finish(false);
  scene.addEventListener('pointerup', (event) => {
    endTap(event);
    lifted(event);
  }, { signal });
  scene.addEventListener('pointercancel', (event) => {
    tap = null;
    finish(true)(event);
  }, { signal });
  scene.addEventListener('lostpointercapture', finish(true), { signal });

  // A pinch: two fingers on the mountains set the Q of the band nearest their middle, apart to
  // widen the bell (lower the Q), together to narrow it. A finger already holding a bubble makes
  // the pinch of that band. Read from the touch events, as a finger's pointer events are the
  // one-finger drag's.
  const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const touchOf = (list, id) => [...list].find((t) => t.identifier === id) ?? null;
  scene.addEventListener('touchstart', (event) => {
    // a second finger makes it no tap (the touch events follow the first finger's pointer events)
    if (event.touches.length > 1) tap = null;
    if (event.touches.length !== 2 || !(onComputer() ? computerNow() : store.part('tone')?.readings)) return;
    const [a, b] = event.touches;
    if (!scene.contains(a.target) || !scene.contains(b.target)) return;
    if (fingers) {
      if (event.cancelable) event.preventDefault();
      return;
    }
    const rect = scene.getBoundingClientRect();
    const middle = {
      x: (a.clientX + b.clientX) / 2 - rect.left, y: (a.clientY + b.clientY) / 2 - rect.top,
    };
    const place = geometry();
    if (!inBand(place, middle.y)) return;
    mark();
    if (!allowed()) {
      if (mayNot() && toned()) turnAway();
      return;
    }
    const old = active;
    const band = old ? old.band
      : grab(bubblesOf(toneNow(), mapNow(), place.w), middle, { reach: REACH.touch, wide: true });
    if (band === null) return;
    if (event.cancelable) event.preventDefault();
    if (onComputer()) {
      old?.drag.release();
      beginPoint(band, { gesture: 'pinch', press: { distance: spacing(a, b) }, pointer: null });
      fingers = { ids: [a.identifier, b.identifier], entry: active };
      return;
    }
    const read = store.part('tone').readings.bands[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    // the finger that grabbed the bubble is let go of as a drag, and the two make a pinch of its band
    old?.drag.release();
    begin(band, { gesture: 'pinch', press: { distance: spacing(a, b) }, pointer: null });
    fingers = { ids: [a.identifier, b.identifier], entry: active };
  }, { signal, passive: false });
  scene.addEventListener('touchmove', (event) => {
    if (!fingers) return;
    const [a, b] = fingers.ids.map((id) => touchOf(event.touches, id));
    if (!a || !b) return;
    if (event.cancelable) event.preventDefault();
    fingers.entry.drag.spread(spacing(a, b));
  }, { signal, passive: false });
  const lift = (event) => {
    if (!fingers || ![...event.changedTouches].some((t) => fingers.ids.includes(t.identifier))) return;
    const { entry } = fingers;
    fingers = null;
    entry.drag.release();
  };
  scene.addEventListener('touchend', lift, { signal });
  scene.addEventListener('touchcancel', lift, { signal });

  // The wheel over the mountains steps the Q of the band nearest in x: up narrows the bell. Each
  // notch keeps the curve for the grace. Elsewhere the wheel is the page's.
  scene.addEventListener('wheel', (event) => {
    if (event.defaultPrevented) return;
    const spot = at(event);
    const place = geometry();
    if (active || !inBand(place, spot.y) || !allowed()) return;
    const band = grab(bubblesOf(toneNow(), mapNow(), place.w), spot, { reach: REACH.wheel, wide: true });
    if (band === null) return;
    event.preventDefault();
    lit = band;
    morph.touch(now());
    mark();
    const got = wheelSteps(rest, event.deltaY, event.deltaMode);
    rest = got.rest;
    if (got.steps === 0) return;
    if (onComputer()) wheelPoint(band, got.steps > 0 ? 1 : -1);
    else core.actions.setBand(band, BAND_PARAM.q, { step: got.steps > 0 ? 1 : -1 }, { device: store.shown() });
  }, { signal, passive: false });

  // Another device is shown: the drag ends where it is and writes nothing more to this one, the
  // curve is drawn from the device now shown.
  live.onSwitch(() => {
    const ending = pending.splice(0);
    for (const entry of points) entry.drag.cancel('switch');
    endWheel();
    tap = null;
    turnedOn = false;
    heldRaws.clear();
    snap = null;
    active = null;
    fingers = null;
    lit = null;
    unsay();
    // only a drag that ends gives the curve its grace
    if (ending.length > 0) morph.held(false, now());
    mark();
    for (const entry of ending) {
      entry.switched = true;
      entry.drag.cancel('switch');
      try {
        scene.releasePointerCapture(entry.pointer);
      } catch {
        // already let go
      }
    }
  });

  // The stream lost: what is left of a drag writes nothing more.
  live.onReading('state', (state) => {
    if (state !== 'lost') return;
    for (const entry of pending) entry.drag.cancel('lost');
    for (const entry of points) entry.drag.cancel('lost');
  });

  signal.addEventListener('abort', () => {
    unsay();
    endWheel();
  }, { once: true });

  /** The band whose bell is lit now: the one held, else the last hovered. */
  const litBand = () => active?.band ?? lit;

  /** The lit band as `{ band, f0, gainDb, q }`, null when none is lit or it is not read in full. */
  function litParsed() {
    if (onComputer()) {
      const lit = litBand();
      return lit === null ? null : parsedPoint(lit);
    }
    const band = litBand();
    const parsed = band === null ? null : bandsOf(toneNow(), snap?.steps ?? steps(), heldRaws)?.[band - 1];
    return parsed ? { band, ...parsed } : null;
  }

  /** What the scene draws of the lit band, null when none is lit (or in Loudness, which has no bands
   * of its own): the diamonds on the curve, which bound the stretch of it the band controls, and the
   * side held. */
  function bellFrame(tone, map, w) {
    const bell = tone?.mode === 'loudness' ? null : litParsed();
    if (!bell) return null;
    return {
      band: bell.band,
      diamonds: diamondsOf(bell, map, w, curveOf(tone)),
      held: active?.side ?? null,
    };
  }

  return {
    step(dt, ms) {
      morph.allow(allowed());
      const m = morph.step(dt, ms);
      // the lit bell goes with the curve
      if (m <= 0 && morph.target === 0) lit = null;
      paint();
      return m;
    },

    frame() {
      if (morph.m <= 0) return null;
      const tone = toneNow();
      const place = geometry();
      const map = mapNow();
      return {
        m: morph.m,
        ghost: ghosted(tone),
        curve: curveOf(tone),
        bubbles: bubblesOf(tone, map, place.w),
        held: active?.band ?? null,
        bell: bellFrame(tone, map, place.w),
      };
    },

    /** The y of the curve drawn now at `x` (CSS px of the scene), null while there is none. */
    curveAt: (x) => curveOf(toneNow())?.(x) ?? null,

    /** The tone curve as a function of x (its y, CSS px of the scene), null while the device has none: what the
     * mountains take the shape of when there is no music to follow. */
    curve: () => curveOf(toneNow()),

    /** Whether the mountains are changing now (a drag is held or the morph is moving): drawn at
     * the screen's rate; a curve at rest is drawn at the level's pace. */
    active: () => active !== null || morph.moving,

    read() {
      const place = geometry();
      const map = mapNow();
      const tone = toneNow();
      return {
        w: place.w,
        h: place.h,
        v: place.v,
        H: place.H,
        gMax: gMaxOf(onComputer() ? null : steps()),
        m: morph.m,
        ghost: ghosted(tone),
        target: morph.target,
        allowed: allowed(),
        held: active?.band ?? null,
        heldSide: active?.side ?? null,
        lit: tone?.mode === 'loudness' ? null : litBand(),
        diamonds: bellFrame(tone, map, place.w)?.diamonds ?? [],
        label: label?.textContent ?? '',
        bubbles: bubblesOf(tone, map, place.w),
        curveY: tone?.response?.length
          ? [0.25, 0.5, 0.75].map((f) => curveAt(tone.response, map, place.w)(f * place.w)) : [],
      };
    },
  };
}
