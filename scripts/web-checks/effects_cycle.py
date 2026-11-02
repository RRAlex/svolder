# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The effects registry and the smoke's engine across Smoke -> Glow -> Smoke and a lost context,
numbers only. Usage: python3 effects_cycle.py chromium|firefox
Checks: Smoke and Glow are registered in the menu's order and the engine module is served; ten
Smoke -> Glow -> Smoke rounds by the menu leave one canvas, one live context, the loop's pace (draw
calls per simulated frame) and the calm rate, and the page's shade; a context the browser takes
and gives back during a settle or a climb's stream throws nothing, makes no note from the lost
settle and leaves the page on Glow (the smoke does not come back by itself: the page owns that)."""
import json
import sys

import common
from smoke_probe import PROBE

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
        return common.Chromium(**kw)
    return common.Firefox(**kw)


def errors(b):
    seen = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in seen if 'Failed to load resource' not in e]


def do(b, js):
    """Run statements (Firefox's `ev` wraps an expression)."""
    if browser == 'chromium':
        return b.ev(js)
    return b._post('/execute/sync', {'script': js, 'args': []})['value']


def load(b, url, wait=1.5):
    b.goto(url, settle=0.8)
    do(b, PROBE)
    for _ in range(50):
        if b.ev('window.__mod !== null'):
            break
        b.sleep(0.1)
    b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
           ".then((m) => { window.__fx = m; return 1; })")
    b.page("import(new URL('assets/themes/ember/smoke-engine.js', document.baseURI).href)"
           ".then((m) => { window.__eng = m; return 1; })")
    for _ in range(50):
        if b.page('window.__fx !== undefined && window.__eng !== undefined'):
            break
        b.sleep(0.1)
    b.sleep(wait)


def pick(b, level):
    """Choose a level by real clicks on the menu."""
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    x, y = common.center(b, f'.fxpop [data-level={level}]')
    b.click(x, y)


def shown(b):
    return b.ev("""(() => { const fx = document.querySelector('.fx');
      return { level: fx && fx.dataset.level,
        canvases: document.querySelectorAll('.fx canvas').length,
        live: window.__liveCount() }; })()""")


def pace(b, secs=2.0):
    """Simulated frames a second and draw calls per simulated frame over `secs`."""
    f0 = b.page('window.__fx.stats().frames')
    d0 = b.ev('window.__counts.draws')
    b.sleep(secs)
    f1 = b.page('window.__fx.stats().frames')
    d1 = b.ev('window.__counts.draws')
    return (f1 - f0) / secs, (d1 - d0) / max(1, f1 - f0)


