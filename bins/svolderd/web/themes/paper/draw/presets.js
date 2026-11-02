/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The presets sections. The tone presets: the line that says which is on, the names to apply and,
// where the configuration may be written, `save as…` and `rename or delete…`. The device's own:
// its slots to recall, `save the bands to…` where the device allows it, and its built-in
// settings. A form or a question for a word opens inside the section's `.p-ask`. Pure.

import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

/** The names of the words of one kind joined by dots. */
const joined = (words) => words.join(' &middot; ');

/** The tone presets section; nothing for a device without them. */
export function presets(_page, device, _view) {
  const p = device?.presets;
  if (!p) return '';
  const inert = Boolean(device.inert);
  const quiet = inert || p.recall === false;
  const { status } = p;
  const name = status?.name === null || status?.name === undefined ? ''
    : `<i>${esc(status.name)}</i>`;
  const names = (p.rows ?? []).map((r, i) => word('preset', r.name, quiet,
    ` id="p-pr-${i}" data-name="${esc(r.name)}"`, `${r.on ? 'on' : ''}${r.built_in ? '' : ' own'}`.trim()));
  const writes = [];
  if (p.editable && !inert) {
    writes.push(word('preset-save', WORDS.saveAs, false));
    if ((p.rows ?? []).some((r) => !r.built_in)) writes.push(word('preset-edit', WORDS.editPresets, false));
  }
  return `<h2 class="p-label">${esc(WORDS.presetsLabel)}</h2>`
    + `<p class="p-status">${esc(status?.lead ?? '')}${name}${esc(status?.tail ?? '')}</p>`
    + `<p class="p-names">${joined(names)}</p>`
    + `${writes.length > 0 ? `<p class="p-writes">${joined(writes)}</p>` : ''}`
    + `${p.note ? `<p class="p-ronote">${esc(p.note)}</p>` : ''}<div class="p-ask"></div>`;
}

/** The device's own presets section; nothing for a device without them. */
export function devicePresets(_page, device, _view) {
  const d = device?.device_presets;
  if (!d) return '';
  const inert = Boolean(device.inert);
  const quiet = inert || d.recall === false;
  const slots = (d.slots ?? []).map((s) => word('slot', s.name, quiet,
    ` id="p-sl-${esc(s.slot)}" data-slot="${esc(s.slot)}"`));
  const save = d.save && !inert ? word('slot-save-list', WORDS.saveBands, false) : '';
  const built = (d.builtins ?? []).map((b, i) => word('setting', b.name, quiet,
    ` id="p-st-${i}" data-name="${esc(b.name)}"`, b.on ? 'on' : ''));
  return `<h2 class="p-label">${esc(WORDS.onDevice)}</h2>`
    + `<p class="p-names">${joined(slots)}</p>${save ? `<p class="p-writes">${save}</p>` : ''}`
    + `<h2 class="p-label">${esc(WORDS.builtIn)}</h2><p class="p-names">${joined(built)}</p>`
    + '<div class="p-ask"></div>';
}
