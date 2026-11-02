/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What this page asked of the device, and when: every change Paper sends goes through `act`, which
// keeps the last fifty as `{ field, value, at, answeredAt, ok }`. The sheet's "last change" says
// `from this phone` only for a change this log holds (svolder keeps no record of who changed what).

/** How long after a request a change seen on the device may still be this page's own (ms). */
export const MINE_MS = 3000;

/** How many requests the log keeps. */
const KEPT = 50;

/** The log of `core`'s writes, `now()` being the clock (ms). */
export function createAct(core, { now }) {
  const entries = [];

  return {
    /**
     * Send the change of `field` to `value` with `call` (given `core.actions`, resolving the
     * answer), noting when it was asked and when and how it was answered. Resolves the answer; a
     * call that throws or rejects is logged as refused and rejects the same way. An answer
     * without `ok: true` counts as refused.
     */
    async act(field, value, call) {
      const entry = { field, value, at: now(), answeredAt: null, ok: null };
      entries.push(entry);
      if (entries.length > KEPT) entries.shift();
      try {
        const answer = await call(core.actions);
        entry.ok = answer?.ok === true;
        return answer;
      } catch (error) {
        entry.ok = false;
        throw error;
      } finally {
        entry.answeredAt = now();
      }
    },

    /** The requests made, oldest first (copies, so a reader cannot change the log). */
    log: () => entries.map((entry) => ({ ...entry })),
  };
}
