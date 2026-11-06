/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Night drive: an outrun road that moves with the music. The sky and the sun are drawn
// on one canvas, the ridge of the live spectrum and the floor of lit cells (a row of the music at
// each step, rolling toward you) on another, over it. It is drawn in the browser from the core's
// model and changes a device only through the core's actions. Evaluating this module touches
// nothing in the page.
//
//   camera.js    where the road's points fall on the scene; axis.js the one frequency axis and the
//                24 columns; music.js the music's levels; floor.js the floor's rows
//   draw/        the canvases' drawing: the sky and the sun, the road and the ridge, the colours
//   scene-wire.js the loop that reads the analysis, rolls the floor and draws the frame
//   live.js      the store's events on the page: which sections are drawn again, which are held
//   draw/sheet.js the neon sheet's markup (draw/modes.js, bands.js, presets.js its sections);
//                sheet-wire.js opens and closes it and works the mode and the input, bands-wire.js the
//                band table and the loudness, presets-wire.js the presets, ask.js the questions they ask
//   draw/slots.js, draw/controls.js, draw/facts.js, draw/meters.js the rest of the sheet (the device's slots, its
//                controls, its facts and faults, an amp's meters); controls-wire.js and facts-wire.js the hands on
//                them, fold.js keeps the facts' fold; draw/gauges.js an amp's gauges in the dash
//   tune.js      the tone control on the mountains as numbers (the map, the bubbles, the morph);
//                tune-wire.js the hands on it, draw/tune.js the bubbles
//   sky.js       the sky's rare events, by the clock; motion.js the Full, Calm and Still levels
//   rest.js      the cursor over the scene hiding after 2 s without a move
//   states.js    how the page stands (ok, out of reach, lost, reading again, no analysis) and the quiet line's words;
//                draw/card.js the card of a device out of reach, card-wire.js the card
//                in the scene, the quiet line and what goes out of reach, with Try now
//   hud.js       the HUD's words from the model; draw/hud.js and draw/volume.js its markup and the
//                outrun volume; hud-wire.js draws them and fades them after the idle time
//   draw/dash.js the dash's markup (the keys, the input, the track, the transport); dash-wire.js draws
//                it and works its keys through the core's hold and actions; act.js logs the writes,
//                lists.js is the list a long input opens
//   words.js     the fixed words
//   debug.js     the readout and the bisect switches the address asks for (`?nd-debug`); debug-wire.js its box;
//                stats.js the draws' times and rate
//   driver.js    what the checks reach into
//   preview.js   the picture of the theme in the frame's menu and picker

import { registerTheme } from '../../core/registry.js';
import { createAct } from './act.js';
import { wireBands } from './bands-wire.js';
import { wireCard } from './card-wire.js';
import { wireControls } from './controls-wire.js';
import { wireDash } from './dash-wire.js';
import { wireDashFade } from './dash-fade.js';
import { ASKED } from './debug.js';
import { wireDebug } from './debug-wire.js';
import { bound, driver } from './driver.js';
import { wireFacts } from './facts-wire.js';
import { createFold } from './fold.js';
import { wireHud } from './hud-wire.js';
import { PARTS_OF, createLive } from './live.js';
import { MOTIONS, motionOf } from './motion.js';
import { wirePresets } from './presets-wire.js';
import { createRest } from './rest.js';
import { preview } from './preview.js';
import { wireScene } from './scene-wire.js';
import { wireSheet } from './sheet-wire.js';
import { MOTION_WORDS, SCENE } from './words.js';

const SKELETON = `<div id="nightdrive" class="nd"><div class="nd-scene" role="img" aria-label="${SCENE.scene}">`
  + '<canvas id="nd-sky" class="nd-canvas"></canvas><canvas id="nd-road" class="nd-canvas"></canvas></div>'
  + '<div id="nd-layer" class="nd-layer"><span id="nd-slot" class="nd-slot"></span><div class="nd-hud">'
  + '<div class="nd-top"><span class="nd-left"></span><span class="nd-right" aria-live="polite"></span></div>'
  + '<div class="nd-vol"></div><div class="nd-eq" hidden></div><div class="nd-quiet" role="status" hidden></div></div>'
  + '<div id="nd-dash" class="nd-dash"></div><div id="nd-cardbox" class="nd-cardbox" hidden></div></div>'
  + '<div id="nd-sheet-wrap" class="nd-sheet-wrap" hidden></div></div>';

