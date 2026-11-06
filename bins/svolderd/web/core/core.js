/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The core a theme is given: the model of the page, the actions that change a device, the
// preferences of the theme mounted, light or dark, the lifecycle of a canvas, the host's analysis,
// the tone curve's maths and gestures, the words and the clock, the frame (the menu's button, its
// menu, the picker of themes and the popups) and the registry of themes. `createCore` makes every
// module and joins what they leave to it: the store's state and events go to the climb's popup and
// the tone mode's note, and a step of the browser's history shows the device it names.
//
// The preferences and the light or dark are bound to the theme being mounted (`prefs`, `scheme`
// and `theme` are of the theme last mounted), and the stream asks the daemon for what that theme
// needs.

import { post, signedOut } from './api.js';
import { createActions } from './actions.js';
import { createAnalysis } from './analysis.js';
import { createClimb } from './climb.js';
import { createFrame } from './frame.js';
import { HOLD_AFTER_MS, HOLD_EVERY_MS, holdRepeat } from './hold.js';
import { bootScheme, createPrefs, storageOf, writeChoice } from './prefs.js';
import { createRegistry } from './registry.js';
import { createScheme, paintChrome } from './scheme.js';
import { createStore } from './store.js';
import { createStream } from './stream.js';
import { createTitle } from './title.js';
import { createToneNote } from './tone-note.js';
import { dismissToast, toast } from './toast.js';
import { canvasLoop } from './lifecycle.js';
import { watchVisible } from './visible.js';
import * as clock from './clock.js';
import * as curve from './tone-curve.js';
import * as drag from './tone-drag.js';
import * as words from './words.js';

/** What the daemon can send beside a device's own parts: the words a theme's `needs` may name. */
const STREAM_NEEDS = ['meters', 'host'];

/** The device a step of the history shows, by its URL segment: the one the entry kept, else the
 * one the address names, else the first the fleet lists. */
function deviceOf(window, store, state) {
  const devices = store.page().fleet?.devices ?? [];
  if (typeof state?.device === 'string') return state.device;
  const asked = new URLSearchParams(window.location.search).get('device');
  const named = devices.find((entry) => {
    try {
      return decodeURIComponent(entry.url) === asked;
    } catch {
      return false;
    }
  });
  return (named ?? devices[0])?.url ?? null;
}

/**
 * The core over `page` (the page's model), listing `themes` (`[{id, label, blurb, module, styles,
 * schemes}]`), in `window`; `importer` loads a theme's module (the registry's own by default);
 * `pick(id)` is what a card of the frame's picker asks for (the page switches themes there);
 * `painted` is the scheme the server drew the first frame in (`'light'`, `'dark'`, or nothing).
 */
export function createCore({
  page, themes = [], window, importer, pick = () => {}, painted = null,
}) {
  const document = window.document;
  const fetchNow = (...args) => window.fetch(...args);
  const storage = storageOf(window);

  const store = createStore(page, { fetch: fetchNow, signedOut });
  const actions = createActions({
    store, post, toast, history: window.history, location: window.location,
  });
  const climb = createClimb({ toast, dismissToast, body: document.body });
  const toneNote = createToneNote({ toast, now: () => window.performance.now() });
  const analysis = createAnalysis({ EventSource: window.EventSource });
  const registry = createRegistry(themes, {
    ...(importer ? { importer } : {}),
    beforeMount: (theme) => bind(theme),
    afterMount: (theme) => {
      try {
        framed.mounted(theme);
      } catch (error) {
        console.warn('svolder: the frame could not take the theme\'s button', error);
      }
    },
  });

  /** The core, whose `prefs`, `scheme` and `theme` follow the theme mounted. */
  const core = {
    model: store,
    actions,
    prefs: null,
    scheme: null,
    theme: null,
    lifecycle: { watchVisible, canvasLoop },
    analysis,
    title: null,
    hold: { repeat: holdRepeat, AFTER_MS: HOLD_AFTER_MS, EVERY_MS: HOLD_EVERY_MS },
    curve,
    drag,
    words,
    clock,
    frame: null,
    keys: [{ key: 'T', words: 'Themes' }, { key: 'Space', words: 'Play or pause' }],
    registry,
    climb,
    toneNote,
    stream: null,
  };

  core.title = createTitle({ store, document });

  const framed = createFrame({ window, core, pick: (id) => pick(id) });
  core.frame = framed.frame;

  core.stream = createStream({
    store,
    needs: () => (core.theme?.needs ?? []).filter((need) => STREAM_NEEDS.includes(need)),
    EventSource: window.EventSource,
    fetch: fetchNow,
    watchVisible,
    signedOut,
    reload: () => window.location.reload(),
    show: (url) => actions.showDevice(url, { push: false }),
    doc: document,
  });

  let bound = false;

  /** Bind the preferences and the scheme to `theme`, as it is mounted. The first theme of the page
   * that has both schemes takes the scheme the server painted as the person's word, so a reload
   * shows what the first byte showed. */
  function bind(theme) {
    core.scheme?.stop();
    core.theme = theme;
    core.prefs = createPrefs(theme.id, storage);
    const schemes = theme.schemes ?? ['dark'];
    if (!bound && schemes.length > 1) core.prefs.set('scheme', bootScheme(painted));
    bound = true;
    core.scheme = createScheme({
      matchMedia: (query) => window.matchMedia(query),
      root: document.documentElement,
      prefs: core.prefs,
      schemes,
      onCookie: (mode) => writeChoice(
        { id: theme.id, scheme: mode },
        { storage, document, path: window.location.pathname },
      ),
    });
    const chrome = (effective, mode) => paintChrome(
      document, registry.get(theme.id), effective, mode,
    );
    core.scheme.on(chrome, { withMode: true });
    chrome(core.scheme.effective(), core.scheme.mode());
  }

  /** The shown device's tone and volume are on show: say what they say. */
  const seen = () => {
    toneNote.seen(store.part('tone'));
    const volume = store.part('volume');
    if (volume) climb.paint(volume);
    else climb.drop();
  };

  // A page read afresh (a snapshot, a device switched to) is a first sight of the tone mode.
  store.on('model', () => {
    toneNote.forget();
    seen();
  });
  store.on('part', ({ device, name, data }) => {
    if (device !== store.shown()) return;
    if (name === 'tone') toneNote.seen(data);
    else if (name === 'volume') climb.paint(data);
  });
  store.on('volume', (data) => {
    if ((data.device ?? store.shown()) === store.shown()) climb.paint(store.part('volume'));
  });

  // A lost link ends the climb the page knew, and what the tone mode was is no longer known when
  // the link goes or comes back.
  let lost = false;
  store.on('state', (state) => {
    const now = state === 'lost';
    if (now) climb.drop();
    if (now !== lost) toneNote.forget();
    lost = now;
  });

  window.addEventListener('popstate', (event) => {
    const url = deviceOf(window, store, event.state);
    if (url) actions.showDevice(url, { push: false });
  });

  return core;
}
