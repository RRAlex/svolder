# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive's states in a browser: a device out of reach (the road stops, the sun greys, the mountains
flatten, the card in the middle with Try now and the seconds, the dash dimmed and out of reach), a page that lost its
daemon (everything freezes and fades, one quiet line), a page reading again or resting (its words, the scene still),
a host with no audio capture (the mountains take the tone curve and breathe, the floor rolls empty, the quiet line
says why), a guest (the scene in full, the controls quiet), a refused write (a toast) and a card arriving while the
top is faded. The node harness nightdrive-states.mjs has the mood and the card's markup.
  out_of_reach_stops_the_road_and_shows_the_card     the road's rows stop, the sun is grey, the mountains flat, the card centred; Try now posts `connect`
  a_lost_stream_freezes_and_fades_with_one_line      the rows stop, the veil dims the sun, one quiet line, the HUD and dash faded and out of reach
  reading_again_and_resting_say_their_words          the quiet line's words and the top line's; the scene still
  no_capture_breathes_the_curve_and_says_why         SVOLDER_FAKE_CAPTURE=off: the ridge follows the curve and breathes, the floor rolls empty, the line says why
  a_guest_sees_the_scene_with_quiet_controls         a real guest: the scene in full colour, the dash's keys quiet, no curve
  a_refused_write_toasts                             the device does not answer Try now: a toast says so
  a_card_while_faded_wakes_the_top                   faded, then the device drops: the top line and the card are back and the fade stays off
Usage: python3 nightdrive_states.py chromium|firefox"""
import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)

PERF = '__core.theme.driver.perf()'
TUNE = '__core.theme.driver.tune()'
# the scene's mood, as the page says it
MOOD = "document.getElementById('nightdrive').dataset.mood"
# the floor's count of rows born so far (the module is the one the page loaded)
ROWS = """import(new URL('assets/themes/nightdrive/driver.js', document.baseURI)).then((m) =>
  m.bound.scene ? m.bound.scene.floor.whole : -1)"""
NEWEST = """import(new URL('assets/themes/nightdrive/driver.js', document.baseURI)).then((m) => {
  const row = m.bound.scene && m.bound.scene.floor.row(0);
  return row ? [...row].map((v) => Math.round(v * 1000)) : null; })"""
# a pixel of a canvas, in CSS px of the scene
PIXEL = """((id, x, y) => { const c = document.getElementById(id);
  const k = c.width / c.clientWidth;
  return [...c.getContext('2d').getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data]; })"""
# the road canvas's topmost solid pixel in the column at x between y0 and y1 (the ridge's edge), null when none
EDGE = """((x, y0, y1) => { const c = document.getElementById('nd-road'); const k = c.width / c.clientWidth;
  const g = c.getContext('2d'); const col = g.getImageData(Math.round(x * k), Math.round(y0 * k), 1, Math.round((y1 - y0) * k)).data;
  for (let i = 0; i < col.length; i += 4) if (col[i + 3] >= 200) return y0 + (i / 4) / k;
  return null; })"""
GEO = """(() => { const r = (sel) => { const e = document.querySelector(sel); if (!e) return null;
    const b = e.getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; };
  const c = document.getElementById('nd-road');
  return { w: c.clientWidth, h: c.clientHeight, card: r('#nd-card'), scene: r('.nd-scene'), dash: r('#nd-dash') }; })()"""
OPACITY = "((sel) => +getComputedStyle(document.querySelector(sel)).opacity)"
QUIET = "[...document.querySelectorAll('.nd-quiet')].filter((e) => !e.hidden && e.textContent.trim()).map((e) => e.textContent.trim())"
DASH_INERT = "document.getElementById('nd-dash').inert"
FADE_KEY = "localStorage.setItem('svolder.theme.nightdrive.fade-after', '2')"
FADED = "document.getElementById('nightdrive').classList.contains('nd-faded')"
TOP = """(() => ({ right: document.querySelector('.nd-right').textContent,
  top: +getComputedStyle(document.querySelector('.nd-top')).opacity }))()"""


def rows_in(b, secs):
    """How many floor rows were born in `secs` seconds."""
    one = b.page(ROWS)
    time.sleep(secs)
    return b.page(ROWS) - one


def grey(px):
    """A pixel that is a grey (its channels near each other), not the sun's orange."""
    return max(px[:3]) - min(px[:3]) <= 40


