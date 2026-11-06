# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Night drive's stills (`make nightdrive-shots`), taken in Chromium on the fake device with the fake
capture's music: a desktop (1280 by 800) at rest, with a bubble held (its bell and its diamonds lit), with
the sheet open, with the device out of reach and with no capture, and a phone (390 by 844, touch) upright
and sideways (844 by 390). Seven pictures in `target/web-shots/nightdrive/`, their paths printed. No check:
the pictures are for looking. Daemons are throwaway fakes under target/web-checks; nothing here touches the
installed binaries or ports 8737/8738. Usage: python3 nightdrive_shots.py"""
import os
import sys
import time

import common
import nightdrive_base as base

OUT = f'{common.REPO}/target/web-shots/nightdrive'
# the HUD would fade (15 s) before a still is taken: this browser keeps it up for ten minutes
KEEP = "localStorage.setItem('svolder.theme.nightdrive.fade-after', '600')"
TUNE = '__core.theme.driver.tune()'
SCENE_BOX = "(() => { const r = document.querySelector('.nd-scene').getBoundingClientRect(); return [r.left, r.top]; })()"
# the floor needs about ten seconds of music to fill its rows
ROLL = 10.0
written = []


def start(b, d, roll=ROLL):
    base.forget(b, d)
    b.page(KEEP)
    if base.load(b, d) != 'nightdrive':
        raise RuntimeError('Night drive did not mount')
    time.sleep(roll)


def shoot(b, name):
    path = f'{OUT}/{name}.png'
    with open(path, 'wb') as f:
        f.write(b.shot())
    written.append(path)


def move(b, x, y, down=False):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y, **({'button': 'left', 'buttons': 1} if down else {})})


def desktop():
    b = common.Chromium(size=(1280, 800))
    try:
        d = common.Daemon('gungnir2-v2', host='fake')
        try:
            base.reset_bands(d, over={3: (6, 1)})
            start(b, d)
            shoot(b, 'desktop-rest')
            # a bubble held: the pointer on band 3's bubble, the button down and a little lift
            t = b.page(TUNE)
            left, top = b.page(SCENE_BOX)
            spot = next(p for p in t['bubbles'] if p['band'] == 3)
            x, y = left + spot['x'], top + spot['y']
            move(b, x, y)
            base.poll(lambda: b.page(TUNE)['m'] >= 0.95 and b.page(TUNE)['lit'] == 3, 3.0)
            b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y, 'button': 'left', 'clickCount': 1, 'buttons': 1})
            move(b, x, y - 6, down=True)
            time.sleep(1.0)
            shoot(b, 'desktop-bubble-held')
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y - 6, 'button': 'left', 'clickCount': 1})
            move(b, left + 40, top + 40)
            base.reset_bands(d, over={3: (6, 1)})
            start(b, d, roll=3.0)
            base.click_el(b, '#nd-sheet')
            base.poll(lambda: b.page("!!document.getElementById('nd-dialog')"), 3.0)
            time.sleep(1.0)
            shoot(b, 'desktop-sheet')
        finally:
            d.stop()
        d = common.Daemon('gungnir2-v2', host='fake')
        try:
            base.reset_bands(d, over={3: (6, 1)})
            start(b, d, roll=4.0)
            d.drop()
            time.sleep(2.5)
            shoot(b, 'desktop-out-of-reach')
        finally:
            d.stop()
        d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
        try:
            base.reset_bands(d, over={3: (6, 1)})
            start(b, d, roll=3.0)
            shoot(b, 'desktop-no-capture')
        finally:
            d.stop()
    finally:
        b.close()


def phone(size, name):
    b = common.Chromium(size=size, touch=True)
    try:
        d = common.Daemon('gungnir2-v2', host='fake')
        try:
            base.reset_bands(d, over={3: (6, 1)})
            start(b, d)
            shoot(b, name)
        finally:
            d.stop()
    finally:
        b.close()


os.makedirs(OUT, exist_ok=True)
desktop()
phone((390, 844), 'phone-portrait')
phone((844, 390), 'phone-landscape')
print('\n'.join(written))
sys.exit(0 if len(written) == 7 else 1)
