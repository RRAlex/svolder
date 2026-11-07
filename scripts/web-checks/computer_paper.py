# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Paper's computer side in a real page (design: the computer flip), on a fake host
(`SVOLDER_HOST=fake`: no PipeWire, no sound): the switch by the title, the 0 to 100 % ruler, the
tone written `TONE . off . EQ`, the points table, all through the page's own hands.
  the_switch_flips_volume_and_tone_together   `showing: the DAC . this computer`, the chosen side
                                       underlined; one tap flips the ruler and the tone, posts nothing
  minus_plus_mute_set_the_computer_volume   one percent a tap, mute and unmute, the request log
  a_point_drag_posts_live_and_done_at_release   a point dragged: live updates, the last one done
  a_tap_adds_a_point_and_x_removes_it   a tap on an empty spot adds a PK at Q 1.0, its x removes it
  the_points_are_listed_by_frequency_and_the_selection_follows   a drag never re-sorts, the release does, the point in hand follows
  sixteen_points_keep_the_layout        the tone section and the page are as tall with 16 as with 7,
                                       the table scrolls and counts what is below
  a_flip_mid_drag_writes_nothing_more_than_the_last_value   the flip ends the drag; at most its last value
  a_shelf_and_every_other_dot_sit_on_the_curve   at rest, in a drag, after it
  the_switch_and_the_cross_keep_the_focus_across_a_keyboard_use
  presets_save_rename_delete_from_the_computer_side
  a_guest_sees_the_computer_inert       a guest flips and looks; every word is quiet and posts nothing
  the_side_survives_a_reload            kept in the browser, a reload writes nothing
  the_same_preset_on_both_asks_and_yes_flattens_the_dac   Paper's own question line: no sends
                                       nothing, yes repeats with confirm
  the_amp_page_flips_to_the_computer_with_the_dac_parts   the switch on an amp's page, the same ruler
                                       and tone, the amp's own sections as they were
  the_computer_side_fits_a_phone        no sideways scroll at 390; the stills of both sizes
Usage: python3 computer_paper.py chromium|firefox"""
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
SHOTS = f'{common.REPO}/target/web-shots/paper'


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# every write the page posts, with the status the daemon answered
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (!(a[1] && a[1].method === 'POST')) return f(...a);"
        " const entry = [String(a[0]), a[1].body, null]; window.__posts.push(entry);"
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; await document.fonts.ready; window.__core = m.core; return m.core.theme.id; })"""
COMPUTER = '__core.model.page().host?.computer'
PENCIL = {'light': 'rgb(179, 38, 30)', 'dark': 'rgb(242, 120, 107)'}
FLEET = [('fake-amp', 'Amp', '1112131415161718')]


def make(size=(1280, 900)):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
    b.preload(HOOK)
    return b


def errors(b):
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'favicon.ico' not in e and 'status of 409' not in e
            and 'status of 401' not in e]


def poll(read, secs=6.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, side=None):
    """Paper on show, the computer's model read; with `side` that side flipped to (writes nothing)."""
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    if b.page(BOOT) != 'paper':
        raise RuntimeError('Paper did not mount')
    poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq?.points?.length)'))
    if side:
        b.page(f"(__core.computer.flip('{side}'), 1)")
        time.sleep(0.4)


def posts(b):
    return [(p[0], json.loads(p[1]) if p[1] else None, p[2]) for p in b.page('window.__posts')]


def host_posts(b):
    return [p for p in posts(b) if p[0].startswith('api/v1/host')]


def text(b, selector):
    return b.page(f"document.querySelector({json.dumps(selector)})?.textContent ?? null")


def rect(b, selector):
    return b.page(f"(() => {{ const r = document.querySelector({json.dumps(selector)})?.getBoundingClientRect();"
                  " return r ? { x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height,"
                  " l: r.left, t: r.top } : null; })()")


def count(b):
    return b.page(f'{COMPUTER}.eq.points.length')


def pct(b):
    return b.page('__core.computer.volumeOf(__core.model.page())?.pct ?? null')


def click(b, selector):
    b.page(f"document.querySelector({json.dumps(selector)}).click()")
    time.sleep(0.25)


def drag_xy(b, x0, y0, x1, y1, hold=False):
    """Press at (x0, y0), move to (x1, y1) in steps of about 8 px, release (unless `hold`:
    the button stays down, Chromium only)."""
    n = max(1, int(math.hypot(x1 - x0, y1 - y0) // 8))
    path = [(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(1, n + 1)]
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x0, 'y': y0})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x0, 'y': y0,
                                            'button': 'left', 'clickCount': 1, 'buttons': 1})
        for x, y in path:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y,
                                                'button': 'left', 'buttons': 1})
        if not hold:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x1, 'y': y1,
                                                'button': 'left', 'clickCount': 1})
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x0), 'y': int(y0)},
                {'type': 'pointerDown', 'button': 0}]
        for x, y in path:
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)})
        acts.append({'type': 'pointerUp', 'button': 0})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'}, 'actions': acts}]})
    time.sleep(0.4)


