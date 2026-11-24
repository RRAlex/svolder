/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD's markup: the EQ line and the amp's note. Pure: the words in, markup out.

import { HUD } from '../words.js';
import { esc } from './esc.js';

/** One word of the EQ line that opens the Sound popup at its tab (`tap`), else plain text. */
const word = (text, stage, kind, tap) => (tap && stage
  ? `<button type="button" id="nd-lw-${esc(stage)}" class="nd-lw" data-act="stage" data-stage="${esc(stage)}"${kind ? ` data-kind="${esc(kind)}"` : ''}`
    + ` aria-label="${esc(HUD.stageOpens(text))}">${esc(text)}</button>`
  : esc(text));

/** The EQ line's inside: the lead (`EQ`) plain, the name bright, the gains dimmer, then ` · ` and the stage words
 * (`SPACE −7.0 · −14.8 dB`, one nowrap piece each; the stage words are buttons while `tap` says the viewer may
 * write), and the fit's number (`line.fit`) where there is one. */
export function eqMarkup(line, tap = false) {
  if (!line) return '';
  const lead = line.lead ? `${esc(line.lead)} ` : '';
  const name = tap && line.levels ? word(line.name, line.stage, null, tap) : esc(line.name);
  const gains = line.gains.length ? ` <span class="nd-gains">${esc(line.gains.join(' '))}</span>` : '';
  const levels = line.levels?.length ? ` · <span class="nd-levels">${line.levels
    .map((l) => `<span class="nd-lv">${word(l.text, l.stage, l.kind, tap)}</span>`).join(' · ')}</span>` : '';
  const fit = line.fit
    ? ` <span class="nd-fit" title="${esc(line.note ?? '')}" aria-hidden="true">${esc(line.fit)}</span>`
      + `<span class="nd-sr">${esc(line.note ?? line.fit)}</span>` : '';
  return `${lead}<b>${name}</b>${gains}${levels}${fit}`;
}

/** What stands where the volume would for an amp with none of its own. */
export const noVolumeMarkup = (words) => `<p class="nd-novol">${esc(words)}</p>`;
