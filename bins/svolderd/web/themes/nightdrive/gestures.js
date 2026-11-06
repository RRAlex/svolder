/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The two ways a hand moves a scale: dragging along it and the keys. Both are given an
// element that stays on the page (the section's box) and look for the thing they work on inside it
// at the moment of the touch or the key, so the section may be drawn again between gestures.

/**
 * Drag along a track: a press picks the element `pick(event)` names (nothing: the press is not
 * this gesture's), `onStart(fraction, event)` hears where along it the press landed, `onMove(fraction)`
 * each move and `onEnd(fraction)` the release (`null` when the browser took the pointer away: a
 * scroll, a cancel). Fractions are 0 to 1 of the picked element's width. The pointer is captured
 * by `el`, so a drag that leaves the track goes on.
 */
export function dragAlong(el, {
  signal, pick, onStart, onMove, onEnd,
}) {
  let drag = null;
  const fraction = (event) => {
    const box = drag.track.getBoundingClientRect();
    return box.width > 0 ? Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)) : 0;
  };
  const finish = (event, cancelled) => {
    if (!drag || (event && event.pointerId !== drag.id)) return;
    const last = cancelled ? null : fraction(event);
    drag = null;
    onEnd(last);
  };
  el.addEventListener('pointerdown', (event) => {
    if (drag || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    const track = pick(event);
    if (!track) return;
    drag = { id: event.pointerId, track };
    try {
      el.setPointerCapture(event.pointerId);
    } catch {
      // a pointer already gone: the release will not come, the cancel below ends it
    }
    event.preventDefault();
    onStart(fraction(event), event);
  }, { signal });
  el.addEventListener('pointermove', (event) => {
    if (drag && event.pointerId === drag.id) onMove(fraction(event));
  }, { signal });
  el.addEventListener('pointerup', (event) => finish(event, false), { signal });
  el.addEventListener('pointercancel', (event) => finish(event, true), { signal });
  el.addEventListener('lostpointercapture', (event) => finish(event, true), { signal });
}

/** The change in steps each key makes (no End: no key jumps to the loudest). */
const KEY_STEPS = {
  ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1, PageUp: 10, PageDown: -10,
};

/**
 * The keys of a slider: arrows change it by one step (up and right more, down and left less),
 * Page Up and Page Down by ten, Home goes to the bottom. Only for keys pressed on an element of
 * `el` that `match` (a selector) names; others are left to the page. `step` and `home` are given
 * that element too.
 */
export function sliderKeys(el, {
  signal, match, step, home,
}) {
  el.addEventListener('keydown', (event) => {
    const target = event.target.closest?.(match);
    if (event.altKey || event.ctrlKey || event.metaKey || !target) return;
    if (event.key === 'Home') {
      event.preventDefault();
      home(target);
    } else if (event.key in KEY_STEPS) {
      event.preventDefault();
      step(KEY_STEPS[event.key], target);
    }
  }, { signal });
}
