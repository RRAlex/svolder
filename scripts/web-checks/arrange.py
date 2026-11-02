# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Ember's Arrange mode in one browser: the sections moved between the band and the two
columns by the arrow keys and by a drag of the handle (a mouse, and a finger under Chromium's touch
emulation), the order kept per browser and per layout (a DAC's is not an amp's), Done, Reset and
Escape, the page's own drags inert meanwhile, a part swapped into a moved section, the key T, the
frame's row and key A, a theme that unmounts mid-arrange, and a phone's order (band, left, right).
Usage: python3 arrange.py chromium|firefox
Set ARRANGE_SHOTS=<dir> to keep four pictures (Chromium): the page in Arrange mode, a section held
mid-drag, the page after Done, and a phone in Arrange mode. Exits 1 on any failed check; prints one
line per check."""
import json
import os
import sys
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
shots = os.environ.get('ARRANGE_SHOTS') if browser == 'chromium' else None
fails = []
passed = 0
KEY = 'svolder.theme.ember.arrange'
ZONE_NAMES = ('band', 'left', 'right')
DEFAULT = {'band': ['playing', 'visualizer'], 'left': ['volume', 'controls'],
           'right': ['tone', 'presets']}  # what a DAC with a player draws


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(size=(1280, 900), **kw):
    if browser == 'chromium':
        return common.Chromium(size=size, args=['--disable-3d-apis'], **kw)
    return common.Firefox(size=size, extra_prefs=['user_pref("webgl.disabled", true);'])


def errors(b):
    """The page's errors, not the failed requests of a daemon that was stopped on purpose."""
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if not e.startswith('log: Failed to load resource')]


# The page's sections by name, zone by zone, as they stand in the DOM.
ZONES = """(() => {
  const view = document.querySelector('#screen .view');
  const name = (e) => (e.id ? e.id.replace('part-', '')
    : e.classList.contains('p-viz') ? 'visualizer' : e.className);
  const zone = (box) => (box ? [...box.children].map(name) : []);
  const cols = view.querySelectorAll(':scope > .col');
  return { band: zone(view.querySelector(':scope > .p-band')), left: zone(cols[0]),
    right: zone(cols[1]) };
})()"""
START = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  const a = await import(new URL('assets/themes/ember/arrange.js', document.baseURI));
  await m.mounted;
  window.__arr = a.startArrange({ root: document.getElementById('theme'), core: m.core,
    prefs: m.core.prefs, layout: m.core.model.page().device.layout });
  return window.__arr !== null; })"""
HOOK = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (u, o) => { if (o && o.method === 'POST') window.__posts.push(String(u));
    return f(u, o); }; })()"""
STATE = """(() => { const v = document.querySelector('#screen .view');
  return { arranging: v.classList.contains('arranging'), arranged: v.classList.contains('arranged'),
    bar: !!document.querySelector('.arr-bar'), handles: document.querySelectorAll('.arr-handle').length,
    inert: document.querySelectorAll('[inert]').length,
    lift: document.querySelectorAll('.arr-lift').length,
    ghost: !!document.querySelector('.arr-ghost'), drop: !!document.querySelector('.arr-drop'),
    zones: document.querySelectorAll('[data-zone]').length,
    stored: localStorage.getItem('svolder.theme.ember.arrange'),
    live: (document.querySelector('.arr-live') || {}).textContent,
    focus: document.activeElement && (document.activeElement.getAttribute('aria-label')
      || document.activeElement.id || document.activeElement.tagName) }; })()"""


def posts_of(b):
    return b.page('window.__posts')


def zones(b):
    return b.ev(ZONES)


def state(b):
    return b.ev(STATE)


def stored(b):
    text = b.ev(f"localStorage.getItem('{KEY}')")
    return json.loads(text) if text else None


def keep(b, order):
    b.ev(f"localStorage.setItem('{KEY}', {json.dumps(json.dumps(order))})")


def open_page(b, d):
    """The page with no order kept and the fetch hook in place."""
    b.goto(d.url + '?effects=off', settle=1.2)
    b.ev(f"localStorage.removeItem('{KEY}')")
    b.goto(d.url + '?effects=off', settle=1.2)
    b.page(HOOK)


def start(b):
    return b.page(START)


def handle_of(name):
    return f"[data-arr={name}] .arr-handle"


def focus_handle(b, name):
    b.ev(f"document.querySelector('{handle_of(name)}').focus()")


def post(d, path, body):
    req = urllib.request.Request(
        f'{d.url}api/v1/devices/{urllib.request.quote("Gungnir 2")}/{path}',
        data=json.dumps(body).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    urllib.request.urlopen(req, timeout=10).read()


def rect(b, selector):
    return b.ev(f"(() => {{ const r = document.querySelector({json.dumps(selector)})"
                f".getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; }})()")


# ------------------------------------------------------------------ pointers

def mouse(b, kind, x, y):
    """One step of a mouse: 'down' at (x, y), 'move' to it with the button down, 'up' there."""
    if browser == 'chromium':
        if kind == 'down':
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
            b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y,
                                                'button': 'left', 'buttons': 1, 'clickCount': 1})
        elif kind == 'move':
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y,
                                                'button': 'left', 'buttons': 1})
        else:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y,
                                                'button': 'left', 'clickCount': 1})
    else:
        acts = []
        if kind == 'down':
            acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                     'y': int(y)}, {'type': 'pointerDown', 'button': 0}]
        elif kind == 'move':
            acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                     'y': int(y)}]
        else:
            acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                     'y': int(y)}, {'type': 'pointerUp', 'button': 0}]
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})


