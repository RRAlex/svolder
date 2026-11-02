/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume's knob: the theme draws its dial whole; this repaints it as readings arrive —
// the ticks lit up to the level (none at silence), the value arc, the handle, a climb's dotted
// arc and target tick, the readout and its notes — by the geometry the page's drawing uses
// (draw/dial.js): view box -30 -16 260 220, centre (100, 100), 270° from -135°, 0° up.

import { ember } from './bind.js';
import {
  angle as dialAngle, arc, fraction, point, SWEEP, spoke as spokeAt, tickClass as classOf,
} from './draw/dial.js';

/** How far from the centre a click still counts as the ring's, in radii (the dial's box has
 * empty corners beyond it). */
const RING_REACH = 1.35;
const SVG = 'http://www.w3.org/2000/svg';

/** The pixels of wheel travel that make one step; a notch of 40 or more is a step of its own. */
const WHEEL_STEP = 40;
/** The steps' worth of travel it takes to cross between silence and the step above it. */
const DETENT = 4;
/** A held − or + repeats after this long, then at this pace (ms). */
const HOLD_AFTER = 380;
const HOLD_EVERY = 70;
/** How long a step asked stays the one shown while the device has not echoed it (ms). */
const ASKED_FOR = 3000;
/** How long the lit ticks, the arc, the handle and the numeral take to move between two
 * readings: less than the 84 ms between a climb's steps, so each reported step is reached
 * before the next arrives. */
const EASE_MS = 70;

class Knob extends HTMLElement {
  connectedCallback() {
    // Kept across swaps, and connected again each time: what it has been sent stands.
    if (this.ready) {
      // Moved by a swap that kept it: listen on the window again.
      if (this.leaving.signal.aborted) {
        this.leaving = new AbortController();
        this.blurs();
      }
      return;
    }
    this.ready = true;
    const d = this.dataset;
    this.lo = Number(d.lo);
    this.hi = Number(d.hi);
    this.silence = d.silence === 'true';
    this.risk = d.risk === '' ? null : Number(d.risk);
    this.cap = d.cap === '' ? null : Number(d.cap);
    this.texts = JSON.parse(d.texts || '[]');
    this.dbs = JSON.parse(d.db || '[]');
    this.raw = d.raw === '' ? null : Number(d.raw);
    this.target = d.target === '' ? null : Number(d.target);
    const num = this.querySelector('.big .num')?.textContent ?? '–';
    const unit = this.querySelector('.big .unit')?.textContent;
    /** The device's own words for the level, as the last reading or the page said them. */
    this.words = unit ? `${num} ${unit}` : num;
    this.svg = this.querySelector('svg.dial');
    // Drawn inert for a guest or a read-only page, whatever the reading.
    this.locked = d.inert === 'true';
    // The step the user has asked for and the device has not yet echoed: what the knob shows.
    this.asked = null;
    this.askedAt = 0;
    // The climb target a touch cancelled, ignored in readings until the stream drops it.
    this.cancelled = null;
    this.travel = 0;
    this.leaving = new AbortController();
    this.wire();
  }

  /** The knob left the page: what listens on the window goes with it. */
  disconnectedCallback() {
    this.leaving.abort();
  }

  /** A window that loses focus ends a held − or +; the listener goes when the knob leaves. */
  blurs() {
    window.addEventListener('blur', this.stopHold, { signal: this.leaving.signal });
  }

  /** Whether the knob takes input: not inert, and the volume has been read. */
  live() {
    return !this.locked && this.raw !== null;
  }

  /** Whether a climb is shown: its target is above the level. */
  climbing() {
    return this.target !== null && this.raw !== null && this.target > this.raw;
  }

  /** Ask for step `raw`: held at the cap, shown at once, and sent. */
  ask(raw) {
    let step = Math.min(this.hi, Math.max(this.lo, Math.round(raw)));
    if (this.cap !== null && step > this.cap) step = this.cap;
    // Nothing to say: the step is the one already asked, or the level the device holds.
    if (step === this.asked || (this.asked === null && step === this.raw && !this.climbing())) {
      return;
    }
    this.asked = step;
    this.askedAt = Date.now();
    this.show();
    this.write(step);
  }

