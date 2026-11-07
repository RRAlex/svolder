/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a theme does to the device: every write svolder takes, as one call. A guest's or a
// read-only page's call sends nothing and says why; a refused write is toasted with the daemon's
// words and the model read again, so what the device holds is shown, not what was chosen; an
// answer's own toast is said and its parts are applied to the store. Every device call takes an
// optional last `{ device }` naming a followed device (the shown one by default) and resolves
// `{ ok, status, error?, ...answer }`.
//
// The volume is written one at a time, the newest waiting, held to the cap, and a raise is not
// sent while the stream is lost or waking (the writer is newest-wins too, and enforces the
// volume rule; this is the page's half of it).
//
// `actions.computer` is the same for the computer's own controls (the routes under
// `api/v1/host/`): its volume, mute, EQ mode, preset and points. Each lane (the volume, the mute,
// the mode, each point) has one request in flight with the newest value waiting; a point's live
// updates are sent at most 20 a second and its last value always, at release (`done: true`). What
// is asked is held (see computer.js) until the host's model has been read after the writes. A
// preset that another device plays too is the daemon's question: `ask` puts it to the person and a
// yes repeats the call with `confirm: true`.

import { NO_ANSWER, newest, refusal } from './api.js';
import {
  addedAt, createHolds, idAt, indexOfId,
} from './computer.js';
import { identifyWords } from './words.js';

/** The words a guest's call is turned back with, and where its button leads. */
const SIGN_IN_WORDS = 'Sign in to change this';
const SIGN_IN = { label: 'Sign in', href: './?sign-in' };
/** The words of a page whose writes are off, when the model gave none. */
const READ_ONLY_WORDS = 'Read-only: [web] writes is off';
/** The words of a refused write that gave none: the page's own writes, and the ones posted by hand. */
const NOT_MADE = 'The change was not made.';
const REFUSED = 'Refused.';
/** What a write to a point that moved or is gone says; the page reads the points again. The daemon
 * words that refusal "the point in slot N has moved or is gone" (`locate` in
 * crates/svolder-host/src/eq/control.rs, pinned by a test there); `movedPoint` matches it. */
const POINT_MOVED = 'That point moved. Try again.';
const movedPoint = (answer) => answer.status === 409 && /moved or is gone/.test(answer.error ?? '');
const NO_DEVICE = 'No device is shown.';
const NOT_SHOWN = 'That device could not be shown.';

/** The actions over `store`, writing with `post` (api.js's), saying things with `toast`, moving the
 * address with `history` and loading pages with `location`. */
