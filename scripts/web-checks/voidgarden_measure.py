# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Void Garden's cost and budget on the real GPU (`make voidgarden-measure`): headless Chromium with
the machine's GPU (ANGLE GL, the WebGL2 tier; the renderer string is printed and must not name
SwiftShader), a throwaway fake daemon with the fake host's music, the budget's own GPU timer
(`EXT_disjoint_timer_query_webgl2`) read through the theme's driver. Prints one JSON line a case.
No check: the numbers are for tuning and for the proof notes, never asserted.
Usage: voidgarden_measure.py WxH[@DPR] case...
  full[:SW+SW..] detail High pinned (the top step), vsync and the frame cap off: the cost of a frame;
                 with the bisect switches SW (`noland`, `nopost`, ...) turned off: the cost of what is left
  step:N[,M..]   each ladder step held at 60 (`step:N@30` at 30), vsync and the frame cap off: the
                 GPU ms a frame costs at that step (median of 4 windows of 4 s), the fps, the scale
  settle[:SECS]  the budget on its own with vsync on, from a clean browser (default 60 s): the fps
                 and the step each second, the step and scale it settled on, the seconds to settle
                 (the last change of step), the steps changed after that (flapping)
  still:PATH[:SECS[:high]]  a PNG of the page after SECS (default 30) seconds of the budget settling (or of detail
                 High pinned): the live page, so two stills are two moments of the music
  pair:PREFIX[:SECS]  PREFIX-full.png and PREFIX-settled.png: the budget settles for SECS (default 30), the
                 live loop stops, and the top step and the settled step are each drawn at the SAME seed and
                 time (`driver.still`): the least-perceptible check, the loss and nothing else
  load[:CPUx[:MBIT]]  a phone's first load (390x844 at DPR 3, a touch screen, a cold browser), the CPU slowed
                 CPUx times (default 4) and the network held to MBIT megabits a second (default 40, 4 ms of
                 latency): the bytes and seconds of the download, the module parse (the vendored files' last
                 byte to their namespaces), the renderer, the layers' build, the first draws (the land cache's
                 first fill and the shaders' compile), and a seek's fill alone (the same draw with the shaders
                 compiled), from the scene's own `perf().load` stamps
  shot:PATH[:SEED[:T[:STEP]]]  a PNG of the frame at (SEED, T) (default 7, 20) drawn at the top step (or
                 at STEP) with the live loop stopped: the same picture from two builds (SMOKE_WEB) for a
                 look comparison, the canvas kept at its size
