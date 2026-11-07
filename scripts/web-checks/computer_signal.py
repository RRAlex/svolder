# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Signal's computer side in a real page (design: the computer flip), through the page's own
controls, on a fake host (`SVOLDER_HOST=fake`: its sink and EQ live in memory, no PipeWire, no sound).
Choosing the computer's hop on the line is the flip; the area below then is the DAC's screen acting
on the computer:
  the_hop_flips_volume_and_tone_together   the line's computer hop (`This computer`, `controls below`
                                        while chosen) flips the level and the tone control, writes
                                        nothing, and another hop flips back
  a_keyboard_flip_puts_the_focus_on_the_chosen_hop   the arrow keys choose the hop and the focus is on it
  minus_plus_mute_set_the_computer_volume   the buttons step the level 1 %, a held one repeats, mute
                                        posts and keeps the number
  a_point_drag_posts_live_and_done_at_release   a drag posts `done: false` while it moves (at most 20 a
                                        second) and one `done: true` at the end; the dot stays ON the
                                        curve and the row is lit and scrolled into view
  a_tap_adds_a_point_and_x_removes_it   a tap on an empty spot posts a PK of Q 1, the row's x removes it,
                                        `+ Add point` adds one at the widest gap
  the_mode_the_type_a_typed_value_and_the_presets_write   Off | EQ, the type choice, a value typed into
                                        a field, a preset recalled
  presets_save_rename_and_delete_on_the_computer_side   Save as, Rename and Delete work as on the DAC
  the_wheel_sets_the_q                  a notch over a dot steps its Q, away from one it is the page's
  sixteen_points_keep_the_layout        the table is as tall with 16 points as with 7
  a_flip_mid_drag_ends_it_with_one_final_value   live posts before the flip, then a flip ends the drag and exactly
                                        one done carries its last value, nothing else (Chromium)
  a_guest_sees_the_computer_inert       a guest may flip and every computer control is inert
  the_side_survives_a_reload            the hop chosen is the computer's after a load, the first paint
                                        never a live 0 %, and the load writes nothing
  the_same_preset_on_both_asks_and_yes_flattens_the_dac   the question card; no sends nothing, yes
                                        confirms and the DAC is flat
  the_amp_page_flips_to_the_computer_with_the_dac_parts   the fake amp's line has the hop and, chosen,
                                        the DAC page's level, curve and points
  a_load_with_the_computer_kept_but_unavailable_shows_the_dac   the DAC's screen is drawn and the hop says why
  the_wire_still_shows_the_eq_changing_the_sound   a route that changes the sound keeps the computer's
                                        stretch yellow and says so, chosen or not
  a_finger_adds_drags_and_pinches       (Chromium, touch emulation) a tap adds, a finger drags a dot,
                                        two fingers halve its Q