  /** Write step `step`: the core holds one write in flight and the newest waiting, and says why a
   * refusal was one. The daemon may land the write on another step (the quietest, under a cap
   * below every step): that step is the one asked, unless a newer ask has replaced it. */
  async write(step) {
    const { core, signal } = ember;
    const answer = await core.actions.setVolume(step);
    if (signal.aborted || answer.queued || answer.dropped) return;
    if (!answer.ok) {
      this.asked = null;
      this.show();
      return;
    }
    const landed = answer.landed?.raw;
    if (typeof landed === 'number' && this.asked !== null && landed !== this.asked) {
      this.asked = landed;
      this.show();
    }
  }

  /** The step a change by `delta` starts from: a raise from what is asked or the climb's target,
   * a lowering from the level (a lower step ends a climb where the device is). */
  base(delta) {
    const level = this.raw;
    if (delta < 0) return Math.min(this.asked ?? level, level);
    return this.asked ?? (this.climbing() ? this.target : level);
  }

  /** Move by `steps` whole steps (a held key, a button, Page Up/Down): no detent but a step's. */
  nudge(steps) {
    if (!this.live()) return;
    this.ask(this.base(steps) + steps);
  }

  /**
   * Move by `units` of travel (a drag, the wheel): one step a unit, except across the silence
   * detent, where the step between silence and the one above takes [`DETENT`] units either way.
   */
  move(units) {
    this.travel += units;
    for (;;) {
      const dir = Math.sign(this.travel);
      if (dir === 0) return;
      const from = this.base(dir);
      const crossing = this.silence && (dir > 0 ? from === this.lo : from === this.lo + 1);
      const need = crossing ? DETENT : 1;
      if (Math.abs(this.travel) < need) return;
      this.travel -= dir * need;
      if (from + dir < this.lo || from + dir > this.hi) {
        this.travel = 0;
        return;
      }
      this.ask(from + dir);
    }
  }

  /** A touch on the dial: a climb that is shown ends before anything else is written. */
  touch() {
    if (this.climbing()) {
      // The stream's next reading says the same; until then no step starts from the target,
      // and a climb step's reading already on its way does not bring it back.
      const { core, signal } = ember;
      const climb = core.climb;
      this.cancelled = this.target;
      this.target = null;
      // Stopped by the user, the climb's words are no longer true: they go at once.
      climb.cancel(this.cancelled);
      core.actions.stopClimb()
        .then((answer) => {
          if (signal.aborted || answer.ok) return;
          // The stop did not land: the climb goes on, and the readings say so again; a touch
          // before the next reading stops it afresh. The core has said why.
          if (this.target === null) this.target = this.cancelled;
          this.cancelled = null;
          climb.cancel(null);
          climb.paint(core.model.part('volume'));
        })
        .catch(() => {});
    }
    this.asked = null;
    this.travel = 0;
  }

  /** What the knob reads out: the asked step's words until the device's echo arrives. */
  show() {
    const asked = this.asked;
    if (asked === null) return this.readout(this.words);
    this.readout(this.texts[asked - this.lo] ?? this.words);
  }

  /** Put `text` in the big number and its unit. */
  readout(text) {
    const dB = text.endsWith(' dB');
    const [num, unit] = dB ? [text.slice(0, -3), 'dB'] : [text, ''];
    const numEl = this.querySelector('.big .num');
    if (numEl) {
      // The minus hangs left of the digits, so they stay centred whatever the sign.
      const signed = num.startsWith('−');
      const neg = document.createElement('span');
      neg.className = 'neg';
      neg.textContent = '−';
      numEl.replaceChildren(...(signed ? [neg, num.slice(1)] : [num]));
    }
    const unitEl = this.querySelector('.big .unit');
    if (unitEl) unitEl.textContent = unit;
  }

