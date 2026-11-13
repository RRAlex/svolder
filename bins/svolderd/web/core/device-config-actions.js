/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The writes to a configured device's settings: its name, the profile it is pinned to, its
// passphrase, the default mark and its removal. Each is one POST to the device its last `options`
// name (`{ device }`, the device's URL segment), and to no other: a call that names none sends
// nothing, the shown device is never taken in its place.
//
// A write answers `{ ok: true, status, devices, profiles }`, the configured devices after it and
// the profiles a device can be pinned to, or `{ ok: false, status, reason, error }`: `reason` is
// one of DEVICE_REFUSALS, for the caller to word at its control, and `error` the daemon's words.
// Nothing is toasted and no model is read again, but for a guest's or a read-only page's call,
// which is turned away as every write is (write.js). A passphrase is sent in the body only, and
// words that quote it are not handed on.

import { refusal } from './api.js';

/** Every reason a write is refused with. The daemon's: `taken` (the name is another device's),
 * `missing` (no such device, or no longer), `invalid` (a name, a profile or a passphrase it does
 * not take), `read_only` and `file_busy` (the configuration file), `stopping` (the daemon is); the
 * device walk's routes add `busy` (another walk runs), `not_waiting` (the walk works on its last
 * answer, or has ended) and `no_such_job` (core/walk-actions.js words its own list).
 * The page's: `signed_out` (a guest, or a sign-in that ended), `forbidden` (writes are off),
 * `no_answer` (the daemon did not answer), `no_device` (the call named none) and `failed` (any
 * other refusal). */
export const DEVICE_REFUSALS = Object.freeze([
  'taken', 'missing', 'invalid', 'read_only', 'file_busy', 'stopping', 'busy', 'not_waiting',
  'no_such_job', 'signed_out', 'forbidden', 'no_answer', 'no_device', 'failed',
]);
const FROM_DAEMON = new Set([
  'taken', 'missing', 'invalid', 'read_only', 'file_busy', 'stopping', 'busy', 'not_waiting',
  'no_such_job',
]);
const BY_STATUS = { 0: 'no_answer', 401: 'signed_out', 403: 'forbidden', 404: 'missing' };

const NOT_NAMED = 'No device is named.';
/** What a refusal says when the daemon gave no words. */
export const NOT_CHANGED = 'The device was not changed.';
const PASSPHRASE_KEPT = 'The passphrase was not changed.';
/** The two passphrases that are words of the format and no secret. */
const OPEN = new Set(['auto', 'none']);

/** The configuration's writes over `store`, posting with `post`; `writes` is the core's
 * `createWrites`. */
export function createDeviceConfigActions({ store, post, writes }) {
  const { turnedAway, target } = writes;

  /** One write to the device `options` name, at `tail` of its routes. `secret` is a passphrase
   * the body carries: a refusal whose words quote it answers words of the page's own. */
  async function send(tail, body, options, secret = null) {
    const away = turnedAway();
    if (away) {
      return { ...away, reason: store.page().viewer?.guest ? 'signed_out' : 'forbidden' };
    }
    const url = target(options);
    if (!url) return { ok: false, status: 0, reason: 'no_device', error: NOT_NAMED };
    const answer = await post(`api/v1/devices/${url}/${tail}`, body);
    if (answer.status !== 401 && refusal(answer) === null) return { ok: true, ...answer };
    const reason = FROM_DAEMON.has(answer.reason) ? answer.reason
      : BY_STATUS[answer.status] ?? 'failed';
    const words = typeof answer.error === 'string' && answer.error ? answer.error : NOT_CHANGED;
    const quoted = Boolean(secret) && !OPEN.has(secret) && words.includes(secret);
    return { ok: false, status: answer.status, reason, error: quoted ? PASSPHRASE_KEPT : words };
  }

  return {
    /** Give the device another name. Its URL segment follows its name. */
    renameDevice: (name, options) => send('edit', { name }, options),
    /** Pin the device to `profile` (a name of the answer's `profiles`); `null` takes the pin off. */
    setProfile: (profile, options) => send('edit', { profile }, options),
    /** Change the device's passphrase: `auto`, `none` or its digits. */
    setPassphrase: (passphrase, options) => send('edit', { passphrase }, options,
      String(passphrase)),
    /** Make the device the default. */
    makeDefault: (options) => send('default', {}, options),
    /** Take the device out of the configuration. */
    removeDevice: (options) => send('remove', {}, options),
  };
}
