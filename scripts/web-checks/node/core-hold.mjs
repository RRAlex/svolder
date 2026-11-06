/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/hold.js: the held `−` and `+` every theme gets from the core, over a fake
// clock and fake event targets.
// Run: node core-hold.mjs
import assert from 'node:assert/strict';
import { HOLD_AFTER_MS, HOLD_CAP_MS, HOLD_EVERY_MS, holdRepeat } from '../../../bins/svolderd/web/core/hold.js';

let n = 0;

// A clock the test moves by hand: timers and Date.now are the fake's while a test runs.
const real = {
  setTimeout: globalThis.setTimeout,
  clearTimeout: globalThis.clearTimeout,
  setInterval: globalThis.setInterval,
  clearInterval: globalThis.clearInterval,
  now: Date.now,
};
let now = 0;
let timers = [];
let next = 1;
globalThis.setTimeout = (fn, ms) => { timers.push({ id: next, at: now + ms, fn, every: 0 }); return next++; };
globalThis.setInterval = (fn, ms) => { timers.push({ id: next, at: now + ms, fn, every: ms }); return next++; };
globalThis.clearTimeout = (id) => { timers = timers.filter((t) => t.id !== id); };
globalThis.clearInterval = globalThis.clearTimeout;
Date.now = () => now;
const advance = (ms) => {
  const to = now + ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= to).sort((a, b) => a.at - b.at || a.id - b.id)[0];
    if (!due) break;
    now = due.at;
    if (due.every) due.at += due.every;
    else timers = timers.filter((t) => t !== due);
    due.fn();
  }
  now = to;
};

class Doc extends EventTarget {
  hidden = false;
}
class Button extends EventTarget {
  constructor(doc, id) {
    super();
    this.id = id;
    this.ownerDocument = doc;
    this.isConnected = true;
    this.captured = [];
  }

  setPointerCapture(id) { this.captured.push(id); }

  contains(el) { return el === this; }
}
const pointer = (type, extra = {}) => {
  const event = new Event(type, { cancelable: true, bubbles: true });
  for (const [key, value] of Object.entries({ pointerId: 7, isPrimary: true, buttons: 1, ...extra })) {
    Object.defineProperty(event, key, { value });
  }
  return event;
};

// A page: a document, a window, a root that holds one button by id.
function page({ id = 'up' } = {}) {
  const doc = new Doc();
  const win = new EventTarget();
  globalThis.window = win;
  globalThis.document = doc;
  const button = new Button(doc, id);
  const root = new EventTarget();
  root.current = button;
  root.querySelector = () => root.current;
  const steps = [];
  const press = (opts = {}) => {
    const event = pointer('pointerdown', { target: button });
    const hold = holdRepeat(event, {
      root, step: (el) => steps.push(el), ...opts,
    });
    return { event, hold };
  };
  return { doc, win, button, root, steps, press };
}

const ok = (name, fn) => {
  now = 0;
  timers = [];
  fn();
  n++;
  console.log('ok', name);
};

ok('a_press_steps_once_then_repeats_after_380_ms_every_70', () => {
  assert.equal([HOLD_AFTER_MS, HOLD_EVERY_MS, HOLD_CAP_MS].join(), '380,70,10000');
  const p = page();
  const { event } = p.press();
  assert.equal(p.steps.length, 1);
  assert.equal(event.defaultPrevented, true);
  assert.deepEqual(p.button.captured, [7]);
  advance(379);
  assert.equal(p.steps.length, 1);
  advance(1);
  assert.equal(p.steps.length, 1);
  advance(70);
  assert.equal(p.steps.length, 2);
  advance(210);
  assert.equal(p.steps.length, 5);
});

ok('the_release_anywhere_ends_it', () => {
  for (const type of ['pointerup', 'pointercancel']) {
    const p = page();
    p.press();
    advance(1000);
    const went = p.steps.length;
    // another pointer's release is not this one's
    p.doc.dispatchEvent(pointer(type, { pointerId: 9 }));
    advance(140);
    assert.ok(p.steps.length > went);
    p.doc.dispatchEvent(pointer(type));
    const then = p.steps.length;
    advance(1000);
    assert.equal(p.steps.length, then, type);
  }
});

ok('a_redraw_keeps_the_hold_on_the_new_button_of_that_id', () => {
  const p = page();
  p.press();
  advance(500);
  const fresh = new Button(p.doc, 'up');
  p.root.current = fresh;
  advance(140);
  assert.equal(p.steps.at(-1), fresh);
  assert.equal(p.steps.at(-2), fresh);
});

ok('a_move_with_no_button_down_ends_it', () => {
  const p = page();
  p.press();
  advance(500);
  p.doc.dispatchEvent(pointer('pointermove', { buttons: 1 }));
  advance(140);
  const went = p.steps.length;
  p.doc.dispatchEvent(pointer('pointermove', { buttons: 0, pointerId: 9 }));
  advance(140);
  assert.ok(p.steps.length > went);
  p.doc.dispatchEvent(pointer('pointermove', { buttons: 0 }));
  const then = p.steps.length;
  advance(1000);
  assert.equal(p.steps.length, then);
});

