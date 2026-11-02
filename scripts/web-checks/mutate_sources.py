# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for what stirs the smoke: each mutation edits a copy of the page's web dir (never
the repo), runs sources.py (Chromium) against it and must make at least one check FAIL (a crash
without a FAIL line is reported as an error, not a kill).
One worker (the default; do not raise MUT_WORKERS): the checks' timing-based bounds flake under
two.
Usage: python3 mutate_sources.py [name ...]   (default: all)   Env: MUT_WORKERS (default 1)"""
import concurrent.futures, os, shutil, subprocess, sys
import common

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
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in MUTATIONS[name]:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    r = subprocess.run([sys.executable, f'{HERE}/sources.py', 'chromium'], env=env,
                       capture_output=True, text=True, timeout=900)
    shutil.rmtree(web, ignore_errors=True)
    fails = [l for l in r.stdout.splitlines() if l.startswith('FAIL')]
    if not fails and r.returncode:
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    return name, fails, ''


names = sys.argv[1:] or list(MUTATIONS)
bad = 0
with concurrent.futures.ThreadPoolExecutor(int(os.environ.get('MUT_WORKERS', '1'))) as ex:
    for name, fails, err in ex.map(run, names):
        print('ERROR   ' if fails is None else 'KILLED  ' if fails else 'SURVIVED', name)
        for f in (fails or [])[:6]:
            print('    ', f[:110])
        if err:
            print('    ', err)
        bad += not fails
print(f'{len(names) - bad} of {len(names)} mutations killed')
sys.exit(1 if bad else 0)
