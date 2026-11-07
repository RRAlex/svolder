# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive on a phone, in a browser: held upright the scene is the top half and the dash a
panel over its foot (the volume large in the scene, the switch as a `DAC | PC` pill in the top left corner, `-` `MUTE` `+` and the transport on one row, the track with the input and `DEVICE`); held sideways the
scene is the whole window with the slim dash; the neon sheet comes up from the bottom or in from the
right and survives a turn; every touch target is 44 px; the menu button is pinned to the window; the
mountains tune from a touch; nothing scrolls sideways with long names; a tall window keeps the scene whole.
  portrait_puts_the_scene_above_the_dash_panel        412x915 and 360x780: the panel is two rows (under 30 %, over 77 % of the window left to the scene), the scene runs behind it, the switch pill is in the scene's top left corner, the volume is in the scene above the sun, the panel lets the road show through and keeps 4.5:1
  landscape_shows_the_scene_full_with_a_slim_dash     844x390 and 667x375: the scene is the window, the dash is one slim row (under 25 %), the switch's pill is in the top left corner
  the_menu_button_is_fixed_on_a_phone                 44 px at 8 px from the top and right, position fixed, in both turns
  the_sheet_slides_up_in_portrait_and_in_from_the_right_in_landscape   the rects, and the keyframes named
  turning_the_phone_keeps_an_open_sheet_open          open in portrait, turned: still open, the same control focused, and back
  touch_targets_are_44_px                             every button and field of the dash and the sheet, the Loudness round keys by their hit area
  touching_the_mountains_tunes_on_a_phone             the scene's top half: a touch shows the curve (a pointer hovers on Firefox), a second grabs a bubble and writes
  no_sideways_scroll_with_long_names                  a 40-character device name, track and preset at 390, 844 and 1100 px wide
  a_tall_window_keeps_the_scene_whole                 1280x1600 and 2560x1440: the dash on screen, the volume above the sun
