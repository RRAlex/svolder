/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Paper: a spec sheet someone wrote on. Cream or charcoal graph paper, print in a serif,
// a red pencil and the device's own facts handwritten in blue in the margins. It is drawn in the
// browser from the core's model and changes the device only through the core's actions. Its mount
// draws the skeleton (draw/sheet.js), fills its sections and keeps them up to date (live.js); its
// unmount takes them away. Evaluating this module touches nothing in the page.
//
//   words.js     the fixed words and the lines worked out from the model
//   draw/        the sections, one function each; place.js puts their marks where they stand
//   live.js      the store's events on the sheet: which sections are drawn again, which are held
//   act.js       the log of what this page asked of the device
//   ruler.js     the volume's ruler as numbers; volume-wire.js the hands on it (gestures.js)
//   tone-wire.js the hands on the tone's words and table; graph.js on the EQ graph's points;
//                lists.js the paper list a written word opens
//   controls-wire.js the hands on the controls (draw/controls.js), scales included;
//                presets-wire.js on the presets and the device's, with ask.js's forms and questions
//   device-wire.js the hands on the transport, the power, `try now` and Identify, and the clock of
//                the ages (draw/playing.js, draw/amp.js, draw/device.js)
//   notes.js     which facts become handwritten notes, in whose words and which is red;
//                changes.js the last change; draw/notes.js draws them, marks.js sets them in
//                the band or the flow and draws the arrows and the ring
//   driver.js    the test driver the contract and completeness checks drive
//   preview.js   the picture of the theme in the frame's menu and picker

import { registerTheme } from '../../core/registry.js';
import { createAct } from './act.js';
import { createChanges } from './changes.js';
import { wireControls } from './controls-wire.js';
import { wireDevice } from './device-wire.js';
import { PHONE_MAX_PX, sheet } from './draw/sheet.js';
import { bound, driver } from './driver.js';
import { createLive } from './live.js';
import { placeNotes } from './marks.js';
import { wirePresets } from './presets-wire.js';
import { preview } from './preview.js';
import { NARROW_PX } from './ruler.js';
import { wireTone } from './tone-wire.js';
import { wireVolume } from './volume-wire.js';

/** The rows Paper gives the frame's menu: its light or dark, Auto following the system. */
function menuRows(core) {
  const row = (mode, label) => ({
    kind: 'radio',
    id: `paper-${mode}`,
    label,
    dataset: { scheme: mode },
    checked: () => core.scheme?.mode() === mode,
    disabled: () => null,
    choose: () => core.scheme?.set(mode),
  });
  return [{ kind: 'heading', text: 'Paper' }, row('auto', 'Auto'), row('light', 'Light'),
    row('dark', 'Dark')];
}

/** Mount Paper on `root`: draw the sheet, follow the model; returns the way to unmount. */
function mount(root, core, { signal }) {
  root.innerHTML = sheet(core.model.page());
  const narrow = window.matchMedia(`(max-width: ${PHONE_MAX_PX}px)`);
  const touch = window.matchMedia('(pointer: coarse)');
  const view = {
    phone: narrow.matches,
    coarse: touch.matches,
    state: core.model.state(),
    keys: core.keys,
    /** The step the person last asked of the volume, until the device reports it. */
    asked: null,
    /** The last change seen on the device (changes.js) and the log of what this page asked. */
    changes: createChanges(),
    log: () => [],
    /** About how wide the volume's ruler is, for the labels it can hold. */
    get rulerPx() {
      return Math.min(1180, window.innerWidth) - (this.phone ? 36 : 92);
    },
    get now() {
      return Date.now();
    },
  };
  const act = createAct(core, { now: () => Date.now() });
  view.log = act.log;
  const live = createLive({ core, root, signal, view });

  // The sheet is drawn again where the screen's kind changes (a phone turned, a window resized).
  narrow.addEventListener('change', (event) => {
    view.phone = event.matches;
    live.drawAll();
  }, { signal });
  touch.addEventListener('change', (event) => {
    view.coarse = event.matches;
    live.drawAll();
  }, { signal });

  // The ruler's labels change between every ten and every twenty dB where its width crosses one.
  let wide = view.rulerPx >= NARROW_PX;
  window.addEventListener('resize', () => {
    placeNotes(root, view);
    if ((view.rulerPx >= NARROW_PX) === wide) return;
    wide = !wide;
    live.draw('volume');
  }, { signal });
  // The notes' marks are drawn again once the fonts have changed what the words measure.
  globalThis.document?.fonts?.ready.then(() => {
    if (!signal.aborted) placeNotes(root, view);
  });

  // The words that go somewhere without changing the device: another device, the way out.
  root.addEventListener('click', (event) => {
    const word = event.target.closest?.('[data-go]');
    if (!word) return;
    if (word.dataset.go === 'device') core.actions.showDevice(word.dataset.device);
    else if (word.dataset.go === 'sign-out') core.actions.signOut();
  }, { signal });

  bound.core = core;
  live.drawAll();
  wireVolume({ root, core, live, act, view, signal });
  wireTone({ root, core, live, act, signal });
  wireControls({ root, core, live, act, signal });
  wirePresets({ root, core, live, act, signal });
  wireDevice({ root, core, act, signal });
  return function unmount() {
    bound.core = null;
    live.stop();
    root.replaceChildren();
  };
}

/** The theme as the registry lists it: it asks the daemon for the meters and the computer's player
 * (no analysis of the music: it draws no visualizer); its menu button is the ink circle over the
 * title's slot; its rows in the frame's menu are the light or dark. */
export const theme = {
  id: 'paper',
  label: 'Paper',
  needs: ['meters', 'host'],
  schemes: ['light', 'dark'],
  menuButton: { icon: 'ink', label: 'Menu' },
  menuRows,
  preview,
  driver,
  mount,
};

registerTheme(theme);
