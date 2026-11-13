/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The writes of the device walk, which the daemon runs: start it, answer its prompt, step back,
// cancel it. `start` is one POST to `api/v1/walk` and answers the job's id; every other call is one
// POST to the job its last `options` name (`{ job }`) and to no other: a call that names none
// sends nothing, "the current walk" is never taken in its place.
//
// A call answers `{ ok: true, status, job? }` (`job` from `start`), or `{ ok: false, status,
// reason, error }`: `reason` is one of WALK_REFUSALS, for the caller to word where it asked, and
// `error` the daemon's words. Nothing is toasted but a guest's or a read-only page's call, which is
// turned away as every write is (write.js). A passphrase is sent in the body only, and words that
// quote it are not handed on.

import { refusal } from './api.js';

/** Every reason a walk call is refused with. The daemon's: `busy` (another walk runs), `stopping`
 * (the daemon is), `invalid` (a body it does not take), `not_waiting` (the walk works on the last
 * answer, or has ended) and `no_such_job` (a walk it does not have). The page's: `signed_out` (a
 * guest, or a sign-in that ended), `forbidden` (writes are off), `no_answer` (the daemon did not
 * answer), `no_job` (the call named none) and `failed` (any other refusal). */
export const WALK_REFUSALS = Object.freeze([
  'busy', 'stopping', 'invalid', 'not_waiting', 'no_such_job', 'signed_out', 'forbidden',
  'no_answer', 'no_job', 'failed',
]);
const FROM_DAEMON = new Set(['busy', 'stopping', 'invalid', 'not_waiting', 'no_such_job']);
const BY_STATUS = { 0: 'no_answer', 401: 'signed_out', 403: 'forbidden', 404: 'no_such_job' };

const NOT_NAMED = 'No walk is named.';
/** What a refusal says when the daemon gave no words. */
export const NOT_DONE = 'The device walk did not take that.';
const SECRET_KEPT = 'The walk did not take that.';

/** The walk's writes over `store`, posting with `post`; `writes` is the core's `createWrites`. */
export function createWalkActions({ store, post, writes }) {
  const { turnedAway } = writes;

  /** One call at `path`: turned away, or posted and read. `secret` is a passphrase the body
   * carries: a refusal whose words quote it answers words of the page's own. */
  async function send(path, body, secret = null) {
    const away = turnedAway();
    if (away) {
      return { ...away, reason: store.page().viewer?.guest ? 'signed_out' : 'forbidden' };
    }
    const answer = await post(path, body);
    if (answer.status !== 401 && refusal(answer) === null) {
      const job = answer.job === undefined ? {} : { job: answer.job };
      const run = answer.run === undefined ? {} : { run: answer.run };
      return { ok: true, status: answer.status, ...job, ...run };
    }
    const reason = FROM_DAEMON.has(answer.reason) ? answer.reason
      : BY_STATUS[answer.status] ?? 'failed';
    const words = typeof answer.error === 'string' && answer.error ? answer.error : NOT_DONE;
    const quoted = Boolean(secret) && words.includes(secret);
    return { ok: false, status: answer.status, reason, error: quoted ? SECRET_KEPT : words };
  }

  /** The call at `tail` of the job `options` name (and the daemon's run, when it names one). */
  function onJob(tail, body, options, secret = null) {
    const job = options?.job;
    if (!Number.isSafeInteger(job) || job < 0) {
      return Promise.resolve({ ok: false, status: 0, reason: 'no_job', error: NOT_NAMED });
    }
    // the run the job's id was told in: the daemon refuses an id of another run
    const { run } = options;
    const named = Number.isSafeInteger(run) ? { ...body, run } : body;
    return send(`api/v1/walk/${job}/${tail}`, named, secret);
  }

  return {
    /** Start a walk; resolves its job's id, or `busy` while another walk runs. */
    start: () => send('api/v1/walk', {}),
    /** Answer the prompt of the job: `{ choice }` (the number of a choice, from 0) or `{ text }`
     * (a name, a passphrase). */
    answer: (answer, options) => {
      const text = typeof answer?.text === 'string' ? answer.text : null;
      return onJob('answer', answer, options, text);
    },
    /** Step the job back to the step before. */
    back: (options) => onJob('back', {}, options),
    /** End the job. */
    cancel: (options) => onJob('cancel', {}, options),
  };
}
