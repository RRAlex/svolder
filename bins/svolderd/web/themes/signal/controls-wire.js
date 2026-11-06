/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the shelf's controls: a band's value stepped with `−` and `+` or typed (Enter writes
// it, Escape puts the old one back), the loudness strength's two steppers, a switch, a choice's
// chips and the list a longer one opens, and a scale dragged, stepped with the keys or sent to its
// centre by a tap on its value. Every write goes through `core.actions` and is logged by `act`; a
// scale writes the step under the hand at once, newest wins (one write in flight, the latest
// waiting). A viewer who may not write taps the same words and the core says why nothing happens.
//
// The shelf is held while a value is being typed, while a list is open and while a scale is in
// hand (a drag, a burst of keys, the write still to be answered), so a change from elsewhere does
// not move what is under the hand: it is drawn from the newest model once the hand is away.

import { rawAt } from './draw/volume.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { closeList, closeOf, openList } from './lists.js';
import { placeOpen } from './place.js';

/** The shelf's steppers a long press repeats: a band's value and the loudness strength. */
const STEPPERS = '[data-act="band"][data-step], [data-act="loudness"][data-step]';

/** How long after the last key or move a scale is let go, so the echo of the last write lands
 * before the shelf is drawn again (ms). */
const LET_GO_MS = 250;

/** Wire the shelf's controls in `root` until `signal` aborts; `keep` is the shelf's count of holds and
 * `release()` gives one back once the press that closed a list is over (press.js). */
export function wireControls({
  root, core, live, act, view, keep, release, signal,
}) {
  const box = root.querySelector('#sg-shelf');
  const { BAND_PARAM } = core.drag;
  const controlOf = (key) => (view.device()?.controls ?? []).find((c) => c.key === key);
  const inert = () => Boolean(view.device()?.inert);

  /** Write `body` to control `key` for `value`, logged, to `device` (the one shown now unless said). */
  const write = (key, value, body, device = view.target()) => act.act(`control:${key}`, value,
    (a) => a.setControl(key, body, { device })).catch(() => {});

  // ---- a value typed ----
  let typing = null;
  box.addEventListener('focusin', (event) => {
    const field = event.target.closest?.('input.sg-val');
    if (!field || typing === field) return;
    typing = field;
    keep.take();
  }, { signal });
  box.addEventListener('focusout', (event) => {
    const field = event.target.closest?.('input.sg-val');
    if (!field || typing !== field) return;
    typing = null;
    // What was typed and not sent is put back; a value sent shows once the device has it.
    field.value = field.defaultValue;
    keep.dropLater();
  }, { signal });
  box.addEventListener('keydown', (event) => {
    const field = event.target.closest?.('input.sg-val');
    if (!field) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      field.value = field.defaultValue;
      field.blur();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const { band, param } = field.dataset;
      core.actions.setBand(Number(band), BAND_PARAM[param], { value: field.value },
        { device: view.target() });
      field.blur();
    }
  }, { signal });

  // ---- a scale in hand ----
  let pending = Promise.resolve();
  const hand = keep.hand({ pending: () => pending, ms: LET_GO_MS });
  signal.addEventListener('abort', () => hand.stop(), { once: true });

  /** One write in flight per scale of a device, the latest waiting, each to the device the scale was
   * shown for; `pending` is the write now in flight. */
  const writers = new Map();
  function send(key, raw) {
    const device = view.target();
    const id = `${device}|${key}`;
    let w = writers.get(id);
    if (!w) {
      w = { busy: null, next: null };
      writers.set(id, w);
    }
    if (w.busy) {
      w.next = raw;
      return;
    }
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

  /** The step the scale shows now. */
  const shownRaw = (scale) => Number(scale.getAttribute('aria-valuenow'));

  /** Show step `raw` on the scale of `c` at once: its knob, its words and its value. */
  function show(scale, c, raw) {
    const knob = scale.querySelector('.sg-sknob')
      ?? scale.appendChild(Object.assign(scale.ownerDocument.createElement('span'),
        { className: 'sg-sknob' }));
    knob.dataset.at = ((raw - c.lo) / (c.hi - c.lo)).toFixed(5);
    placeOpen(scale);
    // The device's own words for the step where it gave them, the number where it did not.
    const text = c.texts?.[raw - c.lo] ?? String(raw);
    scale.setAttribute('aria-valuenow', String(raw));
    scale.setAttribute('aria-valuetext', text);
    const word = scale.closest('.sg-level')?.querySelector('.sg-sval');
    if (word) word.textContent = text;
  }

  /** Ask for step `raw` of the scale's control: held to the range, shown, sent; a viewer who may
   * not write is told by the core and nothing moves. */
  function ask(scale, raw) {
    const c = controlOf(scale.dataset.key);
    if (!c || typeof c.raw !== 'number') return;
    const step = Math.min(c.hi, Math.max(c.lo, Math.round(raw)));
    if (inert()) {
      core.actions.setControl(c.key, { raw: step }, { device: view.target() });
      return;
    }
    if (step === shownRaw(scale)) return;
    hand.grip();
    show(scale, c, step);
    send(c.key, step);
  }

  let gripped = null;
  dragAlong(box, {
    signal,
    pick: (event) => event.target.closest?.('.sg-scale'),
    onStart(at, event) {
      const scale = event.target.closest('.sg-scale');
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
    match: '.sg-scale',
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

  // Another device is shown, or another hop chosen: a list is closed, the scale in hand is let go,
  // the writes waiting for the old device are dropped.
  const away = () => {
    closeList();
    for (const w of writers.values()) w.next = null;
    hand.forget();
    gripped = null;
    typing = null;
    keep.reset();
  };
  live.onSwitch(away);
  live.onChoice(away);

  // ---- the words ----
  /** The option of choice `c` that is current, for a tap that cannot write. */
  const currentOption = (c) => c.options?.find((o) => o.on) ?? c.options?.[0];

  /** One step of the stepper `el`: a band's value or the loudness strength. */
  function stepOnce(el) {
    if (el.dataset.act === 'band') {
      core.actions.setBand(Number(el.dataset.band), BAND_PARAM[el.dataset.param],
        { step: Number(el.dataset.step) }, { device: view.target() });
    } else {
      core.actions.setLoudness({ step: Number(el.dataset.step) }, { device: view.target() });
    }
  }

  // A held stepper is the core's hold: one step at the press, then, after a pause, steps at a
  // steady pace, as the volume's. The shelf is drawn again from each answer, so the hold follows
  // the pointer, not the button: it steps the button of the same id while one is there, and ends
  // with the pointer's release or cancel anywhere, the window losing focus, the page hidden or its
  // stream lost, another device shown, no such button left, or the cap.
  let hold = null;
  live.onSwitch(() => hold?.end());
  /** A press on a stepper of the shelf, or on the Loudness button's round ones under the curve. */
  function onPress(event) {
    const first = event.target.closest?.(STEPPERS);
    if (!first || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    hold = core.hold.repeat(event, {
      root,
      el: first,
      step: stepOnce,
      live: () => !view.device()?.inert && view.state !== 'lost' && view.state !== 'waiting',
      signal,
    });
  }
  const open = root.querySelector('#sg-open');
  for (const area of [box, open]) {
    area.addEventListener('pointerdown', onPress, { signal });
    // A long press on a phone must not open the context menu over a held stepper.
    area.addEventListener('contextmenu', (event) => {
      if (event.target.closest?.(STEPPERS)) event.preventDefault();
    }, { signal });
  }
  // The round `−` and `+` under the curve: a click with no pointer is a key, one step.
  open.addEventListener('click', (event) => {
    const el = event.target.closest?.(STEPPERS);
    if (el && event.detail === 0) stepOnce(el);
  }, { signal });

  box.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    // the input under the band table is the tone wiring's (tone-wire.js)
    if (!el || el.closest('#sg-choices')) return;
    const what = el.dataset.act;
    if (el.matches(STEPPERS)) {
      // A press of the pointer stepped already; a click with no pointer is a key: one step.
      if (event.detail === 0) stepOnce(el);
      return;
    }
    if (what === 'power' || what === 'power-toggle') {
      const p = view.device()?.power;
      const target = what === 'power' ? p : (p?.toggles ?? []).find((t) => t.key === el.dataset.key);
      if (target?.json) write(el.dataset.key, target.on !== true, JSON.parse(target.json));
      return;
    }
    const c = controlOf(el.dataset.key);
    if (!c) return;
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
        onClose: release,
      }));
    }
  }, { signal });
}
