# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Paper in a browser: its registration and its sheet (the title, the footer, the menu
button over the title's slot, the menu's rows), light paper and dark paper, and the first frame of
dark paper. Later tasks add the sections' checks here.
  paper_mounts_from_the_address_and_from_the_picker   `?theme=paper` and a pick from Ember
  the_title_and_the_footer_say_what_the_model_says     name, kind and link, version, the key, the
                                                       theme's name
  the_button_is_the_ink_circle_over_the_slot           the frame's button sits on the title's slot
  auto_follows_the_system                              the system's setting gives dark or light paper
  the_menu_offers_auto_light_and_dark_and_keeps_the_choice
  dark_paper_is_painted_dark_from_the_first_byte       dark chosen, a switch to Ember and back,
                                                       a reload: never a flash of light paper
  contrast_in_light_and_dark                           every kind of text reads at AA on its sheet
  no_sideways_scroll_at_390                            light and dark
  the_needle_sits_at_its_value                         the needle's place and the written level
  dragging_the_needle_writes_its_steps                 a drag along the ruler
  the_written_volume_words_write                       louder, quieter, mute and unmute
  the_needle_takes_the_arrow_keys                      arrows, Page Up and Down, Home; End posts nothing
  dragging_past_the_cap_writes_the_cap                 [web] max_volume_db holds a drag
  a_reading_from_elsewhere_during_a_needle_drag_lands_after_it   (Chromium)
  switching_to_ember_and_back_leaves_one_listener      one tap, one volume write
  a_band_point_drag_writes_the_steps                   a point dragged to a place writes its steps
  the_band_table_steps_write                           the table's - and + write one step
  the_mode_list_chooses_and_closes                     the mode word opens a list: keys, Escape
  the_preset_list_applies_a_preset                     a preset chosen from its list
  the_loudness_strength_steps                          the strength's - and +
  a_tone_update_waits_for_the_point_drag               (Chromium) a change from elsewhere lands
                                                       after the release
  a_choice_word_writes_its_option                      `Optical` writes the input, the underline moves
  a_toggle_word_flips                                  the phase's `on` and `off`
  the_balance_scale_drags_and_centres                  a drag, the keys, a tap on the value
  presets_apply_save_rename_and_delete_with_papers_questions   the forms and the delete question
  a_slot_save_asks_then_writes_with_confirm            `confirm: true` only after `save`
  a_built_in_setting_applies                           `Bass Boost` on a Loki Max
  a_refused_form_stays_open_with_its_text              a name already taken
  an_open_paper_list_survives_a_part_update            (Chromium) the list keeps its focus
  a_device_switch_closes_what_is_open                  a form or a list open on one device
  a_controls_list_holds_its_section                    (Chromium) the same for a control's list
  a_touch_beside_a_point_scrolls_and_on_it_drags       (Chromium) the phone graph's touch
  the_transport_words_press                            `next` posts the host's press and the track changes
  an_amp_shows_power_meters_and_its_readouts           a needle per meter at the model's fraction; power
  the_spec_table_lists_every_fact                      each fact with the model's words
  the_states_in_papers_words                           the card as the red note, try now, the lost
                                                       stream, the waiting page
  a_second_device_is_one_tap_away                      `also here: Amp` shows the amp
  no_sideways_scroll_at_390_with_long_words            long names and titles, DAC, amp and Loki Max
  notes_appear_and_update                              the notes from the model, the EQ's words and the
                                                       track's time move
  from_this_browser_only_for_this_pages_change         the last change, and whose it is
  each_note_sits_near_what_it_is_about                 the band's third on a desktop, the flow on a phone
  one_red_note_is_circled                              one red note, the ellipse, the ring, the arrows
Usage: python3 paper.py chromium|firefox"""
import json
import math
import re
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
# every change of `data-scheme` on any element, from before the page's own scripts run: the value
# it had and the value it has
WATCH = ("window.__schemes = []; new MutationObserver((records) => { for (const r of records) {"
         " window.__schemes.push([r.oldValue, r.target.getAttribute('data-scheme')]); } })"
         ".observe(document, { attributes: true, subtree: true, attributeFilter: ['data-scheme'],"
         " attributeOldValue: true });")
BG = {'dark': 'rgb(30, 31, 34)', 'light': 'rgb(243, 239, 230)'}
PICKER_CARDS = "[...document.querySelectorAll('#picker .tcard')].map((c) => c.dataset.theme)"


def make(size=(1280, 900), **kw):
    if browser == 'chromium':
        b = common.Chromium(size=size, **kw)
    else:
        b = common.Firefox(size=size, **kw)
    b.preload(WATCH)
    return b


def errors(b):
    """The page errors so far, but for the browser's own request for a site icon, which a document
    with no icon link (the one `forget` stands on) draws."""
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'favicon.ico' not in e]


def expect_error(b, fragment):
    """The one console error `fragment` names is an answer the check asked for (a refused write the
    daemon answers 422): taken out of the errors so far, once."""
    errors(b)
    for e in b.errors:
        if fragment in e:
            b.errors.remove(e)
            return True
    return False


def poll(read, secs=5.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, query='?theme=paper'):
    """Load the page and wait for the theme to be on show: the id of the theme mounted."""
    b.goto(f'{d.url}{query}', settle=1.0)
    return b.page(BOOT)


def forget(b, d):
    """This browser forgets its choice of theme and every preference (storage and cookie)."""
    b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
    b.page("""(() => { try { localStorage.clear(); } catch (e) {}
      document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()""")


def click_el(b, selector):
    x, y = common.center(b, selector)
    b.click(x, y)


def html_scheme(b):
    return b.page('document.documentElement.dataset.scheme ?? null')


def background(b):
    return b.page('getComputedStyle(document.body).backgroundColor')


def choose(b, mode):
    """Choose Auto, Light or Dark in the frame's menu, with the mouse."""
    click_el(b, '.fxbtn')
    poll(lambda: b.page("!document.getElementById('fx-menu').hidden"))
    click_el(b, f'.fxpop [data-scheme={mode}]')
    time.sleep(0.3)


def served(b):
    """The page as the server sends it to this browser now (its cookie goes with the request)."""
    return b.page("fetch(location.pathname, { credentials: 'same-origin' }).then((r) => r.text())")


def the_sheet(b, d):
    forget(b, d)
    check('paper_mounts_from_the_address_and_from_the_picker: ?theme=paper mounts Paper',
          load(b, d) == 'paper')
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      sheet: !!document.getElementById('paper'), sheets: [...document.querySelectorAll('link[data-sheet]')]
        .map((l) => l.dataset.sheet), buttons: document.querySelectorAll('#fx-button').length })""")
    check('  the sheet is drawn, with Paper\'s sheet of styles and one menu button',
          on == {'theme': 'paper', 'sheet': True, 'sheets': ['paper'], 'buttons': 1}, on)
    check('  no page error', errors(b) == [], errors(b)[:3])
    forget(b, d)
    check('  the page without the theme is Ember', load(b, d, '?effects=off') == 'ember')
    b.press('T')
    cards = b.page(PICKER_CARDS)
    check('  the picker lists Ember and Paper', cards == ['ember', 'paper'], cards)
    previews = b.page("[...document.querySelectorAll('#picker .tcard')]"
                      ".map((c) => [c.querySelector('b').textContent,"
                      " c.querySelector('small').textContent, !!c.querySelector('.tprev svg')])")
    check('  and Paper\'s card has its name, its blurb and a picture',
          previews[1] == ['Paper', 'light, ink and notes', True], previews)
    click_el(b, '#picker .tcard[data-theme=paper]')
    check('  a pick of Paper from Ember mounts it',
          poll(lambda: b.page("!!document.getElementById('paper')")))
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      sheets: [...document.querySelectorAll('link[data-sheet]')].map((l) => l.dataset.sheet),
      ember: !!document.querySelector('#screen, .fx'),
      buttons: document.querySelectorAll('#fx-button').length })""")
    check('  and leaves nothing of Ember',
          on == {'theme': 'paper', 'sheets': ['paper'], 'ember': False, 'buttons': 1}, on)
    check('  no page error after the switch', errors(b) == [], errors(b)[:3])


def title_and_footer(b, d):
    forget(b, d)
    load(b, d)
    got = b.page("""(() => { const box = (s) => { const r = document.querySelector(s)
        .getBoundingClientRect(); return [r.width, r.height]; };
      return { h1: document.querySelector('#p-title h1').textContent,
        sub: document.querySelector('#p-title .p-sub').textContent,
        foot: document.getElementById('p-foot').textContent,
        version: __core.model.page().viewer.version, slot: box('.paper-slot'),
        card: document.getElementById('p-card').children.length }; })()""")
    check('the_title_and_the_footer_say_what_the_model_says: the title names the device and its line',
          got['h1'] == 'Gungnir 2' and got['sub'].startswith('DAC, connected'), got)
    check('  the footer has the version, the key to the themes and the theme\'s name',
          f"svolder {got['version']}" in got['foot'] and 'T for themes' in got['foot']
          and got['foot'].endswith('Paper'), got['foot'])
    check('  the slot is 30 by 30 and a device that is reachable has no card',
          got['slot'] == [30, 30] and got['card'] == 0, got)
    # an open page across midnight: its date is written again by the clock's tick
    b.page("document.querySelector('#p-foot .p-left').textContent = 'svolder 0.0.0 \\u00b7 1 January'")
    today = b.page("new Date().toLocaleDateString('en-GB', { day: 'numeric', month: 'long' })")
    check('  the footer\'s date is written again by the tick, the version kept',
          poll(lambda: b.page("document.querySelector('#p-foot .p-left').textContent")
               == f"svolder 0.0.0 \u00b7 {today}", 4.0), b.page("document.querySelector('#p-foot .p-left').textContent"))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_button(b, d):
    forget(b, d)
    load(b, d)
    got = b.page("""(() => { const c = (s) => { const r = document.querySelector(s)
        .getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
      const [bx, by] = c('#fx-button'); const [sx, sy] = c('.paper-slot');
      const at = document.elementFromPoint(bx, by); const button = document.getElementById('fx-button');
      return { dx: Math.abs(bx - sx), dy: Math.abs(by - sy), top: at === button || button.contains(at),
        label: button.getAttribute('aria-label'),
        circle: getComputedStyle(button).borderRadius, svg: !!button.querySelector('svg circle.solid') };
    })()""")
    check('the_button_is_the_ink_circle_over_the_slot: its centre is the slot\'s, within 2 px',
          got['dx'] <= 2 and got['dy'] <= 2, got)
    check('  it is on top there, a round button called Menu with the ink circle in it',
          got['top'] and got['label'] == 'Menu' and got['circle'] in ('50%', '15px')
          and got['svg'], got)
    b.resize(390, 900)
    got = b.page("""(() => { const c = (s) => { const r = document.querySelector(s)
        .getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
      const [bx, by] = c('#fx-button'); const [sx, sy] = c('.paper-slot');
      return { dx: Math.abs(bx - sx), dy: Math.abs(by - sy), left: sx }; })()""")
    check('  and over the slot at a phone\'s width too, which is the right end of the title',
          got['dx'] <= 2 and got['dy'] <= 2 and got['left'] > 300, got)
    b.resize(1280, 900)


def auto(d):
    """Auto follows the system: Chromium's emulated setting, a Firefox profile that says dark."""
    if browser == 'chromium':
        b = make()
        try:
            def system(mode):
                b.send('Emulation.setEmulatedMedia', {'features': [
                    {'name': 'prefers-color-scheme', 'value': mode},
                    {'name': 'prefers-reduced-motion', 'value': 'no-preference'}]})
                time.sleep(0.4)

            system('dark')
            forget(b, d)
            load(b, d)
            check('auto_follows_the_system: a dark system gives dark paper',
                  html_scheme(b) == 'dark' and background(b) == BG['dark'],
                  [html_scheme(b), background(b)])
            system('light')
            check('  and a light one gives light paper, the page not reloaded',
                  poll(lambda: html_scheme(b) == 'light') and background(b) == BG['light'],
                  [html_scheme(b), background(b)])
            system('dark')
            check('  and dark again', poll(lambda: html_scheme(b) == 'dark')
                  and background(b) == BG['dark'])
            choose(b, 'light')
            check('  a person\'s Light beats a dark system', html_scheme(b) == 'light'
                  and background(b) == BG['light'], [html_scheme(b), background(b)])
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            b.close()
    else:
        b = common.Firefox(extra_prefs=['user_pref("ui.systemUsesDarkTheme", 1);'])
        try:
            b.preload(WATCH)
            forget(b, d)
            load(b, d)
            check('auto_follows_the_system: a system that says dark gives dark paper',
                  html_scheme(b) == 'dark' and background(b) == BG['dark'],
                  [html_scheme(b), background(b)])
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            b.close()


