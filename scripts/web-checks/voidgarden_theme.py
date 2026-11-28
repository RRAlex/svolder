# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's page in a browser: what it mounts, the menu's rows and the choices they
keep, the keys, the dash and the driver, the dash's fade and reduced motion. The
cells are `none` (both 3D APIs off: the scene is the still, so the page is what is under test) and
`webgl2` (the stock browser with the Still motion level kept, a live scene drawing once a second);
the tiers' own cases are voidgarden_gpu.py's, the states' voidgarden_states.py's, the phone's
voidgarden_phone.py's and the real-time ones voidgarden_timing.py's (what the checks share is in
voidgarden_base.py). No case reads a colour, a position or a density of the scene: the look is the
look run's to change, and it lives in the baselines and the stills.
  the_page_mounts_the_garden                 the layers in order with the slot last, the canvas hidden from a reader, the ground colour, the menu button in the slot
  the_menu_rows_and_their_defaults           Device controls…, Motion, Audio panel (and Pin features panel), Detail, Renderer: their values in order, the defaults checked
  a_choice_is_kept_across_a_reload           every row's choice is in storage and comes back after a reload
  a_choice_stands_without_storage            storage refused: a choice stands for the session, a reload starts afresh, nothing is logged
  a_cycles_the_audio_panel                   `A` goes Panel, Spread, Off, Panel; the menu follows
  the_volume_keys_step_once_and_ask_the_focus  ↑ and ↓ write one step; nothing with a control focused or the Device controls popup open
  the_dash_keys_in_their_order               `−`, `MUTE`, `+` in that order; the writes; muted reads `−∞ dB`; Controls… opens the popup
  the_driver_reaches_and_refuses             reach, operate and shown for the device's volume and mute; an unknown target is refused without a throw
  the_default_idle_time_is_15_s              no preference: the dash is up at 12 s and dim by 18 s, on the page's fake clock
  reduced_motion_starts_on_calm              Calm is the level while the system asks for less motion, kept nothing; Full can be picked
  the_fade_is_opacity_only_and_any_input_wakes  the bar and the HUD go by opacity only, stay in the tab order; a key or a pointer move brings them back and writes nothing
  no_console_error_warning_or_csp            every page of the cell: none