def path(a, z, n=8):
    return [(a[0] + (z[0] - a[0]) * i / n, a[1] + (z[1] - a[1]) * i / n) for i in range(1, n + 1)]


def drag(b, a, z, hold=None):
    """A mouse drag from `a` to `z` in steps; `hold()` runs just before the button goes up."""
    mouse(b, 'down', *a)
    for p in path(a, z):
        mouse(b, 'move', *p)
    b.sleep(0.15)
    if hold:
        hold()
    mouse(b, 'up', *z)
    b.sleep(0.3)


def touch_drag(b, a, z, hold=None):
    b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': [{'x': a[0],
                                                                                 'y': a[1]}]})
    for p in path(a, z):
        b.send('Input.dispatchTouchEvent', {'type': 'touchMove',
                                            'touchPoints': [{'x': p[0], 'y': p[1]}]})
    b.sleep(0.15)
    if hold:
        hold()
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
    b.sleep(0.3)


def save(b, name):
    if shots:
        os.makedirs(shots, exist_ok=True)
        with open(f'{shots}/{name}.png', 'wb') as f:
            f.write(b.shot())


# ------------------------------------------------------------------ the checks

def default_dom(b, d):
    open_page(b, d)
    s = b.ev("""(() => { const v = document.querySelector('#screen .view');
      return { arranged: v.classList.contains('arranged'),
        top: [...v.children].map((e) => e.className.split(' ')[0] || e.id) }; })()""")
    check('the_default_order_is_todays_dom: no .arranged, the band, two columns, the fold-out',
          not s['arranged'] and s['top'] == ['p-band', 'col', 'col', 'p-facts'], s)
    got = zones(b)
    check('  the sections stand where the default order puts them', got == DEFAULT, got)
    drawn = b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
      const s = await import(new URL('assets/themes/ember/draw/screen.js', document.baseURI));
      const html = s.screen(m.core.model.page());
      return [...html.matchAll(/ id="part-([a-z-]+)"| class="(p-viz)"/g)]
        .map((x) => x[1] || 'visualizer').filter((x) => !['facts', 'bands'].includes(x)); })""")
    listed = b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then((m) =>
      m.core.theme.sections.map((x) => `${x.id}=${x.label}`))""")
    check('  the theme lists the sections Arrange moves, with their words',
          listed == ['playing=Now playing', 'visualizer=Visualizer', 'volume=Volume',
                     'controls=Controls', 'power=Power', 'tone=Tone control', 'presets=Presets',
                     'device-presets=Device presets', 'meters=Meters'], listed)
    check('  and it is the screen the default order draws, section for section',
          drawn == sum(got.values(), []), [drawn, got])


def every_section(b, d):
    open_page(b, d)
    bad = []
    flip = False
    n = 0
    got = None
    for name in ['playing', 'visualizer', 'volume', 'controls', 'tone', 'presets']:
        for zone in ZONE_NAMES:
            flip = not flip
            n += 1
            started = start(b)
            b.page(f"(() => {{ window.__arr.moveTo('{name}', '{zone}', 0); return true; }})()")
            b.page("(() => { window.__arr.done(); return true; })()")
            d.write('Gungnir 2', 'phase', flip)
            raw = 30 + n
            post(d, 'volume', {'raw': raw})
            ok = False
            for _ in range(20):
                b.sleep(0.1)
                got = b.ev("""(() => ({ phase: document.querySelector(
                  '#part-controls [data-key=phase]').getAttribute('aria-checked'),
                  raw: document.querySelector('sv-knob').raw }))()""")
                if got['phase'] == str(flip).lower() and got['raw'] == raw:
                    ok = True
                    break
            where = zones(b)
            if not started or not ok or name not in where[zone]:
                bad.append([name, zone, got, where])
    check('every_movable_section_in_every_zone_keeps_the_page_working: after Done a control '
          'write and a volume write both show within 2 s, in the section wherever it stands',
          not bad, bad[:2])
    errs = errors(b)
    check('  with no page error', errs == [], errs)


def keyboard(b, d):
    open_page(b, d)
    b.press('A')
    s = state(b)
    check('A on the page starts Arrange mode: the bar, a handle on every section, the first '
          'handle focused, the sections\' children inert',
          s['arranging'] and s['bar'] and s['handles'] == 6 and s['focus'] == 'Move Now playing'
          and s['inert'] > 6 and s['zones'] == 3, s)
    focus_handle(b, 'volume')
    b.press('ArrowRight')
    got = zones(b)
    s = state(b)
    check('the_keyboard_moves_a_section: Volume, Right, is first in the right column',
          got['right'][0] == 'volume' and got['left'] == ['controls'], got)
    check('  the handle keeps the focus and the live region says where it stands',
          s['focus'] == 'Move Volume' and s['live'] == 'Volume: right column, 1 of 3', s)
    b.press('ArrowUp')
    check('  Up at the top of its column stays', zones(b)['right'][0] == 'volume')
    b.press('ArrowLeft')
    got = zones(b)
    check('  Left goes back to the left column at the same place',
          got['left'] == ['volume', 'controls'] and got['right'] == ['tone', 'presets'], got)
    b.press('ArrowDown')
    check('  Down goes under the next section', zones(b)['left'] == ['controls', 'volume'])
    posts = posts_of(b)
    check('  none of these keys wrote to the device', posts == [], posts)
    b.press('Escape')
    s = state(b)
    check('escape_restores_the_order_from_before: the mode ends, nothing stored',
          zones(b) == DEFAULT and not s['bar']
          and s['handles'] == 0 and s['inert'] == 0 and s['stored'] is None, [zones(b), s])


