# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A popup's frame (core/popups/) in a browser, in every theme: what no node harness can see. One
loop over the themes and over the popups of `POPUPS`; a popup is a row of data there, never a
loop of its own. What a popup draws and what its wire does are node's (node/core-popups.mjs);
the stack's own rules (who owns a key, the history) are modal.py's, whose helpers this imports.

Per theme and popup, at 1280 px:
  nothing_of_the_popup_is_loaded_before_it_is_opened   no request under the popup's folder
                                        before the first open, one after it
  the_popup_is_on_top_of_the_theme      the box is what stands at its own centre and the veil
                                        at the window's corner (`elementFromPoint`: over Night
                                        drive's canvas, Ember's smoke and the menu's button)
  the_page_behind_is_inert              the theme and the frame are inert, the toasts are not
  tab_stays_inside                      Tab goes round the popup's controls and never leaves it:
                                        at the list, and at the last page, where the way back
                                        is the last control in the markup and is not shown
  the_list_and_the_page_stand_side_by_side   in a wide window, the box smaller than the window
  the_colours_are_the_themes_tokens     the box's ground is `--frame-surface` and the title's
                                        colour `--frame-ink` as the theme sets them, in each
                                        scheme the theme has
  nothing_scrolls_sideways              no horizontal overflow of the page or of the box
  the_box_opens_with_no_animation       the box's entry animation is a theme's to set
                                        (`--frame-popup-in`) and none sets it
  an_entry_animation_runs_once_an_opening   on a page loaded afresh, with the token set by
                                        the check to a long animation: the popup's first open
                                        (its loading line, then its module arriving) starts
                                        the animation once and it is still running on the box
                                        when the popup is drawn; a step to a page starts
                                        nothing; a second open starts it once more
  a_list_opens_inside_the_box           (a popup with a list) the panel of a list opened from
                                        the page pane stands inside the box and inside what
                                        the pane shows, next to its chip and on top at its own
                                        centre; a pick closes it, keeps the popup and is
                                        shown, with the focus on the chip; in a pane made to
                                        scroll the panel is no taller than the pane shows,
                                        goes with its chip when the pane is scrolled, and
                                        Escape closes it alone
  escape_closes_the_ask_then_the_popup  (a popup with an inline ask) Escape in the ask closes
                                        the ask alone, a second Escape the popup; the focus is
                                        back where it was and the shown device is the same
  a_drag_that_ends_on_the_veil_keeps_the_popup   (a popup with a name form) a press in the
                                        form's field that is let go on the veil leaves the
                                        popup open with what the field held, and so does a
                                        press on the veil let go in the box; a click on the
                                        veil closes it
  the_page_does_not_scroll_under_the_veil   the veil is a scroll container that contains
                                        its overscroll; in Chromium a wheel over it leaves the
                                        page where it was scrolled to, in each theme whose
                                        page scrolls at this size (one at least). Firefox
                                        keeps `overscroll-behavior` in its compositor, which
                                        a WebDriver wheel does not pass through (it chains out
                                        of a box that overflows too), so there the sheet's
                                        values are the proof
  back_closes_it                        Back closes it and reads no device
  a_popup_opens_from_a_tap_or_a_key     the router says nothing in the console through the
                                        whole run: the check opens a popup by a real click (on
                                        a button it adds to the page for that click, since the
                                        sample has no menu row and no key) and steps by a real
                                        click on the entry; at the end, an open by script with
                                        no tap behind it is said
and at 390 px:
  on_a_phone_the_popup_fills_the_screen   the box is the window; at the list the page pane is
                                        not shown, at a page the list is not and the way back
                                        is; the page behind is inert; the focus is on the
                                        current entry at the list, on the way back after a
                                        step to a page and on the list again after the way
                                        back, never outside the popup; Tab at the list goes
                                        round past the page pane's controls, which are not
                                        shown; no horizontal overflow

Devices (`devices`), a writer's popup, has cases of its own (its `extra`), in every theme, on a
daemon with two configured devices, the second never connected:
  the_overview_has_every_device_with_its_key_graphs   the entries (Overview, each device with
                                        its dot, the tag on the shown one); both devices'
                                        blocks in order, the signal first, every graph an image
                                        with a text alternative or a line of words; the device
                                        that is not connected dimmed by `--frame-dim`, its
                                        opacity 1; nothing scrolls sideways with the graphs
  devices_names_its_pages_in_the_tab_title   `Devices` at the list, `Devices · Second` at a page
  show_on_the_page_switches_the_device_and_the_popup_stays   by real clicks: the second
                                        device's entry, then *Show on the page*: the shown
                                        device and the page behind change, the popup and its
                                        page stand, no entry is pushed, the tag moves, the page
                                        says it is the one shown; and back the same way
  back_steps_page_list_closed           after the switch: the list, then closed, on the device
                                        now shown
  devices_opens_by_its_menu_row         a real click on the menu's button and on `Devices…`
  an_event_of_another_device_leaves_the_focus_where_it_is   with the focus on *Show on the
                                        page*, the other device's model and link arrive (the
                                        store's `setDevice` and `applyPart`, as the stream
                                        calls them): the focus stays on the same button
  a_guest_reads_the_devices_popup       on the guest's daemon: the row is there and an open by the
                                        router shows the popup read-only (no settings), pushes one
                                        entry and never asks the writer's list. On the writer's:
                                        Forward onto the popup's entry of this load shows it
                                        again, and once the page's viewer is a guest's (the model
                                        replaced) the same entry opens the popup read-only and
                                        asks no list
With WEB_CHECK_SHOTS=<folder> a picture of the Overview over each theme is written there.

Help (`help`), the page's own popup, has cases of its own beside those (its `extra`), in every
theme, for a signed-in writer and, on a second daemon, for a guest:
  help_opens_by_its_menu_row_and_by_the_question_mark   a real click on the menu's button and
                                        on the row `Help…` opens it; `?` opens it with the
                                        focus on the page and from the open menu, which closes;
                                        a second `?` does nothing (no entry of the history);
                                        in a field `?` types; a guest has the row and the key
                                        and reads why About has no version
  help_names_its_pages_in_the_tab_title   `Help` at the list, `Help · Everywhere` at that page
  back_steps_page_list_closed           a page entered by a click, then Back twice: the list,
                                        then closed
  a_page_with_no_control_scrolls_by_the_keyboard   Here holds no control: Tab stays inside the
                                        popup; in a box made low the pane is a region named
                                        after its entry that Tab reaches and End scrolls, and
                                        it is no stop again once it shows all it holds
  here_is_the_widget_that_had_the_focus   on a page loaded afresh, Tab (the real key) is
                                        pressed until the focus is on a control in a zone of the
                                        theme's table; `?` then opens Help for the first time
                                        on that page (its code arrives after the page went
                                        inert) and Here's first rows are every listed entry of
                                        the zones that hold that control, read from the control
                                        itself and not from the frame's last focus, with no
                                        line saying the focus is nowhere; Everywhere and back
                                        to Here shows the same rows; after Escape the focus is
                                        on the control again, Tab goes on to a control of
                                        another zone, and `?` shows that one's rows. A theme
                                        whose page has no such control outside a sheet of its
                                        own (Night drive) has no case and says so
  a_click_on_empty_ground_clears_here   with a control of the theme focused, a click on empty
                                        ground leaves the frame no last focus and Here says so
The first open of Help in a theme is by the check's own button like the sample's (the request
count needs a page that never opened it); with WEB_CHECK_SHOTS=<folder> a picture of Here over
each theme, a widget of the theme focused, is written there for a person to look at.

