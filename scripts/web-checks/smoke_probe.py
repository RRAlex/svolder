# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The in-page probe the smoke checks share (numbers only: readPixels, counters; never a picture of
Smoke): smoke_core.py and smoke_page.py install it with `PROBE`."""

# The probe: reads the smoke's canvas in an animation frame registered after the smoke's own
# (same frame, after its draw), over `frames` frames. `best` is the frame with the most alpha,
# `lastNonEmpty` the latest frame that drew anything. Measures are in CSS pixels, y from the top.
PROBE = """
window.__read = (sel, frames) => {
  window.__rd = null;
  const c = document.querySelector(sel); const gl = c.getContext('webgl2');
  let n = 0, best = null, lastNonEmpty = null, err = 0;
  const tick = () => {
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    const w = gl.drawingBufferWidth, h = gl.drawingBufferHeight;
    const px = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    err |= gl.getError();
    let sum = 0; for (let i = 3; i < px.length; i += 4) sum += px[i];
    const f = { w, h, px, sum, k: w / c.clientWidth };
    if (sum > 0) lastNonEmpty = f;
    if (!best || sum > best.sum) best = f;
    if (++n < frames) requestAnimationFrame(tick);
    else window.__rd = { best, lastNonEmpty, err };
  };
  requestAnimationFrame(tick);
};
window.__frame = (which) => (window.__rd && window.__rd[which || 'best']) || null;
window.__meas = (x0, y0, x1, y1, which) => {
  const f = window.__frame(which); if (!f) return null;
  let max = 0, sum = 0, n = 0;
  const X0 = Math.max(0, Math.round(x0 * f.k)), X1 = Math.min(f.w, Math.round(x1 * f.k));
  const Y0 = Math.max(0, Math.round(y0 * f.k)), Y1 = Math.min(f.h, Math.round(y1 * f.k));
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
    const a = f.px[((f.h - 1 - y) * f.w + x) * 4 + 3];
    if (a > max) max = a; sum += a; n++;
  }
  return { max, mean: n ? sum / n : 0, n };
};
window.__centroid = (which) => {
  const f = window.__frame(which); if (!f) return null;
  let sw = 0, sx = 0, sy = 0;
  for (let y = 0; y < f.h; y++) for (let x = 0; x < f.w; x++) {
    const a = f.px[((f.h - 1 - y) * f.w + x) * 4 + 3]; sw += a; sx += a * x; sy += a * y;
  }
  return sw ? [sx / sw / f.k, sy / sw / f.k, sw] : null;
};
window.__peak = (which) => {
  const f = window.__frame(which); if (!f) return null;
  let max = 0; for (let i = 3; i < f.px.length; i += 4) if (f.px[i] > max) max = f.px[i];
  return max;
};
window.__grey = (which) => {
  const f = window.__frame(which); if (!f) return null;
  let worst = 0, seen = 0, spread = 0, alpha = 0;
  for (let i = 0; i < f.px.length; i += 4) {
    const a = f.px[i + 3]; if (a < 8) continue; seen++;
    const d = Math.max(f.px[i], f.px[i + 1], f.px[i + 2]) - Math.min(f.px[i], f.px[i + 1],
      f.px[i + 2]);
    spread += d; alpha += a;
    if (d / a > worst) worst = d / a;
  }
  return { worst, seen, mean: alpha ? spread / alpha : 0 };
};
window.__edge = (e, y0, y1, which) => {
  const f = window.__frame(which); if (!f) return null;
  const E = Math.round(e * f.k), A = (x, y) => f.px[((f.h - 1 - y) * f.w + x) * 4 + 3];
  let edge = 0, beside = 0, atMax = 0;
  for (let y = Math.max(0, Math.round(y0 * f.k)); y < Math.min(f.h, Math.round(y1 * f.k)); y++) {
    edge = Math.max(edge, Math.abs(A(E + 2, y) - A(E - 2, y)));
    atMax = Math.max(atMax, A(E - 2, y), A(E + 2, y));
    for (let x = E + 6; x <= E + 14; x++) beside = Math.max(beside, Math.abs(A(x + 2, y) - A(x - 2,
      y)));
    for (let x = E - 14; x <= E - 6; x++) beside = Math.max(beside, Math.abs(A(x + 2, y) - A(x - 2,
      y)));
  }
  return { edge, beside, atMax };
};
// the module, for tests that need a handle of their own
window.__mod = null; window.__model = null;
Promise.all([import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href),
  import(new URL('assets/themes/ember/smoke.js', document.baseURI).href)]).then(([model, m]) => {
  window.__model = model; window.__mod = m; });
window.__mk = (anchor, level) => {
  const c = document.createElement('canvas'); c.id = 'tc';
  for (const [k, v] of [['position', 'fixed'], ['left', '0'], ['top', '0'], ['width', '100%'],
    ['height', '100lvh'], ['pointer-events', 'none'], ['z-index', '-1']]) c.style.setProperty(k, v);
  document.body.append(c);
  const seed = window.__model.seeded(7);
  window.__shade = window.__model.drawShade(() => 0.9); // among the most colourful shades drawn
  window.__h = window.__mod.start(c, { shade: window.__shade, random: seed });
  if (!window.__h) return false;
  window.__h.haze(false); // the cloud alone; haze.py checks the haze on the real page
  window.__chroma = Math.max(...window.__shade.light) - Math.min(...window.__shade.light);
  window.__h.level(level); window.__h.anchor(anchor); return true;
};
window.__hold = false; window.__held = [];
const raf0 = window.requestAnimationFrame.bind(window);
window.requestAnimationFrame = (cb) => raf0((t) => { if (window.__hold) window.__held.push(cb);
  else cb(t); });
window.__release = () => { window.__hold = false;
  for (const cb of window.__held.splice(0)) raf0(cb); };
window.__counts = { draws: 0 };
const dA = WebGL2RenderingContext.prototype.drawArrays;
WebGL2RenderingContext.prototype.drawArrays = function (...a) { window.__counts.draws++;
  return dA.apply(this, a); };
window.__live = [];
const gc = HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext = function (...a) {
  const ctx = gc.apply(this, a);
  if (ctx && a[0] === 'webgl2' && !window.__live.includes(ctx)) window.__live.push(ctx);
  return ctx;
};
window.__toasts = [];
new MutationObserver((list) => { for (const m of list) for (const n of m.addedNodes)
  if (n.classList && n.classList.contains('toast')) window.__toasts.push(n.textContent); })
  .observe(document.body, { childList: true, subtree: true });
window.__liveCount = () => window.__live.filter((c) => !c.isContextLost()).length;
"""
