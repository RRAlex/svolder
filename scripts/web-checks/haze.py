# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The smoke as the page's background in Chromium or Firefox, numbers only (readPixels and float
readbacks; never a picture of Smoke). Usage: python3 haze.py chromium|firefox [--baseline]
`--baseline` measures only the pace probe and writes it to smoke-baseline.json (the reference the
pace check compares against: a stroke must last longer and travel further than in that recording;
rerun it only to move the reference on purpose). Checks: the haze across the page at 2560x1440
(denser in the side bars, faint behind the column and under the Glow level's alpha, the calm rate,
a steady density from 20 s to 40 s, the volume leaving it alone, the dial's cloud brightening with
it, a resize and a device switch following the new margins within 2 s), the phone's edges, the
shade drawn per load, and the pace of a stroke against the recorded baseline."""
import io
import json
import math
import os
import subprocess
import sys

import common

browser = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith('--') else 'chromium'
BASELINE_ONLY = '--baseline' in sys.argv
BAND = [760, 700, 1800, 900]  # behind the column, between the dial's cloud and `bottom`
PACE_LOADS = 3  # the pace probe's page loads; the median of them is what is compared
PACE_WAIT = 5  # seconds a page runs before a stroke (a stroke beats the baseline by a wide margin)
CALM_SECS = 20  # how long the wide page is left untouched before its first reads (s)
STEADY_SECS = 40  # when the wide page's side bars are read the second time (s)
PHONE_SECS = 25  # a phone seed's wait before its edges are read (s)
STEADY_WAIT = 12  # how long the steadiness page runs untouched before it is sampled (s)
STEADY_READ = 10  # how long it is sampled (s)
STEADY_FACTOR = 10  # the side bands' frame-to-frame change, at most this many times the dial's
STEADY_ABS = 0.002  # and at most this share of their own level (0.0001 to 0.0007 now, 0.028 for a
# pulsed feed)
BASELINE_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'smoke-baseline.json')
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
    """An expression's value in the page's own realm (Firefox's WebDriver scripts run in a sandbox
    with module maps of their own, so there it goes through WebDriver BiDi)."""
    return b.page(expr)


def do(b, js):
    """Run statements in the page's realm."""
    return pg(b, f'(() => {{ {js} }})()')


def val(b, expr, default=None):
    """An expression's value, or `default` when it throws."""
    global shown
    try:
        return pg(b, expr)
    except Exception as e:  # noqa: BLE001
        if shown < 3:
            shown += 1
            print('      (error:', str(e)[:300].replace('\n', ' '), ')')
        return default


def errors(b):
    if browser == 'chromium':
        return b.errors
    try:
        return b.errors + (pg(b, 'window.__errs || []') or [])
    except Exception as e:  # noqa: BLE001
        return b.errors + [f'collector unreadable: {e}']


