# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the effects registry and the engine's partial plans: each mutation edits a
copy of the page's web dir (never the repo), runs effects_cycle.py (Chromium) against it and must
make a check FAIL.
Usage: python3 mutate_registry.py [name ...]   (default: all). Env: MUT_WORKERS (default from the cores and the CPU
pressure; mutpool.py's pool)."""
import os
import sys

import mutpool

HERE = os.path.dirname(os.path.abspath(__file__))
E = 'themes/ember/effects.js'
G = 'themes/ember/smoke-engine.js'
MUTATIONS = {
    'the engine needs a plan\'s velocityLoss': [
        (G, "(plan.velocityLoss ?? 0)", "plan.velocityLoss"),
    ],
    'one effect fewer registered': [
        (E, "register({ id: 'glow', label: 'Glow',", "void ({ id: 'glow', label: 'Glow',"),
    ],
}


def run(name):
    return mutpool.run_script(name, 'effects_cycle.py', MUTATIONS[name])


def attempt(name):
    return mutpool.timed_call(run, name)


# effects_cycle.py asserts the calm-rate band, a pace the gate runs only in its exclusive lane:
# every registry mutant runs alone, as haze's and scroll's do.
names = sys.argv[1:] or list(MUTATIONS)
sys.exit(mutpool.run_names(names, attempt, names))
