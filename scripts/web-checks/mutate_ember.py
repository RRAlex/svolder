# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Ember's computer side: each mutation edits a copy of the page's web
dir (never the repo) and must make a check FAIL: one that ran, and passed, in the same cases unmutated (a crash is an error, not a kill). A mutation of the drawing runs the node harness
ember-parts.mjs on the copy; one of the page's wiring runs computer_ember.py (Chromium) on the copy,
only the cases that name it (a case is the check's `one(...)` name, or `touch`), so the run is short.
Usage: python3 mutate_ember.py [name ...]   (default: all). By hand, slow. Env: MUT_WORKERS (default from the
cores and the CPU pressure; mutpool.py's pool, which holds the web gate lock shared around every run)."""
import os
import shutil
import sys
import time

import common
import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
C = 'themes/ember/computer.js'
S = 'themes/ember/computer-steps.js'
P = 'core/point-drag.js'
G = 'themes/ember/computer-page.js'
D = 'themes/ember/draw/computer.js'
STEP = 'the_computer_table_steps_like_the_dac_table'
# name -> (kind, edits [(file, old, new)], cases of computer_ember.py for a browser mutation)
MUTATIONS = {
    'a stepper steps the wrong way': ('browser', [
        (S, 'hand.step(n, param, Number(dir), { hold });',
         'hand.step(n, param, -Number(dir), { hold });')], [STEP]),
    'a stepper run writes no final value': ('browser', [
        (P, 'act.setPoint(b.writes, {}, { done: true });',
         'void b;')], [STEP]),
    'a step against its key moves the point': ('browser', [
        (P, '    if (next !== null) {', '    if (true) {')],
     [STEP]),
    'a held stepper ends its burst on the timer': ('browser', [
        (S, 'hand.step(n, param, Number(dir), { hold });',
         'hand.step(n, param, Number(dir), { hold: false });')], [STEP]),
    'a held stepper does not repeat': ('browser', [
        (S, '    live: usable,\n', '    live: () => false,\n')], [STEP]),
    'a stepper does not hold the part': ('browser', [
        (C, '    if (on) {\n      holdTone(true);\n      return;\n    }', '    if (on) {\n      return;\n    }')], [STEP, 'a_held_stepper_ends_at_a_flip']),
    'the editor stays hidden': ('browser', [
        (G, 'box.hidden = !point;', 'box.hidden = true;')], [STEP]),
    'a press on a row does not pick it': ('browser', [
        (C, 'if (row && !event.target.closest(\'select, input\')) table.pick(Number(row.dataset.n));',
         'if (row) void 0;')], [STEP]),
    'a cell is written as typed': ('browser', [
        (C, 'cell: act().cell,',
         'cell: (n, param, typed) => act().setPoint(n, { [param]: Number(typed) }, { done: true }),')],
     ['the_mode_the_type_a_typed_value_and_the_presets_write', 'an_untouched_cell_writes_nothing']),
    'a right press drags the frequency': ('browser', [
        (C, 'wide: event.button === 2', 'wide: false')], ['the_wheel_and_a_right_drag_set_the_q']),
    '+ Add point adds nothing': ('browser', [
        (C, 'act().addNext();', 'void 0;')], ['a_tap_adds_a_point_and_x_removes_it']),
    'the drag does not hold the tone part': ('browser', [
        (C, '  begin(s) {\n    holdTone(true);\n', '  begin(s) {\n')],
     ['a_point_drag_posts_live_and_done_at_release']),
    'a finger on a point also adds one': ('browser', [
        (C, 'if (grab(pointsOf(plot, rect), at, { reach: REACH.touch }) === null) {', 'if (true) {')],
     ['touch']),
    'the DAC editor types into the computer editor': ('browser', [
        ('themes/ember/tone-editor.js', "closest('#part-bands .bedit .v')", "closest('.bedit .v')")], [STEP]),
    'the recall question forgets its chip': ('browser', [
        (C, 'returnTo: control.id', 'returnTo: null')],
     ['the_same_preset_on_both_asks_and_yes_flattens_the_dac']),
    'the open question lets the other keys through': ('browser', [
        ('core/ask.js', "        } else {\n          event.stopImmediatePropagation();\n        }\n",
         "        }\n")],
     ['the_open_question_owns_the_keyboard']),
    'the theme picker opens behind the open question': ('browser', [
        ('core/frame.js', "    if (core.ask?.opened()) return;\n", "")],
     ['the_open_question_owns_the_keyboard']),
    'the preset chips have no ids': ('browser', [
        (D, "${attr('id', `pre-${index}`)}", '')],
     ['the_same_preset_on_both_asks_and_yes_flattens_the_dac']),
    'a key on a stepper does nothing': ('browser', [
        (S, 'export function pressedByKey(button) {\n  step(button);', 'export function pressedByKey(button) {\n  void button;')],
     [STEP]),
    'a stepper steps with the stream lost': ('browser', [
        (P, '    if (!usable()) return false;', '    void usable;')], [STEP]),
    'a preset is not saved': ('browser', [
        (C, 'answer = await act().savePreset(value(\'name\'));', 'answer = null;')],
     ['presets_save_rename_delete_from_the_computer_side']),
    'a preset is renamed to itself': ('browser', [
        (C, "act().renamePreset(form.dataset.from, value('to'))", "act().renamePreset(form.dataset.from, form.dataset.from)")],
     ['presets_save_rename_delete_from_the_computer_side']),
    'a preset delete names another preset': ('browser', [
        (C, 'act().deletePreset(form.dataset.from)', "act().deletePreset('none')")],
     ['presets_save_rename_delete_from_the_computer_side']),
    'a second finger off the graph leaves the tap': ('browser', [
        (C, "document.addEventListener('pointerdown', () => hand.dropTap(), mine({ capture: true }));", 'void 0;')],
     ['a_second_finger_off_the_graph_makes_the_tap_no_tap']),
    'a tap on a full table says nothing': ('browser', [
        (C, 'if (answer?.full) ember.core?.frame.toast(answer.error);', 'void answer;')],
     ['a_tap_on_a_full_table_says_so']),
    'a stepper is not inert for a viewer who may not write': ('node', [
        (D, "attr('data-dir', dir)}${attr('aria-label', `${dir < 0 ? 'Lower' : 'Raise'} the ${label.toLowerCase()}`)}${off}>",
         "attr('data-dir', dir)}${attr('aria-label', `${dir < 0 ? 'Lower' : 'Raise'} the ${label.toLowerCase()}`)}>")], []),
    'the minus stepper has a plus sign': ('node', [
        (D, "${dir < 0 ? MINUS : '+'}</button>", "+</button>")], []),
    'a stepper goes the other way': ('node', [
        (D, "attr('data-dir', dir)", "attr('data-dir', -dir)")], []),
    'the editor shows without a point in hand': ('node', [
        (D, "  : '<div class=\"bedit\" hidden></div>');", "  : '');")], []),
    'the editor says the raw value': ('node', [
        (D, '<span class="v">${esc(words[param])}</span>', '<span class="v">${esc(point[param])}</span>')], []),
    'the table says the editor is missing a stepper': ('node', [
        (D, "const STEPPERS = [['f', 'Frequency'], ['g', 'Gain'], ['q', 'Q']];",
         "const STEPPERS = [['f', 'Frequency'], ['g', 'Gain']];")], []),
}


BASE = mutpool.Baselines()
NAMES = {}  # the checks of a browser baseline, by its cases: a kill is a failing check of these


def unmutated(cases):
    """The problem of the unmutated copy's run of `cases`, or None (and then NAMES has its checks)."""
    web = mutpool.scratch_web([])
    try:
        lines, code, err = mutpool.run_checks('computer_ember.py', web, cases)
    finally:
        shutil.rmtree(web, ignore_errors=True)
    if code or any(line.startswith('FAIL') for _, line in lines) or not lines:
        return f'{cases} fails unmutated: ' + (' | '.join(x[:90] for _, x in lines if x.startswith('FAIL'))[:300] or err[-200:])
    NAMES[tuple(cases)] = sorted({c for c, _ in lines})
    return None


def run(name):
    kind, edits, cases = MUTATIONS[name]
    if kind == 'browser':
        problem = BASE.problem(tuple(cases), lambda: unmutated(cases))
        if problem:
            return None, '(error, not a kill) ' + problem
    web = mutpool.scratch_web(edits, name)
    try:
        if kind == 'node':
            # the harness sits where it sits in the repo, so its relative imports reach the copy
            node = f'{web}/t/scripts/web-checks/node'
            os.makedirs(node)
            os.makedirs(f'{web}/t/bins/svolderd')
            for harness in ('ember-parts.mjs', 'html-same.mjs'):
                shutil.copy(f'{HERE}/node/{harness}', node)
            os.symlink(f'{HERE}/node/fixtures', f'{node}/fixtures')
            os.symlink(f'{web}/w', f'{web}/t/bins/svolderd/web')
            r = mutpool.run_group(['node', f'{node}/ember-parts.mjs'], dict(os.environ), 300)
            return mutpool.verdict(mutpool.checks(r.stdout), r.returncode, r.stderr or r.stdout)
        lines, code, err = mutpool.run_checks('computer_ember.py', web, cases)
    finally:
        shutil.rmtree(web, ignore_errors=True)
    # a kill is a failing check that exists and passes unmutated; a crash (a RuntimeError out of the
    # driver) is an error, never a kill: it can be the browser or the load
    return mutpool.verdict(lines, code, err, NAMES[tuple(cases)])


def attempt(name):
    began = time.time()
    killed, note = run(name)
    return killed, note, time.time() - began


sys.exit(mutpool.run_names(sys.argv[1:] or list(MUTATIONS), attempt))
