/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where things stand on the sheet (core/place.js): the class-A share of a meter takes its fraction
// as its width from the scale's start.

import { placeFractions } from '../../core/place.js';

/** Put the marks of `root` where their `data-at` and `data-x`/`data-y` say. */
export const place = (root) => placeFractions(root, { widthClass: 'classa' });
