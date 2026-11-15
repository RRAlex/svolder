/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where things stand in the open area (core/place.js): the Now playing fill takes its fraction as
// its width.

import { placeFractions } from '../../core/place.js';

/** Put the marks of `root` where their `data-at` and `data-x`/`data-y` say. */
export const placeOpen = (root) => placeFractions(root, { widthClass: 'sg-fill' });
