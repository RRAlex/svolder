# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the smoke's core and its page driver: each mutation edits a copy of the
page's web dir (never the repo), runs smoke_core.py (Chromium), or the sections of smoke_page.py
named in PAGE, against it and must make at least one check FAIL.
One worker (the default; do not raise MUT_WORKERS): the checks' timing-based bounds flake under
two.
Usage: python3 mutate_smoke.py [name ...]   (default: all)   Env: HIDDEN_SECS (default 5 here),
MUT_WORKERS (default 1)"""
import concurrent.futures, os, shutil, subprocess, sys
import common

HERE = os.path.dirname(os.path.abspath(__file__))
M = 'themes/ember/smoke-model.js'
S = 'themes/ember/smoke.js'
D, G = 'themes/ember/smoke-director.js', 'themes/ember/smoke-engine.js'
E = 'themes/ember/effects.js'
# ("zero size unguarded" is retired: the director's usable(size) makes the three guards redundant,
# and node/director.mjs "bad input" proves the size bounds; see COVERAGE.md.)
MUTATIONS = {
    'hidden keeps running': [
        (M, "  if (hidden) return 'stop';\n", ''),
        (S, 'if (dead || lost || hidden()) return;', 'if (dead || lost) return;'),
        (
            S,
            'const wake = () => {\n    if (raf || dead || lost || hidden()) return;',
            'const wake = () => {\n    if (raf || dead || lost) return;'
        ),
        (D, '    return this.isLost || !this.isVisible;', '    return this.isLost;'),
    ],
    'no dt clamp, no resume reset': [
        (
            M,
            ('return Number.isFinite(elapsedSecs) ? Math.min(Math.max(elapsedSecs, 0), '
             'TUNING.maxDt) : 0;'),
            'return Number.isFinite(elapsedSecs) ? Math.max(elapsedSecs, 0) : 0;'
        ),
        (
            D,
            '    this.last = this.time(now);\n    this.settled = false;\n  }',
            '    this.settled = false;\n  }'
        ),
        (D, '    if (this.isVisible) this.last = this.time(now);\n', ''),
    ],
    'reset listener left on at a chosen stop': [
        (
            E,
            ("  canvas?.removeEventListener('webglcontextlost', contextLost);\n  handle?.stop?.()"
             ';'),
            '  handle?.stop?.();'
        ),
    ],
    'stop keeps its context': [
        (G, '        lose?.loseContext();\n      }\n    },\n  };', '      }\n    },\n  };'),
    ],
    'away never stops': [
        (M, "  if (away && fadedAt !== null && now >= fadedAt) return 'stop';\n", ''),
    ],
    'scroll ignored': [
        (
            D,
            '        this.shiftX = x;\n        this.shiftY = y;',
            '        void x;\n        void y;'
        ),
    ],
    'resize wipes the cloud': [
        (
            G,
            "    if (grids) {\n      for (const key of ['vel', 'dye']) {",
            "    if (false) {\n      for (const key of ['vel', 'dye']) {"
        ),
    ],
    'context loss not handled': [
        (E, "    canvas.addEventListener('webglcontextlost', contextLost);\n", ''),
    ],
    'calm at 60 fps': [
        (M, 'calmFps: 20,', 'calmFps: 60,'),
    ],
    'no resting source': [
        (
            D,
            '    if (this.anchorAt) {\n      list.push(restSplat(',
            '    if (false) {\n      list.push(restSplat('
        ),
    ],
    'away keeps its colour': [
        (D, 'this.sat = ease(this.sat, this.isAway ? 0 : 1, dt, TUNING.fadeSecs);',
         'this.sat = 1;'),
        (D, 'this.sat = this.isAway ? 0 : this.sat;', ''),
    ],
    'a lost link keeps its colour': [
        (D, 'this.sat = ease(this.sat, this.isAway ? 0 : 1, dt, TUNING.fadeSecs);',
         'this.sat = 1;'),
        (D, 'this.sat = this.isAway ? 0 : this.sat;', ''),
    ],
    'dpr cap ignored': [
        (M, 'dprCap: 1.5,', 'dprCap: 4,'),
    ],
    'no resume reset on show': [
        (D, '    if (this.isVisible) this.last = this.time(now);\n', ''),
    ],
    'away does not dim': [
        (
            D,
            ('    this.dim = ease(this.dim, this.isAway ? TUNING.awayOpacity : 1, dt, '
             'TUNING.fadeSecs);\n'),
            ''
        ),
        (D, '        this.dim = this.isAway ? TUNING.awayOpacity : this.dim;\n', ''),
    ],
    'failed start leaves Smoke enabled': [
        (E, '      unavailable.set(effect.id, noStart(effect));\n', ''),
    ],
    'menu stays open on a history step': [
        (
            'core/menu.js',
            "  window.addEventListener('popstate', () => close(false));\n",
            ''
        ),
    ],
    'no popup text': [
        (E, '  say(RESET_SAID);\n', ''),
    ],
    'canvas clipped to the column': [
        ('themes/ember/style.css', '.fx .smoke { position: absolute;',
         '.fx .smoke { clip-path: inset(0 680px); position: absolute;'),
    ],
    'lost link does not reach Smoke': [
        (E, '  isLost = Boolean(on);\n  handle?.away?.(isAway || isLost);',
         '  isLost = Boolean(on);'),
    ],
    'probe at load': [
        (E, "  let picked = pick({ ...wanted, can: { ok: true } });",
         "  probe();\n  let picked = pick({ ...wanted, can: { ok: true } });"),
    ],
    'probe not remembered': [
        (E, '  if (probed) return;\n', ''),
    ],
    'menu does not probe': [
        (E, 'export function opening() {\n  probe();\n}', 'export function opening() {\n}'),
    ],
    'start throw not caught': [
        (E, "    try {\n      handle = effect.start(canvas, theme(), "
            "{ say: (words) => say(words) });\n"
            "    } catch (error) {\n      console.warn(`svolder: ${effect.label} threw while "
            "starting, so the page shows Glow`,\n        error);\n      handle = null;\n    }",
         "    handle = effect.start(canvas, theme(), { say: (words) => say(words) });"),
    ],
    'probe throw not caught': [
        (E, "    } catch (error) {\n      console.warn(\n        `svolder: the check for "
            "${effect.label} threw, so ${effect.label} is off for this page`,\n        error,\n"
            "      );\n      can = { ok: false, why: noStart(effect) };\n    }",
         '    } finally {\n    }'),
    ],
    'effects init not guarded': [
        ('themes/ember/ember.js', "  try {\n    effects.init({ say: core.frame.toast, changed: () => core.frame.realign(), signal });\n"
         "  } catch (error) {\n"
         "    console.warn('svolder: the effects threw while setting up, so the page shows Glow', "
         "error);\n    const layer = document.querySelector('.fx');\n    if (layer && "
         "!layer.dataset.level) layer.dataset.level = 'glow';\n  }",
         '  effects.init({ say: core.frame.toast, changed: () => core.frame.realign(), signal });'),
    ],
    'glow shown before init': [
        ('themes/ember/style.css', '.fx:not([data-level]) .glow { opacity: 0; }\n', ''),
    ],
    're-pick restarts the smoke': [
        (E, '        if (id !== shown) show(id);', '        show(id);'),
    ],
    'meters stir the rhythm': [
        (D, '        rest: !(this.meterShown[i] >= TUNING.meterRed),', '        rest: false,'),
    ],
    'red kick does not stir': [
        (D, '        rest: !(this.meterShown[i] >= TUNING.meterRed),', '        rest: true,'),
    ],
    'splat not scissored': [
        (G, '    gl.enable(gl.SCISSOR_TEST);\n    gl.scissor(box.x, box.y, box.w, box.h);\n', ''),
    ],
    'scissor box is the whole grid': [
        (G, '    gl.scissor(box.x, box.y, box.w, box.h);',
         '    gl.scissor(0, 0, target.read.w, target.read.h);'),
    ],
    'start catch silent': [
        (E, "      console.warn(`svolder: ${effect.label} threw while starting, so the page "
            "shows Glow`,\n        error);\n", ''),
    ],
    'probe catch silent': [
        (E, "      console.warn(\n        `svolder: the check for ${effect.label} threw, so "
            "${effect.label} is off for this page`,\n        error,\n      );\n", ''),
    ],
    'init catch silent': [
        ('themes/ember/ember.js', "    console.warn('svolder: the effects threw while setting up, so the page "
                    "shows Glow', error);\n", ''),
    ],
    'copy not probed': [
        (G, '      complete = gl.getError() === gl.NO_ERROR;\n      gl.deleteTexture(into);',
         '      gl.deleteTexture(into);'),
    ],
    'context kept at a start throw': [
        (S, '    engine.stop();\n    throw error;', '    throw error;'),
    ],
    'scissor left on': [
        (G, '    gl.disable(gl.SCISSOR_TEST);\n    gl.bindTexture', '    gl.bindTexture'),
    ],
    'size read by the pointer': [
        (S, '  const cssSize = () => sized;',
         '  const cssSize = () => [canvas.clientWidth, canvas.clientHeight];'),
    ],
}


# the mutations smoke_page.py covers, with its sections that must see them
PAGE = {
    'lost link does not reach Smoke': ['lost_link'],
    'a lost link keeps its colour': ['lost_link'],
    'probe at load': ['lazy_probe'],
    'probe not remembered': ['lazy_probe'],
    'menu does not probe': ['lazy_probe'],
    'start throw not caught': ['start_throws'],
    'probe throw not caught': ['start_throws'],
    'effects init not guarded': ['start_throws'],
    'glow shown before init': ['first_paint'],
    're-pick restarts the smoke': ['repick'],
    'meters stir the rhythm': ['calm_rhythm'],
    'red kick does not stir': ['calm_rhythm'],
    'splat not scissored': ['cost'],
    'scissor box is the whole grid': ['cost'],
    'start catch silent': ['start_throws'],
    'probe catch silent': ['start_throws'],
    'init catch silent': ['start_throws'],
    'copy not probed': ['start_cleanup'],
    'context kept at a start throw': ['start_cleanup'],
    'scissor left on': ['cost'],
    'size read by the pointer': ['cost'],
}


def run(name):
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in MUTATIONS[name]:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w', HIDDEN_SECS=os.environ.get('HIDDEN_SECS', '5'))
    cmd = ([f'{HERE}/smoke_page.py', 'chromium', *PAGE[name]] if name in PAGE
           else [f'{HERE}/smoke_core.py', 'chromium'])
    r = subprocess.run([sys.executable, *cmd], env=env, capture_output=True, text=True,
                       timeout=900)
    shutil.rmtree(web, ignore_errors=True)
    fails = [l for l in r.stdout.splitlines() if l.startswith('FAIL')]
    if not fails and r.returncode:
        # no FAIL line: an infrastructure crash is an error, never a kill
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    return name, fails, ''


names = sys.argv[1:] or list(MUTATIONS)
bad = 0
with concurrent.futures.ThreadPoolExecutor(int(os.environ.get('MUT_WORKERS', '1'))) as ex:
    for name, fails, err in ex.map(run, names):
        print('ERROR   ' if fails is None else 'KILLED  ' if fails else 'SURVIVED', name)
        for f in (fails or [])[:3]:
            print('    ', f[:160])
        if err:
            print('    ', err)
        bad += not fails
print(f'{len(names) - bad} of {len(names)} mutations killed')
sys.exit(1 if bad else 0)
