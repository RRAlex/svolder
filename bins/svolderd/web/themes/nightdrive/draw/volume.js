/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The outrun volume: chrome-bevelled figures with a starry navy-to-magenta fill and a pink glow,
// the unit in pink brush script with a swoosh. Vector, drawn as SVG text with gradients; every
// look is a presentation attribute (no `style`), and the ids are Night drive's own.
//
// Russo One has neither the minus (U+2212) nor the infinity (U+221E), so those two are drawn as
// paths with the same fill, stroke and glow as the figures: a bar for the minus, a lying 8 of two
// loops for the infinity. The digits are placed by Russo One's advance widths, so the paths sit
// among them as glyphs would.

import { esc } from './esc.js';

const SIZE = 118;
const BASELINE = 112;
/** The middle of the digits, up from the baseline: where the bar and the 8 lie. */
const MID = 70;
const CENTRE = 335;

/** Russo One's advance widths for the figures, in em; a character it has no entry for is DEFAULT. */
const ADVANCE = {
  0: 0.67, 1: 0.405, 2: 0.59, 3: 0.61, 4: 0.65, 5: 0.6, 6: 0.63, 7: 0.56, 8: 0.67, 9: 0.63, '.': 0.24, ' ': 0.25,
};
const DEFAULT = 0.6;
/** The room a drawn minus and infinity take, in px. */
const MINUS_SLOT = 59;
const INFINITY_SLOT = 140;
const BAR = { w: 42, h: 14 };
const LOOP = { half: 52, rise: 40, bend: 22, width: 14 };

const STROKE = 'paint-order="stroke" stroke-linejoin="round"';
const FIGURE = `fill="url(#nd-stars)" stroke="url(#nd-chrome)" stroke-width="12" ${STROKE}`;
const SHEEN = 'fill="none" stroke="rgba(255,255,255,.55)" stroke-width="1.6"';

/** The figures cut into runs: a minus (the dash of a level not read yet is the same bar), an infinity, or text. */
function runsOf(figures) {
  const runs = [];
  for (const ch of figures) {
    if (ch === '−' || ch === '—') runs.push({ kind: 'minus', w: MINUS_SLOT });
    else if (ch === '∞') runs.push({ kind: 'infinity', w: INFINITY_SLOT });
    else if (runs.at(-1)?.kind === 'text') {
      runs.at(-1).text += ch;
      runs.at(-1).w += (ADVANCE[ch] ?? DEFAULT) * SIZE;
    } else runs.push({ kind: 'text', text: ch, w: (ADVANCE[ch] ?? DEFAULT) * SIZE });
  }
  return runs;
}

/** The minus: a bar, drawn as the glyph would be. */
function bar(x) {
  const x0 = x + (MINUS_SLOT - BAR.w) / 2;
  const d = `M${x0} ${MID - BAR.h / 2}h${BAR.w}v${BAR.h}h${-BAR.w}z`;
  return `<path d="${d}" ${FIGURE}/><path d="${d}" ${SHEEN}/>`;
}

/** The infinity: a lying 8 of two loops, a thick line (the fill) over a wider one (the chrome) with
 * a thin sheen between, which reads as the figures' outline and fill. */
function lying8(x) {
  const cx = x + INFINITY_SLOT / 2;
  const { half, rise, bend, width } = LOOP;
  const d = `M${cx} ${MID}C${cx + bend} ${MID - rise} ${cx + half} ${MID - rise} ${cx + half} ${MID}`
    + `C${cx + half} ${MID + rise} ${cx + bend} ${MID + rise} ${cx} ${MID}`
    + `C${cx - bend} ${MID + rise} ${cx - half} ${MID + rise} ${cx - half} ${MID}`
    + `C${cx - half} ${MID - rise} ${cx - bend} ${MID - rise} ${cx} ${MID}z`;
  const line = (stroke, w) => `<path d="${d}" fill="none" stroke="${stroke}" stroke-width="${w}" stroke-linejoin="round"/>`;
  return line('url(#nd-chrome)', width + 12) + line('rgba(255,255,255,.55)', width + 1.6) + line('url(#nd-stars)', width);
}

const DEFS = '<defs>'
  + '<linearGradient id="nd-chrome" gradientUnits="userSpaceOnUse" x1="0" y1="22" x2="0" y2="120">'
  + '<stop offset="0" stop-color="#ffffff"/><stop offset=".42" stop-color="#9fd8ff"/>'
  + '<stop offset=".5" stop-color="#3b4a8a"/><stop offset=".56" stop-color="#e8f3ff"/>'
  + '<stop offset="1" stop-color="#8aa0d6"/></linearGradient>'
  + '<linearGradient id="nd-fill" x1="0" y1="0" x2="0" y2="1">'
  + '<stop offset="0" stop-color="#16093f"/><stop offset=".5" stop-color="#3c1a8f"/>'
  + '<stop offset=".51" stop-color="#b12bd8"/><stop offset="1" stop-color="#ff3cc8"/></linearGradient>'
  + '<pattern id="nd-stars" width="60" height="60" patternUnits="userSpaceOnUse">'
  + '<rect width="60" height="60" fill="url(#nd-fill)"/>'
  + '<circle cx="8" cy="12" r="1.2" fill="#fff"/><circle cx="37" cy="30" r=".9" fill="#ffd6f7"/>'
  + '<circle cx="52" cy="8" r=".7" fill="#fff"/><circle cx="22" cy="48" r="1" fill="#fff" opacity=".8"/></pattern>'
  + '<filter id="nd-glow" x="-20%" y="-30%" width="140%" height="160%">'
  + '<feGaussianBlur stdDeviation="6" result="b"/><feFlood flood-color="#ff3cc8" flood-opacity=".75"/>'
  + '<feComposite in2="b" operator="in"/><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>'
  + '</filter></defs>';

/** The outrun volume for `v` (`volumeOf`'s `{ figures, unit, aria, dim }`) as an SVG, viewBox 640 by 170. */
export function outrunVolume(v) {
  const runs = runsOf(v.figures);
  const total = runs.reduce((sum, run) => sum + run.w, 0);
  const start = CENTRE - total / 2;
  let x = start;
  let drawn = '';
  for (const run of runs) {
    if (run.kind === 'minus') drawn += bar(x);
    else if (run.kind === 'infinity') drawn += lying8(x);
    else {
      const text = `<text x="${x.toFixed(1)}" y="${BASELINE}" text-anchor="start" font-family="'Russo One', sans-serif" `
        + `font-size="${SIZE}"`;
      drawn += `${text} ${FIGURE}>${esc(run.text)}</text>${text} ${SHEEN}>${esc(run.text)}</text>`;
    }
    x += run.w;
  }
  const ux = Math.round(start + total - 23);
  return `<svg class="nd-odb${v.dim ? ' nd-dim' : ''}" viewBox="0 0 640 170" overflow="visible" role="img" aria-label="${esc(v.aria)}">`
    + `${DEFS}<g filter="url(#nd-glow)" transform="skewX(-8)">${drawn}</g>`
    + `<g filter="url(#nd-glow)"><text x="${ux}" y="158" text-anchor="middle" font-family="'Mr Dafoe', cursive" `
    + `font-size="64" fill="#ff3cc8" transform="rotate(-6 ${ux} 150)">${esc(v.unit)}</text>`
    + `<path d="M${ux - 70} 166Q${ux} 158 ${ux + 90} 150" stroke="#ff3cc8" stroke-width="4" fill="none" `
    + 'stroke-linecap="round"/></g></svg>';
}
