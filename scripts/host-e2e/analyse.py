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
import cmath
import json
import math
import os
import re
import signal
import socket
import subprocess
import sys
import threading
import time
import wave

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)  # python -I leaves the script's directory out
import vu  # noqa: E402
REPO = os.path.dirname(os.path.dirname(HERE))
LAUNCHER = os.path.join(HERE, 'private-pipewire.sh')
SINK = 'svolder_e2e_sink'
# what the daemon names the captured sink (its description, from pipewire.conf)
SINK_DESCRIPTION = 'svolder e2e sink'
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
# The crossfeed phases play left-only and mono tones at this level, and every expected level and
# phase comes from the engine's own response functions (the xf_expect example; the virtual speakers'
# phases V0-V11 use vs_expect).
XF_DBFS = -12.0
XF_DB_TOL = 0.2
EXPECT = os.path.join(REPO, 'target/debug/examples/xf_expect')


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
    if not (run.startswith('/tmp/svolder/pw.') and len(run) == len('/tmp/svolder/pw.XXXXXX')):
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

    def finish2(self):
        """Both channels, from the first sample that is not silent in either."""
        self.t_stop = time.monotonic()
        stop(self.p)
        w = wave.open(self.path)
        fr = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), '<i2').reshape(-1, 2)
        nz = np.nonzero(np.abs(x).max(axis=1) > 3)[0]
        if not len(nz):
            raise Fail('the recording is silent')
        return x[nz[0]:, 0].astype(float), x[nz[0]:, 1].astype(float), fr

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

# ---- the crossfeed stage: tones, the oracle, the measurements ----------------------------------

# The table the run ends with: (case, expected, measured).
TABLE = []


def row(case, want, got):
    TABLE.append((case, want, got))
    say('  T  %-58s expected %-24s measured %s' % (case, want, got))


def stereo_wav(path, left_hz, right_hz, dbfs=XF_DBFS, secs=40):
    """A tone on each channel (0 or None is silence), a second at a time: whole cycles for the
    integer frequencies used here. Returns the one-second block's samples (int16, left and right)."""
    n = np.arange(RATE)
    a = 10 ** (dbfs / 20)

    def channel(f):
        if not f:
            return np.zeros(RATE, '<i2')
        return (a * np.sin(2 * np.pi * f * n / RATE) * 32767).astype('<i2')

    block = np.stack([channel(left_hz), channel(right_hz)], 1)
    w = wave.open(path, 'wb')
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(RATE)
    for _ in range(secs):
        w.writeframes(block.tobytes())
    w.close()
    return block


def play(left_hz, right_hz=None):
    """Replace the playing tone with a left-only (or two-channel) one; returns its samples."""
    old = STATE.get('tone')
    if old is not None:
        stop(old)
        wait_until('the old tone to leave', lambda: not node('svtone'), 5)
    path = '%s/xf-tone-%s-%s.wav' % (RUN, left_hz, right_hz)
    block = stereo_wav(path, left_hz, right_hz)
    STATE['tone'] = spawn('pw-cat', '-p', '--properties', 'node.name=svtone', path)
    wait_until('the tone', lambda: node('svtone'))
    return block


