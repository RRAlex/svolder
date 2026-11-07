# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Ember's computer side in a real page (design: the computer flip), through the page's own
controls, on a fake host (`SVOLDER_HOST=fake`: its sink and EQ live in memory, no PipeWire, no sound):
  the_pill_flips_volume_and_tone_together   the switch beside the device's name flips the knob and
                                        the tone control, writes nothing, and flips back
  minus_plus_mute_set_the_computer_volume   the buttons step the volume 1 %, a held one repeats,
                                        mute posts and keeps the number
  a_point_drag_posts_live_and_done_at_release   a drag posts `done: false` while it moves (at most
                                        20 a second) and one `done: true` at the end; the dragged
                                        point's row is lit and scrolled into view
  a_tap_adds_a_point_and_x_removes_it   a tap on an empty spot posts a PK of Q 1, the row's x
                                        removes it, `+ Add point` adds one at the widest gap
  the_mode_the_type_a_typed_value_and_the_presets_write   Off | EQ, the type choice, a value typed
                                        into a cell, a preset recalled
  the_wheel_and_a_right_drag_set_the_q  a notch over a point steps its Q, a right drag halves it
  sixteen_points_keep_the_layout        the tone section is as tall with 16 points as with 7
  a_flip_mid_drag_writes_nothing        a flip ends the drag; nothing but that drag's own last
                                        value is written (Chromium: a button held across calls)
  a_guest_sees_the_computer_inert       a guest may flip and every computer control is inert
  the_side_survives_a_reload            the side kept in the browser is the one drawn after a load,
                                        and the load writes nothing
  the_same_preset_on_both_asks_and_yes_flattens_the_dac   the question card; no sends nothing, yes
                                        confirms and the DAC is flat
  the_amp_page_flips_to_the_computer_with_the_dac_parts   the fake amp's page has the switch and,
                                        flipped, the DAC page's knob and tone section
  a_finger_adds_drags_and_pinches       (Chromium, touch emulation) a tap adds, a finger drags a
                                        point, two fingers halve its Q
Pictures of the computer side (desktop and phone) are saved under target/web-shots/ember/ (Chromium run).
Usage: python3 computer_ember.py chromium|firefox"""
import json
import math
import os
import sys
import time
import urllib.request

import common

os.environ.setdefault('SVOLDER_FAKE_COMPUTER_VOLUME', '42')

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()
SHOTS = f'{common.REPO}/target/web-shots/ember'


def bare(body):
    """A point write's body without its `slot` (which names the point and is checked on its own)."""
    return {k: v for k, v in body.items() if k != 'slot'}


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


FLEET = [('fake-amp', 'Amp', '1112131415161718')]
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (!(a[1] && a[1].method === 'POST')) return f(...a);"
        " const entry = [String(a[0]), a[1].body, null, performance.now()]; window.__posts.push(entry);"
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
COMPUTER = '__core.model.page().host?.computer'
POINTS = f'{COMPUTER}.eq.points'
TONE = 'JSON.stringify(__core.computer.toneOf(__core.model.page()).points.map((p) => [p.type, p.f, p.g, p.q]))'
CARD = "document.querySelector('.ask')"


def make(size=(1280, 1100)):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
    b.preload(HOOK)
    return b


def errors(b):
    """The page's errors, but for the console's lines about the refusal the question check makes on purpose."""
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'status of 409' not in e]


def poll(read, secs=6.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, query='?theme=ember&effects=off'):
    b.goto(f'{d.url}{query}', settle=1.0)
    assert b.page(BOOT) == 'ember'
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    time.sleep(0.4)


def posts(b, route=None):
    log = b.page('window.__posts.map((p) => [p[0], JSON.parse(p[1] || "null"), p[2], p[3]])')
    return [p for p in log if route is None or p[0].startswith(route)]


def forget(b):
    b.page('window.__posts.length = 0')


def tap(b, selector):
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.15)
    x, y = common.center(b, selector)
    b.click(x, y)
    time.sleep(0.4)


def flip(b, side):
    tap(b, f'#part-switch .opt[data-side="{side}"]')
    poll(lambda: b.page(f"!!document.querySelector('.view[data-side=\"{side}\"]')") if side == 'computer'
         else b.page("!document.querySelector('.view[data-side=\"computer\"]')"))
    time.sleep(0.3)


def seed(b, count):
    """The EQ holds `count` points (a spread of peaks and the two shelves), through the actions."""
    plan = [('LSC', 105, 5.5, 0.7), ('PK', 180, -2.1, 0.9), ('PK', 1200, -1.8, 1.4),
            ('PK', 2900, 3.2, 2.0), ('PK', 5600, -4.0, 3.1), ('PK', 8900, 2.4, 2.5),
            ('HSC', 10000, -1.5, 0.7)]
    extra = [('PK', round(40 * 1.28 ** i), 1.0 + (i % 3), 1.0) for i in range(9)]
    have = b.page(f'{POINTS}.length')
    for kind, f, g, q in (plan + extra)[have - 1:count - 1]:
        b.page(f"__core.actions.computer.addPoint({{ type: {json.dumps(kind)}, f: {f}, g: {g}, q: {q} }}).then(() => 1)")
        time.sleep(0.15)
    poll(lambda: b.page(f'{POINTS}.length') >= count)
    time.sleep(0.5)
    forget(b)


