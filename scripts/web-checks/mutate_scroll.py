# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the scroll stir and the smoke's lost upward push: each mutation edits a copy
of the page's web dir (never the repo), runs the sections of scroll.py (Chromium) that must see it,
and must make at least one check FAIL. Slow: run by hand. Every mutant runs alone, one at a time (the bounds depend on the frame rate).
Usage: python3 mutate_scroll.py [name ...]   (default: all)"""
import os
import sys

import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
M = 'themes/ember/smoke-model.js'
S = 'themes/ember/smoke.js'
D = 'themes/ember/smoke-director.js'
E = 'themes/ember/effects.js'
G = 'themes/ember/smoke-engine.js'
# name: (the sections of scroll.py that must see it, [(file, old, new), ...])
MUTATIONS = {
    'scrolling stirs nothing': ('desktop', [
        (D, '    list.push(...this.scrollSplats(size, dt, now));\n', ''),
    ]),
    'the stir goes against the scroll': ('desktop', [
        (D, 'pull: [0, push, 0, 0],', 'pull: [0, -push, 0, 0],'),
    ]),
    'a fling is not capped': ('desktop', [
        (M, 'Math.min(speed * TUNING.scrollPush, TUNING.maxScrollPush)',
         'speed * TUNING.scrollPush'),
    ]),
    'a page that does not scroll is stirred': ('short', [
        (E, 'if (handle && handle === before && (dx !== 0 || dy !== 0)) handle.scroll?.(dx, dy);',
         'if (handle && handle === before) handle.scroll?.(dx, dy);'),
        (D, '      const d = this.scrollDy + dy;', '      const d = this.scrollDy + (dy || 100);'),
    ]),
    'the resting source pushes up again': ('rest', [
        (M, 'Math.cos(angle) * TUNING.restDrift * strength,', '0,'),
        (M, 'Math.sin(angle) * TUNING.restDrift * strength, 0, 0],',
         '3 * TUNING.restDrift * strength, 0, 0],'),
    ]),
    'a settled scroll keeps stirring': ('desktop', [
        (M, 'scrollFloor: 0.05,', 'scrollFloor: 0,'),
    ]),
    'scrolled-out dye is dropped': ('smear', [
        (G, 'const keep = Math.exp(-tuning.dyeDissipation * Math.max(0, simTime - heldAt));',
         'const keep = 0;'),
    ]),
    'the strip beyond the hold copies the edge': ('smear', [
        (G, '  o = inside ? texture(uSrc, at) : vec4(0.0);',
         '  o = texture(uSrc, clamp(at, 0.0, 1.0));'),
    ]),
    'the touch pan stirs nothing': ('touch', [
        (D, '    list.push(...this.scrollSplats(size, dt, now));\n', ''),
    ]),
}


def run(name):
    sections, edits = MUTATIONS[name]
    return mutpool.run_script(name, 'scroll.py', edits, {'SCROLL_SECTIONS': sections}, secs=600)


def attempt(name):
    return mutpool.timed_call(run, name)


names = sys.argv[1:] or list(MUTATIONS)
# the stir's bounds depend on the frame rate: every mutant runs alone, one at a time
sys.exit(mutpool.run_names(names, attempt, names))
