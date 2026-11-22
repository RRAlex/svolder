#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Which web checks name what a change touches?  (make web-checks-affected)

  web-affected.py [--list] [--tokens-only] [--timing] [--checks-dir DIR] [BASE [TO]]
  web-affected.py --self-test                               a fixture tree, no browser

BASE defaults to $WEB_CHECKS_BASE, then the newest local release-X.Y.Z branch that is an ancestor
of HEAD, else main. With BASE alone the diff is BASE...HEAD plus the working tree and the
untracked files; with TO it is BASE..TO alone (the two forms of touched.py). --list prints the
labels and why, and runs nothing. --tokens-only drops touched.py's half of the union (to judge the
token rules alone) and runs nothing. --checks-dir (default scripts/web-checks) greps another
copy of the checks, for a proof against an older tree; touched.py's half is then skipped.

touched.py lists the checks that LOAD a changed file. This lists, in addition, the checks that
NAME something the diff adds or removes, which is what bounced a gate when a lane changed a shared
strip and a check elsewhere pinned the old state. Tokens come from the added AND removed lines of
the files under bins/svolderd/web/ and bins/svolderd/src/, comment lines left out:
  - element ids: id="x", id: 'x', elementId: 'x', getElementById('x'), '#x' in a selector string
    (and #x in a .css file);
  - popup names and row words of core/popups/listed.js (name: 'x', label: 'x'), and
    menu-popup-<name> for each name;
  - SSE event names: on('x', send_json(tx, "x");
  - quoted strings of 3 or more words or 12 or more characters (notes, chips, menu rows).
In Rust, only send_json event names count in bins/svolderd/src, and double-quoted strings and ids
only in bins/svolderd/src/web/ outside test files (tests.rs, tests/, a #[cfg(test)] block).
A token shorter than 3 characters, or on the stop list (true, false, null, undefined, none, auto,
div, span, button, input, block, flex, grid, hidden, visible, inherit, click, change, and the
media types application/json and text/event-stream), is dropped. So is a token found in more
than 10 check scripts: each dropped token is printed with its count, so nothing goes quietly
(grep for it by hand if it is yours). A bare word must match as a whole word.

When a line of listed.js that names or labels a popup changes, a check also pins the popup list if
its own script names two different popups of the list (menu-popup-X, 'X', the const X or the
row's label) within 160 characters: a check that pins the list names the neighbours of the new
entry, not the entry. Only the POPUPS array counts (no comments, no side-list labels).

A token found as a fixed string in a check's script, in a helper of a browser check other than
common.py, or in a file a node harness reads under scripts/web-checks (touched.py's closure), lists
that script's labels from run-all.sh: both engines. Helper words count only when 3 checks or fewer
import the helper (a helper imported by more is a list of its own). The label's why is its most
specific token (fewest scripts), with "+N more". The result is the union with touched.py's own
list for the same diff. The timing parts (`_timing`, the serial scripts of run-all.sh) are listed
but not run unless --timing: they are owed to the gate on a quiet machine. Unless --list, the
rest run in one `WEB_CHECKS_ONLY="..." bash scripts/web-checks/run-all.sh` (both engines at once,
the gate's own lock; never wrap it in flock) and its status is the exit status.
A diff that changes a file of the gate itself (run-all.sh, ...) still lists and runs what its other
files name, and says one line: the full gate is owed at merge. What it never sees: a check that
asserts a count or an order without naming the id, or a height, a band or a layout that moves
without a name changing; grep for the old list by hand.
"""
import importlib.util
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCOPES = ('bins/svolderd/web/', 'bins/svolderd/src/')
DAEMON_SRC = 'bins/svolderd/src/'
DAEMON_WEB = 'bins/svolderd/src/web/'
LISTED = 'bins/svolderd/web/core/popups/listed.js'
DEFAULT_CHECKS = 'scripts/web-checks'
MAX_SCRIPTS = 10        # a token in more scripts than this is dropped
MAX_IMPORTERS = 3       # a helper imported by more checks than this adds nothing to its importers
PIN_WINDOW = 160
STOP = frozenset('''true false null undefined none auto div span button input block flex grid hidden
visible inherit click change application/json text/event-stream'''.split())

ID_RES = [re.compile(p) for p in (
    r'''\bid\s*=\s*\\?["']([\w-]+)''',
    r'''\b(?:id|elementId)\s*:\s*['"]([\w-]+)['"]''',
    r'''getElementById\(\s*['"]([\w-]+)['"]''',
)]
STRING = re.compile(r'''(['"`])((?:\\.|(?!\1).)*)\1''')
RUST_STRING = re.compile(r'''"((?:\\.|[^"\\])*)"''')
HASH = re.compile(r'#([A-Za-z_][\w-]*)')
LISTED_RE = re.compile(r'''\b(?:name|label)\s*:\s*['"]([^'"]+)['"]''')
NAME_RE = re.compile(r'''\bname\s*:\s*['"]([\w-]+)['"]''')
EVENT_RES = [re.compile(p) for p in (
    r'''\bon\(\s*['"]([\w.-]+)['"]''',
    r'''send_json\(\s*\w+\s*,\s*"([\w.-]+)"''',
)]
COMMENT = re.compile(r'^\s*(//|/\*|\*)')


def _load_touched(root):
    sys.dont_write_bytecode = True      # a stray .pyc under scripts/web-checks lists every check
    spec = importlib.util.spec_from_file_location(
        'web_checks_touched', os.path.join(root, 'scripts/web-checks/touched.py'))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def is_test_file(path):
    base = os.path.basename(path)
    return base == 'tests.rs' or base.endswith('_tests.rs') or base.startswith('test_') \
        or '/tests/' in path


def tokens_of(path, line):
    """The tokens one added or removed line of `path` carries."""
    out = set()
    if COMMENT.match(line):
        return out
    if path.endswith('.rs'):
        for rx in EVENT_RES[1:]:
            out.update(rx.findall(line))
        if path.startswith(DAEMON_WEB) and not is_test_file(path):
            for rx in ID_RES[:1]:
                out.update(rx.findall(line))
            for body in RUST_STRING.findall(line):
                words = body.split()
                if len(body) >= 12 or len(words) >= 3:
                    out.add(body.strip())
        return {t for t in out if len(t) >= 3 and t not in STOP}
    for rx in ID_RES:
        out.update(rx.findall(line))
    for _, body in STRING.findall(line):
        for h in HASH.findall(body):
            out.add(h)
        words = body.split()
        if len(body) >= 12 or len(words) >= 3:
            out.add(body.strip())
    if path.endswith('.css'):
        out.update(h for h in HASH.findall(line) if not re.fullmatch(r'[0-9a-fA-F]{3,8}', h))
    for rx in EVENT_RES:
        out.update(rx.findall(line))
    if path == LISTED:
        out.update(LISTED_RE.findall(line))
        for n in NAME_RE.findall(line):
            out.add(f'menu-popup-{n}')
    return {t for t in out if len(t) >= 3 and t not in STOP}


def popups_of(text):
    """{name: row label} of the POPUPS array of listed.js, names resolved through the consts."""
    consts = dict(re.findall(r"""^export const (\w+)\s*=\s*['"]([^'"]+)['"]""", text, re.M))
    consts_by_value = {v: k for k, v in consts.items()}
    m = re.search(r'^export const POPUPS = \[(.*?)^\];', text, re.S | re.M)
    out = {}
    if not m:
        return out
    body = '\n'.join(ln for ln in m.group(1).splitlines() if not COMMENT.match(ln))
    for entry in re.split(r'^  \{', body, flags=re.M)[1:]:
        n = re.search(r'^    name\s*:\s*(\w+|\'[^\']+\'|"[^"]+")', entry, re.M)
        if not n:
            continue
        raw = n.group(1)
        name = consts.get(raw, raw.strip('\'"'))
        label = re.search(r'^    row\s*:\s*\{\s*label\s*:\s*[\'"]([^\'"]+)[\'"]', entry, re.M)
        out[name] = (label.group(1) if label else '', consts_by_value.get(name))
    return out


def listed_popups(root, base, to, lines):
    """{name: (row label, const)} of the popups as they were and as they are, when the diff
    changes a name or row line of the POPUPS array; else {}."""
    if not any(p == LISTED and not COMMENT.match(ln) and re.match(r'\s*(?:name|row)\s*:', ln)
               for p, ln in lines):
        return {}
    out = {}
    for rev in (base, to):
        if rev is None:
            try:
                with open(os.path.join(root, LISTED), encoding='utf-8') as f:
                    out.update(popups_of(f.read()))
            except OSError:
                pass
        else:
            r = subprocess.run(['git', '-C', root, 'show', f'{rev}:{LISTED}'],
                               capture_output=True, text=True)
            if r.returncode == 0:
                out.update(popups_of(r.stdout))
    return out


def _show(root, rev, path):
    if rev is None:
        try:
            with open(os.path.join(root, path), encoding='utf-8') as f:
                return f.read()
        except (OSError, UnicodeDecodeError):
            return ''
    r = subprocess.run(['git', '-C', root, 'show', f'{rev}:{path}'], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ''


def _cfg_test_start(text):
    """The line of the first `#[cfg(test)]` that is followed by a `mod` item (a test-only `use` or
    fn above real code does not start the test block)."""
    lines = text.splitlines()
    for n, ln in enumerate(lines, 1):
        if ln.strip().startswith('#[cfg(test)]'):
            for nxt in lines[n:n + 3]:
                if re.match(r'\s*(pub(\([\w:]+\))?\s+)?mod\s', nxt):
                    return n
                if nxt.strip() and not nxt.strip().startswith('#['):
                    break
    return None


def diff_lines(root, base, to=None):
    """[(path, line)] of every added and removed line under SCOPES, the lines of a Rust test
    file or a #[cfg(test)] block left out."""
    git = ['git', '-C', root, '-c', 'commit.gpgsign=false']
    specs = [[f'{base}..{to}']] if to else [[f'{base}...HEAD'], ['HEAD']]
    new_rev, out, starts = to, [], {}

    def start(side, path):
        if (side, path) not in starts:
            rev = base if side == 'old' else new_rev
            starts[(side, path)] = _cfg_test_start(_show(root, rev, path)) if path.endswith('.rs') \
                else None
        return starts[(side, path)]

    for spec in specs:
        r = subprocess.run(git + ['diff', '-U0', '--no-color', *spec, '--', *SCOPES],
                           capture_output=True, text=True)
        if r.returncode != 0:
            raise SystemExit(f'web-affected: git diff {" ".join(spec)} failed: {r.stderr.strip()}')
        cur, old_n, new_n = None, 0, 0
        for ln in r.stdout.splitlines():
            if ln.startswith('+++ '):
                cur = ln[6:] if ln.startswith('+++ b/') else cur
            elif ln.startswith('--- '):
                cur = ln[6:] if ln.startswith('--- a/') else cur
            elif ln.startswith('@@'):
                m = re.match(r'@@ -(\d+)(?:,\d+)? \+(\d+)', ln)
                old_n, new_n = (int(m.group(1)), int(m.group(2))) if m else (0, 0)
            elif ln.startswith(('+', '-')) and cur:
                side = 'new' if ln[0] == '+' else 'old'
                n = new_n if side == 'new' else old_n
                if side == 'new':
                    new_n += 1
                else:
                    old_n += 1
                if cur.endswith('.rs') and (is_test_file(cur) or
                                            (start(side, cur) and n >= start(side, cur))):
                    continue
                out.append((cur, ln[1:]))
    if not to:
        r = subprocess.run(git + ['ls-files', '--others', '--exclude-standard', '--', *SCOPES],
                           capture_output=True, text=True)
        for f in r.stdout.splitlines():
            if f.endswith('.rs') and is_test_file(f):
                continue
            text = _show(root, None, f)
            cut = _cfg_test_start(text) if f.endswith('.rs') else None
            out.extend((f, ln) for n, ln in enumerate(text.splitlines(), 1)
                       if not (cut and n >= cut))
    return out


def timing_labels(root, touched):
    """The labels that are timing checks: a `_timing` part and the serial scripts of run-all.sh."""
    with open(os.path.join(root, touched.CHECKS, 'run-all.sh'), encoding='utf-8') as f:
        text = f.read()
    m = re.search(r'serial_scripts=\((.*?)\)', text, re.S)
    serial = set(m.group(1).split()) if m else set()
    return {label for label, script, _ in touched.gate_entries(root)
            if '_timing-' in label or script[:-3] in serial}


def check_files(root, touched, checks_dir):
    """(entries, own, helpers): entries are run-all.sh's (label, script, kind); own maps a script
    to its text and helpers maps it to the texts of its helper files (browser checks without
    common.py; node harnesses: every file under the checks dir they read)."""
    entries = touched.gate_entries(root)
    cache = {}

    def read(path):
        if path not in cache:
            try:
                with open(path, encoding='utf-8') as f:
                    cache[path] = f.read()
            except (OSError, UnicodeDecodeError):
                cache[path] = ''
        return cache[path]

    own, helpers = {}, {}
    for _, script, kind in entries:
        if script in own:
            continue
        own[script] = read(os.path.join(checks_dir, script))
        found = set()
        if kind == 'node':
            found, _ = touched.js_closure(root, script)
        elif kind == 'browser':
            try:
                found = touched.py_closure(root, script)
            except Exception:  # an unreadable helper: the script alone
                found = set()
        me = os.path.normpath(os.path.join(touched.CHECKS, script))
        helpers[script] = {f: read(os.path.join(root, f)) for f in sorted(found)
                           if f.startswith(touched.CHECKS) and f != me and
                           not f.endswith('common.py') and
                           os.path.isfile(os.path.join(root, f))}
    return entries, own, helpers


def by_tokens(root, touched, lines, checks_dir, popups=None):
    """(hits, dropped): hits is {label: why} for every check whose files name a token of the
    changed lines or pin the popup list; dropped is {token: scripts it was found in}."""
    entries, own, helpers = check_files(root, touched, checks_dir)
    importers = Counter(f for hs in helpers.values() for f in hs)
    text = {s: own[s] + ''.join(t for f, t in helpers[s].items() if importers[f] <= MAX_IMPORTERS)
            for s in own}
    toks = {}
    for path, line in lines:
        for t in tokens_of(path, line):
            toks.setdefault(t, path)
    for name, (label, const) in (popups or {}).items():
        toks.setdefault(f'menu-popup-{name}', LISTED)
        if label:
            toks.setdefault(label, LISTED)
    named, dropped = {}, {}
    for t in toks:
        rx = re.compile(r'(?<![\w-])' + re.escape(t) + r'(?![\w-])') \
            if re.fullmatch(r'[\w-]+', t) else None
        found = {s for s, body in text.items() if (rx.search(body) if rx else t in body)}
        if len(found) > MAX_SCRIPTS:
            dropped[t] = len(found)
        elif found:
            named[t] = found
    why = {}                            # script -> [(count, token)]
    for t, scripts in named.items():
        for s in scripts:
            why.setdefault(s, []).append((len(scripts), t))
    reason = {}
    for s, v in why.items():
        v.sort()
        more = f' (+{len(v) - 1} more)' if len(v) > 1 else ''
        reason[s] = f"token '{v[0][1]}'{more} in {s}"
    if popups:
        forms = []
        for name, (label, const) in popups.items():
            alts = [re.escape(f'menu-popup-{name}'), r'''['"]''' + re.escape(name) + r'''['"]''']
            if const:
                alts.append(r'\b' + re.escape(const) + r'\b')
            if label:
                alts.append(re.escape(label))
            forms.append((name, re.compile('|'.join(alts))))
        for s, body in own.items():
            seen = sorted((m.start(), name) for name, rx in forms for m in rx.finditer(body))
            pair = next(((a, b) for i, (pa, a) in enumerate(seen) for pb, b in seen[i + 1:]
                         if a != b and pb - pa <= PIN_WINDOW), None)
            if pair and s not in reason:
                reason[s] = f"popups '{pair[0]}' and '{pair[1]}' together in {s}"
    hits = {}
    for label, script, _ in entries:
        if script in reason:
            hits.setdefault(label, reason[script])
    return dict(sorted(hits.items())), dict(sorted(dropped.items()))


def affected(root, base, to, checks_dir, tokens_only=False):
    """(hits, gate_files, dropped, timing, note): hits {label: why}; gate_files the changed
    files of the gate itself; timing the labels of hits that are timing checks."""
    touched = _load_touched(root)
    changed = touched.changed_files(root, base, to) if to else touched.changed_files(root, base)
    gate = [f for f in changed if f in touched.GATE]
    own_tree = checks_dir == DEFAULT_CHECKS
    croot = root if own_tree else os.path.dirname(os.path.dirname(os.path.abspath(checks_dir)))
    lines = diff_lines(root, base, to)
    popups = listed_popups(root, base, to, lines)
    hits, dropped = by_tokens(croot, touched, lines, os.path.join(croot, DEFAULT_CHECKS) if own_tree
                              else os.path.abspath(checks_dir), popups)
    if own_tree and not tokens_only:
        thit, _, _ = touched.touched(root, [f for f in changed if f not in touched.GATE])
        for label, why in thit.items():
            hits.setdefault(label, f'touched.py: {why}')
    timing = timing_labels(croot, touched) & set(hits)
    return dict(sorted(hits.items())), gate, dropped, timing


def default_base(root):
    base = os.environ.get('WEB_CHECKS_BASE')
    if base:
        return base
    r = subprocess.run(['git', '-C', root, 'for-each-ref', '--sort=-committerdate',
                        '--format=%(refname:short)', 'refs/heads/release-*'],
                       capture_output=True, text=True)
    for name in r.stdout.split():
        if re.fullmatch(r'release-\d+\.\d+\.\d+', name) and subprocess.run(
                ['git', '-C', root, 'merge-base', '--is-ancestor', name, 'HEAD']).returncode == 0:
            return name
    return 'main'


def main(argv):
    if argv[:1] == ['--self-test']:
        return self_test()
    list_only = tokens_only = with_timing = False
    checks_dir, rest = DEFAULT_CHECKS, []
    it = iter(argv)
    for a in it:
        if a == '--list':
            list_only = True
        elif a == '--tokens-only':
            list_only = tokens_only = True
        elif a == '--timing':
            with_timing = True
        elif a == '--checks-dir':
            checks_dir = next(it, None) or sys.exit('web-affected: --checks-dir needs a directory')
        else:
            rest.append(a)
    if len(rest) > 2:
        sys.exit('usage: web-affected.py [--list] [--tokens-only] [--timing] [--checks-dir DIR] '
                 '[BASE [TO]]')
    base = rest[0] if rest else default_base(ROOT)
    to = rest[1] if len(rest) == 2 else None
    hits, gate, dropped, timing = affected(ROOT, base, to, checks_dir, tokens_only)
    span = f'from {base} to {to}' if to else f'since {base}'
    if checks_dir != DEFAULT_CHECKS:
        print("note: touched.py's half is skipped with --checks-dir")
    if gate:
        names = ', '.join(os.path.basename(f) for f in gate)
        print(f'a gate file changed ({names}): the full gate is owed at merge')
    for t, n in dropped.items():
        print(f"dropped '{t}' ({n} scripts): grep by hand if it is yours")
    if not hits:
        print(f'web-affected: no web check names or loads anything changed {span}; nothing to run')
        return 0
    print(f'web-affected: {len(hits)} checks are affected by the change {span}:')
    for label, why in hits.items():
        print(f'  {label:<34} {why}' + ('   [timing]' if label in timing else ''))
    run = [label for label in hits if with_timing or label not in timing]
    left = [label for label in hits if label not in run]
    if left:
        print(f'owed to the gate on a quiet machine (timing, run with --timing): {" ".join(left)}')
    if not run:
        print('nothing else to run')
        return 0
    if list_only:
        print(f'run them:\n  WEB_CHECKS_ONLY="{" ".join(run)}" bash scripts/web-checks/run-all.sh')
        return 0
    env = dict(os.environ, WEB_CHECKS_ONLY=' '.join(run))
    return subprocess.run(['bash', 'scripts/web-checks/run-all.sh'], cwd=ROOT, env=env).returncode


# ---------------------------------------------------------------- self-test

def _write(root, path, text='x\n'):
    full = os.path.join(root, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, 'w') as f:
        f.write(text)


def self_test():
    os.makedirs('/tmp/svolder', exist_ok=True)
    tmp = tempfile.mkdtemp(prefix='web-affected-', dir='/tmp/svolder')
    try:
        _self_test(tmp)
    finally:
        subprocess.run(['rm', '-rf', tmp])
    print('web-affected self-test: ok')


LISTED_TEXT = '''// header comment, name: the router's
export const FX = 'fx';
export const POPUPS = [
  {
    name: FX,
    title: 'Effects',
    entries: [{ id: 'side', label: 'Sidelabel' }],
    row: { label: 'Effects menu', sees: 'everyone' },
  },
  {
    name: 'other',
    title: 'Other',
    row: { label: 'Other', sees: 'writer' },
  },
];
'''


def _self_test(root):
    chk = 'scripts/web-checks/'
    web = 'bins/svolderd/web/'
    for f in ('touched.py', 'fingerprint.py'):
        _write(root, chk + f, open(os.path.join(HERE, 'web-checks', f)).read())
    _write(root, chk + 'run-all.sh', '''
for name in rows; do
    node_entries+=("node-$name|node/$name.mjs|")
done
browser_checks=(menu strip sweepy plain pin helped one two)
split_scripts=(sweepy)
serial_scripts=(slow)
entries+=("slow-chromium|slow.py|" "slow-firefox|slow.py|")
''')
    _write(root, chk + 'common.py', "# names sc-common-only\n")
    _write(root, chk + 'menu.py', "import common\nclick('menu-popup-fx'); expect('Effects menu')\n")
    _write(root, chk + 'strip.py', "import common\nget('sc-row'); get('old-row')\n")
    _write(root, chk + 'sweepy.py', "import common\nget('sc-sweep-only')\n")
    _write(root, chk + 'plain.py', "import common\nprint('nothing')\n")
    _write(root, chk + 'slow.py', "import common\nget('sc-slow')\n")
    # pin names two popups side by side; helped reaches one popup only through a helper
    _write(root, chk + 'pin.py', "import common\nexpect(['fx', 'other'])\n")
    _write(root, chk + 'one.py', "import common\nexpect(['fx'])\n" + "# pad\n" * 40 + "expect(['other'])\n")
    _write(root, chk + 'two.py', "import common\nimport lists\nprint(1)\n")
    _write(root, chk + 'helped.py', "import common\nimport lists\nprint(2)\n")
    _write(root, chk + 'lists.py', "NAMES = ['fx', 'other']\n")
    _write(root, chk + 'node/rows.mjs',
           "const f = './fixtures/rows.json'; const e = 'scene-changed';\n")
    _write(root, chk + 'node/fixtures/rows.json', '{"note": "Saved under a fixture name"}\n')
    _write(root, web + 'core/a.js', "const a = 1;\n")
    _write(root, web + 'core/popups/listed.js', LISTED_TEXT)
    _write(root, 'bins/svolderd/src/lib.rs', 'fn f() {}\n')
    _write(root, 'bins/svolderd/src/web/tests.rs', 'fn t() {}\n')
    _write(root, 'bins/svolderd/src/web/page.rs', 'fn p() {}\n#[cfg(test)]\nmod tests {}\n')
    _write(root, 'crates/x/src/lib.rs', 'fn g() {}\n')

    def git(*a):
        subprocess.run(['git', '-C', root, '-c', 'commit.gpgsign=false', *a], check=True,
                       capture_output=True)
    git('init', '-q', '-b', 'main')
    git('config', 'user.email', 't@t')
    git('config', 'user.name', 't')
    git('add', '-A', '-f')
    git('commit', '-q', '-m', 'x')
    touched = _load_touched(root)

    def labels(*names):
        return sorted(f'{n}-{e}' for n in names for e in ('chromium', 'firefox'))

    def after(*pairs, tokens_only=True):
        for path, text in pairs:
            _write(root, path, text)
        git('add', '-A', '-f')
        git('commit', '-q', '-m', 'two')
        got = affected(root, 'HEAD~1', 'HEAD', DEFAULT_CHECKS, tokens_only)
        git('reset', '-q', '--hard', 'HEAD~1')
        return sorted(got[0]), got

    # an added id lists the check naming it, in both engines, and no other
    got, _ = after((web + 'core/a.js', 'const a = 1;\n<div id="sc-row"></div>\n'))
    assert got == labels('strip'), got
    # a split script's timing part is listed and marked
    got, full = after((web + 'core/a.js', "el('#sc-sweep-only')\n"))
    assert got == labels('sweepy') + labels('sweepy_timing'), got
    assert full[3] == set(labels('sweepy_timing')), full[3]
    # a serial script is a timing check too
    got, full = after((web + 'core/a.js', "el('#sc-slow')\n"))
    assert got == labels('slow') and full[3] == set(got), (got, full[3])
    # listed.js: a new entry lists the check naming its menu id and the checks that pin the list
    # (two different popups of the list in the script itself within 160 characters)
    new = LISTED_TEXT.replace("];\n", "  {\n    name: 'third',\n    title: 'Third',\n"
                              "    row: { label: 'Third row', sees: 'everyone' },\n  },\n];\n")
    got, _ = after((web + 'core/popups/listed.js', new))
    assert got == labels('menu', 'pin'), got       # not one (too far apart), not two/helped (helper)
    # the header comment and the side list make no popup and no token
    got, _ = after((web + 'core/popups/listed.js',
                    LISTED_TEXT.replace("name: the router's", "name: the router's, changed")
                    .replace("'Sidelabel'", "'Side words'")))
    assert got == [], got
    # a removed id lists the checks that still name it
    _write(root, web + 'core/a.js', '<div id="old-row"></div>\n')
    git('add', '-A', '-f')
    git('commit', '-q', '-m', 'has old-row')
    got, _ = after((web + 'core/a.js', 'const a = 1;\n'))
    assert got == labels('strip'), got
    # an event name from the daemon, and a node harness's own text and its fixture
    got, _ = after(('bins/svolderd/src/lib.rs', 'fn f() { send_json(tx, "scene-changed", v); }\n'))
    assert got == ['node-rows'], got
    got, _ = after((web.replace('web/', 'src/web/') + 'page.rs',
                    'fn p() { s("Saved under a fixture name"); }\n#[cfg(test)]\nmod tests {}\n'))
    assert got == ['node-rows'], got
    # a Rust string in a test file or a cfg(test) block reaches no page: nothing
    got, _ = after((web.replace('web/', 'src/web/') + 'tests.rs', 'const A: &str = "Saved under a fixture name";\n'))
    assert got == [], got
    got, _ = after((web.replace('web/', 'src/web/') + 'page.rs',
                    'fn p() {}\n#[cfg(test)]\nmod tests { const A: &str = "Saved under a fixture name"; }\n'))
    assert got == [], got
    # a test-only `use` before real code does not start the test block: only `mod` after it does
    _write(root, 'bins/svolderd/src/web/model.rs',
           '#[cfg(test)]\nuse std::fmt;\nfn real() {}\n#[cfg(test)]\nmod tests {}\n')
    git('add', '-A', '-f')
    git('commit', '-q', '-m', 'model')
    got, _ = after(('bins/svolderd/src/web/model.rs',
                    '#[cfg(test)]\nuse std::fmt;\nfn real() { s("Saved under a fixture name"); }\n'
                    '#[cfg(test)]\nmod tests {}\n'))
    assert got == ['node-rows'], got
    # a Rust string outside src/web/ is no page string either
    got, _ = after(('bins/svolderd/src/lib.rs', 'const A: &str = "Saved under a fixture name";\n'))
    assert got == [], got
    # a diff outside the daemon, a stop-listed token and common.py's words: nothing
    got, _ = after(('crates/x/src/lib.rs', 'const A: &str = "sc-row and Effects menu";\n'))
    assert got == [], got
    got, _ = after((web + 'core/a.js', '<div id="old-row"></div>\nx = true; id="div"; id="sc-common-only";\n'))
    assert got == [], got
    # a comment line names nothing
    got, _ = after((web + 'core/a.js', '<div id="old-row"></div>\n// <div id="sc-row"></div>\n'))
    assert got == [], got

    # the union: a check script edited inside the diff is listed by touched.py (not by a token),
    # next to the token's own check
    union_edit = [(chk + 'plain.py', "import common\nprint('changed')\n"),
                  (web + 'core/a.js', '<div id="old-row"></div>\n<div id="sc-row"></div>\n')]
    got, full = after(*union_edit)
    assert got == labels('strip'), got
    got, full = after(*union_edit, tokens_only=False)
    assert set(labels('strip', 'plain')) <= set(full[0]), full[0]
    assert full[0]['plain-chromium'].startswith('touched.py'), full[0]['plain-chromium']
    assert full[0]['strip-chromium'].startswith('token'), full[0]['strip-chromium']

    # a gate file changed with a new check line: the token's check is still listed, the gate
    # file is reported, and touched.py does not list the whole gate for it
    ra = open(os.path.join(root, chk + 'run-all.sh')).read().replace('plain pin', 'plain fresh pin')
    got, full = after((chk + 'run-all.sh', ra), (chk + 'fresh.py', "import common\n"),
                      ('bins/svolderd/src/web/page.rs',
                       'fn p() { s("<div id=\\"sc-row\\">"); }\n#[cfg(test)]\nmod tests {}\n'),
                      tokens_only=False)
    assert full[1] == [chk + 'run-all.sh'], full[1]
    assert got == labels('fresh', 'strip'), full[0]

    # a token in more than ten scripts is dropped and reported
    for n in range(11):
        _write(root, chk + f'many{n}.py', "import common\nget('sc-everywhere')\n")
    ra = open(os.path.join(root, chk + 'run-all.sh')).read().replace(
        'plain', 'plain ' + ' '.join(f'many{n}' for n in range(11)), 1)
    _write(root, chk + 'run-all.sh', ra)
    git('add', '-A', '-f')
    git('commit', '-q', '-m', 'many')
    got, full = after((web + 'core/a.js', '<div id="old-row"></div>\n<div id="sc-everywhere"></div>\n'))
    assert got == [] and full[2] == {'sc-everywhere': 11}, (got, full[2])


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