def dragging(b, d, touch=False):
    open_page(b, d)
    start(b)
    a = common.center(b, handle_of('tone'))
    band = rect(b, '.p-band')
    viz = rect(b, '.p-viz')
    z = (viz[0] + 24, band[1] + 40)
    seen = {}

    def hold():
        seen.update(b.ev("""(() => { const q = (s) => document.querySelector(s);
          const lift = q('.arr-lift'); const drop = q('.arr-drop').getBoundingClientRect();
          return { lift: lift && lift.dataset.arr, ghost: !!q('.arr-ghost'),
            drop: [drop.left, drop.top, drop.width, drop.height],
            moved: lift.getBoundingClientRect().top - q('.arr-ghost').getBoundingClientRect().top };
        })()"""))
        seen['order'] = zones(b)
        if not touch:
            save(b, 'dragging')

    (touch_drag if touch else drag)(b, a, z, hold)
    who = 'a finger' if touch else 'a mouse'
    check(f'{who} drag by the handle lifts the section: the lift, the ghost of its old place and '
          'the drop line between Now playing and the visualizer (vertical, the height of the '
          'band\'s sections), the DOM unchanged until it lands',
          seen.get('lift') == 'tone' and seen.get('ghost') and abs(seen['moved']) > 20
          and seen['drop'][3] > 100 and seen['drop'][2] <= 5
          and seen['order']['right'] == ['tone', 'presets'], seen)
    got = zones(b)
    check(f'a_drag_by_the_handle_moves_tone_into_the_band ({"touch" if touch else "mouse"}): it lands '
          'there, Tone control in the band between the two',
          got['band'] == ['playing', 'tone', 'visualizer'] and got['right'] == ['presets'], got)
    fit = b.ev("""(() => { const t = document.querySelector('#part-playing .np-title');
      const tops = [...document.querySelector('.p-band').children].map((e) => Math.round(
        e.getBoundingClientRect().top));
      return { cut: t.scrollWidth > t.clientWidth + 1, title: t.textContent.trim(), tops }; })()""")
    check('  with three sections in the band at 1280 px they share one row and the Now playing '
          'title is not cut', not fit['cut'] and len(set(fit['tops'])) == 1, fit)
    s = state(b)
    check('  the lift, the ghost and the line are gone, and the handle has the focus',
          s['lift'] == 0 and not s['ghost'] and not s['drop'] and s['focus'] == 'Move Tone control',
          s)
    b.page("(() => { document.querySelector('.arr-done').click(); return true; })()")
    kept = stored(b)
    check('  Done keeps it for this layout, in this browser (a section the DAC does not draw '
          'keeps its place in the order)',
          kept == {'dac': {'band': ['playing', 'tone', 'visualizer'],
                           'left': ['volume', 'controls', 'power'],
                           'right': ['presets', 'device-presets']}}, kept)
    s = state(b)
    check('  and the page is marked arranged, the marks gone',
          s['arranged'] and not s['bar'] and s['handles'] == 0 and s['inert'] == 0, s)
    if not touch:
        save(b, 'after-done')


def reset_and_escape(b, d):
    open_page(b, d)
    keep(b, {'dac': {'band': ['tone', 'playing', 'visualizer'], 'left': ['controls', 'volume'],
                     'right': ['presets']}})
    b.goto(d.url + '?effects=off', settle=1.2)
    b.press('A')
    before = zones(b)
    check('a kept order is drawn on a load', before['band'] == ['tone', 'playing', 'visualizer']
          and before['left'] == ['controls', 'volume'], before)
    x, y = common.center(b, '.arr-reset')
    b.click(x, y)
    got = zones(b)
    check('reset_puts_the_default_back: at once, and the mode goes on',
          got == {'band': ['playing', 'visualizer'], 'left': ['volume', 'controls'],
                  'right': ['tone', 'presets']} and state(b)['arranging'], got)
    check('  but keeps nothing until Done: the stored order stands', stored(b) is not None)
    b.press('Escape')
    check('escape_restores_the_order_from_before: the kept order, after a Reset',
          zones(b) == before, zones(b))
    b.press('A')
    x, y = common.center(b, '.arr-reset')
    b.click(x, y)
    x, y = common.center(b, '.arr-done')
    b.click(x, y)
    s = state(b)
    check('Reset, then Done, forgets the order: nothing stored, the page not marked arranged',
          s['stored'] is None and not s['arranged'] and not s['bar'], s)
    b.press('A')
    focus_handle(b, 'volume')
    b.press('ArrowRight')
    b.press('Escape')
    check('Escape after a move stores nothing', state(b)['stored'] is None, state(b))


