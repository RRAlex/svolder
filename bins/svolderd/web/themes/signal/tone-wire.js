/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the tone and the choices: a press near a band's point grabs it and a drag puts its
// frequency and gain where the pointer goes (core/tone-drag.js does the gesture and the writes, this
// is the page around it), the wheel over a point steps its Q, the arrow keys on a focused point step
// its frequency (left, right) and its gain (down, up); the input, mode and preset chips and the
// lists they open. Every write goes through `core.actions`; the choices are logged by `act`. A
// viewer who may not write taps the same chips and the core says why nothing happens.
//
// The open area is held from the press until every write of a drag is answered, and while a list is
// open, so what comes from elsewhere meanwhile is drawn from the newest model afterwards, never
// under the hand. While it is held the graph is drawn here, from the values the drag asks for.

import { heldGraph } from './draw/tone.js';
import { closeList, closeOf, openList } from './lists.js';
import { placeOpen } from './place.js';
import { createRelease } from './press.js';
import { WORDS } from './words.js';

const STEPS = { ArrowLeft: ['f', -1], ArrowRight: ['f', 1], ArrowDown: ['g', -1], ArrowUp: ['g', 1] };

/** Wire the tone of the open area and the choices of the shelf in `root` until `signal` aborts;
 * `keep` holds the open area, `shelfKeep` the shelf. */
