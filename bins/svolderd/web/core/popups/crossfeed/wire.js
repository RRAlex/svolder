/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Crossfeed tab's part of the Sound popup: it reads the crossfeed of the computer's model
// (`host.computer.eq.crossfeed`, the daemon's own view) with what this viewer has asked laid over
// it, draws it (draw.js) into the popup's frame and writes the changes through the core's lane
// (`actions.computer.setCrossfeed`: one request at a time, the fields merged, the newest value
// wins). The view that comes back is the truth: an answer of a write and an update of the stream
// may come in either order, and what is shown is the model's as soon as the writes have settled.
//
// A stepper is a `−` and a `+`: a press is one step and one request (`done: true`); a hold
// repeats after `HOLD_MS` every `REPEAT_MS` with `done: false` (heard, not saved) and ends with
// one more request that is `done: true`, at the value reached. The model and preset menus are
// the core's lists (list.js). A refusal is shown in the tab's own words until the next change;
// the values go back to what the daemon holds.
//
// The part draws only while it is the tab shown (`frame` is the Sound popup's view of it, which
// swallows what the other tab draws), and holds nothing the page does not: it follows the model
// and the core's events while open.

import { openList } from '../../list.js';
import {
  IDS, WORDS, modelOptions, pageHtml, presetOptions, shownView, stepped, switchHtml,
} from './draw.js';

/** How long a press must last before it repeats, and how often it repeats (ms). */
export const HOLD_MS = 450;
export const REPEAT_MS = 90;

/** The id of the slot the page is put in. */
export const SLOT = 'popup-sound-xf-slot';

/** The Crossfeed part: `{ show(), hide(), onKey(event) }` over `core`, drawn into
 * `frame` (the tab's view of the popup's frame: `draw`, `put`, `el`, `name`). */
export function wire({ core, frame, view: window = frame.el.ownerDocument.defaultView }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  const actions = core.actions.computer;
  let open = false;
  let refusal = null;
  let pick = null;
  let stopping = new AbortController();
  let offs = [];
  /** A hold of a stepper: `{ field, dir, timer, repeating }`. */
  let hold = null;
  /** Whether the click that follows the end of a repeat is not a step of its own. */
  let spent = false;

  const computerNow = () => store.page().host?.computer ?? null;
  const cfNow = () => computerNow()?.eq?.crossfeed ?? null;
  const shown = () => shownView(cfNow(), actions.crossfeedAsked());
  const slot = () => `<div id="${SLOT}">${pageHtml({
    cf: shown(), refusal, computer: computerNow() !== null,
  })}</div>`;
  const inner = () => pageHtml({ cf: shown(), refusal, computer: computerNow() !== null });
  /** Whether the header has its switch: it does with a crossfeed in view. */
  let headed = false;

  function draw() {
    const cf = shown();
    headed = Boolean(cf);
    frame.draw({ page: 'crossfeed', body: slot(), head: switchHtml(cf) });
  }

  /** Draw what changed, in its places: the page keeps its buttons (and the focus) when it can. */
  function refresh() {
    if (!open) return;
    const cf = shown();
    if (Boolean(cf) !== headed) {
      draw();
      return;
    }
    frame.put(SLOT, inner());
    frame.put(`popup-${frame.name}-head`, switchHtml(cf));
  }

  /** Ask for `fields`; the page shows them at once, a refusal shows its words. */
  function send(fields, done = true) {
    refusal = null;
    const sent = actions.setCrossfeed(fields, { done });
    sent.then((answer) => {
      if (!open || answer?.queued) return;
      if (answer?.ok) refusal = null;
      refresh();
    });
    return sent;
  }

  function step(field, dir, done) {
    const next = stepped(shown(), field, dir);
    if (next === null) return false;
    send({ [field]: next }, done);
    return true;
  }

  function endHold() {
    if (!hold) return;
    const ended = hold;
    hold = null;
    window.clearTimeout(ended.timer);
    if (!ended.repeating) return;
    // the repeat that ends asks for the value reached once more, as the last of the drag
    spent = true;
    window.setTimeout(() => { spent = false; }, 0);
    const value = shown()[ended.field];
    if (Number.isFinite(value)) send({ [ended.field]: value }, true);
  }

  function repeat() {
    if (!hold) return;
    hold.repeating = true;
    if (!step(hold.field, hold.dir, false)) {
      endHold();
      return;
    }
    hold.timer = window.setTimeout(repeat, REPEAT_MS);
  }

  const off = (button) => button.getAttribute('aria-disabled') === 'true';

  function menu(opener, kind) {
    const cf = shown();
    if (!cf || off(opener) || pick) return;
    const model = kind === 'model';
    pick = openList({
      opener,
      label: model ? WORDS.modelLabel : WORDS.presetLabel,
      signal: stopping.signal,
      options: model ? modelOptions(cf) : presetOptions(cf),
      choose: (value) => {
        send(model ? { model: value } : { preset: value });
        doc.getElementById(model ? IDS.model : IDS.preset)?.focus({ preventScroll: true });
      },
      onClose: () => { pick = null; },
    });
  }

  frame.el.addEventListener('pointerdown', (event) => {
    const button = event.target.closest?.('.fpx-step');
    if (!open || !button || event.button !== 0 || off(button)) return;
    endHold();
    hold = {
      field: button.dataset.field,
      dir: Number(button.dataset.dir),
      repeating: false,
      timer: window.setTimeout(repeat, HOLD_MS),
    };
  });

  frame.el.addEventListener('click', (event) => {
    if (!open) return;
    const button = event.target.closest?.('button');
    if (!button || !frame.el.contains(button)) return;
    if (button.classList.contains('fpx-step')) {
      if (spent) {
        spent = false;
        return;
      }
      if (!off(button)) step(button.dataset.field, Number(button.dataset.dir), true);
    } else if (button.dataset.act === 'on') {
      const cf = shown();
      if (cf && !off(button)) send({ on: !cf.on });
    } else if (button.dataset.act === 'model' || button.dataset.act === 'preset') {
      menu(button, button.dataset.act);
    }
  });

  const stopHold = () => endHold();

  return {
    show() {
      if (!open) {
        open = true;
        stopping = new AbortController();
        offs = [
          store.on('model', refresh), store.on('host', refresh), core.computer.on(() => refresh()),
          actions.onCrossfeedRefusal((why) => {
            refusal = why;
            refresh();
          }),
        ];
        for (const type of ['pointerup', 'pointercancel']) {
          window.addEventListener(type, stopHold, true);
        }
        window.addEventListener('blur', stopHold);
      }
      refusal = null;
      draw();
    },
    hide() {
      if (!open) return;
      open = false;
      stopping.abort();
      for (const stop of offs) stop();
      offs = [];
      for (const type of ['pointerup', 'pointercancel']) {
        window.removeEventListener(type, stopHold, true);
      }
      window.removeEventListener('blur', stopHold);
      endHold();
      pick = null;
      refusal = null;
    },
    onKey: (event) => Boolean(pick?.key(event)),
  };
}
