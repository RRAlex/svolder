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
// While a drag is on, and until its writes are answered, the curve and the bubbles are drawn from
// the tone as it stood at the press with the values the drag asks for, so a reading that comes
// meanwhile never moves a bubble under the hand; it is drawn from the model afterwards.

import {
  bandsOf, bubblesOf, createMorph, curveAt, diamondsOf, gMaxOf, heldTone, inBand, labelOf,
  mountainMap, qAtDiamond, tunable,
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
    BAND_PARAM, REACH, bandSteps, bandWriter, createDrag, grab, labelAt, wheelSteps,
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

  const steps = () => store.device()?.tone_steps;

  /** The map of the scene as it is now (the scene may have been resized since the press). */
  const mapNow = () => mountainMap({ ...geometry(), gMax: gMaxOf(steps()) });
  const lazyMap = {
    hzAt: (x) => mapNow().hzAt(x),
    dbAt: (y) => mapNow().dbAt(y),
    yOfDb: (db) => mapNow().yOfDb(db),
  };

  /** The curve of `tone` as `x => y` in CSS px, null while it has none. */
  const curveOf = (tone) => (tone?.response?.length ? curveAt(tone.response, mapNow(), geometry().w) : null);

  /** The tone the scene draws: the model's, or the held one while a drag has not settled. */
  const toneNow = () => (snap ? heldTone(snap.tone, snap.steps, heldRaws) : store.part('tone'));

  /** The viewer may not write (a guest, a page whose writes are off): the core says why. */
  const mayNot = () => {
    const viewer = store.page().viewer ?? {};
    return viewer.guest === true || viewer.writes === false;
  };

  /** Whether the mountains may turn into the curve now: tunable, the stream open, no card. */
  function allowed() {
    const page = store.page();
    return tunable(page) && store.state() === 'open' && !(page.device?.card ?? page.card);
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
  function say(band, raws) {
    const place = geometry();
    const spot = bubblesOf(toneNow(), mapNow(), place.w).find((b) => b.band === band);
    if (!spot) return;
    if (!label) {
      label = scene.ownerDocument.createElement('span');
      label.className = 'nd-label';
      label.setAttribute('aria-hidden', 'true');
      scene.append(label);
    }
    label.textContent = labelOf(snap.steps, band, raws);
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
  function begin(band, { gesture = 'move', press, pointer, side = null }) {
    const tone = store.part('tone');
    const device = store.shown();
    const read = tone.readings.bands[band - 1];
    const raws = heldRaws.get(band) ?? { f: read[0], g: read[1], q: read[2] };
    const entry = { band, drag: null, pointer, side };
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
          say(band, held);
        },
        answer: () => {},
        toast: (words) => core.frame.toast(words),
        ended: () => {
          if (active === entry) active = null;
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

  scene.addEventListener('pointerdown', (event) => {
    if (active || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const press = at(event);
    const place = geometry();
    if (!inBand(place, press.y)) return;
    const touch = event.pointerType === 'touch';
    mark();
    if (!allowed()) {
      // the core turns a viewer who may not write away, with its words
      if (mayNot() && store.part('tone')?.markers?.length) {
        core.actions.setMode(store.part('tone')?.mode);
      }
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
    const qRead = Number.isInteger(store.part('tone').readings?.bands?.[bell?.band - 1]?.[2]);
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
      mark();
      return;
    }
    const read = store.part('tone').readings?.bands?.[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    event.preventDefault();
    begin(band, { press, pointer: event.pointerId });
  }, { signal });

  scene.addEventListener('pointermove', (event) => {
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
  scene.addEventListener('pointerup', finish(false), { signal });
  scene.addEventListener('pointercancel', finish(true), { signal });
  scene.addEventListener('lostpointercapture', finish(true), { signal });

  // A pinch: two fingers on the mountains set the Q of the band nearest their middle, apart to
  // widen the bell (lower the Q), together to narrow it. A finger already holding a bubble makes
  // the pinch of that band. Read from the touch events, as a finger's pointer events are the
  // one-finger drag's.
  const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const touchOf = (list, id) => [...list].find((t) => t.identifier === id) ?? null;
  scene.addEventListener('touchstart', (event) => {
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
      if (mayNot() && store.part('tone')?.markers?.length) {
        core.actions.setMode(store.part('tone')?.mode);
      }
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
    core.actions.setBand(band, BAND_PARAM.q, { step: got.steps > 0 ? 1 : -1 }, { device: store.shown() });
  }, { signal, passive: false });

  // Another device is shown: the drag ends where it is and writes nothing more to this one, the
  // curve is drawn from the device now shown.
  live.onSwitch(() => {
    const ending = pending.splice(0);
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
  });

  signal.addEventListener('abort', unsay, { once: true });

  /** The band whose bell is lit now: the one held, else the last hovered. */
  const litBand = () => active?.band ?? lit;

  /** The lit band as `{ band, f0, gainDb, q }`, null when none is lit or it is not read in full. */
  function litParsed() {
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
        H: place.H,
        gMax: gMaxOf(steps()),
        m: morph.m,
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
