/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash's markup, the strip along the bottom of the scene: `−` `MUTE` `+`, the input (chips, or
// one chip that opens a list), the cover of the track when the host has one, the track with its progress, the transport as inline SVG and
// `DEVICE ▴`. An amp shows its power key and gauges (draw/gauges.js) where the input would be, and its volume keys
// only when it has a volume. A viewer who may not write gets the same keys drawn inert: a tap still
// reaches the core, which says why nothing happens. The progress is worked out against the
// daemon's clock (core/clock.js); `moveProgress` puts the moving parts in place once a second.
// Pure but for `moveProgress`, which sets what the markup names.

import { elapsedUs, fraction, timeWords } from '../../../core/clock.js';
import { DASH } from '../words.js';
import { esc } from './esc.js';
import { inertOf } from '../inert.js';
import { drawGauges } from './gauges.js';

/** At most this many input options are chips; more are one chip that opens a list. */
export const CHIPS_MAX = 3;

/** The transport's icons: the paths of a 16 px box, filled with the key's own colour. */
const ICONS = {
  previous: '<path d="M3 2h2v12H3zM14 2v12L6 8z"/>',
  pause: '<path d="M3 2h4v12H3zM9 2h4v12H9z"/>',
  play: '<path d="M4 2v12l9-6z"/>',
  next: '<path d="M11 2h2v12h-2zM2 2v12l8-6z"/>',
};
const icon = (name) => `<svg class="nd-ic" viewBox="0 0 16 16" width="16" height="16" fill="currentColor" aria-hidden="true" focusable="false">${ICONS[name]}</svg>`;

/** A key of the dash. `act` is its `data-act`, `attrs` the rest (escaped, each with a leading
 * space), `cls` its extra classes; `markup` says `text` is markup the caller drew (an icon). */
function key(act, text, { inert, attrs = '', cls = '', markup = false }) {
  return `<button type="button" class="nd-k${cls ? ` ${cls}` : ''}${inert ? ' quiet' : ''}" data-act="${act}"${attrs}`
    + `${inert ? ' aria-disabled="true"' : ''}>${markup ? text : esc(text)}</button>`;
}

/** The volume keys: `−`, `MUTE`, `+`, in that order; nothing for a device with no volume. */
function volumeKeys(v, inert) {
  if (!v) return '';
  const mute = v.mute
    ? key('mute', DASH.mute, {
      inert, cls: `cy${v.muted ? ' on' : ''}`, attrs: ` id="nd-mute" data-key="${esc(v.mute)}" aria-pressed="${Boolean(v.muted)}"`,
    }) : '';
  return key('volume-down', '−', { inert, attrs: ` id="nd-vdown" aria-label="${DASH.quieter}"` })
    + mute
    + key('volume-up', '+', { inert, attrs: ` id="nd-vup" aria-label="${DASH.louder}"` });
}

/** The input: its options as chips (up to `CHIPS_MAX`) or the current one as a chip that opens a
 * list; nothing for a device with no input choice. */
function inputChips(device, inert) {
  const c = (device?.controls ?? []).find((x) => x.key === 'input' && x.kind === 'choice');
  const options = c?.options ?? [];
  if (options.length === 0) return '';
  if (options.length <= CHIPS_MAX) {
    return options.map((o) => key('input', o.label, {
      inert, cls: `cy chip${o.on ? ' on' : ''}`, attrs: ` data-value="${esc(o.label)}" aria-pressed="${Boolean(o.on)}"`,
    })).join('');
  }
  const now = options.find((o) => o.on) ?? options[0];
  return key('input', `${now.label} ▾`, {
    inert, cls: 'cy chip', attrs: ` data-list="1" aria-haspopup="listbox" aria-expanded="false" aria-label="${esc(`${DASH.input}: ${now.label}`)}"`,
  });
}

/** An amp's power key and its gauges (power and temperature as small neon arcs) where the input would be. */
function ampGauges(device, inert) {
  const p = device?.power;
  if (!p) return '';
  const said = p.word ?? (p.on === true ? 'On' : p.on === false ? 'Standby' : '—');
  const power = key('power', said, {
    inert, cls: `cy${p.on === true ? ' on' : ''}`, attrs: ` id="nd-power" data-key="${esc(p.key)}" aria-label="${DASH.power}: ${esc(said)}"`,
  });
  return power + drawGauges(device);
}

/** The track's progress at the browser's `localMs`, the part having been drawn at `drawnMs`:
 * `{ elapsed, length }` in microseconds, `null` for a part with no progress. */
