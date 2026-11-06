# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The core's own rows that Ember does not use (design 4.7), driven through the core's API from a
test page, so a theme that needs them (Path, Night drive, Paper) finds them working:
  the_core_follows_a_second_device      a fleet of a DAC and an amp: `stream.follow(['Amp'])` brings
                                        the amp's meters into the model over one stream
  a_hop_added_from_the_page_reaches_the_fleet   a declared hop is in the model and the config file
  the_menu_fades_and_comes_back         `frame.fade(true)`: the button goes, any sign brings it back
  a_band_moves_from_plain_coordinates   `drag.createDrag` fed a 400 x 100 map of any drawing
  light_and_dark_follow_the_system      (Chromium) a theme with both schemes follows the system, a
                                        word of the person holds against it and is kept
  a_canvas_loop_pauses_while_hidden     `lifecycle.canvasLoop` stops its frames while the page is
                                        hidden and starts them again
  ember_unmounts_clean                  a theme switched away from and back leaves one listener per
                                        key, no canvas and no error
  the_tab_title_follows_on_every_theme  Ember, Paper, Signal and Night drive: the tab reads `<device> ·
                                        connected — svolder`, `reconnecting` while the daemon is
                                        gone, `connected` when back, and names a device switched to
  ember_mounted_while_lost_recovers     Ember mounted while the stream is lost greys the smoke and
                                        dims the screen, and both come back with the stream
