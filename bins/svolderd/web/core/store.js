/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the page knows: the model of the page (the viewer, the fleet, the device shown, the devices
// followed, the host's player), and how it changes. A snapshot, the stream's events and a write's
// answer all land here; a theme draws from `part` and `device` and hears what changed through
// `on`. Plain data and callbacks in, no DOM: the fetch it reads with is given.
//
//   events   model (page)                 the page was replaced whole, heard once
//            device ({device, data})      one device's whole shape was replaced
//            part ({device, name, data, before})   one part of one device changed
//            volume | level | meters | strip   a reading of a device (the data names `device`)
//            fleet, host                  the fleet or the host's player changed
//            state ('open' | 'lost' | 'resting' | 'waiting')   how the stream is doing
//            opened                       the stream opened (what a theme last read is forgotten)
//
// A replacing model that carries no strip (the stream's never does) keeps the strip the page holds
// for the same device shown; another device shown clears it.

import { NO_ANSWER, signedOut as apiSignedOut } from './api.js';

/** The names of a device's parts, as the model and the `part` events name them. */
export const PARTS = ['link', 'card', 'volume', 'controls', 'meters', 'power', 'tone', 'presets',
  'device_presets', 'facts', 'playing'];

/** The parts a refused write may have left showing what was chosen, not what the device holds:
 * a read of the model republishes them whether or not they differ. */
const REREAD = new Set(['controls', 'power', 'tone', 'device_presets']);

/** A page with nothing in it, for a page that carried no model. */
const empty = () => ({
  v: 1,
  viewer: { signed_in: false, guest: false, writes: false },
  fleet: { devices: [], hops: [] },
  shown: null,
  device: null,
  followed: {},
  card: null,
  host: null,
  strip: null,
});

/** Make the store of `page` (a PageModel), reading with `fetch`; a read the sign-in has ended
 * for calls `signedOut` (api.js's loads the sign-in page). */
export function createStore(page, { fetch, signedOut = apiSignedOut }) {
  let model = page ?? empty();
  let state = 'open';
  const listeners = new Map();

  const shown = () => model.shown ?? null;
  const device = (url = shown()) => {
    if (url === null || url === undefined) return null;
    if (url === model.shown) return model.device ?? null;
    return model.followed?.[url] ?? null;
  };
  const part = (name, url = shown()) => device(url)?.[name] ?? null;

  /** Tell every listener of `event` what happened; one that throws does not stop the rest. */
  function notify(event, data) {
    for (const fn of [...(listeners.get(event) ?? [])]) {
      try {
        fn(data);
      } catch (error) {
        globalThis.console?.error?.('svolder: a listener of', event, 'threw', error);
      }
    }
  }

  const on = (event, fn) => {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(fn);
    return () => listeners.get(event)?.delete(fn);
  };

  const without = ({ device: _, ...rest }) => rest;

  /** Put `value` as the part `name` of the device `url`, if the page holds that device. */
  function put(url, name, value) {
    const held = device(url);
    if (!held) return false;
    held[name] = value;
    return true;
  }

  /** Fold a reading into its device's model. */
  function fold(event, data) {
    const url = data.device ?? shown();
    const held = device(url);
    if (!held) return false;
    if (event === 'volume') {
      held.volume = { ...held.volume, ...without(data) };
    } else if (event === 'meters') {
      held.meters = { ...held.meters, ...without(data) };
    } else if (event === 'level') {
      held.controls = (held.controls ?? []).map((control) => (
        control.kind === 'slider' && control.key === data.key
          ? { ...control, ...without(data) } : control));
    }
    return true;
  }

  function emit(event, data) {
    if (event === 'volume' || event === 'level' || event === 'meters' || event === 'strip') {
      if (!fold(event, data)) return;
    } else if (event === 'fleet') {
      model.fleet = data;
    } else if (event === 'host') {
      model.host = data;
    }
    notify(event, data);
  }

  function applyPart(url, name, data) {
    const before = part(name, url);
    if (!put(url, name, data)) return;
    notify('part', { device: url, name, data, before });
  }

  /** GET a device's model: `{status, device, strip}`, or the words of why not. */
  async function read(url, { strip = false } = {}) {
    try {
      const answer = await fetch(`api/v1/devices/${url}/model${strip ? '?strip=1' : ''}`,
        { credentials: 'same-origin' });
      let json = {};
      try {
        json = await answer.json();
      } catch {
        json = {};
      }
      if (answer.status === 401) signedOut();
      return { status: answer.status, ...json };
    } catch {
      return { status: 0, error: NO_ANSWER };
    }
  }

  return {
    page: () => model,
    shown,
    device,
    part,
    state: () => state,
    on,
    emit,
    read,
    applyPart,

    /** Replace the whole page (a snapshot, a device switch): heard once. */
    replace(next) {
      if ((next.strip ?? null) === null && model.strip && next.shown === model.shown) {
        next.strip = model.strip;
      }
      model = next;
      notify('model', model);
    },

    /** Replace one device's whole shape. */
    setDevice(url, data) {
      if (url === model.shown) model.device = data;
      else if (model.followed) model.followed[url] = data;
      else model.followed = { [url]: data };
      notify('device', { device: url, data });
    },

    /** Stop holding a followed device. */
    forget(url) {
      if (model.followed) delete model.followed[url];
    },

    /** Apply `parts` ({name: data}), the ones a write answered with. */
    applyParts(url, parts) {
      for (const [name, data] of Object.entries(parts ?? {})) applyPart(url, name, data);
    },

    setState(next) {
      if (next === state) return;
      state = next;
      notify('state', state);
    },

    /**
     * Read the device's model again and publish its parts: each one that differs, and the ones a
     * refused write leaves showing what was chosen. Resolves `{ok, status, error?}`.
     */
    async refresh(url = shown()) {
      if (!device(url)) return { ok: false, status: 0 };
      const answer = await read(url);
      if (answer.status !== 200 || !answer.device) {
        return { ok: false, status: answer.status, error: answer.error };
      }
      for (const name of PARTS) {
        const next = answer.device[name] ?? null;
        const changed = JSON.stringify(next) !== JSON.stringify(part(name, url));
        if (changed || REREAD.has(name)) applyPart(url, name, next);
      }
      return { ok: true, status: 200 };
    },
  };
}
