/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The themes the page can wear. The page is told each theme's id, label, blurb, module and
// schemes; a theme's module is loaded when it is first wanted and registers itself when it is
// evaluated (`registerTheme`). A theme that cannot be loaded, says it cannot run here or throws
// while it mounts is replaced by the fallback (Ember) and the person is told.
//
// A theme is `{ id, label, needs, schemes, supported?(), mount(root, core), preview?(box) }`;
// `mount` returns `{ unmount() }` (or the unmount function itself). `mount` is given a third
// argument, `{ signal }`, an AbortSignal that is aborted when the mount throws and when the theme
// is unmounted, so what the theme listens to goes with it.

/** The themes registered by modules as they were evaluated, until a registry takes them. */
const registered = [];

/** A theme's module calls this when it is evaluated. */
export function registerTheme(theme) {
  registered.push(theme);
}

/** The module of a listed theme, loaded relative to the core. */
const importEntry = (entry) => import(new URL(`../${entry.module}`, import.meta.url).href);

/** The registry of the themes `list` names (`[{id, label, blurb, module, schemes}]`); `importer`
 * loads a listed theme's module (the browser's `import` by default); `beforeMount(theme)` is
 * called as a theme is about to mount (the core binds the theme's preferences there) and
 * `afterMount(theme)` once it is on show (the frame learns of its menu rows there). */
export function createRegistry(list, { importer = importEntry, beforeMount, afterMount } = {}) {
  const entries = new Map(list.map((entry) => [entry.id, entry]));
  const themes = new Map();

  /** Take the themes modules registered while they were evaluated. */
  function adopt() {
    for (const theme of registered.splice(0)) themes.set(theme.id, theme);
  }

  return {
    beforeMount,
    afterMount,
    list: () => [...entries.values()],
    get: (id) => entries.get(id),

    /** Add a theme that is already loaded (a check's probe), listed if it was not. */
    add(theme) {
      themes.set(theme.id, theme);
      if (!entries.has(theme.id)) {
        entries.set(theme.id, {
          id: theme.id,
          label: theme.label ?? theme.id,
          blurb: theme.blurb ?? '',
          module: null,
          schemes: theme.schemes ?? ['dark'],
        });
      }
    },

    /** The theme if its module has been loaded, else `null`. */
    loaded: (id) => themes.get(id) ?? null,

    /** The theme `id`, loading its module the first time. Rejects for an id not listed, a module
     * that does not load and one that registers no theme of that id. */
    async load(id) {
      if (themes.has(id)) return themes.get(id);
      const entry = entries.get(id);
      if (!entry) throw new Error(`there is no theme ${id}`);
      await importer(entry);
      adopt();
      if (!themes.has(id)) throw new Error(`the module of theme ${id} registered no theme`);
      return themes.get(id);
    },
  };
}

/** The unmount of what `theme.mount` returned (a function, an object with one, or nothing), then
 * the abort of the mount's signal. */
const unmounter = (mounted, controller) => async () => {
  try {
    if (typeof mounted === 'function') await mounted();
    else await mounted?.unmount?.();
  } finally {
    controller.abort();
  }
};

/**
 * Mount the theme `id` on `root` with `core`; resolves `{ id, unmount }` for the theme mounted.
 * A theme that does not load, that says `supported()` is not ok, whose files `prepare(entry)` (the
 * page's loading of its sheets, metas and scripts; the fallback's too) says are not in place, or
 * that throws while it mounts is replaced by `fallback` and `toast` says "<Label> could not start
 * here; showing Ember"; an id nobody listed mounts the fallback without a word (the page says
 * that). A fallback that cannot mount throws a plain error, with no toast.
 */
export async function mountTheme({
  registry, id, root, core, toast, fallback = 'ember', prepare,
}) {
  async function attempt(which) {
    const theme = await registry.load(which);
    const can = theme.supported?.() ?? { ok: true };
    if (!can.ok) throw new Error(can.why ?? `theme ${which} cannot run here`);
    if (prepare && !(await prepare(registry.get(which) ?? { id: which }))) {
      throw new Error(`the files of theme ${which} did not load`);
    }
    const controller = new AbortController();
    try {
      registry.beforeMount?.(theme);
      const mounted = await theme.mount(root, core, { signal: controller.signal });
      registry.afterMount?.(theme);
      return { id: which, unmount: unmounter(mounted, controller) };
    } catch (error) {
      controller.abort();
      throw error;
    }
  }

  async function last() {
    try {
      return await attempt(fallback);
    } catch (cause) {
      throw new Error(`The fallback theme ${fallback} could not start`, { cause });
    }
  }

  if (!registry.get(id) || id === fallback) return last();
  try {
    return await attempt(id);
  } catch (error) {
    globalThis.console?.warn?.(`svolder: theme ${id} did not start`, error);
    const label = registry.get(id)?.label ?? id;
    toast(`${label} could not start here; showing ${registry.get(fallback)?.label ?? fallback}`);
    root.replaceChildren?.();
    return last();
  }
}
