/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The faults an amp reports, each with its advice, and the foot of the sheet, *Device settings…*
// (the device's facts and readings are the Devices popup's page, drawn by the core). Pure: the model
// in, markup out.

import { SHEET } from '../words.js';
import { esc } from './esc.js';
import { button } from './modes.js';

/** The faults the device reports: the facts' rows of the fault family (ids `230.…`: over-current,
 * over-temperature) with their value, and the advice of the card's faults. Nothing for none. */
export function drawFaults(device) {
  const rows = (device?.facts?.rows ?? []).filter((r) => /^230\./.test(r.id));
  const advice = device?.card?.faults ?? [];
  if (rows.length === 0 && advice.length === 0) return '';
  const list = rows.map((r) => `<dt>${esc(r.label)}</dt><dd data-fact="${esc(r.label)}"`
    + `${r.value === 'active' ? ' class="bad"' : ''}>${esc(r.value)}</dd>`).join('');
  const tips = advice.map((x) => `<li><b>${esc(x.label)}</b> ${esc(x.advice)}</li>`).join('');
  return `<h2 class="nd-lbl bad">${esc(SHEET.faults)}</h2>${list ? `<dl class="nd-faults">${list}</dl>` : ''}`
    + `${tips ? `<ul class="nd-advice">${tips}</ul>` : ''}`;
}

/** The foot of the sheet: *Device settings…*, which opens the device's page in the Devices popup,
 * where the core draws the device's facts, its readings and, for a writer, *Identify*. A viewer
 * who may not write (`inert`: a guest, a page whose writes are off) is offered *Devices…*: the
 * same page, read-only; it is the way to the device list on this theme. */
export function drawFoot(device, inert = Boolean(device?.inert)) {
  if (!device?.url) return '';
  const word = inert ? SHEET.deviceFacts : SHEET.deviceSettings;
  return `<div class="nd-set">${button('device-settings', word, false,
    ` data-device="${esc(device.url)}"`, 'nd-btn')}</div>`;
}
