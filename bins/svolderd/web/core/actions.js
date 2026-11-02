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

import { newest, refusal } from './api.js';
import { identifyWords } from './words.js';

/** The words a guest's call is turned back with, and where its button leads. */
const SIGN_IN_WORDS = 'Sign in to change this';
const SIGN_IN = { label: 'Sign in', href: './?sign-in' };
/** The words of a page whose writes are off, when the model gave none. */
const READ_ONLY_WORDS = 'Read-only: [web] writes is off';
/** The words of a refused write that gave none: the page's own writes, and the ones posted by hand. */
const NOT_MADE = 'The change was not made.';
const REFUSED = 'Refused.';
const NO_DEVICE = 'No device is shown.';
const NOT_SHOWN = 'That device could not be shown.';

/** The actions over `store`, writing with `post` (api.js's), saying things with `toast`, moving the
 * address with `history` and loading pages with `location`. */
export function createActions({ store, post, toast, history, location }) {
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

  const actions = {
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
