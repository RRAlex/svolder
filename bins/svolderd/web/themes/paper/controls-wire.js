/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the controls: a value of a choice, a switch, a list's word that opens a paper list,
// and a scale dragged, stepped with the keys or sent to its centre by a tap on its value. Every
// write goes through `core.actions.setControl` and is logged by `act`; a scale writes the step
// under the hand at once and newest-wins (one write in flight, the latest waiting). A viewer who
// may not write taps the same words and the core says why nothing happens.
//
// The section is held while a list is open and while a scale is in hand (a drag, a burst of keys,
// the write still to be answered), so a change from elsewhere does not move what is under the
// hand: it is drawn from the newest model once the hand is away.

import { dragAlong, sliderKeys } from './gestures.js';
import { closeList, closeOf, openList } from './lists.js';
import { keeper } from './live.js';
import { place } from './place.js';
import { rawAt } from './ruler.js';

/** How long after the last key or move a scale is let go, so the echo of the last write lands
 * before the section is drawn again (ms). */
const LET_GO_MS = 250;

/** Wire the controls of the sheet in `root` until `signal` aborts. */
export function wireControls({
  root, core, live, act, signal,
}) {
  const box = root.querySelector('#p-controls');
  if (!box) return;
  const store = core.model;
  const keep = keeper(live, 'controls');
  const controlOf = (key) => (store.device()?.controls ?? []).find((c) => c.key === key);
  const inert = () => Boolean(store.device()?.inert);

  /** Write `body` to control `key` of `device` (the one shown now unless said) for `value`, logged. */
  const write = (key, value, body, device = store.shown()) => act.act(`control:${key}`, value,
    (a) => a.setControl(key, body, { device })).catch(() => {});

  // ---- a scale in hand ----
  let pending = Promise.resolve();
  const hand = keep.hand({ pending: () => pending, ms: LET_GO_MS });
  signal.addEventListener('abort', () => hand.stop(), { once: true });

  /** One write in flight per scale, the latest waiting, each to the device the scale was shown
   * for; `pending` is the write now in flight. */
  const writers = new Map();
  function send(key, raw) {
    let w = writers.get(key);
    if (!w) {
      w = { busy: null, next: null };
      writers.set(key, w);
    }
    if (w.busy) {
      w.next = raw;
      return;
    }
    const device = store.shown();
    w.busy = (async () => {
      let now = raw;
      while (now !== null && !signal.aborted) {
        w.next = null;
        await write(key, now, { raw: now }, device);
        now = w.next;
      }
      w.busy = null;
    })();
    pending = w.busy;
  }

  /** Show step `raw` on the scale of `c` at once: its mark, its words and its value. */
  function show(scale, c, raw) {
    const mark = scale.querySelector('.p-mark')
      ?? scale.appendChild(Object.assign(scale.ownerDocument.createElement('span'),
        { className: 'p-mark' }));
    mark.dataset.at = ((raw - c.lo) / (c.hi - c.lo)).toFixed(5);
    place(scale);
    // The device's own words for the step where it gave them, the number where it did not.
    const text = c.texts?.[raw - c.lo] ?? String(raw);
    scale.setAttribute('aria-valuenow', String(raw));
    scale.setAttribute('aria-valuetext', text);
    const word = scale.closest('.p-row')?.querySelector('.p-line .p-w, .p-line .p-val');
    if (word) word.textContent = text;
  }

  /** Ask for step `raw` of control `key`: held to the range, shown, sent; a viewer who may not
   * write is told by the core and nothing moves. */
  function ask(scale, raw) {
    const c = controlOf(scale.dataset.key);
    if (!c || typeof c.raw !== 'number') return;
    const step = Math.min(c.hi, Math.max(c.lo, Math.round(raw)));
    if (inert()) {
      core.actions.setControl(c.key, { raw: step }, { device: store.shown() });
      return;
    }
    if (step === shownRaw(scale)) return;
    hand.grip();
    show(scale, c, step);
    send(c.key, step);
  }

  /** The step the scale shows now. */
  const shownRaw = (scale) => Number(scale.getAttribute('aria-valuenow'));

  let gripped = null;
  dragAlong(box, {
    signal,
    pick: (event) => event.target.closest?.('.p-scale'),
    onStart(at, event) {
      const scale = event.target.closest('.p-scale');
      const c = controlOf(scale.dataset.key);
      gripped = null;
      if (!c || typeof c.raw !== 'number') return;
      if (inert()) {
        ask(scale, rawAt(c, at));
        return;
      }
      gripped = { scale, c };
      scale.focus({ preventScroll: true });
      ask(scale, rawAt(c, at));
    },
    onMove(at) {
      if (gripped) ask(gripped.scale, rawAt(gripped.c, at));
    },
    onEnd() {
      if (!gripped) return;
      gripped = null;
      hand.letGo();
    },
  });

  sliderKeys(box, {
    signal,
    match: '.p-scale',
    step(delta, scale) {
      if (Number.isNaN(shownRaw(scale))) return;
      ask(scale, shownRaw(scale) + delta);
      hand.letGo();
    },
    home(scale) {
      const c = controlOf(scale.dataset.key);
      if (!c) return;
      ask(scale, typeof c.centre === 'number' ? c.centre : c.lo);
      hand.letGo();
    },
  });

  // Another device is shown: a list is closed, the scale in hand is let go, the writes waiting
  // for the old device are dropped.
  live.onSwitch(() => {
    closeList();
    for (const w of writers.values()) w.next = null;
    hand.forget();
    gripped = null;
    keep.reset();
  });

  // ---- the words ----
  /** The option of choice `c` that is current, for a tap that cannot write. */
  const currentOption = (c) => c.options?.find((o) => o.on) ?? c.options?.[0];

  box.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el) return;
    const c = controlOf(el.dataset.key);
    if (!c) return;
    const what = el.dataset.act;
    if (what === 'choice') {
      const option = c.options?.find((o) => o.label === el.dataset.value);
      if (option) write(c.key, option.label, JSON.parse(option.json));
    } else if (what === 'toggle') {
      write(c.key, c.on !== true, JSON.parse(c.json));
    } else if (what === 'centre') {
      if (typeof c.centre === 'number') write(c.key, c.centre, { raw: c.centre });
    } else if (what === 'choice-list') {
      if (inert()) {
        const option = currentOption(c);
        if (option) write(c.key, option.label, JSON.parse(option.json));
        return;
      }
      if (closeOf(el)) return;
      keep.around(() => openList({
        opener: el,
        label: c.label,
        options: (c.options ?? []).map((o) => ({ value: o.label, text: o.label, on: o.on })),
        choose(label) {
          const option = c.options.find((o) => o.label === label);
          if (option) write(c.key, option.label, JSON.parse(option.json));
        },
        signal,
        onClose: () => keep.drop(),
      }));
    }
  }, { signal });
}

