/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume section: a printed ruler linear in the device's steps, a red pencil needle, the
// level written beside it, and the written controls (`− quieter · mute · louder +`). The needle is
// a slider; its place is the step the page last asked for until the device reports that step (or
// `ASK_MS` pass), else the step the device reports. What moves (the needle, its words, the mute
// word) can be put in place without drawing the ruler again (`showVolume`), so a reading never
// takes the focus from the needle or a word.

import { place } from '../place.js';
import {
  NARROW_PX, fractionOf, labelGap, marks, percentVolume, ticks, zones,
} from '../ruler.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { onComputer, viewerQuiet } from './side.js';
import { word } from './word.js';

/** How long a step asked for stays the one shown while the device has not reported it (ms). */
export const ASK_MS = 3000;

/** The ticks are drawn on a 1000 by 20 box, stretched over the ruler; heights of the two kinds. */
const TICK_BOX = 1000;
const MAJOR = 14;
const MINOR = 8;

/** A zone starting past this fraction of the ruler is too narrow for its label, which then ends
 * at the ruler's right edge instead of starting inside the zone. */
const ZONE_FLIP = 0.7;

/** The ruler's width in pixels as far as the sheet knows it. */
const widthOf = (view) => view.rulerPx ?? (view.phone ? 340 : 900);

/** Whether the device is climbing to a step above where it is (a glide). */
const gliding = (volume) => volume.target !== null && volume.target !== undefined
  && volume.target > (volume.raw ?? 0);

/** The step the needle stands at: the step asked for while it is not yet the one the device
 * reports and was not asked too long ago, else the one the device reports (or `null`). Once the
 * device has answered the ask, a reading that is not the ask (and is no glide) is another hand's
 * and is shown. */
export function shownRaw(volume, view) {
  const asked = view.asked;
  if (!asked || volume.raw === null || volume.raw === undefined) return volume.raw ?? null;
  if (volume.raw === asked.raw || view.now - asked.at >= ASK_MS) return volume.raw;
  if (asked.answered && !gliding(volume)) return volume.raw;
  return asked.raw;
}

/** What the volume says at the step `raw`: the words (`-26.1 dB`), the number alone and its unit,
 * from the device's report when `raw` is its reading and from its table of steps otherwise. */
function say(volume, raw) {
  if (raw === null || raw === undefined) {
    const unread = volume.unreadWords ?? WORDS.unread;
    return { words: unread, num: unread, unit: '' };
  }
  const words = raw === volume.raw ? volume.text : (volume.texts?.[raw - volume.lo] ?? '');
  const unit = volume.unit ? ` ${volume.unit}` : '';
  const cut = unit && words.endsWith(unit);
  return { words, num: cut ? words.slice(0, -unit.length) : words, unit: cut ? volume.unit : '' };
}

/** The moving things of the volume, worked out: where the needle stands, what is written. */
function moving(volume, view) {
  // the computer's volume is what the core draws (held values included): no step is asked for here
  const raw = volume.percent ? volume.raw : shownRaw(volume, view);
  const said = say(volume, raw);
  const muted = Boolean(volume.muted);
  const at = raw === null ? 0 : fractionOf(volume, raw);
  return {
    raw,
    at,
    flip: at > 0.85,
    words: muted ? WORDS.muted : said.words,
    num: muted ? WORDS.muted : said.num,
    unit: muted ? '' : said.unit,
    muted,
    target: gliding(volume) ? fractionOf(volume, volume.target) : null,
  };
}

/** What the drawn ruler depends on: when it differs from what is drawn, the ruler is drawn
 * again. */
function signature(volume, device, view) {
  const table = volume.db ?? [];
  return JSON.stringify([volume.lo, volume.hi, volume.risk, volume.cap, volume.mute, volume.unit,
    volume.silence, table.length, table.find(Number.isFinite), table.at(-1),
    (volume.marks ?? []).length, Boolean(volume.percent), Boolean(volume.idle), Boolean(device?.inert),
    widthOf(view) < NARROW_PX,
    Number(labelGap(widthOf(view)).toFixed(4))]);
}

/** The tick marks as two SVG paths (minor and major), on the ruler's own box. */
function tickPaths(volume, levels) {
  const major = (at) => levels.some((m) => Math.abs(m.at - at) < 1e-9);
  const stroke = (list, tall) => list.map((at) => `M${(at * TICK_BOX).toFixed(2)} 0V${tall}`).join('');
  const all = ticks(volume);
  return `<path class="minor" d="${stroke(all.filter((a) => !major(a)), MINOR)}"/>`
    + `<path class="major" d="${stroke(all.filter(major), MAJOR)}"/>`;
}

/** What the volume section draws and whether it is out of reach: the computer's volume as a
 * 0 to 100 % ruler while the computer's side is shown, else the device's own. */