The per-popup hook: an entry of `POPUPS`, `name: { ... }` with
  title    the words of its title
  folder   the path under `assets/` that holds the popup's own files (`core/popups/<name>/`)
  add      a JavaScript expression that adds the popup to `__core.popups`, or None for a popup
           the page adds itself (every real one)
  entries  the ids of its side list's entries, in order ([]: a popup with no list)
  ask      the id of a control on its first page that opens an inline ask, or None
  form     the id of a control on its first page that opens a name form, or None
  list     the id of a chip on its first page that opens a list (core/list.js), or None
  extra    a function `(b, d, tag, theme)` for what only this popup has, or None: `b` is the
           browser with the popup open at 1280 px, `d` the daemon (so it can load another page of
           it, a guest's)
Usage: python3 popups.py chromium|firefox"""
import json
import os
import sys
import time

import common
import modal
from modal import AT, BODY, ENTRY, HOOK, INERT, STATE, THEMES, check, load, poll

if not common.LIVE_ASSETS:
    sys.exit('popups.py needs the tree\'s files (the sample popup is not in the binary): '
             'unset SMOKE_EMBEDDED and SVOLDERD')

browser = modal.browser
PHONE = (390, 800)
DESK = (1280, 800)


def sample_extra(b, d, tag, theme):
    """The sample's live line names the device it follows."""
    name = b.page("__core.model.page().fleet.devices.find((d) => d.url === __core.model.shown()).name")
    live = b.page("document.getElementById('popup-sample-live')?.textContent ?? null")
    check(f'{tag}: the live line names the device it follows', isinstance(live, str) and live.startswith(f'{name}: '),
          [name, live])


# the rows Help shows: their entries' ids, and the note over them
HELP_ROWS = ("""(() => { const pop = document.getElementById('popup-help'); if (!pop) return null;
  return { ids: [...pop.querySelectorAll('.fp-page [data-key]')].map((r) => r.dataset.key),
    zones: [...new Set([...pop.querySelectorAll('.fp-page [data-zone]')].map((r) => r.dataset.zone))],
    note: pop.querySelector('.fp-page > .fp-note')?.textContent ?? null,
    headings: [...pop.querySelectorAll('.fp-heading')].map((h) => h.textContent),
    controls: pop.querySelectorAll('.fp-page a, .fp-page button:not(.fp-back), .fp-page input').length }; })()""")
# the pane's marks as a stop of the keyboard
PANE = ("""(() => { const p = document.querySelector('#popup-help .fp-page');
  return { tab: p.getAttribute('tabindex'), role: p.getAttribute('role'),
    name: document.getElementById(p.getAttribute('aria-labelledby') ?? '')?.textContent ?? null,
    over: p.scrollHeight - p.clientHeight, top: p.scrollTop, focus: document.activeElement === p }; })()""")
# a point of the theme with nothing to press under it, the bottom of the window first
GROUND = ("""(() => { const theme = document.getElementById('theme');
  const busy = 'button, a, input, select, textarea, label, summary, [tabindex], [role], [contenteditable], sv-knob, sv-slider';
  for (let y = innerHeight - 5; y > 0; y -= 9) for (let x = 5; x < innerWidth; x += 17) {
    const at = document.elementFromPoint(x, y);
    if (at && theme.contains(at) && !at.closest(busy)) return [x, y]; }
  return null; })()""")
# give the focus to the first element of the theme that takes it and is in a zone of its table
# (`true`), else to its first control; what has it
WIDGET = ("""((zoned) => { const table = __core.keys.listed(__core.frame.keyTable());
  const zones = [...new Set(table.map((e) => e.zone))].filter((z) => z !== 'page');
  const inner = 'button, [tabindex="0"], input';
  const picks = zoned ? zones.flatMap((z) => [...document.querySelectorAll(z)])
      .flatMap((el) => [el, ...el.querySelectorAll(inner)])
    : [...document.querySelectorAll('#theme button, #theme [tabindex="0"], #theme a[href]')];
  for (const el of picks) { el.focus(); if (document.activeElement === el) return el.id || el.className || el.tagName; }
  return null; })""")


# what the control that has the focus is to the theme's table: the zones that hold it and the ids
# of their listed entries, in table order (`null` with the focus in no zone)
ZONED = ("""(() => { const el = document.activeElement;
  if (!el || !document.getElementById('theme').contains(el)) return null;
  const table = __core.keys.listed(__core.frame.keyTable());
  const zones = [...new Set(table.map((e) => e.zone))].filter((z) => z !== 'page' && el.closest(z));
  if (zones.length === 0) return null;
  return { zones, ids: table.filter((e) => zones.includes(e.zone)).map((e) => e.id),
    page: [...table.filter((e) => e.zone === 'page'), ...__core.keys.listed(__core.keys.frame)].map((e) => e.id),
    at: el.id || el.className || el.tagName }; })()""")
# Here's rows as `[id, zone]`, with the note over them
HERE_ROWS = ("""(() => { const pop = document.getElementById('popup-help');
  if (pop?.querySelector('.fp-box')?.dataset.state !== 'ready') return null;
  return { rows: [...pop.querySelectorAll('.fp-page [data-key]')].map((r) => [r.dataset.key, r.dataset.zone]),
    note: pop.querySelector('.fp-page > .fp-note')?.textContent ?? null, at: __core.router.state().page }; })()""")


def tab_to_a_widget(b, other=None, presses=80):
    """Press Tab until the focus is on a control in a zone (of other zones than `other`'s)."""
    for _ in range(presses):
        b.press('Tab')
        got = b.page(ZONED)
        if got and (other is None or not set(got['zones']) & set(other['zones'])):
            return got
    return None


def here_holds(rows, widget):
    """Whether Here's rows are the widget's listed entries first (in its zones), the page's keys
    last, and no line says the focus is nowhere."""
    if not rows or rows['note'] is not None:
        return False
    ids = [row[0] for row in rows['rows']]
    mine = [row[0] for row in rows['rows'] if row[1] in widget['zones']]
    return (mine == widget['ids'] and ids[:len(mine)] == mine and len(mine) > 0
            and ids[len(ids) - len(widget['page']):] == widget['page'])


def here_of_a_widget(b, d, theme):
    tag = f'here_is_the_widget_that_had_the_focus help ({theme})'
    load(b, d, theme)
    b.page(BODY)
    first = tab_to_a_widget(b)
    if first is None or b.page("!!document.querySelector('[aria-modal=true]')"):
        # Night drive's zones are its sheet's, a list's and a question's: none stands on the page
        if theme == 'nightdrive':
            print(f'note {tag}: the page of {theme} has no control in a zone outside a sheet of its own, no case')
        else:
            check(f'{tag}: Tab reaches a control in a zone of the theme\'s table', False, first)
        return
    cold = requests(b, POPUPS['help']['folder'])
    b.press('?')
    ready(b, 'help')
    rows = b.page(HERE_ROWS)
    check(f'{tag}: on its first open of the page, Here is the keys of the control that had the focus, then the page\'s',
          cold == 0 and here_holds(rows, first), [cold, first, rows])
    b.click(*centre(b, '#popup-help-entry-everywhere'))
    poll(lambda: b.page(f"{STATE}[2] === 'everywhere'"), 3.0)
    b.click(*centre(b, '#popup-help-entry-here'))
    poll(lambda: b.page(f"{STATE}[2] === 'here'"), 3.0)
    time.sleep(0.3)
    again = b.page(HERE_ROWS)
    check(f'{tag}: Everywhere and back to Here shows the same rows', here_holds(again, first)
          and again['rows'] == (rows or {}).get('rows'), [first, again])
    b.press('Escape')
    poll(lambda: b.page(f"!{el('help')}"), 4.0)
    time.sleep(0.4)
    back = b.page(ZONED)
    check(f'{tag}: after Escape the focus is on that control again', back is not None and back['at'] == first['at'],
          [first, back])
    second = tab_to_a_widget(b, other=first)
    if second is None:
        check(f'{tag}: Tab goes on to a control of another zone', False)
        return
    b.press('?')
    ready(b, 'help')
    rows = b.page(HERE_ROWS)
    check(f'{tag}: with the focus on a control of another zone, Here is that one\'s keys',
          here_holds(rows, second) and not any(row[1] in first['zones'] for row in rows['rows']), [second, rows])
    close_popup(b, 'help')


def centre(b, selector):
    return b.page(f"(() => {{ const r = document.querySelector({json.dumps(selector)}).getBoundingClientRect();"
                  " return [r.left + r.width / 2, r.top + r.height / 2]; })()")


def by_the_menu(b, name='help'):
    """Open a popup as a person does: a click on the menu's button, then on its row."""
    b.click(*centre(b, '#fx-button'))
    if poll(lambda: b.page("!document.getElementById('fx-menu').hidden"
                           f" && !!document.getElementById('menu-popup-{name}')"), 3.0) is not True:
        return False
    b.click(*centre(b, f'#menu-popup-{name}'))
    return ready(b, name)


def help_keys(b, tag, shown):
    """`?` on the page, a second one, in a field and from the open menu."""
    tag = f'help_opens_by_its_menu_row_and_by_the_question_mark {tag}'
    b.page(BODY)
    b.press('?')
    stands = ready(b, 'help')
    length = b.page('history.length')
    check(f'{tag}: ? on the page opens it at its list', stands is True and b.page(STATE) == [shown, 'help', None],
          b.page(STATE))
    b.press('?')
    time.sleep(0.4)
    check(f'{tag}: a second ? does nothing: Help stands, no entry of the history',
          b.page(f"[!!{el('help')}, history.length]") == [True, length] and b.page(STATE) == [shown, 'help', None],
          [b.page(STATE), length, b.page('history.length')])
    b.press('Escape')
    gone = poll(lambda: b.page(f"!{el('help')}"), 4.0)
    time.sleep(0.4)
    check(f'{tag}: Escape closes it', gone is True)
    b.page("(() => { const f = document.createElement('input'); f.id = 'check-field';"
           " f.style.cssText = 'position:fixed;left:40px;top:40px;z-index:99';"
           " document.getElementById('theme').append(f); f.focus(); return 1; })()")
    b.press('?')
    time.sleep(0.4)
    typed = b.page(f"[document.getElementById('check-field')?.value ?? null, !!{el('help')}]")
    check(f'{tag}: in a field ? types and opens nothing', typed == ['?', False], typed)
    b.page("(document.getElementById('check-field')?.remove(), 1)")
    b.page(BODY)
    b.page('(__core.frame.openMenu(), 1)')
    time.sleep(0.3)
    was = b.page("!document.getElementById('fx-menu').hidden")
    b.press('?')
    stands = ready(b, 'help')
    check(f'{tag}: from the open menu ? closes the menu and opens Help',
          was is True and stands is True and b.page("document.getElementById('fx-menu').hidden") is True, [was, stands])
    b.press('?')
    time.sleep(0.4)
    check(f'{tag}: and a second ? does nothing there either',
          b.page(f"[!!{el('help')}, document.getElementById('fx-menu').hidden]") == [True, True])
    close_popup(b, 'help')


def help_extra(b, d, tag, theme):
    """What only Help has; `b` has it open at its list, and has it so again at the end."""
    name = 'help'
    shown = b.page('__core.model.shown()')
    labels = b.page(f"[...{el(name)}.querySelectorAll('.fp-entry')].map((e) => e.textContent)")
    here = b.page(HELP_ROWS)
    check(f'{tag}: three pages, Here first, with the keys of the page and the frame\'s T and ?',
          labels == ['Here', 'Everywhere', 'About'] and 'themes' in here['ids'] and 'help' in here['ids']
          and here['controls'] == 0, [labels, here])
    check(f'help_names_its_pages_in_the_tab_title {tag}: Help at the list',
          ' · Help — ' in b.page('document.title'), b.page('document.title'))

    # ---- a page with no control ----
    walk = f'a_page_with_no_control_scrolls_by_the_keyboard {tag}'
    inside, names = tab_walk(b, name, 8)
    check(f'{walk}: Here has no control and eight Tabs never leave the popup', inside and len(set(names)) > 1, names)
    fits = b.page(PANE)
    b.page(f"({el(name)}.querySelector('.fp-box').style.height = '170px', 1)")
    low = poll(lambda: (lambda got: got if got['tab'] == '0' else None)(b.page(PANE)), 3.0) or b.page(PANE)
    check(f'{walk}: in a low box the pane is a region named Here that takes the focus',
          low['tab'] == '0' and low['role'] == 'region' and low['name'] == 'Here' and low['over'] > 20, [fits, low])
    reached = False
    for _ in range(8):
        b.press('Tab')
        if b.page(PANE)['focus']:
            reached = True
            break
    at_pane = b.page(AT)
    b.press('End')
    # a browser may glide to the end
    end = poll(lambda: (lambda got: got if abs(got['top'] - got['over']) <= 2 else None)(b.page(PANE)), 3.0) or b.page(PANE)
    end['at'] = [at_pane, b.page(AT)]
    check(f'{walk}: Tab reaches the pane and End scrolls it to its end',
          reached and end['top'] > 0 and abs(end['top'] - end['over']) <= 2, [reached, end])
    inside, names = tab_walk(b, name, 6)
    check(f'{walk}: and Tab still goes round inside the popup', inside and len(set(names)) > 1, names)
    b.page(f"({el(name)}.querySelector('.fp-entry.on').focus(), {el(name)}.querySelector('.fp-box').style.height = '', 1)")
    back = poll(lambda: (lambda got: got if got['tab'] is None else None)(b.page(PANE)), 3.0) or b.page(PANE)
    if fits['over'] <= 0:
        check(f'{walk}: a pane that shows all it holds is no stop', back['tab'] is None and back['role'] is None, back)

    # ---- the pages, the title, Back ----
    b.click(*centre(b, '#popup-help-entry-everywhere'))
    paged = poll(lambda: b.page(f"{STATE}[2] === 'everywhere'"), 3.0)
    time.sleep(0.3)
    every = b.page(HELP_ROWS)
    check(f'{tag}: Everywhere holds the keys of the whole page and no widget\'s',
          paged is True and every['zones'] == ['page'] and every['ids'] == [i for i in here['ids'] if i in every['ids']]
          and 'themes' in every['ids'], every)
    check(f'help_names_its_pages_in_the_tab_title {tag}: Help · Everywhere at that page',
          ' · Help · Everywhere — ' in b.page('document.title'), b.page('document.title'))
    b.page('(history.back(), 1)')
    listed_ = poll(lambda: b.page(f"{STATE}[2] === null && {STATE}[1] === 'help' && !!{el(name)}"), 4.0)
    time.sleep(0.4)
    b.page('(history.back(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.5)
    check(f'back_steps_page_list_closed {tag}', listed_ is True and gone is True
          and b.page(STATE) == [shown, None, None] and 'Help' not in b.page('document.title'),
          [listed_, gone, b.page(STATE), b.page('document.title')])

    # ---- the row and the key ----
    opened = by_the_menu(b)
    check(f'help_opens_by_its_menu_row_and_by_the_question_mark {tag}: the menu\'s row Help… opens it',
          opened is True and b.page("document.getElementById('fx-menu').hidden") is True, opened)
    b.click(*centre(b, '#popup-help-entry-about'))
    poll(lambda: b.page(f"{STATE}[2] === 'about'"), 3.0)
    time.sleep(0.3)
    about = b.page(f"(() => {{ const p = {el(name)}.querySelector('.fp-page');"
                   " return { text: p.textContent, version: __core.model.page().viewer.version ?? null, host: location.host,"
                   " links: [...p.querySelectorAll('a')].map((a) => [a.href, a.target, a.rel]) }; })()")
    check(f'{tag}: About has the version, the daemon and links that open in a new tab',
          about['version'] is not None and about['version'] in about['text'] and about['host'] in about['text']
          and len(about['links']) >= 2 and all(link[0].startswith('https://github.com/RRAlex/svolder')
                                               and link[1:] == ['_blank', 'noopener'] for link in about['links']), about)
    inside, names = tab_walk(b, name, 10)
    check(f'{tag}: at About the links are stops and Tab stays inside', inside
          and any('popup-help' not in str(n) for n in names), names)
    close_popup(b, name)
    help_keys(b, tag, shown)

    # ---- the last focus ----
    ground = f'a_click_on_empty_ground_clears_here {tag}'
    had = b.page(f'{WIDGET}(false)')
    kept = b.page('__core.frame.lastFocus() !== null')
    point = b.page(GROUND)
    if point is None:
        check(f'{ground}: the theme has empty ground at this size', False)
    else:
        b.click(*point)
        time.sleep(0.3)
        after = b.page("[__core.frame.lastFocus() === null, !!document.querySelector('[aria-modal=true]')]")
        b.page(BODY)
        b.press('?')
        ready(b, name)
        rows = b.page(HELP_ROWS)
        check(f'{ground}: a control of the theme had the focus, the click leaves none, and Here says so',
              had is not None and kept is True and after == [True, False] and rows is not None
              and (rows['note'] or '').startswith('Nothing with keys of its own has the focus')
              and 'themes' in rows['ids'], [had, kept, point, after, rows])
        close_popup(b, name)

    shots = os.environ.get('WEB_CHECK_SHOTS')
    if shots:
        b.page(f'{WIDGET}(true)')
        b.press('?')
        if ready(b, name):
            time.sleep(0.6)
            with open(os.path.join(shots, f'{theme}-here.png'), 'wb') as out:
                out.write(b.shot())
            print(f'note a picture of Here over {theme} is in {shots}')
        close_popup(b, name)
        b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    open_popup(b, name)


def guest(b, theme):
    """Help for a guest: the row, the key, what a guest's Here and About say."""
    tag = f'help_opens_by_its_menu_row_and_by_the_question_mark help ({theme}), a guest'
    cold = requests(b, POPUPS['help']['folder'])
    opened = by_the_menu(b)
    rows = b.page(HELP_ROWS)
    check(f'{tag}: the menu has the row and it opens Help, loaded then',
          cold == 0 and opened is True and rows is not None and 'themes' in rows['ids'] and 'help' in rows['ids']
          and 'play' not in rows['ids'], [cold, opened, rows])
    if opened is not True:
        return
    b.click(*centre(b, '#popup-help-entry-about'))
    poll(lambda: b.page(f"{STATE}[2] === 'about'"), 3.0)
    time.sleep(0.3)
    text = b.page(f"{el('help')}.querySelector('.fp-page').textContent")
    check(f'{tag}: About says why a guest reads no version', 'Shown once you are signed in' in text, text)
    close_popup(b, 'help')
    b.page(BODY)
    b.press('?')
    check(f'{tag}: ? opens it', ready(b, 'help') is True)
    b.press('Escape')
    gone = poll(lambda: b.page(f"!{el('help')}"), 4.0)
    time.sleep(0.4)
    check(f'{tag}: Escape closes it and nothing was posted', gone is True and b.page('window.__posts') == [],
          b.page('window.__posts'))


# what the Devices popup shows: its entries as `[page, connected, tag]`, each device's block in the
# pane, what stands in the place of Show on the page, and the frame's token for dim words
DEVICES = ("""(() => { const pop = document.getElementById('popup-devices'); if (!pop) return null;
  const probe = document.createElement('i'); probe.style.cssText = 'position:fixed;left:-99px;color:var(--frame-dim)';
  document.body.append(probe); const dim = getComputedStyle(probe).color; probe.remove();
  return { entries: [...pop.querySelectorAll('.fp-entry')].map((e) => [e.dataset.popupPage,
      e.querySelector('.fp-dot')?.classList.contains('on') ?? null, e.querySelector('.fp-tag')?.textContent ?? null]),
    blocks: [...pop.querySelectorAll('.fpd-device')].map((s) => ({ url: s.dataset.device,
      away: s.classList.contains('fpd-away'), name: s.querySelector('.fpd-name').textContent,
      state: s.querySelector('.fpd-state').textContent.trim(), graphs: s.querySelectorAll('.fpd-graph').length,
      first: s.querySelector('.fpd-graph')?.dataset.series ?? null,
      said: [...s.querySelectorAll('.fp-graph[role=img]')].map((g) => g.getAttribute('aria-label')),
      notes: s.querySelectorAll('.fpd-graph .fp-note').length, waiting: !s.querySelector('.fpd-graph'),
      opacity: getComputedStyle(s).opacity, ink: getComputedStyle(s.querySelector('.fpd-name')).color })),
    show: pop.querySelector('#popup-devices-show button') ? 'button'
      : (pop.querySelector('#popup-devices-show')?.textContent ?? null), dim }; })()""")


def devices_extra(b, d, tag, theme):
    """What only Devices has; `b` has it open at its list, and has it so again at the end. The
    daemon has two configured devices: the first connected, the second (`Second`) never."""
    name = 'devices'
    urls = b.page('__core.model.page().fleet.devices.map((d) => d.url)')
    first, second = urls[0], urls[1]
    shown = b.page('__core.model.shown()')
    # the volume moved twice, a second apart (the ring keeps one point a second), so one series
    # has a line to draw; the popup is opened again to read it
    close_popup(b, name)
    raw = b.page(f"__core.model.part('volume', {json.dumps(first)}).raw")
    for step in (raw - 2, raw):
        b.page(f"__core.actions.setVolume({step}, {{ device: {json.dumps(first)} }}).then(() => 1)")
        time.sleep(1.4)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    open_popup(b, name)
    drawn = poll(lambda: (lambda got: got if got and len(got['blocks']) == 2
                          and not any(block['waiting'] for block in got['blocks']) else None)(b.page(DEVICES)), 8.0)
    got = drawn or b.page(DEVICES)
    check(f'{tag}: Overview, then each configured device with its dot, the tag on the one shown',
          shown == first and got['entries'] == [['overview', None, None], [f'd-{first}', True, 'Shown'],
                                                [f'd-{second}', False, None], ['add', None, None]],
          got['entries'])
    blocks = got['blocks']
    on, off = (blocks + [None, None])[:2]
    check(f'the_overview_has_every_device_with_its_key_graphs {tag}: both devices in the list\'s order, the'
          ' signal first, every graph an image with words or a line of words',
          drawn is not None and [block['url'] for block in blocks] == [first, second]
          and on['graphs'] >= 2 and on['first'] == 'signal' and on['state'] == 'connected'
          and all(len(block['said']) + block['notes'] >= block['graphs'] for block in blocks)
          and all(words and ', last 30 minutes: ' in words for block in blocks for words in block['said'])
          and any((words or '').startswith('Volume, last 30 minutes: now ') and ' dB' in words
                  for words in on['said']), blocks)
    check(f'the_overview_has_every_device_with_its_key_graphs {tag}: a device that is not connected says so and'
          ' is dimmed by the frame\'s token, never by an opacity',
          drawn is not None and off['away'] and not on['away'] and off['state'].startswith('not connected')
          and off['ink'] == got['dim'] and on['ink'] != off['ink'] and off['opacity'] == '1', [on, off, got['dim']])
    check(f'devices_names_its_pages_in_the_tab_title {tag}: Devices at the list',
          ' · Devices — ' in b.page('document.title'), b.page('document.title'))
    r = rects(b, name)
    check(f'nothing_scrolls_sideways {tag}: with the graphs drawn', r['over'] == [0, 0], r['over'])
    shots = os.environ.get('WEB_CHECK_SHOTS')
    if shots:
        time.sleep(0.5)
        with open(os.path.join(shots, f'{theme}-overview.png'), 'wb') as out:
            out.write(b.shot())
        print(f'note a picture of the Overview over {theme} is in {shots}')

    # ---- a device's page and Show on the page ----
    show = f'show_on_the_page_switches_the_device_and_the_popup_stays {tag}'
    b.click(*centre(b, f'#popup-devices [data-popup-page="d-{second}"]'))
    paged = poll(lambda: b.page(f"{STATE}[2] === {json.dumps('d-' + second)}"), 3.0)
    time.sleep(0.4)
    page = b.page(DEVICES)
    check(f'devices_names_its_pages_in_the_tab_title {tag}: Devices · Second at that page',
          paged is True and ' · Devices · Second — ' in b.page('document.title'), b.page('document.title'))
    check(f'{show}: the page of a device that is not shown names it and offers the switch',
          [block['url'] for block in page['blocks']] == [second] and page['show'] == 'button', page)
    # the button has the focus and events of the other device arrive, as the stream hands them
    # to the store: its model whole, then its link
    b.page("(document.getElementById('popup-devices-show-button').focus(),"
           " document.getElementById('popup-devices-show-button').__kept = 1, 1)")
    b.page(f"(__core.model.setDevice({json.dumps(first)}, {{ ...__core.model.device({json.dumps(first)}) }}),"
           f" __core.model.applyPart({json.dumps(first)}, 'link', {{ ...__core.model.part('link', {json.dumps(first)}) }}), 1)")
    time.sleep(0.4)
    kept = b.page("[document.activeElement?.id ?? null, document.getElementById('popup-devices-show-button')?.__kept ?? null]")
    check(f'an_event_of_another_device_leaves_the_focus_where_it_is {tag}: the focus stays on Show on the page,'
          ' the same element', kept == ['popup-devices-show-button', 1], kept)
    length = b.page('history.length')
    b.click(*centre(b, '#popup-devices-show button'))
    moved = poll(lambda: b.page(f'__core.model.shown() === {json.dumps(second)}'), 6.0)
    time.sleep(0.6)
    after = b.page(DEVICES)
    state = b.page(f"[{STATE}, history.length, !!{el(name)}, location.search, document.title,"
                   f" {el(name)}.contains(document.activeElement), {INERT}]")
    check(f'{show}: the shown device is the page\'s, the popup and its page stand, no entry is pushed',
          moved is True and state[0] == [second, name, f'd-{second}'] and state[1] == length and state[2] is True
          and state[3].endswith(f'device={second}') and state[5] is True and state[6] == [True, True, False],
          [moved, state, length])
    check(f'{show}: the tag moved, the page says it is the one shown, and the page behind changed',
          after['entries'] == [['overview', None, None], [f'd-{first}', True, None], [f'd-{second}', False, 'Shown'],
                                    ['add', None, None]]
          and after['show'] == 'This is the device the page shows.' and state[4].startswith('Second'),
          [after['entries'], after['show'], state[4]])
    b.page('(history.back(), 1)')
    listed_ = poll(lambda: b.page(f"{STATE}[2] === null && {STATE}[1] === 'devices' && !!{el(name)}"), 4.0)
    time.sleep(0.4)
    b.page('(history.back(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.5)
    check(f'back_steps_page_list_closed {tag}: on the device now shown', listed_ is True and gone is True
          and b.page(STATE) == [second, None, None] and b.page('__core.model.shown()') == second
          and 'Devices' not in b.page('document.title'), [listed_, gone, b.page(STATE), b.page('document.title')])

    # ---- the row, and the switch back ----
    opened = by_the_menu(b, name)
    check(f'devices_opens_by_its_menu_row {tag}: the menu\'s row Devices… opens it, above Help…',
          opened is True and b.page("document.getElementById('fx-menu').hidden") is True, opened)
    b.click(*centre(b, f'#popup-devices [data-popup-page="d-{first}"]'))
    poll(lambda: b.page(f"{STATE}[2] === {json.dumps('d-' + first)}"), 3.0)
    time.sleep(0.4)
    b.click(*centre(b, '#popup-devices-show button'))
    back = poll(lambda: b.page(f'__core.model.shown() === {json.dumps(first)}'), 6.0)
    time.sleep(0.6)
    check(f'{show}: and back to the first device the same way', back is True
          and b.page(STATE) == [first, name, f'd-{first}'] and b.page(DEVICES)['entries'][1][2] == 'Shown',
          [back, b.page(STATE)])
    close_popup(b, name)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    open_popup(b, name)


def guest_devices(b, theme):
    """Devices is everyone's: a guest has the row and the popup, read-only."""
    tag = f'a_guest_reads_the_devices_popup ({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page('(__core.frame.openMenu(), 1)')
    time.sleep(0.3)
    rows = b.page("[...document.querySelectorAll('#fx-menu [id^=\"menu-popup-\"]')].map((e) => e.id)")
    b.page('(__core.frame.closeMenu(), 1)')
    check(f'{tag}: the menu has Devices\' row, the read-only PipeWire config row and Help\'s, not the writer\'s Sound',
          rows == ['menu-popup-devices', 'menu-popup-setup', 'menu-popup-help'], rows)
    b.page("(__core.router.open('devices', 'overview'), 1)")
    stands = ready(b, 'devices')
    time.sleep(1.0)
    got = [stands, b.page(LISTS), b.page("!!document.querySelector('#popup-devices [id^=popup-devices-set-]')")]
    check(f'{tag}: an open by the router shows the popup read-only: no settings, the writer\'s list never asked',
          got == [True, 0, False], got)
    close_popup(b, 'devices')


# how often the page has asked for the configured devices
LISTS = "performance.getEntriesByType('resource').filter((e) => /\\/api\\/v1\\/devices$/.test(e.name)).length"


def lost_right(b, theme):
    """An entry of this load that names Devices shows it again for a writer, and shows it read-only
    once the page's viewer is a guest."""
    tag = f'a_guest_reads_the_devices_popup ({theme})'
    name = 'devices'
    shown = b.page('__core.model.shown()')
    b.page("(document.getElementById('fx-button').focus(), 1)")
    opened = open_popup(b, name)
    b.page('(history.back(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.4)
    b.page('(history.forward(), 1)')
    again = ready(b, name)
    b.page('(history.back(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0) and gone
    time.sleep(0.4)
    check(f'{tag}: for a writer Forward onto the entry of this load shows Devices again',
          opened is True and again is True and gone is True, [opened, again, gone])
    # the stream says the true viewer on its next model: every replace keeps the guest while the case runs
    b.page("(window.__viewer = __core.model.page().viewer, window.__replace = __core.model.replace,"
           " __core.model.replace = (page) => window.__replace({ ...page,"
           " viewer: { ...page.viewer, guest: true, writes: false } }), __core.model.replace(__core.model.page()), 1)")
    time.sleep(1.0)
    asked = b.page(LISTS)
    b.page('(history.forward(), 1)')
    got = [ready(b, name), b.page(STATE)[1], b.page(LISTS) - asked,
           b.page("!!document.querySelector('#popup-devices [id^=popup-devices-set-]')")]
    check(f'{tag}: once the viewer is a guest the same entry opens the popup read-only and asks no list',
          got == [True, name, 0, False], got)
    close_popup(b, name)
    b.page("(__core.model.replace = window.__replace, __core.model.replace({ ...__core.model.page(),"
           " viewer: window.__viewer }), 1)")
    time.sleep(0.4)


POPUPS = {
    # the page's own popups: the core adds them
    'devices': {
        'title': 'Devices',
        'folder': 'core/popups/devices/',
        'add': None,
        'entries': ['overview', 'd-Gungnir%202', 'd-Second'],
        'ask': None,
        'form': None,
        'list': None,
        'extra': devices_extra,
    },
    'sound': {
        'title': 'Sound',
        'folder': 'core/popups/sound/',
        'add': None,
        'entries': [],
        'ask': None,
        'form': None,
        'list': None,
        'extra': None,
    },
    'help': {
        'title': 'Help',
        'folder': 'core/popups/help/',
        'add': None,
        'entries': ['here', 'everywhere', 'about'],
        'ask': None,
        'form': None,
        'list': None,
        'extra': help_extra,
    },
    # the sample no person reaches: the check adds it to the core it holds
    'sample': {
        'title': 'Sample popup',
        'folder': 'core/popups/sample/',
        'add': ("(__core.popups.add('sample', { title: 'Sample popup',"
                " entries: [{ id: 'one', label: 'Page one' }, { id: 'two', label: 'Page two' }],"
                " load: () => import(new URL('assets/core/popups/sample/wire.js', document.baseURI)) }), 1)"),
        'entries': ['one', 'two'],
        'ask': 'popup-sample-ask-open',
        'form': 'popup-sample-form-open',
        'list': 'popup-sample-list-open',
        'extra': sample_extra,
    },
}
# the sample first: the entry animation's case and the open by script are made on the first
ORDER = ['sample', 'devices', 'sound', 'help']
ONLY = [name for name in ORDER
        if name in os.environ.get('WEB_CHECK_POPUPS', ','.join(ORDER)).split(',')]


def requests(b, folder):
    """How many requests the page has made under `folder`."""
    return b.page("performance.getEntriesByType('resource').filter((e) => e.name.includes("
                  f"{json.dumps('/assets/' + folder)})).length")


def el(name):
    return f"document.getElementById('popup-{name}')"


def ready(b, name):
    """Whether the popup stands and is drawn."""
    return poll(lambda: b.page(f"{el(name)}?.querySelector('.fp-box')?.dataset.state === 'ready'"), 8.0)


# the router's words in the console, kept by the page
WARNS = ("window.__warns = []; const warn = console.warn; console.warn = (...a) => {"
         " window.__warns.push(a.map(String).join(' ')); return warn.apply(console, a); };")
UNTAPPED = "window.__warns.filter((w) => w.includes('no tap or key'))"
# a button over the page's corner whose click opens a popup from the frame's button: the check's
# opens come from a real click, as a person's do
OPENER = """((name, page) => { const o = document.createElement('button'); o.id = 'check-open';
  o.style.cssText = 'position:fixed;left:0;top:0;width:24px;height:24px;z-index:2147483647;opacity:0';
  o.onclick = () => { o.remove(); document.getElementById('fx-button').focus();
    __core.router.open(name, page); };
  document.body.append(o); return 1; })"""


def open_popup(b, name, page=None):
    """Open the popup by a real click, from the frame's button, which has the focus."""
    b.page(f"{OPENER}({json.dumps(name)}, {json.dumps(page)})")
    b.click(12, 12)
    return ready(b, name)


def close_popup(b, name):
    b.page('(__core.router.close(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.4)
    return gone


RECTS = """(() => { const box = (q) => { const e = %s.querySelector(q); if (!e) return null;
    const r = e.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height, shown: r.width > 0 && r.height > 0 }; };
  return { box: box('.fp-box'), side: box('.fp-side'), page: box('.fp-page'), back: box('.fp-back'),
    win: [window.innerWidth, window.innerHeight],
    over: [document.documentElement.scrollWidth - document.documentElement.clientWidth,
      %s.querySelector('.fp-box').scrollWidth - %s.querySelector('.fp-box').clientWidth] }; })()"""
# what stands at a point: 'box' inside the popup's box, 'veil' on the popup outside it, else the
# id or the tag of what is there
AT_POINT = """((x, y) => { const e = document.elementFromPoint(x, y); if (!e) return 'nothing';
  if (e.closest('.fp-box')) return 'box'; if (e.closest('.fp')) return 'veil';
  return e.id || e.tagName; })"""
# a token as the theme sets it, read back through a probe that uses it
TOKENS = """(() => { const probe = document.createElement('i');
  probe.style.cssText = 'position:fixed;left:-99px;background:var(--frame-surface);color:var(--frame-ink)';
  document.body.append(probe); const want = getComputedStyle(probe);
  const out = { surface: want.backgroundColor, ink: want.color,
    box: getComputedStyle(%s.querySelector('.fp-box')).backgroundColor,
    title: getComputedStyle(%s.querySelector('.fp-title')).color };
  probe.remove(); return out; })()"""


# an open list in the popup: the boxes of its panel, its options, its chip, the popup's box and
# its page pane, whether the panel is what stands at its own centre, how far the pane is scrolled
# and can scroll, and what the chip says
LIST = """(() => { const pop = %s; const chip = document.getElementById(%s);
  const r = (e) => { if (!e) return null; const b = e.getBoundingClientRect();
    return { x: b.left, y: b.top, w: b.width, h: b.height }; };
  const panel = pop.querySelector('.fi-list'); const pane = pop.querySelector('.fp-page');
  const p = r(panel); const top = p ? document.elementFromPoint(p.x + p.w / 2, p.y + p.h / 2) : null;
  return { panel: p, options: panel ? [...panel.children].map(r) : [], chip: r(chip),
    box: r(pop.querySelector('.fp-box')), pane: r(pane), top: !!top && !!top.closest('.fi-list'),
    scroll: [pane.scrollTop, pane.scrollHeight - pane.clientHeight], says: chip.textContent,
    words: panel ? [...panel.children].map((o) => o.textContent) : [] }; })()"""


def inside(inner, outer, slack=1.0):
    """Whether the box `inner` lies in the box `outer`."""
    return (inner['x'] >= outer['x'] - slack and inner['y'] >= outer['y'] - slack
            and inner['x'] + inner['w'] <= outer['x'] + outer['w'] + slack
            and inner['y'] + inner['h'] <= outer['y'] + outer['h'] + slack)


def listed(b, name, hook, tag):
    """The popup's list, opened from its chip by a click: where its panel stands."""
    read = lambda: b.page(LIST % (el(name), json.dumps(hook['list'])))
    press = lambda box: b.click(box['x'] + box['w'] / 2, box['y'] + box['h'] / 2)
    tag = f'a_list_opens_inside_the_box {tag}'
    press(read()['chip'])
    got = read()
    if got['panel'] is None:
        check(f'{tag}: a click on the chip opens the list', False, got)
        return
    panel, chip = got['panel'], got['chip']
    pane = got['pane']
    # under its chip, or over it, or (room for neither) against the bottom of what the pane shows
    beside = (abs(panel['y'] - (chip['y'] + chip['h'] + 4)) <= 1 or abs(panel['y'] + panel['h'] - (chip['y'] - 4)) <= 1
              or abs(panel['y'] + panel['h'] - (pane['y'] + pane['h'])) <= 1.5) and abs(panel['x'] - chip['x']) <= 1
    check(f'{tag}: the panel is inside the box and what the pane shows, next to its chip and on top',
          inside(panel, got['box']) and inside(panel, pane) and beside and got['top'] is True
          and panel['w'] > 0 and panel['h'] > 0, got)
    pick = next(i for i, word in enumerate(got['words']) if word not in got['says'])
    press(got['options'][pick])
    time.sleep(0.3)
    after = read()
    check(f'{tag}: a pick closes the list, keeps the popup and is shown, the focus on the chip',
          after['panel'] is None and got['words'][pick] in after['says']
          and b.page('document.activeElement.id') == hook['list'], [after, b.page(AT)])
    # a pane that scrolls: the box made low through the style object, the chip brought into view
    b.page(f"({el(name)}.querySelector('.fp-box').style.height = '180px',"
           f" document.getElementById({json.dumps(hook['list'])}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.3)
    press(read()['chip'])
    low = read()
    if low['panel'] is None or low['scroll'][1] < 12:
        check(f'{tag}: in a low box the pane scrolls and the list opens', False, low)
    else:
        check(f'{tag}: in a pane that scrolls the panel is inside the box and what the pane shows, on top',
              inside(low['panel'], low['box']) and inside(low['panel'], low['pane']) and low['top'] is True, low)
        by = 10 if low['scroll'][0] + 10 <= low['scroll'][1] else -10
        b.page(f"({el(name)}.querySelector('.fp-page').scrollTop += {by}, 1)")
        time.sleep(0.3)
        moved = read()
        gap = lambda one: one['panel']['y'] - one['chip']['y']
        check(f'{tag}: the panel goes with its chip when the pane is scrolled',
              moved['panel'] is not None and moved['scroll'][0] != low['scroll'][0]
              and abs(moved['chip']['y'] - low['chip']['y'] + by) <= 1 and abs(gap(moved) - gap(low)) <= 0.5,
              [low, moved])
        b.press('Escape')
        time.sleep(0.3)
        shut = read()
        check(f'{tag}: Escape closes the list alone, the focus on the chip',
              shut['panel'] is None and b.page(f'!!{el(name)}') is True
              and b.page('document.activeElement.id') == hook['list'], [shut, b.page(AT)])
    b.page(f"({el(name)}.querySelector('.fp-box').style.height = '', 1)")
    time.sleep(0.2)


def rects(b, name):
    return b.page(RECTS % (el(name), el(name), el(name)))


def dragged(b, name, hook, tag):
    """A press in the name form's field let go on the veil, then a click on the veil."""
    tag = f'a_drag_that_ends_on_the_veil_keeps_the_popup {tag}'
    b.page(f"(document.getElementById({json.dumps(hook['form'])}).click(), 1)")
    field = f"{el(name)}?.querySelector('.fi-form input')"
    if poll(lambda: b.page(f'!!{field}'), 3.0) is not True:
        check(f'{tag}: the form opens', False)
        return
    box = b.page(f"(() => {{ const f = {field}; f.value = 'A kept name'; const r = f.getBoundingClientRect();"
                 " return { x: r.left, y: r.top, w: r.width, h: r.height }; })()")
    b.drag(box['x'] + box['w'] - 12, box['y'] + box['h'] / 2, 5, box['y'] + box['h'] / 2)
    time.sleep(0.4)
    after = b.page(f"[!!{el(name)}, {field}?.value ?? null, {STATE}[1]]")
    check(f'{tag}: the popup stands and the field holds its text', after == [True, 'A kept name', name], after)
    if after[0] is not True:
        open_popup(b, name)
        return
    b.press('Escape')
    time.sleep(0.3)
    check(f'{tag}: Escape then closes the form alone', b.page(f"[!!{field}, !!{el(name)}]") == [False, True])
    # the other way round: pressed on the veil, let go in the box
    b.drag(5, box['y'] + box['h'] / 2, box['x'] + box['w'] / 2, box['y'] + box['h'] / 2)
    time.sleep(0.4)
    check(f'{tag}: a press on the veil let go in the box keeps it too', b.page(f'!!{el(name)}') is True)
    b.click(5, 5)
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.5)
    check(f'{tag}: a click on the veil closes it', gone is True, b.page(STATE))
    open_popup(b, name)


# a long entry animation, as a theme would set one, and a count of its starts
ANIMATE = """(() => { const sheet = new CSSStyleSheet();
  sheet.replaceSync('@keyframes check-popup-in { from { opacity: .3 } to { opacity: 1 } }'
    + ' :root { --frame-popup-in: check-popup-in 30s linear; }');
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  window.__starts = 0;
  document.addEventListener('animationstart', (event) => {
    if (event.animationName === 'check-popup-in') window.__starts += 1; }, true);
  return 1; })()"""
# how often it started, how many of it run on the box now, and whether the first has moved on
MOVING = """(() => { const box = %s?.querySelector('.fp-box');
  const runs = (box?.getAnimations() ?? []).filter((a) => a.animationName === 'check-popup-in');
  return [window.__starts, runs.length, runs.every((a) => a.playState === 'running' && a.currentTime > 0)]; })()"""


def animated(b, d, theme, name, hook):
    """The entry animation over a first open (lazy), a step and a second open."""
    tag = f'an_entry_animation_runs_once_an_opening {name} ({theme})'
    load(b, d, theme)
    if hook['add']:
        b.page(hook['add'])
    b.page(BODY)
    b.page(ANIMATE)
    cold = requests(b, hook['folder'])
    stands = open_popup(b, name)
    time.sleep(0.4)
    first = b.page(MOVING % el(name))
    check(f'{tag}: the first open, its module arriving meanwhile, starts it once and it runs on',
          cold == 0 and stands is True and first == [1, 1, True], [cold, stands, first])
    if hook['entries']:
        last = hook['entries'][-1]
        entry = b.page(f"(() => {{ const r = {el(name)}.querySelector('[data-popup-page={last}]')"
                       ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()")
        b.click(*entry)
        poll(lambda: b.page(f"{STATE}[2] === {json.dumps(last)}"), 3.0)
        time.sleep(0.3)
        stepped = b.page(MOVING % el(name))
        check(f'{tag}: a step to a page starts nothing and cuts nothing', stepped == [1, 1, True], stepped)
    close_popup(b, name)
    again = open_popup(b, name)
    time.sleep(0.4)
    second = b.page(MOVING % el(name))
    check(f'{tag}: a second open starts it once more', again is True and second == [2, 1, True], second)
    close_popup(b, name)


def wheeled(b, name, theme, tag):
    """A wheel over the veil, on a page that can scroll."""
    sheet = b.page(f"(() => {{ const s = getComputedStyle({el(name)});"
                   " return [s.overflowX, s.overflowY, s.overscrollBehaviorY]; })()")
    check(f'the_page_does_not_scroll_under_the_veil {tag}: the veil is a scroll container that contains',
          sheet == ['hidden', 'hidden', 'contain'], sheet)
    if browser != 'chromium':
        return
    room = b.page('document.documentElement.scrollHeight - window.innerHeight')
    if room < 200:
        print(f'note the_page_does_not_scroll_under_the_veil {tag}: the page of {theme} does not scroll'
              f' at this size ({room} px), no case')
        return
    close_popup(b, name)
    b.page('(window.scrollTo(0, 60), 1)')
    time.sleep(0.3)
    open_popup(b, name)
    before = b.page('window.scrollY')
    b.wheel(6, 300, 120)
    time.sleep(0.5)
    after = b.page('window.scrollY')
    scrolls.append(theme)
    check(f'the_page_does_not_scroll_under_the_veil {tag}: a wheel over the veil leaves scrollY',
          before == 60 and after == before, [before, after])
    close_popup(b, name)
    b.page('(window.scrollTo(0, 0), 1)')
    time.sleep(0.3)
    open_popup(b, name)


def tab_walk(b, name, presses):
    """Press Tab `presses` times: whether the focus was on a control inside the popup after each,
    and what had it."""
    walk = []
    for _ in range(presses):
        b.press('Tab')
        walk.append(b.page(f"[{el(name)}.contains(document.activeElement) && document.activeElement !== {el(name)}, {AT}]"))
    return all(step[0] for step in walk), [step[1] for step in walk]


def desk(b, d, theme, name, hook):
    tag = f'{name} ({theme})'
    shown = b.page('__core.model.shown()')
    check(f'nothing_of_the_popup_is_loaded_before_it_is_opened {tag}: no request under {hook["folder"]}',
          requests(b, hook['folder']) == 0, requests(b, hook['folder']))
    stands = open_popup(b, name)
    check(f'{tag}: the popup opens and is drawn', stands is True,
          b.page(f"{el(name)}?.querySelector('.fp-box')?.dataset.state ?? null"))
    if stands is not True:
        return
    check(f'nothing_of_the_popup_is_loaded_before_it_is_opened {tag}: its files came with the first open',
          requests(b, hook['folder']) >= 1, requests(b, hook['folder']))
    title = b.page(f"[{el(name)}.getAttribute('role'), document.getElementById({el(name)}.getAttribute('aria-labelledby'))?.textContent]")
    check(f'{tag}: a dialog labelled by its title', title == ['dialog', hook['title']], title)
    r = rects(b, name)
    box = r['box']
    top = b.page(f"[{AT_POINT}({box['x'] + box['w'] / 2}, {box['y'] + box['h'] / 2}), {AT_POINT}(4, 4),"
                 f" {AT_POINT}({r['win'][0] - 20}, 20)]")
    check(f'the_popup_is_on_top_of_the_theme {tag}: the box at its centre, the veil at the corners',
          top == ['box', 'veil', 'veil'], top)
    check(f'the_page_behind_is_inert {tag}', b.page(INERT) == [True, True, False], b.page(INERT))
    inside, names = tab_walk(b, name, 12)
    check(f'tab_stays_inside {tag}: twelve Tabs never leave the popup and come round',
          inside and len(set(names)) < len(names) and len(set(names)) > 1, names)
    if hook['entries']:
        check(f'the_list_and_the_page_stand_side_by_side {tag}',
              r['side']['shown'] and r['page']['shown'] and r['side']['x'] + r['side']['w'] <= r['page']['x'] + 1
              and box['w'] < r['win'][0] and box['h'] < r['win'][1], r)
    else:
        check(f'the_list_and_the_page_stand_side_by_side {tag}: no list, the box smaller than the window',
              r['page']['shown'] and box['w'] < r['win'][0] and box['h'] < r['win'][1], r)
    check(f'nothing_scrolls_sideways {tag}', r['over'] == [0, 0], r['over'])
    moves = b.page(f"getComputedStyle({el(name)}.querySelector('.fp-box')).animationName")
    check(f'the_box_opens_with_no_animation {tag}: no theme sets --frame-popup-in', moves == 'none', moves)
    schemes = b.page('__core.theme.schemes ?? ["dark"]')
    mode = b.page('__core.scheme.mode()')
    for scheme in schemes:
        if len(schemes) > 1:
            b.page(f"(__core.scheme.set({json.dumps(scheme)}), 1)")
            time.sleep(0.3)
        got = b.page(TOKENS % (el(name), el(name)))
        check(f'the_colours_are_the_themes_tokens {tag}, {scheme}: the box is --frame-surface, the title --frame-ink',
              got['box'] == got['surface'] and got['title'] == got['ink'] and got['surface'] != got['ink'], got)
        seen.setdefault(scheme, {})[theme] = got['surface']
    if len(schemes) > 1:
        b.page(f"(__core.scheme.set({json.dumps(mode)}), 1)")
    if hook['extra']:
        hook['extra'](b, d, tag, theme)
    if hook.get('list'):
        listed(b, name, hook, tag)
    if hook['ask']:
        b.page(f"(document.getElementById({json.dumps(hook['ask'])}).focus(),"
               f" document.getElementById({json.dumps(hook['ask'])}).click(), 1)")
        asked = poll(lambda: b.page(f"!!{el(name)}.querySelector('.fi-askbox')"), 3.0)
        b.press('Escape')
        time.sleep(0.3)
        after = b.page(f"[!!{el(name)}?.querySelector('.fi-askbox'), !!{el(name)}, document.activeElement.id]")
        check(f'escape_closes_the_ask_then_the_popup {tag}: Escape in the ask closes the ask alone',
              asked is True and after == [False, True, hook['ask']], [asked, after])
    if hook.get('form'):
        dragged(b, name, hook, tag)
    wheeled(b, name, theme, tag)
    b.press('Escape')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.5)
    after = b.page(f'[document.activeElement.id, {STATE}, {ENTRY}[1], __core.model.shown(), {INERT}]')
    check(f'escape_closes_the_ask_then_the_popup {tag}: Escape closes the popup, the focus is back where it'
          ' was, the device is the same',
          gone is True and after == ['fx-button', [shown, None, None], None, shown, [False, False, False]], [gone, after])
    b.page("(() => { window.__shows = 0; if (window.__wrapped) return 1; window.__wrapped = true;"
           " const show = __core.actions.showDevice;"
           " __core.actions.showDevice = (...a) => { window.__shows += 1; return show(...a); }; return 1; })()")
    again = open_popup(b, name)
    b.page('(history.back(), 1)')
    gone = poll(lambda: b.page(f'!{el(name)}'), 4.0)
    time.sleep(0.5)
    after = b.page(f'[window.__shows, {STATE}, __core.model.shown(), document.activeElement.id]')
    check(f'back_closes_it {tag}: Back closes it, no device is read, the focus is back',
          again is True and gone is True and after == [0, [shown, None, None], shown, 'fx-button'], [again, gone, after])
    if hook['entries']:
        # at a page a wide window does not show the way back, the last control in the markup
        last = hook['entries'][-1]
        paged = open_popup(b, name, last)
        back = rects(b, name)['back']
        # a device's page holds its settings' controls, and the saved presets' page a button on each
        # row: Tab needs more presses to come round
        inside, names = tab_walk(b, name, 16 if name in ('devices', 'sound') else 8)
        check(f'tab_stays_inside {tag}: at page {last}, where the way back is not shown, Tab never leaves'
              ' the popup and comes round',
              paged is True and back is not None and not back['shown'] and inside
              and len(set(names)) < len(names) and len(set(names)) > 1, [paged, back, names])
        check(f'{tag}: it closes from page {last}', close_popup(b, name) is True)


def phone(b, theme, name, hook):
    tag = f'on_a_phone_the_popup_fills_the_screen {name} ({theme})'
    stands = open_popup(b, name)
    if stands is not True:
        check(f'{tag}: the popup opens', False, b.page(f'!!{el(name)}'))
        return
    r = rects(b, name)
    box, win = r['box'], r['win']
    check(f'{tag}: the box is the window',
          abs(box['x']) <= 1 and abs(box['y']) <= 1 and abs(box['w'] - win[0]) <= 1 and abs(box['h'] - win[1]) <= 1, r)
    check(f'{tag}: nothing scrolls sideways', r['over'] == [0, 0], r['over'])
    check(f'{tag}: the page behind is inert', b.page(INERT) == [True, True, False], b.page(INERT))
    top = b.page(f"[{AT_POINT}({win[0] / 2}, {win[1] / 2}), {AT_POINT}(4, {win[1] - 4}), {AT_POINT}({win[0] - 4}, 60)]")
    check(f'{tag}: the box is on top everywhere', top == ['box', 'box', 'box'], top)
    if hook['entries']:
        check(f'{tag}: at the list the list is shown and the page pane is not',
              r['side']['shown'] and not r['page']['shown'] and abs(r['side']['w'] - win[0]) <= 1, r)
        first, last = hook['entries'][0], hook['entries'][-1]
        check(f'{tag}: the focus opens on the current entry of the list',
              b.page('document.activeElement.id') == f'popup-{name}-entry-{first}', b.page(AT))
        # the page pane is not shown at the list and holds the last controls in the markup
        inside, names = tab_walk(b, name, 8)
        check(f'{tag}: at the list eight Tabs never leave the popup and come round',
              inside and len(set(names)) < len(names) and len(set(names)) > 1, names)
        entry = b.page(f"(() => {{ const r = {el(name)}.querySelector('[data-popup-page={last}]')"
                       ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()")
        b.click(*entry)
        poll(lambda: b.page(f"{STATE}[2] === {json.dumps(last)}"), 3.0)
        time.sleep(0.3)
        r = rects(b, name)
        check(f'{tag}: at a page the page is shown with the way back, and the list is not',
              r['page']['shown'] and not r['side']['shown'] and r['back'] is not None and r['back']['shown']
              and abs(r['page']['w'] - win[0]) <= 1 and r['over'] == [0, 0], r)
        check(f'{tag}: after the step to a page the focus is on the way back, inside the popup',
              b.page('document.activeElement.id') == f'popup-{name}-back', b.page(AT))
        b.page(f"({el(name)}.querySelector('[data-popup-back]').click(), 1)")
        listed = poll(lambda: b.page(f"{STATE}[2] === null && !!{el(name)}"), 3.0)
        time.sleep(0.3)
        r = rects(b, name)
        check(f'{tag}: the way back is the list again',
              listed is True and r['side']['shown'] and not r['page']['shown'], r)
        check(f'{tag}: and the focus is on the list, on its current entry',
              b.page('document.activeElement.id') == f'popup-{name}-entry-{first}', b.page(AT))
    else:
        check(f'{tag}: the page pane is the window\'s width', r['page']['shown'] and abs(r['page']['w'] - win[0]) <= 1, r)
    check(f'{tag}: it closes', close_popup(b, name) is True)


# the box's ground per scheme and theme: the themes do not all share one
seen = {}
# the themes whose page scrolled, so the wheel's case ran there
scrolls = []


def main():
    d = common.Daemon(host='fake', more=[('Second', '1112131415161718')])
    try:
        for theme in THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(HOOK + ' ' + WARNS)
                load(b, d, theme)
                for name in ONLY:
                    hook = POPUPS[name]
                    if hook['add']:
                        b.page(hook['add'])
                    b.page(BODY)
                    desk(b, d, theme, name, hook)
                if 'devices' in ONLY:
                    lost_right(b, theme)
                b.resize(*PHONE)
                time.sleep(0.6)
                for name in ONLY:
                    phone(b, theme, name, POPUPS[name])
                check(f'a_popup_opens_from_a_tap_or_a_key ({theme}): the router said nothing in the console',
                      b.page(UNTAPPED) == [], b.page(UNTAPPED))
                if ONLY and b.page("typeof navigator.userActivation?.isActive === 'boolean'"):
                    # the last click or key wears off; an open by script then has no tap behind it
                    idle = poll(lambda: b.page('navigator.userActivation.isActive === false'), 10.0)
                    b.page(f"(__core.router.open({json.dumps(ONLY[0])}), 1)")
                    stands = ready(b, ONLY[0])
                    said = b.page(UNTAPPED)
                    check(f'a_popup_opens_from_a_tap_or_a_key ({theme}): an open by script with no tap is said',
                          idle is True and stands is True and len(said) == 1 and 'opened' in said[0], [idle, said])
                    close_popup(b, ONLY[0])
                if ONLY:
                    b.resize(*DESK)
                    animated(b, d, theme, ONLY[0], POPUPS[ONLY[0]])
                if 'help' in ONLY:
                    here_of_a_widget(b, d, theme)
            finally:
                b.close()
        if 'help' in ONLY:
            g = common.Daemon(host='fake', web='guest = "read"')
            try:
                for theme in THEMES:
                    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
                    try:
                        b.preload(HOOK + ' ' + WARNS)
                        load(b, g, theme)
                        b.page(BODY)
                        guest(b, theme)
                        b.resize(*PHONE)
                        time.sleep(0.6)
                        phone(b, theme, 'help', POPUPS['help'])
                        check(f'a_popup_opens_from_a_tap_or_a_key ({theme}), a guest: the router said nothing'
                              ' in the console', b.page(UNTAPPED) == [], b.page(UNTAPPED))
                        if 'devices' in ONLY:
                            guest_devices(b, theme)
                    finally:
                        b.close()
            finally:
                g.stop()
        grounds = {surface for themes in seen.values() for surface in themes.values()}
        if len(THEMES) > 1 and ONLY:
            check('the_colours_are_the_themes_tokens: the themes do not all give the box one ground',
                  len(grounds) > 1, seen)
            if browser == 'chromium':
                check('the_page_does_not_scroll_under_the_veil: one theme\'s page at least scrolls at this size',
                      len(scrolls) > 0, scrolls)
    finally:
        d.stop()
    print(f'popups.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