def the_menu(b, d):
    forget(b, d)
    load(b, d)
    click_el(b, '.fxbtn')
    poll(lambda: b.page("!document.getElementById('fx-menu').hidden"))
    rows = b.page("""({ heading: document.querySelector('.fxpop .fxh').textContent,
      rows: [...document.querySelectorAll('.fxpop [data-scheme]')].map((r) => [r.dataset.scheme,
        r.textContent, r.getAttribute('role'), r.getAttribute('aria-checked')]) })""")
    check('the_menu_offers_auto_light_and_dark_and_keeps_the_choice: a Paper heading, then Auto, '
          'Light and Dark as radios, Auto checked',
          rows['heading'] == 'Paper' and rows['rows'] == [
              ['auto', 'Auto', 'menuitemradio', 'true'], ['light', 'Light', 'menuitemradio', 'false'],
              ['dark', 'Dark', 'menuitemradio', 'false']], rows)
    click_el(b, '.fxpop [data-scheme=dark]')
    time.sleep(0.3)
    got = b.page("""({ scheme: document.documentElement.dataset.scheme, cookie: document.cookie,
      colour: document.querySelector('meta[name=theme-color]').content,
      meta: document.querySelector('meta[name=color-scheme]').content })""")
    check('  Dark paints dark paper, keeps svolder_theme=paper.dark and the browser\'s colour follows',
          got['scheme'] == 'dark' and 'svolder_theme=paper.dark' in got['cookie']
          and got['colour'] == '#1e1f22' and got['meta'] == 'dark', got)
    check('  the sheet is the dark one', background(b) == BG['dark'], background(b))
    load(b, d, '')
    check('  after a reload the served page carries data-scheme="dark" and stays dark',
          '<html lang="en" data-theme="paper" data-scheme="dark">' in served(b)
          and html_scheme(b) == 'dark' and background(b) == BG['dark'])
    choose(b, 'light')
    got = b.page("""({ scheme: document.documentElement.dataset.scheme, cookie: document.cookie,
      colour: document.querySelector('meta[name=theme-color]').content })""")
    check('  Light paints light paper and the browser\'s colour',
          got['scheme'] == 'light' and 'svolder_theme=paper.light' in got['cookie']
          and got['colour'] == '#f3efe6' and background(b) == BG['light'], got)
    choose(b, 'auto')
    got = b.page('document.cookie')
    check('  Auto keeps the theme alone in the cookie, whatever the system says',
          re.search(r'svolder_theme=paper(;|$)', got) is not None, got)
    click_el(b, '.fxbtn')
    checked = b.page("[...document.querySelectorAll('.fxpop [data-scheme]')]"
                     ".filter((r) => r.getAttribute('aria-checked') === 'true').map((r) => r.dataset.scheme)")
    check('  and the menu shows Auto checked again', checked == ['auto'], checked)
    check('  no page error', errors(b) == [], errors(b)[:3])


def dark_first(b, d):
    """Review Focus 1: dark paper chosen, then a switch to Ember and back, then a reload: the
    first byte is dark and `data-scheme` never becomes light."""
    forget(b, d)
    load(b, d)
    choose(b, 'dark')
    check('dark_paper_is_painted_dark_from_the_first_byte: Dark is chosen',
          html_scheme(b) == 'dark' and 'svolder_theme=paper.dark' in b.page('document.cookie'))
    b.page('window.__schemes.length = 0')
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    check('  a switch to Ember leaves no scheme on the page',
          poll(lambda: b.page("document.documentElement.dataset.theme === 'ember' "
                              "&& !!document.querySelector('#screen')"))
          and html_scheme(b) is None, html_scheme(b))
    cookie = b.page('document.cookie')
    check('  and the cookie names Ember alone', 'svolder_theme=ember' in cookie
          and 'svolder_theme=ember.' not in cookie, cookie)
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=paper]')
    check('  a switch back by the picker brings dark paper, not light',
          poll(lambda: b.page("!!document.getElementById('paper')"))
          and html_scheme(b) == 'dark' and background(b) == BG['dark'],
          [html_scheme(b), background(b)])
    cookie = b.page('document.cookie')
    check('  and the cookie says so again', 'svolder_theme=paper.dark' in cookie, cookie)
    seen = b.page('window.__schemes')
    check('  in all of it the scheme was never light',
          all(now != 'light' and was != 'light' for was, now in seen), seen)
    b.goto(d.url, settle=1.0)
    b.page(BOOT)
    check('  a reload is served dark paper in the first byte',
          '<html lang="en" data-theme="paper" data-scheme="dark">' in served(b)
          and 'content="#1e1f22"' in served(b))
    seen = b.page('window.__schemes')
    check('  and the page it paints never shows light, an observer set before the module ran',
          html_scheme(b) == 'dark' and all(now != 'light' and was != 'light' for was, now in seen),
          seen)
    check('  no page error', errors(b) == [], errors(b)[:3])


# each kind of text the sheet writes: what to find, and where to put a sample when this task's
# sections do not draw one yet
SAMPLES = [
    ('.p-sub', None, None), ('#p-title h1', None, None), ('#p-foot', None, None),
    ('.p-label', None, None),
    ('.pn:not(.red)', None, None), ('.pn.red', None, None),
    ('#p-volume-text', None, None),
]
CONTRAST = """(samples) => {
  const num = (c) => c.match(/[\\d.]+/g).map(Number);
  const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92
    : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const bg = num(getComputedStyle(document.body).backgroundColor);
  const out = {};
  for (const [selector, parent, html] of samples) {
    let el = document.querySelector(selector);
    if (!el && parent) { document.querySelector(parent).insertAdjacentHTML('beforeend', html);
      el = document.querySelector(selector); }
    if (!el) { out[selector] = null; continue; }
    const fg = num(getComputedStyle(el).color);
    const [hi, lo] = [lum(fg), lum(bg)].sort((a, b) => b - a);
    out[selector] = Math.round((hi + 0.05) / (lo + 0.05) * 100) / 100;
  }
  return out;
}"""


def contrast(b, d):
    # a daemon whose route changes the sound has a red note (the path) beside the blue ones
    red = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_ROUTE': 'changed'})
    try:
        forget(b, red)
        load(b, red)
        poll(lambda: b.page("!!document.querySelector('.pn.red')"), 10.0)
        for mode in ('light', 'dark'):
            b.page(f"__core.scheme.set('{mode}')")
            time.sleep(0.3)
            got = b.page(f'({CONTRAST})({json.dumps(SAMPLES)})')
            check(f'contrast_in_light_and_dark: every text on {mode} paper reads at 4.5 or more',
                  all(v is not None and v >= 4.5 for v in got.values()), got)
    finally:
        forget(b, d)  # off the stream before its daemon goes
        red.stop()


def card_contrast():
    """The red note of a real card (a device out of reach) reads at AA in light and dark."""
    gone = b = None
    try:
        gone = common.Daemon('gungnir2-v2')
        b = make()
        gone.drop()
        forget(b, gone)
        load(b, gone)
        poll(lambda: b.page("!!document.querySelector('.pn.red.card')"), 20.0)
        for mode in ('light', 'dark'):
            b.page(f"__core.scheme.set('{mode}')")
            time.sleep(0.3)
            got = b.page(f"({CONTRAST})({json.dumps([('.pn.red.card', None, None)])})")
            check(f'contrast_in_light_and_dark: the card\'s red note on {mode} paper reads at 4.5 or more',
                  all(v is not None and v >= 4.5 for v in got.values()), got)
    finally:
        if b:
            b.close()
        if gone:
            gone.stop()


def no_scroll(b, d):
    forget(b, d)
    load(b, d)
    b.resize(390, 1000)
    for mode in ('light', 'dark'):
        b.page(f"__core.scheme.set('{mode}')")
        time.sleep(0.3)
        got = b.page("({ scroll: document.documentElement.scrollWidth, inner: innerWidth,"
                     " body: document.body.scrollWidth })")
        check(f'no_sideways_scroll_at_390: {mode} paper fits the width',
              got['scroll'] <= got['inner'] and got['body'] <= got['inner'], got)
    b.resize(1280, 900)


