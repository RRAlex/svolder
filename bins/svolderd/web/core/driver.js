/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The core's test driver for the device's facts (`deviceFactsOf`, contract.js): the checks ask it
// to reach a fact of a device and to read the words the page shows for it. It is test surface, as
// a theme's `driver` is: the checks call it (completeness.py, through `core.driver`); no theme and
// no module of the core does, and nothing a person uses goes through it.
//
// The facts are drawn by the core, on the device's page of the Devices popup (popups/devices/),
// whichever theme is mounted, so the driver does what a person does there: it opens the popup at
// that device's page through the router (or steps it there), and for a reading row it opens the
// All readings disclosure. It reads the words off the page's cells, which carry the fact's label
// (`data-fact`) or the reading's id (`data-reading`).
//
//   reach(label, { device }) -> Promise<boolean>   brings the fact into view; false when refused
//   shown(label, { device }) -> string | null      the words shown for it; null when refused
//
// `device` is the url of the device whose fact is meant, named by the caller at every call: the
// driver never takes the device on show for a caller that named none. Refused, without a throw:
// no device named, a device the page does not hold, a label that is not one of that device's facts
// (a theme's fact such as Cover, a target, an unknown word), and a viewer who may not have the
// popup.

import { computerTargetsOf, deviceFactsOf, targetsOf } from './contract.js';
import { CONTROLS, DEVICES, devicePage } from './popups/listed.js';

/** How long a thing the popup draws is waited for, and how often it is looked for again (ms). */
const WAIT_MS = 3000;
const EVERY_MS = 25;

/** `text` as the value of an attribute in a selector. */
const quoted = (text) => `"${String(text).replace(/["\\]/g, '\\$&')}"`;

/**
 * The driver over `store` (the page's model: `device(url)`), `router` and `document`, which holds
 * the popup once it is open.
 */
export function createDriver({ store, router, document }) {
  const view = document.defaultView;
  const controls = createControlsDriver({ store, router, document });

  /** The url a question names, when it is a fact of that device's: else nothing. */
  function asked(label, options) {
    const url = options?.device;
    if (!url) return null;
    // the list holds labels only, so a target or anything else that is no label is not in it
    return deviceFactsOf(store.device(url)).includes(label) ? url : null;
  }

  /** The device's block on the popup's page, when the page is that device's. */
  const block = (url) => document.querySelector(`.fpd-device[data-device=${quoted(url)}]`);

  /** The cell that shows `label` on the device's page: a fact's, or a reading's value. */
  function cellOf(label, url) {
    const on = block(url);
    if (!on) return null;
    return on.querySelector(`dd[data-fact=${quoted(label)}]`)
      ?? on.querySelector(`tr[data-reading=${quoted(label)}] td`);
  }

  /** The first truthy answer of `look()` within the wait, looked for again as it goes. */
  async function until(look) {
    const end = Date.now() + WAIT_MS;
    let got = look();
    while (!got && Date.now() < end) {
      await new Promise((resolve) => view.setTimeout(resolve, EVERY_MS));
      got = look();
    }
    return got;
  }

  return {
    async reach(label, options) {
      const url = asked(label, options);
      if (!url) return false;
      const page = devicePage(url);
      const at = router.state();
      if (at.popup !== DEVICES) router.open(DEVICES, page);
      else if (at.page !== page) router.step(page);
      // a viewer who may not have the popup is turned away
      if (router.state().popup !== DEVICES) return false;
      const on = await until(() => block(url));
      if (!on) return false;
      const reading = (store.device(url)?.facts?.rows ?? []).some((row) => row.id === label);
      if (reading) {
        // a reading is in the All readings table, which is empty until a person opens it
        const details = await until(() => on.querySelector('details.fpd-readings'));
        if (!details) return false;
        details.open = true;
      }
      const cell = await until(() => cellOf(label, url));
      if (!cell) return false;
      cell.scrollIntoView?.({ block: 'center' });
      // in view once scrolled to: it has a box, and the box is inside the window
      return Boolean(await until(() => {
        const box = cell.getBoundingClientRect();
        return box.width > 0 && box.height > 0 && box.bottom > 0 && box.top < view.innerHeight;
      }));
    },

    shown(label, options) {
      const url = asked(label, options);
      if (!url) return null;
      return cellOf(label, url)?.textContent ?? null;
    },

    controls,
  };
}

