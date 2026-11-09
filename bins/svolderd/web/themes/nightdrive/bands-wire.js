/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the sheet's band table and the loudness strength: a value stepped with `−` and `+`
// (a held one repeats through the core's hold) or typed (Enter writes it, Escape puts the old one
// back). Every write goes through `core.actions`; a viewer who may not write taps the same keys and
// the core says why nothing happens.
//
// The sheet is held while a value is typed or a stepper is held. A held stepper's reading is put into
// the table in place, so the key under the finger is never replaced and the number still moves; the
// sheet is drawn from the newest model once the hand is away. The hold steps the key of the same id
// and ends with the pointer's release or cancel anywhere, the window losing focus, the page hidden,
// the stream lost, another device shown, no such key left or the 10 s cap.

import { WORDS } from './draw/bands.js';
import { inertOf } from './inert.js';

/** The sheet's steppers a long press repeats: a band's value, the loudness strength and a value of the computer's
 * points. */
const STEPPERS = '[data-act="band"][data-step], [data-act="loudness"][data-step], [data-act="point-step"][data-step]';

/** A hold ends at this age (ms), whatever else. */
const CAP_MS = 10_000;

/** Wire the band table and the loudness steppers of the sheet (`sheet.body()`) until `signal` aborts. */
export function wireBands({
  core, live, sheet, view, signal,
}) {
  const store = core.model;
  const { BAND_PARAM } = core.drag;
  const { keep } = sheet;
  const wrap = sheet.wrap;
  const inert = () => inertOf(store.page());

  // ---- a value typed ----
  let typing = null;
  wrap.addEventListener('focusin', (event) => {
    const field = event.target.closest?.('input.nd-v');
    if (!field || typing === field) return;
    typing = field;
    keep.take();
  }, { signal });
  wrap.addEventListener('focusout', (event) => {
    const field = event.target.closest?.('input.nd-v');
    if (!field || typing !== field) return;
    typing = null;
    // What was typed and not sent is put back; a value sent shows once the device has it.
    field.value = field.defaultValue;
    keep.dropLater();
  }, { signal });
  wrap.addEventListener('keydown', (event) => {
    const field = event.target.closest?.('input.nd-v');
    if (!field) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      field.value = field.defaultValue;
      field.blur();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (field.dataset.point) {
        // what was typed is rounded, held to the hand range and written as a last value; a cell left as it was writes nothing
        core.actions.computer.cell(Number(field.dataset.point), field.dataset.param, field.value, field.defaultValue);
        field.blur();
        return;
      }
      const { band, param } = field.dataset;
      core.actions.setBand(Number(band), BAND_PARAM[param], { value: field.value }, { device: store.shown() });
      field.blur();
    }
    // (in the capture phase: the sheet's own Escape, on the same element, closes the sheet)
  }, { signal, capture: true });

  /** The press of a stepper that is held now (the core's hold), or null. */
  let hold = null;
  /** Whether a hold is on: a step of a hold that has ended (its repeat already on the way) is none. */
  let holding = false;

  // ---- the values put in place while a stepper is held ----
  /** The model's band values and strength put into the table as they are, but for the field typed in. */
  function patch() {
    const body = sheet.body();
    const tone = store.page().device?.tone;
    if (!body || !tone) return;
    for (const r of tone.bands?.rows ?? []) {
      for (const param of ['f', 'g', 'q']) {
        const field = body.querySelector(`#nd-b${r.band}${param}`);
        if (!field || field === typing) continue;
        field.value = r[param];
        field.defaultValue = r[param];
      }
    }
    const strength = body.querySelector('#nd-loud i');
    if (strength && tone.strength !== undefined) strength.textContent = `strength ${tone.strength ?? '—'}`;
  }
  /** The computer's point values put into the table as they are while a stepper is held, but for the field typed in. */
  function patchPoints() {
    const body = sheet.body();
    const tone = core.computer.tone();
    if (!body || !tone) return;
    for (const p of tone.points) {
      for (const param of ['f', 'g', 'q']) {
        const field = body.querySelector(`#nd-p${p.n}${param}`);
        if (!field || field === typing) continue;
        const text = WORDS[param](p[param]);
        field.value = text;
        field.defaultValue = text;
      }
    }
  }
  /** Whether a stepper may step and a hold may repeat: one rule, so a step is never taken that its hold cannot end. */
  const canStep = () => !inert() && view.state !== 'lost' && view.state !== 'waiting';
  /** The hand that steps the computer's points for the steppers (the core's burst: each step a live write, the last
   * value once as the final one when the hold ends; it follows the point through a re-sort and ends if the point is
   * removed). */
  const stepper = core.drag.pointDrag({ points: () => [], usable: () => canStep() });
  const offComputer = core.computer.on(({ type }) => {
    // a flip, or the point stepped removed by another front end: the hold is over (never on to the point that took its number)
    if (type === 'flip') hold?.end();
    if (type === 'gone') hold?.end();
    if (type !== 'flip' && hold && sheet.isOpen()) patchPoints();
  });
  signal.addEventListener('abort', () => {
    offComputer();
    stepper.destroy();
  }, { once: true });
  live.onReading('part', ({ device, name }) => {
    if (hold && device === store.shown() && name === 'tone' && sheet.isOpen()) patch();
  });

  // ---- the steppers ----
  /** One step of the stepper `el`, to `target`; `held` for a step of a held press. */
  function stepOnce(el, target, held = false) {
    if (el.dataset.act === 'point-step') {
      // a held press goes on with the same point after a re-sort: the burst follows it, the drawn number may not
      const n = held && stepper.burst ? stepper.burst.n : Number(el.dataset.point);
      stepper.step(n, el.dataset.param, Number(el.dataset.step), { hold: held });
    } else if (el.dataset.act === 'band') {
      core.actions.setBand(Number(el.dataset.band), BAND_PARAM[el.dataset.param],
        { step: Number(el.dataset.step) }, { device: target });
    } else {
      core.actions.setLoudness({ step: Number(el.dataset.step) }, { device: target });
    }
  }

  live.onSwitch(() => hold?.end());
  wrap.addEventListener('pointerdown', (event) => {
    const first = event.target.closest?.(STEPPERS);
    if (!first || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    const target = store.shown();
    holding = true;
    hold = core.hold.repeat(event, {
      root: wrap,
      el: first,
      step: (el) => holding && stepOnce(el, target, true),
      find: () => (first.id ? wrap.querySelector(`#${CSS.escape(first.id)}`) : null),
      live: canStep,
      cap: CAP_MS,
      onStart: () => keep.take(),
      onEnd: () => {
        hold = null;
        holding = false;
        stepper.finishBurst();
        keep.drop();
      },
      signal,
    });
  }, { signal });
  // A long press on a phone must not open the context menu over a held stepper.
  wrap.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPPERS)) event.preventDefault();
  }, { signal });
  // A click with no pointer is a key: one step. A press of the pointer stepped already.
  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.(STEPPERS);
    if (el && event.detail === 0) stepOnce(el, store.shown());
  }, { signal });
  signal.addEventListener('abort', () => hold?.end(), { once: true });

  // typing is over when the sheet is drawn for another device
  live.onSwitch(() => { typing = null; });
}