Pictures of the computer side (desktop, phone, the phone's table) are saved under target/web-shots/signal/
(Chromium run).
Usage: python3 computer_signal.py chromium|firefox"""
import json
import math
import os
import re
import sys
import time

import common

os.environ.setdefault('SVOLDER_FAKE_COMPUTER_VOLUME', '42')

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()
SHOTS = f'{common.REPO}/target/web-shots/signal'


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
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };"
        # every text the level has shown since the page began: a live 0 % must never be among them
        " window.__nums = []; new MutationObserver(() => {"
        " const n = document.querySelector('#sg-cbig .sg-num');"
        " if (n && window.__nums[window.__nums.length - 1] !== n.textContent) window.__nums.push(n.textContent); })"
        ".observe(document, { subtree: true, childList: true, characterData: true });")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
COMPUTER = '__core.model.page().host?.computer'
POINTS = f'{COMPUTER}.eq.points'
CARD = "document.querySelector('.ask')"
HOP = '#sg-line [data-hop="computer"]'
DEVICE_HOP = '#sg-line [data-hop="device:Gungnir%202"]'


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


def load(b, d, query='?theme=signal&effects=off'):
    b.goto(f'{d.url}{query}', settle=1.0)
    assert b.page(BOOT) == 'signal'
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
    """Choose the computer's hop (or the device's): what a person taps on the line."""
    tap(b, HOP if side == 'computer' else DEVICE_HOP)
    poll(lambda: b.page(f"__core.computer.side() === '{side}'"))
    time.sleep(0.5)


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
    time.sleep(0.6)
    forget(b)


STATE = """(() => {
  const graph = document.querySelector('#sg-cgraph'); const p = graph.getBoundingClientRect();
  const marks = {};
  for (const m of graph.querySelectorAll('.sg-pt')) {
    const r = m.getBoundingClientRect();
    marks[m.dataset.n] = [r.left + r.width / 2, r.top + r.height / 2];
  }
  const box = document.querySelector('#sg-points .sg-pscroll'); const frame = box.getBoundingClientRect();
  const pts = document.querySelector('#sg-points');
  return { marks, plot: [p.left, p.top, p.width, p.height], height: pts.getBoundingClientRect().height,
    shelf: document.querySelector('#sg-shelf').getBoundingClientRect().height,
    open: document.querySelector('#sg-open').getBoundingClientRect().height,
    rows: [...box.querySelectorAll('.sg-prow')].map((r) => {
      const b = r.getBoundingClientRect();
      return { n: Number(r.dataset.n), on: r.classList.contains('on'), inside: b.top >= frame.top - 1 && b.bottom <= frame.bottom + 1,
        cells: [r.querySelector('select').value, ...[...r.querySelectorAll('input')].map((c) => c.value)] };
    }),
    lit: [...graph.querySelectorAll('.sg-pt.on')].map((m) => Number(m.dataset.n)),
    more: document.querySelector('.sg-pmore').textContent, scroll: box.scrollTop,
    label: document.querySelectorAll('.sg-dlab').length,
    line: document.querySelector('#sg-cline').textContent }; })()"""


def state(b):
    return b.page(STATE)


# The curve's y at a dot's x, from the drawn path (a polyline of 181 points), in the plot's px.
ON_CURVE = """(() => {
  const graph = document.querySelector('#sg-cgraph'); const r = graph.getBoundingClientRect();
  const d = graph.querySelector('.sg-curve').getAttribute('d');
  const pts = [...d.matchAll(/[ML]([\\d.]+) ([\\d.-]+)/g)].map((m) => [Number(m[1]), Number(m[2])]);
  return [...graph.querySelectorAll('.sg-pt')].map((m) => {
    const x = Number(m.dataset.x) * 1000; const k = pts.findIndex(([px]) => px >= x);
    const [x0, y0] = pts[Math.max(k - 1, 0)]; const [x1, y1] = pts[k];
    const y = x1 === x0 ? y1 : y0 + (y1 - y0) * (x - x0) / (x1 - x0);
    return Math.abs(y / 260 - Number(m.dataset.y)) * r.height; }); })()"""


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


def type_into(b, selector, text):
    """Focus the field as a tap does, put `text` in it and press Enter."""
    tap(b, selector)
    focused = b.page(f"document.activeElement === document.querySelector({json.dumps(selector)})")
    b.page(f"(() => {{ const i = document.querySelector({json.dumps(selector)}); i.value = {json.dumps(text)};"
           " i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); })()")
    return focused


# ------------------------------------------------------------------------------------- the cases

def hop_flip(b, d):
    load(b, d)
    words = b.page("[...document.querySelectorAll('#sg-line [data-hop=\"computer\"] b, #sg-line [data-hop=\"computer\"] .sg-w')].map((e) => e.textContent)")
    check('the_hop_flips_volume_and_tone_together: the hop is This computer, with PipeWire and the rate',
          # (the fake computer starts at 42 %: a third word may be the path's own `computer volume N %` or `computer muted`)
          len(words) in (2, 3) and words[:1] == ['This computer'] and words[1].startswith('PipeWire · ')
          and (len(words) == 2 or re.fullmatch(r'(changes the sound|computer volume \d+ ?%|computer muted)', words[2]) is not None), words)
    check('the_hop_flips_volume_and_tone_together: the DAC side first (its own level and curve)',
          b.page("!!document.querySelector('#sg-vtrack') && !!document.querySelector('#sg-graph')"
                 " && !document.querySelector('#sg-cvtrack')") is True)
    flip(b, 'computer')
    check('the_hop_flips_volume_and_tone_together: the hop says its controls are below, boxed',
          b.page(f"document.querySelector('{HOP}').innerText.includes('controls below') && "
                 f"document.querySelector('{HOP}').getAttribute('aria-selected') === 'true'") is True)
    volume = b.page("document.querySelector('#sg-cbig').textContent.trim()")
    pct = b.page(f'{COMPUTER}.volume.pct')
    check('the_hop_flips_volume_and_tone_together: the level reads the computer, in %',
          volume.replace(' ', '') == f'{pct}%', [volume, pct])
    mode = b.page("[...document.querySelectorAll('#sg-cmode [data-act=cmode]')].map((o) => o.textContent + o.getAttribute('aria-pressed'))")
    check('the_hop_flips_volume_and_tone_together: the tone control reads Off | EQ',
          mode == ['Offtrue', 'EQfalse'], mode)
    check('the_hop_flips_volume_and_tone_together: a flip posts nothing', posts(b) == [], posts(b))
    check('the_hop_flips_volume_and_tone_together: nothing below the line says computer',
          b.page("/computer/i.test(document.querySelector('#sg-open').innerText + ' ' + document.querySelector('#sg-shelf').innerText"
                 " + ' ' + document.querySelector('#sg-open').innerHTML + ' ' + document.querySelector('#sg-shelf').innerHTML)") is False)
    facts = b.page("[...document.querySelectorAll('#sg-cfacts dt')].map((e) => e.textContent)")
    check('the_hop_flips_volume_and_tone_together: the facts stay under the controls: the sound card, its format, '
          'the other streams, the way', facts[:3] == ['Sound card', 'Rate and format', 'Other streams'], facts)
    check('the_hop_flips_volume_and_tone_together: and no row for what is a control now',
          b.page("/Muted|Host EQ|Volume in PipeWire/.test(document.querySelector('#sg-cfacts').innerText)") is False)
    flip(b, 'dac')
    check('the_hop_flips_volume_and_tone_together: another hop flips back to the DAC\'s own screen',
          b.page("!!document.querySelector('#sg-vtrack') && !document.querySelector('#sg-cvtrack')") is True
          and b.page("localStorage.getItem('svolder.side')") == 'dac')
    check('the_hop_flips_volume_and_tone_together: no write on the way back', posts(b) == [], posts(b))
    check('the_hop_flips_volume_and_tone_together: the focus is on the DAC\'s hop after the flip back',
          b.page("document.activeElement.dataset.hop") == 'device:Gungnir%202', b.page("document.activeElement.outerHTML.slice(0, 80)"))
    # the player's hop is another hop: choosing it flips back too
    flip(b, 'computer')
    tap(b, '#sg-line [data-hop="player"]')
    check('the_hop_flips_volume_and_tone_together: the player\'s hop flips back as well',
          b.page("__core.computer.side()") == 'dac' and b.page("!!document.querySelector('#sg-player')") is True)


def keyboard(b, d):
    load(b, d)
    b.page(f"document.querySelector('{DEVICE_HOP}').focus()")
    b.press('ArrowLeft')
    time.sleep(0.5)
    at = b.page("document.activeElement.dataset.hop")
    check('a_keyboard_flip_puts_the_focus_on_the_chosen_hop: Left from the DAC chooses the computer and the '
          'focus is on its hop', b.page('__core.computer.side()') == 'computer' and at == 'computer', at)
    check('a_keyboard_flip_puts_the_focus_on_the_chosen_hop: the hop is the one chosen',
          b.page(f"document.querySelector('{HOP}').getAttribute('aria-selected')") == 'true')
    b.press('ArrowRight')
    time.sleep(0.5)
    at = b.page("document.activeElement.dataset.hop")
    check('a_keyboard_flip_puts_the_focus_on_the_chosen_hop: Right goes back to the DAC, the focus on its hop',
          b.page('__core.computer.side()') == 'dac' and at == 'device:Gungnir%202', at)
    check('a_keyboard_flip_puts_the_focus_on_the_chosen_hop: no write', posts(b) == [], posts(b))
    # Enter on the computer's hop chooses it as a tap does
    b.page(f"document.querySelector('{HOP}').focus()")
    b.press('Enter')
    time.sleep(0.5)
    check('a_keyboard_flip_puts_the_focus_on_the_chosen_hop: Enter chooses it',
          b.page('__core.computer.side()') == 'computer'
          and b.page("document.activeElement.dataset.hop") == 'computer')


def volume(b, d):
    load(b, d)
    flip(b, 'computer')
    start = b.page(f'{COMPUTER}.volume.pct')
    tap(b, '#sg-cvup')
    got = poll(lambda: posts(b, 'api/v1/host/volume'))
    check('minus_plus_mute_set_the_computer_volume: + posts one step of 1 %',
          [p[1] for p in got] == [{'pct': start + 1}], got)
    shown = poll(lambda: b.page("document.querySelector('#sg-cbig .sg-num').textContent") == str(start + 1))
    check('minus_plus_mute_set_the_computer_volume: and the level shows it', shown is True)
    tap(b, '#sg-cvdown')
    tap(b, '#sg-cvdown')
    time.sleep(0.6)
    check('minus_plus_mute_set_the_computer_volume: two steps down read 2 % lower',
          b.page(f'{COMPUTER}.volume.pct') == start - 1, b.page(f'{COMPUTER}.volume'))
    forget(b)
    tap(b, '#sg-cmute')
    got = poll(lambda: posts(b, 'api/v1/host/mute'))
    check('minus_plus_mute_set_the_computer_volume: mute posts muted',
          [p[1] for p in got] == [{'muted': True}], got)
    time.sleep(0.5)
    check('minus_plus_mute_set_the_computer_volume: and keeps the number, the button saying Unmute',
          b.page("document.querySelector('#sg-cbig .sg-num').textContent") == str(start - 1)
          and b.page("document.querySelector('#sg-cmute').textContent") == 'Unmute')
    tap(b, '#sg-cmute')
    time.sleep(0.5)
    check('minus_plus_mute_set_the_computer_volume: unmute', b.page(f'{COMPUTER}.volume.muted') is False)
    # a press on the track and a key
    forget(b)
    rect = b.page("(() => { const r = document.querySelector('#sg-cvtrack').getBoundingClientRect();"
                  " return [r.left, r.top + r.height / 2, r.width]; })()")
    b.click(rect[0] + rect[2] * 0.8, rect[1])
    time.sleep(0.8)
    got = posts(b, 'api/v1/host/volume')
    check('minus_plus_mute_set_the_computer_volume: a press at four fifths of the track asks for 80 %',
          bool(got) and bare(got[-1][1]) == {'pct': 80} and b.page(f'{COMPUTER}.volume.pct') == 80, got)
    forget(b)
    b.page("document.querySelector('#sg-cvtrack').focus()")
    b.press('ArrowDown')
    time.sleep(0.6)
    check('minus_plus_mute_set_the_computer_volume: the arrow down on the focused track is one step',
          b.page(f'{COMPUTER}.volume.pct') == 79, b.page(f'{COMPUTER}.volume'))
    forget(b)
    x, y = common.center(b, '#sg-cvup')
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
          len(s['marks']) == 7 and len(s['rows']) == 7 and s['more'] == 'one more below ↓'
          and s['rows'][6]['inside'] is False and s['rows'][0]['inside'] is True, s)
    off = b.page(ON_CURVE)
    check('a_point_drag_posts_live_and_done_at_release: every dot is ON the curve (within 1.5 px), a shelf\'s too',
          len(off) == 7 and max(off) < 1.5, off)
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
    gaps = [round(y2[3] - x2[3]) for x2, y2 in zip(log[:-2], log[1:-1])]
    check('a_point_drag_posts_live_and_done_at_release: at most 20 a second (the live ones 45 ms apart or more)',
          all(g >= 45 for g in gaps), gaps)
    held = [p[1] for p in log]
    check('a_point_drag_posts_live_and_done_at_release: the posts carry the point\'s frequency and gain',
          any('f' in p for p in held) and any('g' in p for p in held), held[:4])
    cx, cy = a['marks']['4']
    # the dot follows the pointer in x; in y it is ON the summed curve, so it can sit off the pointer
    check('a_point_drag_posts_live_and_done_at_release: the dot is under the release in x (3 px)',
          abs(cx - (x + 40)) <= 3, [cx - x - 40])
    check('a_point_drag_posts_live_and_done_at_release: the row says what the model holds, and is lit with its dot',
          a['rows'][3]['cells'][1] != before[1] and a['rows'][3]['cells'][2] != before[2]
          and a['rows'][3]['on'] is True and a['lit'] == [4], [before, a['rows'][3], a['lit']])
    point = b.page(f'{POINTS}[3]')
    check('a_point_drag_posts_live_and_done_at_release: the daemon holds the last value sent',
          abs(point['g'] - held[-1].get('g', point['g'])) < 0.01 and point['f'] != 2900, [point, held[-1]])
    off = b.page(ON_CURVE)
    check('a_point_drag_posts_live_and_done_at_release: and the dots are still on the curve afterwards',
          max(off) < 1.5, off)
    if during:
        check('a_point_drag_posts_live_and_done_at_release: while it moves a label shows and the row is lit',
              during['label'] == 1 and during['rows'][3]['on'] is True, during)
    x, y = a['marks']['7']
    gesture(b, x, y, [(x - 20, y + 10)])
    z = state(b)
    check('a_point_drag_posts_live_and_done_at_release: the dragged point\'s row is lit and scrolled into view',
          z['rows'][6]['on'] is True and z['rows'][6]['inside'] is True and z['scroll'] > 0 and z['more'] == '',
          [z['rows'][6], z['scroll'], z['more']])
    b.page("document.querySelector('#sg-points .sg-pscroll').scrollTop = 0")
    time.sleep(0.3)
    check('a_point_drag_posts_live_and_done_at_release: scrolled back, the words come back',
          state(b)['more'] == 'one more below ↓')
    # a keyboard step of a focused dot: one grid step of the frequency, written done
    forget(b)
    b.page("document.querySelector('#sg-cpt-2').focus()")
    f0 = b.page(f'{POINTS}[1].f')
    b.press('ArrowRight')
    time.sleep(0.8)
    got = posts(b, 'api/v1/host/eq/points/2')
    check('a_point_drag_posts_live_and_done_at_release: an arrow on a focused dot steps its frequency, done',
          len(got) == 1 and got[0][1].get('done') is True and got[0][1]['f'] > f0, [f0, got])


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
    time.sleep(0.6)
    s = state(b)
    # the points are listed by frequency: the new one is where its frequency puts it
    at = b.page(f"{POINTS}.findIndex((p) => Math.abs(p.f - {add['f']}) < 0.01) + 1")
    nx, ny = s['marks'][str(at)]
    off = b.page(ON_CURVE)
    check('a_tap_adds_a_point_and_x_removes_it: the new point is drawn under the tap in x (4 px), on the curve',
          abs(nx - spot[0]) <= 4 and max(off) < 1.5, [nx - spot[0], off])
    label = b.page(f"document.querySelector('#sg-crm-{at}').getAttribute('aria-label')")
    check('a_tap_adds_a_point_and_x_removes_it: its row has an x labelled Remove point %d' % at,
          label == 'Remove point %d' % at and b.page(f"document.querySelector('#sg-crm-{at}').textContent") == '×', label)
    forget(b)
    tap(b, f'#sg-crm-{at}')
    got = poll(lambda: posts(b, f'api/v1/host/eq/points/{at}/remove'))
    check('a_tap_adds_a_point_and_x_removes_it: the x removes it', len(got) == 1, got)
    check('a_tap_adds_a_point_and_x_removes_it: three points again',
          poll(lambda: b.page(f'{POINTS}.length') == 3) is True)
    forget(b)
    tap(b, '#sg-cadd')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points'))
    freqs = sorted([20, 20000] + [p['f'] for p in b.page(POINTS)[:3]])
    gap = max(range(len(freqs) - 1), key=lambda i: math.log(freqs[i + 1] / freqs[i]))
    want = math.sqrt(freqs[gap] * freqs[gap + 1])
    check('a_tap_adds_a_point_and_x_removes_it: + Add point adds a PK, 0 dB, Q 1 at the widest gap',
          len(got) == 1 and got[0][1]['g'] == 0 and got[0][1]['q'] == 1 and abs(got[0][1]['f'] - want) < 0.5,
          [got, want])
    check('a_tap_adds_a_point_and_x_removes_it: the button reads + Add point',
          b.page("document.querySelector('#sg-cadd').textContent") == '+ Add point')
    time.sleep(0.6)
    forget(b)
    for n in (4, 3, 2):
        tap(b, f'#sg-crm-{n}')
        time.sleep(0.5)
    b.page("__core.actions.computer.setPoint(1, { g: 3 }, { done: true }).then(() => 1)")
    poll(lambda: b.page(f'{POINTS}[0].g') == 3)
    forget(b)
    tap(b, '#sg-crm-1')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/1/remove'))
    flat = poll(lambda: b.page(f'{POINTS}[0].g') == 0)
    check('a_tap_adds_a_point_and_x_removes_it: the lone point keeps its x, labelled Remove point 1, and it sets the point flat',
          len(got) == 1 and flat is True and b.page(f'{POINTS}.length') == 1
          and b.page("document.querySelector('#sg-crm-1').getAttribute('aria-label')") == 'Remove point 1',
          [got, b.page(POINTS)])


def sorted_points(b, d):
    name = 'the_points_are_listed_by_frequency_and_the_selection_follows'
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    fs = lambda: b.page(f'{POINTS}.map((p) => p.f)')
    start = fs()
    check(f'{name}: the daemon lists the points by frequency', start == sorted(start), start)
    tap(b, '.sg-prow[data-n="1"] .sg-pn')
    check(f'{name}: point 1 is the row in hand', b.page("document.querySelector('.sg-prow[data-n=\"1\"]').classList.contains('on')") is True)
    b.page("__core.actions.computer.setPoint(1, { f: 19000 }, { done: false }).then(() => 1)")
    mid = poll(lambda: fs()[0] == 19000 and fs())
    check(f'{name}: mid-drag point 1 is still point 1 (nothing jumps under the finger)',
          mid and mid[1:] == start[1:], [start, mid])
    b.page("__core.actions.computer.setPoint(1, {}, { done: true }).then(() => 1)")
    want = sorted(start[1:] + [19000])
    check(f'{name}: at the release the table is in order', poll(lambda: fs() == want) is True, [start, fs()])
    last = len(start)
    time.sleep(0.6)
    on = b.page("[...document.querySelectorAll('.sg-prow.on')].map((r) => r.dataset.n)")
    check(f'{name}: and the row in hand is the one that moved', on == [str(last)], on)
    nums = b.page("[...document.querySelectorAll('.sg-prow .sg-pn')].map((r) => r.textContent.trim())")
    check(f'{name}: the # column counts the sorted order', nums == [str(i + 1) for i in range(last)], nums)


def at_fourteen(b, field):
    """The model says point 3 is at +14 dB, as a preset's would be, and the field is redrawn from it."""
    b.page(
        "(() => { const h = __core.model.page().host;"
        " const pts = h.computer.eq.points.map((p, i) => (i === 2 ? { ...p, g: 14 } : p));"
        " __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } }); })()")
    text = f"document.querySelector('{field}').value"
    shown = poll(lambda: '14.0' in (b.page(text) or ''))
    check('an_untouched_cell_writes_nothing: the field is redrawn from the model at +14.0 dB', shown is True, b.page(text))


