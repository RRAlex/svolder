# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""How the page drives the smoke, in Chromium or Firefox, numbers only (readPixels, counters;
never a picture of Smoke).
Usage: python3 smoke_page.py chromium|firefox [section ...]   (default: every section)
Checks: a lost page stream greys and stops Smoke (and brings it back with the stream), WebGL is
probed only when Smoke shows or the menu first opens, a throw while Smoke starts leaves the page's
stream and keys working, the glow is hidden until the effects set their level, picking the shown
level keeps the running smoke, an amp's meters leave the calm rate alone below the red zone, what
a splat and a pointer move cost (a scissored splat, no layout read in a pointer move; Chromium
only)."""
import json
import sys

import common
from smoke_probe import PROBE

CALM_SECS = 6.0  # how long the amp's signal is watched for meter splats (s)
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


def make(**kw):
    if browser == 'chromium':
        args = ['--disable-3d-apis'] if kw.pop('no_gl', False) else []
        return common.Chromium(args=args, **kw)
    prefs = ['user_pref("webgl.disabled", true);'] if kw.pop('no_gl', False) else []
    return common.Firefox(extra_prefs=prefs, **kw)


def errors(b):
    seen = b.errors if browser == 'chromium' else b.collect_errors()
    # a stopped daemon is not an error of the page (its failed requests are logged)
    return [e for e in seen if 'Failed to load resource' not in e]


def do(b, js):
    """Run statements (Firefox's `ev` wraps an expression)."""
    if browser == 'chromium':
        return b.ev(js)
    return b._post('/execute/sync', {'script': js, 'args': []})['value']


def load(b, url, wait=0.3):
    b.goto(url, settle=0.8)
    do(b, PROBE)
    for _ in range(50):
        if b.ev('window.__mod !== null'):
            break
        b.sleep(0.1)
    # the effects module, for its counters and its `lost`
    b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
           ".then((m) => { window.__fx = m; return 1; })")
    b.sleep(wait)


def read(b, sel='canvas.smoke', frames=14, wait=0.6):
    b.ev(f'window.__read({json.dumps(sel)}, {frames})')
    for _ in range(100):
        b.sleep(wait if frames > 10 else 0.1)
        if b.ev('window.__rd !== null'):
            return
        wait = 0.1
    raise RuntimeError('probe did not finish')


def frames(b):
    return b.page('window.__fx.stats() && window.__fx.stats().frames')


def pick(b, level):
    """Choose a level by real clicks on the menu."""
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    x, y = common.center(b, f'.fxpop [data-level={level}]')
    b.click(x, y)


def hook_fetch(b):
    b.ev("""(() => { window.__posts = []; const f = window.fetch;
      window.fetch = (...a) => { window.__posts.push(String(a[0])); return f(...a); };
    })()""")


def volume_posts(b):
    return [p for p in b.ev('window.__posts') if '/volume' in p or 'controls/' in p]


# Counts the contexts asked of a canvas, from the first script on.
COUNT_CONTEXTS = """window.__ctx = 0;
const g = HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext = function (k, ...r) {
  if (k === 'webgl2') window.__ctx++;
  return g.call(this, k, ...r);
};"""


# Makes the page's shade a colourful one (every random draw before the effects set their level is
# 0.9: a saturation of 35 %), so a colour check has something to tell from grey; the draws are
# the browser's own again afterwards.
COLOURFUL = """window.__fixRandom = true; const rand0 = Math.random;
Math.random = () => (window.__fixRandom ? 0.9 : rand0());
new MutationObserver((rs, ob) => {
  if (!document.querySelector('.fx[data-level]')) return;
  window.__fixRandom = false; ob.disconnect();
}).observe(document, { subtree: true, attributes: true, attributeFilter: ['data-level'] });"""
# The smallest spread/alpha (the mean over pixels of alpha >= 8) the colourful shade's smoke shows:
# half its greyer shade's chroma (saturation 11.7 %, lightness 68 %: 0.075)
IN_COLOUR = 0.037


