/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picture of the theme Void Garden the frame's menu and picker show: a night valley under a
// dark sky, three ridge lines, a river running down the middle and a few beacon dots, drawn as SVG
// that scales to the box it is given (the picker's is 160 by 104 px, the menu's a third of
// that). It has no text: the card names the theme. Always on the dark ground, whatever the page is
// wearing.

const W = 160;
const H = 104;

/** A number that tells the pictures on one page apart, so their gradients do not share ids. */
let drawn = 0;

/** The beacons: `[x, y, radius]` in the picture's units, on the ridges and by the river. */
const BEACONS = [[34, 62, 1.8], [58, 52, 1.4], [112, 58, 2], [134, 68, 1.5], [92, 76, 1.2]];

/** Draw the picture of Void Garden into `box`. */
export function preview(box) {
  const id = `vg-pv-${drawn++}`;
  const dots = BEACONS.map(([x, y, r]) => `<circle cx="${x}" cy="${y}" r="${r}" fill="#7ff3d0"/>`
    + `<circle cx="${x}" cy="${y}" r="${r * 3}" fill="url(#${id}-glow)"/>`).join('');
  box.innerHTML = `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Void Garden" xmlns="http://www.w3.org/2000/svg">`
    + `<defs><linearGradient id="${id}-sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#030406"/>`
    + '<stop offset="1" stop-color="#0d1a2b"/></linearGradient>'
    + `<radialGradient id="${id}-glow"><stop offset="0" stop-color="#7ff3d0" stop-opacity=".45"/>`
    + '<stop offset="1" stop-color="#7ff3d0" stop-opacity="0"/></radialGradient></defs>'
    + `<rect width="${W}" height="${H}" fill="#030406"/><rect width="${W}" height="62" fill="url(#${id}-sky)"/>`
    + '<path d="M0 58L0 44L18 38L36 46L58 34L80 44L104 36L128 45L146 38L160 43L160 62L0 62Z" fill="#08111c" stroke="#2c5a7a" stroke-width="1" stroke-linejoin="round"/>'
    + '<path d="M0 70L0 56L22 50L46 58L70 49L96 58L120 52L144 59L160 54L160 104L0 104Z" fill="#060d16" stroke="#3d7a9a" stroke-width="1" stroke-linejoin="round"/>'
    + '<path d="M0 90L0 78L30 70L64 76L100 68L134 75L160 72L160 104L0 104Z" fill="#04080f" stroke="#4f9ab8" stroke-width="1" stroke-linejoin="round"/>'
    + '<path d="M82 62C76 70 88 74 80 82C72 90 86 96 78 104" fill="none" stroke="#3fb6d8" stroke-width="2.2" stroke-linecap="round" stroke-opacity=".8"/>'
    + `${dots}</svg>`;
}
