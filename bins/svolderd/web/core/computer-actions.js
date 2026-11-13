/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a theme does to the computer: the writes of the routes under `api/v1/host/`: its volume,
// mute, EQ mode, preset and points. Each lane (the volume, the mute, the mode, each point) has one
// request in flight with the newest value waiting; a point's live updates are sent at most 20 a
// second and its last value always, at release (`done: true`). What is asked is held (see
// computer.js) until the host's model has been read after the writes. A preset that another device
// plays too is the daemon's question: `ask` puts it to the person and a yes repeats the call with
// `confirm: true`. The preset calls name the device whose list they act on (`options.device`).

import { NO_ANSWER, newest, refusal } from './api.js';
import {
  addedAt, FLAT_POINT, fullAnswer, HAND, idAt, indexOfId, maxPoints, nextFree, pointAt,
} from './computer.js';
import { computerSteps, stepAgainst } from './tone-drag.js';
import { parse, thousandsReading } from './words.js';
import { NO_DEVICE, REFUSED } from './write.js';

const NOT_A_NUMBER = 'Not a number.';

/** What a cell's text means for a field held to `[low, high]`: a number, `null` for none, or `{ error }` when a comma
 * could be a decimal mark or a thousands separator and the range does not tell which (`1,000` in a frequency is 1000,
 * `20,500` is 20.5; `20,000` and `2,000` as a gain are asked again; when neither is in range the decimal one
 * is returned and held to the range like any number out of it). */
function readTyped(typed, low, high) {
  const decimal = parse(typed);
  const thousands = thousandsReading(typed);
  if (decimal === null || thousands === null) return decimal;
  const inside = (x) => x >= low && x <= high;
  if (inside(decimal) !== inside(thousands)) return inside(decimal) ? decimal : thousands;
  if (!inside(decimal)) return decimal;
  const kilo = thousands % 1000 === 0 ? `${thousands / 1000}k` : String(thousands);
  return { error: `Write ${decimal} or ${kilo}.` };
}
/** What a write to a point that moved or is gone says; the page reads the points again. The daemon
 * words that refusal "the point in slot N has moved or is gone" (`locate` in
 * crates/svolder-host/src/eq/control.rs, pinned by a test there); `movedPoint` matches it. */
const POINT_MOVED = 'That point moved. Try again.';
const movedPoint = (answer) => answer.status === 409 && /moved or is gone/.test(answer.error ?? '');
const round = (value, places) => Math.round(value * 10 ** places) / 10 ** places;

/**
 * The computer's actions over `store`, writing with `post`, saying things with `toast`; `held` is
 * the pending values of computer.js, `writes` the core's `createWrites`, `ask` the core's question and
 * `computer` the point in hand (a removal of that point clears it); `timing` replaces the clock.
 */
