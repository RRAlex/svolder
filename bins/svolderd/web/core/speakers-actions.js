/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The writes and reads of the Virtual speakers tab: the choice and the switch (`api/v1/host/
// speakers`, the computer's own, held like the crossfeed's until the model has been read after
// the write) and the sets (`api/v1/speakers/catalogue`, `fetch`, `import`, `forget`). The daemon
// is the one process that reaches the sets' sources and keeps them; each call here is one POST.
//
// A choice and the switch are one lane: the fields merge (newest wins) and one request is in flight
// at a time, because choosing a set rebuilds the chain and turning on after it must wait for it.
// `fetch` and `import` answer once, when the daemon has downloaded, checked and converted the set
// (the page gets no progress; a request the caller aborts cancels the job in the daemon).
//
// The set calls answer `{ ok: true, status, ...answer }` or `{ ok: false, status, reason, error }`,
// with `aborted: true` when the caller's `signal` ended the request. `reason` is one of
// SPEAKERS_REFUSALS; `error` is the daemon's words, or the page's. Nothing is toasted.

import { newest, refusal } from './api.js';
import { REFUSED } from './write.js';

/** Every reason a set call is refused with. The daemon's: `busy`, `not_listed`, `unreachable`,
 * `invalid`, `missing`, `stopping`, `internal`. The page's: `too_large` (a 413), `signed_out`,
 * `forbidden` (writes off), `no_answer` and `failed`. */
export const SPEAKERS_REFUSALS = Object.freeze([
  'busy', 'not_listed', 'unreachable', 'invalid', 'missing', 'stopping', 'internal', 'too_large',
  'signed_out', 'forbidden', 'no_answer', 'failed',
]);
const FROM_DAEMON = new Set([
  'busy', 'not_listed', 'unreachable', 'invalid', 'missing', 'stopping', 'internal',
]);
const BY_STATUS = { 0: 'no_answer', 401: 'signed_out', 403: 'forbidden', 413: 'too_large' };

/** What a refusal says when nobody gave words. */
export const NOT_DONE = 'The sets could not be reached.';

/** The speakers' calls over `store`, posting with `post`; `held` is the core's pending values and
 * `writes` its `createWrites`; `toast` says a refused choice when no popup listens. */
export function createSpeakersActions({ store, post, writes, held, toast }) {
  const { turnedAway } = writes;
  const refusals = new Set();
  const asked = { body: null, shown: {} };
  held.listen((type) => {
    if (type === 'settled' && held.get('speakers') === null) asked.shown = {};
  });

  /** The one lane of the choice and the switch: the newest fields wait while one request runs. */
  const state = { running: false, last: null };
  state.send = newest(async () => {
    const body = asked.body;
    asked.body = null;
    const answer = await post('api/v1/host/speakers', body);
    const why = answer.status === 401 ? null : refusal(answer);
    if (why === null) {
      state.last = { ok: answer.status !== 401, ...answer };
      return null;
    }
    state.last = { ok: false, ...answer, error: answer.error || REFUSED };
    return state.last.error;
  }, (why) => {
    asked.shown = {};
    asked.body = null;
    if (refusals.size === 0) toast(why);
    else refusals.forEach((fn) => fn(why));
    held.drop('speakers');
    held.resync();
  });

  async function lane(value) {
    const away = turnedAway();
    if (away) return away;
    held.set('speakers', value);
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
    if (state.last?.ok) held.release('speakers');
    else held.drop('speakers');
    return state.last ?? { ok: true, status: 0, value };
  }

  /** One set request to `path`: turned away, or posted and read as an answer. */
  async function send(path, body, signal) {
    const away = turnedAway();
    if (away) return { ...away, reason: store.page().viewer?.guest ? 'signed_out' : 'forbidden' };
    const answer = await post(path, body, signal ? { signal } : undefined);
    if (answer.aborted) {
      return { ok: false, aborted: true, status: 0, reason: 'no_answer', error: '' };
    }
    if (answer.status !== 401 && refusal(answer) === null) return { ok: true, ...answer };
    const reason = FROM_DAEMON.has(answer.reason) ? answer.reason
      : BY_STATUS[answer.status] ?? 'failed';
    const error = typeof answer.error === 'string' && answer.error ? answer.error : NOT_DONE;
    return { ok: false, status: answer.status, reason, error };
  }

  return {
    /** Change the virtual speakers: `fields` is `on` and/or `set` (a fetched set's id). The newest
     * value of each field wins; one request at a time. */
    setSpeakers(fields) {
      asked.body = { ...(asked.body ?? {}), ...fields };
      asked.shown = { ...asked.shown, ...fields };
      return lane({ ...asked.shown });
    },
    /** The fields asked and not yet answered by the model. */
    speakersAsked: () => held.get('speakers')?.value ?? {},
    /** `fn(words)` hears each refusal of a choice or switch; returns the way to stop. */
    onSpeakersRefusal(fn) {
      refusals.add(fn);
      return () => refusals.delete(fn);
    },
    /** The catalogue's sets and the person's own, with whether each is fetched. */
    speakersCatalogue: (request = {}) => send('api/v1/speakers/catalogue', {}, request.signal),
    /** Download, check and keep the catalogue set `id`; answers after the daemon's work. */
    speakersFetch: (id, request = {}) => send('api/v1/speakers/fetch', { id }, request.signal),
    /** Keep a file as the person's own set: `data` is the file as base64, `room` `with` or `no`. */
    speakersImport: ({ data, label, room }, request = {}) => send('api/v1/speakers/import', {
      data, ...(label ? { label } : {}), ...(room ? { room } : {}),
    }, request.signal),
    /** Remove the set `id` from this machine. */
    speakersForget: (id, request = {}) => send('api/v1/speakers/forget', { id }, request.signal),
    /** The sets' calls refresh the daemon's view through its stream; the held choice is read. */
    resyncSpeakers: () => held.resync(),
  };
}
