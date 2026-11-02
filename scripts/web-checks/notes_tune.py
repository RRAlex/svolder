# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Tuning probe (not a check): the stamp's look in the canvas's own pixels over several volume
steps, numbers only (readPixels, never a picture of Smoke).
Usage: python3 notes_tune.py [chromium|firefox]"""
import json
import math
import os
import sys
import time
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
shown = 0


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(**kw):
    if browser == 'chromium':
        return common.Chromium(**kw)
    return common.Firefox(**kw)


def pg(b, expr):
    """An expression's value in the page's own realm. Firefox's WebDriver scripts run in a sandbox
    with module maps of their own, so there the expression goes through WebDriver BiDi
    (script.evaluate in the page's realm, the value passed as JSON)."""
    if browser == 'chromium':
        return b.ev(expr)
    import asyncio
    import urllib.request as rq
    import websockets
    ctx = json.load(rq.urlopen(f'http://127.0.0.1:{b.gport}/session/{b.sid}/window'))['value']
    wrapped = ('Promise.resolve(' + expr + ').then((v) => JSON.stringify(v === undefined ? null : '
                                           'v))')

    async def go():
        async with websockets.connect(b.bidi_url, max_size=None) as ws:
            await ws.send(json.dumps({'id': 1, 'method': 'script.evaluate', 'params': {
                'expression': wrapped, 'target': {'context': ctx}, 'awaitPromise': True,
                'resultOwnership': 'none'}}))
            reply = json.loads(await ws.recv())
            if reply.get('type') == 'error' or reply['result'].get('type') != 'success':
                raise RuntimeError(json.dumps(reply)[:400])
            return json.loads(reply['result']['result']['value'])
    return asyncio.run(go())


def do(b, js):
    """Run statements in the page's realm."""
    return pg(b, f'(() => {{ {js} }})()')


def val(b, expr, default=None):
    """An expression's value, or `default` when it throws (a missing feature is a FAIL, not a
    crash)."""
    try:
        return pg(b, expr)
    except Exception as e:  # noqa: BLE001
        global shown
        if shown < 3:
            shown += 1
            print('      (error:', str(e)[:300].replace('\n', ' '), ')')
        return default


def errors(b):
    """Page errors: Chromium's CDP events; Firefox's BiDi preload collector, read in the page's
    realm (the collector lives there)."""
    if browser == 'chromium':
        return b.errors
    try:
        return b.errors + (pg(b, 'window.__errs || []') or [])
    except Exception as e:  # noqa: BLE001
        return b.errors + [f'collector unreadable: {e}']


PAGE = """
window.__fx = null;
import(new URL('assets/themes/ember/effects.js', document.baseURI).href).then((m) => { window.__fx = m; });
window.__st = () => (window.__fx && window.__fx.stats ? window.__fx.stats() : null);
// The stamp's own effect, from the dye grids: every frame the page keeps the dye as it left the
// frame before (a frame callback registered after the smoke's own), and the frame in which a note
// is stamped gives the pair (before, after). `__thin` then compares, by the note's mask (the atlas
// read back from the GPU), the change in the glyph and in the halo with the change outside both
// (the same frame's advection and fading), in the drawn opacity (1 - exp(-gain * amount)) and in
// the dye's colour lightness. Numbers only.
window.__tr = { prev: null, n: 0, hit: null, on: false };
window.__trStart = () => {
  const tr = window.__tr; tr.on = true; tr.hit = null; tr.prev = null;
  tr.n = window.__st().notes.stamped;
  const tick = () => {
    if (!tr.on) return;
    const rb = window.__fx.readback(), st = window.__st();
    if (rb && st) {
      if (st.notes.stamped > tr.n && tr.prev) {
        tr.hit = { prev: tr.prev, cur: rb.dye, note: st.notes.last, size: [window.innerWidth,
          window.innerHeight] };
        tr.n = st.notes.stamped; tr.on = false; return;
      }
      tr.prev = rb.dye;
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
};
window.__thin = (atlas, gain) => {
  const hit = window.__tr.hit; if (!hit) return null;
  const { prev, cur, note } = hit, cell = atlas.w / 3, t = note.tilt * Math.PI / 180;
  const cs = Math.cos(t), sn = Math.sin(t), cw = document.querySelector('canvas.smoke').clientWidth;
  const ch = document.querySelector('canvas.smoke').clientHeight;
  // per entry: n, wt, opacity ratio, light change, dye before
  const acc = { g: [0, 0, 0, 0, 0], h: [0, 0, 0, 0, 0], o: [0, 0, 0, 0, 0] };
  for (let gy = 0; gy < cur.h; gy++) for (let gx = 0; gx < cur.w; gx++) {
    const x = (gx + 0.5) / cur.w * cw, y = (1 - (gy + 0.5) / cur.h) * ch;
    const dx = x - note.x, dy = -(y - note.y);
    const qx = (cs * dx + sn * dy) / note.size + 0.5, qy = (-sn * dx + cs * dy) / note.size + 0.5;
    if (qx < 0 || qx >= 1 || qy < 0 || qy >= 1) continue;
    const mi = (Math.floor(qy * cell) * atlas.w + Math.floor((note.glyph + qx) * cell)) * 4;
    const m = atlas.data[mi], g = atlas.data[mi + 1], i = (gy * cur.w + gx) * 4;
    const a0 = prev.data[i + 3], a1 = cur.data[i + 3];
    if (!(a0 > 1e-3)) continue;
    const op = (1 - Math.exp(-gain * a1)) / (1 - Math.exp(-gain * a0));
    const l0 = (prev.data[i] + prev.data[i + 1] + prev.data[i + 2]) / 3 / a0;
    const l1 = a1 > 1e-4 ? (cur.data[i] + cur.data[i + 1] + cur.data[i + 2]) / 3 / a1 : l0;
    let bucket, wt = 1;
    if (m > 200) bucket = acc.g; else if (m < 5 && g < 5) bucket = acc.o;
    else if (m < 5) { bucket = acc.h; wt = g / 255; } else continue;
    bucket[0]++; bucket[1] += wt; bucket[2] += wt * op; bucket[3] += wt * (l1 - l0);
    bucket[4] += wt * a0;
  }
  const mean = (b, k) => (b[1] ? b[k] / b[1] : null);
  const o = { op: mean(acc.o, 2), light: mean(acc.o, 3) };
  return { note, glyphPx: acc.g[0], haloPx: acc.h[0], outsidePx: acc.o[0],
    glyphOpacity: mean(acc.g, 2) / o.op, haloOpacity: mean(acc.h, 2) / o.op,
    glyphLight: mean(acc.g, 3) - o.light, outsideOpacity: o.op, dyeBefore: mean(acc.g, 4),
    drawnBefore: 1 - Math.exp(-gain * mean(acc.g, 4)) };
};
"""


def load(b, url, wait=3.0):
    b.goto(url, settle=0.8)
    do(b, PAGE)
    for _ in range(50):
        if val(b, 'window.__fx !== null', False):
            break
        b.sleep(0.1)
    b.sleep(wait)


def notes(b):
    """The smoke's note counters: stamped, live, last (or {} before the first)."""
    return (val(b, 'window.__st()', None) or {}).get('notes') or {}


def stamped(b):
    return notes(b).get('stamped', -1)


def flick(b, secs=1.4, cx=640, cy=420, amp=340):
    """A hard unpressed pointer flick: fast zig-zags `amp` px either way around (cx, cy) for
    about `secs`, rows 40 px apart (adds no dye)."""
    t_end = time.time() + secs
    pts = []
    for i in range(6):
        y = cy - 100 + i * 40
        pts += line(cx - amp + (i % 2) * 2 * amp, y, cx - amp + ((i + 1) % 2) * 2 * amp, y + 20,
                    max(2, round(amp / 85)))
    while time.time() < t_end:
        stroke(b, pts, False, ms=6)


def brisk(b, secs=1.5):
    """A brisk but ordinary wave: about 600 px/s side to side."""
    t_end = time.time() + secs
    while time.time() < t_end:
        stroke(b, line(300, 500, 900, 500, 40) + line(900, 500, 300, 500, 40), False, ms=16)


def stroke(b, pts, pressed, ms=16):
    """A pointer stroke through `pts` (viewport px): real pointer events, a button held when
    `pressed`."""
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': pts[0][0], 'y': pts[0][1]})
        if pressed:
            b.send('Input.dispatchMouseEvent', {
                'type': 'mousePressed', 'x': pts[0][0], 'y': pts[0][1],
                'button': 'left', 'buttons': 1, 'clickCount': 1})
        for x, y in pts[1:]:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y,
                                                'buttons': 1 if pressed else 0,
                                                **({'button': 'left'} if pressed else {})})
            b.sleep(ms / 1000)
        if pressed:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': pts[-1][0],
                                                'y': pts[-1][1], 'button': 'left', 'buttons': 0,
                                                'clickCount': 1})
        return
    acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(pts[0][0]),
             'y': int(pts[0][1])}]
    if pressed:
        acts.append({'type': 'pointerDown', 'button': 0})
    for x, y in pts[1:]:
        acts.append({'type': 'pointerMove', 'duration': ms, 'origin': 'viewport', 'x': int(x),
                     'y': int(y)})
    if pressed:
        acts.append({'type': 'pointerUp', 'button': 0})
    b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                      'parameters': {'pointerType': 'mouse'}, 'actions': acts}]})


