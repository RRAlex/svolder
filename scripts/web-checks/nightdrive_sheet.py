# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive's neon sheet in a browser: opened by DEVICE and by the menu's Device row, closed
by the cross, Escape and a tap on the scene, the scene dimmed under it, the band table stepped and typed
(a held stepper repeats and is let go at 10 s), the mode and Loudness, the presets saved, renamed and
deleted in place, the sheet held while a value is typed or a list is open, the phone's two layouts, a
switch of device with the sheet open, and a guest's sheet. Shared helpers: nightdrive_base.py.
Usage: python3 nightdrive_sheet.py chromium|firefox"""
import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)

# ---- the neon sheet ----
SHEET_OPEN = "!!document.getElementById('nd-dialog')"
SHEET_FOCUS = "(() => { const d = document.getElementById('nd-dialog'); return !!d && d.contains(document.activeElement); })()"
ACTIVE_ID = "document.activeElement ? document.activeElement.id : ''"
FIELD = "(id) => document.getElementById(id).value"


def sheet_open(b, via='#nd-sheet'):
    click_el(b, via)
    return poll(lambda: b.page(SHEET_OPEN), 3.0)


def sheet_closed(b):
    return poll(lambda: not b.page(SHEET_OPEN), 3.0)


def sheet_field(b, ident):
    return b.page(f"document.getElementById('{ident}').value")


def type_into(b, selector, text):
    """Focus the field `selector` with a click, put `text` in it and press Enter."""
    click_el(b, selector)
    b.page(f"(() => {{ const e = document.querySelector({json.dumps(selector)}); e.value = {json.dumps(text)}; return 1; }})()")
    b.press('Enter')


def calm_start(b, d, size=None, query='?theme=nightdrive'):
    """A fresh page on the shown device with its bands, mode and presets as a case expects: the
    five bands at BAND_HZ and BAND_DB, EQ, no preset of the person's own. Records the writes."""
    for row in d.model(DEVICE)['presets']['rows']:
        if not row['built_in']:
            d.post(DEVICE, 'presets/delete', {'name': row['name']})
    reset_bands(d)
    forget(b, d)
    if size:
        b.resize(*size)
    load(b, d, query)
    time.sleep(0.8)
    b.page(REC)


