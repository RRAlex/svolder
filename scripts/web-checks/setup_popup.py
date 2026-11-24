# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The PipeWire config popup (core/popups/setup/) and the config entry of each theme's Path row
(Ember's label button and its chip, Signal's and Paper's chips), in a browser, wide and on a phone,
against the fake host whose config passes (Ember draws no chip; its label `Path` is the button) and
the one that fails (`SVOLDER_FAKE_CONFIG=fail`, `config N`). The logic is node's
(node/core-setup.mjs); this proves what only a browser sees. Per theme and size:
  the_chip_is_the_daemons_words      Ember (only while the check fails; while it passes the label `Path` is
                                     the button), Signal (player screen) and Paper show the chip with the
                                     card's `path.config.chip` words (Paper draws the tick as a stroke); Night
                                     drive draws none
  the_chip_opens_the_popup           a real click on the chip opens PipeWire config; the menu row `PipeWire
                                     config…` opens it too, in every theme, Night drive included
  the_popup_prints_the_card          the title, the sink, the `From player` and `To DAC` rows (the card's
                                     words, `hw:3` for the fake), the summary, seven lines (the failing ones
                                     tagged, as many as the card says), a row per hop of the section and the
                                     limit
                                     12 px under the last list; nothing scrolls sideways; the rows are at
                                     least 24 px
  the_keyboard_opens_and_closes_it   Tab reaches the button, Enter opens the popup, a redraw of the card and
                                     of the host comes while it stands, Escape closes it and the focus is on
                                     the button again
  signal_computer_side               (Signal) the chip under the computer's shelf opens it too
  the_popup_names_the_hops           the Path row's `changes` is drawn as the section `What changes the
                                     sound`, one row per hop, key, label and words as the row has them; the
                                     fake DAC is below 0 dB so the DAC row is always there, the failing fake
                                     adds the computer's
  paper_fails_in_pencil              (Paper) a failing line and the summary dot are in `--paper-pencil`
Usage: python3 setup_popup.py chromium|firefox"""
import os
import sys
import time

import common
import modal
from modal import THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, ready, rects

browser = modal.browser

SETUP = "__core.model.page().device?.playing?.path?.config ?? null"
CHANGES = "__core.model.page().device?.playing?.path?.row?.changes ?? null"
CHIPS = {'ember': '#np-config', 'signal': '#sg-config', 'paper': '#p-config'}
# Ember draws its chip only while the check fails; while it passes the label `Path` is the button
LABEL = '#np-label'
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


def button_of(theme, fail):
    """The selector of the button that opens the popup on the path line."""
    return LABEL if theme == 'ember' and not fail else CHIPS[theme]


def chip_text(b, selector):
    return b.page(f"(() => {{ const c = document.querySelector({selector!r}); if (!c) return null;"
                  " const k = c.cloneNode(true); k.querySelectorAll('.p-sr, .np-sr, svg').forEach((n) => n.remove());"
                  " return k.textContent.trim(); })()")


REDRAW = ("(() => { const m = __core.model, p = m.page(); const playing = p.device.playing;"
          " m.emit('part', { device: m.shown(), name: 'playing', data: playing });"
          " m.emit('host', p.host); m.emit('model', p); return 1; })()")


def by_keyboard(b, selector, tag):
    """Tab to the button, Enter, a redraw under the popup, Escape: the focus is on the button again."""
    ident = f'document.activeElement?.matches({selector!r})'
    b.page(modal.BODY)
    reached = False
    for _ in range(120):
        b.press('Tab')
        if b.page(ident) is True:
            reached = True
            break
    check(f'the_keyboard_opens_and_closes_it {tag}: Tab reaches the button', reached, b.page('document.activeElement?.id'))
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
    focus = b.page(ident)
    check(f'the_keyboard_opens_and_closes_it {tag}: Escape closes it and the focus is on the button',
          gone is True and focus is True, [gone, focus])
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
        button = button_of(theme, fail)
        want = card['chip'].replace('✓', '').strip() if theme == 'paper' else card['chip']
        if button == LABEL:
            want = 'Path'
            check(f'the_chip_is_the_daemons_words {tag}: a passing check draws no chip, the label is the button',
                  b.page("!document.querySelector('.np-setup')") is True)
        got = poll(lambda: chip_text(b, button), 4.0)
        check(f'the_chip_is_the_daemons_words {tag}: {want}', got == want, [got, want])
        # the keyboard first, on the page as it loaded: Escape closes it as a person does
        by_keyboard(b, button, tag)
        tap(b, button)
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
                      " note: document.querySelector('#popup-setup .fp-page > .fp-note')?.textContent ?? null,"
                      " gap: (() => { const n = document.querySelector('#popup-setup .fp-page > .fp-note');"
                      " return n ? getComputedStyle(n).marginTop : null; })() }; })()")
        fails = sum(1 for line in card['lines'] if line['outcome'] == 'fail')
        # the section is the Path row's `changes`: the passing fake changes the sound only at its DAC
        # (always below 0 dB, fake host, playing.py), so that row is required in every theme; the
        # failing fake adds the computer (a filter in the sink, a forced-rate resample)
        changes = b.page(CHANGES)
        seen = b.page("(() => { const list = document.querySelector('#popup-setup [data-changes]');"
                      " return list ? { hops: [...list.querySelectorAll('[data-hop]')].map((r) => ({"
                      " hop: r.dataset.hop, label: r.querySelector('.fp-row-label')?.textContent,"
                      " words: r.querySelector('.fp-row-value')?.textContent })),"
                      " heading: [...document.querySelectorAll('#popup-setup .fp-heading')].map((h) => h.textContent) } : null; })()")
        want = [{'hop': c['key'], 'label': c['label'],
                 'words': ' · '.join(w['text'] for w in c['words'])} for c in changes or []]
        check(f'the_popup_names_the_hops {tag}: the section is the row\'s changes, shown, with its words',
              bool(changes) and seen is not None and seen['hops'] == want
              and any(c['hop'] == 'dac' and c['words'].startswith('volume') for c in want)
              and (not fail or any(c['hop'] == 'computer' for c in want))
              and 'What changes the sound' in seen['heading'], [changes, seen])
        want_rows = 9 + len(changes or [])
        io = b.page("(() => { const out = {}; for (const r of document.querySelectorAll('#popup-setup [data-io]'))"
                    " out[r.dataset.io] = { label: r.querySelector('.fp-row-label')?.textContent,"
                    " words: r.querySelector('.fp-row-value')?.textContent }; return out; })()")
        check(f'the_popup_prints_the_card {tag}: From player and To DAC are the card\'s words, under the sink',
              bool(card.get('from_player')) and bool(card.get('to_dac'))
              and io == {'from_player': {'label': 'From player', 'words': card['from_player']},
                         'to_dac': {'label': 'To DAC', 'words': card['to_dac']}}
              and card['to_dac'].endswith('stereo \u00b7 hw:3') and 'kHz' in card['from_player'],
              [io, card.get('from_player'), card.get('to_dac')])
        gap = b.page("(() => { const rows = document.querySelectorAll('#popup-setup [data-io]');"
                     " const sum = document.querySelector('#popup-setup .fpk-sum');"
                     " return rows.length && sum ? Math.round(sum.getBoundingClientRect().top"
                     " - rows[rows.length - 1].getBoundingClientRect().bottom) : null; })()")
        check(f'the_popup_prints_the_card {tag}: the summary stands 12 px under the last io row',
              gap == 12, gap)
        check(f'the_popup_prints_the_card {tag}: title, sink, summary, seven lines and the hops, the failing tagged',
              page['title'] == 'PipeWire config' and page['sink'] == card['sink'] and page['sum'] == card['summary']
              and page['rows'] == want_rows and page['fails'] == fails and page['low'] == 0 and bool(page['note']), [page, fails])
        folder = os.environ.get('WEB_CHECK_SHOTS')
        if folder:
            time.sleep(0.5)
            with open(os.path.join(folder, f'{theme}-setup-{size}-{"fail" if fail else "pass"}.png'), 'wb') as out:
                out.write(b.shot())
        check(f'the_popup_prints_the_card {tag}: the limit sits 12 px under the last list, the section or not',
              page['gap'] == '12px', page['gap'])
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
