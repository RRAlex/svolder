/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The renderer and its tiers: the GPU canvas, the capability bag the layers ask. One renderer,
// `WebGPURenderer`: with a WebGPU adapter it runs the WebGPU tier, without one (or when the
// WebGL2 floor is asked for) its WebGL2 backend, the same scene on both. The layers ask the bag
// (`{ tier, compute, neighbours, timestamps, timer, maxParticles, reason, vendor }`), never the
// browser.
// The three files
// come in with a dynamic `import()` at mount, so the other themes load none of them; a canvas
// with neither tier throws at `createGpu`, and the wire shows its still.
//
//   loadThree()                 the two vendored namespaces `{ T, S }`
//   createGpu({ canvas, forceWebGL, why, load, firefox })  -> gpu (`why`: the WebGL2 floor's
//                               reason, when it was not the viewer's choice: 'faster'; `firefox`:
//                               the WebGPU tier is kept polled, from the UA by default):
//                               `{ T, S, tier, caps, renderer, size(w, h),
//                               render(scene, camera), compile(scene, camera), target(w, h),
//                               read(scene, camera, target),
//                               gpuMs(), timer, dispose() }`
//   keepPolled(renderer)        Firefox's WebGPU device polled (empty submits) while it draws, until
//                               the device is lost: its frames otherwise wait for its 100 ms poll
//   disposeAfterFrames(gpu, win)  let `gpu` go three display frames from now (polling kept meanwhile)
//   createQueryTimer(gl)        the WebGL2 tier's GPU time: `{ begin(), end(), read() }` or null
//   tierReason({ forced, hasApi, secure })   why the WebGL2 tier: 'chosen', 'not a secure context',
//                               'no adapter'
//   tierWords(caps)             the readout: `WebGPU · amd`, `WebGL2 · no adapter`

/** The most sprite instances a layer may ask for on the WebGL2 tier (a full-size scene drew 40k
 * additive sprites cheaply on every cell when measured); the WebGPU tier's stateful counts are
 * four times that. */
export const MAX_PARTICLES = 40000;
export const WEBGPU_PARTICLE_FACTOR = 4;

/** Why the scene is on the WebGL2 tier: the floor was asked for, the browser gives no WebGPU API
 * on an address that is not a secure context (a LAN http address), or it gave no adapter. */
export function tierReason({ forced = false, hasApi = true, secure = true } = {}) {
  if (forced) return 'chosen';
  if (!hasApi && !secure) return 'not a secure context';
  return 'no adapter';
}

/** The tier as the debug readout and the picker card say it: `WebGPU · amd` (the adapter's vendor,
 * when it names one) or `WebGL2 · no adapter`. */
export function tierWords(caps) {
  if (!caps) return null;
  if (caps.tier === 'webgpu') return caps.vendor ? `WebGPU · ${caps.vendor}` : 'WebGPU';
  return `WebGL2 · ${caps.reason ?? 'no adapter'}`;
}

/** The vendored three build and its TSL, by relative path (the page's CSP serves only 'self'). */
export async function loadThree() {
  const [T, S] = await Promise.all([
    import('./vendor/three.webgpu.js'),
    import('./vendor/three.tsl.js'),
  ]);
  return { T, S };
}

/** The browser's WebGPU adapter, or null (then the renderer takes its WebGL2 backend, which three
 * says in a console warning when it has to fall back by itself); whether the API is there at all
 * and whether the page is a secure context, for the reason. */
async function probeAdapter() {
  const api = globalThis.navigator?.gpu;
  const secure = globalThis.isSecureContext !== false;
  try {
    return { adapter: (await api?.requestAdapter?.()) ?? null, hasApi: Boolean(api), secure };
  } catch {
    return { adapter: null, hasApi: Boolean(api), secure };
  }
}

/**
 * Avoid on WebGL2: a draw issued right after a compute pass without a flush between them. Firefox on
 * Mesa GL drops that draw: a render target reads back as zeros, the sky of a frame goes missing.
 * Probably the compute's transform feedback commands are not yet submitted; a flush, a finish or a
 * readPixels between the two each cures it. Instead flush the GL after every compute pass; this
 * wraps the renderer's two compute calls so no caller can forget it. The WebGPU tier has no GL
 * context and is left alone.
 */
export function flushAfterCompute(renderer) {
  const flush = () => renderer.backend?.gl?.flush?.();
  const compute = renderer.compute.bind(renderer);
  renderer.compute = (...args) => {
    const done = compute(...args);
    flush();
    return done;
  };
  if (typeof renderer.computeAsync === 'function') {
    const computeAsync = renderer.computeAsync.bind(renderer);
    renderer.computeAsync = async (...args) => {
      const done = await computeAsync(...args);
      flush();
      return done;
    };
  }
}

/** How often the WebGPU tier hands its queue an empty submit while the scene draws, ms, and for
 * how long after the last draw it goes on doing so. */