export function wireTone({
  root, core, live, act, keep, shelfKeep, signal,
}) {
  const box = root.querySelector('#sg-open');
  const shelf = root.querySelector('#sg-shelf');
  const store = core.model;
  const {
    BAND_PARAM, REACH, bandSteps, bandWriter, createDrag, grab, labelAt, wheelSteps,
  } = core.drag;

  /** The drags begun and not finished, oldest first (their writes are waited for in turn). */
  const pending = [];
  /** The raw steps each band last held, by a drag that has not settled. */
  const heldRaws = new Map();
  /** The drag the pointer has now. */
  let active = null;
  /** The two fingers of a pinch going, and its drag: `{ ids, entry }`. */
  let fingers = null;
  let rest = 0;

  const inert = () => Boolean(store.device()?.inert);

  /** A tap that cannot write: the core turns it away and says why (a guest, a page whose writes
   * are off); the mode asked is the one it is already in. */
  const refuse = () => core.actions.setMode(store.part('tone')?.mode);

  // A list closed by a press outside it gives its hold back only after that press is over, so the
  // redraw cannot replace the chip under the click that follows.
  const release = createRelease(root.ownerDocument, shelfKeep, signal);

  /** Open the list `options` under `opener`, holding the shelf while it is open. */
  function list(opener, label, options, choose) {
    // The hold is taken before the list opens (opening one closes another, whose release must not
    // let the area go) and given back if it cannot open. It is given back on the next task after the
    // list closes (or once the press that closed it is over), so a redraw does not replace what is
    // under the click that follows.
    shelfKeep.around(() => openList({
      opener, label, options, choose, signal, onClose: release,
    }));
  }

  // ---- the chips ----
  const controlOf = (key) => (store.device()?.controls ?? []).find((c) => c.key === key);
  const writeControl = (c, option) => act.act(`control:${c.key}`, option.label,
    (a) => a.setControl(c.key, JSON.parse(option.json), { device: store.shown() })).catch(() => {});

  /** A tap on a choice: the shelf's input, or the mode under the curve. */
  function onChoice(event) {
    const el = event.target.closest?.('[data-act]');
    if (!el || !el.closest('#sg-choices, #sg-mode')) return;
    const device = store.device();
    if (!device) return;
    const { act: what } = el.dataset;
    if (what === 'choice') {
      const c = controlOf(el.dataset.key);
      const option = c?.options?.find((o) => o.label === el.dataset.value);
      if (option) writeControl(c, option);
    } else if (what === 'choice-list') {
      const c = controlOf(el.dataset.key);
      if (!c) return;
      if (device.inert) {
        const option = c.options?.find((o) => o.on) ?? c.options?.[0];
        if (option) writeControl(c, option);
        return;
      }
      if (closeOf(el)) return;
      list(el, c.label, (c.options ?? []).map((o) => ({ value: o.label, text: o.label, on: o.on })),
        (label) => {
          const option = c.options.find((o) => o.label === label);
          if (option) writeControl(c, option);
        });
    } else if (what === 'mode') {
      if (device.inert) {
        refuse();
        return;
      }
      core.toneNote.clicked(el.dataset.mode);
      act.act('mode', el.dataset.mode, (a) => a.setMode(el.dataset.mode, { device: store.shown() }))
        .catch(() => {});
    }
  }
  shelf.addEventListener('click', onChoice, { signal });
  box.addEventListener('click', onChoice, { signal });

  // ---- the graph ----

  /** The points on the drawn graph in px, from where they stand. */
  function pointsIn(graph, rect) {
    return [...graph.querySelectorAll('.sg-pt[data-band]')].map((p) => ({
      band: Number(p.dataset.band),
      x: Number(p.dataset.x) * rect.width,
      y: Number(p.dataset.y) * rect.height,
    }));
  }

  /** The drag's words beside its point: `40.0 Hz, +3.10 dB`, above it and inside the plot. */
  function say(graph, rect, steps, band, raws, at) {
    let label = graph.querySelector('.sg-dlab');
    if (!label) {
      label = graph.ownerDocument.createElement('span');
      label.className = 'sg-dlab';
      label.setAttribute('aria-hidden', 'true');
      graph.append(label);
    }
    label.textContent = `${steps.bands[band - 1].f[raws.f]?.[1]}, ${steps.gain[raws.g]?.[1]}`;
    const size = { width: label.offsetWidth, height: label.offsetHeight };
    const spot = labelAt({ x: at.x * rect.width, y: at.y * rect.height },
      { width: rect.width, height: rect.height }, size);
    label.style.left = `${spot.x.toFixed(1)}px`;
    label.style.top = `${spot.y.toFixed(1)}px`;
  }

  /** The graph drawn from the held values of `band`: the curve, the point, its words. */
  function draw(graph, rect, band, raws) {
    const tone = store.part('tone');
    const steps = store.device()?.tone_steps;
    const held = tone && steps ? heldGraph(tone, steps, band, raws) : null;
    if (!held) return;
    graph.querySelector('.sg-curve').setAttribute('d', held.d);
    graph.querySelector('.sg-shade').setAttribute('d', held.shade);
    graph.querySelector('.sg-curve').classList.remove('faint');
    graph.querySelector('.sg-shade').classList.remove('faint');
    const point = graph.querySelector(`.sg-pt[data-band="${band}"]`);
    point.dataset.x = held.point.x.toFixed(5);
    point.dataset.y = held.point.y.toFixed(5);
    point.setAttribute('aria-valuenow', String(raws.g));
    point.setAttribute('aria-valuetext', steps.gain[raws.g][1]);
    point.setAttribute('aria-label', WORDS.point(band, steps.bands[band - 1].f[raws.f][1],
      steps.gain[raws.g][1]));
    placeOpen(graph);
    say(graph, rect, steps, band, raws, held.point);
  }

  /** Begin dragging `band` on `graph`: a `move` from the press at `at`, or a `pinch` (`at` is the
   * fingers' distance, `pointer` null: the fingers are read from the touches). */
  function begin(graph, band, rect, { gesture, at, pointer }) {
    const tone = store.part('tone');
    const device = store.shown();
    const read = tone.readings.bands[band - 1];
    const raws = heldRaws.get(band) ?? { f: read[0], g: read[1], q: read[2] };
    const steps = bandSteps(store.device().tone_steps, band);
    const entry = { band, drag: null, pointer };
    entry.drag = createDrag({
      band,
      gesture,
      start: raws,
      press: gesture === 'pinch' ? { distance: at } : at,
      plot: { width: rect.width, height: rect.height },
      steps,
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
          const now = box.querySelector('#sg-graph');
          if (now) draw(now, now.getBoundingClientRect(), band, held);
        },
        answer: () => {},
        toast: (words) => core.frame.toast(words),
        ended: () => {
          if (active === entry) active = null;
          if (fingers?.entry === entry) fingers = null;
          try {
            if (pointer !== null) graph.releasePointerCapture(pointer);
          } catch {
            // already let go
          }
          Promise.all(pending.map((p) => p.drag.settled)).then(() => {
            // A switch has reset the holds and forgotten the drag: nothing is left to give back.
            if (entry.switched) return;
            pending.splice(pending.indexOf(entry), 1);
            if (pending.length === 0) heldRaws.clear();
            keep.drop();
            box.querySelector(`#sg-pt-${band}`)?.focus({ preventScroll: true });
          });
        },
      },
    });
    keep.take();
    pending.push(entry);
    active = entry;
    if (pointer === null) return;
    try {
      graph.setPointerCapture(pointer);
    } catch {
      // without capture the drag still ends on the cancel
    }
  }

  box.addEventListener('pointerdown', (event) => {
    if (active || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const graph = event.target.closest?.('#sg-graph');
    if (!graph || !store.part('tone')?.readings) return;
    // A touch grabs only from a point's own area (which keeps `touch-action: none`): a touch beside
    // it is the page's scroll, which the browser must not take half-way through a drag.
    if (event.pointerType === 'touch' && !event.target.closest('.sg-pt')) return;
    const rect = graph.getBoundingClientRect();
    const at = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const reach = event.pointerType === 'touch' ? REACH.touch : REACH.mouse;
    const band = grab(pointsIn(graph, rect), at, { reach });
    if (band === null) return;
    event.preventDefault();
    if (inert()) {
      refuse();
      return;
    }
    const read = store.part('tone').readings.bands[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    begin(graph, band, rect, { gesture: 'move', at, pointer: event.pointerId });
  }, { signal });

  box.addEventListener('pointermove', (event) => {
    if (!active || event.pointerId !== active.pointer) return;
    const graph = box.querySelector('#sg-graph');
    if (!graph) return;
    const rect = graph.getBoundingClientRect();
    active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
  }, { signal });
  const finish = (cancelled) => (event) => {
    if (!active || event.pointerId !== active.pointer) return;
    const graph = box.querySelector('#sg-graph');
    if (graph && !cancelled) {
      const rect = graph.getBoundingClientRect();
      active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }
    active.drag.release();
  };
  box.addEventListener('pointerup', finish(false), { signal });
  box.addEventListener('pointercancel', finish(true), { signal });
  box.addEventListener('lostpointercapture', finish(true), { signal });

  // A pinch: two fingers on the graph set the Q of a band, spread apart to widen the bell and
  // together to narrow it (twice as far apart halves the Q). A finger already on a point makes the
  // pinch of its band; two fingers elsewhere on the graph pinch the band nearest their midpoint. The
  // page neither zooms nor scrolls while the two are down. Touches are read from the touch events,
  // as a finger's pointer events are the one-finger drag's.
  const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const touchOf = (list, id) => [...list].find((t) => t.identifier === id) ?? null;
  box.addEventListener('touchstart', (event) => {
    const graph = event.target.closest?.('#sg-graph');
    if (!graph || event.touches.length !== 2 || !store.part('tone')?.readings) return;
    const [a, b] = event.touches;
    if (a.target.closest?.('#sg-graph') !== graph || b.target.closest?.('#sg-graph') !== graph) return;
    if (fingers) {
      if (event.cancelable) event.preventDefault();
      return;
    }
    const rect = graph.getBoundingClientRect();
    const middle = {
      x: (a.clientX + b.clientX) / 2 - rect.left, y: (a.clientY + b.clientY) / 2 - rect.top,
    };
    const old = active;
    const band = old ? old.band : grab(pointsIn(graph, rect), middle, { reach: REACH.touch, wide: true });
    if (band === null) return;
    if (event.cancelable) event.preventDefault();
    if (inert()) {
      refuse();
      return;
    }
    const read = store.part('tone').readings.bands[band - 1];
    if (!heldRaws.has(band) && !(read ?? []).every(Number.isInteger)) return;
    // The finger that grabbed the point is let go of as a drag, and the two make a pinch of its band.
    old?.drag.release();
    begin(graph, band, rect, { gesture: 'pinch', at: spacing(a, b), pointer: null });
    fingers = { ids: [a.identifier, b.identifier], entry: active };
  }, { signal, passive: false });
  box.addEventListener('touchmove', (event) => {
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
  box.addEventListener('touchend', lift, { signal });
  box.addEventListener('touchcancel', lift, { signal });

  // Another device is shown: the drag ends where it is, a list is closed, and every hold is let go
  // (a hold given back later would drop one taken since).
  live.onSwitch(() => {
    const ending = pending.splice(0);
    heldRaws.clear();
    active = null;
    fingers = null;
    closeList();
    keep.reset();
    for (const entry of ending) {
      entry.switched = true;
      entry.drag.cancel('switch');
    }
  });

  // The stream lost: what is left of a drag writes nothing more.
  live.onReading('state', (state) => {
    if (state !== 'lost') return;
    for (const p of pending) p.drag.cancel('lost');
  });

  // The arrow keys on a focused point step its frequency or its gain.
  box.addEventListener('keydown', (event) => {
    const point = event.target.closest?.('.sg-pt[data-band]');
    const move = STEPS[event.key];
    if (!point || !move || event.altKey || event.ctrlKey || event.metaKey) return;
    event.preventDefault();
    if (active) return;
    if (inert()) {
      refuse();
      return;
    }
    core.actions.setBand(Number(point.dataset.band), BAND_PARAM[move[0]], { step: move[1] },
      { device: store.shown() });
  }, { signal });

  // The wheel over a point steps its Q: up narrows the bell. Elsewhere it is the page's.
  box.addEventListener('wheel', (event) => {
    const point = event.target.closest?.('.sg-pt[data-band]');
    if (!point || active || inert()) return;
    event.preventDefault();
    const got = wheelSteps(rest, event.deltaY, event.deltaMode);
    rest = got.rest;
    if (got.steps === 0) return;
    core.actions.setBand(Number(point.dataset.band), BAND_PARAM.q, { step: got.steps > 0 ? 1 : -1 },
      { device: store.shown() });
  }, { signal, passive: false });
}