def the_sheet_opens(b, d):
    calm_start(b, d)
    opened = sheet_open(b)
    got = b.page("""(() => { const d = document.getElementById('nd-dialog');
      const s = document.querySelector('.nd-scene'), l = document.getElementById('nd-layer');
      return { role: d.getAttribute('role'), modal: d.getAttribute('aria-modal'), inside: d.contains(document.activeElement),
        scene: s.inert, layer: l.inert, sections: [...d.querySelectorAll('section')].map((x) => x.id) }; })()""")
    check('device_opens_the_sheet_and_focus_goes_in: DEVICE opens a modal dialog and the focus is inside it',
          opened and got['role'] == 'dialog' and got['modal'] == 'true' and got['inside'] is True, got)
    check('  the scene and the dash under it are out of reach',
          got['scene'] is True and got['layer'] is True, got)
    check('  its sections run Mode, Input, Bands, Presets, Controls, Facts', got['sections'] == ['nd-mode', 'nd-input', 'nd-bands', 'nd-presets', 'nd-controls', 'nd-facts'], got)
    boxes = b.page("""(() => { const r = (id) => { const e = document.getElementById(id).getBoundingClientRect();
      return [e.left, e.top, e.right, e.bottom]; };
      const d = document.getElementById('nd-dialog').getBoundingClientRect();
      return { mode: r('nd-mode'), input: r('nd-input'), bands: r('nd-bands'), presets: r('nd-presets'),
        dialog: [d.left, d.top, d.right, d.bottom], w: innerWidth, h: innerHeight }; })()""")
    m, i, bd, pr, dl = boxes['mode'], boxes['input'], boxes['bands'], boxes['presets'], boxes['dialog']
    check('  on a desktop: Mode left of Input, the band table under Mode, the presets under Input',
          m[0] < i[0] and abs(bd[0] - m[0]) < 2 and abs(pr[0] - i[0]) < 2 and bd[1] >= m[3] - 1 and pr[1] >= i[3] - 1
          and abs(m[1] - i[1]) < 2 and abs(bd[1] - pr[1]) < 2, boxes)
    check('  and the sheet is inside the window', dl[0] >= 0 and dl[1] >= 0 and dl[2] <= boxes['w'] and dl[3] <= boxes['h'], boxes)
    b.resize(1280, 1600)
    time.sleep(0.4)
    tall = b.page("(() => { const d = document.getElementById('nd-dialog').getBoundingClientRect(); return [d.top, d.bottom, innerHeight, document.documentElement.scrollHeight]; })()")
    check('  a tall window keeps the sheet whole and the page does not scroll',
          tall[0] >= 0 and tall[1] <= tall[2] and tall[3] <= tall[2], tall)
    b.resize(1280, 900)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_sheet_from_the_menu(b, d):
    calm_start(b, d)
    open_menu(b)
    row = b.page("(() => { const r = document.getElementById('menu-nd-sheet'); return r ? [r.textContent.trim(), r.hidden] : null; })()")
    click_el(b, '#menu-nd-sheet')
    opened = poll(lambda: b.page(SHEET_OPEN), 3.0)
    check('the_menu_device_row_opens_it: the menu has a Device… row and it opens the sheet with the focus inside',
          row is not None and row[0] == 'Device…' and opened and b.page(SHEET_FOCUS) is True, [row, opened])
    b.press('Escape')
    check('  Escape closes it and the focus goes back to the menu button',
          sheet_closed(b) and b.page(ACTIVE_ID) == 'fx-button', [b.page(SHEET_OPEN), b.page(ACTIVE_ID)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_sheet_closes(b, d):
    calm_start(b, d)
    opened = sheet_open(b)
    click_el(b, '#nd-sheet-x')
    shut = sheet_closed(b)
    check('close_by_x_escape_or_a_tap_on_the_scene_and_focus_returns: the cross closes it, the dash works again and DEVICE has the focus',
          opened and shut and b.page(ACTIVE_ID) == 'nd-sheet'
          and b.page("document.getElementById('nd-layer').inert") is False
          and b.page("document.querySelector('.nd-scene').inert") is False,
          [opened, shut, b.page(ACTIVE_ID)])
    sheet_open(b)
    b.press('Escape')
    check('  Escape closes it and DEVICE has the focus', sheet_closed(b) and b.page(ACTIVE_ID) == 'nd-sheet', b.page(ACTIVE_ID))
    sheet_open(b)
    b.click(30, 450)
    check('  a tap on the scene closes it and DEVICE has the focus', sheet_closed(b) and b.page(ACTIVE_ID) == 'nd-sheet', b.page(ACTIVE_ID))
    sheet_open(b)
    click_el(b, '#nd-bands')
    time.sleep(0.3)
    check('  a tap inside the sheet does not close it', b.page(SHEET_OPEN) is True)
    b.press('Escape')
    sheet_closed(b)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_sheet_runs_dimmed(b, d):
    calm_start(b, d)
    still = b.page("getComputedStyle(document.querySelector('.nd-scene')).filter")
    sheet_open(b)
    dim = b.page("[getComputedStyle(document.querySelector('.nd-scene')).filter, getComputedStyle(document.getElementById('nd-layer')).filter]")
    drew = rate(b, 1.0)
    check('the_scene_runs_dimmed_under_it: both the scene and the dash are dimmed (brightness .45, saturate .8) and the scene goes on drawing',
          still == 'none' and all('brightness(0.45)' in f and 'saturate(0.8)' in f for f in dim) and drew >= 5, [still, dim, drew])
    b.press('Escape')
    sheet_closed(b)
    check('  and undimmed once it is closed',
          b.page("getComputedStyle(document.querySelector('.nd-scene')).filter") == 'none')


PARAM_OF = {'f': 'frequency', 'g': 'gain', 'q': 'q'}


def sheet_band_posts(b):
    return [(int(u.rsplit('/', 1)[1]), json.loads(body)) for u, body, _ in b.page('window.__posts')
            if 'api/v1/devices/' in u and '/eq/bands/' in u and body]


def the_band_table(b, d):
    calm_start(b, d)
    sheet_open(b)
    for n in range(1, 6):
        for p in 'fgq':
            click_el(b, f'#nd-b{n}{p}p')
    time.sleep(1.5)
    posts = sheet_band_posts(b)
    steps = {(n, body['param']) for n, body in posts if body.get('step') == 1 and set(body) == {'param', 'step'}}
    check('the_band_table_steps_and_types_each_value: a `+` on every frequency, gain and Q of the five bands writes a step of 1 to that band',
          steps == {(n, PARAM_OF[p]) for n in range(1, 6) for p in 'fgq'}, [sorted(steps), len(posts)])
    reset_bands(d)
    b.page('window.__posts.length = 0')
    time.sleep(0.5)
    click_el(b, '#nd-b2gm')
    time.sleep(0.8)
    down = sheet_band_posts(b)
    check('  a `−` writes a step of −1 to that band only', down == [(2, {'param': 'gain', 'step': -1})], down)
    reset_bands(d)
    b.page('window.__posts.length = 0')
    time.sleep(0.5)
    typed = {'f': '1000', 'g': '+4.0', 'q': '2'}
    for p, text in typed.items():
        type_into(b, f'#nd-b3{p}', text)
        time.sleep(0.5)
    posts = sheet_band_posts(b)
    check('  a typed value and Enter write that value to that band',
          posts == [(3, {'param': PARAM_OF[p], 'value': text}) for p, text in typed.items()], posts)
    rows = '__core.model.page().device.tone.bands.rows[2]'
    check('  and the field shows what the device has once it answers (the nearest step it has)',
          poll(lambda: sheet_field(b, 'nd-b3g') == b.page(rows + '.g') != '+3.00 dB' and sheet_field(b, 'nd-b3f') == b.page(rows + '.f')
               and sheet_field(b, 'nd-b3f') != '', 5.0),
          [sheet_field(b, 'nd-b3g'), sheet_field(b, 'nd-b3f'), b.page(rows)])
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-b4g')
    b.page("document.getElementById('nd-b4g').value = '9'")
    b.press('Escape')
    time.sleep(0.4)
    check('  Escape puts the old value back, writes nothing and leaves the sheet open',
          sheet_band_posts(b) == [] and sheet_field(b, 'nd-b4g') != '9' and b.page(SHEET_OPEN) is True,
          [sheet_band_posts(b), sheet_field(b, 'nd-b4g')])
    reset_bands(d)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_band_hold(b, d):
    """Chromium and Firefox with synthetic presses: a held stepper repeats the way the volume does."""
    calm_start(b, d)
    sheet_open(b)

    def count():
        return len(sheet_band_posts(b))

    def stops(how, name):
        reset_bands(d)
        b.page('window.__posts.length = 0')
        synth(b, '#nd-b3qp', 'pointerdown')
        time.sleep(0.9)
        went = count()
        how()
        time.sleep(0.4)
        then = count()
        time.sleep(0.6)
        check(name, went >= 3 and count() == then, [went, then, count()])

    reset_bands(d)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-b3qp', 'pointerdown')
    time.sleep(0.2)
    first = count()
    time.sleep(1.0)
    synth(b, '#nd-b3qp', 'pointerup')
    time.sleep(0.4)
    posts = [[body, at] for (u, body, at) in b.page('window.__posts') if '/eq/bands/' in u and body]
    held = len(posts)
    gaps = [round(q[1] - p[1]) for p, q in zip(posts, posts[1:])]
    print(f'     holding +: {held} posts in 1.2 s, gaps {gaps[:12]}')
    check('holding_a_band_stepper_repeats: one step at the press, then, after the pause, the steps keep coming',
          first == 1 and held >= 5, [first, held])
    check('  the first repeat comes about 380 ms after the press (300 to 600)',
          len(posts) > 1 and 300 <= posts[1][1] - posts[0][1] <= 600, gaps[:3])
    check('  and it steps the same band and parameter each time',
          all(json.loads(p[0])['param'] == 'q' and json.loads(p[0])['step'] == 1 for p in posts), posts[:2])
    time.sleep(0.5)
    check('  a release stops them', count() == held, [held, count()])
    stops(lambda: synth(b, '#nd-b3qp', 'pointercancel'), '  a cancelled pointer stops them')
    stops(lambda: b.page("window.dispatchEvent(new Event('blur'))"), '  a blur of the window stops them')
    stops(lambda: b.page(HIDE), '  a hidden page stops them')
    b.page(SHOW)
    # a redraw in the middle of a hold keeps going, the key replaced by its twin of the same id
    reset_bands(d)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-b3qp', 'pointerdown')
    time.sleep(0.5)
    b.page("(() => { window.__btn = document.getElementById('nd-b3qp'); __core.model.emit('fleet', __core.model.page().fleet); return 1; })()")
    mid = count()
    time.sleep(0.6)
    check('  the steps go on while the sheet is held and a reading from elsewhere does not replace the key',
          b.page("window.__btn.isConnected && window.__btn === document.getElementById('nd-b3qp')") is True and count() > mid,
          [mid, count()])
    check('  the value moves in place while the key is held',
          poll(lambda: sheet_field(b, 'nd-b3q') == b.page('__core.model.page().device.tone.bands.rows[2].q'), 3.0),
          [sheet_field(b, 'nd-b3q'), b.page('__core.model.page().device.tone.bands.rows[2].q')])
    menu = b.page("""(() => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      document.getElementById('nd-b3qp').dispatchEvent(e); return e.defaultPrevented; })()""")
    check('  a context menu (a long press on a phone) on a held key is prevented', menu is True, menu)
    synth(b, '#nd-b3qp', 'pointerup')
    time.sleep(0.3)
    # a hold ends at 10 s whatever else: steps still arrive between 9.2 s and 9.8 s after the press and past 10.2 s, none after 10.7 s
    reset_bands(d)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-b3qp', 'pointerdown')
    time.sleep(11.5)
    stamps = [at for u, body, at in b.page('window.__posts') if '/eq/bands/' in u and body]
    first = stamps[0] if stamps else 0
    late = [round(at - first) for at in stamps if at - first > 10700]
    past = [at for at in stamps if 10200 <= at - first <= 10700]
    between = [at for at in stamps if 9200 <= at - first <= 9800]
    check('  a held key is let go at 10 s: steps still come between 9.2 s and 9.8 s and past 10.2 s, none after 10.7 s',
          len(between) >= 3 and len(past) >= 1 and late == [], [len(between), len(past), late[:3], round(stamps[-1] - first) if stamps else None])
    synth(b, '#nd-b3qp', 'pointerup')
    time.sleep(0.3)
    reset_bands(d)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-b3qp', 'pointerdown')
    time.sleep(0.9)
    went = count()
    b.page("__core.model.emit('state', 'lost')")
    time.sleep(0.4)
    then = count()
    time.sleep(0.6)
    check('  the stream lost stops them', went >= 3 and count() == then, [went, then, count()])
    synth(b, '#nd-b3qp', 'pointerup')
    b.page("__core.model.emit('state', 'open')")
    check('  no page error', errors(b) == [], errors(b)[:3])


def mode_posts(b):
    return [json.loads(body) for u, body, _ in b.page('window.__posts') if u.endswith('eq/mode') and body]


def loud_posts(b):
    return [json.loads(body) for u, body, _ in b.page('window.__posts') if u.endswith('eq/loudness') and body]


def the_mode_and_loudness(b, d):
    calm_start(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    poll(lambda: d.model(DEVICE)['tone']['mode'] == 'eq', 5.0)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    sheet_open(b)
    click_el(b, '#nd-mode-loudness')
    check('the_mode_and_loudness_strength_set: a tap on Loudness writes that mode',
          poll(lambda: mode_posts(b) == [{'mode': 'loudness'}], 4.0), mode_posts(b))
    seen = poll(lambda: b.page("!!document.getElementById('nd-loudp')"), 5.0)
    words = b.page("(() => { const c = document.getElementById('nd-mode-loudness'); return [c.getAttribute('aria-pressed'), c.querySelector('i').textContent, c.textContent]; })()")
    check('  Loudness then carries its round − and + and its strength under the name',
          seen and words[0] == 'true' and words[1].startswith('strength ') and words[2].startswith('Loudness'), words)
    order = b.page("""(() => { const r = (id) => document.getElementById(id).getBoundingClientRect();
      const c = r('nd-mode-loudness'); return [r('nd-loudm').right, (c.left + c.right) / 2, r('nd-loudp').left, c.width, c.left, c.right,
        r('nd-loudm').left, r('nd-loudp').right]; })()""")
    check('  the round keys are on its two ends, inside the chip, one each side of its middle',
          order[0] < order[1] < order[2] and order[4] <= order[6] and order[7] <= order[5], order)
    now = b.page("__core.model.page().device.tone.strength")
    click_el(b, '#nd-loudp')
    check('  the round + writes a step of 1 to the loudness', poll(lambda: loud_posts(b) == [{'step': 1}], 4.0), loud_posts(b))
    check('  and the strength under the name follows the device',
          poll(lambda: b.page("document.querySelector('#nd-mode-loudness i').textContent") == f"strength {b.page('__core.model.page().device.tone.strength')}"
               and b.page('__core.model.page().device.tone.strength') != now, 5.0),
          [now, b.page("document.querySelector('#nd-mode-loudness i').textContent")])
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-loudm')
    check('  the round − writes a step of −1', poll(lambda: loud_posts(b) == [{'step': -1}], 4.0), loud_posts(b))
    b.page('window.__posts.length = 0')
    synth(b, '#nd-loudp', 'pointerdown')
    time.sleep(0.9)
    synth(b, '#nd-loudp', 'pointerup')
    time.sleep(0.4)
    check('  a held round key repeats', len(loud_posts(b)) >= 3, len(loud_posts(b)))
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-mode-eq')
    check('  a tap on EQ writes that mode', poll(lambda: mode_posts(b) == [{'mode': 'eq'}], 4.0), mode_posts(b))
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    check('  no page error', errors(b) == [], errors(b)[:3])


def preset_posts(b, tail):
    return [json.loads(body) for u, body, _ in b.page('window.__posts') if u.endswith(tail) and body]


def preset_names(b):
    return b.page("[...document.querySelectorAll('#nd-presets [data-act=preset]')].map((c) => c.dataset.name)")


def the_presets(b, d):
    calm_start(b, d)
    sheet_open(b)
    builtin = [r['name'] for r in d.model(DEVICE)['presets']['rows']]
    check('a_preset_saves_renames_and_deletes: the built-in presets are chips', preset_names(b) == builtin and len(builtin) >= 3, preset_names(b))
    click_el(b, f'#nd-presets [data-name="{builtin[1]}"]')
    check('  a tap on a chip recalls that preset', poll(lambda: preset_posts(b, 'presets/recall') == [{'name': builtin[1]}], 4.0),
          preset_posts(b, 'presets/recall'))
    click_el(b, '#nd-preset-save')
    form = poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-askbox input')"), 3.0)
    check('  Save as… opens a form in place with the focus in its field',
          form and b.page("document.activeElement === document.querySelector('#nd-presets .nd-askbox input')") is True)
    b.page('window.__posts.length = 0')
    b.page("document.querySelector('#nd-presets .nd-askbox input').value = 'night one'")
    b.press('Enter')
    check('  Enter saves it under that name', poll(lambda: preset_posts(b, 'presets') == [{'name': 'night one'}], 4.0),
          preset_posts(b, 'presets'))
    check('  and it is a chip of the person\'s own, on, once the device answers',
          poll(lambda: 'night one' in preset_names(b), 5.0) and b.page(
              "document.querySelector('#nd-presets [data-name=\"night one\"]').classList.contains('own')") is True,
          preset_names(b))
    check('  the form is gone and the focus is back on Save as…', poll(lambda: b.page(
        "!document.querySelector('#nd-presets .nd-askbox')") and b.page(ACTIVE_ID) == 'nd-preset-save', 3.0), b.page(ACTIVE_ID))
    # rename: the bands are on Warm (what was recalled), so a list of the person's own asks which
    click_el(b, '#nd-preset-rename')
    which = poll(lambda: b.page("[...document.querySelectorAll('.nd-list .nd-opt')].map((o) => o.dataset.value)"), 3.0)
    check('  Rename, with the bands on another preset, lists the person\'s own to choose from', which == ['night one'], which)
    click_el(b, '.nd-opt[data-value="night one"]')
    poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-askbox input')"), 3.0)
    check('  Rename asks for the new name with the old one in the field',
          b.page("document.querySelector('#nd-presets .nd-askbox input').value") == 'night one')
    b.page('window.__posts.length = 0')
    b.page("document.querySelector('#nd-presets .nd-askbox input').value = 'night two'")
    b.press('Enter')
    check('  Enter renames it', poll(lambda: preset_posts(b, 'presets/rename') == [{'from': 'night one', 'to': 'night two'}], 4.0),
          preset_posts(b, 'presets/rename'))
    check('  and the chip has the new name', poll(lambda: 'night two' in preset_names(b) and 'night one' not in preset_names(b), 5.0),
          preset_names(b))
    # a cancelled ask writes nothing
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-preset-rename')
    poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == 1, 3.0)
    click_el(b, '.nd-opt[data-value="night two"]')
    poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-askbox input')"), 3.0)
    b.press('Escape')
    check('  Escape closes the form, writes nothing and the sheet stays open',
          poll(lambda: b.page("!document.querySelector('#nd-presets .nd-askbox')"), 3.0)
          and preset_posts(b, 'presets/rename') == [] and b.page(SHEET_OPEN) is True)
    # delete asks first
    click_el(b, '#nd-preset-delete')
    poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == 1, 3.0)
    click_el(b, '.nd-opt[data-value="night two"]')
    asked = poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-question')"), 3.0)
    check('  Delete asks in place and the safe answer has the focus',
          asked and b.page("document.activeElement.dataset.answer") == 'keep', b.page("document.activeElement.dataset.answer"))
    click_el(b, '#nd-presets [data-answer=keep]')
    time.sleep(0.4)
    check('  Keep deletes nothing', preset_posts(b, 'presets/delete') == [] and 'night two' in preset_names(b), preset_names(b))
    click_el(b, '#nd-preset-delete')
    poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == 1, 3.0)
    click_el(b, '.nd-opt[data-value="night two"]')
    poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-question')"), 3.0)
    check('  nothing is deleted before the answer', preset_posts(b, 'presets/delete') == [])
    click_el(b, '#nd-presets [data-answer=delete]')
    check('  Delete deletes it', poll(lambda: preset_posts(b, 'presets/delete') == [{'name': 'night two'}], 4.0),
          preset_posts(b, 'presets/delete'))
    check('  and the chip is gone', poll(lambda: preset_names(b) == builtin, 5.0), preset_names(b))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_sheet_holds(b, d):
    calm_start(b, d)
    sheet_open(b)
    click_el(b, '#nd-b2g')
    before = sheet_field(b, 'nd-b5g')
    other = d.model(DEVICE)['tone']['readings']['bands'][4]
    d.post(DEVICE, 'eq/bands/5', {'param': 'gain', 'raw': other[1] - 3 if other[1] > 3 else other[1] + 3})
    poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][4][1] != other[1], 5.0)
    time.sleep(0.6)
    check('the_sheet_holds_while_typing_or_a_list_is_open: while a value is typed in, a change from elsewhere does not redraw the sheet',
          b.page(ACTIVE_ID) == 'nd-b2g' and sheet_field(b, 'nd-b5g') == before, [b.page(ACTIVE_ID), sheet_field(b, 'nd-b5g'), before])
    b.press('Escape')
    check('  and it is drawn from the newest model once the field is let go',
          poll(lambda: sheet_field(b, 'nd-b5g') != before, 4.0), [sheet_field(b, 'nd-b5g'), before])
    check('  Escape in a field leaves the sheet open', b.page(SHEET_OPEN) is True)
    check('  no page error', errors(b) == [], errors(b)[:3])


