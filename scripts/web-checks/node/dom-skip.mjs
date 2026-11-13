/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the skip of the harnesses that need jsdom (dom.mjs): where jsdom does not
// load, a harness says SKIPPED as its last line and fails (exit 77), and passes with the same
// line only when WEB_CHECK_ALLOW_NO_JSDOM=1 allows it; where jsdom loads, nothing is skipped.
// It runs a real harness (core-html.mjs is pure and never skips; core-modal.mjs needs the page)
// as a child, with WEB_CHECK_JSDOM_MODULE naming a module that does not exist. Those cases need
// no jsdom. The one case that does (with jsdom, nothing is skipped) is left out where jsdom does
// not load here, and this harness then ends as every other that needs it: SKIPPED as its last
// line, exit 77, or 0 when the skip is allowed.
// Run: node dom-skip.mjs
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

/** Whether jsdom loads on this machine, by the name dom.mjs loads it by. */
const HAVE = (() => {
  try {
    createRequire(import.meta.url).resolve(process.env.WEB_CHECK_JSDOM_MODULE ?? 'jsdom');
    return true;
  } catch {
    return false;
  }
})();
const ALLOWED = process.env.WEB_CHECK_ALLOW_NO_JSDOM === '1';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** Run the harness `name` with `env` laid over this one's; `{ status, last }`. */
function run(name, env = {}) {
  const file = fileURLToPath(new URL(`./${name}.mjs`, import.meta.url));
  const got = spawnSync(process.execPath, [file], {
    env: { ...process.env, WEB_CHECK_ALLOW_NO_JSDOM: '', ...env }, encoding: 'utf8',
  });
  return { status: got.status, last: got.stdout.trim().split('\n').at(-1) };
}
const GONE = { WEB_CHECK_JSDOM_MODULE: 'jsdom-that-is-not-installed' };

ok('without_jsdom_a_harness_that_needs_it_skips_loudly_and_fails', () => {
  const got = run('core-modal', GONE);
  assert.equal(got.status, 77);
  assert.match(got.last, /^SKIPPED core-modal\.mjs: jsdom is missing .*WEB_CHECK_ALLOW_NO_JSDOM=1/);
});

ok('the_environment_can_allow_the_skip_and_the_line_is_still_the_last', () => {
  const got = run('core-modal', { ...GONE, WEB_CHECK_ALLOW_NO_JSDOM: '1' });
  assert.equal(got.status, 0);
  assert.equal(got.last, 'SKIPPED core-modal.mjs: jsdom is missing (the apt package node-jsdom)');
});

ok('any_other_value_does_not_allow_it', () => {
  assert.equal(run('core-modal', { ...GONE, WEB_CHECK_ALLOW_NO_JSDOM: 'yes' }).status, 77);
});

if (HAVE) {
  ok('with_jsdom_nothing_is_skipped', () => {
    const got = run('core-modal');
    assert.equal(got.status, 0);
    assert.match(got.last, /^core-modal\.mjs: \d+ passed$/);
  });
}

ok('a_pure_harness_never_skips', () => {
  const got = run('core-html', GONE);
  assert.equal(got.status, 0);
  assert.doesNotMatch(got.last, /SKIPPED/);
});

console.log(`dom-skip.mjs: ${n} passed`);
if (!HAVE) {
  // the cases above passed; the one that needs jsdom did not run, and that is said, not hidden
  console.log(`SKIPPED dom-skip.mjs: jsdom is missing (the apt package node-jsdom)${
    ALLOWED ? '' : '; WEB_CHECK_ALLOW_NO_JSDOM=1 lets the gate pass without it'}`);
  process.exit(ALLOWED ? 0 : 77);
}
