/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picture of the theme Night drive the frame's menu and picker show: the dusk's gradient, the
// striped sun, a cyan ridge and four lines of the road, drawn as SVG that scales to the box it is
// given (the picker's is 160 by 104 px, the menu's a third of that). It has no text: the card names
// the theme. Always on the dark ground, whatever the page is wearing.

import { PALETTE, thumbStripes } from './draw/palette.js';

const W = 160;
const H = 104;
const HORIZON = 58;

/** A number that tells the pictures on one page apart, so their gradients do not share ids. */
let drawn = 0;

/** Draw the picture of Night drive into `box`. */
export function preview(box) {
  const id = `nd-pv-${drawn++}`;
  const lines = [8, 52, 108, 152].map((x) => `<path d="M80 ${HORIZON}L${x} ${H}"/>`).join('');
  // fewer, thicker stripes than the scene's: they must show at the menu's 32 px (draw/palette.js)
  const stripes = thumbStripes().map(([above, thick]) => `<rect x="56" y="${(HORIZON - above - thick).toFixed(1)}" width="48" height="${thick.toFixed(1)}" fill="${PALETTE.sunStripe}"/>`).join('');
  box.innerHTML = `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Night drive" xmlns="http://www.w3.org/2000/svg">`
    + `<defs><linearGradient id="${id}-sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0a0018"/>`
    + '<stop offset="1" stop-color="#5b0f5e"/></linearGradient>'
    + `<linearGradient id="${id}-sun" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffd54a"/>`
    + '<stop offset=".5" stop-color="#ff8a3d"/><stop offset="1" stop-color="#ff2d78"/></linearGradient>'
    + `<clipPath id="${id}-clip"><rect width="${W}" height="${HORIZON}"/></clipPath></defs>`
    + `<rect width="${W}" height="${H}" fill="#0a0018"/><rect width="${W}" height="${HORIZON}" fill="url(#${id}-sky)"/>`
    + `<g clip-path="url(#${id}-clip)"><circle cx="80" cy="${HORIZON - 4}" r="24" fill="url(#${id}-sun)"/>`
    + `${stripes}</g>`
    + `<path d="M0 ${HORIZON}L0 46L14 40L26 49L40 36L54 47L70 42L90 50L108 38L124 48L140 41L160 47L160 ${HORIZON}Z" fill="#14002a" stroke="#22d3ee" stroke-width="1.5" stroke-linejoin="round"/>`
    + `<g stroke="#ff3cc8" stroke-opacity=".7" stroke-width="1" fill="none">${lines}</g>`
    + '</svg>';
}
