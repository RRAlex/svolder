# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's GPU foundation in a browser: the tiers (the renderer's tier as the cell
forces it, the WebGL2 floor asked for by the address), a frame that is the same for the same seed
and time and differs for another seed, the pixels of the live scene, the frame budget stepping
down in the plan's order under a forced draw time and back up, the analysis held only while the
scene shows, the still and the quiet line when neither WebGPU nor WebGL2 gives a context (the keys
still write), the colour of the palette through the colour path (within 2/255), a neighbour read
over a storage buffer, the sprites' per-instance sizes by both techniques, and no console error or
warning and no CSP violation in any cell. A cell is one engine at one tier (Chromium: stock is
WebGL2, the four SwiftShader flags give WebGPU; Firefox: stock is WebGL2, the `dom.webgpu.enabled`
pref gives WebGPU; `none` turns both off). Pixel checks are statistics of a small readback and
comparisons of known colours, never golden images and never a frame rate; the budget is driven by
`driver.forceDrawMs`. Wait for results by a DOM flag or a poll over the browser's own channel.
  the_tier_is_as_forced                      `driver.tier()`, the capability bag, the readout words (and `?vg-gpu=webgl2` on WebGPU)
  the_probe_keeps_the_faster_tier            WebGPU cell: `?vg-probe=slow-<tier>` (that tier draws every sixth frame):
                                             the other is the scene's, kept with its key, read back unmeasured
  the_palette_is_the_covers                  the fake host's cover drawn 32x32: the scene's pair is palette.js's for it
  the_audio_is_read_at_k_not_0               bands, spectrum and wave uploaded twice, read in a render at k != 0: the second upload
  a_neighbour_read_follows_the_uploads       a compute-written buffer follows two uploads; a CPU-written one is refused
  the_frame_is_reproducible_and_seeded       `driver.frame(7, 2)` twice the same statistics, another seed differs
  the_pixels_show_the_scene                  `driver.pixels()` has lit pixels of the live scene
  the_budget_steps_down_in_order_and_back    forced 28 ms: the backdrop's steps in order; forced 2 ms: the last step returns
  the_hero_stays_whole_over_the_scaled_backdrop  the frame at (7, 20) on the canvas at the top step and at the
                                             backdrop's last step (share 0.5): the hero's box (`driver.heroAt`) as bright,
                                             every part of the frame lit as much; again after a resize
  the_analysis_is_held_only_while_shown      one holder shown; none 2 s after the page is hidden; one again
  no_gpu_shows_the_still_and_the_keys_write  the still SVG, the quiet line, steps still sent, the volume key writes
  the_palette_reads_as_its_hex               three palette hexes through `lib.colour`, within 2/255; raw floats are not
  a_neighbour_read_needs_the_pbo_path        `lib.neighbours` gives the neighbour sums; a plain read is the spike's WebGL2 fault
  sprites_take_a_size_per_instance           px and world sprites of 4, 8, 16, 32 give squares of those sizes
  no_console_error_warning_or_csp            every page of the cell: none
Usage: python3 voidgarden_gpu.py chromium|firefox [webgl2,webgpu,none]"""
import json
import sys
import time

import common

engine = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
CELLS = (sys.argv[2] if len(sys.argv) > 2 else 'webgl2,webgpu,none').split(',')
fails = []
passed = 0
t0 = time.time()

CHROMIUM_WEBGPU = ['--enable-unsafe-webgpu', '--enable-features=Vulkan',
                   '--use-webgpu-adapter=swiftshader', '--use-angle=swiftshader']
CHROMIUM_NONE = ['--disable-3d-apis']
FIREFOX_WEBGPU = ['user_pref("dom.webgpu.enabled", true);']
FIREFOX_NONE = ['user_pref("webgl.disabled", true);', 'user_pref("dom.webgpu.enabled", false);']

# console warnings and errors, CSP violations and page errors of every page, from its first script
HOOK = """window.__vg = { logs: [], csp: [] };
for (const k of ['warn', 'error']) { const o = console[k].bind(console);
  console[k] = (...a) => { window.__vg.logs.push(k + ': ' + a.map(String).join(' ').slice(0, 300)); o(...a); }; }