def untouched(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    forget(b)
    tap(b, '#sg-cf-3g')
    b.press('Enter')
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: Enter on a cell as it was posts nothing', posts(b) == [], posts(b))
    # a point beyond the hand range shows its value as it is
    at_fourteen(b, '#sg-cf-3g')
    tap(b, '#sg-cf-3g')
    # the field that Enter leaves holds +14, whatever a host event did between the poll and the tap
    held = "(document.querySelector('#sg-cf-3g') || {}).value"
    editing = poll(lambda: '14.0' in (b.page(held) or ''))
    b.press('Enter')
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: Enter on a +14 dB field as it was posts nothing (it is not pulled in to +12)',
          editing is True and posts(b) == [], (editing, b.page(held), posts(b)))
    at_fourteen(b, '#sg-cf-3g')
    type_into(b, '#sg-cf-3g', '14')
    time.sleep(0.6)
    check('an_untouched_cell_writes_nothing: the same number typed again posts nothing', posts(b) == [], posts(b))
    # the host says point 3 is at +14 dB and the key is pressed in the same breath
    b.page("(() => { EMIT const dot = document.querySelector('.sg-pt[data-n=\"3\"]'); dot.focus();"
           " dot.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })); })()".replace('EMIT', "const h = __core.model.page().host; const pts = h.computer.eq.points.map((p, i) => i === 2 ? { ...p, g: 14 } : p); __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } });"))
    time.sleep(0.5)
    check('a_step_against_its_key_does_nothing: ArrowUp on a +14 dB point posts nothing',
          posts(b) == [], posts(b))


