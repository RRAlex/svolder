/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the player's hop opens (P4): Now playing grown into the open area, with the transport, and the
// path in words with its verdict. While the player is chosen the bottom bar is not drawn (live.js).
// Pure: the model in, markup out; `moveProgress` (draw/playing.js) moves the progress of `#sg-pp`.

import { fraction } from '../../../core/clock.js';
import { PLAYER } from '../words.js';
import { esc } from './esc.js';
import { coverMarkup, progressOf, timeWordsOf, transport } from './playing.js';
import { inertPage } from './word.js';

/** The path as lines: each hop's label and its words, then the verdict in the colour of its class. */
function pathLines(path) {
  if (!path) return `<p class="sg-detail">${esc(PLAYER.noPath)}</p>`;
  const hops = (path.hops ?? []).map((hop) => `<li class="${hop.changes ? 'sg-warn' : ''}"><b>${esc(hop.label)}</b> `
    + `${esc(hop.detail ?? '')}</li>`).join('');
  const cls = path.class === 'ok' ? 'sg-ok' : 'sg-warn';
  return `<ul class="sg-pathlist">${hops}</ul>`
    + `${path.words ? `<p class="sg-verdict ${cls}">${esc(path.words)}</p>` : ''}`;
}

/** The open area of the player hop for `page` at the browser's `localMs`. */
export function playerScreen(page, localMs = Date.now()) {
  // The player the Now playing bar shows: the shown device's reading of it, else the host's.
  const host = page.device?.playing ?? page.host;
  const track = host?.track;
  if (!track) return `<section id="sg-player" class="sg-plain"><p class="sg-detail">${esc(PLAYER.noTrack)}</p></section>`;
  const inert = inertPage(page);
  const prog = progressOf(host, track, localMs, localMs);
  const at = prog ? fraction(prog.elapsed, prog.length) : 0;
  const time = prog ? timeWordsOf(prog) : '';
  const attrs = prog ? ` data-drawn="${localMs}"` : '';
  const byline = [track.artist, track.album].filter(Boolean).map(esc).join(' · ');
  const path = host.path ?? page.host?.path ?? null;
  return `<section id="sg-player" class="sg-plain"><span class="sg-lbl">${esc(PLAYER.title)}</span>`
    + `${coverMarkup(track)}<h2 class="sg-title">${esc(track.headline)}</h2>${byline ? `<p class="sg-byline">${byline}</p>` : ''}`
    + `<div class="sg-pprog" id="sg-pp"${attrs}><span class="sg-bar"><span class="sg-fill" data-at="${at.toFixed(5)}"></span></span>`
    + `<span class="sg-time">${esc(time)}</span></div>`
    + `${host.buttons ? transport(track.status, inert) : ''}`
    + `<span class="sg-lbl sg-gap">${esc(PLAYER.path)}</span>${pathLines(path)}</section>`;
}