def input_now(d):
    c = next(x for x in d.model(DEVICE)['controls'] if x['key'] == 'input')
    return next(o['label'] for o in c['options'] if o['on']), c


def the_sheet_list():
    """A device with six inputs: the input is one chip that opens a list; the sheet is held while it
    is open."""
    g = common.Daemon('yggdrasil', host='fake')
    b = make()
    try:
        forget(b, g)
        load(b, g)
        time.sleep(0.8)
        b.page(REC)
        sheet_open(b)
        now, c = input_now(g)
        other = next(o for o in c['options'] if o['label'] != now)
        check('the_sheet_holds_while_a_list_is_open: six inputs are one chip naming the current input',
              b.page("document.querySelectorAll('#nd-input [data-act=input]').length") == 0
              and b.page("document.getElementById('nd-input-list').textContent") == f'{now} ▾', now)
        click_el(b, '#nd-input-list')
        opened = poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == len(c['options']), 3.0)
        check('  the list opens with the current input marked and the chip expanded',
              opened and b.page("document.querySelector('.nd-opt.on').textContent") == now
              and b.page("document.getElementById('nd-input-list').getAttribute('aria-expanded')") == 'true')
        g.post(DEVICE, 'controls/input', json.loads(other['json']))
        poll(lambda: input_now(g)[0] == other['label'], 8.0)
        time.sleep(0.8)
        check('  a change from elsewhere does not redraw the sheet while the list is open (the list and the chip stay)',
              b.page("document.querySelectorAll('.nd-list').length") == 1
              and b.page("document.getElementById('nd-input-list').textContent") == f'{now} ▾', b.page(
                  "document.getElementById('nd-input-list').textContent"))
        b.press('Escape')
        check('  Escape closes the list, keeps the sheet, and the sheet then shows what came, the focus on the chip',
              poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 0 and b.page(SHEET_OPEN) is True
                   and b.page("document.getElementById('nd-input-list').textContent") == f"{other['label']} ▾"
                   and b.page(ACTIVE_ID) == 'nd-input-list', 4.0),
              [b.page("document.getElementById('nd-input-list').textContent"), b.page(ACTIVE_ID)])
        b.page('window.__posts.length = 0')
        click_el(b, '#nd-input-list')
        poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 1, 3.0)
        want = now
        click_el(b, f'.nd-opt[data-value="{want}"]')
        check('  a pick from the list writes that input and the list is gone',
              poll(lambda: input_now(g)[0] == want, 8.0) and len(posts_to(b, 'controls/input')) == 1
              and b.page("document.querySelectorAll('.nd-list').length") == 0, [want, posts_to(b, 'controls/input')])
        b.page('window.__posts.length = 0')
        click_el(b, '#nd-input-list')
        poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 1, 3.0)
        click_el(b, '#nd-input-list')
        check('  a second tap on the chip closes the list and writes nothing',
              poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 0, 3.0)
              and posts_to(b, 'controls/input') == [])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


