# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A device switch, a history step and a theme mounted again with the effects layer, in Chromium: the second device is
a card-only screen (not found: no dial, away), so this also covers the layer's fallback place and
its away look. Usage: python3 effects_devices.py"""
import sys

import common

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


GLOW = """(() => { const g = document.querySelector('.glow').getBoundingClientRect();
  const k = document.querySelector('sv-knob'); const v = document.querySelector('#screen .view');
  const vr = v.getBoundingClientRect(); const d = k && k.svg ? k.geometry() : null;
  return { gx: g.left + g.width / 2, gy: g.top + g.height / 2, dial: d && [d.cx, d.cy],
           fallback: [vr.left + vr.width / 2, vr.top + 170],
           away: document.querySelector('.fx').classList.contains('away'),
           filter: getComputedStyle(document.querySelector('.glow')).filter,
           fx: document.querySelectorAll('.fx').length,
             btn: document.querySelectorAll('.fxbtn').length,
           level: document.querySelector('.fx').dataset.level }; })()"""


def near(a, b):
    return abs(a[0] - b[0]) <= 2 and abs(a[1] - b[1]) <= 2


d = common.Daemon(more=[('Second', '1112131415161718')])
b = common.Chromium(size=(1280, 900))
try:
    b.goto(d.url + '?effects=glow')
    s = b.ev(GLOW)
    check(
        'first device: glow on the dial, not away',
        near([s['gx'], s['gy']], s['dial']) and not s['away'],
        s,
    )
    b.ev("document.querySelectorAll('.devs a.dev')[1].click()")
    b.sleep(1.5)
    s = b.ev(GLOW)
    check('second device (card only): glow at the fallback place (column centre, 170 px down)',
          s['dial'] is None and near([s['gx'], s['gy']], s['fallback']), s)
    check('second device: the away look (grey)', s['away'] and 'grayscale' in s['filter'], s)
    b.ev("history.back()")
    b.sleep(1.5)
    s = b.ev(GLOW)
    check('history step back: one layer, one button, glow back on the dial, not away',
          s['fx'] == 1 and s['btn'] == 1 and s['dial'] and near([s['gx'], s['gy']], s['dial'])
          and not s['away'], s)
    check('  the level of the page stands (Glow)', s['level'] == 'glow', s)
    # the menu works on the page the history step brought back
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    x, y = common.center(b, '[data-level=off]')
    b.click(x, y)
    s = b.ev(GLOW)
    check('  the menu on the restored page switches the new layer', s['level'] == 'off', s)
    b.ev("document.querySelectorAll('.devs a.dev')[1].click()")
    b.sleep(1.5)
    s = b.ev(GLOW)
    check('a device switch keeps the chosen level (Off)', s['level'] == 'off' and s['fx'] == 1, s)
    # the theme unmounts and mounts again: one layer, one button, the level kept, the menu working
    b.ev("""import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
      const registry = await import(new URL('assets/core/registry.js', document.baseURI));
      await (await m.mounted).unmount();
      const gone = document.querySelectorAll('.fx, .fxbtn').length;
      await registry.mountTheme({ registry: m.core.registry, id: 'ember',
        root: document.getElementById('theme'), core: m.core, toast: m.core.frame.toast });
      return gone; })""")
    b.sleep(1.5)
    s = b.ev(GLOW)
    check('mounted again: one layer, one button, the chosen level (Off) kept',
          s['fx'] == 1 and s['btn'] == 1 and s['level'] == 'off', s)
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)
    x, y = common.center(b, '[data-level=glow]')
    b.click(x, y)
    s = b.ev(GLOW)
    check('  and its menu switches the new layer', s['level'] == 'glow', s)
    check('no page error', b.errors == [], b.errors)
finally:
    b.close()
    d.stop()
print(f'effects_devices.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
