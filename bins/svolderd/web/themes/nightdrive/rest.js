/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The mouse cursor over the scene rests with it: after 2 s with no move over the scene (not over the
// dash, the HUD or a menu, and not while a button is held, as in a drag on the mountains) the root
// carries `nd-rest`, whose CSS hides the cursor over the scene; the next move brings it back. For a
// fine pointer only: a finger has no cursor to hide.

/** How long the pointer must be still over the scene before the cursor goes, ms. */
export const REST_MS = 2000;

/**
 * Hide the cursor of `root` (its class `nd-rest`) after `REST_MS` with none of `pointermove`,
 * `pointerdown` or `pointerup` on `doc`, while the last of them was over the scene (`.nd-scene`) with no
 * button held. `keepOut()` lists the elements (the HUD's words: click-through, so the scene is what the
 * pointer is over) whose boxes keep the cursor. `fine` is whether the primary pointer is not a coarse one (a phone's); `schedule(fn, ms)` runs `fn` later and
 * gives the way to cancel it. Returns `{ stop() }`.
 */
export function createRest({
  root,
  doc = globalThis.document,
  fine = true,
  keepOut = () => [],
  schedule = (fn, ms) => {
    const id = setTimeout(fn, ms);
    return () => clearTimeout(id);
  },
}) {
  if (!fine) return { stop() {} };
  const classes = root.classList;
  let over = false;
  let pressed = false;
  let cancel = null;

  function arm() {
    cancel?.();
    cancel = schedule(tick, REST_MS);
  }

  function tick() {
    cancel = null;
    if (pressed) {
      arm();
      return;
    }
    if (over) classes.add('nd-rest');
  }

  function seen(event) {
    classes.remove('nd-rest');
    pressed = event.buttons !== 0 && event.type !== 'pointerup';
    const inside = (el) => {
      const r = el.getBoundingClientRect();
      return event.clientX >= r.left && event.clientX <= r.right && event.clientY >= r.top && event.clientY <= r.bottom;
    };
    over = event.pointerType !== 'touch' && Boolean(event.target?.closest?.('.nd-scene')) && !keepOut().some(inside);
    arm();
  }

  const types = ['pointermove', 'pointerdown', 'pointerup'];
  for (const type of types) doc.addEventListener(type, seen, { capture: true, passive: true });
  return {
    stop() {
      for (const type of types) doc.removeEventListener(type, seen, { capture: true });
      cancel?.();
      cancel = null;
      classes.remove('nd-rest');
    },
  };
}
