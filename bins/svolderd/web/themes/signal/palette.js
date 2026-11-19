/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Signal's palettes: the viewer's choice of colours for the whole theme, kept by this browser
// like the theme's other view choices (`core.prefs`, the key `palette`). A palette is a set of
// values of the `--sg-*` tokens in style.css, chosen by the `data-palette` attribute of the root;
// Blueprint, the default, is no attribute; a person who never chose gets it, and a kept choice
// (Warm amber too) stays. The rows of the frame's menu are built here from the list, and nothing
// else in the theme knows which palettes there are.

/** The palettes, in the order of the menu: the id (the value of `data-palette`) and the name. */
export const PALETTES = [
  { id: 'blueprint', label: 'Blueprint' },
  { id: 'amber', label: 'Warm amber' },
  { id: 'phosphor', label: 'Phosphor' },
  { id: 'oxblood', label: 'Oxblood and brass' },
];

/** The default palette. */
export const DEFAULT_PALETTE = PALETTES[0].id;

/** The heading of the menu's palette rows. */
export const PALETTE_HEADING = 'Palette';

/** `id` when it names a palette, else the default (a stored value may be anything). */
export const paletteOf = (id) => (PALETTES.some((one) => one.id === id) ? id : DEFAULT_PALETTE);

/** Dress the page in the palette `id` (the root of `doc`): the attribute is the palette's id, and
 * none for the default. */
export function applyPalette(doc, id) {
  const root = doc.documentElement;
  const chosen = paletteOf(id);
  if (chosen === DEFAULT_PALETTE) delete root.dataset.palette;
  else root.dataset.palette = chosen;
}

/** Take the palette off the page (the theme is leaving). */
export function clearPalette(doc) {
  delete doc.documentElement.dataset.palette;
}

/** The menu's rows for the palettes: a heading and a radio for each. `state` holds the palette
 * now (`state.palette`), `choose(id)` takes a new one, `signal` is the mount's: nothing is
 * chosen once it has gone. */
export function paletteRows({ state, choose, signal }) {
  return [
    { kind: 'heading', text: PALETTE_HEADING },
    ...PALETTES.map(({ id, label }) => ({
      kind: 'radio',
      id: `sg-palette-${id}`,
      label,
      dataset: { palette: id },
      checked: () => !signal.aborted && state.palette === id,
      disabled: () => null,
      choose() {
        if (!signal.aborted) choose(id);
      },
    })),
  ];
}
