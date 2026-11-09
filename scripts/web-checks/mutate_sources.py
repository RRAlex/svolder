# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for what stirs the smoke: each mutation edits a copy of the page's web dir (never
the repo), runs sources.py (Chromium) against it and must make at least one check FAIL (a crash
without a FAIL line is reported as an error, not a kill).
One at a time whatever MUT_WORKERS says (the checks' timing-based bounds flake side by side); the web
gate lock is held shared around each run.
Usage: python3 mutate_sources.py [name ...]   (default: all)"""
import os
import sys

import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
M, S, ST, MT = ('themes/ember/smoke-model.js', 'themes/ember/smoke.js', 'themes/ember/live.js',
                'themes/ember/meters.js')
D = 'themes/ember/smoke-director.js'
LISTEN = "  window.addEventListener('pointermove', onPointer, { passive: true });\n"
# the first line of the pointer handler, which the pointer mutations anchor on, and of the
# director's pointer rule (the away check lives there)
POINTER = "    if (dead || lost || !engine.ready || event.pointerType === 'touch') return;\n"
RULE = "    if (this.isLost || this.isAway || !usable(size)) return false;\n"
RULE_NO_AWAY = "    if (this.isLost || !usable(size)) return false;\n"
# "pointer: y not flipped" and "puff: not at the dot" run in node (node/mutate_director.sh), since
# sources.py no longer reads the velocity's sign or the puff's place on a handle of its own.
MUTATIONS = {
    'pointer: no listener': [
        (S, LISTEN, ''),
    ],
    'pointer: listener not removed at stop': [
        (S, "      window.removeEventListener('pointermove', onPointer);\n", ''),
    ],
    'pointer: listener not passive': [
        (
            S,
            "window.addEventListener('pointermove', onPointer, { passive: true });",
            "window.addEventListener('pointermove', onPointer);"
        ),
    ],
    'pointer: preventDefault': [
        (
            S,
            POINTER + '    const size = cssSize();\n    const moves',
            POINTER + ('    event.preventDefault();\n    const size = cssSize();\n'
                       '    const moves')
        ),
    ],
    'pointer: setPointerCapture': [
        (
            S,
            POINTER + '    const size = cssSize();\n    const moves',
            POINTER + ('    document.body.setPointerCapture?.(event.pointerId);\n'
                       '    const size = cssSize();\n    const moves')
        ),
    ],
    'pointer: always pressed': [
        (S, 'event.buttons !== 0, size,', 'true, size,'),
    ],
    'pointer: never pressed': [
        (S, 'event.buttons !== 0, size,', 'false, size,'),
    ],
    'pointer: no maxPush cap': [
        (M, 'maxPush: 80,', 'maxPush: 1e6,'),
    ],
    'pointer: coalesced events ignored': [
        (S, 'const moves = event.getCoalescedEvents?.() ?? [];', 'const moves = [];'),
    ],
    'pointer: no pause rule': [
        (
            D,
            'if (before && here.t - before.t <= TUNING.pointerGapSecs) {',
            'if (before) {'
        ),
    ],
    'pointer: queue not capped': [
        (M, 'maxPointerSplats: 12,', 'maxPointerSplats: 1e9,'),
    ],
    'pointer: stirs while away': [
        (
            D,
            RULE,
            RULE_NO_AWAY
        ),
        (
            D,
            ('    if (this.isAway) {\n      this.pointerQueue = [];\n      this.puffQueue = [];\n'
             '      this.scrollDy = 0;\n      this.scrollV = 0;\n      return list;\n    }\n'),
            '    if (this.isAway) {\n      this.scrollDy = 0;\n      this.scrollV = 0;\n    }\n'
        ),
    ],
    'puff: the page never steps': [
        (
            ST,
            ('    effects.step(moved.direction, moved.size, knob.fraction(data.raw),\n'
             '      effects.bigRise({ ...rise, enteredRed }));\n'),
            ''
        ),
    ],
    'puff: first reading counts': [
        (
            ST,
            'effects.stepOf(lastVolume, now)',
            'effects.stepOf(lastVolume ?? { raw: 0, lo: knob.lo, hi: knob.hi }, now)'
        ),
    ],
    'puff: not forgotten on a screen swap': [
        (ST,
         '  function modelChanged() {\n    forgetVolume();\n',
         '  function modelChanged() {\n'),
    ],
    'puff: not forgotten on a stream open': [
        (ST, "    store.on('opened', forgetVolume),\n", ''),
    ],
    'puff: queue not capped': [
        (M, 'maxPuffs: 8,', 'maxPuffs: 1e9,'),
    ],
    'puff: no force cap': [
        (
            M,
            'const share = unit(size / TUNING.puffFullSize)',
            'const share = (size / TUNING.puffFullSize)'
        ),
    ],
    'meters: not wired in the page': [
        (ST, '    effects.meters(meters.readings());\n', ''),
    ],
    'meters: ignored by the handle': [
        (D, '    this.meterTargets = fractions.map(', '    this.meterTargets = [].map('),
    ],
    'meters: order reversed': [
        (D, 'this.meterTargets[i] ?? 0',
         'this.meterTargets[this.meterTargets.length - 1 - i] ?? 0'),
    ],
    'meters: readings out of order': [
        (
            MT,
            'return [...this.arches.values()].map((arch) => arch.fraction);',
            'return [...this.arches.values()].map((arch) => arch.fraction).reverse();'
        ),
    ],
    'meters: readings kept when the anchor loses its meters': [
        (
            D,
            ('    if (before && (!this.anchorAt || this.anchorAt.meters.length !== '
             'before.meters.length)) {\n      this.meterTargets = [];\n      this.meterShown = '
             '[];\n      this.meterAngles = [];\n    }\n'),
            ''
        ),
    ],
    'meters: readings kept through away': [
        (
            D,
            ('    if (next) {\n      this.meterTargets = [];\n      this.meterShown = [];\n'
             '      this.meterAngles = [];\n    }\n'),
            '    if (next) {\n      this.meterAngles = [];\n    }\n'
        ),
    ],
    'meters: iterates the readings, not the anchor\'s meters': [
        (
            D,
            '    meters.forEach((at, i) => {\n      if (!at) return;',
            '    this.meterTargets.forEach((_, i) => {\n      const at = meters[i];'
        ),
    ],
}


def run(name):
    return mutpool.run_script(name, 'sources.py', MUTATIONS[name])


def attempt(name):
    return mutpool.timed_call(run, name)


sys.exit(mutpool.run_names(sys.argv[1:] or list(MUTATIONS), attempt, serial=True))
