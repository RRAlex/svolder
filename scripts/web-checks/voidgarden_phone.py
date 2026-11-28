# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden on a phone in a browser (and 0.40.0 3.10's chrome on both): the layout's rules at phone widths (360, 390 and 768
upright, 844 sideways), in each audio panel mode and on each side of the switch, and what a finger
does. Nothing here reads a colour, a size of a part or a place on the screen that the look run may
move: a rule is that a thing is inside the window, clear of another, a thumb's size (the 24 px of
WCAG 2.2) or above the dash. The cells are `none` (both 3D APIs off: the layout alone) and `webgl2`
(the stock browser with Still kept at 390 upright: the canvas is the window's, the page does not
scroll with a live scene). Chromium is a phone (touch and a coarse pointer); Firefox has no touch
in WebDriver, so it runs the layout rules at the widths and the finger's cases are Chromium's (the
node harness voidgarden-dash.mjs holds the first-tap rule for both).
  nothing_scrolls_sideways_and_the_dash_stands_in_the_window   the page is not wider than the window; the dash and its keys are inside it
  every_key_is_a_thumbs_size              every dash key that shows is 24 px or more each way
  the_menu_button_is_clear                the button is the thing at the slot, and no HUD line or panel part overlaps the slot
  the_modules_stand_above_the_dash        a HUD module is inside the window and above the dash; `--vg-dash-h` is the dash's height with its offset
  the_panel_stays_in_the_window           every part of the panel is inside the window across its width
  the_computer_side_keeps_the_rules       the same rules after the switch to the computer
  the_chrome_fades_to_nothing_and_a_tap_only_wakes  the capsules, the chip, the menu button and the HUD are gone 3 s (4 s on a phone) after the last input; a tap only wakes
  the_features_panel_opens_at_the_top_left_of_the_menu_button   (0.40.0 3.10) the chip opens the panel at the top, 9 s of history, the whole foot; a tap closes it
  the_phone_has_a_chip_and_one_capsule, the_sheet_opens_from_the_dots   the phone's state 1 and 3: 44 px targets, four lines, the whole title, a swipe or a tap above closes it
  the_desktop_has_two_capsules_and_tooltips, the_panel_stays_while_pinned   the corners, two-line tooltips on hover and focus, the pinned panel
  a_tap_opens_the_menu                    Chromium: a tap on the menu button opens the menu; a tap on a row takes it
  a_phone_starts_capped                   Chromium, `webgl2`: a coarse pointer begins capped at density 1
  no_console_error_warning_or_csp         every page: none
