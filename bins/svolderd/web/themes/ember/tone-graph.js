/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Response graph under the mouse and the finger: a press on a band's point grabs it, a left
// drag moves its frequency and gain, a right drag or the wheel sets its Q, and so does a pinch. The
// gestures are tone-drag.js's and the curve's maths tone-curve.js's; this is the DOM around them:
// the pointer's place in the plot, the pointer capture, the cursor, the label, the redrawing of the
// curve while the values are held (the page draws them itself, with no easing), the band table's
// cells written from each answer, and the swap hold that keeps the graph in place until the drag
// and its writes are done.
//
// A finger is read from the touch events, not the pointer's (the mouse path leaves a `touch`
// pointer alone): a touch that starts within reach of a point is taken (the page does not scroll
// for it), two touches on the plot are a pinch (the page neither zooms nor scrolls), and a touch
// anywhere else is the page's. Both ways make the same drag, so they share the label, the drawing
// and the swap hold.
//
// The steps and words come from the screen's `#tone-steps` tables and the readings from the
// graph's `data-readings`, so the page never converts or formats a number itself.
//
// ember.js calls `installToneGraph` where its listeners stand, giving it the function that draws
// the parts whose swap waited, and `endDrag` when a whole screen is about to be swapped.

import { newest } from '../../core/api.js';
import { ember, mine as scoped, shown } from './bind.js';
import { holdTone, staleTone } from './keep.js';
import { pickBand, syncEditor } from './tone-editor.js';
import { once, settle } from './tone-motion.js';
import {
  POINTS, clippingPeak, linePath, logFreqs, markerPoint, peakWords, responseDb, yOf,
} from '../../core/tone-curve.js';
import { LABEL_GAP_PX, REACH, createDrag, grab, labelAt, wheelSteps } from '../../core/tone-drag.js';

const SVG = 'http://www.w3.org/2000/svg';
const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;
const STEP_MAX = 127;
// The band table's column of each parameter (the band's number is column 0).
const COLUMN = { f: 1, g: 2, q: 3 };
const PARAM = { f: 'frequency', g: 'gain', q: 'q' };
const FREQS = logFreqs(POINTS);
// A wheel burst goes on from the last Q it sent while notches keep coming within this many ms.
const WHEEL_BURST_MS = 700;

/** Draws the parts whose swap waited (live.js's), given to [`installToneGraph`]. */
let readWaiting = () => {};

/** The drag under way, or the last one let go of and waiting for its answers, or nothing. */
let session = null;
/** Every drag that has begun and not finished: the one above and those only waiting for their
 * answers (a new grab may start while they wait). */
const live = new Set();
/** The drag that began last. */
let latest = null;
/** The wheel's burst: the band, the last Q sent and its sender; `claim` while it owns the wheel
 * over the graph, `held` the time of the last notch that changed the Q (see [`onWheel`]). */
let wheel = null;
/** The plot the cursor is over with the grab cursor on it. */
let hovered = null;

const valid = (raw) => Number.isInteger(raw) && raw >= 0 && raw <= STEP_MAX;
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

// ----------------------------------------------------------------------------- what the page holds

let tablesNode = null;
let tablesParsed = null;

/** The step tables the screen carries, parsed once per screen, or nothing. */
function stepTables() {
  const node = document.getElementById('tone-steps');
  if (node !== tablesNode) {
    tablesNode = node;
    try {
      tablesParsed = node ? JSON.parse(node.textContent) : null;
    } catch {
      tablesParsed = null;
    }
  }
  return tablesParsed;
}

/** The raw steps a finished drag asked of its band: its held values, and the Q of a wheel notch
 * that came after it. */
const askedBy = (s) => ({
  ...s.drag.held, q: s.wheelRefused ? s.drag.held.q : s.wheelQ ?? s.drag.held.q,
});

/** The readings of a graph: `{ mode, bands: [[f, g, q], …] }`, or nothing. A drag that is let go
 * of and waits for its answers is put over them: the figure still holds the readings from before
 * it, and what is drawn (and may be grabbed again) is where it left its band. */
