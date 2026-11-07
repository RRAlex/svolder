# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive's computer side in a real page (design: the computer flip), through the page's own
controls, on a fake host (`SVOLDER_HOST=fake`: its sink and EQ live in memory, no PipeWire, no sound):
  the_two_keys_flip_volume_and_tone_together   two keys by the menu button (the device's name, This
                                        computer) flip the volume figure, the dash's keys and the EQ line
  minus_plus_mute_set_the_computer_volume   `-` `+` step 1 %, a held one repeats, mute keeps the number
  a_point_drag_posts_live_and_done_at_release   a drag of a bubble on the mountains posts `done: false`
                                        while it moves (at most 20 a second) and one `done: true`
  every_dot_is_on_the_curve             each bubble stands on the drawn curve, a shelf's and a held one too
  a_tap_adds_a_point_and_x_removes_it   a tap on an empty spot of the mountains (the curve up) adds a PK, the
                                        sheet's `x` removes a point, `+ Add point` adds one at the widest gap
  the_mode_the_type_a_typed_value_and_the_presets_write   Off | EQ, the type, a value typed into a field, a preset
  the_wheel_and_the_arrows_step_the_volume   the wheel over the big number and ↑ ↓ step the volume (the device's step; 1 % on the
                                        computer), held arrows repeat, none with a control focused or a sheet or question open
  the_switch_keys_never_ellipsize_this_computer   `This computer` shows whole from 600 px up, upright or sideways
  the_wheel_sets_the_q                  a notch over a bubble steps its Q, ended by a done
  sixteen_points_keep_the_layout        the sheet is as tall with 16 points as with 7
  a_flip_mid_drag_writes_nothing        a flip ends the drag; only that drag's own last value is written
  a_guest_sees_the_computer_inert       a guest may flip and every computer control is inert
  the_side_survives_a_reload            the side kept is the one drawn after a load; no live 0 %, no write
  the_mountains_draw_the_curve_dimmed_while_the_eq_is_off   the curve and bubbles at 45 % with the EQ off, dots still on it
  a_flip_ends_a_wheel_burst_with_one_done   the burst's timer is cancelled by the flip
  the_keys_keep_the_focus_across_a_keyboard_flip   the key chosen keeps the focus
  presets_save_rename_delete_from_the_computer_side   the same actions as the DAC's
  the_same_preset_on_both_asks_and_yes_flattens_the_dac   the question; no sends nothing, yes confirms
  the_amp_page_flips_to_the_computer_with_the_dac_parts   an amp's page, flipped, has the DAC parts
  the_fade_keeps_the_switch_keys_as_the_menu_button   the dash fades, the keys by the menu button stay (a phone has them as a pill in the top left corner); a tap on a key flips at once
  the_right_column_runs_keys_then_words_then_the_eq_line   the words on the right of the top line stand under the keys,
                                        the EQ line under them; nothing meets
  the_phone_panel_starts_with_the_switch   the switch as a `DAC | PC` pill in the scene's top left corner (not the panel), the scene over 77 %, the figure in the scene
  a_finger_adds_drags_and_pinches       (Chromium, touch emulation) a tap adds, a finger drags, two fingers halve the Q
Pictures of the computer side (desktop and phone, the sheet shut and open) are saved under target/web-shots/nightdrive/
(Chromium run). Shared helpers: nightdrive_base.py.
Usage: python3 computer_nightdrive.py chromium|firefox"""
import os
import re

os.environ.setdefault('SVOLDER_FAKE_COMPUTER_VOLUME', '42')

import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)


def bare(body):
    """A point write's body without its `slot` (which names the point and is checked on its own)."""
    return {k: v for k, v in body.items() if k != 'slot'}

SHOTS = f'{common.REPO}/target/web-shots/nightdrive'
FLEET = [('fake-amp', 'Amp', '1112131415161718')]
QUERY = '?theme=nightdrive&effects=off'
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (!(a[1] && a[1].method === 'POST')) return f(...a);"
        " const entry = [String(a[0]), a[1].body, null, performance.now()]; window.__posts.push(entry);"
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };")
COMPUTER = '__core.model.page().host?.computer'
POINTS = f'{COMPUTER}.eq.points'
TUNE = '__core.theme.driver.tune()'
# the map's window (tune.js): 0 dB at H - BASE v, the gain range SPAN v above and below it
WINDOW_BASE, WINDOW_SPAN = 0.08, 0.14
TUNE_ON = "document.querySelector('.nd-scene').dataset.tune === 'on'"
CARD = "document.querySelector('.ask')"
SHEET = "!!document.getElementById('nd-dialog')"
# the plan of a non-flat curve: a low shelf, peaks of both signs and a high shelf
PLAN = [('LSC', 105, 5.5, 0.7), ('PK', 180, -2.1, 0.9), ('PK', 1200, -1.8, 1.4), ('PK', 2900, 3.2, 2.0),
        ('PK', 5600, -4.0, 3.1), ('PK', 8900, 2.4, 2.5), ('HSC', 10000, -1.5, 0.7)]
EXTRA = [('PK', round(40 * 1.28 ** i), 1.0 + (i % 3), 1.0) for i in range(9)]


def page_errors(b):
    """The page's errors, but for the console's lines about the refusal the question check makes on purpose."""
    return [e for e in errors(b) if 'status of 409' not in e]


def open_page(d, size=(1280, 900), **kw):
    b = make(size, **kw)
    b.preload(HOOK)
    return b


def start(b, d, query=QUERY):
    """A page on the shown device with the computer read; the kept side is forgotten first."""
    forget(b, d)
    load(b, d, query)
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    time.sleep(0.5)
    b.page('window.__posts.length = 0')


def posts(b, route=None):
    log = b.page('window.__posts.map((p) => [p[0], JSON.parse(p[1] || "null"), p[2], p[3]])')
    return [p for p in log if route is None or p[0].startswith(route)]


def clear(b):
    b.page('window.__posts.length = 0')


def tap(b, selector):
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.15)
    click_el(b, selector)
    time.sleep(0.4)


def tap_synth(b, selector):
    """A touch's events on the element: the pointer goes down and up and a click follows, with no move before."""
    synth(b, selector, 'pointerdown')
    synth(b, selector, 'pointerup', buttons=0)
    synth(b, selector, 'click')
    time.sleep(0.3)


def flip(b, side):
    tap(b, f'#nd-side-{side}')
    poll(lambda: b.page('__core.computer.side()') == side)
    time.sleep(0.3)


def seed(b, count):
    """The EQ holds `count` points (a spread with the two shelves), through the actions."""
    have = b.page(f'{POINTS}.length')
    for kind, f, g, q in (PLAN + EXTRA)[have - 1:count - 1]:
        b.page(f"__core.actions.computer.addPoint({{ type: {json.dumps(kind)}, f: {f}, g: {g}, q: {q} }}).then(() => 1)")
        time.sleep(0.15)
    poll(lambda: b.page(f'{POINTS}.length') >= count)
    time.sleep(0.5)
    clear(b)


def open_sheet(b):
    if not b.page(SHEET):
        click_el(b, '#nd-sheet')
        poll(lambda: b.page(SHEET), 3.0)
        time.sleep(0.4)


def close_sheet(b):
    if b.page(SHEET):
        click_el(b, '#nd-sheet-x')
        poll(lambda: not b.page(SHEET), 3.0)
        time.sleep(0.3)


# ------------------------------------------------------------------------------ pointer input

def mouse_chromium(b, kind, x, y, button='left', buttons=0, **extra):
    b.send('Input.dispatchMouseEvent', {'type': kind, 'x': x, 'y': y, 'button': button,
                                        'buttons': buttons, 'clickCount': 1, **extra})


def hover(b, x, y):
    if browser == 'chromium':
        mouse_chromium(b, 'mouseMoved', x, y)
    else:
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': [
            {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)}]}]})


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


def gesture(b, x, y, to, step=4):
    path = path_to(x, y, to, step)
    if browser == 'chromium':
        press(b, x, y)
        for mx, my in path:
            move(b, mx, my)
        release(b, *path[-1])
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)},
                {'type': 'pointerDown', 'button': 0}]
        for mx, my in path:
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(mx), 'y': int(my)})
        acts.append({'type': 'pointerUp', 'button': 0})
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


def curve_up(b):
    """Move the pointer over the mountains until the curve is up (the morph past one half); the bubbles
    `{band: (x, y)}` in viewport px, and the tune's reading."""
    t = b.page(TUNE)
    box = b.page("(() => { const r = document.querySelector('.nd-scene').getBoundingClientRect(); return [r.left, r.top]; })()")
    x, y = t['w'] * 0.5, t['H'] - 0.04 * t['h']
    for i in range(30):
        hover(b, x + (i % 2) * 3, y)
        time.sleep(0.07)
        t = b.page(TUNE)
        if t['m'] >= 0.6:
            break
    return {bub['band']: (box[0] + bub['x'], box[1] + bub['y']) for bub in t['bubbles']}, t, box


