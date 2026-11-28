# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's land layers in a browser: the sky, the land, the river with its
waterfalls and the flora, on both tiers. Every layer's materials compile with no console error or
warning; a layer's bisect switch (the layer list's `setOff`) removes its pixels; the uniforms move
with the cues they read (a muted fixture frosts the land: its saturation drops; a changed path
thickens the fog over the far land; another palette pair recolours the sky and the water); the
terrain's vertex function, read back through a float render target, equals the JS mirror within
0.05 at 3 x 256 sample points; a frame is the same for the same seed and time and another seed
differs (the real theme, through its driver). A cell is one engine at one tier (Chromium: stock is
WebGL2, the four SwiftShader flags give WebGPU; Firefox: stock is WebGL2, the `dom.webgpu.enabled`
pref gives WebGPU). Pixel checks are statistics of a small readback, never golden images and never
a frame rate.
  every_layers_materials_compile           sky, land, river, flora alone: compile and draw, nothing logged
  a_bisect_switch_removes_the_layers_pixels  each layer off changes the readback, and by enough
  the_shading_is_per_fragment              the rings at the budget's thinnest share (0.4), the fragment's octaves
                                           pinned, give the full rings' picture per lit pixel (mean |difference|)
                                           and keep its fine detail (luminance gradient per unit of luminance)
  the_budget_sets_the_fragment             octaves 8, 8, 7, 6 and detail 1, .7, .45, 0 by the ring share
  the_hero_lights_the_land                 the land alone, the hero's place set near the camera: brighter
  the_land_is_frosted_by_a_mute            the land alone, u.frost 1: the saturation drops to under half
  a_changed_path_thickens_the_fog          the land alone, u.haze 1: the picture changes toward the fog
  the_palette_pair_colours_the_sky_and_river  the sky alone (its own rows), the river alone
  the_seam_follows_build                   ctx.terrain is the mirror of the seed built last
  shared_state_survives_hidden_layers      a sky hidden from frame one, a land hidden over a flip
  the_rings_follow_the_camera              each ring around the camera, each cut by the finer one
  the_lattice_hash_is_the_cpus             the GPU's integer hash equals the CPU's bit for bit, 256 points
  the_vertex_height_is_the_mirror          the terrain function in a float target, 3 worlds
  the_frame_is_reproducible_and_seeded     `driver.frame(7, 2)` twice the same, another seed differs, and
                                           the same again after another seed and moment
  the_terrain_cache                        texels = the mirror's, no tile seam, cached land = direct land,
                                           streamed = cold, a build keeps it warm, a seek draws first, <= 32 MB,
                                           reset, packed RGBA8 (its own page evaluations), the step's detail term
  no_console_error_warning_or_csp          every page of the cell: none
Usage: python3 voidgarden_land.py chromium|firefox [webgl2,webgpu]"""
import sys
import time

import common

engine = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
CELLS = (sys.argv[2] if len(sys.argv) > 2 else 'webgl2,webgpu').split(',')
fails = []
passed = 0
t0 = time.time()

CHROMIUM_WEBGPU = ['--enable-unsafe-webgpu', '--enable-features=Vulkan',
                   '--use-webgpu-adapter=swiftshader', '--use-angle=swiftshader']
FIREFOX_WEBGPU = ['user_pref("dom.webgpu.enabled", true);']

HOOK = """window.__vg = { logs: [], csp: [] };
for (const k of ['warn', 'error']) { const o = console[k].bind(console);
  console[k] = (...a) => { window.__vg.logs.push(k + ': ' + a.map(String).join(' ').slice(0, 300)); o(...a); }; }
