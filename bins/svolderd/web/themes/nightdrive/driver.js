/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What Night drive's checks reach into: its mounted scene (the floor, the music's levels) and the
// test driver the contract and completeness checks drive (see the checks' README; core/contract.js
// lists what they ask for). The driver does what a person does on Night drive's page: it shuts or
// opens the sheet (the volume, the mute, the input, the power key, the transport and the cover live
// on the dash; everything else on the sheet), unfolds the facts, scrolls to the word, scale or chip,
// then taps it, presses a key on a scale or picks an option of the list a chip opens (`operate`),
// and reads the words the page shows for it (`shown`). It never calls an action of the core itself:
// a write the check sees went through Night drive's own handlers.
//
//   reach(target) -> boolean            puts the sheet as the target needs it, scrolls to it, says whether it is drawn
//   operate(target, change) -> void     { step: ±1 } | { raw } | { value } | { on } | { apply } |
//                                       { recall } | { save } | {}
//   shown(target) -> string | null      the words shown for a control, or for a fact by its label
//   state() -> { lost, waiting, resting, toasts }
//   linkWords() -> string | null        the words drawn beside the shown device's name that tell its link
//                                       state (null when none are drawn)
//
//   perf() -> { draws, drawMs, p95, fps, pace, scale, capped, step, wet }   the draws so far, the mean and the 95th percentile of the last 60 in ms, the draws in the last second, the pace
//                                       asked of the loop now (the least ms between draws), the canvases' backing
//                                       pixels per CSS px, whether draws are held to 30 a second, the steps down taken, whether the budget
//                                       lets the wet road be drawn
//   sky() -> { log, rays, stars, levels }   the sky events' log (rays, star, shower, with their inputs), the rays' alpha now
//                                       (0 to 0.55), the shooting stars alive, and what the music reaches: `levels` is
//                                       `{ bass, highs, bassHeldS, highsJump, bassRel, highsRel, jumpRel, max60: { bass, highs, highsJump },
//                                       ear }`, the `Rel`s against the last minute's loudest and `ear` the mood's
//                                       `{ level, peak, rel, med, long, balance, peakHeld, embers }`
//   look() -> { mood, wet, haze, x, cruise, flare, wave, since, embers }   what the mood has made of the music now
//   tune() -> { w, h, H, gMax, m, target, allowed, held, heldSide, lit, diamonds, label, bubbles, curveY }
//                                       the mountains' tuning now: the scene's size in CSS px, the morph, the band
//                                       held (and the diamond's side when it is Q that is held) and its label, the
//                                       band whose bell is lit with its two diamonds `[{ side, x, y }]`, the bubbles
//                                       `[{ band, x, y }]` in CSS px
//   mood() -> { mood, dull, flat }      how the page stands (`ok`, `unreachable`, `lost`, `wait`, `nocap`), whether the scene is
//                                       drawn dull (greys, nothing moving) and whether the mountains lie flat
//   forceStepDown() -> bool             step the budget down as slow draws would (the wet road goes first)
//   curveAt(x) -> y                     the y of the curve drawn now at an x in CSS px, null while none

/** The core and the mounted scene, `null` until Night drive is mounted and the page's `view` (nightdrive.js and scene-wire.js fill them). */
export const bound = { core: null, scene: null, view: null, sheet: null };

/** The attribute-selector form of `text`. */
const q = (text) => CSS.escape(String(text));

/** The next frame, and a moment for the handlers a tap starts to run. */
const settle = () => new Promise((resolve) => {
  requestAnimationFrame(() => setTimeout(resolve, 30));
});

/** The first element matching `selector`, or nothing. */
const one = (selector) => document.querySelector(selector);

/** Whether `element` takes room on the page. */
function taken(element) {
  if (!element?.isConnected) return false;
  if (typeof element.checkVisibility === 'function' && !element.checkVisibility()) return false;
  const box = element.getBoundingClientRect();
  return box.width > 0 && box.height > 0;
}

