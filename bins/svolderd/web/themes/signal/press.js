/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Giving a hold back without taking the button from under the press that closed what held it. A list
// closed by a press outside it (on another chip) gives its hold back only after that press is over,
// so the redraw cannot replace the chip under the click that follows.

/**
 * The way to give back one hold of `keep` (a `keeper`): `release()` gives it back on the next task,
 * or once the press now going is over (pointer up or cancel on `doc`). Listens until `signal`
 * aborts.
 */
export function createRelease(doc, keep, signal) {
  let pressed = false;
  doc.addEventListener('pointerdown', () => { pressed = true; }, { capture: true, signal });
  const over = () => { pressed = false; };
  doc.addEventListener('pointerup', over, { capture: true, signal });
  doc.addEventListener('pointercancel', over, { capture: true, signal });

  return function release() {
    if (!pressed) {
      keep.dropLater();
      return;
    }
    let finished = false;
    const done = () => {
      if (finished) return;
      finished = true;
      doc.removeEventListener('pointerup', done, true);
      doc.removeEventListener('pointercancel', done, true);
      keep.dropLater();
    };
    doc.addEventListener('pointerup', done, { capture: true, signal });
    doc.addEventListener('pointercancel', done, { capture: true, signal });
  };
}