def empty_spot(marks, t, box):
    """A place on the mountains' band at least 40 px from every bubble."""
    for fx in (0.12, 0.3, 0.45, 0.62, 0.78, 0.9):
        for dy in (0.0, 0.05, -0.05, 0.08, -0.08):
            x, y = box[0] + fx * t['w'], box[1] + t['H'] - 0.08 * t['h'] + dy * t['h']
            if all(math.hypot(x - mx, y - my) > 40 for mx, my in marks.values()):
                return x, y
    return None


# ------------------------------------------------------------------------------------- the cases

VOLUME_ARIA = "document.querySelector('.nd-vol svg')?.getAttribute('aria-label')"
KEYS = "[...document.querySelectorAll('#nd-switch .nd-k')].map((k) => k.textContent)"


def the_keys(b, d):
    reset_bands(d)
    start(b, d)
    name = 'the_two_keys_flip_volume_and_tone_together'
    check(f'{name}: two keys by the menu button, the DAC pressed first',
          b.page(KEYS) == ['Gungnir 2', 'This computer']
          and b.page("document.querySelector('#nd-switch .nd-k[aria-pressed=true]').textContent") == 'Gungnir 2')
    geo = b.page("""(() => { const r = (s) => { const b = document.querySelector(s).getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; };
      return { keys: r('#nd-switch .nd-swkeys'), button: r('#fx-button'), eq: document.querySelector('.nd-eq') ? r('.nd-eq') : null }; })()""")
    k, m = geo['keys'], geo['button']
    check(f'{name}: they stand left of the menu button, on its row',
          k[2] < m[0] and abs((k[1] + k[3]) / 2 - (m[1] + m[3]) / 2) < 22, geo)
    check(f'{name}: the DAC side shows the device\'s own volume in dB and its bands\' gains on the EQ line',
          'dB' in (b.page(VOLUME_ARIA) or '') and b.page("!!document.querySelector('.nd-eq .nd-gains')"))
    flip(b, 'computer')
    pct = b.page(f'{COMPUTER}.volume.pct')
    check(f'{name}: the figure reads the computer, in %', b.page(VOLUME_ARIA) == f'Volume {pct} %', [b.page(VOLUME_ARIA), pct])
    eq = b.page("document.querySelector('.nd-eq').textContent.trim()")
    check(f'{name}: the EQ line reads EQ (or OFF) and the preset, with no gains list',
          eq.split()[0] in ('EQ', 'OFF') and not b.page("!!document.querySelector('.nd-eq .nd-gains')"), eq)
    check(f'{name}: the dash keeps its keys and the switch shows the computer',
          b.page("[...document.querySelectorAll('#nd-dash [data-act]')].slice(0, 3).map((e) => e.dataset.act)") == ['volume-down', 'mute', 'volume-up']
          and b.page("document.querySelector('#nd-switch .nd-k[aria-pressed=true]').textContent") == 'This computer')
    check(f'{name}: a flip posts nothing', posts(b) == [], posts(b))
    open_sheet(b)
    sheet = b.page("document.getElementById('nd-dialog').textContent")
    heads = b.page("[...document.querySelectorAll('#nd-dialog .nd-lbl')].map((e) => e.textContent)")
    check(f'{name}: the sheet runs Mode, then Points, with Off | EQ',
          'Points' in heads and heads.index('Mode') < heads.index('Points') and b.page("[...document.querySelectorAll('#nd-mode .nd-chip')].map((e) => e.textContent)") == ['Off', 'EQ'], heads)
    check(f'{name}: nothing below the switch says computer',
          b.page("""(() => { const own = [...document.querySelectorAll('#nd-layer > :not(.nd-hud), #nd-layer > .nd-hud > :not(#nd-switch), #nd-dialog')];
            const words = own.flatMap((e) => [e.textContent, ...[...e.querySelectorAll('[aria-label], [title]')].map((x) => x.getAttribute('aria-label') || x.title)]);
            return words.some((w) => /computer/i.test(w)); })()""") is False, sheet[:200])
    close_sheet(b)
    flip(b, 'dac')
    check(f'{name}: and back to the device\'s own parts, again without a write',
          'dB' in (b.page(VOLUME_ARIA) or '') and b.page("!!document.querySelector('.nd-eq .nd-gains')") and posts(b) == [], posts(b))


def volume(b, d):
    start(b, d)
    flip(b, 'computer')
    start_pct = b.page(f'{COMPUTER}.volume.pct')
    name = 'minus_plus_mute_set_the_computer_volume'
    tap(b, '#nd-vup')
    got = poll(lambda: posts(b, 'api/v1/host/volume'))
    check(f'{name}: + posts one step of 1 %', [p[1] for p in got] == [{'pct': start_pct + 1}], got)
    check(f'{name}: and the figure shows it', poll(lambda: b.page(VOLUME_ARIA) == f'Volume {start_pct + 1} %') is True, b.page(VOLUME_ARIA))
    tap(b, '#nd-vdown')
    tap(b, '#nd-vdown')
    time.sleep(0.6)
    check(f'{name}: two steps down read 2 % lower', b.page(f'{COMPUTER}.volume.pct') == start_pct - 1, b.page(f'{COMPUTER}.volume'))
    clear(b)
    tap(b, '#nd-mute')
    got = poll(lambda: posts(b, 'api/v1/host/mute'))
    check(f'{name}: mute posts muted', [p[1] for p in got] == [{'muted': True}], got)
    time.sleep(0.5)
    check(f'{name}: the key is lit and the figure keeps the number, saying muted',
          b.page("document.getElementById('nd-mute').getAttribute('aria-pressed')") == 'true'
          and b.page(VOLUME_ARIA) == f'Volume {start_pct - 1} %, muted', b.page(VOLUME_ARIA))
    tap(b, '#nd-mute')
    time.sleep(0.5)
    check(f'{name}: unmute', b.page(f'{COMPUTER}.volume.muted') is False)
    clear(b)
    x, y = common.center(b, '#nd-vup')
    before = b.page(f'{COMPUTER}.volume.pct')
    hold(b, x, y, 1.3)
    after = b.page(f'{COMPUTER}.volume.pct')
    check(f'{name}: a held + repeats, a step at a time up to the end', before + 4 <= after <= 100, [before, after])
    last = posts(b, 'api/v1/host/volume')
    check(f'{name}: the last post is the level reached, never above 100',
          bool(last) and last[-1][1]['pct'] == after and all(p[1]['pct'] <= 100 for p in last), last[-3:])


def drag(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 7)
    name = 'a_point_drag_posts_live_and_done_at_release'
    marks, t, box = curve_up(b)
    check(f'{name}: seven bubbles on the mountains, the curve up', len(marks) == 7 and t['m'] >= 0.5, [len(marks), t['m']])
    x, y = marks[4]
    during = None
    if browser == 'chromium':
        press(b, x, y)
        for mx, my in path_to(x, y, [(x + 40, y - 30)]):
            move(b, mx, my)
        during = b.page(TUNE)
        release(b, x + 40, y - 30)
        b.sleep(1.2)
    else:
        gesture(b, x, y, [(x + 40, y - 30)])
    log = posts(b, 'api/v1/host/eq/points/4')
    flags = [p[1].get('done') for p in log]
    check(f'{name}: live updates, then one done at the release',
          len(log) >= 2 and flags[-1] is True and all(f is False for f in flags[:-1]), flags)
    gaps = [round(q[3] - p[3]) for p, q in zip(log[:-2], log[1:-1])]
    check(f'{name}: at most 20 a second (the live ones 45 ms apart or more)', len(gaps) >= 2 and min(gaps) >= 45, gaps)
    held = [p[1] for p in log]
    check(f'{name}: the posts carry the point\'s frequency and gain', any('f' in p for p in held) and any('g' in p for p in held), held[:4])
    t = b.page(TUNE)
    cx = next(bub['x'] for bub in t['bubbles'] if bub['band'] == 4) + box[0]
    check(f'{name}: the bubble sits on the release\'s frequency (6 px)', abs(cx - (x + 40)) <= 6, [cx - x - 40])
    point = b.page(f'{POINTS}[3]')
    check(f'{name}: the daemon holds the last value sent', abs(point['g'] - held[-1].get('g', point['g'])) < 0.01 and point['f'] != 2900, [point, held[-1]])
    if during:
        check(f'{name}: while it moves a label shows the three values and the bubble is held',
              bool(re.match(r'^[\d.]+ Hz · [+−]?[\d.]+ dB · Q [\d.]+$', during['label'])) and during['held'] == 4, during['label'])
    check(f'{name}: nothing is left held afterwards', b.page(TUNE)['held'] is None and b.page("!document.querySelector('.nd-label')"))


