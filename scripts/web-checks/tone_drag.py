# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Response graph's points under the mouse: a left drag moves a band's frequency and gain and
the device's band follows (the point sits under the release, the readings hold the steps the label
showed), a drag straight across leaves the gain alone, a right drag halves the Q for 120 px to the
right, two wheel notches over a point raise the Q by two and the wheel elsewhere is the page's, a
wheel burst keeps its band while the pointer drifts off the point (and ends on a pause, on the
pointer leaving the plot and on a press), a guest grabs and posts nothing, a drag in Flat switches
the mode to EQ with one toast. In Chromium only (a press held across calls): a write from outside
during a drag does not swap the graph and lands after the release, a device that goes away
mid-drag toasts once and posts nothing after, and (the writes held in the page) a second press right
after a release starts a drag, a wheel notch after a release is the readings' Q, and a wheel burst
starts again from a Q set by someone else.
Numbers from the page, no pictures.
Usage: python3 tone_drag.py chromium|firefox"""
import json
import math
import sys
import time
import urllib.parse
import urllib.request

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


# What the page records for the checks: every write it posts, every popup that appeared, the last
# label shown while a point was dragged, and what a wheel event looked like to a later listener.
HOOK = """(() => {
  window.__posts = []; window.__toasts = []; window.__label = null; window.__wheel = [];
  const f = window.fetch;
  window.fetch = (...a) => {
    window.__posts.push([String(a[0]), a[1] && a[1].body, performance.now()]);
    return f(...a);
  };
  new MutationObserver((list) => {
    for (const m of list) for (const n of m.addedNodes) {
      if (n.nodeType === 1 && n.classList.contains('toast')) window.__toasts.push(n.textContent);
    }
    const l = document.querySelector('.dlab');
    if (l && l.textContent) window.__label = l.textContent;
  }).observe(document.body, { subtree: true, childList: true, characterData: true });
  document.addEventListener('wheel', (e) => window.__wheel.push(
    { prevented: e.defaultPrevented, dy: e.deltaY, mode: e.deltaMode }), { passive: true });
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
  return { mode: r.mode, bands: r.bands, marks, steps,
    plot: [p.left, p.top, p.width, p.height], scrollY: window.scrollY,
    scrollable: document.documentElement.scrollHeight > innerHeight,
    cells: [...document.querySelectorAll('#part-bands .band')]
      .map((row) => [...row.children].slice(1).map((c) => c.textContent)),
    posts: window.__posts.filter((q) => /eq\\/bands/.test(q[0]))
      .map((q) => [q[0], JSON.parse(q[1]), q[2]]),
    toasts: window.__toasts.slice(), label: window.__label,
    grab: document.querySelectorAll('.plot.can-grab').length,
    drag: document.querySelectorAll('.dlab').length, wheel: window.__wheel.slice(),
    peak: !!f.querySelector('.peak') }; })()"""


def state(b):
    return b.ev(STATE)


def seen(b):
    """The writes posted and the popups shown so far, for a page that may have lost its part."""
    return b.ev("""({ posts: window.__posts.filter((q) => /eq\\/bands/.test(q[0])).length,
      toasts: window.__toasts.slice(), drag: document.querySelectorAll('.dlab').length })""")


# ------------------------------------------------------------------------------ pointer input

def mouse_chromium(b, kind, x, y, button='left', buttons=0, **extra):
    b.send('Input.dispatchMouseEvent', {'type': kind, 'x': x, 'y': y, 'button': button,
                                        'buttons': buttons, 'clickCount': 1, **extra})


def press(b, x, y, button='left'):
    """Chromium only: the button goes down at (x, y) and stays down."""
    mouse_chromium(b, 'mouseMoved', x, y)
    mouse_chromium(b, 'mousePressed', x, y, button, 1 if button == 'left' else 2)


def move(b, x, y, button='left'):
    mouse_chromium(b, 'mouseMoved', x, y, button, 1 if button == 'left' else 2)
    b.sleep(0.03)


def release(b, x, y, button='left'):
    mouse_chromium(b, 'mouseReleased', x, y, button, 0)


def gesture(b, x, y, to, button='left', step=4):
    """A press at (x, y), moves in about `step` px steps through the points of `to` (a list), a
    release at the last one."""
    path = []
    px, py = x, y
    for tx, ty in to:
        n = max(1, int(math.hypot(tx - px, ty - py) // step))
        path += [(px + (tx - px) * i / n, py + (ty - py) * i / n) for i in range(1, n + 1)]
        px, py = tx, ty
    if browser == 'chromium':
        press(b, x, y, button)
        for mx, my in path:
            move(b, mx, my, button)
        release(b, *path[-1], button)
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                 'y': int(y)}, {'type': 'pointerDown', 'button': 0 if button == 'left' else 2}]
        for mx, my in path:
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport',
                         'x': int(mx), 'y': int(my)})
        acts.append({'type': 'pointerUp', 'button': 0 if button == 'left' else 2})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})
    b.sleep(1.2)


def wheel(b, x, y, dy):
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': y,
                                            'deltaX': 0, 'deltaY': dy})
    else:
        b._post('/actions', {'actions': [{'type': 'wheel', 'id': 'wheel', 'actions': [
            {'type': 'scroll', 'x': int(x), 'y': int(y), 'deltaX': 0, 'deltaY': int(dy),
             'duration': 0, 'origin': 'viewport'}]}]})
    b.sleep(0.4)


def raw_of(steps, band, param, words):
    """The raw step whose words are `words`, from the screen's tables."""
    rows = {'f': steps['bands'][band - 1]['f'], 'g': steps['gain'], 'q': steps['q']}[param]
    return [i for i, row in enumerate(rows) if row[1] == words]