DEVICE = 'Gungnir 2'
POSTS = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push([String(a[0]), String((a[1] || {}).body || '')]);
    return f(...a); }; return 1; })()"""
# the needle's centre as a fraction of the ruler, and the numbers beside it
NEEDLE = """(() => { const n = document.getElementById('p-needle').getBoundingClientRect();
  const r = document.getElementById('p-ruler').getBoundingClientRect();
  return { at: (n.left + n.width / 2 - r.left) / r.width, left: r.left, width: r.width,
    top: n.top + n.height / 2, cx: n.left + n.width / 2, w: r.width,
    text: document.getElementById('p-volume-text').textContent,
    now: document.getElementById('p-needle').getAttribute('aria-valuenow') }; })()"""


def volume_posts(b):
    """The volume writes this page has sent so far: the raw each carried."""
    got = b.page('window.__posts')
    return [json.loads(body)['raw'] for url, body in got if url.endswith('/volume') and body]


def volume(d):
    return d.model(DEVICE)['volume']


def settled(d, raw, secs=10.0):
    return poll(lambda: volume(d)['raw'] == raw, secs)


def needle(b):
    return b.page(NEEDLE)


def press(b, x, y):
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y,
                                            'button': 'left', 'clickCount': 1, 'buttons': 1})


def move(b, x, y):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y, 'button': 'left',
                                        'buttons': 1})


def release(b, x, y):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y,
                                        'button': 'left', 'clickCount': 1})


def drag(b, x0, y, x1):
    """Press at (x0, y), move to x1 in steps of about 8 px with the button down, release."""
    n = max(1, int(abs(x1 - x0) // 8))
    if browser == 'chromium':
        press(b, x0, y)
        for i in range(1, n + 1):
            move(b, x0 + (x1 - x0) * i / n, y)
        release(b, x1, y)
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x0),
                 'y': int(y)}, {'type': 'pointerDown', 'button': 0}]
        for i in range(1, n + 1):
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport',
                         'x': int(x0 + (x1 - x0) * i / n), 'y': int(y)})
        acts.append({'type': 'pointerUp', 'button': 0})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})
    time.sleep(0.3)


def rounded(model, fraction):
    return min(model['hi'], max(model['lo'], round(model['lo'] + fraction * (model['hi'] - model['lo']))))


def the_volume(b, d):
    forget(b, d)
    load(b, d)
    b.page(POSTS)
    m = volume(d)
    lo, hi = m['lo'], m['hi']
    for raw in (lo + 10, (lo + hi) // 2, hi - 10):
        d.post(DEVICE, 'volume', {'raw': raw})
        ok = settled(d, raw, 20.0)
        got = poll(lambda: abs(needle(b)['at'] * 1000 - (raw - lo) / (hi - lo) * 1000) < 1.5
                   * 1000 / needle(b)['w'], 10.0)
        model = volume(d)
        got = needle(b)
        check(f'the_needle_sits_at_its_value: raw {raw} puts the needle at its fraction and the '
              'level is written beside it',
              ok and abs(got['at'] - (raw - lo) / (hi - lo)) * got['w'] <= 1.5
              and got['text'] == model['text'], [raw, got, model['text']])
    check('  no page error', errors(b) == [], errors(b)[:3])

    # a drag along the ruler writes the steps it passes, and settles at the one it ends on
    d.post(DEVICE, 'volume', {'raw': lo + 20})
    settled(d, lo + 20, 10.0)
    time.sleep(0.5)
    got = needle(b)
    b.page('window.__posts.length = 0')
    drag(b, got['cx'], got['top'], got['left'] + 0.6 * got['width'])
    want = rounded(m, 0.6)
    check('dragging_the_needle_writes_its_steps: the model settles at the step of 60 % of the ruler',
          settled(d, want, 10.0), [want, volume(d)['raw'], volume_posts(b)[-5:]])
    check('  the last write is that step and the needle stands there',
          volume_posts(b)[-1:] == [want]
          and abs(needle(b)['at'] - (want - lo) / (hi - lo)) * got['width'] <= 1.5,
          [volume_posts(b)[-3:], needle(b)])
    # a press on the ruler away from the needle moves it there
    got = needle(b)
    b.page('window.__posts.length = 0')
    drag(b, got['left'] + 0.3 * got['width'], got['top'] + 20, got['left'] + 0.3 * got['width'])
    want = rounded(m, 0.3)
    check('  a press on the ruler outside the needle moves it there',
          settled(d, want, 10.0), [want, volume(d)['raw']])

    # the written words
    d.post(DEVICE, 'volume', {'raw': lo + 30})
    settled(d, lo + 30, 10.0)
    time.sleep(0.4)
    click_el(b, '[data-act=volume-up]')
    check('the_written_volume_words_write: `louder +` raises by one step',
          settled(d, lo + 31, 10.0), volume(d)['raw'])
    click_el(b, '[data-act=volume-down]')
    check('  `\u2212 quieter` lowers by one step', settled(d, lo + 30, 10.0), volume(d)['raw'])
    words = b.page("[...document.querySelectorAll('.p-written .p-w')].map((w) => w.textContent)")
    check('  the three words are written', words == ['\u2212 quieter', 'mute', 'louder +'], words)
    click_el(b, '[data-act=mute]')
    check('  `mute` mutes and the word reads `unmute`; the value says `muted`',
          poll(lambda: volume(d)['muted'] is True)
          and poll(lambda: b.page("document.querySelector('[data-act=mute]').textContent")
                   == 'unmute')
          and poll(lambda: needle(b)['text'] == 'muted'),
          [volume(d)['muted'], needle(b)['text']])
    click_el(b, '[data-act=mute]')
    check('  and again unmutes',
          poll(lambda: volume(d)['muted'] is False)
          and poll(lambda: b.page("document.querySelector('[data-act=mute]').textContent")
                   == 'mute'), volume(d)['muted'])

    # the keys
    settled(d, lo + 30, 10.0)
    b.page("document.getElementById('p-needle').focus()")
    b.page('window.__posts.length = 0')
    b.press('ArrowUp')
    b.press('ArrowUp')
    check('the_needle_takes_the_arrow_keys: two ArrowUp raise two steps',
          settled(d, lo + 32, 10.0), volume(d)['raw'])
    b.press('ArrowDown')
    check('  ArrowDown lowers one', settled(d, lo + 31, 10.0), volume(d)['raw'])
    b.press('PageUp')
    check('  PageUp raises ten', settled(d, lo + 41, 10.0), volume(d)['raw'])
    b.press('PageDown')
    check('  PageDown lowers ten', settled(d, lo + 31, 10.0), volume(d)['raw'])
    count = len(volume_posts(b))
    b.press('End')
    time.sleep(0.4)
    check('  End posts nothing and the needle keeps its step',
          len(volume_posts(b)) == count and volume(d)['raw'] == lo + 31,
          [count, volume_posts(b)[-2:]])
    b.press('Home')
    check('  Home goes to the bottom', settled(d, lo, 10.0), volume(d)['raw'])
    aria = b.page("(() => { const n = document.getElementById('p-needle');"
                  " return [n.getAttribute('role'), n.getAttribute('aria-label'),"
                  " n.getAttribute('aria-valuemin'), n.getAttribute('aria-valuemax'),"
                  " n.tabIndex]; })()")
    check('  the needle is a slider in the tab order', aria == ['slider', 'Volume', str(lo), str(hi), 0],
          aria)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_cap():
    """A drag past a cap held in the config, on a daemon and in a browser of its own (the page's
    stream ends with the daemon)."""
    capped = common.Daemon('gungnir2-v2', web='auth = "none"\nmax_volume_db = -30.0', host='fake')
    b = make()
    try:
        forget(b, capped)
        load(b, capped)
        b.page(POSTS)
        m = volume(capped)
        cap = m['cap']
        check('dragging_past_the_cap_writes_the_cap: the device reports a cap', cap is not None, m)
        capped.post(DEVICE, 'volume', {'raw': m['lo'] + 10})
        settled(capped, m['lo'] + 10, 10.0)
        time.sleep(0.5)
        got = needle(b)
        b.page('window.__posts.length = 0')
        drag(b, got['cx'], got['top'], got['left'] + got['width'] + 40)
        posts = volume_posts(b)
        check('  no write goes above the cap', posts != [] and max(posts) <= cap, [cap, posts])
        check('  the model settles at the cap', settled(capped, cap, 10.0), volume(capped)['raw'])
        got = needle(b)
        check('  and the needle stops at the cap\'s place',
              abs(got['at'] - (cap - m['lo']) / (m['hi'] - m['lo'])) * got['width'] <= 1.5, got)
        zone = b.page("(() => { const z = document.querySelector('.p-zone.cap');"
                      " const r = document.getElementById('p-ruler').getBoundingClientRect();"
                      " const q = z.getBoundingClientRect();"
                      " return (q.left - r.left) / r.width; })()")
        check('  the cap is marked on the ruler where the needle stands',
              abs(zone - got['at']) * got['width'] <= 2, [zone, got['at']])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        capped.stop()


def the_hold(b, d):
    """A change from elsewhere during a needle drag: the needle stays under the pointer, and after
    the release shows the newest reading."""
    forget(b, d)
    load(b, d)
    m = volume(d)
    lo, hi = m['lo'], m['hi']
    d.post(DEVICE, 'volume', {'raw': lo + 20})
    settled(d, lo + 20, 10.0)
    time.sleep(0.5)
    got = needle(b)
    held = rounded(m, 0.3)
    other = lo + 50
    x = got['left'] + 0.3 * got['width']
    press(b, got['cx'], got['top'])
    for i in range(1, 6):
        move(b, got['cx'] + (x - got['cx']) * i / 5, got['top'])
    time.sleep(0.5)
    d.post(DEVICE, 'volume', {'raw': other})
    check('a_reading_from_elsewhere_during_a_needle_drag_lands_after_it: the device took the other '
          'reading', settled(d, other, 10.0), volume(d)['raw'])
    time.sleep(0.6)
    mid = needle(b)
    check('  while the hand is down the needle stays under the pointer',
          abs(mid['at'] - (held - lo) / (hi - lo)) * mid['width'] <= 1.5, [mid, held])
    release(b, x, got['top'])
    after = poll(lambda: abs(needle(b)['at'] - (other - lo) / (hi - lo)) * needle(b)['w'] <= 1.5, 3.5)
    check('  once released, within 3 s, it shows the newest reading', after,
          [needle(b), other, volume(d)['raw']])
    check('  no page error', errors(b) == [], errors(b)[:3])


# ---- the tone ----

POINTS = """(() => { document.getElementById('p-graph').scrollIntoView({ block: 'center' });
  const g = document.getElementById('p-graph').getBoundingClientRect();
  return { graph: [g.left, g.top, g.width, g.height],
    points: [...document.querySelectorAll('#p-graph .pt')].map((p) => {
      const r = p.getBoundingClientRect();
      return { band: +p.dataset.band, cx: r.left + r.width / 2, cy: r.top + r.height / 2,
               x: +p.dataset.x, y: +p.dataset.y }; }) }; })()"""


def tone(d):
    return d.model(DEVICE)['tone']


def bands(d):
    return tone(d)['readings']['bands']


def points(b):
    b.page(POINTS)
    time.sleep(0.2)
    return b.page(POINTS)


def drag_xy(b, x0, y0, x1, y1, hold=False):
    """Press at (x0, y0), move to (x1, y1) in steps of about 8 px, release (unless `hold`: the
    button stays down, Chromium only)."""
    n = max(1, int(math.hypot(x1 - x0, y1 - y0) // 8))
    path = [(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(1, n + 1)]
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x0, 'y': y0})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x0, 'y': y0,
                                            'button': 'left', 'clickCount': 1, 'buttons': 1})
        for x, y in path:
            move(b, x, y)
        if not hold:
            release(b, x1, y1)
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x0),
                 'y': int(y0)}, {'type': 'pointerDown', 'button': 0}]
        for x, y in path:
            acts.append({'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x),
                         'y': int(y)})
        acts.append({'type': 'pointerUp', 'button': 0})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})
    time.sleep(0.3)


def nearest_step(table, value, log=False):
    """The index of the table's number nearest `value` (by ratio for a frequency), the lowest on a tie."""
    best, at = None, 0
    for i, row in enumerate(table):
        v = row[0]
        if not isinstance(v, (int, float)):
            continue
        dist = abs(math.log(v / value)) if log else abs(v - value)
        if best is None or dist < best:
            best, at = dist, i
    return at


def the_point_drag(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.5)
    b.page(POSTS)
    steps = d.model(DEVICE)['tone_steps']
    got = points(b)
    gx, gy, gw, gh = got['graph']
    two = next(p for p in got['points'] if p['band'] == 2)
    x1, y1 = two['cx'] + 80, two['cy'] - 30
    drag_xy(b, two['cx'], two['cy'], x1, y1)
    hz = 20 * math.exp((x1 - gx) / gw * math.log(1000))
    db = 13 - ((y1 - gy) / gh * 260) / 10
    want_f = nearest_step(steps['bands'][1]['f'], hz, log=True)
    want_g = nearest_step(steps['gain'], db)
    ok = poll(lambda: bands(d)[1][:2] == [want_f, want_g], 8.0)
    check('a_band_point_drag_writes_the_steps: band 2 lands on the steps nearest the release point',
          ok, [bands(d)[1], [want_f, want_g]])
    after = points(b)
    two = next(p for p in after['points'] if p['band'] == 2)
    # the point stands where the written steps say (under the release point, but for the band's
    # own reach: its frequency stops at the last step)
    at_x = gx + 1000 * math.log(steps['bands'][1]['f'][want_f][0] / 20) / math.log(1000) / 1000 * gw
    check('  the point stands where the written steps put it', abs(two['cx'] - at_x) <= 1.5
          and abs(two['cy'] - y1) <= gh / 128 + 2, [two, at_x, y1])
    cell = b.page("document.querySelector('#p-bands tr[data-band=\"2\"] .p-val').textContent")
    check('  the table says the frequency the drag wrote',
          cell == steps['bands'][1]['f'][want_f][1], [cell, steps['bands'][1]['f'][want_f]])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_table(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.5)
    before = bands(d)[0]
    click_el(b, '#p-b1gp')
    check('the_band_table_steps_write: band 1 gain `+` raises the raw gain by one',
          poll(lambda: bands(d)[0][1] == before[1] + 1, 8.0), [before, bands(d)[0]])
    cell = """document.querySelector('#p-bands tr[data-band="1"] td:nth-child(3) .p-val').textContent"""
    check('  and the table shows the new value',
          poll(lambda: b.page(cell) == tone(d)['bands']['rows'][0]['g']),
          [b.page(cell), tone(d)['bands']['rows'][0]])
    before = bands(d)[0]
    click_el(b, '#p-b1qm')
    check('  band 1 Q `\u2212` lowers the raw Q by one',
          poll(lambda: bands(d)[0][2] == before[2] - 1, 8.0), [before, bands(d)[0]])
    before = bands(d)[0]
    click_el(b, '#p-b1fp')
    check('  band 1 frequency `+` raises the raw frequency by one',
          poll(lambda: bands(d)[0][0] == before[0] + 1, 8.0), [before, bands(d)[0]])
    # the arrow keys on a focused point
    before = bands(d)[1]
    b.page("document.getElementById('p-pt-2').focus()")
    b.press('ArrowLeft')
    check('  ArrowLeft on band 2\'s point lowers its frequency by one',
          poll(lambda: bands(d)[1][0] == before[0] - 1, 8.0), [before, bands(d)[1]])
    b.press('ArrowUp')
    check('  ArrowUp raises its gain by one', poll(lambda: bands(d)[1][1] == before[1] + 1, 8.0),
          [before, bands(d)[1]])
    check('  and the point keeps the focus',
          poll(lambda: b.page('document.activeElement.id') == 'p-pt-2'),
          b.page('document.activeElement.id'))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_lists(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    d.post(DEVICE, 'presets/recall', {'name': 'Flat'})
    time.sleep(0.6)
    b.page(POSTS)
    modes = [m['name'] for m in tone(d)['modes']]
    click_el(b, '#p-mode')
    got = poll(lambda: b.page("(() => { const l = document.querySelector('.plist[role=listbox]');"
                              " return l && [[...l.querySelectorAll('[role=option]')].map((o) => o.dataset.value),"
                              " [...l.querySelectorAll('[aria-selected=true]')].map((o) => o.dataset.value),"
                              " document.activeElement.dataset.value, l.getAttribute('aria-label')]; })()"))
    check('the_mode_list_chooses_and_closes: the mode word opens a listbox of the modes with the '
          'current one selected and focused', got and got[0] == modes and got[1] == ['eq']
          and got[2] == 'eq', got)
    b.press('ArrowDown')
    b.press('Enter')
    check('  ArrowDown then Enter writes the next mode',
          poll(lambda: tone(d)['mode'] == modes[modes.index('eq') + 1], 8.0), tone(d)['mode'])
    check('  the list closes', poll(lambda: b.page("!document.querySelector('.plist')")),
          'still open')
    check('  and the opener is focused again',
          poll(lambda: b.page('document.activeElement.id') == 'p-mode'),
          b.page('document.activeElement.id'))
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    poll(lambda: b.page("document.getElementById('p-mode').textContent") == 'EQ', 5.0)
    b.page('window.__posts.length = 0')
    click_el(b, '#p-mode')
    poll(lambda: b.page("!!document.querySelector('.plist')"))
    b.press('Escape')
    time.sleep(0.4)
    posts = b.page("window.__posts.filter((p) => /eq\\/mode/.test(p[0])).length")
    check('  Escape closes a second opening and writes nothing',
          b.page("!document.querySelector('.plist')") and posts == 0
          and tone(d)['mode'] == 'eq', [posts, tone(d)['mode']])
    check('  the focus returns to the opener', b.page('document.activeElement.id') == 'p-mode',
          b.page('document.activeElement.id'))
    # a press outside closes it; the word itself toggles
    click_el(b, '#p-mode')
    poll(lambda: b.page("!!document.querySelector('.plist')"))
    click_el(b, '#p-title h1')
    check('  a press outside closes it', poll(lambda: b.page("!document.querySelector('.plist')")), 'open')
    click_el(b, '#p-mode')
    poll(lambda: b.page("!!document.querySelector('.plist')"))
    click_el(b, '#p-mode')
    check('  the word toggles it shut', poll(lambda: b.page("!document.querySelector('.plist')")), 'open')

    # a preset
    click_el(b, '#p-preset')
    names = poll(lambda: b.page("[...document.querySelectorAll('.plist .p-opt')].map((o) => o.textContent)"))
    check('the_preset_list_applies_a_preset: the preset word lists the presets', 'Warm' in (names or []),
          names)
    click_el(b, '.plist .p-opt[data-value=Warm]')
    check('  choosing Warm applies it', poll(lambda: [r['on'] for r in d.model(DEVICE)['presets']['rows']
                                                       if r['name'] == 'Warm'] == [True], 8.0),
          d.model(DEVICE)['presets']['rows'])
    check('  and the heading names it', poll(lambda: b.page("document.getElementById('p-preset').textContent")
                                            == 'Warm'), b.page("document.getElementById('p-preset').textContent"))

    # the loudness strength
    d.post(DEVICE, 'eq/mode', {'mode': 'loudness'})
    d.post(DEVICE, 'eq/loudness', {'raw': 40})
    poll(lambda: b.page("!!document.getElementById('p-loudp')"), 8.0)
    click_el(b, '#p-loudp')
    check('the_loudness_strength_steps: `+` raises the strength by one',
          poll(lambda: tone(d)['strength'] == '41', 8.0), tone(d)['strength'])
    poll(lambda: b.page("!!document.getElementById('p-loudm')"))
    click_el(b, '#p-loudm')
    check('  `\u2212` lowers it by one', poll(lambda: tone(d)['strength'] == '40', 8.0),
          tone(d)['strength'])
    check('  the strength is written beside its steppers',
          poll(lambda: '40' in b.page("document.querySelector('.p-strength').textContent")),
          b.page("document.querySelector('.p-strength').textContent"))
    check('  no page error', errors(b) == [], errors(b)[:3])
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})


