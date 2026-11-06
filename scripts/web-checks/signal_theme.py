# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Signal in a browser: its registration and its frame (the line of hops, its dots, the
box round the chosen hop, choosing by the pointer and the keys), the DAC's first screen (the
volume, the curve, the choices) with the Now playing bar, and the shelf below it with the slim bar.
Declared hops: the + sheet and the edits of a hop the person declared. The phone: the strip. Later
tasks add the rest of the open area's checks here.
  signal_is_listed_and_mounts_from_the_picker   `?theme=signal` and a pick from Ember
  the_line_shows_every_hop_in_order             player, computer, the device, then its hops
  declared_hops_are_rings_and_live_ones_dots    plain rings for what svolder does not drive
  a_click_moves_the_box                         the box and the open area follow the hop clicked
  a_draw_between_press_and_release              a draw between a click's press and release keeps the click
  a_draw_in_a_drag                              a draw in a finger drag keeps the track where the finger has it
  left_right_move_the_choice                    arrows, Home and End on the line
  the_dots_move_while_playing_and_stop_on_pause
  the_dots_stand_still_under_reduced_motion     one still frame, no loop
  no_frame_while_hidden                         the dashes hold while the page is hidden
  a_hop_that_leaves_gives_the_box_back          a hop removed, a device switched to
  no_host_starts_the_line_at_the_device         the quiet word, no player or computer
  an_amp_no_hop_links_stands_alone              greyed way in, the ring for the DAC svolder does not drive
  the_curve_has_a_row_of_its_own_in_the_middle_layout   768 and 900 px: the curve and an amp's meters 300 px or wider, a point drags
  a_linked_amp_shown_alone_has_the_dac_before_it   greyed way in, the DAC its row names (a live hop), a thin stretch, the amp
  contrast_on_the_ground                        every kind of text reads at AA
  the_first_screen_has_h6s_columns              310 px, the rest, 190 px, 40 px apart; the volume at 96 px
  minus_and_plus_step_the_volume                the buttons, Mute, the keys on the track
  dragging_past_the_cap_writes_the_cap          [web] max_volume_db holds a drag of the knob
  a_point_drag_sets_a_band                      a drag of band 2's point writes the steps nearest
  a_tone_update_waits_for_the_point_drag        a change from elsewhere during the drag is drawn after (Chromium)
  an_input_chip_sets_the_input                  the chip writes it; the mode chip and the preset list
  a_guests_tap_posts_nothing                    inert controls, the core's words, no request
  the_bottom_bar_plays_and_pauses               the transport presses the player; the dots follow
  the_bar_shows_the_cover_when_there_is_one     the host's picture before the transport, 36 px; the player's area too
  the_cover_follows_the_track                   another address, a 404 hides it with no gap; no cover, no box
  no_cover_leaves_no_gap                        the daemon stops holding it: the transport starts the bar
  the_band_table_steps_each_value               steps, a typed value, Escape, a value kept while typing
  a_preset_saves_renames_and_deletes            Save as, Rename, Delete in place; Delete asks first
  a_slot_save_asks_first                        a Loki Max slot is written only after `Save`
  an_open_list_survives_a_part_update           a list and a question stay through a change from elsewhere
  the_facts_unfold_and_the_choice_survives_a_reload   folded by default, kept as `open` / `closed`
  the_power_switch_writes                       a preamp's shelf has its power, a tap writes it
  the_slim_bar_pins_when_the_line_scrolls_away  64 px under the frame's layer, the chosen hop with the volume
  a_slim_bar_tap_chooses_and_scrolls_up         the tap chooses the hop and the page goes to the top
  a_linked_amp_is_a_live_dot                    a solid amber dot, followed by the stream; its area: power, meters, the amp
  an_amp_control_writes_to_the_amps_url         power, a switch and Identify post to the amp's route, never the DAC's
  the_dac_keeps_its_volume_written_while_the_amp_is_chosen   the DAC's label carries the volume and follows it
  the_dacs_name_opens_the_device_list           a second tap on the shown device's name lists the devices
  a_chosen_amp_that_leaves_the_fleet_gives_the_box_back_to_the_dac   no throw, no write to the gone URL
  unlink_removes_the_hop_and_the_amp_stands_alone   a question first; the hop is removed; the amp alone on its line
  a_guest_cannot_unlink_or_switch_an_amp        inert, the core's words, nothing posted
  the_plus_sheet_adds_a_declared_hop            the sheet, Add, then `svolder hops list` (same config, temp homes) shows it; a connected amp links
  edit_changes_the_name_and_note                Edit in place, the box follows a renamed hop; a linked hop edits its note only
  move_earlier_reorders_the_line                the whole list's place; the ends are disabled
  remove_asks_then_removes                      one question in place; Keep sends nothing
  a_chosen_hop_removed_elsewhere_gives_the_box_back   the form and the question close, nothing posted
  a_guest_cannot_add_or_edit_a_hop              the menu's Add device… refuses, every edit inert, the core's words, nothing posted
  the_strip_centres_the_chosen_hop              390 px: a strip 92 px high, the chosen hop at the middle and boxed 112 by 74
  a_swipe_chooses_the_neighbour                 a drag or a tap moves the choice one hop; vertical strokes still scroll
  the_strip_pins_when_scrolled                  sticky at the top under the frame's layer; focus lands clear of it
  touch_targets_are_44_px                       the buttons, hops, fields and sliders, the Add device sheet, Loudness's round buttons
  no_sideways_scroll_with_long_hop_names        a DAC, an amp and a Loki Max, 390 and 1100 px
  the_said_and_stub_do_not_overlap              host off and the stream lost, three widths
  the_menu_button_clears_the_strip              a 44 px button over no hop, dot, wire or box (360, 390, 719 px)
  the_strips_dots_stop                          reduced motion and a hidden page, at 390 px
  focus_clears_the_pinned_block_with_both_lines the stub and the quiet word both under the strip
Usage: python3 signal_theme.py chromium|firefox"""
import json
import math
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
PICKER_CARDS = "[...document.querySelectorAll('#picker .tcard')].map((c) => c.dataset.theme)"
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
HOPS = """[...document.querySelectorAll('#sg-line [role=option]')].map((o) => o.dataset.hop)"""
CHOSEN = """(() => { const o = document.querySelector('#sg-line [aria-selected=true]');
  return o && o.dataset.hop; })()"""
OFFSET = """(() => { const l = document.querySelector('#sg-line [data-flow=before]');
  return l && l.getAttribute('stroke-dashoffset'); })()"""
# a declared pair of hops after the first device, and the second device linked as an amp
# the elements Signal places through the CSSOM (a style set by script, which the content policy allows)
CSSOM = ''.join(f':not(.{c})' for c in ('sg-hop', 'sg-box', 'sg-pt', 'sg-vknob', 'sg-fill', 'sg-flab',
                                         'sg-vcap', 'sg-dlab', 'sg-list', 'sg-sknob', 'sg-sctr', 'sg-needle',
                                         'sg-sop', 'sg-slide'))
HOPS_TOML = ('auth = "none"\n\n[[hop]]\ndevice = "Amp"\nafter = "Gungnir 2"\n\n'
             '[[hop]]\nname = "HD 650"\nkind = "headphones"\nafter = "Gungnir 2"\nnote = "on the desk"\n')
FLEET = [('aegir2', 'Amp', '1112131415161718')]


def make(size=(1280, 900), **kw):
    if browser == 'chromium':
        return common.Chromium(size=size, **kw)
    return common.Firefox(size=size, **kw)


def errors(b):
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'favicon.ico' not in e]


def poll(read, secs=5.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, query='?theme=signal', route=False):
    """Load the page and wait for the theme to be on show: the id of the theme mounted. With
    `route` it also waits for the computer's route, which a page reads once it watches the host."""
    b.goto(f'{d.url}{query}', settle=1.0)
    theme = b.page(BOOT)
    if route:
        poll(lambda: b.page("(() => { const w = document.querySelector('[data-hop=computer] .sg-w');"
                            " return !!w && w.textContent !== 'route not read'; })()"), 10.0)
    return theme


def forget(b, d):
    """This browser forgets its choice of theme and every preference (storage and cookie)."""
    b.goto(f'{d.url}assets/core/frame.css', settle=0.2)
    b.page("""(() => { try { localStorage.clear(); } catch (e) {}
      document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()""")


def click_el(b, selector):
    x, y = common.center(b, selector)
    b.click(x, y)


def add_device(b):
    """Open the sheet the way a person does: the frame's menu, its Add device… row."""
    click_el(b, '.fxbtn')
    poll(lambda: b.page("!!document.getElementById('menu-add-device')"), 3.0)
    click_el(b, '#menu-add-device')


def hop_click(b, hop):
    click_el(b, f'#sg-line [data-hop="{hop}"]')


def hop_click_chosen(b, hop):
    """One click on a hop, then wait for the page to have it chosen (the box and the open area follow
    a moment after the click). It never presses again: a click the page drops fails the check."""
    hop_click(b, hop)
    return poll(lambda: b.page(CHOSEN) == hop, 3.0)


def the_mount(b, d):
    forget(b, d)
    check('signal_is_listed_and_mounts_from_the_picker: ?theme=signal mounts Signal',
          load(b, d) == 'signal')
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      page: !!document.getElementById('signal'), sheets: [...document.querySelectorAll('link[data-sheet]')]
        .map((l) => l.dataset.sheet), buttons: document.querySelectorAll('#fx-button').length })""")
    check('  the page is drawn, with Signal\'s sheet of styles and one menu button',
          on == {'theme': 'signal', 'page': True, 'sheets': ['signal'], 'buttons': 1}, on)
    check('  no page error', errors(b) == [], errors(b)[:3])
    forget(b, d)
    check('  the page without the theme is Ember', load(b, d, '?effects=off') == 'ember')
    b.press('T')
    cards = b.page(PICKER_CARDS)
    check('  the picker lists Ember, Paper, Signal and Night drive',
          cards == ['ember', 'paper', 'signal', 'nightdrive'], cards)
    card = b.page("(() => { const c = document.querySelector('#picker .tcard[data-theme=signal]');"
                  " return [c.querySelector('b').textContent, c.querySelector('small').textContent,"
                  " !!c.querySelector('.tprev svg')]; })()")
    check('  and Signal\'s card has its name, its blurb and a picture',
          card == ['Signal', 'the line the sound takes', True], card)
    click_el(b, '#picker .tcard[data-theme=signal]')
    check('  a pick of Signal from Ember mounts it',
          poll(lambda: b.page("!!document.getElementById('signal')")))
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      sheets: [...document.querySelectorAll('link[data-sheet]')].map((l) => l.dataset.sheet),
      ember: !!document.querySelector('#screen, .fx'),
      buttons: document.querySelectorAll('#fx-button').length,
      menu: document.querySelector('meta[name=theme-color]').content })""")
    check('  and leaves nothing of Ember',
          on == {'theme': 'signal', 'sheets': ['signal'], 'ember': False, 'buttons': 1,
                 'menu': '#0f0d0b'}, on)
    check('  no page error after the switch', errors(b) == [], errors(b)[:3])


def the_line(b, d):
    forget(b, d)
    load(b, d, route=True)
    ids = b.page(HOPS)
    check('the_line_shows_every_hop_in_order: player, computer, the device, its amp, its headphones',
          ids == ['player', 'computer', 'device:Gungnir%202', 'device:Amp', 'hop:HD 650'], ids)
    names = b.page("[...document.querySelectorAll('#sg-line [role=option] b')].map((e) => e.textContent)")
    check('  with the device\'s name (and the caret that opens the list), the amp\'s and the headphones\'',
          names[2:] == ['Gungnir 2 \u25BE', 'Amp', 'HD 650'], names)
    xs = b.page("(() => { const l = document.getElementById('sg-line').getBoundingClientRect();"
                " return [...document.querySelectorAll('#sg-line [role=option]')]"
                ".map((o) => { const r = o.getBoundingClientRect(); return (r.left + r.width / 2 - l.left) / l.width; }); })()")
    check('  spread evenly between 10 % and 90 % of the line\'s width',
          abs(xs[0] - 0.1) < 0.01 and abs(xs[-1] - 0.9) < 0.01
          and max(abs((xs[i + 1] - xs[i]) - (xs[1] - xs[0])) for i in range(len(xs) - 1)) < 0.01, xs)
    shape = b.page("""(() => { const q = (s) => document.querySelector(s);
      const band = q('#sg-line').getBoundingClientRect(); const box = q('.sg-box').getBoundingClientRect();
      const dot = q('.sg-d-device.sg-shown').getBoundingClientRect(); const lab = q('.sg-hop b').getBoundingClientRect();
      return { band: band.height, box: [box.width, box.height, box.top - band.top],
        dot: dot.top + dot.height / 2 - band.top, label: lab.top - band.top,
        role: q('#sg-line').getAttribute('role'), orient: q('#sg-line').getAttribute('aria-orientation'),
        notch: getComputedStyle(q('.sg-box'), '::after').content }; })()""")
    check('  the band is 140 px, the box 200 by 104 at y 30, the dots on y 60, the names from y 74',
          shape['band'] == 140 and shape['box'] == [200, 104, 30]
          and abs(shape['dot'] - 60) <= 1 and 72 <= shape['label'] <= 80, shape)
    check('  it is one horizontal listbox with a notch on the box',
          shape['role'] == 'listbox' and shape['orient'] == 'horizontal'
          and shape['notch'] in ('""', '\'\''), shape)
    check('  the device is chosen on load, the box under it',
          b.page(CHOSEN) == 'device:Gungnir%202' and b.page(
              "(() => { const o = document.querySelector('[aria-selected=true]').getBoundingClientRect();"
              " const x = document.querySelector('.sg-box').getBoundingClientRect();"
              " return Math.abs((o.left + o.width / 2) - (x.left + x.width / 2)) < 1; })()"))
    stray = b.page(f"[...document.querySelectorAll('#signal [style]{CSSOM}')].map((e) => e.className || e.id)")
    check('  no style attribute in what Signal drew but on the elements it positions through the CSSOM',
          stray == [] and b.page("document.querySelectorAll('#signal svg [style]').length") == 0, stray)
    check('  no page error', errors(b) == [], errors(b)[:3])


CLEAR = """(() => { const hit = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; };
  const meets = (a, c) => a[0] < c[2] && c[0] < a[2] && a[1] < c[3] && c[1] < a[3];
  const box = hit(document.querySelector('.sg-box'));
  const btn = document.getElementById('fx-button');
  return { box, button: btn ? hit(btn) : null, onButton: !!btn && meets(box, hit(btn)),
    plus: !!document.getElementById('sg-add') }; })()"""


def the_last_box_clears_the_buttons(b, d):
    """The chosen last hop's box (200 px) does not meet the menu button, from 720 to 1600 px; the page
    has no floating + any more (Add device is a row of the menu)."""
    for width in (720, 760, 800, 860, 900, 960, 1024, 1100, 1280, 1440, 1600):
        b.resize(width, 800)
        time.sleep(0.5)
        hop_click(b, b.page("[...document.querySelectorAll('#sg-line [role=option]')].pop().dataset.hop"))
        time.sleep(0.4)
        got = b.page(CLEAR)
        check(f'the_last_box_clears_the_buttons: at {width} px the chosen last hop\'s box does not meet the menu button, and there is no + on the page',
              got['button'] is not None and not got['plus'] and not got['onButton'], got)
    hop_click(b, 'device:Gungnir%202')
    b.resize(1280, 900)


def rings_and_dots(b, d):
    forget(b, d)
    load(b, d, route=True)
    got = b.page("""({ rings: document.querySelectorAll('#sg-line .sg-ring').length,
      dots: [...document.querySelectorAll('#sg-line .sg-dot')].map((c) => c.getAttribute('class')),
      decl: [...document.querySelectorAll('#sg-line .sg-decl')].map((o) => o.dataset.hop),
      words: document.querySelector('[data-hop="hop:HD 650"] .sg-w').textContent,
      italic: getComputedStyle(document.querySelector('[data-hop="hop:HD 650"] .sg-w')).fontStyle })""")
    check('declared_hops_are_rings_and_live_ones_dots: one ring (the headphones), four dots',
          got['rings'] == 1 and len(got['dots']) == 4 and got['decl'] == ['hop:HD 650'], got)
    check('  the ring\'s words are the kind and the note, in italic',
          got['words'] == 'your headphones · on the desk' and got['italic'] == 'italic', got)
    colours = b.page("""(() => { const f = (s) => getComputedStyle(document.querySelector(s)).fill;
      return [f('.sg-d-player'), f('.sg-d-computer'), f('.sg-d-device.sg-shown'), f('.sg-ring')]; })()""")
    check('  green for the player, yellow for the computer, amber for the device, the ground in the ring',
          colours == ['rgb(124, 194, 126)', 'rgb(234, 210, 122)', 'rgb(240, 167, 58)', 'rgb(15, 13, 11)'],
          colours)
    check('  no page error', errors(b) == [], errors(b)[:3])


def choosing(b, d):
    forget(b, d)
    load(b, d, route=True)
    hop_click_chosen(b, 'hop:HD 650')
    got = b.page("""(() => { const o = document.querySelector('[data-hop="hop:HD 650"]').getBoundingClientRect();
      const x = document.querySelector('.sg-box').getBoundingClientRect();
      return { chosen: document.querySelector('[aria-selected=true]').dataset.hop,
        count: document.querySelectorAll('[aria-selected=true]').length,
        centred: Math.abs((o.left + o.width / 2) - (x.left + x.width / 2)) < 1,
        open: document.getElementById('sg-open').dataset.hop }; })()""")
    check('a_click_moves_the_box: a click on a hop chooses it, the box and the open area follow',
          got == {'chosen': 'hop:HD 650', 'count': 1, 'centred': True, 'open': 'hop:HD 650'}, got)
    click_el(b, '#sg-line [data-hop=player] .sg-d-player, #sg-line [data-hop=player] b')
    check('  a click on the player chooses it', b.page(CHOSEN) == 'player', b.page(CHOSEN))
    # a click on the dot's own place chooses too
    xy = b.page("""(() => { const c = document.querySelector('.sg-d-computer').getBoundingClientRect();
      return [c.left + c.width / 2, c.top + c.height / 2]; })()""")
    b.click(xy[0], xy[1])
    check('  and a click on a hop\'s dot', b.page(CHOSEN) == 'computer', b.page(CHOSEN))
    check('  no page error', errors(b) == [], errors(b)[:3])

    hop_click(b, 'device:Gungnir%202')
    b.press('ArrowRight')
    first = b.page(CHOSEN)
    focus = b.page("document.activeElement.dataset.hop")
    b.press('ArrowRight')
    second = b.page(CHOSEN)
    b.press('End')
    last = b.page(CHOSEN)
    b.press('ArrowRight')
    stay = b.page(CHOSEN)
    b.press('ArrowLeft')
    back = b.page(CHOSEN)
    b.press('Home')
    home = b.page(CHOSEN)
    b.press('ArrowLeft')
    check('left_right_move_the_choice: Right goes to the next hop, Left back, Home and End to the ends',
          (first, second, last, stay, back, home, b.page(CHOSEN)) == (
              'device:Amp', 'hop:HD 650', 'hop:HD 650', 'hop:HD 650', 'device:Amp', 'player', 'player'),
          (first, second, last, stay, back, home))
    check('  the focus goes with the choice', focus == first and b.page(
        "document.activeElement.dataset.hop") == 'player', focus)
    tabs = b.page("[...document.querySelectorAll('#sg-line [role=option]')].map((o) => o.tabIndex)")
    check('  one tab stop, on the chosen hop', tabs == [0, -1, -1, -1, -1], tabs)
    check('  no page error', errors(b) == [], errors(b)[:3])


# Each draw of the line goes through `morph`, which sets the innerHTML of one template: counting those
# counts the draws (an emit that the page ignores would not count).
COUNT_DRAWS = """(() => { const d = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
  window.__redraws = 0;
  Object.defineProperty(HTMLTemplateElement.prototype, 'innerHTML', { configurable: true, get: d.get,
    set(v) { window.__redraws += 1; return d.set.call(this, v); } });
  return true; })()"""
REDRAW_BETWEEN = COUNT_DRAWS.replace('return true;', """document.addEventListener('pointerdown',
  () => __core.model.emit('fleet', __core.model.page().fleet), { capture: true, once: true }); return true;""")


def a_draw_between_press_and_release(b, d):
    """The line is drawn again whenever the model changes; a draw between a press and its release must
    not cost the click (the two must land on one element)."""
    for hop in ('computer', 'hop:HD 650'):
        forget(b, d)
        load(b, d, route=True)
        b.page(REDRAW_BETWEEN)
        mark = b.page("(() => { window.__mark = document.querySelector('#sg-line [data-hop=\"%s\"]');"
                      " return true; })()" % hop)
        hop_click(b, hop)
        got = poll(lambda: b.page(CHOSEN) == hop, 3.0)
        check(f'a_draw_between_press_and_release: one click on {hop} chooses it though the line is drawn again '
              'between the press and the release',
              got and b.page('window.__redraws') >= 2
              and b.page(f"window.__mark === document.querySelector('#sg-line [data-hop=\"{hop}\"]')"),
              [b.page(CHOSEN), b.page('window.__redraws')])
    check('  no page error', errors(b) == [], errors(b)[:3])


