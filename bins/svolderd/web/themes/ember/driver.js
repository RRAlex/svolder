/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The test driver of the theme Ember, which the contract and completeness checks drive (see the
// checks' README; core/contract.js lists what they ask for). It does what a person does on
// Ember's own DOM: it opens a fold or scrolls to a control (`reach`), then presses its button,
// presses a key on it or picks its value (`operate`), and reads the words the page shows for it
// (`shown`). It never calls an action of the core itself: a write the check sees went through
// Ember's own handlers.
//
//   reach(target) -> boolean            shows it, opening a fold or scrolling as a person would
//   operate(target, change) -> void     { step: ±1 } | { raw } | { value } | { on } | { apply } |
//                                       { recall } | { save } | { side } (the switch) | {}
//   shown(target) -> string | null      the words shown for a control, or for a fact by its label
//   state() -> { lost, waiting, resting, toasts }
//   linkWords() -> string | null        the words drawn beside the shown device's name that tell its link
//                                       state (null when none are drawn)

import { ember } from './bind.js';

/** The attribute-selector form of `text`. */
const q = (text) => CSS.escape(String(text));

/** The next frame, and a moment for the handlers a click starts to run. */
const settle = () => new Promise((resolve) => {
  requestAnimationFrame(() => setTimeout(resolve, 30));
});

/** The device the page shows. */
const device = () => ember.core.model.device();

/** Whether `element` takes room on the page: not hidden by a closed fold or `display: none`. */
function taken(element) {
  if (!element?.isConnected) return false;
  if (typeof element.checkVisibility === 'function' && !element.checkVisibility()) return false;
  const box = element.getBoundingClientRect();
  return box.width > 0 && box.height > 0;
}

/** Whether `element` is inside the box of the scrolling table it is in (a row under its fade is
 * not seen), or is in none. */
function unclipped(element) {
  const box = element.closest('.pscroll');
  if (!box) return true;
  const inside = element.getBoundingClientRect();
  const frame = box.getBoundingClientRect();
  return inside.top >= frame.top - 1 && inside.bottom <= frame.bottom + 1;
}

/** Whether `element` is on the screen now (at least a part of it is in the window). */
function onScreen(element) {
  const box = element.getBoundingClientRect();
  return box.bottom > 0 && box.right > 0 && box.top < innerHeight && box.left < innerWidth;
}

/** The first of `selectors` that is on the page. */
const first = (...selectors) => selectors.map((s) => document.querySelector(s)).find(Boolean)
  ?? null;

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

/** The element a computer target is, as the page draws it, or nothing. */
function findComputer(target) {
  const row = () => `#part-tone .pt[data-n="${q(target.n)}"]`;
  switch (target.computer) {
    case 'flip': return document.querySelector('#part-switch .opt[data-side="computer"]');
    case 'volume': return document.querySelector('sv-knob[data-computer] svg.dial');
    case 'mute': return document.querySelector('sv-knob[data-computer] .mute');
    case 'mode': return document.querySelector('#part-tone .opt[data-comp="mode"][data-mode="eq"]');
    case 'add': return document.querySelector('#part-tone [data-comp="add"]');
    case 'remove': return document.querySelector(`${row()} [data-comp="remove"]`);
    case 'preset': return document.querySelector(`#part-presets .pre[data-name="${q(target.name)}"]`);
    case 'point': return document.querySelector(target.param === 'type'
      ? `${row()} select[data-comp="type"]`
      : `${row()} .cell[data-param="${q(target.param)}"]`);
    default: return null;
  }
}

/** The element a target is, as the page draws it (before any fold is opened), or nothing. */
function find(target) {
  if ('computer' in target) return findComputer(target);
  const dev = device();
  if ('control' in target) {
    const key = target.control;
    const { kind } = controlOf(key) ?? {};
    switch (kind) {
      case 'volume': return document.querySelector('sv-knob svg.dial');
      case 'mute': return document.querySelector('sv-knob .mute');
      case 'power': return document.querySelector(`#part-power .power[data-key="${q(key)}"]`);
      case 'choice': return first(`.opt[data-key="${q(key)}"]`, `select[data-key="${q(key)}"]`);
      case 'toggle': return document.querySelector(`.sw[data-key="${q(key)}"]`);
      case 'slider': return document.querySelector(`sv-slider[data-key="${q(key)}"] svg`);
      case 'readout': return document.querySelector(`dl.readout[data-key="${q(key)}"]`);
      default: return null;
    }
  }
  if ('band' in target) {
    return document.querySelector(`#part-bands .band[data-band="${q(target.band)}"]`);
  }
  if ('mode' in target) return document.querySelector(`#part-tone .opt[data-mode="${q(target.mode)}"]`);
  if ('loudness' in target) return document.querySelector('#part-tone .rb');
  if ('preset' in target) {
    return document.querySelector(`#part-presets .pre[data-name="${q(target.preset)}"]`);
  }
  if ('slot' in target) {
    const slot = dev?.device_presets?.slots.find((entry) => entry.slot === target.slot);
    return slot ? document.querySelector(`#part-device-presets .slot .pre[data-name="${q(slot.name)}"]`)
      : null;
  }
  if ('setting' in target) {
    return document.querySelector(
      `#part-device-presets .prow:not(.slot) .pre[data-name="${q(target.setting)}"]`,
    );
  }
  if ('transport' in target) {
    return document.querySelector(`#part-playing .np-btn[data-press="${q(target.transport)}"]`);
  }
  return null;
}

/** The Device fold-out's details element. */
const fold = () => document.querySelector('#part-facts .device-facts');

/** The row of the Device section's list whose label is `label`, or the reading row of id
 * `label`: its value element. */
function factCell(label) {
  for (const row of document.querySelectorAll('#part-facts .facts dl.readout')) {
    if (row.querySelector('dt')?.textContent === label) return row.querySelector('dd');
  }
  for (const row of document.querySelectorAll('#part-facts .readings tbody tr')) {
    if (row.querySelector('td.id')?.textContent === label) return row.querySelector('td.val');
  }
  return null;
}

/** The cover of the track, the one fact the Now playing card shows. */
const COVER = '#part-playing .np-cover img';

/** Whether `label` names a fact, not a target. */
const isFact = (label) => typeof label === 'string';

/** A click on `element`, as a press of its button or a key's click on it. */
function press(element) {
  element.click();
}

/** A key press on `element`. */
function key(element, name) {
  element.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }));
}

