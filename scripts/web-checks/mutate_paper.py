# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Paper's page: each mutation edits a copy of the page's web dir
(never the repo), runs the cases of paper.py (Chromium; computer_paper.py for the computer's side) that name it against it and must
make a check FAIL that ran, and passed, in the unmutated run; the same cases run unmutated first and must pass (a case that cannot run alone is an error).
Usage: python3 mutate_paper.py [name ...]   (default: all). By hand, slow. Env: MUT_WORKERS (default from the
cores and the CPU pressure; mutpool.py's pool)."""
import os
import shutil
import sys

import common  # noqa: F401
import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
P = 'themes/paper/'
PAGE = {
    'place sets nothing': [
        ('core/place.js', "const at = percent(el.dataset.at);", "const at = null;"),
        ('core/place.js', "const left = percent(el.dataset.x);", "const left = null;"),
        ('core/place.js', "const top = percent(el.dataset.y);", "const top = null;"),
    ],
    'Auto ignores the system': [
        (P + 'style.css', ':not([data-scheme="light"])', ''),
    ],
    'a held section is drawn anyway': [
        (P + 'live.js', "if (holding.has(name)) {", "if (false) {"),
    ],
    'an open form keeps a lost sheet usable': [
        (P + 'live.js', "view.state in STILL && gesturing.size === 0;", "view.state in STILL && holding.size === 0;"),
    ],
    'act logs nothing': [
        ('core/act.js', "entries.push(entry);", ""),
    ],
    'a meter\'s needle stands at its class-A share': [
        (P + 'draw/amp.js', 'data-at="${m.fraction.toFixed(5)}"',
         'data-at="${Number(m.class_a ?? 0).toFixed(5)}"'),
    ],
}

# Mutations of the computer's side: each runs computer_paper.py (Chromium) instead of paper.py.
W = P + 'computer-wire.js'
COMPUTER = {
    'a tap has no time limit': [('core/computer.js', 'TAP_MS = 600', 'TAP_MS = 600000')],
    'a tap may move 60 px': [('core/computer.js', 'TAP_PX = 6', 'TAP_PX = 60')],
    'a typed cell loses its words': [
        (W, 'actions.cell(n, param, cell.value, valueWords',
         "actions.cell(n, param, cell.value.replace(/[^0-9+.-]/g, ''), valueWords"),
    ],
    'a wheel burst ends at every notch': [('core/computer.js', 'WHEEL_END_MS = 300', 'WHEEL_END_MS = 0')],
    'a drag is drawn on the first point': [(W, 's.drawn = s.n;', 's.drawn = 1;')],
    'the held drag gives its section back after a switch': [(W, 'if (!s.switched) keep.drop();', 'keep.drop();')],
    'a second finger leaves the tap': [(W, "addEventListener('pointerdown', () => hand.dropTap(), { signal, capture: true });", "addEventListener('pointerdown', () => {}, { signal, capture: true });")],
    'a guest is told the table is full': [('core/point-drag.js', 'const refusal = actions.refuse() ?? fullAnswer(now.max);', 'const refusal = fullAnswer(now.max);')],
    'a full table is one point too late to say so': [('core/computer.js', 'canAdd: points.length < max,', 'canAdd: points.length <= max,')],
    'the point in hand does not follow a re-sort': [
        ('core/computer.js', "picked.n === extra.from) picked.n = extra.to;", "picked.n === extra.from) picked.n = picked.n;"),
    ],
}
# The cases each mutant runs, and no others (the baseline of them runs unmutated first, and must pass): for
# paper.py the functions of the run (mutpool.gated_copy gates them), for computer_paper.py the names of its
# `one(...)` cases.
CASES = {
    'place sets nothing': ['the_volume'],
    'Auto ignores the system': ['auto'],
    'a held section is drawn anyway': ['tone_hold', 'the_balance'],
    'act logs nothing': ['the_notes', 'from_this_browser'],
    'an open form keeps a lost sheet usable': ['the_states'],
    'a meter\'s needle stands at its class-A share': ['the_amp'],
    'a tap has no time limit': ['a_tap_is_short_and_still'],
    'a tap may move 60 px': ['a_tap_is_short_and_still'],
    'a typed cell loses its words': ['typed_words_read_k_and_a_comma'],
    'a wheel burst ends at every notch': ['wheel_and_arrow_bursts_end_in_one_done'],
    'a drag is drawn on the first point': ['a_drag_is_drawn_on_the_point_held'],
    'the held drag gives its section back after a switch': ['a_drag_ended_by_a_flip_leaves_later_holds_alone'],
    'a second finger leaves the tap': ['a_second_finger_makes_the_tap_no_tap'],
    'a guest is told the table is full': ['a_guest_tap_on_a_full_table_is_told_to_sign_in'],
    'a full table is one point too late to say so': ['a_full_table_says_so', 'sixteen_points_keep_the_layout'],
    'the point in hand does not follow a re-sort': ['a_point_drag_posts_live_and_done_at_release',
                                                    'the_points_are_listed_by_frequency_and_the_selection_follows'],
}
# name: (the script, its cases, the edits)
MUTATIONS = {**PAGE, **COMPUTER}  # every mutant the runner can run (snippets.py reads this)
SPEC = {**{n: ('paper.py', CASES[n], e) for n, e in PAGE.items()},
        **{n: ('computer_paper.py', CASES[n], e) for n, e in COMPUTER.items()}}

BASE = mutpool.Baselines()
NAMES = {}  # the checks of each unmutated run: a kill is a failing check of these


def run(name):
    script, cases, edits = SPEC[name]
    gate = 'argv' if script == 'computer_paper.py' else 'env'

    def unmutated():
        web = mutpool.scratch_web([])
        try:
            lines, code, err = mutpool.run_checks(script, web, cases, gate)
        finally:
            shutil.rmtree(web, ignore_errors=True)
        if any(line.startswith('FAIL') for _, line in lines) or (code and not lines):
            return f'{script} {cases} fails unmutated: ' + ' | '.join(x[:100] for _, x in lines if x.startswith('FAIL'))[:300]
        NAMES[(script, tuple(cases or ()))] = sorted({c for c, _ in lines})
        return None

    problem = BASE.problem((script, tuple(cases or ())), unmutated)
    if problem:
        return None, '(error, not a kill) ' + problem
    web = mutpool.scratch_web(edits, name)
    try:
        lines, code, err = mutpool.run_checks(script, web, cases, gate)
    finally:
        shutil.rmtree(web, ignore_errors=True)
    return mutpool.verdict(lines, code, err, NAMES[(script, tuple(cases or ()))])


def attempt(name):
    return mutpool.timed_call(run, name)


sys.exit(mutpool.run_names(sys.argv[1:] or list(SPEC), attempt))