def leaving(b, d):
    forget(b, d)
    load(b, d, route=True)
    hop_click(b, 'hop:HD 650')
    b.page("""(() => { const page = JSON.parse(JSON.stringify(__core.model.page()));
      page.fleet.hops = page.fleet.hops.filter((h) => h.id !== 'HD 650'); __core.model.replace(page); })()""")
    got = b.page("({ chosen: document.querySelector('[aria-selected=true]').dataset.hop,"
                 " open: document.getElementById('sg-open').dataset.hop, hops: " + HOPS + " })")
    check('a_hop_that_leaves_gives_the_box_back: a declared hop removed from the model gives the box to the device',
          got['chosen'] == 'device:Gungnir%202' and got['open'] == 'device:Gungnir%202'
          and 'hop:HD 650' not in got['hops'], got)
    hop_click(b, 'device:Amp')
    b.page("__core.actions.showDevice('Amp')")
    ok = poll(lambda: b.page(CHOSEN) == 'device:Amp', 8.0)
    check('  a device switch gives the choice to the device shown', ok, [b.page(CHOSEN), b.page(HOPS)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def motion(b, d):
    forget(b, d)
    load(b, d, route=True)
    one = b.page(OFFSET)
    time.sleep(0.4)
    two = b.page(OFFSET)
    check('the_dots_move_while_playing_and_stop_on_pause: the dashes move while the player plays',
          one is not None and two is not None and one != two, [one, two])
    thin = b.page("[...document.querySelectorAll('[data-flow=after]')].length")
    check('  the thin line after the device has its own dashes', thin == 1, thin)
    b.page("""(() => { const host = __core.model.page().host; host.track.status = 'paused';
      __core.model.emit('host', host); })()""")
    one = b.page(OFFSET)
    time.sleep(0.25)
    two = b.page(OFFSET)
    check('  and hold when it is paused', one == two and one is not None, [one, two])
    b.page("""(() => { const host = __core.model.page().host; host.track.status = 'playing';
      __core.model.emit('host', host); })()""")
    one = b.page(OFFSET)
    time.sleep(0.4)
    check('  and move again when it plays', b.page(OFFSET) != one, [one, b.page(OFFSET)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def hidden(b, d):
    forget(b, d)
    load(b, d, route=True)
    time.sleep(0.3)
    b.page(HIDE)
    time.sleep(0.4)
    one = b.page(OFFSET)
    time.sleep(0.6)
    two = b.page(OFFSET)
    check('no_frame_while_hidden: the dashes do not move while the page is hidden',
          one is not None and one == two, [one, two])
    b.page(SHOW)
    time.sleep(0.3)
    one = b.page(OFFSET)
    time.sleep(0.4)
    check('  and move again once it shows', b.page(OFFSET) != one, [one, b.page(OFFSET)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def reduced(d):
    b = make(reduced=True)
    try:
        forget(b, d)
        load(b, d, route=True)
        playing = b.page("__core.model.page().host.track.status")
        one = b.page(OFFSET)
        time.sleep(0.6)
        two = b.page(OFFSET)
        check('the_dots_stand_still_under_reduced_motion: the player plays and the dashes do not move',
              playing == 'playing' and one is not None and one == two, [playing, one, two])
        check('  the line is drawn once, still, with its dashes',
              b.page("document.querySelectorAll('#sg-line [data-flow]').length") >= 2)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


CONTRAST = """() => {
  const num = (c) => c.match(/[\\d.]+/g).map(Number);
  const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92
    : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const grounds = [[15, 13, 11], [23, 19, 15]].map(lum);
  const out = {};
  const seen = (name, el) => { if (!el) { out[name] = null; return; }
    const l = lum(num(getComputedStyle(el).color));
    out[name] = Math.round(Math.min(...grounds.map((g) => (Math.max(l, g) + 0.05) / (Math.min(l, g) + 0.05))) * 100) / 100; };
  seen('name', document.querySelector('.sg-hop:not(.sg-cur):not(.sg-decl) b'));
  seen('chosen name', document.querySelector('.sg-hop.sg-cur b'));
  seen('ring name', document.querySelector('.sg-decl b'));
  seen('words', document.querySelector('.sg-hop:not(.sg-decl) .sg-w'));
  seen('ring words', document.querySelector('.sg-decl .sg-w'));
  seen('ok words', document.querySelector('.sg-ok'));
  seen('warn words', document.querySelector('.sg-warn'));
  const stub = document.getElementById('sg-stub'); seen('stub', stub.textContent ? stub : null);
  return out;
}"""


def contrast():
    red = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET,
                        env={'SVOLDER_FAKE_ROUTE': 'changed'})
    b = make()
    try:
        forget(b, red)
        load(b, red, route=True)
        got = b.page(f'({CONTRAST})()')
        check('contrast_on_the_ground: every text on the ground reads at 4.5 or more',
              all(v is not None and v >= 4.5 for k, v in got.items() if k != 'stub'), got)
        red_off = common.Daemon('gungnir2-v2', host='off')
        try:
            forget(b, red_off)
            load(b, red_off)
            stub = b.page(f'({CONTRAST})()')['stub']
            check('  and the quiet word of a computer not watched', stub is not None and stub >= 4.5, stub)
        finally:
            forget(b, red_off)
            red_off.stop()
    finally:
        forget(b, red)
        b.close()
        red.stop()


DEVICE = 'Gungnir 2'
POSTS = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { window.__posts.push([String(a[0]), String((a[1] || {}).body || '')]);
    return f(...a); }; return 1; })()"""
KNOB = """(() => { const k = document.getElementById('sg-knob').getBoundingClientRect();
  const r = document.getElementById('sg-vtrack').getBoundingClientRect();
  return { at: (k.left + k.width / 2 - r.left) / r.width, left: r.left, width: r.width,
    top: k.top + k.height / 2, cx: k.left + k.width / 2,
    now: document.getElementById('sg-vtrack').getAttribute('aria-valuenow') }; })()"""
POINTS = """(() => { const g = document.getElementById('sg-graph'); g.scrollIntoView({ block: 'center' });
  const r = g.getBoundingClientRect();
  return { graph: [r.left, r.top, r.width, r.height],
    points: [...g.querySelectorAll('.sg-pt')].map((p) => { const q = p.getBoundingClientRect();
      return { band: +p.dataset.band, cx: q.left + q.width / 2, cy: q.top + q.height / 2,
               x: +p.dataset.x, y: +p.dataset.y }; }) }; })()"""


def volume(d):
    return d.model(DEVICE)['volume']


def settled(d, raw, secs=10.0):
    return poll(lambda: volume(d)['raw'] == raw, secs)


def volume_posts(b):
    """The volume writes this page has sent so far: the raw each carried."""
    got = b.page('window.__posts')
    return [json.loads(body)['raw'] for url, body in got if url.endswith('/volume') and body]


def posts_to(b, tail):
    """The bodies this page posted to a route ending in `tail`."""
    return [body for url, body in b.page('window.__posts') if url.endswith(tail)]


def move(b, x, y):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y, 'button': 'left',
                                        'buttons': 1})


def release(b, x, y):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y,
                                        'button': 'left', 'clickCount': 1})


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


def bands(d):
    return d.model(DEVICE)['tone']['readings']['bands']


def points(b):
    """The graph's geometry once two reads in a row agree (the scroll into view has settled)."""
    last = [b.page(POINTS)]
    poll(lambda: last.append(b.page(POINTS)) is None and last[-1] == last[-2], 3.0)
    return last[-1]


def ready(b, d, mode='eq'):
    """The device is in `mode` and the page has drawn it (five points in EQ)."""
    d.post(DEVICE, 'eq/mode', {'mode': mode})
    poll(lambda: d.model(DEVICE)['tone']['mode'] == mode, 8.0)
    poll(lambda: b.page("document.querySelectorAll('#sg-graph .sg-pt').length") == (5 if mode == 'eq' else 0)
         and b.page("document.querySelector('[data-act=mode].on').dataset.mode") == mode, 8.0)


def knob_at(b, raw):
    """The page shows step `raw` on the volume track."""
    return poll(lambda: b.page("document.getElementById('sg-vtrack').getAttribute('aria-valuenow')") == str(raw), 8.0)


def control(d, key):
    return next(c for c in d.model(DEVICE)['controls'] if c['key'] == key)


def chosen(c):
    return next((o['label'] for o in c['options'] if o['on']), None)


def first_screen(b, d):
    forget(b, d)
    load(b, d, route=True)
    got = b.page("""(() => { const o = getComputedStyle(document.getElementById('sg-open'));
      const big = getComputedStyle(document.querySelector('#sg-big .sg-num'));
      const cols = o.gridTemplateColumns.split(' ');
      return { cols: [cols[0], cols.length], gap: o.columnGap,
        size: big.fontSize, weight: big.fontWeight,
        sections: ['sg-volume', 'sg-tone', 'sg-choices'].map((i) => !!document.getElementById(i)),
        inOpen: !!document.querySelector('#sg-open #sg-choices, #sg-open #sg-mode') }; })()""")
    check('the_first_screen_has_h6s_columns: a DAC\'s is the volume (310 px) and the curve, 40 px apart; the volume at 96 px, weight 200;'
          ' the input and the mode are not in it',
          got['cols'][0] == '310px' and got['cols'][1] == 2 and got['gap'] == '40px' and got['size'] == '96px'
          and got['weight'] == '200' and got['sections'] == [True, True, True] and not got['inOpen'], got)
    text = b.page("document.querySelector('#sg-big').textContent")
    model = volume(d)['text']
    check('  the volume reads the device\'s words with the unit', text.replace(' ', '') == model.replace(' ', ''),
          [text, model])
    bar = b.page("(() => { const n = document.getElementById('sg-now').getBoundingClientRect();"
                 " return [n.height > 0, n.bottom <= innerHeight + 1 || scrollY + innerHeight >= "
                 "document.documentElement.scrollHeight - 1]; })()")
    check('  the Now playing bar is at the bottom of the page', bar[0] is True, bar)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_volume_buttons(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    m = volume(d)
    lo, hi = m['lo'], m['hi']
    d.post(DEVICE, 'volume', {'raw': lo + 30})
    settled(d, lo + 30, 10.0)
    knob_at(b, lo + 30)
    click_el(b, '#sg-vup')
    check('minus_and_plus_step_the_volume: `+` raises by one step', settled(d, lo + 31, 10.0), volume(d)['raw'])
    click_el(b, '#sg-vdown')
    check('  `−` lowers by one step', settled(d, lo + 30, 10.0), volume(d)['raw'])
    click_el(b, '#sg-vup')
    click_el(b, '#sg-vup')
    check('  two quick taps raise two steps from the step asked, not the one read',
          settled(d, lo + 32, 10.0), volume(d)['raw'])
    check('  the large number follows the device', poll(lambda: b.page(
        "document.querySelector('#sg-big').textContent").replace(' ', '') == volume(d)['text'].replace(' ', '')),
          [b.page("document.querySelector('#sg-big').textContent"), volume(d)['text']])
    words = b.page("[...document.querySelectorAll('#sg-volume .sg-vrow button')].map((w) => w.textContent)")
    check('the_volume_buttons_are_in_embers_order: the buttons are `−`, `Mute` and `+`',
          words == ['−', 'Mute', '+'], words)
    click_el(b, '#sg-mute')
    check('muted_shows_minus_infinity: `Mute` mutes, reads `Unmute` and the number says `−∞`',
          poll(lambda: volume(d)['muted'] is True)
          and poll(lambda: b.page("document.getElementById('sg-mute').textContent") == 'Unmute')
          and poll(lambda: b.page("document.querySelector('#sg-big .sg-num').textContent") == '\u2212\u221e'),
          [volume(d)['muted'], b.page("document.getElementById('sg-mute').textContent")])
    got = b.page("""(() => { const t = document.getElementById('sg-vtrack');
      return [document.querySelector('#sg-big small').textContent, t.getAttribute('aria-valuetext'),
        document.getElementById('sg-mute').getAttribute('aria-pressed')]; })()""")
    check('  the unit stays `dB`, the slider says `muted` and the button is pressed',
          got == ['dB', 'muted', 'true'], got)
    click_el(b, '#sg-mute')
    check('  and again unmutes', poll(lambda: volume(d)['muted'] is False)
          and poll(lambda: b.page("document.getElementById('sg-mute').textContent") == 'Mute'), volume(d)['muted'])
    # a hold on the focus: the button keeps it through the redraw its own write causes
    b.page("document.getElementById('sg-vup').focus()")
    b.press('Enter')
    settled(d, lo + 33, 10.0)
    knob_at(b, lo + 33)
    check('  a button keeps the focus after its write is read back',
          b.page("document.activeElement.id") == 'sg-vup', b.page("document.activeElement.id"))
    # the keys on the track
    b.page("document.getElementById('sg-vtrack').focus()")
    b.press('ArrowUp')
    b.press('ArrowUp')
    check('  two ArrowUp on the track raise two steps', settled(d, lo + 35, 10.0), volume(d)['raw'])
    b.press('ArrowDown')
    check('  ArrowDown lowers one', settled(d, lo + 34, 10.0), volume(d)['raw'])
    b.press('Home')
    check('  Home goes to the bottom', settled(d, lo, 10.0), volume(d)['raw'])
    aria = b.page("(() => { const n = document.getElementById('sg-vtrack');"
                  " return [n.getAttribute('role'), n.getAttribute('aria-label'), n.getAttribute('aria-valuemin'),"
                  " n.getAttribute('aria-valuemax'), n.tabIndex]; })()")
    check('  the track is a slider in the tab order', aria == ['slider', 'Volume', str(lo), str(hi), 0], aria)
    # a drag along the track writes the steps it passes
    d.post(DEVICE, 'volume', {'raw': lo + 20})
    settled(d, lo + 20, 10.0)
    knob_at(b, lo + 20)
    got = b.page(KNOB)
    drag_xy(b, got['cx'], got['top'], got['left'] + 0.6 * got['width'], got['top'])
    want = min(hi, max(lo, round(lo + 0.6 * (hi - lo))))
    check('  a drag of the knob settles at the step of 60 % of the track', settled(d, want, 10.0),
          [want, volume(d)['raw']])
    check('  no page error', errors(b) == [], errors(b)[:3])


def synth(b, selector, type_, **extra):
    """A pointer event of the primary mouse on the element `selector` (Chromium and Firefox alike)."""
    b.page(f"""(() => {{ const e = document.querySelector('{selector}'); const r = e.getBoundingClientRect();
      e.dispatchEvent(new PointerEvent('{type_}', {{ pointerId: 11, isPrimary: true, pointerType: 'mouse',
        button: 0, buttons: 1, clientX: r.left + 5, clientY: r.top + 5, bubbles: true, cancelable: true }}));
      return 1; }})()""")


def the_hold(b, d):
    """A held `+` repeats after a pause; a release, a blur, a hidden page and the button leaving stop it."""
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    lo = volume(d)['lo']

    def start():
        d.post(DEVICE, 'volume', {'raw': lo + 30})
        settled(d, lo + 30, 10.0)
        knob_at(b, lo + 30)
        b.page('window.__posts.length = 0')

    def count():
        return len(volume_posts(b))

    def stops(how, name):
        start()
        synth(b, '#sg-vup', 'pointerdown')
        time.sleep(0.9)
        went = count()
        how()
        time.sleep(0.3)
        then = count()
        time.sleep(0.6)
        check(name, went >= 3 and count() == then, [went, then, count()])

    start()
    synth(b, '#sg-vup', 'pointerdown')
    time.sleep(0.2)
    first = count()
    time.sleep(0.9)
    synth(b, '#sg-vup', 'pointerup')
    time.sleep(0.3)
    held = count()
    check('holding_plus_repeats: a press steps at once, then after the pause the steps keep coming',
          first == 1 and held >= 4, [first, held])
    time.sleep(0.6)
    check('  a release stops them', count() == held, [held, count()])
    stops(lambda: synth(b, '#sg-vup', 'pointercancel'), '  a cancelled pointer stops them')
    stops(lambda: b.page("window.dispatchEvent(new Event('blur'))"), '  a blur of the window stops them')
    stops(lambda: b.page(HIDE), '  a hidden page stops them')
    b.page(SHOW)
    stops(lambda: b.page("document.getElementById('sg-vup').remove()"),
          '  the button leaving the page stops them')
    load(b, d, route=True)
    b.page(POSTS)
    start()
    synth(b, '#sg-vup', 'pointerdown')
    time.sleep(0.5)
    b.page("(() => { window.__btn = document.getElementById('sg-vup');"
           " __core.model.emit('fleet', __core.model.page().fleet); return 1; })()")
    time.sleep(0.1)
    mid = count()
    time.sleep(0.5)
    same = b.page("window.__btn.isConnected && window.__btn === document.getElementById('sg-vup')")
    menu = b.page("""(() => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      document.getElementById('sg-vup').dispatchEvent(e); return e.defaultPrevented; })()""")
    check('  a redraw in the middle of a hold keeps the button and the steps go on',
          same is True and count() > mid, [same, mid, count()])
    check('  a context menu (a long press on a phone) on a held button is prevented', menu is True, menu)
    synth(b, '#sg-vup', 'pointerup')
    time.sleep(0.3)
    start()
    b.page("document.getElementById('sg-vup').focus()")
    b.press('Enter')
    time.sleep(0.9)
    check('  Enter on the focused button stays one step', count() == 1, count())
    # another hand sets the device back where it was, within moments of the step just asked: the
    # same step asked again is a new ask, not one the page already has
    start()
    b.page("document.getElementById('sg-vup').focus()")
    asked_at = time.time()
    b.press('Enter')
    time.sleep(0.5)
    check('  the step asked before went out', count() == 1, count())
    d.post(DEVICE, 'volume', {'raw': lo + 30})
    settled(d, lo + 30, 10.0)
    time.sleep(0.3)
    b.page('window.__posts.length = 0')
    synth(b, '#sg-vup', 'pointerdown')
    check('  the step asked before is still under 3 s old, as the check needs',
          time.time() - asked_at < 3.0, time.time() - asked_at)
    time.sleep(0.2)
    synth(b, '#sg-vup', 'pointerup')
    time.sleep(0.3)
    check('  a step asked again after another hand set the device back is written', count() == 1, count())
    if browser == 'chromium':
        start()
        x, y = common.center(b, '#sg-vdown')
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y,
                                            'button': 'left', 'clickCount': 1, 'buttons': 1})
        time.sleep(1.0)
        went = count()
        b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y,
                                            'button': 'left', 'clickCount': 1})
        time.sleep(0.3)
        then = count()
        time.sleep(0.5)
        check('  a held mouse on `−` lowers it step after step and a release stops it',
              went >= 4 and count() == then and volume(d)['raw'] < lo + 30, [went, then, count()])
    d.post(DEVICE, 'volume', {'raw': lo + 30})
    start()
    synth(b, '#sg-vup', 'pointerdown')
    time.sleep(0.6)
    went = count()
    b.page("__core.actions.showDevice('Amp')")
    time.sleep(0.3)
    then = count()
    time.sleep(0.6)
    check('  showing another device stops it', went >= 3 and count() == then, [went, then, count()])
    check('  no page error', errors(b) == [], errors(b)[:3])


BLOCK = """(() => { const t = (s) => document.querySelector(s).getBoundingClientRect();
  const w = t('.sg-bandwrap'); const sh = t('#sg-shelf'); const n = t('#sg-now');
  const content = Math.max(...[...document.querySelectorAll('#sg-open > *')]
    .map((e) => e.getBoundingClientRect().bottom));
  return { top: w.top + scrollY, shelfTop: sh.top + scrollY, content: content + scrollY,
    nowBottom: n.bottom + scrollY, doc: document.documentElement.scrollHeight, vh: innerHeight }; })()"""


def a_tall_window(b, d):
    """Up to 1100 px of height the page fills the window as it always did; taller, the line, the open
    area, the shelf and the bar stay together as one block, centred in it."""
    forget(b, d)
    load(b, d, route=True)
    poll(lambda: b.page("!!document.getElementById('sg-facts')"), 10.0)
    got = {}
    for height in (1000, 1100, 2000):
        b.resize(1280, height)
        time.sleep(0.3)
        got[height] = b.page(BLOCK)
    one = got[1000]
    check('the_page_fills_a_window_up_to_1100: at 1280 by 1000 the line is at the top, the bar at the '
          'bottom of the window or the page, nothing centred',
          one['top'] == 0 and one['doc'] >= one['vh'] and one['nowBottom'] == one['vh'], one)
    gap = lambda g: g['shelfTop'] - g['content']  # noqa: E731
    tall = got[2000]
    check('a_tall_window_keeps_the_block_together: at 1280 by 2000 the gap between the open area and the shelf '
          'is no larger than at 1100',
          tall['doc'] <= tall['vh'] and gap(tall) <= gap(got[1100]) + 1, [got[1100], tall])
    check('  and the block is centred in the window (the room above equals the room below, within 3 px)',
          tall['top'] > 100 and abs(tall['top'] - (tall['vh'] - tall['nowBottom'])) <= 3, tall)
    b.resize(600, 1800)
    time.sleep(0.3)
    phone = b.page(BLOCK)
    check('  a phone-wide window keeps the page as it is: the line stays at the top',
          phone['top'] == 0, phone)
    b.resize(1280, 900)
    check('  no page error', errors(b) == [], errors(b)[:3])


SHELF_COLS = """(() => { const r = (s) => { const b = document.querySelector(s).getBoundingClientRect();
    return [b.left, b.top, b.right, b.bottom]; };
  return { cols: getComputedStyle(document.querySelector('.sg-cols')).gridTemplateColumns.split(' ').length,
    bands: r('#sg-bands'), mid: r('#sg-mid'), facts: r('#sg-facts'),
    side: document.documentElement.scrollWidth > innerWidth }; })()"""


def the_shelf_has_two_columns(b, d):
    forget(b, d)
    load(b, d, route=True)
    poll(lambda: b.page("!!document.getElementById('sg-facts')"), 10.0)
    for width in (720, 800, 899):
        b.resize(width, 900)
        time.sleep(0.3)
        g = b.page(SHELF_COLS)
        check(f'  at {width} px the shelf stays one column (the band table needs the width)',
              g.get('cols') == 1 and not g.get('side'), g)
    for width in (900, 1100, 1280, 1700):
        b.resize(width, 900)
        time.sleep(0.3)
        g = b.page(SHELF_COLS)
        if 'bands' not in g:
            check(f'the_shelf_has_two_columns: at {width} px the shelf is drawn', False, g)
            continue
        bands, mid, facts = g['bands'], g['mid'], g['facts']
        check(f'the_shelf_has_two_columns: from 900 px, at {width} px the band table and the middle column stand side by side, '
              'the facts below them across the width',
              g['cols'] == 2 and mid[0] >= bands[2] and abs(mid[1] - bands[1]) <= 2
              and abs(facts[0] - bands[0]) <= 1 and abs(facts[2] - mid[2]) <= 1
              and facts[1] >= max(bands[3], mid[3]) - 1 and not g['side'], g)
    b.resize(600, 900)
    time.sleep(0.3)
    check('  a phone-wide window keeps one column', b.page(SHELF_COLS)['cols'] == 1, b.page(SHELF_COLS))
    b.resize(1280, 900)


TOUCH = """(() => { window.__tp = (type, pts, changed) => { const g = document.getElementById('sg-graph');
  const mk = (p) => new Touch({ identifier: p.id, target: g, clientX: p.x, clientY: p.y });
  const touches = pts.map(mk);
  const ev = new TouchEvent(type, { touches, targetTouches: touches, changedTouches: changed.map(mk),
    cancelable: true, bubbles: true });
  g.dispatchEvent(ev); return ev.defaultPrevented; }; return 1; })()"""


def a_pinch(d):
    """Two fingers on the curve set the Q of the band nearest their midpoint: apart widens the bell
    (lowers the Q), together narrows it; the page neither zooms nor scrolls."""
    b = make_phone()
    try:
        forget(b, d)
        load(b, d, route=True)
        ready(b, d)
        b.page(POSTS)
        b.page(TOUCH)
        steps = d.model(DEVICE)['tone_steps']
        got = points(b)
        three = next(p for p in got['points'] if p['band'] == 3)
        y = got['graph'][1] + 8
        q0 = steps['q'][bands(d)[2][2]][0]
        at = lambda half: [{'id': 1, 'x': three['cx'] - half, 'y': y}, {'id': 2, 'x': three['cx'] + half, 'y': y}]  # noqa: E731
        before = bands(d)[2][:2]
        started = b.page(f"window.__tp('touchstart', {json.dumps(at(15))}, {json.dumps(at(15)[1:])})")
        moved = b.page(f"window.__tp('touchmove', {json.dumps(at(30))}, {json.dumps(at(30))})")
        want = nearest_step(steps['q'], q0 / 2)
        check('a_pinch_sets_the_q: two fingers moving apart to twice their distance halve the Q of the band '
              'nearest their midpoint, within a step', poll(lambda: bands(d)[2][2] == want, 8.0),
              [bands(d)[2], want, q0])
        check('  the page neither zooms nor scrolls (the touches are prevented) and the band keeps its '
              'frequency and gain', started is True and moved is True and bands(d)[2][:2] == before,
              [started, moved, bands(d)[2][:2], before])
        b.page(f"window.__tp('touchmove', {json.dumps(at(7))}, {json.dumps(at(7))})")
        want = nearest_step(steps['q'], q0 * 2)
        check('  together to half the distance they started at doubles it (the bell narrows)',
              poll(lambda: bands(d)[2][2] == want, 8.0), [bands(d)[2], want])
        b.page(f"window.__tp('touchend', {json.dumps(at(7)[:1])}, {json.dumps(at(7)[1:])})")
        time.sleep(0.5)
        went = len(posts_to(b, 'eq/bands/3'))
        b.page(f"window.__tp('touchmove', {json.dumps(at(60)[:1])}, {json.dumps(at(60)[:1])})")
        time.sleep(0.6)
        check('  a lifted finger ends it: more movement writes nothing',
              bands(d)[2][2] == want and len(posts_to(b, 'eq/bands/3')) == went,
              [bands(d)[2], want])
        # a finger already on band 2's point, a second one so that the midpoint is near band 3
        two = next(p for p in got['points'] if p['band'] == 2)
        before3 = bands(d)[2][:]
        before2 = bands(d)[1][:]
        q2 = steps['q'][before2[2]][0]
        b.page(POSTS)
        b.page(f"""(() => {{ const p = document.querySelector('.sg-pt[data-band="2"]');
          p.dispatchEvent(new PointerEvent('pointerdown', {{ pointerId: 5, isPrimary: true, pointerType: 'touch',
            clientX: {two['cx']}, clientY: {two['cy']}, bubbles: true, cancelable: true }})); return 1; }})()""")
        far = [{'id': 1, 'x': two['cx'], 'y': two['cy']}, {'id': 2, 'x': three['cx'] + 20, 'y': two['cy']}]
        wide = [far[0], {'id': 2, 'x': three['cx'] + 80, 'y': two['cy']}]
        b.page(f"window.__tp('touchstart', {json.dumps(far)}, {json.dumps(far[1:])})")
        b.page(f"window.__tp('touchmove', {json.dumps(wide)}, {json.dumps(wide)})")
        check('a_pinch_handoff: a finger on band 2\'s point and a second one makes a pinch of band 2 (the picked '
              'band), not of band 3 nearest the midpoint',
              poll(lambda: bands(d)[1][2] != before2[2], 8.0) and bands(d)[2] == before3
              and bands(d)[1][:2] == before2[:2], [bands(d), before2, before3])
        b.page(f"window.__tp('touchend', {json.dumps(wide)}, {json.dumps(wide)})")
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()


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
        knob_at(b, m['lo'] + 10)
        got = b.page(KNOB)
        b.page('window.__posts.length = 0')
        drag_xy(b, got['cx'], got['top'], got['left'] + got['width'] + 40, got['top'])
        posts = volume_posts(b)
        check('  no write goes above the cap', posts != [] and max(posts) <= cap, [cap, posts])
        check('  the model settles at the cap', settled(capped, cap, 10.0), volume(capped)['raw'])
        got = b.page(KNOB)
        check('  and the knob stops at the cap\'s place',
              abs(got['at'] - (cap - m['lo']) / (m['hi'] - m['lo'])) * got['width'] <= 1.5, got)
        zone = b.page("(() => { const z = document.querySelector('.sg-vcap');"
                      " const r = document.getElementById('sg-vtrack').getBoundingClientRect();"
                      " const q = z.getBoundingClientRect(); return (q.left + q.width / 2 - r.left) / r.width; })()")
        check('  the cap is marked on the track where the knob stands',
              abs(zone - got['at']) * got['width'] <= 2, [zone, got['at']])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, capped)
        finally:
            b.close()
            capped.stop()


