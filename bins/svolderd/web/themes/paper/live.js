/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sheet kept up to date: what the core's store says, put into the sheet's sections. Each
// section is drawn by its function in draw/ from the model as it is, into the element of its name
// (`#p-<name>`). A change of the whole model draws every section; a part that changed draws the
// sections that show it (a reading of the meters moves their needles in place); the host and the
// fleet draw Now playing, the title and the notes, and so does a reading of the volume (the notes
// tell the last change); the stream's state fades or stills the sheet and rewrites the title's
// line.
//
// A section in use is not redrawn under the person's hand: a gesture or an open list `hold`s it,
// and the change that came meanwhile is drawn from the newest model once the hold ends, never
// from the copy at the hold's start. Every draw ends by putting the marks in their places
// (place.js) and telling the frame to follow the button to its slot.

import { QUICK, SECTIONS } from './draw/sheet.js';
import { clearMoved, placeNotes } from './marks.js';
import { place } from './place.js';

/** The parts of a device each section shows, by the model's names; `'*'` is every part. */
export const PARTS_OF = {
  title: ['link', 'card', 'facts', 'controls', 'playing'],
  card: ['card'],
  volume: ['volume'],
  tone: ['tone', 'presets'],
  controls: ['controls'],
  power: ['power'],
  meters: ['meters'],
  presets: ['presets'],
  'device-presets': ['device_presets'],
  playing: ['playing'],
  device: ['facts'],
  notes: '*',
  foot: [],
};

/** The stream's states in which the sheet cannot be used: it is faded (lost) or still (waiting)
 * and `inert`. */
const STILL = { lost: 'faded', waiting: 'waiting' };

/** A selector for the element of `box` that has the focus, to find its twin after the box is drawn
 * again: its id, or its `data-act`; nothing when the focus is elsewhere. */
function focusOf(box) {
  const active = globalThis.document?.activeElement;
  if (!active || !box.contains?.(active)) return null;
  if (active.id) return `#${active.id}`;
  return active.dataset?.act ? `[data-act="${active.dataset.act}"]` : null;
}

/** A counted hold of the section `name`, for gestures and open lists that may overlap: it is held
 * from the first `take()` and let go at the `drop()` that matches the last. `around(fn)` runs `fn`
 * with a hold taken and gives it back if `fn` throws. `reset()` lets go of every hold at once (the
 * shown device was replaced). `hand({ pending, ms })` is the hold of a thing in the hand (a scale):
 * `grip()` on every move or key, `letGo()` when the hand is away, which gives the hold back once
 * `pending()` (a promise: the writes not yet answered) is settled and `ms` have passed, unless a
 * new grip came meanwhile (a slow answer must not release a hold the next gesture took). */