def unread_volume(b, d):
    load(b, d)
    flip(b, 'computer')
    b.page("(() => { const h = __core.model.page().host;"
           " __core.model.emit('host', { ...h, computer: { ...h.computer, volume: null } }); })()")
    time.sleep(0.5)
    forget(b)
    unread = b.page("__core.computer.volumeOf(__core.model.page())?.inert")
    tap(b, '#sg-cvtrack')
    tap(b, '#sg-cmute')
    tap(b, '#sg-cvup')
    tap(b, '#sg-cvdown')
    time.sleep(0.5)
    check('an_unread_volume_writes_nothing: a press on the track, Mute and the steps send nothing before the first read',
          unread is True and posts(b, 'api/v1/host/volume') == [] and posts(b, 'api/v1/host/mute') == [],
          [unread, posts(b)])


def controls(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 4)
    tap(b, '#sg-cm-off')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check('the_mode_the_type_a_typed_value_and_the_presets_write: Off posts off',
          modes == [{'on': False}] and b.page(f'{COMPUTER}.eq.on') is False
          and b.page("document.querySelector('#sg-cm-off').getAttribute('aria-pressed')") == 'true', modes)
    forget(b)
    tap(b, '#sg-cm-eq')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check('the_mode_the_type_a_typed_value_and_the_presets_write: EQ posts on',
          modes == [{'on': True}] and b.page(f'{COMPUTER}.eq.on') is True, modes)
    forget(b)
    b.page("(() => { const s = document.querySelector('#sg-ct-2');"
           " s.value = 'HSC'; s.dispatchEvent(new Event('change', { bubbles: true })); })()")
    time.sleep(0.8)
    check('the_mode_the_type_a_typed_value_and_the_presets_write: the type choice writes the shelf',
          b.page(f'{POINTS}[1].type') == 'HSC' and b.page("document.querySelector('#sg-ct-2').value") == 'HSC',
          b.page(f'{POINTS}[1]'))
    forget(b)
    typed = type_into(b, '#sg-cf-3f', '2.5k')
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a tap on a value makes it a field to type in', typed is True)
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/3'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: 2.5k is sent as 2500, done',
          bool(got) and bare(got[-1][1]) == {'f': 2500, 'done': True}, got)
    check('every_point_write_carries_its_slot: the typed value names its point by slot', bool(got) and isinstance(got[-1][1].get('slot'), int)
          and 1 <= got[-1][1]['slot'] <= 16, got)
    time.sleep(0.8)
    # the points are listed by frequency: the point typed to 2500 Hz is listed after the 1200 Hz one
    at = b.page(f"{POINTS}.findIndex((p) => p.f === 2500) + 1")
    check('the_mode_the_type_a_typed_value_and_the_presets_write: the row shows it, where 2500 Hz lists it',
          at == 4 and b.page(f"document.querySelector('#sg-cf-{at}f').value") == '2500 Hz', at)
    forget(b)
    type_into(b, f'#sg-cf-{at}g', '40')
    got = poll(lambda: posts(b, f'api/v1/host/eq/points/{at}'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a gain beyond the hand range is held to 12',
          bool(got) and bare(got[-1][1]) == {'g': 12, 'done': True}, got)
    forget(b)
    type_into(b, f'#sg-cf-{at}q', 'abc')
    time.sleep(0.5)
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a word is not a number and writes nothing',
          posts(b, f'api/v1/host/eq/points/{at}') == [])
    forget(b)
    tap(b, '[data-act="cpreset"][data-name="Warm"]')
    got = poll(lambda: posts(b, 'api/v1/host/eq/preset'))
    check('the_mode_the_type_a_typed_value_and_the_presets_write: a preset recalled posts its name',
          bool(got) and got[0][1].get('name') == 'Warm', got)
    time.sleep(0.8)
    check('the_mode_the_type_a_typed_value_and_the_presets_write: and is the one lit and named',
          b.page("document.querySelector('[data-act=cpreset][data-name=Warm]').getAttribute('aria-pressed')") == 'true'
          and 'Warm' in b.page("document.querySelector('#sg-cpresets .sg-status').textContent"))
    off = b.page("[...document.querySelectorAll('[data-act=cpreset][aria-disabled=true]')].map((p) => p.dataset.name + ':' + p.title)")
    check('the_mode_the_type_a_typed_value_and_the_presets_write: an unplayable preset is quiet with its reason',
          all(x.split(':')[1] != '' for x in off), off)


