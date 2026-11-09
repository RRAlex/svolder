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
// points sum to, whatever their type: the core's hand (`core.drag.pointDrag`) does the gestures and the writes, given
// the curve's window of the scene as its plot. A press grabs a point and a drag moves its frequency and gain, the
// wheel and two fingers set its Q, and a tap on an empty spot adds a PK there. They are written as live updates, the
// last value at the release; a flip of the side ends a drag or a wheel burst and writes only that last value. The
// lit point has the bell's diamonds, a drag of one setting its Q (the one gesture of the computer's the theme holds
// itself).
//
// While a drag is on, and until its writes are answered, the curve and the bubbles are drawn from
// the tone as it stood at the press with the values the drag asks for, so a reading that comes
// meanwhile never moves a bubble under the hand; it is drawn from the model afterwards.

import {
  bandsOf, bubblesOf, computerTone, createMorph, curveAt, diamondsOf, gMaxOf, heldTone, inBand, labelOf,
  mountainMap, qAtDiamond, tunable, tunableComputer,
} from './tune.js';

/** How near a bubble the pointer lights its bell, CSS px: farther than a press grabs (REACH 12) so
 * a hand drifting toward a diamond keeps the bell, which then stays lit until the curve goes. */
const HOVER_PX = 24;

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
    BAND_PARAM, REACH, bandSteps, bandWriter, computerSteps, createDrag, grab, labelAt, labelWords, startOf, wheelSteps,
  } = core.drag;
  const morph = createMorph();
  const now = () => globalThis.performance.now();

  /** The drags of the device's bands begun and not finished, oldest first (their writes are waited for in turn). */
  const pending = [];
  /** The raw steps each band last held, by a drag that has not settled. */
  const heldRaws = new Map();
  /** The tone and the steps as they stood at the first press of the drags not yet settled. */
  let snap = null;
  /** The drag of a band of the device the pointer has now (the computer's points, and their diamonds, are the core
   * hand's, `ph`). */
  let active = null;
  /** The two fingers of a pinch going, and its drag: `{ ids, entry }`. */
  let fingers = null;
  /** The point or band whose bell is lit while nothing is held: the last bubble hovered or turned. */
  let lit = null;
  let rest = 0;
  let label = null;

  const steps = () => store.device()?.tone_steps;
  /** Whether the computer is the side shown: the points are its EQ's. */
  const onComputer = () => core.computer.side() === 'computer';

  /** The computer's tone as the mountains draw it, worked out again only after the model or a held value changed. */
  let computerCache = null;
  /** A drag of one of the computer's points that began with the EQ off has moved it: the daemon turns the EQ on
   * with the point, so the curve stops being dim from the first move, not at the next reading of the model. */
  let turnedOn = false;
  /** The hand's session the dim was last judged for. */
  let judged = null;
  const forget = () => {
    computerCache = null;
    if (!active && !ph.active) turnedOn = false;
  };
  for (const event of ['model', 'device', 'host', 'fleet']) live.onReading(event, forget);
  const offComputer = core.computer.on(({ type }) => {
    forget();
    // the point or band lit on one side is not the same number on the other
    if (type === 'flip') lit = null;
  });
  signal.addEventListener('abort', offComputer, { once: true });
  const computerNow = () => {
    computerCache ??= computerTone(core.computer.toneOf(store.page()));
    return computerCache;
  };

  /** The map of the scene as it is now (the scene may have been resized since the press), kept until its numbers change. */
  let mapCache = null;
  const mapNow = () => {
    const place = geometry();
    const gMax = gMaxOf(onComputer() ? null : steps());
    const c = mapCache;
    if (c && c.w === place.w && c.h === place.h && c.v === place.v && c.H === place.H && c.gMax === gMax) return c.map;
    mapCache = {
      w: place.w, h: place.h, v: place.v, H: place.H, gMax, map: mountainMap({ ...place, gMax }),
    };
    return mapCache.map;
  };
  const lazyMap = {
    hzAt: (x) => mapNow().hzAt(x),
    dbAt: (y) => mapNow().dbAt(y),
    yOfDb: (db) => mapNow().yOfDb(db),
  };

  /** The curve of `tone` as `x => y` in CSS px, null while it has none; the same function while the tone, the map
   * and the scene's width are. */
  let curveCache = null;
  const curveOf = (tone, map = mapNow()) => {
    if (!tone?.response?.length) return null;
    const w = geometry().w;
    const c = curveCache;
    if (c && c.response === tone.response && c.map === map && c.w === w) return c.curve;
    curveCache = { response: tone.response, map, w, curve: curveAt(tone.response, map, w) };
    return curveCache.curve;
  };

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

  /** A viewer who may not write is turned away by the core with its words (nothing happens, nothing is said, for one who
   * may); only where there is a tone to tune. */
  const turnAway = () => { if (toned()) core.actions.refuse(); };

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

  // ---- the computer's points: the core's hand over the curve's window of the scene ----
  /** The window the curve is drawn in, as the plot the hand reads: `top` and `height` in CSS px of the scene. The
   * hand's own map spans the curve's range above and below the 0 dB line, so this plot, the map's 0 dB line at
   * its middle, makes the hand's frequency and gain the map's. */
  function windowOf() {
    const map = mapNow();
    const perDb = map.yOfDb(0) - map.yOfDb(1);
    const range = core.curve.RANGE_DB;
    return { top: map.yOfDb(0) - range * perDb, height: 2 * range * perDb };
  }

  const ph = core.drag.pointDrag({
    rectOf: () => {
      const rect = scene.getBoundingClientRect();
      const win = windowOf();
      return {
        left: rect.left, top: rect.top + win.top, width: geometry().w, height: win.height,
      };
    },
    points: () => {
      const win = windowOf();
      return bubblesOf(computerNow(), mapNow(), geometry().w).map((b) => ({ band: b.band, x: b.x, y: b.y - win.top }));
    },
    usable: () => onComputer() && allowed(),
    toast: (words) => core.frame.toast(words),
    on: {
      pick: (n) => { lit = n; },
      begin: () => {
        morph.held(true, now());
        mark();
      },
      held: (s) => {
        if (s.wrote && judged !== s) {
          judged = s;
          if (computerNow()?.mode === 'off') turnedOn = true;
        }
        say(s.n, labelWords(s.values(), { qPrefix: 'Q ' }));
      },
      ended: (s) => {
        try {
          if (s.lastPointer !== null) scene.releasePointerCapture(s.lastPointer);
        } catch {
          // already let go
        }
        unsay();
        morph.held(false, now());
        mark();
      },
      finished: () => {
        if (!active && !ph.active) turnedOn = false;
      },
      // a point the table lists elsewhere now: the lit one, a diamond's drag and the label follow it
      // a tap on a full table says how many the EQ holds at most
      added: (answer) => {
        if (answer?.full) core.frame.toast(answer.error);
      },
      moved: ({ from, to }) => {
        if (lit === from) lit = to;
        if (label && heldBand() === to) say(to, label.textContent);
      },
    },
  });

  /** The point or band held now, null when none. */
  const heldBand = () => active?.band ?? (ph.active ? ph.session.n : null);

  // ---- a drag of a band of the device ----
  function begin(band, {
    gesture = 'move', press, pointer, side = null,
  }) {
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
    entry.move = side === null ? (spot) => entry.drag.move(spot) : (spot) => {
      const index = qAtDiamond(tables.q, shape, side, lazyMap.hzAt(spot.x));
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

  // ---- a drag of a diamond of the lit point of the computer: its Q ----
  /** Begin the drag of the diamond `side` of point `n` of the computer's EQ from the press at `pointer`, as a session
   * of the core's hand: its Q goes out as live updates and, once the drag has ended and every one is answered, the
   * last as the final value. A flip of the side ends it. */
  function beginDiamond(n, { pointer, side }) {
    const tables = computerSteps();
    const point = computerNow()?.points[n - 1];
    const s = ph.begin({ n, plot: scene, gesture: 'pinch', press: { distance: 1 }, pointerId: pointer });
    if (!s || s === 'refused' || !point) return;
    // the Q of the step whose half point is under the pointer, as the device's diamond does
    s.diamond = { side, shape: parsedPoint(n), q0: startOf(tables, point).q };
    try {
      scene.setPointerCapture(pointer);
    } catch {
      // without capture the drag still ends on the cancel
    }
  }

  /** The pointer of a diamond's drag is at `spot`: the Q of the step whose half point is under it. */
  function spreadDiamond(s, spot) {
    const tables = computerSteps();
    const { side, shape, q0 } = s.diamond;
    const index = qAtDiamond(tables.q, shape, side, lazyMap.hzAt(spot.x));
    s.drag.spread(tables.q[q0] / tables.q[index]);
  }

  /** Point `n` of the computer's EQ as `{ band, f0, gainDb, q, type }`, null when it is not there. */
  function parsedPoint(n) {
    const p = computerNow()?.points[n - 1];
    return p ? {
      band: n, f0: p.f, gainDb: p.g, q: p.q, type: p.type,
    } : null;
  }

  scene.addEventListener('pointerdown', (event) => {
    if (active || ph.active || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const press = at(event);
    const place = geometry();
    if (!inBand(place, press.y)) return;
    const touch = event.pointerType === 'touch';
    mark();
    if (!allowed()) {
      // the core turns a viewer who may not write away, with its words
      turnAway();
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
    const shapes = bell && qRead ? diamondsOf(bell, mapNow(), place.w, curveOf(toneNow())) : [];
    const away = (p) => Math.hypot(p.x - press.x, p.y - press.y);
    const bubbleBand = grab(bubbles, press, { reach, picked: onComputer() ? core.computer.picked() : null });
    const nearBubble = bubbleBand === null ? Infinity : away(bubbles.find((p) => p.band === bubbleBand));
    const nearDiamond = shapes.filter((p) => away(p) <= reach).sort((a, b) => away(a) - away(b))[0];
    if (nearDiamond && away(nearDiamond) < nearBubble) {
      event.preventDefault();
      if (onComputer()) beginDiamond(bell.band, { pointer: event.pointerId, side: nearDiamond.side });
      else {
        begin(bell.band, {
          gesture: 'pinch', press: { distance: 1 }, pointer: event.pointerId, side: nearDiamond.side,
        });
      }
      return;
    }
    if (onComputer()) {
      // a finger on a point drags it by the touch events (the core's hand); the pointer's press of a finger
      // only makes a tap on empty ground
      if (!(touch && bubbleBand !== null)) {
        ph.down(event, scene, { grabs: !touch });
        if (touch && bubbleBand === null) morph.touch(now());
      }
      mark();
      return;
    }
    const band = bubbleBand;
    if (band === null) {
      if (touch) morph.touch(now());
      mark();
      return;
    }
    const read = store.part('tone').readings?.bands?.[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    event.preventDefault();
    begin(band, { press, pointer: event.pointerId });
  }, { signal });

  scene.addEventListener('pointermove', (event) => {
    const took = ph.move(event);
    if (took || (active && event.pointerId === active.pointer)) {
      const spot = at(event);
      // a held drag keeps the curve with the pointer out of the band; the pointer's place still counts
      if (event.pointerType !== 'touch') morph.over(inBand(geometry(), spot.y), now());
      if (!took) active.move(spot);
      else if (ph.session?.diamond && !ph.session.over) spreadDiamond(ph.session, spot);
      return;
    }
    if (active || ph.active) return;
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
    if (active || ph.active || event.pointerType === 'touch') return;
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
    const s = ph.session;
    if (s && event.pointerId === s.pointerId && event.pointerType === 'touch') morph.touch(now());
    // a diamond's drag ends at the spot the pointer let go
    if (s?.diamond && !s.over && event.pointerId === s.pointerId) spreadDiamond(s, at(event));
    ph.up(event);
    lifted(event);
  }, { signal });
  scene.addEventListener('pointercancel', (event) => {
    ph.cancel(event);
    finish(true)(event);
  }, { signal });
  scene.addEventListener('lostpointercapture', (event) => {
    ph.cancel(event);
    finish(true)(event);
  }, { signal });

  // A pinch: two fingers on the mountains set the Q of the band nearest their middle, apart to
  // widen the bell (lower the Q), together to narrow it. A finger already holding a bubble makes
  // the pinch of that band. Read from the touch events, as a finger's pointer events are the
  // one-finger drag's. The computer's points are the core's hand's: a finger on one drags it, two
  // fingers pinch the point in hand or the nearest to their middle.
  const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const touchOf = (list, id) => [...list].find((t) => t.identifier === id) ?? null;

  /** The fingers on the computer's points. */
  function touchPoints(event) {
    if (event.touches.length > 1) ph.dropTap();
    if (event.touches.length < 1 || event.touches.length > 2 || !computerNow()) return;
    if ([...event.touches].some((t) => !scene.contains(t.target))) return;
    const rect = scene.getBoundingClientRect();
    const place = geometry();
    const middleY = [...event.touches].reduce((sum, t) => sum + t.clientY, 0) / event.touches.length - rect.top;
    if (!inBand(place, middleY)) return;
    const pair = event.touches.length === 2;
    // a finger on a diamond is that drag's own: its touch is not a press on a point (a bubble may stand within reach)
    if (!pair && (active || ph.active)) return;
    // the press of one finger was judged by its pointer: the curve may still be the music, or turned away
    if (!pair && (morph.m < 0.5 || !allowed())) return;
    if (pair) {
      mark();
      if (!allowed()) {
        turnAway();
        return;
      }
      // a second finger on a diamond's drag lets go of it: its point is the one in hand for the pinch
      active?.drag.release();
      if (ph.session?.diamond) ph.session.drag.release();
    }
    ph.touchStart(event, scene, { single: true });
  }

  scene.addEventListener('touchstart', (event) => {
    if (onComputer()) {
      touchPoints(event);
      return;
    }
    if (event.touches.length !== 2 || !store.part('tone')?.readings) return;
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
      turnAway();
      return;
    }
    const old = active;
    const band = old ? old.band
      : grab(bubblesOf(toneNow(), mapNow(), place.w), middle, { reach: REACH.touch, wide: true });
    if (band === null) return;
    if (event.cancelable) event.preventDefault();
    const read = store.part('tone').readings.bands[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    // the finger that grabbed the bubble is let go of as a drag, and the two make a pinch of its band
    old?.drag.release();
    begin(band, { gesture: 'pinch', press: { distance: spacing(a, b) }, pointer: null });
    fingers = { ids: [a.identifier, b.identifier], entry: active };
  }, { signal, passive: false });
  scene.addEventListener('touchmove', (event) => {
    ph.touchMove(event);
    if (!fingers) return;
    const [a, b] = fingers.ids.map((id) => touchOf(event.touches, id));
    if (!a || !b) return;
    if (event.cancelable) event.preventDefault();
    fingers.entry.drag.spread(spacing(a, b));
  }, { signal, passive: false });
  const lift = (event) => {
    ph.touchEnd(event);
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
    if (active || ph.active || !inBand(place, spot.y) || !allowed()) return;
    if (onComputer()) {
      if (!ph.wheel(event, scene, { wide: true })) return;
      lit = ph.burst?.n ?? lit;
      morph.touch(now());
      mark();
      return;
    }
    const band = grab(bubblesOf(toneNow(), mapNow(), place.w), spot, { reach: REACH.wheel, wide: true });
    if (band === null) return;
    event.preventDefault();
    lit = band;
    morph.touch(now());
    mark();
    const got = wheelSteps(rest, event.deltaY, event.deltaMode);
    rest = got.rest;
    if (got.steps === 0) return;
    core.actions.setBand(band, BAND_PARAM.q, { step: got.steps > 0 ? 1 : -1 }, { device: store.shown() });
  }, { signal, passive: false });

  // Another device is shown: the drag ends where it is and writes nothing more to this one, the
  // curve is drawn from the device now shown.
  live.onSwitch(() => {
    const ending = pending.splice(0);
    ph.switched();
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
    ph.lost();
  });

  signal.addEventListener('abort', () => {
    unsay();
    ph.destroy();
  }, { once: true });

  /** The band whose bell is lit now: the one held, else the last hovered. */
  const litBand = () => heldBand() ?? lit;

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
  function bellFrame(tone, map, w, curve) {
    const bell = tone?.mode === 'loudness' ? null : litParsed();
    if (!bell) return null;
    return {
      band: bell.band,
      diamonds: diamondsOf(bell, map, w, curve),
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
      const curve = curveOf(tone, map);
      return {
        m: morph.m,
        ghost: ghosted(tone),
        curve,
        bubbles: bubblesOf(tone, map, place.w),
        held: heldBand(),
        bell: bellFrame(tone, map, place.w, curve),
      };
    },

    /** The y of the curve drawn now at `x` (CSS px of the scene), null while there is none. */
    curveAt: (x) => curveOf(toneNow())?.(x) ?? null,

    /** The tone curve as a function of x (its y, CSS px of the scene), null while the device has none: what the
     * mountains take the shape of when there is no music to follow. */
    curve: () => curveOf(toneNow()),

    /** Whether the mountains are changing now (a drag is held or the morph is moving): drawn at
     * the screen's rate; a curve at rest is drawn at the level's pace. */
    active: () => active !== null || ph.active || morph.moving,

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
        held: heldBand(),
        heldSide: active?.side ?? ph.session?.diamond?.side ?? null,
        lit: tone?.mode === 'loudness' ? null : litBand(),
        diamonds: bellFrame(tone, map, place.w, curveOf(tone, map))?.diamonds ?? [],
        label: label?.textContent ?? '',
        bubbles: bubblesOf(tone, map, place.w),
        curveY: tone?.response?.length
          ? [0.25, 0.5, 0.75].map((f) => curveOf(tone, map)(f * place.w)) : [],
      };
    },
  };
}
