/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a press on the computer's side does on the theme Ember's page: the switch, the mode, the
// presets, the table of points (a row picked, a value typed, the type chosen, a point removed or
// added) and the curve under the mouse and the finger. A tap on an empty spot adds a point, a
// press on a point drags its frequency and gain, the wheel, a right drag or a pinch set its Q; the
// gestures are tone-drag.js's and the steps and writes the core's (`computerSteps`,
// `pointWriter`). While a point is dragged the tone part's swap waits (keep.js) and this module
// moves the curve, the point and the row's cells itself from what the core holds; the part is
// drawn again from the host's model once the writes are answered. Every element carries
// `data-comp` and none carries `data-json`, so press.js leaves them alone.

import { ember, mine } from './bind.js';
import { pick } from './computer-view.js';
import {
  curveOf, gainText, hzText, moreWords, qText,
} from './draw/computer.js';
import { holdTone } from './keep.js';
import {
  LABEL_GAP_PX, REACH, computerSteps, createDrag, grab, labelAt, plotMap, pointWriter, startOf,
  wheelSteps,
} from '../../core/tone-drag.js';

const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;
/** A press that moves less than this (CSS px) and lets go within TAP_MS is a tap. */
const TAP_PX = 6;
const TAP_MS = 600;
/** A wheel burst ends, and its last Q is written as the final value, after this long. */
const WHEEL_END_MS = 300;
const STEPS = computerSteps();
const FIELD = { f: [20, 20000], g: [-12, 12], q: [0.1, 10] };

/** Draws the parts whose swap waited (live.js's), given to [`installComputer`]. */
let readWaiting = () => {};

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const core = () => ember.core;
const act = () => ember.core.actions.computer;
const tone = () => core().computer.toneOf(core().model.page());

/** The drag under way (or let go of and waiting for its answers), or nothing. */
let session = null;
/** A press on an empty spot that may turn into a tap. */
let tap = null;
/** The wheel's burst. */
let wheel = null;
/** The plot the cursor is over with the grab cursor on it. */
let hovered = null;

/** Whether the computer's controls may be used: the page takes writes and the stream is up. */
function usable() {
  const part = document.getElementById('part-tone');
  const body = document.body.classList;
  return Boolean(part) && !part.hasAttribute('data-inert') && !part.closest('[inert]')
    && body.contains('writes') && !body.contains('lost');
}

/** Whether `element` is a control that is switched off. */
const off = (element) => element.getAttribute('aria-disabled') === 'true' || element.disabled;

const within = (event, rect) => ({ x: event.clientX - rect.left, y: event.clientY - rect.top });
const plotOf = (target) => target.closest?.('.eqg.pts .plot') ?? null;

/** The points a press can grab, in CSS px within a plot of `rect`. */
function pointsIn(rect) {
  const t = tone();
  return (t?.markers ?? []).map((m) => ({
    band: m.n,
    x: (m.x / VIEW_WIDTH) * rect.width,
    y: (m.y / VIEW_HEIGHT) * rect.height,
  }));
}

// ------------------------------------------------------------------------------- the table

const scroller = () => document.querySelector('#part-tone .pscroll');

/** The words under the table: the rows beyond the ones in view. */
function updateMore() {
  const box = scroller();
  const more = document.querySelector('#part-tone .more');
  if (!box || !more) return;
  const rowHeight = box.scrollHeight / Math.max(box.children.length, 1);
  const hidden = Math.ceil((box.scrollHeight - box.scrollTop - box.clientHeight - 1) / rowHeight);
  more.textContent = box.classList.contains('long') ? moreWords(hidden) : '';
  box.classList.toggle('end', hidden <= 0);
}

/** Scroll the table, not the page, so that row `n` is in view. */
function reveal(n) {
  const box = scroller();
  const row = box?.querySelector(`.pt[data-n="${n}"]`);
  if (!box || !row) return;
  const top = row.offsetTop - box.offsetTop;
  if (top < box.scrollTop) box.scrollTop = top;
  else if (top + row.offsetHeight > box.scrollTop + box.clientHeight) {
    box.scrollTop = top + row.offsetHeight - box.clientHeight;
  }
  updateMore();
}