def preset_edits(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 3)
    forget(b)
    tap(b, '#sg-cpreset-save')
    form = poll(lambda: b.page("!!document.querySelector('#sg-cpresets .sg-askbox input')"))
    check('presets_save_rename_and_delete_on_the_computer_side: Save as opens the name form', form is True)
    b.page("(() => { const i = document.querySelector('#sg-cpresets .sg-askbox input'); i.value = 'Mine';"
           " i.closest('form').requestSubmit(); })()")
    got = poll(lambda: posts(b, 'api/v1/devices/Gungnir%202/presets'))
    check('presets_save_rename_and_delete_on_the_computer_side: the curve is saved from the computer under the name',
          bool(got) and got[0][1] == {'name': 'Mine', 'from': 'computer'}, got)
    listed = poll(lambda: b.page("[...document.querySelectorAll('[data-act=cpreset]')].some((p) => p.dataset.name === 'Mine')"), 10)
    check('presets_save_rename_and_delete_on_the_computer_side: it joins the list, as one of the person\'s own',
          listed is True, b.page("[...document.querySelectorAll('[data-act=cpreset]')].map((p) => p.dataset.name)"))
    check('presets_save_rename_and_delete_on_the_computer_side: Rename and Delete are offered now',
          b.page("!!document.querySelector('#sg-cpreset-rename') && !!document.querySelector('#sg-cpreset-delete')") is True)
    forget(b)
    tap(b, '#sg-cpreset-rename')
    # the preset the curve is on is the one renamed; otherwise a list of the person's own asks which
    if b.page("!!document.querySelector('.sg-list')"):
        tap(b, '.sg-list .sg-opt[data-value="Mine"]')
    form = poll(lambda: b.page("!!document.querySelector('#sg-cpresets .sg-askbox input')"))
    b.page("(() => { const i = document.querySelector('#sg-cpresets .sg-askbox input'); i.value = 'Mine two';"
           " i.closest('form').requestSubmit(); })()")
    got = poll(lambda: posts(b, 'api/v1/devices/Gungnir%202/presets/rename'))
    check('presets_save_rename_and_delete_on_the_computer_side: Rename posts the old and the new name',
          form is True and bool(got) and got[0][1] == {'from': 'Mine', 'to': 'Mine two'}, got)
    listed = poll(lambda: b.page("[...document.querySelectorAll('[data-act=cpreset]')].some((p) => p.dataset.name === 'Mine two')"), 10)
    check('presets_save_rename_and_delete_on_the_computer_side: and the list says the new name', listed is True)
    forget(b)
    tap(b, '#sg-cpreset-delete')
    if b.page("!!document.querySelector('.sg-list')"):
        tap(b, '.sg-list .sg-opt[data-value="Mine two"]')
    asked = poll(lambda: b.page("!!document.querySelector('#sg-cpresets .sg-askbox [data-answer=delete]')"))
    check('presets_save_rename_and_delete_on_the_computer_side: Delete asks first, and nothing is sent before the answer',
          asked is True and posts(b, 'api/v1/devices/Gungnir%202/presets/delete') == [])
    tap(b, '#sg-cpresets .sg-askbox [data-answer="delete"]')
    got = poll(lambda: posts(b, 'api/v1/devices/Gungnir%202/presets/delete'))
    check('presets_save_rename_and_delete_on_the_computer_side: yes deletes it',
          bool(got) and got[0][1] == {'name': 'Mine two'}, got)
    gone = poll(lambda: b.page("![...document.querySelectorAll('[data-act=cpreset]')].some((p) => p.dataset.name === 'Mine two')"), 10)
    check('presets_save_rename_and_delete_on_the_computer_side: and it leaves the list', gone is True)


