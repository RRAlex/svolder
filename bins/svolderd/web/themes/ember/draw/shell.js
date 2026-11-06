/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember's shell: what stands around the screen, as the page's body holds it. The
// effects layer, the header (the wordmark, the device switcher with the shown device's link dot
// beside its name, a guest's Sign in, the empty slot the frame draws its menu button over), the
// empty `main#screen` the screen is drawn into, the history canvas, the footer and the list of
// keys. The server's frame carries the model's data block and the `<head>`. Pure, with no DOM.

import { attr, esc } from './html.js';

/** The place the frame draws its menu button over, at the header's right: empty, of the button's
 * size (style.css). */
const SLOT = '<span class="fxslot"></span>';

/** The keys a viewer who may change a device has. */
const WRITER_KEYS = '<p class="ktitle">Keys</p><dl> <dt><kbd>←</kbd> <kbd>→</kbd></dt><dd>Volume one step</dd> <dt><kbd>m</kbd></dt><dd>Mute</dd> <dt><kbd>0</kbd></dt><dd>Centre the balance</dd> <dt class="kviz"><kbd>v</kbd></dt><dd class="kviz">The visualizer\'s look</dd> <dt><kbd>f</kbd></dt><dd>Tone control to Flat</dd> <dt><kbd>p</kbd></dt><dd>The preset that is on</dd> <dt>Curve</dt><dd>Drag a point for frequency and gain; wheel, right drag or pinch for Q</dd> <dt><kbd>A</kbd></dt><dd>Arrange sections</dd> <dt><kbd>T</kbd></dt><dd>Themes</dd> <dt><kbd>Space</kbd></dt><dd>Play or pause</dd> <dt><kbd>?</kbd></dt><dd>This list</dd> </dl>';

/** The keys a guest, or a page with writes off, has: those that change nothing on the device. */
const READER_KEYS = '<p class="ktitle">Keys</p><dl> <dt class="kviz"><kbd>v</kbd></dt><dd class="kviz">The visualizer\'s look</dd> <dt><kbd>A</kbd></dt><dd>Arrange sections</dd> <dt><kbd>T</kbd></dt><dd>Themes</dd> <dt><kbd>?</kbd></dt><dd>This list</dd> </dl>';

/** The class the page's body carries: a guest's, a writer's, or none. */
export function bodyClass(page) {
  if (page.viewer.guest) return 'guest';
  return page.viewer.writes ? 'writes' : '';
}

/** One device in the switcher, or the only one, named. */
function entry(d, shown, only) {
  const words = `<b>${esc(d.name)}</b><small>${esc(d.family_word)}</small>`;
  if (only) return `<span class="dev ${esc(d.family)} one">${words}</span>`;
  const url = `./?device=${d.url}`;
  return `<a class="dev ${esc(d.family)}"${attr('href', url)}${attr('aria-current', d.url === shown ? 'page' : 'false')}${attr('data-device', d.url)}>${words}</a>`;
}

/** The shown device's link: a dot beside its name, and the state's word, which stays for a screen
 * reader and is shown whenever the link is not up (trouble is never shown by colour alone). */
const pill = (link) => `<span id="part-link" class="link ${esc(link.state)}" role="img"${attr('aria-label', link.word)}><i aria-hidden="true"></i><span class="lw">${esc(link.word)}</span></span>`;

/**
 * The body of the page of `page` around its screen. `wordmark` is the wordmark's SVG, which the
 * page holds as markup of its own.
 */
export function shell(page, wordmark = '') {
  const { viewer } = page;
  const devices = page.fleet.devices;
  const link = page.device ? page.device.link : null;
  const shown = devices.some((d) => d.url === page.shown);
  // Each device is a row of its own, so the link's dot stays with the name it follows.
  const switcher = devices.map((d) => `<span class="devrow">${entry(d, page.shown, devices.length === 1)}`
    + `${link && d.url === page.shown ? pill(link) : ''}</span>`).join('')
    + (link && !shown ? pill(link) : '');
  const side = viewer.guest ? '<a class="pill" href="./?sign-in">Sign in</a>' : '';
  const version = viewer.version ? `svolder ${esc(viewer.version)} web UI` : 'svolder web UI';
  const signOut = viewer.sign_out && viewer.version
    ? '<button type="button" id="sign-out" class="link">Sign out</button>' : '';
  const readOnly = !viewer.writes && !viewer.guest
    ? '<span class="ro">Read-only: <code>[web] writes</code> is off</span>' : '';
  const hints = viewer.writes
    ? '<p class="kh2" aria-hidden="true"><kbd>←</kbd><kbd>→</kbd> volume <kbd>m</kbd> mute <kbd>0</kbd> centre <kbd>f</kbd> Flat <kbd>p</kbd> presets <kbd>?</kbd> keys</p>'
    : '<p class="kh2" aria-hidden="true"><kbd>?</kbd> keys</p>';
  return `<div class="fx" aria-hidden="true"><div class="glow"></div></div> <div class="app"> <header class="top"> ${wordmark} <nav class="devs" aria-label="Devices"> ${switcher} </nav> <div class="side"> ${side} </div> ${SLOT} </header> <main id="screen"></main> <canvas class="hist" id="hist" aria-hidden="true"></canvas> <footer class="foot"> <span> ${version} </span> ${signOut} ${readOnly} ${hints} </footer> </div> <div class="keys" id="keys" hidden>${viewer.writes ? WRITER_KEYS : READER_KEYS}</div>`;
}
