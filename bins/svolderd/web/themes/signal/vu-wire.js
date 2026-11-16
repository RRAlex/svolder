/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands and the clock of the computer's level meters (draw/vu.js draws them): the glow a tap on a
// dial face turns (W1, W2, W3), the reference under the dials, the Meters | Spectrum chips, and what
// holds the streams and moves the needles.
//
// What is held, and when (the core's analysis streams are shared and open while anything holds them):
//   Meters view     the levels-only stream (`core.levels`: 4 bytes a frame) and one animation loop that
//                   puts the two needles, the two readings and the two lamps in place from the core's
//                   `VuModel` (the rules of the needle are the model's, on the page's clock). The loop runs
//                   while a needle moves or a lamp is lit; a new levels frame, a reference or view change
//                   starts it again;
//   Spectrum view   the full stream (`core.analysis`) and a canvas loop of the smoothed bands; the
//                   levels stream and the needles' loop are let go.
// Nothing is held while the meters are not on the page (the DAC's side, the page hidden for more than
// the grace of `watchVisible`, the section scrolled away). The capture's states other than running
// bring the needles to rest and say why under the dials.
//
// The section is drawn by live.js with the rest of the open area; the needles are then put back by
// `inPlace` (live.js's `view.openDrawn`), never by a draw per frame. The choices (glow, view,
// reference) are this browser's: `core.prefs` `vu-glow`, `vu-view`, `vu-ref`, each read guarded.

import { VizModel } from '../../core/viz-model.js';
import {
  REF_DEFAULT, REF_MAX, REF_MIN, VuModel, decodeLevels,
} from '../../core/vu-model.js';
import { GLOWS, needleAngle } from './draw/vu.js';
import { drawSpectrum, fitCanvas } from './draw/spectrum.js';
import { VU, vuText } from './words.js';

/** The least time between two changes of a reading's text, in ms (10 Hz). */
const READING_MS = 100;

const clampRef = (n) => Math.min(REF_MAX, Math.max(REF_MIN, Math.round(n)));
/** A reference read from storage: a whole number, as the terminal's setting wants. */
const isRef = (v) => Number.isInteger(v);
/** The channels, in the order of the slots of the loop and of the model's `L`, `R`. */
const CHANNELS = ['L', 'R'];
const clock = () => globalThis.performance.now();

/** The four level bytes of a stream event's text (`{ t, lv }`, or a whole frame), or null. */
function levelsOf(text) {
  return decodeLevels(text)?.levels ?? null;
}

/**
 * Wire the level meters of the page in `root` until `signal` aborts. Returns `{ state() }`, what the
 * section is drawn from; `view.openDrawn` is set to put a section just drawn in place (and to hold or
 * let go of the streams to match).
 */
