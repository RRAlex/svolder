/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone curve's gestures as a model the page's wiring drives and node drives alike: which
// band's point a press grabs, what a drag, a right drag, a pinch and the wheel ask of the band, and
// how the writes go out. Plain numbers and callbacks in, no DOM and no clock: the pointer's place
// is given in CSS px within the plot, the tables of the band's steps and the writes are passed in.
//
// A left drag puts the point where the pointer is, each axis staying where the press found it until
// the pointer has gone AXIS_HOLD_PX along it, and the gain sticking at the band's 0 dB step while
// the pointer is within `zeroHoldPx` (ZERO_HOLD_PX unless given, 0 for none) of the 0 dB line; the
// pointer's place is turned into a frequency and a gain by `map` (`plotMap` unless given: the
// curve's own plot, so any drawing can feed the drag); a right drag sets the Q by how far the
// pointer has gone sideways (halved every Q_HALVES_PX to the right), a pinch by how far apart the
// fingers are, the wheel one device step a notch. Every value lands on the table's nearest step, so
// what is drawn is what is written, and the map from pixels to values goes on past the plot's
// edges. Writes go out as the pointer moves, one in flight per parameter and the newest waiting
// (`newest` of api.js); a drag that ends keeps its last write, except one whose stream was lost or
// whose write was refused, which writes nothing more.

import { MODE_SET_WORDS, newest } from './api.js';
import { dbAt, hzAt, yOf } from './tone-curve.js';

// How far from a band's point a press grabs it, in CSS px; the wheel starts a burst from further,
// as a hand rolling a wheel is not as steady as one holding a button.
export const REACH = { mouse: 12, touch: 24, wheel: 20 };
// How far the pointer goes along an axis before that axis follows it, in CSS px.
export const AXIS_HOLD_PX = 8;
// How far from the 0 dB line a drag's gain sticks at 0 dB, in CSS px (at most the px of 1 dB).
export const ZERO_HOLD_PX = 10;
// The right drag's travel that halves the Q, in CSS px.
export const Q_HALVES_PX = 120;
// The pixel delta that makes one wheel step (a trackpad's deltas add up to it).
export const WHEEL_STEP_PX = 40;
// The gap between the dragged point and its label, in CSS px.
export const LABEL_GAP_PX = 10;

const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;

// The curve's plot as a map from the pointer's place (CSS px within `plot`, { width, height }) to
// values: the frequency at an x, the gain at a y, and the y of a gain.
export function plotMap(plot) {
  return {
    hzAt: (x) => hzAt(x / plot.width * VIEW_WIDTH),
    dbAt: (y) => dbAt(y / plot.height * VIEW_HEIGHT),
    yOfDb: (db) => yOf(db) / VIEW_HEIGHT * plot.height,
  };
}

// The band number whose point a press at `at` grabs, or null. Of the points within `reach` the
// nearer wins, and on a tie the band `picked`, then the lower. A `wide` press (a right press or a
// pinch) with none in reach takes the band nearest in x, on the same tie rules.
export function grab(points, at, { reach, wide = false, picked = null }) {
  const nearest = (distanceOf, limit) => {
    let best = null;
    let tied = [];
    for (const point of points) {
      const distance = distanceOf(point);
      if (!(distance <= limit)) continue;
      if (best === null || distance < best) {
        best = distance;
        tied = [point.band];
      } else if (distance === best) {
        tied.push(point.band);
      }
    }
    if (tied.length === 0) return null;
    return tied.includes(picked) ? picked : Math.min(...tied);
  };
  const near = nearest((p) => (p.x - at.x) ** 2 + (p.y - at.y) ** 2, reach ** 2);
  if (near !== null || !wide) return near;
  return nearest((p) => Math.abs(p.x - at.x), Infinity);
}

// One wheel event: how many device steps it makes and the pixels left over. `steps` > 0 narrows
// the bell (the Q's raw step up): the wheel rolled up, deltaY below zero. A mouse's notch comes as
// a pixel delta of 50 to 120 and is one step whatever its size (the rest is dropped); smaller
// pixel deltas (a trackpad's) add up, WHEEL_STEP_PX to a step, `rest` carrying the remainder to
// the next event; in lines or pages (deltaMode 1 and 2) each event is one step.
export function wheelSteps(rest, deltaY, deltaMode) {
  if (deltaMode !== 0) return { steps: -Math.sign(deltaY) || 0, rest };
  if (Math.abs(deltaY) >= WHEEL_STEP_PX) return { steps: -Math.sign(deltaY), rest: 0 };
  const total = rest + deltaY;
  const whole = Math.trunc(total / WHEEL_STEP_PX);
  return { steps: -whole || 0, rest: total - whole * WHEEL_STEP_PX };
}

// Where a label of `size` goes: centred above the point, kept inside the plot.
export function labelAt(point, plot, size) {
  const within = (at, extent, room) => Math.max(0, Math.min(at, room - extent));
  return {
    x: within(point.x - size.width / 2, size.width, plot.width),
    y: within(point.y - LABEL_GAP_PX - size.height, size.height, plot.height),
  };
}

