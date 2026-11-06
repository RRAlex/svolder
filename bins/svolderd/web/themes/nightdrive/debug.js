/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's readout and its bisect switches, asked for on the address: `?nd-debug` shows the
// readout, `?nd-debug=noglow,nofloor` also turns the named layers off, to find on a phone what costs
// the most. The query rides through the core's own routing; a fragment does not (the core drops it
// at once). Nothing here touches the page.

/** The switches: each turns off one layer or effect. */
export const SWITCHES = Object.freeze({
  noglow: 'the glow: the sun\'s, the ridge\'s and the bubbles\', and the HUD\'s and dash\'s CSS shadows',
  nofloor: 'the floor\'s heat grid',
  nosky: 'the sky, the sun, the stars and the events',
  noridge: 'the mountains',
  nodash: 'the dash along the bottom',
  nohud: 'the top line, the volume and the EQ line',
  nomood: 'the mood: the night, the sun, the heat and the road\'s speed stay as they were, with no flare and no wave',
  nowet: 'the wet road: the sky and the mountains mirrored in the floor',
  noembers: 'the embers that rise from the floor at a peak',
  nohaze: 'the haze over the far floor',
  dpr1: 'the density: 1 backing pixel per CSS pixel',
});

/** What a query string asks: `null` for no readout, else `{ off: Set of switches, unknown: [words] }`. */
export function debugOf(search) {
  const params = new URLSearchParams(search ?? '');
  if (!params.has('nd-debug')) return null;
  const words = params.get('nd-debug').split(',').map((w) => w.trim().toLowerCase()).filter(Boolean);
  return {
    off: new Set(words.filter((w) => Object.hasOwn(SWITCHES, w))),
    unknown: words.filter((w) => !Object.hasOwn(SWITCHES, w)),
  };
}

/** What this page was opened with (read once, so a step to another device, which drops the query
 * from the address, leaves the readout on). */
export const ASKED = debugOf(globalThis.location?.search);