addEventListener('securitypolicyviolation', (e) => window.__vg.csp.push(e.violatedDirective + ' ' + e.blockedURI));
addEventListener('error', (e) => window.__vg.logs.push('error: ' + e.message));
addEventListener('unhandledrejection', (e) => window.__vg.logs.push('rejection: ' + e.reason));"""

BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
DRIVER = '__core.theme.driver'


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def poll(read, secs=8.0, every=0.2):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(every)
        got = read()
    return got


def make(cell):
    if engine == 'chromium':
        b = common.Chromium(size=(1280, 800), args=CHROMIUM_WEBGPU if cell == 'webgpu' else [])
    else:
        b = common.Firefox(size=(1280, 800),
                           extra_prefs=FIREFOX_WEBGPU if cell == 'webgpu' else [])
    b.preload(HOOK)
    return b


SEEN = []


def harvest(b):
    try:
        SEEN.extend(b.page('window.__vg.logs') or [])
        SEEN.extend(f'csp: {c}' for c in b.page('window.__vg.csp') or [])
    except Exception:
        pass


def logs(b):
    harvest(b)
    found = list(SEEN)
    SEEN.clear()
    errs = b.errors if engine == 'chromium' else b.collect_errors()
    return found + [e for e in errs if 'favicon' not in e]


# The page-level cases. One renderer and one layer list of a 160 x 100 target; `look(seed, z, over)`
# builds the seed, puts the camera on the river at depth `z` and reads the pixels back (RGBA bytes).
PRELUDE = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLayers } = await import(base + 'layers.js');
  const { createRig } = await import(base + 'motion.js');
  const land = await import(base + 'layers/land.js');
  const out = { logs: {} };
  const canvas = document.createElement('canvas');
  canvas.width = 16; canvas.height = 16;
  document.body.append(canvas);
  const gpu = await createGpu({ canvas });
  const { T, S } = gpu;
  gpu.size(16, 16);
  const W = 160, H = 100;
  const target = gpu.target(W, H);
  const layers = createLayers(gpu, { seed: 7 });
  const { ctx } = layers;
  const rigU = createRig({ seed: 7 }).u;
  const budget = { grit: true, bloom2: true, q: 1, rings: 1, scale: 1, capped: false };
  const pairs = [['#7fe0ff', '#ff7a4d'], ['#ff6464', '#62ffe9']];
  const BW = 480, BH = 300;
  const targetBig = gpu.target(BW, BH);
  const look = async (seed, z, over = {}, palette = 0, bud = budget, big = false, hero = null) => {
    layers.build(seed);
    ctx.hero = hero;
    const t = ctx.terrain;
    const cam = { x: t.riverX(z), y: 9, z, lookX: t.riverX(z - 34), lookY: 4, lookZ: z - 34,
      fov: 55, roll: 0, aspect: W / H, dist: 1 };
    layers.update({ t: 20, dt: 0, cam, u: { ...rigU, time: 20, colRate: 1, loud: 0.4, ...over },
      budget: bud, palette: pairs[palette], seed });
    ctx.camera.aspect = W / H; ctx.camera.updateProjectionMatrix();
    return gpu.read(ctx.scene, ctx.camera, big ? targetBig : target);
  };
  const stat = (px) => {
    let r = 0, g = 0, b = 0, sat = 0, lit = 0;
    for (let i = 0; i < W * H; i++) {
      const R = px[i * 4], G = px[i * 4 + 1], B = px[i * 4 + 2];
      r += R; g += G; b += B; sat += Math.max(R, G, B) - Math.min(R, G, B);
      if (0.2126 * R + 0.7152 * G + 0.0722 * B > 16) lit++;
    }
    const n = W * H;
    return { r: r / n, g: g / n, b: b / n, sat: sat / n, lit };
  };
  const region = (px, y0, y1) => {
    let r = 0, g = 0, b = 0, n = 0;
    for (let y = Math.floor(H * y0); y < Math.floor(H * y1); y++) for (let x = 0; x < W; x++) {
      const i = (y * W + x) * 4; r += px[i]; g += px[i + 1]; b += px[i + 2]; n++;
    }
    return [r / n, g / n, b / n];
  };
  const differ = (a, b) => { let n = 0;
    for (let i = 0; i < W * H; i++) { if (Math.abs(a[i*4]-b[i*4]) + Math.abs(a[i*4+1]-b[i*4+1]) + Math.abs(a[i*4+2]-b[i*4+2]) > 12) n++; }
    return n; };
  const L = (px, i) => px[i * 4] + px[i * 4 + 1] + px[i * 4 + 2];
  const fine = (px) => {
    let n = 0, lum = 0, grad = 0;
    for (let y = 0; y < BH - 1; y++) for (let x = 0; x < BW - 1; x++) {
      const i = y * BW + x;
      if (L(px, i) < 6 || L(px, i + 1) < 6 || L(px, i + BW) < 6) continue;
      n++; lum += L(px, i); grad += Math.abs(L(px, i + 1) - L(px, i)) + Math.abs(L(px, i + BW) - L(px, i));
    }
    return { n, lum: lum / Math.max(n, 1), grad: grad / Math.max(lum, 1) };
  };
  const only = (name) => layers.setOff(layers.list.map((l) => l.name).filter((n) => n !== name));
"""

TAIL = """  out.logs.all = window.__vg.logs.slice();
  return out;
})()"""