Usage: python3 nightdrive_phone.py chromium|firefox"""
import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)

# the rects of what the layout places, [left, top, right, bottom] in CSS px
GEO = """(() => { const r = (sel) => { const e = document.querySelector(sel); if (!e) return null;
    const b = e.getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; };
  const button = document.getElementById('fx-button');
  return { w: innerWidth, h: innerHeight, scene: r('.nd-scene'), canvas: r('#nd-road'), dash: r('#nd-dash'),
    vol: r('.nd-vol svg'), down: r('#nd-vdown'), mute: r('#nd-mute'), up: r('#nd-vup'), track: r('#nd-track'),
    prev: r('#nd-tr-0'), play: r('#nd-tr-1'), next: r('#nd-tr-2'), sheet: r('#nd-sheet'), button: r('#fx-button'),
    side: r('#nd-side-dac'), computer: r('#nd-side-computer'), input: r('#nd-dash [data-act=input]'), now: r('.nd-now'),
    pos: getComputedStyle(document.getElementById('frame')).position, top: r('.nd-top'), eq: r('.nd-eq'),
    scrollW: document.documentElement.scrollWidth, scrollH: document.documentElement.scrollHeight,
    bodyW: document.body.scrollWidth,
    eqGains: (() => { const e = document.querySelector('.nd-gains'); return !!e && getComputedStyle(e).display !== 'none' && /[0-9]/.test(e.textContent); })(),
    sunTop: (() => { const c = document.getElementById('nd-sky'); const k = c.width / c.clientWidth;
      const x = Math.round(c.width / 2); const d = c.getContext('2d').getImageData(x, 0, 1, c.height).data;
      for (let i = 0; i < c.height; i++) if (d[i * 4] > 230 && d[i * 4 + 1] > 170 && d[i * 4 + 2] < 120) return i / k; return -1; })() }; })()"""
SHEET_OPEN = "!!document.getElementById('nd-dialog')"
DIALOG = """(() => { const e = document.getElementById('nd-dialog'); if (!e) return null;
  const b = e.getBoundingClientRect(); return { rect: [b.left, b.top, b.right, b.bottom],
    anim: getComputedStyle(e).animationName, w: innerWidth, h: innerHeight,
    shut: document.getElementById('nightdrive').dataset.sheet || '',
    focus: document.activeElement ? document.activeElement.id : '',
    inside: e.contains(document.activeElement) }; })()"""
SHORT = "[...document.querySelectorAll('#nd-switch .nd-k')].map((k) => getComputedStyle(k, '::before').content.replace(/\"/g, ''))"
NAMES = "[...document.querySelectorAll('#nd-switch .nd-k')].map((k) => k.getAttribute('aria-label'))"
TUNE = '__core.theme.driver.tune()'
TUNE_ON = "document.querySelector('.nd-scene').dataset.tune === 'on'"
SCENE_BOX = """(() => { const r = document.querySelector('.nd-scene').getBoundingClientRect();
  return [r.left, r.top]; })()"""
# every button and field of `selector`'s parts that can be pressed: its size, and whether a 44 px box around its
# middle still lands on it (a hit area bigger than the look)
TARGETS = """((selector) => { const out = [];
  for (const e of document.querySelectorAll(selector)) {
    if (e.closest('[hidden]') || e.disabled) continue;
    e.scrollIntoView({ block: 'nearest' });
    const r = e.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    const cx = (r.left + r.right) / 2, cy = (r.top + r.bottom) / 2;
    const hit = (x, y) => { const t = document.elementFromPoint(x, y); return t === e || e.contains(t); };
    const wide = r.width >= 43.5 || (hit(cx - 21.5, cy) && hit(cx + 21.5, cy));
    const tall = r.height >= 43.5 || (hit(cx, cy - 21.5) && hit(cx, cy + 21.5));
    out.push({ id: e.id || e.dataset.act || e.className, wide, tall, size: [Math.round(r.width), Math.round(r.height)] });
  }
  return out; })"""


def page_at(d, size, dpr=2, **kw):
    """A browser at `size`, with the page loaded and settled."""
    b = make(size=size, dpr=dpr, **kw)
    forget(b, d)
    load(b, d)
    time.sleep(1.0)
    return b


def inside(a, outer, slack=1.0):
    return a is not None and a[0] >= outer[0] - slack and a[1] >= outer[1] - slack and a[2] <= outer[2] + slack and a[3] <= outer[3] + slack


# the bound on the panel's text: each text colour against the panel's colour over the brightest thing that can be drawn
# behind it. The floor's cells run from hsl(300 100 % 55 %) to hsl(180 100 % 65 %), the last the brightest, rgb(89, 255, 255),
# drawn opaque (the cells are the bound: an ember or a shooting star can add warm light over one for a moment). Run as a
# function of the selectors it must find: one with no element on the page is `missing`, and the check fails on it.
CONTRAST = """((want) => { const dash = document.getElementById('nd-dash');
  const lin = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
  const lum = (r, g, b) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  const bg = getComputedStyle(dash).backgroundColor.match(/[\\d.]+/g).map(Number);
  const a = bg.length > 3 ? bg[3] : 1;
  const behind = [89, 255, 255];
  const under = lum(...[0, 1, 2].map((n) => bg[n] * a + behind[n] * (1 - a)));
  const missing = want.filter((sel) => ![...dash.querySelectorAll(sel)].some((e) => e.textContent.trim() !== ''));
  const texts = [...dash.querySelectorAll(want.join(', '))].filter((e) => e.textContent.trim() !== '').map((e) => {
    const col = getComputedStyle(e).color.match(/[\\d.]+/g).map(Number);
    const l = lum(col[0], col[1], col[2]);
    return { what: e.className + ' ' + e.textContent.slice(0, 12), ratio: (Math.max(l, under) + 0.05) / (Math.min(l, under) + 0.05) };
  });
  return { alpha: a, texts, worst: Math.min(...texts.map((t) => t.ratio)), count: texts.length, under, missing }; })"""


DAC_TEXTS = '[".nd-k", ".nd-title", ".nd-sub", ".nd-time"]'
AMP_TEXTS = '[".nd-k", ".nd-gauge", ".nd-gauge i"]'


def the_amp_panel_contrast():
    """An amp's page on a phone held upright (360x780; the fake Aegir 2): its power key, gauges and their labels are in
    the panel, and every text in it keeps 4.5:1 over the brightest cell that can be behind it; the check names the
    selectors it must find."""
    d = common.Daemon('aegir2', host='fake')
    try:
        b = page_at(d, (360, 780))
        try:
            seen = b.page(f'({CONTRAST})({AMP_TEXTS})')
            check('the_amp_panel_text_keeps_contrast: the power key and the gauges with their labels are in the panel',
                  not seen['missing'], seen['missing'])
            check('  every text in it keeps 4.5:1 over the brightest cell that can be behind it',
                  seen['alpha'] < 0.85 and seen['worst'] >= 4.5, [seen['alpha'], round(seen['worst'], 2), seen['texts']])
        finally:
            b.close()
    finally:
        d.stop()


def the_portrait_panel(d):
    """412x915 and 360x780: the panel is two rows (under 30 % of the window, so over 70 % is the scene) and the scene
    runs behind it; the switch is a pill (`DAC | PC`) in the scene's top left corner,
    36 px tall with a 44 px hit area; the volume is in the scene under the EQ line, above the sun; in the panel `-` `MUTE`
    `+` and the transport are one row, then the track with the input and `DEVICE`; the panel is translucent and its text
    keeps 4.5:1 over the brightest of what is behind it; no scroll."""
    reset_bands(d)
    for size in [(412, 915), (360, 780)]:
        b = page_at(d, size)
        try:
            g = b.page(GEO)
            w, h = g['w'], g['h']
            panel = g['dash']
            share = (panel[3] - panel[1]) / h
            print(f'  measured {w}x{h}: panel {panel[3] - panel[1]:.0f} px = {100 * share:.1f} %, the scene above it {100 * panel[1] / h:.1f} %')
            check(f'portrait_puts_the_scene_above_the_dash_panel: {w}x{h}: the panel is its own rows, under 30 % of the window, '
                  f'to the bottom and edge to edge',
                  share < 0.30 and panel[3] >= h - 1 and panel[0] <= 1 and panel[2] >= w - 1, [panel, h])
            check('  the scene above it is over 77 % of the window',
                  panel[1] / h > 0.77, [panel, h])
            check('  the scene runs behind the panel (the window, top to bottom) and the canvas fills it',
                  all(abs(a - e) <= 1 for a, e in zip(g['scene'], [0, 0, w, h])) and g['canvas'][3] - g['canvas'][1] >= h - 1,
                  [g['scene'], g['canvas'], panel])
            check('  the volume is in the scene, centred under the device line and the EQ line, above the sun',
                  g['vol'] is not None and g['vol'][3] <= panel[1] and g['vol'][1] >= g['eq'][3] - 1
                  and abs((g['vol'][0] + g['vol'][2]) / 2 - w / 2) <= 2 and g['vol'][3] <= g['sunTop'], [g['vol'], g['eq'], g['sunTop']])
            keys = [g['down'], g['mute'], g['up']]
            sw = [g['side'], g['computer']]
            check('  the switch is one pill `DAC | PC` in the scene\'s top left corner, 8 px from the left, level with the menu button, '
                  'touching its two segments, not in the panel',
                  all(k is not None for k in sw) and abs(g['side'][0] - 8) <= 1 and abs((g['side'][1] + g['side'][3]) / 2 - (g['button'][1] + g['button'][3]) / 2) <= 1
                  and abs(g['side'][1] - g['computer'][1]) <= 1 and 0 <= g['side'][2] - g['computer'][0] <= 2 and g['computer'][3] <= panel[1]
                  and not b.page("!!document.querySelector('#nd-dash #nd-side-dac')"), [g['side'], g['computer'], g['button']])
            check('  the pill clears the menu button and the device line (the line starts right of it) and the pill stays in the window',
                  g['computer'][2] < g['top'][0] and g['top'][2] < g['button'][0] and g['computer'][2] < g['button'][0] and inside(g['computer'], [0, 0, w, h]), [g['computer'], g['top'], g['button']])
            check('  the segments read `DAC` and `PC` (an amp `AMP`), each keeps its whole name as its accessible name',
                  b.page(SHORT) == ['DAC', 'PC'] and b.page(NAMES) == [b.page("__core.model.page().device.name"), 'This computer'], [b.page(SHORT), b.page(NAMES)])
            check('  the pill\'s segments are 36 px tall (the panel\'s keys too) and every key 44 px to the hand',
                  all(35.5 <= k[3] - k[1] <= 36.5 for k in sw + keys + [g['prev'], g['play'], g['next'], g['sheet']])
                  and all(t['wide'] and t['tall'] for t in b.page(f'({TARGETS})("#nd-switch .nd-k, #nd-dash .nd-k")')),
                  b.page(f'({TARGETS})("#nd-switch .nd-k, #nd-dash .nd-k")'))
            check('  in the panel `-` `MUTE` `+` in that order and the transport on ONE row, all in the panel',
                  all(inside(k, panel) for k in keys + [g['prev'], g['play'], g['next']])
                  and g['down'][2] <= g['mute'][0] and g['mute'][2] <= g['up'][0] and g['up'][2] <= g['prev'][0]
                  and g['prev'][2] <= g['play'][0] and g['play'][2] <= g['next'][0]
                  and all(abs(k[1] - g['down'][1]) <= 1 for k in keys + [g['prev'], g['play'], g['next']]),
                  [keys, g['prev'], g['play'], g['next'], panel])
            check('  then the track, the input and `DEVICE` on the Now-playing line below, all in the panel',
                  inside(g['track'], panel) and g['down'][3] <= g['track'][1] + 1 and g['now'][1] >= g['down'][3] - 1
                  and g['input'] is not None and inside(g['input'], panel) and inside(g['sheet'], panel)
                  and g['track'][2] <= g['input'][0] and g['input'][2] <= g['sheet'][0]
                  and g['down'][3] <= g['input'][1] and g['down'][3] <= g['sheet'][1] and abs(g['input'][1] - g['sheet'][1]) <= 1,
                  [g['track'], g['input'], g['sheet'], g['now']])
            check('  the device line and the EQ line stay in the scene and the EQ line holds the gains; the page does not scroll',
                  g['top'][3] <= panel[1] and g['eq'][3] <= panel[1] and g['eqGains'] and g['scrollH'] <= h and g['scrollW'] <= w, g)
            seen = b.page(f'({CONTRAST})({DAC_TEXTS})')
            check('  the panel is translucent (under 85 % opaque) and every text in it keeps 4.5:1 over the brightest thing that can be behind it',
                  seen['alpha'] < 0.85 and not seen['missing'] and seen['worst'] >= 4.5, [seen['alpha'], round(seen['worst'], 2), seen['count'], seen['texts']])
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            b.close()


def the_portrait_volume_fades(d):
    """Upright, the volume is part of the HUD: after the idle time it fades with the device line and the EQ line,
    and the panel stays as it is."""
    b = make(size=(412, 915), dpr=2)
    try:
        load_fading(b, d)
        gone = poll(lambda: all(b.page(f'{OPACITY}("{s}")') == 0 for s in ('.nd-top', '.nd-vol', '.nd-eq')), 9.0)
        check('the_portrait_volume_fades_with_the_hud: the volume, the device line and the EQ line reach opacity 0 upright',
              gone, [b.page(f'{OPACITY}("{s}")') for s in ('.nd-top', '.nd-vol', '.nd-eq')])
        check('  and the panel stays whole', b.page(f'{OPACITY}("#nd-dash")') == 1, b.page(f'{OPACITY}("#nd-dash")'))
    finally:
        b.close()


def the_landscape_scene(d):
    """844x390 and 667x375: the scene is the whole window and the dash one slim row along the bottom (under 25 % of the
    height, one row) with every key in the window and the switch's pill in the top left corner,
    at the left, clear of the volume figure."""
    for size in [(844, 390), (667, 375)]:
        b = page_at(d, size)
        try:
            g = b.page(GEO)
            w, h = g['w'], g['h']
            dash = g['dash']
            print(f'  measured {w}x{h}: strip {dash[3] - dash[1]:.0f} px = {100 * (dash[3] - dash[1]) / h:.1f} %')
            check(f'landscape_shows_the_scene_full_with_a_slim_dash: {w}x{h}: the scene and its canvas are the whole window',
                  all(abs(a - e) <= 1 for a, e in zip(g['scene'], [0, 0, w, h])) and all(abs(a - e) <= 1 for a, e in zip(g['canvas'], [0, 0, w, h])), g)
            keys = [g['down'], g['mute'], g['up'], g['input'], g['prev'], g['play'], g['next'], g['sheet']]
            check('  the dash is one slim row along the bottom (under 25 %), every key inside the window and on the one row',
                  dash[3] <= h and (dash[3] - dash[1]) < 0.25 * h and all(inside(k, [0, 0, w, h]) for k in keys)
                  and all(abs((k[1] + k[3]) / 2 - (keys[0][1] + keys[0][3]) / 2) <= 1 for k in keys), [dash, keys])
            sw = [g['side'], g['computer']]
            check('  the switch is the same pill in the top left corner, level with the menu button, left of the device line and clear of the volume',
                  all(k is not None and inside(k, [0, 0, w, h]) for k in sw) and abs(g['side'][0] - 8) <= 1
                  and abs((g['side'][1] + g['side'][3]) / 2 - (g['button'][1] + g['button'][3]) / 2) <= 1
                  and g['side'][0] < g['computer'][0] and g['computer'][2] < g['top'][0] and g['computer'][2] <= g['vol'][0]
                  and g['top'][2] < g['button'][0] and not b.page("!!document.querySelector('#nd-dash #nd-side-dac')"), [g['top'], sw, g['vol'], g['button']])
            check('  the volume is in the scene, above the dash', g['vol'] is not None and g['vol'][3] <= dash[1], [g['vol'], dash])
            check('  the page does not scroll', g['scrollH'] <= h and g['scrollW'] <= w, g)
            # no switch shown (a device with no computer side): the device line keeps the scene's edge inset
            got = b.page("""(() => { const page = document.getElementById('nightdrive'); page.classList.remove('nd-sw-hud', 'nd-has-switch');
                const probe = document.createElement('div'); probe.style.width = 'var(--nd-edge)'; page.appendChild(probe);
                const edge = probe.getBoundingClientRect().width; probe.remove();
                return { left: parseFloat(getComputedStyle(document.querySelector('.nd-top')).left), edge }; })()""")
            check('  with no switch shown the device line keeps the scene\'s edge inset', abs(got['left'] - got['edge']) <= 0.5, got)
        finally:
            b.close()


def the_menu_button(d):
    """The frame's button is 44 px, 8 px from the top and the right of the window, position fixed, in both turns."""
    for size in [(390, 844), (844, 390)]:
        b = page_at(d, size)
        try:
            g = b.page(GEO)
            w = g['w']
            btn = g['button']
            check(f'the_menu_button_is_fixed_on_a_phone: {w}x{g["h"]}: 44 px, 8 px from the top and the right, position fixed',
                  g['pos'] == 'fixed' and [round(v) for v in btn] == [w - 52, 8, w - 8, 52], [g['pos'], btn])
        finally:
            b.close()