def survives(b, d):
    open_page(b, d)
    order = {'band': ['playing', 'tone', 'visualizer'], 'left': ['volume', 'controls'],
             'right': ['presets']}
    keep(b, {'dac': order})
    b.goto(d.url + '?effects=off', settle=1.2)
    check('the_order_survives_a_reload_and_a_screen_swap: a reload', zones(b) == order and state(b)['arranged'], zones(b))
    # a switch to the amp and back draws the DAC's screen anew
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then((m) => {
      m.core.actions.showDevice(m.core.model.page().fleet.devices[1].url); return true; })""")
    b.sleep(1.5)
    amp = zones(b)
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then((m) => {
      m.core.actions.showDevice(m.core.model.page().fleet.devices[0].url); return true; })""")
    b.sleep(1.5)
    check('the_order_survives_a_reload_and_a_screen_swap: a switch to the amp and back',
          zones(b) == order, [amp, zones(b)])
    check('  the amp is drawn in its own default order, not the DAC\'s',
          amp['left'] == ['meters'] and amp['right'] == ['controls'], amp)


def leak(b, d):
    open_page(b, d)
    start(b)
    b.page("(() => { window.__arr.moveTo('tone', 'band', 0); window.__arr.done(); return true; })()")
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then((m) => {
      m.core.actions.showDevice(m.core.model.page().fleet.devices[1].url); return true; })""")
    b.sleep(1.5)
    amp = zones(b)
    check('a_dac_order_does_not_leak_to_the_amp', amp['band'] == ['playing', 'visualizer']
          and amp['left'] == ['meters'] and amp['right'] == ['controls'], amp)
    check('  and the page is not marked arranged', not state(b)['arranged'], state(b))
    start(b)
    b.page("(() => { window.__arr.moveTo('meters', 'right', 1); window.__arr.done(); return true; })()")
    kept = stored(b)
    check('  the amp\'s order is kept beside the DAC\'s (the amp does not draw a power button: it '
          'keeps its place in the order)',
          set(kept) == {'dac', 'amp'} and kept['amp']['right'] == ['power', 'controls', 'meters']
          and zones(b)['right'] == ['controls', 'meters'] and kept['dac']['band'][0] == 'tone',
          [kept, zones(b)])
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then((m) => {
      m.core.actions.showDevice(m.core.model.page().fleet.devices[0].url); return true; })""")
    b.sleep(1.5)
    check('  back on the DAC its order stands', zones(b)['band'][0] == 'tone', zones(b))


def inert(b, d):
    open_page(b, d)
    start(b)
    k = b.ev("""(() => { const k = document.querySelector('sv-knob'); const g = k.geometry();
      return { raw: k.raw, x: g.cx, y: g.cy - 0.9 * g.radius, r: g.radius }; })()""")
    drag(b, (k['x'], k['y']), (k['x'] + 60, k['y'] + 20))
    point = '#part-tone circle.mk'
    has_point = b.ev(f"!!document.querySelector('{point}')")
    if has_point:
        x, y = common.center(b, point)
        drag(b, (x, y), (x + 30, y - 40))
    b.ev("document.querySelector('#part-controls [data-key=phase]').scrollIntoView({block: 'center'})")
    x, y = common.center(b, '#part-controls [data-key=phase]')
    b.click(x, y)
    after = b.ev("""(() => ({ raw: document.querySelector('sv-knob').raw,
      phase: document.querySelector('#part-controls [data-key=phase]').getAttribute('aria-checked'),
      }))()""")
    check('while_arranging_the_knob_and_the_curve_are_inert: a drag on the dial, '
          'a drag on the curve and a click on a switch post nothing and change nothing',
          has_point and posts_of(b) == [] and after['raw'] == k['raw']
          and after['phase'] == 'false', [has_point, after])
    b.press('ArrowRight')
    check('  the arrow keys on the page do not step the volume either',
          posts_of(b) == [], posts_of(b))
    b.page("(() => { window.__arr.done(); return true; })()")
    b.press('ArrowRight')
    b.sleep(0.5)
    posts = posts_of(b)
    check('  after Done the same key steps the volume again', len(posts) == 1
          and posts[0].endswith('/volume'), posts)


def swap_lands(b, d):
    open_page(b, d)
    start(b)
    b.page("(() => { window.__arr.moveTo('controls', 'right', 0); return true; })()")
    d.write('Gungnir 2', 'phase', True)
    got = None
    for _ in range(20):
        b.sleep(0.1)
        got = b.ev("""(() => { const c = document.querySelector('#part-controls');
          const cols = document.querySelectorAll('#screen .view > .col');
          return { where: c.parentElement === cols[1],
            phase: c.querySelector('[data-key=phase]').getAttribute('aria-checked'),
            handle: c.firstElementChild.classList.contains('arr-handle'),
            inert: [...c.children].filter((e) => e !== c.firstElementChild)
              .every((e) => e.hasAttribute('inert')) }; })()""")
        if got['phase'] == 'true':
            break
    check('a part swapped while arranging lands in the moved section by id and is given its '
          'handle and its inert children again', got == {'where': True, 'phase': 'true',
                                                         'handle': True, 'inert': True}, got)
    focus_handle(b, 'controls')
    d.write('Gungnir 2', 'phase', False)
    b.sleep(1.0)
    check('  a swap of the section holding the focused handle keeps the focus on its handle',
          state(b)['focus'] == 'Move Controls', state(b))
    b.press('ArrowLeft')
    check('  and its handle still moves it', zones(b)['left'][0] == 'controls', zones(b))
    b.page("(() => { window.__arr.cancel(); return true; })()")