addEventListener('securitypolicyviolation', (e) => window.__vg.csp.push(e.violatedDirective + ' ' + e.blockedURI));
addEventListener('error', (e) => window.__vg.logs.push('error: ' + e.message));
addEventListener('unhandledrejection', (e) => window.__vg.logs.push('rejection: ' + e.reason));"""

BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
DRIVER = '__core.theme.driver'
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
REC = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push([String(a[0]), String((a[1] || {}).body || '')]);
    return f(...a); }; return 1; })()"""
STATE = 'svolder.theme.voidgarden.tier'


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
        args = {'webgpu': CHROMIUM_WEBGPU, 'none': CHROMIUM_NONE}.get(cell, [])
        b = common.Chromium(size=(1280, 800), args=args)
    else:
        prefs = {'webgpu': FIREFOX_WEBGPU, 'none': FIREFOX_NONE}.get(cell, [])
        b = common.Firefox(size=(1280, 800), extra_prefs=prefs)
    b.preload(HOOK)
    return b


SEEN = []


def harvest(b):
    """Keep what the page on show logged before it is left."""
    try:
        SEEN.extend(b.page('window.__vg.logs') or [])
        SEEN.extend(f'csp: {c}' for c in b.page('window.__vg.csp') or [])
    except Exception:
        pass


def logs(b):
    """What the cell's pages logged: console warnings and errors, CSP violations, page errors."""
    harvest(b)
    found = list(SEEN)
    SEEN.clear()
    errs = b.errors if engine == 'chromium' else b.collect_errors()
    # the browser's own favicon request, which the daemon has no answer for
    return found + [e for e in errs if 'favicon' not in e]


def load(b, d, query='?theme=voidgarden'):
    harvest(b)
    b.goto(f'{d.url}{query}', settle=1.0)
    return b.page(BOOT)


def scene_tier(b, secs=40.0):
    """The scene's tier once the renderer answered ('webgpu', 'webgl2', 'none')."""
    return poll(lambda: b.page(f'{DRIVER}.tier()'), secs, 0.3)


def stats_of(b, seed, t):
    return b.page(f'{DRIVER}.frame({seed}, {t})')


# ------------------------------------------------------------------ cases of the scene
def the_tier(b, d, cell):
    got = scene_tier(b)
    want = 'webgpu' if cell == 'webgpu' else 'webgl2'
    check(f'[{cell}] the tier is {want}', got == want, got)
    perf = b.page(f'{DRIVER}.perf()')
    check(f'[{cell}] perf names the tier and has drawn', perf['tier'] == want and poll(
        lambda: b.page(f'{DRIVER}.perf().draws') > 0, 20.0), perf)
    words = b.page(f'{DRIVER}.perf().words')
    check(f'[{cell}] the readout words name the tier', words == 'WebGL2 · no adapter' if want == 'webgl2'
          else words.startswith('WebGPU'), words)
    seeds = poll(lambda: (lambda s: s if s['rig'] and s['layers'] else None)(b.page(f'{DRIVER}.perf().seeds')), 10.0)
    check(f'[{cell}] the first track\'s seed reaches the rig and the layers', bool(seeds) and seeds['rig'] == seeds['layers'],
          b.page(f'{DRIVER}.perf().seeds'))
    if cell == 'webgpu':
        load(b, d, '?theme=voidgarden&vg-gpu=webgl2')
        got = scene_tier(b)
        check(f'[{cell}] ?vg-gpu=webgl2 asks for the WebGL2 floor', got == 'webgl2', got)
        check(f'[{cell}] and the readout says it was chosen', b.page(f'{DRIVER}.perf().words') == 'WebGL2 · chosen',
              b.page(f'{DRIVER}.perf().words'))
        load(b, d)
        scene_tier(b)


PICK = "svolder.theme.voidgarden.renderer-pick"


