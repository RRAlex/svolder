# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The PipeWire config popup (core/popups/setup/) and the config chip of each theme's path line, in a
browser, wide and on a phone, against the fake host whose config passes (`config ✓`) and the one that fails
(`SVOLDER_FAKE_CONFIG=fail`, `config N`). The logic is node's (node/core-setup.mjs); this proves what only a
browser sees. Per theme and size:
  the_chip_is_the_daemons_words      Ember, Signal (player screen) and Paper show the chip with the card's
                                     `path.config.chip` words (Paper draws the tick as a stroke); Night drive
                                     draws none
  the_chip_opens_the_popup           a real click on the chip opens PipeWire config; the menu row `PipeWire
                                     config…` opens it too, in every theme, Night drive included
  the_popup_prints_the_card          the title, the sink, the summary, seven lines (the failing ones tagged,
                                     as many as the card says) and the limit; nothing scrolls sideways; the
                                     rows are at least 24 px
  the_keyboard_opens_and_closes_it   Tab reaches the chip (by its id), Enter opens the popup, a redraw of the
                                     card and of the host comes while it stands, Escape closes it and the
                                     focus is on the chip again
  signal_computer_side               (Signal) the chip under the computer's shelf opens it too
  paper_fails_in_pencil              (Paper) a failing line and the summary dot are in `--paper-pencil`
