/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the volume: the knob dragged or stepped with the keys, a press on the track, and the
// `−`, `+` and `Mute` buttons. Every write goes through `core.actions` (the cap, the pacing, the
// refusals and the glide stay where they are) and is logged by `act`.
//
// The knob shows the step asked for at once (`view.asked`) and the device's own once it reports it
// or `ASK_MS` pass. A drag takes a hold of the open area (`keep`, shared with the tone's), so what
// the device reports meanwhile is drawn when the hand lets go, from the newest reading, never from the one at the touch.

import { ASK_MS, rawAt, shownRaw, showVolume } from './draw/volume.js';
import { dragAlong, sliderKeys } from './gestures.js';

const STEPS = '[data-act="volume-down"], [data-act="volume-up"]';

/** Wire the volume of the open area in `root` until `signal` aborts. */
export function wireVolume({
  root, core, live, act, view, keep, signal,
}) {
  const box = root.querySelector('#sg-open');
  const store = core.model;
  let timer = null;
  signal.addEventListener('abort', () => clearTimeout(timer), { once: true });

  const volume = () => store.part('volume');
  const section = () => box.querySelector('#sg-volume');
  const inert = () => Boolean(store.device()?.inert);
  const climbing = (v) => v.target !== null && v.target !== undefined && v.raw !== null
    && v.target > v.raw;

  /** The knob and its words from the model and what is asked, in place. */
  function show() {
    const v = volume();
    const at = section();
    if (v && at) showVolume(at, v, view);
  }

  /** The step asked for stops standing in for the reading after `ASK_MS`: the area is drawn again
   * then, from the reading. */
  function expire() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (view.asked && Date.now() - view.asked.at >= ASK_MS) {
        view.asked = null;
        live.draw('open');
      }
    }, ASK_MS + 50);
  }

  /** The step a change starts from: a raise from the step asked or the climb's target, a lowering
   * from the level (a lower step ends a climb where the device is). */
  function base(v, delta) {
    view.now = Date.now();
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
      view.asked = { raw: landed, at: Date.now() };
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
   * not write is told so by the core and the knob does not move. */
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
    const at = Date.now();
    const same = (view.asked?.raw === step && shownRaw(v, { asked: view.asked, now: at }) === step)
      || (!view.asked && step === v.raw && !climbing(v));
    if (same) return;
    view.now = at;
    view.asked = { raw: step, at };
    expire();
    show();
    send(step, device).catch(() => {});
  }

  const nudge = (delta) => {
    const v = volume();
    if (v && v.raw !== null && v.raw !== undefined) ask(base(v, delta) + delta);
  };

  // A drag along the track, from the knob or from a press elsewhere on it.
  let gripped = false;
  // The device the drag began on: every write of the drag goes to it.
  let device = null;
  dragAlong(box, {
    signal,
    pick: (event) => event.target.closest?.('#sg-vtrack'),
    onStart(at) {
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
      keep.take();
      ask(rawAt(v, at), device);
    },
    onMove(at) {
      const v = volume();
      if (gripped && v) ask(rawAt(v, at), device);
    },
    onEnd() {
      if (!gripped) return;
      gripped = false;
      keep.drop();
      box.querySelector('#sg-vtrack')?.focus({ preventScroll: true });
    },
  });

  // Another device is shown: what was asked of the old one is not asked of this one, and a drag
  // still going ends, so it never writes to the one now shown (the shared hold is reset by the
  // tone's own switch handler, so it is not given back here).
  live.onSwitch(() => {
    view.asked = null;
    clearTimeout(timer);
    gripped = false;
  });

  sliderKeys(box, {
    signal,
    match: '#sg-vtrack',
    step: (delta) => nudge(delta),
    home() {
      const v = volume();
      if (v) ask(v.lo);
    },
  });

  // A held `−` or `+` is the core's hold: one step at the press, then, after a pause, steps at a
  // steady pace. The open area is held (`keep`) while a press lasts, so a reading does not replace
  // the button under the finger; the hold ends with the release, the pointer's cancel or loss, the
  // window losing focus, the page hidden or its stream lost, another device shown, and the button
  // leaving the page.
  let hold = null;
  // Another device shown ends it without giving the shared hold back: the tone's own switch
  // handler resets that.
  let give = true;
  live.onSwitch(() => {
    give = false;
    hold?.end();
    give = true;
  });
  const quiet = () => view.state === 'lost' || view.state === 'waiting';
  box.addEventListener('pointerdown', (event) => {
    const button = event.target.closest?.(STEPS);
    if (!button || !button.closest('#sg-volume') || !event.isPrimary
      || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    // A press that steps once: a viewer who may not write is told so, once, not on every repeat.
    hold = core.hold.repeat(event, {
      root: box,
      el: button,
      step: (el) => nudge(el.dataset.act === 'volume-up' ? 1 : -1),
      live: () => !inert() && !quiet(),
      leave: true,
      cap: Infinity,
      onStart: () => keep.take(),
      onEnd: () => {
        hold = null;
        if (give) keep.drop();
      },
      signal,
    });
  }, { signal });

  // A long press on a phone must not open the context menu over a held button.
  box.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPS)) event.preventDefault();
  }, { signal });

  box.addEventListener('click', (event) => {
    const word = event.target.closest?.('[data-act]');
    if (!word || !word.closest('#sg-volume')) return;
    const v = volume();
    // A press of the pointer stepped already (above); a click with no pointer is a key: one step.
    if (word.matches(STEPS)) {
      if (event.detail === 0) nudge(word.dataset.act === 'volume-up' ? 1 : -1);
    } else if (word.dataset.act === 'mute' && v) {
      const value = !v.muted;
      act.act('mute', value, (actions) => actions.setControl(word.dataset.key, { value }))
        .catch(() => {});
    }
  }, { signal });
}
