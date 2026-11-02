# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Response graph under a finger and a pinch, in Chromium under mobile and touch emulation
(390x844, CDP Input.dispatchTouchEvent, as touch.py): a finger that goes down on a point drags
that band and the page does not scroll; a finger that goes down on the graph away from every point
scrolls the page and writes nothing; a touch between two points within 24 px grabs the nearer; two
fingers spreading to twice their distance halve the picked band's Q (within one step) with the
page neither zoomed nor scrolled; with no band picked a pinch acts on the band nearest the
fingers' midpoint; a second finger landing on a point already held makes a pinch of that band;
a touch cancelled mid-drag ends it and lets the tone part's waiting swap land; a third finger
during a pinch is ignored and lifting it changes nothing; a touch on a point right after a touch
drag's release, its writes still waiting, starts a new drag whose writes go after the first's.
Chromium only: headless Firefox delivers touch events but does not pan on them (see touch.py).
Numbers from the page, no pictures. Usage: python3 tone_touch.py"""
import json
import math
import sys
import urllib.request

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


# What the page records: every request it posts, and every touch event's default-prevented state.
HOOK = """(() => {
  window.__posts = []; window.__toasts = []; window.__touch = [];
  window.__delay = 0; window.__pending = 0;
  const f = window.fetch;
  window.fetch = async (...a) => {
    window.__posts.push([String(a[0]), a[1] && a[1].body]);
    if (window.__delay && /eq\\/bands/.test(String(a[0]))) {
      window.__pending++;
      await new Promise((r) => setTimeout(r, window.__delay));
      window.__pending--;
    }
    return f(...a);
  };
  new MutationObserver((list) => {
    for (const m of list) for (const n of m.addedNodes) {
      if (n.nodeType === 1 && n.classList.contains('toast')) window.__toasts.push(n.textContent);
    }
  }).observe(document.body, { subtree: true, childList: true });
  for (const type of ['touchstart', 'touchmove']) {
    window.addEventListener(type, (e) => window.__touch.push([type, e.defaultPrevented]),
      { passive: true });
  }
})()"""
STATE = """(() => {
  const f = document.querySelector('#part-tone .eqg'); const r = JSON.parse(f.dataset.readings);
  const p = f.querySelector('.plot').getBoundingClientRect();
  const marks = {};
  for (const m of f.querySelectorAll('.mk')) {
    marks[m.dataset.band] = [p.left + m.getAttribute('cx') / 1000 * p.width,
                             p.top + m.getAttribute('cy') / 260 * p.height];
  }
  const steps = JSON.parse(document.getElementById('tone-steps').textContent);
  const picked = document.querySelector('#part-bands .band[aria-pressed="true"]');
  return { mode: r.mode, bands: r.bands, marks, steps, plot: [p.left, p.top, p.width, p.height],
    scrollY: window.scrollY, scale: window.visualViewport.scale,
    picked: picked ? Number(picked.dataset.band) : null,
    cells: [...document.querySelectorAll('#part-bands .band')]
      .map((row) => [...row.children].slice(1).map((c) => c.textContent)),
    posts: window.__posts.filter((q) => /eq\\/bands/.test(q[0])).map((q) => JSON.parse(q[1])),
    toasts: window.__toasts.slice(), drag: document.querySelectorAll('.dlab').length,
    touch: window.__touch.slice() }; })()"""


def state(b):
    return b.ev(STATE)


def post(dac, path, body):
    urllib.request.urlopen(urllib.request.Request(
        f'{dac.url}api/v1/devices/Gungnir%202/{path}', data=json.dumps(body).encode(),
        method='POST', headers={'Content-Type': 'application/json',
                                'Origin': dac.url.rstrip('/')}), timeout=10).read()


# ------------------------------------------------------------------------------- touch input

def fingers(pts):
    return [{'x': x, 'y': y, 'id': i} for i, (x, y) in enumerate(pts)]


def down(b, *pts):
    """All fingers in `pts` are down (the ones already down keep their place in the list)."""
    b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': fingers(pts)})
    b.sleep(0.05)


def move(b, *pts):
    b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': fingers(pts)})
    b.sleep(0.03)


def cancel(b, settle=1.5):
    """The browser takes the touches away (a system gesture, a call): touchcancel."""
    b.send('Input.dispatchTouchEvent', {'type': 'touchCancel', 'touchPoints': []})
    b.sleep(settle)


def lift(b, finger, pt, settle=0.3):
    """The finger numbered `finger`, at `pt`, is lifted; the others stay down."""
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd',
                                        'touchPoints': [{'x': pt[0], 'y': pt[1], 'id': finger}]})
    b.sleep(settle)


def up(b, settle=1.2):
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
    b.sleep(settle)


def stroke(b, x, y, to, step=4):
    """One finger down at (x, y), moved in about `step` px steps to `to`, lifted there."""
    n = max(1, int(math.hypot(to[0] - x, to[1] - y) // step))
    down(b, (x, y))
    for i in range(1, n + 1):
        move(b, (x + (to[0] - x) * i / n, y + (to[1] - y) * i / n))
    up(b)


def pinch(b, mid, start, end, steps=8):
    """Two fingers `start` px apart (side by side) about `mid`, spread to `end` px apart."""
    def at(d):
        return [(mid[0] - d / 2, mid[1]), (mid[0] + d / 2, mid[1])]
    down(b, *at(start))
    for i in range(1, steps + 1):
        move(b, *at(start + (end - start) * i / steps))
    up(b)


def fresh(b, url):
    b.goto(url + '?effects=off', settle=1.0)
    b.ev(HOOK)
    centre(b)


def centre(b):
    b.ev("document.querySelector('#part-tone .eqg').scrollIntoView("
         "{ block: 'center', behavior: 'instant' })")
    b.sleep(0.4)


# The page's expression for the raw frequency step of a band (0-based index, then a distance in
# px from the plot's left, twice) whose point is nearest that distance.
NEAREST = """(async () => {
  const { markerPoint } = await import(new URL('assets/core/tone-curve.js', document.baseURI).href);
  const steps = JSON.parse(document.getElementById('tone-steps').textContent);
  const width = document.querySelector('#part-tone .eqg .plot').getBoundingClientRect().width;
  let best = null;
  steps.bands[%d].f.forEach(([hz], raw) => {
    if (typeof hz !== 'number') return;
    const x = markerPoint('eq', { f0: hz, gainDb: 0, q: 1 }).x / 1000 * width;
    if (best === null || Math.abs(x - %f) < Math.abs(best.x - %f)) best = { raw, x };
  });
  return best.raw; })()"""


def q_of(s, band):
    return s['steps']['q'][s['bands'][band - 1][2]][0]


# ------------------------------------------------------------------------------- the checks

dac = common.Daemon('gungnir2-v2')
b = common.Chromium(size=(390, 844), touch=True,
                    args=['--disable-features=OverscrollHistoryNavigation'])
try:
    # the page is put in EQ first, so a drag is an ordinary one
    post(dac, 'eq/mode', {'mode': 'eq'})
    fresh(b, dac.url)
    s = state(b)
    check('the graph reads a band set in EQ with five points and nothing picked',
          s['mode'] == 'eq' and len(s['marks']) == 5 and s['picked'] is None,
          [s['mode'], s['picked']])

    # a finger down on band 3's point drags it; the page stays where it is
    x, y = s['marks']['3']
    before = s['cells'][2]
    # a short way right: band 3's frequency window is narrow on a phone-sized plot
    stroke(b, x, y, (x + 20, y - 25))
    a = state(b)
    cx, cy = a['marks']['3']
    check('a finger dragging from a point moves that band (frequency and gain change)',
          a['cells'][2][0] != before[0] and a['cells'][2][1] != before[1], [before, a['cells'][2]])
    check('  the point sits under the finger\'s last place (1.5 px)',
          abs(cx - (x + 20)) <= 1.5 and abs(cy - (y - 25)) <= 1.5, [cx - x - 20, cy - y + 25])
    check('  the page did not scroll', a['scrollY'] == s['scrollY'], [s['scrollY'], a['scrollY']])
    check('  the others stayed', all(a['bands'][i] == s['bands'][i] for i in (0, 1, 3, 4)))
    check('  the band picked in the table is band 3', a['picked'] == 3, a['picked'])
    check('  it wrote steps of frequency and gain only (raw), and the label is gone',
          a['posts'] and all('raw' in p and p['param'] in ('frequency', 'gain')
                             for p in a['posts']) and a['drag'] == 0, [a['posts'], a['drag']])
    check('  its touch start and moves were prevented',
          a['touch'] and all(t[1] for t in a['touch']), a['touch'])

    # a finger down on the graph away from every point scrolls the page and writes nothing
    centre(b)
    s = state(b)
    px, py, pw, ph = s['plot']

    def far(x, y):
        return min(math.hypot(x - mx, y - my) for mx, my in s['marks'].values())
    spot = max(((px + 6 + i * 8, py + 5) for i in range(int((pw - 12) // 8))),
               key=lambda p: far(*p))
    posts_before = len(s['posts'])
    touches_before = len(s['touch'])
    stroke(b, spot[0], spot[1], (spot[0], spot[1] + 220), step=22)
    a = state(b)
    print(f'      away from every point ({far(*spot):.0f} px): scrollY {s["scrollY"]} -> '
          f'{a["scrollY"]}')
    check('a finger down away from every point scrolls the page', a['scrollY'] < s['scrollY'] - 50,
          [s['scrollY'], a['scrollY']])
    check('  and writes nothing', len(a['posts']) == posts_before and a['drag'] == 0,
          [a['posts'][posts_before:], a['drag']])
    check('  and no touch was prevented', all(not t[1] for t in a['touch'][touches_before:]),
          a['touch'][touches_before:])

    # right after a drag's release, with its writes still unanswered (held 3 s here), a finger
    # down away from every point scrolls the page: the drag is over
    centre(b)
    s = state(b)
    px, py, pw, ph = s['plot']
    x, y = s['marks']['3']
    b.ev('window.__delay = 3000')
    down(b, (x, y))
    move(b, (x + 6, y - 6))
    move(b, (x + 14, y - 12))
    up(b, settle=0.05)
    mid = b.ev('({ pending: window.__pending, drag: document.querySelectorAll(".dlab").length })')
    spot = max(((px + 6 + i * 8, py + 5) for i in range(int((pw - 12) // 8))),
               key=lambda p: min(math.hypot(p[0] - mx, p[1] - my)
                                 for mx, my in s['marks'].values()))
    scroll0 = b.ev('window.scrollY')
    touches_before = len(b.ev('window.__touch'))
    stroke(b, spot[0], spot[1], (spot[0], spot[1] + 220), step=22)
    a = state(b)
    print(f'      a drag just let go of ({mid["pending"]} writes unanswered): scrollY '
          f'{scroll0} -> {a["scrollY"]}')
    check('the drag just let go of has writes still unanswered and no label',
          mid['pending'] > 0 and mid['drag'] == 0, mid)
    check('  a finger down away from every point scrolls the page at once',
          a['scrollY'] < scroll0 - 50, [scroll0, a['scrollY']])
    check('  and none of its touch events was prevented',
          all(not t[1] for t in a['touch'][touches_before:]), a['touch'][touches_before:])
    b.ev('window.__delay = 0')
    b.sleep(3.5)
    centre(b)

    # two points within 24 px of a touch: the nearer is grabbed. Bands 3 and 4 are put as close
    # as their windows let them to 168 and 192 px from the plot's left (the windows end
    # where the next begins, so the points can be near)
    centre(b)
    s = state(b)
    for band, at in ((3, 168), (4, 192)):
        raw = b.ev(NEAREST % (band - 1, at, at))
        post(dac, f'eq/bands/{band}', {'param': 'frequency', 'raw': raw})
    b.sleep(1.2)
    centre(b)
    s = state(b)
    (x3, y3), (x4, y4) = s['marks']['3'], s['marks']['4']
    gap = math.hypot(x4 - x3, y4 - y3)
    print(f'      band 3 and band 4 are {gap:.1f} px apart')
    check('band 4 stands 12 to 40 px from band 3 (both reachable from between)',
          12 <= gap <= 40, gap)
    t = (x3 + (x4 - x3) * 0.35, y3 + (y4 - y3) * 0.35)  # nearer band 3, both within 24 px
    down(b, t)
    a = state(b)
    check('a touch between two points within 24 px grabs the nearer (band 3)',
          a['picked'] == 3 and a['drag'] == 1, [a['picked'], a['drag']])
    up(b)
    t = (x3 + (x4 - x3) * 0.65, y3 + (y4 - y3) * 0.65)  # nearer band 4
    down(b, t)
    a = state(b)
    check('  and at the other side the other (band 4)', a['picked'] == 4 and a['drag'] == 1,
          [a['picked'], a['drag']])
    up(b)

    # a pinch spreading to twice the distance halves the picked band's Q
    centre(b)
    s = state(b)
    px, py, pw, ph = s['plot']
    check('band 4 is the band picked', s['picked'] == 4, s['picked'])
    q0 = q_of(s, 4)
    posts_before = len(s['posts'])
    pinch(b, (px + pw * 0.5, py + 8), 60, 120)
    a = state(b)
    q1 = q_of(a, 4)
    print(f'      pinch 60 -> 120 px: Q {q0} -> {q1}, scale {a["scale"]}, scrollY '
          f'{s["scrollY"]} -> {a["scrollY"]}')
    check('a pinch to twice the distance halves the picked band\'s Q (within 0.1 octave)',
          abs(math.log2(q1 / q0) + 1) < 0.1, [q0, q1, a['bands'][3]])
    check('  the frequency and gain stayed', a['bands'][3][:2] == s['bands'][3][:2],
          [s['bands'][3], a['bands'][3]])
    check('  only Q was written', all(p['param'] == 'q' for p in a['posts'][posts_before:])
          and len(a['posts']) > posts_before, a['posts'][posts_before:])
    check('  the page did not zoom', a['scale'] == 1, a['scale'])
    check('  and did not scroll', a['scrollY'] == s['scrollY'], [s['scrollY'], a['scrollY']])
    check('  the label is gone', a['drag'] == 0, a['drag'])

    # with no band picked a pinch acts on the band nearest the fingers' midpoint
    fresh(b, dac.url)
    s = state(b)
    px, py, pw, ph = s['plot']
    check('a fresh page has no band picked', s['picked'] is None, s['picked'])
    mx = s['marks']['2'][0] + 6
    q0 = q_of(s, 2)
    pinch(b, (mx, py + 8), 70, 140)
    a = state(b)
    q1 = q_of(a, 2)
    check('with no band picked a pinch acts on the band nearest the midpoint (band 2)',
          abs(math.log2(q1 / q0) + 1) < 0.1 and a['picked'] == 2, [q0, q1, a['picked']])
    check('  the other bands stayed', all(a['bands'][i] == s['bands'][i] for i in (0, 2, 3, 4)),
          [s['bands'], a['bands']])
    check('  the page did not zoom or scroll', a['scale'] == 1 and a['scrollY'] == s['scrollY'],
          [a['scale'], a['scrollY'], s['scrollY']])

    # a second finger landing while the first holds a point makes a pinch of that band
    fresh(b, dac.url)
    s = state(b)
    x5, y5 = s['marks']['5']
    q0 = q_of(s, 5)
    first = (x5, y5)
    second = (x5 - 60, y5 + 40)
    down(b, first)
    down(b, first, second)
    for i in range(1, 9):
        k = 1 + i / 8
        move(b, first, (first[0] + (second[0] - first[0]) * k,
                        first[1] + (second[1] - first[1]) * k))
    up(b)
    a = state(b)
    q1 = q_of(a, 5)
    check('a second finger on a held point pinches that band\'s Q (twice as far halves it)',
          abs(math.log2(q1 / q0) + 1) < 0.1 and a['picked'] == 5, [q0, q1, a['picked']])
    check('  the point stayed where it was', a['bands'][4][:2] == s['bands'][4][:2],
          [s['bands'][4], a['bands'][4]])
    check('  the page did not zoom or scroll', a['scale'] == 1 and a['scrollY'] == s['scrollY'],
          [a['scale'], a['scrollY'], s['scrollY']])

    # a touchcancel mid-drag ends the drag and the tone part's hold lets go: a write from outside
    # that waited lands, and the next touch on a point drags again
    fresh(b, dac.url)
    s = state(b)
    x, y = s['marks']['3']
    b.ev("window.__fig = document.querySelector('#part-tone .eqg')")
    down(b, (x, y))
    move(b, (x + 6, y - 6))
    move(b, (x + 14, y - 12))
    post(dac, 'eq/bands/4', {'param': 'gain', 'raw': 110})
    b.sleep(0.8)
    held = b.ev("""({ same: window.__fig.isConnected,
      drag: document.querySelectorAll('.dlab').length })""")
    cancel(b)
    a = state(b)
    swapped = b.ev('!window.__fig.isConnected')
    check('a drag by a finger holds the swap of the tone part',
          held['same'] and held['drag'] == 1, held)
    check('a touchcancel mid-drag ends the drag (no label)', a['drag'] == 0, a['drag'])
    check('  the band moved where the finger had got to and the device has it',
          a['bands'][2][:2] != s['bands'][2][:2], [s['bands'][2], a['bands'][2]])
    check('  the swap that waited landed with the outside write in the readings',
          swapped and a['bands'][3][1] == 110, [swapped, a['bands'][3]])
    x, y = a['marks']['3']
    down(b, (x, y))
    again = state(b)
    up(b)
    check('  and the next touch on a point drags again', again['drag'] == 1, again['drag'])

    # a third finger during a pinch is ignored, and lifting it changes nothing
    fresh(b, dac.url)
    s = state(b)
    px, py, pw, ph = s['plot']
    mx = s['marks']['2'][0] + 6
    q0 = q_of(s, 2)
    low = [(mx - 35, py + 8), (mx + 35, py + 8)]
    third = (px + pw * 0.8, py + ph * 0.7)

    def pair(d):
        return [(mx - d / 2, py + 8), (mx + d / 2, py + 8)]
    down(b, *low)
    down(b, *low, third)
    mid = state(b)
    for i in range(1, 5):
        move(b, *pair(70 + 70 * i / 8), third)
    lift(b, 2, third)
    lifted = state(b)
    posts_then = len(lifted['posts'])
    b.sleep(0.4)
    still = state(b)
    for i in range(5, 9):
        move(b, *pair(70 + 70 * i / 8))
    up(b)
    a = state(b)
    q1 = q_of(a, 2)
    check('a third finger landing in a pinch leaves the drag going', mid['drag'] == 1, mid['drag'])
    check('  lifting it ends nothing and writes nothing',
          lifted['drag'] == 1 and still['drag'] == 1 and len(still['posts']) == posts_then,
          [lifted['drag'], still['drag'], posts_then, len(still['posts'])])
    check('  the pinch goes on to halve the band\'s Q (twice as far, within 0.1 octave)',
          abs(math.log2(q1 / q0) + 1) < 0.1 and a['picked'] == 2, [q0, q1, a['picked']])
    check('  only Q was written, the others stayed',
          all(p['param'] == 'q' for p in a['posts']) and a['posts']
          and all(a['bands'][i] == s['bands'][i] for i in (0, 2, 3, 4)),
          [a['posts'], s['bands'], a['bands']])
    check('  the page did not zoom or scroll', a['scale'] == 1 and a['scrollY'] == s['scrollY'],
          [a['scale'], a['scrollY'], s['scrollY']])
    check('  no label is left', a['drag'] == 0, a['drag'])

    # a touch on a point while the last touch drag's writes are still waiting starts a new drag,
    # and its writes follow the first drag's
    fresh(b, dac.url)
    s = state(b)
    x, y = s['marks']['3']
    b.ev('window.__delay = 2000; window.__posts.length = 0')
    down(b, (x, y))
    for i in range(1, 6):
        move(b, (x - 3 * i, y))
    up(b, settle=0.05)
    a = state(b)
    mx, my = a['marks']['3']
    pending = b.ev('window.__pending')
    down(b, (mx, my))
    mid = state(b)
    move(b, (mx + 14, my))
    up(b, settle=0.05)
    b.sleep(9.0)
    b.ev('window.__delay = 0')
    a = state(b)
    truth = b.ev("fetch('api/v1/devices/Gungnir%202/model').then((r) => r.json())"
                 ".then((j) => j.device.tone.readings.bands)")
    check('the first touch drag was let go of with its writes waiting', pending > 0, pending)
    check('a touch on its point right after starts a new drag', mid['drag'] == 1, mid['drag'])
    check('  the point ends under the second finger (1.5 px)',
          abs(a['marks']['3'][0] - (mx + 14)) <= 1.5, [a['marks']['3'], mx + 14])
    check('  and the device has the second drag\'s place, not the first\'s last write',
          a['bands'][2] == truth[2], [a['bands'][2], truth[2]])
    check('  no label is left', a['drag'] == 0, a['drag'])

    check('no page error', b.errors == [], b.errors)
finally:
    b.close()
    dac.stop()

print(f'tone_touch.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
