# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive in a browser: its registration and its frame (listed, mounted from the
address and the picker, the menu button in its slot), the scene (sky, sun, mountains and the floor
drawn from the host's synthetic capture; the floor rolling with the music) and what it costs (the
analysis held only while the scene is on show, no frame while the page is hidden) and the HUD (the
top line, the outrun volume, the EQ line and the fade). Later tasks add the rest of the page's
checks here.
  nightdrive_is_listed_and_mounts_from_the_picker   `?theme=nightdrive` and a pick from Ember
  the_scene_draws_sky_sun_mountains_and_floor       pixels: the top of the sky, the sun, a lit floor cell
  the_floor_rolls_with_the_music                    draws go on and the newest row changes within 1 s
  the_analysis_is_held_while_shown_and_let_go_when_hidden_or_unmounted   one holder; none after the grace, none after a switch
  a_switch_away_and_back_leaves_one_loop            the page's frame requests for each frame of the browser stay as they were
  the_menu_button_sits_in_its_slot                  the frame's button over the 44 px slot
  contrast_on_the_scene                             each text token on the ground and on the panel at AA
  the_menu_has_motion_full_calm_still               three radios, Full checked
  motion_is_kept_per_browser                        Calm kept across a load, under the theme's own key
  calm_stops_the_road                               no new floor row in 2 s, the draws go on
  still_draws_about_one_frame_a_second              3 to 7 draws in 5 s
  the_hud_says_what_the_model_says                  the top line, the EQ line and the volume against the model
  the_top_fades_after_the_idle_time_and_any_input_wakes_it   opacity 0 within 5 s (pref 2 s); 1 within 0.3 s of a pointer move
  the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it    not hidden while faded; a Tab wakes it
  a_volume_change_from_elsewhere_shows_the_volume_for_5_s    only the volume shows; faded again by 6 s
  trouble_while_faded_shows_the_top_line            the device dropped while faded: the top line back with its words, held
  the_menu_button_is_not_in_the_fade                the frame's button is outside what fades
  the_figures_are_russo_one_and_the_unit_mr_dafoe   the two faces are loaded and in use
  the_hud_words_clear_the_sun_or_pass_contrast_there   1280x800 and 1280x1600: no overlap with the sun, or 4.5 on its darkest stop
  minus_and_plus_step_the_volume_and_mute_toggles   `+`, `−` and `MUTE` write the step and the mute; the keys are `−` `MUTE` `+`
  muted_reads_minus_infinity                        the volume says `−∞` with the `dB`, MUTE is pressed
  holding_plus_repeats_and_the_release_ends_it      one step at the press, the next at about 380 ms, then about every 70 ms; a release, a blur, a hidden page and a lost stream end it
  dragging_past_the_cap_writes_the_cap              [web] max_volume_db: a held `+` writes the cap and nothing above it
  an_input_chip_sets_the_input                      two inputs are chips; a tap writes the input
  a_long_input_list_opens_and_picks                 five inputs are one chip with a list over it; a pick writes it; Escape closes it
  the_transport_plays_and_pauses                    three svg keys; pause posts its press and the key becomes play
  a_guests_tap_posts_nothing                        a guest's dash is inert; a tap posts nothing and the core's words show
  a_press_on_the_dash_while_faded_acts_and_wakes    a press on `+` while faded posts the volume and shows the top again
  the_dash_fades_to_a_fifth_with_the_hud            the dash at 0.2 with the HUD (desktop, 915x412); a move wakes it; the waking tap writes nothing
  the_dash_stays_while_its_input_list_is_open       the HUD faded, the dash at 1; the upright panel never fades
  hovering_the_mountains_shows_the_curve_and_leaving_lets_it_go   data-tune on over the mountains, off in the sky; a bubble per band
  a_bubble_drag_sets_frequency_and_gain             the last writes are the steps nearest the drop, to that band of that device
  a_flat_device_switches_to_eq_on_the_first_drag    Flat becomes EQ, one toast
  the_curve_stays_1_5_s_after_the_drag              on at 1.2 s, off by 2.5 s
  the_value_shows_above_the_bubble                  Chromium: the label's words and place while held
  a_guest_tap_on_the_mountains_says_why             a real guest: no morph, nothing posted, the core's words
  a_tone_update_waits_for_the_drag                  Chromium: a write from elsewhere moves no bubble under the hand
  a_device_switch_mid_drag_writes_nothing_to_the_new_device   Chromium: nothing posted to the new device or after the switch
  a_device_switch_without_a_drag_shows_no_curve     a switch with no drag on gives the new device no grace curve
  a_bubble_is_grabbed_at_dpr_2                      Chromium, dpr 2: drawn and grabbed at the same CSS px
  hovering_a_bubble_lights_its_bell_and_diamonds    the band's bell and a diamond each side, at half the gain by the core's own response
  dragging_a_diamond_sets_q                         the drop at a step's half point writes that step's Q, to that band only
  a_diamond_is_grabbed_at_dpr_2                     Chromium, dpr 2: drawn and grabbed at the same CSS px
  a_press_on_the_bubble_takes_the_bubble_beside_a_diamond   Chromium: the narrowest Q puts a diamond within 12 px; the press moves the band, never the Q
  the_wheel_steps_the_nearest_bands_q               a notch up narrows, down widens; nearest in x when no bubble is near
  a_pinch_sets_the_q_of_the_band_nearest_the_fingers   Chromium, touch: twice the distance halves the Q of band 3
  a_first_touch_shows_the_curve_and_a_second_grabs  Chromium, touch: the first touch shows, the second grabs
  the_wheel_and_the_pinch_keep_the_curve_1_5_s      on at 1.0 s, off by 2.6 s after a notch or lifted fingers