def dots(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 7)
    name = 'every_dot_is_on_the_curve'
    marks, t, box = curve_up(b)

    def off_curve():
        # one task of the page reads the bubbles and the curve, so a drag in motion cannot move one between them
        read = b.page("""(() => { const d = __core.theme.driver; const t = d.tune();
          return t.bubbles.map((bub) => [bub.band, bub.y, d.curveAt(bub.x)]); })()""")
        return [(band, y, curve) for band, y, curve in read if curve is None or abs(y - curve) > 0.6], read
    bad, read = off_curve()
    check(f'{name}: all seven, the shelves too, stand on the drawn curve', not bad and len(read) == 7, bad)
    if browser == 'chromium':
        x, y = marks[4]
        press(b, x, y)
        bad_held = []
        seen = 0
        for mx, my in path_to(x, y, [(x + 60, y - 40)], step=6):
            move(b, mx, my)
            bad_off, read = off_curve()
            bad_held += bad_off
            seen += len(read)
        release(b, x + 60, y - 40)
        b.sleep(1.0)
        check(f'{name}: the held dot too, all along a drag (every bubble read at each move)', not bad_held and seen >= 7 * 5, [bad_held[:3], seen])
    seed(b, 7)
    bad, _ = off_curve()
    check(f'{name}: and after the writes are answered', not bad, bad)


def add_remove(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 3)
    name = 'a_tap_adds_a_point_and_x_removes_it'
    t = b.page(TUNE)
    x0, y0 = t['w'] * 0.55, t['H'] - 0.08 * t['h']
    b.click(x0, y0)
    time.sleep(0.5)
    check(f'{name}: a tap while the mountains are still the music adds nothing', posts(b, 'api/v1/host/eq/points') == [] and b.page(f'{POINTS}.length') == 3, posts(b))
    marks, t, box = curve_up(b)
    spot = empty_spot(marks, t, box)
    b.click(*spot)
    got = poll(lambda: posts(b, 'api/v1/host/eq/points'))
    add = got[0][1] if got else {}
    check(f'{name}: a tap with the curve up posts a PK of Q 1 where it was made',
          len(got) == 1 and add.get('type') == 'PK' and add.get('q') == 1 and 20 <= add.get('f', 0) <= 20000, got)
    poll(lambda: b.page(f'{POINTS}.length') == 4)
    time.sleep(0.5)
    marks, t, box = curve_up(b)
    check(f'{name}: the new bubble is at the tapped frequency (6 px)', any(abs(mx - spot[0]) <= 6 for mx, _ in marks.values()), [spot, marks])
    open_sheet(b)
    label = b.page("document.getElementById('nd-px-4').getAttribute('aria-label')")
    check(f'{name}: its row has an x labelled Remove point 4', label == 'Remove point 4', label)
    clear(b)
    tap(b, '#nd-px-4')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/4/remove'))
    check(f'{name}: the x removes it', len(got) == 1, got)
    check(f'{name}: three points again', poll(lambda: b.page(f'{POINTS}.length') == 3) is True)
    clear(b)
    tap(b, '#nd-point-add')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points'))
    freqs = sorted([20, 20000] + [p['f'] for p in b.page(POINTS)[:3]])
    gap = max(range(len(freqs) - 1), key=lambda i: math.log(freqs[i + 1] / freqs[i]))
    want = math.sqrt(freqs[gap] * freqs[gap + 1])
    check(f'{name}: + Add point adds a PK, 0 dB, Q 1 at the widest gap',
          len(got) == 1 and got[0][1]['g'] == 0 and got[0][1]['q'] == 1 and abs(got[0][1]['f'] - want) < 0.5, [got, want])
    time.sleep(0.6)
    for n in (4, 3, 2):
        tap(b, f'#nd-px-{n}')
        time.sleep(0.5)
    b.page("__core.actions.computer.setPoint(1, { g: 3 }, { done: true }).then(() => 1)")
    poll(lambda: b.page(f'{POINTS}[0].g') == 3)
    clear(b)
    tap(b, '#nd-px-1')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/1/remove'))
    flat = poll(lambda: b.page(f'{POINTS}[0].g') == 0)
    check(f'{name}: the lone point keeps its x, labelled Remove point 1, and it sets the point flat',
          len(got) == 1 and flat is True and b.page(f'{POINTS}.length') == 1
          and b.page("document.querySelector('#nd-px-1').getAttribute('aria-label')") == 'Remove point 1',
          [got, b.page(POINTS)])


def sorted_points(b, d):
    name = 'the_points_are_listed_by_frequency'
    start(b, d)
    flip(b, 'computer')
    seed(b, 4)
    open_sheet(b)
    fs = lambda: b.page(f'{POINTS}.map((p) => p.f)')
    first = fs()
    check(f'{name}: the daemon lists the points by frequency', first == sorted(first), first)
    b.page("__core.actions.computer.setPoint(1, { f: 19000 }, { done: false }).then(() => 1)")
    mid = poll(lambda: fs()[0] == 19000 and fs())
    check(f'{name}: mid-drag point 1 is still point 1 (nothing jumps under the finger)',
          mid and mid[1:] == first[1:], [first, mid])
    b.page("__core.actions.computer.setPoint(1, {}, { done: true }).then(() => 1)")
    want = sorted(first[1:] + [19000])
    check(f'{name}: at the release the table is in order', poll(lambda: fs() == want) is True, [first, fs()])
    time.sleep(0.6)
    rows = b.page("[...document.querySelectorAll('#nd-points-table tbody tr')].map((r) => r.dataset.point)")
    check(f'{name}: the # column counts the sorted order', rows == [str(i + 1) for i in range(len(first))], rows)
    last = len(first)
    shown = b.page(f"document.querySelector('#nd-p{last}f').value")
    check(f'{name}: the moved point is the last row', shown.replace(' ', '').lower().startswith('19'), shown)


def controls(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 4)
    open_sheet(b)
    name = 'the_mode_the_type_a_typed_value_and_the_presets_write'
    tap(b, '#nd-mode-off')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check(f'{name}: Off posts off', modes == [{'on': False}] and b.page(f'{COMPUTER}.eq.on') is False
          and b.page("document.getElementById('nd-mode-off').getAttribute('aria-pressed')") == 'true', modes)
    clear(b)
    tap(b, '#nd-mode-eq')
    time.sleep(0.6)
    modes = [p[1] for p in posts(b, 'api/v1/host/eq') if p[0] == 'api/v1/host/eq']
    check(f'{name}: EQ posts on', modes == [{'on': True}] and b.page(f'{COMPUTER}.eq.on') is True, modes)
    clear(b)
    b.page("(() => { const s = document.getElementById('nd-p2t'); s.value = 'HSC'; s.dispatchEvent(new Event('change', { bubbles: true })); })()")
    time.sleep(0.8)
    check(f'{name}: the type choice writes the shelf',
          b.page(f'{POINTS}[1].type') == 'HSC' and b.page("document.getElementById('nd-p2t').value") == 'HSC', b.page(f'{POINTS}[1]'))
    clear(b)
    click_el(b, '#nd-p3f')
    b.page("(() => { const e = document.getElementById('nd-p3f'); e.value = '2.5k'; return 1; })()")
    b.press('Enter')
    got = poll(lambda: posts(b, 'api/v1/host/eq/points/3'))
    check(f'{name}: 2.5k typed into a field is sent as 2500, done', bool(got) and bare(got[-1][1]) == {'f': 2500, 'done': True}, got)
    check('every_point_write_carries_its_slot: the typed value names its point by slot', bool(got) and isinstance(got[-1][1].get('slot'), int)
          and 1 <= got[-1][1]['slot'] <= 16, got)
    time.sleep(0.8)
    # the points are listed by frequency: the point typed to 2500 Hz is listed after the 1200 Hz one
    at = b.page(f"{POINTS}.findIndex((p) => p.f === 2500) + 1")
    check(f'{name}: the row shows it, where 2500 Hz lists it', at == 4 and b.page(f"document.getElementById('nd-p{at}f').value") == '2500 Hz',
          [at, b.page(f"document.getElementById('nd-p{at}f')?.value")])
    clear(b)
    click_el(b, f'#nd-p{at}g')
    b.page(f"(() => {{ const e = document.getElementById('nd-p{at}g'); e.value = '40'; return 1; }})()")
    b.press('Enter')
    got = poll(lambda: posts(b, f'api/v1/host/eq/points/{at}'))
    check(f'{name}: a gain beyond the hand range is held to 12', bool(got) and bare(got[-1][1]) == {'g': 12, 'done': True}, got)
    clear(b)
    click_el(b, f'#nd-p{at}q')
    b.page(f"(() => {{ const e = document.getElementById('nd-p{at}q'); e.value = 'loud'; return 1; }})()")
    b.press('Enter')
    time.sleep(0.5)
    check(f'{name}: words that are no number write nothing and say so',
          posts(b, f'api/v1/host/eq/points/{at}') == [] and b.page("[...document.querySelectorAll('#toasts .toast')].some((t) => t.dataset.text === 'Not a number.')"))
    tap(b, '#nd-presets [data-act="preset"][data-name="Warm"]')
    got = poll(lambda: posts(b, 'api/v1/host/eq/preset'))
    check(f'{name}: a preset recalled posts its name', bool(got) and got[0][1].get('name') == 'Warm', got)
    time.sleep(0.8)
    check(f'{name}: and is the one lit',
          b.page("document.querySelector('#nd-presets [data-name=Warm]').getAttribute('aria-pressed')") == 'true'
          and 'Warm' in b.page("document.querySelector('#nd-presets .nd-status').textContent"))
    off = b.page("[...document.querySelectorAll('#nd-presets [data-act=preset][aria-disabled=true]')].map((p) => p.dataset.name + ':' + (p.title || ''))")
    unplayable = b.page(f'{COMPUTER}.presets.filter((p) => !p.playable).length')
    check(f'{name}: an unplayable preset is disabled with its reason',
          len(off) == unplayable and not any(x.split(':')[1] == '' for x in off), [off, unplayable])


