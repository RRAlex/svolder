/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page's effects: three levels (Smoke, Glow, Off) kept per browser, the layer fixed behind
// everything that shows them, and where the dial or the meters sit. ember.js and live.js call it
// where the screen's level, steps, meters, away state and layout change.
//
// The effects are a registry: Smoke and Glow register here, in the order of `LEVELS` (Off is no
// effect). The frame's menu draws Ember's rows from `menuRows`: one radio per level (its
// `data-level` and its label, from the registry), then Off; what the menu does with a radio
// (whether it can be chosen, and why not) comes from the registered effect's `supported()`.
//
// Below the levels the menu holds the page's own switches (the visualizer's): rows that are on or
// off, not levels. A module registers one with `switchRow`; the menu shows it, checked as the
// module says, while the module says the screen has something for it to switch.

import * as smoke from './smoke.js';
import { TUNING, drawShade, seeded } from './smoke-model.js';

export { VolumeBurst, bigRise, stepOf } from './smoke-model.js';

/** The levels, in the order the menu lists them. */
export const LEVELS = ['smoke', 'glow', 'off'];

/** The registered effects, in the order of [`LEVELS`] (Off is no effect, so it is not here). */
const registry = [];

/**
 * Register an effect: `id` (one of [`LEVELS`]), `label` for people, `supported()` (`{ ok, why }`:
 * whether it can run here and, when it cannot, why in one line), `start(canvas, theme, hooks)`
 * (its handle, or `null` when it did not start) and `canvas` (default `true`: whether it draws on
 * a canvas the layer makes for it; Glow is the layer's own CSS and has none). A second effect of
 * the same `id` replaces the first.
 */
export function register({ id, label, supported, start, canvas = true }) {
  const effect = { id, label, supported, start, canvas };
  const at = registry.findIndex((e) => e.id === id);
  if (at >= 0) registry[at] = effect;
  else registry.push(effect);
}

/** The registered effects, in registration order. */
export function registered() {
  return registry.map((e) => ({ ...e }));
}

/** The registered effect for `id`, or `undefined` (Off). */
const effectOf = (id) => registry.find((e) => e.id === id);

// Smoke runs on its own director and tuning, handed over here (a theme names an effect and may
// hand it others); Glow is CSS only.
register({
  id: 'smoke',
  label: 'Smoke',
  supported: () => smoke.supported(),
  start: (canvas, theme) => smoke.start(canvas, theme, {
    director: smoke.makeDirector, tuning: TUNING,
  }),
});
register({ id: 'glow', label: 'Glow', supported: () => ({ ok: true, why: null }),
  start: () => null, canvas: false });

/** The storage key the choice is kept under. */
export const KEY = 'svolder.effects';

/** The level kept in `storage`: one of [`LEVELS`], or `null` when none is kept, the value is not
 * one or the read is refused. */
export function stored(storage) {
  try {
    const value = storage?.getItem(KEY);
    return LEVELS.includes(value) ? value : null;
  } catch {
    return null;
  }
}

/** Keep `level` in `storage`; `false` when it is not a level or the write is refused. */
export function keep(storage, level) {
  if (!LEVELS.includes(level)) return false;
  try {
    storage.setItem(KEY, level);
    return true;
  } catch {
    return false;
  }
}

/**
 * The level for a page view. `asked` is the address's `effects` level or `null`; `chosen` is what
 * the address, the kept level or the default (Glow under reduced motion, else Smoke) names;
 * `level` is what shows (Glow in place of a Smoke that `can` not run); `smokeOff` is why Smoke
 * cannot run, or `null`.
 */
export function pick({ stored: kept, asked, reduced, can }) {
  const chosen = asked ?? kept ?? (reduced ? 'glow' : 'smoke');
  return {
    level: chosen === 'smoke' && !can.ok ? 'glow' : chosen,
    chosen,
    smokeOff: can.ok ? null : can.why,
  };
}

/** The menu's switches, by their `data-switch`: `{ label, get(), set(on), available() }` (the page
 * registers them, see [`switchRow`]). */