function readingsOf(figure) {
  let readings;
  try {
    readings = JSON.parse(figure.dataset.readings);
  } catch {
    return null;
  }
  if (!Array.isArray(readings?.bands)) return null;
  for (const s of live) {
    if (!s.over || !s.wrote || s.refused || s.lost || s.figure !== figure) continue;
    const held = askedBy(s);
    readings = {
      mode: 'eq',
      bands: readings.bands.map((r, i) => (i === s.band - 1 ? [held.f, held.g, held.q] : r)),
    };
  }
  return readings;
}

/** The value of step `raw` in `rows` (`[[value, words], …]`), or nothing. */
function entry(rows, raw) {
  const row = valid(raw) ? rows?.[raw] : null;
  return row && typeof row[0] === 'number' && Number.isFinite(row[0]) ? row : null;
}

/** Band `index` (0-based) as `{ f0, gainDb, q }` from its raw steps `[f, g, q]`, or nothing when
 * a step is unread or not one the tables hold. */
function bandOf(tables, index, raws) {
  if (!Array.isArray(raws) || raws.length !== 3) return null;
  const f = entry(tables.bands?.[index]?.f, raws[0]);
  const g = entry(tables.gain, raws[1]);
  const q = entry(tables.q, raws[2]);
  return f && g && q && f[0] > 0 ? { f0: f[0], gainDb: g[0], q: q[0] } : null;
}

/** The points a press can grab, in CSS px within a plot of `rect`: one per band that is read in
 * full, none in Loudness or while the mode is unread. */
function pointsOf(tables, readings, rect) {
  if (readings.mode !== 'eq' && readings.mode !== 'flat') return [];
  const out = [];
  readings.bands.forEach((raws, index) => {
    const band = bandOf(tables, index, raws);
    if (!band) return;
    const at = markerPoint(readings.mode, band);
    out.push({
      band: index + 1,
      x: at.x / VIEW_WIDTH * rect.width,
      y: at.y / VIEW_HEIGHT * rect.height,
    });
  });
  return out;
}

/** Whether the graph may be used: the screen takes writes and the stream is up. */
function usable() {
  const part = document.getElementById('part-tone');
  const body = document.body.classList;
  return !!part && !part.hasAttribute('data-inert') && !part.closest('[inert]')
    && body.contains('writes') && !body.contains('lost');
}

/** The band picked in the table, or nothing. */
function pickedBand() {
  const row = document.querySelector('#part-bands .band[aria-pressed="true"]');
  return row ? Number(row.dataset.band) : null;
}

/** Where `event` is within a plot of `rect`, in CSS px. */
const within = (event, rect) => ({ x: event.clientX - rect.left, y: event.clientY - rect.top });

// ------------------------------------------------------------------------------------- the drawing

/** The words of the band's three values at `raws` (`{ f, g, q }`), joined as the table's cells. */
function wordsOf(tables, band, raws) {
  const words = [
    tables.bands[band - 1].f[raws.f]?.[1], tables.gain[raws.g]?.[1], tables.q[raws.q]?.[1],
  ];
  return words.join(' · ');
}

/** Show the label beside the dragged point: its words, above the point and inside the plot, or
 * below it where there is no room above. */
function placeLabel(s, raws) {
  const rect = s.plot.getBoundingClientRect();
  s.label.textContent = wordsOf(s.tables, s.band, raws);
  const band = bandOf(s.tables, s.band - 1, [raws.f, raws.g, raws.q]);
  if (!band) return;
  const at = markerPoint('eq', band);
  const point = { x: at.x / VIEW_WIDTH * rect.width, y: at.y / VIEW_HEIGHT * rect.height };
  const size = { width: s.label.offsetWidth, height: s.label.offsetHeight };
  const plot = { width: rect.width, height: rect.height };
  const place = labelAt(point, plot, size);
  if (point.y - LABEL_GAP_PX - size.height < 0) {
    place.y = clamp(point.y + LABEL_GAP_PX, 0, Math.max(plot.height - size.height, 0));
  }
  s.label.style.setProperty('--lx', place.x.toFixed(1));
  s.label.style.setProperty('--ly', place.y.toFixed(1));
}

