/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Signal: the sound's way from the player to the headphones as one line on a dark
// ground, with the device you chose opened below it. It is drawn in the browser from the core's
// model and changes a device only through the core's actions. Its mount draws the skeleton, fills
// the line and the open area and keeps them up to date (live.js); its unmount takes them away.
// Evaluating this module touches nothing in the page.
//
//   line.js      the hops of the line from the model, and `openers`: what each kind of hop opens
//   draw/line.js the line drawn: the wire, its dots, the labels, the box; the dashes' loop
//   live.js      the store's events on the page: which sections are drawn again, which are held
//   draw/        the DAC's first screen (volume, tone, choices), an amp's open area, the shelf below
//                it (band table, presets, controls, the device's own presets, the facts) and the Now
//                playing bar
//   draw/player.js, draw/card.js   what the player's hop opens and the card of a device in trouble;
//                card-wire.js its Try now and its seconds
//   draw/computer.js, computer-wire.js   what the computer's hop opens: choosing it is the flip, and
//                the DAC's screen then acts on the computer (volume, curve, points, presets, facts)
//   draw/hop.js  the `+` sheet and the open area of a hop the person declared
//   volume-wire.js, tone-wire.js, controls-wire.js, presets-wire.js, facts-wire.js, playing-wire.js,
//                amp-wire.js, hops-wire.js, device-list.js, lists.js, ask.js   the hands on them
//   slim.js      the bar that pins at the top when the line scrolls away
//   strip-wire.js the finger on the phone's strip (the line under 720 px, drawn by draw/line.js)
//   act.js       the log of what this page asked of the device
//   words.js     the fixed words and the small lines worked out from the model
//   driver.js    the test driver the contract and completeness checks drive
//   preview.js   the picture of the theme in the frame's menu and picker

import { registerTheme } from '../../core/registry.js';
import { createAct } from './act.js';
import { ampScreen, viaScreen } from './draw/amp.js';
import { declaredScreen, hopTools } from './draw/hop.js';
import { card, heldCard, stateCard } from './draw/card.js';
import { computerOpen, computerScreen, computerShelf } from './draw/computer.js';
import { playerScreen } from './draw/player.js';
import { createFlow } from './draw/line.js';
import { drawShelf } from './draw/shelf.js';
import { modeRow } from './draw/choices.js';
import { toneBox } from './draw/tone.js';
import { volumeBox } from './draw/volume.js';
import { inertPage } from './draw/word.js';
import { wireAmp } from './amp-wire.js';
import { wireCard } from './card-wire.js';
import { wireComputer } from './computer-wire.js';
import { openDevices } from './device-list.js';
import { wireControls } from './controls-wire.js';
import { wireFacts } from './facts-wire.js';
import { wireHops } from './hops-wire.js';
import { bound, driver } from './driver.js';
import { createFold } from './fold.js';
import { createLive, keeper } from './live.js';
import {
  deviceOf, followedUrls, hopsOf, openers, shelves, shownHopId, sideOf,
} from './line.js';
import { HOPS } from './words.js';
import { wirePlaying } from './playing-wire.js';
import { wirePresets } from './presets-wire.js';
import { createRelease } from './press.js';
import { preview } from './preview.js';
import { wireSlim } from './slim.js';
import { wireStrip } from './strip-wire.js';
import { wireTone } from './tone-wire.js';
import { wireVolume } from './volume-wire.js';

export { openers };

/** The first screen of a DAC: the volume and the tone curve, with the mode under it on a phone (on a
 * desktop it heads the shelf's presets column). */
function dacScreen(page, _hop, view, stale = false) {
  const device = page.device;
  // What was last heard of a device in trouble stays in view, dimmed and out of reach.
  const mark = stale ? ' sg-stale' : '';
  const lock = stale ? ' inert' : '';
  return `<section id="sg-volume" class="sg-vol${mark}"${lock}>${volumeBox(device, view)}</section>`
    + `<section id="sg-tone" class="sg-tone${mark}"${lock}>${toneBox(device)}${view.phone() ? modeRow(device) : ''}</section>`;
}

/** The families whose first screen is the DAC's: what the model has of them is drawn the same way. */
const SCREENS = { dac: dacScreen, preamp: dacScreen };

/** The family of the device a hop is, from the fleet's list. */
const familyOf = (page, hop) => (page.fleet?.devices ?? []).find((device) => device.url === hop.url)?.family;

/** The model of the device a hop is, its words for a viewer who may not write added. A followed
 * device is read by the stream: null until its first reading. */
function modelOf(page, hop) {
  const device = deviceOf(page, hop.url);
  return device ? { ...device, inert: Boolean(device.inert) || inertPage(page) } : null;
}

