# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's cost in a browser: the post chain draws the scene once per frame, and the
bisect switches (`?vg-debug=no<layer>`, `nopost`) do what they say. The counts are draws of the
scene's own objects (each object's `onBeforeRender`) and renders of the renderer, never a frame rate
or a GPU time. A cell is one engine at one tier (as voidgarden_fx.py).
  the_scene_is_drawn_once_per_frame     every scene object draws once in a draw of the chain, and once in each frame of a live loop
  a_second_draw_still_shows_the_scene   two draws in one task (a probe) each draw the scene again
  a_layer_switch_draws_nothing_of_it    each `no<layer>` of the address leaves that layer's objects undrawn, the others drawn
  nopost_draws_no_post_pass             the chain's renders are absent: a draw costs a plain render of the scene
Usage: python3 voidgarden_perf.py chromium|firefox [webgl2,webgpu]"""
import os
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

CASES = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLayers } = await import(base + 'layers.js');
  const { debugOf, withSwitches, LAYER_SWITCHES } = await import(base + 'debug.js');
  const { ORDER } = await import(base + 'layers.js');
  const W = 160, H = 100;
  const canvas = document.createElement('canvas'); canvas.width = W; canvas.height = H; document.body.append(canvas);
  const gpu = await createGpu({ canvas }); gpu.size(W, H);
  const out = { tier: gpu.tier };
  const baseU = { time: 3, gt: 3, bass: 0.5, mid: 0.4, high: 0.3, l: .4, r: .4, width: .6, loud: .6, flux: .2, cent: .4, flat: .2, peak: 0, shock: 1, morph: 0, frost: 0, haze: 0, cross: 0, grain: 0, static: 0, tear: 0, tearY: 0, fade: 1, seed: 7, calm: 0, colRate: 1, volume: .5 };
  const cam = { x: 0, y: 7, z: 0, lookX: 26, lookY: 4, lookZ: -34, fov: 55, roll: 0, aspect: W / H, dist: 1 };
  let n = 0;
  const frame = () => { n++; return { t: 3 + n * 0.05, dt: 0.05, cam, u: { ...baseU, time: 3 + n * 0.05, gt: 3 + n * 0.05 }, budget: { grit: true, bloom2: true, q: 1 },
    palette: ['#7fe0ff', '#ff7a4d'], seed: 7, section: 'build',
    director: { groove: { kick: 0.5, snare: 0.2, hat: 0.1, energy: 0.7, bar: 0.2, down: 0.1 }, beat: 9, bpm: 120, hits: [3, 1, 0], section: 'build' } }; };
  // the draws of each layer's own objects, counted where the renderer is about to draw each one
  const per = {};
  const counted = (layers) => {
    const draws = {};
    for (const layer of layers.list) {
      draws[layer.name] = 0;
      layer.object.traverse((o) => { const was = o.onBeforeRender; o.onBeforeRender = function (...a) { draws[layer.name]++; per[o.uuid] = (per[o.uuid] || 0) + 1; return was.apply(this, a); }; });
    }
    return draws;
  };
  const zero = (d) => { for (const k of Object.keys(d)) d[k] = 0; for (const k of Object.keys(per)) delete per[k]; };
  const peak = () => Math.max(0, ...Object.values(per));
  const renders = () => gpu.renderer.info.render.calls;
  const rest = () => new Promise((r) => setTimeout(r, 0));
  const make = (search) => withSwitches(createLayers, debugOf(search))(gpu, { seed: 7 });
  const loop = (d) => new Promise((done) => {
    const frames = []; let i = 0;
    const tick = () => { zero(d); layers.update(frame()); const r = renders(); layers.draw(); frames.push({ ...d, peak: peak(), renders: renders() - r }); if (++i < 5) requestAnimationFrame(tick); else done(frames); };
    requestAnimationFrame(tick);
  });
  let layers = make('');
  let d = counted(layers);
  // an update first: the land's terrain cache makes its textures there, and the shaders read them
  layers.update(frame());
  await gpu.renderer.compileAsync(layers.ctx.scene, layers.ctx.camera);
  // the first draw of the chain runs every pass: the later draws must run as many
  layers.update(frame()); const f0 = renders(); layers.draw(); out.firstRenders = renders() - f0; await rest();
  // one draw of the chain
  zero(d); layers.update(frame()); const r0 = renders(); layers.draw(); out.chainRenders = renders() - r0; out.once = { ...d }; out.oncePeak = peak();
  // two draws in one task, no update between: each draws the scene
  zero(d); const t0 = renders(); layers.draw(); const t1 = renders(); layers.draw(); out.twiceRenders = [t1 - t0, renders() - t1];
  out.twice = { ...d }; out.twicePeak = peak();
  // a live loop: every frame draws each object once
  out.live = await loop(d);
  layers.dispose();
  // the layers' switches, by the address's own words: two sets that between them name every layer
  // (built from the switches' own table, so a dead or new switch fails here)
  const named = Object.entries(LAYER_SWITCHES).filter(([word]) => word !== 'nopost').map(([, layer]) => layer);
  out.switchLayers = named; out.order = ORDER.filter((n) => n !== 'post');
  const sets = [named.slice(0, Math.ceil(named.length / 2)), named.slice(Math.ceil(named.length / 2))];
  const words = (set) => Object.entries(LAYER_SWITCHES).filter(([word, layer]) => set.includes(layer) && word !== 'nopost').map(([word]) => word);
  out.switches = [];
  for (const off of sets) {
    layers = make('?vg-debug=' + words(off).join(',')); d = counted(layers);
    layers.update(frame()); layers.draw(); await rest(); zero(d);
    layers.update(frame()); layers.draw();
    out.switches.push({ off, draws: { ...d } });
    layers.dispose();
  }
  // nopost: no pass of the chain, one render of the scene
  layers = make('?vg-debug=nopost'); d = counted(layers);
  layers.update(frame()); layers.draw(); await rest(); zero(d);
  layers.update(frame()); const r1 = renders(); layers.draw();
  out.nopost = { renders: renders() - r1, draws: { ...d } };
  const r2 = renders(); gpu.render(layers.ctx.scene, layers.ctx.camera); out.plainRenders = renders() - r2;
  layers.dispose();
  out.csp = window.__vg.csp;
  return out;
})()"""


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(cell):
    if engine == 'chromium':
        b = common.Chromium(size=(640, 400), args=CHROMIUM_WEBGPU if cell == 'webgpu' else [])
    else:
        b = common.Firefox(size=(640, 400), extra_prefs=FIREFOX_WEBGPU if cell == 'webgpu' else [])
    b.preload(HOOK)
    return b