(0.40.0 3.13) the_explore_labels_are_one_line_with_a_fast_forward_glyph (360-412 wide, nothing cut, 44 px), the_phone_has_no_top_line,
a_tap_leaves_no_selected_look, the_features_chip_fades_with_the_menu_button_and_the_capsule, the_top_line_fades_with_the_chrome_on_a_desktop;
and the 0.40.0 phone layout (`none` cell): the_panel_is_compact_and_at_the_top, the_keys_are_a_finger_wide (44 px of hit
area), the_dash_is_one_row_with_an_overflow, the_text_reads_over_a_bright_scene (4.5:1 over white), most_of_the_scene_stays_clear,
the_hud_shows_one_module_at_a_time, a_narrow_mouse_window_shows_a_scrollbar, a_wide_window_keeps_its_layout,
the_phone_media_query_is_the_javascripts (style.css vs hud.js PHONE_QUERY), the_host_scroller_is_reached_by_tab
(and node/voidgarden-hud.mjs a_phones_panel_wire_passes_compact). A key past the window's edge passes inside a sideways scroller
that brings it in (the bar scrolled to its end and measured again).
Usage: python3 voidgarden_phone.py chromium|firefox [none,webgl2]"""
import os
import re
import sys
import time

import common
import voidgarden_base as vb
from voidgarden_base import check, load, make, poll

CELLS = (sys.argv[2] if len(sys.argv) > 2 else 'none,webgl2').split(',')
SIZES = [(360, 740), (390, 844), (768, 1024), (844, 390)]
TARGET_MIN = 24
WEB = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'bins', 'svolderd', 'web', 'themes', 'voidgarden'))
SLACK = 1.5
MODES = ['panel', 'spread', 'off']

MEASURE = """(() => {
  const box = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; };
  const shown = (e) => { const s = getComputedStyle(e); const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none' && +s.opacity > 0.05
      && !e.closest('[hidden]'); };
  const slot = document.getElementById('vg-slot');
  const at = slot.getBoundingClientRect();
  const hit = document.elementFromPoint(at.left + at.width / 2, at.top + at.height / 2);
  const button = document.getElementById('fx-button');
  const dash = document.getElementById('vg-dash');
  return {
    iw: innerWidth, ih: innerHeight, sw: document.documentElement.scrollWidth,
    slot: box(slot), onSlot: !!hit && (hit === button || button.contains(hit)),
    dash: box(dash),
    keys: [...dash.querySelectorAll('button')].filter(shown).map((e, i) => { e.dataset.vgk = i; return [`${i}:${e.id || e.dataset.act || ''}`, ...box(e)]; }),
    dashH: parseFloat(getComputedStyle(document.getElementById('voidgarden')).getPropertyValue('--vg-dash-h')),
    mods: [...document.querySelectorAll('#vg-hud .vg-h-mod[data-mod]')].filter(shown).map((e) => [e.dataset.mod, ...box(e)]),
    top: [...document.querySelectorAll('#vg-hud .vg-h-top')].filter(shown).map((e) => box(e)),
    panel: [...document.querySelectorAll('#vg-panel .vg-h-feat, #vg-panel .vg-h-pn, #vg-panel .vg-h-foot')].filter(shown).map((e) => box(e)),
  }; })()"""


def inside(r, w, h):
    return r[0] >= -SLACK and r[1] >= -SLACK and r[2] <= w + SLACK and r[3] <= h + SLACK


def meets(a, b):
    return a[0] < b[2] - SLACK and b[0] < a[2] - SLACK and a[1] < b[3] - SLACK and b[1] < a[3] - SLACK


def fresh(cell, d, size, **kw):
    b = make(cell, size=size, still=cell != 'none', touch=vb.engine == 'chromium', **kw)
    vb.forget(b, d)
    # the chrome fades to nothing after a few seconds: the layout rules measure it awake
    b.page("localStorage.setItem('svolder.theme.voidgarden.fade-after', '600')")
    return b


def set_mode(b, mode):
    b.page(f"{vb.DRIVER}.operate('Audio panel', {{ mode: '{mode}' }})")
    time.sleep(0.4)


def state_tag(cell, size, mode, side):
    return f'{cell} {size[0]}x{size[1]} {mode} {side}'


# ---- the measures ----
COLOURS = """const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
  const rgba = (s) => { const m = s.match(/[\\d.e-]+/g).map(Number); const k = s.startsWith('color(') ? 255 : 1;
    return [m[0] * k, m[1] * k, m[2] * k, m.length > 3 ? m[3] : 1]; };
  const white = [255, 255, 255];
  const overWhite = (c) => [0, 1, 2].map((i) => c[3] * c[i] + (1 - c[3]) * white[i]);
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };"""


# ---- 0.40.0 task 3.10: the chrome as approved ----
# what the chrome shows now: every visible button of the dash with its box, the capsules, the chip, the sheet
CHROME = """(() => { %s
  const box = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; };
  const vis = (e) => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden'; };
  const dash = document.getElementById('vg-dash');
  const sheet = dash.querySelector('.vg-sheet');
  const cap = dash.querySelector('.vg-cap');
  const under = overWhite(rgba(getComputedStyle(cap).backgroundColor));
  const col = (e) => (e ? ratio(rgba(getComputedStyle(e).color), under) : null);
  const tip = dash.querySelector('.vg-tip b');
  const tipBg = dash.querySelector('.vg-tip') ? overWhite(rgba(getComputedStyle(dash.querySelector('.vg-tip')).backgroundColor)) : null;
  const title = dash.querySelector('.vg-title');
  return { vw: innerWidth, vh: innerHeight, sw: document.documentElement.scrollWidth,
    caps: [...dash.querySelectorAll('.vg-cap')].filter(vis).map(box),
    buttons: [...dash.querySelectorAll('button')].filter(vis).map((e) => ({ id: e.dataset.act, box: box(e),
      w: e.getBoundingClientRect().width, h: e.getBoundingClientRect().height })),
    chip: dash.querySelector('#vg-fchip') && vis(dash.querySelector('#vg-fchip')),
    sheet: sheet && vis(sheet) ? box(sheet) : null,
    rows: sheet ? [...sheet.querySelectorAll('.vg-sh-row')].map((r) => ({ top: Math.round(r.getBoundingClientRect().top),
      kids: [...r.querySelectorAll('button')].map((k) => Math.round(k.getBoundingClientRect().top)) })) : [],
    title: title && { white: getComputedStyle(title).whiteSpace, ellipsis: getComputedStyle(title).textOverflow === 'ellipsis' },
    ink: col(dash.querySelector('.vg-vol')), sub: col(dash.querySelector('.vg-sub')), titleInk: col(title),
    chipOn: col(dash.querySelector('.vg-chip.on')), tip: tip && tipBg ? ratio(rgba(getComputedStyle(tip).color), tipBg) : null,
    slot: box(document.getElementById('vg-slot')),
    dashH: parseFloat(getComputedStyle(document.getElementById('voidgarden')).getPropertyValue('--vg-dash-h')) }; })()""" % COLOURS


PANEL_NOW = """(() => { %s
  const pn = document.querySelector('#vg-panel .vg-h-pn'); const r = pn.getBoundingClientRect();
  const s = getComputedStyle(pn);
  const bg = rgba(s.backgroundColor); const under = overWhite(bg);
  const fig = pn.querySelector('.vg-h-v');
  const shown = r.width > 0 && r.height > 0 && s.display !== 'none';
  return { vw: innerWidth, vh: innerHeight, shown, box: [r.left, r.top, r.right, r.bottom], n: pn.querySelectorAll('.vg-h-feat').length,
    title: pn.querySelector('.vg-h-title').textContent, axis: [...pn.querySelectorAll('.vg-h-axbox span')].map((e) => e.textContent),
    foot: pn.querySelector('.vg-h-foot').textContent.replace(/\\s+/g, ' ').trim(), figures: ratio(rgba(getComputedStyle(fig).color), under),
    outline: s.borderTopColor, cols: new Set([...pn.querySelectorAll('.vg-h-feat')].map((f) => Math.round(f.getBoundingClientRect().left))).size }; })()""" % COLOURS

FINGER = 44
CENTRE = 40          # the hero's centre: a square this far each way around the window's middle


def centre(w, h):
    return [w / 2 - CENTRE, h / 2 - CENTRE, w / 2 + CENTRE, h / 2 + CENTRE]


def area(r):
    return (r[2] - r[0]) * (r[3] - r[1])


def feat(b, on):
    """Open or close the features panel the way the chip does (the page's own event)."""
    b.page(f"window.__vgSteps.dispatchEvent(new CustomEvent('feat', {{ detail: {{ open: {str(on).lower()} }} }}))")
    time.sleep(0.5)


def touch_at(b, x, y, move_to=None):
    if vb.engine != 'chromium':
        # Firefox has no touch in WebDriver: a click is the tap, and there is no swipe
        if not move_to:
            b.click(x, y)
        time.sleep(0.5)
        return
    b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': [{'x': x, 'y': y, 'id': 1}]})
    if move_to:
        for step in (0.4, 0.7, 1.0):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': [
                {'x': x + (move_to[0] - x) * step, 'y': y + (move_to[1] - y) * step, 'id': 1}]})
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
    time.sleep(0.5)