LAND_CASES = PRELUDE + """  const names = ['sky', 'land', 'river', 'flora'];
  // every layer alone compiles and draws with nothing logged
  out.compile = {};
  for (const name of names) {
    const before = window.__vg.logs.length;
    layers.setOff(layers.list.map((l) => l.name).filter((n) => n !== name));
    await look(name === 'flora' ? 11 : 7, name === 'flora' ? -200 : -175);
    await gpu.renderer.compileAsync(ctx.scene, ctx.camera);
    const px = await look(name === 'flora' ? 11 : 7, name === 'flora' ? -200 : -175);
    out.compile[name] = { lit: stat(px).lit, logged: window.__vg.logs.slice(before),
      count: layers.list.find((l) => l.name === name).object.children.map((c) => c.count ?? 0).reduce((a, b) => a + b, 0) };
  }
  layers.setOff([]);
  // a bisect switch removes the layer's pixels
  out.bisect = {};
  for (const [name, seed, z] of [['sky', 7, -175], ['land', 7, -175], ['river', 7, -175], ['flora', 11, -200]]) {
    const on = await look(seed, z);
    layers.setOff([name]);
    const off = await look(seed, z);
    layers.setOff([]);
    out.bisect[name] = { changed: differ(on, off), onLit: stat(on).lit, offLit: stat(off).lit };
  }
  // the cues, each with only the layer it names on (the others hidden)
  // readPixels rows run bottom-up on WebGL2: the top quarter of the picture is the last rows there
  const top = gpu.tier === 'webgl2' ? [0.75, 1] : [0, 0.25];
  only('land');
  const live = await look(7, -175);
  const frosted = await look(7, -175, { frost: 1 });
  out.frost = { live: stat(live).sat, frosted: stat(frosted).sat };
  // the shading is per fragment: the thinnest rings (share 0.4) with the fragment's octaves and detail
  // pinned to the full ones' give the same picture per lit pixel (a per-vertex shading would differ
  // wherever a facet lies) and keep the luminance's fine gradient
  out.thin = [];
  for (const [seed, z] of [[7, -175], [4242, -20], [90125, -1790]]) {
    const rec = { seed, z };
    ctx.land.pin = { top: 8, det: 1 };
    const px = {};
    for (const [key, rings] of [['full', 1], ['thin', 0.4]]) {
      px[key] = await look(seed, z, {}, 0, { ...budget, rings }, true);
      rec[key] = fine(px[key]);
    }
    ctx.land.pin = null;
    let diff = 0, both = 0;
    for (let i = 0; i < BW * BH; i++) {
      if (L(px.full, i) < 6 && L(px.thin, i) < 6) continue;
      both++; diff += Math.abs(L(px.full, i) - L(px.thin, i));
    }
    rec.meanDiff = diff / Math.max(both, 1) / 3;
    rec.meanLum = rec.full.lum / 3;
    out.thin.push(rec);
  }
  // the budget's step lowers the fragment's cost: octaves 8, 8, 7, 6 and detail 1 .. 0 by the share
  ctx.land.pin = null;
  {
    layers.build(7);
    const t = ctx.terrain;
    const got = [];
    for (const rings of [1, 0.75, 0.55, 0.4]) {
      const cam = { x: t.riverX(-175), y: 9, z: -175, lookX: t.riverX(-209), lookY: 4, lookZ: -209,
        fov: 55, roll: 0, aspect: W / H, dist: 1 };
      layers.update({ t: 20, dt: 0, cam, u: { ...rigU, time: 20, colRate: 1, loud: 0.4 },
        budget: { ...budget, rings }, palette: pairs[0], seed: 7 });
      got.push(layers.list.find((l) => l.name === 'land').fragment());
    }
    out.fragSteps = got;
  }
  // the hero lights the land near it (the land alone, the hero's place as it publishes it)
  const unlit = await look(7, -175);
  const heroLit = await look(7, -175, {}, 0, budget, false,
    { pos: [ctx.terrain.riverX(-183), 5, -183], hs: 6 });
  out.heroLight = { unlit: stat(unlit), lit: stat(heroLit) };
  const clear = await look(7, -175, { haze: 0 });
  const hazy = await look(7, -175, { haze: 1 });
  out.haze = { changed: differ(clear, hazy), lit: stat(clear).lit,
    clear: stat(clear), hazy: stat(hazy) };
  only('sky');
  const skyA = await look(7, -175, {}, 0);
  const skyB = await look(7, -175, {}, 1);
  out.palette = { first: region(skyA, top[0], top[1]), second: region(skyB, top[0], top[1]) };
  only('river');
  const waterA = await look(7, -175, {}, 0);
  const waterB = await look(7, -175, {}, 1);
  out.water = { first: region(waterA, 0, 1), second: region(waterB, 0, 1), lit: stat(waterA).lit };
  layers.setOff([]);
  // the seam follows build: the same land as the mirror of the seed built last
  out.seam = [];
  for (const seed of [4242, 7]) {
    layers.build(seed);
    const mir = land.createTerrain(seed);
    let worst = 0;
    for (const [x, z] of [[0, -50], [30, -400], [-60, -900], [12, -1500], [5, 40], [80, -2500],
      [-20, -3100], [44, -150], [-9, -700], [3, -1900]]) {
      worst = Math.max(worst, Math.abs(ctx.terrain.height(x, z) - mir.height(x, z, 5)),
        Math.abs(ctx.terrain.riverX(z) - mir.riverX(z)));
    }
    out.seam.push({ seed, worst, other: Math.abs(ctx.terrain.riverX(-400)
      - land.createTerrain(seed === 7 ? 4242 : 7).riverX(-400)) });
  }
  // shared state survives a hidden layer: a sky hidden from the first frame, a land hidden across
  // a world change
  {
    const make = (off) => createLayers(gpu, { seed: 7, off: new Set(off) });
    const frameOf = (l, z, extra = {}) => { const t = l.ctx.terrain;
      l.update({ t: 20, dt: 0, cam: { x: t.riverX(z), y: 9, z, lookX: t.riverX(z - 34), lookY: 4,
        lookZ: z - 34, fov: 55, roll: 0, aspect: 1.6, dist: 1 },
        u: { ...rigU, time: 20, colRate: 1, loud: 0.4 }, budget, palette: pairs[1], seed: 7, ...extra }); };
    const whole = make([]);
    frameOf(whole, -175);
    const dark = make(['sky']);
    frameOf(dark, -175);
    const c = (l) => l.ctx.env.horC.value.toArray().map((v) => +v.toFixed(5));
    out.hidden = { sky: c(dark), whole: c(whole), sun: dark.ctx.env.sunDir.value.toArray(),
      sunWhole: whole.ctx.env.sunDir.value.toArray() };
    const lone = make(['land']);
    frameOf(lone, -100);
    lone.build(90125);
    frameOf(lone, -100);
    frameOf(lone, -1850);
    const W = lone.ctx.land.nodes.W;
    out.hidden.world = { arZ: W.arZ.value, want: lone.ctx.land.mirror.refAt(-1850).arZ,
      riv: W.riv.value.toArray(), wantRiv: lone.ctx.land.mirror.params.riv };
    for (const l of [whole, dark, lone]) l.dispose();
  }
  // the rings follow the camera and each cuts away what the finer one covers
  {
    layers.build(7);
    const t = ctx.terrain;
    const cam = { x: t.riverX(-300), y: 9, z: -300, lookX: t.riverX(-334), lookY: 4, lookZ: -334,
      fov: 55, roll: 0, aspect: 1.6, dist: 1 };
    layers.update({ t: 20, dt: 0, cam, u: { ...rigU, time: 20, colRate: 1 }, budget,
      palette: pairs[0], seed: 7 });
    out.rings = { cam, rings: ctx.land.rings };
  }
  // the terrain's vertex function read back: x from the quad's u, z from its v (stored, so the
  // readback's row order does not matter), three worlds
  const sample = async (seed, z0) => {
    layers.build(seed);
    const t = ctx.terrain;
    const cx = t.riverX(z0);
    layers.update({ t: 1, dt: 0, cam: { x: cx, y: 9, z: z0, lookX: cx, lookY: 4, lookZ: z0 - 34, fov: 55, roll: 0, aspect: 1, dist: 1 },
      u: { ...rigU, colRate: 1 }, budget, palette: pairs[0], seed });
    const nodes = ctx.land.nodes;
    const x0 = cx - 60, zspan = 120, xspan = 120;
    const uvn = S.uv();
    const x = S.float(x0).add(uvn.x.mul(xspan));
    const z = S.float(z0 - zspan / 2).add(uvn.y.mul(zspan));
    const m = new T.MeshBasicNodeMaterial();
    // the target keeps no negative value: the numbers are written 500 up
    m.colorNode = S.vec4(nodes.height(5)(x, z).add(500), nodes.height(7)(x, z).add(500),
      nodes.riverX(z).add(500), uvn.y);
    m.transparent = true; m.blending = T.NoBlending;
    const scene = new T.Scene();
    scene.add(new T.Mesh(new T.PlaneGeometry(2, 2), m));
    const rt = new T.RenderTarget(16, 16, { type: T.FloatType });
    gpu.renderer.setRenderTarget(rt);
    gpu.renderer.render(scene, new T.OrthographicCamera(-1, 1, 1, -1, -1, 1));
    gpu.renderer.setRenderTarget(null);
    const raw = await gpu.renderer.readRenderTargetPixelsAsync(rt, 0, 0, 16, 16);
    const mir = ctx.land.mirror;
    let worst5 = 0, worst7 = 0, worstR = 0, bad = 0;
    for (let j = 0; j < 16; j++) for (let i = 0; i < 16; i++) {
      const p = (j * 16 + i) * 4;
      const xs = x0 + ((i + 0.5) / 16) * xspan;
      const zs = z0 - zspan / 2 + raw[p + 3] * zspan;
      const e5 = Math.abs(raw[p] - 500 - mir.height(xs, zs, 5));
      const e7 = Math.abs(raw[p + 1] - 500 - mir.height(xs, zs, 7));
      const er = Math.abs(raw[p + 2] - 500 - mir.riverX(zs));
      if (!(e5 < 1e9)) bad++;
      worst5 = Math.max(worst5, e5); worst7 = Math.max(worst7, e7); worstR = Math.max(worstR, er);
    }
    rt.dispose(); m.dispose();
    return { seed, z0, worst5, worst7, worstR, bad };
  };
  // the lattice hash: the GPU's integers give the CPU's bits, at 16 x 16 lattice points (a column
  // of the target is one x whichever way the rows run; the y is stored)
  {
    const nodes = ctx.land.nodes;
    const uvn = S.uv();
    const ix = S.floor(uvn.x.mul(15.99)).mul(37).sub(300);
    const iy = S.floor(uvn.y.mul(15.99)).mul(53).sub(400);
    const m = new T.MeshBasicNodeMaterial();
    m.colorNode = S.vec4(nodes.lattice(ix, iy), nodes.lattice(ix.add(1), iy), 0, iy.add(500));
    m.transparent = true; m.blending = T.NoBlending;
    const scene = new T.Scene();
    scene.add(new T.Mesh(new T.PlaneGeometry(2, 2), m));
    const rt = new T.RenderTarget(16, 16, { type: T.FloatType });
    gpu.renderer.setRenderTarget(rt);
    gpu.renderer.render(scene, new T.OrthographicCamera(-1, 1, 1, -1, -1, 1));
    gpu.renderer.setRenderTarget(null);
    const raw = await gpu.renderer.readRenderTargetPixelsAsync(rt, 0, 0, 16, 16);
    let worst = 0, wrong = 0;
    for (let j = 0; j < 16; j++) for (let i = 0; i < 16; i++) {
      const p = (j * 16 + i) * 4;
      const x = i * 37 - 300, y = Math.round(raw[p + 3] - 500);
      const e = Math.max(Math.abs(raw[p] - land.latticeHash(x, y)), Math.abs(raw[p + 1] - land.latticeHash(x + 1, y)));
      worst = Math.max(worst, e); if (e > 0) wrong++;
    }
    out.lattice = { worst, wrong };
    rt.dispose(); m.dispose();
  }
  out.height = [];
  for (const [seed, z0] of [[7, -175], [4242, -20], [90125, -1790]]) out.height.push(await sample(seed, z0));
""" + TAIL

