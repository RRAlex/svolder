# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Self-test of mutpool.py, the pool the browser mutation runners share: no browser, a second or two.
Usage: python3 mutpool_test.py"""
import atexit
import os
import shutil
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import tmp_base  # noqa: E402
import threading

import mutpool

fails = []


def check(name, cond, detail=''):
    print('ok  ' if cond else 'FAIL', name, '' if cond else detail)
    if not cond:
        fails.append(name)


# the default workers follow the cores and the pressure, and MUT_WORKERS wins
check('an idle 24-core machine gets 8 workers', mutpool.default_workers(24, 0.0) == 8)
check('a half-busy 24-core machine gets fewer', mutpool.default_workers(24, 50.0) == 4)
check('a saturated machine gets one', mutpool.default_workers(24, 100.0) == 1)
check('a small machine gets one', mutpool.default_workers(2, 0.0) == 1)
os.environ['MUT_WORKERS'] = '3'
check('MUT_WORKERS overrides', mutpool.workers() == 3)
del os.environ['MUT_WORKERS']

# a failing baseline is not kept: the next asker runs it again; a passing one runs once
base, runs = mutpool.Baselines(), []


def flaky():
    runs.append(1)
    return 'fails unmutated' if len(runs) == 1 else None


check('a baseline that flaked is reported', base.problem('k', flaky) == 'fails unmutated')
check('and run again, not remembered', base.problem('k', flaky) is None and len(runs) == 2)
check('a passing baseline runs once', base.problem('k', flaky) is None and len(runs) == 2)

# a daemon that lost the race for its port is started again; any other failure is a result
work = tmp_base.mkdtemp('mp')
atexit.register(shutil.rmtree, work, True)
count = os.path.join(work, 'count')
script = os.path.join(work, 'run.py')


def make_script(lose):
    """A check script that says its daemon did not answer for its first `lose` runs, then passes."""
    with open(script, 'w') as f:
        f.write(f"""import os, sys
n = len(open({count!r}).read()) if os.path.exists({count!r}) else 0
open({count!r}, 'a').write('x')
if n < {lose}:
    sys.stderr.write('RuntimeError: daemon did not answer; its log ended: address in use\\n')
    sys.exit(1)
print('ok   a check')
""")


make_script(1)
r = mutpool.launch([sys.executable, script], dict(os.environ), 30, tries=3)
check('a lost start is run again', 'ok   a check' in r.stdout and len(open(count).read()) == 2, r)
os.remove(count)
make_script(5)
r = mutpool.launch([sys.executable, script], dict(os.environ), 30, tries=3)
check('but only up to its tries', r.returncode == 1 and len(open(count).read()) == 3)

# the verdict: a named check kills, another check alone does not, no output is an error
lines = [('a', 'FAIL a: x'), ('b', 'ok   b'), ('c', 'FAIL c: y')]
check('a named failing check kills', mutpool.verdict(lines, 1, '', ['a'])[0] == ['FAIL a: x'])
check('another check failing alone is a survivor', mutpool.verdict(lines, 1, '', ['b'])[0] == [])
check('no output and a failing run is an error', mutpool.verdict([], 1, 'boom')[0] is None)

# the pool: pool kills stand, a non-kill is run again alone and only that result counts, a timing mutant runs alone
log = []
state = {'flaky': 0}
lock = threading.Lock()


def attempt(name):
    with lock:
        log.append(name)
        if name == 'flaky':
            state['flaky'] += 1
            return ([], '', 0) if state['flaky'] == 1 else (['FAIL x'], '', 0)
    return (['FAIL x'] if name != 'survivor' else [], '', 0)


os.environ['MUT_WORKERS'] = '3'
mutpool.GATE = os.path.join(work, 'gate.lock')
code = mutpool.run_names(['killed', 'flaky', 'timed', 'survivor'], attempt, ['timed'])
check('a survivor of the pool and of its alone run fails the run', code == 1)
check('the flaky one was run twice, the killed once', log.count('flaky') == 2 and log.count('killed') == 1)
check('the timing one ran after the pool', log.index('timed') > log.index('killed'))
check('the survivor was run again alone', log.count('survivor') == 2)

# a raising attempt is an ERROR row, not the end of the run
def boom(name):
    raise RuntimeError('scratch assert')


check('an attempt that raises is an error row and the run goes on', mutpool.run_names(['a'], boom) == 1)

# Ctrl-C and TERM end every child group (each run is its own session, so the terminal never reaches it)
import signal
import subprocess
import time
marker = 'mutpool-test-sleeper-' + str(os.getpid())
driver = os.path.join(work, 'driver.py')
with open(driver, 'w') as f:
    f.write(f"""import os, sys
sys.path.insert(0, {mutpool.HERE!r})
import mutpool
mutpool.GATE = {os.path.join(work, 'gate2.lock')!r}
def attempt(name):
    r = mutpool.launch(['sh', '-c', 'sleep 60 # ' + {marker!r}], dict(os.environ), 120)
    return [], '', 0
sys.exit(mutpool.run_names(['a', 'b', 'c', 'd', 'e'], attempt))
""")
for sig in (signal.SIGINT, signal.SIGTERM):
    env = dict(os.environ, MUT_WORKERS='2')
    p = subprocess.Popen([sys.executable, driver], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    time.sleep(5)
    left = subprocess.run(['pgrep', '-f', 'sleep 60 # ' + marker], capture_output=True, text=True).stdout.split()
    check(f'{sig.name}: the children are running before it', len(left) == 2, left)
    p.send_signal(sig)
    try:
        p.communicate(timeout=30)
        stopped = True
    except subprocess.TimeoutExpired:
        p.kill()
        stopped = False
    left = subprocess.run(['pgrep', '-f', 'sleep 60 # ' + marker], capture_output=True, text=True).stdout.split()
    check(f'{sig.name}: the run stops, quickly, with no child left', stopped and p.returncode != 0 and not left, (p.returncode, left))
    subprocess.run(['pkill', '-f', 'sleep 60 # ' + marker])

# MUT_WORKERS garbage is a clear error
os.environ['MUT_WORKERS'] = 'abc'
try:
    mutpool.workers()
    check('MUT_WORKERS=abc is refused', False)
except SystemExit as e:
    check('MUT_WORKERS=abc is refused', 'MUT_WORKERS' in str(e.code))
sys.exit(1 if fails else 0)
