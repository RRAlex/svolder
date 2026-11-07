/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page kept up to date: what the core's store says, put into the sections of the page. The
// line (`#sg-line`, with the quiet word beside it) is drawn from the hops of line.js; the open
// area (`#sg-open`) is what the chosen hop's opener (`openers[kind]`) draws below it. A change of
// the whole model, of the fleet, the host or the shown device draws both (and the shelf below the
// first screen, `#sg-shelf`, the slim bar, `#sg-slim`, and the Now playing bar, `#sg-now`, which
// shows the player); a part of the shown device draws the sections that show it. The stream's state fades or stills the page.
//
// A section in use is not drawn again under the person's hand: `hold(name, true)` keeps it, and
// what came meanwhile is drawn from the newest model when `hold(name, false)` lets it go.
//
// A hop chosen that is no longer on the line (a linked amp unplugged, a declared hop removed, the
// player quit) and a device switch give the choice back to the shown device.

import { metersInPlace } from './draw/amp.js';
import { line, place, placeStrip, strip, stub } from './draw/line.js';
import { drawPlaying } from './draw/playing.js';
import { keepChoice, openers, shelves, shownHopId } from './line.js';
import { morph } from './morph.js';
import { placeOpen } from './place.js';
import { drawSlim, volumeWords } from './slim.js';
import { STATES } from './words.js';

/** The parts of the shown device each section shows, by the model's names; `'*'` is every part. */
export const PARTS_OF = {
  line: ['playing', 'facts', 'link', 'card'],
  open: '*',
  shelf: ['controls', 'tone', 'presets', 'device_presets', 'power'],
};

/** The stream's states in which the page cannot be used: faded (lost) or still (waiting). */
const STILL = { lost: 'sg-faded', waiting: 'sg-waiting' };

/** What the page says under the line while the stream is lost or the page reads the device again. */
const SAID = { lost: STATES.reconnecting, waiting: STATES.waitingLine };

/** A selector for the hop of `band` that has the focus, to find its twin after the line is drawn
 * again. */
function focusOf(band) {
  const active = globalThis.document?.activeElement;
  if (!active || !band.contains?.(active)) return null;
  return active.dataset?.hop ? `[data-hop="${CSS.escape(active.dataset.hop)}"]` : null;
}

/** A selector for the element of `box` that has the focus, to find its twin after the box is drawn
 * again: its id; nothing when the focus is elsewhere. */
function focusId(box) {
  const active = globalThis.document?.activeElement;
  return active?.id && box.contains?.(active) ? `#${CSS.escape(active.id)}` : null;
}

/**
 * A counted hold of the section `name`, for gestures and open lists that may overlap: held from the
 * first `take()` and let go at the `drop()` that matches the last. `around(fn)` runs `fn` with a
 * hold taken and gives it back if `fn` throws. `dropLater()` gives one back on the next task,
 * unless `reset()` came first. `reset()` lets go of every hold at once (the shown device was
 * replaced). `hand({ pending, ms })` is the hold of a thing in the hand (a scale): `grip()` on every
 * move or key, `letGo()` when the hand is away, which gives the hold back once `pending()` (a
 * promise: the writes not yet answered) is settled and `ms` have passed, unless a new grip came
 * meanwhile (a slow answer must not release a hold the next gesture took).
 */
export function keeper(live, name) {
  let holds = 0;
  /** Counts the resets: a give-back planned before one is for a hold the reset has let go. */
  let epoch = 0;
  const keep = {
    take() {
      holds++;
      live.hold(name, true);
    },
    drop() {
      holds = Math.max(0, holds - 1);
      if (holds === 0) live.hold(name, false);
    },
    dropLater() {
      const planned = epoch;
      setTimeout(() => {
        if (epoch === planned) keep.drop();
      }, 0);
    },
    around(fn) {
      keep.take();
      try {
        return fn();
      } catch (error) {
        keep.drop();
        throw error;
      }
    },
    reset() {
      epoch++;
      holds = 0;
      live.hold(name, false);
    },
    hand({ pending, ms }) {
      let held = false;
      let generation = 0;
      let heldIn = 0;
      let timer = null;
      return {
        grip() {
          generation++;
          clearTimeout(timer);
          if (held) return;
          held = true;
          heldIn = epoch;
          keep.take();
        },
        letGo() {
          clearTimeout(timer);
          const mine = generation;
          timer = setTimeout(async () => {
            await pending();
            if (generation !== mine || !held) return;
            held = false;
            if (heldIn === epoch) keep.drop();
          }, ms);
        },
        /** The hold was reset from outside: forget it, and any release on its way. */
        forget() {
          generation++;
          clearTimeout(timer);
          held = false;
        },
        stop: () => clearTimeout(timer),
      };
    },
  };
  return keep;
}