/** The open area of an amp: its power, its meters, the amp itself; Unlink where a hop links it. */
function ampOpener(page, hop) {
  const row = (page.fleet?.hops ?? []).find((one) => one.device === hop.url);
  const dac = row && (page.fleet?.devices ?? []).find((device) => device.url === row.after);
  const tools = row ? hopTools(row, page.fleet.hops, inertPage(page), { remove: false, form: true }) : '';
  return ampScreen(modelOf(page, hop), { dac: dac?.name ?? null, linkId: row?.id ?? null, tools });
}

/** What a device hop opens, by the family of the device: a DAC's first screen (only for the device
 * shown: a followed one has no part of the page to draw from), an amp's area (shown or followed). */
openers.device = (page, hop, view) => {
  if (hop.via) return viaScreen(hop);
  const family = familyOf(page, hop);
  if (family === 'amp') return ampOpener(page, hop);
  if (hop.url !== page.shown || !(family in SCREENS)) return '';
  // A device in trouble (its card) or a page that waits or rests: the card first, then what was last
  // heard dimmed and inert (P6).
  const held = heldCard(page) ?? stateCard(view.state);
  if (!held) return page.device ? SCREENS[family](page, hop, view) : '';
  const shown = card(held, { quiet: inertPage(page), now: view.now });
  if (!page.device) return shown;
  // A fault on a connected device leaves its controls live; only a link that is down stills them.
  if (held.kind === 'fault') return shown + SCREENS[family](page, hop, view);
  return shown + SCREENS[family]({ ...page, device: { ...page.device, inert: true } }, hop, view, true);
};

/** What the player opens: Now playing grown, the transport and the path in words (P4). */
openers.player = (page, _hop, view) => playerScreen(page, view.now);

/** The parts of the computer side (the volume and the tone control the core reads out of the page's
 * host), or nothing while the computer's controls are not what is shown (not served, unavailable). */
function computerParts(page, view) {
  const computer = view.computer;
  if (computer?.side() !== 'computer') return null;
  return {
    volume: computer.volumeOf(page),
    tone: computer.toneOf(page),
    picked: view.pick?.device === page.shown ? view.pick.n : null,
  };
}

/** What the computer opens: choosing its hop is the flip, so the DAC's first screen, drawn for the
 * computer (the volume and the curve); the route's facts alone where its controls are not shown. */
openers.computer = (page, _hop, view) => {
  const parts = computerParts(page, view);
  return parts ? computerOpen(parts, { phone: view.phone(), inert: inertPage(page), picked: parts.picked })
    : computerScreen(page);
};

/** What a page with no hop to open says: the card of the page, when it has one. */
openers.none = (page, _hop, view) => {
  const held = heldCard(page) ?? stateCard(view.state);
  return held ? card(held, { quiet: inertPage(page), now: view.now }) : '';
};

/** What a declared hop opens: its own words and the buttons that change them (P3). */
openers.declared = (page, hop) => declaredScreen(page, hop, inertPage(page));

/** The shelf of the computer's hop: the points, the mode and the presets, then the route's facts. */
shelves.computer = (page, _hop, view) => {
  const parts = computerParts(page, view);
  return parts ? computerShelf(page, parts, { phone: view.phone(), inert: inertPage(page), picked: parts.picked })
    : '';
};

/** The shelf of a device hop: the rest of what the model has of the device. An amp's power and
 * Identify are in its area. */
shelves.device = (page, hop, view) => {
  const family = familyOf(page, hop);
  if (family === 'amp') {
    const device = modelOf(page, hop);
    return device ? drawShelf(page, device, view, { power: true, identify: true }) : '';
  }
  return hop.url === page.shown && page.device && family in SCREENS
    ? drawShelf(page, page.device, view, { choices: true, phone: view.phone() }) : '';
};

/** The skeleton: the line's band with the slot the frame's button sits over, the quiet word, the
 * open area and the place of the Now playing bar. */
const SKELETON = '<div id="signal" class="sg"><div class="sg-bandwrap"><span class="sg-slot"></span>'
  + '<div id="sg-line" class="sg-band" role="listbox" aria-orientation="horizontal" '
  + 'aria-label="The signal path"></div><p id="sg-stub" class="sg-stub"></p><p id="sg-said" class="sg-said" role="status"></p>'
  + '</div>'
  + '<nav id="sg-slim" class="sg-slim" aria-label="The signal path" hidden></nav>'
  + '<div id="sg-open" class="sg-open"></div><div id="sg-shelf" class="sg-shelf"></div>'
  + '<footer id="sg-now" class="sg-now"></footer><div id="sg-sheet" class="sg-sheetwrap" hidden></div></div>';