  /** Listen for the dial's gestures, the keys and the buttons. */
  wire() {
    const dial = this.svg;
    dial.addEventListener('pointerdown', (event) => {
      if (event.button === 0 && this.live()) this.drag(event);
    });
    dial.addEventListener('pointermove', (event) => {
      if (!this.classList.contains('drag')) this.hover(event);
    });
    dial.addEventListener('pointerleave', () => {
      if (!this.classList.contains('drag')) delete dial.dataset.zone;
    });
    dial.addEventListener('wheel', (event) => this.wheel(event), { passive: false });
    dial.addEventListener('keydown', (event) => this.key(event));
    const mute = this.querySelector('.mute');
    mute?.addEventListener('click', () => this.mute());
    // Holding Enter or Space on the focused button repeats its click: one press, one toggle.
    mute?.addEventListener('keydown', (event) => {
      if (event.repeat) event.preventDefault();
    });
    this.repeat();
  }

  /** The arrow keys, Page Up/Down and Home. */
  key(event) {
    if (!this.live() || event.ctrlKey || event.metaKey || event.altKey) return;
    const up = event.key === 'ArrowUp' || event.key === 'ArrowRight';
    const down = event.key === 'ArrowDown' || event.key === 'ArrowLeft';
    if (up || down) this.nudge(up ? 1 : -1);
    else if (event.key === 'PageUp' || event.key === 'PageDown') {
      this.nudge(event.key === 'PageUp' ? 8 : -8);
    } else if (event.key === 'Home') this.ask(this.lo);
    else return;
    event.preventDefault();
    event.stopPropagation();
  }

  /** The wheel: a notch is a step, across the silence detent as any travel. */
  wheel(event) {
    if (!this.live()) return;
    event.preventDefault();
    // Lines (a notch of a mouse wheel) count 40 pixels each, pages ten times that.
    const unit = [1, WHEEL_STEP, 10 * WHEEL_STEP][event.deltaMode] ?? 1;
    const pixels = event.deltaY * unit;
    const dir = pixels < 0 ? 1 : -1;
    if (Math.sign(this.travel) !== dir) this.travel = 0;
    if (Math.abs(pixels) >= WHEEL_STEP) {
      this.wheelSum = 0;
      this.move(dir);
      return;
    }
    this.wheelSum = (this.wheelSum ?? 0) + pixels;
    if (Math.abs(this.wheelSum) >= WHEEL_STEP) {
      this.wheelSum = 0;
      this.move(dir);
    }
  }

  /** One whole step up (`delta` 1) or down (-1), as the − and + buttons take it. */
  step(delta) {
    this.nudge(delta);
  }

  /** Mute, or unmute: what the Mute button does. */
  toggleMute() {
    return this.mute();
  }

  /** Mute writes mute on, which ends a climb in the writer; unmute writes it off. */
  async mute() {
    const key = this.dataset.mute;
    if (this.locked || !key) return;
    const stops = this.climbing() && !this.classList.contains('muted');
    const { core, signal } = ember;
    const answer = await core.actions.setControl(key,
      { value: !this.classList.contains('muted') });
    // A mute that landed stopped the climb: its words go at once.
    if (!signal.aborted && answer.ok && stops) core.frame.dismissToast('climb');
  }

  /** The − and + buttons: a press steps once, held it repeats after a pause. */
  repeat() {
    let first = 0;
    let every = 0;
    const stop = () => {
      clearTimeout(first);
      clearInterval(every);
    };
    // A hold ends when the page cannot take it: the stream lost, the tab hidden, the window
    // blurred, or the knob swapped out.
    this.stopHold = stop;
    this.blurs();
    for (const button of this.querySelectorAll('.step[data-step]')) {
      const press = () => {
        if (!this.isConnected || !this.live() || document.hidden
          || document.body.classList.contains('lost')) {
          return stop();
        }
        this.nudge(Number(button.dataset.step));
      };
      // A key press on a focused button clicks it with no pointer: one step.
      button.addEventListener('click', (event) => {
        if (event.detail === 0) press();
      });
      button.addEventListener('pointerdown', (event) => {
        if (event.button !== 0 || !this.live()) return;
        event.preventDefault();
        try {
          button.setPointerCapture(event.pointerId);
        } catch {
          // A pointer that is gone cannot be captured; the hold ends with its pointer events.
        }
        press();
        stop();
        first = setTimeout(() => {
          every = setInterval(press, HOLD_EVERY);
        }, HOLD_AFTER);
        // One set of end listeners a press: the first to fire ends the hold and all go.
        button.pressing?.abort();
        const done = new AbortController();
        button.pressing = done;
        const ended = () => {
          stop();
          done.abort();
        };
        for (const end of ['pointerup', 'pointerleave', 'pointercancel', 'lostpointercapture']) {
          button.addEventListener(end, ended, { signal: done.signal });
        }
      });
    }
  }

