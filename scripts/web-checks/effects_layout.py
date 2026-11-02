# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The page-level glow layer in Chromium or Firefox: no horizontal scroll, the glow centred on
the dial through scroll and resize, its size, and no hard edge at the content column's sides
(pixel rows of a Glow-level screenshot; no smoke is ever drawn or captured here).
Usage: python3 effects_layout.py [chromium|firefox]"""
import io
import sys

import numpy as np
from PIL import Image

import common

fails = []
passed = 0
browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def edge_report(b, tag):
    """The worst step across each side of `.view`'s box against the steps beside it."""
    info = b.ev("""(() => {
      const v = document.querySelector('#screen .view').getBoundingClientRect();
      const k = document.querySelector('sv-knob').geometry();
      const g = document.querySelector('.glow').getBoundingClientRect();
      return { left: v.left, right: v.right, gx: g.left + g.width / 2, gy: g.top + g.height / 2,
               gw: g.width, cx: k.cx, cy: k.cy, radius: k.radius, sy: scrollY };
    })()""")
    img = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB')).astype(int)
    h, w, _ = img.shape
    k = w / b.ev('window.innerWidth')  # image pixels per CSS pixel
    for key in ('left', 'right', 'cy'):
        info[key] = info[key] * k
    rows = range(max(0, int(info['cy'] - 380 * k)), min(h, int(info['cy'] + 380 * k)))
    out = {}
    for side in ('left', 'right'):
        e = int(round(info[side]))
        edge, beside, clean = [], [], 0
        lo, hi = min(8, e - 4), min(8, w - e - 5)  # the 8 px of focus-ring room beside the edge
        if lo >= 6 and hi >= 6:
            for y in rows:
                row = img[y]
                # the steps over 4 px beside the edge, left and right of it
                steps = [int(np.abs(row[x + 2] - row[x - 2]).max())
                         for x in list(range(e - lo, e - 3)) + list(range(e + 3, e + hi))]
                if max(steps) > 3:
                    continue  # content (text, the dial, a card) crosses this row: not the glow's
                clean += 1
                edge.append(int(np.abs(row[e + 2] - row[e - 2]).max()))
                beside.append(max(steps))
        out[side] = (max(edge) if edge else None, max(beside) if beside else None, clean)
    return info, out


def run():
    dac = common.Daemon('gungnir2-v2')
    amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    guest = common.Daemon(web='guest = "read"', env={
        'SVOLDER_WEB_TOKEN': 'smoke-checks-token-0123456789abcdefghij'})
    b = (common.Chromium(size=(1280, 900)) if browser == 'chromium'
         else common.Firefox(size=(1280, 900)))
    try:
        # -- no horizontal scroll at any width, both screens
        for url, label in ((dac.url, 'DAC'), (amp.url, 'amp')):
            b.goto(url)
            for w in (360, 390, 1280, 2560):
                b.resize(w, 900)
                sw = b.ev('[document.scrollingElement.scrollWidth, '
                          'document.documentElement.clientWidth]')
                got = b.ev('innerWidth')
                check(f'{label} at {w}: the viewport really is {w} px wide', abs(got - w) <= 1, got)
                check(
                    f'{label} at {w}: scrollWidth == clientWidth (the viewport less any scrollbar)',
                    sw[0] == sw[1],
                    sw,
                )

        # -- the glow's size, centred on the dial, through resize and scroll
        b.goto(dac.url + '?effects=glow')
        for w, h, want in ((390, 844, None), (1280, 900, 760), (2560, 1440, 760)):
            b.resize(w, h)
            b.sleep(0.5)
            if want is None:  # 160 % of the view: the viewport less a scrollbar, the page's 20 px
                want = 1.6 * (b.ev('document.documentElement.clientWidth') - 40 + 16)  # sides + 8
            g = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
              const k = document.querySelector('sv-knob').geometry();
              return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy,
                      document.querySelector('.glow').offsetWidth]; })()""")
            check(f'{w}: the glow is centred on the dial (within 2 px)',
                  abs(g[0] - g[2]) <= 2 and abs(g[1] - g[3]) <= 2, g)
            check(f'{w}: the glow is {want:.0f} px wide', abs(g[4] - want) <= 1, g)
        b.resize(390, 844)
        b.goto(dac.url + '?effects=glow')
        b.sleep(0.5)
        scrollable = b.ev('document.scrollingElement.scrollHeight - innerHeight')
        check('the phone-width DAC page scrolls', scrollable >= 300, scrollable)
        b.ev('window.scrollTo(0, 300)')
        b.sleep(0.6)
        g = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
          const k = document.querySelector('sv-knob').geometry();
          return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy, scrollY]; })()""")
        check('phone, scrolled 300 px: the glow is still centred on the dial (within 2 px)',
              g[4] >= 299 and abs(g[0] - g[2]) <= 2 and abs(g[1] - g[3]) <= 2, g)
        b.ev('window.scrollTo(0, 0)')
        sw = b.ev('[document.scrollingElement.scrollWidth, document.documentElement.clientWidth]')
        check('phone: still no horizontal scroll with the glow in place', sw[0] == sw[1], sw)

        # -- no hard edge at the column's sides (Glow level)
        for (w, h) in ((2560, 1440), (390, 844)):
            b.resize(w, h)
            b.goto(dac.url + '?effects=glow')
            b.resize(w, h)
            b.sleep(0.7)
            info, out = edge_report(b, f'{w}')
            for side, (edge, beside, clean) in out.items():
                if clean < 5:
                    print(f'skip {w}x{h}: the column\'s {side} side is not measurable '
                          f'(content crosses all but {clean} rows)')
                    continue
                check(f'{w}x{h}: no hard edge at the column\'s {side} side (edge step {edge}, '
                      f'steps beside {beside}, {clean} clean rows)',
                      edge <= max(beside, 1), (info, out))
        # amp: the layer follows the meters
        b.resize(1280, 900)
        b.goto(amp.url + '?effects=glow')
        b.sleep(0.5)
        g = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
          const m = document.querySelector('.meters').getBoundingClientRect();
          return [g.left + g.width / 2, g.top + g.height / 2, m.left + m.width / 2,
                  m.top + m.height / 2]; })()""")
        check('amp: the glow is centred on the meters (within 2 px)',
              abs(g[0] - g[2]) <= 2 and abs(g[1] - g[3]) <= 2, g)
        # -- the header does not wrap at phone widths, for a guest too (button, pill, Sign in)
        for who, url in (('signed in', dac.url), ('guest', guest.url)):
            for w in (360, 390):
                b.resize(w, 844)
                b.goto(url)
                got = b.ev('innerWidth')
                check(f'{who} at {w}: the viewport really is {w} px wide', abs(got - w) <= 1, got)
                rows = b.ev("""(() => {
                  const top = (s) => { const e = document.querySelector(s);
                    return e ? e.getBoundingClientRect() : null; };
                  const items = ['.top .wm', '.fxbtn', '#part-link', '.side a.pill']
                    .map(top).filter(Boolean);
                  return { tops: items.map(r => Math.round(r.top + r.height / 2)),
                           right: Math.max(...items.map(r => r.right)), n: items.length,
                           top: document.querySelector('.top').getBoundingClientRect().height,
                           w: document.documentElement.clientWidth };
                })()""")
                one_row = max(rows['tops']) - min(rows['tops']) <= 3
                check(f'{who} at {w}: wordmark, button and pills share one header row, inside the '
                      f'viewport ({rows["n"]} items)', one_row and rows['right'] <= rows['w'], rows)
        check('no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()
        amp.stop()
        guest.stop()


run()
print(f'effects_layout.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