// The index of the entry of `table` nearest `value` under `distance`, the lowest on a tie, or
// `fallback` where no entry is (a value that is not a number).
function nearestStep(table, value, distance, fallback) {
  let best = -1;
  let least = Infinity;
  table.forEach((entry, i) => {
    const d = distance(entry, value);
    if (d < least) {
      best = i;
      least = d;
    }
  });
  return best < 0 ? fallback : best;
}

const byDistance = (entry, value) => Math.abs(entry - value);
const byLogDistance = (entry, value) => Math.abs(Math.log(entry / value));

// One drag of one band's point: `gesture` is 'move' (frequency and gain), 'width' (a right drag's
// Q) or 'pinch' (two fingers' Q). `start` is the raw steps the band reads, `press` where the
// pointer went down (or the fingers' distance), `steps` the band's tables, `write(param, raw)`
// sends one write and answers { text, refused, modeSetToEq }, and `on` hears: held(raws) when the
// values asked change, answer(param, text) for each write's words, toast(words), and ended() once.
// `map` ({ hzAt, dbAt, yOfDb }, in the same px as `press`) and `zeroHoldPx` are the drawing's own;
// the defaults are the curve's plot.
export function createDrag({
  band, gesture, start, press, plot, steps, write, on, map = plotMap(plot), zeroHoldPx,
}) {
  const held = { f: start.f, g: start.g, q: start.q };
  const asked = { ...held };
  const free = { x: false, y: false };
  const values = { q0: steps.q[start.q] };
  // The values last sent per parameter, and how many sends have not settled (with the callers
  // waiting for none).
  const sent = {};
  let unsettled = 0;
  let idle = [];
  let ended = false;
  let stopped = false;
  let toasted = false;
  let refusedOnce = false;

  const end = () => {
    if (ended) return;
    ended = true;
    on.ended();
  };

  const refuse = (words) => {
    if (refusedOnce) return;
    refusedOnce = true;
    stopped = true;
    on.toast(words);
    end();
  };

  const senders = Object.fromEntries(['f', 'g', 'q'].map((param) => [param, newest(async (raw) => {
    if (stopped) return null;
    // the pointer went back to the value already on its way: it is not written twice
    if (raw === sent[param]) return null;
    sent[param] = raw;
    const answer = await write(param, raw);
    if (answer?.refused) return answer.refused;
    if (answer?.text !== undefined) on.answer(param, answer.text);
    if (answer?.modeSetToEq && !toasted) {
      toasted = true;
      on.toast(MODE_SET_WORDS);
    }
    return null;
  }, refuse)]));

  // Every send is counted and its promise settles when its write has (one that only replaced a
  // waiting value settles at once), so the count is above 0 for as long as a write is in flight
  // or waiting, whatever the order the promises settle in.
  const send = (param, raw) => {
    unsettled++;
    const done = () => {
      unsettled--;
      if (unsettled === 0) {
        const waiters = idle;
        idle = [];
        waiters.forEach((resolve) => resolve());
      }
    };
    senders[param](raw).then(done, done);
  };

  // Ask for `targets` ({ param: raw }): hold what changed, tell the page, send what is new.
  const ask = (targets) => {
    const changed = Object.entries(targets).filter(([param, raw]) => raw !== asked[param]);
    if (changed.length === 0) return;
    for (const [param, raw] of changed) {
      asked[param] = raw;
      held[param] = raw;
    }
    on.held({ ...held });
    for (const [param, raw] of changed) send(param, raw);
  };

  const inPlot = plot.width > 0 && plot.height > 0;
  // The band's 0 dB step (none: no zone), the y the plot draws 0 dB at and the zone's half width.
  const zeroStep = steps.g.indexOf(0);
  const zeroY = map.yOfDb(0);
  const zeroHold = zeroHoldPx ?? Math.min(ZERO_HOLD_PX, Math.abs(map.yOfDb(0) - map.yOfDb(1)));

  return {
    // The raw steps the drag asks of the band now.
    get held() { return { ...held }; },
    // Resolves once every write sent has been answered (a fresh promise each time).
    get settled() {
      return new Promise((resolve) => {
        if (unsettled === 0) resolve();
        else idle.push(resolve);
      });
    },
    // The pointer is at `at` (CSS px in the plot, beyond it too).
    move(at) {
      if (ended || !inPlot) return;
      if (gesture === 'move') {
        if (Math.abs(at.x - press.x) >= AXIS_HOLD_PX) free.x = true;
        if (Math.abs(at.y - press.y) >= AXIS_HOLD_PX) free.y = true;
        const targets = {};
        if (free.x) {
          const hz = map.hzAt(at.x);
          targets.f = nearestStep(steps.f, hz, byLogDistance, held.f);
        } else {
          targets.f = held.f;
        }
        if (free.y && zeroStep >= 0 && zeroHold > 0 && Math.abs(at.y - zeroY) <= zeroHold) {
          targets.g = zeroStep;
        } else if (free.y) {
          const db = map.dbAt(at.y);
          targets.g = nearestStep(steps.g, db, byDistance, held.g);
        } else {
          targets.g = held.g;
        }
        ask(targets);
      } else if (gesture === 'width') {
        const q = values.q0 * 2 ** (-(at.x - press.x) / Q_HALVES_PX);
        ask({ q: nearestStep(steps.q, q, byDistance, held.q) });
      }
    },
    // The fingers are `distance` apart: twice as far as at the press halves the Q.
    spread(distance) {
      if (ended || gesture !== 'pinch' || !(distance > 0) || !(press.distance > 0)) return;
      const q = values.q0 * press.distance / distance;
      ask({ q: nearestStep(steps.q, q, byDistance, held.q) });
    },
    // The button went up or the fingers lifted: what was asked is written (or is on its way).
    release() { end(); },
    // The drag ends where it is: 'esc' and 'switch' keep what was asked, 'lost' (the stream) writes
    // nothing more, a write still waiting being dropped.
    cancel(why) {
      if (why === 'lost') stopped = true;
      end();
    },
    band,
  };
}

