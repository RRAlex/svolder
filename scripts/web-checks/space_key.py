# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The frame's second reserved key, Space, in every theme: on the page body it posts one press to
api/v1/host/press (pause while the fake player plays) and the page does not scroll; on a focused
button it activates that button and posts no press; in a text field it types; with a menu or a
dialog open it presses nothing; with no player on the page it posts nothing.
Usage: python3 space_key.py chromium|firefox"""
import sys

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
THEMES = ('ember', 'paper', 'signal', 'nightdrive')


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# every POST is noted, with the path the page used (relative, no leading slash)
HOOK = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { if (a[1] && a[1].method === 'POST') {
    window.__posts.push([String(a[0]), String(a[1].body)]); } return f(...a); }; })()"""
PRESSES = "window.__posts.filter((p) => /host\\/press/.test(p[0]))"
BODY = "(document.activeElement && document.activeElement.blur(), document.body.focus(), 1)"


def make(daemon, theme, size=(1280, 500)):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
    b.goto(f'{daemon.url}?theme={theme}', settle=1.5)
    b.ev(HOOK)
    b.ev(BODY)
    return b


dac = common.Daemon(host='fake')
try:
    for theme in THEMES:
        b = make(dac, theme)
        try:
            before = b.ev('window.scrollY')
            b.press(' ')
            b.sleep(0.5)
            got = b.ev(PRESSES)
            check(f'space_on_the_body_posts_one_press_and_does_not_scroll ({theme})',
                  len(got) == 1 and got[0][0] == 'api/v1/host/press' and got[0][1].replace(' ', '') in ('{"button":"pause"}', '{"button":"play"}')
                  and b.ev('window.scrollY') == before, [got, before, b.ev('window.scrollY')])
            b.ev('window.__posts.length = 0')
            b.ev("document.getElementById('fx-button').focus()")
            b.press(' ')
            b.sleep(0.4)
            opened = b.ev("document.getElementById('fx-button').getAttribute('aria-expanded')")
            check(f'  ({theme}) space on a focused button activates it and posts no press',
                  opened == 'true' and b.ev(PRESSES) == [], [opened, b.ev(PRESSES)])
            # an open menu owns the key: with the focus on the page (not on a row, which Space activates itself)
            b.ev(BODY)
            b.ev('window.__posts.length = 0')
            menu_open = b.ev("!document.getElementById('fx-menu').hidden")
            b.press(' ')
            b.sleep(0.3)
            check(f'  ({theme}) with the menu open and the focus on the page space presses nothing',
                  menu_open is True and b.ev(PRESSES) == [], [menu_open, b.ev(PRESSES)])
            b.press('Escape')
            b.ev(BODY)
            # an open picker owns it too
            b.press('T')
            b.ev(BODY)
            b.ev('window.__posts.length = 0')
            picker = b.ev("!!document.getElementById('picker')")
            b.press(' ')
            b.sleep(0.3)
            check(f'  ({theme}) with the picker open space presses nothing', picker is True and b.ev(PRESSES) == [], [picker, b.ev(PRESSES)])
            b.press('Escape')
            b.ev(BODY)
            if theme == 'nightdrive':
                # and an open sheet (a modal dialog), the focus on the dialog itself
                b.ev("document.getElementById('nd-sheet').click()")
                b.sleep(0.4)
                b.ev("document.getElementById('nd-dialog').focus()")
                b.ev('window.__posts.length = 0')
                dialog = b.ev("!!document.getElementById('nd-dialog') && document.activeElement === document.getElementById('nd-dialog')")
                b.press(' ')
                b.sleep(0.3)
                check('  (nightdrive) with the sheet open space presses nothing', dialog is True and b.ev(PRESSES) == [], [dialog, b.ev(PRESSES)])
                b.press('Escape')
                b.ev(BODY)
            # a text field types
            b.ev("""(() => { const i = document.createElement('input'); i.id = 'probe-field';
              document.body.append(i); i.focus(); return 1; })()""")
            b.press(' ')
            b.sleep(0.3)
            typed = b.ev("document.getElementById('probe-field').value")
            check(f'  ({theme}) space in a text field types a space and posts no press',
                  typed == ' ' and b.ev(PRESSES) == [], [typed, b.ev(PRESSES)])
        finally:
            b.close()
finally:
    dac.stop()

off = common.Daemon()
try:
    for theme in THEMES:
        b = make(off, theme)
        try:
            b.press(' ')
            b.sleep(0.4)
            check(f'space_with_no_player_posts_nothing ({theme})', b.ev(PRESSES) == [], b.ev(PRESSES))
        finally:
            b.close()
finally:
    off.stop()

print(f'space_key.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