STATE = """(() => {
  const part = document.querySelector('#part-tone');
  const plot = part.querySelector('.eqg .plot'); const p = plot.getBoundingClientRect();
  const marks = {};
  for (const m of plot.querySelectorAll('.mk')) {
    marks[m.dataset.n] = [p.left + m.getAttribute('cx') / 1000 * p.width, p.top + m.getAttribute('cy') / 260 * p.height];
  }
  const box = part.querySelector('.pscroll'); const frame = box.getBoundingClientRect();
  return { marks, plot: [p.left, p.top, p.width, p.height], height: part.getBoundingClientRect().height,
    rows: [...part.querySelectorAll('.pt')].map((r) => {
      const b = r.getBoundingClientRect();
      return { n: Number(r.dataset.n), on: r.classList.contains('on'), inside: b.top >= frame.top - 1 && b.bottom <= frame.bottom + 1,
        cells: [r.querySelector('select').value, ...[...r.querySelectorAll('.cell')].map((c) => c.textContent)] };
    }),
    pitch: part.querySelectorAll('.pt').length > 1 ? part.querySelectorAll('.pt')[1].getBoundingClientRect().top - part.querySelectorAll('.pt')[0].getBoundingClientRect().top : 0,
    gap: part.querySelector('.more').getBoundingClientRect().top - frame.bottom,
    more: part.querySelector('.more').textContent, scroll: box.scrollTop, label: document.querySelectorAll('.dlab').length,
    line: part.querySelector('.pline').textContent }; })()"""


def state(b):
    return b.page(STATE)


# ------------------------------------------------------------------------------ pointer input

def mouse_chromium(b, kind, x, y, button='left', buttons=0, **extra):
    b.send('Input.dispatchMouseEvent', {'type': kind, 'x': x, 'y': y, 'button': button,
                                        'buttons': buttons, 'clickCount': 1, **extra})


def press(b, x, y, button='left'):
    mouse_chromium(b, 'mouseMoved', x, y)
    mouse_chromium(b, 'mousePressed', x, y, button, 1 if button == 'left' else 2)


def move(b, x, y, button='left'):
    mouse_chromium(b, 'mouseMoved', x, y, button, 1 if button == 'left' else 2)
    b.sleep(0.03)


def release(b, x, y, button='left'):
    mouse_chromium(b, 'mouseReleased', x, y, button, 0)


def path_to(x, y, to, step=4):
    out = []
    px, py = x, y
    for tx, ty in to:
        n = max(1, int(math.hypot(tx - px, ty - py) // step))
        out += [(px + (tx - px) * i / n, py + (ty - py) * i / n) for i in range(1, n + 1)]
        px, py = tx, ty
    return out


def gesture(b, x, y, to, button='left', step=4):
    path = path_to(x, y, to, step)
    if browser == 'chromium':
        press(b, x, y, button)
        for mx, my in path:
            move(b, mx, my, button)
        release(b, *path[-1], button)
    else:
        code = 0 if button == 'left' else 2
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)},
                {'type': 'pointerDown', 'button': code}]
        for mx, my in path:
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(mx), 'y': int(my)})
        acts.append({'type': 'pointerUp', 'button': code})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'}, 'actions': acts}]})
    b.sleep(1.2)


def wheel(b, x, y, dy):
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': y, 'deltaX': 0, 'deltaY': dy})
    else:
        b._post('/actions', {'actions': [{'type': 'wheel', 'id': 'wheel', 'actions': [
            {'type': 'scroll', 'x': int(x), 'y': int(y), 'deltaX': 0, 'deltaY': int(dy),
             'duration': 0, 'origin': 'viewport'}]}]})
    b.sleep(0.5)


def hold(b, x, y, secs):
    if browser == 'chromium':
        press(b, x, y)
        b.sleep(secs)
        release(b, x, y)
    else:
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': [
            {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)},
            {'type': 'pointerDown', 'button': 0}, {'type': 'pause', 'duration': int(secs * 1000)},
            {'type': 'pointerUp', 'button': 0}]}]})
    b.sleep(0.6)


# ------------------------------------------------------------------------------------- the cases

def pill(b, d):
    load(b, d)
    check('the_pill_flips_volume_and_tone_together: the DAC side first, the pill beside the name',
          b.page("document.querySelector('#part-switch .opt[aria-pressed=true]').textContent") == 'Gungnir 2'
          and b.page("!!document.querySelector('#part-tone .opt[data-mode=flat]')")
          and b.page("!document.querySelector('sv-knob[data-computer]')")
          and b.page("document.querySelector('.devs #part-switch') !== null"))
    words = b.page("[...document.querySelectorAll('#part-switch .opt')].map((o) => o.textContent)")
    check('the_pill_flips_volume_and_tone_together: it reads the name and This computer',
          words == ['Gungnir 2', 'This computer'], words)
    flip(b, 'computer')
    volume = b.page("document.querySelector('sv-knob[data-computer] .big').textContent.trim()")
    pct = b.page(f'{COMPUTER}.volume.pct')
    check('the_pill_flips_volume_and_tone_together: the knob reads the computer, in %',
          volume.replace(' ', '') == f'{pct}%', [volume, pct])
    mode = b.page("[...document.querySelectorAll('#part-tone .seg .opt')].map((o) => o.textContent + o.getAttribute('aria-pressed'))")
    check('the_pill_flips_volume_and_tone_together: the tone control reads Off | EQ',
          mode == ['Offtrue', 'EQfalse'], mode)
    check('the_pill_flips_volume_and_tone_together: a flip posts nothing', posts(b) == [], posts(b))
    check('the_pill_flips_volume_and_tone_together: nothing below the switch says computer',
          b.page("/computer/i.test([...document.querySelectorAll('#part-volume, #part-tone, #part-presets')].map((e) => e.innerText).join(' '))") is False)
    flip(b, 'dac')
    check('the_pill_flips_volume_and_tone_together: and back to the DAC\'s own parts',
          b.page("!!document.querySelector('#part-tone .opt[data-mode=flat]') && !document.querySelector('sv-knob[data-computer]')"))
    check('the_pill_flips_volume_and_tone_together: no write on the way back', posts(b) == [], posts(b))


