/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash fades to a fifth with the HUD, when it is over the scene (a desktop, a phone held sideways; the
// panel of a phone held upright is under the scene and stays). The root carries `nd-dash-dim` while it is
// faded; the stylesheet does the rest (opacity only, no transition under reduced motion). It stays as it is
// while a key is held or a list is open (the dash is held), while it has the keyboard focus and while a mouse
// rests over it. Whatever wakes the HUD wakes it, and the input that wakes a faded dash only wakes it: the
// press or tap that did is not given to the key under the finger, however long it is held before it is released.

/** How long after the release of a press that only woke the dash its click may still come, ms. */
const CLICK_MS = 100;

/** How often a faded dash looks again at what keeps it up, ms. */
const LOOK_MS = 500;

/** The windows where the dash is a panel under the scene (the stylesheet's phone held upright). */
const PANEL = '(max-width: 719px) and (min-height: 500px)';

/**
 * Wire the dash fade of `page` (`#nightdrive`, holding `#nd-dash`) to `fade` (hud-wire.js `createFade`) and
 * `live` (its `held('dash')`) until `signal` aborts. Returns `{ dimmed() }`.
 */
export function wireDashFade({
  page, fade, live, signal,
}) {
  const doc = page.ownerDocument;
  const dash = page.querySelector('#nd-dash');
  const win = doc.defaultView;
  let dimmed = false;
  let over = false;
  let looking = null;
  let woke = null;
  let deaf = false;

  const panel = () => win.matchMedia(PANEL).matches;
  const kept = () => live.held('dash') || dash.querySelector(':focus-visible') !== null || over;

  function set(on) {
    dimmed = on;
    page.classList.toggle('nd-dash-dim', on);
  }

  function look() {
    looking = null;
    if (!fade.faded()) return;
    if (!dimmed && !kept() && !panel()) set(true);
    else if (dimmed && (kept() || panel())) set(false);
    looking = setTimeout(look, LOOK_MS);
  }

  fade.watch((kind, event) => {
    clearTimeout(looking);
    looking = null;
    if (kind === 'fade') {
      look();
      return;
    }
    // the input that wakes a dimmed dash is only that, if it lands on the dash
    if (dimmed && event?.type === 'pointerdown' && dash.contains(event.target)) woke = event;
    if (dimmed) set(false);
  });

  // The press that woke a dimmed dash goes no further, and neither does its click, whenever it comes: the press is
  // deaf from its start until just after its release. Any other press starts as a plain one.
  // (a key press also clears it, so a keyboard press is never swallowed, and no lost release leaves it stuck)
  doc.addEventListener('pointerdown', () => { deaf = false; }, { capture: true, signal });
  doc.addEventListener('keydown', () => { deaf = false; }, { capture: true, signal });
  dash.addEventListener('pointerdown', (event) => {
    if (woke !== event) return;
    woke = null;
    deaf = true;
    event.stopPropagation();
    event.preventDefault();
  }, { capture: true, signal });
  let letGo = null;
  const released = () => {
    if (!deaf) return;
    clearTimeout(letGo);
    letGo = setTimeout(() => { deaf = false; }, CLICK_MS);
  };
  doc.addEventListener('pointerup', released, { capture: true, signal });
  doc.addEventListener('pointercancel', released, { capture: true, signal });
  dash.addEventListener('click', (event) => {
    if (!deaf) return;
    event.stopPropagation();
    event.preventDefault();
  }, { capture: true, signal });

  dash.addEventListener('pointerenter', (event) => { over = event.pointerType === 'mouse'; }, { signal });
  dash.addEventListener('pointerleave', () => { over = false; }, { signal });
  win.addEventListener('resize', () => { if (dimmed && panel()) set(false); }, { signal });

  signal.addEventListener('abort', () => {
    clearTimeout(looking);
    set(false);
  }, { once: true });

  return { dimmed: () => dimmed };
}
