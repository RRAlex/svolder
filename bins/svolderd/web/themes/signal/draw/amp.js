/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The open area of an amp (P2): its power with its switches and readouts, its meters as scales with
// the class A share shaded and a needle at the reading, and the device's name with what its facts
// say first, Identify and Unlink. Pure: the model in, markup out; the needles are placed by
// `placeOpen` (place.js) and kept current by `metersInPlace` without drawing the section again.

import { AMP } from '../words.js';
import { esc } from './esc.js';
import { power } from './shelf.js';
import { button } from './word.js';

/** One meter's needle: where it stands, or hidden while the amp has not read it. */
const needle = (m) => (typeof m.fraction === 'number'
  ? `<span class="sg-needle" data-at="${m.fraction.toFixed(5)}"></span>`
  : '<span class="sg-needle off"></span>');

/** The meters: a row for each channel, nothing for an amp without. */
function meters(device) {
  const m = device.meters;
  if (!m) return '';
  const rows = (m.meters ?? []).map((x) => `<div class="sg-meter"><span class="sg-chan" aria-label="${esc(AMP.channel(x.channel))}">`
    + `${esc(x.channel)}</span><div class="sg-mscale meter" data-key="${esc(x.key)}">`
    + `<span class="sg-fill sg-zone" data-at="${Number(x.class_a ?? 0).toFixed(5)}"></span>${needle(x)}</div>`
    + `<span class="sg-mtext" data-meter="${esc(x.key)}">${esc(x.text)}</span></div>`).join('');
  return `<span class="sg-lbl">${esc(AMP.meters)}</span>${rows}`
    + `<p class="sg-mstate sg-note">${m.state ? `<i>${esc(m.state)}</i>` : ''}</p>`
    + `<p class="sg-note">${esc(AMP.meterNote)}</p>`;
}

/** Put a reading of the meters in place without drawing the section again: the needles, their words
 * and the state. Says whether it could (the same meters as were drawn). */
export function metersInPlace(box, device) {
  const list = device?.meters?.meters ?? [];
  const scales = [...(box.querySelectorAll?.('.sg-mscale') ?? [])];
  if (scales.length === 0 || scales.length !== list.length
    || scales.some((s, i) => s.dataset.key !== list[i].key)) return false;
  list.forEach((x, i) => {
    const mark = scales[i].querySelector('.sg-needle');
    const read = typeof x.fraction === 'number';
    if (read) {
      mark.dataset.at = x.fraction.toFixed(5);
      mark.style.left = `${(x.fraction * 100).toFixed(2)}%`;
    } else delete mark.dataset.at;
    mark.classList.toggle('off', !read);
    const text = box.querySelector(`[data-meter="${CSS.escape(x.key)}"]`);
    if (text && text.textContent !== x.text) text.textContent = x.text;
  });
  const state = box.querySelector('.sg-mstate');
  if (state) state.innerHTML = device.meters.state ? `<i>${esc(device.meters.state)}</i>` : '';
  return true;
}

/** The faults the amp reports: the facts' rows of the fault family (ids `230.…`: over-current,
 * over-temperature), each with its value, and the advice of the card's faults. */
function faults(device) {
  const rows = (device.facts?.rows ?? []).filter((r) => /^230\./.test(r.id));
  const advice = device.card?.faults ?? [];
  if (rows.length === 0 && advice.length === 0) return '';
  const list = rows.map((r) => `<dt>${esc(r.label)}</dt><dd data-fact="${esc(r.label)}"`
    + `${r.value === 'active' ? ' class="sg-bad"' : ''}>${esc(r.value)}</dd>`).join('');
  const tips = advice.map((x) => `<li><b>${esc(x.label)}</b> ${esc(x.advice)}</li>`).join('');
  return `<span class="sg-lbl">${esc(AMP.faults)}</span>${list ? `<dl class="sg-facts sg-faults">${list}</dl>` : ''}`
    + `${tips ? `<ul class="sg-advice">${tips}</ul>` : ''}`;
}

/** The third column: the amp's name, what its facts say first, Identify, and Unlink where a hop of
 * the line links it (`linkId`, the hop's id in the configuration). */
function about(device, linkId, tools) {
  const f = device.facts;
  const inert = Boolean(device.inert);
  const rows = f ? [['Firmware', f.firmware], ['Link', f.link]].filter(([, v]) => v !== undefined && v !== null)
    .map(([k, v]) => `<dt>${esc(k)}</dt><dd data-fact="${esc(k)}">${esc(v)}</dd>`).join('') : '';
  const identify = f?.identify_offered
    ? button('amp-identify', AMP.identify, !f.identify || inert, ' id="sg-amp-identify"', 'sg-btn sg-pill') : '';
  const unlink = linkId === null || linkId === undefined ? ''
    : button('amp-unlink', AMP.unlink, inert, ` id="sg-amp-unlink" data-hop-id="${esc(linkId)}"`, 'sg-btn sg-pill');
  return `<span class="sg-lbl">${esc(AMP.name(device.name))}</span>`
    + `${rows ? `<dl class="sg-facts">${rows}</dl>` : ''}${faults(device)}`
    + `<div class="sg-set">${identify}${unlink}</div><div class="sg-ask"></div>${tools}`;
}

/** The open area of the amp `device`: power, meters, the amp itself. `dac` is the name of the DAC
 * the line shows (for the note that the level is its), `linkId` the configuration's id of the hop
 * that links the amp; `tools` is what the hop's own buttons draw (its note, its moves). */
export function ampScreen(device, { dac, linkId, tools = '' }) {
  if (!device) return `<p class="sg-note">${esc(AMP.reading)}</p>`;
  const powerBox = power(device);
  return `<section id="sg-amp-power" class="sg-ampcol sg-box2">${powerBox}`
    + `<p class="sg-note">${esc(AMP.noVolume(dac))}</p></section>`
    + `<section id="sg-amp-meters" class="sg-ampcol">${meters(device)}</section>`
    + `<section id="sg-amp-about" class="sg-ampcol">${about(device, linkId, tools)}</section>`;
}

/** The open area of the DAC before a linked amp, which is not the device shown: its name and a button
 * that shows it. */
export function viaScreen(hop) {
  return `<section id="sg-via" class="sg-ampcol sg-box2"><span class="sg-lbl">${esc(AMP.name(hop.name))}</span>`
    + `<p class="sg-note">${esc((hop.words ?? []).join(' \u00b7 '))}</p>`
    + `<div class="sg-set">${button('show-device', AMP.showDevice(hop.name), false, ' id="sg-via-show"', 'sg-btn sg-pill')}</div></section>`;
}