The timing cases (a draw's cost, draws and rows a second) are in nightdrive_timing.py, run in the gate's serial lane.
The neon sheet's cases are in nightdrive_sheet.py; what both share is in nightdrive_base.py.
Usage: python3 nightdrive_theme.py chromium|firefox"""
import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)

PICKER_CARDS = "[...document.querySelectorAll('#picker .tcard')].map((c) => c.dataset.theme)"
HOLDERS = '__core.analysis.holders'
# the canvases' pixels, in the canvas's own pixels: [r, g, b, a] at a place given in CSS px
PIXEL = """((id, x, y) => { const c = document.getElementById(id);
  const k = c.width / c.clientWidth;
  return [...c.getContext('2d').getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data]; })"""
# the newest row of the floor, as the scene holds it (the module is the one the page loaded)
NEWEST = """import(new URL('assets/themes/nightdrive/driver.js', document.baseURI)).then((m) => {
  const row = m.bound.scene && m.bound.scene.floor.row(0);
  return row ? [m.bound.scene.floor.whole, [...row].map((v) => Math.round(v * 1000))] : null; })"""














def the_mount(b, d):
    forget(b, d)
    check('nightdrive_is_listed_and_mounts_from_the_picker: ?theme=nightdrive mounts Night drive',
          load(b, d) == 'nightdrive')
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      page: !!document.getElementById('nightdrive'), sheets: [...document.querySelectorAll('link[data-sheet]')]
        .map((l) => l.dataset.sheet), buttons: document.querySelectorAll('#fx-button').length,
      canvases: document.querySelectorAll('#nightdrive canvas').length,
      colour: document.querySelector('meta[name=theme-color]').content })""")
    check('  the page is drawn, with Night drive\'s sheet, two canvases and one menu button',
          on == {'theme': 'nightdrive', 'page': True, 'sheets': ['nightdrive'], 'buttons': 1,
                 'canvases': 2, 'colour': '#0a0018'}, on)
    check('  no page error', errors(b) == [], errors(b)[:3])
    forget(b, d)
    check('  the page without the theme is Ember', load(b, d, '?effects=off') == 'ember')
    b.press('T')
    cards = b.page(PICKER_CARDS)
    check('  the picker lists Ember, Paper, Signal and Night drive',
          cards == ['ember', 'paper', 'signal', 'nightdrive'], cards)
    # a theme's picture is drawn once its module has loaded, which takes a moment more with every file it has
    poll(lambda: b.page("!!document.querySelector('#picker .tcard[data-theme=nightdrive] .tprev svg')"), 5.0)
    card = b.page("(() => { const c = document.querySelector('#picker .tcard[data-theme=nightdrive]');"
                  " return [c.querySelector('b').textContent, c.querySelector('small').textContent,"
                  " !!c.querySelector('.tprev svg')]; })()")
    check('  and Night drive\'s card has its name, its blurb and a picture',
          card == ['Night drive', 'an outrun road that moves with the music', True], card)
    click_el(b, '#picker .tcard[data-theme=nightdrive]')
    check('  a pick of Night drive from Ember mounts it',
          poll(lambda: b.page("!!document.getElementById('nightdrive')")))
    on = b.page("""({ theme: document.documentElement.dataset.theme,
      sheets: [...document.querySelectorAll('link[data-sheet]')].map((l) => l.dataset.sheet),
      ember: !!document.querySelector('#screen, .fx'),
      buttons: document.querySelectorAll('#fx-button').length })""")
    check('  and leaves nothing of Ember',
          on == {'theme': 'nightdrive', 'sheets': ['nightdrive'], 'ember': False, 'buttons': 1}, on)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_scene(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    geo = b.page("""(() => { const c = document.getElementById('nd-road');
      return { w: c.clientWidth, h: c.clientHeight, k: c.width / c.clientWidth }; })()""")
    w, h = geo['w'], geo['h']
    horizon = h * 0.56
    top = [b.page(f'({PIXEL})("nd-sky", {x}, 1)') for x in (w * 0.31, w * 0.5, w * 0.69)]
    # the night's top runs from its deep end (5, 2, 26) to its warm one (20, 4, 41) with the mood
    near = [p for p in top if 2 <= p[0] <= 23 and p[1] <= 8 and 22 <= p[2] <= 45]
    check('the_scene_draws_sky_sun_mountains_and_floor: the top of the sky is the deep night, between #05021a and #140429 with the mood',
          len(near) >= 2 and geo['k'] == 1, [top, geo])
    sun = b.page(f'({PIXEL})("nd-sky", {w / 2}, {horizon - h * 0.04})')
    check('  the sun\'s centre is warm', sun[0] > 200 and sun[1] > 100 and sun[3] == 255, sun)
    ground = b.page(f'({PIXEL})("nd-sky", 3, {h - 3})')
    check('  the ground under the road is the night', abs(ground[0] - 10) <= 4 and ground[2] < 40, ground)
    time.sleep(2.0)
    lit = b.page("""((horizon) => { const c = document.getElementById('nd-road');
      const g = c.getContext('2d'); const w = c.width; const h = c.height;
      const y0 = Math.round(horizon * c.width / c.clientWidth) + 4;
      const data = g.getImageData(Math.round(w * 0.25), y0, Math.round(w * 0.5), h - y0 - 2).data;
      let strong = 0; let best = 0;
      for (let i = 3; i < data.length; i += 4) { best = Math.max(best, data[i]); if (data[i] >= 140) strong++; }
      return { strong, best }; })""" + f'({horizon})')
    print('     sampled: top', top, 'sun', sun, 'ground', ground, 'floor', lit)
    check('  a floor cell is lit after 2 s', lit['strong'] > 200 and lit['best'] > 160, lit)
    ridge = b.page("""((horizon) => { const c = document.getElementById('nd-road');
      const g = c.getContext('2d'); const k = c.width / c.clientWidth; let cyan = 0;
      const data = g.getImageData(0, Math.round((horizon - 0.25 * c.clientHeight) * k), c.width,
        Math.round(0.26 * c.clientHeight * k)).data;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i + 3] > 200 && data[i] < 120 && data[i + 1] > 150 && data[i + 2] > 180) cyan++; }
      return cyan; })""" + f'({horizon})')
    check('  the ridge\'s cyan edge is drawn over the horizon', ridge > 300, ridge)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_floor(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    time.sleep(1.2)
    one = b.page(NEWEST)
    draws = b.page(DRAWS)
    time.sleep(1.0)
    two = b.page(NEWEST)
    check('the_floor_rolls_with_the_music: the draws go on and the newest row is a new one within 1 s',
          b.page(DRAWS) > draws + 10 and one and two and two[0] > one[0] and one[1] != two[1],
          [draws, b.page(DRAWS), one, two])
    check('  the row is the music: its columns are not all alike',
          len(set(two[1])) > 3 and max(two[1]) > 100, two)
    # a step-down while the music plays drops the wet road; paused, it comes back as soon as the draws measure under budget
    b.page('__core.theme.driver.forceStepDown()')
    held = b.page('__core.theme.driver.perf().wet')
    # the budget measures 2 ms a draw whatever the browser takes, so the return-under-budget path runs in every browser
    b.page('__core.theme.driver.forceDrawMs(2)')
    b.page(PAUSE)
    back = poll(lambda: b.page('__core.theme.driver.perf().wet') is True, 12.0)
    check('while_idle_the_wet_road_returns_once_the_draws_measure_under_budget: dropped while playing, back within 12 s of the pause',
          held is False and back is True, [held, back, b.page(PERF)])
    b.page('__core.theme.driver.forceDrawMs(null)')
    b.page(PAUSE.replace('pause', 'play'))


def the_analysis(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(HOLDERS) == 1)
    time.sleep(0.5)
    check('the_analysis_is_held_while_shown_and_let_go_when_hidden_or_unmounted: one holder while shown',
          b.page(HOLDERS) == 1, b.page(HOLDERS))
    b.page(HIDE)
    time.sleep(1.0)
    check('  still held inside the core\'s grace', b.page(HOLDERS) == 1, b.page(HOLDERS))
    check('  let go once the page has been hidden for the grace', poll(lambda: b.page(HOLDERS) == 0, 4.0),
          b.page(HOLDERS))
    b.page(SHOW)
    check('  held again once shown', poll(lambda: b.page(HOLDERS) == 1, 4.0), b.page(HOLDERS))
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=paper]')
    poll(lambda: b.page("document.documentElement.dataset.theme") == 'paper')
    time.sleep(0.5)
    check('  let go at once when another theme replaces it',
          b.page("document.documentElement.dataset.theme") == 'paper' and b.page(HOLDERS) == 0,
          [b.page("document.documentElement.dataset.theme"), b.page(HOLDERS)])
    check('  no page error', errors(b) == [], errors(b)[:3])




# requestAnimationFrame requests the page makes (counted through a wrapper) against the browser's own frames
# (counted by a loop of the check's, made with the real function)
RAF_COUNT = """(() => { if (!window.__rafs) { window.__rafs = 0; window.__frames = 0; const real = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (fn) => { window.__rafs++; return real(fn); };
  const own = () => { window.__frames++; real(own); }; real(own); } return 1; })()"""
RAF_READ = '[window.__rafs, window.__frames]'


def rafs_per_frame(b, secs=2.0):
    one = b.page(RAF_READ)
    time.sleep(secs)
    two = b.page(RAF_READ)
    return (two[0] - one[0]) / max(1, two[1] - one[1])


def pick_theme(b, theme):
    b.press('T')
    click_el(b, f'#picker .tcard[data-theme={theme}]')
    poll(lambda: b.page("document.documentElement.dataset.theme") == theme)
    time.sleep(1.0)


def the_loops(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    b.page(RAF_COUNT)
    time.sleep(0.5)
    before = rafs_per_frame(b)
    pick_theme(b, 'paper')
    pick_theme(b, 'nightdrive')
    poll(lambda: b.page(DRAWS) > 3)
    after = rafs_per_frame(b)
    check('a_switch_away_and_back_leaves_one_loop: the frame requests for each frame of the browser stay about what they were (a second loop adds one a frame)',
          before > 0.3 and after < before + 0.5, [before, after])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_menu_button(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.5)
    got = b.page("""(() => { const s = document.getElementById('nd-slot').getBoundingClientRect();
      const q = document.getElementById('fx-button').getBoundingClientRect();
      return { slot: [s.width, s.height], right: innerWidth - s.right, top: s.top,
        dx: (q.left + q.width / 2) - (s.left + s.width / 2),
        dy: (q.top + q.height / 2) - (s.top + s.height / 2), size: [q.width, q.height],
        label: document.getElementById('fx-button').getAttribute('aria-label') }; })()""")
    check('the_menu_button_sits_in_its_slot: a 44 px slot at the top right, the button over it',
          got['slot'] == [44, 44] and 6 <= got['right'] <= 12 and 6 <= got['top'] <= 12
          and abs(got['dx']) <= 2 and abs(got['dy']) <= 2 and got['label'] == 'Menu', got)
    check('  the button is at least 44 px', got['size'][0] >= 44 and got['size'][1] >= 44, got)
    click_el(b, '#fx-button')
    check('  and opens the frame\'s menu', poll(lambda: b.page("!!document.getElementById('menu-theme')"), 3.0))
    b.press('Escape')
    check('  no page error', errors(b) == [], errors(b)[:3])


CONTRAST = """() => {
  const num = (c) => c.match(/[\\d.]+/g).map(Number);
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92
    : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const style = getComputedStyle(document.documentElement);
  const token = (name) => style.getPropertyValue(name).trim();
  // the panel (rgba .62) over the brightest of the sky's own colours, the dusk at the horizon
  const [pr, pg, pb, pa] = num(token('--nd-panel'));
  const dusk = hex(token('--nd-dusk'));
  const panel = [pr, pg, pb].map((v, i) => v * pa + dusk[i] * (1 - pa));
  const grounds = { ground: lum(hex(token('--nd-night'))), panel: lum(panel) };
  const out = {};
  for (const name of ['ink', 'mid', 'dim', 'pink-ink', 'track', 'track-sub', 'cyan-ink', 'cyan-key',
    'warn', 'bad']) {
    const l = lum(hex(token(`--nd-${name}`)));
    for (const [on, g] of Object.entries(grounds)) {
      out[`${name} on ${on}`] = Math.round((Math.max(l, g) + 0.05) / (Math.min(l, g) + 0.05) * 100) / 100;
    }
  }
  const frame = getComputedStyle(document.getElementById('fx-button'));
  const fl = lum(num(frame.color));
  out['menu button on the panel'] = Math.round((Math.max(fl, grounds.panel) + 0.05)
    / (Math.min(fl, grounds.panel) + 0.05) * 100) / 100;
  return out;
}"""


def contrast(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.5)
    got = b.page(f'({CONTRAST})()')
    check('contrast_on_the_scene: every text token on the ground and on the panel reads at 4.5 or more',
          len(got) == 21 and all(v >= 4.5 for v in got.values()), {k: v for k, v in got.items() if v < 4.5})










def the_motion_menu(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.5)
    check('the_menu_has_motion_full_calm_still: the menu opens', open_menu(b))
    rows = b.page(MOTION_ROWS)
    check('  three radios, Full, Calm and Still, in that order, Full checked',
          rows == [{'id': 'full', 'text': 'Full', 'role': 'menuitemradio', 'checked': 'true'},
                   {'id': 'calm', 'text': 'Calm', 'role': 'menuitemradio', 'checked': 'false'},
                   {'id': 'still', 'text': 'Still', 'role': 'menuitemradio', 'checked': 'false'}], rows)
    check('  and no kept level yet', b.page(KEY) is None, b.page(KEY))
    b.press('Escape')
    pick(b, 'calm')
    check('motion_is_kept_per_browser: Calm is kept under the theme\'s own key',
          b.page(KEY) == '"calm"', b.page(KEY))
    load(b, d)
    time.sleep(0.5)
    open_menu(b)
    rows = b.page(MOTION_ROWS)
    check('  and it is the level after a load', [r['id'] for r in rows if r['checked'] == 'true'] == ['calm']
          and b.page(PERF)['pace'] == 100, [rows, b.page(PERF)])
    b.press('Escape')
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_calm_road(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    time.sleep(1.0)
    pick(b, 'calm')
    time.sleep(0.5)
    one = b.page(NEWEST)
    draws = b.page(DRAWS)
    time.sleep(2.0)
    two = b.page(NEWEST)
    check('calm_stops_the_road: no new row in 2 s, and the draws go on',
          one and two and one[0] == two[0] and b.page(DRAWS) > draws + 5, [one and one[0], two and two[0], draws, b.page(DRAWS)])
    pick(b, 'full')
    time.sleep(1.5)
    three = b.page(NEWEST)
    check('  Full rolls it again', three and three[0] > two[0], [two[0], three and three[0]])


def the_still_frame(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    pick(b, 'still')
    time.sleep(1.0)
    got = rate(b, 5.0)
    check('still_draws_about_one_frame_a_second: 3 to 7 draws in 5 s', 3 <= got <= 7, got)
    check('  its pace is 1000', b.page(PERF)['pace'] == 1000, b.page(PERF))
















# the sky canvas round the sun, six o'clock to three: the flare now, the pixel just outside the sun's rim (1.02 radii, 60
# degrees up), one in its halo (1.4 radii) and the sky at the same height out at the left edge (the median of three, no
# star): [r, g, b, a] each
SUN_RING = """(() => { const c = document.getElementById('nd-sky'); const k = c.width / c.clientWidth;
  const px = (x, y) => [...c.getContext('2d').getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data];
  const h = c.clientHeight; const r = 0.22 * h; const cx = c.clientWidth / 2; const cy = 0.56 * h - 0.04 * h; const a = 1.05;
  const med = (list) => [0, 1, 2].map((i) => list.map((p) => p[i]).sort((u, v) => u - v)[1]);
  const y = cy - 1.4 * r * Math.sin(a);
  return { flare: __core.theme.driver.look().flare, edge: px(cx + 1.02 * r * Math.cos(a), cy - 1.02 * r * Math.sin(a)),
    halo: px(cx + 1.4 * r * Math.cos(a), y), far: med([px(20, y), px(60, y), px(100, y)]) }; })"""
# the first y (from the top) in the column x of the road canvas that is painted, in CSS px
TOP_PAINTED = """((x) => { const c = document.getElementById('nd-road'); const k = c.width / c.clientWidth;
  const d = c.getContext('2d').getImageData(Math.round(x * k), 0, 1, c.height).data;
  for (let i = 0; i < c.height; i++) if (d[i * 4 + 3] > 25) return i / k; return -1; })"""
# the road canvas's pixel over the night, as the person sees it: [r, g, b]
SEEN = """((id, x, y) => { const c = document.getElementById(id); const k = c.width / c.clientWidth;
  const [r, g, b, a] = c.getContext('2d').getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data;
  const f = a / 255; return [r * f + 10 * (1 - f), g * f, b * f + 24 * (1 - f), a]; })"""
QUIETLY = '?theme=nightdrive&nd-debug=noridge,nomood,nowet,nohaze,noembers'


def the_heat(b, d):
    """The floor as a heat grid: a loud floor is hot where a quiet one is dark, and its crossings are lifted
    above where a quiet one's lie; both with the mood's layers off, so only the floor is in view."""
    forget(b, d)
    load(b, d, QUIETLY)
    poll(lambda: b.page(DRAWS) > 3)
    geo = b.page("""(() => { const c = document.getElementById('nd-road'); return [c.clientWidth, c.clientHeight]; })()""")
    w, h = geo
    b.page(f'({FLOOR_AT})(0.05)')
    time.sleep(1.2)
    quiet = b.page(f'({SEEN})("nd-road", {w / 2 + 77}, {h - 40})')
    top_quiet = b.page(f'({TOP_PAINTED})({w / 2})')
    b.page(f'({FLOOR_AT})(0.9)')
    # the heat is read against the floor's own peak, which a loud floor sets at once: the cell is hot once it is drawn
    loud = poll(lambda: (lambda p: p if p[0] > 120 and p[1] > 90 else None)(
        b.page(f'({SEEN})("nd-road", {w / 2 + 77}, {h - 40})')), 4.0) or b.page(f'({SEEN})("nd-road", {w / 2 + 77}, {h - 40})')
    top_loud = b.page(f'({TOP_PAINTED})({w / 2})')
    check('the_loud_floor_is_hotter_than_the_quiet_one: at the foot of the road the loud heat is orange where the quiet is dark',
          loud[0] > 120 and loud[1] > 90 and quiet[1] < 40 and loud[0] + loud[1] > quiet[0] + quiet[1] + 120, [quiet, loud])
    check('the_loud_floor_is_lifted: the road is painted 8 CSS px or more higher up the screen when every cell is loud',
          top_quiet > 0 and top_loud > 0 and top_quiet - top_loud >= 8, [top_quiet, top_loud])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_quiet_night(b, d):
    """In the quiet the night is deep, the road wet (the sun and the mountains in it) and the air hazy; loud
    music has none of these. The bands are held, the ear moved at once, the page's own draws go on."""
    forget(b, d)
    load(b, d, '?theme=nightdrive&nd-debug=noridge')
    poll(lambda: b.page(DRAWS) > 3)
    geo = b.page("""(() => { const c = document.getElementById('nd-road'); return [c.clientWidth, c.clientHeight]; })()""")
    w, h = geo
    H = 0.56 * h
    b.page(f'({HOLD})(0.8, 60)')
    time.sleep(1.5)
    loud = {'look': b.page(LOOK), 'ear': b.page(EAR),
            'sky': b.page(f'({SEEN})("nd-sky", {w * 0.1}, {H - 10})'),
            'mirror': b.page(f'({PIXEL})("nd-sky", {w / 2}, {H + 20})'),
            'haze': b.page(f'({PIXEL})("nd-road", {w * 0.06}, {H + 12})')[3]}
    b.page(f'({HOLD})(0, 120)')
    time.sleep(1.5)
    quiet = {'look': b.page(LOOK), 'ear': b.page(EAR),
             'sky': b.page(f'({SEEN})("nd-sky", {w * 0.1}, {H - 10})'),
             'mirror': b.page(f'({PIXEL})("nd-sky", {w / 2}, {H + 20})'),
             'haze': b.page(f'({PIXEL})("nd-road", {w * 0.06}, {H + 12})')[3]}
    check('the_night_follows_the_song: the loud passage has a mood over 0.9, a dry road and no haze; the silence a mood under 0.3, '
          'a wet road and thick haze', loud['look']['mood'] > 0.9 and loud['look']['wet'] == 0 and loud['look']['haze'] == 0
          and quiet['look']['mood'] < 0.3 and quiet['look']['wet'] > 0.5 and quiet['look']['haze'] > 0.7,
          [loud['look'], quiet['look'], loud['ear'], quiet['ear']])
    check('  the sky at the horizon is deeper in the quiet (the red channel 40 or more lower)',
          quiet['sky'][0] < loud['sky'][0] - 40, [quiet['sky'], loud['sky']])
    check('the_wet_road_shows_the_sun_in_the_quiet_and_not_when_loud: the sky canvas just under the horizon is sun-coloured '
          'in the quiet and the night otherwise', quiet['mirror'][0] > 60 and loud['mirror'][0] < 25,
          [quiet['mirror'], loud['mirror']])
    check('the_haze_lies_over_the_far_floor_in_the_quiet_only: the road canvas beside the road is painted there in the quiet '
          'and clear when loud', quiet['haze'] > 80 and loud['haze'] < 15, [quiet['haze'], loud['haze']])
    # the readout's switches take each layer away
    for name, field in (('nowet', 'wet'), ('nohaze', 'haze')):
        load(b, d, f'?theme=nightdrive&nd-debug={name}')
        poll(lambda: b.page(DRAWS) > 3)
        b.page(f'({HOLD})(0, 120)')
        time.sleep(1.2)
        got = b.page(LOOK)
        other = 'haze' if field == 'wet' else 'wet'
        check(f'{name}_takes_its_layer_away_and_leaves_the_others: {field} 0, {other} over 0.5', got[field] == 0 and got[other] > 0.5, got)
    load(b, d, '?theme=nightdrive&nd-debug=nomood')
    poll(lambda: b.page(DRAWS) > 3)
    b.page(f'({HOLD})(0, 120)')
    time.sleep(1.2)
    got = b.page(LOOK)
    check('nomood_keeps_the_scene_as_it_was: the mood stays 0.7 and the cruise 1 in the silence', got['mood'] == 0.7 and got['cruise'] == 1, got)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_wet_road(b, d):
    """The reflection is made at every draw (a fine pointer): in the quiet, with the wet road drawn, the copies
    of the sky the road is drawn from keep step with the scene's own draws."""
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    b.page(f'({HOLD})(0, 120)')
    time.sleep(1.5)
    look, was = b.page(LOOK), b.page(PERF)
    time.sleep(1.5)
    now = b.page(PERF)
    draws, copies = now['draws'] - was['draws'], now['mirrored'] - was['mirrored']
    check('the_wet_roads_mirror_is_never_older_than_one_draw: in the quiet the wet road is drawn',
          look['wet'] > 0.5 and now['wet'] is True, [look, now])
    check('  and a copy of the sky is made at every draw, not a few times a second',
          draws >= 20 and copies >= draws - 1, [draws, copies])


def the_moment(b, d):
    """A drop: the silence, then loud music, brings a flare at the sun and a wave down the floor, and the embers
    after the peak has lasted. Nothing of it with the switches, in Calm half the flare, in Still none."""
    forget(b, d)
    load(b, d, '?theme=nightdrive&nd-debug=noridge')
    poll(lambda: b.page(DRAWS) > 3)
    geo = b.page("""(() => { const c = document.getElementById('nd-road'); return [c.clientWidth, c.clientHeight]; })()""")
    w, h = geo
    H = 0.56 * h
    b.page(f'({HOLD})(0, 120)')
    time.sleep(1.0)
    before = b.page(f'({SUN_RING})()')
    quiet = b.page(LOOK)
    b.page(f'({HOLD})(0.8, 0)')
    during = poll(lambda: (lambda r: r if r['flare'] > 0.6 else None)(b.page(f'({SUN_RING})()')), 4.0)
    peak = b.page(LOOK)
    check('a_drop_brings_a_flare_and_a_wave: loud music after the silence lifts the flare over 0.6 within 4 s, with the wave',
          bool(during) and peak['wave'] > 0.3 and quiet['flare'] == 0, [quiet, peak])
    green = lambda p: p[1]
    check('  the sun grows: the pixel just outside its rim is the night before and sun-coloured during the flare (green under 90, over 120)',
          bool(during) and green(before['edge']) < 90 and green(during['edge']) > 120, [before['edge'], during and during['edge']])
    lift = lambda r: r['halo'][1] - r['far'][1]
    check('  a warm halo rises round it: the green channel at 1.4 radii is 8 or more over the sky at the same height',
          bool(during) and lift(during) >= 8 and abs(lift(before)) <= 4, [lift(before), during and lift(during)])
    # the flare ends 2.5 s after the drop; a beat in the music may begin another, so the case waits for a moment
    # with none and reads the rim and the halo then
    gone = poll(lambda: (lambda r: r if r['flare'] == 0 and b.page(LOOK)['wave'] == 0 else None)(b.page(f'({SUN_RING})()')), 8.0)
    after = gone or b.page(f'({SUN_RING})()')
    check('  and it is gone 2.5 s later: no flare, no wave, the rim and the halo as before',
          b.page(LOOK)['flare'] == 0 and b.page(LOOK)['wave'] == 0 and green(after['edge']) < 90 and abs(lift(after)) <= 6 and bool(during) and lift(after) < lift(during) - 2,
          [b.page(LOOK), after])
    # the embers: 6 s of peak
    poll(lambda: b.page(LOOK)['embers'] > 0.5, 12.0)
    time.sleep(1.5)
    sparks = b.page('(() => DRIVER.then((m) => m.bound.scene.embers.list.length))()'.replace('DRIVER', DRIVER))
    check('the_embers_rise_after_a_peak_of_6_s: the ear says embers over 0.5 within 12 s of loud music and sparks are alive', b.page(LOOK)['embers'] > 0.5 and sparks > 0, [b.page(LOOK), sparks])
    load(b, d, '?theme=nightdrive&nd-debug=noembers')
    poll(lambda: b.page(DRAWS) > 3)
    b.page(f'({HOLD})(0.8, 60)')
    time.sleep(8.0)
    sparks = b.page('(() => DRIVER.then((m) => m.bound.scene.embers.list.length))()'.replace('DRIVER', DRIVER))
    check('noembers_takes_the_sparks_away: none alive after 8 s of loud music', b.page(LOOK)['embers'] == 0 and sparks == 0, [b.page(LOOK), sparks])
    check('  no page error', errors(b) == [], errors(b)[:3])








HUD_TEXT = """(() => ({ left: document.querySelector('.nd-left').textContent,
  right: document.querySelector('.nd-right').textContent,
  tone: document.querySelector('.nd-right').className,
  eq: document.querySelector('.nd-eq').hidden ? null : document.querySelector('.nd-eq').textContent,
  aria: document.querySelector('.nd-vol svg')?.getAttribute('aria-label') ?? null,
  title: document.title }))()"""


def move(b):
    b.click(640, 450)


def the_hud_text(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    got = b.page(HUD_TEXT)
    m = d.model()
    inp = next((c for c in m['controls'] if c.get('key') == 'input' and c.get('kind') == 'choice'), None)
    chosen = next((o['label'] for o in inp['options'] if o.get('on')), None) if inp else None
    rate = next((c['text'] for c in m['controls'] if c.get('key') == 'sample_rate'), None)
    want = ' \u00b7 '.join(x for x in ['Gungnir 2', chosen, rate] if x)
    check('the_hud_says_what_the_model_says: the top line names the device, its input and its rate',
          got['left'] == want, [got['left'], want])
    text = m['volume']['text']
    neg = text[:1] in '-\u2212'
    figures = text.replace(' dB', '').lstrip('-\u2212')
    check('  the volume reads the model\'s dB', got['aria'] is not None and figures in got['aria']
          and ('minus' in got['aria']) == neg, [got['aria'], text])
    mode = m['tone']['mode']
    first = {'flat': 'FLAT', 'eq': 'EQ'}.get(mode)
    if first is None:
        print('skip   the EQ line at load: the tone starts in', mode)
    else:
        check(f'  the EQ line reads the model\'s mode ({mode}) at load', (got['eq'] or '').startswith(first),
              [got['eq'], mode])
    status, _ = d.post('Gungnir 2', 'eq/mode', {'mode': 'eq'})
    poll(lambda: (b.page(HUD_TEXT)['eq'] or '').startswith('EQ'), 5.0)
    got = b.page(HUD_TEXT)
    m = d.model()
    gains = [f"{abs(round(float(r['g'].split()[0]), 1)):.1f}" for r in m['tone']['bands']['rows']]
    nums = (got['eq'] or '').replace('\u2212', '-').split()
    check('  the EQ line reads the model\'s gains', got['eq'] is not None and got['eq'].startswith('EQ')
          and [x.lstrip('+-') for x in nums if x[:1] in '+-0123456789'] == gains, [got['eq'], gains])
    host_eq = b.page('__core.model.page().host?.route?.host_eq === true')
    check('  the verdict is the model\'s host EQ (nothing when it is off)',
          (got['right'] != '') == bool(host_eq) and 'bad' not in got['tone'], [got['right'], host_eq])
    check('  the volume is drawn as the outrun SVG', b.page("!!document.querySelector('.nd-vol svg.nd-odb')"))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_fade(b, d):
    load_fading(b, d)
    time.sleep(0.8)
    check('the_top_fades_after_the_idle_time_and_any_input_wakes_it: shown at first',
          b.page(f'{OPACITY}(".nd-top")') == 1 and not b.page(FADED))
    t0 = time.time()
    done = poll(lambda: all(b.page(f'{OPACITY}("{s}")') == 0 for s in ('.nd-top', '.nd-vol', '.nd-eq')), 7.0)
    check('  all three are at opacity 0 within 5 s', done and time.time() - t0 <= 5.2, time.time() - t0)
    move(b)
    shown = [b.page(f'{OPACITY}("{s}")') for s in ('.nd-top', '.nd-vol', '.nd-eq')]
    check('  a pointer move brings them back at once', shown == [1, 1, 1] and not b.page(FADED), shown)
    time.sleep(0.3)
    check('  and the idle time counts from the last input: still shown 1 s later',
          b.page(f'{OPACITY}(".nd-top")') == 1)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tab_order(b, d):
    load_fading(b, d)
    poll(lambda: b.page(FADED), 6.0)
    time.sleep(2.7)
    got = b.page("""(() => [...document.querySelectorAll('.nd-top, .nd-vol, .nd-eq')].map((e) => {
      const s = getComputedStyle(e);
      return [s.display !== 'none', s.visibility === 'visible', !e.closest('[aria-hidden=true]'),
        !e.closest('[hidden]') || e.matches('.nd-eq')]; }))()""")
    check('the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it: faded but not hidden, not display:none, in the tree',
          b.page(FADED) and len(got) == 3 and all(all(r) for r in got), [b.page(FADED), got])
    b.press('Tab')
    time.sleep(0.2)
    now = b.page("[!!document.activeElement && document.activeElement !== document.body, "
                 "document.getElementById('nightdrive').classList.contains('nd-faded')]")
    check('  a Tab moves the focus and wakes the HUD', now == [True, False], now)
    # a focus moved with no key and no pointer (a screen reader, a dialog giving it back) wakes it as well
    b.page("document.activeElement.blur()")
    faded = poll(lambda: b.page(FADED), 6.0)
    b.page("document.getElementById('nd-sheet').focus()")
    time.sleep(0.2)
    check('  a focus moved with no key or pointer wakes the faded HUD',
          faded and not b.page(FADED), [faded, b.page(FADED)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_volume_from_elsewhere(b, d):
    load_fading(b, d)
    poll(lambda: b.page(FADED), 6.0)
    time.sleep(2.7)
    before = d.model()['volume']['text']
    d.write('Gungnir 2', 'volume', -50 if before != '-50.0 dB' else -40)
    seen = poll(lambda: b.page(f'{OPACITY}(".nd-vol")') > 0.9, 5.0)
    t0 = time.time()
    both = [b.page(f'{OPACITY}(".nd-top")'), b.page(f'{OPACITY}(".nd-eq")')]
    check('a_volume_change_from_elsewhere_shows_the_volume_for_5_s: only the volume shows',
          seen and both == [0, 0] and b.page(FADED), [seen, both])
    time.sleep(max(0, 4.5 - (time.time() - t0)))
    check('  still shown at 4.5 s', b.page(f'{OPACITY}(".nd-vol")') > 0.9, b.page(f'{OPACITY}(".nd-vol")'))
    time.sleep(max(0, 6.0 - (time.time() - t0)))
    check('  faded again by 6 s', b.page(f'{OPACITY}(".nd-vol")') < 0.05 and b.page(FADED),
          [b.page(f'{OPACITY}(".nd-vol")'), time.time() - t0])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_trouble(b):
    d = common.Daemon('gungnir2-v2', host='fake')
    try:
        trouble_run(b, d)
    finally:
        forget(b, d)
        if browser == 'chromium':
            b.errors.clear()
        else:
            b.errors = []
        d.stop()


def trouble_run(b, d):
    load_fading(b, d)
    poll(lambda: b.page(FADED), 6.0)
    time.sleep(2.7)
    check('trouble_while_faded_shows_the_top_line: faded before the device drops',
          b.page(f'{OPACITY}(".nd-top")') == 0 and b.page(FADED))
    d.drop()
    back = poll(lambda: not b.page(FADED) and b.page(f'{OPACITY}(".nd-top")') == 1, 25.0)
    got = b.page(HUD_TEXT)
    status = got['title'].split(' \u00b7 ')[1].split(' \u2014 ')[0] if ' \u00b7 ' in got['title'] else None
    check('  the device out of reach brings the top line back with its words', back and got['right'] != ''
          and got['right'] == status and 'bad' in got['tone'], got)
    time.sleep(5.0)
    check('  and the fade stays off while it lasts (5 s past the idle time)',
          not b.page(FADED) and b.page(f'{OPACITY}(".nd-top")') == 1 and b.page(HUD_TEXT)['right'] == got['right'])
    check('  no page error', [e for e in errors(b) if not any(w in e for w in NETWORK)] == [],
          errors(b)[:3])


def the_menu_button_outside_the_fade(b, d):
    load_fading(b, d)
    poll(lambda: b.page(FADED), 6.0)
    time.sleep(2.7)
    got = b.page("""(() => { const q = document.getElementById('fx-button');
      return [!q.closest('.nd-top, .nd-vol, .nd-eq'), !q.closest('.nd-faded'),
        +getComputedStyle(q).opacity, document.getElementById('nightdrive').classList.contains('nd-faded')]; })()""")
    check('the_menu_button_is_not_in_the_fade: the HUD is faded and the frame\'s button is outside it',
          got[0] and got[1] and got[2] > 0 and got[3], got)


def the_fonts(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    got = b.page("""(async () => { await document.fonts.load("118px 'Russo One'", '23.5');
      await document.fonts.load("64px 'Mr Dafoe'", 'dB');
      const fam = (sel) => getComputedStyle(document.querySelector(sel)).fontFamily;
      return { russo: document.fonts.check("118px 'Russo One'", '23.5'),
        dafoe: document.fonts.check("64px 'Mr Dafoe'", 'dB'),
        figures: fam('.nd-vol text[font-size="118"]'), unit: fam('.nd-vol text[font-size="64"]') }; })()""")
    check('the_figures_are_russo_one_and_the_unit_mr_dafoe: both faces are loaded',
          got['russo'] and got['dafoe'], got)
    check('  and the figures and the unit use them', 'Russo One' in got['figures'] and 'Mr Dafoe' in got['unit'], got)




def volume(d):
    return d.model(DEVICE)['volume']


def settled(d, raw, secs=10.0):
    return poll(lambda: volume(d)['raw'] == raw, secs)


def volume_posts(b):
    """The volume writes this page has sent so far: [raw, time in ms]."""
    return [[json.loads(body)['raw'], at] for url, body, at in b.page('window.__posts')
            if url.endswith('/volume') and body]






DASH_WORDS = """[...document.querySelectorAll('#nd-dash button')].map((w) => [w.dataset.act, w.textContent.trim()])"""
VOL_ARIA = "document.querySelector('.nd-vol svg')?.getAttribute('aria-label')"
# the figures are text; the minus and the infinity are drawn as paths (Russo One has neither), so a
# muted volume has no figure text, an infinity of three strokes and its unit
VOL_TEXT = """(() => { const v = document.querySelector('.nd-vol svg'); return v ? [v.querySelectorAll('text[font-size="118"]').length,
  v.querySelectorAll('path[stroke="url(#nd-stars)"]').length, v.textContent] : null; })()"""


def start_at(b, d, raw):
    d.post(DEVICE, 'volume', {'raw': raw})
    settled(d, raw)
    time.sleep(0.4)


def the_dash(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    m = volume(d)
    lo, hi = m['lo'], m['hi']
    words = b.page(DASH_WORDS)
    acts = [w[0] for w in words]
    check('the_dash_runs_minus_mute_plus_input_track_transport_device: the keys are `−`, `MUTE`, `+`, in that order,'
          ' then the input, then the transport, then DEVICE',
          words[:3] == [['volume-down', '−'], ['mute', 'MUTE'], ['volume-up', '+']]
          and len(acts) > 7 and set(acts[3:-4]) == {'input'} and acts[-4:] == ['prev', 'play', 'next', 'sheet'], words)
    box = b.page("""(() => { const r = document.getElementById('nd-dash').getBoundingClientRect();
      const q = [...document.querySelectorAll('#nd-dash .nd-k')].map((k) => k.getBoundingClientRect().height);
      return [Math.round(r.bottom), innerHeight, Math.min(...q), Math.max(...q)]; })()""")
    check('  the dash sits along the bottom and its keys are 40 px high', box[0] <= box[1] and box[1] - box[0] < 60
          and box[2] == 40 and box[3] == 40, box)
    sub = b.page("document.querySelector('.nd-sub').textContent")
    track = b.page("__core.model.page().device.playing.track")
    check('the_track_reads_title_artist_album_and_time: the dash names the track, its artist, its album and the time',
          b.page("document.querySelector('.nd-title').textContent") == track['headline']
          and track['artist'] in sub and track['album'] in sub and ':' in sub, [sub, track])
    icons = b.page("""[...document.querySelectorAll('#nd-dash [data-act=prev], #nd-dash [data-act=play], #nd-dash [data-act=next]')].map((w) => {
      const v = w.querySelector('svg'); return [w.textContent.trim(), w.querySelectorAll('svg').length,
        v && v.getAttribute('aria-hidden'), v && getComputedStyle(v).fill === getComputedStyle(w).color,
        w.hasAttribute('style') || (v && v.hasAttribute('style'))]; })""")
    check('the_transport_is_svg_not_glyphs: each key is one inline svg of its own colour, with no text',
          icons == [['', 1, 'true', True, False]] * 3, icons)
    # the volume
    start_at(b, d, lo + 30)
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-vup')
    check('minus_and_plus_step_the_volume_and_mute_toggles: `+` raises by one step', settled(d, lo + 31), volume(d)['raw'])
    click_el(b, '#nd-vdown')
    check('  `−` lowers by one step', settled(d, lo + 30), volume(d)['raw'])
    click_el(b, '#nd-vup')
    click_el(b, '#nd-vup')
    check('  two quick taps raise two steps from the step asked', settled(d, lo + 32), volume(d)['raw'])
    time.sleep(0.4)
    asked = b.page('window.__posts.length')
    check('  the HUD volume follows the device', poll(lambda: b.page(VOL_ARIA) is not None and
          volume(d)['text'].replace(' dB', '').lstrip('-−') in b.page(VOL_ARIA)), [b.page(VOL_ARIA), volume(d)['text']])
    click_el(b, '#nd-mute')
    check('  `MUTE` mutes', poll(lambda: volume(d)['muted'] is True), volume(d)['muted'])
    check('muted_reads_minus_infinity: the HUD volume says `−∞` with the dB, and the key is pressed',
          poll(lambda: b.page(VOL_TEXT) == [0, 1, 'dB'] and b.page(VOL_ARIA) == 'muted, minus infinity dB'
               and b.page("document.getElementById('nd-mute').getAttribute('aria-pressed')") == 'true'),
          [b.page(VOL_TEXT), b.page(VOL_ARIA)])
    click_el(b, '#nd-mute')
    check('  and again unmutes', poll(lambda: volume(d)['muted'] is False)
          and poll(lambda: b.page("document.getElementById('nd-mute').getAttribute('aria-pressed')") == 'false'))
    # a step this page asked for does not pop the volume in the HUD (it is not another hand's)
    b.page("document.getElementById('nd-vup').focus()")
    b.press('Enter')
    check('  a key on the focused `+` is one step and keeps the focus through the redraw',
          settled(d, lo + 33) and poll(lambda: b.page('document.activeElement.id') == 'nd-vup'),
          [volume(d)['raw'], b.page('document.activeElement.id')])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_hold(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    lo = volume(d)['lo']

    def count():
        return len(volume_posts(b))

    def start():
        start_at(b, d, lo + 20)
        b.page('window.__posts.length = 0')

    def stops(how, name):
        start()
        synth(b, '#nd-vup', 'pointerdown')
        time.sleep(0.9)
        went = count()
        how()
        time.sleep(0.4)
        then = count()
        time.sleep(0.6)
        check(name, went >= 3 and count() == then, [went, then, count()])

    start()
    synth(b, '#nd-vup', 'pointerdown')
    time.sleep(0.2)
    first = count()
    time.sleep(1.0)
    synth(b, '#nd-vup', 'pointerup')
    time.sleep(0.4)
    posts = volume_posts(b)
    held = len(posts)
    # the press's own step at once, the next about 380 ms on, then about every 70 ms (a write waits for
    # the one before it, so the posts can come a little less often than the steps)
    gaps = [round(b2[1] - b1[1]) for b1, b2 in zip(posts, posts[1:])]
    print(f'     holding +: {held} posts in 1.2 s, gaps {gaps[:12]}')
    check('holding_plus_repeats_and_the_release_ends_it: one step at the press, then, after the pause, the steps keep coming',
          first == 1 and held >= 5, [first, held])
    check('  the first repeat comes about 380 ms after the press (300 to 600)',
          len(posts) > 1 and 300 <= posts[1][1] - posts[0][1] <= 600, gaps[:3])
    steady = gaps[2:]
    check('  and the steady steps come every 70 ms or so (median 55 to 120)',
          len(steady) >= 3 and 55 <= sorted(steady)[len(steady) // 2] <= 120, steady)
    time.sleep(0.5)
    check('  a release stops them', count() == held, [held, count()])
    stops(lambda: synth(b, '#nd-vup', 'pointercancel'), '  a cancelled pointer stops them')
    stops(lambda: b.page("window.dispatchEvent(new Event('blur'))"), '  a blur of the window stops them')
    stops(lambda: b.page(HIDE), '  a hidden page stops them')
    b.page(SHOW)
    stops(lambda: b.page("__core.model.emit('state', 'lost')"), '  the stream lost stops them')
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    start()
    synth(b, '#nd-vup', 'pointerdown')
    time.sleep(0.5)
    b.page("(() => { window.__btn = document.getElementById('nd-vup');"
           " __core.model.emit('fleet', __core.model.page().fleet); return 1; })()")
    time.sleep(0.1)
    mid = count()
    time.sleep(0.5)
    check('  a redraw in the middle of a hold keeps the button and the steps go on',
          b.page("window.__btn.isConnected && window.__btn === document.getElementById('nd-vup')") is True
          and count() > mid, [mid, count()])
    menu = b.page("""(() => { const e = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
      document.getElementById('nd-vup').dispatchEvent(e); return e.defaultPrevented; })()""")
    check('  a context menu (a long press on a phone) on a held key is prevented', menu is True, menu)
    synth(b, '#nd-vup', 'pointerup')
    time.sleep(0.3)
    start()
    b.page("document.getElementById('nd-vup').focus()")
    b.press('Enter')
    time.sleep(0.9)
    check('  Enter on the focused key stays one step', count() == 1, count())
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_ask_again(b, d):
    """A step asked, then another hand sets the device back: the same step asked again is written."""
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    lo = volume(d)['lo']
    start_at(b, d, lo + 20)
    b.page('window.__posts.length = 0')
    asked_at = time.time()
    b.page("document.getElementById('nd-vup').focus()")
    b.press('Enter')
    time.sleep(0.9)
    check('a_step_asked_again_after_another_hand_set_the_device_back_is_written: Enter on `+` writes one step',
          len(volume_posts(b)) == 1, len(volume_posts(b)))
    start_at(b, d, lo + 20)
    b.page('window.__posts.length = 0')
    b.page("document.getElementById('nd-vup').focus()")
    b.press('Enter')
    time.sleep(0.6)
    check('  the step asked before is still under 3 s old, as the check needs',
          time.time() - asked_at < 3.0, time.time() - asked_at)
    check('  a step asked again after another hand set the device back is written',
          len(volume_posts(b)) == 1, len(volume_posts(b)))
    # a device that answers a write and reads nothing new: after the 3 s the step asked is not held
    b.page("""(() => { const f = window.fetch; window.fetch = (...a) => String(a[0]).endsWith('/volume')
      ? (window.__posts.push([String(a[0]), String((a[1] || {}).body || ''), performance.now()]),
         Promise.resolve(new Response('{"ok":true}', { status: 200, headers: { 'Content-Type': 'application/json' } })))
      : f(...a); return 1; })()""")
    start_at(b, d, lo + 20)
    b.page('window.__posts.length = 0')
    b.page("document.getElementById('nd-vup').focus()")
    b.press('Enter')
    time.sleep(3.4)
    b.press('Enter')
    time.sleep(0.6)
    check('  a step asked again 3 s after a write that no reading followed is written',
          len(volume_posts(b)) == 2, [p[0] for p in volume_posts(b)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_cap():
    """A held `+` past a cap held in the config, on a daemon and in a browser of its own."""
    capped = common.Daemon('gungnir2-v2', web='auth = "none"\nmax_volume_db = -30.0', host='fake')
    b = make()
    try:
        forget(b, capped)
        load(b, capped)
        time.sleep(0.8)
        b.page(REC)
        m = volume(capped)
        cap = m['cap']
        check('dragging_past_the_cap_writes_the_cap: the device reports a cap', cap is not None, m)
        start_at(b, capped, cap - 4)
        b.page('window.__posts.length = 0')
        synth(b, '#nd-vup', 'pointerdown')
        time.sleep(1.6)
        synth(b, '#nd-vup', 'pointerup')
        posts = [p[0] for p in volume_posts(b)]
        check('  a held `+` past the cap writes the cap and nothing above it',
              posts != [] and max(posts) == cap, [cap, posts])
        check('  the model settles at the cap', settled(capped, cap), volume(capped)['raw'])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, capped)
        finally:
            b.close()
            capped.stop()


def the_dash_input(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    c = next(x for x in d.model(DEVICE)['controls'] if x['key'] == 'input')
    labels = [o['label'] for o in c['options']]
    now = next(o['label'] for o in c['options'] if o['on'])
    want = next(x for x in labels if x != now)
    one = b.page("document.querySelectorAll('#nd-dash [data-act=input]').length")
    check('a_long_input_list_opens_and_picks: five inputs are one chip that names the current one',
          len(labels) > 3 and one == 1
          and b.page("document.querySelector('#nd-dash [data-act=input]').textContent") == f'{now} ▾', [one, labels])
    click_el(b, '#nd-dash [data-act=input]')
    opened = poll(lambda: b.page("document.querySelectorAll('.nd-list .nd-opt').length") == len(labels), 3.0)
    got = b.page("""(() => { const l = document.querySelector('.nd-list').getBoundingClientRect();
      const c = document.querySelector('#nd-dash [data-act=input]');
      const q = c.getBoundingClientRect();
      return { above: l.bottom <= q.top + 1, inside: l.top >= 0 && l.right <= innerWidth,
        expanded: c.getAttribute('aria-expanded'), on: document.querySelector('.nd-opt.on').textContent }; })()""")
    check('  the list opens over the chip, inside the page, with the current input marked',
          opened and got['above'] and got['inside'] and got['expanded'] == 'true' and got['on'] == now, got)
    b.press('Escape')
    check('  Escape closes it and writes nothing', poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 0, 3.0)
          and posts_to(b, 'controls/input') == [], posts_to(b, 'controls/input'))
    click_el(b, '#nd-dash [data-act=input]')
    poll(lambda: b.page("document.querySelectorAll('.nd-list').length") == 1, 3.0)
    click_el(b, f'.nd-opt[data-value="{want}"]')
    check('  a pick writes that input', poll(lambda: next(o['label'] for o in next(
        x for x in d.model(DEVICE)['controls'] if x['key'] == 'input')['options'] if o['on']) == want, 8.0), want)
    check('  and the list is gone and the chip names it', poll(lambda: b.page(
        "document.querySelectorAll('.nd-list').length") == 0 and b.page(
        "document.querySelector('#nd-dash [data-act=input]').textContent") == f'{want} ▾', 5.0))
    # put it back for the next checks
    d.post(DEVICE, 'controls/input', json.loads(next(o['json'] for o in c['options'] if o['label'] == now)))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_chips():
    """Two inputs are two chips (a device of its own: the Loki Max's XLR and RCA)."""
    two = common.Daemon('lokimax', host='fake')
    b = make()
    try:
        forget(b, two)
        load(b, two)
        time.sleep(0.8)
        b.page(REC)
        chips = b.page("[...document.querySelectorAll('#nd-dash [data-act=input]')].map((c) => [c.textContent, c.getAttribute('aria-pressed')])")
        check('an_input_chip_sets_the_input: two inputs are two chips, the current one pressed',
              sorted(c[0] for c in chips) == ['RCA', 'XLR'] and sorted(c[1] for c in chips) == ['false', 'true'], chips)
        off = next(c[0] for c in chips if c[1] == 'false')
        click_el(b, f'#nd-dash [data-act=input][data-value="{off}"]')

        def chosen():
            m = two.model(DEVICE)
            return next(o['label'] for o in next(x for x in m['controls'] if x['key'] == 'input')['options'] if o['on'])
        check('  a tap writes that input and the chip is pressed', poll(lambda: chosen() == off, 8.0)
              and poll(lambda: b.page(f'document.querySelector(\'#nd-dash [data-value="{off}"]\').getAttribute("aria-pressed")') == 'true', 5.0),
              [chosen(), off])
        check('  and the dash has no volume keys on a device with no volume',
              b.page("document.querySelectorAll('#nd-dash [data-act=volume-up], #nd-dash [data-act=mute]').length") == 0)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, two)
        finally:
            b.close()
            two.stop()


COVER = """(() => { const c = document.querySelector('#nd-dash .nd-cover'); const i = c && c.querySelector('img');
  const r = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.right, b.width, b.height]; };
  const dash = document.getElementById('nd-dash');
  return { has: !!c, hidden: c ? getComputedStyle(c).display === 'none' : null,
    box: c ? r(c) : null, radius: c ? getComputedStyle(c).borderTopLeftRadius : null,
    edge: c ? getComputedStyle(c).borderTopColor : null, loaded: i ? i.complete && i.naturalWidth > 0 : null,
    src: i ? i.getAttribute('src') : null, abs: i ? i.src : null, alt: i ? i.getAttribute('alt') : null,
    keys: r(dash.querySelector('.nd-keys')), now: r(dash.querySelector('.nd-now')), title: r(dash.querySelector('.nd-title')),
    style: !!dash.querySelector('[style]:not(.nd-prog i)') }; })()"""


def the_dash_cover(d):
    """The cover in the dash: the fake host's picture between the input and the title, none leaving no
    gap, another track another picture."""
    b = make(size=(1280, 800))
    try:
        forget(b, d)
        load(b, d)
        poll(lambda: b.page(COVER)['loaded'], 6.0)
        c = b.page(COVER)
        check('the_dash_shows_the_cover_when_there_is_one: the host\'s picture is there, drawn, square, rounded 4 px '
              'with a cyan edge, between the input and the title',
              c['has'] and c['loaded'] and abs(c['box'][2] - c['box'][3]) <= 1 and c['box'][2] > 30 and c['radius'] == '4px'
              and c['edge'] == 'rgb(34, 211, 238)' and c['alt'] == ''
              and c['keys'][1] < c['box'][0] < c['box'][1] <= c['title'][0] + 0.5, c)
        check('  it is the daemon\'s own, a same-origin address with the version, and the markup has no style attribute',
              c['src'].startswith('api/v1/host/cover?v=') and c['abs'].startswith(d.url) and not c['style'], c)
        check('  no page error', errors(b) == [], errors(b)[:3])
        # a phone held upright: the track has the row to itself, the cover stays on it, before the title
        b.resize(390, 844)
        time.sleep(0.8)
        p = b.page(COVER)
        dash = b.page("(() => { const r = document.getElementById('nd-dash').getBoundingClientRect(); return [r.left, r.right]; })()")
        check('  on a phone held upright the cover stays on the track\'s own row, square, inside the dash, before the title',
              p['has'] and not p['hidden'] and abs(p['box'][2] - p['box'][3]) <= 1 and dash[0] <= p['box'][0]
              and p['box'][1] <= p['title'][0] + 0.5 and p['title'][2] > 100 and p['title'][1] <= dash[1], [p, dash])
    finally:
        b.close()


def the_dash_without_a_cover():
    """The host's picture goes (a daemon that stops holding it): no box and no gap, the title moves left."""
    g = common.Daemon('gungnir2-v2', host='fake')
    b = make(size=(1280, 800))
    try:
        forget(b, g)
        load(b, g)
        poll(lambda: b.page(COVER)['loaded'], 6.0)
        before = b.page(COVER)
        g.drop_cover()
        gone = poll(lambda: not b.page(COVER)['has'] or b.page(COVER)['hidden'], 14.0)
        after = b.page(COVER)
        check('no_cover_leaves_no_gap: with the cover gone there is no box, and the title starts where the track\'s own box does, '
              'further left than before',
              before['has'] and gone and after['title'][0] < before['title'][0] - 20
              and abs(after['title'][0] - after['now'][0]) <= 0.5, [before['title'], after])
        check('  no page error', [e for e in errors(b) if 'cover' not in e and '404' not in e] == [], errors(b)[:3])
    finally:
        b.close()
        g.stop()


def the_cover_follows_the_track(d):
    """Another track brings its own picture; a picture the daemon no longer holds is hidden and leaves no gap."""
    b = make(size=(1280, 800))
    try:
        forget(b, d)
        load(b, d)
        poll(lambda: b.page(COVER)['loaded'], 6.0)
        first = b.page(COVER)
        b.page("""(() => { const url = __core.model.shown(); const p = structuredClone(__core.model.page().device.playing);
          p.track.cover = 'deadbeefdeadbeef'; p.track.headline = 'Another Song';
          __core.model.applyPart(url, 'playing', p); return 1; })()""")
        other = poll(lambda: (b.page(COVER)['src'] or '').endswith('deadbeefdeadbeef') and b.page(COVER), 5.0)
        hidden = poll(lambda: b.page(COVER)['hidden'], 5.0)
        mid = b.page(COVER)
        check('the_cover_follows_the_track: another track\'s picture is asked for by its own address, and one the daemon '
              'does not hold (a real 404) is hidden, its box gone, the title where the track begins',
              bool(other) and other['src'] != first['src'] and hidden is True and mid['box'][2] == 0
              and abs(mid['title'][0] - mid['now'][0]) <= 0.5, [first['src'], other and other['src'], mid])
        b.page("""(() => { const url = __core.model.shown(); const p = structuredClone(__core.model.page().device.playing);
          p.track.cover = null; p.track.headline = 'No Picture';
          __core.model.applyPart(url, 'playing', p); return 1; })()""")
        none = poll(lambda: not b.page(COVER)['has'], 5.0)
        check('  a track with no cover has no box at all', none is True and b.page("document.querySelector('.nd-title').textContent") == 'No Picture',
              b.page(COVER))
        b.page("""(() => { const url = __core.model.shown(); const p = structuredClone(__core.model.page().device.playing);
          p.track.cover = %s; p.track.headline = 'Back';
          __core.model.applyPart(url, 'playing', p); return 1; })()""" % json.dumps(first['src'].split('v=')[1]))
        back = poll(lambda: b.page(COVER)['has'] and b.page(COVER)['loaded'] and not b.page(COVER)['hidden'], 5.0)
        check('  and the first picture comes back when the track does', back is True, b.page(COVER))
    finally:
        b.close()


def the_dash_transport(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    b.page(REC)
    b.page("fetch('api/v1/host/press', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ button: 'play' }) })")
    poll(lambda: b.page("__core.model.page().device.playing.track.status") == 'playing', 6.0)
    time.sleep(0.5)
    b.page('window.__posts.length = 0')
    names = lambda: b.page("[...document.querySelectorAll('#nd-dash [data-button]')].map((w) => w.dataset.button)")  # noqa: E731
    check('the_transport_plays_and_pauses: previous, pause and next', poll(lambda: names() == ['previous', 'pause', 'next'], 5.0), names())
    click_el(b, '#nd-dash [data-button=pause]')
    check('  `pause` posts the host\'s press and the model says paused',
          poll(lambda: posts_to(b, 'host/press') == ['{"button":"pause"}'], 5.0)
          and poll(lambda: b.page("__core.model.page().device.playing.track.status") == 'paused', 8.0),
          [posts_to(b, 'host/press'), b.page("__core.model.page().device.playing.track.status")])
    check('  the key becomes `play`', poll(lambda: names() == ['previous', 'play', 'next'], 5.0), names())
    click_el(b, '#nd-dash [data-button=play]')
    check('  `play` brings back `pause`', poll(lambda: names() == ['previous', 'pause', 'next'], 8.0), names())
    was = b.page("document.querySelector('.nd-time').textContent")
    check('  the time on the dash moves on while it plays',
          poll(lambda: b.page("document.querySelector('.nd-time').textContent") != was, 4.0), was)
    fill = b.page("document.querySelector('.nd-prog i').style.width")
    check('  the progress bar is filled from the elapsed share', fill not in ('', None) and fill.endswith('%'), fill)
    click_el(b, '#nd-dash [data-button=next]')
    check('  `next` posts its press', poll(lambda: '{"button":"next"}' in posts_to(b, 'host/press'), 5.0), posts_to(b, 'host/press'))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_guest():
    """A real guest: a daemon whose visitors may read, a browser with no sign-in."""
    g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = make()
    try:
        forget(b, g)
        load(b, g)
        time.sleep(0.8)
        b.page(REC)
        guest = b.page('__core.model.page().viewer.guest')
        live = b.page("[...document.querySelectorAll('#nd-dash button:not([aria-disabled=true])')].map((x) => x.dataset.act)")
        check('a_guests_tap_posts_nothing: a real guest page draws the dash\'s keys inert (only the sheet\'s stays)',
              guest is True and live == ['sheet'], [guest, live])
        for one in ['#nd-vup', '#nd-vdown', '#nd-mute', '#nd-dash [data-act=input]']:
            click_el(b, one)
        time.sleep(0.6)
        posts = b.page('window.__posts')  # None: the page went to the sign-in, which only a request that went out brings
        writes = None if posts is None else [u for u, body, _ in posts if 'api/v1/devices/' in u and body]
        toast = b.page('document.body.innerText')
        check('  a tap on `+`, `−`, MUTE and the input posts nothing and the core says to sign in',
              writes == [] and 'Sign in' in toast, [writes, toast[-160:]])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


def dash_alpha(b):
    return b.page(f'{OPACITY}("#nd-dash")')


def dash_fades(b, d, label, corner):
    """The dash fades to a fifth with the HUD, a move wakes it, the press that wakes it writes nothing and the
    next one does, and it stays while a list of its own is open."""
    load_fading(b, d)
    lo = volume(d)['lo']
    start_at(b, d, lo + 20)
    b.page(REC)
    check(f'the_dash_fades_to_a_fifth_with_the_hud: {label}: shown at first', dash_alpha(b) == 1 and not b.page(FADED), dash_alpha(b))
    got = poll(lambda: dash_alpha(b) == 0.2 and b.page(f'{OPACITY}(".nd-top")') == 0, 12.0)
    check('  after the idle time it is at 0.2, the HUD faded with it', got and b.page(FADED),
          [dash_alpha(b), b.page(FADED), b.page(f'{OPACITY}(".nd-top")')])
    b.click(*corner)
    check('a_move_wakes_the_dash_at_once: the HUD and the dash are back', dash_alpha(b) == 1 and not b.page(FADED), [dash_alpha(b), b.page(FADED)])
    poll(lambda: dash_alpha(b) == 0.2, 12.0)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-vup', 'pointerdown')
    synth(b, '#nd-vup', 'pointerup', buttons=0)
    time.sleep(0.6)
    check('a_tap_on_a_faded_key_only_wakes_the_dash: nothing is posted and the dash is awake',
          volume_posts(b) == [] and dash_alpha(b) == 1 and volume(d)['raw'] == lo + 20, [volume_posts(b), dash_alpha(b), volume(d)['raw']])
    synth(b, '#nd-vup', 'pointerdown')
    synth(b, '#nd-vup', 'pointerup', buttons=0)
    check('  the next tap writes', poll(lambda: len(volume_posts(b)) == 1, 5.0) and settled(d, lo + 21), [volume_posts(b), volume(d)['raw']])
    poll(lambda: dash_alpha(b) == 0.2, 12.0)
    synth(b, '#nd-tr-1', 'pointerdown')
    synth(b, '#nd-tr-1', 'pointerup', buttons=0)
    synth(b, '#nd-tr-1', 'click')
    time.sleep(0.5)
    check('  the click of the tap that woke it presses no transport key', not any(u.endswith('/press') or '/press' in u for u, _, _ in b.page('window.__posts')),
          b.page('window.__posts'))
    # a press that woke the dash and is held a long time still presses nothing when it is let go
    poll(lambda: dash_alpha(b) == 0.2, 12.0)
    b.page('window.__posts.length = 0')
    synth(b, '#nd-tr-1', 'pointerdown')
    time.sleep(1.5)
    synth(b, '#nd-tr-1', 'pointerup', buttons=0)
    synth(b, '#nd-tr-1', 'click')
    time.sleep(0.2)
    check('a_long_press_that_woke_a_faded_dash_presses_nothing_when_let_go: no transport press after 1.5 s',
          not any('/press' in u for u, _, _ in b.page('window.__posts')), b.page('window.__posts'))
    synth(b, '#nd-tr-1', 'pointerdown')
    synth(b, '#nd-tr-1', 'pointerup', buttons=0)
    synth(b, '#nd-tr-1', 'click')
    check('  the next press does', poll(lambda: any('/press' in u for u, _, _ in b.page('window.__posts')), 4.0), b.page('window.__posts'))
    poll(lambda: dash_alpha(b) == 0.2, 12.0)
    # a list of the dash's own keeps it up while the HUD fades
    time.sleep(0.8)
    b.page("document.activeElement && document.activeElement.blur()")
    synth(b, '#nd-dash [data-act=input]', 'click')
    opened = poll(lambda: b.page("!!document.querySelector('.nd-list')"), 3.0)
    time.sleep(6.5)
    check('the_dash_stays_while_its_input_list_is_open: the list is open, the HUD faded, the dash at 1',
          opened and b.page(FADED) and dash_alpha(b) == 1, [opened, b.page(FADED), dash_alpha(b)])
    b.press('Escape')
    b.page("document.activeElement && document.activeElement.blur()")
    check('  once the list is closed (and the keyboard focus left) the dash fades again', poll(lambda: dash_alpha(b) == 0.2, 14.0), dash_alpha(b))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_dash_fade(b, d):
    dash_fades(b, d, '1280x900', (10, 10))


def the_dash_fade_sideways(d):
    b = make(size=(915, 412), dpr=2)
    try:
        dash_fades(b, d, '915x412', (10, 10))
    finally:
        b.close()


def the_dash_fade_portrait(d):
    b = make(size=(412, 915), dpr=2)
    try:
        load_fading(b, d)
        poll(lambda: b.page(FADED), 9.0)
        time.sleep(5.0)
        check('the_portrait_panel_never_fades: the HUD is faded and the panel stays at 1', b.page(FADED) and dash_alpha(b) == 1,
              [b.page(FADED), dash_alpha(b)])
    finally:
        b.close()


def the_dash_press_while_faded(b, d):
    """A real press from a desktop pointer: the move that comes first wakes the dash, so the press acts."""
    load_fading(b, d)
    poll(lambda: b.page(FADED), 6.0)
    time.sleep(2.7)
    lo = volume(d)['lo']
    start_at(b, d, lo + 20)
    b.page(REC)
    poll(lambda: dash_alpha(b) == 0.2, 12.0)
    b.page('window.__posts.length = 0')
    click_el(b, '#nd-vup')
    woke = [b.page(FADED), b.page(f'{OPACITY}(".nd-top")'), dash_alpha(b)]
    check('a_press_on_the_dash_while_faded_acts_and_wakes: a mouse arriving wakes it and its press on `+` posts the volume',
          poll(lambda: len(volume_posts(b)) == 1, 5.0) and settled(d, lo + 21), [volume_posts(b), volume(d)['raw']])
    check('  and shows the top again', woke == [False, 1, 1], woke)
    check('  no page error', errors(b) == [], errors(b)[:3])


SUN_SCAN = """(() => {
  const num = (c) => c.match(/[\\d.]+/g).map(Number);
  const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92
    : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const c = document.getElementById('nd-road');
  const w = c.clientWidth, h = c.clientHeight, H = h * 0.56, cy = H - h * 0.04, r = h * 0.22;
  const sun = { l: w / 2 - r, r: w / 2 + r, t: cy - r, b: Math.min(cy + r, H) };
  const dark = lum([0xff, 0x2d, 0x78]);
  const out = [];
  const right = document.querySelector('.nd-right');
  // the longest verdict, in the amber it has, whether or not the model has one now
  if (!right.textContent.trim()) { right.textContent = 'host EQ on \u00b7 changes the sound'; right.className = 'nd-right warn'; }
  for (const sel of ['.nd-left', '.nd-right', '.nd-eq', '.nd-novol']) {
    const e = document.querySelector(sel);
    if (!e || !e.textContent.trim()) continue;
    const q = e.getBoundingClientRect();
    if (!q.width) continue;
    const over = q.left < sun.r && q.right > sun.l && q.top < sun.b && q.bottom > sun.t;
    const l = lum(num(getComputedStyle(e).color));
    const ratio = (Math.max(l, dark) + 0.05) / (Math.min(l, dark) + 0.05);
    out.push({ sel, box: [q.left, q.top, q.right, q.bottom].map(Math.round), over, ratio: Math.round(ratio * 100) / 100 });
  }
  const vq = document.querySelector('.nd-vol svg')?.getBoundingClientRect();
  return { size: [w, h], sun: Object.values(sun).map(Math.round), out,
    volume: vq ? [vq.left, vq.top, vq.right, vq.bottom].map(Math.round) : null }; })()"""


def the_sun(b, d):
    forget(b, d)
    load(b, d)
    time.sleep(0.8)
    for w, h in ((1280, 800), (1280, 1600)):
        b.resize(w, h)
        time.sleep(0.8)
        got = b.page(SUN_SCAN)
        print(f'     {w}x{h}: sun (l, r, t, b) {got["sun"]}, volume box {got["volume"]},', [(o['sel'], o['box'], o['over'], o['ratio']) for o in got['out']])
        bad = [o for o in got['out'] if o['over'] and o['ratio'] < 4.5]
        check(f'the_hud_words_clear_the_sun_or_pass_contrast_there: {w}x{h}: every word clears the sun or reads at 4.5 on it',
              len(got['out']) >= 3 and got['size'] == [w, h] and bad == [], got)
    b.resize(1280, 900)


# ---- tuning on the mountains ----
TUNE = '__core.theme.driver.tune()'
TUNE_ON = "document.querySelector('.nd-scene').dataset.tune === 'on'"
SCENE_BOX = """(() => { const r = document.querySelector('.nd-scene').getBoundingClientRect();
  return [r.left, r.top]; })()"""
STEPS = '__core.model.page().device.tone_steps'


def mouse_to(b, x, y):
    """The pointer moves to (x, y) with no button down."""
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
    else:
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'},
              'actions': [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)}]}]})


def mouse_down(b, x, y):
    """Chromium only: the button goes down at (x, y) and stays down."""
    mouse_to(b, x, y)
    b.send('Input.dispatchMouseEvent', {'type': 'mousePressed', 'x': x, 'y': y, 'button': 'left',
                                        'clickCount': 1, 'buttons': 1})


def mouse_drag(b, x, y, path, away=None, hold=False):
    """A press at (x, y), moves through the points of `path` in steps of about 6 px, a release at the
    last one (unless `hold`: Chromium only, the button stays down) and a move on to `away`."""
    points = []
    px, py = x, y
    for tx, ty in path:
        n = max(1, int(math.hypot(tx - px, ty - py) // 6))
        points += [(px + (tx - px) * i / n, py + (ty - py) * i / n) for i in range(1, n + 1)]
        px, py = tx, ty
    if browser == 'chromium':
        mouse_down(b, x, y)
        for mx, my in points:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': mx, 'y': my, 'button': 'left', 'buttons': 1})
            b.sleep(0.02)
        if not hold:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': px, 'y': py, 'button': 'left',
                                                'clickCount': 1})
            if away:
                mouse_to(b, *away)
    else:
        move_to = lambda mx, my: {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(mx), 'y': int(my)}
        acts = [move_to(x, y), {'type': 'pointerDown', 'button': 0}] + [move_to(mx, my) for mx, my in points]
        acts.append({'type': 'pointerUp', 'button': 0})
        if away:
            acts.append(move_to(*away))
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'},
                                          'actions': acts}]})


def mouse_up(b, x, y):
    b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': x, 'y': y, 'button': 'left', 'clickCount': 1})






def tuned(b, d, query='?theme=nightdrive', over=None, flat=False, reset=True):
    """Reset the bands to known values (reset_bands), load the page, record its writes, and give the
    scene's size: (tune(), left, top)."""
    if reset:
        reset_bands(d, over, flat)
    forget(b, d)
    load(b, d, query)
    time.sleep(0.8)
    b.page(REC)
    left, top = b.page(SCENE_BOX)
    return b.page(TUNE), left, top






def expected_raws(b, t, band, x, y):
    """The steps a drag of `band` to (x, y) of the scene lands on, worked out here from the scene's
    size and the device's tables (the map: 20 Hz to 20 kHz across, 0 dB at H - 0.08h, 0.14h for the
    largest gain)."""
    steps = b.page(STEPS)
    gmax = max(abs(v) for v, _ in steps['gain'])
    hz = 20 * 1000 ** (x / t['w'])
    db = (t['H'] - 0.08 * t['h'] - y) * gmax / (0.14 * t['h'])
    return nearest_of(steps['bands'][band - 1]['f'], hz, log=True), nearest_of(steps['gain'], db)


def bubble(t, band):
    return next(p for p in t['bubbles'] if p['band'] == band)


def the_tuning_hover(b, d):
    t, left, top = tuned(b, d)
    w, h, H = t['w'], t['h'], t['H']
    check('hovering_the_mountains_shows_the_curve_and_leaving_lets_it_go: at rest the scene is the music',
          t['m'] == 0 and t['bubbles'] != [] and not b.page(TUNE_ON), t)
    mouse_to(b, left + w / 2, top + H - 0.1 * h)
    on = poll(lambda: b.page(TUNE_ON) and b.page(TUNE)['m'] > 0.95, 3.0)
    t = b.page(TUNE)
    check('  the pointer over the mountains: data-tune is on and the curve has come, with a bubble per band',
          on and len(t['bubbles']) == 5 and t['allowed'] is True, t)
    mouse_to(b, left + w / 2, top + 0.1 * h)
    off = poll(lambda: not b.page(TUNE_ON) and b.page(TUNE)['m'] < 0.02, 3.0)
    check('  the pointer back in the sky: data-tune goes and the mountains return to the music', off, b.page(TUNE))
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tuning_drag(b, d):
    t, left, top = tuned(b, d, flat=True)
    w, h, H = t['w'], t['h'], t['H']
    check('a_flat_device_switches_to_eq_on_the_first_drag: the device starts in Flat',
          d.model(DEVICE)['tone']['mode'] == 'flat')
    first = bubble(t, 2)
    mouse_to(b, left + first['x'], top + first['y'])
    poll(lambda: b.page(TUNE)['m'] >= 0.6, 3.0)
    time.sleep(0.2)
    # to a place that is neither the band's own frequency nor the 0 dB zone
    gx, gy = first['x'] + 170, first['y'] - 0.04 * h
    mouse_drag(b, left + first['x'], top + first['y'], [(left + gx, top + gy)],
               away=(left + w / 2, top + 0.1 * h))
    f_raw, g_raw = expected_raws(b, t, 2, gx, gy)
    landed = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][1][:2] == [f_raw, g_raw], 5.0)
    got = d.model(DEVICE)['tone']['readings']
    check('a_bubble_drag_sets_frequency_and_gain: the band lands on the steps nearest where it was dropped',
          landed, [got['bands'][1], [f_raw, g_raw]])
    posts = band_posts(b, DEVICE)
    last = {p[1]['param']: p[1]['raw'] for p in posts if p[0].endswith('/eq/bands/2')}
    check('  the last write of each parameter is that step, to band 2 of this device only',
          last.get('frequency') == f_raw and last.get('gain') == g_raw
          and all(p[0].endswith('/eq/bands/2') for p in posts) and last.get('q') is None, [last, posts[-3:]])
    check('  Flat becomes EQ (one toast says so)', poll(lambda: d.model(DEVICE)['tone']['mode'] == 'eq')
          and b.page('document.body.innerText').count('Mode set to EQ') == 1, d.model(DEVICE)['tone']['mode'])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tuning_grace(b, d):
    t, left, top = tuned(b, d)
    w, h, H = t['w'], t['h'], t['H']
    spot = bubble(t, 3)
    mouse_to(b, left + spot['x'], top + spot['y'])
    poll(lambda: b.page(TUNE)['m'] >= 0.6, 3.0)
    time.sleep(0.2)
    mouse_drag(b, left + spot['x'], top + spot['y'], [(left + spot['x'] + 60, top + spot['y'] - 20)],
               away=(left + w / 2, top + 0.1 * h))
    t0 = time.time()
    time.sleep(max(0, 1.2 - (time.time() - t0)))
    at12 = b.page(TUNE_ON)
    time.sleep(max(0, 2.5 - (time.time() - t0)))
    at25 = b.page(TUNE_ON)
    check('the_curve_stays_1_5_s_after_the_drag: the pointer left the mountains at the release: the curve is on at 1.2 s '
          'and off by 2.5 s', at12 is True and at25 is False, [at12, at25])


def the_tuning_label(b, d):
    t, left, top = tuned(b, d)
    spot = bubble(t, 4)
    mouse_to(b, left + spot['x'], top + spot['y'])
    poll(lambda: b.page(TUNE)['m'] >= 0.6, 3.0)
    time.sleep(0.2)
    gx, gy = spot['x'] - 90, spot['y'] - 30
    mouse_drag(b, left + spot['x'], top + spot['y'], [(left + gx, top + gy)], hold=True)
    time.sleep(0.4)
    now = b.page(TUNE)
    f_raw, g_raw = expected_raws(b, t, 4, gx, gy)
    steps = b.page(STEPS)
    q_raw = b.page(BANDS)[3][2]
    words = (f"{steps['bands'][3]['f'][f_raw][1]} · {steps['gain'][g_raw][1]} · Q {steps['q'][q_raw][1]}")
    box = b.page("""(() => { const l = document.querySelector('.nd-label'); if (!l) return null;
      const r = l.getBoundingClientRect(); const s = document.querySelector('.nd-scene').getBoundingClientRect();
      return { text: l.textContent, bottom: r.bottom - s.top, left: r.left - s.left, right: r.right - s.left,
        hidden: l.getAttribute('aria-hidden') }; })()""")
    held = bubble(now, 4)
    check('the_value_shows_above_the_bubble: while a bubble is held its label reads frequency, gain and Q from the steps\' words',
          now['held'] == 4 and box is not None and box['text'] == words, [now['held'], box, words])
    check('  the label sits above the held bubble and over it horizontally',
          box is not None and box['bottom'] <= held['y'] - 7 and box['left'] - 1 <= held['x'] <= box['right'] + 1,
          [box, held])
    check('  the label is hidden from the accessibility tree',
          box is not None and box['hidden'] == 'true')
    mouse_up(b, left + gx, top + gy)
    time.sleep(0.4)
    check('  the label goes with the release', b.page("document.querySelector('.nd-label')") is None,
          b.page(TUNE)['label'])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tuning_waits(b, d):
    """A reading that comes while a bubble is held does not move any bubble; it is drawn after."""
    t, left, top = tuned(b, d)
    h = t['h']
    one, three = bubble(t, 1), bubble(t, 3)
    mouse_to(b, left + one['x'], top + one['y'])
    poll(lambda: b.page(TUNE)['m'] >= 0.6, 3.0)
    time.sleep(0.2)
    mouse_drag(b, left + one['x'], top + one['y'], [(left + one['x'] + 20, top + one['y'] - 0.03 * h)],
               hold=True)
    time.sleep(0.3)
    steps = b.page(STEPS)
    zero = steps['gain'].index(next(g for g in steps['gain'] if g[0] == 0))
    other = zero + 6
    d.post(DEVICE, 'eq/bands/3', {'param': 'gain', 'raw': other})
    poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][1] == other, 5.0)
    time.sleep(0.5)
    mid = b.page(TUNE)
    check('a_tone_update_waits_for_the_drag: a write from elsewhere during the drag moves no bubble under the hand',
          bubble(mid, 3)['y'] == three['y'] and mid['held'] == 1, [bubble(mid, 3), three, mid['held']])
    mouse_up(b, left + one['x'] + 20, top + one['y'] - 0.03 * h)
    gmax = max(abs(v) for v, _ in steps['gain'])
    want = t['H'] - 0.08 * h - steps['gain'][other][0] * 0.14 * h / gmax
    after = poll(lambda: abs(bubble(b.page(TUNE), 3)['y'] - want) < 0.5, 5.0)
    check('  and after the release the bubble is drawn from the model\'s new reading', after,
          [bubble(b.page(TUNE), 3), want])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tuning_switch():
    """The drag on one device, a switch to another in the middle of it."""
    d = common.Daemon('gungnir2-v2', host='fake', fleet=[('bifrost3', NEW_DEVICE, '1112131415161718')])
    b = make()
    try:
        # the other device's band 1 is set apart, so the curve shown after the switch is known
        d.post(NEW_DEVICE, 'eq/bands/1', {'param': 'gain', 'raw': 110})
        poll(lambda: d.model(NEW_DEVICE)['tone']['readings']['bands'][0][1] == 110, 5.0)
        before = d.model(NEW_DEVICE)['tone']['readings']['bands']
        t, left, top = tuned(b, d)
        h = t['h']
        two = bubble(t, 2)
        mouse_to(b, left + two['x'], top + two['y'])
        poll(lambda: b.page(TUNE)['m'] >= 0.6, 3.0)
        time.sleep(0.2)
        mouse_drag(b, left + two['x'], top + two['y'], [(left + two['x'] + 60, top + two['y'] - 0.04 * h)],
                   hold=True)
        time.sleep(0.5)
        mid = band_posts(b, DEVICE)
        held_curve = b.page(TUNE)['curveY']
        check('a_device_switch_mid_drag_writes_nothing_to_the_new_device: the drag is on and has written to the first device',
              b.page(TUNE)['held'] == 2 and len(mid) >= 1, [b.page(TUNE)['held'], len(mid)])
        url = b.page(f"__core.model.page().fleet.devices.find((x) => x.name === '{NEW_DEVICE}').url")
        b.page(f"__core.actions.showDevice('{url}', {{ push: false }})")
        poll(lambda: b.page('__core.model.page().device.name') == NEW_DEVICE, 5.0)
        time.sleep(0.6)
        count = len(band_posts(b))
        # the button is still down: the pointer goes on moving over the new device's mountains
        for i in range(1, 8):
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': left + two['x'] + 60 + i * 12,
                                                'y': top + two['y'] - 0.04 * h - i * 6, 'button': 'left', 'buttons': 1})
            b.sleep(0.03)
        mouse_up(b, left + two['x'] + 150, top + two['y'] - 0.04 * h - 50)
        time.sleep(1.2)
        now = b.page(TUNE)
        check('  the drag ended with the switch: nothing is held and no label shows',
              now['held'] is None and now['label'] == '' and b.page("document.querySelector('.nd-label')") is None, now)
        check('  nothing was written to the device now shown, nor anywhere after the switch',
              band_posts(b, NEW_DEVICE) == [] and len(band_posts(b)) == count,
              [len(band_posts(b, NEW_DEVICE)), count, len(band_posts(b))])
        check('  the device now shown has the bands it had', d.model(NEW_DEVICE)['tone']['readings']['bands'] == before,
              d.model(NEW_DEVICE)['tone']['readings']['bands'])
        steps = b.page(STEPS)
        gmax = max(abs(v) for v, _ in steps['gain'])
        one = bubble(now, 1)
        want = now['H'] - 0.08 * h - steps['gain'][110][0] * 0.14 * h / gmax
        check('  the curve is drawn from the new device: five bubbles, its band 1 at its own gain, a curve unlike the held one',
              abs(one['y'] - want) < 0.5 and len(now['bubbles']) == 5 and len(now['curveY']) == 3
              and now['curveY'] != held_curve, [one, want, now['curveY'], held_curve])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_tuning_switch_idle():
    """A switch of device with no drag on gives the new device no curve."""
    d = common.Daemon('gungnir2-v2', host='fake', fleet=[('bifrost3', NEW_DEVICE, '1112131415161718')])
    b = make()
    try:
        t, left, top = tuned(b, d)
        mouse_to(b, left + t['w'] / 2, top + 0.1 * t['h'])
        time.sleep(0.5)
        url = b.page(f"__core.model.page().fleet.devices.find((x) => x.name === '{NEW_DEVICE}').url")
        b.page(f"__core.actions.showDevice('{url}', {{ push: false }})")
        poll(lambda: b.page('__core.model.page().device.name') == NEW_DEVICE, 5.0)
        seen = []
        for _ in range(6):
            time.sleep(0.15)
            seen.append([b.page(TUNE_ON), round(b.page(TUNE)['m'], 3)])
        check('a_device_switch_without_a_drag_shows_no_curve: the pointer in the sky, a switch of device: data-tune stays off '
              'and the morph stays at 0 (no grace without a drag)', seen == [[False, 0]] * 6, seen)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


