/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the EQ graph: a press near a band's point grabs it and a drag puts its frequency
// and gain where the pointer goes (core/tone-drag.js does the gesture and the writes, this is the
// page around it), the wheel over a point steps its Q, and the arrow keys on a focused point step
// its frequency (left, right) and its gain (down, up). Every write goes through `core.actions`.
//
// The tone section is held from the press until every write of the drag is answered, so what comes
// from elsewhere meanwhile is drawn from the newest model after the release, and the section is
// not drawn again under the hand. While it is held the graph is drawn here, from the values the
// drag asks for.

import { heldGraph } from './draw/tone.js';
import { place } from './place.js';
import { WORDS } from './words.js';

const STEPS = { ArrowLeft: ['f', -1], ArrowRight: ['f', 1], ArrowDown: ['g', -1], ArrowUp: ['g', 1] };

/**
 * Wire the graph of the sheet in `root` until `signal` aborts. `keep` is `{ take(), drop() }`, the
 * holds on the tone section (a drag and an open list may hold it together); `refuse()` says why a
 * viewer who may not write cannot.
 */
export function wireGraph({
  root, core, live, keep, refuse, signal,
}) {
  const box = root.querySelector('#p-tone');
  if (!box) return;
  const store = core.model;
  const { BAND_PARAM, REACH, bandSteps, bandWriter, createDrag, grab, labelAt, wheelSteps } = core.drag;

  /** The drags begun and not finished, oldest first (their writes are waited for in turn). */
  const pending = [];
  /** The raw steps each band last held, by a drag that has not settled. */
  const heldRaws = new Map();
  /** The drag the pointer has now. */
  let active = null;
  let rest = 0;

  const inert = () => Boolean(store.device()?.inert);

  /** The points on the drawn graph in px, from where they stand. */
  function pointsIn(graph, rect) {
    return [...graph.querySelectorAll('.pt[data-band]')].map((p) => ({
      band: Number(p.dataset.band),
      x: Number(p.dataset.x) * rect.width,
      y: Number(p.dataset.y) * rect.height,
    }));
  }

  /** The drag's words beside its point: `40.0 Hz, +3.10 dB`, above it and inside the plot. */
  function say(graph, rect, steps, band, raws, at) {
    let label = graph.querySelector('.p-dlab');
    if (!label) {
      label = graph.ownerDocument.createElement('span');
      label.className = 'p-dlab';
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
    const curve = graph.querySelector('.p-curve');
    curve.setAttribute('d', held.d);
    curve.classList.remove('faint');
    const point = graph.querySelector(`.pt[data-band="${band}"]`);
    point.dataset.x = held.point.x.toFixed(5);
    point.dataset.y = held.point.y.toFixed(5);
    point.setAttribute('aria-valuenow', String(raws.g));
    point.setAttribute('aria-valuetext', steps.gain[raws.g][1]);
    point.setAttribute('aria-label', WORDS.point(band, steps.bands[band - 1].f[raws.f][1],
      steps.gain[raws.g][1]));
    place(graph);
    say(graph, rect, steps, band, raws, held.point);
  }

  /** The band's cell in the table shows the value a write answered. */
  function writeCell(band, param, text) {
    const cell = box.querySelector(`#p-bands tr[data-band="${band}"] .p-w[data-param="${param}"]`)
      ?.parentElement?.querySelector('.p-val');
    if (cell) cell.textContent = text;
  }

  /** Begin dragging `band` from the press `event` on `graph`. */
  function begin(graph, band, event, rect, at) {
    const tone = store.part('tone');
    const device = store.shown();
    const read = tone.readings.bands[band - 1];
    const raws = heldRaws.get(band) ?? { f: read[0], g: read[1], q: read[2] };
    const steps = bandSteps(store.device().tone_steps, band);
    const pointer = event.pointerId;
    const entry = { band, drag: null, pointer };
    entry.drag = createDrag({
      band,
      gesture: 'move',
      start: raws,
      press: at,
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
          const now = box.querySelector('#p-graph');
          if (now) draw(now, now.getBoundingClientRect(), band, held);
        },
        answer: (param, text) => writeCell(band, param, text),
        toast: (words) => core.frame.toast(words),
        ended: () => {
          if (active === entry) active = null;
          try {
            graph.releasePointerCapture(pointer);
          } catch {
            // already let go
          }
          Promise.all(pending.map((p) => p.drag.settled)).then(() => {
            pending.splice(pending.indexOf(entry), 1);
            if (pending.length === 0) heldRaws.clear();
            keep.drop();
            box.querySelector(`#p-pt-${band}`)?.focus({ preventScroll: true });
          });
        },
      },
    });
    keep.take();
    pending.push(entry);
    active = entry;
    try {
      graph.setPointerCapture(pointer);
    } catch {
      // without capture the drag still ends on the cancel
    }
  }

  box.addEventListener('pointerdown', (event) => {
    if (active || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const graph = event.target.closest?.('#p-graph');
    if (!graph || !store.part('tone')?.readings) return;
    // A touch grabs only from a point's own area (which keeps `touch-action: none`): a touch beside
    // it is the page's scroll, which the browser must not take half-way through a drag.
    if (event.pointerType === 'touch' && !event.target.closest('.pt')) return;
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
    begin(graph, band, event, rect, at);
  }, { signal });

  box.addEventListener('pointermove', (event) => {
    if (!active || event.pointerId !== active.pointer) return;
    const graph = box.querySelector('#p-graph');
    if (!graph) return;
    const rect = graph.getBoundingClientRect();
    active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
  }, { signal });
  const finish = (cancelled) => (event) => {
    if (!active || event.pointerId !== active.pointer) return;
    const graph = box.querySelector('#p-graph');
    if (graph && !cancelled) {
      const rect = graph.getBoundingClientRect();
      active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }
    active.drag.release();
  };
  box.addEventListener('pointerup', finish(false), { signal });
  box.addEventListener('pointercancel', finish(true), { signal });
  box.addEventListener('lostpointercapture', finish(true), { signal });

  // The stream lost: what is left of a drag writes nothing more.
  live.onReading('state', (state) => {
    if (state !== 'lost') return;
    for (const p of pending) p.drag.cancel('lost');
  });

  // The arrow keys on a focused point step its frequency or its gain.
  box.addEventListener('keydown', (event) => {
    const point = event.target.closest?.('.pt[data-band]');
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
    const point = event.target.closest?.('.pt[data-band]');
    if (!point || active || inert()) return;
    event.preventDefault();
    const got = wheelSteps(rest, event.deltaY, event.deltaMode);
    rest = got.rest;
    if (got.steps === 0) return;
    core.actions.setBand(Number(point.dataset.band), BAND_PARAM.q, { step: got.steps > 0 ? 1 : -1 },
      { device: store.shown() });
  }, { signal, passive: false });
}
