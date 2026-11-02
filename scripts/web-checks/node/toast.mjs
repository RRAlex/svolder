/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/toast.js, the popups: the same words again, with no button, leave the one
// popup standing and start its time over; other words, or a button, make a popup of their own or
// replace the one of their key. A small fake document and clock stand in for the browser. Run:
// node toast.mjs
import assert from 'node:assert/strict';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

// A clock that moves when the test says.
let now = 0;
let seq = 0;
let timers = [];
globalThis.setTimeout = (fn, ms) => { seq++; timers.push({ id: seq, at: now + ms, fn }); return seq; };
globalThis.clearTimeout = (id) => { timers = timers.filter((t) => t.id !== id); };
globalThis.requestAnimationFrame = (fn) => fn();
const advance = (ms) => {
  const end = now + ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= end).sort((a, b) => a.at - b.at)[0];
    if (!due) break;
    timers = timers.filter((t) => t !== due);
    now = due.at;
    due.fn();
  }
  now = end;
};

// Just enough of a document.
function element(tag) {
  const el = {
    tag, dataset: {}, children: [], className: '', parent: null, nodes: [],
    classList: { add() {}, remove() {} },
    setAttribute() {},
    append(...kids) { for (const kid of kids) { kid.parent = el; el.children.push(kid); } },
    remove() { if (el.parent) el.parent.children = el.parent.children.filter((c) => c !== el); },
    querySelector(sel) { return sel === 'a' ? el.children.find((c) => c.tag === 'a') ?? null : null; },
  };
  return el;
}
const body = element('body');
globalThis.document = {
  body,
  getElementById: (id) => body.children.find((c) => c.id === id) ?? null,
  createElement: (tag) => element(tag),
  createTextNode: (text) => ({ tag: '#text', text, children: [] }),
};
const { toast, dismissToast } = await import('../../../bins/svolderd/web/core/toast.js');
const stack = () => document.getElementById('toasts');
const shown = () => stack().children.filter((box) => !box.dataset.leaving);

ok('the_same_words_again_leave_one_popup_and_start_its_time_over', () => {
  const first = toast('Saved');
  advance(1500);
  const second = toast('Saved');
  assert.equal(second, first, 'the popup that stands is the one returned');
  assert.equal(shown().length, 1);
  advance(1500);
  assert.equal(shown().length, 1, '3 s after the first, 1.5 s after the second: still up');
  advance(1000);
  assert.equal(shown().length, 0, 'it leaves 2.2 s after the second');
  advance(300);
  assert.equal(stack().children.length, 0, 'and is gone after its fade');
});

ok('other_words_stack_and_a_button_makes_a_popup_of_its_own', () => {
  const a = toast('One');
  const b = toast('Two');
  assert.notEqual(a, b);
  assert.equal(shown().length, 2);
  const c = toast('One', { label: 'Sign in', href: './?sign-in' });
  assert.notEqual(c, a, 'a popup with a button replaces the one of its key');
  assert.equal(shown().filter((box) => box.dataset.key === 'One').length, 1);
  assert.ok(c.querySelector('a'));
  // The same words again do not take a button's popup for their own.
  const d = toast('One');
  assert.notEqual(d, c);
  advance(3000);
});

ok('a_popup_that_is_leaving_is_not_reused_and_a_kept_one_stays', () => {
  const a = toast('Gone');
  advance(2200);
  assert.equal(a.dataset.leaving, 'true');
  const b = toast('Gone');
  assert.notEqual(b, a, 'a popup on its way out is not taken back');
  const kept = toast('Climbing', null, { key: 'climb', ms: 0 });
  advance(10000);
  assert.ok(shown().includes(kept), 'ms 0 stays until it is taken away');
  const again = toast('Climbing', null, { key: 'climb', ms: 0 });
  assert.equal(again, kept);
  dismissToast('climb');
  advance(300);
  assert.ok(!shown().includes(kept));
});

console.log(`toast.mjs: ${n} checks passed`);