def the_probe(b, d, cell):
    """Both tiers measured, the faster kept: a tier slowed by the address (`vg-probe=slow-<tier>`:
    it draws every sixth display frame) loses; the next visit reads the kept pick; nothing of the
    probe stays on the page."""
    for slow, want in (('webgpu', 'webgl2'), ('webgl2', 'webgpu')):
        b.page(f'(localStorage.removeItem("{PICK}"), 1)')
        load(b, d, f'?theme=voidgarden&vg-probe=slow-{slow}')
        got = scene_tier(b, 120.0)
        pick = b.page(f'{DRIVER}.perf().pick') or {}
        fps = pick.get('fps') or {}
        check(f'[{cell}] a slowed {slow} loses the probe: the scene is on {want}', got == want, (got, fps))
        check(f'[{cell}]   it drew fewer frames a second', (fps.get(slow) or 0) < (fps.get(want) or 0), fps)
        kept = ((json.loads(b.page(f'localStorage.getItem("{PICK}")') or 'null') or {}).get('picks') or [{}])[-1]
        check(f'[{cell}]   the pick is kept with its key', kept.get('tier') == want and '|' in kept.get('key', ''), kept)
        check(f'[{cell}]   no probe canvas stays', b.page('document.querySelectorAll("#vg-probe").length') == 0)
        if want == 'webgl2':
            check(f'[{cell}]   the readout says it was faster', b.page(f'{DRIVER}.perf().words') == 'WebGL2 · faster',
                  b.page(f'{DRIVER}.perf().words'))
        load(b, d)
        got = scene_tier(b)
        check(f'[{cell}]   the next visit starts on the kept {want}, unmeasured',
              got == want and b.page(f'{DRIVER}.perf().pick') is None, got)
    b.page(f'(localStorage.removeItem("{PICK}"), 1)')
    load(b, d)
    scene_tier(b)