/** Whether `element` is on the screen now (at least a part of it is in the window). */
function onScreen(element) {
  const box = element.getBoundingClientRect();
  return box.bottom > 0 && box.right > 0 && box.top < innerHeight && box.left < innerWidth;
}

/** What a control key stands for in the shown device's model. */
function controlOf(key) {
  const dev = bound.core.model.device();
  if (dev?.volume && key === dev.volume.key) return { kind: 'volume' };
  if (dev?.volume && key === dev.volume.mute) return { kind: 'mute' };
  if (dev?.power && key === dev.power.key) return { kind: 'power' };
  const found = [...(dev?.controls ?? []), ...(dev?.power?.toggles ?? []),
    ...(dev?.power?.readouts ?? [])].find((control) => control.key === key);
  return found ? { kind: found.kind, control: found, power: dev.power?.toggles?.includes(found) } : null;
}

/** Whether `target` is drawn on the dash (the sheet shut) rather than on the sheet. */
function onDash(target) {
  if (target === 'Cover') return true;
  if (typeof target === 'string') return false;
  if ('transport' in target) return true;
  if (!('control' in target)) return false;
  const kind = controlOf(target.control)?.kind;
  if (kind === 'volume' || kind === 'mute') return true;
  if (kind === 'power') return Boolean(one('#nd-dash [data-act="power"]'));
  return kind === 'choice' && target.control === 'input';
}

/** The element a target is, as the page draws it, or nothing. */
function find(target) {
  if (target === 'Cover') return one('#nd-dash .nd-cover img');
  if (typeof target === 'string') return one(`#nd-facts-body td[data-fact="${q(target)}"]`);
  if ('control' in target) {
    const key = q(target.control);
    const kind = controlOf(target.control);
    switch (kind?.kind) {
      case 'volume': return one('#nd-dash [data-act="volume-up"]');
      case 'mute': return one('#nd-dash [data-act="mute"]');
      case 'power': return one('#nd-dash [data-act="power"]') ?? one('#nd-controls [data-act="power"]');
      case 'choice':
        if (target.control === 'input') {
          return one('#nd-dash [data-act="input"][data-list]') ?? one('#nd-dash [data-act="input"]');
        }
        return one(`#nd-controls [data-act="choice"][data-key="${key}"]`)
          ?? one(`#nd-controls [data-act="choice-list"][data-key="${key}"]`);
      case 'toggle': return kind.power
        ? one(`#nd-controls [data-act="power-toggle"][data-key="${key}"]`)
        : one(`#nd-controls [data-act="toggle"][data-key="${key}"]`);
      case 'slider': return one(`#nd-controls .nd-scale[role="slider"][data-key="${key}"]`);
      case 'readout': return one(`#nd-controls [data-readout="${key}"]`);
      default: return null;
    }
  }
  if ('band' in target) {
    return one(`[data-act="band-type"][data-band="${q(target.band)}"][data-param="${q(target.param)}"]`);
  }
  if ('mode' in target) return one(`#nd-mode [data-act="mode"][data-mode="${q(target.mode)}"]`);
  if ('loudness' in target) return one('#nd-loud');
  if ('preset' in target) return one(`#nd-presets [data-act="preset"][data-name="${q(target.preset)}"]`);
  if ('slot' in target) return one(`#nd-slots [data-act="slot"][data-slot="${q(target.slot)}"]`);
  if ('setting' in target) return one(`#nd-slots [data-act="setting"][data-name="${q(target.setting)}"]`);
  if ('transport' in target) return one(`#nd-dash [data-button="${q(target.transport)}"]`);
  return null;
}

/** A tap on `element`. */
const tap = (element) => element.click();

/** A key press on `element`, which a person has focused first. */
function key(element, name) {
  element.focus({ preventScroll: true });
  element.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }));
}

/** Open the list under `opener` and choose the option whose value is `value`. A viewer who may not
 * write gets no list: the tap is all there is, and the core says why. */
