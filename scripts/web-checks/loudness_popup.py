# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Sound popup's Loudness tab (core/popups/loudness/, the tab strip of core/popups/sound/) in a browser,
in every theme: what no node harness can see. The tab's logic is node's (node/core-loudness.mjs); the other
tabs' are crossfeed_popup.py's and eq_library.py's; popups.py holds the frame. The readout numbers (volume,
depth, bass, mids, treble) come from the controller's live read, which the fake host does not have yet, so
the page checks lay the worked example (35 % of a 100 % reference: depth 13.7, bass +7.3, mids -7.3, treble
+2.1) over the host model and answer `api/v1/host/loudness` in the page; `real_daemon` is the same tab over
the daemon's own view and route with no overlay.
Per theme:
  the_tabs_are_in_chain_order_and_loudness_is_second   Sound… opens on the EQ library tab; its tabs read EQ
                                        library, Loudness, Crossfeed, Virtual speakers; a real click on
                                        Loudness shows the tab (switch, slider, picture, the faint line of
                                        the EQ and the stage together) and no pills
  geometry_at_1280_and_390_per_tab       on the Loudness tab, wide and on a phone: nothing scrolls sideways,
                                        the whole tab strip is inside the window, every button and the
                                        slider's row at least 44 px, the page behind where it was
  the_readout_is_the_views_numbers       35 %, depth 13.7 dB, bass +7.3, mids -7.3, treble +2.1 as drawn
  a_drag_is_done_false_and_its_release_done_true   a real drag of the slider: requests with `done: false`, the
                                        last one `done: true`, the number drawn the view's
  the_keyboard_and_escape                Tab reaches the tabs, the switch, the slider and the reference
                                        buttons; an arrow on the slider is one `done: true` request; the
                                        arrows move between the tabs; Escape closes the popup, the focus back
                                        on the frame's button
  use_current_volume_and_the_switch      a click on the button asks `reference_now`; the switch turns it off,
                                        the page dims, the tab's dot goes
  the_colours_are_the_themes_tokens      the switch's `on` track, the curve and the faint line are the theme's
                                        `--frame-accent` and `--frame-graph-compare`
  reduced_motion                         with the preference on, nothing in the tab animates or transitions
  real_daemon                            the tab over the daemon's own view and route (no overlay): the tab is
                                        there, a key on the slider reaches the host, which snaps it, and what is
                                        drawn is the host's number; the switch ends in what the host holds