export const POLL = Object.freeze({ everyMs: 4, idleMs: 250 });

/**
 * Keep the WebGPU device polled while the scene draws. Firefox learns that a frame's GPU work is
 * done (the canvas's copy for the compositor, a buffer's map) only when it polls the device: at a
 * submit, or on its own 100 ms timer. Once a frame's GPU work outlasts a display frame, no submit
 * comes before the next frame (that waits for the canvas), so every frame waits for the timer:
 * 20 a second whatever the GPU time (the 3.11 baseline: a two-pass page at 19.6 ms of GPU draws
 * 20 a second, 51 with this). An empty submit every `everyMs` while draws come is that poll;
 * `idleMs` after the last render or compute it stops, and for good once the device is lost (a
 * submit to a lost device throws nothing). `timers` are the checks' (`now`, `setInterval`,
 * `clearInterval`). Returns `stop()`.
 */
export function keepPolled(renderer, { everyMs = POLL.everyMs, idleMs = POLL.idleMs, timers = globalThis } = {}) {
  const now = () => timers.performance?.now?.() ?? timers.now();
  let lastAt = -Infinity;
  let ticking = null;
  let lost = false;
  const stop = () => {
    if (ticking !== null) timers.clearInterval(ticking);
    ticking = null;
  };
  Promise.resolve(renderer.backend?.device?.lost).then((info) => {
    if (info === undefined) return;
    lost = true;
    stop();
  }, () => {});
  const tick = () => {
    if (now() - lastAt > idleMs) {
      stop();
      return;
    }
    try {
      renderer.backend?.device?.queue?.submit([]);
    } catch {
      stop();
    }
  };
  const mark = () => {
    lastAt = now();
    if (ticking === null && !lost) ticking = timers.setInterval(tick, everyMs);
  };
  for (const name of ['render', 'compute']) {
    const call = renderer[name]?.bind(renderer);
    if (!call) continue;
    renderer[name] = (...args) => {
      mark();
      return call(...args);
    };
  }
  return stop;
}

/**
 * Let `gpu` go `frames` display frames from now, not at once: a WebGPU device destroyed right after
 * a draw, with its canvas removed, left Firefox with no animation frame for the page ever again
 * (3.11: leaving Void Garden at 4K, the next theme never drew). The polling goes on (it is `gpu`'s
 * own, ended by its dispose) and the frames are undrawn ones; `maxMs` lets it go anyway where the
 * frames do not come (a hidden page). Idempotent.
 */
export function disposeAfterFrames(gpu, win, { frames = 3, maxMs = 400 } = {}) {
  let done = false;
  let timer = null;
  const go = () => {
    if (done) return;
    done = true;
    win.clearTimeout?.(timer);
    gpu?.dispose?.();
  };
  if (!gpu || typeof win?.requestAnimationFrame !== 'function') {
    go();
    return;
  }
  timer = win.setTimeout?.(go, maxMs);
  let seen = 0;
  const step = () => {
    if (done) return;
    if (++seen >= frames) go();
    else win.requestAnimationFrame(step);
  };
  win.requestAnimationFrame(step);
}

/**
 * The GPU time of the draws on the WebGL2 tier, from the browser's timer queries
 * (`EXT_disjoint_timer_query_webgl2`), or null where the browser gives none (Firefox does not):
 * `begin()` and `end()` around the draws of a frame, `read()` the mean ms of the frames that have
 * finished since the last read (null when none has; a disjoint reading is dropped). At most 8
 * queries are in flight. Only one such query may be open, so the renderer's own are off.
 */
export function createQueryTimer(gl) {
  const ext = gl?.getExtension?.('EXT_disjoint_timer_query_webgl2');
  if (!ext) return null;
  const pending = [];
  let open = null;
  let sum = 0;
  let count = 0;
  const poll = () => {
    while (pending.length
      && gl.getQueryParameter(pending[0], gl.QUERY_RESULT_AVAILABLE)) {
      const query = pending.shift();
      if (!gl.getParameter(ext.GPU_DISJOINT_EXT)) {
        sum += gl.getQueryParameter(query, gl.QUERY_RESULT) / 1e6;
        count++;
      }
      gl.deleteQuery(query);
    }
  };
  return {
    begin() {
      if (open || pending.length >= 8) return;
      open = gl.createQuery();
      gl.beginQuery(ext.TIME_ELAPSED_EXT, open);
    },
    end() {
      if (!open) return;
      gl.endQuery(ext.TIME_ELAPSED_EXT);
      pending.push(open);
      open = null;
    },
    read() {
      poll();
      if (!count) return null;
      const ms = sum / count;
      sum = 0;
      count = 0;
      return ms;
    },
  };
}

/** What names the GPU: the WebGPU adapter's vendor, architecture and device, or the GL renderer's
 * (the unmasked strings where the browser gives them); empty when nothing says. */