# The terrain cache on its own page evaluation (the land alone): texels, seams, cached against direct,
# streamed against cold, the step, a seek, memory, reset. Each evaluation is well under the 60 s the
# CDP/BiDi evaluate allows.
CACHE_CASES = PRELUDE + """
  only('land');
  {
    const LC = await import(base + 'layers/land-cache.js');
    const R = LC.RES;
    const texels = async (cache, mir) => {
      const rows = [];
      for (const L of cache.levels) {
        const raw = await gpu.renderer.readRenderTargetPixelsAsync(L.rts[0], 0, 0, R, R);
        const at = (gx, gz) => {
          const sx = ((gx % R) + R) % R, sy = ((gz % R) + R) % R;
          const p = ((gpu.tier === 'webgl2' ? R - 1 - sy : sy) * R + sx) * 4;
          if (cache.packed) {
            // the 16-bit height, the normal's stereographic projection
            const sx = (raw[p + 2] / 255) * 2 - 1, sz = (raw[p + 3] / 255) * 2 - 1;
            const d = 1 + sx * sx + sz * sz;
            return { h: ((raw[p] * 256 + raw[p + 1]) / 65535) * 256 - 64,
              n: [(2 * sx) / d, (1 - sx * sx - sz * sz) / d, (2 * sz) / d] };
          }
          const f = (v) => (raw instanceof Uint16Array ? T.DataUtils.fromHalfFloat(v) : v);
          return { h: f(raw[p]), n: [f(raw[p + 1]), f(raw[p + 3]), f(raw[p + 2])] };
        };
        const angle = (a, m) => {
          const len = Math.hypot(...a.n);
          const c = (a.n[0] * m.n[0] + a.n[1] * m.n[1] + a.n[2] * m.n[2]) / len;
          return (Math.acos(Math.min(1, c)) * 180) / Math.PI;
        };
        // pairs of texels on either side of tile edges, along rows and columns of the shown tiles
        let dh = 0, dn = 0, seam = 0, n = 0;
        const sh = L.shown;
        for (let r = 0; r < 6; r++) {
          for (let e = 1; e < 13; e += 2) {
            for (const across of [false, true]) {
              const line = (across ? sh.x0 : sh.z0) * 32 + 5 + r * 71;
              const edge = ((across ? sh.z0 : sh.x0) + e) * 32;
              const g = (i) => (across ? [line, i] : [i, line]);
              const a = at(...g(edge - 1)), b = at(...g(edge));
              const ma = LC.cacheTexel(mir, L.k, ...g(edge - 1), 8);
              const mb = LC.cacheTexel(mir, L.k, ...g(edge), 8);
              dh = Math.max(dh, Math.abs(a.h - ma.h), Math.abs(b.h - mb.h));
              dn = Math.max(dn, angle(a, ma), angle(b, mb));
              seam = Math.max(seam, Math.abs((b.h - a.h) - (mb.h - ma.h)));
              n += 2;
            }
          }
        }
        rows.push({ k: L.k, n, dh, dn, seam });
      }
      return rows;
    };
    const cache = ctx.land.cache;
    out.cache = { texels: [], look: [] };
    for (const [seed, z] of [[7, -175], [90125, -1790]]) {
      await look(seed, z);
      out.cache.texels.push({ seed, z, levels: await texels(cache, ctx.land.mirror) });
    }
    // cached against direct, per lit pixel, and the fine shading kept
    for (const [seed, z] of [[7, -175], [4242, -20], [90125, -1790]]) {
      const cached = await look(seed, z, {}, 0, budget, true);
      ctx.land.direct = true;
      const direct = await look(seed, z, {}, 0, budget, true);
      ctx.land.direct = false;
      let diff = 0, both = 0, lum = 0;
      for (let i = 0; i < BW * BH; i++) {
        if (L(cached, i) < 6 && L(direct, i) < 6) continue;
        both++; diff += Math.abs(L(cached, i) - L(direct, i)); lum += L(direct, i);
      }
      out.cache.look.push({ seed, z, n: both, meanDiff: diff / Math.max(both, 1) / 3,
        meanLum: lum / Math.max(both, 1) / 3, grad: fine(cached).grad / fine(direct).grad });
    }
    // a place reached by a flight in small steps that stream (16 u/s at 30 fps; no seek, no build)
    // is the place a cold cache draws (`createLayers` makes a cache of its own and one update fills
    // it: cold by construction, as driver.frame is)
    const flyTo = (z, bud = budget, l = layers) => { const t = l.ctx.terrain;
      l.update({ t: 20, dt: 0, cam: { x: t.riverX(z), y: 9, z, lookX: t.riverX(z - 34), lookY: 4,
        lookZ: z - 34, fov: 55, roll: 0, aspect: W / H, dist: 1 }, u: { ...rigU, time: 20, colRate: 1, loud: 0.4 },
        budget: bud, palette: pairs[0], seed: 7 }); };
    const shot = (l) => { l.ctx.camera.aspect = W / H; l.ctx.camera.updateProjectionMatrix();
      return gpu.read(l.ctx.scene, l.ctx.camera, targetBig); };
    const moved = (a, b) => { let n = 0; for (let i = 0; i < BW * BH * 4; i++) if (a[i] !== b[i]) n++; return n; };
    await look(7, -175, {}, 0, budget, true);
    const flown0 = cache.stats();
    for (let z = -175; z > -400; z -= 0.53) flyTo(z);
    for (let i = 0; i < 60; i++) flyTo(-400);
    const streamed = shot(layers);
    const flown1 = cache.stats();
    const coldL = createLayers(gpu, { seed: 7 });
    coldL.setOff(coldL.list.map((l) => l.name).filter((n) => n !== 'land'));
    flyTo(-400, budget, coldL);
    const coldPx = shot(coldL);
    out.cache.warm = { moved: moved(streamed, coldPx), tiles: flown1.tiles - flown0.tiles,
      sync: flown1.sync - flown0.sync, coldSync: coldL.ctx.land.cache.stats().sync };
    // `build` of the live seed (a new terrain object: driver.frame ends with one) keeps the cache
    layers.build(7);
    flyTo(-400);
    const kept = cache.stats();
    out.cache.keep = { tiles: kept.tiles - flown1.tiles, sync: kept.sync - flown1.sync,
      moved: moved(shot(layers), coldPx) };
    // the budget's step: the cache keeps the mockup's octaves, so no tile is redrawn or stale and no tile's
    // edge steps; the step sets the detail term (and the rings)
    const low = { ...budget, rings: 0.4 };
    for (let i = 0; i < 30; i++) flyTo(-400, low);
    const lowStats = cache.stats();
    out.cache.step = { redrawn: lowStats.tiles - kept.tiles, sync: lowStats.sync - kept.sync,
      octs: cache.levels.map((Lv) => Lv.oct.value), wantOcts: cache.levels.map((Lv) => LC.levelOct(8, Lv.k)),
      frag: layers.list.find((l) => l.name === 'land').fragment(),
      texels: await texels(cache, ctx.land.mirror) };
    for (let i = 0; i < 30; i++) flyTo(-400);
    coldL.dispose();
    // a seek: one update far away draws every shown tile of every level
    const syncs = cache.stats().sync;
    flyTo(-5000);
    out.cache.seek = { syncs: cache.stats().sync - syncs, levels: cache.levels.map((Lv) => {
      const sh = Lv.shown;
      let held = 0;
      for (let tz = sh.z0; tz < sh.z1; tz++) for (let tx = sh.x0; tx < sh.x1; tx++) {
        if (Lv.keys[LC.slotOf(tz) * LC.CACHE.tiles + LC.slotOf(tx)] === `${tx},${tz}`) held++;
      }
      return { centred: sh.x0 === Lv.cx - 7 && sh.z0 === Lv.cz - 7 && sh.x1 === Lv.cx + 7 && sh.z1 === Lv.cz + 7, held };
    }) };
    // and back by a seek: the place is the cold one
    flyTo(-400);
    out.cache.seekBack = { moved: moved(shot(layers), coldPx) };
    // the memory of its textures
    const bytesOf = (rt) => rt.width * rt.height * (rt.texture.type === T.HalfFloatType ? 8 : 4);
    out.cache.bytes = cache.levels.reduce((a, Lv) => a + Lv.rts.reduce((b, rt) => b + bytesOf(rt), 0), 0);
    out.cache.stated = cache.stats().bytes;
    // reset drops the tiles and rebuilds them: the place is the cold one again
    cache.reset();
    flyTo(-400);
    out.cache.reset = { moved: moved(shot(layers), coldPx), sync: cache.stats().sync };
  }
""" + TAIL

