/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The audio panel's drawing: its markup (DOM text for the ten features' labels and values and the
// lamps, so a screen reader and the driver read the words) and the one 2D canvas that draws the
// traces over each feature's empty slot: a bar with its peak mark and the 18 s history beside it
// (Panel), and in Spread each feature in its own format around the screen's sides: bass, mids and
// highs as upright meters with a trail, loudness and novelty as strips, centroid and flatness as
// dials, and the stereo rows. The markup is pure; `paintPanel` draws into a context it is given,
// placing each trace by the slot's box (`rect` reads it).
//
//   panelMarkup(mode, words, opt) the panel's markup for `mode` ('panel' or 'spread'); `opt.phone`
//                                 gives the Panel its 9 s history
//   panelInk(pair)                the three colours of the scene's live palette the traces take
//   PANEL_LAYOUT                     the Panel's measures (the panel's own)
//   FORMS                         the format of each feature in Spread
//   paintPanel(ctx, root, data, opt)   draw the traces of `root`

import { KEYS } from '../features.js';
import { PANEL } from '../words.js';

/** The format of each feature in Spread; in Panel every one is a row. */
export const FORMS = Object.freeze({
  bass: 'col',
  mid: 'col',
  high: 'col',
  loud: 'strip',
  flux: 'strip',
  cent: 'dial',
  flat: 'dial',
  l: 'row',
  r: 'row',
  width: 'row',
});

/** The Panel's measures, in CSS pixels, as the panel is drawn: the box, its row pitch, a bar
 * and where the trace starts in a row's slot (the slot begins at the bar). */
export const PANEL_LAYOUT = Object.freeze({
  width: 384, row: 15, bar: 100, barH: 5, trace: 150, traceW: 160, segments: 24,
});

/** The history the Panel shows, seconds, and where its tick marks stand (seconds before now). */
export const WINDOWS = Object.freeze({
  desk: { seconds: 18, ticks: [15, 10, 5] },
  phone: { seconds: 9, ticks: [6, 3] },
});

/** The panel's markup: a feature's label, value and trace slot, the history's axis, then the lamps
 * and the rate. */
export function panelMarkup(mode, words = {}, { phone = false } = {}) {
  const form = (key) => (mode === 'spread' ? FORMS[key] : 'bars');
  const win = phone ? WINDOWS.phone : WINDOWS.desk;
  const feats = KEYS.map((key) => `<div class="vg-h-feat" data-key="${key}"`
    + ` data-form="${form(key)}">`
    + `<span class="vg-h-k">${PANEL.features[key]}</span>`
    + `<span class="vg-h-v">${words[key] ?? '0'}</span>`
    + `<i class="vg-h-tr" data-trace="${key}" aria-hidden="true"></i></div>`).join('');
  const axis = `<p class="vg-h-axis" aria-hidden="true"><span class="vg-h-axbox">${win.ticks
    .map((t) => `<span data-at="${((1 - t / win.seconds) * 100).toFixed(2)}">-${t}S</span>`)
    .join('')}<span class="vg-h-now">NOW</span></span></p>`;
  const title = phone ? `${PANEL.title} · ${win.seconds} s` : `${PANEL.title} · live · ${win.seconds} s history`;
  return `<section class="vg-h-pn" data-mode="${mode}" data-win="${phone ? 'phone' : 'desk'}" aria-label="${PANEL.title}">`
    + `<h2 class="vg-h-title">${title}</h2>${feats}${mode === 'spread' ? '' : axis}`
    + `<p class="vg-h-foot"><span class="vg-h-lamp" data-lamp="over">${PANEL.over}</span>`
    + `<span class="vg-h-lamp" data-lamp="link">${PANEL.link}</span>`
    + '<span class="vg-h-rate"></span></p></section>';
}

/**
 * The three colours the panel's traces take from the scene's live palette `pair` (its two colours
 * and a third between them, as three bands' colours), and the ink of the rest. `base` is the
 * fixed part (`hair`, `gap`, `mid`, `ink`, `dim`).
 */