export function createActions({
  store, post, toast, history, location, holds = null, ask = null, timing = {},
}) {
  const held = holds ?? createHolds({ store, fetch: async () => ({ status: 0 }) });
  const clock = {
    now: () => globalThis.performance.now(),
    later: (fn, ms) => setTimeout(fn, ms),
    cancel: (id) => clearTimeout(id),
    ...timing,
  };
  /** Per device: the volume's sender, whether a run is going, the last answer, the stop's promise. */
  const volumes = new Map();
  let showing = 0;

  /** Turn a call away before any request when the page may not write; the answer to give, or
   * nothing when it may. */
  function turnedAway() {
    const viewer = store.page().viewer ?? {};
    if (viewer.guest) {
      toast(SIGN_IN_WORDS, SIGN_IN);
      return { ok: false, status: 0, error: SIGN_IN_WORDS };
    }
    if (viewer.writes === false) {
      const why = viewer.read_only || READ_ONLY_WORDS;
      toast(why);
      return { ok: false, status: 0, error: why };
    }
    return null;
  }

  /** The device a call is for, or the answer of one that has none. */
  const target = (options) => options?.device ?? store.shown();

  /**
   * What an answer means for the page: a refusal is toasted (`fallback` when it has no words) and
   * the device's model read again; success applies the answer's parts and says its toast.
   */
  function finish(url, answer, fallback = NOT_MADE) {
    if (answer.status === 401) return { ok: false, ...answer };
    if (refusal(answer) !== null) {
      const words = answer.error || fallback;
      toast(words);
      if (url) store.refresh(url);
      return { ok: false, ...answer, error: words };
    }
    if (url && answer.parts) store.applyParts(url, answer.parts);
    if (answer.toast) toast(answer.toast);
    return { ok: true, ...answer };
  }

  /** A write to the device `options` names: turned away, or posted to `path` and finished. */
  async function write(path, body, options, fallback) {
    const away = turnedAway();
    if (away) return away;
    const url = target(options);
    if (!url) return { ok: false, status: 0, error: NO_DEVICE };
    return finish(url, await post(`api/v1/devices/${url}/${path}`, body), fallback);
  }

  /** The volume's state for a device, made when first asked for. */
  function volumeOf(url) {
    let state = volumes.get(url);
    if (state) return state;
    state = { running: false, last: null, stopped: Promise.resolve() };
    state.send = newest(async (raw) => {
      // A touch's volume/stop, which every write after it waits for.
      await state.stopped;
      const held = store.part('volume', url);
      // A raise waiting behind a write goes out only while the page is live: a page that was
      // lost, or is waking, or has since switched to another device, has no gesture behind it. A
      // lowering of a device the page still reads always goes.
      const live = store.state() === 'open' && store.device(url) !== null;
      if (!live && (held?.raw === null || held?.raw === undefined || raw > held.raw)) {
        state.last = { ok: false, status: 0, dropped: true, raw };
        return null;
      }
      const answer = await post(`api/v1/devices/${url}/volume`, { raw });
      const why = answer.status === 401 ? null : refusal(answer);
      if (why === null) {
        state.last = finish(url, answer);
        return null;
      }
      state.last = { ok: false, ...answer, error: answer.error || REFUSED };
      return state.last.error;
    }, (why) => {
      // What waited behind the refused write is dropped by the sender; the person is told, and
      // the device's volume is read again.
      toast(why);
      store.refresh(url);
    });
    volumes.set(url, state);
    return state;
  }


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

  const computerActions = {
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

    setMute: (muted) => muteLane(Boolean(muted)),
    setMode: (mode) => modeLane(mode === 'eq'),

    /** Recall the preset `name` on the computer. When a connected device plays it too the daemon
     * hands back a question: it is asked, a yes flattens that device and applies (`confirm`), a no
     * sends nothing (`declined: true`). `asker(question)` puts the question in the theme's own words
     * and resolves true or false (the core's card by default). */
    async recallPreset(name, { confirm = false, asker = null } = {}) {
      const away = turnedAway();
      if (away) return away;
      const device = store.shown();
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
        const done = await computerActions.recallPreset(name, { confirm: true });
        if (done.ok && device) store.refresh(device);
        return done;
      }
      return structural(answer);
    },

    /** Save the computer's EQ as a preset of the shown device's list (`from: computer`). */
    async savePreset(name) {
      const away = turnedAway();
      if (away) return away;
      const url = store.shown();
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

    /** Remove point `n` (1-based). */
    async removePoint(n) {
      const away = turnedAway();
      if (away) return away;
      if (!Number.isInteger(n) || n < 1) return { ok: false, status: 0, error: REFUSED };
      const slot = modelPoints()[n - 1]?.slot;
      const answer = await post(`api/v1/host/eq/points/${n}/remove`, slot === undefined ? {} : { slot });
      return structural(movedPoint(answer) ? { ...answer, error: POINT_MOVED } : answer);
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

  const actions = {
    computer: computerActions,

    /** Ask for volume step `raw`: rounded, held to the device's range and cap, written one at a
     * time with the newest waiting. The call that starts a run resolves with the last write's
     * answer (`landed` says the step the daemon chose, `ok: false, dropped: true` a raise that was
     * not sent because the page was not live); one that only queues resolves `queued`. */
    async setVolume(raw, options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, error: NO_DEVICE };
      const held = store.part('volume', url);
      let step = Math.round(raw);
      if (held) {
        if (Number.isFinite(held.hi)) step = Math.min(held.hi, step);
        if (Number.isFinite(held.lo)) step = Math.max(held.lo, step);
        if (held.cap !== null && held.cap !== undefined && step > held.cap) step = held.cap;
      }
      const state = volumeOf(url);
      if (state.running) {
        state.send(step);
        return { ok: true, status: 0, queued: true, raw: step };
      }
      state.running = true;
      state.last = null;
      try {
        await state.send(step);
      } finally {
        state.running = false;
      }
      return state.last ?? { ok: true, status: 0, raw: step };
    },

    /** A touch on the dial ends a climb: the daemon is told, and the volume writes after it wait
     * for its answer. */
    async stopClimb(options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, error: NO_DEVICE };
      const state = volumeOf(url);
      const sent = post(`api/v1/devices/${url}/volume/stop`, {});
      state.stopped = sent.catch(() => {});
      return finish(url, await sent, REFUSED);
    },

    setControl: (key, body, options) => write(`controls/${key}`, body, options),
    setMode: (mode, options) => write('eq/mode', { mode }, options),
    setBand: (band, param, body, options) => write(`eq/bands/${band}`, { param, ...body }, options,
      REFUSED),
    setLoudness: (body, options) => write('eq/loudness', body, options, REFUSED),
    recallPreset: (name, options) => write('presets/recall', { name }, options),
    savePreset: (name, options) => write('presets', { name }, options),
    renamePreset: (from, to, options) => write('presets/rename', { from, to }, options),
    deletePreset: (name, options) => write('presets/delete', { name }, options),
    recallSlot: (slot, options) => write('device-presets/recall', { slot }, options),
    saveSlot: (slot, confirmed, options) => write('device-presets/save',
      { slot, confirm: Boolean(confirmed) }, options),
    applySetting: (name, options) => write('device-presets/apply', { name }, options),
    connect: (options) => write('connect', {}, options, 'The device did not answer.'),

    /** The Identify cue; the toast is its words, with the gap the device's facts carry. */
    async identify(options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, error: NO_DEVICE };
      const answer = await post(`api/v1/devices/${url}/identify`, {});
      if (answer.status !== 401) {
        const gap = store.part('facts', url)?.identify_gap_secs ?? 2;
        toast(identifyWords(answer, String(gap)));
      }
      return { ok: answer.status === 200, ...answer };
    },

    /** A transport button of the host's player. */
    async press(button) {
      const away = turnedAway();
      if (away) return away;
      return finish(null, await post('api/v1/host/press', { button }), REFUSED);
    },

    /** End this browser's sign-in and load the page as a visitor. */
    async signOut() {
      const answer = await post('logout', {});
      const why = refusal(answer);
      if (why) {
        toast(why);
        return { ok: false, ...answer, error: why };
      }
      location.assign('./');
      return { ok: true, ...answer };
    },

    /** Say that a change needs a sign-in, with the way to one (a tap on an inert control). */
    signInPrompt() {
      toast(SIGN_IN_WORDS, SIGN_IN);
    },

    /** Show another device without loading the page: its model is read, the page replaced, the
     * address pushed (unless this is a step of the history). A device that cannot be shown
     * stays unchanged and says so. The newest call wins. */
    async showDevice(url, { push = true } = {}) {
      const mine = ++showing;
      const answer = await store.read(url, { strip: true });
      if (mine !== showing) return { ok: false, status: 0, superseded: true };
      if (answer.status === 401) return { ok: false, status: 401 };
      if (answer.status !== 200 || !answer.device) {
        const words = answer.status === 0 ? answer.error : (answer.error || NOT_SHOWN);
        toast(words);
        return { ok: false, status: answer.status, error: words };
      }
      const page = store.page();
      const { [url]: _, ...followed } = page.followed ?? {};
      store.replace({
        ...page,
        shown: url,
        device: answer.device,
        followed,
        card: null,
        strip: answer.strip ?? null,
      });
      if (push) history.pushState({ device: url }, '', `./?device=${url}`);
      return { ok: true, status: 200 };
    },

    /** The declared hops: each answer is the list as it now stands. */
    hops: {
      add: (hop) => hopWrite('api/v1/hops', hop),
      edit: (id, fields) => hopWrite('api/v1/hops/edit', { id, ...fields }),
      remove: (id) => hopWrite('api/v1/hops/remove', { id }),
      move: (id, to) => hopWrite('api/v1/hops/move', { id, to }),
    },
  };

  /** One hop write: the fleet's hops are replaced by the answer's list. */
  async function hopWrite(path, body) {
    const away = turnedAway();
    if (away) return away;
    const done = finish(null, await post(path, body), REFUSED);
    if (done.ok && Array.isArray(done.hops)) {
      store.emit('fleet', { ...store.page().fleet, hops: done.hops });
    }
    return done;
  }

  return actions;
}