def shape_change(b, d):
    order = {'band': ['tone', 'playing', 'visualizer'], 'left': ['controls', 'volume'],
             'right': ['presets']}
    open_page(b, d)
    keep(b, {'dac': order})
    d.restart()
    b.goto(d.url + '?effects=off', settle=1.0)
    shape = b.ev("document.querySelector('#screen .view').dataset.shape")
    link = None
    for _ in range(50):
        try:
            link = d.model()['link']['state']
        except Exception:
            link = None
        if link == 'on':
            break
        b.sleep(0.2)
    check('a device that goes away: it is connected before the gesture', link == 'on', link)
    d.drop()
    now = None
    for _ in range(80):
        now = b.ev("document.querySelector('#screen .view').dataset.shape")
        if now != shape:
            break
        b.sleep(0.25)
    gone = zones(b)
    check('the_order_survives_a_reload_and_a_screen_swap: a screen that changes shape (the device '
          'goes away) is drawn in the kept order',
          now != shape and gone['band'] == ['tone', 'playing', 'visualizer']
          and state(b)['arranged'], [shape, now, gone])
    locked = b.ev("document.querySelectorAll('#screen .view > .col[inert]').length")
    b.press('A')
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    hidden = b.ev("document.getElementById('menu-arrange').hidden")
    b.press('Escape')
    check('a device whose link is lost (its columns inert) cannot be arranged: A does nothing and '
          'the menu does not offer the row', locked == 2 and not state(b)['arranging']
          and not state(b)['bar'] and hidden is True, [locked, state(b), hidden])
    d.restart()
    back = None
    for _ in range(40):
        b.sleep(0.2)
        back = b.ev("document.querySelector('#screen .view').dataset.shape")
        if back == shape:
            break
    check('  and so is the screen when the device comes back', back == shape
          and zones(b) == order, [back, zones(b)])
    b.press('A')
    check('  Arrange mode starts there', state(b)['arranging'], state(b))
    link = d.model()['link']['state']
    check('  the device is connected before it goes away under the mode', link == 'on', link)
    d.drop()
    for _ in range(60):
        if not state(b)['arranging']:
            break
        b.sleep(0.25)
    toasts = b.ev("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)")
    s = state(b)
    check('  and when the screen changes shape under it, the mode ends: the order from before, '
          'nothing stored, a popup says so', not s['bar'] and s['handles'] == 0
          and b.ev('document.querySelectorAll("[data-arr-inert]").length') == 0
          and json.loads(s['stored'])['dac'] == order
          and any('arranging stopped' in t for t in toasts), [s, toasts])
    errs = errors(b)
    check('  with no page error', errs == [], errs)


def lost_capture(b, d):
    """A handle that could not capture the pointer: the drag still ends where the button is let
    go, outside the screen."""
    open_page(b, d)
    b.page("(() => { Element.prototype.setPointerCapture = () => {}; return true; })()")
    start(b)
    a = common.center(b, handle_of('tone'))
    seen = {}

    def hold():
        seen.update(state(b))

    drag(b, a, (a[0], 20), hold)
    s = state(b)
    check('a drag whose pointer capture fails still ends where the button is let go: nothing '
          'stays lifted, no line, no ghost, and the handle works again',
          seen['lift'] == 1 and s['lift'] == 0 and not s['ghost'] and not s['drop']
          and s['arranging'], [seen, s])
    focus_handle(b, 'volume')
    b.press('ArrowRight')
    check('  and the keyboard still moves a section', zones(b)['right'][0] == 'volume', zones(b))
    b.press('Escape')


def picker(b, d):
    open_page(b, d)
    b.press('A')
    focus_handle(b, 'volume')
    before = zones(b)
    b.press('T')
    s = b.ev("""(() => { const p = document.getElementById('picker');
      return { open: !!p, modal: p && p.getAttribute('aria-modal') }; })()""")
    check('t_opens_the_picker_while_arranging: with a handle focused', s['open'], s)
    check('  and leaves the order as it was', zones(b) == before and state(b)['bar'], zones(b))
    b.press('Escape')
    s = state(b)
    check('  Escape closes the picker and the mode goes on', s['bar'] and not b.ev(
        "!!document.getElementById('picker')"), s)
    b.press('Escape')
    check('  a second Escape ends the mode', not state(b)['bar'], state(b))
    errs = errors(b)
    check('  with no page error', errs == [], errs)


