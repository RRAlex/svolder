/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What Night drive's drawing needs of the computer's side, read out of the core: the switch and, while
// the computer is the side shown, its volume and its tone control. `volume` and `tone` are worked out
// when first asked for (the tone's curve is the dear part).

import { inertOf } from './inert.js';

/**
 * The computer's view of the page `core` holds, or `null` where there is no switch (no device shown,
 * or no computer in the model): `{ side, sw, inert, volume, tone }`.
 */
export function computerView(core) {
  const page = core.model.page();
  const sw = core.computer.switchOf(page);
  if (!sw) return null;
  const on = sw.side === 'computer';
  let volume;
  let tone;
  return {
    side: sw.side,
    sw,
    inert: inertOf(page),
    get volume() {
      if (!on) return null;
      volume ??= core.computer.volumeOf(page);
      return volume;
    },
    get tone() {
      if (!on) return null;
      tone ??= core.computer.toneOf(page);
      return tone;
    },
  };
}