def the_tuning_dpr_2(d):
    """A density of 2: a bubble is grabbed where it is drawn, in CSS px."""
    b = common.Chromium(size=(1000, 600), dpr=2)
    try:
        t, left, top = tuned(b, d)
        dims = b.page("(() => { const c = document.getElementById('nd-road'); return [c.width, c.clientWidth]; })()")
        two = bubble(t, 2)
        mouse_to(b, left + two['x'], top + two['y'])
        poll(lambda: b.page(TUNE)['m'] >= 0.95, 3.0)
        time.sleep(0.2)
        # the ring is drawn 7 px above the centre; the pixel there is the ring's cyan, the centre is the dark fill
        ring = b.page(f'({PIXEL})("nd-road", {two["x"]}, {two["y"] - 7})')
        centre = b.page(f'({PIXEL})("nd-road", {two["x"]}, {two["y"]})')
        check('a_bubble_is_grabbed_at_dpr_2: the canvas is at twice the CSS size and the bubble is drawn at its place in it',
              dims[0] == 2 * dims[1] and ring[1] > 200 and ring[2] > 220 and centre[0] < 40 and centre[1] < 90,
              [dims, ring, centre])
        mouse_down(b, left + two['x'] + 16, top + two['y'])
        time.sleep(0.2)
        far = b.page(TUNE)['held']
        mouse_up(b, left + two['x'] + 16, top + two['y'])
        mouse_down(b, left + two['x'] + 6, top + two['y'])
        time.sleep(0.2)
        near = b.page(TUNE)['held']
        mouse_up(b, left + two['x'] + 6, top + two['y'])
        spread = min((math.hypot(two['x'] + dx - d_['x'], two['y'] - d_['y']) for d_ in b.page(TUNE)['diamonds']
                      for dx in (6, 16)), default=0)
        check('  a press 6 px from the centre grabs it and one 16 px away (past the reach of 12) does not, '
              'with no diamond near either press',
              near == 2 and far is None and spread > 30, [near, far, spread])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