def tone_hold(b, d):
    """A change from elsewhere during a point drag: the held point stays, and after the release the
    newest reading is drawn."""
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.5)
    got = points(b)
    gx, gy, gw, gh = got['graph']
    two = next(p for p in got['points'] if p['band'] == 2)
    four = next(p for p in got['points'] if p['band'] == 4)
    x1, y1 = two['cx'] + 30, two['cy'] + 20
    drag_xy(b, two['cx'], two['cy'], x1, y1, hold=True)
    mid = points(b)
    held = next(p for p in mid['points'] if p['band'] == 2)
    raw_g = bands(d)[3][1]
    status, _ = d.post(DEVICE, 'eq/bands/4', {'param': 'gain', 'raw': min(127, raw_g + 25)})
    time.sleep(1.0)
    now = points(b)
    still = next(p for p in now['points'] if p['band'] == 4)
    same = next(p for p in now['points'] if p['band'] == 2)
    check('a_tone_update_waits_for_the_point_drag: the device took the other write',
          status == 200 and bands(d)[3][1] == min(127, raw_g + 25), [status, bands(d)[3]])
    check('  while the hand is down band 4\'s point stays where it was and the held point stays',
          abs(still['y'] - four['y']) < 1e-4 and abs(same['cy'] - held['cy']) < 1.0,
          [four, still, held, same])
    release(b, x1, y1)
    want = None
    def moved():
        nonlocal want
        want = next(p for p in points(b)['points'] if p['band'] == 4)
        return abs(want['y'] - four['y']) > 0.01
    check('  once released, within 3 s, band 4\'s point shows the newest reading',
          poll(moved, 3.5), [four, want])
    check('  no page error', errors(b) == [], errors(b)[:3])


def one_listener(b, d):
    forget(b, d)
    load(b, d)
    b.page(POSTS)
    m = volume(d)
    d.post(DEVICE, 'volume', {'raw': m['lo'] + 30})
    settled(d, m['lo'] + 30, 10.0)
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    poll(lambda: b.page("document.documentElement.dataset.theme === 'ember' "
                        "&& !!document.querySelector('#screen')"))
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=paper]')
    poll(lambda: b.page("!!document.getElementById('p-needle')"))
    time.sleep(0.5)
    b.page('window.__posts.length = 0')
    click_el(b, '[data-act=volume-up]')
    time.sleep(1.0)
    posts = volume_posts(b)
    check('switching_to_ember_and_back_leaves_one_listener: one tap sends one volume write',
          posts == [m['lo'] + 31], posts)
    check('  no page error', errors(b) == [], errors(b)[:3])

def control(d, key):
    return next(c for c in d.model(DEVICE)['controls'] if c['key'] == key)


def chosen(c):
    return next((o['label'] for o in c['options'] if o['on']), None)


def posts_to(b, tail):
    """The bodies this page posted to a route ending in `tail`."""
    return [body for url, body in b.page('window.__posts') if url.endswith(tail)]


def set_text(b, selector, text):
    """Put `text` in the input (what a person typed)."""
    b.page(f"""(() => {{ const i = document.querySelector({json.dumps(selector)});
      i.value = {json.dumps(text)}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); return 1; }})()""")


def the_controls(b, d):
    forget(b, d)
    load(b, d)
    options = [o['label'] for o in control(d, 'input')['options']]
    want = next(o for o in options if o != chosen(control(d, 'input')) and o == 'Optical')
    click_el(b, f'[data-act=choice][data-key=input][data-value={want}]')
    check('a_choice_word_writes_its_option: `Optical` is written to the input',
          poll(lambda: chosen(control(d, 'input')) == want, 8.0), chosen(control(d, 'input')))
    now = poll(lambda: b.page("[...document.querySelectorAll('[data-act=choice][data-key=input].on')]"
                              ".map((w) => w.dataset.value)") == [want], 5.0)
    check('  and the pencil underline moves to it (the other values are dotted)',
          now and b.page("getComputedStyle(document.querySelector("
                         "'[data-act=choice][data-key=input].on')).textDecorationThickness") == '2px'
          and b.page("getComputedStyle(document.querySelector("
                     "'[data-act=choice][data-key=input]:not(.on)')).textDecorationStyle") == 'dotted',
          b.page("[...document.querySelectorAll('[data-act=choice][data-key=input]')]"
                 ".map((w) => [w.className, getComputedStyle(w).textDecorationLine])"))
    click_el(b, '[data-act=choice][data-key=input][data-value="USB 1"]')
    poll(lambda: chosen(control(d, 'input')) == 'USB 1', 8.0)

    # a switch
    before = control(d, 'phase')['on']
    word = "document.querySelector('[data-act=toggle][data-key=phase]').textContent"
    check('a_toggle_word_flips: the word reads the phase', b.page(word) == ('on' if before else 'off'),
          b.page(word))
    click_el(b, '[data-act=toggle][data-key=phase]')
    check('  a tap flips the phase on the device and the word follows',
          poll(lambda: control(d, 'phase')['on'] is (not before), 8.0)
          and poll(lambda: b.page(word) == ('off' if before else 'on')), [control(d, 'phase'), b.page(word)])
    click_el(b, '[data-act=toggle][data-key=phase]')
    poll(lambda: control(d, 'phase')['on'] is before, 8.0)
    check('  no page error', errors(b) == [], errors(b)[:3])


SCALE = """(() => { const s = document.querySelector('.p-scale[data-key=balance]');
  const r = s.getBoundingClientRect(); const m = s.querySelector('.p-mark').getBoundingClientRect();
  return { left: r.left, width: r.width, top: r.top + r.height / 2, mark: m.left + m.width / 2,
    now: s.getAttribute('aria-valuenow'), text: s.getAttribute('aria-valuetext'),
    at: s.querySelector('.p-mark').dataset.at,
    word: document.querySelector('[data-act=centre][data-key=balance]').textContent }; })()"""


def the_balance(b, d):
    forget(b, d)
    load(b, d)
    c = control(d, 'balance')
    lo, hi, centre = c['lo'], c['hi'], c['centre']
    b.page(POSTS)
    got = b.page(SCALE)
    check('the_balance_scale_drags_and_centres: the scale is a slider with the mark at the reading',
          got['now'] == str(c['raw'])
          and abs((got['mark'] - got['left']) / got['width'] - (c['raw'] - lo) / (hi - lo))
          * got['width'] <= 1.5, [got, c['raw']])
    drag(b, got['mark'], got['top'], got['left'] + 0.25 * got['width'])
    want = min(hi, max(lo, math.floor(lo + 0.25 * (hi - lo) + 0.5)))
    check('  dragging the mark to 25 % writes the step there',
          poll(lambda: abs(control(d, 'balance')['raw'] - want) <= 1, 8.0), [want, control(d, 'balance')['raw']])
    raws = [json.loads(x)['raw'] for x in posts_to(b, '/controls/balance') if x]
    check('  the writes are raw steps and the last one is that step (within one, a browser rounds a pixel)',
          raws and abs(raws[-1] - want) <= 1 and all(lo <= r <= hi for r in raws), raws[-5:])
    time.sleep(0.6)
    got = b.page(SCALE)
    check('  the mark and the words stand at the step the device holds',
          got['now'] == str(control(d, 'balance')['raw']) and got['word'] == control(d, 'balance')['text'],
          [got, control(d, 'balance')])
    # the keys
    b.page("document.querySelector('.p-scale[data-key=balance]').focus()")
    before = control(d, 'balance')['raw']
    b.press('ArrowRight')
    check('  ArrowRight steps it up by one', poll(lambda: control(d, 'balance')['raw'] == before + 1, 8.0),
          [before, control(d, 'balance')['raw']])
    # the value written beside it writes the centre
    click_el(b, '[data-act=centre][data-key=balance]')
    check('  a tap on the value writes the centre',
          poll(lambda: control(d, 'balance')['raw'] == centre, 8.0), [centre, control(d, 'balance')['raw']])
    check('  and the mark sits at the centre tick',
          poll(lambda: abs(float(b.page(SCALE)['at']) - (centre - lo) / (hi - lo)) < 1e-3),
          b.page(SCALE))
    check('  no page error', errors(b) == [], errors(b)[:3])


PRESET_NAMES = "[...document.querySelectorAll('#p-presets [data-act=preset]')].map((w) => w.dataset.name)"
OPTIONS = "[...document.querySelectorAll('.plist .p-opt')].map((o) => o.textContent)"


def preset_rows(d):
    return [r['name'] for r in d.model(DEVICE)['presets']['rows']]


