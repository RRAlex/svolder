#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Which web checks does a change touch?  (make web-touched)

  touched.py [BASE]            the checks affected by what changed from BASE to HEAD, the working
                               tree and the untracked files (BASE defaults to $WEB_CHECKS_BASE, then
                               main; a lane cut from a release branch passes that branch)
  touched.py A B               the checks affected by the diff from A to B alone
  touched.py --names ...       the labels only, one per line
  touched.py --refresh         add this tree's load records to load-map.json (a record never
                               shrinks an entry, so a partial run is safe)
  touched.py --refresh --replace  swap the entries of the checks that have a record: for a full
                               gate that passed only (run-all.sh does it at its end)
  touched.py --self-test       a fixture tree, no browser

The rule: it never under-selects. When in doubt it lists more, and says why.

The map. Every browser check records the files its browsers asked the daemon for, on every run, in
target/web-checks/deps/<script>-<engine>@<pid>.txt (common.py; `/assets/<path>` is the web dir's
`<path>`; the files of one label are unioned, so the sweeps' child processes add up). The committed
file scripts/web-checks/load-map.json keeps them for fresh worktrees; a full gate run refreshes it:

  {"licence_1": ..., "licence_2": ...,
   "web_files": ["core/a.js", ...],              every file of the web dir when it was written
   "checks": {"nightdrive_theme-chromium": ["core/a.js", ...], ...}}   web-dir-relative paths

A check's files are the map's entry, this tree's records, its own script and the helpers it imports
(browser checks); a node harness's files are its script and every file its string literals lead to
(relative to the script, under the web dir or under the repository root), followed through js files, any extension: .js,
.css, .svg, .json, .rs. A harness that names the web dir itself, or the repository root, as a base
for computed paths depends on every web file.

Per changed file:
 - every check whose files include it is listed;
 - a web file also lists every browser check with no record (neither in the map nor in a record
   of this tree: a fresh worktree, a new check) and, when no browser check names the file (a new
   file, a stale map), every browser check;
 - common.py lists every browser check;
 - a gate file (run-all.sh, fingerprint.py, gate_cache.sh, lockwait.sh, gate_wait.sh) says the full
   gate is needed and lists every check;
 - another file under scripts/web-checks that no check reads (and that is not a note, this tool or
   its map) prints a WARNING and lists every check;
 - a file elsewhere (a Rust-only diff) lists only the harnesses that name it, usually none.
Both engines are listed.
"""
import importlib
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
WEB = 'bins/svolderd/web/'
CHECKS = 'scripts/web-checks/'
MAP = CHECKS + 'load-map.json'
LICENCE = {'licence_1': 'SPDX-FileCopyrightText: 2026 the svolder authors',
           'licence_2': 'SPDX-License-Identifier: GPL-3.0-or-later'}
# harness files that every browser check runs through
HARNESS = (CHECKS + 'common.py',)
# files of the gate itself: no mapping vouches for them, the full gate does
GATE = tuple(CHECKS + n for n in ('run-all.sh', 'fingerprint.py', 'gate_cache.sh', 'lockwait.sh',
                                  'gate_wait.sh'))
# files here that no check reads and no check can be affected by
QUIET = re.compile(r'(^|/)(touched\.py|load-map\.json|[^/]*\.md|mutate_[^/]*|mutants\.sh|'
                   r'mutpool\.py|snippets\.py|node-list\.py)$')
# a string literal naming a file; a literal ending in the web dir or the root is a base
PATH = re.compile(r'''['"`]([\w./${}-]*\.(?:m?js|json|css|svg|rs|html|png|woff2?))['"`]''')
WORD = re.compile(r"['\"]([\w-]{2,40})['\"]")
BASE_WEB = re.compile(r'bins/svolderd/web/?(?:\$\{|[\'"`])')
BASE_ROOT = re.compile(r"new URL\(['\"`](?:\.\./)+['\"`]")


def git(root, *args):
    r = subprocess.run(['git', '-C', root, '-c', 'commit.gpgsign=false', *args],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f'touched: git {" ".join(args)} failed: {r.stderr.strip()}')
    return r.stdout.splitlines()


def changed_files(root, base, head=None):
    if head is not None:
        return sorted(set(git(root, 'diff', '--name-only', base, head)))
    return sorted(set(git(root, 'diff', '--name-only', f'{base}...HEAD')
                      + git(root, 'diff', '--name-only', 'HEAD')
                      + git(root, 'ls-files', '--others', '--exclude-standard')))


def gate_entries(root):
    """(label, script, kind) of every check run-all.sh lists, kind `node`, `gate` (a shell proof)
    or `browser`; each browser check of the gates in both engines."""
    text = open(os.path.join(root, CHECKS, 'run-all.sh'), encoding='utf-8').read()
    found = []
    m = re.search(r'for name in(.*?); do\s+node_entries', text, re.S)
    for name in (m.group(1).replace('\\', ' ').split() if m else []):
        found.append((f'node-{name}', f'node/{name}.mjs', 'node'))
    names = []
    for arr in re.findall(r'browser_checks=\((.*?)\)', text, re.S):
        names += arr.replace('\\', ' ').split()
    for name in dict.fromkeys(names):
        for engine in ('chromium', 'firefox'):
            found.append((f'{name}-{engine}', f'{name}.py', 'browser'))
    listed = ' '.join(line for line in text.splitlines() if 'entries+=(' in line)
    for label, script in re.findall(r'"([\w-]+)\|([\w./]+)\|\w*"', listed):
        kind = 'browser' if script.endswith('.py') else 'node' if script.endswith('.mjs') else 'gate'
        if (label, script, kind) not in found:
            found.append((label, script, kind))
    return list(dict.fromkeys(found))


def js_closure(root, start):
    """(files, all_web): the files a node check leads to through its string literals, followed
    through js files; all_web when it names the web dir or the root as a base."""
    seen, todo, all_web = set(), [os.path.join(root, CHECKS, start)], False
    web = os.path.join(root, WEB)
    while todo:
        f = todo.pop()
        if f in seen or not os.path.isfile(f):
            continue
        seen.add(f)
        if not f.endswith(('.js', '.mjs')):
            continue
        try:
            text = open(f, encoding='utf-8').read()
        except (OSError, UnicodeDecodeError):
            continue
        if BASE_WEB.search(text) or BASE_ROOT.search(text):
            all_web = True
        words = set(WORD.findall(text))
        for lit in PATH.findall(text):
            # a templated name (`./${name}.mjs`) leads to every file a plain word of this file
            # fills it with (run('core-modal') names core-modal.mjs)
            lits = [lit] if '$' not in lit else \
                [re.sub(r'\$\{[^}]*\}', w, lit) for w in words if '$' not in w]
            for one in lits:
                if '$' in one:
                    continue
                for base in (os.path.dirname(f), web, root):
                    todo.append(os.path.normpath(os.path.join(base, one)))
    return {os.path.relpath(f, root) for f in seen}, all_web


def py_closure(root, script):
    os.environ['WEB_CHECKS_ROOT'] = root
    if HERE not in sys.path:
        sys.path.insert(0, HERE)
    fp = importlib.import_module('fingerprint')
    return {os.path.relpath(f, root) for f in fp.closure(script)}


def read_map(root):
    """(checks, web_files) of the committed map: label -> repo paths; ({}, set()) when absent."""
    try:
        with open(os.path.join(root, MAP), encoding='utf-8') as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {}, set()
    return ({k: {WEB + p for p in v} for k, v in data.get('checks', {}).items()},
            {WEB + p for p in data.get('web_files', [])})


def read_records(root):
    """label -> repo paths of the web files this tree's runs loaded (files of a label unioned)."""
    out = {}
    d = os.path.join(root, 'target', 'web-checks', 'deps')
    if not os.path.isdir(d):
        return out
    for name in sorted(os.listdir(d)):
        if name.endswith('.txt'):
            label = name[:-4].split('@')[0]
            with open(os.path.join(d, name), encoding='utf-8') as f:
                paths = {line.strip() for line in f}
            out.setdefault(label, set()).update(
                WEB + p[len('/assets/'):] for p in paths if p.startswith('/assets/'))
    return out


def known_loads(root):
    """label -> repo paths, the map and this tree's records together."""
    loads, _ = read_map(root)
    loads = {k: set(v) for k, v in loads.items()}
    for label, paths in read_records(root).items():
        loads.setdefault(label, set()).update(paths)
    return loads


def web_files(root):
    out = set()
    for dirpath, _, names in os.walk(os.path.join(root, WEB)):
        out.update(os.path.relpath(os.path.join(dirpath, n), root) for n in names)
    return out


def refresh(root, replace=False):
    """Rewrite load-map.json. By default each record joins the map's entry of its check (a partial
    run's record is a subset and must not shrink it); replace=True (the full gate that passed)
    swaps the entries of the checks that have a record."""
    checks = {k: {p[len(WEB):] for p in v} for k, v in read_map(root)[0].items()}
    for label, paths in read_records(root).items():
        new = {p[len(WEB):] for p in paths}
        checks[label] = new if replace else checks.get(label, set()) | new
    data = dict(LICENCE)
    data['web_files'] = sorted(p[len(WEB):] for p in web_files(root))
    data['checks'] = {k: sorted(v) for k, v in sorted(checks.items())}
    with open(os.path.join(root, MAP), 'w', encoding='utf-8') as f:
        json.dump(data, f, indent=1)
        f.write('\n')
    print(f'touched: {MAP} written: {len(data["checks"])} checks, {len(data["web_files"])} web files')


def area(path):
    """The part of the web dir a file belongs to: core, fonts, themes/<T>; a file at the top is its own."""
    rest = path[len(WEB):].split('/')
    if len(rest) == 1:
        return rest[0]
    return '/'.join(rest[:2]) if rest[0] == 'themes' else rest[0]


def web_scope(root, label):
    """The repo paths of the web files whose change makes a browser check's last pass void: every
    file of the areas (core, fonts, themes/<T>) the check loads, and every web file no check
    loads (a new one). None when the check has no entry in the map or the records (then the whole
    web dir counts). The same rule `touched` lists checks by."""
    loads = known_loads(root)
    mine = loads.get(label)
    if not mine:
        return None
    loaded = set().union(*loads.values())
    areas = {area(p) for p in mine}
    return sorted(f for f in web_files(root) if area(f) in areas or f not in loaded)


def touched(root, changed):
    """(hits, warnings, unrecorded): hits is {label: why}, warnings are lines to print first,
    unrecorded the browser labels with no load record."""
    entries = gate_entries(root)
    loads = known_loads(root)
    _, mapped = read_map(root)
    files, all_web = {}, set()
    for label, script, kind in entries:
        if kind == 'node':
            files[label], wide = js_closure(root, script)
            if wide:
                all_web.add(label)
        elif kind == 'browser':
            files[label] = py_closure(root, script) | loads.get(label, set())
        else:
            files[label] = {CHECKS + script}
    browser = [label for label, _, kind in entries if kind == 'browser']
    unrecorded = [label for label in browser if label not in loads]
    hit, warnings = {}, []
    full = False

    def add(label, why):
        hit.setdefault(label, why)

    for f in changed:
        if f in GATE:
            full = True
            warnings.append(f'{f} is a file of the gate itself: run the full gate (make web-checks)')
            continue
        if f in HARNESS:
            for label in browser:
                add(label, f)
        direct = [label for label, fs in files.items() if f in fs]
        for label in direct:
            add(label, f)
        if f.startswith(WEB):
            for label in all_web:
                add(label, f)
            for label in unrecorded:
                add(label, f'{f} (no load record)')
            # a check counts by area too: a line missing from its entry cannot drop it
            for label in browser:
                if any(area(p) == area(f) for p in loads.get(label, ())):
                    add(label, f'{f} (the check loads from {area(f)})')
            if not any(f in loads.get(label, set()) for label in browser):
                why = 'no browser check has loaded it' if os.path.isfile(os.path.join(root, f)) \
                    else 'a deleted file nothing records'
                if f in mapped:
                    why = 'the map has it but no check'
                warnings.append(f'{f}: {why}: every browser check is listed')
                for label in browser:
                    add(label, f'{f} (unmapped)')
        elif f.startswith(CHECKS) and not direct and f not in HARNESS and not QUIET.search(f):
            warnings.append(f'{f}: no check reads it and it is in no map: every check is listed')
            for label in files:
                add(label, f'{f} (unmapped)')
    if full:
        for label in files:
            add(label, 'the gate itself changed')
    return dict(sorted(hit.items())), warnings, unrecorded


def report(root, changed, base_text, names_only):
    hit, warnings, unrecorded = touched(root, changed)
    if names_only:
        if hit:
            print('\n'.join(hit))
        return
    for w in warnings:
        print(f'WARNING: {w}')
    if not changed or not any(f.startswith((WEB, CHECKS)) for f in changed):
        print(f'touched: no file under the web dir or scripts/web-checks changed {base_text}; '
              f'{len(hit)} checks name a changed file elsewhere')
    if not hit:
        return
    print(f'touched: {len(hit)} checks are affected by the change {base_text}:')
    for label, why in hit.items():
        print(f'  {label:<34} {why}')
    if unrecorded:
        print(f'note: {len(unrecorded)} browser checks have no load record: they are listed for every '
              f'web file. A full gate run refreshes {MAP}.')
    print('run them:')
    if any(w.startswith(tuple(GATE)) for w in warnings):
        print('  make web-checks')
    else:
        print(f'  WEB_CHECKS_ONLY="{" ".join(hit)}" bash scripts/web-checks/run-all.sh')


# ---------------------------------------------------------------- self-test

def _write(root, path, text='x\n'):
    full = os.path.join(root, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, 'w') as f:
        f.write(text)


def self_test():
    tmp = tempfile.mkdtemp(prefix='touched-')
    try:
        _self_test(tmp)
    finally:
        subprocess.run(['rm', '-rf', tmp])
    print('touched self-test: ok')


def _self_test(root):
    _write(root, CHECKS + 'run-all.sh', '''
# a check is "label|script|argument"
for name in alpha beta gamma delta; do
    node_entries+=("node-$name|node/$name.mjs|")
done
node_entries+=("gate-x|gate_x.sh|")
if [ "$quick" -eq 1 ]; then
    browser_checks=(one paper)
else
    browser_checks=(one paper \\
        computer_one sweepy)
fi
entries+=("solo-chromium|solo.py|" "pre|pre.py|")
''')
    _write(root, CHECKS + 'node/alpha.mjs', "import('../../../bins/svolderd/web/core/a.js'); import './dom.mjs';\n")
    _write(root, CHECKS + 'node/beta.mjs', "import './dom.mjs';\n")
    _write(root, CHECKS + 'node/gamma.mjs',
           "const css = new URL('core/frame.css', web); const r = '../../../crates/x/src/eq/c.rs';\n"
           "const f = 'bins/svolderd/web/themes/paper/style.css';\n")
    _write(root, CHECKS + 'node/viz.mjs', "const web = (n) => new URL(`../../../bins/svolderd/web/${n}`, base);\n")
    _write(root, CHECKS + 'node/dom.mjs')
    _write(root, CHECKS + 'node/delta.mjs',
           "const run = (n) => spawn(new URL(`./${n}.mjs`, import.meta.url)); run('beta'); run('dom');\n")
    _write(root, CHECKS + 'gate_x.sh')
    for n in ('one', 'paper', 'computer_one', 'solo', 'pre', 'sweepy'):
        _write(root, CHECKS + n + '.py', 'import common\n')
    _write(root, CHECKS + 'common.py')
    _write(root, CHECKS + 'fingerprint.py', open(os.path.join(HERE, 'fingerprint.py')).read())
    for f in ('core/a.js', 'core/b.js', 'core/new.js', 'core/frame.css', 'themes/paper/p.js',
              'themes/paper/style.css', 'themes/nightdrive/n.js'):
        _write(root, WEB + f)
    _write(root, 'src/lib.rs')
    _write(root, 'crates/x/src/eq/c.rs')
    git(root, 'init', '-q', '-b', 'main')
    git(root, 'config', 'user.email', 't@t')
    git(root, 'config', 'user.name', 't')

    def commit():
        git(root, 'add', '-A', '-f')
        git(root, 'commit', '-q', '-m', 'x')

    def after(*paths):
        for p in paths:
            _write(root, p, 'changed\n')
        git(root, 'add', '-A', '-f')
        git(root, 'commit', '-q', '-m', 'two')
        got = touched(root, changed_files(root, 'HEAD~1', 'HEAD'))
        git(root, 'reset', '-q', '--hard', 'HEAD~1')
        return sorted(got[0]), got[1]

    def labels(*names):
        return sorted(f'{n}-{e}' for n in names for e in ('chromium', 'firefox'))

    everything = sorted(l for l, _, _ in gate_entries(root))
    assert 'label' not in everything, 'a comment is no entry'
    commit()
    # a fresh worktree: no map, no record, so every browser check lacks one
    every_browser = labels('one', 'paper', 'computer_one', 'sweepy') + ['pre', 'solo-chromium']
    every_browser.sort()
    got, warn = after(WEB + 'core/b.js')
    assert [g for g in got if not g.startswith(('node', 'gate'))] == every_browser, got
    assert {'node-alpha'} - set(got) == {'node-alpha'}, 'a node closure is not touched by b.js'
    assert 'node-gamma' not in got
    assert after('src/lib.rs') == ([], []), 'a Rust-only diff lists nothing'
    assert after(CHECKS + 'node/dom.mjs')[0] == ['node-alpha', 'node-beta', 'node-delta'], 'a helper lists its harnesses'
    got = after(CHECKS + 'node/beta.mjs')[0]
    assert got == ['node-beta', 'node-delta'], f'a harness another one runs by a templated name: {got}'
    got, _ = after(WEB + 'core/frame.css')
    assert 'node-gamma' in got and 'paper-chromium' in got and 'one-firefox' in got, got
    got, _ = after('crates/x/src/eq/c.rs')
    assert got == ['node-gamma'], got
    got, _ = after(WEB + 'themes/paper/style.css')
    assert 'node-gamma' in got, got
    # a map and a record: only the unrecorded checks and the checks that have the file
    _write(root, MAP, json.dumps({**LICENCE, 'web_files': ['core/a.js', 'core/b.js', 'core/new.js'],
                                  'checks': {'one-chromium': ['core/b.js'], 'one-firefox': ['core/b.js'],
                                             'paper-chromium': ['core/a.js'], 'paper-firefox': ['core/a.js'],
                                             'computer_one-chromium': [], 'computer_one-firefox': [],
                                             'sweepy-chromium': [], 'sweepy-firefox': [],
                                             'solo-chromium': [], 'pre': []}}))
    commit()
    got, warn = after(WEB + 'core/b.js')
    assert got == ['one-chromium', 'one-firefox', 'paper-chromium', 'paper-firefox'], got  # by area
    got, warn = after(WEB + 'core/a.js')
    assert got == ['node-alpha', 'one-chromium', 'one-firefox', 'paper-chromium', 'paper-firefox'], got
    got, warn = after(WEB + 'core/new.js')
    assert got == every_browser and any('every browser check' in w for w in warn), (got, warn)
    # a record of this tree joins the map: sweep children of one label add up
    _write(root, 'target/web-checks/deps/sweepy-chromium@1.txt', '/assets/themes/paper/p.js\n')
    _write(root, 'target/web-checks/deps/sweepy-chromium@2.txt', '/assets/themes/nightdrive/n.js\n')
    got, _ = after(WEB + 'themes/paper/p.js')
    assert 'sweepy-chromium' in got and 'one-chromium' not in got, got
    got, _ = after(WEB + 'themes/nightdrive/n.js')
    assert 'sweepy-chromium' in got, got
    # a stale entry: knob.js left out of one check's entry, another check names it
    _write(root, MAP, json.dumps({**LICENCE, 'web_files': ['core/a.js', 'core/b.js', 'core/new.js'],
                                  'checks': {'one-chromium': ['core/a.js'], 'one-firefox': ['core/b.js'],
                                             'paper-chromium': ['themes/paper/p.js'], 'paper-firefox': ['core/b.js'],
                                             'computer_one-chromium': [], 'computer_one-firefox': [],
                                             'sweepy-chromium': [], 'sweepy-firefox': [],
                                             'solo-chromium': [], 'pre': []}}))
    commit()
    got, _ = after(WEB + 'core/b.js')
    assert 'one-chromium' in got and 'one-firefox' in got and 'paper-firefox' in got, got
    assert 'paper-chromium' not in got, got
    _write(root, MAP, json.dumps({**LICENCE, 'web_files': ['core/a.js', 'core/b.js', 'core/new.js'],
                                  'checks': {'one-chromium': ['core/b.js'], 'one-firefox': ['core/b.js'],
                                             'paper-chromium': ['core/a.js'], 'paper-firefox': ['core/a.js'],
                                             'computer_one-chromium': [], 'computer_one-firefox': [],
                                             'sweepy-chromium': [], 'sweepy-firefox': [],
                                             'solo-chromium': [], 'pre': []}}))
    commit()
    # a check missing from the map is listed for every web file
    _write(root, CHECKS + 'run-all.sh', open(os.path.join(root, CHECKS + 'run-all.sh')).read().replace(
        'sweepy)', 'sweepy fresh)'))
    _write(root, CHECKS + 'fresh.py', 'import common\n')
    commit()
    got, _ = after(WEB + 'core/b.js')
    assert 'fresh-chromium' in got and 'fresh-firefox' in got, got
    # gate and unmapped files are loud
    everything = sorted(l for l, _, _ in gate_entries(root))
    got, warn = after(CHECKS + 'lockwait.sh')
    assert got == everything and any('full gate' in w for w in warn), (got, warn)
    got, warn = after(CHECKS + 'mystery.txt')
    assert got == everything and any('mystery.txt' in w for w in warn), (got, warn)
    assert after(CHECKS + 'README.md') == ([], []), 'a note is not unmapped'
    got, _ = after(CHECKS + 'common.py')
    assert not any(g.startswith('node') for g in got) and 'solo-chromium' in got and 'pre' in got, got
    # refresh writes what the records say and keeps the rest
    _write(root, 'target/web-checks/deps/sweepy-chromium@1.txt', '/assets/themes/paper/p.js\n')
    _write(root, 'target/web-checks/deps/sweepy-chromium@2.txt', '/assets/themes/nightdrive/n.js\n')
    refresh(root)
    data = json.load(open(os.path.join(root, MAP)))
    assert sorted(data['checks']['sweepy-chromium']) == ['themes/nightdrive/n.js', 'themes/paper/p.js'], data
    assert data['checks']['one-chromium'] == ['core/b.js'] and 'core/new.js' in data['web_files']
    assert list(data)[:2] == ['licence_1', 'licence_2']
    # a partial run's record joins an entry, a full gate's replaces it
    _write(root, 'target/web-checks/deps/one-chromium@1.txt', '/assets/core/a.js\n')
    refresh(root)
    assert sorted(json.load(open(os.path.join(root, MAP)))['checks']['one-chromium']) == \
        ['core/a.js', 'core/b.js']
    refresh(root, replace=True)
    assert json.load(open(os.path.join(root, MAP)))['checks']['one-chromium'] == ['core/a.js']
    # the scope a cached pass depends on: the entry's areas plus the files no check loads
    scope = web_scope(root, 'paper-chromium')
    assert WEB + 'core/b.js' in scope and WEB + 'themes/nightdrive/n.js' not in scope \
        and WEB + 'themes/paper/style.css' in scope, scope  # style.css: no check loads it
    assert web_scope(root, 'nosuch-chromium') is None


def main(argv):
    names_only = '--names' in argv
    argv = [a for a in argv if a != '--names']
    root = os.environ.get('WEB_CHECKS_ROOT') or os.path.dirname(os.path.dirname(HERE))
    if argv[:1] == ['--self-test']:
        return self_test()
    if argv[:1] == ['--refresh']:
        return refresh(root, replace='--replace' in argv[1:])
    if len(argv) == 2:
        changed = changed_files(root, argv[0], argv[1])
        text = f'from {argv[0]} to {argv[1]}'
    else:
        base = argv[0] if argv else os.environ.get('WEB_CHECKS_BASE', 'main')
        changed = changed_files(root, base)
        text = f'since {base}'
    report(root, changed, text, names_only)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
