/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the computer's side (the area the computer's hop opens): the volume (the buttons, a
// held one repeating, the track dragged or stepped with the keys, Mute), the mode, the presets (one
// recalled, the curve saved as one, one renamed or deleted), the points' table (a row picked, a
// value typed, a type chosen, a point added or removed) and the curve under the mouse and the
// finger: a press on a dot drags its frequency and gain, a tap on an empty spot adds a point, the
// wheel or two fingers set a Q, the arrow keys step a focused dot. Every write goes through
// `core.actions.computer` (the steps and the pacing are the core's: `computerSteps`,
// `pointWriter`); a viewer who may not write taps the same words and the core says why nothing
// happens.
//
// What is being changed is held by the core (`core.computer`), so a reading from the host does not
// move it. The open area is held (`keep`) from a press to the answer of its last write, and the
// shelf while a field is typed in or a list is open; meanwhile this module moves the curve, the
// dots, the cells and the level itself from what the core holds, and the sections are drawn again
// from the newest model afterwards. Flipping to the DAC ends a drag (the core's `track`).

import { askForm, askQuestion, closeAsk } from './ask.js';
import {
  fractionOf, gainText, hzText, lineHtml, qText,
} from './draw/computer.js';
import { curvePath, shadePath } from './draw/tone.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { closeList, closeOf, openList } from './lists.js';
import { placeOpen } from './place.js';
import { POINTS, SHELF, WORDS } from './words.js';

/** A press that moves less than this (CSS px) and lets go within TAP_MS is a tap. */
const TAP_PX = 6;
const TAP_MS = 600;
/** A wheel burst ends, and its last Q is written as the final value, after this long (ms). */
const WHEEL_END_MS = 300;
/** The hand ranges a typed value is held to. */
const FIELD = { f: [20, 20000], g: [-12, 12], q: [0.1, 10] };
const KEYS = { ArrowLeft: ['f', -1], ArrowRight: ['f', 1], ArrowDown: ['g', -1], ArrowUp: ['g', 1] };
/** The steppers a long press repeats. */
const STEPPERS = '[data-act="cvol-down"], [data-act="cvol-up"]';
const ACTS = new Set(['cvol-down', 'cvol-up', 'cmute', 'cmode', 'cpreset', 'cpreset-save', 'cpreset-rename',
  'cpreset-delete', 'cadd', 'cremove']);

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The number a typed value means: `2.9k`, `2900 Hz`, `+3,2 dB` and `−2` all read; `null` when it
 * is none. */
export function parse(text) {
  const clean = text.trim().replace('−', '-').replace(',', '.').replace(/\s*(hz|db)$/i, '');
  const kilo = /k$/i.test(clean);
  const number = Number(kilo ? clean.slice(0, -1) : clean);
  return clean !== '' && Number.isFinite(number) ? number * (kilo ? 1000 : 1) : null;
}

/** Wire the computer's side of the page in `root` until `signal` aborts; `keep` holds the open area,
 * `shelfKeep` the shelf and `release()` gives one of the shelf's back once the press that closed a
 * list is over. */
export function wireComputer({
  root, core, live, view, keep, shelfKeep, release, signal,
}) {
  const box = root.querySelector('#sg-open');
  const shelf = root.querySelector('#sg-shelf');
  const doc = root.ownerDocument;
  const store = core.model;
  const act = core.actions.computer;
  const {
    REACH, computerSteps, createDrag, grab, labelAt, plotMap, pointWriter, startOf, wheelSteps,
  } = core.drag;
  const STEPS = computerSteps();

  const tone = () => core.computer.toneOf(store.page());
  const volume = () => core.computer.volumeOf(store.page());
  const shown = () => core.computer.side() === 'computer';
  const inert = () => {
    const viewer = store.page().viewer ?? {};
    return viewer.guest === true || viewer.writes === false;
  };
  const quiet = () => view.state === 'lost' || view.state === 'waiting';
  /** A tap that cannot write: the core turns it away and says why (a guest, a page whose writes are
   * off). */
  const refuse = () => act.setMode(tone()?.modes.find((m) => m.on)?.name);

  /** The drag under way (or let go of and waiting for its answers), a press that may turn into a
   * tap, the wheel's burst and the two fingers of a pinch. */
  let session = null;
  let tap = null;
  let wheel = null;
  /** How far the points' table is scrolled (it keeps that across a draw). */
  let scroll = 0;

  // ------------------------------------------------------------------------------ the table

  const scroller = () => shelf.querySelector('.sg-pscroll');

  /** The words under the table: the rows beyond the ones in view. */
  function updateMore() {
    const area = scroller();
    const more = shelf.querySelector('.sg-pmore');
    if (!area || !more) return;
    const rowHeight = area.scrollHeight / Math.max(area.children.length, 1);
    const hidden = Math.ceil((area.scrollHeight - area.scrollTop - area.clientHeight - 1) / rowHeight);
    more.textContent = area.classList.contains('long') && hidden > 0 ? POINTS.more(hidden) : '';
    area.classList.toggle('end', hidden <= 0);
  }

  /** Scroll the table, not the page, so that row `n` is in view. */
  function reveal(n) {
    const area = scroller();
    const row = area?.querySelector(`.sg-prow[data-n="${n}"]`);
    if (!area || !row) return;
    const top = row.offsetTop;
    if (top < area.scrollTop) area.scrollTop = top;
    else if (top + row.offsetHeight > area.scrollTop + area.clientHeight) {
      area.scrollTop = top + row.offsetHeight - area.clientHeight;
    }
    updateMore();
  }

  /** Light row `n` of the table and its dot (and scroll the row into view). */
  function pickPoint(n, { scrolling = true } = {}) {
    view.pick = { n, device: store.shown() };
    for (const row of shelf.querySelectorAll('.sg-prow')) row.classList.toggle('on', Number(row.dataset.n) === n);
    for (const dot of box.querySelectorAll('#sg-cgraph .sg-pt')) dot.classList.toggle('on', Number(dot.dataset.n) === n);
    if (scrolling) reveal(n);
  }

  // The shelf is drawn again: the table keeps its scroll and shows its picked row.
  view.shelfDrawn = () => {
    const area = scroller();
    if (!area) return;
    area.scrollTop = scroll;
    if (view.pick?.n !== null && view.pick?.device === store.shown()) reveal(view.pick.n);
    updateMore();
  };
  shelf.addEventListener('scroll', (event) => {
    if (event.target !== scroller()) return;
    scroll = scroller().scrollTop;
    updateMore();
  }, { capture: true, passive: true, signal });

  // ------------------------------------------------------------------------- a drag in motion

  /** Put the level, the curve, the dots, the cells and the mode in place from what the core holds
   * now (a drag or a press in motion), without drawing the sections again. */
  function patch() {
    if (!shown()) return;
    const v = volume();
    const track = box.querySelector('#sg-cvtrack');
    if (v && track) {
      const at = (v.pct === null ? 0 : clamp(v.pct / 100, 0, 1)).toFixed(5);
      box.querySelector('#sg-cbig .sg-num').textContent = v.text;
      box.querySelector('#sg-cbig small').textContent = v.pct === null ? '' : v.unit;
      box.querySelector('#sg-cbig').classList.toggle('sg-muted', v.muted);
      track.querySelector('.sg-fill').dataset.at = at;
      track.querySelector('.sg-vknob').dataset.at = at;
      if (v.pct !== null) {
        track.setAttribute('aria-valuenow', String(v.pct));
        track.setAttribute('aria-valuetext', `${v.pct} %`);
      }
      const mute = box.querySelector('[data-act="cmute"]');
      mute.textContent = v.muted ? WORDS.unmute : WORDS.mute;
      mute.setAttribute('aria-pressed', String(v.muted));
      placeOpen(track);
    }
    const t = tone();
    const graph = box.querySelector('#sg-cgraph');
    if (!t || !graph) return;
    graph.querySelector('.sg-curve').setAttribute('d', curvePath(t.response));
    graph.querySelector('.sg-shade').setAttribute('d', shadePath(t.response));
    const eq = t.modes.find((m) => m.name === 'eq').on;
    for (const el of graph.querySelectorAll('.sg-curve, .sg-shade')) el.classList.toggle('faint', !eq);
    const dots = t.markers.map(fractionOf);
    for (const [i, p] of t.points.entries()) {
      const dot = graph.querySelector(`.sg-pt[data-n="${p.n}"]`);
      if (!dot) continue;
      dot.dataset.x = dots[i].x.toFixed(5);
      dot.dataset.y = dots[i].y.toFixed(5);
      dot.setAttribute('aria-valuenow', String(p.g));
      dot.setAttribute('aria-valuetext', gainText(p.g));
      dot.setAttribute('aria-label', POINTS.point(p.n, hzText(p.f), gainText(p.g)));
    }
    placeOpen(graph);
    for (const chip of root.querySelectorAll('[data-act="cmode"]')) {
      const on = t.modes.find((m) => m.name === chip.dataset.mode)?.on === true;
      chip.setAttribute('aria-pressed', String(on));
      chip.classList.toggle('on', on);
    }
    for (const p of t.points) {
      const row = shelf.querySelector(`.sg-prow[data-n="${p.n}"]`);
      if (!row) continue;
      const text = { f: hzText(p.f), g: gainText(p.g), q: qText(p.q) };
      for (const [param, words] of Object.entries(text)) {
        const field = row.querySelector(`[data-param="${param}"]`);
        if (field && doc.activeElement !== field && field.value !== words) field.value = words;
      }
      const type = row.querySelector('select');
      if (type && doc.activeElement !== type) type.value = p.type;
    }
    const line = shelf.querySelector('#sg-cline');
    if (line) line.innerHTML = lineHtml(t);
  }

  // An edit that ended moved a point in the table's frequency order: the point in hand, a drag's
  // number and the focus on its dot follow it.
  let refocus = null;
  const stopHearing = core.computer.on(({ type, from, to }) => {
    if (type === 'flip') {
      view.pick = { n: null, device: null };
      scroll = 0;
    } else if (type === 'held') {
      patch();
    } else if (type === 'moved') {
      if (view.pick?.n === from) view.pick = { ...view.pick, n: to };
      if (session?.n === from) session.n = to;
      const found = /^(sg-cpt-|sg-crm-|sg-ct-|sg-cf-)(\d+)(.*)$/.exec(doc.activeElement?.id ?? '');
      if (found && Number(found[2]) === from) refocus = `${found[1]}${to}${found[3]}`;
    } else if (type === 'settled' && shown()) {
      live.draw('open');
      live.draw('shelf');
      if (refocus !== null) doc.getElementById(refocus)?.focus({ preventScroll: true });
      refocus = null;
    }
  });
  signal.addEventListener('abort', () => {
    stopHearing();
    session = null;
    tap = null;
    wheel = null;
    view.shelfDrawn = null;
  }, { once: true });

  // ------------------------------------------------------------------------------ the volume

  /** One step of the level (a key, a button): written, shown in place by the core's hold. */
  const nudge = (delta) => {
    if (!shown()) return;
    if (Math.abs(delta) === 1) {
      act.step(delta).catch(() => {});
      return;
    }
    const v = volume();
    if (v && v.pct !== null) act.setVolume(v.pct + delta).catch(() => {});
  };

  let gripped = false;
  dragAlong(box, {
    signal,
    pick: (event) => event.target.closest?.('#sg-cvtrack'),
    onStart(at) {
      const v = volume();
      gripped = false;
      if (!shown() || !v) return;
      if (inert() || v.inert) {
        // the guest is turned away with a word; a writer whose volume is unread writes nothing
        if (inert()) act.setVolume(Math.round(at * 100));
        return;
      }
      gripped = true;
      keep.take();
      act.setVolume(Math.round(at * 100));
    },
    onMove(at) {
      if (gripped) act.setVolume(Math.round(at * 100));
    },
    onEnd() {
      if (!gripped) return;
      gripped = false;
      keep.drop();
      box.querySelector('#sg-cvtrack')?.focus({ preventScroll: true });
    },
  });

  sliderKeys(box, {
    signal,
    match: '#sg-cvtrack',
    step: (delta) => nudge(delta),
    home: () => {
      if (shown() && volume()?.pct !== null) act.setVolume(0);
    },
  });

  // A held `−` or `+` is the core's hold: one step at the press, then steps at a steady pace. The
  // open area is held while a press lasts, so a reading does not replace the button under the finger.
  let hold = null;
  let give = true;
  live.onSwitch(() => {
    give = false;
    hold?.end();
    give = true;
  });
  box.addEventListener('pointerdown', (event) => {
    const button = event.target.closest?.(STEPPERS);
    if (!button || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    hold = core.hold.repeat(event, {
      root: box,
      el: button,
      step: (el) => nudge(el.dataset.act === 'cvol-up' ? 1 : -1),
      live: () => !inert() && !quiet(),
      leave: true,
      cap: Infinity,
      onStart: () => keep.take(),
      onEnd: () => {
        hold = null;
        if (give) keep.drop();
      },
      signal,
    });
  }, { signal });
  // A long press on a phone must not open the context menu over a held button.
  box.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPPERS)) event.preventDefault();
  }, { signal });

  // ---------------------------------------------------------------------------- the dragging

  /** The points a press can grab, in CSS px within a graph of `rect`. */
  const pointsIn = (graph, rect) => [...graph.querySelectorAll('.sg-pt[data-n]')].map((p) => ({
    band: Number(p.dataset.n),
    x: Number(p.dataset.x) * rect.width,
    y: Number(p.dataset.y) * rect.height,
  }));
  const within = (event, rect) => ({ x: event.clientX - rect.left, y: event.clientY - rect.top });
  const graphOf = (target) => target.closest?.('#sg-cgraph') ?? null;
  const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const touchOf = (list, id) => [...list].find((touch) => touch.identifier === id) ?? null;

  /** The words beside the dragged point: its three values, above it and inside the plot. */
  function say(s) {
    const t = tone();
    const point = t?.points[s.n - 1];
    const dot = s.graph.querySelector(`.sg-pt[data-n="${s.n}"]`);
    if (!point || !dot) return;
    const rect = s.graph.getBoundingClientRect();
    s.label.textContent = `${hzText(point.f)} · ${gainText(point.g)} · ${qText(point.q)}`;
    const at = { x: Number(dot.dataset.x) * rect.width, y: Number(dot.dataset.y) * rect.height };
    const size = { width: s.label.offsetWidth, height: s.label.offsetHeight };
    const spot = labelAt(at, { width: rect.width, height: rect.height }, size);
    s.label.style.left = `${spot.x.toFixed(1)}px`;
    s.label.style.top = `${spot.y.toFixed(1)}px`;
  }

  /** Every write of the drag is answered: the last value is sent as final, the hold goes and the
   * sections are drawn again from the model. */
  async function finish(s) {
    await Promise.all(s.drags.map((drag) => drag.settled));
    if (s.wrote) await s.write.release();
    if (session === s) session = null;
    s.forget();
    if (!s.switched) keep.drop();
    doc.querySelector(`#sg-cpt-${s.n}`)?.focus({ preventScroll: true });
  }

  /** The drag has ended: the pointer is let go of and the words beside it go. */
  function ended(s) {
    watchTouch(s, false);
    s.touchIds = null;
    s.pointerId = null;
    s.over = true;
    s.label.remove();
    finish(s);
  }

  /** Start the gesture `gesture` of the session's point from `start` (its raw steps), pressed at
   * `press`; a drag it replaces must have been let go of by the caller. */
  function launch(s, gesture, press, start) {
    const rect = s.graph.getBoundingClientRect();
    const drag = createDrag({
      band: s.n,
      gesture,
      start,
      press,
      plot: { width: rect.width, height: rect.height },
      steps: STEPS,
      write: s.write,
      on: {
        held: () => {
          s.wrote = true;
          say(s);
        },
        answer: () => {},
        toast: (words) => core.frame.toast(words),
        ended: () => {
          if (s.drag === drag) ended(s);
        },
      },
    });
    s.drag = drag;
    s.drags.push(drag);
  }

  /** Begin a drag of point `n` on `graph`, the pointer's (`pointerId`) or the fingers' (`touchIds`). */
  function begin({
    n, graph, gesture, press, pointerId = null, touchIds = null,
  }) {
    const point = tone()?.points[n - 1];
    if (!point) return null;
    const label = doc.createElement('span');
    label.className = 'sg-dlab';
    label.setAttribute('aria-hidden', 'true');
    graph.append(label);
    const s = {
      n, graph, label, drag: null, drags: [], pointerId, touchIds, wrote: false, over: false,
      switched: false, forget: () => {},
      write: pointWriter({ actions: core.actions, n, steps: STEPS }),
    };
    session = s;
    keep.take();
    s.forget = core.computer.track({ cancel: (why) => s.drag?.cancel(why) });
    pickPoint(n);
    launch(s, gesture, press, startOf(STEPS, point));
    say(s);
    if (pointerId !== null) {
      try {
        graph.setPointerCapture(pointerId);
      } catch {
        // without capture the drag still ends on the cancel
      }
    }
    if (touchIds) watchTouch(s, true);
    return s;
  }

  box.addEventListener('pointerdown', (event) => {
    if (wheel) finishWheel();
    const graph = graphOf(event.target);
    if (!graph || !shown() || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    if (session && !session.over) return;
    const rect = graph.getBoundingClientRect();
    const at = within(event, rect);
    const touch = event.pointerType === 'touch';
    // A touch grabs only from a dot's own area (which keeps `touch-action: none`): a touch beside
    // it is the page's scroll, or a tap.
    const n = touch && !event.target.closest('.sg-pt') ? null
      : grab(pointsIn(graph, rect), at, { reach: touch ? REACH.touch : REACH.mouse, picked: view.pick?.n ?? null });
    if (n === null) {
      tap = { graph, at, id: event.pointerId, time: Date.now() };
      return;
    }
    event.preventDefault();
    if (inert()) {
      refuse();
      return;
    }
    begin({
      n, graph, gesture: 'move', press: at, pointerId: event.pointerId,
    });
  }, { signal });

  box.addEventListener('pointermove', (event) => {
    if (tap && event.pointerId === tap.id) {
      const rect = tap.graph.getBoundingClientRect();
      const now = within(event, rect);
      if (Math.hypot(now.x - tap.at.x, now.y - tap.at.y) > TAP_PX) tap = null;
    }
    const s = session;
    if (!s || s.over || event.pointerId !== s.pointerId) return;
    s.drag.move(within(event, s.graph.getBoundingClientRect()));
  }, { signal });

  /** A press on an empty spot is let go of where it began: a point is added there. */
  async function endTap(event) {
    const t = tap;
    tap = null;
    if (!t || event.pointerId !== t.id || Date.now() - t.time > TAP_MS) return;
    const rect = t.graph.getBoundingClientRect();
    const at = within(event, rect);
    if (Math.hypot(at.x - t.at.x, at.y - t.at.y) > TAP_PX || !shown()) return;
    const map = plotMap({ width: rect.width, height: rect.height });
    const point = core.computer.addAt(map.hzAt(t.at.x), map.dbAt(t.at.y));
    if (!point) return;
    const done = await act.addPoint(point);
    if (done.ok) pickAfterAdd(done.n);
  }

  box.addEventListener('pointerup', (event) => {
    const s = session;
    if (s && !s.over && event.pointerId === s.pointerId) {
      s.drag.move(within(event, s.graph.getBoundingClientRect()));
      s.drag.release();
      return;
    }
    endTap(event);
  }, { signal });
  for (const name of ['pointercancel', 'lostpointercapture']) {
    box.addEventListener(name, (event) => {
      if (event.pointerId === tap?.id) tap = null;
      const s = session;
      if (s && !s.over && event.pointerId === s.pointerId) s.drag.release();
    }, { signal });
  }

  // --------------------------------------------------------------------------------- touch

  function watchTouch(s, on) {
    const call = on ? 'addEventListener' : 'removeEventListener';
    s.graph[call]('touchmove', onTouchMove, { passive: false });
    s.graph[call]('touchend', onTouchEnd);
    s.graph[call]('touchcancel', onTouchEnd);
  }

  /** Two fingers on the graph pinch a point's Q: spread apart to widen the bell, together to narrow
   * it. A finger already on a point makes the pinch of its point; two fingers elsewhere pinch the
   * point nearest their midpoint. The page neither zooms nor scrolls while the two are down. */
  box.addEventListener('touchstart', (event) => {
    const graph = graphOf(event.target);
    if (!graph || !shown() || event.touches.length !== 2) return;
    const [a, b] = event.touches;
    if (graphOf(a.target) !== graph || graphOf(b.target) !== graph) return;
    const s = session;
    if (s && !s.over) {
      if (event.cancelable) event.preventDefault();
      if (s.touchIds?.length !== 1 || s.graph !== graph) return;
      const first = touchOf(event.touches, s.touchIds[0]);
      const second = [...event.touches].find((t) => t.identifier !== s.touchIds[0]);
      if (!first || !second) return;
      const old = s.drag;
      s.touchIds = [first.identifier, second.identifier];
      launch(s, 'pinch', { distance: spacing(first, second) }, startOf(STEPS, tone().points[s.n - 1]));
      old.release();
      return;
    }
    const rect = graph.getBoundingClientRect();
    const middle = within({
      clientX: (a.clientX + b.clientX) / 2, clientY: (a.clientY + b.clientY) / 2,
    }, rect);
    const points = pointsIn(graph, rect);
    const n = points.some((p) => p.band === view.pick?.n) ? view.pick.n
      : grab(points, middle, { reach: REACH.touch, wide: true });
    if (n === null) return;
    if (event.cancelable) event.preventDefault();
    if (inert()) {
      refuse();
      return;
    }
    tap = null;
    begin({
      n, graph, gesture: 'pinch', press: { distance: spacing(a, b) }, touchIds: [a.identifier, b.identifier],
    });
  }, { signal, passive: false });

  function onTouchMove(event) {
    const s = session;
    if (!s?.touchIds || s.touchIds.length !== 2) return;
    const [a, b] = s.touchIds.map((id) => touchOf(event.touches, id));
    if (!a || !b) return;
    if (event.cancelable) event.preventDefault();
    s.drag.spread(spacing(a, b));
  }

  function onTouchEnd(event) {
    const s = session;
    if (!s?.touchIds) return;
    if (![...event.changedTouches].some((t) => s.touchIds.includes(t.identifier))) return;
    s.drag.release();
  }

  // ------------------------------------------------------------------------ wheel and keys

  /** A burst of wheel notches ends: the Q it reached is written as the final value. */
  function finishWheel() {
    const w = wheel;
    wheel = null;
    if (!w) return;
    clearTimeout(w.timer);
    w.forget();
    act.setPoint(w.n, {}, { done: true });
  }

  /** A notch over a dot steps its Q (up narrows) and the page does not scroll; elsewhere the page
   * scrolls. */
  box.addEventListener('wheel', (event) => {
    const graph = graphOf(event.target);
    if (!graph || event.ctrlKey || !shown()) return;
    if (session && !session.over) {
      event.preventDefault();
      return;
    }
    const rect = graph.getBoundingClientRect();
    const points = pointsIn(graph, rect);
    const n = wheel && points.some((p) => p.band === wheel.n) ? wheel.n
      : grab(points, within(event, rect), { reach: REACH.wheel, picked: view.pick?.n ?? null });
    if (n === null) return;
    event.preventDefault();
    if (inert()) {
      refuse();
      return;
    }
    const point = tone().points[n - 1];
    if (!wheel || wheel.n !== n) {
      finishWheel();
      keep.take();
      const untrack = core.computer.track({ end: finishWheel });
      wheel = {
        n, idx: startOf(STEPS, point).q, rest: 0, timer: 0,
        forget: () => {
          untrack();
          keep.drop();
        },
      };
    }
    const got = wheelSteps(wheel.rest, event.deltaY, event.deltaMode);
    wheel.rest = got.rest;
    const idx = clamp(wheel.idx + got.steps, 0, STEPS.q.length - 1);
    clearTimeout(wheel.timer);
    wheel.timer = setTimeout(finishWheel, WHEEL_END_MS);
    if (idx === wheel.idx) return;
    wheel.idx = idx;
    pickPoint(n);
    act.setPoint(n, { q: STEPS.q[idx] }, { done: false });
  }, { signal, passive: false });

  /** The arrow keys on a focused dot step its frequency (left, right) and its gain (down, up) one
   * step of the grid. Escape ends a drag where it is. */
  box.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      session?.drag.cancel('esc');
      return;
    }
    const dot = event.target.closest?.('.sg-pt[data-n]');
    const move = KEYS[event.key];
    if (!dot || !move || event.altKey || event.ctrlKey || event.metaKey || !shown()) return;
    event.preventDefault();
    if (session && !session.over) return;
    if (inert()) {
      refuse();
      return;
    }
    const n = Number(dot.dataset.n);
    const point = tone()?.points[n - 1];
    if (!point) return;
    const [param, delta] = move;
    const at = clamp(startOf(STEPS, point)[param] + delta, 0, STEPS[param].length - 1);
    pickPoint(n);
    // a step never goes against its key: a point beyond the hand range stays where it is
    if (Math.sign(STEPS[param][at] - point[param]) !== delta) return;
    act.setPoint(n, { [param]: STEPS[param][at] }, { done: true });
  }, { signal });

  // Another device is shown, the stream is lost or the window loses its focus: a drag ends where
  // it is (a lost stream writes nothing more).
  live.onSwitch(() => {
    if (session) session.switched = true;
    session?.drag.cancel('switch');
    finishWheel();
    closeAsk();
    closeList();
    view.pick = { n: null, device: null };
  });
  live.onReading('state', (state) => {
    if (state === 'lost') session?.drag.cancel('lost');
  });
  doc.defaultView?.addEventListener('blur', () => session?.drag.release(), { signal });

  // --------------------------------------------------------------------------- the points

  /** After a point was added: its row is lit and in view. */
  function pickAfterAdd(n) {
    if (n > 0) pickPoint(n);
  }

  /** Turn a field of the table into what was typed: Enter writes the value (within the hand
   * ranges), Escape or leaving puts the old one back. */
  shelf.addEventListener('focusin', (event) => {
    if (!event.target.closest?.('.sg-pval, .sg-ptype') || shelf.dataset.typing) return;
    shelf.dataset.typing = '1';
    shelfKeep.take();
    const field = event.target;
    if (field.matches('.sg-pval') && !field.readOnly) field.select?.();
    const n = Number(field.dataset.n);
    if (n) pickPoint(n, { scrolling: false });
  }, { signal });
  shelf.addEventListener('focusout', (event) => {
    const field = event.target.closest?.('.sg-pval, .sg-ptype');
    if (!field || !shelf.dataset.typing) return;
    delete shelf.dataset.typing;
    // What was typed and not sent is put back; a value sent shows once the model has it.
    if (field.matches('.sg-pval')) field.value = field.defaultValue;
    shelfKeep.dropLater();
  }, { signal });
  shelf.addEventListener('keydown', async (event) => {
    const field = event.target.closest?.('.sg-pval');
    if (!field) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      field.value = field.defaultValue;
      field.blur();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const value = parse(field.value);
      const { n, param } = field.dataset;
      field.blur();
      if (inert()) {
        refuse();
        return;
      }
      if (value === null) {
        core.frame.toast('Not a number.');
        return;
      }
      if (value === parse(field.defaultValue)) return;
      const [lo, hi] = FIELD[param];
      await act.setPoint(Number(n), { [param]: clamp(value, lo, hi) }, { done: true });
    }
  }, { signal });

  // a type chosen in a row
  shelf.addEventListener('change', (event) => {
    const select = event.target.closest?.('.sg-ptype');
    if (!select) return;
    const n = Number(select.dataset.n);
    const { value } = select;
    if (inert()) {
      select.value = select.querySelector('[selected]')?.value ?? select.value;
      refuse();
      return;
    }
    select.blur();
    act.setPoint(n, { type: value }, { done: true });
  }, { signal });

  // --------------------------------------------------------------------- the presses on words

  /** The place a form or a question opens in: the ask of the section the word is in. */
  const within2 = (el) => el.closest('.sg-box2')?.querySelector('.sg-ask') ?? shelf.querySelector('.sg-ask');

  /** Hold the shelf while an ask is open; the ask's `onClose` gives the hold back. */
  const ask = (open) => shelfKeep.around(() => open(() => shelfKeep.dropLater()));

  /** Open `list` under `opener` holding the shelf; `then` hears the value chosen. */
  function pick(opener, label, options, then) {
    if (closeOf(opener)) return;
    shelfKeep.around(() => openList({
      opener, label, options, choose: then, signal, onClose: release,
    }));
  }

  const device = () => ({ device: store.shown() });
  /** What the computer offers is the shown device's list: read again after the list was changed. */
  const resynced = (answer) => {
    if (answer?.ok) core.computer.resync();
    return answer;
  };

  /** Rename or delete: the preset the curve is on if it is the person's own, else a list of the
   * person's own to choose from. */
  function ownPreset(el, label, then) {
    const t = tone();
    const own = (t?.presets ?? []).filter((p) => !p.built_in);
    const current = own.find((p) => p.name === t?.preset);
    if (current) {
      then(current.name);
      return;
    }
    pick(el, label, own.map((p) => ({ value: p.name, text: p.name, on: false })), then);
  }

  function press(el) {
    const what = el.dataset.act;
    const opened = device();
    if (what === 'cmode') {
      if (el.getAttribute('aria-pressed') !== 'true') act.setMode(el.dataset.mode).catch(() => {});
    } else if (what === 'cpreset') {
      act.recallPreset(el.dataset.name).catch(() => {});
    } else if (what === 'cpreset-save') {
      ask((onClose) => askForm({
        within: within2(el), kind: 'save', value: '', signal, opener: el, onClose,
        submit: (text) => act.savePreset(text),
      }));
    } else if (what === 'cpreset-rename') {
      ownPreset(el, SHELF.whichRename, (name) => ask((onClose) => askForm({
        within: within2(el), kind: 'rename', value: name, signal, opener: el, onClose,
        submit: (text) => core.actions.renamePreset(name, text, opened).then(resynced),
      })));
    } else if (what === 'cpreset-delete') {
      ownPreset(el, SHELF.whichDelete, (name) => ask((onClose) => askQuestion({
        within: within2(el), words: SHELF.askDelete(name), yes: 'delete', no: 'keep',
        signal, opener: el, onClose,
        go: () => core.actions.deletePreset(name, opened).then(resynced),
      })));
    } else if (what === 'cadd') {
      const t = tone();
      const point = t && core.computer.addAt(core.computer.nextFree(t.points), 0);
      if (point) act.addPoint(point).then((done) => done.ok && pickAfterAdd(done.n)).catch(() => {});
    } else if (what === 'cremove') {
      const n = Number(el.dataset.n);
      if (view.pick?.n === n) view.pick = { n: null, device: null };
      act.removePoint(n).then(() => {
        const t = tone();
        const next = t && doc.querySelector(`#sg-crm-${Math.min(n, t.points.length)}`);
        (next ?? doc.querySelector('#sg-cadd'))?.focus({ preventScroll: true });
      }).catch(() => {});
    }
  }

  root.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el || !ACTS.has(el.dataset.act) || !root.contains(el)) return;
    const what = el.dataset.act;
    if (what === 'cvol-down' || what === 'cvol-up') {
      // A press of the pointer stepped already; a click with no pointer is a key: one step.
      if (event.detail === 0) nudge(what === 'cvol-up' ? 1 : -1);
      return;
    }
    if (what === 'cmute') {
      const v = volume();
      if (v && (inert() || !v.inert)) act.setMute(!v.muted).catch(() => {});
      return;
    }
    // A word that cannot be used does nothing, but for the viewer the core turns away: that call
    // is made so the toast says why.
    if (el.getAttribute('aria-disabled') === 'true' && !inert()) return;
    press(el);
  }, { signal });

  // a row is picked by a press on it (not on a word that acts)
  shelf.addEventListener('click', (event) => {
    const row = event.target.closest?.('.sg-prow');
    if (row && !event.target.closest('select, input, button')) pickPoint(Number(row.dataset.n));
  }, { signal });
}
