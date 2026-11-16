# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Sound popup's Virtual speakers tab (core/popups/speakers/) in a browser, in every theme: what no
node harness can see. The tab's logic is node's (node/core-speakers.mjs); crossfeed_popup.py holds the
other tab and the frame. The daemon's speakers view and the sets' routes are laid over the page by the
check (as the daemon's contract says), so the layout is checked with all three views' timing present;
`real_daemon` then checks the tab over the fake host's own view (two synthetic sets in the daemon's
store), where the timing is whatever the daemon gives (absent: the panels say so).
Per theme:
  three_views_side_by_side_at_1280   the three panels are side by side (same top, none overlapping),
                                        the chips are hidden, nothing scrolls sideways
  one_view_and_chips_at_390          on a phone one panel is shown with the chips, a click on a chip
                                        shows another panel, and it is the one shown after a reopen
  the_switch_a_set_and_a_fetch       real clicks: the switch is one request, `Use` another, a fetch
                                        runs (Fetching…, Cancel) and Cancel aborts it
  the_colours_are_the_themes_tokens  the picture's bar and curves are `--frame-accent` and `--frame-ink`
  the_keyboard_and_escape            arrows on the chips, Escape closes the popup, the focus back
  real_daemon                        the tab over the fake host: the sets are listed as fetched, Use
                                        reaches the host and the view follows, the switch ends in on
