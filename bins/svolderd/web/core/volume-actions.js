/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume of a device: written one at a time, the newest waiting, held to the device's range and
// the cap, and a raise is not sent while the stream is lost or waking (the writer is newest-wins
// too, and enforces the volume rule; this is the page's half of it). A touch on the dial ends a
// climb, and the writes after it wait for the daemon's answer. Each call acts on the device its
// last `options` name.

import { newest, refusal } from './api.js';
import { NO_DEVICE, REFUSED } from './write.js';

/**
 * The volume actions over `store`, writing with `post` and saying things with `toast`; `writes` is
 * the core's `createWrites`. Returns `{ setVolume, stopClimb }`.
 */
export function createVolumeActions({ store, post, toast, writes }) {
  const { turnedAway, finish, target } = writes;
  /** Per device: the volume's sender, whether a run is going, the last answer, the stop's promise. */
  const volumes = new Map();

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
      // lowering of a device the page still reads always goes. The state is the page's, not the
      // device's: a wait on any owner's stream drops raises for every device.
      const live = store.state() === 'open' && store.device(url) !== null;
      if (!live && (held?.raw === null || held?.raw === undefined || raw > held.raw)) {
        // A refusal of this run stays the run's answer; the dropped raise says nothing over it.
        if (state.last?.ok !== false || state.last.dropped) {
          state.last = { ok: false, status: 0, dropped: true, raw };
        }
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

  const volumeActions = {
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
  };

  return volumeActions;
}
