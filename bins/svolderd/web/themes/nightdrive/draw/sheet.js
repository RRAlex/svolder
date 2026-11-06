/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The neon sheet: the rest of the device over the dimmed scene. The sections are drawn in the order
// the person settled on: Mode and Input across the top, the band table beside the presets (the
// device's slots and controls come under them), the faults an amp reports across the top and the
// device's facts, folded, across the bottom. On a phone they run in one column, the presets before the
// bands. A device draws only what its model has. Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { bandTable } from './bands.js';
import { inputRow, modeRow } from './modes.js';
import { inertOf } from '../inert.js';
import { presetBox } from './presets.js';
import { drawControls } from './controls.js';
import { drawFacts, drawFaults } from './facts.js';
import { drawMeters } from './meters.js';
import { drawSlots } from './slots.js';

/** A section of the sheet, left out when it has nothing to say. */
const section = (id, html) => (html ? `<section id="${id}" class="nd-sec">${html}</section>` : '');

/** The sheet's sections for `page`'s device; `view.facts.open` says whether its facts are unfolded. */
export function drawSheet(page, view = {}) {
  const device = page?.device;
  if (!device) return '';
  const inert = inertOf(page);
  const own = { inert };
  return `${section('nd-faults', drawFaults(device))}`
    + `${section('nd-mode', modeRow(device, inert))}${section('nd-input', inputRow(device, inert))}`
    + `${section('nd-bands', bandTable(device, inert))}`
    + `<div id="nd-side" class="nd-side">${section('nd-presets', presetBox(device, inert))}`
    + `${section('nd-slots', drawSlots(device, own))}${section('nd-controls', drawControls(device, own))}${section('nd-meters', drawMeters(device))}</div>`
    + `${section('nd-facts', drawFacts(device, Boolean(view.facts?.open), inert))}`;
}

/** The sheet's frame: the close key and the body the sections are drawn into. */
export function sheetFrame() {
  return `<div id="nd-dialog" class="nd-sheet" role="dialog" aria-modal="true" aria-label="${SHEET.label}" tabindex="-1">`
    + `<button type="button" id="nd-sheet-x" class="nd-x" data-act="sheet-close" aria-label="${SHEET.close}">✕</button>`
    + '<div id="nd-sheet-body" class="nd-sheet-body"></div></div>';
}