def the_tuning_guest():
    """A real guest: a tap on the mountains posts nothing and the core says why."""
    g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
    b = make()
    try:
        t, left, top = tuned(b, g, reset=False)
        w, h, H = t['w'], t['h'], t['H']
        mouse_to(b, left + w / 2, top + H - 0.1 * h)
        time.sleep(0.8)
        hover = b.page(TUNE)
        check('a_guest_tap_on_the_mountains_says_why: a guest never turns the mountains into the curve',
              b.page('__core.model.page().viewer.guest') is True and hover['m'] == 0 and not b.page(TUNE_ON)
              and hover['allowed'] is False, hover)
        spot = bubble(t, 2)
        b.click(left + spot['x'], top + spot['y'])
        time.sleep(0.8)
        posts = b.page('window.__posts')  # None: the page went to the sign-in, which only a request that went out brings
        writes = None if posts is None else [u for u, body, _ in posts if 'api/v1/devices/' in u and body]
        toast = b.page('document.body.innerText')
        check('  a tap on the mountains posts nothing and the core says to sign in',
              writes == [] and 'Sign in' in toast, [writes, toast[-160:]])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, g)
        finally:
            b.close()
            g.stop()


HALVES = """import(new URL('assets/themes/nightdrive/tune.js', document.baseURI)).then((m) => {
  const device = __core.model.page().device;
  const band = m.bandsOf(device.tone, device.tone_steps, new Map())[%(band)d - 1];
  const x = (hz) => %(w)s * Math.log(hz / 20) / Math.log(1000);
  return device.tone_steps.q.map(([q]) => { const p = m.halfPoints({ ...band, q }); return [x(p.lo), x(p.hi)]; }); })"""
