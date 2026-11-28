# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Void Garden's stills (`make voidgarden-shots`), taken in Chromium (the stock WebGL2 tier, Full
motion) on the fake device with the fake host's music: a desktop (1280 by 800) at rest, with the
audio panel spread, with the device out of reach and with no audio capture, and a phone (390 by 844,
touch) upright and sideways (844 by 390), each on the device's side and on the computer's. Eight
pictures in `target/web-shots/voidgarden/`, their paths printed. No check: the pictures are for
looking, and the look run's fixes (and the baselines) are judged by them. Daemons are throwaway
fakes under target/web-checks; nothing here touches the installed binaries or ports 8737/8738.
Usage: python3 voidgarden_shots.py"""
import os
import sys
import time

import common
import voidgarden_base as vb

OUT = f'{common.REPO}/target/web-shots/voidgarden'
# the HUD and the dash would fade before a still is taken: this browser keeps them up for ten minutes
KEEP = ("try { localStorage.setItem('svolder.theme.voidgarden.fade-after', '600');"
        " localStorage.setItem('svolder.theme.voidgarden.motion', '\"full\"'); } catch (e) {}")
ROLL = 8.0
written = []


def start(b, d, roll=ROLL):
    vb.forget(b, d)
    b.page(KEEP)
    if vb.load(b, d) != 'voidgarden':
        raise RuntimeError('Void Garden did not mount')
    time.sleep(roll)


def shoot(b, name):
    path = f'{OUT}/{name}.png'
    with open(path, 'wb') as f:
        f.write(b.shot())
    written.append(path)


def desktop():
    b = vb.make('webgl2', size=(1280, 800))
    try:
        d = common.Daemon('gungnir2-v2', host='fake')
        try:
            start(b, d)
            shoot(b, 'desktop-rest')
            b.page(f"{vb.DRIVER}.operate('Audio panel', {{ mode: 'spread' }})")
            time.sleep(1.5)
            shoot(b, 'desktop-spread')
            start(b, d, roll=4.0)
            d.drop()
            vb.poll(lambda: b.page("document.getElementById('voidgarden').dataset.mood") == 'unreachable', 40.0)
            time.sleep(2.5)
            shoot(b, 'desktop-out-of-reach')
        finally:
            d.stop()
        d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
        try:
            start(b, d, roll=4.0)
            shoot(b, 'desktop-no-capture')
        finally:
            d.stop()
    finally:
        b.close()


def phone(size, name):
    b = vb.make('webgl2', size=size, touch=True)
    try:
        d = common.Daemon('gungnir2-v2', host='fake')
        try:
            start(b, d)
            shoot(b, name)
            b.page("__core.computer.flip('computer')")
            time.sleep(1.5)
            shoot(b, f'{name}-computer')
        finally:
            d.stop()
    finally:
        b.close()


os.makedirs(OUT, exist_ok=True)
desktop()
phone((390, 844), 'phone-portrait')
phone((844, 390), 'phone-landscape')
print('\n'.join(written))
sys.exit(0 if len(written) == 8 else 1)