def the_presets(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.4)
    b.page(POSTS)
    click_el(b, '#p-presets [data-act=preset][data-name=Bass]')
    check('presets_apply_save_rename_and_delete_with_papers_questions: a tap applies a preset',
          poll(lambda: [r['on'] for r in d.model(DEVICE)['presets']['rows'] if r['name'] == 'Bass'] == [True], 8.0),
          d.model(DEVICE)['presets']['rows'])
    check('  the status line says it is on, the name in italics',
          poll(lambda: b.page("document.querySelector('#p-presets .p-status i')?.textContent") == 'Bass'),
          b.page("document.querySelector('#p-presets .p-status').textContent"))
    # save as
    click_el(b, '[data-act=preset-save]')
    shape = poll(lambda: b.page("""(() => { const i = document.querySelector('#p-presets .p-askbox input');
      return i && { max: i.maxLength, req: i.required, label: i.getAttribute('aria-label'),
        answers: [...document.querySelectorAll('#p-presets .p-askbox [data-answer]')]
          .map((w) => w.dataset.answer), focus: document.activeElement === i }; })()"""))
    check('  `save as…` opens a ruled input (40 characters, required, named) with save and cancel',
          shape == {'max': 40, 'req': True, 'label': 'Preset name', 'answers': ['save', 'cancel'],
                    'focus': True}, shape)
    set_text(b, '#p-presets .p-askbox input', 'HD 800 S')
    click_el(b, '#p-presets .p-askbox [data-answer=save]')
    check('  save writes the name; the form closes and the name is a word of its own',
          poll(lambda: 'HD 800 S' in preset_rows(d), 8.0)
          and poll(lambda: b.page("!document.querySelector('#p-presets .p-askbox')"))
          and poll(lambda: 'HD 800 S' in b.page(PRESET_NAMES)), [preset_rows(d), b.page(PRESET_NAMES)])
    # rename
    click_el(b, '[data-act=preset-edit]')
    opts = poll(lambda: b.page(OPTIONS))
    check('  `rename or delete…` lists the presets of one\'s own to rename or delete',
          opts == ['Rename “HD 800 S”', 'Delete “HD 800 S”'], opts)
    click_el(b, '.plist .p-opt[data-value="rename:HD 800 S"]')
    held = poll(lambda: b.page("document.querySelector('#p-presets .p-askbox input')?.value"))
    check('  rename asks for the new name, the old one in the line', held == 'HD 800 S', held)
    check('  its input is called New name and its answers are rename and cancel',
          b.page("document.querySelector('#p-presets .p-askbox input').getAttribute('aria-label')") == 'New name'
          and b.page("[...document.querySelectorAll('#p-presets .p-askbox [data-answer]')]"
                     ".map((w) => w.dataset.answer)") == ['rename', 'cancel'])
    set_text(b, '#p-presets .p-askbox input', 'HD 800 S x')
    click_el(b, '#p-presets .p-askbox [data-answer=rename]')
    check('  the rename is written', poll(lambda: 'HD 800 S x' in preset_rows(d) and 'HD 800 S' not in preset_rows(d),
                                         8.0), preset_rows(d))
    # delete, kept and then answered
    poll(lambda: 'HD 800 S x' in b.page(PRESET_NAMES))
    click_el(b, '[data-act=preset-edit]')
    poll(lambda: b.page(OPTIONS))
    click_el(b, '.plist .p-opt[data-value="delete:HD 800 S x"]')
    words = poll(lambda: b.page("document.querySelector('#p-presets .p-question')?.textContent"))
    check('  delete asks in Paper\'s words',
          words == 'Delete “HD 800 S x”? The bands on the device stay as they are.', words)
    check('  the answers are delete and keep, and keep has the focus',
          b.page("[...document.querySelectorAll('#p-presets .p-askbox [data-answer]')]"
                 ".map((w) => w.dataset.answer)") == ['delete', 'keep']
          and b.page("document.activeElement.dataset.answer") == 'keep')
    b.page('window.__posts.length = 0')
    click_el(b, '#p-presets .p-askbox [data-answer=keep]')
    time.sleep(0.5)
    check('  keep closes it and posts nothing',
          b.page("!document.querySelector('#p-presets .p-askbox')") and posts_to(b, 'presets/delete') == []
          and 'HD 800 S x' in preset_rows(d), [posts_to(b, 'presets/delete'), preset_rows(d)])
    click_el(b, '[data-act=preset-edit]')
    poll(lambda: b.page(OPTIONS))
    click_el(b, '.plist .p-opt[data-value="delete:HD 800 S x"]')
    poll(lambda: b.page("!!document.querySelector('#p-presets .p-question')"))
    click_el(b, '#p-presets .p-askbox [data-answer=delete]')
    check('  delete removes it', poll(lambda: 'HD 800 S x' not in preset_rows(d), 8.0)
          and poll(lambda: 'HD 800 S x' not in b.page(PRESET_NAMES)), preset_rows(d))
    check('  with no preset of one\'s own left, `rename or delete…` is not written',
          poll(lambda: b.page("!document.querySelector('[data-act=preset-edit]')")))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_refusal(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'presets', {'name': 'Taken A'})
    d.post(DEVICE, 'presets', {'name': 'Taken B'})
    poll(lambda: b.page("!!document.querySelector('[data-act=preset-edit]')"), 8.0)
    click_el(b, '[data-act=preset-edit]')
    poll(lambda: b.page(OPTIONS))
    click_el(b, '.plist .p-opt[data-value="rename:Taken A"]')
    poll(lambda: b.page("!!document.querySelector('#p-presets .p-askbox input')"))
    set_text(b, '#p-presets .p-askbox input', 'Taken B')
    click_el(b, '#p-presets .p-askbox [data-answer=rename]')
    toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"),
                 8.0)
    state = poll(lambda: b.page("""(() => { const i = document.querySelector('#p-presets .p-askbox input');
      return i && !i.disabled && { value: i.value, open: true }; })()"""), 5.0)
    check('a_refused_form_stays_open_with_its_text: the core toasts why', bool(toast), toast)
    # Chromium logs the refused request's status as a console line; Firefox does not
    check('  and the daemon answered the rename 422, the only console line it left (Chromium)',
          expect_error(b, 'presets/rename') or browser == 'firefox', b.errors[:3])
    check('  the form is still open, enabled, holding what was typed',
          state == {'value': 'Taken B', 'open': True}, state)
    check('  and nothing was renamed', 'Taken A' in preset_rows(d) and preset_rows(d).count('Taken B') == 1,
          preset_rows(d))
    click_el(b, '#p-presets .p-askbox [data-answer=cancel]')
    check('  cancel closes it', poll(lambda: b.page("!document.querySelector('#p-presets .p-askbox')")))
    for name in ('Taken A', 'Taken B'):
        d.post(DEVICE, 'presets/delete', {'name': name})
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_survival(b, d):
    """A change from elsewhere while a list is open: the list stays, with its focus."""
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.4)
    click_el(b, '#p-mode')
    poll(lambda: b.page("!!document.querySelector('.plist')"))
    phase = control(d, 'phase')['on']
    d.write(DEVICE, 'phase', not phase)
    raw_g = bands(d)[0][1]
    d.post(DEVICE, 'eq/bands/1', {'param': 'gain', 'raw': min(127, raw_g + 15)})
    poll(lambda: control(d, 'phase')['on'] is (not phase), 8.0)
    time.sleep(1.2)
    state = b.page("""({ open: !!document.querySelector('.plist'),
      focus: !!document.activeElement.closest('.plist'),
      phase: document.querySelector('[data-act=toggle][data-key=phase]').textContent,
      gain: document.querySelector('#p-bands tr[data-band="1"] td:nth-child(3) .p-val').textContent })""")
    old_gain = state['gain']
    check('an_open_paper_list_survives_a_part_update: the list stays open and keeps its focus',
          state['open'] and state['focus'], state)
    check('  the controls (another section) show the new phase meanwhile',
          state['phase'] == ('off' if phase else 'on'), state)
    b.press('Escape')
    check('  Escape closes it and the focus is back on the mode word',
          poll(lambda: b.page("!document.querySelector('.plist') "
                              "&& document.activeElement.id === 'p-mode'")))
    now = poll(lambda: b.page("document.querySelector('#p-bands tr[data-band=\"1\"] td:nth-child(3)"
                              " .p-val').textContent") != old_gain, 4.0)
    check('  the tone section then shows the newest reading, and the controls the new phase',
          now and b.page("document.querySelector('[data-act=toggle][data-key=phase]').textContent")
          == ('off' if phase else 'on'), [old_gain])
    d.write(DEVICE, 'phase', phase)
    d.post(DEVICE, 'eq/bands/1', {'param': 'gain', 'raw': raw_g})
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_touch(d):
    """On a phone: a touch on a point's own area drags it; a touch beside it is the page's scroll."""
    b = make(size=(390, 844), touch=True)
    try:
        touch_checks(b, d)
    finally:
        b.close()


def touch_checks(b, d):
    forget(b, d)
    load(b, d)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    time.sleep(0.6)
    b.page("document.getElementById('p-graph').scrollIntoView({ block: 'center' })")
    time.sleep(0.4)
    got = points(b)
    two = next(p for p in got['points'] if p['band'] == 2)
    g0 = bands(d)[1][1]

    def swipe(x, y0, y1):
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': [{'x': x, 'y': y0}]})
        n = max(1, int(abs(y1 - y0) // 6))
        for i in range(1, n + 1):
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove',
                   'touchPoints': [{'x': x, 'y': y0 + (y1 - y0) * i / n}]})
        time.sleep(0.15)
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
        time.sleep(0.5)

    # 23 px from the centre: inside the reach a touch had, outside the point's own 44 px area, where
    # the browser's own touch would be the page's scroll. The press is sent from the page (the
    # browser's touch adjustment would otherwise snap it to the point), at the element under it.
    b.page(f"""(() => {{ const at = (type, y) => {{ const x = {two['cx'] - 23};
        const el = type === 'pointerdown' ? document.elementFromPoint(x, {two['cy']}) : document.getElementById('p-graph');
        el.dispatchEvent(new PointerEvent(type, {{ bubbles: true, cancelable: true, pointerId: 41,
          pointerType: 'touch', isPrimary: true, clientX: x, clientY: y }})); }};
      at('pointerdown', {two['cy']}); at('pointermove', {two['cy'] - 30});
      at('pointermove', {two['cy'] - 60}); at('pointerup', {two['cy'] - 60}); return 1; }})()""")
    time.sleep(0.8)
    check('a_touch_beside_a_point_scrolls_and_on_it_drags: a touch 23 px beside a point, outside its '
          'own area, grabs nothing and writes no band', bands(d)[1][1] == g0, [g0, bands(d)[1]])
    swipe(two['cx'] + 6, two['cy'], two['cy'] - 30)
    check('  a vertical touch on the point\'s own area drags its gain up',
          poll(lambda: bands(d)[1][1] > g0, 8.0), [g0, bands(d)[1][1]])
    check('  no page error', errors(b) == [], errors(b)[:3])
    d.post(DEVICE, 'eq/bands/2', {'param': 'gain', 'raw': g0})


LOKI = 'Loki Max'


def the_device_presets():
    loki = common.Daemon(profile='lokimax', name=LOKI)
    b = make()
    try:
        forget(b, loki)
        load(b, loki)
        b.page(POSTS)
        shape = b.page("""({ slots: [...document.querySelectorAll('#p-device-presets [data-act=slot]')]
          .map((w) => w.textContent), set: [...document.querySelectorAll('#p-device-presets [data-act=setting]')]
          .map((w) => w.textContent), labels: [...document.querySelectorAll('#p-device-presets .p-label')]
          .map((w) => w.textContent) })""")
        check('a_slot_save_asks_then_writes_with_confirm: the slots, the built-in settings and their headings',
              shape == {'slots': ['Preset 1', 'Preset 2', 'Preset 3'],
                        'set': ['Flat', 'Bass Boost', 'Treble Boost', 'Smiley', 'Loudness'],
                        'labels': ['On the device', 'Built in']}, shape)
        click_el(b, '[data-act=slot-save-list]')
        opts = poll(lambda: b.page(OPTIONS))
        check('  `save the bands to…` lists the slots', opts == ['Preset 1', 'Preset 2', 'Preset 3'], opts)
        click_el(b, '.plist .p-opt[data-value="2"]')
        words = poll(lambda: b.page("document.querySelector('#p-device-presets .p-question')?.textContent"))
        time.sleep(0.4)
        check('  choosing a slot asks, in Paper\'s words, and has posted nothing',
              words == 'Save the bands to Preset 2? What the device holds there is replaced.'
              and posts_to(b, 'device-presets/save') == [], [words, posts_to(b, 'device-presets/save')])
        check('  the answers are save and keep, keep has the focus',
              b.page("[...document.querySelectorAll('#p-device-presets .p-askbox [data-answer]')]"
                     ".map((w) => w.dataset.answer)") == ['save', 'keep']
              and b.page('document.activeElement.dataset.answer') == 'keep')
        click_el(b, '#p-device-presets .p-askbox [data-answer=keep]')
        time.sleep(0.5)
        check('  keep closes it and posts nothing',
              b.page("!document.querySelector('#p-device-presets .p-askbox')")
              and posts_to(b, 'device-presets/save') == [], posts_to(b, 'device-presets/save'))
        click_el(b, '[data-act=slot-save-list]')
        poll(lambda: b.page(OPTIONS))
        click_el(b, '.plist .p-opt[data-value="2"]')
        poll(lambda: b.page("!!document.querySelector('#p-device-presets .p-question')"))
        click_el(b, '#p-device-presets .p-askbox [data-answer=save]')
        sent = poll(lambda: posts_to(b, 'device-presets/save'), 8.0)
        check('  save posts the slot with confirm: true, and only once',
              [json.loads(x) for x in sent] == [{'slot': 2, 'confirm': True}], sent)
        check('  and the question closes', poll(lambda: b.page(
            "!document.querySelector('#p-device-presets .p-askbox')")))
        # a built-in setting
        click_el(b, '[data-act=setting][data-name="Bass Boost"]')
        check('a_built_in_setting_applies: `Bass Boost` is applied and its word lights',
              poll(lambda: [x['on'] for x in loki.model(LOKI)['device_presets']['builtins']
                            if x['name'] == 'Bass Boost'] == [True], 8.0)
              and poll(lambda: b.page("document.querySelector('[data-act=setting][data-name=\"Bass Boost\"]')"
                                      ".classList.contains('on')")), loki.model(LOKI)['device_presets']['builtins'])
        # a slot is recalled by its word
        click_el(b, '[data-act=slot][data-slot="2"]')
        time.sleep(0.5)
        check('  a slot word recalls the slot', json.loads(posts_to(b, 'device-presets/recall')[-1]) == {'slot': 2},
              posts_to(b, 'device-presets/recall'))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        loki.stop()


