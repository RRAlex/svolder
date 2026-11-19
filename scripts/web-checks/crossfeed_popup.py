# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Sound popup (core/popups/sound/, its Crossfeed tab core/popups/crossfeed/) in a browser, in every
theme: what no node harness can see. The tab's logic is node's (node/core-crossfeed.mjs); the EQ
library tab's is eq_library.py's; popups.py holds the frame. The daemon's crossfeed is the page's
own: the check lays a crossfeed view over the host model and answers `api/v1/host/crossfeed` in the
page (as the host's contract says), so the check holds in a daemon that has the route and in one that
has not yet, and what is checked is the page.
Per theme:
  the_menu_row_opens_sound_and_the_tabs_switch   the menu's row `Sound…` opens it on the EQ library
                                        tab (its pills), a real click on Crossfeed shows the tab,
                                        the switch in the header and the picture, a click on EQ
                                        library goes back to the page it was on
  geometry_at_1280_and_390_per_tab       on each tab, wide and on a phone: no horizontal overflow
                                        of the page or the box, the tab strip and the pills
                                        inside the window, every stepper at least 44 px, the page
                                        behind where it was (the frame's button does not move,
                                        the page does not scroll)
  a_press_is_a_request_and_the_view_is_shown   a real click on Level's `+`: one request with the
                                        new amount and `done: true`, then the number drawn is
                                        the view's
  the_keyboard_and_escape                Tab reaches the tabs, the switch and the steppers; the
                                        arrows move between the tabs; Escape closes the open menu
                                        first, then the popup, with the focus back on the frame's
                                        button
  the_colours_are_the_themes_tokens      the switch's `on` track and the picture's curves are the
                                        theme's `--frame-accent` and `--frame-ink`
  real_daemon                            the tab over the daemon's own view and route (no overlay): the
                                        host's models and picture are drawn, Level + reaches the
                                        host and the number shown is the one it snapped and streamed,
                                        the switch ends in what it holds, Mid/Side bass reaches it
  a_guest_has_no_row_and_no_popup        on a guest's daemon: no Sound row, an open by the router
                                        draws nothing
With WEB_CHECK_SHOTS=<folder> a picture of the tab, `<theme>-crossfeed-<wide|phone>-<on|off>.png and `<theme>-crossfeed-wide-eq-library.png``, is
written there.
Usage: python3 crossfeed_popup.py chromium|firefox"""
import os
import sys
import time

import common
import modal
from modal import BODY, THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, rects

browser = modal.browser

# the host's view of the crossfeed, as the contract gives it, and the fake host that applies an op
FAKE = r"""(() => {
  const curve = () => { const hz = [], same = [], other = [];
    for (let i = 0; i < 61; i++) { const f = Math.round(20 * 10 ** (i / 20) * 100) / 100; hz.push(f);
      same.push(-0.2 * Math.log10(f / 20)); other.push(-8.6 - 12 * Math.log10(f / 20)); }
    return { hz, same_db: same, other_db: other }; };
  const rg = (min, max, step) => ({ min, max, step });
  const models = [
    { word: 'bauer', name: 'Bauer', description: 'Feeds a low-passed, delayed copy of each channel into the other channel at a set level.',
      presets: [{ name: 'Chu Moy', crossover_hz: 700, amount_pct: 33 }, { name: 'bs2b default', crossover_hz: 700, amount_pct: 37 }],
      default_preset: 'bs2b default',
      crossover_hz: rg(300, 2000, 10), amount_pct: rg(5, 50, 1), delay_max_ms: 1, delay_step_ms: 0.02, delay_settable: true },
    { word: 'mid-side', name: 'Mid/Side bass', description: 'Blends the bass toward mono.',
      presets: [{ name: 'Mono bass', crossover_hz: 120, amount_pct: 50 }], default_preset: 'Mono bass',
      crossover_hz: rg(60, 300, 5), amount_pct: rg(5, 50, 1), delay_max_ms: 0, delay_step_ms: 0, delay_settable: false }];
  const x = window.__xf = { posts: [], on: true,
    view: { on: true, model: 'bauer', preset: 'bs2b default', edited_from: null, amount_pct: 37.33, amount_db: -8.6,
      feed_db: 4.5, crossover_hz: 700, delay_ms: 0.265, delay_floor_ms: 0.265, delay_settable: true, delay_own: true,
      output: 'Gungnir 2', note: null, off_reason: null, unavailable: null, curve: curve(), models } };
  const real = window.fetch;
  const withView = (computer) => ({ ...computer, eq: { ...(computer?.eq ?? { on: true, points: [], max: 16 }), crossfeed: x.view } });
  const answer = (status, body) => Promise.resolve({ status, ok: status < 400, json: async () => body });
  window.fetch = (path, init) => {
    if (String(path) === 'api/v1/host/crossfeed') {
      const op = JSON.parse(init.body); x.posts.push(op);
      if (typeof op.amount_pct === 'number') Object.assign(x.view, { amount_pct: op.amount_pct, preset: null, edited_from: 'bs2b default', amount_db: -8.2 });
      if (typeof op.on === 'boolean') x.view.on = op.on;
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
  const steps = [...document.querySelectorAll('#popup-sound .fpx-step')].map((e) => { const r = e.getBoundingClientRect(); return Math.min(r.width, r.height); });
  const strip = document.querySelector('#popup-sound .fp-tabs'), on = document.querySelector('#popup-sound .fp-tab.on');
  const shown = !strip || !on || (on.getBoundingClientRect().left >= strip.getBoundingClientRect().left - 1
    && on.getBoundingClientRect().right <= strip.getBoundingClientRect().right + 1);
  return { bar: inside('#popup-sound .fp-tabs, #popup-sound .fp-pill') && shown, body: inside('#popup-sound .fp-page *'),
    steps, button: document.getElementById('fx-button').getBoundingClientRect().toJSON(), scroll: scrollY }; })()"""


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
        with open(os.path.join(folder, f'{theme}-crossfeed-{name}.png'), 'wb') as out:
            out.write(b.shot())


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
    pills = b.page(f"{TABS}.map((e) => e.textContent.trim())")
    check(f'the_menu_row_opens_sound_and_the_tabs_switch {tag}: Sound… opens on the EQ library tab',
          opened is True and pills == ['EQ library', 'Loudness', 'Crossfeed', 'Virtual speakers', 'AutoEq', 'Upload a file',
                    'Saved presets'], pills)
    geometry(b, tag, 'wide, EQ library', behind)
    shot(b, theme, 'wide-eq-library')
    tab(b, 'xf')
    shown = [page_of(b), b.page("!!document.getElementById('popup-sound-xf-switch')"),
             b.page("!!document.querySelector('#popup-sound .fpx-svg')"),
             b.page("!!document.querySelector('#popup-sound .fp-pill')")]
    check(f'the_menu_row_opens_sound_and_the_tabs_switch {tag}: a click on Crossfeed shows the tab, its '
          'switch and its picture, and no pills', shown == ['crossfeed', True, True, False], shown)
    g = geometry(b, tag, 'wide, Crossfeed', behind)
    check(f'geometry_at_1280_and_390_per_tab {tag}: wide: every stepper is at least 44 px',
          len(g['steps']) == 6 and min(g['steps']) >= 44, g['steps'])
    shot(b, theme, 'wide-on')
    # the switch, the stepper, the numbers
    b.click(*centre(b, '#popup-sound-xf-amount_pct-more'))
    poll(lambda: b.page('window.__xf.posts.length') >= 1, 3.0)
    time.sleep(0.5)
    posts = b.page('window.__xf.posts')
    amount = b.page("document.getElementById('popup-sound-xf-amount').textContent")
    check(f'a_press_is_a_request_and_the_view_is_shown {tag}: Level + is one request with done: true, and '
          'the number drawn is the view\'s', posts == [{'amount_pct': 38.33, 'done': True}] and '38 %' in amount
          and '−8.2 dB' in amount, [posts, amount])
    b.click(*centre(b, '#popup-sound-xf-switch'))
    poll(lambda: b.page('window.__xf.view.on') is False, 3.0)
    time.sleep(0.5)
    off = b.page("[document.getElementById('popup-sound-xf-switch').getAttribute('aria-checked'),"
                 " !!document.querySelector('#popup-sound .fpx-off')]")
    check(f'a_press_is_a_request_and_the_view_is_shown {tag}: the switch turns it off and the page is '
          'dimmed, still editable', off == ['false', True], off)
    shot(b, theme, 'wide-off')
    b.click(*centre(b, '#popup-sound-xf-switch'))
    time.sleep(0.5)
    # the colours are the theme's tokens
    colours = b.page("""(() => { const probe = (v) => { const e = document.createElement('i'); e.style.color = `var(${v})`;
        document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; };
      const stroke = (q) => getComputedStyle(document.querySelector(q)).stroke;
      const track = getComputedStyle(document.querySelector('.fpx-switch-track')).backgroundColor;
      return [track === probe('--frame-accent'), stroke('.fpx-same') === probe('--frame-ink'),
        stroke('.fpx-other') === probe('--frame-accent')]; })()""")
    check(f'the_colours_are_the_themes_tokens {tag}: the switch and the two curves are the frame tokens',
          colours == [True, True, True], colours)
    # keyboard: the tabs by the arrows, Tab inside, Escape closes the menu then the popup
    b.page("(document.getElementById('popup-sound-tab-xf').focus(), 1)")
    b.press('ArrowLeft')
    time.sleep(0.4)
    back = [page_of(b), b.page('document.activeElement.id')]
    b.press('ArrowRight')
    time.sleep(0.4)
    fwd = [page_of(b), b.page('document.activeElement.id')]
    check(f'the_keyboard_and_escape {tag}: the arrows move between the tabs',
          back == ['loudness', 'popup-sound-tab-ld'] and fwd == ['crossfeed', 'popup-sound-tab-xf'], [back, fwd])
    seen = []
    for _ in range(14):
        b.press('Tab')
        seen.append(b.page("document.activeElement.id || document.activeElement.className"))
    inside = b.page("document.activeElement.closest('.fp') !== null")
    check(f'the_keyboard_and_escape {tag}: Tab goes round the tabs, the switch, the menus and the steppers '
          'and stays in the popup', inside is True and 'popup-sound-xf-switch' in seen
          and 'popup-sound-xf-model' in seen and 'popup-sound-xf-delay_ms-more' in seen, seen)
    b.page("(document.getElementById('popup-sound-xf-model').focus(), 1)")
    b.press('Enter')
    time.sleep(0.4)
    listed = b.page("!!document.querySelector('#popup-sound .fi-list')")
    b.press('Escape')
    time.sleep(0.4)
    still = b.page("!!document.getElementById('popup-sound') && !document.querySelector('#popup-sound .fi-list')")
    b.press('Escape')
    gone = poll(lambda: b.page('!document.getElementById("popup-sound")'), 3.0)
    focus = b.page('document.activeElement.id')
    check(f'the_keyboard_and_escape {tag}: Escape closes the open menu first, then the popup, the focus back '
          'on the frame\'s button', listed is True and still is True and gone is True and focus == 'fx-button',
          [listed, still, gone, focus])
    # a phone
    b.resize(*PHONE)
    time.sleep(0.5)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    behind = b.page(GEOMETRY)
    by_the_menu(b, 'sound')
    geometry(b, tag, 'phone, EQ library', behind)
    tab(b, 'xf')
    g = geometry(b, tag, 'phone, Crossfeed', behind)
    check(f'geometry_at_1280_and_390_per_tab {tag}: phone: every stepper is at least 44 px',
          len(g['steps']) == 6 and min(g['steps']) >= 44, g['steps'])
    shot(b, theme, 'phone-on')
    b.click(*centre(b, '#popup-sound-xf-switch'))
    time.sleep(0.5)
    shot(b, theme, 'phone-off')
    b.click(*centre(b, '#popup-sound-xf-switch'))
    time.sleep(0.4)
    close_popup(b, 'sound')
    b.resize(*DESK)
    time.sleep(0.4)


def real(b, theme):
    """The same tab over the daemon's own route and view (no overlay): the view has the host's
    models and curve, a press on Level + reaches `api/v1/host/crossfeed` and the number drawn is the
    one the host snapped and streamed back, the switch ends in what the host holds (on, or off with the
    refusal in the tab's own words), and a hold ends with the host's value."""
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    tab(b, 'xf')
    view = "__core.model.page().host?.computer?.eq?.crossfeed"
    have = poll(lambda: b.page(f"!!{view}"), 5.0)
    drawn = b.page("[!!document.querySelector('#popup-sound .fpx-svg'),"
                   " document.getElementById('popup-sound-xf-model')?.textContent]")
    check(f'real_daemon {tag}: the host\'s own view is drawn (model, picture)',
          have is True and drawn[0] is True and drawn[1] == 'Bauer', [have, drawn])
    before = b.page(f"{view}.amount_pct")
    b.click(*centre(b, '#popup-sound-xf-amount_pct-more'))
    held = poll(lambda: b.page(f"{view}.amount_pct") != before, 6.0)
    time.sleep(0.6)
    now = b.page(f"{view}.amount_pct")
    text = b.page("document.getElementById('popup-sound-xf-amount').textContent")
    check(f'real_daemon {tag}: Level + reaches the host, which snaps it, and the page shows the host\'s number',
          held is True and now == round(before + 1) and f'{round(now)} %' in text
          and b.page("document.getElementById('popup-sound-xf-preset').textContent") == 'Custom',
          [before, now, text])
    b.click(*centre(b, '#popup-sound-xf-switch'))
    time.sleep(1.0)
    state = b.page(f"[{view}.on, document.getElementById('popup-sound-xf-switch').getAttribute('aria-checked'),"
                   " document.getElementById('popup-sound-xf-words')?.textContent ?? null]")
    check(f'real_daemon {tag}: the switch ends in what the host holds, a refusal in the tab\'s words',
          str(state[0]).lower() == state[1] and (state[0] is True or state[2]), state)
    b.click(*centre(b, '#popup-sound-xf-model'))
    time.sleep(0.3)
    picked = b.page("(() => { const o = [...document.querySelectorAll('#popup-sound .fi-opt')]"
                    ".find((e) => e.textContent.trim() === 'Mid/Side bass'); o?.click(); return !!o; })()")
    check(f'real_daemon {tag}: the model list offers Mid/Side bass', picked is True, picked)
    time.sleep(1.0)
    got = b.page(f"[{view}.model, document.getElementById('popup-sound-xf-delay').textContent]")
    check(f'real_daemon {tag}: picking Mid/Side bass reaches the host, and its delay reads none',
          got[0] == 'mid-side' and got[1].strip() == 'none', got)
    close_popup(b, 'sound')


def no_way_in(b, theme):
    tag = f'a_guest_has_no_row_and_no_popup ({theme})'
    b.page('(__core.frame.openMenu(), 1)')
    time.sleep(0.3)
    rows = b.page("[...document.querySelectorAll('#fx-menu [id^=\"menu-popup-\"]')].map((e) => e.id)")
    b.page('(__core.frame.closeMenu(), 1)')
    b.page("(__core.router.open('sound', 'crossfeed'), 1)")
    time.sleep(0.5)
    check(f'{tag}: no Sound row, and an open by the router draws nothing',
          rows == ['menu-popup-devices', 'menu-popup-setup', 'menu-popup-help']
          and b.page("!document.getElementById('popup-sound')") is True, rows)


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
    guest = common.Daemon(host='fake', web='guest = "read"')
    try:
        for theme in THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, guest, theme)
                b.page(BODY)
                no_way_in(b, theme)
            finally:
                b.close()
    finally:
        guest.stop()
    print(f'crossfeed_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
