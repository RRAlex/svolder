/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the node harnesses of the theme Paper share for the computer's side: a fixture page with the
// computer's model in its host, and the core's own `computer` over a stand-in store, so the sections
// are drawn from the same data the page draws from. Not a test.
import { readFileSync } from 'node:fs';
import { createComputer, createHolds } from '../../../bins/svolderd/web/core/computer.js';

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));

export const point = (type, f, g, q) => ({ type, f, g, q });

/** The seven points of the approved picture. */
export const SEVEN = [
  point('LSC', 105, 5.5, 0.7), point('PK', 180, -2.1, 0.9), point('PK', 1200, -1.8, 1.4),
  point('PK', 2900, 3.2, 2), point('PK', 5600, -4, 3.1), point('PK', 8900, 2.4, 2.5),
  point('HSC', 10000, -1.5, 0.7),
];

/** The computer's model as the host event carries it. */
export function modelOf({ points = SEVEN, on = true, pct = 42, over = {} } = {}) {
  return {
    volume: pct === null ? null : { pct, muted: false, sink: 'Schiit' },
    eq: {
      on, running_unknown: false, preamp_db: -6, preset: null, live: true, note: null, max: 16,
      points, response: [],
    },
    presets: [
      { name: 'Flat', built_in: true, on: false, playable: true, why: null },
      { name: 'Warm', built_in: true, on: false, playable: true, why: null },
      { name: 'Huge', built_in: false, on: false, playable: false, why: '17 filters' },
    ],
    unavailable: null,
    ...over,
  };
}

/** A fixture page (`name`) with the computer's model in its host; `viewer` replaces the viewer. */
export function pageWith(name, computer, viewer) {
  const page = clone(fixture.cases.find((c) => c.name === name).page);
  page.host = { track: null, route: null, ...(page.host ?? {}), computer };
  if (viewer) page.viewer = { ...page.viewer, ...viewer };
  return page;
}

/** The view a sheet is drawn with, over the core's computer for `page` (`side`: what is kept). */
export function computerView(page, side = 'computer') {
  const map = new Map();
  const storage = {
    getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  };
  const store = { page: () => page, shown: () => page.shown, on() {}, emit() {} };
  const holds = createHolds({ store, fetch: async () => ({ status: 0 }) });
  const computer = createComputer({ store, storage, holds });
  computer.flip(side);
  return {
    view: { phone: false, coarse: false, state: 'open', now: 0, rulerPx: 1000, computer, point: null },
    computer,
  };
}

/** The words of `html` as a reader gets them: its text and the labels it carries. */
export function wordsOf(html) {
  const labels = [...html.matchAll(/(?:aria-label|aria-valuetext|title|value)="([^"]*)"/g)].map((m) => m[1]);
  const text = html.replace(/<[^>]*>/g, ' ');
  return `${text} ${labels.join(' ')}`.replace(/&middot;/g, '·').replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ');
}