/** Highlight row `n` of the table (and scroll it into view). */
function pickPoint(n, { scroll = true } = {}) {
  pick.n = n;
  for (const row of document.querySelectorAll('#part-tone .pt')) {
    row.classList.toggle('on', Number(row.dataset.n) === n);
  }
  if (scroll) reveal(n);
}

/** A screen or the tone part is in place: the table keeps its scroll, the picked row shows. */
let kept = 0;
export function syncComputer() {
  const box = scroller();
  if (!box) return;
  box.scrollTop = kept;
  if (pick.n !== null) reveal(pick.n);
  updateMore();
}

/** Move the curve, the points and the cells to what the core holds now (a drag in motion). */
function patch() {
  const t = tone();
  const part = document.getElementById('part-tone');
  if (!t || !part?.classList.contains('comp')) return;
  const c = curveOf(t);
  const svg = part.querySelector('.eqg svg');
  const line = svg?.querySelector('.line');
  line?.setAttribute('d', c.line);
  if (line) line.dataset.y = c.ys;
  svg?.querySelector('.area')?.setAttribute('d', c.area);
  svg?.querySelector('.ghost')?.setAttribute('d', c.ghost ?? '');
  for (const m of t.markers) {
    const mark = svg?.querySelector(`.mk[data-n="${m.n}"]`);
    mark?.setAttribute('cx', m.x.toFixed(1));
    mark?.setAttribute('cy', m.y.toFixed(1));
  }
  for (const p of t.points) {
    const row = part.querySelector(`.pt[data-n="${p.n}"]`);
    if (!row) continue;
    const text = { f: hzText(p.f), g: gainText(p.g), q: qText(p.q) };
    for (const [param, words] of Object.entries(text)) {
      const cell = row.querySelector(`.cell[data-param="${param}"]`);
      if (cell && cell.textContent !== words && !cell.querySelector('input')) cell.textContent = words;
    }
  }
  const pline = part.querySelector('.pline');
  if (pline) pline.firstChild.textContent = t.line;
}

// ------------------------------------------------------------------------------ the dragging

/** The label beside the dragged point: its three values, above it and inside the plot. */
function placeLabel(s) {
  const t = tone();
  const point = t?.points[s.n - 1];
  const mark = t?.markers.find((m) => m.n === s.n);
  if (!point || !mark) return;
  const rect = s.plot.getBoundingClientRect();
  s.label.textContent = `${hzText(point.f)} · ${gainText(point.g)} · ${qText(point.q)}`;
  const at = { x: (mark.x / VIEW_WIDTH) * rect.width, y: (mark.y / VIEW_HEIGHT) * rect.height };
  const size = { width: s.label.offsetWidth, height: s.label.offsetHeight };
  const place = labelAt(at, { width: rect.width, height: rect.height }, size);
  if (at.y - LABEL_GAP_PX - size.height < 0) {
    place.y = clamp(at.y + LABEL_GAP_PX, 0, Math.max(rect.height - size.height, 0));
  }
  s.label.style.setProperty('--lx', place.x.toFixed(1));
  s.label.style.setProperty('--ly', place.y.toFixed(1));
}

/** Every write of the drag is answered: the last value is sent as final, the hold on the tone
 * part goes, and the part is read again from the model. */
async function finish(s) {
  await Promise.all(s.drags.map((drag) => drag.settled));
  if (s.wrote) await s.write.release();
  if (session === s) session = null;
  s.forget();
  holdTone(false, true);
  readWaiting();
}

/** The drag has ended: the pointer is let go of and the label goes. */
function ended(s) {
  watchTouch(s, false);
  s.touchIds = null;
  s.pointerId = null;
  s.over = true;
  s.plot.classList.remove('grabbing');
  s.label.remove();
  s.marker?.classList.remove('on');
  finish(s);
}

/** Start the gesture `gesture` of the session's point from `start` (its raw steps), pressed at
 * `press`; a drag it replaces must have been let go of by the caller. */
