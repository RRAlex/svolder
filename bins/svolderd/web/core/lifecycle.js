/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The loop of a theme's canvas: it draws one frame after another while the canvas is on show and
// stops while it is not (a hidden tab, scrolled away), follows the canvas's size and the screen's
// pixel density, and, with the system set for less motion, draws one still frame (again at each
// resize) and runs no loop. `draw(dt, now)` gets seconds, `dt` held to a tenth of one so a pause
// does not arrive as a jump; `resize(width, height, dpr)` gets CSS pixels and the density.

/** The longest step a frame is told of, in seconds. */
const MAX_DT = 0.1;

/**
 * The loop of `canvas`. `watchVisible` is visible.js's, `matchMedia` the window's. Returns
 * `{ start(), stop(), running, reduced }`: `running` is true while frames are being asked for,
 * `reduced` whether the system asks for less motion.
 */
export function canvasLoop({ canvas, draw, resize, watchVisible, matchMedia }) {
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  let watch = null;
  let observer = null;
  let started = false;
  let shown = false;
  let frame = 0;
  let last = null;
  let size = null;

  /** Tell `resize` the canvas's size when it changed. */
  function fit() {
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    const dpr = globalThis.devicePixelRatio ?? 1;
    if (size && size.width === width && size.height === height && size.dpr === dpr) return false;
    size = { width, height, dpr };
    resize(width, height, dpr);
    return true;
  }

  const animated = () => started && shown && !motion.matches;

  function tick(ms) {
    frame = 0;
    if (!animated()) return;
    const now = ms / 1000;
    const dt = last === null ? 0 : Math.min(MAX_DT, Math.max(0, now - last));
    last = now;
    fit();
    draw(dt, now);
    frame = requestAnimationFrame(tick);
  }

  /** Run the frames when they should run, stop them when not; a still frame for reduced motion. */
  function sync() {
    if (animated()) {
      if (!frame) {
        last = null;
        frame = requestAnimationFrame(tick);
      }
      return;
    }
    if (frame) {
      cancelAnimationFrame(frame);
      frame = 0;
    }
    if (started && shown && motion.matches) {
      fit();
      draw(0, (globalThis.performance?.now?.() ?? 0) / 1000);
    }
  }

  const onMotion = () => { sync(); };

  return {
    start() {
      if (started) return;
      started = true;
      size = null;
      motion.addEventListener?.('change', onMotion);
      if (typeof globalThis.ResizeObserver === 'function') {
        observer = new globalThis.ResizeObserver(() => {
          if (fit() && motion.matches && started && shown) {
            draw(0, (globalThis.performance?.now?.() ?? 0) / 1000);
          }
        });
        observer.observe(canvas);
      }
      watch = watchVisible({
        element: canvas,
        onVisible(on) {
          shown = on;
          sync();
        },
      });
    },

    stop() {
      started = false;
      shown = false;
      watch?.stop();
      watch = null;
      observer?.disconnect();
      observer = null;
      motion.removeEventListener?.('change', onMotion);
      sync();
      frame = 0;
    },

    get running() { return frame !== 0; },
    get reduced() { return motion.matches; },
  };
}