def untouched(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 4)
    open_sheet(b)
    name = 'an_untouched_cell_writes_nothing'
    b.page("(() => { const h = __core.model.page().host; const pts = h.computer.eq.points.map((p, i) => i === 2 ? { ...p, g: 14 } : p);"
           " __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } }); })()")
    time.sleep(0.6)
    clear(b)
    shown = b.page("document.getElementById('nd-p3g').value")
    click_el(b, '#nd-p3g')
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: Enter on a cell as it was posts nothing (a +14 dB point stays)',
          '14' in (shown or '') and posts(b) == [], [shown, posts(b)])
    click_el(b, '#nd-p3g')
    b.page("(() => { const e = document.getElementById('nd-p3g'); e.value = '14'; return 1; })()")
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: the same number typed again posts nothing', posts(b) == [], posts(b))


def wheel_q(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 4)
    name = 'the_wheel_sets_the_q'
    marks, t, box = curve_up(b)
    x, y = marks[4]
    q0 = b.page(f'{POINTS}[3].q')
    clear(b)
    wheel(b, x, y, -120)
    wheel(b, x, y, -120)
    time.sleep(0.8)
    log = posts(b, 'api/v1/host/eq/points/4')
    q1 = b.page(f'{POINTS}[3].q')
    check(f'{name}: two notches up narrow the Q, ended by a done',
          q1 > q0 and len(log) >= 2 and log[-1][1].get('done') is True and all('q' in p[1] for p in log[:-1]), [q0, q1, log])
    check(f'{name}: the frequency and gain did not move', len(log) >= 2 and not any('f' in p[1] or 'g' in p[1] for p in log), log[:3])


def key_held(b, name, secs):
    """The key `name` down for `secs` seconds (the page does its own repeating), then up."""
    if browser == 'chromium':
        key, code, vk, _ = common.KEYS[name]
        base = {'key': key, 'code': code, 'windowsVirtualKeyCode': vk, 'nativeVirtualKeyCode': vk}
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyDown'})
        b.sleep(secs)
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyUp'})
    else:
        k = common.WD_KEYS[name]
        b._post('/actions', {'actions': [{'type': 'key', 'id': 'kb', 'actions': [
            {'type': 'keyDown', 'value': k}, {'type': 'pause', 'duration': int(secs * 1000)}, {'type': 'keyUp', 'value': k}]}]})
    b.sleep(0.6)


def from_the_scene(b, d):
    """The wheel over the big volume number and the arrow keys step the volume, as `-` and `+` do."""
    start(b, d)
    name = 'the_wheel_and_the_arrows_step_the_volume'
    raw = lambda: b.page("__core.model.part('volume').raw")
    pct = lambda: b.page(f'{COMPUTER}.volume.pct')
    figure = lambda: b.page("""(() => { const r = document.querySelector('.nd-vol .nd-odb').getBoundingClientRect();
      return [(r.left + r.right) / 2, (r.top + r.bottom) / 2]; })()""")
    r0 = raw()
    x, y = figure()
    wheel(b, x, y, -120)
    check(f'{name}: a notch up over the number is one step louder (the device\'s own)', poll(lambda: raw() == r0 + 1) is True, [r0, raw()])
    wheel(b, x, y, 120)
    check(f'{name}: a notch down is one step quieter', poll(lambda: raw() == r0) is True, [r0, raw()])
    wheel(b, 200, 500, -120)
    check(f'{name}: the wheel elsewhere on the scene is not the volume\'s', raw() == r0, [r0, raw()])
    b.press('ArrowUp')
    check(f'{name}: the up arrow with nothing focused is one step louder', poll(lambda: raw() == r0 + 1) is True, [r0, raw()])
    b.press('ArrowDown')
    check(f'{name}: the down arrow is one step quieter', poll(lambda: raw() == r0) is True, [r0, raw()])
    for key in ('ArrowLeft', 'ArrowRight'):
        b.press(key)
    check(f'{name}: the sideways arrows are not read', raw() == r0, [r0, raw()])
    b.page("document.getElementById('nd-mute').focus()")
    b.press('ArrowUp')
    check(f'{name}: a control with the focus keeps the key', raw() == r0, [r0, raw()])
    # a click on + leaves the focus on it: the wheel over the number is not the keyboard's and still steps
    b.page("document.getElementById('nd-vup').focus()")
    click_el(b, '#nd-vup')
    poll(lambda: raw() == r0 + 1)
    check(f'{name}: the + key clicked keeps the focus', b.page('document.activeElement.id') == 'nd-vup', b.page('document.activeElement.id'))
    wheel(b, x, y, -120)
    check(f'{name}: and the wheel over the number still steps the volume', poll(lambda: raw() == r0 + 2) is True, [r0, raw()])
    click_el(b, '#nd-vdown')
    click_el(b, '#nd-vdown')
    poll(lambda: raw() == r0)
    b.page('document.activeElement.blur()')
    b.page("document.getElementById('fx-button').click()")
    time.sleep(0.4)
    b.page('document.activeElement.blur()')
    b.press('ArrowUp')
    check(f'{name}: not while the frame\'s menu is open', raw() == r0, [r0, raw()])
    b.press('Escape')
    b.page('document.activeElement.blur()')
    open_sheet(b)
    b.page('document.activeElement.blur()')
    b.press('ArrowUp')
    check(f'{name}: not while the sheet is open', raw() == r0, [r0, raw()])
    close_sheet(b)
    b.page('document.activeElement.blur()')
    b.page("(() => { const e = document.createElement('div'); e.id = 'probe-card'; e.setAttribute('aria-modal', 'true'); document.body.append(e); })()")
    b.press('ArrowUp')
    check(f'{name}: not while a question is open', raw() == r0, [r0, raw()])
    b.page("document.getElementById('probe-card').remove()")
    clear(b)
    key_held(b, 'ArrowUp', 1.3)
    held = raw()
    check(f'{name}: a held up arrow repeats like a held +', held >= r0 + 4, [r0, held, len(posts(b))])
    flip(b, 'computer')
    b.page('document.activeElement.blur()')
    p0 = pct()
    wheel(b, x, y, -120)
    check(f'{name}: on the computer side a notch is 1 %', poll(lambda: pct() == p0 + 1) is True, [p0, pct()])
    b.press('ArrowDown')
    check(f'{name}: and so is an arrow', poll(lambda: pct() == p0) is True, [p0, pct()])
    clear(b)
    key_held(b, 'ArrowDown', 1.3)
    check(f'{name}: a held arrow repeats on the computer side too, 1 % a step', pct() <= p0 - 4, [p0, pct()])
    forget(b, d)
    b.page(FADE_KEY)
    load(b, d, QUERY)
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.0)
    # the pointer rests over the number; the HUD fades; the first wheel only wakes it, the next steps
    x, y = figure()
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.0)
    r1 = raw()
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': y, 'deltaX': 0, 'deltaY': -120})
    else:
        wheel(b, x, y, -120)
    time.sleep(0.5)
    check(f'{name}: the first wheel over a faded number only wakes the HUD', b.page(FADED) is False and raw() == r1, [r1, raw(), b.page(FADED)])
    wheel(b, x, y, -120)
    check(f'{name}: the next one steps', poll(lambda: raw() == r1 + 1) is True, [r1, raw()])
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.0)
    r1 = raw()
    b.press('ArrowUp')
    check(f'{name}: an arrow wakes a faded dash and steps', b.page(FADED) is False and poll(lambda: raw() == r1 + 1) is True, [r1, raw(), b.page(FADED)])


def whole_keys(b, d):
    """`This computer` is never ellipsized, whatever the window: desktop and phone widths from 600 px up."""
    name = 'the_switch_keys_never_ellipsize_this_computer'
    for size in [(600, 800), (650, 700), (720, 800), (800, 600), (1000, 700), (640, 450), (700, 420)]:
        b.resize(*size)
        start(b, d)
        got = b.page("""(() => { const e = document.getElementById('nd-side-computer'); const r = e.getBoundingClientRect();
          const t = document.createRange(); t.selectNodeContents(e); return { clipped: t.getBoundingClientRect().width > e.clientWidth + 0.5 || getComputedStyle(e).textOverflow === 'ellipsis' && e.scrollWidth > e.clientWidth + 12, width: r.width, text: e.textContent, right: r.right, inside: r.right <= innerWidth && r.left >= 0 }; })()""")
        check(f'{name}: {size[0]}x{size[1]}: the key shows all of its words', not got['clipped'] and got['inside'], got)