# The in-page helpers. `__keep(name)` stores the dye grid's amount channel (float readback);
# `__sum(name, base, rect)` and `__cen(name, base, rect)` read the excess of one store over
# another (`base` null: the store itself) over a rectangle in CSS px, from the top left.
PAGE = """
window.__fx = null;
import(new URL('assets/themes/ember/effects.js', document.baseURI).href).then((m) => { window.__fx = m; });
window.__store = {};
window.__keep = (name) => {
  const r = window.__fx.readback(); if (!r) return false;
  const c = document.querySelector('canvas.smoke');
  const t = r.dye, a = new Float32Array(t.w * t.h);
  for (let i = 0; i < a.length; i++) a[i] = t.data[i * 4 + 3];
  window.__store[name] = { a, w: t.w, h: t.h, cw: c.clientWidth, ch: c.clientHeight,
    at: performance.now() / 1000 };
  return true;
};
window.__cells = (s, x0, y0, x1, y1) => ({
  X0: Math.max(0, Math.floor(x0 / s.cw * s.w)), X1: Math.min(s.w, Math.ceil(x1 / s.cw * s.w)),
  Y0: Math.max(0, Math.floor((1 - y1 / s.ch) * s.h)),
  Y1: Math.min(s.h, Math.ceil((1 - y0 / s.ch) * s.h)) });
window.__sum = (name, base, x0, y0, x1, y1) => {
  const s = window.__store[name], b = base && window.__store[base];
  const { X0, X1, Y0, Y1 } = window.__cells(s, x0, y0, x1, y1); let sum = 0, n = 0;
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
    sum += s.a[y * s.w + x] - (b ? b.a[y * s.w + x] : 0); n++;
  }
  return { sum, n, mean: n ? sum / n : 0 };
};
window.__cen = (name, base, x0, y0, x1, y1) => {
  const s = window.__store[name], b = window.__store[base];
  const { X0, X1, Y0, Y1 } = window.__cells(s, x0, y0, x1, y1);
  let w = 0, sx = 0, sy = 0;
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
    const e = Math.max(0, s.a[y * s.w + x] - b.a[y * s.w + x]);
    w += e; sx += e * (x + 0.5); sy += e * (y + 0.5);
  }
  if (!(w > 0)) return null;
  return [sx / w / s.w * s.cw, (1 - sy / w / s.h) * s.ch, w];
};
window.__t = (name) => window.__store[name].at;
// the canvas's own alpha (0 to 255) over rectangles in CSS px, read in the animation frames after
// the smoke's own: [{ mean, max }, ...] of the frame with the most alpha (a calm frame that is not
// drawn leaves an empty buffer, so the best of `frames` is the drawn one)
window.__al = (rects, frames) => new Promise((done) => {
  let best = null, left = frames;
  const tick = () => {
    const c = document.querySelector('canvas.smoke'), gl = c.getContext('webgl2');
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    const w = gl.drawingBufferWidth, h = gl.drawingBufferHeight, k = w / c.clientWidth;
    const px = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    let total = 0;
    for (let i = 3; i < px.length; i += 4) total += px[i];
    if (!best || total > best.total) {
      best = { total, out: rects.map(([x0, y0, x1, y1]) => {
        let sum = 0, n = 0, max = 0;
        for (let y = Math.round(y0 * k); y < Math.min(h, Math.round(y1 * k)); y++)
          for (let x = Math.round(x0 * k); x < Math.min(w, Math.round(x1 * k)); x++) {
            const a = px[((h - 1 - y) * w + x) * 4 + 3]; sum += a; n++; if (a > max) max = a;
          }
        return { mean: n ? sum / n : 0, max };
      }) };
    }
    if (--left > 0) requestAnimationFrame(tick); else done(best.out);
  };
  requestAnimationFrame(tick);
});
window.__frames = () => window.__fx.stats().frames;
// the mean dye amount over rectangles (CSS px) at every smoke frame for `secs` seconds, read in
// the animation frames after the smoke's own: one series of [frame, mean] per rectangle
window.__steady = (rects, secs) => new Promise((done) => {
  const t0 = performance.now(), series = rects.map(() => []);
  let seen = -1;
  const tick = () => {
    const f = window.__fx.stats().frames;
    if (f !== seen) {
      seen = f;
      const r = window.__fx.readback();
      if (r) {
        const d = r.dye, c = document.querySelector('canvas.smoke');
        rects.forEach(([x0, y0, x1, y1], i) => {
          const X0 = Math.max(0, Math.floor(x0 / c.clientWidth * d.w));
          const X1 = Math.min(d.w, Math.ceil(x1 / c.clientWidth * d.w));
          const Y0 = Math.max(0, Math.floor((1 - y1 / c.clientHeight) * d.h));
          const Y1 = Math.min(d.h, Math.ceil((1 - y0 / c.clientHeight) * d.h));
          let sum = 0, n = 0;
          for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
            sum += d.data[(y * d.w + x) * 4 + 3]; n++;
          }
          series[i].push([f, n ? sum / n : 0]);
        });
      }
    }
    if (performance.now() - t0 < secs * 1000) requestAnimationFrame(tick); else done(series);
  };
  requestAnimationFrame(tick);
});
"""


def load(b, url, wait=3.0):
    b.goto(url, settle=0.8)
    do(b, PAGE)
    for _ in range(50):
        if val(b, 'window.__fx !== null', False):
            break
        b.sleep(0.1)
    b.sleep(wait)


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


def column(b):
    """The content column's box [left, top, right, bottom] in viewport px, and the dial's centre."""
    return b.ev("(() => {"
                " const v = document.querySelector('#screen .view').getBoundingClientRect();"
                " const k = document.querySelector('sv-knob').geometry();"
                " return { box: [v.left, v.top, v.right, v.bottom], dial: [k.cx, k.cy, k.radius] };"
                " })()")