ok('blur_hidden_or_not_live_ends_it', () => {
  const stops = {
    blur: (p) => p.win.dispatchEvent(new Event('blur')),
    hidden: (p) => { p.doc.hidden = true; advance(70); },
    visibility: (p) => { p.doc.hidden = true; p.doc.dispatchEvent(new Event('visibilitychange')); },
    live: (p) => { p.alive = false; advance(70); },
  };
  for (const [name, how] of Object.entries(stops)) {
    const p = page();
    p.alive = true;
    p.press({ live: () => p.alive });
    advance(500);
    assert.ok(p.steps.length >= 2, name);
    how(p);
    const then = p.steps.length;
    advance(1000);
    // the press and the step at 450 ms, and no other
    assert.equal(then, 2, name);
    assert.equal(p.steps.length, 2, name);
  }
});

ok('no_button_left_ends_it', () => {
  const p = page();
  p.press();
  advance(500);
  p.root.current = null;
  advance(70);
  const then = p.steps.length;
  advance(1000);
  assert.equal(p.steps.length, then);
  // with no id the pressed element itself, while it is connected
  const q = page({ id: '' });
  q.press();
  advance(500);
  q.button.isConnected = false;
  advance(70);
  const gone = q.steps.length;
  advance(1000);
  assert.equal(q.steps.length, gone);
});

ok('leave_ends_on_pointerleave_and_lost_capture', () => {
  for (const type of ['pointerleave', 'lostpointercapture']) {
    const p = page();
    p.press({ leave: true });
    advance(500);
    p.button.dispatchEvent(pointer(type));
    const then = p.steps.length;
    advance(1000);
    assert.equal(p.steps.length, then, type);
  }
});

ok('without_leave_a_lost_capture_does_not_end_it', () => {
  const p = page();
  p.press();
  advance(500);
  p.button.dispatchEvent(pointer('lostpointercapture'));
  p.button.dispatchEvent(pointer('pointerleave'));
  const then = p.steps.length;
  advance(700);
  assert.ok(p.steps.length > then);
});

ok('the_cap_ends_it_after_10_s', () => {
  const p = page();
  p.press();
  advance(380 + 10_000 + 70);
  const then = p.steps.length;
  // the press and a step every 70 ms until the tick past the cap
  assert.equal(then, 143);
  advance(2000);
  assert.equal(p.steps.length, then);
  // an unbounded cap goes on
  const q = page();
  q.press({ cap: Infinity });
  advance(30_000);
  assert.ok(q.steps.length > 400);
});

ok('the_abort_ends_it', () => {
  const p = page();
  const controller = new AbortController();
  p.press({ signal: controller.signal });
  advance(500);
  controller.abort();
  const then = p.steps.length;
  advance(1000);
  assert.equal(p.steps.length, then);
  // end() does too, and twice is harmless
  const q = page();
  const { hold } = q.press();
  advance(500);
  hold.end();
  hold.end();
  const gone = q.steps.length;
  advance(1000);
  assert.equal(q.steps.length, gone);
});

ok('a_viewer_who_may_not_write_gets_one_step_and_no_repeat', () => {
  const p = page();
  const { event } = p.press({ live: () => false, onStart() { throw new Error('started'); } });
  advance(2000);
  assert.equal(p.steps.length, 1);
  assert.equal(event.defaultPrevented, false);
  assert.deepEqual(p.button.captured, []);
});

ok('the_context_menu_is_kept_off_a_held_button', () => {
  const p = page();
  p.press();
  const held = pointer('contextmenu', { target: p.button });
  p.root.dispatchEvent(held);
  assert.equal(held.defaultPrevented, true);
  const other = pointer('contextmenu', { target: new Button(p.doc, 'other') });
  p.root.dispatchEvent(other);
  assert.equal(other.defaultPrevented, false);
  p.doc.dispatchEvent(pointer('pointerup'));
  const after = pointer('contextmenu', { target: p.button });
  p.root.dispatchEvent(after);
  assert.equal(after.defaultPrevented, false);
});

ok('on_start_and_on_end_bracket_it_once', () => {
  const p = page();
  const calls = [];
  const { hold } = p.press({ onStart: () => calls.push('start'), onEnd: () => calls.push('end') });
  assert.deepEqual(calls, ['start']);
  advance(500);
  p.doc.dispatchEvent(pointer('pointerup'));
  hold.end();
  p.win.dispatchEvent(new Event('blur'));
  assert.deepEqual(calls, ['start', 'end']);
});

Object.assign(globalThis, {
  setTimeout: real.setTimeout, clearTimeout: real.clearTimeout,
  setInterval: real.setInterval, clearInterval: real.clearInterval,
});
Date.now = real.now;
console.log(`core-hold.mjs: ${n} passed`);