def load_page(b, d, query='?theme=nightdrive', fade=False):
    forget(b, d)
    if fade:
        b.page(FADE_KEY)
    load(b, d, query)
    time.sleep(1.0)


def sun_pixel(b):
    """The sun's middle, a little under its top: [r, g, b, a] on the sky canvas, and the scene's size."""
    g = b.page(GEO)
    w, h = g['w'], g['h']
    H = h * 0.56
    return b.page(f'({PIXEL})("nd-sky", {w / 2}, {H - h * 0.04 - h * 0.12})'), g


def lowest_edge(b, g):
    """The highest the mountains reach (the smallest y of their edge) over a handful of columns."""
    w, h = g['w'], g['h']
    H = h * 0.56
    ys = [b.page(f'({EDGE})({w * f}, {H - 0.3 * h}, {H + 2})') for f in (0.06, 0.12, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)]
    return min(y for y in ys if y is not None) if any(y is not None for y in ys) else None, H


def the_card(d, b):
    """The device dropped: the road's rows stop, the sun is grey, the mountains lie flat, the card stands in the middle
    with its title, Try now and the seconds, the dash is dimmed and out of reach, and Try now posts `connect` to this device."""
    load_page(b, d)
    time.sleep(1.0)
    before_sun, g = sun_pixel(b)
    before_edge, H = lowest_edge(b, g)
    rolling = rows_in(b, 2.0)
    check('out_of_reach_stops_the_road_and_shows_the_card: before the drop the road rolls, the sun is orange and the mountains rise',
          rolling >= 3 and not grey(before_sun) and before_edge is not None and before_edge < H - 0.07 * g['h'],
          [rolling, before_sun, before_edge, H])
    d.drop()
    shown = poll(lambda: b.page("!!document.getElementById('nd-card')"), 35.0)
    time.sleep(1.5)
    g = b.page(GEO)
    check('  a card says it is out of reach, in the middle of the scene',
          shown and g['card'] is not None and abs((g['card'][0] + g['card'][2]) / 2 - g['w'] / 2) <= 2
          and abs((g['card'][1] + g['card'][3]) / 2 - g['scene'][3] / 2) <= 0.2 * g['h']
          and b.page(MOOD) == 'unreachable', [g, b.page(MOOD)])
    words = b.page("document.getElementById('nd-card').innerText")
    check('  with its words, Try now and the seconds to the next try',
          'out of reach' in words.lower() and 'Try now' in words and poll(lambda: __import__('re').search(r'next try in \d+ s', b.page("document.getElementById('nd-card').innerText")), 3.0),
          words)
    secs = b.page("document.querySelector('.nd-retry').textContent")
    time.sleep(2.5)
    check('  the seconds count down on the page', b.page("document.querySelector('.nd-retry').textContent") != secs or secs == 'trying again now',
          [secs, b.page("document.querySelector('.nd-retry').textContent")])
    check('  the road has stopped: no row born in 2.5 s', rows_in(b, 2.5) == 0, b.page(ROWS))
    sun, g = sun_pixel(b)
    edge, H = lowest_edge(b, g)
    check('  the sun is grey and the mountains lie flat (their edge within 6 % of the height of the horizon)',
          grey(sun) and edge is not None and edge > H - 0.06 * g['h'], [sun, edge, H])
    check('  the dash is dimmed and out of reach (inert)',
          b.page(DASH_INERT) is True and b.page(f'({OPACITY})("#nd-dash")') < 0.8, [b.page(DASH_INERT), b.page(f'({OPACITY})("#nd-dash")')])
    check('  no embers, wet road or haze in trouble: the perf readout says the wet road is not drawn and the look is dull',
          b.page("__core.theme.driver.mood().dull") is True and b.page("__core.theme.driver.mood().flat") is True, b.page('__core.theme.driver.mood()'))
    b.page(REC)
    click_el(b, '#nd-try')
    posts = poll(lambda: [u for u, body, _ in b.page('window.__posts') if u.endswith('/connect')], 4.0)
    check('  Try now posts connect, to this device', len(posts) == 1 and 'Gungnir%202' in posts[0], posts)
    check('  no page error', [e for e in errors(b) if not any(w in e for w in NETWORK)] == [], errors(b)[:3])


