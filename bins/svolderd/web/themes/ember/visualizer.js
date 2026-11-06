/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The visualizer card's canvas layer: the daemon's analysis frames (`GET api/v1/host/frames`)
// drawn as a spectrogram, as lumiBars or as lumiBars soft, with the two channels' curves and
// their max hold over any of them. The canvas has no ground of its own: silence is transparent,
// so the card's translucent background and the smoke behind it show. The rules (decoding,
// smoothing, the hold, the clock, the beats) are in viz-model.js and the colours of the
// spectrogram in viz-colour.js; this file only draws them and wires the card: a tap on the card
// cycles the look (a chip names it for a second), the corner toggle shows and hides the graph,
// `v` cycles (keys.js), both choices are kept in this browser's storage, and a first-time hint
// says to tap.
//
// The card costs nothing when nobody looks. The frames stream (the core's analysis) is held only
// while the card is on show (visible.js: the tab in front and the card inside the viewport, with a
// grace so a quick scroll does not close it) and the animation frames run only while it is on show
// and the capture runs. The card's box is fixed by the style sheet, so nothing moves when it
// starts, when the capture is not possible (it says so in a quiet line) or when the picture
// changes.
//
// The card can be switched off (the effects menu's Visualizer row, `setOn`): it then leaves the
// page altogether, so nothing draws and no frames stream is open, the Now playing card has the
// band to itself and the daemon holds no capture for the page. The choice is this browser's,
// kept with the others (`on`); viz-pref.js, a classic script in the page's head, tells the style
// sheet before the first paint so a reload does not show the card for a moment.
//
// On a phone the card stands under the volume (or the meters) and the style sheet orders it so, but
// the card is drawn in Now playing's band, which is before the volume in the page's order: a
// keyboard or a screen reader would meet the card first. `place` moves the card in the page's order
// to where it shows (after the volume on a phone, into the band on a desktop), as the page is
// resized (the window's `resize`, which comes before the layout that follows); nothing moves on
// screen. A column that is locked is inert, and the card stays out of it. A screen in an order the
// person arranged (`.arranged`) is left alone: its sections stand where they put them.
//
// The card's markup is drawn from the model (draw/parts.js); the canvas is drawn here.
// The spectrogram lives in a canvas of its own that scrolls a column at a time; the visible one
// is drawn from it, so the graph over it is redrawn without touching the history. A screen swap
// brings a new card element: the stream, the model and the history belong to this module, not
// to the element, so the picture carries on in the new one.
//
// The effects layer hears the same frames: each draw hands it the loudness (the glow adds it to
// the volume's level) and each beat that is due (the smoke puffs); when the drawing stops, the
// effects are told the music has, and the page is as it was without frames.

import { BANDS, VizModel, posFreq } from '../../core/viz-model.js';
import { CURVE_FILL_ALPHA, CURVE_LEFT, CURVE_RIGHT, alpha, pixel } from './viz-colour.js';
import { watchVisible } from '../../core/visible.js';
import { ember, mine, orderOf } from './bind.js';
import { seat } from './sections.js';
import { setVizDelay } from './facts.js';
import * as effects from './effects.js';

/** The storage key the choices are kept under: `{ look?, graph?, on?, hinted }`. `look`, `graph`
 * and `on` are there only once the viewer has chosen them, so a default that changes later reaches
 * a browser that never chose. The number is the layout's version; an earlier key is never read.
 * viz-pref.js reads `on` under the same key. */
export const KEY = 'svolder.viz.2';

/** The looks a tap cycles, in order: the id (what the card's `data-look` says) and the name the
 * chip shows. */
export const LOOKS = Object.freeze([
  Object.freeze({ id: 'spectrogram', name: 'Spectrogram' }),
  Object.freeze({ id: 'lumibars', name: 'lumiBars' }),
  Object.freeze({ id: 'lumibars-soft', name: 'lumiBars soft' }),
]);

/** The look a browser with nothing kept starts on (an index into `LOOKS`): lumiBars soft. A tap
 * still goes round in `LOOKS`' order, so from it the next look is the spectrogram. */
const DEFAULT_LOOK = 2;

/** What the first-time chip says. */
export const HINT = 'tap to change the look';