def the_sheet_input_chips():
    """Two inputs are two chips in the sheet (the Loki Max's XLR and RCA); a tap writes that input."""
    two = common.Daemon('lokimax', host='fake')
    b = make()
    try:
        forget(b, two)
        load(b, two)
        time.sleep(0.8)
        b.page(REC)
        sheet_open(b)
        chips = b.page("[...document.querySelectorAll('#nd-input [data-act=input]')].map((c) => [c.textContent, c.getAttribute('aria-pressed')])")
        check('the_sheet_input_is_chips_for_a_short_list: XLR and RCA are two chips, one pressed',
              sorted(c[0] for c in chips) == ['RCA', 'XLR'] and sorted(c[1] for c in chips) == ['false', 'true'], chips)
        off = next(c[0] for c in chips if c[1] == 'false')
        click_el(b, f'#nd-input [data-value="{off}"]')

        def chosen():
            m = two.model(DEVICE)
            return next(o['label'] for o in next(x for x in m['controls'] if x['key'] == 'input')['options'] if o['on'])
        check('  a tap writes that input and the chip is pressed', poll(lambda: chosen() == off, 8.0) and poll(
            lambda: b.page(f'document.querySelector(\'#nd-input [data-value="{off}"]\').getAttribute("aria-pressed")') == 'true', 5.0),
            [chosen(), off])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, two)
        finally:
            b.close()
            two.stop()