  /** The dial's centre on the screen and its track's radius, in pixels. */
  geometry() {
    const box = this.svg.getBoundingClientRect();
    // The dial's view box is -30 -16 260 220, centred on (100, 100), the track's radius 80.
    return {
      cx: box.left + (box.width * 130) / 260,
      cy: box.top + (box.height * 116) / 220,
      radius: (box.width * 80) / 260,
    };
  }

  /** Whether a press at (`x`, `y`) starts a vertical drag: it is in the dial's centre. */
  centre(x, y) {
    const { cx, cy, radius } = this.geometry();
    return (Math.hypot(x - cx, y - cy) || 1) < radius * 0.45;
  }

  /**
   * Whether a click at (`x`, `y`) may jump the volume: it is on the ring's own ground, not
   * beyond `RING_REACH` of the radius, and not on the readout's number, its unit or the sub
   * line (which sit over the dial and let the pointer through).
   */
  ringHit(x, y) {
    const { cx, cy, radius } = this.geometry();
    if ((Math.hypot(x - cx, y - cy) || 1) > radius * RING_REACH) return false;
    return !['.big .num', '.big .unit', '.sub'].some((sel) => {
      const el = this.querySelector(sel);
      if (!el) return false;
      const range = document.createRange();
      range.selectNodeContents(el);
      const r = range.getBoundingClientRect();
      return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    });
  }

  /**
   * The cursor over the dial says how it moves: up and down in the centre (`ns-resize`), round
   * on the ring (`grab`, `grabbing` while a drag turns it); nothing special when it takes no
   * input.
   */
  hover(event) {
    if (!this.live()) delete this.svg.dataset.zone;
    else if (this.centre(event.clientX, event.clientY)) this.svg.dataset.zone = 'centre';
    else if (this.ringHit(event.clientX, event.clientY)
      && this.stepAt(this.bearing(event.clientX, event.clientY)) !== null) {
      this.svg.dataset.zone = 'ring';
    } else delete this.svg.dataset.zone;
  }

  /** The angle of the point (`x`, `y`) from the dial's top, clockwise, in degrees. */
  bearing(x, y) {
    const { cx, cy } = this.geometry();
    return (Math.atan2(x - cx, -(y - cy)) * 180) / Math.PI;
  }

  /**
   * The raw step the dial's sweep puts at `deg` (the inverse of `angle`), or `null` in the gap at
   * the bottom, outside the sweep.
   */
  stepAt(deg) {
    if (Math.abs(deg) > SWEEP / 2) return null;
    return this.lo + Math.round(((deg + SWEEP / 2) / SWEEP) * Math.max(1, this.hi - this.lo));
  }

