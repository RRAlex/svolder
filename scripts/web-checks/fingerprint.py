#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The web gate's result cache (used by run-all.sh on the quick and task gates).

A browser check's fingerprint is a hash of everything it reads that this tree controls: the built
daemon and TUI binaries, the web directory the daemon serves, the check's file, every helper it
imports here (common.py and the rest, followed through their imports) or names by file name, the
engine's name and the browser's version, and the env that moves the daemon or the web dir. A pass
stored under the same fingerprint is reused.

  fingerprint.py base                    print the part shared by every check (binaries, web dir, browsers)
  fingerprint.py check SCRIPT ENGINE BASE  print the fingerprint of one check (exit 1 when there is none)
  fingerprint.py full SCRIPT ENGINE      the same with a fresh base (taken right before and after a run)
  fingerprint.py hit LABEL FP            exit 0 when LABEL last passed with FP
  fingerprint.py store LABEL FP          record a pass
  fingerprint.py forget LABEL            drop a record

Env: WEB_CHECKS_ROOT (the tree, default this file's), WEB_CHECKS_CACHE (the cache directory, default
target/web-checks/cache), WEB_CHECKS_BROWSER_VERSION (skips asking the browsers, for the self-test).
"""
import ast
import hashlib
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get('WEB_CHECKS_ROOT') or os.path.dirname(os.path.dirname(HERE))
CHECKS_DIR = os.path.join(ROOT, 'scripts', 'web-checks')
CACHE = os.environ.get('WEB_CHECKS_CACHE') or os.path.join(ROOT, 'target', 'web-checks', 'cache')
# files that name a runner's own, not a check's input: a mutation runner or a note never changes a pass
NAMED = re.compile(r'[\w./-]+\.(?:json|mjs|js|py|css|html|svg|sh|txt|yaml|yml)\b')


def h_file(path):
    h = hashlib.sha256()
    try:
        with open(path, 'rb') as f:
            for chunk in iter(lambda: f.read(1 << 20), b''):
                h.update(chunk)
    except OSError:
        return 'missing'
    return h.hexdigest()


def h_tree(path):
    h = hashlib.sha256()
    for dirpath, dirs, files in os.walk(path):
        dirs.sort()
        for name in sorted(files):
            full = os.path.join(dirpath, name)
            h.update(os.path.relpath(full, path).encode() + b'\0' + h_file(full).encode() + b'\n')
    return h.hexdigest()


# Env a check reads to choose or change its cases (a subset, a duration, stills kept): a run with any of
# them set is not the whole check, so its pass is never stored, and they are hashed into base() too.
SUBSET_EXACT = {'ND_ONLY', 'ND_CASES', 'WEB_CHECK_THEMES', 'HAZE_SECTIONS', 'SCROLL_SECTIONS',
                'HIDDEN_SECS', 'SECS', 'REPS', 'VIZ_SHOTS', 'ARRANGE_SHOTS', 'MUT_WORKERS'}
SUBSET_PREFIX = ('LOOK_', 'ND_', 'SVOLDER_FAKE_')


def subset_env():
    return sorted((k, v) for k, v in os.environ.items()
                  if k in SUBSET_EXACT or k.startswith(SUBSET_PREFIX))


def browser_versions():
    """The browsers' versions, or None when one cannot be read (then nothing is cached)."""
    pinned = os.environ.get('WEB_CHECKS_BROWSER_VERSION')
    if pinned is not None:
        return pinned
    out = []
    for name in ('chromium', 'firefox', 'geckodriver'):
        exe = shutil.which(name) or f'/snap/bin/{name}'
        try:
            r = subprocess.run([exe, '--version'], capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.SubprocessError):
            return None
        if r.returncode != 0 or not r.stdout.strip():
            return None
        out.append(f'{name} {r.stdout.strip()}')
    return '|'.join(out)