# The packed RGBA8 encoding (a device without float targets) on a page of its own.
PACKED_CASES = PRELUDE + """
  only('land');
  {
    const LC = await import(base + 'layers/land-cache.js');
    const R = LC.RES;
    const texels = async (cache, mir) => {
      const rows = [];
      for (const L of cache.levels) {
        const raw = await gpu.renderer.readRenderTargetPixelsAsync(L.rts[0], 0, 0, R, R);
        const at = (gx, gz) => {
          const sx = ((gx % R) + R) % R, sy = ((gz % R) + R) % R;
          const p = ((gpu.tier === 'webgl2' ? R - 1 - sy : sy) * R + sx) * 4;
          if (cache.packed) {
            // the 16-bit height, the normal's stereographic projection
            const sx = (raw[p + 2] / 255) * 2 - 1, sz = (raw[p + 3] / 255) * 2 - 1;
            const d = 1 + sx * sx + sz * sz;
            return { h: ((raw[p] * 256 + raw[p + 1]) / 65535) * 256 - 64,
              n: [(2 * sx) / d, (1 - sx * sx - sz * sz) / d, (2 * sz) / d] };
          }
          const f = (v) => (raw instanceof Uint16Array ? T.DataUtils.fromHalfFloat(v) : v);
          return { h: f(raw[p]), n: [f(raw[p + 1]), f(raw[p + 3]), f(raw[p + 2])] };
        };
        const angle = (a, m) => {
          const len = Math.hypot(...a.n);
          const c = (a.n[0] * m.n[0] + a.n[1] * m.n[1] + a.n[2] * m.n[2]) / len;
          return (Math.acos(Math.min(1, c)) * 180) / Math.PI;
        };
        // pairs of texels on either side of tile edges, along rows and columns of the shown tiles
        let dh = 0, dn = 0, seam = 0, n = 0;
        const sh = L.shown;
        for (let r = 0; r < 6; r++) {
          for (let e = 1; e < 13; e += 2) {
            for (const across of [false, true]) {
              const line = (across ? sh.x0 : sh.z0) * 32 + 5 + r * 71;
              const edge = ((across ? sh.z0 : sh.x0) + e) * 32;
              const g = (i) => (across ? [line, i] : [i, line]);
              const a = at(...g(edge - 1)), b = at(...g(edge));
              const ma = LC.cacheTexel(mir, L.k, ...g(edge - 1), 8);
              const mb = LC.cacheTexel(mir, L.k, ...g(edge), 8);
              dh = Math.max(dh, Math.abs(a.h - ma.h), Math.abs(b.h - mb.h));
              dn = Math.max(dn, angle(a, ma), angle(b, mb));
              seam = Math.max(seam, Math.abs((b.h - a.h) - (mb.h - ma.h)));
              n += 2;
            }
          }
        }
        rows.push({ k: L.k, n, dh, dn, seam });
      }
      return rows;
    };
    const cold = await look(7, -175, {}, 0, budget, true);
    out.packed = {};
    // the packed encoding, as a device without float targets has it
    gpu.caps.floatTargets = false;
    const packedLayers = createLayers(gpu, { seed: 7 });
    delete gpu.caps.floatTargets;
    packedLayers.setOff(packedLayers.list.map((l) => l.name).filter((n) => n !== 'land'));
    {
      const t = packedLayers.ctx.terrain;
      packedLayers.update({ t: 20, dt: 0, cam: { x: t.riverX(-175), y: 9, z: -175, lookX: t.riverX(-209),
        lookY: 4, lookZ: -209, fov: 55, roll: 0, aspect: W / H, dist: 1 },
        u: { ...rigU, time: 20, colRate: 1, loud: 0.4 }, budget, palette: pairs[0], seed: 7 });
      packedLayers.ctx.camera.aspect = W / H; packedLayers.ctx.camera.updateProjectionMatrix();
      const px = await gpu.read(packedLayers.ctx.scene, packedLayers.ctx.camera, targetBig);
      let diff = 0, both = 0, lum = 0;
      for (let i = 0; i < BW * BH; i++) {
        if (L(cold, i) < 6 && L(px, i) < 6) continue;
        both++; diff += Math.abs(L(cold, i) - L(px, i)); lum += L(cold, i);
      }
      out.packed = { isPacked: packedLayers.ctx.land.cache.packed, meanDiff: diff / Math.max(both, 1) / 3,
        meanLum: lum / Math.max(both, 1) / 3, texels: await texels(packedLayers.ctx.land.cache, packedLayers.ctx.land.mirror) };
    }
    packedLayers.dispose();
  }
""" + TAIL



