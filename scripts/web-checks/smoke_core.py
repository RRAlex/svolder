# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The smoke's core in Chromium or Firefox, numbers only (readPixels; never a picture of Smoke).
Usage: python3 smoke_core.py chromium|firefox      Env: HIDDEN_SECS (default 8)
Checks: the level shown (Smoke / Glow / Off, reduced motion vs a stored choice), the cloud at the
anchor and a far corner holding only the haze, refusal and forced loss, the Smoke/Glow switches
and a device switch (one context, one canvas, no warning, no popup), a hidden tab, away, the calm
rate, sizing and scroll, and the column edges at 2560x1440."""
import json
import os
import sys

import common
from smoke_probe import PROBE

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
HIDDEN = float(os.environ.get('HIDDEN_SECS', '8'))
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
        return common.Chromium(block_cookies=kw.pop('block_storage', False), args=args, **kw)
    prefs = ['user_pref("webgl.disabled", true);'] if kw.pop('no_gl', False) else []
    return common.Firefox(extra_prefs=prefs, **kw)


def do(b, js):
    """Run statements (Firefox's `ev` wraps an expression)."""
    if browser == 'chromium':
        return b.ev(js)
    return b._post('/execute/sync', {'script': js, 'args': []})['value']


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def settle_import(b):
    for _ in range(50):
        if b.ev('window.__mod !== null'):
            return
        b.sleep(0.1)
    raise RuntimeError('smoke.js did not import')


def load(b, url, wait=0.3):
    b.goto(url, settle=0.8)
    do(b, PROBE)
    settle_import(b)
    b.sleep(wait)


def read(b, sel='canvas.smoke', frames=14, wait=0.6):
    b.ev(f'window.__read({json.dumps(sel)}, {frames})')
    for _ in range(100):
        b.sleep(wait if frames > 10 else 0.1)
        if b.ev('window.__rd !== null'):
            return
        wait = 0.1
    raise RuntimeError('probe did not finish')


def state(b):
    return b.ev("""(() => {
      const fx = document.querySelector('.fx');
      const item = document.querySelector('[data-level=smoke]');
      return { level: fx && fx.dataset.level,
        canvases: document.querySelectorAll('.fx canvas').length,
        glow: fx && getComputedStyle(fx.querySelector('.glow')).display !== 'none',
        smokeOff: item && item.getAttribute('aria-disabled'), title: item && item.title,
        checked: [...document.querySelectorAll('.fxpop [role=menuitemradio]')]
          .filter(i => i.getAttribute('aria-checked') === 'true').map(i => i.dataset.level) };
    })()""")


def pick(b, level):
    """Choose a level by real clicks on the menu."""
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    x, y = common.center(b, f'[data-level={level}]')
    b.click(x, y)


def anchor_of(b):
    return b.ev("(() => { const k = document.querySelector('sv-knob').geometry();"
                " return [k.cx, k.cy, k.radius]; })()")


def run():
    dac = common.Daemon('gungnir2-v2')
    two = common.Daemon('gungnir2-v2', more=[('Second', '1112131415161718')])
    b = make(size=(1280, 900))
    try:
        # ---- the default, the stored choice, reduced motion
        load(b, dac.url)
        s = state(b)
        check('default (nothing stored, no reduced motion): Smoke, one canvas, glow undrawn',
              s['level'] == 'smoke' and s['canvases'] == 1 and not s['glow'], s)
        b.sleep(1.2)
        read(b)
        cx, cy, r = anchor_of(b)
        near = b.ev(f'window.__meas({cx - 150}, {cy - 220}, {cx + 150}, {cy + 40})')
        corner = b.ev('window.__meas(0, 0, 60, 60)')
        peak = b.ev('window.__peak()')
        print(f'      at 1.7 s: alpha near the dial max {near["max"]} mean {near["mean"]:.2f}; '
              f'far corner max {corner["max"]}; whole canvas peak {peak}')
        check(
            'no GL error over the frames read',
            b.ev('window.__rd.err') == 0,
            b.ev('window.__rd.err'),
        )
        check(
            'after 1 s, the dial\'s neighbourhood holds non-transparent pixels',
            near['max'] >= 6,
            near,
        )
        check(
            'after 1 s, a far corner holds the haze\'s level, not the dial\'s (alpha under the '
            'Glow level\'s peak, 52, and under the dial\'s)',
            corner['max'] <= 52 and corner['max'] < near['max'],
            corner,
        )
        b.close()

        b = make(size=(1280, 900), reduced=True)
        load(b, dac.url)
        s = state(b)
        check('reduced motion, nothing stored: Glow, no canvas',
              s['level'] == 'glow' and s['canvases'] == 0 and s['glow'], s)
        b.ev("localStorage.setItem('svolder.effects', 'smoke') || 0")
        load(b, dac.url)
        b.sleep(1.2)
        s = state(b)
        read(b)
        peak = b.ev('window.__peak()')
        check('reduced motion with a stored smoke: the canvas exists and draws (Smoke, not Glow)',
              s['level'] == 'smoke' and s['canvases'] == 1 and not s['glow'] and peak > 0,
              [s, peak])
        b.close()

        # ---- picking Off and Glow removes the canvas; Smoke makes a fresh one
        b = make(size=(1280, 900))
        load(b, dac.url)
        pick(b, 'off')
        s = state(b)
        check('Off: the canvas is gone, no glow', s['level'] == 'off' and s['canvases'] == 0
              and not s['glow'], s)
        pick(b, 'glow')
        s = state(b)
        check('Glow: no canvas, the glow shows', s['level'] == 'glow' and s['canvases'] == 0
              and s['glow'], s)
        pick(b, 'smoke')
        b.sleep(1.5)
        s = state(b)
        check('Smoke again: one canvas, glow undrawn', s['level'] == 'smoke' and s['canvases'] == 1
              and not s['glow'], s)
        read(b)
        check('  ... and it draws', b.ev('window.__peak()') > 0)
        check('no page error so far', errors(b) == [], errors(b))
        b.close()

        # ---- refused: no WebGL here
        b = make(size=(1280, 900), no_gl=True)
        load_ok = True
        b.goto(dac.url)
        b.ev("localStorage.setItem('svolder.effects', 'smoke') || 0")
        b.goto(dac.url)
        s = state(b)
        check('WebGL refused: Glow shows, Smoke is unavailable with its reason',
              s['level'] == 'glow' and s['canvases'] == 0 and s['glow'] and s['smokeOff'] == 'true'
              and 'no WebGL2' in s['title'], s)
        pick(b, 'smoke')
        s = state(b)
        check('  choosing the disabled Smoke changes nothing', s['level'] == 'glow', s)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- forced loss
        b = make(size=(1280, 900))
        load(b, dac.url)
        b.sleep(0.8)
        b.ev("document.querySelector('canvas.smoke').getContext('webgl2')"
             ".getExtension('WEBGL_lose_context').loseContext()")
        b.sleep(1.0)
        s = state(b)
        toasts = b.ev('window.__toasts')
        check('lost context: Glow shows, no canvas, Smoke unavailable for the page\'s life',
              s['level'] == 'glow' and s['canvases'] == 0 and s['glow'] and s['smokeOff'] == 'true',
              s)
        check('  one popup, with the exact words', toasts == [
            "Smoke stopped: the browser's graphics were reset; showing the glow"], toasts)
        check('  the reason is the menu item\'s title', 'graphics were reset' in s['title'], s)
        pick(b, 'smoke')
        s = state(b)
        check('  Smoke cannot be chosen again', s['level'] == 'glow', s)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- a device switch (the Smoke/Glow rounds are effects_cycle.py's)
        b = make(size=(1280, 900))
        load(b, two.url)
        b.sleep(1.0)
        b.ev("document.querySelectorAll('.devs a.dev')[1].click()")
        b.sleep(1.5)
        s1 = state(b)
        b.ev("history.back()")
        b.sleep(1.5)
        s2 = state(b)
        live = b.ev('window.__liveCount()')
        check('device switch to the card-only screen and back: Smoke, one canvas, one context',
              s1['level'] == 'smoke' and s2['level'] == 'smoke' and s2['canvases'] == 1
              and s1['canvases'] == 1 and live <= 1, [s1, s2, live])
        read(b)
        check('  and the smoke draws after it', b.ev('window.__peak()') > 0)
        check(
            '  no popup after the device switch',
            b.ev('window.__toasts') == [],
            b.ev('window.__toasts'),
        )
        if browser == 'chromium':
            bad = [w for w in b.warnings if 'too many' in w.lower() or 'graphics' in w.lower()]
            check('  no "too many active WebGL contexts" warning in the console', not bad, bad)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- a hidden tab, a handle of its own
        b = make(size=(1280, 900))
        load(b, dac.url + '?effects=off')
        check(
            'test canvas starts',
            b.ev('window.__mk({ x: 400, y: 600, radius: 100, meters: [] }, 0.8)'),
        )
        b.sleep(1.5)
        f0 = b.ev('window.__h.stats().frames')
        rate = f0
        b.sleep(2.0)
        f1 = b.ev('window.__h.stats().frames')
        print(f'      calm rate: {(f1 - f0) / 2.0:.1f} simulated frames per second (rAF-driven)')
        read(b, '#tc', 8)
        check('test canvas: no GL error', b.ev('window.__rd.err') == 0, b.ev('window.__rd.err'))
        check('calm: about 20 simulated frames per second', 14 <= (f1 - f0) / 2.0 <= 26, (f0, f1))
        def hide(hold):
            do(b, """window.__hold = HOLD; // a real hidden tab holds its callbacks
              Object.defineProperty(document, 'visibilityState', { configurable: true,
                get: () => 'hidden' });
              document.dispatchEvent(new Event('visibilitychange'));""".replace(
                'HOLD', str(hold).lower()))

        def show():
            do(b, """delete document.visibilityState;
              window.__first = null;
              document.dispatchEvent(new Event('visibilitychange'));
              window.__release();
              requestAnimationFrame(() => { window.__first = window.__h.stats(); });""")
            b.sleep(0.5)
            return b.ev('window.__first')

        # (a) callbacks keep coming while hidden: the smoke's own logic must stop
        hide(False)
        b.sleep(0.3)
        d0, fr0 = b.ev('window.__counts.draws'), b.ev('window.__h.stats().frames')
        b.sleep(min(HIDDEN, 8))
        d1, fr1 = b.ev('window.__counts.draws'), b.ev('window.__h.stats().frames')
        check('hidden (callbacks still delivered, so the smoke\'s own rule stops it): no frames '
              'simulated and no draw calls', fr1 == fr0 and d1 == d0, (fr0, fr1, d0, d1))
        first = show()
        check('  the first frame after is one frame with dt < 0.049',
              first is not None and first['frames'] == fr1 + 1 and 0 <= first['dt'] < 0.049, first)
        b.sleep(0.5)
        # (b) a real hidden tab: its pending callback is held for the whole time
        hide(True)
        b.sleep(0.3)
        d0, fr0 = b.ev('window.__counts.draws'), b.ev('window.__h.stats().frames')
        b.sleep(HIDDEN)
        d1, fr1 = b.ev('window.__counts.draws'), b.ev('window.__h.stats().frames')
        check(f'hidden for {HIDDEN:.0f} s with its frame callback held (visibilityState '
              f'overridden, '
              f'visibilitychange dispatched): no frames simulated and no draw calls',
              fr1 == fr0 and d1 == d0, (fr0, fr1, d0, d1))
        first = show()
        check('the first frame after is one frame whose dt is the time since the page showed '
              '(< 0.049, under the 0.05 clamp: the held callback runs with a reset clock)',
              first is not None and first['frames'] == fr1 + 1 and 0 <= first['dt'] < 0.049, first)
        b.sleep(1.0)
        f2 = b.ev('window.__h.stats().frames')
        check('  and the loop runs on after it', f2 > fr1 + 5, (fr1, f2))
        do(b, "window.__h.stop(); document.getElementById('tc').remove()")

        # ---- away: grey, then stop; back: colour and frames
        b.ev('window.__mk({ x: 400, y: 600, radius: 100, meters: [] }, 0.8)')
        b.sleep(2.5)
        read(b, '#tc', 6)
        grey0 = b.ev('window.__grey()')
        pk0 = b.ev('window.__peak()')
        b.ev('window.__h.away(true)')
        b.sleep(0.7)
        b.ev("window.__read('#tc', 90)")
        b.sleep(2.2)
        fa = b.ev('window.__h.stats().frames')
        b.sleep(1.0)
        fb = b.ev('window.__h.stats().frames')
        grey1 = b.ev("window.__grey('lastNonEmpty')")
        pk1 = b.ev("window.__peak('lastNonEmpty')")
        chroma = b.ev('window.__chroma')
        print(f'      colour spread/alpha (mean over pixels with alpha >= 8; the light tint\'s own '
              f'{chroma:.3f}) before away {grey0["mean"]:.3f}, last frame after the fade '
              f'{grey1["mean"]:.3f}')
        check('away: the colour fades to grey (spread/alpha from > 0.7 to < 0.3 of the light '
              'tint\'s chroma, the mean over pixels with alpha >= 8, rounding to 8 bits '
              'included)',
              grey0['mean'] > 0.7 * chroma and grey1['seen'] > 0 and grey1['mean'] < 0.3 * chroma,
              [grey0, grey1, chroma])
        print(f'      away dim: densest alpha {pk0} -> {pk1} of 255 (awayOpacity 0.25)')
        check('away: the cloud dims as well (densest alpha at most 0.35 x its value before)',
              pk1 <= 0.35 * pk0 and pk1 > 0, [pk0, pk1])
        check('away, after its fade: frames stop', fa == fb, (fa, fb))
        b.ev('window.__h.away(false)')
        b.sleep(1.0)
        fc = b.ev('window.__h.stats().frames')
        read(b, '#tc', 12)
        grey2 = b.ev('window.__grey()')
        check('back from away: frames resume and the colour returns',
              fc > fb + 5 and grey2['mean'] > 0.7 * chroma, (fb, fc, grey2, chroma))
        do(b, "window.__h.stop(); document.getElementById('tc').remove()")
        check('stop: the handle frees its context (isContextLost)',
              b.ev('window.__liveCount()') == 0,
              b.ev('window.__liveCount()'))
        check('no page error (hidden, away)', errors(b) == [], errors(b))

        # a layer of no size for a moment, and bad deltas: no error, the cloud neither wiped nor
        # poisoned (the source is off, so only the old cloud can show afterwards)
        b.ev('window.__mk({ x: 400, y: 600, radius: 100, meters: [] }, 0.8)')
        b.sleep(2.5)
        b.ev('window.__h.anchor(null)')
        read(b, '#tc', 3)
        sum0 = b.ev('window.__centroid()')[2]
        do(b, "document.getElementById('tc').style.setProperty('width', '0');"
              " document.getElementById('tc').style.setProperty('height', '0');"
              " window.__h.scroll(0, 50); window.__h.scroll(NaN, 5); window.__h.scroll(0, "
              "Infinity);"
              " window.__h.anchor({ x: NaN, y: 3 });")
        b.sleep(0.4)
        do(b, "document.getElementById('tc').style.setProperty('width', '100%');"
              " document.getElementById('tc').style.setProperty('height', '100lvh');")
        b.sleep(0.2)
        read(b, '#tc', 8)
        cen = b.ev('window.__centroid()') or [float('nan')] * 3  # no cloud: the check fails
        pk = b.ev('window.__peak()')
        print(f'      after a 0x0 layer and bad deltas: alpha sum {sum0:.0f} -> '
              f'{(cen or [0, 0, 0])[2]:.0f}, peak {pk}')
        check('a 0x0 layer for a moment, then back: no GL error, the old cloud is still there '
              '(alpha sum > 60 % of before) and sane (peak < 150)',
              b.ev('window.__rd.err') == 0 and cen is not None and cen[2] > 0.6 * sum0
              and 0 < pk < 150,
              [sum0, cen, pk])
        check('  no page error', errors(b) == [], errors(b))
        do(b, "window.__h.stop(); document.getElementById('tc').remove()")
        b.close()

        # ---- scroll moves the grids; resize keeps the cloud
        b = make(size=(390, 844))
        load(b, dac.url + '?effects=off')
        b.ev('window.__mk({ x: 195, y: 560, radius: 100, meters: [] }, 0.9)')
        b.sleep(2.5)
        b.ev('window.__h.anchor(null)')
        read(b, '#tc', 3)
        c0 = b.ev('window.__centroid()')
        do(b, "window.__rd = null; window.__h.scroll(0, 300);"
             " window.__read('#tc', 1)")
        b.sleep(0.4)
        c1 = b.ev('window.__centroid()')
        print('      scroll(0, 300): the cloud\'s centroid y '
              + ' -> '.join('no frame' if c is None else f'{c[1]:.0f}' for c in (c0, c1)))
        check('scroll(0, 300): a frame was read, and the cloud moves up by about 300 px (the dye '
              'rises a little too)',
              c0 is not None and c1 is not None and 255 <= c0[1] - c1[1] <= 345
              and abs(c0[0] - c1[0]) < 15, [c0, c1])
        b.ev('window.__h.scroll(0, -1000)')
        b.sleep(0.4)
        read(b, '#tc', 6)
        check('what leaves the viewport is gone (scroll 1000 px away: nothing left)',
              (b.ev('window.__peak()') or 0) <= 3, b.ev('window.__peak()'))
        # a resize moves the old cloud into the new grids, not a wipe (no source feeding it now)
        b.ev('window.__h.anchor({ x: 195, y: 500, radius: 100, meters: [] })')
        b.sleep(2.5)
        b.ev('window.__h.anchor(null)')
        read(b, '#tc', 3)
        s0 = b.ev('window.__centroid()')[2]
        b.resize(390, 700)
        b.ev("window.__read('#tc', 3)")
        b.sleep(0.4)
        s1 = b.ev('window.__centroid()')
        print(f'      resize 390x844 -> 390x700 with no source: alpha sum {s0:.0f} -> '
              f'{(s1 or [0, 0, 0])[2]:.0f}')
        check('a resize resamples the cloud into the new grids (alpha sum keeps > 30 % after '
              '0.4 s, with the source off)', s1 is not None and s1[2] > 0.3 * s0, [s0, s1])
        do(b, "window.__h.stop(); document.getElementById('tc').remove()")
        b.close()

        b = make(size=(390, 844))
        load(b, dac.url)
        b.sleep(2.0)
        read(b)
        cx, cy, r = anchor_of(b)
        before = b.ev(f'window.__meas({cx - 120}, {cy - 160}, {cx + 120}, {cy + 40})')
        b.resize(390, 760)
        b.sleep(1.0)
        read(b)
        cx2, cy2, _ = anchor_of(b)
        after = b.ev(f'window.__meas({cx2 - 120}, {cy2 - 160}, {cx2 + 120}, {cy2 + 40})')
        peak = b.ev('window.__peak()')
        print(f'      390x844 -> 390x760: near the dial max {before["max"]} -> {after["max"]}, '
              f'mean {before["mean"]:.2f} -> {after["mean"]:.2f}')
        check('a 390x844 -> 390x760 resize keeps non-empty pixels (resampled, not wiped)',
              after['max'] >= 0.5 * before['max'] and peak > 0, [before, after])
        sw = b.ev('[document.scrollingElement.scrollWidth, document.documentElement.clientWidth]')
        check('  no horizontal scroll with the smoke layer at 390', sw[0] == sw[1], sw)
        b.resize(390, 844)
        b.sleep(0.5)
        scrollable = b.ev('document.scrollingElement.scrollHeight - innerHeight')
        cx, cy, _ = anchor_of(b)
        step = min(300, int(cy - 120)) if scrollable >= 300 else 0
        read(b)
        c0 = b.ev('window.__centroid()')
        b.ev(f'window.scrollTo(0, {step})')
        b.sleep(0.15)
        read(b, 'canvas.smoke', 14)
        c1 = b.ev('window.__centroid()')
        cx2, cy2, _ = anchor_of(b)
        print(f'      phone page scrolled {step} px (dial y {cy:.0f} -> {cy2:.0f}): cloud centroid '
              f'y {c0[1]:.0f} -> {c1[1]:.0f}')
        check('phone page scrolled: the cloud follows the dial (centroid moves by the dial\'s '
              'move, '
              '+-60 px)', step > 0 and abs((c0[1] - c1[1]) - (cy - cy2)) <= 60, [c0, c1, cy, cy2])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- Smoke fails to start although supported() said yes: unavailable, one popup, no loop
        b = make(size=(1280, 900))
        load(b, dac.url + '?effects=glow')
        # the menu's first opening probes WebGL (it passes); the start that follows fails
        x, y = common.center(b, '.fxbtn')
        b.click(x, y)
        b.click(x, y)
        do(b, "const g = HTMLCanvasElement.prototype.getContext;"
              " HTMLCanvasElement.prototype.getContext = function (k, ...r) {"
              " return k === 'webgl2' ? null : g.call(this, k, ...r); };")
        live0 = b.ev('window.__live.length')
        pick(b, 'smoke')
        s = state(b)
        t1 = b.ev('window.__toasts')
        pick(b, 'smoke')
        pick(b, 'smoke')
        t2 = b.ev('window.__toasts')
        s2 = state(b)
        check('a start that fails after supported(): Glow, Smoke disabled with "Smoke could not '
              'start '
              'here", one popup',
              s['level'] == 'glow' and s['canvases'] == 0 and s['smokeOff'] == 'true'
              and s['title'] == 'Smoke could not start here' and len(t1) == 1, [s, t1])
        check('  choosing it again makes no new canvas, context or popup',
              s2['canvases'] == 0 and t2 == t1 and b.ev('window.__live.length') == live0,
              [s2, t2])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- the menu open while a history step brings the page back: no offset is copied (CSP)
        b = make(size=(1280, 900))
        load(b, two.url + '?effects=glow')
        b.ev("document.querySelectorAll('.devs a.dev')[1].click()")
        b.sleep(1.5)
        b.ev("document.querySelector('.fxbtn').click()")
        b.sleep(0.3)
        check(
            'the menu is open with its offset',
            b.ev("document.querySelector('.fxpop').hasAttribute('style')"),
        )
        b.ev('history.back()')
        b.sleep(1.5)
        check('history step with the menu open: the menu is closed and no style attribute is left',
              b.ev("(() => { const p = document.querySelector('.fxpop');"
                   " return p.hidden && !p.hasAttribute('style'); })()"))
        check('  no page error / CSP report (Chromium logs CSP; Firefox does not log it headless)',
              errors(b) == [], errors(b))
        b.close()

        # ---- the keyboard flow with Smoke available
        b = make(size=(1280, 900))
        load(b, dac.url)
        b.sleep(1.0)

        def kb():
            return b.ev("(() => "
                        "{ const a = document.activeElement;"
                        " const fx = document.querySelector('.fx');"
                        " const pop = document.querySelector('.fxpop');"
                        " return { focus: a && (a.dataset && a.dataset.level || a.id || a.tagName),"
                        " open: !pop.hidden, level: fx.dataset.level,"
                        " canvases: document.querySelectorAll('.fx canvas').length }; })()")
        b.ev("document.querySelector('.fxbtn').focus()")
        b.press('Enter')
        k = kb()
        check('keys: Enter opens with focus on the checked item (Smoke)',
              k['open'] and k['focus'] == 'smoke', k)
        b.press('ArrowDown')
        check('  ArrowDown reaches Glow', kb()['focus'] == 'glow', kb())
        b.press('ArrowDown')
        check('  ArrowDown reaches Off', kb()['focus'] == 'off', kb())
        b.press('ArrowDown')
        check('  ArrowDown reaches the Arrange row', kb()['focus'] == 'menu-arrange', kb())
        b.press('ArrowDown')
        check('  ArrowDown wraps to the Theme row', kb()['focus'] == 'menu-theme', kb())
        b.press('End')
        check('  End lands on the Arrange row (the last the arrows reach)',
              kb()['focus'] == 'menu-arrange', kb())
        b.press('Home')
        check('  Home lands on the Theme row', kb()['focus'] == 'menu-theme', kb())
        b.press('Escape')
        k = kb()
        check(
            '  Escape closes it, focus back on the button',
            not k['open'] and k['focus'] == 'fx-button',
            k,
        )
        b.press('Enter')
        b.press('ArrowDown')
        b.press('Enter')
        k = kb()
        check('  Enter on Glow picks it: the canvas is gone, the menu closed, focus on the button',
              k['level'] == 'glow' and k['canvases'] == 0 and not k['open']
              and k['focus'] == 'fx-button', k)
        b.press('Enter')
        b.press('ArrowUp')
        check('  ArrowUp from Glow reaches Smoke', kb()['focus'] == 'smoke', kb())
        b.press('Enter')
        b.sleep(1.5)
        k = kb()
        read(b)
        check('  Enter on Smoke starts it: one canvas that draws, focus on the button',
              k['level'] == 'smoke' and k['canvases'] == 1 and b.ev('window.__peak()') > 0
              and k['focus'] == 'fx-button', k)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- an amp: the resting source sits between the meters
        amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
        try:
            b = make(size=(1280, 900))
            load(b, amp.url)
            b.sleep(2.5)
            read(b)
            mc = b.ev("(() => "
                      "{ const r = document.querySelector('.meters').getBoundingClientRect();"
                      " return [r.left + r.width / 2, r.top + r.height / 2]; })()")
            near = b.ev(f'window.__meas({mc[0] - 150}, {mc[1] - 250}, {mc[0] + 150}, {mc[1] + 60})')
            corner = b.ev('window.__meas(0, 0, 60, 60)')
            cen = b.ev('window.__centroid()')
            print(f'      amp: meters centre ({mc[0]:.0f},{mc[1]:.0f}); alpha near it max '
                  f'{near["max"]}; '
                  f'cloud centroid ({cen[0]:.0f},{cen[1]:.0f}); far corner max {corner["max"]}')
            check('amp: the cloud sits at the meters\' centre (dye there, centroid within 150 px '
                  'sideways and 150 px up or down)', near['max'] >= 6
                  and abs(cen[0] - mc[0]) < 150 and abs(cen[1] - mc[1]) < 150, [near, cen, mc])
            check('  amp: a far corner holds the haze\'s level, not the meters\' (alpha under the '
                  'Glow level\'s peak, 52, and under theirs), no GL error',
                  corner['max'] <= 52 and corner['max'] < near['max']
                  and b.ev('window.__rd.err') == 0, corner)
            check('  amp: no page error', errors(b) == [], errors(b))
            b.close()
        finally:
            amp.stop()

        # ---- the backing size: CSS size x min(dpr, 1.5)
        if browser == 'chromium':
            b = make(size=(800, 600), dpr=2)
            load(b, dac.url)
            wh = b.ev("(() => { const c = document.querySelector('canvas.smoke');"
                      " return [c.width, c.height, c.clientWidth, c.clientHeight]; })()")
            check('dpr 2: the backing store is the CSS size x 1.5 (dprCap)',
                  abs(wh[0] - wh[2] * 1.5) <= 1 and abs(wh[1] - wh[3] * 1.5) <= 1, wh)
            b.close()

        # ---- 2560x1440: the column's edges
        b = make(size=(2560, 1440))
        load(b, dac.url)
        b.sleep(3.0)
        # readPixels reads the drawing buffer, which no CSS clip touches: that the canvas is not
        # clipped to the column is read from its box and from what could clip it
        geo = b.ev("""(() => {
          const c = document.querySelector('canvas.smoke'), r = c.getBoundingClientRect();
          const root = document.documentElement;
          const bad = [];
          const layer = c.parentElement;
          for (let n = c; n && n !== document; n = n.parentElement) {
            const st = getComputedStyle(n);
            const names = [];
            if (st.clipPath !== 'none') names.push('clip-path ' + st.clipPath);
            if (st.mask !== 'none' && st.maskImage !== 'none') names.push('mask');
            if (st.clip !== 'auto' && st.position !== 'static') names.push('clip ' + st.clip);
            if (/paint|layout|strict|content/.test(st.contain)) names.push('contain ' + st.contain);
            if (n !== c && n !== layer && st.overflow !== 'visible' && n !== root
              && n !== document.body) names.push('overflow ' + st.overflow);
            if (n !== c) {
              for (const k of ['transform', 'filter', 'perspective']) {
                if (st[k] !== 'none') names.push(k + ' ' + st[k]);
              }
            }
            if (names.length) bad.push((n.id || n.className || n.tagName) + ': ' + names.join());
          }
          const lr = layer.getBoundingClientRect();
          return { box: [r.left, r.top, r.width, r.height],
            view: [0, 0, root.clientWidth, innerHeight], layerBox: [lr.left, lr.top, lr.width,
              lr.height], parent: c.parentElement.matches('#theme > .fx:first-child'), bad,
            overflow: getComputedStyle(c).overflow };
        })()""")
        check('2560x1440: the canvas\'s box is the viewport\'s',
              all(abs(x - y) <= 1 for x, y in zip(geo['box'], geo['view'])), geo)
        check('  its parent is #theme > .fx:first-child, whose box is the viewport\'s too',
              geo['parent'] and all(abs(x - y) <= 1 for x, y in zip(geo['layerBox'], geo['view'])),
              geo)
        check('  nothing from the canvas to the root clips it (clip-path, mask, contain, an '
              'overflow, a transform)', geo['bad'] == [] and geo['overflow'] != 'hidden', geo)
        v = b.ev("(() => "
                 "{ const v = document.querySelector('#screen .view').getBoundingClientRect();"
                 " const k = document.querySelector('sv-knob').geometry();"
                 " return [v.left, v.right, k.cy]; })()")
        left, right, cy = v
        for side, e in (('left', left), ('right', right)):
            # a pressed pointer dragged to and fro over the column's edge leaves dye there
            for i in range(120):
                b.ev(f"window.dispatchEvent(new PointerEvent('pointermove', {{ "
                     f"clientX: {e - 60 + (i % 24) * 5}, clientY: {cy - 60 + (i % 7) * 18}, "
                     f"pointerType: 'mouse', pointerId: 5, buttons: 1 }}))")
                b.sleep(0.02)
            read(b)
            m = b.ev(f'window.__edge({e}, {cy - 150}, {cy + 150})')
            print(f'      2560, the column\'s {side} edge x={e:.0f}: step {m["edge"]}, steps '
                  f'beside {m["beside"]}, alpha at the edge {m["atMax"]}')
            check(f'2560x1440: dye over the column\'s {side} edge shows there (alpha >= 6) and has '
                  f'no hard edge (the step across it <= 1.5 x the steps beside it + 2; a hard '
                  f'edge steps by the cloud\'s alpha)',
                  m['atMax'] >= 6 and m['edge'] <= 1.5 * max(m['beside'], 1) + 2, m)
        b.goto(dac.url + '?effects=off')
        do(b, PROBE)
        settle_import(b)
        v = b.ev("(() => "
                 "{ const v = document.querySelector('#screen .view').getBoundingClientRect();"
                 " return [v.left, v.right]; })()")
        b.ev(f'window.__mk({{ x: {v[0]}, y: 700, radius: 100, meters: [] }}, 1)')
        b.sleep(3.5)
        read(b, '#tc')
        m = b.ev(f'window.__edge({v[0]}, 300, 760, "best")')
        print(f'      smoke placed on the column\'s left edge: step {m["edge"]}, steps beside '
              f'{m["beside"]}, alpha at the edge {m["atMax"]}')
        check('a cloud straddling the column\'s edge has no hard edge there (the step across it <= '
              '1.5 x the turbulence\'s own steps beside it + 2)',
              m['atMax'] >= 6 and m['edge'] <= m['beside'] * 1.5 + 2, m)
        check('no page error (2560)', errors(b) == [], errors(b))
        b.close()
    finally:
        b.close()
        dac.stop()
        two.stop()


run()
print(f'smoke_core.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