# The pace probe's stroke: a pressed horizontal stroke in the lower part of the column, well
# under the dial, 20 moves 16 ms apart; the excess is the dye over what the same cells held just
# before the stroke.
def args(rect):
    return ', '.join(str(v) for v in rect)


def pace(b):
    """What a stroke leaves, as the dye's EXCESS over the same cells read just before it: the sum
    along the stroke's path (a band of +-PAD px) 1, 2 and 4 s after it ends, and the distance the
    excess's centroid travels between its end and 2 s later. Returns a dict."""
    geo = column(b)
    left, top, right, bottom = geo['box']
    y = min(bottom - 90, 860)
    cx = (left + right) / 2
    x0, x1 = cx - 240, cx + 240
    pad = 110
    band = (x0 - pad, y - pad, x1 + pad, y + pad)
    wide = (left, y - 300, right, min(900, y + 100))
    do(b, "window.__keep('base');")
    stroke(b, line(x0, y, x1, y, 20), True)
    b.sleep(0.05)
    out = {'path': [x0, y, x1, y]}
    ends = {}
    for label, wait in (('t0', 0.0), ('t1', 1.0), ('t2', 1.0), ('t4', 2.0)):
        b.sleep(wait)
        do(b, f"window.__keep('{label}');")
        ends[label] = val(b, f"window.__t('{label}')")
    for label in ('t0', 't1', 't2', 't4'):
        out[f'along_{label}'] = val(b, f"window.__sum('{label}', 'base', {args(band)}).sum")
        out[f'total_{label}'] = val(b, f"window.__sum('{label}', 'base', {args(wide)}).sum")
    c0 = val(b, f"window.__cen('t0', 'base', {args(wide)})")
    c2 = val(b, f"window.__cen('t2', 'base', {args(wide)})")
    out['centroid_t0'] = c0
    out['centroid_t2'] = c2
    out['travel_2s'] = (math.hypot(c2[0] - c0[0], c2[1] - c0[1]) if c0 and c2 else None)
    out['secs_t0_t2'] = ends['t2'] - ends['t0']
    out['secs_t0_t4'] = ends['t4'] - ends['t0']
    return out


def commit_state():
    """The commit the numbers were measured on, and whether the tree matched it."""
    repo = common.REPO
    sha = subprocess.run(['git', '-C', repo, 'rev-parse', 'HEAD'], capture_output=True,
                         text=True).stdout.strip()
    dirty = subprocess.run(['git', '-C', repo, 'status', '--porcelain', '--', 'bins'],
                           capture_output=True, text=True).stdout.strip()
    return sha, dirty == ''


def alphas(b, rects, frames=5):
    """The canvas's alpha (0 to 255), mean and max, over each rectangle, by readPixels."""
    return pg(b, 'window.__al(%s, %d)' % (json.dumps(rects), frames))


def glow_peak(dac, size=(1280, 900), rect=None):
    """The Glow level's own densest pixel as an equivalent alpha (0 to 1) of --glow-1 over --bg,
    at the demo's volume (a Glow-level screenshot, allowed; glow_level.py's method), over the
    window of `size` or only the `rect` [x0, y0, x1, y1] (CSS px) of it."""
    import numpy as np
    from PIL import Image
    b = make(size=size)
    try:
        b.goto(dac.url + '?effects=off')
        bg = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB')).astype(float)
        bgc = bg[5, 5]
        b.goto(dac.url + '?effects=glow')
        b.sleep(1.0)
        do(b, "document.querySelector('#screen').style.setProperty('visibility', 'hidden');"
              " document.querySelector('.top').style.setProperty('visibility', 'hidden');")
        b.sleep(0.3)
        img = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB')).astype(float)
        g1 = np.array([0x8b, 0x5c, 0xf6], float)
        a = ((img - bgc) / (g1 - bgc)).max(axis=2)
        a = np.where(np.abs(bg - bgc).max(axis=2) <= 1, a, 0)
        if rect:
            x0, y0, x1, y1 = rect
            a = a[y0:y1, x0:x1]
        return float(a.max())
    finally:
        b.close()


