/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The writes that name no device: the host player's transport buttons and the declared hops. A
// hop's answer is the list as it now stands, and the fleet's hops are replaced by it.

import { REFUSED } from './write.js';

/** The host actions over `store`, posting with `post`; `writes` is the core's `createWrites`.
 * Returns `{ press, hops }`. */
export function createHostActions({ store, post, writes }) {
  const { turnedAway, finish } = writes;

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

  const hostActions = {
    /** A transport button of the host's player. */
    async press(button) {
      const away = turnedAway();
      if (away) return away;
      return finish(null, await post('api/v1/host/press', { button }), REFUSED);
    },

    /** The declared hops: each answer is the list as it now stands. */
    hops: {
      add: (hop) => hopWrite('api/v1/hops', hop),
      edit: (id, fields) => hopWrite('api/v1/hops/edit', { id, ...fields }),
      remove: (id) => hopWrite('api/v1/hops/remove', { id }),
      move: (id, to) => hopWrite('api/v1/hops/move', { id, to }),
    },
  };

  return hostActions;
}
