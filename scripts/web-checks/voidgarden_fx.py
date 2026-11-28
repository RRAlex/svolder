# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's fx layers in a browser: the beacons (compute-written instance data, placed
on a terrain handed in), eruptions and sparks in compute, and the post chain on a RenderPipeline.
A cell is one engine at one tier (Chromium: stock is WebGL2, the four SwiftShader flags give WebGPU;
Firefox: stock is WebGL2, the `dom.webgpu.enabled` pref gives WebGPU). The pixel cases are statistics
of the canvas read in the draw's own task, never golden images and never a frame rate.
  the_materials_compile                 `compileAsync` of the scene and a draw through the chain: no error or warning
  the_beacons_draw_and_the_switch_clears  beacons on: lit pixels in their region; off: fewer
  the_beacons_move_with_the_groove      a kick of 0 against 1 changes the readback
  each_post_effect_changes_the_picture  strength 0 against 1 of every effect changes the readback; one render twice is equal
  the_budget_drops_grit_then_bloom2     the grit step and the bloom's second octave change the readback
  the_haze_follows_the_changed_path     u.haze 0 against 1: the strength and the readback
  an_eruption_lands_on_the_terrain      landed particles within 0.05 of the height the scene lands them on (its heights table)
  sparks_leave_on_a_hit                 a kick event puts live sparks in the pool
  the_accents_are_gated                 a drop's entry fires an accent in Full and none in Calm
