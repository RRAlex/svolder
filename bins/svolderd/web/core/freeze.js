/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Freezing a section of the page under the person's hand. A section in use (a value being typed, a
// list open, a question asked, a stepper held, a scale in the hand) is not drawn again while it is
// frozen: what came meanwhile is drawn from the newest model when the last hold on it goes, never
// from the copy at the hold's start. Three parts, which every theme that redraws sections shares:
//   keeper(live, name)         the holds on one section, counted: a gesture's (`take`, `drop`,
//                              `hand`) and an open list's or form's (`around`, `release`) apart,
//                              so a theme can tell a gesture in hand from a list that is only open
//   createRelease(doc, keep)   the way to give a hold back without taking the button from under
//                              the press that closed what held it
//   createLive({ core, signal, view, sections, device })
//                              the sections drawn from the store's events, and the ones that are
//                              held kept as they are until they are let go
// The sections of `createLive` are `{ parts, draw(page, device) }`: the parts of the device a
// section shows (by the model's names, `'*'` for every part) and how it is drawn from the model.
// `device()` names the device the sections show: the caller says which, this module never chooses.
// A theme reaches this as `core.freeze`.

/**
 * The holds on the section `name` of `live` (an object with `freeze(name, on, gesture)`): a typed
 * value, an open list and a held stepper each keep the section as it is, and the last one to go
 * lets it be drawn. `take()` and `drop()` are a gesture's hold; `around(fn)` takes a list's or
 * form's for `fn` (and gives it back if `fn` throws) and `release()` gives it back; `dropLater()`
 * gives one back on the next task (a list's first), so a redraw does not replace what is under the
 * click that follows. `reset()` lets every hold go (another device is shown) and cancels the
 * give-backs planned before it. `hand({ pending, ms })` is the hold of a thing in the hand (a
 * scale): `grip()` on every move or key, `letGo()` when the hand is away, which gives the hold
 * back once `pending()` (a promise: the writes not yet answered) is settled and `ms` have passed,
 * unless a new grip came meanwhile (a slow answer must not release a hold the next gesture took).
 * A hand that a reset let go of holds again at its next grip.
 */
export function keeper(live, name) {
  /** The gestures' holds and the open lists' and forms', counted apart. */
  let gestures = 0;
  let asks = 0;
  /** Counts the resets: a give-back planned before one is for a hold the reset has let go. */
  let epoch = 0;
  const note = () => live.freeze(name, gestures + asks > 0, gestures > 0);
  const keep = {
    take() {
      gestures++;
      note();
    },
    drop() {
      gestures = Math.max(0, gestures - 1);
      note();
    },
    release() {
      asks = Math.max(0, asks - 1);
      note();
    },
    dropLater() {
      const planned = epoch;
      setTimeout(() => {
        if (epoch !== planned) return;
        if (asks > 0) keep.release();
        else keep.drop();
      }, 0);
    },
    around(fn) {
      asks++;
      note();
      try {
        return fn();
      } catch (error) {
        keep.release();
        throw error;
      }
    },
    reset() {
      epoch++;
      gestures = 0;
      asks = 0;
      note();
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
          if (held && heldIn === epoch) return;
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
 * The way to give back one hold of `keep` (a `keeper`) without taking the button from under the
 * press that closed what held it: `release()` gives it back on the next task, or once the press now
 * going is over (pointer up or cancel on `doc`). Listens until `signal` aborts.
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

/**
 * Keep the sections of a page or a popup up to date for `core` until `signal` aborts. `device()`
 * is the address of the device the sections show, asked at every event: a page passes the device
 * it shows, a popup the one it was opened for. `view` is the caller's state (`view.state` follows
 * the stream). A change of the whole model, of that device, the fleet, the host, the computer's
 * side or the stream's state draws every section; a part of that device draws the sections that
 * show it; another device's events draw nothing. Returns `{ drawAll, draw, freeze, thaw, frozen,
 * onSwitch, onReading, stop }`; `freeze(name)` keeps a section as it is until `thaw(name)` (or
 * `freeze(name, false)`) draws what came meanwhile.
 */
export function createLive({ core, signal, view, sections, device }) {
  if (typeof device !== 'function') {
    throw new TypeError('createLive: device() names the device to follow');
  }
  const store = core.model;
  const holding = new Set();
  const stale = new Set();
  const off = [];
  const switchers = [];
  /** The device the caller named when it was last asked: a change of it is a switch. */
  let seen = device();

  /** The caller names another device than the one the sections were drawn for: those who hold
   * something of the old one are told. */
  function switched() {
    if (device() === seen) return;
    seen = device();
    for (const fn of switchers) fn();
  }

  /** The device a reading is about. One that names none is the shown device's: that is how the
   * store folds it, and the only use of the shown device here. */
  const about = (data) => data?.device ?? store.shown();

  /** Draw the section `name` from the model as it is; nothing while it is held. */
  function draw(name) {
    if (holding.has(name)) {
      stale.add(name);
      return;
    }
    stale.delete(name);
    sections[name]?.draw(store.page(), device());
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
  listen('device', (data) => {
    if (data.device !== device()) return;
    switched();
    drawAll();
  });
  listen('part', (data) => {
    if (data.device === device()) drawParts(data.name);
  });
  listen('volume', (data) => {
    if (about(data) === device()) drawParts('volume');
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
    holding.clear();
    stale.clear();
  }
  signal.addEventListener('abort', stop, { once: true });
  if (signal.aborted) stop();

  /** Keep the section `name` as it is (`on`, the default) until it is thawed: what came while it
   * was frozen is drawn then. A `keeper`'s third argument (a gesture is in hand) is for a theme's
   * own use. */
  function freeze(name, on = true) {
    if (on) {
      holding.add(name);
      return;
    }
    holding.delete(name);
    if (stale.has(name)) draw(name);
  }

  return {
    drawAll,
    draw,
    freeze,

    /** Let the section `name` go: what came while it was frozen is drawn now. */
    thaw: (name) => freeze(name, false),

    frozen: (name) => holding.has(name),

    /** Hear a switch to another device until the caller goes: what was asked of the old one is not
     * asked of this one. */
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
