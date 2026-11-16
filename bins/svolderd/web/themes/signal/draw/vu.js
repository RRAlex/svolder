/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The computer's level meters in Signal: two dials (left, right) drawn from one geometry, the glow
// the person chose (a class on the section: the dial face is the button that turns it), the reading
// and the peak lamp under each, the reference control and the Meters | Spectrum chips. Pure: the
// state in, markup out. The needles and readings are put in place by vu-wire.js (`transform`
// through the style object: the page carries no `style` attribute), so a face is drawn once.
//
// The scale is the one the model's deflection `f` (0 at the left stop, 1 at the right, a little more
// at the pin) is drawn on: the ticks are put where the model's own `Scale.deflection` puts each VU mark.

import { Scale } from '../../../core/vu-model.js';
import { VU } from '../words.js';
import { esc } from './esc.js';

/** The face's geometry in its viewBox: size, pivot (`cx`, `py`), the scale's radius `r` and its half sweep `s`
 * in degrees, the unit's distance from the bottom `uy`. The CSS gives the needle's origin the same pivot. */
export const FACE = Object.freeze({
  w: 240, h: 118, cx: 120, py: 168, r: 130, s: 42, uy: 10,
});

/** The glows, in the order a tap on a face goes through them. */
export const GLOWS = Object.freeze(['w1', 'w2', 'w3']);

/** The deflection (0 to 1.03) of a level in VU (a ratio to the reference of 10^(VU/20)). */
const fracOf = (db) => Scale.deflection(10 ** (db / 20));

/** The angle of the needle, in degrees from straight up, at deflection `f`. */
export const needleAngle = (f) => -FACE.s + 2 * FACE.s * Math.max(0, Math.min(1.03, f));

const point = (radius, angle) => {
  const t = (angle * Math.PI) / 180;
  return [FACE.cx + radius * Math.sin(t), FACE.py - radius * Math.cos(t)];
};
const fx = (n) => n.toFixed(2);

function arc(radius, f0, f1) {
  const [x0, y0] = point(radius, needleAngle(f0));
  const [x1, y1] = point(radius, needleAngle(f1));
  return `M${fx(x0)} ${fx(y0)} A${radius} ${radius} 0 0 1 ${fx(x1)} ${fx(y1)}`;
}

const MAJOR = [-20, -10, -7, -5, -3, 0, 3];
const MINOR = [-15, -6, -4, -2, -1, 1, 2];
const markText = (d) => (d > 0 ? `+${d}` : d < 0 ? `−${-d}` : '0');

/** The face's scale: the arc, the red zone over 0 VU, the ticks and their numbers, the unit. */
function scale(unit) {
  const parts = [`<path class="arc" d="${arc(FACE.r, 0, fracOf(0))}" stroke-width="1.2"/>`,
    `<path class="zone" d="${arc(FACE.r - 3, fracOf(0), 1)}" stroke-width="6"/>`];
  for (const d of MINOR) {
    const a = needleAngle(fracOf(d));
    const [x1, y1] = point(FACE.r + 1, a);
    const [x2, y2] = point(FACE.r - 7, a);
    parts.push(`<line class="tk minor" x1="${fx(x1)}" y1="${fx(y1)}" x2="${fx(x2)}" y2="${fx(y2)}" stroke-width="1"/>`);
  }
  for (const d of MAJOR) {
    const a = needleAngle(fracOf(d));
    const [x1, y1] = point(FACE.r + 2, a);
    const [x2, y2] = point(FACE.r - 11, a);
    const [nx, ny] = point(FACE.r + 14, a);
    parts.push(`<line class="tk" x1="${fx(x1)}" y1="${fx(y1)}" x2="${fx(x2)}" y2="${fx(y2)}" stroke-width="${d === 0 ? 2.6 : 1.5}"/>`,
      `<text class="num${d > 0 ? ' hot' : ''}" x="${fx(nx)}" y="${fx(ny + 3.5)}">${markText(d)}</text>`);
  }
  parts.push(`<text class="unit" x="${FACE.cx}" y="${FACE.h - FACE.uy}">${esc(unit)}</text>`);
  return `<g class="scale">${parts.join('')}</g>`;
}

/** One needle: a wide faint stroke, a narrower one and the shaft (the bloom is part of the group, so
 * a frame is one transform); the right one ends in a ring, the left in a dot. */
