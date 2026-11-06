/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The page's state on the page itself: the mood of the page (`data-mood` on the root, which the CSS dims by),
// the card in the middle of the scene while the shown device is out of reach, and the quiet line that
// says what is not as usual (the stream lost, the page reading again or resting, no music analysis). The
// dash is out of reach (`inert`) while the device is out of reach, and the whole page but its words while the
// stream is lost. `Try now` asks the daemon to connect to the shown device through the core
// (a viewer who may not write taps it and the core says why nothing happens); the seconds to the next
// try count down once a second while the page is on show.

import { drawCard } from './draw/card.js';
import { inertOf } from './inert.js';
import { heldCard, moodOf, quietOf } from './states.js';
import { STATES } from './words.js';

/** How often the seconds are counted down (ms). */
const TICK_MS = 1000;

/** The card's markup with its moving parts (the seconds, and the clock they were drawn against) left out: what a
 * redraw compares. */
const sigOf = (html) => html.replace(/ data-secs="[^"]*" data-drawn="[^"]*">[^<]*</, '><');

/**
 * Wire the states in `root` (`#nd-cardbox`, `.nd-quiet`, `#nd-dash`, `#nd-sheet-wrap`) to `core`, `view` (its `state`
 * and `capture`) and `act` until `signal` aborts. Returns `{ draw(page) }`.
 */
export function wireCard({
  root, core, view, act, signal,
}) {
  const page = root.querySelector('#nightdrive');
  const box = root.querySelector('#nd-cardbox');
  const line = root.querySelector('.nd-quiet');
  const dash = root.querySelector('#nd-dash');
  const wrap = root.querySelector('#nd-sheet-wrap');
  const scene = root.querySelector('.nd-scene');
  let sig = '';

  /** Put the seconds left where the card says them, against the clock now. */
  function count() {
    for (const el of box.querySelectorAll('.nd-retry[data-secs]')) {
      const left = Math.max(0, Math.ceil(Number(el.dataset.secs) - (Date.now() - Number(el.dataset.drawn)) / 1000));
      const text = STATES.retryIn(left);
      if (el.textContent !== text) el.textContent = text;
    }
  }

  function draw(model) {
    const mood = moodOf(model, view.state, view.capture);
    page.dataset.mood = mood;
    const held = mood === 'unreachable' ? heldCard(model) : null;
    const html = held ? drawCard(held, { quiet: inertOf(model), now: Date.now() }) : '';
    if (sigOf(html) !== sig) {
      sig = sigOf(html);
      box.innerHTML = html;
    } else if (held && typeof held.retry_in === 'number') {
      // the same card: only the seconds the daemon counts to are said again, in place
      const el = box.querySelector('.nd-retry');
      if (el) {
        el.dataset.secs = String(held.retry_in);
        el.dataset.drawn = String(Date.now());
        count();
      }
    }
    box.hidden = html === '';
    const said = quietOf(mood, view.state, view.capture);
    if (line.textContent !== said) line.textContent = said;
    line.hidden = said === '';
    // out of reach: the dash. Lost: the whole page but its words (the top line and the quiet line stay in the
    // accessibility tree so they are read out) and the frame, which is outside the page: the scene, the dash, the
    // card and the sheet
    const lost = mood === 'lost';
    dash.inert = lost || mood === 'unreachable';
    wrap.inert = lost;
    // the sheet, while it stands, keeps the scene out of reach whatever the mood says
    scene.inert = lost || page.dataset.sheet === 'open';
    box.inert = lost;
  }

  box.addEventListener('click', (event) => {
    if (!event.target.closest?.('[data-act="try"]')) return;
    const device = core.model.shown();
    act.act('connect', true, (actions) => actions.connect({ device })).catch(() => {});
  }, { signal });

  const timer = setInterval(() => {
    if (globalThis.document?.hidden) return;
    count();
  }, TICK_MS);
  signal.addEventListener('abort', () => clearInterval(timer), { once: true });

  return { draw };
}
