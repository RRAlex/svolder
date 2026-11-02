# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Glow level's own densest pixel (a Glow-level screenshot, allowed) as an equivalent alpha of
--glow-1 over --bg, to tune the smoke's densest cloud against. Usage: python3 glow_level.py"""
import io, sys
import numpy as np
from PIL import Image
import common

d = common.Daemon('gungnir2-v2')
b = common.Chromium(size=(1280, 900))
try:
    b.goto(d.url + '?effects=off')
    bg = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB')).astype(float)
    bgc = bg[5, 5]
    b.goto(d.url + '?effects=glow')
    b.sleep(1.0)
    b.ev("document.querySelector('#screen').style.setProperty('visibility', 'hidden') || 0")
    b.ev("document.querySelector('.top').style.setProperty('visibility', 'hidden') || 0")
    b.sleep(0.3)
    lvl = b.ev("getComputedStyle(document.querySelector('.fx')).getPropertyValue('--lvl')")
    img = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB')).astype(float)
    g1 = np.array([0x8b, 0x5c, 0xf6], float)
    k = b.ev("document.querySelector('sv-knob').geometry().cx"), b.ev(
        "document.querySelector('sv-knob').geometry().cy",
    )
    # only pixels where the "off" screenshot equals the background (no content): the glow alone
    clean = np.abs(bg - bgc).max(axis=2) <= 1
    a = ((img - bgc) / (g1 - bgc)).max(axis=2)  # equivalent opacity of glow-1, over the bg
    a = np.where(clean, a, 0)
    pk = a.max()
    area = (a >= 0.1 * pk).sum()
    print(f'glow 10%-of-peak region: equivalent radius sqrt(area/pi) = '
          f'{np.sqrt(area / np.pi):.0f} px, '
          f'bounding box {np.ptp(np.where((a >= 0.1 * pk).any(axis=0))[0])} x '
          f'{np.ptp(np.where((a >= 0.1 * pk).any(axis=1))[0])} px')
    y, x = np.unravel_index(a.argmax(), a.shape)
    patch = a[y - 12:y + 12, x - 12:x + 12][clean[y - 12:y + 12, x - 12:x + 12]]
    print(f'glow core: alpha spread over a 24x24 patch at the peak {patch.max() - patch.min():.3f} '
          f'(of peak {pk:.3f})')
    print(f'level {lvl.strip()}  bg {bgc}  densest equivalent alpha {a.max():.3f} '
          f'(= {a.max()*255:.0f}/255) at {np.unravel_index(a.argmax(), a.shape)[::-1]}; dial {k}')
finally:
    b.close(); d.stop()
