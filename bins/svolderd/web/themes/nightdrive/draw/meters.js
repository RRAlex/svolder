/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// An amp's meters in the sheet: a scale for each channel with the class A share shaded and a needle at the
// reading, and the reading's words. Pure: the model in, markup out; the needles are placed by `placeSheet`
// (place.js) and kept current by `metersInPlace` without drawing the sheet again.

import { METERS } from '../words.js';
import { esc } from './esc.js';

/** One meter's needle: where it stands, or hidden while the amp has not read it. */
const needle = (m) => (typeof m.fraction === 'number'
  ? `<span class="nd-needle" data-at="${m.fraction.toFixed(5)}"></span>`
  : '<span class="nd-needle off"></span>');

/** The meters of `device`: a row for each channel; nothing for a device without. */
export function drawMeters(device) {
  const m = device?.meters;
  if (!m || (m.meters ?? []).length === 0) return '';
  const rows = m.meters.map((x) => `<div class="nd-meter"><span class="nd-chan" aria-label="${esc(METERS.channel(x.channel))}">`
    + `${esc(x.channel)}</span><div class="nd-mscale" data-key="${esc(x.key)}">`
    + `<span class="nd-zone" data-at="${Number(x.class_a ?? 0).toFixed(5)}"></span>${needle(x)}</div>`
    + `<span class="nd-mtext" data-meter="${esc(x.key)}">${esc(x.text)}</span></div>`).join('');
  return `<h2 class="nd-lbl">${esc(METERS.title)}</h2>${rows}`
    + `<p class="nd-mstate nd-note">${m.state ? `<i>${esc(m.state)}</i>` : ''}</p>`
    + `<p class="nd-note">${esc(METERS.note)}</p>`;
}

/** Put a reading of the meters in place without drawing the sheet again: the needles, their words and the
 * state. Says whether it could (the same meters as were drawn). */
export function metersInPlace(box, device) {
  const list = device?.meters?.meters ?? [];
  const scales = [...(box.querySelectorAll?.('.nd-mscale') ?? [])];
  if (scales.length === 0 || scales.length !== list.length
    || scales.some((s, i) => s.dataset.key !== list[i].key)) return false;
  list.forEach((x, i) => {
    const mark = scales[i].querySelector('.nd-needle');
    const read = typeof x.fraction === 'number';
    if (read) {
      mark.dataset.at = x.fraction.toFixed(5);
      mark.style.left = `${(x.fraction * 100).toFixed(2)}%`;
    } else delete mark.dataset.at;
    mark.classList.toggle('off', !read);
    const text = box.querySelector(`[data-meter="${CSS.escape(x.key)}"]`);
    if (text && text.textContent !== x.text) text.textContent = x.text;
  });
  const state = box.querySelector('.nd-mstate');
  if (state) state.innerHTML = device.meters.state ? `<i>${esc(device.meters.state)}</i>` : '';
  return true;
}
