/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page kept up to date: what the core's store says, put into the sections of the page. A
// section is `{ parts, draw(page) }`: the parts of the shown device it shows (by the model's names,
// `'*'` for every part) and how it is drawn from the model. A change of the whole model, of the
// shown device, the fleet or the host draws every section; a part of the shown device draws the
// sections that show it; the stream's state goes into `view.state` and draws every section.
//
// A section in use is not drawn again under the person's hand: `hold(name)` keeps it, and what
// came meanwhile is drawn from the newest model at `release(name)`. The sheet's holds (a value typed,
// a list open, a question asked, a stepper held) are counted by `keeper`. What is in the person's hand
// and belongs to the device shown (a drag on the mountains) hears `onSwitch` and lets go.

/** The parts of the shown device each section shows, by the model's names. */
export const PARTS_OF = {
  hud: ['link', 'card', 'volume', 'controls', 'tone', 'presets', 'playing'],
  dash: ['link', 'card', 'volume', 'controls', 'power', 'playing'],
  sheet: ['link', 'card', 'controls', 'power', 'tone', 'presets', 'device_presets', 'facts'],
};

/**
 * Keep the sections of `root` up to date for `core` until `signal` aborts. `view` is the page's
 * state (`view.state` follows the stream). Returns `{ drawAll, draw, hold, release, held, onSwitch,
 * onReading, stop }`.
 */
export function createLive({ core, signal, view, sections }) {
  const store = core.model;
  const holding = new Set();
  const stale = new Set();
  const off = [];
  const switchers = [];
  /** The device the page showed when it last looked: a change of it is a switch. */
  let seen = store.shown();

  /** Another device is shown than the one the page was drawn for: those who hold something of the
   * old one are told. */
  function switched() {
    if (store.shown() === seen) return;
    seen = store.shown();
    for (const fn of switchers) fn();
  }

  /** Draw the section `name` from the model as it is; nothing while it is held. */
  function draw(name) {
    if (holding.has(name)) {
      stale.add(name);
      return;
    }
    stale.delete(name);
    sections[name]?.draw(store.page());
  }

  const drawAll = () => Object.keys(sections).forEach(draw);

  const drawParts = (part) => {
    for (const [name, section] of Object.entries(sections)) {
      if (section.parts === '*' || section.parts?.includes(part)) draw(name);
    }
  };

  const listen = (event, fn) => off.push(store.on(event, fn));
  listen('model', () => {
    switched();
    view.state = store.state();
    drawAll();
  });
  listen('device', ({ device }) => {
    if (device !== store.shown()) return;
    switched();
    drawAll();
  });
  listen('part', ({ device, name }) => {
    if (device === store.shown()) drawParts(name);
  });
  listen('volume', (data) => {
    if ((data.device ?? store.shown()) === store.shown()) drawParts('volume');
  });
  for (const event of ['host', 'fleet']) listen(event, drawAll);
  // the side flipped, or a value of the computer is held or settled
  off.push(core.computer.on(drawAll));
  listen('state', (state) => {
    view.state = state;
    drawAll();
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

    /** Keep the section `name` as it is until it is released. */
    hold(name) {
      holding.add(name);
    },

    /** Let the section `name` go: what came while it was held is drawn now. */
    release(name) {
      holding.delete(name);
      if (stale.has(name)) draw(name);
    },

    held: (name) => holding.has(name),

    /** Hear a switch to another shown device until the page goes: what was asked of the old one is
     * not asked of this one. */
    onSwitch(fn) {
      switchers.push(fn);
    },

    /** Hear a reading of the store (`state`, `volume`, ...) until the page goes. */
    onReading(event, fn) {
      off.push(store.on(event, fn));
    },

    stop,
  };
}

/**
 * A count of the holds on the section `name`, so that a typed value, an open list and a held stepper
 * each keep the sheet as it is and the last one to go lets it be drawn. `take()` and `drop()` are one
 * hold; `dropLater()` gives one back on the next task (so a redraw does not replace what is under the
 * click that follows); `around(fn)` takes one for `fn` and gives it back if `fn` throws; `reset()`
 * lets every hold go (another device is shown) and cancels the give-backs planned before it.
 */
export function keeper(live, name) {
  let holds = 0;
  let epoch = 0;
  const keep = {
    take() {
      holds++;
      live.hold(name);
    },
    drop() {
      holds = Math.max(0, holds - 1);
      if (holds === 0) live.release(name);
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
      live.release(name);
    },
    /** The hold of a thing in the hand (a scale): `grip()` on every move or key, `letGo()` when the hand is
     * away, which gives the hold back once `pending()` (a promise: the writes not yet answered) is settled
     * and `ms` have passed, unless a new grip came meanwhile (a slow answer must not release a hold the next
     * gesture took). */
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
 * The way to give back one hold of `keep` (a `keeper`) without taking the button from under the press
 * that closed what held it: `release()` gives it back on the next task, or once the press now going is
 * over (pointer up or cancel on `doc`). Listens until `signal` aborts.
 */
export function createRelease(doc, keep, signal) {
  let pressed = false;
  doc.addEventListener('pointerdown', () => { pressed = true; }, { capture: true, signal });
  const over = () => { pressed = false; };
  doc.addEventListener('pointerup', over, { capture: true, signal });
  doc.addEventListener('pointercancel', over, { capture: true, signal });
  return function release() {
    if (!pressed) {
      keep.dropLater();
      return;
    }
    let finished = false;
    const done = () => {
      if (finished) return;
      finished = true;
      doc.removeEventListener('pointerup', done, true);
      doc.removeEventListener('pointercancel', done, true);
      keep.dropLater();
    };
    doc.addEventListener('pointerup', done, { capture: true, signal });
    doc.addEventListener('pointercancel', done, { capture: true, signal });
  };
}