/** Mount Night drive on `root`: draw the scene, follow the model; returns the way to unmount. */
function mount(root, core, { signal }) {
  root.innerHTML = SKELETON;
  bound.core = core;
  const page = root.querySelector('#nightdrive');
  const view = {
    /** A phone: a narrow window, or a short one (a phone held sideways). */
    phone: () => innerWidth < 720 || innerHeight < 500,
    portrait: () => innerHeight > innerWidth,
    /** The motion level: `full`, `calm` or `still` (the menu's rows, kept per browser). */
    motion: motionOf(core.prefs, globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches),
    state: core.model.state(),
    /** The music analysis's state as the stream said it, `{ state, why }`, `null` before it has (scene-wire.js). */
    capture: null,
    /** The tuning on the mountains and the sheet of the device: their parts come with them. */
    tune: null,
    sheet: null,
    /** Whether the device's facts are unfolded on the sheet (fold.js: kept per browser). */
    facts: createFold(() => core.prefs),
    /** The volume step this page asked for and is waiting on, `{ raw, at }`, so the reading that answers it is its own. */
    asked: null,
    /** Open the sheet of the device from `opener` (the dash's `DEVICE ▴` or the menu's row) and close it
     * (sheet-wire.js). */
    openSheet: () => {},
    closeSheet: () => {},
  };
  let hud = null;
  let dash = null;
  let sheet = null;
  let cards = null;
  const live = createLive({
    core,
    signal,
    view,
    sections: {
      scene: {
        parts: ['card'],
        draw: (model) => {
          page.dataset.state = view.state;
          cards?.draw(model);
        },
      },
      hud: { parts: PARTS_OF.hud, draw: (model) => hud?.draw(model) },
      dash: { parts: PARTS_OF.dash, draw: (model) => dash?.draw(model) },
      sheet: { parts: PARTS_OF.sheet, draw: (model) => sheet?.draw(model) },
    },
  });
  const act = createAct(core, { now: () => Date.now() });
  cards = wireCard({
    root, core, view, act, signal,
  });
  dash = wireDash({
    root, core, live, view, signal, act,
  });
  sheet = wireSheet({
    root, core, live, act, view, signal,
  });
  wireBands({
    core, live, sheet, view, signal,
  });
  wirePresets({
    core, act, sheet, signal,
  });
  wireControls({
    core, live, act, sheet, signal,
  });
  wireFacts({
    core, live, view, sheet, signal,
  });
  bound.sheet = (opener) => view.openSheet(opener);
  hud = wireHud({ root, core, view });
  wireDashFade({
    page, fade: hud.fade, live, signal,
  });
  bound.view = view;
  const off = ASKED?.off ?? new Set();
  for (const name of off) page.classList.add(`nd-${name}`);
  const scene = wireScene({
    root, core, view, live, signal, off,
  });
  bound.scene = scene;
  const readout = ASKED && wireDebug({
    layer: root.querySelector('#nd-layer'), scene, view, canvas: root.querySelector('#nd-road'), off: [...off],
  });
  core.frame.anchor(root.querySelector('#nd-slot'), { fixed: true });
  const rest = createRest({
    root: page, doc: root.ownerDocument, fine: !globalThis.matchMedia('(pointer: coarse)').matches,
    keepOut: () => [...root.querySelectorAll('.nd-left, .nd-right, .nd-eq, .nd-odb, .nd-novol, .nd-quiet')],
  });
  live.drawAll();
  return function unmount() {
    bound.core = null;
    bound.sheet = null;
    bound.scene = null;
    bound.view = null;
    view.tune = null;
    sheet.close(false);
    rest.stop();
    readout?.stop();
    scene.stop();
    live.stop();
    hud.stop();
    core.frame.anchor(null);
    root.replaceChildren();
  };
}

/** The theme's rows in the frame's menu: the motion levels, a rule and the device's sheet. */
function menuRows() {
  return [
    { kind: 'heading', text: SCENE.motion },
    ...MOTIONS.map((id) => ({
      kind: 'radio',
      id,
      label: MOTION_WORDS[id],
      dataset: { motion: id },
      checked: () => bound.view?.motion === id,
      disabled: () => null,
      choose() {
        if (!bound.view) return;
        bound.view.motion = id;
        bound.core?.prefs.set('motion', id);
      },
    })),
    { kind: 'sep' },
    {
      kind: 'action',
      id: 'nd-sheet',
      label: SCENE.device,
      elementId: 'menu-nd-sheet',
      available: () => Boolean(bound.sheet && bound.core?.model.shown()),
      run: () => bound.sheet?.(),
    },
  ];
}

/** The theme as the registry lists it: it asks the daemon for the meters, the computer's player and
 * the music's analysis; dark only; its menu button is the waves over the scene's top right, and its menu rows are the motion levels. */
export const theme = {
  id: 'nightdrive',
  label: SCENE.label,
  needs: ['meters', 'host', 'analysis'],
  schemes: ['dark'],
  menuButton: { icon: 'waves', label: SCENE.menu },
  preview,
  driver,
  menuRows,
  mount,
};

registerTheme(theme);