/** The reading of a slider or of the volume now, from the model. */
function levelNow(target) {
  const dev = device();
  if (controlOf(target.control)?.kind === 'volume') return dev.volume.raw;
  return dev.controls.find((control) => control.key === target.control)?.raw ?? null;
}

/** Move a level to `raw` by single steps (the arrow keys of a slider, the buttons of the volume),
 * each one waiting for the page to show the one before. */
async function toward(target, raw) {
  const volume = controlOf(target.control)?.kind === 'volume';
  for (let n = 0; n < 40; n++) {
    const now = levelNow(target);
    if (now === null || now === raw) return;
    const up = raw > now;
    if (volume) press(document.querySelector(`sv-knob .step[data-step="${up ? 1 : -1}"]`));
    else key(find(target), up ? 'ArrowRight' : 'ArrowLeft');
    for (let wait = 0; wait < 40 && levelNow(target) === now; wait++) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    if (levelNow(target) === now) return;
  }
}

/** Open the band's editor, as a click on its row does; the editor's stepper of `param`. */
async function editor(band, param) {
  const row = find({ band });
  const box = document.querySelector('#part-bands .bedit');
  if (box?.hidden !== false || box.dataset.band !== String(band)) {
    press(row);
    await settle();
  }
  return document.querySelector(`#part-bands .bedit .stp[data-param="${q(param)}"]`);
}

/** Type `value` into a band's value field and press Enter. */
async function typeBand(stepper, value) {
  press(stepper.querySelector('.v'));
  await settle();
  const input = stepper.querySelector('.v input');
  input.value = value;
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
}

/** Bring `target` (a control or a fact) into view and say whether it is there to be seen: the
 * element is looked for again after the scroll, which a redrawn part may have replaced. */
