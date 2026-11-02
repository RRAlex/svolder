/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The picture of the theme Ember the frame's menu and picker show: a dark page with the dial's
// amber ring, its glow behind and a row of meter bars, drawn as SVG that scales to the box it is
// given (the picker's is 160 by 104 px, the menu's a third of that).

/** A number that tells the pictures on one page apart, so their gradients do not share ids. */
let drawn = 0;

/** Draw the picture of Ember into `box`. */
export function preview(box) {
  const id = `ember-preview-${drawn++}`;
  const bars = [10, 18, 26, 34, 44, 38, 28, 20, 14]
    .map((h, i) => `<rect x="${14 + i * 15}" y="${96 - h * 0.6}" width="8" height="${h * 0.6}" rx="2" fill="#dd762f" opacity="${0.35 + i * 0.05}"/>`)
    .join('');
  box.innerHTML = `<svg viewBox="0 0 160 104" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Ember" xmlns="http://www.w3.org/2000/svg">`
    + `<defs><radialGradient id="${id}" cx="50%" cy="42%" r="55%">`
    + '<stop offset="0" stop-color="#8b5cf6" stop-opacity=".55"/>'
    + '<stop offset="1" stop-color="#8b5cf6" stop-opacity="0"/></radialGradient></defs>'
    + '<rect width="160" height="104" fill="#0f0d0b"/>'
    + `<rect width="160" height="104" fill="url(#${id})"/>`
    + '<circle cx="80" cy="44" r="26" fill="none" stroke="#2c261f" stroke-width="6"/>'
    + '<path d="M58.5 59.5A26 26 0 1 1 101.5 59.5" fill="none" stroke="#f0a73a" stroke-width="6" stroke-linecap="round"/>'
    + '<circle cx="80" cy="44" r="4" fill="#f8d67c"/>'
    + bars
    + '</svg>';
}
