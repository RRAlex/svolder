# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the haze, its shade and its pace: each mutation edits a copy of the page's
web dir (never the repo), runs the sections of haze.py (Chromium, or Firefox with --firefox) that
must see it, and must make at least one check FAIL. Slow (a section is up to three minutes): run
by hand, one worker.
Usage: python3 mutate_haze.py [--firefox] [name ...]   (default: all)"""
import os
import shutil
import subprocess
import sys

import common

BROWSER = 'firefox' if '--firefox' in sys.argv else 'chromium'
HERE = os.path.dirname(os.path.abspath(__file__))
M = 'themes/ember/smoke-model.js'
S = 'themes/ember/smoke.js'
D = 'themes/ember/smoke-director.js'
E = 'themes/ember/effects.js'
# name: (the sections of haze.py that must see it, [(file, old, new), ...])
MUTATIONS = {
    'no haze is fed': ('phone', [
        (D, 'this.haze_.splats({ size, column: this.columnAt, now, dt: owed })', '[]'),
    ]),
    'no side gradient': ('phone', [
        (M, 'hazeSide: 0.024,', 'hazeSide: 0,'),
    ]),
    'the side bands are empty low down': ('phone', [
        (M, 'y = ((i % TUNING.hazeSideCount) + 0.5) * slot', 'y = 0.1 * ch + 0 * slot'),
    ]),
    'the haze follows the volume': ('wide', [
        (D, '    plan.draw = { bright: this.dim, sat: this.sat };',
         '    plan.draw = { bright: sourceStrength(this.lvl) * this.dim, sat: this.sat };'),
    ]),
    'no fade after the margins change': ('wide', [
        (D, 'this.fadeUntil = now + TUNING.hazeFadeSecs;', 'this.fadeUntil = now;'),
    ]),
    'a start without its haze': ('wide', [
        (M, '(first ? 1 : 0)', '0'),
    ]),
    'the haze stirs the rhythm': ('wide', [
        (D, '        rest: true,\n        ...uvOf(h, size),',
         '        rest: false,\n        ...uvOf(h, size),'),
    ]),
    'the column is not given': ('wide', [
        (E, '{ column: columnOf(view) }', 'null'),
        (E, '    return { x: cx, y: cy, radius, meters: [], column };',
         '    return { x: cx, y: cy, radius, meters: [] };'),
    ]),
    'a shade drawn at every start': ('colour', [
        (E, '  shade ??= drawShade(Math.random);\n  return {\n    shade,',
         '  shade = drawShade(Math.random);\n  return {\n    shade,'),
    ]),
    'no shade reaches the smoke': ('colour', [
        (E, '    shade: shade && {\n      hsl:', '    shade: null && {\n      hsl:'),
    ]),
    'the haze is fed in pulses': ('steady', [
        (M, '    const good = Number.isFinite(dt) && dt > 0;\n    if (this.fed && !good) return null;\n'
            '    this.fed = true;\n    return good ? dt : 0;',
         '    const good = Number.isFinite(dt) && dt > 0;\n'
         '    this.owed = (this.owed ?? 0) + (good ? dt : 0);\n'
         '    if (this.fed && this.owed < 0.25) return null;\n'
         '    const due = this.owed;\n    this.owed = 0;\n    this.fed = true;\n    return due;'),
    ]),
    'the two side tones are one': ('colour', [
        (M, 'sides: [hslToRgb(h - step, s, l), hslToRgb(h + step, s, l)],',
         'sides: [hslToRgb(h, s, l), hslToRgb(h, s, l)],'),
    ]),
    'the old pace': ('pace', [
        (M, 'velocityDissipation: 0.02,', 'velocityDissipation: 0.25,'),
        (M, 'dyeDissipation: 0.35,', 'dyeDissipation: 0.9,'),
        (M, '  curl: 10,', '  curl: 22,'),
    ]),
}


def run(name):
    sections, edits = MUTATIONS[name]
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in edits:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w', HAZE_SECTIONS=sections)
    r = subprocess.run([sys.executable, f'{HERE}/haze.py', BROWSER], env=env,
                       capture_output=True, text=True, timeout=900)
    shutil.rmtree(web, ignore_errors=True)
    fails = [x for x in r.stdout.splitlines() if x.startswith('FAIL')]
    if not fails and r.returncode:
        # no FAIL line: an infrastructure crash is an error, never a kill
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    return name, fails, ''


names = [a for a in sys.argv[1:] if not a.startswith('--')] or list(MUTATIONS)
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
