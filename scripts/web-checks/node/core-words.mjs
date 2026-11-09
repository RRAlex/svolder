/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's words (core/words.js): a typed number, and a point's frequency,
// gain and Q as a table writes them. Pure. Run: node core-words.mjs
import assert from 'node:assert/strict';
import {
  dbText, fixedEven, gainText, hzText, parse, qText, thousandsReading,
} from '../../../bins/svolderd/web/core/words.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

ok('a_typed_number_reads_with_k_a_comma_a_unit_and_a_true_minus', () => {
  assert.equal(parse('2.9k'), 2900);
  assert.equal(parse('2,9K'), 2900);
  assert.equal(parse('2900 Hz'), 2900);
  assert.equal(parse('+3,2 dB'), 3.2);
  assert.equal(parse('−2'), -2);
  assert.equal(parse(' 1.5k Hz '), 1500);
  assert.equal(parse(7), 7);
});

ok('a_comma_reads_as_a_decimal_mark_and_as_thousands_when_exactly_three_digits_follow_it', () => {
  assert.equal(parse('1,000'), 1);
  assert.equal(thousandsReading('1,000'), 1000);
  assert.equal(thousandsReading('10,000 Hz'), 10000);
  assert.equal(thousandsReading('-1,000'), -1000);
  assert.equal(thousandsReading('1,000,000'), 1000000);
  assert.equal(thousandsReading('+1,000'), 1000);
  for (const text of ['0,500', '0,010', '-0,500', '+0,000', '01,000', '1,5k', '20,5', '2.9k', '1,00', '1,0000', '1.000,5', '1.000', ',000', '1000', '1,000k', '']) {
    assert.equal(thousandsReading(text), null, text);
  }
});

ok('a_typed_word_that_is_no_number_is_null', () => {
  for (const text of ['', ' ', 'k', 'loud', '1.2.3', 'Hz', '1k2', 'Infinity', '--1']) {
    assert.equal(parse(text), null, JSON.stringify(text));
  }
});

ok('a_frequency_is_whole_above_100_and_one_decimal_below', () => {
  assert.equal(hzText(20), '20 Hz');
  assert.equal(hzText(99.96), '100 Hz');
  assert.equal(hzText(55.55), '55.5 Hz');
  assert.equal(hzText(100.4), '100 Hz');
  assert.equal(hzText(2900), '2900 Hz');
});

ok('a_gain_has_its_sign_a_true_minus_and_no_sign_on_zero', () => {
  assert.equal(gainText(3.2), '+3.2 dB');
  assert.equal(gainText(-2), '−2.0 dB');
  assert.equal(gainText(0), '0.0 dB');
  assert.equal(gainText(0.04), '0.0 dB');
  assert.equal(gainText(-0.04), '0.0 dB');
  assert.equal(dbText(-6), '−6.0');
  assert.equal(dbText(0), '0.0');
});

ok('a_q_has_two_decimals', () => {
  assert.equal(qText(1), '1.00');
  assert.equal(qText(0.707), '0.71');
});

ok('fixed_even_rounds_a_tie_to_the_even_digit_as_the_daemons_format_does', () => {
  // Rust's `format!("{:.N}")` (core::fmt's exact float printing): the double's exact value is
  // rounded, a value exactly halfway going to the even digit. JS's toFixed takes it up.
  const rust = [
    [0.125, 2, '0.12'], [0.375, 2, '0.38'], [0.135, 2, '0.14'], [2.5, 0, '2'], [0.5, 0, '0'],
    [1.5, 0, '2'], [-0.125, 2, '-0.12'], [-2.5, 0, '-2'], [0.25, 1, '0.2'], [0.75, 1, '0.8'],
    [100.0625, 3, '100.062'], [0.35, 1, '0.3'], [0.45, 1, '0.5'], [1234.5678, 2, '1234.57'],
    [0, 2, '0.00'], [3, 1, '3.0'], [-0.04, 1, '-0.0'], [1e21, 1, '1000000000000000000000.0'],
  ];
  for (const [value, digits, want] of rust) {
    assert.equal(fixedEven(value, digits), want, `${value} to ${digits}`);
  }
  assert.equal(fixedEven(-0, 1), '-0.0', 'a negative zero keeps its sign');
  assert.equal((0.125).toFixed(2), '0.13', 'the difference the helper closes');
});

console.log(`core-words.mjs: ${n} passed`);