/** Draw the graph from the readings with the dragged band at `raws` (`{ f, g, q }`): the line,
 * the area, the point, the peak line and its words, as the theme draws them in EQ. */
function draw(s, raws) {
  const bands = s.readings.bands.map((r, i) => (i === s.band - 1 ? [raws.f, raws.g, raws.q] : r));
  const parsed = bands.map((r, i) => bandOf(s.tables, i, r));
  const complete = parsed.every(Boolean);
  const ys = complete ? responseDb(parsed, FREQS).map(yOf) : FREQS.map(() => yOf(0));
  const line = linePath(ys);
  const svg = s.plot.querySelector('svg');
  const pathLine = svg.querySelector('.line');
  pathLine?.setAttribute('d', line);
  if (pathLine) pathLine.dataset.y = ys.map((y) => y.toFixed(1)).join(',');
  svg.querySelector('.area')?.setAttribute('d', `${line}L1000.0 130.0L0.0 130.0Z`);
  svg.querySelector('.ghost')?.remove();
  const own = parsed[s.band - 1];
  if (own && s.marker) {
    const at = markerPoint('eq', own);
    s.marker.setAttribute('cx', at.x.toFixed(1));
    s.marker.setAttribute('cy', at.y.toFixed(1));
  }
  const peak = complete ? clippingPeak('eq', parsed) : null;
  drawPeak(s, peak);
  placeLabel(s, raws);
}

/** The peak line and its words: made, moved or taken away as `peak` (dB, or nothing) says. */
function drawPeak(s, peak) {
  const svg = s.plot.querySelector('svg');
  let line = svg.querySelector('.peak');
  let words = s.figure.querySelector('.pkt');
  if (peak === null) {
    line?.remove();
    words?.remove();
    return;
  }
  const y = yOf(peak).toFixed(1);
  if (!line) {
    line = document.createElementNS(SVG, 'line');
    line.setAttribute('class', 'peak');
    line.setAttribute('x1', '0');
    line.setAttribute('x2', String(VIEW_WIDTH));
    svg.insertBefore(line, svg.querySelector('.mk'));
  }
  line.setAttribute('y1', y);
  line.setAttribute('y2', y);
  if (!words) {
    words = document.createElement('span');
    words.className = 'pkt';
    s.plot.after(words);
  }
  words.textContent = peakWords(peak);
}

/** Write the dragged band's cell from a write's answer and flash it. */
function writeCell(s, param, text) {
  if (!s.plot.isConnected || shown() !== s.device) return;
  const cell = document.querySelector(`#part-bands .band[data-band="${s.band}"]`)
    ?.children[COLUMN[param]];
  if (!cell) return;
  if (cell.textContent !== text) {
    cell.textContent = text;
    once(cell, 'flash');
  }
  syncEditor();
}

// --------------------------------------------------------------------------------------- the drag

/** The drags that began before `s` and have not finished, on its device. */
function earlier(s) {
  const out = [];
  for (const o of live) {
    if (o === s) break;
    if (o.device === s.device) out.push(o);
  }
  return out;
}

/** The write of one parameter of the dragged band, as tone-drag.js asks for it. */
function writeOf(s) {
  return async (param, raw) => {
    const { core, signal } = ember;
    // behind every write of the drags before this one (their last values must not land after
    // this one's); an earlier drag never waits for a later one, so this cannot wait forever
    await Promise.all(earlier(s).flatMap((o) => o.drags.map((drag) => drag.settled)));
    if (s.lost || signal.aborted) return {};
    const answer = await core.actions.setBand(s.band, PARAM[param], { raw },
      { device: s.device });
    if (signal.aborted) return {};
    if (!answer.ok) {
      s.refused = true;
      return { refused: answer.error };
    }
    return { text: answer.text, modeSetToEq: answer.mode_set_to_eq };
  };
}