def wheel_q(b, d):
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
    check('the_wheel_sets_the_q: two notches up narrow the Q, ended by a done',
          q1 > q0 and bool(log) and log[-1][1].get('done') is True and all('q' in p[1] for p in log[:-1]), [q0, q1, log])
    wheel(b, s['plot'][0] + 3, s['plot'][1] + 3, 120)
    check('the_wheel_sets_the_q: the wheel away from a dot is the page\'s',
          b.page(f'{POINTS}[3].q') == q1 and len(posts(b, 'api/v1/host/eq/points/4')) == len(log))


def sixteen(b, d):
    load(b, d)
    flip(b, 'computer')
    seed(b, 7)
    seven = state(b)
    page7 = b.page('document.documentElement.scrollHeight')
    seed(b, 16)
    sixteen_ = state(b)
    check('sixteen_points_keep_the_layout: sixteen rows, the table still shows six and says 10 more',
          len(sixteen_['rows']) == 16 and sixteen_['more'] == '10 more below ↓', [len(sixteen_['rows']), sixteen_['more']])
    check('sixteen_points_keep_the_layout: the points, the shelf and the open area are as tall as with seven (1 px)',
          abs(seven['height'] - sixteen_['height']) <= 1 and abs(seven['shelf'] - sixteen_['shelf']) <= 1
          and abs(seven['open'] - sixteen_['open']) <= 1, [seven['height'], sixteen_['height'], seven['shelf'], sixteen_['shelf']])
    check('sixteen_points_keep_the_layout: nor does the page grow', b.page('document.documentElement.scrollHeight') == page7,
          [page7, b.page('document.documentElement.scrollHeight')])
    check('sixteen_points_keep_the_layout: the line counts them and + Add point is off',
          sixteen_['line'].startswith('16 of 16 points')
          and b.page("document.querySelector('#sg-cadd').getAttribute('aria-disabled')") == 'true')
    forget(b)
    tap(b, '#sg-cadd')
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
    b.sleep(0.5)
    before = posts(b, 'api/v1/host/eq/points/3')
    live = [p for p in before if p[1].get('done') is False]
    check('a_flip_mid_drag_ends_it_with_one_final_value: the drag started: live posts went out before the flip',
          len(live) >= 2 and all(p[1].get('done') is False for p in before), before)
    forget(b)
    b.page("document.querySelector('#sg-line [data-hop=\"device:Gungnir%202\"]').click()")
    b.sleep(1.0)
    release(b, x + 30, y - 20)
    b.sleep(1.0)
    after = posts(b)
    done = [p for p in after if p[0] == 'api/v1/host/eq/points/3' and p[1].get('done') is True]
    last = {k: v for p in live for k, v in p[1].items() if k in ('f', 'g', 'q')}
    check('a_flip_mid_drag_ends_it_with_one_final_value: nothing but the drag\'s own point was written after the flip',
          bool(after) and all(p[0] == 'api/v1/host/eq/points/3' for p in after), after)
    check('a_flip_mid_drag_ends_it_with_one_final_value: exactly one done, carrying the last value',
          len(done) == 1 and all(done[0][1].get(k) == v for k, v in last.items() if k in done[0][1]) and len(after) == 1, [done, last])
    check('a_flip_mid_drag_ends_it_with_one_final_value: the drag ends at once: no label, the DAC side shown, the area not held',
          b.page("document.querySelectorAll('.sg-dlab').length") == 0 and b.page('__core.computer.side()') == 'dac'
          and b.page("!!document.querySelector('#sg-graph') && !document.querySelector('#sg-cgraph')"))
    b.page("document.querySelector('#sg-line [data-hop=\"computer\"]').click()")
    b.sleep(1.0)
    ok_ = state(b)
    check('a_flip_mid_drag_ends_it_with_one_final_value: back on the computer the area draws from the model',
          ok_['rows'][2]['cells'][1].endswith('Hz') and len(ok_['marks']) == 5, ok_['rows'][2])


