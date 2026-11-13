# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Signal's page: each mutation edits a copy of the page's web dir
(never the repo), runs signal_theme.py (Chromium; computer_signal.py for the computer side's) against
it and must make a check the mutant names FAIL: a failure of any other check is not a kill (a flake
must not pass for one). The mutants the node harnesses kill are in node/mutate_signal.sh.
Usage: python3 mutate_signal.py [name ...]   (default: all). By hand, slow. Env: MUT_WORKERS (default from the cores and the CPU pressure; mutpool.py's pool).
With more workers the mutants run in a pool, each in its own scratch web copy with its own daemon and
browser (nothing is shared between runs, and there is no baseline). A mutant whose named check measures
repeat timing (TIMING_CHECKS) never runs in the pool: those run alone after it. Every mutant not KILLED in the pool runs again alone after it: only the
alone result counts, and both are printed. The summary lists the mutants in the order of MUTATIONS."""
import os
import shutil
import sys
import time

import common
import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
S = 'themes/signal/'
# name: ([the checks that must fail, by the start of their names], edits[, the check script])
MUTATIONS = {
    'an amp control is written without the amp\'s url': (['an_amp_control_writes_to_the_amps_url'], [
        (S + 'amp-wire.js', "JSON.parse(target.json), { device: amp.url })", "JSON.parse(target.json), {})"),
    ]),
    'the dots ignore reduced motion': (['the_dots_stand_still_under_reduced_motion'], [
        (S + 'signal.js', "matchMedia: (query) => window.matchMedia(query),",
         "matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {},"
         " addListener() {}, removeListener() {} }),"),
    ]),
    'the slim bar never pins': (['the_slim_bar_pins_when_the_line_scrolls_away'], [
        (S + 'slim.js', "bar.hidden = last.isIntersecting;", "bar.hidden = true;"),
    ]),
    'the Add to the chain row opens the sheet for a guest': (['a_guest_cannot_add_or_edit_a_hop'], [
        (S + 'hops-wire.js', "  function add() {\n    if (inert()) {", "  function add() {\n    if (false) {"),
    ]),
    'a held band stepper never repeats': (['holding_a_band_stepper_repeats'], [
        (S + 'controls-wire.js', "      live: () => !view.device()?.inert && view.state !== 'lost' && view.state !== 'waiting',",
         "      live: () => false,"),
    ]),
    'the menu button follows the page': (['the_menu_button_is_fixed_on_a_phone'], [
        (S + 'signal.js', "{ fixed: true }", "{ fixed: false }"),
    ]),
    'the mode is not under the curve on a phone': (['the_mode_has_its_place'], [
        (S + 'signal.js', "${view.phone() ? modeRow(device) : ''}", "${''}"),
    ]),
    'the tab keeps its plain title': (['the_tab_title_names_the_device_and_the_connection'], [
        ('core/title.js', "    document.title = titleOf({ name, status: statusOf(page, store.state()), more });", "    void more;"),
    ]),
    'a held plus never repeats': (['holding_plus_repeats'], [
        (S + 'volume-wire.js', "      live: () => !inert() && !quiet(),", "      live: () => false,"),
    ]),
    'a blur does not stop a hold': (['holding_plus_repeats'], [
        ('core/repeat.js', "  globalThis.window.addEventListener('blur', end, own);\n", ""),
    ]),
    'a hold does not hold the open area': (['holding_plus_repeats'], [
        (S + 'volume-wire.js', "      onStart: () => keep.take(),", "      onStart: () => {},"),
        (S + 'volume-wire.js', "        if (give) keep.drop();", "        if (give && false) keep.drop();"),
    ]),
    'a pinch narrows when the fingers spread': (['a_pinch_sets_the_q'], [
        (S + 'tone-wire.js', "fingers.entry.drag.spread(spacing(a, b));",
         "fingers.entry.drag.spread(1e6 / spacing(a, b));"),
    ]),
    'a tall window stretches the open area': (['a_tall_window_keeps_the_block_together'], [
        (S + 'style.css', "  .sg-open { flex: none; }\n", ""),
    ]),
    'the shelf has three columns again': (['the_shelf_has_two_columns'], [
        (S + 'style.css', "grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 34px 40px;",
         "grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) minmax(0, 1fr); gap: 34px 40px;"),
    ]),
    'a pinch leaves its touch listeners on the graph': (['a_finger_adds_drags_and_pinches: a pinch takes'], [
        ('core/point-drag.js', "el[call]('touchmove', move, { passive: false });",
         "el[call]('touchmove', (event) => move(event), { passive: false });"),
    ], 'computer_signal.py'),
    'the question opens in the chip the shelf had': (['a_question_opens_after_the_shelf_was_drawn_again'], [
        (S + 'computer-wire.js', "const here = opener.isConnected ? opener : (doc.getElementById(opener.id) ?? opener);",
         "const here = opener;"),
    ], 'computer_signal.py'),
    'the wheel is not forwarded to the hand': (['the_wheel_sets_the_q'], [
        (S + 'computer-wire.js', "    if (graph) hand.wheel(event, graph);", "    if (graph) void hand;"),
    ], 'computer_signal.py'),
    'a key on a focused dot is not forwarded to the hand': (['a_point_drag_posts_live_and_done_at_release: an arrow'], [
        (S + 'computer-wire.js', "    if (dot) hand.key(event, Number(dot.dataset.n));", "    if (dot) void hand;"),
    ], 'computer_signal.py'),
    'the recall question is the core card': (['the_same_preset_on_both_asks_and_yes_flattens_the_dac'], [
        (S + 'computer-wire.js', "act.recallPreset(el.dataset.name, { asker: asker(el) })", "act.recallPreset(el.dataset.name)"),
    ], 'computer_signal.py'),
    # three layers each stop the repeat after a flip (the hop's own end, the core's lookup of the pressed
    # button, the step's `shown()` guard): one removed alone leaves the others, so the mutant removes all
    'a flip does not end a held plus (nothing else stops the repeat)': (['a_flip_mid_hold_ends_the_repeat'], [
        (S + 'computer-wire.js', "    give = false;\n    hold?.end();\n    give = true;", "    give = false;\n    give = true;"),
        ('core/repeat.js', "  if (pressed.id) return root.querySelector(", "  if (false) return root.querySelector("),
        ('core/repeat.js', "  return pressed.isConnected ? pressed : null;", "  return pressed;"),
        (S + 'computer-wire.js', "    if (shown()) act.nudge(delta).catch(() => {});", "    act.nudge(delta).catch(() => {});"),
    ], 'computer_signal.py'),
    'the typed cell is read after the field was put back': (['an_untouched_cell_writes_nothing', 'the_mode_the_type_a_typed_value'], [
        (S + 'computer-wire.js', "        field.value = field.defaultValue;\n        if (field.isConnected)", "        if (field.isConnected)"),
        (S + 'computer-wire.js', "        field.blur();\n      },", "        field.value = field.defaultValue;\n        field.blur();\n      },"),
        ('core/table.js', 'await cell(n, param, typed, shown);', 'await cell(n, param, input.value, shown);'),
    ], 'computer_signal.py'),
    'a second finger off the graph leaves the tap': (['a_second_finger_off_the_graph_makes_the_tap_no_tap'], [
        (S + 'computer-wire.js', "addEventListener('pointerdown', () => hand.dropTap(), { signal, capture: true });",
         "addEventListener('pointerdown', () => {}, { signal, capture: true });"),
    ], 'computer_signal.py'),
    'a tap on a full table says nothing': (['a_tap_on_a_full_table_says_so'], [
        (S + 'computer-wire.js', "else if (answer.full) core.frame.toast(answer.error);", ""),
    ], 'computer_signal.py'),
    'the strip does not centre the chosen hop': (['the_strip_centres_the_chosen_hop'], [
        (S + 'draw/line.js', "translateX(${drag - at * HOP_PX}px)", "translateX(${drag}px)"),
    ]),
}

# checks that measure repeat pauses and durations: a loaded machine can fake their result, so they run alone
TIMING_CHECKS = ('holding_plus_repeats', 'holding_a_band_stepper_repeats', 'a_flip_mid_hold_ends_the_repeat')


def timed(name):
    return any(n.startswith(TIMING_CHECKS) for n in MUTATIONS[name][0])


def run(name):
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    named, edits, *script = MUTATIONS[name]
    for f, old, new in edits:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    r = mutpool.launch([sys.executable, f'{HERE}/{script[0] if script else "signal_theme.py"}', 'chromium'], env=env)
    shutil.rmtree(web, ignore_errors=True)
    # a check's own line names it; the indented lines after it are parts of it
    failed, fails, current = [], [], ''
    for line in r.stdout.splitlines():
        if not line.startswith(('ok  ', 'FAIL')):
            continue
        text = line[5:]
        if not text.startswith('  '):
            current = text
        if line.startswith('FAIL'):
            failed.append(line)
            # only a check the mutant names counts; any other failure is listed and is not a kill
            if any(current.startswith(n) for n in named):
                fails.append(line)
    if not fails and r.returncode and not failed:
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    other = [x for x in failed if x not in fails]
    return name, fails, ('(failed only other checks, not a kill) ' + other[0][:120] if other and not fails else '')


def attempt(name):
    began = time.time()
    _, fails, err = run(name)
    return fails, err, time.time() - began


def main():
    names = sys.argv[1:] or list(MUTATIONS)
    return mutpool.run_names(names, attempt, [n for n in names if timed(n)])


sys.exit(main())
