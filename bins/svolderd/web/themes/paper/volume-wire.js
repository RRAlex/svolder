/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the volume: the needle dragged or stepped with the keys, a press on the ruler, and
// the written words (`− quieter`, `mute`, `louder +`). Every write goes through `core.actions`
// (the cap, the pacing, the refusals and the glide stay where they are) and is logged by `act`.
//
// The needle shows the step asked for at once (`view.asked`) and the device's own once it
// reports it or `ASK_MS` pass. A drag holds the volume section, so what the device reports meanwhile
// is drawn when the hand lets go, from the newest reading, never from the one at the touch.

import { ASK_MS, shownRaw, showVolume } from './draw/volume.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { fractionOf, rawAt } from './ruler.js';

const KEY_MATCH = '#p-needle';

/** Wire the volume of the sheet in `root` until `signal` aborts. */
export function wireVolume({ root, core, live, act, view, signal }) {
  const box = root.querySelector('#p-volume');
  if (!box) return;
  const store = core.model;
  let timer = null;
  signal.addEventListener('abort', () => clearTimeout(timer), { once: true });

  const volume = () => store.part('volume');
  const inert = () => Boolean(store.device()?.inert);
  /** The computer's side is drawn: the ruler and the words are its own (computer-wire.js). */
  const flipped = () => core.computer.side() === 'computer';
  const climbing = (v) => v.target !== null && v.target !== undefined && v.raw !== null
    && v.target > v.raw;

  /** The needle and its words from the model and what is asked, in place. */
  function show() {
    const v = volume();
    if (v && box.querySelector('#p-needle')) showVolume(box, v, view);
  }

  /** The step asked for stops standing in for the reading after `ASK_MS`: the section is drawn
   * again then, from the reading. */
  function expire() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (view.asked && view.now - view.asked.at >= ASK_MS) {
        view.asked = null;
        live.draw('volume');
      }
    }, ASK_MS + 50);
  }

  /** The step a change starts from: a raise from the step asked or the climb's target, a lowering
   * from the level (a lower step ends a climb where the device is). */
  function base(v, delta) {
    const asked = view.asked && shownRaw(v, view) === view.asked.raw ? view.asked.raw : null;
    if (delta < 0) return Math.min(asked ?? v.raw, v.raw);
    return asked ?? (climbing(v) ? v.target : v.raw);
  }

  async function send(step, device) {
    const mine = view.asked;
    const answer = await act.act('volume', step, (actions) => actions.setVolume(step, { device }));
    if (signal.aborted || answer.queued || answer.dropped) return;
    // Another ask has taken the place of this one meanwhile: its answer is not for that.
    if (view.asked !== mine || !mine) return;
    if (!answer.ok) {
      view.asked = null;
      show();
      return;
    }
    // The daemon may land the write on another step (the quietest, under a cap below every step).
    const landed = answer.landed?.raw;
    if (typeof landed === 'number' && landed !== mine.raw) {
      view.asked = { raw: landed, at: view.now };
      show();
    }
    // The device has the step: a reading that comes after this and differs from it is another
    // hand's (`answered`, set by that reading, below).
    view.asked.acked = true;
  }

  live.onReading('volume', () => {
    const asked = view.asked;
    if (asked?.acked && !asked.answered) {
      asked.answered = true;
      show();
    }
  });

  /** Ask for step `raw`: held to the range and the cap, shown at once, and sent. A viewer who may
   * not write is told so by the core and the needle does not move. */
  function ask(raw, device = store.shown()) {
    const v = volume();
    if (!v || v.raw === null || v.raw === undefined) return;
    let step = Math.min(v.hi, Math.max(v.lo, Math.round(raw)));
    if (v.cap !== null && v.cap !== undefined && step > v.cap) step = v.cap;
    if (inert()) {
      core.actions.setVolume(step, { device });
      return;
    }
    // The step already asked for stands only while it is the one shown: once the device has it and
    // reads another step, that is another hand's and the same ask is a new one.
    const same = (view.asked?.raw === step && shownRaw(v, { asked: view.asked, now: Date.now() }) === step)
      || (!view.asked && step === v.raw && !climbing(v));
    if (same) return;
    view.asked = { raw: step, at: view.now };
    expire();
    show();
    send(step, device).catch(() => {});
  }

  const nudge = (delta) => {
    const v = flipped() ? null : volume();
    if (v && v.raw !== null && v.raw !== undefined) ask(base(v, delta) + delta);
  };

  // A drag along the ruler, from the needle or from a press elsewhere on it.
  let offset = 0;
  let gripped = false;
  // The device the drag began on: every write of the drag goes to it.
  let device = null;
  dragAlong(box, {
    signal,
    pick: (event) => (flipped() ? null : event.target.closest?.('#p-ruler')),
    onStart(at, event) {
      const v = volume();
      gripped = false;
      if (!v || v.raw === null || v.raw === undefined) return;
      if (inert()) {
        ask(rawAt(v, at));
        return;
      }
      gripped = true;
      device = store.shown();
      if (climbing(v)) core.actions.stopClimb();
      live.hold('volume', true);
      const onNeedle = Boolean(event.target.closest?.('#p-needle'));
      // A press on the needle keeps its distance from the pointer, so it does not jump.
      offset = onNeedle ? fractionOf(v, shownRaw(v, view)) - at : 0;
      ask(rawAt(v, at + offset), device);
    },
    onMove(at) {
      const v = volume();
      if (gripped && v) ask(rawAt(v, at + offset), device);
    },
    onEnd() {
      if (!gripped) return;
      gripped = false;
      live.hold('volume', false);
      box.querySelector('#p-needle')?.focus({ preventScroll: true });
    },
  });

  // Another device is shown: what was asked of the old one is not asked of this one, and a drag
  // still going ends, so it never writes to the one now shown.
  live.onSwitch(() => {
    view.asked = null;
    clearTimeout(timer);
    if (!gripped) return;
    gripped = false;
    live.hold('volume', false);
  });

  sliderKeys(box, {
    signal,
    match: KEY_MATCH,
    step: (delta) => nudge(delta),
    home() {
      const v = flipped() ? null : volume();
      if (v) ask(v.lo);
    },
  });

  box.addEventListener('click', (event) => {
    const word = event.target.closest?.('[data-act]');
    if (!word || flipped()) return;
    const v = volume();
    if (word.dataset.act === 'volume-down') nudge(-1);
    else if (word.dataset.act === 'volume-up') nudge(1);
    else if (word.dataset.act === 'mute' && v) {
      const value = !v.muted;
      act.act('mute', value, (actions) => actions.setControl(word.dataset.key, { value }))
        .catch(() => {});
    }
  }, { signal });
}