const switches = new Map();

/**
 * Register the menu's switch row `id` (the `data-switch` of its button): `label` is its words,
 * `get()` says whether it is on, `set(on)` turns it on or off, and `available()` whether the
 * screen has anything for it to switch (the row shows only then).
 */
export function switchRow(id, { label, get, set, available }) {
  switches.set(id, { label, get, set, available });
}

/** Forget the switch row `id`: its page is gone. */
export function removeSwitchRow(id) {
  switches.delete(id);
}

/** The element holding the effects, the first child of the theme's root. */
let layer = null;

/** The level that shows, why an effect cannot run (by id; empty until the first probe), the
 * browser's storage (or `null`), whether the screen is away and whether the page's stream is
 * lost. */
let shown = 'glow';
const unavailable = new Map();
let storage = null;
let isAway = false;
let isLost = false;

/** Whether the effects' support has been probed (once per page: Smoke's probe makes a throwaway
 * WebGL context, so a page on Glow or Off does not pay for it until the menu first opens). */
let probed = false;

/** The smoke's canvas and handle while it shows. */
let canvas = null;
let handle = null;

/** The menu's reason for an effect that failed to start. */
const noStart = (effect) => `${effect.label} could not start here`;

/** Smoke's reasons in the menu when the browser reset its graphics (with the popup for the
 * first). */
const RESET_SAID = "Smoke stopped: the browser's graphics were reset; showing the glow";
const RESET_WHY = "The browser's graphics were reset, so Smoke is off for this page";

/** The page's scroll position at the last layout, so the smoke gets the delta. */
let seenX = 0;
let seenY = 0;

/** The muted shade this page drew, once, for the smoke (kept across Smoke, Glow and Smoke again),
 * and the check-only seed of the smoke's random draws (`null`: `Math.random`). */
let shade = null;
let seedValue = null;

/** The check-only forced roll of every settle (`null`: a random one). */
let rollValue = null;

/** The last glow level the page sent (the volume's), for a smoke that starts later. */
let lastLevel = 0.4;

/** How the music, while the visualizer's frames flow, adds to the glow's level: `GAIN` is the
 * most it adds (to the volume's level, which stays what the level is without music), and the
 * frame's loudest RMS level (0 to 1 of the capture's -72 to 0 dB scale) counts from `FLOOR` over
 * `SPAN`, so ordinary music reads in the upper half and a quiet passage adds little. A beat's
 * puff is `BEAT_FLOOR` of a full puff and grows with the loudness to a full one. */
export const MUSIC = Object.freeze({ GAIN: 0.3, FLOOR: 0.4, SPAN: 0.5, BEAT_FLOOR: 0.4 });

/** How loud a frame is, 0 to 1, from its `[rmsL, rmsR, peakL, peakR]` levels (each 0 to 1): the
 * louder channel's RMS on the scale of [`MUSIC`]; anything else is silence. */
export function loudness(levels) {
  if (!levels || levels.length < 2) return 0;
  const rms = Math.max(Number(levels[0]), Number(levels[1]));
  if (!Number.isFinite(rms)) return 0;
  return Math.min(1, Math.max(0, (rms - MUSIC.FLOOR) / MUSIC.SPAN));
}

/** The glow's level with `music` (a loudness, 0 to 1, or `null` for none) on the `volume`'s level
 * (0 to 1): the volume's, plus the music's share, never over 1. */
export function glowLevel(volume, music) {
  const heard = Number.isFinite(music) ? Math.min(1, Math.max(0, music)) : 0;
  return Math.min(1, volume + MUSIC.GAIN * heard);
}

/** How loud the music is now (a loudness), or `null` while no frames flow. */
let heard = null;

/** The page's word to the person (a toast). */
let say = () => {};
/** Tells the frame the menu's rows may say something else now. */
let menuChanged = () => {};

/**
 * The layer in the page now. A layer that is another element than the one held (a theme
 * mounted again draws its own) takes over the level, the away look and the menu's checks.
 */