export function wireVu({
  root, core, view, signal,
}) {
  const box = root.querySelector('#sg-open');
  const doc = root.ownerDocument;
  const win = doc.defaultView;

  // ------------------------------------------------------------------------------ the choices

  const pick = (name, ok, fallback) => {
    const value = core.prefs?.get(name, fallback);
    return ok(value) ? value : fallback;
  };
  let glow = pick('vu-glow', (v) => GLOWS.includes(v), 'w3');
  let viewName = pick('vu-view', (v) => v === 'meters' || v === 'spectrum', 'meters');
  let ref = clampRef(pick('vu-ref', isRef, REF_DEFAULT));
  const keep = (name, value) => core.prefs?.set(name, value);

  /** The capture's state as the streams last said it: `{ state, why }` or null before any. */
  let capture = null;
  const noteOf = () => {
    if (!capture || capture.state === 'running') return '';
    return capture.state === 'starting' ? VU.capture.starting : VU.capture.unavailable(capture.why);
  };

  const vu = new VuModel({ ref });
  const viz = new VizModel();
  const reduced = win?.matchMedia?.('(prefers-reduced-motion: reduce)');

  // ------------------------------------------------------------------------- the part in place

  const area = () => box.querySelector('#sg-cvu');
  /** The elements the loop writes to (one slot a channel, with what was last written to them, so a frame
   * writes only what changed), found again after each draw of the section. */
  let els = null;
  let readingAt = -Infinity;

  function find() {
    const a = area();
    els = a ? CHANNELS.map((c) => {
      const lamp = a.querySelector(`[data-lamp="${c}"]`);
      return {
        needle: a.querySelector(`.sg-vneedle[data-needle="${c}"]`),
        lamp,
        word: lamp.querySelector('.sg-sr'),
        num: a.querySelector(`[data-read="${c}"]`),
        deg: null,
        on: null,
        words: null,
      };
    }) : null;
    readingAt = -Infinity;
  }

  /** The needles, readings and lamps from the model at `now`; the readings' text too when `text`
   * (or when the last change of one is `READING_MS` old). */
  function frame(now, text = false) {
    if (!els) return;
    const s = reduced?.matches === true ? vu.settled(now) : vu.step(now);
    const words = text || now - readingAt >= READING_MS;
    if (words) readingAt = now;
    for (let i = 0; i < 2; i++) {
      const one = i === 0 ? s.L : s.R;
      const e = els[i];
      const deg = Math.round(needleAngle(one.f) * 100);
      if (e.deg !== deg) {
        e.needle.style.transform = `rotate(${(deg / 100).toFixed(2)}deg)`;
        e.deg = deg;
      }
      if (e.on !== one.lamp) {
        e.lamp.classList.toggle('on', one.lamp);
        e.word.textContent = VU.peakState(one.lamp);
        e.on = one.lamp;
      }
      if (words) {
        const said = vuText(one.db);
        if (e.words !== said) {
          e.num.textContent = said;
          e.words = said;
        }
      }
    }
  }

  const showNote = () => {
    const note = area()?.querySelector('#sg-vstate');
    if (note && note.textContent !== noteOf()) note.textContent = noteOf();
  };

  // ------------------------------------------------------------------------------ the streams

  let levelsHold = null;
  let fullHold = null;
  let raf = 0;
  /** The two RMS bytes of the last levels frame, so a frame that says the same wakes nothing. */
  const heard = [-1, -1];
  let specLoop = null;
  let specCanvas = null;
  let watch = null;

  /** The capture's state, from either stream. */
  function onCapture(data) {
    const state = ['starting', 'running', 'unavailable'].includes(data.capture) ? data.capture : 'unavailable';
    capture = { state, why: data.why ?? null };
    if (state !== 'running') {
      vu.clear();
      viz.clear();
      heard.fill(-1);
    }
    showNote();
    wake();
  }

  const shown = () => area() !== null;

  /** Whether the needles are on show to be moved. */
  const onShow = () => shown() && viewName === 'meters' && watch?.visible === true;

  /** The model at rest (no needle moving, no lamp lit): nothing more to draw until a frame, a reference
   * or a view changes it. */
  const atRest = (now) => vu.resting(now, reduced?.matches === true);

  /** Run the loop if the needles are on show; a tick that finds them at rest ends it. */
  function wake() {
    if (onShow()) {
      if (!raf) raf = win.requestAnimationFrame(tick);
    } else if (raf) {
      win.cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  function tick() {
    raf = 0;
    const now = clock();
    frame(now);
    if (!onShow()) return;
    if (atRest(now)) frame(now, true);
    else raf = win.requestAnimationFrame(tick);
  }

  function stopSpectrum() {
    specLoop?.stop();
    specLoop = null;
    specCanvas = null;
  }

  function startSpectrum() {
    const canvas = area()?.querySelector('#sg-vspec');
    if (!canvas) return;
    if (specCanvas === canvas && specLoop) return;
    stopSpectrum();
    specCanvas = canvas;
    let ctx = null;
    let colour = '';
    let size = { width: 0, height: 0 };
    specLoop = core.lifecycle.canvasLoop({
      canvas,
      draw: () => {
        viz.step(clock());
        if (ctx) drawSpectrum(ctx, size.width, size.height, viz, colour);
      },
      resize: (width, height, dpr) => {
        size = { width, height };
        colour = win.getComputedStyle(canvas).color;
        ctx = fitCanvas(canvas, width, height, dpr);
      },
      watchVisible: core.lifecycle.watchVisible,
      matchMedia: (query) => win.matchMedia(query),
      // under reduced motion the line is still drawn from each reading, a few times a second
      reduced: 'theme',
      pace: () => (reduced?.matches === true ? 250 : null),
    });
    specLoop.start();
  }

  /** Hold what the section on show needs and let go of the rest. */
  function sync() {
    const held = shown() && watch?.active === true;
    const wantLevels = held && viewName === 'meters';
    const wantFull = held && viewName === 'spectrum';
    if (wantLevels && !levelsHold) {
      levelsHold = core.levels.open({
        capture: onCapture,
        frame: (text) => {
          const lv = levelsOf(text);
          if (!lv) return;
          const now = clock();
          vu.push(lv, now);
          const moved = lv[0] !== heard[0] || lv[1] !== heard[1];
          heard[0] = lv[0];
          heard[1] = lv[1];
          if (moved || !atRest(now)) wake();
        },
      });
    } else if (!wantLevels && levelsHold) {
      levelsHold.close();
      levelsHold = null;
      vu.clear();
      heard.fill(-1);
      capture = fullHold ? capture : null;
    }
    if (wantFull && !fullHold) {
      fullHold = core.analysis.open({
        hello: (data) => viz.hello(data.now_ms, clock()),
        capture: onCapture,
        frame: (text) => viz.push(text, clock()),
      });
    } else if (!wantFull && fullHold) {
      fullHold.close();
      fullHold = null;
      viz.clear();
      capture = levelsHold ? capture : null;
    }
    wake();
    if (shown() && viewName === 'spectrum') startSpectrum();
    else stopSpectrum();
  }

  /** Whether the section is on show: the tab is in front and the section is in the window; the streams
   * outlast it by the grace. */
  watch = core.lifecycle.watchVisible({
    onVisible: sync, onShow: sync, onHide: sync,
  });

  /** A section was drawn (live.js): the needles go where the model has them, the streams follow. */
  function inPlace() {
    find();
    watch.observe(area());
    sync();
    if (els) {
      showNote();
      frame(clock());
    }
  }

  // ---------------------------------------------------------------------------------- the hands

  /** Show the choices in the section as they stand, without drawing it again. */
  function paint() {
    const a = area();
    if (!a) return;
    for (const name of GLOWS) a.classList.toggle(`sg-${name}`, name === glow);
    for (const face of a.querySelectorAll('.sg-vface')) face.setAttribute('aria-label', VU.glowWords(glow));
    a.dataset.view = viewName;
    for (const chip of a.querySelectorAll('[data-act="vu-view"]')) {
      chip.setAttribute('aria-pressed', String(chip.dataset.view === viewName));
    }
    for (const pane of a.querySelectorAll('.sg-vpane')) pane.hidden = pane.dataset.pane !== viewName;
    const out = a.querySelector('#sg-vref-out');
    if (out) out.textContent = VU.refWords(ref);
    const toggle = (id, off) => {
      const b = a.querySelector(id);
      if (!b) return;
      if (off) b.setAttribute('aria-disabled', 'true');
      else b.removeAttribute('aria-disabled');
    };
    toggle('#sg-vref-down', ref <= REF_MIN);
    toggle('#sg-vref-up', ref >= REF_MAX);
    const reset = a.querySelector('#sg-vref-reset');
    if (reset) reset.hidden = ref === REF_DEFAULT;
  }

  function setRef(next) {
    const value = clampRef(next);
    if (value === ref) return;
    ref = value;
    vu.setRef(ref);
    keep('vu-ref', ref);
    const hadFocus = doc.activeElement?.id === 'sg-vref-reset';
    paint();
    if (hadFocus && ref === REF_DEFAULT) area()?.querySelector('#sg-vref-up')?.focus({ preventScroll: true });
    frame(clock(), true);
    wake();
  }

  root.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el || !box.contains(el) || !String(el.dataset.act).startsWith('vu-')) return;
    if (el.getAttribute('aria-disabled') === 'true') return;
    const what = el.dataset.act;
    if (what === 'vu-glow') {
      glow = GLOWS[(GLOWS.indexOf(glow) + 1) % GLOWS.length];
      keep('vu-glow', glow);
      paint();
    } else if (what === 'vu-view') {
      if (el.dataset.view === viewName) return;
      viewName = el.dataset.view;
      keep('vu-view', viewName);
      paint();
      sync();
      if (viewName === 'meters') frame(clock());
    } else if (what === 'vu-ref-down') {
      setRef(ref - 1);
    } else if (what === 'vu-ref-up') {
      setRef(ref + 1);
    } else if (what === 'vu-ref-reset') {
      setRef(REF_DEFAULT);
    }
  }, { signal });

  signal.addEventListener('abort', () => {
    watch.stop();
    levelsHold?.close();
    fullHold?.close();
    levelsHold = null;
    fullHold = null;
    if (raf) win.cancelAnimationFrame(raf);
    raf = 0;
    stopSpectrum();
  }, { once: true });

  view.openDrawn = inPlace;
  signal.addEventListener('abort', () => { view.openDrawn = null; }, { once: true });

  return {
    /** What the section is drawn from. */
    state: () => ({
      glow, view: viewName, ref, defaults: REF_DEFAULT, min: REF_MIN, max: REF_MAX, note: noteOf(),
    }),
  };
}
