/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Draws the HUD over the scene and keeps it current: the top line that is always on (the device,
// its link words, the verdict or the trouble), and the Host, DAC and Amp modules of hud.js as DOM
// text with one 2D canvas for the bars and sparklines. The modules are drawn again when the model
// changes (an amp's meters and the volume are put in the rows already drawn, so the rows stay the
// same elements); the director's moments (the `step`s on `vg.steps`) bring a module in for a few
// seconds and the audio panel's Spread keeps them all in; the VU rows follow the levels stream,
// held only while the Host module shows and the page is visible. A module's strength is its opacity
// alone, so its words stay in the page for a screen reader and the driver; a module that is out
// takes no pointer. Time is the scene's: the sum of the `dt` of the steps. The panel tells the HUD
// its mode with a `panel` event on `vg.steps`.

import { changedOf } from '../../core/path-words.js';
import { SETUP } from '../../core/popups/listed.js';
import { decodeLevels } from '../../core/vu-model.js';
import { dbmOf, rateOf } from './cues.js';
import {
  hudMarkup, metersInPlace, paintHud, topMarkup, volumeInPlace,
} from './draw/hud.js';
import {
  Levels, LinkHistory, Moments, PHONE_QUERY, ampOf, modulesOf, topLine,
} from './hud.js';
import { HUD } from './words.js';

/** The model events that change what the modules say; an amp's `meters` and the volume have their
 * own way. */
const EVENTS = ['model', 'device', 'part', 'fleet', 'host', 'level', 'state'];

/** A module this strong or less is out: it takes no pointer. */
const OUT = 0.02;

const SKELETON = '<canvas class="vg-h-cv" aria-hidden="true"></canvas>'
  + '<div class="vg-h-top"></div><div class="vg-h-mods"></div>';

/** 
 * Wire the HUD in `vg.page` (its `#vg-hud`) to `vg.core`. Returns `{ driver, spread }`: the driver
 * part answers `shown(label)` with the words a row shows (`Output`, `VU L`, `Path`, `Volume`, ...;
 * case blind; `Top line` for the line that is always on) and `linkWords()`, and `spread()` is
 * whether the panel is spread.
 */
export function wireHud(vg) {
  const { core, page, signal } = vg;
  const doc = page.ownerDocument;
  const win = doc.defaultView;
  const root = page.querySelector('#vg-hud');
  root.innerHTML = SKELETON;
  const canvas = root.querySelector('canvas');
  const top = root.querySelector('.vg-h-top');
  const box = root.querySelector('.vg-h-mods');
  let ctx;
  /** The canvas's 2D context, asked for at the first draw (null where there is none). */
  const context = () => { ctx ??= canvas.getContext?.('2d') ?? null; return ctx; };
  const phone = win?.matchMedia?.(PHONE_QUERY) ?? null;
  const narrow = () => Boolean(phone?.matches);

  const moments = new Moments();
  const links = new LinkHistory();
  const levels = new Levels();
  let now = 0;
  let modules = [];
  let drawn = '';
  let spread = false;
  let holding = null;
  let ink = null;
  let sized = '';
  let blank = false;

  /** The computer EQ's curve the core works out (the viewer's own edits laid over the model's). */
  const curveOf = (model) => core.computer?.toneOf?.(model)?.response ?? null;

  /** The alpha of a module: whole while the panel is spread (on a screen wide enough for them to
   * stand apart), else the director's moment's. */
  const alphaOf = (id) => (spread && !narrow() ? 1 : moments.alpha(id));

  /** The strength of every module as its opacity; a module that is out takes no pointer. */
  function applyAlpha() {
    for (const el of box.querySelectorAll('[data-mod]')) {
      const a = alphaOf(el.dataset.mod);
      el.style.opacity = a.toFixed(3);
      el.dataset.on = a > OUT ? '1' : '0';
    }
  }

  /** The top line from the model and the stream's state. */
  function drawTop() {
    const line = topLine(core.model.page(), core.model.state?.());
    const html = topMarkup(line);
    if (top.dataset.drawn !== html) {
      top.innerHTML = html;
      top.dataset.drawn = html;
    }
  }

  /** Draw the modules again when their markup is not the one drawn; a control with the keyboard on
   * it is found again by its row. */
  function render() {
    drawTop();
    const model = core.model.page();
    modules = modulesOf(model, { curve: curveOf(model) });
    const html = hudMarkup(modules);
    if (html === drawn) return;
    drawn = html;
    const at = doc.activeElement;
    const focus = box.contains(at) ? at.closest('[data-row]')?.dataset.row : null;
    box.innerHTML = html;
    if (focus) {
      const again = [...box.querySelectorAll('[data-row]')].find((r) => r.dataset.row === focus);
      again?.querySelector('button')?.focus({ preventScroll: true });
    }
    applyAlpha();
    paintVu(0);
    paint();
  }

  /** The VU rows from the levels after `dt` seconds. */
  function paintVu(dt) {
    const reading = levels.step(dt, now);
    for (const [id, r] of [['vu-l', reading.L], ['vu-r', reading.R]]) {
      const row = box.querySelector(`[data-row="${id}"]`);
      if (!row) continue;
      const text = r.lamp ? `${r.text} · ${HUD.over}` : r.text;
      const words = row.querySelector('.vg-h-v');
      if (words.textContent !== text) words.textContent = text;
      const slot = row.querySelector('.vg-h-slot');
      slot.dataset.v = r.bar.toFixed(4);
      slot.dataset.pk = r.peak.toFixed(4);
      if (r.lamp) row.dataset.tone = 'bad';
      else delete row.dataset.tone;
    }
  }

  /** The colours the bars and lines take, from the theme's tokens, read once. */
  function inkOf() {
    if (!ink) {
      const style = win.getComputedStyle(root);
      const token = (name, fallback) => style.getPropertyValue(name).trim() || fallback;
      ink = {
        fill: token('--vg-teal', '#4fd1c5'),
        line: token('--vg-teal-ink', '#7fe3c8'),
        warn: token('--vg-warn', '#f2c55c'),
        hair: token('--vg-hair', 'rgba(230, 241, 247, .10)'),
        gap: 'rgba(3, 4, 6, .8)',
        dim: token('--vg-mid', '#a9bccb'),
      };
    }
    return ink;
  }

  /** The canvas's backing store for the HUD's size and the screen's density. */
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

  /** Draw the bars and lines into the slots of the rows; nothing is drawn (the canvas is cleared
   * once) while every module is out. */
  function paint() {
    const alpha = Object.fromEntries(modules.map((m) => [m.id, alphaOf(m.id)]));
    const out = Object.values(alpha).every((a) => a <= OUT);
    if (out && blank) return;
    const c = context();
    if (!c) return;
    fit(c);
    blank = out;
    const eq = modules.find((m) => m.id === 'host')?.rows.find((r) => r.id === 'eq')?.slot;
    const link = modules.find((m) => m.id === 'dac')?.rows.find((r) => r.id === 'link')?.slot;
    paintHud(c, box, {
      sparks: eq ? { eq: eq.values } : {},
      link: link ? { ...link, points: links.points(now) } : null,
      alpha,
      ink: inkOf(),
    }, { rect: (el) => (el === box ? root.getBoundingClientRect() : el.getBoundingClientRect()) });
  }

  /** Keep the levels stream while the Host module shows and the page is visible, else let it go. */
  function holdLevels() {
    const want = !doc.hidden && (spread || moments.active('host'));
    if (want && !holding) {
      holding = core.levels.open({
        capture: (data) => { if (data?.capture !== 'running') levels.clear(); },
        frame: (text) => {
          const lv = decodeLevels(text)?.levels;
          if (lv) levels.push(lv, now);
        },
      });
    } else if (!want && holding) {
      holding.close();
      holding = null;
      levels.clear();
    }
  }

  /** What the rates on show are, so a change of either is a moment. */
  const rateKey = (model) => {
    const sample = (model?.device?.controls ?? []).find((c) => c.key === 'sample_rate');
    const route = model?.host?.route;
    const input = rateOf(sample?.text) ?? '';
    return `${route?.sink?.rate_hz ?? ''}|${route?.player_rate_hz ?? ''}|${input}`;
  };

  function onStep(event) {
    const state = event.detail;
    const dt = state.dt ?? 0;
    now += dt * 1000;
    const model = core.model.page();
    const age = model.device?.facts?.signal_age;
    links.push(dbmOf(age?.head), age?.heard_at_ms, now, model.shown);
    moments.step(dt, {
      events: state.events ?? [],
      changed: changedOf(model),
      rate: rateKey(model),
      has: modules.map((m) => m.id),
      narrow: narrow(),
    });
    applyAlpha();
    holdLevels();
    paintVu(dt);
    paint();
  }

  const stops = EVENTS.map((name) => core.model.on(name, render));
  stops.push(core.model.on('volume', () => {
    const model = core.model.page();
    modules = modulesOf(model, { curve: curveOf(model) });
    const row = modules.find((m) => m.id === 'dac')?.rows.find((r) => r.id === 'volume');
    if (!volumeInPlace(box, row)) {
      render();
      return;
    }
    drawn = hudMarkup(modules);
    paint();
  }));
  stops.push(core.model.on('meters', () => {
    const model = core.model.page();
    const amp = ampOf(model);
    if (!amp || !metersInPlace(box, amp)) {
      render();
      return;
    }
    modules = modulesOf(model);
    drawn = hudMarkup(modules);
    paint();
  }));
  vg.steps.addEventListener('step', onStep, { signal });
  vg.steps.addEventListener('panel', (event) => {
    spread = event.detail.mode === 'spread';
    root.dataset.spread = spread ? '1' : '0';
    applyAlpha();
    holdLevels();
    paint();
  }, { signal });
  phone?.addEventListener?.('change', () => { applyAlpha(); paint(); }, { signal });
  doc.addEventListener('visibilitychange', holdLevels, { signal });
  root.addEventListener('click', (event) => {
    if (event.target.closest?.('[data-act="config"]')) core.router.open(SETUP);
  }, { signal });
  // a control the keyboard reaches in a module that is out brings it in
  root.addEventListener('focusin', (event) => {
    const id = event.target.closest?.('[data-mod]')?.dataset.mod;
    if (id) moments.on(id, 4, narrow());
    applyAlpha();
  }, { signal });
  signal.addEventListener('abort', () => {
    for (const stop of stops) stop();
    holding?.close();
    holding = null;
  }, { once: true });

  render();

  return {
    spread: () => spread,
    driver: {
      /** The words the row labelled `label` shows now (case blind), its note after them. */
      shown(label) {
        const want = String(label).toLowerCase();
        if (want === 'top line') return top.textContent.trim() || null;
        for (const row of box.querySelectorAll('[data-row]')) {
          if (row.querySelector('.vg-h-k').textContent.toLowerCase() !== want) continue;
          const note = row.querySelector('.vg-h-note');
          const said = row.querySelector('.vg-h-v').textContent;
          return note ? `${said} \u00b7 ${note.textContent}` : said;
        }
        return null;
      },

      /** The words beside the shown device's name that tell its link state, or null while the link
       * is on. */
      linkWords() {
        const link = core.model.device?.()?.link;
        if (!link || link.state === 'on') return null;
        return top.querySelector('.vg-h-l')?.textContent.trim() || null;
      },
    },
  };
}
