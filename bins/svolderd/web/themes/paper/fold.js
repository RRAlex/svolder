/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Whether `The device` is open. The last choice is held here, in memory, so it holds when the
// browser refuses storage (a private window, blocked site data); the stored value is only the
// starting point, and every choice is written to it as well.

/** The fold over `prefs()` (the theme's preferences now, or nothing before there are any): `open`
 * and `toggle()`. A store that throws or keeps nothing changes nothing but what is remembered. */
export function createFold(prefs) {
  let chosen = null;
  const stored = () => {
    try {
      return prefs()?.get('deviceOpen', false) === true;
    } catch {
      return false;
    }
  };
  return {
    get open() {
      return chosen ?? stored();
    },
    toggle() {
      chosen = !(chosen ?? stored());
      try {
        prefs()?.set('deviceOpen', chosen);
      } catch {
        // kept in memory only
      }
      return chosen;
    },
  };
}