function attach() {
  const now = document.querySelector('.fx');
  if (now === layer) return layer !== null;
  stopSmoke();
  layer = now;
  if (!layer) return false;
  applied = glowLevel(lastLevel, heard);
  layer.style.setProperty('--lvl', applied.toFixed(2));
  layer.classList.toggle('away', isAway);
  layer.classList.toggle('hearing', heard !== null);
  show(shown);
  return true;
}

/** The level last written to the layer and the smoke, so a draw that changes nothing writes
 * nothing. */
let applied = null;

/** The glow's level as the layer's custom property (through CSSOM) and the smoke's: the volume's
 * level with the music's share while it plays. While music plays only a change of the two
 * decimals the property takes is written (the smoke's target moves by less than that between
 * them), so a draw per frame costs a few writes a second, not sixty. */
function apply() {
  const x = glowLevel(lastLevel, heard);
  if (x === applied) return;
  const same = applied !== null && x.toFixed(2) === applied.toFixed(2);
  if (same && heard !== null) return;
  applied = x;
  if (!same) layer?.style.setProperty('--lvl', x.toFixed(2));
  handle?.level?.(x);
}

/** The glow's level from the volume, 0 to 1; the music's share stays on top while it plays. */
export function level(x) {
  if (!Number.isFinite(x)) return;
  lastLevel = x;
  attach();
  apply();
}

/** The music's loudness (0 to 1, from [`loudness`]) from the visualizer's frames, each draw while
 * they flow; `null` when they stop (the card hidden, the capture gone), which takes the glow back
 * to the volume's level. A number that is not a loudness changes nothing. */
export function music(x) {
  if (x !== null && !Number.isFinite(x)) return;
  // to the 0.01 the glow can show, so a loudness that moved less changes nothing
  const next = x === null ? null : Math.round(Math.min(1, Math.max(0, x)) * 100) / 100;
  if (next === heard) return;
  const toggled = (next === null) !== (heard === null);
  heard = next;
  attach();
  if (toggled) layer?.classList.toggle('hearing', heard !== null);
  apply();
}

/** A beat of the music, on time (the visualizer schedules it from the tempo): Smoke puffs from
 * the knob (at the volume's fraction, as a volume step takes it), bigger for louder music; Glow
 * and Off ignore it, and so does a page the frames do not reach. */
export function beat() {
  if (heard === null) return;
  handle?.beat?.(MUSIC.BEAT_FLOOR + (1 - MUSIC.BEAT_FLOOR) * heard, lastLevel);
}

/** A volume step: Smoke puffs from the dial; Glow and Off ignore it. `big` is a step of a big
 * rise (over 10 dB in a burst, or into the red zone), which makes a note more likely. */
export function step(direction, size, at, big = false) {
  handle?.step?.(direction, size, at, big);
}

/** The amp's meter fractions, in the anchor's meter order; Glow and Off ignore them. */
export function meters(fractions) {
  handle?.meters?.(fractions);
}

/** The smoke's counters while Smoke shows, else `null`, and the page's `shade` (the base's `hsl`,
 * the `greyShare`, the two `sides` tones and the `dial` tone); read by the browser checks only,
 * no page code calls it. */
export function stats() {
  const counters = handle?.stats?.();
  if (!counters) return null;
  return {
    ...counters,
    shade: shade && {
      hsl: { ...shade.hsl }, greyShare: shade.greyShare, sides: shade.sides.map((c) => [...c]),
      dial: [...shade.dial],
    },
  };
}

/** Pause (`false`, emptying the grids) or resume the haze's feeding while Smoke shows; read by
 * the browser checks only, no page code calls it (the checks of what a stroke or a step leaves
 * want the cloud alone). */
export function haze(on) {
  handle?.haze?.(on);
}

/** Make the smoke's random draws come from `seeded(n)` from the next Smoke start on; read by the
 * browser checks only, no page code calls it. */
export function seed(n) {
  seedValue = Number.isFinite(n) ? n : null;
}

