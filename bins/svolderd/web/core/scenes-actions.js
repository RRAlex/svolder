/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What a theme does to the scenes: the writes of the routes under `api/v1/scenes/`: save, recall,
// rename and delete, and the read of the view. A guest's or a read-only page's call sends nothing
// and says why (write.js's turn-away). Every call resolves `{ ok, status, ...answer }`: a refusal
// carries the daemon's words in `error` and its `reason` (`bad_name`, `taken`, `missing`, ...),
// and a page acts on the reason and shows the words, never reads them. A refusal is not toasted:
// the popup that asked says it in place. No call retries: a scene that is gone (a 404, or the
// reason `missing`) is answered `gone: true` and the view is read again once, so the list shows
// what the daemon has.
//
// A recall that would play one preset on a device and on the computer at once answers 409 with
// an `ask` (`kind: both_sides`, its `question` in scene words): `asker(question)` puts it in the
// theme's own place and resolves true or false, and a yes sends the same call with `confirm:
// true`, which recalls the scene as it was saved. A no sends nothing (`declined: true`).

import { get, refusal } from './api.js';
import { REFUSED } from './write.js';

/** The words of a scene that another front end took away meanwhile. */
export const GONE = 'That scene is gone.';

/** The scenes' actions over `store`, writing with `post` (api.js's); `writes` is the core's
 * `createWrites` (its turn-away), `fetch` and `signedOut` read the view as the store does. */
export function createScenesActions({ store, post, writes, fetch, signedOut }) {
  /** Read the view again: the one the daemon holds is what the page shows. */
  async function read() {
    const answer = await get('api/v1/scenes', { fetch, signedOut });
    if (answer.status === 200 && Array.isArray(answer.scenes)) {
      const { status: _status, ...view } = answer;
      store.emit('scenes', view);
    }
    return answer;
  }

  /** What an answer is: a refusal's words and reason (a missing scene is `gone`, and the view is
   * read again), or the answer itself. */
  async function finish(answer) {
    if (answer.status === 401) return { ok: false, ...answer };
    if (refusal(answer) === null) return { ok: true, ...answer };
    const gone = answer.status === 404 || answer.reason === 'missing';
    if (gone) await read();
    return {
      ok: false, ...answer, ...(gone ? { gone: true } : {}),
      error: gone ? GONE : (answer.error || REFUSED),
    };
  }

  /** One write: turned away, or posted to `path` under the scenes and finished. */
  async function write(path, body) {
    const away = writes.turnedAway();
    if (away) return away;
    return finish(await post(`api/v1/scenes/${path}`, body));
  }

  /** Recall the scene `name`: `{ ok, name, outcome }`. The both-sides question is put to
   * `asker`; with none the answer is handed back (`ask`, status 409). */
  async function recall(name, { confirm = false, asker = null } = {}) {
    const away = writes.turnedAway();
    if (away) return away;
    const answer = await post('api/v1/scenes/recall', {
      name, ...(confirm ? { confirm: true } : {}),
    });
    const question = answer.status === 409 && answer.ask?.kind === 'both_sides';
    if (question && !confirm && asker) {
      if (!(await asker(answer.ask.question))) {
        return { ok: false, status: 409, declined: true, ask: answer.ask };
      }
      return recall(name, { confirm: true });
    }
    if (question) return { ok: false, ...answer };
    return finish(answer);
  }

  return {
    read,
    recall,

    /** Save the sound as it is now under `name`; `force` saves over a scene of that name (after
     * a `taken` refusal and a yes). */
    save: (name, { force = false } = {}) => write('save', {
      name, ...(force ? { force: true } : {}),
    }),

    rename: (from, to) => write('rename', { from, to }),

    delete: (name) => write('delete', { name }),
  };
}