# the cover the fake host's player names, drawn as the wire draws it, and the pair palette.js picks
COVER_PAIR = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { palette, fromCover } = await import(base + 'palette.js');
  const track = __core.model.page().host.track;
  const image = new Image();
  image.src = 'api/v1/host/cover?v=' + encodeURIComponent(track.cover);
  await image.decode();
  const canvas = document.createElement('canvas'); canvas.width = canvas.height = 32;
  const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0, 32, 32);
  const px = ctx.getImageData(0, 0, 32, 32).data;
  const text = { title: track.headline, artist: track.artist };
  return { want: palette(px, text), cover: fromCover(px), seeded: palette(null, text) };
})()"""


def the_palette(b, cell):
    got = b.page(COVER_PAIR)
    check(f'[{cell}] the fake cover has colour (the case is not vacuous)', got['cover'] is not None
          and got['want'] != got['seeded'], got)
    check(f'[{cell}] the scene\'s palette is the cover\'s pair', poll(
        lambda: b.page(f'{DRIVER}.perf().palette') == got['want'], 10.0), [got, b.page(f'{DRIVER}.perf().palette')])


def the_frame(b, cell):
    # the page hidden: the live loop draws nothing, so the frames read are the only draws (the live
    # loop's own state is not the frame's)
    b.page(HIDE)
    time.sleep(1.5)
    a = stats_of(b, 7, 2)
    again = stats_of(b, 7, 2)
    other = stats_of(b, 8, 2)
    b.page(SHOW)
    check(f'[{cell}] the same seed and time give the same statistics', a == again and a['lit'] > 0, [a, again])
    check(f'[{cell}] another seed differs', other['places'] != a['places'], [a, other])
    live = b.page(f'{DRIVER}.pixels()')
    check(f'[{cell}] the pixels of the live scene have lit pixels', live and live['lit'] > 0, live)


# the ladder as the page sees it, recorded in the page itself (a poll over the browser's channel can
# miss a step of one second on a loaded machine): every change of the budget's levels, until `steps`
RECORD = """new Promise(async (resolve) => { const seen = []; const end = performance.now() + 60000;
  // from the top at 60: High held for a moment, then Auto goes on from there
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  __core.theme.driver.forceDrawMs(28);
  __core.prefs.set('detail', 'high'); await wait(900); __core.prefs.set('detail', 'auto'); await wait(700);
  const tick = () => { const p = __core.theme.driver.perf();
    const r3 = (v) => Math.round(v * 1000) / 1000;
    const row = [[p.sky, p.water, p.det, p.rings, p.flora, p.back, p.q, p.post, p.grit, p.bloom2, p.hero, p.scale].map((v) => (typeof v === 'number' ? r3(v) : v)), p.capped, p.step];
    if (!seen.length || seen[seen.length - 1].join() !== row.join()) seen.push(row);
    if (p.step >= STEPS || performance.now() > end) resolve(seen); else setTimeout(tick, 40); };
  tick(); })"""
# the way back: the page waits for the step to fall below `from`
BACK = """new Promise((resolve) => { const end = performance.now() + 30000;
  const tick = () => { const p = __core.theme.driver.perf();
    if (p.step < FROM || performance.now() > end) resolve(p); else setTimeout(tick, 100); };
  __core.theme.driver.forceDrawMs(2); tick(); })"""


def ladder(step, base=1.0):
    """The budget's levels at `step` (a 1280x800 window at density 1: base scale 1.0): sky, water,
    land detail, rings, trees, backdrop share, particles, post, grit, bloom2, hero, frame scale, as
    the page reads them. The ladder's first round is the backdrop's: the sky's, the water's and the
    land's detail to 0.5, the rings to .75, the trees to .7, then the backdrop's share .9 .82 .75;
    the second round takes the details to 0, the rings to .55, the trees to .5, the share to .5."""
    rungs = [('sky', 0.5), ('water', 0.5), ('det', 0.5), ('rings', 0.75), ('flora', 0.7),
             ('back', 0.9), ('back', 0.82), ('back', 0.75), ('sky', 0), ('water', 0), ('det', 0),
             ('rings', 0.55), ('flora', 0.5), ('back', 0.67), ('back', 0.6), ('back', 0.55),
             ('back', 0.5), ('rings', 0.4)]
    names = ['sky', 'water', 'det', 'rings', 'flora', 'back', 'q', 'post', 'grit', 'bloom2', 'hero',
             'scale']
    out = dict(sky=1, water=1, det=1, rings=1, flora=1, back=1, q=1, post=1, grit=True, bloom2=True,
               hero=1, scale=base)
    for what, value in rungs[:step]:
        out[what] = min(out[what], value)
    return [round(out[k], 3) if not isinstance(out[k], bool) else out[k] for k in names]


def the_budget(b, cell):
    b.page(f"localStorage.removeItem('{STATE}')")
    seen = b.page(RECORD.replace('STEPS', '8'))
    steps = [r[2] for r in seen]
    check(f'[{cell}] forced 28 ms (neither 60 nor 30 is held) steps down to step 8 or beyond (a slow draw may take two)',
          all(a < b for a, b in zip(steps, steps[1:])) and steps[-1] >= 8, seen)
    check(f'[{cell}] every step is the ladder\'s: the backdrop first (the sky, the water, the land, the trees, its share), in order',
          all(r[0] == ladder(r[2]) for r in seen), seen)
    check(f'[{cell}] the particles, the post, the effects, the hero and the frame are untouched while the backdrop\'s steps are spent (the target stays 60)',
          all(r[0][6:] == [1, 1, True, True, 1, 1] and r[1] is False for r in seen if r[2] <= 8), seen)
    top = steps[-1]
    after = b.page(BACK.replace('FROM', str(top)))
    check(f'[{cell}] forced 2 ms steps back, the last step first',
          after['step'] < top and [round(after[k], 3) if not isinstance(after[k], bool) else after[k] for k in (
              'sky', 'water', 'det', 'rings', 'flora', 'back', 'q', 'post', 'grit', 'bloom2', 'hero', 'scale')]
          == ladder(after['step']), after)
    b.page(f'{DRIVER}.forceDrawMs(null)')


# the canvas after `driver.still` at step STEP (the live loop stopped), read in the same task: 160x90 RGBA
GRAB = """(async () => { const d = __core.theme.driver; d.freeze(); const out = {};
  for (const step of [0, STEPS]) { d.hold(step, 60); await d.still(7, 20);
    const c = document.querySelector('#vg-scene'); const o = document.createElement('canvas'); o.width = 160; o.height = 90;
    const g = o.getContext('2d'); g.drawImage(c, 0, 0, 160, 90); out[step] = Array.from(g.getImageData(0, 0, 160, 90).data);
    out['p' + step] = d.perf().back; out['h' + step] = d.heroAt(); }
  return out; })()"""
# a smaller canvas: the scene is resized before the stills
SHRINK = """(() => { const c = document.querySelector('#vg-scene'); c.style.width = '70%'; c.style.height = '70%'; return 1; })()"""


def composite_stats(px):
    lum = [0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2] for i in range(0, len(px), 4)]
    return lum


def the_composite(b, d, cell, resize=False):
    """The hero and the beacons are drawn at the canvas's size over the backdrop drawn at its share: the
    frame at the backdrop's last step keeps the hero's box (around `driver.heroAt()`) as bright and
    every band of the frame as lit (a front pass that lost the hero, or a quad that lost the
    backdrop, fails here)."""
    load(b, d)
    scene_tier(b)
    if resize:
        b.page(SHRINK)
        b.sleep(1.5)
    got = b.page(GRAB.replace('STEPS', str(BACKDROP_STEPS)))
    top, low = composite_stats(got['0']), composite_stats(got[str(BACKDROP_STEPS)])
    w, h = 160, 90
    # the hero's box: its centre (clip units) and 12 % of the width around it
    hx, hy, on = got['h0']
    cx, cy = (hx * 0.5 + 0.5) * w, (0.5 - hy * 0.5) * h
    box = [y * w + x for y in range(h) for x in range(w) if abs(x - cx) <= 0.12 * w and abs(y - cy) <= 0.12 * w]
    mean = lambda lum, idx: sum(lum[i] for i in idx) / max(1, len(idx))
    hero_top, hero_low = mean(top, box), mean(low, box)
    bands = [[i for i in range(len(top)) if (i // w) * 3 // 90 == k] for k in range(3)]
    lit = [(mean(top, band), mean(low, band)) for band in bands]
    tag = f'[{cell}{" resized" if resize else ""}]'

    def edges(lum):
        # the box's mean step between neighbours: the hero's wires and bars are its edges
        return sum(abs(lum[i + 1] - lum[i]) + abs(lum[i + w] - lum[i]) for i in box if i + w < len(lum)) / len(box)

    apart = sum(abs(top[i] - low[i]) for i in box) / len(box)
    sharp = (edges(top), edges(low))
    check(f'{tag} the backdrop at share {got["p" + str(BACKDROP_STEPS)]}: the hero\'s box is the top step\'s, as bright and as sharp',
          on == 1 and len(box) > 100 and hero_top > 40 and 0.8 <= hero_low / hero_top <= 1.25
          and sharp[0] > 3 and 0.8 <= sharp[1] / sharp[0] <= 1.25 and apart < 6,
          {'top': round(hero_top, 1), 'low': round(hero_low, 1), 'edges': [round(x, 2) for x in sharp],
           'apart': round(apart, 2), 'at': got['h0'], 'box': len(box)})
    check(f'{tag} every band of the frame (sky, middle, land) as lit over the scaled backdrop',
          all(a > 4 and 0.75 <= c / a <= 1.33 for a, c in lit), [[round(a, 1), round(c, 1)] for a, c in lit])


# the backdrop's steps of the ladder (the python mirror's `ladder` rungs): its last one, share 0.5
BACKDROP_STEPS = 18


def the_analysis(b, cell):
    held = '__core.analysis.holders'
    check(f'[{cell}] one holder while the scene shows', poll(lambda: b.page(held) == 1, 10.0), b.page(held))
    b.page(HIDE)
    check(f'[{cell}] none after the page is hidden', poll(lambda: b.page(held) == 0, 8.0), b.page(held))
    b.page(SHOW)
    check(f'[{cell}] one again when it shows', poll(lambda: b.page(held) == 1, 10.0), b.page(held))


def no_gpu(b, d):
    got = scene_tier(b)
    check('[none] the tier is none', got == 'none', got)
    check('[none] the still of the valley is in the page', b.page(
        "!!document.querySelector('#vg-still svg')"))
    check('[none] the quiet line says what the theme needs', b.page(
        "document.getElementById('vg-quiet').textContent") == 'This theme needs WebGPU or WebGL2')
    one = b.page(f'{DRIVER}.perf().steps')
    time.sleep(3.2)
    check('[none] the feed goes on at the Still pace', b.page(f'{DRIVER}.perf().steps') - one >= 2,
          [one, b.page(f'{DRIVER}.perf().steps')])
    b.page(REC)
    b.page("document.activeElement && document.activeElement.blur()")
    b.press('ArrowUp')
    check('[none] the volume key still writes', poll(lambda: any(
        '/volume' in u for u, _ in b.page('window.__posts')), 5.0), b.page('window.__posts'))


# ------------------------------------------------------------------ the page-level GPU cases
GPU_CASES = """(async () => {
  const base = new URL('assets/themes/voidgarden/', document.baseURI).href;
  const { createGpu } = await import(base + 'gpu.js');
  const { createLib } = await import(base + 'tsl/lib.js');
  const { createSprites } = await import(base + 'tsl/sprite.js');
  const { PALETTES } = await import(base + 'palette.js');
  const out = {};
  const canvas = document.createElement('canvas');
  canvas.width = 16; canvas.height = 16;
  document.body.append(canvas);
  const gpu = await createGpu({ canvas });
  const { T, S } = gpu;
  const lib = createLib(T, S, gpu.tier);
  out.tier = gpu.tier;
  out.caps = gpu.caps;
  gpu.size(16, 16);
  // the colour: a flat quad of a palette hex read back from the canvas in the draw's own task
  const cam = new T.OrthographicCamera(-1, 1, 1, -1, -1, 1);
  const quad = (node) => {
    const m = new T.MeshBasicNodeMaterial(); m.colorNode = node;
    const s = new T.Scene(); s.add(new T.Mesh(new T.PlaneGeometry(2, 2), m)); return s;
  };
  const readCanvas = () => {
    const c = document.createElement('canvas'); c.width = 16; c.height = 16;
    const x = c.getContext('2d'); x.drawImage(canvas, 0, 0);
    return [[1, 1], [14, 1], [1, 14], [14, 14], [8, 8]].map(([px, py]) => [...x.getImageData(px, py, 1, 1).data].slice(0, 3));
  };
  const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  out.colour = [];
  for (const hex of [PALETTES[0][0], PALETTES[1][0], PALETTES[0][1]]) {
    gpu.render(quad(lib.colour(hex)), cam);
    const want = rgb(hex);
    const got = readCanvas();
    out.colour.push({ hex, worst: Math.max(...got.flatMap((p) => p.map((v, i) => Math.abs(v - want[i])))) });
    const raw = want.map((v) => v / 255);
    gpu.render(quad(S.vec3(raw[0], raw[1], raw[2])), cam);
    out.colour[out.colour.length - 1].rawWorst = Math.max(...readCanvas().flatMap((p) => p.map((v, i) => Math.abs(v - want[i]))));
  }
  // the neighbour read: out[i] = in[i + 1] + in[i - 1] over a ring of 8
  const N = 8;
  const want = [8 + 2, 1 + 3, 2 + 4, 3 + 5, 4 + 6, 5 + 7, 6 + 8, 7 + 1];
  const ring = async (wrap) => {
    const inp = lib.cpuBuffer(N); lib.upload(inp, [1, 2, 3, 4, 5, 6, 7, 8]);
    const mid = lib.computeBuffer(N);
    const dst = lib.computeBuffer(N);
    const src = wrap(mid);
    const copy = S.Fn(() => { mid.element(S.instanceIndex).assign(inp.element(S.instanceIndex)); })().compute(N);
    const node = S.Fn(() => {
      const i = S.instanceIndex;
      dst.element(i).assign(src.element(i.add(1).mod(N)).add(src.element(i.add(N - 1).mod(N))));
    })().compute(N);
    // two frames: WebGL2 makes the texture of a buffer when the pass that reads it is first built, so
    // a write that came before that pass first ran is seen from the next write on
    for (let frame = 0; frame < 2; frame++) {
      await gpu.renderer.computeAsync(copy);
      await gpu.renderer.computeAsync(node);
    }
    return Array.from(new Float32Array(await gpu.renderer.getArrayBufferAsync(dst.value)));
  };
  out.neighbour = { want, helper: await ring((b) => lib.neighbours(b)), plain: await ring((b) => b) };
  // two uploads: the neighbour read must follow the second, from a buffer the CPU wrote and from one
  // a compute pass copied it into (the way a layer's state is made)
  const twice = async (kind) => {
    const inp = lib.cpuBuffer(N); const mid = lib.computeBuffer(N); const dst = lib.computeBuffer(N);
    const copy = S.Fn(() => { mid.element(S.instanceIndex).assign(inp.element(S.instanceIndex)); })().compute(N);
    let src;
    try { src = lib.neighbours(kind === 'cpu' ? inp : mid); } catch (e) { return { threw: String(e.message) }; }
    const node = S.Fn(() => {
      const i = S.instanceIndex;
      dst.element(i).assign(src.element(i.add(1).mod(N)).add(src.element(i.add(N - 1).mod(N))));
    })().compute(N);
    let got = null;
    for (const base of [0, 10]) {
      lib.upload(inp, Array.from({ length: N }, (_, i) => base + i + 1));
      if (kind !== 'cpu') await gpu.renderer.computeAsync(copy);
      await gpu.renderer.computeAsync(node);
      got = Array.from(new Float32Array(await gpu.renderer.getArrayBufferAsync(dst.value)));
    }
    const v = (i) => 10 + ((i + N) % N) + 1;
    return { got, want: Array.from({ length: N }, (_, i) => v(i + 1) + v(i - 1)) };
  };
  out.neighbourTwice = { cpu: await twice('cpu'), computed: await twice('computed') };
  // the audio the shaders read, uploaded twice with other values, read at k != 0 in a render of 32
  // pixels (one k each) and compared with the SECOND upload
  const aud = lib.audio();
  const val = (i, salt) => ((i * 37 + salt * 11) % 200) / 255;
  const fill = (n, salt) => Float32Array.from({ length: n }, (_, i) => val(i, salt));
  const AW = 32;
  const px32 = gpu.target(AW, 1);
  const x = S.int(S.screenCoordinate.x.floor());
  const sceneA = quad(S.vec3(aud.bandL(x.mul(7).add(1)), aud.bandR(x.mul(7).add(1)), aud.spectrum(x.mul(2).add(1))));
  const sceneB = quad(S.vec3(aud.wave(x.mul(8).add(3)), aud.wave(S.int(255).sub(x.mul(8))), 0));
  const unit = new T.OrthographicCamera(-1, 1, 1, -1, -1, 1);
  const byteAt = (px, c) => Array.from({ length: AW }, (_, i) => px[i * 4 + c]);
  out.audio = { reads: [] };
  for (const salt of [1, 2]) {
    aud.set({ left: fill(240, salt), right: fill(240, salt + 5), bins: fill(64, salt + 9), samples: fill(256, salt + 13) });
    const a = await gpu.read(sceneA, unit, px32);
    const b = await gpu.read(sceneB, unit, px32);
    const idx = [...Array(AW).keys()];
    const want = (n, f, salt2) => idx.map((i) => Math.round(255 * val(f(i), salt2)));
    const worst = (got, w) => Math.max(...got.map((g, i) => Math.abs(g - w[i])));
    out.audio.reads.push({
      salt,
      l: worst(byteAt(a, 0), want(0, (i) => i * 7 + 1, salt)),
      r: worst(byteAt(a, 1), want(0, (i) => i * 7 + 1, salt + 5)),
      bin: worst(byteAt(a, 2), want(0, (i) => i * 2 + 1, salt + 9)),
      w1: worst(byteAt(b, 0), want(0, (i) => i * 8 + 3, salt + 13)),
      w2: worst(byteAt(b, 1), want(0, (i) => 255 - i * 8, salt + 13)),
      sample: byteAt(a, 0).slice(0, 6),
    });
  }
  // the sprites: sizes 4, 8, 16, 32 px at x = 16, 40, 72, 108
  const sprites = createSprites(T, S, lib);
  const sizes = [4, 8, 16, 32]; const cx = [16, 40, 72, 108]; const edges = [0, 28, 56, 90, 128];
  const ortho = new T.OrthographicCamera(0, 128, 128, 0, -10, 10);
  const target = gpu.target(128, 128);
  out.sprites = {};
  for (const mode of ['px', 'world']) {
    const position = lib.cpuBuffer(4, 'vec4'); lib.upload(position, cx.flatMap((x) => [x, 64, 0, 0]));
    const size = lib.cpuBuffer(4); lib.upload(size, sizes);
    const scene = new T.Scene();
    scene.add(sprites.make({ count: 4, position, size, mode, additive: false }));
    const px = await gpu.read(scene, ortho, target);
    const widths = cx.map((_, i) => { let n = 0; for (let x = edges[i]; x < edges[i + 1]; x++) if (px[(64 * 128 + x) * 4] > 127) n++; return n; });
    let lit = 0; for (let i = 0; i < 128 * 128; i++) if (px[i * 4] > 127) lit++;
    out.sprites[mode] = { widths, lit };
  }
  out.csp = window.__vg.csp;
  return out;
})()"""


def gpu_cases(b, d, cell):
    harvest(b)
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    got = b.page(GPU_CASES)
    want_tier = 'webgpu' if cell == 'webgpu' else 'webgl2'
    caps = got['caps']
    check(f'[{cell}] the capability bag follows the tier', got['tier'] == want_tier
          and caps['neighbours'] == (want_tier == 'webgpu') and caps['timestamps'] == (want_tier == 'webgpu')
          and caps['compute'] is True and caps['maxParticles'] == (160000 if want_tier == 'webgpu' else 40000)
          and caps['reason'] == (None if want_tier == 'webgpu' else 'no adapter')
          and isinstance(caps['vendor'], str) and (want_tier == 'webgl2' or engine == 'firefox' or caps['vendor'] != ''), caps)
    check(f'[{cell}] palette hexes read back within 2/255', all(c['worst'] <= 2 for c in got['colour']), got['colour'])
    check(f'[{cell}] raw floats of the same hexes do not (73/255 too bright)',
          all(c['rawWorst'] > 20 for c in got['colour']), got['colour'])
    nb = got['neighbour']
    check(f'[{cell}] the pbo helper gives the neighbour sums', nb['helper'] == nb['want'], nb)
    if want_tier == 'webgl2':
        check(f'[{cell}] a plain neighbour read is wrong on WebGL2 (why the helper is there)',
              nb['plain'] != nb['want'], nb)
    for second in [got['audio']['reads'][1]]:
        check(f'[{cell}] bandL(k), bandR(k), spectrum(k), wave(k) read the second upload at k != 0',
              all(second[k] <= 1 for k in ('l', 'r', 'bin', 'w1', 'w2')), got['audio'])
    nt = got['neighbourTwice']
    check(f'[{cell}] a neighbour read of a CPU-written buffer is refused (it would read the first upload)',
          'threw' in nt['cpu'], nt['cpu'])
    check(f'[{cell}] a neighbour read of a compute-written buffer follows the second upload',
          nt['computed']['got'] == nt['computed']['want'], nt['computed'])
    for mode in ('px', 'world'):
        s = got['sprites'][mode]
        sizes = [4, 8, 16, 32]
        check(f'[{cell}] {mode} sprites are squares of 4, 8, 16, 32', all(
            abs(w - z) <= 1 for w, z in zip(s['widths'], sizes)) and abs(s['lit'] - sum(z * z for z in sizes)) <= 40, s)


def one_cell(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(cell)
    try:
        if cell == 'none':
            load(b, d)
            no_gpu(b, d)
        else:
            load(b, d)
            the_tier(b, d, cell)
            if cell == 'webgpu':
                the_probe(b, d, cell)
            the_palette(b, cell)
            the_frame(b, cell)
            the_analysis(b, cell)
            the_budget(b, cell)
            gpu_cases(b, d, cell)
            the_composite(b, d, cell)
            the_composite(b, d, cell, resize=True)
        found = logs(b)
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
print(f'voidgarden_gpu.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
