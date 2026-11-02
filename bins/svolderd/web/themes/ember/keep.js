/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A part of the screen that holds a kept element (a slider, the band editor, Try now) is not
// swapped while a slider in it is being dragged, in a browser that cannot move it. htmx keeps an
// hx-preserve element with Element.moveBefore; without it, it puts the old element back with
// replaceChild, which drops the element's pointer capture and focus. The same goes while a
// stepper of the band editor is held down. The latest swap is applied when the use ends, drawn from
// the newest copy the model holds of the part (`drawNewest`), not from the swap that was held; a
// swap that goes ahead gives the focus back, to a page that has none. While the curve's point is
// dragged, and until the writes it sent are answered, the tone part waits in every browser: a
// swapped graph would drop the pointer capture and the page's own drawing.

/** Whether this browser moves a kept element in place instead of replacing it. */
export const moves = () => typeof Element !== 'undefined' && 'moveBefore' in Element.prototype;

/** The parts that hold kept elements. */
export const KEEPERS = ['part-controls', 'part-tone', 'part-card'];

/** The parts whose swap is waiting for a use to end, by id. */
const waiting = new Set();

/** The stepper button held down, or nothing. */
let gripped = null;

/** How many drags on the tone curve hold the tone part's swap, whatever the browser: a drag
 * holds once from its press and lets go once when its last write is answered, and a new drag
 * may start while the one before waits for its answers. */
let toneHolds = 0;

/** Hold the tone part's swap for one more drag on the curve (`on`), or let that drag's hold go:
 * the part is read again by [`release`] once the last has gone (`again` puts it among the swaps
 * that wait, so a drag the device did not take is read back from the server). */
export function holdTone(on, again = false) {
  toneHolds = on ? toneHolds + 1 : Math.max(0, toneHolds - 1);
  if (!on && again) waiting.add('part-tone');
}

/** Put the tone part among the swaps that wait, to be read again as soon as nothing holds it: the
 * page drew a value the device did not take. */
export function staleTone() {
  waiting.add('part-tone');
}

/** Note the button a stepper hold is on (`null` when the hold ends). */
export function grip(button) {
  gripped = button;
}

/** The parts with a request of the person's own unanswered, by id: a swap of one waits, so the
 * answer is not taken away from under what asked. */
const working = new Map();

/** A request that part `id` asked for begins (`on`) or is answered. */
export function request(id, on) {
  const left = (working.get(id) ?? 0) + (on ? 1 : -1);
  if (left > 0) working.set(id, left);
  else working.delete(id);
}

/** Whether a request of part `id` is unanswered. */
export const requested = (id) => working.has(id);

/** Whether a kept element of `part` is being used: a slider dragged or a stepper held down, a
 * pointer that is down. Focus alone does not hold a swap; it is given back after it (see
 * [`remember`]). */
export function inUse(part) {
  if (!part) return false;
  if (gripped?.isConnected && part.contains(gripped)) return true;
  for (const slider of part.querySelectorAll('sv-slider')) {
    if (slider.dragging) return true;
  }
  return false;
}

/** The focusable things of a kept element, in order. */
const FOCUSABLE = 'button, input, select, [tabindex]';

/** The kept element holding `active` and its place among the kept element's focusable things,
 * or nothing: noted before a swap that would put the element back and drop its focus. */
export function remember(active) {
  const keeper = active?.closest?.('[hx-preserve]');
  if (moves() || !keeper?.id) return null;
  const index = [...keeper.querySelectorAll(FOCUSABLE)].indexOf(active);
  return index < 0 ? null : { id: keeper.id, index };
}

/** Give the focus back to what `noted` (from [`remember`]) names, once the swap has put the
 * kept element back, if the page has no focus now (`active` is the focused element); `find`
 * looks an id up. */
export function refocus(noted, find = (i) => document.getElementById(i),
  active = document.activeElement) {
  if (active && active !== document.body) return;
  const target = noted && find(noted.id)?.querySelectorAll(FOCUSABLE)[noted.index];
  target?.focus?.({ preventScroll: true });
}

/** Whether the swap of `part` (an id) must wait: true when it does, and it is remembered. `also`
 * says whether a use of the page's own holds it (a form open, a request unanswered). */
export function holds(id, find = (i) => document.getElementById(i), also = () => false) {
  if (also(id)) {
    waiting.add(id);
    return true;
  }
  if (toneHolds > 0 && id === 'part-tone') {
    waiting.add(id);
    return true;
  }
  if (moves() || !KEEPERS.includes(id) || !inUse(find(id))) return false;
  waiting.add(id);
  return true;
}

/** Apply the swaps that waited and whose use has ended: `reload(id)` draws the part anew. `also`
 * is the page's own holds, as for [`holds`]. */
export function release(reload, find = (i) => document.getElementById(i), also = () => false) {
  for (const id of [...waiting]) {
    if (inUse(find(id)) || (toneHolds > 0 && id === 'part-tone') || also(id)) continue;
    waiting.delete(id);
    reload(id);
  }
}

/** The model's name of the part whose element has the id `id`: `part-device-presets` is
 * `device_presets`. */
export const modelName = (id) => id.replace(/^part-/, '').replace(/-/g, '_');

/** The `reload` of [`release`] that draws a held part from the newest copy `store` holds: however
 * many swaps arrived while it was held, it is drawn once, from the last. `draw(id, data)` puts
 * `data` (the part's model, `null` where the device has none) on the page. */
export function drawNewest(store, draw) {
  return (id) => draw(id, store.part(modelName(id)));
}

/** Forget the swaps that waited: a whole screen came in. */
export function clear() {
  waiting.clear();
}

/** The page is unmounted: nothing waits, nothing is held, and no request is waited for. */
export function reset() {
  waiting.clear();
  working.clear();
  gripped = null;
  toneHolds = 0;
}
