# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The pool the browser mutation runners (mutate_*.py) share. A runner gives it the mutant names, an
`attempt(name)` returning `(killed, note, secs)` (`killed` is the list of killing lines, `[]` for a
survivor, None for an error) and the names that must run alone (the ones whose named check measures
timing). The pool:
- runs the mutants with MUT_WORKERS workers (default: `default_workers()`, from the cores and the
  CPU pressure; 1 runs them one by one, in order, with no pool phase);
- holds the web gate lock (/tmp/svolder-web-gate.lock) shared around every run, exclusive around a timing one;
- never runs a timing mutant in the pool: those run alone after it;
- runs every mutant the pool did not KILL (survived or error) again alone: only the alone result counts
  and both are printed. A pool KILLED stands (a check the mutant names failed);
- prints the results in the order of the mutants, then `N of M mutations killed`.
It also holds the helpers every runner needs: `run_group` (a run in a session of its own, ended
with its whole group past a timeout), `launch` (the same, trying again when the check's daemon lost
the race for its free port), `checks` (the (check, line) pairs of a check script's output)."""
import atexit
import concurrent.futures
import contextlib
import fcntl
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time

GATE = '/tmp/svolder-web-gate.lock'
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import common  # noqa: E402
# a daemon that did not answer: common.free_port binds, closes and returns, so two daemons starting at
# once can be given the same port and one of them never serves
START_FAILED = 'daemon did not answer'


def pressure():
    """The CPU's `some avg10` from /proc/pressure/cpu, in percent; 0 when the file is not there."""
    try:
        with open('/proc/pressure/cpu') as f:
            m = re.search(r'avg10=([0-9.]+)', f.readline())
        return float(m.group(1)) if m else 0.0
    except OSError:
        return 0.0


def default_workers(cores=None, some=None):
    """About a third of the cores a busy machine leaves free (each worker is a daemon and a browser),
    at least 1 and at most 8."""
    cores = cores or os.cpu_count() or 1
    some = pressure() if some is None else some
    return max(1, min(8, int(cores * max(0.0, 1.0 - some / 100.0) / 3)))


def workers():
    """MUT_WORKERS, or `default_workers()`."""
    raw = os.environ.get('MUT_WORKERS', '').strip()
    if not raw:
        return default_workers()
    try:
        return max(1, int(raw))
    except ValueError:
        sys.exit(f'MUT_WORKERS must be a whole number, not {raw!r}')


_GROUPS = set()
_GROUPS_LOCK = threading.RLock()  # reentrant: reap() runs in a signal handler, maybe inside a holder
_STOPPING = threading.Event()


def reap():
    """End every process group a run started and still has (a TERM, a moment for the check to close its
    browser and daemon, then a KILL). Every run is a session of its own, so the terminal's Ctrl-C does not reach it."""
    with _GROUPS_LOCK:
        groups = list(_GROUPS)
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for g in groups:
            try:
                os.killpg(g, sig)
            except OSError:
                pass
        if sig == signal.SIGTERM and groups:
            time.sleep(2)


atexit.register(reap)


def run_group(cmd, env, secs, cwd=HERE):
    """Run `cmd` in a session of its own; past `secs` end the whole group (a TERM first, so the check
    closes its browsers and stops its daemon, then a KILL), so nothing it started outlives it. A
    timeout is a failed run with no check output, not an error that ends the mutation run."""
    if _STOPPING.is_set():
        raise KeyboardInterrupt
    proc = subprocess.Popen(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                            cwd=cwd, start_new_session=True)
    # a signal between Popen and this add leaves a child unregistered for that instant; the stopping flag is set first
    # by the handler, and the child is then reaped here
    with _GROUPS_LOCK:
        _GROUPS.add(proc.pid)
    if _STOPPING.is_set():
        reap()
        raise KeyboardInterrupt
    try:
        out, err = proc.communicate(timeout=secs)
    except BaseException as e:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except OSError:
            pass
        try:
            out, err = proc.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            out, err = '', ''
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except OSError:
            pass
        proc.wait()
        if not isinstance(e, subprocess.TimeoutExpired):
            raise
        err = f'timed out after {secs} s\n{err or ""}'
    finally:
        with _GROUPS_LOCK:
            _GROUPS.discard(proc.pid)
    return subprocess.CompletedProcess(cmd, proc.returncode, out or '', err or '')


def launch(cmd, env, secs=1800, cwd=HERE, tries=3):
    """`run_group`, run again (up to `tries` runs) while the run left no check line and its daemon did
    not answer: a port taken between its choice and its bind, not a result."""
    for _ in range(tries):
        if _STOPPING.is_set():
            raise KeyboardInterrupt
        r = run_group(cmd, env, secs, cwd)
        lost = START_FAILED in r.stderr and not re.search(r'^(ok  |FAIL)', r.stdout, re.M)
        if not lost:
            break
    return r


def checks(stdout):
    """The (check, line) pairs of a check script's output: a check's own line names it, the indented
    lines after it are parts of it (they carry the name of the check they belong to)."""
    lines, current = [], ''
    for line in stdout.splitlines():
        if not line.startswith(('ok  ', 'FAIL')):
            continue
        text = line[5:]
        if not text.startswith('  '):
            current = text
        lines.append((current, line))
    return lines


def lines_of(name, result, tag=''):
    killed, note, secs = result
    out = ['ERROR   ' if killed is None else 'KILLED  ' if killed else 'SURVIVED']
    out[0] += f' {name} ({secs:.0f} s){tag}'
    out += ['     ' + f[:170] for f in (killed or [])[:3]]
    if note:
        out.append('     ' + note)
    return out


@contextlib.contextmanager
def gate(exclusive):
    """The web gate lock: shared for a browser run, exclusive for a timing one (waits for the others)."""
    with open(GATE, 'a') as f:
        fcntl.flock(f, fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH)
        try:
            yield
        finally:
            fcntl.flock(f, fcntl.LOCK_UN)


def run_names(names, attempt, timed=(), serial=False):
    """Run `names` through `attempt` as described above; print as it goes and the summary; the exit
    code (1 when a mutant is not killed). An `attempt` that raises is an ERROR row for that mutant. On
    SIGINT/SIGTERM (or any exception that leaves the run) every child group is ended, the queue is
    dropped and the run stops with 130."""
    timed = set(timed)
    plain = attempt

    def attempt(name):
        began = time.time()
        try:
            with gate(name in timed):
                return plain(name)
        except KeyboardInterrupt:
            raise
        except Exception as e:
            return None, f'(error, not a kill) {type(e).__name__}: {str(e)[:200]}', time.time() - began

    n = 1 if serial else workers()
    final = {}
    ex = None
    old = {}

    def stop(signum, frame):
        _STOPPING.set()
        reap()
        raise KeyboardInterrupt

    if threading.current_thread() is threading.main_thread():
        for sig in (signal.SIGINT, signal.SIGTERM):
            old[sig] = signal.signal(sig, stop)
    try:
        if n <= 1:
            for name in names:
                final[name] = attempt(name)
                print('\n'.join(lines_of(name, final[name])), flush=True)
        else:
            alone = [x for x in names if x in timed]
            pooled = [x for x in names if x not in timed]
            again = []
            ex = concurrent.futures.ThreadPoolExecutor(n)
            futures = {x: ex.submit(attempt, x) for x in pooled}
            for name in pooled:
                result = futures[name].result()
                print('\n'.join(lines_of(name, result, ' [pool]')), flush=True)
                if result[0]:
                    final[name] = result
                else:
                    again.append(name)
            ex.shutdown(wait=True)
            # alone, one at a time: what the pool did not kill, then what needs the machine to itself
            for name in again + alone:
                final[name] = attempt(name)
                print('\n'.join(lines_of(name, final[name], ' [alone]')), flush=True)
            print('--- the results, in order (only the alone result counts for a mutant run alone) ---')
            for name in names:
                print('\n'.join(lines_of(name, final[name])))
    except BaseException:
        _STOPPING.set()
        if ex:
            ex.shutdown(wait=False, cancel_futures=True)
        reap()
        done = sum(1 for x in names if x in final and final[x][0])
        print(f'interrupted: {done} of {len(names)} mutations killed so far', flush=True)
        raise
    finally:
        for sig, h in old.items():
            signal.signal(sig, h)
    bad = sum(1 for name in names if not final[name][0])
    print(f'{len(names) - bad} of {len(names)} mutations killed')
    return 1 if bad else 0


# ---- the pieces of a browser mutant that every runner repeats ----------------------------------------

def scratch_web(edits, label=''):
    """A scratch copy of the web dir with `edits` [(file, old, new)] made, each `old` once; the scratch
    dir (the copy is `<dir>/w`). The caller removes it."""
    web = common.scratch('mut-web-')
    try:
        shutil.copytree(common.WEB, web + '/w')
        for f, old, new in edits:
            p = f'{web}/w/{f}'
            s = open(p).read()
            assert s.count(old) == 1, f'{label}: {old[:50]!r} not once in {f}'
            open(p, 'w').write(s.replace(old, new, 1))
    except BaseException:
        shutil.rmtree(web, ignore_errors=True)
        raise
    return web


GATE_SRC = '''
import os as _os
_CASES = set(filter(None, _os.environ.get('MUT_CASES', '').split(',')))


def _case(fn):
    def gated(*a, **k):
        if _CASES and fn.__name__ not in _CASES:
            return None
        return fn(*a, **k)
    gated.__name__ = fn.__name__
    return gated

'''


def gated_copy(script, into):
    """`script` with every function it calls as a bare statement (`name(...)` alone on a line, at its
    top level or one block deep) gated by MUT_CASES (the names to run; unset runs them all), written
    into `into`; the path. For a script that is a flat run of such calls (paper.py)."""
    text = open(f'{HERE}/{script}').read()
    defined = set(re.findall(r'^def (\w+)\(', text, re.M))
    run = list(re.finditer(r'^\w+ = common\.Daemon\(', text, re.M))[-1].start()
    called = set(re.findall(r'^ *(\w+)\((?:b, d|d|)\)$', text[run:], re.M))
    cases = sorted(defined & called)
    first = re.search(r'^def ', text, re.M).start()
    text = text[:first] + GATE_SRC.lstrip('\n') + '\n\n' + text[first:]
    text = re.sub(r'^def (%s)\(' % '|'.join(cases), lambda m: '@_case\n' + m.group(0), text, flags=re.M)
    # a one-line INFO print reads the page of a run that skipped the cases which load it: not a check, left out
    text = re.sub(r"^( *)print\('INFO [^\n]*\)$", r'\1pass', text, flags=re.M)
    path = f'{into}/{script}'
    open(path, 'w').write(text)
    return path


def run_checks(script, web, cases=None, gate='argv'):
    """Run the check `script` (Chromium) on the web copy `web/w` for `cases` (None: the whole script):
    `gate='argv'` passes them after the browser name (computer_ember.py, computer_paper.py), `'env'`
    gates the script's case functions with MUT_CASES (gated_copy). The (check, line) pairs, the return
    code and stderr."""
    work = common.scratch('mut-run-')
    env = dict(os.environ, SMOKE_WEB=web + '/w', PYTHONPATH=HERE)
    path, extra = f'{HERE}/{script}', []
    try:
        if cases and gate == 'env':
            path = gated_copy(script, work)
            env['MUT_CASES'] = ','.join(cases)
        elif cases:
            extra = list(cases)
        r = launch([sys.executable, path, 'chromium', *extra], env)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return checks(r.stdout), r.returncode, r.stderr


class Baselines:
    """The unmutated run of a script's cases, once per key (whichever thread asks first runs it, the
    others wait): `problem(key, run)` is `run()`'s problem string or None. Only a pass is kept; a
    problem (which may be a flake under load) is run again by the next asker, so the alone re-run of a
    mutant never repeats the pool's flake."""

    def __init__(self):
        self.ok, self.locks, self.guard = set(), {}, threading.Lock()

    def problem(self, key, run):
        with self.guard:
            lock = self.locks.setdefault(key, threading.Lock())
        with lock:
            if key in self.ok:
                return None
            found = run()
            if not found:
                self.ok.add(key)
            return found


def verdict(lines, code, err, named=None):
    """`(killed, note)` of one run's check lines: the FAIL lines of a check `named` (substrings of a
    check's name or line; None: any check) are the kill; None (an error, not a kill) when the run
    failed with no check output; a failure of any other check alone is a survivor with a note."""
    killed, other = [], []
    for current, line in lines:
        if line.startswith('FAIL'):
            if named is None or any(n in current or n in line for n in named):
                killed.append(line)
            else:
                other.append(line)
    if not killed and not other and code:
        tail = (err.strip().splitlines() or ['no output'])[-1][:200]
        return None, '(error, not a kill) ' + tail
    if other and not killed:
        return [], '(failed only other checks, not a kill) ' + other[0][:140]
    return killed, ''


def timed_call(fn, name):
    """`(killed, note, secs)` of `fn(name)` returning `(killed, note)`: the `attempt` the pool wants."""
    began = time.time()
    killed, note = fn(name)
    return killed, note, time.time() - began


def run_script(name, script, edits, env=None, args=('chromium',), secs=900):
    """`(killed, note)` of the plain kind: the web copy with `edits` made, `script` run on it; any FAIL
    line kills (the script gates its own sections through `env`/`args`)."""
    web = scratch_web(edits, name)
    try:
        r = launch([sys.executable, f'{HERE}/{script}', *args], dict(os.environ, SMOKE_WEB=web + '/w', **(env or {})), secs)
    finally:
        shutil.rmtree(web, ignore_errors=True)
    return verdict(checks(r.stdout), r.returncode, r.stderr)