def the_sheet_comes_from_its_side(d):
    """Portrait: the sheet is the full width from 30 % down to the bottom and enters by `nd-sheet-up`;
    landscape: min(420 px, 60 vw) wide against the right edge, the full height, entering by `nd-sheet-side`."""
    for size, kind in [((390, 844), 'up'), ((844, 390), 'side')]:
        b = page_at(d, size)
        try:
            click_el(b, '#nd-sheet')
            poll(lambda: b.page(SHEET_OPEN), 3.0)
            time.sleep(0.6)
            got = b.page(DIALOG)
            left, top, right, bottom = got['rect']
            w, h = got['w'], got['h']
            if kind == 'up':
                ok = abs(top - 0.3 * h) <= 3 and left <= 1 and right >= w - 1 and bottom >= h - 1
            else:
                ok = abs(right - w) <= 1 and abs((right - left) - min(420, 0.6 * w)) <= 2 and top <= 1 and bottom >= h - 1
            check(f'the_sheet_slides_up_in_portrait_and_in_from_the_right_in_landscape: {w}x{h}: '
                  f'{"from 30 % down, the full width" if kind == "up" else "min(420 px, 60 vw) against the right edge"}',
                  ok, got)
            check(f'  and it entered by the `nd-sheet-{kind}` keyframes', got['anim'] == f'nd-sheet-{kind}', got['anim'])
        finally:
            b.close()


