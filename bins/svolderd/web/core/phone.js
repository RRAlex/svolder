/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The one width that says "phone": a window this wide or narrower gets the phone layout, in the
// frame's sheet (core/frame.css turns at the same number; `make theme-css-check` holds the two
// together) and in every theme that has one.

/** The widest window that is a phone, in CSS px. */
export const PHONE_MAX_PX = 719;