def line(x0, y0, x1, y1, n):
    return [(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(n + 1)]


def dial(b):
    return b.ev("(() => { const k = document.querySelector('sv-knob').geometry();"
                " return [k.cx, k.cy, k.radius]; })()")


def knob(b):
    return b.ev("(() => { const k = document.querySelector('sv-knob');"
                " return { raw: k.raw, lo: k.lo, hi: k.hi }; })()")


def dot_of(b):
    """Where the knob's dot is now, from its own angle (as the page computes it)."""
    return b.ev("(() => { const k = document.querySelector('sv-knob'), g = k.geometry();"
                " const a = k.angle(k.raw) * Math.PI / 180;"
                " return [g.cx + g.radius * Math.sin(a), g.cy - g.radius * Math.cos(a)]; })()")


def run():
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        load(b, dac.url, wait=3.0)
        cx, cy, r = dial(b)
        print('dial centre', round(cx), round(cy), 'radius', round(r))
        rows = []
        spots = [('core', cx, cy, 40), ('plume', cx, cy - 180, 60),
                 ('edge', cx + 190, cy + 100, 60)]
        for rep in range(int(os.environ.get('REPS', '2'))):
            for name, x, y, amp in spots:
                do(b, 'window.__trStart()')
                t0 = time.time()
                while time.time() - t0 < 12:
                    flick(b, secs=0.5, cx=x, cy=y, amp=amp)
                    if val(b, 'window.__tr.hit !== null', False):
                        break
                th = val(b, 'window.__thin(window.__fx.readback().atlas, 25)', None)
                if th is None:
                    print(name, 'no stamp'); continue
                n = th['note']
                print('%-5s note(%4.0f,%4.0f) glyph %d size %2.0f | glyph px %4d halo px %4d | '
                      'dye before %.3f '
                      '(drawn %.3f of its ceiling) | opacity glyph x%.3f halo x%.3f | glyph light '
                      '%+.3f'
                      % (name, n['x'], n['y'], n['glyph'], n['size'], th['glyphPx'], th['haloPx'],
                         th['dyeBefore'], th['drawnBefore'], th['glyphOpacity'], th['haloOpacity'],
                         th['glyphLight']))
                rows.append((name, th))
                b.sleep(4.3)
    finally:
        b.close()
        dac.stop()


run()