def turning_keeps_the_sheet(d):
    """The sheet open in portrait with a control focused: turned sideways it is still open, the same control still
    has the focus, and the sheet is the side one; turned back, the same."""
    b = page_at(d, (390, 844))
    try:
        click_el(b, '#nd-sheet')
        poll(lambda: b.page(SHEET_OPEN), 3.0)
        time.sleep(0.5)
        before = b.page(DIALOG)
        check('turning_the_phone_keeps_an_open_sheet_open: open in portrait with a control inside focused',
              before['inside'] and before['focus'] != '' and before['shut'] == 'open', before)
        for size, kind in [((844, 390), 'side'), ((390, 844), 'up')]:
            b.resize(*size)
            time.sleep(0.8)
            now = b.page(DIALOG)
            check(f'  turned to {size[0]}x{size[1]}: the sheet is still open, behind it still out of reach, the same control '
                  f'still focused', now is not None and now['shut'] == 'open' and now['focus'] == before['focus'] and now['inside'], [now, before['focus']])
            if now is None:
                continue
            left, top, right, bottom = now['rect']
            w, h = now['w'], now['h']
            ok = (abs(right - w) <= 1 and top <= 1) if kind == 'side' else (abs(top - 0.3 * h) <= 3 and left <= 1 and right >= w - 1)
            check(f'  and it takes the {"side" if kind == "side" else "bottom"} layout of that turn', ok, now)
    finally:
        b.close()


