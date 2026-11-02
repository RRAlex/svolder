/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The strip along the bottom: one bar a second of what the device reports, drawn on a canvas at
// most 30 times a second. The page loads it with each screen and feeds it from the stream, and
// starts its frames once at start-up.

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

const KEEP_SECS = 1800;
let shades = null;

/** A design token's value. */
const token = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** The colours the strip draws with, read once from the page's tokens (and the view's accent). */
function colours() {
  if (shades) return shades;
  const names = ['--r-gold', '--r-gold-2', '--r-amber', '--r-amber-2', '--r-copper'];
  const stops = names.map((name) => {
    const hex = token(name) || '#f0a73a';
    return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  });
  /** The ramp's colour at `t`, 0 to 1. */
  const ramp = (t) => {
    const at = clamp(t, 0, 1) * (stops.length - 1);
    const i = Math.min(stops.length - 2, Math.floor(at));
    const [a, b] = [stops[i], stops[i + 1]];
    const mix = a.map((v, k) => Math.round(v + (b[k] - v) * (at - i)));
    return `#${mix.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  };
  const view = document.querySelector('#screen .view');
  const acc = (view && getComputedStyle(view).getPropertyValue('--acc').trim()) || token('--acc');
  const tints = {
    gold: token('--r-gold'),
    target: token('--r-target'),
    glow: token('--glow-3'),
    red: token('--r-red'),
    brass: token('--r-brass'),
    dim: token('--dim'),
    acc,
    good: token('--good'),
    warn: token('--warn'),
  };
  shades = {
    ramp,
    tints,
    stub: token('--hair').replace(/[\d.]+\)\s*$/, '.16)'),
    font: getComputedStyle(document.body).fontFamily,
  };
  return shades;
}

export const strip = {
  bars: new Map(),
  marks: new Map(),
  kind: 'dac',
  newest: 0,
  arrived: 0,

  /** Forget every bar and read the first paint's, when the page carries one. */
  load() {
    this.bars.clear();
    this.marks.clear();
    this.newest = 0;
    shades = null;
    const block = document.getElementById('strip');
    if (!block) return;
    try {
      this.add(JSON.parse(block.textContent));
    } catch {
      // No first paint: the stream's snapshot brings the bars.
    }
  },

  /** Take in bars and marks, a second's once each. */
  add(data) {
    if (!data) return;
    if (data.kind) this.kind = data.kind;
    for (const bar of data.bars ?? []) {
      this.bars.set(bar.t, bar);
      if (bar.t > this.newest) {
        this.newest = bar.t;
        this.arrived = performance.now();
      }
    }
    for (const mark of data.marks ?? []) this.marks.set(`${mark.t}|${mark.text}`, mark);
    const oldest = this.newest - KEEP_SECS;
    for (const t of this.bars.keys()) if (t < oldest) this.bars.delete(t);
    for (const [key, mark] of this.marks) if (mark.t < oldest) this.marks.delete(key);
  },

  /** The bar's colour: its tint, a ramp's by its height. */
  colour(bar, shade) {
    if (bar.tint !== 'ramp') return shade.tints[bar.tint] ?? shade.ramp(bar.h);
    return shade.ramp(this.kind === 'amp' ? (bar.h - 0.25) / 0.75 : bar.h);
  },

  /** Draw the bars newest at the right, sliding left through the current second. */
  draw(now) {
    const canvas = document.getElementById('hist');
    if (!canvas) return;
    const [w, h] = [canvas.clientWidth, canvas.clientHeight];
    if (!w || !h) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const [cw, ch] = [Math.round(w * dpr), Math.round(h * dpr)];
    if (canvas.width !== cw || canvas.height !== ch) {
      canvas.width = cw;
      canvas.height = ch;
    }
    const shade = colours();
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const pitch = w >= 700 ? 7 : 6;
    const width = pitch - 2;
    const top = h - 4;
    // The newest bar's second runs on by the wall clock, so a gap with no bars (the link away)
    // scrolls by as an empty stretch and the bars that follow land where they belong.
    const elapsed = clamp((now - this.arrived) / 1000, 0, KEEP_SECS);
    const head = this.newest + Math.floor(elapsed);
    const frac = elapsed - Math.floor(elapsed);
    const left = (t) => w - (head - t + 1 + frac) * pitch + 1;
    for (let k = 0; left(this.newest - k) + width >= 0; k++) {
      const bar = this.bars.get(this.newest - k);
      if (!bar) continue;
      const x = left(this.newest - k);
      if (bar.stub) {
        ctx.globalAlpha = 1;
        ctx.fillStyle = shade.stub;
        ctx.fillRect(x, h - 2, width, 2);
        continue;
      }
      const height = Math.max(2, bar.h * top);
      ctx.fillStyle = this.colour(bar, shade);
      ctx.globalAlpha = bar.dim ? 0.055 : 0.11;
      ctx.fillRect(x, h - height, width, height);
      ctx.globalAlpha = bar.dim ? 0.21 : 0.42;
      ctx.fillRect(x, h - height, width, 1.5);
    }
    this.drawMarks(ctx, shade, w, h, left, width);
    ctx.globalAlpha = 1;
  },

  /** Each mark as a hairline and a label at its top: the newer label wins a 64 px stretch. */
  drawMarks(ctx, shade, w, h, left, width) {
    const marks = [...this.marks.values()].sort((a, b) => b.t - a.t);
    const labelled = [];
    ctx.font = `500 10.5px ${shade.font}`;
    ctx.textBaseline = 'top';
    for (const mark of marks) {
      const x = left(mark.t) + width / 2;
      if (x < 0 || x > w) continue;
      const colour = shade.tints[mark.tint] ?? shade.ramp(0.5);
      ctx.fillStyle = colour;
      ctx.globalAlpha = 0.5;
      ctx.fillRect(Math.round(x), 0, 1, h);
      if (labelled.some((other) => Math.abs(other - x) < 64)) continue;
      labelled.push(x);
      ctx.globalAlpha = 1;
      ctx.textAlign = x > w - 90 ? 'right' : 'left';
      ctx.fillText(mark.text, x + (x > w - 90 ? -4 : 4), 2);
    }
  },
};

let lastDraw = 0;
let onScreen = true;
let watched = null;
let seen = null;

/** Note whether the strip's canvas is in view: it is not redrawn while scrolled away. */
function watchCanvas() {
  const canvas = document.getElementById('hist');
  if (!canvas || canvas === watched || typeof IntersectionObserver === 'undefined') return;
  watched = canvas;
  seen?.disconnect();
  seen = new IntersectionObserver((entries) => {
    onScreen = entries[entries.length - 1].isIntersecting;
  });
  seen.observe(canvas);
}

/** One animation frame: the strip, at most 30 a second, held still while the stream is lost or
 * the strip is out of view. */
function frame(now) {
  if (!running) return;
  requestAnimationFrame(frame);
  if (now - lastDraw < 33) return;
  lastDraw = now;
  if (document.hidden || document.body.classList.contains('lost')) return;
  watchCanvas();
  if (onScreen) strip.draw(now);
}

/** Whether the strip's animation frames are asked for. */
let running = false;

/** Start the strip's animation frames. */
export function startStrip() {
  if (running) return;
  running = true;
  requestAnimationFrame(frame);
}

/** Stop the strip's animation frames and stop watching its canvas. */
export function stopStrip() {
  running = false;
  seen?.disconnect();
  seen = null;
  watched = null;
}