def guest(b, d):
    load(b, d)
    check('a_guest_sees_the_computer_inert: the guest has the hop',
          b.page(f"!!document.querySelector('{HOP}')"))
    flip(b, 'computer')
    inert = b.page("[...document.querySelectorAll('#sg-open [data-act], #sg-shelf [data-act], #sg-shelf select')]"
                   ".filter((e) => e.getAttribute('aria-disabled') !== 'true' && !e.disabled).map((e) => e.id)")
    check('a_guest_sees_the_computer_inert: every computer control is inert', inert == [], inert)
    check('a_guest_sees_the_computer_inert: the track too',
          b.page("document.querySelector('#sg-cvtrack').getAttribute('aria-disabled')") == 'true')
    forget(b)
    for sel in ('#sg-cvup', '#sg-cm-eq', '#sg-cadd', '[data-act="cpreset"][data-name="Warm"]'):
        tap(b, sel)
    s = state(b)
    x, y = s['marks']['1']
    gesture(b, x, y, [(x + 30, y - 20)])
    b.click(s['plot'][0] + 40, s['plot'][1] + 20)
    time.sleep(0.5)
    check('a_guest_sees_the_computer_inert: presses and drags write nothing', posts(b) == [], posts(b))
    check('a_guest_sees_the_computer_inert: a press says why',
          b.page("document.querySelectorAll('#toasts .toast').length") > 0)


def kept(b, d):
    load(b, d)
    flip(b, 'computer')
    check('the_side_survives_a_reload: the side is kept in the browser',
          b.page("localStorage.getItem('svolder.side')") == 'computer')
    b.goto(f'{d.url}?theme=signal&effects=off', settle=1.0)
    b.page(BOOT)
    poll(lambda: b.page("!!document.querySelector('#sg-cvtrack')"))
    time.sleep(0.8)
    check('the_side_survives_a_reload: the computer\'s hop is chosen after the load and its area drawn',
          b.page(f"document.querySelector('{HOP}').getAttribute('aria-selected')") == 'true'
          and b.page("!!document.querySelector('#sg-cvtrack') && !!document.querySelector('#sg-cgraph')") is True)
    nums = b.page('window.__nums')
    check('the_side_survives_a_reload: the first paint is a dash or the level read, never a live 0 %',
          bool(nums) and '0' not in nums and set(nums) <= {'—', str(b.page(f'{COMPUTER}.volume.pct'))}, nums)
    check('the_side_survives_a_reload: the load wrote nothing', posts(b) == [], posts(b))
    # the device's hop chosen by hand is the side kept as the device's
    flip(b, 'dac')
    b.goto(f'{d.url}?theme=signal&effects=off', settle=1.0)
    b.page(BOOT)
    time.sleep(0.8)
    check('the_side_survives_a_reload: and a load with the device kept chooses the device\'s hop',
          b.page("document.querySelector('#sg-line [aria-selected=true]').dataset.hop") == 'device:Gungnir%202')


def both_sides(b, d):
    load(b, d)
    status, _ = d.post('Gungnir 2', 'presets/recall', {'name': 'Warm'})
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the DAC plays Warm', status == 200, status)
    time.sleep(1.0)
    playing = lambda: b.page(  # noqa: E731
        "JSON.stringify(__core.model.device().presets?.rows?.filter((r) => r.on).map((r) => r.name))")
    flip(b, 'computer')
    forget(b)
    tap(b, '[data-act="cpreset"][data-name="Warm"]')
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
    tap(b, '[data-act="cpreset"][data-name="Warm"]')
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
    poll(lambda: b.page("!!document.querySelector('#sg-line [data-hop=\"device:Amp\"][aria-selected=true]')"))
    time.sleep(1.2)
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: the amp\'s line has the computer\'s hop',
          b.page(f"!!document.querySelector('{HOP}')") and b.page(f"document.querySelector('{HOP} b').textContent") == 'This computer')
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: its own area first',
          b.page("!!document.querySelector('#sg-open .sg-amp, #sg-amp-meters, #sg-amp-power') && !document.querySelector('#sg-cvtrack')"))
    tap(b, HOP)
    time.sleep(0.9)
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: the hop says its controls are below while chosen',
          'controls below' in b.page(f"document.querySelector('{HOP}').innerText"))
    parts = b.page("({ volume: !!document.querySelector('#sg-cvolume #sg-cvtrack'), tone: !!document.querySelector('#sg-ctone #sg-cgraph'),"
                   " presets: !!document.querySelector('#sg-cpresets'), mode: !!document.querySelector('#sg-cmode'),"
                   " rows: document.querySelectorAll('#sg-points .sg-prow').length,"
                   " text: document.querySelector('#sg-cbig').textContent.trim(), amp: !!document.querySelector('#sg-amp-meters') })")
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: the DAC page\'s level, curve, mode, points and presets',
          parts['volume'] and parts['tone'] and parts['presets'] and parts['mode'] and parts['rows'] >= 1
          and parts['text'].replace(' ', '').endswith('%') and not parts['amp'], parts)
    forget(b)
    tap(b, '#sg-cvup')
    got = poll(lambda: posts(b, 'api/v1/host/volume'))
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: and they write the computer\'s volume',
          len(got) == 1, got)
    tap(b, '#sg-line [data-hop="device:Amp"]')
    check('the_amp_page_flips_to_the_computer_with_the_dac_parts: another hop is the amp\'s page again',
          b.page("!!document.querySelector('#sg-amp-meters, #sg-amp-power') && !document.querySelector('#sg-cvtrack')"))