async function pick(opener, value) {
  tap(opener);
  await settle();
  const option = one(`.nd-list .nd-opt[data-value="${q(value)}"]`);
  if (option) tap(option);
  else if (!bound.core.model.device().inert) throw new Error(`no option ${value}`);
}

/** Whether the sheet stands open. */
const sheetOpen = () => Boolean(one('#nd-dialog'));

/** Put the sheet where `target` needs it: shut for the dash, open for the rest. */
async function sheetFor(target) {
  const want = !onDash(target);
  if (sheetOpen() === want) return;
  tap(want ? one('#nd-sheet') : one('#nd-sheet-x'));
  await settle();
  await settle();
}

/** Open the facts when `target` is drawn inside them folded away, as a person taps their title. */
async function unfold(target) {
  if (typeof target !== 'string' || !one('#nd-facts-body[hidden]')) return;
  tap(one('[data-act="facts-fold"]'));
  await settle();
}

/** Put the sheet right for `target`, scroll to it and say whether it is drawn and in view; looked for
 * again after the scroll, which a section drawn again may have replaced. */
async function seen(target) {
  await sheetFor(target);
  await unfold(target);
  find(target)?.scrollIntoView({ block: 'center' });
  await settle();
  const element = find(target);
  return Boolean(element) && taken(element) && onScreen(element);
}

/** Step the volume to `raw` by taps on its keys, each waiting for the model to show the one before. */
async function volumeTo(raw) {
  const read = () => bound.core.model.device()?.volume?.raw;
  for (let n = 0; n < 80; n++) {
    const at = read();
    if (at === raw || typeof at !== 'number') return;
    tap(one(`#nd-dash [data-act="volume-${raw > at ? 'up' : 'down'}"]`));
    for (let wait = 0; wait < 40 && read() === at; wait++) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    // a step that cannot move it (the cap) is a quiet stop: the check judges by what was sent
    if (read() === at) return;
  }
}

/** Move a scale to `raw` by arrow presses until it stands there, each waiting for the page to show the one before. */
async function scaleTo(target, raw) {
  const read = () => Number(find(target)?.getAttribute('aria-valuenow'));
  for (let n = 0; n < 60; n++) {
    const at = read();
    if (at === raw || Number.isNaN(at)) return;
    key(find(target), raw > at ? 'ArrowRight' : 'ArrowLeft');
    await settle();
    if (read() === at) return;
  }
}

/** Whether a switch is in the state `on` says. */
const stands = (element, on) => element.getAttribute('aria-pressed') === String(on);