function sideOf(page, device, view) {
  if (onComputer(view)) {
    const read = view.computer.volumeOf(page);
    if (read) return { v: percentVolume(read), inert: viewerQuiet(page) || read.inert, computer: true };
  }
  return { v: device?.volume, inert: Boolean(device?.inert), computer: false };
}

/** The ids of the written words, by side. */
const ACTS = {
  device: { down: 'volume-down', up: 'volume-up', mute: 'mute' },
  computer: { down: 'c-down', up: 'c-up', mute: 'c-mute' },
};

/** The volume section for the shown device, or for the computer; nothing for a device without a
 * volume. */
export function volume(page, device, view) {
  const { v, inert, computer } = sideOf(page, device, view);
  if (!v) return '';
  const now = moving(v, view);
  const acts = computer ? ACTS.computer : ACTS.device;
  const labels = marks(v, widthOf(view));
  const { risk, cap } = zones(v);
  const zone = (name, at, text) => (at === null ? ''
    : `<div class="p-zone ${name}${at > ZONE_FLIP ? ' flip' : ''}" data-at="${at.toFixed(5)}"><span>${esc(text)}</span></div>`);
  const written = [word(acts.down, WORDS.quieter, inert, computer ? ' id="p-cdown"' : '')];
  if (v.mute) {
    written.push(word(acts.mute, now.muted ? WORDS.unmute : WORDS.mute, inert,
      computer ? ' id="p-cmute"' : ` data-key="${esc(v.mute)}"`));
  }
  written.push(word(acts.up, WORDS.louder, inert, computer ? ' id="p-cup"' : ''));
  const idle = v.idle ? ' idle' : '';
  return `<div id="p-big" class="p-big"><span class="p-num">${esc(now.num)}</span>`
    + `<small>${esc(now.unit)}</small></div>`
    + `<div id="p-ruler" class="p-ruler${computer ? ' pct' : ''}" data-sig="${esc(signature(v, { inert }, view))}">`
    + `<svg class="p-ticks" viewBox="0 0 ${TICK_BOX} 20" preserveAspectRatio="none" aria-hidden="true">`
    + `${tickPaths(v, labels)}</svg>`
    + labels.map((m) => `<span class="p-mark" data-at="${m.at.toFixed(5)}">${esc(m.text)}</span>`).join('')
    + zone('risk', risk, WORDS.risk) + zone('cap', cap, WORDS.cap)
    + `<span class="p-target" data-at="${(now.target ?? 0).toFixed(5)}"${now.target === null ? ' hidden' : ''}></span>`
    + `<div id="p-needle" class="p-needle${idle}" role="slider" tabindex="0" aria-label="Volume"`
    + ` aria-valuemin="${esc(v.lo)}" aria-valuemax="${esc(v.hi)}"`
    + `${now.raw === null ? '' : ` aria-valuenow="${esc(now.raw)}"`}`
    + ` aria-valuetext="${esc(now.words)}" data-at="${now.at.toFixed(5)}"></div>`
    + `<span id="p-volume-text"${now.flip ? ' class="flip"' : ''}${idle ? ' hidden' : ''} data-at="${now.at.toFixed(5)}">${esc(now.words)}</span>`
    + `</div><p class="p-written">${written.join(' &middot; ')}</p>`;
}

/** Put the moving things in the drawn volume `box` (the needle, the words, the mute word, the
 * climb's target) without drawing it again, and the marks in their places. */
export function showVolume(box, v, view) {
  const now = moving(v, view);
  const needle = box.querySelector('#p-needle');
  const text = box.querySelector('#p-volume-text');
  needle.dataset.at = now.at.toFixed(5);
  needle.setAttribute('aria-valuetext', now.words);
  if (now.raw === null) needle.removeAttribute('aria-valuenow');
  else needle.setAttribute('aria-valuenow', String(now.raw));
  text.dataset.at = now.at.toFixed(5);
  text.textContent = now.words;
  text.classList.toggle('flip', now.flip);
  const big = box.querySelector('#p-big');
  big.querySelector('.p-num').textContent = now.num;
  big.querySelector('small').textContent = now.unit;
  const mute = box.querySelector('[data-act="mute"], [data-act="c-mute"]');
  if (mute) mute.textContent = now.muted ? WORDS.unmute : WORDS.mute;
  const target = box.querySelector('.p-target');
  target.hidden = now.target === null;
  target.dataset.at = (now.target ?? 0).toFixed(5);
  place(box);
}

/** A reading of the volume put in place when the drawn ruler still fits it: says whether it did
 * (otherwise nothing was changed and the section must be drawn again). */
export function volumeInPlace(box, page, device, view) {
  const { v, inert } = sideOf(page, device, view);
  const ruler = box.querySelector?.('#p-ruler');
  if (!v || !ruler || ruler.dataset.sig !== signature(v, { inert }, view)) return false;
  showVolume(box, v, view);
  return true;
}