def the_switch():
    """Another device shown while a form or a list is open: it closes, nothing stays held."""
    fleet = common.Daemon('gungnir2-v2', fleet=[('lokimax', LOKI, '4142434445464748')])
    b = make()
    try:
        forget(b, fleet)
        load(b, fleet)
        other = b.page(f"__core.model.page().fleet.devices.find((d) => d.name === {json.dumps(LOKI)}).url")
        click_el(b, '[data-act=preset-save]')
        poll(lambda: b.page("!!document.querySelector('#p-presets .p-askbox')"))
        b.page(f"__core.actions.showDevice({json.dumps(other)})")
        check('a_device_switch_closes_what_is_open: a form open on one device is gone when another is shown',
              poll(lambda: b.page("!document.querySelector('.p-askbox') && !document.getElementById('p-presets')"
                                  ".children.length && !!document.querySelector('[data-act=slot]')")),
              b.page("document.getElementById('p-presets').innerHTML"))
        click_el(b, '[data-act=slot-save-list]')
        poll(lambda: b.page("!!document.querySelector('.plist')"))
        mine = b.page("__core.model.page().fleet.devices.find((d) => d.name !== "
                      f"{json.dumps(LOKI)}).url")
        b.page(f"__core.actions.showDevice({json.dumps(mine)})")
        check('  and a list open on the Loki Max is gone when the Gungnir is shown again, the sections '
              'drawn for it',
              poll(lambda: b.page("!document.querySelector('.plist') && !document.querySelector('[data-act=slot]')"
                                  " && !!document.querySelector('[data-act=preset-save]')")),
              b.page("document.getElementById('p-device-presets').innerHTML"))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        fleet.stop()


def drag_across_a_switch():
    """A needle drag still going when another device is shown writes nothing, and the volume is
    not left held: back on the first device a key press on the needle writes."""
    fleet = common.Daemon('gungnir2-v2', host='fake', fleet=[('fake-amp', 'Amp', '1112131415161718')])
    b = make()
    try:
        forget(b, fleet)
        load(b, fleet)
        poll(lambda: b.page("!!document.getElementById('p-needle')"), 10.0)
        time.sleep(0.8)
        got = needle(b)
        b.page(POSTS)
        press(b, got['cx'], got['top'])
        for i in range(1, 4):
            move(b, got['cx'] + 6 * i, got['top'])
        poll(lambda: len(volume_posts(b)) > 0, 5.0)
        click_el(b, '.p-also [data-go=device]')
        poll(lambda: b.page("document.querySelector('#p-title h1').textContent") == 'Amp', 8.0)
        time.sleep(0.5)
        b.page(POSTS)
        for i in range(1, 8):
            move(b, got['cx'] + 18 + 12 * i, got['top'])
        time.sleep(0.8)
        release(b, got['cx'] + 120, got['top'])
        check('a_drag_that_outlives_a_device_switch_writes_nothing: the pointer kept moving after the amp '
              'was shown and no volume was sent', volume_posts(b) == [], volume_posts(b))
        time.sleep(0.5)
        b.page("__core.actions.showDevice(__core.model.page().fleet.devices.find((d) => d.name === 'Gungnir 2').url)")
        poll(lambda: b.page("document.querySelector('#p-title h1').textContent") == 'Gungnir 2'
             and b.page("!!document.getElementById('p-needle')"), 10.0)
        time.sleep(0.8)
        b.page(POSTS)
        b.page("document.getElementById('p-needle').focus()")
        b.press('ArrowLeft')
        check('  back on the Gungnir the volume is not held: a key press on the needle writes',
              poll(lambda: len(volume_posts(b)) > 0, 5.0), volume_posts(b))
        # a step asked of the Gungnir and not yet answered is not the new start of `louder +` after
        # a switch: the writes are held back so the ask stays unanswered
        b.page("""(() => { const f = window.fetch; window.__vol = [];
          window.fetch = (...a) => { const u = String(a[0]);
            if (u.endsWith('/volume')) { window.__vol.push(JSON.parse(String((a[1] || {}).body)).raw);
              return new Promise((r) => setTimeout(r, 2500)).then(() => f(...a)); }
            return f(...a); }; return 1; })()""")
        base = b.page("__core.model.part('volume').raw")
        click_el(b, '[data-act=volume-up]')
        poll(lambda: b.page('window.__vol.length') == 1, 5.0)
        b.page("__core.actions.showDevice(__core.model.page().fleet.devices.find((d) => d.name === 'Amp').url)")
        poll(lambda: b.page("document.querySelector('#p-title h1').textContent") == 'Amp', 8.0)
        b.page("__core.actions.showDevice(__core.model.page().fleet.devices.find((d) => d.name === 'Gungnir 2').url)")
        poll(lambda: b.page("!!document.getElementById('p-needle')"), 8.0)
        time.sleep(0.5)
        click_el(b, '[data-act=volume-up]')
        poll(lambda: b.page('window.__vol.length') == 2, 5.0)
        check('  after a switch `louder +` steps from the device\'s own reading, not from the step asked '
              'before', b.page('window.__vol') == [base + 1, base + 1], [base, b.page('window.__vol')])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        fleet.stop()


def the_list_hold():
    hati = common.Daemon(profile='hati', name='Hati')
    b = make()
    try:
        forget(b, hati)
        load(b, hati)
        click_el(b, '[data-act=choice-list][data-key=load_capacitance]')
        poll(lambda: b.page("!!document.querySelector('.plist')"))
        word = "document.querySelector('[data-act=toggle][data-key=subsonic]').textContent"
        was = b.page(word)
        hati.write('Hati', 'subsonic', was == 'off')
        time.sleep(1.2)
        state = b.page("""({ open: !!document.querySelector('.plist'),
          focus: !!document.activeElement.closest('.plist') })""")
        check('a_controls_list_holds_its_section: the list stays open with its focus while another '
              'control changes', state['open'] and state['focus'] and b.page(word) == was, [state, b.page(word)])
        b.press('Escape')
        check('  Escape closes it, the focus is back on its word and the section shows the newest reading',
              poll(lambda: b.page(word) != was, 5.0)
              and b.page("document.activeElement.dataset.key") == 'load_capacitance', b.page(word))
        click_el(b, '[data-act=choice-list][data-key=load_capacitance]')
        poll(lambda: b.page("!!document.querySelector('.plist')"))
        picked = b.page("document.querySelector('.plist .p-opt:not(.on)').dataset.value")
        click_el(b, '.plist .p-opt:not(.on)')
        check('  a value chosen from the list is written and the list closes',
              poll(lambda: next(o['label'] for o in next(c for c in hati.model('Hati')['controls']
                                                         if c['key'] == 'load_capacitance')['options']
                                if o['on']) == picked, 8.0)
              and poll(lambda: b.page("!document.querySelector('.plist')")), picked)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        hati.stop()


HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""


def reach(b, selector):
    """Click the element after scrolling it to the middle of the window."""
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.2)
    click_el(b, selector)


def the_transport(b, d):
    forget(b, d)
    load(b, d)
    b.page(POSTS)
    words = b.page("[...document.querySelectorAll('#p-playing [data-act=transport]')]"
                   ".map((w) => [w.dataset.button, w.textContent])")
    check('the_transport_words_press: back, pause and next are written under the track',
          words == [['previous', 'back'], ['pause', 'pause'], ['next', 'next']], words)
    track = b.page("document.querySelector('#p-playing .p-track').textContent")
    check('  the track is its headline and its artist', track == b.page(
        "__core.model.device().playing.track.headline + ', ' + __core.model.device().playing.track.artist"),
          track)
    click_el(b, '[data-act=transport][data-button=next]')
    check('  `next` posts the host\'s press',
          poll(lambda: posts_to(b, 'host/press') == ['{"button":"next"}'], 5.0), posts_to(b, 'host/press'))
    click_el(b, '[data-act=transport][data-button=pause]')
    check('  `pause` posts its press, the host model says paused and the word becomes `play`',
          poll(lambda: posts_to(b, 'host/press')[-1:] == ['{"button":"pause"}'], 5.0)
          and poll(lambda: b.page("__core.model.device().playing.track.status") == 'paused', 8.0)
          and poll(lambda: b.page("[...document.querySelectorAll('#p-playing [data-act=transport]')]"
                                  ".map((w) => w.textContent)") == ['back', 'play', 'next'], 5.0),
          b.page("document.querySelector('#p-playing .p-transport').textContent"))
    click_el(b, '[data-act=transport][data-button=play]')
    check('  `play` brings back `pause`', poll(lambda: b.page(
        "document.querySelector('#p-playing .p-transport').textContent") == 'back \u00b7 pause \u00b7 next', 8.0),
          b.page("document.querySelector('#p-playing .p-transport').textContent"))
    check('  no page error', errors(b) == [], errors(b)[:3])


FRACTIONS = """(() => __core.model.device().meters.meters.map((m) => {
  const mark = document.querySelector(`.p-scale.meter[data-key="${m.key}"] .p-mark`);
  return [m.fraction, !mark || mark.dataset.at === undefined ? null : Number(mark.dataset.at)]; }))()"""