/** The parameter a drag's key names, as the actions write it. */
export const BAND_PARAM = { f: 'frequency', g: 'gain', q: 'q' };

/** The steps of band `band` (1-based) out of a device's step tables (`tables`): the frequencies
 * of that band (a step that is a word, not a number, is NaN), and the gain and Q tables. */
export function bandSteps(tables, band) {
  return {
    f: tables.bands[band - 1].f.map(([v]) => (typeof v === 'number' ? v : NaN)),
    g: tables.gain.map(([v]) => v),
    q: tables.q.map(([v]) => v),
  };
}

/**
 * The `write(param, raw)` of a drag of band `band` on `device`, over `actions`: it waits for the
 * promises `before()` lists (the drags before this one, whose last values must not land after
 * this one's), writes nothing once `aborted()`, and answers what createDrag asks of a write:
 * `{ text, modeSetToEq }`, `{ refused }` with the words of a refusal, or `{}` when it was dropped.
 */
export function bandWriter({ actions, band, device, before = () => [], aborted = () => false }) {
  return async (param, raw) => {
    await Promise.all(before());
    if (aborted()) return {};
    const answer = await actions.setBand(band, BAND_PARAM[param], { raw }, { device });
    if (aborted()) return {};
    if (!answer.ok) return { refused: answer.error };
    return { text: answer.text, modeSetToEq: answer.mode_set_to_eq };
  };
}

const round = (value, places) => Math.round(value * 10 ** places) / 10 ** places;

let computerTables = null;

/**
 * The steps of a point of the computer's EQ, in `bandSteps`' shape (`{ f, g, q }`, each a table of
 * values a raw step indexes): the frequency on a 1/24-octave grid from 20 Hz to 20 kHz (the last
 * step is 20 kHz itself), the gain every 0.1 dB from −12 to +12, the Q on a 1/24-decade grid from
 * 0.1 to 10. These are the hand's ranges: a value a preset holds outside them is played as written
 * and sits at the nearest step until a hand moves that field.
 */
export function computerSteps() {
  if (computerTables) return computerTables;
  const f = Array.from({ length: 240 }, (_, k) => round(20 * 2 ** (k / 24), 3));
  if (f.at(-1) < 20000) f.push(20000);
  const g = Array.from({ length: 241 }, (_, i) => (i - 120) / 10);
  const q = Array.from({ length: 49 }, (_, k) => round(0.1 * 10 ** (k / 24), 3));
  computerTables = { f, g, q };
  return computerTables;
}

/** The raw steps of `tables` nearest the point `{ f, g, q }`: where a drag of it starts. */
export function startOf(tables, point) {
  const near = (table, value, distance) => {
    let best = 0;
    let least = Infinity;
    table.forEach((entry, i) => {
      const d = distance(entry, value);
      if (d < least) {
        best = i;
        least = d;
      }
    });
    return best;
  };
  return {
    f: near(tables.f, point.f, byLogDistance),
    g: near(tables.g, point.g, byDistance),
    q: near(tables.q, point.q, byDistance),
  };
}

/**
 * The `write(param, raw)` of a drag of point `n` (1-based) of the computer's EQ, over `actions`:
 * the raw step is turned into the value of `steps` (the computer's by default) and sent as a live
 * update (`done: false`); the writer's `release()` sends what was moved once more, as the last
 * value. A point that was only tapped has moved nothing and sends nothing. Answers what createDrag
 * asks of a write: `{ refused }` with the words of a refusal, or `{}`.
 */
export function pointWriter({ actions, n, steps = computerSteps() }) {
  const write = async (param, raw) => {
    const value = steps[param][raw];
    if (value === undefined) return {};
    const answer = await actions.computer.setPoint(n, { [param]: value }, { done: false });
    return answer.ok || answer.queued ? {} : { refused: answer.error };
  };
  write.release = () => actions.computer.setPoint(n, {}, { done: true });
  return write;
}
