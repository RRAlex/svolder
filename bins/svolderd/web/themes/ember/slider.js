/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A level other than the volume (a balance, a gain level, loudness strength): the theme draws
// the track, the fill from the centre (or the start) to the thumb and the value; this moves the
// thumb and fill and sets the words as readings arrive. The thumb's place is also the custom
// property --at on the element, set through CSSOM, which the page's CSP allows.

import { ember, shown } from './bind.js';
import { createLevelWriter } from './level-write.js';

const SVG = 'http://www.w3.org/2000/svg';

class Slider extends HTMLElement {
  connectedCallback() {
    // Kept across swaps, and connected again each time: what it has been sent stands.
    if (this.ready) return;
    this.ready = true;
    const d = this.dataset;
    this.lo = Number(d.lo);
    this.hi = Number(d.hi);
    this.centre = d.centre === '' ? null : Number(d.centre);
    this.raw = d.raw === '' ? null : Number(d.raw);
    /** The last reading the device gave. */
    this.known = this.raw;
    this.svg = this.querySelector('svg.bslider');
    this.place(this.raw);
    // One write in flight, the newest waiting, each for the device shown when it was asked; what
    // the device answers is shown until the stream says otherwise. The core says why a refusal
    // was one.
    this.send = createLevelWriter({
      key: this.dataset.key,
      actions: ember.core.actions,
      shown,
      connected: () => this.isConnected && !ember.signal?.aborted,
      landed: (answer) => this.update({ raw: answer.raw, text: answer.text }),
      refused: () => {
        // What was shown but not written: back to the last reading the device gave.
        this.raw = this.known;
        this.place(this.known);
      },
    });
    this.wire();
  }

  /** Whether the slider takes input: not drawn inert, and read. */
  live() {
    return this.svg.getAttribute('aria-disabled') !== 'true' && this.raw !== null;
  }

  /** Show `raw` at once and write it. */
  ask(raw) {
    const step = Math.min(this.hi, Math.max(this.lo, Math.round(raw)));
    if (step === this.raw) return;
    this.raw = step;
    this.place(step);
    this.send(step);
  }

  /** Listen for a drag on the track, the arrow keys and a tap on the value. */
  wire() {
    this.svg.addEventListener('pointerdown', (event) => {
      if (event.button !== 0 || !this.live()) return;
      event.preventDefault();
      this.svg.focus({ preventScroll: true });
      try {
        this.svg.setPointerCapture(event.pointerId);
      } catch {
        // A pointer that is gone cannot be captured.
      }
      const at = (e) => {
        const box = this.svg.getBoundingClientRect();
        const along = Math.min(1, Math.max(0, (e.clientX - box.left) / box.width));
        this.ask(this.lo + along * (this.hi - this.lo));
      };
      this.dragging = true;
      const end = () => {
        this.dragging = false;
        // A reading that arrived while the finger was down and says what the thumb shows.
        if (this.last && this.last.raw === this.raw) this.words(this.last);
        this.svg.removeEventListener('pointermove', move);
        this.svg.removeEventListener('pointerup', end);
        this.svg.removeEventListener('pointercancel', end);
        this.svg.removeEventListener('lostpointercapture', end);
      };
      // A drag with no button down, or on a slider that left the page, writes nothing.
      const move = (e) => {
        if (e.buttons === 0 || !this.isConnected) return end();
        at(e);
      };
      at(event);
      this.svg.addEventListener('pointermove', move);
      this.svg.addEventListener('pointerup', end);
      this.svg.addEventListener('pointercancel', end);
      this.svg.addEventListener('lostpointercapture', end);
    });
    this.svg.addEventListener('keydown', (event) => {
      if (!this.live() || this.raw === null) return;
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const up = event.key === 'ArrowUp' || event.key === 'ArrowRight';
      const down = event.key === 'ArrowDown' || event.key === 'ArrowLeft';
      if (up || down) this.ask(this.raw + (up ? 1 : -1));
      else if (event.key === 'Home' && this.centre !== null) this.ask(this.centre);
      else return;
      event.preventDefault();
    });
    this.querySelector('.val')?.addEventListener('click', () => {
      if (this.live() && this.centre !== null) this.ask(this.centre);
    });
  }

  /** The place of raw step `raw` along the track, as a percentage. */
  percent(raw) {
    return (100 * (raw - this.lo)) / Math.max(1, this.hi - this.lo);
  }

  /** Move the thumb and the fill to `raw`, or take them away before a reading. */
  place(raw) {
    const svg = this.svg;
    if (!svg) return;
    let fill = svg.querySelector('.b-fill');
    let thumb = svg.querySelector('.b-thumb');
    if (raw === null) {
      fill?.remove();
      thumb?.remove();
      this.style.removeProperty('--at');
      return;
    }
    const at = `${this.percent(raw).toFixed(2)}%`;
    const from = this.centre === null ? '0%' : `${this.percent(this.centre).toFixed(2)}%`;
    if (!fill) {
      fill = document.createElementNS(SVG, 'line');
      fill.setAttribute('class', 'b-fill');
      fill.setAttribute('y1', '22');
      fill.setAttribute('y2', '22');
      svg.querySelector('.b-track').after(fill);
    }
    if (!thumb) {
      thumb = document.createElementNS(SVG, 'circle');
      thumb.setAttribute('class', 'b-thumb');
      thumb.setAttribute('cy', '22');
      thumb.setAttribute('r', '10');
      svg.append(thumb);
    }
    fill.setAttribute('x1', from);
    fill.setAttribute('x2', at);
    thumb.setAttribute('cx', at);
    this.style.setProperty('--at', at);
  }

  /** Repaint from a `level` event: {key, raw, text}. */
  update(data) {
    // The finger is the truth while it is down; the stream catches up after.
    this.known = data.raw;
    this.last = data;
    if (this.dragging) return;
    this.raw = data.raw;
    this.place(data.raw);
    this.words(data);
  }

  /** Show a reading's number and its words for a screen reader. */
  words(data) {
    const val = this.querySelector('.val');
    if (val) val.textContent = data.text;
    if (data.raw === null) this.svg.removeAttribute('aria-valuenow');
    else this.svg.setAttribute('aria-valuenow', String(data.raw));
    this.svg.setAttribute('aria-valuetext', data.text);
  }
}

customElements.define('sv-slider', Slider);
