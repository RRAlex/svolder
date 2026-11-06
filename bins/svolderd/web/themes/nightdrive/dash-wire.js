/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash drawn from the model and the hands on it: `−` and `+` (a press steps once and a held one
// repeats through the core's hold), `MUTE`, the input's chips and list, the transport and `DEVICE ▴`.
// Every write goes through `core.actions` (the cap, the pacing, the refusals and the glide stay
// where they are) and is logged by `act`. A viewer who may not write taps the same keys and the
// core says why nothing happens.
//
// A step is asked as `view.asked = { raw, at }` before it is posted, so the reading that answers it
// is not taken for another hand's (hud-wire.js); a raise starts from the step asked while it is
// fresh. The dash is held (`live.hold`) while a key is pressed or a list is open, so a reading
// never replaces the key under the finger; what came meanwhile is drawn when the hand lets go.

import { drawDash, moveProgress, placeFills } from './draw/dash.js';
import { inertOf } from './inert.js';
import { closeList, closeOf, openList } from './lists.js';

const STEPS = '[data-act="volume-down"], [data-act="volume-up"]';

/** How long a step asked for stays the base of the next one while the device has not said, ms. */
const ASK_MS = 3000;

/** How often the progress moves, ms. */
const TICK_MS = 1000;

/**
 * Wire the dash in `root` (`#nd-dash`) to `core` and `view` until `signal` aborts. `live` is
 * `createLive`'s handle (`hold`/`release`), `act` the write log. Returns `{ draw(page) }`.
 */
export function wireDash({
  root, core, live, act, view, signal,
}) {
  const dash = root.querySelector('#nd-dash');
  const store = core.model;
  const now = () => globalThis.performance.now();
  let drawn = { sig: null, at: Date.now() };
  let hold = null;
  let shown = store.shown();

  const device = () => store.page().device;
  const volume = () => store.part('volume');
  const inert = () => inertOf(store.page());
  /** The page may repeat a held key: it may write and its stream is open. */
  const writable = () => !inert() && view.state === 'open';
  const climbing = (v) => v.target !== null && v.target !== undefined && v.raw !== null && v.target > v.raw;

  // ---- drawing ----
  /** The markup with the moving parts (the fill and the time) left out: what a redraw compares. */
  const sigOf = (html) => html.replace(/ data-at="[^"]*"/, '').replace(/<span class="nd-time">[^<]*<\/span>/, '');

  function draw(page) {
    const at = Date.now();
    const html = drawDash(page, view, at);
    const sig = sigOf(html);
    if (sig === drawn.sig) {
      // the same keys: only the progress moves, in place
      drawn.at = at;
      moveProgress(dash, page, at, at, true);
      return;
    }
    const focus = dash.contains(document.activeElement) ? document.activeElement.id : '';
    dash.innerHTML = html;
    drawn = { sig, at };
    placeFills(dash);
    if (focus) dash.querySelector(`#${CSS.escape(focus)}`)?.focus({ preventScroll: true });
  }

  // A cover the daemon no longer holds is not shown, and leaves no gap.
  dash.addEventListener('error', (event) => {
    if (event.target.matches?.('.nd-cover img')) event.target.closest('.nd-cover').hidden = true;
  }, { capture: true, signal });

  // The progress moves once a second while the player plays and the page is on show.
  const timer = setInterval(() => {
    if (globalThis.document?.hidden) return;
    moveProgress(dash, store.page(), Date.now(), drawn.at);
  }, TICK_MS);
  signal.addEventListener('abort', () => clearInterval(timer), { once: true });

  // ---- the volume ----
  /** The step a change starts from: a raise from the step asked while it is fresh or the climb's
   * target, a lowering from the level (a lower step ends a climb where the device is). */
  function base(v, delta) {
    const asked = view.asked && now() - view.asked.at < ASK_MS ? view.asked.raw : null;
    if (delta < 0) return Math.min(asked ?? v.raw, v.raw);
    return asked ?? (climbing(v) ? v.target : v.raw);
  }

  async function send(step, target) {
    const mine = view.asked;
    const answer = await act.act('volume', step, (actions) => actions.setVolume(step, { device: target }));
    if (signal.aborted || answer.queued || answer.dropped) return;
    if (view.asked !== mine || !mine) return;
    if (!answer.ok) {
      view.asked = null;
      return;
    }
    // The daemon may land the write on another step (the quietest, under a cap below every step).
    const landed = answer.landed?.raw;
    if (typeof landed === 'number' && landed !== mine.raw) view.asked = { raw: landed, at: now() };
    // The device has the step: a reading that comes after this and differs from it is another
    // hand's (the listener below).
    if (view.asked) view.asked.acked = true;
  }

  /** Ask for step `raw` of `target`: held to the range and the cap, noted in `view.asked`, sent. A
   * viewer who may not write is told so by the core and nothing is sent. */
  function ask(raw, target = store.shown()) {
    const v = volume();
    if (!v || v.raw === null || v.raw === undefined) return;
    let step = Math.min(v.hi, Math.max(v.lo, Math.round(raw)));
    if (v.cap !== null && v.cap !== undefined && step > v.cap) step = v.cap;
    if (inert()) {
      core.actions.setVolume(step, { device: target });
      return;
    }
    // The step already asked for stands only while it is fresh and the device has not read another
    // step since it had it: that reading is another hand's, and the same ask is a new one; a
    // device that answered and read nothing does not hold the step past ASK_MS.
    const same = (view.asked?.raw === step && now() - view.asked.at < ASK_MS
      && (!view.asked.answered || v.raw === step))
      || (!view.asked && step === v.raw && !climbing(v));
    if (same) return;
    view.asked = { raw: step, at: now() };
    send(step, target).catch(() => {});
  }

  const nudge = (delta, target) => {
    const v = volume();
    // A viewer who may not write is told so even where the device has no volume reading (the core
    // turns the call away before it looks at the step).
    const viewer = store.page().viewer ?? {};
    if (viewer.guest === true || viewer.writes === false) {
      core.actions.setVolume(0, { device: target });
      return;
    }
    if (v && v.raw !== null && v.raw !== undefined) ask(base(v, delta) + delta, target);
  };

  // A reading that is not the answer to the step asked is another hand's: the next step starts from it.
  const offVolume = store.on('volume', (data) => {
    const a = view.asked;
    if (!a?.acked || data.raw === undefined) return;
    if (data.raw !== a.raw) view.asked = null;
    else a.answered = true;
  });
  signal.addEventListener('abort', offVolume, { once: true });

  // Another device shown ends what was asked of the old one: a hold, a list, the step asked.
  const offModel = store.on('model', () => {
    if (store.shown() === shown) return;
    shown = store.shown();
    view.asked = null;
    hold?.end();
    closeList();
  });
  signal.addEventListener('abort', offModel, { once: true });

  // A held `−` or `+` is the core's hold: one step at the press and, after a pause, steps at a steady
  // pace. The dash is held while a key is down, so a reading does not replace the key under the
  // finger; the hold also ends when the pointer leaves the key.
  dash.addEventListener('pointerdown', (event) => {
    const el = event.target.closest?.(STEPS);
    if (!el || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    const target = store.shown();
    hold = core.hold.repeat(event, {
      root: dash,
      el,
      step: (at) => nudge(at.dataset.act === 'volume-up' ? 1 : -1, target),
      live: writable,
      leave: true,
      // No time cap, as the volume of Signal: a held key runs until the release, the window's blur, a
      // hidden page, a move with no button down, a lost stream or the cap on the volume itself.
      cap: Infinity,
      onStart: () => live.hold('dash'),
      onEnd: () => {
        hold = null;
        live.release('dash');
      },
      signal,
    });
  }, { signal });
  dash.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPS)) event.preventDefault();
  }, { signal });

  // ---- a list given back only once the press that closed it is over ----
  let pressed = false;
  const over = () => { pressed = false; };
  const doc = dash.ownerDocument;
  doc.addEventListener('pointerdown', () => { pressed = true; }, { capture: true, signal });
  doc.addEventListener('pointerup', over, { capture: true, signal });
  doc.addEventListener('pointercancel', over, { capture: true, signal });
  function giveBack() {
    if (!pressed) {
      setTimeout(() => live.release('dash'), 0);
      return;
    }
    const done = () => {
      doc.removeEventListener('pointerup', done, true);
      doc.removeEventListener('pointercancel', done, true);
      setTimeout(() => live.release('dash'), 0);
    };
    doc.addEventListener('pointerup', done, { capture: true, signal });
    doc.addEventListener('pointercancel', done, { capture: true, signal });
  }

  // ---- the other keys ----
  const write = (field, value, call) => act.act(field, value, call).catch(() => {});

  dash.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el) return;
    const what = el.dataset.act;
    if (el.matches(STEPS)) {
      // A press of the pointer stepped already; a click with no pointer is a key: one step.
      if (event.detail === 0) nudge(what === 'volume-up' ? 1 : -1);
      return;
    }
    const v = volume();
    const d = device();
    if (what === 'mute' && v) {
      const value = !v.muted;
      write('mute', value, (a) => a.setControl(el.dataset.key, { value }));
    } else if (what === 'input') {
      const c = (d?.controls ?? []).find((x) => x.key === 'input' && x.kind === 'choice');
      if (!c) return;
      if (el.dataset.list) {
        if (inert()) {
          const cur = c.options?.find((o) => o.on) ?? c.options?.[0];
          if (cur) write('control:input', cur.label, (a) => a.setControl('input', JSON.parse(cur.json)));
          return;
        }
        if (closeOf(el)) return;
        live.hold('dash');
        openList({
          opener: el,
          label: c.label ?? 'Input',
          options: (c.options ?? []).map((o) => ({ value: o.label, text: o.label, on: o.on })),
          choose(label) {
            const option = c.options.find((o) => o.label === label);
            if (option) write('control:input', option.label, (a) => a.setControl('input', JSON.parse(option.json)));
          },
          signal,
          onClose: giveBack,
        });
      } else {
        const option = c.options?.find((o) => o.label === el.dataset.value);
        if (option) write('control:input', option.label, (a) => a.setControl('input', JSON.parse(option.json)));
      }
    } else if (what === 'power') {
      const p = d?.power;
      if (p?.json) write(`control:${p.key}`, p.on !== true, (a) => a.setControl(p.key, JSON.parse(p.json)));
    } else if (what === 'prev' || what === 'play' || what === 'next') {
      write('transport', el.dataset.button, (a) => a.press(el.dataset.button));
    } else if (what === 'sheet') {
      view.openSheet(el);
    }
  }, { signal });

  signal.addEventListener('abort', () => {
    hold?.end();
    closeList();
  }, { once: true });

  return { draw };
}