BELL_AT = """import(new URL('assets/core/tone-curve.js', document.baseURI)).then((c) => {
  const device = __core.model.page().device;
  const read = device.tone.readings.bands[%(band)d - 1];
  const f0 = device.tone_steps.bands[%(band)d - 1].f[read[0]][0];
  const gainDb = device.tone_steps.gain[read[1]][0];
  const q = device.tone_steps.q[read[2]][0];
  return [gainDb, c.responseDb([{ f0, gainDb, q }], %(hz)s)]; })"""
STROKE = """((xs) => { const c = document.getElementById('nd-road'); const k = c.width / c.clientWidth;
  const g = c.getContext('2d'); return xs.map((x) => { const y = __core.theme.driver.curveAt(x); let pink = 0; let cyan = 0;
  for (let dy = -3; dy <= 3; dy++) { const p = g.getImageData(Math.round(x * k), Math.round((y + dy) * k), 1, 1).data;
    if (p[0] > 200 && p[1] > 110 && p[1] < 210 && p[2] > 190) pink++;
    if (p[0] < 110 && p[1] > 170 && p[2] > 190) cyan++; }
  return [pink, cyan]; }); })"""
PINK_EDGE = """((id, x0, x1, y0, y1) => { const c = document.getElementById(id); const k = c.width / c.clientWidth;
  const g = c.getContext('2d'); let n = 0;
  for (let x = x0; x <= x1; x += 2) for (let y = y0; y <= y1; y += 1) {
    const p = g.getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data;
    if (p[0] > 220 && p[1] > 120 && p[1] < 200 && p[2] > 200) n++; }
  return n; })"""