/** How long a look's name stays on the chip, and the hint, in milliseconds. */
const CHIP_MS = 1000;
const HINT_MS = 3500;
/** How long a waterfall takes to cross the card, in milliseconds. */
const CROSS_MS = 12000;
/** The longest gap between two draws that moves the waterfall as one step, in milliseconds. */
const MAX_STEP_MS = 250;
/** The graph's curves reach this share of the card's height at full scale. */
const CURVE_SHARE = 0.85;
/** The bars take this share of their slot (the rest is the gap). */
const BAR_SHARE = 0.6;
/** The bars' brightness is the level to this power: a quiet band is dark, a loud one lit. */
const BAR_POWER = 2.2;
const SOFT_POWER = 2.0;
/** The soft look fades to the background over this share of the height at the top and foot. */
const SOFT_EDGE = 0.3;
/** The most the canvas is scaled up for a sharp screen. */
const MAX_DPR = 3;
/** The delay row is written this often, in milliseconds. */
const DELAY_EVERY_MS = 1000;

/** The prism the left channel's bars are drawn in, and the rainbow of the right's, low to high
 * frequency along the card. */
const PRISM = ['#ff2d55', '#ff6a2a', '#ffcc33', '#7ddc3c', '#22c7c7', '#2b8cff', '#7a5cff'];
const RAINBOW = ['#ff2a4f', '#ff8a1f', '#ffd21f', '#4ee06c', '#22d3ee', '#4f7cff', '#a855f7'];

const rgba = (c, a) => `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${a})`;
const clock = () => performance.timeOrigin + performance.now();

/** What the viewer chose, as kept: `look` (an id) and `graph` only once they changed them. */
let chosen = {};
/** The choices as shown; `look` is an index into `LOOKS`. */
let prefs = readPrefs();

/** What `localStorage` holds, or the defaults: the card on, lumiBars soft with the graph off, no
 * hint seen. A look, a graph or the switch is taken only when the viewer chose it. */
function readPrefs() {
  const fresh = { look: DEFAULT_LOOK, graph: false, on: true, hinted: false };
  chosen = {};
  try {
    const kept = JSON.parse(window.localStorage.getItem(KEY));
    if (!kept || typeof kept !== 'object') return fresh;
    const at = LOOKS.findIndex((look) => look.id === kept.look);
    if (at >= 0) chosen.look = kept.look;
    if (typeof kept.graph === 'boolean') chosen.graph = kept.graph;
    if (typeof kept.on === 'boolean') chosen.on = kept.on;
    return {
      look: at < 0 ? DEFAULT_LOOK : at,
      graph: chosen.graph === true,
      on: chosen.on !== false,
      hinted: kept.hinted === true,
    };
  } catch {
    return fresh;
  }
}

/** Keep the viewer's choices and whether the hint was shown, where the browser lets it; the page
 * works without. */
function keepPrefs() {
  try {
    window.localStorage.setItem(KEY, JSON.stringify({ ...chosen, hinted: prefs.hinted }));
  } catch {
    // The choice lasts until the page is closed.
  }
}

// What the card shows, by the module: the model, the history canvas, the stream and its state.
const model = new VizModel();
/** The spectrogram's history: a canvas that is not on the page, `hist.width` columns across. */
let hist = null;
let histCtx = null;
/** The one-column image the waterfall's new columns are made in, and the band lookup for it. */
let column = null;
let rows = null;
/** The elapsed part of a column the waterfall has not yet moved by, in columns. */
let carry = 0;
/** When the waterfall was last advanced, ms on the page clock; null when it was not running. */
let lastAdvance = null;

/** The card's parts as the page holds them now, or null when the screen has no card. */
let ui = null;
/** The card taken out of the screen while the visualizer is off, to put back when it is on, and
 * the screen's view it was taken from (a card of a screen that is gone is not put back). */
let stashed = null;
let stashedView = null;
let watch = null;
/** The hold on the core's frames stream, while the card is on show. */
let holder = null;
let loop = 0;
let chipTimer = 0;
let lastDelay = -Infinity;
let resizer = null;
/** What the capture says: `starting`, `running` or `unavailable`, the sink it reads and why it
 * cannot; null before the stream said anything. */
let capture = null;

/** Whether the animation frames should run: the card is on show and the capture runs. */
const drawing = () => ui !== null && watch?.visible === true && capture?.state === 'running';