/** Make every settle's roll come out as `v` (0 always makes a note, 1 never does; `null` back to
 * random draws), now and for the next Smoke start; read by the browser checks only, no page code
 * calls it. */
export function roll(v) {
  rollValue = Number.isFinite(v) ? v : null;
  handle?.roll?.(rollValue);
}

/** The smoke's velocity and dye grids as floats while Smoke shows, else `null`; read by the
 * browser checks only, no page code calls it; `null` too where float readback is unsupported. */
export function readback() {
  return handle?.readback?.() ?? null;
}

/** The screen's device is away (it is off or its link is lost): the effects grey. */
export function away(on) {
  isAway = Boolean(on);
  attach();
  layer?.classList.toggle('away', isAway);
  handle?.away?.(isAway || isLost);
}

/** The page's own stream is lost (or back): Smoke greys and stops like an away screen, and
 * Glow's own lost look (the body's class) stays as it is. */
export function lost(on) {
  isLost = Boolean(on);
  handle?.away?.(isAway || isLost);
}

/** The centre of an element's box, in viewport pixels. */
function centre(element) {
  const box = element.getBoundingClientRect();
  return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
}

/** The content column's box of a screen, `{ left, right }` in viewport pixels. */
function columnOf(root) {
  const box = root.getBoundingClientRect();
  return { left: box.left, right: box.right };
}

/**
 * Where the screen's effects centre sits, in viewport pixels: the dial of a DAC (with its track's
 * radius), the meters of an amp (with each meter's centre in page order), and in both the
 * content column's box (`column`); nothing on a screen with neither.
 */
export function anchorOf(root) {
  const column = columnOf(root);
  const knob = root.querySelector('sv-knob');
  if (knob?.svg) {
    const { cx, cy, radius } = knob.geometry();
    return { x: cx, y: cy, radius, meters: [], column };
  }
  const meters = root.querySelector('.meters');
  if (meters) {
    const at = centre(meters);
    const box = meters.getBoundingClientRect();
    return {
      ...at,
      radius: Math.min(box.width, box.height) / 2,
      meters: [...meters.querySelectorAll('figure.meter')].map(centre),
      column,
    };
  }
  return null;
}

/** Re-read where the glow belongs and move it there: the anchor, or the column's centre 170 px
 * below the screen's top when the screen has no dial and no meters. */
export function layout() {
  const dx = window.scrollX - seenX;
  const dy = window.scrollY - seenY;
  seenX = window.scrollX;
  seenY = window.scrollY;
  const before = handle;
  if (!attach()) return;
  if (handle && handle === before && (dx !== 0 || dy !== 0)) handle.scroll?.(dx, dy);
  const view = document.querySelector('#screen .view');
  if (!view) return;
  const box = view.getBoundingClientRect();
  const found = anchorOf(view) ?? { x: box.left + box.width / 2, y: box.top + 170 };
  layer.style.setProperty('--fx-x', `${found.x.toFixed(1)}px`);
  layer.style.setProperty('--fx-y', `${found.y.toFixed(1)}px`);
  layer.style.setProperty('--fx-col', `${view.clientWidth}px`);
  // the first level and place stand now; a frame later the glow's opacity starts to fade
  if (layer.dataset.level && !layer.classList.contains('settled')) {
    requestAnimationFrame(() => layer?.classList.add('settled'));
  }
  handle?.anchor?.(anchorOf(view) ?? { column: columnOf(view) });
}

/** The alpha (0 to 1) of a computed background colour: what follows `/` in any colour function
 * (`rgb()`, `color()`, `oklab()`), the fourth part of a comma `rgba()`, 0 for `transparent`; any
 * other value counts as opaque. */
export function backgroundAlpha(css) {
  if (css === 'transparent') return 0;
  const fn = css.match(/^[a-z-]+\((.*)\)$/i);
  if (!fn) return 1;
  const slash = fn[1].lastIndexOf('/');
  const part = slash >= 0 ? fn[1].slice(slash + 1) : fn[1].split(',').slice(3).join(',');
  const text = part.trim();
  const value = parseFloat(text);
  if (!Number.isFinite(value)) return 1;
  return text.endsWith('%') ? value / 100 : value;
}

