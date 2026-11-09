# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Night drive's page: each mutation edits a copy of the page's web dir
(never the repo), runs the browser checks it names (Chromium) against it and must make a check the
mutant names FAIL: a failure of any other check is not a kill (a flake must not pass for one). The
mutants the node harnesses kill are in node/mutate_nightdrive.sh.

A run is `(script, cases, names)`: the script, the cases of it to run (`ND_CASES`: every `the_*` case
function of the Night drive scripts not listed is skipped; None runs the whole script) and the check
names (substrings of a check's line, or of the check an indented line belongs to) one of which must
FAIL. Before a mutant is run, the unmutated copy runs the same cases once and the named checks must
exist and pass, so a name that no longer exists or a case that cannot run alone is an error, not a kill.

Usage: python3 mutate_nightdrive.py [name ...]   (default: all). By hand, slow. Env: MUT_WORKERS (default from the cores and the CPU pressure).
With more workers the mutants run in a pool, each in its own scratch web copy with its own daemon and
browser (nothing is shared but the unmutated baselines, which run once each under a lock). A mutant that
uses the timing check (nightdrive_timing.py) never runs in the pool: those run alone after it, and so does
every mutant that was not KILLED in the pool: only the alone result counts, and both are printed. The
summary lists the mutants in the order of MUTATIONS. The pool is mutpool.py's. A baseline that failed is not kept:
the mutant's alone run makes it again."""
import os
import re
import shutil
import sys
import threading
import time

import common
import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
N = 'themes/nightdrive/'

THEME = 'nightdrive_theme.py'
TIMING = 'nightdrive_timing.py'
SHEET = 'nightdrive_sheet.py'
PHONE = 'nightdrive_phone.py'
STATES = 'nightdrive_states.py'

# name: ([(script, cases or None, [the checks that must fail])], [(file, old, new)])
MUTATIONS = {
    'focus does not wake the fade': ([
        (THEME, ['the_tab_order'], ['the_faded_hud_stays_in_the_tab_order_and_focus_wakes_it']),
    ], [
        (N + 'hud-wire.js', "'touchstart', 'focusin'];", "'touchstart'];"),
    ]),
    'trouble fades': ([
        (THEME, ['the_trouble'], ['trouble_while_faded_shows_the_top_line']),
        (STATES, ['the_faded_top'], ['a_card_while_faded_wakes_the_top']),
    ], [
        (N + 'hud-wire.js', "fade.hold(statusOf(model, view.state) !== TITLE.connected);", "fade.hold(false);"),
    ]),
    # a loop each mount starts and nothing ends: the first load has one more than it should, and a switch away and
    # back one more again
    'a mount starts a loop nothing ends': ([
        (THEME, ['the_loops'], ['a_switch_away_and_back_leaves_one_loop']),
    ], [
        (N + 'scene-wire.js', "  loop.start();\n  // a mount that fails",
         "  loop.start();\n  const stray = () => requestAnimationFrame(stray);\n  stray();\n  // a mount that fails"),
    ]),
    'the analysis is kept while hidden': ([
        (THEME, ['the_analysis'], ['the_analysis_is_held_while_shown_and_let_go_when_hidden_or_unmounted']),
    ], [
        (N + 'scene-wire.js', "onShow: open, onHide: close }", "onShow: open, onHide: () => {} }"),
    ]),
    'reduced motion starts on Full': ([
        (TIMING, ['the_reduced_start'], ['reduced_motion_starts_on_calm_and_full_can_be_picked']),
    ], [
        (N + 'motion.js', "return reduced ? 'calm' : 'full';", "return 'full';"),
    ]),
    'Still draws at the screen\'s rate': ([
        (THEME, ['the_pace_on_a_fake_clock'], ['still_draws_about_one_frame_a_second']),
    ], [
        (N + 'motion.js', "still: 1000 }", "still: 0 }"),
    ]),
    'the dash\'s hold never repeats': ([
        (THEME, ['the_dash_hold'], ['holding_plus_repeats_and_the_release_ends_it']),
    ], [
        (N + 'dash-wire.js', "      live: writable,\n      leave: true,", "      live: () => false,\n      leave: true,"),
    ]),
    'a diamond drag writes nothing': ([
        (THEME, ['the_tuning', 'the_q_drag'], ['dragging_a_diamond_sets_q']),
    ], [
        (N + 'tune-wire.js', "        begin(bell.band, {\n          gesture: 'pinch', press: { distance: 1 }, pointer: event.pointerId, side: nearDiamond.side,\n        });",
         "        void 0;"),
    ]),
    'a pinch takes band 1': ([
        (THEME, ['the_tuning', 'the_q_pinch'], ['a_pinch_sets_the_q_of_the_band_nearest_the_fingers']),
    ], [
        (N + 'tune-wire.js', ": grab(bubblesOf(toneNow(), mapNow(), place.w), middle, { reach: REACH.touch, wide: true });",
         ": 1;"),
    ]),
    'hit tests in canvas pixels': ([
        (THEME, ['the_tuning', 'the_tuning_dpr_2', 'the_q_dpr_2'],
         ['a_bubble_is_grabbed_at_dpr_2', 'a_diamond_is_grabbed_at_dpr_2']),
    ], [
        (N + 'tune-wire.js', "return { x: event.clientX - rect.left, y: event.clientY - rect.top };\n  };\n\n  // ---- the label",
         "return { x: (event.clientX - rect.left) * devicePixelRatio, y: (event.clientY - rect.top) * devicePixelRatio };\n  };\n\n  // ---- the label"),
    ]),
    'a device switch mid-drag writes': ([
        (THEME, ['the_tuning', 'the_tuning_switch'], ['a_device_switch_mid_drag_writes_nothing_to_the_new_device']),
    ], [
        (N + 'tune-wire.js', "    const ending = pending.splice(0);", "    const ending = [];"),
        (N + 'tune-wire.js', "    snap = null;\n    active = null;\n    fingers = null;\n    lit = null;\n    unsay();",
         "    fingers = null;\n    lit = null;\n    unsay();"),
        (N + 'tune-wire.js', "        actions: core.actions,\n        band,\n        device,",
         "        actions: { setBand: (b, p, body) => core.actions.setBand(b, p, body, { device: store.shown() }) },\n        band,\n        device,"),
    ]),
    'a guest\'s tap on the mountains morphs': ([
        (THEME, ['the_tuning', 'the_tuning_guest'], ['a_guest_tap_on_the_mountains_says_why']),
    ], [
        (N + 'tune-wire.js', "tunes && store.state() === 'open' && !(page.device?.card ?? page.card);",
         "tunes && store.state() === 'open' && !(page.device?.card ?? page.card) || store.page().viewer?.guest === true;"),
    ]),
    'a first touch grabs at once': ([
        (THEME, ['the_tuning', 'the_q_pinch'], ['a_first_touch_shows_the_curve_and_a_second_grabs']),
        (PHONE, ['the_mountains_on_a_phone'], ['touching_the_mountains_tunes_on_a_phone']),
    ], [
        (N + 'tune-wire.js', "    if (morph.m < 0.5) {\n      if (touch) morph.touch(now());", "    if (morph.m < 0) {\n      if (touch) morph.touch(now());"),
    ]),
    'the tuning band loses its margin above': ([
        (PHONE, ['the_band_edge_on_a_phone'], ["the_band_edge_on_a_phone: the pointer just inside the band's top edge shows the curve"]),
    ], [
        (N + 'tune.js', "y > H - BAND.up * v - BAND.margin &&", "y > H - BAND.up * v + BAND.margin &&"),
    ]),
    'the sheet does not give the focus back': ([
        (SHEET, ['the_sheet', 'the_sheet_closes'], ['close_by_x_escape_or_a_tap_on_the_scene_and_focus_returns']),
    ], [
        (N + 'sheet-wire.js', "    back?.focus({ preventScroll: true });", "    void back;"),
    ]),
    'the sheet\'s columns are swapped': ([
        (SHEET, ['the_sheet', 'the_sheet_opens'], ['device_opens_the_sheet_and_focus_goes_in']),
    ], [
        (N + 'style.css', "grid-template-columns: 1.15fr 1fr; gap: 16px 28px;", "grid-template-columns: 1.15fr 1fr; direction: rtl; gap: 16px 28px;"),
    ]),
    'Keep sends the question\'s write': ([
        (SHEET, ['the_sheet', 'the_presets'], ['  Keep deletes nothing']),
        (SHEET, ['the_sheet', 'a_slot_save_asks_first'], ['  Keep writes nothing']),
    ], [
        (N + 'ask.js', "    if (answer === yes) send();\n    else if (answer === no && !sending) handle.close();",
         "    if (answer === yes) send();\n    else if (answer === no && !sending) send();"),
    ]),
    'a question sends before the answer': ([
        (SHEET, ['the_sheet', 'the_presets'], ['  Keep deletes nothing']),
        (SHEET, ['the_sheet', 'a_slot_save_asks_first'], ['a_slot_save_asks_first: choosing a slot asks in place and sends nothing yet']),
    ], [
        (N + 'ask.js', "  handle.box.addEventListener('click', (event) => {\n    const answer = answerOf(event);\n    if (answer === yes) send();",
         "  go();\n  handle.box.addEventListener('click', (event) => {\n    const answer = answerOf(event);\n    if (answer === yes) send();"),
    ]),
    'a second tap on the input chip writes': ([
        (SHEET, ['the_sheet', 'the_sheet_list'], ['  a second tap on the chip closes the list and writes nothing']),
    ], [
        (N + 'sheet-wire.js', "      if (closeOf(el)) return;\n      keep.around(() => openList({\n        opener: el,\n        label: c.label ?? SHEET.input,",
         "      if (closeOf(el)) {\n        writeInput(c, c.options?.find((o) => o.on) ?? c.options?.[0]);\n        return;\n      }\n      keep.around(() => openList({\n        opener: el,\n        label: c.label ?? SHEET.input,"),
    ]),
    'Escape in a band field writes': ([
        (SHEET, ['the_sheet', 'the_band_table'], ['  Escape puts the old value back, writes nothing and leaves the sheet open']),
    ], [
        (N + 'bands-wire.js', "      event.stopPropagation();\n      field.value = field.defaultValue;\n      field.blur();",
         "      event.stopPropagation();\n      core.actions.setBand(Number(field.dataset.band), BAND_PARAM[field.dataset.param], { value: field.value }, { device: store.shown() });\n      field.value = field.defaultValue;\n      field.blur();"),
    ]),
    'the core lets a guest\'s call go out': ([
        (THEME, ['the_dash_guest'], ['a_guests_tap_posts_nothing']),
        (THEME, ['the_tuning', 'the_tuning_guest'], ['a_guest_tap_on_the_mountains_says_why']),
        (SHEET, ['the_sheet', 'the_sheet_guest'], ['a_guests_sheet_is_inert_in_a_browser']),
    ], [
        ('core/actions.js', "      toast(SIGN_IN_WORDS, SIGN_IN);\n      return { ok: false, status: 0, error: SIGN_IN_WORDS };",
         "      toast(SIGN_IN_WORDS, SIGN_IN);"),
        ('core/actions.js', "      toast(why);\n      return { ok: false, status: 0, error: why };", "      toast(why);"),
    ]),
    'an amp\'s meters are heard as a part': ([
        (SHEET, ['the_sheet', 'an_amps_meters_move_live_on_the_sheet'],
         ['  a needle changes at least twice within 3 s with the sheet left open']),
    ], [
        (N + 'controls-wire.js', "live.onReading('meters', (data) => {\n    if ((data.device ?? store.shown()) !== store.shown() || !sheet.isOpen()) return;",
         "live.onReading('part', (data) => {\n    if (data.name !== 'meters' || (data.device ?? store.shown()) !== store.shown() || !sheet.isOpen()) return;"),
    ]),
    'the wet road\'s copy is made a tenth of a second apart': ([
        (THEME, ['the_wet_road'], ['  and a copy of the sky is made at every draw, not a few times a second']),
    ], [
        (N + 'scene-wire.js', "    perf.mirrored++;\n    const copy", "    if (frame.t - (mirror.at ?? -9) < 0.1) { drawWet(sky, mirror, geo, frame.look.wet, frame.t); return; }\n    mirror.at = frame.t;\n    perf.mirrored++;\n    const copy"),
    ]),
    'the first tap on a faded dash is let through': ([
        (THEME, ['the_dash_fade'], ['a_tap_on_a_faded_key_only_wakes_the_dash']),
    ], [
        (N + 'dash-fade.js', "    if (woke !== event) return;\n    woke = null;", "    if (true) return;\n    woke = null;"),
    ]),
    # the draw-cost case holds the budget still itself (`forceDrawMs`), so the mutant needs no edit to the budget
    'the sky is drawn twelve times in every draw': ([
        (TIMING, ['what_a_draw_costs'], ['  a draw with every cell lit takes 4.5 ms or less']),
    ], [
        (N + 'scene-wire.js', "      drawSky(sky, geo, frame);\n      if (frame.look.wet",
         "      for (let i = 0; i < 12; i++) drawSky(sky, geo, frame);\n      if (frame.look.wet"),
    ]),
    # the press is let go of after 1.5 s; a mutant that ends the deaf window 0.6 s after the press leaves the
    # short taps as they are, so only the long-press check can kill it
    'a long press that woke the dash presses on its release': ([
        (THEME, ['the_dash_fade'], ['a_long_press_that_woke_a_faded_dash_presses_nothing_when_let_go']),
    ], [
        (N + 'dash-fade.js', "    woke = null;\n    deaf = true;\n", "    woke = null;\n    deaf = true;\n    setTimeout(() => { deaf = false; }, 600);\n"),
    ]),
    'the top line does not wrap on a phone': ([
        (THEME, ['the_phone_top_link'], ['the_top_line_with_the_link_words_has_room']),
    ], [
        (N + 'style.css', "  .nd-left { overflow-wrap: anywhere; }", "  .nd-left { white-space: nowrap; }"),
    ]),
    'the menu button follows the page on a phone': ([
        (PHONE, ['the_menu_button'], ['the_menu_button_is_fixed_on_a_phone']),
    ], [
        (N + 'nightdrive.js', "{ fixed: true }", "{ fixed: false }"),
    ]),
    'Space toggles play while the theme menu is open': ([
        ('space_key.py', None, ['(nightdrive) with the menu open and the focus on the page space presses nothing']),
    ], [
        ('core/frame.js', "if (spaceKey(event) && !menu.opened() &&", "if (spaceKey(event) &&"),
    ]),
    'the faded dash rests at a tenth': ([
        (THEME, ['the_dash_fade'], ['the_dash_fades_to_a_fifth_with_the_hud']),
    ], [
        (N + 'style.css', ".nd-dash-dim .nd-dash { opacity: .2;", ".nd-dash-dim .nd-dash { opacity: .1;"),
    ]),
    'the phone\'s volume keys leave the first row': ([
        (PHONE, ['the_portrait_panel'], ['  in the panel `-` `MUTE` `+` in that order and the transport on ONE row']),
    ], [
        (N + 'style.css', "  .nd-keys > :is(#nd-vdown, #nd-mute, #nd-vup) { order: 1; }", "  .nd-keys > :is(#nd-vdown, #nd-mute, #nd-vup) { order: 6; }"),
    ]),
    'the scene stops at the phone\'s panel': ([
        (PHONE, ['the_portrait_panel'], ['  the scene runs behind the panel']),
    ], [
        (N + 'style.css', "  .nd-scene { grid-area: 1 / 1 / 3 / 2; }", "  .nd-scene { grid-area: 1 / 1 / 2 / 2; }"),
    ]),
    'the phone\'s panel is opaque': ([
        (PHONE, ['the_portrait_panel'], ['  the panel is translucent']),
    ], [
        (N + 'style.css', "    background: var(--nd-panel-phone); box-shadow: none;", "    background: #07040d; box-shadow: none;"),
    ]),
    'the phone\'s keys are 44 px tall': ([
        (PHONE, ['the_portrait_panel'], ["  the pill's segments are 36 px tall (the panel's keys too)"]),
    ], [
        (N + 'style.css', "  .nd-dash .nd-k { min-width: 36px; height: 36px;", "  .nd-dash .nd-k { min-width: 36px; height: 44px;"),
    ]),
    'the device line is inset for a switch that is not shown': ([
        (PHONE, ['the_landscape_scene'], ['  with no switch shown the device line keeps the scene\'s edge inset']),
    ], [
        (N + 'style.css', "  .nd-sw-hud .nd-top { left: calc(max(8px,", "  .nd-top { left: calc(max(8px,"),
    ]),
    'the pill\'s segments lose their hit area': ([
        (PHONE, ['the_portrait_panel'], ['  the pill\'s segments are 36 px tall']),
    ], [
        (N + 'style.css', '.nd-swkeys .nd-k::after { content: ""; position: absolute; inset: -5px 0; }', '.nd-swkeys .nd-k::after { content: ""; position: absolute; inset: 0; }'),
    ]),
    'the switch pill is not in the corner upright': ([
        (PHONE, ['the_portrait_panel'], ['  the switch is one pill']),
    ], [
        (N + 'style.css', "  .nd-hud > .nd-switch { position: fixed; }", "  .nd-hud > .nd-switch { position: static; }"),
    ]),
    'the pill says the whole names': ([
        (PHONE, ['the_portrait_panel'], ['  the segments read `DAC` and `PC`']),
    ], [
        (N + 'draw/dash.js', "const short = { dac: amp ? 'AMP' : 'DAC', computer: 'PC' };", "const short = { dac: 'DAC', computer: 'This computer' };"),
    ]),
    'choosing Full on a phone does not probe': ([
        (TIMING, ['the_full_switch'], ['choosing_full_on_a_phone_lifts_the_cap_at_once']),
    ], [
        (N + 'scene-wire.js', "if (view.motion === 'full' && motionWas !== 'full') budget.probe();", ""),
    ]),
    'a paused phone is drawn every 125 ms': ([
        (TIMING, ['the_paused_phone'], ['a_paused_phone_keeps_the_road_smooth']),
    ], [
        (N + 'motion.js', "export const IDLE_PACE = 1000 / 30;", "export const IDLE_PACE = 125;"),
    ]),
    'the recalled preset\'s question is not asked in the sheet': ([
        ('computer_nightdrive.py', None, ['the_same_preset_on_both_asks_and_yes_flattens_the_dac: it asks in the sheet']),
    ], [
        (N + 'presets-wire.js', "recallPreset(name, { asker: bothSides(el) })", "recallPreset(name)"),
    ]),
    'a point\'s stepper steps the wrong way': ([
        ('computer_nightdrive.py', None, ['the_points_table_has_steppers_that_step_and_repeat: + steps the gain one tenth']),
    ], [
        (N + 'bands-wire.js', "el.dataset.param, Number(el.dataset.step), { hold: held });", "el.dataset.param, -Number(el.dataset.step), { hold: held });"),
    ]),
    'the points\' table counts its rows by its children': ([
        ('computer_nightdrive.py', None, ['sixteen_points_keep_the_layout']),
    ], [
        (N + 'sheet-wire.js', "    rows: (area) => area.querySelectorAll('tbody tr'),\n", ""),
    ]),
    'a tap on a full table says nothing': ([
        ('computer_nightdrive.py', None, ['a_tap_on_a_full_table_says_so: a tap says the most the daemon holds']),
    ], [
        (N + 'tune-wire.js', "if (answer?.full) core.frame.toast(answer.error);", "void answer;"),
    ]),
    'a held stepper steps the neighbour after a re-sort': ([
        ('computer_nightdrive.py', None, ['a_held_stepper_follows_its_point_through_a_re_sort: every step went to the same point']),
    ], [
        (N + 'bands-wire.js', "stepper.step(n, el.dataset.param, Number(el.dataset.step), { hold: held });",
         "core.actions.computer.stepPoint(Number(el.dataset.point), el.dataset.param, Number(el.dataset.step));"),
    ]),
    'a finger on a shelf\'s diamond drags the point instead of the Q': ([
        ('computer_nightdrive.py', None, ["one finger on a shelf's diamond moves its Q and neither its frequency nor its gain",
                                          'one finger on a diamond drags its Q, one done at the lift']),
    ], [
        (N + 'tune-wire.js', "if (onComputer()) beginDiamond(bell.band, { pointer: event.pointerId, side: nearDiamond.side });",
         "if (onComputer()) void 0;"),
    ]),
    'a finger on a diamond lets it go': ([
        ('computer_nightdrive.py', None, ['one finger on a diamond drags its Q, one done at the lift']),
    ], [
        (N + 'tune-wire.js', "    const pair = event.touches.length === 2;", "    const pair = true;"),
    ]),
    'the touch band is taller than the curve window': ([
        ('computer_nightdrive.py', None, ['a tap 6 px above the band makes no point']),
    ], [
        (N + 'tune.js', "const BAND = { up: WINDOW.base + WINDOW.span, down: WINDOW.span - WINDOW.base, margin: 26 };", "const BAND = { up: 0.32, down: 0.1, margin: 26 };"),
    ]),
    'a stacked point is drawn above the band': ([
        ('computer_nightdrive.py', None, ['every bubble is drawn inside the band']),
    ], [
        (N + 'tune.js', "yOfDb: (db) => base - (Math.max(-gMax, Math.min(gMax, db)) * span) / gMax,", "yOfDb: (db) => base - (db * span) / gMax,"),
    ]),
    'the wheel over the volume steps the wrong way': ([
        ('computer_nightdrive.py', None, ['the_wheel_and_the_arrows_step_the_volume: a notch up over the number']),
    ], [
        (N + 'volume-keys.js', "if (got.steps !== 0) nudge(got.steps > 0 ? 1 : -1);", "if (got.steps !== 0) nudge(got.steps > 0 ? -1 : 1);"),
    ]),
    'the arrows step with a control focused': ([
        ('computer_nightdrive.py', None, ['the_wheel_and_the_arrows_step_the_volume: a control with the focus keeps the key']),
    ], [
        (N + 'volume-keys.js', "    if (at && at !== doc.body && at !== doc.documentElement) return true;", "    if (false) return true;"),
    ]),
    'the wheel over the number is stopped before the wake': ([
        ('computer_nightdrive.py', None, ['the_wheel_and_the_arrows_step_the_volume: the first wheel over a faded number only wakes the HUD']),
    ], [
        (N + 'volume-keys.js', "    event.preventDefault();\n    if (page.classList.contains('nd-faded')) return;", "    event.preventDefault();\n    event.stopImmediatePropagation();\n    if (page.classList.contains('nd-faded')) return;"),
    ]),
    'the wheel over the number needs the focus free': ([
        ('computer_nightdrive.py', None, ['the_wheel_and_the_arrows_step_the_volume: and the wheel over the number still steps the volume']),
    ], [
        (N + 'volume-keys.js', "if (!figure || event.ctrlKey || covered()) return;", "if (!figure || event.ctrlKey || busy()) return;"),
    ]),
    'the amp\'s gauge labels are dim again': ([
        (PHONE, ['the_amp_panel_contrast'], ['  every text in it keeps 4.5:1']),
    ], [
        (N + 'style.css', ".nd-gauge i { font-style: normal; color: var(--nd-track-sub);", ".nd-gauge i { font-style: normal; color: var(--nd-mid);"),
    ]),
    'This computer is capped in width': ([
        ('computer_nightdrive.py', None, ['the_switch_keys_never_ellipsize_this_computer']),
    ], [
        (N + 'style.css', ".nd-swkeys #nd-side-computer { flex: none; }", ".nd-swkeys #nd-side-computer { flex: none; max-width: 90px; overflow: hidden; }"),
    ]),
    'the picker\'s close button is under 44 px': ([
        ('effects_menu.py', None, ['the_picker_close_button_is_44_px_inside_the_window (nightdrive']),
    ], [
        ('core/frame.css', "position: absolute; top: 8px; right: 8px; width: 44px; height: 44px; padding: 0; display: grid;",
         "position: absolute; top: 8px; right: 8px; width: 30px; height: 30px; padding: 0; display: grid;"),
    ]),
}

# the case functions of a Night drive script are every top-level `def the_*` and these
CASE_DEF = re.compile(r'^def (the_\w+|a_\w+|an_\w+|every_\w+|identify_\w+|hidden|what_a_draw_costs)\(', re.M)
GATE = '''
import os as _os
_CASES = set(filter(None, _os.environ.get('ND_CASES', '').split(',')))


def _case(fn):
    def gated(*a, **k):
        if _CASES and fn.__name__ not in _CASES:
            return None
        return fn(*a, **k)
    gated.__name__ = fn.__name__
    return gated

'''


def gated_copy(script, into):
    """The script with every case function gated by ND_CASES, written into `into`; the path."""
    text = open(f'{HERE}/{script}').read()
    marker = 'from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)\n'
    assert marker in text, script
    text = text.replace(marker, marker + GATE, 1)
    text = CASE_DEF.sub(lambda m: '@_case\n' + m.group(0), text)
    path = f'{into}/{script}'
    open(path, 'w').write(text)
    return path


def run_script(web, script, cases):
    """Run `script` (Chromium) against the web dir `web` with only `cases`; the (check, line) pairs
    of its output and the return code. A check's own line names it; the indented lines after it are
    parts of it."""
    work = common.scratch('mut-run-')
    env = dict(os.environ, SMOKE_WEB=web, PYTHONPATH=HERE)
    try:
        if cases is None:
            path = f'{HERE}/{script}'
        else:
            path = gated_copy(script, work)
            env['ND_CASES'] = ','.join(cases)
        r = mutpool.launch([sys.executable, path, 'chromium'], env, 1800)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    lines = mutpool.checks(r.stdout)
    return lines, r.returncode, r.stderr


def web_copy(edits):
    web = common.scratch('mut-web-')
    try:
        shutil.copytree(common.WEB, web + '/w')
        for f, old, new in edits:
            p = f'{web}/w/{f}'
            s = open(p).read()
            assert s.count(old) == 1, f'{old[:50]!r} not once in {f}'
            open(p, 'w').write(s.replace(old, new, 1))
    except BaseException:
        shutil.rmtree(web, ignore_errors=True)
        raise
    return web


BASELINE = {}  # the keys of the baselines that passed
# one lock for the table of locks, and one lock per baseline: each baseline runs once, whichever thread asks first
_LOCKS = {}
_LOCKS_GUARD = threading.Lock()


def baseline(script, cases, names):
    """The unmutated copy's run of the same cases: every name must exist and pass. Kept per run."""
    key = (script, tuple(cases or ()), tuple(names))
    with _LOCKS_GUARD:
        lock = _LOCKS.setdefault(key, threading.Lock())
    with lock:
        return _baseline(key, script, cases, names)


def _baseline(key, script, cases, names):
    if key not in BASELINE:
        web = web_copy([])
        try:
            lines, code, err = run_script(web + '/w', script, cases)
        finally:
            shutil.rmtree(web, ignore_errors=True)
        bad = []
        for n in names:
            mine = [(c, line) for c, line in lines if n in c or n in line]
            if not mine:
                bad.append(f'{n!r} does not exist in this run')
            elif any(line.startswith('FAIL') for _, line in mine):
                bad.append(f'{n!r} fails unmutated')
        problem = '; '.join(bad) or None
        # only a passing baseline is kept: one that flaked under load must be run again, not repeated
        if problem:
            return problem
        BASELINE[key] = None
    return BASELINE[key]


def run(name):
    runs, edits = MUTATIONS[name]
    for script, cases, names in runs:
        problem = baseline(script, cases, names)
        if problem:
            return None, f'(error, not a kill) {script}: {problem}'
    web = web_copy(edits)
    killed, other, missing = [], [], []
    try:
        for script, cases, names in runs:
            lines, code, err = run_script(web + '/w', script, cases)
            # a named check that printed no line did not run (it skipped, or the run died): not a survivor
            if not any(n in c or n in line for n in names for c, line in lines):
                missing.append(f'{script}: none of {names} ran')
            for current, line in lines:
                if not line.startswith('FAIL'):
                    continue
                if any(n in current or n in line for n in names):
                    killed.append(line)
                else:
                    other.append(line)
            if not any(line.startswith('FAIL') for _, line in lines) and code:
                other.append('(no check output) ' + (err.strip().splitlines() or ['no output'])[-1][:160])
    finally:
        shutil.rmtree(web, ignore_errors=True)
    if missing and not killed:
        return None, '(error, not a kill) the named check did not run: ' + '; '.join(missing)[:200]
    note = ''
    if other and not killed:
        note = '(failed only other checks, not a kill) ' + other[0][:140]
    return killed, note


def attempt(name):
    began = time.time()
    killed, note = run(name)
    return killed, note, time.time() - began


def main():
    names = sys.argv[1:] or list(MUTATIONS)
    timed = [n for n in names if any(script == TIMING for script, _, _ in MUTATIONS[n][0])]
    return mutpool.run_names(names, attempt, timed)


sys.exit(main())
