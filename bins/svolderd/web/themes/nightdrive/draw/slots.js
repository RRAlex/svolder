/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The device's own presets: the slots it holds, to recall or write over (the write asks first), and the
// settings it has built in. Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { esc } from './esc.js';
import { button, chip } from './modes.js';

/** The slots and built-in settings of `device`; nothing for a device with none. `view.inert` (a guest, a
 * page whose writes are off) draws the words quiet. */
export function drawSlots(device, view = {}) {
  const d = device?.device_presets;
  if (!d) return '';
  const inert = Boolean(view.inert ?? device.inert);
  const quiet = inert || d.recall === false;
  const slots = (d.slots ?? []).map((s) => button('slot', s.name, quiet,
    ` id="nd-sl-${esc(s.slot)}" data-slot="${esc(s.slot)}"`, 'nd-chip')).join('');
  const save = d.save && !inert ? `<div class="nd-set">${chip('slot-save-list', SHEET.saveBands, false, ' id="nd-slot-save"')}</div>` : '';
  const built = (d.builtins ?? []).map((b, i) => chip('setting', b.name, quiet,
    ` id="nd-st-${i}" data-name="${esc(b.name)}"`, b.on === true)).join('');
  return `<h2 class="nd-lbl">${esc(SHEET.onDevice)}</h2><div class="nd-set">${slots}</div>${save}`
    + `<h2 class="nd-lbl">${esc(SHEET.builtIn)}</h2><div class="nd-set">${built}</div><div class="nd-ask"></div>`;
}
