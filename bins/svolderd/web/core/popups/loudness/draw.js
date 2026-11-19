/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Loudness tab of the Sound popup as strings (model in, markup out; no DOM, no state). Every
// number, range, step and the curve is the host's (`EqView.loudness`, the view the daemon
// streams): this file lays them out and works out nothing of the stage. The one thing it adds to
// the host's curve is a faint second line, the host's curve laid over the computer's EQ response
// (`eq.response`, the points' curve the host also streams): the two together, as the sound plays,
// while the EQ is on. What the viewer has asked and the host has not yet answered is laid over the
// view by `shownView`.
//
// The page, in order: what loudness is (a note), the DAC's own loudness when it is on too, then
// Strength (a slider) and Reference (a value with `−` and `+`, and "Use current volume"), the
// picture, the live readout in a line and in rows (volume now, depth, bass, mids, treble). The
// on/off switch is the popup's header (`switchHtml`). Off is dimmed and still editable. Classes
// are frame.css's `fpl-*`, `fpx-*` and `fp-*`, which read the `--frame-*` tokens alone.

import { attr, esc } from '../../html.js';
import { dbText, fixedEven, gainText } from '../../words.js';

/** The id of `part` of the Loudness tab. */
export const lid = (part) => `popup-sound-ld-${part}`;

/** The places and controls a wire reaches by id. */
export const IDS = Object.freeze({
  switch: lid('switch'),
  page: lid('page'),
  controls: lid('controls'),
  readout: lid('readout'),
  strength: lid('strength'),
  strengthValue: lid('strength-value'),
  reference: lid('reference'),
  now: lid('now'),
  words: lid('words'),
});

/** The fields a stepper moves. */
export const FIELDS = Object.freeze(['reference_pct']);

/** The id of the `−` (`dir` -1) or `+` (1) of `field`. */
export const stepId = (field, dir) => lid(`${field}-${dir < 0 ? 'less' : 'more'}`);

/** The tab's words. */
export const WORDS = {
  on: 'On',
  off: 'Off',
  switchLabel: 'Loudness',
  intro: 'Quiets the sound the way the ear needs it at low volume: less of the middle, the bass '
    + 'and treble held up against it. It only cuts; it never makes anything louder.',
  none: 'Loudness is not offered here.',
  noComputer: 'The computer\'s sound is not on, so there is no loudness to set.',
  dacOn: 'The DAC\'s loudness is on too.',
  strength: 'Strength',
  strengthLabel: 'Loudness strength',
  reference: 'Reference',
  less: 'Lower the reference',
  more: 'Raise the reference',
  now: (volume) => (Number.isFinite(volume) ? `Use current volume (${Math.round(volume)} %)`
    : 'Use current volume'),
  pictureLabel: 'What the loudness stage does to each frequency, from 20 Hz to 20 kHz, never above 0 dB',
  legendStage: 'Loudness',
  legendWith: 'With the EQ',
  readoutOff: 'Off: the sound is not changed.',
  readoutFlat: 'Playing at the reference: flat, nothing is cut',
  readoutFollowing: ({ volume, reference }) => `Playing at ${volume} % of a ${reference} % reference: `
    + 'following the volume',
  readoutHeld: 'Not following the volume now.',
  volume: 'Volume now',
  depth: 'Depth',
  bass: 'Bass',
  mids: 'Mids',
  treble: 'Treble, against the mids',
  unread: 'not read yet',
};

/** The view `ld` with what has been asked (`asked`: the fields of a write not yet answered) laid
 * over it, so a press shows at once. The readout is the host's and does not move until it says. */
export function shownView(ld, asked = {}) {
  if (!ld) return ld;
  const out = { ...ld };
  if (typeof asked.on === 'boolean') out.on = asked.on;
  for (const field of ['strength_pct', 'reference_pct']) {
    if (Number.isFinite(asked[field])) out[field] = asked[field];
  }
  return out;
}

/** The range of `field` in the view, `{ min, max, step }`, or `null`. */
export const rangeOf = (ld, field) => ld?.ranges?.[field] ?? null;

/** The value a step of `dir` (-1 or 1) from the field's value in view asks for, held to the range
 * (the host rounds it to its step again); `null` when the field cannot move that way. */
export function stepped(ld, field, dir) {
  const range = rangeOf(ld, field);
  if (!range || !(range.step > 0)) return null;
  const now = ld[field];
  if (!Number.isFinite(now)) return null;
  const places = 6;
  const raw = Math.round((now + dir * range.step) * 10 ** places) / 10 ** places;
  const next = Math.min(Math.max(raw, range.min), range.max);
  return next === now ? null : next;
}

/** Whether the host says loudness cannot follow the volume now. The controls stay live: a change
    is saved and takes effect when loudness can follow again. */
export const blocked = (ld) => typeof ld?.unavailable === 'string' && ld.unavailable !== '';

