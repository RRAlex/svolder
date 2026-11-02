# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Paper's stills (`make paper-shots`): a DAC and an amp, at a desktop's and a phone's size, on light
and on dark paper, taken in Chromium once the fonts are in. Eight pictures,
`target/web-shots/paper/{dac,amp}-{desktop,phone}-{light,dark}.png`, and their paths printed. No
check: the pictures are for looking. Daemons are throwaway fakes under target/web-checks; nothing
here touches the installed binaries or ports 8737/8738. Usage: python3 paper_shots.py"""
import os
import sys
import time

import common

OUT = f'{common.REPO}/target/web-shots/paper'
SIZES = [('desktop', (1280, 1100)), ('phone', (390, 1000))]
SCHEMES = ['light', 'dark']
# the device each daemon shows: a DAC with the computer's fake player, an amp with a signal
DEVICES = [
    ('dac', dict(profile='gungnir2-v2', host='fake')),
    ('amp', dict(profile='fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})),
]
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; await document.fonts.ready; return m.core.theme.id; })"""


def shoot(kind, daemon, b, written):
    for size_name, size in SIZES:
        b.resize(*size)
        for scheme in SCHEMES:
            b.goto(f'{daemon.url}?theme=paper.{scheme}', settle=1.0)
            if b.ev(BOOT) != 'paper':
                raise RuntimeError(f'{kind}: Paper did not mount')
            time.sleep(0.8)
            path = f'{OUT}/{kind}-{size_name}-{scheme}.png'
            with open(path, 'wb') as f:
                f.write(b.shot())
            written.append(path)


os.makedirs(OUT, exist_ok=True)
written = []
b = common.Chromium(size=SIZES[0][1])
try:
    for kind, options in DEVICES:
        daemon = common.Daemon(**options)
        try:
            shoot(kind, daemon, b, written)
        finally:
            daemon.stop()
finally:
    b.close()
print('\n'.join(written))
sys.exit(0 if len(written) == 8 else 1)