def the_point_drag(b, d):
    forget(b, d)
    load(b, d)
    ready(b, d)
    b.page(POSTS)
    steps = d.model(DEVICE)['tone_steps']
    got = points(b)
    gx, gy, gw, gh = got['graph']
    check('the_curve_has_a_point_per_band_in_the_browser: five points, five bands',
          sorted(p['band'] for p in got['points']) == [1, 2, 3, 4, 5], got['points'])
    two = next(p for p in got['points'] if p['band'] == 2)
    x1, y1 = two['cx'] + 80, two['cy'] - 30
    drag_xy(b, two['cx'], two['cy'], x1, y1)
    hz = 20 * math.exp((x1 - gx) / gw * math.log(1000))
    db = 13 - ((y1 - gy) / gh * 260) / 10
    want_f = nearest_step(steps['bands'][1]['f'], hz, log=True)
    want_g = nearest_step(steps['gain'], db)
    ok = poll(lambda: bands(d)[1][:2] == [want_f, want_g], 8.0)
    check('a_point_drag_sets_a_band: band 2 lands on the steps nearest the release point',
          ok, [bands(d)[1], [want_f, want_g]])
    after = points(b)
    two = next(p for p in after['points'] if p['band'] == 2)
    at_x = gx + 1000 * math.log(steps['bands'][1]['f'][want_f][0] / 20) / math.log(1000) / 1000 * gw
    check('  the point stands where the written steps put it', abs(two['cx'] - at_x) <= 1.5
          and abs(two['cy'] - y1) <= gh / 128 + 2, [two, at_x, y1])
    check('  no page error', errors(b) == [], errors(b)[:3])


def tone_hold(b, d):
    """A change from elsewhere during a point drag: the held point stays, and after the release the
    newest reading is drawn."""
    forget(b, d)
    load(b, d)
    ready(b, d)
    got = points(b)
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


def the_choices(b, d):
    forget(b, d)
    load(b, d)
    b.page(POSTS)
    options = [o['label'] for o in control(d, 'input')['options']]
    want = next(o for o in options if o != chosen(control(d, 'input')) and o == 'Optical')
    click_el(b, f'[data-act=choice][data-key=input][data-value={want}]')
    check('an_input_chip_sets_the_input: `Optical` is written to the input',
          poll(lambda: chosen(control(d, 'input')) == want, 8.0), chosen(control(d, 'input')))
    check('  by one write: the input chip under the band table is wired once',
          len(posts_to(b, 'controls/input')) == 1, posts_to(b, 'controls/input'))
    on = poll(lambda: b.page("[...document.querySelectorAll('[data-act=choice][data-key=input].on')]"
                             ".map((w) => w.dataset.value)") == [want], 5.0)
    check('  and the chip that is on moves to it', on)
    click_el(b, '[data-act=choice][data-key=input][data-value="USB 1"]')
    poll(lambda: chosen(control(d, 'input')) == 'USB 1', 8.0)
    # the mode chips
    click_el(b, '[data-act=mode][data-mode=flat]')
    check('  the mode chip `Flat` sets the tone mode',
          poll(lambda: d.model(DEVICE)['tone']['mode'] == 'flat', 8.0), d.model(DEVICE)['tone']['mode'])
    check('  and the chip that is on follows', poll(lambda: b.page(
        "[...document.querySelectorAll('[data-act=mode].on')].map((w) => w.dataset.mode)") == ['flat']))
    click_el(b, '[data-act=mode][data-mode=eq]')
    poll(lambda: d.model(DEVICE)['tone']['mode'] == 'eq', 8.0)
    # the Preset chip is gone: the shelf's preset buttons recall
    check('  there is no Preset chip, and the presets are buttons in the shelf',
          b.page("!document.getElementById('sg-preset') && !document.querySelector('[data-act=preset-list]')"
                 " && document.querySelectorAll('#sg-shelf [data-act=preset]').length > 1"))
    # a mode chip pressed while a change from elsewhere lands is not lost to the redraw
    d.post(DEVICE, 'eq/mode', {'mode': 'flat'})
    time.sleep(1.0)
    click_el(b, '[data-act=mode][data-mode=eq]')
    check('  a mode chip pressed after a change from elsewhere is not lost to the redraw',
          poll(lambda: d.model(DEVICE)['tone']['mode'] == 'eq', 8.0), d.model(DEVICE)['tone']['mode'])
    click_el(b, '#sg-shelf [data-act=preset][data-name=Bass]')
    check('  a preset button recalls `Bass`', poll(lambda: any('Bass' in body for body in posts_to(b, 'presets/recall')), 5.0),
          posts_to(b, 'presets/recall'))
    check('  and the shelf is drawn again from the newest model with it on',
          poll(lambda: b.page("document.querySelector('#sg-shelf [data-act=preset][data-name=Bass]')"
                              ".getAttribute('aria-pressed')") == 'true', 8.0))
    check('  no page error', errors(b) == [], errors(b)[:3])


def guest_tap():
    """A real guest: a daemon whose visitors may read (`guest = "read"`), a browser with no sign-in.
    Every control is drawn inert and a tap on each kind posts nothing."""
    d = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        guest = b.page("__core.model.page().viewer.guest")
        live = b.page("[...document.querySelectorAll('#sg-open button:not([aria-disabled=true]),"
                      " #sg-open [role=slider]:not([aria-disabled=true]), #sg-now button:not([aria-disabled=true]),"
                      " #sg-shelf button:not([aria-disabled=true]):not([data-act=facts-fold]),"
                      " #sg-shelf [role=slider]:not([aria-disabled=true]),"
                      " #sg-shelf input:not([aria-disabled=true])')]"
                      ".map((x) => x.id || x.dataset.act)")
        check('a_guests_tap_posts_nothing: a real guest page draws every control of the first screen inert',
              guest is True and live == [], [guest, live])
        taps = ['#sg-vup', '#sg-vdown', '#sg-mute', '[data-act=choice][data-key=input][data-value=Optical]',
                '[data-act=mode][data-mode=flat]', '#sg-pr-1', '#sg-pt-2']
        missing = [t for t in taps if not b.page(f'!!document.querySelector({json.dumps(t)})')]
        for one in taps:
            if one not in missing:
                click_el(b, one)
        # a guest is not given the player's buttons at all (`buttons` is false), so there is no
        # transport to tap; its drawing is covered by the node check on the guest fixtures
        check('  the guest page has every control to tap', missing == [], missing)
        check('  and no transport button', b.page("document.querySelectorAll('#sg-now [data-act=transport]').length") == 0)
        b.page("document.getElementById('sg-pt-2').focus()")
        b.press('ArrowUp')
        b.page("document.getElementById('sg-vtrack').focus()")
        b.press('ArrowUp')
        # the shelf: a stepper, a typed value, a switch, a preset, the balance's centre and its keys,
        # Identify; and no Save as, Rename or Delete at all
        for tapped in ['#sg-b1gp', '#sg-t-phase', '#sg-pr-1', '#sg-ce-balance', '#sg-identify']:
            tap(b, tapped)
        type_into(b, '#sg-b1f', '100 Hz')
        b.press('Enter')
        b.page("document.getElementById('sg-s-balance').focus()")
        b.press('ArrowRight')
        shelf_writes = b.page("document.querySelectorAll('#sg-preset-save, #sg-preset-rename, #sg-preset-delete,"
                              " #sg-slot-save').length")
        time.sleep(0.6)
        writes = [u for u, body in b.page('window.__posts') if ('api/v1/devices/' in u or 'host/press' in u) and body]
        toast = b.page("document.body.innerText")
        check('  a tap on each kind of control (volume, mute, input, mode, preset, a point) and the keys post nothing',
              writes == [] and 'Sign in' in toast, [writes, toast[-200:]])
        check('  nor does anything in the shelf (a stepper, a typed value, a switch, a preset, the balance,'
              ' Identify), which has no Save as, Rename or Delete', shelf_writes == 0, shelf_writes)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_bar(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    words = b.page("[...document.querySelectorAll('#sg-now [data-act=transport]')]"
                   ".map((w) => [w.dataset.button, w.getAttribute('aria-label')])")
    check('the_bottom_bar_plays_and_pauses: previous, pause and next are the bar\'s buttons',
          words == [['previous', 'Previous'], ['pause', 'Pause'], ['next', 'Next']], words)
    icons = b.page("""[...document.querySelectorAll('#sg-now [data-act=transport]')].map((w) => {
      const v = w.querySelector('svg'); const r = v && v.getBoundingClientRect();
      return [w.textContent.trim(), w.querySelectorAll('svg').length, v && v.getAttribute('aria-hidden'),
        v && getComputedStyle(v).fill === getComputedStyle(w).color, v && v.hasAttribute('style'),
        r && [r.width, r.height]]; })""")
    check('transport_icons_are_drawn_shapes: each of the four is an inline svg of the button\'s colour, 14 px, '
          'aria-hidden, with no text and no style attribute', icons == [
              ['', 1, 'true', True, False, [14, 14]]] * 3, icons)
    track = b.page("document.querySelector('#sg-now .sg-track').textContent")
    want = b.page("__core.model.device().playing.track.headline + ' · ' + __core.model.device().playing.track.artist")
    check('  it names the track and its artist', track == want, [track, want])
    first_offset = b.page(OFFSET)
    check('  the dots move while it plays', poll(lambda: b.page(OFFSET) != first_offset, 3.0))
    click_el(b, '#sg-now [data-act=transport][data-button=next]')
    check('  `next` posts the host\'s press',
          poll(lambda: posts_to(b, 'host/press') == ['{"button":"next"}'], 5.0), posts_to(b, 'host/press'))
    click_el(b, '#sg-now [data-act=transport][data-button=pause]')
    check('  `pause` posts its press and the host model says paused',
          poll(lambda: posts_to(b, 'host/press')[-1:] == ['{"button":"pause"}'], 5.0)
          and poll(lambda: b.page("__core.model.page().host.track.status") == 'paused', 8.0),
          b.page("__core.model.page().host.track.status"))
    check('  the button becomes `Play`', poll(lambda: b.page(
        "[...document.querySelectorAll('#sg-now [data-act=transport]')].map((w) => w.dataset.button)")
        == ['previous', 'play', 'next'], 5.0))
    time.sleep(0.3)
    one = b.page(OFFSET)
    time.sleep(0.4)
    check('  the dots stop with the player paused through the bar', one is not None and b.page(OFFSET) == one,
          [one, b.page(OFFSET)])
    click_el(b, '#sg-now [data-act=transport][data-button=play]')
    check('  `play` brings back `Pause` and the dots start again',
          poll(lambda: b.page("[...document.querySelectorAll('#sg-now [data-act=transport]')]"
                              ".map((w) => w.dataset.button)") == ['previous', 'pause', 'next'], 8.0)
          and poll(lambda: b.page("__core.model.page().host.track.status") == 'playing', 8.0), '')
    one = b.page(OFFSET)
    check('  and the dashes move once more', poll(lambda: b.page(OFFSET) != one, 3.0), [one, b.page(OFFSET)])
    first = b.page("document.querySelector('#sg-now .sg-time').textContent")
    check('  the time on the bar moves on while it plays', poll(lambda: b.page(
        "document.querySelector('#sg-now .sg-time').textContent") != first, 4.0), first)
    check('  no page error', errors(b) == [], errors(b)[:3])


# ---- the shelf, the facts and the slim bar ----

LOKI = 'Loki Max'
SHELF = "document.getElementById('sg-shelf')"


def tap(b, selector):
    """A click on the element, scrolled to the middle of the view first."""
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.15)
    click_el(b, selector)


def type_into(b, selector, text):
    """Put `text` in a value field the way a person typing would leave it, the field focused."""
    b.page(f"(() => {{ const f = document.querySelector({json.dumps(selector)}); f.scrollIntoView({{ block: 'center' }});"
           f" f.focus(); f.value = {json.dumps(text)}; return 1; }})()")


def field(b, selector):
    return b.page(f"document.querySelector({json.dumps(selector)}).value")


def band_row(d, band):
    return d.model(DEVICE)['tone']['bands']['rows'][band - 1]


def the_band_table(b, d):
    forget(b, d)
    load(b, d)
    ready(b, d, 'eq')
    b.page(POSTS)
    cells = b.page("[...document.querySelectorAll('#sg-bands-table tbody tr')].map((r) => "
                   "[...r.querySelectorAll('input.sg-val')].map((i) => i.value))")
    rows = d.model(DEVICE)['tone']['bands']['rows']
    want = [[r['f'], r['g'], r['q']] for r in rows]
    check('the_band_table_steps_each_value: the table has a row for each band with the device\'s own words',
          cells == want, [cells, want])
    for param, word in (('f', 'frequency'), ('g', 'gain'), ('q', 'q')):
        tap(b, f'#sg-b2{param}p')
        tap(b, f'#sg-b2{param}m')
    posts = [json.loads(body) for body in posts_to(b, 'eq/bands/2')]
    check('  each of the three values of band 2 steps up and down by one device step',
          posts == [{'param': w, 'step': st} for w in ('frequency', 'gain', 'q') for st in (1, -1)], posts)
    before = band_row(d, 3)['g']
    type_into(b, '#sg-b3g', '3 dB')
    b.press('Enter')
    check('  a value typed in is written with its unit and the field shows what the device took',
          poll(lambda: band_row(d, 3)['g'] != before, 8.0)
          and poll(lambda: field(b, '#sg-b3g') == band_row(d, 3)['g'], 8.0),
          [before, band_row(d, 3)['g'], field(b, '#sg-b3g')])
    check('  and the body names the value, not a step',
          any(json.loads(x).get('value') == '3 dB' for x in posts_to(b, 'eq/bands/3')), posts_to(b, 'eq/bands/3'))
    sent = len(posts_to(b, 'eq/bands/4'))
    type_into(b, '#sg-b4g', 'abc')
    b.press('Escape')
    check('  Escape puts the old value back and writes nothing',
          field(b, '#sg-b4g') == band_row(d, 4)['g'] and len(posts_to(b, 'eq/bands/4')) == sent,
          [field(b, '#sg-b4g'), band_row(d, 4)['g']])
    # what is being typed is not taken away by a change from elsewhere
    type_into(b, '#sg-b5g', '1.5')
    d.post(DEVICE, 'eq/bands/1', {'param': 'gain', 'step': 1})
    time.sleep(1.0)
    check('  a value being typed stays while another client changes the bands',
          field(b, '#sg-b5g') == '1.5' and b.page("document.activeElement.id") == 'sg-b5g',
          [field(b, '#sg-b5g'), b.page("document.activeElement.id")])
    b.press('Escape')
    shown = poll(lambda: field(b, '#sg-b1g') == band_row(d, 1)['g'], 8.0)
    check('  and it is drawn from the newest model once the field is let go', shown,
          [field(b, '#sg-b1g'), band_row(d, 1)['g']])
    check('  no page error', errors(b) == [], errors(b)[:3])


def asking(b):
    """The form or the question open in the shelf: its text, or None."""
    return b.page("(() => { const a = document.querySelector('#sg-shelf .sg-askbox');"
                  " return a ? a.textContent : null; })()")


def the_presets(b, d):
    forget(b, d)
    load(b, d)
    ready(b, d, 'eq')
    b.page(POSTS)
    tap(b, '#sg-preset-save')
    check('a_preset_saves_renames_and_deletes: Save as opens a form in the shelf with a name field',
          poll(lambda: b.page("!!document.querySelector('#sg-shelf .sg-askbox input')"))
          and b.page("document.activeElement.tagName") == 'INPUT')
    b.press('Escape')
    check('  Escape closes it and writes nothing', asking(b) is None and posts_to(b, 'presets') == [],
          posts_to(b, 'presets'))
    tap(b, '#sg-preset-save')
    poll(lambda: asking(b) is not None)
    type_into(b, '#sg-shelf .sg-askbox input', 'Desk')
    b.press('Enter')
    check('  a name typed and sent saves the preset', poll(lambda: 'Desk' in d.model(DEVICE)['presets']['rows'].__repr__(), 8.0),
          posts_to(b, 'presets'))
    check('  the form closes and the preset is a chip in the shelf',
          poll(lambda: asking(b) is None and b.page("[...document.querySelectorAll('#sg-shelf [data-act=preset]')]"
                                                     ".map((c) => c.dataset.name).includes('Desk')"), 8.0))
    # rename: the preset the bands are on is the one asked about, or a list of the person's own first
    tap(b, '#sg-preset-rename')
    poll(lambda: asking(b) is not None or b.page("!!document.querySelector('.sg-list')"))
    if asking(b) is None:
        click_el(b, '.sg-list .sg-opt[data-value=Desk]')
    check('  Rename asks for a new name, with the old one in the field',
          poll(lambda: field(b, '#sg-shelf .sg-askbox input') == 'Desk'), field(b, '#sg-shelf .sg-askbox input'))
    type_into(b, '#sg-shelf .sg-askbox input', 'Desk 2')
    b.press('Enter')
    check('  and writes the rename', poll(lambda: 'Desk 2' in preset_names(d), 8.0)
          and 'Desk' not in preset_names(d), preset_names(d))
    # delete: asks first; Keep sends nothing
    tap(b, '#sg-preset-delete')
    poll(lambda: asking(b) is not None or b.page("!!document.querySelector('.sg-list')"))
    if asking(b) is None:
        click_el(b, '.sg-list .sg-opt[data-value="Desk 2"]')
    q = poll(lambda: asking(b))
    check('  Delete asks in place, naming the preset, and sends nothing yet',
          q is not None and 'Desk 2' in q and posts_to(b, 'presets/delete') == [], q)
    check('  the safe answer, Keep, has the focus', b.page("document.activeElement.dataset.answer") == 'keep')
    click_el(b, '#sg-shelf .sg-askbox [data-answer=keep]')
    check('  Keep closes it and the preset stays', asking(b) is None and posts_to(b, 'presets/delete') == []
          and 'Desk 2' in preset_names(d))
    tap(b, '#sg-preset-delete')
    poll(lambda: asking(b) is not None or b.page("!!document.querySelector('.sg-list')"))
    if asking(b) is None:
        click_el(b, '.sg-list .sg-opt[data-value="Desk 2"]')
    poll(lambda: asking(b))
    click_el(b, '#sg-shelf .sg-askbox [data-answer=delete]')
    check('  Delete answered deletes it', poll(lambda: 'Desk 2' not in preset_names(d), 8.0)
          and poll(lambda: asking(b) is None), preset_names(d))
    check('  no page error', errors(b) == [], errors(b)[:3])


def preset_names(d):
    return [r['name'] for r in d.model(DEVICE)['presets']['rows']]


