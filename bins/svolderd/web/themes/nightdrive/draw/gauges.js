/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// An amp's gauges in the dash, where the input would be: its power and what it reads of its temperature,
// each a small neon arc with its words. The power fills the arc when it is on, half of it while it
// warms up and none in standby; a reading with a number in it and `temp` in its name fills the arc by that
// number over 100. Every other reading shows as words. No profile reports a temperature as a reading yet (an amp
// reports its over-temperature as a fault), so this arc is drawn only from a reading of that shape and is
// unproven on a real amp. Pure: the model in, markup out.

import { GAUGE } from '../words.js';
import { esc } from './esc.js';

/** The path of the arc, a half circle in a 32 by 20 box, its length set to 100 so a fill is a share. */
const ARC = '<path class="nd-arc-bg" d="M4 18A12 12 0 0 1 28 18" pathLength="100" fill="none"/>';

/** A gauge: the arc filled to `share` (0 to 1) and its `label` and `text`. */
function gauge(kind, label, text, share) {
  const fill = Math.round(Math.min(1, Math.max(0, share)) * 100);
  return `<span class="nd-gauge" data-gauge="${kind}" role="img" aria-label="${esc(`${label}: ${text}`)}">`
    + `<svg class="nd-arc" viewBox="0 0 32 20" width="32" height="20" aria-hidden="true" focusable="false">${ARC}`
    + `<path class="nd-arc-fg" d="M4 18A12 12 0 0 1 28 18" pathLength="100" fill="none" stroke-dasharray="${fill} 100"/></svg>`
    + `<i>${esc(label)}</i> ${esc(text)}</span>`;
}

/** The temperature of a readout: its number, or null where it has none or is not a temperature. */
function degrees(r) {
  if (!/temp/i.test(`${r.key} ${r.label}`)) return null;
  const n = Number.parseFloat(String(r.text).replace('\u2212', '-').replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

/** The gauges of the amp `device`: power, then its temperature and the other readings it holds. Nothing
 * for a device with no power. */
export function drawGauges(device) {
  const p = device?.power;
  if (!p) return '';
  const said = p.word ?? (p.on === true ? 'On' : p.on === false ? 'Standby' : '—');
  const warming = p.warming === true || said === 'Warming up';
  const out = [gauge('power', GAUGE.power, said, warming ? 0.5 : p.on === true ? 1 : 0)];
  for (const r of p.readouts ?? []) {
    if (r.key === 'warming_up') continue;
    const t = degrees(r);
    out.push(t === null
      ? `<span class="nd-gauge" data-gauge="reading"><i>${esc(r.label)}</i> ${esc(r.text)}</span>`
      : gauge('temperature', r.label || GAUGE.temperature, r.text, t / 100));
  }
  return out.join('');
}
