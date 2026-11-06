/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume of the DAC's first screen: the level large with its unit, `−` `+` and `Mute`, and a
// thin track under them whose knob is a slider (a drag or the keys), the cap marked on it where the
// config holds one. The knob stands at the step the page last asked for until the device reports
// it (or `ASK_MS` pass), else at the step the device reports. What moves (the knob, the words, the
// mute word) can be put in place without drawing the section again (`showVolume`), so a reading
// never takes the hand's place. Pure but for `showVolume`, which sets what the markup names.

import { WORDS } from '../words.js';
import { placeOpen } from '../place.js';
import { esc } from './esc.js';
import { button } from './word.js';

/** How long a step asked for stays the one shown while the device has not reported it (ms). */
export const ASK_MS = 3000;

/** Whether the device is climbing to a step above where it is (a glide). */
const gliding = (volume) => volume.target !== null && volume.target !== undefined
  && volume.target > (volume.raw ?? 0);

/** The step the knob stands at: the step asked for while it is not yet the one the device reports
 * and was not asked too long ago, else the one the device reports (or `null`). Once the device has
 * answered the ask, a reading that is not the ask (and is no glide) is another hand's and is shown. */
export function shownRaw(volume, view) {
  const asked = view.asked;
  if (!asked || volume.raw === null || volume.raw === undefined) return volume.raw ?? null;
  if (volume.raw === asked.raw || view.now - asked.at >= ASK_MS) return volume.raw;
  if (asked.answered && !gliding(volume)) return volume.raw;
  return asked.raw;
}

/** The fraction of the track (0 to 1) step `raw` stands at. */
export const fractionOf = (volume, raw) => (volume.hi > volume.lo
  ? Math.min(1, Math.max(0, (raw - volume.lo) / (volume.hi - volume.lo))) : 0);

/** The step a fraction of the track is, within the range. */
export const rawAt = (volume, fraction) => Math.min(volume.hi,
  Math.max(volume.lo, Math.round(volume.lo + fraction * (volume.hi - volume.lo))));

/** What the volume says at step `raw`: the words (`-26.1 dB`), the number alone and its unit, from
 * the device's report when `raw` is its reading and from its table of steps otherwise. */
function say(volume, raw) {
  if (raw === null || raw === undefined) return { words: WORDS.unread, num: WORDS.unread, unit: '' };
  const words = raw === volume.raw ? volume.text : (volume.texts?.[raw - volume.lo] ?? '');
  const unit = volume.unit ? ` ${volume.unit}` : '';
  const cut = unit && words.endsWith(unit);
  return { words, num: cut ? words.slice(0, -unit.length) : words, unit: cut ? volume.unit : '' };
}

/** The moving things of the volume, worked out. */
function moving(volume, view) {
  const raw = shownRaw(volume, view);
  const said = say(volume, raw);
  const muted = Boolean(volume.muted);
  return {
    raw,
    at: raw === null ? 0 : fractionOf(volume, raw),
    words: muted ? WORDS.muted : said.words,
    num: muted ? WORDS.silence : said.num,
    unit: muted ? (volume.unit || 'dB') : said.unit,
    muted,
  };
}

/** The volume's box for the shown device; nothing for a device without a volume. */
export function volumeBox(device, view) {
  const v = device?.volume;
  if (!v) return '';
  const now = moving(v, view);
  const inert = Boolean(device.inert);
  const cap = v.cap !== null && v.cap !== undefined && v.hi > v.lo
    ? `<span class="sg-vcap" data-at="${fractionOf(v, v.cap).toFixed(5)}" title="${esc(WORDS.cap)}"></span>` : '';
  const mute = v.mute
    ? button('mute', now.muted ? WORDS.unmute : WORDS.mute, inert,
      ` id="sg-mute" data-key="${esc(v.mute)}" aria-pressed="${now.muted}"`, 'sg-btn sg-pill') : '';
  return `<span class="sg-lbl">${esc(WORDS.volume)}</span>`
    + `<div id="sg-big" class="sg-big"><span class="sg-num">${esc(now.num)}</span>`
    + `<small>${esc(now.unit)}</small></div>`
    + '<div class="sg-vrow">'
    + `${button('volume-down', '−', inert, ` id="sg-vdown" aria-label="${esc(WORDS.quieter)}"`, 'sg-btn')}`
    + `${mute}`
    + `${button('volume-up', '+', inert, ` id="sg-vup" aria-label="${esc(WORDS.louder)}"`, 'sg-btn')}`
    + '</div>'
    + `<div id="sg-vtrack" class="sg-vtrack${inert ? ' quiet' : ''}" role="slider" tabindex="0"`
    + ` aria-label="${esc(WORDS.volume)}" aria-valuemin="${esc(v.lo)}" aria-valuemax="${esc(v.hi)}"`
    + `${now.raw === null ? '' : ` aria-valuenow="${esc(now.raw)}"`} aria-valuetext="${esc(now.words)}"`
    + `${inert ? ' aria-disabled="true"' : ''}>`
    + `<span class="sg-fill" data-at="${now.at.toFixed(5)}"></span>${cap}`
    + `<span id="sg-knob" class="sg-vknob" data-at="${now.at.toFixed(5)}"></span></div>`;
}

/** Put the moving things in the drawn volume `box` (the knob, the words, the mute word) without
 * drawing it again. */
export function showVolume(box, v, view) {
  const now = moving(v, view);
  const track = box.querySelector('#sg-vtrack');
  if (!track) return;
  if (now.raw === null) track.removeAttribute('aria-valuenow');
  else track.setAttribute('aria-valuenow', String(now.raw));
  track.setAttribute('aria-valuetext', now.words);
  track.querySelector('.sg-fill').dataset.at = now.at.toFixed(5);
  track.querySelector('.sg-vknob').dataset.at = now.at.toFixed(5);
  const big = box.querySelector('#sg-big');
  big.querySelector('.sg-num').textContent = now.num;
  big.querySelector('small').textContent = now.unit;
  const mute = box.querySelector('[data-act="mute"]');
  if (mute) {
    mute.textContent = now.muted ? WORDS.unmute : WORDS.mute;
    mute.setAttribute('aria-pressed', String(now.muted));
  }
  placeOpen(track);
}
