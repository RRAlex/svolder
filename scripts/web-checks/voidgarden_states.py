# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's states in a browser: a device out of reach (the page's mood, the card in
the middle with its words, Try now and the seconds, the dash out of reach but for the side switch
and Controls…), a page that lost its daemon (one quiet line, the dash out of reach, the stream back
brings it all again), a page reading again or resting (its words), a host with no audio capture
(the mood and the line that says why, the controls still work), a guest (the keys say why nothing
happens, `A` still cycles the panel), a refused write (the core's toast with the daemon's words) and
a card arriving while the dash is faded. Every case ends with no console error, warning, CSP violation or page error (a refused write's own status line is the browser's). The mood is the page's `data-mood`; what the scene does
with it is the look's (the look run's to change), so no case reads a pixel. The node harness
voidgarden-dash.mjs has the words and the markup. The cells are `none` (both 3D APIs off) and
`webgl2` (the stock browser with Still kept, asked for by the second argument; the default is `none`): the states are
the page's, not a tier's.
  out_of_reach_shows_the_card              the mood, the card's words and Try now, the seconds counting down, the dash out of reach but for the side switch and Controls…
  a_lost_stream_says_it_in_one_line        the mood and the line, the dash inert; the stream back brings the dash and the mood back
  reading_again_and_resting_say_their_words  the quiet line's words, the mood, the driver's state
  no_capture_says_why_and_works            SVOLDER_FAKE_CAPTURE=off: the mood and the line that says why; `+` still writes
  a_guest_taps_get_the_cores_words         a real guest: no card, a tap on `+` writes nothing and says why, `A` still cycles the panel
  a_refused_write_toasts                   the daemon refuses a rename onto a name taken and the core toasts its words
  a_card_while_faded_wakes_the_dash        faded, then the device drops: the page is up and the card on show, and the fade stays off
