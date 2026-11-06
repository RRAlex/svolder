/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A pressed `−` or `+`, held: one step at the press and, after a pause, steps at a steady pace.
// Signal's volume and steppers and Ember's knob use it, so the end of a hold is the same: the
// pointer's release anywhere, a move of it with no button down (a release nobody saw), the window
// losing focus, the page hidden, the hold no longer allowed, no button left to step, the cap, the
// signal's abort. The page may draw the button again meanwhile: the hold follows the button of its
// id, not the element pressed.

/** A held press repeats after this long (ms), then at the pace below, for at most the cap. */
export const HOLD_AFTER_MS = 380;
export const HOLD_EVERY_MS = 70;
export const HOLD_CAP_MS = 10_000;

/** The button the hold steps: the one of the pressed one's id under `root`, else the pressed one
 * while it is on the page. */
function buttonOf(root, pressed) {
  if (pressed.id) return root.querySelector(`#${globalThis.CSS?.escape?.(pressed.id) ?? pressed.id}`);
  return pressed.isConnected ? pressed : null;
}

/**
 * Step on the pointer `event`'s press and keep stepping while it is held. `step(el)` takes one
 * step of the button `el`. `find()` gives the button to step (default: `buttonOf`), `live()` says
 * whether the page may repeat (false: the press steps once and that is all). `leave: true` ends it
 * also when the pointer leaves the button or the button loses the capture. `onStart()` and
 * `onEnd()` bracket a hold that began, once each. `el` is the pressed button (default: the
 * event's target). Returns `{ end() }`.
 */
export function holdRepeat(event, {
  root, step, find, live = () => true, leave = false, cap = HOLD_CAP_MS, onStart, onEnd, signal,
  el = event.target,
}) {
  const none = { end() {} };
  const button = find ?? (() => buttonOf(root, el));
  step(el);
  if (!live() || signal?.aborted) return none;
  event.preventDefault();
  try {
    el.setPointerCapture(event.pointerId);
  } catch {
    // a pointer already gone: its release is the end
  }
  const doc = el.ownerDocument;
  const { pointerId } = event;
  const done = new AbortController();
  let first = 0;
  let every = 0;
  let began = 0;
  let over = false;

  const end = () => {
    if (over) return;
    over = true;
    clearTimeout(first);
    clearInterval(every);
    done.abort();
    onEnd?.();
  };
  const again = () => {
    const at = button();
    if (!at || doc.hidden || !live() || Date.now() - began > cap) end();
    else step(at);
  };
  const same = (e) => e.pointerId === pointerId;

  onStart?.();
  first = setTimeout(() => {
    began = Date.now();
    every = setInterval(again, HOLD_EVERY_MS);
  }, HOLD_AFTER_MS);

  const own = { signal: done.signal };
  const edge = { signal: done.signal, capture: true };
  // A release that is never seen (a system dialog, a swallowed touch) is caught by a move with no
  // button down, and by the cap.
  doc.addEventListener('pointermove', (e) => { if (same(e) && e.buttons === 0) end(); }, edge);
  for (const type of ['pointerup', 'pointercancel']) {
    doc.addEventListener(type, (e) => { if (same(e)) end(); }, edge);
  }
  doc.addEventListener('visibilitychange', () => { if (doc.hidden) end(); }, own);
  globalThis.window.addEventListener('blur', end, own);
  // A long press on a phone must not open the context menu over a held button.
  root.addEventListener('contextmenu', (e) => {
    const held = button();
    if (held && (e.target === held || held.contains(e.target))) e.preventDefault();
  }, own);
  if (leave) {
    for (const type of ['pointerleave', 'lostpointercapture']) el.addEventListener(type, end, own);
  }
  signal?.addEventListener('abort', end, { once: true, signal: done.signal });
  return { end };
}