/** The drag has ended: the pointer is let go of and the label goes; once every write is answered
 * the swap hold goes too. */
function ended(s) {
  watchTouch(s, false);
  // the pointer and the fingers are done with: a touch or wheel now is the page's again
  s.touchIds = null;
  s.pointerId = null;
  s.over = true;
  s.plot.classList.remove('grabbing');
  s.label.remove();
  s.marker?.classList.remove('on');
  Promise.all(s.drags.map((drag) => drag.settled)).then(() => finish(s));
}

/** Every write of the drag has been answered. A drag the device took keeps what it drew (the
 * stream's echo lands on it without a jump); one it refused, or that lost the stream, is read
 * back from the server. */
function finish(s) {
  if (session === s) session = null;
  live.delete(s);
  const again = s.wrote && (s.refused || s.lost || s.wheelRefused);
  // only the drag that wrote last says what the figure holds: it began from what the ones before
  // it asked
  if (s.wrote && !again && s.figure.isConnected && latest === s) {
    const held = askedBy(s);
    const bands = s.readings.bands.map((r, i) => (i === s.band - 1 ? [held.f, held.g, held.q] : r));
    s.figure.dataset.readings = JSON.stringify({ mode: 'eq', bands });
  }
  holdTone(false, again);
  readWaiting();
}

/** End the drag where it is, as a press let go with no more to say about the pointer. */
function release() {
  session?.drag.release();
}

/** End the drag where it is because the screen is going: what was asked is still written. */
export function endDrag() {
  session?.drag.cancel('switch');
}

/** Start the gesture `gesture` of the session's band from `start` (its raw steps `{ f, g, q }`),
 * pressed at `press`, and make it the session's drag. A drag it replaces must have been let go of
 * by the caller; its writes are still waited for. */
function launch(s, gesture, press, start) {
  const rect = s.plot.getBoundingClientRect();
  const drag = createDrag({
    band: s.band,
    gesture,
    start,
    press,
    plot: { width: rect.width, height: rect.height },
    steps: s.steps,
    write: writeOf(s),
    on: {
      held: (held) => {
        if (!s.wrote) {
          // from its first write on this drag says what the figure holds (it began from what
          // the drags before it asked)
          s.wrote = true;
          latest = s;
        }
        draw(s, held);
      },
      answer: (param, text) => writeCell(s, param, text),
      toast: (words) => ember.core?.frame.toast(words),
      // a replaced drag has nothing more to end
      ended: () => {
        if (s.drag === drag) ended(s);
      },
    },
  });
  s.drag = drag;
  s.drags.push(drag);
}

/** Begin a drag of `band` on `plot`, the pointer's (`pointerId`) or the fingers' (`touchIds`). */
function begin({
  band, plot, tables, readings, gesture, press, pointerId = null, touchIds = null,
}) {
  const raws = readings.bands[band - 1];
  const label = document.createElement('span');
  label.className = 'dlab';
  label.setAttribute('aria-hidden', 'true');
  const s = {
    band, plot, figure: plot.closest('.eqg'), tables, readings, label, drag: null, drags: [],
    pointerId, touchIds,
    steps: {
      f: tables.bands[band - 1].f.map(([v]) => (typeof v === 'number' ? v : NaN)),
      g: tables.gain.map(([v]) => v),
      q: tables.q.map(([v]) => v),
    },
    device: shown(), marker: plot.querySelector(`.mk[data-band="${band}"]`),
    wrote: false, refused: false, lost: false, over: false,
  };
  const before = { session, wheel };
  let holding = false;
  session = s;
  live.add(s);
  wheel = null;
  try {
    settle();
    holdTone(true);
    holding = true;
    pickBand(band);
    plot.append(label);
    hovered = null;
    plot.classList.remove('can-grab');
    plot.classList.add('grabbing');
    s.marker?.classList.add('on');
    launch(s, gesture, press, { f: raws[0], g: raws[1], q: raws[2] });
    placeLabel(s, { f: raws[0], g: raws[1], q: raws[2] });
    if (touchIds) watchTouch(s, true);
  } catch (error) {
    // A drag that could not start leaves no hold behind.
    live.delete(s);
    ({ session, wheel } = before);
    watchTouch(s, false);
    if (holding) holdTone(false);
    label.remove();
    plot.classList.remove('grabbing');
    s.marker?.classList.remove('on');
    throw error;
  }
  return s;
}

