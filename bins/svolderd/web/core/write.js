/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The shape of every write the core makes: a guest's or a read-only page's call is turned away
// before any request with the words of why, an answer is finished (a refusal is toasted with the
// daemon's words and the device's model read again; success applies the answer's parts and says its
// toast), and a write to a device is posted under the device its call names. The words and the
// rule of who may write are here too.

import { refusal } from './api.js';

/** The words a guest's call is turned back with, and where its button leads. */
export const SIGN_IN_WORDS = 'Sign in to change this';
export const SIGN_IN = { label: 'Sign in', href: './?sign-in' };
/** The words of a page whose writes are off, when the model gave none. */
const READ_ONLY_WORDS = 'Read-only: [web] writes is off';
/** The words of a refused write that gave none: the page's own writes, and the ones posted by hand. */
const NOT_MADE = 'The change was not made.';
export const REFUSED = 'Refused.';
export const NO_DEVICE = 'No device is shown.';

/** Whether `page`'s viewer may write: false for a guest and for a page whose writes are off. Pure:
 * a theme's drawing asks it (`actions.canWrite` is it over the store's page). */
export function mayWrite(page) {
  const viewer = page?.viewer;
  if (viewer?.guest) return false;
  return viewer?.writes !== false;
}

/** The writes over `store`, posting with `post` (api.js's) and saying things with `toast`:
 * `{ turnedAway, finish, write, target }`. */
export function createWrites({ store, post, toast }) {
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

  /** The device a call acts on: the one its options name. No action reads the shown device itself;
   * `aimed` (actions.js) is the one place that names it, for a caller that named none. */
  const target = (options) => options?.device;

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

  return { turnedAway, finish, write, target };
}