def shade_ok(sh):
    h, s, l = sh['hsl']['h'], sh['hsl']['s'], sh['hsl']['l']
    return (205 <= h <= 275 and 54 <= s <= 60 and 55 <= l <= 74
            and 0.3 <= sh['greyShare'] <= 0.7)


def hue_of(rgb):
    """The hue (degrees) of an sRGB triple 0 to 1."""
    r, g, bl = rgb
    hi, lo = max(rgb), min(rgb)
    if hi == lo:
        return 0.0
    d = hi - lo
    h = (((g - bl) / d) % 6 if hi == r else (bl - r) / d + 2 if hi == g else (r - g) / d + 4)
    return h * 60 % 360


def hue_gap(a, b):
    return min(abs(a - b), 360 - abs(a - b))


def palette_ok(sh):
    """The page's three tones: two side tones and the dial's, each pair 20 degrees apart or more
    (node proves the draw over 10 000 seeds; this reads what the page was given)."""
    tones = [hue_of(t) for t in (*sh['sides'], sh['dial'])]
    return all(hue_gap(tones[i], tones[j]) >= 20 for i in range(3) for j in range(i + 1, 3))


def stats(b):
    return val(b, 'window.__fx.stats()', None) or {}


def wide(dac, two, glow):
    """2560x1440, one page for four minutes: the haze's shape, the calm rate, its steady density,
    the volume, the dial's cloud, a resize and a device switch."""
    b = make(size=(2560, 1440))
    try:
        load(b, two.url, wait=0.5)
        left, right, bottom = [0, 0, 600, 1440], [1960, 0, 2560, 1440], [760, 700, 1800, 1440]
        # the volume is low from the start: the dial's cloud has the first CALM_SECS to reach it
        val(b, 'window.__fx.level(0.06)')
        f0 = val(b, 'window.__frames()')
        b.sleep(CALM_SECS)
        f1 = val(b, 'window.__frames()')
        rate = (f1 - f0) / CALM_SECS
        calm = val(b, "import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href)"
                      '.then((m) => m.TUNING.calmFps)', None)
        lo, hi = 0.85 * calm, 1.15 * calm  # 17 to 23 at the approved 20
        check(f'untouched for {CALM_SECS} s the page runs at the calm rate ({rate:.1f} frames a '
              f'second; {lo:g} to {hi:g})',
              lo <= rate <= hi, rate)
        sl, sr, bh, band = alphas(b, [left, right, bottom, BAND])
        side = (sl['mean'] + sr['mean']) / 2
        whole_first = alphas(b, [[0, 0, 2560, 1440]])[0]['mean']
        side_first = side
        print(f'      2560x1440 at {CALM_SECS} s: side bars mean {side:.2f} (max '
              f'{max(sl["max"], sr["max"])}), behind the column mean {bh["mean"]:.2f} max '
              f'{bh["max"]}; glow peak {glow * 255:.0f}')
        check('the side bars hold more than twice the haze behind the column',
              side > 2 * bh['mean'], [side, bh])
        check('  and behind the column there is haze', bh['mean'] > 0.3 and bh['max'] > 0, bh)
        # the Glow level draws nothing down there, so its densest pixel (the legibility bound
        # above) is the reference for the band the dial's cloud reaches
        print(f'      the band y 700-900 behind the column: max {band["max"]}; the Glow level\'s '
              f'densest pixel {glow * 255:.0f}')
        check('  and in the band above it (y 700-900, the dial\'s cloud included) the smoke\'s max '
              'stays at or under the Glow level\'s densest pixel',
              band['max'] <= glow * 255, [band, glow * 255])
        check('  and behind the column it stays at or under the Glow level\'s alpha',
              bh['max'] <= glow * 255, [bh, glow * 255])
        # the haze does not follow the volume: only the dial's cloud does. The side bars are read
        # 1.5 s after each level change (the eased level has settled, the haze has hardly moved),
        # the dial's cloud 9 s after (its dye has caught up)
        dial = val(b, "(() => { const k = document.querySelector('sv-knob').geometry();"
                      " return [k.cx, k.cy, k.radius]; })()")
        near = [dial[0] - 160, dial[1] - 220, dial[0] + 160, dial[1] + 60]
        lo_dial = alphas(b, [near])[0]
        lo_side = alphas(b, [left, right])
        val(b, 'window.__fx.level(1)')
        b.sleep(1.5)
        hi_side = alphas(b, [left, right])
        b.sleep(7.5)
        hi_dial = alphas(b, [near])[0]
        a = sum(x['mean'] for x in lo_side) / 2
        c = sum(x['mean'] for x in hi_side) / 2
        print(f'      side bars at level 0.06 {a:.2f}, 1.5 s after level 1 {c:.2f}; the dial\'s '
              f'neighbourhood mean {lo_dial["mean"]:.2f} -> {hi_dial["mean"]:.2f}, max '
              f'{lo_dial["max"]} -> {hi_dial["max"]}')
        check('the haze does not follow the volume (side bars at level 0.06 and 1 within 15 %)',
              abs(a - c) <= 0.15 * a, [a, c])
        check('the dial\'s cloud is brighter at a higher level',
              hi_dial['mean'] > 1.1 * lo_dial['mean'] and hi_dial['max'] > lo_dial['max'],
              [lo_dial, hi_dial])
        b.sleep(max(0, STEADY_SECS - CALM_SECS - 9 - 4))
        whole_late = alphas(b, [[0, 0, 2560, 1440]])[0]['mean']
        sl, sr, bh = alphas(b, [left, right, bottom])
        m0 = bh
        side0 = (sl['mean'] + sr['mean']) / 2
        print(f'      whole-canvas mean alpha {whole_first:.2f} at {CALM_SECS} s, '
              f'{whole_late:.2f} at {STEADY_SECS} s')
        # each side source's strength follows its place along the band (it wanders over 90 s), so
        # the six of them differ by up to about a quarter between two instants; a runaway or an
        # emptying would be far more than that
        check(f'the side bars\' density at {CALM_SECS} s and at {STEADY_SECS} s agrees within '
              '30 % (the dial\'s cloud wanders over the rest of the canvas)',
              abs(side0 - side_first) <= 0.3 * side_first,
              [side_first, side0, whole_first, whole_late])
        # a resize to 1280: the bands follow within 2 s, the column holds no leftover
        b.resize(1280, 900)
        b.sleep(1.6)
        band_l, band_r, mid = [0, 600, 150, 900], [1130, 600, 1280, 900], [250, 600, 1030, 900]
        # where the old side bars, stretched into the new grid, lie beside the new bands
        stretched = [165, 600, 350, 900]
        bl, br, md, st = alphas(b, [band_l, band_r, mid, stretched])
        edge = (bl['mean'] + br['mean']) / 2
        print(f'      2560 -> 1280 after 2 s: edge bands mean {edge:.2f}, the column\'s middle '
              f'mean {md["mean"]:.2f} max {md["max"]} (haze behind the column before: mean '
              f'{m0["mean"]:.2f} max {m0["max"]})')
        check('after a resize to 1280, the edge bands hold the haze within 2 s',
              edge > 2 * max(md['mean'], 0.2), [edge, md])
        check('  and what is now the column holds no more than the haze\'s own level (where the '
              'old side bars were stretched to included)',
              md['mean'] <= 1.5 * m0['mean'] + 1 and md['max'] <= m0['max'] + 2
              and st['mean'] <= 1.5 * m0['mean'] + 1, [md, st, m0])
        check('  and no page error', errors(b) == [], errors(b))
        b.resize(2560, 1440)
        b.sleep(3)
        # a device switch to the card-only screen (away: no smoke feeds there) and back: the new
        # canvas holds the haze within 2 s
        val(b, "document.querySelectorAll('.devs a.dev')[1].click()")
        b.sleep(1.5)
        val(b, 'history.back()')
        b.sleep(2.0)
        sl, sr, bh = alphas(b, [left, right, bottom])
        side = (sl['mean'] + sr['mean']) / 2
        print(f'      after a device switch and back: side bars {side:.2f}, behind the column '
              f'{bh["mean"]:.2f}')
        check('after a device switch and back the side bars hold more than twice the column\'s '
              'haze, and 0.7 of what they held, and the column stays under the Glow level\'s '
              'alpha, within 2 s (where the three spread sources lie moves the column\'s mean '
              'between about 1 and 6, so it is bounded by the legibility bound, not by the '
              'mean of another instant)',
              side > 2 * bh['mean'] and bh['max'] <= glow * 255 and side > 0.7 * side0,
              [side, bh, m0, side0, glow * 255])
        check('  and no page error', errors(b) == [], errors(b))
    finally:
        b.close()