/** The card's own element for a part, or null. */
function parts(root) {
  const q = (selector) => root.querySelector(selector);
  const face = q('.viz-face');
  const canvas = q('canvas');
  if (!face || !canvas) return null;
  return {
    root, face, canvas, ctx: canvas.getContext('2d'), chip: q('.viz-chip'),
    toggle: q('.viz-graph'), note: q('.viz-note'), sink: q('.viz-sink'),
  };
}

// ------------------------------------------------------------------ the card's look

/** Write the choices on the card: its look and graph, the toggle's state and the face's name. */
function applyPrefs() {
  const { root, toggle, face } = ui;
  const look = LOOKS[prefs.look];
  root.dataset.look = look.id;
  root.dataset.graph = prefs.graph ? 'on' : 'off';
  toggle.setAttribute('aria-pressed', String(prefs.graph));
  face.setAttribute('aria-label', `Visualizer, ${look.name}. Press to change the look`);
}

/** Whether Now playing says the sound goes to a sink that is not this page's DAC: then the card
 * names the sink (the server writes the mark on the Now playing part). */
const labelled = () => document.getElementById('part-playing')?.hasAttribute('data-sink-label');

/** Write the capture's state on the card: the state, the quiet line when there is no capture, and
 * the sink's name when the sound is not this page's DAC's. */
function render() {
  if (!ui) return;
  const { root, note, sink } = ui;
  root.dataset.state = capture?.state ?? 'idle';
  const unavailable = capture?.state === 'unavailable';
  note.hidden = !unavailable;
  note.textContent = unavailable
    ? `Visualizer unavailable: ${capture.why || 'nothing to capture'}` : '';
  const named = capture?.state === 'running' && capture.sink && labelled();
  sink.hidden = !named;
  sink.textContent = named ? `Playing to ${capture.sink}` : '';
}

/** Show `text` on the chip for `ms`. */
function showChip(text, ms) {
  if (!ui) return;
  clearTimeout(chipTimer);
  ui.chip.textContent = text;
  ui.chip.classList.add('show');
  chipTimer = setTimeout(() => ui?.chip.classList.remove('show'), ms);
}

/** The first time the card runs in a browser it says how to change the look. */
function hint() {
  if (prefs.hinted) return;
  prefs.hinted = true;
  keepPrefs();
  showChip(HINT, HINT_MS);
}

/** The next look, named on the chip; false when the screen has no card. Called by a tap and by
 * the `v` key. */
export function cycleLook() {
  if (!ui) return false;
  prefs.look = (prefs.look + 1) % LOOKS.length;
  chosen.look = LOOKS[prefs.look].id;
  keepPrefs();
  applyPrefs();
  showChip(LOOKS[prefs.look].name, CHIP_MS);
  paint();
  return true;
}

/** Show or hide the graph over the looks. */
function setGraph(on) {
  prefs.graph = on;
  chosen.graph = on;
  keepPrefs();
  applyPrefs();
  paint();
}

// ------------------------------------------------------------------ the canvases

/** The spectrogram column for each row of a card `h` rows tall: the band below the row's
 * frequency and how far up to the next, row 0 the highest frequency. The bands are 1/24 octave,
 * so a row between two centres takes a mix of them. */
function bandRows(h) {
  const low = new Uint8Array(h);
  const part = new Float32Array(h);
  for (let y = 0; y < h; y++) {
    const at = Math.log2(posFreq(h - (y + 0.5), h) / 20) * 24 - 0.5;
    const clamped = Math.min(BANDS - 1, Math.max(0, at));
    low[y] = Math.min(BANDS - 2, Math.floor(clamped));
    part[y] = Math.min(1, clamped - low[y]);
  }
  return { low, part };
}

/** A canvas `w` by `h` that is not on the page and transparent, as if silence had been playing
 * for its whole width (silence has no colour, so no white first frames). */
function blankHistory(w, h) {
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  return { canvas, ctx: canvas.getContext('2d') };
}

/** Size the visible canvas to its box at the screen's pixel ratio, and the history with it (what
 * it held is stretched to the new size). False when the box has no size (the card is hidden). */