def the_shelf_controls(b, d):
    forget(b, d)
    load(b, d)
    ready(b, d, 'eq')
    b.page(POSTS)
    keys = b.page("[...document.querySelectorAll('#sg-shelf [data-key]')].map((e) => e.dataset.key)")
    want = [c['key'] for c in d.model(DEVICE)['controls'] if c['kind'] in ('toggle', 'slider') ]
    check('  the shelf draws the device\'s switches and its balance', all(k in keys for k in want), [keys, want])
    check('  the input is in the choices under the band table, not in the first screen',
          b.page("document.querySelectorAll('#sg-shelf #sg-choices [data-key=input]').length") > 0
          and b.page("document.querySelectorAll('#sg-open [data-key=input]').length") == 0, keys)
    phase = control(d, 'phase')['on']
    tap(b, '#sg-t-phase')
    check('  a switch writes its opposite', poll(lambda: control(d, 'phase')['on'] != phase, 8.0),
          posts_to(b, 'controls/phase'))
    raw = control(d, 'balance')['raw']
    b.page("(() => { const s = document.getElementById('sg-s-balance'); s.scrollIntoView({ block: 'center' });"
           " s.focus(); return 1; })()")
    b.press('ArrowRight')
    check('  a key on the balance scale steps it and the page shows the step at once',
          poll(lambda: control(d, 'balance')['raw'] == raw + 1, 8.0)
          and b.page("document.getElementById('sg-s-balance').getAttribute('aria-valuenow')") == str(raw + 1),
          [control(d, 'balance')['raw'], raw])
    tap(b, '#sg-ce-balance')
    check('  a tap on its value writes the centre', poll(lambda: control(d, 'balance')['raw'] == 64, 8.0),
          control(d, 'balance')['raw'])
    # a drag along the scale
    x0, y0 = common.center(b, '#sg-s-balance')
    r = b.page("(() => { const r = document.getElementById('sg-s-balance').getBoundingClientRect();"
               " return [r.left, r.width]; })()")
    drag_xy(b, x0, y0, r[0] + r[1] * 0.75, y0)
    check('  a drag along the scale writes the step under the pointer',
          poll(lambda: control(d, 'balance')['raw'] > 90, 8.0), control(d, 'balance')['raw'])
    # the loudness strength
    d.post(DEVICE, 'eq/mode', {'mode': 'loudness'})
    check('  the loudness strength has its steppers in Loudness mode',
          poll(lambda: b.page("!!document.getElementById('sg-loudp')"), 8.0))
    tap(b, '#sg-loudp')
    check('  and a tap on `+` writes one step', poll(lambda: [json.loads(x) for x in posts_to(b, 'eq/loudness')]
                                                     == [{'step': 1}], 5.0), posts_to(b, 'eq/loudness'))
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_facts(b, d):
    forget(b, d)
    load(b, d)
    b.page(POSTS)
    got = b.page("""(() => { const t = document.getElementById('sg-facts-fold');
      const body = document.getElementById('sg-facts-body');
      return { expanded: t.getAttribute('aria-expanded'), hidden: body.hidden,
        shown: body.getBoundingClientRect().height > 0 }; })()""")
    check('the_facts_unfold_and_the_choice_survives_a_reload: the facts start folded',
          got == {'expanded': 'false', 'hidden': True, 'shown': False}, got)
    tap(b, '#sg-facts-fold')
    got = b.page("""({ expanded: document.getElementById('sg-facts-fold').getAttribute('aria-expanded'),
      shown: document.getElementById('sg-facts-body').getBoundingClientRect().height > 0,
      stored: localStorage.getItem('svolder.theme.signal.facts'),
      firmware: document.querySelector('[data-fact=Firmware]').textContent })""")
    check('  a tap unfolds them and the choice is stored as `open`',
          got['expanded'] == 'true' and got['shown'] and got['stored'] == '"open"', got)
    check('  they read the device\'s own words', got['firmware'] == b.page("__core.model.device().facts.firmware"), got)
    load(b, d)
    b.page(POSTS)
    check('  a reload finds them open',
          b.page("document.getElementById('sg-facts-body').hidden") is False)
    tap(b, '#sg-identify')
    check('  Identify is under them and posts', poll(lambda: posts_to(b, 'identify') == ['{}'], 5.0),
          posts_to(b, 'identify'))
    tap(b, '#sg-facts-fold')
    check('  a second tap folds them again and stores `closed`',
          b.page("localStorage.getItem('svolder.theme.signal.facts')") == '"closed"')
    load(b, d)
    check('  and a reload keeps them folded', b.page("document.getElementById('sg-facts-body').hidden") is True)
    # a browser that refuses its storage still folds and unfolds
    b.page("(() => { Object.defineProperty(window, 'localStorage', { get() { throw new Error('refused'); } });"
           " return 1; })()")
    b.page("(() => { const t = document.getElementById('sg-facts-fold'); t.click(); return 1; })()")
    check('  without storage it still unfolds',
          b.page("document.getElementById('sg-facts-body').hidden") is False)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_slim_bar(b, d):
    forget(b, d)
    b.resize(1280, 600)
    load(b, d, route=True)
    b.page(POSTS)
    top = b.page("(() => { const s = document.getElementById('sg-slim'); return s.hidden; })()")
    check('the_slim_bar_pins_when_the_line_scrolls_away: at the top of the page the bar is not shown', top is True)
    b.page("window.scrollTo(0, 360)")
    check('  scrolled past the line, it is shown', poll(lambda: b.page("!document.getElementById('sg-slim').hidden"), 5.0))
    got = b.page("""(() => { const s = document.getElementById('sg-slim'); const r = s.getBoundingClientRect();
      const cs = getComputedStyle(s);
      return { top: r.top, height: r.height, width: r.width, vw: innerWidth, pos: cs.position,
        z: Number(cs.zIndex), frame: Number(getComputedStyle(document.getElementById('frame')).zIndex),
        hops: [...s.querySelectorAll('[data-slim]')].map((x) => x.dataset.slim),
        line: [...document.querySelectorAll('#sg-line [data-hop]')].map((x) => x.dataset.hop),
        cur: [...s.querySelectorAll('[aria-current=true]')].map((x) => x.textContent) }; })()""")
    vol = b.page("__core.model.device().volume.text")
    name = b.page("__core.model.shown() && __core.model.page().fleet.devices.find((x) => x.url === __core.model.shown()).name")
    check('  64 px high, across the page at the top, under the frame\'s layer',
          got['top'] == 0 and got['height'] == 64 and got['width'] == got['vw'] and got['pos'] == 'fixed'
          and got['z'] < got['frame'], got)
    check('  the same hops as the line, in its order', got['hops'] == got['line'], got)
    check('  the chosen hop is ringed and carries the DAC\'s volume',
          got['cur'] == [f'{name} · {vol}'], [got['cur'], name, vol])
    b.page("window.scrollTo(0, 0)")
    check('  back at the top it goes away again', poll(lambda: b.page("document.getElementById('sg-slim').hidden"), 5.0))
    check('  no page error', errors(b) == [], errors(b)[:3])
    b.resize(1280, 900)


def the_slim_bar_tap(b, d):
    forget(b, d)
    b.resize(1280, 600)
    load(b, d, route=True)
    b.page("window.scrollTo(0, 360)")
    poll(lambda: b.page("!document.getElementById('sg-slim').hidden"), 5.0)
    click_el(b, '#sg-slim [data-slim=player]')
    check('a_slim_bar_tap_chooses_and_scrolls_up: a tap on the player chooses it',
          poll(lambda: chosen_now(b) == 'player', 5.0), chosen_now(b))
    check('  and the page is back at the top, the bar gone',
          poll(lambda: b.page("scrollY") == 0, 5.0) and poll(lambda: b.page("document.getElementById('sg-slim').hidden"), 5.0),
          b.page("scrollY"))
    check('  the box on the line stands under the player', b.page(
        "document.querySelector('#sg-line [data-hop=player]').getAttribute('aria-selected')") == 'true')
    # a phone has its own strip: no slim bar there
    b.resize(390, 800)
    load(b, d, route=True)
    b.page("window.scrollTo(0, 500)")
    time.sleep(0.4)
    check('  on a phone the bar is not drawn',
          b.page("getComputedStyle(document.getElementById('sg-slim')).display") == 'none')
    b.resize(1280, 900)


def chosen_now(b):
    return b.page(CHOSEN)


def focus_clear(b, d):
    forget(b, d)
    b.resize(1280, 600)
    load(b, d)
    b.page("document.getElementById('sg-facts-fold').click()")
    for last in ("(() => { const all = [...document.querySelectorAll('#sg-shelf button, #sg-shelf input')]"
                 ".filter((e) => e.offsetParent !== null); all[all.length - 1].focus(); return 1; })()",
                 "(() => { document.getElementById('sg-b1f').focus(); return 1; })()"):
        b.page("window.scrollTo(0, 0)")
        b.page(last)
        time.sleep(0.5)
        got = b.page("""(() => { const e = document.activeElement; const r = e.getBoundingClientRect();
          const bar = document.getElementById('sg-now').getBoundingClientRect();
          const slim = document.getElementById('sg-slim');
          const top = slim.hidden ? 0 : slim.getBoundingClientRect().bottom;
          return { id: e.id, top: r.top, bottom: r.bottom, barTop: bar.top, slimBottom: top }; })()""")
        check('  keyboard focus (' + got['id'] + ') is fully visible, clear of the slim bar and above the Now playing bar',
              got['bottom'] <= got['barTop'] and got['top'] >= got['slimBottom'], got)
    b.resize(1280, 900)


AMP = 'Amp'
DAC_HOP = 'device:Gungnir%202'
AMP_HOP = 'device:Amp'


def amp_chosen(b):
    """Choose the linked amp and wait for its area (the stream reads the followed device)."""
    hop_click(b, AMP_HOP)
    return poll(lambda: b.page("!!document.querySelector('#sg-open #sg-power')"), 10.0)


def a_linked_amp(b, d):
    forget(b, d)
    load(b, d, route=True)
    poll(lambda: b.page("__core.stream.followed().length === 1"), 5.0)
    got = b.page("""(() => { const c = [...document.querySelectorAll('#sg-line svg.sg-wire circle')][3];
      return { cls: c.getAttribute('class'), fill: getComputedStyle(c).fill,
        decl: !!document.querySelector('[data-hop="device:Amp"].sg-decl'),
        after: document.querySelectorAll('[data-flow=after]').length,
        followed: __core.stream.followed() }; })()""")
    check('a_linked_amp_is_a_live_dot: a solid amber dot, no ring, the thin stretch before it, the amp followed',
          'sg-dot' in got['cls'] and 'sg-ring' not in got['cls'] and got['fill'] == 'rgb(240, 167, 58)'
          and got['decl'] is False and got['after'] == 1 and got['followed'] == [AMP], got)
    check('  the DAC is not followed, only the linked amp',
          DAC_HOP.split(':')[1] not in b.page("__core.stream.followed()"))
    check('  choosing it draws its area', amp_chosen(b))
    area = b.page("""(() => { const o = document.getElementById('sg-open');
      const dev = __core.model.device('Amp');
      return { cols: ['sg-amp-power', 'sg-amp-meters', 'sg-amp-about'].map((i) => !!document.getElementById(i)),
        power: document.getElementById('sg-power').textContent, word: dev.power.word,
        scales: [...o.querySelectorAll('.sg-mscale')].map((s) => ({ key: s.dataset.key,
          zone: s.querySelector('.sg-zone').style.width, at: s.querySelector('.sg-needle').dataset.at,
          left: s.querySelector('.sg-needle').style.left })),
        want: dev.meters.meters.map((x) => [x.key, (x.class_a * 100).toFixed(2) + '%', x.fraction]),
        shelfPower: document.querySelectorAll('#sg-shelf #sg-power').length,
        powers: document.querySelectorAll('#sg-power').length,
        unlink: !!document.getElementById('sg-amp-unlink'), identify: !!document.getElementById('sg-amp-identify'),
        volumeScreen: !!document.getElementById('sg-volume') }; })()""")
    check('  power, meters and the amp itself, with Unlink and Identify, and no DAC screen',
          area['cols'] == [True, True, True] and area['power'] == area['word'] and area['unlink'] and area['identify']
          and area['volumeScreen'] is False, area)
    check('  the class A share is shaded and the needles stand at the readings',
          [[x['key'], float(x['zone'].rstrip('%'))] for x in area['scales']]
          == [[w[0], float(w[1].rstrip('%'))] for w in area['want']]
          and all(abs(float(x['left'].rstrip('%')) - float(x['at']) * 100) < 0.01 for x in area['scales'] if x.get('at')),
          area)
    check('  one power switch on the page', area['powers'] == 1 and area['shelfPower'] == 0, area)
    check('  no page error', errors(b) == [], errors(b)[:3])


