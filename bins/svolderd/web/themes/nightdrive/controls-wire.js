/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the sheet's Controls and the device's own presets: a switch, a choice's chips and the list
// a longer one opens, the power key and its switches, a scale dragged, stepped with the keys or sent to its
// centre by a tap on its value, and the device's slots (recalled, or written over after a question) and
// built-in settings. Every write goes through `core.actions` and is logged by `act`; a scale writes the
// step under the hand at once, newest wins (one write in flight, the latest waiting). A viewer who may not
// write taps the same words and the core says why nothing happens.
//
// The sheet is held while a scale is in hand (a drag, a burst of keys, the write still to be answered),
// while a list or a question is open, so a change from elsewhere does not move what is under the hand: it
// is drawn from the newest model once the hand is away. What is written is written to the device that
// was shown when the word was tapped.

import { askQuestion } from './ask.js';
import { fractionOf, rawAt } from './draw/controls.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { inertOf } from './inert.js';
import { closeList, closeOf, openList } from './lists.js';
import { metersInPlace } from './draw/meters.js';
import { placeSheet } from './place.js';
import { SHEET } from './words.js';

/** How long after the last key or move a scale is let go, so the echo of the last write lands before the
 * sheet is drawn again (ms). */
const LET_GO_MS = 250;

/** Wire the controls of the sheet (`sheet.wrap`) until `signal` aborts. */
export function wireControls({
  core, live, act, sheet, signal,
}) {
  const store = core.model;
  const { keep, release, wrap } = sheet;
  const controlOf = (key) => (store.device()?.controls ?? []).find((c) => c.key === key);
  const inert = () => inertOf(store.page());

  /** Write `body` to control `key` for `value`, logged, to `device` (the one shown now unless said). */
  const write = (key, value, body, device = store.shown()) => act.act(`control:${key}`, value,
    (a) => a.setControl(key, body, { device })).catch(() => {});

  // ---- a scale in hand ----
  let pending = Promise.resolve();
  const hand = keep.hand({ pending: () => pending, ms: LET_GO_MS });
  signal.addEventListener('abort', () => hand.stop(), { once: true });

  /** One write in flight per scale of a device, the latest waiting, each to the device the scale was shown for: a
   * step asked after a switch is the new device's own run, never the old one's. */
  const writers = new Map();
  function send(key, raw) {
    const device = store.shown();
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

  const shownRaw = (scale) => Number(scale.getAttribute('aria-valuenow'));

  /** Show step `raw` on the scale of `c` at once: its knob, its words and its value. */
  function show(scale, c, raw) {
    const knob = scale.querySelector('.nd-knob')
      ?? scale.appendChild(Object.assign(scale.ownerDocument.createElement('span'), { className: 'nd-knob' }));
    knob.dataset.at = fractionOf(c, raw).toFixed(5);
    placeSheet(scale);
    // the device's own words for the step where it gave them, the number where it did not
    const text = c.texts?.[raw - c.lo] ?? String(raw);
    scale.setAttribute('aria-valuenow', String(raw));
    scale.setAttribute('aria-valuetext', text);
    const word = scale.closest('.nd-level')?.querySelector('.nd-val');
    if (word) word.textContent = text;
  }

  /** Ask for step `raw` of the scale's control: held to the range, shown, sent; a viewer who may not write
   * is told by the core and nothing moves. */
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

  let gripped = null;
  dragAlong(wrap, {
    signal,
    pick: (event) => event.target.closest?.('#nd-controls .nd-scale'),
    onStart(at, event) {
      const scale = event.target.closest('.nd-scale');
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

  sliderKeys(wrap, {
    signal,
    match: '#nd-controls .nd-scale',
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

  // Another device is shown: a list or question open for the old one is closed, the scale in hand is let
  // go, the writes waiting for the old device are dropped.
  live.onSwitch(() => {
    closeList();
    for (const w of writers.values()) w.next = null;
    hand.forget();
    gripped = null;
  });

  // ---- the amp's meters: a reading is put into the needles in place, the sheet is not drawn again ----
  live.onReading('meters', (data) => {
    if ((data.device ?? store.shown()) !== store.shown() || !sheet.isOpen()) return;
    if (!metersInPlace(wrap, store.device())) live.draw('sheet');
  });

  // ---- the words ----
  /** The option of choice `c` that is current, for a tap that cannot write. */
  const currentOption = (c) => c.options?.find((o) => o.on) ?? c.options?.[0];

  /** Where a question about a slot opens: the slots' own place. */
  const within = (el) => el.closest('.nd-sec')?.querySelector('.nd-ask') ?? wrap.querySelector('.nd-ask');

  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.('#nd-controls [data-act], #nd-slots [data-act]');
    if (!el) return;
    const device = store.device();
    if (!device) return;
    const what = el.dataset.act;
    // a word that cannot write does nothing, but for the viewer the core turns away: that call is made so the
    // toast says why
    if (el.getAttribute('aria-disabled') === 'true' && !inert()) return;
    const opened = { device: store.shown() };
    if (what === 'power' || what === 'power-toggle') {
      const p = device.power;
      const target = what === 'power' ? p : (p?.toggles ?? []).find((t) => t.key === el.dataset.key);
      if (target?.json) write(target.key, target.on !== true, JSON.parse(target.json));
      return;
    }
    if (what === 'slot') {
      const slot = Number(el.dataset.slot);
      act.act('slot', slot, (a) => a.recallSlot(slot, opened)).catch(() => {});
      return;
    }
    if (what === 'setting') {
      act.act('setting', el.dataset.name, (a) => a.applySetting(el.dataset.name, opened)).catch(() => {});
      return;
    }
    if (what === 'slot-save-list') {
      const options = (device.device_presets?.slots ?? []).map((s) => ({ value: s.slot, text: s.name, on: false }));
      if (closeOf(el)) return;
      keep.around(() => openList({
        opener: el,
        label: SHEET.saveList,
        options,
        choose(value) {
          const slot = options.find((o) => String(o.value) === String(value));
          // the write is sent only from the question's `Save`
          keep.around(() => askQuestion({
            within: within(el), words: SHEET.askSlot(slot.text), yes: 'save', no: 'keep', signal, opener: el,
            onClose: () => keep.dropLater(),
            go: () => act.act('slot', slot.value, (a) => a.saveSlot(slot.value, true, opened)),
          }));
        },
        signal,
        onClose: release,
      }));
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