function fit() {
  const { face, canvas } = ui;
  const ratio = Math.min(MAX_DPR, Math.max(1, window.devicePixelRatio || 1));
  const w = Math.round(face.clientWidth * ratio);
  const h = Math.round(face.clientHeight * ratio);
  if (w === 0 || h === 0) return false;
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  if (!hist || hist.width !== w || hist.height !== h) {
    const next = blankHistory(w, h);
    if (hist) next.ctx.drawImage(hist, 0, 0, w, h);
    hist = next.canvas;
    histCtx = next.ctx;
    column = histCtx.createImageData(1, h);
    rows = bandRows(h);
    carry = 0;
  }
  return true;
}

/** Write the spectrogram's newest column from the smoothed bands. */
function newColumn() {
  const { low, part } = rows;
  const data = column.data;
  const { left, right } = model;
  for (let y = 0; y < low.length; y++) {
    const i = low[y];
    const f = part[y];
    const l = left[i] + (left[i + 1] - left[i]) * f;
    const r = right[i] + (right[i + 1] - right[i]) * f;
    const [red, green, blue] = pixel(l, r);
    data[4 * y] = red;
    data[4 * y + 1] = green;
    data[4 * y + 2] = blue;
    data[4 * y + 3] = Math.round(255 * alpha(l, r));
  }
}

/** Move the waterfall one step: by the time since the last, a card's width in `CROSS_MS`, the
 * history drawn onto itself shifted left and the new columns put at its right edge. */
function advance(now) {
  if (!hist) return;
  const dt = lastAdvance === null ? 0 : Math.min(MAX_STEP_MS, now - lastAdvance);
  lastAdvance = now;
  carry += (dt * hist.width) / CROSS_MS;
  const n = Math.min(Math.floor(carry), hist.width);
  if (n <= 0) return;
  carry -= n;
  // `copy`, not the default: the history is semi-transparent, and laid over itself it would
  // thicken at every step.
  histCtx.globalCompositeOperation = 'copy';
  histCtx.drawImage(hist, -n, 0);
  histCtx.globalCompositeOperation = 'source-over';
  newColumn();
  for (let j = 0; j < n; j++) histCtx.putImageData(column, hist.width - n + j, 0);
}

/** The bars of one channel: 240 bars across, `BAR_SHARE` of their slot wide, as bright as the
 * band's level, on a gradient along the frequency axis. */
function bars(ctx, levels, stops, w, h, power) {
  const gradient = ctx.createLinearGradient(0, 0, w, 0);
  stops.forEach((colour, i) => gradient.addColorStop(i / (stops.length - 1), colour));
  ctx.fillStyle = gradient;
  const slot = w / BANDS;
  const width = Math.max(0.5, slot * BAR_SHARE);
  for (let i = 0; i < BANDS; i++) {
    const opacity = levels[i] ** power;
    if (opacity < 0.004) continue;
    ctx.globalAlpha = opacity;
    ctx.fillRect(i * slot, 0, width, h);
  }
  ctx.globalAlpha = 1;
}

/** lumiBars: left in the prism and right in the rainbow, added, on a transparent ground; the soft
 * look fades out toward the top and the foot (erasing, so the card shows through). */
function lumi(ctx, w, h, soft) {
  const power = soft ? SOFT_POWER : BAR_POWER;
  ctx.globalCompositeOperation = 'lighter';
  bars(ctx, model.left, PRISM, w, h, power);
  bars(ctx, model.right, RAINBOW, w, h, power);
  ctx.globalCompositeOperation = 'source-over';
  if (!soft) return;
  const edge = h * SOFT_EDGE;
  ctx.globalCompositeOperation = 'destination-out';
  const top = ctx.createLinearGradient(0, 0, 0, edge);
  top.addColorStop(0, 'rgba(0, 0, 0, 1)');
  top.addColorStop(1, 'rgba(0, 0, 0, 0)');
  ctx.fillStyle = top;
  ctx.fillRect(0, 0, w, edge);
  const foot = ctx.createLinearGradient(0, h - edge, 0, h);
  foot.addColorStop(0, 'rgba(0, 0, 0, 0)');
  foot.addColorStop(1, 'rgba(0, 0, 0, 1)');
  ctx.fillStyle = foot;
  ctx.fillRect(0, h - edge, w, edge);
  ctx.globalCompositeOperation = 'source-over';
}

/** A channel's curve over bands `levels`, as a path across the card; `close` shuts it to the foot
 * for the fill. */
