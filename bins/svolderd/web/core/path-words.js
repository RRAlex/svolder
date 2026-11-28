/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the path names about the sound, as structure and no words of a theme's own: which stages of
// the computer's filter chain the path names, whether the computer changes the sound, and the DAC's
// own stage. The words are the daemon's (`path.words` and the hops' `detail`, from the Path judge of
// svolder-ipc); this module only picks them out of the page's model, so every theme that reads the
// path reads it one way. Pure functions of a page: no state, no service.

/** The path's words that name a stage of the computer's filter chain: the words `chain_words` in
 * crates/svolder-ipc/src/host/path.rs gives them; change both together. */
export const STAGE_WORDS = [
  'host EQ on', 'loudness on', 'crossfeed on', 'virtual speakers on', 'EQ pass-through',
];

/** The words of the DAC hop's detail that name the DAC's own stage: the words `DacStage::hop_words`
 * in crates/svolder-ipc/src/host/path.rs gives them (a volume reads `volume -3.3 dB`); change both
 * together. */
export const DAC_WORDS = [
  'tone control', 'balance off centre', 'muted', 'phase inverted', 'NOS filter',
];

/**
 * What the path says of the computer: `{ eq, changes, stages }`. `eq` is whether the route runs the
 * host EQ; `changes` whether the computer changes the sound (the `computer` hop's own flag, else
 * the route's host EQ when the path has no such hop); `stages` the stages of the filter chain the
 * path names, in its order (empty with no host EQ, or when the path names none: the theme then
 * says its fixed words).
 */
export function computerOf(page) {
  const hop = page?.device?.playing?.path?.hops?.find((h) => h.label === 'computer');
  const eq = page?.host?.route?.host_eq === true;
  const changes = hop ? hop.changes === true : eq;
  const stages = eq
    ? (page?.device?.playing?.path?.words ?? []).filter((word) => STAGE_WORDS.includes(word))
    : [];
  return { eq, changes, stages };
}

/** The DAC's own stage the path names, each word as the daemon wrote it (`volume -3.3 dB`, `tone
 * control`); empty when the DAC does nothing of its own to the sound. */
export function dacOf(page) {
  const hop = page?.device?.playing?.path?.hops?.find((h) => h.label === 'DAC');
  return String(hop?.detail ?? '').split(', ')
    .filter((word) => word.startsWith('volume ') || DAC_WORDS.includes(word));
}

/** Whether the path changes the sound anywhere the page can tell: the computer does, or the DAC has
 * a stage of its own. */
export function changedOf(page) {
  return computerOf(page).changes || dacOf(page).length > 0;
}
