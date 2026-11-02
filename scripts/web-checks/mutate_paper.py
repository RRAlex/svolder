# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Paper's page: each mutation edits a copy of the page's web dir
(never the repo), runs paper.py (Chromium) against it and must make a check FAIL.
Usage: python3 mutate_paper.py [name ...]   (default: all). One worker, by hand, slow."""
import os
import shutil
import subprocess
import sys

import common

HERE = os.path.dirname(os.path.abspath(__file__))
P = 'themes/paper/'
MUTATIONS = {
    'place sets nothing': [
        (P + 'place.js', "const at = percent(el.dataset.at);", "const at = null;"),
        (P + 'place.js', "const left = percent(el.dataset.x);", "const left = null;"),
        (P + 'place.js', "const top = percent(el.dataset.y);", "const top = null;"),
    ],
    'Auto ignores the system': [
        (P + 'style.css', ':not([data-scheme="light"])', ''),
    ],
    'a held section is drawn anyway': [
        (P + 'live.js', "if (holding.has(name)) {", "if (false) {"),
    ],
    'act logs nothing': [
        (P + 'act.js', "entries.push(entry);", ""),
    ],
    'a meter\'s needle stands at its class-A share': [
        (P + 'draw/amp.js', 'data-at="${m.fraction.toFixed(5)}"',
         'data-at="${Number(m.class_a ?? 0).toFixed(5)}"'),
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
    r = subprocess.run([sys.executable, f'{HERE}/paper.py', 'chromium'], env=env,
                       capture_output=True, text=True, timeout=1800)
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