def the_sheet_on_a_phone(b, d):
    calm_start(b, d, size=(390, 844))
    sheet_open(b)
    time.sleep(0.5)
    got = b.page("""(() => { const r = (id) => document.getElementById(id).getBoundingClientRect();
      const d = r('nd-dialog'); const chip = r('nd-mode-eq'); const st = r('nd-b1gp');
      return { top: d.top, left: d.left, right: d.right, bottom: d.bottom, w: innerWidth, h: innerHeight,
        mode: r('nd-mode').top, input: r('nd-input').top, presets: r('nd-presets').top, bands: r('nd-bands').top,
        chip: chip.height, st: [st.width, st.height], x: r('nd-sheet-x').height }; })()""")
    check('a_phone_sheet_slides_up_to_30_percent_from_the_top: the sheet spans the width from 30 % of the height to the bottom',
          abs(got['top'] - 0.3 * got['h']) <= 3 and got['left'] <= 1 and got['right'] >= got['w'] - 1 and got['bottom'] >= got['h'] - 1, got)
    check('  one column: Mode, Input, presets, then the bands', got['mode'] < got['input'] < got['presets'] < got['bands'], got)
    check('  44 px targets: the chips, the band steppers and the cross', got['chip'] >= 44 and min(got['st']) >= 44 and got['x'] >= 44, got)
    scroll = b.page("(() => { const e = document.getElementById('nd-sheet-body'); return [e.scrollHeight > e.clientHeight, document.documentElement.scrollHeight <= innerHeight]; })()")
    check('  the sheet scrolls inside itself and the page does not', scroll == [True, True], scroll)
    b.press('Escape')
    sheet_closed(b)
    b.resize(844, 390)
    time.sleep(0.4)
    sheet_open(b)
    time.sleep(0.5)
    side = b.page("""(() => { const d = document.getElementById('nd-dialog').getBoundingClientRect();
      return { left: d.left, right: d.right, top: d.top, bottom: d.bottom, w: innerWidth, h: innerHeight }; })()""")
    check('a_phone_in_landscape_gets_the_sheet_from_the_right: min(420 px, 60 vw) wide against the right edge, the full height',
          abs(side['right'] - side['w']) <= 1 and abs((side['right'] - side['left']) - min(420, 0.6 * side['w'])) <= 2
          and side['top'] <= 1 and side['bottom'] >= side['h'] - 1, side)
    b.press('Escape')
    sheet_closed(b)
    b.resize(1280, 900)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_sheet_switch():
    """The sheet open on one device, a switch to another: it is drawn for the new device and nothing
    is written to the old one or to the new."""
    d = common.Daemon('gungnir2-v2', host='fake', fleet=[('bifrost3', NEW_DEVICE, '1112131415161718')])
    b = make()
    try:
        calm_start(b, d)
        d.post(NEW_DEVICE, 'eq/bands/1', {'param': 'gain', 'raw': 110})
        poll(lambda: d.model(NEW_DEVICE)['tone']['readings']['bands'][0][1] == 110, 5.0)
        before = d.model(NEW_DEVICE)['tone']['readings']['bands']
        load(b, d)
        time.sleep(0.8)
        b.page(REC)
        sheet_open(b)
        old = sheet_field(b, 'nd-b1g')
        click_el(b, '#nd-preset-save')
        poll(lambda: b.page("!!document.querySelector('#nd-presets .nd-askbox input')"), 3.0)
        b.page("document.querySelector('#nd-presets .nd-askbox input').value = 'half done'")
        synth(b, '#nd-b1gp', 'pointerdown')
        time.sleep(0.5)
        went = len(band_posts(b, DEVICE))
        url = b.page(f"__core.model.page().fleet.devices.find((x) => x.name === '{NEW_DEVICE}').url")
        b.page(f"__core.actions.showDevice('{url}', {{ push: false }})")
        poll(lambda: b.page('__core.model.page().device.name') == NEW_DEVICE, 5.0)
        time.sleep(0.4)
        count = len(b.page('window.__posts'))
        time.sleep(1.2)
        synth(b, '#nd-b1gp', 'pointerup')
        time.sleep(0.5)
        new = b.page("__core.model.page().device.tone.bands.rows[0].g")
        check('a_device_switch_with_the_sheet_open_redraws_it_for_the_new_device: the sheet stays open and shows the new device\'s bands',
              b.page(SHEET_OPEN) is True and sheet_field(b, 'nd-b1g') == new and new != old, [sheet_field(b, 'nd-b1g'), new, old])
        check('  the question open for the old device is gone and nothing was posted by the switch, the held key or the question',
              b.page("!document.querySelector('#nd-presets .nd-askbox')") and went >= 1
              and len(b.page('window.__posts')) == count, [went, count, len(b.page('window.__posts'))])
        check('  nothing at all was written to the device now shown', band_posts(b, NEW_DEVICE) == [] and [
            u for u, body, _ in b.page('window.__posts') if body and urllib.parse.quote(NEW_DEVICE) in u] == [])
        check('  and it has the bands it had', d.model(NEW_DEVICE)['tone']['readings']['bands'] == before)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_sheet_guest():
    """A real guest: the sheet is drawn inert, a tap posts nothing and the core says why."""
    g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = make()
    try:
        forget(b, g)
        load(b, g)
        time.sleep(0.8)
        b.page(REC)
        opened = sheet_open(b)
        live = b.page("[...document.querySelectorAll('#nd-dialog button:not([aria-disabled=true])')].map((x) => x.id)")
        check('a_guests_sheet_is_inert_in_a_browser: every key of a guest\'s sheet but the cross and the facts\' fold is aria-disabled',
              opened and live == ['nd-sheet-x', 'nd-facts-fold'], live)
        # the core is asked each time (a tap is never swallowed by the page): its actions are wrapped to say so
        b.page("""(() => { window.__asked = []; for (const name of ['setMode', 'setBand', 'recallPreset', 'setControl']) {
          const was = __core.actions[name]; __core.actions[name] = (...a) => { window.__asked.push(name); return was(...a); }; }
          return 1; })()""")
        for one in ['#nd-mode-flat', '#nd-b2gp', '#nd-pr-1', '#nd-input-0']:
            if not b.page(f"!!document.querySelector('{one}')"):
                break  # the page went away: a request went out and the daemon sent it to sign in
            click_el(b, one)
            time.sleep(0.4)
        asked = b.page('window.__asked')
        posts = b.page('window.__posts')  # None: the page went to the sign-in, which only a request that went out brings
        writes = None if posts is None else [u for u, body, _ in posts if 'api/v1/devices/' in u and body]
        toast = b.page('document.body.innerText')
        check('  a tap on a mode, a stepper, a preset and an input is asked of the core, posts nothing and the core says to sign in',
              asked == ['setMode', 'setBand', 'recallPreset', 'setControl'] and writes == [] and 'Sign in' in toast,
              [asked, writes, toast[-160:]])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