def flicker(series):
    """The mean frame-to-frame change of a series of [frame, mean], per frame and as a share of the
    series' own mean (a read that skipped frames counts per frame it spans)."""
    steps = [abs(b[1] - a[1]) / (b[0] - a[0]) for a, b in zip(series, series[1:]) if b[0] > a[0]]
    level = sum(m for _, m in series) / len(series)
    return sum(steps) / len(steps) / level if steps and level > 0 else None


def steady(dac):
    """The side bands as steady as the dial's cloud: at rest for STEADY_READ s, the mean dye over
    each side band and over the dial's neighbourhood is read at every smoke frame; the bands'
    frame-to-frame change, as a share of their own level, is at most STEADY_FACTOR times the
    dial's and under STEADY_ABS."""
    b = make(size=(1920, 1080))
    try:
        load(b, dac.url, wait=0.5)
        b.sleep(STEADY_WAIT)
        geo = column(b)
        left, top, right, bottom = geo['box']
        cx, cy, radius = geo['dial']
        rects = [[0, 0, left, 1080], [right, 0, 1920, 1080],
                 [cx - 1.3 * radius, cy - 1.3 * radius, cx + 1.3 * radius, cy + 1.3 * radius]]
        out = pg(b, 'window.__steady(%s, %d)' % (json.dumps(rects), STEADY_READ))
        lf, rf, df = (flicker(x) for x in out)
        print(f'      at rest for {STEADY_READ} s ({len(out[0])} frames read): the frame-to-frame '
              f'change as a share of the level: left band {lf:.4f}, right band {rf:.4f}, dial '
              f'{df:.4f}')
        check(f'the side bands change from frame to frame by no more than {STEADY_FACTOR:g} times '
              f'what the dial\'s cloud does, and by under {STEADY_ABS:g} of their level',
              None not in (lf, rf, df) and max(lf, rf) <= STEADY_FACTOR * df
              and max(lf, rf) <= STEADY_ABS, [lf, rf, df])
        check('  and no page error', errors(b) == [], errors(b))
    finally:
        b.close()


