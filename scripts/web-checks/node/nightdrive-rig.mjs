/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The core's pieces over a page with a computer's EQ in it, for Night drive's node harnesses: the real store, holds,
// computer side and actions, with a fake post, read, question and storage. Not a test: nightdrive-hud.mjs,
// nightdrive-tune.mjs and nightdrive-draw.mjs import it.
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { createComputer, createHolds } from '../../../bins/svolderd/web/core/computer.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import * as drag from '../../../bins/svolderd/web/core/tone-drag.js';

export const point = (type, f, g, q) => ({ type, f, g, q });

/** A non-flat curve: a low shelf, a few peaks and a high shelf. */
export const SEVEN = [
  point('LSC', 105, 5.5, 0.7), point('PK', 180, -2.1, 0.9), point('PK', 1200, -1.8, 1.4), point('PK', 2900, 3.2, 2),
  point('PK', 5600, -4, 3.1), point('PK', 8900, 2.4, 2.5), point('HSC', 10000, -1.5, 0.7),
];

export const modelOf = (over = {}) => ({
  volume: { pct: 42, muted: false, sink: 'Schiit' },
  eq: {
    on: true, running_unknown: false, preamp_db: -6, preset: null, live: true, note: null, max: 16, points: SEVEN, response: [],
  },
  presets: [
    { name: 'Warm', built_in: true, on: false, playable: true, why: null },
    { name: 'Huge', built_in: false, on: false, playable: false, why: '17 filters' },
  ],
  unavailable: null,
  ...over,
});

export const DAC = {
  name: 'Gungnir 2', url: 'Gungnir%202', family: 'dac', volume: { text: '-23.5 dB', unit: 'dB', muted: false, raw: 40, mute: 'mute' },
  tone: null,
};
export const AMP = { name: 'Asgard 3', url: 'Asgard%203', family: 'amp', volume: null, tone: null };

export const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
export const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };

const memory = () => {
  const map = new Map();
  return { map, getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, String(v)), removeItem: (k) => map.delete(k) };
};

/** The page over `computer` and `device`; `side` is the side kept. `reply(path, body)` answers a post. */
export function computerRig({
  computer = modelOf(), viewer = SIGNED, device = DAC, side = 'computer', reply, answer = false,
} = {}) {
  const posts = [];
  const toasts = [];
  const questions = [];
  const page = {
    v: 1, viewer, fleet: { devices: [device], hops: [] }, shown: device.url, device, followed: {}, card: null,
    host: { track: null, route: null, computer }, strip: null,
  };
  const store = createStore(page, {
    signedOut: () => {},
    fetch: async () => ({ status: 200, json: async () => ({ device }) }),
  });
  const holds = createHolds({ store, fetch: async () => ({ status: 200, json: async () => ({ computer }) }) });
  const storage = memory();
  storage.setItem('svolder.side', side);
  const actions = createActions({
    store,
    post: async (path, body) => { posts.push([path, body]); return reply ? reply(path, body) : { status: 200, ok: true }; },
    toast: (text) => toasts.push(text),
    history: { pushState: () => {} },
    location: { assign: () => {} },
    holds,
    ask: async (question) => { questions.push(question); return answer; },
  });
  const computerCore = createComputer({ store, storage, holds });
  const core = {
    model: store, computer: computerCore, actions, drag, frame: { toast: (text) => toasts.push(text) }, prefs: { get: (k, d) => d },
  };
  return {
    core, store, holds, posts, toasts, questions, storage,
  };
}
