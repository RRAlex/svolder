/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the strip a phone has: a finger drawn along it slides the track, and past a short
// distance it chooses the neighbour on the far side; a tap on a hop is the click the line's own
// listener hears. The strip keeps `touch-action: pan-y`, so a vertical stroke still scrolls the
// page. A drag that moved is not also a tap.

import { HOP_PX, slide } from './draw/line.js';

/** How far a finger must pull before it is a drag (px), and how far for a hop to be chosen. */
const SLOP = 8;
const THRESHOLD = 40;

/**
 * Wire the strip `band` until `signal` aborts. `hops()` lists the hops of the line, `chosen()` is
 * the one chosen, `choose(id)` chooses another.
 */
export function wireStrip({
  band, hops, chosen, choose, signal,
}) {
  let touch = null;
  let moved = false;

  const track = () => band.querySelector('.sg-slide');

  /** Pull the track along with the finger, at most one step either way. */
  function pull(dx) {
    const el = track();
    if (!el) return;
    el.classList.add('sg-drag');
    band.dataset.dragging = '';
    el.style.transform = slide(Number(el.dataset.at), Math.max(-HOP_PX, Math.min(HOP_PX, dx)));
  }

  /** Let go of the track: back where the chosen hop is (a glide). */
  function settle() {
    const el = track();
    if (!el) return;
    el.classList.remove('sg-drag');
    el.style.transform = slide(Number(el.dataset.at));
  }

  band.addEventListener('pointerdown', (event) => {
    if (!track() || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    touch = { id: event.pointerId, x: event.clientX, dragging: false };
    moved = false;
  }, { signal });

  band.addEventListener('pointermove', (event) => {
    if (!touch || event.pointerId !== touch.id) return;
    const dx = event.clientX - touch.x;
    if (!touch.dragging) {
      if (Math.abs(dx) < SLOP) return;
      touch.dragging = true;
      moved = true;
      try {
        band.setPointerCapture(event.pointerId);
      } catch {
        // a pointer the browser no longer has: the drag goes on without the capture
      }
    }
    pull(dx);
  }, { signal });

  function end(event, cancelled) {
    if (!touch || event.pointerId !== touch.id) return;
    const { dragging, x } = touch;
    touch = null;
    delete band.dataset.dragging;
    if (!dragging) return;
    const dx = event.clientX - x;
    const list = hops();
    const at = list.findIndex((hop) => hop.id === chosen());
    const to = !cancelled && Math.abs(dx) >= THRESHOLD ? at + (dx < 0 ? 1 : -1) : at;
    // a choice draws the strip again, from where the finger left the track; else it slides back
    if (to !== at && list[to]) choose(list[to].id);
    else settle();
    // the click that follows the end of a drag is not a tap: `moved` holds until the next press or key
  }
  band.addEventListener('pointerup', (event) => end(event, false), { signal });
  band.addEventListener('pointercancel', (event) => end(event, true), { signal });

  band.addEventListener('keydown', () => { moved = false; }, { signal });

  band.addEventListener('click', (event) => {
    if (!moved) return;
    event.stopPropagation();
    event.preventDefault();
  }, { capture: true, signal });
}