function needle(channel) {
  const tipY = FACE.py - FACE.r + 4;
  const line = (cls) => `<line class="${cls}" x1="${FACE.cx}" y1="${FACE.py}" x2="${FACE.cx}" y2="${tipY}"/>`;
  const tip = channel === 'R'
    ? `<circle class="tip R" cx="${FACE.cx}" cy="${tipY}" r="3.2"/>`
    : `<circle class="tip L" cx="${FACE.cx}" cy="${tipY}" r="2.2"/>`;
  return `<g class="sg-vneedle" data-needle="${channel}">${line('bloom a')}${line('bloom b')}${line(`shaft ${channel}`)}${tip}</g>`;
}

/** The face of the dial of `channel` (`L` or `R`) as an SVG. */
export function face(channel) {
  return `<svg viewBox="0 0 ${FACE.w} ${FACE.h}" aria-hidden="true" focusable="false">${scale(`${channel} · ${VU.unit}`)}`
    + `${needle(channel)}<rect class="sill" x="0" y="${FACE.h - 3}" width="${FACE.w}" height="3"/></svg>`;
}

/** The dial of `channel`: the face (a button that turns the glow), then the channel's name, its peak lamp
 * and its reading. */
export function dial(channel, glow) {
  const side = channel === 'L' ? VU.left : VU.right;
  return `<div class="sg-vdial"><button type="button" class="sg-vface" data-act="vu-glow" data-ch="${channel}"`
    + ` id="sg-vface-${channel}" aria-label="${esc(VU.glowWords(glow))}">${face(channel)}</button>`
    + `<div class="sg-vrd" role="group" aria-label="${esc(VU.channel(side))}"><span class="sg-vch" aria-hidden="true">${channel}</span>`
    + `<span class="sg-vlamp" data-lamp="${channel}" id="sg-vlamp-${channel}">${esc(VU.peak)}<span class="sg-sr">${esc(VU.peakState(false))}</span></span>`
    + `<span class="sg-vnum" data-read="${channel}" id="sg-vnum-${channel}">${esc(VU.below)}</span></div></div>`;
}

/** The reference control: what 0 VU is, `−` and `+` (a dB each) and Reset, drawn only when the reference
 * is not the default. */
export function reference(ref, defaults, { min, max }) {
  return `<div class="sg-vref"><span class="sg-lbl">${esc(VU.reference)}</span>`
    + `<output id="sg-vref-out" class="sg-vout">${esc(VU.refWords(ref))}</output>`
    + `<button type="button" class="sg-btn sg-small" data-act="vu-ref-down" id="sg-vref-down"`
    + ` aria-label="${esc(VU.lower)}"${ref <= min ? ' aria-disabled="true"' : ''}>−</button>`
    + `<button type="button" class="sg-btn sg-small" data-act="vu-ref-up" id="sg-vref-up"`
    + ` aria-label="${esc(VU.raise)}"${ref >= max ? ' aria-disabled="true"' : ''}>+</button>`
    + `<button type="button" class="sg-btn sg-pill" data-act="vu-ref-reset" id="sg-vref-reset"`
    + `${ref === defaults ? ' hidden' : ''}>${esc(VU.reset(defaults))}</button></div>`;
}

/** The two chips that choose Meters or Spectrum. */
function views(view) {
  const chip = (name, text) => `<button type="button" class="sg-btn sg-pill" data-act="vu-view" data-view="${name}"`
    + ` id="sg-vview-${name}" aria-pressed="${view === name}">${esc(text)}</button>`;
  return `<div class="sg-vviews" role="group" aria-label="${esc(VU.views)}">${chip('meters', VU.meters)}${chip('spectrum', VU.spectrum)}</div>`;
}

/**
 * The section: the chips in its corner, then the Meters pane (two dials, the reference, the note) or the
 * Spectrum pane (a canvas), one of them hidden. `state` is `{ glow, view, ref, defaults, min, max, note }`.
 */
export function vuArea(state) {
  const meters = state.view === 'meters';
  return `<section id="sg-cvu" class="sg-vu sg-${state.glow}" data-view="${state.view}">`
    + `<div class="sg-vhead"><span class="sg-lbl">${esc(VU.title)}</span>${views(state.view)}</div>`
    + `<div class="sg-vpane" data-pane="meters"${meters ? '' : ' hidden'}>`
    + `<div class="sg-vdials">${dial('L', state.glow)}${dial('R', state.glow)}</div>`
    + `<p class="sg-note sg-vstate" id="sg-vstate">${esc(state.note ?? '')}</p>`
    + `${reference(state.ref, state.defaults, state)}<p class="sg-note">${esc(VU.note)}</p></div>`
    + `<div class="sg-vpane" data-pane="spectrum"${meters ? ' hidden' : ''}>`
    + `<canvas id="sg-vspec" class="sg-vspec" role="img" aria-label="${esc(VU.spectrumLabel)}"></canvas></div></section>`;
}