export function panelInk(pair, base) {
  const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const [a, b] = pair ?? [];
  const ok = (h) => typeof h === 'string' && /^#[0-9a-f]{6}$/i.test(h);
  if (!ok(a) || !ok(b)) return base;
  const ra = rgb(a);
  const rb = rgb(b);
  const mixed = ra.map((v, i) => (v + rb[i]) / 2);
  // Each accent is hsl(hue of the palette band, S * 95 %, 55 + 8 i %): the hue only, bright
  const hs = (c) => {
    const [r, g, bl] = c.map((v) => v / 255);
    const mx = Math.max(r, g, bl);
    const mn = Math.min(r, g, bl);
    const d = mx - mn;
    let h = 0;
    if (d > 0) {
      if (mx === r) h = ((g - bl) / d + 6) % 6;
      else if (mx === g) h = (bl - r) / d + 2;
      else h = (r - g) / d + 4;
    }
    const l = (mx + mn) / 2;
    return { h: Math.round(h * 60), s: d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1)) };
  };
  const hues = [hs(ra), hs(mixed), hs(rb)];
  // the palette's own saturation is not known here: a high one, 95 % of the usual saturation
  const sat = Math.round(Math.min(1, Math.max(0.75, ...hues.map((x) => x.s))) * 95);
  const [c0, c1, c2] = hues.map((x, i) => `hsl(${x.h},${sat}%,${55 + i * 8}%)`);
  return { ...base, c0, c1, c2, c3: base.ink, frame: b };
}

/** Where the history's tick marks stand, seconds before now. */
const HISTORY_TICKS_S = [15, 10, 5];

const clamp = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);

/** The colour of a feature: the three accents by family. */
const ACCENT = Object.freeze({
  bass: 0, mid: 1, high: 2, l: 3, r: 3, width: 1, loud: 0, flux: 2, cent: 1, flat: 2,
});

/** A bar with a peak mark. `ticked` draws five ticks under it. */
function bar(c, x, y, w, h, v, peak, ink, col, ticked = false) {
  c.fillStyle = ink.hair;
  c.fillRect(x, y, w, h);
  c.fillStyle = col;
  c.fillRect(x, y, Math.round(w * clamp(v)), h);
  c.fillStyle = ink.gap;
  for (let i = 1; i < 24; i++) c.fillRect(x + Math.round((w * i) / 24), y, 1, h);
  if (ticked) {
    c.fillStyle = ink.dim ?? ink.mid;
    for (let i = 0; i <= 4; i++) c.fillRect(x + Math.round(((w - 1) * i) / 4), y + h + 1, 1, i % 2 ? 2 : 3);
  }
  c.fillStyle = ink.ink;
  c.fillRect(x + Math.round((w - 2) * clamp(peak)), y - 2, 2, h + 4);
}

/** The history `series` (oldest first, 0 to 1) as a line across a box. */
function trace(c, x, y, w, h, series, col) {
  c.strokeStyle = col;
  c.lineWidth = 1;
  c.beginPath();
  const n = series.length;
  for (let i = 0; i < n; i++) {
    const px = x + (i / (n - 1)) * w;
    const py = y + h - clamp(series[i]) * h;
    if (i) c.lineTo(px, py);
    else c.moveTo(px, py);
  }
  c.stroke();
}

/** Ticks at -15, -10 and -5 s across a box. */
function ticks(c, x, y, w, h, seconds, ink, marks = HISTORY_TICKS_S) {
  c.fillStyle = ink.dim ?? ink.mid;
  for (const t of marks) c.fillRect(x + Math.round(w * (1 - t / seconds)), y, 1, h);
}