With WEB_CHECK_SHOTS=<folder> a picture, `<theme>-speakers-<wide|phone>-<on|off>.png`, is written there.
Usage: python3 speakers_popup.py chromium|firefox"""
import os
import sys
import time

import common
import modal
from modal import BODY, THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, rects

browser = modal.browser

FAKE = r"""(() => {
  const hz = []; for (let i = 0; i < 61; i++) hz.push(Math.round(20 * 10 ** (i / 20) * 100) / 100);
  const curve = { hz, ll_db: hz.map((f) => -1 - 0.5 * Math.log10(f / 20)), lr_db: hz.map((f) => -7 - 3 * Math.log10(f / 20)),
    rl_db: hz.map((f) => -7.5 - 3 * Math.log10(f / 20)), rr_db: hz.map((f) => -1.2 - 0.5 * Math.log10(f / 20)) };
  const many = (shift) => Array.from({ length: 14 }, (_, i) => ({ ms: i < 2 ? i * 0.25 : 1 + i * 1.8 + shift, db: -i * 2.1 }));
  const timing = { arrivals: { left: many(0), right: many(0.3),
      tail: { times_ms: [1, 10, 25, 50], left: [-18, -30, -45, -60], right: [-19, -32, -47, -60] } },
    decay: { times_ms: [0.1, 1, 10, 100, 500], low: [0, -3, -12, -35, -60], mid: [0, -5, -20, -50, -60], high: [0, -8, -30, -60, -60], end_ms: 500 } };
  const row = (id, room, label, fetched, bytes) => ({ id, room, room_name: room === 'with' ? 'With room' : 'No room', term: room === 'with' ? 'BRIR' : 'HRTF', label, licence: 'LicenseRef-synthetic', credit: '', source_url: 'https://example.test/',
    bytes, own: false, fetched });
  const rows = [row('fake-no-room', 'no', 'Synthetic, no room', true, 36591729), row('fake-with-room', 'with', 'Synthetic, with room', true, 8359449),
    row('hutubs-pp1', 'no', 'HUTUBS, subject pp1', false, 1659720)];
  const view = (set, on) => { const one = rows.find((r) => r.id === set);
    return { on, set, room: one.room, room_name: one.room === 'with' ? 'With room' : 'No room', term: one.room === 'with' ? 'BRIR' : 'HRTF',
      label: one.label, licence: 'LicenseRef-synthetic', credit: '', source_url: 'https://example.test/',
      angles: 'left 45.0 deg, right 315.0 deg, 1.5 m', taps_ms: 500, loudness_db: -7.2, cut: true, note: null, off_reason: null,
      unavailable: null, curve, timing, sets: rows.filter((r) => r.fetched).map((r) => ({ id: r.id, room: r.room, room_name: r.room === 'with' ? 'With room' : 'No room',
        term: r.room === 'with' ? 'BRIR' : 'HRTF', label: r.label, licence: r.licence, credit: r.credit, source_url: r.source_url })) }; };
  const s = window.__sp = { posts: [], fetches: [], aborted: 0, view: view('fake-with-room', true) };
  const real = window.fetch;
  const withView = (computer) => ({ ...computer, eq: { ...(computer?.eq ?? { on: true, points: [], max: 16 }), speakers: s.view } });
  const answer = (status, body) => Promise.resolve({ status, ok: status < 400, json: async () => body });
  window.fetch = (path, init) => {
    const p = String(path);
    if (p === 'api/v1/host/speakers') {
      const op = JSON.parse(init.body); s.posts.push(op);
      if (op.set) s.view = view(op.set, op.on ?? s.view.on);
      if (typeof op.on === 'boolean') s.view = { ...s.view, on: op.on };
      return answer(200, {});
    }
    if (p === 'api/v1/speakers/catalogue') return answer(200, { sets: rows, fetching: true, max_sets: 32 });
    if (p === 'api/v1/speakers/fetch') {
      s.fetches.push(JSON.parse(init.body));
      return new Promise((resolve, reject) => init.signal.addEventListener('abort', () => { s.aborted++; reject(new Error('aborted')); }));
    }
    if (p.startsWith('api/v1/host') && !p.includes('/', 12)) {
      return real(path, init).then(async (r) => { const j = await r.json(); return { status: r.status, ok: r.ok,
        json: async () => ({ ...j, computer: withView(j.computer) }) }; });
    }
    return real(path, init);
  };
  const emit = __core.model.emit;
  __core.model.emit = (event, data) => emit(event, event === 'host' && data?.computer ? { ...data, computer: withView(data.computer) } : data);
  __core.model.emit('host', __core.model.page().host);
  return 1; })()"""

PANELS = ("[...document.querySelectorAll('#popup-sound .fps-panel')].map((e) => { const r = e.getBoundingClientRect();"
          " return { top: Math.round(r.top), left: Math.round(r.left), right: Math.round(r.right), w: Math.round(r.width) }; })")


def tab(b, which):
    b.click(*centre(b, f'#popup-sound-tab-{which}'))
    time.sleep(0.5)


def press(b, selector):
    """A real click on the element, scrolled into view first (the sets are below the pictures)."""
    b.page(f"(document.querySelector({selector!r}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.2)
    b.click(*centre(b, selector))


def shot(b, theme, name):
    folder = os.environ.get('WEB_CHECK_SHOTS')
    if folder:
        time.sleep(0.5)
        with open(os.path.join(folder, f'{theme}-speakers-{name}.png'), 'wb') as out:
            out.write(b.shot())


def sideways(b, tag, state):
    b.page(BODY)
    r = rects(b, 'sound')
    check(f'{tag}: {state}: nothing scrolls sideways', r['over'] == [0, 0], r['over'])


def case(b, theme):
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page(FAKE)
    time.sleep(0.4)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    tab(b, 'sp')
    panels = b.page(PANELS)
    chips = b.page("getComputedStyle(document.querySelector('#popup-sound .fps-chips')).display")
    row = len(panels) == 3 and len({p['top'] for p in panels}) == 1 and all(p['w'] > 250 for p in panels) \
        and panels[0]['right'] <= panels[1]['left'] and panels[1]['right'] <= panels[2]['left']
    check(f'three_views_side_by_side_at_1280 {tag}: three panels in a row, no chips', row and chips == 'none', [panels, chips])
    check(f'three_views_side_by_side_at_1280 {tag}: every panel draws its picture',
          b.page("document.querySelectorAll('#popup-sound .fps-panel svg').length") == 3, '')
    sideways(b, f'three_views_side_by_side_at_1280 {tag}', 'wide')
    shot(b, theme, 'wide-on')
    # the switch, a set and a fetch, with real clicks
    b.click(*centre(b, '#popup-sound-sp-switch'))
    poll(lambda: b.page('window.__sp.posts.length') >= 1, 3.0)
    time.sleep(0.5)
    check(f'the_switch_a_set_and_a_fetch {tag}: the switch is one request and the page is dimmed',
          b.page('window.__sp.posts') == [{'on': False}] and b.page("!!document.querySelector('#popup-sound .fps-off')") is True,
          b.page('window.__sp.posts'))
    shot(b, theme, 'wide-off')
    press(b, '#popup-sound-sp-set-fake-no-room [data-act="use"]')
    poll(lambda: b.page('window.__sp.posts.length') >= 2, 3.0)
    time.sleep(0.5)
    check(f'the_switch_a_set_and_a_fetch {tag}: Use is a request for that set and the page follows',
          b.page('window.__sp.posts')[-1] == {'set': 'fake-no-room'}
          and b.page("document.getElementById('popup-sound-sp-room-no').getAttribute('aria-checked')") == 'true',
          b.page('window.__sp.posts'))
    press(b, '#popup-sound-sp-set-hutubs-pp1 [data-act="fetch"]')
    time.sleep(0.5)
    running = b.page("document.getElementById('popup-sound-sp-set-hutubs-pp1').textContent")
    check(f'the_switch_a_set_and_a_fetch {tag}: a fetch runs with Fetching… and Cancel',
          b.page('window.__sp.fetches') == [{'id': 'hutubs-pp1'}] and 'Fetching' in running and 'Cancel' in running, running)
    press(b, '#popup-sound-sp-set-hutubs-pp1 [data-act="cancel"]')
    time.sleep(0.5)
    check(f'the_switch_a_set_and_a_fetch {tag}: Cancel aborts the request and the row is Fetch again',
          b.page('window.__sp.aborted') == 1
          and b.page("!!document.querySelector('#popup-sound-sp-set-hutubs-pp1 [data-act=\"fetch\"]')") is True, '')
    b.click(*centre(b, '#popup-sound-sp-switch'))
    time.sleep(0.4)
    colours = b.page("""(() => { const probe = (v) => { const e = document.createElement('i'); e.style.color = `var(${v})`;
        document.body.append(e); const c = getComputedStyle(e).color; e.remove(); return c; };
      const stroke = (q) => getComputedStyle(document.querySelector(q)).stroke;
      const fill = (q) => getComputedStyle(document.querySelector(q)).fill;
      return [stroke('.fps-bar') === probe('--frame-accent'), stroke('.fps-same') === probe('--frame-ink'),
        stroke('.fps-other') === probe('--frame-accent'), fill('.fps-dot') === probe('--frame-accent'),
        stroke('.fps-band-mid') === probe('--frame-graph-series')]; })()""")
    check(f'the_colours_are_the_themes_tokens {tag}: bar, curves, dot and bands are frame tokens',
          colours == [True] * 5, colours)
    bands = b.page("""['low', 'mid', 'high'].map((n) => { const s = getComputedStyle(document.querySelector('.fps-band-' + n));
      return s.stroke + ' / ' + s.strokeDasharray; })""")
    check(f'the_three_decay_bands_differ {tag}: colour or dash tells low, mid and high apart', len(set(bands)) == 3, bands)
    close_popup(b, 'sound')
    # a phone: one panel and the chips
    b.resize(*PHONE)
    time.sleep(0.5)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    tab(b, 'sp')
    shown = b.page("[...document.querySelectorAll('#popup-sound .fps-panel')].map((e) => getComputedStyle(e).display !== 'none')")
    chips = b.page("getComputedStyle(document.querySelector('#popup-sound .fps-chips')).display")
    check(f'one_view_and_chips_at_390 {tag}: one panel with the chips', shown == [True, False, False] and chips != 'none', [shown, chips])
    sideways(b, f'one_view_and_chips_at_390 {tag}', 'phone')
    shot(b, theme, 'phone-on')
    b.click(*centre(b, '#popup-sound-sp-chip-2'))
    time.sleep(0.4)
    shown = b.page("[...document.querySelectorAll('#popup-sound .fps-panel')].map((e) => getComputedStyle(e).display !== 'none')")
    check(f'one_view_and_chips_at_390 {tag}: a click on Decay shows it alone', shown == [False, False, True], shown)
    shot(b, theme, 'phone-decay')
    close_popup(b, 'sound')
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    tab(b, 'sp')
    again = b.page("[...document.querySelectorAll('#popup-sound .fps-panel')].map((e) => getComputedStyle(e).display !== 'none')")
    check(f'one_view_and_chips_at_390 {tag}: the chip is remembered in the browser', again == [False, False, True], again)
    # the keyboard
    b.page("(document.getElementById('popup-sound-sp-chip-2').focus(), 1)")
    b.press('ArrowLeft')
    time.sleep(0.4)
    focus = b.page('document.activeElement.id')
    b.press('Escape')
    gone = poll(lambda: b.page('!document.getElementById("popup-sound")'), 3.0)
    check(f'the_keyboard_and_escape {tag}: the arrows move the chip, Escape closes the popup',
          focus == 'popup-sound-sp-chip-1' and gone is True and b.page('document.activeElement.id') == 'fx-button',
          [focus, gone])
    b.resize(*DESK)
    time.sleep(0.4)


def real(b, theme):
    """The tab over the fake host's own view: two synthetic sets in the daemon's store."""
    tag = f'({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    by_the_menu(b, 'sound')
    tab(b, 'sp')
    view = '__core.model.page().host?.computer?.eq?.speakers'
    have = poll(lambda: b.page(f'!!{view}'), 5.0)
    listed = poll(lambda: b.page("!!document.getElementById('popup-sound-sp-set-fake-with-room')"), 6.0)
    check(f'real_daemon {tag}: the host\'s view is drawn and its two sets are listed as fetched',
          have is True and listed is True
          and b.page("!!document.querySelector('#popup-sound-sp-set-fake-no-room [data-act=\"use\"]')") is True, [have, listed])
    shot(b, theme, 'real-none')
    press(b, '#popup-sound-sp-set-fake-with-room [data-act="use"]')
    chosen = poll(lambda: b.page(f'{view}.set') == 'fake-with-room', 6.0)
    time.sleep(0.6)
    words = b.page("document.getElementById('popup-sound-sp-set-fake-with-room').textContent")
    check(f'real_daemon {tag}: Use reaches the host and the row says chosen', chosen is True and 'chosen' in words, [chosen, words])
    b.click(*centre(b, '#popup-sound-sp-switch'))
    turned = poll(lambda: b.page(f'{view}.on') is True, 6.0)
    time.sleep(0.6)
    check(f'real_daemon {tag}: the switch ends in what the host holds',
          turned is True and b.page("document.getElementById('popup-sound-sp-switch').getAttribute('aria-checked')") == 'true',
          b.page(f'{view}.on'))
    shot(b, theme, 'real-on')
    close_popup(b, 'sound')


def main():
    """Every case, in every theme; exits 1 on a failure."""
    for theme in THEMES:
        d = common.Daemon(host='fake')
        try:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, d, theme)
                case(b, theme)
            finally:
                b.close()
        finally:
            d.stop()
        fresh = common.Daemon(host='fake')
        try:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, fresh, theme)
                real(b, theme)
            finally:
                b.close()
        finally:
            fresh.stop()
    print(f'speakers_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
