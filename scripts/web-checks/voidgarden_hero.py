# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's hero and particles layers in a browser, both tiers: every material
compiles and the layers draw with no console error or warning, the bisect switches remove their
pixels, each of the nine hero forms and each of the sculpture's ten draws a picture of its own, a
morph trigger of the director changes the hero's readback, the particle counts follow the budget
and the cap, the swarm's neighbour pass on WebGPU gives a known flock step's accelerations (a CPU
reference) and is absent on WebGL2 with no error. A cell is one engine at one tier (as
voidgarden_gpu.py). Pixel checks are statistics of a small readback, never golden images and never
a frame rate.
  every_material_compiles_and_draws       compileAsync of the scene, the layers draw, no console error or warning
  the_bisect_switches_remove_the_pixels   hero and particles off: the region's statistics change
  each_form_draws_its_own_picture         the nine forms (the sculpture's ten) differ from each other
  a_morph_trigger_changes_the_hero        a director morph event: the readback moves
  the_counts_follow_the_budget            a lower budget draws fewer particles, never over the cap
  the_stateful_particles_follow_the_burst a burst moves the offsets away from the form and back
  the_swarm_flocks_on_webgpu              the neighbour pass equals the CPU reference; absent on WebGL2
  the_cues_move_the_hero                  frost desaturates the forms, a changed path hazes the sculpture
  the_frame_is_reproducible               the same seed and time give the same statistics
Usage: python3 voidgarden_hero.py chromium|firefox [webgl2,webgpu]"""
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

# one gpu and the layers on a canvas of the page, with a rig and a director to make frames
SETUP = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLayers } = await import(base + 'layers.js');
  const { createRig } = await import(base + 'motion.js');
  const { createDirector } = await import(base + 'director.js');
  const { particleCounts, flockAcc, SWARM } = await import(base + 'layers/particles.js');
  const hero = await import(base + 'layers/hero.js');
  const canvas = document.createElement('canvas'); canvas.width = 320; canvas.height = 200;
  document.body.append(canvas);
  const gpu = await createGpu({ canvas, forceWebGL: FORCE });
  gpu.size(320, 200);
  const layers = createLayers(gpu, { seed: 5 });
  let rig = createRig({ seed: 5 });
  const target = gpu.target(160, 100);
  const loud = { bass: 0.5, mid: 0.4, high: 0.3, loud: 0.5, l: 0.4, r: 0.4, width: 0.5, flux: 0.2, cent: 0.45, flat: 0.2 };
  const ALL = layers.list.map((l) => l.name);
  const director = createDirector();
  const state = { rig, features: { ...loud }, director: director.state, events: [], t: 0, q: 1 };
  /** One frame: the rig stepped `dt`, the layers updated. The director's state goes with the
   * frame unless `over.bare` (a frame with no director: the hero reads the eased number then);
   * `over.events` are the step's director events; `over.morph` the eased number's toggle. */
  const step = (over = {}) => {
    const dt = over.dt ?? 1 / 30;
    state.t += dt;
    state.rig.step(dt, { motion: over.motion ?? 'full', features: state.features, cues: {},
      director: { ...state.director, morph: over.morph ?? 0 }, events: [], aspect: 1.6 });
    const rig = state.rig;
    const frame = { t: state.t, dt, cam: rig.cam, viewH: over.viewH ?? 400, u: { ...rig.u, colRate: 1, ...(over.u ?? {}) },
      budget: { q: over.q ?? state.q, hero: over.hero ?? over.q ?? state.q }, palette: ['#7fe0ff', '#ff7a4d'], seed: 5 };
    if (!over.bare) frame.director = state.director;
    if (over.events) frame.events = over.events;
    layers.update(frame);
  };
  /** From rest: a fresh rig and clock, as a probe frame has. */
  const reset = () => { state.rig = createRig({ seed: 5 }); state.t = 0; };
  const solo = (...names) => layers.setOff(ALL.filter((n) => !names.includes(n)));
  // the programs of what is on show are compiled before it is read (a first frame may skip them)
  const read = async () => {
    await gpu.renderer.compileAsync(layers.ctx.scene, layers.ctx.camera);
    return layers.readStats(target);
  };
  const logs = () => window.__vg.logs.slice();
  window.__h = { gpu, layers, reset, state, step, solo, read, logs, hero, ALL, particleCounts, flockAcc, SWARM,
    createDirector, target, heroLayer: () => layers.list.find((l) => l.name === 'hero'),
    swarmLayer: () => layers.list.find((l) => l.name === 'particles') };
  return { tier: gpu.tier, layers: ALL };
})()"""

STATE = 'svolder.theme.voidgarden.tier'


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
        b = common.Chromium(size=(1280, 800), args=CHROMIUM_WEBGPU if cell == 'webgpu' else [])
    else:
        b = common.Firefox(size=(1280, 800), extra_prefs=FIREFOX_WEBGPU if cell == 'webgpu' else [])
    b.preload(HOOK)
    return b


def page_logs(b):
    return (b.page('window.__vg.logs') or []) + [f'csp: {c}' for c in b.page('window.__vg.csp') or []]


def js(b, body):
    """Run `body` (an async function body using `h`, the harness) in the page."""
    return b.page('(async () => { const h = window.__h; ' + body + ' })()')


# ------------------------------------------------------------------------------------ the cases
def compiles_and_draws(b, cell, tier):
    got = js(b, """
      // a step first: the land's terrain cache makes its textures in an update, and the shader reads them
      h.step();
      await h.gpu.renderer.compileAsync(h.layers.ctx.scene, h.layers.ctx.camera);
      for (let i = 0; i < 3; i++) h.step();
      h.solo('hero');
      const s = await h.read();
      return { lit: s.lit, luma: s.luma, state: h.layers.list.find((l) => l.name === 'hero').state() };""")
    logs = page_logs(b)
    check(f'[{cell}] the hero draws lit pixels after compileAsync', got['lit'] > 20, got)
    check(f'[{cell}] and no console error or warning', logs == [], logs[:4])
    return got


def the_bisect_switches(b, cell):
    got = js(b, """
      h.layers.build(5);
      const form = (f) => { h.heroLayer().probe().state.plan.to(f); h.heroLayer().probe().state.plan.settle(); };
      form(7);
      h.solo('hero', 'particles');
      h.step({ dt: 0 });
      const both = await h.read();
      h.solo('hero'); const heroOnly = await h.read();
      h.solo(); const none = await h.read();
      return { both, heroOnly, none };""")
    check(f'[{cell}] with the hero off nothing is drawn, with it on it is', got['none']['lit'] < 5 < 20 < got['heroOnly']['lit'], got)
    check(f'[{cell}] the particles layer off removes the swarm\'s pixels', got['both']['luma'] > got['heroOnly']['luma'] * 1.01
          and got['both']['lit'] >= got['heroOnly']['lit'], got)


def each_form_draws_its_own_picture(b, cell):
    js(b, "h.layers.build(5); h.solo('hero', 'particles');")
    got = []
    for f in range(9):
        for sc in (range(10) if f == 8 else [0]):
            s = js(b, f"""
              const plan = h.heroLayer().probe().state.plan;
              plan.to({f}); if ({f} === 8) plan.sculpt({sc});
              h.step({{ dt: 0 }});
              const s = await h.read();
              return {{ lit: s.lit, places: s.places }};""")
            got.append({'f': f, 'sc': sc, **s})
    check(f'[{cell}] all nine forms and all ten sculpture forms draw', len(got) == 18 and all(g['lit'] > 20 for g in got),
          [g for g in got if g['lit'] <= 20])
    pics = {g['places'] for g in got}
    dup = [(g['f'], g['sc']) for g in got if [x['places'] for x in got].count(g['places']) > 1]
    check(f'[{cell}] each draws a picture of its own', len(pics) == len(got), dup)


def a_morph_trigger_changes_the_hero(b, cell):
    run = """
      h.layers.build(5); h.reset();
      h.solo('hero');
      const hl = h.heroLayer();
      h.step({ dt: 0, bare: BARE });
      for (let i = 0; i < 90; i++) h.step({ bare: BARE, morph: MORPH && i >= 1 ? 1 : 0, events: EVENT && i === 1 ? [{ type: 'morph', morph: 1 }] : undefined });
      const s = await h.read();
      return { places: s.places, form: hl.state().form, morphs: hl.state().morphs };"""
    def go(event, morph, bare):
        return js(b, run.replace('EVENT', event).replace('MORPH', morph).replace('BARE', bare))
    control, event = go('false', 'false', 'false'), go('true', 'false', 'false')
    check(f'[{cell}] a director morph event moves the hero against a control at the same time (form, count and picture)',
          event['morphs'] == 1 and control['morphs'] == 0 and event['places'] != control['places'], [control, event])
    ncontrol, number = go('false', 'false', 'true'), go('false', 'true', 'true')
    check(f'[{cell}] the eased number\'s toggle is a trigger when the frame has no director (counted, and the picture differs)',
          number['morphs'] == 1 and ncontrol['morphs'] == 0 and number['places'] != ncontrol['places'], [ncontrol, number])


def the_counts_follow_the_budget(b, cell, tier):
    got = js(b, """
      h.layers.build(5);
      h.solo('hero');
      const plan = h.heroLayer().probe().state.plan;
      plan.to(8); plan.sculpt(3);
      h.step({ dt: 0, q: 1 });
      const full = { counts: h.heroLayer().state().counts, lit: (await h.read()).lit };
      h.step({ dt: 0, hero: 0.3 });
      const low = { counts: h.heroLayer().state().counts, lit: (await h.read()).lit };
      return { full, low, cap: h.gpu.caps.maxParticles };""")
    f, lo, cap = got['full'], got['low'], got['cap']
    check(f'[{cell}] the full budget draws the tier\'s count, never over the cap', f['counts']['sculpt'] <= cap
          and f['counts']['eq'] <= cap and f['counts']['sculpt'] == min(cap, 50000 * (2 if tier == 'webgpu' else 1)), got)
    want = min(cap, int(50000 * (2 if tier == 'webgpu' else 1) * 0.3))
    check(f'[{cell}] a hero share of 0.3 (the budget\'s `hero`) draws 0.3 of the tier\'s count and fewer pixels', lo['counts']['sculpt'] == want
          and lo['counts']['sculpt'] < f['counts']['sculpt'] and lo['lit'] < f['lit'], got)


def the_stateful_particles(b, cell):
    got = js(b, """
      const r = h.gpu.renderer;
      const hl = h.heroLayer(); const p = hl.probe();
      const read = async (buf, n) => Array.from(new Float32Array(await r.getArrayBufferAsync(buf.value)).slice(0, n * 4));
      const worst = (a) => Math.max(...a.map(Math.abs));
      const run = async (form, buffers) => {
        h.layers.build(5); h.solo('hero');
        p.state.plan.to(form); if (form === 8) p.state.plan.sculpt(1);
        h.step({ dt: 0 });
        const rest = await read(buffers.pos, 256);
        const o0 = await read(buffers.off, 256);
        h.step({ events: [{ type: 'section', section: 'drop' }] });
        h.step();
        const burst = await read(buffers.off, 256);
        for (let i = 0; i < 40; i++) h.step({ dt: 1 / 20 });
        const later = await read(buffers.off, 256);
        return { rest: worst(rest), nonzero: rest.filter((v) => v !== 0).length, o0: worst(o0), burst: worst(burst), later: worst(later),
          finite: rest.every(Number.isFinite) && burst.every(Number.isFinite) };
      };
      return { eq: await run(3, p.eqBufs), sculpt: await run(8, p.scBufs) };""")
    for name in ('eq', 'sculpt'):
        g = got[name]
        check(f'[{cell}] the {name} particles are written by their compute pass (finite, not all zero)', g['finite'] and g['nonzero'] > 100
              and g['rest'] > 0.1, g)
        check(f'[{cell}] the {name} offsets are zero at rest, a burst throws them out, and they reform', g['o0'] == 0 and g['burst'] > 0.5
              and g['later'] < g['burst'] * 0.2, g)


def the_swarm(b, cell, tier):
    got = js(b, """
      const r = h.gpu.renderer;
      const hl = h.swarmLayer(); const pr = hl.probe();
      h.layers.build(5);
      h.heroLayer().probe().state.plan.to(7);
      h.solo('hero', 'particles');
      for (let i = 0; i < 12; i++) h.step({ dt: i ? 1 / 30 : 0 });
      const read = async (buf, n) => Array.from(new Float32Array(await r.getArrayBufferAsync(buf.value)).slice(0, n * 4));
      const out = { flock: pr.flock, hasPass: Boolean(pr.passes.flock), state: hl.state() };
      const pos = await read(pr.own.pos, 64); const trail = await read(pr.own.trail[2], 64);
      out.moved = pos.some((v) => v !== 0); out.trail = trail.some((v) => v !== 0);
      if (pr.flock) {
        // a known step: 64 motes, known places and velocities, the pass's accelerations against the CPU's
        const M = 64; const P = new Float32Array(M * 4); const V = new Float32Array(M * 4);
        for (let i = 0; i < M; i++) { P.set([Math.sin(i * 1.7) * 0.5, Math.cos(i * 2.3) * 0.5, Math.sin(i * 0.9) * 0.5, 0], i * 4); V.set([Math.cos(i), 0.1 * i % 1, -Math.sin(i), 0], i * 4); }
        pr.own.pos.value.array.set(P, 0); pr.own.pos.value.needsUpdate = true;
        pr.own.vel.value.array.set(V, 0); pr.own.vel.value.needsUpdate = true;
        pr.k.active.value = M; pr.k.shift.value = 5;
        r.compute(pr.passes.flock);
        const acc = await read(pr.own.acc, M);
        let worst = 0; let big = 0;
        for (let i = 0; i < M; i++) {
          const want = h.flockAcc(i, { pos: P, vel: V, n: M, shift: 5 });
          for (let c = 0; c < 3; c++) { worst = Math.max(worst, Math.abs(acc[i * 4 + c] - want[c])); big = Math.max(big, Math.abs(want[c])); }
        }
        out.flockStep = { worst, big };
      }
      return out;""")
    if tier == 'webgpu':
        check(f'[{cell}] the swarm flocks: the neighbour pass exists on WebGPU', got['flock'] and got['hasPass'], got)
        fs = got.get('flockStep') or {'worst': 9, 'big': 0}
        check(f'[{cell}] a known flock step equals the CPU reference (cohesion, separation, alignment)', fs['big'] > 0.1
              and fs['worst'] < 1e-3 * max(1, fs['big']), got)
    else:
        check(f'[{cell}] on WebGL2 the swarm has no neighbour pass and flies without it', not got['flock'] and not got['hasPass'], got)
    check(f'[{cell}] the swarm and its trails are written by compute', got['moved'] and got['trail'], got)
    logs = page_logs(b)
    check(f'[{cell}] and no console error or warning from the swarm', logs == [], logs[:4])


def the_cues_move_the_hero(b, cell):
    got = js(b, """
      h.layers.build(5);
      h.solo('hero');
      const plan = h.heroLayer().probe().state.plan;
      // the mean saturation and blue tint of the lit pixels (a per-pixel statistic of a readback of 480 x 300: a
      // smaller one piles the sculpture's points up into white)
      const big = h.gpu.target(480, 300);
      const one = async (over) => {
        h.step({ dt: 0, u: over });
        await h.gpu.renderer.compileAsync(h.layers.ctx.scene, h.layers.ctx.camera);
        const px = await h.gpu.read(h.layers.ctx.scene, h.layers.ctx.camera, big);
        let lit = 0; let sat = 0; let tint = 0; let places = 0;
        for (let i = 0; i < px.length; i += 4) {
          const hi = Math.max(px[i], px[i + 1], px[i + 2]); const lo = Math.min(px[i], px[i + 1], px[i + 2]);
          if (hi > 24) { lit++; sat += (hi - lo) / hi; tint += (px[i + 2] - px[i]) / (px[i + 2] + px[i] + 1); places = (Math.imul(places, 31) + i) >>> 0; }
        }
        return { sat: lit ? sat / lit : 0, tint: lit ? tint / lit : 0, lit, places };
      };
      const out = {};
      for (const form of [3, 8]) {
        plan.to(form); if (form === 8) plan.sculpt(1);
        const calm = await one({ frost: 0, haze: 0 });
        const frost = await one({ frost: 1, haze: 0 });
        const haze = await one({ frost: 0, haze: 1 });
        out[form] = { calm, frost, haze };
      }
      return out;""")
    for form, name in ((3, 'EQ torus'), (8, 'sculpture')):
        g = got[str(form)]
        check(f'[{cell}] a muted scene frosts the {name} (its colour moves toward the pale blue-grey)', g['calm']['lit'] > 20 and g['frost']['tint'] > g['calm']['tint'] + 0.03, g)
    check(f'[{cell}] a changed path hazes the sculpture (its picture moves), not the EQ torus', got['8']['haze']['places'] != got['8']['calm']['places']
          and got['3']['haze']['places'] == got['3']['calm']['places'], got)


def the_fade_is_applied_by_the_materials(b, cell):
    got = js(b, """
      h.layers.build(5);
      h.solo('hero', 'particles');
      const out = {};
      for (const form of [1, 3, 7, 8]) {
        h.heroLayer().probe().state.plan.to(form);
        h.step({ dt: 0, u: { fade: 1 } }); const on = await h.read();
        h.step({ dt: 0, u: { fade: 0 } }); const off = await h.read();
        out[form] = { on: on.lit, off: off.lit, offLuma: off.luma };
      }
      h.heroLayer().probe().state.plan.to(3);
      h.layers.ctx.env = { fogD: 0.2 };
      h.step({ dt: 0 }); const fogged = await h.read();
      h.layers.ctx.env = undefined;
      h.step({ dt: 0 }); const plain = await h.read();
      return { out, fog: fogged.places !== plain.places };""")
    for f, g in got['out'].items():
        check(f'[{cell}] form {f}: u.fade 0 leaves nothing lit, 1 draws', g['on'] > 20 and g['off'] == 0 and g['offLuma'] == 0, g)
    check(f'[{cell}] the land\'s ctx.env.fogD is read (the picture moves with it)', got['fog'], got)


def the_frame_is_reproducible(b, cell):
    got = js(b, """
      h.solo('hero', 'particles');
      const probe = async (seed) => { h.layers.build(seed); h.reset(); h.step({ dt: 0 }); return (await h.read()).places; };
      const first = await probe(5);
      // live draws between the two reads: sections, morphs, a budget step and a resize
      h.layers.build(5);
      h.step({ dt: 0 });
      for (let i = 0; i < 60; i++) h.step({ events: i % 20 === 3 ? [{ type: 'morph' }, { type: 'section' }] : [], q: i > 30 ? 0.6 : 1 });
      h.gpu.size(480, 300);
      for (let i = 0; i < 10; i++) h.step();
      const again = await probe(5);
      const other = await probe(6);
      return { first, again, other };""")
    check(f'[{cell}] the same seed and time give the same hero picture after live draws and a resize, another seed differs',
          got['first'] == got['again'] and got['first'] != got['other'], got)


# fixed (seed, seconds) of the stills, and the size they are drawn at
STILLS = [(5, 2), (11, 6), (23, 12)]


def stills(b, cell, folder):
    """With WEB_CHECK_SHOTS=<folder>: the hero and the swarm at three fixed (seed, t), 960x600 on
    the cell's tier, `hero-<seed>-<t>-<cell>.png`, and the sculpture's ten forms in a sheet."""
    js(b, """
      const c = h.gpu.renderer.domElement; c.width = 960; c.height = 600; h.gpu.size(960, 600);
      c.style.position = 'fixed'; c.style.left = '0'; c.style.top = '0'; c.style.zIndex = '99999';
      c.style.width = '960px'; c.style.height = '600px';
      h.solo('hero', 'particles'); return 1;""")
    os.makedirs(folder, exist_ok=True)
    for seed, t in STILLS + [(5, 'forms')]:
        forms = ['sculpture-%d' % k for k in range(10)] if t == 'forms' else [None]
        for form in forms:
            js(b, f"""
              const T = {0 if t == 'forms' else t};
              h.layers.build({seed}); h.reset();
              for (let i = 0; i < Math.round(T * 30); i++) h.step({{ dt: 1 / 30 }});
              const plan = h.heroLayer().probe().state.plan;
              {"plan.to(8); plan.sculpt(%d);" % int(form.split('-')[1]) if form else ""}
              h.step({{ dt: 0 }});
              await h.gpu.renderer.compileAsync(h.layers.ctx.scene, h.layers.ctx.camera);
              h.layers.draw(); h.layers.draw();
              return 1;""")
            name = f'hero-{seed}-{t}-{cell}.png' if not form else f'hero-{form}-{cell}.png'
            with open(os.path.join(folder, name), 'wb') as f:
                f.write(b.shot())


def one_cell(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(cell)
    try:
        b.goto(f'{d.url}?theme=paper', settle=1.0)
        b.page(BOOT)
        info = b.page(SETUP.replace('FORCE', 'false'))
        want = 'webgpu' if cell == 'webgpu' else 'webgl2'
        check(f'[{cell}] the tier is {want}', info['tier'] == want, info)
        compiles_and_draws(b, cell, want)
        the_bisect_switches(b, cell)
        each_form_draws_its_own_picture(b, cell)
        a_morph_trigger_changes_the_hero(b, cell)
        the_counts_follow_the_budget(b, cell, want)
        the_stateful_particles(b, cell)
        the_swarm(b, cell, want)
        the_cues_move_the_hero(b, cell)
        the_fade_is_applied_by_the_materials(b, cell)
        the_frame_is_reproducible(b, cell)
        found = page_logs(b)
        check(f'[{cell}] no console error or warning, no CSP violation, no page error', found == [], found[:4])
        if os.environ.get('WEB_CHECK_SHOTS'):
            stills(b, cell, os.environ['WEB_CHECK_SHOTS'])
    finally:
        try:
            b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
            b.page('localStorage.clear()')
        finally:
            b.close()
            d.stop()


for cell in CELLS:
    one_cell(cell)
print(f'voidgarden_hero.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