def one(name, fn, fleet=(), web='auth = "none"', env=None):
    d = common.Daemon('gungnir2-v2', host='fake', fleet=list(fleet), web=web, env=env or {})
    b = make()
    try:
        fn(b, d)
        check(f'{name}: no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def a_switch(b, d):
    name = 'the_switch_flips_volume_and_tone_together'
    load(b, d)
    line = text(b, '#p-title .p-showing')
    check(f'{name}: the title says what is shown', (line or '').split() == ['showing:', 'the', 'DAC', '·', 'this', 'computer'], line)
    state = b.page("(() => { const w = [...document.querySelectorAll('.p-showing .p-w')];"
                   " return w.map((x) => [x.textContent, x.classList.contains('on'), x.getAttribute('aria-pressed'),"
                   " getComputedStyle(x).textDecorationStyle, getComputedStyle(x).textDecorationColor]); })()")
    check(f'{name}: the DAC is the one underlined, solid in pencil, the computer dotted',
          state[0][:3] == ['the DAC', True, 'true'] and state[0][3] == 'solid' and state[0][4] == PENCIL['light']
          and state[1][:3] == ['this computer', False, 'false'] and state[1][3] == 'dotted', state)
    dac_ruler = b.page("document.querySelector('#p-ruler')?.classList.contains('pct') ?? null")
    dac_mode = b.page("!!document.querySelector('[data-act=\"mode-list\"]')")
    check(f'{name}: the DAC side is the ruler in dB and the tone words as before', dac_ruler is False and dac_mode is True,
          [dac_ruler, dac_mode])
    click(b, '[data-act="side"][data-side="computer"]')
    state = b.page("[...document.querySelectorAll('.p-showing .p-w')].map((x) => [x.classList.contains('on'),"
                   " getComputedStyle(x).textDecorationStyle])")
    check(f'{name}: the chosen side moves with the tap', state == [[False, 'dotted'], [True, 'solid']], state)
    labels = b.page("[...document.querySelectorAll('#p-ruler .p-mark')].map((m) => m.textContent)")
    check(f'{name}: the ruler reads 0 to 100', b.page("document.querySelector('#p-ruler').classList.contains('pct')") is True
          and labels == ['0', '25', '50', '75', '100'], labels)
    zones = b.page("document.querySelectorAll('#p-volume .p-zone').length")
    check(f'{name}: no red zone and no cap', zones == 0, zones)
    shown = text(b, '#p-volume-text')
    check(f'{name}: the volume is the one the host reads, in percent',
          shown == f'{pct(b)} %' and pct(b) is not None, [shown, pct(b)])
    heading = text(b, '#p-tone .p-label')
    modes = b.page("[...document.querySelectorAll('#p-tone [data-act=\"c-mode\"]')].map((m) => [m.textContent,"
                   " m.classList.contains('on')])")
    check(f'{name}: the tone is written Tone . off . EQ with the chosen underlined',
          ' '.join((heading or '').split()) == 'Tone · off · EQ' and [m[0] for m in modes] == ['off', 'EQ']
          and sum(1 for m in modes if m[1]) == 1, [heading, modes])
    below = b.page("['#p-volume', '#p-tone', '#p-presets', '#p-notes'].map((s) => document.querySelector(s)?.innerText ?? '')"
                   ".join(' ') + ' ' + [...document.querySelectorAll('#p-volume [aria-label], #p-tone [aria-label], #p-presets [aria-label]')]"
                   ".map((e) => e.getAttribute('aria-label')).join(' ')")
    # the path's own words (its computer hop line, `computer volume N %`, `computer muted`) are the one exception
    own = re.sub(r'computer(: [^\n]*| (volume \d+ ?%|muted))', '', below.lower())
    check(f'{name}: below the switch nothing says computer', 'computer' not in own, below[:300])
    check(f'{name}: the flip posted nothing', posts(b) == [], posts(b))
    click(b, '[data-act="side"][data-side="dac"]')
    check(f'{name}: and back, the DAC\'s ruler and words return',
          b.page("document.querySelector('#p-ruler').classList.contains('pct')") is False
          and b.page("!!document.querySelector('[data-act=\"mode-list\"]')") is True and posts(b) == [], posts(b))


def a_volume(b, d):
    name = 'minus_plus_mute_set_the_computer_volume'
    load(b, d, 'computer')
    start = pct(b)
    check(f'{name}: the ruler reads the sink', start is not None and text(b, '#p-volume-text') == f'{start} %', start)
    click(b, '#p-cup')
    check(f'{name}: louder is one percent more', poll(lambda: pct(b) == start + 1) is True, [start, pct(b)])
    click(b, '#p-cdown')
    click(b, '#p-cdown')
    check(f'{name}: quieter twice is two percent less', poll(lambda: pct(b) == start - 1) is True, [start, pct(b)])
    # the model on the daemon's side agrees once the writes settle
    time.sleep(0.6)
    check(f'{name}: the host reads the same', b.page(f'{COMPUTER}.volume.pct') == start - 1, b.page(f'{COMPUTER}.volume'))
    click(b, '#p-cmute')
    check(f'{name}: mute', poll(lambda: b.page(f'{COMPUTER}.volume.muted') is True) is True
          and text(b, '#p-cmute') == 'unmute' and text(b, '#p-volume-text') == 'muted',
          [text(b, '#p-cmute'), text(b, '#p-volume-text')])
    click(b, '#p-cmute')
    check(f'{name}: unmute', poll(lambda: b.page(f'{COMPUTER}.volume.muted') is False) is True)
    sent = [p[1] for p in posts(b) if p[0] == 'api/v1/host/volume']
    check(f'{name}: the request log is 1 % steps', [s['pct'] for s in sent] == [start + 1, start, start - 1], sent)
    check(f'{name}: the mute writes', [p[1] for p in posts(b) if p[0] == 'api/v1/host/mute'] == [{'muted': True}, {'muted': False}],
          posts(b))
    # a drag along the ruler lands where the pointer goes
    box = rect(b, '#p-ruler')
    drag_xy(b, box['l'] + box['w'] * 0.2, box['y'], box['l'] + box['w'] * 0.7, box['y'])
    check(f'{name}: a drag along the ruler writes the percent under the pointer',
          poll(lambda: abs((pct(b) or 0) - 70) <= 1) is True, pct(b))
    # the keys on the needle: an arrow is one percent
    now = pct(b)
    b.page("document.querySelector('#p-needle').focus()")
    b.press('ArrowRight')
    check(f'{name}: the arrow key is one percent', poll(lambda: pct(b) == now + 1) is True, [now, pct(b)])
    check(f'{name}: no write was above 100 or below 0', all(0 <= p[1]['pct'] <= 100 for p in posts(b) if p[0] == 'api/v1/host/volume'))


def settle_to(b, target):
    """Add or remove points through the page's own actions until the EQ holds `target` of them
    (the setup of a check, not the thing it checks)."""
    for _ in range(40):
        have = count(b)
        if have == target:
            return True
        if have < target:
            b.page("__core.actions.computer.addPoint(__core.computer.addAt(__core.computer.nextFree("
                   "__core.computer.toneOf(__core.model.page()).points), 0))")
        else:
            b.page(f"__core.actions.computer.removePoint({have})")
        poll(lambda: count(b) != have)
        time.sleep(0.15)
    return count(b) == target


def points_of(b):
    return b.page("[...document.querySelectorAll('#p-tone .pt')].map((p) => { const r = p.getBoundingClientRect();"
                  " return { n: Number(p.dataset.n), x: r.x + r.width / 2, y: r.y + r.height / 2 }; })")


def a_drag(b, d):
    name = 'a_point_drag_posts_live_and_done_at_release'
    load(b, d, 'computer')
    settle_to(b, 5)
    click(b, '#p-cm-eq')
    time.sleep(0.5)
    before = b.page(f'{COMPUTER}.eq.points[1]')
    log = len(posts(b))
    marks = points_of(b)
    two = next(p for p in marks if p['n'] == 2)
    drag_xy(b, two['x'], two['y'], two['x'] + 90, two['y'] - 25)
    moved = poll(lambda: b.page(f'{COMPUTER}.eq.points[1].f') != before['f'], 6.0)
    sent = [p for p in posts(b)[log:] if p[0] == 'api/v1/host/eq/points/2']
    check(f'{name}: live updates, then the last one done',
          len(sent) >= 2 and all(s[1].get('done') is False for s in sent[:-1]) and sent[-1][1].get('done') is True
          and all(s[2] == 200 for s in sent), sent)
    # the points are listed by frequency once the drag ends: the dragged one is where its last value puts it
    final = sent[-1][1] if sent else {}
    at = poll(lambda: b.page(f"{COMPUTER}.eq.points.findIndex((p) => Math.abs(p.f - {final.get('f', -1)}) < 0.01) + 1"))
    after = b.page(f'{COMPUTER}.eq.points[{(at or 2) - 1}]')
    check(f'{name}: the point moved up and to the right and kept its type', moved is True and after['f'] > before['f']
          and after['g'] > before['g'] and after['type'] == before['type'], [before, after])
    check(f'{name}: the row and the mark of the point in hand are drawn selected',
          poll(lambda: b.page(f"document.querySelector('#p-bands tr[data-n=\"{at}\"]').classList.contains('sel')"
                              f" && document.querySelector('#p-pt-{at}').classList.contains('sel')") is True) is True)
    cell = b.page(f"document.querySelector('#p-c{at}f').value")
    check(f'{name}: the cell reads the host', cell == f"{round(after['f'])} Hz" or after['f'] < 100, [cell, after])
    # a typed value goes as typed, within the hand ranges
    b.page(f"(() => {{ const c = document.querySelector('#p-c{at}g'); c.focus(); c.value = '99';"
           " c.dispatchEvent(new Event('input', { bubbles: true })); c.dispatchEvent(new Event('change', { bubbles: true })); })()")
    check(f'{name}: a typed gain is held to 12 dB', poll(lambda: b.page(f'{COMPUTER}.eq.points[{at - 1}].g') == 12) is True,
          b.page(f'{COMPUTER}.eq.points'))


def a_tap(b, d):
    name = 'a_tap_adds_a_point_and_x_removes_it'
    load(b, d, 'computer')
    n = count(b)
    graph = rect(b, '#p-graph')
    spots = [(graph['l'] + graph['w'] * fx, graph['t'] + graph['h'] * fy) for fx, fy in
             [(0.33, 0.85), (0.5, 0.9), (0.66, 0.1), (0.4, 0.12), (0.8, 0.8)]]
    marks = points_of(b)
    far = max(spots, key=lambda s: min(math.hypot(s[0] - m['x'], s[1] - m['y']) for m in marks))
    b.click(far[0], far[1])
    check(f'{name}: a tap on an empty spot adds one point', poll(lambda: count(b) == n + 1) is True, [n, count(b)])
    added = [p for p in posts(b) if p[0] == 'api/v1/host/eq/points']
    check(f'{name}: it is a PK at Q 1.0', len(added) == 1 and added[0][1]['type'] == 'PK' and added[0][1]['q'] == 1
          and 20 <= added[0][1]['f'] <= 20000 and -12 <= added[0][1]['g'] <= 12, added)
    time.sleep(0.4)
    # the points are listed by frequency: the new row is where its frequency puts it
    at = b.page(f"{COMPUTER}.eq.points.findIndex((p) => Math.abs(p.f - {added[0][1]['f']}) < 0.01) + 1")
    row = b.page(f"[...document.querySelectorAll('#p-bands tbody tr:nth-child({at}) td')].map((c) => c.querySelector('input')?.value ?? c.textContent.trim())")
    check(f'{name}: the new row is where its frequency lists it, PK, Q 1.00', at >= 1 and row[0] == 'PK' and row[3] == '1.00', [at, row])
    check(f'{name}: and it is the one in hand', poll(lambda: b.page(f"document.querySelector('#p-bands tbody tr:nth-child({at})').classList.contains('sel')") is True) is True)
    # the remove: its label, the cross, the route
    label = b.page(f"document.querySelector('[data-act=\"c-remove\"][data-n=\"{at}\"]').getAttribute('aria-label')")
    cross = text(b, f'[data-act="c-remove"][data-n="{at}"]')
    check(f'{name}: the remove is a cross labelled Remove point {at}', label == f'Remove point {at}' and cross == '×', [label, cross])
    click(b, f'[data-act="c-remove"][data-n="{at}"]')
    check(f'{name}: the cross removes it', poll(lambda: count(b) == n) is True, [n, count(b)])
    check(f'{name}: by the route of that point', any(p[0] == f'api/v1/host/eq/points/{at}/remove' and p[2] == 200 for p in posts(b)), posts(b))
    # `+ Add point` adds at the widest gap
    add = text(b, '#p-add')
    click(b, '#p-add')
    check(f'{name}: + Add point adds a PK at 0 dB', add == '+ Add point' and poll(lambda: count(b) == n + 1) is True
          and b.page(f'{COMPUTER}.eq.points.some((p) => p.g === 0 && p.q === 1 && p.type === "PK")') is True, [add, count(b)])


def a_sorted(b, d):
    name = 'the_points_are_listed_by_frequency_and_the_selection_follows'
    load(b, d, 'computer')
    settle_to(b, 5)
    click(b, '#p-cm-eq')
    time.sleep(0.5)
    fs = lambda: b.page(f'{COMPUTER}.eq.points.map((p) => p.f)')
    start = fs()
    check(f'{name}: the daemon lists the points by frequency', start == sorted(start), start)
    # a drag never re-sorts: point 1 goes to the far end while the others keep their numbers
    spot = rect(b, '#p-c1f')
    b.click(spot['x'], spot['y'])
    time.sleep(0.2)
    b.page("document.activeElement.blur()")
    check(f'{name}: point 1 is the one in hand', b.page("document.querySelector('#p-bands tr[data-n=\"1\"]').classList.contains('sel')") is True)
    b.page("__core.actions.computer.setPoint(1, { f: 19000 }, { done: false }).then(() => 1)")
    mid = poll(lambda: fs()[0] == 19000 and fs())
    check(f'{name}: mid-drag point 1 is still point 1 (nothing jumps under the finger)',
          mid and mid[1:] == start[1:] and b.page("document.querySelector('#p-bands tr[data-n=\"1\"]').classList.contains('sel')") is True, [start, mid])
    b.page("__core.actions.computer.setPoint(1, {}, { done: true }).then(() => 1)")
    end = poll(lambda: fs() == sorted(start[1:] + [19000]) and fs())
    check(f'{name}: at the release the table is in order', end == sorted(start[1:] + [19000]), [start, fs()])
    last = len(start)
    check(f'{name}: and the point in hand is the one that moved', poll(lambda: b.page(
        f"document.querySelector('#p-bands tr[data-n=\"{last}\"]').classList.contains('sel')") is True) is True,
          b.page("[...document.querySelectorAll('#p-bands tr.sel')].map((r) => r.dataset.n)"))
    check(f'{name}: the # column counts the sorted order',
          b.page("[...document.querySelectorAll('#p-bands tbody tr')].map((r) => r.querySelector('th').textContent.trim())")
          == [str(i + 1) for i in range(last)])


def sixteen(b, d):
    name = 'sixteen_points_keep_the_layout'
    load(b, d, 'computer')

    check(f'{name}: seven points', settle_to(b, 7), count(b))
    time.sleep(0.5)
    seven = b.page("({ tone: document.querySelector('#p-tone').getBoundingClientRect().height,"
                   " page: document.documentElement.scrollHeight, cols: document.querySelector('.p-cols').getBoundingClientRect().height })")
    more7 = text(b, '#p-more')
    check(f'{name}: seven points say one more below', more7 == 'one more below', more7)
    check(f'{name}: sixteen points', settle_to(b, 16), count(b))
    time.sleep(0.5)
    sixteen_ = b.page("({ tone: document.querySelector('#p-tone').getBoundingClientRect().height,"
                      " page: document.documentElement.scrollHeight, cols: document.querySelector('.p-cols').getBoundingClientRect().height })")
    check(f'{name}: the tone section is as tall with 16 as with 7', abs(seven['tone'] - sixteen_['tone']) <= 2, [seven, sixteen_])
    check(f'{name}: and so is the page', abs(seven['page'] - sixteen_['page']) <= 2, [seven, sixteen_])
    box = b.page("(() => { const s = document.querySelector('#p-ptsbox'); return { h: s.clientHeight, all: s.scrollHeight,"
                 " over: getComputedStyle(s).overflowY }; })()")
    check(f'{name}: the table scrolls', box['all'] > box['h'] and box['over'] == 'auto', box)
    check(f'{name}: it says ten more below, and no + Add point', text(b, '#p-more') == '10 more below'
          and b.page("!!document.querySelector('#p-add')") is False, text(b, '#p-more'))
    b.page("(() => { const s = document.querySelector('#p-ptsbox'); s.scrollTop = s.scrollHeight; })()")
    time.sleep(0.4)
    check(f'{name}: at the end nothing is below', b.page("document.querySelector('#p-more').hidden") is True)
    last = b.page("(() => { const s = document.querySelector('#p-ptsbox').getBoundingClientRect();"
                  " const r = document.querySelector('#p-bands tbody tr:last-child').getBoundingClientRect();"
                  " return r.bottom <= s.bottom + 1; })()")
    check(f'{name}: the sixteenth row is in view', last is True)
    line = text(b, '.p-line-count')
    check(f'{name}: the line counts them', ' '.join((line or '').split()).startswith('16 of 16 points · preamp '), line)


POINTER = ("(window.__ptr = (type, el, x, y, buttons) => el.dispatchEvent(new PointerEvent(type, { bubbles: true,"
           " cancelable: true, pointerId: 7, pointerType: 'mouse', isPrimary: true, button: 0, buttons,"
           " clientX: x, clientY: y })), 1)")


def a_flip_mid_drag(b, d):
    """The pointer is held with events from the page (both browsers: the handlers read only the
    pointer's id, place and kind)."""
    name = 'a_flip_mid_drag_writes_nothing_more_than_the_last_value'
    load(b, d, 'computer')
    b.page(POINTER)
    settle_to(b, 5)
    click(b, '#p-cm-eq')
    time.sleep(0.5)
    two = next(p for p in points_of(b) if p['n'] == 2)
    b.page(f"(() => {{ const el = document.querySelector('#p-pt-2'); __ptr('pointerdown', el, {two['x']}, {two['y']}, 1);"
           f" for (let i = 1; i <= 8; i++) __ptr('pointermove', el, {two['x']} + i * 9, {two['y']} - i * 3, 1); return 1; }})()")
    time.sleep(0.5)
    live = [p for p in posts(b) if p[0] == 'api/v1/host/eq/points/2' and p[1].get('done') is False]
    check(f'{name}: the drag is on: live updates were posted before the flip', len(live) >= 1, posts(b))
    log = len(posts(b))
    b.page("(__core.computer.flip('dac'), 1)")
    time.sleep(0.8)
    between = posts(b)[log:]
    last = {k: v for k, v in live[-1][1].items() if k != 'done'} if live else {}
    check(f'{name}: the flip wrote exactly one done carrying the last value sent',
          len(between) == 1 and between[0][0] == 'api/v1/host/eq/points/2' and between[0][1].get('done') is True
          and all(between[0][1].get(k) == v for k, v in last.items()), [between, last])
    settled = len(posts(b))
    b.page(f"(() => {{ const el = document.querySelector('#p-graph') || document.body; __ptr('pointermove', el, {two['x']} + 120, {two['y']} - 40, 1);"
           f" __ptr('pointerup', el, {two['x']} + 120, {two['y']} - 40, 0); return 1; }})()")
    time.sleep(0.6)
    check(f'{name}: the pointer going on and letting go writes nothing', len(posts(b)) == settled, posts(b)[settled:])
    check(f'{name}: the DAC side is shown', b.page("__core.computer.side()") == 'dac'
          and b.page("document.querySelector('#p-ruler').classList.contains('pct')") is False)
    click(b, '[data-act="side"][data-side="computer"]')
    check(f'{name}: and the table is drawn from the host when it returns',
          b.page("document.querySelectorAll('#p-bands tbody tr').length") == 5)


def curve_gap(b, n):
    """How far (viewBox units) the dot of point `n` is from the drawn curve at its place."""
    return b.page(f"""(() => {{
      const dot = document.querySelector('#p-pt-{n}');
      const x = Number(dot.dataset.x) * 1000, y = Number(dot.dataset.y) * 260;
      const pts = document.querySelector('.p-curve').getAttribute('d').match(/[ML]([\\d.]+) ([\\d.-]+)/g)
        .map((t) => t.slice(1).split(' ').map(Number));
      let best = Infinity;
      for (let i = 0; i + 1 < pts.length; i++) {{
        const [x0, y0] = pts[i], [x1, y1] = pts[i + 1];
        if (x < x0 || x > x1) continue;
        const at = y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        best = Math.min(best, Math.abs(at - y));
      }}
      return best;
    }})()""")


def a_shelf(b, d):
    name = 'a_shelf_and_every_other_dot_sit_on_the_curve'
    load(b, d, 'computer')
    b.page(POINTER)
    b.page("__core.actions.computer.setMode('eq')")
    b.page("__core.actions.computer.addPoint({ type: 'LSC', f: 105, g: 5.5, q: 0.7 })")
    poll(lambda: count(b) >= 2)
    b.page("__core.actions.computer.addPoint({ type: 'PK', f: 130, g: -4, q: 1.4 })")
    poll(lambda: count(b) >= 3)
    time.sleep(0.8)
    ns = list(range(1, count(b) + 1))
    gaps = [curve_gap(b, n) for n in ns]
    check(f'{name}: at rest, each dot is on the line (within 2 units)', all(g <= 2 for g in gaps), gaps)
    shelf = next(n for n in ns if b.page(f'{COMPUTER}.eq.points[{n - 1}].type') == 'LSC')
    dy = b.page(f"Number(document.querySelector('#p-pt-{shelf}').dataset.y) * 260")
    written = b.page(f"(() => 130 - {COMPUTER}.eq.points[{shelf - 1}].g * 10)()")
    check(f'{name}: the shelf\'s dot is not at its written gain', abs(dy - written) > 3, [dy, written])
    # during a drag, the held dot and the others stay on the line
    mark = next(p for p in points_of(b) if p['n'] == shelf)
    b.page(f"(() => {{ const el = document.querySelector('#p-pt-{shelf}'); __ptr('pointerdown', el, {mark['x']}, {mark['y']}, 1);"
           f" for (let i = 1; i <= 8; i++) __ptr('pointermove', el, {mark['x']} + i * 7, {mark['y']} + i * 2, 1); return 1; }})()")
    time.sleep(0.4)
    moved = [p for p in posts(b) if p[0] == f'api/v1/host/eq/points/{shelf}' and p[1].get('done') is False]
    check(f'{name}: the drag is on (live updates were posted)', len(moved) >= 1, posts(b)[-4:])
    held = [curve_gap(b, n) for n in ns]
    check(f'{name}: held in a drag, every dot is on the line', all(g <= 2 for g in held), held)
    b.page(f"__ptr('pointerup', document.querySelector('#p-pt-{shelf}'), {mark['x']} + 56, {mark['y']} + 16, 0)")
    time.sleep(0.8)
    settled = [curve_gap(b, n) for n in range(1, count(b) + 1)]
    check(f'{name}: and after the release', all(g <= 2 for g in settled), settled)
    if browser == 'chromium':
        b.resize(1280, 900)
        with open(f'{SHOTS}/computer-shelf-desktop.png', 'wb') as f:
            f.write(b.shot())


def a_focus(b, d):
    name = 'the_switch_and_the_cross_keep_the_focus_across_a_keyboard_use'
    load(b, d)
    b.page("document.querySelector('#p-side-computer').focus()")
    b.press('Enter')
    time.sleep(0.5)
    active = b.page("document.activeElement.id")
    check(f'{name}: Enter on this computer flips and the focus stays there', b.page("__core.computer.side()") == 'computer'
          and active == 'p-side-computer', active)
    b.press('Enter')
    time.sleep(0.5)
    check(f'{name}: and a second Enter does not flip back', b.page("__core.computer.side()") == 'computer')
    settle_to(b, 4)
    time.sleep(0.5)
    b.page("document.querySelector('#p-crm-3').focus()")
    b.press('Enter')
    check(f'{name}: the cross removes point 3', poll(lambda: count(b) == 3) is True, count(b))
    time.sleep(0.5)
    active = b.page("document.activeElement.id")
    check(f'{name}: the focus is on the cross of the row now third', active == 'p-crm-3', active)


def preset_names(b):
    return b.page(f"{COMPUTER}.presets.filter((p) => !p.built_in).map((p) => p.name)")


def untouched(b, d):
    name = 'an_untouched_cell_writes_nothing'
    load(b, d, 'computer')
    cell = json.dumps('#p-bands tbody tr:nth-child(1) .p-cell[data-param="g"]')
    key = ("(k, pre) => { if (pre) pre(); const i = document.querySelector(" + cell + "); i.focus();"
           " i.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })); }")
    b.page('window.__posts.length = 0')
    b.page(f"({key})('Enter')")
    time.sleep(0.5)
    check(f'{name}: Enter on a cell as it was posts nothing', host_posts(b) == [], host_posts(b))
    # a point beyond the hand range shows its value as it is (the cell is set as a preset's would be)
    b.page(f"({key})('Enter', () => {{ const h = __core.model.page().host;"
           " const pts = h.computer.eq.points.map((p, k) => (k === 0 ? { ...p, g: 14 } : p));"
           " __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } });"
           " })")
    time.sleep(0.5)
    check(f'{name}: Enter on a +14 dB cell as it was posts nothing (it is not pulled in to +12)',
          host_posts(b) == [], host_posts(b))
    b.page('window.__posts.length = 0')
    # the host says the point is at +14 dB and the arrow is pressed in the same breath
    b.page("(" + key + ")('ArrowUp', () => { const h = __core.model.page().host;"
           " const pts = h.computer.eq.points.map((p, i) => (i === 0 ? { ...p, g: 14 } : p));"
           " __core.model.emit('host', { ...h, computer: { ...h.computer, eq: { ...h.computer.eq, points: pts } } }); })")
    time.sleep(0.5)
    check('a_step_against_its_key_does_nothing: ArrowUp on a +14 dB point posts nothing',
          host_posts(b) == [], host_posts(b))


def an_edit(b, d):
    name = 'presets_save_rename_delete_from_the_computer_side'
    load(b, d, 'computer')
    check(f'{name}: no edit word while no preset is the person\'s own', b.page("!!document.querySelector('#p-c-edit')") is False)
    click(b, '#p-c-save')
    b.page("(() => { const i = document.querySelector('#p-presets .p-input'); i.value = 'Mine'; i.form.requestSubmit(); })()")
    check(f'{name}: saved from the table', poll(lambda: 'Mine' in (preset_names(b) or [])) is True, preset_names(b))
    check(f'{name}: the edit word appears', poll(lambda: b.page("!!document.querySelector('#p-c-edit')")) is True)
    click(b, '#p-c-edit')
    b.page("document.querySelector('.plist .p-opt[data-value=\"rename:Mine\"]').click()")
    time.sleep(0.4)
    b.page("(() => { const i = document.querySelector('#p-presets .p-input'); i.value = 'Mine two'; i.form.requestSubmit(); })()")
    check(f'{name}: renamed', poll(lambda: preset_names(b) == ['Mine two']) is True, preset_names(b))
    click(b, '#p-c-edit')
    b.page("document.querySelector('.plist .p-opt[data-value=\"delete:Mine two\"]').click()")
    time.sleep(0.4)
    asked = text(b, '#p-presets .p-question')
    check(f'{name}: delete asks in Paper\'s own line', 'Delete' in (asked or ''), asked)
    b.page("document.querySelector('#p-presets [data-answer=\"delete\"]').click()")
    check(f'{name}: deleted', poll(lambda: preset_names(b) == []) is True, preset_names(b))
    routes = [p[0].rsplit('/', 2)[-2] + '/' + p[0].rsplit('/', 1)[-1] for p in posts(b)]
    check(f'{name}: by the presets routes', any(r.endswith('presets/rename') for r in routes)
          and any(r.endswith('presets/delete') for r in routes), routes)


def a_guest(b, d):
    name = 'a_guest_sees_the_computer_inert'
    load(b, d)
    click(b, '[data-act="side"][data-side="computer"]')
    check(f'{name}: a guest flips and looks', b.page("__core.computer.side()") == 'computer'
          and b.page("document.querySelector('#p-ruler').classList.contains('pct')") is True)
    words = b.page("[...document.querySelectorAll('#p-volume [data-act], #p-tone [data-act], #p-presets [data-act]')]"
                   ".map((e) => [e.dataset.act, e.getAttribute('aria-disabled'), e.classList.contains('quiet')])")
    check(f'{name}: every computer word is quiet', len(words) > 6 and all(w[1] == 'true' and w[2] for w in words),
          [w for w in words if not (w[1] == 'true' and w[2])])
    cells = b.page("[...document.querySelectorAll('#p-tone .p-cell')].every((c) => c.getAttribute('aria-disabled') === 'true')")
    check(f'{name}: so is every cell', cells is True)
    switch = b.page("[...document.querySelectorAll('.p-showing .p-w')].every((e) => !e.classList.contains('quiet'))")
    check(f'{name}: the switch is not', switch is True)
    before = pct(b)
    click(b, '#p-cup')
    click(b, '#p-cm-off')
    click(b, '#p-add')
    b.page("(() => { const c = document.querySelector('#p-c1g'); c.value = '5'; c.dispatchEvent(new Event('change', { bubbles: true })); })()")
    time.sleep(0.5)
    check(f'{name}: no tap or typing writes anything, and the volume stays', posts(b) == [] and pct(b) == before, posts(b))
    toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.dataset.text)"))
    check(f'{name}: the core says why', bool(toast) and 'sign in' in ' '.join(toast).lower(), toast)
    box = rect(b, '#p-ruler')
    drag_xy(b, box['l'] + box['w'] * 0.3, box['y'], box['l'] + box['w'] * 0.6, box['y'])
    check(f'{name}: a drag on the ruler writes nothing', posts(b) == [], posts(b))