export function deviceOf(adapter, gl) {
  try {
    const info = adapter?.info;
    if (info) return [info.vendor, info.architecture, info.device].filter(Boolean).join(' ');
    const ext = gl?.getExtension?.('WEBGL_debug_renderer_info');
    if (ext) return [gl.getParameter(ext.UNMASKED_VENDOR_WEBGL), gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)].filter(Boolean).join(' ');
    return String(gl?.getParameter?.(gl.RENDERER) ?? '');
  } catch {
    return '';
  }
}

/**
 * The renderer on `canvas`. Rejects when neither WebGPU nor WebGL2 gives a context. `load` is the
 * way the vendored files come in (`loadThree`; the checks hand a fake).
 */
export async function createGpu({
  canvas, forceWebGL: floor = false, why = null, load = loadThree, timers = globalThis,
  firefox = /\bFirefox\//.test(globalThis.navigator?.userAgent ?? ''),
}) {
  const { T, S } = await load();
  const { adapter, hasApi, secure } = await probeAdapter();
  const renderer = new T.WebGPURenderer({
    canvas,
    antialias: false,
    alpha: false,
    forceWebGL: floor || !adapter,
    powerPreference: 'high-performance',
    // the WebGPU tier reads its GPU time from timestamp queries; the WebGL2 tier's own timer
    // query (below) needs the renderer's off
    trackTimestamp: Boolean(adapter) && !floor,
  });
  await renderer.init();
  const tier = renderer.backend.isWebGPUBackend ? 'webgpu' : 'webgl2';
  // the WebGPU tier's GPU time is read from timestamp queries; the WebGL2 tier's from the timer
  // query below, where the browser has one; `hasFeature` is legal only after `init`
  const timestamps = tier === 'webgpu' && renderer.hasFeature('timestamp-query');
  const timer = tier === 'webgl2' ? createQueryTimer(renderer.backend?.gl) : null;
  const caps = {
    tier,
    compute: true,
    neighbours: tier === 'webgpu',
    timestamps,
    // where the GPU time comes from: 'timestamps' (WebGPU), 'query' (WebGL2), or null
    timer: timestamps ? 'timestamps' : timer ? 'query' : null,
    maxParticles: tier === 'webgpu' ? MAX_PARTICLES * WEBGPU_PARTICLE_FACTOR : MAX_PARTICLES,
    // for the readout: the adapter's vendor (empty without one), why the WebGL2 tier (null on
    // WebGPU)
    vendor: String(adapter?.info?.vendor ?? ''),
    // which GPU, for the budget's kept tier: the adapter's words, or the GL renderer's
    device: deviceOf(tier === 'webgpu' ? adapter : null, renderer.backend?.gl),
    reason: tier === 'webgpu' ? null : (floor && why) || tierReason({ forced: floor, hasApi, secure }),
  };
  if (tier === 'webgl2') flushAfterCompute(renderer);
  // Firefox's alone: Chromium notices finished work without it (and a phone pays for the submits)
  const unpoll = tier === 'webgpu' && firefox ? keepPolled(renderer, { timers }) : () => {};
  renderer.setPixelRatio(1);
  let resolving = false;

  return {
    T,
    S,
    tier,
    caps,
    renderer,
    timer,

    /** The backing size of the canvas, in pixels: no inline style (the CSP's `style-src 'self'`).
     */
    size(width, height) {
      renderer.setSize(Math.max(1, Math.round(width)), Math.max(1, Math.round(height)), false);
    },

    render(scene, camera) {
      renderer.render(scene, camera);
    },

    /** Compile every material `camera` sees in `scene`, without drawing: a promise. */
    compile(scene, camera) {
      return renderer.compileAsync(scene, camera);
    },

    /** A render target of `width` by `height` for a small readback. */
    target(width, height) {
      return new T.RenderTarget(width, height, { type: T.UnsignedByteType });
    },

    /** Render `scene` with `camera` into `target` and read its pixels (RGBA bytes, row by row). */
    async read(scene, camera, target) {
      renderer.setRenderTarget(target);
      renderer.render(scene, camera);
      renderer.setRenderTarget(null);
      const pixels = await renderer.readRenderTargetPixelsAsync(
        target, 0, 0, target.width, target.height,
      );
      return new Uint8Array(pixels.buffer ?? pixels);
    },

    /** The GPU time of the renders and the compute passes since the last read, in ms, or null where
     * the tier gives none (or one is already being read). */
    async gpuMs() {
      if (!timestamps || resolving) return null;
      resolving = true;
      try {
        const render = await renderer.resolveTimestampsAsync('render');
        const compute = await renderer.resolveTimestampsAsync('compute');
        const ms = (Number.isFinite(render) ? render : 0) + (Number.isFinite(compute) ? compute : 0);
        return ms > 0 ? ms : null;
      } finally {
        resolving = false;
      }
    },

    dispose() {
      unpoll();
      renderer.dispose();
    },
  };
}