def posts_json(b, tail):
    return [json.loads(body) for u, body, _ in b.page('window.__posts') if u.endswith(tail) and body]


def click_in(b, selector):
    """Click an element of the sheet's own scroll: brought into the middle of its view first."""
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.2)
    click_el(b, selector)


def the_facts_unfold_and_the_choice_survives_a_reload(b, d):
    calm_start(b, d)
    sheet_open(b)
    state = b.page("""(() => { const f = document.getElementById('nd-facts-body'); const h = document.getElementById('nd-facts-fold');
      return { hidden: f.hidden, expanded: h.getAttribute('aria-expanded'), stored: localStorage.getItem('svolder.theme.nightdrive.facts') }; })()""")
    check('the_facts_start_folded: the facts are folded and nothing is stored', state == {'hidden': True, 'expanded': 'false', 'stored': None}, state)
    click_el(b, '#nd-facts-fold')
    state = b.page("""(() => ({ hidden: document.getElementById('nd-facts-body').hidden,
      stored: localStorage.getItem('svolder.theme.nightdrive.facts'), firmware: !!document.querySelector('#nd-spec [data-fact=Firmware]'),
      rows: document.querySelectorAll('#nd-readings tr').length }))()""")
    check('the_facts_unfold_and_the_choice_survives_a_reload: the title unfolds the spec and the readings and the choice is kept',
          state['hidden'] is False and state['stored'] == '"open"' and state['firmware'] and state['rows'] > 3, state)
    b.goto(f'{d.url}?theme=nightdrive', settle=1.0)
    b.page(BOOT_THEME)
    sheet_open(b)
    check('  a reload finds them open', b.page("document.getElementById('nd-facts-body').hidden") is False)
    click_el(b, '#nd-facts-fold')
    check('  and folded again', b.page("document.getElementById('nd-facts-body').hidden") is True
          and b.page("localStorage.getItem('svolder.theme.nightdrive.facts')") == '"closed"')
    # the readings show their ages while the sheet is open
    click_el(b, '#nd-facts-fold')
    ages = b.page("[...document.querySelectorAll('#nd-readings td.nd-age')].map((e) => e.textContent).slice(0, 3)")
    check('  the readings say how long ago they were heard', len(ages) == 3 and all(a for a in ages), ages)
    check('  no page error', errors(b) == [], errors(b)[:3])


def identify_says_the_daemons_words(b, d):
    calm_start(b, d)
    sheet_open(b)
    if b.page("document.getElementById('nd-facts-body').hidden"):
        click_el(b, '#nd-facts-fold')
    b.page('window.__posts.length = 0')
    b.page("document.getElementById('nd-identify').scrollIntoView({ block: 'center' })")
    time.sleep(0.2)
    click_el(b, '#nd-identify')
    poll(lambda: [u for u, _, _ in b.page('window.__posts') if u.endswith('/identify')], 4.0)
    sent = [u for u, _, _ in b.page('window.__posts') if u.endswith('/identify')]
    toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"), 3.0)
    check('identify_says_the_daemons_words: Identify posts once and the popup is the core\'s words',
          len(sent) == 1 and toast and any('Identify sent' in t or 'wait' in t for t in toast), [sent, toast])
    click_in(b, '#nd-facts-fold')