def frame_row(b, d):
    open_page(b, d)
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    row = b.ev("""(() => { const r = document.getElementById('menu-arrange');
      return r && { text: r.textContent, role: r.getAttribute('role'), hidden: r.hidden,
        key: r.querySelector('.mkey').textContent }; })()""")
    check('the frame\'s menu has the row "Arrange sections…" with its key A',
          row == {'text': 'Arrange sections…A', 'role': 'menuitem', 'hidden': False, 'key': 'A'},
          row)
    x, y = common.center(b, '#menu-arrange')
    b.click(x, y)
    b.sleep(0.3)
    s = state(b)
    check('  choosing it starts Arrange mode and focuses a handle',
          s['arranging'] and s['focus'] == 'Move Now playing', s)
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    hidden = b.ev("document.getElementById('menu-arrange').hidden")
    check('  while arranging the row is not offered', hidden is True, hidden)
    b.press('Escape')
    b.press('Escape')
    check('  (menu closed, then the mode ended)', not state(b)['bar'], state(b))


def unmount(b, d):
    open_page(b, d)
    keep(b, {'dac': {'band': ['playing', 'visualizer'], 'left': ['controls', 'volume'],
                     'right': ['tone', 'presets']}})
    b.goto(d.url + '?effects=off', settle=1.2)
    before_inert = b.ev('document.querySelectorAll("[inert]").length')
    b.press('A')
    focus_handle(b, 'volume')
    b.press('ArrowRight')
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
      await (await m.mounted).unmount(); return true; })""")
    b.sleep(0.5)
    left = b.ev("""(() => ({ bar: document.querySelectorAll('.arr-bar, .arr-handle, .arr-ghost, '
      + '.arr-drop').length, inert: document.querySelectorAll('[inert]').length,
      stored: localStorage.getItem('svolder.theme.ember.arrange') }))()""")
    check('a theme that unmounts mid-arrange leaves no mark, no inert and the stored order as it '
          'was', left['bar'] == 0 and left['inert'] == 0 and json.loads(left['stored'])['dac'][
              'left'] == ['controls', 'volume'], left)
    b.page("""import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
      const registry = await import(new URL('assets/core/registry.js', document.baseURI));
      await registry.mountTheme({ registry: m.core.registry, id: 'ember',
        root: document.getElementById('theme'), core: m.core, toast: m.core.frame.toast });
      return true; })""")
    b.sleep(1.0)
    b.press('A')
    s = state(b)
    check('  mounted again, A starts a fresh Arrange mode and Done keeps what it moved',
          s['arranging'] and s['handles'] == 6 and before_inert == 0, [s, before_inert])
    b.press('Escape')
    errs = errors(b)
    check('  with no page error', errs == [], errs)


def viz_switch(b, d):
    open_page(b, d)
    keep(b, {'dac': {'band': ['playing'], 'left': ['volume', 'controls'],
                     'right': ['tone', 'visualizer', 'presets']}})
    b.goto(d.url + '?effects=off', settle=1.2)
    check('a visualizer kept in the right column is drawn there',
          zones(b)['right'] == ['tone', 'visualizer', 'presets'], zones(b))
    for want in ('off', 'on'):
        x, y = common.center(b, '.fxbtn')
        b.click(x, y)
        x, y = common.center(b, '.fxpop [data-switch=viz]')
        b.click(x, y)
        b.sleep(0.4)
    check('  switched off and on again it comes back to the same place',
          zones(b)['right'] == ['tone', 'visualizer', 'presets'], zones(b))
    b.press('A')
    check('  and a handle is on it while arranging', state(b)['handles'] == 6, state(b))
    b.press('Escape')
    # the switch used while arranging puts the card where the order being arranged has it
    b.goto(d.url + '?effects=off', settle=1.2)
    b.ev(f"localStorage.removeItem('{KEY}')")
    b.goto(d.url + '?effects=off', settle=1.2)
    start(b)
    b.page("(() => { window.__arr.moveTo('visualizer', 'right', 1); return true; })()")
    for _ in ('off', 'on'):
        x, y = common.center(b, '.fxbtn')
        b.click(x, y)
        x, y = common.center(b, '.fxpop [data-switch=viz]')
        b.click(x, y)
        b.sleep(0.4)
    got = zones(b)
    check('  and used while arranging it puts the card where the arrangement has it, not where the '
          'kept order does, with its handle', got['right'] == ['tone', 'visualizer', 'presets']
          and got['band'] == ['playing'] and state(b)['handles'] == 6, [got, state(b)])
    b.page("(() => { window.__arr.done(); return true; })()")
    check('  and Done keeps it there', stored(b)['dac']['right'][:3] == ['tone', 'visualizer', 'presets'],
          stored(b))


PRESSES = """(() => { window.__presses = []; const f = window.fetch;
  window.fetch = (u, o) => { const p = f(u, o);
    if (o && o.method === 'POST' && String(u).includes('host/press')) {
      p.then((r) => window.__presses.push([String(o.body), r.status])); }
    return p; }; })()"""


def previous_stays(b, d):
    """Previous-track stays visible and clickable in a narrow Now playing cell, and a wide cell has
    the whole card: three sections in the band at 1280 px and at 900 px (two per row), two at 1280."""
    def look():
        return b.ev("""(() => { const q = (s) => document.querySelector(s);
          const prev = q('#part-playing .np-btn.prev'); const r = prev.getBoundingClientRect();
          const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          const cover = getComputedStyle(q('#part-playing .np-cover')).display;
          const view = q('#screen .view').getBoundingClientRect();
          return { shown: getComputedStyle(prev).display !== 'none' && r.width > 20,
            hit: !!at && !!at.closest('.np-btn.prev'), cover,
            inside: r.left >= view.left && r.right <= view.right,
            cell: Math.round(q('#part-playing').getBoundingClientRect().width),
            title: (() => { const t = q('#part-playing .np-title');
              return t.scrollWidth <= t.clientWidth + 1; })() }; })()""")

    def press(label, narrow):
        b.page(PRESSES)
        s = look()
        x, y = common.center(b, '#part-playing .np-btn.prev')
        b.click(x, y)
        b.sleep(0.5)
        sent = b.page('window.__presses')
        check(f'previous_stays_in_a_narrow_now_playing_cell ({label}): the button is shown inside '
              'the card, is what a click lands on, and the click sends the previous-track request',
              s['shown'] and s['hit'] and s['inside'] and s['title']
              and (s['cover'] == 'none') == narrow and sent == [['{"button":"previous"}', 200]],
              [s, sent])

    three = {'band': ['playing', 'tone', 'visualizer'], 'left': ['volume', 'controls'],
             'right': ['presets']}
    b.resize(1280, 900)
    open_page(b, d)
    keep(b, {'dac': three})
    b.goto(d.url + '?effects=off', settle=1.2)
    press('three in the band at 1280 px', True)
    b.resize(900, 900)
    b.goto(d.url + '?effects=off', settle=1.2)
    press('two in the first row at 900 px', True)
    b.resize(1280, 900)
    keep(b, {'dac': {**three, 'band': ['playing', 'tone'], 'right': ['visualizer', 'presets']}})
    b.goto(d.url + '?effects=off', settle=1.2)
    press('two in the band at 1280 px, the whole card', False)


def glow_follows(b, d):
    b.goto(d.url + '?effects=glow', settle=1.2)
    b.ev(f"localStorage.removeItem('{KEY}')")
    b.goto(d.url + '?effects=glow', settle=1.2)
    start(b)
    b.sleep(0.4)
    begun = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
      const k = document.querySelector('sv-knob').geometry();
      return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy]; })()""")
    b.page("(() => { window.__arr.moveTo('volume', 'right', 0); return true; })()")
    b.sleep(0.4)
    centres = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
      const k = document.querySelector('sv-knob').geometry();
      return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy]; })()""")
    b.page("(() => { window.__arr.done(); return true; })()")
    b.sleep(0.4)
    after = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
      const k = document.querySelector('sv-knob').geometry();
      return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy]; })()""")
    near = lambda c: abs(c[0] - c[2]) <= 2 and abs(c[1] - c[3]) <= 2  # noqa: E731
    check('the glow stays centred on the dial when the mode starts, when the volume is moved and '
          'after Done', near(begun) and near(centres) and near(after)
          and zones(b)['right'][0] == 'volume', [begun, centres, after])
    # a move that leaves the page as tall as it was resizes nothing: the dial moves down inside the
    # left column, the right one being the taller, and the glow must follow it all the same
    b.goto(d.url + '?effects=glow', settle=1.2)
    b.ev(f"localStorage.removeItem('{KEY}')")
    b.goto(d.url + '?effects=glow', settle=1.2)
    start(b)
    b.sleep(0.4)
    height = b.ev('document.documentElement.scrollHeight')
    dial = b.ev("document.querySelector('sv-knob').geometry().cy")
    b.page("(() => { window.__arr.moveTo('volume', 'left', 1); return true; })()")
    b.sleep(0.4)
    moved = b.ev("""(() => { const g = document.querySelector('.glow').getBoundingClientRect();
      const k = document.querySelector('sv-knob').geometry();
      return [g.left + g.width / 2, g.top + g.height / 2, k.cx, k.cy,
        document.documentElement.scrollHeight]; })()""")
    check('  and when the move leaves the page as tall as it was (Volume down inside its column)',
          near(moved) and moved[4] == height and abs(moved[3] - dial) > 50
          and zones(b)['left'] == ['controls', 'volume'], [height, dial, moved])
    b.page("(() => { window.__arr.cancel(); return true; })()")


