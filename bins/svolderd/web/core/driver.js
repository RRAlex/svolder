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

import { deviceFactsOf } from './contract.js';
import { DEVICES, devicePage } from './popups/listed.js';

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
  };
}
