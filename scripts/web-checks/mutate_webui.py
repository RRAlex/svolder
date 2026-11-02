# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the Tone pane's mode message and Loudness steps (tone_note.py), the effects
menu's q key (effects_menu.py), the volume knob's ring click (knob_click.py), the folded Now playing
line and its title's loop (playing.py) and what ignores reduced motion (reduced_motion.py): each
mutation edits a copy of the page's web dir (never the repo), runs the check that must see it
(Chromium) and must make at least one check FAIL. Slow: run by hand, one worker.
Usage: python3 mutate_webui.py [name ...]   (default: all)"""
import os
import shutil
import subprocess
import sys

import common

HERE = os.path.dirname(os.path.abspath(__file__))
ST = 'core/tone-note.js'
TE = 'themes/ember/tone-editor.js'
K = 'themes/ember/knob.js'
E = 'themes/ember/effects.js'
PL = 'themes/ember/playing.js'
M = 'themes/ember/meters.js'
TM = 'themes/ember/tone-motion.js'
RM = "matchMedia('(prefers-reduced-motion: reduce)').matches"
CSS = 'themes/ember/style.css'
JUMPS = '      const jumps = !mode && !climbed && this.ringHit(x0, y0);\n'
# name: (the check that must see it, [(file, old, new), ...])
MUTATIONS = {
    'the mode message is never shown': ('tone_note.py', [
        (ST, "if (mode !== null && tone.note && (mine || (before !== null && mode !== before))) {",
         "if (false) {"),
    ]),
    'a click on a mode is not the person\'s own': ('tone_note.py', [
        (ST, "if (mode !== null && tone.note && (mine || (before !== null && mode !== before))) {",
         "if (mode !== null && tone.note && (before !== null && mode !== before)) {"),
    ]),
    'a reload shows the mode message': ('tone_note.py', [
        (ST, "if (mode !== null && tone.note && (mine || (before !== null && mode !== before))) {",
         "if (mode !== null && tone.note) {"),
    ]),
    'the Loudness + steps down': ('tone_note.py', [
        (TE, "  const body = JSON.parse(button.dataset.json);\n  const { actions }",
         "  const body = { step: -1 };\n  const { actions }"),
    ]),
    'stepOnce leaves the Loudness words as they were': ('tone_note.py', [
        (TE, "    words.textContent = answer.text;\n",
         "    words.textContent = words.textContent;\n"),
    ]),
    'a held Loudness button is lost when its part is swapped': ('tone_note.py', [
        (TE, "  if (button.isConnected) return button;\n", "  return button;\n"),
    ]),
    'a colour-mix background counts as transparent': ('tone_note.py', [
        (E, "const fn = css.match(/^[a-z-]+\\((.*)\\)$/i);\n  if (!fn) return 1;",
         "const fn = css.match(/^rgba?\\((.*)\\)$/i);\n  if (!fn) return 0;"),
    ]),
    'q does not close the effects menu': ('effects_menu.py', [
        ('core/menu.js', "key.toLowerCase() === 'q'", "false"),
    ]),
    'the ring click is a step off': ('knob_click.py', [
        (K, 'return this.lo + Math.round(', 'return this.lo + 1 + Math.round('),
    ]),
    'a click in the gap jumps': ('knob_click.py', [
        (K, 'if (Math.abs(deg) > SWEEP / 2) return null;', 'if (false) return null;'),
    ]),
    'a click while a climb shows jumps': ('knob_click.py', [
        (K, JUMPS, '      const jumps = !mode && this.ringHit(x0, y0);\n'),
    ]),
    'the readout takes a click': ('knob_click.py', [
        (K, JUMPS, '      const jumps = !mode && !climbed;\n'),
    ]),
    'the dial box beyond the ring takes a click': ('knob_click.py', [
        (K, '> radius * RING_REACH) return false;', '> radius * 1e9) return false;'),
    ]),
    'the knob does not ease under reduced motion': ('reduced_motion.py', [
        (K, 'from === level || document.hidden', f'from === level || {RM} || document.hidden'),
    ]),
    'a held peak jumps under reduced motion': ('reduced_motion.py', [
        (M, 'arch.peak = Math.max(arch.fraction, arch.peak - FALL);',
         f'arch.peak = {RM} ? arch.fraction : Math.max(arch.fraction, arch.peak - FALL);'),
    ]),
    'the tone curve does not ease under reduced motion': ('reduced_motion.py', [
        (TM, '|| from.length !== to.length) return false;',
         f'|| from.length !== to.length || {RM}) return false;'),
    ]),
    'a changed band value does not flash under reduced motion': ('reduced_motion.py', [
        (TM, 'if (!element) return;', f'if (!element || {RM}) return;'),
    ]),
    'the glow stands still under reduced motion': ('reduced_motion.py', [
        (CSS, '.glow { animation: glow-turn 35s linear infinite; }',
         '@media (prefers-reduced-motion: no-preference) {\n'
         '  .glow { animation: glow-turn 35s linear infinite; }\n}'),
    ]),
    'the glow keeps its slow fade under reduced motion': ('reduced_motion.py', [
        (CSS, '.fx.settled.hearing .glow { transition-duration: .3s; }',
         '@media (prefers-reduced-motion: no-preference) {\n'
         '  .fx.settled.hearing .glow { transition-duration: .3s; }\n}'),
    ]),
    'a catch-all stills the page under reduced motion': ('reduced_motion.py', [
        (CSS, '@keyframes pulse { 50% { opacity: .35; } }',
         '@keyframes pulse { 50% { opacity: .35; } }\n'
         '@media (prefers-reduced-motion: reduce) {\n'
         '  .fx { --lvl: .6 !important; }\n'
         '  *, *::before, *::after { animation: none !important; transition: none !important; }\n'
         '}'),
    ]),
    'a cut title does not loop': ('playing.py', [
        (PL, 'el.animate(plan.frames, timing).startTime = start;', 'void start;'),
    ]),
    'the loop runs one iteration only': ('playing.py', [
        (PL, 'iterations: Infinity, easing', 'iterations: 1, easing'),
    ]),
    'the loop goes at another pace': ('playing.py', [
        (PL, 'const PACE = 30;', 'const PACE = 60;'),
    ]),
    'the loop is eased instead of linear': ('playing.py', [
        (PL, "iterations: Infinity, easing: 'linear'",
         "iterations: Infinity, easing: 'ease-in-out'"),
    ]),
    'the lap does not rest at its start': ('playing.py', [
        (PL, 'const REST = 2000;', 'const REST = 0;'),
    ]),
    'the loop has no copy': ('playing.py', [
        (PL, '  if (!copy) {\n    copy = document', '  if (false) {\n    copy = document'),
        (PL, "for (const el of [inner, title.querySelector('.np-copy')]) {",
         'for (const el of [inner]) {'),
    ]),
    'the copy does not move with the text': ('playing.py', [
        (PL, "for (const el of [inner, title.querySelector('.np-copy')]) {",
         'for (const el of [inner]) {'),
    ]),
    'the copy is not aria-hidden': ('playing.py', [
        (PL, "    copy.setAttribute('aria-hidden', 'true');\n", ''),
    ]),
    'the copy is a different gap from the lap (the wrap jumps)': ('playing.py', [
        (PL, 'const dist = textWidth + GAP;', 'const dist = textWidth + GAP + 20;'),
    ]),
    'the copy stays when the title fits': ('playing.py', [
        (PL, '    copy?.remove();\n', ''),
    ]),
    'the loop goes on while the card is open': ('playing.py', [
        (PL, 'if (over === 0 || document.hidden) {',
         'if (over === 0) return;\n  if (document.hidden) {'),
        (PL, '    copy?.remove();\n'
         '    title.getAnimations({ subtree: true }).forEach((a) => a.cancel());\n',
         '    copy?.remove();\n'),
    ]),
    'the loop goes on while the page is hidden': ('playing.py', [
        (PL, 'if (over === 0 || document.hidden) {', 'if (over === 0) {'),
    ]),
    'the loop stops under reduced motion': ('playing.py', [
        (PL, 'if (over === 0 || document.hidden) {',
         'if (over === 0 || document.hidden\n      || ' + RM + ') {'),
    ]),
    'a page that shows again does not start the loop': ('playing.py', [
        (PL, "document.addEventListener('visibilitychange', () => syncLoop(), mine());",
         "document.addEventListener('visibilitychange',\n"
         '    () => { if (document.hidden) syncLoop(); }, mine());'),
    ]),
    'the page hiding does not stop the loop': ('playing.py', [
        (PL, "  document.addEventListener('visibilitychange', () => syncLoop(), mine());\n", ''),
    ]),
    'a same-track swap restarts the loop at its rest': ('playing.py', [
        (PL, 'syncLoop(same ? carried.time : 0);', 'syncLoop(0);'),
    ]),
    'a new track takes the old track\'s loop on': ('playing.py', [
        (PL, 'syncLoop(same ? carried.time : 0);', 'syncLoop(carried ? carried.time : 0);'),
    ]),
    'a sync with nothing to change restarts the loop': ('playing.py', [
        (PL, '  if (going && from === 0'
         ' && Math.abs(going.effect.getTiming().duration - plan.total) < 1) return;\n', ''),
    ]),
    'folding the line does not start the loop': ('playing.py', [
        (PL, '    show(root);\n    syncLoop();\n  }, mine());', '    show(root);\n  }, mine());'),
    ]),
    'a resize does not fit the title again': ('playing.py', [
        (PL, '        fitPath();\n        syncLoop();\n      });', '        fitPath();\n      });'),
    ]),
    'no re-fit after a font loads': ('playing.py', [
        (PL, "  document.fonts?.addEventListener?.('loadingdone', () => syncLoop(), mine());\n", ''),
    ]),
    'a cut title has no fade': ('playing.py', [
        (CSS, '''    -webkit-mask-image: linear-gradient(90deg, #000 calc(100% - 14px), transparent);
    mask-image: linear-gradient(90deg, #000 calc(100% - 14px), transparent);''',
         '''    -webkit-mask-image: none; mask-image: none;'''),
    ]),
    'a cut title ends in an ellipsis': ('playing.py', [
        (CSS, '''  .p-playing:not([data-expanded]) .np-title {
    text-overflow: clip;''', '''  .p-playing:not([data-expanded]) .np-title {
    text-overflow: ellipsis;'''),
    ]),
    'the fade waits for the script to mark the title cut': ('playing.py', [
        (CSS, '  .p-playing:not([data-expanded]) .np-title {\n    text-overflow: clip;',
         '  .p-playing:not([data-expanded]) .np-title.cut {\n    text-overflow: clip;'),
    ]),
    'the fold arrow stays on the right of the head': ('playing.py', [
        (CSS, 'position: absolute; right: 8px; bottom: 0; width: 22px; height: 14px;',
         'position: static; width: 22px; height: 14px;'),
    ]),
    'the bar runs under the arrow': ('playing.py', [
        (CSS, 'position: absolute; left: 14px; right: 34px; bottom: 7px; display: block;',
         'position: absolute; left: 14px; right: 14px; bottom: 7px; display: block;'),
    ]),
    'the folded line has no next': ('playing.py', [
        (CSS, 'display: grid; width: 32px; height: var(--tap); border-color: transparent;',
         'display: none; width: 32px; height: var(--tap); border-color: transparent;'),
    ]),
    'the folded next draws a ring': ('playing.py', [
        (CSS, 'height: var(--tap); border-color: transparent; border-radius: 0;',
         'height: var(--tap); border-radius: 0;'),
    ]),
    'the folded next is a round button': ('playing.py', [
        (CSS, 'height: var(--tap); border-color: transparent; border-radius: 0;',
         'height: var(--tap);'),
    ]),
}


def run(name):
    script, edits = MUTATIONS[name]
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    for f, old, new in edits:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    r = subprocess.run([sys.executable, f'{HERE}/{script}', 'chromium'], env=env,
                       capture_output=True, text=True, timeout=900)
    shutil.rmtree(web, ignore_errors=True)
    fails = [x for x in r.stdout.splitlines() if x.startswith('FAIL')]
    if not fails and r.returncode:
        # no FAIL line: an infrastructure crash is an error, never a kill
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