def the_touch_targets(d):
    """Every button and field of the dash, the sheet and the frame's button is 44 px, or a 44 px box about its middle
    lands on it (the Loudness round keys' hit area is bigger than their look), in both turns; the sheet is read
    in Loudness so those keys are on show."""
    d.post(DEVICE, 'eq/mode', {'mode': 'loudness'})
    poll(lambda: d.model(DEVICE)['tone']['mode'] == 'loudness', 6.0)
    try:
        for size in [(390, 844), (844, 390)]:
            b = page_at(d, size)
            try:
                dash = b.page(f"({TARGETS})('#nd-dash button, #fx-button')")
                bad = [t for t in dash if not (t['wide'] and t['tall'])]
                check(f'touch_targets_are_44_px: {size[0]}x{size[1]}: the dash\'s {len(dash)} keys and the menu button',
                      len(dash) >= 8 and not bad, bad)
                click_el(b, '#nd-sheet')
                poll(lambda: b.page(SHEET_OPEN), 3.0)
                time.sleep(0.5)
                rows = b.page(f"({TARGETS})('#nd-sheet-wrap button, #nd-sheet-wrap input')")
                bad = [t for t in rows if not (t['wide'] and t['tall'])]
                round_keys = [t for t in rows if t['id'] in ('nd-loudm', 'nd-loudp')]
                check(f'  the sheet\'s {len(rows)} buttons and fields, with the Loudness round keys among them',
                      len(rows) >= 10 and len(round_keys) == 2 and not bad, [bad, round_keys])
                small = [t for t in round_keys if min(t['size']) < 43]
                check('  the round keys\' look is smaller than their hit area (the area is fixed, not the look)',
                      len(small) == 2, round_keys)
            finally:
                b.close()
    finally:
        d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
        poll(lambda: d.model(DEVICE)['tone']['mode'] == 'eq', 6.0)