function launch(s, gesture, press, start) {
  const rect = s.plot.getBoundingClientRect();
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
        placeLabel(s);
      },
      answer: () => {},
      toast: (words) => core().frame.toast(words),
      ended: () => {
        if (s.drag === drag) ended(s);
      },
    },
  });
  s.drag = drag;
  s.drags.push(drag);
}

/** Begin a drag of point `n` on `plot`, the pointer's (`pointerId`) or the fingers' (`touchIds`). */
function begin({
  n, plot, gesture, press, pointerId = null, touchIds = null,
}) {
  const point = tone()?.points[n - 1];
  if (!point) return null;
  const label = document.createElement('span');
  label.className = 'dlab';
  label.setAttribute('aria-hidden', 'true');
  const s = {
    n,
    plot,
    label,
    drag: null,
    drags: [],
    pointerId,
    touchIds,
    write: pointWriter({ actions: ember.core.actions, n, steps: STEPS }),
    marker: plot.querySelector(`.mk[data-n="${n}"]`),
    wrote: false,
    over: false,
    forget: () => {},
  };
  session = s;
  holdTone(true);
  s.forget = core().computer.track({ cancel: (why) => s.drag?.cancel(why) });
  pickPoint(n);
  plot.append(label);
  plot.classList.remove('hot');
  plot.classList.add('grabbing');
  s.marker?.classList.add('on');
  launch(s, gesture, press, startOf(STEPS, point));
  placeLabel(s);
  if (touchIds) watchTouch(s, true);
  return s;
}

function onDown(event) {
  if (wheel) finishWheel();
  if (event.pointerType === 'touch' || (event.button !== 0 && event.button !== 2)) {
    if (event.pointerType === 'touch') startTap(event);
    return;
  }
  const plot = plotOf(event.target);
  if (!plot || !usable() || (session && !session.over)) return;
  const rect = plot.getBoundingClientRect();
  const at = within(event, rect);
  const wide = event.button === 2;
  const n = grab(pointsIn(rect), at, { reach: REACH.mouse, wide, picked: pick.n });
  if (n === null) {
    if (!wide) startTap(event);
    return;
  }
  event.preventDefault();
  try {
    plot.setPointerCapture(event.pointerId);
  } catch {
    // Without capture the drag still ends on a move with no button down.
  }
  begin({
    n, plot, gesture: wide ? 'width' : 'move', press: at, pointerId: event.pointerId,
  });
}

/** A press on an empty spot of the plot: a tap if it is let go of where it began. */
function startTap(event) {
  const plot = plotOf(event.target);
  tap = null;
  if (!plot || !usable() || (session && !session.over) || event.isPrimary === false) return;
  const rect = plot.getBoundingClientRect();
  const at = within(event, rect);
  if (grab(pointsIn(rect), at, { reach: event.pointerType === 'touch' ? REACH.touch : REACH.mouse })
    !== null) return;
  tap = { plot, at, id: event.pointerId, time: Date.now() };
}

/** The tap is let go of: a point is added where it was made. */
async function endTap(event) {
  const t = tap;
  tap = null;
  if (!t || event.pointerId !== t.id || Date.now() - t.time > TAP_MS) return;
  const rect = t.plot.getBoundingClientRect();
  const at = within(event, rect);
  if (Math.hypot(at.x - t.at.x, at.y - t.at.y) > TAP_PX || !usable()) return;
  const map = plotMap({ width: rect.width, height: rect.height });
  const point = core().computer.addAt(map.hzAt(t.at.x), map.dbAt(t.at.y));
  if (point) await act().addPoint(point);
}

// ------------------------------------------------------------------------------------- touch

const touchOf = (list, id) => [...list].find((touch) => touch.identifier === id) ?? null;
const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);

function watchTouch(s, on) {
  const call = on ? 'addEventListener' : 'removeEventListener';
  s.plot[call]('touchmove', onTouchMove, { passive: false });
  s.plot[call]('touchend', onTouchEnd);
  s.plot[call]('touchcancel', onTouchEnd);
}

/** A finger down on the plot: within reach of a point it grabs it (the page does not scroll for
 * it); a second finger makes a pinch of the drag; anywhere else the touch is the page's. */
