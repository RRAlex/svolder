# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The page's small motions with the system asking for reduced motion (emulated): they all run as
for anyone, and only the first visit's default effects level (Glow in place of Smoke) follows the
setting. The effects menu's Off is the way to still the page. Checks: the first default is Glow
and a picked level wins; the glow turns, follows its level and fades at the page's own speeds; the
dot of a link that is not up pulses; the knob eases between two readings; an amp's held peak falls
by steps; the tone curve eases from one shape to the next and a changed band value flashes; the
phone's cut Now playing title loops; and with the effects Off the rest of the page is still (the
title's loop stops only when the card opens). Numbers from the page, no pictures.
Usage: python3 reduced_motion.py chromium|firefox"""
import json
import sys
import urllib.request

import common

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
    return (common.Chromium if browser == 'chromium' else common.Firefox)(reduced=True, **kw)


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def post(dac, path, body):
    urllib.request.urlopen(urllib.request.Request(
        f'{dac.url}api/v1/devices/Gungnir%202/{path}', data=json.dumps(body).encode(),
        method='POST', headers={'Content-Type': 'application/json',
                                'Origin': dac.url.rstrip('/')}), timeout=10).read()


# ------------------------------------------------------------------ the effects layer
dac = common.Daemon('gungnir2-v2')
b = make()
try:
    b.goto(dac.url, settle=1.0)
    s = b.ev("""({ reduced: matchMedia('(prefers-reduced-motion: reduce)').matches,
      level: document.querySelector('.fx').dataset.level })""")
    check('the emulation holds: the page sees reduced motion', s['reduced'] is True, s)
    check('the first default is Glow, as it always was under reduced motion',
          s['level'] == 'glow', s)
    glow = """(() => { const g = document.querySelector('.glow'); const cs = getComputedStyle(g);
      return { name: cs.animationName, dur: cs.animationDuration, iter: cs.animationIterationCount,
        running: document.getAnimations().filter((a) => a.animationName === 'glow-turn'
          && a.playState === 'running').length,
        fade: cs.transitionDuration,
        lvl: getComputedStyle(document.querySelector('.fx')).getPropertyValue('--lvl').trim() };
    })()"""
    g = b.ev(glow)
    check('the glow turns: its animation runs, once in 35 s, over and over',
          g['name'] == 'glow-turn' and g['dur'] == '35s' and g['iter'] == 'infinite'
          and g['running'] == 1, g)
    check('  and it fades between levels at the page\'s own speed, 1 s',
          g['fade'] == '1s', g)
    b.ev("document.querySelector('.fx').classList.add('hearing')")
    check('  and keeps up with the music at .3 s', b.ev(glow)['fade'] == '0.3s', b.ev(glow))
    b.ev("document.querySelector('.fx').style.setProperty('--lvl', '.9')")
    check('  its level is not pinned at .6: the page sets it',
          b.ev(glow)['lvl'] == '.9', b.ev(glow))
    # the dot of a link that is not up pulses
    pulse = b.ev("""(() => { const l = document.querySelector('.link'); l.classList.add('wait');
      const cs = getComputedStyle(l.querySelector('i'));
      return { name: cs.animationName, iter: cs.animationIterationCount }; })()""")
    check('the dot of a link that is not up pulses', pulse['name'] == 'pulse'
          and pulse['iter'] == 'infinite', pulse)
    # transitions are not cut either: a button's border colour eases, as it does for anyone
    tr = b.ev("""getComputedStyle(document.querySelector('.fxbtn')).transitionDuration""")
    check('a control\'s transition keeps its time (nothing is cut to 0 s)',
          tr not in ('0s', '0s, 0s'), tr)
    # the knob eases between two readings
    b.ev("""(() => { const k = document.querySelector('sv-knob'); window.__from = k.shown;
      window.__seen = []; k.glide(k.shown - 6);
      const frame = () => { window.__seen.push(k.shown); if (window.__seen.length < 12)
        requestAnimationFrame(frame); };
      requestAnimationFrame(frame); })()""")
    b.sleep(0.6)
    k = b.ev("""({ from: window.__from, seen: window.__seen,
      end: document.querySelector('sv-knob').shown })""")
    mid = [v for v in k['seen'] if k['end'] < v < k['from']]
    check('the knob eases between two readings: some frame stands between them',
          k['from'] is not None and k['end'] == k['from'] - 6 and len(mid) >= 1, k)
    check('no page error', errors(b) == [], errors(b))
    # picking a level wins over the default, and Off stills the page
    b.ev("document.querySelector('.fx').classList.remove('hearing')")
    b.goto(dac.url + '?effects=off', settle=1.0)
    off = b.ev("""(() => { const fx = document.querySelector('.fx');
      return { level: fx.dataset.level,
        shown: getComputedStyle(fx.querySelector('.glow')).display,
        running: document.getAnimations().filter((a) => a.animationName === 'glow-turn'
          && a.playState === 'running').length }; })()""")
    check('with the effects Off the glow is gone and nothing of it turns',
          off['level'] == 'off' and off['shown'] == 'none' and off['running'] == 0, off)
    b.goto(dac.url + '?effects=glow', settle=1.0)
    check('a level asked for is the level, as for anyone',
          b.ev("document.querySelector('.fx').dataset.level") == 'glow')
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the tone curve
dac = common.Daemon('gungnir2-v2')
b = make()
try:
    post(dac, 'eq/mode', {'mode': 'eq'})
    b.goto(dac.url, settle=1.2)
    b.ev("""(() => { window.__d = new Set(); window.__flash = 0;
      const frame = () => {
        const line = document.querySelector('#part-tone .eqg .line');
        if (line) window.__d.add(line.getAttribute('d'));
        window.__flash = Math.max(window.__flash,
          document.querySelectorAll('#part-bands .band span.flash').length);
        window.__go = requestAnimationFrame(frame); };
      frame(); })()""")
    b.sleep(0.3)
    start = b.ev('window.__d.size')
    post(dac, 'eq/bands/3', {'param': 'gain', 'raw': 120})
    b.sleep(1.2)
    seen = b.ev('({ shapes: window.__d.size, flash: window.__flash })')
    check('the tone curve eases from the old shape to the new: shapes between them are drawn',
          start >= 1 and seen['shapes'] >= start + 3, [start, seen])
    check('the changed band value flashes', seen['flash'] >= 1, seen)
    check('no page error with the tone curve', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ an amp's meters
amp = common.Daemon('fake-amp', name='Fake amp')
b = make()
try:
    b.goto(amp.url, settle=1.5)
    held = b.ev("""(() => { const m = document.querySelector('sv-meters');
      const arch = [...m.arches.values()][0];
      arch.fraction = 0.2; arch.peak = 0.9; arch.heldAt = performance.now() - 5000;
      m.fallStep(); const one = arch.peak;
      m.fallStep(); return { one, two: arch.peak }; })()""")
    check('a held peak falls by steps (0.03 each), not at one jump to the reading',
          abs(held['one'] - 0.87) < 1e-9 and abs(held['two'] - 0.84) < 1e-9, held)
    check('no page error on the amp page', errors(b) == [], errors(b))
finally:
    b.close()
    amp.stop()

# ------------------------------------------------------------------ the folded title's loop
dac = common.Daemon(host='fake', env={'SVOLDER_FAKE_TITLE': 'Everything In Its Right Place'})
b = make(size=(341, 800))
try:
    b.goto(dac.url + '?effects=off', settle=1.0)
    loop = """(() => { const s = document.querySelector('.np-title').firstElementChild;
      const t = document.querySelector('.np-title'); const a = s.getAnimations()[0];
      return { reduced: matchMedia('(prefers-reduced-motion: reduce)').matches,
        cut: t.classList.contains('cut'), anims: s.getAnimations().length,
        inf: !!a && a.effect.getTiming().iterations === Infinity,
        x: s.getBoundingClientRect().left - t.getBoundingClientRect().left }; })()"""
    g = b.ev(loop)
    b.sleep(3.0)
    h = b.ev(loop)
    check('the phone\'s cut title still loops under reduced motion, and it moves',
          g['reduced'] is True and g['cut'] and g['anims'] == 1 and g['inf'] and h['x'] < -5,
          [g, h])
    check('no page error with the title\'s loop', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

print(f'{browser}: {passed} checks passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
