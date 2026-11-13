/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/html.js, the markup helpers every theme and popup draws with: `esc` makes
// a word safe in text and in a quoted attribute, `attr` writes one attribute. Run: node core-html.mjs
import assert from 'node:assert/strict';
import { attr, esc } from '../../../bins/svolderd/web/core/html.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

ok('esc_escapes_the_five_markup_characters', () => {
  assert.equal(esc(`<a href="x" t='y'>&</a>`), '&lt;a href=&quot;x&quot; t=&#39;y&#39;&gt;&amp;&lt;/a&gt;');
});
ok('esc_escapes_the_ampersand_once', () => {
  assert.equal(esc('&amp;'), '&amp;amp;');
});
ok('esc_takes_null_and_undefined_as_empty_and_other_values_as_text', () => {
  assert.equal(esc(null), '');
  assert.equal(esc(undefined), '');
  assert.equal(esc(0), '0');
  assert.equal(esc(false), 'false');
});
ok('attr_quotes_and_escapes_a_value', () => {
  assert.equal(attr('title', 'a "b" <c>'), ' title="a &quot;b&quot; &lt;c&gt;"');
  assert.equal(attr('value', 0), ' value="0"');
});
ok('attr_writes_a_true_value_bare_and_no_value_as_nothing', () => {
  assert.equal(attr('disabled', true), ' disabled');
  assert.equal(attr('disabled', false), '');
  assert.equal(attr('disabled', null), '');
  assert.equal(attr('disabled', undefined), '');
});

console.log(`${n} core-html checks passed`);
