/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What Ember's drawing needs of the computer's side, read out of the core: the switch, and, while
// the computer is the side shown, its volume, its tone control and the point picked in the table.
// The pick is this page's own (the row highlighted and scrolled into view); it goes when the
// device shown changes or the side flips.

/** The point picked in the table (1-based) and the device it was picked on. */
export const pick = { n: null, device: null };

/**
 * The computer's view on the page `core` holds, or `null` where there is no switch (no device
 * shown, or no computer in the model): `{ side, sw, volume, tone, writes, picked }`; `volume` and
 * `tone` are set on the computer side only.
 */
export function computerView(core) {
  const page = core.model.page();
  const sw = core.computer.switchOf(page);
  if (!sw) return null;
  const side = core.computer.side();
  const shown = core.model.shown();
  if (pick.device !== shown || side !== 'computer') {
    pick.n = null;
    pick.device = shown;
  }
  const on = side === 'computer';
  return {
    side,
    sw,
    volume: on ? core.computer.volumeOf(page) : null,
    tone: on ? core.computer.toneOf(page) : null,
    writes: Boolean(page.viewer?.writes),
    picked: pick.n,
  };
}