def the_lost_stream(d, b):
    """The page's stream lost: the road's rows stop, the veil dims the sun, one quiet line says it, the volume, the EQ line and
    the dash fade and the dash is out of reach; the stream back brings it all again."""
    load_page(b, d)
    time.sleep(1.0)
    lit, g = sun_pixel(b)
    lit_veil = b.page(f'({PIXEL})("nd-road", {g["w"] / 2}, {g["h"] * 0.1})')
    b.page("__core.model.emit('state', 'lost')")
    time.sleep(1.5)
    check('a_lost_stream_freezes_and_fades_with_one_line: the mood is lost and one quiet line says Reconnecting',
          b.page(MOOD) == 'lost' and b.page(QUIET) == ['Reconnecting to svolder…'] and b.page("!!document.getElementById('nd-card')") is False,
          [b.page(MOOD), b.page(QUIET)])
    check('  everything is frozen: no row born in 2.5 s, the draws slowed to a pace of 400 ms or more',
          rows_in(b, 2.5) == 0 and b.page(PERF)['pace'] >= 400, [b.page(ROWS), b.page(PERF)])
    sun, g = sun_pixel(b)
    veil = b.page(f'({PIXEL})("nd-road", {g["w"] / 2}, {g["h"] * 0.1})')
    check('  the sun is grey and a veil lies over the scene (the road canvas is 40 to 60 % opaque in the sky, clear before)',
          grey(sun) and 100 <= veil[3] <= 165 and lit_veil[3] == 0, [lit, sun, veil, lit_veil])
    faded = [b.page(f'({OPACITY})("{s}")') for s in ('.nd-vol', '.nd-eq', '#nd-dash')]
    check('  the volume, the EQ line and the dash fade, and the dash is out of reach',
          all(o < 0.6 for o in faded) and b.page(DASH_INERT) is True, [faded, b.page(DASH_INERT)])
    check('  the top line says reconnecting', b.page(TOP)['right'] == 'reconnecting', b.page(TOP))
    parts = "[...document.querySelectorAll('.nd-scene, #nd-dash, #nd-sheet-wrap, #nd-cardbox')].map((e) => e.inert)"
    words = "[...document.querySelectorAll('.nd-top, .nd-quiet')].map((e) => !e.closest('[inert]'))"
    check('  the page is inert but its words: the scene, the dash, the sheet and the card are, the top line and the quiet line are not (they stay read)',
          b.page(parts) == [True, True, True, True] and b.page(words) == [True, True], [b.page(parts), b.page(words)])
    b.page("__core.model.emit('state', 'open')")
    time.sleep(1.5)
    sun, g = sun_pixel(b)
    check('  the stream back: the mood is ok, the line gone, the dash live, the sun lit and the road rolling again',
          b.page(MOOD) == 'ok' and b.page(QUIET) == [] and b.page(DASH_INERT) is False and not grey(sun) and rows_in(b, 2.0) >= 3,
          [b.page(MOOD), b.page(QUIET), b.page(DASH_INERT), sun])
    check('  and nothing is inert again', b.page(parts) == [False, False, False, False], b.page(parts))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_waiting_page(d, b):
    """The page reading the device again, and resting: Signal's words on the quiet line, the top line's words, the scene still."""
    load_page(b, d)
    time.sleep(1.0)
    for state, line, top in [('waiting', 'reading the device again…', 'reading again'),
                             ('resting', 'Paused while the page is hidden; it reads the device again when shown.', 'resting')]:
        b.page(f"__core.model.emit('state', '{state}')")
        time.sleep(1.2)
        check(f'reading_again_and_resting_say_their_words: {state}: the quiet line says "{line[:24]}…" and the top line "{top}"',
              b.page(QUIET) == [line] and b.page(TOP)['right'] == top and b.page(MOOD) == 'wait', [b.page(QUIET), b.page(TOP), b.page(MOOD)])
        check('  and the scene is still: no row born in 2.5 s', rows_in(b, 2.5) == 0, b.page(ROWS))
    b.page("__core.model.emit('state', 'open')")
    time.sleep(1.2)
    check('  back to open: no line, the road rolls', b.page(QUIET) == [] and rows_in(b, 2.0) >= 3, [b.page(QUIET), b.page(ROWS)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_missing_capture():
    """A host that cannot capture audio (SVOLDER_FAKE_CAPTURE=off): the mountains take the tone curve's shape (every sample
    within the breathing's size of it) and breathe (their edge moves by about the breath's size over a few seconds),
    the floor rolls empty (rows all 0.05) and the quiet line says why; the controls still work."""
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
    b = make(size=(1280, 800))
    try:
        reset_bands(d)
        load_page(b, d)
        time.sleep(2.0)
        line = b.page(QUIET)
        check('no_capture_breathes_the_curve_and_says_why: the mood is nocap and the quiet line says why',
              b.page(MOOD) == 'nocap' and len(line) == 1 and line[0].startswith('no music analysis: ') and len(line[0]) > 24, [b.page(MOOD), line])
        g = b.page(GEO)
        w, h = g['w'], g['h']
        H = h * 0.56
        xs = [w * f for f in (0.1, 0.25, 0.4, 0.55, 0.7, 0.85)]
        amp = 0.006 * h
        follows, swings = [], {x: [] for x in xs}
        for _ in range(9):
            for x in xs:
                edge = b.page(f'({EDGE})({x}, {H - 0.3 * h}, {H + 2})')
                want = b.page(f'__core.theme.driver.curveAt({x})')
                if edge is not None and want is not None:
                    follows.append(abs(edge - want))
                    swings[x].append(edge)
            time.sleep(0.9)
        worst = max(follows) if follows else None
        moved = max((max(v) - min(v)) for v in swings.values() if v)
        check('  the ridge is the tone curve to within the breath (0.006 of the height) and a few pixels',
              worst is not None and worst <= amp + 4, [worst, amp])
        check('  and it breathes: the edge moves by at least 0.6 of the breath over 8 s, and no more than twice it',
              0.6 * amp <= moved <= 2.0 * amp + 3, [moved, amp])
        row = b.page(NEWEST)
        check('  the floor rolls empty: rows are born and every column is at 0.05',
              rows_in(b, 2.0) >= 3 and row is not None and set(row) == {50}, [row, b.page(ROWS)])
        before = d.model(DEVICE)['volume']['raw']
        click_el(b, '#nd-vdown')
        moved_vol = poll(lambda: d.model(DEVICE)['volume']['raw'] != before, 5.0)
        check('  everything else works: a tap on `−` lowers the volume', moved_vol, [before, d.model(DEVICE)['volume']['raw']])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_guest(b=None):
    """A real guest: the scene in full colour and the dash's keys quiet; a hover over the mountains brings no curve."""
    g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = make(size=(1280, 800))
    try:
        load_page(b, g)
        time.sleep(1.0)
        sun, geo = sun_pixel(b)
        quiet = b.page("[...document.querySelectorAll('#nd-dash [data-act=volume-up], #nd-dash [data-act=volume-down], #nd-dash [data-act=mute]')].map((e) => [e.classList.contains('quiet'), +getComputedStyle(e).opacity])")
        check('a_guest_sees_the_scene_with_quiet_controls: a real guest page: the sun lit, the mood ok, no card, the dash\'s keys half-lit',
              b.page('__core.model.page().viewer.guest') is True and b.page(MOOD) == 'ok' and not grey(sun)
              and b.page("!!document.getElementById('nd-card')") is False and b.page(DASH_INERT) is False
              and len(quiet) == 3 and all(q[0] and 0.4 <= q[1] <= 0.6 for q in quiet), [sun, b.page(MOOD), quiet])
        left, top = b.page("(() => { const r = document.querySelector('.nd-scene').getBoundingClientRect(); return [r.left, r.top]; })()")
        t = b.page(TUNE)
        if browser == 'chromium':
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': left + t['w'] / 2, 'y': top + t['H'] - 0.1 * t['h']})
        time.sleep(1.0)
        check('  the mountains never turn into the curve for a guest',
              b.page("document.querySelector('.nd-scene').dataset.tune !== 'on'") and b.page(TUNE)['m'] == 0, b.page(TUNE))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


def the_guest_without_capture():
    """No capture, one page, two viewers: for a signed-in viewer the mountains take the tone curve's shape; once the page's viewer is
    a guest (its model says so; the page's model is wrapped for this) they rest at the baseline (0 dB, 0.08 of the height over
    the horizon) and breathe, never turning into the curve."""
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
    b = make(size=(1280, 800))
    try:
        reset_bands(d)
        load_page(b, d)
        time.sleep(2.0)
        geo = b.page(GEO)
        w, h = geo['w'], geo['h']
        H = h * 0.56
        base = H - 0.08 * h
        amp = 0.006 * h
        xs = [w * f for f in (0.1, 0.25, 0.4, 0.55, 0.7, 0.85)]

        def reach():
            """How far the edge is from the baseline, and from the curve, at the worst of the columns."""
            from_base, from_curve = 0, 0
            for x in xs:
                edge = b.page(f'({EDGE})({x}, {H - 0.3 * h}, {H + 2})')
                want = b.page(f'__core.theme.driver.curveAt({x})')
                if edge is not None and want is not None:
                    from_base = max(from_base, abs(edge - base))
                    from_curve = max(from_curve, abs(edge - want))
            return from_base, from_curve
        signed = reach()
        b.page("""(() => { const m = __core.model, was = m.page.bind(m);
          m.page = () => ({ ...was(), viewer: { ...was().viewer, guest: true, writes: false } }); return 1; })()""")
        time.sleep(1.5)
        guest = reach()
        check('a_guest_sees_the_scene_with_quiet_controls: with no capture a signed-in viewer\'s mountains are the curve (within the breath), '
              'a guest\'s rest at the baseline within the breath, and the curve is further than that from the baseline',
              b.page(MOOD) == 'nocap' and signed[1] <= amp + 4 and signed[0] > amp + 8 and guest[0] <= amp + 4 and guest[1] > amp + 8,
              [signed, guest, amp])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_refused_write(d, b):
    """The daemon refuses a rename onto a name taken, asked of the core from Night drive's page: the core toasts the daemon's
    own words, nothing is renamed, and the page is still usable."""
    d.post(DEVICE, 'presets', {'name': 'Taken A'})
    d.post(DEVICE, 'presets', {'name': 'Taken B'})
    names = lambda: [r['name'] for r in d.model(DEVICE)['presets']['rows']]
    poll(lambda: 'Taken B' in names(), 8.0)
    load_page(b, d)
    b.page(REC)
    b.page("__core.actions.renamePreset('Taken A', 'Taken B')")
    toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"), 8.0)
    status, answer = d.post(DEVICE, 'presets/rename', {'from': 'Taken A', 'to': 'Taken B'})
    words = answer.get('error') if status >= 400 else None
    check('a_refused_write_toasts: the daemon refuses a rename onto a name taken and the core toasts its words',
          bool(toast) and bool(words) and toast[-1] == words, [toast, words])
    check('  the rename was posted and nothing was renamed, and the page is still usable',
          posts_to(b, 'presets/rename') != [] and 'Taken A' in names() and names().count('Taken B') == 1
          and b.page('__core.model.state()') == 'open' and b.page(MOOD) == 'ok', names())
    for name in ('Taken A', 'Taken B'):
        d.post(DEVICE, 'presets/delete', {'name': name})
    check('  no page error but the refused rename\'s status line', [e for e in errors(b) if '422' not in e and not any(w in e for w in NETWORK[:2])] == [], errors(b)[:3])
    # the refusal's own status line is not an error of the page's: the cases after this one start clean
    if browser == 'chromium':
        b.errors.clear()
    else:
        b.errors = []


def the_faded_top(d, b):
    """The top faded (after 2 s), then the device drops: the top line is back with its words, the card is on show and the
    fade stays off while it lasts."""
    load_page(b, d, fade=True)
    poll(lambda: b.page(FADED), 8.0)
    time.sleep(2.7)
    check('a_card_while_faded_wakes_the_top: faded before the device drops',
          b.page(FADED) and b.page(TOP)['top'] == 0, b.page(TOP))
    d.drop()
    back = poll(lambda: not b.page(FADED) and b.page(TOP)['top'] == 1 and b.page("!!document.getElementById('nd-card')"), 35.0)
    top = b.page(TOP)
    check('  the top line is back with its words and the card is on show', back and top['right'] != '' and b.page(MOOD) == 'unreachable', [top, b.page(MOOD)])
    time.sleep(4.0)
    check('  the fade stays off while the card lasts (4 s past the idle time)',
          not b.page(FADED) and b.page(TOP)['top'] == 1 and b.page("!!document.getElementById('nd-card')"), b.page(TOP))
    check('  no page error', [e for e in errors(b) if not any(w in e for w in NETWORK)] == [], errors(b)[:3])


d = common.Daemon('gungnir2-v2', host='fake')
b = make(size=(1280, 800))
try:
    the_lost_stream(d, b)
    the_refused_write(d, b)
    the_waiting_page(d, b)
    the_card(d, b)
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
d = common.Daemon('gungnir2-v2', host='fake')
b = make(size=(1280, 800))
try:
    the_faded_top(d, b)
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
the_missing_capture()
the_guest()
the_guest_without_capture()
base.finish('nightdrive_states.py')