With WEB_CHECK_SHOTS=<folder> a picture of the tab, `<theme>-loudness-<wide|phone>-<on|off|ref>.png`, is written
there.
Usage: python3 loudness_popup.py chromium|firefox"""
import os
import sys
import time

import common
import modal
from modal import BODY, THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, rects

browser = modal.browser

# the host's view of the loudness, as the contract gives it (the worked example), and a fake host that applies
# an op; the computer's EQ is on with one point, so the faint line shows
FAKE = r"""(() => {
  const hz = []; for (let i = 0; i < 61; i++) hz.push(Math.round(20 * 10 ** (i / 20) * 100) / 100);
  const curve = (lift, mids, treble) => hz.map((f) => Math.round((mids + lift / (1 + (f / 150) ** 2)
    + treble * (1 - 1 / (1 + (f / 4000) ** 2))) * 100) / 100);
  const rg = (min, max, step) => ({ min, max, step });
  const x = window.__ld = { posts: [], view: { on: true, strength_pct: 35, reference_pct: 100, volume_pct: 35,
    depth_db: 13.7, lift_db: 7.3, mids_db: -7.3, treble_db: 2.1, following: true, note: null, unavailable: null,
    curve: { hz, db: curve(7.3, -7.3, 2.1) }, ranges: { strength_pct: rg(0, 100, 5), reference_pct: rg(1, 100, 1) } } };
  x.worked = { depth_db: 13.7, lift_db: 7.3, mids_db: -7.3, treble_db: 2.1, curve: x.view.curve };
  const real = window.fetch;
  const withView = (computer) => ({ ...computer, eq: { ...(computer?.eq ?? { max: 16 }), on: true, preamp_db: -3,
    points: [{ slot: 1, type: 'PK', f: 1000, g: 4, q: 1 }],
    response: Array.from({ length: 181 }, (_, i) => (i > 90 && i < 115 ? 4 : 0)), loudness: x.view } });
  const answer = (status, body) => Promise.resolve({ status, ok: status < 400, json: async () => body });
  window.fetch = (path, init) => {
    if (String(path) === 'api/v1/host/loudness') {
      const op = JSON.parse(init.body); x.posts.push(op);
      if (typeof op.strength_pct === 'number') x.view.strength_pct = Math.round(op.strength_pct / 5) * 5;
      if (typeof op.reference_pct === 'number') x.view.reference_pct = op.reference_pct;
      if (op.reference_now) x.view.reference_pct = 35;
      if (typeof op.on === 'boolean') {
        // the host plays no stage while it is off: its view has no depth and a flat curve
        x.view.on = op.on;
        const v = x.view;
        const w = x.worked;
        Object.assign(v, op.on ? w : { depth_db: 0, lift_db: 0, mids_db: 0, treble_db: 0 });
        v.curve = op.on ? w.curve : { hz, db: hz.map(() => 0) };
      }
      return answer(200, {});
    }
    if (String(path).startsWith('api/v1/host') && !String(path).includes('/', 12)) {
      return real(path, init).then(async (r) => { const j = await r.json(); return { status: r.status, ok: r.ok,
        json: async () => ({ ...j, computer: withView(j.computer) }) }; });
    }
    return real(path, init);
  };
  const emit = __core.model.emit;
  __core.model.emit = (event, data) => emit(event, event === 'host' && data?.computer ? { ...data, computer: withView(data.computer) } : data);
  __core.model.emit('host', __core.model.page().host);
  return 1; })()"""

TABS = "[...document.querySelectorAll('#popup-sound .fp-tab, #popup-sound .fp-pill')]"
GEOMETRY = """(() => { const w = innerWidth;
  const inside = (q) => [...document.querySelectorAll(q)].every((e) => { const r = e.getBoundingClientRect(); return r.left >= 0 && r.right <= w + 0.5; });
  const small = (q) => [...document.querySelectorAll(q)].map((e) => { const r = e.getBoundingClientRect(); return Math.min(r.width, r.height); });
  const rows = [...document.querySelectorAll('#popup-sound .fpl-row')].map((e) => e.getBoundingClientRect().height);
  const strip = document.querySelector('#popup-sound .fp-tabs'), on = document.querySelector('#popup-sound .fp-tab.on');
  const shown = !strip || !on || (on.getBoundingClientRect().left >= strip.getBoundingClientRect().left - 1
    && on.getBoundingClientRect().right <= strip.getBoundingClientRect().right + 1);
  return { bar: inside('#popup-sound .fp-tabs, #popup-sound .fp-pill') && shown, body: inside('#popup-sound .fp-page *'),
    steps: small('#popup-sound .fpx-step'), rows, tabs: small('#popup-sound .fp-tab'),
    button: document.getElementById('fx-button').getBoundingClientRect().toJSON(), scroll: scrollY }; })()"""
STATE = """(() => { const v = __core.model.page().host?.computer?.eq?.loudness; return v ? [v.on, v.strength_pct,
  v.reference_pct] : null; })()"""


def tab(b, which):
    """A real click on a tab."""
    b.click(*centre(b, f'#popup-sound-tab-{which}'))
    time.sleep(0.4)


def page_of(b):
    return b.page('__core.router.state().page')


def geometry(b, tag, state, before):
    """`before` is the page's geometry read before the popup was opened."""
    b.page(BODY)
    r = rects(b, 'sound')
    g = b.page(GEOMETRY)
    check(f'geometry_at_1280_and_390_per_tab {tag}: {state}: nothing scrolls sideways, the bar and the page '
          'are inside the window', r['over'] == [0, 0] and g['bar'] and g['body'], [r['over'], g['bar'], g['body']])
    check(f'geometry_at_1280_and_390_per_tab {tag}: {state}: the page behind is where it was',
          g['button'] == before['button'] and g['scroll'] == before['scroll'], [before['button'], g['button']])
    return g