def volume(b, d):
    load(b, d)
    flip(b, 'computer')
    start = b.page(f'{COMPUTER}.volume.pct')
    tap(b, 'sv-knob[data-computer] .step[data-step="1"]')
    got = poll(lambda: [p for p in posts(b, 'api/v1/host/volume')])
    check('minus_plus_mute_set_the_computer_volume: + posts one step of 1 %',
          [p[1] for p in got] == [{'pct': start + 1}], got)
    shown = poll(lambda: b.page("document.querySelector('sv-knob[data-computer] .big .num').textContent") == str(start + 1))
    check('minus_plus_mute_set_the_computer_volume: and the knob shows it', shown is True)
    tap(b, 'sv-knob[data-computer] .step[data-step="-1"]')
    tap(b, 'sv-knob[data-computer] .step[data-step="-1"]')
    time.sleep(0.6)
    check('minus_plus_mute_set_the_computer_volume: two steps down read 2 % lower',
          b.page(f'{COMPUTER}.volume.pct') == start - 1, b.page(f'{COMPUTER}.volume'))
    forget(b)
    tap(b, 'sv-knob[data-computer] .mute')
    got = poll(lambda: posts(b, 'api/v1/host/mute'))
    check('minus_plus_mute_set_the_computer_volume: mute posts muted',
          [p[1] for p in got] == [{'muted': True}], got)
    time.sleep(0.5)
    check('minus_plus_mute_set_the_computer_volume: and keeps the number, saying muted',
          b.page("document.querySelector('sv-knob[data-computer] .big .num').textContent") == str(start - 1)
          and b.page("document.querySelector('sv-knob[data-computer] .sub').textContent") == 'muted')
    tap(b, 'sv-knob[data-computer] .mute')
    time.sleep(0.5)
    check('minus_plus_mute_set_the_computer_volume: unmute',
          b.page(f'{COMPUTER}.volume.muted') is False)
    forget(b)
    x, y = common.center(b, 'sv-knob[data-computer] .step[data-step="1"]')
    before = b.page(f'{COMPUTER}.volume.pct')
    hold(b, x, y, 1.3)
    after = b.page(f'{COMPUTER}.volume.pct')
    check('minus_plus_mute_set_the_computer_volume: a held + repeats, a step at a time up to the end',
          after >= before + 4 and after <= 100, [before, after])
    last = posts(b, 'api/v1/host/volume')
    check('minus_plus_mute_set_the_computer_volume: the last post is the level reached, never above 100',
          bool(last) and last[-1][1]['pct'] == after and all(p[1]['pct'] <= 100 for p in last), last[-3:])


def drag(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 7)
    s = state(b)
    check('a_point_drag_posts_live_and_done_at_release: seven points, the table shows six and says so',
          len(s['marks']) == 7 and len(s['rows']) == 7 and s['more'] == '1 more ↓ scroll'
          and s['rows'][6]['inside'] is False and s['rows'][0]['inside'] is True, s)
    check('a_point_drag_posts_live_and_done_at_release: rows are 42 px apart and the words clear the last row',
          abs(s['pitch'] - 42) <= 0.5 and s['gap'] >= 4, [s['pitch'], s['gap']])
    x, y = s['marks']['4']
    before = s['rows'][3]['cells']
    during = None
    if browser == 'chromium':
        press(b, x, y)
        for mx, my in path_to(x, y, [(x + 40, y - 30)]):
            move(b, mx, my)
        during = state(b)
        release(b, x + 40, y - 30)
        b.sleep(1.2)
    else:
        gesture(b, x, y, [(x + 40, y - 30)])
    a = state(b)
    log = posts(b, 'api/v1/host/eq/points/4')
    flags = [p[1].get('done') for p in log]
    check('a_point_drag_posts_live_and_done_at_release: live updates, then one done at the release',
          len(log) >= 2 and flags[-1] is True and all(f is False for f in flags[:-1]), flags)
    gaps = [round(y[3] - x[3]) for x, y in zip(log[:-2], log[1:-1])]
    check('a_point_drag_posts_live_and_done_at_release: at most 20 a second (the live ones 45 ms apart or more)',
          len(gaps) >= 1 and all(g >= 45 for g in gaps), gaps)
    held = [p[1] for p in log]
    check('a_point_drag_posts_live_and_done_at_release: the posts carry the point\'s frequency and gain',
          any('f' in p for p in held) and any('g' in p for p in held), held[:4])
    cx, cy = a['marks']['4']
    check('a_point_drag_posts_live_and_done_at_release: the point sits on the release\'s frequency (2 px)',
          abs(cx - (x + 40)) <= 2, [cx - x - 40])
    check('a_point_drag_posts_live_and_done_at_release: the row says what the model holds',
          a['rows'][3]['cells'][1] != before[1] and a['rows'][3]['cells'][2] != before[2]
          and a['rows'][3]['on'] is True, [before, a['rows'][3]])
    point = b.page(f'{POINTS}[3]')
    check('a_point_drag_posts_live_and_done_at_release: the daemon holds the last value sent',
          abs(point['g'] - held[-1].get('g', point['g'])) < 0.01 and point['f'] != 2900, [point, held[-1]])
    if during:
        check('a_point_drag_posts_live_and_done_at_release: while it moves a label shows and the row is lit',
              during['label'] == 1 and during['rows'][3]['on'] is True, during)
    # the last point's row is scrolled into view when it is dragged, and lit
    x, y = a['marks']['7']
    gesture(b, x, y, [(x - 20, y + 10)])
    z = state(b)
    check('a_point_drag_posts_live_and_done_at_release: the dragged point\'s row is lit and scrolled into view',
          z['rows'][6]['on'] is True and z['rows'][6]['inside'] is True and z['scroll'] > 0 and z['more'] == '',
          [z['rows'][6], z['scroll'], z['more']])
    # the table scrolls by itself: back to the top, the row says what is below again
    b.page("document.querySelector('.pscroll').scrollTop = 0")
    time.sleep(0.3)
    check('a_point_drag_posts_live_and_done_at_release: scrolled back, the words come back',
          state(b)['more'] == '1 more ↓ scroll')