def the_features_panel_opens_at_the_top_left_of_the_menu_button(b, tag, landscape=False):
    closed = b.page(PANEL_NOW)
    check(f'the_features_panel_opens_at_the_top_left_of_the_menu_button: [{tag}] closed at rest: the panel takes no room',
          not closed['shown'], closed)
    chip = b.page("document.getElementById('vg-fchip')?.getAttribute('aria-expanded')")
    feat(b, True)
    p = b.page(PANEL_NOW)
    m = b.page(CHROME)
    w, h = p['vw'], p['vh']
    check(f'  [{tag}] opened: at the very top, left of the menu button, in the window, over the top third (more sideways)',
          p['shown'] and inside(p['box'], w, h) and p['box'][1] <= 16 + SLACK and p['box'][2] <= m['slot'][0] + SLACK
          and p['box'][3] <= (0.9 if landscape else 1 / 3) * h and not meets(p['box'], centre(w, h)), [p['box'], m['slot'], h])
    check(f'  [{tag}] its history is 9 s: title, -6S/-3S/NOW and the foot keeps the lamps, the rate, the seed and Q',
          '9 s' in p['title'] and p['axis'] == ['-6S', '-3S', 'NOW'] and 'OVER' in p['foot'].upper() and 'LINK' in p['foot'].upper()
          and 'SEED' in p['foot'] and ' Q ' in p['foot'], p)
    check(f'  [{tag}] ten features in one column, figures hold 4.5:1 over a white sky',
          p['n'] == 10 and p['cols'] == 1 and p['figures'] >= 4.5, p)
    check(f'  [{tag}] the chip says it is open (aria-expanded {chip} then true) and is out of the way',
          chip == 'false' and b.page("document.getElementById('vg-fchip').getAttribute('aria-expanded')") == 'true'
          and not b.page(CHROME)['chip'], chip)
    x, y = common.center(b, '#vg-panel .vg-h-pn')
    touch_at(b, x, y)
    check(f'  [{tag}] a tap on the panel closes it', not b.page(PANEL_NOW)['shown'], b.page(PANEL_NOW))
    feat(b, True)
    feat(b, False)
    check(f'  [{tag}] and the chip (the page\'s own event) closes it too', not b.page(PANEL_NOW)['shown'])
    return p


def the_phone_has_a_chip_and_one_capsule(b, tag):
    m = b.page(CHROME)
    w, h = m['vw'], m['vh']
    ids = [k['id'] for k in m['buttons']]
    check(f'the_phone_has_a_chip_and_one_capsule: [{tag}] the features chip and one capsule: volume −/+, play, ⋯ (nothing else shows)',
          m['chip'] and len(m['caps']) == 1 and ids == ['features', 'volume-down', 'volume-up', 'play', 'more'] and m['sheet'] is None, [ids, m['caps']])
    check(f'  [{tag}] the page does not scroll sideways, the capsule is in the window and clear of the centre',
          m['sw'] <= w + 1 and inside(m['caps'][0], w, h) and not meets(m['caps'][0], centre(w, h)), [m['sw'], m['caps']])
    small = [k for k in m['buttons'] if k['w'] < FINGER - 0.5 or k['h'] < FINGER - 0.5]
    check(f'  [{tag}] every touch target is {FINGER} px each way', not small, small[:3])
    check(f'  [{tag}] the capsule\'s text and the chip hold 4.5:1 over a white sky',
          min(m['ink'], m['chipOn']) >= 4.5, [m['ink'], m['chipOn']])
    return m