function onTouchStart(event) {
  const plot = plotOf(event.target);
  if (!plot || !usable()) return;
  const s = session;
  if (s && !s.over) {
    if (!s.touchIds || s.plot !== plot) return;
    event.preventDefault();
    if (s.touchIds.length !== 1 || event.touches.length !== 2) return;
    const first = touchOf(event.touches, s.touchIds[0]);
    const second = [...event.changedTouches].find((t) => t.identifier !== s.touchIds[0]);
    if (!first || !second) return;
    const old = s.drag;
    s.touchIds = [first.identifier, second.identifier];
    launch(s, 'pinch', { distance: spacing(first, second) }, startOf(STEPS, tone().points[s.n - 1]));
    old.release();
    return;
  }
  const rect = plot.getBoundingClientRect();
  const points = pointsIn(rect);
  if (event.touches.length === 1) {
    const touch = event.touches[0];
    const n = grab(points, within(touch, rect), { reach: REACH.touch, picked: pick.n });
    if (n === null) return;
    event.preventDefault();
    begin({
      n, plot, gesture: 'move', press: within(touch, rect), touchIds: [touch.identifier],
    });
  } else if (event.touches.length === 2) {
    const [a, b] = event.touches;
    const middle = within({
      clientX: (a.clientX + b.clientX) / 2, clientY: (a.clientY + b.clientY) / 2,
    }, rect);
    const n = points.some((p) => p.band === pick.n) ? pick.n
      : grab(points, middle, { reach: REACH.touch, wide: true });
    if (n === null) return;
    event.preventDefault();
    begin({
      n, plot, gesture: 'pinch', press: { distance: spacing(a, b) }, touchIds: [a.identifier, b.identifier],
    });
  }
}

function onTouchMove(event) {
  const s = session;
  if (!s?.touchIds) return;
  const touches = s.touchIds.map((id) => touchOf(event.touches, id));
  if (touches.some((t) => !t)) return;
  if (event.cancelable) event.preventDefault();
  if (touches.length === 1) s.drag.move(within(touches[0], s.plot.getBoundingClientRect()));
  else s.drag.spread(spacing(touches[0], touches[1]));
}

function onTouchEnd(event) {
  const s = session;
  if (!s?.touchIds) return;
  const gone = [...event.changedTouches].filter((t) => s.touchIds.includes(t.identifier));
  if (gone.length === 0) return;
  if (event.type === 'touchend' && s.touchIds.length === 1) {
    s.drag.move(within(gone[0], s.plot.getBoundingClientRect()));
  }
  s.drag.release();
}

// ---------------------------------------------------------------------------- mouse and wheel

function hover(event) {
  const plot = event.pointerType === 'touch' ? null : plotOf(event.target);
  if (hovered && hovered !== plot) hovered.classList.remove('hot');
  hovered = plot;
  if (!plot) return;
  const rect = plot.getBoundingClientRect();
  const can = usable()
    && grab(pointsIn(rect), within(event, rect), { reach: REACH.mouse, picked: pick.n }) !== null;
  plot.classList.toggle('hot', can);
}

function onMove(event) {
  if (tap) {
    const box = tap.plot.getBoundingClientRect();
    if (Math.hypot(event.clientX - box.left - tap.at.x, event.clientY - box.top - tap.at.y) > TAP_PX) {
      tap = null;
    }
  }
  const s = session;
  if (!s || s.over) {
    hover(event);
    return;
  }
  if (event.pointerId !== s.pointerId) return;
  if (event.buttons === 0) {
    s.drag.release();
    return;
  }
  s.drag.move(within(event, s.plot.getBoundingClientRect()));
}

function onUp(event) {
  const s = session;
  if (s && !s.over && event.pointerId === s.pointerId) {
    s.drag.move(within(event, s.plot.getBoundingClientRect()));
    s.drag.release();
    return;
  }
  endTap(event);
}

/** A burst of wheel notches ends: the Q it reached is written as the final value. */
function finishWheel() {
  const w = wheel;
  wheel = null;
  if (!w) return;
  clearTimeout(w.timer);
  w.forget();
  act().setPoint(w.n, {}, { done: true });
}

/** A notch over a point steps its Q (up narrows) and the page does not scroll; elsewhere the page
 * scrolls. */