def the_amp():
    amp = common.Daemon('aegir2', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make()
    try:
        forget(b, amp)
        load(b, amp)
        # the fake amp starts in standby: it is turned on first, and gives its readings once warm
        if b.page("document.querySelector('[data-act=power]').textContent") == 'off':
            click_el(b, '[data-act=power]')
        check('  the amp is on and reads its meters',
              poll(lambda: b.page("__core.model.device().meters?.meters.some((m) => m.fraction !== null)"), 30.0))
        b.page(POSTS)
        seen = set()
        close = True
        for _ in range(10):
            for model, mark in b.page(FRACTIONS):
                seen.add(mark)
                close = close and ((model is None and mark is None)
                                   or (model is not None and mark is not None and abs(model - mark) <= 0.05))
            time.sleep(0.3)
        check('an_amp_shows_power_meters_and_its_readouts: every needle sits at its meter\'s fraction, '
              'read in the same tick, and moves', close and len(seen) > 2, [close, sorted(seen, key=str)])
        # the meter section as the page's own module draws it for a reading, its first draw: the needle
        # stands at the fraction, never at the class-A share beside it
        drawn = b.page("""import(new URL('assets/themes/paper/draw/amp.js', document.baseURI)).then((m) => {
          const box = document.createElement('div');
          box.innerHTML = m.meters({}, { meters: { state: '', meters: [{ key: 'k', channel: 'L',
            fraction: 0.7, class_a: 0.25, text: 'x' }] } }, {});
          return [box.querySelector('.p-scale.meter .p-mark').dataset.at,
            box.querySelector('.p-zone.classa').dataset.at]; })""")
        check('  a meter\'s first draw puts the needle at its fraction and the hatch at its class-A share',
              drawn == ['0.70000', '0.25000'], drawn)
        shape = b.page("""({ meters: document.querySelectorAll('#p-meters .p-scale.meter').length,
          zones: [...document.querySelectorAll('#p-meters .p-zone.classa')].map((z) => z.getBoundingClientRect().width > 0),
          power: document.getElementById('p-power').textContent.trim().startsWith('Power: on'),
          printed: document.getElementById('p-power').textContent.includes('Warming up')
            || !!document.querySelector('#p-power [data-readout]') })""")
        check('  two meters with their class-A share hatched, `Power: on`, and no readout line',
              shape['meters'] == 2 and all(shape['zones']) and shape['power'] and not shape['printed'], shape)
        click_el(b, '[data-act=power]')
        check('  `on` becomes `off`: power is written false',
              poll(lambda: [json.loads(x) for x in posts_to(b, 'controls/power')] == [{'value': False}], 8.0)
              and poll(lambda: amp.model('Amp')['power']['on'] is False, 8.0)
              and poll(lambda: b.page("document.querySelector('[data-act=power]').textContent") == 'off'),
              [posts_to(b, 'controls/power'), amp.model('Amp')['power']])
        click_el(b, '[data-act=power]')
        check('  and back to `on`',
              poll(lambda: amp.model('Amp')['power']['on'] is True, 8.0)
              and poll(lambda: b.page("document.querySelector('[data-act=power]').textContent") == 'on'),
              amp.model('Amp')['power'])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        amp.stop()


def the_spec(b, d):
    forget(b, d)
    load(b, d)
    facts = d.model(DEVICE)['facts']
    got = b.page("Object.fromEntries([...document.querySelectorAll('#p-device [data-fact]')]"
                 ".map((t) => [t.dataset.fact, t.textContent]))")
    want = {'Product': facts['product'], 'Firmware': facts['firmware'], 'Profile': facts['profile'],
            'Link': facts['link'], 'Unit': facts['unit']}
    want['DSP'] = '—' if facts['dsp'] is None else str(facts['dsp'])
    check('the_spec_table_lists_every_fact: product, unit, firmware, DSP, profile and link say what the model says',
          all(got.get(k) == v for k, v in want.items()), [got, want])
    check('  the signal and every reading row are there, each with the age it was heard',
          'Signal' in got and all(r['id'] in got for r in facts['rows'])
          and b.page("document.querySelectorAll('#p-readings tr').length") == len(facts['rows']),
          [len(facts['rows']), b.page("document.querySelectorAll('#p-readings tr').length")])
    check('  the device\'s section is headed `The device` and `All readings`',
          b.page("[...document.querySelectorAll('#p-device .p-label')].map((h) => h.textContent)")
          == ['The device', 'All readings'])
    b.page(POSTS)
    reach(b, '[data-act=identify]')
    check('  `identify` asks the unit and says so',
          poll(lambda: posts_to(b, '/identify') == ['{}'], 5.0), posts_to(b, '/identify'))
    check('  no page error', errors(b) == [], errors(b)[:3])


WATCHED = """(() => { window.__seen = { card: false, waiting: false, line: false };
  const sheet = document.getElementById('paper');
  const look = () => { if (document.querySelector('.pn.red.card')) window.__seen.card = true;
    if (sheet.classList.contains('waiting')) window.__seen.waiting = true;
    const sub = document.querySelector('.p-sub');
    if (sub && sub.textContent === 'reading the device again…') window.__seen.line = true; };
  new MutationObserver(look).observe(sheet, { attributes: true, childList: true, subtree: true,
    characterData: true }); look(); return 1; })()"""
SUB = "document.querySelector('.p-sub')?.textContent ?? ''"


def the_states():
    # a device out of reach: the card is the red note under the title; try now posts connect
    gone = b = None
    try:
        gone = common.Daemon('gungnir2-v2')
        b = make()
        gone.drop()
        forget(b, gone)
        load(b, gone)
        b.page(POSTS)
        check('the_states_in_papers_words: a device out of reach draws its card as the red note',
              poll(lambda: b.page("!!document.querySelector('#p-card .pn.red.card')"), 20.0))
        got = b.page("""(() => { const note = document.querySelector('#p-card .pn.red.card');
          const t = document.getElementById('p-title').getBoundingClientRect();
          const n = note.getBoundingClientRect();
          return { title: note.textContent, below: n.top >= t.bottom - 1,
            try: document.querySelector('#p-card [data-act=try]')?.textContent,
            retry: document.querySelector('#p-card .p-retry')?.textContent,
            notes: document.querySelectorAll('.pn.red').length }; })()""")
        check('  its title sits under the title line, with `try now` and the seconds to the next try',
              got['title'].endswith('is out of reach') and got['below'] and got['try'] == 'try now'
              and (got['retry'] or '').startswith('trying again') and got['notes'] == 1, got)
        click_el(b, '#p-card [data-act=try]')
        check('  `try now` posts connect', poll(lambda: posts_to(b, '/connect') == ['{}'], 5.0),
              posts_to(b, '/connect'))
    finally:
        if b:
            b.close()
        if gone:
            gone.stop()
    # the daemon stopped and back, and the page hidden and shown
    d = b = None
    try:
        d = common.Daemon('gungnir2-v2')
        b = make()
        forget(b, d)
        load(b, d)
        check('  with the daemon there the subtitle is the device\'s line', 'Reconnecting' not in b.page(SUB))
        d.proc.terminate()
        d.proc.wait(5)
        check('  the daemon stopped: the subtitle reads `Reconnecting to svolder…`, the sheet is faded and inert',
              poll(lambda: b.page(SUB) == 'Reconnecting to svolder…', 20.0)
              and b.page("document.getElementById('paper').classList.contains('faded')")
              and b.page("document.getElementById('paper').inert"), b.page(SUB))
        d.restart()
        check('  restarted: the subtitle comes back and the sheet can be used',
              poll(lambda: 'Reconnecting' not in b.page(SUB) and b.page(SUB) != '', 30.0)
              and not b.page("document.getElementById('paper').inert"), b.page(SUB))
        b.page(WATCHED)
        b.page(HIDE)
        time.sleep(3.0)
        b.page(SHOW)
        check('  hidden 3 s then shown: the page waits (`reading the device again…`) until the first reading',
              poll(lambda: b.page('window.__seen.waiting'), 10.0) and poll(lambda: b.page('window.__seen.line'), 10.0)
              and poll(lambda: not b.page("document.getElementById('paper').classList.contains('waiting')"), 15.0),
              b.page('window.__seen'))
        # a stopped daemon is a refused connection in the console, which is the point of the case
        unexpected = [e for e in errors(b) if 'ERR_CONNECTION_REFUSED' not in e
                      and 'ERR_INCOMPLETE_CHUNKED_ENCODING' not in e]
        check('  no page error but the refused connections of the stopped daemon', unexpected == [],
              unexpected[:3])
    finally:
        if b:
            b.close()
        if d:
            d.stop()


def the_fleet():
    fleet = common.Daemon('gungnir2-v2', fleet=[('fake-amp', 'Amp', '1112131415161718')],
                          env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make()
    try:
        forget(b, fleet)
        load(b, fleet)
        line = b.page("document.querySelector('.p-also')?.textContent")
        check('a_second_device_is_one_tap_away: the title says `also here: Amp`', line == 'also here: Amp', line)
        click_el(b, '.p-also [data-go=device]')
        check('  a tap shows the amp: the address is `?device=Amp` and Paper draws its meters',
              poll(lambda: b.page("location.search") == '?device=Amp'
                   and b.page("document.querySelector('#p-title h1').textContent") == 'Amp'
                   and b.page("document.querySelectorAll('#p-meters .p-scale.meter').length") == 2, 8.0),
              [b.page('location.search'), b.page("document.querySelector('#p-title h1').textContent")])
        check('  and the Gungnir is now the other device',
              b.page("document.querySelector('.p-also').textContent") == 'also here: Gungnir 2')
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        fleet.stop()


def presets_toml(names):
    band = ('band = [{ f = 40, g = 3.0, q = 0.7 }, { f = 160, g = 0.0, q = 4.3 }, { f = 632, g = 0.0, q = 4.3 }, '
            '{ f = 2515, g = 0.0, q = 4.3 }, { f = 10000, g = 0.0, q = 4.3 }]')
    return ''.join(f'\n[[preset]]\nname = "{n}"\nbands = 5\nmode = "eq"\nloudness = 0\n{band}\n' for n in names)


REACH = """(() => { const w = innerWidth; const over = [];
  for (const el of document.querySelectorAll('#paper [data-act], #paper h1, #paper .p-sub, #paper .p-track,'
    + ' #paper .p-detail, #paper .pn, #paper td, #paper th, #paper .p-also, #paper .p-zone span,'
    + ' #paper .p-mark')) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && (r.right > w + 0.5 || r.left < -0.5)) over.push([el.tagName, el.className,
      el.dataset.act || el.id || el.textContent.slice(0, 24), Math.round(r.left), Math.round(r.right)]); }
  return { scroll: document.documentElement.scrollWidth, body: document.body.scrollWidth, inner: w, over }; })()"""

NOTES = """(() => Object.fromEntries([...document.querySelectorAll('.pn[data-note]')]
  .map((n) => [n.dataset.note, n.textContent])))()"""
BOX = """(selector) => { const r = document.querySelector(selector).getBoundingClientRect();
  return { l: r.left, t: r.top, r: r.right, b: r.bottom }; }"""


def seconds(text):
    """The seconds a track note's `m:ss of ...` says have gone, or None."""
    found = re.match(r'(\d+):(\d\d) of ', text or '')
    return int(found.group(1)) * 60 + int(found.group(2)) if found else None


def the_notes(b, d):
    forget(b, d)
    load(b, d)
    want = ['eq', 'path', 'track', 'firmware', 'link']
    got = poll(lambda: all(k in b.page(NOTES) for k in want) and b.page(NOTES), 10.0)
    check('notes_appear_and_update: the EQ, the path, the track, the firmware and the link are written',
          bool(got), b.page(NOTES))
    got = b.page(NOTES)
    facts = b.page('__core.model.device().facts')
    check('  the firmware note says the model\'s firmware and the track note names the album',
          got['firmware'].startswith(f"firmware {facts['firmware']}")
          and got['track'].endswith('\u00b7 Demo Tape') and re.match(r'\d+:\d\d of \d+:\d\d', got['track']),
          got)
    paper = b.page("document.getElementById('paper').textContent")
    check('  no arrow glyph and no marker is left in the sheet\'s words',
          '\u2192' not in paper and '\ue000' not in paper)
    poll(lambda: 'rate matches all the way' in b.page(NOTES).get('path', ''), 10.0)
    tick = b.page("!!document.querySelector('.pn[data-note=path] svg.p-tick')")
    check('  `rate matches all the way` ends in an ink stroke',
          'rate matches all the way' in b.page(NOTES)['path'] and tick, b.page(NOTES)['path'])
    before = got['eq']
    for _ in range(14):
        click_el(b, '#p-b3gp')
    check('  band 3 gain `+` changes the EQ note within 2 s',
          poll(lambda: b.page(NOTES)['eq'] != before, 2.0), [before, b.page(NOTES)['eq']])
    one = seconds(b.page(NOTES)['track'])
    time.sleep(2.3)
    two = seconds(b.page(NOTES)['track'])
    check('  the track note\'s time advances over 2 s', one is not None and two is not None
          and 1 <= two - one <= 4, [one, two])
    check('  no page error', errors(b) == [], errors(b)[:3])


def from_this_browser(b, d):
    forget(b, d)
    load(b, d)
    lo = d.model(DEVICE)['volume']['lo']
    d.post(DEVICE, 'volume', {'raw': lo + 30})
    settled(d, lo + 30)
    time.sleep(0.5)
    b.page(POSTS)
    d.post(DEVICE, 'volume', {'raw': lo + 26})
    changed = poll(lambda: (b.page(NOTES).get('change') or '').startswith('last change: '), 8.0)
    text = b.page(NOTES).get('change', '')
    check('from_this_browser_only_for_this_pages_change: a write from elsewhere gives `last change` and its time',
          changed and re.search(r', \d\d:\d\d$', text) and 'from this' not in text, text)
    time.sleep(3.5)
    click_el(b, '[data-act=volume-up]')
    check('  `louder +` from this page ends in `, from this browser`',
          poll(lambda: (b.page(NOTES).get('change') or '').endswith(', from this browser'), 8.0),
          b.page(NOTES).get('change'))
    now = chosen(control(d, 'input'))
    want = next(o['label'] for o in control(d, 'input')['options'] if o['label'] != now and o['label'] != 'AES')
    time.sleep(3.5)
    click_el(b, f'[data-act=choice][data-key=input][data-value="{want}"]')
    text = ''
    poll(lambda: (b.page(NOTES).get('change') or '').startswith('last change: Input'), 8.0)
    text = b.page(NOTES).get('change') or ''
    check('a_control_change_reaches_the_last_change_note: a choice from this page says `Input <from> to <to>`',
          text.startswith(f'last change: Input {now} to {want}, ') and text.endswith(', from this browser'), text)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_places(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: len(b.page(NOTES)) >= 5, 10.0)
    b.resize(1280, 1000)
    time.sleep(0.4)
    got = b.page("""(() => { const band = document.getElementById('p-notes').getBoundingClientRect();
      const cols = document.querySelector('.p-cols').getBoundingClientRect();
      const out = { band: [band.left, band.right], below: band.top >= cols.bottom - 1, notes: [] };
      for (const n of document.querySelectorAll('.pn[data-note]')) {
        const a = document.getElementById('p-' + n.dataset.about).getBoundingClientRect();
        const r = n.getBoundingClientRect(); const third = band.width / 3;
        const want = Math.min(2, Math.max(0, Math.floor(((a.left + a.width / 2 - band.left) / band.width) * 3)));
        out.notes.push([n.dataset.note, n.dataset.about, want, (r.left + r.width / 2 - band.left) / third,
          Number(n.dataset.n), n.parentElement.id]); }
      return out; })()""")
    inside = all(w <= c <= w + 1 and parent == 'p-notes' for _, _, w, c, _, parent in got['notes'])
    ordered = [x[4] for x in got['notes']] == sorted(x[4] for x in got['notes'])
    check('each_note_sits_near_what_it_is_about: on a desktop each note is in the band\'s third under its '
          'section\'s centre, in note order, below the columns',
          got['below'] and len(got['notes']) >= 5 and inside and ordered, got)
    b.resize(390, 1400)
    time.sleep(0.6)
    got = b.page("""(() => [...document.querySelectorAll('.pn[data-note]')].map((n) => {
      let prev = n.previousElementSibling;
      while (prev && prev.classList.contains('pn') && prev.dataset.about === n.dataset.about) {
        prev = prev.previousElementSibling; }
      return [n.dataset.note, n.dataset.about, prev ? prev.id : null, n.parentElement.id]; }))()""")
    check('  on a phone each note follows its section in the flow',
          len(got) >= 5 and all(about and prev == f'p-{about}' and parent != 'p-notes'
                                for _, about, prev, parent in got), got)
    b.resize(1280, 900)
    time.sleep(0.4)
    back = b.page("[...document.querySelectorAll('.pn[data-note]')].every((n) => n.parentElement.id === 'p-notes')")
    check('  back at desktop width the notes are in the band again', back)
    check('  no page error', errors(b) == [], errors(b)[:3])


RED = """(() => { const reds = [...document.querySelectorAll('.pn.red')];
  const el = reds[0]; const r = el && el.getBoundingClientRect();
  const ell = [...document.querySelectorAll('#p-marks ellipse')].map((e) => {
    const x = e.getBoundingClientRect(); return { l: x.left, t: x.top, r: x.right, b: x.bottom }; });
  return { reds: reds.length, id: el && (el.dataset.note || 'card'), text: el && el.textContent,
    box: r && { l: r.left, t: r.top, r: r.right, b: r.bottom }, ellipses: ell,
    rings: [...document.querySelectorAll('.pn.red span.ring')].map((x) => x.textContent),
    ink: document.querySelectorAll('#p-marks path.arrow.ink').length,
    pencil: document.querySelectorAll('#p-marks path.arrow.red').length,
    scroll: [document.documentElement.scrollWidth, innerWidth] }; })()"""


def circled(got):
    box, ell = got['box'], got['ellipses']
    return (len(ell) == 1 and box is not None and ell[0]['l'] <= box['l'] + 0.5 and ell[0]['t'] <= box['t'] + 0.5
            and ell[0]['r'] >= box['r'] - 0.5 and ell[0]['b'] >= box['b'] - 0.5)


def the_red_note():
    route = common.Daemon('gungnir2-v2', host='fake', env={'SVOLDER_FAKE_ROUTE': 'changed'})
    gone = common.Daemon('gungnir2-v2')
    b = make()
    try:
        forget(b, route)
        load(b, route)
        # a boost on band 3 gives the EQ a peak
        route.post(DEVICE, 'eq/mode', {'mode': 'eq'})
        poll(lambda: b.page("__core.model.device().tone?.mode") == 'eq', 10.0)
        time.sleep(1.0)
        for _ in range(14):
            click_el(b, '#p-b3gp')
        poll(lambda: b.page("document.querySelector('.pn.red')?.dataset.note") == 'peak', 10.0)
        got = b.page(RED)
        check('one_red_note_is_circled: one red note (the EQ\'s peak comes first), ringed by an ellipse '
              'whose box holds the note', got['reds'] == 1 and got['id'] == 'peak' and circled(got), got)
        check('  both notes are about the tone: only the red note\'s pencil arrow is drawn, no ink one '
              'crosses it', got['ink'] == 0 and got['pencil'] >= 2, got)
        path = b.page("document.querySelector('.pn[data-note=path]').textContent")
        arrows = b.page("""(() => { const n = document.querySelector('.pn[data-note=path]');
          const a = n.querySelector('svg.p-arrow'); return a && { stroke: getComputedStyle(a).stroke,
            ink: getComputedStyle(n).color, hidden: n.querySelector('.p-sr').textContent }; })()""")
        check('  a path of different rates draws its arrows as ink strokes and says `to`',
              arrows and arrows['stroke'] == arrows['ink'] and arrows['hidden'] == ' to '
              and ' to ' in path and '\u2192' not in path, [arrows, path])
        route.post(DEVICE, 'eq/mode', {'mode': 'flat'})
        poll(lambda: b.page("document.querySelector('.pn.red')?.dataset.note") == 'path', 10.0)
        got = b.page(RED)
        check('  without the peak the path that changes the sound is the red note, ringed',
              got['reds'] == 1 and got['id'] == 'path' and 'change the sound' in got['text'] and circled(got), got)
        check('  and with no red note at the tone the EQ note\'s ink arrow is drawn again beside the pencil one',
              got['ink'] >= 2 and got['pencil'] >= 2, got)
        b.resize(390, 1400)
        time.sleep(0.6)
        got = b.page(RED)
        check('  on a phone the red note\'s lead words are ringed in pencil and no ellipse is drawn',
              got['reds'] == 1 and len(got['rings']) == 1 and got['text'].startswith(got['rings'][0].strip())
              and got['ellipses'] == [] and got['ink'] == 0 and got['scroll'][0] <= got['scroll'][1], got)
        b.resize(1280, 900)
        check('  no page error', errors(b) == [], errors(b)[:3])
        forget(b, gone)
        gone.drop()
        load(b, gone)
        poll(lambda: b.page("!!document.querySelector('.pn.red.card')"), 20.0)
        time.sleep(0.5)
        got = b.page(RED)
        check('  a card\'s red note is the one red note, drawn once under the title, and circled',
              got['reds'] == 1 and got['id'] == 'card' and circled(got)
              and b.page("document.querySelectorAll('.pn.red').length") == 1
              and b.page("document.querySelectorAll('#p-notes .pn.red').length") == 0, got)
        check('  no page error with the card', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        route.stop()
        gone.stop()



def zone_label():
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(size=(390, 1000))
    try:
        forget(b, d)
        load(b, d)
        d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
        poll(lambda: b.page("__core.model.device().tone?.mode") == 'eq', 10.0)
        time.sleep(1.0)
        for _ in range(14):
            click_el(b, '#p-b3gp')
        poll(lambda: b.page("!!document.querySelector('.p-zone.risk')"), 10.0)
        for mode in ('light', 'dark'):
            b.page(f"__core.scheme.set('{mode}')")
            time.sleep(0.3)
            got = b.page("""(() => { const ruler = document.querySelector('#p-ruler').getBoundingClientRect();
              const spans = [...document.querySelectorAll('.p-zone span')].map((s) => {
                const r = s.getBoundingClientRect(); return [s.textContent, Math.round(r.left), Math.round(r.right)]; });
              return { spans, ruler: [Math.round(ruler.left), Math.round(ruler.right)], inner: innerWidth,
                scroll: document.documentElement.scrollWidth }; })()""")
            reach = b.page(REACH)
            check(f'the_zone_label_stays_on_the_ruler_at_390: {mode} paper, a boosted band draws the risk zone '
                  'and its label ends inside the ruler and the viewport, with no sideways scroll',
                  len(got['spans']) >= 1 and all(s[2] <= got['ruler'][1] + 1 and s[2] <= got['inner'] and s[1] >= 0
                                                 for s in got['spans'])
                  and got['scroll'] <= got['inner'] and reach['over'] == [], [got, reach])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


def long_words():
    unbroken = 'Supercalifragilisticexpialidocious!'.ljust(40, '!')
    title = f'Everything In Its Right Place {unbroken} And Then Some More Words To Make It Long Enough'
    title = (title + ' On A Phone Screen Twice Over Again And Again')[:120]
    assert len(title) == 120 and unbroken in title
    names = [f'Preset {i}' for i in range(1, 8)] + ['W' * 40, 'Late night jazz with a warm low end and more'[:40].strip()]
    dac = amp = loki = b = None
    try:
        dac = common.Daemon('gungnir2-v2', host='fake', web='auth = "none"\n' + presets_toml(names),
                            fleet=[('fake-amp', 'Desk / Left', '1112131415161718')],
                            env={'SVOLDER_FAKE_TITLE': title, 'SVOLDER_FAKE_SIGNAL': '1'})
        amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
        loki = common.Daemon(profile='lokimax', name=LOKI)
        b = make(size=(390, 1000))
        for label, daemon, query in (('the DAC', dac, ''), ('the amp', amp, ''), ('the Loki Max', loki, ''),
                                     ('the odd name', dac, 'odd')):
            forget(b, daemon)
            load(b, daemon)
            b.resize(390, 1000)
            if query == 'odd':
                click_el(b, '.p-also [data-go=device]')
                poll(lambda: b.page("document.querySelector('#p-title h1').textContent") == 'Desk / Left', 8.0)
            time.sleep(0.5)
            if daemon is dac:
                drawn = b.page("""({ names: [...document.querySelectorAll('#p-presets [data-act=preset]')]
                  .map((w) => w.dataset.name), h1: document.querySelector('#p-title h1').textContent,
                  also: document.querySelector('.p-also')?.textContent ?? '',
                  track: document.querySelector('#p-playing .p-track')?.textContent ?? '' })""")
                if query == '':
                    ok = ('W' * 40 in drawn['names'] and sum(n in drawn['names'] for n in names) == 9
                          and 'Desk / Left' in drawn['also'] and unbroken in drawn['track']
                          and drawn['track'].startswith(title[:100]))
                else:
                    ok = drawn['h1'] == 'Desk / Left'
                check(f'  the long things are drawn ({label}): nine presets with the 40-character one, '
                      'the odd device name and the long title (its first 100 characters and the '
                      'unbroken 40-character word)', ok, drawn)
            for mode in ('light', 'dark'):
                b.page(f"__core.scheme.set('{mode}')")
                time.sleep(0.3)
                got = b.page(REACH)
                check(f'no_sideways_scroll_at_390_with_long_words: {label}, {mode} paper fits the width '
                      'and every word is inside it',
                      got['scroll'] <= got['inner'] and got['body'] <= got['inner'] and got['over'] == [], got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        if b:
            b.close()
        for daemon in (dac, amp, loki):
            if daemon:
                daemon.stop()


d = common.Daemon('gungnir2-v2', host='fake')
try:
    auto(d)
    b = make()
    try:
        the_sheet(b, d)
        title_and_footer(b, d)
        the_button(b, d)
        the_menu(b, d)
        dark_first(b, d)
        contrast(b, d)
        card_contrast()
        no_scroll(b, d)
        the_volume(b, d)
        the_cap()
        if browser == 'chromium':
            the_hold(b, d)
        the_point_drag(b, d)
        the_table(b, d)
        the_lists(b, d)
        if browser == 'chromium':
            tone_hold(b, d)
        the_controls(b, d)
        the_balance(b, d)
        the_presets(b, d)
        the_refusal(b, d)
        if browser == 'chromium':
            the_survival(b, d)
            the_touch(d)
        one_listener(b, d)
        the_notes(b, d)
        from_this_browser(b, d)
        the_places(b, d)
        the_transport(b, d)
        the_spec(b, d)
        check('no page error in the whole run', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
finally:
    d.stop()
the_device_presets()
the_switch()
the_amp()
the_red_note()
the_states()
the_fleet()
zone_label()
long_words()
if browser == 'chromium':
    the_list_hold()
    drag_across_a_switch()
print(f'paper.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
