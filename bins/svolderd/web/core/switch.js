/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme on show and the way to another: the first mount (with the choice the address and the
// browser hold), and a switch from the picker, which keeps the choice, loads the new theme's files,
// unmounts the theme on show and mounts the new one. A theme's files are its style sheets, its
// meta tags and its scripts (the page lists them for each theme); whichever theme ends up mounted,
// the fallback included, has them in place first. A theme that does not start is replaced by
// Ember (the registry says so in a popup) and the choice goes back to Ember; when Ember cannot
// start either, the page says so in a popup that stays, with a way to reload.

import { guarded, parseChoice, readChoice, storageOf, writeChoice } from './prefs.js';
import { mountTheme } from './registry.js';
import { paintChrome } from './scheme.js';

/** How long a style sheet or a script may take to load before the theme counts as not starting. */
const LOAD_MS = 5000;

/** What the daemon is asked to send for `theme`, as a string that is the same for the same asks. */
const needsOf = (theme) => [...(theme?.needs ?? [])].sort().join(',');

/** The switch of themes over `root` (the element themes are drawn into) for `core`. */
export function createSwitcher({ core, window, root, timeoutMs = LOAD_MS }) {
  const document = window.document;
  const storage = storageOf(window);
  let current = null;
  let busy = false;

  /** The light or dark to keep with theme `id`, if it has both and the person chose one: the
   * theme bound's own, else what its preferences hold. */
  function schemeOf(id) {
    const theme = core.registry.loaded(id);
    if ((theme?.schemes?.length ?? 1) < 2) return null;
    let mode = null;
    if (core.theme?.id === id) mode = core.scheme?.mode();
    else {
      try {
        mode = JSON.parse(guarded(storage).read(`svolder.theme.${id}.scheme`));
      } catch {
        mode = null;
      }
    }
    return mode === 'light' || mode === 'dark' ? mode : null;
  }

  /** Keep `id` as the choice, with the light or dark the person chose for it, if it has both. */
  function keep(id) {
    writeChoice({ id, scheme: schemeOf(id) },
      { storage, document, path: window.location.pathname });
  }

  /** The choice as it stands for the theme on show, written when this browser holds another: the
   * theme's own light or dark comes with it, whichever way it got here. */
  function settle() {
    const held = readChoice({ storage, cookie: document.cookie });
    if (held?.id !== current.id || held.scheme !== schemeOf(current.id)) keep(current.id);
  }

  /** Resolves true when `el` (just put in the page) has loaded, false when it cannot or takes
   * longer than the bound. */
  function loaded(el) {
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(false), timeoutMs);
      const done = (ok) => () => {
        clearTimeout(timer);
        resolve(ok);
      };
      el.addEventListener('load', done(true), { once: true });
      el.addEventListener('error', done(false), { once: true });
    });
  }

  /** The attribute `name` of the element of `tag` whose `key` is `value`, if the page has one. */
  const has = (tag, key, value) => [...document.querySelectorAll(tag)]
    .some((el) => el.getAttribute(key) === value);

  /**
   * The theme's files in the page: its meta tags first (a script may read them as it loads), then
   * its style sheets and its scripts, in order, each one the page does not already have. Resolves
   * true once each has loaded; when one cannot, or takes too long, what this call added is taken
   * out again and it resolves false.
   */
  async function load(entry) {
    const added = [];
    const put = (el) => {
      document.head.append(el);
      added.push(el);
      return el;
    };
    try {
      for (const { name, content } of entry.metas ?? []) {
        const there = [...document.querySelectorAll('meta[data-sheet]')]
          .some((el) => el.dataset.sheet === entry.id && el.getAttribute('name') === name);
        if (there) continue;
        const meta = document.createElement('meta');
        meta.setAttribute('name', name);
        meta.setAttribute('content', content);
        meta.dataset.sheet = entry.id;
        put(meta);
      }
      const waits = [];
      for (const href of entry.styles ?? []) {
        if (has('link[data-sheet]', 'href', `assets/${href}`)) continue;
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = `assets/${href}`;
        link.dataset.sheet = entry.id;
        waits.push(loaded(put(link)));
      }
      // The scripts one after the other, in the order the theme names them.
      const scripts = (async () => {
        for (const src of entry.scripts ?? []) {
          if (has('script[src]', 'src', `assets/${src}`)) continue;
          const script = document.createElement('script');
          script.src = `assets/${src}`;
          if (!(await loaded(put(script)))) return false;
        }
        return true;
      })();
      const results = await Promise.all([...waits, scripts]);
      if (results.every(Boolean)) return true;
    } catch (error) {
      console.warn(`svolder: the files of theme ${entry.id} did not load`, error);
    }
    for (const el of added) el.remove();
    return false;
  }

  /** Take the style sheets and meta tags of every theme but `id` out of the page. */
  function dropFiles(id) {
    for (const el of [...document.querySelectorAll('link[data-sheet]'),
      ...document.querySelectorAll('meta[data-sheet]')]) {
      if (el.dataset.sheet !== id) el.remove();
    }
  }

  /** The page's own words about the theme on show: the root's `data-theme`, the browser's colours. */
  function announce(id) {
    document.documentElement.dataset.theme = id;
    const scheme = core.scheme;
    paintChrome(document, core.registry.get(id), scheme?.effective() ?? 'dark',
      scheme?.mode() ?? 'auto');
  }

  /** The theme `id` goes on show, with its files in place; with `reopen`, the stream is asked
   * again when the new theme needs other feeds than the last. Throws when not even the fallback
   * can start. */
  async function mount(id, reopen) {
    const before = needsOf(core.theme);
    current = await mountTheme({
      registry: core.registry, id, root, core, toast: core.frame.toast, prepare: load,
    });
    announce(current.id);
    dropFiles(current.id);
    settle();
    if (reopen && needsOf(core.theme) !== before) core.stream.open();
    return current;
  }

  /** Not even the fallback could start: the page says so, with a way to try again. */
  function fail(error) {
    console.warn('svolder: no theme could start', error);
    root.replaceChildren();
    current = null;
    core.frame.toast('svolder could not draw the page.',
      { label: 'Reload', href: window.location.href }, { ms: 0 });
  }

  return {
    /** The theme on show, `{ id, unmount }`, or nothing before the first mount. */
    current: () => current,

    /**
     * Mount the theme the frame was drawn for (`requested`), then settle the choice: a valid
     * `?theme=` is kept, one that names nothing is told (an empty one is ignored, as the server
     * does), a rendered theme that is not the browser's stored choice becomes it, and a theme that
     * fell back to Ember leaves Ember as the choice. Resolves the theme mounted, or null when none
     * could start.
     */
    async start(requested) {
      const asked = new URLSearchParams(window.location.search).get('theme') || null;
      const wanted = asked === null ? null : parseChoice(asked)?.id;
      const listed = wanted !== null && wanted !== undefined && core.registry.get(wanted);
      if (asked !== null && !listed) core.frame.toast(`There is no theme “${asked}”.`);
      try {
        await mount(requested, false);
      } catch (error) {
        fail(error);
        return null;
      }
      const stored = readChoice({ storage, cookie: '' });
      if (listed || current.id !== requested || (stored && stored.id !== current.id)) {
        keep(current.id);
      }
      return current;
    },

    /** The person picked the theme `id` in the picker. Nothing happens before the first mount. */
    async to(id) {
      const entry = core.registry.get(id);
      if (!entry || !current || id === current.id || busy) return;
      busy = true;
      try {
        const before = current;
        keep(id);
        if (!(await load(entry))) {
          dropFiles(before.id);
          keep(before.id);
          core.frame.toast(`${entry.label} could not start here; showing ${core.theme.label}`);
          return;
        }
        core.frame.closeMenu(false);
        core.frame.closePicker();
        try {
          await before.unmount();
        } catch (error) {
          console.warn('svolder: the theme did not unmount cleanly', error);
        }
        root.replaceChildren();
        current = null;
        core.frame.fade(false);
        core.frame.anchor(null);
        try {
          await mount(id, true);
        } catch (error) {
          fail(error);
          return;
        }
        if (current.id !== id) keep(current.id);
        document.getElementById('fx-button')?.focus();
      } finally {
        busy = false;
      }
    },
  };
}
