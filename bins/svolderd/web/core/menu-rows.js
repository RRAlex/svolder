/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The rows of the frame's menu, as data: which rows there are and in what order, for a viewer and
// the rows the theme on show gives. Nothing here touches the page: menu.js draws the rows and runs
// them, and hands in what a row's `run` reaches (`run.picker`, `run.signOut`, `run.popup`). No row
// names a device: another one is shown from the Devices popup. A row every theme's menu has is
// added here, in `frameRows`; a popup's row is not written here: it comes from the popup's entry
// in the page's list of popups (popups/listed.js), so the row and the popup are one entry.
//
// The shape of a row is menu.js's (heading, radio, check, action, sep). Two fields are the frame's
// own: `go` (the row leads on: a `›` after its words) and `picture`, which marks the Theme row that
// menu.js gives the theme's small preview.

import { hasRow, rowId, seenBy } from './popups/listed.js';

/** The rows the frame adds to every theme's: a row for each of `popups` the viewer sees (the
 * entries of the page's list, in its order, but those listed with `menu: false`, which a theme
 * opens itself; `run.popup(name)` opens one), then Sign out or Sign in. `viewer` is the page's
 * (`guest`, `writes`, `sign_out`). */
export function frameRows({ viewer, run, popups = [] }) {
  const out = popups.filter((popup) => hasRow(popup) && seenBy(popup, viewer)).map((popup) => ({
    kind: 'action',
    label: popup.row.label,
    elementId: rowId(popup.name),
    go: true,
    run: () => run.popup(popup.name),
  }));
  if (viewer?.guest) {
    out.push({
      kind: 'action', tag: 'a', label: 'Sign in', href: './?sign-in', elementId: 'menu-sign-in',
      run() {},
    });
  } else if (viewer?.sign_out) {
    out.push({
      kind: 'action', label: 'Sign out', elementId: 'menu-sign-out',
      run: () => run.signOut(),
    });
  }
  return out;
}

/** Every row of the menu, in order: the Theme row (it opens the picker), the rows `given` by the
 * theme on show, then, when the frame has rows of its own, a rule and those rows. */
export function menuRows({
  theme, given, viewer, run, popups = [],
}) {
  const own = frameRows({ viewer, run, popups });
  return [
    {
      kind: 'action', label: `Theme: ${theme?.label ?? ''}`, elementId: 'menu-theme', go: true,
      picture: true, run: run.picker,
    },
    ...given,
    ...(own.length > 0 ? [{ kind: 'sep', own: true }] : []),
    ...own,
  ];
}