def sixteen(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 7)
    open_sheet(b)
    name = 'sixteen_points_keep_the_layout'
    seven = b.page("(() => { const d = document.getElementById('nd-dialog').getBoundingClientRect(); const t = document.getElementById('nd-bands').getBoundingClientRect(); return [d.height, t.height]; })()")
    seed(b, 16)
    time.sleep(0.5)
    sixteen_ = b.page("(() => { const d = document.getElementById('nd-dialog').getBoundingClientRect(); const t = document.getElementById('nd-bands').getBoundingClientRect(); return [d.height, t.height]; })()")
    check(f'{name}: sixteen rows, the table still shows six and says 10 more',
          b.page("document.querySelectorAll('#nd-points-table tbody tr').length") == 16
          and b.page("document.getElementById('nd-more').textContent") == '10 more below ↓', b.page("document.getElementById('nd-more').textContent"))
    check(f'{name}: the sheet and its points section are as tall as with seven (1 px)',
          abs(seven[0] - sixteen_[0]) <= 1 and abs(seven[1] - sixteen_[1]) <= 1, [seven, sixteen_])
    check(f'{name}: the line counts them and + Add point is off',
          b.page("document.querySelector('.nd-pline').textContent").startswith('16 of 16 points')
          and b.page("document.getElementById('nd-point-add').getAttribute('aria-disabled')") == 'true')
    clear(b)
    tap(b, '#nd-point-add')
    check(f'{name}: a press on it posts nothing', posts(b) == [], posts(b))
    b.page("document.getElementById('nd-pscroll').scrollTop = 1000")
    time.sleep(0.4)
    check(f'{name}: scrolled to the end the words under the table go', b.page("document.getElementById('nd-more').textContent") == '')


def mid_drag(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 5)
    name = 'a_flip_mid_drag_writes_nothing'
    marks, t, box = curve_up(b)
    x, y = marks[3]
    press(b, x, y)
    for mx, my in path_to(x, y, [(x + 30, y - 20)]):
        move(b, mx, my)
    b.sleep(0.3)
    live = posts(b, 'api/v1/host/eq/points/3')
    check(f'{name}: the drag is under way: live updates went out', len(live) >= 2 and all(p[1].get('done') is False for p in live), live)
    last_live = live[-1][1]
    clear(b)
    b.page("__core.computer.flip('dac')")
    b.sleep(1.0)
    release(b, x + 30, y - 20)
    b.sleep(1.0)
    after = posts(b)
    check(f'{name}: the flip itself wrote nothing; the drag\'s own last value is the one post, done',
          len(after) == 1 and after[0][0] == 'api/v1/host/eq/points/3' and after[0][1].get('done') is True, after)
    check(f'{name}: and it is the value already playing (the last live post\'s frequency and gain)',
          len(after) == 1 and all(after[0][1].get(k) == v for k, v in last_live.items() if k != 'done'), [last_live, after])
    b.sleep(1.0)
    check(f'{name}: and nothing follows', len(posts(b)) == 1, posts(b))
    check(f'{name}: the drag ended at once: no label, nothing held, the DAC side shown',
          b.page("!document.querySelector('.nd-label')") and b.page(TUNE)['held'] is None and b.page('__core.computer.side()') == 'dac')
    b.page("__core.computer.flip('computer')")
    b.sleep(1.0)
    marks, t, box = curve_up(b)
    check(f'{name}: back on the computer the bubbles are the five points', len(marks) == 5, marks)


def guest(b, d):
    start(b, d)
    name = 'a_guest_sees_the_computer_inert'
    check(f'{name}: the guest has the switch', b.page("!!document.getElementById('nd-side-computer')") and b.page("document.getElementById('nd-side-computer').getAttribute('aria-disabled')") is None)
    flip(b, 'computer')
    open_sheet(b)
    live = b.page("[...document.querySelectorAll('#nd-dash [data-act=volume-up], #nd-dash [data-act=mute], #nd-dialog [data-act=mode], #nd-dialog [data-act=point-add], #nd-dialog [data-act=point-remove], #nd-dialog [data-act=preset], #nd-dialog input[data-point], #nd-dialog select')]"
                  ".filter((e) => e.getAttribute('aria-disabled') !== 'true' && !e.disabled).map((e) => e.id)")
    check(f'{name}: every computer control is inert', live == [], live)
    close_sheet(b)
    clear(b)
    for sel in ('#nd-vup', '#nd-mute'):
        tap(b, sel)
    marks, t, box = curve_up(b)
    spot = empty_spot(marks, t, box) or (t['w'] * 0.5, t['H'])
    b.click(*spot)
    time.sleep(0.5)
    check(f'{name}: presses and a tap on the mountains write nothing', posts(b) == [], posts(b))
    check(f'{name}: and the core says why', b.page("document.querySelectorAll('#toasts .toast').length") >= 1)


def kept(b, d):
    start(b, d)
    flip(b, 'computer')
    name = 'the_side_survives_a_reload'
    check(f'{name}: the side is kept in the browser', b.page("localStorage.getItem('svolder.side')") == 'computer')
    # every paint of the figure after the load, from the first one: never a live 0 %
    b.preload("""window.__seen = []; new MutationObserver(() => { const v = document.querySelector('.nd-vol svg');
      if (v) window.__seen.push(v.getAttribute('aria-label')); }).observe(document, { subtree: true, childList: true, attributes: true });""")
    b.goto(f'{d.url}{QUERY}', settle=0.3)
    b.page(BOOT)
    poll(lambda: b.page('__core.computer.side()') == 'computer')
    time.sleep(1.2)
    seen = b.page('window.__seen') or []
    check(f'{name}: the computer side is drawn after the load',
          b.page("document.querySelector('#nd-switch .nd-k[aria-pressed=true]').textContent") == 'This computer')
    check(f'{name}: the figure read the computer once it was read, no live 0 %', b.page(VOLUME_ARIA) not in (None, 'Volume 0 %', 'Volume, not read yet'), b.page(VOLUME_ARIA))
    check(f'{name}: and no paint of it ever said 0 %', not any(s == 'Volume 0 %' for s in seen), seen[:6])
    check(f'{name}: the load wrote nothing', posts(b) == [], posts(b))


def wheel_flip(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 4)
    name = 'a_flip_ends_a_wheel_burst_with_one_done'
    marks, t, box = curve_up(b)
    x, y = marks[4]
    clear(b)
    wheel(b, x, y, -120)
    b.page("__core.computer.flip('dac')")
    time.sleep(1.2)
    log = posts(b, 'api/v1/host/eq/points/4')
    flags = [p[1].get('done') for p in log]
    check(f'{name}: live updates, then exactly one done, the flip itself writing nothing',
          len(log) >= 2 and flags[-1] is True and flags.count(True) == 1, flags)
    time.sleep(0.8)
    check(f'{name}: and nothing follows (the burst\'s timer is cancelled)', len(posts(b, 'api/v1/host/eq/points/4')) == len(log), len(posts(b)))


def ghost(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 7)
    name = 'the_mountains_draw_the_curve_dimmed_while_the_eq_is_off'
    edge = """(() => { const d = __core.theme.driver; const t = d.tune(); const c = document.getElementById('nd-road'); const k = c.width / c.clientWidth;
      const g = c.getContext('2d'); const cols = [];
      const far = (x) => t.bubbles.every((b) => Math.abs(b.x - x) > 40) && (!t.diamonds || t.diamonds.every((q) => Math.abs(q.x - x) > 40));
      for (let x0 = 20; x0 < t.w - 20; x0 += 25) { if (!far(x0)) continue; const x = Math.round(x0 * k); const y = Math.round(d.curveAt(x0) * k);
        const px = g.getImageData(x, Math.max(0, y - 3), 1, 7).data;
        let top = 0; for (let i = 0; i < px.length; i += 4) top = Math.max(top, (px[i] + px[i + 1] + px[i + 2]) * px[i + 3] / 255); cols.push(top); }
      cols.sort((a, b) => a - b); const best = cols[Math.floor(cols.length / 2)];
      return { best, ghost: t.ghost, bubbles: t.bubbles.length, m: t.m }; })()"""
    marks, t, box = curve_up(b)
    time.sleep(0.4)
    on = b.page(edge)
    b.page("__core.actions.computer.setMode('off').then(() => 1)")
    poll(lambda: b.page(f'{COMPUTER}.eq.on') is False)
    marks, t, box = curve_up(b)
    time.sleep(0.4)
    off = b.page(edge)
    check(f'{name}: with the EQ on the curve is at full strength', on['ghost'] is False and on['best'] > 300, on)
    check(f'{name}: with it off the same points are drawn, flagged dimmed', off['ghost'] is True and off['bubbles'] == 7 and off['m'] >= 0.5, off)
    check(f'{name}: and the curve\'s edge is dimmer than with the EQ on (the edge away from the bubbles)', off['best'] < 0.75 * on['best'], [on, off])
    bad = b.page("""(() => { const d = __core.theme.driver; return d.tune().bubbles.filter((bub) => Math.abs(bub.y - d.curveAt(bub.x)) > 0.6).length; })()""")
    check(f'{name}: the dots are still on the curve', bad == 0, bad)
    if browser == 'chromium':
        os.makedirs(SHOTS, exist_ok=True)
        with open(f'{SHOTS}/computer-desktop-eq-off.png', 'wb') as f:
            f.write(b.shot())