/** The header's switch: a button that is a switch, with the state in words beside it. */
export function switchHtml(ld) {
  if (!ld) return '';
  const on = ld.on === true;
  return `<button type="button" class="fpx-switch${on ? ' on' : ''}" role="switch"`
    + `${attr('id', IDS.switch)}${attr('aria-checked', String(on))}`
    + `${attr('aria-label', WORDS.switchLabel)}`
    + ` data-act="on"><span class="fpx-switch-track" aria-hidden="true">`
    + '<span class="fpx-switch-knob"></span></span>'
    + `<span class="fpx-switch-words">${esc(on ? WORDS.on : WORDS.off)}</span></button>`;
}

const W = 300;
const H = 120;
const DB_TOP = 3;
const DB_BOTTOM = -24;
const num = (value) => fixedEven(value, 1);
const xAt = (hz, lo, hi) => (W * Math.log(hz / lo)) / Math.log(hi / lo);

/** The EQ's response at `hz`, from its log-spaced samples between 20 Hz and 20 kHz
 * (`eq.response`, 181 of them), by a straight line between the two samples round it; `null`
 * without them. */
function eqAt(response, hz, lo, hi) {
  if (!Array.isArray(response) || response.length < 2) return null;
  const at = ((response.length - 1) * Math.log(hz / lo)) / Math.log(hi / lo);
  const i = Math.min(Math.max(Math.floor(at), 0), response.length - 2);
  const t = Math.min(Math.max(at - i, 0), 1);
  const value = response[i] + (response[i + 1] - response[i]) * t;
  return Number.isFinite(value) ? value : null;
}

/** The curve of the stage with the EQ's response added, at the stage's own frequencies, or `null`
 * while the EQ is off or has no points (the two lines would be one). */
export function combined(ld, eq) {
  const hz = ld?.curve?.hz ?? [];
  const db = ld?.curve?.db ?? [];
  if (eq?.on !== true || !(eq.points?.length > 0) || hz.length < 2 || db.length !== hz.length) return null;
  const sums = hz.map((f, i) => {
    const e = eqAt(eq.response, f, hz[0], hz.at(-1));
    return e === null ? null : db[i] + e;
  });
  return sums.every((one) => one !== null) ? sums : null;
}

/** An axis end in words: `20 Hz`, `20 kHz`. */
const axisWords = (hz) => (hz >= 1000 ? `${Number((hz / 1000).toFixed(1))} kHz` : `${hz} Hz`);

/** The picture: the stage's curve of the view as it comes (filled under, a dashed line at 0 dB),
 * and the faint line of the stage and the EQ together while the EQ plays. Nothing without a curve.
 * The axes are fixed (20 Hz to 20 kHz, +3 to -24 dB) and widen only for a combined line that
 * leaves them. */
export function pictureHtml(ld, eq) {
  const hz = ld?.curve?.hz ?? [];
  const db = ld?.curve?.db ?? [];
  if (hz.length < 2 || db.length !== hz.length) return '';
  const lo = hz[0];
  const hi = hz.at(-1);
  const both = combined(ld, eq);
  const top = Math.max(DB_TOP, both ? Math.ceil(Math.max(...both) + 0.5) : DB_TOP);
  const bottom = Math.min(DB_BOTTOM, both ? Math.floor(Math.min(...both) - 0.5) : DB_BOTTOM);
  const yAt = (value) => ((top - Math.min(Math.max(value, bottom), top)) / (top - bottom)) * H;
  const line = (values) => hz.map((f, i) => `${i === 0 ? 'M' : 'L'}${num(xAt(f, lo, hi))} ${num(yAt(values[i]))}`)
    .join('');
  const marks = [100, 1000, 10000].filter((f) => f > lo && f < hi)
    .map((f) => `<path class="fpx-grid" d="M${num(xAt(f, lo, hi))} 0V${H}"/>`).join('')
    + [-6, -12, -18].map((v) => `<path class="fpx-grid" d="M0 ${num(yAt(v))}H${W}"/>`).join('');
  const zero = num(yAt(0));
  const keys = `<li class="fpe-key"><span class="fpe-swatch fpl-stage-key"></span>${esc(WORDS.legendStage)}</li>`
    + (both ? `<li class="fpe-key"><span class="fpe-swatch compare"></span>${esc(WORDS.legendWith)}</li>` : '');
  return `<figure class="fpl-picture"><div class="fp-graph fpl-curve" role="img"${attr('aria-label', WORDS.pictureLabel)}>`
    + `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">${marks}`
    + `<path class="fpl-zero" d="M0 ${zero}L${W} ${zero}"/>`
    + `<path class="fpl-fill" d="${line(db)}L${W} ${zero}L0 ${zero}Z"/>`
    + (both ? `<path class="fpl-with" d="${line(both)}"/>` : '')
    + `<path class="fpl-line" d="${line(db)}"/></svg></div>`
    + `<div class="fpx-axis" aria-hidden="true"><span>${esc(axisWords(lo))}</span><span>1 kHz</span>`
    + `<span>${esc(axisWords(hi))}</span></div>`
    + `<ul class="fpe-legend fpl-legend">${keys}</ul></figure>`;
}