def touch_send(b, kind, points):
    b.send('Input.dispatchTouchEvent', {'type': kind, 'touchPoints': [{'x': x, 'y': y, 'id': i} for i, x, y in points]})


def the_mountains_on_a_phone(d):
    """A phone held upright: the mountains are in the scene's top half. Chromium, touch: a first touch there shows
    the curve and grabs nothing, a second grabs a bubble and a move writes that band; a touch in the panel shows no
    curve. Firefox: the pointer over the mountains shows the curve, over the panel it does not."""
    reset_bands(d)
    if browser == 'chromium':
        b = common.Chromium(size=(390, 844), dpr=2, touch=True)
    else:
        b = make(size=(390, 844), dpr=2)
    try:
        forget(b, d)
        load(b, d)
        time.sleep(0.8)
        b.page(REC)
        left, top = b.page(SCENE_BOX)
        t = b.page(TUNE)
        spot = next(p for p in t['bubbles'] if p['band'] == 3)
        sx, sy = left + spot['x'], top + spot['y']
        check('touching_the_mountains_tunes_on_a_phone: the mountains lie in the top half of the window',
              sy < 844 / 2 and top + t['H'] < 844 * 0.52, [sy, top, t['H']])
        if browser == 'chromium':
            touch_send(b, 'touchStart', [(1, 200, 700)])
            touch_send(b, 'touchEnd', [])
            time.sleep(0.4)
            check('  a touch in the panel shows no curve', b.page(TUNE_ON) is False and b.page(TUNE)['held'] is None, b.page(TUNE))
            touch_send(b, 'touchStart', [(1, sx, sy)])
            touch_send(b, 'touchEnd', [])
            shown = poll(lambda: b.page(TUNE_ON) is True, 2.0)
            first = b.page(TUNE)
            check('  a first touch on the mountains shows the curve and grabs nothing',
                  shown and first['held'] is None and band_posts(b) == [], [shown, first['held'], band_posts(b)])
            poll(lambda: b.page(TUNE)['m'] >= 0.6, 2.0)
            touch_send(b, 'touchStart', [(1, sx, sy)])
            time.sleep(0.2)
            second = b.page(TUNE)
            for k in range(1, 7):
                touch_send(b, 'touchMove', [(1, sx, sy + 6 * k)])
                time.sleep(0.03)
            poll(lambda: band_posts(b, DEVICE), 4.0)
            posts = band_posts(b, DEVICE)
            touch_send(b, 'touchEnd', [])
            check('  a second touch grabs the bubble, and a move down writes that band of this device',
                  second['held'] == 3 and posts != [] and all(p[0].endswith('/eq/bands/3') for p in posts), [second['held'], posts[-2:]])
        else:
            mouse_to(b, sx, sy + 0.04 * t['h'])
            shown = poll(lambda: b.page(TUNE_ON) is True, 3.0)
            check('  the pointer over the mountains shows the curve', shown, b.page(TUNE))
            mouse_to(b, 200, 700)
            gone = poll(lambda: b.page(TUNE_ON) is False, 3.0)
            check('  the pointer over the panel lets it go', gone, b.page(TUNE))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
        reset_bands(d)


