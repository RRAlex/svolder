# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""What stirs the smoke in Chromium or Firefox, numbers only (readPixels, never a picture
of Smoke). Usage: python3 sources.py chromium|firefox
Checks: a pointer stroke (pressed and not), listeners that never interfere, the dial's drag still
turning the knob, a volume write puffing at the dot, no puff on a load / device switch / reconnect,
a long glide and a hard stroke staying finite and bounded (float readback of the velocity and dye
grids), the amp's meters emitting at each meter's centre."""
import json
import os
import sys
import urllib.request

import common

HARD_SECS = 6  # how long the same place is stroked hard and pressed (s)
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
window.__pd = 0; window.__cap = 0;
const pd0 = Event.prototype.preventDefault;
Event.prototype.preventDefault = function (...a) { window.__pd++; return pd0.apply(this, a); };
if (Element.prototype.setPointerCapture) {
  const sc = Element.prototype.setPointerCapture;
  Element.prototype.setPointerCapture = function (...a) { window.__cap++; return sc.apply(this, a);
    };
}
window.__lis = [];
window.__tlis = [];
for (const kind of ['addEventListener', 'removeEventListener']) {
  const orig = window[kind].bind(window);
  window[kind] = (type, fn, opts) => {
    if (type === 'pointermove') window.__lis.push([kind, opts === undefined ? null
      : typeof opts === 'object' ? { passive: opts.passive === true } : opts]);
    if (type === 'touchstart' || type === 'touchmove') window.__tlis.push([kind, type,
      typeof opts === 'object' ? { passive: opts.passive === true } : null]);
    return orig(type, fn, opts);
  };
}
window.__sleep = (ms) => new Promise((r) => setTimeout(r, ms));
window.__mv = (id, x, y) => new PointerEvent('pointermove', { pointerId: id, clientX: x,
  clientY: y, buttons: 0 });
// one pointermove whose getCoalescedEvents lists `n` moves, `ms` apart
window.__coal = async (id, n, ms) => {
  const list = [];
  for (let i = 0; i < n; i++) { list.push(window.__mv(id, 300 + i * 6, 500));
    await window.__sleep(ms); }
  const e = window.__mv(id, 300 + n * 6, 500); e.getCoalescedEvents = () => list;
  window.dispatchEvent(e);
};
window.__st = () => (window.__fx && window.__fx.stats ? window.__fx.stats() : null);
// a float readback of the grids, kept for the measures below
window.__snap = () => {
  const r = window.__fx && window.__fx.readback ? window.__fx.readback() : null;
  const c = document.querySelector('canvas.smoke');
  window.__g = r && c ? { r, cw: c.clientWidth, ch: c.clientHeight } : null;
  return window.__g !== null;
};
window.__cells = (grid, x0, y0, x1, y1) => {
  const g = window.__g, t = g.r[grid];
  const X0 = Math.max(0, Math.floor(x0 / g.cw * t.w)), X1 = Math.min(t.w,
    Math.ceil(x1 / g.cw * t.w));
  const Y0 = Math.max(0, Math.floor((1 - y1 / g.ch) * t.h)), Y1 = Math.min(t.h,
    Math.ceil((1 - y0 / g.ch) * t.h));
  return { t, X0, X1, Y0, Y1 };
};
// the sum of the dye amount (its alpha channel) over a rectangle in CSS px, from the top left
window.__dye = (x0, y0, x1, y1) => {
  const { t, X0, X1, Y0, Y1 } = window.__cells('dye', x0, y0, x1, y1); let s = 0;
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) s += t.data[(y * t.w + x) * 4 + 3];
  return s;
};
// the velocity over a rectangle: the mean and the greatest of each component (cells/s, y up)
window.__vel = (x0, y0, x1, y1) => {
  const { t, X0, X1, Y0, Y1 } = window.__cells('vel', x0, y0, x1, y1);
  let sx = 0, sy = 0, n = 0, mx = -1e9, Mx = -1e9, my = -1e9, My = -1e9;
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
    const vx = t.data[(y * t.w + x) * 4], vy = t.data[(y * t.w + x) * 4 + 1];
    sx += vx; sy += vy; n++; mx = Math.max(mx, vx); Mx = Math.max(Mx, -vx); my = Math.max(my, vy);
    My = Math.max(My, -vy);
  }
  return { mx: sx / n, my: sy / n, maxx: mx, maxy: my, minx: -Mx, miny: -My };
};
window.__bounds = () => {
  const g = window.__g; let bad = 0, maxV = 0, maxDye = 0, dye = 0;
  for (const k of ['vel', 'dye']) for (const v of g.r[k].data) if (!Number.isFinite(v)) bad++;
  const v = g.r.vel.data;
  for (let i = 0; i < v.length; i += 4) maxV = Math.max(maxV, Math.hypot(v[i], v[i + 1]));
  const d = g.r.dye.data;
  for (let i = 0; i < d.length; i += 4) { maxDye = Math.max(maxDye, d[i + 3]); dye += d[i + 3]; }
  return { bad, maxV, maxDye, dye, cells: d.length / 4 };
};
// the canvas's own alpha, summed over a rectangle in CSS px, read in a frame after the smoke's own
window.__alpha = (x0, y0, x1, y1) => new Promise((done) => requestAnimationFrame(() => {
  const c = document.querySelector('canvas.smoke'), gl = c.getContext('webgl2');
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  const w = gl.drawingBufferWidth, h = gl.drawingBufferHeight, k = w / c.clientWidth;
  const px = new Uint8Array(w * h * 4);
  gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
  let s = 0;
  for (let y = Math.round(y0 * k); y < Math.min(h, Math.round(y1 * k)); y++)
    for (let x = Math.round(x0 * k); x < Math.min(w, Math.round(x1 * k)); x++)
      s += px[((h - 1 - y) * w + x) * 4 + 3];
  done(s);
}));
"""


