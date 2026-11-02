/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sheet of the theme Paper: its skeleton (`sheet`), and the functions that draw each section
// of it from the model (`SECTIONS`). Every function is `(page, device, view) -> string`, where
// `view` is what the screen is like (`{ phone, state, coarse, now, keys }`); a section nobody has
// written yet draws nothing. No markup here carries a `style` attribute (positions go through
// place.js) and every word from the model is escaped.

import { WORDS, dateWords, titleWords } from '../words.js';
import { meters, metersInPlace, power } from './amp.js';
import { controls } from './controls.js';
import { card, device } from './device.js';
import { esc } from './esc.js';
import { notes } from './notes.js';
import { playing } from './playing.js';
import { devicePresets, presets } from './presets.js';
import { tone } from './tone.js';
import { volume, volumeInPlace } from './volume.js';

export { esc };

/** The width under which the sheet is a phone's: one column, the notes after what they are about. */
export const PHONE_MAX_PX = 719;

/** The sections of the two columns, top to bottom. */
const LEFT = ['tone', 'power', 'meters'];
const RIGHT = ['controls', 'playing', 'presets', 'device-presets'];

/** An empty section of the skeleton. */
const part = (name, tag = 'section') => `<${tag} id="p-${name}"></${tag}>`;

/** The skeleton of the sheet: every section's element empty, the marks' layer over them. */
export function sheet(_page) {
  return `<div id="paper" class="paper">${part('title', 'header')}${part('card')}`
    + `${part('volume')}<div class="p-cols"><div id="p-left">${LEFT.map((n) => part(n)).join('')}</div>`
    + `<div id="p-right">${RIGHT.map((n) => part(n)).join('')}</div></div>`
    + `${part('notes')}${part('device')}${part('foot', 'footer')}`
    + '<svg id="p-marks" aria-hidden="true" xmlns="http://www.w3.org/2000/svg"></svg></div>';
}

/** A word that goes somewhere (another device, the sign-in): underlined, in a button or a link. */
const go = (what, text, extra = '') => `<button type="button" class="p-w" data-go="${what}"${extra}>${esc(text)}</button>`;

/** The title: the device's name and its line, the others of the fleet, the slot the frame's button
 * sits over. While the stream is lost or the page reads the device again, the line says so. */
function title(page, device, view) {
  const { name, sub } = titleWords(page, device);
  const said = { lost: WORDS.reconnecting, waiting: WORDS.waiting }[view.state];
  const line = said ?? sub;
  const others = (page.fleet?.devices ?? []).filter((d) => d.url !== page.shown);
  const also = others.length > 0
    ? `<p class="p-also">${esc(WORDS.also)} ${others.map((d) => go('device', d.name,
      ` data-device="${esc(d.url)}"`)).join(' &middot; ')}</p>`
    : '';
  return `<h1>${esc(name)}</h1>${line
    ? `<small class="p-sub${view.state === 'lost' ? ' lost' : ''}">${esc(line)}</small>` : ''}`
    + `${also}<span class="paper-slot"></span>`;
}

/** The footer: the program, its version and the date; the key that opens the themes; the way out
 * or in, and the theme's name. */
function foot(page, device, view) {
  const viewer = page.viewer ?? {};
  const version = viewer.version ? ` ${esc(viewer.version)}` : '';
  const left = `${esc(WORDS.program)}${version} &middot; ${esc(dateWords(view.now))}`;
  const keys = (view.keys ?? []).map((k) => esc(WORDS.keyHint(k))).join(' &middot; ');
  let right = esc(WORDS.themeName);
  if (viewer.guest) {
    right = `<a class="p-w" href="./?sign-in">${esc(WORDS.signIn)}</a> &middot; ${right}`;
  } else if (viewer.sign_out && viewer.version) {
    right = `${go('sign-out', WORDS.signOut)} &middot; ${right}`;
  }
  return `<span class="p-left">${left}</span><span class="p-keys">${keys}</span>`
    + `<span class="p-right">${right}</span>`;
}

/** The sections drawn so far, by name; the others draw nothing. */
export const SECTIONS = {
  title, card, volume, tone, power, meters, controls, playing, presets,
  'device-presets': devicePresets, device, notes, foot,
};

/** The sections that can put a change in place without being drawn again, by name:
 * `(box, page, device, view) -> whether it did`. */
export const QUICK = { volume: volumeInPlace, meters: metersInPlace };