PHONE_SEEDS = (1, 2, 3)  # the draws of the phone section: each one a fresh Smoke start


def phone(dac, glow):
    """390x844: no margins, so the window's own edges carry the lighter gradient. Each of
    `PHONE_SEEDS` seeds the smoke's draws and restarts Smoke from the menu; the edges beat the
    middle in each, and the middle stays at or under the Glow level's densest pixel."""
    b = make(size=(390, 844))
    try:
        load(b, dac.url, wait=0.5)
        for n in PHONE_SEEDS:
            val(b, f'window.__fx.seed({n})')
            x, y = common.center(b, '.fxbtn')
            for level in ('glow', 'smoke'):
                b.click(x, y)
                lx, ly = common.center(b, f'.fxpop [data-level={level}]')
                b.click(lx, ly)
                b.sleep(0.5)
            b.sleep(PHONE_SECS)
            # below the dial's cloud, so only the haze is in the rectangles
            left, right, mid = [0, 560, 60, 844], [330, 560, 390, 844], [100, 560, 290, 844]
            l, r, m = alphas(b, [left, right, mid], 4)
            edge = (l['mean'] + r['mean']) / 2
            print(f'      390x844 seed {n}: edges mean {edge:.2f}, the middle {m["mean"]:.2f} '
                  f'(max {m["max"]}); glow peak {glow * 255:.0f}')
            check(f'at phone width, seed {n}, the edges are denser than the middle',
                  edge > m['mean'], [l, r, m])
            check('  and the middle holds haze', m['mean'] > 0.2, m)
            check('  and the middle stays at or under the Glow level\'s alpha',
                  m['max'] <= glow * 255, [m, glow * 255])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()