def amp(x, fr, f, t0=0.4):
    """The complex amplitude of the tone at `f` Hz in `x`, over whole periods of the sampled
    tone after `t0` s (no leakage)."""
    seg = x[int(t0 * fr):]
    per = fr // math.gcd(int(f), fr)
    n = (len(seg) // per) * per
    if n < per:
        raise Fail('a measurement window shorter than one period of %g Hz' % f)
    k = np.exp(-2j * np.pi * f * np.arange(n) / fr)
    return 2 * (seg[:n] * k).sum() / n


def db_of(c):
    return 20 * math.log10(max(abs(c), 1e-12))


def wrap(a):
    return (a + math.pi) % (2 * math.pi) - math.pi


def oracle(xf, rate, hzs):
    """The engine's complex gains at each frequency for the crossfeed the view describes, at the
    sink's rate: [(same, other, delay_samples)]."""
    if not os.access(EXPECT, os.X_OK):
        raise Fail('no ' + EXPECT + ' (make host-e2e builds it)')
    delay = 'own' if xf['delay_own'] else repr(xf['delay_ms'])
    r = subprocess.run([EXPECT, str(rate), xf['model'], repr(xf['amount_pct']), repr(xf['crossover_hz']),
                        delay] + [repr(float(h)) for h in hzs], capture_output=True, text=True, timeout=30)
    if r.returncode != 0:
        raise Fail('xf_expect failed: ' + r.stderr.strip())
    out = []
    for line in r.stdout.splitlines():
        o = json.loads(line)
        out.append((complex(*o['same']), complex(*o['other']), o['delay_samples']))
    return out


def capture(secs=1.8):
    rec = Recording()
    time.sleep(secs + 0.3)
    return rec.finish2()


def measure_tone(left_hz, right_hz, want_l, want_r, f, rate, deadline=12.0):
    """Record until both channels' complex gains (re the input tone) are the wanted ones, within
    XF_DB_TOL dB (or 2 LSB when the wanted level is under 8 LSB) and one sample of phase between
    the channels; returns (reached, seconds, measured L gain, measured R gain). A left-only tone
    has the right channel as the cross path."""
    t0 = time.monotonic()
    a_in = 10 ** (XF_DBFS / 20)
    lsb = 1.0 / 32768
    got = (None, None)
    while time.monotonic() - t0 < deadline:
        try:
            xl, xr, fr = capture()
        except Fail:
            continue
        cl = amp(xl, fr, f) / 32768 / a_in
        cr = amp(xr, fr, f) / 32768 / a_in
        got = (cl, cr)
        ok = True
        for have, want in ((cl, want_l), (cr, want_r)):
            if abs(want) * a_in < 8 * lsb:
                ok = ok and abs(have - want) * a_in < 2 * lsb
            else:
                ok = ok and abs(db_of(have) - db_of(want)) <= XF_DB_TOL
        if ok and abs(want_l) * a_in >= 8 * lsb and abs(want_r) * a_in >= 8 * lsb:
            one_sample = 2 * math.pi * f / rate + 0.004
            ok = abs(wrap(np.angle(cr / cl) - np.angle(want_r / want_l))) <= one_sample
        if ok:
            return True, time.monotonic() - t0, cl, cr
    return False, time.monotonic() - t0, got[0], got[1]


def bypass_ports_exact(label):
    """The chain's stage ports hold exactly the bypass values (what a chain without crossfeed reads
    back), so the null test is not only a 16-bit one."""
    held = chain_params()
    want = {}
    for c in 'lr':
        want.update({c + 'dir:b0': 1.0, c + 'dir:b1': 0.0, c + 'dir:b2': 0.0, c + 'dir:a1': 0.0, c + 'dir:a2': 0.0,
                     c + 'sum:Gain 1': 1.0, c + 'sum:Gain 2': 0.0, c + 'del:Delay (s)': 0.0, c + 'mix:Gain 2': 0.0})
        for p in ('x', 's'):
            for k in ('b0', 'b1', 'b2', 'a1', 'a2'):
                want['%s%s:%s' % (c, p, k)] = 0.0
    off = {k: (held.get(k), v) for k, v in want.items() if held.get(k) != v}
    check(not off, '%s: the stage ports hold exactly the bypass values (%s)' % (label, off or 'all %d' % len(want)))


def xf_case(conn, label, f, mono=False, left=None):
    """One tone through the chain as it is now (the daemon's view says what it is): the measured
    gains of both channels against the engine's. Returns (measured L, measured R, expected
    same, expected other, oracle delay samples)."""
    view = conn.view()
    rate = sink_rate() or RATE
    pre = view['eq']['preamp_db']
    same, other, delay = oracle(view['eq']['crossfeed'], rate, [f])[0]
    g = 10 ** (pre / 20)
    if mono:
        want_l = want_r = g * (same + other)
        play(f, f)
    else:
        want_l, want_r = g * same, g * other
        play(f, None)
    ok, took, cl, cr = measure_tone(f, f if mono else None, want_l, want_r, f, rate)
    if not ok:
        with open(RUN + '/fail-dump.log', 'w') as fh:
            fh.write(tool('pw-dump').stdout)
        say('  the tone is linked to: %s' % tone_links())
        held = chain_params()
        say('  the stage ports at the failure: %s' % {k: v for k, v in held.items() if k[1:4] in ('mix', 'del', 'sum') or k[1:4] in ('dir', 'x:b', 's:b', 'x:a', 's:a')})
    kind = 'mono' if mono else 'left-only'
    tag = '%s %g Hz' % (kind, f)
    if cl is None:
        check(False, '%s: %s: nothing was recorded' % (label, tag))
        return None
    row('%s, %s: left re input' % (label, tag), '%+.2f dB' % db_of(want_l), '%+.2f dB' % db_of(cl))
    row('%s, %s: right re input' % (label, tag), '%+.2f dB' % db_of(want_r), '%+.2f dB' % db_of(cr))
    if not mono and abs(want_l) > 1e-3 and abs(want_r) > 1e-3 and abs(cl) > 1e-4 and abs(cr) > 1e-4:
        pw = wrap(np.angle(want_r / want_l))
        pm = wrap(np.angle(cr / cl))
        row('%s, %s: right behind left' % (label, tag),
            '%.1f us' % (-pw / (2 * math.pi * f) * 1e6), '%.1f us' % (-pm / (2 * math.pi * f) * 1e6))
    check(ok, '%s, %s: both channels play as the engine says (L %+.2f dB want %+.2f, R %+.2f dB want %+.2f; %.1f s)'
          % (label, tag, db_of(cl), db_of(want_l), db_of(cr), db_of(want_r), took))
    return cl, cr, want_l, want_r, delay


def null_test(label, f, block):
    """The left-only tone `f` through a chain that should do nothing: the recording is the input,
    sample for sample, and the right channel is exactly silent."""
    xl, xr, fr = capture(1.6)
    per = fr // math.gcd(int(f), fr)
    ref = block[:per, 0].astype(float)
    seg = xl[int(0.4 * fr):]
    n = (len(seg) // per) * per
    shift = int(np.argmax(np.fft.irfft(np.fft.rfft(seg[:per]) * np.conj(np.fft.rfft(ref)), per)))
    tiled = np.resize(np.roll(ref, shift), n)
    res_l = np.abs(seg[:n] - tiled).max()
    res_r = np.abs(xr[int(0.4 * fr):]).max()
    row('%s: left minus input' % label, '0 LSB', '%g LSB' % res_l)
    row('%s: right' % label, '0 LSB', '%g LSB' % res_r)
    check(res_l == 0 and res_r == 0,
          '%s: the stage changes nothing, null against the input (peak %g LSB left, %g LSB right; 1 LSB is -90 dBFS)'
          % (label, res_l, res_r))


def chain_params():
    """The chain node's control ports as the graph holds them now: {name: value}."""
    out = {}
    for o in chain_nodes()[:1]:
        for props in (o['info'].get('params') or {}).get('Props', []):
            ps = props.get('params') or []
            out.update(dict(zip(ps[0::2], ps[1::2])))
    return out


def tone_links():
    """The names of the nodes the tone stream is linked to (the sink or the chain)."""
    d = dump()
    names = {o['id']: (o.get('info') or {}).get('props', {}).get('node.name') for o in d if o['type'].endswith('Node')}
    tone = [i for i, n in names.items() if n == 'svtone']
    return sorted({names.get(o['info']['input-node-id']) for o in d if o['type'].endswith('Link')
                   and o['info']['output-node-id'] in tone})


def chain_nodes():
    return [o for o in dump() if o['type'].endswith('Node')
            and (o.get('info') or {}).get('props', {}).get('node.name') == CHAIN]


def chain_playing():
    """Whether the chain's sink node is `running` in the graph (a stream plays through it)."""
    n = chain_nodes()
    return bool(n) and n[0]['info'].get('state') == 'running'


def tone_link_ids():
    """The ids of the links from the tone stream's node."""
    d = dump()
    tone = {o['id'] for o in d if o['type'].endswith('Node') and (o.get('info') or {}).get('props', {}).get('node.name') == 'svtone'}
    return sorted(o['id'] for o in d if o['type'].endswith('Link') and o['info']['output-node-id'] in tone)


def go_quiet():
    """End the tone and wait until nothing plays through the chain (the sink is not running)."""
    old = STATE.pop('tone', None)
    if old is not None:
        stop(old)
    wait_until('the tone to leave', lambda: not node('svtone'), 5)
    wait_until('the chain\'s sink to stop running', lambda: not chain_playing(), 10)


def retone():
    STATE['tone'] = spawn('pw-cat', '-p', '--properties', 'node.name=svtone', RUN + '/tone.wav')
    wait_until('the tone', lambda: node('svtone'))


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


# ---- the virtual speakers stage: the set, the oracle, the measurements --------------------------

VS_EXPECT = os.path.join(REPO, 'target/debug/examples/vs_expect')
VS_LABEL = 'Synthetic room'


def vs_oracle(rate, wav, hzs):
    """The engine's four complex gains at each frequency for the stored WAV, on a sink at `rate`:
    [{'ll', 'lr', 'rl', 'rr'}] (the vs_expect example)."""
    if not os.access(VS_EXPECT, os.X_OK):
        raise Fail('no ' + VS_EXPECT + ' (make host-e2e builds it)')
    r = subprocess.run([VS_EXPECT, str(rate), wav] + [repr(float(h)) for h in hzs],
                       capture_output=True, text=True, timeout=30)
    if r.returncode != 0:
        raise Fail('vs_expect failed: ' + r.stderr.strip())
    return [{k: complex(*o[k]) for k in ('ll', 'lr', 'rl', 'rr')} for o in map(json.loads, r.stdout.splitlines())]


def vs_import(conn, path, label=VS_LABEL):
    """Import the fixture as the person's own set (a job) and wait until the catalogue lists it
    as fetched; returns its entry."""
    r = conn.ask({'cmd': 'speakers', 'op': {'op': 'import', 'path': path, 'label': label, 'room': 'with'}})
    if r.get('result') != 'job':
        raise Fail('the import was answered with %s' % r)

    def found():
        c = conn.ask({'cmd': 'speakers', 'op': {'op': 'catalogue'}})
        return next((s for s in c.get('sets', []) if s.get('label') == label and s.get('fetched')), None)
    return wait_until('the imported set in the catalogue', found, 20)


def vs_wav(set_id):
    """Where the daemon keeps the set's converted WAV (what the convolvers read, and the oracle)."""
    return os.path.join(RUN, 'state', 'svolder', 'speakers', set_id + '.wav')


def chain_conf():
    """The path of the config the running chain process was started with, or None."""
    for pid in _private_pids('host-eq.conf'):
        try:
            args = open('/proc/%d/cmdline' % pid, 'rb').read().decode(errors='replace').split('\0')
        except OSError:
            continue
        for a in args:
            if a.endswith('host-eq.conf'):
                return a
    return None


VS_CONF_OFF = re.compile(r'name = lvs label = mixer control = \{ "Gain 1" = 1 "Gain 2" = 0 "Gain 3" = 0 \}')


def vs_bypass_ok():
    """Both speaker mixers hold the bypass (dry 1, wet 0 and 0) exactly."""
    held = chain_params()
    return all(held.get(m + ':Gain %d' % i) == v for m in ('lvs', 'rvs') for i, v in ((1, 1.0), (2, 0.0), (3, 0.0)))


def vs_bypass_exact(label):
    """Both speaker mixers hold the bypass exactly, with the convolvers loaded."""
    held = chain_params()
    want = {}
    for m in ('lvs', 'rvs'):
        want.update({m + ':Gain 1': 1.0, m + ':Gain 2': 0.0, m + ':Gain 3': 0.0})
    off = {k: (held.get(k), v) for k, v in want.items() if held.get(k) != v}
    check(not off, '%s: the speakers mixers hold exactly the bypass (%s)' % (label, off or 'all %d' % len(want)))


def vs_null(label, f, bypass_ports=True):
    """A left-only tone through a chain that should do nothing, once the tone is linked to the chain (and the
    mixers hold the bypass): the null test."""
    block = play(f)
    check(settles(lambda: tone_links() == [CHAIN] and (not bypass_ports or vs_bypass_ok()), 10.0),
          '%s: the tone is linked to the chain' % label)
    null_test(label, f, block)


def vs_forget_refused(conn, sid, wav, label):
    """Forgetting the set is refused as busy while a chain holds it, and its WAV is still there."""
    r = conn.ask({'cmd': 'speakers', 'op': {'op': 'forget', 'id': sid}})
    check(r.get('result') != 'ack' and r.get('reason') == 'busy' and os.path.exists(wav),
          '%s: forgetting the set is refused as busy and the WAV stays (%s)' % (label, json.dumps(r)[:140]))


def vs_case(conn, label, f, wav, side='left'):
    """One tone through the chain with the virtual speakers on: both ears' complex gains against
    the engine's response of the stored WAV at the sink's rate (side: left, right or mono).
    Returns (measured L, measured R, wanted L, wanted R)."""
    view = conn.view()
    sp = view['eq']['speakers']
    check(sp['on'] is True, '%s: the view says the virtual speakers are on' % label)
    rate = sink_rate() or RATE
    g = 10 ** (view['eq']['preamp_db'] / 20)
    r = vs_oracle(rate, wav, [f])[0]
    if side == 'left':
        want_l, want_r = g * r['ll'], g * r['lr']
        play(f, None)
    elif side == 'right':
        want_l, want_r = g * r['rl'], g * r['rr']
        play(None, f)
    else:
        want_l, want_r = g * (r['ll'] + r['rl']), g * (r['lr'] + r['rr'])
        play(f, f)
    ok, took, cl, cr = measure_tone(f, f if side != 'left' else None, want_l, want_r, f, rate)
    tag = '%s %g Hz' % (side, f)
    if not ok:
        with open(RUN + '/fail-dump.log', 'w') as fh:
            fh.write(tool('pw-dump').stdout)
        say('  the tone is linked to: %s' % tone_links())
        say('  the speakers ports at the failure: %s' % {k: v for k, v in chain_params().items() if k[:3] in ('lvs', 'rvs')})
    if cl is None:
        check(False, '%s: %s: nothing was recorded' % (label, tag))
        return None
    row('%s, %s: left ear re input' % (label, tag), '%+.2f dB' % db_of(want_l), '%+.2f dB' % db_of(cl))
    row('%s, %s: right ear re input' % (label, tag), '%+.2f dB' % db_of(want_r), '%+.2f dB' % db_of(cr))
    if abs(want_l) > 1e-3 and abs(want_r) > 1e-3 and abs(cl) > 1e-4 and abs(cr) > 1e-4:
        pw = wrap(np.angle(want_r / want_l))
        pm = wrap(np.angle(cr / cl))
        row('%s, %s: right ear behind left' % (label, tag),
            '%.1f us' % (-pw / (2 * math.pi * f) * 1e6), '%.1f us' % (-pm / (2 * math.pi * f) * 1e6))
    check(ok, '%s, %s: both ears play as the engine says (L %+.2f dB want %+.2f, R %+.2f dB want %+.2f; %.1f s)'
          % (label, tag, db_of(cl), db_of(want_l), db_of(cr), db_of(want_r), took))
    return cl, cr, want_l, want_r


def cpu_seconds(pid):
    """The user plus system CPU time of a process in seconds (clock ticks, so +-0.01 s)."""
    stat = open('/proc/%d/stat' % pid).read()
    fields = stat[stat.rindex(')') + 2:].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK')


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
    web_sock = socket.socket()
    web_sock.bind(('127.0.0.1', 0))
    web_port = web_sock.getsockname()[1]
    web_sock.close()
    web_token = 'host-e2e-' + os.urandom(12).hex()
    env.update(SVOLDER_HOST_EQ_MANAGER='child', SVOLDER_SYSTEMCTL='/bin/false',
               SVOLDER_TRANSPORT='fake', SVOLDER_SOCKET=sock, SVOLDER_EQ_IDLE_TICKS='60', DBUS_SESSION_BUS_ADDRESS='unix:path=' + bus,
               SVOLDER_SPEAKERS_URL='http://127.0.0.1:9/', SVOLDER_AUTOEQ_URL='http://127.0.0.1:9/',
               # the VU phase reads the levels-only frames over the daemon's web page: a free loopback port
               # of this run (never 8737 or 8738) and a token of its own
               SVOLDER_WEB_ENABLED='1', SVOLDER_WEB_LISTEN='127.0.0.1:%d' % web_port, SVOLDER_WEB_TOKEN=web_token)
    daemons = [0]

    def start_daemon():
        """A daemon of this run (its log is kept per start); returns (process, a connection)."""
        daemons[0] += 1
        out = open('%s/daemon%s.log' % (RUN, '' if daemons[0] == 1 else daemons[0]), 'w')
        p = subprocess.Popen([binary, '--socket', sock], env=env, stdout=out, stderr=subprocess.STDOUT,
                             stdin=subprocess.DEVNULL, start_new_session=True)
        STATE['children'].append(p)
        c = wait_until('a connection', lambda: _try_conn(sock) if p.poll() is None else Fail('the daemon ended'), 15)
        if isinstance(c, Fail):
            raise c
        return p, c

    daemon, conn = start_daemon()
    v = conn.view()
    check(v.get('eq') and v['eq']['unavailable'] is None, 'the daemon serves the computer EQ (%s)' % v['eq'])
    check(v['eq']['on'] is False and not node(CHAIN), 'the EQ starts off and no chain runs (nothing written at start)')

    def point():
        return conn.view()['eq']['points'][0]

    watch = Watch(sock)
    tone_wav(RUN + '/tone.wav')
    STATE['tone'] = spawn('pw-cat', '-p', '--properties', 'node.name=svtone', RUN + '/tone.wav')
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
    old_id = node(CHAIN)['id']
    go_quiet()   # a chain nothing plays through is removed at once (with a stream it stays as a pass-through)
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the view to say off', lambda: conn.view()['eq']['on'] is False, 5)
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    retone()
    conn.eq({'op': 'set', 'index': 1, 'kind': 'PK', 'f': 1000.0, 'g': 6.0, 'q': 1.0, 'done': True})
    wait_until('the view to say on', lambda: conn.view()['eq']['on'] is True, 10)
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    new_id = node(CHAIN)['id']
    say('E2 the chain\'s node id: %d before the chain was taken away, %d after it started again' % (old_id, new_id))
    check(old_id != new_id,
          'a chain started again is a node under a new id (%d, then %d): the controller tells a restart by its id' % (old_id, new_id))
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

    # E3. 16 points: a live update carries every slot (about 5 KB a line). The drag of one point
    # reaches the chain whole and the chain is not restarted.
    for i in range(15):
        conn.eq({'op': 'add', 'kind': 'PK', 'f': 120.0 * 1.4 ** i, 'g': 0.5 if i % 2 else -0.5, 'q': 1.0})
    wait_until('16 points', lambda: len(conn.view()['eq']['points']) == 16, 5)
    chain16 = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    # The drag moves the point under the tone (1 kHz), by -3 dB: that is a 3 dB change at the tone,
    # ten times the tolerance, and a cut leaves the preamp where E2 capped it. The level is
    # read before the drag and after it: a line that is cut or refused leaves the chain at the
    # first level, and the step fails, where the model alone (which follows the view) would not.
    tone_at = min(range(len(conn.view()['eq']['points'])),
                  key=lambda k: abs(conn.view()['eq']['points'][k]['f'] - TONE_HZ)) + 1
    g0 = conn.view()['eq']['points'][tone_at - 1]['g']
    ok0, _, lv_before = wait_level(base, expected_db(conn.view(), RATE), 0.15, 15)
    check(ok0, 'with 16 points the chain plays as the model says before the drag (%+.3f dB)' % (lv_before or 0))
    for i in range(1, 41):
        conn.eq({'op': 'set', 'index': tone_at, 'g': g0 - 3.0 * i / 40, 'done': False})
        time.sleep(0.05)
    conn.eq({'op': 'set', 'index': tone_at, 'g': g0 - 3.0, 'done': True})
    wait_until('-3 dB in the view', lambda: abs(conn.view()['eq']['points'][tone_at - 1]['g'] - (g0 - 3.0)) < 0.01, 5)
    full = conn.view()
    ok, _, lv16 = wait_level(base, expected_db(full, RATE), 0.15, 15)
    moved = (lv16 or 0) - (lv_before or 0)
    check(ok and full['eq']['live'] is True and abs(moved + 3.0) <= 0.3,
          'with 16 points the drag lands as the model says (%+.3f dB, model %+.3f), the level moved %+.3f dB '
          '(about -3) and the view says live' % (lv16 or 0, expected_db(full, RATE), moved))
    after16 = node(CHAIN)
    check(after16 is not None and (after16['id'], _private_pids('host-eq.conf')) == chain16,
          'the chain was not restarted during the 16-point drag')
    for _ in range(15):
        conn.eq({'op': 'remove', 'index': 2})
    wait_until('one point again', lambda: len(conn.view()['eq']['points']) == 1, 5)
    conn.eq({'op': 'set', 'index': 1, 'g': 0.0, 'done': True})

    # F. Off: the chain goes, the level returns
    rec = Recording()
    time.sleep(0.6)
    conn.eq({'op': 'mode', 'on': False})
    f_id, f_links = node(CHAIN)['id'], tone_link_ids()
    wait_until('the view to say off', lambda: conn.view()['eq']['on'] is False, 5)
    # With the tone playing the chain stays as a flat pass-through: the same node, the same links.
    time.sleep(1.0)
    check(node(CHAIN) is not None and node(CHAIN)['id'] == f_id and tone_link_ids() == f_links,
          'F Off with a stream playing keeps the chain (node %s) and the tone\'s links %s (now %s)' % (f_id, f_links, tone_link_ids()))
    check(conn.view()['eq']['note'] == 'chain kept as a pass-through until idle', 'F the view says the chain is kept (%r)' % conn.view()['eq']['note'])
    # The pass-through plays flat under the preamp the chain was loaded with (a rate change puts the
    # slots back to that config, so it never goes above what that needs): here the E-phase curve's -6 dB.
    held_db = conn.view()['eq']['preamp_db']
    say('F the pass-through preamp: %+.2f dB' % held_db)
    reached, _, rel = wait_level(base, held_db, 0.1, 10)
    time.sleep(0.3)
    x, fr = rec.finish()
    gap, runs = longest_zero_ms(x, fr)
    lv = (rel if rel is not None else float('nan')) + base
    off = conn.view()
    say('F  Off: level %.3f dB (%+.3f re the unfiltered level); near-zero runs >= 0.5 ms: %s' % (lv, lv - base, zero_runs(x, fr, 24)))
    check(reached, 'the level settles at the pass-through\'s preamp (%+.2f dB) after Off' % held_db)
    # Idle (the tone ends): the chain goes after the idle time (3 s here).
    go_quiet()
    gone = wait_until('the pass-through chain to go once idle', lambda: node(CHAIN) is None, 12)
    check(gone and node(CHAIN) is None, 'F the pass-through chain is removed once nothing plays through it')
    check(settles(lambda: conn.view()['eq']['note'] is None), 'F the note is gone with the chain (%r)' % conn.view()['eq']['note'])
    retone()
    seen_off = settles(lambda: watch.host_eq() is False)
    check(seen_off, 'the daemon\'s route reader sees no chain in the path once the EQ is off (route %s)' % watch.route)
    back, _, lv0 = wait_level(base, 0.0, 0.1, 10)
    check(back, 'the level is back to %.1f dBFS +- 0.1 once the chain is gone (%s)' % (TONE_DBFS, lv0))
    check(gap <= RELINK_MS + GAP_MARGIN_MS, 'the On -> Off gap is %.1f ms, within %.0f ms' % (gap, RELINK_MS + GAP_MARGIN_MS))
    check(off['eq']['on'] is False and len(off['eq']['points']) == 1, 'the view says off and keeps the point')

    # X1. crossfeed off, the EQ on (one flat point): the stage is in the chain and does nothing.
    # A left-only tone comes out as it went in, sample for sample, and the right channel is silent.
    xfv = conn.view()['eq'].get('crossfeed')
    check(xfv is not None and xfv['on'] is False and xfv['model'] == 'bauer',
          'the view carries the crossfeed, off, on Bauer (%s)' % (xfv and {k: xfv[k] for k in ('on', 'model', 'preset')}))
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    wait_until('the view to say on', lambda: conn.view()['eq']['on'], 5)
    chain_x = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    check(conn.view()['eq']['preamp_db'] == 0.0 and not conn.view()['eq']['crossfeed']['on'],
          'the EQ is flat, the preamp 0 dB and the crossfeed off')
    block = play(1000)
    time.sleep(1.0)
    null_test('X1 bypass (EQ on, crossfeed off) left-only 1 kHz', 1000, block)
    bypass_ports_exact('X1')
    if os.environ.get('HOST_E2E_PWDUMP_DIR'):
        with open(os.path.join(os.environ['HOST_E2E_PWDUMP_DIR'], 'live-chain-crossfeed.json'), 'w') as fh:
            fh.write(tool('pw-dump').stdout)

    # X2. crossfeed on: Bauer, bs2b default, at the sink's rate. Every expected number is the
    # engine's response (xf_expect) at the tone, with the preamp the view says.
    conn.eq({'op': 'crossfeed', 'on': True})
    xv = conn.view()['eq']['crossfeed']
    check(xv['on'] is True and xv['model'] == 'bauer' and xv['preset'] == 'bs2b default'
          and abs(xv['feed_db'] - 4.5) < 1e-6,
          'crossfeed on plays Bauer, bs2b default (feed %.2f dB, crossover %g Hz, own lag %.3f ms)'
          % (xv['feed_db'], xv['crossover_hz'], xv['delay_floor_ms']))
    r100 = xf_case(conn, 'X2 Bauer default', 100)
    r5k = xf_case(conn, 'X2 Bauer default', 5000)
    own_phase = np.angle(r100[1] / r100[0])
    cross100 = db_of(r100[1] / r100[0])
    cross5k = db_of(r5k[1] / r5k[0])
    check(abs(cross100 + xv['feed_db']) <= 1.0,
          'at 100 Hz the right channel sits %.2f dB re the left, near the feed level -%.1f dB (the 700 Hz low pass takes a little)'
          % (cross100, xv['feed_db']))
    check(cross5k < cross100 - 15.0, 'at 5 kHz the right channel is far lower (%.1f dB re the left, against %.1f at 100 Hz)'
          % (cross5k, cross100))
    for f in (20, 100, 1000, 5000):
        got = xf_case(conn, 'X2 Bauer default', f, mono=True)
        check(got is not None and db_of(got[0]) <= 0.1 and db_of(got[1]) <= 0.1,
              'a mono %g Hz tone plays at %+.2f / %+.2f dB re the input, never above it (the preamp rule)'
              % (f, db_of(got[0]), db_of(got[1])) if got else 'mono tone recorded')
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_x,
          'turning crossfeed on did not restart the chain (node %d, pids %s)' % chain_x)

    # X2b. the delay, live: 0.60 ms in total
    conn.eq({'op': 'crossfeed', 'delay_ms': 0.6})
    xv = conn.view()['eq']['crossfeed']
    check(abs(xv['delay_ms'] - 0.6) < 1e-9 and xv['delay_own'] is False, 'the view says a total lag of %.2f ms' % xv['delay_ms'])
    rd = xf_case(conn, 'X2b Bauer delay 0.60 ms', 100)
    rate = sink_rate() or RATE
    added = -wrap(np.angle(rd[1] / rd[0]) - own_phase) / (2 * math.pi * 100 / rate)
    total_ms = xv['delay_floor_ms'] + added * 1000 / rate
    row('X2b total lag at 100 Hz (own %.3f ms + added)' % xv['delay_floor_ms'], '0.60 ms (+0, -1 sample)',
        '%.3f ms (%.1f samples added, engine %d)' % (total_ms, added, rd[4]))
    check(0.6 - 1.3 * 1000 / rate <= total_ms <= 0.6 + 0.3 * 1000 / rate,
          'the total lag is %.3f ms for a 0.60 ms ask, within one sample (%.3f ms), live' % (total_ms, 1000 / rate))
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_x, 'the delay change did not restart the chain')

    # X2c. M/S bass, 50 % at 120 Hz: the bass goes to both channels, the highs stay put
    conn.eq({'op': 'crossfeed', 'model': 'mid-side'})
    xv = conn.view()['eq']['crossfeed']
    check(xv['model'] == 'mid-side' and abs(xv['amount_pct'] - 50.0) < 1e-6 and xv['crossover_hz'] == 120.0,
          'M/S bass loads its default: %g %% at %g Hz' % (xv['amount_pct'], xv['crossover_hz']))
    ms20 = xf_case(conn, 'X2c M/S bass', 20)
    xf_case(conn, 'X2c M/S bass', 120)
    ms2k = xf_case(conn, 'X2c M/S bass', 2000)
    check(ms20 is not None and db_of(ms20[0]) > -12 and db_of(ms20[1]) > -12 and abs(db_of(ms20[0]) - db_of(ms20[1])) < 1.5,
          'a left-only 20 Hz tone comes out on both channels (%+.2f / %+.2f dB re the input)'
          % (db_of(ms20[0]), db_of(ms20[1])) if ms20 else 'M/S 20 Hz recorded')
    pre = conn.view()['eq']['preamp_db']
    check(ms2k is not None and abs(db_of(ms2k[0]) - pre) <= 0.2 and db_of(ms2k[1]) < -45,
          'a left-only 2 kHz tone stays on the left, unchanged but for the preamp (%+.2f dB left, preamp %+.2f dB; %.1f dB right)'
          % (db_of(ms2k[0]), pre, db_of(ms2k[1])) if ms2k else 'M/S 2 kHz recorded')
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_x, 'the model change did not restart the chain')

    # X2e. a change sent while nothing plays (the chain idle): it holds when the sound starts again
    conn.eq({'op': 'crossfeed', 'model': 'bauer'})
    old = STATE.pop('tone')
    stop(old)
    wait_until('the tone to leave', lambda: not node('svtone'), 5)
    time.sleep(1.5)
    conn.eq({'op': 'crossfeed', 'amount_pct': 30.0})
    xf_case(conn, 'X2e Bauer 30 %, set while idle', 100)

    # X2d. back to off with the chain still running: nothing left of the stage
    conn.eq({'op': 'crossfeed', 'model': 'bauer'})
    conn.eq({'op': 'crossfeed', 'on': False})
    check(conn.view()['eq']['crossfeed']['on'] is False, 'the view says crossfeed off')
    block = play(100)
    time.sleep(1.0)
    null_test('X2d after crossfeed off (EQ on) left-only 100 Hz', 100, block)
    bypass_ports_exact('X2d')

    # S1. crossfeed turned on with the EQ off and NOTHING playing: the chain has not run, and PipeWire
    # lists its controls at 0 (lpre:Mult too) until it does. The first tone plays at the preamp the
    # view says, never the -120 dB floor, and the chain holds that preamp once it runs.
    def idle_look(label):
        held = chain_params()
        say('%s: lpre:Mult %s, l01:b0 %s, %d of %d controls not 0, node %s'
            % (label, held.get('lpre:Mult'), held.get('l01:b0'), sum(1 for x in held.values() if x != 0),
               len(held), (node(CHAIN) or {}).get('info', {}).get('state')))

    def plays_at_the_view(label):
        v = conn.view()
        pre = v['eq']['preamp_db']
        mult = chain_params().get('lpre:Mult')
        held = None if mult is None else 20 * math.log10(max(mult, 1e-12))
        row(label, 'the view\'s preamp, above -20 dB, held by the chain', '%+.2f dB, lpre:Mult %s' % (pre, mult))
        check(v['eq']['live'] and pre > -20.0 and held is not None and abs(held - pre) <= 0.01,
              '%s: the chain plays at the view\'s preamp %+.2f dB (lpre:Mult %s), live %s (%r)'
              % (label, pre, mult, v['eq']['live'], v['eq']['note']))

    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    conn.eq({'op': 'crossfeed', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    idle_look('S1 the chain as it started, nothing playing')
    time.sleep(2.0)
    v = conn.view()
    check(v['eq']['preamp_db'] > -20.0 and v['eq']['live'],
          'S1: crossfeed on over a chain that has not run: the view\'s preamp is %+.2f dB, live %s (%r)'
          % (v['eq']['preamp_db'], v['eq']['live'], v['eq']['note']))
    xf_case(conn, 'S1 crossfeed on with nothing playing, then a tone', 1000)
    plays_at_the_view('S1 once the tone plays')
    go_quiet()
    conn.eq({'op': 'crossfeed', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)

    # S2. the EQ turned on with nothing playing, then a live edit, then a tone: the edit plays at
    # its own preamp
    conn.eq({'op': 'set', 'index': 1, 'kind': 'PK', 'f': TONE_HZ, 'g': 0.0, 'q': 1.0})
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    idle_look('S2 the chain as it started, nothing playing')
    time.sleep(2.0)
    conn.eq({'op': 'set', 'index': 1, 'g': -3.0, 'done': True})
    time.sleep(1.0)
    v = conn.view()
    check(v['eq']['preamp_db'] > -20.0 and v['eq']['live'],
          'S2: an edit over a chain that has not run: the view\'s preamp is %+.2f dB, live %s (%r)'
          % (v['eq']['preamp_db'], v['eq']['live'], v['eq']['note']))
    retone()
    want = expected_db(conn.view(), sink_rate() or RATE)
    ok, took, lv = wait_level(base, want, 0.2, 10)
    row('S2 EQ on idle, -3 dB PK on the tone, then a tone', '%+.2f dB' % want,
        'nan' if lv is None else '%+.2f dB' % lv)
    check(ok, 'S2: the tone plays at %+.2f dB re off once it starts (%s dB after %.1f s)' % (want, lv, took))
    plays_at_the_view('S2 once the tone plays')
    conn.eq({'op': 'set', 'index': 1, 'g': 0.0, 'done': True})

    # S3. a pause long enough for the chain to be suspended, crossfeed on: the chain that ran keeps
    # listing what it holds, the EQ stays live with the stage on, and the sound comes back as it was
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    retone()
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    conn.eq({'op': 'crossfeed', 'on': True})
    xf_case(conn, 'S3 crossfeed on, before a pause', 1000)
    go_quiet()
    wait_until('the chain to be suspended',
               lambda: (node(CHAIN) or {}).get('info', {}).get('state') == 'suspended', 20)
    time.sleep(6.0)
    idle_look('S3 suspended after a pause')
    v = conn.view()
    check(v['eq']['live'] and v['eq']['crossfeed']['on'],
          'S3: a suspended chain keeps the EQ live and crossfeed on (%r)' % (v['eq']['crossfeed']['note'] or v['eq']['note']))
    xf_case(conn, 'S3 crossfeed on, after the pause', 1000)
    conn.eq({'op': 'crossfeed', 'on': False})

    # X3a. crossfeed on with the EQ off: one chain for the crossfeed alone, the EQ flat
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    retone()   # the chain is applied with a stream playing, as it always was here
    conn.eq({'op': 'crossfeed', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    v = conn.view()
    check(len(chain_nodes()) == 1 and len(_private_pids('host-eq.conf')) == 1,
          'crossfeed on with the EQ off runs one chain (%d node, %d process)' % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    check(v['eq']['on'] is False and v['eq']['crossfeed']['on'] is True and len(v['eq']['points']) == 1
          and v['eq']['points'][0]['g'] == 0.0,
          'the view says the EQ off, crossfeed on, the EQ points untouched (%s)' % v['eq']['points'])
    xf_case(conn, 'X3a EQ off, crossfeed on (flat chain)', 100)
    xf_case(conn, 'X3a EQ off, crossfeed on (flat chain)', 1000)
    # X3b. both off with nothing playing: no chain, no process
    go_quiet()
    conn.eq({'op': 'crossfeed', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    wait_until('the chain\'s process to end', lambda: not _private_pids('host-eq.conf'), 10)
    v = conn.view()
    check(v['eq']['on'] is False and v['eq']['crossfeed']['on'] is False and not chain_nodes()
          and not _private_pids('host-eq.conf'),
          'both off: no svolder_eq node, no chain process (and no unit: systemctl is /bin/false)')

    # X5. the last reason for the chain goes while a stream plays: the chain stays as a pass-through
    # (same node, same process, the stream's links untouched, no new sink), flat at 0 dB; the idle
    # time (3 s here) after nothing plays it goes; a capture of the chain's own monitor is not a stream that plays.
    # (A paused stream is pinned by the fixtures of props_of: no tool of the run can pause a stream; pw-cat
    # stays `running` under SIGSTOP and ignores a Pause command.)
    for how in ('ended', 'monitor capture', 'passive monitor capture'):
        play(100)
        conn.eq({'op': 'crossfeed', 'on': True})
        wait_until('the chain\'s node', lambda: node(CHAIN), 10)
        wait_until('the tone to play through the chain', chain_playing, 10)
        ids = (node(CHAIN)['id'], _private_pids('host-eq.conf'), tone_link_ids())
        sinks = sorted(o['id'] for o in dump() if o['type'].endswith('Node') and (o.get('info') or {}).get('props', {}).get('media.class') == 'Audio/Sink')
        check(tone_links() == [CHAIN], 'X5 (%s) the tone is linked to the chain only (%s)' % (how, tone_links()))
        conn.eq({'op': 'crossfeed', 'on': False})
        time.sleep(1.5)
        now = (node(CHAIN)['id'] if node(CHAIN) else None, _private_pids('host-eq.conf'), tone_link_ids())
        sinks_now = sorted(o['id'] for o in dump() if o['type'].endswith('Node') and (o.get('info') or {}).get('props', {}).get('media.class') == 'Audio/Sink')
        check(now == ids and sinks_now == sinks,
              'X5 (%s) crossfeed off keeps the chain node, its process, the stream\'s links and the sinks: %s -> %s' % (how, ids, now))
        v = conn.view()
        check(v['eq']['on'] is False and v['eq']['crossfeed']['on'] is False and v['eq']['note'] == 'chain kept as a pass-through until idle',
              'X5 (%s) the view says the chain is kept (%r)' % (how, v['eq']['note']))
        pp = chain_params()
        check(abs(pp.get('lmix:Gain 2', 1.0)) < 1e-9 and abs(pp.get('lpre:Mult', 0.0) - 1.0) < 1e-6,
              'X5 (%s) the pass-through is flat at 0 dB (stage gain %s, preamp mult %s)' % (how, pp.get('lmix:Gain 2'), pp.get('lpre:Mult')))
        cap = None
        if how.endswith('monitor capture'):
            # The stream ends and a capture of the chain's own monitor goes on (non-passive keeps the
            # sink running, the way a visualiser or a screen-share does): nothing plays.
            props = 'node.autoconnect=false node.name=svmoncap' + (' node.passive=true' if how.startswith('passive') else '')
            cap = spawn('pw-record', '--rate', str(RATE), '--channels', '2', '--format', 's16', '-P', props, RUN + '/moncap.wav')
            wait_until('the capture\'s ports', lambda: 'svmoncap:input_FL' in tool('pw-link', '-i').stdout, 8)
            for c in ('FL', 'FR'):
                check(tool('pw-link', '%s:monitor_%s' % (CHAIN, c), 'svmoncap:input_%s' % c).returncode == 0, 'X5 the capture is linked to the chain\'s monitor %s' % c)
            stop(STATE.pop('tone'))
            say('X5 (%s) the chain sink state with only the capture: %s' % (how, chain_nodes()[0]['info'].get('state')))
        elif how == 'ended':
            stop(STATE.pop('tone'))
        gone = settles(lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 15.0)
        say('X5 (%s) the chain after the idle time: %s' % (how, 'gone' if gone else 'still there (sink state %s)' % (chain_nodes()[0]['info'].get('state') if chain_nodes() else None)))
        check(gone, 'X5 (%s) the pass-through chain is removed once nothing plays through it' % how)
        if cap is not None:
            stop(cap)
        if how != 'ended':
            go_quiet()

    # X3c. a rate change with the stage on: it plays again at the new rate, the same levels
    retone()
    conn.eq({'op': 'mode', 'on': True})
    conn.eq({'op': 'crossfeed', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    at48 = xf_case(conn, 'X3c rate 48000 Hz', 100)
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    check(r.returncode == 0, 'clock.force-rate 44100 set on the private instance')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    v = conn.view()
    check(v['eq']['crossfeed']['on'] is True, 'crossfeed is still on after the rate change (note %r)' % v['eq']['crossfeed']['note'])
    at44 = xf_case(conn, 'X3c rate 44100 Hz', 100)
    xf_case(conn, 'X3c rate 44100 Hz', 1000)
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    check(r.returncode == 0, 'clock.force-rate released')
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    at48b = xf_case(conn, 'X3c back at 48000 Hz', 100)
    if at48 and at44 and at48b:
        for name, a, b in (('left', at48[0], at44[0]), ('right', at48[1], at44[1]), ('right, back', at48[1], at48b[1])):
            row('X3c %s channel, 100 Hz, level across the rate change' % name, '%+.2f dB' % db_of(a), '%+.2f dB' % db_of(b))
            check(abs(db_of(a) - db_of(b)) <= 0.3, 'the %s channel keeps its level across the rate change (%+.2f, %+.2f dB)'
                  % (name, db_of(a), db_of(b)))

    # X3e. the chain's process dies with crossfeed on: the daemon sees it, starts the chain again once with
    # crossfeed off and its note; a second death right after is ridden out too, a third leaves the EQ off with a note;
    # never a stacked node
    pids = _private_pids('host-eq.conf')
    check(len(pids) == 1, 'one chain process before it is killed (%s)' % pids)
    os.kill(pids[0], signal.SIGKILL)
    t_kill = time.monotonic()
    restarted = None
    while time.monotonic() - t_kill < 10.0:
        v = conn.view()
        xv = v['eq']['crossfeed']
        new_pids = _private_pids('host-eq.conf')
        if not xv['on'] and len(new_pids) == 1 and new_pids != pids and chain_nodes():
            restarted = v
            break
        time.sleep(0.1)
    took = time.monotonic() - t_kill
    v = restarted or v
    xv = v['eq']['crossfeed']
    row('X3e the view after the chain process was killed', 'chain started again within ~2 s, crossfeed off with its note',
        'eq on %s live %s, crossfeed on %s, off_reason %s, %d chain node, %d process, after %.1f s' % (
            v['eq']['on'], v['eq']['live'], xv['on'], xv['off_reason'], len(chain_nodes()),
            len(_private_pids('host-eq.conf')), took))
    check(restarted is not None and took <= 4.0 and v['eq']['on'] and v['eq']['live'] and xv['off_reason'] == 'chain-restarted'
          and xv['note'], 'the killed chain was started again %.1f s after the kill with crossfeed off (%r)' % (took, xv['note']))
    check(len(chain_nodes()) == 1 and len(_private_pids('host-eq.conf')) == 1,
          'one chain after the restart (%d node, %d process)' % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    # it dies again right after (PipeWire restarting does this once): started again; a third death in a row is a
    # crash loop: the EQ is left off, with a note, and nothing is left running
    time.sleep(1.0)
    pids = _private_pids('host-eq.conf')
    os.kill(pids[0], signal.SIGKILL)
    wait_until('the chain started again after the second kill',
               lambda: len(_private_pids('host-eq.conf')) == 1 and _private_pids('host-eq.conf') != pids and chain_nodes(), 10)
    settles(lambda: conn.view()['eq']['on'] and conn.view()['eq']['live'], 6.0)
    v = conn.view()
    check(v['eq']['on'] and v['eq']['live'], 'one quick death is ridden out: the EQ is on and live (%r)' % v['eq']['note'])
    time.sleep(1.0)
    os.kill(_private_pids('host-eq.conf')[0], signal.SIGKILL)
    t_kill = time.monotonic()
    while time.monotonic() - t_kill < 10.0:
        v = conn.view()
        if not v['eq']['on']:
            break
        time.sleep(0.1)
    took = time.monotonic() - t_kill
    time.sleep(1.0)
    row('X3e the view after the chain died three times in a row', 'EQ off with a note, no chain',
        'eq on %s live %s, note %r, %d chain node, %d process, after %.1f s' % (
            v['eq']['on'], v['eq']['live'], v['eq']['note'], len(chain_nodes()), len(_private_pids('host-eq.conf')), took))
    check(not v['eq']['on'] and not v['eq']['live'] and v['eq']['note'] and 'chain stopped' in v['eq']['note'],
          'the EQ is off with a note that the chain stopped (%r)' % v['eq']['note'])
    check(len(chain_nodes()) == 0 and len(_private_pids('host-eq.conf')) == 0,
          'no chain runs after the third death (%d node, %d process)' % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    # the EQ off and on again builds one chain, with crossfeed off
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    time.sleep(1.0)
    check(len(chain_nodes()) == 1 and len(_private_pids('host-eq.conf')) == 1 and not conn.view()['eq']['crossfeed']['on'],
          'one chain after the EQ is turned off and on, crossfeed off (%d node, %d process)'
          % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    conn.eq({'op': 'crossfeed', 'on': True})

    # X3d. the default sink moves to a second null sink: crossfeed turns itself off, with its note
    r = tool('pw-cli', 'create-node', 'adapter',
             '{ factory.name=support.null-audio-sink node.name=svolder_e2e_sink2 '
             'node.description="svolder e2e sink 2" media.class=Audio/Sink audio.position=[ FL FR ] '
             'object.linger=true priority.session=500 '
             'adapter.auto-port-config={ mode=dsp monitor=true control=false position=preserve } }')
    check(r.returncode == 0, 'a second null sink was made in the private instance (%s)' % (r.stdout + r.stderr).strip()[:100])
    sink2 = wait_until('the second sink', lambda: node('svolder_e2e_sink2'), 8)
    sink1 = node(SINK)['id']
    check(conn.view()['eq']['crossfeed']['on'] is True, 'crossfeed is on before the switch')
    t_switch = time.monotonic()
    r = tool('wpctl', 'set-default', str(sink2['id']))
    check(r.returncode == 0, 'the default sink is now the second one (%s)' % r.stderr.strip()[:100])
    xv = None
    while time.monotonic() - t_switch < 6.0:
        xv = conn.view()['eq']['crossfeed']
        if not xv['on']:
            break
        time.sleep(0.05)
    took = time.monotonic() - t_switch
    row('X3d crossfeed off after the default sink moved', 'off within 2 s, with a note',
        '%s after %.2f s, off_reason %s, note %r' % ('off' if not xv['on'] else 'STILL ON', took, xv['off_reason'], xv['note']))
    check(not xv['on'] and took <= 2.0 and xv['note'] and xv['off_reason'] == 'output-changed',
          'crossfeed turned itself off %.2f s after the default sink moved, with the note %r' % (took, xv['note']))
    tool('wpctl', 'set-default', str(sink1))
    tool('pw-cli', 'destroy', str(sink2['id']))
    wait_until('the first sink to be the default again',
               lambda: tool('wpctl', 'inspect', '@DEFAULT_AUDIO_SINK@').stdout.count('svolder_e2e_sink"') > 0, 8)
    conn.eq({'op': 'mode', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)

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

    # ---- Virtual speakers (V0-V11): four convolvers and two mixers after the crossfeed stage, played
    # through a synthetic set (the vs_expect fixture, imported as the person's own set and converted by
    # the daemon); every expected number is the engine's response of the stored WAV (vs_expect).
    # G left the sink at 50 % (a gain of 0.125): back to full so the tones play at their own level
    conn.ask({'cmd': 'host_volume', 'pct': 100})
    check(tool('wpctl', 'get-volume', '@DEFAULT_AUDIO_SINK@').stdout.startswith('Volume: 1.00'), 'the sink is back at full volume')
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
    fixture = RUN + '/vs-room.wav'
    r = subprocess.run([VS_EXPECT, '--write', fixture], capture_output=True, text=True, timeout=30)
    check(r.returncode == 0 and os.path.getsize(fixture) > 4 * 4800 * 4, 'vs_expect wrote the synthetic 4-channel room (%s)' % (r.stderr.strip() or fixture))
    entry = vs_import(conn, fixture)
    sid = entry['id']
    swav = vs_wav(sid)
    check(os.path.exists(swav), 'the daemon stored the converted set as %s' % swav)
    check(entry['room'] == 'with' and entry['fetched'] is True and entry['own'] is True,
          'the catalogue lists it as an own set under With room (%s)' % {k: entry[k] for k in ('id', 'room', 'rate', 'taps_ms', 'loudness_db', 'cut')})
    vsv = conn.view()['eq']['speakers']
    check(vsv is not None and vsv['on'] is False and vsv['set'] is None, 'the view carries the virtual speakers, off, no set chosen yet')

    # V0. a set chosen while the speakers are off is only kept: the chain is not rebuilt and no WAV is loaded
    retone()
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    check(settles(lambda: tone_links() == [CHAIN], 10.0), 'the tone plays through the chain')
    chain_v0 = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    conn.eq({'op': 'speakers', 'set': sid})
    vsv = conn.view()['eq']['speakers']
    check(vsv['set'] == sid and vsv['on'] is False and vsv['room_name'] == 'With room' and vsv['term'] == 'BRIR',
          'the view says %s chosen, off, "%s · %s"' % (vsv['set'], vsv['room_name'], vsv['term']))
    conf_path = chain_conf()
    check(conf_path is not None and os.path.exists(conf_path), 'the chain\'s config is readable (%s)' % conf_path)
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_v0 and 'lvs:Gain 1' not in chain_params()
          and 'vLL' not in open(conf_path).read(),
          'choosing a set while the speakers are off changed nothing: same chain, no stage in the graph or the config')
    check(conn.view()['eq']['preamp_db'] == 0.0, 'the EQ is flat, the preamp 0 dB')

    # V1. on rebuilds the chain once with the stage in it; the config on disk holds the stage off while it plays;
    # off keeps the convolvers loaded and idle, and the chain is a bit-exact bypass
    conn.eq({'op': 'speakers', 'on': True})
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0 and len(_private_pids('host-eq.conf')) == 1 and node(CHAIN), 10.0),
          'turning the speakers on rebuilt the chain with the stage playing (pids %s -> %s)' % (chain_v0[1], _private_pids('host-eq.conf')))
    check(_private_pids('host-eq.conf') != chain_v0[1] and len(chain_nodes()) == 1, 'one rebuild, one chain (%d node, %d process)' % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    conf_path = chain_conf() or conf_path
    conf_text = open(conf_path).read()
    check('vLL' in conf_text and 'vRR' in conf_text and VS_CONF_OFF.search(conf_text) is not None
          and chain_params().get('lvs:Gain 2') == 1.0,
          'while the speakers play the config on disk holds the four convolvers and the stage OFF (dry 1, wet 0, 0)')
    vs_forget_refused(conn, sid, swav, 'V1 while the speakers play')
    conn.eq({'op': 'speakers', 'on': False})
    check(settles(lambda: 'vLL' not in open(conf_path).read(), 10.0), 'after off the config is rewritten without the convolvers')
    check('lvs:Gain 1' in chain_params(), 'the graph still holds the idle stage (convolvers loaded)')
    vs_null('V1 bypass (set loaded, speakers off) left-only 1 kHz', 1000)
    vs_bypass_exact('V1')
    vs_forget_refused(conn, sid, swav, 'V1 off but loaded')
    chain_off = (node(CHAIN)['id'], _private_pids('host-eq.conf'))

    # V6b. the WAV is gone while the stage is OFF but loaded: a rate change rebuilds the chain without the stage and
    # says so; the chain stays alive and never goes silent
    os.remove(swav)
    tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    check(settles(lambda: _private_pids('host-eq.conf') not in ([], chain_off[1]) and node(CHAIN) and 'lvs:Gain 1' not in chain_params(), 10.0),
          'the rebuild after the WAV went missing left the stage out (pids %s -> %s)' % (chain_off[1], _private_pids('host-eq.conf')))
    check(settles(lambda: 'could not be read' in (conn.view()['eq']['note'] or ''), 6.0), 'the EQ says why: %r' % conn.view()['eq']['note'])
    check(len(chain_nodes()) == 1 and len(_private_pids('host-eq.conf')) == 1 and conn.view()['eq']['live'], 'the chain is alive and live')
    tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    vs_null('V6b chain after the fallback (speakers off) left-only 1 kHz', 1000, bypass_ports=False)
    chain_untouched = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    r = conn.eq({'op': 'speakers', 'set': sid, 'on': True}, expect_ack=False)
    check(r.get('result') != 'ack' and 'missing or damaged' in json.dumps(r),
          'turning on a set whose WAV is missing is refused in words (%s)' % json.dumps(r)[:160])
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_untouched and conn.view()['eq']['speakers']['on'] is False,
          'the refusal left the chain and the speakers as they were')
    entry = vs_import(conn, fixture)
    check(entry['id'] == sid and os.path.exists(swav), 'the set is back as %s' % sid)

    # the converter kept the four paths apart: the stored set is the fixture scaled by one real gain and moved by
    # the converter's lead-in (the fixture's first Dirac is at sample 0, so the store puts 1 ms of zeros in front)
    probe = [100, 300, 1000, 2000, 5000]
    fx, st = vs_oracle(RATE, fixture, probe), vs_oracle(RATE, swav, probe)
    lead_s = (RATE // 1000) / RATE
    scale = [st[k][p] / fx[k][p] * cmath.exp(2j * math.pi * f * lead_s)
             for k, f in enumerate(probe) for p in ('ll', 'lr', 'rl', 'rr')]
    check(all(abs(x - scale[0]) <= 2e-3 * abs(scale[0]) for x in scale) and scale[0].real > 0 and abs(scale[0].imag) < 2e-3 * scale[0].real,
          'the stored set is the fixture times one real gain on all four paths at %d frequencies (x%.3f)' % (len(probe), scale[0].real))

    # V2. on, a tone on the left speaker alone: both ears' level and phase against the engine's response
    conn.eq({'op': 'speakers', 'set': sid, 'on': True})
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0 and len(_private_pids('host-eq.conf')) == 1 and node(CHAIN), 10.0),
          'the speakers are on with the set back')
    chain_v1 = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    vsv = conn.view()['eq']['speakers']
    check(vsv['on'] is True and vsv['set'] == sid, 'the view says the virtual speakers are on')
    for f in (100, 1000, 5000):
        vs_case(conn, 'V2', f, swav, 'left')
    # V3. the right speaker alone
    for f in (100, 1000, 5000):
        vs_case(conn, 'V3', f, swav, 'right')
    # V4. mono: never above 0 dBFS at any frequency (the set is scaled so the worst case is -0.1 dB)
    for f in (100, 1000, 5000):
        got = vs_case(conn, 'V4', f, swav, 'mono')
        check(got is not None and db_of(got[0]) <= 0.1 and db_of(got[1]) <= 0.1,
              'a mono %g Hz tone plays at %+.2f / %+.2f dB re the input, never above it' % (f, db_of(got[0]), db_of(got[1])) if got else 'mono tone recorded')
    grid = [20 * 1000 ** (i / 399) for i in range(400)]
    sweep = vs_oracle(RATE, swav, grid)
    worst = max(max(20 * math.log10(abs(r['ll'] + r['rl'])), 20 * math.log10(abs(r['lr'] + r['rr']))) for r in sweep)
    check(worst <= 0.0, 'the engine\'s mono response is at most %+.2f dB re the input over 20 Hz to 20 kHz (400 points)' % worst)
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_v1, 'measuring did not restart the chain (node %d, pids %s)' % chain_v1)
    # off and on again: no rebuild either way, and off is the bit-exact bypass again
    conn.eq({'op': 'speakers', 'on': False})
    vs_null('V1b bypass after the speakers were on, left-only 1 kHz', 1000)
    vs_bypass_exact('V1b')
    conn.eq({'op': 'speakers', 'on': True})
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0, 6.0), 'the speakers play again')
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_v1, 'off and on again did not restart the chain')
    vs_case(conn, 'V2b after off and on', 1000, swav, 'left')

    # V10. the chain process's CPU while a tone plays through the speakers (recorded, not a gate)
    pid = _private_pids('host-eq.conf')[0]
    c0, t0 = cpu_seconds(pid), time.monotonic()
    time.sleep(6.0)
    cpu = (cpu_seconds(pid) - c0) / (time.monotonic() - t0) * 100
    row('V10 chain process CPU, speakers on, a tone playing (6 s window)', 'recorded, no limit', '%.2f %% of one core' % cpu)

    # V5. a rate change: the convolvers re-resample, the speakers stay on and play at the new rate
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    check(r.returncode == 0, 'clock.force-rate 44100 set on the private instance')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0 and conn.view()['eq']['speakers']['on'] is True, 8.0),
          'the stage was sent again after the rate change and the speakers are still on (%r)' % conn.view()['eq']['speakers']['note'])
    v44 = [vs_case(conn, 'V5 rate 44100 Hz', f, swav, 'left') for f in (100, 1000)]
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    check(r.returncode == 0, 'clock.force-rate released')
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    vs_case(conn, 'V5 back at 48000 Hz', 100, swav, 'left')

    # V6. the set's WAV is gone while the chain holds it: the next rebuild (a rate change) leaves the stage out,
    # with the speakers off and a note, and the chain stays alive and plays the dry sound
    os.remove(swav)
    r = tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '44100')
    wait_until('the sink at 44100', lambda: sink_rate() == 44100, 6)
    check(settles(lambda: conn.view()['eq']['speakers']['on'] is False, 8.0), 'the speakers turned themselves off once the WAV was gone and the chain rebuilt')
    vsv = conn.view()['eq']['speakers']
    row('V6 the view after the WAV went missing', 'off, set-missing, a note to fetch it again',
        'on %s, off_reason %s, note %r' % (vsv['on'], vsv['off_reason'], vsv['note']))
    check(vsv['off_reason'] == 'set-missing' and vsv['note'] and 'missing or damaged' in vsv['note'],
          'the fallback says why: %r (%s)' % (vsv['note'], vsv['off_reason']))
    check(len(chain_nodes()) == 1 and len(_private_pids('host-eq.conf')) == 1 and conn.view()['eq']['on'] and conn.view()['eq']['live'],
          'the chain is alive after the fallback (%d node, %d process, EQ on and live)' % (len(chain_nodes()), len(_private_pids('host-eq.conf'))))
    tool('pw-metadata', '-n', 'settings', '0', 'clock.force-rate', '0')
    wait_until('the sink back at 48000', lambda: sink_rate() == 48000, 6)
    vs_null('V6 chain after the fallback (was on), left-only 1 kHz', 1000, bypass_ports=False)
    # fetch it again (the same file is the same set), choose it, on
    entry = vs_import(conn, fixture)
    check(entry['id'] == sid and os.path.exists(swav), 'the set is back as %s' % sid)
    conn.eq({'op': 'speakers', 'set': sid, 'on': True})
    check(settles(lambda: conn.view()['eq']['speakers']['on'] is True and 'lvs:Gain 2' in chain_params(), 10.0), 'the speakers are on again with the set back')
    vs_case(conn, 'V6 after the set came back', 1000, swav, 'left')

    # V7. crossfeed on turns the speakers off (one update), and the speakers on turn crossfeed off
    conn.eq({'op': 'crossfeed', 'on': True})
    v = conn.view()['eq']
    check(v['crossfeed']['on'] is True and v['speakers']['on'] is False and v['speakers']['set'] == sid,
          'crossfeed on turned the speakers off and kept the choice (crossfeed %s, speakers %s)' % (v['crossfeed']['on'], v['speakers']['on']))
    check(settles(vs_bypass_ok, 6.0), 'the speakers mixers hold the bypass (both mixers, all three gains) while crossfeed plays')
    xf_case(conn, 'V7 crossfeed over the speakers', 1000)
    conn.eq({'op': 'speakers', 'on': True})
    v = conn.view()['eq']
    check(v['crossfeed']['on'] is False and v['speakers']['on'] is True,
          'the speakers on turned crossfeed off (crossfeed %s, speakers %s)' % (v['crossfeed']['on'], v['speakers']['on']))
    vs_case(conn, 'V7 speakers back', 1000, swav, 'left')

    # V8. the default sink moves to a second null sink: the speakers turn themselves off, with a note
    r = tool('pw-cli', 'create-node', 'adapter',
             '{ factory.name=support.null-audio-sink node.name=svolder_e2e_sink2 '
             'node.description="svolder e2e sink 2" media.class=Audio/Sink audio.position=[ FL FR ] '
             'object.linger=true priority.session=500 '
             'adapter.auto-port-config={ mode=dsp monitor=true control=false position=preserve } }')
    check(r.returncode == 0, 'a second null sink was made in the private instance (%s)' % (r.stdout + r.stderr).strip()[:100])
    sink2 = wait_until('the second sink', lambda: node('svolder_e2e_sink2'), 8)
    sink1 = node(SINK)['id']
    check(conn.view()['eq']['speakers']['on'] is True, 'the speakers are on before the switch')
    t_switch = time.monotonic()
    r = tool('wpctl', 'set-default', str(sink2['id']))
    check(r.returncode == 0, 'the default sink is now the second one (%s)' % r.stderr.strip()[:100])
    sv = None
    while time.monotonic() - t_switch < 6.0:
        sv = conn.view()['eq']['speakers']
        if not sv['on']:
            break
        time.sleep(0.05)
    took = time.monotonic() - t_switch
    row('V8 speakers off after the default sink moved', 'off within 2 s, with a note',
        '%s after %.2f s, off_reason %s, note %r' % ('off' if not sv['on'] else 'STILL ON', took, sv['off_reason'], sv['note']))
    check(not sv['on'] and took <= 2.0 and sv['note'] and sv['off_reason'] == 'output-changed' and sv['set'] == sid,
          'the speakers turned themselves off %.2f s after the default sink moved, with the note %r, the choice kept' % (took, sv['note']))
    tool('wpctl', 'set-default', str(sink1))
    tool('pw-cli', 'destroy', str(sink2['id']))
    wait_until('the first sink to be the default again',
               lambda: tool('wpctl', 'inspect', '@DEFAULT_AUDIO_SINK@').stdout.count('svolder_e2e_sink"') > 0, 8)
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)

    # V11. off on a chain that runs for the speakers alone, with a tone playing: the chain stays as a pass-through
    # until idle (never silent while live), then goes
    block = play(1000)
    conn.eq({'op': 'speakers', 'set': sid, 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0 and conn.view()['eq']['on'] is False, 10.0),
          'a chain for the speakers alone runs, the EQ off')
    wait_until('the tone to play through the chain', chain_playing, 10)
    check(settles(lambda: tone_links() == [CHAIN], 10.0), 'the tone is linked to the chain')
    chain_pt = (node(CHAIN)['id'], _private_pids('host-eq.conf'))
    conn.eq({'op': 'speakers', 'on': False})
    v = conn.view()['eq']
    check((node(CHAIN)['id'], _private_pids('host-eq.conf')) == chain_pt and v['note'] and 'pass-through' in v['note'] and v['speakers']['on'] is False,
          'off kept the chain as a pass-through (%r), same node and process' % v['note'])
    null_test('V11 pass-through (speakers off, EQ off) left-only 1 kHz', 1000, block)
    go_quiet()
    check(settles(lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 15.0), 'the pass-through chain is removed once nothing plays through it')

    # V9. the speakers on with a chain for them alone (EQ off), then a graceful stop and a kill of the daemon: each
    # restart finds them off, the choice kept, no chain, and nothing in the config or the graph playing a WAV
    def vs_restart_checks(what, reason):
        wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
        v = conn.view()['eq']
        sv = v['speakers']
        check(sv['on'] is False and sv['set'] == sid and v['on'] is False,
              'after %s the speakers are off, the choice kept (%r, off_reason %s)' % (what, sv['note'], sv['off_reason']))
        if reason:
            check(sv['off_reason'] == reason and sv['note'], 'after %s the view says why: %r (%s)' % (what, sv['note'], sv['off_reason']))
        text = open(conf_path).read() if os.path.exists(conf_path) else ''
        check(re.search(r'name = (lvs|rvs) label = mixer control = \{ "Gain 1" = 0 ', text) is None,
              'after %s the config holds no stage that plays (%s)' % (what, 'no config left' if not text else 'stage %s' % ('listed, off' if 'lvs' in text else 'absent')))
        check(node(CHAIN) is None and not _private_pids('host-eq.conf') and 'lvs:' not in tool('pw-dump').stdout,
              'after %s no chain runs and no stage is in the graph (pw-dump has no lvs: control)' % what)

    retone()
    conn.eq({'op': 'speakers', 'set': sid, 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    check(settles(lambda: conn.view()['eq']['speakers']['on'] is True and 'lvs:Gain 2' in chain_params(), 10.0),
          'a chain for the speakers alone runs (EQ on %s)' % conn.view()['eq']['on'])
    conf_path = chain_conf() or conf_path
    conn.ask({'cmd': 'stop'})
    watch.close()
    conn.close()
    try:
        code = daemon.wait(10)
    except subprocess.TimeoutExpired:
        raise Fail('the daemon did not end after stop')
    check(code == 0, 'the daemon ended on stop (exit %s) with the speakers playing' % code)
    wait_until('the chain to end', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
    daemon, conn = start_daemon()
    watch = Watch(sock)
    vs_restart_checks('a graceful restart', None)
    conn.eq({'op': 'speakers', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    os.kill(daemon.pid, signal.SIGKILL)
    daemon.wait(5)
    watch.close()
    conn.close()
    gone = settles(lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10.0)
    say('V9 the chain after the daemon was killed: %s' % ('gone' if gone else 'still there'))
    daemon, conn = start_daemon()
    watch = Watch(sock)
    vs_restart_checks('a killed daemon\'s restart', 'restart')
    # the choice survives, and the next chain it builds holds the stage off (the tone of the first half ends first:
    # a second retone would leave it running under the same name)
    go_quiet()
    retone()
    conn.eq({'op': 'mode', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    check(settles(lambda: node(CHAIN) and len(_private_pids('host-eq.conf')) == 1 and conn.view()['eq']['live'], 10.0), 'a chain runs')
    vsv = conn.view()['eq']['speakers']
    check('lvs:Gain 1' not in chain_params() and 'vLL' not in open(chain_conf()).read() and vsv['set'] == sid and vsv['on'] is False,
          'a chain built while the speakers are off holds no stage (no lvs: control, no convolver in the config), the choice kept')
    # on: one rebuild, and it plays
    pids = _private_pids('host-eq.conf')
    conn.eq({'op': 'speakers', 'on': True})
    check(settles(lambda: chain_params().get('lvs:Gain 2') == 1.0 and _private_pids('host-eq.conf') not in ([], pids), 10.0),
          'turning the speakers on rebuilt the chain once with the stage playing')
    vs_case(conn, 'V9 after the restarts', 1000, swav, 'left')
    go_quiet()
    conn.eq({'op': 'mode', 'on': False})
    conn.eq({'op': 'speakers', 'on': False})
    wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 15)
    say('host-e2e: virtual speakers phases V0-V11 done')

    # U. The VU meters' levels: the real capture (pw-record of the sink's monitor) into the daemon, read as the
    # levels-only frames (`?only=levels`) of its web page. The sink is at full volume and unmuted (G left it at 50 %
    # and the V phases put it back; set again here so this phase never depends on them), the EQ and the speakers
    # off, so the monitor carries the signal as played. A level byte maps -72..0 dBFS over 0..255; a peak byte of 255
    # is an over (a sample at 16-bit full scale or above).
    conn.ask({'cmd': 'host_volume', 'pct': 100})
    conn.ask({'cmd': 'host_mute', 'muted': False})
    r = tool('wpctl', 'get-volume', '@DEFAULT_AUDIO_SINK@').stdout.strip()
    check(r.startswith('Volume: 1.00') and 'MUTED' not in r, 'U the sink is at full volume and unmuted (%s)' % r)
    v = conn.view()['eq']
    check(node(CHAIN) is None and not v['on'] and not v['speakers']['on'] and not v['crossfeed']['on'],
          'U no chain, the EQ, the speakers and the crossfeed off: the monitor carries the signal as played')
    go_quiet()
    levels = vu.Levels(web_port, web_token)
    try:
        def vu_play(label, x):
            """Play `x` on both channels until the next one; returns the median and extremes of the level bytes
            of the frames that arrive once the capture holds it (a half second after both channels show signal)."""
            go_quiet()
            path = '%s/vu-%s.wav' % (RUN, label)
            vu.write_wav(path, x)
            t_play = time.monotonic()
            STATE['tone'] = spawn('pw-cat', '-p', '--properties', 'node.name=svtone', path)
            wait_until('the tone', lambda: node('svtone'))
            wait_until('both channels of the frames to carry it',
                       lambda: any(f['lv'][0] > 100 and f['lv'][1] > 100 for f in levels.since(t_play)), 15)
            # settled: 15 more frames (half a second at the capture's pace) after the first with signal, then the
            # window of the next 45
            t_sig = time.monotonic()
            wait_until('15 frames of the signal', lambda: len(levels.since(t_sig)) >= 15, 10)
            t0 = time.monotonic()
            wait_until('45 frames of the window', lambda: len(levels.since(t0)) >= 45, 10)
            frames = levels.since(t0)
            check(len(frames) >= 30, 'U %s: %d levels-only frames in the window' % (label, len(frames)))
            st = vu.window_stats(frames)
            say('U  %s: rms bytes %s (%.2f, %.2f dBFS), peak bytes %s..%s' % (
                label, st['rms'], vu.byte_db(st['rms'][0]), vu.byte_db(st['rms'][1]), st['peak_min'], st['peak_max']))
            return st

        # a -18 dBFS RMS 1 kHz sine (peak -15 dBFS): the RMS byte reads -18 dB within 0.3 dB
        st = vu_play('sine-rms-18', vu.sine(rms_dbfs=-18.0))
        check(vu.only_levels_shape([f for _, f in levels.frames]), 'U every frame of ?only=levels is exactly {t, lv} with four bytes')
        names = [e[1] for e in levels.events]
        check(names[:1] == ['hello'] and set(names) <= {'hello', 'capture', 'frame'},
              'U the levels-only stream sends hello, capture and frames only (%s)' % sorted(set(names)))
        state = [e[2] for e in levels.events if e[1] in ('hello', 'capture')][-1]
        check(state.get('capture') == 'running' and state.get('sink') == SINK_DESCRIPTION,
              'U the capture runs on the private sink (%s on %s)' % (state.get('capture'), state.get('sink')))
        for ch, b in zip('LR', st['rms']):
            check(abs(vu.byte_db(b) + 18.0) <= 0.3, 'U the %s RMS byte %d reads %.2f dBFS, -18 +- 0.3' % (ch, b, vu.byte_db(b)))
        check(max(st['peak_max']) <= 254, 'U a -18 dBFS RMS sine is no over (peak bytes %s)' % (st['peak_max'],))
        # a 0 dBFS square (16-bit full scale): every frame's peak byte is 255, the over
        st = vu_play('square-0', vu.square(0.0))
        check(st['peak_min'] == (255, 255), 'U a 0 dBFS square: the peak byte is 255 in every frame (%s)' % (st['peak_min'],))
        # a -0.1 dBFS sine: close to full scale and no over
        st = vu_play('sine-peak-0.1', vu.sine(peak_dbfs=-0.1))
        check(max(st['peak_max']) <= 254 and min(st['peak_min']) >= 253,
              'U a -0.1 dBFS sine: the peak byte is 254 or less, never 255 (%s..%s)' % (st['peak_min'], st['peak_max']))
        # a -0.1 dBFS square: the 0.1 dB under full scale is not an over either
        st = vu_play('square-peak-0.1', vu.square(-0.1))
        check(max(st['peak_max']) <= 254 and min(st['peak_min']) >= 253,
              'U a -0.1 dBFS square: the peak byte is 254 or less (%s..%s)' % (st['peak_min'], st['peak_max']))
    finally:
        levels.close()
    go_quiet()
    say('host-e2e: VU phase U done')

    # X4. a restart of the daemon: crossfeed is off after it, and a chain for it alone is gone
    def restart_checks(what):
        v = conn.view()
        xf = v['eq']['crossfeed']
        wait_until('no chain', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
        check(xf['on'] is False and v['eq']['on'] is False, 'after %s the crossfeed is off and the EQ off (off_reason %s, note %r)'
              % (what, xf['off_reason'], xf['note']))
        check(node(CHAIN) is None and not _private_pids('host-eq.conf'), 'after %s no svolder_eq node and no chain process' % what)
        return xf

    conn.eq({'op': 'mode', 'on': False})
    wait_until('the chain\'s node to leave', lambda: node(CHAIN) is None, 10)
    conn.eq({'op': 'crossfeed', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    conn.ask({'cmd': 'stop'})
    watch.close()
    conn.close()
    try:
        code = daemon.wait(10)
    except subprocess.TimeoutExpired:
        raise Fail('the daemon did not end after stop')
    check(code == 0, 'the daemon ended on stop (exit %s) with a chain for crossfeed alone' % code)
    wait_until('the chain to end', lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10)
    daemon, conn = start_daemon()
    watch = Watch(sock)
    restart_checks('a graceful restart')
    conn.eq({'op': 'crossfeed', 'on': True})
    wait_until('the chain\'s node', lambda: node(CHAIN), 10)
    os.kill(daemon.pid, signal.SIGKILL)
    daemon.wait(5)
    watch.close()
    conn.close()
    gone = settles(lambda: node(CHAIN) is None and not _private_pids('host-eq.conf'), 10.0)
    say('X4 the chain after the daemon was killed: %s' % ('gone' if gone else 'still there'))
    daemon, conn = start_daemon()
    watch = Watch(sock)
    xf = restart_checks('a killed daemon\'s restart')
    check(xf['off_reason'] == 'restart' and xf['note'], 'the restart that found crossfeed on says why: %r (%s)' % (xf['note'], xf['off_reason']))

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
    say('host-e2e: %d crossfeed numbers above (the T lines)' % len(TABLE))
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
