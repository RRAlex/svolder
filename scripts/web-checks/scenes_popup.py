# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Scenes popup (core/popups/scenes/) in a browser, wide and on a phone, in every theme, against the
real daemon's scenes (fake devices, the fake host's computer): the logic is node's (node/core-scenes.mjs);
this proves what only a browser and the daemon's own answers show. A device `Hall` is configured and never on
the air (not connected); `Gungnir 2` is on the air in the first daemon and is taken off it in the second, so
the second daemon's scenes hold a part that is away. Per theme and size:
  the_menu_row_opens_the_popup     a writer's page model holds `scenes`, the menu has `Scenes…` after Sound and
                                   a real click opens the popup; nothing scrolls sideways; every button that
                                   shows is at least 44 px
  the_list_is_the_daemons          a row for each scene, the chips from the daemon's parts, the line under the
                                   field names Hall as not connected and says no volume, gain, mute or power is saved
  a_recall_lights_the_row          a real click on a name recalls: the answer line, the row lit from the view
  the_event_moves_the_lamp         another front end (a request from the check) recalls another scene: the lamp
                                   moves, the answer line stays
  a_save_names_what_it_left_out    a typed name and Enter: `Saved …` and the daemon's note for Hall; the list gets
                                   the scene by the event; a name taken asks `Save over …?`
  the_keyboard_and_the_focus       Enter on a pencil, a bin, a taken save: Escape closes the ask and the focus is on
                                   what opened it; an answer leaves the focus inside the popup
  a_scene_gone_elsewhere           a delete question open, the scene deleted by another front end, a yes: "That scene
                                   is gone." and the list is the daemon's
  the_away_part                    (second daemon) a dashed chip `… · not connected`, a recall that only left that
                                   part says `… recalled.` with the daemon's note, the reading line and the dashed
                                   chip are 4.5:1 against the box
  a_guest_and_writes_off_have_none  (wide) a guest's daemon and one with `writes = false`: no `scenes` in the page
                                   model, no row, the router opens nothing
With WEB_CHECK_SHOTS=<folder> a picture of the popup, `<theme>-scenes-<wide|phone>.png`, is written there.
Usage: python3 scenes_popup.py chromium|firefox"""
import json
import os
import sys
import time
import urllib.error
import urllib.request

import common
import modal
from modal import THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, ready, rects

browser = modal.browser

HALL = ('Hall', '1111111111111111')
ROWS = ("[...document.querySelectorAll('#popup-scenes-list .sc-row')]"
        ".map((r) => ({ name: r.dataset.name, on: r.classList.contains('on') }))")
NAMES = "[...document.querySelectorAll('#popup-scenes-list .sc-row')].map((r) => r.dataset.name)"
LIT = "[...document.querySelectorAll('#popup-scenes-list .sc-row.on')].map((r) => r.dataset.name)"
SAY = "(document.getElementById('popup-scenes-say')?.textContent ?? '').replace(/\\s+/g, ' ').trim()"
FOCUS = "document.activeElement?.id ?? ''"
OVER = "[document.documentElement.scrollWidth - document.documentElement.clientWidth]"
POPUP_OVER = ("(() => { const p = document.querySelector('#popup-scenes .fp-page');"
              " return p ? p.scrollWidth - p.clientWidth : null; })()")
# every button that shows on the popup's page (`.fp-page`), with its box. The frame's own close button is left
# out: it belongs to the frame, and Signal draws it 38 px for a mouse on a wide window by design (0.36.9), 44 px on
# a coarse pointer and under 720 px.
BUTTONS = ("[...document.querySelectorAll('#popup-scenes .fp-page button')].filter((b) => b.offsetParent !== null)"
           ".map((b) => { const r = b.getBoundingClientRect();"
           " return { id: b.id, label: b.getAttribute('aria-label') || b.textContent.trim(), w: r.width, h: r.height }; })")
# the contrast of an element's text against the first opaque ground behind it
CONTRAST = """((sel) => {
  const el = document.querySelector(sel); if (!el) return null;
  const parse = (c) => { const m = c.match(/rgba?\\(([^)]+)\\)/); const v = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number);
    return { r: v[0], g: v[1], b: v[2], a: v.length > 3 ? v[3] : 1 }; };
  const lin = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; };
  const lum = (c) => 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
  let ground = null;
  for (let n = el; n; n = n.parentElement) {
    const c = parse(getComputedStyle(n).backgroundColor);
    if (c.a > 0.95) { ground = c; break; }
  }
  if (!ground) return null;
  const fg = parse(getComputedStyle(el).color);
  const mix = { r: fg.r * fg.a + ground.r * (1 - fg.a), g: fg.g * fg.a + ground.g * (1 - fg.a), b: fg.b * fg.a + ground.b * (1 - fg.a) };
  const a = lum(mix), b = lum(ground);
  return Math.round(((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)) * 100) / 100;
})(%s)"""


def api(d, path, body=None):
    """The scenes' routes from the check, as another front end's request from the page's own origin."""
    req = urllib.request.Request(
        f'{d.url}api/v1/scenes{path}', data=None if body is None else json.dumps(body).encode(),
        method='GET' if body is None else 'POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    try:
        with urllib.request.urlopen(req, timeout=10) as answer:
            return answer.status, json.load(answer)
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b'{}')


