# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The settings of a device's page in the Devices popup, in a browser, in every theme: what no node
harness can see (a real click, the focus, the layout). The wire's logic is node's
(node/core-device-wire.mjs); popups.py holds the popup's frame and its Overview. The daemon is a
fake with three configured devices, the first (`Gungnir 2`) shown and connected, the others
(`Second`, `Third`) never connected; its configuration is its own for the run, and each case that renames
or makes a default leaves it as it found it.

Per theme, at 1280 px unless said:
  the_settings_stand_above_the_graphs    on a device's page: the name field, the profile's
                                        button, the passphrase's kind, *Make default* or the word
                                        Default, in that order, above the key graphs; none on the
                                        Overview
  a_rename_follows_the_device            a name typed and saved by a real click on Save (and by
                                        Enter): the POST names the page's device, the page, the
                                        tab's title and the side list move to the new name in
                                        place (no entry pushed), the focus is on the name field;
                                        a name another device has is refused as words at the
                                        field (`aria-invalid`), what was typed stays, the focus
                                        is on the field
  the_shown_devices_rename_moves_the_page_behind   the shown device's rename: the popup stands on
                                        the new address and the page behind shows the renamed
                                        device, not the first one
  the_passphrase_has_three_choices       Automatic, None, a code: each is the POST's body; the
                                        code is typed, kept through the reveal toggle, and is
                                        nowhere in the page after it was sent; a question names
                                        the device when it is playing is node's (the fake
                                        device plays nothing)
  the_profile_is_picked_from_a_list      the list opens inside the popup's box, a pick posts the
                                        profile, the focus is on the button again; Escape closes
                                        the list alone
  make_default_moves_the_mark            Second becomes the default and the first has Make
                                        default again; it is made the default again at the end
  the_forms_are_dressed_by_the_frames_tokens   the fieldset has no border, the radios take the
                                        frame's accent, the legend and labels the frame's inks
                                        (the tokens the theme sets)
  nothing_scrolls_sideways               at 1280 and at 390 px with the passphrase form open
  the_facts_stand_under_the_graphs       Product, Unit, Firmware, DSP, Profile, Link, Signal say what the
                                        model says; the facts stand after the key graphs; Identify posts
                                        to the page's device and the toast is the daemon's words; on a
                                        device that is not connected it is disabled with its reason
  all_readings_is_a_disclosure           closed and empty at first; a real click opens it: a row per
                                        reading with the model's words and an age; a second closes it
  the_fold_out_foot_opens_the_page       each theme's *Device settings…* (Ember's fold-out, Paper's word,
                                        Signal's shelf, Night drive's sheet) opens the popup at the
                                        page of the device, wide and at 390 px
  the_overview_says_the_config_changed  a hand edit of the daemon's config.toml: the Overview opens
                                        with the daemon's words (the file it reads, `svolder daemon
                                        reload`), shown, wide and at 390 px with nothing scrolling
                                        sideways; none before the edit; a rename through the page
                                        is no change; a SIGHUP to the daemon takes the line away at
                                        the page's next read, with no reopening
  remove_asks_once_and_ends_on_the_overview   a non-shown device: the question names it, Keep sends
                                        nothing and has the focus, Remove posts to it and the pane
                                        ends on the Overview with the line; then the shown device is
                                        removed (the page behind moves to the one left, the popup
                                        stands) and a rename after it ends on the right device
  a_guest_reads_the_devices_popup        on the guest's daemon: the menu's row opens the popup, read-only:
                                        the fleet's list with no *+ Add a device*, the Overview's key
                                        graphs, a device's page with its facts (no Unit), All readings,
                                        *Show on the page*, and no settings, *Remove…*, Identify or
                                        config line; an address naming the add page shows the Overview;
                                        at 390 px nothing scrolls sideways; the theme's foot reads
                                        *Devices…* and a real click opens the page, wide and at 390 px;
                                        no POST and no GET of `api/v1/devices` in the whole case
  a_page_with_writes_off_reads_the_devices_popup   the same for a signed-in viewer on a daemon with
                                        `writes = false`, with the Unit in the facts
  add_a_device_draws_the_daemons_walk    in every theme, on a daemon with an unconfigured device on the
                                        air: *+ Add a device* is the side list's last entry; its page
                                        warns, naming the linked device, with the safe answer focused;
                                        Go on, Scan: the rows were drawn while it listened (the fake
                                        hears at once, so an observer notes them); Escape at the pick
                                        stops the walk (the daemon's state says cancelled); at 390 px
                                        the page does not scroll sideways and a phone's Back stops a
                                        walk too
  add_a_device_walks_to_the_end          (the first theme) pick (the configured unit is not offered),
                                        the name, done: the device's name and link, the list gains it,
                                        *Open its page* steps in place with no gesture warning
  add_a_device_rejoins_a_walk            a reload mid-walk: the page's own walk is taken over where it
                                        stands and answered; a walk started over the socket shows
                                        read-only and is left running when the page leaves
  add_the_first_device_of_a_fresh_install  no device configured: menu, Devices, *+ Add a device*,
                                        done: the page behind shows the device, no entry pushed
  a_device_added_after_the_last_was_removed_is_taken_up_by_the_page   (every theme) no device
                                        configured: a walk over the socket adds one and the page
                                        takes it up; its last device removed leaves the no-device
                                        card; a walk adds one again and the page takes it up, with
                                        no reload and no *Show on the page*
  (the daemon's fake cannot refuse the advertised code, so a wrong passphrase is node's:
  node/core-device-walk.mjs)
With WEB_CHECK_SHOTS=<folder> `<theme>-add-pick.png` (the add page at the pick, wide) and
`<theme>-add-warning-phone.png` (its warning at 390 px) are written there too, and `<theme>-device.png`, a device's page scrolled to the top (the name with *Show on the page* at its right), is written
there.
Usage: python3 devices_popup.py chromium|firefox"""
import json
import os
import signal
import sys
import time

import common
import modal
from modal import BODY, POSTS, STATE, THEMES, check, load, poll
from popups import (DESK, PHONE, UNTAPPED, WARNS, by_the_menu, centre, close_popup, el, open_popup, ready,
                    rects)

browser = modal.browser
NAME = 'popup-devices-set-name-field'


def sid(setting, part=None):
    return f'popup-devices-set-{setting}' + (f'-{part}' if part else '')


def put_text(b, ident, text):
    """Type `text` into the field: its value and the events a typist's keys make."""
    b.page(f"(() => {{ const f = document.getElementById({json.dumps(ident)}); f.focus();"
           f" f.value = {json.dumps(text)};"
           " f.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()")


def press_on(b, ident):
    """A real click on the control: scrolled into view first."""
    b.page(f"(document.getElementById({json.dumps(ident)}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.15)
    b.click(*centre(b, '#' + ident))
    time.sleep(0.3)


def click_summary(b):
    """A real click on the All readings disclosure's summary (its centre is the table once it is open)."""
    sel = '#popup-devices-readings > summary'
    b.page(f"(document.querySelector({json.dumps(sel)}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.15)
    b.click(*centre(b, sel))
    time.sleep(0.3)


def writes(b, route='/edit'):
    return [json.loads(p[1]) for p in b.page(POSTS) if p[0].endswith(route)]


def where(b):
    return b.page("[document.activeElement?.id ?? null, "
                  f"document.getElementById('popup-devices').contains(document.activeElement)]")


def state(b):
    return b.page(STATE)


def settle(b, page, secs=8.0):
    """The popup is at `page` and its settings are drawn."""
    return poll(lambda: b.page(f"{STATE}[2] === {json.dumps(page)}"
                               f" && !!document.getElementById({json.dumps(NAME)})"), secs)


def kind_words(b):
    return b.page(f"document.getElementById({json.dumps(sid('passphrase'))}).textContent.trim()")


def devices_case(b, d, theme):
    tag = f'({theme})'
    urls = b.page('__core.model.page().fleet.devices.map((d) => d.url)')
    first, second = urls[0], urls[1]
    name = 'devices'
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    check(f'devices opens {tag}', open_popup(b, name, f'd-{second}') is True)
    settle(b, f'd-{second}')
    time.sleep(0.5)

    # ---- the settings stand above the graphs ----
    order = b.page("""(() => { const block = document.getElementById('popup-devices-body');
      return [...block.querySelectorAll('#popup-devices-settings, .fpd-graphs, .fpd-state')]
        .map((e) => e.id.replace(/-[0-9a-f]+$/, '')); })()""")
    check(f'the_settings_stand_above_the_graphs {tag}: the state, the settings, then the graphs',
          order == ['popup-devices-state', 'popup-devices-settings', 'popup-devices-graphs'], order)
    rows = b.page("[...document.querySelectorAll('#popup-devices-settings .fp-row-label')].map((e) => e.textContent)")
    check(f'the_settings_stand_above_the_graphs {tag}: name, profile, passphrase, default, remove',
          rows == ['Name', 'Profile', 'Passphrase', 'Default device', 'Remove device'], rows)
    check(f'the_settings_stand_above_the_graphs {tag}: the name field holds the name',
          b.page(f"document.getElementById({json.dumps(NAME)}).value") == 'Second')
    # ---- the forms are dressed by the frame's tokens ----
    press_on(b, sid('passphrase', 'change'))
    press_on(b, sid('passphrase', 'kind-code'))
    dress = b.page("""(() => { const fs = document.querySelector('#popup-devices-settings fieldset');
      const probe = document.createElement('i');
      probe.style.cssText = 'position:fixed;left:-99px;color:var(--frame-ink);accent-color:var(--frame-accent)';
      document.body.append(probe); const want = getComputedStyle(probe);
      const radio = fs.querySelector('input[type=radio]');
      const out = { border: getComputedStyle(fs).borderTopWidth, radios: fs.querySelectorAll('input[type=radio]').length,
        accent: getComputedStyle(radio).accentColor, wantAccent: want.accentColor,
        label: getComputedStyle(fs.querySelector('label')).color, ink: want.color,
        legend: getComputedStyle(fs.querySelector('legend')).color };
      probe.remove(); return out; })()""")
    check(f'the_forms_are_dressed_by_the_frames_tokens {tag}: no border on the fieldset, three radios on the'
          ' accent, the labels in the ink', dress['border'] == '0px' and dress['radios'] == 3
          and dress['accent'] == dress['wantAccent'] and dress['label'] == dress['ink'], dress)
    r = rects(b, name)
    check(f'nothing_scrolls_sideways {tag}: with the passphrase form open', r['over'] == [0, 0], r['over'])

    press_on(b, sid('passphrase', 'cancel'))

    # ---- a rename, by Save and by Enter, and a refusal ----
    posts = len(b.page(POSTS))
    length = b.page('history.length')
    put_text(b, NAME, 'Gungnir 2')  # another device's name
    press_on(b, sid('name', 'save'))
    refused = poll(lambda: b.page(f"document.getElementById({json.dumps(sid('name', 'words'))}).textContent") != '', 6.0)
    field = b.page(f"(() => {{ const f = document.getElementById({json.dumps(NAME)});"
                   " return [f.getAttribute('aria-invalid'), f.value, document.activeElement === f]; })()")
    words = b.page(f"document.getElementById({json.dumps(sid('name', 'words'))}).textContent")
    check(f'a_rename_follows_the_device {tag}: a name another device has is refused, as words at the field, with'
          ' what was typed kept and the focus on it',
          refused and field == ['true', 'Gungnir 2', True] and len(words) > 3
          and writes(b)[-1:] == [{'name': 'Gungnir 2'}] and len(b.page(POSTS)) == posts + 1, [refused, field, words])
    put_text(b, NAME, 'Second 2')
    b.press('Enter')
    moved = poll(lambda: b.page(f"{STATE}[2] === 'd-Second%202'"), 8.0)
    time.sleep(0.8)
    got = b.page(f"""[document.title, [...document.querySelectorAll('#popup-devices .fp-entry')].map((e) => e.textContent),
      document.activeElement.id, history.length, document.getElementById({json.dumps(NAME)}).value,
      document.getElementById({json.dumps(NAME)}).getAttribute('aria-invalid'), __core.model.shown()]""")
    check(f'a_rename_follows_the_device {tag}: Enter sends the form for the page\'s device; the page, the title and'
          ' the list follow the new name in place, no entry is pushed',
          moved is True and writes(b)[-1] == {'name': 'Second 2'}
          and any(p[0].endswith('/Second/edit') for p in b.page(POSTS)) and ' · Devices · Second 2 — ' in got[0]
          and any(e.startswith('Second 2') for e in got[1]) and got[3] == length and got[4] == 'Second 2'
          and got[5] == 'false' and got[2] == NAME and got[6] == first, got)
    # back to the name it had
    put_text(b, NAME, 'Second')
    press_on(b, sid('name', 'save'))
    back = poll(lambda: b.page(f"{STATE}[2] === 'd-Second'"), 8.0)
    time.sleep(0.5)
    check(f'a_rename_follows_the_device {tag}: and back to its own name', back is True
          and b.page(f"document.getElementById({json.dumps(NAME)}).value") == 'Second')

    # ---- the passphrase: three choices ----
    for choice, label in (('auto', 'Automatic'), ('none', 'None')):
        press_on(b, sid('passphrase', 'change'))
        press_on(b, sid('passphrase', f'kind-{choice}'))
        press_on(b, sid('passphrase', 'save'))
        poll(lambda: writes(b)[-1:] == [{'passphrase': choice}], 6.0)
        poll(lambda: kind_words(b).startswith(label) or kind_words(b).startswith('Automatic'), 4.0)
        check(f'the_passphrase_has_three_choices {tag}: {choice} is sent for Second and the form closes on it',
              writes(b)[-1:] == [{'passphrase': choice}] and b.page(f"!document.getElementById({json.dumps(sid('passphrase', 'field'))})")
              and where(b)[0] == sid('passphrase', 'change'), [writes(b)[-1:], where(b)])
    press_on(b, sid('passphrase', 'change'))
    press_on(b, sid('passphrase', 'kind-code'))
    code = '4321'
    put_text(b, sid('passphrase', 'field'), code)
    press_on(b, sid('passphrase', 'reveal'))
    shown = b.page(f"[document.getElementById({json.dumps(sid('passphrase', 'field'))}).type,"
                   f" document.getElementById({json.dumps(sid('passphrase', 'field'))}).value,"
                   f" document.getElementById({json.dumps(sid('passphrase', 'field'))}).hasAttribute('value')]")
    check(f'the_passphrase_has_three_choices {tag}: the code is kept through the reveal toggle and is no attribute',
          shown == ['text', code, False], shown)
    press_on(b, sid('passphrase', 'save'))
    poll(lambda: writes(b)[-1:] == [{'passphrase': code}], 6.0)
    time.sleep(0.6)
    leak = b.page(f"(document.getElementById('popup-devices').outerHTML + document.title + location.href"
                  f" + JSON.stringify(history.state)).includes({json.dumps(code)})")
    check(f'the_passphrase_has_three_choices {tag}: the code is sent for Second and is nowhere in the popup, the title, the address or the history after',
          writes(b)[-1:] == [{'passphrase': code}] and leak is False, [writes(b)[-1:], leak])

    # ---- the profile ----
    press_on(b, sid('profile', 'open'))
    panel = b.page("""(() => { const l = document.querySelector('#popup-devices .fi-list');
      const box = document.querySelector('#popup-devices .fp-box').getBoundingClientRect();
      if (!l) return null; const r = l.getBoundingClientRect();
      return { inside: r.left >= box.left - 1 && r.right <= box.right + 1 && r.top >= box.top - 1 && r.bottom <= box.bottom + 1,
        options: [...l.children].map((o) => [o.dataset.value, o.textContent]), focus: document.activeElement.className }; })()""")
    check(f'the_profile_is_picked_from_a_list {tag}: the list opens inside the box with the automatic choice first',
          panel is not None and panel['inside'] and panel['options'][0] == ['', 'Automatic'] and len(panel['options']) > 1,
          panel)
    b.press('Escape')
    time.sleep(0.3)
    check(f'the_profile_is_picked_from_a_list {tag}: Escape closes the list alone',
          b.page("!document.querySelector('#popup-devices .fi-list')") is True
          and state(b)[1] == 'devices' and where(b)[0] == sid('profile', 'open'), [state(b), where(b)])
    if panel and len(panel['options']) > 1:
        pin = panel['options'][1][0]
        press_on(b, sid('profile', 'open'))
        b.click(*centre(b, f'#popup-devices .fi-opt[data-value="{pin}"]'))
        poll(lambda: writes(b)[-1:] == [{'profile': pin}], 6.0)
        time.sleep(0.6)
        check(f'the_profile_is_picked_from_a_list {tag}: a pick posts the profile for Second and the focus is on the button',
              writes(b)[-1:] == [{'profile': pin}] and where(b)[0] == sid('profile', 'open'), [writes(b)[-1:], where(b)])
        press_on(b, sid('profile', 'open'))
        b.click(*centre(b, '#popup-devices .fi-opt[data-value=""]'))
        poll(lambda: writes(b)[-1:] == [{'profile': None}], 6.0)
        check(f'the_profile_is_picked_from_a_list {tag}: and the pin is taken off again', writes(b)[-1:] == [{'profile': None}])

    # ---- make default ----
    press_on(b, sid('default', 'make'))
    done = poll(lambda: b.page(f"!document.getElementById({json.dumps(sid('default', 'make'))})"), 6.0)
    check(f'make_default_moves_the_mark {tag}: Second is the default, the request names it',
          done is True and any(p[0].endswith('/Second/default') for p in b.page(POSTS))
          and 'Default' in b.page(f"document.getElementById({json.dumps(sid('default'))}).textContent"))
    b.page(f"(__core.router.step('d-{first}'), 1)")
    settle(b, f'd-{first}')
    time.sleep(0.4)
    press_on(b, sid('default', 'make'))
    poll(lambda: b.page(f"!document.getElementById({json.dumps(sid('default', 'make'))})"), 6.0)
    check(f'make_default_moves_the_mark {tag}: and the first device is the default again',
          any(p[0].endswith(f'/{first}/default') for p in b.page(POSTS)))

    # ---- the shown device's rename ----
    # (the page of the shown device)
    put_text(b, NAME, 'Main')
    press_on(b, sid('name', 'save'))
    moved = poll(lambda: b.page(f"__core.model.shown() === 'Main' && {STATE}[2] === 'd-Main'"), 10.0)
    time.sleep(0.8)
    got = b.page(f"[__core.model.shown(), {STATE}, document.getElementById('popup-devices') !== null]")
    check(f'the_shown_devices_rename_moves_the_page_behind {tag}: the popup stands on the new address and the page'
          ' behind shows the renamed device, not the first', moved is True and got[1][1] == 'devices'
          and got[0] == 'Main' and got[2] is True, got)
    put_text(b, NAME, first.replace('%20', ' '))
    press_on(b, sid('name', 'save'))
    poll(lambda: b.page(f"__core.model.shown() === {json.dumps(first)}"), 10.0)
    time.sleep(0.6)
    check(f'the_shown_devices_rename_moves_the_page_behind {tag}: and back to its own name',
          b.page(f"__core.model.shown() === {json.dumps(first)}") is True, b.page('__core.model.shown()'))

    # ---- a shown device that is not the fleet's first, renamed ----
    b.page(f"(__core.actions.showDevice({json.dumps(second)}, {{ push: false }}), 1)")
    poll(lambda: b.page(f"__core.model.shown() === {json.dumps(second)}"), 6.0)
    b.page(f"(__core.router.step('d-{second}'), 1)")
    settle(b, f'd-{second}')
    time.sleep(0.4)
    for to, expect in (('Other (left)', 'Other%20%28left%29'), (second, second)):
        put_text(b, NAME, to)
        press_on(b, sid('name', 'save'))
        poll(lambda: b.page(f"__core.model.shown() === {json.dumps(expect)}"), 10.0)
        time.sleep(0.6)
        got = b.page(f"[__core.model.shown(), {STATE}]")
        check(f'the_shown_devices_rename_moves_the_page_behind {tag}: the shown device that is not the first is'
              f' renamed to {to}: the page behind shows it, not the first, and the popup stands on it',
              got[0] == expect and got[1][1:] == ['devices', f'd-{expect}'] and got[0] != first, got)
    b.page(f"(__core.actions.showDevice({json.dumps(first)}, {{ push: false }}), 1)")
    poll(lambda: b.page(f"__core.model.shown() === {json.dumps(first)}"), 6.0)

    # ---- a phone ----
    b.resize(*PHONE)
    time.sleep(0.6)
    b.page(f"(__core.router.step('d-{second}'), 1)")
    settle(b, f'd-{second}')
    press_on(b, sid('passphrase', 'change'))
    press_on(b, sid('passphrase', 'kind-code'))
    r = rects(b, name)
    check(f'nothing_scrolls_sideways {tag}: at 390 px with the passphrase form open and the code chosen',
          r['over'] == [0, 0] and r['box']['w'] <= PHONE[0] + 1, r)
    sized = b.page("""(() => { const probe = document.createElement('i');
      probe.style.cssText = 'position:fixed;left:-99px;height:var(--frame-tap)';
      document.body.append(probe); const tap = probe.getBoundingClientRect().height; probe.remove();
      const hs = [...document.querySelectorAll('#popup-devices-settings button,'
        + ' #popup-devices-settings input:not([type=radio]), #popup-devices-settings .fi-form label')]
        .filter((e) => e.getBoundingClientRect().width > 0).map((e) => e.getBoundingClientRect().height);
      return { tap, least: Math.min(...hs), count: hs.length }; })()""")
    check(f'the_settings_are_tappable {tag}: every button, field and label shown at 390 px is the frame\'s'
          ' tap size (--frame-tap) high', sized['count'] > 0 and sized['least'] >= sized['tap'] - 1, sized)
    press_on(b, sid('passphrase', 'cancel'))
    b.resize(*DESK)
    time.sleep(0.5)
    close_popup(b, name)


def shut(b):
    """Close whatever popup stands, by the router, and wait for it to be gone."""
    b.page('(__core.router.close(), 1)')
    poll(lambda: b.page("__core.router.state().popup") is None, 4.0)
    time.sleep(0.3)


def identify_posts(b):
    return [p[0] for p in b.page(POSTS) if p[0].endswith('/identify')]


def facts_case(b, d, theme):
    """The facts, Identify and All readings of the first (connected) device's page, and the same
    page of the one that is not connected."""
    tag = f'({theme})'
    urls = b.page('__core.model.page().fleet.devices.map((d) => d.url)')
    first, second = urls[0], urls[1]
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    check(f'the_facts_stand_under_the_graphs {tag}: the popup opens at the first device',
          open_popup(b, 'devices', f'd-{first}') is True)
    drawn = poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
    time.sleep(0.5)
    facts = d.model('Gungnir 2')['facts']
    got = b.page("""Object.fromEntries([...document.querySelectorAll('#popup-devices-facts dd[data-fact]')]
      .map((e) => [e.dataset.fact, e.textContent]))""")
    want = {'Firmware': facts['firmware'], 'Profile': facts['profile'], 'Link': facts['link'],
            'Unit': facts['unit'], 'DSP': '—' if facts['dsp'] is None else str(facts['dsp'])}
    check(f'the_facts_stand_under_the_graphs {tag}: product, unit, firmware, DSP, profile, link and signal say what'
          ' the model says', drawn is True and all(got.get(k) == v for k, v in want.items())
          and got.get('Product', '').startswith(facts['product'] or 'product ')
          and got.get('Signal', '').startswith((facts.get('signal_age') or {}).get('head', facts['signal'])),
          [got, want])
    order = b.page("""[...document.querySelector('#popup-devices-body .fpd-device').children].map((e) => e.id
      ? e.id.replace(/-[0-9a-f]+$/, '') : e.className)""")
    check(f'the_facts_stand_under_the_graphs {tag}: the settings, the graphs, the facts, then All readings',
          order[-3:] == ['popup-devices-graphs', 'popup-devices-facts', 'popup-devices-readings'], order)
    # ---- Identify ----
    b.page(POSTS)
    b.page('window.__posts.length = 0')
    press_on(b, 'popup-devices-identify')
    poll(lambda: identify_posts(b), 4.0)
    sent = identify_posts(b)
    toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"), 3.0)
    check(f'the_facts_stand_under_the_graphs {tag}: Identify posts once to the page\'s device and says the daemon\'s'
          ' words', len(sent) == 1 and f'/devices/{first}/identify' in sent[0]
          and toast and any('Identify sent' in t or 'wait' in t for t in toast), [sent, toast])
    # ---- All readings ----
    before = b.page("""({ open: document.getElementById('popup-devices-readings').open,
      body: document.getElementById('popup-devices-readings-body').innerHTML })""")
    check(f'all_readings_is_a_disclosure {tag}: closed and empty at first',
          before == {'open': False, 'body': ''}, before)
    click_summary(b)
    poll(lambda: b.page("!!document.querySelector('#popup-devices-readings-body tr[data-reading]')"), 5.0)
    rows = b.page("""Object.fromEntries([...document.querySelectorAll('#popup-devices-readings-body tr[data-reading]')]
      .map((tr) => [tr.dataset.reading, tr.querySelector('td').textContent]))""")
    ages = b.page("[...document.querySelectorAll('#popup-devices-readings-body td.fpd-reading-age')].map((e) => e.textContent)")
    mine = {r['id']: r['value'] for r in d.model('Gungnir 2')['facts']['rows']}
    check(f'all_readings_is_a_disclosure {tag}: a click opens it: a row per reading with the model\'s words and an age',
          len(rows) == len(mine) and len(rows) > 3 and all(rows.get(k) == v or k in rows for k, v in mine.items())
          and sum(rows.get(k) == v for k, v in mine.items()) >= len(mine) - 3
          and len(ages) == len(rows) and all(a.strip() for a in ages), [len(rows), len(mine), ages[:4]])
    r = rects(b, 'devices')
    check(f'all_readings_is_a_disclosure {tag}: nothing scrolls sideways with the table open', r['over'] == [0, 0], r['over'])
    shot = os.environ.get('WEB_CHECK_SHOTS')
    if shot:
        b.page("(document.querySelector('#popup-devices .fp-page').scrollTop = 0, 1)")
        time.sleep(0.5)
        with open(os.path.join(shot, f'{theme}-device.png'), 'wb') as out:
            out.write(b.shot())
        print(f'note a picture of a device\'s page with its facts and All readings in view, over {theme}, is in {shot}')
    click_summary(b)
    emptied = poll(lambda: b.page("document.getElementById('popup-devices-readings-body').innerHTML === ''"), 3.0)
    check(f'all_readings_is_a_disclosure {tag}: a second click closes it and empties the place', emptied is True)
    # ---- a phone ----
    b.resize(*PHONE)
    time.sleep(0.6)
    click_summary(b)
    poll(lambda: b.page("!!document.querySelector('#popup-devices-readings-body tr[data-reading]')"), 5.0)
    if shot and theme == 'signal':
        b.page("(document.querySelector('#popup-devices .fp-page').scrollTop = 0, 1)")
        time.sleep(0.4)
        with open(os.path.join(shot, 'signal-device-phone.png'), 'wb') as out:
            out.write(b.shot())
    r = rects(b, 'devices')
    check(f'all_readings_is_a_disclosure {tag}: at 390 px the facts and the table do not scroll sideways',
          r['over'] == [0, 0] and r['box']['w'] <= PHONE[0] + 1, r)
    click_summary(b)
    b.resize(*DESK)
    time.sleep(0.4)
    # ---- a device that is not connected ----
    b.page(f"(__core.router.step('d-{second}'), 1)")
    poll(lambda: b.page(f"{STATE}[2] === 'd-{second}'"), 5.0)
    time.sleep(0.8)
    away = b.page("""(() => { const i = document.getElementById('popup-devices-identify');
      return i ? { off: i.getAttribute('aria-disabled'), why: document.getElementById('popup-devices-identify-note').textContent } : null; })()""")
    b.page('window.__posts.length = 0')
    if away:
        press_on(b, 'popup-devices-identify')
    check(f'the_facts_stand_under_the_graphs {tag}: a device that is not connected has no Identify, or one that is disabled'
          ' with its reason and sends nothing', (away is None or (away['off'] == 'true' and 'not connected' in away['why']))
          and identify_posts(b) == [], [away, identify_posts(b)])
    shut(b)


# how each theme's own foot is reached: what is opened first, and the control to press
FOOTS = {
    'ember': ("(document.querySelector('.device-facts').open = true, 1)", '[data-device-settings]'),
    'paper': ('1', '#p-device [data-act=device-settings]'),
    'signal': ('1', '#sg-facts [data-act=device-settings]'),
    'nightdrive': ("(document.getElementById('nd-sheet').click(), 1)", '#nd-facts [data-act=device-settings]'),
}


def foot_case(b, theme):
    if theme not in FOOTS:
        return
    tag = f'({theme})'
    first = b.page('__core.model.page().fleet.devices.map((d) => d.url)')[0]
    for label, size in (('wide', DESK), ('at 390 px', PHONE)):
        b.resize(*size)
        time.sleep(0.6)
        prepare, selector = FOOTS[theme]
        b.page(BODY)
        b.page(prepare)
        time.sleep(0.6)
        b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
        time.sleep(0.3)
        b.click(*centre(b, selector))
        opened = poll(lambda: b.page(f"{STATE}[1] === 'devices'"), 8.0)
        ready(b, 'devices')
        page = b.page(f"{STATE}[2]")
        drawn = poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
        check(f'the_fold_out_foot_opens_the_page {tag} {label}: a real click on *Device settings…* opens the popup'
              ' at the page of the device, whose facts are drawn', opened is True and page == f'd-{first}'
              and drawn is True, [opened, page, drawn])
        shut(b)
        if theme == 'nightdrive':
            b.page("(document.getElementById('nd-sheet-x')?.click(), 1)")
            time.sleep(0.5)
        elif theme == 'ember':
            b.page("(document.querySelector('.device-facts').open = false, 1)")
    b.resize(*DESK)
    time.sleep(0.4)


def remove_case(b, d, theme):
    """*Remove…* of a device that is not shown, then of the shown one, then a rename after it."""
    tag = f'({theme})'
    urls = b.page('__core.model.page().fleet.devices.map((d) => d.url)')
    first, third = urls[0], urls[2]
    rm, ask = sid('remove', 'open'), sid('remove', 'ask')
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: the popup opens at the third device',
          open_popup(b, 'devices', f'd-{third}') is True)
    settle(b, f'd-{third}')
    time.sleep(0.4)
    b.page(POSTS)
    b.page('window.__posts.length = 0')
    press_on(b, rm)
    q = b.page(f"""(() => ({{ text: document.getElementById({json.dumps(ask)}).textContent,
      at: document.activeElement?.dataset.answer ?? null,
      danger: !!document.querySelector('#{ask} [data-answer=remove].danger') }}))()""")
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: the question names the device, the safe answer has the'
          ' focus and nothing is sent', q['text'].startswith('Remove Third?') and q['at'] == 'keep' and q['danger']
          and writes(b, '/remove') == [], q)
    b.press('Escape')
    time.sleep(0.4)
    closed = b.page(f"[document.getElementById({json.dumps(ask)}).textContent, document.activeElement?.id,"
                    f" {STATE}[1]]")
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: Escape closes the question alone, the focus goes back to'
          ' Remove… and nothing is sent', closed == ['', rm, 'devices'] and writes(b, '/remove') == [], closed)
    press_on(b, rm)
    b.click(*centre(b, f'#{ask} [data-answer=remove]'))
    poll(lambda: b.page(f"{STATE}[2] === 'overview'"), 8.0)
    time.sleep(0.8)
    got = b.page("""({ notice: document.getElementById('popup-devices-notice')?.textContent,
      at: document.activeElement?.id, entries: [...document.querySelectorAll('#popup-devices .fp-entry')]
      .map((e) => e.textContent), shown: __core.model.shown(), blocks: document.querySelectorAll('.fpd-device').length })""")
    posted = [p[0] for p in b.page(POSTS) if p[0].endswith('/remove')]
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: one request to the third device; the pane is the Overview'
          ' with the line, which has the focus; the list lost the entry; the page behind is unchanged',
          len(posted) == 1 and f'/devices/{third}/remove' in posted[0] and got['notice'] == 'Third was removed.'
          and got['at'] == 'popup-devices-notice' and not any('Third' in e for e in got['entries'])
          and got['shown'] == first and got['blocks'] == 2, [posted, got])
    # ---- the shown device ----
    b.page(f"(__core.router.step('d-{first}'), 1)")
    settle(b, f'd-{first}')
    time.sleep(0.4)
    press_on(b, rm)
    b.click(*centre(b, f'#{ask} [data-answer=remove]'))
    poll(lambda: b.page(f"{STATE}[2] === 'overview'"), 8.0)
    now = poll(lambda: b.page(f"__core.model.shown() !== {json.dumps(first)}"), 10.0)
    time.sleep(1.0)
    got = b.page(f"[__core.model.shown(), {STATE}, document.getElementById('popup-devices') !== null]")
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: the shown device removed: the page behind moves to the'
          ' device that is left and the popup stands on the Overview', now is True and got[0] != first
          and got[1][1:] == ['devices', 'overview'] and got[2] is True, got)
    # ---- a rename after it: the page behind ends on the renamed device ----
    left = got[0]
    b.page(f"(__core.router.step('d-{left}'), 1)")
    settle(b, f'd-{left}')
    time.sleep(0.4)
    put_text(b, NAME, 'Renamed')
    press_on(b, sid('name', 'save'))
    poll(lambda: b.page("__core.model.shown() === 'Renamed'"), 10.0)
    time.sleep(0.8)
    got = b.page(f"[__core.model.shown(), {STATE}]")
    check(f'remove_asks_once_and_ends_on_the_overview {tag}: a rename after the removal ends on the renamed device,'
          ' with the popup standing', got[0] == 'Renamed' and got[1][1:] == ['devices', 'd-Renamed'], got)
    close_popup(b, 'devices')


CONFIG = 'popup-devices-config'


def config_words(b):
    return b.page(f"document.getElementById({json.dumps(CONFIG)})?.textContent ?? null")


def hand_edit(path, secs):
    """Edit the daemon's file by hand: a comment line, and a modification time `secs` after its
    last, so the edit shows whatever the clock's granularity."""
    before = os.stat(path).st_mtime
    with open(path, 'a') as f:
        f.write('# edited by hand\n')
    os.utime(path, (before + secs, before + secs))


def config_case(b, d, theme):
    """The Overview's line that config.toml changed since the daemon's last reload."""
    tag = f'({theme})'
    path = f'{d.dir}/config/svolder/config.toml'
    words = f'config: {path} changed since the last reload: svolder daemon reload (or SIGHUP)'
    second = b.page('__core.model.page().fleet.devices.map((d) => d.url)')[1]
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    check(f'the_overview_says_the_config_changed {tag}: the popup opens on the Overview',
          open_popup(b, 'devices', 'overview') is True)
    time.sleep(0.6)
    check(f'the_overview_says_the_config_changed {tag}: no line before an edit', config_words(b) == '',
          config_words(b))
    close_popup(b, 'devices')

    hand_edit(path, 5)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    open_popup(b, 'devices', 'overview')
    poll(lambda: bool(config_words(b)), 6.0)
    seen = b.page(f"""(() => {{ const e = document.getElementById({json.dumps(CONFIG)});
      const r = e.getBoundingClientRect(); const line = e.firstElementChild?.getBoundingClientRect();
      return {{ words: e.textContent, role: e.getAttribute('role'), shown: !!line && line.height > 0,
        inside: !!line && line.left >= r.left - 1 && line.right <= r.right + 1 }}; }})()""")
    check(f'the_overview_says_the_config_changed {tag}: after a hand edit the Overview opens with the daemon\'s'
          ' words, shown, in a status place', seen['words'] == words and seen['role'] == 'status'
          and seen['shown'] and seen['inside'], seen)
    b.resize(*PHONE)
    time.sleep(0.6)
    r = rects(b, 'devices')
    check(f'the_overview_says_the_config_changed {tag}: at 390 px the line shows and nothing scrolls sideways',
          r['over'] == [0, 0] and config_words(b) == words, [r['over'], config_words(b)])
    b.resize(*DESK)
    time.sleep(0.5)

    # a write of the daemon's own is no change: it reads the file back, so the line goes with it
    # (the hand edit is a comment, which the write carries on); the name is put back after
    b.page(f"(__core.router.step('d-{second}'), 1)")
    settle(b, f'd-{second}')
    put_text(b, NAME, 'Seconds')
    press_on(b, sid('name', 'save'))
    poll(lambda: b.page(f"{STATE}[2] === 'd-Seconds'"), 10.0)
    time.sleep(0.6)
    b.page("(__core.router.step('overview'), 1)")
    time.sleep(0.6)
    check(f'the_overview_says_the_config_changed {tag}: a rename through the page leaves no line',
          config_words(b) == '', config_words(b))
    b.page("(__core.router.step('d-Seconds'), 1)")
    settle(b, 'd-Seconds')
    put_text(b, NAME, second)
    press_on(b, sid('name', 'save'))
    poll(lambda: b.page(f"{STATE}[2] === 'd-{second}'"), 10.0)
    time.sleep(0.6)

    # an edit again, then a SIGHUP: the Overview reads the list again on its own while the line
    # stands, and the line goes with no reopening
    close_popup(b, 'devices')
    hand_edit(path, 20)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    open_popup(b, 'devices', 'overview')
    poll(lambda: bool(config_words(b)), 8.0)
    check(f'the_overview_says_the_config_changed {tag}: a second edit shows again when the popup opens',
          config_words(b) == words, config_words(b))
    os.kill(d.proc.pid, signal.SIGHUP)
    gone = poll(lambda: config_words(b) == '', 16.0)
    check(f'the_overview_says_the_config_changed {tag}: a SIGHUP takes the line away at the next read, with the'
          ' popup still open', gone is True and b.page(f"{STATE}[1]") == 'devices', config_words(b))
    close_popup(b, 'devices')


# every GET of the writer's list is noted, with the POSTs the page's own hook keeps
GETS = ("(window.__lists = [], (() => { const f = window.fetch; window.fetch = (...a) => {"
        " if (/api\\/v1\\/devices$/.test(String(a[0]))) window.__lists.push(String(a[0]));"
        " return f(...a); }; })(), 1)")
WRITES_ON_PAGE = ("[...document.querySelectorAll('#popup-devices [id^=popup-devices-set-], #popup-devices-settings,"
                  " #popup-devices-identify, #popup-devices-config, #popup-devices-entry-add,"
                  " #popup-devices-walk')].map((e) => e.id)")


def reader_case(b, theme, who):
    """What a guest and a signed-in viewer of a page with writes off both get: the Devices popup,
    read-only. `who` is 'guest' or 'writes_off'."""
    guest = who == 'guest'
    name = ('a_guest_reads_the_devices_popup' if guest
            else 'a_page_with_writes_off_reads_the_devices_popup')
    tag = f'{name} ({theme})'
    b.resize(*DESK)
    time.sleep(0.4)
    b.page(BODY)
    b.page(GETS)
    b.page('window.__posts.length = 0')
    urls = b.page('__core.model.page().fleet.devices.map((d) => d.url)')
    first, second = urls[0], urls[1]
    # ---- the row, the list, the Overview ----
    check(f'{tag}: the menu\'s row opens the popup', by_the_menu(b, 'devices') is True)
    entries = b.page("[...document.querySelectorAll('#popup-devices .fp-entry')].map((e) => e.dataset.popupPage)")
    check(f'{tag}: the side list is Overview and the fleet\'s devices, with no *+ Add a device*',
          entries == ['overview'] + [f'd-{u}' for u in urls], entries)
    graphs = poll(lambda: b.page("document.querySelectorAll('#popup-devices .fpd-device .fpd-graph').length") > 0, 8.0)
    check(f'{tag}: the Overview draws each device with its key graphs, and nothing that writes',
          graphs is True and b.page("document.querySelectorAll('#popup-devices .fpd-device').length") == len(urls)
          and b.page(WRITES_ON_PAGE) == [], b.page(WRITES_ON_PAGE))
    # ---- a device's page ----
    b.page(f"(__core.router.step('d-{first}'), 1)")
    drawn = poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
    facts = b.page("""Object.fromEntries([...document.querySelectorAll('#popup-devices-facts dd[data-fact]')]
      .map((e) => [e.dataset.fact, e.textContent]))""")
    check(f'{tag}: a device\'s page draws its facts, {"without" if guest else "with"} the Unit, and no Identify,'
          ' settings or *Remove…*',
          drawn is True and ('Unit' not in facts if guest else bool(facts.get('Unit')))
          and 'Signal' in facts and b.page(WRITES_ON_PAGE) == []
          and 'Remove' not in b.page("document.getElementById('popup-devices').textContent"), [facts, b.page(WRITES_ON_PAGE)])
    order = b.page("""[...document.querySelector('#popup-devices-body .fpd-device').children].map((e) => e.id
      ? e.id.replace(/-[0-9a-f]+$/, '') : e.className)""")
    check(f'{tag}: the page has no gap where the settings stood: the graphs follow the state line',
          order[:3] == ['fpd-head', 'popup-devices-state', 'popup-devices-graphs'], order)
    click_summary(b)
    rows = poll(lambda: b.page("document.querySelectorAll('#popup-devices-readings-body tr[data-reading]').length") > 3, 5.0)
    check(f'{tag}: All readings opens', rows is True)
    click_summary(b)
    # Show on the page stays: it only picks the device the page behind shows
    b.page(f"(__core.router.step('d-{second}'), 1)")
    poll(lambda: b.page(f"{STATE}[2] === 'd-{second}'"), 5.0)
    shown = poll(lambda: b.page("!!document.getElementById('popup-devices-show-button')"), 5.0)
    check(f'{tag}: *Show on the page* is there on a device that is not shown', shown is True)
    # ---- no add page ----
    b.page("(__core.router.open('devices', 'add'), 1)")
    time.sleep(0.5)
    check(f'{tag}: an address that names the add page shows the Overview and asks the walk nothing',
          b.page("!document.getElementById('popup-devices-walk')") is True and walk_posts(b) == []
          and b.page(WRITES_ON_PAGE) == [], walk_posts(b))
    # ---- a phone ----
    b.resize(*PHONE)
    time.sleep(0.6)
    b.page(f"(__core.router.step('d-{first}'), 1)")
    poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
    r = rects(b, 'devices')
    check(f'{tag}: at 390 px the page does not scroll sideways', r['over'] == [0, 0] and r['box']['w'] <= PHONE[0] + 1, r)
    shot = os.environ.get('WEB_CHECK_SHOTS')
    if shot and not guest:
        b.page(f"(__core.router.step('overview'), 1)")
        time.sleep(0.8)
        with open(os.path.join(shot, f'{theme}-writes-off-overview-phone.png'), 'wb') as out:
            out.write(b.shot())
    b.resize(*DESK)
    time.sleep(0.5)
    if shot and guest:
        b.page(f"(__core.router.step('d-{first}'), 1)")
        poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
        b.page("(document.querySelector('#popup-devices .fp-page').scrollTop = 0, 1)")
        time.sleep(0.6)
        with open(os.path.join(shot, f'{theme}-guest-device.png'), 'wb') as out:
            out.write(b.shot())
    shut(b)
    # ---- the theme's foot reads *Devices…* and opens the same page, wide and at 390 px (a theme with no foot, Void Garden,
    # opens the popup from the menu only) ----
    for label, size in ((('wide', DESK), ('at 390 px', PHONE)) if theme in FOOTS else ()):
        b.resize(*size)
        time.sleep(0.6)
        prepare, selector = FOOTS[theme]
        b.page(BODY)
        b.page(prepare)
        time.sleep(0.6)
        b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
        time.sleep(0.3)
        words = b.page(f"document.querySelector({json.dumps(selector)}).textContent.trim()")
        if shot and label == 'wide':
            with open(os.path.join(shot, f'{theme}-{who}-foot.png'), 'wb') as out:
                out.write(b.shot())
        b.click(*centre(b, selector))
        opened = poll(lambda: b.page(f"{STATE}[1] === 'devices'"), 8.0)
        ready(b, 'devices')
        page = b.page(f"{STATE}[2]")
        drawn = poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
        check(f'{tag} {label}: the foot reads *Devices…* and a real click opens the popup at the device\'s page,'
              ' read-only', words == 'Devices…' and opened is True and page == f'd-{first}' and drawn is True
              and b.page(WRITES_ON_PAGE) == [], [words, opened, page, drawn])
        shut(b)
        if theme == 'nightdrive':
            b.page("(document.getElementById('nd-sheet-x')?.click(), 1)")
            time.sleep(0.5)
        elif theme == 'ember':
            b.page("(document.querySelector('.device-facts').open = false, 1)")
    b.resize(*DESK)
    time.sleep(0.4)
    # ---- the whole case wrote nothing and never asked the writer's list ----
    check(f'{tag}: no POST and no GET of api/v1/devices during the whole case',
          b.page(POSTS) == [] and b.page('window.__lists') == [], [b.page(POSTS), b.page('window.__lists')])


WALK = 'popup-devices-walk'
ADD_ENTRY = '#popup-devices-entry-add'
BIFROST = '1122334455667788'


def add_daemon():
    """A daemon whose first device is configured and connected, with a Bifrost 3 on the air that is
    not configured."""
    return common.Daemon(env={'SVOLDER_FAKE_MORE': f'bifrost3@{BIFROST}'})


def press_sel(b, selector):
    """A real click on the first element `selector` names, scrolled into view first."""
    b.page(f"(document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.15)
    b.click(*centre(b, selector))
    time.sleep(0.3)


def head(b):
    """The walk page's heading: the daemon's prompt title."""
    return b.page(f"document.querySelector('#{WALK}-head h4')?.textContent ?? null")


def at_step(b, title, secs=15.0):
    return poll(lambda: head(b) == title, secs)


def walk_page(b):
    """The popup is at the add page."""
    return poll(lambda: b.page(f"{STATE}[1] === 'devices' && {STATE}[2] === 'add'"), 6.0)


def to_add(b):
    """From a quiet page to the add page as a person goes: the menu's button and row, then the side
    list's last entry (a phone shows the list first)."""
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    if by_the_menu(b, 'devices') is not True:
        return False
    if b.page("document.querySelector('#popup-devices .fp-side')?.checkVisibility()") is not True:
        return False
    press_sel(b, ADD_ENTRY)
    return walk_page(b) is True


def choice_text(b, i):
    return b.page(f"document.querySelector('#{WALK}-controls [data-walk-choice=\"{i}\"]')?.textContent ?? null")


def walk_posts(b, tail=''):
    return [p for p in b.page(POSTS) if p[0].startswith('api/v1/walk') and p[0].endswith(tail)]


def last_job(d):
    states = d.walk_states()
    return (max(states), states[max(states)]) if states else (None, None)


def wait_kind(d, kind, secs=8.0, job=None):
    """The daemon's state of the newest walk (or `job`) is `kind`."""
    def read():
        states = d.walk_states(0.8)
        mine = states.get(job) if job is not None else (states[max(states)] if states else None)
        return mine if mine and mine['kind'] == kind else None
    return poll(read, secs)


def add_look_case(b, d, theme):
    """The warning, the rows heard while it listens and a stop by Escape, wide and at 390 px."""
    tag = f'({theme})'
    shot = os.environ.get('WEB_CHECK_SHOTS')
    b.resize(*DESK)
    time.sleep(0.5)
    check(f'add_a_device_draws_the_daemons_walk {tag}: the add page opens from the side list\'s last entry',
          to_add(b) is True)
    entries = b.page("[...document.querySelectorAll('#popup-devices .fp-entry')].map((e) => e.id)")
    check(f'add_a_device_draws_the_daemons_walk {tag}: *+ Add a device* is the last entry',
          entries[-1] == 'popup-devices-entry-add', entries)
    check(f'add_a_device_draws_the_daemons_walk {tag}: the page warns of the linked device',
          at_step(b, 'A device is linked', 12.0) is True, head(b))
    got = b.page(f"""(() => ({{ text: document.getElementById('{WALK}-head').textContent,
      choices: [...document.querySelectorAll('#{WALK}-controls [data-walk-choice]')].map((e) => e.textContent),
      at: document.activeElement?.dataset.walkChoice ?? null,
      pushed: history.length }}))()""")
    check(f'add_a_device_draws_the_daemons_walk {tag}: it names the device, offers Go on and Cancel, and the safe'
          ' answer (Cancel) has the focus', 'Gungnir 2' in got['text'] and got['choices'] == ['Go on', 'Cancel']
          and got['at'] == '1', got)
    style = b.page(f"""(() => {{ const q = (s) => getComputedStyle(document.querySelector(s));
      const b = q('#{WALK}-controls .fpw-choice'); const t = q('#popup-devices .fp-title');
      return {{ font: b.fontFamily === t.fontFamily, tap: parseFloat(b.minHeight) >= 36,
        ink: b.color === t.color, over: document.documentElement.scrollWidth - document.documentElement.clientWidth }}; }})()""")
    check(f'add_a_device_draws_the_daemons_walk {tag}: the controls take the frame\'s font, ink and tap size',
          style['font'] and style['tap'] and style['ink'], style)
    # ---- Go on, Scan: the rows appear while it listens ----
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    check(f'add_a_device_draws_the_daemons_walk {tag}: Go on leads to the scan prompt',
          at_step(b, 'Find the device', 6.0) is True, head(b))
    pushed = b.page('history.length')
    # the fake hears at once, so the listening is over in a moment: an observer notes what the page
    # drew while it lasted (and the page of the scan, kept, is what the picture shows)
    b.page(f"""(() => {{ window.__scan = {{ rows: [], listening: false, busy: null }};
      const root = document.getElementById('popup-devices');
      new MutationObserver(() => {{
        const rows = [...document.querySelectorAll('#{WALK}-rows li')];
        if (rows.length > window.__scan.rows.length) {{
          window.__scan.rows = rows.map((e) => [e.dataset.unit, e.dataset.configured ?? null]);
          window.__scan.busy = [...document.querySelectorAll('#{WALK}-controls button')]
            .filter((e) => e.id !== '{WALK}-stop').every((e) => e.getAttribute('aria-disabled') === 'true');
        }}
        if (/Listening/.test(document.getElementById('{WALK}-status')?.textContent ?? '')) window.__scan.listening = true;
      }}).observe(root, {{ subtree: true, childList: true, characterData: true }}); return 1; }})()""")
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    at_step(b, 'Which device', 20.0)
    scan = b.page('window.__scan')
    check(f'add_a_device_draws_the_daemons_walk {tag}: units were listed while it listened, a configured one saying'
          ' which device it is, and only Stop could be pressed', scan['listening'] is True
          and len(scan['rows']) == 2 and any(r[1] == 'Gungnir 2' for r in scan['rows']) and scan['busy'] is True, scan)
    if shot:
        time.sleep(0.3)
        with open(os.path.join(shot, f'{theme}-add-pick.png'), 'wb') as out:
            out.write(b.shot())
    b.press('Escape')
    gone = poll(lambda: b.page(f"{STATE}[1] === null"), 4.0)
    stopped = wait_kind(d, 'cancelled', 8.0)
    check(f'add_a_device_draws_the_daemons_walk {tag}: Escape at the pick closes the popup and the daemon\'s walk is'
          ' cancelled', gone is True and bool(stopped), [gone, stopped])
    check(f'add_a_device_draws_the_daemons_walk {tag}: the steps pushed one entry at most (the page\'s own) and the'
          ' router never warned of a push with no gesture',
          b.page('history.length') - pushed <= 0 and b.page(UNTAPPED) == [], [pushed, b.page('history.length')])
    # ---- a phone ----
    b.resize(*PHONE)
    time.sleep(0.6)
    check(f'add_a_device_draws_the_daemons_walk {tag}: at 390 px the add page opens too', to_add(b) is True)
    at_step(b, 'A device is linked', 12.0)
    time.sleep(0.4)
    r = rects(b, 'devices')
    check(f'add_a_device_draws_the_daemons_walk {tag}: at 390 px the page does not scroll sideways and the warning'
          ' stands in the box', r['over'] == [0, 0] and r['box']['w'] <= PHONE[0] + 1
          and b.page(f"!!document.querySelector('#{WALK}-controls [data-walk-choice=\"1\"]')?.checkVisibility()") is True, r)
    if shot:
        with open(os.path.join(shot, f'{theme}-add-warning-phone.png'), 'wb') as out:
            out.write(b.shot())
    b.page('history.back()')
    list_again = poll(lambda: b.page(f"{STATE}[1] === 'devices' && {STATE}[2] === null"), 6.0)
    stopped = wait_kind(d, 'cancelled', 8.0)
    check(f'add_a_device_draws_the_daemons_walk {tag}: a phone\'s Back to the list stops the walk',
          list_again is True and bool(stopped), [list_again, stopped])
    b.page('(__core.router.close(), 1)')
    time.sleep(0.4)
    b.resize(*DESK)
    time.sleep(0.4)


def add_flow_case(b, d, theme):
    """A whole walk to the end, then Escape and Back mid-walk, a reload, and a walk of the socket."""
    tag = f'({theme})'
    name = 'add_a_device_walks_to_the_end'
    check(f'{name} {tag}: the add page opens', to_add(b) is True)
    # one linked device: the warning comes first
    at_step(b, 'A device is linked', 12.0)
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    at_step(b, 'Find the device', 6.0)
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    picked = at_step(b, 'Which device', 20.0)
    choices = b.page(f"[...document.querySelectorAll('#{WALK}-controls [data-walk-choice]')].map((e) => [e.textContent,"
                     " e.getAttribute('aria-disabled')])")
    check(f'{name} {tag}: the pick lists the units heard; the configured one is marked and cannot be pressed',
          picked is True and len(choices) == 2 and any(c[1] == 'true' and 'configured as Gungnir 2' in c[0] for c in choices)
          and any(c[1] is None and BIFROST in c[0] for c in choices), choices)
    index = next((i for i, c in enumerate(choices) if BIFROST in c[0]), 1)
    press_sel(b, f'#{WALK}-controls [data-walk-choice="{index}"]')
    named = at_step(b, 'What to call it', 10.0)
    field = b.page(f"(() => {{ const f = document.getElementById('{WALK}-field');"
                   " return [f?.placeholder, f?.value, document.activeElement === f]; })()")
    check(f'{name} {tag}: the name step is a field that holds nothing, with the daemon\'s default as its hint and'
          ' the focus', named is True and field == ['Bifrost 3', '', True], field)
    put_text(b, f'{WALK}-field', 'Desk DAC')
    pushed = b.page('history.length')
    b.press('Enter')
    done = poll(lambda: b.page(f"!!document.getElementById('{WALK}-done')"), 25.0)
    time.sleep(0.5)
    card = b.page(f"document.getElementById('{WALK}-status').textContent.replace(/\\s+/g, ' ').trim()")
    entries = b.page("[...document.querySelectorAll('#popup-devices .fp-entry')].map((e) => e.textContent.trim())")
    listed = [row['name'] for row in json.load(urllib_open(d, 'api/v1/devices'))['devices']]
    check(f'{name} {tag}: done says the device\'s name, and the list gains it', done is True
          and card.startswith('Desk DAC was added.') and any('Desk DAC' in e for e in entries) and 'Desk DAC' in listed,
          [card, entries, listed])
    linked = poll(lambda: 'It is connected.' in b.page(f"document.getElementById('{WALK}-status').textContent"), 12.0)
    check(f'{name} {tag}: its link is the fleet\'s, put in place when it connects', linked is True, card)
    check(f'{name} {tag}: no entry was pushed by the walk\'s steps', b.page('history.length') == pushed
          and b.page(f"{STATE}[2]") == 'add', [pushed, b.page('history.length')])
    press_sel(b, f'#{WALK}-open')
    opened = poll(lambda: b.page(f"{STATE}[2] === 'd-Desk%20DAC'"), 5.0)
    check(f'{name} {tag}: *Open its page* opens the device\'s page with no router warning of a push with no gesture',
          opened is True and b.page(UNTAPPED) == [], b.page(UNTAPPED))
    # ---- Escape mid-walk ----
    press_sel(b, ADD_ENTRY)
    at_step(b, 'A device is linked', 12.0)
    job = last_job(d)[0]
    b.press('Escape')
    gone = poll(lambda: b.page(f"{STATE}[1] === null"), 4.0)
    stopped = wait_kind(d, 'cancelled', 8.0, job)
    check(f'{name} {tag}: Escape mid-walk stops it by its job; the daemon says cancelled', gone is True and bool(stopped),
          [job, gone, stopped])
    # ---- a reload mid-walk, then the walk is taken over where it stands ----
    name = 'add_a_device_rejoins_a_walk'
    check(f'{name} {tag}: the add page opens again', to_add(b) is True)
    at_step(b, 'A device is linked', 12.0)
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    at_step(b, 'Find the device', 6.0)
    job = last_job(d)[0]
    load(b, d, theme)
    check(f'{name} {tag}: after a reload the add page opens again', to_add(b) is True)
    check(f'{name} {tag}: the walk is shown where it stands (the scan prompt), answerable',
          at_step(b, 'Find the device', 10.0) is True and choice_text(b, 0) == 'Scan', head(b))
    b.page('window.__posts.length = 0')
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    time.sleep(0.5)
    posted = walk_posts(b, '/answer')
    check(f'{name} {tag}: its answer goes to the walk that was running, not to a new one',
          len(posted) == 1 and posted[0][0] == f'api/v1/walk/{job}/answer' and not walk_posts(b, 'v1/walk'), posted)
    b.press('Escape')
    wait_kind(d, 'cancelled', 8.0, job)
    # ---- a walk of the socket is read-only ----
    started = d.socket({'cmd': 'walk', 'op': {'op': 'start'}})
    other = started.get('job')
    check(f'{name} {tag}: a walk can be started over the socket', isinstance(other, int), started)
    check(f'{name} {tag}: the add page opens beside it', to_add(b) is True)
    said = poll(lambda: 'started from the terminal' in b.page(f"document.getElementById('{WALK}-status').textContent"), 8.0)
    shown = b.page(f"[document.querySelectorAll('#{WALK}-controls button, #{WALK}-controls input').length,"
                   f" document.getElementById('{WALK}-status').textContent.replace(/\\s+/g, ' ').trim()]")
    check(f'{name} {tag}: it is shown read-only: who started it, its step\'s title, nothing to press',
          said is True and shown[0] == 0 and 'socket' not in shown[1] and 'Which device' not in shown[1] and 'A device is linked' in shown[1], shown)
    b.press('Escape')
    time.sleep(0.8)
    left = d.walk_states().get(other, {})
    check(f'{name} {tag}: leaving does not stop it', left.get('kind') == 'progress', left)
    d.socket({'cmd': 'walk', 'op': {'op': 'cancel', 'job': other}})
    wait_kind(d, 'cancelled', 8.0, other)


def urllib_open(d, path):
    import urllib.request
    return urllib.request.urlopen(d.url + path, timeout=10)


def first_device_case(b, d, theme):
    """No device configured: the add page's walk, to done, and the page behind shows the device."""
    tag = f'({theme})'
    name = 'add_the_first_device_of_a_fresh_install'
    check(f'{name} {tag}: the page shows no device at first', b.page('__core.model.shown()') is None)
    check(f'{name} {tag}: the add page opens by the menu', to_add(b) is True)
    # nothing is linked: the walk starts at its scan
    first = at_step(b, 'Find the device', 12.0)
    check(f'{name} {tag}: with no device linked there is no warning, the walk starts at its scan', first is True, head(b))
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    at_step(b, 'Which device', 20.0)
    press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
    at_step(b, 'What to call it', 10.0)
    pushed = b.page('history.length')
    b.press('Enter')
    done = poll(lambda: b.page(f"!!document.getElementById('{WALK}-done')"), 25.0)
    shown = poll(lambda: b.page('__core.model.shown() !== null'), 15.0)
    time.sleep(0.8)
    got = b.page(f"[__core.model.shown(), __core.model.page().card?.kind ?? null, history.length, {STATE}[2]]")
    check(f'{name} {tag}: done, and the page behind now shows the device, the empty card gone, no entry pushed',
          done is True and shown is True and got[1] != 'dim' and got[2] == pushed and got[3] == 'add', [done, got, pushed])
    b.page('(__core.router.close(), 1)')


def socket_walk_to_done(d, secs=40.0):
    """A walk over the daemon's socket (no browser), answered step by step to its end; the last
    state's kind."""
    started = d.socket({'cmd': 'walk', 'op': {'op': 'start'}})
    job = started.get('job')
    end = time.time() + secs
    last = None
    while time.time() < end and isinstance(job, int):
        state = d.walk_states(0.8).get(job, {})
        last = state.get('kind')
        if last in ('done', 'failed', 'cancelled'):
            break
        if last == 'progress' and state.get('phase') == 'waiting':
            op = {'op': 'answer', 'job': job, 'choice': 0}
            if state.get('step') == 'name':
                op['text'] = 'Added again'
            d.socket({'cmd': 'walk', 'op': op})
    return last


def add_back_case(b, d, theme):
    """The last device is removed and one is added back (here over the socket, as the TUI would):
    the page behind shows the empty card, then takes up the device without *Show on the page* or a
    reload."""
    tag = f'({theme})'
    name = 'a_device_added_after_the_last_was_removed_is_taken_up_by_the_page'
    check(f'{name} {tag}: the page shows no device at first', b.page('__core.model.shown()') is None)
    check(f'{name} {tag}: a walk over the socket adds the first device',
          socket_walk_to_done(d) == 'done')
    check(f'{name} {tag}: the page takes it up', poll(lambda: b.page('__core.model.shown() !== null'), 15.0) is True)
    time.sleep(0.8)
    url = b.page('__core.model.shown()')
    rm, ask = sid('remove', 'open'), sid('remove', 'ask')
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    check(f'{name} {tag}: the popup opens at the device', open_popup(b, 'devices', f'd-{url}') is True)
    settle(b, f'd-{url}')
    time.sleep(0.4)
    press_on(b, rm)
    b.click(*centre(b, f'#{ask} [data-answer=remove]'))
    poll(lambda: b.page(f"{STATE}[2] === 'overview'"), 8.0)
    empty = poll(lambda: b.page("__core.model.shown() === null"), 10.0)
    time.sleep(0.8)
    got = b.page("__core.model.page().card?.kind ?? null")
    check(f'{name} {tag}: the last device removed leaves the page behind on the no-device card',
          empty is True and got == 'dim', [empty, got])
    close_popup(b, 'devices')
    mark = b.page("(window.__same = Math.random(), window.__same)")
    last = socket_walk_to_done(d)
    check(f'{name} {tag}: a walk over the socket adds a device', last == 'done', last)
    taken = poll(lambda: b.page("__core.model.shown() !== null"), 15.0)
    time.sleep(0.8)
    got = b.page("""({ shown: __core.model.shown(), card: __core.model.page().card?.kind ?? null,
      same: window.__same, bodyDim: document.body.classList.contains('lost') })""")
    check(f'{name} {tag}: the page takes up the device with no reload and no Show on the page',
          taken is True and got['card'] != 'dim' and got['same'] == mark and not got['bodyDim'], [taken, got, mark])


def add_back_cases():
    for theme in THEMES:
        h = common.Daemon(blank=True, env={'SVOLDER_FAKE_MORE': f'bifrost3@{BIFROST}'})
        try:
            b = modal.browser_for(browser, DESK, theme)
            try:
                b.preload(modal.HOOK + ' ' + WARNS)
                load(b, h, theme)
                b.page(BODY)
                add_back_case(b, h, theme)
            finally:
                b.close()
        finally:
            h.stop()


def main():
    if os.environ.get('DEVICES_POPUP_ONLY') == 'add_back':
        add_back_cases()
        print(f'devices_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
        sys.exit(1 if modal.fails else 0)
    # each theme has a daemon of its own: a case that removes or renames a device leaves a known
    # state for the next case, never for the next theme
    for theme in THEMES:
        d = common.Daemon(host='fake', more=[('Second', '1112131415161718'), ('Third', '2122232425262728')])
        try:
            b = modal.browser_for(browser, DESK, theme)
            try:
                b.preload(modal.HOOK + ' ' + WARNS)
                load(b, d, theme)
                b.page(BODY)
                devices_case(b, d, theme)
                facts_case(b, d, theme)
                foot_case(b, theme)
                config_case(b, d, theme)
                remove_case(b, d, theme)
            finally:
                b.close()
        finally:
            d.stop()
        # the add page, on a daemon with a unit on the air that is not configured
        a = add_daemon()
        try:
            b = modal.browser_for(browser, DESK, theme)
            try:
                b.preload(modal.HOOK + ' ' + WARNS)
                load(b, a, theme)
                b.page(BODY)
                add_look_case(b, a, theme)
                if theme == THEMES[0]:
                    add_flow_case(b, a, theme)
            finally:
                b.close()
        finally:
            a.stop()
    f = common.Daemon(blank=True, env={'SVOLDER_FAKE_MORE': f'bifrost3@{BIFROST}'})
    try:
        b = modal.browser_for(browser, DESK, THEMES[0])
        try:
            b.preload(modal.HOOK + ' ' + WARNS)
            load(b, f, THEMES[0])
            b.page(BODY)
            first_device_case(b, f, THEMES[0])
        finally:
            b.close()
    finally:
        f.stop()
    add_back_cases()
    g = common.Daemon(host='fake', web='guest = "read"',
                      more=[('Second', '1112131415161718'), ('Third', '2122232425262728')])
    try:
        for theme in THEMES:
            b = modal.browser_for(browser, DESK, theme)
            try:
                b.preload(modal.HOOK + ' ' + WARNS)
                load(b, g, theme)
                b.page(BODY)
                reader_case(b, theme, 'guest')
            finally:
                b.close()
    finally:
        g.stop()
    w = common.Daemon(host='fake', web='auth = "none"\nwrites = false',
                      more=[('Second', '1112131415161718'), ('Third', '2122232425262728')])
    try:
        for theme in THEMES:
            b = modal.browser_for(browser, DESK, theme)
            try:
                b.preload(modal.HOOK + ' ' + WARNS)
                load(b, w, theme)
                b.page(BODY)
                reader_case(b, theme, 'writes_off')
            finally:
                b.close()
    finally:
        w.stop()
    print(f'devices_popup.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