function onDown(event) {
  if (wheel) wheel.claim = false;
  if ((session && !session.over) || event.pointerType === 'touch') return;
  if (event.button !== 0 && event.button !== 2) return;
  const plot = event.target.closest?.('.eqg .plot');
  const tables = plot && usable() ? stepTables() : null;
  const readings = tables && readingsOf(plot.closest('.eqg'));
  if (!readings) return;
  const rect = plot.getBoundingClientRect();
  const at = within(event, rect);
  const wide = event.button === 2;
  const band = grab(pointsOf(tables, readings, rect), at,
    { reach: REACH.mouse, wide, picked: pickedBand() });
  if (band === null) return;
  event.preventDefault();
  try {
    plot.setPointerCapture(event.pointerId);
  } catch {
    // Without capture the drag still ends on a move with no button down.
  }
  try {
    begin({
      band, plot, tables, readings, gesture: wide ? 'width' : 'move', press: at,
      pointerId: event.pointerId,
    });
  } catch (error) {
    // a drag that could not start does not keep the pointer either
    try {
      plot.releasePointerCapture(event.pointerId);
    } catch {
      // not captured, or already let go
    }
    throw error;
  }
}

// ----------------------------------------------------------------------------------------- touch

/** The touch of `list` with `id`, or nothing. */
function touchOf(list, id) {
  for (const touch of list) {
    if (touch.identifier === id) return touch;
  }
  return null;
}

/** How far apart two touches are, in CSS px. */
const spacing = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);

/** Listen to the touches of the drag `s` on its plot (a touch's events all go to where it
 * started, which is within the plot), or stop. */
function watchTouch(s, on) {
  const call = on ? 'addEventListener' : 'removeEventListener';
  s.plot[call]('touchmove', onTouchMove, { passive: false });
  s.plot[call]('touchend', onTouchEnd);
  s.plot[call]('touchcancel', onTouchEnd);
}

/** The band a pinch with the fingers at `at` acts on: the band picked, or with none picked the
 * one nearest the fingers' midpoint. */
function pinchBand(points, at) {
  const picked = pickedBand();
  if (points.some((point) => point.band === picked)) return picked;
  return grab(points, at, { reach: REACH.touch, wide: true });
}

/** A finger went down on the plot: within reach of a point it grabs it; a second finger makes a
 * pinch of the drag, or starts one; anywhere else the page keeps its touch. */
function onTouchStart(event) {
  const plot = event.target.closest?.('.eqg .plot');
  if (!plot || !usable()) return;
  const s = session;
  if (s && !s.over) {
    // a drag that is over only waits for its writes: a touch is a new grab, or the page's own
    if (!s.touchIds || s.plot !== plot) return;
    event.preventDefault();
    if (s.touchIds.length !== 1 || event.touches.length !== 2) return;
    const first = touchOf(event.touches, s.touchIds[0]);
    const second = [...event.changedTouches].find((t) => t.identifier !== s.touchIds[0]);
    if (!first || !second) return;
    // the finger that grabbed the point is let go of as a drag and the two make a pinch of its band
    const old = s.drag;
    s.touchIds = [first.identifier, second.identifier];
    launch(s, 'pinch', { distance: spacing(first, second) }, old.held);
    old.release();
    return;
  }
  const tables = stepTables();
  const readings = tables && readingsOf(plot.closest('.eqg'));
  if (!readings) return;
  const rect = plot.getBoundingClientRect();
  const points = pointsOf(tables, readings, rect);
  if (event.touches.length === 1) {
    const touch = event.touches[0];
    const at = within(touch, rect);
    const band = grab(points, at, { reach: REACH.touch, picked: pickedBand() });
    if (band === null) return;
    event.preventDefault();
    begin({
      band, plot, tables, readings, gesture: 'move', press: at, touchIds: [touch.identifier],
    });
  } else if (event.touches.length === 2) {
    const [a, b] = event.touches;
    if (a.target.closest?.('.eqg .plot') !== plot || b.target.closest?.('.eqg .plot') !== plot) {
      return;
    }
    const middle = within({
      clientX: (a.clientX + b.clientX) / 2, clientY: (a.clientY + b.clientY) / 2,
    }, rect);
    const band = pinchBand(points, middle);
    if (band === null) return;
    event.preventDefault();
    begin({
      band, plot, tables, readings, gesture: 'pinch', press: { distance: spacing(a, b) },
      touchIds: [a.identifier, b.identifier],
    });
  }
}

