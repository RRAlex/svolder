/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The amp: its power as a written line (`Power: on`, with the word the amp says while it warms
// up) and its switches, and its meters as small printed scales with a pencil needle each, the
// class-A share of the scale hatched, the reading written beside. The readouts the power part
// carries (temperatures and the like) are notes, not lines. Pure: the model in, markup out.

import { WORDS } from '../words.js';
import { SCALE_BOX, control, ticksOf } from './controls.js';
import { esc } from './esc.js';
import { word } from './word.js';

/** The power section: the line with its word, then the amp's switches; nothing for a device
 * without power. */
export function power(_page, device, _view) {
  const p = device?.power;
  if (!p) return '';
  const inert = Boolean(device.inert);
  const said = p.on === true ? WORDS.onWord : p.on === false ? WORDS.offWord : WORDS.unknownWord;
  const spoken = String(p.word ?? '').toLowerCase();
  const more = p.warming || (spoken && spoken !== WORDS.onWord && spoken !== WORDS.offWord)
    ? ` <i>${esc(spoken)}</i>` : '';
  const line = `<p class="p-row p-line">${esc(WORDS.powerLead)}${word('power', said, inert,
    ` id="p-power-word" data-key="${esc(p.key)}" aria-pressed="${p.on === true}"`,
    p.on === true ? 'on' : '')}${more}</p>`;
  return line + (p.toggles ?? []).map((c) => control(c, inert)).join('');
}

/** One meter's needle: where it stands, or hidden while the amp has not read it. */
const needle = (m) => (typeof m.fraction === 'number'
  ? `<span class="p-mark" data-key="${esc(m.key)}" data-at="${m.fraction.toFixed(5)}"></span>`
  : `<span class="p-mark off" data-key="${esc(m.key)}"></span>`);

/** The meters section; nothing for a device without. */
export function meters(_page, device, _view) {
  const m = device?.meters;
  if (!m) return '';
  const rows = (m.meters ?? []).map((x) => `<div class="p-meter"><span class="p-chan">${esc(x.channel)}</span>`
    + `<div class="p-scale meter" data-key="${esc(x.key)}">`
    + `<svg viewBox="0 0 ${SCALE_BOX} 12" preserveAspectRatio="none" aria-hidden="true">`
    + `${ticksOf({ lo: 0, hi: 10 })}</svg>`
    + `<div class="p-zone classa" data-at="${Number(x.class_a ?? 0).toFixed(5)}"></div>${needle(x)}</div>`
    + `<span class="p-text" data-meter="${esc(x.key)}">${esc(x.text)}</span></div>`);
  return `<h2 class="p-label p-head">${esc(WORDS.metersLabel)}</h2>${rows.join('')}`
    + `<p class="p-mstate">${m.state ? `<i>${esc(m.state)}</i>` : ''}</p>`;
}

/** Put a reading of the meters in place without drawing the section again: the needles, their
 * words and the state line. Says whether it could (the same meters as were drawn). */
export function metersInPlace(box, _page, device, _view) {
  const list = device?.meters?.meters ?? [];
  const marks = [...(box.querySelectorAll?.('.p-scale.meter') ?? [])];
  if (marks.length === 0 || marks.length !== list.length
    || marks.some((s, i) => s.dataset.key !== list[i].key)) return false;
  list.forEach((x, i) => {
    const mark = marks[i].querySelector('.p-mark');
    if (typeof x.fraction === 'number') mark.dataset.at = x.fraction.toFixed(5);
    else delete mark.dataset.at;
    mark.classList.toggle('off', typeof x.fraction !== 'number');
    if (typeof x.fraction === 'number') mark.style.left = `${(x.fraction * 100).toFixed(2)}%`;
    const text = box.querySelector(`[data-meter="${CSS.escape(x.key)}"]`);
    if (text && text.textContent !== x.text) text.textContent = x.text;
  });
  const state = box.querySelector('.p-mstate');
  if (state) state.innerHTML = device.meters.state ? `<i>${esc(device.meters.state)}</i>` : '';
  return true;
}