def check_highlight(name, b, lo, hi, w, bubbles):
    """The curve's own stroke is pink well inside lo..hi (past the 12 px fade) and its own colour well
    outside (the lit band's stretch only); a bubble's ring is skipped."""
    clear = lambda x: all(abs(x - p['x']) > 14 for p in bubbles)
    inside = [x for x in range(int(lo['x']) + 20, int(hi['x']) - 20, 6) if clear(x)]
    outside = [x for x in list(range(4, int(lo['x']) - 30, 6)) + list(range(int(hi['x']) + 30, int(w) - 4, 6))
               if clear(x)]
    # one read of the canvas: a pointer that rests lets the curve go after 1.5 s
    got = b.page(f'({STROKE})({json.dumps(inside + outside)})')
    pink, away = got[:len(inside)], got[len(inside):]
    check(f'{name}: pink inside lo..hi, the curve\'s own colour well outside',
          len(inside) >= 5 and len(outside) >= 5 and all(p[0] >= 1 and p[1] == 0 for p in pink)
          and all(p[0] == 0 and p[1] >= 1 for p in away),
          [[(x, p) for x, p in zip(inside, pink) if not (p[0] >= 1 and p[1] == 0)][:5],
           [(x, p) for x, p in zip(outside, away) if not (p[0] == 0 and p[1] >= 1)][:5], lo['x'], hi['x']])


def wheel_at(b, x, y, dy, settle=0.4):
    """One wheel notch at (x, y): the pointer moves there first."""
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y})
        b.send('Input.dispatchMouseEvent', {'type': 'mouseWheel', 'x': x, 'y': y, 'deltaX': 0, 'deltaY': dy})
    else:
        b._post('/actions', {'actions': [{'type': 'wheel', 'id': 'wheel', 'actions': [
            {'type': 'scroll', 'x': int(x), 'y': int(y), 'deltaX': 0, 'deltaY': int(dy),
             'duration': 0, 'origin': 'viewport'}]}]})
    time.sleep(settle)


def lit_tuning(b, d, query='?theme=nightdrive', over=None):
    """The page loaded with band 3 a wide bell (+6 dB, Q 1; `over` for another), the pointer on its
    bubble with the curve up: (tune(), left, top, gain raw, q raw)."""
    t, left, top = tuned(b, d, query, over={3: (6, 1)} if over is None else over)
    gain, q = d.model(DEVICE)['tone']['readings']['bands'][2][1:3]
    spot = bubble(t, 3)
    mouse_to(b, left + spot['x'], top + spot['y'])
    poll(lambda: b.page(TUNE)['m'] >= 0.95 and b.page(TUNE)['lit'] == 3, 3.0)
    time.sleep(0.2)
    return b.page(TUNE), left, top, gain, q


def the_q_hover(b, d):
    t, left, top, gain, q = lit_tuning(b, d)
    spot, h = bubble(t, 3), t['h']
    lo, hi = t['diamonds']
    hz = [20 * 1000 ** (lo['x'] / t['w']), 20 * 1000 ** (hi['x'] / t['w'])]
    db, at = b.page(BELL_AT % {'band': 3, 'hz': json.dumps(hz)})
    check('hovering_a_bubble_lights_its_bell_and_diamonds: the band is lit with a diamond each side of its bubble',
          t['lit'] == 3 and [x['side'] for x in t['diamonds']] == ['lo', 'hi'] and lo['x'] < spot['x'] < hi['x'],
          [t['lit'], t['diamonds'], spot])
    on_curve = [b.page(f'__core.theme.driver.curveAt({x["x"]})') for x in (lo, hi)]
    check('  the diamonds sit ON the curve, and are the band alone at half its gain, from the core\'s own '
          'response, at their frequencies',
          abs(lo['y'] - on_curve[0]) < 0.5 and abs(hi['y'] - on_curve[1]) < 0.5
          and all(abs(a - db / 2) < 0.1 for a in at) and abs(db) > 3, [lo, hi, on_curve, db, at])
    ring = [b.page(f'({PINK_EDGE})("nd-road", {d_["x"] - 1}, {d_["x"] + 1}, {d_["y"] - 9}, {d_["y"] + 9})') for d_ in (lo, hi)]
    check('  both diamonds are drawn in pink on the road canvas', all(n >= 2 for n in ring), ring)
    check_highlight('the_highlight_is_the_curve_in_pink_between_the_diamonds', b, lo, hi, t['w'], t['bubbles'])
    other = bubble(t, 5)
    mouse_to(b, left + other['x'], top + other['y'])
    poll(lambda: b.page(TUNE)['lit'] == 5, 3.0)
    check('  hovering another bubble lights that band instead', b.page(TUNE)['lit'] == 5, b.page(TUNE)['lit'])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_flat_band_highlight(b, d):
    """A band at 0 dB highlights the analog width around its bubble on the curve."""
    t, left, top, gain, q = lit_tuning(b, d, over={3: (0, 1)})
    lo, hi = t['diamonds']
    spot = bubble(t, 3)
    check('a_flat_band_highlights_the_analog_width_around_its_bubble: lit with a diamond each side of its bubble',
          t['lit'] == 3 and lo['x'] < spot['x'] < hi['x'] and hi['x'] - lo['x'] > 40, [t['lit'], t['diamonds'], spot])
    check_highlight('  the highlight is pink between them', b, lo, hi, t['w'], t['bubbles'])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_tuning_range(d):
    """The map's span of 0.14h: the bubble at the largest gain stays inside the scene and clear of the
    top line, at a desktop and a phone held upright; the smallest dips onto the floor, inside the scene."""
    for label, mk in (('1280x800', lambda: make(size=(1280, 800))),
                      ('390x844', lambda: common.Chromium(size=(390, 844), touch=True))):
        if label != '1280x800' and browser != 'chromium':
            continue
        b = mk()
        try:
            for gain, name in ((99, 'largest'), (-99, 'smallest')):
                t, left, top = tuned(b, d, over={3: (gain, 1)})
                spot = bubble(t, 3)
                rects = b.page("""(() => { const s = document.querySelector('.nd-scene').getBoundingClientRect();
                  const t = document.querySelector('.nd-top').getBoundingClientRect();
                  return { sceneTop: s.top, topBottom: t.bottom - s.top }; })()""")
                if gain > 0:
                    ok = 7 + 4 <= spot['y'] and spot['y'] - 7 > rects['topBottom'] + 8
                else:
                    ok = spot['y'] + 7 < t['h'] and spot['y'] > t['H']
                check(f'the_tuning_range_fits_the_scene: {label}, the bubble at the {name} gain is at y={spot["y"]:.0f} '
                      f'of {t["h"]:.0f} (horizon {t["H"]:.0f}, top line ends {rects["topBottom"]:.0f})',
                      ok, [spot, t['H'], t['h'], rects])
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            try:
                forget(b, d)
            finally:
                b.close()


def the_resting_pointer(b, d):
    """A pointer that rests on the mountains lets the curve go after 1.5 s; a move brings it back."""
    t, left, top = tuned(b, d)
    w, h, H = t['w'], t['h'], t['H']
    x, y = left + w / 2, top + H - 0.1 * h
    mouse_to(b, x, y)
    t0 = time.time()
    on = poll(lambda: b.page(TUNE_ON), 1.0)
    check('a_resting_pointer_lets_the_curve_go: a move over the mountains turns the curve on at once', on is True, on)
    time.sleep(max(0, 1.0 - (time.time() - t0)))
    at10 = b.page(TUNE_ON)
    time.sleep(max(0, 2.5 - (time.time() - t0)))
    gone = b.page(TUNE_ON)
    settled = poll(lambda: b.page(TUNE)['m'] == 0, 3.0)
    check('  resting there, it is still on at 1.0 s and gone by 2.5 s, with the pointer in the same place',
          at10 is True and gone is False and settled, [at10, gone, b.page(TUNE)['m']])
    mouse_to(b, x + 1, y)
    back = poll(lambda: b.page(TUNE_ON), 1.0)
    check('  a 1 px move brings it back', back is True, back)
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_q_drag(b, d):
    t, left, top, gain, q = lit_tuning(b, d)
    w, h = t['w'], t['h']
    xs = b.page(HALVES % {'band': 3, 'w': w})
    hi = t['diamonds'][1]
    # a step wider than the one now, whose upper half point is on the scene and clear of the bubble
    pick = next(j for j in (q - 2, q + 2, q - 1, q + 1)
                if 0 <= j < len(xs) and j != q and hi['x'] > 0 and bubble(t, 3)['x'] + 25 < xs[j][1] < w - 15)
    mouse_drag(b, left + hi['x'], top + hi['y'], [(left + xs[pick][1], top + hi['y'])],
               away=(left + w / 2, top + 0.1 * h))
    t0 = time.time()
    landed = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][2] == pick, 5.0)
    got = d.model(DEVICE)['tone']['readings']['bands'][2]
    posts = band_posts(b, DEVICE)
    params = {p[1]['param'] for p in posts}
    check('dragging_a_diamond_sets_q: dropped at the half point of a step the band takes that step',
          landed and got[1] == gain, [got, pick, q])
    check('  only Q is written, to band 3 of this device, the last write being that step',
          posts != [] and params == {'q'} and all(p[0].endswith('/eq/bands/3') for p in posts)
          and posts[-1][1].get('raw') == pick, [posts[-3:], pick])
    time.sleep(max(0, 1.2 - (time.time() - t0)))
    at12 = b.page(TUNE_ON)
    time.sleep(max(0, 2.6 - (time.time() - t0)))
    check('  the curve stays 1.5 s after the diamond is let go: on at 1.2 s, off by 2.6 s with the pointer away',
          at12 is True and b.page(TUNE_ON) is False, [at12, b.page(TUNE_ON)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_q_hold(b, d):
    """Chromium: a diamond held keeps the curve and says what it is."""
    t, left, top, gain, q = lit_tuning(b, d)
    w, h = t['w'], t['h']
    xs = b.page(HALVES % {'band': 3, 'w': w})
    hi = t['diamonds'][1]
    pick = next(j for j in (q - 2, q + 2, q - 1, q + 1)
                if 0 <= j < len(xs) and j != q and bubble(t, 3)['x'] + 25 < xs[j][1] < w - 15)
    mouse_drag(b, left + hi['x'], top + hi['y'], [(left + xs[pick][1], top + hi['y'])], hold=True)
    time.sleep(0.4)
    mid = b.page(TUNE)
    # the pointer leaves the mountains' band with the button down: still held, still the curve
    for i in range(1, 6):
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': left + xs[pick][1],
                                            'y': top + hi['y'] - i * 0.06 * h, 'button': 'left', 'buttons': 1})
        b.sleep(0.03)
    time.sleep(2.5)
    held = b.page(TUNE)
    check('  a held diamond is a held drag: the side is hi, the label says the Q, and the curve and the full rate '
          'stay for 2.5 s with the pointer out of the mountains\' band',
          mid['held'] == 3 and mid['heldSide'] == 'hi' and 'Q' in mid['label'] and held['held'] == 3
          and b.page(TUNE_ON) is True and held['m'] > 0.95 and b.page('__core.theme.driver.perf().pace') == 0,
          [mid['held'], mid['heldSide'], mid['label'], held['held'], held['m']])
    mouse_up(b, left + xs[pick][1], top + hi['y'] - 5 * 0.06 * h)
    time.sleep(0.4)
    check('  the release ends it: nothing held, no label', b.page(TUNE)['held'] is None
          and b.page("document.querySelector('.nd-label')") is None, b.page(TUNE)['label'])