def lost_link():
    """The daemon stops under a Smoke page: grey, then no frames, no notes, no stir; back with
    the stream."""
    d = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        b.preload(COLOURFUL)
        load(b, d.url)
        b.sleep(2.0)
        read(b)
        pk0, grey0 = b.ev('window.__peak()'), b.ev('window.__grey()')
        check('lost link: before, Smoke runs in colour (the colourful shade\'s)',
              pk0 > 0 and grey0['mean'] > IN_COLOUR, [pk0, grey0])
        # every settle wins its roll, so a stir that settles forms a note; the drag below is a stir
        # pending when the daemon stops, and none of it may settle into a note after the loss
        b.ev('window.__fx.roll(0)')
        # dye for the grey to show on: a pressed pointer drags over the dial's neighbourhood
        for i in range(40):
            b.ev(f"window.dispatchEvent(new PointerEvent('pointermove', {{ clientX: {500 + i * 6},"
                 f" clientY: {330 + (i % 5) * 4}, pointerType: 'mouse', pointerId: 3,"
                 f" buttons: 1 }}))")
            b.sleep(0.03)
        # a real stir (5 short sides a second and more) right before the daemon stops
        b.ev("(() => { let i = 0; const t = setInterval(() => {"
             " window.dispatchEvent(new PointerEvent('pointermove', { clientX: 200 + 200 * (i % 5),"
             " clientY: 500, pointerType: 'mouse', pointerId: 4, buttons: 0 }));"
             " if (++i > 10) clearInterval(t); }, 16); })()")
        b.sleep(0.4)
        before = b.page('window.__fx.stats().notes')
        d.proc.terminate()
        d.proc.wait(5)
        for _ in range(120):
            b.sleep(0.25)
            if b.ev("document.body.classList.contains('lost')"):
                break
        check('lost link: the page says lost', b.ev("document.body.classList.contains('lost')"))
        b.ev("window.__read('canvas.smoke', 120)")
        b.sleep(3.0)
        fa = frames(b)
        for x, y in ((300, 300), (500, 400), (700, 300), (900, 500)):
            b.click(x, y)
        b.sleep(1.5)
        fb = frames(b)
        grey1 = b.ev("window.__grey('lastNonEmpty')")
        print(f'      lost: frames {fa} -> {fb} over 1.5 s with the pointer moving; colour '
              f'spread/alpha before {grey0["mean"]:.3f}, after the fade {grey1["worst"]:.3f}')
        check('lost link: frames stop after the fade, whatever the pointer does', fa == fb,
              (fa, fb))
        check('lost link: the colour is grey (mean spread/alpha under 0.0111, 0.3 of the lowest '
              'colour IN_COLOUR asks for, over pixels of alpha >= 8)',
              grey1 is not None and grey1['seen'] > 0 and grey1['mean'] < 0.3 * IN_COLOUR,
              grey1)
        after = b.page('window.__fx.stats().notes')
        check('lost link: the stir pending at the loss settles into no note and rolls no settle',
              before['rolls'] == 0 and after['stamped'] == before['stamped']
              and after['rolls'] == 0, [before, after])
        check('lost link: Glow\'s own away class is not toggled by it',
              b.ev("document.querySelector('.fx').classList.contains('away')") is False)
        d.restart()
        for _ in range(240):
            b.sleep(0.25)
            if not b.ev("document.body.classList.contains('lost')"):
                break
        check('lost link: the stream comes back', not b.ev(
            "document.body.classList.contains('lost')"))
        b.sleep(3.0)
        back = b.page('window.__fx.stats().notes')
        check('lost link: and the stir pending at the loss forms no note once the stream is back',
              back['stamped'] == before['stamped'] and back['rolls'] == 0, [before, back])
        fc = frames(b)
        read(b, frames=14)
        grey2 = b.ev('window.__grey()')
        check('lost link: back, frames resume and the colour returns',
              fc > fb + 5 and grey2['mean'] > IN_COLOUR, (fb, fc, grey2))
        check('lost link: no page error', errors(b) == [], errors(b))
        b.close()
        # Glow keeps its own lost look: no away class, the glow grey by the body's class
        d.restart()
        b = make(size=(1280, 900))
        load(b, d.url + '?effects=glow')
        b.sleep(1.0)
        d.proc.terminate()
        d.proc.wait(5)
        for _ in range(120):
            b.sleep(0.25)
            if b.ev("document.body.classList.contains('lost')"):
                break
        look = b.ev("(() => { const fx = document.querySelector('.fx');"
                    " return [fx.classList.contains('away'),"
                    " getComputedStyle(fx.querySelector('.glow')).filter]; })()")
        check('lost link, Glow: grey by the body\'s class, no away class',
              look[0] is False and 'grayscale' in look[1], look)
    finally:
        b.close()
        d.stop()


