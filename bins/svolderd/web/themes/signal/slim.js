/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The slim bar: when the line scrolls out of view a bar 64 px high pins at the top of the page with
// the same hops, their names only, the one chosen in a ringed pill, the device's volume beside its
// name. A tap on a hop chooses it and brings the page back to the top. The bar is for the desktop
// layout (a phone's strip pins itself); the frame's layer, with the menu button and the toasts, is
// above it.

import { esc } from './draw/esc.js';
import { WORDS } from './words.js';

/** The slim bar's buttons: a name each, the chosen one ringed, the shown device's with its volume
 * (`volume` is the words, or nothing). */
export function drawSlim(hops, chosen, shownId, volume) {
  return hops.map((hop) => {
    const words = hop.id === shownId && volume ? `${hop.name} · ${volume}` : hop.name;
    const cls = `sg-sh${hop.id === chosen ? ' sg-cur' : ''}${hop.live ? '' : ' sg-decl'}`;
    return `<button type="button" class="${cls}" data-slim="${esc(hop.id)}" `
      + `aria-current="${hop.id === chosen}" title="${esc(hop.name)}">${esc(words)}</button>`;
  }).join('');
}

/** The words of the shown device's volume for the bar: its step's words, or `−∞ dB` while muted. */
export const volumeWords = (device) => {
  const v = device?.volume;
  if (!v) return '';
  return v.muted ? `${WORDS.silence} ${v.unit || 'dB'}` : (v.text ?? '');
};

/**
 * Wire the bar `bar` over the band `band`: shown while the band is out of the viewport. A tap on a
 * hop calls `choose(id)` and scrolls to the top (at once under reduced motion). Returns nothing;
 * everything ends with `signal`.
 */
export function wireSlim({
  bar, band, choose, signal, win = globalThis.window,
}) {
  if (typeof win?.IntersectionObserver === 'function') {
    const watch = new win.IntersectionObserver((entries) => {
      const last = entries[entries.length - 1];
      bar.hidden = last.isIntersecting;
    });
    watch.observe(band);
    signal.addEventListener('abort', () => watch.disconnect(), { once: true });
  }
  bar.addEventListener('click', (event) => {
    const hop = event.target.closest?.('[data-slim]');
    if (!hop) return;
    choose(hop.dataset.slim);
    // Choosing redraws the open area and can shorten the page; read the height so the page has its
    // new length (and the scroll its clamped place) before the smooth scroll starts, as a layout
    // change under a running smooth scroll can end it where it stood.
    void win.document?.documentElement?.scrollHeight;
    const still = win.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
    win.scrollTo({ top: 0, behavior: still ? 'auto' : 'smooth' });
  }, { signal });
}
