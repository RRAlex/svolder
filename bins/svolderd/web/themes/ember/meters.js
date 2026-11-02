/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// An amp's meters: one arch of 32 segments per meter. The theme draws the arches; this lights
// the segments up to each reading, keeps a peak segment in --ink for 1.1 s and lets it fall
// 0.03 every 100 ms, and writes each percentage and the state line.

const HOLD_MS = 1100;
const FALL = 0.03;
const FALL_EVERY_MS = 100;

class Meters extends HTMLElement {
  connectedCallback() {
    // Kept across swaps, and connected again each time: the arches stand, the timer restarts.
    if (this.arches) {
      this.fall = setInterval(() => this.fallStep(), FALL_EVERY_MS);
      return;
    }
    this.arches = new Map();
    for (const figure of this.querySelectorAll('figure.meter')) {
      this.arches.set(figure.dataset.key, {
        figure,
        segs: [...figure.querySelectorAll('line.seg')],
        pct: figure.querySelector('.m-pct'),
        fraction: null,
        peak: 0,
        heldAt: 0,
        lit: -1,
        marked: -1,
      });
    }
    this.fall = setInterval(() => this.fallStep(), FALL_EVERY_MS);
  }

  disconnectedCallback() {
    clearInterval(this.fall);
  }

  /** Repaint from a `meters` event: {live, state, meters: [{key, fraction, text}]}. */
  update(data) {
    this.dataset.live = String(data.live);
    const state = this.closest('section')?.querySelector('.mstate');
    if (state) state.textContent = data.state ?? '';
    const now = performance.now();
    for (const meter of data.meters) {
      const arch = this.arches.get(meter.key);
      if (!arch) continue;
      arch.fraction = meter.fraction;
      if (meter.fraction === null) {
        arch.peak = 0;
      } else if (meter.fraction >= arch.peak) {
        arch.peak = meter.fraction;
        arch.heldAt = now;
      }
      this.percent(arch, meter.text, meter.fraction !== null);
      this.paint(arch);
    }
  }

  /** The reading's words, with the % sign after a number. */
  percent(arch, text, number) {
    arch.pct.textContent = text;
    const caption = arch.pct.parentElement;
    for (const node of [...caption.childNodes]) {
      if (node.nodeType === Node.TEXT_NODE) node.remove();
    }
    if (number) arch.pct.after(document.createTextNode('%'));
  }

  /** Light the segments up to the reading and mark the peak segment above them. */
  paint(arch) {
    const n = arch.segs.length;
    const lit = arch.fraction === null ? 0 : Math.round(arch.fraction * n);
    const peak = arch.fraction === null ? -1 : Math.max(1, Math.round(arch.peak * n)) - 1;
    const marked = peak >= lit ? peak : -1;
    if (lit !== arch.lit) {
      arch.segs.forEach((seg, i) => seg.classList.toggle('on', i < lit));
      arch.lit = lit;
    }
    if (marked !== arch.marked) {
      arch.segs[arch.marked]?.classList.remove('peak');
      arch.segs[marked]?.classList.add('peak');
      arch.marked = marked;
    }
  }

  /** Let each held peak fall, gradually, once its hold is over. */
  fallStep() {
    const now = performance.now();
    for (const arch of this.arches.values()) {
      if (arch.fraction === null || arch.peak <= arch.fraction) continue;
      if (now - arch.heldAt < HOLD_MS) continue;
      arch.peak = Math.max(arch.fraction, arch.peak - FALL);
      this.paint(arch);
    }
  }

  /** The meters' readings in figure order (0 to 1, `null` unread). */
  readings() {
    return [...this.arches.values()].map((arch) => arch.fraction);
  }

  /** The larger of the meters' readings, 0 to 1, or null without one. */
  louder() {
    let best = null;
    for (const arch of this.arches.values()) {
      if (arch.fraction !== null && (best === null || arch.fraction > best)) best = arch.fraction;
    }
    return best;
  }
}

customElements.define('sv-meters', Meters);