Usage: python3 voidgarden_theme.py chromium|firefox [none,webgl2] [--part logic|timing]"""
import sys
import time

import common
import fakeclock
import parts
import voidgarden_base as vb
from voidgarden_base import DRIVER, check, load, make, poll

# the logic part is the `none` cell (the pool); the timing part is the live `webgl2` cell, which takes every core while Chromium
# draws it in software and so runs in the serial lane (run-all.sh)
CELLS = (sys.argv[2] if len(sys.argv) > 2 else {'logic': 'none', 'timing': 'webgl2'}.get(parts.PART, 'none,webgl2')).split(',')
ORDER = ['vg-scene', 'vg-hud', 'vg-panel', 'vg-dash', 'vg-card', 'vg-slot']
# the menu's rows in the order the frame lists them: the value ids and their words
ROWS = [
    ('motion', ['full', 'calm', 'still'], ['Full', 'Calm', 'Still'], 'full'),
    ('panel', ['panel', 'spread', 'off'], ['Panel', 'Spread', 'Off'], 'panel'),
    ('detail', ['auto', 'high', 'low'], ['Auto', 'High', 'Low'], 'auto'),
    ('renderer', ['auto', 'webgpu', 'webgl2'], ['Auto', 'WebGPU', 'WebGL2'], 'auto'),
]
CHECKED = """((g) => [...document.querySelectorAll(`[data-${g}]`)].filter((e) => e.getAttribute('aria-checked') === 'true')
  .map((e) => e.dataset[g]))"""
VALUES = """((g) => [...document.querySelectorAll(`[data-${g}]`)].map((e) => e.dataset[g] + '=' + e.textContent.trim()))"""
FADES = "document.querySelector('[data-switch=panel-pin]').getAttribute('aria-checked')"
RAW = '__core.model.page().device.volume.raw'


def tag(cell, name):
    return f'{name} ({cell})'


def fresh(cell, d, **kw):
    b = make(cell, still=cell != 'none', **kw)
    vb.forget(b, d)
    return b


def the_page_mounts_the_garden(cell, d):
    b = fresh(cell, d)
    try:
        check(tag(cell, 'the_page_mounts_the_garden: Void Garden is on show'), load(b, d) == 'voidgarden')
        if cell != 'none':
            check('  the scene is drawn by the cell\'s tier (a silent fall back to the still would pass the live cases on the still)',
                  bool(poll(lambda: b.page(f'{DRIVER}.tier()') == cell, 40.0)), b.page(f'{DRIVER}.tier()'))
        kids = b.page("[...document.getElementById('voidgarden').children].map((e) => e.id)")
        check('  its layers stand in order, the slot last (a still may stand after the canvas)',
              [k for k in kids if k != 'vg-still'] == ORDER and kids[-1] == 'vg-slot', kids)
        check('  the canvas is hidden from a reader', b.page("document.getElementById('vg-scene').getAttribute('aria-hidden')") == 'true')
        check('  the ground is the dark one: #030406', b.page('getComputedStyle(document.body).backgroundColor') == 'rgb(3, 4, 6)')
        check('  the tab title is the core\'s (the device, its state and the app)',
              b.page('document.title').endswith('— svolder') and 'Gungnir 2' in b.page('document.title'), b.page('document.title'))
        slot = b.page("""(() => { const s = document.getElementById('vg-slot').getBoundingClientRect(); const m = document.getElementById('fx-button');
          const at = document.elementFromPoint(s.left + s.width / 2, s.top + s.height / 2);
          return { top: s.top, right: innerWidth - s.right, w: s.width, h: s.height, onTop: !!at && (at === m || m.contains(at)),
            inSlot: document.getElementById('vg-slot').contains(m) || s.width >= 40 }; })()""")
        check('  the menu button is the thing at the slot, which is at the top right and a thumb wide',
              slot['onTop'] and slot['w'] >= 40 and slot['h'] >= 40 and slot['top'] < 40 and slot['right'] < 40, slot)
    finally:
        b.close()


def the_menu_rows_and_their_defaults(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        check(tag(cell, 'the_menu_rows_and_their_defaults: the menu opens'), vb.open_menu(b))
        heads = b.page("[...document.querySelectorAll('.fxh')].map((e) => e.textContent.trim())")
        check('  the headings: Motion, Audio panel, Detail, Renderer', heads == ['Motion', 'Audio panel', 'Detail', 'Renderer'], heads)
        for group, ids, words, default in ROWS:
            got = b.page(f'({VALUES})("{group}")')
            check(f'  {group}: {" | ".join(words)}', got == [f'{i}={w}' for i, w in zip(ids, words)], got)
            on = b.page(f'({CHECKED})("{group}")')
            # the stock cell keeps Still (the check's own), every other row is at its default
            expect = ['still'] if group == 'motion' and cell != 'none' else [default]
            check(f'  {group}: {expect[0]} is the one checked', on == expect, on)
        check('  Pin features panel is off (the panel fades with the rest)', b.page(FADES) == 'false', b.page(FADES))
        check('  the other themes\' rows are not here (no Look, no Arrange)',
              not b.page("!!document.querySelector('[data-look], [data-arrange]')"))
    finally:
        b.close()


def choose(b, selector):
    """Open the menu, press the row, close the menu."""
    vb.open_menu(b)
    # the menu scrolls inside the window: bring the row in before the press
    b.page(f"document.querySelector({selector!r}).scrollIntoView({{ block: 'center' }})")
    vb.click_el(b, selector)
    time.sleep(0.3)
    if b.page("!!document.querySelector('[data-motion]')"):
        vb.close_menu(b)


def pick_all(b):
    choose(b, '[data-motion=calm]')
    choose(b, '[data-panel=spread]')
    choose(b, '[data-detail=low]')
    choose(b, '[data-renderer=webgl2]')
    choose(b, '[data-switch=panel-pin]')


def the_page_state(b):
    vb.open_menu(b)
    got = {g: b.page(f'({CHECKED})("{g}")') for g, *_ in ROWS}
    got['fades'] = b.page(FADES)
    vb.close_menu(b)
    return got


WANT = {'motion': ['calm'], 'panel': ['spread'], 'detail': ['low'], 'renderer': ['webgl2'], 'fades': 'true'}


def a_choice_is_kept_across_a_reload(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        pick_all(b)
        check(tag(cell, 'a_choice_is_kept_across_a_reload: every row took the choice'), the_page_state(b) == WANT, the_page_state(b))
        kept = {k: vb.pref(b, k) for k in ('motion', 'panel', 'detail', 'renderer', 'panel-fades')}
        check('  and storage holds each', kept == {'motion': 'calm', 'panel': 'spread', 'detail': 'low', 'renderer': 'webgl2',
                                                   'panel-fades': False}, kept)
        load(b, d)
        check('  after a reload every row shows it', the_page_state(b) == WANT, the_page_state(b))
        check('  and the panel is the one chosen', b.page(f"{DRIVER}.shown('Audio panel')") in ('spread', 'Spread', None)
              and b.page("document.getElementById('vg-panel').dataset.mode") == 'spread')
    finally:
        b.close()


def a_choice_stands_without_storage(cell, d):
    b = make(cell, storage=False)
    try:
        b.goto(f'{d.url}?theme=voidgarden', settle=1.0)
        b.page(vb.BOOT)
        check(tag(cell, 'a_choice_stands_without_storage: the page mounts with storage refused'), b.page('__core.theme.id') == 'voidgarden')
        choose(b, '[data-panel=spread]')
        choose(b, '[data-detail=high]')
        got = the_page_state(b)
        check('  a choice stands for the session', got['panel'] == ['spread'] and got['detail'] == ['high'], got)
        check('  the panel follows it', b.page("document.getElementById('vg-panel').dataset.mode") == 'spread')
        check('  nothing was kept', vb.pref(b, 'panel') == 'refused', vb.pref(b, 'panel'))
        load(b, d)
        got = the_page_state(b)
        check('  a reload starts afresh at the defaults', got['panel'] == ['panel'] and got['detail'] == ['auto'], got)
        check('  no error, warning or CSP violation', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def a_cycles_the_audio_panel(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        seen = []
        for _ in range(4):
            b.press('a')
            time.sleep(0.25)
            seen.append(b.page("document.getElementById('vg-panel').dataset.mode"))
        check(tag(cell, 'a_cycles_the_audio_panel: Panel, Spread, Off, Panel'), seen == ['spread', 'off', 'panel', 'spread'], seen)
        check('  the menu follows the last one', the_page_state(b)['panel'] == ['spread'])
        check('  and storage keeps it', vb.pref(b, 'panel') == 'spread', vb.pref(b, 'panel'))
    finally:
        b.close()


def the_volume_keys_step_once_and_ask_the_focus(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        b.page(vb.REC)
        time.sleep(0.5)
        raw = b.page(RAW)
        b.press('ArrowUp')
        check(tag(cell, 'the_volume_keys_step_once_and_ask_the_focus: ↑ writes one step up'),
              poll(lambda: len(vb.posts(b, '/volume')) == 1, 3.0) and vb.posts(b, '/volume')[0][1] == f'{{"raw":{raw + 1}}}', vb.posts(b))
        poll(lambda: b.page(RAW) == raw + 1, 3.0)
        b.press('ArrowDown')
        check('  ↓ writes one step down', poll(lambda: len(vb.posts(b, '/volume')) == 2, 3.0)
              and vb.posts(b, '/volume')[1][1] == f'{{"raw":{raw}}}', vb.posts(b))
        # a control with the focus keeps the key
        b.page("document.getElementById('vg-mute').focus()")
        n = len(vb.posts(b, '/volume'))
        b.press('ArrowUp')
        time.sleep(0.6)
        check('  with a control focused the key writes nothing', len(vb.posts(b, '/volume')) == n, vb.posts(b))
        b.page('document.activeElement.blur()')
        # the popup over the page keeps it too
        b.page("__core.router.open('controls')")
        poll(lambda: b.page("__core.router.state().popup") == 'controls', 3.0)
        b.page('document.activeElement && document.activeElement.blur()')
        b.press('ArrowUp')
        time.sleep(0.6)
        check('  with the Device controls popup open the key writes nothing', len(vb.posts(b, '/volume')) == n, vb.posts(b))
        b.press('Escape')
        check('  Escape closes the popup', poll(lambda: b.page('__core.router.state().popup') in (None, ''), 3.0), b.page('__core.router.state()'))
    finally:
        b.close()


def the_dash_keys_in_their_order(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        b.page(vb.REC)
        time.sleep(0.5)
        order = b.page("[...document.querySelectorAll('#vg-dash [data-act]')].map((e) => e.id).filter((i) => ['vg-vdown', 'vg-vup', 'vg-mute'].includes(i))")
        check(tag(cell, 'the_dash_keys_in_their_order: `−`, `+`, mute stand in that order (the figure between the first two)'), order == ['vg-vdown', 'vg-vup', 'vg-mute'], order)
        words = b.page("['vg-vdown', 'vg-mute', 'vg-vup'].map((i) => document.getElementById(i).getAttribute('aria-label'))")
        check('  and are named for a reader', words == ['Quieter', 'Mute', 'Louder'], words)
        raw = b.page(RAW)
        vb.click_el(b, '#vg-vup')
        check('  `+` writes one step up', poll(lambda: any(p[1] == f'{{"raw":{raw + 1}}}' for p in vb.posts(b, '/volume')), 3.0), vb.posts(b))
        vb.click_el(b, '#vg-mute')
        check('  MUTE writes the mute', poll(lambda: any(p[1] == '{"value":true}' for p in vb.posts(b, '/controls/mute')), 3.0), vb.posts(b))
        check('  and the figure reads −∞ dB', poll(lambda: b.page("document.getElementById('vg-vol').textContent") == '−∞ dB', 3.0),
              b.page("document.getElementById('vg-vol').textContent"))
        vb.open_menu(b)
        vb.click_el(b, '#menu-vg-controls')
        check('  Controls… (the menu\'s row) opens the Device controls popup', poll(lambda: b.page('__core.router.state().popup') == 'controls', 3.0),
              b.page('__core.router.state()'))
        b.press('Escape')
        poll(lambda: b.page('__core.router.state().popup') in (None, ''), 3.0)
    finally:
        b.close()


def the_driver_reaches_and_refuses(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        time.sleep(0.5)
        got = b.page(f"""(async () => {{ const dr = {DRIVER}; const out = {{}};
          out.reach = await dr.reach({{ control: 'volume' }});
          out.shown = dr.shown({{ control: 'volume' }});
          out.fact = dr.shown('Output');
          out.unknownReach = await dr.reach({{ control: 'no-such-control' }});
          out.unknownOperate = await dr.operate({{ control: 'no-such-control' }}, {{}});
          out.unknownShown = dr.shown({{ control: 'no-such-control' }});
          const before = __core.model.page().device.volume.raw;
          out.step = await dr.operate({{ control: 'volume' }}, {{ step: 1 }});
          out.moved = await new Promise((done) => {{ const end = performance.now() + 3000;
            const look = () => (__core.model.page().device.volume.raw === before + 1 ? done(true)
              : performance.now() > end ? done(false) : setTimeout(look, 50)); look(); }});
          out.mute = await dr.operate({{ control: 'mute' }}, {{ on: true }});
          out.muted = dr.shown({{ control: 'mute' }});
          out.state = dr.state();
          return out; }})()""")
        check(tag(cell, 'the_driver_reaches_and_refuses: the volume is reached and its figure is the dash\'s'),
              got['reach'] is True and isinstance(got['shown'], str) and '/' not in got['shown'], got)
        check('  a fact is read by its label', got['fact'] and 'playing' in got['fact'], got['fact'])
        check('  an unknown target is refused, never thrown', got['unknownReach'] is False and got['unknownOperate'] is False
              and got['unknownShown'] is None, got)
        check('  a step moves the volume and the mute holds', got['step'] is True and got['moved'] is True and got['mute'] is True
              and got['muted'] == 'on', got)
        check('  state() says how the page stands', isinstance(got['state'], dict) and got['state']['lost'] is False
              and 'toasts' in got['state'], got['state'])
    finally:
        b.close()


def reduced_motion_starts_on_calm(cell, d):
    b = make(cell, reduced=True)
    try:
        vb.forget(b, d)
        load(b, d)
        got = the_page_state(b)
        check(tag(cell, 'reduced_motion_starts_on_calm: Calm is the level while the system asks for less motion'),
              got['motion'] == ['calm'], got)
        check('  and the choice is not kept (the system\'s word, not the person\'s)', vb.pref(b, 'motion') is None, vb.pref(b, 'motion'))
        choose(b, '[data-motion=full]')
        load(b, d)
        got = the_page_state(b)
        check('  Full can be picked and stays across a reload', got['motion'] == ['full'] and vb.pref(b, 'motion') == 'full', got)
    finally:
        b.close()


def the_fade_is_opacity_only_and_any_input_wakes(cell, d):
    b = fresh(cell, d)
    try:
        b.page("localStorage.setItem('svolder.theme.voidgarden.fade-after', '2')")
        load(b, d)
        if cell != 'none':
            # a live scene's first draws block the page for seconds (each driver call waits for them): start once it has drawn
            poll(lambda: b.page(f'{DRIVER}.perf().draws') >= 3, 60.0)
            # and it has faded meanwhile (the idle time is the check's 2 s): a key brings it back
            b.press('Escape')
            b.page("window.dispatchEvent(new PointerEvent('pointermove'))")
            poll(lambda: b.page("+getComputedStyle(document.querySelector('#vg-dash .vg-cap')).opacity") > 0.5
                 and b.page("+getComputedStyle(document.getElementById('vg-hud')).opacity") > 0.5, 40.0)
        b.page(vb.REC)
        bar = "+getComputedStyle(document.querySelector('#vg-dash .vg-cap')).opacity"
        hud = "+getComputedStyle(document.getElementById('vg-hud')).opacity"
        dimmed = "document.getElementById('voidgarden').classList.contains('vg-dim')"
        awake, awake_hud = b.page(bar), b.page(hud)
        check(tag(cell, 'the_fade_is_opacity_only_and_any_input_wakes: the dash is shown while in use'), awake > 0.5, awake)
        dim = poll(lambda: b.page(dimmed), 20.0)
        # the fade is a transition of 0.6 s: wait for it to end. The chrome fades to nothing (the user: 100 %): the
        # capsules, the HUD, the panel (unpinned) and the frame's menu button
        panel = "+getComputedStyle(document.querySelector('#vg-panel .vg-h-pbox')).opacity"
        button = "+getComputedStyle(document.getElementById('fx-button')).opacity"
        def faint():
            first = (b.page(bar), b.page(hud), b.page(panel), b.page(button))
            time.sleep(0.8)
            return first == (b.page(bar), b.page(hud), b.page(panel), b.page(button)) and set(first) == {0}
        low = poll(faint, 8.0)
        was = b.page("""(() => { const e = document.getElementById('vg-dash'); const s = getComputedStyle(e.querySelector('.vg-bar')); const k = getComputedStyle(e.querySelector('button'));
          return { vis: s.visibility === 'visible' && k.visibility === 'visible' ? 'visible' : 'hidden', disp: s.display, inert: e.inert || !!e.closest('[inert]'), hidden: e.hidden,
            tab: [...e.querySelectorAll('button')].every((k) => k.tabIndex >= 0 && !k.inert) }; })()""")
        check('  after the idle time the page is dim: the capsules, the HUD, the panel and the menu button are at 0, by opacity',
              dim and low, [dim, b.page(bar), b.page(hud), b.page(panel), b.page(button)])
        check('  by opacity only: the dash is still shown, in the tab order and in the accessibility tree',
              was['vis'] == 'visible' and was['disp'] != 'none' and not was['inert'] and not was['hidden'] and was['tab'], was)
        before = len(vb.posts(b, '/volume'))
        gone = b.page(bar)
        b.press('Escape')
        # the page fades again two seconds after (the check's idle time): the bar only has to be on its way up
        check('  a key wakes it (the class goes and the bar rises)',
              poll(lambda: not b.page(dimmed), 3.0) and poll(lambda: b.page(bar) > gone + 0.1, 6.0), [gone, b.page(bar)])
        check('  the waking key wrote nothing', len(vb.posts(b, '/volume')) == before, vb.posts(b))
        check('  it fades again after the idle time', poll(lambda: b.page(dimmed), 8.0))
        if vb.engine == 'chromium':
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': 300, 'y': 300})
        else:
            b.click(300, 300)
        check('  a pointer move wakes it', poll(lambda: not b.page(dimmed), 3.0))
        vb.click_el(b, '#vg-vup')
        check('  and the tap then writes', poll(lambda: len(vb.posts(b, '/volume')) == before + 1, 3.0), vb.posts(b))
    finally:
        b.close()


def the_default_idle_time_is_3_s(cell, d):
    # on the page's fake clock (fakeclock.py): the count is the page's rule, not the machine's pace
    b = fresh(cell, d)
    try:
        fakeclock.install(b)
        load(b, d)
        fakeclock.take(b)
        dimmed = "document.getElementById('voidgarden').classList.contains('vg-dim')"
        # the page ran on the real clock for its start (a second or two) before the check took the clock
        # the page's start ran on the real clock: a first input starts the count on the clock the check moves
        b.page("document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))")
        for _ in range(2):
            fakeclock.advance(b, 1000)
        up = not b.page(dimmed)
        for _ in range(3):
            fakeclock.advance(b, 1000)
        check(tag(cell, 'the_default_idle_time_is_3_s: with no preference the chrome is up at 2 s and dim by 5 s (3 s, on the clock the check moves)'),
              up and b.page(dimmed), [up, b.page(dimmed)])
    finally:
        b.close()


def no_console_error_warning_or_csp(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        time.sleep(1.5)
        pick = ['[data-panel=off]', '[data-motion=calm]', '[data-motion=still]']
        for sel in pick:
            choose(b, sel)
        found = vb.logs(b)
        check(tag(cell, 'no_console_error_warning_or_csp: no error, no warning, no CSP violation, no page error'), found == [], found[:4])
    finally:
        b.close()


CASES = [the_page_mounts_the_garden, the_menu_rows_and_their_defaults, a_choice_is_kept_across_a_reload,
         a_choice_stands_without_storage, a_cycles_the_audio_panel, the_volume_keys_step_once_and_ask_the_focus,
         the_dash_keys_in_their_order, the_driver_reaches_and_refuses, reduced_motion_starts_on_calm,
         the_fade_is_opacity_only_and_any_input_wakes, the_default_idle_time_is_3_s, no_console_error_warning_or_csp]
# a live scene costs seconds a page on software rendering, so the stock cell runs the cases that
# are about the scene's own page (what mounts, the rows and their kept choices, the fade, the
# tier); the keys, the dash, the driver and the rest are the page's and run on `none`
LIVE = {the_page_mounts_the_garden, the_menu_rows_and_their_defaults, a_choice_is_kept_across_a_reload,
        the_fade_is_opacity_only_and_any_input_wakes, no_console_error_warning_or_csp}

d = common.Daemon('gungnir2-v2', host='fake')
try:
    for cell in CELLS:
        for case in CASES:
            if case not in LIVE and cell != 'none':
                continue
            case(cell, d)
finally:
    d.stop()
vb.done('voidgarden_theme.py' + parts.part_tag())