def add_remove(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 3)
    s = state(b)
    px, py, pw, ph = s['plot']
    spot = None
    for fx in (0.12, 0.3, 0.45, 0.62, 0.78, 0.9):
        for fy in (0.12, 0.88, 0.25, 0.75):
            x, y = px + fx * pw, py + fy * ph
            if all(math.hypot(x - mx, y - my) > 40 for mx, my in s['marks'].values()):
                spot = (x, y)
                break
        if spot:
            break
    b.click(*spot)
    got = poll(lambda: posts(b, 'api/v1/host/eq/points'))
    add = got[0][1] if got else {}
    check('a_tap_adds_a_point_and_x_removes_it: a tap posts a PK of Q 1 where it was made',
          len(got) == 1 and add.get('type') == 'PK' and add.get('q') == 1 and 20 <= add.get('f', 0) <= 20000, got)
    poll(lambda: b.page(f'{POINTS}.length') == 4)
    time.sleep(0.5)
    s = state(b)
    # the points are listed by frequency: the new one is where its frequency puts it
    at = b.page(f"{POINTS}.findIndex((p) => Math.abs(p.f - {add['f']}) < 0.01) + 1")
    nx, ny = s['marks'][str(at)]
    check('a_tap_adds_a_point_and_x_removes_it: the new point is drawn at the tapped frequency (4 px)',
          abs(nx - spot[0]) <= 4, [nx - spot[0], ny - spot[1]])
    label = b.page(f"document.querySelector('#part-tone .pt[data-n=\"{at}\"] [data-comp=remove]').getAttribute('aria-label')")
    check('a_tap_adds_a_point_and_x_removes_it: its row has an x labelled Remove point %d' % at,
          label == 'Remove point %d' % at, label)
    forget(b)
    tap(b, f'#part-tone .pt[data-n="{at}"] [data-comp="remove"]')
    got = poll(lambda: posts(b, f'api/v1/host/eq/points/{at}/remove'))
    check('a_tap_adds_a_point_and_x_removes_it: the x removes it', len(got) == 1, got)
    check('a_tap_adds_a_point_and_x_removes_it: three points again',
          poll(lambda: b.page(f'{POINTS}.length') == 3) is True)
    forget(b)
    tap(b, '#part-tone [data-comp="add"]')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points'))
    freqs = sorted([20, 20000] + [p['f'] for p in b.page(POINTS)[:3]])
    gap = max(range(len(freqs) - 1), key=lambda i: math.log(freqs[i + 1] / freqs[i]))
    want = math.sqrt(freqs[gap] * freqs[gap + 1])
    check('a_tap_adds_a_point_and_x_removes_it: + Add point adds a PK, 0 dB, Q 1 at the widest gap',
          len(got) == 1 and got[0][1]['g'] == 0 and got[0][1]['q'] == 1 and abs(got[0][1]['f'] - want) < 0.5,
          [got, want])
    time.sleep(0.6)
    forget(b)
    # a lone point keeps its x, which sets it flat
    for n in (4, 3, 2):
        tap(b, f'#part-tone .pt[data-n="{n}"] [data-comp="remove"]')
        time.sleep(0.5)
    b.page("__core.actions.computer.setPoint(1, { g: 3 }, { done: true }).then(() => 1)")
    poll(lambda: b.page(f'{POINTS}[0].g') == 3)
    forget(b)
    tap(b, '#part-tone .pt[data-n="1"] [data-comp="remove"]')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/1/remove'))
    flat = poll(lambda: b.page(f'{POINTS}[0].g') == 0)
    check('a_tap_adds_a_point_and_x_removes_it: the lone point keeps its x, labelled Remove point 1, and it sets the point flat',
          len(got) == 1 and flat is True and b.page(f'{POINTS}.length') == 1
          and b.page("document.querySelector('#part-tone .pt[data-n=\"1\"] [data-comp=remove]').getAttribute('aria-label')") == 'Remove point 1',
          [got, b.page(POINTS)])


def sorted_points(b, d):
    name = 'the_points_are_listed_by_frequency_and_the_selection_follows'
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    fs = lambda: b.page(f'{POINTS}.map((p) => p.f)')
    start = fs()
    check(f'{name}: the daemon lists the points by frequency', start == sorted(start), start)
    tap(b, '#part-tone .pt[data-n="1"] .pn')
    check(f'{name}: point 1 is the row in hand', b.page("document.querySelector('#part-tone .pt[data-n=\"1\"]').classList.contains('on')") is True)
    # a drag never re-sorts: point 1 goes to the far end while the others keep their numbers
    b.page("__core.actions.computer.setPoint(1, { f: 19000 }, { done: false }).then(() => 1)")
    mid = poll(lambda: fs()[0] == 19000 and fs())
    check(f'{name}: mid-drag point 1 is still point 1 (nothing jumps under the finger)',
          mid and mid[1:] == start[1:], [start, mid])
    b.page("__core.actions.computer.setPoint(1, {}, { done: true }).then(() => 1)")
    want = sorted(start[1:] + [19000])
    check(f'{name}: at the release the table is in order', poll(lambda: fs() == want) is True, [start, fs()])
    last = len(start)
    time.sleep(0.6)
    on = b.page("[...document.querySelectorAll('#part-tone .pt.on')].map((r) => r.dataset.n)")
    check(f'{name}: and the row in hand is the one that moved', on == [str(last)], on)
    nums = b.page("[...document.querySelectorAll('#part-tone .pt .pn')].map((r) => r.textContent.trim())")
    check(f'{name}: the # column counts the sorted order', nums == [str(i + 1) for i in range(last)], nums)