/** Whether the page shows what is behind it at the viewport point `x`, `y`: no control there, and
 * no element from there up to the body with a background of its own (a card, a graph). */
function showsSmoke(x, y) {
  const at = document.elementFromPoint(x, y);
  if (at?.closest(CLICKABLE)) return false;
  for (let el = at; el && el !== document.body && el !== document.documentElement;
    el = el.parentElement) {
    const style = getComputedStyle(el);
    if (style.backgroundImage !== 'none') return false;
    if (backgroundAlpha(style.backgroundColor) >= 0.35) return false;
  }
  return true;
}

/** Whether the box of `half` px around `x`, `y` shows the smoke at nine points (its centre, edges
 * and corners): where a note can be seen. */
function openBox(x, y, half) {
  const steps = [-1, 0, 1];
  return steps.every((dx) => steps.every((dy) => showsSmoke(x + dx * half, y + dy * half)));
}

/** The page's look for the smoke: its shade (drawn on first use), its random generator and the
 * test for where a note can be seen. */
function theme() {
  shade ??= drawShade(Math.random);
  return {
    shade, random: seedValue === null ? Math.random : seeded(seedValue), roll: rollValue,
    open: openBox,
  };
}

/** Ask once whether each registered effect can run here (a throw counts as it cannot) and note
 * the reasons. */
function probe() {
  if (probed) return;
  probed = true;
  for (const effect of registry) {
    let can;
    try {
      can = effect.supported();
    } catch (error) {
      console.warn(
        `svolder: the check for ${effect.label} threw, so ${effect.label} is off for this page`,
        error,
      );
      can = { ok: false, why: noStart(effect) };
    }
    if (!can.ok) unavailable.set(effect.id, can.why);
  }
}

/** The browser reset its graphics under the smoke: Glow shows for the rest of the page's life,
 * Smoke is marked unavailable, and the person is told once. */
function contextLost() {
  unavailable.set('smoke', RESET_WHY);
  show('glow');
  say(RESET_SAID);
}

/** Stop the smoke: its canvas goes and its handle frees what it holds. The reset listener goes
 * first, so a stop chosen here never reads as the browser's reset. */
function stopSmoke() {
  canvas?.removeEventListener('webglcontextlost', contextLost);
  handle?.stop?.();
  handle = null;
  canvas?.remove();
  canvas = null;
}

/** Show `want` (Smoke falls back to Glow when it does not start or cannot run) and return what
 * shows. */
function show(want) {
  stopSmoke();
  const next = unavailable.has(want) ? 'glow' : want;
  shown = next;
  const effect = effectOf(next);
  if (effect?.canvas) {
    canvas = document.createElement('canvas');
    canvas.className = 'smoke';
    canvas.setAttribute('aria-hidden', 'true');
    layer.append(canvas);
    try {
      handle = effect.start(canvas, theme(), { say: (words) => say(words) });
    } catch (error) {
      console.warn(`svolder: ${effect.label} threw while starting, so the page shows Glow`,
        error);
      handle = null;
    }
    canvas.addEventListener('webglcontextlost', contextLost);
    if (!handle) {
      canvas.removeEventListener('webglcontextlost', contextLost);
      canvas.remove();
      canvas = null;
      unavailable.set(effect.id, noStart(effect));
      shown = 'glow';
      say(`${effect.label} could not start here, so the page shows Glow.`);
    }
  }
  layer.dataset.level = shown;
  menuChanged();
  if (handle) {
    handle.level?.(glowLevel(lastLevel, heard));
    handle.away?.(isAway || isLost);
    layout();
  }
  return shown;
}

/** The menu is about to open: ask whether each effect can run here (the first time only). */
export function opening() {
  probe();
}

