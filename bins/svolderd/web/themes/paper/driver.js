/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The test driver of the theme Paper, which the contract and completeness checks drive (see the
// checks' README; core/contract.js lists what they ask for). It does what a person does on Paper's
// own sheet: it scrolls to a written word or a scale, then taps the word, presses a key on the
// needle or picks an option of the paper list a word opens (`operate`), and reads the words the
// sheet shows for it (`shown`). It never calls an action of the core itself: a write the check
// sees went through Paper's own handlers.
//
//   reach(target) -> boolean            scrolls to it and says whether it is drawn
//   operate(target, change) -> void     { step: ±1 } | { raw } | { value } | { on } | { apply } |
//                                       { recall } | { save } | {}
//   shown(target) -> string | null      the words shown for a control, or for a fact by its label
//   state() -> { lost, waiting, resting, toasts }
//   linkWords() -> string | null        the words drawn beside the shown device's name that tell its link
//                                       state (null when none are drawn)

/** The core Paper was given, `null` until Paper is mounted (paper.js fills it). */
export const bound = { core: null };

/** The attribute-selector form of `text`. */
const q = (text) => CSS.escape(String(text));

/** The next frame, and a moment for the handlers a tap starts to run. */
const settle = () => new Promise((resolve) => {
  requestAnimationFrame(() => setTimeout(resolve, 30));
});

/** The device the sheet shows. */
const device = () => bound.core.model.device();

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
  const dev = device();
  if (dev?.volume && key === dev.volume.key) return { kind: 'volume' };
  if (dev?.volume && key === dev.volume.mute) return { kind: 'mute' };
  if (dev?.power && key === dev.power.key) return { kind: 'power' };
  const found = [...(dev?.controls ?? []), ...(dev?.power?.toggles ?? []),
    ...(dev?.power?.readouts ?? [])].find((control) => control.key === key);
  return found ? { kind: found.kind, control: found } : null;
}

/** The element a target is, as the sheet draws it, or nothing. */
function find(target) {
  if (target === 'Cover') return one('#p-playing .p-cover img');
  if (typeof target === 'string') return one(`td[data-fact="${q(target)}"]`);
  if ('control' in target) {
    const key = q(target.control);
    switch (controlOf(target.control)?.kind) {
      case 'volume': return one('#p-needle');
      case 'mute': return one('[data-act="mute"]');
      case 'power': return one('[data-act="power"]');
      case 'choice': return one(`[data-act="choice"][data-key="${key}"]`)
        ?? one(`[data-act="choice-list"][data-key="${key}"]`);
      case 'toggle': return one(`[data-act="toggle"][data-key="${key}"]`);
      case 'slider': return one(`.p-scale[role="slider"][data-key="${key}"]`);
      case 'readout': return one(`[data-readout="${key}"]`);
      default: return null;
    }
  }
  if ('band' in target) {
    return one(`[data-act="band"][data-band="${q(target.band)}"][data-param="${q(target.param)}"]`);
  }
  if ('mode' in target) return one('[data-act="mode-list"]');
  if ('loudness' in target) return one('[data-act="loudness"]');
  if ('preset' in target) return one(`[data-act="preset"][data-name="${q(target.preset)}"]`);
  if ('slot' in target) return one(`[data-act="slot"][data-slot="${q(target.slot)}"]`);
  if ('setting' in target) return one(`[data-act="setting"][data-name="${q(target.setting)}"]`);
  if ('transport' in target) {
    return one(`[data-act="transport"][data-button="${q(target.transport)}"]`);
  }
  return null;
}

/** A tap on `element`. */
const tap = (element) => element.click();

/** A key press on `element`, which a person has focused first. */
function key(element, name) {
  element.focus({ preventScroll: true });
  element.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }));
}

/** Open the paper list under `opener` and choose the option whose value is `value`. A viewer who
 * may not write gets no list: the tap is all there is, and the core says why. */
async function pick(opener, value) {
  tap(opener);
  await settle();
  const option = one(`.plist .p-opt[data-value="${q(value)}"]`);
  if (option) tap(option);
  else if (!device().inert) throw new Error(`no option ${value}`);
}

/** Move a level to `raw` by arrow presses on its needle or scale until it stands there, each press
 * waiting for the sheet to show the one before. */
