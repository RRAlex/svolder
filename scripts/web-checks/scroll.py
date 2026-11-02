# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Scrolling stirs the smoke, and the smoke has no upward push, in Chromium or Firefox, numbers
only (float readbacks of the velocity and dye grids; never a picture of Smoke).
Usage: python3 scroll.py chromium|firefox
Checks: the strip a scroll uncovers holds none of the edge's dye (the dial's cloud cut by the
edge, then a wheel scroll back up); wheel steps on the DAC page at 1280x700 raise the velocity
along the scroll (down: up the screen; back up: down it); a burst of 30 steps in 0.5 s stays under
a stated speed with every texel finite; a page too short to scroll (the amp at 1280x1400) takes
wheel steps with no error and no scroll splat; at rest the cloud's vertical centre does not sit
above its source (the mean over 12 s of a seeded run); in Chromium under touch emulation (390x844;
headless Firefox does not pan) a vertical pan scrolls the page, raises the velocity along the
scroll, and nothing is prevented. The haze is paused (the handle's `haze(false)`) so the cloud and
the stir are read alone."""
import os
import sys

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
# the sections to run (SCROLL_SECTIONS, comma-separated), for the mutation runner; default all
SECTIONS = os.environ.get('SCROLL_SECTIONS', 'desktop,short,rest,smear,touch').split(',')
fails = []
passed = 0

# The bounds. Velocities are the grid's own units (cells per second).
SCROLL_RISE = 8.0  # the mean vertical velocity moves at least this much along five wheel steps of
# 100 px (11.1 measured; its 17 cells a second at the peak carry the smoke 120 px in 1.5 s)
SCROLL_PEAK = 14.0  # and the greatest speed on the grid reaches at least this much after them
BURST_MAX = 35.0  # the greatest speed on the grid after a 30-step burst (maxScrollPush is 30)
PAN_RISE = 1.0  # the same for the touch pan
REST_UP_SHARE = 0.08  # the mean height of the cloud's centre over its source, of the canvas height
STRIP_MAX = 0.01  # the most dye (amount) in any cell of the strip beyond the hold, at 0.15 s
# (0.0016 measured; a copy of the edge row would carry the cut cloud's 0.099 down it)
STEP_MAX = 2.0  # the most one row's mean may be over its neighbour's across the former cut line
# (times: 1.3 measured, 2.6 when the dye is dropped)
KEEP_MIN = 0.4  # the least share of their earlier density that the ten rows above the cut line
# hold again (0.59 measured, 0.30 when the dye is dropped)
STRIP_ROWS = 150  # the strip's rows read, from the top of the viewport (CSS px)
REST_SECS = 12  # the length of the no-bias run (s)
REST_SEEDS = (3,)  # the seeded draws of the no-bias run, one fresh page each


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


def pg(b, expr):
    """An expression's value in the page's own realm (Firefox's scripts run in a sandbox)."""
    return b.page(expr)


def do(b, js):
    return pg(b, f'(() => {{ {js} }})()')


def errors(b):
    if browser == 'chromium':
        return b.errors
    return b.errors + (pg(b, 'window.__errs || []') or [])


# `__v()` reads the velocity grid: the mean vertical velocity (y up), the greatest speed, how many
# texels of either grid are not finite. `__rest()` is the dye's vertical centre, in CSS px from the
# top, and its weight.
PAGE = """
window.__fx = null;
import(new URL('assets/themes/ember/effects.js', document.baseURI).href).then((m) => { window.__fx = m; });
window.__v = () => {
  const r = window.__fx.readback(); if (!r) return null;
  const v = r.vel; let sy = 0, max = 0, bad = 0;
  for (let i = 0; i < v.w * v.h; i++) {
    const x = v.data[i * 4], y = v.data[i * 4 + 1];
    if (!Number.isFinite(x) || !Number.isFinite(y)) { bad++; continue; }
    sy += y; if (Math.hypot(x, y) > max) max = Math.hypot(x, y);
  }
  for (const a of r.dye.data) if (!Number.isFinite(a)) bad++;
  return { mean: sy / (v.w * v.h), max, bad };
};
window.__rest = () => {
  const r = window.__h.readback(); if (!r) return null;
  const d = r.dye, h = innerHeight;
  let w = 0, sy = 0;
  for (let gy = 0; gy < d.h; gy++) for (let gx = 0; gx < d.w; gx++) {
    const a = d.data[(gy * d.w + gx) * 4 + 3]; w += a; sy += a * (1 - (gy + 0.5) / d.h) * h;
  }
  return w > 0 ? { y: sy / w, w, h } : null;
};
// the greatest dye amount of any cell in the rectangle (CSS px from the top left) and its mean
window.__strip = (x0, y0, x1, y1) => {
  const r = window.__fx.readback(); if (!r) return null;
  const d = r.dye, c = document.querySelector('canvas.smoke');
  const X0 = Math.max(0, Math.floor(x0 / c.clientWidth * d.w));
  const X1 = Math.min(d.w, Math.ceil(x1 / c.clientWidth * d.w));
  const Y0 = Math.max(0, Math.floor((1 - y1 / c.clientHeight) * d.h));
  const Y1 = Math.min(d.h, Math.ceil((1 - y0 / c.clientHeight) * d.h));
  let max = 0, sum = 0, n = 0;
  for (let y = Y0; y < Y1; y++) for (let x = X0; x < X1; x++) {
    const a = d.data[(y * d.w + x) * 4 + 3]; if (a > max) max = a; sum += a; n++;
  }
  return { max, mean: n ? sum / n : 0 };
};
// the mean dye of `n` rows between y0 and y1 (CSS px from the top), over x0 to x1, top row first
window.__rows = (x0, x1, y0, y1, n) => {
  const out = [];
  for (let k = 0; k < n; k++) {
    const m = window.__strip(x0, y0 + (y1 - y0) * k / n, x1, y0 + (y1 - y0) * (k + 1) / n);
    out.push(m ? m.mean : 0);
  }
  return out;
};
window.__prevented = 0;
window.addEventListener('touchmove', (e) => { if (e.defaultPrevented) window.__prevented++; });
"""


def load(b, url, wait=2.5):
    b.goto(url, settle=0.8)
    do(b, PAGE)
    for _ in range(50):
        if pg(b, 'window.__fx !== null'):
            break
        b.sleep(0.1)
    do(b, 'window.__fx.haze(false);')
    b.sleep(wait)


def wheel(b, dy, x=640, y=400):
    """One wheel step of `dy` px over the page."""
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': y,
                                            'deltaX': 0, 'deltaY': dy})
        return
    b._post('/actions', {'actions': [{'type': 'wheel', 'id': 'w', 'actions': [
        {'type': 'scroll', 'x': int(x), 'y': int(y), 'deltaX': 0, 'deltaY': int(dy),
         'duration': 0, 'origin': 'viewport'}]}]})


