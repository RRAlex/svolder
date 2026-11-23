# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compose the README's GNOME indicator pictures from the headless shell's scale-3 shots.
Usage: python3 gnome_shots.py SHOTS_DIR OUT_DIR

For each state the shell saved `<fixture>-3x-menu.png`: the top bar with the indicator's pill lit
and the open popup below it, on the test shell's wallpaper. The picture keeps the panel strip
cropped at the indicator (nothing else of the panel) above the popup, with the wallpaper replaced
by the shell's own black panel colour, and is written as lossless WebP."""
import sys

from PIL import Image, ImageDraw

STATES = {
    "untouched": "gnome-indicator-untouched.webp",
    "dac_changed": "gnome-indicator-changed.webp",
}
PANEL_H = 96  # the panel's height at scale 3
RADIUS = 54  # the popup's corner radius at scale 3
PAD = 24  # margin around the popup, at scale 3
RIGHT = 1190  # the strip ends just right of the indicator's pill: no other panel content


def far(p, bg):
    return sum(abs(a - b) for a, b in zip(p, bg)) > 60


def compose(src, dst):
    im = Image.open(src).convert("RGB")
    bg = im.getpixel((10, im.height // 2))
    mid = im.height // 2
    xs = [x for x in range(im.width) if far(im.getpixel((x, mid)), bg)]
    ys = [y for y in range(PANEL_H + 4, im.height) if far(im.getpixel((xs[0] + 300, y)), bg)]
    left, right, top, bottom = xs[0], xs[-1] + 1, ys[0], ys[-1] + 1
    popup = im.crop((left, top, right, bottom))
    mask = Image.new("L", popup.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (1, 1, popup.width - 2, popup.height - 2), RADIUS, fill=255
    )
    width = popup.width + 2 * PAD
    out = Image.new("RGB", (width, PANEL_H + PAD + popup.height + PAD), (0, 0, 0))
    strip = im.crop((left - PAD, 0, min(RIGHT, right + PAD), PANEL_H))
    out.paste(strip, (0, 0))
    out.paste(popup, (PAD, PANEL_H + PAD), mask)
    out.save(dst, "WEBP", lossless=True, quality=100, method=6)
    print(f"gnome-shots: {dst} {out.width}x{out.height}")


def main():
    shots, outdir = sys.argv[1], sys.argv[2]
    for fixture, name in STATES.items():
        compose(f"{shots}/{fixture}-3x-menu.png", f"{outdir}/{name}")


if __name__ == "__main__":
    main()