def shot(b, theme, name):
    folder = os.environ.get('WEB_CHECK_SHOTS')
    if folder:
        time.sleep(0.5)
        with open(os.path.join(folder, f'{theme}-loudness-{name}.png'), 'wb') as out:
            out.write(b.shot())


def slider_box(b):
    return b.page("(() => { const r = document.getElementById('popup-sound-ld-strength').getBoundingClientRect();"
                  " return { x: r.left, y: r.top, w: r.width, h: r.height }; })()")


def case(b, theme):
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page(FAKE)
    time.sleep(0.4)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    behind = b.page(GEOMETRY)
    opened = by_the_menu(b, 'sound')
    names = b.page(f"{TABS}.map((e) => e.textContent.trim())")
    check(f'the_tabs_are_in_chain_order_and_loudness_is_second {tag}: Sound… opens on the EQ library tab, its '
          'tabs in chain order', opened is True and names == ['EQ library', 'Loudness', 'Crossfeed',
                                                              'Virtual speakers', 'AutoEq', 'Upload a file',
                                                              'Saved presets'], names)
    tab(b, 'ld')
    shown = [page_of(b), b.page("!!document.getElementById('popup-sound-ld-switch')"),
             b.page("!!document.getElementById('popup-sound-ld-strength')"),
             b.page("!!document.querySelector('#popup-sound .fpl-line')"),
             b.page("!!document.querySelector('#popup-sound .fpl-with')"),
             b.page("!!document.querySelector('#popup-sound .fp-pill')"),
             b.page("!!document.querySelector('#popup-sound-tab-ld .fp-dot.on')")]
    check(f'the_tabs_are_in_chain_order_and_loudness_is_second {tag}: a click on Loudness shows the tab, its '
          'switch, slider, curve and the faint combined line, the tab\'s dot and no pills',
          shown == ['loudness', True, True, True, True, False, True], shown)
    g = geometry(b, tag, 'wide, Loudness', behind)
    check(f'geometry_at_1280_and_390_per_tab {tag}: wide: every button and row is at least 44 px',
          len(g['steps']) == 2 and min(g['steps']) >= 44 and min(g['rows']) >= 44 and min(g['tabs']) >= 44,
          [g['steps'], g['rows'], g['tabs']])
    words = b.page("document.querySelector('#popup-sound .fpl').textContent.replace(/\\s+/g, ' ')")
    check(f'the_readout_is_the_views_numbers {tag}: 35 %, depth 13.7, bass +7.3, mids -7.3, treble +2.1',
          all(part in words for part in ['Volume now35 %', 'Depth13.7 dB', 'Bass+7.3 dB', 'Mids−7.3 dB',
                                         '+2.1 dB', 'following the volume']), words)
    shot(b, theme, 'wide-on')
    # a real drag of the slider
    box = slider_box(b)
    b.drag(box['x'] + box['w'] * 0.35, box['y'] + box['h'] / 2, box['x'] + box['w'] * 0.8, box['y'] + box['h'] / 2)
    poll(lambda: b.page('window.__ld.posts.some((p) => p.done === true)'), 3.0)
    time.sleep(0.5)
    posts = b.page('window.__ld.posts')
    value = b.page("document.getElementById('popup-sound-ld-strength-value').textContent")
    check(f'a_drag_is_done_false_and_its_release_done_true {tag}: a real drag sends done: false and ends with one '
          'done: true, and the number drawn is the view\'s',
          len(posts) >= 2 and all(p['done'] is False for p in posts[:-1]) and posts[-1]['done'] is True
          and all('strength_pct' in p for p in posts) and value == f"{b.page('window.__ld.view.strength_pct')} %",
          [posts, value])
    # use current volume, the switch
    b.page('(window.__ld.posts.length = 0, 1)')
    b.click(*centre(b, '#popup-sound-ld-now'))
    poll(lambda: b.page('window.__ld.posts.length') >= 1, 3.0)
    time.sleep(0.5)
    asked = [b.page('window.__ld.posts'), b.page("document.getElementById('popup-sound-ld-reference').textContent")]
    check(f'use_current_volume_and_the_switch {tag}: the button asks the host to take the volume, and the reference '
          'drawn is the host\'s', asked[0] == [{'reference_now': True, 'done': True}] and '35 %' in asked[1], asked)
    b.click(*centre(b, '#popup-sound-ld-switch'))
    poll(lambda: b.page('window.__ld.view.on') is False, 3.0)
    time.sleep(0.5)
    off = b.page("[document.getElementById('popup-sound-ld-switch').getAttribute('aria-checked'),"
                 " !!document.querySelector('#popup-sound .fpl-off'),"
                 " !!document.querySelector('#popup-sound-tab-ld .fp-dot')]")
    check(f'use_current_volume_and_the_switch {tag}: the switch turns it off, the page is dimmed and the tab has '
          'no dot', off == ['false', True, False], off)
    flat = b.page("document.querySelector('#popup-sound .fpl').textContent.replace(/\\s+/g, ' ')")
    check(f'use_current_volume_and_the_switch {tag}: off, the view is the real one: depth and the three gains read '
          'zero, as the stage plays nothing', all(part in flat for part in ['Depth0.0 dB', 'Bass0.0 dB', 'Mids0.0 dB'])
          and 'Depth13.7' not in flat, flat)
    shot(b, theme, 'wide-off')
    b.click(*centre(b, '#popup-sound-ld-switch'))
    time.sleep(0.5)
    # the colours are the theme's tokens
    colours = b.page("""(() => { const probe = (v) => { const e = document.createElement('i'); e.style.color = `var(${v})`;
        document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; };
      const stroke = (q) => getComputedStyle(document.querySelector(q)).stroke;
      const track = getComputedStyle(document.querySelector('.fpx-switch-track')).backgroundColor;
      return [track === probe('--frame-accent'), stroke('.fpl-line') === probe('--frame-accent'),
        stroke('.fpl-with') === probe('--frame-graph-compare')]; })()""")
    check(f'the_colours_are_the_themes_tokens {tag}: the switch, the curve and the faint line are the frame tokens',
          colours == [True, True, True], colours)
    # keyboard
    b.page("(document.getElementById('popup-sound-tab-ld').focus(), 1)")
    b.press('ArrowLeft')
    time.sleep(0.4)
    back = [page_of(b), b.page('document.activeElement.id')]
    b.press('ArrowRight')
    time.sleep(0.4)
    fwd = [page_of(b), b.page('document.activeElement.id')]
    b.press('ArrowRight')
    time.sleep(0.4)
    after = [page_of(b), b.page('document.activeElement.id')]
    b.press('ArrowLeft')
    time.sleep(0.4)
    check(f'the_keyboard_and_escape {tag}: the arrows move between the tabs in chain order',
          back == ['autoeq', 'popup-sound-tab-eq'] and fwd == ['loudness', 'popup-sound-tab-ld']
          and after == ['crossfeed', 'popup-sound-tab-xf'], [back, fwd, after])
    seen = []
    for _ in range(14):
        b.press('Tab')
        seen.append(b.page("document.activeElement.id || document.activeElement.className"))
    inside = b.page("document.activeElement.closest('.fp') !== null")
    check(f'the_keyboard_and_escape {tag}: Tab goes round the tabs, the switch, the slider and the reference '
          'buttons and stays in the popup', inside is True and 'popup-sound-ld-switch' in seen
          and 'popup-sound-ld-strength' in seen and 'popup-sound-ld-reference_pct-less' in seen, seen)
    b.page('(window.__ld.posts.length = 0, 1)')
    b.page("(document.getElementById('popup-sound-ld-strength').focus(), 1)")
    b.press('ArrowRight')
    poll(lambda: b.page('window.__ld.posts.length') >= 1, 3.0)
    time.sleep(0.5)
    keyed = b.page('window.__ld.posts')
    check(f'the_keyboard_and_escape {tag}: an arrow on the slider is exactly one request, with done: true, and '
          'the slider keeps the focus', len(keyed) == 1 and keyed[0]['done'] is True
          and keyed[0]['strength_pct'] > 35 and b.page('document.activeElement.id') == 'popup-sound-ld-strength',
          [keyed])
    b.press('Escape')
    gone = poll(lambda: b.page('!document.getElementById("popup-sound")'), 3.0)
    focus = b.page('document.activeElement.id')
    check(f'the_keyboard_and_escape {tag}: Escape closes the popup, the focus back on the frame\'s button',
          gone is True and focus == 'fx-button', [gone, focus])
    # a phone
    b.resize(*PHONE)
    time.sleep(0.5)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    behind = b.page(GEOMETRY)
    by_the_menu(b, 'sound')
    tab(b, 'ld')
    g = geometry(b, tag, 'phone, Loudness', behind)
    check(f'geometry_at_1280_and_390_per_tab {tag}: phone: every button and row is at least 44 px, the four tabs '
          'inside the window', len(g['steps']) == 2 and min(g['steps']) >= 44 and min(g['rows']) >= 44
          and min(g['tabs']) >= 44 and len(g['tabs']) == 4, [g['steps'], g['rows'], g['tabs']])
    wide = b.page("(() => { const s = document.querySelector('#popup-sound .fp-tabs');"
                  " return s.scrollWidth > s.clientWidth + 1; })()")
    check(f'geometry_at_1280_and_390_per_tab {tag}: phone: the four tabs scroll sideways in one line, '
          'except in Signal, which squeezes them', wide is (theme != 'signal'), wide)
    # the arrows move the shown tab and nothing else: the strip does not also scroll by the key's own action
    tab(b, 'eq')
    b.page("(document.getElementById('popup-sound-tab-eq').focus(), 1)")
    seen = []
    for key in ['ArrowRight'] * 4 + ['ArrowLeft']:
        b.press(key)
        time.sleep(0.4)
        seen.append(b.page("""(() => { const s = document.querySelector('#popup-sound .fp-tabs'), t = document.activeElement.getBoundingClientRect(), r = s.getBoundingClientRect();
          return [t.left >= r.left - 1 && t.right <= r.right + 1, document.documentElement.scrollWidth <= innerWidth]; })()"""))
    check(f'the_keyboard_and_escape {tag}: phone: after each arrow the focused tab is whole inside the strip and '
          'the page is no wider than the window', all(a and c for a, c in seen), seen)
    tab(b, 'ld')
    shot(b, theme, 'phone-on')
    b.click(*centre(b, '#popup-sound-ld-switch'))
    time.sleep(0.5)
    shot(b, theme, 'phone-off')
    b.click(*centre(b, '#popup-sound-ld-switch'))
    time.sleep(0.4)
    b.page('(() => { const v = window.__ld.view; v.volume_pct = 100; v.depth_db = 0; v.lift_db = 0;'
           ' v.mids_db = 0; v.treble_db = 0; v.curve.db = v.curve.db.map(() => 0);'
           ' __core.model.emit("host", __core.model.page().host); return 1; })()')
    time.sleep(0.5)
    flat = b.page("document.querySelector('#popup-sound .fpl-readout').textContent")
    check(f'the_readout_is_the_views_numbers {tag}: at the reference the readout says flat',
          'flat, nothing is cut' in flat, flat)
    shot(b, theme, 'phone-ref')
    close_popup(b, 'sound')
    b.resize(*DESK)
    time.sleep(0.4)