def kept(b, d):
    name = 'the_side_survives_a_reload'
    load(b, d, 'computer')
    check(f'{name}: the side is kept in the browser', b.page("localStorage.getItem('svolder.side')") == 'computer')
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    poll(lambda: b.page(f'!!{COMPUTER}?.eq'))
    time.sleep(0.6)
    check(f'{name}: the reload shows the computer side', b.page("__core.computer.side()") == 'computer'
          and b.page("document.querySelector('#p-ruler').classList.contains('pct')") is True
          and b.page("document.querySelectorAll('[data-act=\"side\"].on').length") == 1
          and b.page("document.querySelector('[data-act=\"side\"].on').dataset.side") == 'computer')
    check(f'{name}: and wrote nothing', posts(b) == [], posts(b))
    other = b.page("(() => { try { localStorage.removeItem('svolder.side'); } catch (e) {} return 1; })()")
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    check(f'{name}: without it the page starts on the DAC', other == 1 and b.page("__core.computer.side()") == 'dac')


def both(b, d):
    name = 'the_same_preset_on_both_asks_and_yes_flattens_the_dac'
    load(b, d, 'computer')
    status, _ = d.post('Gungnir 2', 'presets/recall', {'name': 'Warm'})
    time.sleep(1.0)
    playing = lambda: b.page("JSON.stringify(__core.model.device().presets?.rows?.filter((r) => r.on).map((r) => r.name))")  # noqa: E731
    check(f'{name}: the DAC plays Warm', status == 200 and 'Warm' in (playing() or ''), [status, playing()])
    click(b, '[data-act="c-preset"][data-name="Warm"]')
    card = poll(lambda: b.page("!!document.querySelector('#p-presets .p-askbox')"))
    words = b.page("document.querySelector('#p-presets .p-question')?.textContent")
    check(f'{name}: Paper asks in its own line, not the core card', card is True and 'flatten' in (words or '')
          and b.page("!!document.querySelector('.ask')") is False, words)
    b.page("document.querySelector('#p-presets [data-answer=\"keep\"]').click()")
    time.sleep(0.5)
    sent = [p for p in posts(b) if p[0] == 'api/v1/host/eq/preset']
    check(f'{name}: no sends nothing more and says nothing', len(sent) == 1 and sent[0][2] == 409
          and b.page("document.querySelectorAll('#toasts .toast').length") == 0, sent)
    click(b, '[data-act="c-preset"][data-name="Warm"]')
    poll(lambda: b.page("!!document.querySelector('#p-presets .p-askbox')"))
    b.page("document.querySelector('#p-presets [data-answer=\"apply\"]').click()")
    time.sleep(1.0)
    sent = [p for p in posts(b) if p[0] == 'api/v1/host/eq/preset']
    check(f'{name}: yes repeats with confirm and lands', len(sent) == 3 and sent[2][1].get('confirm') is True and sent[2][2] == 200, sent)
    flat = poll(lambda: 'Warm' not in (playing() or '') and b.page(f'{COMPUTER}.eq.preset') == 'Warm')
    dac = d.model('Gungnir 2')
    check(f'{name}: the DAC is flat and the daemon agrees', flat is True
          and not any(r.get('on') and r['name'] == 'Warm' for r in dac['presets']['rows']), [playing(), dac['presets']])
    status_line = ' '.join((text(b, '#p-presets .p-status') or '').split())
    check(f'{name}: the presets say Warm is on', status_line == 'Warm is on.', status_line)