def the_sheet_opens_from_the_dots(b, tag):
    x, y = common.center(b, '#vg-more')
    touch_at(b, x, y)
    m = b.page(CHROME)
    w, h = m['vw'], m['vh']
    check(f'the_sheet_opens_from_the_dots: [{tag}] ⋯ opens a bottom sheet with four lines in the window',
          m['sheet'] is not None and inside(m['sheet'], w, h) and len(m['rows']) == 4 and m['sheet'][3] > h * 0.5, [m['sheet'], len(m['rows'])])
    one_line = [len(set(r['kids'])) <= 1 for r in m['rows'] if r['kids']]
    check(f'  [{tag}] the device, the computer and the input are on one line; the three explore actions on one line',
          len(m['rows'][0]['kids']) >= 2 and len(set(m['rows'][0]['kids'])) == 1 and len(m['rows'][3]['kids']) == 3
          and len(set(m['rows'][3]['kids'])) == 1, m['rows'])
    check(f'  [{tag}] the whole title wraps and is never cut',
          m['title']['white'] == 'normal' and not m['title']['ellipsis'], m['title'])
    ids = [k['id'] for k in m['buttons']]
    check(f'  [{tag}] nothing is lost: transport, mute, the explore actions and the input are in it',
          all(i in ids for i in ('prev', 'play', 'next', 'mute', 'input', 'explore', 'side')), ids)
    small = [k for k in m['buttons'] if k['w'] < FINGER - 0.5 or k['h'] < FINGER - 0.5]
    check(f'  [{tag}] every touch target is {FINGER} px each way', not small, small[:3])
    touch_at(b, w / 2, 40)
    closed = b.page(CHROME)['sheet'] is None
    check(f'  [{tag}] a tap above closes it', closed, b.page(CHROME)['sheet'])
    x, y = common.center(b, '#vg-more')
    touch_at(b, x, y)
    sheet = b.page(CHROME)['sheet']
    if vb.engine == 'chromium':
        touch_at(b, w / 2, sheet[1] + 30, move_to=(w / 2, sheet[1] + 150))
        check(f'  [{tag}] a swipe down closes it', b.page(CHROME)['sheet'] is None, b.page(CHROME)['sheet'])
    if b.page(CHROME)['sheet'] is not None:
        touch_at(b, w / 2, 40)


def the_desktop_has_two_capsules_and_tooltips(b):
    m = b.page(CHROME)
    w, h = m['vw'], m['vh']
    ids = [k['id'] for k in m['buttons']]
    check('the_desktop_has_two_capsules_and_tooltips: two capsules in the bottom corners, the centre clear',
          len(m['caps']) == 2 and m['caps'][0][0] < w / 4 and m['caps'][1][2] > 3 * w / 4 and all(c[3] > h - 120 for c in m['caps'])
          and not any(meets(c, centre(w, h)) for c in m['caps']), m['caps'])
    check('  nothing is lost: side, volume, mute, the input chip, the transport and the three explore keys',
          all(i in ids for i in ('side', 'volume-down', 'volume-up', 'mute', 'input', 'prev', 'play', 'next', 'explore')), ids)
    check('  the capsules\' text and the chips hold 4.5:1 over a white sky, the tooltip\'s name too',
          min(m['ink'], m['sub'], m['titleInk'], m['chipOn'], m['tip']) >= 4.5, [m['ink'], m['sub'], m['titleInk'], m['chipOn'], m['tip']])
    tip = "(() => { const t = document.getElementById('vg-tip-scene'); const r = t.getBoundingClientRect(); const s = getComputedStyle(t);" \
          " return { op: +s.opacity, lines: [...t.children].map((c) => Math.round(c.getBoundingClientRect().top)), text: t.textContent }; })()"
    check('  the tooltip is shut at rest', b.page(tip)['op'] == 0, b.page(tip))
    x, y = common.center(b, '#vg-ex-scene')
    if vb.engine == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
    else:
        b.click(x, y)
    got = poll(lambda: b.page(tip)['op'] > 0.9, 3.0)
    t = b.page(tip)
    check('  on hover: two lines (the name and its key, then what it does)',
          got and len(t['lines']) == 2 and t['lines'][1] > t['lines'][0] and 'Next scene' in t['text'] and 'N' in t['text']
          and 'Another world on the same track' in t['text'], t)
    if vb.engine == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': 300, 'y': 300})
    b.press('Tab')
    b.page("document.getElementById('vg-ex-effect').focus()")
    check('  on focus the tooltip is there too (Effect: Fire one of the scene\'s effects)',
          poll(lambda: b.page("+getComputedStyle(document.getElementById('vg-tip-effect')).opacity") > 0.9, 3.0)
          and 'one of the scene' in b.page("document.getElementById('vg-tip-effect').textContent"))
    b.page('document.activeElement.blur()')
    p = b.page(PANEL_NOW)
    check('  the panel: one column of ten, 384 px wide at the top right, clear of the menu button, 18 s',
          p['n'] == 10 and p['cols'] == 1 and abs((p['box'][2] - p['box'][0]) - 384) <= 2 and p['box'][2] <= m['slot'][0] + SLACK
          and '18 s' in p['title'] and p['axis'] == ['-15S', '-10S', '-5S', 'NOW'] and p['figures'] >= 4.5, p)
    check('  its outline is in the scene palette\'s second colour (not the theme teal)', p['outline'] != 'rgb(79, 209, 197)', p['outline'])