def wire(b, d):
    load(b, d)
    check('the_wire_still_shows_the_eq_changing_the_sound: the route changes the sound, the computer\'s stretch is yellow',
          poll(lambda: b.page("!!document.querySelector('.sg-wire .sg-changes')")) is True)
    words = b.page(f"document.querySelector('{HOP}').innerText")
    check('the_wire_still_shows_the_eq_changing_the_sound: and its hop says so while another hop is chosen',
          'changes the sound' in words, words)
    flip(b, 'computer')
    check('the_wire_still_shows_the_eq_changing_the_sound: with the computer chosen the stretch stays yellow',
          b.page("!!document.querySelector('.sg-wire .sg-changes')") is True)
    words = b.page(f"document.querySelector('{HOP}').innerText")
    check('the_wire_still_shows_the_eq_changing_the_sound: and the hop says its controls are below',
          'controls below' in words, words)
    check('the_wire_still_shows_the_eq_changing_the_sound: the facts name the other streams mixing in',
          b.page("/2 other streams mix in/.test(document.querySelector('#sg-cfacts').innerText)") is True,
          b.page("document.querySelector('#sg-cfacts').innerText"))
    check('the_wire_still_shows_the_eq_changing_the_sound: no word of the facts says computer, whatever the path says',
          b.page("/computer/i.test(document.querySelector('#sg-cfacts').innerText + document.querySelector('#sg-cfacts').innerHTML)") is False,
          b.page("document.querySelector('#sg-cfacts').innerText"))
    flip(b, 'dac')


def unavailable(b, d):
    load(b, d, query='?theme=signal&effects=off')
    b.page("localStorage.setItem('svolder.side', 'computer')")
    b.goto(f'{d.url}?theme=signal&effects=off', settle=1.5)
    b.page(BOOT)
    time.sleep(1.0)
    check('a_load_with_the_computer_kept_but_unavailable_shows_the_dac: the DAC\'s own screen is drawn',
          b.page("!!document.querySelector('#sg-vtrack') && !document.querySelector('#sg-cvtrack')") is True)
    words = b.page(f"document.querySelector('{HOP}').innerText")
    check('a_load_with_the_computer_kept_but_unavailable_shows_the_dac: and the computer\'s hop says why',
          'computer controls unavailable' in words and 'controls below' not in words, words)
    check('a_load_with_the_computer_kept_but_unavailable_shows_the_dac: the DAC\'s hop is the one chosen',
          b.page("document.querySelector('#sg-line [aria-selected=true]').dataset.hop") == 'device:Gungnir%202')


def touch():
    """A finger on the curve (Chromium, touch emulation, a phone)."""
    d = common.Daemon('gungnir2-v2', host='fake', env={})
    b = common.Chromium(size=(390, 844), dpr=2, touch=True)
    b.preload(HOOK)
    try:
        load(b, d)
        flip(b, 'computer')
        seed(b, 4)
        b.page("document.querySelector('#sg-cgraph').scrollIntoView({ block: 'center' })")
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
        check('a_finger_adds_drags_and_pinches: a finger on a dot drags it, done at the lift, no scroll',
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
        check('a_finger_adds_drags_and_pinches: the phone\'s mode ends the first screen, the table is under the curve',
              b.page("!!document.querySelector('#sg-open #sg-cmode') && !document.querySelector('#sg-shelf #sg-cmode')") is True)
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
        for name, size, dpr in (('desktop', (1280, 1010), 1), ('phone', (390, 844), 2)):
            b = common.Chromium(size=size, dpr=dpr)
            b.preload(HOOK)
            try:
                load(b, d)
                flip(b, 'computer')
                seed(b, 7)
                b.page("window.scrollTo(0, 0)")
                time.sleep(0.8)
                shot = f'{SHOTS}/computer-{name}.png'
                with open(shot, 'wb') as f:
                    f.write(b.shot())
                print('shot', shot)
                if name == 'phone':
                    b.page("document.querySelector('#sg-points').scrollIntoView({ block: 'start' })")
                    b.page("window.scrollBy(0, -110)")
                    time.sleep(0.8)
                    shot = f'{SHOTS}/computer-phone-table.png'
                    with open(shot, 'wb') as f:
                        f.write(b.shot())
                    print('shot', shot)
            finally:
                b.close()
    finally:
        d.stop()


one('the_hop_flips_volume_and_tone_together', hop_flip)
one('a_keyboard_flip_puts_the_focus_on_the_chosen_hop', keyboard)
one('minus_plus_mute_set_the_computer_volume', volume)
one('a_point_drag_posts_live_and_done_at_release', drag)
one('a_tap_adds_a_point_and_x_removes_it', add_remove)
one('the_mode_the_type_a_typed_value_and_the_presets_write', controls)
one('an_untouched_cell_writes_nothing', untouched)
one('an_unread_volume_writes_nothing', unread_volume)
one('the_points_are_listed_by_frequency_and_the_selection_follows', sorted_points)
one('presets_save_rename_and_delete_on_the_computer_side', preset_edits)
one('the_wheel_sets_the_q', wheel_q)
one('sixteen_points_keep_the_layout', sixteen)
if browser == 'chromium':
    one('a_flip_mid_drag_ends_it_with_one_final_value', mid_drag)
one('a_guest_sees_the_computer_inert', guest, web='guest = "read"')
one('the_side_survives_a_reload', kept)
one('the_same_preset_on_both_asks_and_yes_flattens_the_dac', both_sides)
one('the_amp_page_flips_to_the_computer_with_the_dac_parts', amp)
one('a_load_with_the_computer_kept_but_unavailable_shows_the_dac', unavailable, env={'SVOLDER_FAKE_HOST_EQ': 'off'})
one('the_wire_still_shows_the_eq_changing_the_sound', wire, env={'SVOLDER_FAKE_ROUTE': 'changed'})
if browser == 'chromium':
    touch()
    pictures()
print(f'computer_signal.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