export function keeper(live, name) {
  let holds = 0;
  const keep = {
    take() {
      holds++;
      live.hold(name, true);
    },
    drop() {
      holds = Math.max(0, holds - 1);
      if (holds === 0) live.hold(name, false);
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
      holds = 0;
      live.hold(name, false);
    },
    hand({ pending, ms }) {
      let held = false;
      let generation = 0;
      let timer = null;
      return {
        grip() {
          generation++;
          clearTimeout(timer);
          if (held) return;
          held = true;
          keep.take();
        },
        letGo() {
          clearTimeout(timer);
          const mine = generation;
          timer = setTimeout(async () => {
            await pending();
            if (generation !== mine || !held) return;
            held = false;
            keep.drop();
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
 * Keep the sheet in `root` up to date for `core`, until `signal` aborts. `view` is the facts about
 * the screen the sections draw by (`{ phone, state, coarse, now }`), which this keeps current
 * where it knows (`state`). Returns `{ drawAll, draw, hold, held, onReading, stop }`.
 */
export function createLive({ core, root, signal, view }) {
  const store = core.model;
  const holding = new Set();
  const stale = new Set();
  const off = [];
  const switchers = [];
  /** The device the sheet showed when it last looked: a change of it is a switch. */
  let seen = store.shown();

  /** Draw the section `name` from the model as it is; nothing while it is held (it is drawn when
   * the hold ends). Says whether it was drawn. */
  function paint(name) {
    if (holding.has(name)) {
      stale.add(name);
      return false;
    }
    const box = root.querySelector(`#p-${name}`);
    if (!box) return false;
    stale.delete(name);
    // A reading that changes what moves (the needle, its words) is put in place, so what has the
    // focus keeps it; anything else is drawn again, the focus put back where it was.
    if (QUICK[name]?.(box, store.page(), store.device(), view)) return true;
    // The notes tell the last change: the keeper looks at the model before they are drawn.
    if (name === 'notes') view.changes?.seen(store.device(), view.now, view.log?.() ?? []);
    // The notes a phone moved after their sections go with the band's own.
    if (name === 'notes') clearMoved(root);
    const focused = focusOf(box);
    box.innerHTML = SECTIONS[name]?.(store.page(), store.device(), view) ?? '';
    if (focused) box.querySelector(focused)?.focus();
    // The slot the frame's button sits over is part of the title, which was just drawn again.
    if (name === 'title') core.frame.anchor(box.querySelector('.paper-slot'));
    return true;
  }

  /** The marks are in place and the button follows its slot. */
  function settle() {
    place(root);
    placeNotes(root, view);
    core.frame.realign();
  }

  /** The sheet's classes for the stream's state: faded while it is lost, still while the page waits
   * for the device again, both out of reach of the person. */
  function stillness() {
    const sheet = root.querySelector('#paper');
    if (!sheet) return;
    for (const [state, name] of Object.entries(STILL)) {
      sheet.classList.toggle(name, view.state === state);
    }
    // A gesture in hand is not cut short: the sheet turns inert when the hold ends.
    sheet.inert = view.state in STILL && holding.size === 0;
  }

  function draw(name) {
    if (paint(name)) settle();
  }

  function drawAll() {
    view.state = store.state();
    stillness();
    let any = false;
    for (const name of Object.keys(PARTS_OF)) any = paint(name) || any;
    if (any) settle();
  }

  const listen = (event, fn) => {
    off.push(store.on(event, fn));
  };

  /** Another device is shown than the one the sections were drawn for: whatever was open on the
   * old one (a list, a form, a question, a scale in hand) is closed first, so nothing held keeps
   * the old device's words on the sheet and no click is read against the new one's. */
  function switched() {
    if (store.shown() === seen) return;
    seen = store.shown();
    view.changes?.baseline(store.device());
    for (const fn of switchers) fn();
  }

  listen('model', () => {
    switched();
    view.changes?.baseline(store.device());
    drawAll();
  });
  listen('device', ({ device }) => {
    if (device !== store.shown()) return;
    switched();
    drawAll();
  });
  listen('part', ({ device, name }) => {
    if (device !== store.shown()) return;
    for (const [section, parts] of Object.entries(PARTS_OF)) {
      if (parts === '*' || parts.includes(name)) draw(section);
    }
  });
  for (const event of ['host', 'fleet']) {
    listen(event, () => {
      draw('playing');
      draw('title');
      draw('notes');
    });
  }
  // A reading of the meters moves their needles in place.
  listen('meters', (data) => {
    if ((data?.device ?? store.shown()) === store.shown()) draw('meters');
  });
  // A reading of the volume (a glide's steps, a write's echo) moves the needle in place.
  listen('volume', (data) => {
    if ((data?.device ?? store.shown()) !== store.shown()) return;
    draw('volume');
    draw('notes');
  });
  listen('state', (state) => {
    view.state = state;
    stillness();
    draw('title');
  });

  function stop() {
    for (const fn of off.splice(0)) fn();
    switchers.length = 0;
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

    /** Hear the shown device being replaced by another, until the sheet goes. */
    onSwitch(fn) {
      switchers.push(fn);
    },

    /** Hear a reading of the store (`volume`, `level`, `meters`, `strip`) until the sheet goes;
     * returns the way to stop hearing it. */
    onReading(event, fn) {
      const stopHearing = store.on(event, fn);
      off.push(stopHearing);
      return stopHearing;
    },

    stop,
  };
}