/** Whether two targets are the same one: the same members with the same values. */
function same(a, b) {
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((key) => a[key] === b[key]);
}

/** The test driver of the Device controls popup (see the top of the file), over `store`, `router`
 * and `document`. */
function createControlsDriver({ store, router, document }) {
  const view = document.defaultView;
  const popupId = 'popup-controls';
  const settle = () => new Promise((resolve) => view.setTimeout(resolve, 30));

  /** The first truthy answer of `look()` within the wait, looked for again as it goes. */
  async function until(look) {
    const end = Date.now() + WAIT_MS;
    let got = look();
    while (!got && Date.now() < end) {
      await new Promise((resolve) => view.setTimeout(resolve, EVERY_MS));
      got = look();
    }
    return got;
  }

  /** The popup's box once its page is drawn, else nothing (not open, or still loading). */
  const popup = () => {
    const box = document.getElementById(popupId);
    return box?.querySelector('#ctl-body') ? box : null;
  };

  /** The side a target needs the popup on: `null` for the switch itself, which both sides draw. */
  const sideOf = (target) => (target.computer === 'flip' ? null : ('computer' in target ? 'computer' : 'dac'));

  /** Whether the page lists `target` for the shown device or its computer. */
  function listed(target) {
    if (!target || typeof target !== 'object' || !store.device(store.shown())) return false;
    const list = 'computer' in target ? computerTargetsOf(store.page())
      : targetsOf(store.device(store.shown()));
    return list.some((one) => same(one, target));
  }

  /** What a control key stands for in the shown device's model. */
  function controlOf(key) {
    const device = store.device(store.shown());
    if (device?.volume && key === device.volume.key) return { kind: 'volume' };
    if (device?.volume && key === device.volume.mute) return { kind: 'mute' };
    if (device?.power && key === device.power.key) return { kind: 'power' };
    const toggles = device?.power?.toggles ?? [];
    const found = [...(device?.controls ?? []), ...toggles, ...(device?.power?.readouts ?? [])]
      .find((control) => control.key === key);
    return found ? { kind: found.kind, power: toggles.includes(found) } : null;
  }

  /** The element of `target` as the popup draws it, or nothing. */
  function find(target) {
    const root = popup();
    if (!root) return null;
    const one = (selector) => root.querySelector(selector);
    const act = (name, extra = '') => one(`[data-act="${name}"]${extra}`);
    if ('computer' in target) {
      switch (target.computer) {
        case 'flip': return act('side');
        case 'volume': return act('c-volume-up');
        case 'mute': return act('c-mute');
        case 'mode': return act('c-mode', '[data-mode="eq"]');
        case 'bypass': return act('c-bypass');
        case 'add': return act('c-add');
        case 'remove': return act('c-remove', `[data-n="${target.n}"]`);
        case 'point': return act('c-point', `[data-n="${target.n}"][data-param=${quoted(target.param)}]`);
        case 'preset': return act('c-preset', `[data-name=${quoted(target.name)}]`);
        default: return null;
      }
    }
    if ('control' in target) {
      const key = quoted(target.control);
      const kind = controlOf(target.control);
      switch (kind?.kind) {
        case 'volume': return act('volume-up');
        case 'mute': return act('mute');
        case 'power': return act('power');
        case 'choice': return act('choice', `[data-key=${key}]`);
        case 'toggle': return kind.power ? act('power-toggle', `[data-key=${key}]`)
          : act('toggle', `[data-key=${key}]`);
        case 'slider': return act('slider', `[data-key=${key}][data-step="1"]`);
        case 'readout': return one(`[data-readout=${key}]`);
        default: return null;
      }
    }
    if ('band' in target) {
      return act('band', `[data-band=${quoted(target.band)}][data-param=${quoted(target.param)}][data-step="1"]`);
    }
    if ('mode' in target) return act('mode', `[data-mode=${quoted(target.mode)}]`);
    if ('loudness' in target) return act('loudness', '[data-step="1"]');
    if ('preset' in target) return act('preset', `[data-name=${quoted(target.preset)}]`);
    if ('slot' in target) return act('slot', `[data-slot=${quoted(target.slot)}]`);
    if ('setting' in target) return act('setting', `[data-name=${quoted(target.setting)}]`);
    if ('transport' in target) return act('transport', `[data-button=${quoted(target.transport)}]`);
    return null;
  }

  /** The side the popup shows now. */
  const sideNow = () => popup()?.querySelector('[data-act="side"][aria-pressed="true"]')?.dataset.side ?? 'dac';

  /** Whether `el` takes room in the window. */
  function seen(el) {
    const box = el.getBoundingClientRect();
    return box.width > 0 && box.height > 0 && box.bottom > 0 && box.top < view.innerHeight;
  }

  async function reach(target) {
    if (!listed(target)) return false;
    if (router.state().popup !== CONTROLS) router.open(CONTROLS);
    // a viewer who may not have the popup is turned away
    if (router.state().popup !== CONTROLS) return false;
    if (!(await until(popup))) return false;
    const side = sideOf(target);
    if (side && sideNow() !== side) {
      popup().querySelector(`[data-act="side"][data-side="${side}"]`)?.click();
      await settle();
    }
    const el = await until(() => find(target));
    if (!el) return false;
    el.scrollIntoView?.({ block: 'center' });
    return Boolean(await until(() => seen(el)));
  }

  /** The words shown for an element: a switch is `on` or `off`. */
  function wordsOf(target, el) {
    const root = popup();
    const text = (selector) => root.querySelector(selector)?.textContent ?? null;
    if ('computer' in target) {
      switch (target.computer) {
        case 'flip': return root.querySelector('[data-act="side"][aria-pressed="true"]')?.textContent ?? null;
        case 'volume': return text('[data-shown="c-volume"]');
        case 'mute': case 'bypass': return el.getAttribute('aria-pressed') === 'true' ? 'on' : 'off';
        case 'mode': return root.querySelector('[data-act="c-mode"][aria-pressed="true"]')?.textContent ?? null;
        case 'point': return el.value;
        default: return el.textContent.trim();
      }
    }
    if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      if (kind === 'volume') return text('[data-shown="volume"]');
      if (kind === 'mute' || kind === 'power' || kind === 'toggle') {
        return el.getAttribute('aria-pressed') === 'true' ? 'on' : 'off';
      }
      if (kind === 'slider') return text(`[data-shown=${quoted(target.control)}]`);
      if (kind === 'choice') {
        return root.querySelector(`[data-act="choice"][data-key=${quoted(target.control)}][aria-pressed="true"]`)
          ?.textContent ?? null;
      }
      // a readout is a row of a label and its value: the words are the value's
      if (kind === 'readout') return el.querySelector('.fp-row-value')?.textContent.trim() ?? null;
      return el.textContent.trim();
    }
    if ('band' in target) {
      return text(`[data-cell="band"][data-band=${quoted(target.band)}][data-param=${quoted(target.param)}]`);
    }
    if ('mode' in target) return root.querySelector('[data-act="mode"][aria-pressed="true"]')?.textContent ?? null;
    if ('loudness' in target) return text('[data-shown="loudness"]');
    return el.textContent.trim();
  }

  /** Step the volume to `raw` by taps on its keys, each waiting for the model to show the one before. */
  async function volumeTo(read, up, down, raw) {
    for (let n = 0; n < 140; n++) {
      const at = read();
      if (at === raw || typeof at !== 'number') return;
      (raw > at ? up : down)().click();
      let waited = 0;
      while (read() === at && waited < 40) {
        await new Promise((resolve) => view.setTimeout(resolve, 25));
        waited += 1;
      }
      // a step that cannot move it (the cap) is a quiet stop
      if (read() === at) return;
    }
  }

  async function operate(target, how = {}) {
    if (!(await reach(target))) return false;
    const root = popup();
    const el = find(target);
    if (!el) return false;
    const url = store.shown();
    const click = async (one) => {
      one.click();
      await settle();
    };
    const pressed = (one) => one.getAttribute('aria-pressed') === 'true';
    const byAct = (name, extra = '') => () => root.querySelector(`[data-act="${name}"]${extra}`);
    if ('computer' in target) {
      const { computer } = target;
      if (computer === 'flip') {
        const key = root.querySelector(`[data-act="side"][data-side="${how.side}"]`);
        if (!key) return false;
        await click(key);
      } else if (computer === 'volume') {
        const read = () => store.page().host?.computer?.volume?.pct ?? null;
        if ('raw' in how) await volumeTo(read, byAct('c-volume-up'), byAct('c-volume-down'), how.raw);
        else await click(byAct(how.step < 0 ? 'c-volume-down' : 'c-volume-up')());
      } else if (computer === 'mute' || computer === 'bypass') {
        if (!('on' in how) || pressed(el) !== how.on) await click(el);
      } else if (computer === 'mode') {
        await click(root.querySelector(`[data-act="c-mode"][data-mode="${'on' in how && !how.on ? 'off' : 'eq'}"]`));
      } else if (computer === 'point' && el.matches('select')) {
        el.value = how.value;
        el.dispatchEvent(new view.Event('change', { bubbles: true }));
        await settle();
      } else if (computer === 'point') {
        if (!('value' in how)) return false;
        el.focus({ preventScroll: true });
        el.value = how.value;
        el.dispatchEvent(new view.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        await settle();
      } else await click(el);
      return true;
    }
    if ('control' in target) {
      const kind = controlOf(target.control)?.kind;
      const device = store.device(url);
      if (kind === 'volume') {
        const read = () => store.device(url)?.volume?.raw;
        if ('raw' in how) await volumeTo(read, byAct('volume-up'), byAct('volume-down'), how.raw);
        else await click(byAct(how.step < 0 ? 'volume-down' : 'volume-up')());
      } else if (kind === 'mute') {
        if (!('on' in how) || Boolean(device.volume.muted) !== how.on) await click(el);
      } else if (kind === 'power' || kind === 'toggle') {
        if (!('on' in how) || pressed(el) !== how.on) await click(el);
      } else if (kind === 'choice') {
        const pill = root.querySelector(`[data-act="choice"][data-key=${quoted(target.control)}][data-value=${quoted(how.value)}]`);
        if (!pill) return false;
        await click(pill);
      } else if (kind === 'slider') {
        const key = target.control;
        const read = () => (device && store.device(url)?.controls.find((c) => c.key === key)?.raw);
        const slide = (dir) => root.querySelector(`[data-act="slider"][data-key=${quoted(key)}][data-step="${dir}"]`);
        if ('raw' in how) await volumeTo(read, () => slide(1), () => slide(-1), how.raw);
        else await click(slide(how.step < 0 ? -1 : 1));
      }
      return true;
    }
    if ('band' in target) {
      await click(root.querySelector(`[data-act="band"][data-band=${quoted(target.band)}][data-param=${quoted(target.param)}][data-step="${how.step < 0 ? -1 : 1}"]`));
    } else if ('loudness' in target) {
      await click(root.querySelector(`[data-act="loudness"][data-step="${how.step < 0 ? -1 : 1}"]`));
    } else if ('slot' in target && how.save) {
      const save = root.querySelector(`[data-act="slot-save"][data-slot=${quoted(target.slot)}]`);
      if (!save) return false;
      await click(save);
      const yes = await until(() => root.querySelector('.fi-askbox [data-answer="save"]'));
      if (!yes) return false;
      await click(yes);
    } else {
      await click(el);
    }
    return true;
  }

  return {
    reach,
    operate,
    shown(target) {
      if (!listed(target)) return null;
      const el = find(target);
      return el ? wordsOf(target, el) : null;
    },
    state(target) {
      if (!listed(target)) return null;
      const el = find(target);
      if (!el) return null;
      const press = el.getAttribute('aria-pressed');
      return {
        pressed: press === null ? null : press === 'true',
        inert: el.getAttribute('aria-disabled') === 'true',
        said: document.getElementById('ctl-say')?.textContent.trim() || null,
        toasts: [...document.querySelectorAll('#toasts .toast')]
          .filter((box) => !box.dataset.leaving).map((box) => box.dataset.text),
      };
    },
  };
}
