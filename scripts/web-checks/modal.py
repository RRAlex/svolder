# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The core's modal layer (core/modal.js) and its router of the history (core/router.js) in a
browser, in every theme, with a probe modal and a probe popup (no popup of the page exists yet):

  the_top_modal_owns_every_key          first the control, with nothing open: Space on the page
                                        posts one press of the player (the frame's own page-wide
                                        key, in every theme), ArrowLeft in Ember and ArrowUp in
                                        Night drive post a volume (the two themes that listen for
                                        keys on the whole page). Then, with a modal of the stack
                                        open, the modal is handed every key pressed, and none
                                        posts a write or opens anything behind it (arrows, m, 0,
                                        f, p, v, a, ?, q, T, Space, the focus in the modal or on
                                        the page), Tab stays in it, the page under it is inert,
                                        Escape closes it and the focus is back on its opener; then
                                        T is the frame's again and toggles the picker.
                                        What each assert can catch: in Ember the "no key posts"
                                        one fails as soon as the frame stops handing keys to the
                                        stack (Ember's listener asks nothing else). Night drive's
                                        keys and the frame's Space and T also ask
                                        `core.frame.covered()`, and Paper and Signal have no
                                        page-wide key listener of their own, so in those three
                                        "no key posts" fails only when both guards do; there the
                                        hand-off itself is held by "the modal is handed every
                                        key" and "Escape closes it"
  the_question_owns_the_keyboard        the same under `core.ask` (ArrowLeft in Ember posts no
                                        volume), Escape answers No
  t_is_the_sheets_under_a_themes_own_sheet   (Night drive, Signal) with the theme's own modal
                                        sheet open T opens no picker; with it closed T does
  a_device_switch_closes_every_modal    the picker and a question over it are closed when another
                                        device is shown, the question answered No
  back_steps_the_popup_and_never_changes_the_device   a popup registered with the router, which
                                        puts it on the stack: Back steps page → list → closed and
                                        reads no device; Forward after Back shows it again; Escape,
                                        and a close asked for during a step back to the list, land
                                        on the entry the popup was opened from, and Forward after
                                        them opens nothing; a step past the popup changes the
                                        device only where the entry names another; a device shown
                                        from inside the popup closes it and takes its entries' place
  a_theme_switch_closes_every_modal     the question over the picker is closed by a pick, and so is
                                        a popup, which opens again afterwards
  the_popups_are_centred_up_to_the_phone_width   (Ember) a toast is centred at 719 px wide and at
                                        the right at 720 px: the frame's sheet turns at the core's
                                        PHONE_MAX_PX
Usage: python3 modal.py chromium|firefox"""
import json
import os
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
THEMES = [t for t in ('ember', 'paper', 'signal', 'nightdrive')
          if t in os.environ.get('WEB_CHECK_THEMES', 'ember,paper,signal,nightdrive').split(',')]


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# every POST the page makes is noted
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (a[1] && a[1].method === 'POST') window.__posts.push([String(a[0]), String(a[1].body)]);"
        " return f(...a); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
POSTS = 'window.__posts'
BODY = "(document.activeElement && document.activeElement.blur(), document.body.focus(), 1)"
AT = "(document.activeElement && (document.activeElement.id || document.activeElement.className || document.activeElement.tagName))"
# nothing of the page's own opened: the picker, the menu, the Help popup and Ember's Arrange bar
QUIET = ("(!document.querySelector('#picker, #popup-help, .arr-bar')"
         " && document.getElementById('fx-menu').hidden)")
INERT = ("[document.getElementById('theme').inert, document.getElementById('frame').inert,"
         " Boolean(document.getElementById('toasts')?.inert)]")
# a modal of the stack with two buttons, opened from the frame's button
PROBE = """(() => { const el = document.createElement('div'); el.id = 'probe-modal';
  el.innerHTML = '<button id="probe-a">a</button><button id="probe-b">b</button>';
  window.__closed = 0; window.__keys = [];
  document.getElementById('fx-button').focus();
  __core.modal.open({ el, onClose: () => { window.__closed += 1; },
    onKey: (event) => { window.__keys.push(event.key); return false; } });
  return [document.activeElement.id, el.getAttribute('aria-modal'), el.getAttribute('role')]; })()"""
# a popup registered with the router, which puts it on the stack and takes it off, as a popup of
# the page will be
POPUP = """(() => { window.__told = [];
  const el = document.createElement('div'); el.id = 'probe-popup';
  el.innerHTML = '<button id="probe-x">x</button>';
  __core.router.register('probe', {
    el,
    show(page) { window.__told.push('show ' + page); },
    hide() { window.__told.push('hide'); },
  });
  window.__shows = 0; const show = __core.actions.showDevice;
  __core.actions.showDevice = (...a) => { window.__shows += 1; return show(...a); };
  return 1; })()"""
OPEN = "!!document.getElementById('probe-popup')"
STATE = "(() => { const s = __core.router.state(); return [s.device, s.popup, s.page]; })()"
ENTRY = "[history.state && history.state.device, history.state && history.state.popup, history.state && history.state.page]"
# the keys both browsers' drivers can press, and the ones sent as events to where the focus is
PRESSED = ('ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'm', '?', 'a', 'v', 'q', 't', 'T')
SENT = ('0', 'f', 'p', 'A')


def poll(read, secs=6.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def keys(b, also=()):
    for key in PRESSED + tuple(also):
        b.press(key)
    for key in SENT:
        b.page("(document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown',"
               f" {{ key: {json.dumps(key)}, bubbles: true, cancelable: true }})), 1")
    time.sleep(0.6)


def load(b, d, theme):
    b.goto(f'{d.url}?theme={theme}&effects=off', settle=1.5)
    b.page(BOOT)
    time.sleep(0.5)
    return b.page('__core.model.page().fleet.devices.map((d) => d.url)')


# the keys a theme or the frame acts on with the focus on the page, and the POST each makes
CONTROLS = {'ember': ('ArrowLeft', 'volume'), 'nightdrive': ('ArrowUp', 'volume')}


def posted(b, what):
    return [p for p in b.page(POSTS) if what in p[0]]


def owns(b, theme):
    tag = f'the_top_modal_owns_every_key ({theme})'
    # the control: with nothing open these keys do write
    b.page(BODY)
    b.page(f'{POSTS}.length = 0, 1')
    b.press(' ')
    check(f'{tag}: the control: with nothing open Space on the page posts a press of the player',
          poll(lambda: len(posted(b, 'host/press')) == 1, 3.0) is True, b.page(POSTS))
    if theme in CONTROLS:
        key, what = CONTROLS[theme]
        b.page(BODY)
        b.page(f'{POSTS}.length = 0, 1')
        b.press(key)
        check(f'{tag}: the control: with nothing open {key} on the page posts a {what}',
              poll(lambda: len(posted(b, what)) >= 1, 3.0) is True, b.page(POSTS))
        time.sleep(0.8)
    try:
        opened = b.page(PROBE)
    except Exception as error:  # noqa: BLE001
        check(f'{tag}: a modal opens on the stack', False, str(error)[:300])
        return
    check(f'{tag}: a modal opens on the stack, a dialog with the focus in it',
          opened == ['probe-a', 'true', 'dialog'], opened)
    check(f'{tag}: the page under it is inert, the popups are not', b.page(INERT) == [True, True, False], b.page(INERT))
    b.page(f'{POSTS}.length = 0, 1')
    keys(b, also=(' ',))
    check(f'{tag}: no key posts a write with the focus in the modal', b.page(POSTS) == [], b.page(POSTS))
    b.page(BODY)
    keys(b, also=(' ',))
    check(f'{tag}: nor with the focus on the page', b.page(POSTS) == [], b.page(POSTS))
    handed = b.page('window.__keys')
    check(f'{tag}: the modal is handed every key pressed, twice over',
          handed == list(PRESSED + (' ',) + SENT) * 2, handed)
    check(f'{tag}: nothing opened behind it (T, ?, a, the menu)', b.page(QUIET) is True)
    check(f'{tag}: it is still open', b.page("!!document.getElementById('probe-modal')") is True)
    b.press('Tab')
    first = b.page(AT)
    b.press('Tab')
    second = b.page(AT)
    b.press('Tab')
    check(f'{tag}: Tab stays in it', [first, second, b.page(AT)] == ['probe-a', 'probe-b', 'probe-a'],
          [first, second, b.page(AT)])
    b.press('Escape')
    closed = b.page("[!document.getElementById('probe-modal'), window.__closed, document.activeElement.id]")
    check(f'{tag}: Escape closes it and the focus is back on its opener', closed == [True, 1, 'fx-button'], closed)
    check(f'{tag}: the page is in reach again', b.page(INERT) == [False, False, False], b.page(INERT))
    b.page(BODY)
    b.press('T')
    picker = poll(lambda: b.page("!!document.getElementById('picker')"))
    b.press('T')
    check(f'{tag}: T is the frame\'s again: it opens the picker and closes it',
          picker is True and poll(lambda: b.page("!document.getElementById('picker')")) is True, picker)


def question(b, theme):
    tag = f'the_question_owns_the_keyboard ({theme})'
    b.page(BODY)
    b.page("window.__answer = null, (__core.ask('Probe?').then((a) => { window.__answer = a; }), 1)")
    poll(lambda: b.page("!!document.querySelector('.ask')"))
    b.page(f'{POSTS}.length = 0, 1')
    keys(b)
    check(f'{tag}: no key posts a write behind the question', b.page(POSTS) == [], b.page(POSTS))
    check(f'{tag}: nothing opened behind it and it is still open',
          b.page(f"{QUIET} && !!document.querySelector('.ask')") is True)
    b.press('Escape')
    check(f'{tag}: Escape answers No', poll(lambda: b.page('window.__answer === false')) is True,
          b.page('window.__answer'))


SHEETS = {
    'nightdrive': ("document.getElementById('nd-sheet').click()", "!!document.getElementById('nd-dialog')"),
    'signal': ("(document.querySelector('.fxbtn').click(), document.getElementById('menu-add-device').click())",
               "!!document.getElementById('sg-sheet-form')"),
}


def own_sheet(b, theme):
    tag = f't_is_the_sheets_under_a_themes_own_sheet ({theme})'
    opener, there = SHEETS[theme]
    b.page(BODY)
    b.page(f'({opener}, 1)')
    stands = poll(lambda: b.page(there), 4.0)
    b.press('T')
    time.sleep(0.5)
    check(f'{tag}: with the sheet open T opens no picker and the sheet stays',
          stands is True and b.page(f"!document.getElementById('picker') && {there}") is True,
          [stands, b.page("!!document.getElementById('picker')"), b.page(there)])
    b.press('Escape')
    shut = poll(lambda: b.page(f'!({there})'), 4.0)
    b.page(BODY)
    b.press('T')
    opened = poll(lambda: b.page("!!document.getElementById('picker')"), 3.0)
    check(f'{tag}: with it closed T opens the picker', shut is True and opened is True, [shut, opened])
    b.press('Escape')
    poll(lambda: b.page("!document.getElementById('picker')"), 3.0)


def stacked(b):
    """The picker with a question over it; whether both stand."""
    b.page(BODY)
    b.press('T')
    poll(lambda: b.page("!!document.getElementById('picker')"))
    b.page("window.__answer = null, (__core.ask('Probe?').then((a) => { window.__answer = a; }), 1)")
    return poll(lambda: b.page("!!document.getElementById('picker') && !!document.querySelector('.ask')"))


GONE = ("(!document.getElementById('picker') && !document.querySelector('.ask')"
        " && window.__answer === false && !document.getElementById('theme').inert)")


def device_switch(b, theme, urls):
    tag = f'a_device_switch_closes_every_modal ({theme})'
    both = stacked(b)
    b.page(f"__core.actions.showDevice({json.dumps(urls[1])}).then(() => 1)")
    gone = poll(lambda: b.page(GONE))
    check(f'{tag}: the picker and the question over it are closed, the question answered No',
          both is True and gone is True,
          [both, b.page("[!!document.getElementById('picker'), !!document.querySelector('.ask'), window.__answer]")])
    b.page(f"__core.actions.showDevice({json.dumps(urls[0])}).then(() => 1)")
    poll(lambda: b.page(f'__core.model.shown() === {json.dumps(urls[0])}'))


def router(b, theme, urls):
    tag = f'back_steps_the_popup_and_never_changes_the_device ({theme})'
    first, second = urls[0], urls[1]
    try:
        b.page(POPUP)
    except Exception as error:  # noqa: BLE001
        check(f'{tag}: a popup registers with the router', False, str(error)[:300])
        return
    b.page(BODY)
    start = b.page('[history.length, location.search, __core.model.shown()]')
    b.page("(__core.router.open('probe'), 1)")
    b.page("(__core.router.step('page-1'), 1)")
    got = b.page(f'[{OPEN}, {STATE}, history.length, location.search]')
    check(f'{tag}: opening and entering a page push, the address stays the device\'s',
          got == [True, [first, 'probe', 'page-1'], start[0] + 2, start[1]], [start, got])
    b.page('(history.back(), 1)')
    listed = poll(lambda: b.page(f'{STATE}[2] === null'))
    check(f'{tag}: Back from the page is the popup\'s list', listed is True and b.page(OPEN) is True, b.page(STATE))
    b.page('(history.back(), 1)')
    shut = poll(lambda: b.page(f'!{OPEN}'))
    check(f'{tag}: Back from the list closes it, no device was read and the shown one stays',
          shut is True and b.page('[window.__shows, __core.model.shown()]') == [0, first],
          b.page('[window.__shows, __core.model.shown(), window.__told]'))
    b.page('(history.forward(), 1)')
    check(f'{tag}: Forward after Back shows the popup again', poll(lambda: b.page(OPEN)) is True, b.page('window.__told'))
    # Escape from a page: the stack closes the popup, which tells the router
    b.page("(__core.router.step('page-1'), 1)")
    b.press('Escape')
    landed = poll(lambda: b.page(f'{ENTRY}[1] === null && history.length === {start[0] + 2}'))
    time.sleep(0.4)
    check(f'{tag}: Escape closes it and lands on the entry it was opened from',
          landed is True and b.page(f'[{OPEN}, {STATE}, window.__shows]') == [False, [first, None, None], 0],
          b.page(f'[{OPEN}, {STATE}, {ENTRY}, window.__shows, history.length]'))
    for ahead in (1, 2):
        b.page('(history.forward(), 1)')
        time.sleep(0.6)
        check(f'{tag}: Forward after Escape opens nothing ({ahead})',
              b.page(f'[{OPEN}, {ENTRY}[1], window.__shows, __core.model.shown()]') == [False, None, 0, first],
              b.page(f'[{OPEN}, {ENTRY}, window.__shows, window.__told]'))
    # a close asked for while a step back to the list is on its way: two steps of the router's own
    b.page("(__core.router.open('probe'), __core.router.step('page-2'), 1)")
    base = b.page('history.length')
    b.page("(__core.router.step(null), __core.router.close(), 1)")
    time.sleep(0.8)
    check(f'{tag}: a close during a step back to the list lands on the entry the popup was opened from',
          b.page(f'[{OPEN}, {ENTRY}[1], {STATE}, window.__shows]') == [False, None, [first, None, None], 0]
          and b.page('history.length') == base,
          b.page(f'[{OPEN}, {ENTRY}, {STATE}, window.__shows, history.length, {base}]'))
    # the device part: another device, the popup over it, and back past both
    b.page(f"__core.actions.showDevice({json.dumps(second)}).then(() => 1)")
    poll(lambda: b.page(f'__core.model.shown() === {json.dumps(second)}'))
    b.page("(window.__shows = 0, __core.router.open('probe'), 1)")
    b.page('(history.back(), 1)')
    shut = poll(lambda: b.page(f'!{OPEN}'))
    time.sleep(0.4)
    check(f'{tag}: Back from a popup over another device closes it and leaves that device',
          shut is True and b.page('[window.__shows, __core.model.shown()]') == [0, second],
          b.page('[window.__shows, __core.model.shown()]'))
    b.page('(history.back(), 1)')
    back = poll(lambda: b.page(f'__core.model.shown() === {json.dumps(first)}'))
    check(f'{tag}: the step past it shows the device that entry names', back is True and b.page('window.__shows') == 1,
          b.page('[window.__shows, __core.model.shown()]'))
    # a device shown from inside the popup: it closes, and the device's entry takes its entries' place
    b.page("(__core.router.open('probe'), __core.router.step('page-3'), 1)")
    base = b.page('history.length')
    b.page(f"__core.actions.showDevice({json.dumps(second)}).then(() => 1)")
    poll(lambda: b.page(f'{ENTRY}[0] === {json.dumps(second)}'))
    time.sleep(0.4)
    got = b.page(f'[{OPEN}, {ENTRY}, __core.model.shown(), history.length, location.search]')
    check(f'{tag}: a device shown from inside the popup closes it and pushes over the popup\'s entries',
          got[:3] == [False, [second, None, None], second] and got[3] == base - 1 and got[4].endswith(second),
          [got, base])
    b.page('(history.back(), 1)')
    back = poll(lambda: b.page(f'__core.model.shown() === {json.dumps(first)}'))
    time.sleep(0.4)
    check(f'{tag}: and Back from it is the first device, no popup', back is True and b.page(OPEN) is False,
          b.page(f'[{OPEN}, {ENTRY}, __core.model.shown()]'))


def theme_switch(b, theme):
    tag = f'a_theme_switch_closes_every_modal ({theme})'
    other = 'paper' if theme != 'paper' else 'signal'
    both = stacked(b)
    b.page(f"document.querySelector('#picker .tcard[data-theme={other}]').click()")
    switched = poll(lambda: b.page(f"document.documentElement.dataset.theme === '{other}'"), 10.0)
    gone = poll(lambda: b.page(GONE))
    check(f'{tag}: a pick closes the question over the picker and the page is in reach',
          both is True and switched is True and gone is True,
          [both, switched, b.page("[!!document.getElementById('picker'), !!document.querySelector('.ask'), window.__answer]")])
    # a popup, which nothing but the stack tells: it is closed, the history follows, it opens again
    b.page("(__core.router.open('probe'), 1)")
    stands = poll(lambda: b.page(OPEN))
    b.press('T')
    b.page(f"document.querySelector('#picker .tcard[data-theme={theme}]')?.click() ?? null")
    still = b.page(f'[{OPEN}, !!document.getElementById("picker")]')
    check(f'{tag}: T does nothing under a popup', stands is True and still == [True, False], [stands, still])
    b.page(f"(__core.frame.openPicker(), document.querySelector('#picker .tcard[data-theme={theme}]').click())")
    back = poll(lambda: b.page(f"document.documentElement.dataset.theme === '{theme}'"), 10.0)
    time.sleep(0.6)
    after = b.page(f'[{OPEN}, {STATE}[1], {ENTRY}[1], document.getElementById("theme").inert]')
    check(f'{tag}: a switch closes a popup and the history steps back past it',
          back is True and after == [False, None, None, False], [back, after])
    b.page("(__core.router.open('probe'), 1)")
    check(f'{tag}: and the popup opens again afterwards', poll(lambda: b.page(OPEN)) is True, b.page(STATE))
    b.press('Escape')
    poll(lambda: b.page(f'!{OPEN}'))


def phone_width(d):
    tag = 'the_popups_are_centred_up_to_the_phone_width'
    got = {}
    for width in (719, 720):
        b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(width, 800))
        try:
            b.preload(HOOK)
            load(b, d, 'ember')
            b.page("(__core.frame.toast('Probe', null, { ms: 0 }), 1)")
            time.sleep(0.5)
            got[width] = b.page("""(() => { const r = document.querySelector('.toast').getBoundingClientRect();
              return [Math.round(r.left + r.width / 2), Math.round(r.right), document.documentElement.clientWidth]; })()""")
        finally:
            b.close()
    narrow, wide = got[719], got[720]
    check(f'{tag}: centred at 719 px, at the right at 720 px',
          abs(narrow[0] - narrow[2] / 2) <= 2 and abs(wide[1] - (wide[2] - 18)) <= 2, got)
    number = open(f'{common.WEB}/core/phone.js').read()
    check(f'{tag}: the number is the core\'s PHONE_MAX_PX', 'export const PHONE_MAX_PX = 719;' in number)


def main():
    """Every case, in every theme; exits 1 on a failure."""
    d = common.Daemon(host='fake', more=[('Second', '1112131415161718')])
    try:
        for theme in THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(1280, 800))
            try:
                b.preload(HOOK)
                urls = load(b, d, theme)
                owns(b, theme)
                question(b, theme)
                if theme in SHEETS:
                    own_sheet(b, theme)
                device_switch(b, theme, urls)
                router(b, theme, urls)
                theme_switch(b, theme)
            finally:
                b.close()
        if 'ember' in THEMES:
            phone_width(d)
    finally:
        d.stop()

    print(f'modal.py: {passed} passed, {len(fails)} failed')
    sys.exit(1 if fails else 0)


# popups.py imports this file for its helpers (check, poll, load, the page expressions): the
# cases run only when it is the script
if __name__ == '__main__':
    main()
