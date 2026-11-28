/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Draws the audio panel and keeps it current: the ten features' words and the traces of their 18 s
// history (the scene's history, read from each `step` on `vg.steps`), as Panel (one box), Spread
// (each feature in its own format around the screen's sides) or Off. The Panel:
// 24-segment bars with a peak hold and ticks, three-digit figures, 18 s traces (9 s on a phone)
// with their axis, the lamps and the rate, seed and quality at the foot, in the colours of the
// scene's live palette. It fades away with the rest of the chrome (the page's `vg-dim`, in the
// style sheet; opacity only, its words stay in the page) unless the menu pins it. On a phone it is
// shown only while the dash's features chip has it open (`data-feat` on the page, `feat` on
// `vg.steps`), at the top left of the menu button, and a tap on it closes it. The time is the
// scene's: the sum of the steps' `dt`. `setMode(mode)` is how the theme's pref and its `A` key
// change the mode; the HUD hears it as a `panel` event on `vg.steps` (Spread keeps its modules in).

import { WINDOWS, panelInk, panelMarkup, paintPanel } from './draw/panel.js';
import { SECONDS } from './history.js';
import { KEYS } from './features.js';
import { FADE, PHONE_QUERY, featureWords, rateWords } from './hud.js';
import { PANEL } from './words.js';

/** The modes, in the order `A` cycles them. */
export const MODES = Object.freeze(['panel', 'spread', 'off']);

/** The over lamp holds this long, and is lit above this strength (a decay per second). */
const OVER_FALL_PER_S = 3;
const OVER_LIT = 0.2;

/** The pref the menu's `Audio panel` row keeps the mode in. */
const PANEL_PREF = 'panel';

const SKELETON = '<canvas class="vg-h-pcv" aria-hidden="true"></canvas>'
  + '<div class="vg-h-pbox"></div>';

/** 
 * Wire the panel in `vg.page` (its `#vg-panel`) to `vg.core`. Returns `{ driver, setMode, setFades,
 * mode }`; the driver part answers `shown(label)` with the mode's word (`Audio panel`) or a
 * feature's value (`Bass`, `Centroid`, ...; case blind) and `operate('Audio panel', { mode })`.
 * Time is the scene's: the sum of the `dt` of the steps.
 */