def controls(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    tap(b, '#part-tone .opt[data-mode="off"]')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check('the_mode_the_type_a_typed_value_and_the_presets_write: Off posts off',
          modes == [{'on': False}] and b.page(f'{COMPUTER}.eq.on') is False and
          b.page("document.querySelector('#part-tone .opt[data-mode=off]').getAttribute('aria-pressed')") == 'true',
          modes)
    forget(b)
    tap(b, '#part-tone .opt[data-mode="eq"]')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check('the_mode_the_type_a_typed_value_and_the_presets_write: EQ posts on',
          modes == [{'on': True}] and b.page(f'{COMPUTER}.eq.on') is True, modes)
    forget(b)
    b.page("(() => { const s = document.querySelector('#part-tone .pt[data-n=\"2\"] select');"
           " s.value = 'HSC'; s.dispatchEvent(new Event('change', { bubbles: true })); })()")
    time.sleep(0.8)
    check('the_mode_the_type_a_typed_value_and_the_presets_write: the type choice writes the shelf',
          b.page(f'{POINTS}[1].type') == 'HSC' and
          b.page("document.querySelector('#part-tone .pt[data-n=\"2\"] select').value") == 'HSC',
          b.page(f'{POINTS}[1]'))
    forget(b)
    tap(b, '#part-tone .pt[data-n="3"] .cell[data-param="f"]')
    typed = b.page("(() => { const i = document.querySelector('#part-tone .pt[data-n=\"3\"] .cell[data-param=\"f\"] input');"
                   " return !!i && document.activeElement === i; })()")
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a tap on a value makes a field', typed is True)
    b.page("(() => { const i = document.querySelector('#part-tone .pt[data-n=\"3\"] .cell input'); i.value = '2.5k';"
           " i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()")
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/3'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: 2.5k is sent as 2500, done',
          bool(got) and bare(got[-1][1]) == {'f': 2500, 'done': True}, got)
    check('every_point_write_carries_its_slot: the typed value names its point by slot', bool(got) and isinstance(got[-1][1].get('slot'), int)
          and 1 <= got[-1][1]['slot'] <= 16, got)
    time.sleep(0.6)
    # the points are listed by frequency: the point typed to 2500 Hz is listed after the 1200 Hz one
    at = b.page(f"{POINTS}.findIndex((p) => p.f === 2500) + 1")
    check('the_mode_the_type_a_typed_value_and_the_presets_write: the row shows it, where 2500 Hz lists it',
          at == 4 and b.page(f"document.querySelector('#part-tone .pt[data-n=\"{at}\"] .cell[data-param=\"f\"]').textContent") == '2500 Hz',
          at)
    forget(b)
    tap(b, f'#part-tone .pt[data-n="{at}"] .cell[data-param="g"]')
    b.page(f"(() => {{ const i = document.querySelector('#part-tone .pt[data-n=\"{at}\"] .cell input'); i.value = '40';"
           " i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()")
    got = poll(lambda: posts(b, f'api/v1/host/eq/points/{at}'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a gain beyond the hand range is held to 12',
          bool(got) and bare(got[-1][1]) == {'g': 12, 'done': True}, got)
    forget(b)
    tap(b, '#part-presets .pre[data-name="Warm"]')
    got = poll(lambda: posts(b, 'api/v1/host/eq/preset'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a preset recalled posts its name',
          bool(got) and got[0][1].get('name') == 'Warm', got)
    time.sleep(0.8)
    check('the_mode_the_type_a_typed_value_and_the_presets_write: and is the one lit',
          b.page("document.querySelector('#part-presets .pre[data-name=Warm]').getAttribute('aria-pressed')") == 'true'
          and 'Warm' in b.page("document.querySelector('#part-presets .pstat').textContent"))
    off = b.page("[...document.querySelectorAll('#part-presets .pre[aria-disabled=true]')].map((p) => p.dataset.name + ':' + p.querySelector('small').textContent)")
    check('the_mode_the_type_a_typed_value_and_the_presets_write: an unplayable preset is disabled with its reason',
          all(':' in x and x.split(':')[1] not in ('built in', 'yours') for x in off), off)


def wheel_and_right(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    s = state(b)
    x, y = s['marks']['4']
    q0 = b.page(f'{POINTS}[3].q')
    forget(b)
    wheel(b, x, y, -120)
    wheel(b, x, y, -120)
    time.sleep(0.8)
    log = posts(b, 'api/v1/host/eq/points/4')
    q1 = b.page(f'{POINTS}[3].q')
    check('the_wheel_and_a_right_drag_set_the_q: two notches up narrow the Q, ended by a done',
          q1 > q0 and bool(log) and log[-1][1].get('done') is True and all('q' in p[1] for p in log[:-1]), [q0, q1, log])
    wheel(b, s['plot'][0] + 3, s['plot'][1] + 3, 120)
    check('the_wheel_and_a_right_drag_set_the_q: the wheel away from a point is the page\'s',
          b.page(f'{POINTS}[3].q') == q1 and len(posts(b, 'api/v1/host/eq/points/4')) == len(log))
    forget(b)
    x, y = state(b)['marks']['4']
    gesture(b, x, y, [(x + 120, y)], button='right')
    q2 = b.page(f'{POINTS}[3].q')
    check('the_wheel_and_a_right_drag_set_the_q: a right drag 120 px to the right halves the Q (one step)',
          abs(q2 - q1 / 2) <= q1 / 2 * 0.15, [q1, q2])
    log = posts(b, 'api/v1/host/eq/points/4')
    check('the_wheel_and_a_right_drag_set_the_q: the frequency and gain did not move',
          all('f' not in p[1] and 'g' not in p[1] for p in log), log[:3])


def at_fourteen(b, cell, text):
    """The model says point 3 is at +14 dB, as a preset's would be, and the cell is redrawn from it."""
    b.page(
        "(() => { const h = __core.model.page().host;"
        " const pts = h.computer.eq.points.map((p, i) => (i === 2 ? { ...p, g: 14 } : p));"
        " __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } }); })()")
    shown = poll(lambda: '14.0' in (b.page(text) or ''))
    check('an_untouched_cell_writes_nothing: the cell is redrawn from the model at +14.0 dB', shown is True, b.page(text))


def untouched(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    forget(b)
    cell = '#part-tone .pt[data-n="3"] .cell[data-param="g"]'
    tap(b, cell)
    b.press('Enter')
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: Enter on a cell as it was posts nothing', posts(b) == [], posts(b))
    # a point beyond the hand range shows its value as it is
    at_fourteen(b, cell, f"document.querySelector({json.dumps(cell)}).textContent")
    tap(b, cell)
    # the field that Enter leaves holds +14, whatever a host event did between the poll and the tap
    held = "(document.querySelector('#part-tone .pt[data-n=\"3\"] .cell input') || {}).value"
    editing = poll(lambda: '14' in (b.page(held) or ''))
    b.press('Enter')
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: Enter on a +14 dB cell as it was posts nothing (it is not pulled in to +12)',
          editing is True and posts(b) == [], (editing, b.page(held), posts(b)))
    at_fourteen(b, cell, f"document.querySelector({json.dumps(cell)}).textContent")
    tap(b, cell)
    b.page("(() => { const i = document.querySelector('#part-tone .pt[data-n=\"3\"] .cell input'); i.value = '14';"
           " i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); })()")
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: the same number typed again posts nothing', posts(b) == [], posts(b))


def sixteen(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 7)
    seven = state(b)
    seed(b, 16)
    sixteen_ = state(b)
    check('sixteen_points_keep_the_layout: sixteen rows, the table still shows six and says 10 more',
          len(sixteen_['rows']) == 16 and sixteen_['more'] == '10 more ↓ scroll', [len(sixteen_['rows']), sixteen_['more']])
    check('sixteen_points_keep_the_layout: the tone section is as tall as with seven (1 px)',
          abs(seven['height'] - sixteen_['height']) <= 1, [seven['height'], sixteen_['height']])
    check('sixteen_points_keep_the_layout: the line counts them and + Add point is off',
          sixteen_['line'].startswith('16 of 16 points')
          and b.page("document.querySelector('#part-tone [data-comp=add]').getAttribute('aria-disabled')") == 'true')
    forget(b)
    tap(b, '#part-tone [data-comp="add"]')
    check('sixteen_points_keep_the_layout: a press on it posts nothing', posts(b) == [], posts(b))


def mid_drag(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 5)
    s = state(b)
    x, y = s['marks']['3']
    press(b, x, y)
    for mx, my in path_to(x, y, [(x + 30, y - 20)]):
        move(b, mx, my)
    b.sleep(0.3)
    forget(b)
    b.page("__core.computer.flip('dac')")
    b.sleep(1.0)
    release(b, x + 30, y - 20)
    b.sleep(1.0)
    after = posts(b)
    check('a_flip_mid_drag_writes_nothing: the flip itself wrote nothing; the drag\'s own last value is the one post, done',
          len(after) == 1 and after[0][0] == 'api/v1/host/eq/points/3' and after[0][1].get('done') is True, after)
    b.sleep(1.0)
    check('a_flip_mid_drag_writes_nothing: and nothing follows', len(posts(b)) == 1, posts(b))
    check('a_flip_mid_drag_writes_nothing: the drag ends at once: no label, the DAC side shown, the part is not held',
          b.page("document.querySelectorAll('.dlab').length") == 0 and b.page('__core.computer.side()') == 'dac'
          and b.page("!!document.querySelector('#part-tone .opt[data-mode=flat]')"))
    b.page("__core.computer.flip('computer')")
    b.sleep(1.0)
    ok_ = state(b)
    check('a_flip_mid_drag_writes_nothing: back on the computer the tone part draws from the model',
          ok_['rows'][2]['cells'][1].endswith('Hz') and len(ok_['marks']) == 5, ok_['rows'][2])


def keyboard(b, d):
    load(b, d)
    b.page("document.getElementById('side-computer').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check('the_pill_keeps_the_focus_across_a_keyboard_flip: Enter on This computer leaves the focus on it',
          b.page('document.activeElement.id') == 'side-computer'
          and b.page("!!document.querySelector('sv-knob[data-computer]')"), b.page('document.activeElement.id'))
    b.page("document.getElementById('side-dac').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check('the_pill_keeps_the_focus_across_a_keyboard_flip: and back on the device\'s half',
          b.page('document.activeElement.id') == 'side-dac'
          and b.page("!document.querySelector('sv-knob[data-computer]')"), b.page('document.activeElement.id'))
    check('the_pill_keeps_the_focus_across_a_keyboard_flip: nothing written', posts(b) == [], posts(b))


def save_as(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 3)
    want = b.page(TONE)
    tap(b, '#part-presets .psave [data-open]')
    b.page("(() => { const f = document.querySelector('#part-presets form[data-form=save]');"
           " f.elements.name.value = 'Kept curve'; f.requestSubmit(); })()")
    got = poll(lambda: [p for p in posts(b) if p[0].endswith('/presets')])
    check('presets_save_rename_delete_from_the_computer_side: Save as posts the points on show',
          len(got) == 1 and got[0][1] == {'name': 'Kept curve', 'from': 'computer'}, got)
    listed = poll(lambda: b.page("[...document.querySelectorAll('#part-presets .pre')].some((p) => p.dataset.name === 'Kept curve')"))
    mine = d.model('Gungnir 2')['presets']['rows']
    saved = next((r for r in mine if r['name'] == 'Kept curve'), None)
    check('presets_save_rename_delete_from_the_computer_side: it is in the list, yours',
          listed is True and saved is not None
          and b.page("document.querySelector('#part-presets .pre[data-name=\"Kept curve\"] small').textContent") == 'yours', mine)
    forget(b)
    tap(b, '#part-presets [aria-label="Rename Kept curve"]')
    b.page("(() => { const f = document.querySelector('#part-presets form[data-form=rename]');"
           " f.elements.to.value = 'Kept two'; f.requestSubmit(); })()")
    renamed = poll(lambda: b.page("[...document.querySelectorAll('#part-presets .pre')].some((p) => p.dataset.name === 'Kept two')"))
    check('presets_save_rename_delete_from_the_computer_side: rename', renamed is True, posts(b))
    tap(b, '#part-presets [aria-label="Delete Kept two"]')
    b.page("document.querySelector('#part-presets form[data-form=delete]:not([hidden])').requestSubmit()")
    gone = poll(lambda: b.page("![...document.querySelectorAll('#part-presets .pre')].some((p) => p.dataset.name === 'Kept two')"))
    check('presets_save_rename_delete_from_the_computer_side: delete', gone is True, posts(b))
    check('presets_save_rename_delete_from_the_computer_side: the points were not changed', b.page(TONE) == want)


def guest(b, d):
    load(b, d)
    check('a_guest_sees_the_computer_inert: the guest has the switch',
          b.page("!!document.querySelector('#part-switch .opt[data-side=computer]')"))
    flip(b, 'computer')
    inert = b.page("[...document.querySelectorAll('#part-tone [data-comp], #part-presets [data-comp], sv-knob[data-computer] .step, sv-knob[data-computer] .mute')]"
                   ".filter((e) => e.getAttribute('aria-disabled') !== 'true' && !e.disabled).map((e) => e.className)")
    check('a_guest_sees_the_computer_inert: every computer control is inert', inert == [], inert)
    check('a_guest_sees_the_computer_inert: the dial too',
          b.page("document.querySelector('sv-knob[data-computer] svg.dial').getAttribute('aria-disabled')") == 'true')
    forget(b)
    for sel in ('sv-knob[data-computer] .step[data-step="1"]', '#part-tone .opt[data-mode="eq"]',
                '#part-tone [data-comp="add"]', '#part-presets .pre[data-name="Warm"]'):
        tap(b, sel)
    s = state(b)
    x, y = s['marks']['1']
    gesture(b, x, y, [(x + 30, y - 20)])
    b.click(s['plot'][0] + 40, s['plot'][1] + 20)
    time.sleep(0.5)
    check('a_guest_sees_the_computer_inert: presses and drags write nothing', posts(b) == [], posts(b))
    check('a_guest_sees_the_computer_inert: a press says to sign in',
          b.page("!!document.querySelector('#toasts .toast, .signin, [data-signin]')") is not None)


def kept(b, d):
    load(b, d)
    flip(b, 'computer')
    check('the_side_survives_a_reload: the side is kept in the browser',
          b.page("localStorage.getItem('svolder.side')") == 'computer')
    b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
    b.page(BOOT)
    poll(lambda: b.page("!!document.querySelector('sv-knob[data-computer]')"))
    time.sleep(0.8)
    check('the_side_survives_a_reload: the computer side is drawn after the load',
          b.page("!!document.querySelector('sv-knob[data-computer]') && !!document.querySelector('#part-tone.comp')"
                 " && document.querySelector('#part-switch .opt[aria-pressed=true]').dataset.side") == 'computer')
    check('the_side_survives_a_reload: the knob read the computer once it was read, no live 0 %',
          b.page("document.querySelector('sv-knob[data-computer] .big .num').textContent") not in ('0', '–'))
    check('the_side_survives_a_reload: the load wrote nothing', posts(b) == [], posts(b))


def both_sides(b, d):
    load(b, d)
    status, _ = d.post('Gungnir 2', 'presets/recall', {'name': 'Warm'})
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the DAC plays Warm', status == 200, status)
    time.sleep(1.0)
    playing = lambda: b.page(  # noqa: E731
        "JSON.stringify(__core.model.device().presets?.rows?.filter((r) => r.on).map((r) => r.name))")
    flip(b, 'computer')
    forget(b)
    tap(b, '#part-presets .pre[data-name="Warm"]')
    card = poll(lambda: b.page(f'!!{CARD}'))
    words = b.page(f'{CARD}?.querySelector(".ask-words").textContent') or ''
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: it asks, with the device named',
          card is True and 'flatten' in words and 'Gungnir 2' in words, words)
    b.page(f'{CARD}.querySelector(".ask-no").click()')
    time.sleep(0.8)
    log = posts(b)
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: no sends nothing more and says nothing',
          len(log) == 1 and log[0][2] == 409 and b.page(f'!{CARD}')
          and b.page("document.querySelectorAll('#toasts .toast').length") == 0, log)
    tap(b, '#part-presets .pre[data-name="Warm"]')
    poll(lambda: b.page(f'!!{CARD}'))
    b.page(f'{CARD}.querySelector(".ask-yes").click()')
    time.sleep(1.2)
    log = posts(b)
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: yes repeats with confirm and lands',
          len(log) == 3 and log[2][1].get('confirm') is True and log[2][2] == 200, log)
    flat = poll(lambda: 'Warm' not in (playing() or '') and b.page(f'{COMPUTER}.eq.preset') == 'Warm')
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the DAC is flat, the computer plays it',
          flat is True, [playing(), b.page(f'{COMPUTER}.eq.preset')])