/** The drag's fingers moved: a finger carries the point, two fingers' distance the Q. */
function onTouchMove(event) {
  const s = session;
  if (!s?.touchIds) return;
  const touches = s.touchIds.map((id) => touchOf(event.touches, id));
  if (touches.some((t) => !t)) return;
  if (event.cancelable) event.preventDefault();
  if (touches.length === 1) {
    s.drag.move(within(touches[0], s.plot.getBoundingClientRect()));
  } else {
    s.drag.spread(spacing(touches[0], touches[1]));
  }
}

/** One of the drag's fingers lifted or the browser took the touch: the drag ends where it is. */
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

/** The cursor: `can-grab` over a point in reach of a press. */
function hover(event) {
  const plot = event.pointerType === 'touch' ? null : event.target.closest?.('.eqg .plot');
  if (!plot && wheel) wheel.claim = false;
  if (hovered && hovered !== plot) hovered.classList.remove('can-grab');
  hovered = plot ?? null;
  if (!plot) return;
  let can = false;
  const tables = usable() ? stepTables() : null;
  const readings = tables && readingsOf(plot.closest('.eqg'));
  if (readings) {
    const rect = plot.getBoundingClientRect();
    can = grab(pointsOf(tables, readings, rect), within(event, rect),
      { reach: REACH.mouse, picked: pickedBand() }) !== null;
  }
  plot.classList.toggle('can-grab', can);
}

function onMove(event) {
  const s = session;
  if (!s || s.over) {
    hover(event);
    return;
  }
  if (event.pointerId !== s.pointerId) return;
  if (event.buttons === 0) {
    release();
    return;
  }
  s.drag.move(within(event, s.plot.getBoundingClientRect()));
}

function onUp(event) {
  const s = session;
  if (!s || event.pointerId !== s.pointerId) return;
  s.drag.move(within(event, s.plot.getBoundingClientRect()));
  s.drag.release();
}

function onLost() {
  if (wheel) wheel.claim = false;
  for (const s of live) {
    s.lost = true;
    s.drag.cancel('lost');
  }
}

/** One wheel event over the graph: within reach of a point (the wheel's) it steps that band's Q a
 * device step a notch (up narrows) and the page does not scroll. A burst of notches owns the wheel
 * over the plot until WHEEL_BURST_MS pass without one, the pointer leaves the plot, a press comes
 * or the stream is lost: its next notches go on stepping its band even when the hand has drifted
 * off the point. Only a notch that changes the Q renews that claim, so a trackpad's small deltas
 * or a Q at its limit do not hold the page. Elsewhere nothing happens and the page scrolls. */