Usage: python3 voidgarden_fx.py chromium|firefox [webgl2,webgpu]"""
import json
import os
import sys
import time

import common

engine = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
CELLS = (sys.argv[2] if len(sys.argv) > 2 else 'webgl2,webgpu').split(',')
fails = []
passed = 0
t0 = time.time()
SHOTS = os.environ.get('WEB_CHECK_SHOTS')

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


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


CASES = """(async (shotNames) => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLayers } = await import(base + 'layers.js');
  const { EFFECTS } = await import(base + 'tsl/post.js');
  const W = 640, H = 400;
  const out = {};
  const canvas = document.createElement('canvas'); canvas.width = W; canvas.height = H; document.body.append(canvas);
  const gpu = await createGpu({ canvas }); gpu.size(W, H);
  const layers = createLayers(gpu, { seed: 7 });
  out.tier = gpu.tier;
  // the land layer's build sets the context's terrain: the ground an eruption lands on is that one
  layers.build(7); const terrain = layers.ctx.terrain;
  const bl = layers.list.find((l) => l.name === 'beacons');
  const post = layers.list.find((l) => l.name === 'post');
  const baseU = { time: 3, gt: 3, bass: 0.5, mid: 0.4, high: 0.3, l: .4, r: .4, width: .6, loud: .6, flux: .2, cent: .4, flat: .2, peak: 0, shock: 1, morph: 0, frost: 0, haze: 0, cross: 0, grain: 0, static: 0, tear: 0, tearY: 0, fade: 1, seed: 7, calm: 0, colRate: 1, volume: .5 };
  const cam = (z = 0) => ({ x: 0, y: 7, z, lookX: 26, lookY: 4, lookZ: z - 34, fov: 55, roll: 0, aspect: W / H, dist: 1 });
  const budget = { grit: true, bloom2: true, q: 1 };
  const director = (kick) => ({ groove: { kick, snare: 0.2, hat: 0.1, energy: 0.7, bar: 0.2, down: 0.1 }, beat: 9, bpm: 120, hits: [3, 1, 0], section: 'build' });
  const frame = (o = {}) => ({ t: 3, dt: 0.05, cam: cam(), u: { ...baseU, ...(o.u || {}) }, budget, palette: ['#7fe0ff', '#ff7a4d'], seed: 7, director: director(0.5), section: 'build', ...o, u: { ...baseU, ...(o.u || {}) } });
  const grab = () => { const c = document.createElement('canvas'); c.width = W; c.height = H; const x = c.getContext('2d'); x.drawImage(canvas, 0, 0); return [c, x.getImageData(0, 0, W, H).data]; };
  const lit = (px, x0 = 0, x1 = W) => { let n = 0; for (let y = 0; y < H; y++) for (let x = x0; x < x1; x++) { const i = (y * W + x) * 4; if (px[i] + px[i + 1] + px[i + 2] > 120) n++; } return n; };
  const diff = (a, b) => { let s = 0; for (let i = 0; i < a.length; i += 4) s += Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1]) + Math.abs(a[i + 2] - b[i + 2]); return s / (W * H * 3); };
  // a render of one frame with `set` applied to the chain's strengths after the frame's update
  const shot = async (f, set = null, name = null) => {
    layers.update(f); if (set) set(); layers.draw(); const g = grab();
    await new Promise((r) => setTimeout(r, 0)); return g[1];
  };
  const warm = async () => { for (let i = 0; i < 3; i++) { await shot(frame({ t: 3 + i * .05 })); await new Promise((r) => setTimeout(r, 150)); } };
  // the materials compile, the chain draws (an update first: the land's terrain cache makes its
  // textures there, and the land's shader reads them)
  layers.update(frame({ t: 3 }));
  await gpu.renderer.compileAsync(layers.ctx.scene, layers.ctx.camera);
  await warm();
  out.compiled = true;
  // beacons and the switch
  const m = bl.state.members[0];
  const aim = { x: m.x - 20, y: m.y + 6, z: m.z + 30, lookX: m.x, lookY: m.y + 4, lookZ: m.z, fov: 55, roll: 0, aspect: W / H, dist: 1 };
  const on = await shot(frame({ cam: aim }), null, 'valley');
  layers.setOff(['beacons']); const off = await shot(frame({ cam: aim })); layers.setOff([]);
  out.beacons = { on: lit(on), off: lit(off), diff: diff(on, off) };
  // the groove
  const k0 = await shot(frame({ director: director(0) })); const k1 = await shot(frame({ director: director(1) }));
  out.groove = { diff: diff(k0, k1) };
  // each effect, 0 against 1; a first frame for the datamosh's previous picture
  out.effects = {};
  const effFrame = (z) => frame({ cam: cam(z), dt: 0 });
  const twice = async (name, v) => { await shot(effFrame(0)); return shot(effFrame(-3), () => { post.post.strengths[name].value = v; }); };
  const base0 = {};
  for (const name of EFFECTS) {
    const zero = (n) => { for (const e of EFFECTS) post.post.strengths[e].value = (e === n ? 1 : (e === 'bloom' || e === 'grain' ? 0 : 0)); };
    await shot(effFrame(0), () => zero(null));
    const a = await shot(effFrame(-3), () => zero(null));
    await shot(effFrame(0), () => zero(null));
    const a2 = await shot(effFrame(-3), () => zero(null));
    await shot(effFrame(0), () => zero(name));
    const b = await shot(effFrame(-3), () => zero(name));
    out.effects[name] = { diff: diff(a, b), again: diff(a, a2) };
  }
  // the budget
  const full = await shot(frame()); 
  const noGrit = await shot(frame({ budget: { grit: false, bloom2: true, q: 1 } }));
  const noBloom2 = await shot(frame({ budget: { grit: false, bloom2: false, q: 1 } }));
  out.budget = { grit: diff(full, noGrit), bloom2: diff(noGrit, noBloom2), gritLevel: post.levels().strengths.grain };
  // the haze
  const h0 = await shot(frame({ u: { haze: 0 } })); const l0 = post.levels().strengths.haze;
  const h1 = await shot(frame({ u: { haze: 1 } })); const l1 = post.levels().strengths.haze;
  out.haze = { l0, l1, diff: diff(h0, h1) };
  // an eruption on the terrain
  const rd = async (b) => new Float32Array(await gpu.renderer.getArrayBufferAsync(b.value));
  // an eruption lands where the heights table puts it (the terrain read on a 1-unit grid, bilinear, as
  // the compute pass reads it): the check compares with that, within 0.05, not with the terrain between
  // the cells, which the table is up to a few tenths off by design on the real land
  const { BEACON } = await import(base + 'layers/beacons.js');
  const patch = { x: 10, z: -40 };
  // the table is made here from the terrain, not by the scene's own helper: a ground the scene makes
  // flat or off-centre is then a difference
  const nT = BEACON.tableCells, table = new Float32Array(nT * nT);
  for (let iz = 0; iz < nT; iz++) for (let ix = 0; ix < nT; ix++) {
    table[iz * nT + ix] = terrain.height(patch.x - (nT * BEACON.tableStep) / 2 + ix * BEACON.tableStep,
      patch.z - (nT * BEACON.tableStep) / 2 + iz * BEACON.tableStep); }
  const landsAt = (x, z) => { const n = BEACON.tableCells;
    const gx = Math.min(n - 1.001, Math.max(0, (x - patch.x) / BEACON.tableStep + n / 2));
    const gz = Math.min(n - 1.001, Math.max(0, (z - patch.z) / BEACON.tableStep + n / 2));
    const ix = Math.floor(gx), iz = Math.floor(gz), fx = gx - ix, fz = gz - iz;
    const h = (a, b) => table[(iz + b) * n + ix + a];
    return (h(0, 0) * (1 - fx) + h(1, 0) * fx) * (1 - fz) + (h(0, 1) * (1 - fx) + h(1, 1) * fx) * fz; };
  bl.erupt(patch.x, patch.z);
  for (let i = 0; i < 90; i++) layers.update(frame({ dt: 0.05 }));
  const pos = await rd(bl.buffers.ePos), vel = await rd(bl.buffers.eVel);
  let landed = 0, worst = 0;
  for (let i = 0; i < pos.length / 4; i++) if (vel[i * 4 + 3] === 1) { landed++; worst = Math.max(worst, Math.abs(pos[i * 4 + 1] - landsAt(pos[i * 4], pos[i * 4 + 2]))); }
  out.erupt = { landed, worst };
  // sparks on a kick
  layers.update(frame({ dt: 0.05, events: [{ type: 'kick', amp: 1 }] })); layers.update(frame({ dt: 0.05 }));
  const sp = await rd(bl.buffers.sPos);
  let live = 0; for (let i = 0; i < sp.length / 4; i++) if (sp[i * 4 + 3] > 0) live++;
  out.sparks = { live };
  // the accents behind the gate
  const levelsSum = () => { const a = post.accents.levels(); return a.barcode + a.mosh + a.vhs + a.sort + a.flare; };
  layers.update(frame({ t: 20, dt: 0.1, events: [{ type: 'section', section: 'drop' }], section: 'drop' }));
  layers.update(frame({ t: 20.1, dt: 0.1, section: 'drop' }));
  out.accentFull = levelsSum();
  for (let i = 0; i < 6; i++) layers.update(frame({ t: 21 + i * 3, dt: 3, section: 'drop' }));
  layers.update(frame({ t: 40, dt: 0.1, events: [{ type: 'section', section: 'drop' }], section: 'drop', u: { calm: 1 } }));
  layers.update(frame({ t: 40.1, dt: 0.1, section: 'drop', u: { calm: 1 } }));
  out.accentCalm = levelsSum();
  // the spectral light: a cluster of that mode is as bright as the slice of the spectrum is loud
  const sm = bl.state.members.find((x) => x.lm === 2 && x.kind !== 4);
  if (sm) {
    const saim = { x: sm.x - 12, y: sm.y + 5, z: sm.z + 18, lookX: sm.x, lookY: sm.y + 3, lookZ: sm.z, fov: 55, roll: 0, aspect: W / H, dist: 1 };
    const spectrum = (v) => layers.ctx.audio.set({ bins: new Float32Array(64).fill(v) });
    spectrum(0); const quiet = await shot(frame({ cam: saim, dt: 0 }));
    spectrum(1); const loud = await shot(frame({ cam: saim, dt: 0 }));
    spectrum(0);
    out.spectral = { diff: diff(quiet, loud) };
  } else out.spectral = null;
  // a seed's start reads nothing the live frames left
  const target = gpu.target(160, 100);
  layers.build(7); layers.update(frame({ dt: 0 })); const r1 = await layers.readStats(target);
  for (let i = 0; i < 12; i++) layers.update(frame({ dt: 0.1, events: [{ type: 'kick' }, { type: 'peak', flash: true }] }));
  layers.build(7); layers.update(frame({ dt: 0 })); const r2 = await layers.readStats(target);
  out.repro = { same: JSON.stringify(r1) === JSON.stringify(r2), lit: r1.lit };
  // the night ground through the chain: the noise stays about one 8-bit step wide
  layers.setOff(layers.list.map((l) => l.name).filter((n) => n !== 'post'));
  await shot(frame({ dt: 0 }));
  const night = await shot(frame({ dt: 0 }));
  const chan = []; for (let y = 120; y < 280; y++) for (let x = 200; x < 440; x++) chan.push(night[(y * W + x) * 4 + 1]);
  chan.sort((a, b) => a - b);
  const med = chan[chan.length >> 1];
  out.night = { median: med, p01: chan[Math.floor(chan.length * 0.01)] - med, p99: chan[Math.floor(chan.length * 0.99)] - med };
  layers.setOff([]);
  // the fade is applied once, by each material: with the chain on, half a fade reads about half
  const mean = (px) => { let t = 0; for (let i = 0; i < px.length; i += 4) t += px[i] + px[i + 1] + px[i + 2]; return t / (W * H * 3); };
  const f1 = await shot(frame({ cam: aim, dt: 0, u: { fade: 1 } }));
  const f5 = await shot(frame({ cam: aim, dt: 0, u: { fade: 0.5 } }));
  out.fade = { full: mean(f1), half: mean(f5) };
  out.csp = window.__vg.csp;
  for (const n of shotNames) { /* stills */ }
  return out;
})"""

STILLS = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLayers } = await import(base + 'layers.js');
  const W = 1280, H = 800;
  const canvas = document.createElement('canvas'); canvas.width = W; canvas.height = H; document.body.append(canvas);
  const gpu = await createGpu({ canvas }); gpu.size(W, H);
  const layers = createLayers(gpu, { seed: 7 });
  const terrain = { height: (x, z) => Math.max(0, 3 + 0.08 * x + 1.5 * Math.sin(z / 6) + 0.8 * Math.cos(x / 5) - 2), riverX: (z) => 4 * Math.sin(z / 40) };
  layers.ctx.terrain = terrain; layers.build(7);
  const bl = layers.list.find((l) => l.name === 'beacons');
  const u = { time: 3, gt: 3, bass: 0.5, mid: 0.4, high: 0.3, l: .4, r: .4, width: .6, loud: .6, flux: .2, cent: .4, flat: .2, peak: 0, shock: 1, morph: 0, frost: 0, haze: 0, cross: 0, grain: 0, static: 0, tear: 0, tearY: 0, fade: 1, seed: 7, calm: 0, colRate: 1, volume: .5 };
  const out = {};
  const sections = [['intro', 0.1, 0.3, -20], ['build', 0.4, 0.6, -150], ['drop', 0.9, 0.9, -320]];
  for (const [section, kick, energy, z] of sections) {
    const cam = { x: 0, y: 7, z, lookX: 14, lookY: 4, lookZ: z - 34, fov: 55, roll: 0, aspect: W / H, dist: 1 };
    const events = section === 'drop' ? [{ type: 'section', section: 'drop' }, { type: 'kick', amp: 1 }] : [];
    for (let i = 0; i < 12; i++) {
      layers.update({ t: 3 + i * .05, dt: 0.15, cam, u: { ...u, time: 3 + i * .15, gt: 3 + i * .15, peak: i === 0 && section === 'drop' ? 1 : 0 }, budget: { grit: true, bloom2: true, q: 1 }, palette: ['#7fe0ff', '#ff7a4d'], seed: 7, section,
        director: { groove: { kick, snare: 0.3, hat: 0.2, energy, bar: 0.3, down: 0.2 }, beat: 20 + i * 0.1, bpm: 120, hits: [3, 1, 0], section }, events: i === 0 ? events : [] });
      layers.draw();
      if (i % 4 === 3 && i < 11) await new Promise((r) => setTimeout(r, 250));
    }
    const c = document.createElement('canvas'); c.width = W; c.height = H; c.getContext('2d').drawImage(canvas, 0, 0);
    out[section] = c.toDataURL('image/png');
  }
  return out;
})()"""


