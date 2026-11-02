/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Light and dark for a theme: it follows the system's setting until a person says otherwise, and
// the person's word is kept per theme. A theme with one scheme ignores both. The scheme in force
// is on the root as `data-scheme` (not for a theme with one scheme: the page names none), and
// `onCookie` hears the person's word (`'light'`, `'dark'`, or `null` for auto) to keep it where the
// server reads it.

const MODES = ['auto', 'light', 'dark'];

/** The browser's colours for a theme's `entry` (`{color_scheme, theme_color, theme_color_dark}` of
 * the themes list) under the scheme `effective` and the person's `mode`: the colour scheme the
 * browser draws its own parts in (both, while the person leaves it to the system) and the colour of
 * its toolbar. */
export function chromeOf(entry, effective, mode) {
  return {
    colorScheme: mode === 'auto' ? entry.color_scheme : effective,
    themeColor: effective === 'dark' && entry.theme_color_dark
      ? entry.theme_color_dark : entry.theme_color,
  };
}

/** Put `chromeOf` into `document`'s meta tags: the colour scheme, and every theme colour (the page
 * may carry one for each scheme of the system, which no longer applies once the scheme is
 * known). */
export function paintChrome(document, entry, effective, mode) {
  if (!entry) return;
  const { colorScheme, themeColor } = chromeOf(entry, effective, mode);
  const scheme = document.querySelector('meta[name="color-scheme"]');
  if (colorScheme) scheme?.setAttribute('content', colorScheme);
  if (!themeColor) return;
  for (const meta of document.querySelectorAll('meta[name="theme-color"]')) {
    meta.removeAttribute('media');
    meta.setAttribute('content', themeColor);
  }
}

/** The scheme of a theme with `schemes` over `root`, remembered in `prefs`, following `matchMedia`. */
export function createScheme({ matchMedia, root, prefs, schemes, onCookie }) {
  const system = matchMedia('(prefers-color-scheme: dark)');
  const heard = new Set();
  const modeHeard = new Set();
  const stored = prefs.get('scheme', 'auto');
  let chosen = MODES.includes(stored) ? stored : 'auto';
  let shown = null;

  /** The scheme in force: the theme's only one, else the person's, else the system's. */
  function effective() {
    if (schemes.length === 1) return schemes[0];
    const want = chosen === 'auto' ? (system.matches ? 'dark' : 'light') : chosen;
    return schemes.includes(want) ? want : schemes[0];
  }

  function apply() {
    const now = effective();
    if (schemes.length > 1) root.dataset.scheme = now;
    else delete root.dataset.scheme;
    if (now === shown) return;
    shown = now;
    for (const fn of [...heard]) fn(now, chosen);
  }

  const follow = () => apply();
  system.addEventListener('change', follow);
  apply();

  return {
    mode: () => chosen,
    effective,

    /** Hold `mode` ('auto', 'light' or 'dark'): kept, applied and heard. A theme with one scheme
     * has nothing to choose, and a word that is not a mode changes nothing. */
    set(mode) {
      if (!MODES.includes(mode) || schemes.length === 1) return;
      chosen = mode;
      prefs.set('scheme', mode);
      onCookie?.(mode === 'auto' ? null : mode);
      const before = shown;
      apply();
      if (shown === before) for (const fn of [...modeHeard]) fn(shown, chosen);
    },

    /** Hear the scheme in force (and the mode) whenever the scheme changes; returns the way out.
     * With `withMode`, also hear a change of the mode that leaves the scheme as it was (for what
     * shows the mode itself, as the browser's colour scheme does). */
    on(fn, { withMode = false } = {}) {
      heard.add(fn);
      if (withMode) modeHeard.add(fn);
      return () => {
        heard.delete(fn);
        modeHeard.delete(fn);
      };
    },

    stop() {
      system.removeEventListener('change', follow);
      heard.clear();
      modeHeard.clear();
    },
  };
}