def lazy_probe():
    """WebGL is asked for only when Smoke shows, or the menu first opens; the answer is kept."""
    d = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        b.preload(COUNT_CONTEXTS)
        b.goto(d.url + '?effects=glow')
        b.ev("localStorage.setItem('svolder.effects', 'glow') || 0")
        b.goto(d.url)
        n = b.ev('window.__ctx')
        check('Glow: no WebGL context is made at load (not even a probe)', n == 0, n)
        pick_open = common.center(b, '.fxbtn')
        b.click(*pick_open)
        n1 = b.ev('window.__ctx')
        check('Glow: the menu opening makes the probe, once', n1 == 1, n1)
        b.click(*pick_open)
        b.click(*pick_open)
        b.click(*pick_open)
        check('  closed and opened again, the answer is remembered', b.ev('window.__ctx') == 1,
              b.ev('window.__ctx'))
        pick(b, 'smoke')
        b.sleep(1.0)
        check('  choosing Smoke starts it with one context of its own, no second probe',
              b.ev('window.__ctx') == 2 and b.ev("document.querySelectorAll('.fx canvas').length")
              == 1, b.ev('window.__ctx'))
        b.ev("localStorage.setItem('svolder.effects', 'off') || 0")
        b.goto(d.url)
        check('Off: no WebGL context is made at load', b.ev('window.__ctx') == 0,
              b.ev('window.__ctx'))
        b.ev("localStorage.removeItem('svolder.effects')")
        b.goto(d.url)
        b.sleep(0.5)
        n = b.ev('window.__ctx')
        check('Smoke: the probe and the smoke\'s own context at load', n == 2, n)
        b.click(*common.center(b, '.fxbtn'))
        check('  the menu opening asks for nothing more', b.ev('window.__ctx') == 2,
              b.ev('window.__ctx'))
        check('no page error', errors(b) == [], errors(b))
        b.close()
        # no WebGL here: a Glow page learns it when the menu opens
        b = make(size=(1280, 900), no_gl=True)
        b.preload(COUNT_CONTEXTS)
        b.goto(d.url)
        b.ev("localStorage.setItem('svolder.effects', 'glow') || 0")
        b.goto(d.url)
        check('no WebGL, Glow: nothing is asked at load', b.ev('window.__ctx') == 0,
              b.ev('window.__ctx'))
        b.click(*common.center(b, '.fxbtn'))
        item = b.ev("(() => { const i = document.querySelector('[data-level=smoke]');"
                    " return [i.getAttribute('aria-disabled'), i.title]; })()")
        check('  the first open marks Smoke unavailable with its reason',
              item[0] == 'true' and 'no WebGL2' in item[1], item)
    finally:
        b.close()
        d.stop()


# Keeps what the page says through console.warn (both browsers' drivers read it the same way).
KEEP_WARNS = """window.__warns = [];
const w = console.warn.bind(console);
console.warn = (...a) => { window.__warns.push(a.map(String).join(' ')); w(...a); };"""


def start_throws():
    """A throw while Smoke starts is Smoke unavailable; the page's stream, knob and keys work."""
    d = common.Daemon('gungnir2-v2')
    cases = (
        ('the first context request throws',
         "const g = HTMLCanvasElement.prototype.getContext;"
         " HTMLCanvasElement.prototype.getContext = function (k, ...r) {"
         " if (k === 'webgl2') throw new Error('injected');"
         " return g.call(this, k, ...r); };"),
        ('the smoke\'s own context request throws (the probe passed)',
         "let n = 0; const g = HTMLCanvasElement.prototype.getContext;"
         " HTMLCanvasElement.prototype.getContext = function (k, ...r) {"
         " if (k === 'webgl2' && ++n === 2) throw new Error('injected');"
         " return g.call(this, k, ...r); };"),
        ('the effects\' own setup throws (the size observer, on a Glow page)',
         "window.ResizeObserver = function () { throw new Error('injected'); };"),
    )
    try:
        for name, js in cases:
            b = make(size=(1280, 900))
            try:
                b.preload(KEEP_WARNS)
                b.preload(js)
                b.goto(d.url + ('?effects=glow' if 'observer' in name else ''), settle=1.5)
                lost = b.ev("document.body.classList.contains('lost')")
                raw = b.ev("(document.querySelector('sv-knob') || {}).raw")
                glow = b.ev("(document.querySelector('.fx') || { dataset: {} }).dataset.level")
                check(f'{name}: the page shows Glow and is not lost',
                      glow == 'glow' and lost is False and raw is not None, [glow, lost, raw])
                hook_fetch(b)
                b.press('ArrowLeft')
                b.sleep(1.0)
                check(f'  {name}: the page\'s keys still write the volume',
                      len(volume_posts(b)) >= 1, volume_posts(b))
                now = b.ev("(document.querySelector('sv-knob') || {}).raw")
                check(f'  {name}: the stream still updates the knob', now != raw, [raw, now])
                b.click(*common.center(b, '.fxbtn'))
                item = b.ev("(() => { const p = document.querySelector('.fxpop');"
                            " const i = p.querySelector('[data-level=smoke]');"
                            " return [!p.hidden, i.getAttribute('aria-disabled')]; })()")
                check(f'  {name}: the menu works',
                      item[0] is True and (item[1] == 'true' or 'observer' in name), item)
                want = ('check for Smoke threw' if 'first' in name
                        else 'threw while starting' if 'probe passed' in name
                        else 'effects threw while setting up')
                warns = b.page('window.__warns')
                check(f'  {name}: the page says why in a console warning',
                      any(want in w for w in warns), warns)
                check(f'  {name}: no page error', errors(b) == [], errors(b))
            finally:
                b.close()
    finally:
        d.stop()