def amp_writes(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    check('  the amp is chosen', amp_chosen(b))
    was = d.model(AMP)['power']['on']
    tap(b, '#sg-power')
    check('an_amp_control_writes_to_the_amps_url: the power switch writes the opposite to the amp\'s route',
          poll(lambda: d.model(AMP)['power']['on'] is not was, 8.0), d.model(AMP)['power'])
    posts = b.page('window.__posts')
    check('  and every request names the amp, none the DAC',
          len(posts) >= 1 and all('/devices/Amp/' in u for u, _ in posts if '/devices/' in u)
          and not any('Gungnir' in u for u, _ in posts), posts)
    # on: the meters read, and the needles move in place to the readings (not by a new drawing)
    b.page("(() => { window.__needle = document.querySelector('#sg-open .sg-needle'); return 1; })()")
    read = poll(lambda: b.page("document.querySelector('#sg-open .sg-needle:not(.off)') !== null"), 15.0)
    got = b.page("""(() => { const n = document.querySelector('#sg-open .sg-needle:not(.off)');
      const m = __core.model.device('Amp').meters.meters[0];
      return n && { left: parseFloat(n.style.left), want: m.fraction * 100, same: n === window.__needle,
        text: document.querySelector('[data-meter=' + m.key + ']').textContent === m.text }; })()""")
    check('  the amp on, its needles stand at the readings and move in place',
          read and got and abs(got['left'] - got['want']) < 0.5 and got['text'], got)
    tap(b, '#sg-power')
    poll(lambda: d.model(AMP)['power']['on'] is was, 8.0)
    poll(lambda: b.page("!!document.getElementById('sg-pt-power_meters')"), 5.0)
    tap(b, '#sg-pt-power_meters')
    tap(b, '#sg-amp-identify')
    check('  the switch and Identify too',
          poll(lambda: any(u.endswith('/devices/Amp/identify') for u, _ in b.page('window.__posts')), 5.0)
          and any('/devices/Amp/controls/power_meters' in u for u, _ in b.page('window.__posts')),
          b.page('window.__posts'))
    posts = b.page('window.__posts')
    check('  never a request to the DAC\'s route',
          not any('Gungnir' in u for u, _ in posts) and len(posts) >= 3, posts)
    check('  no page error', errors(b) == [], errors(b)[:3])


def dac_volume(b, d):
    forget(b, d)
    load(b, d, route=True)
    got = b.page("""({ own: !!document.querySelector('[data-hop="device:Gungnir%202"] .sg-v') })""")
    check('the_dac_keeps_its_volume_written_while_the_amp_is_chosen: with the DAC chosen its screen has the volume',
          got == {'own': False}, got)
    check('  the amp is chosen', amp_chosen(b))
    label = lambda: b.page("document.querySelector('[data-hop=\"device:Gungnir%202\"] .sg-v')?.textContent")
    want = b.page("__core.model.device().volume.text")
    check('  the DAC\'s label carries its volume', label() == want and want, [label(), want])
    m = volume(d)
    raw = m['lo'] + 40 if volume(d)['raw'] != m['lo'] + 40 else m['lo'] + 41
    d.post(DEVICE, 'volume', {'raw': raw})
    check('  and follows it', poll(lambda: label() == volume(d)['text'] and settled(d, raw, 1.0), 10.0),
          [label(), volume(d)['text']])
    slim = b.page("document.querySelector('#sg-slim [data-slim=\"device:Gungnir%202\"]').textContent")
    check('  the slim bar says the same', slim.endswith(volume(d)['text']), slim)
    hop_click(b, DAC_HOP)
    check('  the DAC chosen again, the label gives the volume to its screen',
          poll(lambda: label() is None, 5.0), label())
    check('  no page error', errors(b) == [], errors(b)[:3])


def dacs_name(b, d):
    forget(b, d)
    load(b, d, route=True)
    name = f'#sg-line [data-hop="{DAC_HOP}"] b'
    click_el(b, f'#sg-line [data-hop="{DAC_HOP}"] b')
    pop = b.page("document.querySelector('[data-hop=\"device:Gungnir%202\"]').getAttribute('aria-haspopup')")
    check('  the chosen DAC\'s name shows a caret and says it opens a list',
          pop == 'listbox' and b.page("!!document.querySelector('[data-hop=\"device:Gungnir%202\"] b .sg-more')"), pop)
    got = b.page("""({ list: [...document.querySelectorAll('.sg-list .sg-opt')].map((o) => [o.textContent, o.dataset.value, o.className.includes('on')]),
      chosen: """ + CHOSEN + """ })""")
    check('the_dacs_name_opens_the_device_list: a tap on the name of the chosen DAC lists the devices, the shown one marked',
          got['list'] == [['Gungnir 2', 'Gungnir%202', True], ['Amp', 'Amp', False]] and got['chosen'] == DAC_HOP, got)
    b.press('Escape')
    check('  Escape closes it', poll(lambda: b.page("document.querySelectorAll('.sg-list').length === 0"), 3.0))
    click_el(b, name)
    b.page("""(() => { const page = JSON.parse(JSON.stringify(__core.model.page()));
      __core.model.emit('fleet', { ...page.fleet, devices: page.fleet.devices.filter((x) => x.url !== 'Amp') }); })()""")
    check('  a change of the fleet closes it, so it never lists a device that left',
          b.page("document.querySelectorAll('.sg-list').length") == 0)
    load(b, d, route=True)
    click_el(b, name)
    hop_click(b, 'player')
    check('  choosing another hop closes it', b.page("document.querySelectorAll('.sg-list').length") == 0)
    hop_click(b, DAC_HOP)
    hop_click(b, AMP_HOP)
    click_el(b, f'#sg-line [data-hop="{DAC_HOP}"] b')
    got = b.page("({ lists: document.querySelectorAll('.sg-list').length, chosen: " + CHOSEN + " })")
    check('  the first tap on the DAC while another hop is chosen chooses it, it opens no list',
          got == {'lists': 0, 'chosen': DAC_HOP}, got)
    click_el(b, f'#sg-line [data-hop="{DAC_HOP}"] b')
    click_el(b, '.sg-list .sg-opt[data-value=Amp]')
    check('  a pick shows that device', poll(lambda: b.page("__core.model.shown()") == AMP, 8.0)
          and poll(lambda: b.page(CHOSEN) == AMP_HOP, 5.0), [b.page("__core.model.shown()"), b.page(CHOSEN)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def amp_leaves(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    check('  the amp is chosen', amp_chosen(b))
    tap(b, '#sg-amp-unlink')
    check('a_chosen_amp_that_leaves_the_fleet_gives_the_box_back_to_the_dac: a question open for it',
          poll(lambda: b.page("!!document.querySelector('#sg-amp-about .sg-askbox')"), 3.0))
    b.page("""(() => { const page = JSON.parse(JSON.stringify(__core.model.page()));
      page.fleet.devices = page.fleet.devices.filter((x) => x.url !== 'Amp');
      page.fleet.hops = page.fleet.hops.map((h) => h.device === 'Amp' ? { ...h, live: false } : h);
      __core.model.emit('fleet', page.fleet); })()""")
    got = b.page("""({ chosen: """ + CHOSEN + """, open: document.getElementById('sg-open').dataset.hop,
      ask: !!document.querySelector('.sg-askbox'), power: !!document.getElementById('sg-power'),
      followed: __core.stream.followed(), hops: """ + HOPS + """ })""")
    check('  the box goes to the DAC, the area is the DAC\'s, the question is gone, the amp is not followed',
          got['chosen'] == DAC_HOP and got['open'] == DAC_HOP and got['ask'] is False and got['power'] is False
          and got['followed'] == [] and AMP_HOP not in got['hops'], got)
    check('  nothing was written to the gone amp',
          not any('/devices/Amp/' in u or '/hops/' in u for u, _ in b.page('window.__posts')), b.page('window.__posts'))
    check('  no page error', errors(b) == [], errors(b)[:3])


def amp_faults():
    """An amp whose model carries over-temperature and over-current rows but no power part
    (the `fake-amp` profile): the area draws the faults, and no power switch."""
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=[('fake-amp', 'Amp', '1112131415161718')])
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        hop_click(b, AMP_HOP)
        poll(lambda: b.page("!!document.getElementById('sg-amp-about')"), 10.0)
        got = b.page("""({ faults: [...document.querySelectorAll('#sg-amp-about .sg-faults dt')].map((x) => x.textContent),
          values: [...document.querySelectorAll('#sg-amp-about .sg-faults dd')].map((x) => x.textContent),
          rows: __core.model.device('Amp').facts.rows.filter((r) => /^Over-/.test(r.label)).map((r) => r.label + '=' + r.value),
          power: !!document.getElementById('sg-power') })""")
        check('an_amp_opens_power_meters_and_faults: an amp with fault rows draws each, with the device\'s words',
              got['faults'] == ['Over-current', 'Over-temperature'] and got['values'] == ['clear', 'clear']
              and sorted(got['rows']) == ['Over-current=clear', 'Over-temperature=clear'] and got['power'] is False, got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def unlink():
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        amp_chosen(b)
        tap(b, '#sg-amp-unlink')
        name = b.page("__core.model.device('Amp').name")
        row = b.page("__core.model.page().fleet.hops.find((h) => h.device === 'Amp').id")
        got = b.page("""({ answers: [...document.querySelectorAll('#sg-amp-about .sg-askbox [data-answer]')]
        .map((x) => x.dataset.answer), words: document.querySelector('#sg-amp-about .sg-question')?.textContent,
        posts: window.__posts.filter(([u]) => u.includes('hops')).length })""")
        check('unlink_removes_the_hop_and_the_amp_stands_alone: Unlink asks once, in place, and writes nothing yet',
              got['answers'] == ['unlink', 'keep'] and name in got['words'] and got['posts'] == 0, got)
        tap(b, '#sg-amp-about .sg-askbox [data-answer=keep]')
        got = b.page("({ ask: !!document.querySelector('.sg-askbox'), hops: window.__posts.filter(([u]) => u.includes('hops')).length,"
                     " chosen: " + CHOSEN + " })")
        check('  Keep closes it, nothing posted, the amp still chosen',
              got == {'ask': False, 'hops': 0, 'chosen': AMP_HOP}, got)
        tap(b, '#sg-amp-unlink')
        tap(b, '#sg-amp-about .sg-askbox [data-answer=unlink]')
        check('  Unlink posts the hop\'s removal and the hop leaves the line',
              poll(lambda: AMP_HOP not in b.page(HOPS), 8.0), b.page(HOPS))
        posts = [(u, bd) for u, bd in b.page('window.__posts') if u.endswith('hops/remove')]
        check('  once, by the hop\'s id', len(posts) == 1 and json.loads(posts[0][1]) == {'id': row}, [posts, row])
        got = b.page("({ chosen: " + CHOSEN + ", hops: " + HOPS + ", followed: __core.stream.followed(),"
                     " rows: __core.model.page().fleet.hops.map((h) => h.device || h.name) })")
        check('  the box is on the DAC, the amp is not followed, the other hop stays',
              got['chosen'] == DAC_HOP and got['followed'] == [] and got['rows'] == ['HD 650'], got)
        b.page("__core.actions.showDevice('Amp')")
        poll(lambda: b.page(CHOSEN) == AMP_HOP, 8.0)
        got = b.page("""({ hops: """ + HOPS + """, ring: document.querySelector('[data-hop=standin-dac]')?.textContent })""")
        check('  shown on its own it stands alone: the ring for the DAC svolder does not drive, then the amp',
              got['hops'] == ['player', 'computer', 'standin-dac', AMP_HOP]
              and got['ring'] == 'a DAC svolder does not drive', got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def guest_amp():
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML.replace('auth = "none"', 'guest = "read"'), fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        poll(lambda: b.page("__core.stream.followed().length === 1"), 5.0)
        check('a_guest_cannot_unlink_or_switch_an_amp: a guest page', b.page("__core.model.page().viewer.guest") is True)
        amp_chosen(b)
        live = b.page("[...document.querySelectorAll('#sg-open button:not([aria-disabled=true]),"
                      " #sg-shelf button:not([aria-disabled=true]):not([data-act=facts-fold])')].map((x) => x.id || x.dataset.act)")
        check('  every control of the amp\'s area is drawn inert', live == [], live)
        for one in ('#sg-power', '#sg-pt-power_meters', '#sg-amp-identify', '#sg-amp-unlink'):
            if b.page(f'!!document.querySelector({json.dumps(one)})'):
                click_el(b, one)
        time.sleep(0.4)
        posts = b.page('window.__posts')
        check('  a tap on each posts nothing and asks no question',
              not any(w in u for u, _ in posts for w in ('controls', 'identify', 'hops'))
              and not b.page("!!document.querySelector('.sg-askbox')"), posts)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def hati_power():
    d = common.Daemon(profile='hati', name='Hati')
    b = make()
    try:
        forget(b, d)
        load(b, d)
        b.page(POSTS)
        was = b.page("__core.model.device().power.on === true")
        tap(b, '#sg-power')
        check('the_power_switch_writes: the shelf of a preamp has its power switch and a tap writes the opposite',
              poll(lambda: [json.loads(x) for x in posts_to(b, 'controls/power')] == [{'value': not was}], 5.0),
              posts_to(b, 'controls/power'))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def loki_shelf():
    d = common.Daemon(profile='lokimax', name=LOKI)
    b = make()
    try:
        forget(b, d)
        load(b, d)
        b.page(POSTS)
        shape = b.page("""({ slots: [...document.querySelectorAll('#sg-shelf [data-act=slot]')].map((x) => x.textContent),
          built: [...document.querySelectorAll('#sg-shelf [data-act=setting]')].map((x) => x.textContent),
          tone: !!document.getElementById('sg-bands-table'),
          sliders: document.querySelectorAll('#sg-shelf .sg-scale').length })""")
        check('a_slot_save_asks_first: a Loki Max shelf has its three slots, five settings and no band table',
              shape['slots'] == ['Preset 1', 'Preset 2', 'Preset 3'] and len(shape['built']) == 5
              and shape['tone'] is False and shape['sliders'] >= 6, shape)
        tap(b, '#sg-slot-save')
        poll(lambda: b.page("document.querySelectorAll('.sg-list .sg-opt').length") == 3)
        # an open list stays through a change from elsewhere
        d.post(LOKI, 'controls/band1', {'raw': 4})
        time.sleep(1.0)
        check('an_open_list_survives_a_part_update: the list of slots stays open while another client moves a band',
              b.page("document.querySelectorAll('.sg-list .sg-opt').length") == 3
              and b.page("document.getElementById('sg-slot-save').isConnected"))
        click_el(b, '.sg-list .sg-opt[data-value="2"]')
        q = poll(lambda: asking(b))
        check('  a slot chosen asks first and sends nothing', q is not None and 'Preset 2' in q
              and posts_to(b, 'device-presets/save') == [], [q, posts_to(b, 'device-presets/save')])
        d.post(LOKI, 'controls/band1', {'raw': 8})
        b.page("__core.model.emit('host', __core.model.page().host)")
        b.page("__core.model.emit('fleet', __core.model.page().fleet)")
        time.sleep(1.0)
        check('  the question stays while a part changes and a host and a fleet event come',
              asking(b) is not None and 'Preset 2' in asking(b))
        click_el(b, '#sg-shelf .sg-askbox [data-answer=keep]')
        check('  Keep sends nothing', posts_to(b, 'device-presets/save') == [] and poll(lambda: asking(b) is None))
        check('  and the shelf then shows the newest band',
              poll(lambda: b.page("document.getElementById('sg-s-band1').getAttribute('aria-valuenow')") == '8', 8.0),
              b.page("document.getElementById('sg-s-band1').getAttribute('aria-valuenow')"))
        tap(b, '#sg-slot-save')
        poll(lambda: b.page("document.querySelectorAll('.sg-list .sg-opt').length") == 3)
        click_el(b, '.sg-list .sg-opt[data-value="2"]')
        poll(lambda: asking(b))
        click_el(b, '#sg-shelf .sg-askbox [data-answer=save]')
        saved = poll(lambda: posts_to(b, 'device-presets/save'), 5.0)
        check('  Save answered writes the slot with its confirmation',
              [json.loads(x) for x in saved] == [{'slot': 2, 'confirm': True}], saved)
        tap(b, '#sg-sl-1')
        check('  a slot tapped is recalled with no question',
              poll(lambda: [json.loads(x) for x in posts_to(b, 'device-presets/recall')] == [{'slot': 1}], 5.0),
              posts_to(b, 'device-presets/recall'))
        tap(b, '#sg-st-1')
        check('  and a built-in setting is applied',
              poll(lambda: [json.loads(x) for x in posts_to(b, 'device-presets/apply')] == [{'name': 'Bass Boost'}], 5.0),
              posts_to(b, 'device-presets/apply'))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()



def no_host():
    d = common.Daemon('gungnir2-v2', host='off')
    b = make()
    try:
        forget(b, d)
        load(b, d)
        got = b.page("""({ hops: """ + HOPS + """, stub: document.getElementById('sg-stub').textContent,
          chosen: """ + CHOSEN + """ })""")
        check('no_host_starts_the_line_at_the_device: the line is the device alone, the quiet word beside it',
              got == {'hops': ['device:Gungnir%202'], 'stub': 'the computer is not watched',
                      'chosen': 'device:Gungnir%202'}, got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        forget(b, d)
        b.close()
        d.stop()


def alone():
    d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, '?theme=signal&device=Amp')
        got = b.page("""({ hops: """ + HOPS + """, chosen: """ + CHOSEN + """,
          ring: document.querySelector('[data-hop=standin-dac]')?.textContent,
          grey: document.querySelectorAll('#sg-line .sg-grey-dot').length,
          flow: document.querySelectorAll('[data-flow=before]').length,
          greyLine: document.querySelectorAll('#sg-line .sg-grey').length })""")
        # the daemon reads no route for an amp's page: the computer stays, greyed, with no facts
        check('an_amp_no_hop_links_stands_alone: the player and the computer greyed, the ring, then the amp',
              got['hops'] == ['player', 'computer', 'standin-dac', 'device:Amp']
              and got['chosen'] == 'device:Amp' and got['ring'] == 'a DAC svolder does not drive'
              and got['grey'] == 2 and got['flow'] == 0 and got['greyLine'] == 1, got)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        forget(b, d)
        b.close()
        d.stop()


def linked_amp_alone():
    """A linked amp shown on its own page (`?device=Amp`): the greyed way in, the DAC its row names, then the amp."""
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, '?theme=signal&device=Amp')
        got = b.page("""({ hops: """ + HOPS + """, chosen: """ + CHOSEN + """,
          words: [...document.querySelectorAll('[data-hop=computer] .sg-w')].length,
          dac: document.querySelector('[data-hop="device:Gungnir%202"]')?.textContent,
          dacDot: document.querySelectorAll('#sg-line .sg-wire .sg-dot.sg-d-device').length,
          greyDots: document.querySelectorAll('#sg-line .sg-grey-dot').length,
          greyLine: document.querySelectorAll('#sg-line .sg-grey').length,
          before: document.querySelectorAll('[data-flow=before]').length,
          after: document.querySelectorAll('#sg-line .sg-after[data-flow=after]').length,
          ring: !!document.querySelector('[data-hop=standin-dac]') })""")
        check('a_linked_amp_shown_alone_has_the_dac_before_it: the greyed way in, the DAC, then the amp, no ring',
              got['hops'] == ['player', 'computer', DAC_HOP, AMP_HOP] and got['chosen'] == AMP_HOP
              and got['ring'] is False and got['dac'].startswith('Gungnir 2'), got)
        check('  the way in to the DAC is grey and does not flow, the thin stretch runs from the DAC to the amp',
              got['greyLine'] == 1 and got['greyDots'] == 2 and got['before'] == 0 and got['after'] == 1
              and got['dacDot'] == 2, got)
        check('  the computer says nothing of a route on a page that holds none', got['words'] == 0, got)
        b.page("document.querySelector('#sg-line [data-hop=\"' + " + repr(AMP_HOP) + " + '\"]').focus()")
        b.press('ArrowLeft')
        got = b.page("({ shown: __core.model.shown(), chosen: " + CHOSEN + ", focus: document.activeElement.dataset.hop, "
                     "button: document.getElementById('sg-via-show')?.textContent })")
        check('  an ArrowLeft onto the DAC boxes it in place: the page stays on the amp, focus and box agree, '
              'the area offers Show <DAC>',
              got["shown"] == AMP and got['chosen'] == DAC_HOP and got['focus'] == DAC_HOP
              and (got['button'] or '').startswith('Show Gungnir 2'), got)
        b.press('Enter')
        check('  Enter on it shows the DAC, as the device list does',
              poll(lambda: b.page("__core.model.shown()") == 'Gungnir%202', 8.0)
              and poll(lambda: b.page(CHOSEN) == DAC_HOP, 5.0), [b.page("__core.model.shown()"), b.page(CHOSEN)])
        load(b, d, '?theme=signal&device=Amp')
        hop_click(b, DAC_HOP)
        check('  a click on the DAC hop shows that device, as the device list does',
              poll(lambda: b.page("__core.model.shown()") == 'Gungnir%202', 8.0)
              and poll(lambda: b.page(CHOSEN) == DAC_HOP, 5.0), [b.page("__core.model.shown()"), b.page(CHOSEN)])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        forget(b, d)
        b.close()
        d.stop()


def middle_layout():
    """From 720 px to where H6's tone column reaches 300 px the curve (and an amp's meters) have a
    full-width row of their own: drawable at 768 and 900 px, a point can be dragged, and H6 is whole
    from 1020 px."""
    d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET)
    try:
        for width in (768, 900, 1020):
            b = make(size=(width, 900))
            try:
                forget(b, d)
                load(b, d, route=True)
                ready(b, d)
                got = b.page("""(() => { const r = (id) => document.getElementById(id).getBoundingClientRect();
                  const cols = getComputedStyle(document.getElementById('sg-open')).gridTemplateColumns.split(' ').length;
                  const g = document.getElementById('sg-graph').getBoundingClientRect();
                  return { cols, graph: g.width, volumeBottom: r('sg-volume').bottom, toneTop: r('sg-tone').top,
                    volumeTop: r('sg-volume').top, toneLeft: r('sg-tone').left,
                    scrollsSideways: document.documentElement.scrollWidth > innerWidth }; })()""")
                if width < 1020:
                    check(f'the_curve_has_a_row_of_its_own_in_the_middle_layout: at {width} px the curve is 300 px or wider, '
                          'under the volume', got['cols'] == 2 and got['graph'] >= 300
                          and got['toneTop'] >= got['volumeBottom']
                          and not got['scrollsSideways'], got)
                    b.page(POSTS)
                    steps = d.model(DEVICE)['tone_steps']
                    pts = points(b)
                    two = next(p for p in pts['points'] if p['band'] == 2)
                    before = bands(d)[1][:2]
                    drag_xy(b, two['cx'], two['cy'], two['cx'] + 60, two['cy'] - 30)
                    check(f'  at {width} px a point can be dragged: band 2 is written',
                          poll(lambda: bands(d)[1][:2] != before, 8.0), [before, bands(d)[1], steps['bands'][1]['f'][:3]])
                else:
                    check('  at 1020 px the first screen is the volume and the curve, two columns, the curve 300 px or wider on the volume\'s row',
                          got['cols'] == 2 and got['graph'] >= 300 and abs(got['toneTop'] - got['volumeTop']) <= 1, got)
                load(b, d, '?theme=signal&device=Amp')
                poll(lambda: b.page("!!document.getElementById('sg-amp-meters')"), 10.0)
                meters = b.page("""(() => { const m = document.querySelector('#sg-amp-meters .sg-mscale');
                  const open = document.getElementById('sg-open').getBoundingClientRect();
                  const box = document.getElementById('sg-amp-meters').getBoundingClientRect();
                  const power = document.getElementById('sg-amp-power').getBoundingClientRect();
                  return { count: document.querySelectorAll('#sg-amp-meters .sg-mscale').length,
                    scale: m ? m.getBoundingClientRect().width : 0, column: box.width, below: box.top >= power.bottom,
                    inside: box.right <= open.right + 1 && box.left >= open.left - 1 }; })()""")
                if width < 1020:
                    check(f'  at {width} px the amp\'s meters are 300 px or wider, in a row under its power',
                          meters['count'] > 0 and meters['scale'] >= 300 and meters['below'] and meters['inside'], meters)
                else:
                    check('  at 1020 px the amp\'s meters column is 300 px or wider, beside its power',
                          meters['count'] > 0 and meters['column'] >= 300 and not meters['below'], meters)
                check('  no page error', errors(b) == [], errors(b)[:3])
            finally:
                try:
                    forget(b, d)
                finally:
                    b.close()
    finally:
        d.stop()


# ---- declared hops: the + sheet, Edit, Move, Remove ----

DECL = ('auth = "none"\n\n[[hop]]\nname = "HD 650"\nkind = "headphones"\nafter = "Gungnir 2"\nnote = "on the desk"\n\n'
        '[[hop]]\nname = "Cans"\nkind = "other"\nafter = "Gungnir 2"\n')
HD, CANS = 'hop:HD 650', 'hop:Cans'
SHEET_OPEN = "!!document.querySelector('#sg-sheet:not([hidden]) form')"
EDIT_OPEN = "!!document.querySelector('#sg-hop-edit form')"
ACTIVE = "document.activeElement && document.activeElement.id"


def hop_posts(b):
    """The hop writes this page has posted: (route tail, body as sent)."""
    return [(u.split('api/v1/hops')[1], json.loads(body or '{}'))
            for u, body in b.page('window.__posts') if 'api/v1/hops' in u]


def cli_hops(d):
    return d.cli('hops', 'list')


def choose_hop(b, hop):
    hop_click(b, hop)
    return poll(lambda: b.page(CHOSEN) == hop, 5.0)


def plus_sheet():
    d = common.Daemon('gungnir2-v2', host='fake', web=DECL, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        check('  there is no + on the page: the menu has the row Add device…, a live one',
              b.page("!document.getElementById('sg-add')") and (lambda: (click_el(b, '.fxbtn'), poll(lambda: b.page(
                  "!!document.getElementById('menu-add-device')"), 3.0), b.page(
                  "(() => { const r = document.getElementById('menu-add-device');"
                  " return r.textContent.trim() === 'Add device…' && r.getAttribute('aria-disabled') !== 'true'; })()"),
                  b.press('Escape'))[2])())
        add_device(b)
        got = b.page("""({ open: """ + SHEET_OPEN + """, title: (document.getElementById('sg-sheet-title') || {}).textContent,
          chips: [...document.querySelectorAll('#sg-sheet [data-act=hop-kind]')].map((x) => x.textContent + '|' + x.getAttribute('aria-checked')),
          name: document.getElementById('sg-add-name').disabled, focus: document.activeElement.textContent })""")
        check('the_plus_sheet_adds_a_declared_hop: the sheet, "After the Gungnir 2", the connected amp first and chosen',
              got['open'] and got['title'] == 'After the Gungnir 2' and got['name'] is True
              and got['chips'] == ['Amp (connected)|true', 'Amp|false', 'Headphones|false', 'Speakers|false', 'Other|false']
              and got['focus'] == 'Amp (connected)', got)
        INERT = "[...document.querySelectorAll('.sg-bandwrap, #sg-open, #sg-shelf, #sg-now')].map((x) => x.inert)"
        check('  the page behind the sheet is inert while it stands', b.page(INERT) == [True] * 4, b.page(INERT))
        tap(b, '#sg-sheet [data-kind=speakers]')
        got = b.page("({ value: document.getElementById('sg-add-name').value, off: document.getElementById('sg-add-name').disabled })")
        check('  a kind chosen after the connected amp clears its name and frees the field', got == {'value': '', 'off': False}, got)
        tap(b, '#sg-sheet [data-device]')
        got = b.page("({ value: document.getElementById('sg-add-name').value, off: document.getElementById('sg-add-name').disabled })")
        check('  the amp again: its own name, the field off', got == {'value': 'Amp', 'off': True}, got)
        b.press('Escape')
        check('  Escape lifts it', b.page(INERT) == [False] * 4, b.page(INERT))
        add_device(b)
        b.press('Escape')
        check('  Escape closes it, the focus returns to the menu button, nothing posted',
              not b.page(SHEET_OPEN) and b.page(ACTIVE) == 'fx-button' and hop_posts(b) == [], [b.page(ACTIVE), hop_posts(b)])
        add_device(b)
        tap(b, '#sg-sheet [data-act=sheet-cancel]')
        check('  Cancel closes it too, nothing posted', not b.page(SHEET_OPEN) and hop_posts(b) == [])
        # what is typed is kept through readings from elsewhere
        add_device(b)
        tap(b, '#sg-sheet [data-kind=speakers]')
        type_into(b, '#sg-add-name', 'Monitors')
        type_into(b, '#sg-add-note', 'left pair')
        b.page("(() => { const m = __core.model; m.emit('host', m.page().host); m.emit('fleet', m.page().fleet);"
               " m.emit('part', { device: m.shown(), name: 'playing' }); return 1; })()")
        time.sleep(0.4)
        got = b.page("({ open: " + SHEET_OPEN + ", name: (document.getElementById('sg-add-name') || {}).value,"
                     " note: (document.getElementById('sg-add-note') || {}).value,"
                     " kind: (document.querySelector('#sg-sheet [aria-checked=true]') || {}).dataset.kind })")
        check('  a reading from elsewhere leaves the typed fields alone', got == {
            'open': True, 'name': 'Monitors', 'note': 'left pair', 'kind': 'speakers'}, got)
        tap(b, '#sg-sheet-go')
        check('  Add: the hop is on the line, chosen, the sheet closed',
              poll(lambda: 'hop:Monitors' in b.page(HOPS), 8.0) and poll(lambda: b.page(CHOSEN) == 'hop:Monitors', 5.0)
              and not b.page(SHEET_OPEN), [b.page(HOPS), b.page(CHOSEN)])
        check('  it posted the name, the kind, the note and the DAC it follows',
              hop_posts(b) == [('', {'name': 'Monitors', 'kind': 'speakers', 'note': 'left pair', 'after': 'Gungnir 2'})],
              hop_posts(b))
        got = b.page("({ name: (document.querySelector('#sg-hop-main .sg-hopname') || {}).textContent,"
                     " dashed: document.querySelector('.sg-box').classList.contains('sg-boxdecl'),"
                     " ring: !!document.querySelector('#sg-line [data-hop=\"hop:Monitors\"].sg-decl') })")
        check('  its open area shows it, the box dashed round a ring',
              got == {'name': 'Monitors', 'dashed': True, 'ring': True}, got)
        listed = cli_hops(d)
        check('  `svolder hops list` (the same config, temp homes) shows it',
              'Monitors' in listed and 'speakers' in listed and 'left pair' in listed, listed)
        # a refused add leaves the sheet open with what was typed
        add_device(b)
        tap(b, '#sg-sheet [data-kind=amp]')
        type_into(b, '#sg-add-name', 'HD 650')
        tap(b, '#sg-sheet-go')
        time.sleep(0.6)
        got = b.page("({ open: " + SHEET_OPEN + ", name: (document.getElementById('sg-add-name') || {}).value,"
                     " off: document.getElementById('sg-sheet-go').disabled })")
        check('  a refused add (a name taken) keeps the sheet open with the typed name, ready again',
              got == {'open': True, 'name': 'HD 650', 'off': False}, got)
        check('  (the refusal is a 422 the page logs, nothing else)',
              [e for e in errors(b) if '422' not in e] == [], errors(b)[:3])
        tap(b, '#sg-sheet [data-act=sheet-cancel]')
        # a name with a slash and a space goes through whole
        b.page('window.__posts = []')
        add_device(b)
        tap(b, '#sg-sheet [data-kind=other]')
        type_into(b, '#sg-add-name', 'Left/Right Cans')
        tap(b, '#sg-sheet-go')
        check('  a name with a slash and a space is added whole and chosen',
              poll(lambda: b.page(CHOSEN) == 'hop:Left/Right Cans', 8.0)
              and hop_posts(b) == [('', {'name': 'Left/Right Cans', 'kind': 'other', 'after': 'Gungnir 2'})]
              and b.page(INERT) == [False] * 4 and 'Left/Right Cans' in cli_hops(d), [b.page(CHOSEN), hop_posts(b)])
        # a connected amp is linked instead
        b.page('window.__posts = []')
        add_device(b)
        got = b.page("({ chips: [...document.querySelectorAll('#sg-sheet [data-act=hop-kind]')][0].textContent,"
                     " name: document.getElementById('sg-add-name').value })")
        check('  the connected amp is offered first, with its own name', got == {'chips': 'Amp (connected)', 'name': 'Amp'}, got)
        tap(b, '#sg-sheet-go')
        check('  Add links it: a live amp on the line, followed by the stream',
              poll(lambda: AMP_HOP in b.page(HOPS), 8.0) and poll(lambda: b.page(CHOSEN) == AMP_HOP, 5.0)
              and poll(lambda: b.page("__core.stream.followed()") == ['Amp'], 5.0), [b.page(HOPS), b.page(CHOSEN)])
        check('  it posted the device and the DAC it follows, no name or kind',
              hop_posts(b) == [('', {'device': 'Amp', 'after': 'Gungnir 2'})], hop_posts(b))
        check('  `svolder hops list` shows it linked', 'Amp' in cli_hops(d), cli_hops(d))
        add_device(b)
        chips = b.page("[...document.querySelectorAll('#sg-sheet [data-act=hop-kind]')].map((x) => x.textContent)")
        check('  a linked amp is not offered again', chips == ['Amp', 'Headphones', 'Speakers', 'Other'], chips)
        b.press('Escape')
        check('  no page error but the refusal\'s 422', [e for e in errors(b) if '422' not in e] == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def edit_move_remove():
    d = common.Daemon('gungnir2-v2', host='fake', web=DECL, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        choose_hop(b, HD)
        tap(b, '#sg-hop-edit-open')
        got = b.page("""({ name: (document.getElementById('sg-edit-name') || {}).value, note: (document.getElementById('sg-edit-note') || {}).value,
          kind: (document.querySelector('#sg-hop-edit [aria-checked=true]') || {}).dataset.kind, focus: """ + ACTIVE + """ })""")
        check('edit_changes_the_name_and_note: Edit opens the sheet\'s fields in place, filled, the name focused',
              got == {'name': 'HD 650', 'note': 'on the desk', 'kind': 'headphones', 'focus': 'sg-edit-name'}, got)
        b.press('Escape')
        check('  Escape closes it, the focus returns to Edit, nothing posted',
              not b.page(EDIT_OPEN) and b.page(ACTIVE) == 'sg-hop-edit-open' and hop_posts(b) == [], [b.page(ACTIVE), hop_posts(b)])
        tap(b, '#sg-hop-edit-open')
        type_into(b, '#sg-edit-name', 'HD 660')
        type_into(b, '#sg-edit-note', '')
        b.page("(() => { const m = __core.model; m.emit('host', m.page().host); m.emit('fleet', m.page().fleet); return 1; })()")
        time.sleep(0.4)
        check('  a reading from elsewhere leaves the form alone',
              b.page(EDIT_OPEN) and b.page("document.getElementById('sg-edit-name').value") == 'HD 660')
        tap(b, '#sg-edit-go')
        check('  Save: the box follows the renamed hop, the area shows the new name and no note',
              poll(lambda: b.page(CHOSEN) == 'hop:HD 660', 8.0) and not b.page(EDIT_OPEN)
              and b.page("document.querySelector('#sg-hop-main .sg-hopname').textContent") == 'HD 660'
              and b.page("document.querySelector('#sg-hop-main .sg-hopnote').textContent") == 'no note',
              [b.page(CHOSEN), b.page(HOPS)])
        check('  it posted the id, the new name, the kind and no note',
              hop_posts(b) == [('/edit', {'id': 'HD 650', 'name': 'HD 660', 'kind': 'headphones', 'note': None})], hop_posts(b))
        listed = cli_hops(d)
        check('  `svolder hops list` shows the new name', 'HD 660' in listed and 'HD 650' not in listed, listed)
        # move earlier
        b.page('window.__posts = []')
        choose_hop(b, CANS)
        got = b.page("({ earlier: document.getElementById('sg-hop-earlier').dataset.to, earlierOff: document.getElementById('sg-hop-earlier').disabled,"
                     " later: document.getElementById('sg-hop-later').disabled })")
        check('move_earlier_reorders_the_line: Cans is second: Move earlier goes to place 0, Move later is off',
              got == {'earlier': '0', 'earlierOff': False, 'later': True}, got)
        tap(b, '#sg-hop-earlier')
        check('  the line reorders, Cans stays chosen',
              poll(lambda: b.page(HOPS)[-2:] == [CANS, 'hop:HD 660'], 8.0) and b.page(CHOSEN) == CANS, [b.page(HOPS), b.page(CHOSEN)])
        check('  it posted the whole list\'s place', hop_posts(b) == [('/move', {'id': 'Cans', 'to': 0})], hop_posts(b))
        got = b.page("({ earlier: document.getElementById('sg-hop-earlier').disabled, later: document.getElementById('sg-hop-later').dataset.to,"
                     " words: document.querySelector('#sg-hop-main .sg-note').textContent })")
        check('  now it is first: Move earlier is off, Move later is place 1, the place is said',
              got['earlier'] is True and got['later'] == '1' and 'It sits after the Gungnir 2, before the HD 660.' in got['words'], got)
        listed = cli_hops(d)
        check('  `svolder hops list` has them in that order', 0 <= listed.find('Cans') < listed.find('HD 660'), listed)
        # remove
        b.page('window.__posts = []')
        tap(b, '#sg-hop-remove')
        got = b.page("({ answers: [...document.querySelectorAll('#sg-hop-main .sg-askbox [data-answer]')].map((x) => x.dataset.answer),"
                     " words: (document.querySelector('#sg-hop-main .sg-question') || {}).textContent })")
        check('remove_asks_then_removes: Remove asks once, in place, and writes nothing yet',
              got['answers'] == ['remove', 'keep'] and 'Cans' in got['words'] and hop_posts(b) == [], got)
        tap(b, '#sg-hop-main .sg-askbox [data-answer=keep]')
        check('  Keep closes it, nothing posted, the hop still chosen',
              not b.page("!!document.querySelector('.sg-askbox')") and hop_posts(b) == [] and b.page(CHOSEN) == CANS)
        tap(b, '#sg-hop-remove')
        tap(b, '#sg-hop-main .sg-askbox [data-answer=remove]')
        check('  Remove posts the removal and the box goes back to the DAC',
              poll(lambda: CANS not in b.page(HOPS), 8.0) and poll(lambda: b.page(CHOSEN) == DAC_HOP, 5.0)
              and hop_posts(b) == [('/remove', {'id': 'Cans'})], [b.page(HOPS), b.page(CHOSEN), hop_posts(b)])
        check('  the DAC\'s first screen is back', b.page("!!document.getElementById('sg-volume')"))
        check('  `svolder hops list` no longer has it', 'Cans' not in cli_hops(d), cli_hops(d))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


REMOVE_ELSEWHERE = """(() => { const f = JSON.parse(JSON.stringify(__core.model.page().fleet));
  f.hops = f.hops.filter((h) => h.id !== %s); __core.model.emit('fleet', f); return 1; })()"""


def removed_elsewhere():
    d = common.Daemon('gungnir2-v2', host='fake', web=DECL, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        choose_hop(b, HD)
        tap(b, '#sg-hop-edit-open')
        type_into(b, '#sg-edit-note', 'half typed')
        b.page(REMOVE_ELSEWHERE % json.dumps('HD 650'))
        got = b.page("({ chosen: " + CHOSEN + ", form: " + EDIT_OPEN + ", volume: !!document.getElementById('sg-volume'),"
                     " hops: " + HOPS + " })")
        check('a_chosen_hop_removed_elsewhere_gives_the_box_back: with its Edit form open, the box is on the DAC, '
              'the form gone, the DAC\'s screen drawn',
              got['chosen'] == DAC_HOP and got['form'] is False and got['volume'] is True and HD not in got['hops'], got)
        choose_hop(b, CANS)
        tap(b, '#sg-hop-remove')
        check('  a Remove question is open', b.page("!!document.querySelector('#sg-hop-main .sg-askbox')"))
        b.page(REMOVE_ELSEWHERE % json.dumps('Cans'))
        got = b.page("({ chosen: " + CHOSEN + ", ask: !!document.querySelector('.sg-askbox'), main: !!document.getElementById('sg-hop-main') })")
        check('  with its question open: the box on the DAC, the question gone',
              got == {'chosen': DAC_HOP, 'ask': False, 'main': False}, got)
        check('  nothing was written', hop_posts(b) == [], hop_posts(b))
        check('  the page works after: the menu row still opens its sheet', (lambda: (add_device(b), b.page(SHEET_OPEN))[1])())
        b.press('Escape')
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def linked_note():
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        amp_chosen(b)
        tap(b, '#sg-hop-edit-open')
        got = b.page("({ text: document.getElementById('sg-hop-edit-open').textContent, name: !!document.getElementById('sg-edit-name'),"
                     " kinds: document.querySelectorAll('#sg-hop-edit [data-act=hop-kind]').length,"
                     " note: !!document.getElementById('sg-edit-note'), remove: !!document.getElementById('sg-hop-remove') })")
        check('a_linked_hop_edits_only_its_note: a linked amp\'s Edit has the note only (no name, no kind, no Remove: Unlink is its own)',
              got == {'text': 'Edit the note', 'name': False, 'kinds': 0, 'note': True, 'remove': False}, got)
        type_into(b, '#sg-edit-note', 'left channel')
        tap(b, '#sg-edit-go')
        check('  Save keeps the amp chosen and writes the note alone',
              poll(lambda: b.page("(__core.model.page().fleet.hops.find((h) => h.device === 'Amp') || {}).note") == 'left channel', 8.0)
              and b.page(CHOSEN) == AMP_HOP and hop_posts(b) == [('/edit', {'id': 'Amp', 'note': 'left channel'})],
              [b.page(CHOSEN), hop_posts(b)])
        check('  `svolder hops list` shows the note', 'left channel' in cli_hops(d), cli_hops(d))
        got = b.page("({ later: document.getElementById('sg-hop-later').dataset.to, earlier: document.getElementById('sg-hop-earlier').disabled })")
        check('  a linked amp moves too: first of two, it can go later (place 1)', got == {'later': '1', 'earlier': True}, got)
        tap(b, '#sg-hop-later')
        check('  moved: the amp is after HD 650, still chosen',
              poll(lambda: b.page(HOPS)[-2:] == ['hop:HD 650', AMP_HOP], 8.0) and b.page(CHOSEN) == AMP_HOP, b.page(HOPS))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def guest_hops():
    d = common.Daemon('gungnir2-v2', host='fake', web=DECL.replace('auth = "none"', 'guest = "read"'), fleet=FLEET)
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        b.page(POSTS)
        check('a_guest_cannot_add_or_edit_a_hop: a guest page', b.page("__core.model.page().viewer.guest") is True)
        add_device(b)
        time.sleep(0.4)
        check('  the menu\'s Add device… opens no sheet for a guest, posts nothing and says why',
              not b.page(SHEET_OPEN) and hop_posts(b) == [] and 'Sign in' in b.page('document.body.innerText'),
              [b.page(SHEET_OPEN), hop_posts(b)])
        choose_hop(b, HD)
        live = b.page("[...document.querySelectorAll('#sg-open button:not([aria-disabled=true]):not([disabled])')].map((x) => x.id)")
        check('  every button of the declared hop\'s area is drawn inert', live == [], live)
        b.page('window.__posts = []')
        for one in ('#sg-hop-edit-open', '#sg-hop-earlier', '#sg-hop-later', '#sg-hop-remove'):
            if b.page(f'!!document.querySelector({json.dumps(one)})'):
                click_el(b, one)
        time.sleep(0.4)
        check('  a tap on Edit, Move earlier / later and Remove posts nothing and opens no form or question',
              hop_posts(b) == [] and not b.page(EDIT_OPEN) and not b.page("!!document.querySelector('.sg-askbox')"), hop_posts(b))
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


# ---- the player, the computer and the states (Task 7) ----

NOW_HIDDEN = "document.getElementById('sg-now').hidden"
PLAYER_BUTTONS = ("[...document.querySelectorAll('#sg-player [data-act=transport]')]"
                  ".map((w) => w.dataset.button)")


def the_player(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    check('the_player_transport_works_when_chosen: with the DAC chosen the bottom bar is drawn',
          b.page(NOW_HIDDEN) is False and b.page("!!document.querySelector('#sg-now .sg-np')"))
    hop_click(b, 'player')
    check('  choosing the player opens Now playing in the open area',
          poll(lambda: b.page("!!document.getElementById('sg-player')")) and b.page(CHOSEN) == 'player')
    check('  and the bottom bar hides', b.page(NOW_HIDDEN) is True
          and b.page("document.getElementById('sg-now').children.length") == 0)
    headline = b.page("__core.model.page().host.track.headline")
    got = b.page("""({ title: document.querySelector('#sg-player .sg-title').textContent,
      path: [...document.querySelectorAll('#sg-player .sg-pathlist li b')].map((x) => x.textContent),
      verdict: document.querySelector('#sg-player .sg-verdict')?.textContent,
      want: __core.model.device().playing.path.words })""")
    check('  it names the track and puts the path in words with its verdict',
          got['title'] == headline and got['path'] == ['player', 'computer', 'DAC'] and got['verdict'] == got['want'], got)
    check('  previous, pause and next are its buttons', b.page(PLAYER_BUTTONS) == ['previous', 'pause', 'next'])
    first = b.page(OFFSET)
    check('  the dots keep moving while the player plays', poll(lambda: b.page(OFFSET) != first, 3.0))
    click_el(b, '#sg-player [data-act=transport][data-button=next]')
    check('  `next` posts the host\'s press', poll(lambda: posts_to(b, 'host/press') == ['{"button":"next"}'], 5.0),
          posts_to(b, 'host/press'))
    click_el(b, '#sg-player [data-act=transport][data-button=pause]')
    check('  `pause` posts its press, the model says paused and the button becomes `Play`',
          poll(lambda: posts_to(b, 'host/press')[-1:] == ['{"button":"pause"}'], 5.0)
          and poll(lambda: b.page("__core.model.page().host.track.status") == 'paused', 8.0)
          and poll(lambda: b.page(PLAYER_BUTTONS) == ['previous', 'play', 'next'], 5.0), b.page(PLAYER_BUTTONS))
    time.sleep(0.3)
    one = b.page(OFFSET)
    time.sleep(0.4)
    check('  the dots stop while it is paused', one is not None and b.page(OFFSET) == one, [one, b.page(OFFSET)])
    click_el(b, '#sg-player [data-act=transport][data-button=play]')
    check('  `play` brings back `Pause`', poll(lambda: b.page(PLAYER_BUTTONS) == ['previous', 'pause', 'next'], 8.0))
    time_was = b.page("document.querySelector('#sg-pp .sg-time').textContent")
    check('  the time in the area moves on while it plays',
          poll(lambda: b.page("document.querySelector('#sg-pp .sg-time').textContent") != time_was, 4.0), time_was)
    hop_click(b, f'device:{DEVICE.replace(" ", "%20")}')
    check('  choosing the DAC brings the bottom bar back and takes the player\'s area away',
          poll(lambda: b.page(NOW_HIDDEN) is False and b.page("!!document.querySelector('#sg-now .sg-np')")
               and not b.page("document.getElementById('sg-player')")))
    check('  no page error', errors(b) == [], errors(b)[:3])


COVER = """(() => { const c = document.querySelector('#sg-now .sg-cover'); const i = c && c.querySelector('img');
  const r = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.right, b.width, b.height]; };
  const np = document.querySelector('#sg-now .sg-np');
  return { has: !!c, hidden: c ? getComputedStyle(c).display === 'none' : null, box: c ? r(c) : null,
    loaded: i ? i.complete && i.naturalWidth > 0 : null, src: i ? i.getAttribute('src') : null,
    abs: i ? i.src : null, alt: i ? i.getAttribute('alt') : null,
    track: np ? r(np.querySelector('.sg-track')) : null, np: np ? r(np) : null,
    transport: np ? r(np.querySelector('.sg-transport')) : null,
    style: !!(c && (c.hasAttribute('style') || c.querySelector('[style]'))) }; })()"""
SWAP = """(() => { const url = __core.model.shown(); const p = structuredClone(__core.model.page().device.playing);
  p.track.cover = %s; p.track.headline = %s;
  __core.model.applyPart(url, 'playing', p); return 1; })()"""


def the_cover(b, d):
    """The cover in the Now playing bar: the fake host's picture, small, before the transport; none
    leaves no box and no gap; another track brings its own address."""
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(COVER)['loaded'], 6.0)
    c = b.page(COVER)
    check('the_bar_shows_the_cover_when_there_is_one: the host\'s picture is there, drawn, 36 CSS px square, '
          'before the transport',
          c['has'] and c['loaded'] and abs(c['box'][2] - 36) <= 2.5 and c['alt'] == ''
          and c['box'][1] <= c['transport'][0] + 0.5, c)
    check('  it is the daemon\'s own, a same-origin address with the version, and the markup has no style attribute',
          (c['src'] or '').startswith('api/v1/host/cover?v=') and (c['abs'] or '').startswith(d.url) and not c['style'], c)
    first = c['src']
    b.page(SWAP % (json.dumps('deadbeefdeadbeef'), json.dumps('Another Song')))
    other = poll(lambda: (b.page(COVER)['src'] or '').endswith('deadbeefdeadbeef') and b.page(COVER), 5.0)
    hidden = poll(lambda: b.page(COVER)['hidden'], 5.0)
    mid = b.page(COVER)
    check('the_cover_follows_the_track: another track\'s picture is asked for by its own address, and one the '
          'daemon does not hold (a real 404) is hidden, its box gone, the transport where the bar begins',
          bool(other) and other['src'] != first and hidden is True and mid['box'][2] == 0
          and abs(mid['transport'][0] - mid['np'][0]) <= 0.5, [first, other and other['src'], mid])
    b.page(SWAP % ('null', json.dumps('No Picture')))
    none = poll(lambda: not b.page(COVER)['has'], 5.0)
    check('  a track with no cover has no box at all and the transport starts the bar',
          none is True and abs(b.page(COVER)['transport'][0] - b.page(COVER)['np'][0]) <= 0.5, b.page(COVER))
    b.page(SWAP % (json.dumps(first.split('v=')[1]), json.dumps('Back')))
    back = poll(lambda: b.page(COVER)['has'] and b.page(COVER)['loaded'] and not b.page(COVER)['hidden'], 5.0)
    check('  and the first picture comes back when the track does', back is True, b.page(COVER))
    hop_click(b, 'player')
    poll(lambda: b.page("!!document.getElementById('sg-player')"))
    area = b.page("""(() => { const c = document.querySelector('#sg-player .sg-cover');
      const i = c && c.querySelector('img'); const t = document.querySelector('#sg-player .sg-title');
      return { has: !!c, loaded: i ? i.complete && i.naturalWidth > 0 : null,
        before: !!c && !!(c.compareDocumentPosition(t) & Node.DOCUMENT_POSITION_FOLLOWING) }; })()""")
    check('  the player\'s own area shows the cover before the title too',
          area['has'] and area['loaded'] and area['before'], area)
    b.page(SWAP % (json.dumps('deadbeefdeadbeef'), json.dumps('Another Song')))
    gone = poll(lambda: b.page("""(() => { const c = document.querySelector('#sg-player .sg-cover');
      return !!c && getComputedStyle(c).display === 'none'; })()"""), 5.0)
    check('  and in the player\'s own area a picture the daemon does not hold (a real 404) is hidden too, its box gone',
          gone is True, b.page("document.querySelector('#sg-player .sg-cover')?.outerHTML"))
    check('  no page error', [e for e in errors(b) if 'cover' not in e and '404' not in e] == [], errors(b)[:3])
    # the 404 of the picture the daemon does not hold was the check's own ask
    if browser == 'chromium':
        b.errors[:] = [e for e in b.errors if 'host/cover' not in e]


def the_cover_on_a_phone(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(COVER)['loaded'], 6.0)
    c = b.page(COVER)
    vw = b.page('innerWidth')
    check('the_bar_shows_the_cover_on_a_phone: 36 px, inside the window, before the transport',
          c['has'] and c['loaded'] and abs(c['box'][2] - 36) <= 2.5 and c['box'][0] >= 0
          and c['box'][1] <= c['transport'][0] + 0.5 and c['np'][1] <= vw + 0.5, [c, vw])


def the_cover_gone():
    """The host's picture goes (a daemon that stops holding it): no box and no gap."""
    g = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    b = make()
    try:
        forget(b, g)
        load(b, g)
        poll(lambda: b.page(COVER)['loaded'], 6.0)
        before = b.page(COVER)
        g.drop_cover()
        gone = poll(lambda: not b.page(COVER)['has'] or b.page(COVER)['hidden'], 14.0)
        after = b.page(COVER)
        check('no_cover_leaves_no_gap: with the cover gone there is no box, and the transport starts where '
              'the bar does',
              before['has'] and gone and abs(after['transport'][0] - after['np'][0]) <= 0.5
              and after['transport'][0] < before['transport'][0] - 20, [before['transport'], after])
        check('  no page error', [e for e in errors(b) if 'cover' not in e and '404' not in e] == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


def the_computer(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    hop_click_chosen(b, 'computer')
    check('the_computer_hop_has_no_control: the computer opens its facts',
          poll(lambda: b.page("!!document.getElementById('sg-computer')")) and b.page(CHOSEN) == 'computer')
    got = b.page("""(() => { const r = __core.model.page().host.route;
      const rows = Object.fromEntries([...document.querySelectorAll('#sg-computer dt')].map((t) => [t.textContent, t.nextElementSibling.textContent]));
      return { rows, sink: r.sink.words, pct: r.sink.volume_pct, eq: r.host_eq, mixing: r.others_mixing }; })()""")
    rows = got['rows']
    check('  the sink, its rate and format, PipeWire\'s volume, mute, the host EQ and the other streams are read from the route',
          rows.get('Sound card') == got['sink'] and rows.get('Volume in PipeWire') == f"{got['pct']} %"
          and 'kHz' in rows.get('Rate and format', '') and rows.get('Muted') in ('yes', 'no')
          and rows.get('Host EQ', '').startswith('on' if got['eq'] else 'off') and 'Other streams' in rows
          and 'The sound’s way' in rows, got)
    check('  the bottom bar stays', b.page(NOW_HIDDEN) is False)
    acting = b.page("""[...document.querySelectorAll('#sg-open *')].filter((e) => e.matches(
      'button, input, select, textarea, a, [tabindex], [role=slider], [data-act]') || e.tabIndex >= 0).length""")
    check('  nothing in it can be focused or acts', acting == 0, acting)
    click_el(b, '#sg-computer dl')
    b.press('Enter')
    time.sleep(0.3)
    writes = [p for p in b.page('window.__posts') if not p[0].endswith(('/model', '/stream'))]
    check('  a click or a key on it sends nothing', writes == [], writes[:3])
    check('  no page error', errors(b) == [], errors(b)[:3])


LOST = "document.querySelector('.sg-lostring')"
TRY = "document.getElementById('sg-try')"


def a_lost_device():
    toml = 'auth = "none"\n\n[[hop]]\nname = "HD 650"\nkind = "headphones"\nafter = "Gungnir 2"\n'
    gone = common.Daemon('gungnir2-v2', host='fake', web=toml)
    b = make()
    try:
        gone.drop()
        forget(b, gone)
        load(b, gone)
        b.page(POSTS)
        check('a_lost_device_shows_try_now_and_the_seconds: the card is in the open area',
              poll(lambda: b.page("!!document.getElementById('sg-card')"), 20.0))
        got = b.page("""({ title: document.querySelector('#sg-card h2').textContent,
          try: document.getElementById('sg-try')?.textContent,
          retry: document.querySelector('#sg-card .sg-retry')?.textContent,
          inOpen: !!document.querySelector('#sg-open #sg-card') })""")
        check('  it says why, with `Try now` and the seconds to the next try',
              got['title'].endswith('is out of reach') and got['try'] == 'Try now' and got['inOpen']
              and (got['retry'] or '').startswith('next try in'), got)
        title = tab_title(b)
        card = b.page("document.querySelector('#sg-card h2').textContent")
        check('  the tab reads the device and the card\'s title, lowercased',
              title == f'Gungnir 2 \u00b7 {card.lower()} \u2014 svolder', [title, card])
        seconds = b.page("document.querySelector('#sg-card .sg-retry').textContent")
        check('  the seconds count down', poll(lambda: b.page("document.querySelector('#sg-card .sg-retry').textContent")
                                               != seconds, 6.0), seconds)
        got = b.page("""({ ring: !!document.querySelector('.sg-lostring.sg-t-bad'),
          dashes: document.querySelectorAll('#sg-line [data-flow=after]').length,
          still: !!document.querySelector('#sg-line .sg-after.sg-still'),
          dimmed: document.querySelectorAll('#sg-line .sg-hop.sg-dimmed').length,
          before: document.querySelectorAll('#sg-line [data-flow=before]').length,
          fill: getComputedStyle(document.querySelector('.sg-lostring')).fill })""")
        check('  the wire ends in a hollow red ring, what comes after it is still and dimmed',
              got['ring'] and got['dashes'] == 0 and got['still'] and got['dimmed'] == 1 and got['before'] >= 1, got)
        first = b.page(OFFSET)
        check('  the player and the computer keep flowing before the ring', poll(lambda: b.page(OFFSET) != first, 3.0))
        got = b.page("""({ inert: ['sg-volume', 'sg-tone'].map((i) => document.getElementById(i)?.inert),
          live: [...document.querySelectorAll('#sg-volume button')].filter((x) => x.getAttribute('aria-disabled') !== 'true').length,
          opacity: getComputedStyle(document.getElementById('sg-volume')).opacity })""")
        check('  the last heard volume and curve are dimmed and inert',
              got['inert'] == [True, True] and got['live'] == 0 and float(got['opacity']) < 0.6, got)
        click_el(b, '#sg-try')
        check('  `Try now` posts connect', poll(lambda: posts_to(b, '/connect') == ['{}'], 5.0), posts_to(b, '/connect'))
        check('  no page error', errors(b) == [], errors(b)[:3])
        # a lost device under reduced motion: the line is drawn once and nothing moves
        r = make(reduced=True)
        try:
            forget(r, gone)
            load(r, gone, route=True)
            poll(lambda: r.page("!!document.getElementById('sg-card')"), 20.0)
            one = r.page(OFFSET)
            time.sleep(0.6)
            check('  under reduced motion the lost card is drawn and no dash moves',
                  one is not None and r.page(OFFSET) == one and r.page("!!document.querySelector('.sg-lostring')"),
                  [one, r.page(OFFSET)])
            r.page(HIDE)
            time.sleep(0.4)
            check('  and a hidden page draws no frame', r.page(OFFSET) == one)
        finally:
            r.close()
        gone.restart()
        check('  the device back: the card, the ring and the dimming go',
              poll(lambda: b.page("!document.getElementById('sg-card') && !document.querySelector('.sg-lostring')"
                                  " && !!document.querySelector('#sg-line [data-flow=after]')"), 30.0))
    finally:
        try:
            forget(b, gone)
        finally:
            b.close()
            gone.stop()


def a_lost_stream():
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make()
    try:
        forget(b, d)
        load(b, d, route=True)
        first = poll(lambda: b.page(OFFSET), 10.0)
        moved = poll(lambda: b.page(OFFSET) != first, 5.0)
        check('a_lost_stream_greys_the_line_and_makes_the_page_inert: with the daemon there the page says nothing',
              b.page("document.getElementById('sg-said').textContent") == ''
              and not b.page("document.getElementById('signal').inert"))
        check('  the dots were moving before the daemon stopped', first is not None and moved)
        check('  the tab reads `connected` while the stream is up', tab_title(b) == 'Gungnir 2 \u00b7 connected \u2014 svolder', tab_title(b))
        dashes = b.page("document.querySelectorAll('#sg-line [data-flow]').length")
        d.proc.terminate()
        d.proc.wait(5)
        check('  the daemon stopped: the subtitle reads `Reconnecting to svolder…`',
              poll(lambda: b.page("document.getElementById('sg-said').textContent") == 'Reconnecting to svolder…', 20.0))
        check('  and the tab reads `reconnecting` while the stream is lost',
              poll(lambda: tab_title(b) == 'Gungnir 2 \u00b7 reconnecting \u2014 svolder', 5.0), tab_title(b))
        got = b.page("""({ faded: document.getElementById('signal').classList.contains('sg-faded'),
          inert: document.getElementById('signal').inert,
          opacity: getComputedStyle(document.getElementById('signal')).opacity,
          focusable: [...document.querySelectorAll('#signal button, #signal [tabindex]')].filter((e) => e.matches(':focus-visible')).length })""")
        check('  the page is faded and inert', got['faded'] and got['inert'] and float(got['opacity']) < 0.6, got)
        one = b.page(OFFSET)
        time.sleep(0.5)
        check('  and the dots stand still, drawn and not moving',
              one is not None and b.page(OFFSET) == one
              and b.page("document.querySelectorAll('#sg-line [data-flow]').length") == dashes,
              {'offsets': [one, b.page(OFFSET)], 'host_is_null': b.page("__core.model.page().host === null"),
               'flow_stretches': [dashes, b.page("document.querySelectorAll('#sg-line [data-flow]').length")]})
        d.restart()
        check('  restarted: the words go and the page can be used',
              poll(lambda: b.page("document.getElementById('sg-said').textContent") == ''
                   and not b.page("document.getElementById('signal').inert"), 30.0))
        check('  and the tab reads `connected` again', poll(lambda: tab_title(b) == 'Gungnir 2 \u00b7 connected \u2014 svolder', 10.0),
              tab_title(b))
        unexpected = [e for e in errors(b) if 'ERR_CONNECTION_REFUSED' not in e
                      and 'ERR_INCOMPLETE_CHUNKED_ENCODING' not in e]
        check('  no page error but the refused connections of the stopped daemon', unexpected == [], unexpected[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def a_refused_write():
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make()
    try:
        forget(b, d)
        load(b, d)
        d.post(DEVICE, 'presets', {'name': 'Taken A'})
        d.post(DEVICE, 'presets', {'name': 'Taken B'})
        poll(lambda: 'Taken B' in preset_names(d), 8.0)
        ready(b, d, 'eq')
        b.page(POSTS)
        tap(b, '#sg-preset-rename')
        poll(lambda: asking(b) is not None or b.page("!!document.querySelector('.sg-list')"))
        if asking(b) is None:
            click_el(b, '.sg-list .sg-opt[data-value="Taken A"]')
        poll(lambda: asking(b) is not None)
        type_into(b, '#sg-shelf .sg-askbox input', 'Taken B')
        b.press('Enter')
        toast = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"), 8.0)
        # the words the daemon itself gives for the same refusal are the words the core toasts
        status, answer = d.post(DEVICE, 'presets/rename', {'from': 'Taken A', 'to': 'Taken B'})
        words = answer.get('error') if status >= 400 else None
        check('a_refused_write_toasts: the daemon refuses a rename onto a name taken and the core toasts why',
              bool(toast) and bool(words) and toast[-1] == words, [toast, words])
        check('  the rename was posted and nothing was renamed',
              posts_to(b, 'presets/rename') != [] and 'Taken A' in preset_names(d)
              and preset_names(d).count('Taken B') == 1, preset_names(d))
        check('  the page reads the device again and is still usable',
              b.page("__core.model.state()") == 'open' and not b.page("document.getElementById('signal').inert"))
        for name in ('Taken A', 'Taken B'):
            d.post(DEVICE, 'presets/delete', {'name': name})
        unexpected = [e for e in errors(b) if 'presets/rename' not in e]
        check('  no page error but the refused rename\'s status line', unexpected == [], unexpected[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()



# ---- the phone (P7b): the strip across the top ----

def make_phone(reduced=False):
    """A phone: 390 x 844 with touch emulation (Firefox: its touch events switched on)."""
    if browser == 'chromium':
        return common.Chromium(size=(390, 844), touch=True, reduced=reduced,
                               args=['--disable-features=OverscrollHistoryNavigation'])
    return common.Firefox(size=(390, 844), reduced=reduced,
                          extra_prefs=['user_pref("dom.w3c_touch_events.enabled", 1);'])


def finger(b, pts, secs=0.3):
    """A touch stroke through the points `pts` ((x, y) each); one point is a tap."""
    if browser == 'chromium':
        b.send('Input.dispatchTouchEvent', {'type': 'touchStart', 'touchPoints': [{'x': pts[0][0], 'y': pts[0][1]}]})
        for x, y in pts[1:]:
            b.send('Input.dispatchTouchEvent', {'type': 'touchMove', 'touchPoints': [{'x': x, 'y': y}]})
            time.sleep(secs / max(1, len(pts) - 1))
        b.send('Input.dispatchTouchEvent', {'type': 'touchEnd', 'touchPoints': []})
    else:
        acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(pts[0][0]), 'y': int(pts[0][1])},
                {'type': 'pointerDown', 'button': 0}]
        for x, y in pts[1:]:
            acts.append({'type': 'pointerMove', 'duration': int(secs * 1000 / max(1, len(pts) - 1)),
                         'origin': 'viewport', 'x': int(x), 'y': int(y)})
        acts.append({'type': 'pointerUp', 'button': 0})
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'finger',
                                          'parameters': {'pointerType': 'touch'}, 'actions': acts}]})
    b.sleep(0.5)


def swipe(b, x0, x1, y=45, steps=8):
    finger(b, [(x0 + (x1 - x0) * i / steps, y) for i in range(steps + 1)])


def finger_tap(b, selector):
    x, y = common.center(b, selector)
    finger(b, [(x, y)])


def finger_add_device(b):
    """The menu's Add device… row, by finger taps."""
    finger_tap(b, '.fxbtn')
    poll(lambda: b.page("!!document.getElementById('menu-add-device')"), 3.0)
    finger_tap(b, '#menu-add-device')


def at_middle(b, hop):
    """Wait for the strip to have slid the hop under its middle (its glide is over)."""
    return poll(lambda: b.page(f"""(() => {{ const o = document.querySelector('#sg-line [data-hop="{hop}"]');
      if (!o) return false; const r = o.getBoundingClientRect();
      return Math.abs(r.left + r.width / 2 - (innerWidth - 60) / 2) < 1.5; }})()"""), 5.0)


STRIP_SHAPE = """(() => { const q = (s) => document.querySelector(s);
  const mid = (e) => { const r = e.getBoundingClientRect(); return r.left + r.width / 2; };
  const band = q('#sg-line').getBoundingClientRect(); const box = q('.sg-pbox').getBoundingClientRect();
  const opts = [...document.querySelectorAll('#sg-line [role=option]')];
  return { band: [band.width, band.height, band.top], vw: innerWidth, ids: opts.map((o) => o.dataset.hop),
    xs: opts.map(mid), box: [box.width, box.height, box.left + box.width / 2, box.top - band.top],
    plus: !!q('#sg-add'), desktop: document.querySelectorAll('#sg-line .sg-hop, #sg-line .sg-box').length,
    role: q('#sg-line').getAttribute('role'), orient: q('#sg-line').getAttribute('aria-orientation'),
    selected: document.querySelectorAll('#sg-line [aria-selected=true]').length,
    open: q('#sg-open').dataset.hop, chosen: (q('#sg-line [aria-selected=true]') || {}).dataset.hop }; })()"""


def the_strip(b, d):
    forget(b, d)
    load(b, d, route=True)
    at_middle(b, 'device:Gungnir%202')
    got = b.page(STRIP_SHAPE)
    ids = ['player', 'computer', 'device:Gungnir%202', 'device:Amp', 'hop:HD 650']
    check('the_strip_centres_the_chosen_hop: under 720 px the line is a strip 92 px high across the page, the hops of the desktop line',
          got['ids'] == ids and got['band'][:2] == [330, 92] and got['desktop'] == 0, got)
    check('  the chosen hop is at the strip\'s middle (the page less the 60 px the menu button keeps), the neighbours 110 px either side',
          all(abs(got['xs'][i] - (165 + 110 * (i - 2))) < 1.5 for i in range(5)), got['xs'])
    check('  boxed 112 by 74 at the middle, from y 10',
          got['box'][:2] == [112, 74] and abs(got['box'][2] - 165) < 1.5 and got['box'][3] == 10, got['box'])
    paint = b.page("""(() => { const w = document.querySelector('#sg-line .sg-wire'); const r = w.getBoundingClientRect();
      const s = document.querySelector('.sg-slide'); const dot = document.querySelector('.sg-d-device.sg-shown').getBoundingClientRect();
      const at = document.elementFromPoint(275, 47);
      return { wire: [r.width, r.height], slide: getComputedStyle(s).overflow, dot: dot.left + dot.width / 2,
        next: !!at && !!at.closest('[data-hop="device:Amp"]') }; })()""")
    check('  the wire\'s picture spans every hop (nothing is cut at the strip\'s width), the dot of the device at the middle, and the neighbour takes the touches that land on it',
          paint['wire'][0] >= 840 and paint['wire'][1] == 92 and paint['slide'] == 'visible'
          and abs(paint['dot'] - 165) < 1.5 and paint['next'], paint)
    check('  one horizontal listbox, one option chosen, the open area is the chosen hop\'s',
          got['role'] == 'listbox' and got['orient'] == 'horizontal' and got['selected'] == 1
          and got['chosen'] == 'device:Gungnir%202' and got['open'] == 'device:Gungnir%202', got)
    check('  there is no + after the last hop (Add device… is in the menu)', got['plus'] is False, got['plus'])
    first = poll(lambda: b.page(OFFSET), 10.0)
    check('  the wire runs under the hops, green before the DAC, and its dots move while the player plays',
          first is not None and poll(lambda: b.page(OFFSET) != first, 5.0), first)
    stray = b.page(f"[...document.querySelectorAll('#signal [style]{CSSOM}')].map((e) => e.className || e.id)")
    check('  no style attribute but on the elements placed through the CSSOM', stray == [], stray)
    # the keys of a listbox hold on the strip
    b.page("document.querySelector('#sg-line [aria-selected=true]').focus()")
    b.press('ArrowRight')
    check('  ArrowRight chooses the next hop and the strip slides to it',
          b.page(CHOSEN) == 'device:Amp' and at_middle(b, 'device:Amp'), b.page(CHOSEN))
    b.press('End')
    check('  End chooses the last', b.page(CHOSEN) == 'hop:HD 650' and at_middle(b, 'hop:HD 650'), b.page(CHOSEN))
    b.press('Home')
    check('  Home the first', b.page(CHOSEN) == 'player' and at_middle(b, 'player'), b.page(CHOSEN))
    check('  no page error', errors(b) == [], errors(b)[:3])


def a_swipe(b, d):
    forget(b, d)
    load(b, d, route=True)
    at_middle(b, 'device:Gungnir%202')
    b.page(POSTS)
    swipe(b, 300, 140)
    check('a_swipe_chooses_the_neighbour: a drag left past a short way chooses the next hop and the strip slides to it',
          poll(lambda: b.page(CHOSEN) == 'device:Amp', 5.0) and at_middle(b, 'device:Amp'), b.page(CHOSEN))
    swipe(b, 200, 222)
    check('  a short drag chooses nothing and the strip goes back', b.page(CHOSEN) == 'device:Amp' and at_middle(b, 'device:Amp'),
          b.page(CHOSEN))
    swipe(b, 140, 300)
    check('  a drag right chooses the one before', poll(lambda: b.page(CHOSEN) == 'device:Gungnir%202', 5.0)
          and at_middle(b, 'device:Gungnir%202'), b.page(CHOSEN))
    finger_tap(b, '#sg-line [data-hop=computer]')
    check('  a tap on a neighbour chooses it', poll(lambda: b.page(CHOSEN) == 'computer', 5.0) and at_middle(b, 'computer'),
          b.page(CHOSEN))
    swipe(b, 300, 140)
    check('  and a swipe is not also a tap: it moved one hop, not two',
          poll(lambda: b.page(CHOSEN) == 'device:Gungnir%202', 5.0), b.page(CHOSEN))
    swipe(b, 140, 300)
    swipe(b, 140, 300)
    check('  a drag past the first hop chooses nothing more', b.page(CHOSEN) == 'player' and at_middle(b, 'player'), b.page(CHOSEN))
    # a page with something to scroll: the DAC's
    b.page("document.querySelector('#sg-line [aria-selected=true]').focus()")
    b.press('ArrowRight')
    b.press('ArrowRight')
    at_middle(b, 'device:Gungnir%202')
    finger(b, [(165, 85 - 10 * i) for i in range(9)], 0.5)
    if browser == 'chromium':
        check('  a vertical stroke on the strip scrolls the page and chooses nothing',
              b.page('scrollY') > 20 and b.page(CHOSEN) == 'device:Gungnir%202', [b.page('scrollY'), b.page(CHOSEN)])
        b.page('window.scrollTo(0, 0)')
    else:
        check('  a vertical stroke chooses nothing', b.page(CHOSEN) == 'device:Gungnir%202', b.page(CHOSEN))
    sent = [p for p in b.page('window.__posts') if not p[0].endswith(('/model', '/stream'))]
    check('  a swipe or a tap on the strip sends nothing to a device', sent == [], sent)
    check('  no page error', errors(b) == [], errors(b)[:3])


def a_pinned_strip(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page('window.scrollTo(0, 700)')
    time.sleep(0.4)
    got = b.page("""(() => { const w = document.querySelector('.sg-bandwrap'); const r = w.getBoundingClientRect();
      const cs = getComputedStyle(w); const at = document.elementFromPoint(165, 45);
      return { y: window.scrollY, top: r.top, height: r.height, pos: cs.position, z: Number(cs.zIndex),
        frame: Number(getComputedStyle(document.getElementById('frame')).zIndex),
        under: !!at && !!at.closest('#sg-line'), slim: getComputedStyle(document.getElementById('sg-slim')).display }; })()""")
    check('the_strip_pins_when_scrolled: scrolled well down the page the strip is at the top, over the page, under the frame\'s layer',
          got['y'] > 300 and abs(got['top']) < 1 and got['height'] >= 92 and got['pos'] == 'sticky'
          and got['z'] < got['frame'] and got['under'], got)
    check('  the slim bar is not drawn (the strip is the bar)', got['slim'] == 'none', got)
    btn = b.page("(() => { const r = document.getElementById('fx-button').getBoundingClientRect();"
                 " return { top: r.top, bottom: r.bottom, right: innerWidth - r.right }; })()")
    check('  the menu button rides with the pinned strip (on screen, at the top, at the right)',
          0 <= btn['top'] and btn['bottom'] <= 92 and btn['right'] < 20, btn)
    finger_tap(b, '#sg-line [data-hop=computer]')
    check('  a tap on a neighbour while pinned chooses it and the strip is still at the top',
          poll(lambda: b.page(CHOSEN) == 'computer', 5.0)
          and abs(b.page("document.querySelector('.sg-bandwrap').getBoundingClientRect().top")) < 1, b.page(CHOSEN))
    # focus lands clear of the strip and of the Now playing bar
    b.page("document.querySelector('#sg-line [data-hop=\"device:Gungnir%202\"]').click()")
    b.page('window.scrollTo(0, 0)')
    poll(lambda: b.page("!!document.getElementById('sg-b1f')"), 5.0)
    for focus in ("document.getElementById('sg-b1f').focus()",
                  "(() => { const all = [...document.querySelectorAll('#sg-shelf button, #sg-shelf input')]"
                  ".filter((e) => e.offsetParent !== null); all[all.length - 1].focus(); })()"):
        b.page('window.scrollTo(0, 0)')
        b.page(focus)
        time.sleep(0.5)
        f = b.page("""(() => { const e = document.activeElement; const r = e.getBoundingClientRect();
          return { id: e.id, top: r.top, bottom: r.bottom,
            strip: document.querySelector('.sg-bandwrap').getBoundingClientRect().bottom,
            bar: document.getElementById('sg-now').getBoundingClientRect().top }; })()""")
        check(f'  keyboard focus ({f["id"]}) is fully visible, under the strip and above the Now playing bar',
              f['top'] >= f['strip'] - 0.5 and f['bottom'] <= f['bar'] + 0.5, f)
    check('  no page error', errors(b) == [], errors(b)[:3])


SMALL = """(() => { const out = [];
  for (const e of document.querySelectorAll('#fx-button, #signal button, #signal [role=option], #signal input, #signal [role=slider]')) {
    if (e.closest('[hidden]') || e.closest('#sg-slim')) continue;
    const r = e.getBoundingClientRect(); if (r.width === 0 && r.height === 0) continue;
    if (e.matches('[role=slider]')) {
      if (e.closest('[inert]')) continue;
      e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect();
      const x = r.left + r.width / 2; const y = r.top + r.height / 2;
      const hit = (dy) => { const t = document.elementFromPoint(x, y + dy); return !!t && (t === e || e.contains(t)); };
      if (!hit(-21) || !hit(21)) out.push([e.id || e.className, 'slider hit area', Math.round(r.height)]);
      continue;
    }
    if (Math.min(r.width, r.height) < 43.5) out.push([e.id || e.className, Math.round(r.width), Math.round(r.height)]);
  } return out; })()"""


def small_targets(b):
    """Every control Signal draws (and the frame's button) that is under 44 px on this page."""
    return b.page(SMALL)


def touch_targets(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page("document.getElementById('sg-facts-fold').click()")
    found = small_targets(b)
    check('touch_targets_are_44_px: every button, hop, field and slider Signal draws on a DAC page at 390 px, and the frame\'s menu button (sized by Signal\'s token), is 44 px or more',
          found == [], found)
    got = b.page("document.querySelectorAll('#signal button, #signal [role=option], #signal input').length")
    check('  (and the page is drawn: the check saw its controls)', got > 40, got)
    # a declared hop's area, an amp's, and the card of a lost device
    b.page("document.querySelector('#sg-line [data-hop=\"hop:HD 650\"]').click()")
    poll(lambda: b.page("!!document.getElementById('sg-hop-edit-open')"))
    found = small_targets(b)
    check('  a declared hop\'s area (Edit, Move earlier, Move later, Remove) is 44 px or more too', found == [], found)
    b.page("document.querySelector('#sg-line [data-hop=\"device:Amp\"]').click()")
    poll(lambda: b.page("!!document.getElementById('sg-amp-power')"), 10.0)
    found = small_targets(b)
    check('  an amp\'s area and shelf too', found == [], found)
    b.page("document.querySelector('#sg-line [data-hop=\"device:Gungnir%202\"]').click()")
    # the sheet, from the menu's row
    finger_add_device(b)
    opened = poll(lambda: b.page(SHEET_OPEN))
    check('  a finger on the menu button and its Add device… row opens the sheet', opened)
    sheet = b.page("""[...document.querySelectorAll('#sg-sheet button, #sg-sheet input')].filter((e) => !e.closest('[hidden]'))
      .map((e) => { const r = e.getBoundingClientRect(); return [e.id || e.dataset.act || e.className, Math.round(r.width), Math.round(r.height)]; })
      .filter((x) => Math.min(x[1], x[2]) < 43.5)""")
    check('  the sheet\'s chips, fields and buttons too', sheet == [], sheet)
    b.press('Escape')
    check('  no page error', errors(b) == [], errors(b)[:3])


def tab_title(b):
    return b.page('document.title')


def the_tab_title(b, d):
    forget(b, d)
    load(b, d, route=True)
    check('the_tab_title_names_the_device_and_the_connection: connected, it reads `<device> · connected — svolder`',
          poll(lambda: tab_title(b) == 'Gungnir 2 \u00b7 connected \u2014 svolder', 5.0), tab_title(b))
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    check('  a pick of another theme keeps the tab naming the device (the title is the core\'s)',
          poll(lambda: tab_title(b) == 'Gungnir 2 \u00b7 connected \u2014 svolder', 5.0), tab_title(b))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_menu_row(b, d):
    forget(b, d)
    load(b, d, route=True)
    b.page(POSTS)
    click_el(b, '.fxbtn')
    row = poll(lambda: b.page("(() => { const r = document.getElementById('menu-add-device');"
                              " return r && r.textContent.trim(); })()"), 3.0)
    check('the_menu_row_adds_a_device: the frame\'s menu has `Add device…` while a device is shown', row == 'Add device\u2026', row)
    b.press('Escape')
    add_device(b)
    check('  its row opens the sheet; the focus goes back to the menu button when it closes',
          poll(lambda: b.page(SHEET_OPEN), 3.0) and (b.press('Escape') or True)
          and poll(lambda: not b.page(SHEET_OPEN), 3.0) and b.page(ACTIVE) == 'fx-button', b.page(ACTIVE))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_mode_places(b, d):
    """Mode: at 1280 px it heads the shelf's middle column (not on the first screen); on a phone it is
    under the curve, and the shelf opens with the input, then the presets, then the band table."""
    forget(b, d)
    b.resize(1280, 900)
    load(b, d, route=True)
    ready(b, d, 'eq')
    got = b.page("""(() => { const m = document.getElementById('sg-mode');
      const kids = [...document.getElementById('sg-mid').children];
      const bands = document.getElementById('sg-bands'); const ch = document.getElementById('sg-choices');
      const t = document.getElementById('sg-bands-table');
      return { inMid: kids[0] === m, inOpen: !!document.querySelector('#sg-open #sg-mode'),
        presetsAfter: kids.findIndex((e) => e.id === 'sg-presets') > 0,
        input: !!ch && bands.contains(ch) && !!ch.querySelector('[data-key=input]')
          && ch.getBoundingClientRect().top >= t.getBoundingClientRect().bottom - 1,
        up: !!document.getElementById('sg-up') }; })()""")
    check('the_mode_has_its_place: at 1280 px Mode heads the shelf\'s middle column, ahead of the presets, not on the first screen;'
          ' the input is under the band table',
          got['inMid'] and not got['inOpen'] and got['presetsAfter'] and got['input'] and not got['up'], got)
    b.resize(390, 844)
    got = poll(lambda: b.page("!!document.querySelector('#sg-tone #sg-mode')"), 5.0)
    check('  a breakpoint change redraws: at 390 px Mode is under the curve', got)
    got = b.page("""(() => { const q = (s) => document.querySelector(s);
      const tone = q('#sg-tone'); const g = q('#sg-graph').getBoundingClientRect();
      const m = q('#sg-mode').getBoundingClientRect(); const sh = [...q('#sg-shelf .sg-cols').children].map((e) => e.id);
      const up = q('#sg-up'); const y = (e) => e.getBoundingClientRect().top;
      return { under: m.top >= g.bottom - 1, once: document.querySelectorAll('#sg-mode').length,
        order: sh.slice(0, 3), upKids: up ? [...up.children].map((e) => e.id) : null,
        inputFirst: !!up && !!up.querySelector('#sg-choices [data-key=input]'),
        flow: !!up && y(q('#sg-choices')) < y(q('#sg-presets')) && y(q('#sg-presets')) < y(q('#sg-bands')),
        sideways: document.documentElement.scrollWidth > innerWidth }; })()""")
    check('  the phone\'s mode sits right under the curve, drawn once', got['under'] and got['once'] == 1, got)
    check('  the phone\'s shelf opens with the input, then the presets, then the band table',
          got['order'][:2] == ['sg-up', 'sg-bands'] and got['inputFirst'] and got['flow'] and not got['sideways'], got)
    b.resize(1280, 900)
    got = poll(lambda: b.page("document.getElementById('sg-mid').firstElementChild.id === 'sg-mode'"), 5.0)
    check('  and back at 1280 px it returns to the middle column', got)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_loudness_buttons(b, d):
    forget(b, d)
    b.resize(1280, 900)
    load(b, d, route=True)
    ready(b, d, 'eq')
    check('the_loudness_buttons_step_the_strength: with Loudness off there is no strength control',
          b.page("!document.getElementById('sg-loud')"))
    d.post(DEVICE, 'eq/mode', {'mode': 'loudness'})
    poll(lambda: b.page("!!document.getElementById('sg-loudp')"), 8.0)
    b.page(POSTS)
    got = b.page("""(() => { const w = document.getElementById('sg-loud'); const m = document.getElementById('sg-loudm');
      const p = document.getElementById('sg-loudp'); const c = w.querySelector('[data-mode=loudness]');
      const r = (e) => e.getBoundingClientRect();
      return { wrap: w.classList.contains('sg-loudw'), box2: w.classList.contains('sg-box2'),
        round: [m, p].every((e) => e.classList.contains('sg-rb') && getComputedStyle(e).borderRadius.includes('50%')),
        over: r(m).left >= r(c).left - 1 && r(m).right <= r(c).left + r(c).width / 2
          && r(p).right <= r(c).right + 1 && r(p).left >= r(c).left + r(c).width / 2,
        strength: w.querySelector('.sg-lv').textContent, inMode: !!w.closest('#sg-mode'),
        shelfBox: document.querySelectorAll('#sg-shelf .sg-box2#sg-loud').length }; })()""")
    check('  while it is on its chip is one wrap with round `\u2212` and `+` over its ends and the strength under its name',
          got['wrap'] and not got['box2'] and got['round'] and got['over'] and got['inMode'] and got['shelfBox'] == 0
          and got['strength'], got)
    before = got['strength']
    click_el(b, '#sg-loudp')
    check('  a click on `+` writes one step up', poll(lambda: [json.loads(x) for x in posts_to(b, 'eq/loudness')]
                                                     == [{'step': 1}], 5.0), posts_to(b, 'eq/loudness'))
    check('  and the strength under the name follows',
          poll(lambda: b.page("document.querySelector('#sg-loud .sg-lv').textContent") != before, 8.0))
    b.page('window.__posts.length = 0')
    click_el(b, '#sg-loudm')
    check('  a click on `\u2212` writes one step down', poll(lambda: [json.loads(x) for x in posts_to(b, 'eq/loudness')]
                                                           == [{'step': -1}], 5.0), posts_to(b, 'eq/loudness'))
    b.page('window.__posts.length = 0')
    synth(b, '#sg-loudp', 'pointerdown')
    time.sleep(1.0)
    held = len(posts_to(b, 'eq/loudness'))
    synth(b, '#sg-loudp', 'pointerup')
    time.sleep(0.3)
    then = len(posts_to(b, 'eq/loudness'))
    time.sleep(0.6)
    check('  held, `+` repeats after the pause and a release stops it', held >= 3 and len(posts_to(b, 'eq/loudness')) == then,
          [held, then, len(posts_to(b, 'eq/loudness'))])
    b.page('window.__posts.length = 0')
    b.page("document.getElementById('sg-loudp').focus()")
    b.press('Enter')
    time.sleep(0.5)
    check('  a key press on it is one step', len(posts_to(b, 'eq/loudness')) == 1, posts_to(b, 'eq/loudness'))
    # a phone: the same wrap under the curve
    b.resize(390, 844)
    poll(lambda: b.page("!!document.querySelector('#sg-tone #sg-loud')"), 5.0)
    got = b.page("""(() => { const t = document.getElementById('sg-tone').getBoundingClientRect();
      const r = document.getElementById('sg-loud').getBoundingClientRect();
      return { under: r.top >= t.top, fits: r.right <= innerWidth, sideways: document.documentElement.scrollWidth > innerWidth }; })()""")
    check('  at 390 px it is under the curve and fits the screen', got['under'] and got['fits'] and not got['sideways'], got)
    miss = b.page("""(() => { const out = [];
      for (const id of ['sg-loudm', 'sg-loudp']) { const e = document.getElementById(id); e.scrollIntoView({ block: 'center' });
        const r = e.getBoundingClientRect(); const cx = r.left + r.width / 2; const cy = r.top + r.height / 2;
        for (const [dx, dy] of [[-21, -21], [21, -21], [-21, 21], [21, 21], [0, 0]]) {
          const t = document.elementFromPoint(cx + dx, cy + dy); if (t !== e) out.push([id, dx, dy, t && (t.id || t.className)]); } }
      return out; })()""")
    check('  and each round button answers a touch anywhere in a 44 px square (the visible disc is smaller)', miss == [], miss)
    b.resize(1280, 900)
    d.post(DEVICE, 'eq/mode', {'mode': 'eq'})
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_band_hold(b, d):
    """A held band `+` repeats across the redraws of the shelf, and stops on a release, a blur, a hidden
    page and a switch; a key is one step; a long press has no context menu."""
    forget(b, d)
    b.resize(1280, 900)
    load(b, d, route=True)
    ready(b, d, 'eq')
    b.page(POSTS)

    def count():
        return len(posts_to(b, 'eq/bands/2'))

    b.page("document.getElementById('sg-b2gp').scrollIntoView({ block: 'center' })")
    b.page("window.__btn = document.getElementById('sg-b2gp')")
    synth(b, '#sg-b2gp', 'pointerdown')
    time.sleep(0.2)
    first = count()
    time.sleep(1.2)
    synth(b, '#sg-b2gp', 'pointerup')
    time.sleep(0.3)
    held = count()
    time.sleep(0.6)
    check('holding_a_band_stepper_repeats: a press steps at once, then after the pause the steps keep coming',
          first == 1 and held >= 4, [first, held])
    check('  the shelf was drawn again meanwhile and the hold followed the button of the same id',
          b.page("window.__btn !== document.getElementById('sg-b2gp')") is True)
    check('  a release stops it', count() == held, [held, count()])
    bodies = {json.dumps(json.loads(x), sort_keys=True) for x in posts_to(b, 'eq/bands/2')}
    check('  every step is one step of the same value', bodies == {json.dumps({'param': 'gain', 'step': 1}, sort_keys=True)}, bodies)

    def stops(how, name):
        # the page has read the device again (a hold does not run while it is waiting)
        poll(lambda: b.page("__core.model.state()") == 'open', 10.0)
        b.page('window.__posts.length = 0')
        synth(b, '#sg-b2gp', 'pointerdown')
        time.sleep(0.9)
        went = count()
        how()
        time.sleep(0.3)
        then = count()
        time.sleep(0.6)
        check(name, went >= 3 and count() == then, [went, then, count()])

    stops(lambda: synth(b, '#sg-b2gp', 'pointercancel'), '  a cancelled pointer stops it')
    stops(lambda: b.page("window.dispatchEvent(new Event('blur'))"), '  a blur of the window stops it')
    stops(lambda: b.page(HIDE), '  a hidden page stops it')
    b.page(SHOW)
    stops(lambda: b.page("""(() => { document.dispatchEvent(new PointerEvent('pointermove', { pointerId: 11,
      isPrimary: true, pointerType: 'mouse', buttons: 0, bubbles: true })); return 1; })()"""),
          '  a move reporting no button down (a release never seen) stops it')
    stops(lambda: b.page("document.getElementById('sg-b2gp').remove()"), '  the button leaving the page stops it')
    load(b, d, route=True)
    ready(b, d, 'eq')
    b.page(POSTS)
    b.page("document.getElementById('sg-b2gp').scrollIntoView({ block: 'center' })")
    stops(lambda: b.page("__core.actions.showDevice('Amp')"), '  another device shown stops it')
    load(b, d, route=True)
    ready(b, d, 'eq')
    b.page(POSTS)
    menu = b.page("""(() => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      document.getElementById('sg-b2gp').dispatchEvent(e); return e.defaultPrevented; })()""")
    check('  a context menu (a long press on a phone) on a stepper is prevented', menu is True, menu)
    # a second finger: it starts no hold of its own and its release does not end the first one's
    two = """(() => { const e = document.getElementById('sg-b2gp'); const r = e.getBoundingClientRect();
      e.dispatchEvent(new PointerEvent('%s', { pointerId: 12, isPrimary: false, pointerType: 'touch',
        button: 0, buttons: 1, clientX: r.left + 5, clientY: r.top + 5, bubbles: true, cancelable: true })); return 1; })()"""
    b.page('window.__posts.length = 0')
    b.page(two % 'pointerdown')
    time.sleep(0.9)
    check('  a second, non-primary pointer on a stepper steps nothing', count() == 0, count())
    synth(b, '#sg-b2gp', 'pointerdown')
    time.sleep(0.6)
    b.page(two % 'pointerup')
    mid = count()
    time.sleep(0.5)
    check('  and the second pointer\'s release does not end the first one\'s hold', mid >= 2 and count() > mid, [mid, count()])
    synth(b, '#sg-b2gp', 'pointerup')
    time.sleep(0.3)
    b.page('window.__posts.length = 0')
    sel = b.page("""(() => { const cs = getComputedStyle(document.getElementById('sg-b2gp'));
      return [cs.userSelect, cs.touchAction]; })()""")
    check('  and a stepper selects nothing and takes no panning', sel == ['none', 'none'], sel)
    b.page("document.getElementById('sg-b2gp').focus()")
    b.press('Enter')
    time.sleep(0.5)
    check('  a key press on a stepper is one step', count() == 1, posts_to(b, 'eq/bands/2'))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_button_stays_with_the_strip(b, d):
    """On a phone the menu button is placed against the window: it keeps its place in the pinned strip
    over scroll steps, and on a desktop too."""
    forget(b, d)
    load(b, d, route=True)
    read = """(() => { const f = document.getElementById('frame'); const btn = document.getElementById('fx-button').getBoundingClientRect();
      const strip = document.querySelector('.sg-bandwrap').getBoundingClientRect();
      return { fixed: f.classList.contains('fixed'), pos: getComputedStyle(f).position, y: window.scrollY,
        dx: btn.left - strip.left, dy: btn.top - strip.top }; })()"""
    seen = []
    for y in (0, 150, 400, 700, 120, 0):
        b.page(f'window.scrollTo(0, {y})')
        time.sleep(0.35)
        seen.append(b.page(read))
    check('the_menu_button_is_fixed_on_a_phone: its frame is placed against the window (class `fixed`, position fixed)',
          all(s['fixed'] and s['pos'] == 'fixed' for s in seen), seen[0])
    check('  across scroll steps (0, 150, 400, 700 and back) the button does not move against the strip',
          max(s['dx'] for s in seen) - min(s['dx'] for s in seen) < 1.5
          and max(s['dy'] for s in seen) - min(s['dy'] for s in seen) < 1.5, [(s['y'], s['dx'], s['dy']) for s in seen])
    check('  and the page really scrolled', max(s['y'] for s in seen) > 300, [s['y'] for s in seen])
    # a desktop width: pinned too, as in every theme: the frame is fixed and the button stays 8 px from the top
    b.resize(1280, 500)
    poll(lambda: b.page("document.getElementById('frame').classList.contains('fixed')"), 5.0)
    b.page('window.scrollTo(0, 0)')
    time.sleep(0.3)
    top0 = b.page("document.getElementById('fx-button').getBoundingClientRect().top")
    b.page('window.scrollTo(0, 200)')
    time.sleep(0.3)
    got = b.page("""({ fixed: document.getElementById('frame').classList.contains('fixed'),
      pos: getComputedStyle(document.getElementById('frame')).position, y: scrollY,
      top: document.getElementById('fx-button').getBoundingClientRect().top })""")
    check('  at 1280 px it is pinned too (fixed): the button stays 8 px from the top when the page scrolls',
          got['fixed'] and got['pos'] == 'fixed' and got['y'] > 100 and abs(top0 - 8) < 1.5 and abs(got['top'] - 8) < 1.5,
          [top0, got])
    b.resize(390, 844)
    b.page('window.scrollTo(0, 0)')
    check('  no page error', errors(b) == [], errors(b)[:3])


def other_themes_pin_the_button_too(b, d):
    """Ember and Paper pin the frame's button to the window as Signal does on a phone: after a scroll
    the frame is `fixed` and the button is still 8 px from the top."""
    for theme in ('paper', 'ember'):
        forget(b, d)
        load(b, d, f'?theme={theme}')
        poll(lambda: b.page("!!document.getElementById('fx-button')"), 5.0)
        b.page('window.scrollTo(0, 300)')
        time.sleep(0.4)
        got = b.page("""({ fixed: document.getElementById('frame').classList.contains('fixed'),
          pos: getComputedStyle(document.getElementById('frame')).position, y: scrollY,
          top: document.getElementById('fx-button').getBoundingClientRect().top })""")
        check(f'other_themes_pin_the_button_too: {theme} at 390 px, scrolled, the frame is fixed and the button 8 px from the top',
              got['fixed'] and got['pos'] == 'fixed' and got['y'] > 0 and abs(got['top'] - 8) <= 1, got)


LONG = 'Extremely-long-declared-hop-name-xx'
assert len(LONG) == 35
LONG = LONG + '-end!'
assert len(LONG) == 40


def long_toml(after):
    names = [LONG, 'Left/Right Cans', 'Hop D', 'Hop E', 'Hop F', 'Hop G']
    return 'auth = "none"\n\n' + ''.join(
        f'[[hop]]\nname = "{n}"\nkind = "other"\nafter = "{after}"\n\n' for n in names)


SIDE = "({ doc: document.documentElement.scrollWidth, body: document.body.scrollWidth, iw: innerWidth })"
LABELS = """[...document.querySelectorAll('#sg-line .sg-hop')].map((o) => { const r = o.getBoundingClientRect();
  return [o.dataset.hop, r.left, r.right]; })"""


def no_sideways_scroll(b, d, query, who, widths):
    for width in widths:
        b.resize(width, 844)
        forget(b, d)
        load(b, d, query)
        poll(lambda: b.page("document.querySelectorAll('#sg-line [role=option]').length") >= 7, 10.0)
        side = b.page(SIDE)
        check(f'  {who} at {width} px: six hops after the device, a 40-character one among them, no sideways scroll',
              side['doc'] <= side['iw'] and side['body'] <= side['iw'], side)
        if width >= 720:
            spans = b.page(LABELS)
            spans.sort(key=lambda x: x[1])
            check(f'  {who} at {width} px: every label stays inside the page and clear of the next',
                  spans[0][1] >= -1 and spans[-1][2] <= width + 1
                  and all(spans[i][2] <= spans[i + 1][1] + 1 for i in range(len(spans) - 1)), spans)
        for hop in ('hop:' + LONG, 'hop:Left/Right Cans'):
            if b.page(f"!!document.querySelector('#sg-line [data-hop=\"{hop}\"]')"):
                b.page(f"document.querySelector('#sg-line [data-hop=\"{hop}\"]').click()")
                time.sleep(0.4)
                side = b.page(SIDE)
                check(f'  {who} at {width} px: with `{hop[4:]}` chosen its area does not scroll the page sideways',
                      side['doc'] <= side['iw'] and side['body'] <= side['iw'], side)
        errs = errors(b)
        check(f'  {who} at {width} px: no page error', errs == [], errs[:3])


def long_names():
    first = True
    for who, make_daemon, query in (
            ('a DAC', lambda: common.Daemon('gungnir2-v2', host='fake', web=long_toml('Gungnir 2'), fleet=FLEET),
             '?theme=signal'),
            ('an amp', lambda: common.Daemon('gungnir2-v2', host='fake', web=long_toml('Amp'), fleet=FLEET),
             '?theme=signal&device=Amp'),
            ('a Loki Max', lambda: common.Daemon(profile='lokimax', name=LOKI, web=long_toml(LOKI)),
             '?theme=signal')):
        d = make_daemon()
        b = make_phone()
        try:
            if first:
                check('no_sideways_scroll_with_long_hop_names: a 40-character name, a name with a space and a slash, six hops after the device',
                      True)
                first = False
            no_sideways_scroll(b, d, query, who, (390, 1100))
        finally:
            try:
                forget(b, d)
            finally:
                b.close()
                d.stop()


def said_and_stub():
    d = common.Daemon('gungnir2-v2', host='off')
    b = make_phone()
    try:
        forget(b, d)
        load(b, d)
        d.proc.terminate()
        d.proc.wait(5)
        poll(lambda: b.page("document.getElementById('sg-said').textContent") != '', 20.0)
        for width in (1280, 720, 390):
            b.resize(width, 844)
            time.sleep(0.4)
            r = b.page("""(() => { const rect = (id) => { const e = document.getElementById(id); const r = e.getBoundingClientRect();
              return [e.textContent, r.left, r.top, r.right, r.bottom]; }; return [rect('sg-stub'), rect('sg-said')]; })()""")
            stub, said = r
            apart = stub[3] <= said[1] or said[3] <= stub[1] or stub[4] <= said[2] or said[4] <= stub[2]
            check(f'the_said_and_stub_do_not_overlap: at {width} px, with the host off and the stream lost, the two quiet lines do not meet',
                  bool(stub[0]) and bool(said[0]) and apart, r)
    finally:
        b.close()
        d.stop()


def the_button_clears_the_strip(b, d):
    forget(b, d)
    for width in (360, 390, 719):
        b.resize(width, 844)
        load(b, d, route=True)
        for hop in ('player', 'device:Gungnir%202', 'hop:HD 650'):
            b.page(f"document.querySelector('#sg-line [data-hop=\"{hop}\"]').click()")
            at_middle(b, hop)
            time.sleep(0.3)
            got = b.page("""(() => { const btn = document.getElementById('fx-button').getBoundingClientRect();
              const band = document.getElementById('sg-line').getBoundingClientRect();
              const seen = { left: 0, right: band.right };
              const hit = (r) => { const l = Math.max(r.left, seen.left); const rr = Math.min(r.right, seen.right);
                return rr > l && l < btn.right + 7 && rr > btn.left - 7 && r.top < btn.bottom + 7 && r.bottom > btn.top - 7; };
              const under = [];
              for (const e of document.querySelectorAll('#sg-line .sg-pbox, #sg-line circle, #sg-line line')) {
                if (hit(e.getBoundingClientRect())) under.push(e.id || e.getAttribute('class')); }
              const covered = [];
              for (const o of document.querySelectorAll('#sg-line [role=option]')) {
                const r = o.getBoundingClientRect();
                for (let x = r.left + 2; x < r.right; x += 6) for (let y = r.top + 2; y < r.bottom; y += 6) {
                  if (x < 0 || x >= band.right) continue;
                  const t = document.elementFromPoint(x, y);
                  if (t && t.closest('#fx-button')) covered.push([o.dataset.hop, x, y]); } }
              return { size: [btn.width, btn.height], btn: [btn.left, btn.right, btn.top, btn.bottom], under, covered: covered.slice(0, 3) }; })()""")
            check(f'the_menu_button_clears_the_strip: at {width} px with {hop} chosen, a 44 px button over no hop, dot, wire or box',
                  got['size'] == [44, 44] and got['under'] == [] and got['covered'] == [], got)
    check('  no page error', errors(b) == [], errors(b)[:3])
    b.resize(390, 844)


def a_draw_in_a_drag(b, d):
    """A draw while a finger drags the track leaves the track where the finger has it."""
    forget(b, d)
    load(b, d, route=True)
    b.page(COUNT_DRAWS)
    got = b.page("""(() => { const band = document.getElementById('sg-line');
      const ev = (type, x) => band.dispatchEvent(new PointerEvent(type, { pointerId: 7, isPrimary: true,
        pointerType: 'touch', clientX: x, clientY: 40, bubbles: true }));
      const track = () => document.querySelector('.sg-slide');
      ev('pointerdown', 200); ev('pointermove', 150);
      const before = track().style.transform;
      __core.model.emit('fleet', __core.model.page().fleet);
      const same = track() === document.querySelector('.sg-slide');
      const out = { before, after: track().style.transform, drag: track().classList.contains('sg-drag'),
        draws: window.__redraws };
      ev('pointercancel', 150);
      out.settled = !track().classList.contains('sg-drag');
      return out; })()""")
    check('a_draw_in_a_drag: a draw in the middle of a finger drag keeps the drag class and the track where it is',
          got['draws'] == 1 and got['drag'] and got['after'] == got['before'] and got['settled'], got)


def the_strips_dots_stop(d):
    for reduced in (True, False):
        b = make_phone(reduced=reduced)
        try:
            forget(b, d)
            load(b, d, route=True)
            first = poll(lambda: b.page(OFFSET), 10.0)
            if reduced:
                time.sleep(0.6)
                check('the_strips_dots_stop: under reduced motion the strip\'s dots stand still (drawn once)',
                      first is not None and b.page(OFFSET) == first, [first, b.page(OFFSET)])
            else:
                check('  the strip\'s dots move while the player plays', poll(lambda: b.page(OFFSET) != first, 5.0), first)
                b.page(HIDE)
                time.sleep(0.4)
                held = b.page(OFFSET)
                time.sleep(0.6)
                check('  and hold while the page is hidden', b.page(OFFSET) == held, [held, b.page(OFFSET)])
                b.page(SHOW)
                check('  and move again when it is shown', poll(lambda: b.page(OFFSET) != held, 5.0), held)
        finally:
            b.close()


def a_pinned_block_with_both_lines():
    d = common.Daemon('gungnir2-v2', host='off')
    b = make_phone()
    try:
        forget(b, d)
        load(b, d)
        b.page("document.getElementById('sg-said').textContent = 'Reconnecting to svolder…'")
        time.sleep(0.4)
        h = b.page("document.querySelector('.sg-bandwrap').getBoundingClientRect().height")
        check('focus_clears_the_pinned_block_with_both_lines: the stub and the quiet word both stand under the strip (taller than 128 px)',
              h > 128 and bool(b.page("document.getElementById('sg-stub').textContent")), h)
        for focus in ("document.getElementById('sg-b1f').focus()",
                      "(() => { const all = [...document.querySelectorAll('#sg-shelf button, #sg-shelf input')]"
                      ".filter((e) => e.offsetParent !== null); all[all.length - 1].focus(); })()"):
            b.page('window.scrollTo(0, 0)')
            b.page(focus)
            time.sleep(0.5)
            f = b.page("""(() => { const e = document.activeElement; const r = e.getBoundingClientRect();
              return { id: e.id, top: r.top, strip: document.querySelector('.sg-bandwrap').getBoundingClientRect().bottom }; })()""")
            check(f'  keyboard focus ({f["id"]}) lands below the whole pinned block', f['top'] >= f['strip'] - 0.5, f)
    finally:
        b.close()
        d.stop()


def a_lost_card_on_a_phone():
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make_phone()
    try:
        d.drop()
        forget(b, d)
        load(b, d)
        poll(lambda: b.page("!!document.getElementById('sg-try')"), 20.0)
        found = small_targets(b)
        check('  the lost card\'s Try now and the rest are 44 px or more', found == [] and b.page("!!document.getElementById('sg-try')"), found)
    finally:
        b.close()
        d.stop()


def the_phone():
    d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    b = make_phone()
    try:
        the_strip(b, d)
        the_cover_on_a_phone(b, d)
        a_swipe(b, d)
        a_draw_in_a_drag(b, d)
        a_pinned_strip(b, d)
        the_button_stays_with_the_strip(b, d)
        other_themes_pin_the_button_too(b, d)
        touch_targets(b, d)
        the_button_clears_the_strip(b, d)
        a_pinch(d)
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()
    dots = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
    try:
        the_strips_dots_stop(dots)
    finally:
        dots.stop()
    long_names()
    said_and_stub()
    a_pinned_block_with_both_lines()
    a_lost_card_on_a_phone()



d = common.Daemon('gungnir2-v2', host='fake', web=HOPS_TOML, fleet=FLEET)
b = make()
try:
    the_mount(b, d)
    the_line(b, d)
    the_last_box_clears_the_buttons(b, d)
    rings_and_dots(b, d)
    choosing(b, d)
    a_draw_between_press_and_release(b, d)
    a_linked_amp(b, d)
    amp_writes(b, d)
    dac_volume(b, d)
    dacs_name(b, d)
    amp_leaves(b, d)
    leaving(b, d)
    motion(b, d)
    hidden(b, d)
    reduced(d)
    first_screen(b, d)
    the_volume_buttons(b, d)
    the_hold(b, d)
    the_point_drag(b, d)
    if browser == 'chromium':
        tone_hold(b, d)
    the_choices(b, d)
    the_bar(b, d)
    the_player(b, d)
    the_cover(b, d)
    the_computer(b, d)
    the_band_table(b, d)
    the_presets(b, d)
    the_shelf_controls(b, d)
    the_facts(b, d)
    the_shelf_has_two_columns(b, d)
    the_mode_places(b, d)
    the_loudness_buttons(b, d)
    the_band_hold(b, d)
    the_menu_row(b, d)
    the_tab_title(b, d)
    a_tall_window(b, d)
    the_slim_bar(b, d)
    the_slim_bar_tap(b, d)
    focus_clear(b, d)
    check('no page error in the whole run', errors(b) == [], errors(b)[:3])
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
loki_shelf()
hati_power()
amp_faults()
unlink()
guest_amp()
plus_sheet()
edit_move_remove()
removed_elsewhere()
linked_note()
guest_hops()
no_host()
alone()
linked_amp_alone()
middle_layout()
a_lost_device()
a_lost_stream()
a_refused_write()
the_phone()
the_cover_gone()
the_cap()
guest_tap()
contrast()
print(f'signal_theme.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