def run():
    dac = common.Daemon('gungnir2-v2')
    b = make(size=(1280, 900))
    try:
        load(b, dac.url + '?effects=smoke')
        ids = b.page('window.__fx.registered().map((e) => e.id)')
        check('Smoke and Glow are registered, in the order of the menu (Off is no effect)',
              ids == ['smoke', 'glow'], ids)
        sup = b.page('window.__fx.registered().map((e) => e.supported().ok && '
                     "typeof e.start === 'function' && typeof e.label === 'string')")
        check('  each has a label, a supported() that says yes here and a start',
              sup == [True, True], sup)
        check('  the engine module is served and makes engines',
              b.page("typeof window.__eng.createEngine === 'function' && "
                     "typeof window.__eng.supported === 'function'") is True)
        # a plan from another director may omit what the engine's own director always sets
        partial = b.page("""(() => {
          const c = document.createElement('canvas');
          const e = window.__eng.createEngine(c);
          if (!e) return null;
          e.fit([640, 400]);
          e.run({ dt: 0.016, splats: [{ x: 0.5, y: 0.5, radius: 0.1, add: [4, 0, 0, 0],
            dye: [0.1, 0.1, 0.4, 0.5] }] });
          for (let i = 0; i < 6; i++) e.run({ dt: 0.016, splats: [] });
          const g = e.readback();
          const out = [g.vel.data.every(Number.isFinite), g.dye.data.every(Number.isFinite),
            Math.max(...g.vel.data.map(Math.abs))];
          e.stop();
          return out;
        })()""")
        check('a plan without velocityLoss or vorticity runs: the fields stay finite and the '
              'splat still moves the fluid', partial is not None and partial[0] and partial[1]
              and partial[2] > 0, partial)

        menu = b.page("[...document.querySelectorAll('.fxpop [role=menuitemradio]')]"
                      ".map((i) => [i.dataset.level, i.textContent.trim()])")
        reg = b.page("window.__fx.registered().map((e) => [e.id, e.label])")
        levels = b.page('window.__fx.LEVELS')
        check('the menu\'s level rows agree with the registry: the registered ids then off, in '
              'the order of LEVELS, with the registered labels',
              menu == reg + [['off', 'Off']] and [m[0] for m in menu] == levels,
              [menu, reg, levels])

        # ---- ten Smoke -> Glow -> Smoke rounds
        b.sleep(1.0)
        rate0, per0 = pace(b)
        shade0 = json.dumps(b.page('window.__fx.stats().shade'))
        worst = {'live': 0, 'canvases': 0}
        for _ in range(10):
            pick(b, 'glow')
            b.sleep(0.25)
            s = shown(b)
            worst = {k: max(worst[k], s[k]) for k in worst}
            check_glow = s['level'] == 'glow' and s['canvases'] == 0
            pick(b, 'smoke')
            b.sleep(0.25)
            s = shown(b)
            worst = {k: max(worst[k], s[k]) for k in worst}
            if not (check_glow and s['level'] == 'smoke' and s['canvases'] == 1):
                check('a round of the menu shows Glow with no canvas and Smoke with one',
                      False, (check_glow, s))
                break
        else:
            check('ten rounds by the menu: Glow shows no canvas, Smoke exactly one', True)
        s = shown(b)
        check('  one live context and one canvas at a time', worst['live'] <= 1
              and worst['canvases'] <= 1 and s['live'] == 1, [worst, s])
        b.sleep(1.5)
        rate1, per1 = pace(b)
        print(f'      calm rate {rate0:.1f} -> {rate1:.1f} frames/s; draw calls per frame '
              f'{per0:.1f} -> {per1:.1f}')
        calm = b.page("import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href)"
                      '.then((m) => m.TUNING.calmFps)')
        lo, hi = 0.7 * calm, 1.3 * calm  # 14 to 26 at the approved 20
        check(f'  the calm rate after ({lo:g} to {hi:g} frames a second)', lo <= rate1 <= hi, rate1)
        check('  one animation loop: draw calls per simulated frame as before (within 15 %)',
              per0 > 0 and abs(per1 - per0) <= 0.15 * per0, (per0, per1))
        shade1 = json.dumps(b.page('window.__fx.stats().shade'))
        check('  the page\'s shade is the one it drew', shade1 == shade0, (shade0, shade1))
        check('  no page error and no popup', errors(b) == [] and b.ev('window.__toasts') == [],
              [errors(b), b.ev('window.__toasts')])
        b.close()

        # ---- a context lost and given back, on a smoke of its own (the page's effects off)
        for what in ('a settle (a forced roll pending)', "a climb's stream", 'the control'):
            b = make(size=(1280, 900))
            load(b, dac.url + '?effects=off', wait=0.5)
            b.ev('window.__mk({ x: 640, y: 450, radius: 100, meters: [] }, 0.6)')
            b.ev('window.__h.roll(0)')
            b.sleep(1.0)
            if what.startswith('a settle') or what == 'the control':
                b.ev('window.__h.click(640, 450)')
            else:
                do(b, """window.__n = 0; window.__t = setInterval(() => { window.__h.step(1, 0.1,
                  0.5, true); if (++window.__n > 100) clearInterval(window.__t); }, 100);""")
            b.sleep(0.2)
            before = b.ev('window.__h.stats().notes.stamped')
            f0 = b.ev('window.__h.stats().frames')
            lose = what != 'the control'
            if lose:
                do(b, "window.__lc = document.getElementById('tc').getContext('webgl2')"
                     ".getExtension('WEBGL_lose_context'); window.__lc.loseContext()")
                b.sleep(0.5)
                b.ev('window.__lc.restoreContext()')
            waited = 0.0
            limit = 2.5 if lose else 3.5
            while waited < limit:
                if not lose and b.ev('window.__h.stats().notes.stamped') > before:
                    break
                b.sleep(0.1)
                waited += 0.1
            if not lose:
                print(f'      control: the note formed {waited:.1f} s after the lose point '
                      f'(the lost cases wait 2.5 s)')
            after = b.ev('window.__h.stats().notes.stamped')
            f1 = b.ev('window.__h.stats().frames')
            if not lose:
                check('control, nothing lost: the same settle does make a note', after > before,
                      (before, after))
                b.ev('window.__h.stop()')
                b.close()
                continue
            check(f'lost and restored during {what}: no note from the lost settle',
                  after == before, (before, after))
            check('  the lost smoke runs no more frames (the page decides what comes next)',
                  f1 - f0 <= 3, (f0, f1))
            b.ev('window.__h.stop()')
            check('  stopping it afterwards throws nothing', errors(b) == [], errors(b))
            b.close()

        # ---- the same on the page's own smoke: Glow takes over, nothing throws
        b = make(size=(1280, 900))
        load(b, dac.url + '?effects=smoke', wait=1.0)
        b.page('window.__fx.roll(0)')
        do(b, """window.__n = 0; window.__t = setInterval(() => { window.__fx.step(1, 0.1, 0.5,
          true); if (++window.__n > 60) clearInterval(window.__t); }, 100);""")
        b.sleep(0.5)
        b.ev("document.querySelector('canvas.smoke').getContext('webgl2')"
             ".getExtension('WEBGL_lose_context').loseContext()")
        b.sleep(1.0)
        s = shown(b)
        check('the page\'s smoke lost in a climb\'s stream: Glow shows, no canvas',
              s['level'] == 'glow' and s['canvases'] == 0, s)
        b.sleep(1.5)
        check('  the stream goes on into Glow without an error', errors(b) == [], errors(b))
        b.close()
    finally:
        dac.stop()
    print(f'effects_cycle {browser}: {passed} passed, {len(fails)} failed')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(run())
