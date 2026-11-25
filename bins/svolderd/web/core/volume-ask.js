/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The step a page asked the volume for, as every theme keeps it: `view.asked` is
// `{ raw, at, read?, seen?, acked?, answered? }` while the page's own ask waits for the device to
// read it back (`at` in the ms of the clock the theme gives as `view.now`). Until the device
// reports that step the ask stands for the reading, for `ASK_MS` at most; once the device has
// answered the ask, a reading that is not the ask (and is no glide up to a target) is another
// hand's and wins.
//
// Pure rules (`shownRaw`, `baseOf`), the two steps that mark an ask (`noteReading`, `ackAsk`), one
// timer (`askExpiry`) that ends a stale ask, and the asker (`createVolumeAsker`) that holds them
// together for every theme: the cap clamp, the "same ask" test, the landed fold.

/** How long a step asked for stays the one shown while the device has not reported it (ms). */
export const ASK_MS = 3000;

/** Whether the device is climbing to a step above where it is (a glide). */
export const gliding = (volume) => volume.target !== null && volume.target !== undefined
  && volume.target > (volume.raw ?? 0);

/**
 * The step the volume stands at: the step asked for while it is not yet the one the device reports
 * and was not asked too long ago, else the one the device reports (or `null`). `view` is
 * `{ asked, now }`.
 */
export function shownRaw(volume, view) {
  const asked = view.asked;
  if (!asked || volume.raw === null || volume.raw === undefined) return volume.raw ?? null;
  if (volume.raw === asked.raw || view.now - asked.at >= ASK_MS) return volume.raw;
  if (asked.answered && !gliding(volume)) return volume.raw;
  return asked.raw;
}

/**
 * The step a change by `delta` starts from: a raise from the step asked while it is the one shown
 * (else from the climb's target, else the level), a lowering from the level (a lower step ends a
 * climb where the device is).
 */
export function baseOf(volume, view, delta) {
  const asked = view.asked && shownRaw(volume, view) === view.asked.raw ? view.asked.raw : null;
  if (delta < 0) return Math.min(asked ?? volume.raw, volume.raw);
  const climbing = volume.target !== null && volume.target !== undefined && volume.raw !== null
    && volume.target > volume.raw;
  return asked ?? (climbing ? volume.target : volume.raw);
}

/**
 * A reading of the volume arrived while `view.asked` stands. Once the device has answered the write
 * (`acked`) the reading answers the ask (`answered`) and the call says so (`true`: the section is
 * shown again). Before that the reading is kept for `ackAsk`, because the write's answer and the
 * stream's reading can come in either order: `read` is the last one, `seen` latches that the
 * device read the step asked (a later reading before the answer is then another hand's).
 */
export function noteReading(view, raw) {
  const asked = view.asked;
  if (!asked || raw === undefined) return false;
  if (!asked.acked) {
    asked.read = raw;
    if (raw === asked.raw) asked.seen = true;
    return false;
  }
  if (asked.answered) return false;
  asked.answered = true;
  return true;
}

/**
 * The device answered the write of `asked` (the object `view.asked` held at the send): marks it
 * `acked`, and `answered` when the device had read the step asked before the answer came. Says
 * whether it answered the ask (`true`: the section is shown again).
 */
export function ackAsk(view, asked) {
  asked.acked = true;
  if (!asked.seen) return false;
  asked.answered = true;
  return true;
}

/**
 * The timer that ends a stale ask: `arm()` (after an ask) calls `redraw()` once `ASK_MS` have
 * passed since `view.asked` was made, having cleared it, so the section shows the reading. `now()`
 * is the clock of `view.asked.at`. `clear()` stops it; so does `signal`.
 */
export function askExpiry({ view, now, redraw, signal }) {
  let timer = null;
  const clear = () => clearTimeout(timer);
  signal.addEventListener('abort', clear, { once: true });
  return {
    arm() {
      clear();
      timer = setTimeout(() => {
        if (view.asked && now() - view.asked.at >= ASK_MS) {
          view.asked = null;
          redraw();
        }
      }, ASK_MS + 50);
    },
    clear,
  };
}

