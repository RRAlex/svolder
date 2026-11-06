/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sheet's first row: the tone mode (a chip for each mode the device has, Loudness carrying its
// round `−` and `+` and its strength under its name while it is on) and the input (its options as
// chips, or the current one as a chip that opens a list). Pure: the model in, markup out.

import { SHEET } from '../words.js';
import { esc } from './esc.js';

/** The markup of a button that does `act`; `inert` draws it quiet and `aria-disabled`: a tap on it
 * still reaches the core, which says why nothing happens. `text` is escaped unless `markup`. */
export function button(act, text, inert, attrs = '', cls = '', markup = false) {
  return `<button type="button" class="${cls}${inert ? ' quiet' : ''}" data-act="${act}"${attrs}`
    + `${inert ? ' aria-disabled="true"' : ''}>${markup ? text : esc(text)}</button>`;
}

/** A chip of the sheet; `on` (when given) is its pressed state. */
export const chip = (act, text, inert, attrs, on, markup = false) => button(act, text, inert,
  `${attrs}${on === undefined ? '' : ` aria-pressed="${on}"`}`, `nd-chip${on ? ' on' : ''}`, markup);

/** An id made from a control's key and a suffix: what keeps the focus when the sheet is drawn again. */
export const idOf = (prefix, key, more = '') => `${prefix}-${String(key).replace(/[^\w-]/g, '_')}${more}`;

/** The Loudness chip while it is on: its round `−` and `+` over the ends and the strength under its
 * name (a button cannot hold buttons, so the three sit in one wrap). */
function loudChip(m, tone, inert) {
  const step = (delta, sign, words, side) => button('loudness', sign, inert,
    ` id="nd-loud${delta < 0 ? 'm' : 'p'}" data-step="${delta}" aria-label="${esc(words)}"`, `nd-rb ${side}`);
  const name = `${esc(m.label)}<i>${esc(SHEET.strength(tone.strength))}</i>`;
  return `<span id="nd-loud" class="nd-loud">${step(-1, SHEET.minus, SHEET.lowerLoudness, 'l')}`
    + `${button('mode', name, inert, ` id="nd-mode-${esc(m.name)}" data-mode="${esc(m.name)}" aria-pressed="true"`,
      'nd-chip on', true)}`
    + `${step(1, SHEET.plus, SHEET.raiseLoudness, 'r')}</span>`;
}

/** The tone mode: a chip for each mode the device has; nothing for a device with no tone. */
export function modeRow(device, inert = false) {
  const tone = device?.tone;
  if (!tone?.modes?.length) return '';
  const chips = tone.modes.map((m) => (m.on === true && tone.loud ? loudChip(m, tone, inert)
    : chip('mode', m.label, inert, ` id="nd-mode-${esc(m.name)}" data-mode="${esc(m.name)}"`, m.on === true)))
    .join('');
  return `<h2 class="nd-lbl">${esc(SHEET.mode)}</h2><div class="nd-set">${chips}</div>`;
}

/** The input: the control `input` as chips, or as one chip that opens a list. The sheet follows the model's
 * `style` (a list above five options), while the dash shows a list above three: it has less room. */
export function inputRow(device, inert = false) {
  const c = (device?.controls ?? []).find((x) => x.key === 'input' && x.kind === 'choice');
  const options = c?.options ?? [];
  if (options.length === 0) return '';
  const body = c.style === 'list'
    ? chip('input-list', `${(options.find((o) => o.on) ?? options[0]).label} ▾`, inert,
      ` id="nd-input-list" aria-haspopup="listbox" aria-expanded="false"`)
    : options.map((o, i) => chip('input', o.label, inert,
      ` id="nd-input-${i}" data-value="${esc(o.label)}"`, o.on === true)).join('');
  return `<h2 class="nd-lbl">${esc(c.label ?? SHEET.input)}</h2><div class="nd-set">${body}</div>`;
}
