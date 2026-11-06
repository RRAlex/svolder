# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""What the Night drive browser harnesses share (nightdrive_theme.py, nightdrive_timing.py, nightdrive_sheet.py): the
browser chosen on the command line, the pass and fail counts, and the helpers that load the page,
read its request log, move the bands to known values and so on. Not run by itself."""
import json
import math
import sys
import time
import urllib.parse

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


BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""


HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""


SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""


DRAWS = '__core.theme.driver.perf().draws'


def make(size=(1280, 900), **kw):
    if browser == 'chromium':
        return common.Chromium(size=size, **kw)
    return common.Firefox(size=size, **kw)


# what a page's own failed request logs (a refused or dropped fetch): not an error of the page's code
NETWORK = ('Failed to load resource', 'Failed to fetch', 'NetworkError')


def errors(b):
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'favicon.ico' not in e]


def poll(read, secs=5.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, query='?theme=nightdrive'):
    """Load the page and wait for the theme to be on show: the id of the theme mounted."""
    b.goto(f'{d.url}{query}', settle=1.0)
    return b.page(BOOT)


FADE_KEY = "localStorage.setItem('svolder.theme.nightdrive.fade-after', '2')"
OPACITY = "((sel) => +getComputedStyle(document.querySelector(sel)).opacity)"
FADED = "document.getElementById('nightdrive').classList.contains('nd-faded')"


def forget(b, d):
    """This browser forgets its choice of theme and every preference (storage and cookie)."""
    b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
    b.page("""(() => { try { localStorage.clear(); } catch (e) {}
      document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()""")


def load_fading(b, d, query='?theme=nightdrive'):
    """Load with the fade after 2 s (the preference the checks set; the menu has no row for it)."""
    forget(b, d)
    b.page(FADE_KEY)
    return load(b, d, query)


def click_el(b, selector):
    x, y = common.center(b, selector)
    b.click(x, y)


def open_menu(b):
    click_el(b, '#fx-button')
    return poll(lambda: b.page("!!document.querySelector('[data-motion]')"), 3.0)


def rate(b, secs):
    one = b.page(DRAWS)
    time.sleep(secs)
    return b.page(DRAWS) - one


DEVICE = 'Gungnir 2'


# every request the page makes, with the page's clock at the time (performance.now(), ms)
REC = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push([String(a[0]), String((a[1] || {}).body || ''), performance.now()]);
    return f(...a); }; return 1; })()"""


def posts_to(b, tail):
    return [body for url, body, _ in b.page('window.__posts') if url.endswith(tail)]


def synth(b, selector, type_, **extra):
    """A pointer event of the primary mouse on the element `selector` (Chromium and Firefox alike)."""
    b.page(f"""(() => {{ const e = document.querySelector('{selector}'); const r = e.getBoundingClientRect();
      e.dispatchEvent(new PointerEvent('{type_}', {{ pointerId: 11, isPrimary: true, pointerType: 'mouse',
        button: 0, buttons: 1, clientX: r.left + 5, clientY: r.top + 5, bubbles: true, cancelable: true, ...{json.dumps(extra)} }}));
      return 1; }})()""")


BANDS = """__core.model.page().device.tone.readings.bands"""


NEW_DEVICE = 'Bifrost 3'


BAND_HZ = (60, 250, 1000, 4000, 12000)


BAND_DB = (3, -3, 6, -2, 2)


def reset_bands(d, over=None, flat=False, b=None):
    """Put the shown device's five bands at known values whatever an earlier case left: each at its
    step nearest BAND_HZ, BAND_DB and a Q of 1; `over` is {band: (gain dB, Q)} for the bands a case
    wants otherwise (a Q of 1e9 is the narrowest step). `flat` leaves the device in Flat. Waits for
    the daemon, and for the page `b` when given. Returns the readings."""
    over = over or {}
    steps = d.model(DEVICE)['tone_steps']
    want = []
    for k in range(5):
        db, q = over.get(k + 1, (BAND_DB[k], 1))
        want.append([nearest_of(steps['bands'][k]['f'], BAND_HZ[k], log=True),
                     nearest_of(steps['gain'], db), nearest_of(steps['q'], q)])
    now = d.model(DEVICE)['tone']['readings']['bands']
    for k, row in enumerate(want):
        for param, raw, was in zip(('frequency', 'gain', 'q'), row, now[k]):
            if raw != was:
                d.post(DEVICE, f'eq/bands/{k + 1}', {'param': param, 'raw': raw})
    poll(lambda: [r[:3] for r in d.model(DEVICE)['tone']['readings']['bands']] == want, 8.0)
    if d.model(DEVICE)['tone']['mode'] != ('flat' if flat else 'eq'):
        d.post(DEVICE, 'eq/mode', {'mode': 'flat' if flat else 'eq'})
        poll(lambda: d.model(DEVICE)['tone']['mode'] == ('flat' if flat else 'eq'), 8.0)
    if b is not None:
        poll(lambda: [r[:3] for r in b.page(BANDS)] == want, 8.0)
    time.sleep(0.3)
    return want


def band_posts(b, device=None):
    """The band writes this page has sent, [url, body, time]; of one device when named. Paths are
    relative ('api/v1/devices/…'), as the core posts them."""
    found = [[u, json.loads(body), at] for u, body, at in b.page('window.__posts')
             if 'api/v1/devices/' in u and '/eq/bands/' in u and body]
    return [p for p in found if device is None or urllib.parse.quote(device) in p[0]]


def nearest_of(rows, value, log=False):
    """The index of the step of `rows` ([[value, words], …]) nearest `value`, the lowest on a tie."""
    dist = (lambda v: abs(math.log(v / value))) if log else (lambda v: abs(v - value))
    best = None
    for i, (v, _) in enumerate(rows):
        if isinstance(v, (int, float)) and (best is None or dist(v) < best[0]):
            best = (dist(v), i)
    return best[1]


PERF = '__core.theme.driver.perf()'


KEY = "localStorage.getItem('svolder.theme.nightdrive.motion')"


MOTION_ROWS = """(() => [...document.querySelectorAll('[data-motion]')].map((e) => ({
  id: e.dataset.motion, text: e.textContent.trim(), role: e.getAttribute('role'),
  checked: e.getAttribute('aria-checked') })))()"""


PAUSE = """fetch('api/v1/host/press', { method: 'POST', headers: { 'content-type': 'application/json' },
  body: '{"button":"pause"}' }).then((r) => r.status)"""


DRIVER = "import(new URL('assets/themes/nightdrive/driver.js', document.baseURI))"


# the bands, and with them the ear, held at a level the page chooses: the model is not stepped any more and
# its bands are `v` (0 to 1 over -72 to 0 dBFS); the ear is moved at once to where `v` leaves it after `secs`
# seconds of silence first, or after a loud passage first when it is silence that is held (the ear is
# stepped by the page's own draws as well, with the same bands)
HOLD = """((v, snap) => DRIVER.then((m) => { const s = m.bound.scene; s.model.step = () => {}; s.model.beatDue = () => false;
  s.model.left.fill(v); s.model.right.fill(v);
  const units = new Float32Array(240).fill(Math.min(1, Math.max(0, (72 * v - 12) / 60)));
  if (snap) { s.ear.step(2, new Float32Array(240).fill(0.9)); s.ear.step(snap, units); }
  return 1; }))""".replace('DRIVER', DRIVER)


LOOK = '__core.theme.driver.look()'


EAR = '__core.theme.driver.sky().levels.ear'


# every cell of the floor at `level`, the rows no longer born
FLOOR_AT = """((level) => DRIVER.then((m) => { const f = m.bound.scene.floor; const row = new Float32Array(24).fill(level);
  if (!f.stopped) { f.stopped = true; const adv = f.advance.bind(f); f.born = adv; f.advance = () => {}; }
  for (let i = 0; i < 30; i++) f.born(1, 1, row, true); return 1; }))""".replace('DRIVER', DRIVER)


def p95(values):
    values = sorted(values)
    return values[min(len(values) - 1, int(0.95 * len(values)))]


def median(values):
    return sorted(values)[len(values) // 2]


def pick(b, level):
    open_menu(b)
    click_el(b, f'[data-motion={level}]')
    time.sleep(0.4)


def finish(name):
    """Print the harness's totals and exit with its status."""
    print(f'{name} {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
    sys.exit(1 if fails else 0)