def write_each_control(b, d, tag):
    """Every control the model of the shown device lists, through the sheet: a toggle flips, a choice writes its
    other option (a list through its list), a scale steps with the arrow, a readout has its words and writes nothing,
    and the device's power, its switches and its readouts the same; each write is one post to controls/<key> with the
    body the model gave (a relative path)."""
    m = d.model(DEVICE)
    seen = []

    def posted(key):
        return poll(lambda: [json.loads(body) for u, body, _ in b.page('window.__posts') if u == f'api/v1/devices/Gungnir%202/controls/{key}' and body], 4.0)

    for c in m['controls']:
        key = c['key']
        b.page('window.__posts.length = 0')
        if c['kind'] == 'toggle':
            click_in(b, f'#nd-t-{key}')
            got = posted(key)
            check(f'every_control_writes ({tag}): toggle {key} posts its flipped body', got == [json.loads(c['json'])], [got, c['json']])
        elif c['kind'] == 'choice':
            off = [o for o in c['options'] if not o.get('on')]
            if not off:
                check(f'every_control_writes ({tag}): choice {key} has another option to write', False, c)
                continue
            if c['key'] == 'input':
                if c.get('style') == 'list':
                    click_in(b, '#nd-input-list')
                    poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") > 0, 3.0)
                    click_in(b, f'.nd-opt[data-value="{off[0]["label"]}"]')
                else:
                    click_in(b, f'#nd-input [data-value="{off[0]["label"]}"]')
            elif c.get('style') == 'list':
                click_in(b, f'#nd-l-{key}')
                poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") > 0, 3.0)
                click_in(b, f'.nd-opt[data-value="{off[0]["label"]}"]')
            else:
                click_in(b, f'#nd-controls [data-key="{key}"][data-value="{off[0]["label"]}"]')
            got = posted(key)
            check(f'every_control_writes ({tag}): choice {key} writes its other option', got == [json.loads(off[0]['json'])], [got, off[0]['json']])
        elif c['kind'] == 'slider' and isinstance(c.get('raw'), int):
            b.page(f"(() => {{ const e = document.getElementById('nd-s-{key}'); e.scrollIntoView({{ block: 'center' }}); e.focus(); return 1; }})()")
            b.press('ArrowRight' if c['raw'] < c['hi'] else 'ArrowLeft')
            got = posted(key)
            want = c['raw'] + 1 if c['raw'] < c['hi'] else c['raw'] - 1
            check(f'every_control_writes ({tag}): scale {key} steps with the arrow', got and got[-1] == {'raw': want}, [got, want])
        elif c['kind'] == 'readout':
            has = b.page(f"!!document.querySelector('#nd-controls [data-readout=\"{key}\"]')")
            check(f'every_control_writes ({tag}): readout {key} is words', has is True)
        else:
            # a kind this walk does not handle (a slider the device has not read yet, a new kind) is a failure, not a pass
            check(f'every_control_writes ({tag}): control {key} ({c["kind"]}) is one the walk handles', False, c)
            continue
        seen.append(key)
        time.sleep(0.3)
    p = m.get('power')
    if p:
        b.page('window.__posts.length = 0')
        click_in(b, '#nd-power-key')
        got = posted(p['key'])
        check(f'every_control_writes ({tag}): the power key posts its body', got == [json.loads(p['json'])], [got, p['json']])
        for t in p.get('toggles', []):
            b.page('window.__posts.length = 0')
            click_in(b, f'#nd-pt-{t["key"]}')
            got = posted(t['key'])
            check(f'every_control_writes ({tag}): power switch {t["key"]} posts', got == [json.loads(t['json'])], [got, t['json']])
        for r in p.get('readouts', []):
            check(f'every_control_writes ({tag}): power readout {r["key"]} is words',
                  b.page(f"!!document.querySelector('#nd-controls [data-readout=\"{r['key']}\"]')") is True)
    return seen


def every_control_writes(b, d):
    """Each control of the fake DAC through the sheet, in the request log."""
    calm_start(b, d)
    sheet_open(b)
    before = d.model(DEVICE)
    seen = write_each_control(b, d, 'the fake DAC')
    check('every_control_writes: the DAC\'s whole list was walked', set(seen) == {c['key'] for c in before['controls']}, seen)
    bal = next(c for c in d.model(DEVICE)['controls'] if c['key'] == 'balance')
    b.page('window.__posts.length = 0')
    click_in(b, '#nd-ce-balance')
    got = poll(lambda: posts_json(b, 'controls/balance'), 4.0)
    check('  a tap on a scale\'s value writes the centre', got == [{'raw': bal['centre']}], got)
    b.page('window.__posts.length = 0')
    x0, y0, x1, y1 = b.page("(() => { const r = document.getElementById('nd-s-balance').getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; })()")
    if x1 > x0:
        b.click(x0 + (x1 - x0) * 0.25, (y0 + y1) / 2)
        got = poll(lambda: posts_json(b, 'controls/balance'), 4.0)
        want = round(bal['lo'] + 0.25 * (bal['hi'] - bal['lo']))
        check('  a press a quarter of the way along writes that step', got and abs(got[-1]['raw'] - want) <= 1, [got, want])
    check('  no page error', errors(b) == [], errors(b)[:3])