/**
 * Ember's rows in the frame's menu: the effects' heading, a radio for each registered effect and
 * Off (an effect that cannot run here is disabled, with the reason, once the menu has asked, see
 * [`opening`]), and the page's own switches under a rule.
 */
export function menuRows() {
  const rows = [{ kind: 'heading', text: 'Ember\'s effects' }];
  for (const id of LEVELS) {
    const label = effectOf(id)?.label ?? 'Off';
    rows.push({
      kind: 'radio',
      id,
      label,
      dataset: { level: id },
      checked: () => shown === id,
      disabled: () => unavailable.get(id) ?? null,
      choose() {
        // the level already shown keeps running: a fresh start would be an empty cloud
        if (id !== shown) show(id);
        keep(storage, id);
      },
    });
  }
  if (switches.size > 0) rows.push({ kind: 'sep' });
  for (const [id, one] of switches) {
    rows.push({
      kind: 'check',
      id,
      label: one.label,
      dataset: { switch: id },
      checked: () => Boolean(one.get()),
      available: () => Boolean(one.available()),
      flip: () => one.set(!one.get()),
    });
  }
  return rows;
}

/** The screen's controls a click on which may make a note: buttons, switches, radios, input
 * choices, tone-control modes and sliders. */
export const CLICKABLE = 'button, [role=switch], [role=radio], [role=button], sv-slider, '
  + 'input[type=radio], input[type=checkbox]';

/** What a click on the header's device tabs or the frame's menu is: never a control's. */
const NOT_CONTROLS = '.dev, .fxbtn, #fx-menu';

/** The screen's enabled control a click on `target` lands on, or `null`. */
export function controlOf(target) {
  if (!(target instanceof Element) || target.closest(NOT_CONTROLS)) return null;
  const control = target.closest(CLICKABLE);
  return control && control.closest('#screen') && control.getAttribute('aria-disabled') !== 'true'
    && !control.disabled ? control : null;
}

/** Wire the layer at page start: the level (kept level, the address's `effects`, reduced motion,
 * whether Smoke can run) and the layer's follow of the window's size, the scroll and the page's
 * height. `say` is the page's toast and `changed()` tells the frame's menu that its rows may say
 * something else; what is wired goes when `signal` is aborted (the smoke stops with it, see
 * [`stop`]). */
export function init({ say: speak = () => {}, changed = () => {}, signal } = {}) {
  if (!document.querySelector('.fx')) return;
  say = speak;
  menuChanged = changed;
  seenX = window.scrollX;
  seenY = window.scrollY;
  try {
    storage = window.localStorage;
  } catch {
    storage = null;
  }
  const asked = new URLSearchParams(location.search).get('effects');
  const wanted = {
    stored: stored(storage),
    asked: LEVELS.includes(asked) ? asked : null,
    reduced: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
  };
  // Smoke is probed here only when it is the level to show
  let picked = pick({ ...wanted, can: { ok: true } });
  if (picked.chosen === 'smoke') {
    probe();
    picked = pick({
      ...wanted, can: { ok: !unavailable.has('smoke'), why: unavailable.get('smoke') ?? null },
    });
  }
  layer = document.querySelector('.fx');
  shade ??= drawShade(Math.random);
  show(picked.level);
  document.addEventListener('click', (event) => {
    const control = controlOf(event.target);
    if (!control) return;
    // a key's click has no pointer place: use the control's centre
    const at = event.detail === 0 ? centre(control) : { x: event.clientX, y: event.clientY };
    handle?.click?.(at.x, at.y);
  }, { signal });
  let queued = false;
  const later = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      layout();
    });
  };
  window.addEventListener('resize', later, { passive: true, signal });
  window.addEventListener('scroll', later, { passive: true, signal });
  const resizing = new ResizeObserver(later);
  resizing.observe(document.documentElement);
  signal?.addEventListener('abort', () => {
    resizing.disconnect();
    stop();
  });
}

/** The layer's end: the smoke stops and what the layer holds is freed. */
export function stop() {
  stopSmoke();
  layer = null;
  isLost = false;
  menuChanged = () => {};
}
