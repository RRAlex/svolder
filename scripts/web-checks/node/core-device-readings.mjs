/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/popups/devices/readings.js, the All readings disclosure of a device's
// page as strings: closed and open, a connected device and one that is not, a device with no
// reading, the value through the converter, the ages in the core's words with what the ticker
// reads, every string escaped, ids that hold across a rename. Run: node core-device-readings.mjs
import assert from 'node:assert/strict';
import { ageWords } from '../../../bins/svolderd/web/core/words.js';
import {
  READINGS_AGES, READINGS_BODY_ID, READINGS_ID, READINGS_TABLE_ID, READING_WORDS, readingsBodyHtml,
  readingsHtml, wordsOf,
} from '../../../bins/svolderd/web/core/popups/devices/readings.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const NOW = 1_800_000_000_000;
const ROWS = [
  { id: '10', label: 'Volume', value: '63 (−26.1 dB)', heard_at_ms: NOW - 4_000 },
  { id: '2', label: 'Input', value: '1 (USB 1)', heard_at_ms: NOW - 200_000 },
  { id: '12.3.3', label: 'EQ band 3 gain', value: '+0.0 dB', heard_at_ms: null },
];
const device = (more = {}, facts = {}) => ({
  url: 'desk-amp', name: 'Desk amp', connected: true,
  model: { name: 'Desk amp', facts: { now_ms: NOW, rows: ROWS, ...facts } },
  ...more,
});
const OPEN = { open: true };
const count = (html, piece) => html.split(piece).length - 1;
const tagOf = (html, id) => {
  const found = html.match(new RegExp(`<[^>]*\\sid="${id}"[^>]*>`));
  assert.ok(found, `no element with the id ${id}`);
  return found[0];
};
const rowsOf = (html) => [...html.matchAll(/<tr data-reading="([^"]*)">(.*?)<\/tr>/g)]
  .map(([, id, inner]) => ({ id, inner }));

ok('closed_unless_the_state_says_open_and_nothing_is_drawn_inside_while_closed', () => {
  for (const state of [undefined, null, {}, { open: false }]) {
    const html = readingsHtml(device(), state);
    assert.match(tagOf(html, READINGS_ID), /^<details /);
    assert.doesNotMatch(tagOf(html, READINGS_ID), /\sopen/);
    assert.equal(count(html, '<summary'), 1);
    assert.match(html, /<summary[^>]*>All readings<\/summary>/);
    assert.match(html, new RegExp(`<div id="${READINGS_BODY_ID}"></div></details>$`));
    assert.equal(readingsBodyHtml(device(), state), '');
  }
  // nothing is asked of the converter while it is closed
  readingsHtml(device(), {}, { say: () => assert.fail('worded while closed') });
});

ok('open_holds_a_table_of_every_reading_its_name_its_value_and_its_age', () => {
  const html = readingsHtml(device(), OPEN);
  assert.match(tagOf(html, READINGS_ID), /^<details open /);
  assert.match(tagOf(html, READINGS_TABLE_ID), /^<table /);
  assert.match(html, /<caption>Readings of Desk amp<\/caption>/);
  assert.equal(count(html, '<th scope="col"'), 3);
  for (const head of [READING_WORDS.name, READING_WORDS.value, READING_WORDS.age]) {
    assert.match(html, new RegExp(`<th scope="col">${head}</th>`));
  }
  const rows = rowsOf(html);
  assert.deepEqual(rows.map((row) => row.id), ROWS.map((row) => row.id));
  assert.match(rows[0].inner, /^<th scope="row">Volume<\/th><td>63 \(−26\.1 dB\)<\/td><td /);
  assert.equal(readingsHtml(device(), OPEN),
    readingsHtml(device(), {}).replace('<details ', '<details open ')
      .replace(`<div id="${READINGS_BODY_ID}"></div>`,
        `<div id="${READINGS_BODY_ID}">${readingsBodyHtml(device(), OPEN)}</div>`));
});

ok('the_summary_holds_no_heading_and_the_draw_uses_the_frames_classes', () => {
  const html = readingsHtml(device(), OPEN);
  assert.doesNotMatch(html.match(/<summary.*?<\/summary>/)[0], /<h\d/);
  for (const [, classes] of html.matchAll(/class="([^"]*)"/g)) {
    for (const one of classes.split(' ')) assert.match(one, /^(fp|fpd|fi)-/);
  }
  assert.doesNotMatch(html, /style=/);
});

ok('a_value_is_what_the_converter_says_and_the_daemons_words_without_one', () => {
  const one = device();
  const seen = [];
  const html = readingsBodyHtml(one, OPEN, {
    say: (row, model) => { seen.push([row, model]); return `<${row.id}> in words`; },
  });
  assert.deepEqual(seen.map(([row]) => row), ROWS);
  assert.ok(seen.every(([, model]) => model === one.model));
  assert.deepEqual(rowsOf(html).map((row) => row.inner.match(/<td>(.*?)<\/td>/)[1]),
    ROWS.map((row) => `&lt;${row.id}&gt; in words`));
  assert.deepEqual(ROWS.map(wordsOf), ROWS.map((row) => row.value));
  // a converter that has no words for a reading leaves the daemon's
  const some = readingsBodyHtml(one, OPEN, { say: (row) => (row.id === '2' ? 'USB 1' : null) });
  assert.deepEqual(rowsOf(some).map((row) => row.inner.match(/<td>(.*?)<\/td>/)[1]),
    ['63 (−26.1 dB)', 'USB 1', '+0.0 dB']);
});

