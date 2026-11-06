/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The browser tab's title, for every theme: the shown device's name and whether the page is
// connected to it (`Bifrost 3 · connected — svolder`). A theme may add words after the status
// through `add`. Plain data in; the document it writes to is given.

/** The words of the title. */
export const TITLE = {
  app: 'svolder',
  connected: 'connected',
  reconnecting: 'reconnecting',
  waiting: 'reading again',
  resting: 'resting',
  fault: 'fault',
};

/** How the page stands with the shown device: the stream lost, the page reading again or resting,
 * the title of the card of a device out of reach (lowercased; `fault` for a fault), else connected. */
export function statusOf(page, state) {
  if (state === 'lost') return TITLE.reconnecting;
  if (state === 'waiting') return TITLE.waiting;
  if (state === 'resting') return TITLE.resting;
  const card = page?.device?.card ?? page?.card ?? null;
  if (card) {
    return card.kind === 'fault' ? TITLE.fault : String(card.title ?? '').toLowerCase() || TITLE.fault;
  }
  return TITLE.connected;
}

/** The title of the device `name` with its `status` and the `more` words after it. */
export function titleOf({ name, status, more = [] }) {
  if (!name) return TITLE.app;
  return `${name} · ${status}${more.map((word) => ` · ${word}`).join('')} — ${TITLE.app}`;
}

/** Keep `document`'s title as the store says; `add(fn)` adds the words `fn(page)` returns (or null)
 * after the status and gives the function that takes them away again. */
export function createTitle({ store, document }) {
  const parts = new Set();

  function refresh() {
    if (!document) return;
    const page = store.page();
    const name = (page?.fleet?.devices ?? []).find((d) => d.url === store.shown())?.name;
    const more = [];
    for (const fn of parts) {
      try {
        const word = fn(page);
        if (word) more.push(word);
      } catch (error) {
        globalThis.console?.error?.('svolder: a title part threw', error);
      }
    }
    document.title = titleOf({ name, status: statusOf(page, store.state()), more });
  }

  const off = ['model', 'device', 'part', 'fleet', 'state'].map((event) => store.on(event, refresh));
  refresh();

  return {
    add(fn) {
      parts.add(fn);
      refresh();
      return () => {
        parts.delete(fn);
        refresh();
      };
    },
    refresh,
    stop() {
      for (const fn of off.splice(0)) fn();
      if (document) document.title = TITLE.app;
    },
    statusOf,
  };
}
