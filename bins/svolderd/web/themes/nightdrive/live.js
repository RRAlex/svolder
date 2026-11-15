/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The parts of the shown device each section shows. The page is kept up to date by the core's
// `core.freeze.createLive({ core, signal, view, sections, device })`: a section is
// `{ parts, draw(page, device) }`,
// and one in use is not drawn again under the person's hand (`freeze(name)` keeps it, and what came
// meanwhile is drawn from the newest model at `thaw(name)`). The sheet's holds (a value typed, a
// list open, a question asked, a stepper held) are counted by `core.freeze.keeper`. What is in the
// person's hand and belongs to the device shown (a drag on the mountains) hears `onSwitch` and lets
// go.

/** The parts of the shown device each section shows, by the model's names. */
export const PARTS_OF = {
  hud: ['link', 'card', 'volume', 'controls', 'tone', 'presets', 'playing'],
  dash: ['link', 'card', 'volume', 'controls', 'power', 'tone', 'playing'],
  sheet: ['link', 'card', 'controls', 'power', 'tone', 'presets', 'device_presets', 'facts'],
};
