/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Which preset the bands are on, from the model's presets. No DOM.

/** The preset the bands are on: `{ name, edited }`, or `null` when none is on or recalled. The model
 * names the recalled preset after the bands were changed too, then with a status tail that says so. */
export function presetOf(presets) {
  const on = presets?.rows?.find((r) => r.on);
  if (on) return { name: on.name, edited: false };
  const name = presets?.status?.name;
  if (name === null || name === undefined || name === '') return null;
  return { name, edited: /\bedited\b/.test(presets.status.tail ?? '') };
}

/** A preset as a chip names it: `Warm`, or `Warm, edited`. */
export const presetName = (preset) => (preset.edited ? `${preset.name}, edited` : preset.name);
