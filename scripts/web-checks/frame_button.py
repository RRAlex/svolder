# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The frame's menu button is the same in every theme: 44 by 44 px, 8 px from the window's top edge
and 8 px from its right edge, at every width, and the theme's header keeps that corner clear. Every
theme the page lists (read from the core's registry, as `contract.py` does; Night drive too) is
mounted on the same fake daemon at 1280 by 800, 768 by 1024, 412 by 915 and 360 by 780, once for a
signed-in viewer and once for a guest (Ember's Sign in stands left of the button); the button's box
must equal 44 by 44 at 8 and 8 within a pixel, and nothing the theme draws (a word, a control, an
image; not the scene or a layer hidden from the page's reader) may overlap it.
  the_menu_button_is_the_same_in_every_theme
  nothing_stands_under_the_menu_button       at the top of the page, also with a long device name
                                             (scrolled content passes under the opaque button, by design)
  the_button_stays_pinned_over_scrolling_content   the same rectangle after a scroll, an opaque backdrop
Usage: python3 frame_button.py chromium|firefox"""
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()
SIZES = [('1280x800', (1280, 800)), ('768x1024', (768, 1024)), ('412x915', (412, 915)), ('360x780', (360, 780))]
WANT = {'size': 44, 'right': 8, 'top': 8}
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
BOX = """(() => { const r = document.querySelector('#frame .fxbtn').getBoundingClientRect();
  const w = document.documentElement.clientWidth;
  return { fromRight: Math.round((w - r.right) * 10) / 10, top: Math.round(r.top * 10) / 10,
    width: r.width, height: r.height }; })()"""

UNDER = """(() => { const btn = document.querySelector('#frame .fxbtn').getBoundingClientRect();
  const hidden = (e) => !!e.closest('[aria-hidden=true], canvas, [hidden]');
  const own = (e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
  const out = [];
  for (const e of document.querySelectorAll('#theme *')) {
    if (hidden(e) || e.matches('.fxslot, .paper-slot, .sg-slot, .nd-slot, #nd-slot, .nd-scene')) continue;
    if (!(own(e) || e.matches('button, a, input, select, textarea, img, [role=img], [role=slider]'))) continue;
    const cs = getComputedStyle(e);
    if (cs.display === 'none' || cs.visibility === 'hidden' || Number(cs.opacity) === 0) continue;
    const r = e.getBoundingClientRect();
    if (r.width < 3 || r.height < 3) continue;
    if (r.left < btn.right - 0.5 && r.right > btn.left + 0.5 && r.top < btn.bottom - 0.5 && r.bottom > btn.top + 0.5) {
      out.push((e.id ? '#' + e.id : e.tagName.toLowerCase()) + '.' + String(e.getAttribute('class') || '').split(' ')[0]);
    }
  }
  return out; })()"""
LISTED = "__core.registry.list().map((t) => t.id)"


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(size):
    return common.Chromium(size=size) if browser == 'chromium' else common.Firefox(size=size)


def poll(read, secs=5.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got




SCROLL = """(() => { const room = document.documentElement.scrollHeight - innerHeight;
  window.scrollTo(0, Math.min(300, Math.max(room, 0))); return { y: scrollY, room: Math.max(room, 0) }; })()"""
BACKDROP = "getComputedStyle(document.querySelector('#frame .fxbtn')).backgroundColor"


def solid(colour):
    """Whether a computed colour is opaque."""
    if colour.startswith('rgba'):
        return float(colour[colour.index('(') + 1:colour.index(')')].split(',')[3]) >= 0.999
    return colour.startswith('rgb(')


def run(who, daemon):
    themes = None
    for label, size in SIZES:
        b = make(size)
        try:
            read = {}
            for theme in themes or ['ember']:
                b.goto(f'{daemon.url}assets/core/frame.css', settle=0.2)
                b.page("(() => { try { localStorage.clear(); } catch (e) {} "
                       "document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()")
                b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
                mounted = b.page(BOOT)
                if themes is None:
                    themes = b.page(LISTED)
                time.sleep(0.6)
                if mounted != theme:
                    read[theme] = {'box': mounted}
                    continue
                at = {'box': b.page(BOX), 'under': b.page(UNDER)}
                y = b.page(SCROLL)
                time.sleep(0.4)
                at.update(y=y['y'], room=y['room'], box2=b.page(BOX), bg=b.page(BACKDROP))
                read[theme] = at
            # the other themes, once the first is known
            for theme in themes:
                if theme in read:
                    continue
                b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
                mounted = b.page(BOOT)
                time.sleep(0.6)
                if mounted != theme:
                    read[theme] = {'box': mounted}
                    continue
                at = {'box': b.page(BOX), 'under': b.page(UNDER)}
                y = b.page(SCROLL)
                time.sleep(0.4)
                at.update(y=y['y'], room=y['room'], box2=b.page(BOX), bg=b.page(BACKDROP))
                read[theme] = at
            print(who, label, {t: r.get('box') for t, r in read.items()})
            for theme in themes:
                at = read[theme]
                box = at['box']
                check(f'the_menu_button_is_the_same_in_every_theme: {theme} for {who} at {label} is 44 by 44, '
                      f'8 px from the top and 8 px from the right: {box}',
                      isinstance(box, dict) and abs(box['width'] - WANT['size']) <= 1
                      and abs(box['height'] - WANT['size']) <= 1 and abs(box['fromRight'] - WANT['right']) <= 1
                      and abs(box['top'] - WANT['top']) <= 1, [theme, box])
                check(f'nothing_stands_under_the_menu_button: {theme} for {who} at {label}',
                      at.get('under') == [], [theme, at.get('under')])
                if 'y' not in at:
                    continue
                # a theme whose page can scroll must have scrolled, or the rectangle was not tried against a scroll
                moved = at['room'] == 0 or abs(at['y'] - min(300, at['room'])) <= 1
                check(f'the_button_stays_pinned_over_scrolling_content: {theme} for {who} at {label} keeps its '
                      f'rectangle after a scroll to {at["y"]} px (of {at["room"]} px of room), on an opaque backdrop ({at["bg"]})',
                      moved and at['box2'] == box and solid(at['bg']), [theme, box, at['box2'], at['bg'], at['y'], at['room']])
            if label in ('412x915', '360x780'):
                scrolled = [t for t in themes if read[t].get('room', 0) > 0 and read[t].get('y', 0) > 0]
                check(f'the_scrolled_case_scrolls: at {label} the page of at least one theme scrolled for {who}, '
                      f'so the pinned rectangle was tried against scrolling content', len(scrolled) >= 1, scrolled)
        finally:
            b.close()


d = common.Daemon('gungnir2-v2', host='fake')
g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
long = common.Daemon('gungnir2-v2', host='fake', name='A device with a very long name that goes on and on')
try:
    run('a signed-in viewer', d)
    run('a guest', g)
    run('a device with a very long name', long)
finally:
    d.stop()
    g.stop()
    long.stop()
print(f'frame_button.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