def amp(b, d):
    load(b, d)
    b.page("__core.actions.showDevice('Amp').then(() => 1)")
    poll(lambda: b.page("document.querySelector('.view')?.dataset.family") == 'amp' or
         b.page("!!document.querySelector('.view.amp')"))
    time.sleep(1.2)
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: the amp page has the switch, named for the amp',
          b.page("[...document.querySelectorAll('#part-switch .opt')].map((o) => o.textContent)") == ['Amp', 'This computer'])
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: its own layout first',
          b.page("!!document.querySelector('.view.amp') && !document.querySelector('sv-knob[data-computer]')"))
    flip(b, 'computer')
    time.sleep(0.8)
    parts = b.page("({ dac: !!document.querySelector('.view.dac'), knob: !!document.querySelector('sv-knob[data-computer] svg.dial'),"
                   " tone: !!document.querySelector('#part-tone.comp .eqg.pts'), presets: !!document.querySelector('#part-presets.comp'),"
                   " rows: document.querySelectorAll('#part-tone .pt').length,"
                   " text: document.querySelector('sv-knob[data-computer] .big').textContent.trim() })")
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: the DAC page\'s knob, tone section and presets',
          parts['dac'] and parts['knob'] and parts['tone'] and parts['presets'] and parts['rows'] >= 1
          and parts['text'].replace(' ', '').endswith('%'), parts)
    forget(b)
    tap(b, 'sv-knob[data-computer] .step[data-step="1"]')
    got = poll(lambda: posts(b, 'api/v1/host/volume'))
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: and they write the computer\'s volume',
          len(got) == 1, got)
    flip(b, 'dac')
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: flipped back it is the amp\'s page',
          b.page("!!document.querySelector('.view.amp')"))


