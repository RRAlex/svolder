/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Now playing bar: the transport, the track's headline and artist, its progress and its time.
// It is the player's own view of the track (the shown device's, else the host's). The progress is
// worked out against the daemon's clock (core/clock.js) so a browser whose clock is wrong still
// shows the player's time; `moveProgress` puts the moving parts in place once a second. Pure: the
// model in, markup out.

import { daemonNow, elapsedUs, fraction, timeWords } from '../../../core/clock.js';
import { WORDS } from '../words.js';
import { placeOpen } from '../place.js';
import { esc } from './esc.js';
import { button, inertPage } from './word.js';

/** The transport's icons, drawn as shapes so no font can swap them for a picture: the paths of a
 * 16 px box, filled with the button's own colour. */
const ICONS = {
  previous: '<path d="M3 3h2v10H3zM14 3v10L6 8z"/>',
  pause: '<path d="M4 3h3v10H4zM9 3h3v10H9z"/>',
  play: '<path d="M4 2.5v11L13 8z"/>',
  next: '<path d="M11 3h2v10h-2zM2 3v10l8-5z"/>',
};
const icon = (name) => `<svg class="sg-ic" viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true" focusable="false">${ICONS[name]}</svg>`;

/** The version of the cover whose picture failed to load, or null: the draws leave it out until a
 * draw sees another cover or none, which forgets the failure. */
let failedCover = null;

/** Remember that the picture of cover `version` failed to load (`null` forgets it). */
export function markCoverFailed(version) {
  failedCover = version ?? null;
}

/** The track's cover, a picture before its words (sized by the theme's CSS); nothing where the host
 * has none, or where its picture failed to load (no box, no gap). The address carries the cover's
 * version, so another track is another address. */
export function coverMarkup(track) {
  if (String(track?.cover ?? '') !== failedCover) failedCover = null;
  if (track?.cover === null || track?.cover === undefined || String(track.cover) === failedCover) return '';
  const v = encodeURIComponent(track.cover);
  return `<span class="sg-cover" aria-hidden="true"><img src="api/v1/host/cover?v=${v}" data-cover="${v}" alt=""></span>`;
}

/** The transport buttons for a player in `status`: previous, pause or play, next. */
export function transport(status, inert) {
  const middle = status === 'playing' ? ['pause', WORDS.pause] : ['play', WORDS.play];
  const all = [['previous', WORDS.previous], middle, ['next', WORDS.next]];
  return `<span class="sg-transport">${all.map(([name, label], i) => button('transport',
    icon(name), inert, ` id="sg-tr-${i}" data-button="${name}" aria-label="${label}"`,
    'sg-btn sg-small', true)).join('')}</span>`;
}

/** The track's progress at the browser's `localMs`:
 * `{ elapsed, length }` in microseconds, `null` for a part with no progress. The daemon's clock now
 * is what its part said then plus the time since. */
export function progressOf(p, track, localMs) {
  const prog = track.progress;
  if (!prog || !Number.isFinite(p.now_ms)) return null;
  const now = daemonNow(p.now_ms, localMs);
  const elapsed = elapsedUs({
    status: track.status,
    position: prog.position_us,
    rate: prog.rate_milli,
    readAt: prog.read_at_ms,
    length: track.length_us,
  }, now);
  return { elapsed, length: track.length_us };
}

/** `1:42 / 5:29`, or the elapsed time alone for a track of no known length. */
export const timeWordsOf = (prog) => `${timeWords(prog.elapsed)}${prog.length ? ` / ${timeWords(prog.length)}` : ''}`;

/** The Now playing bar for `page` at the browser's `localMs`; nothing when the model has no player
 * to speak of. */
export function drawPlaying(page, localMs = Date.now()) {
  const p = page.device?.playing ?? page.host;
  if (!p) return '';
  const track = p.track;
  if (!track) return `<div class="sg-np"><span class="sg-track"><i>${esc(WORDS.nothingPlaying)}</i></span></div>`;
  const inert = inertPage(page) || Boolean(page.device?.inert);
  const prog = progressOf(p, track, localMs);
  const at = prog ? fraction(prog.elapsed, prog.length) : 0;
  const time = prog ? timeWordsOf(prog) : '';
  const artist = track.artist ? ` · ${esc(track.artist)}` : '';
  return `<div class="sg-np" id="sg-np">${coverMarkup(track)}${p.buttons ? transport(track.status, inert) : ''}`
    + `<span class="sg-track"><b>${esc(track.headline)}</b>${artist}</span>`
    + `<span class="sg-bar"><span class="sg-fill" data-at="${at.toFixed(5)}"></span></span>`
    + `<span class="sg-time">${esc(time)}</span></div>`;
}

/** Put the bar's progress where `localMs` says: the fill and the time, from the player of `page`. Nothing for a player with no progress. */
export function moveProgress(bar, page, localMs) {
  const p = page.device?.playing ?? page.host;
  const track = p?.track;
  const prog = track ? progressOf(p, track, localMs) : null;
  if (!prog || track.status !== 'playing') return;
  bar.querySelector('.sg-fill').dataset.at = fraction(prog.elapsed, prog.length).toFixed(5);
  bar.querySelector('.sg-time').textContent = timeWordsOf(prog);
  placeOpen(bar);
}