def land_cases(b, d, cell):
    harvest(b)
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    got = b.page(LAND_CASES)
    for name, c in got['compile'].items():
        check(f'[{cell}] {name} alone compiles, draws and logs nothing',
              c['logged'] == [] and (name == 'flora' and c['count'] > 0 or name != 'flora'), c)
    for name, c in got['bisect'].items():
        need = 40 if name in ('river', 'flora') else 400
        check(f'[{cell}] {name} off removes its pixels', c['changed'] >= need, c)
    f = got['frost']
    check(f'[{cell}] a mute frosts the land alone: the saturation drops to under half',
          f['live'] > 5 and f['frosted'] < f['live'] * 0.5, f)
    for t in got['thin']:
        check(f'[{cell}] the shading is per fragment: the thinnest rings, octaves and detail pinned, give '
              f'the full rings\' picture per lit pixel and keep its fine shading (seed {t["seed"]}, z {t["z"]})',
              t['thin']['n'] > 1500 and t['meanDiff'] < 0.06 * t['meanLum']
              and t['thin']['grad'] >= 0.9 * t['full']['grad'], t)
    check(f'[{cell}] the budget\'s step sets the fragment\'s octaves (8, 8, 7, 6) and detail (1, .7, .45, 0)',
          [(f['top'], round(f['det'], 2)) for f in got['fragSteps']]
          == [(8, 1), (8, 0.7), (7, 0.45), (6, 0)], got['fragSteps'])
    land_rest(b, got, cell)


