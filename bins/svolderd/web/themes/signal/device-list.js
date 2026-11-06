/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The list of the devices svolder drives, opened by the name of the shown device on the line (as
// the frame's Device row does): a pick shows that device through the core. Nothing to open for a
// fleet of one.

import { openList } from './lists.js';
import { WORDS } from './words.js';

/** Open the list under `opener` (the shown device's hop on the line); returns its handle
 * (`{ close }`), or null when there is nothing to list (a fleet of one). */
export function openDevices({ opener, core, signal }) {
  const page = core.model.page();
  const devices = page.fleet?.devices ?? [];
  if (devices.length < 2) return null;
  return openList({
    opener,
    label: WORDS.devices,
    options: devices.map((device) => ({ value: device.url, text: device.name, on: device.url === page.shown })),
    choose(url) {
      if (url !== core.model.shown()) core.actions.showDevice(url);
    },
    signal,
  });
}