def keyboard(b, d):
    start(b, d)
    name = 'the_keys_keep_the_focus_across_a_keyboard_flip'
    b.page("document.getElementById('nd-side-computer').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: Enter on This computer leaves the focus on it',
          b.page('document.activeElement.id') == 'nd-side-computer' and b.page('__core.computer.side()') == 'computer', b.page('document.activeElement.id'))
    b.page("document.getElementById('nd-side-dac').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: and back on the device\'s key',
          b.page('document.activeElement.id') == 'nd-side-dac' and b.page('__core.computer.side()') == 'dac', b.page('document.activeElement.id'))
    check(f'{name}: nothing written', posts(b) == [], posts(b))


def save_as(b, d):
    start(b, d)
    flip(b, 'computer')
    seed(b, 3)
    open_sheet(b)
    want = b.page(f'JSON.stringify({POINTS}.map((p) => [p.type, p.f, p.g, p.q]))')
    name = 'presets_save_rename_delete_from_the_computer_side'
    tap(b, '#nd-preset-save')
    b.page("(() => { const e = document.querySelector('.nd-askbox input'); e.value = 'Kept curve'; return 1; })()")
    b.press('Enter')
    got = poll(lambda: [p for p in posts(b) if p[0].endswith('/presets')])
    check(f'{name}: Save as posts the points on show', len(got) == 1 and got[0][1] == {'name': 'Kept curve', 'from': 'computer'}, got)
    listed = poll(lambda: b.page("[...document.querySelectorAll('#nd-presets [data-act=preset]')].some((p) => p.dataset.name === 'Kept curve')"))
    mine = d.model('Gungnir 2')['presets']['rows']
    check(f'{name}: it is in the list, the person\'s own',
          listed is True and any(r['name'] == 'Kept curve' and not r['built_in'] for r in mine)
          and b.page("document.querySelector('#nd-presets [data-name=\"Kept curve\"]').classList.contains('own')"), mine)

    def own(act, then):
        tap(b, f'#nd-preset-{act}')
        if b.page("!!document.querySelector('.nd-list .nd-opt')"):
            click_el(b, f'.nd-list .nd-opt[data-value="{then}"]')
            time.sleep(0.4)
    own('rename', 'Kept curve')
    b.page("(() => { const e = document.querySelector('.nd-askbox input'); e.value = 'Kept two'; return 1; })()")
    b.press('Enter')
    renamed = poll(lambda: b.page("[...document.querySelectorAll('#nd-presets [data-act=preset]')].some((p) => p.dataset.name === 'Kept two')"))
    check(f'{name}: rename', renamed is True, posts(b))
    own('delete', 'Kept two')
    click_el(b, '.nd-askbox [data-answer="delete"]')
    gone = poll(lambda: b.page("![...document.querySelectorAll('#nd-presets [data-act=preset]')].some((p) => p.dataset.name === 'Kept two')"))
    check(f'{name}: delete', gone is True, posts(b))
    check(f'{name}: the points were not changed', b.page(f'JSON.stringify({POINTS}.map((p) => [p.type, p.f, p.g, p.q]))') == want)


def both_sides(b, d):
    start(b, d)
    name = 'the_same_preset_on_both_asks_and_yes_flattens_the_dac'
    status, _ = d.post('Gungnir 2', 'presets/recall', {'name': 'Warm'})
    check(f'{name}: the DAC plays Warm', status == 200, status)
    time.sleep(1.0)

    def playing():
        return b.page("JSON.stringify(__core.model.device().presets?.rows?.filter((r) => r.on).map((r) => r.name))")
    flip(b, 'computer')
    open_sheet(b)
    clear(b)
    tap(b, '#nd-presets [data-act="preset"][data-name="Warm"]')
    card = poll(lambda: b.page(f'!!{CARD}'))
    words = b.page(f'{CARD}?.querySelector(".ask-words").textContent') or ''
    check(f'{name}: it asks, with the device named', card is True and 'flatten' in words and 'Gungnir 2' in words, words)
    b.page(f'{CARD}.querySelector(".ask-no").click()')
    time.sleep(0.8)
    log = posts(b)
    check(f'{name}: no sends nothing more and says nothing',
          len(log) == 1 and log[0][2] == 409 and b.page(f'!{CARD}') and b.page("document.querySelectorAll('#toasts .toast').length") == 0, log)
    tap(b, '#nd-presets [data-act="preset"][data-name="Warm"]')
    poll(lambda: b.page(f'!!{CARD}'))
    b.page(f'{CARD}.querySelector(".ask-yes").click()')
    time.sleep(1.2)
    log = posts(b)
    check(f'{name}: yes repeats with confirm and lands', len(log) == 3 and log[2][1].get('confirm') is True and log[2][2] == 200, log)
    flat = poll(lambda: 'Warm' not in (playing() or '') and b.page(f'{COMPUTER}.eq.preset') == 'Warm')
    check(f'{name}: the DAC is flat, the computer plays it', flat is True, [playing(), b.page(f'{COMPUTER}.eq.preset')])


def amp(b, d):
    start(b, d)
    name = 'the_amp_page_flips_to_the_computer_with_the_dac_parts'
    b.page("__core.actions.showDevice('Amp').then(() => 1)")
    poll(lambda: b.page('__core.model.shown()') == 'Amp')
    time.sleep(1.5)
    check(f'{name}: the amp page has the switch, named for the amp', b.page(KEYS) == ['Amp', 'This computer'], b.page(KEYS))
    check(f'{name}: its own parts first: no volume keys', b.page("!document.querySelector('#nd-dash [data-act=volume-up]')"))
    flip(b, 'computer')
    time.sleep(0.8)
    open_sheet(b)
    parts = b.page("""({ keys: !!document.querySelector('#nd-dash [data-act=volume-up]'), figure: document.querySelector('.nd-vol svg')?.getAttribute('aria-label'),
      mode: !!document.getElementById('nd-mode-eq'), points: document.querySelectorAll('#nd-points-table tbody tr').length,
      presets: !!document.getElementById('nd-presets'), line: document.querySelector('.nd-eq')?.textContent.trim() })""")
    check(f'{name}: the DAC page\'s volume keys, figure, mode, points and presets',
          parts['keys'] and (parts['figure'] or '').endswith('%') and parts['mode'] and parts['points'] >= 1 and parts['presets']
          and (parts['line'] or '').split()[0] in ('EQ', 'OFF'), parts)
    close_sheet(b)
    clear(b)
    tap(b, '#nd-vup')
    got = poll(lambda: posts(b, 'api/v1/host/volume'))
    check(f'{name}: and they write the computer\'s volume', len(got) == 1, got)
    flip(b, 'dac')
    check(f'{name}: flipped back it is the amp\'s page', b.page("!document.querySelector('#nd-dash [data-act=volume-up]')"))


def fade(b, d):
    forget(b, d)
    b.page(FADE_KEY)
    load(b, d, QUERY)
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    name = 'the_fade_keeps_the_switch_keys_as_the_menu_button'
    faded = poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.2)
    keys = b.page("+getComputedStyle(document.getElementById('nd-switch')).opacity")
    dash = b.page("+getComputedStyle(document.getElementById('nd-dash')).opacity")
    top = b.page("+getComputedStyle(document.querySelector('.nd-top')).opacity")
    check(f'{name}: the HUD and the dash fade and the switch stays whole', faded and top < 0.2 and dash < 0.3 and keys == 1, [faded, top, dash, keys])
    clear(b)
    tap_synth(b, '#nd-vup')
    time.sleep(0.5)
    check(f'{name}: the first tap on the faded dash only wakes it', posts(b) == [] and not b.page(FADED), posts(b))
    tap_synth(b, '#nd-vup')
    check(f'{name}: and the next one steps the volume', poll(lambda: len(posts(b, 'api/v1/devices/')) >= 1), posts(b))
    # the keys by the menu button are not the dash: they work the first time they are pressed from a faded page
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.0)
    keys = b.page("+getComputedStyle(document.getElementById('nd-switch')).opacity")
    tap_synth(b, '#nd-side-computer')
    time.sleep(0.5)
    check(f'{name}: a press on them flips at once, from a faded page', keys == 1 and b.page('__core.computer.side()') == 'computer', keys)


