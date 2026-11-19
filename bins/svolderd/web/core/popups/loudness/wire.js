/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Loudness tab's part of the Sound popup: it reads the loudness of the computer's model
// (`host.computer.eq.loudness`, the daemon's own view) with what this viewer has asked laid over
// it, draws it (draw.js) into the popup's frame and writes the changes through the core's lane
// (`actions.computer.setComputerLoudness`: one request at a time, the fields merged, the newest value
// wins). The view that comes back is the truth: an answer of a write and an update of the stream
// may come in either order, and what is shown is the model's as soon as the writes have settled.
//
// The strength is a slider: while a pointer holds it each value is one request with `done: false`
// (heard, not saved) and its release is one more with `done: true`, at the value reached; a key
// on the slider is exactly one request with `done: true` (the browser's `input` and `change` of a
// key are one request: only a held pointer sends on `input`). The page does not draw the slider
// again while it is dragged, only what follows the host's readings (the picture and the readout)
// and the number beside it. The reference is a `−` and a `+`: a press is one step and one
// request, a hold repeats after `HOLD_MS` every `REPEAT_MS` with `done: false` and ends with one
// more request that is `done: true`; "Use current volume" asks the host to take the volume it
// reads. A refusal is shown in the tab's own words until the next change; the values go back to
// what the daemon holds.
//
// The part draws only while it is the tab shown (`frame` is the Sound popup's view of it, which
// swallows what the other tabs draw), and holds nothing the page does not: it follows the model
// and the core's events while open.

import {
  IDS, pageHtml, readingsHtml, shownView, stepped, switchHtml,
} from './draw.js';

/** How long a press must last before it repeats, and how often it repeats (ms). */
export const HOLD_MS = 450;
export const REPEAT_MS = 90;

/** The id of the slot the page is put in. */
export const SLOT = 'popup-sound-ld-slot';

/** The Loudness part: `{ show(), hide(), onKey(event) }` over `core`, drawn into
 * `frame` (the tab's view of the popup's frame: `draw`, `put`, `el`, `name`). */
export function wire({ core, frame, view: window = frame.el.ownerDocument.defaultView }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  const actions = core.actions.computer;
  let open = false;
  let refusal = null;
  let offs = [];
  /** Whether the slider is being dragged: it is then left as it is. */
  let dragging = false;
  /** Whether a pointer holds the slider (a key does not make a drag). */
  let pressing = false;
  /** A hold of a stepper: `{ field, dir, timer, repeating }`. */
  let hold = null;
  /** Whether the click that follows the end of a repeat is not a step of its own. */
  let spent = false;

  const computerNow = () => store.page().host?.computer ?? null;
  const ldNow = () => computerNow()?.eq?.loudness ?? null;
  const shown = () => shownView(ldNow(), actions.loudnessAsked());
  /** Whether the DAC shown has its own loudness on: it is the tone control's mode. */
  const dacOn = () => store.part('tone', store.shown())?.mode === 'loudness';
  const inner = () => pageHtml({
    ld: shown(), eq: computerNow()?.eq ?? null, refusal, computer: computerNow() !== null,
    dac: dacOn(),
  });
  const slot = () => `<div id="${SLOT}">${inner()}</div>`;
  /** Whether the header has its switch: it does with a loudness in view. */
  let headed = false;

  function draw() {
    const ld = shown();
    headed = Boolean(ld);
    frame.draw({ page: 'loudness', body: slot(), head: switchHtml(ld) });
  }

  /** Draw what changed, in its places: the page keeps its controls (and the focus) when it can. */
  function refresh() {
    if (!open) return;
    const ld = shown();
    if (Boolean(ld) !== headed) {
      draw();
      return;
    }
    if (dragging && ld) {
      // the slider stays in the hand: the number beside it and the readings follow
      frame.put(IDS.readout, readingsHtml(ld, computerNow()?.eq ?? null));
      const value = doc.getElementById(IDS.strengthValue);
      if (value) value.textContent = `${Math.round(ld.strength_pct)} %`;
    } else {
      frame.put(SLOT, inner());
    }
    frame.put(`popup-${frame.name}-head`, switchHtml(ld));
  }

  /** Ask for `fields`; the page shows them at once, a refusal shows its words. */
  function send(fields, done = true) {
    refusal = null;
    const sent = actions.setComputerLoudness(fields, { done });
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

  frame.el.addEventListener('pointerdown', (event) => {
    if (open && event.target?.id === IDS.strength) pressing = true;
  });

  frame.el.addEventListener('input', (event) => {
    const slider = event.target;
    if (!open || slider.id !== IDS.strength) return;
    if (pressing) {
      dragging = true;
      send({ strength_pct: Number(slider.value) }, false);
    }
    const value = doc.getElementById(IDS.strengthValue);
    if (value) value.textContent = `${Math.round(Number(slider.value))} %`;
  });

  frame.el.addEventListener('change', (event) => {
    const slider = event.target;
    if (!open || slider.id !== IDS.strength) return;
    dragging = false;
    send({ strength_pct: Number(slider.value) }, true);
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
      const ld = shown();
      if (ld && !off(button)) send({ on: !ld.on });
    } else if (button.dataset.act === 'now') {
      if (!off(button)) send({ reference_now: true });
    }
  });

  const stopHold = () => endHold();
  // a slider let go anywhere ends its drag, whether or not a `change` follows (a cancelled one)
  const stopDrag = () => {
    pressing = false;
    if (!dragging) return;
    window.setTimeout(() => {
      dragging = false;
      refresh();
    }, 0);
  };

  return {
    show() {
      if (!open) {
        open = true;
        offs = [
          store.on('model', refresh), store.on('host', refresh), core.computer.on(() => refresh()),
          store.on('part', (event) => {
            if (event?.name === 'tone') refresh();
          }),
          actions.onLoudnessRefusal((why) => {
            refusal = why;
            refresh();
          }),
        ];
        for (const type of ['pointerup', 'pointercancel']) {
          window.addEventListener(type, stopHold, true);
          window.addEventListener(type, stopDrag, true);
        }
        window.addEventListener('blur', stopHold);
      }
      refusal = null;
      dragging = false;
      pressing = false;
      draw();
    },
    hide() {
      if (!open) return;
      open = false;
      for (const stop of offs) stop();
      offs = [];
      for (const type of ['pointerup', 'pointercancel']) {
        window.removeEventListener(type, stopHold, true);
        window.removeEventListener(type, stopDrag, true);
      }
      window.removeEventListener('blur', stopHold);
      endHold();
      dragging = false;
      pressing = false;
      refusal = null;
    },
    onKey: () => false,
  };
}