def start_cleanup():
    """A failure inside the probe's copy, or in the smoke after its context exists, leaves nothing
    behind."""
    d = common.Daemon('gungnir2-v2')
    try:
        b = make(size=(1280, 900))
        try:
            # the copy the splats make fails with a GL error: Smoke is unavailable, with its reason
            b.preload("WebGL2RenderingContext.prototype.copyTexSubImage2D = function () {"
                      " this.activeTexture(1); };")
            b.goto(d.url, settle=1.5)
            item = b.ev("(() => { const i = document.querySelector('.fxpop [data-level=smoke]');"
                        " return [document.querySelector('.fx').dataset.level,"
                        " i.getAttribute('aria-disabled'), i.title]; })()")
            check('a failing half-float copy: Glow shows and Smoke is unavailable (half-float)',
                  item[0] == 'glow' and item[1] == 'true' and 'half-float' in item[2], item)
        finally:
            b.close()
        b = make(size=(1280, 900))
        try:
            b.preload("window.__ctxs = [];"
                      " const g = HTMLCanvasElement.prototype.getContext;"
                      " HTMLCanvasElement.prototype.getContext = function (k, ...r) {"
                      " const c = g.call(this, k, ...r); if (k === 'webgl2') window.__ctxs.push(c);"
                      " return c; };"
                      " window.ResizeObserver = function () { throw new Error('injected'); };")
            b.goto(d.url, settle=1.5)
            made = b.ev('window.__ctxs.length')
            alive = b.ev('window.__ctxs.filter((c) => c && !c.isContextLost()).length')
            check('a throw in the smoke after its context exists: the context is lost at once',
                  made >= 2 and alive == 0, [made, alive])
        finally:
            b.close()
    finally:
        d.stop()


# Records, once, the glow's computed opacity next to its level's opacity in the task that sets
# the layer's data-level, and again in the first animation frame after it.
WATCH_GLOW = """window.__glowSeen = [];
new MutationObserver((rs, ob) => {
  const fx = document.querySelector('.fx');
  if (!fx || !fx.dataset.level) return;
  ob.disconnect();
  const glow = fx.querySelector('.glow');
  const read = () => [parseFloat(getComputedStyle(glow).opacity),
    .22 + .78 * parseFloat(fx.style.getPropertyValue('--lvl') || getComputedStyle(fx)
      .getPropertyValue('--lvl'))];
  const now = read();
  requestAnimationFrame(() => {
    const next = read();
    window.__glowSeen.push([...now, ...next]);
  });
}).observe(document, { subtree: true, attributes: true, attributeFilter: ['data-level'] });"""


