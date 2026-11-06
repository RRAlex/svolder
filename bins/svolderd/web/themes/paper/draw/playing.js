/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Now playing: the track the computer's player has (the device's own view of it, or the host's
// when no device is shown) in italics with its artist after, and the transport as written words
// where the viewer has the buttons, and the cover beside them in monochrome when there is one. Pure: the model in, markup out.

import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

/** The transport's words for a player in `status`: back, pause or play, next. */
function transport(status, inert) {
  const middle = status === 'playing'
    ? ['pause', WORDS.pause] : ['play', WORDS.play];
  const words = [['previous', WORDS.back], middle, ['next', WORDS.next]];
  return `<p class="p-transport">${words.map(([button, text]) => word('transport', text, inert,
    ` data-button="${button}"`)).join(' &middot; ')}</p>`;
}

/** The track's cover, a small print beside its words; nothing where the host has none (no box, no
 * gap). The address carries the cover's version, so another track is another address. The style
 * draws it in monochrome. */
function cover(track) {
  if (track.cover === null || track.cover === undefined) return '';
  return `<span class="p-cover" aria-hidden="true"><img src="api/v1/host/cover?v=${encodeURIComponent(track.cover)}" alt="" width="56" height="56"></span>`;
}

/** The Now playing section; nothing when the model has no player to speak of. */
export function playing(page, device, _view) {
  const p = device?.playing ?? page.host;
  if (!p) return '';
  const label = `<h2 class="p-label p-head">${esc(WORDS.playingLabel)}</h2>`;
  const t = p.track;
  if (!t) return `${label}<p class="p-track"><i>${esc(WORDS.nothingPlaying)}</i></p>`;
  const artist = t.artist ? `, ${esc(t.artist)}` : '';
  const words = `<p class="p-track"><i>${esc(t.headline)}</i>${artist}</p>`
    + `${p.buttons ? transport(t.status, Boolean(device?.inert)) : ''}`;
  const print = cover(t);
  return print ? `${label}<div class="p-nowrow">${print}<div class="p-nowtext">${words}</div></div>` : `${label}${words}`;
}
