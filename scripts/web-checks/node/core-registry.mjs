/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/registry.js, the themes the page can wear: a theme that throws on mount,
// one that says it cannot run here and an id nobody has all fall back to Ember (the first two
// with a toast saying which), a theme loads once, and a module that registers itself is found.
// Run: node core-registry.mjs
import assert from 'node:assert/strict';
import { createRegistry, mountTheme, registerTheme } from '../../../bins/svolderd/web/core/registry.js';

let n = 0;
// A theme that does not start is warned about in the console; the checks keep that quiet and count it.
const warned = [];
console.warn = (...args) => warned.push(args);
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const LIST = [
  { id: 'ember', label: 'Ember', blurb: 'warm', module: 'themes/ember/ember.js', schemes: ['dark'] },
  { id: 'probe', label: 'Probe', blurb: 'a test', module: 'themes/probe/probe.js', schemes: ['dark'] },
];
// A theme that records what it is asked.
const theme = (id, over = {}) => {
  const log = { mounted: 0, unmounted: 0 };
  return {
    log,
    id,
    label: id,
    needs: [],
    schemes: ['dark'],
    mount() { log.mounted++; return { unmount() { log.unmounted++; } }; },
    ...over,
  };
};
const rig = (extra = {}) => {
  const toasts = [];
  const registry = createRegistry(LIST, { importer: async (entry) => extra[entry.id]?.() });
  const ember = theme('ember');
  registry.add(ember);
  const root = { children: 0, replaceChildren() { this.children = 0; } };
  const mount = (id) => mountTheme({
    registry, id, root, core: { is: 'core' }, toast: (text) => toasts.push(text),
  });
  return { registry, toasts, ember, mount, root };
};

await ok('a_theme_that_throws_on_mount_falls_back_to_ember_with_a_toast', async () => {
  const { registry, toasts, ember, mount } = rig();
  registry.add(theme('probe', { mount() { throw new Error('no canvas'); } }));
  const mounted = await mount('probe');
  assert.equal(mounted.id, 'ember');
  assert.deepEqual(toasts, ['Probe could not start here; showing Ember']);
  assert.equal(ember.log.mounted, 1);
  assert.equal(warned.length, 1, 'the console is told why');
  mounted.unmount();
  assert.equal(ember.log.unmounted, 1);
});

await ok('a_theme_that_says_it_cannot_run_falls_back', async () => {
  const { registry, toasts, mount } = rig();
  const probe = theme('probe', { supported: () => ({ ok: false, why: 'no WebGL' }) });
  registry.add(probe);
  const mounted = await mount('probe');
  assert.equal(mounted.id, 'ember');
  assert.deepEqual(toasts, ['Probe could not start here; showing Ember']);
  assert.equal(probe.log.mounted, 0, 'it was never mounted');
});

await ok('an_unknown_id_mounts_ember', async () => {
  const { toasts, ember, mount } = rig();
  const mounted = await mount('nope');
  assert.equal(mounted.id, 'ember');
  assert.equal(ember.log.mounted, 1);
  assert.deepEqual(toasts, [], 'the page says that an id is unknown, not the mount');
});

await ok('a_theme_that_mounts_is_given_the_root_and_the_core_and_unmounts', async () => {
  const { registry, toasts, mount, root } = rig();
  const seen = [];
  const probe = theme('probe', {
    mount(at, core) { seen.push([at, core]); return () => seen.push('left'); },
  });
  registry.add(probe);
  const mounted = await mount('probe');
  assert.equal(mounted.id, 'probe');
  assert.deepEqual(seen, [[root, { is: 'core' }]]);
  await mounted.unmount();
  assert.deepEqual(seen.at(-1), 'left', 'a function returned is the unmount too');
  assert.deepEqual(toasts, []);
});

await ok('the_registry_calls_beforeMount_with_the_theme_about_to_mount_fallback_included', async () => {
  const bound = [];
  const registry = createRegistry(LIST, { beforeMount: (t) => bound.push(t.id) });
  registry.add(theme('ember'));
  registry.add(theme('probe', { mount() { throw new Error('no'); } }));
  const root = { replaceChildren() {} };
  await mountTheme({ registry, id: 'probe', root, core: {}, toast() {} });
  assert.deepEqual(bound, ['probe', 'ember'], 'each theme is bound as it is about to mount');
});