def first_paint():
    """The glow is hidden until the effects set the level; then it stands at it."""
    d = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        for query, level in (('?effects=glow', 'glow'),):
            b.preload(WATCH_GLOW)
            b.goto(d.url + query, settle=1.8)
            bare = b.ev("""(() => { const fx = document.createElement('div');
              fx.className = 'fx'; fx.innerHTML = '<div class="glow"></div>';
              document.body.append(fx);
              const o = getComputedStyle(fx.querySelector('.glow')).opacity; fx.remove();
              return o; })()""")
            check('the glow of a layer with no level yet has opacity 0', float(bare) == 0, bare)
            now = b.ev("(() => { const fx = document.querySelector('.fx');"
                       " return [fx.dataset.level,"
                       " getComputedStyle(fx.querySelector('.glow')).opacity]; })()")
            check(f'after the effects start, the {level} glow stands at its level (> 0.2)',
                  now[0] == level and float(now[1]) > 0.2, now)
            seen = b.ev('window.__glowSeen')
            check(f'the {level} glow stands at its level in the task that sets it, no fade from 0',
                  len(seen) == 1 and abs(seen[0][0] - seen[0][1]) < 0.02, seen)
            check('  and in the next animation frame',
                  len(seen) == 1 and abs(seen[0][2] - seen[0][3]) < 0.02, seen)
    finally:
        b.close()
        d.stop()