/** The volume's words as the daemon writes them, from the figures' reading (`minus 23.5 dB`). */
const volumeWords = (aria) => aria.replace(/^minus /, '−');
/** The test driver. */
export const driver = {
  async reach(target) {
    for (let attempt = 0; attempt < 5; attempt++) {
      if (await seen(target)) return true;
    }
    return false;
  },

  async operate(target, change = {}) {
    const element = find(target);
    if (!element) throw new Error(`nothing to operate for ${JSON.stringify(target)}`);
    if (typeof target === 'object' && 'control' in target) {
      const dev = bound.core.model.device();
      const kind = controlOf(target.control)?.kind;
      if (kind === 'volume') {
        if ('raw' in change) await volumeTo(change.raw);
        else tap(one(`#nd-dash [data-act="volume-${change.step < 0 ? 'down' : 'up'}"]`));
      } else if (kind === 'mute') {
        if (!('on' in change) || Boolean(dev.volume.muted) !== change.on) tap(element);
      } else if (kind === 'power' || kind === 'toggle') {
        if (!('on' in change) || !stands(element, change.on)) tap(element);
      } else if (kind === 'choice') {
        if (element.dataset.list || element.dataset.act === 'choice-list') await pick(element, change.value);
        else {
          const chips = `[data-act="${element.dataset.act}"]`
            + (element.dataset.act === 'choice' ? `[data-key="${q(target.control)}"]` : '');
          tap(one(`${chips}[data-value="${q(change.value)}"]`) ?? element);
        }
      } else if (kind === 'slider') {
        if ('raw' in change) await scaleTo(target, change.raw);
        else key(element, change.step < 0 ? 'ArrowLeft' : 'ArrowRight');
      }
    } else if (typeof target === 'object' && 'band' in target) {
      const step = change.step < 0 ? -1 : 1;
      tap(one(`[data-act="band"][data-band="${q(target.band)}"][data-param="${q(target.param)}"]`
        + `[data-step="${step}"]`));
    } else if (typeof target === 'object' && 'loudness' in target) {
      tap(one(`[data-act="loudness"][data-step="${change.step < 0 ? -1 : 1}"]`));
    } else if (typeof target === 'object' && 'slot' in target && change.save) {
      const opener = one('[data-act="slot-save-list"]');
      if (!opener) throw new Error('no word to save the bands');
      await pick(opener, target.slot);
      await settle();
      tap(one('.nd-askbox [data-answer="save"]'));
    } else {
      tap(element);
    }
    await settle();
  },

  shown(target) {
    const element = find(target);
    if (!element) return null;
    if (target === 'Cover') return element.getAttribute('src');
    if (typeof target === 'string') return element.textContent;
    if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      if (kind === 'volume') {
        const aria = one('.nd-vol svg')?.getAttribute('aria-label');
        return aria === undefined || aria === null ? null : volumeWords(aria);
      }
      if (kind === 'slider') {
        return element.closest('.nd-level')?.querySelector('.nd-val')?.textContent ?? null;
      }
      if (kind === 'choice') {
        const now = one(`${target.control === 'input' ? '#nd-dash [data-act="input"]'
          : `#nd-controls [data-act="choice"][data-key="${q(target.control)}"]`}.on`) ?? element;
        return now.textContent.replace(/ ▾$/, '');
      }
      return element.textContent.trim();
    }
    if ('band' in target) return element.value;
    if ('loudness' in target) return element.querySelector('i')?.textContent.match(/\d+/)?.[0] ?? null;
    return element.textContent.trim();
  },

  linkWords() {
    const link = bound.core.model.device()?.link;
    if (!link || link.state === 'on') return null;
    return one('.nd-left')?.textContent.trim() || null;
  },

  state() {
    const core = bound.core;
    return {
      lost: Boolean(one('#nd-card')) || one('#nightdrive')?.dataset.mood === 'lost' || core.model.state() === 'lost',
      waiting: core.model.state() === 'waiting',
      resting: core.model.state() === 'resting',
      toasts: [...document.querySelectorAll('#toasts .toast')]
        .filter((box) => !box.dataset.leaving).map((box) => box.dataset.text),
    };
  },

  perf() {
    const scene = bound.scene;
    return scene ? scene.perf() : { draws: 0, drawMs: 0, p95: 0, fps: 0, pace: 0, scale: 1, capped: false, step: 0, wet: false };
  },

  /** Step the budget down as sixty slow draws would (the wet road is dropped); false before the scene is mounted. */
  forceStepDown() {
    bound.scene?.forceStepDown();
    return Boolean(bound.scene);
  },

  /** Make the budget measure `ms` for every draw (null: the real time); false before the scene is mounted. */
  forceDrawMs(ms) {
    bound.scene?.forceDrawMs(ms);
    return Boolean(bound.scene);
  },

  sky() {
    const scene = bound.scene;
    return scene ? scene.sky() : { log: [], rays: 0, stars: [], levels: null };
  },

  look() {
    return bound.scene ? { ...bound.scene.look.read() } : null;
  },

  tune() {
    return bound.scene ? bound.scene.tune.read() : null;
  },

  mood() {
    return bound.scene ? bound.scene.mood() : null;
  },

  curveAt(x) {
    return bound.scene ? bound.scene.tune.curveAt(x) : null;
  },
};
