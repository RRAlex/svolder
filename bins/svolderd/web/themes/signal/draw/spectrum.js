/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Spectrum view of the computer's level meters: one line over the 240 smoothed bands the core's
// `VizModel` keeps (the two channels averaged; no beats, no held peaks), in the canvas's own text colour
// (`--sg-computer` through the style sheet) over a few faint octave marks. Drawn only while the view
// is shown; the canvas is sized by the loop that calls this.

import {
  BANDS, FREQ_MARKS, bandPos, freqPos,
} from '../../../core/viz-model.js';

/** The part of the canvas's height the loudest band reaches. */
const TOP = 0.92;

/** Size `canvas`'s backing store to its CSS size (`width` x `height` CSS pixels) and `dpr`, and return its
 * context drawn in CSS pixels, or null where there is none. */
export function fitCanvas(canvas, width, height, dpr) {
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  const ctx = canvas.getContext('2d');
  ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
  return ctx;
}

/** Draw the model's bands on `ctx` over `width` x `height` CSS pixels, in `colour`. */
export function drawSpectrum(ctx, width, height, model, colour) {
  ctx.clearRect(0, 0, width, height);
  ctx.lineWidth = 1;
  ctx.strokeStyle = colour;
  ctx.globalAlpha = 0.16;
  for (const hz of FREQ_MARKS) {
    const x = Math.round(freqPos(hz, width)) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  ctx.beginPath();
  for (let i = 0; i < BANDS; i++) {
    const x = bandPos(i, width);
    const y = height - Math.min(1, (model.left[i] + model.right[i]) / 2) * TOP * height - 1;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1.6;
  ctx.lineJoin = 'round';
  ctx.stroke();
  ctx.lineTo(width, height);
  ctx.lineTo(0, height);
  ctx.closePath();
  ctx.globalAlpha = 0.12;
  ctx.fillStyle = colour;
  ctx.fill();
  ctx.globalAlpha = 1;
}
