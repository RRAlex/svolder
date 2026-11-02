/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the theme Ember's modules reach the page through: the core it was given, and the signal of
// the mount it is in (aborted when the theme is unmounted, so every listener that carries it
// goes). `ember.js` fills it as it mounts; the modules read it when they are used, never when they
// are loaded.

import { orderFor } from './order.js';

/** The core and the mount's signal, `null` until Ember is mounted. */
export const ember = { core: null, signal: null };

/** The device shown, as its URL segment, or nothing. */
export const shown = () => ember.core?.model.shown() ?? undefined;

/** The options of a listener that goes when the theme is unmounted. */
export const mine = (options = {}) => ({ ...options, signal: ember.signal ?? undefined });

/** The order being arranged, while Arrange mode is on (arrange.js sets it); nothing otherwise. */
export const live = { order: null };

/** The order the screen of `device` is drawn in: the one being arranged, else the one this browser
 * kept for its layout, else the layout's own. */
export const orderOf = (device) => live.order
  ?? orderFor(ember.core?.prefs, device?.layout ?? 'dac');