/**
 * Keep the page in `root` up to date for `core`, until `signal` aborts. `view` is what the
 * sections draw by: `{ state, chosen, hops(), flow, motion(), asked, now }`. Returns `{ drawAll, draw,
 * hold, held, onSwitch, onReading, stop }`.
 */
export function createLive({ core, root, signal, view }) {
  const store = core.model;
  const holding = new Set();
  const stale = new Set();
  const off = [];
  const switchers = [];
  const choosers = [];
  /** The device the page showed when it last looked: a change of it is a switch. */
  let seen = store.shown();

  /** The hop to keep chosen, now: kept while the line has it, else the shown device's, or the
   * computer's when this viewer's side is the computer's and the person has not chosen a hop by
   * hand (a load, a device shown). A choice that was lost (the amp left, the hop was removed)
   * closes what was open for it. With no line there is nothing to choose among yet. */
  function reconcile() {
    const hops = view.hops();
    if (hops.length === 0) return;
    const shown = store.shown() ? shownHopId(store.shown()) : null;
    const before = view.chosen;
    view.chosen = keepChoice(view.byHand ? before : null, hops, shown, core.computer.kept());
    if (view.chosen !== before) for (const fn of choosers) fn();
  }

  /** The URL of the device the chosen hop is, whether it is the shown one or a followed one. */
  function chosenUrl() {
    const hop = view.hops().find((one) => one.id === view.chosen);
    return hop?.kind === 'device' ? hop.url : null;
  }

  const SECTIONS = {
    line(page) {
      const band = root.querySelector('#sg-line');
      const hops = view.hops();
      const shown = store.shown() ? shownHopId(store.shown()) : null;
      const focused = focusOf(band);
      const canList = (page.fleet?.devices?.length ?? 0) > 1;
      // A phone has the strip, a desktop the line: one hop model, one listbox, drawn for the width.
      // The elements of the line are kept across draws (`morph`): a press on a hop and its release stay
      // on one element whatever is drawn between them, and a track in the middle of its glide goes on.
      // A track drawn anew starts from where the old one stands (its computed transform), not from
      // the end its style names.
      const old = band.querySelector('.sg-slide');
      // A finger is dragging the track (strip-wire.js): the draw leaves it where the finger has it.
      const dragged = 'dragging' in band.dataset && old ? old.style.transform : null;
      const at = old ? window.getComputedStyle(old).transform : '';
      const from = at === 'none' ? '' : at;
      morph(band, view.phone() ? strip(hops, shown, view.chosen, canList) : line(hops, shown, view.chosen, canList));
      if (view.phone()) placeStrip(band, band.querySelector('.sg-slide') === old ? '' : from);
      else place(band);
      const track = band.querySelector('.sg-slide');
      if (dragged !== null && track === old) {
        track.classList.add('sg-drag');
        track.style.transform = dragged;
      }
      if (focused) band.querySelector(focused)?.focus({ preventScroll: true });
      root.querySelector('#sg-stub').textContent = stub(page);
      view.flow.apply();
      view.motion();
    },
    open(page) {
      const hop = view.hops().find((one) => one.id === view.chosen);
      const open = root.querySelector('#sg-open');
      const focused = focusId(open);
      view.now = Date.now();
      open.dataset.hop = hop?.id ?? '';
      open.innerHTML = (hop ? openers[hop.kind] : openers.none)?.(page, hop, view) ?? '';
      placeOpen(open);
      if (focused) open.querySelector(focused)?.focus({ preventScroll: true });
    },
    shelf(page) {
      const hop = view.hops().find((one) => one.id === view.chosen);
      const shelf = root.querySelector('#sg-shelf');
      const focused = focusId(shelf);
      shelf.innerHTML = hop ? (shelves[hop.kind]?.(page, hop, view) ?? '') : '';
      placeOpen(shelf);
      // The points' table keeps its scroll across the draw (the computer's shelf).
      view.shelfDrawn?.(shelf);
      if (focused) shelf.querySelector(focused)?.focus({ preventScroll: true });
    },
    slim(page) {
      root.querySelector('#sg-slim').innerHTML = drawSlim(view.hops(), view.chosen,
        store.shown() ? shownHopId(store.shown()) : null, volumeWords(page.device));
    },
    now(page) {
      const bar = root.querySelector('#sg-now');
      // The player's own area has the transport and the progress: the bar is not drawn then.
      bar.hidden = view.chosen === 'player';
      if (bar.hidden) {
        bar.innerHTML = '';
        return;
      }
      const focused = focusId(bar);
      bar.innerHTML = drawPlaying(page);
      placeOpen(bar);
      if (focused) bar.querySelector(focused)?.focus({ preventScroll: true });
    },
  };

  /** Draw the section `name` from the model as it is; nothing while it is held. */
  function paint(name) {
    if (holding.has(name)) {
      stale.add(name);
      return;
    }
    stale.delete(name);
    if (name === 'line') reconcile();
    SECTIONS[name](store.page());
  }

  /** The page's classes for the stream's state: faded while it is lost, still while the page
   * waits for the device again, both out of reach of the person. */
  function stillness() {
    const page = root.querySelector('#signal');
    if (!page) return;
    for (const [state, name] of Object.entries(STILL)) page.classList.toggle(name, view.state === state);
    page.inert = view.state in STILL;
    root.querySelector('#sg-said').textContent = SAID[view.state] ?? '';
  }

  function draw(name) {
    paint(name);
    if (name === 'line') {
      paint('open');
      paint('shelf');
      paint('slim');
      paint('now');
    } else if (name === 'open') {
      paint('slim');
    }
  }

  function drawAll() {
    view.state = store.state();
    stillness();
    draw('line');
  }

  const listen = (event, fn) => {
    off.push(store.on(event, fn));
  };

  /** Another device is shown than the one the page was drawn for: the choice goes back to it. */
  function switched() {
    if (store.shown() === seen) return;
    seen = store.shown();
    // Nothing is chosen by hand on the new device: its own hop, or the computer's while that is the side.
    view.byHand = false;
    view.chosen = core.computer.kept() === 'computer' ? null : (store.shown() ? shownHopId(store.shown()) : null);
    for (const fn of switchers) fn();
  }

  listen('model', () => {
    switched();
    drawAll();
  });
  listen('device', ({ device }) => {
    if (device !== store.shown()) return;
    switched();
    drawAll();
  });
  listen('part', ({ device, name }) => {
    const shows = (parts) => parts === '*' || parts.includes(name);
    if (device !== store.shown()) {
      // A followed device: only the hop chosen is on show.
      if (device !== chosenUrl()) return;
      draw('open');
      if (shows(PARTS_OF.shelf)) draw('shelf');
      return;
    }
    if (shows(PARTS_OF.line)) {
      draw('line');
      return;
    }
    if (shows(PARTS_OF.open)) draw('open');
    if (shows(PARTS_OF.shelf)) draw('shelf');
  });
  // A followed device read whole (its first sight, or after a reopened stream).
  listen('device', ({ device }) => {
    if (device === store.shown() || device !== chosenUrl()) return;
    draw('open');
    draw('shelf');
  });
  // A reading of the chosen amp's meters moves its needles in place; the area is drawn again only
  // when the meters are not the ones drawn.
  listen('meters', (data) => {
    const url = data.device ?? store.shown();
    const open = root.querySelector('#sg-open');
    if (url !== chosenUrl() || !open?.querySelector('.sg-mscale')) return;
    if (!metersInPlace(open, store.device(url))) draw('open');
  });
  // The volume the line writes under the DAC's name while another hop is chosen follows the device.
  listen('volume', (data) => {
    const url = data.device ?? store.shown();
    if (url !== store.shown() || chosenUrl() === url) return;
    paint('line');
    paint('slim');
  });
  for (const event of ['host', 'fleet']) listen(event, () => draw('line'));
  listen('state', (state) => {
    view.state = state;
    stillness();
    // The wire's ring at the shown device and the card follow the state.
    draw('line');
  });

  function stop() {
    for (const fn of off.splice(0)) fn();
    switchers.length = 0;
    choosers.length = 0;
  }
  signal.addEventListener('abort', stop, { once: true });
  if (signal.aborted) stop();

  return {
    drawAll,
    draw,

    /** Hold the section `name` (`on`) or let it go: what came while it was held is drawn now. */
    hold(name, on) {
      if (on) {
        holding.add(name);
        return;
      }
      holding.delete(name);
      stillness();
      if (stale.has(name)) draw(name);
    },

    held: (name) => holding.has(name),

    /** Hear a switch to another shown device until the page goes: what was asked of the old one is
     * not asked of this one. */
    onSwitch(fn) {
      switchers.push(fn);
    },

    /** Hear the choice of another hop until the page goes: what was open for the old one closes. */
    onChoice(fn) {
      choosers.push(fn);
    },

    /** Tell those who hear it that another hop is chosen. */
    choosing() {
      for (const fn of choosers) fn();
    },

    /** Hear a reading of the store (`state`, `volume`, ...) until the page goes; returns the way to
     * stop hearing it. */
    onReading(event, fn) {
      const stopHearing = store.on(event, fn);
      off.push(stopHearing);
      return stopHearing;
    },

    stop,
  };
}
