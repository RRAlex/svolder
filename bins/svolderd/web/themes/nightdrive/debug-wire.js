/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The readout box: a small fixed box under the device line, drawn twice a second, made only when the
// address asks for it (debug.js). Its words are the scene's own numbers, for someone with a phone and no console.

/** The longest the long-task count looks back, ms. */
const LONG_MS = 10000;

/** How often the box is drawn again, ms. */
export const EVERY_MS = 500;

/** The box's lines from `read`: `{ perf, sky, view, canvas, dpr, longTasks, off }`. */
export function readoutLines(read) {
  const { perf, sky, view, canvas, dpr, longTasks, off } = read;
  const levels = sky?.levels;
  const lv = levels
    ? `bass ${levels.bass.toFixed(2)} highs ${levels.highs.toFixed(2)} jump ${levels.highsJump.toFixed(2)}`
    : 'no analysis';
  const ear = levels?.ear;
  const hear = ear
    ? `ear med ${ear.med.toFixed(2)} long ${ear.long.toFixed(2)} balance ${ear.balance.toFixed(2)} `
      + `embers ${ear.embers.toFixed(2)}`
    : 'no ear';
  return [
    `${perf.fps.toFixed(0)} draws/s  asked ${perf.pace ? `${perf.pace.toFixed(0)} ms` : 'every frame'}`,
    `draw ${perf.drawMs.toFixed(1)} ms mean  ${perf.p95.toFixed(1)} p95`,
    `canvas ${canvas[0]}x${canvas[1]}  dpr ${dpr}`,
    `scale ${perf.scale.toFixed(2)}  capped ${perf.capped ? 'yes' : 'no'}  step ${perf.step}  `
      + `wet ${perf.wet ? 'ok' : 'dropped'}`,
    `motion ${view.motion}`,
    `long tasks 10 s: ${longTasks === null ? 'n/a' : longTasks}`,
    lv,
    hear,
    off.length ? `off: ${off.join(' ')}` : 'off: nothing',
  ];
}

/** Make the box in `layer`; returns `{ stop() }`. `scene` is scene-wire.js's handle, `view` the page's,
 * `canvas` the road's canvas, `off` the switches in force, `clock()` the page's ms. */
export function wireDebug({
  layer, scene, view, canvas, off, clock = () => globalThis.performance.now(),
}) {
  const box = layer.ownerDocument.createElement('div');
  box.className = 'nd-debug';
  box.setAttribute('aria-hidden', 'true');
  layer.append(box);
  const longs = [];
  let observer = null;
  try {
    if (globalThis.PerformanceObserver?.supportedEntryTypes?.includes('longtask')) {
      observer = new globalThis.PerformanceObserver((list) => {
        for (const entry of list.getEntries()) longs.push(entry.startTime);
      });
      observer.observe({ type: 'longtask' });
    }
  } catch {
    observer = null;
  }

  function draw() {
    const now = clock();
    while (longs.length && longs[0] < now - LONG_MS) longs.shift();
    box.textContent = readoutLines({
      perf: scene.perf(),
      sky: scene.sky(),
      view,
      canvas: [canvas.width, canvas.height],
      dpr: `${(globalThis.devicePixelRatio ?? 1).toFixed(2)} -> ${scene.perf().scale.toFixed(2)}`,
      longTasks: observer ? longs.length : null,
      off,
    }).join('\n');
  }

  draw();
  const timer = globalThis.setInterval(draw, EVERY_MS);
  return {
    stop() {
      globalThis.clearInterval(timer);
      observer?.disconnect();
      box.remove();
    },
  };
}
