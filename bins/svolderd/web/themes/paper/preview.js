/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picture of the theme Paper the frame's menu and picker show: a sheet of graph paper with the
// name in print, a ruler with its red pencil needle and a line of blue handwriting, drawn as SVG
// that scales to the box it is given (the picker's is 160 by 104 px, the menu's a third of that).
// Always the light paper, whatever the page is wearing.

const W = 160;
const H = 104;

/** The grid: a line every 10 px each way. */
function grid() {
  let path = '';
  for (let x = 10; x < W; x += 10) path += `M${x} 0V${H}`;
  for (let y = 10; y < H; y += 10) path += `M0 ${y}H${W}`;
  return `<path d="${path}" fill="none" stroke="rgba(60,80,120,.1)" stroke-width="1"/>`;
}

/** The ruler's ticks: one every 6 % of the width, across y 40 to 50. */
function ticks() {
  let path = '';
  for (let i = 1; i * 0.06 < 1; i++) {
    const x = (i * 0.06 * W).toFixed(1);
    path += `M${x} 40V50`;
  }
  return `<path d="${path}" fill="none" stroke="#23201b" stroke-width="1"/>`;
}

/** Draw the picture of Paper into `box`. */
export function preview(box) {
  box.innerHTML = `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Paper" xmlns="http://www.w3.org/2000/svg">`
    + `<rect width="${W}" height="${H}" fill="#f3efe6"/>${grid()}`
    + '<text x="10" y="22" font-family="\'Crimson Pro\', Georgia, serif" font-size="15" fill="#23201b">svolder</text>'
    + `${ticks()}`
    + '<path d="M105.6 34V54" stroke="#b3261e" stroke-width="2" stroke-linecap="round" fill="none"/>'
    + '<text x="10" y="80" font-family="\'Caveat\', cursive" font-size="15" fill="#2b4a8a">warm: +3 dB at 60 Hz</text>'
    + '</svg>';
}
