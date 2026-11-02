# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The sweep: the DAC page, an amp page, a Loki Max page (its own presets part) and a lost-link
page at 390x844, 1280x900 and 2560x1440, at each effects level (?effects=smoke, glow, off), in
Chromium or Firefox. Each load must raise no uncaught error, no console error and no Content
Security Policy or WebGL-context warning (Chromium reports those; Firefox's page errors come from a
BiDi preload collector), show no horizontal scroll, and show the level's layer: one smoke canvas on
Smoke, the glow on Glow, neither on Off. Numbers only, no pictures.
Usage: python3 sweep.py chromium|firefox"""
import re
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
SIZES = ((390, 844), (1280, 900), (2560, 1440))
LEVELS = ('smoke', 'glow', 'off')
SETTLE = 1.2  # seconds a load settles before it is read
# a warning that matters here (the software GL's own notes, as headless Chromium prints them, do
# not)
BAD_WARNING = re.compile(r'content security policy|too many active|context lost|graphics', re.I)
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


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


STATE = """(() => ({
  canvases: document.querySelectorAll('canvas.smoke').length,
  glow: !!document.querySelector('.glow') && getComputedStyle(document.querySelector('.glow'))
    .display !== 'none',
  scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
  level: document.querySelector('.fx') && document.querySelector('.fx').dataset.level,
}))()"""


def sweep(label, daemon, wait, pill):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=SIZES[0])
    try:
        for size in SIZES:
            b.resize(*size)
            for level in LEVELS:
                b.goto(f'{daemon.url}?effects={level}', settle=wait)
                s = b.ev(STATE)
                at = f'{label} {size[0]}x{size[1]} ?effects={level}'
                word = b.ev("document.getElementById('part-link').textContent.trim()")
                check(f'{at}: the link pill reads "{pill}"', word == pill, word)
                errs = errors(b)
                warns = [w for w in getattr(b, 'warnings', []) if BAD_WARNING.search(w)]
                check(f'{at}: no error, no CSP or context warning', not errs and not warns,
                      [errs, warns])
                check(f'{at}: no horizontal scroll', s['scroll'][0] <= s['scroll'][1], s)
                if level == 'smoke':
                    # Smoke runs where WebGL2 does: both browsers here grant it
                    check(
                        f'{at}: one smoke canvas, no glow',
                        s['canvases'] == 1 and not s['glow'],
                        s,
                    )
                else:
                    check(f'{at}: no smoke canvas, the glow '
                          f'{"shown" if level == "glow" else "gone"}',
                          s['canvases'] == 0 and s['glow'] == (level == 'glow'), s)
                if browser == 'chromium':
                    b.errors.clear()
                    b.warnings.clear()
    finally:
        b.close()


daemons = []
try:
    dac = common.Daemon('gungnir2-v2')
    daemons.append(dac)
    amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    daemons.append(amp)
    # the link is cut on the check's own cue, before its sweep; the pill's word is checked on every
    # load
    loki = common.Daemon('lokimax', name='Loki Max')
    daemons.append(loki)
    gone = common.Daemon('gungnir2-v2')
    daemons.append(gone)
    link = gone.model()['link']['state']
    check('lost link: the device is connected before it is dropped', link == 'on', link)
    gone.drop()
    for _ in range(100):
        link = gone.model()['link']['state']
        if link != 'on':
            break
        time.sleep(0.1)
    check('lost link: the device is gone once dropped', link != 'on', link)
    sweep('DAC', dac, SETTLE, 'connected')
    sweep('amp', amp, SETTLE, 'connected')
    sweep('Loki Max', loki, SETTLE, 'connected')
    sweep('lost link', gone, SETTLE, 'not connected')
finally:
    for d in daemons:
        d.stop()
print(f'{passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
