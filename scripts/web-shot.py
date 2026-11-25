#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""One picture of a page, drawn by headless Chromium over the DevTools protocol, for the shots of
`make web-shots` that must wait for what the page draws by itself: the visualizer card of a page
with the fake host. It opens the page at WIDTH x HEIGHT CSS pixels at twice the pixel density
(the README's pictures), waits until the card has drawn frames (it runs, its canvas holds pixels,
and they changed since the last look), then writes the PNG. The card's first-run hint is marked as
seen beforehand, so no chip covers its corner.

With --expect-card the page must have the card: one that is missing, or never draws within 20 s,
is a failure with no picture. Without it a page with no card is shot as it is.

With --sound the picture is the Sound popup, open on its EQ library tab (the default page) with a search
done and one measurement previewed: a fake daemon of its own (the fake host, homes under a scratch
dir) whose AutoEq is the loopback fixture of the web checks, so nothing reaches the internet. Usage
then: python3 web-shot.py --sound OUT.png WIDTH HEIGHT.

Exits 1, with no picture, when the page logs an error or a Content Security Policy report.

Needs python3-websockets (the web checks' own driver, scripts/web-checks/common.py, is used).
Usage: python3 web-shot.py [--expect-card] URL OUT.png WIDTH HEIGHT"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), 'web-checks'))
import common  # noqa: E402

# The card's own words: it runs, and its canvas has pixels (the same read visualizer.py makes).
DRAWN = """(() => {
  const card = document.querySelector('.p-viz');
  if (!card) return null;
  const c = card.querySelector('canvas');
  const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
  let non = 0; let hash = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] !== 0) non++;
    hash = (hash * 31 + d[i] + d[i + 1] * 3 + d[i + 2] * 7 + d[i + 3] * 11) | 0;
  }
  return { state: card.dataset.state, non, hash }; })()"""
HINTED = ("try { localStorage.setItem('svolder.viz.2', JSON.stringify({ hinted: true })); }"
          " catch (e) {}")


def centre(b, selector):
    return b.ev(f"(() => {{ const r = document.querySelector({json.dumps(selector)}).getBoundingClientRect();"
                " return [r.left + r.width / 2, r.top + r.height / 2]; })()")


def until(b, js, secs=15):
    """Whether `js` became true within `secs` seconds."""
    end = time.time() + secs
    while time.time() < end:
        if b.ev(js):
            return True
        b.sleep(0.2)
    return False


def sound(out, width, height):
    """The Sound popup on its EQ library tab: opened by the menu's row, a headphone searched for, the
    first measurement previewed. The page is the default theme with the glow, as the other pictures."""
    fixture = common.AutoEqFixture()
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_AUTOEQ_URL': fixture.url})
    b = common.Chromium(size=(width, height), dpr=2)
    try:
        b.preload(HINTED)
        b.goto(f'{d.url}?effects=glow', settle=1.0)
        b.click(*centre(b, '#fx-button'))
        if not until(b, "!document.getElementById('fx-menu').hidden && !!document.getElementById('menu-popup-sound')", 5):
            print('web-shot: the menu has no Sound row', file=sys.stderr)
            return 1
        b.click(*centre(b, '#menu-popup-sound'))
        if not until(b, "document.getElementById('popup-sound')?.querySelector('.fp-box')?.dataset.state === 'ready'", 8):
            print('web-shot: the Sound popup never opened', file=sys.stderr)
            return 1
        b.ev("(() => { const f = document.getElementById('popup-eq-library-query'); f.focus(); f.value = 'hd 650';"
             " f.dispatchEvent(new Event('input', { bubbles: true })); })()")
        if not until(b, "document.querySelectorAll('.fpe-group').length === 1", 10):
            print('web-shot: the search found no headphone', file=sys.stderr)
            return 1
        b.click(*centre(b, '#popup-eq-library-v-0-0'))
        if not until(b, "!!document.querySelector('#popup-eq-library-preview .fp-graph')", 15):
            print('web-shot: no preview was drawn', file=sys.stderr)
            return 1
        b.ev("(document.activeElement && document.activeElement.blur(), 1)")
        b.sleep(1.0)
        if b.errors:
            print('web-shot: the page logged errors:', *b.errors, sep='\n  ', file=sys.stderr)
            return 1
        with open(out, 'wb') as f:
            f.write(b.shot())
    finally:
        b.close()
        d.stop()
        fixture.stop()
    return 0


def main():
    if sys.argv[1:2] == ['--sound']:
        return sound(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]))
    args = [a for a in sys.argv[1:] if a != '--expect-card']
    expect = len(args) != len(sys.argv) - 1
    url, out, width, height = args[0], args[1], int(args[2]), int(args[3])
    b = common.Chromium(size=(width, height), dpr=2)
    try:
        b.preload(HINTED)
        b.goto(url, settle=1.0)
        seen = b.ev(DRAWN)
        if seen is not None or expect:
            # a page with the card: wait for frames that are on the canvas and still changing
            end = time.time() + 20
            last = None
            while time.time() < end:
                seen = b.ev(DRAWN)
                if seen is not None and seen['state'] == 'running' and seen['non'] > 500:
                    if last is not None and seen['hash'] != last:
                        break
                    last = seen['hash']
                b.sleep(0.4)
            else:
                print(f'web-shot: the visualizer never drew on {url}: {seen}', file=sys.stderr)
                return 1
            # a few more frames, so the bars stand where music puts them
            b.sleep(1.5)
        if b.errors:
            print(f'web-shot: the page logged errors on {url}:', *b.errors, sep='\n  ',
                  file=sys.stderr)
            return 1
        with open(out, 'wb') as f:
            f.write(b.shot())
    finally:
        b.close()
    return 0


sys.exit(main())
