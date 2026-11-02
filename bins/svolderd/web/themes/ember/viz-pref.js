/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A classic script in the page's head, run before the first paint: when this browser switched the
// visualizer off (visualizer.js keeps that under the same key), the page says so on its root
// element at once, and the style sheet leaves the card out of the layout; a page that waited for
// the module would draw the card and then lose it. The module takes over once it runs. Every
// access is guarded: a browser without storage shows the card.
try {
  const kept = JSON.parse(window.localStorage.getItem('svolder.viz.2'));
  if (kept && kept.on === false) document.documentElement.dataset.viz = 'off';
} catch {
  // No storage: the card shows.
}
