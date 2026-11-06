# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Signal's stills (`make signal-shots`), taken in Chromium: a DAC at desktop width (1280), with the
linked amp chosen, with a declared hop chosen, with its link lost, the Add device sheet open, the linked amp
shown on its own page, a tablet (768 by 1024) and the phone (390 by 844) at rest, scrolled to
the tone control and scrolled with the strip pinned. Ten pictures in
`target/web-shots/signal/`, their paths printed. No check: the pictures are for looking. Daemons are
throwaway fakes under target/web-checks; nothing here touches the installed binaries or ports
8737/8738. Usage: python3 signal_shots.py"""
import os
import sys
import time

import common

OUT = f'{common.REPO}/target/web-shots/signal'
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
ROUTE = """(() => { const w = document.querySelector('[data-hop=computer] .sg-w');
  return !!w && w.textContent !== 'route not read'; })()"""
# a declared pair of hops after the DAC: a linked amp and headphones with a note
LINKED = ('auth = "none"\n\n[[hop]]\ndevice = "Amp"\nafter = "Gungnir 2"\n\n'
          '[[hop]]\nname = "HD 650"\nkind = "headphones"\nafter = "Gungnir 2"\nnote = "on the desk"\n')
# the amp offered by the Add device sheet is one no hop links yet
UNLINKED = 'auth = "none"\n\n[[hop]]\nname = "HD 650"\nkind = "headphones"\nafter = "Gungnir 2"\nnote = "on the desk"\n'
FLEET = [('aegir2', 'Amp', '1112131415161718')]
written = []


def start(b, d, query='?theme=signal'):
    b.goto(f'{d.url}{query}', settle=1.0)
    if b.ev(BOOT) != 'signal':
        raise RuntimeError('Signal did not mount')
    for _ in range(100):  # the computer's route, which the page reads once it watches the host
        if b.page(ROUTE):
            break
        time.sleep(0.1)
    time.sleep(1.0)


def tap(b, selector):
    b.page(f"document.querySelector('{selector}').scrollIntoView({{ block: 'center' }})")
    time.sleep(0.15)
    x, y = common.center(b, selector)
    b.click(x, y)
    time.sleep(0.8)


def top(b):
    b.page('window.scrollTo(0, 0)')
    time.sleep(0.5)


def shoot(b, name):
    path = f'{OUT}/{name}.png'
    with open(path, 'wb') as f:
        f.write(b.shot())
    written.append(path)


def desktop():
    b = common.Chromium(size=(1280, 800))
    try:
        d = common.Daemon('gungnir2-v2', host='fake', web=LINKED, fleet=FLEET)
        try:
            start(b, d)
            shoot(b, 'desktop-dac')
            tap(b, '#sg-line [data-hop="device:Amp"]')
            top(b)
            shoot(b, 'desktop-amp')
            tap(b, '#sg-line [data-hop="hop:HD 650"]')
            top(b)
            shoot(b, 'desktop-declared')
            start(b, d, '?theme=signal&device=Amp')
            shoot(b, 'desktop-amp-shown')
        finally:
            d.stop()
        d = common.Daemon('gungnir2-v2', host='fake', web=UNLINKED, fleet=FLEET)
        try:
            start(b, d)
            tap(b, '.fxbtn')
            time.sleep(0.3)
            tap(b, '#menu-add-device')
            shoot(b, 'desktop-plus-sheet')
        finally:
            d.stop()
        d = common.Daemon('gungnir2-v2', host='fake', web=UNLINKED)
        try:
            d.drop()
            time.sleep(1.0)
            b.goto(f'{d.url}?theme=signal', settle=2.5)
            if b.ev(BOOT) != 'signal':
                raise RuntimeError('Signal did not mount')
            time.sleep(1.5)
            shoot(b, 'desktop-lost')
        finally:
            d.stop()
    finally:
        b.close()


def tablet():
    b = common.Chromium(size=(768, 1024))
    try:
        d = common.Daemon('gungnir2-v2', host='fake', web=LINKED, fleet=FLEET)
        try:
            start(b, d)
            shoot(b, 'tablet-dac')
        finally:
            d.stop()
    finally:
        b.close()


def phone():
    b = common.Chromium(size=(390, 844), touch=True)
    try:
        d = common.Daemon('gungnir2-v2', host='fake', web=LINKED, fleet=FLEET)
        try:
            start(b, d)
            shoot(b, 'phone-dac')
            # scrolled so that the pinned Now playing bar covers only the gap before the next section
            b.page('window.scrollTo(0, 222)')
            time.sleep(0.8)
            shoot(b, 'phone-tone')
            b.page('window.scrollTo(0, 900)')
            time.sleep(0.8)
            shoot(b, 'phone-scrolled')
        finally:
            d.stop()
    finally:
        b.close()


os.makedirs(OUT, exist_ok=True)
desktop()
tablet()
phone()
print('\n'.join(written))
sys.exit(0 if len(written) == 10 else 1)