def the_q_beside(b, d):
    """Chromium: at the narrowest Q a diamond lies beside its bubble; a press on the bubble's centre
    takes the bubble (the nearest point wins, a bubble before a diamond at the same distance)."""
    t, left, top, gain, q = lit_tuning(b, d, over={3: (0, 1e9)})
    h = t['h']
    spot = bubble(t, 3)
    gap = min(math.hypot(spot['x'] - d_['x'], spot['y'] - d_['y']) for d_ in t['diamonds'])
    mouse_drag(b, left + spot['x'], top + spot['y'], [(left + spot['x'] + 60, top + spot['y'] - 0.04 * h)],
               away=(left + t['w'] / 2, top + 0.1 * h))
    time.sleep(0.6)
    posts = band_posts(b, DEVICE)
    params = {p[1]['param'] for p in posts}
    check('a_press_on_the_bubble_takes_the_bubble_beside_a_diamond: a diamond lies within 12 px of the bubble, '
          'a press on its centre moves the band (frequency or gain) and never writes the Q',
          gap < 12 and posts != [] and params <= {'frequency', 'gain'} and params != set()
          and all(p[0].endswith('/eq/bands/3') for p in posts)
          and d.model(DEVICE)['tone']['readings']['bands'][2][2] == q, [gap, posts[-3:], q])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_q_dpr_2(d):
    """A density of 2: a diamond is drawn and grabbed at the same CSS px."""
    b = common.Chromium(size=(1000, 600), dpr=2)
    try:
        t, left, top, gain, q = lit_tuning(b, d)
        dims = b.page("(() => { const c = document.getElementById('nd-road'); return [c.width, c.clientWidth]; })()")
        lit = len(t['diamonds']) == 2
        check('a_diamond_is_grabbed_at_dpr_2: the pointer at the bubble, in CSS px, lights its bell and its two diamonds',
              lit, t['diamonds'])
        if not lit:
            return
        hi = t['diamonds'][1]
        edge = b.page(f'({PINK_EDGE})("nd-road", {hi["x"] - 1}, {hi["x"] + 1}, {hi["y"] - 9}, {hi["y"] + 9})')
        centre = b.page(f'({PIXEL})("nd-road", {hi["x"]}, {hi["y"]})')
        check('a_diamond_is_grabbed_at_dpr_2: the canvas is at twice the CSS size and the diamond is drawn at its place in it '
              '(a dark centre under its pink edge)',
              dims[0] == 2 * dims[1] and edge >= 2 and centre[0] < 120 and centre[1] < 40 and centre[2] < 130,
              [dims, edge, centre])
        mouse_down(b, left + hi['x'] + 16, top + hi['y'])
        time.sleep(0.2)
        far = [b.page(TUNE)['held'], b.page(TUNE)['heldSide']]
        mouse_up(b, left + hi['x'] + 16, top + hi['y'])
        time.sleep(0.2)
        mouse_down(b, left + hi['x'] + 6, top + hi['y'])
        time.sleep(0.2)
        near = [b.page(TUNE)['held'], b.page(TUNE)['heldSide']]
        mouse_up(b, left + hi['x'] + 6, top + hi['y'])
        check('  a press 6 px from the diamond grabs it and one 16 px away (past the reach of 12) does not',
              near == [3, 'hi'] and far == [None, None], [near, far])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()


def the_q_wheel(b, d):
    t, left, top, gain, q = lit_tuning(b, d)
    h = t['h']
    steps = b.page(STEPS)
    spot = bubble(t, 3)
    before = b.page(BANDS)
    wheel_at(b, left + spot['x'] + 10, top + spot['y'], -100)
    up = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][2] == q + 1, 5.0)
    posts = band_posts(b, DEVICE)
    check('the_wheel_steps_the_nearest_bands_q: a notch up over band 3\'s bubble narrows it one step, band 3 only',
          up and len(posts) == 1 and posts[0][0].endswith('/eq/bands/3')
          and posts[0][1] == {'param': 'q', 'step': 1}
          and d.model(DEVICE)['tone']['readings']['bands'][:2] == before[:2], [posts, before])
    wheel_at(b, left + spot['x'] + 10, top + spot['y'], 100)
    down = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][2] == q, 5.0)
    posts = band_posts(b, DEVICE)
    check('  a notch down widens it one step', down and len(posts) == 2 and posts[1][1] == {'param': 'q', 'step': -1},
          posts)
    # past the reach of every bubble the band nearest in x takes it
    last = max(t['bubbles'], key=lambda p: p['x'])
    wheel_at(b, left + min(last['x'] + 40, t['w'] - 4), top + t['H'] - 0.01 * h, -100)
    far = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][last['band'] - 1][2]
               == before[last['band'] - 1][2] + 1, 5.0)
    posts = band_posts(b, DEVICE)
    check('  with no bubble near, the band nearest in x takes the notch',
          far and len(posts) == 3 and posts[2][0].endswith(f"/eq/bands/{last['band']}")
          and posts[2][1] == {'param': 'q', 'step': 1}, [posts[2:], last])
    before_sky = len(band_posts(b))
    wheel_at(b, left + spot['x'], top + 0.1 * h, -100)
    time.sleep(0.4)
    check('  a wheel in the sky is the page\'s: nothing is written', len(band_posts(b)) == before_sky,
          band_posts(b)[before_sky:])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_q_wheel_grace(b, d):
    t, left, top, gain, q = lit_tuning(b, d)
    spot = bubble(t, 3)
    wheel_at(b, left + spot['x'] + 10, top + spot['y'], -100, settle=0)
    mouse_to(b, left + t['w'] / 2, top + 0.1 * t['h'])
    t0 = time.time()
    time.sleep(1.0)
    at10 = b.page(TUNE_ON)
    time.sleep(max(0, 2.6 - (time.time() - t0)))
    at26 = b.page(TUNE_ON)
    check('the_wheel_and_the_pinch_keep_the_curve_1_5_s: after a wheel notch with the pointer gone to the sky '
          'the curve is on at 1.0 s and off by 2.6 s', at10 is True and at26 is False, [at10, at26])


def fingers_at(b, points):
    """Chromium: the fingers `points` ((id, x, y) each) are down, or move there."""
    return [{'x': x, 'y': y, 'id': i} for i, x, y in points]


def touch_send(b, kind, points):
    b.send('Input.dispatchTouchEvent', {'type': kind, 'touchPoints': fingers_at(b, points)})


def the_q_pinch():
    """Chromium, a phone: two fingers set the Q of the band nearest their middle; a first touch only
    shows the curve and a second grabs."""
    d = common.Daemon('gungnir2-v2', host='fake')
    b = common.Chromium(size=(390, 844), touch=True,
                        args=['--disable-features=OverscrollHistoryNavigation'])
    try:
        t, left, top = tuned(b, d, over={3: (6, 1)})
        gain, q = d.model(DEVICE)['tone']['readings']['bands'][2][1:3]
        w, h, H = t['w'], t['h'], t['H']
        steps = b.page(STEPS)
        spot = bubble(t, 3)
        # a first touch on the mountains shows the curve and nothing more
        touch_send(b, 'touchStart', [(1, left + spot['x'], top + spot['y'])])
        time.sleep(0.15)
        down = b.page(TUNE)['held']
        touch_send(b, 'touchEnd', [])
        shown = poll(lambda: b.page(TUNE_ON) is True, 2.0)
        first = b.page(TUNE)
        check('a_first_touch_shows_the_curve_and_a_second_grabs: a touch on the mountains while they are the music '
              'brings the curve and grabs nothing (not while the finger is down either)',
              shown and down is None and first['held'] is None and band_posts(b) == [],
              [shown, down, first['held'], band_posts(b)])
        poll(lambda: b.page(TUNE)['m'] >= 0.6, 2.0)
        touch_send(b, 'touchStart', [(1, left + spot['x'], top + spot['y'])])
        time.sleep(0.2)
        second = b.page(TUNE)
        touch_send(b, 'touchEnd', [])
        check('  a second touch, once the curve is up, grabs the bubble', second['held'] == 3 and second['m'] >= 0.5,
              [second['held'], second['m']])
        time.sleep(0.3)
        check('  neither wrote anything (no movement)', band_posts(b) == [], band_posts(b))
        # a finger on a diamond while the curve is still up: held, and the curve stays well past the grace
        lit = b.page(TUNE)
        xs = b.page(HALVES % {'band': 3, 'w': w})
        hi = lit['diamonds'][1] if lit['diamonds'] else None
        pick = next((j for j in (q - 2, q + 2, q - 1, q + 1)
                     if hi and 0 <= j < len(xs) and j != q and spot['x'] + 25 < xs[j][1] < w - 8), None)
        touch_send(b, 'touchStart', [(1, left + hi['x'], top + hi['y'])])
        time.sleep(2.2)
        held = b.page(TUNE)
        check('  a finger held on a lit bell\'s diamond is a held drag: the curve and the band stay lit past the grace',
              hi is not None and held['held'] == 3 and held['heldSide'] == 'hi' and b.page(TUNE_ON) is True
              and held['m'] > 0.95, [lit['lit'], hi, held['held'], held['heldSide'], held['m']])
        touch_send(b, 'touchMove', [(1, left + xs[pick][1], top + hi['y'])])
        landed = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][2] == pick, 5.0)
        touch_send(b, 'touchEnd', [])
        check('  and dragged to a step\'s half point it writes that step\'s Q (a touch, the diamond grabbed from the lit bell)',
              pick is not None and landed, [pick, d.model(DEVICE)['tone']['readings']['bands'][2]])
        reset_bands(d, {3: (6, 1)}, b=b)
        # the pinch: the middle by band 3's x, below its bubble (the band nearest in x)
        y = top + H + 0.05 * h
        q0 = steps['q'][q][0]
        want = nearest_of(steps['q'], q0 / 2)
        before = d.model(DEVICE)['tone']['readings']['bands'][:]
        cx = left + spot['x']
        time.sleep(1.8)
        touch_send(b, 'touchStart', [(1, cx - 15, y), (2, cx + 15, y)])
        time.sleep(0.1)
        for k in range(1, 7):
            touch_send(b, 'touchMove', [(1, cx - 15 - 2.5 * k, y), (2, cx + 15 + 2.5 * k, y)])
            time.sleep(0.03)
        landed = poll(lambda: d.model(DEVICE)['tone']['readings']['bands'][2][2] == want, 6.0)
        posts = band_posts(b, DEVICE)
        after = d.model(DEVICE)['tone']['readings']['bands']
        check('a_pinch_sets_the_q_of_the_band_nearest_the_fingers: fingers moving to twice their distance halve the Q '
              'of band 3 (nearest in x, no bubble near their middle) within a step',
              landed and posts != [] and {p[1]['param'] for p in posts} == {'q'}
              and all(p[0].endswith('/eq/bands/3') for p in posts) and posts[-1][1].get('raw') == want
              and after[2][:2] == before[2][:2] and after[:2] == before[:2] and after[3:] == before[3:],
              [posts[-3:], want, before, after])
        mid = b.page(TUNE)
        check('  the pinch is a held drag of band 3 while the fingers are down', mid['held'] == 3, mid['held'])
        went = len(posts)
        touch_send(b, 'touchEnd', [])
        t0 = time.time()
        time.sleep(0.5)
        check('  lifting the fingers ends it', b.page(TUNE)['held'] is None, b.page(TUNE)['held'])
        time.sleep(max(0, 1.0 - (time.time() - t0)))
        at10 = b.page(TUNE_ON)
        time.sleep(max(0, 2.6 - (time.time() - t0)))
        at26 = b.page(TUNE_ON)
        check('  the curve stays 1.5 s after the lifted fingers: on at 1.0 s and gone by 2.6 s',
              at10 is True and at26 is False and len(band_posts(b, DEVICE)) == went, [at10, at26])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()


SUN_COLUMN = """((id, x, y0, y1) => { const c = document.getElementById(id); const k = c.width / c.clientWidth;
  const g = c.getContext('2d'); const out = [];
  for (let y = Math.round(y0 * k); y <= Math.round(y1 * k); y++) out.push([...g.getImageData(Math.round(x * k), y, 1, 1).data]);
  return out; })"""


def the_sun_reaches_the_horizon(d):
    """A band at its lowest gain lets the ridge dip to the horizon: the sun, in the sky canvas under
    the mountains, is warm all the way down to the horizon (no dark gap in its foot, the lowest 0.045h),
    at two window heights and two densities; higher up it is striped."""
    cases = [(1280, 900, 1), (1280, 560, 1)] + ([(1280, 900, 2)] if browser == 'chromium' else [])
    for width, height, dpr in cases:
        b = common.Chromium(size=(width, height), dpr=dpr) if browser == 'chromium' else make(size=(width, height))
        try:
            t, left, top = tuned(b, d, over={3: (-99, 1)})
            two = bubble(t, 3)
            mouse_to(b, left + two['x'], top + t['H'] - 0.1 * t['h'])
            poll(lambda: b.page(TUNE)['m'] >= 0.95, 3.0)
            time.sleep(0.3)
            t = b.page(TUNE)
            two = bubble(t, 3)
            H = t['H']
            col = b.page(f'({SUN_COLUMN})("nd-sky", {two["x"]}, {H - 0.045 * t["h"]}, {H - 2})')
            cold = [p for p in col if not (p[0] > 200 and p[1] > 80 and p[2] < 130)]
            check(f'the_sun_reaches_the_horizon: {width}x{height} dpr {dpr}, the sky under the dip at x={two["x"]:.0f} is '
                  f'sun-coloured from 0.045h above the horizon to 2 CSS px above it', len(col) > 10 and cold == [], [len(cold), cold[:3]])
            high = b.page(f'({SUN_COLUMN})("nd-sky", {t["w"] / 2}, {H - 0.25 * t["h"]}, {H - 0.05 * t["h"]})')
            runs = 0
            was = False
            for p in high:
                dark = p[0] < 130 and p[1] < 40
                runs += dark and not was
                was = dark
            check('  higher up the sun is striped: five dark stripes between 0.25h and 0.05h above the horizon', runs == 5, runs)
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            b.close()


def the_readout(d):
    """The readout and the bisect switches the address asks for: no box without `nd-debug`; with it a box
    that is aria-hidden, says the draws a second and the scene's numbers, and is drawn twice a second
    and no more; each switch turns off its layer, and the query is still there after the load."""
    b = make(size=(412, 915))
    try:
        forget(b, d)
        load(b, d)
        check('the_readout_is_not_made_without_the_parameter',
              b.page("document.querySelector('.nd-debug') === null") is True)
        load(b, d, '?theme=nightdrive&nd-debug')
        time.sleep(1.5)
        box = b.page("""(() => { const e = document.querySelector('.nd-debug');
          return e && [e.textContent, e.getAttribute('aria-hidden'), getComputedStyle(e).position, location.search]; })()""")
        check('the_readout_shows_the_numbers_and_hides_from_the_tree_and_the_query_stays',
              bool(box) and box[1] == 'true' and box[2] == 'absolute' and box[3] == '?theme=nightdrive&nd-debug'
              and all(w in box[0] for w in ('draws/s', 'ms mean', 'p95', 'canvas ', 'scale ', 'capped ', 'motion full', 'long tasks 10 s:', 'bass ')), box)
        b.page("""(() => { window.__writes = 0; const e = document.querySelector('.nd-debug');
          new MutationObserver((l) => { window.__writes += l.length; }).observe(e, { childList: true, characterData: true, subtree: true });
          return 1; })()""")
        time.sleep(3.0)
        writes = b.page('window.__writes')
        check('  the box is drawn twice a second, never more: 4 to 8 times in 3 s', 4 <= writes <= 8, writes)
        load(b, d, '?theme=nightdrive&nd-debug=nosky,nodash,nohud,noglow')
        time.sleep(1.0)
        got = b.page(f"""(() => ({{ sky: ({PIXEL})('nd-sky', 206, 40)[3], dash: getComputedStyle(document.getElementById('nd-dash')).display,
          top: getComputedStyle(document.querySelector('.nd-top')).display,
          glow: getComputedStyle(document.querySelector('.nd-chip, .nd-k')).boxShadow,
          off: document.querySelector('.nd-debug').textContent.split('\\n').pop() }}))()""")
        check('nosky_nodash_nohud_and_noglow_turn_off_what_they_name',
              got['sky'] == 0 and got['dash'] == 'none' and got['top'] == 'none' and got['glow'] == 'none'
              and got['off'] == 'off: nosky nodash nohud noglow', got)
        load(b, d, '?theme=nightdrive&nd-debug=nofloor,noridge,nohaze,noembers')
        time.sleep(1.0)
        floor = b.page(f"""(() => {{ const c = document.getElementById('nd-road'); const g = c.getContext('2d');
          const d = g.getImageData(0, 0, c.width, c.height).data; let n = 0;
          for (let i = 3; i < d.length; i += 4) if (d[i] > 0) n++; return n; }})()""")
        check('nofloor_noridge_nohaze_and_noembers_leave_the_road_canvas_empty', floor == 0, floor)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
    if browser == 'chromium':
        b = common.Chromium(size=(412, 915), dpr=2)
        try:
            load(b, d, '?theme=nightdrive&nd-debug=dpr1')
            time.sleep(1.0)
            dims = b.page("(() => { const c = document.getElementById('nd-road'); return [c.width, c.clientWidth, c.height, c.clientHeight]; })()")
            check('dpr1_draws_at_one_backing_pixel_per_css_pixel', dims[0] == dims[1] and dims[2] == dims[3], dims)
        finally:
            b.close()




