# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A click on the volume knob's ring jumps there: the reading goes to the step the click's angle
stands for; a click above three quarters climbs; a click while a climb shows only stops it; a
cap held in [web] max_volume_db holds a click too; a press that moves 10 px is a drag; a click in
the centre, on the readout, in the SVG's corners, in the gap at the bottom and on the step
already held write nothing; a tap under
Chromium's touch emulation jumps like a click. Numbers from the page, no pictures.
Usage: python3 knob_click.py chromium|firefox"""
import math
import sys

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
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


HOOK = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push(String(a[0])); return f(...a); }; })()"""
STATE = """(() => { const k = document.querySelector('sv-knob'); const g = k.geometry();
  return { raw: k.raw, lo: k.lo, hi: k.hi, cap: k.cap, climbing: k.climbing(),
           cx: g.cx, cy: g.cy, r: g.radius,
           posts: window.__posts.filter((p) => /volume/.test(p)).length }; })()"""


def state(b):
    return b.ev(STATE)


def angle_of(s, step):
    return -135 + 270 * (step - s['lo']) / (s['hi'] - s['lo'])


def pt(s, deg, k=0.9):
    a = math.radians(deg)
    return s['cx'] + k * s['r'] * math.sin(a), s['cy'] - k * s['r'] * math.cos(a)


def at_frac(s, frac):
    """The step a fraction of the range up, and the point on the ring that stands for it."""
    step = s['lo'] + round(frac * (s['hi'] - s['lo']))
    return step, pt(s, angle_of(s, step))


def press_move_release(b, x, y, dx, dy):
    """A press at (x, y), a move by (dx, dy) in steps of 2 px with the button down, a release."""
    n = max(1, int(math.hypot(dx, dy) // 2))
    if browser == 'chromium':
        base = {'button': 'left', 'clickCount': 1}
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y, **base,
                                            'buttons': 1})
        for i in range(1, n + 1):
            b.send('Input.dispatchMouseEvent', {
                'type': 'mouseMoved', 'x': x + dx * i / n, 'y': y + dy * i / n,
                'button': 'left', 'buttons': 1})
        b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x + dx, 'y': y + dy,
                                            **base})
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                 'y': int(y)}, {'type': 'pointerDown', 'button': 0}]
        for i in range(1, n + 1):
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport',
                         'x': int(x + dx * i / n), 'y': int(y + dy * i / n)})
        acts.append({'type': 'pointerUp', 'button': 0})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})
    b.sleep(0.2)


def make(daemon, **kw):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(1280, 1100), **kw)
    b.goto(daemon.url + '?effects=off', settle=1.0)
    b.ev(HOOK)
    b.sleep(0.5)
    return b


