/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's points table (core/table.js): the words under the table counting
// the rows out of view, a row scrolled into view, the row in hand lit and kept by the core, the
// scroll kept across a draw, and the flow of a value typed in a cell (Enter writes, Escape and
// leaving put the old text back). The elements are fakes.
// Run: node core-table.mjs
import assert from 'node:assert/strict';
import { ROWS_SHOWN } from '../../../bins/svolderd/web/core/computer.js';
import { createTable, editCell, isLong } from '../../../bins/svolderd/web/core/table.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const classes = () => {
  const set = new Set();
  return { set, contains: (c) => set.has(c), toggle: (c, on) => { if (on) set.add(c); else set.delete(c); } };
};
/** A table box of `count` rows of 20 px, 120 px tall (six rows in view). */
function fake(count) {
  const box = { scrollTop: 0, clientHeight: 120, classList: classes(), children: [] };
  const note = { textContent: '' };
  const rows = Array.from({ length: count }, (_, i) => ({ offsetTop: i * 20, offsetHeight: 20, n: i + 1 }));
  box.children = rows;
  Object.defineProperty(box, 'scrollHeight', { get: () => count * 20 });
  if (count > ROWS_SHOWN) box.classList.set.add('long');
  const picks = [];
  const store = { n: null };
  const computer = { pick: (k) => { store.n = k; }, picked: () => store.n };
  const table = createTable({
    computer, box: () => box, note: () => note, row: (k) => rows[k - 1] ?? null,
    lit: (k) => picks.push(k), moreWords: (hidden) => `${hidden} more`,
  });
  return { box, note, rows, table, picks, store };
}

await ok('the_words_under_the_table_count_the_rows_out_of_view', async () => {
  assert.equal(isLong(ROWS_SHOWN), false);
  assert.equal(isLong(ROWS_SHOWN + 1), true);
  const t = fake(10);
  t.table.updateMore();
  assert.equal(t.note.textContent, '4 more');
  assert.equal(t.box.classList.contains('end'), false);
  t.box.scrollTop = 80;
  t.table.updateMore();
  assert.equal(t.note.textContent, '');
  assert.equal(t.box.classList.contains('end'), true);
  const short = fake(4);
  short.table.updateMore();
  assert.equal(short.note.textContent, '', 'a short table counts nothing');
  assert.equal(short.box.classList.contains('end'), true);
});

await ok('a_row_is_scrolled_into_view_inside_the_table_and_picking_lights_it', async () => {
  const t = fake(10);
  t.table.reveal(10);
  assert.equal(t.box.scrollTop, 80);
  t.table.reveal(1);
  assert.equal(t.box.scrollTop, 0);
  t.table.reveal(3);
  assert.equal(t.box.scrollTop, 0, 'a row in view does not scroll');
  t.table.reveal(99);
  assert.equal(t.box.scrollTop, 0, 'a row that is not there does nothing');
  t.table.pick(8);
  assert.deepEqual([t.store.n, t.picks, t.box.scrollTop], [8, [8], 40]);
  t.table.pick(1, { scroll: false });
  assert.deepEqual([t.store.n, t.picks, t.box.scrollTop], [1, [8, 1], 40]);
  const nothing = fake(3);
  nothing.table.pick(2);
  assert.equal(nothing.box.scrollTop, 0);
});

await ok('the_scroll_is_kept_across_a_draw_and_the_point_in_hand_is_shown', async () => {
  const t = fake(10);
  t.box.scrollTop = 40;
  t.table.scrolled();
  t.box.scrollTop = 0;
  t.table.restore();
  assert.equal(t.box.scrollTop, 40);
  t.store.n = 10;
  t.table.restore();
  assert.equal(t.box.scrollTop, 80, 'the picked row is shown');
  t.box.scrollTop = 20;
  t.table.keep();
  t.box.scrollTop = 0;
  t.store.n = null;
  t.table.restore();
  assert.equal(t.box.scrollTop, 20);
  t.table.reset();
  t.table.restore();
  assert.equal(t.box.scrollTop, 0);
});