def right_column(b, d):
    """With words on the right of the top line (the device out of reach) and the EQ line, the keys by the menu button, the
    words, the EQ line, the figure and the device line meet nowhere and stay inside the window."""
    d.drop()
    forget(b, d)
    load(b, d, QUERY)
    poll(lambda: 'not connected' in (b.page("document.querySelector('.nd-left').textContent") or ''), 12.0)
    time.sleep(0.6)
    name = 'the_right_column_runs_keys_then_words_then_the_eq_line'
    boxes = b.page("""(() => { const r = (s) => { const e = document.querySelector(s); if (!e || e.hidden) return null;
        const b = e.getBoundingClientRect(); return b.width ? [b.left, b.top, b.right, b.bottom] : null; };
      return { keys: r('#nd-switch .nd-swkeys'), words: r('.nd-right'), eq: r('.nd-eq'), figure: r('.nd-vol svg'), device: r('.nd-left'),
        button: r('#fx-button'), width: innerWidth, said: document.querySelector('.nd-right').textContent }; })()""")
    named = [k for k in ('device', 'keys', 'words', 'eq', 'figure', 'button') if boxes[k]]
    meets = [(a, c) for i, a in enumerate(named) for c in named[i + 1:]
             if boxes[a][0] < boxes[c][2] and boxes[c][0] < boxes[a][2] and boxes[a][1] < boxes[c][3] and boxes[c][1] < boxes[a][3]]
    check(f'{name}: the keys and the words are drawn', boxes['keys'] is not None and boxes['words'] is not None and boxes['said'] != '', boxes)
    check(f'{name}: nothing meets and everything is inside the window',
          not meets and all(boxes[k][0] >= 0 and boxes[k][2] <= boxes['width'] for k in named), [meets, boxes])
    check(f'{name}: the words are under the keys', boxes['words'] is not None and boxes['keys'] is not None and boxes['words'][1] >= boxes['keys'][3] - 1, boxes)


def phone(b, d):
    reset_bands(d)
    forget(b, d)
    b.resize(412, 915)
    load(b, d, QUERY)
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    time.sleep(0.8)
    name = 'the_phone_panel_starts_with_the_switch'
    geo = b.page("""(() => { const r = (s) => { const e = document.querySelector(s); if (!e) return null; const b = e.getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; };
      return { h: innerHeight, scene: r('.nd-scene'), dash: r('#nd-dash'), dac: r('#nd-side-dac'), computer: r('#nd-side-computer'), down: r('#nd-vdown'),
        figure: r('.nd-vol svg'), inDash: !!document.querySelector('#nd-dash #nd-side-dac'), slot: document.getElementById('nd-switch').children.length,
        top: r('.nd-top'), eq: r('.nd-eq'), button: r('#fx-button'),
        short: [...document.querySelectorAll('#nd-switch .nd-k')].map((k) => getComputedStyle(k, '::before').content.replace(/"/g, '')),
        names: [...document.querySelectorAll('#nd-switch .nd-k')].map((k) => k.getAttribute('aria-label')),
        gains: !!document.querySelector('.nd-eq .nd-gains') && /[0-9]/.test(document.querySelector('.nd-gains').textContent) }; })()""")
    check(f'{name}: the switch is one pill in the scene\'s top left corner (8 px from the edge, level with the menu button), none of it in the panel',
          not geo['inDash'] and geo['slot'] == 1 and abs(geo['dac'][0] - 8) <= 1 and geo['computer'][3] <= geo['dash'][1]
          and abs(geo['dac'][1] - geo['computer'][1]) < 2 and abs((geo['dac'][1] + geo['dac'][3]) / 2 - (geo['button'][1] + geo['button'][3]) / 2) <= 1, geo)
    check(f'{name}: the pill clears the device line and the menu button; its segments read DAC and PC and keep their whole names',
          geo['computer'][2] < geo['top'][0] and geo['top'][2] < geo['button'][0] and geo['short'] == ['DAC', 'PC']
          and geo['names'] == ['Gungnir 2', 'This computer'], geo)
    check(f'{name}: the keys are 36 px tall', 35.5 <= geo['dac'][3] - geo['dac'][1] <= 36.5 and 35.5 <= geo['computer'][3] - geo['computer'][1] <= 36.5, geo)
    share = geo['dash'][1] / geo['h']
    check(f'{name}: the scene above the panel is over 77 % of the screen', share > 0.77, share)
    check(f'{name}: the volume figure is in the scene', geo['figure'][3] <= geo['scene'][3], geo)
    check(f'{name}: the EQ line shows its values on the DAC side', geo['gains'], geo)
    # a keyboard flip keeps the focus on the key chosen, in the panel too (the dash is drawn again)
    b.page("document.getElementById('nd-side-computer').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: Enter on This computer leaves the focus on it',
          b.page('document.activeElement.id') == 'nd-side-computer' and b.page('__core.computer.side()') == 'computer', b.page('document.activeElement.id'))
    b.page("document.getElementById('nd-side-dac').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check(f'{name}: and back on the device\'s key', b.page('document.activeElement.id') == 'nd-side-dac' and b.page('__core.computer.side()') == 'dac')
    flip(b, 'computer')
    check(f'{name}: flipped, the figure reads %, the EQ line reads EQ or OFF',
          (b.page(VOLUME_ARIA) or '').endswith('%') and b.page("document.querySelector('.nd-eq').textContent.trim()").split()[0] in ('EQ', 'OFF'))
    # sideways the pill is in the top left corner: the dash fades and they stay, so the first tap on them flips
    b.resize(844, 390)
    time.sleep(0.8)
    check(f'{name}: held sideways the pill is in the top left corner, not in the dash',
          not b.page("!!document.querySelector('#nd-dash #nd-side-dac')") and b.page("document.getElementById('nd-switch').children.length") == 1)
    forget(b, d)
    b.page(FADE_KEY)
    load(b, d, QUERY)
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(3.5)
    check('the_fade_keeps_the_switch_keys_as_the_menu_button: held sideways the dash fades to a fifth and the switch\'s keys stay whole',
          b.page("+getComputedStyle(document.getElementById('nd-dash')).opacity") == 0.2 and b.page("document.getElementById('nightdrive').classList.contains('nd-dash-dim')")
          and b.page("+getComputedStyle(document.getElementById('nd-side-computer')).opacity") == 1)
    tap_synth(b, '#nd-side-computer')
    time.sleep(0.4)
    check('the_fade_keeps_the_switch_keys_as_the_menu_button: a tap on a key flips at once, the dash woken', b.page('__core.computer.side()') == 'computer'
          and b.page("+getComputedStyle(document.getElementById('nd-dash')).opacity") == 1)


def pictures():
    """The computer side on a curve with shelves, desktop and phone, the sheet shut and open (Chromium), each on a
    daemon of its own: the curve is the plan's seven points, and a bubble is under the pointer with its bell lit."""
    os.makedirs(SHOTS, exist_ok=True)
    for kind, size, dpr in (('desktop', (1280, 800), 1), ('phone', (390, 844), 2)):
        d = common.Daemon('gungnir2-v2', host='fake', env={})
        b = common.Chromium(size=size, dpr=dpr)
        b.preload(HOOK)
        try:
            start(b, d)
            flip(b, 'computer')
            seed(b, 8)
            b.page('__core.actions.computer.removePoint(1).then(() => 1)')
            poll(lambda: b.page(f'{POINTS}.length') == 7)
            time.sleep(0.6)
            marks, t, box = curve_up(b)
            hover(b, *marks[4])
            time.sleep(0.5)
            hover(b, marks[4][0] + 1, marks[4][1])
            time.sleep(0.6)
            # the bubble held (pressed, not moved: nothing is written)
            press(b, marks[4][0] + 1, marks[4][1])
            time.sleep(0.5)
            with open(f'{SHOTS}/computer-{kind}.png', 'wb') as f:
                f.write(b.shot())
            release(b, marks[4][0] + 1, marks[4][1])
            time.sleep(0.5)
            open_sheet(b)
            b.page("(() => { const e = document.getElementById('nd-p4f'); if (e) e.focus(); return 1; })()")
            time.sleep(0.5)
            with open(f'{SHOTS}/computer-{kind}-sheet.png', 'wb') as f:
                f.write(b.shot())
            print('shots', f'{SHOTS}/computer-{kind}.png')
        finally:
            b.close()
            d.stop()


def touch(d, size):
    """A finger on the mountains (Chromium, touch emulation) in a window of `size`: a phone upright, a phone on its
    side, a desktop-sized window."""
    b = common.Chromium(size=size, dpr=2, touch=True)
    b.preload(HOOK)
    name = f'a_finger_adds_drags_and_pinches ({size[0]}x{size[1]})'
    try:
        forget(b, d)
        b.page(FADE_KEY)
        load(b, d, QUERY)
        poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq)'))
        time.sleep(0.5)
        b.page('window.__posts.length = 0')
        flip(b, 'computer')
        seed(b, 4)

        def fingers(pts):
            return [{'x': x, 'y': y, 'id': i} for i, (x, y) in enumerate(pts)]

        def touch_at(pts, secs=0.05):
            b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': fingers(pts)})
            b.sleep(secs)

        def lift():
            b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        t = b.page(TUNE)
        spot0 = (t['w'] * 0.5, t['H'] + 0.02 * t['h'])
        # the first touch only shows the curve
        touch_at([spot0])
        lift()
        b.sleep(0.8)
        check(f'{name}: the first touch shows the curve and adds nothing', posts(b, 'api/v1/host/eq/points') == [] and b.page(TUNE)['m'] >= 0.5, [posts(b), b.page(TUNE)['m']])
        t = b.page(TUNE)
        marks = {bub['band']: (bub['x'], bub['y']) for bub in t['bubbles']}
        spot = None
        for fx in (0.15, 0.3, 0.5, 0.7, 0.85):
            for dy in (0.0, 0.04, -0.04, 0.07):
                x, y = fx * t['w'], t['H'] - 0.08 * t['h'] + dy * t['h']
                if all(math.hypot(x - mx, y - my) > 50 for mx, my in marks.values()):
                    spot = (x, y)
                    break
            if spot:
                break
        touch_at([spot])
        lift()
        b.sleep(1.0)
        got = posts(b, 'api/v1/host/eq/points')
        check(f'{name}: a tap on an empty spot, the curve up, adds a PK', len(got) == 1 and got[0][1]['type'] == 'PK', got)
        clear(b)
        t = b.page(TUNE)
        x, y = next((bub['x'], bub['y']) for bub in t['bubbles'] if bub['band'] == 2)
        scroll = b.page('window.scrollY')
        touch_at([(x, y)])
        for mx, my in path_to(x, y, [(x + 30, y - 20)]):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': fingers([(mx, my)])})
            b.sleep(0.03)
        lift()
        b.sleep(1.2)
        log = posts(b, 'api/v1/host/eq/points/2')
        check(f'{name}: a finger on a bubble drags it, done at the lift, no scroll',
              len(log) >= 2 and log[-1][1].get('done') is True and b.page('window.scrollY') == scroll, [len(log), scroll])
        clear(b)
        t = b.page(TUNE)
        x, y = next((bub['x'], bub['y']) for bub in t['bubbles'] if bub['band'] == 2)
        q0 = b.page(f'{POINTS}[1].q')
        a, c = (x - 40, y), (x + 40, y)
        touch_at([a, c])
        for i in range(1, 11):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': fingers([(a[0] - 4 * i, a[1]), (c[0] + 4 * i, c[1])])})
            b.sleep(0.03)
        lift()
        b.sleep(1.2)
        q1 = b.page(f'{POINTS}[1].q')
        check(f'{name}: two fingers twice as far apart halve the Q', abs(q1 - q0 / 2) <= q0 / 2 * 0.2, [q0, q1])
        # the band that takes a touch is the window the curve is drawn in (+12 to -12 dB) and a hand's reach beyond it
        t = b.page(TUNE)
        v = t['v']
        top = t['H'] - (WINDOW_BASE + WINDOW_SPAN) * v - 26
        bottom = t['H'] + (WINDOW_SPAN - WINDOW_BASE) * v + 26
        marks = [(bub['x'], bub['y']) for bub in t['bubbles']]
        fx = next(f / 20 * t['w'] for f in range(1, 20) if all(abs(f / 20 * t['w'] - mx) > 60 for mx, _ in marks))
        clear(b)
        have = len(b.page(POINTS))
        touch_at([(fx, top + 1)])
        lift()
        b.sleep(1.0)
        t = b.page(TUNE)
        added = [bub for bub in t['bubbles'] if abs(bub['x'] - fx) < 3]
        check(f'{name}: a tap at the top of the band adds a point drawn inside it, at +12 dB',
              len(b.page(POINTS)) == have + 1 and len(added) == 1 and top < added[0]['y'] <= t['H'] - (WINDOW_BASE + WINDOW_SPAN) * v + 1, [added, top, t['H']])
        mode = b.page(TUNE_ON)
        touch_at([(added[0]['x'], added[0]['y'])])
        b.sleep(0.3)
        held = b.page(TUNE)['held']
        lift()
        b.sleep(0.5)
        check(f'{name}: a second touch on that point, where it is drawn, grabs it, and the curve stays up',
              added and held == added[0]['band'] and mode is True and b.page(TUNE_ON) is True, [held, added, mode, b.page(TUNE_ON)])
        fx2 = next(f / 20 * t['w'] for f in range(1, 20) if all(math.hypot(f / 20 * t['w'] - bub['x'], t['H'] - 0.08 * t['h'] - bub['y']) > 40 for bub in t['bubbles']))
        poll(lambda: b.page(TUNE)['m'] < 0.3, 4.0)
        touch_at([(fx2, t['H'] - 0.08 * t['h'])])
        lift()
        poll(lambda: b.page(TUNE)['m'] >= 0.5, 2.0)
        clear(b)
        have = len(b.page(POINTS))
        touch_at([(fx2, top - 6)])
        lift()
        b.sleep(0.8)
        check(f'{name}: with the curve up, a tap 6 px above the band makes no point',
              b.page(TUNE)['m'] >= 0.5 and len(b.page(POINTS)) == have and posts(b, 'api/v1/host/eq/points') == [], [have, b.page(TUNE)['m'], posts(b)])
        # outside the band the touch is still a touch on the scene: it wakes a faded HUD
        poll(lambda: b.page(TUNE)['m'] < 0.3, 6.0)
        faded = poll(lambda: b.page(FADED), 8.0)
        touch_at([(fx2, top - 6)])
        lift()
        b.sleep(0.5)
        check(f'{name}: a tap above the band wakes a faded HUD and adds nothing',
              faded and not b.page(FADED) and len(b.page(POINTS)) == have and posts(b, 'api/v1/host/eq/points') == [], [faded, b.page(FADED), have, posts(b)])
        # three points of +12 dB on one frequency add up to far beyond 12 dB: the bubbles stay inside the band
        for _ in range(3):
            b.page("__core.actions.computer.addPoint({ type: 'PK', f: 3000, g: 12, q: 1 }).then(() => 1)")
            b.sleep(0.2)
        b.sleep(1.0)
        t = b.page(TUNE)
        stacked = [bub for bub in t['bubbles'] if abs(bub['x'] - t['w'] * math.log(3000 / 20) / math.log(1000)) < 3]
        check(f'{name}: every bubble is drawn inside the band, however the points add up',
              len(stacked) >= 3 and all(top < bub['y'] < bottom for bub in t['bubbles']), [t['bubbles'], top, bottom])
        check(f'{name}: no page error', page_errors(b) == [], page_errors(b)[:3])
    finally:
        b.close()


