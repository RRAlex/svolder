# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""What the Void Garden browser checks share (voidgarden_theme.py, voidgarden_states.py,
voidgarden_phone.py, voidgarden_timing.py): the engine chosen on the command line, the cells
(one engine at one tier: `none` has both 3D APIs off, `webgl2` is the stock browser, `webgpu` the
engine's WebGPU switches), the pass and fail counts, the page hook that records console warnings
and errors, CSP violations and page errors from the first script on, and the helpers that load the
page and read the theme's driver. Not run by itself."""
import json
import sys
import time

import common

engine = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()

CHROMIUM_FLAGS = {
    'none': ['--disable-3d-apis'],
    'webgl2': [],
    'webgpu': ['--enable-unsafe-webgpu', '--enable-features=Vulkan',
               '--use-webgpu-adapter=swiftshader', '--use-angle=swiftshader'],
}
FIREFOX_PREFS = {
    'none': ['user_pref("webgl.disabled", true);', 'user_pref("dom.webgpu.enabled", false);'],
    'webgl2': [],
    'webgpu': ['user_pref("dom.webgpu.enabled", true);'],
}

HOOK = """window.__vg = { logs: [], csp: [], steps: 0 };
for (const k of ['warn', 'error']) { const o = console[k].bind(console);
  console[k] = (...a) => { window.__vg.logs.push(k + ': ' + a.map(String).join(' ').slice(0, 300)); o(...a); }; }
addEventListener('securitypolicyviolation', (e) => window.__vg.csp.push(e.violatedDirective + ' ' + e.blockedURI));
addEventListener('error', (e) => window.__vg.logs.push('error: ' + e.message));
addEventListener('unhandledrejection', (e) => window.__vg.logs.push('rejection: ' + e.reason));
(() => { const add = EventTarget.prototype.addEventListener;
  EventTarget.prototype.addEventListener = function (type, fn, opt) {
    if (type === 'step' && this !== window && this !== document && !window.__vgSteps) {
      window.__vgSteps = this;
      add.call(this, 'step', (e) => { window.__vg.steps++; window.__vgLast = e.detail; });
    }
    return add.call(this, type, fn, opt); }; })();"""

# the page's own writes, recorded with the page's clock (ms)
REC = """(() => { if (window.__posts) return 1; window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push([String(a[0]), String((a[1] || {}).body || ''), performance.now()]);
    return f(...a); }; return 1; })()"""

# every storage call throws: a private window, blocked site data
NO_STORAGE = """(() => { const no = () => { throw new DOMException('blocked', 'SecurityError'); };
  for (const k of ['getItem', 'setItem', 'removeItem', 'clear', 'key']) {
    try { Storage.prototype[k] = no; } catch (e) {} } })();"""

# Void Garden keeps the Motion level under this key; Still draws once a second, so a case that
# is not about the scene's own cost does not spend the pool's CPU on software rendering
STILL = """try { if (localStorage.getItem('svolder.theme.voidgarden.motion') === null)
  localStorage.setItem('svolder.theme.voidgarden.motion', '"still"'); } catch (e) {}"""

BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
# the page's visibility is a switch the check holds (the stream rests when it is hidden)
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
  document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
DRIVER = '__core.theme.driver'
DEVICE = 'Gungnir 2'


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def poll(read, secs=10.0, every=0.1):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(every)
        got = read()
    return got


def make(cell='none', size=(1280, 800), still=False, storage=True, reduced=False, touch=False):
    """A browser of the cell with the page hook (and, as asked, the Still pref, no storage)."""
    if engine == 'chromium':
        b = common.Chromium(size=size, args=CHROMIUM_FLAGS[cell], reduced=reduced, touch=touch)
    else:
        b = common.Firefox(size=size, extra_prefs=FIREFOX_PREFS[cell], reduced=reduced)
    b.preload(HOOK + ('' if storage else NO_STORAGE) + (STILL if still and storage else ''))
    return b


def load(b, d, query='?theme=voidgarden', settle=1.0):
    b.goto(f'{d.url}{query}', settle=settle)
    return b.page(BOOT)


def forget(b, d):
    """This browser forgets its choice of theme and every preference (storage and cookie)."""
    b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
    b.page("""(() => { try { localStorage.clear(); } catch (e) {}
      document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()""")


def logs(b):
    """Every console warning or error, CSP violation and page error since the page began."""
    found = list(b.page('window.__vg.logs') or []) + [f'csp: {c}' for c in b.page('window.__vg.csp') or []]
    errs = b.errors if engine == 'chromium' else b.collect_errors()
    return found + [e for e in errs if 'favicon' not in e]


def posts(b, tail=''):
    """The page's writes whose address ends with `tail`: [url, body, ms]."""
    return [p for p in b.page('window.__posts') or [] if p[0].endswith(tail)]


def click_el(b, selector):
    # a row low in a long menu is under the menu's own scroll: bring it into view first
    b.ev(f"(document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'nearest' }}), 1)")
    x, y = common.center(b, selector)
    b.click(x, y)


def open_menu(b):
    click_el(b, '#fx-button')
    return poll(lambda: b.page("!!document.querySelector('[data-motion]')"), 3.0)


def close_menu(b):
    b.press('Escape')
    return poll(lambda: not b.page("!!document.querySelector('[data-motion]')"), 3.0)


def pref(b, name):
    """The theme's kept value of `name` (parsed), or None."""
    text = b.page(f"(() => {{ try {{ return localStorage.getItem('svolder.theme.voidgarden.{name}'); }} "
                  "catch (e) { return 'refused'; } })()")
    if text in (None, 'refused'):
        return text
    try:
        return json.loads(text)
    except ValueError:
        return text


def done(script):
    """Print the totals and exit 1 on a failure."""
    print(f'{script}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
    sys.exit(1 if fails else 0)