function curve(ctx, levels, w, h, close) {
  const slot = w / BANDS;
  const reach = h * CURVE_SHARE;
  ctx.beginPath();
  if (close) ctx.moveTo(0, h);
  for (let i = 0; i < BANDS; i++) {
    const x = (i + 0.5) * slot;
    const y = h - Math.min(1, Math.max(0, levels[i])) * reach;
    if (i === 0 && !close) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  if (close) {
    ctx.lineTo(w, h);
    ctx.closePath();
  }
}

/** The graph: each channel's smoothed curve over its own faint fill (plain alpha, so the two
 * never add up to white) and its max hold, dotted. */
function graph(ctx, w, h) {
  const ratio = w / ui.face.clientWidth;
  const channels = [
    [CURVE_LEFT, model.left, model.holdLeft],
    [CURVE_RIGHT, model.right, model.holdRight],
  ];
  for (const [colour, levels, held] of channels) {
    ctx.fillStyle = rgba(colour, CURVE_FILL_ALPHA);
    curve(ctx, levels, w, h, true);
    ctx.fill();
    ctx.strokeStyle = rgba(colour, 1);
    ctx.lineWidth = 1.5 * ratio;
    curve(ctx, levels, w, h, false);
    ctx.stroke();
    ctx.strokeStyle = rgba(colour, 0.7);
    ctx.lineWidth = ratio;
    ctx.setLineDash([2 * ratio, 3 * ratio]);
    curve(ctx, held, w, h, false);
    ctx.stroke();
    ctx.setLineDash([]);
  }
}

/** Draw the card as it is now: the look, then the graph when it is on, on a cleared canvas. */
function paint() {
  if (!ui || !hist || ui.canvas.width !== hist.width) return;
  const { ctx, canvas } = ui;
  const { width: w, height: h } = canvas;
  ctx.clearRect(0, 0, w, h);
  if (LOOKS[prefs.look].id === 'spectrogram') ctx.drawImage(hist, 0, 0);
  else lumi(ctx, w, h, LOOKS[prefs.look].id === 'lumibars-soft');
  if (prefs.graph) graph(ctx, w, h);
}

// ------------------------------------------------------------------ the animation

/** One animation frame: take the newest frame waiting, move the waterfall, draw. */
function animate() {
  loop = 0;
  if (!drawing()) return;
  const now = clock();
  model.step(now);
  effects.music(effects.loudness(model.levels));
  if (model.beatDue(now)) effects.beat();
  advance(now);
  paint();
  if (now - lastDelay >= DELAY_EVERY_MS) {
    lastDelay = now;
    setVizDelay(model.delayMs(now));
  }
  loop = requestAnimationFrame(animate);
}

/** Run the animation frames if they should run and are not. */
function start() {
  if (loop || !drawing()) return;
  lastAdvance = null;
  lastDelay = -Infinity;
  loop = requestAnimationFrame(animate);
}

/** Stop the animation frames; the delay row goes, as the card is not running. */
function stop() {
  if (loop) cancelAnimationFrame(loop);
  loop = 0;
  lastAdvance = null;
  setVizDelay(null);
  effects.music(null);
}

// ------------------------------------------------------------------ the stream

/** The capture's state as the stream said it (the `hello` and the `capture` events carry the
 * same fields): note it, show it, and run or rest the animation. */
function follow(data) {
  const state = ['starting', 'running', 'unavailable'].includes(data.capture)
    ? data.capture : 'unavailable';
  capture = { state, sink: data.sink ?? null, why: data.why ?? null };
  render();
  if (state === 'running') {
    if (watch?.visible) hint();
    start();
  } else {
    stop();
    model.clear();
    paint();
  }
}

/** Let go of the frames stream. */
function closeStream() {
  holder?.close();
  holder = null;
}

/** Hold the frames stream, in place of any held. The `hello` gives the daemon's clock, which the
 * model takes its offset from (taken again on each stream), then `capture` events follow the
 * capture and `frame` events carry the bands. The core opens the stream while anything holds it
 * and asks for it again, after a wait that doubles, when the browser gave up on it. */
function openStream() {
  closeStream();
  holder = ember.core.analysis.open({
    hello: (data) => model.hello(data.now_ms, clock()),
    capture: follow,
    frame: (text) => model.push(text, clock()),
  });
}

// ------------------------------------------------------------------ the page

/** Put the card where the page's order matches what shows: after the volume (or the meters) when
 * the style sheet dissolves the band into the screen's column (a phone), in the band otherwise.
 * The style sheet's own `display` on the band says which: no width is guessed here. */
function place(root) {
  const view = root.closest('.view');
  const band = view?.querySelector('.p-band');
  if (!band || view.classList.contains('arranged')) return;
  const anchor = view.querySelector('#part-volume, #part-meters');
  const phone = getComputedStyle(band).display === 'contents';
  if (phone && anchor && !anchor.closest('[inert]')) {
    if (anchor.nextElementSibling !== root) anchor.after(root);
  } else if (root.parentElement !== band) {
    band.append(root);
  }
}

/** The page's order is set again (an arrangement ended): the card stands where the window's width
 * puts it. */
export function settle() {
  if (ui) place(ui.root);
}

/** Take the card in the screen as the one to draw. */
function bind(root) {
  const next = parts(root);
  if (!next) return release();
  ui = next;
  place(root);
  applyPrefs();
  render();
  if (fit()) paint();
  resizer?.disconnect();
  if ('ResizeObserver' in window) {
    // An observer that cannot be made leaves the card at the size it was bound with.
    try {
      resizer = new ResizeObserver(() => {
        if (ui && fit()) paint();
      });
      resizer.observe(ui.face);
    } catch {
      resizer = null;
    }
  }
  if (watch) watch.observe(root);
  else {
    watch = watchVisible({
      element: root,
      onVisible(shown) {
        if (shown) start();
        else stop();
      },
      onShow: openStream,
      onHide() {
        // What the capture was is not known any more: nothing is drawn until the next stream
        // speaks, so the picture does not run on from a stream that has closed.
        closeStream();
        stop();
        capture = null;
        render();
      },
    });
  }
  start();
}

/** There is no card on the screen: nothing runs. */
function release() {
  watch?.stop();
  watch = null;
  closeStream();
  stop();
  resizer?.disconnect();
  resizer = null;
  ui = null;
  capture = null;
  model.clear();
}

/** Whether the visualizer is on (the card shows wherever a screen has one). */
export const isOn = () => prefs.on;

/** Whether the screen has a card, on show or taken out while the visualizer is off: it has
 * something for the switch to switch. */
export const hasBand = () => document.querySelector('#screen .p-viz') !== null
  || (stashed !== null && stashedView?.isConnected === true);

/** The screen is in place (first paint, a switch, a redrawn screen): draw its card, if it has
 * one, in the place of the last. With the visualizer off the card is taken out of the screen
 * (and kept, to put back) and nothing runs. */
export function syncVisualizer() {
  let root = document.querySelector('#screen .p-viz');
  if (!prefs.on) {
    document.documentElement.dataset.viz = 'off';
    if (root) {
      stashed = root;
      stashedView = root.closest('.view');
      root.remove();
    }
    release();
    return;
  }
  delete document.documentElement.dataset.viz;
  const view = document.querySelector('#screen .view');
  if (!root && stashed && view && stashedView === view) {
    root = stashed;
    seat(view, root, 'visualizer', orderOf(ember.core?.model.page().device));
  }
  stashed = null;
  stashedView = null;
  if (root) bind(root);
  else release();
}

/** Switch the visualizer on or off, as the menu's row asks: kept in this browser's storage, and
 * the screen's card put back or taken away at once. The page moves here, once, as the person's
 * own act; nothing else moves it. */
export function setOn(on) {
  if (on === prefs.on) return;
  prefs.on = on;
  chosen.on = on;
  keepPrefs();
  syncVisualizer();
}

/** Listen for a tap on the card, on the corner toggle, for the window's size (the card's place in
 * the page's order) and for the Now playing part that says whether the sink is named. */
export function installVisualizer() {
  window.addEventListener('resize', () => {
    if (ui) place(ui.root);
  }, mine({ passive: true }));
  document.addEventListener('click', (event) => {
    if (event.target.closest?.('.p-viz .viz-graph')) setGraph(!prefs.graph);
    else if (event.target.closest?.('.p-viz .viz-face')) cycleLook();
  }, mine());
  document.body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id === 'part-playing') render();
  }, mine());
  // The card goes with the theme: nothing runs and no frames stream is held.
  ember.signal?.addEventListener('abort', release);
}
