/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The foot of the shelf: *Device settings…* (*Devices…* for a viewer who may not write), which
// opens the device's page in the Devices popup, where the core draws the device's facts, its
// readings and, for a writer, *Identify*. Pure: the model in, markup out.

import { SHELF } from '../words.js';
import { esc } from './esc.js';
import { button, inertPage } from './word.js';

/** The foot for `device`: *Device settings…*, or *Devices…* when the page may not write. */
export function facts(page, device) {
  if (!device?.url) return '';
  const word = inertPage(page) ? SHELF.deviceFacts : SHELF.deviceSettings;
  return `<div class="sg-set">${button('device-settings', word, false,
    ` data-device="${esc(device.url)}"`, 'sg-btn sg-pill')}</div>`;
}