/** Mount Signal on `root`: draw the page, follow the model; returns the way to unmount. */
function mount(root, core, { signal }) {
  root.innerHTML = SKELETON;
  bound.core = core;
  const band = root.querySelector('#sg-line');
  const slot = root.querySelector('.sg-slot');
  const act = createAct(core, { now: () => Date.now() });
  const flow = createFlow(band);
  const loop = core.lifecycle.canvasLoop({
    canvas: band,
    draw: (dt) => flow.step(dt),
    resize: () => {},
    watchVisible: core.lifecycle.watchVisible,
    matchMedia: (query) => window.matchMedia(query),
  });
  // Under 720 px the line is the strip of P7b; the same hops and the same listbox, drawn for the width.
  const narrow = window.matchMedia('(max-width: 719px)');
  const view = {
    phone: () => narrow.matches,
    state: core.model.state(),
    /** The hop chosen (a `Hop.id`): the shown device's until the person chooses another. */
    chosen: core.computer.kept() === 'computer' ? null
      : (core.model.shown() ? shownHopId(core.model.shown()) : null),
    hops: () => hopsOf(core.model.page(), core.model.shown(), view.state, view.chosen),
    /** The core's computer side (the flip is this page's choice of the computer's hop), and the
     * point picked in the table. */
    computer: core.computer,
    /** Whether the person has chosen a hop (else the shown device's, or the computer's when that is
     * the side kept, is the one chosen). */
    byHand: false,
    pick: { n: null, device: null },
    /** The device the controls below the line act on: the hop chosen when it is a device (a linked
     * amp is not the device shown), else the shown one. */
    target() {
      const hop = view.hops().find((one) => one.id === view.chosen);
      return hop?.kind === 'device' && hop.url ? hop.url : core.model.shown();
    },
    device: () => core.model.device(view.target()),
    log: act.log,
    /** The step of the volume asked for and not yet read back (`{ raw, at, answered }`), and the
     * clock the drawing goes by (ms). */
    asked: null,
    now: Date.now(),
    /** Whether the device's facts are open (kept in the preferences, and in memory). */
    facts: createFold(() => core.prefs),
    flow,
    /** The dashes move while the player plays and the stream is open, and while the band is on
     * show (the loop's own rule); otherwise the loop is not running. */
    motion() {
      const playing = core.model.page().host?.track?.status === 'playing' && view.state === 'open';
      if (playing) loop.start();
      else loop.stop();
    },
    /** Choose the hop `id`: the box moves to it and the open area is drawn for it. */
    choose(id) {
      const hop = view.hops().find((one) => one.id === id);
      if (!hop) return;
      if (id !== view.chosen) live.choosing();
      view.chosen = id;
      view.byHand = true;
      // The side is the computer's exactly while its hop is chosen; flipping writes nothing.
      const side = sideOf(id);
      if (core.computer.kept() !== side) core.computer.flip(side);
      // The line is drawn again too: the shown device writes its volume under its name while
      // another hop is chosen.
      live.draw('line');
    },
  };
  const live = createLive({ core, root, signal, view });
  narrow.addEventListener('change', () => live.draw('line'), { signal });

  // The devices the line links after the shown one are followed by the stream, for their power and
  // their meters; the shown device is not in the list.
  const follow = () => {
    const shown = core.model.shown();
    core.stream.follow(followedUrls(view.hops(), shown));
  };
  const stopFollowing = ['model', 'fleet'].map((event) => core.model.on(event, follow));
  stopFollowing.push(core.model.on('fleet', () => closeDevices()));
  follow();

  /** A tap on a hop chooses it; a tap on the name of the shown device, already chosen, opens the list
   * of the devices to show another. */
  let devices = null;
  const closeDevices = () => {
    devices?.close(false);
    devices = null;
  };
  live.onChoice(closeDevices);
  function activate(option, byName) {
    const id = option.dataset.hop;
    // The DAC before a linked amp is another device: a click, a tap or Enter shows it, as the device
    // list does; the arrows only box it, like every other hop.
    const via = view.hops().find((one) => one.id === id && one.via);
    if (via) {
      core.actions.showDevice(via.url);
      return;
    }
    const shownId = core.model.shown() ? shownHopId(core.model.shown()) : null;
    if (byName && id === view.chosen && id === shownId) {
      closeDevices();
      devices = openDevices({ opener: option, core, signal });
      return;
    }
    view.choose(id);
  }
  band.addEventListener('click', (event) => {
    const option = event.target.closest?.('[data-hop]');
    if (option) activate(option, Boolean(event.target.closest('b')));
  }, { signal });
  band.addEventListener('keydown', (event) => {
    const focused = event.target.closest?.('[data-hop]');
    if ((event.key === 'Enter' || event.key === ' ') && focused) {
      event.preventDefault();
      activate(focused, true);
      return;
    }
    const options = [...band.querySelectorAll('[role=option]')];
    const at = options.findIndex((option) => option.dataset.hop === view.chosen);
    const to = {
      ArrowLeft: Math.max(0, at - 1),
      ArrowRight: Math.min(options.length - 1, at + 1),
      Home: 0,
      End: options.length - 1,
    }[event.key];
    if (to === undefined || options.length === 0) return;
    event.preventDefault();
    view.choose(options[to].dataset.hop);
    band.querySelector(`[data-hop="${CSS.escape(view.chosen)}"]`)?.focus({ preventScroll: true });
  }, { signal });

  // One count of the holds on the open area, for the knob, a point and an open list together.
  const keep = keeper(live, 'open');
  wireVolume({ root, core, live, act, view, keep, signal });
  wirePlaying({ root, core, act, signal });
  wireCard({ root, core, act, signal });
  // The shelf's own count of holds, for what is typed, asked or open in it.
  const shelfKeep = keeper(live, 'shelf');
  wireTone({ root, core, live, act, keep, shelfKeep, signal });
  const release = createRelease(root.ownerDocument, shelfKeep, signal);
  wireAmp({ root, core, live, act, view, keep, signal });
  root.querySelector('#sg-open').addEventListener('click', (event) => {
    if (!event.target.closest?.('[data-act="show-device"]')) return;
    const hop = view.hops().find((one) => one.id === view.chosen && one.via);
    if (hop) core.actions.showDevice(hop.url);
  }, { signal });
  const hops = wireHops({ root, core, live, view, keep, signal });
  bound.add = hops.add;
  bound.canAdd = hops.canAdd;
  wireControls({ root, core, live, act, view, keep: shelfKeep, release, signal });
  wirePresets({ root, core, live, act, view, keep: shelfKeep, release, signal });
  wireComputer({ root, core, live, act, view, keep, shelfKeep, release, signal });
  wireFacts({ root, core, live, view, signal });
  wireStrip({ band, hops: view.hops, chosen: () => view.chosen, choose: view.choose, signal });
  wireSlim({ bar: root.querySelector('#sg-slim'), band: root.querySelector('.sg-bandwrap'),
    // a tap on the DAC before a linked amp shows the DAC, as a tap on the line does
    choose: (id) => {
      const via = view.hops().find((one) => one.id === id && one.via);
      if (via) core.actions.showDevice(via.url);
      else view.choose(id);
    },
    signal });

  // The pinned block's height (the strip, the quiet lines under it, the notch's padding): focus keeps clear of it.
  const wrap = root.querySelector('.sg-bandwrap');
  if (typeof ResizeObserver === 'function') {
    const pinned = new ResizeObserver(() => {
      root.querySelector('#signal')?.style.setProperty('--sg-pinned', `${wrap.offsetHeight}px`);
    });
    pinned.observe(wrap);
    signal.addEventListener('abort', () => pinned.disconnect(), { once: true });
  }
  // On a phone the strip is pinned at the top, so the button is placed against the window there and
  // never has to chase the page as it scrolls.
  core.frame.anchor(slot, { fixed: true });
  live.drawAll();
  return function unmount() {
    bound.core = null;
    bound.add = null;
    bound.canAdd = null;
    for (const stop of stopFollowing) stop();
    core.stream.follow([]);
    loop.stop();
    live.stop();
    core.frame.anchor(null);
    root.replaceChildren();
  };
}

/** The theme's row in the frame's menu: Add device…, the sheet of what comes after the device. */
function menuRows() {
  return [{
    kind: 'action',
    id: 'add-device',
    label: HOPS.addMenu,
    elementId: 'menu-add-device',
    available: () => Boolean(bound.add && bound.canAdd?.() && bound.core?.model.shown()),
    run: () => bound.add?.(),
  }];
}

/** The theme as the registry lists it: it asks the daemon for the meters and the computer's
 * player; dark only, no effects; the frame's menu has its Add device row; its menu button is the
 * ink circle over the line band's top right. */
export const theme = {
  id: 'signal',
  label: 'Signal',
  needs: ['meters', 'host'],
  schemes: ['dark'],
  menuButton: { icon: 'ink', label: 'Menu' },
  menuRows,
  preview,
  driver,
  mount,
};

registerTheme(theme);