/** The line under the picture: what the stage is doing, in words of the view's numbers. */
export function readoutWords(ld) {
  if (ld.on !== true) return WORDS.readoutOff;
  if (!ld.following) return ld.note || WORDS.readoutHeld;
  return ld.depth_db > 0
    ? WORDS.readoutFollowing({
      volume: Math.round(ld.volume_pct), reference: Math.round(ld.reference_pct),
    })
    : WORDS.readoutFlat;
}

/** The readout: the line, and the numbers in rows. */
function readoutHtml(ld) {
  const row = (label, text) => `<div class="fpx-row"><span class="fpx-label">${esc(label)}</span>`
    + `<span class="fpx-value">${esc(text)}</span></div>`;
  const volume = Number.isFinite(ld.volume_pct) ? `${Math.round(ld.volume_pct)} %` : WORDS.unread;
  return `<p class="fpl-readout" role="status">${esc(readoutWords(ld))}</p>`
    + '<div class="fpl-rows">'
    + `${row(WORDS.volume, volume)}${row(WORDS.depth, `${dbText(ld.depth_db)} dB`)}`
    + `${row(WORDS.bass, gainText(ld.lift_db))}${row(WORDS.mids, gainText(ld.mids_db))}`
    + `${row(WORDS.treble, gainText(ld.treble_db))}</div>`;
}

/** The two controls: Strength (a slider) and Reference (steppers and "Use current volume"). */
export function controlsHtml(ld) {
  const range = rangeOf(ld, 'strength_pct');
  const refRange = rangeOf(ld, 'reference_pct');
  const step = (dir, glyph, words) => {
    const off = refRange === null || stepped(ld, 'reference_pct', dir) === null;
    return `<button type="button" class="fi-btn fpx-step"${attr('id', stepId('reference_pct', dir))}`
      + `${attr('data-field', 'reference_pct')}${attr('data-dir', String(dir))}`
      + `${attr('aria-label', words)}${off ? ' aria-disabled="true"' : ''}>${glyph}</button>`;
  };
  const strength = `${Math.round(ld.strength_pct)} %`;
  const slider = range
    ? `<input type="range"${attr('id', IDS.strength)} min="${range.min}" max="${range.max}"`
      + ` step="${range.step}" value="${Math.round(ld.strength_pct)}"`
      + `${attr('aria-valuetext', `${Math.round(ld.strength_pct)} percent`)}`
      + '>'
    : '';
  const unread = !Number.isFinite(ld.volume_pct);
  return `<div class="fpl-row"><label class="fpx-label" for="${IDS.strength}">${esc(WORDS.strength)}</label>`
    + `${slider}<span class="fpl-val" id="${IDS.strengthValue}">${esc(strength)}</span></div>`
    + `<div class="fpl-row ref"><span class="fpx-label" id="${IDS.reference}-label">${esc(WORDS.reference)}</span>`
    + `<span class="fpl-ref"><span class="fpl-steps">${step(-1, '−', WORDS.less)}`
    + `<span class="fpl-val" id="${IDS.reference}"${attr('aria-labelledby', `${IDS.reference}-label`)}>`
    + `${esc(`${Math.round(ld.reference_pct)} %`)}</span>${step(1, '+', WORDS.more)}</span>`
    + `<button type="button" class="fi-btn" id="${IDS.now}" data-act="now"${unread ? ' aria-disabled="true"' : ''}>`
    + `${esc(WORDS.now(ld.volume_pct))}</button></span></div>`;
}

/** The part of the page that follows the host's readings: the picture and the readout. */
export const readingsHtml = (ld, eq) => `${pictureHtml(ld, eq)}${readoutHtml(ld)}`;

/** The whole tab. `m` is `{ ld, eq, refusal, computer, dac }`: the view as shown (`shownView`; none
 * when the host offers no loudness), the computer's EQ (for the faint line), the words of the last
 * refusal, whether the computer is there, and whether the DAC's own loudness is on too. */
export function pageHtml({
  ld, eq = null, refusal = null, computer = true, dac = false,
}) {
  if (!computer) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.noComputer)}</p>`;
  if (!ld) return `<p class="fp-note" id="${IDS.page}">${esc(WORDS.none)}</p>`;
  const off = ld.on !== true;
  let out = `<div class="fpl${off ? ' fpl-off' : ''}" id="${IDS.page}">`;
  if (blocked(ld)) out += `<p class="fp-note fp-failed" role="alert">${esc(ld.unavailable)}</p>`;
  if (refusal) out += `<p class="fp-note fp-failed" role="alert" id="${IDS.words}">${esc(refusal)}</p>`;
  out += `<p class="fp-note fpx-context">${esc(WORDS.intro)}</p>`;
  if (dac) out += `<p class="fp-note fpl-dac">${esc(WORDS.dacOn)}</p>`;
  out += `<div class="fpl-controls" id="${IDS.controls}">${controlsHtml(ld)}</div>`
    + `<div class="fpl-readings" id="${IDS.readout}">${readingsHtml(ld, eq)}</div></div>`;
  return out;
}