def cache_cases(b, d, cell):
    """The terrain cache and its packed encoding, each on a page evaluation of its own."""
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    c = b.page(CACHE_CASES)['cache']
    for t in c['texels']:
        check(f'[{cell}] the cache\'s texels are the mirror\'s at every level (seed {t["seed"]}): height within '
              '0.08, normal within 1.5 deg', all(r['n'] >= 140 and r['dh'] < 0.08 and r['dn'] < 1.5
                                                 for r in t['levels']), t)
        check(f'[{cell}] no step across a tile\'s edge (seed {t["seed"]}): the height and normal continue as the '
              'mirror\'s', all(r['seam'] < 0.08 for r in t['levels']), t)
    for t in c['look']:
        check(f'[{cell}] the land shaded from the cache is the land shaded per fragment (seed {t["seed"]}): mean '
              '|difference| per lit pixel under 5 % of its light, the fine shading kept',
              t['n'] > 1500 and t['meanDiff'] < 0.05 * t['meanLum'] and t['grad'] >= 0.75, t)
    w = c['warm']
    check(f'[{cell}] a place reached by a streamed flight (small steps, no seek) is the cold cache\'s place: '
          'tiles were drawn, no level drawn at once, the pixels equal',
          w['tiles'] > 100 and w['sync'] == 0 and w['coldSync'] == 5 and w['moved'] == 0, w)
    check(f'[{cell}] a build of the live seed (driver.frame ends with one) leaves the live cache warm: '
          'nothing redrawn, the place unchanged', c['keep']['tiles'] == 0 and c['keep']['sync'] == 0
          and c['keep']['moved'] == 0, c['keep'])
    st = c['step']
    check(f'[{cell}] the budget\'s step sets the detail term, not the tiles\' octaves: after it no tile is '
          'redrawn, the octaves stay the mockup\'s, the fragment\'s detail falls, and no tile edge steps',
          st['redrawn'] == 0 and st['sync'] == 0 and st['octs'] == st['wantOcts']
          and st['frag']['det'] < 1 and all(r['n'] >= 140 and r['dh'] < 0.08 and r['dn'] < 1.5
                                            and r['seam'] < 0.08 for r in st['texels']), st)
    check(f'[{cell}] a seek draws every shown tile of every level before the frame',
          c['seek']['syncs'] == 5 and all(lv['centred'] and lv['held'] == 196 for lv in c['seek']['levels']),
          c['seek'])
    check(f'[{cell}] a place reached by a seek is the cold one', c['seekBack']['moved'] == 0, c['seekBack'])
    check(f'[{cell}] the cache\'s textures take at most 32 MB', c['bytes'] <= 32 * 2 ** 20
          and c['bytes'] == c['stated'], (c['bytes'], c['stated']))
    check(f'[{cell}] a reset cache drops its tiles and draws the place again, the cold one',
          c['reset']['moved'] == 0, c['reset'])
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    p = b.page(PACKED_CASES)['packed']
    check(f'[{cell}] the packed RGBA8 cache gives the float one\'s land and the mirror\'s texels',
          p['isPacked'] and p['meanDiff'] < 0.02 * p['meanLum']
          and all(r['dh'] < 0.08 and r['dn'] < 1.5 and r['seam'] < 0.08 for r in p['texels']), p)


