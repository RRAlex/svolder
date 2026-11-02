# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Tuning aid: copies the page's web dir, sets TUNING values, and prints the resting cloud's
numbers at several levels from readPixels (numbers only). Usage: python3 tune.py key=val ...
Env: SECS (settle time, default 4)."""
import os, re, shutil, sys, json
import common

web = common.scratch('tune-web-')
shutil.copytree(common.WEB, web + '/w')
p = web + '/w/themes/ember/smoke-model.js'
s = open(p).read()
for kv in sys.argv[1:]:
    k, v = kv.split('=')
    s, n = re.subn(rf'(\n  {k}: )[^,]+(,)', rf'\g<1>{v}\2', s)
    assert n == 1, k
open(p, 'w').write(s)
os.environ['SMOKE_WEB'] = web + '/w'
common.WEB = web + '/w'
core = open(os.path.join(os.path.dirname(__file__), 'smoke_core.py')).read()
exec(core.split('def run():')[0].replace(
    "sys.argv[1] if len(sys.argv) > 1 else 'chromium'", "'chromium'"))
d = common.Daemon('gungnir2-v2')
b = make(size=(1280, 900))
try:
    for lvl in (0.06, 0.5, 1.0):
        load(b, d.url + '?effects=off')
        b.ev(f'window.__mk({{ x: 640, y: 650, radius: 100, meters: [] }}, {lvl})')
        b.sleep(float(os.environ.get('SECS', '4')))
        read(b, '#tc', 12)
        peak = b.ev('window.__peak()')
        c = b.ev('window.__centroid()')
        # extent: rows/columns where alpha >= 10 % of the peak, through the peak
        m = b.ev("""(() => { const f = window.__frame('best'); let pk = 0, px = 0, py = 0;
          const A = (x, y) => f.px[((f.h-1-y)*f.w+x)*4+3];
          for (let y = 0; y < f.h; y++) for (let x = 0; x < f.w; x++) { const a = A(x, y);
            if (a > pk) { pk = a; px = x; py = y; } }
          let n = 0; for (let y = 0; y < f.h; y++) for (let x = 0; x < f.w; x++) if (A(x,
            y) >= pk * .1) n++;
          let lo = 255, hi = 0;
          for (let y = py - 48; y < py + 48; y++) for (let x = px - 48; x < px + 48; x++) {
            const a = A(x, y); lo = Math.min(lo, a); hi = Math.max(hi, a); }
          return { pk, r: Math.sqrt(n / Math.PI) / f.k, spread: (hi - lo) / 255,
                   at: [px / f.k, py / f.k] }; })()""")
        print(f'level {lvl}: peak {m["pk"]}/255 = {m["pk"]/255:.3f}; 10%-of-peak radius '
              f'{m["r"]:.0f} px; core spread (96x96 patch) {m["spread"]:.3f}; peak at '
              f'({m["at"][0]:.0f},{m["at"][1]:.0f}) '
              f'anchor (640,650)')
finally:
    b.close(); d.stop()
