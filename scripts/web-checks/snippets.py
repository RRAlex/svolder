#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Snippet check for the mutation runners (mutate_*.py): every mutation's `old` text must occur
exactly once in the file it names under bins/svolderd/web/. A runner whose snippet has moved or
changed fails with a traceback or a NO-MATCH line only when someone runs it by hand (they are
slow), so a stale entry can rot unseen; this finds it in a second, with no browser and no build.

The runners run their suites when they are imported, so each is loaded the way its own top level
reads: only its imports, assignments and function definitions run (the constants, the `+`
expressions and the tables are then the runner's own), never the loop at its end. Its `common`
import is a stub, so nothing starts a browser or touches the repo. Both table shapes are read as
the runner's apply code reads them: `{name: [(file, old, new), ...]}` (also
`{name: (section, [(file, old, new), ...])}`) and effects' `[(name, file, old, new, cmd), ...]`.

Prints `snippets: N entries, M not matching`, then one line per failure (runner, mutation name,
file, match count); exits 1 when M is not 0.
A runner whose table holds no entry fails too, so an emptied table cannot pass.
Usage: python3 scripts/web-checks/snippets.py"""
import ast
import glob
import os
import sys
import types

HERE = os.path.dirname(os.path.abspath(__file__))
WEB = os.path.join(os.path.dirname(os.path.dirname(HERE)), 'bins', 'svolderd', 'web')
KEEP = (ast.Import, ast.ImportFrom, ast.Assign, ast.AnnAssign, ast.FunctionDef)


def load(path):
    """The runner's namespace after its imports, assignments and defs, and nothing else."""
    tree = ast.parse(open(path).read(), path)
    tree.body = [node for node in tree.body if isinstance(node, KEEP)]
    saved = sys.modules.get('common')
    sys.modules['common'] = types.ModuleType('common')
    space = {'__file__': path, '__name__': 'snippets_loaded'}
    try:
        exec(compile(tree, path, 'exec'), space)
    finally:
        if saved is None:
            del sys.modules['common']
        else:
            sys.modules['common'] = saved
    return space


def entries(space):
    """`(mutation name, file, old)` for every entry of the runner's table."""
    if 'MUTATIONS' in space:
        for name, value in space['MUTATIONS'].items():
            edits = value if isinstance(value, list) else value[1]
            for file, old, _new in edits:
                yield name, file, old
    elif 'M' in space and isinstance(space['M'], list):
        for name, file, old, _new, _cmd in space['M']:
            yield name, file, old
    else:
        raise SystemExit('no MUTATIONS dict or M list')


def main():
    total, failures = 0, []
    for path in sorted(glob.glob(os.path.join(HERE, 'mutate_*.py'))):
        runner = os.path.basename(path)
        found = list(entries(load(path)))
        if not found:
            failures.append((runner, '(table)', '-', 0))
        for name, file, old in found:
            total += 1
            target = os.path.join(WEB, file)
            count = open(target).read().count(old) if os.path.isfile(target) else 0
            if count != 1:
                failures.append((runner, name, file, count))
    print(f'snippets: {total} entries, {len(failures)} not matching')
    for runner, name, file, count in failures:
        print(f'  {runner}: {name!r}: {file}: {count} matches')
    return 1 if failures else 0


if __name__ == '__main__':
    sys.path.insert(0, HERE)
    sys.exit(main())