async function seen(target) {
  if (target === 'Cover') {
    document.querySelector(COVER)?.scrollIntoView({ block: 'center' });
    await settle();
    const img = document.querySelector(COVER);
    return Boolean(img) && taken(img) && onScreen(img);
  }
  if (isFact(target)) {
    const details = fold();
    if (!details) return false;
    if (!details.open) {
      press(details.querySelector('summary'));
      await settle();
    }
    factCell(target)?.scrollIntoView({ block: 'center' });
    await settle();
    const cell = factCell(target);
    return Boolean(cell) && taken(cell) && onScreen(cell);
  }
  if ('band' in target) {
    // The band's row shows the band; a person opens its editor to see the parameter.
    find(target)?.scrollIntoView({ block: 'center' });
    await settle();
    if (!find(target) || !taken(find(target)) || !onScreen(find(target))) return false;
    await editor(target.band, target.param);
    const stepper = document.querySelector(
      `#part-bands .bedit .stp[data-param="${q(target.param)}"]`,
    );
    stepper?.scrollIntoView({ block: 'center' });
    await settle();
    const now = document.querySelector(`#part-bands .bedit .stp[data-param="${q(target.param)}"]`)
      ?? find(target);
    return taken(now) && onScreen(now);
  }
  find(target)?.scrollIntoView({ block: 'center' });
  await settle();
  const element = find(target);
  return Boolean(element) && taken(element) && onScreen(element) && unclipped(element);
}

/** The words that say a loudness stepper's direction. */
const LOUDNESS = { '-1': 'Lower the loudness', 1: 'Raise the loudness' };

/** The computer's volume now, from the model. */
const computerPct = () => ember.core.computer.volumeOf(ember.core.model.page())?.pct ?? null;

