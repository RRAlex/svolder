/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The toasts at the bottom right: a stack made on first use, each toast with its own timer and
// fade. The page, the components and the effects say things through `toast` and take them away
// with `dismissToast`.

/** How many toasts show at once: a fifth pushes the oldest timed one out. */
const TOASTS_MAX = 4;
/** How long a toast stays (ms), unless its caller says otherwise. */
const TOAST_MS = 2200;

/** The stack the toasts live in, made on first use: fixed at the bottom right, out of the flow. */
function toastStack() {
  let stack = document.getElementById('toasts');
  if (!stack) {
    stack = document.createElement('div');
    stack.id = 'toasts';
    stack.className = 'toasts';
    stack.setAttribute('role', 'status');
    document.body.append(stack);
  }
  return stack;
}

/** Take toast `box` off the stack at once, its timer with it. */
function drop(box) {
  clearTimeout(box.hideTimer);
  box.remove();
}

/** Take toast `box` off the stack, after its fade. */
function leave(box) {
  clearTimeout(box.hideTimer);
  box.dataset.leaving = 'true';
  box.classList.remove('show');
  setTimeout(() => box.remove(), 220);
}

/**
 * Show `text` as a toast at the bottom right for `ms` (2.2 s), with a button `action` =
 * {label, href} when given. Toasts stack, the newest at the bottom, four at most (a fifth pushes
 * out the oldest that has a timer and is not already leaving, never one kept with `ms` 0); the
 * `key`, the text unless given, names a toast, and a new toast replaces the one of its key
 * instead of stacking; the same words again, with no button, leave the toast standing and
 * start its time over (the same words twice show once, as one toast); `ms` 0 keeps it until
 * `dismissToast` takes it away.
 */
export function toast(text, action, { key = text, ms = TOAST_MS } = {}) {
  const stack = toastStack();
  const old = [...stack.children].find((box) => box.dataset.key === key);
  const same = old && !action && !old.dataset.leaving && old.dataset.text === text;
  if (same && !old.querySelector('a')) {
    clearTimeout(old.hideTimer);
    old.hideTimer = ms > 0 ? setTimeout(() => leave(old), ms) : undefined;
    return old;
  }
  const box = document.createElement('div');
  box.className = 'toast';
  box.dataset.key = key;
  box.dataset.text = text;
  box.append(document.createTextNode(text));
  if (action) {
    const anchor = document.createElement('a');
    anchor.className = 'toast-action';
    anchor.href = action.href;
    anchor.textContent = action.label;
    box.append(anchor);
  }
  if (old) drop(old);
  stack.append(box);
  const shown = () => [...stack.children].filter((one) => !one.dataset.leaving);
  while (shown().length > TOASTS_MAX) {
    const oldest = shown().find((one) => one.hideTimer);
    if (!oldest) break;
    drop(oldest);
  }
  // One frame later, so the fade in runs from its resting place.
  requestAnimationFrame(() => box.classList.add('show'));
  if (ms > 0) box.hideTimer = setTimeout(() => leave(box), ms);
  return box;
}

/** Take away the toast of `key`, if it is still up. */
export function dismissToast(key) {
  const box = [...(document.getElementById('toasts')?.children ?? [])].find(
    (one) => one.dataset.key === key,
  );
  if (box) leave(box);
}