function onWheel(event) {
  const plot = event.target.closest?.('.eqg .plot');
  if (!plot) {
    if (wheel) wheel.claim = false;
    return;
  }
  if (event.ctrlKey || !usable()) {
    if (wheel) wheel.claim = false;
    return;
  }
  if (session && !session.over) {
    event.preventDefault();
    return;
  }
  const tables = stepTables();
  const readings = tables && readingsOf(plot.closest('.eqg'));
  if (!readings) return;
  const rect = plot.getBoundingClientRect();
  const points = pointsOf(tables, readings, rect);
  const name = shown();
  const now = performance.now();
  const owned = wheel?.claim && wheel.device === name && now - wheel.held < WHEEL_BURST_MS
    && points.some((point) => point.band === wheel.band);
  const band = owned ? wheel.band
    : grab(points, within(event, rect), { reach: REACH.wheel, picked: pickedBand() });
  if (band === null) return;
  event.preventDefault();
  // the Q the page holds for the band (a drag just let go of is in it); a burst goes on from the
  // last Q it sent only while the readings are a Q it knows: one of its own, or the one it began
  // from, as a late answer is. Another Q (a right drag, a stepper, another front end) restarts it.
  const current = readings.bands[band - 1][2];
  const burst = wheel && wheel.band === band && wheel.device === name
    && now - wheel.at < WHEEL_BURST_MS && wheel.known.has(current);
  const base = burst ? wheel.raw : current;
  if (!valid(base)) return;
  // deltaMode first: Firefox reports a wheel's lines only to a page that has read it
  const { deltaMode } = event;
  const { steps, rest } = wheelSteps(burst ? wheel.rest : 0, event.deltaY, deltaMode);
  const raw = clamp(base + steps, 0, STEP_MAX);
  const keep = owned ? { claim: true, held: wheel.held } : { claim: false, held: -Infinity };
  wheel = burst ? wheel
    : { band, device: name, send: null, known: new Set(), ...keep };
  wheel.known.add(base).add(raw);
  Object.assign(wheel, { raw, rest, at: now });
  // only a notch that moves the Q keeps the page held
  if (steps !== 0 && raw !== base) Object.assign(wheel, { claim: true, held: now });
  // a drag let go of that waits for its answers puts the Q of the wheel after its own
  for (const s of live) {
    if (s.over && s.band === band && s.figure === plot.closest('.eqg')) s.wheelQ = raw;
  }
  if (steps === 0 || raw === base) return;
  pickBand(band);
  const mine = wheel;
  mine.send ??= newest(async (value) => {
    const { core, signal } = ember;
    // behind the answers of a drag let go of just before: its own last Q must not land after
    await Promise.all([...live].filter((s) => s.over && s.band === band && s.device === name)
      .flatMap((s) => s.drags.map((drag) => drag.settled)));
    if (signal.aborted) return null;
    const answer = await core.actions.setBand(band, 'q', { raw: value }, { device: name });
    if (signal.aborted) return null;
    if (!answer.ok) {
      // the Q the page took for the band is not the device's: draw the part again from the model
      for (const s of live) {
        if (s.band === band && s.device === name) s.wheelRefused = true;
      }
      staleTone();
      readWaiting();
      return answer.error ?? 'The change was not made.';
    }
    const cell = document.querySelector(`#part-bands .band[data-band="${band}"]`)
      ?.children[COLUMN.q];
    if (cell && answer.text && shown() === name) {
      cell.textContent = answer.text;
      syncEditor();
    }
    return null;
  }, () => {});
  mine.send(raw);
}

/** Listen for the mouse and the finger on the graph; `given.readWaiting` is called when a drag's
 * swap hold goes. */
export function installToneGraph(given) {
  readWaiting = given.readWaiting;
  document.addEventListener('pointerdown', onDown, scoped());
  document.addEventListener('touchstart', onTouchStart, scoped({ passive: false }));
  document.addEventListener('pointermove', onMove, scoped());
  document.addEventListener('pointerup', onUp, scoped());
  for (const name of ['pointercancel', 'lostpointercapture']) {
    document.addEventListener(name, (event) => {
      if (event.pointerId === session?.pointerId) release();
    }, scoped());
  }
  document.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.('.eqg .plot') && usable()) event.preventDefault();
  }, scoped());
  document.addEventListener('wheel', onWheel, scoped({ passive: false }));
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') session?.drag.cancel('esc');
  }, scoped({ capture: true }));
  window.addEventListener('blur', release, scoped());
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) release();
  }, scoped());
  document.addEventListener('svolder-lost', (event) => {
    if (event.detail) onLost();
  }, scoped());
}
