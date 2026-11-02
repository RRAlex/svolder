# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The quick gate's one short smoke check, in Chromium or Firefox, numbers only: the page starts
Smoke (one canvas), the fluid grids read back finite and bounded after a pointer stroke, and the
loop rests at the calm rate. Usage: python3 smoke_quick.py chromium|firefox
It does not replace smoke_core.py, smoke_page.py or the others: `make web-checks` runs those."""
import sys

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
# the bounds of sources.py's long glide and hard stroke, which a short stroke stays well under
MAX_V = 75  # cells a second
MAX_DYE = 1


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(**kw):
    return common.Chromium(**kw) if browser == 'chromium' else common.Firefox(**kw)


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def run():
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        b.goto(dac.url + '?effects=smoke', settle=0.8)
        b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
               ".then((m) => { window.__fx = m; return 1; })")
        b.sleep(2.0)
        canvases = b.page("document.querySelectorAll('canvas.smoke').length")
        check('Smoke starts: one canvas', canvases == 1, canvases)
        pts = [(300 + 40 * i, 500 + 8 * (i % 3)) for i in range(25)]
        if browser == 'chromium':
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': pts[0][0],
                                                'y': pts[0][1]})
            for x, y in pts[1:]:
                b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
                b.sleep(0.016)
        else:
            acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport',
                     'x': pts[0][0], 'y': pts[0][1]}]
            acts += [{'type': 'pointerMove', 'duration': 16, 'origin': 'viewport', 'x': x, 'y': y}
                     for x, y in pts[1:]]
            b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                              'parameters': {'pointerType': 'mouse'},
                                              'actions': acts}]})
        b.sleep(0.5)
        grids = b.page("""(() => { const r = window.__fx.readback(); let bad = 0, maxV = 0,
          maxDye = 0; if (!r) return null;
          for (const k of ['vel', 'dye']) for (const v of r[k].data) if (!Number.isFinite(v)) bad++;
          const v = r.vel.data, d = r.dye.data;
          for (let i = 0; i < v.length; i += 4) maxV = Math.max(maxV, Math.hypot(v[i], v[i + 1]));
          for (let i = 0; i < d.length; i += 4) maxDye = Math.max(maxDye, d[i + 3]);
          return { bad, maxV, maxDye }; })()""")
        print(f'      after a stroke: {grids}')
        check('after a stroke every texel of both grids is finite and bounded',
              bool(grids) and grids['bad'] == 0 and grids['maxV'] < MAX_V
              and grids['maxDye'] < MAX_DYE, grids)
        b.sleep(3.0)
        f0 = b.page('window.__fx.stats().frames')
        b.sleep(3.0)
        rate = (b.page('window.__fx.stats().frames') - f0) / 3.0
        print(f'      calm rate: {rate:.1f} simulated frames a second')
        calm = b.page("import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href)"
                      '.then((m) => m.TUNING.calmFps)')
        lo, hi = 0.7 * calm, 1.3 * calm  # 14 to 26 at the approved 20
        check(f'rests at the calm rate ({lo:g} to {hi:g} simulated frames a second)',
              lo <= rate <= hi, rate)
        check('no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()


run()
print(f'smoke_quick.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