def mouse_to(b, x, y):
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
    else:
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'},
              'actions': [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)}]}]})


# a long device name with no break, a long track, a long preset
LONG_NAME = 'NightDriveWithAVeryLongDeviceName0123456'
LONG_TRACK = 'An Extraordinarily Long Track Title That Goes On'
LONG_PRESET = 'a preset named after a very long phrase'


def the_long_names():
    """A 40-character device name with no break, a long track and a long preset name: no sideways scroll and
    nothing of the dash or the sheet past the window's edge at 390, 844 (held sideways) and 1100 px wide."""
    d = common.Daemon('gungnir2-v2', host='fake', name=LONG_NAME, env={'SVOLDER_FAKE_TITLE': LONG_TRACK})
    try:
        d.post(LONG_NAME, 'presets', {'name': LONG_PRESET})
        poll(lambda: any(r['name'] == LONG_PRESET for r in d.model(LONG_NAME)['presets']['rows']), 6.0)
        for size in [(390, 844), (844, 390), (1100, 800)]:
            b = make(size=size, dpr=2)
            try:
                b.goto(f'{d.url}?theme=nightdrive', settle=1.0)
                b.page(BOOT)
                time.sleep(1.0)
                g = b.page(GEO)
                w = g['w']
                seen = b.page("""(() => ({ name: document.querySelector('.nd-left').textContent,
                  track: document.querySelector('.nd-title') && document.querySelector('.nd-title').textContent }))()""")
                out = b.page(OVERFLOW % "'.nd-hud, #nd-dash'")
                check(f'no_sideways_scroll_with_long_names: {w}px, the name {len(LONG_NAME)} characters, the track and the preset long: '
                      f'the page is no wider than the window and no part of the HUD or the dash is past its edge',
                      LONG_NAME in seen['name'] and seen['track'] == LONG_TRACK and g['scrollW'] <= w and g['bodyW'] <= w and out == [],
                      [seen, g['scrollW'], out])
                click_el(b, '#nd-sheet')
                poll(lambda: b.page(SHEET_OPEN), 3.0)
                time.sleep(0.5)
                names = b.page("[...document.querySelectorAll('#nd-presets [data-act=preset]')].map((c) => c.dataset.name)")
                out = b.page(OVERFLOW % "'#nd-dialog'")
                body = b.page("""(() => { const e = document.getElementById('nd-sheet-body'); return [e.scrollWidth, e.clientWidth]; })()""")
                check('  with the sheet open: the preset is on show, the sheet\'s body scrolls only down, nothing is past the edge',
                      LONG_PRESET in names and body[0] <= body[1] and out == [] and b.page(GEO)['scrollW'] <= w, [names, body, out])
                check('  no page error', errors(b) == [], errors(b)[:3])
            finally:
                b.close()
    finally:
        d.stop()


