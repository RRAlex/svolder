# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the effects registry and the engine's partial plans: each mutation edits a
copy of the page's web dir (never the repo), runs effects_cycle.py (Chromium) against it and must
make a check FAIL.
Usage: python3 mutate_registry.py [name ...]   (default: all). One worker."""
import os
import shutil
import subprocess
import sys

import common

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
    'a different label': [
        (E, "  label: 'Smoke',", "  label: 'Smokey',"),
    ],
    'a different label on Glow': [
        (E, "label: 'Glow', supported", "label: 'Glare', supported"),
    ],
}


def run(name):
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in MUTATIONS[name]:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    r = subprocess.run([sys.executable, f'{HERE}/effects_cycle.py', 'chromium'], env=env,
                       capture_output=True, text=True, timeout=900)
    shutil.rmtree(web, ignore_errors=True)
    fails = [x for x in r.stdout.splitlines() if x.startswith('FAIL')]
    if not fails and r.returncode:
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    return name, fails, ''


names = sys.argv[1:] or list(MUTATIONS)
bad = 0
for name in names:
    name, fails, err = run(name)
    print('ERROR   ' if fails is None else 'KILLED  ' if fails else 'SURVIVED', name, flush=True)
    for f in (fails or [])[:3]:
        print('    ', f[:160])
    if err:
        print('    ', err)
    bad += not fails
print(f'{len(names) - bad} of {len(names)} mutations killed')
sys.exit(1 if bad else 0)