def one(name, fn, size=(1280, 900), fleet=(), web='auth = "none"'):
    d = common.Daemon('gungnir2-v2', host='fake', fleet=fleet, env={}, web=web)
    b = open_page(d, size)
    try:
        fn(b, d)
        check(f'{name}: no page error', page_errors(b) == [], page_errors(b)[:3])
    finally:
        b.close()
        d.stop()


one('the_two_keys_flip_volume_and_tone_together', the_keys)
one('minus_plus_mute_set_the_computer_volume', volume)
one('a_point_drag_posts_live_and_done_at_release', drag)
one('every_dot_is_on_the_curve', dots)
one('a_tap_adds_a_point_and_x_removes_it', add_remove)
one('the_points_are_listed_by_frequency', sorted_points)
one('the_mode_the_type_a_typed_value_and_the_presets_write', controls, size=(1280, 900))
one('an_untouched_cell_writes_nothing', untouched)
one('the_wheel_sets_the_q', wheel_q)
one('sixteen_points_keep_the_layout', sixteen)
if browser == 'chromium':
    one('a_flip_mid_drag_writes_nothing', mid_drag)
one('a_guest_sees_the_computer_inert', guest, web='guest = "read"')
one('the_side_survives_a_reload', kept)
one('a_flip_ends_a_wheel_burst_with_one_done', wheel_flip)
one('the_mountains_draw_the_curve_dimmed_while_the_eq_is_off', ghost)
one('the_wheel_and_the_arrows_step_the_volume', from_the_scene)
one('the_switch_keys_never_ellipsize_this_computer', whole_keys)
one('the_keys_keep_the_focus_across_a_keyboard_flip', keyboard)
one('presets_save_rename_delete_from_the_computer_side', save_as)
one('the_same_preset_on_both_asks_and_yes_flattens_the_dac', both_sides)
one('the_amp_page_flips_to_the_computer_with_the_dac_parts', amp, fleet=FLEET)
one('the_fade_keeps_the_switch_keys_as_the_menu_button', fade)
one('the_right_column_runs_keys_then_words_then_the_eq_line', right_column, size=(1280, 800))
one('the_phone_panel_starts_with_the_switch', phone, size=(412, 915))
if browser == 'chromium':
    d = common.Daemon('gungnir2-v2', host='fake', env={})
    try:
        for size in ((390, 844), (844, 390), (1280, 900)):
            touch(d, size)
    finally:
        d.stop()
    pictures()
base.finish('computer_nightdrive.py')