await ok('the_registry_calls_afterMount_once_a_theme_is_on_show_and_not_for_one_that_threw', async () => {
  const shown = [];
  const registry = createRegistry(LIST, { afterMount: (t) => shown.push(t.id) });
  registry.add(theme('ember'));
  registry.add(theme('probe', { mount() { throw new Error('no'); } }));
  const root = { replaceChildren() {} };
  await mountTheme({ registry, id: 'probe', root, core: {}, toast() {} });
  assert.deepEqual(shown, ['ember'], 'the theme that threw was never on show; the fallback was');
});

await ok('prepare_runs_for_each_theme_about_to_mount_the_fallback_included_and_a_no_means_the_fallback', async () => {
  const asked = [];
  const registry = createRegistry(LIST, {});
  registry.add(theme('ember'));
  registry.add(theme('probe'));
  const root = { replaceChildren() {} };
  const toasts = [];
  const mounted = await mountTheme({
    registry, id: 'probe', root, core: {}, toast: (t) => toasts.push(t),
    prepare: async (entry) => { asked.push(entry.id); return entry.id !== 'probe'; },
  });
  assert.equal(mounted.id, 'ember');
  assert.deepEqual(asked, ['probe', 'ember'], 'the fallback is prepared too');
  assert.deepEqual(toasts, ['Probe could not start here; showing Ember']);
  await assert.rejects(mountTheme({
    registry, id: 'ember', root, core: {}, toast() {}, prepare: async () => false,
  }), /fallback theme ember could not start/i);
});

await ok('a_mount_that_throws_aborts_its_signal_and_unmount_aborts_it_too', async () => {
  const { registry, mount } = rig();
  const left = [];
  registry.add(theme('probe', {
    mount(_root, _core, { signal }) {
      signal.addEventListener('abort', () => left.push('threw'));
      throw new Error('no canvas');
    },
  }));
  await mount('probe');
  assert.deepEqual(left, ['threw'], 'what the failed theme listened to goes');
  const kept = rig();
  kept.registry.add(theme('probe', {
    mount(_root, _core, { signal }) {
      signal.addEventListener('abort', () => left.push('unmounted'));
      return { unmount() { left.push('unmount'); } };
    },
  }));
  const mounted = await kept.mount('probe');
  assert.deepEqual(left, ['threw']);
  await mounted.unmount();
  assert.deepEqual(left, ['threw', 'unmount', 'unmounted']);
});

await ok('a_fallback_that_cannot_start_throws_without_a_toast', async () => {
  const { registry, toasts, mount } = rig();
  registry.add(theme('ember', { mount() { throw new Error('boom'); } }));
  await assert.rejects(mount('ember'), /fallback theme ember could not start/);
  await assert.rejects(mount('nope'), /fallback theme ember could not start/);
  assert.deepEqual(toasts, [], 'no "Ember could not start here; showing Ember"');
  registry.add(theme('probe', { mount() { throw new Error('boom'); } }));
  await assert.rejects(mount('probe'), /fallback/);
  assert.deepEqual(toasts, ['Probe could not start here; showing Ember']);
});

await ok('a_module_that_fails_to_load_or_register_falls_back', async () => {
  const failing = rig({ probe: async () => { throw new Error('404'); } });
  const mounted = await failing.mount('probe');
  assert.equal(mounted.id, 'ember');
  assert.deepEqual(failing.toasts, ['Probe could not start here; showing Ember']);
  const silent = rig({ probe: async () => undefined });
  assert.equal((await silent.mount('probe')).id, 'ember', 'a module that registers nothing');
});

await ok('a_module_that_registers_itself_is_found_and_loaded_once', async () => {
  let imports = 0;
  const registry = createRegistry(LIST, {
    importer: async () => { imports++; registerTheme(theme('probe')); },
  });
  assert.equal(registry.loaded('probe'), null);
  const loaded = await registry.load('probe');
  assert.equal(loaded.id, 'probe');
  assert.equal(registry.loaded('probe'), loaded);
  await registry.load('probe');
  assert.equal(imports, 1);
  await assert.rejects(registry.load('nope'), /nope/);
});

await ok('the_list_and_get_give_what_the_page_was_told_and_add_extends_it', async () => {
  const { registry } = rig();
  assert.deepEqual(registry.list().map((t) => t.id), ['ember', 'probe']);
  assert.equal(registry.get('probe').label, 'Probe');
  assert.equal(registry.get('nope'), undefined);
  registry.add(theme('extra', { label: 'Extra', blurb: 'new', schemes: ['light', 'dark'] }));
  assert.deepEqual(registry.list().map((t) => t.id), ['ember', 'probe', 'extra']);
  assert.deepEqual(registry.get('extra').schemes, ['light', 'dark']);
  assert.ok(registry.loaded('extra'));
});

console.log(`${n} core-registry checks passed`);
