#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The computer EQ end to end on a PRIVATE PipeWire: drives the daemon, records, asserts.

Run by run.sh, inside the environment private-pipewire.sh prints; it refuses (exit 2) without it.
It starts the repo-built svolderd against that instance (child manager, no systemctl, fake
device), plays a 1 kHz -20 dBFS tone into the private null sink, and records what the sink emits
AFTER the filter chain: the recorder is not auto-connected but linked by hand to the sink's own
monitor, because a recorder WirePlumber places itself would hear the signal before the chain.

Everything here talks to the instance through the inherited private environment. Nothing plays
until `private-pipewire.sh proof` has passed. Prints one line per measurement and `host-e2e:
PASS` or the first failed assertion (exit 1).
"""
import json
import math
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import wave

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
LAUNCHER = os.path.join(HERE, 'private-pipewire.sh')
SINK = 'svolder_e2e_sink'
CHAIN = 'svolder_eq'
RATE = 48000
TONE_HZ = 1000.0
TONE_DBFS = -20.0
# The worst gap seen for a chain restart is 85 ms (spike); the assertion adds the margin of one
# more block of the graph (42.7 ms).
RELINK_MS = 85.0
GAP_MARGIN_MS = 42.7
# A near-zero run: samples under this peak for this long. 1 ms is 48 samples.
ZERO = 2e-4
# The largest 1 ms peak step allowed while a point is dragged. A live coefficient change leaves a
# one-millisecond dip in the peak that grows with the filter's gain at the tone; the drag keeps the
# tone off the peak (0.5 dB measured), a gap or a click is a step of several dB.
STEP_DB = 1.0


def refuse(why):
    print('host-e2e: refusing to run: ' + why, file=sys.stderr)
    sys.exit(2)


def private_env():
    run = os.environ.get('SVPW_RUN', '')
    want = {
        'XDG_RUNTIME_DIR': run, 'PIPEWIRE_RUNTIME_DIR': run, 'PIPEWIRE_REMOTE': 'svolder-e2e-0',
        'XDG_CONFIG_HOME': run + '/config', 'XDG_STATE_HOME': run + '/state',
        'XDG_DATA_HOME': run + '/data', 'XDG_CACHE_HOME': run + '/cache',
        'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + run + '/no-bus',
    }
    if not (run.startswith('/tmp/svpw.') and len(run) == len('/tmp/svpw.XXXXXX')):
        refuse('SVPW_RUN is not a private-pipewire.sh run directory (run through make host-e2e)')
    for k, v in want.items():
        if os.environ.get(k) != v:
            refuse('%s is not the private instance\'s (%r)' % (k, os.environ.get(k)))
    return run


RUN = private_env()
STATE = {'children': []}


class Fail(Exception):
    pass


# HOST_E2E_BYPASS=1 (a proof that the level checks fail on a chain that does nothing, set by run.sh
# for that proof only) records the tone before the chain and goes on after a failed check.
BYPASS = os.environ.get('HOST_E2E_BYPASS') == '1'
STATE['failed'] = []


def check(ok, what):
    if not ok:
        if BYPASS:
            STATE['failed'].append(what)
            print('  FAIL: ' + what, flush=True)
            return
        raise Fail(what)
    print('  ok: ' + what, flush=True)


def say(line):
    print(line, flush=True)


# ---- processes -----------------------------------------------------------------------------

def spawn(*argv, log=None):
    out = open(log, 'w') if log else subprocess.DEVNULL
    p = subprocess.Popen(list(argv), stdout=out, stderr=subprocess.STDOUT if log else subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True)
    STATE['children'].append(p)
    return p


def stop(p, wait=5):
    if p.poll() is None:
        try:
            os.killpg(p.pid, signal.SIGTERM)
        except OSError:
            pass
        try:
            p.wait(wait)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except OSError:
                pass
            p.wait(3)


def cleanup():
    for p in STATE['children']:
        stop(p)


def tool(*argv, timeout=15):
    return subprocess.run(list(argv), capture_output=True, text=True, timeout=timeout)


def dump():
    r = tool('pw-dump')
    return json.loads(r.stdout)


def node(name, d=None):
    for o in d if d is not None else dump():
        if o['type'].endswith('Node') and (o.get('info') or {}).get('props', {}).get('node.name') == name:
            return o
    return None


def wait_until(what, fn, deadline=15.0, every=0.1):
    t0 = time.monotonic()
    while time.monotonic() - t0 < deadline:
        v = fn()
        if v:
            return v
        time.sleep(every)
    raise Fail('timed out waiting for ' + what)


def settles(fn, deadline=4.0, every=0.1):
    """Whether `fn` turned true within `deadline` seconds (a failed wait is a failed check, not an abort)."""
    t0 = time.monotonic()
    while time.monotonic() - t0 < deadline:
        if fn():
            return True
        time.sleep(every)
    return False


def sink_rate():
    o = node(SINK)
    try:
        return o['info']['params']['Format'][0]['rate']
    except (TypeError, KeyError, IndexError):
        return None


# ---- the daemon's socket -------------------------------------------------------------------

class Conn:
    def __init__(self, path):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(path)
        self.s.settimeout(10)
        self.f = self.s.makefile('rw')
        self.n = 0
        self.late = 0

    def ask(self, request):
        self.n += 1
        i = self.n
        self.f.write(json.dumps({'id': i, 'request': request}) + '\n')
        self.f.flush()
        while True:
            try:
                line = self.f.readline()
            except (socket.timeout, TimeoutError):
                raise Fail('the daemon did not answer %s within 10 s' % json.dumps(request)[:120])
            if not line:
                raise Fail('the daemon closed the socket')
            m = json.loads(line)
            if m.get('msg') == 'response' and m.get('id') == i:
                return m['response']

    def eq(self, op, expect_ack=True):
        r = self.ask({'cmd': 'host_eq', 'op': op})
        if expect_ack and r.get('result') != 'ack':
            raise Fail('host_eq %s answered %s' % (op, r))
        return r

    def view(self):
        r = self.ask({'cmd': 'host'})
        if r.get('result') != 'host':
            raise Fail('host answered %s' % r)
        return r

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


class Watch:
    """A subscription that carries the host's view, which holds the daemon's route reads open: the
    newest route it was sent is `route`, `None` before the first one."""

    def __init__(self, path):
        self.route = None
        self.last = None
        self.seen = threading.Event()
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(path)
        self.f = self.s.makefile('rw')
        self.f.write(json.dumps({'id': 1, 'request': {'cmd': 'subscribe', 'device': None, 'host': True}}) + '\n')
        self.f.flush()
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        try:
            for line in self.f:
                m = json.loads(line).get('event')
                if isinstance(m, dict) and m.get('event') == 'host':
                    self.last = m
                    self.route = m.get('route')
                    self.seen.set()
        except (OSError, ValueError):
            return

    def host_eq(self):
        route = self.route
        return None if route is None else route.get('host_eq')

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


# ---- recording and measuring ---------------------------------------------------------------

def tone_wav(path, secs=240):
    """The tone, written a second at a time: 1 kHz is a whole number of cycles in a second."""
    n = np.arange(RATE)
    a = 10 ** (TONE_DBFS / 20)
    x = (a * np.sin(2 * np.pi * TONE_HZ * n / RATE) * 32767).astype('<i2')
    block = np.stack([x, x], 1).tobytes()
    w = wave.open(path, 'wb')
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(RATE)
    for _ in range(secs):
        w.writeframes(block)
    w.close()


class Recording:
    """A recorder of the sink's own monitor (after the chain), until `finish`. Under
    HOST_E2E_BYPASS it hears the tone stream itself, which is what a chain that did nothing
    would leave."""
    count = 0

    def __init__(self, path=None):
        Recording.count += 1
        self.name = 'svrec%d' % Recording.count
        self.path = path or '%s/rec%d.wav' % (RUN, Recording.count)
        self.p = spawn('pw-record', '--rate', str(RATE), '--channels', '2', '--format', 's16',
                       '-P', 'node.autoconnect=false node.name=' + self.name, self.path)
        wait_until('the recorder\'s ports', lambda: self.name + ':input_FL' in tool('pw-link', '-i').stdout, 8)
        for c in ('FL', 'FR'):
            src = 'svtone:output_%s' % c if BYPASS else '%s:monitor_%s' % (SINK, c)
            r = tool('pw-link', src, '%s:input_%s' % (self.name, c))
            if r.returncode != 0:
                raise Fail('could not link the recorder: ' + r.stderr)
        self.t_stop = None

    def finish(self):
        self.t_stop = time.monotonic()
        stop(self.p)
        w = wave.open(self.path)
        fr = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), '<i2').reshape(-1, 2)[:, 0].astype(float) / 32768
        nz = np.nonzero(np.abs(x) > 1e-4)[0]
        if not len(nz):
            raise Fail('the recording is silent')
        return x[nz[0]:], fr

    def window(self, x, fr, a, b):
        """The level between two monotonic times, counted back from the moment it was stopped."""
        end = len(x) / fr
        return level_db(x, fr, end - (self.t_stop - a), end - (self.t_stop - b))


def level_db(x, fr, t0, t1, f=TONE_HZ):
    seg = x[int(t0 * fr):int(t1 * fr)]
    if len(seg) < 100:
        raise Fail('a level window outside the recording')
    k = np.exp(-2j * np.pi * f * np.arange(len(seg)) / fr)
    return 20 * math.log10(2 * abs((seg * k).sum()) / len(seg) + 1e-12)


def tail_level(x, fr, before_end=2.0, length=1.0):
    """The level over `length` s that ended `before_end` s before the recording did."""
    end = len(x) / fr - before_end
    return level_db(x, fr, end - length, end)


def zero_runs(x, fr, minlen=24):
    """Every near-zero run of at least `minlen` samples, as (start s, ms)."""
    z = np.abs(x) < ZERO
    # run lengths without a python loop per sample
    d = np.diff(np.concatenate(([0], z.astype(np.int8), [0])))
    starts = np.nonzero(d == 1)[0]
    ends = np.nonzero(d == -1)[0]
    return [(round(float(s) / fr, 3), round(float(e - s) / fr * 1e3, 2)) for s, e in zip(starts, ends) if e - s >= minlen]


def longest_zero_ms(x, fr, margin=0.2):
    """The longest near-zero run, ignoring `margin` s at both ends of the recording."""
    a, b = int(margin * fr), len(x) - int(margin * fr)
    runs = zero_runs(x[a:b], fr, 1)
    return max((ms for _, ms in runs), default=0.0), runs


def peak_steps(x, fr, margin=0.2):
    """The largest step in dB between the peaks of two neighbouring milliseconds, and the
    quietest millisecond's peak."""
    x = x[int(margin * fr):len(x) - int(margin * fr)]
    n = int(fr * 0.001)
    m = len(x) // n
    pk = np.abs(x[:m * n]).reshape(m, n).max(axis=1)
    pk = np.maximum(pk, 1e-9)
    d = 20 * np.log10(pk[1:] / pk[:-1])
    big = np.argsort(-np.abs(d))[:3]
    STATE['steps'] = [(round(float(margin + (i + 1) * 0.001), 3), round(float(d[i]), 2)) for i in sorted(big)]
    return float(np.abs(d).max()), float(pk.min())


