/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hand on the foot of the shelf: *Device settings…* (*Devices…* for a reader) opens the
// device's page in the Devices popup (the facts and the readings are that page's, Identify too for
// a writer). It is a person's tap, so it goes through the router as a gesture.

/** Wire the shelf's foot in `root` until `signal` aborts. */
export function wireFacts({ root, core, signal }) {
  root.querySelector('#sg-shelf').addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act="device-settings"]');
    if (el?.dataset.device) core.popups.openDevice(el.dataset.device);
  }, { signal });
}
