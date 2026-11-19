/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The notes section: the sheet's handwritten notes (notes.js says which and in whose words), each
// a `span.pn` carrying what it is about (`data-about`) and a tilt class chosen from its id. Where
// a note says "to" between two values the arrow is drawn as an ink stroke, followed by the word
// for a reader who listens or copies; the tick of "rate matches" is a stroke too. The card's red
// note is drawn under the title (draw/device.js), so it is not drawn here. Where each note sits
// (the band's third, or after its section on a phone) is marks.js's work. The model in, markup
// out; the last change is the keeper's (`view.changes`, fed by live.js).

import { daemonNow } from '../../../core/clock.js';
import { CONFIG_TICK, notesOf } from '../notes.js';
import { ARROW, RATE_MATCHES, WORDS } from '../words.js';
import { esc } from './esc.js';
import { onComputer } from './side.js';

/** The hand-drawn arrow: a shaft with a slight bow and an open head, in the colour of the text. */
const ARROW_SVG = '<svg class="p-arrow" aria-hidden="true" focusable="false" viewBox="0 0 36 14" '
  + 'width="36" height="14"><path d="M1.5 9.5C10 4 22 4.5 32 7.5"/><path d="M25.5 2.5L32.5 7.5L24.5 12"/></svg>';

/** The tick that ends "rate matches all the way". */
const TICK_SVG = '<svg class="p-tick" aria-hidden="true" focusable="false" viewBox="0 0 14 14" '
  + 'width="14" height="14"><path d="M1.5 7.5L5.5 11.5L12.5 2.5"/></svg>';

/** The tilts a note can lean to, by class (`t0` to `t4`): chosen by a hash of its id, so a note
 * leans the same way every time it is drawn. */
export const TILTS = 5;
export function tiltOf(id) {
  let h = 0;
  for (const ch of String(id)) h = (h * 31 + ch.charCodeAt(0)) % 1009;
  return h % TILTS;
}

/** A note's words as markup: escaped, each arrow an ink stroke and the word `to`, the tick after
 * the words that say the rate matches. */
export function ink(text) {
  return esc(text)
    .replace(new RegExp(` ?${ARROW} ?`, 'g'), `${ARROW_SVG}<span class="p-sr"> ${WORDS.handArrow} </span>`)
    .replace(RATE_MATCHES, `${RATE_MATCHES}${TICK_SVG}`);
}

/** The config chip as a button that opens the PipeWire config popup: the daemon's short words with
 * its tick drawn as a stroke, its summary the accessible name, its class a class of the button. */
function chipButton(n, i) {
  const words = esc(n.text).replace(CONFIG_TICK, TICK_SVG);
  return `<button type="button" id="p-config" class="pn t${tiltOf(n.id)} p-setup ${esc(n.chip)}" data-note="${esc(n.id)}"`
    + ` data-n="${i}" data-about="${esc(n.about)}" data-act="config" aria-haspopup="dialog"`
    + `${n.title ? ` title="${esc(n.title)}" aria-label="${esc(n.chipWords)}, ${esc(n.title)}"` : ''}>${words}</button>`;
}

/** The daemon's clock now, in milliseconds, for the player part `p`, `now` being this browser's. */
function clockOf(p, now) {
  if (!Number.isFinite(p.now_ms)) return undefined;
  return daemonNow(p.now_ms, now);
}

/** What the track note carries for the clock that keeps its time current: the player's reading
 * and the daemon's clock as of this draw, and the words as drawn (the clock writes the note again
 * only when its words differ). */
function trackData(p, track, now, text) {
  const clock = clockOf(p, now);
  const prog = track.progress;
  return `${clock === undefined ? '' : ` data-clock="${clock}" data-drawn="${now}"`}`
    + ` data-words="${esc(text)}" data-status="${esc(track.status)}"`
    + `${prog ? ` data-position="${prog.position_us}" data-rate="${prog.rate_milli}" data-read-at="${prog.read_at_ms}"` : ''}`
    + `${track.length_us ? ` data-length="${track.length_us}"` : ''}`
    + ` data-album="${esc(track.album ?? '')}"`;
}

/** The notes section: every note but the card's. */
export function notes(page, device, view) {
  const p = device?.playing ?? page?.host ?? null;
  const now = view.now ?? Date.now();
  const clockNowMs = p ? clockOf(p, now) : undefined;
  // the computer's side flips the volume and the tone: the device's notes about them, and about
  // its presets, are for the other side
  const flipped = onComputer(view);
  const own = notesOf(page, device, { change: view.changes?.last() ?? null, coarse: view.coarse, clockNowMs })
    .filter((n) => n.id !== 'card' && !(flipped && ['volume', 'tone', 'presets'].includes(n.about)));
  return own
    .map((n, i) => (n.chip ? chipButton(n, i) : `<span class="pn t${tiltOf(n.id)}${n.red ? ' red' : ''}" data-note="${esc(n.id)}"`
      + ` data-n="${i}"`
      + ` data-about="${esc(n.about)}"${n.title ? ` title="${esc(n.title)}"` : ''}${n.id === 'track' ? trackData(p, p.track, now, n.text) : ''}>`
      + `${ink(n.text)}</span>`))
    .join('');
}