  /**
   * Turn the dial by a drag: round it, or up and down along it. Started near the centre it is
   * vertical; elsewhere the first movement decides, along the radius (vertical) or round it. A
   * press on the ring released without that movement is a click: it asks for the step under it,
   * unless it only stopped a climb, or it lies in the gap at the bottom.
   */
  drag(event) {
    event.preventDefault();
    this.svg.focus({ preventScroll: true });
    try {
      this.svg.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that is gone cannot be captured; the drag has nothing to follow.
    }
    const climbed = this.climbing();
    this.touch();
    this.classList.add('drag');
    const { cx, cy, radius } = this.geometry();
    const [x0, y0] = [event.clientX, event.clientY];
    const reach = Math.hypot(x0 - cx, y0 - cy) || 1;
    const angleOf = (e) => (Math.atan2(e.clientX - cx, -(e.clientY - cy)) * 180) / Math.PI;
    let mode = reach < radius * 0.45 ? 'vertical' : null;
    const say = () => { this.svg.dataset.zone = mode === 'vertical' ? 'centre' : 'turn'; };
    say();
    let lastY = y0;
    let lastAngle = angleOf(event);
    const span = this.hi - this.lo;
    const move = (e) => {
      // A button released where no pointerup reached the dial ends the drag.
      if (e.buttons === 0) return end();
      if (!mode) {
        const [dx, dy] = [e.clientX - x0, e.clientY - y0];
        if (Math.hypot(dx, dy) < 6) return;
        const [rx, ry] = [(x0 - cx) / reach, (y0 - cy) / reach];
        mode = Math.abs(dx * rx + dy * ry) > Math.abs(dx * ry - dy * rx) ? 'vertical' : 'round';
        say();
      }
      if (mode === 'vertical') this.move((-(e.clientY - lastY) * span) / (radius * 3));
      else {
        let turn = angleOf(e) - lastAngle;
        if (turn > 180) turn -= 360;
        if (turn < -180) turn += 360;
        this.move((turn * span) / SWEEP);
      }
      lastY = e.clientY;
      lastAngle = angleOf(e);
    };
    const release = () => {
      const jumps = !mode && !climbed && this.ringHit(x0, y0);
      const step = jumps ? this.stepAt(angleOf(event)) : null;
      if (step !== null) this.ask(step);
      end();
    };
    const end = () => {
      this.classList.remove('drag');
      delete this.svg.dataset.zone;
      this.svg.removeEventListener('pointermove', move);
      this.svg.removeEventListener('pointerup', release);
      this.svg.removeEventListener('pointercancel', end);
      this.svg.removeEventListener('lostpointercapture', end);
    };
    this.svg.addEventListener('pointermove', move);
    this.svg.addEventListener('pointerup', release);
    this.svg.addEventListener('pointercancel', end);
    this.svg.addEventListener('lostpointercapture', end);
  }

  /** The angle of raw step `raw`, in degrees. */
  angle(raw) {
    return dialAngle(raw, this.lo, this.hi);
  }

  /** The dB of step `raw`, or `null` for silence, a step out of range or a dial without dB. */
  dbOf(raw) {
    const db = this.dbs[raw - this.lo];
    return Number.isFinite(db) ? db : null;
  }

  /** The lowest finite dB of the dial (silence counts as that step), or `null` without dB. */
  get lowDb() {
    const finite = this.dbs.filter(Number.isFinite);
    return finite.length > 0 ? Math.min(...finite) : null;
  }

  /** The volume's place in its range, 0 to 1. */
  fraction(raw) {
    return fraction(raw, this.lo, this.hi);
  }

  /** The class of the tick at step `raw`, by the dial's rule. */
  tickClass(raw, level) {
    return classOf(raw, level, this);
  }

  /** Repaint from a `volume` event. */
  update(data) {
    this.raw = data.raw;
    // A target the user's touch ended stays ended until a reading drops it (the stop landed).
    if (data.target !== this.cancelled) this.cancelled = null;
    this.target = this.cancelled === null ? data.target : null;
    this.risk = data.risk;
    this.cap = data.cap;
    this.words = data.text;
    // The asked step stands until a reading reflects it, as the level or as the climb's target
    // (the number then shows the live level), or, if none does, a while after it was asked.
    if (this.asked !== null && (data.raw === this.asked || this.target === this.asked
      || Date.now() - this.askedAt > ASKED_FOR)) {
      this.asked = null;
    }
    this.classList.toggle('muted', data.muted);
    const level = data.raw === null || (this.silence && data.raw === this.lo) ? null : data.raw;
    for (const label of this.svg.querySelectorAll('.ltick, .lbl')) {
      const red = this.risk !== null && Number(label.dataset.at) >= this.risk;
      label.classList.toggle('red', red);
    }
    const zone = this.risk !== null ? arc(this.angle(this.risk), SWEEP / 2, 80) : null;
    this.path('zone', zone);
    this.glide(level);
    const climbing = this.climbing();
    this.querySelector('.sub').textContent = climbing
      ? `to ${data.target_text}`
      : data.muted ? 'muted' : '';
    this.svg.setAttribute('aria-valuetext', data.aria);
    if (data.raw !== null) this.svg.setAttribute('aria-valuenow', String(data.raw));
    else this.svg.removeAttribute('aria-valuenow');
    if (this.locked || data.unread) this.svg.setAttribute('aria-disabled', 'true');
    else this.svg.removeAttribute('aria-disabled');
    this.querySelector('.mute')?.setAttribute('aria-pressed', String(data.muted));
    const lines = (list) => list.map((note) => {
      const p = document.createElement('p');
      p.className = note.red ? 'note risk hot' : 'note';
      p.textContent = note.text;
      return p;
    });
    this.querySelector('.notes .slot').replaceChildren(...lines(data.notes.filter((n) => n.eq)));
    this.querySelector('.notes .rest').replaceChildren(
      ...lines(data.notes.filter((n) => !n.eq && !n.popup)),
    );
  }