function onWheel(event) {
  const plot = plotOf(event.target);
  if (!plot || event.ctrlKey || !usable()) return;
  if (session && !session.over) {
    event.preventDefault();
    return;
  }
  const rect = plot.getBoundingClientRect();
  const points = pointsIn(rect);
  const n = wheel && points.some((p) => p.band === wheel.n) ? wheel.n
    : grab(points, within(event, rect), { reach: REACH.wheel, picked: pick.n });
  if (n === null) return;
  event.preventDefault();
  const point = tone().points[n - 1];
  if (!wheel || wheel.n !== n) {
    finishWheel();
    wheel = {
      n, idx: startOf(STEPS, point).q, rest: 0, timer: 0, forget: core().computer.track({ end: finishWheel }),
    };
  }
  const { steps, rest } = wheelSteps(wheel.rest, event.deltaY, event.deltaMode);
  wheel.rest = rest;
  const idx = clamp(wheel.idx + steps, 0, STEPS.q.length - 1);
  clearTimeout(wheel.timer);
  wheel.timer = setTimeout(finishWheel, WHEEL_END_MS);
  if (idx === wheel.idx) return;
  wheel.idx = idx;
  pickPoint(n);
  act().setPoint(n, { q: STEPS.q[idx] }, { done: false });
}

// ----------------------------------------------------------------------------------- the table

/** The number a typed value means: `2.9k`, `2900 Hz`, `+3,2 dB` and `−2` all read; `null` when it
 * is none. */
function parse(text) {
  const clean = text.trim().replace('−', '-').replace(',', '.').replace(/\s*(hz|db)$/i, '');
  const kilo = /k$/i.test(clean);
  const number = Number(kilo ? clean.slice(0, -1) : clean);
  return clean !== '' && Number.isFinite(number) ? number * (kilo ? 1000 : 1) : null;
}

/** Turn a cell into a field: Enter writes the value typed (within the hand ranges), Esc or
 * leaving puts it back. */
function edit(cell) {
  if (cell.querySelector('input')) return;
  const n = Number(cell.dataset.n);
  const { param } = cell.dataset;
  const input = document.createElement('input');
  input.inputMode = 'decimal';
  input.setAttribute('aria-label', cell.getAttribute('aria-label'));
  const before = cell.textContent;
  input.value = before.replace(/\s*(Hz|dB)$/, '');
  cell.replaceChildren(input);
  input.focus();
  input.select();
  let done = false;
  const close = () => {
    if (done) return;
    done = true;
    if (cell.isConnected) cell.textContent = before;
    readWaiting();
  };
  input.addEventListener('keydown', async (key) => {
    if (key.key === 'Escape') {
      key.stopPropagation();
      close();
    }
    if (key.key !== 'Enter') return;
    const value = parse(input.value);
    done = true;
    cell.textContent = before;
    if (value === null) {
      core().frame.toast('Not a number.');
      readWaiting();
      return;
    }
    if (value === parse(before)) {
      readWaiting();
      return;
    }
    const [lo, hi] = FIELD[param];
    await act().setPoint(n, { [param]: clamp(value, lo, hi) }, { done: true });
    readWaiting();
  });
  input.addEventListener('blur', close);
}

/** The id of the switch's half the focus is owed to after a keyboard flip. */
let wantFocus = null;
function giveFocus() {
  if (wantFocus && document.activeElement?.id !== wantFocus) {
    document.getElementById(wantFocus)?.focus({ preventScroll: true });
  }
}

/** A form of the presets' list is sent: save the points on show, rename or delete a preset. */
async function submitted(event) {
  const form = event.target.closest?.('form[data-comp="form"]');
  if (!form) return;
  event.preventDefault();
  const value = (name) => form.elements[name]?.value ?? '';
  const { actions } = core();
  form.dataset.sending = '';
  let answer = null;
  try {
    if (form.dataset.form === 'rename') answer = await actions.renamePreset(form.dataset.from, value('to'));
    else if (form.dataset.form === 'delete') answer = await actions.deletePreset(form.dataset.from);
    else answer = await act().savePreset(value('name'));
    if (answer?.ok) await core().computer.resync();
  } finally {
    if (!answer?.ok) delete form.dataset.sending;
    else readWaiting();
  }
}