def make(daemon, size=(1280, 1100)):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
    b.goto(daemon.url + '?effects=off', settle=1.0)
    b.ev(HOOK)
    b.sleep(0.5)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


# ------------------------------------------------------------------------- the drags

DX = 40

dac = common.Daemon('gungnir2-v2')
b = make(dac)
try:
    s = state(b)
    check('the graph reads a band set in Flat with all five points',
          s['mode'] == 'flat' and len(s['marks']) == 5 and len(s['steps']['gain']) == 128,
          s['mode'])

    # a left drag in Flat: 40 px right (band 3's window ends 53 px away, so 60 px would be cut
    # short), 30 px up, band 3 (the page's own Flat -> EQ)
    x, y = s['marks']['3']
    before = s['cells'][2]
    gesture(b, x, y, [(x + DX, y - 30)])
    a = state(b)
    cx, cy = a['marks']['3']
    check('a left drag moves band 3: frequency and gain change',
          a['cells'][2][0] != before[0] and a['cells'][2][1] != before[1], [before, a['cells'][2]])
    check('  the point sits under the release (1.5 px)',
          abs(cx - (x + DX)) <= 1.5 and abs(cy - (y - 30)) <= 1.5, [cx - x - DX, cy - y + 30])
    words = (a['label'] or '').split(' · ')
    got = a['bands'][2]
    check('  the readings hold the steps the label showed',
          len(words) == 3 and got[0] in raw_of(a['steps'], 3, 'f', words[0])
          and got[1] in raw_of(a['steps'], 3, 'g', words[1])
          and got[2] in raw_of(a['steps'], 3, 'q', words[2]), [words, got])
    check('  the cells show what the label showed', a['cells'][2] == words, [a['cells'][2], words])
    check('  the others stayed', a['bands'][0] == s['bands'][0] and a['bands'][3] == s['bands'][3])
    check('  in Flat the drag switched the mode to EQ, with one toast',
          a['mode'] == 'eq' and a['toasts'].count('Mode set to EQ') == 1, [a['mode'], a['toasts']])
    check('  it wrote steps (raw), at most one frequency and one gain at a time',
          a['posts'] and all('raw' in p[1] and 'step' not in p[1] for p in a['posts'])
          and all(p[1]['param'] in ('frequency', 'gain') for p in a['posts']), a['posts'])
    check('  the label is gone with the drag and the point is not left grabbed',
          a['drag'] == 0, a['drag'])
    check('  the band picked in the table is band 3',
          b.ev("document.querySelector('#part-bands .band[aria-pressed=\"true\"]')"
               "?.dataset.band") == '3')

    # a drag straight across leaves the gain alone
    s = state(b)
    x, y = s['marks']['2']
    before = s['cells'][1]
    n = len(s['posts'])
    gesture(b, x, y, [(x + 40, y + 3), (x + 80, y - 4)])
    a = state(b)
    check('a drag straight across changes the frequency and leaves the gain cell',
          a['cells'][1][0] != before[0] and a['cells'][1][1] == before[1], [before, a['cells'][1]])
    check('  and posts no gain write',
          all(p[1]['param'] == 'frequency' for p in a['posts'][n:]), a['posts'][n:])

    # a right drag 120 px right roughly halves Q
    s = state(b)
    x, y = s['marks']['3']
    q0 = s['steps']['q'][s['bands'][2][2]][0]
    gesture(b, x, y, [(x + 120, y)], button='right')
    a = state(b)
    q1 = a['steps']['q'][a['bands'][2][2]][0]
    check('a right drag 120 px right roughly halves the Q', abs(math.log2(q1 / q0) + 1) < 0.05,
          [q0, q1, a['bands'][2]])
    check('  the frequency and gain stayed', a['bands'][2][:2] == s['bands'][2][:2],
          [s['bands'][2], a['bands'][2]])
    check('  a right press wrote only Q', all(p[1]['param'] == 'q' for p in a['posts'][-4:]),
          a['posts'][-4:])

    # the wheel: two notches up over a point raise Q by 2; away from every point it is the page's
    s = state(b)
    x, y = s['marks']['4']
    q0 = s['bands'][3][2]
    wheel(b, x, y, -40)
    wheel(b, x, y, -40)
    b.sleep(0.8)
    a = state(b)
    check('two wheel notches up over a point raise its Q raw by 2',
          a['bands'][3][2] == q0 + 2, [q0, a['bands'][3], a['wheel']])
    check('  the wheel over the point was the page\'s own (prevented)',
          all(w['prevented'] for w in a['wheel'][-2:]), a['wheel'][-2:])
    # a mouse's notch is a pixel delta of 53 to 120, not 40: one event of -100 px is one step
    wheel(b, x, y, -100)
    b.sleep(0.8)
    d = state(b)
    check('  one wheel event of -100 px raises its Q raw by exactly 1',
          d['bands'][3][2] == q0 + 3, [q0 + 2, d['bands'][3], d['wheel'][-1:]])
    n = len(d['wheel'])
    px, py, pw, ph = d['plot']
    wheel(b, px + pw * 0.5, py + ph * 0.92, 100)
    c = state(b)
    check('the wheel away from every point is not prevented',
          len(c['wheel']) > n and not c['wheel'][-1]['prevented'], c['wheel'][n:])
    if c['scrollable']:
        check('  and scrolls the page', c['scrollY'] > d['scrollY'], [d['scrollY'], c['scrollY']])

    # (a notch down widens the bell by a step and would scroll the page down)
    # a wheel burst owns the wheel: the pointer drifts off the point between notches and the burst
    # still steps its band and the page does not scroll; a wheel 18 px from a point starts one
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)
    s = state(b)
    px, py, pw, ph = s['plot']
    x, y = s['marks']['2']
    q0 = s['bands'][1][2]
    towards = (1 if x < px + pw / 2 else -1, 1 if y < py + ph / 2 else -1)
    n = len(s['wheel'])
    wheel(b, x, y, 100)
    wheel(b, x + towards[0] * 13, y + towards[1] * 13, 100)
    wheel(b, x + towards[0] * 5, y + towards[1] * 28, 100)
    wheel(b, x + towards[0] * 22, y + towards[1] * 8, 100)
    b.sleep(0.8)
    a = state(b)
    check('a wheel burst whose pointer drifts 18 to 29 px off the point keeps stepping its Q',
          a['bands'][1][2] == q0 - 4, [q0, a['bands'][1], a['wheel'][n:]])
    check('  every notch of it was the page\'s own (prevented) and the page stayed put',
          len(a['wheel']) - n == 4 and all(w['prevented'] for w in a['wheel'][n:])
          and s['scrollable'] and a['scrollY'] == s['scrollY'],
          [a['wheel'][n:], s['scrollable'], s['scrollY'], a['scrollY']])
    check('  and no other band was stepped',
          [a['bands'][i] for i in (0, 2, 3, 4)] == [s['bands'][i] for i in (0, 2, 3, 4)],
          [a['bands'], s['bands']])

    # after 700 ms of quiet the wheel over empty graph is the page's again
    b.sleep(0.5)
    n = len(a['wheel'])
    wheel(b, x + towards[0] * 22, y + towards[1] * 8, 100)
    c = state(b)
    check('after a quiet spell the wheel 25 px from the point is not prevented and steps nothing',
          len(c['wheel']) > n and not c['wheel'][-1]['prevented']
          and c['bands'][1] == a['bands'][1] and c['scrollY'] > a['scrollY'],
          [c['wheel'][n:], c['bands'][1], a['scrollY'], c['scrollY']])
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)

    # the burst of a band keeps it when the pointer drifts into reach of ANOTHER band's point: a
    # notch there (synthetic, at band 3's point) steps band 2, not band 3
    s = state(b)
    x, y = s['marks']['2']
    ox, oy = s['marks']['3']
    q0 = s['bands'][1][2]
    wheel(b, x, y, 100)
    kept = b.ev("""(() => {
      const plot = document.querySelector('#part-tone .plot');
      return !plot.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true,
        clientX: %f, clientY: %f, deltaY: 100, deltaMode: 0 })); })()""" % (ox, oy))
    b.sleep(0.9)
    a = state(b)
    check('a burst near another band\'s point keeps stepping its own band',
          kept and a['bands'][1][2] == q0 - 2 and a['bands'][2] == s['bands'][2],
          [kept, q0, a['bands'][1], s['bands'][2], a['bands'][2]])

    # only a notch that changes the Q holds the page: a stream of small pixel deltas (a trackpad's)
    # that never makes a step, and a Q at its limit, let the page go 700 ms after the last step
    b.sleep(0.8)
    s = state(b)
    x, y = s['marks']['2']
    off = (x + towards[0] * 25, y)
    held = b.ev("""(async () => {
      const fire = (cx, cy, dy) => !document.querySelector('#part-tone .plot').dispatchEvent(
        new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy,
          deltaY: dy, deltaMode: 0 }));
      const wait = (ms) => new Promise((r) => setTimeout(r, ms));
      const out = { step: fire(%f, %f, 100) };
      const t0 = performance.now();
      out.small = [];
      while (performance.now() - t0 < 1500) {
        out.small.push([Math.round(performance.now() - t0), fire(%f, %f, 0.5)]);
        await wait(16);
      }
      return out; })()""" % (x, y, off[0], off[1]))
    early = [p for t_, p in held['small'] if t_ < 500]
    late = [p for t_, p in held['small'] if t_ > 900]
    check('small deltas that never make a step hold the page for 700 ms, and no longer',
          held['step'] and early and all(early) and late and not any(late),
          [held['step'], early[:3], late[:3], len(held['small'])])
    b.sleep(1.0)
    urllib.request.urlopen(urllib.request.Request(
        f'{dac.url}api/v1/devices/Gungnir%202/eq/bands/2',
        data=json.dumps({'param': 'q', 'raw': 127}).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': dac.url.rstrip('/')}),
        timeout=10).read()
    b.sleep(1.5)
    s = state(b)
    x, y = s['marks']['2']
    limit = b.ev("""(async () => {
      const fire = (cx, cy, dy) => !document.querySelector('#part-tone .plot').dispatchEvent(
        new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: cx, clientY: cy,
          deltaY: dy, deltaMode: 0 }));
      const on = fire(%f, %f, -100);
      await new Promise((r) => setTimeout(r, 50));
      return { on, off: fire(%f, %f, -100) }; })()""" % (x, y, x + towards[0] * 25, y))
    check('a Q at its limit does not hold the page: the notch on the point is prevented, a '
          'later one 25 px off is not',
          s['bands'][1][2] == 127 and limit['on'] and not limit['off'], [s['bands'][1], limit])
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)

    # the wheel starts a burst from 18 px away (the drag's 12 px reach is too tight for it)
    s = state(b)
    x, y = s['marks']['2']
    q0 = s['bands'][1][2]
    n = len(s['wheel'])
    wheel(b, x + towards[0] * 18, y, 100)
    b.sleep(0.8)
    a = state(b)
    check('a wheel 18 px from a point steps that band\'s Q and is prevented',
          a['bands'][1][2] == q0 - 1 and a['wheel'][-1]['prevented'] and len(a['wheel']) == n + 1,
          [q0, a['bands'][1], a['wheel'][n:]])
    # ... and not from 30 px away
    b.sleep(0.5)
    n = len(a['wheel'])
    wheel(b, x + towards[0] * 30, y, 100)
    c = state(b)
    check('a wheel 30 px from a point starts nothing and is the page\'s own',
          len(c['wheel']) > n and not c['wheel'][-1]['prevented']
          and c['bands'][1] == a['bands'][1] and c['scrollY'] > a['scrollY'],
          [c['wheel'][n:], c['bands'][1], a['scrollY'], c['scrollY']])
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)

    # the pointer leaving the plot ends the burst: a notch off the point right after it scrolls
    s = state(b)
    x, y = s['marks']['2']
    q0 = s['bands'][1][2]
    wheel(b, x, y, 100)
    b.ev("""document.body.dispatchEvent(new PointerEvent('pointermove', { bubbles: true,
      pointerId: 1, pointerType: 'mouse', clientX: 2, clientY: 2 }))""")
    n = len(b.ev('window.__wheel'))
    wheel(b, x + towards[0] * 22, y, 100)
    b.sleep(0.8)
    a = state(b)
    check('a burst ends when the pointer leaves the plot: the next notch off the point scrolls',
          len(a['wheel']) > n and not a['wheel'][-1]['prevented'] and a['bands'][1][2] == q0 - 1
          and a['scrollY'] > 0, [q0, a['bands'][1], a['wheel'][n:], a['scrollY']])
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)

    # a press ends the burst too
    s = state(b)
    x, y = s['marks']['2']
    q0 = s['bands'][1][2]
    wheel(b, x, y, 100)
    b.ev("""(() => { const plot = document.querySelector('#part-tone .plot');
      const p = plot.getBoundingClientRect();
      const at = { bubbles: true, pointerId: 9, pointerType: 'mouse', button: 0,
        clientX: p.left + p.width * 0.5, clientY: p.top + p.height * 0.9 };
      plot.dispatchEvent(new PointerEvent('pointerdown', { ...at, buttons: 1 }));
      plot.dispatchEvent(new PointerEvent('pointerup', { ...at, buttons: 0 })); })()""")
    n = len(b.ev('window.__wheel'))
    wheel(b, x + towards[0] * 22, y, 100)
    b.sleep(0.8)
    a = state(b)
    check('a burst ends on a press: the next notch off the point is the page\'s',
          len(a['wheel']) > n and not a['wheel'][-1]['prevented'] and a['bands'][1][2] == q0 - 1
          and a['scrollY'] > 0, [q0, a['bands'][1], a['wheel'][n:], a['scrollY']])
    b.ev('window.scrollTo(0, 0)')
    b.sleep(1.0)

    # a press within 100 ms of a tone swap, while the swap's curve is still easing, stands: the
    # drawn line and its data-y agree after the tween's own timer has run
    b.ev("""(() => { window.__synth = null;
      document.body.addEventListener('htmx:oobAfterSwap', (event) => {
        if (event.detail.target?.id !== 'part-tone' || window.__synth) return;
        window.__synth = 'armed';
        setTimeout(() => {
          const f = document.querySelector('#part-tone .eqg');
          const plot = f.querySelector('.plot');
          const p = plot.getBoundingClientRect(); const m = f.querySelector('.mk[data-band="3"]');
          const x = p.left + m.getAttribute('cx') / 1000 * p.width;
          const y = p.top + m.getAttribute('cy') / 260 * p.height;
          const fire = (type, dy, buttons) => plot.dispatchEvent(new PointerEvent(type, {
            bubbles: true, pointerId: 7, pointerType: 'mouse', button: 0, buttons,
            clientX: x, clientY: y + dy }));
          fire('pointerdown', 0, 1); fire('pointermove', -30, 1); window.__synth = 'down';
        }, 40);
      }); })()""")
    urllib.request.urlopen(urllib.request.Request(
        f'{dac.url}api/v1/devices/Gungnir%202/eq/bands/4',
        data=json.dumps({'param': 'gain', 'raw': 120}).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': dac.url.rstrip('/')}),
        timeout=10).read()
    b.sleep(0.9)
    eased = b.ev("""(() => { const line = document.querySelector('#part-tone .line');
      const path = line.getAttribute('d');
      const drawn = [...path.matchAll(/[ML][\\d.]+ ([\\d.]+)/g)].map((m) => +m[1]);
      const kept = line.dataset.y.split(',').map(Number);
      return { synth: window.__synth, label: document.querySelectorAll('.dlab').length,
        worst: Math.max(...drawn.map((v, i) => Math.abs(v - kept[i]))),
        n: [drawn.length, kept.length] };
      })()""")
    check('a press within 100 ms of a tone swap is not overwritten by the swap\'s easing',
          eased['synth'] == 'down' and eased['label'] == 1 and eased['n'][0] == eased['n'][1]
          and eased['worst'] <= 0.06, eased)
    b.ev("""(() => { const plot = document.querySelector('#part-tone .plot');
      plot.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 7,
        pointerType: 'mouse', button: 0, buttons: 0, clientX: 0, clientY: 0 })); })()""")
    b.sleep(0.6)
    check('no page error', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ----------------------------------------------------------------------------- a guest
guest = common.Daemon('gungnir2-v2', web='guest = "read"')
b = make(guest)
try:
    s = state(b)
    x, y = s['marks']['3']
    gesture(b, x, y, [(x + DX, y - 30)])
    a = state(b)
    check('a guest page grabs nothing and posts nothing',
          a['posts'] == [] and a['drag'] == 0 and a['grab'] == 0
          and a['bands'] == s['bands'], [a['posts'], a['grab']])
    check('  no page error', errors(b) == [], errors(b))
finally:
    b.close()
    guest.stop()

# ------------------------------------------------- held swap and a device that goes away
if browser == 'chromium':
    dac = common.Daemon('gungnir2-v2')
    b = make(dac)
    try:
        # the page is put in EQ first, so the held drag is an ordinary one
        urllib.request.urlopen(urllib.request.Request(
            f'{dac.url}api/v1/devices/Gungnir%202/eq/mode',
            data=json.dumps({'mode': 'eq'}).encode(), method='POST',
            headers={'Content-Type': 'application/json',
                     'Origin': dac.url.rstrip('/')}), timeout=10).read()
        b.sleep(1.0)
        s = state(b)
        x, y = s['marks']['3']
        b.ev("""(() => { window.__fig = document.querySelector('#part-tone .eqg');
          window.__mk4 = document.querySelector('.mk[data-band="4"]'); })()""")
        press(b, x, y)
        for i in range(1, 8):
            move(b, x + 3 * i, y - 2 * i)
        written = urllib.request.urlopen(urllib.request.Request(
            f'{dac.url}api/v1/devices/Gungnir%202/eq/bands/4',
            data=json.dumps({'param': 'gain', 'raw': 110}).encode(), method='POST',
            headers={'Content-Type': 'application/json', 'Origin': dac.url.rstrip('/')}),
            timeout=10)
        said = json.load(written)
        # and a write of the dragged band itself, to a parameter the drag does not send
        same = json.load(urllib.request.urlopen(urllib.request.Request(
            f'{dac.url}api/v1/devices/Gungnir%202/eq/bands/3',
            data=json.dumps({'param': 'q', 'raw': 100}).encode(), method='POST',
            headers={'Content-Type': 'application/json', 'Origin': dac.url.rstrip('/')}),
            timeout=10))
        b.sleep(1.5)
        held = b.ev("""({ fig: window.__fig.isConnected, mk: window.__mk4.isConnected,
          same: window.__fig === document.querySelector('#part-tone .eqg'),
          cell: document.querySelectorAll('#part-bands .band')[3].children[2].textContent,
          drag: document.querySelectorAll('.dlab').length })""")
        check('during a held drag a write from outside does not swap the graph',
              held['fig'] and held['mk'] and held['same'] and held['drag'] == 1, held)
        # the drag goes on after the swap that did not come
        move(b, x + 30, y - 20)
        release(b, x + 30, y - 20)
        b.sleep(1.5)
        a = state(b)
        check('  after the release the band-4 cell shows the written value',
              a['cells'][3][1] == said['text'], [a['cells'][3], said])
        check('  and the drag landed under the release',
              abs(a['marks']['3'][0] - (x + 30)) <= 1.5
              and abs(a['marks']['3'][1] - (y - 20)) <= 1.5,
              [a['marks']['3'], x + 30, y - 20])
        check('  after the release the band-3 cells show the device\'s readings',
              a['bands'][2][2] == 100 and a['cells'][2][2] == same['text']
              and a['cells'][2][0] == a['steps']['bands'][2]['f'][a['bands'][2][0]][1]
              and a['cells'][2][1] == a['steps']['gain'][a['bands'][2][1]][1],
              [a['bands'][2], a['cells'][2], same])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()

    # ---------------------------------------------- a drag let go of, its writes still held
    GATE = """(() => {
      window.__hold = false; window.__gated = [];
      const f = window.fetch;
      window.__refuse = false;
      window.fetch = (...a) => {
        const band = /eq\\/bands/.test(String(a[0]));
        if (window.__refuse && band && /"param":"q"/.test(a[1].body)) {
          return Promise.resolve(new Response(JSON.stringify({ error: 'Refused: test' }),
            { status: 409, headers: { 'Content-Type': 'application/json' } }));
        }
        return window.__hold && band
          ? new Promise((res) => window.__gated.push(() => res(f(...a)))) : f(...a);
      };
      window.__open = () => {
        window.__hold = false;
        window.__gated.splice(0).forEach((g) => g());
      };
      window.__readings = [];
      new MutationObserver((list) => { for (const m of list) {
        try { window.__readings.push(JSON.parse(m.target.dataset.readings).bands); } catch {}
      } }).observe(document.body, { subtree: true, attributes: true,
        attributeFilter: ['data-readings'] });
    })()"""
    TRUTH = ("fetch('api/v1/devices/Gungnir%202/model').then((r) => r.json())"
             ".then((j) => j.device.tone.readings.bands)")
    PAGE_READINGS = "JSON.parse(document.querySelector('#part-tone .eqg').dataset.readings).bands"
    dac = common.Daemon('gungnir2-v2')
    b = make(dac)
    try:
        urllib.request.urlopen(urllib.request.Request(
            f'{dac.url}api/v1/devices/Gungnir%202/eq/mode',
            data=json.dumps({'mode': 'eq'}).encode(), method='POST',
            headers={'Content-Type': 'application/json',
                     'Origin': dac.url.rstrip('/')}), timeout=10).read()
        b.sleep(1.0)
        b.ev(GATE)

        # a wheel notch over the band just let go of is the readings' Q: the finish does not put
        # the drag's older Q back
        s = state(b)
        x, y = s['marks']['3']
        q0 = s['bands'][2][2]
        b.ev('window.__hold = true')
        press(b, x, y)
        for i in range(1, 9):
            move(b, x + 4 * i, y - 3 * i)
        release(b, x + 32, y - 24)
        b.sleep(0.3)
        gated = b.ev('window.__gated.length')
        a = state(b)
        mx, my = a['marks']['3']
        wheel(b, mx, my, -100)
        b.ev('window.__open()')
        b.sleep(2.0)
        qs = [r[2][2] for r in b.ev('window.__readings')]
        truth = b.ev(TRUTH)
        check('a drag let go of with its writes held has them waiting', gated >= 1, gated)
        check('a wheel notch after the release steps from the drag\'s Q and the finish keeps it',
              qs != [] and all(q == q0 + 1 for q in qs) and truth[2][2] == q0 + 1,
              [q0, qs, truth[2]])
        check('  the page and the device agree on the band', b.ev(PAGE_READINGS)[2] == truth[2],
              [b.ev(PAGE_READINGS)[2], truth[2]])

        # a second press right after a release, the writes held, starts a drag
        b.ev('window.__readings.length = 0; window.__hold = true')
        s = state(b)
        x, y = s['marks']['3']
        press(b, x, y)
        for i in range(1, 9):
            move(b, x + 3 * i, y + 2 * i)
        release(b, x + 24, y + 16)
        b.sleep(0.3)
        a = state(b)
        mx, my = a['marks']['3']
        press(b, mx, my)
        for i in range(1, 7):
            move(b, mx - 5 * i, my - 3 * i)
        mid = seen(b)['drag']
        release(b, mx - 30, my - 18)
        b.sleep(0.3)
        held = b.ev('window.__gated.length')
        b.ev('window.__open()')
        b.sleep(2.0)
        a = state(b)
        truth = b.ev(TRUTH)
        check('a press right after a release, the writes held, starts a drag', mid == 1, mid)
        check('  the second drag\'s writes wait behind the first\'s too', held >= 2, held)
        check('  the point ends under the second release (1.5 px)',
              abs(a['marks']['3'][0] - (mx - 30)) <= 1.5
              and abs(a['marks']['3'][1] - (my - 18)) <= 1.5, [a['marks']['3'], mx - 30, my - 18])
        check('  the page and the device agree on the band', a['bands'][2] == truth[2],
              [a['bands'][2], truth[2]])
        check('  no label is left', a['drag'] == 0, a['drag'])

        # a second drag's write on a parameter that has a waiting value in the first goes after
        # it: the device ends on the second drag's value, not the first's stale one
        b.sleep(1.0)
        b.ev('window.__posts.length = 0; window.__hold = true')
        s = state(b)
        x, y = s['marks']['3']
        press(b, x, y)
        for i in range(1, 9):
            move(b, x - 3 * i, y)
        release(b, x - 24, y)
        b.sleep(0.3)
        a = state(b)
        mx, my = a['marks']['3']
        press(b, mx, my)
        move(b, mx + 16, my)
        words = (b.ev('window.__label') or '').split(' · ')
        release(b, mx + 16, my)
        b.sleep(0.3)
        b.ev('window.__open()')
        b.sleep(2.0)
        a = state(b)
        truth = b.ev(TRUTH)
        fposts = [p[1]['raw'] for p in a['posts']
                  if p[0].endswith('/bands/3') and p[1]['param'] == 'frequency']
        want = raw_of(a['steps'], 3, 'f', words[0])
        check('a second drag\'s single write goes after the first drag\'s waiting one',
              len(fposts) >= 3 and fposts[-1] in want and truth[2][0] in want,
              [fposts, want, truth[2]])

        # a drag let go of, then a press and release that moves nothing: the first drag's finish
        # still puts its values in the readings
        b.sleep(1.0)
        b.ev('window.__readings.length = 0; window.__hold = true')
        s = state(b)
        x, y = s['marks']['3']
        press(b, x, y)
        for i in range(1, 7):
            move(b, x + 3 * i, y)
        release(b, x + 18, y)
        b.sleep(0.3)
        a = state(b)
        mx, my = a['marks']['3']
        press(b, mx, my)
        release(b, mx, my)
        b.sleep(0.3)
        b.ev('window.__open()')
        b.sleep(2.0)
        recorded = b.ev('window.__readings')
        truth = b.ev(TRUTH)
        check('a press that moves nothing after a drag does not lose the drag\'s readings',
              recorded != [] and recorded[-1][2][:2] == truth[2][:2], [recorded[-1:], truth[2]])

        # a wheel notch the device refuses: the part is read again and shows the device's Q
        b.sleep(1.0)
        b.ev('window.__readings.length = 0; window.__hold = true; window.__refuse = false')
        # the reads of the model from the refusal's toast on (the core reads with fetch)
        b.ev("""(() => { window.__xhr = []; window.__reset = false;
          const fetched = window.fetch;
          window.fetch = function (u, ...r) {
            window.__xhr.push(String(u)); return fetched.call(this, u, ...r); };
          new MutationObserver(() => {
            const shown = [...document.querySelectorAll('.toast')]
              .some((t) => t.textContent === 'Refused: test');
            if (shown && !window.__reset) { window.__reset = true; window.__xhr.length = 0; }
          }).observe(document.body, { subtree: true, childList: true }); })()""")
        s = state(b)
        x, y = s['marks']['3']
        q0 = b.ev(TRUTH)[2][2]
        press(b, x, y)
        for i in range(1, 6):
            move(b, x - 3 * i, y)
        release(b, x - 15, y)
        b.sleep(0.3)
        a = state(b)
        mx, my = a['marks']['3']
        wheel(b, mx, my, -100)
        b.ev('window.__refuse = true')
        b.ev('window.__open()')
        b.sleep(2.5)
        b.ev('window.__refuse = false')
        a = state(b)
        truth = b.ev(TRUTH)
        reads = [u for u in b.ev('window.__xhr') if u.endswith('/model')]
        check('a wheel notch the device refused toasts and the part shows the device\'s Q',
              'Refused: test' in a['toasts'] and truth[2][2] == q0 and a['bands'][2] == truth[2],
              [a['toasts'], q0, a['bands'][2], truth[2]])
        check('  and the model is read again after the refusal', len(reads) >= 1, reads)

        # a wheel burst goes on from the readings when they carry a Q the burst does not know
        # (a stepper, another front end) inside its window: a swap's readings are put in by the
        # page here, the stream's echo being slower than the burst's 700 ms
        b.sleep(1.0)
        got = b.ev("""(async () => {
          const f = () => document.querySelector('#part-tone .eqg');
          const plot = f().querySelector('.plot'); const p = plot.getBoundingClientRect();
          const m = f().querySelector('.mk[data-band="4"]');
          const x = p.left + m.getAttribute('cx') / 1000 * p.width;
          const y = p.top + m.getAttribute('cy') / 260 * p.height;
          const readings = () => JSON.parse(f().dataset.readings);
          const notch = () => plot.dispatchEvent(new WheelEvent('wheel', {
            bubbles: true, cancelable: true, clientX: x, clientY: y, deltaY: -100,
            deltaMode: 0 }));
          const first = window.__posts.length;
          const q0 = readings().bands[3][2];
          notch();
          const other = readings();
          other.bands[3][2] = q0 + 10;
          f().dataset.readings = JSON.stringify(other);
          notch();
          await new Promise((r) => setTimeout(r, 1000));
          const raws = window.__posts.slice(first).filter((q) => /eq\\/bands\\/4/.test(q[0]))
            .map((q) => JSON.parse(q[1])).filter((q) => q.param === 'q').map((q) => q.raw);
          return { q0, raws };
        })()""")
        check('a notch in a burst steps from a Q the readings carry that the burst does not know',
              got['raws'][-1:] == [got['q0'] + 11], got)

        # and from the Q a right drag set inside the window
        b.sleep(1.0)
        got = b.ev("""(async () => {
          const at = () => {
            const f = document.querySelector('#part-tone .eqg');
            const plot = f.querySelector('.plot'); const p = plot.getBoundingClientRect();
            const m = f.querySelector('.mk[data-band="4"]');
            return [plot, p.left + m.getAttribute('cx') / 1000 * p.width,
                    p.top + m.getAttribute('cy') / 260 * p.height];
          };
          const first = window.__posts.length;
          const [plot, x, y] = at();
          const notch = () => plot.dispatchEvent(new WheelEvent('wheel', {
            bubbles: true, cancelable: true, clientX: x, clientY: y, deltaY: -100,
            deltaMode: 0 }));
          const fire = (type, dx, buttons) => plot.dispatchEvent(new PointerEvent(type, {
            bubbles: true, pointerId: 8, pointerType: 'mouse', button: 2, buttons,
            clientX: x + dx, clientY: y }));
          notch();
          fire('pointerdown', 0, 2); fire('pointermove', 60, 2); fire('pointermove', 120, 2);
          fire('pointerup', 120, 0);
          notch();
          await new Promise((r) => setTimeout(r, 1500));
          return window.__posts.slice(first).filter((p) => /eq\\/bands\\/4/.test(p[0]))
            .map((p) => JSON.parse(p[1])).filter((p) => p.param === 'q').map((p) => p.raw);
        })()""")
        truth = b.ev(TRUTH)
        check('a notch after a right drag inside the burst\'s window steps from the drag\'s Q',
              len(got) >= 3 and got[-1] == got[-2] + 1 and got[-2] < got[0] and
              truth[3][2] == got[-1], [got, truth[3]])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        dac.stop()

    gone = common.Daemon('gungnir2-v2')
    b = make(gone)
    try:
        s = state(b)
        x, y = s['marks']['3']
        link = b.ev("""(async () => (await (await fetch('api/v1/devices/Gungnir%202/model')).json())
          .device.link.state)()""")
        check('a device that goes away mid-drag: the device is connected when the press starts',
              link == 'on', link)
        press(b, x, y)
        refused_at = None
        # the path keeps clear of the 0 dB zone (band 3 starts at 0 dB) so the gain changes on
        # nearly every move: a write has to reach the gone device before the page hears it went.
        # The device is dropped on the check's own cue, after the drag has begun and written, and
        # after the page's stream is closed: the page cannot hear that the link went (it would
        # then dim and end the drag with no write refused), so the next write is the one that
        # finds out.
        began, i, dropped = time.time(), 0, False
        while time.time() - began < 20:
            i += 1
            move(b, x + 40 * math.sin(i / 6), y - 25 * abs(math.sin(i / 9)) - 14)
            if not dropped and i == 30:
                check('  the drag has written to the device before it is dropped',
                      seen(b)['posts'] > 0, seen(b))
                b.ev("""import(new URL('assets/core/boot.js', document.baseURI))
                  .then((m) => m.core.stream.close())""")
                gone.drop()
                dropped = True
            if dropped and i % 10 == 0:
                if b.ev("window.__toasts.some((t) => t !== 'Mode set to EQ')"):
                    refused_at = b.ev("performance.now()")
                    break
        b.sleep(0.5)
        posts_then = seen(b)['posts']
        move(b, x + 60, y - 40)
        release(b, x + 60, y - 40)
        b.sleep(1.5)
        a = seen(b)
        check('a device that goes away mid-drag toasts', refused_at is not None, a['toasts'])
        words = [t for t in a['toasts'] if t != 'Mode set to EQ']
        check('  once', len(words) == 1, a['toasts'])
        check('  and the drag posts nothing after it, and no label is left',
              a['posts'] == posts_then and a['drag'] == 0, [posts_then, a])
        # the tone part is read back from the server after the refusal: it shows the device's
        # readings, not the place the drag had drawn
        mine = b.ev("""(() => { const f = document.querySelector('#part-tone .eqg');
          return { readings: f.dataset.readings, drag: document.querySelectorAll('.dlab').length,
            cells: [...document.querySelectorAll('#part-bands .band')]
              .map((row) => [...row.children].slice(1).map((c) => c.textContent)) }; })()""")
        theirs = b.ev("""(async () => {
          const j = await (await fetch('api/v1/devices/Gungnir%202/model')).json();
          const t = j.device.tone;
          return { readings: t && JSON.stringify(t.readings),
            cells: t.bands.rows.map((row) => [row.f, row.g, row.q]) }; })()""")
        check('  and the tone part shows the device\'s reading again after the refusal',
              theirs['readings'] is not None and mine['readings'] == theirs['readings']
              and mine['cells'] == theirs['cells'], [mine, theirs])
        # the browser logs each refused write (409) itself: those are the only errors
        other = [e for e in errors(b) if 'status of 409' not in e]
        check('  no page error but the refused writes\' own 409s', other == [], other)
    finally:
        b.close()
        gone.stop()

print(f'tone_drag.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
