/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Whether the device's facts are open. The last choice is held here, in memory, so it holds when the
// browser refuses storage (a private window, blocked site data); the stored word (`open` or
// `closed`, under `svolder.theme.signal.facts`) is only the starting point, and every choice is
// written to it as well.

/** The fold over `prefs()` (the theme's preferences now, or nothing before there are any): `open`
 * and `toggle()`. A store that throws or keeps nothing changes nothing but what is remembered. */
export function createFold(prefs) {
  let chosen = null;
  const stored = () => {
    try {
      return prefs()?.get('facts', 'closed') === 'open';
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
        prefs()?.set('facts', chosen ? 'open' : 'closed');
      } catch {
        // kept in memory only
      }
      return chosen;
    },
  };
}
