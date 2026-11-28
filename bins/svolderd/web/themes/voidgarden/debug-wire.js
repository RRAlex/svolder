/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The readout's box in the page: a small fixed box, drawn twice a second, made only when the
// address asks for it (debug.js). Its words are the scene's own numbers, for someone with a phone
// and no console. The parts a switch turned off are marked on the page (`vg-no-dash`, ...).

import { readoutLines } from './debug.js';

/** How often the box is drawn again, ms. */
export const EVERY_MS = 500;

/**
 * Make the box in `page` for the address's `asked` (debug.js `debugOf`); returns `{ stop() }`.
 * `perf()` is the scene's (`driver.perf`), `canvas` the scene's canvas, `clock()` the page's ms
 * (handed in).
 */
export function wireDebug({
  page, asked, perf, canvas, clock,
}) {
  for (const part of asked.parts) page.classList.add(`vg-no-${part}`);
  const box = page.ownerDocument.createElement('div');
  box.id = 'vg-debug';
  box.className = 'vg-debug';
  box.setAttribute('aria-hidden', 'true');
  // before the menu button's slot, which stays last in the page
  (page.querySelector('#vg-slot') ?? page.lastChild)?.before(box);
  let before = { at: clock(), draws: perf()?.draws ?? 0 };
  let fps = 0;

  function draw() {
    const now = clock();
    const read = perf();
    const seconds = (now - before.at) / 1000;
    if (seconds > 0 && read) {
      fps = (read.draws - before.draws) / seconds;
      before = { at: now, draws: read.draws };
    }
    box.textContent = readoutLines({
      perf: read,
      fps,
      canvas: [canvas.width, canvas.height],
      dpr: `${(globalThis.devicePixelRatio ?? 1).toFixed(2)} -> ${(read?.scale ?? 1).toFixed(2)}`,
      switches: asked.switches,
    }).join('\n');
  }

  draw();
  const timer = globalThis.setInterval(draw, EVERY_MS);
  return {
    stop() {
      globalThis.clearInterval(timer);
      box.remove();
      for (const part of asked.parts) page.classList.remove(`vg-no-${part}`);
    },
  };
}