ok('an_age_is_in_the_cores_words_with_what_the_ticker_reads', () => {
  const html = readingsBodyHtml(device(), OPEN);
  assert.match(tagOf(html, READINGS_TABLE_ID), new RegExp(`\\sdata-now="${NOW}"`));
  const rows = rowsOf(html);
  assert.match(rows[0].inner,
    new RegExp(`<td class="fpd-reading-age" data-heard-at="${NOW - 4_000}">4 s ago</td>$`));
  assert.ok(rows[1].inner.endsWith(`>${ageWords(200)} ago</td>`));
  assert.equal(ageWords(200), '3 min');
  // a reading the daemon has no stamp for has no age, and nothing for the ticker to write
  assert.ok(rows[2].inner.endsWith(`<td class="fpd-reading-age">${READING_WORDS.unknown}</td>`));
  assert.equal(count(html, 'data-heard-at='), 2);
  // the selectors a wire hands core.clock.tickAges name what is drawn
  assert.equal(READINGS_AGES.stamp, `#${READINGS_TABLE_ID}`);
  assert.equal(READINGS_AGES.ages, `#${READINGS_TABLE_ID} td.fpd-reading-age[data-heard-at]`);
  // a reading stamped after the model's clock is not from the future
  const ahead = device({}, { rows: [{ ...ROWS[0], heard_at_ms: NOW + 900 }] });
  assert.match(readingsBodyHtml(ahead, OPEN), />0 s ago<\/td>/);
});

ok('a_device_that_is_not_connected_shows_its_last_known_readings_as_such', () => {
  const away = readingsHtml(device({ connected: false }), OPEN);
  assert.match(away, /<caption>Last known readings of Desk amp \(not connected\)<\/caption>/);
  assert.match(tagOf(away, READINGS_TABLE_ID), /\sdata-last-known[\s>=]/);
  assert.equal(rowsOf(away).length, ROWS.length);
  const here = readingsHtml(device(), OPEN);
  assert.doesNotMatch(here, /data-last-known|Last known/);
});

ok('a_device_with_no_reading_says_so_connected_or_not', () => {
  const none = [
    device({ model: null }), device({ model: undefined }), device({ model: {} }),
    device({}, { rows: [] }), device({}, { rows: undefined }),
  ];
  for (const one of none) {
    const html = readingsHtml(one, OPEN);
    assert.doesNotMatch(html, /<table/);
    assert.match(html, new RegExp(`<p class="fp-note">${READING_WORDS.none}</p>`));
    const away = readingsHtml({ ...one, connected: false }, OPEN);
    assert.doesNotMatch(away, /<table/);
    assert.match(away, new RegExp(`<p class="fp-note">${READING_WORDS.noneAway}</p>`));
    assert.match(tagOf(html, READINGS_ID), /^<details /);
  }
  assert.notEqual(READING_WORDS.none, READING_WORDS.noneAway);
});

ok('a_reading_the_profile_does_not_name_is_called_by_its_id', () => {
  const html = readingsBodyHtml(device({}, {
    rows: [{ id: '77.1', label: '?', value: '5', heard_at_ms: null },
      { id: '77.2', label: '', value: '6', heard_at_ms: null }],
  }), OPEN);
  assert.deepEqual(rowsOf(html).map((row) => row.inner.match(/<th scope="row">(.*?)<\/th>/)[1]),
    ['77.1', '77.2']);
});

ok('every_string_is_escaped', () => {
  const bad = '<img src=x onerror="alert(1)">&\'';
  const html = readingsHtml(device({ name: bad }, {
    now_ms: bad,
    rows: [{ id: bad, label: bad, value: bad, heard_at_ms: bad }],
  }), OPEN, { words: { ...READING_WORDS, title: bad, name: bad, value: bad, age: bad, unknown: bad } });
  assert.doesNotMatch(html, /<img|onerror="alert/);
  assert.ok(count(html, '&lt;img src=x onerror=&quot;alert(1)&quot;&gt;&amp;&#39;') >= 8);
  const none = readingsHtml(device({ name: bad, model: null }), OPEN,
    { words: { ...READING_WORDS, none: bad } });
  assert.doesNotMatch(none, /<img/);
  // a converter's words are text too
  const said = readingsBodyHtml(device(), OPEN, { say: () => bad });
  assert.doesNotMatch(said, /<img/);
});

ok('the_ids_are_of_the_part_and_hold_across_a_rename', () => {
  const ids = (html) => [...html.matchAll(/\sid="([^"]*)"/g)].map(([, id]) => id);
  const before = ids(readingsHtml(device(), OPEN));
  const after = ids(readingsHtml(device({ name: 'Den amp', url: 'den-amp' }), OPEN));
  assert.deepEqual(before, [READINGS_ID, READINGS_BODY_ID, READINGS_TABLE_ID]);
  assert.deepEqual(after, before);
  assert.equal(new Set(before).size, before.length);
  for (const id of before) assert.match(id, /^popup-devices-readings/);
  assert.doesNotMatch(before.join(' '), /desk|amp/i);
});

console.log(`${n} core-device-readings checks passed`);
