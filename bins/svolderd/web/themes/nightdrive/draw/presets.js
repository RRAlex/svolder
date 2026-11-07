/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone presets of the sheet: a chip for each (the person's own in italics), what is on, and the
// ways to save, rename or delete, which ask in a place the section keeps (ask.js). Presets made by
// AutoEq are chips like the others; one the computer's EQ cannot play is quiet, with its reason as its title. Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { button, chip } from './modes.js';
import { esc } from './esc.js';

/** The presets of `device`; nothing for a device with none. */
export function presetBox(device, inert = false) {
  const p = device?.presets;
  if (!p) return '';
  const quiet = inert || p.recall === false;
  const { status } = p;
  const name = status?.name === null || status?.name === undefined ? '' : `<i>${esc(status.name)}</i>`;
  const rows = p.rows ?? [];
  const chips = rows.map((r, i) => button('preset', r.name, quiet || r.playable === false,
    ` id="nd-pr-${i}" data-name="${esc(r.name)}" aria-pressed="${r.on === true}"${r.why ? ` title="${esc(r.why)}"` : ''}`,
    `nd-chip${r.on ? ' on' : ''}${r.built_in ? '' : ' own'}`)).join('');
  const own = rows.some((r) => !r.built_in);
  const writes = p.editable && !inert
    ? chip('preset-save', SHEET.saveAs, false, ' id="nd-preset-save"')
      + (own ? chip('preset-rename', SHEET.rename, false, ' id="nd-preset-rename"')
        + chip('preset-delete', SHEET.remove, false, ' id="nd-preset-delete"') : '')
    : '';
  return `<h2 class="nd-lbl">${esc(SHEET.presets)}</h2><div class="nd-set">${chips}</div>`
    + `<p class="nd-status">${esc(status?.lead ?? '')}${name}${esc(status?.tail ?? '')}</p>`
    + `${writes ? `<div class="nd-set">${writes}</div>` : ''}`
    + `${p.note ? `<p class="nd-note">${esc(p.note)}</p>` : ''}<div class="nd-ask"></div>`;
}