def repick():
    """Picking the shown level only closes the menu: the smoke goes on, the cloud stays."""
    d = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        load(b, d.url)
        b.sleep(2.5)
        read(b)
        before = b.ev('window.__centroid()')
        live = b.ev('window.__live.length')
        b.ev("document.querySelector('canvas.smoke').__same = 1")
        pick(b, 'smoke')
        b.ev("window.__read('canvas.smoke', 4)")
        b.sleep(0.5)
        after = b.ev('window.__centroid()')
        check('re-picking Smoke: the same canvas, no new context',
              b.ev("!!document.querySelector('canvas.smoke').__same") is True
              and b.ev('window.__live.length') == live, [live, b.ev('window.__live.length')])
        check('  and the cloud is still there (alpha sum >= 60 % of before)',
              before is not None and after is not None and after[2] >= 0.6 * before[2],
              [before, after])
        check('  the menu closed, focus on the button',
              b.ev("document.querySelector('.fxpop').hidden") is True
              and b.ev('document.activeElement.id') == 'fx-button')
        check('  no popup', b.ev('window.__toasts') == [], b.ev('window.__toasts'))
        pick(b, 'glow')
        pick(b, 'glow')
        check('re-picking Glow: still Glow, the menu closed',
              b.ev("document.querySelector('.fx').dataset.level") == 'glow'
              and b.ev("document.querySelector('.fxpop').hidden") is True)
        check('no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        d.stop()


def calm_rhythm():
    """A meter's emission is a resting source for the rhythm; its red-zone kick stirs."""
    amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make(size=(1280, 900))
    try:
        load(b, amp.url)
        b.sleep(1.0)
        # the fake's signal comes in bursts, some into the red zone; the page's readings must
        # reach the smoke (the calm rate is measured elsewhere, the rule is proved in node)
        b.page("""(() => { window.__red = []; window.__fr = [];
          setInterval(() => {
            const r = document.querySelector('sv-meters').readings();
            if (Math.max(...r.map((x) => x ?? 0)) >= 0.93) window.__red.push(performance.now());
          }, 40);
          setInterval(() => window.__fr.push([performance.now(),
            window.__fx.stats().frames, window.__fx.stats().splats.meter]), 250);
          return 1; })()""")
        b.sleep(CALM_SECS)
        red, fr = b.page('window.__red'), b.page('window.__fr')
        emitted = fr[-1][2] - fr[0][2] if len(fr) > 1 else 0
        print(f'      amp with a signal, no pointer: {emitted} meter splats in {CALM_SECS:.0f} s; '
              f'red seen in {len(red)} samples')
        check('an amp playing: the page feeds the readings to the smoke (meter splats counted)',
              emitted > 0, emitted)
        b.close()
        # a handle of our own: a meter under the red zone is calm, one in it stirs
        b = make(size=(1280, 900))
        load(b, amp.url + '?effects=off')
        b.ev("window.__mk({ x: 640, y: 600, radius: 100, meters: [{ x: 600, y: 500 },"
             " { x: 680, y: 500 }] }, 0.5)")
        b.ev('window.__h.meters([0.6, 0.5])')
        b.sleep(3.5)
        f0 = b.ev('window.__h.stats().frames')
        b.sleep(2.0)
        low = (b.ev('window.__h.stats().frames') - f0) / 2.0
        # just into the red zone: its kick stirs, and its agitation is too small to make a note
        b.ev('window.__h.meters([0.935, 0.5])')
        b.sleep(0.5)
        f0 = b.ev('window.__h.stats().frames')
        b.sleep(2.0)
        red = (b.ev('window.__h.stats().frames') - f0) / 2.0
        print(f'      own handle: meters at 0.6 -> {low:.1f} frames a second; one at 0.935 -> '
              f'{red:.1f}')
        check('meters below the red zone: the calm rate', 14 <= low <= 26, low)
        check('a meter in its red zone stirs: nearly every animation frame (> 40 a second)',
              red > 40, red)
        check('no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        amp.stop()


def cost():
    """A splat is drawn inside its own box, and a pointer move reads no layout."""
    d = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        b.preload("""window.__cw = 0;
          const own = Object.getOwnPropertyDescriptor(Element.prototype, 'clientWidth');
          Object.defineProperty(Element.prototype, 'clientWidth', { configurable: true,
            get() { if (this.tagName === 'CANVAS') window.__cw++; return own.get.call(this); } });
          window.__sc = { draws: [], on: 0, off: 0, box: null, vp: null, live: false };
          const P = WebGL2RenderingContext.prototype;
          const sc = P.scissor, en = P.enable, di = P.disable, vp = P.viewport, dr = P.drawArrays;
          P.scissor = function (x, y, w, h) { window.__sc.box = [x, y, w, h];
            return sc.call(this, x, y, w, h); };
          P.enable = function (c) { if (c === 0x0C11) { window.__sc.on++; window.__sc.live = true; }
            return en.call(this, c); };
          P.disable = function (c) { if (c === 0x0C11) { window.__sc.off++;
            window.__sc.live = false; } return di.call(this, c); };
          P.viewport = function (x, y, w, h) { window.__sc.vp = [w, h];
            return vp.call(this, x, y, w, h); };
          // a scissored draw: the box against the size of the grid being drawn into
          P.drawArrays = function (...a) { if (window.__sc.live) window.__sc.draws.push(
            { box: window.__sc.box, grid: window.__sc.vp }); return dr.apply(this, a); };""")
        load(b, d.url + '?effects=off')
        b.ev("window.__mk({ x: 640, y: 450, radius: 100, meters: [] }, 0.6)")
        b.sleep(1.5)
        before = b.ev('window.__cw')
        b.ev("""(() => { let t = 0;
          for (let i = 0; i < 60; i++) {
            window.dispatchEvent(new PointerEvent('pointermove', { clientX: 300 + i * 8,
              clientY: 300 + (i % 7) * 3, pointerType: 'mouse', pointerId: 1, buttons: 0 }));
          } })()""")
        after = b.ev('window.__cw')
        check('60 pointer moves read no canvas layout size', after == before, (before, after))
        b.ev("window.__sc.draws = []; window.__sc.on = 0; window.__sc.off = 0;")
        for i in range(30):
            b.ev(f"window.dispatchEvent(new PointerEvent('pointermove', {{ clientX: {400 + i * 12},"
                 f" clientY: {500 + (i % 5) * 6}, pointerType: 'mouse', pointerId: 2,"
                 f" buttons: 1 }}))")
            b.sleep(0.03)
        b.sleep(0.5)
        sc = b.ev('window.__sc')
        small = [x for x in sc['draws']
                 if x['box'][2] * x['box'][3] < 0.3 * x['grid'][0] * x['grid'][1]]
        grids = sorted({tuple(x['grid']) for x in sc['draws']})
        print(f'      splats: {len(sc["draws"])} scissored draws on grids {grids}, {len(small)} '
              f'under 30 % of their own grid, SCISSOR_TEST on {sc["on"]} off {sc["off"]}')
        check('the pointer\'s splats are scissored to a box under 30 % of the grid drawn into',
              len(small) >= 20 and all(g[0] < 1280 or g[1] < 900 for g in grids),
              [len(sc['draws']), len(small), grids])
        check('  every scissor is switched off again', sc['on'] == sc['off'] and sc['on'] > 0, sc)
        off = b.ev("(() => { const gl = window.__live[0]; return gl.isEnabled(gl.SCISSOR_TEST); "
                   "})()")
        check('  and no scissor is left on after a frame', off is False, off)
        check('no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        d.stop()


sections = [lost_link, lazy_probe, start_throws, start_cleanup, first_paint, repick, calm_rhythm]
if browser == 'chromium':
    # Firefox's WebDriver scripts see the page's prototypes through wrappers, so the counters
    # `cost` hangs on them would not see the page's own calls
    sections.append(cost)
only = sys.argv[2:]
for section in sections:
    if not only or section.__name__ in only:
        section()
print(f'smoke_page.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