def steps(b, n, dy, gap):
    for _ in range(n):
        wheel(b, dy)
        b.sleep(gap)


def vel(b):
    return pg(b, 'window.__v()')


def scroll_splats(b):
    return pg(b, 'window.__fx.stats().splats.scroll')


def desktop():
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 700))
    try:
        load(b, dac.url)
        room = pg(b, 'document.documentElement.scrollHeight - innerHeight')
        check('the DAC page scrolls at 1280x700', room > 300, room)
        v0 = vel(b)
        s0 = scroll_splats(b)
        steps(b, 5, 100, 0.05)
        b.sleep(0.15)
        v1 = vel(b)
        print(f'      5 wheel steps down: mean vy {v0["mean"]:.3f} -> {v1["mean"]:.3f}, '
              f'max |v| {v1["max"]:.1f}, scroll splats {s0} -> {scroll_splats(b)}')
        check('wheel steps down: the velocity rises along the scroll (the mean vy goes up by '
              f'{SCROLL_RISE:g} cells a second or more, and the peak reaches {SCROLL_PEAK:g})',
              v1['mean'] - v0['mean'] > SCROLL_RISE and v1['max'] >= SCROLL_PEAK, [v0, v1])
        check('  and the scroll splats were applied', scroll_splats(b) > s0, [s0, scroll_splats(b)])
        steps(b, 8, -100, 0.05)
        b.sleep(0.15)
        v2 = vel(b)
        print(f'      8 wheel steps back up: mean vy {v1["mean"]:.3f} -> {v2["mean"]:.3f}')
        check('wheel steps up: the velocity falls along the scroll (the mean vy goes down)',
              v2['mean'] < v1['mean'] - SCROLL_RISE, [v1, v2])
        # a momentum-like burst: 30 steps in 0.5 s
        b.resize(1280, 200)  # a short window: the 30 steps then outrun maxScrollPush
        do(b, 'window.__fx.haze(false); scrollTo(0, 0);')
        b.sleep(1.0)
        room = pg(b, 'document.documentElement.scrollHeight - innerHeight')
        step = min(60, int(room * 0.8 / 30))
        # one step a frame in the page's own loop, so the 30 steps fall within about half a second
        do(b, f'let n = 0; const go = () => {{ scrollBy(0, {step});'
              f' if (++n < 30) requestAnimationFrame(go); }}; requestAnimationFrame(go);')
        b.sleep(0.6)
        v3 = vel(b)
        print(f'      burst of 30 steps of {step} px in 0.5 s: max |v| {v3["max"]:.1f} '
              f'(bound {BURST_MAX}), non-finite texels {v3["bad"]}')
        check(f'a burst of 30 steps in 0.5 s: the greatest speed stays under {BURST_MAX:g}',
              v3['max'] < BURST_MAX, v3)
        check('  and every texel of both grids is finite', v3['bad'] == 0, v3)
        b.sleep(1.5)
        v4 = vel(b)
        check('  and the smoke is still finite a little later', v4['bad'] == 0, v4)
        # a few seconds after the scroll: calm again, and the scroll splats have stopped
        b.sleep(3.0)
        n0, f0 = scroll_splats(b), pg(b, 'window.__fx.stats().frames')
        b.sleep(2.0)
        n1, f1 = scroll_splats(b), pg(b, 'window.__fx.stats().frames')
        print(f'      5 s after the burst: scroll splats {n0} -> {n1}, '
              f'{(f1 - f0) / 2:.1f} frames/s')
        check('a few seconds after a scroll the scroll splats have stopped', n1 == n0, [n0, n1])
        check('  and the rhythm is back at the calm rate (under 30 frames a second)',
              (f1 - f0) / 2 < 30, [f0, f1])
        check('no page error (desktop)', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()


def short():
    amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make(size=(1280, 1400))
    try:
        load(b, amp.url)
        room = pg(b, 'document.documentElement.scrollHeight - innerHeight')
        print(f'      the amp at 1280x1400: {room} px of scroll room')
        check('the amp page does not scroll at 1280x1400', room <= 0, room)
        s0 = scroll_splats(b)
        steps(b, 6, 100, 0.05)
        b.resize(1280, 1398)  # a layout pass on a page that still cannot scroll
        b.sleep(0.3)
        print(f'      wheel steps on a short page: scroll splats {s0} -> {scroll_splats(b)}, '
              f'scrollY {pg(b, "scrollY")}')
        check('wheel steps on a page too short to scroll: no scroll splat, no scroll',
              scroll_splats(b) == s0 and pg(b, 'scrollY') == 0, [s0, scroll_splats(b)])
        check('  and no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        amp.stop()


def rest():
    """The cloud of a handle of its own (seeded draws, no haze), over its resting source."""
    dac = common.Daemon('gungnir2-v2')
    offsets = []
    try:
        for seed in REST_SEEDS:
            b = make(size=(1280, 900))
            try:
                b.goto(dac.url + '?effects=off', settle=0.8)
                do(b, PAGE)
                do(b, """const c = document.createElement('canvas');
                  for (const [k, v] of [['position', 'fixed'], ['left', '0'], ['top', '0'],
                    ['width', '100%'], ['height', '100lvh'], ['pointer-events', 'none']])
                    c.style.setProperty(k, v);
                  document.body.append(c);
                  Promise.all([import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href),
                    import(new URL('assets/themes/ember/smoke.js', document.baseURI).href)])
                    .then(([model, m]) => {
                      const random = model.seeded(SEED);
                      window.__h = m.start(c, { shade: model.drawShade(random), random });
                      window.__h.haze(false);
                      window.__h.level(0.8);
                      window.__h.anchor({ x: 640, y: 450, radius: 100, meters: [] });
                    });""".replace('SEED', str(seed)))
                for _ in range(50):
                    if pg(b, '!!window.__h'):
                        break
                    b.sleep(0.1)
                mine = []
                for _ in range(REST_SECS):
                    b.sleep(1.0)
                    r = pg(b, 'window.__rest()')
                    if r:
                        mine.append((450 - r['y']) / r['h'])
                offsets.append(sum(mine) / max(1, len(mine)))
                print(f'      seed {seed}: the cloud centre over its source (of the height), '
                      f'mean of {len(mine)} reads {offsets[-1]:+.3f}')
            finally:
                b.close()
        mean = sum(offsets) / len(offsets)
        print(f'      mean of the runs {mean:+.3f} (bound {REST_UP_SHARE})')
        check(f'at rest the cloud\'s centre is not above its source over {REST_SECS} s '
              f'(mean height over it under {REST_UP_SHARE} of the canvas)',
              mean < REST_UP_SHARE, offsets)
    finally:
        dac.stop()


def smear():
    """The dye a scroll moves out of view comes back on the reverse scroll. The page is scrolled
    until the dial sits near the top of the viewport, so the dial's cloud is cut by the edge (the
    haze is paused: the cloud is the densest dye there), then scrolled back to the top by wheel
    steps a second later. Rows of the dial's neighbourhood are read at 0.15 s: no step between two
    neighbouring rows across the former cut line beyond STEP_MAX times, the rows above it holding at
    least KEEP_MIN of their density from before (a scroll that drops the dye fails both), and no
    row denser than the cloud was before. A scroll longer than the hold
    (half a viewport) leaves its farther strip empty: the second scroll, 600 px, reads that
    strip."""
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 700))
    try:
        load(b, dac.url)
        do(b, 'window.__fx.level(0.8);')
        b.sleep(4.0)
        dial_x, dial_y = pg(b, 'document.querySelector("sv-knob").geometry().cx'), pg(
            b, 'document.querySelector("sv-knob").geometry().cy + scrollY')
        x0, x1 = max(0, dial_x - 200), dial_x + 200
        rows = f'window.__rows({x0}, {x1}, 0, 700, 70)'
        before = pg(b, rows)
        peak = max(before)
        top = max(160, int(dial_y - 70))
        do(b, f'scrollTo(0, {top});')
        b.sleep(1.0)
        steps(b, 8, -50, 0.04)
        b.sleep(0.15)
        y = pg(b, 'scrollY')
        after = pg(b, rows)
        # the cut was at the viewport's top, which is `top` px down the page: `top` px down the
        # viewport once back at the top (10 px rows, 70 of them)
        cut = int(top / 10)
        near = after[max(0, cut - 5):cut + 6]
        above = sum(after[cut - 12:cut - 2])
        was = sum(before[cut - 12:cut - 2])
        steps_up = [max(p, q) / (min(p, q) + 0.0005) for p, q in zip(near, near[1:])]
        print(f'      dial {dial_y:.0f} px down, scrolled to {top}, back at {y}: '
              f'cloud peak before {peak:.4f}; row means round the cut line (row {cut}) '
              f'{[round(v, 4) for v in near]}; greatest step {max(steps_up):.1f} '
              f'(bound {STEP_MAX}); the ten rows above hold {above / was:.2f} of what they held '
              f'before (bound {KEEP_MIN}); densest row after {max(after):.4f}')
        check('the dial\'s cloud was dense before the scroll', peak > 0.03, before)
        check(f'after a scroll down that cuts the dial\'s cloud and back up, no step across the '
              f'former cut line beyond {STEP_MAX:g} times between neighbouring rows',
              y == 0 and max(steps_up) <= STEP_MAX, [near, steps_up])
        check(f'  and the smoke above it is back: the ten rows above hold at least {KEEP_MIN:g} of '
              'what they held before the scroll', above >= KEEP_MIN * was, [above, was])
        check('  and no band: no row denser than the cloud was before (within 15 %)',
              max(after) <= 1.15 * peak, [max(after), peak])
        # a scroll longer than the hold: the strip beyond it is empty, not a copy of the edge
        do(b, 'scrollTo(0, 600);')
        b.sleep(1.0)
        steps(b, 12, -50, 0.04)
        b.sleep(0.15)
        got = pg(b, f'window.__strip(0, 0, 1280, {STRIP_ROWS})')
        print(f'      600 px down and back (the hold is half a viewport, 350 px): the farthest '
              f'strip (rows 0-{STRIP_ROWS}) max {got["max"]:.4f} (bound {STRIP_MAX})')
        check('after a scroll longer than the hold, the strip beyond it holds no more than '
              f'{STRIP_MAX:g} of dye (it is empty, not a copy of the edge)',
              got['max'] <= STRIP_MAX, got)
        check('no page error (smear)', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()


def touch_pan():
    dac = common.Daemon('gungnir2-v2')
    b = common.Chromium(size=(390, 844), touch=True,
                        args=['--disable-features=OverscrollHistoryNavigation'])
    try:
        load(b, dac.url)
        y0 = b.ev('scrollY')
        v0 = vel(b)
        s0 = scroll_splats(b)
        import time
        t0 = time.time()
        pts = [(200, 700 - 50 * i) for i in range(9)]
        for i, (x, y) in enumerate(pts):
            b.send('Input.dispatchTouchEvent', {
                'type': 'touchStart' if i == 0 else 'touchMove', 'touchPoints': [{'x': x, 'y': y}],
                'timestamp': t0 + 0.4 * i / 8})
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': [],
                                            'timestamp': t0 + 0.4})
        b.sleep(0.15)
        v1 = vel(b)
        y1 = b.ev('scrollY')
        print(f'      touch pan up 400 px: scrollY {y0} -> {y1}, mean vy {v0["mean"]:.3f} -> '
              f'{v1["mean"]:.3f}, scroll splats {s0} -> {scroll_splats(b)}')
        check('a vertical touch pan scrolls the page', y1 > y0 + 50, [y0, y1])
        check('  the velocity rises along the scroll (the mean vy goes up)',
              v1['mean'] - v0['mean'] > PAN_RISE, [v0, v1])
        check('  nothing prevented a touch move', b.ev('window.__prevented') == 0)
        check('  and no page error', b.errors == [], b.errors)
    finally:
        b.close()
        dac.stop()


if __name__ == '__main__':
    if 'desktop' in SECTIONS:
        desktop()
    if 'short' in SECTIONS:
        short()
    if 'rest' in SECTIONS:
        rest()
    if 'smear' in SECTIONS:
        smear()
    if 'touch' in SECTIONS and browser == 'chromium':
        touch_pan()
    print(f'scroll.py {browser}: {passed} passed, {len(fails)} failed')
    sys.exit(1 if fails else 0)