def reset(d, names=('Late night', 'Desk', 'Movie')):
    """The daemon holds exactly these scenes, saved from the settings as they are."""
    _, view = api(d, '')
    for scene in view.get('scenes', []):
        api(d, '/delete', {'name': scene['name']})
    for name in names:
        api(d, '/save', {'name': name})


def js(text):
    return json.dumps(text)


def row_button(name, act):
    return (f"[...document.querySelectorAll('#popup-scenes-list .sc-row')].find((r) => r.dataset.name === {js(name)})"
            f"?.querySelector('[data-act=\"{act}\"]:not(.sc-recall)')")


def click_row(b, name, act):
    """A real click on the row's button, scrolled into view first."""
    b.page(f"({row_button(name, act)}.scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.25)
    x, y = b.page(f"(() => {{ const r = {row_button(name, act)}.getBoundingClientRect();"
                  " return [r.left + r.width / 2, r.top + r.height / 2]; })()")
    b.click(x, y)


def focus_row(b, name, act):
    b.page(f"({row_button(name, act)}.focus(), 1)")


def type_in(b, selector, text):
    b.page(f"(() => {{ const f = document.querySelector({js(selector)}); f.focus(); f.value = {js(text)};"
           " f.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()")


def says(b, start, secs=6.0):
    return poll(lambda: b.page(SAY).startswith(start), secs)


def shot(b, theme, size):
    folder = os.environ.get('WEB_CHECK_SHOTS')
    if folder:
        time.sleep(0.5)
        with open(os.path.join(folder, f'{theme}-scenes-{size}.png'), 'wb') as out:
            out.write(b.shot())


def opened(b, tag):
    got = by_the_menu(b, 'scenes')
    check(f'the_menu_row_opens_the_popup {tag}: a click on Scenes… opens it', got is True, got)
    return got is True


def writer(b, d, theme, size):
    tag = f'{theme} {size}'
    reset(d)
    load(b, d, theme)
    model = poll(lambda: b.page("__core.model.page().scenes?.scenes?.length ?? null"), 8.0)
    check(f'the_menu_row_opens_the_popup {tag}: the page model holds the scenes', model == 3, model)
    b.click(*centre(b, '#fx-button'))
    poll(lambda: b.page("!document.getElementById('fx-menu').hidden"), 3.0)
    rows = b.page("[...document.querySelectorAll('#fx-menu [id^=\"menu-popup-\"]')].map((r) => r.id)")
    check(f'the_menu_row_opens_the_popup {tag}: the row is after Sound and before PipeWire config',
          rows == ['menu-popup-devices', 'menu-popup-sound', 'menu-popup-scenes', 'menu-popup-setup',
                   'menu-popup-help'], rows)
    b.press('Escape')
    time.sleep(0.3)
    if not opened(b, tag):
        return
    check(f'the_menu_row_opens_the_popup {tag}: the title is Scenes',
          b.page("document.getElementById('popup-scenes-title')?.textContent") == 'Scenes')
    check(f'the_menu_row_opens_the_popup {tag}: nothing scrolls sideways',
          b.page(OVER) == [0] and b.page(POPUP_OVER) == 0, [b.page(OVER), b.page(POPUP_OVER)])
    small = [x for x in b.page(BUTTONS) if x['h'] < 43.5 or x['w'] < 43.5]
    check(f'the_menu_row_opens_the_popup {tag}: every button that shows is at least 44 px', small == [], small)
    names = b.page(NAMES)
    check(f'the_list_is_the_daemons {tag}: a row for each scene, in the file\'s order',
          names == ['Late night', 'Desk', 'Movie'], names)
    chips = b.page("[...document.querySelectorAll('#popup-scenes-list .sc-row')[0].querySelectorAll('.fp-tag')]"
                   ".map((c) => c.textContent)")
    check(f'the_list_is_the_daemons {tag}: the chips are the scene\'s parts', chips == ['computer', 'Gungnir 2'], chips)
    hint = b.page("document.getElementById('popup-scenes-hint')?.textContent.trim()")
    check(f'the_list_is_the_daemons {tag}: the line under the field names Hall and what is never saved',
          hint.startswith('Saves the computer and Gungnir 2. Hall is not connected and is left out.')
          and hint.endswith('No volume, gain, mute or power is saved.'), hint)
    check(f'the_list_is_the_daemons {tag}: the no-volume sentence stands once',
          b.page("document.getElementById('popup-scenes-page').textContent")
          .count('No volume, gain, mute or power is saved.') == 1)

    # a recall by a real click
    click_row(b, 'Desk', 'recall')
    got = says(b, 'Desk recalled.')
    check(f'a_recall_lights_the_row {tag}: the answer says it', got is True, b.page(SAY))
    lit = poll(lambda: b.page(LIT) == ['Desk'], 6.0)
    check(f'a_recall_lights_the_row {tag}: the row is lit from the view', lit is True, b.page(ROWS))
    # another front end recalls another scene: the lamp moves, the answer stays
    status, _ = api(d, '/recall', {'name': 'Late night'})
    moved = poll(lambda: b.page(LIT) == ['Late night'], 6.0)
    check(f'the_event_moves_the_lamp {tag}: the lamp follows the other front end', status == 200 and moved is True,
          [status, b.page(ROWS)])
    check(f'the_event_moves_the_lamp {tag}: the answer line stays', b.page(SAY).startswith('Desk recalled.'), b.page(SAY))

    shot(b, theme, size)
    # a save: the daemon's note for Hall
    type_in(b, '#popup-scenes-name', 'Evening')
    b.press('Enter')
    got = says(b, 'Saved Evening.')
    check(f'a_save_names_what_it_left_out {tag}: Saved, and the note for Hall',
          got is True and b.page(SAY) == 'Saved Evening. Hall is not connected: it is not in the scene.', b.page(SAY))
    listed = poll(lambda: 'Evening' in b.page(NAMES), 6.0)
    check(f'a_save_names_what_it_left_out {tag}: the scene arrives by the event', listed is True, b.page(NAMES))
    check(f'a_save_names_what_it_left_out {tag}: the field is empty again',
          b.page("document.getElementById('popup-scenes-name').value") == '')

    # a taken name asks in place; Escape closes the ask and the focus is on the field
    type_in(b, '#popup-scenes-name', 'Evening')
    b.press('Enter')
    asked = poll(lambda: b.page("document.querySelector('#popup-scenes-over .fi-question')?.textContent") ==
                 'Save over “Evening”?', 6.0)
    check(f'a_save_names_what_it_left_out {tag}: a taken name asks Save over', asked is True,
          b.page("document.getElementById('popup-scenes-over').textContent"))
    b.press('Escape')
    poll(lambda: b.page("!document.querySelector('#popup-scenes-over .fi-question')"), 4.0)
    check(f'the_keyboard_and_the_focus {tag}: Escape closes the save question, not the popup, focus on the field',
          b.page("!document.querySelector('#popup-scenes-over .fi-question')") is True
          and b.page("!!document.getElementById('popup-scenes')") is True
          and b.page(FOCUS) == 'popup-scenes-name', [b.page(FOCUS)])
    b.press('Enter')
    poll(lambda: b.page("!!document.querySelector('#popup-scenes-over .fi-question')"), 4.0)
    click_answer(b, '#popup-scenes-over', 'overwrite')
    got = says(b, 'Saved Evening.')
    check(f'a_save_names_what_it_left_out {tag}: Save over saves', got is True, b.page(SAY))
    api(d, '/delete', {'name': 'Evening'})
    poll(lambda: 'Evening' not in b.page(NAMES), 6.0)

    # rename: the pencil by the keyboard; Escape gives the focus back; then a rename
    focus_row(b, 'Desk', 'rename')
    pencil = b.page(FOCUS)
    b.press('Enter')
    form = poll(lambda: b.page("!!document.querySelector('#popup-scenes-list form input')"), 4.0)
    check(f'the_keyboard_and_the_focus {tag}: Enter on the pencil opens the name field', form is True, pencil)
    b.press('Escape')
    poll(lambda: b.page("!document.querySelector('#popup-scenes-list form')"), 4.0)
    check(f'the_keyboard_and_the_focus {tag}: Escape closes it and the focus is on the pencil',
          b.page(FOCUS) == pencil and b.page("!!document.getElementById('popup-scenes')") is True,
          [pencil, b.page(FOCUS)])
    b.press('Enter')
    poll(lambda: b.page("!!document.querySelector('#popup-scenes-list form input')"), 4.0)
    b.page("(() => { const f = document.querySelector('#popup-scenes-list form input'); f.value = 'Desk, speakers';"
           " f.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()")
    b.press('Enter')
    got = says(b, 'Desk is now Desk, speakers.')
    check(f'the_keyboard_and_the_focus {tag}: Enter renames', got is True, b.page(SAY))
    poll(lambda: 'Desk, speakers' in b.page(NAMES), 6.0)
    inside = b.page("document.getElementById('popup-scenes').contains(document.activeElement)")
    check(f'the_keyboard_and_the_focus {tag}: the focus stays in the popup after a rename', inside is True, b.page(FOCUS))

    # delete: the bin by the keyboard; Escape; then the question answered
    focus_row(b, 'Movie', 'delete')
    bin_id = b.page(FOCUS)
    b.press('Enter')
    q = poll(lambda: b.page("document.querySelector('#popup-scenes-list .fi-question')?.textContent"), 4.0)
    check(f'the_keyboard_and_the_focus {tag}: Enter on the bin asks in the row',
          q == 'Delete “Movie”? The sound now playing does not change.', q)
    b.press('Escape')
    poll(lambda: b.page("!document.querySelector('#popup-scenes-list .fi-question')"), 4.0)
    check(f'the_keyboard_and_the_focus {tag}: Escape closes it and the focus is on the bin', b.page(FOCUS) == bin_id,
          [bin_id, b.page(FOCUS)])

    # a delete question open and the scene deleted elsewhere: the answer is "gone", the list the daemon's
    click_row(b, 'Movie', 'delete')
    poll(lambda: b.page("!!document.querySelector('#popup-scenes-list .fi-question')"), 4.0)
    status, _ = api(d, '/delete', {'name': 'Movie'})
    click_answer(b, '#popup-scenes-list', 'delete')
    got = says(b, 'That scene is gone.')
    check(f'a_scene_gone_elsewhere {tag}: the answer says it is gone (daemon status {status})',
          status == 200 and got is True, [status, b.page(SAY)])
    listed = poll(lambda: 'Movie' not in b.page(NAMES), 6.0)
    check(f'a_scene_gone_elsewhere {tag}: the list is the daemon\'s', listed is True, b.page(NAMES))
    close_popup(b, 'scenes')


def click_answer(b, within, answer):
    b.page(f"(document.querySelector('{within} [data-answer=\"{answer}\"]').scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.25)
    x, y = b.page(f"(() => {{ const r = document.querySelector('{within} [data-answer=\"{answer}\"]')"
                  ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()")
    b.click(x, y)
    poll(lambda: b.page(f"!document.querySelector('{within} [data-answer=\"{answer}\"]')"), 4.0)


def away(b, d, theme, size):
    """The second daemon: Gungnir 2 is off the air, so its part of every scene is away."""
    tag = f'{theme} {size}'
    load(b, d, theme)
    poll(lambda: b.page("__core.model.page().scenes?.scenes?.length ?? null") == 3, 8.0)
    if not opened(b, f'{tag} (away)'):
        return
    chips = poll(lambda: b.page("[...document.querySelectorAll('#popup-scenes-list .sc-row')[0].querySelectorAll('.fp-tag')]"
                                ".map((c) => c.textContent)") == ['computer', 'Gungnir 2 · not connected'], 6.0)
    check(f'the_away_part {tag}: a part that is away is a chip that says so', chips is True,
          b.page("document.querySelector('#popup-scenes-list .sc-row').textContent"))
    dashed = b.page("getComputedStyle(document.querySelector('#popup-scenes-list .sc-off')).borderStyle")
    check(f'the_away_part {tag}: and its edge is dashed', dashed == 'dashed', dashed)
    ratio = b.page(CONTRAST % js('#popup-scenes-list .sc-off'))
    check(f'the_away_part {tag}: the dashed chip reads 4.5:1 against the box', ratio is not None and ratio >= 4.5, ratio)
    click_row(b, 'Movie', 'recall')
    got = says(b, 'Movie recalled.')
    say = b.page(SAY)
    check(f'the_away_part {tag}: a recall that only left the away part says recalled, with the daemon\'s note',
          got is True and 'Gungnir 2 is not connected' in say and 'in part' not in say, say)
    # the scene that is on is not settled: no lamp and the dim line
    b.page("(() => { const m = __core.model; m.emit('scenes', { ...m.page().scenes, known: false }); return 1; })()")
    poll(lambda: b.page("document.getElementById('popup-scenes-reading')?.textContent.trim()") == 'reading…', 4.0)
    reading = b.page("document.getElementById('popup-scenes-reading')?.textContent.trim()")
    check(f'the_away_part {tag}: an unsettled view says reading… and lights no row',
          reading == 'reading…' and b.page(LIT) == [], [reading, b.page(LIT)])
    ratio = b.page(CONTRAST % js('#popup-scenes-reading .sc-reading'))
    check(f'the_away_part {tag}: the reading line reads 4.5:1 against the box', ratio is not None and ratio >= 4.5, ratio)
    b.page("(() => { const m = __core.model; m.emit('scenes', { ...m.page().scenes, known: true }); return 1; })()")
    shot(b, theme, f'{size}-away')
    close_popup(b, 'scenes')


def none(b, d, theme, who):
    load(b, d, theme)
    time.sleep(0.5)
    has = b.page("'scenes' in __core.model.page()")
    check(f'a_guest_and_writes_off_have_none {theme}, {who}: no `scenes` in the page model', has is False, has)
    b.click(*centre(b, '#fx-button'))
    poll(lambda: b.page("!document.getElementById('fx-menu').hidden"), 3.0)
    row = b.page("!!document.getElementById('menu-popup-scenes')")
    check(f'a_guest_and_writes_off_have_none {theme}, {who}: no row in the menu', row is False, row)
    b.press('Escape')
    time.sleep(0.3)
    b.page("(__core.router.open('scenes'), 1)")
    time.sleep(0.6)
    check(f'a_guest_and_writes_off_have_none {theme}, {who}: the router opens nothing',
          b.page("__core.router.state().popup") is None and b.page("!document.getElementById('popup-scenes')") is True,
          b.page("__core.router.state()"))


def main():
    d = common.Daemon(host='fake', more=[HALL])
    g = common.Daemon(host='fake', more=[HALL])
    try:
        reset(g)
        g.drop()
        time.sleep(1.5)
        for theme in THEMES:
            for size, label in ((DESK, 'wide'), (PHONE, 'phone')):
                b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
                try:
                    b.preload(modal.HOOK)
                    writer(b, d, theme, label)
                    away(b, g, theme, label)
                finally:
                    b.close()
    finally:
        d.stop()
        g.stop()
    for who, web in (('a guest', 'guest = "read"'), ('writes off', 'auth = "none"\nwrites = false')):
        n = common.Daemon(host='fake', web=web)
        try:
            for theme in THEMES:
                b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
                try:
                    b.preload(modal.HOOK)
                    none(b, n, theme, who)
                    status, _ = api(n, '')
                    check(f'a_guest_and_writes_off_have_none {theme}, {who}: the list route refuses ({status})',
                          status in (401, 403), status)
                finally:
                    b.close()
        finally:
            n.stop()
    print(f'scenes_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