def base():
    web = os.environ.get('SMOKE_WEB', os.path.join(ROOT, 'bins', 'svolderd', 'web'))
    versions = browser_versions()
    if versions is None:
        return ''
    parts = [
        'svolderd ' + h_file(os.environ.get('SVOLDERD', os.path.join(ROOT, 'target/debug/svolderd'))),
        'svolder ' + h_file(os.environ.get('SVOLDER', os.path.join(ROOT, 'target/debug/svolder'))),
        'web ' + h_tree(web),
        'embedded ' + os.environ.get('SMOKE_EMBEDDED', ''),
        'browsers ' + versions,
        'env ' + repr(subset_env()),
    ]
    return hashlib.sha256('\n'.join(parts).encode()).hexdigest()


def local_imports(path):
    """The files in this directory a Python file imports."""
    found = set()
    try:
        tree = ast.parse(open(path, encoding='utf-8').read())
    except (OSError, SyntaxError):
        return found
    for node in ast.walk(tree):
        mods = []
        if isinstance(node, ast.Import):
            mods = [a.name for a in node.names]
        elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
            mods = [node.module]
        for m in mods:
            cand = os.path.join(CHECKS_DIR, m.split('.')[0] + '.py')
            if os.path.isfile(cand):
                found.add(cand)
    return found


def named_files(path):
    """The files in this directory a Python file really opens or runs: a string literal that is an
    argument of a call (open, subprocess, Path, a helper's own loader), never a comment or a docstring."""
    found = set()
    try:
        tree = ast.parse(open(path, encoding='utf-8').read())
    except (OSError, SyntaxError):
        return found
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        for arg in list(node.args) + [k.value for k in node.keywords]:
            for sub in ast.walk(arg):
                if isinstance(sub, ast.Constant) and isinstance(sub.value, str):
                    for m in NAMED.findall(sub.value):
                        cand = os.path.normpath(os.path.join(CHECKS_DIR, m.lstrip('/')))
                        if cand.startswith(CHECKS_DIR + os.sep) and os.path.isfile(cand):
                            found.add(cand)
    return found


def closure(script):
    seen, todo = set(), [os.path.join(CHECKS_DIR, script)]
    while todo:
        f = todo.pop()
        if f in seen:
            continue
        seen.add(f)
        base_name = os.path.basename(f)
        # a mutation runner is not an input of the check it mutates
        todo += [g for g in local_imports(f) | named_files(f)
                 if not os.path.basename(g).startswith('mutate_') and g not in seen
                 and os.path.basename(g) != base_name]
    return sorted(seen)


def check(script, engine, base_fp):
    if not base_fp:
        return ''
    h = hashlib.sha256()
    h.update(f'{base_fp}\n{engine}\n'.encode())
    for f in closure(script):
        h.update(os.path.relpath(f, CHECKS_DIR).encode() + b'\0' + h_file(f).encode() + b'\n')
    return h.hexdigest()


def record(label):
    return os.path.join(CACHE, re.sub(r'[^\w.-]', '_', label) + '.fp')


def main(argv):
    cmd = argv[1] if len(argv) > 1 else ''
    if cmd == 'base' and len(argv) == 2:
        print(base())
    elif cmd == 'check' and len(argv) == 5:
        fp = check(argv[2], argv[3], argv[4])
        print(fp)
        return 0 if fp else 1
    elif cmd == 'full' and len(argv) == 4:
        fp = check(argv[2], argv[3], base())
        print(fp)
        return 0 if fp else 1
    elif cmd == 'hit' and len(argv) == 4:
        try:
            return 0 if open(record(argv[2])).read().strip() == argv[3] else 1
        except OSError:
            return 1
    elif cmd == 'store' and len(argv) == 4:
        # a run with a subset env set (or no fingerprint) is not the whole check: nothing is stored
        if not argv[3] or subset_env():
            return 0
        os.makedirs(CACHE, exist_ok=True)
        tmp = record(argv[2]) + f'.{os.getpid()}.tmp'
        with open(tmp, 'w') as f:
            f.write(argv[3] + '\n')
        os.replace(tmp, record(argv[2]))
    elif cmd == 'forget' and len(argv) == 3:
        try:
            os.unlink(record(argv[2]))
        except OSError:
            pass
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