def make(cell):
    if engine == 'chromium':
        b = common.Chromium(size=(1280, 800), args=CHROMIUM_WEBGPU if cell == 'webgpu' else [])
    else:
        b = common.Firefox(size=(1280, 800), extra_prefs=FIREFOX_WEBGPU if cell == 'webgpu' else [])
    b.preload(HOOK)
    return b


def run_cases(b, limit=900.0):
    """The page's cases, started in the page and polled: a software renderer draws the merged scene
    in seconds a frame, and one page call is cut off by the harness after 60 s (each shot gives the
    page's task queue a turn, so the polls are answered)."""
    b.page("(() => { window.__fxDone = null; window.__fxErr = null; (" + CASES + ")([]).then("
           "(r) => { window.__fxDone = r; }, (e) => { window.__fxErr = String((e && e.stack) || e); }); return 1; })()")
    end = time.time() + limit
    while time.time() < end:
        err = b.page('window.__fxErr')
        if err:
            raise RuntimeError(err)
        got = b.page('window.__fxDone')
        if got:
            return got
        time.sleep(1.0)
    raise RuntimeError(f'the cases did not finish in {limit:.0f} s')


def cases(b, cell):
    got = run_cases(b)
    want = 'webgpu' if cell == 'webgpu' else 'webgl2'
    check(f'[{cell}] the tier is {want}', got['tier'] == want, got['tier'])
    check(f'[{cell}] the materials compile and the chain draws', got['compiled'])
    bc = got['beacons']
    check(f'[{cell}] the beacons draw pixels, and the bisect switch removes them',
          bc['on'] > bc['off'] + 200 and bc['diff'] > 0.05, bc)
    check(f'[{cell}] the beacons move with the groove', got['groove']['diff'] > 0.02, got['groove'])
    for name, e in got['effects'].items():
        check(f'[{cell}] post effect {name}: strength 1 changes the picture, a repeat is far closer',
              e['diff'] > 0.01 and e['again'] < e['diff'] / 2, e)
    bg = got['budget']
    check(f'[{cell}] the grit step and then the bloom\'s second octave each change the picture',
          bg['grit'] > 0.01 and bg['bloom2'] > 0.005 and bg['gritLevel'] == 1, bg)
    hz = got['haze']
    check(f'[{cell}] the heat haze follows the changed path', hz['l0'] == 0 and hz['l1'] == 1 and hz['diff'] > 0.01, hz)
    er = got['erupt']
    check(f'[{cell}] eruption particles land on the terrain within 0.05', er['landed'] > 100 and er['worst'] <= 0.05, er)
    check(f'[{cell}] spectral light follows the spectrum (a quiet slice dims it)',
          got['spectral'] is not None and got['spectral']['diff'] > 0.02, got['spectral'])
    check(f'[{cell}] a seed\'s start reads nothing the live frames left', got['repro']['same'] and got['repro']['lit'] > 0, got['repro'])
    nt = got['night']
    print('night ground', nt)
    check(f'[{cell}] grain and dither on the night ground stay within about 4/255',
          nt['p99'] <= 5 and nt['p01'] >= -5 and nt['median'] > 0, nt)
    fd = got['fade']
    check(f'[{cell}] the fade is applied once: half a fade reads about half, not a quarter, with the chain on',
          0.45 < fd['half'] / fd['full'] < 0.85, fd)
    check(f'[{cell}] a kick puts sparks in the pool', got['sparks']['live'] > 0, got['sparks'])
    check(f'[{cell}] a drop\'s entry fires an accent in Full and none in Calm',
          got['accentFull'] > 0 and got['accentCalm'] == 0, {k: got[k] for k in ('accentFull', 'accentCalm')})


def stills(b, cell):
    shots = b.page(STILLS)
    import base64
    os.makedirs(SHOTS, exist_ok=True)
    for name, url in shots.items():
        with open(f'{SHOTS}/fx-{engine}-{cell}-{name}.png', 'wb') as f:
            f.write(base64.b64decode(url.split(',')[1]))


def one_cell(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(cell)
    try:
        b.goto(f'{d.url}?theme=paper', settle=1.0)
        b.page(BOOT)
        cases(b, cell)
        if SHOTS:
            stills(b, cell)
        found = list(b.page('window.__vg.logs') or []) + [f'csp: {c}' for c in b.page('window.__vg.csp') or []]
        errs = b.errors if engine == 'chromium' else b.collect_errors()
        found += [e for e in errs if 'favicon' not in e]
        check(f'[{cell}] no console error or warning, no CSP violation, no page error', found == [], found[:4])
    finally:
        try:
            b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
            b.page('localStorage.clear()')
        finally:
            b.close()
            d.stop()


for cell in CELLS:
    one_cell(cell)
print(f'voidgarden_fx.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