export function createComputerActions({
  store, post, toast, held, writes, ask = null, computer = null, timing = {},
}) {
  const { turnedAway, finish, target, write } = writes;
  const clock = {
    now: () => globalThis.performance.now(),
    later: (fn, ms) => setTimeout(fn, ms),
    cancel: (id) => clearTimeout(id),
    ...timing,
  };

  /** The newest-waiting writer of one value of the computer (`key` is its hold): the call that
   * starts a run resolves with its last answer, one that only queues resolves `queued`. */
  function lane(key, path, bodyOf) {
    const state = { running: false, last: null };
    state.send = newest(async (value) => {
      const answer = await post(path, bodyOf(value));
      const why = answer.status === 401 ? null : refusal(answer);
      if (why === null) {
        state.last = { ok: answer.status !== 401, ...answer };
        return null;
      }
      state.last = { ok: false, ...answer, error: answer.error || REFUSED };
      return state.last.error;
    }, (why) => {
      toast(why);
      held.drop(key);
      held.resync();
    });
    return async (value) => {
      const away = turnedAway();
      if (away) return away;
      held.set(key, value);
      if (state.running) {
        state.send(value);
        return { ok: true, status: 0, queued: true, value };
      }
      state.running = true;
      state.last = null;
      try {
        await state.send(value);
      } finally {
        state.running = false;
      }
      if (state.last?.ok) held.release(key);
      else held.drop(key);
      return state.last ?? { ok: true, status: 0, value };
    };
  }

  /** What an answer of a structural write (add, remove, preset) means: a refusal is toasted, and
   * the computer's model is read again either way. */
  function structural(answer) {
    held.resync();
    if (answer.status === 401) return { ok: false, ...answer };
    const why = refusal(answer);
    if (why === null) return { ok: true, ...answer };
    toast(why);
    return { ok: false, ...answer, error: why };
  }

  const volumeLane = lane('volume', 'api/v1/host/volume', (pct) => ({ pct }));
  const muteLane = lane('mute', 'api/v1/host/mute', (muted) => ({ muted }));
  const modeLane = lane('mode', 'api/v1/host/eq', (on) => ({ on }));

  /** Per point as the caller numbers it (1-based): the id (slot) of the point it names, found when
   * its first change came and kept until the edit ends, so another front end's re-sort that moves
   * the point to another place cannot send the changes to a neighbour; what waits to be sent, what
   * the drag has asked, who waits for it. */
  const points = new Map();
  const POINT_EVERY_MS = 50;
  const POINT_FIELDS = ['type', 'f', 'g', 'q'];

  // A hold that ended under a drag (a preset was loaded, which renumbers the slots): the point
  // the drag held is another point now, so what it goes on asking is not sent.
  held.listen((type, extra) => {
    if (type !== 'ended') return;
    for (const state of points.values()) {
      if (state.id === null || `point:${state.id}` !== extra.key) continue;
      const { waiters } = state;
      Object.assign(state, {
        id: null, ended: !state.done, fields: {}, values: {}, done: false, waiters: [],
      });
      waiters.forEach((resolve) => resolve({ ok: true, status: 0, gone: true }));
    }
  });

  const modelPoints = () => store.page().host?.computer?.eq?.points ?? [];
  /** Point `n` (1-based) as this viewer sees it: the model's, the values being changed laid over. */
  const currentOf = (n) => ({
    ...(modelPoints()[n - 1] ?? FLAT_POINT),
    ...held.get(`point:${idAt(modelPoints(), n)}`)?.value,
  });

  function pointOf(n) {
    if (!points.has(n)) {
      points.set(n, {
        id: null, ended: false, fields: {}, done: false, values: {}, waiters: [], inflight: false,
        last: -Infinity,
        timer: null,
      });
    }
    return points.get(n);
  }

  /** Send what waits for point `n`, if nothing is in flight and the pace allows; else wait. */
  function pump(n) {
    const state = pointOf(n);
    if (state.inflight) return;
    const pending = Object.keys(state.fields).length > 0;
    if (!pending && !state.done) return;
    const wait = state.done ? 0 : Math.max(0, state.last + POINT_EVERY_MS - clock.now());
    if (wait > 0) {
      state.timer ??= clock.later(() => {
        state.timer = null;
        pump(n);
      }, wait);
      return;
    }
    if (state.timer !== null) {
      clock.cancel(state.timer);
      state.timer = null;
    }
    const at = indexOfId(modelPoints(), state.id ?? n);
    if (at === null) {
      // the point is gone (another front end removed it): the edit ends, writing nothing, and what the drag
      // goes on asking until its release lands on no point
      const { waiters: waiting, done: last } = state;
      held.drop(`point:${state.id}`);
      Object.assign(state, {
        id: null, ended: !last, fields: {}, values: {}, done: false, waiters: [],
      });
      waiting.forEach((resolve) => resolve({ ok: true, status: 0, gone: true }));
      return;
    }
    const body = pending ? state.fields : { ...state.values };
    const { done, waiters } = state;
    state.fields = {};
    state.done = false;
    state.waiters = [];
    state.inflight = true;
    state.last = clock.now();
    // the point is named by its slot, which it keeps whatever place the table lists it in, so a
    // re-sort the page has not read yet cannot send the write to another point
    const slot = modelPoints()[at - 1]?.slot;
    sendPoint(at, n, state, slot === undefined ? { ...body, done } : { ...body, done, slot }, done, waiters);
  }

  async function sendPoint(at, n, state, body, done, waiters) {
    let result;
    try {
      const answer = await post(`api/v1/host/eq/points/${at}`, body);
      const why = answer.status === 401 ? null : refusal(answer);
      if (why === null) {
        result = { ok: answer.status !== 401, ...answer };
      } else {
        const moved = movedPoint(answer);
        const said = moved ? POINT_MOVED : why;
        toast(said);
        result = { ok: false, ...answer, error: said, moved };
      }
    } catch {
      result = { ok: false, status: 0, error: NO_ANSWER };
    }
    state.inflight = false;
    waiters.forEach((resolve) => resolve(result));
    if (!result.ok) {
      const rest = state.waiters;
      const id = state.id ?? n;
      // a point that moved or went ends the edit: what the hand goes on asking is not sent
      Object.assign(state, {
        id: null, ended: Boolean(result.moved) && !done, fields: {}, done: false, values: {}, waiters: [],
      });
      rest.forEach((resolve) => resolve(result));
      held.drop(`point:${id}`);
      held.resync();
      return;
    }
    if (Object.keys(state.fields).length === 0 && !state.done) {
      if (done) {
        const id = state.id ?? n;
        state.values = {};
        state.id = null;
        held.release(`point:${id}`);
      }
    } else {
      pump(n);
    }
  }

  /** A preset of a device's list was changed: a success has the computer read its list
   * again. */
  function presetChanged(answer) {
    if (answer.ok) held.resync();
    return answer;
  }

  const computerActions = {
    /** Point `n` (1-based) as this viewer sees it: the model's, or the flat point when the list
     * has none, with the values being changed laid over. */
    current: (n) => currentOf(n),

    /** Ask for the volume `pct`: rounded, held to 0 to 100 (never a boost), one request at a time
     * with the newest waiting. */
    setVolume(pct) {
      const step = Math.round(Number(pct));
      if (!Number.isFinite(step)) return Promise.resolve({ ok: false, status: 0, error: REFUSED });
      return volumeLane(Math.min(100, Math.max(0, step)));
    },

    /** One step (`+1` or `-1`) from the volume as drawn. A sink another program boosted above 100
     * is not raised further, and a step down writes 100. Nothing is written before the first
     * read. */
    async step(delta) {
      const now = held.get('volume')?.value ?? store.page().host?.computer?.volume?.pct ?? null;
      if (now === null) return { ok: false, status: 0, inert: true };
      if (now > 100) {
        if (delta > 0) return { ok: true, status: 0, noop: true };
        return computerActions.setVolume(100);
      }
      const next = Math.min(100, Math.max(0, now + (delta > 0 ? 1 : -1)));
      if (next === now) return { ok: true, status: 0, noop: true };
      return computerActions.setVolume(next);
    },

    /** A step of `delta` from the volume as drawn: one step (`+1`, `-1`) is `step`'s; a larger one
     * (a page key) is the volume as drawn plus it, held to 0 to 100. Nothing is written before the
     * first read. */
    async nudge(delta) {
      if (Math.abs(delta) === 1) return computerActions.step(delta);
      const away = turnedAway();
      if (away) return away;
      const at = held.get('volume')?.value ?? store.page().host?.computer?.volume?.pct ?? null;
      if (!Number.isFinite(delta)) return { ok: false, status: 0, error: REFUSED };
      if (at === null) return { ok: false, status: 0, inert: true };
      return computerActions.setVolume(at + delta);
    },

    setMute: (muted) => muteLane(Boolean(muted)),

    /** Mute, or unmute when it is muted as drawn. A guest or a read-only page is turned away with
     * its words; nothing is written before the volume's first read. */
    async toggleMute() {
      const away = turnedAway();
      if (away) return away;
      const read = store.page().host?.computer?.volume;
      if (!read && held.get('volume') === null) return { ok: false, status: 0, inert: true };
      const muted = held.get('mute')?.value ?? read?.muted ?? false;
      return muteLane(!muted);
    },
    setMode: (mode) => modeLane(mode === 'eq'),

    /** Recall the preset `name` on the computer. When a connected device plays it too the daemon
     * hands back a question: it is asked, a yes flattens that device and applies (`confirm`), a no
     * sends nothing (`declined: true`). `asker(question)` puts the question in the theme's own words
     * and resolves true or false (the core's card by default). */
    async recallPreset(name, { confirm = false, asker = null, device } = {}) {
      const away = turnedAway();
      if (away) return away;
      // the daemon looks the device up by its name, not by the segment of an address
      let named = device;
      try {
        named = device && decodeURIComponent(device);
      } catch {
        named = device;
      }
      const body = {
        name, ...(named ? { device: named } : {}), ...(confirm ? { confirm: true } : {}),
      };
      const answer = await post('api/v1/host/eq/preset', body);
      const question = answer.status === 409 && answer.ask?.kind === 'both_sides';
      const askIt = asker ?? ask;
      if (question && !confirm && askIt) {
        if (!(await askIt(answer.ask.question))) {
          return { ok: false, status: 409, declined: true, ask: answer.ask };
        }
        const done = await computerActions.recallPreset(name, { confirm: true, device });
        if (done.ok && device) store.refresh(device);
        return done;
      }
      return structural(answer);
    },

    /** Rename the preset `from` of the device's list; the computer's list is read again. */
    async renamePreset(from, to, options) {
      return presetChanged(await write('presets/rename', { from, to }, options, REFUSED));
    },

    /** Delete the preset `name` of the device's list; the computer's list is read again. */
    async deletePreset(name, options) {
      return presetChanged(await write('presets/delete', { name }, options, REFUSED));
    },

    /** Save the computer's EQ as a preset of the device's list (`from: computer`). */
    async savePreset(name, options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, error: NO_DEVICE };
      const done = finish(url, await post(`api/v1/devices/${url}/presets`,
        { name, from: 'computer' }));
      held.resync();
      return done;
    },

    /** Add the point `{ type, f, g, q }` (type PK, LSC or HSC). */
    async addPoint(point) {
      const away = turnedAway();
      if (away) return away;
      const body = {
        type: String(point?.type ?? 'PK').toUpperCase(), f: point?.f, g: point?.g, q: point?.q,
      };
      const points = modelPoints();
      const answer = structural(await post('api/v1/host/eq/points', body));
      // the new point is listed with the others by frequency
      return answer.ok ? { ...answer, n: addedAt(points, Number(body.f)) } : answer;
    },

    /** `+ Add point`: a PK of Q 1 and 0 dB in the widest gap between the points. A full table adds
     * nothing and says so (`full: true`, the `max`, the words "N is the most"); a computer with
     * no EQ adds nothing. */
    async addNext() {
      const away = turnedAway();
      if (away) return away;
      const eq = store.page().host?.computer?.eq;
      if (!eq) return { ok: false, status: 0, error: REFUSED, none: true };
      const count = Math.max(eq.points?.length ?? 0, 1);
      const max = maxPoints(eq);
      if (count >= max) return fullAnswer(max);
      const listed = Array.from({ length: count }, (_, i) => currentOf(i + 1));
      return computerActions.addPoint(pointAt(nextFree(listed), 0));
    },

    /** Remove point `n` (1-based). Once it is removed, when it was the point in hand none is, and a
     * point in hand listed after it is the one before (`computer.repick` says so). */
    async removePoint(n) {
      const away = turnedAway();
      if (away) return away;
      if (!Number.isInteger(n) || n < 1) return { ok: false, status: 0, error: REFUSED };
      const slot = modelPoints()[n - 1]?.slot;
      const answer = await post(`api/v1/host/eq/points/${n}/remove`, slot === undefined ? {} : { slot });
      const result = structural(movedPoint(answer) ? { ...answer, error: POINT_MOVED, moved: true } : answer);
      // the point in hand goes with it, and one listed after it is listed one place higher
      const at = result.ok ? computer?.picked() ?? null : null;
      if (at === n) computer.repick(null);
      else if (at !== null && at > n) computer.repick(at - 1);
      return result;
    },

    /**
     * What was typed in the cell of point `n`'s `param` (`typed`; `shown` is the text the cell
     * showed before it was edited). Not a number is toasted and writes nothing; so does a value
     * equal to the cell's own text or to the point's current value before it is held to the hand
     * range (a point beyond the range is not moved by retyping its number); the value is first
     * rounded to the places `HAND` gives its field. Otherwise it, held to the hand range, is
     * written as a last value.
     */
    async cell(n, param, typed, shown) {
      const away = turnedAway();
      if (away) return away;
      if (!Number.isInteger(n) || n < 1 || !HAND[param]) return { ok: false, status: 0, error: REFUSED };
      const [low, high, places] = HAND[param];
      const value = readTyped(typed, low, high);
      if (value === null) {
        toast(NOT_A_NUMBER);
        return { ok: false, status: 0, error: NOT_A_NUMBER, invalid: true };
      }
      if (typeof value === 'object') {
        toast(value.error);
        return { ok: false, status: 0, error: value.error, invalid: true };
      }
      const typedValue = round(value, places);
      const current = currentOf(n)[param];
      if (typedValue === parse(shown ?? '') || typedValue === current) {
        return { ok: true, status: 0, nothing: true };
      }
      return computerActions.setPoint(n, { [param]: Math.min(high, Math.max(low, typedValue)) },
        { done: true });
    },

    /**
     * One step of the hand's grid on point `n`'s `param`, up (`delta` 1) or down (`-1`), written as
     * a last value. A step never goes against its key: a point beyond the hand range stays where
     * it is (`nothing: true`).
     */
    stepPoint(n, param, delta) {
      const away = turnedAway();
      if (away) return Promise.resolve(away);
      if (!Number.isInteger(n) || n < 1 || !HAND[param]) {
        return Promise.resolve({ ok: false, status: 0, error: REFUSED });
      }
      const next = stepAgainst(computerSteps(), currentOf(n), param, delta);
      if (next === null) return Promise.resolve({ ok: true, status: 0, nothing: true });
      return computerActions.setPoint(n, { [param]: next }, { done: true });
    },

    /**
     * Change point `n` (1-based): `fields` is any of `type`, `f`, `g`, `q`. With `done: false` (a
     * drag in motion) the update is sent at most 20 a second per point, the newest values
     * replacing the ones waiting, and the call resolves with the answer of the request that
     * carried them; with `done: true` it is sent at once after the request in flight, and with no
     * fields it sends what the drag moved once more as the last value (a point that was only
     * tapped has moved nothing and sends nothing).
     */
    setPoint(n, fields = {}, { done = false } = {}) {
      if (!Number.isInteger(n) || n < 1) {
        return Promise.resolve({ ok: false, status: 0, error: REFUSED });
      }
      const away = turnedAway();
      if (away) return Promise.resolve(away);
      const state = pointOf(n);
      if (state.ended) {
        if (done) state.ended = false;
        return Promise.resolve({ ok: true, status: 0, gone: true });
      }
      const given = Object.entries(fields).filter(([key]) => POINT_FIELDS.includes(key));
      if (given.length > 0) state.id ??= idAt(modelPoints(), n);
      for (const [key, value] of given) {
        state.fields[key] = value;
        state.values[key] = value;
      }
      if (given.length > 0) held.set(`point:${state.id}`, { ...state.values });
      const moved = Object.keys(state.values).length > 0;
      if (!done && given.length === 0) return Promise.resolve({ ok: true, status: 0, queued: true });
      if (done && !moved) return Promise.resolve({ ok: true, status: 0, nothing: true });
      if (done) state.done = true;
      return new Promise((resolve) => {
        state.waiters.push(resolve);
        pump(n);
      });
    },
  };

  return computerActions;
}