Usage: python3 setup_popup.py chromium|firefox"""
import sys
import time

import common
import modal
from modal import THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, ready, rects

browser = modal.browser

SETUP = "__core.model.page().device?.playing?.path?.config ?? null"
CHIPS = {'ember': '#np-config', 'signal': '#sg-config', 'paper': '#p-config'}
SHOW = {
    # a phone's folded Ember card hides the path until a tap opens it
    'ember': "(() => { const p = document.getElementById('part-playing');"
             " if (p && !p.hasAttribute('data-expanded')) p.querySelector('.np-head').click(); return 1; })()",
}
OVER = "[document.documentElement.scrollWidth - document.documentElement.clientWidth]"


def tap(b, selector):
    """A real click on `selector`, scrolled into view first (Paper's notes stand below the fold)."""
    b.page(f"(document.querySelector({selector!r}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.3)
    b.click(*centre(b, selector))


def choose_hop(b, steps):
    """Signal's line by the keyboard (a phone's strip slides the hops off the screen): the chosen hop, then
    `steps` presses of Left, which go from the device to the computer (1) and to the player (2)."""
    b.page("(document.querySelector('.sg-sop.sg-cur, .sg-hop.sg-cur').focus(), 1)")
    for _ in range(steps):
        b.press('ArrowLeft')
    time.sleep(0.6)


def chip_text(b, theme):
    return b.page(f"(() => {{ const c = document.querySelector({CHIPS[theme]!r}); if (!c) return null;"
                  " const k = c.cloneNode(true); k.querySelectorAll('.p-sr, svg').forEach((n) => n.remove());"
                  " return k.textContent.trim(); })()")


REDRAW = ("(() => { const m = __core.model, p = m.page(); const playing = p.device.playing;"
          " m.emit('part', { device: m.shown(), name: 'playing', data: playing });"
          " m.emit('host', p.host); m.emit('model', p); return 1; })()")


def by_keyboard(b, theme, tag):
    """Tab to the chip, Enter, a redraw under the popup, Escape: the focus is on the chip again."""
    ident = CHIPS[theme][1:]
    b.page(modal.BODY)
    reached = False
    for _ in range(120):
        b.press('Tab')
        if b.page('document.activeElement?.id') == ident:
            reached = True
            break
    check(f'the_keyboard_opens_and_closes_it {tag}: Tab reaches the chip', reached, b.page('document.activeElement?.id'))
    if not reached:
        return
    b.press('Enter')
    opened = ready(b, 'setup')
    check(f'the_keyboard_opens_and_closes_it {tag}: Enter opens the popup', opened is True,
          [opened, b.page('__core.router.state()'), b.page('document.activeElement?.id')])
    b.page(REDRAW)
    time.sleep(0.6)
    b.press('Escape')
    gone = poll(lambda: b.page("!document.getElementById('popup-setup')"), 4.0)
    time.sleep(0.4)
    focus = b.page('document.activeElement?.id')
    check(f'the_keyboard_opens_and_closes_it {tag}: Escape closes it and the focus is on the chip',
          gone is True and focus == ident, [gone, focus])
    # and again, as a person does: the focus is on the chip, Enter opens it a second time
    b.press('Enter')
    again = ready(b, 'setup')
    check(f'the_keyboard_opens_and_closes_it {tag}: Enter on the chip opens it again after a close', again is True,
          [again, b.page('__core.router.state()'), b.page('document.activeElement?.id')])
    b.press('Escape')
    poll(lambda: b.page("!document.getElementById('popup-setup')"), 4.0)
    time.sleep(0.4)


def signal_computer(b, tag):
    """The chip under the computer's shelf opens the popup too."""
    choose_hop(b, 1)
    there = b.page("!!document.querySelector('#sg-shelf #sg-config, #sg-open #sg-config')")
    check(f'signal_computer_side {tag}: the computer hop shows the chip', there is True, there)
    if there is True:
        tap(b, '#sg-config')
        opened = ready(b, 'setup')
        check(f'signal_computer_side {tag}: a click on it opens the popup', opened is True, opened)
        close_popup(b, 'setup')


def case(b, d, theme, size, fail):
    tag = f'{theme} {size} {"fail" if fail else "pass"}'
    load(b, d, theme)
    card = poll(lambda: b.page(SETUP), 8.0)
    check(f'the_chip_is_the_daemons_words {tag}: the card carries a config check', bool(card), card)
    if not card:
        return
    if theme in SHOW:
        b.page(SHOW[theme])
        time.sleep(0.5)
    if theme == 'signal':
        # Signal draws the path on the player's screen
        choose_hop(b, 2)
        check(f'the_chip_is_the_daemons_words {tag}: the player\'s screen is on show',
              b.page("!!document.getElementById('sg-player') && !document.getElementById('sg-cfacts')") is True)
    if theme in CHIPS:
        # only Paper draws its tick as a stroke; the others print the daemon's chip as it is
        want = card['chip'].replace('✓', '').strip() if theme == 'paper' else card['chip']
        got = poll(lambda: chip_text(b, theme), 4.0)
        check(f'the_chip_is_the_daemons_words {tag}: {card["chip"]}', got == want, [got, want])
        # the keyboard first, on the page as it loaded: Escape closes it as a person does
        by_keyboard(b, theme, tag)
        tap(b, CHIPS[theme])
        opened = ready(b, 'setup')
        check(f'the_chip_opens_the_popup {tag}: a click on the chip', opened is True, opened)
    else:
        check(f'the_chip_is_the_daemons_words {tag}: no chip on this theme',
              b.page("!document.querySelector('.np-setup, .sg-chip, .p-setup')") is True)
        opened = by_the_menu(b, 'setup')
        check(f'the_chip_opens_the_popup {tag}: the menu row opens it', opened is True, opened)
    if opened is True:
        page = b.page("(() => { const q = (s) => document.querySelector('#popup-setup ' + s);"
                      " return { title: q('.fp-title')?.textContent, sink: q('.fpk-sink')?.textContent,"
                      " sum: q('.fpk-sum')?.textContent, rows: document.querySelectorAll('#popup-setup .fp-row').length,"
                      " fails: document.querySelectorAll('#popup-setup .fp-tag').length,"
                      " low: [...document.querySelectorAll('#popup-setup .fp-row')].filter((r) => r.getBoundingClientRect().height < 24).length,"
                      " note: document.querySelector('#popup-setup .fp-page > .fp-note')?.textContent ?? null }; })()")
        fails = sum(1 for line in card['lines'] if line['outcome'] == 'fail')
        check(f'the_popup_prints_the_card {tag}: title, sink, summary, seven lines, the failing tagged',
              page['title'] == 'PipeWire config' and page['sink'] == card['sink'] and page['sum'] == card['summary']
              and page['rows'] == 7 and page['fails'] == fails and page['low'] == 0 and bool(page['note']), [page, fails])
        check(f'the_popup_prints_the_card {tag}: nothing scrolls sideways', b.page(OVER) == [0],
              rects(b, 'setup')['over'])
        if theme == 'paper' and fail:
            ink = b.page("(() => { const pencil = getComputedStyle(document.documentElement)"
                         ".getPropertyValue('--paper-pencil').trim();"
                         " const probe = document.createElement('i'); probe.style.color = pencil;"
                         " document.body.append(probe); const want = getComputedStyle(probe).color; probe.remove();"
                         " return [want, getComputedStyle(document.querySelector('#popup-setup .fpk-fail')).color]; })()")
            check(f'paper_fails_in_pencil {tag}: a failing line is red pencil', ink[0] == ink[1], ink)
        close_popup(b, 'setup')
    if theme == 'signal':
        signal_computer(b, tag)
    # the menu row in every theme
    via = by_the_menu(b, 'setup')
    check(f'the_chip_opens_the_popup {tag}: the menu row PipeWire config… opens it', via is True, via)
    close_popup(b, 'setup')


def main():
    """Every case, in every theme, for the passing and the failing fake config; exits 1 on a failure."""
    for fail in (False, True):
        d = common.Daemon(host='fake', env={'SVOLDER_FAKE_CONFIG': 'fail'} if fail else None)
        try:
            for theme in THEMES:
                for size, label in ((DESK, 'wide'), (PHONE, 'phone')):
                    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
                    try:
                        b.preload(modal.HOOK)
                        case(b, d, theme, label, fail)
                    finally:
                        b.close()
        finally:
            d.stop()
    print(f'setup_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
