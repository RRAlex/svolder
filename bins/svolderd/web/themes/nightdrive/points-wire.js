/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the sheet's table of the computer's points: `+ Add point`, the `×` that removes a row and
// the type chosen in a row (a value typed into a field is bands-wire.js's, which holds the sheet while it is
// typed). Every write goes through `core.actions.computer`; a viewer who may not write taps the same keys
// and the core says why nothing happens.

import { held, parse } from './points.js';
import { SHEET } from './words.js';

/** The write a field of the points' table asks on Enter: its value held to the hand range, `done`. `null`
 * for a value left as it was, and (with the toast said) for words that are no number. */
export function writeField(core, field) {
  const n = Number(field.dataset.point);
  const { param } = field.dataset;
  const value = parse(field.value);
  if (value === null) {
    core.frame.toast(SHEET.notNumber);
    return null;
  }
  // a cell left as it was writes nothing (a point beyond the hand range would be pulled in to it)
  if (value === parse(field.defaultValue)) return null;
  return core.actions.computer.setPoint(n, { [param]: held(param, value) }, { done: true });
}

/** Wire the table of points in the sheet (`sheet.wrap`) to `core` until `signal` aborts. */
export function wirePoints({
  core, act, sheet, signal,
}) {
  const store = core.model;
  const { wrap } = sheet;
  const { computer } = core;

  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act="point-add"], [data-act="point-remove"]');
    if (!el || computer.side() !== 'computer') return;
    if (el.dataset.act === 'point-remove') {
      const n = Number(el.dataset.point);
      act.act('point', n, (a) => a.computer.removePoint(n)).catch(() => {});
      return;
    }
    const tone = computer.toneOf(store.page());
    const point = tone && computer.addAt(computer.nextFree(tone.points), 0);
    if (point) act.act('point', 'add', (a) => a.computer.addPoint(point)).catch(() => {});
  }, { signal });

  wrap.addEventListener('change', (event) => {
    const select = event.target.closest?.('select[data-act="point-type"]');
    if (!select || computer.side() !== 'computer') return;
    const n = Number(select.dataset.point);
    const type = select.value;
    select.blur();
    act.act('point', n, (a) => a.computer.setPoint(n, { type }, { done: true })).catch(() => {});
  }, { signal });
}
