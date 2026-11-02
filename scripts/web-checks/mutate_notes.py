# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the smoke's notes: each mutation edits a copy of the page's web dir (never
the repo), runs notes.py (Chromium) against it and must make at least one check FAIL (a crash
without a FAIL line is reported as an error, not a kill).
One worker (the default; do not raise MUT_WORKERS): the checks' timing-based bounds flake under
two.
Usage: python3 mutate_notes.py [name ...]   (default: all)   Env: MUT_WORKERS (default 1)"""
import concurrent.futures, os, shutil, subprocess, sys
import common

HERE = os.path.dirname(os.path.abspath(__file__))
M, S, N = 'themes/ember/smoke-model.js', 'themes/ember/smoke.js', 'themes/ember/smoke-notes.js'
D, G = 'themes/ember/smoke-director.js', 'themes/ember/smoke-engine.js'
ST, E = 'themes/ember/live.js', 'themes/ember/effects.js'
SCREEN = "control && control.closest('#screen') && control.getAttribute"
NOT_CONTROLS = "const NOT_CONTROLS = '.dev, .fxbtn, #fx-menu';"
STAMP = "    plan.note = this.noteAt(now, size);\n"
# the first line of the director's pointer rule, which the pointer mutations anchor on
POINTER = "    if (this.isLost || this.isAway || !usable(size)) return false;\n"
POINTER_NO_AWAY = "    if (this.isLost || !usable(size)) return false;\n"
# These rules are mutated by the node harness: "stamps while away", "away leaves the pending stir
# standing", "a click works while away", "a scroll does not stir the notes", "the haze stirs the
# notes", "meters feed it below red", "meters fed from the eased reading, not the peak"
# (node/mutate_director.sh) and "pointer feeds it far too much", "pointer fed from the capped speed"
# (node/mutate_notes.sh).
MUTATIONS = {
    'no stamp pass': [
        (D, STAMP, ''),
    ],
    'gate ignored': [
        (D, '    if (!this.gate.allows(now)) return null;\n', ''),
    ],
    'gate never records': [
        (D, '    this.gate.record(now);\n', ''),
    ],
    'a click has no puff': [
        (
            D,
            ('    this.puffQueue.push({\n      ...clickPuff({ x, y }, this.random() * 2 * Math.PI),'
             ' rgb: shadePick(this.shade, this.random),\n    });\n'),
            ''
        ),
    ],
    'the real lost link leaves the pending stir standing': [
        (
            E,
            'handle?.away?.(isAway || isLost);\n}\n\n/** The centre',
            'handle?.away?.(isAway);\n}\n\n/** The centre'
        ),
    ],
    'the real lost link: the smoke keeps the stir': [
        (D, '    this.watch.reset();\n    this.fadedAt', '    this.fadedAt'),
    ],
    'a click is ignored by the smoke': [
        (D, '    this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.clickOdds);\n', ''),
    ],
    'a click has the settle odds': [
        (
            D,
            'this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.clickOdds)',
            'this.watch.stir(TUNING.clickAgit, x, y, t, TUNING.settleOdds)'
        ),
    ],
    'a big rise has the settle odds': [
        (D, 'big ? TUNING.bigRiseOdds : TUNING.settleOdds', 'TUNING.settleOdds'),
    ],
    'every rise is a big one': [
        (D, 'big ? TUNING.bigRiseOdds : TUNING.settleOdds', 'TUNING.bigRiseOdds'),
    ],
    'the page never says a rise is big': [
        (ST, '      effects.bigRise({ ...rise, enteredRed }));', '      false);'),
    ],
    'the page counts every rise as the red zone': [
        (
            ST,
            ('const enteredRed = knob.risk !== null && before.raw < knob.risk '
             '&& data.raw >= knob.risk;'),
            'const enteredRed = true;'
        ),
    ],
    'the header counts as a control': [
        (E, NOT_CONTROLS, "const NOT_CONTROLS = '.zzz';"),
        (E, SCREEN, 'control && control.getAttribute'),
    ],
    'a click anywhere counts': [
        (E, SCREEN, 'control && control.getAttribute'),
        (E, NOT_CONTROLS, "const NOT_CONTROLS = '.zzz';"),
        (E, "const CLICKABLE = 'button, [role=switch]",
         "const CLICKABLE = 'header, button, [role=switch]"),
    ],
    'pointer does not feed the agitation': [
        (
            D,
            ('        this.watch.stir(pointerAgitation({\n          dist: Math.hypot(here.x - '
             'before.x, here.y - before.y), dt: here.t - before.t,\n          shortSide: '
             'short,\n        }), here.x, here.y, t, TUNING.settleOdds);\n'),
            ''
        ),
    ],
    'volume step does not feed it': [
        (
            D,
            '    this.watch.stir(stepStir(size), dot.x, dot.y, t, odds);\n',
            ''
        ),
    ],
    'meters do not feed it': [
        (
            D,
            ('      this.watch.stir(meterAgitation(peak.fraction, TUNING.meterRed, dt), peakAt.x, '
             'peakAt.y, now,\n        TUNING.settleOdds);\n'),
            ''
        ),
    ],
    'a single volume step does not arm a settle': [
        (M, 'agitStepMin: 1.5,', 'agitStepMin: 0.1,'),
    ],
    'settles come at once': [
        (M, 'settleSecs: 0.8,', 'settleSecs: 0,'),
        (M, 'settleBelow: 0.25,', 'settleBelow: 50,'),
    ],
    'stamp placed at the anchor, not where': [
        (
            D,
            'const spot = this.noteSpot(notePosition(at, cell * 0.71, size), cell, size);',
            'const spot = notePosition({ x: 1000, y: 100 }, cell * 0.71, size);'
        ),
    ],
    'stamp not clamped into the canvas': [
        (
            D,
            'const spot = this.noteSpot(notePosition(at, cell * 0.71, size), cell, size);',
            'const spot = { x: at.x - 5000, y: at.y };'
        ),
    ],
    'size read as the cell, not the glyph': [
        (D, 'const cell = noteCell(height);', 'const cell = height;'),
    ],
    'stamp too small': [
        (M, 'noteSizeMin: 0.06,', 'noteSizeMin: 0.02,'),
        (M, 'noteSizeMax: 0.09,', 'noteSizeMax: 0.03,'),
    ],
    'a note carves no gap': [
        (M, 'noteCarve: 0.85,', 'noteCarve: 0,'),
    ],
    'a note forms in empty sky': [
        (S, 'density: () => engine.density(),', 'density: () => null,'),
    ],
    'a note is left under its control': [
        (E, '    open: openBox,\n', ''),
    ],
    'stamp has no ink': [
        (M, 'noteInk: 0.7,', 'noteInk: 0,'),
    ],
    'stamp has no halo': [
        (M, 'noteHalo: 0.25,', 'noteHalo: 0,'),
    ],
    'stamp halo far too strong': [
        (M, 'noteHalo: 0.25,', 'noteHalo: 3,'),
    ],
    'stamp turned the wrong way': [
        (G, 'uRot: [Math.cos(note.angle), Math.sin(note.angle)]',
         'uRot: [Math.cos(note.angle), -Math.sin(note.angle)]'),
    ],
    'stamp upside down': [
        (
            G,
            'p = vec2(uRot.x * p.x + uRot.y * p.y, -uRot.y * p.x + uRot.x * p.y) / uSize + 0.5;',
            ('p = vec2(uRot.x * p.x + uRot.y * p.y, -uRot.y * p.x + uRot.x * p.y) / uSize + '
             '0.5;\n  p.y = 1.0 - p.y;')
        ),
    ],
    'stamp reads the wrong glyph': [
        (
            G,
            'vec2((uGlyph + p.x) / 3.0, p.y)',
            'vec2((mod(uGlyph + 1.0, 3.0) + p.x) / 3.0, p.y)'
        ),
    ],
    'atlas uploaded flipped': [
        (G, 'gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);\n', ''),
    ],
    'atlas not freed at stop': [
        (G, '        if (atlasTex) gl.deleteTexture(atlasTex.tex);\n', ''),
    ],
    'atlas halo lost': [
        (M, 'noteHaloBlur: 0.07,', 'noteHaloBlur: 0,'),
        (M, 'noteHaloWidth: 0.05,', 'noteHaloWidth: 0,'),
    ],
    'stamp ignores the tint': [
        (G, 'col = mix(col, uTint.rgb, m.r * uInk);', 'col = mix(col, col, m.r * uInk);'),
    ],
    'an earlier GL error turns the notes off': [
        (
            G,
            ('    for (let i = 0; i < 16; i++) { // an earlier error is not ours\n      if '
             '(gl.getError() === gl.NO_ERROR) break;\n    }\n'),
            ''
        ),
    ],
    'a failed atlas is not caught': [
        (
            G,
            '  } catch {\n    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);\n  }',
            '  } catch (e) {\n    throw e;\n  }'
        ),
    ],
}


# the mutations of the settle and click rules, which `notes.py settle` alone kills
QUICK = {
    'a click is ignored by the smoke',
    'a click has the settle odds', 'a big rise has the settle odds',
    'every rise is a big one', 'the page never says a rise is big',
    'the page counts every rise as the red zone', 'the header counts as a control',
    'a click anywhere counts',
    'a single volume step does not arm a settle',
}


# the mutations of the real lost-link path, which `smoke_page.py chromium lost_link` kills
LOST = {
    'the real lost link leaves the pending stir standing',
    'the real lost link: the smoke keeps the stir',
}
QUICK |= {'a click has no puff'}

# the mutations of what a person sees, which `notes.py visible` alone kills
VISIBLE = {'a note carves no gap', 'a note forms in empty sky',
           'a note is left under its control'}


def run(name):
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in MUTATIONS[name]:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    if name in LOST:
        argv = [f'{HERE}/smoke_page.py', 'chromium', 'lost_link']
    elif name in VISIBLE:
        argv = [f'{HERE}/notes.py', 'chromium', 'visible']
    else:
        argv = [f'{HERE}/notes.py', 'chromium', *(['settle'] if name in QUICK else [])]
    r = subprocess.run([sys.executable, *argv], env=env,
                       capture_output=True, text=True, timeout=1500)
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
