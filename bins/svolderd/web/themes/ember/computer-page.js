/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the theme Ember's computer side reads of the page and moves on it while a drag or a stepper
// is held: whether its controls may be used, the editor of the point in hand, and the curve, the
// points, the cells and the drag's label, moved to what the core holds now. While one is on, the
// tone part's swap waits (keep.js); the part is drawn again from the host's model afterwards.

import { ember } from './bind.js';
import {
  curveOf, editorBody, pointWords,
} from './draw/computer.js';
import { LABEL_GAP_PX, labelAt } from '../../core/tone-drag.js';
import { fixedEven } from '../../core/words.js';

/** The core's tone control now, with the values being changed laid over it. */
export const tone = () => ember.core.computer.tone();

/** Whether the computer's controls may be used: the page takes writes and the stream is up. */
export function usable() {
  const part = document.getElementById('part-tone');
  const body = document.body.classList;
  return Boolean(part) && !part.hasAttribute('data-inert') && !part.closest('[inert]')
    && ember.core.actions.canWrite() && !body.contains('lost');
}

/** Whether `element` is a control that is switched off. */
export const off = (element) => element.getAttribute('aria-disabled') === 'true' || element.disabled;

/** The editor under the table shows the point in hand: its three values with a - and a + each. */
export function paintEditor() {
  const box = document.querySelector('#part-tone .bedit');
  if (!box) return;
  const n = ember.core.computer.picked();
  const point = n === null ? null : tone()?.points[n - 1];
  box.hidden = !point;
  if (!point) {
    box.replaceChildren();
    delete box.dataset.n;
  } else if (box.dataset.n !== String(point.n) || !box.querySelector('.stp')) {
    box.dataset.n = String(point.n);
    box.innerHTML = editorBody(point, !ember.core.actions.canWrite());
  } else {
    for (const [param, text] of Object.entries(pointWords(point))) {
      const value = box.querySelector(`.stp[data-param="${param}"] .v`);
      if (value && value.textContent !== text) value.textContent = text;
    }
  }
}

/** Move the curve, the points and the cells to what the core holds now (a drag in motion). */
export function patch() {
  const t = tone();
  const part = document.getElementById('part-tone');
  if (!t || !part?.classList.contains('comp')) return;
  const c = curveOf(t);
  const svg = part.querySelector('.eqg svg');
  const line = svg?.querySelector('.line');
  line?.setAttribute('d', c.line);
  if (line) line.dataset.y = c.ys;
  svg?.querySelector('.area')?.setAttribute('d', c.area);
  svg?.querySelector('.ghost')?.setAttribute('d', c.ghost ?? '');
  for (const m of t.markers) {
    const mark = svg?.querySelector(`.mk[data-n="${m.n}"]`);
    mark?.setAttribute('cx', fixedEven(m.x, 1));
    mark?.setAttribute('cy', fixedEven(m.y, 1));
  }
  for (const p of t.points) {
    const row = part.querySelector(`.pt[data-n="${p.n}"]`);
    if (!row) continue;
    for (const [param, words] of Object.entries(pointWords(p))) {
      const cell = row.querySelector(`.cell[data-param="${param}"]`);
      if (cell && cell.textContent !== words && !cell.querySelector('input')) cell.textContent = words;
    }
  }
  paintEditor();
  const pline = part.querySelector('.pline');
  if (pline) pline.firstChild.textContent = t.line;
}

/** The label beside the dragged point (the core's session `s`): its three values, above it and
 * inside the plot. */
export function placeLabel(s) {
  const rect = s.plot.getBoundingClientRect();
  const at = ember.core.eq.points.markersIn(tone()?.markers.filter((m) => m.n === s.n), rect)[0];
  const words = s.words();
  if (!at || !words) return;
  s.label.textContent = words;
  const size = { width: s.label.offsetWidth, height: s.label.offsetHeight };
  const place = labelAt(at, { width: rect.width, height: rect.height }, size);
  if (at.y - LABEL_GAP_PX - size.height < 0) {
    place.y = Math.min(Math.max(at.y + LABEL_GAP_PX, 0), Math.max(rect.height - size.height, 0));
  }
  s.label.style.setProperty('--lx', place.x.toFixed(1));
  s.label.style.setProperty('--ly', place.y.toFixed(1));
}

/** The core's touch watch of each `touch` (`{ move, end }`): one per handlers' pair, so the
 * listeners added to a plot are the ones taken off it. */
const watches = new WeakMap();

/** The touch listeners a drag by finger needs on its plot: `touch.move` and `touch.end`. */
function watchTouch(plot, touch, on) {
  if (!watches.has(touch)) watches.set(touch, ember.core.eq.points.touchWatch(touch));
  watches.get(touch)(plot, on);
}

/** A drag begins on the core's session `s`: its label, the grab cursor and the lit marker. */
export function showDrag(s, touch) {
  s.label = document.createElement('span');
  s.label.className = 'dlab';
  s.label.setAttribute('aria-hidden', 'true');
  s.marker = s.plot.querySelector(`.mk[data-n="${s.n}"]`);
  s.plot.append(s.label);
  s.plot.classList.remove('hot');
  s.plot.classList.add('grabbing');
  s.marker?.classList.add('on');
  if (s.touchIds) watchTouch(s.plot, touch, true);
}

/** The drag has let go of the pointer: its label goes. */
export function hideDrag(s, touch) {
  watchTouch(s.plot, touch, false);
  s.plot.classList.remove('grabbing');
  s.label.remove();
  s.marker?.classList.remove('on');
}

/** The point of the drag was listed elsewhere: the lit marker and the label follow it. */
export function followDrag(s) {
  s.marker?.classList.remove('on');
  s.marker = s.plot.querySelector(`.mk[data-n="${s.n}"]`);
  s.marker?.classList.add('on');
  placeLabel(s);
}
