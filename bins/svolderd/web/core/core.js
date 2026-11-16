/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The core a theme is given: the model of the page, the actions that change a device or the
// computer, the side of the page this viewer looks at (the device or the computer), the question a
// theme can ask, the stack of modals, the router of the history, the preferences of the theme
// mounted, light or dark, the lifecycle of a canvas, the host's analysis and its levels alone, the
// tone curve's maths and gestures (`eq`: the bands' drag, the points' hand, the table), the plots'
// SVG, the words and the clock, the section keeper, the frame (the menu's button, its menu, the
// picker of themes and the toasts), the keys (the frame's own list and the binding of a theme's key
// table), the registry of themes and the checks' driver for the device's facts (driver.js:
// test surface, as a theme's own driver is). `createCore` makes every module and joins what they
// leave to it: the store's state and events go to the climb's toast and the tone mode's note, a
// device shown pushes through the router, and another device on show closes every modal but a
// popup that stays (popups/listed.js).
//
// The preferences and the light or dark are bound to the theme being mounted (`prefs`, `scheme`
// and `theme` are of the theme last mounted; `mounted` is what its `mount()` returned, while it
// is on show), and the stream asks the daemon for what that theme
// needs (the theme is one owner of the stream's feeds, set as it mounts; see stream.js).

import { post, signedOut } from './api.js';
import { createActions } from './actions.js';
import { createAsk } from './ask.js';
import { createAnalysis } from './analysis.js';
import { createClimb } from './climb.js';
import { createComputer, createPending, POINT_TYPES } from './computer.js';
import { createDriver } from './driver.js';
import { createFrame } from './frame.js';
import { createKeys } from './keys.js';
import { menuOwns } from './menu.js';
import { createModals } from './modal.js';
import { REPEAT_AFTER_MS, REPEAT_EVERY_MS, repeat } from './repeat.js';
import {
  labelWords, markersIn, pointDrag, pointValues, POINT_KEYS, touchWatch,
} from './point-drag.js';
import { bootScheme, createPrefs, storageOf, writeChoice } from './prefs.js';
import { createRegistry } from './registry.js';
import { HELP, POPUPS } from './popups/listed.js';
import { createPopups } from './popups/popups.js';
import { createRouter } from './router.js';
import { createScheme, paintChrome } from './scheme.js';
import { createStore } from './store.js';
import { createStream } from './stream.js';
import { createTable, editCell, isLong } from './table.js';
import { createTitle } from './title.js';
import { createToneNote } from './tone-note.js';
import { dismissToast, toast } from './toast.js';
import { canvasLoop } from './lifecycle.js';
import { watchVisible } from './visible.js';
import * as clock from './clock.js';
import * as freeze from './freeze.js';
import * as plot from './plot.js';
import * as curve from './tone-curve.js';
import * as drag from './tone-drag.js';
import * as words from './words.js';

/**
 * The core over `page` (the page's model), listing `themes` (`[{id, label, blurb, module, styles,
 * schemes}]`), in `window`; `importer` loads a theme's module (the registry's own by default);
 * `pick(id)` is what a card of the frame's picker asks for (the page switches themes there);
 * `painted` is the scheme the server drew the first frame in (`'light'`, `'dark'`, or nothing);
 * `popups` is the list of the page's popups (popups/listed.js), each added to `core.popups`.
 */