Env: SVOLDERD (a daemon binary, default target/debug/svolderd), SMOKE_WEB (the web dir)."""
import base64
import json
import statistics
import sys

import common

SIZE = sys.argv[1] if len(sys.argv) > 1 else '1920x1080'
CASES = sys.argv[2:] or ['full', 'settle']
GPU = ['--enable-gpu', '--ignore-gpu-blocklist', '--use-angle=gl']
UNLIMITED = ['--disable-gpu-vsync', '--disable-frame-rate-limit']
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
DRIVER = '__core.theme.driver'
PERF = f"""(() => {{ const p = {DRIVER}.perf(); const cv = document.querySelector('#vg-scene');
  return {{ draws: p.draws, gpuMs: p.gpuMs, ms: p.ms, step: p.step, rungs: p.rungs, scale: p.scale,
    target: p.target, q: p.q, rings: p.rings, grit: p.grit, bloom2: p.bloom2, detail: p.detail,
    back: p.back, sky: p.sky, water: p.water, det: p.det, flora: p.flora, post: p.post, hero: p.hero,
    canvas: [cv.width, cv.height], t: performance.now() }}; }})()"""
RENDERER = """(() => { const c = document.createElement('canvas'); const g = c.getContext('webgl2');
  const e = g.getExtension('WEBGL_debug_renderer_info'); return e ? g.getParameter(e.UNMASKED_RENDERER_WEBGL) : 'n/a'; })()"""
# 4 s of reading the driver every 50 ms (the GPU ms, and the CPU ms of a draw)
WINDOW = f"""new Promise((done) => {{ const a = {PERF}; const gpu = []; const cpu = [];
  const id = setInterval(() => {{ const p = {DRIVER}.perf(); if (p.gpuMs !== null) gpu.push(p.gpuMs);
    if (p.ms !== null) cpu.push(p.ms); }}, 50);
  setTimeout(() => {{ clearInterval(id); const b = {PERF};
    gpu.sort((x, y) => x - y); cpu.sort((x, y) => x - y);
    done({{ fps: 1000 * (b.draws - a.draws) / (b.t - a.t), gpu: gpu.length ? gpu[gpu.length >> 1] : null,
      cpu: cpu.length ? cpu[cpu.length >> 1] : null }}); }}, 4000); }})"""


def size():
    w, _, h = SIZE.partition('@')[0].partition('x')
    dpr = float(SIZE.partition('@')[2] or 1)
    return int(w), int(h), dpr


def open_page(d, unlimited, detail=None, debug=True, switches=''):
    w, h, dpr = size()
    b = common.Chromium(size=(w, h), dpr=dpr, args=GPU + (UNLIMITED if unlimited else []))
    b.preload("try { localStorage.setItem('svolder.theme.voidgarden.motion', '\"full\"'); "
              + (f"localStorage.setItem('svolder.theme.voidgarden.detail', '\"{detail}\"'); " if detail else '')
              + "} catch (e) {}")
    query = f'&vg-debug={switches}' if switches else ('&vg-debug' if debug else '')
    b.goto(f'{d.url}?theme=voidgarden' + query, settle=1)
    if b.page(BOOT) != 'voidgarden':
        raise RuntimeError('Void Garden did not mount')
    return b


def windows(b, count=4):
    wins = [b.page(WINDOW) for _ in range(count)]
    gpu = [x['gpu'] for x in wins if x['gpu'] is not None]
    cpu = [x['cpu'] for x in wins if x['cpu'] is not None]
    return {'fps': round(statistics.median(x['fps'] for x in wins), 1),
            'gpuMs': round(statistics.median(gpu), 1) if gpu else None,
            'cpuMs': round(statistics.median(cpu), 2) if cpu else None,
            'gpuMsWindows': [round(x, 1) for x in gpu]}


def case_full(d, switches=''):
    b = open_page(d, True, 'high', switches=switches.replace('+', ','))
    try:
        b.sleep(10)
        return {'case': f'full:{switches}' if switches else 'full', **windows(b), 'info': b.page(PERF),
                'renderer': b.page(RENDERER)}
    finally:
        b.close()


def case_steps(d, spec):
    b = open_page(d, True)
    out = []
    try:
        b.sleep(4)
        for item in spec.split(','):
            step, _, fps = item.partition('@')
            b.page(f'{DRIVER}.hold({int(step)}, {int(fps or 60)})')
            b.sleep(3)
            out.append({'case': f'step:{item}', **windows(b), 'info': b.page(PERF)})
        return out
    finally:
        b.close()


def case_settle(d, secs):
    b = open_page(d, False)
    rows = []
    try:
        last = b.page(PERF)
        for _ in range(secs):
            b.sleep(1)
            now = b.page(PERF)
            rows.append({'fps': round(1000 * (now['draws'] - last['draws']) / (now['t'] - last['t']), 1),
                         'step': now['step'], 'target': now['target'], 'scale': round(now['scale'], 3),
                         'back': now['back'],
                         'gpuMs': None if now['gpuMs'] is None else round(now['gpuMs'], 1)})
            last = now
        info = b.page(PERF)
        changes = [i for i in range(1, len(rows)) if (rows[i]['step'], rows[i]['target'])
                   != (rows[i - 1]['step'], rows[i - 1]['target'])]
        settled_at = (changes[-1] if changes else 0) + 1
        tail = rows[len(rows) // 2:]
        return {'case': f'settle:{secs}', 'settledAtS': settled_at,
                'settled': {'step': info['step'], 'of': info['rungs'], 'target': info['target'],
                            'scale': round(info['scale'], 3), 'back': info['back'], 'sky': info['sky'],
                            'water': info['water'], 'det': info['det'], 'rings': info['rings'],
                            'flora': info['flora'], 'q': round(info['q'], 2), 'post': info['post'],
                            'grit': info['grit'], 'bloom2': info['bloom2'], 'hero': info['hero']},
                'fpsLastHalfMedian': round(statistics.median(r['fps'] for r in tail), 1),
                'fpsLastHalfMin': min(r['fps'] for r in tail),
                'stepChangesAfterSettle': len([c for c in changes if c >= settled_at]),
                'canvas': info['canvas'], 'perSecond': rows, 'renderer': b.page(RENDERER)}
    finally:
        b.close()


def save_png(shot, path):
    raw = base64.b64decode(shot['data'])
    from PIL import Image
    import io
    img = Image.open(io.BytesIO(raw)).convert('RGB')
    if img.width > 1920:
        # a 4K still is kept at 1920 wide: the look is judged, not the pixel count
        img = img.resize((1920, round(img.height * 1920 / img.width)), Image.LANCZOS)
    img.save(path, optimize=True)


def case_still(d, path, secs, high):
    b = open_page(d, False, 'high' if high else None)
    try:
        b.sleep(secs)
        shot = b.send('Page.captureScreenshot', {'format': 'png'})
        save_png(shot, path)
        return {'case': f'still:{path}', 'info': b.page(PERF)}
    finally:
        b.close()


def case_pair(d, prefix, secs):
    # no debug readout: the loop is stopped, so it would name the step before
    b = open_page(d, False, debug=False)
    try:
        b.sleep(secs)
        settled = b.page(PERF)
        b.page(f'{DRIVER}.freeze()')
        out = {}
        for name, step, fps in (('full', 0, 60), ('settled', settled['step'], settled['target'])):
            b.page(f'{DRIVER}.hold({step}, {fps})')
            b.page(f'{DRIVER}.still(7, 20)')
            shot = b.send('Page.captureScreenshot', {'format': 'png'})
            save_png(shot, f'{prefix}-{name}.png')
            info = b.page(PERF)
            out[name] = {'step': info['step'], 'scale': round(info['scale'], 3), 'canvas': info['canvas']}
        return {'case': f'pair:{prefix}', **out, 'renderer': b.page(RENDERER)}
    finally:
        b.close()


def case_shot(d, path, seed, t, step):
    b = open_page(d, False, debug=False)
    try:
        b.sleep(4)
        b.page(f'{DRIVER}.freeze()')
        b.page(f'{DRIVER}.hold({step}, 60)')
        b.page(f'{DRIVER}.still({seed}, {t})')
        # the scene alone: the overlay's live numbers are not the picture
        b.page("""(() => { const c = document.querySelector('#vg-scene');
          for (const e of document.querySelectorAll('body *')) if (!e.contains(c)) e.style.visibility = 'hidden';
          return 1; })()""")
        shot = b.send('Page.captureScreenshot', {'format': 'png'})
        raw = base64.b64decode(shot['data'])
        with open(path, 'wb') as f:
            f.write(raw)
        info = b.page(PERF)
        return {'case': f'shot:{path}', 'seed': seed, 't': t, 'step': info['step'], 'canvas': info['canvas']}
    finally:
        b.close()


LOAD = """(() => { const p = __core.theme.driver.perf(); const res = performance.getEntriesByType('resource');
  const nav = performance.getEntriesByType('navigation')[0];
  const mine = res.filter((r) => /assets\\//.test(r.name));
  const three = mine.filter((r) => /vendor\\/three/.test(r.name));
  const end = (list) => Math.max(0, ...list.map((r) => r.responseEnd));
  const start = (list) => Math.min(...list.map((r) => r.fetchStart));
  return { load: p.load, draws: p.draws, assets: mine.length,
    bytes: mine.reduce((a, r) => a + (r.encodedBodySize || r.transferSize || 0), 0),
    threeBytes: three.reduce((a, r) => a + (r.encodedBodySize || r.transferSize || 0), 0),
    threeFetch: [start(three), end(three)], assetsEnd: end(mine), domContentLoaded: nav && nav.domContentLoadedEventEnd }; })()"""


def case_load(d, cpu, mbit):
    b = common.Chromium(size=(390, 844), dpr=3, touch=True, args=GPU)
    try:
        b.preload("try { localStorage.setItem('svolder.theme.voidgarden.motion', '\"full\"'); } catch (e) {}")
        b.send('Emulation.setCPUThrottlingRate', {'rate': cpu})
        b.send('Network.emulateNetworkConditions', {'offline': False, 'latency': 4,
               'downloadThroughput': mbit * 1e6 / 8, 'uploadThroughput': mbit * 1e6 / 8})
        b.goto(f'{d.url}?theme=voidgarden', settle=1)
        if b.page(BOOT) != 'voidgarden':
            raise RuntimeError('Void Garden did not mount')
        for _ in range(120):
            got = b.page(LOAD)
            if got['load'] and got['load'].get('second') is not None:
                break
            b.sleep(0.5)
        seek = b.page(f"""(async () => {{ const a = performance.now(); await {DRIVER}.frame(4242, 1);
          return performance.now() - a; }})()""")
        return {'case': f'load:{cpu}:{mbit}', **got, 'seekMs': round(seek, 1), 'renderer': b.page(RENDERER)}
    finally:
        b.close()


def main():
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_WEB_ASSETS': common.WEB})
    try:
        for case in CASES:
            kind, _, arg = case.partition(':')
            if kind == 'full':
                got = [case_full(d, arg)]
            elif kind == 'step':
                got = case_steps(d, arg)
            elif kind == 'still':
                bits = arg.split(':')
                got = [case_still(d, bits[0], float(bits[1]) if len(bits) > 1 else 30.0, 'high' in bits[2:])]
            elif kind == 'pair':
                bits = arg.split(':')
                got = [case_pair(d, bits[0], float(bits[1]) if len(bits) > 1 else 30.0)]
            elif kind == 'shot':
                bits = arg.split(':')
                got = [case_shot(d, bits[0], int(bits[1]) if len(bits) > 1 else 7,
                                 float(bits[2]) if len(bits) > 2 else 20.0, int(bits[3]) if len(bits) > 3 else 0)]
            elif kind == 'load':
                bits = arg.split(':') if arg else []
                got = [case_load(d, float(bits[0]) if bits else 4.0, float(bits[1]) if len(bits) > 1 else 40.0)]
            elif kind == 'settle':
                got = [case_settle(d, int(arg or 60))]
            else:
                raise SystemExit(f'unknown case {case}')
            for line in got:
                print(json.dumps({'size': SIZE, **line}), flush=True)
    finally:
        d.stop()


main()