def touch():
    """A finger on the curve (Chromium, touch emulation, a phone)."""
    d = common.Daemon('gungnir2-v2', host='fake', env={})
    b = common.Chromium(size=(390, 844), dpr=2, touch=True)
    b.preload(HOOK)
    try:
        load(b, d)
        flip(b, 'computer')
        seed(b, 4)
        b.page("document.querySelector('#part-tone .eqg').scrollIntoView({ block: 'center' })")
        time.sleep(0.4)
        s = state(b)
        px, py, pw, ph = s['plot']
        spot = None
        for fx in (0.15, 0.3, 0.5, 0.7, 0.85):
            for fy in (0.15, 0.85, 0.3, 0.7):
                x, y = px + fx * pw, py + fy * ph
                if all(math.hypot(x - mx, y - my) > 40 for mx, my in s['marks'].values()):
                    spot = (x, y)
                    break
            if spot:
                break

        def fingers(pts):
            return [{'x': x, 'y': y, 'id': i} for i, (x, y) in enumerate(pts)]
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': fingers([spot])})
        b.sleep(0.05)
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        b.sleep(1.0)
        got = posts(b, 'api/v1/host/eq/points')
        check('a_finger_adds_drags_and_pinches: a tap on an empty spot adds a PK',
              len(got) == 1 and got[0][1]['type'] == 'PK', got)
        forget(b)
        s = state(b)
        x, y = s['marks']['2']
        scroll = b.page('window.scrollY')
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': fingers([(x, y)])})
        # straight up: the gain moves and the frequency does not, so the table keeps its order
        for mx, my in path_to(x, y, [(x, y - 30)]):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': fingers([(mx, my)])})
            b.sleep(0.03)
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        b.sleep(1.2)
        log = posts(b, 'api/v1/host/eq/points/2')
        check('a_finger_adds_drags_and_pinches: a finger on a point drags it, done at the lift, no scroll',
              len(log) >= 2 and log[-1][1].get('done') is True and b.page('window.scrollY') == scroll, [len(log), scroll])
        forget(b)
        s = state(b)
        x, y = s['marks']['2']
        q0 = b.page(f'{POINTS}[1].q')
        a, c = (x - 40, y), (x + 40, y)
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': fingers([a, c])})
        for i in range(1, 11):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': fingers(
                [(a[0] - 4 * i, a[1]), (c[0] + 4 * i, c[1])])})
            b.sleep(0.03)
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        b.sleep(1.2)
        q1 = b.page(f'{POINTS}[1].q')
        check('a_finger_adds_drags_and_pinches: two fingers twice as far apart halve the Q',
              abs(q1 - q0 / 2) <= q0 / 2 * 0.2, [q0, q1])
        check('a_finger_adds_drags_and_pinches: no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def one(name, fn, env=None, fleet=FLEET, web='auth = "none"', size=(1280, 1100)):
    d = common.Daemon('gungnir2-v2', host='fake', fleet=fleet, env=env or {}, web=web)
    b = make(size)
    try:
        fn(b, d)
        check(f'{name}: no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def pictures():
    """The computer side, desktop and phone, for looking (Chromium)."""
    os.makedirs(SHOTS, exist_ok=True)
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_SIGNAL': '1'})
    try:
        for name, size, dpr in (('desktop', (1280, 1100), 1), ('phone', (390, 844), 2)):
            b = common.Chromium(size=size, dpr=dpr)
            b.preload(HOOK)
            try:
                load(b, d, '?theme=ember')
                flip(b, 'computer')
                seed(b, 7)
                time.sleep(1.0)
                if name == 'phone':
                    full = b.page('document.documentElement.scrollHeight')
                    b.resize(390, full)
                    time.sleep(0.8)
                path = f'{SHOTS}/computer-{name}.png'
                with open(path, 'wb') as f:
                    f.write(b.shot())
                print('shot', path)
            finally:
                b.close()
    finally:
        d.stop()


one('the_pill_flips_volume_and_tone_together', pill)
one('minus_plus_mute_set_the_computer_volume', volume)
one('a_point_drag_posts_live_and_done_at_release', drag)
one('a_tap_adds_a_point_and_x_removes_it', add_remove)
one('the_mode_the_type_a_typed_value_and_the_presets_write', controls)
one('the_points_are_listed_by_frequency_and_the_selection_follows', sorted_points)
one('the_wheel_and_a_right_drag_set_the_q', wheel_and_right)
one('an_untouched_cell_writes_nothing', untouched)
one('sixteen_points_keep_the_layout', sixteen)
if browser == 'chromium':
    one('a_flip_mid_drag_writes_nothing', mid_drag)
one('a_guest_sees_the_computer_inert', guest, web='guest = "read"')
one('the_side_survives_a_reload', kept)
one('the_pill_keeps_the_focus_across_a_keyboard_flip', keyboard)
one('presets_save_rename_delete_from_the_computer_side', save_as)
one('the_same_preset_on_both_asks_and_yes_flattens_the_dac', both_sides)
one('the_amp_page_flips_to_the_computer_with_the_dac_parts', amp)
if browser == 'chromium':
    touch()
    pictures()
print(f'computer_ember.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
