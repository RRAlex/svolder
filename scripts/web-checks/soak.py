# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A soak of the smoke, by hand (not part of run-all.sh).

    python3 soak.py chromium [minutes] [out.json]

It runs 30 minutes by default and compares every 5-minute sample with the same minute of
soak-baseline.json, a recorded 30-minute run: the frame rate within 1 frame a second and the mean
alpha within 15 %; the largest |velocity| within 25 % is reported, not failing (a noisy maximum).
The exit code is 1 on a frame rate or alpha miss or a page error, else 0.

Loads the web UI against a fake daemon, leaves it alone for the given minutes (default 30) and,
every 5 minutes, logs the frame rate (frames a second), the whole-canvas mean alpha (0-255 scale,
as haze.alphas reads it) and the largest |velocity| of the fluid grid. A final line lists the page
errors. With an output path it also writes the samples and the commit as JSON.
"""
import json
import os
import subprocess
import sys

FPS_TOLERANCE = 1.0  # frames a second
ALPHA_TOLERANCE = 0.15  # share of the baseline's mean alpha
VELOCITY_TOLERANCE = 0.25  # share of the baseline's largest |velocity| (reported only)

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
minutes = int(sys.argv[2]) if len(sys.argv) > 2 else 30
out = sys.argv[3] if len(sys.argv) > 3 else None
sys.argv = ['haze.py', browser]
import haze, common  # noqa: E402,E401

STEP = 5


def compare(samples, baseline):
    """Print each sample against the baseline's of the same minute; the number of frame rate and
    alpha misses (the largest |velocity| is only reported)."""
    base = {x['minute']: x for x in baseline}
    misses = 0
    for x in samples:
        was = base.get(x['minute'])
        if not was:
            continue
        fps_ok = abs(x['fps'] - was['fps']) <= FPS_TOLERANCE
        alpha_ok = abs(x['mean_alpha'] - was['mean_alpha']) <= ALPHA_TOLERANCE * was['mean_alpha']
        v_off = x['max_v'] is None or abs(x['max_v'] - was['max_v']) > VELOCITY_TOLERANCE * was[
            'max_v']
        misses += (not fps_ok) + (not alpha_ok)
        print(f'compare {x["minute"]:2d} min: fps {x["fps"]} vs {was["fps"]} '
              f'{"ok" if fps_ok else "MISS"}; mean alpha {x["mean_alpha"]} vs {was["mean_alpha"]} '
              f'{"ok" if alpha_ok else "MISS"}; max |v| {x["max_v"]} vs {was["max_v"]:.2f} '
              f'{"(outside 25 %, reported only)" if v_off else "ok"}')
    return misses
commit = subprocess.run(['git', '-C', here, 'rev-parse', '--short', 'HEAD'],
                        capture_output=True, text=True).stdout.strip()
samples = []
dac = common.Daemon('gungnir2-v2')
b = haze.make(size=(1280, 900))
try:
    haze.load(b, dac.url, wait=0.5)
    haze.do(b, """window.__vmax = () => { const r = window.__fx.readback(); if (!r) return null;
      const v = r.vel.data; let m = 0;
      for (let i = 0; i < v.length; i += 4) m = Math.max(m, Math.hypot(v[i], v[i + 1]));
      return m; };""")
    f0 = haze.val(b, 'window.__frames()')
    for k in range(1, minutes // STEP + 1):
        b.sleep(STEP * 60 - 4)
        f1 = haze.val(b, 'window.__frames()')
        a = haze.alphas(b, [[0, 0, 1280, 900]])[0]
        vmax = haze.val(b, 'window.__vmax()')
        fps = (f1 - f0) / (STEP * 60 - 4)
        samples.append({'minute': STEP * k, 'fps': round(fps, 1), 'mean_alpha': round(a['mean'], 3),
                        'max_alpha': a['max'], 'max_v': vmax})
        print(f'soak {STEP * k:2d} min: {fps:.1f} frames/s, whole-canvas mean alpha '
              f'{a["mean"]:.3f} (max {a["max"]}), max |v| {vmax}', flush=True)
        f0 = haze.val(b, 'window.__frames()')
    errs = haze.errors(b)
    print('errors', errs)
    bad = compare(samples, json.load(open(os.path.join(here, 'soak-baseline.json')))['samples'])
    if out:
        with open(out, 'w') as f:
            json.dump({'licence_1': 'SPDX-FileCopyrightText: 2026 the svolder authors',
                       'licence_2': 'SPDX-License-Identifier: GPL-3.0-or-later',
                       'commit': commit, 'browser': browser, 'samples': samples,
                       'errors': errs}, f, indent=2)
    if bad or errs:
        print(f'soak: FAIL ({bad} frame rate or alpha misses, {len(errs)} page errors)')
        sys.exit(1)
    print('soak: within the baseline')
finally:
    b.close()
    dac.stop()
