/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The writes and reads of the EQ library: the AutoEq search, the preview of a measurement for a
// side, its import as a preset, the upload of a file's text as one and the overwrite of a saved
// preset with the curve that is on. The daemon is the only process that reaches AutoEq; each call
// here is one POST to a route under `api/v1/autoeq/` (or the device's presets route) and no more.
//
// The side an import or a preview is about is named in the call's last argument, never taken from
// the page: `{ device }` (the device's URL segment, sent as the device's name) or `{ computer:
// true }`. A call that names neither sends nothing (`no_target`). The search and the upload are
// about no side and take their request apart from that: `request` holds an abort `signal`.
//
// Each answers `{ ok: true, status, ...answer }` or `{ ok: false, status, reason, error }`, with
// `aborted: true` when the caller's `signal` ended the request (no failure to word). `reason` is
// one of AUTOEQ_REFUSALS, for the caller to word at its control; `error` is the daemon's own words,
// or the page's when it gave none. Nothing is toasted and no model is read again but for a guest's
// or a read-only page's call, which is turned away as every write is (write.js).

import { refusal } from './api.js';

/** Every reason a call is refused with. The daemon's: `busy`, `not_listed`, `unreachable`, `taken`,
 * `bad_name`, `invalid`, `missing`, `not_found_yet`, `not_connected`, `read_only`, `file_busy`,
 * `stopping`, `internal`. The page's: `too_large` (a 413), `signed_out` (a guest, or a sign-in that
 * ended), `forbidden` (writes are off), `no_answer`, `no_target` (the call named no side) and
 * `failed` (any other refusal). */
export const AUTOEQ_REFUSALS = Object.freeze([
  'busy', 'not_listed', 'unreachable', 'taken', 'bad_name', 'invalid', 'missing', 'not_found_yet',
  'not_connected', 'read_only', 'file_busy', 'stopping', 'internal', 'too_large', 'signed_out',
  'forbidden', 'no_answer', 'no_target', 'failed',
]);
const FROM_DAEMON = new Set([
  'busy', 'not_listed', 'unreachable', 'taken', 'bad_name', 'invalid', 'missing', 'not_found_yet',
  'not_connected', 'read_only', 'file_busy', 'stopping', 'internal',
]);
const BY_STATUS = { 0: 'no_answer', 401: 'signed_out', 403: 'forbidden', 413: 'too_large' };

/** What a refusal says when the daemon gave no words. */
export const NOT_DONE = 'AutoEq did not answer.';
const NOT_NAMED = 'No side of the EQ is named.';

/** The EQ library's calls over `store`, posting with `post` (api.js's); `writes` is the core's
 * `createWrites`. */
export function createAutoEqActions({ store, post, writes }) {
  const { turnedAway, target } = writes;

  /** The daemon's name of the device at the URL segment `url`: the fleet's, else the segment
   * decoded. */
  function nameOf(url) {
    const row = (store.page().fleet?.devices ?? []).find((one) => one.url === url);
    if (row?.name) return row.name;
    try {
      return decodeURIComponent(url);
    } catch {
      return url;
    }
  }

  /** The side `options` name, as the daemon's `target`; nothing when it names none. */
  function sideOf(options) {
    if (options?.computer === true) return { side: 'computer' };
    const url = target(options);
    return url ? { side: 'device', device: nameOf(url) } : null;
  }

  /** One request to `path`: turned away, or posted and read as an answer. */
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

  /** A call about a side: `build(side)` is the body, sent only when `options` name one. */
  function aboutSide(path, build, options) {
    const side = sideOf(options);
    if (!side) {
      return Promise.resolve({ ok: false, status: 0, reason: 'no_target', error: NOT_NAMED });
    }
    return send(path, build(side), options?.signal);
  }

  return {
    /** Search the index for `query` (at most 60 characters); `refresh` fetches the index again. */
    autoEqSearch: (query, request = {}) => send('api/v1/autoeq/search',
      { query, ...(request.refresh ? { refresh: true } : {}) }, request.signal),

    /** The preview of the measurement `variant` for the side `options` name, with `options.compare`
     * (a second measurement) overlaid when given. */
    autoEqPreview: (variant, options) => aboutSide('api/v1/autoeq/preview', (side) => ({
      variant, target: side, ...(options?.compare ? { compare: options.compare } : {}),
    }), options),

    /** Save the measurement `variant` as a preset for the side `options` name. A `name` left out
     * is the daemon's (the headphone's own, shortened); `force` writes over the user's preset of
     * that name. */
    autoEqImport: (variant, options) => aboutSide('api/v1/autoeq/import', (side) => ({
      variant, target: side,
      ...(options?.name ? { name: options.name } : {}), ...(options?.force ? { force: true } : {}),
    }), options),

    /** Save parametric EQ text as a preset called `name`. */
    autoEqUpload: (text, name, request = {}) => send('api/v1/autoeq/upload',
      { text, name, ...(request.force ? { force: true } : {}) }, request.signal),

    /** Write the bands of the device `options` name over the user's preset `name`. A refusal is
     * the answer's and is not toasted: the popup words it where the question was. */
    async overwritePreset(name, options) {
      const away = turnedAway();
      if (away) return away;
      const url = target(options);
      if (!url) return { ok: false, status: 0, reason: 'no_target', error: NOT_NAMED };
      const answer = await post(`api/v1/devices/${url}/presets`, { name, force: true });
      if (answer.status === 401 || refusal(answer) !== null) {
        const reason = FROM_DAEMON.has(answer.reason) ? answer.reason
          : BY_STATUS[answer.status] ?? 'failed';
        const error = typeof answer.error === 'string' && answer.error ? answer.error : NOT_DONE;
        return { ok: false, status: answer.status, reason, error };
      }
      // the page words the success itself, under the list: the answer's toast would say it twice
      if (answer.parts) store.applyParts(url, answer.parts);
      return { ok: true, ...answer };
    },
  };
}