def run_cases(b, limit=900.0):
    """The page's cases, started in the page and polled (see voidgarden_fx.py)."""
    b.page("(() => { window.__pfDone = null; window.__pfErr = null; (" + CASES + ").then("
           "(r) => { window.__pfDone = r; }, (e) => { window.__pfErr = String((e && e.stack) || e); }); return 1; })()")
    end = time.time() + limit
    while time.time() < end:
        err = b.page('window.__pfErr')
        if err:
            raise RuntimeError(err)
        got = b.page('window.__pfDone')
        if got:
            return got
        time.sleep(1.0)
    raise RuntimeError(f'the cases did not finish in {limit:.0f} s')


def cases(b, cell):
    got = run_cases(b)
    want = 'webgpu' if cell == 'webgpu' else 'webgl2'
    check(f'[{cell}] the tier is {want}', got['tier'] == want, got['tier'])
    once, twice, live = got['once'], got['twice'], got['live']
    drawn = [k for k, v in once.items() if v > 0]
    check(f'[{cell}] the scene is drawn once per frame: each object draws once in a draw of the chain',
          len(drawn) >= 4 and got['oncePeak'] == 1, {'draws': once, 'peak': got['oncePeak']})
    check(f'[{cell}] every frame of a live loop draws each object once, and runs every pass of the chain',
          len(live) == 5 and all(f['peak'] == 1 and f['renders'] == got['chainRenders'] for f in live),
          {'live': live, 'chainRenders': got['chainRenders']})
    check(f'[{cell}] a later draw runs as many passes as the first (no pass is skipped for good)',
          got['chainRenders'] == got['firstRenders'] and got['chainRenders'] > 8, [got['firstRenders'], got['chainRenders']])
    check(f'[{cell}] two draws in one task each draw the scene again and run the whole chain again',
          got['twicePeak'] == 2 and all(twice[k] == 2 * once[k] for k in once)
          and got['twiceRenders'] == [got['chainRenders']] * 2,
          {'once': once, 'twice': twice, 'renders': got['twiceRenders'], 'chainRenders': got['chainRenders']})
    check(f'[{cell}] the switches name every layer but the post chain, each once',
          sorted(got['switchLayers']) == sorted(got['order']), {'switches': got['switchLayers'], 'order': got['order']})
    sw = got['switches']
    check(f'[{cell}] a layer\'s switch draws nothing of it and leaves the others drawn',
          all(all(s['draws'][k] == 0 for k in s['off'])
              and any(x > 0 for n, x in s['draws'].items() if n not in s['off']) for s in sw), sw)
    npost = got['nopost']
    check(f'[{cell}] nopost draws no pass of the chain: the scene rendered as a plain render is, the objects drawn',
          npost['renders'] == got['plainRenders'] and sum(npost['draws'].values()) > 0
          and got['chainRenders'] > got['plainRenders'], {
              'nopost': npost, 'plain': got['plainRenders'], 'chainRenders': got['chainRenders']})


def one_cell(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(cell)
    try:
        b.goto(f'{d.url}?theme=paper', settle=1.0)
        b.page(BOOT)
        cases(b, cell)
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
print(f'voidgarden_perf.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