def amp_page(b, d):
    name = 'the_amp_page_flips_to_the_computer_with_the_dac_parts'
    load(b, d)
    b.page("__core.actions.showDevice('Amp')")
    poll(lambda: text(b, '#p-title h1') == 'Amp')
    time.sleep(1.0)
    line = ' '.join((text(b, '#p-title .p-showing') or '').split())
    check(f'{name}: the amp\'s page has the switch', line == 'showing: the amp · this computer', line)
    own = b.page("({ power: !!document.querySelector('#p-power [data-act=\"power\"]'),"
                 " meters: !!document.querySelector('#p-meters'),"
                 " ruler: document.querySelector('#p-ruler')?.classList.contains('pct') ?? null })")
    click(b, '[data-act="side"][data-side="computer"]')
    check(f'{name}: nothing was written by the flip', posts(b) == [], posts(b))
    parts = b.page("({ pct: document.querySelector('#p-ruler')?.classList.contains('pct') ?? null,"
                   " needle: !!document.querySelector('#p-needle'), graph: !!document.querySelector('#p-graph'),"
                   " modes: document.querySelectorAll('#p-tone [data-act=\"c-mode\"]').length,"
                   " rows: document.querySelectorAll('#p-bands tbody tr').length, add: !!document.querySelector('#p-add'),"
                   " power: !!document.querySelector('#p-power [data-act=\"power\"]'), meters: !!document.querySelector('#p-meters'),"
                   " name: document.querySelector('#p-title h1')?.textContent })")
    check(f'{name}: the amp flips to the DAC page\'s computer ruler and tone', parts['pct'] is True and parts['needle']
          and parts['graph'] and parts['modes'] == 2 and parts['rows'] >= 1 and parts['add'], parts)
    check(f'{name}: the amp\'s own sections are as they were', own['power'] == parts['power'] and own['meters'] == parts['meters']
          and parts['name'] == 'Amp', [own, parts])
    start = pct(b)
    click(b, '#p-cup')
    check(f'{name}: the volume is the computer\'s, one percent', poll(lambda: pct(b) == start + 1) is True
          and any(p[0] == 'api/v1/host/volume' for p in posts(b)), [start, pct(b)])
    click(b, '[data-act="side"][data-side="dac"]')
    check(f'{name}: and back, the amp\'s own page', b.page("document.querySelector('#p-ruler')?.classList.contains('pct') ?? false") is False
          and b.page("document.querySelector('[data-act=\"c-mode\"]')") is None)


