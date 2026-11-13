#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The web gate's node list against the node harnesses on disk: every `node/*.mjs` harness is named
in run-all.sh's `for name in ...` list, and every name of that list is a file. A harness that is
renamed or added without the list fails the gate on a missing file, or never runs, and nothing
says so until the gate is run (over half an hour); this finds it in a second.

`HELPERS` are the `node/*.mjs` files that are no harness: other harnesses import them and they
run no test of their own, so the gate does not list them. Each carries its reason.

Prints `node-list: N harnesses listed, M problems`, then one line per problem; exits 1 when M is
not 0.
Usage: python3 scripts/web-checks/node-list.py [--self-test]"""
import glob
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HELPERS = {
    'dom': 'the jsdom page the harnesses that need a DOM share',
    'html-same': 'the HTML comparison ember-parts.mjs holds the drawing to',
    'look-values': "the approved look's numbers, read by the smoke's harnesses",
    'nightdrive-rig': "the core's pieces over a fixture page, for Night drive's harnesses",
    'paper-rig': "the fixture page and the core's computer, for Paper's harnesses",
}
# The list is the words between `for name in` and `; do` of the loop that fills `node_entries`.
LIST = re.compile(r'^node_entries=\(\)\nfor name in (.*?); do\n\s*node_entries\+=', re.S | re.M)


def listed(text):
    """The names of run-all.sh's node list, in order, or None when the loop is not found."""
    found = LIST.search(text)
    if not found:
        return None
    return found.group(1).replace('\\\n', ' ').split()


def problems(names, on_disk):
    """One line per harness on disk that is not listed, listed name with no file, name listed
    twice and helper that is listed or gone."""
    out = []
    if names is None:
        return ["run-all.sh: the `for name in ...; do` loop that fills node_entries was not found"]
    for name in sorted(set(names)):
        if names.count(name) > 1:
            out.append(f'{name}: listed {names.count(name)} times in run-all.sh')
        if name in HELPERS:
            out.append(f'{name}: listed in run-all.sh, but it is a helper ({HELPERS[name]})')
        elif name not in on_disk:
            out.append(f'{name}: listed in run-all.sh, but node/{name}.mjs does not exist')
    for name in sorted(on_disk):
        if name not in names and name not in HELPERS:
            out.append(f'{name}: node/{name}.mjs is not in run-all.sh, so the gate never runs it '
                       '(a helper is named in HELPERS of node-list.py, with its reason)')
    for name in sorted(HELPERS):
        if name not in on_disk:
            out.append(f'{name}: named a helper in node-list.py, but node/{name}.mjs does not exist')
    return out


def self_test():
    loop = 'node_entries=()\nfor name in a b \\\n    c; do\n    node_entries+=("x")\ndone\n'
    disk = {'a', 'b', 'c'} | set(HELPERS)
    cases = [
        ('the list is read across a continued line', listed(loop) == ['a', 'b', 'c']),
        ('a list that matches the files passes', problems(['a', 'b', 'c'], disk) == []),
        ('a listed name with no file fails', len(problems(['a', 'b', 'c', 'gone'], disk)) == 1),
        ('a harness that is not listed fails', len(problems(['a', 'b'], disk)) == 1),
        ('a name listed twice fails', len(problems(['a', 'b', 'b', 'c'], disk)) == 1),
        ('a listed helper fails', len(problems(['a', 'b', 'c', 'dom'], disk)) == 1),
        ('a helper that is gone fails', len(problems(['a', 'b', 'c'], disk - {'dom'})) == 1),
        ('a run-all.sh without the loop fails', len(problems(listed('nothing'), disk)) == 1),
    ]
    bad = [name for name, ok in cases if not ok]
    for name in bad:
        print(f'node-list self-test FAILED: {name}')
    print(f'node-list self-test: {len(cases)} cases, {len(bad)} failed')
    return 1 if bad else 0


def main():
    if '--self-test' in sys.argv[1:]:
        return self_test()
    names = listed(open(os.path.join(HERE, 'run-all.sh')).read())
    on_disk = {os.path.basename(p)[:-4] for p in glob.glob(os.path.join(HERE, 'node', '*.mjs'))}
    found = problems(names, on_disk)
    print(f'node-list: {len(names or [])} harnesses listed, {len(found)} problems')
    for line in found:
        print(f'  {line}')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