export function wirePanel(vg) {
  const { page, signal } = vg;
  const doc = page.ownerDocument;
  const win = doc.defaultView;
  const root = page.querySelector('#vg-panel');
  root.innerHTML = SKELETON;
  const canvas = root.querySelector('canvas');
  const box = root.querySelector('.vg-h-pbox');
  let ctx;
  /** The canvas's 2D context, asked for at the first draw (null where there is none). */
  const context = () => { ctx ??= canvas.getContext?.('2d') ?? null; return ctx; };

  // a phone's Panel keeps 9 s of history (the CSS lays it out the same way)
  const phone = win?.matchMedia?.(PHONE_QUERY) ?? null;

  let mode = 'panel';
  // the `Pin features panel` row's pref: the panel fades with the chrome unless it was pinned
  let fades = vg.prefs?.get?.('panel-fades', true) !== false;
  let over = 0;
  let paletteKey = '';
  let last = null;
  let ink = null;
  let sized = '';

  const words = {};

  function draw() {
    box.innerHTML = panelMarkup(mode, words, { phone: Boolean(phone?.matches) });
    // the axis' marks stand by the style API (an inline style attribute is the page's policy's to refuse)
    for (const mark of box.querySelectorAll('[data-at]')) mark.style.left = `${mark.dataset.at}%`;
    root.dataset.pinned = fades ? '0' : '1';
    vg.page.dataset.pinned = fades ? '0' : '1';
    root.hidden = mode === 'off';
    root.dataset.mode = mode;
  }

  /** The colours the traces take: the scene's live palette's (three of them), else the theme's tokens
   * (read once). */
  function inkOf(pair) {
    if (!ink) {
      const style = win.getComputedStyle(root);
      const token = (name, fallback) => style.getPropertyValue(name).trim() || fallback;
      ink = {
        hair: 'rgba(236, 246, 255, .08)',
        dim: 'rgba(236, 246, 255, .42)',
        gap: 'rgba(3, 6, 10, .8)',
        mid: token('--vg-mid', '#a9bccb'),
        ink: token('--vg-ink', '#e6f1f7'),
        c0: token('--vg-c0', '#4fd1c5'),
        c1: token('--vg-c1', '#7fe3c8'),
        c2: token('--vg-c2', '#b7a6ff'),
        c3: token('--vg-c3', '#e6f1f7'),
      };
    }
    const live = panelInk(pair, ink);
    const key = `${live.c0}|${live.c1}`;
    if (key !== paletteKey) {
      paletteKey = key;
      root.style.setProperty('--vg-pal0', live.c0);
      root.style.setProperty('--vg-pal1', live.c1);
      root.style.setProperty('--vg-pal2', live.c2);
    }
    return live;
  }

  function fit(c) {
    const rect = root.getBoundingClientRect();
    const dpr = win?.devicePixelRatio ?? 1;
    const key = `${rect.width}x${rect.height}@${dpr}`;
    if (key === sized) return;
    sized = key;
    canvas.width = Math.max(1, Math.round(rect.width * dpr));
    canvas.height = Math.max(1, Math.round(rect.height * dpr));
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function paint() {
    const c = mode === 'off' || !last ? null : context();
    if (!c) return;
    fit(c);
    const history = last.history;
    paintPanel(c, root, {
      values: last.features,
      peaks: Object.fromEntries(KEYS.map((k) => [k, history.hold(k)])),
      series: Object.fromEntries(KEYS.map((k) => [k, history.series(k)])),
      seconds: SECONDS,
      compact: false,
      window: phone?.matches ? WINDOWS.phone.seconds : WINDOWS.desk.seconds,
      ticks: phone?.matches ? WINDOWS.phone.ticks : WINDOWS.desk.ticks,
      ink: inkOf(last.palette),
    });
  }

  /** Put the words of the features and the lamps in place. */
  function put(state) {
    for (const key of KEYS) {
      words[key] = featureWords(key, state.features[key]);
      const el = box.querySelector(`[data-key="${key}"] .vg-h-v`);
      if (el && el.textContent !== words[key]) el.textContent = words[key];
    }
    const lit = (name, on) => {
      const lamp = box.querySelector(`[data-lamp="${name}"]`);
      if (lamp) lamp.dataset.on = on ? '1' : '0';
    };
    lit('over', over > OVER_LIT);
    lit('link', Boolean(state.cues?.linkDrop || state.cues?.weak));
    const q = vg.perf?.()?.q;
    const hz = rateWords(state.cues?.rateHz);
    const tight = Boolean(phone?.matches);
    const said = [tight ? hz.replace(' ', '') : hz, `SEED ${Number(state.seed ?? 0).toFixed(2)}`,
      `Q ${Number.isFinite(q) ? q.toFixed(2) : '—'}`].join(tight ? ' ' : '  ');
    const el = box.querySelector('.vg-h-rate');
    if (el && el.textContent !== said) el.textContent = said;
  }

  function onStep(event) {
    const state = event.detail;
    last = state;
    over = Math.max(state.raw?.peak ?? 0, over - (state.dt ?? 0) * OVER_FALL_PER_S);
    // a muted scene holds the panel to a part of its strength; the chrome's fade is the style sheet's
    root.style.opacity = state.cues?.mute ? String(FADE.muted) : '1';
    put(state);
    paint();
  }

  /** Change the mode (`panel`, `spread` or `off`): the panel wakes and the HUD is told. */
  function setMode(next) {
    if (!MODES.includes(next)) return false;
    const changed = next !== mode;
    mode = next;
    if (changed) {
      sized = '';
      draw();
    }
    if (last) put(last);
    paint();
    vg.steps.dispatchEvent(new globalThis.CustomEvent('panel', { detail: { mode } }));
    return true;
  }

  /** The pref: on, the panel fades with the chrome; off, it is pinned and stays. */
  function setFades(on) {
    fades = Boolean(on);
    root.dataset.pinned = fades ? '0' : '1';
    vg.page.dataset.pinned = fades ? '0' : '1';
  }

  vg.steps.addEventListener('step', onStep, { signal });
  // the dash's features chip opens and closes the panel on a phone; a tap on the panel closes it
  vg.steps.addEventListener('feat', () => { sized = ''; paint(); }, { signal });
  root.addEventListener('click', () => {
    // a tap on a faded panel only wakes the chrome (as the dash's tap does): it closes nothing
    if (fades && vg.page.classList.contains('vg-dim')) return;
    if (phone?.matches && vg.page.dataset.feat === 'open') {
      vg.steps.dispatchEvent(new globalThis.CustomEvent('feat', { detail: { open: false } }));
    }
  }, { signal });
  phone?.addEventListener?.('change', () => {
    sized = '';
    draw();
    if (last) put(last);
    paint();
  }, { signal });
  draw();

  return {
    mode: () => mode,
    setMode,
    setFades,
    driver: {
      shown(label) {
        const want = String(label).toLowerCase();
        if (want === 'audio panel') return PANEL.modes[mode];
        const key = KEYS.find((k) => PANEL.features[k].toLowerCase() === want);
        return key ? (box.querySelector(`[data-key="${key}"] .vg-h-v`)?.textContent ?? null) : null;
      },
      operate(target, change = {}) {
        if (String(target).toLowerCase() !== 'audio panel') return false;
        const taken = setMode(change.mode);
        if (taken) vg.prefs?.set?.(PANEL_PREF, change.mode);
        return taken;
      },
    },
  };
}