def refused_storage(b, d):
    open_page(b, d)
    b.press('A')
    focus_handle(b, 'volume')
    b.press('ArrowRight')
    b.ev("Storage.prototype.setItem = () => { throw new Error('blocked'); }")
    x, y = common.center(b, '.arr-done')
    b.click(x, y)
    toasts = b.ev("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)")
    check('a browser that refuses storage keeps the page whole: the order stands for the page and '
          'a popup says it is not kept', zones(b)['right'][0] == 'volume' and any(
              'would not keep' in t for t in toasts), [zones(b), toasts])
    errs = errors(b)
    check('  with no page error', errs == [], errs)


def phone(b, d):
    b.resize(390, 844)
    open_page(b, d)
    order = {'band': ['tone', 'playing', 'visualizer'], 'left': ['controls', 'volume'],
             'right': ['presets']}
    keep(b, {'dac': order})
    b.goto(d.url + '?effects=off', settle=1.2)
    want = ['tone', 'playing', 'visualizer', 'controls', 'volume', 'presets']
    selectors = json.dumps({n: '.p-viz' if n == 'visualizer' else f'#part-{n}' for n in want})
    for label in ('', ' while arranging'):
        if label:
            start(b)
            b.sleep(0.4)
            save(b, 'phone-arranging')
        tops = b.ev(f"""(() => {{ const sel = {selectors};
          const top = (n) => document.querySelector(sel[n]).getBoundingClientRect().top + scrollY;
          return {{ tops: {json.dumps(want)}.map(top),
            wide: document.documentElement.scrollWidth - document.documentElement.clientWidth }};
        }})()""")
        flat = sum(zones(b).values(), [])
        check(f'on_a_phone_the_order_reads_band_left_right{label}: the DOM and the tops follow '
              'the stored order, no sideways scroll',
              flat == want and tops['tops'] == sorted(tops['tops'])
              and len(set(tops['tops'])) == len(want) and tops['wide'] <= 0, [flat, tops])
    b.page("(() => { window.__arr.cancel(); return true; })()")
    b.ev(f"localStorage.removeItem('{KEY}')")
    b.goto(d.url + '?effects=off', settle=1.2)
    got = b.ev("""(() => { const q = (s) => document.querySelector(s).getBoundingClientRect();
      return { playing: q('#part-playing').top, viz: q('.p-viz').top, volume: q('#part-volume').top,
        arranged: document.querySelector('#screen .view').classList.contains('arranged') }; })()""")
    check('  the default order on a phone is as it was: Now playing, the volume, the visualizer',
          not got['arranged'] and got['playing'] < got['volume'] < got['viz'], got)
    start(b)
    b.page("(() => { window.__arr.cancel(); return true; })()")
    got = b.ev("""(() => { const q = (s) => document.querySelector(s).getBoundingClientRect();
      return { playing: q('#part-playing').top, viz: q('.p-viz').top, volume: q('#part-volume').top,
        arranged: document.querySelector('#screen .view').classList.contains('arranged') }; })()""")
    check('  and so it is after arranging and then cancelling, the card put back under the volume',
          not got['arranged'] and got['playing'] < got['volume'] < got['viz'], got)