Usage: python3 core_rows.py chromium|firefox"""
import json
import math
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# every EventSource the page opens is kept, and every write it posts
HOOK = ("window.__sources = []; const E = window.EventSource;"
        " window.EventSource = class extends E { constructor(u, o) { super(u, o);"
        " window.__sources.push(this); } };"
        " window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (a[1] && a[1].method === 'POST') window.__posts.push([String(a[0]), a[1].body]);"
        " return f(...a); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
# a theme the page does not ship, both schemes, which draws one line
PROBE = """(() => { __core.registry.add({ id: 'probe', label: 'Probe', blurb: 'a theme for the checks',
  needs: [], schemes: ['light', 'dark'],
  mount(root) { root.innerHTML = '<p id="probe-line">Probe</p>'; return () => root.replaceChildren(); },
  preview(box) { box.textContent = 'P'; } }); return 1; })()"""
FLEET = [('fake-amp', 'Amp', '1112131415161718')]


def make(size=(1280, 900), **kw):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size, **kw)
    b.preload(HOOK)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def poll(read, secs=5.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, daemon, query='?effects=off'):
    b.goto(daemon.url + query, settle=1.0)
    b.page(BOOT)
    time.sleep(0.5)


def follows_a_second_device(b, d):
    load(b, d)
    b.page("__core.stream.follow(['Amp'])")
    got = poll(lambda: b.page("!!__core.model.device('Amp')?.meters"), 8.0)
    check('the_core_follows_a_second_device: the amp\'s model is in the page, meters and all',
          got, b.page("JSON.stringify(__core.model.device('Amp'))")[:200])
    one = b.page("JSON.stringify(__core.model.device('Amp').meters)")
    time.sleep(1.5)
    two = b.page("JSON.stringify(__core.model.device('Amp').meters)")
    check('the_core_follows_a_second_device: its meters change', one != two, [one, two])
    live = b.page("window.__sources.filter((s) => s.readyState !== 2).map((s) => s.url)")
    check('the_core_follows_a_second_device: one stream is open and its address holds follow=Amp',
          len(live) == 1 and 'follow=Amp' in live[0], live)
    b.page("__core.stream.follow([])")
    check('the_core_follows_a_second_device: not followed any more, the amp is forgotten',
          poll(lambda: b.page("__core.model.device('Amp')") is None),
          b.page("__core.model.device('Amp') === null"))
    live = b.page("window.__sources.filter((s) => s.readyState !== 2).map((s) => s.url)")
    check('  and the stream is the one device again',
          len(live) == 1 and 'follow' not in live[0], live)


def hop_reaches_the_fleet(b, d):
    load(b, d)
    answer = b.page("__core.actions.hops.add({ name: 'HD 650', kind: 'headphones' })"
                    ".then((a) => a.ok)")
    check('a_hop_added_from_the_page_reaches_the_fleet: the daemon takes it', answer is True,
          answer)
    t = time.time()
    got = poll(lambda: b.page("__core.model.page().fleet.hops[0]?.id") == 'HD 650', 1.5)
    check('a_hop_added_from_the_page_reaches_the_fleet: it is hops[0] of the fleet within 1.5 s',
          got, [b.page('JSON.stringify(__core.model.page().fleet.hops)'), time.time() - t])
    config = open(f'{d.dir}/config/svolder/config.toml').read()
    check('a_hop_added_from_the_page_reaches_the_fleet: and it is in the config file',
          'HD 650' in config, config[-300:])


def menu_fades(b, d):
    load(b, d)
    button = ("(() => document.getElementById('fx-button').classList.contains('faded'))()")
    b.page('__core.frame.fade(true)')
    check('the_menu_fades_and_comes_back: the button fades after 2 s without a sign',
          poll(lambda: b.page(button), 4.0), b.page(button))
    wakes = {
        'a pointer move': "window.dispatchEvent(new Event('pointermove'))",
        'a pointer press': "window.dispatchEvent(new Event('pointerdown'))",
        'a touch': "window.dispatchEvent(new Event('touchstart'))",
    }
    for what, script in wakes.items():
        b.page(script)
        back = not b.page(button)
        check(f'the_menu_fades_and_comes_back: {what} brings it back at once', back)
        check(f'the_menu_fades_and_comes_back: and it fades again after {what}',
              poll(lambda: b.page(button), 4.0))
    b.press('Escape')
    check('the_menu_fades_and_comes_back: a key brings it back', not b.page(button))
    b.page('__core.frame.fade(false)')
    time.sleep(2.5)
    check('the_menu_fades_and_comes_back: a theme that does not ask keeps it', not b.page(button))
    check('the_menu_fades_and_comes_back: no page error', errors(b) == [], errors(b)[:3])


def nearest(table, value, distance):
    return min(range(len(table)), key=lambda i: (distance(table[i], value), i))


def band_moves(b, d):
    load(b, d)
    model = d.model('Gungnir 2')
    steps = model['tone_steps']
    band = 3
    hz = [row[0] for row in steps['bands'][band - 1]['f']]
    gain = [row[0] for row in steps['gain']]
    q = [row[0] for row in steps['q']]
    # a drawing of 400 x 100 px: frequency along the width (20 Hz to 20 kHz, logarithmic), gain
    # down the height (+12 dB at the top to -12 dB at the bottom)
    to_hz = lambda x: 20 * 1000 ** (x / 400)  # noqa: E731
    to_db = lambda y: 12 - 24 * y / 100  # noqa: E731
    want_f = nearest(hz, to_hz(250), lambda e, v: abs(math.log(e / v)))
    want_g = nearest(gain, to_db(25), lambda e, v: abs(e - v))
    start = model['tone']['readings']['bands'][band - 1]
    script = f"""(async () => {{
      const steps = {json.dumps({'f': hz, 'g': gain, 'q': q})};
      const map = {{ hzAt: (x) => 20 * 1000 ** (x / 400), dbAt: (y) => 12 - 24 * y / 100,
        yOfDb: (db) => (12 - db) / 24 * 100 }};
      const write = async (param, raw) => {{
        const answer = await __core.actions.setBand({band}, param, {{ raw }});
        return answer.ok ? {{ text: answer.text }} : {{ refused: answer.error }}; }};
      const drag = __core.drag.createDrag({{ band: {band}, gesture: 'move',
        start: {{ f: {start[0]}, g: {start[1]}, q: {start[2]} }},
        press: {{ x: 200, y: 50 }}, plot: {{ width: 400, height: 100 }}, steps, write, map,
        on: {{ held() {{}}, answer() {{}}, toast() {{}}, ended() {{}} }} }});
      drag.move({{ x: 215, y: 45 }}); drag.move({{ x: 250, y: 25 }}); drag.release();
      await drag.settled; return drag.held; }})()"""
    held = b.page(script)
    check('a_band_moves_from_plain_coordinates: the drag holds the steps of the target Hz and dB',
          held['f'] == want_f and held['g'] == want_g, [held, want_f, want_g])
    got = poll(lambda: d.model('Gungnir 2')['tone']['readings']['bands'][band - 1][:2]
               == [want_f, want_g])
    check('a_band_moves_from_plain_coordinates: band 3\'s f and g at the daemon are those steps',
          got, [d.model('Gungnir 2')['tone']['readings']['bands'][band - 1], want_f, want_g])
    check('a_band_moves_from_plain_coordinates: no page error', errors(b) == [], errors(b)[:3])


def light_and_dark(b, d):
    """The system's setting, the person's word against it, and the cookie that keeps it."""
    def system(mode):
        b.send('Emulation.setEmulatedMedia', {'features': [
            {'name': 'prefers-color-scheme', 'value': mode},
            {'name': 'prefers-reduced-motion', 'value': 'no-preference'}]})
        time.sleep(0.4)

    scheme = "document.documentElement.dataset.scheme"
    system('light')
    load(b, d)
    b.page(PROBE)
    b.press('T')
    b.page("document.querySelector('#picker .tcard[data-theme=probe]').click()")
    check('light_and_dark_follow_the_system: the probe theme is on show',
          poll(lambda: b.page("!!document.getElementById('probe-line')")))
    check('light_and_dark_follow_the_system: a light system gives light',
          poll(lambda: b.page(scheme) == 'light'), b.page(scheme))
    system('dark')
    check('light_and_dark_follow_the_system: a dark system gives dark, with no word of the person',
          poll(lambda: b.page(scheme) == 'dark'), b.page(scheme))
    b.page("__core.scheme.set('light')")
    check('light_and_dark_follow_the_system: the person\'s light holds against the dark system',
          b.page(scheme) == 'light', b.page(scheme))
    cookie = b.page('document.cookie')
    check('light_and_dark_follow_the_system: and is kept in the cookie as probe.light',
          'svolder_theme=probe.light' in cookie, cookie)
    system('light')
    system('dark')
    check('light_and_dark_follow_the_system: the system changing does not undo it',
          b.page(scheme) == 'light', b.page(scheme))
    b.page("__core.scheme.set('auto')")
    check('light_and_dark_follow_the_system: auto follows the system again',
          poll(lambda: b.page(scheme) == 'dark'), b.page(scheme))
    check('light_and_dark_follow_the_system: no page error', errors(b) == [], errors(b)[:3])
    system('no-preference')


def canvas_loop(b, d):
    load(b, d)
    b.page("""(() => { const canvas = document.createElement('canvas'); canvas.width = 100;
      canvas.height = 50; canvas.style.cssText = 'position: fixed; left: 0; top: 0; width: 100px;'
      + ' height: 50px;'; document.body.append(canvas); window.__draws = 0;
      window.__loop = __core.lifecycle.canvasLoop({ canvas, draw: () => { window.__draws++; },
        resize() {}, watchVisible: __core.lifecycle.watchVisible,
        matchMedia: (q) => matchMedia(q) }); window.__loop.start(); return 1; })()""")
    check('a_canvas_loop_pauses_while_hidden: the loop draws while the page is shown',
          poll(lambda: b.page('window.__draws') > 5), b.page('window.__draws'))
    b.page(HIDE)
    check('a_canvas_loop_pauses_while_hidden: it stops once the page has been hidden for a moment',
          poll(lambda: b.page('window.__loop.running') is False, 5.0), b.page('window.__loop.running'))
    one = b.page('window.__draws')
    time.sleep(0.8)
    check('a_canvas_loop_pauses_while_hidden: and draws nothing while hidden',
          b.page('window.__draws') == one, [one, b.page('window.__draws')])
    b.page(SHOW)
    check('a_canvas_loop_pauses_while_hidden: it draws again when the page is shown',
          poll(lambda: b.page('window.__draws') > one + 5), [one, b.page('window.__draws')])
    b.page("(() => { window.__loop.stop(); document.querySelector('canvas:last-of-type')?.remove();"
           " return 1; })()")
    check('a_canvas_loop_pauses_while_hidden: no page error', errors(b) == [], errors(b)[:3])


def unmounts_clean(b, d):
    load(b, d, '?effects=glow')
    b.page(PROBE)
    b.press('T')
    b.page("document.querySelector('#picker .tcard[data-theme=probe]').click()")
    check('ember_unmounts_clean: the probe theme is on show',
          poll(lambda: b.page("!!document.getElementById('probe-line')")))
    left = b.page("""({ canvases: document.querySelectorAll('canvas').length,
      layers: document.querySelectorAll('.fx').length, buttons: document.querySelectorAll('#fx-button').length,
      knobs: document.querySelectorAll('sv-knob').length })""")
    check('ember_unmounts_clean: Ember leaves no canvas, no effects layer and no knob; one button',
          left == {'canvases': 0, 'layers': 0, 'buttons': 1, 'knobs': 0}, left)
    b.press('T')
    b.page("document.querySelector('#picker .tcard[data-theme=ember]').click()")
    check('ember_unmounts_clean: Ember is back',
          poll(lambda: b.page("!!document.querySelector('#screen sv-knob')")))
    time.sleep(1.0)
    # one key press is one write: no listener of the first mount is left to write as well
    for _ in range(2):
        before = b.page('window.__posts.length')
        b.press('m')
        time.sleep(0.8)
        made = b.page('window.__posts.slice(%d).map((p) => p[0])' % before)
        check('ember_unmounts_clean: one key press makes one write', len(made) == 1
              and made[0].endswith('controls/mute'), made)
    layers = b.page("document.querySelectorAll('.fx').length")
    check('ember_unmounts_clean: one effects layer', layers == 1, layers)
    check('ember_unmounts_clean: no page error', errors(b) == [], errors(b)[:3])


def tab_title(b):
    return b.page('document.title')


def title_follows_on_every_theme(b, d):
    load(b, d)
    for theme in ('ember', 'paper', 'signal', 'nightdrive'):
        b.press('T')
        b.page(f"document.querySelector('#picker .tcard[data-theme={theme}]').click()")
        poll(lambda: b.page('__core.theme.id') == theme)
        name = b.page("__core.model.page().fleet.devices.find((x) => x.url === __core.model.shown()).name")
        want = f'{name} \u00b7 connected \u2014 svolder'
        check(f'the_tab_title_follows_on_every_theme: {theme}: connected reads `{want}`',
              poll(lambda: tab_title(b) == want, 5.0), tab_title(b))
        d.proc.terminate()
        d.proc.wait(5)
        gone = f'{name} \u00b7 reconnecting \u2014 svolder'
        check(f'the_tab_title_follows_on_every_theme: {theme}: the daemon dropped, reconnecting',
              poll(lambda: tab_title(b) == gone, 40.0), tab_title(b))
        d.restart()
        check(f'the_tab_title_follows_on_every_theme: {theme}: back, connected again',
              poll(lambda: tab_title(b) == want, 60.0), tab_title(b))


def title_names_the_device_switched_to(b, d):
    load(b, d)
    for theme in ('ember', 'paper', 'signal', 'nightdrive'):
        b.press('T')
        b.page(f"document.querySelector('#picker .tcard[data-theme={theme}]').click()")
        poll(lambda: b.page('__core.theme.id') == theme)
        urls = b.page('__core.model.page().fleet.devices.map((x) => x.url)')
        for url in reversed(urls):
            b.page(f"__core.actions.showDevice({json.dumps(url)})")
            name = b.page("__core.model.page().fleet.devices.find((x) => x.url === %s)?.name"
                          % json.dumps(url))
            want = f'{name} \u00b7 connected \u2014 svolder'
            check(f'the_tab_title_follows_on_every_theme: {theme}: a switch to {url} names {name}',
                  poll(lambda: tab_title(b) == want, 8.0), [name, tab_title(b)])


def mounted_while_lost(b, d):
    load(b, d, '?effects=smoke')
    b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
           ".then((m) => { window.__fx = m; return 1; })")
    frames = lambda: b.page('window.__fx.stats() && window.__fx.stats().frames')
    b.page(PROBE)
    b.press('T')
    b.page("document.querySelector('#picker .tcard[data-theme=probe]').click()")
    poll(lambda: b.page("!!document.getElementById('probe-line')"))
    d.proc.terminate()
    d.proc.wait(5)
    check('ember_mounted_while_lost_recovers: the page says lost',
          poll(lambda: b.page("document.body.classList.contains('lost')"), 40))
    b.press('T')
    b.page("document.querySelector('#picker .tcard[data-theme=ember]').click()")
    poll(lambda: b.page("!!document.querySelector('#screen sv-knob')"))
    time.sleep(3.0)
    a = frames()
    time.sleep(1.5)
    check('ember_mounted_while_lost_recovers: mounted while lost, the screen is inert and the '
          'smoke has stopped',
          b.page("document.getElementById('screen').hasAttribute('inert')") is True
          and a is not None and a == frames(), [a, frames()])
    d.restart()
    poll(lambda: not b.page("document.body.classList.contains('lost')"), 60)
    time.sleep(2.0)
    a = frames()
    time.sleep(1.5)
    check('ember_mounted_while_lost_recovers: with the stream back the screen answers and the '
          'smoke runs again',
          b.page("document.getElementById('screen').hasAttribute('inert')") is False
          and a is not None and frames() > a + 5, [a, frames()])
    # a stopped daemon is not an error of the page (its failed requests are logged)
    seen = [e for e in errors(b) if 'Failed to load resource' not in e]
    check('ember_mounted_while_lost_recovers: no page error', seen == [], seen[:3])


dac = fleet = None
try:
    dac = common.Daemon('gungnir2-v2')
    fleet = common.Daemon('gungnir2-v2', fleet=FLEET, env={'SVOLDER_FAKE_SIGNAL': '1'})
    steps = [(follows_a_second_device, fleet), (hop_reaches_the_fleet, dac), (menu_fades, dac),
             (band_moves, dac), (canvas_loop, dac), (unmounts_clean, dac),
             (title_follows_on_every_theme, dac), (title_names_the_device_switched_to, fleet),
             (mounted_while_lost, dac)]
    if browser == 'chromium':
        steps.insert(4, (light_and_dark, dac))
    for step, daemon in steps:
        b = make()
        try:
            step(b, daemon)
        finally:
            b.close()
finally:
    for daemon in (dac, fleet):
        if daemon:
            daemon.stop()
print(f'core_rows.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