dac = common.Daemon('gungnir2-v2')
b = make(dac)
try:
    s = state(b)
    check('the dial is live with a range', s['raw'] is not None and s['hi'] > s['lo'], s)
    quarter = s['lo'] + 0.75 * (s['hi'] - s['lo'])

    # a click below three quarters sets that step
    step, (x, y) = at_frac(s, 0.40)
    b.click(x, y)
    b.sleep(1.0)
    got = state(b)
    check('a click at 40 % of the sweep sets that step', got['raw'] == step and not got['climbing'],
          [step, got])
    step2, (x, y) = at_frac(s, 0.20)
    b.click(x, y)
    b.sleep(1.0)
    got = state(b)
    check('  a click lower down sets that one', got['raw'] == step2, [step2, got])

    # a click on the step held writes nothing
    before = state(b)
    _, (x, y) = at_frac(s, 0.20)
    b.click(x, y)
    b.sleep(0.5)
    after = state(b)
    check('a click on the step already held writes nothing',
          after['posts'] == before['posts'] and after['raw'] == before['raw'], [before, after])

    # a click above three quarters climbs; a click during it stops it and does not jump
    top, (x, y) = at_frac(s, 0.97)
    b.click(x, y)
    b.sleep(0.8)
    a = state(b)
    check('a click at 97 % starts a climb (three quarters written, the knob shows the climb)',
          a['climbing'] and a['raw'] >= quarter - 1 and a['raw'] < top, [top, a])
    b.sleep(0.5)
    a2 = state(b)
    check('  the reading rises over time', a2['raw'] > a['raw'], [a, a2])
    low, (x, y) = at_frac(s, 0.30)
    b.click(x, y)
    b.sleep(0.8)
    c = state(b)
    check('a click during the climb stops it and does not jump',
          not c['climbing'] and c['raw'] >= a2['raw'] and c['raw'] != low, [a2, c])
    b.sleep(1.0)
    c2 = state(b)
    check('  the reading no longer rises', c2['raw'] == c['raw'], [c, c2])
    b.click(x, y)
    b.sleep(1.0)
    c3 = state(b)
    check('  the next click jumps', c3['raw'] == low, [low, c3])

    # a press moved 10 px round the ring is a drag, not a jump to the press point
    start = state(b)
    far, _ = at_frac(s, 0.70)
    deg = angle_of(s, far)
    x, y = pt(s, deg)
    a_rad = math.radians(deg)
    press_move_release(b, x, y, 10 * math.cos(a_rad), 10 * math.sin(a_rad))
    b.sleep(1.0)
    d = state(b)
    check('a press moved 10 px round the ring is a drag from where the volume was',
          d['raw'] != far and start['raw'] < d['raw'] <= start['raw'] + 6, [start, far, d])

    # the centre and the gap write nothing
    before = state(b)
    b.click(s['cx'], s['cy'])
    b.sleep(0.5)
    mid = state(b)
    check('a click in the centre writes nothing', mid['posts'] == before['posts']
          and mid['raw'] == before['raw'], [before, mid])
    # the readout and the empty corners of the dial's box write nothing
    spots = b.ev("""(() => { const k = document.querySelector('sv-knob');
      const mid = (sel) => { const r = document.createRange();
        r.selectNodeContents(k.querySelector(sel)); const b = r.getBoundingClientRect();
        return [(b.left + b.right) / 2, (b.top + b.bottom) / 2]; };
      const v = k.querySelector('svg').getBoundingClientRect();
      const n = document.createRange(); n.selectNodeContents(k.querySelector('.big .num'));
      const nb = n.getBoundingClientRect(), ny = (nb.top + nb.bottom) / 2;
      return { unit: mid('.big .unit'), numR: [nb.right - 2, ny], numL: [nb.left + 2, ny],
               corner: [v.left + 4, v.top + 4], corner2: [v.right - 4, v.bottom - 4] }; })()""")
    for name, label in (('unit', "the readout's unit"), ('numR', "the readout's last digit"),
                        ('numL', "the readout's first character"),
                        ('corner', "the SVG's top corner"),
                        ('corner2', "the SVG's bottom corner")):
        before = state(b)
        b.click(*spots[name])
        b.sleep(0.5)
        got = state(b)
        check(f'a click on {label} writes nothing',
              got['posts'] == before['posts'] and got['raw'] == before['raw'],
              [name, spots[name], before, got])
    for deg in (180, 150, -150):
        x, y = pt(s, deg)
        b.click(x, y)
        b.sleep(0.5)
        gap = state(b)
        check(f'a click in the gap ({deg} deg) writes nothing',
              gap['posts'] == before['posts'] and gap['raw'] == before['raw'], [before, gap])
    check('no page error', (b.errors if browser == 'chromium' else b.collect_errors()) == [])
finally:
    b.close()
    dac.stop()

# a cap held in [web] max_volume_db holds a click too
capped = common.Daemon('gungnir2-v2', web='auth = "none"\nmax_volume_db = -40.0')
b = make(capped)
try:
    s = state(b)
    check('the cap shows on the dial', s['cap'] is not None and s['cap'] < s['hi'], s)
    _, (x, y) = at_frac(s, 0.70)
    b.click(x, y)
    b.sleep(1.2)
    got = state(b)
    check('a click above the cap lands at the cap', got['raw'] == s['cap'], [s['cap'], got])
finally:
    b.close()
    capped.stop()

# a tap under touch emulation jumps like a click
if browser == 'chromium':
    tap = common.Daemon('gungnir2-v2')
    b = common.Chromium(size=(390, 844), touch=True,
                        args=['--disable-features=OverscrollHistoryNavigation'])
    try:
        b.goto(tap.url + '?effects=off', settle=1.0)
        b.ev(HOOK)
        b.sleep(0.5)
        s = state(b)
        step, (x, y) = at_frac(s, 0.35)
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart',
                                            'touchPoints': [{'x': x, 'y': y}]})
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        b.sleep(1.0)
        got = state(b)
        check('a tap on the ring jumps like a click', got['raw'] == step, [step, got])
    finally:
        b.close()
        tap.stop()
print(f'knob_click.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