/** The table's and the controls' presses. */
function clicked(event) {
  const switcher = event.target.closest('#part-switch .opt[data-side]');
  if (switcher) {
    // a press from the keyboard keeps the focus on the chosen half across the redraw
    const side = switcher.dataset.side;
    wantFocus = event.detail === 0 ? `side-${side}` : null;
    core().computer.flip(side);
    giveFocus();
    setTimeout(() => {
      giveFocus();
      wantFocus = null;
    }, 60);
    return;
  }
  const control = event.target.closest('[data-comp]');
  const row = event.target.closest('#part-tone .pt');
  if (!control) {
    if (row && !event.target.closest('select, input')) pickPoint(Number(row.dataset.n));
    return;
  }
  if (off(control) || !document.body.classList.contains('writes')) return;
  const n = Number(control.dataset.n);
  switch (control.dataset.comp) {
    case 'mode':
      if (control.getAttribute('aria-pressed') !== 'true') act().setMode(control.dataset.mode);
      break;
    case 'preset':
      act().recallPreset(control.dataset.name);
      break;
    case 'add': {
      const t = tone();
      const point = t && core().computer.addAt(core().computer.nextFree(t.points), 0);
      if (point) act().addPoint(point);
      break;
    }
    case 'remove':
      if (pick.n === n) pick.n = null;
      act().removePoint(n);
      break;
    case 'cell':
      pickPoint(n, { scroll: false });
      edit(control);
      break;
    default:
  }
}

/** A type chosen in a row. */
function chosen(event) {
  const select = event.target.closest?.('#part-tone select[data-comp="type"]');
  if (!select || off(select)) return;
  const n = Number(select.dataset.n);
  const { value } = select;
  select.blur();
  pickPoint(n, { scroll: false });
  act().setPoint(n, { type: value }, { done: true });
}

/** Listen for the presses on the computer's controls and the mouse and the finger on its curve;
 * `given.readWaiting` is called when a hold ends; the listeners go with the mount's signal. */
export function installComputer(given) {
  readWaiting = given.readWaiting;
  document.addEventListener('click', clicked, mine());
  document.addEventListener('change', chosen, mine());
  document.addEventListener('submit', submitted, mine());
  document.addEventListener('pointerdown', onDown, mine());
  document.addEventListener('pointermove', onMove, mine());
  document.addEventListener('pointerup', onUp, mine());
  for (const name of ['pointercancel', 'lostpointercapture']) {
    document.addEventListener(name, (event) => {
      if (event.pointerId === session?.pointerId) session.drag.release();
      if (event.pointerId === tap?.id) tap = null;
    }, mine());
  }
  document.addEventListener('touchstart', onTouchStart, mine({ passive: false }));
  document.addEventListener('wheel', onWheel, mine({ passive: false }));
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') session?.drag.cancel('esc');
  }, mine({ capture: true }));
  document.addEventListener('svolder-lost', (event) => {
    if (event.detail && session) session.drag.cancel('lost');
  }, mine());
  window.addEventListener('blur', () => session?.drag.release(), mine());
  document.addEventListener('scroll', (event) => {
    if (event.target === scroller()) {
      kept = scroller().scrollTop;
      updateMore();
    }
  }, mine({ capture: true, passive: true }));

  // The table keeps its scroll across the part's swap; a drag in motion moves what is drawn.
  document.body.addEventListener('htmx:oobBeforeSwap', (event) => {
    if (event.detail.target?.id === 'part-tone') kept = scroller()?.scrollTop ?? 0;
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id === 'part-switch') giveFocus();
    if (event.detail.target?.id === 'part-tone') syncComputer();
  }, mine());
  const stop = core().computer.on(({ type, from, to }) => {
    // an edit that ended moved a point in the table's frequency order: the row in hand follows
    if (type === 'moved' && pick.n === from) pick.n = to;
    if (session) patch();
  });
  ember.signal?.addEventListener('abort', () => {
    stop();
    session = null;
    tap = null;
    wheel = null;
    hovered = null;
  }, { once: true });
}