def load(b, url, wait=3.0):
    b.goto(url, settle=0.8)
    do(b, PAGE)
    for _ in range(50):
        if val(b, 'window.__fx !== null', False):
            break
        b.sleep(0.1)
    val(b, 'window.__fx.haze(false)')  # what a stroke or a step leaves is read without the haze
    b.sleep(wait)


def alpha(b, rect):
    """The canvas's summed alpha over `rect`, by readPixels in an animation frame."""
    return pg(b, 'window.__alpha(%s)' % ', '.join(str(v) for v in rect))


def snap(b):
    return bool(val(b, 'window.__snap()', False))


def dye(b, rect):
    if not snap(b):
        return float('nan')
    return val(b, 'window.__dye(%s)' % ', '.join(str(v) for v in rect), float('nan'))


def stats(b):
    return val(b, 'window.__st()', None) or {}


def splats(b, kind):
    return (stats(b).get('splats') or {}).get(kind, -1)


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
    two = common.Daemon('gungnir2-v2', more=[('Second', '1112131415161718')])
    b = make(size=(1280, 900))
    try:
        # ---- the pointer: velocity always, dye only when pressed
        load(b, dac.url)
        s0 = stats(b)
        check('the handle reports its splat counts (stats().splats)', 'splats' in s0, s0)
        cx, cy, r = dial(b)
        band = (60, 660, 420, 760)
        base = dye(b, band)
        stroke(b, line(80, 700, 400, 700, 20), False)
        d_free = dye(b, band) - base
        check('an unpressed stroke adds no dye (the band holds < 0.5 more than before)',
              abs(d_free) < 0.5, d_free)
        a0 = alpha(b, band)
        stroke(b, line(80, 720, 400, 720, 20), True)
        a1 = alpha(b, band)
        p_press = splats(b, 'pointer')
        print(f'      pressed stroke: pointer splats {p_press}; canvas alpha in the band '
              f'{a0} -> {a1}')
        check('a pressed stroke is counted and the canvas shows its dye (readPixels alpha in the '
              'band at least doubles)', p_press > 0 and a1 > 2 * max(a0, 200), [p_press, a0, a1])
        pr = stats(b)
        check('  the stroke made no preventDefault call and no pointer capture on the page',
              pg(b, 'window.__pd') == 0 and pg(b, 'window.__cap') == 0, [pg(b, 'window.__pd'),
                                                                         pg(b, 'window.__cap')])
        bad = val(b, 'window.__snap() ? window.__bounds().bad : -1', -1)
        check(
            '  no non-finite texel in the velocity and dye grids after the strokes',
            bad == 0,
            bad,
        )

        # ---- the page's own source: passive listeners, none of the forbidden calls
        src = urllib.request.urlopen(dac.url + 'assets/themes/ember/smoke.js').read().decode()
        check('smoke.js adds pointermove on window, passive; no preventDefault, no '
              'setPointerCapture',
              "window.addEventListener('pointermove'" in src and 'passive: true' in src
              and 'preventDefault' not in src and 'setPointerCapture' not in src
              and "removeEventListener('pointermove'" in src)

        # ---- the dial turns by a drag, and the drag stirs the smoke
        cx, cy, r = dial(b)
        k0 = knob(b)
        p0 = splats(b, 'pointer')
        stroke(b, line(cx, cy, cx, cy - 70, 14), True)
        b.sleep(1.0)
        k1 = knob(b)
        p1 = splats(b, 'pointer')
        check('a drag on the dial centre turns the knob up (the reading changes)',
              k1['raw'] is not None and k0['raw'] is not None and k1['raw'] > k0['raw'], [k0, k1])
        check('  and the drag stirred the smoke (pointer splats counted)', p1 > p0, [p0, p1])
        # the same place stroked hard and pressed for 10 s: bounded and finite
        t_end = __import__('time').time() + HARD_SECS
        zig = []
        for i in range(8):
            zig += line(cx - 200 + (i % 2) * 400, cy - 150 + i * 40, cx - 200 + ((i + 1) % 2) * 400,
                        cy - 130 + i * 40, 6)
        while __import__('time').time() < t_end:
            stroke(b, zig, True, ms=8)
        snap(b)
        rep = val(b, 'window.__bounds()', {}) or {}
        print(f'      {HARD_SECS} s of hard pressed strokes over the same place: max |v| '
              f'{rep.get("maxV", 0):.1f}, '
              f'max dye {rep.get("maxDye", 0):.3f}, dye sum {rep.get("dye", 0):.0f}, non-finite '
              f'{rep.get("bad")}')
        check(f'repeated hard strokes over one place for {HARD_SECS} s: finite, max |v| < 75 '
              'cells/s (55 measured; 89 without the cap), max dye < 1',
              rep.get('bad') == 0 and rep.get('maxV', 1e9) < 75 and rep.get('maxDye', 9) < 1,
              rep)

        # synthetic pointer events: coalesced moves are used, a pause starts a fresh stroke, and a
        # burst in one frame is bounded
        def pointer_splats():
            return splats(b, 'pointer')
        n0 = pointer_splats()
        pg(b, 'window.__coal(77, 4, 12)')
        b.sleep(0.4)
        n1 = pointer_splats()
        check('a pointermove with 4 coalesced events counts 3 pushes (the first has no earlier '
              'move to measure from)', n1 - n0 == 3, [n0, n1])
        n2 = pg(b, '''(async () => {
          const at = () => window.__st().splats.pointer, mv = window.__mv, s = window.__sleep;
          const c0 = at();
          window.dispatchEvent(mv(79, 100, 600)); await s(300);
          window.dispatchEvent(mv(79, 200, 600)); await s(20);
          window.dispatchEvent(mv(79, 215, 600)); await s(150);
          return at() - c0; })()''')
        n3 = pointer_splats()
        n2, n1 = n2, 0
        check('a pause of 300 ms between two moves starts a fresh stroke (no push), the next move '
              'after 20 ms does push (one push in all)', n2 == 1, n2)
        pg(b, 'window.__coal(80, 60, 3)')
        b.sleep(0.5)
        n4 = pointer_splats()
        print(
            f'      a pointermove with 60 coalesced events in one frame: {n4 - n3} pushes applied',
        )
        check('a burst of 60 moves in one frame applies at most maxPointerSplats = 12 pushes '
              '(the newest)', n4 - n3 == 12, [n3, n4])
        p0 = splats(b, 'puff')
        do(b, 'for (let i = 0; i < 50; i++) window.__fx.step(1, 0.05, 0.3);')
        b.sleep(0.5)
        p1 = splats(b, 'puff')
        check('a burst of 50 volume steps in one frame applies at most maxPuffs = 8 puffs',
              p1 - p0 == 8, [p0, p1])
        errs = errors(b)
        check('no page error so far', errs == [], errs)
        b.close()

        # the same drag with the effects off moves the knob the same way
        off = common.Daemon('gungnir2-v2')
        b = make(size=(1280, 900))
        load(b, off.url + '?effects=off', wait=1.0)
        cx, cy, r = dial(b)
        ko0 = knob(b)
        stroke(b, line(cx, cy, cx, cy - 70, 14), True)
        b.sleep(1.0)
        ko1 = knob(b)
        check('  the same drag with effects off gives the same step as with Smoke',
              ko1['raw'] - ko0['raw'] == k1['raw'] - k0['raw'], [ko0, ko1, k0, k1])
        b.close()
        off.stop()

        # ---- a volume write puffs at the dot; a load does not
        dac.write('Gungnir 2', 'volume', -50)
        b = make(size=(1280, 900))
        load(b, dac.url, wait=3.0)
        pf0 = splats(b, 'puff')
        check('a page load is not a step: no puff after it', pf0 == 0, pf0)
        cx, cy, r = dial(b)
        dot = dot_of(b)
        win = lambda d: (d[0] - 45, d[1] - 45, d[0] + 45, d[1] + 45)
        rest = [dye(b, win(dot)) for _ in range(3)]
        rest_mean = sum(rest) / 3
        dac.write('Gungnir 2', 'volume', -8)
        peak = 0
        seen = []
        for _ in range(30):
            b.sleep(0.1)
            d = dot_of(b)
            v = dye(b, win(d))
            peak = max(peak, v) if v == v else peak
            seen.append(round(v, 1))
        pf1 = splats(b, 'puff')
        print(f'      volume -50 -> -8 dB (dot at {tuple(round(v) for v in dot_of(b))}, the arc '
              f'top at {round(cx)}, {round(cy - r)}): puffs {pf1}; dye in a 90 px window at the '
              f'dot: '
              f'resting '
              f'{rest_mean:.1f}, peak over the glide {peak:.1f}')
        check('a volume write is a step: puffs are counted', pf1 > 0, pf1)
        check('  the dye at the dot rises above its resting level (peak > 1.3 x resting)',
              rest_mean == rest_mean and peak > 1.3 * rest_mean > 0, [rest_mean, peak, seen])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- a device switch is not a step either (the second device reads another volume)
        two.write('Gungnir 2', 'volume', -50)
        b = make(size=(1280, 900))
        load(b, two.url, wait=2.0)
        pa = splats(b, 'puff')
        b.ev("document.querySelectorAll('.devs a.dev')[1].click()")
        b.sleep(2.5)
        pb = splats(b, 'puff')
        two.write('Gungnir 2', 'volume', -30)
        b.ev("document.querySelectorAll('.devs a.dev')[0].click()")
        b.sleep(2.5)
        pc = splats(b, 'puff')
        print(f'      puffs: after the load {pa}, after switching to the second device {pb}, '
              f'back to the first (written meanwhile) {pc}')
        check('a device switch (to another volume, and back to a volume written meanwhile) '
              'makes no puff', pa == 0 and pb == 0 and pc == 0, [pa, pb, pc])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- a reconnect (the daemon restarted, the page's stream reopens) is not a step
        d3 = common.Daemon('gungnir2-v2')
        try:
            d3.write('Gungnir 2', 'volume', -45)
            b = make(size=(1280, 900))
            load(b, d3.url, wait=2.0)
            ka = knob(b)
            d3.restart()
            b.sleep(7.0)
            kb = knob(b)
            pr = splats(b, 'puff')
            print(f'      daemon restarted: reading {ka["raw"]} -> {kb["raw"]}, puffs {pr}')
            check('the first reading after a reconnect is not a step (no puff, even when it '
                  'differs from the one before the loss)', pr == 0, [ka, kb, pr])
            other = [e for e in errors(b) if not any(k in e for k in ('ERR_', 'Failed to load',
                                                                      'EventSource'))]
            check('  no page error beyond the lost stream', other == [], other)
            b.close()
        finally:
            d3.stop()

        # ---- Chromium only: the stream's own handlers called with made-up readings, so that each
        # of the page's forgetting points (a new model, a lost link, a stream opening) is
        # checked on its own; the real device switch and reconnect above cover the others
        if browser == 'chromium':
            b = make(size=(1280, 900))
            b.send('Page.addScriptToEvaluateOnNewDocument', {'source': """
              window.__hs = {}; window.__lastVol = null;
              const add = EventSource.prototype.addEventListener;
              EventSource.prototype.addEventListener = function (type, fn, ...rest) {
                let f = fn;
                if (type === 'volume') f = function (e) { window.__lastVol = e.data;
                  return fn.call(this, e); };
                window.__hs[type] = f;
                return add.call(this, type, f, ...rest);
              };
              window.__vol = (raw) => { const d = JSON.parse(window.__lastVol); d.raw = raw;
                window.__hs.volume({ data: JSON.stringify(d) }); };"""})
            load(b, dac.url, wait=2.5)

            def puffs_after(js):
                b.ev('window.__vol(30); window.__vol(40); 0')  # the reading to step from
                b.sleep(0.4)
                before = splats(b, 'puff')
                b.ev(js + '; 0')
                b.sleep(0.4)
                return splats(b, 'puff') - before
            ctl = puffs_after('window.__vol(40); window.__vol(50)')
            page = b.page("import(new URL('assets/core/boot.js', document.baseURI).href)"
                        ".then((m) => JSON.stringify(m.core.model.page()))")
            swp = puffs_after("window.__vol(40); window.__hs.model({ data: "
                              f"{json.dumps(page)} }}); window.__vol(50)")
            opn = puffs_after('window.__vol(40); window.__hs.open({}); window.__vol(50)')
            lst = puffs_after('window.__vol(40); window.__hs.error({}); window.__vol(50)')
            print(f'      made-up readings 40 then 50: plain {ctl}, with a model between '
                  f'{swp}, with the stream opening {opn}, with a lost link {lst}')
            check(
                'two readings in a row are a step (the made-up path works: one puff)',
                ctl == 1,
                ctl,
            )
            check('  a new model between two readings: the next is not a step', swp == 0, swp)
            check('  the stream opening between them: not a step', opn == 0, opn)
            check('  a lost link between them: not a step', lst == 0, lst)
            b.sleep(3.0)
            b.close()

        # ---- a long glide and a hard stroke stay finite and bounded
        b = make(size=(1280, 900))
        load(b, dac.url, wait=8.0)  # the resting cloud fills out without a plume: let it settle
        snap(b)
        rest_b = val(b, 'window.__bounds()', {}) or {}
        # 40 steps, one every 50 ms, up the dial (the page's own step entry)
        do(b, """window.__done = false; let n = 0;
          const t = setInterval(() => { window.__fx.step(1, 0.01, 0.2 + n * 0.015);
            if (++n >= 40) { clearInterval(t); window.__done = true; } }, 50);""")
        b.sleep(2.6)
        snap(b)
        glide = val(b, 'window.__bounds()', {}) or {}
        p40 = splats(b, 'puff')
        print(f'      40-step glide (rest: max |v| {rest_b.get("maxV", 0):.1f}, dye sum '
              f'{rest_b.get("dye", 0):.1f}): puffs {p40}, max |v| {glide.get("maxV", 0):.1f}, '
              f'dye sum {glide.get("dye", 0):.1f}, max dye {glide.get("maxDye", 0):.3f}, '
              f'non-finite {glide.get("bad")}')
        # (a frame that stalls under load keeps only the newest maxPuffs, so a few may be dropped)
        check('a 40-step glide: at least 30 of its 40 puffs applied, every texel finite',
              30 <= p40 <= 40 and glide.get('bad') == 0, [p40, glide])
        check('  the glide\'s cloud stays a stream: the dye sum under 1.4 x the resting cloud + 30 '
              'and max |v| under 60 cells/s',
              glide.get('dye', 1e9) < 1.4 * rest_b.get('dye', 0) + 30
              and glide.get('maxV', 1e9) < 60,
              [rest_b, glide])
        # a worst case: 40 big steps, one every frame
        do(b, """for (let n = 0; n < 40; n++) setTimeout(() =>
          window.__fx.step(n % 2 ? 1 : -1, 0.5, 0.5), n * 16);""")
        b.sleep(2.0)
        snap(b)
        worst = val(b, 'window.__bounds()', {}) or {}
        print(f'      40 full-size steps, one per frame: max |v| {worst.get("maxV", 0):.1f}, '
              f'dye sum {worst.get("dye", 0):.1f}, non-finite {worst.get("bad")}')
        check('  40 full-size steps, one per frame: finite, max |v| < 60, the dye sum under '
              '2.2 x the resting cloud',
              worst.get('bad') == 0 and worst.get('maxV', 1e9) < 60
              and worst.get('dye', 1e9) < 2.2 * rest_b.get('dye', 0), [worst, rest_b])
        # a long hard stroke: pressed, zig-zag, 6 s
        pts = []
        for i in range(12):
            pts += line(
                80 + (i % 2) * 1100,
                100 + i * 60,
                80 + ((i + 1) % 2) * 1100,
                130 + i * 60,
                10,
            )
        stroke(b, pts, True, ms=8)
        snap(b)
        hard = val(b, 'window.__bounds()', {}) or {}
        ph = splats(b, 'pointer')
        print(f'      long hard pressed stroke ({len(pts)} moves): pointer splats {ph}, max |v| '
              f'{hard.get("maxV", 0):.1f}, max dye {hard.get("maxDye", 0):.3f}, dye sum '
              f'{hard.get("dye", 0):.1f}, non-finite {hard.get("bad")}')
        check('a long hard stroke: every texel finite, max |v| < 70 cells/s (46 measured; '
              '104 without the cap)', hard.get('bad') == 0 and hard.get('maxV', 1e9) < 70, hard)
        check('  its dye stays bounded (max dye under 5)', hard.get('maxDye', 1e9) < 5, hard)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- an amp: the meters emit at their centres
        amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
        quiet = common.Daemon('fake-amp', name='Amp')
        try:
            res = {}
            for tag, d in (('signal', amp), ('quiet', quiet)):
                b = make(size=(1280, 900))
                load(b, d.url, wait=4.0)
                cs = b.ev("[...document.querySelectorAll('.meters figure.meter')].map((f) => {"
                          " const r = f.getBoundingClientRect();"
                          " return [r.left + r.width / 2, r.top + r.height / 2]; })")
                sums = [dye(b, (x - 40, y - 40, x + 40, y + 40)) for x, y in cs]
                order = b.ev("(() => { const r = document.querySelector('sv-meters').readings();"
                             " const p = [...document.querySelectorAll('figure.meter .m-pct')]"
                             ".map((e) => Number(e.textContent)); return [r, p]; })()")
                mk = pg(b, """(() => { const m = document.querySelector('sv-meters');
                  const keys = [...document.querySelectorAll('figure.meter')]
                    .map((f) => f.dataset.key);
                  m.update({ live: true, state: '', meters: keys.map((key, i) => ({ key,
                    fraction: 0.2 + 0.3 * i, text: String(20 + 30 * i) })) });
                  return [m.readings(), keys.map((k, i) => 0.2 + 0.3 * i)]; })()""")
                res[tag] = (cs, sums, splats(b, 'meter'), errors(b), order, mk)
                b.close()
            cs, hot, nh, e1, order, mk = res['signal']
            _, cold, nc, e2, _, _ = res['quiet']
            print(f'      amp: meters at {[(round(x), round(y)) for x, y in cs]}; '
                  f'dye at each centre '
                  f'with the signal {[round(v, 1) for v in hot]}, '
                  f'without {[round(v, 1) for v in cold]};'
                  f' meter splats {nh} / {nc}')
            check('amp: the meters emit (splats counted with the signal, none without)',
                  nh > 0 and nc == 0, [nh, nc])
            check('  the dye at every meter\'s centre is > 1.5 x what the resting source gives '
                  'there '
                  'without a signal', len(cs) > 0 and all(h > 1.5 * c for h, c in zip(hot, cold)),
                  [hot, cold])
            check('  no page error', e1 == [] and e2 == [], [e1, e2])
            check('  readings() after an update with distinct fractions is in figure order',
                  mk[0] == mk[1], mk)
            rd, pc = order
            check('  readings() gives the fractions in figure order (each x 100 matches its '
                  'figure\'s percentage text, to 1)', len(rd) == len(cs) == len(pc) and all(
                      r is not None and abs(r * 100 - p) <= 1 for r, p in zip(rd, pc)), order)

            # a handle of its own: a list longer or shorter than the anchor's meters uses the
            # common prefix
            b = make(size=(1280, 900))
            load(b, amp.url + '?effects=off', wait=0.5)
            do(b, """const c = document.createElement('canvas');
              for (const [k, v] of [['position', 'fixed'], ['left', '0'], ['top', '0'],
                ['width', '100%'], ['height', '100lvh'], ['pointer-events', 'none']])
                c.style.setProperty(k, v);
              c.className = 'smoke'; document.body.append(c);
              import(new URL('assets/themes/ember/smoke.js', document.baseURI).href).then((m) => {
                window.__h = m.start(c, { glow: ['#8b5cf6', '#6d3fd1', '#b9a0ff'], bg: '#08070c' });
                window.__h.haze(false);
                window.__h.level(0.8);
                window.__h.anchor({ x: 640, y: 850, radius: 100, meters: [{ x: 250, y: 450 },
                  { x: 640, y: 450 }, { x: 1030, y: 450 }] });
                window.__h.meters([0.9, 0]); });""")
            b.sleep(3.0)
            win = lambda x: (x - 40, 410, x + 40, 490)
            # `__fx` reads the page's own smoke, so a handle of its own is read directly
            g = pg(b, "(() => { const r = window.__h.readback(); const c = document.querySelector("
                     "'canvas.smoke'); const t = r.dye; const out = [];"
                     " for (const x of [250, 640, 1030]) { let s = 0;"
                     " for (let yy = 410; yy < 490; yy++) for (let xx = x - 40; xx < x + 40; "
                     "xx++) {"
                     " const gx = Math.floor(xx / c.clientWidth * t.w);"
                     " const gy = Math.floor((1 - yy / c.clientHeight) * t.h);"
                     " s += t.data[(gy * t.w + gx) * 4 + 3]; } out.push(s); } return out; })()")
            print(f'      a list of two readings for three meters: dye at their centres {g}')
            check('a list shorter than the anchor\'s meters uses the common prefix (the first '
                  'meter\'s dye > 4 x each of the other two)',
                  g[0] > 4 * g[1] and g[0] > 4 * g[2], g)
            pg(b, "window.__h.meters([0.9, 0.9, 0.9, 0.9, 0.9]) || 0")
            pg(b, "window.__h.anchor({ x: 640, y: 450, radius: 100, meters: [{ x: 640, y: 450 }] })"
                 " || 0")
            b.sleep(1.5)
            check('a list longer than the anchor\'s meters is accepted (no page error)',
                  errors(b) == [], errors(b))
            # a stale reading must not emit after the anchor changes (an amp, a DAC, an amp)
            pg(b, "window.__h.level(0.8) || window.__h.meters([1, 1]) || "
                  "window.__h.anchor({ x: 640,"
                  " y: 450, radius: 100, meters: [{ x: 300, y: 450 }, { x: 980, y: 450 }] })")
            b.sleep(1.2)
            m0 = pg(b, 'window.__h.stats().splats.meter')
            pg(b, "window.__h.anchor({ x: 640, y: 450, radius: 100, meters: [] }) || 0")
            b.sleep(0.3)
            pg(b, "window.__h.anchor({ x: 640, y: 450, radius: 100, meters: [{ x: 300, y: 450 },"
                  " { x: 980, y: 450 }] }) || 0")
            m1 = pg(b, 'window.__h.stats().splats.meter')
            b.sleep(0.6)
            m2 = pg(b, 'window.__h.stats().splats.meter')
            print(
                f'      meter splats: before the anchor left {m0}, after coming back {m1} -> {m2}',
            )
            check('after the anchor loses its meters and gets them back, no meter emits until a '
                  'fresh '
                  'meters call (the eased readings were forgotten)', m0 > 0 and m2 == m1,
                  [m0, m1, m2])
            pg(b, "window.__h.meters([1, 1]) || 0")
            b.sleep(1.0)
            pg(b, "window.__h.away(true) || 0")
            b.sleep(0.3)
            pg(b, "window.__h.away(false) || 0")
            b.sleep(0.2)
            k1 = pg(b, 'window.__h.stats().splats.meter')
            b.sleep(0.6)
            k2 = pg(b, 'window.__h.stats().splats.meter')
            check(
                'after an away spell no meter emits until a fresh meters call',
                k2 == k1,
                [k1, k2],
            )
            stroke(b, line(100, 600, 500, 600, 12), False)
            a0 = pg(b, 'window.__h.stats().splats.pointer')
            pg(b, 'window.__h.away(true) || 0')
            stroke(b, line(100, 650, 500, 650, 12), False)
            a1 = pg(b, 'window.__h.stats().splats.pointer')
            pg(b, 'window.__h.away(false) || 0')
            check('a stroke is counted, and while away it stirs nothing', a0 > 0 and a1 == a0,
                  [a0, a1])
            # the handle's pointer listener: added passive on the window, removed by stop
            before = pg(b, 'window.__lis')
            t_before = pg(b, 'window.__tlis')
            pg(b, 'window.__h.stop() || 0')
            after = pg(b, 'window.__lis')
            t_after = pg(b, 'window.__tlis')
            print(f'      pointermove listeners seen on window: {before} then, after stop, {after}')
            check('start adds one passive pointermove listener on the window, stop removes one',
                  before == [['addEventListener', {'passive': True}]]
                  and after[-1:] == [['removeEventListener', None]] and len(after) == 2,
                  [before, after])
            check('start adds passive touchstart and touchmove listeners on the window, stop '
                  'removes both',
                  t_before == [['addEventListener', 'touchstart', {'passive': True}],
                               ['addEventListener', 'touchmove', {'passive': True}]]
                  and sorted(t_after[2:]) == [['removeEventListener', 'touchmove', None],
                                              ['removeEventListener', 'touchstart', None]]
                  and len(t_after) == 4, [t_before, t_after])
            b.close()
        finally:
            amp.stop()
            quiet.stop()
    finally:
        b.close()
        dac.stop()
        two.stop()


run()
print(f'sources.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