def the_chrome_fades_to_nothing_and_a_tap_only_wakes(d, phone):
    """On the page's default idle time (4 s on a phone, 3 s on a desktop): up before it, gone after, back on input."""
    size = (390, 844) if phone else (1280, 800)
    b = make('none', size=size, touch=phone and vb.engine == 'chromium')
    try:
        vb.forget(b, d)
        load(b, d)
        b.page(vb.REC)
        wake = "document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))"
        faded = ("(() => { const q = (s) => +getComputedStyle(document.querySelector(s)).opacity;"
                 " return [q('#vg-dash .vg-cap'), q('#fx-button'), q('#vg-hud')]; })()")
        idle = 4.0 if phone else 3.0
        b.page(wake)
        time.sleep(idle - 1.2)
        up = b.page(faded)
        time.sleep(1.2 + 0.2 + 0.8)
        gone = b.page(faded)
        tag = 'a phone (4 s)' if phone else 'a desktop (3 s)'
        check(f'the_chrome_fades_to_nothing_and_a_tap_only_wakes: {tag}: up just before the idle time, gone (opacity 0) just after: the capsule, the menu button, the HUD',
              min(up[:2]) > 0.9 and set(gone) == {0}, [up, gone])
        if phone:
            gone_chip = b.page("+getComputedStyle(document.getElementById('vg-fchip')).opacity")
            check('  the features chip goes with them', gone_chip == 0, gone_chip)
            before = len(vb.posts(b, '/volume'))
            x, y = common.center(b, '#vg-vup')
            touch_at(b, x, y)
            check('  a tap on the faded chrome brings it back and steps nothing',
                  b.page(faded)[0] > 0.5 and len(vb.posts(b, '/volume')) == before, [b.page(faded), vb.posts(b)])
            touch_at(b, x, y)
            check('  the next tap steps', poll(lambda: len(vb.posts(b, '/volume')) == before + 1, 3.0), vb.posts(b))
        else:
            if vb.engine == 'chromium':
                b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': 400, 'y': 300})
            else:
                b.click(400, 300)
            check('  a pointer move brings it back', poll(lambda: b.page(faded)[0] > 0.9 and b.page(faded)[1] > 0.9, 3.0), b.page(faded))
        check('  no page error', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def the_panel_stays_while_pinned(d):
    b = make('none', size=(1280, 800))
    try:
        vb.forget(b, d)
        load(b, d)
        vb.open_menu(b)
        b.page("document.querySelector('[data-switch=panel-pin]').scrollIntoView({ block: 'center' })")
        vb.click_el(b, '[data-switch=panel-pin]')
        vb.close_menu(b)
        time.sleep(4.5)
        panel = "+getComputedStyle(document.querySelector('#vg-panel .vg-h-pbox')).opacity"
        dash = "+getComputedStyle(document.querySelector('#vg-dash .vg-cap')).opacity"
        check('the_panel_stays_while_pinned: with Pin features panel on, the chrome fades and the panel stays',
              b.page(dash) == 0 and b.page(panel) == 1, [b.page(dash), b.page(panel)])
    finally:
        b.close()

HUD = """(() => { const vh = innerHeight, vw = innerWidth;
  const on = [...document.querySelectorAll('#vg-hud [data-mod][data-on="1"]')];
  const box = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; };
  const pn = document.querySelector('#vg-panel .vg-h-pn');
  return { vh, vw, n: on.length, mods: on.map((e) => [e.dataset.mod, ...box(e)]), panel: box(pn),
    dash: box(document.querySelector('#vg-dash')) }; })()"""

# a moment, then 0.6 s of scene time in steps (the moments run on the steps' dt, not the wall clock)
MOMENT = """(() => { const at = (dt, events) => window.__vgSteps.dispatchEvent(new CustomEvent('step', { detail: { ...window.__vgLast, dt, events } }));
  at(0.01, [{ type: 'section', section: '%s', flash: false }]); for (let i = 0; i < 3; i++) at(0.2, []); return 1; })()"""

# the Host module's scroller: its chips, and whether the last one is inside the scroller's visible box
CHIPS = """(() => { const m = document.querySelector('#vg-hud [data-mod="host"]'); const r = m.getBoundingClientRect();
  const chips = [...m.querySelectorAll('.vg-h-chip')]; const last = chips[chips.length - 1]; if (!last) return { n: 0 };
  const c = last.getBoundingClientRect();
  return { n: chips.length, scrolls: m.scrollHeight > m.clientHeight, focused: document.activeElement === last,
    inside: c.top >= r.top - 1 && c.bottom <= r.bottom + 1 && c.height > 0 }; })()"""

# ---- the cases ----
def the_hud_shows_one_module_at_a_time(b, tag, share=0.30):
    for section in ('build', 'drop'):
        b.page(MOMENT % section)
        time.sleep(0.2)
        m = b.page(HUD)
        w, h = m['vw'], m['vh']
        mod = m['mods'][0] if m['mods'] else None
        check(f'the_hud_shows_one_module_at_a_time: [{tag}] after a {section} moment exactly one module is on', m['n'] == 1, m['mods'])
        if mod:
            check(f'  [{tag}] it is in the window, above the dash, clear of the panel and at most {share:.0%} of the height',
                  inside(mod[1:], w, h) and mod[4] <= m['dash'][1] + SLACK and not meets(mod[1:], m['panel'])
                  and mod[4] - mod[2] <= share * h, [mod, m['dash'], m['panel']])


def the_phone_media_query_is_the_javascripts():
    css = open(os.path.join(WEB, 'style.css')).read()
    js = open(os.path.join(WEB, 'hud.js')).read()
    core = open(os.path.join(WEB, '..', '..', 'core', 'phone.js')).read()
    px = re.search(r'PHONE_MAX_PX\s*=\s*(\d+)', core).group(1)
    q = re.search(r'PHONE_QUERY\s*=\s*`([^`]*)`', js).group(1).replace('${PHONE_MAX_PX}', px)
    blocks = re.findall(r'@media\s*(\([^{]*?max-height:\s*500px\))\s*\{', css)
    check('the_phone_media_query_is_the_javascripts: every phone @media in style.css reads as hud.js PHONE_QUERY',
          len(blocks) >= 2 and all(re.sub(r'\s+', ' ', x).strip() == q for x in blocks), [q, blocks])


def the_host_scroller_is_reached_by_tab(b, tag):
    b.page(MOMENT % 'build')
    time.sleep(0.3)
    b.page("(() => { document.activeElement?.blur(); window.scrollTo(0, 0); return 1; })()")
    state = b.page(CHIPS)
    for _ in range(80):
        if state['n'] == 0 or state.get('focused'):
            break
        b.press('Tab')
        state = b.page(CHIPS)
    check(f'the_host_scroller_is_reached_by_tab: [{tag}] Tab reaches the last chip of the Host module and it is inside the scroller\'s box',
          state['n'] > 0 and state['scrolls'] and state['focused'] and state['inside'], state)


def rules(b, cell, size, mode, side):
    """Every layout rule at one state of the page; returns nothing, checks the lot."""
    # a phone shows the Panel only while the chip has it open
    phone = b.page('innerWidth') < 720 or b.page("matchMedia('(pointer: coarse) and (max-height: 500px)').matches")
    if phone and mode != 'off':
        feat(b, True)
    m = b.page(MEASURE)
    tag = state_tag(cell, size, mode, side)
    w, h = m['iw'], m['ih']
    check(f'nothing_scrolls_sideways_and_the_dash_stands_in_the_window: {tag}: the page is no wider than the window and the dash is inside it',
          m['sw'] <= w + 1 and inside(m['dash'], w, h), [m['sw'], w, m['dash']])
    small = [k for k in m['keys'] if k[3] - k[1] < TARGET_MIN or k[4] - k[2] < TARGET_MIN]
    out = [k for k in m['keys'] if not inside(k[1:], w, h)]
    check(f'every_key_is_a_thumbs_size: {tag}: every dash key that shows is {TARGET_MIN} px or more each way and inside the window',
          not small and not out and len(m['keys']) >= 4, [small, out, len(m['keys'])])
    lines = m['top'] + m['panel'] + [k[1:] for k in m['mods']]
    over = [r for r in lines if meets(r, m['slot'])]
    check(f'the_menu_button_is_clear: {tag}: the button is the thing at the slot and nothing of the HUD or the panel overlaps the slot',
          m['onSlot'] and not over, [m['onSlot'], over[:2], m['slot']])
    if w < 720:
        above = [k for k in m['mods'] if k[4] > m['dash'][1] + SLACK or not inside(k[1:], w, h)]
        check(f'the_modules_stand_above_the_dash: {tag}: a HUD module is in the window and above the dash',
              not above, [above, m['dash']])
    check(f'the_modules_stand_above_the_dash: {tag}: --vg-dash-h is the dash\'s height with its offset (the HUD and the panel keep clear by it)',
          abs(m['dashH'] - (h - m['dash'][1])) <= 2, [m['dashH'], h, m['dash']])
    off = [r for r in m['panel'] if r[0] < -SLACK or r[2] > w + SLACK]
    check(f'the_panel_stays_in_the_window: {tag}: every part of the panel is inside the window across its width (and there are parts to see, unless Off)',
          not off and (bool(m['panel']) == (mode != 'off')), [off[:3], len(m['panel'])])
    if phone:
        feat(b, False)


def layout(cell, d, size):
    b = fresh(cell, d, size)
    try:
        load(b, d)
        time.sleep(1.5)
        for side in ('dac', 'computer'):
            b.page(f"__core.computer.flip('{side}')")
            time.sleep(0.8)
            check(f'the_computer_side_keeps_the_rules: {cell} {size[0]}x{size[1]}: the page shows the {side}',
                  b.page('__core.computer.side()') == side, b.page('__core.computer.side()'))
            for mode in MODES:
                set_mode(b, mode)
                rules(b, cell, size, mode, side)
        check(f'no_console_error_warning_or_csp: {cell} {size[0]}x{size[1]}', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def touch(b, selector):
    x, y = common.center(b, selector)
    pts = [{'x': x, 'y': y, 'id': 1}]
    b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': pts})
    b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
    time.sleep(0.5)


def a_tap_opens_the_menu(d):
    b = fresh('none', d, (390, 844))
    try:
        load(b, d)
        touch(b, '#fx-button')
        check('a_tap_opens_the_menu: a tap on the menu button opens the menu', poll(lambda: b.page("!!document.querySelector('[data-motion]')"), 3.0))
        b.page("document.querySelector('[data-panel=spread]').scrollIntoView({ block: 'center' })")
        touch(b, '[data-panel=spread]')
        check('  a tap on a row takes it', poll(lambda: b.page("document.getElementById('vg-panel').dataset.mode") == 'spread', 3.0),
              b.page("document.getElementById('vg-panel').dataset.mode"))
        check('  no page error', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def a_phone_starts_capped(d):
    b = make('webgl2', size=(390, 844), touch=True)
    try:
        b.preload("try { localStorage.setItem('svolder.theme.voidgarden.motion', '\"full\"'); } catch (e) {}")
        vb.forget(b, d)
        load(b, d)
        time.sleep(3.0)
        p = b.page(f'{vb.DRIVER}.perf()')
        check('a_phone_starts_capped: a coarse pointer begins capped, at density 1, under Full',
              b.page("matchMedia('(pointer: coarse)').matches") is True and p['motion'] == 'full' and p['capped'] is True
              and p['scale'] == 1 and p['pace'] >= 1000 / 30 - 1, p)
        check('  no page error', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def browser35(d, size, finger=True):
    """The no-GPU cell at `size`, a coarse pointer when `finger` (Chromium only)."""
    b = make('none', size=size, touch=finger and vb.engine == 'chromium')
    return b


def phone35(d, size, finger=True):
    tag = f'{size[0]}x{size[1]}' + (' touch' if finger else '')
    b = browser35(d, size, finger)
    try:
        load(b, d)
        poll(lambda: b.page("document.querySelectorAll('#vg-hud [data-row]').length > 3"), 20.0)
        poll(lambda: b.page('!!window.__vgLast'), 20.0)
        poll(lambda: b.page("document.querySelectorAll('#vg-dash .vg-k').length > 3"), 10.0)
        time.sleep(0.5)
        landscape = size[0] > size[1]
        p = the_features_panel_opens_at_the_top_left_of_the_menu_button(b, tag, landscape)
        the_phone_has_a_chip_and_one_capsule(b, tag)
        the_sheet_opens_from_the_dots(b, tag)
        the_hud_shows_one_module_at_a_time(b, tag, share=0.30)
        if size[0] < 720:
            the_host_scroller_is_reached_by_tab(b, tag)
        check(f'no_console_error_warning_or_csp: {tag}', vb.logs(b) == [], vb.logs(b)[:3])
        return None
    finally:
        b.close()


def the_phone_layout(d):
    """3.5's cases on the no-GPU cell: a phone, a bigger phone, a narrow mouse window, sideways, a wide window."""
    finger = vb.engine == 'chromium'
    the_phone_media_query_is_the_javascripts()
    phone35(d, (390, 844), finger)
    phone35(d, (412, 915), finger)
    phone35(d, (600, 800), finger=False)
    if finger:
        phone35(d, (844, 390), True)
    b = make('none', size=(1280, 800))
    try:
        load(b, d)
        poll(lambda: b.page("document.querySelectorAll('#vg-dash .vg-k').length > 3"), 10.0)
        the_desktop_has_two_capsules_and_tooltips(b)
    finally:
        b.close()


# ---- 0.40.0 task 3.13: look run 3's phone findings ----
EXPLORE_NOW = """(() => { const out = {};
  for (const k of ['seed', 'scene', 'effect']) {
    const e = document.querySelector(`#vg-ex-${k}-s`); if (!e) { out[k] = null; continue; }
    const s = getComputedStyle(e); const x = e.querySelector('.vg-xn'); const r = e.getBoundingClientRect();
    out[k] = { text: x.textContent, label: e.getAttribute('aria-label'), w: r.width, h: r.height, top: Math.round(r.top),
      cut: x.getBoundingClientRect().right > r.right + 0.5 || x.getBoundingClientRect().left < r.left - 0.5,
      lines: Math.round(x.getBoundingClientRect().height / parseFloat(getComputedStyle(x).lineHeight || 16)),
      bg: s.backgroundColor, outline: s.outlineStyle === 'none' || parseFloat(s.outlineWidth) === 0 ? 'none' : s.outlineStyle,
      focus: e.matches(':focus-visible'), active: e.matches(':active'), on: e.classList.contains('on') || e.getAttribute('aria-pressed') };
  }
  const row = document.querySelector('.vg-sh-ex'); out.row = row.getBoundingClientRect().right; out.rowScroll = Math.max(...[...row.children].map((c) => c.getBoundingClientRect().right));
  return out; })()"""


def the_phones_sheet_labels_are_one_line_and_not_cut(d, size):
    b = fresh('none', d, size)
    try:
        load(b, d)
        poll(lambda: b.page("!!document.getElementById('vg-more')"), 10.0)
        x, y = common.center(b, '#vg-more')
        touch_at(b, x, y)
        m = b.page(EXPLORE_NOW)
        tag = f'{size[0]}x{size[1]}'
        want = {'seed': '\u23e9\ufe0e Seed', 'scene': '\u23e9\ufe0e Scene', 'effect': 'Effect'}
        names = {'seed': 'New seed (S)', 'scene': 'Next scene (N)', 'effect': 'Effect (E)'}
        ks = [m[k] for k in want]
        check(f'the_explore_labels_are_one_line_with_a_fast_forward_glyph: [{tag}] the sheet reads "\u23e9\ufe0e Seed", "\u23e9\ufe0e Scene", "Effect"; the accessible names stay whole',
              all(k and k['text'] == want[n] and k['label'] == names[n] for n, k in zip(want, ks)), m)
        check(f'  [{tag}] nothing is cut (the label inside its button, the row not scrolling), each label on one line, the three on one row',
              all(k and not k['cut'] and k['lines'] <= 1 for k in ks) and len({k['top'] for k in ks}) == 1 and m['rowScroll'] <= m['row'] + 0.5, m)
        check(f'  [{tag}] 44 px targets', all(k and k['h'] >= FINGER - 0.5 and k['w'] >= FINGER - 0.5 for k in ks), m)
        top = b.page("(() => { const e = document.querySelector('#vg-hud .vg-h-top'); return e ? getComputedStyle(e).display : 'none'; })()")
        check(f'the_phone_has_no_top_line: [{tag}] the HUD\'s top line is not shown (the sheet\'s first line names the device)', top == 'none', top)
        check(f'  [{tag}] no page error', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def a_tap_leaves_no_look_and_the_chrome_fades_together(d):
    """Chromium, a phone: after a finger's tap the explore buttons look as before; the chip, the menu button and the capsule fade as one."""
    b = make('none', size=(390, 844), touch=True)
    try:
        vb.forget(b, d)
        load(b, d)
        b.page(vb.REC)
        x, y = common.center(b, '#vg-more')
        touch_at(b, x, y)
        before = b.page(EXPLORE_NOW)
        for k in ('seed', 'scene', 'effect'):
            x, y = common.center(b, f'#vg-ex-{k}-s')
            touch_at(b, x, y)
        after = b.page(EXPLORE_NOW)
        left = {k: [after[k]['bg'], after[k]['outline'], after[k]['focus'], after[k]['active']] for k in ('seed', 'scene', 'effect')}
        check('a_tap_leaves_no_selected_look: after a finger\'s tap the three explore buttons have the background, outline and focus of an untapped one',
              all(after[k]['bg'] == before[k]['bg'] and after[k]['outline'] == 'none' and not after[k]['focus'] and not after[k]['active'] and not after[k]['on']
                  for k in ('seed', 'scene', 'effect')), [left, {k: before[k]['bg'] for k in ('seed', 'scene', 'effect')}])
        others = b.page("(() => [...document.querySelectorAll('#vg-sheet button')].filter((e) => !e.id.startsWith('vg-ex-')).map((e) => [e.dataset.act, getComputedStyle(e).outlineStyle, e.matches(':focus-visible')]))()")
        check('  the sheet\'s other buttons carry no focus ring after a tap either', all(o[1] == 'none' and not o[2] for o in others), others)
        touch_at(b, 195, 40)
        feat(b, True)
        x, y = common.center(b, '#vg-panel .vg-h-pn')
        touch_at(b, x, y)
        faded = ("(() => { const q = (s) => +getComputedStyle(document.querySelector(s)).opacity;"
                 " return { chip: q('#vg-fchip'), menu: q('#fx-button'), cap: q('#vg-dash .vg-cap'), hud: q('#vg-hud') }; })()")
        wake = "document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))"
        b.page(wake)
        time.sleep(4.0 + 0.2 + 1.2)
        gone = b.page(faded)
        check('the_features_chip_fades_with_the_menu_button_and_the_capsule: after the idle time (4 s) the chip, the menu button, the capsule and the HUD are gone together',
              set(gone.values()) == {0}, gone)
        b.page(wake)
        time.sleep(0.8)
        up = b.page(faded)
        check('  one input brings them all back', min(up['chip'], up['menu'], up['cap']) > 0.9, up)
    finally:
        b.close()


def the_top_line_fades_with_the_chrome_on_a_desktop(d):
    b = make('none', size=(1280, 800))
    try:
        vb.forget(b, d)
        load(b, d)
        b.page(vb.REC)
        q = "(() => { const e = document.querySelector('#vg-hud .vg-h-top'); const s = e && getComputedStyle(e);" \
            " let o = 1; for (let n = e; n; n = n.parentElement) o *= +getComputedStyle(n).opacity;" \
            " return e ? { shown: s.display !== 'none', o } : null; })()"
        b.page("document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))")
        time.sleep(0.8)
        up = b.page(q)
        time.sleep(3.0 + 0.2 + 1.2)
        gone = b.page(q)
        check('the_top_line_fades_with_the_chrome_on_a_desktop: shown while awake (the device\'s line), gone with the rest after the idle time',
              up and up['shown'] and up['o'] > 0.9 and gone and gone['o'] == 0, [up, gone])
    finally:
        b.close()


d = common.Daemon('gungnir2-v2', host='fake')
try:
    for cell in CELLS:
        for size in SIZES if cell == 'none' else [(390, 844)]:
            layout(cell, d, size)
    if vb.engine == 'chromium':
        the_chrome_fades_to_nothing_and_a_tap_only_wakes(d, True)
        the_chrome_fades_to_nothing_and_a_tap_only_wakes(d, False)
        the_panel_stays_while_pinned(d)
        a_tap_leaves_no_look_and_the_chrome_fades_together(d)
        the_top_line_fades_with_the_chrome_on_a_desktop(d)
        a_tap_opens_the_menu(d)
        if 'webgl2' in CELLS:
            a_phone_starts_capped(d)
    if 'none' in CELLS:
        the_phone_layout(d)
        for size in ((360, 740), (390, 844), (412, 915)):
            the_phones_sheet_labels_are_one_line_and_not_cut(d, size)
finally:
    d.stop()
vb.done('voidgarden_phone.py')
