# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A finger stirs the smoke, in Chromium under mobile and touch emulation (390x844, CDP
Input.dispatchTouchEvent, over ordinary page area, not the knob). The browser pans on such a
touch and cancels its pointer stream, so the smoke reads the touch events themselves. Checks: a
touch stroke applies pointer splats and puts dye along its path; a firm flick across about three
quarters of the width in 0.10-0.15 s (page time) forms a note once it settles, a 255 px one in
0.10 s none; a stroke that is not a flick forms none; the page still scrolls under a vertical
drag and nothing prevents a touch move; the finger's pointer moves count once (none through the
pointer path).
Headless Firefox delivers touch events to the page (the profile pref dom.w3c_touch_events.enabled
= 1 and a WebDriver pointer action of type touch) but does not pan on them, so it never cancels
the pointer stream and cannot show the case this covers: Chromium only. Numbers only, no
pictures. Usage: python3 touch.py"""
import sys
import time

import common

fails = []
passed = 0


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


HOOK = """(async () => {
  window.__fx = await import(new URL('assets/themes/ember/effects.js', document.baseURI).href);
  window.__tm = []; window.__prevented = 0; window.__pm = 0;
  window.addEventListener('touchmove', (e) => {
    window.__tm.push(e.timeStamp / 1000); if (e.defaultPrevented) window.__prevented++; });
  window.addEventListener('pointermove', (e) => { if (e.pointerType === 'touch') window.__pm++; });
  return true; })()"""


def touch(b, pts, secs):
    """A touch stroke through `pts` spread evenly over `secs` (events carry their own times)."""
    t0 = time.time()
    n = len(pts) - 1
    for i, (x, y) in enumerate(pts):
        b.send('Input.dispatchTouchEvent', {
            'type': 'touchStart' if i == 0 else 'touchMove', 'touchPoints': [{'x': x, 'y': y}],
            'timestamp': t0 + secs * i / n})
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': [],
                                        'timestamp': t0 + secs})
    b.sleep(0.3)


def line(x0, y0, x1, y1, n):
    return [(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(n + 1)]


def stats(b):
    return b.ev('window.__fx.stats()')


def band(b, y0, y1):
    """The dye's alpha summed over the grid rows between viewport rows y0 and y1."""
    return b.ev("""(() => { const d = window.__fx.readback().dye, h = innerHeight;
      let s = 0; for (let gy = 0; gy < d.h; gy++) { const y = (1 - (gy + 0.5) / d.h) * h;
        if (y < %d || y > %d) continue;
        for (let gx = 0; gx < d.w; gx++) s += d.data[(gy * d.w + gx) * 4 + 3]; } return s; })()"""
                % (y0, y1))


def fresh(b, url):
    b.goto(url, settle=1.0)
    b.ev(HOOK)
    b.sleep(2.5)
    b.ev('window.__fx.roll(0)')  # every settle wins, so a note forms wherever a stir settles


dac = common.Daemon('gungnir2-v2')
# the browser's own swipe-back gesture would leave the page on a stroke from the left edge
b = common.Chromium(size=(390, 844), touch=True,
                    args=['--disable-features=OverscrollHistoryNavigation'])
try:
    fresh(b, dac.url)
    before_dye = band(b, 560, 640)
    s0 = stats(b)
    touch(b, line(50, 600, 350, 600, 8), 0.4)
    s1 = stats(b)
    print(f'      touch stroke: splats {s0["splats"]["pointer"]} -> {s1["splats"]["pointer"]}, '
          f'dye in the rows {before_dye:.3f} -> {band(b, 560, 640):.3f}')
    check('a touch stroke over ordinary page area applies pointer splats',
          s1['splats']['pointer'] > s0['splats']['pointer'], [s0, s1])
    check('  and puts dye along its path (the rows it crossed hold more)',
          band(b, 560, 640) > before_dye + 0.05, [before_dye, band(b, 560, 640)])
    check('  the finger\'s pointer moves are left alone (the touch events carry it)',
          s1['splats']['pointer'] - s0['splats']['pointer'] <= len(b.ev('window.__tm')),
          [s1['splats'], len(b.ev('window.__tm'))])
    check('  nothing prevented a touch move', b.ev('window.__prevented') == 0)
    print('      touchmove page times (s):',
          [round(t - b.ev('window.__tm')[0], 3) for t in b.ev('window.__tm')])
    b.sleep(3.0)
    check('  not a flick: an ordinary stroke still forms one note once it settles',
          stats(b)['notes']['stamped'] == 1, stats(b)['notes'])

    # over the knob (touch-action: none) the finger's pointer stream is not cancelled: its moves
    # still count once, through the touch events
    fresh(b, dac.url)
    b.ev('window.__tm.length = 0')
    s0 = stats(b)
    touch(b, line(50, 300, 350, 300, 8), 0.4)
    s1 = stats(b)
    moved = s1['splats']['pointer'] - s0['splats']['pointer']
    print(f'      over the knob: {moved} splats from {len(b.ev("window.__tm"))} touch moves, '
          f'{b.ev("window.__pm")} touch pointer moves seen')
    check('a touch stroke whose pointer stream goes on (over the knob) counts each move once',
          0 < moved <= len(b.ev('window.__tm')) and b.ev('window.__pm') > 0,
          [moved, b.ev('window.__tm'), b.ev('window.__pm')])

    # a flick: 255 px in 0.10 s stamps one once it settles, 312 px in 0.10 s one more
    fresh(b, dac.url)
    touch(b, line(50, 600, 305, 600, 4), 0.10)
    b.sleep(3.0)
    n = stats(b)['notes']
    check('a 255 px touch flick in 0.10 s stamps one once it settles', n['stamped'] == 1, n)
    first = n['stamped']
    b.sleep(2.0)
    b.ev('window.__tm.length = 0')
    touch(b, line(40, 600, 352, 600, 4), 0.10)
    b.sleep(3.0)
    n = stats(b)['notes']
    tm = b.ev('window.__tm')
    print(
        f'      the firm flick: {n}; touchmove spacing '
        f'{[round(b2 - a, 3) for a, b2 in zip(tm, tm[1:])]}',
    )
    check('a firm touch flick across about three quarters of the width (312 px, 0.10 s) stamps '
          'a note', n['stamped'] == first + 1 and n['live'] == 1, n)
    last = n.get('last') or {}
    check('  near the flick', bool(last) and abs(last['y'] - 600) <= last['cell']
          and 0 <= last['x'] <= 390, last)

    # the page still scrolls
    fresh(b, dac.url)
    y0 = b.ev('scrollY')
    touch(b, line(200, 700, 200, 300, 8), 0.4)
    y1 = b.ev('scrollY')
    check('a vertical touch drag still scrolls the page', y1 > y0 + 50, [y0, y1])
    check('  and nothing prevented a touch move', b.ev('window.__prevented') == 0)
    errs = b.errors
    check('no page error', errs == [], errs)
finally:
    b.close()
    dac.stop()
print(f'{passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
