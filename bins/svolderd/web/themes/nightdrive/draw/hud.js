/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD's markup: the EQ line and the amp's note. Pure: the words in, markup out.

import { esc } from './esc.js';

/** The EQ line's inside: the lead (`EQ`) plain, the name bright, the gains dimmer. */
export function eqMarkup(line) {
  if (!line) return '';
  const lead = line.lead ? `${esc(line.lead)} ` : '';
  const gains = line.gains.length ? ` <span class="nd-gains">${esc(line.gains.join(' '))}</span>` : '';
  return `${lead}<b>${esc(line.name)}</b>${gains}`;
}

/** What stands where the volume would for an amp with none of its own. */
export const noVolumeMarkup = (words) => `<p class="nd-novol">${esc(words)}</p>`;