/** Operate one of the computer's targets as a person does: a press, or a value typed into a cell. */
async function operateComputer(target, change) {
  const { computer } = target;
  if (computer === 'flip') {
    const button = document.querySelector(`#part-switch .opt[data-side="${q(change.side)}"]`);
    if (!button) throw new Error('no switch');
    press(button);
    await settle();
    return;
  }
  const element = find(target);
  if (!element) throw new Error(`nothing to operate for ${JSON.stringify(target)}`);
  if (computer === 'volume') {
    if ('raw' in change) {
      for (let n = 0; n < 120 && computerPct() !== null && computerPct() !== change.raw; n++) {
        const before = computerPct();
        press(document.querySelector(`sv-knob[data-computer] .step[data-step="${change.raw > before ? 1 : -1}"]`));
        for (let wait = 0; wait < 40 && computerPct() === before; wait++) {
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
      }
    } else {
      const direction = change.step < 0 ? -1 : 1;
      press(document.querySelector(`sv-knob[data-computer] .step[data-step="${direction}"]`));
    }
  } else if (computer === 'mute') {
    if (!('on' in change) || element.getAttribute('aria-pressed') !== String(change.on)) press(element);
  } else if (computer === 'mode') {
    const want = 'on' in change ? (change.on ? 'eq' : 'off') : 'eq';
    press(document.querySelector(`#part-tone .opt[data-comp="mode"][data-mode="${want}"]`));
  } else if (computer === 'point' && element.matches('select')) {
    element.value = change.value;
    element.dispatchEvent(new Event('change', { bubbles: true }));
  } else if (computer === 'point') {
    if (!('value' in change)) throw new Error('a point is changed by typing a value');
    press(element);
    await settle();
    const input = element.querySelector('input');
    input.value = change.value;
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  } else {
    press(element);
  }
  await settle();
}

/** The words the page shows for one of the computer's targets. */
function shownComputer(target, element) {
  switch (target.computer) {
    case 'flip': return element.parentElement.querySelector('[aria-pressed="true"]')?.textContent ?? null;
    case 'volume': {
      const knob = element.closest('sv-knob');
      const num = knob.querySelector('.big .num')?.textContent ?? '';
      const unit = knob.querySelector('.big .unit')?.textContent;
      return unit ? [num, unit].join(' ') : num;
    }
    case 'mute': return element.textContent.trim();
    case 'mode': return element.parentElement.parentElement.querySelector('[aria-pressed="true"]')?.textContent ?? null;
    case 'add': case 'remove': return element.textContent.trim();
    case 'preset': return element.dataset.name;
    default: return element.matches('select') ? element.value : element.textContent.trim();
  }
}

export const driver = {
  async reach(target) {
    // A part the stream redraws while it is scrolled to is looked for again, as a person would.
    for (let attempt = 0; attempt < 4; attempt++) {
      if (await seen(target)) return true;
    }
    return false;
  },

  async operate(target, change = {}) {
    if ('computer' in target) {
      await operateComputer(target, change);
    } else if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      const element = find(target);
      if (!element) throw new Error(`no control ${target.control}`);
      if (kind === 'volume') {
        if ('raw' in change) await toward(target, change.raw);
        else press(document.querySelector(`sv-knob .step[data-step="${change.step < 0 ? -1 : 1}"]`));
      } else if (kind === 'mute') {
        if (!('on' in change) || element.getAttribute('aria-pressed') !== String(change.on)) press(element);
      } else if (kind === 'power') {
        if (!('on' in change) || element.getAttribute('aria-pressed') !== String(change.on)) press(element);
      } else if (kind === 'toggle') {
        if (!('on' in change) || element.getAttribute('aria-checked') !== String(change.on)) press(element);
      } else if (kind === 'choice') {
        const list = element.matches('select');
        if (list) {
          element.value = change.value;
          element.dispatchEvent(new Event('change', { bubbles: true }));
        } else {
          const option = document.querySelector(
            `.opt[data-key="${q(target.control)}"][data-value="${q(change.value)}"]`,
          );
          press(option ?? element);
        }
      } else if (kind === 'slider') {
        if ('raw' in change) await toward(target, change.raw);
        else key(element, change.step < 0 ? 'ArrowLeft' : 'ArrowRight');
      }
    } else if ('band' in target) {
      const stepper = await editor(target.band, target.param);
      if (!stepper) {
        // No editor opens (a viewer who may not change the band): the row says so itself.
        press(find(target));
      } else if ('value' in change) {
        await typeBand(stepper, change.value);
      } else {
        press(stepper.querySelectorAll('button.btn')[change.step < 0 ? 0 : 1]);
      }
    } else if ('loudness' in target) {
      press(document.querySelector(`#part-tone .rb[aria-label="${LOUDNESS[change.step < 0 ? -1 : 1]}"]`));
    } else if ('slot' in target && change.save) {
      const slot = device().device_presets.slots.find((entry) => entry.slot === target.slot);
      const opener = document.querySelector(
        `#part-device-presets [aria-label="Save the bands to ${q(slot.name)}"]`,
      );
      if (opener.getAttribute('aria-disabled') === 'true') {
        press(opener);
      } else {
        press(opener);
        await settle();
        const form = opener.closest('.prow').nextElementSibling;
        press(form.querySelector('button.danger'));
      }
    } else {
      const element = find(target);
      if (!element) throw new Error(`nothing to press for ${JSON.stringify(target)}`);
      press(element);
    }
    await settle();
  },

  linkWords() {
    const word = document.querySelector('#part-link .lw');
    if (!word || getComputedStyle(word).display === 'none' || word.getBoundingClientRect().width < 8) return null;
    return word.textContent.trim();
  },

  shown(target) {
    if (target === 'Cover') return document.querySelector(COVER)?.getAttribute('src') ?? null;
    if (isFact(target)) return factCell(target)?.textContent ?? null;
    const element = find(target);
    if (!element) return null;
    if ('computer' in target) return shownComputer(target, element);
    if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      if (kind === 'volume') {
        const num = element.closest('sv-knob').querySelector('.big .num')?.textContent ?? '';
        const unit = element.closest('sv-knob').querySelector('.big .unit')?.textContent;
        return unit ? `${num} ${unit}` : num;
      }
      if (kind === 'mute') return element.textContent.trim();
      if (kind === 'power') return element.closest('#part-power').querySelector('.pstate')?.textContent ?? null;
      if (kind === 'readout') return element.querySelector('dd')?.textContent ?? null;
      if (kind === 'toggle') return element.querySelector('span')?.textContent ?? null;
      if (kind === 'slider') return element.closest('sv-slider').querySelector('.val')?.textContent ?? null;
      if (kind === 'choice') {
        if (element.matches('select')) return element.selectedOptions[0]?.textContent ?? null;
        return document.querySelector(`.opt[data-key="${q(target.control)}"][aria-pressed="true"]`)
          ?.dataset.value ?? null;
      }
    }
    if ('band' in target) {
      const column = { f: 1, g: 2, q: 3 }[target.param];
      return element.children[column]?.textContent ?? null;
    }
    if ('loudness' in target) {
      return document.querySelector('#part-tone #loud-strength .n')?.textContent ?? null;
    }
    if ('mode' in target || 'transport' in target) {
      return element.getAttribute('aria-label') ?? element.textContent.replace(/\s+/g, ' ').trim();
    }
    return element.dataset.name ?? null;
  },

  state() {
    const core = ember.core;
    return {
      lost: Boolean(document.querySelector('#screen .view .card.warn'))
        || core.model.state() === 'lost',
      waiting: core.model.state() === 'waiting',
      resting: core.model.state() === 'resting',
      toasts: [...document.querySelectorAll('#toasts .toast')]
        .filter((box) => !box.dataset.leaving).map((box) => box.dataset.text),
    };
  },
};