def colour(dac):
    """The shade: inside the ranges, different on each load, kept across Smoke, Glow, Smoke."""
    shades = []
    for _ in range(2):
        b = make(size=(1280, 900))
        try:
            load(b, dac.url, wait=0.5)
            shades.append(stats(b).get('shade'))
            if len(shades) == 1:
                first = shades[0]
                x, y = common.center(b, '.fxbtn')
                for level in ('glow', 'smoke'):
                    b.click(x, y)
                    lx, ly = common.center(b, f'.fxpop [data-level={level}]')
                    b.click(lx, ly)
                    b.sleep(0.5)
                again = stats(b).get('shade')
                check('Smoke, Glow, Smoke keeps the same shade', again == first, [first, again])
        finally:
            b.close()
    check('the shade lies inside the ranges',
          all(sh is not None and shade_ok(sh) for sh in shades), shades)
    check('  and the page was given two side tones and a dial tone, 20 degrees or more apart',
          all(sh is not None and 'sides' in sh and palette_ok(sh) for sh in shades), shades)
    check('  and two loads draw different shades',
          shades[0] is not None and shades[1] is not None and shades[0] != shades[1], shades)


def pace_runs(dac, b):
    """The pace probe on `PACE_LOADS` fresh page loads, and the median of each number."""
    runs = []
    for _ in range(PACE_LOADS):
        load(b, dac.url, wait=PACE_WAIT)
        runs.append(pace(b))
    mid = {}
    for key in ('along_t1', 'along_t2', 'along_t4', 'travel_2s'):
        mid[key] = sorted(r[key] for r in runs)[len(runs) // 2]
    return {'runs': runs, 'median': mid}


def pace_check(dac):
    """A stroke lasts longer and travels further than in the recorded baseline
    (smoke-baseline.json)."""
    with open(BASELINE_FILE) as f:
        base = json.load(f)[browser]['median']
    b = make(size=(1280, 900))
    try:
        got = pace_runs(dac, b)['median']
        print(f'      pace (median of {PACE_LOADS}): dye along the path at 4 s '
              f'{got["along_t4"]:.1f} (baseline: {base["along_t4"]:.1f}), the centroid\'s 2 s '
              f'travel {got["travel_2s"]:.0f} px (baseline: {base["travel_2s"]:.0f})')
        check('a stroke\'s dye along its path at 4 s beats the baseline\'s',
              got['along_t4'] > base['along_t4'], [got, base])
        check('  and its centroid travels further in 2 s',
              got['travel_2s'] > base['travel_2s'], [got, base])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()


def baseline():
    """Measure the pace probe on the web tree as it is (SMOKE_WEB; BASELINE_COMMIT names the commit
    that tree is, the checkout's HEAD when unset) and write it into the baseline file."""
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        got = pace_runs(dac, b)
        print('      pace probe:', json.dumps(got['median']))
        sha, clean = commit_state()
        try:
            with open(BASELINE_FILE) as f:
                saved = json.load(f)
        except OSError:
            saved = {}
        saved['commit'] = os.environ.get('BASELINE_COMMIT', sha)
        saved['size'] = [1280, 900]
        saved['loads'] = PACE_LOADS
        saved['wait_secs'] = PACE_WAIT
        saved[browser] = got
        with open(BASELINE_FILE, 'w') as f:
            json.dump(saved, f, indent=2)
            f.write('\n')
        print('wrote', BASELINE_FILE)
    finally:
        b.close()
        dac.stop()


def run():
    """Every section, or the ones named in HAZE_SECTIONS (a comma list of wide, phone, colour,
    steady, pace; the mutation runner uses it to run only what a mutation can break)."""
    want = os.environ.get('HAZE_SECTIONS', 'wide,phone,colour,steady,pace').split(',')
    dac = common.Daemon('gungnir2-v2')
    two = common.Daemon('gungnir2-v2', more=[('Second', '1112131415161718')])
    try:
        if 'wide' in want:
            wide(dac, two, glow_peak(dac))
        if 'phone' in want:
            phone(dac, glow_peak(dac))
        if 'colour' in want:
            colour(dac)
        if 'steady' in want:
            steady(dac)
        if 'pace' in want:
            pace_check(dac)
    finally:
        dac.stop()
        two.stop()


if __name__ == '__main__':
    if BASELINE_ONLY:
        baseline()
    else:
        run()
    print(f'haze.py {browser}: {passed} passed, {len(fails)} failed')
    sys.exit(1 if fails else 0)