await ok('a_table_in_a_tbody_with_unequal_rows_counts_the_rows_below_and_fades', async () => {
  // rows of 10, 30, 20, 20, 40, 20, 20, 20 px in a box 100 px tall: 180 px of rows
  const heights = [10, 30, 20, 20, 40, 20, 20, 20];
  let top = 0;
  const trs = heights.map((h) => { const r = { offsetTop: top, offsetHeight: h }; top += h; return r; });
  const body = { children: trs };
  const box = { scrollTop: 0, clientHeight: 100, scrollHeight: top, children: [body], classList: classes() };
  box.classList.set.add('long');
  const note = { textContent: '' };
  const heard = [];
  const table = createTable({
    computer: { pick() {}, picked: () => null }, box: () => box, note: () => note,
    row: (k) => trs[k - 1], moreWords: (h) => `${h} below`, rows: (area) => area.children[0].children,
    exact: true, fadeClass: 'fade', onMore: (h) => heard.push(h),
  });
  table.updateMore();
  assert.equal(note.textContent, '4 below', 'rows 5 (ends at 120) to 8 are below the view');
  assert.equal(box.classList.contains('fade'), true);
  assert.equal(box.classList.contains('end'), false);
  box.scrollTop = 80;
  table.updateMore();
  assert.equal(note.textContent, '', 'every row ends within 181');
  assert.equal(box.classList.contains('fade'), false);
  assert.equal(box.classList.contains('end'), true);
  assert.deepEqual(heard, [4, 0]);
  // the mean-height count with a rows() list is not fooled by one child: 8 rows, not 1
  const mean = createTable({
    computer: { pick() {}, picked: () => null }, box: () => box, note: () => note,
    row: (k) => trs[k - 1], moreWords: (h) => `${h} more`, rows: (area) => area.children[0].children,
  });
  box.scrollTop = 0;
  mean.updateMore();
  assert.equal(note.textContent, '4 more');
  // no words element: the count still reaches the callback
  const bare = createTable({
    computer: { pick() {}, picked: () => null }, box: () => box, note: () => null,
    row: () => null, moreWords: String, rows: (area) => area.children[0].children, exact: true,
    onMore: (h) => heard.push(h),
  });
  bare.updateMore();
  assert.equal(heard.at(-1), 4);
});

/** An input that hears keydown and blur. */
function input(value) {
  const heard = {};
  return {
    value, heard, addEventListener: (type, fn) => { heard[type] = fn; },
    key: async (key) => {
      const event = { key, prevented: false, stopped: false,
        preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; } };
      await heard.keydown(event);
      return event;
    },
  };
}

await ok('a_typed_cell_writes_on_enter_and_puts_the_text_back_on_escape_or_leaving', async () => {
  const calls = [];
  const flow = (field, over = {}) => editCell(field, {
    n: 2, param: 'g', shown: '+3.0 dB',
    cell: async (...args) => { calls.push(['cell', ...args]); return { ok: true }; },
    restore: () => calls.push(['restore']), after: () => calls.push(['after']), ...over,
  });
  const a = input('4,5');
  flow(a);
  const event = await a.key('Enter');
  assert.equal(event.prevented, true);
  assert.deepEqual(calls, [['restore'], ['cell', 2, 'g', '4,5', '+3.0 dB'], ['after']]);
  // the field going away after Enter changes nothing
  a.heard.blur();
  assert.equal(calls.length, 3);
  calls.length = 0;
  const b = input('9');
  flow(b);
  const esc = await b.key('Escape');
  assert.equal(esc.stopped, true);
  assert.deepEqual(calls, [['restore'], ['after']]);
  await b.key('Enter');
  assert.equal(calls.length, 2, 'a flow that is over writes nothing');
  calls.length = 0;
  const c = input('9');
  flow(c);
  c.heard.blur();
  c.heard.blur();
  assert.deepEqual(calls, [['restore'], ['after']], 'leaving ends it once');
  calls.length = 0;
  const d = input('1');
  flow(d);
  await d.key('Tab');
  assert.deepEqual(calls, []);
  // a write that throws still lets the theme draw what waited
  calls.length = 0;
  const e = input('1');
  flow(e, { cell: async () => { throw new Error('down'); } });
  await assert.rejects(() => e.key('Enter'));
  assert.deepEqual(calls, [['restore'], ['after']]);
});

await ok('a_typed_cell_is_read_before_the_theme_puts_the_old_text_back', async () => {
  const calls = [];
  const field = input('4,5');
  editCell(field, {
    n: 2, param: 'g', shown: '+3.0 dB',
    cell: async (...args) => { calls.push(args); return { ok: true }; },
    restore: () => { field.value = '+3.0 dB'; },
  });
  await field.key('Enter');
  assert.deepEqual(calls, [[2, 'g', '4,5', '+3.0 dB']], 'what was typed, not the text put back');
});

console.log(`core-table.mjs: ${n} passed`);