def motion(b, theme):
    """With reduced motion on, nothing of the tab animates or transitions."""
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page(FAKE)
    time.sleep(0.3)
    by_the_menu(b, 'sound')
    tab(b, 'ld')
    moving = b.page("""[...document.querySelectorAll('#popup-sound .fpl, #popup-sound .fpl *, #popup-sound .fpx-switch, #popup-sound .fpx-switch *')].filter((e) => { const s = getComputedStyle(e);
      return (s.animationName !== 'none' && parseFloat(s.animationDuration) > 0)
        || s.transitionDuration.split(',').some((d) => parseFloat(d) > 0); }).map((e) => e.className)""")
    check(f'reduced_motion {tag}: nothing in the Loudness tab animates or transitions', moving == [], moving)
    close_popup(b, 'sound')


def real(b, theme):
    """The same tab over the daemon's own route and view (no overlay): the tab is offered, a key on the slider
    reaches `api/v1/host/loudness`, the host snaps it and the page shows the host's number, the switch ends in
    what the host holds."""
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    have = poll(lambda: b.page(f"!!__core.model.page().host?.computer?.eq?.loudness"), 5.0)
    names = b.page(f"{TABS}.map((e) => e.textContent.trim()).slice(0, 4)")
    check(f'real_daemon {tag}: the host\'s own view offers the tab, second in the strip',
          have is True and names == ['EQ library', 'Loudness', 'Crossfeed', 'Virtual speakers'], [have, names])
    tab(b, 'ld')
    view = "__core.model.page().host?.computer?.eq?.loudness"
    drawn = b.page("[!!document.querySelector('#popup-sound .fpl-line'),"
                   " document.getElementById('popup-sound-ld-strength')?.value]")
    check(f'real_daemon {tag}: the host\'s curve and strength are drawn', drawn[0] is True and drawn[1] is not None,
          drawn)
    before = b.page(f"{view}.strength_pct")
    b.page("(document.getElementById('popup-sound-ld-strength').focus(), 1)")
    b.press('ArrowRight')
    held = poll(lambda: b.page(f"{view}.strength_pct") != before, 6.0)
    time.sleep(0.6)
    now = b.page(f"{view}.strength_pct")
    text = b.page("document.getElementById('popup-sound-ld-strength-value').textContent")
    check(f'real_daemon {tag}: a key on the slider reaches the host, which snaps it, and the page shows the '
          'host\'s number', held is True and now == before + 5 and text == f'{round(now)} %', [before, now, text])
    was = b.page(f"{view}.on")
    b.click(*centre(b, '#popup-sound-ld-switch'))
    time.sleep(1.0)
    state = b.page(f"[{view}.on, document.getElementById('popup-sound-ld-switch').getAttribute('aria-checked'),"
                   " document.getElementById('popup-sound-ld-words')?.textContent ?? null]")
    check(f'real_daemon {tag}: the switch flips what the host holds, and no refusal words show',
          state == [not was, str(not was).lower(), None], [was, state])
    close_popup(b, 'sound')


def main():
    """Every case, in every theme; exits 1 on a failure."""
    d = common.Daemon(host='fake')
    try:
        for theme in THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, d, theme)
                case(b, theme)
            finally:
                b.close()
            b = (common.Chromium(size=DESK, reduced=True) if browser == 'chromium'
                 else common.Firefox(size=DESK, reduced=True))
            try:
                b.preload(modal.HOOK)
                load(b, d, theme)
                motion(b, theme)
            finally:
                b.close()
            # the daemon's own state is kept between runs: each theme has a daemon of its own
            fresh = common.Daemon(host='fake')
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, fresh, theme)
                real(b, theme)
            finally:
                b.close()
                fresh.stop()
    finally:
        d.stop()
    print(f'loudness_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