/**
 * The volume ask every theme shares. `volume()` is the reading `{ raw, target, hi, lo, cap }` or
 * `null` (nothing is asked then); `inert()` says the viewer may not write: `refuse(step, device)`
 * then makes the core say so and nothing is shown or sent. `write(step, device)` posts the step and
 * resolves with the core's answer; `show()` draws the ask (optional); `expiry` is an `askExpiry`
 * (optional); `shown()` names the device an ask goes to when the call names none (optional);
 * `now()` is the clock of `view.asked.at`; `signal` ends the wait for an answer. The asker never
 * writes into `view` but `view.asked`.
 * Returns `{ ask(raw, device), nudge(delta, device), base(v, delta) }`.
 *
 * An ask is skipped only when it cannot change where the device ends up: nothing is in flight
 * unanswered and the step is the device's level, or the step is the ask in flight while that is
 * fresh (no flood). A lost ask older than `ASK_MS` is sent again, and a step that differs from the
 * one in flight is always sent (it queues behind it, newest wins).
 * The step is held to the range and the cap here; the core's `setVolume` clamps again, as the last
 * guard before the daemon, which enforces the rule.
 */
export function createVolumeAsker({
  view, signal, volume, inert, refuse, write, show = () => {}, expiry = null, now,
  shown = () => undefined,
}) {
  async function send(step, device) {
    const mine = view.asked;
    const answer = await write(step, device);
    if (signal.aborted || answer.queued || answer.dropped) return;
    // Another ask has taken the place of this one meanwhile: its answer is not for that.
    if (view.asked !== mine || !mine) return;
    if (!answer.ok) {
      view.asked = null;
      show();
      return;
    }
    // The daemon may land the write on another step (the quietest, under a cap below every step).
    const landed = answer.landed?.raw;
    if (typeof landed === 'number' && landed !== mine.raw) {
      view.asked = { raw: landed, at: now(), read: mine.read, seen: mine.read === landed };
      show();
    }
    // The device has the step: a reading that comes after this and differs from it is another
    // hand's (`answered`, set by that reading, or at once by one that came first).
    if (ackAsk(view, view.asked)) show();
  }

  /** Ask for step `raw`: rounded, held to the range and the cap, shown at once, and sent. */
  function ask(raw, device = shown()) {
    const v = volume();
    if (!v || v.raw === null || v.raw === undefined) return;
    let step = Math.min(v.hi, Math.max(v.lo, Math.round(raw)));
    if (v.cap !== null && v.cap !== undefined && step > v.cap) step = v.cap;
    if (inert()) {
      refuse(step, device);
      return;
    }
    const at = now();
    const last = view.asked;
    // Skipped only when it cannot change where the device ends up: nothing is in flight unanswered
    // and the step is the level (a glide is no level), or the step is the ask in flight while that
    // is fresh. Anything else is sent, so a press behind a stale raise still lands.
    const flying = last && !last.answered;
    const fresh = last && last.raw === step && at - last.at < ASK_MS
      && (!last.answered || v.raw === step);
    if (fresh || (!flying && step === v.raw && !gliding(v))) return;
    view.asked = { raw: step, at };
    expiry?.arm();
    show();
    send(step, device).catch(() => {});
  }

  /** The step a change by `delta` starts from (`baseOf`), on the asker's clock. The theme's view is
   * only read: its `now` may be a getter. */
  function base(v, delta) {
    return baseOf(v, { asked: view.asked, now: now() }, delta);
  }

  /** Ask for the step `delta` from the base. */
  function nudge(delta, device = shown()) {
    const v = volume();
    if (v && v.raw !== null && v.raw !== undefined) ask(base(v, delta) + delta, device);
  }

  return { ask, nudge, base };
}