const FORMS_DRAW = {
  /** The Panel's row: a 100x5 bar of 24 segments with its peak hold and ticks, and the history beside it
   * (its last `d.window` seconds), with ticks at the axis' marks. */
  bars(c, b, d, key, col) {
    const L = PANEL_LAYOUT;
    bar(c, b.x, b.y + 2, L.bar, L.barH, d.values[key], d.peaks[key], d.ink, col, true);
    const tw = Math.max(8, b.w - L.trace);
    const tx = b.x + L.trace;
    const all = d.series[key];
    const keep = Math.max(2, Math.round((all.length * d.window) / d.seconds));
    const series = all.length > keep ? all.slice(all.length - keep) : all;
    ticks(c, tx, b.y + 9, tw, 2, d.window, d.ink, d.ticks);
    c.fillStyle = d.ink.hair;
    c.fillRect(tx, b.y + 9, tw, 1);
    trace(c, tx, b.y, tw, 9, series, col);
  },
  row(c, b, d, key, col) {
    if (d.compact) {
      bar(c, b.x, b.y + 1, b.w, 5, d.values[key], d.peaks[key], d.ink, col);
      return;
    }
    const barW = Math.round(b.w * 0.38);
    bar(c, b.x, b.y + 2, barW, 5, d.values[key], d.peaks[key], d.ink, col);
    ticks(c, b.x + barW + 8, b.y + b.h - 2, b.w - barW - 8, 2, d.seconds, d.ink);
    c.fillStyle = d.ink.hair;
    c.fillRect(b.x + barW + 8, b.y + b.h, b.w - barW - 8, 1);
    trace(c, b.x + barW + 8, b.y, b.w - barW - 8, b.h, d.series[key], col);
  },
  col(c, b, d, key, col) {
    const mh = b.h;
    const v = clamp(d.values[key]);
    c.fillStyle = d.ink.hair;
    c.fillRect(b.x, b.y, 8, mh);
    c.fillStyle = col;
    c.fillRect(b.x, b.y + mh - Math.round(mh * v), 8, Math.round(mh * v));
    c.fillStyle = d.ink.gap;
    for (let j = 1; j < 24; j++) c.fillRect(b.x, b.y + Math.round((mh * j) / 24), 8, 1);
    c.fillStyle = d.ink.ink;
    c.fillRect(b.x - 2, b.y + mh - Math.round(mh * clamp(d.peaks[key])) - 1, 12, 2);
    const s = d.series[key];
    c.strokeStyle = col;
    c.lineWidth = 1;
    c.beginPath();
    for (let j = 0; j < s.length; j++) {
      const vv = s[s.length - 1 - j];
      const px = b.x + 12 + clamp(vv) * 20;
      const py = b.y + (j / (s.length - 1)) * mh;
      if (j) c.lineTo(px, py);
      else c.moveTo(px, py);
    }
    c.stroke();
  },
  strip(c, b, d, key, col) {
    const s = d.series[key];
    c.fillStyle = d.ink.hair;
    c.fillRect(b.x, b.y + b.h, b.w, 1);
    const alpha = c.globalAlpha;
    c.globalAlpha = alpha * 0.35;
    c.fillStyle = col;
    c.beginPath();
    c.moveTo(b.x, b.y + b.h);
    for (let j = 0; j < s.length; j++) {
      c.lineTo(b.x + (j / (s.length - 1)) * b.w, b.y + b.h - clamp(s[j]) * b.h);
    }
    c.lineTo(b.x + b.w, b.y + b.h);
    c.fill();
    c.globalAlpha = alpha;
    trace(c, b.x, b.y, b.w, b.h, s, col);
  },
  dial(c, b, d, key, col) {
    const cx = b.x + b.w / 2;
    const cy = b.y + b.h / 2;
    const r = Math.min(b.w, b.h) / 2 - 8;
    const a1 = Math.PI * 0.75;
    const a2 = Math.PI * 2.25;
    const av = a1 + (a2 - a1) * clamp(d.values[key]);
    const ap = a1 + (a2 - a1) * clamp(d.peaks[key]);
    c.lineWidth = 3;
    c.strokeStyle = d.ink.hair;
    c.beginPath();
    c.arc(cx, cy, r, a1, a2);
    c.stroke();
    c.strokeStyle = col;
    c.beginPath();
    c.arc(cx, cy, r, a1, av);
    c.stroke();
    c.lineWidth = 1;
    c.strokeStyle = d.ink.mid;
    c.beginPath();
    for (let j = 0; j <= 8; j++) {
      const a = a1 + ((a2 - a1) * j) / 8;
      const r1 = r + (j % 4 ? 5 : 7);
      c.moveTo(cx + Math.cos(a) * (r + 3), cy + Math.sin(a) * (r + 3));
      c.lineTo(cx + Math.cos(a) * r1, cy + Math.sin(a) * r1);
    }
    c.stroke();
    c.strokeStyle = d.ink.ink;
    c.beginPath();
    c.moveTo(cx + Math.cos(ap) * (r - 5), cy + Math.sin(ap) * (r - 5));
    c.lineTo(cx + Math.cos(ap) * (r + 2), cy + Math.sin(ap) * (r + 2));
    c.stroke();
  },
};

/** 
 * Draw the traces of the panel `root` into `ctx` (the size of `root` in CSS pixels). `data`: `{
 * values, peaks, series, seconds, ink }`: the features' values (0 to 1), their peak marks, their
 * history (`Float32Array`s, oldest first), the history's length in seconds, `compact` (a bar and no
 * trace in a Panel row) and the colours (`ink`:
 * `hair`, `gap`, `mid`, `ink`, `c0`, `c1`, `c2`, `c3`). `opt.rect(el)` is an element's box.
 */
export function paintPanel(ctx, root, data, { rect = (el) => el.getBoundingClientRect() } = {}) {
  const base = rect(root);
  ctx.clearRect(0, 0, base.width, base.height);
  for (const el of root.querySelectorAll('.vg-h-tr')) {
    if (el.getClientRects?.().length === 0) continue;
    const key = el.dataset.trace;
    const r = rect(el);
    const box = { x: r.left - base.left, y: r.top - base.top, w: r.width, h: r.height };
    const form = el.closest('[data-form]').dataset.form;
    FORMS_DRAW[form](ctx, box, data, key, data.ink[`c${ACCENT[key]}`]);
  }
  ctx.globalAlpha = 1;
}