def phone(b, d):
    name = 'the_computer_side_fits_a_phone'
    os.makedirs(SHOTS, exist_ok=True)
    load(b, d, 'computer')
    settle_to(b, 7)
    click(b, '#p-cm-eq')
    time.sleep(0.5)
    if browser == 'chromium':
        b.resize(1280, 900)
        time.sleep(0.5)
        with open(f'{SHOTS}/computer-desktop.png', 'wb') as f:
            f.write(b.shot())
    b.resize(390, 900)
    b.goto(f'{d.url}?theme=paper', settle=1.0)
    b.page(BOOT)
    poll(lambda: b.page(f'!!{COMPUTER}?.eq'))
    time.sleep(0.8)
    check(f'{name}: seven points, as many rows', b.page("document.querySelectorAll('#p-bands tbody tr').length") == 7)
    wide = b.page("document.documentElement.scrollWidth - document.documentElement.clientWidth")
    check(f'{name}: no sideways scroll at 390', wide <= 0, wide)
    cells = b.page("[...document.querySelectorAll('#p-bands tbody tr:first-child td')].map((c) => c.getBoundingClientRect().width)")
    check(f'{name}: the table fits its box', b.page("document.querySelector('#p-ptsbox').scrollWidth <= document.querySelector('#p-ptsbox').clientWidth + 1") is True, cells)
    if browser == 'chromium':
        with open(f'{SHOTS}/computer-phone.png', 'wb') as f:
            f.write(b.shot())
        print('shots', f'{SHOTS}/computer-desktop.png', f'{SHOTS}/computer-phone.png')