def a_slot_save_asks_first():
    two = common.Daemon('lokimax', host='fake')
    b = make()
    try:
        forget(b, two)
        load(b, two)
        time.sleep(0.8)
        b.page(REC)
        sheet_open(b)
        slots = b.page("[...document.querySelectorAll('#nd-slots [data-act=slot]')].map((e) => e.dataset.slot)")
        settings = b.page("[...document.querySelectorAll('#nd-slots [data-act=setting]')].map((e) => e.dataset.name)")
        check('a_loki_max_shows_its_slots_and_settings: three slots and the built-in settings are chips', slots == ['1', '2', '3'] and len(settings) >= 3, [slots, settings])
        click_in(b, '#nd-slots [data-slot="2"]')
        check('  a tap on a slot recalls it', poll(lambda: posts_json(b, 'device-presets/recall') == [{'slot': 2}], 4.0), posts_json(b, 'device-presets/recall'))
        click_in(b, f'#nd-slots [data-name="{settings[1]}"]')
        check('  a tap on a built-in setting applies it', poll(lambda: posts_json(b, 'device-presets/apply') == [{'name': settings[1]}], 4.0), posts_json(b, 'device-presets/apply'))
        b.page('window.__posts.length = 0')
        click_in(b, '#nd-slot-save')
        poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == 3, 3.0)
        click_el(b, '.nd-opt[data-value="3"]')
        asked = poll(lambda: b.page("!!document.querySelector('#nd-slots .nd-question')"), 3.0)
        check('a_slot_save_asks_first: choosing a slot asks in place and sends nothing yet',
              asked and posts_json(b, 'device-presets/save') == [] and b.page("document.activeElement.dataset.answer") == 'keep',
              posts_json(b, 'device-presets/save'))
        click_in(b, '#nd-slots [data-answer=keep]')
        time.sleep(0.4)
        check('  Keep writes nothing', posts_json(b, 'device-presets/save') == [])
        click_in(b, '#nd-slot-save')
        poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == 3, 3.0)
        click_el(b, '.nd-opt[data-value="3"]')
        poll(lambda: b.page("!!document.querySelector('#nd-slots .nd-question')"), 3.0)
        click_in(b, '#nd-slots [data-answer=save]')
        check('  Save posts the slot with the confirmation, only then', poll(lambda: posts_json(b, 'device-presets/save') == [{'slot': 3, 'confirm': True}], 4.0),
              posts_json(b, 'device-presets/save'))
        # a slider control of the Loki Max writes too
        b.page('window.__posts.length = 0')
        b.page("document.querySelector('#nd-controls .nd-scale').focus()")
        b.press('ArrowRight')
        got = poll(lambda: [u for u, _, _ in b.page('window.__posts') if '/controls/band' in u], 4.0)
        check('  a band level of the device writes through its scale', len(got) >= 1, got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, two)
        finally:
            b.close()
            two.stop()


def an_amp_shown_alone_keeps_the_music_and_its_gauges():
    amp = common.Daemon('aegir2', host='fake')
    b = make()
    try:
        forget(b, amp)
        load(b, amp)
        time.sleep(1.0)
        b.page(REC)
        got = b.page("""(() => ({ gauges: [...document.querySelectorAll('#nd-dash [data-gauge]')].map((e) => e.dataset.gauge),
          input: !!document.querySelector('#nd-dash [data-act=input]'), draws: __core.theme.driver.perf().draws,
          power: !!document.getElementById('nd-power') }))()""")
        check('an_amp_shown_alone_keeps_the_music_and_its_gauges: power gauge and key where the input would be, no input chip',
              'power' in got['gauges'] and got['power'] and not got['input'], got)
        time.sleep(0.6)
        check('  the scene goes on drawing', b.page(DRAWS) > got['draws'] + 5, [got['draws'], b.page(DRAWS)])
        x, y = common.center(b, '.nd-scene')
        b.click(x, y * 0.9)
        time.sleep(0.4)
        check('an_amp_never_morphs: over the mountains the scene does not turn to tuning', b.page("document.querySelector('.nd-scene').dataset.tune") in (None, 'off'),
              b.page("document.querySelector('.nd-scene').dataset.tune"))
        sheet_open(b)
        got = b.page("[...document.querySelectorAll('#nd-dialog section')].map((s) => s.id)")
        check('  its sheet has Controls and the facts and no band table', 'nd-controls' in got and 'nd-facts' in got and 'nd-bands' not in got, got)
        scales = b.page("[...document.querySelectorAll('#nd-meters .nd-mscale')].map((e) => [e.dataset.key, !!e.querySelector('.nd-needle'), !!e.querySelector('.nd-zone')])")
        check('an_amp_shows_its_meters_in_the_sheet: a scale with a needle and a shaded share for each channel', len(scales) == 2 and all(x[1] and x[2] for x in scales), scales)
        words = b.page("[...document.querySelectorAll('#nd-meters [data-meter]')].map((e) => e.textContent)")
        check('  and the reading\'s words', len(words) == 2 and all(words), words)
        b.page('window.__posts.length = 0')
        write_each_control(b, amp, 'the fake amp')
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, amp)
        finally:
            b.close()
            amp.stop()


def an_amps_meters_move_live_on_the_sheet():
    amp = common.Daemon('fake-amp', name='Amp', host='fake', env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make()
    try:
        forget(b, amp)
        load(b, amp)
        time.sleep(1.0)
        sheet_open(b)
        if not poll(lambda: b.page("!!document.querySelector('#nd-meters .nd-needle:not(.off)')"), 3.0):
            click_el(b, '#nd-power')
        check('an_amps_meters_move_live_on_the_sheet: the amp reads its meters',
              poll(lambda: b.page("!!document.querySelector('#nd-meters .nd-needle:not(.off)')"), 15.0))
        # the sheet stays as it is: the same needle element, moved in place by the stream's readings
        b.page("window.__needle = document.querySelector('#nd-meters .nd-needle')")
        seen = []
        end = time.time() + 3.0
        while time.time() < end:
            seen.append(b.page("document.querySelector('#nd-meters .nd-needle').dataset.at || ''"))
            time.sleep(0.15)
        moves = sum(1 for x, y in zip(seen, seen[1:]) if x != y)
        check('  a needle changes at least twice within 3 s with the sheet left open', moves >= 2, seen)
        check('  and it is the same needle, moved in place',
              b.page("window.__needle === document.querySelector('#nd-meters .nd-needle')"))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, amp)
        finally:
            b.close()
            amp.stop()


BOOT_THEME = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => { await m.mounted; window.__core = m.core; return m.core.theme.id; })"""


def the_sheet(b, d):
    the_sheet_opens(b, d)
    the_sheet_from_the_menu(b, d)
    the_sheet_closes(b, d)
    the_sheet_runs_dimmed(b, d)
    the_band_table(b, d)
    the_band_hold(b, d)
    the_mode_and_loudness(b, d)
    the_presets(b, d)
    the_facts_unfold_and_the_choice_survives_a_reload(b, d)
    identify_says_the_daemons_words(b, d)
    every_control_writes(b, d)
    the_sheet_holds(b, d)
    the_sheet_list()
    the_sheet_on_a_phone(b, d)
    the_sheet_input_chips()
    a_slot_save_asks_first()
    an_amp_shown_alone_keeps_the_music_and_its_gauges()
    an_amps_meters_move_live_on_the_sheet()
    the_sheet_guest()
    the_sheet_switch()

d = common.Daemon('gungnir2-v2', host='fake')
b = make()
try:
    the_sheet(b, d)
    check('no page error in the whole run', errors(b) == [], errors(b)[:3])
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
base.finish('nightdrive_sheet.py')