COVER_SPAN = """(() => { const dash = document.getElementById('nd-dash'); const c = dash.querySelector('.nd-cover');
  const r = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom, b.width, b.height]; };
  return c && { cover: r(c), title: r(dash.querySelector('.nd-title')), bar: r(dash.querySelector('.nd-prog')) }; })()"""


def the_cover_size(d):
    """The cover spans the track's block: its top at the top of the title's text box and its bottom at the
    bottom of the progress bar, square, on a desktop and on a phone."""
    for size in [(1280, 800), (412, 915)]:
        b = make(size=size)
        try:
            forget(b, d)
            load(b, d)
            poll(lambda: b.page(COVER)['loaded'], 6.0)
            time.sleep(0.5)
            c = b.page(COVER_SPAN)
            check(f'the_cover_spans_the_title_to_the_bar: {size[0]}x{size[1]}, its top within 2 px of the title\'s text box top, '
                  f'its bottom within 2 px of the bar\'s bottom, and square',
                  bool(c) and abs(c['cover'][1] - c['title'][1]) <= 2 and abs(c['cover'][3] - c['bar'][3]) <= 2
                  and abs(c['cover'][4] - c['cover'][5]) <= 1, c)
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            b.close()


CURSOR = "((x, y) => getComputedStyle(document.elementFromPoint(x, y)).cursor)"


def the_cursor_rest(d):
    """Over the scene the mouse cursor hides after 2 s without a move and comes back on the next move; never
    over the dash; not while a button is held on the mountains (Chromium)."""
    b = make(size=(1280, 800))
    try:
        t, left, top = tuned(b, d)
        w, h, H = t['w'], t['h'], t['H']
        sky = (left + 0.2 * w, top + 0.3 * h)
        mouse_to(b, *sky)
        time.sleep(0.5)
        check('the_cursor_hides_when_it_rests: a moment after a move over the scene it is not hidden',
              b.page(f'({CURSOR})({sky[0]}, {sky[1]})') != 'none', b.page(f'({CURSOR})({sky[0]}, {sky[1]})'))
        time.sleep(1.7)
        check('  2.2 s after the move it is none', b.page(f'({CURSOR})({sky[0]}, {sky[1]})') == 'none',
              b.page(f'({CURSOR})({sky[0]}, {sky[1]})'))
        mouse_to(b, sky[0] + 4, sky[1])
        time.sleep(0.2)
        check('  the next move brings it back', b.page(f'({CURSOR})({sky[0] + 4}, {sky[1]})') != 'none',
              b.page(f'({CURSOR})({sky[0] + 4}, {sky[1]})'))
        key = b.page("""(() => { const r = document.querySelector('#nd-dash .nd-k').getBoundingClientRect();
          return [r.left + r.width / 2, r.top + r.height / 2]; })()""")
        mouse_to(b, *key)
        time.sleep(2.6)
        check('  over the dash it is never none, however long the pointer rests',
              b.page(f'({CURSOR})({key[0]}, {key[1]})') != 'none', b.page(f'({CURSOR})({key[0]}, {key[1]})'))
        mouse_to(b, sky[0], sky[1])
        time.sleep(2.4)
        away = b.page(f'({CURSOR})({sky[0]}, {sky[1]})')
        mouse_to(b, *key)
        time.sleep(0.2)
        check('  and it hid over the scene again, then the dash had it back', away == 'none'
              and b.page(f'({CURSOR})({key[0]}, {key[1]})') != 'none', away)
        word = b.page("""(() => { const r = document.querySelector('.nd-left').getBoundingClientRect();
          return [r.left + r.width / 2, r.top + r.height / 2]; })()""")
        mouse_to(b, *word)
        time.sleep(2.6)
        seen = b.page(f'({CURSOR})({word[0]}, {word[1]})')
        check('  over a word of the HUD (click-through, so the scene is under it) it is not none either', seen != 'none', seen)
        two = bubble(t, 3)
        bx, by = left + two['x'], top + two['y']
        if browser == 'chromium':
            mouse_drag(b, bx, by, [(bx + 8, by - 8)], hold=True)
        else:
            move_to = lambda mx, my: {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(mx), 'y': int(my)}
            b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'},
                    'actions': [move_to(bx, by), {'type': 'pointerDown', 'button': 0}, move_to(bx + 8, by - 8)]}]})
        time.sleep(2.6)
        seen = b.page(f'({CURSOR})({bx + 8}, {by - 8})')
        if browser == 'chromium':
            mouse_up(b, bx + 8, by - 8)
        else:
            b._post('/actions', method='DELETE')
        check('  while a bubble is held still on the mountains it is not none', seen != 'none', seen)
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()


def rects_meet(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


PHONE_TOP = """(() => { const r = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.top, b.right, b.bottom]; };
  const some = (sel) => { const e = document.querySelector(sel); return e && !e.hidden && e.textContent.trim() ? r(e) : null; };
  const gains = document.querySelector('.nd-gains');
  return { device: some('.nd-left'), verdict: some('.nd-right'), eq: some('.nd-eq'), db: r(document.querySelector('.nd-odb')),
    button: r(document.getElementById('fx-button')), gains: gains && getComputedStyle(gains).display,
    eqText: document.querySelector('.nd-eq').innerText.trim(), width: innerWidth }; })()"""


def the_phone_top(d):
    """On a phone held upright the device line, the dB readout, the EQ readout and the menu button each have their
    own space (no two rects meet) at 412x915 and 360x780, density 3; the EQ readout holds its gains; and the
    tone curve's top, with every band at the highest gain, is below the dB readout (in the scene)."""
    gmax = max(v for v, _ in d.model(DEVICE)['tone_steps']['gain'])
    for size in [(412, 915), (360, 780)]:
        for flat in (False, True):
            reset_bands(d, flat=flat)
            b = make(size=size, dpr=3)
            try:
                forget(b, d)
                load(b, d)
                time.sleep(1.0)
                got = b.page(PHONE_TOP)
                names = ['device', 'db', 'eq', 'button'] + (['verdict'] if got['verdict'] else [])
                meets = [(x, y) for i, x in enumerate(names) for y in names[i + 1:] if got[x] and got[y] and rects_meet(got[x], got[y])]
                check(f'the_phone_top_has_room: {size[0]}x{size[1]} {"Flat" if flat else "EQ"}, the device line, the dB readout, the EQ readout '
                      f'and the menu button (and the verdict when there is one) meet nowhere', not meets and bool(got['eq']) and bool(got['device']), [meets, got])
                if not flat:
                    check('  the EQ readout holds the bands\' gains on a phone', got['gains'] not in (None, 'none') and got['eqText'].startswith('EQ') and any(c.isdigit() for c in got['eqText']), got)
            finally:
                b.close()
    for size in [(412, 915), (360, 780)]:
        b = make(size=size, dpr=3)
        try:
            t, left, top = tuned(b, d, over={k: (gmax, 1) for k in range(1, 6)})
            mouse_to(b, left + t['w'] / 2, top + t['H'] - 0.1 * t['h'])
            poll(lambda: b.page(TUNE)['m'] >= 0.95, 3.0)
            time.sleep(0.3)
            t = b.page(TUNE)
            top_rects = b.page(PHONE_TOP)
            # the readouts' stack ends at its lowest box: the curve at the device's own top gain stays below all of it
            scene_bottom = b.page("document.querySelector('.nd-scene').getBoundingClientRect().bottom")
            db = [0, 0, 0, max(top_rects[k][3] for k in ('db', 'device', 'verdict', 'eq') if top_rects[k] and top_rects[k][1] < scene_bottom)]
            ys = [b.page(f'__core.theme.driver.curveAt({x})') for x in range(0, int(t['w']), 4)]
            curve = min(y for y in ys if y is not None)
            peak = min(p['y'] for p in t['bubbles'])
            check(f'  {size[0]}x{size[1]}: the curve\'s top (every band at +{gmax:g} dB) is below the readouts\' lowest box',
                  curve > db[3] and peak > db[3], [curve, peak, db])
        finally:
            b.close()
    # a long device name wraps in its column: it still meets neither the dB readout nor the button
    long = common.Daemon('gungnir2-v2', host='fake', name='A device with a very long name that goes on and on and on')
    try:
        for size in [(412, 915), (360, 780)]:
            b = make(size=size, dpr=3)
            try:
                b.goto(f'{long.url}?theme=nightdrive', settle=1.0)
                b.page(BOOT)
                time.sleep(1.0)
                got = b.page(PHONE_TOP)
                names = ['device', 'db', 'eq', 'button']
                meets = [(x, y) for i, x in enumerate(names) for y in names[i + 1:] if got[x] and got[y] and rects_meet(got[x], got[y])]
                check(f'  {size[0]}x{size[1]}: with a 55-character device name nothing meets and the name stays inside the window',
                      not meets and got['device'][0] >= 0 and got['device'][2] <= size[0], [meets, got])
            finally:
                b.close()
    finally:
        long.stop()
    reset_bands(d)


def the_phone_top_link():
    """The top line while the link is not on, `Gungnir 2 · not connected · USB 1 · 48 kHz` and the card's words on
    the right: at 1280x800, 412x915 and 360x780 its two parts, the dB readout, the EQ readout and the menu button
    meet nowhere and the line stays inside the window."""
    g = common.Daemon('gungnir2-v2', host='fake')
    g.drop()
    try:
        for size in [(1280, 800), (412, 915), (360, 780)]:
            b = make(size=size, dpr=3)
            try:
                b.goto(f'{g.url}?theme=nightdrive', settle=1.0)
                b.page(BOOT)
                said = poll(lambda: 'not connected' in (b.page("document.querySelector('.nd-left').textContent") or ''), 12.0)
                time.sleep(0.5)
                got = b.page(PHONE_TOP)
                names = ['device', 'verdict', 'db', 'eq', 'button']
                meets = [(x, y) for i, x in enumerate(names) for y in names[i + 1:]
                         if got[x] and got[y] and rects_meet(got[x], got[y])]
                inside = all(got[k][0] >= 0 and got[k][2] <= size[0] for k in ('device', 'verdict') if got[k])
                check(f'the_top_line_with_the_link_words_has_room: {size[0]}x{size[1]}, the link not on: the line\'s two parts, '
                      'the dB readout, the EQ readout and the menu button meet nowhere and stay inside the window',
                      bool(said) and bool(got['device']) and bool(got['verdict']) and not meets and inside, [said, meets, got])
            finally:
                b.close()
    finally:
        g.stop()


def the_phone_budget(d):
    """Chromium, a phone at density 3 with the CPU slowed: the canvases are held to a density of 2 and
    drawn at 30 a second from the first frame (a finger is a coarse pointer), the scale steps down on slow
    draws without touching the Motion choice. The tap on the frame's button is a
    smoke test at 6x (the menu opens within a second): at that load a draw is about 4 ms, under the
    budget, so it does not test the step-down; at 30x, where the budget acts, the throttled main thread
    alone takes 1.2 to 1.7 s to open the menu, so no second-long bound holds there."""
    b = common.Chromium(size=(390, 844), dpr=3, touch=True)
    try:
        forget(b, d)
        load(b, d)
        time.sleep(1.5)
        dims = b.page("""(() => { const c = document.getElementById('nd-road');
          return [c.width, c.clientWidth, c.height, c.clientHeight]; })()""")
        first = b.page(PERF)
        check('a_phone_stays_usable: at density 3 the canvases are held to a backing density of 2',
              dims[0] == 2 * dims[1] and dims[2] == 2 * dims[3] and first['scale'] == 2
              and first['step'] == 0, [dims, first])
        check('a_phone_starts_without_the_wet_road: the budget does not allow it at first', first['wet'] is False, first)
        b.send('Emulation.setCPUThrottlingRate', {'rate': 6})
        time.sleep(4.0)
        six = b.page(PERF)
        print(f'INFO  chromium 390x844 dpr 3, CPU 6x slower: drawMs {six["drawMs"]:.1f} ms, scale {six["scale"]}')
        b.send('Emulation.setCPUThrottlingRate', {'rate': 30})
        stepped = poll(lambda: b.page(PERF)['scale'] < 2, 15.0)
        slow = b.page(PERF)
        print(f'INFO  chromium 390x844 dpr 3, CPU 30x slower: drawMs {slow["drawMs"]:.1f} ms, scale {slow["scale"]}, '
              f'step {slow["step"]}, capped {slow["capped"]}')
        after = b.page("""(() => { const c = document.getElementById('nd-road');
          return [c.width, c.clientWidth, document.getElementById('fx-button') !== null,
            localStorage.getItem('svolder.theme.nightdrive.motion')]; })()""")
        motion = b.page("""import(new URL('assets/themes/nightdrive/driver.js', document.baseURI)).then((m) => m.bound.view.motion)""")
        check('  slow draws step the scale down, the CSS size is the same and the Motion choice is untouched',
              stepped and slow['scale'] < 2 and slow['scale'] >= 1 and after[0] == round(after[1] * slow['scale'])
              and after[3] is None and motion == 'full', [slow, after, motion])
        b.send('Emulation.setCPUThrottlingRate', {'rate': 6})
        time.sleep(1.0)
        x, y = common.center(b, '#fx-button')
        t0 = time.time()
        touch_send(b, 'touchStart', [(1, x, y)])
        touch_send(b, 'touchEnd', [])
        opened = poll(lambda: b.page("!!document.querySelector('[data-motion]')"), 3.0)
        took = time.time() - t0
        check('  a_tap_opens_the_menu_with_the_cpu_slowed: with the CPU 6x slower, a tap on the frame\'s button opens the menu '
              'within a second', opened and took < 1.0, [opened, round(took, 2), b.page(PERF)])
        check('  no page error', errors(b) == [], errors(b)[:3])
        b.send('Emulation.setCPUThrottlingRate', {'rate': 1})
    finally:
        try:
            forget(b, d)
        finally:
            b.close()


def the_tuning():
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make()
    try:
        the_tuning_hover(b, d)
        the_tuning_drag(b, d)
        the_tuning_grace(b, d)
        the_resting_pointer(b, d)
        the_q_hover(b, d)
        the_flat_band_highlight(b, d)
        the_q_drag(b, d)
        the_q_wheel(b, d)
        the_q_wheel_grace(b, d)
        the_sun_reaches_the_horizon(d)
        the_tuning_range(d)
        if browser == 'chromium':
            the_phone_budget(d)
        if browser == 'chromium':
            the_q_hold(b, d)
            the_q_beside(b, d)
            the_q_dpr_2(d)
            the_tuning_label(b, d)
            the_tuning_waits(b, d)
            the_tuning_dpr_2(d)
    finally:
        try:
            forget(b, d)
        finally:
            b.close()
            d.stop()
    if browser == 'chromium':
        the_tuning_switch()
        the_q_pinch()
    the_tuning_switch_idle()
    the_tuning_guest()



d = common.Daemon('gungnir2-v2', host='fake')
b = make()
try:
    the_mount(b, d)
    the_scene(b, d)
    the_floor(b, d)
    the_heat(b, d)
    the_quiet_night(b, d)
    the_wet_road(b, d)
    the_moment(b, d)
    the_analysis(b, d)
    the_loops(b, d)
    the_menu_button(b, d)
    contrast(b, d)
    the_motion_menu(b, d)
    the_calm_road(b, d)
    the_still_frame(b, d)
    the_hud_text(b, d)
    the_fade(b, d)
    the_tab_order(b, d)
    the_volume_from_elsewhere(b, d)
    the_menu_button_outside_the_fade(b, d)
    the_fonts(b, d)
    the_sun(b, d)
    the_dash(b, d)
    the_dash_hold(b, d)
    the_dash_ask_again(b, d)
    the_dash_input(b, d)
    the_dash_transport(b, d)
    the_dash_cover(d)
    the_cover_follows_the_track(d)
    the_dash_without_a_cover()
    the_dash_fade(b, d)
    the_dash_press_while_faded(b, d)
    the_dash_fade_sideways(d)
    the_dash_fade_portrait(d)
    the_dash_cap()
    the_dash_chips()
    the_dash_guest()
    the_trouble(b)
    the_readout(d)
    the_cover_size(d)
    the_cursor_rest(d)
    the_phone_top(d)
    the_phone_top_link()
    the_tuning()
    b.resize(1280, 800)
    check('no page error in the whole run', errors(b) == [], errors(b)[:3])
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
base.finish('nightdrive_theme.py')