export function createCore({
  page, themes = [], window, importer, pick = () => {}, painted = null, popups = POPUPS,
}) {
  const document = window.document;
  const fetchNow = (...args) => window.fetch(...args);
  const storage = storageOf(window);

  const store = createStore(page, { fetch: fetchNow, signedOut });
  const pending = createPending({ store, fetch: fetchNow });
  const modal = createModals({ document, fallback: () => document.getElementById('fx-button') });
  const ask = createAsk({ document, modal });
  const computer = createComputer({ store, storage, pending });
  // a device shown pushes its entry through the router, which shows a device when a step of the
  // history names another and keeps its popups on the stack
  const router = createRouter({
    window,
    modal,
    device: store.shown(),
    show: (url) => actions.showDevice(url, { push: false }),
    // the tab's title names the open popup and its page
    changed: () => core.title?.refresh(),
  });
  const actions = createActions({
    store, post, toast, history: router, location: window.location, pending, ask, computer,
    needs: () => core.stream.needed(),
  });
  const climb = createClimb({ toast, dismissToast, body: document.body });
  const toneNote = createToneNote({ toast, now: () => window.performance.now() });
  const analysis = createAnalysis({ EventSource: window.EventSource });
  // the light form of the same frames, `{ t, lv }` alone, for the VU meter (vu-model.js)
  const levels = createAnalysis({
    EventSource: window.EventSource, url: 'api/v1/host/frames?only=levels',
  });
  const registry = createRegistry(themes, {
    ...(importer ? { importer } : {}),
    beforeMount: (theme) => {
      core.mounted = null;
      bind(theme);
    },
    afterUnmount: () => {
      core.mounted = null;
    },
    afterMount: (theme, instance) => {
      core.mounted = instance;
      try {
        framed.mounted(theme);
      } catch (error) {
        console.warn('svolder: the frame could not take the theme\'s button', error);
      }
    },
  });

  /** The tone control's hands, by what they move: `bands` is the band drag (tone-drag.js); `points`
   * makes the hand on the computer's curve (point-drag.js), over this core's actions and computer,
   * and carries its helpers; `table` makes the points' table (table.js) and carries the typed cell,
   * the label words and the point types. */
  const tone = {
    bands: drag,
    points: Object.assign(
      (options) => pointDrag({
        actions, computer, toast: (text) => core.frame.toast(text), ...options,
      }),
      { pointValues, markersIn, POINT_KEYS, touchWatch },
    ),
    table: Object.assign(
      (options) => createTable({ computer, ...options }),
      { editCell, labelWords, POINT_TYPES, isLong },
    ),
  };

  /** The core, whose `prefs`, `scheme` and `theme` follow the theme mounted. */
  const core = {
    model: store,
    actions,
    computer,
    ask,
    modal,
    router,
    // the popups of the page, each loaded when it is first opened (popups/popups.js)
    popups: null,
    prefs: null,
    scheme: null,
    theme: null,
    // what the theme on show returned from `mount()`, `{ unmount, menuRows?, keys? }`, or `null`
    // (none on show, or its mount returned a function): the frame reads it before `theme`
    mounted: null,
    lifecycle: { watchVisible, canvasLoop },
    analysis,
    // the host's levels alone (`{ t, lv }` frames, ~45 B each), shared by its holders like
    // `analysis` and apart from it: a holder of one does not open the other
    levels,
    title: null,
    // press-and-hold repeat (repeat.js); `hold` is its old name, pinned for a theme written
    // against it
    repeat,
    hold: { repeat, AFTER_MS: REPEAT_AFTER_MS, EVERY_MS: REPEAT_EVERY_MS },
    curve,
    plot,
    eq: tone,
    // `eq` was `drag`: its old name, pinned, for a theme written against it
    drag: { ...drag, ...tone.points, ...tone.table, pointDrag: tone.points, table: tone.table },
    words,
    clock,
    // the section keeper: a section under the person's hand is kept from a redraw (freeze.js)
    freeze,
    frame: null,
    // the key table's hands (`bind`, `listed`, `zoneOf`, `zonesOf`) and the frame's own keys as
    // a list, `keys.frame` (keys.js); `?` is listed once the page has a Help popup
    keys: createKeys({
      document,
      menuOwns,
      base: () => ({
        canWrite: actions.canWrite(),
        help: core.popups?.names().includes(HELP) === true,
      }),
    }),
    registry,
    climb,
    toneNote,
    stream: null,
    // the checks' way to the device's facts, on the Devices popup's device page (driver.js)
    driver: createDriver({ store, router, document }),
  };

  core.title = createTitle({ store, document });
  core.popups = createPopups({ core, document, listed: popups });

  const framed = createFrame({ window, core, pick: (id) => pick(id) });
  core.frame = framed.frame;

  core.stream = createStream({
    store,
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

  // A page read afresh (a snapshot, a device switched to) is a first sight of the tone mode, and
  // what stood over the device shown before does not stand over another: a question, the picker
  // and every popup close, but a popup that says it stays (it is about the devices, not about
  // the one on show).
  let shownWas = store.shown();
  store.on('model', () => {
    if (store.shown() !== shownWas) {
      shownWas = store.shown();
      modal.closeAll({ device: true });
    }
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

  return core;
}