export function progressOf(p, track, localMs, drawnMs) {
  const prog = track.progress;
  if (!prog || !Number.isFinite(p.now_ms)) return null;
  const daemonNow = p.now_ms + (localMs - drawnMs);
  const elapsed = elapsedUs({
    status: track.status,
    position: prog.position_us,
    rate: prog.rate_milli,
    readAt: prog.read_at_ms,
    length: track.length_us,
  }, daemonNow);
  return { elapsed, length: track.length_us };
}

/** `1:42 / 5:29`, or the elapsed time alone for a track of no known length. */
export const timeWordsOf = (prog) => `${timeWords(prog.elapsed)}${prog.length ? ` / ${timeWords(prog.length)}` : ''}`;

/** The track: title, then artist · album · time, then the progress bar; nothing for a page with no
 * player to speak of. */
function trackMarkup(p, localMs) {
  if (!p) return '<div class="nd-track" id="nd-track"></div>';
  const track = p.track;
  if (!track) return `<div class="nd-track" id="nd-track"><b class="nd-title">${esc(DASH.nothing)}</b></div>`;
  const prog = progressOf(p, track, localMs, localMs);
  const at = prog ? fraction(prog.elapsed, prog.length) : 0;
  const words = [track.artist, track.album].filter(Boolean).map(esc);
  if (prog) words.push(`<span class="nd-time">${esc(timeWordsOf(prog))}</span>`);
  return `<div class="nd-track" id="nd-track"><b class="nd-title">${esc(track.headline)}</b>`
    + `<small class="nd-sub">${words.join(' · ')}</small>`
    + `<span class="nd-prog"><i data-at="${at.toFixed(5)}"></i></span></div>`;
}

/** The cover of the track, a small picture between the input and the title; nothing where the host
 * has none (no box, no gap). The address carries the cover's version, so another track is another
 * picture; one the daemon no longer holds is hidden by the dash's error listener. */
function coverMarkup(track) {
  if (track?.cover === null || track?.cover === undefined) return '';
  return `<span class="nd-cover" aria-hidden="true"><img src="api/v1/host/cover?v=${encodeURIComponent(track.cover)}" alt="" width="30" height="30"></span>`;
}

/** The transport for a player in `status`: previous, pause or play, next. */
function transport(status, inert) {
  const middle = status === 'playing' ? ['pause', DASH.pause] : ['play', DASH.play];
  const all = [['prev', 'previous', DASH.previous], ['play', middle[0], middle[1]], ['next', 'next', DASH.next]];
  return all.map(([act, name, label], i) => key(act, icon(name), {
    inert, markup: true, attrs: ` id="nd-tr-${i}" data-button="${name}" aria-label="${label}"`,
  })).join('');
}

/**
 * The dash for `page` at the browser's `localMs`. The sheet's key is always there; the player's
 * transport only when the player has buttons to give (a guest is not given them).
 */
export function drawDash(page, view, localMs = Date.now()) {
  const device = page?.device ?? null;
  const inert = inertOf(page);
  const amp = (page?.fleet?.devices ?? []).find((d) => d.url === page?.shown)?.family === 'amp';
  const p = device?.playing ?? page?.host ?? null;
  const left = volumeKeys(device?.volume, inert) + (amp ? ampGauges(device, inert) : inputChips(device, inert));
  const right = (p?.buttons ? transport(p.track?.status, inert) : '')
    + key('sheet', DASH.device, { inert: false, cls: 'cy', attrs: ` id="nd-sheet" aria-haspopup="dialog" aria-label="${DASH.deviceLabel}"` });
  return `<div class="nd-grp nd-keys">${left}</div><div class="nd-now">${coverMarkup(p?.track)}${trackMarkup(p, localMs)}</div>`
    + `<div class="nd-grp nd-tr">${right}</div>`;
}

/** Put the track's progress where `localMs` says: the fill and the time, from the player of `page`
 * the dash was drawn for at `drawnMs`. Nothing for a player with no progress, nor (unless
 * `always`) for one that is not playing. */
export function moveProgress(root, page, localMs, drawnMs, always = false) {
  const p = page?.device?.playing ?? page?.host;
  const track = p?.track;
  const prog = track ? progressOf(p, track, localMs, drawnMs) : null;
  if (!prog || (!always && track.status !== 'playing')) return;
  const fill = root.querySelector('.nd-prog i');
  const time = root.querySelector('.nd-time');
  if (fill) fill.style.width = `${(fraction(prog.elapsed, prog.length) * 100).toFixed(2)}%`;
  if (time) time.textContent = timeWordsOf(prog);
}

/** Put every `[data-at]` of `root` at that fraction of its box's width (no `style` attribute: the
 * page's content policy forbids it, the style object is allowed). */
export function placeFills(root) {
  for (const el of root.querySelectorAll('.nd-prog i[data-at]')) {
    el.style.width = `${(Number(el.dataset.at) * 100).toFixed(2)}%`;
  }
}