def phone_touch_drag(b, d):
    """A finger drag on a phone: Tone control is held low on the page and carried to the top, the
    page scrolling under the held handle, and it lands first in the band."""
    open_page(b, d)
    b.press('A')
    b.ev("document.querySelector('[data-arr=tone] .arr-handle').scrollIntoView({block: 'center'})")
    b.sleep(0.3)
    x, y = common.center(b, '[data-arr=tone] .arr-handle')
    touch = lambda kind, pts: b.send('Input.dispatchTouchEvent',  # noqa: E731
                                     {'type': kind, 'touchPoints': pts})
    start_y = b.ev('scrollY')
    touch('touchStart', [{'x': x, 'y': y}])
    for i in range(1, 11):
        touch('touchMove', [{'x': x, 'y': y + (30 - y) * i / 10}])
    top = None
    for _ in range(40):
        b.sleep(0.2)
        top = b.ev('scrollY')
        if top == 0:
            break
    touch('touchEnd', [])
    b.sleep(0.4)
    got = zones(b)
    check('a_drag_by_the_handle_moves_tone_into_the_band (phone, touch): the page scrolls under '
          'the held handle and Tone control lands first in the band',
          start_y > 500 and top == 0 and got['band'] == ['tone', 'playing', 'visualizer']
          and got['right'] == ['presets'], [start_y, top, got])
    b.press('Escape')


dac = fleet = gone = None
try:
    dac = common.Daemon('gungnir2-v2', host='fake')
    gone = common.Daemon('gungnir2-v2', host='fake')
    fleet = common.Daemon('gungnir2-v2', host='fake', more=[('Amp', '1112131415161718')],
                          env={'SVOLDER_FAKE_MORE': 'fake-amp@1112131415161718',
                               'SVOLDER_FAKE_SIGNAL': '1'})
    b = make()
    try:
        default_dom(b, dac)
        keyboard(b, dac)
        dragging(b, dac)
        reset_and_escape(b, dac)
        inert(b, dac)
        swap_lands(b, dac)
        lost_capture(b, dac)
        picker(b, dac)
        frame_row(b, dac)
        unmount(b, dac)
        viz_switch(b, dac)
        previous_stays(b, dac)
        glow_follows(b, dac)
        refused_storage(b, dac)
        every_section(b, dac)
        shape_change(b, gone)
        survives(b, fleet)
        leak(b, fleet)
        phone(b, dac)
        errs = errors(b)
        check('no page error in the whole run', errs == [], errs)
        if shots:
            b.resize(1280, 900)
            open_page(b, dac)
            b.press('A')
            b.sleep(0.4)
            save(b, 'arranging')
    finally:
        b.close()
    if browser == 'chromium':
        t = common.Chromium(size=(1280, 900), touch=True, args=['--disable-3d-apis'])
        try:
            dragging(t, dac, touch=True)
        finally:
            t.close()
        t = common.Chromium(size=(390, 844), touch=True, args=['--disable-3d-apis'])
        try:
            phone_touch_drag(t, dac)
        finally:
            t.close()
finally:
    for daemon in (dac, fleet, gone):
        if daemon:
            daemon.stop()
print(f'arrange.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