Usage: python3 voidgarden_states.py chromium|firefox [none,webgl2]"""
import re
import sys
import time

import common
import voidgarden_base as vb
from voidgarden_base import DEVICE, check, load, make, poll

CELLS = (sys.argv[2] if len(sys.argv) > 2 else 'none').split(',')
MOOD = "document.getElementById('voidgarden').dataset.mood"
LINE = "(() => { const e = document.querySelector('#vg-card .vg-line'); return e && !e.hidden ? e.textContent : ''; })()"
CARD = "(() => { const e = document.getElementById('vg-cardbox'); return e ? { title: e.querySelector('h2').textContent, text: e.innerText } : null; })()"
DIMMED = "document.getElementById('voidgarden').classList.contains('vg-dim')"
# which keys of the dash are out of reach (inert), by their data-act
INERT = """Object.fromEntries([...document.querySelectorAll('#vg-dash [data-act]')].map((e) => [e.dataset.act + (e.id ? ':' + e.id : ''), e.inert]))"""
# the browser's own network lines (a refused or dropped fetch) are not an error of the page's code
NETWORK = ('Failed to load resource', 'Failed to fetch', 'NetworkError', '422')


def tag(cell, name):
    return f'{name} ({cell})'


def fresh(cell, d, **kw):
    b = make(cell, still=cell != 'none', **kw)
    vb.forget(b, d)
    return b


def errors(b):
    return [e for e in vb.logs(b) if not any(w in e for w in NETWORK)]


def out_of_reach_shows_the_card(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = fresh(cell, d)
    try:
        load(b, d)
        time.sleep(1.0)
        check(tag(cell, 'out_of_reach_shows_the_card: before the drop the mood is ok and there is no card'),
              b.page(MOOD) == 'ok' and b.page(CARD) is None and b.page(LINE) == '', [b.page(MOOD), b.page(CARD)])
        d.drop()
        shown = poll(lambda: b.page(MOOD) == 'unreachable' and b.page(CARD), 35.0)
        time.sleep(1.0)
        card = b.page(CARD)
        held = b.page('__core.model.page().device.card')
        check('  the card says it is out of reach, in the model\'s own title', bool(shown) and card and held and card['title'] == held['title'], [card, held])
        check('  with Try now and the seconds to the next try', card and 'Try now' in card['text']
              and re.search(r'next try in \d+ s|trying again now', card['text']), card)
        secs = b.page("document.querySelector('.vg-retry').textContent")
        time.sleep(2.5)
        check('  the seconds count down on the page', b.page("document.querySelector('.vg-retry').textContent") != secs or secs == 'trying again now',
              [secs, b.page("document.querySelector('.vg-retry').textContent")])
        inert = b.page(INERT)
        out = {k for k, v in inert.items() if v}
        free = {k for k, v in inert.items() if not v}
        check('  the dash is out of reach but for the side switch and the scene\'s explore keys',
              {'volume-up:vg-vup', 'volume-down:vg-vdown', 'mute:vg-mute'} <= out
              and all(k.startswith(('side', 'explore')) for k in free) and any(k.startswith('explore') for k in free), inert)
        vb.open_menu(b)
        vb.click_el(b, '#menu-vg-controls')
        check('  Controls… (the menu\'s row) still opens the popup', poll(lambda: b.page('__core.router.state().popup') == 'controls', 3.0), b.page('__core.router.state()'))
        b.press('Escape')
        poll(lambda: b.page('__core.router.state().popup') in (None, ''), 3.0)
        b.page(vb.REC)
        vb.click_el(b, '#vg-try')
        sent = poll(lambda: [u for u, body, _ in b.page('window.__posts') if u.endswith('/connect')], 4.0)
        check('  Try now posts connect, to this device', len(sent) == 1 and 'Gungnir%202' in sent[0], sent)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def a_lost_stream_says_it_in_one_line(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        time.sleep(1.0)
        b.page("__core.model.emit('state', 'lost')")
        check(tag(cell, 'a_lost_stream_says_it_in_one_line: the mood is lost and the one quiet line says Reconnecting'),
              poll(lambda: b.page(MOOD) == 'lost', 4.0) and b.page(LINE) == 'Reconnecting to svolder…' and b.page(CARD) is None,
              [b.page(MOOD), b.page(LINE)])
        inert = b.page(INERT)
        check('  the dash\'s device keys are out of reach', inert['volume-up:vg-vup'] and inert['volume-down:vg-vdown'] and inert['mute:vg-mute'], inert)
        check('  the page says so to the driver', b.page(f'{vb.DRIVER}.state()')['lost'] is True, b.page(f'{vb.DRIVER}.state()'))
        b.page("__core.model.emit('state', 'open')")
        check('  the stream back: the mood is ok, the line gone, the dash live',
              poll(lambda: b.page(MOOD) == 'ok' and b.page(LINE) == '', 4.0) and not any(b.page(INERT).values()),
              [b.page(MOOD), b.page(LINE), b.page(INERT)])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


def reading_again_and_resting_say_their_words(cell, d):
    b = fresh(cell, d)
    try:
        load(b, d)
        time.sleep(1.0)
        b.page("__core.model.emit('state', 'waiting')")
        check(tag(cell, 'reading_again_and_resting_say_their_words: waiting: the mood is wait and the line says "reading the device again…"'),
              poll(lambda: b.page(MOOD) == 'wait', 4.0) and b.page(LINE) == 'reading the device again…', [b.page(MOOD), b.page(LINE)])
        b.page("__core.model.emit('state', 'open')")
        poll(lambda: b.page(MOOD) == 'ok', 4.0)
        # a page that is hidden really rests: the core's stream says so and the page says it in words
        b.page(vb.HIDE)
        check('  resting (the page hidden): the driver says it, the mood is wait and the line says why',
              poll(lambda: b.page(f'{vb.DRIVER}.state()')['resting'], 6.0) and poll(lambda: b.page(MOOD) == 'wait', 3.0)
              and b.page(LINE) == 'Paused while the page is hidden; it reads the device again when shown.',
              [b.page(f'{vb.DRIVER}.state()'), b.page(MOOD), b.page(LINE)])
        b.page(vb.SHOW)
        check('  shown again: no line, the mood ok, not resting',
              poll(lambda: b.page(MOOD) == 'ok' and b.page(LINE) == '' and not b.page(f'{vb.DRIVER}.state()')['resting'], 6.0),
              [b.page(MOOD), b.page(LINE), b.page(f'{vb.DRIVER}.state()')])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


def no_capture_says_why_and_works(cell):
    d = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
    b = fresh(cell, d)
    try:
        load(b, d)
        got = poll(lambda: b.page(MOOD) == 'nocap', 12.0)
        line = b.page(LINE)
        check(tag(cell, 'no_capture_says_why_and_works: the mood is nocap and the line says why'),
              got and line.startswith('no music analysis: ') and len(line) > 24, [b.page(MOOD), line])
        b.page(vb.REC)
        raw = b.page('__core.model.page().device.volume.raw')
        vb.click_el(b, '#vg-vup')
        check('  everything else works: `+` raises the volume', poll(lambda: d.model(DEVICE)['volume']['raw'] == raw + 1, 5.0),
              [raw, d.model(DEVICE)['volume']['raw']])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def a_guest_taps_get_the_cores_words(cell):
    g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = fresh(cell, g)
    try:
        load(b, g)
        time.sleep(1.0)
        check(tag(cell, 'a_guest_taps_get_the_cores_words: a real guest: the mood is ok and there is no card'),
              b.page('__core.model.page().viewer.guest') is True and b.page(MOOD) == 'ok' and b.page(CARD) is None, [b.page(MOOD)])
        b.page(vb.REC)
        vb.click_el(b, '#vg-vup')
        time.sleep(0.8)
        toasts = b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)")
        check('  a tap on `+` writes nothing and the core says why', vb.posts(b, '/volume') == [] and len(toasts) >= 1, [vb.posts(b), toasts])
        b.press('ArrowUp')
        time.sleep(0.6)
        check('  nor does ↑', vb.posts(b, '/volume') == [], vb.posts(b))
        b.press('a')
        time.sleep(0.3)
        check('  `A` still cycles the audio panel (it changes the view, not the device)',
              b.page("document.getElementById('vg-panel').dataset.mode") == 'spread', b.page("document.getElementById('vg-panel').dataset.mode"))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        g.stop()


def a_refused_write_toasts(cell, d):
    b = fresh(cell, d)
    try:
        d.post(DEVICE, 'presets', {'name': 'Taken A'})
        d.post(DEVICE, 'presets', {'name': 'Taken B'})
        names = lambda: [r['name'] for r in d.model(DEVICE)['presets']['rows']]
        poll(lambda: 'Taken B' in names(), 8.0)
        load(b, d)
        b.page(vb.REC)
        b.page("__core.actions.renamePreset('Taken A', 'Taken B')")
        toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"), 8.0)
        status, answer = d.post(DEVICE, 'presets/rename', {'from': 'Taken A', 'to': 'Taken B'})
        words = answer.get('error') if status >= 400 else None
        check(tag(cell, 'a_refused_write_toasts: the daemon refuses a rename onto a name taken and the core toasts its words'),
              bool(toast) and bool(words) and toast[-1] == words, [toast, words])
        check('  nothing was renamed and the page is still usable',
              vb.posts(b, 'presets/rename') != [] and 'Taken A' in names() and names().count('Taken B') == 1
              and b.page('__core.model.state()') == 'open' and b.page(MOOD) == 'ok', names())
        for name in ('Taken A', 'Taken B'):
            d.post(DEVICE, 'presets/delete', {'name': name})
        check('  no page error but the refused rename\'s status line', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


def a_card_while_faded_wakes_the_dash(cell):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = fresh(cell, d)
    try:
        b.page("localStorage.setItem('svolder.theme.voidgarden.fade-after', '2')")
        load(b, d)
        check(tag(cell, 'a_card_while_faded_wakes_the_dash: the page fades after the idle time'), poll(lambda: b.page(DIMMED), 8.0))
        d.drop()
        back = poll(lambda: not b.page(DIMMED) and b.page(MOOD) == 'unreachable' and b.page(CARD), 35.0)
        check('  the page is up again and the card is on show', bool(back), [b.page(DIMMED), b.page(MOOD)])
        time.sleep(4.0)
        check('  the fade stays off while the card lasts (4 s past the idle time)', not b.page(DIMMED) and b.page(CARD) is not None,
              [b.page(DIMMED), b.page(CARD)])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


d = common.Daemon('gungnir2-v2', host='fake')
try:
    for cell in CELLS:
        out_of_reach_shows_the_card(cell)
        a_lost_stream_says_it_in_one_line(cell, d)
        reading_again_and_resting_say_their_words(cell, d)
        no_capture_says_why_and_works(cell)
        a_guest_taps_get_the_cores_words(cell)
        a_refused_write_toasts(cell, d)
        a_card_while_faded_wakes_the_dash(cell)
finally:
    d.stop()
vb.done('voidgarden_states.py')