one('the_switch_flips_volume_and_tone_together', a_switch)
one('minus_plus_mute_set_the_computer_volume', a_volume)
one('a_point_drag_posts_live_and_done_at_release', a_drag)
one('a_tap_adds_a_point_and_x_removes_it', a_tap)
one('the_points_are_listed_by_frequency_and_the_selection_follows', a_sorted)
one('sixteen_points_keep_the_layout', sixteen)
one('a_flip_mid_drag_writes_nothing_more_than_the_last_value', a_flip_mid_drag)
one('a_shelf_and_every_other_dot_sit_on_the_curve', a_shelf)
one('the_switch_and_the_cross_keep_the_focus_across_a_keyboard_use', a_focus)
one('an_untouched_cell_writes_nothing', untouched)
one('presets_save_rename_delete_from_the_computer_side', an_edit)
one('a_guest_sees_the_computer_inert', a_guest, web='guest = "read"')
one('the_side_survives_a_reload', kept)
one('the_same_preset_on_both_asks_and_yes_flattens_the_dac', both)
one('the_amp_page_flips_to_the_computer_with_the_dac_parts', amp_page, fleet=FLEET, env={'SVOLDER_FAKE_SIGNAL': '1'})
one('the_computer_side_fits_a_phone', phone)
print(f'computer_paper.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