# the visible parts of the elements `%s` (a selector list) that reach past the window's left or right edge
OVERFLOW = """(() => { const out = [];
  for (const root of document.querySelectorAll(%s)) {
    for (const e of [root, ...root.querySelectorAll('*')]) {
      if (e.closest('[hidden]') || e.closest('svg')) continue;
      const r = e.getBoundingClientRect();
      if (r.width && (r.left < -1 || r.right > innerWidth + 1)) out.push([e.className || e.tagName, Math.round(r.left), Math.round(r.right)]);
    }
  }
  return out.slice(0, 6); })()"""


SUN = """(() => { const c = document.getElementById('nd-road'); const h = c.clientHeight, w = c.clientWidth;
  const H = h * 0.56, cy = H - h * 0.04, r = h * 0.22;
  return { w, h, sun: [w / 2 - r, cy - r, w / 2 + r, Math.min(cy + r, H)], size: [innerWidth, innerHeight] }; })()"""


def the_tall_window(d):
    """1280x1600 and 2560x1440: the scene is the window, the dash is on screen along its bottom, the volume is above
    the sun and the sun is whole on the scene; the page does not scroll."""
    for size in [(1280, 1600), (2560, 1440)]:
        b = page_at(d, size, dpr=1)
        try:
            g = b.page(GEO)
            s = b.page(SUN)
            w, h = g['w'], g['h']
            check(f'a_tall_window_keeps_the_scene_whole: {w}x{h}: the scene is the window, the sun whole on it',
                  s['size'] == [w, h] and s['w'] == w and s['h'] == h and s['sun'][1] >= 0, [s, g['scene']])
            check('  the dash is on screen along the bottom, every key inside the window',
                  inside(g['dash'], [0, 0, w, h]) and g['dash'][3] >= h - 40
                  and all(inside(g[k], [0, 0, w, h]) for k in ('down', 'mute', 'up', 'prev', 'play', 'next', 'sheet')), g['dash'])
            check('  the volume is above the sun, and under the top of the window',
                  g['vol'] is not None and g['vol'][1] >= 0 and g['vol'][3] <= s['sun'][1] + 1, [g['vol'], s['sun']])
            check('  the page does not scroll', g['scrollH'] <= h and g['scrollW'] <= w, g)
        finally:
            b.close()


d = common.Daemon('gungnir2-v2', host='fake')
try:
    the_portrait_panel(d)
    the_amp_panel_contrast()
    the_portrait_volume_fades(d)
    the_landscape_scene(d)
    the_menu_button(d)
    the_sheet_comes_from_its_side(d)
    turning_keeps_the_sheet(d)
    the_touch_targets(d)
    the_mountains_on_a_phone(d)
    the_tall_window(d)
finally:
    d.stop()
the_long_names()
base.finish('nightdrive_phone.py')