def timeline(x, fr, step=0.1):
    return [level_db(x, fr, i * step, (i + 1) * step) for i in range(int(len(x) / fr / step) - 1)]


# ---- what the filter should do (the cookbook, written out again here) ------------------------

def biquad_db(kind, f0, gain_db, q, f, fs):
    a_ = 10 ** (gain_db / 40)
    w0 = 2 * math.pi * f0 / fs
    c, s = math.cos(w0), math.sin(w0)
    alpha = s / (2 * q)
    if kind == 'PK':
        b = [1 + alpha * a_, -2 * c, 1 - alpha * a_]
        a = [1 + alpha / a_, -2 * c, 1 - alpha / a_]
    else:
        beta = 2 * math.sqrt(a_) * alpha
        if kind == 'LSC':
            b = [a_ * ((a_ + 1) - (a_ - 1) * c + beta), 2 * a_ * ((a_ - 1) - (a_ + 1) * c),
                 a_ * ((a_ + 1) - (a_ - 1) * c - beta)]
            a = [(a_ + 1) + (a_ - 1) * c + beta, -2 * ((a_ - 1) + (a_ + 1) * c),
                 (a_ + 1) + (a_ - 1) * c - beta]
        else:
            b = [a_ * ((a_ + 1) + (a_ - 1) * c + beta), -2 * a_ * ((a_ - 1) + (a_ + 1) * c),
                 a_ * ((a_ + 1) + (a_ - 1) * c - beta)]
            a = [(a_ + 1) - (a_ - 1) * c + beta, 2 * ((a_ - 1) - (a_ + 1) * c),
                 (a_ + 1) - (a_ - 1) * c - beta]
    z = np.exp(-1j * 2 * math.pi * f / fs)
    h = (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
    return 20 * math.log10(abs(h))


def expected_db(view, fs):
    """What the chain should do to the tone: the points' summed response at 1 kHz plus the
    preamp the daemon says it applied, as a change from the unfiltered level."""
    eq = view['eq']
    return sum(biquad_db(p['kind'], p['f'], p['g'], p['q'], TONE_HZ, fs) for p in eq['points']) + eq['preamp_db']


# ---- the run -------------------------------------------------------------------------------

# What the daemon inherits: a short list, the private variables, and what the script sets. Nothing
# else of the caller's SVOLDER_* reaches it.
KEEP_ENV = ('PATH', 'HOME', 'LANG', 'LC_ALL', 'LC_CTYPE', 'USER', 'LOGNAME', 'TERM', 'SVPW_RUN',
            'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'XDG_STATE_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME',
            'PIPEWIRE_RUNTIME_DIR', 'PIPEWIRE_REMOTE', 'DBUS_SESSION_BUS_ADDRESS', 'DBUS_SYSTEM_BUS_ADDRESS')


def measure(base, seconds=1.0):
    """The tone's level for `seconds` through a recorder of its own, re `base` (a window, not a wait)."""
    rec = Recording()
    time.sleep(seconds + 0.3)
    try:
        x, fr = rec.finish()
    except Fail:
        return None   # a window with nothing in it (a chain restarting): the caller tries again
    return level_db(x, fr, 0.2, len(x) / fr - 0.1) - base


def wait_level(base, want, tol, deadline):
    """Measure until the level is `want` +- tol; (reached, seconds taken, the last level)."""
    t0 = time.monotonic()
    lv = None
    while time.monotonic() - t0 < deadline:
        lv = measure(base, 0.6)
        if lv is not None and abs(lv - want) <= tol:
            return True, time.monotonic() - t0, lv
    return False, time.monotonic() - t0, lv


def main():
    proof = tool('bash', LAUNCHER, 'proof')
    if proof.returncode != 0:
        refuse('proof failed, nothing plays: ' + proof.stdout + proof.stderr)
    say(proof.stdout.strip())
    binary = os.path.join(REPO, 'target/debug/svolderd')
    if not os.access(binary, os.X_OK):
        refuse('no ' + binary + ' (run through make host-e2e, which builds it)')
    say('daemon binary %s (built %s)' % (binary, time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(os.path.getmtime(binary)))))
    if BYPASS:
        say('HOST_E2E_BYPASS: the recorder hears the tone before the chain; the level checks must fail')

    say('load at the start: ' + _load())
    sock = RUN + '/svolder.sock'
    # The daemon reads the route only with a session bus to hold the host on, so it gets a bus of
    # its own in the run directory: no player, no service, and nothing else is given its address
    # (the private PipeWire and WirePlumber keep the unreachable one).
    bus = RUN + '/svolder-bus'
    with open(RUN + '/bus.conf', 'w') as conf:
        conf.write('<busconfig><type>session</type><listen>unix:path=%s</listen><auth>EXTERNAL</auth>'
                   '<policy context="default"><allow send_destination="*" eavesdrop="true"/>'
                   '<allow eavesdrop="true"/><allow own="*"/></policy></busconfig>' % bus)
    spawn('dbus-daemon', '--config-file=' + RUN + '/bus.conf', '--nofork')
    wait_until('the daemon\'s own bus', lambda: os.path.exists(bus), 10)
    env = {k: v for k, v in os.environ.items() if k in KEEP_ENV}
    env.update(SVOLDER_HOST_EQ_MANAGER='child', SVOLDER_SYSTEMCTL='/bin/false',
               SVOLDER_TRANSPORT='fake', SVOLDER_SOCKET=sock, DBUS_SESSION_BUS_ADDRESS='unix:path=' + bus)
    out = open(RUN + '/daemon.log', 'w')
    daemon = subprocess.Popen([binary, '--socket', sock], env=env, stdout=out, stderr=subprocess.STDOUT,
                              stdin=subprocess.DEVNULL, start_new_session=True)
    STATE['children'].append(daemon)
    wait_until('the daemon\'s socket', lambda: os.path.exists(sock) and daemon.poll() is None, 15)
    conn = wait_until('a connection', lambda: _try_conn(sock), 10)
    v = conn.view()
    check(v.get('eq') and v['eq']['unavailable'] is None, 'the daemon serves the computer EQ (%s)' % v['eq'])
    check(v['eq']['on'] is False and not node(CHAIN), 'the EQ starts off and no chain runs (nothing written at start)')

    def point():
        return conn.view()['eq']['points'][0]

    watch = Watch(sock)
    tone_wav(RUN + '/tone.wav')
    spawn('pw-cat', '-p', '--properties', 'node.name=svtone', RUN + '/tone.wav')
    wait_until('the tone', lambda: node('svtone'))

    # A. the unfiltered level (the tone reaches the sink: polled, not waited for)
    t0 = time.monotonic()
    ok, _, _ = wait_level(TONE_DBFS, 0.0, 0.1, 15)
    check(ok, 'the tone reaches the sink at %.1f dBFS +- 0.1 with the EQ off (after %.1f s)' % (TONE_DBFS, time.monotonic() - t0))
    rec = Recording()
    time.sleep(2.0)
    x, fr = rec.finish()
    base = level_db(x, fr, 0.2, len(x) / fr - 0.1)
    stepA, _ = peak_steps(x, fr)
    say('A  EQ off: tone %.3f dB re full scale; max 1 ms peak step %.2f dB' % (base, stepA))
    check(abs(base - TONE_DBFS) <= 0.1, 'the unfiltered tone is %.3f dBFS' % base)

    # B. EQ on with one PK at 1 kHz, 0 dB; the Off -> EQ gap
    rec = Recording()
    time.sleep(0.6)
    conn.eq({'op': 'set', 'index': 1, 'kind': 'PK', 'f': 1000.0, 'g': 0.0, 'q': 1.0})
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    wait_until('the view to say on', lambda: conn.view()['eq']['on'], 5)
    time.sleep(1.6)
    x, fr = rec.finish()
    gap, runs = longest_zero_ms(x, fr)
    lv = rec.window(x, fr, rec.t_stop - 1.2, rec.t_stop - 0.2) - base
    say('B  Off -> EQ: level %+.3f dB re off; near-zero runs >= 0.5 ms: %s' % (lv, zero_runs(x, fr, 24)))
    check(abs(lv) <= 0.1, 'one PK at 1 kHz, 0 dB leaves the level unchanged (%+.3f dB)' % lv)
    limit = RELINK_MS + GAP_MARGIN_MS
    check(gap <= limit, 'the Off -> EQ gap is %.1f ms, within %.0f ms (worst chain restart seen %.0f ms + one block)'
          % (gap, limit, RELINK_MS))
    check(node(CHAIN) is not None, 'the chain\'s node (%s) exists while the EQ is on' % CHAIN)
    # the route the daemon reads says the live chain is in the path (the verdict's `host EQ on`)
    seen_on = settles(lambda: watch.host_eq() is True)
    check(seen_on, 'the daemon\'s route reader sees the live chain (svolder_eq) in the path while the EQ is on (route %s)' % watch.route)

    # C. the drag, the peak off the tone (1.5 kHz), so a working boost and a bypass differ in level:
    # 100 updates at 20/s from 0 to +6 dB, then the last value
    views = []
    polling = threading.Event()
    poll_conn = Conn(sock)

    def poll():
        while not polling.is_set():
            try:
                views.append(poll_conn.view()['eq']['preamp_db'])
            except Exception:
                return
            time.sleep(0.1)

    th = threading.Thread(target=poll, daemon=True)
    rec = Recording()
    time.sleep(0.6)
    conn.eq({'op': 'set', 'index': 1, 'f': 1500.0, 'g': 0.0, 'done': True})
    wait_until('the peak at 1.5 kHz', lambda: point()['f'] == 1500.0, 5)
    time.sleep(0.6)
    chain_before = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    say('C  before the drag: chain node %d, pids %s; load %s' % (chain_before[0], chain_before[1], _load()))
    th.start()
    t0 = time.monotonic()
    late = 0
    for i in range(1, 101):
        due = t0 + i * 0.05
        now = time.monotonic()
        if now < due:
            time.sleep(due - now)
        elif now - due > 0.025:
            late += 1
        conn.eq({'op': 'set', 'index': 1, 'g': 6.0 * i / 100, 'done': False})
    drag_s = time.monotonic() - t0
    conn.eq({'op': 'set', 'index': 1, 'g': 6.0, 'done': True})
    t_done = time.monotonic()
    wait_until('+6 dB in the view', lambda: point()['g'] == 6.0, 5)
    view = conn.view()
    want = expected_db(view, RATE)
    reached, _, final = wait_level(base, want, 0.1, 10)
    polling.set()
    th.join(2)
    poll_conn.close()
    chain_node = node(CHAIN)
    chain_after = (chain_node['id'] if chain_node else None, _private_pids('host-eq.conf'))
    say('C  after the drag: chain node %s, pids %s; load %s' % (chain_after[0], chain_after[1], _load()))
    time.sleep(0.3)
    x, fr = rec.finish()
    check(reached, 'the level settles at the model after the last value')
    final = final if final is not None else float('nan')
    gap, runs = longest_zero_ms(x, fr)
    step, quiet = peak_steps(x, fr)
    t_drag = (t0, t_done)
    during = [rec.window(x, fr, t_drag[0] + k * 0.25, t_drag[0] + k * 0.25 + 0.1) - base for k in range(1, 19)]
    say('C  drag (peak at 1.5 kHz): 100 updates in %.2f s (%.1f/s, %d late); longest near-zero run %.2f ms; max 1 ms peak step %.2f dB (limit %.1f) '
        '(largest (s, dB) %s); net level re off during the drag %.3f .. %.3f dB; final %+.3f dB (model %+.3f); preamp %s' % (
            drag_s, 100 / drag_s, late, gap, step, STEP_DB, STATE['steps'], min(during), max(during), final, want, view['eq']['preamp_db']))
    check(gap <= 1.0, 'no near-zero run over 1 ms during the drag (%.2f ms)' % gap)
    check(step <= STEP_DB, 'no 1 ms peak step over %.1f dB during the drag (%.2f dB; a coefficient change leaves a one-millisecond dip)' % (STEP_DB, step))
    check(point()['g'] == 6.0 and view['eq']['live'] is True, 'the view says +6.0 dB and live')
    check(views and max(views) <= 0.0 and abs(view['eq']['preamp_db'] + 6.0) <= 0.1,
          'the preamp stayed at or under unity through the drag (%d reads, max %.1f dB) and ends at %.1f dB'
          % (len(views), max(views or [0]), view['eq']['preamp_db']))
    # The preamp of this drag is minus the gain, so it never goes below the final one: a deep
    # preamp mid-drag would show as digital silence.
    deepest = view['eq']['preamp_db'] - 0.1
    check(views and min(views) >= deepest,
          'the preamp never went deeper than the final %.1f dB during the drag (lowest read %.1f dB)'
          % (view['eq']['preamp_db'], min(views or [0])))
    check(chain_before == chain_after,
          'the chain was not restarted during the drag (node %s, pids %s before; %s, %s after)'
          % (chain_before[0], chain_before[1], chain_after[0], chain_after[1]))
    check(abs(final - want) <= 0.1, 'the +6 dB peak at 1.5 kHz lands at %+.3f dB at the tone, the model says %+.3f (within 0.1)' % (final, want))
    check(want < -2.0 and final < -2.0, 'a working boost reads %+.2f dB here, far from a bypass at 0 dB' % final)
    check(max(during) <= 0.1 and abs(during[-1] - want) <= 0.3, 'the net level never rose above unity during the drag (max %+.3f dB) and follows the preamp to the end' % max(during))

    # D. +6 dB peak on the tone (net 0 by design), a move to 2 kHz, then a live type change to a low
    # shelf at 700 Hz (about -6 dB at the tone, so it is not a bypass either)
    rec = Recording()
    time.sleep(0.6)
    marks = []
    lv = []
    for op, cond in (
            ({'op': 'set', 'index': 1, 'f': 1000.0, 'done': True}, lambda: point()['f'] == 1000.0),
            ({'op': 'set', 'index': 1, 'f': 2000.0, 'done': True}, lambda: point()['f'] == 2000.0),
            ({'op': 'set', 'index': 1, 'kind': 'LSC', 'f': 700.0, 'done': True}, lambda: point()['kind'] == 'LSC')):
        conn.eq(op)
        wait_until('the view to follow', cond, 5)
        mark = conn.view()
        marks.append((None, None, mark))
        reached, _, level = wait_level(base, expected_db(mark, RATE), 0.15, 10)
        lv.append(level if level is not None else float('nan'))
    time.sleep(0.3)
    x, fr = rec.finish()
    gap, runs = longest_zero_ms(x, fr)
    step, _ = peak_steps(x, fr)
    wants = [expected_db(m[2], RATE) for m in marks]
    say('D  peak on the tone / move to 2 kHz / low shelf: %s dB (model %s); longest near-zero run %.2f ms; max 1 ms peak step %.2f dB' % (
        ['%+.3f' % l for l in lv], ['%+.3f' % w for w in wants], gap, step))
    check(abs(lv[0] - wants[0]) <= 0.1, '+6 dB at 1 kHz with the preamp lands at %+.3f dB (model %+.3f, within 0.1)' % (lv[0], wants[0]))
    check(marks[2][2]['eq']['points'][0]['kind'] == 'LSC', 'the point is now a low shelf')
    check(abs(lv[1] - wants[1]) <= 0.15 and wants[1] < -3.0, 'the moved peak plays as the model says (%+.3f dB)' % lv[1])
    check(abs(lv[2] - wants[2]) <= 0.15 and wants[2] < -3.0, 'the low shelf (700 Hz, +6 dB) plays as the model says (%+.3f dB; a live type change)' % lv[2])
    check(abs(lv[1] - lv[2]) > 1.0, 'the type change is audible at 1 kHz (%.2f dB apart)' % abs(lv[1] - lv[2]))
    check(gap <= 1.0, 'no near-zero run over 1 ms across the move and the type change (%.2f ms)' % gap)

    # E. a forced rate change: the graph restarts, the controller sends every slot again. The peak
    # (1.05 kHz, Q 10) is off the tone, so a missing re-send, a bypass and the right level all differ.
    conn.eq({'op': 'set', 'index': 1, 'kind': 'PK', 'f': 1050.0, 'g': 6.0, 'q': 10.0, 'done': True})
    wait_until('the narrow peak', lambda: point()['q'] == 10.0 and point()['kind'] == 'PK', 5)
    check(sink_rate() == 48000, 'the sink runs at 48000 Hz before the change')
    want48 = expected_db(conn.view(), 48000)
    ok, took, lv48 = wait_level(base, want48, 0.15, 15)
    check(ok and want48 < -1.0, 'at 48000 Hz the narrow peak plays as the model says (%+.3f dB, model %+.3f)' % (lv48 or 0, want48))
    rec = Recording()
    time.sleep(0.6)
    t_force = time.monotonic()
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    check(r.returncode == 0, 'clock.force-rate 44100 set on the private instance')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    seen44 = time.monotonic() - t_force
    want44 = expected_db(conn.view(), 44100)
    ok44, took44, lv44 = wait_level(base, want44, 0.15, 20)
    check(ok44, 'at 44100 Hz the re-sent peak plays as the model says (%+.3f dB, model %+.3f), %.1f s after the change' % (lv44 or 0, want44, took44 + seen44))
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    check(r.returncode == 0, 'clock.force-rate released')
    t_back = time.monotonic()
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    ok48, took48, lv48b = wait_level(base, want48, 0.15, 20)
    check(ok48, 'back at 48000 Hz the re-sent peak plays as the model says (%+.3f dB, model %+.3f), %.1f s after the release' % (lv48b or 0, want48, time.monotonic() - t_back))
    time.sleep(0.5)
    x, fr = rec.finish()
    say('E  rate change: 44100 seen after %.2f s, the right level %.2f s after the change (%+.3f dB, model %+.3f); back at 48000 %+.3f dB '
        '(model %+.3f); near-zero runs >= 0.5 ms: %s' % (seen44, took44 + seen44, lv44 or 0, want44, lv48b or 0, want48, zero_runs(x, fr, 24)))

    # E2. After a live softening, a forced rate change never plays over unity. The graph goes back
    # to the config's coefficients (the hot curve the chain was applied with, a +6 dB peak on the
    # tone); the preamp does not, so it must never have risen past what that curve needs.
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the view to say off', lambda: conn.view()['eq']['on'] is False, 5)
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    conn.eq({'op': 'set', 'index': 1, 'kind': 'PK', 'f': 1000.0, 'g': 6.0, 'q': 1.0, 'done': True})
    wait_until('the view to say on', lambda: conn.view()['eq']['on'] is True, 10)
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    applied = conn.view()
    ok, _, lv_hot = wait_level(base, expected_db(applied, RATE), 0.15, 15)
    check(ok, 'the +6 dB peak on the tone, applied, plays as the model says (%+.3f dB)' % (lv_hot or 0))
    for i in range(1, 21):
        conn.eq({'op': 'set', 'index': 1, 'g': 6.0 * (1 - i / 20), 'done': False})
        time.sleep(0.05)
    conn.eq({'op': 'set', 'index': 1, 'g': 0.0, 'done': True})
    wait_until('0 dB in the view', lambda: point()['g'] == 0.0, 5)
    softened = conn.view()
    ok, _, lv_soft = wait_level(base, expected_db(softened, RATE), 0.15, 10)
    check(ok and lv_soft is not None and lv_soft <= 0.1,
          'the softened curve plays as the model says and not over unity (%+.3f dB, preamp %+.1f dB)'
          % (lv_soft or 0, softened['eq']['preamp_db']))
    rec = Recording()
    time.sleep(0.6)
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    check(r.returncode == 0, 'clock.force-rate 44100 set again')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    ok44, _, lv44s = wait_level(base, expected_db(conn.view(), 44100), 0.15, 20)
    check(ok44, 'at 44100 Hz the softened curve plays as the model says (%+.3f dB)' % (lv44s or 0))
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    check(r.returncode == 0, 'clock.force-rate released again')
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    ok48, _, lv48s = wait_level(base, expected_db(conn.view(), RATE), 0.15, 20)
    check(ok48, 'back at 48000 Hz the softened curve plays as the model says (%+.3f dB)' % (lv48s or 0))
    time.sleep(0.5)
    x, fr = rec.finish()
    tl = [t for t in timeline(x, fr) if t > -100]
    top = max(tl) - base
    say('E2 softened live, forced rate change: highest 0.1 s level %+.3f dB re the unfiltered tone over %.1f s (%d windows); '
        'preamp %+.1f dB (applied %+.1f)' % (top, len(x) / fr, len(tl), softened['eq']['preamp_db'], applied['eq']['preamp_db']))
    check(top <= 0.1, 'no 0.1 s window of the tone plays over unity through the rate changes (highest %+.3f dB)' % top)
    check(softened['eq']['preamp_db'] <= applied['eq']['preamp_db'] + 0.05,
          'the preamp stayed at the applied curve\'s %+.1f dB after the live softening (%+.1f dB)'
          % (applied['eq']['preamp_db'], softened['eq']['preamp_db']))

    # F. Off: the chain goes, the level returns
    rec = Recording()
    time.sleep(0.6)
    conn.eq({'op': 'mode', 'on': False})
    gone = wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    wait_until('the view to say off', lambda: conn.view()['eq']['on'] is False, 5)
    reached, _, rel = wait_level(base, 0.0, 0.1, 10)
    time.sleep(0.3)
    x, fr = rec.finish()
    gap, runs = longest_zero_ms(x, fr)
    lv = (rel if rel is not None else float('nan')) + base
    off = conn.view()
    say('F  Off: level %.3f dB (%+.3f re the unfiltered level); near-zero runs >= 0.5 ms: %s' % (lv, lv - base, zero_runs(x, fr, 24)))
    check(reached, 'the level settles after Off')
    check(gone and node(CHAIN) is None, 'Off removes the chain\'s node from the graph')
    seen_off = settles(lambda: watch.host_eq() is False)
    check(seen_off, 'the daemon\'s route reader sees no chain in the path once the EQ is off (route %s)' % watch.route)
    check(abs(lv - TONE_DBFS) <= 0.1, 'the level is back to %.1f dBFS +- 0.1 after Off' % TONE_DBFS)
    check(gap <= RELINK_MS + GAP_MARGIN_MS, 'the On -> Off gap is %.1f ms, within %.0f ms' % (gap, RELINK_MS + GAP_MARGIN_MS))
    check(off['eq']['on'] is False and len(off['eq']['points']) == 1, 'the view says off and keeps the point')

    # G. volume 50 % and mute, read back from the sink itself
    conn.ask({'cmd': 'host_volume', 'pct': 50})
    r = tool('wpctl', 'get-volume', '@DEFAULT_AUDIO_SINK@').stdout.strip()
    check(r.startswith('Volume: 0.50') and 'MUTED' not in r, 'the sink reads 0.50 (%s)' % r)
    check(conn.view()['volume']['pct'] == 50, 'the view says 50 %')
    conn.ask({'cmd': 'host_mute', 'muted': True})
    r = tool('wpctl', 'get-volume', '@DEFAULT_AUDIO_SINK@').stdout.strip()
    check(r.startswith('Volume: 0.50') and 'MUTED' in r, 'the sink is muted at 0.50 (%s)' % r)
    check(conn.view()['volume']['muted'] is True, 'the view says muted')
    conn.ask({'cmd': 'host_mute', 'muted': False})

    # H. the daemon ending ends the child chain
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    chain_pids = _private_pids('host-eq.conf')
    check(len(chain_pids) == 1, 'one chain process runs while the EQ is on (%s)' % chain_pids)
    conn.ask({'cmd': 'stop'})
    watch.close()
    conn.close()
    try:
        code = daemon.wait(10)
    except subprocess.TimeoutExpired:
        raise Fail('the daemon did not end after stop')
    wait_until('the chain to end', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
    check(code == 0, 'the daemon ended (exit %s)' % code)
    check(node(CHAIN) is None and not _private_pids('host-eq.conf'), 'the daemon ending ended the child chain (process and node gone)')
    if STATE['failed']:
        raise Fail('%d check(s) failed: %s' % (len(STATE['failed']), '; '.join(STATE['failed'])))
    say('host-e2e: PASS')


def _try_conn(sock):
    try:
        c = Conn(sock)
        c.view()
        return c
    except OSError:
        return None


def _load():
    """The machine's load as one line: /proc/loadavg and the CPU pressure stall lines."""
    out = []
    for path in ('/proc/loadavg', '/proc/pressure/cpu'):
        try:
            out.append(open(path).read().strip().replace('\n', ' | '))
        except OSError:
            out.append('%s unreadable' % path)
    return ' ; '.join(out)


def _private_pids(word):
    """Pids carrying this run's marker in their environment, whose command line has `word`."""
    out = []
    for d in os.listdir('/proc'):
        if not d.isdigit():
            continue
        try:
            cmd = open('/proc/%s/cmdline' % d, 'rb').read().replace(b'\0', b' ').decode(errors='replace')
            env = open('/proc/%s/environ' % d, 'rb').read().split(b'\0')
        except OSError:
            continue
        if word in cmd and ('SVPW_RUN=' + RUN).encode() in env:
            out.append(int(d))
    return out


def _on_term(*_):
    raise KeyboardInterrupt


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, _on_term)
    code = 0
    try:
        main()
    except Fail as e:
        print('host-e2e: FAIL: %s' % e, file=sys.stderr)
        code = 1
    except KeyboardInterrupt:
        print('host-e2e: interrupted', file=sys.stderr)
        code = 130
    finally:
        cleanup()
    sys.exit(code)
