/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the volume: the needle dragged or stepped with the keys, a press on the ruler, and
// the written words (`− quieter`, `mute`, `louder +`). Every write goes through `core.actions`
// (the cap, the pacing, the refusals and the glide stay where they are) and is logged by `act`.
//
// The needle shows the step asked for at once (`view.asked`) and the device's own once it
// reports it or `ASK_MS` pass. A drag holds the volume section, so what the device reports meanwhile
// is drawn when the hand lets go, from the newest reading, never from the one at the touch.

import { askExpiry, createVolumeAsker, noteReading, shownRaw } from '../../core/volume-ask.js';
import { showVolume } from './draw/volume.js';
import { dragAlong, sliderKeys } from '../../core/gestures.js';
import { fractionOf, rawAt } from './ruler.js';

const KEY_MATCH = '#p-needle';

/** Wire the volume of the sheet in `root` until `signal` aborts. */
export function wireVolume({ root, core, live, act, view, signal }) {
  const box = root.querySelector('#p-volume');
  if (!box) return;
  const store = core.model;

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
  const expiry = askExpiry({ view, now: Date.now, redraw: () => live.draw('volume'), signal });

  /** Ask, nudge and base: the core's asker (core/volume-ask.js). A viewer who may not write is
   * told so by the core and the needle does not move. */
  const asker = createVolumeAsker({
    view,
    signal,
    volume: () => (flipped() ? null : volume()),
    inert,
    refuse: (step, device) => core.actions.setVolume(step, { device }),
    write: (step, device) => act.act('volume', step,
      (actions) => actions.setVolume(step, { device })),
    show,
    expiry,
    now: Date.now,
    shown: () => store.shown(),
  });
  const { ask, nudge } = asker;

  live.onReading('volume', () => {
    if (noteReading(view, volume()?.raw)) show();
  });

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
      live.freeze('volume', true);
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
      live.freeze('volume', false);
      box.querySelector('#p-needle')?.focus({ preventScroll: true });
    },
  });

  // Another device is shown: what was asked of the old one is not asked of this one, and a drag
  // still going ends, so it never writes to the one now shown.
  live.onSwitch(() => {
    view.asked = null;
    expiry.clear();
    if (!gripped) return;
    gripped = false;
    live.freeze('volume', false);
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