async function toward(target, raw) {
  const read = () => {
    const element = find(target);
    if (!element) throw new Error(`nothing to move for ${JSON.stringify(target)}`);
    return Number(element.getAttribute('aria-valuenow'));
  };
  for (let n = 0; n < 60; n++) {
    const at = read();
    if (at === raw || Number.isNaN(at)) return;
    key(find(target), raw > at ? 'ArrowRight' : 'ArrowLeft');
    for (let wait = 0; wait < 40 && read() === at; wait++) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    // A press that cannot move it (the cap) is a quiet stop: the check judges by what was sent.
    if (read() === at) return;
  }
}

/** Whether a switch is in the state `on` says. */
const stands = (element, on) => element.getAttribute('aria-pressed') === String(on);

/** Open `The device` when `target` is drawn inside it folded away, as a person taps its heading. */
async function unfold(target) {
  if (!find(target)?.closest('#p-device-body[hidden]')) return;
  tap(one('[data-act="device-fold"]'));
  await settle();
}

/** Scroll to `target` and say whether it is drawn and in view; looked for again after the
 * scroll, which a section drawn again may have replaced. */
async function seen(target) {
  await unfold(target);
  find(target)?.scrollIntoView({ block: 'center' });
  await settle();
  const element = find(target);
  return Boolean(element) && taken(element) && onScreen(element);
}

export const driver = {
  async reach(target) {
    for (let attempt = 0; attempt < 4; attempt++) {
      if (await seen(target)) return true;
    }
    return false;
  },

  async operate(target, change = {}) {
    const element = find(target);
    if (!element) throw new Error(`nothing to operate for ${JSON.stringify(target)}`);
    if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      if (kind === 'volume') {
        if ('raw' in change) await toward(target, change.raw);
        else tap(one(`[data-act="volume-${change.step < 0 ? 'down' : 'up'}"]`));
      } else if (kind === 'mute') {
        if (!('on' in change) || Boolean(device().volume.muted) !== change.on) tap(element);
      } else if (kind === 'power') {
        if (!('on' in change) || !stands(element, change.on)) tap(element);
      } else if (kind === 'toggle') {
        if (!('on' in change) || !stands(element, change.on)) tap(element);
      } else if (kind === 'choice') {
        if (element.dataset.act === 'choice-list') await pick(element, change.value);
        else {
          tap(one(`[data-act="choice"][data-key="${q(target.control)}"]`
            + `[data-value="${q(change.value)}"]`) ?? element);
        }
      } else if (kind === 'slider') {
        if ('raw' in change) await toward(target, change.raw);
        else key(element, change.step < 0 ? 'ArrowLeft' : 'ArrowRight');
      }
    } else if ('band' in target) {
      const step = change.step < 0 ? -1 : 1;
      tap(one(`[data-act="band"][data-band="${q(target.band)}"][data-param="${q(target.param)}"]`
        + `[data-step="${step}"]`));
    } else if ('mode' in target) {
      await pick(element, target.mode);
    } else if ('loudness' in target) {
      tap(one(`[data-act="loudness"][data-step="${change.step < 0 ? -1 : 1}"]`));
    } else if ('slot' in target && change.save) {
      const opener = one('[data-act="slot-save-list"]');
      if (!opener) throw new Error('no word to save the bands');
      await pick(opener, target.slot);
      await settle();
      tap(one('.p-askbox [data-answer="save"]'));
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
      if (kind === 'volume') return one('#p-volume-text')?.textContent ?? null;
      if (kind === 'slider') {
        return element.closest('.p-row')?.querySelector('.p-line .p-w, .p-line .p-val')
          ?.textContent ?? null;
      }
      if (kind === 'choice') {
        const now = [...document.querySelectorAll(
          `[data-act="choice"][data-key="${q(target.control)}"].on`)][0];
        return (now ?? element).textContent;
      }
      return element.textContent.trim();
    }
    if ('band' in target) return element.closest('td').querySelector('.p-val')?.textContent ?? null;
    if ('loudness' in target) return one('.p-strength .p-val')?.textContent ?? null;
    return element.textContent.trim();
  },

  linkWords() {
    return one('#p-title .p-sub')?.textContent.trim() || null;
  },

  state() {
    const core = bound.core;
    return {
      lost: Boolean(document.querySelector('#p-card .p-card')) || core.model.state() === 'lost',
      waiting: core.model.state() === 'waiting',
      resting: core.model.state() === 'resting',
      toasts: [...document.querySelectorAll('#toasts .toast')]
        .filter((box) => !box.dataset.leaving).map((box) => box.dataset.text),
    };
  },
};