  /**
   * Bring what shows the level to `level` (null: none): at once, or, when the level moves from
   * one reading to another, over [`EASE_MS`] in a straight line, so a climb's steps run into
   * each other. Every end of a move is a level the device reported.
   */
  glide(level) {
    cancelAnimationFrame(this.easing);
    this.easing = 0;
    const from = this.shown ?? null;
    if (from === null || level === null || from === level || document.hidden) {
      this.shown = level;
      this.paint(level);
      return;
    }
    const began = performance.now();
    const step = (now) => {
      const done = Math.min(1, (now - began) / EASE_MS);
      this.shown = done < 1 ? from + (level - from) * done : level;
      this.paint(this.shown);
      this.easing = done < 1 ? requestAnimationFrame(step) : 0;
    };
    this.easing = requestAnimationFrame(step);
  }

  /** Draw the level `shown` (null: silence or unread): ticks, value arc, climb and handle. */
  paint(shown) {
    // Whatever draws the dial keeps the level it drew, so the next move starts from it.
    this.shown = shown;
    const at = shown === null ? null : Math.round(shown);
    for (const tick of this.svg.querySelectorAll('line.tick')) {
      tick.setAttribute('class', `tick ${this.tickClass(Number(tick.dataset.raw), at)}`);
    }
    const lit = shown !== null && shown > this.lo;
    this.path('value', lit ? arc(-SWEEP / 2, this.angle(shown), 80) : null);
    const climbing = this.climbing();
    const raw = shown ?? this.raw;
    const climb = climbing ? arc(this.angle(raw), this.angle(this.target), 80) : null;
    this.path('target', climb);
    this.spoke('ttick', climbing ? this.angle(this.target) : null, 73, 87);
    this.handle(this.raw === null ? null : raw);
    // The readout names the level the reading reports, never one passed through on the way.
    this.show();
  }

  /** Set path `name`'s `d`, making or removing it. */
  path(name, d) {
    let el = this.svg.querySelector(`path.${name}`);
    if (!d) return el?.remove();
    if (!el) {
      el = document.createElementNS(SVG, 'path');
      el.setAttribute('class', name);
      this.svg.querySelector('path.track').after(el);
    }
    el.setAttribute('d', d);
  }

  /** Set the radial line `name` at `deg` from `r1` to `r2`, or remove it. */
  spoke(name, deg, r1, r2) {
    let el = this.svg.querySelector(`line.${name}`);
    if (deg === null) return el?.remove();
    if (!el) {
      el = document.createElementNS(SVG, 'line');
      el.setAttribute('class', name);
      this.svg.append(el);
    }
    const [x1, y1, x2, y2] = spokeAt(deg, r1, r2);
    for (const [k, v] of [['x1', x1], ['y1', y1], ['x2', x2], ['y2', y2]]) {
      el.setAttribute(k, v);
    }
  }

  /** Put the handle at `raw`, or take it away. */
  handle(raw) {
    let el = this.svg.querySelector('circle.handle');
    if (raw === null) return el?.remove();
    if (!el) {
      el = document.createElementNS(SVG, 'circle');
      el.setAttribute('class', 'handle');
      el.setAttribute('r', '6.5');
      this.svg.append(el);
    }
    const [x, y] = point(this.angle(raw), 80);
    el.setAttribute('cx', x);
    el.setAttribute('cy', y);
  }
}

customElements.define('sv-knob', Knob);
