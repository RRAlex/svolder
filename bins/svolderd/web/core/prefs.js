/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What this browser keeps: a theme's own preferences, and the choice of theme with its light or
// dark. Storage is the browser's own and may refuse (a private window, blocked site data), so every
// access is guarded and the page works whole without it. The choice is also written as a cookie,
// which the server reads to paint the first frame in the theme chosen: `svolder_theme=<id>` or
// `<id>.light` / `<id>.dark`.

/** The storage key of the theme chosen. */
export const CHOICE_KEY = 'svolder.theme';
/** The cookie the server reads the choice from. */
export const COOKIE = 'svolder_theme';

/** `<id>` or `<id>.light` / `<id>.dark`: a theme id is lower-case letters, digits, `-` and `_`. */
const CHOICE = /^([a-z0-9][a-z0-9_-]*)(?:\.(light|dark))?$/;

/** `storage` (a Storage, or nothing) as reads that give `null` and writes that give `false`
 * where the browser refuses. */
export function guarded(storage) {
  return {
    read(key) {
      try {
        return storage?.getItem(key) ?? null;
      } catch {
        return null;
      }
    },
    write(key, text) {
      try {
        if (!storage) return false;
        storage.setItem(key, text);
        return true;
      } catch {
        return false;
      }
    },
    remove(key) {
      try {
        storage?.removeItem(key);
      } catch {
        // The page works without it.
      }
    },
  };
}

/** The preferences of the theme `themeId`: JSON values under `svolder.theme.<id>.<name>`. */
export function createPrefs(themeId, storage) {
  const keep = guarded(storage);
  const key = (name) => `svolder.theme.${themeId}.${name}`;
  return {
    get(name, fallback) {
      const text = keep.read(key(name));
      if (text === null) return fallback;
      try {
        return JSON.parse(text);
      } catch {
        return fallback;
      }
    },
    set: (name, value) => keep.write(key(name), JSON.stringify(value)),
    remove: (name) => keep.remove(key(name)),
  };
}

/** The scheme the server painted the first frame in (`'light'`, `'dark'`, or nothing painted) as
 * the person's word for the theme: the first byte is the choice, whatever storage says. */
export function bootScheme(painted) {
  return painted ?? 'auto';
}

/** The browser's storage, or nothing where it refuses even to be asked for. */
export function storageOf(window) {
  try {
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

/** `{ id, scheme }` out of a choice's text, or nothing for text that is not one. */
export function parseChoice(text) {
  const found = CHOICE.exec(text ?? '');
  return found ? { id: found[1], scheme: found[2] ?? null } : null;
}

/** The choice this browser holds: the cookie the page was rendered with first, then storage; or
 * nothing. `cookie` is `document.cookie`. */
export function readChoice({ storage, cookie }) {
  for (const pair of (cookie ?? '').split(';')) {
    const [name, ...value] = pair.trim().split('=');
    if (name === COOKIE) {
      const choice = parseChoice(value.join('='));
      if (choice) return choice;
    }
  }
  return parseChoice(guarded(storage).read(CHOICE_KEY));
}

/** Keep the choice in storage and in the cookie, for a year; `path` is the page's directory. */
export function writeChoice({ id, scheme }, { storage, document, path }) {
  const text = scheme ? `${id}.${scheme}` : id;
  guarded(storage).write(CHOICE_KEY, text);
  try {
    document.cookie = `${COOKIE}=${text}; Path=${path}; Max-Age=31536000; SameSite=Lax`;
  } catch {
    // The page works without it.
  }
}
