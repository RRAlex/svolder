/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picture of the theme Signal the frame's menu and picker show: the line with five hops, the
// device in its box, drawn as SVG that scales to the box it is given (the picker's is 160 by 104
// px, the menu's a third of that). Always on the dark ground, whatever the page is wearing.

const W = 160;
const H = 104;
const Y = 40;
/** The hops' places across the picture. */
const XS = [16, 48, 80, 112, 144];

/** A number that tells the pictures on one page apart, so their gradients do not share ids. */
let drawn = 0;

/** Draw the picture of Signal into `box`. */
export function preview(box) {
  const id = `sg-pv-${drawn++}`;
  box.innerHTML = `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Signal" xmlns="http://www.w3.org/2000/svg">`
    + `<defs><radialGradient id="${id}" cx="50%" cy="70%" r="70%"><stop offset="0" stop-color="#0f2038"/>`
    + '<stop offset="1" stop-color="#08121f"/></radialGradient></defs>'
    + `<rect width="${W}" height="${H}" fill="url(#${id})"/>`
    + `<path d="M${XS[0]} ${Y}H${XS[2]}" stroke="#72e0a0" stroke-width="2" stroke-linecap="round" stroke-dasharray="1 5" fill="none"/>`
    + `<path d="M${XS[1]} ${Y}H${XS[2]}" stroke="#f0df7c" stroke-width="2" stroke-linecap="round" stroke-dasharray="1 5" fill="none"/>`
    + `<path d="M${XS[2]} ${Y}H${XS[4]}" stroke="#4a7aa8" stroke-width="1.5" stroke-linecap="round" stroke-dasharray="1 5" fill="none"/>`
    + `<rect x="${XS[2] - 19}" y="${Y - 16}" width="38" height="46" rx="7" fill="rgba(79,214,240,.07)" stroke="#4fd6f0"/>`
    + `<circle cx="${XS[0]}" cy="${Y}" r="3.5" fill="#72e0a0"/><circle cx="${XS[1]}" cy="${Y}" r="3.5" fill="#f0df7c"/>`
    + `<circle cx="${XS[2]}" cy="${Y}" r="4.5" fill="#4fd6f0"/>`
    + `<circle cx="${XS[3]}" cy="${Y}" r="3" fill="#08121f" stroke="#4a7aa8"/><circle cx="${XS[4]}" cy="${Y}" r="3" fill="#08121f" stroke="#4a7aa8"/>`
    + '<text x="12" y="90" font-family="system-ui, sans-serif" font-size="11" fill="#e9f2ff">signal</text>'
    + '</svg>';
}