def land_rest(b, got, cell):
    hl = got['heroLight']
    check(f'[{cell}] the hero lights the land near it: the land alone is brighter with the hero by it',
          hl['lit']['r'] + hl['lit']['g'] + hl['lit']['b'] > 1.005 * (hl['unlit']['r'] + hl['unlit']['g']
          + hl['unlit']['b']) and hl['unlit']['lit'] > 1500, hl)
    h = got['haze']
    mean = lambda s: (s['r'] + s['g'] + s['b'])  # noqa: E731
    check(f'[{cell}] a changed path thickens the fog over the land alone',
          h['changed'] > 800 and abs(mean(h['clear']) - mean(h['hazy'])) > 6, h)
    p = got['palette']
    check(f'[{cell}] another palette pair recolours the sky alone (its own rows)',
          sum(abs(a - c) for a, c in zip(p['first'], p['second'])) > 12, p)
    w = got['water']
    check(f'[{cell}] another palette pair recolours the river alone',
          w['lit'] > 200 and sum(abs(a - c) for a, c in zip(w['first'], w['second'])) > 1.5, w)
    for s in got['seam']:
        check(f'[{cell}] ctx.terrain after build({s["seed"]}) is that seed\'s mirror, and not the other\'s',
              s['worst'] < 1e-9 and s['other'] > 0.01, s)
    hd = got['hidden']
    check(f'[{cell}] a sky hidden from the first frame leaves the land the palette, mood and sun',
          hd['sky'] == hd['whole'] and hd['sun'] == hd['sunWhole'] and hd['sky'] != [1, 1, 1], hd)
    check(f'[{cell}] a land hidden across a world change keeps the world current for the water',
          hd['world']['arZ'] == hd['world']['want'] and hd['world']['arZ'] != 0
          and max(abs(a - c) for a, c in zip(hd['world']['riv'], hd['world']['wantRiv'])) < 1e-9,
          hd['world'])
    rg = got['rings']
    cam = rg['cam']
    inner = rg['rings']
    check(f'[{cell}] the rings follow the camera and each cuts away the finer one\'s square',
          all(abs(cam['x'] - r['cx']) < r['half'] and abs(cam['z'] - r['cz']) < r['half'] for r in inner)
          and all(abs(inner[i]['inner'][0] - inner[i - 1]['cx']) < 1e-9
                  and abs(inner[i]['inner'][1] - inner[i - 1]['cz']) < 1e-9
                  and abs(inner[i]['inner'][2] - (inner[i - 1]['half'] - 2 * inner[i]['sp'])) < 1e-6
                  and inner[i]['inner'][3] == 1 for i in (1, 2))
          and inner[0]['inner'][3] == 0, rg)
    check(f'[{cell}] the GPU\'s lattice hash is the CPU\'s bit for bit', got['lattice']['wrong'] == 0,
          got['lattice'])
    for e in got['height']:
        check(f'[{cell}] the vertex function is the mirror (seed {e["seed"]}, z {e["z0"]}, 5 and 7 octaves,'
              ' river x)', e['bad'] == 0 and e['worst5'] < 0.05 and e['worst7'] < 0.05
              and e['worstR'] < 0.05, e)


# the page's visibility is a switch: while it is hidden the live loop draws nothing, so the frames read
# below are the only draws (the live loop's own state is not the frame's)
HIDDEN = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
  document.dispatchEvent(new Event('visibilitychange')); })()"""


def the_frame(b, cell):
    stats = []
    b.page(HIDDEN)
    time.sleep(1.5)
    for seed, t in ((7, 2), (7, 2), (8, 2), (7, 40), (7, 2)):
        stats.append(b.page(f'{DRIVER}.frame({seed}, {t})'))
    check(f'[{cell}] the same seed and time give the same statistics, with lit pixels',
          stats[0] == stats[1] and stats[0]['lit'] > 100, stats[:2])
    check(f'[{cell}] another seed differs', stats[2]['places'] != stats[0]['places'], stats)
    check(f'[{cell}] after another seed and another moment (the terrain cache full of them) the frame is '
          'the cold one', stats[4] == stats[0], [stats[0], stats[4]])


def one_cell(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(cell)
    try:
        land_cases(b, d, cell)
        cache_cases(b, d, cell)
        harvest(b)
        b.goto(f'{d.url}?theme=voidgarden', settle=1.0)
        b.page(BOOT)
        poll(lambda: b.page(f'{DRIVER}.tier()'), 40.0, 0.3)
        the_frame(b, cell)
        found = logs(b)
        check(f'[{cell}] no console error or warning, no CSP violation, no page error',
              found == [], found[:4])
    finally:
        try:
            b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
            b.page('localStorage.clear()')
        finally:
            b.close()
            d.stop()


for cell in CELLS:
    one_cell(cell)
print(f'voidgarden_land.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
