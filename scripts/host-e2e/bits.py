#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""host-e2e phases B and C: are known 16- and 24-bit samples bit-exact at a sink converter's output, and
does `svolder host check` say so? (phase B asks PipeWire alone; phase C asks svolder's words against it)

Run by run.sh inside the environment private-pipewire.sh prints (it refuses, exit 2, without it), alone
with `make host-e2e-bits`. One `pipewire -c` client of the PRIVATE instance (bits.conf) provides pipe
sinks: each is a sink whose converter hands what a device would receive to a FIFO. A seeded pattern is
played to one of them with pw-cat at stream and sink volume 1.0, the FIFO is read in a thread of its
own, and every sample from the pattern's first frame on is compared. Each row of the table below is a
check with its expected outcome; a negative control (a known breaker) that comes out exact fails the
run, so the comparison is shown able to fail:

  stream        sink                       result
  S16           S16LE, S24LE, S32LE        exact
  24-bit in S32 S24LE, S32LE               exact
  24-bit in S32 S16LE                      not exact (narrowing)
  S16           S16LE + triangular dither  not exact
  S16           S16LE + dither noise 4     not exact
  S16           S16LE + a filter graph     not exact
  S16 mono      S16LE stereo               exact (both channels are copies)
  S16 / 24-bit  stream dither noise 4      exact (a playback stream's converter does not quantise)
  24-bit @44.1k 44.1 kHz sink, allowed-rates [ 44100 48000 ]: exact; with clock.force-rate 48000: not exact
  24-bit @48k   clock.force-rate 48000     exact (the stream's own rate)
  24-bit @44.1k a 48 kHz sink, no force    not exact (resampled by the sink's rate)

A negative control passes only with a full-length sound AND its own difference signature (cut to 16 bits,
a step of 16 bits on about half the samples, at most 8 steps, every sample halved, nearly all samples differ);
silence or a short capture is an accident, not a pass. The stream's own `dither.noise` is read back from
the player node's Props while it plays, and `clock.force-rate` from the metadata.

Phase C: while each row's player plays, the row's sink is the default in the private `default` metadata
and `target/debug/svolder host check --json --player "Fixture Player"` reads the private graph (no
daemon, `SVOLDER_PW_DUMP` unset). An exact row must read `untouched`; a NOT-exact row must read `changed`
and name its cause (so untouched implies exact, and no known breaker goes unnamed); each row's seven
config lines must pass or fail as the row's setup says (a fault fails its own line, the others pass). The
null sink then shows the rates line following `clock.allowed-rates` and the forced-rate line following
`clock.force-rate` in the private `settings` metadata, both put back.

What it proves: PipeWire's stream converter, the mixer with one input, the volume at 1.0 and the sink
converter are transparent for 16- and 24-bit samples, and each fault breaks it. What it does not prove:
what ALSA does past the sink converter (hw: hands the bytes on, plug: converts), the USB driver and the
DAC; and that an ALSA sink's adapter is the same converter object (the same library and params shape).

HOST_E2E_BITS_FLIP=1 flips the lowest bit of one written sample of each capture before it is compared: every exact row must then
fail (a proof that one wrong bit is seen); it is a dev aid and the run fails by design.
`python3 -I bits.py --selftest` checks the comparison on made-up data and needs no PipeWire.

Prints one line per check (phase C adds each row's words and the null-sink steps) and a closing
`host-e2e-bits: ... all as expected` line, or every failed check
(exit 1); the PASS line is run.sh's.
"""
import collections
import fcntl
import json
import os
import re
import select
import signal
import subprocess
import sys
import threading
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
LAUNCHER = os.path.join(HERE, 'private-pipewire.sh')
RATE = 48000
SECONDS = 2
SEED = 20261118
# The sink's own sample width and its bytes per sample as written to the FIFO.
SINK_BITS = {'S16LE': 16, 'S24LE': 24, 'S32LE': 32}
SINK_BYTES = {'S16LE': 2, 'S24LE': 3, 'S32LE': 4}
# pw-cat's stream formats: 16-bit samples, or 24-bit content in the top bits of 32 (as players send it).
STREAM_FMT = {16: 's16', 24: 's32'}

STATE = {'children': [], 'failed': []}
FLIP = os.environ.get('HOST_E2E_BITS_FLIP') == '1'


class Fail(Exception):
    pass


def say(line):
    print(line, flush=True)


def check(ok, what):
    if ok:
        say('  ok: ' + what)
    else:
        STATE['failed'].append(what)
        say('  FAIL: ' + what)


def refuse(why):
    print('host-e2e-bits: refusing to run: ' + why, file=sys.stderr)
    sys.exit(2)


# ---- the comparison (pure: the selftest runs it on made-up data) -----------------------------

def pattern(bits, frames, channels, seed):
    """Seeded random signed samples of `bits` (frames x channels, int64): the extremes first, and no zero
    sample anywhere, so the first non-zero output frame is the pattern's first frame."""
    rng = np.random.default_rng(seed)
    lo, hi = -(1 << (bits - 1)), (1 << (bits - 1)) - 1
    x = rng.integers(lo, hi + 1, size=(frames, channels), dtype=np.int64)
    x[x == 0] = 1
    x[0, :] = hi
    x[1, :] = lo
    return x


def source_bytes(x, bits):
    """The stream's bytes: s16 little endian, or 24-bit content in the top bits of s32 little endian."""
    if bits == 16:
        return x.astype('<i2').tobytes()
    return (x << 8).astype('<i4').tobytes()


def dom32(x, bits):
    """Samples of `bits` in a common 32-bit domain (shifted to the top), so any two widths compare."""
    return x.astype(np.int64) << (32 - bits)


def decode(raw, sink_fmt, channels):
    """A FIFO's bytes as samples in the 32-bit domain, frames x channels (a partial frame is dropped)."""
    nb = SINK_BYTES[sink_fmt]
    frame = nb * channels
    raw = raw[:len(raw) // frame * frame]
    b = np.frombuffer(raw, dtype=np.uint8).reshape(-1, nb)
    if nb == 2:
        v = (b[:, 0].astype(np.int64) | (b[:, 1].astype(np.int64) << 8))
        v = (v ^ 0x8000) - 0x8000
    elif nb == 3:
        v = (b[:, 0].astype(np.int64) | (b[:, 1].astype(np.int64) << 8) | (b[:, 2].astype(np.int64) << 16))
        v = (v ^ 0x800000) - 0x800000
    else:
        v = (b[:, 0].astype(np.int64) | (b[:, 1].astype(np.int64) << 8) | (b[:, 2].astype(np.int64) << 16)
             | (b[:, 3].astype(np.int64) << 24))
        v = (v ^ 0x80000000) - 0x80000000
    return (v << (32 - 8 * nb)).reshape(-1, channels)


def compare(out, src):
    """Whether `out` (frames x channels, 32-bit domain) holds `src` (frames x 1 or x channels, same domain)
    from its first non-zero frame on, every sample equal. A mono source must be on every output channel.
    Returns a dict: exact, start (the first non-zero frame), differing, total, max_lsb (largest gap in units
    of the 32-bit domain's 2^8, a 24-bit step), run (frames from the first to the last non-zero one)."""
    n = len(src)
    nz = np.nonzero(np.any(out != 0, axis=1))[0]
    if len(nz) == 0:
        return {'exact': False, 'start': None, 'differing': n * out.shape[1], 'total': n * out.shape[1],
                'max_lsb': 0, 'why': 'no sound at the sink'}
    start = int(nz[0])
    run = int(nz[-1]) - start + 1  # frames from the first to the last non-zero one
    seg = out[start:start + n]
    if len(seg) < n:
        return {'exact': False, 'start': start, 'differing': n * out.shape[1], 'total': n * out.shape[1],
                'max_lsb': 0, 'why': 'only %d of %d frames came out' % (len(seg), n)}
    want = np.broadcast_to(src, seg.shape) if src.shape[1] == 1 else src
    diff = seg != want
    return {'exact': not bool(diff.any()), 'start': start, 'differing': int(diff.sum()), 'total': int(diff.size),
            'max_lsb': int(np.max(np.abs(seg - want))) >> 8, 'why': None, 'seg': seg, 'want': np.array(want),
            'run': run}


def first_sound(out):
    """The first frame of `out` (frames x channels) with a non-zero sample, or None."""
    nz = np.nonzero(np.any(out != 0, axis=1))[0]
    return int(nz[0]) if len(nz) else None


def has_tail(raw, sink_fmt, frames, margin=512):
    """Whether `raw` holds `frames` + `margin` frames from its first non-zero frame on (bytes of an earlier
    case's silence in front do not count)."""
    out = decode(raw, sink_fmt, 2)
    start = first_sound(out)
    return start is not None and len(out) - start >= frames + margin


def flip_one(raw, sink_fmt, offset=1000):
    """`raw` with the lowest bit of the first channel's sample `offset` frames after the first non-zero frame
    flipped, in the bytes as written (so the proof covers the decode at the sink's own width)."""
    start = first_sound(decode(raw, sink_fmt, 2))
    if start is None:
        return raw
    off = (start + offset) * 2 * SINK_BYTES[sink_fmt]
    if off >= len(raw):
        return raw
    b = bytearray(raw)
    b[off] ^= 1
    return bytes(b)


def best_corr(a, b, maxlag=2048):
    """The largest normalised cross-correlation of two equal-length signals over lags up to `maxlag` frames."""
    a = np.asarray(a, dtype=np.float64)
    a = a - a.mean()
    b = np.asarray(b, dtype=np.float64)
    b = b - b.mean()
    size = 1 << int(np.ceil(np.log2(len(a) + len(b))))
    c = np.fft.irfft(np.fft.rfft(a, size) * np.conj(np.fft.rfft(b, size)), size)
    c = np.concatenate([c[:maxlag + 1], c[-maxlag:]])
    return float(np.max(np.abs(c)) / (np.linalg.norm(a) * np.linalg.norm(b)))


def selftest():
    src = dom32(pattern(16, 1000, 2, 1), 16)
    out = np.vstack([np.zeros((300, 2), dtype=np.int64), src, np.zeros((50, 2), dtype=np.int64)])
    r = compare(out, src)
    assert r['exact'] and r['start'] == 300, r
    flipped = out.copy()
    flipped[300 + 500, 1] ^= 1 << 16
    r = compare(flipped, src)
    assert not r['exact'] and r['differing'] == 1, r
    r = compare(np.vstack([np.zeros((299, 2), dtype=np.int64), src, np.zeros((51, 2), dtype=np.int64)]), src)
    assert r['exact'] and r['start'] == 299, r
    shifted = np.vstack([np.zeros((300, 2), dtype=np.int64), src[1:], np.zeros((51, 2), dtype=np.int64)])
    assert not compare(shifted, src)['exact'], 'a pattern one frame short must not pass'
    assert not compare(np.zeros((100, 2), dtype=np.int64), src)['exact'], 'silence must not pass'
    assert not compare(out[:600], src)['exact'], 'a cut pattern must not pass'
    mono = dom32(pattern(16, 500, 1, 2), 16)
    both = np.vstack([np.zeros((10, 2), dtype=np.int64), np.repeat(mono, 2, axis=1)])
    assert compare(both, mono)['exact'], 'mono on both channels'
    one = both.copy()
    one[100, 0] ^= 1 << 16
    assert not compare(one, mono)['exact'], 'one wrong channel must not pass'
    assert not (pattern(16, 1000, 2, 1) == 0).any() and not (pattern(24, 1000, 2, 1) == 0).any()
    # the decode of each sink width and the encode of each stream width agree with the 32-bit domain
    for bits, fmt in ((16, 'S16LE'), (24, 'S24LE'), (32, 'S32LE')):
        x = pattern(min(bits, 24), 64, 2, 3) if bits != 32 else pattern(24, 64, 2, 3) << 8
        raw = b''.join(int(v & ((1 << (8 * SINK_BYTES[fmt])) - 1)).to_bytes(SINK_BYTES[fmt], 'little') for v in x.flatten())
        got = decode(raw, fmt, 2)
        assert (got == (x << (32 - bits)).reshape(-1, 2)).all(), fmt
    assert np.frombuffer(source_bytes(pattern(24, 4, 1, 1), 24), dtype='<i4').tolist() == \
        (pattern(24, 4, 1, 1) << 8).flatten().tolist()
    selftest_controls()
    selftest_judge()
    selftest_capture()
    say('host-e2e-bits: selftest ok')


# ---- the private instance --------------------------------------------------------------------

def private_env():
    run = os.environ.get('SVPW_RUN', '')
    want = {
        'XDG_RUNTIME_DIR': run, 'PIPEWIRE_RUNTIME_DIR': run, 'PIPEWIRE_REMOTE': 'svolder-e2e-0',
        'XDG_CONFIG_HOME': run + '/config', 'XDG_STATE_HOME': run + '/state',
        'XDG_DATA_HOME': run + '/data', 'XDG_CACHE_HOME': run + '/cache',
        'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + run + '/no-bus',
    }
    if not (run.startswith('/tmp/svolder/pw.') and len(run) == len('/tmp/svolder/pw.XXXXXX')):
        refuse('SVPW_RUN is not a private-pipewire.sh run directory (run through make host-e2e-bits)')
    for k, v in want.items():
        if os.environ.get(k) != v:
            refuse('%s is not the private instance\'s (%r)' % (k, os.environ.get(k)))
    return run


def spawn(*argv, log=None):
    out = open(log, 'w') if log else subprocess.DEVNULL
    p = subprocess.Popen(list(argv), stdout=out, stderr=subprocess.STDOUT if log else subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True)
    STATE['children'].append(p)
    return p


def stop(p, wait=5):
    """End a process this run started, by its pid (it leads its own session, so its group is ours)."""
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
    return json.loads(tool('pw-dump').stdout)


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


class Reader(threading.Thread):
    """Reads one FIFO until told to stop (a thread of its own: a blocked reader would stall the sink)."""

    def __init__(self, path):
        super().__init__(daemon=True)
        self.path = path
        self.chunks = []
        self.total = 0
        self.halt = threading.Event()
        self.fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
        # The sink writes without blocking and drops what a full pipe cannot take: a big pipe (1 MiB, the
        # unprivileged limit) keeps a reader that a busy machine delays from losing samples.
        try:
            fcntl.fcntl(self.fd, 1031, 1 << 20)  # F_SETPIPE_SZ
        except OSError:
            pass

    def drain(self):
        """Throw away what is in the pipe (an earlier case's silence) before this case's player starts: read
        until it has been empty three times in a row."""
        quiet = 0
        while quiet < 3:
            try:
                data = os.read(self.fd, 1 << 16)
            except BlockingIOError:
                data = b''
            if data:
                quiet = 0
            else:
                quiet += 1
                time.sleep(0.05)

    def run(self):
        while not self.halt.is_set():
            ready, _, _ = select.select([self.fd], [], [], 0.1)
            if not ready:
                continue
            data = os.read(self.fd, 1 << 16)
            if data:
                self.chunks.append(data)
                self.total += len(data)
            else:
                time.sleep(0.01)  # no writer yet: EOF reads return at once

    def finish(self):
        self.halt.set()
        self.join(5)
        os.close(self.fd)
        return b''.join(self.chunks)


def params_of(o, key='Props'):
    """The first `key` entry's flat `params` array of a pw-dump node as a dict (a converter's Props)."""
    for p in ((o or {}).get('info') or {}).get('params', {}).get(key) or []:
        flat = p.get('params')
        if isinstance(flat, list):
            return dict(zip(flat[0::2], flat[1::2]))
    return {}


def sink_format(name):
    """The negotiated Format of a running sink as (format, rate, channels), or None while it is suspended."""
    o = node(name)
    if not o:
        return None
    fmts = ((o.get('info') or {}).get('params') or {}).get('Format') or []
    if not fmts:
        return None
    f = fmts[0]
    return (f.get('format'), f.get('rate'), f.get('channels'))


def sink_volume(name):
    """(channel volumes, mute, volume) from the sink node's Props; Nones when the node is gone."""
    o = node(name)
    for p in (((o or {}).get('info') or {}).get('params') or {}).get('Props') or []:
        if 'channelVolumes' in p:
            return p['channelVolumes'], p.get('mute'), p.get('volume')
    return None, None, None


# Each sink: its format and its rate (bits.conf).
SINKS = {'bits_s16': ('S16LE', 48000), 'bits_s24': ('S24LE', 48000), 'bits_s32': ('S32LE', 48000),
         'bits_s24_44': ('S24LE', 44100), 'bits_s16_tri': ('S16LE', 48000),
         'bits_s16_noise': ('S16LE', 48000), 'bits_s16_graph': ('S16LE', 48000)}

LSB16 = 1 << 16  # one 16-bit step in the 32-bit domain


# ---- the negative controls' own signatures (pure; the selftest runs them) --------------------

def _frac(res):
    return res['differing'] / res['total']


def sig_narrow(res):
    """Narrowed to 16 bits: every sample is the source cut to its top 16 bits (less than one 16-bit step off)."""
    d = np.abs(res['seg'] - res['want'])
    return bool(d.max() < LSB16 and _frac(res) > 0.9), 'cut below bit 16 (max gap %d of 24-bit steps)' % (d.max() >> 8)


def sig_triangular(res):
    """Triangular dither: a 16-bit step off on a third to two thirds of the samples, never more."""
    m = int(np.abs(res['seg'] - res['want']).max())
    return bool(0 < m <= LSB16 and 0.3 <= _frac(res) <= 0.7), 'at most 1 step of 16 bits on %.0f %% of samples' % (100 * _frac(res))


def sig_noise(res):
    """Dither noise 4: off by at most 8 steps of 16 bits, and off."""
    m = int(np.abs(res['seg'] - res['want']).max())
    return bool(0 < m <= 8 * LSB16), 'at most %d steps of 16 bits' % (m // LSB16)


def sig_graph(res):
    """The gain-0.5 filter graph: every sample is half the source (within one step of 16 bits)."""
    d = np.abs(res['seg'] - res['want'] // 2)
    return bool(d.max() <= LSB16 and _frac(res) > 0.99), 'halved (max gap from half %d of 24-bit steps)' % (d.max() >> 8)


def sig_forced(res):
    """Resampled by a forced rate (44.1 to 48 and back at the sink): nearly every sample differs, yet the
    signal is there, a copy of the source at some lag (correlation over 0.5)."""
    corr = best_corr(res['seg'][:, 0], res['want'][:, 0])
    return bool(_frac(res) > 0.9 and corr > 0.5), '%.0f %% of samples differ, correlation %.2f with the source' % (100 * _frac(res), corr)


def sig_rate_mismatch(res):
    """A 44.1 kHz stream into a 48 kHz sink: nearly every sample differs and the sound lasts 48/44.1 as long
    (within 400 frames)."""
    want = res['total'] / 2 * 48000 / 44100
    return bool(_frac(res) > 0.9 and abs(res['run'] - want) <= 400), \
        '%.0f %% of samples differ, %d frames of sound (48 kHz of the same seconds: %d)' % (100 * _frac(res), res['run'], want)


Case = collections.namedtuple('Case', 'label sink bits channels rate props force exact sig noise cause', defaults=(None,))

# The rows of the table in the module docstring, in order. `noise`: the dither.noise the player's own
# converter must read while it plays (a stream property that did not take would make its row vacuous).
CASES = [
    Case('S16 into S16LE', 'bits_s16', 16, 2, 48000, None, None, True, None, None),
    Case('S16 into S24LE (<<8)', 'bits_s24', 16, 2, 48000, None, None, True, None, None),
    Case('S16 into S32LE (<<16)', 'bits_s32', 16, 2, 48000, None, None, True, None, None),
    Case('24-bit in S32 into S24LE', 'bits_s24', 24, 2, 48000, None, None, True, None, None),
    Case('24-bit in S32 into S32LE (<<8)', 'bits_s32', 24, 2, 48000, None, None, True, None, None),
    Case('S16 mono into S16LE stereo (both channels copies)', 'bits_s16', 16, 1, 48000, None, None, True, None, None),
    Case('S16, stream dither.noise 4, into S16LE', 'bits_s16', 16, 2, 48000, 'dither.noise = 4', None, True, None, 4),
    Case('24-bit in S32, stream dither.noise 4, into S24LE', 'bits_s24', 24, 2, 48000, 'dither.noise = 4', None, True, None, 4),
    # force-rate pair on a rig that follows the stream (allowed-rates [ 44100 48000 ], a 44.1 kHz sink)
    Case('24-bit in S32 at 44.1 kHz into the 44.1 kHz S24LE sink, no force', 'bits_s24_44', 24, 2, 44100, None, None, True, None, None),
    Case('24-bit in S32 at 48 kHz into S24LE, clock.force-rate 48000 (the stream\'s own rate)', 'bits_s24', 24, 2, 48000, None, 48000, True, None, None),
    # the negative controls: each known breaker must break it, and in its own way
    Case('NEG narrowing: 24-bit in S32 into S16LE', 'bits_s16', 24, 2, 48000, None, None, False, sig_narrow, None, 'cut to 16 bits'),
    Case('NEG sink dither: S16 into S16LE, dither.method triangular', 'bits_s16_tri', 16, 2, 48000, None, None, False, sig_triangular, None, 'dither on (triangular)'),
    Case('NEG sink dither: S16 into S16LE, dither.noise 4', 'bits_s16_noise', 16, 2, 48000, None, None, False, sig_noise, None, 'dither noise 4'),
    Case('NEG filter graph: S16 into S16LE with audioconvert.filter-graph.0', 'bits_s16_graph', 16, 2, 48000, None, None, False, sig_graph, None, 'a filter inside the sink'),
    Case('NEG forced rate: 24-bit in S32 at 44.1 kHz into the 44.1 kHz S24LE sink, clock.force-rate 48000', 'bits_s24_44', 24, 2, 44100, None, 48000, False, sig_forced, None, '(rate forced)'),
    Case('NEG 24-bit in S32 at 44.1 kHz into the 48 kHz S24LE sink, no force (resampled by the sink rate)', 'bits_s24', 24, 2, 44100, None, None, False, sig_rate_mismatch, None, 'resampled 44.1 → 48 kHz'),
]


SETUP_KEYS = ['rates', 'forced_rate', 'dither', 'format', 'plug', 'stereo', 'filter']


def expected_setup(c):
    """The outcome of each config line for a row's rig: the forced-rate line fails with a force, the dither
    line on the two dither sinks, the 24-bit line on the S16LE sinks (a 16-bit output), the filter line on the
    filter-graph sink; every other line passes (the pipe sinks are 2-channel, no ALSA, rates inside the
    allowed ones)."""
    return {'rates': 'pass', 'forced_rate': 'fail' if c.force is not None else 'pass',
            'dither': 'fail' if c.sink in ('bits_s16_tri', 'bits_s16_noise') else 'pass',
            'format': 'fail' if SINKS[c.sink][0] == 'S16LE' else 'pass', 'plug': 'pass', 'stereo': 'pass',
            'filter': 'fail' if c.sink == 'bits_s16_graph' else 'pass'}


SINK_DESC = {'bits_s16': 'bits S16LE', 'bits_s24': 'bits S24LE', 'bits_s32': 'bits S32LE',
             'bits_s24_44': 'bits S24LE 44.1 kHz', 'bits_s16_tri': 'bits S16LE triangular dither',
             'bits_s16_noise': 'bits S16LE dither noise 4', 'bits_s16_graph': 'bits S16LE filter graph'}


def judge_words(c, exact, j):
    """What a row's `svolder host check --json` must say. Returns the list of problems (empty: as expected):
    the verdict (`untouched` exactly when the samples came out exact; else `changed` naming the cause), and
    each config line against `expected_setup`."""
    bad = []
    path = j.get('path') or {}
    verdict, words = path.get('verdict'), ' | '.join(path.get('words') or [])
    if j.get('sink') != SINK_DESC[c.sink]:
        bad.append('the check judged the sink %r, not %r' % (j.get('sink'), SINK_DESC[c.sink]))
    if 'assumed' in words:
        bad.append('the player was assumed, not matched by name (%s)' % words)
    if exact:
        if verdict != 'untouched':
            bad.append('exact samples, but the path reads %s (%s)' % (verdict, words))
    elif verdict == 'untouched':
        bad.append('NOT exact samples, but the path reads untouched (%s): an unsound verdict' % words)
    elif verdict != 'changed':
        bad.append('NOT exact samples, the path reads %s, not changed (%s)' % (verdict, words))
    elif c.cause is None:
        bad.append('NOT exact, but this row has no known cause (the bits are the failure; the path reads %s)' % words)
    elif c.cause not in words:
        bad.append('NOT exact, changed, but it does not name %r (%s)' % (c.cause, words))
    lines = {l.get('key'): l.get('outcome') for l in (j.get('config') or {}).get('lines') or []}
    if sorted(lines) != sorted(SETUP_KEYS):
        bad.append('config lines are %s, not the seven' % sorted(lines))
    for key, want in expected_setup(c).items():
        if lines.get(key) != want:
            bad.append('config line %s is %s, expected %s' % (key, lines.get(key), want))
    return bad


def selftest_judge():
    """The words judge on made-up output: each kind of disagreement is a problem, agreement is none."""
    def out(verdict, words, sink='bits S16LE', **lines):
        base = {k: 'pass' for k in SETUP_KEYS}
        base.update(lines)
        return {'sink': sink, 'path': {'verdict': verdict, 'words': words},
                'config': {'lines': [{'key': k, 'outcome': v, 'words': ''} for k, v in base.items()]}}

    exact_c = [c for c in CASES if c.label == 'S16 into S24LE (<<8)'][0]
    forced = [c for c in CASES if c.label.startswith('NEG forced rate')][0]
    assert forced.cause == '(rate forced)'
    assert judge_words(forced, False, out('changed', ['resampled 48 \u2192 44.1 kHz'], 'bits S24LE 44.1 kHz', forced_rate='fail')), 'another resample is not the forced rate'
    assert judge_words(forced, False, out('changed', ['resampled 44.1 \u2192 48 kHz (rate forced)'], 'bits S24LE 44.1 kHz', forced_rate='fail')) == []
    tri = [c for c in CASES if c.label.startswith('NEG sink dither: S16 into S16LE, dither.method')][0]
    assert judge_words(exact_c, True, out('untouched', [], 'bits S24LE', format='pass')) == []
    assert judge_words(exact_c, True, out('untouched', [], 'bits S16LE', format='pass')), 'the wrong sink'
    assert judge_words(exact_c, True, out('untouched', ['player assumed'], 'bits S24LE')), 'an assumed player'
    assert judge_words(exact_c, False, out('changed', ['cut to 16 bits'], 'bits S24LE')), 'a not-exact exact row, no raise'
    assert judge_words(exact_c, True, out('changed', ['cut to 16 bits'], 'bits S24LE')), 'exact but changed'
    assert judge_words(exact_c, True, out('untouched', [], 'bits S24LE', filter='fail')), 'a wrong config line'
    assert judge_words(exact_c, True, out('untouched', [], 'bits S24LE') | {'config': {'lines': []}}), 'no config lines'
    good = out('changed', ['dither on (triangular)'], 'bits S16LE triangular dither', dither='fail', format='fail')
    assert judge_words(tri, False, good) == []
    assert judge_words(tri, False, out('untouched', [], 'bits S16LE triangular dither', dither='fail', format='fail')), 'unsound untouched'
    assert judge_words(tri, False, out('unknown', ['x'], 'bits S16LE triangular dither', dither='fail', format='fail')), 'unknown is not changed'
    assert judge_words(tri, False, out('changed', ['computer volume 50 %'], 'bits S16LE triangular dither', dither='fail', format='fail')), 'wrong cause'
    assert judge_words(tri, False, out('changed', ['dither on (triangular)'], 'bits S16LE triangular dither', format='fail')), 'dither line must fail'


def selftest_controls():
    """Each negative control accepts its own signature and rejects an accident (silence, a short capture,
    exact, another breaker's signature)."""
    src = dom32(pattern(24, 4000, 2, 5), 24)
    zeros = np.zeros((300, 2), dtype=np.int64)
    rng = np.random.default_rng(7)

    def res_of(body, tail=50):
        return compare(np.vstack([zeros, body, np.zeros((tail, 2), dtype=np.int64)]), src)

    def case(prefix):
        return [c for c in CASES if c.label.startswith(prefix)][0]

    cut = (src >> 16) << 16
    tri = src + (rng.integers(0, 2, src.shape) * 2 - 1) * LSB16 * (rng.random(src.shape) < 0.5)
    noise = src + rng.integers(-8, 9, src.shape) * LSB16
    half = ((src // 2) >> 16) << 16
    cases = {'NEG narrowing': cut, 'NEG sink dither: S16 into S16LE, dither.method': tri,
             'NEG sink dither: S16 into S16LE, dither.noise': noise, 'NEG filter graph': half,
             'NEG forced rate: 24-bit in S32 at 44.1 kHz into the 44.1': np.roll(src, 5, axis=0) + rng.integers(-3000, 3000, src.shape)}
    for prefix, good in cases.items():
        c = case(prefix)
        assert control_ok(c, res_of(good))[0], 'its own signature must pass: ' + prefix
        assert not control_ok(c, res_of(src))[0], 'exact must fail: ' + prefix
        assert not control_ok(c, compare(np.zeros((900, 2), dtype=np.int64), src))[0], 'silence must fail: ' + prefix
        assert not control_ok(c, compare(np.vstack([zeros, good[:1000]]), src))[0], 'a short capture must fail: ' + prefix
    assert not control_ok(case('NEG forced rate: 24-bit in S32 at 44.1 kHz into the 44.1'), res_of(rng.permutation(src)))[0], \
        'a garbage capture is not a resampled copy'
    # A resample lands between two source frames: the best whole-frame lag then sees about 0.64 of the signal
    # (a half-frame delay of white noise, here the mean of two neighbours), which the threshold must accept;
    # unrelated noise of the same kind must not pass.
    half_delayed = (src + np.roll(src, 1, axis=0)) // 2 + 1
    half = res_of(half_delayed)
    assert 0.5 < best_corr(half['seg'][:, 0], half['want'][:, 0]) < 0.8, 'the test signal is a half-frame delay'
    assert control_ok(case('NEG forced rate: 24-bit in S32 at 44.1 kHz into the 44.1'), half)[0], 'a half-frame delayed copy passes'
    noise_only = dom32(pattern(24, 4000, 2, 99), 24)
    assert not control_ok(case('NEG forced rate: 24-bit in S32 at 44.1 kHz into the 44.1'), res_of(noise_only))[0], \
        'unrelated noise fails'
    big = dom32(pattern(24, 40000, 2, 9), 24)
    rate_case = case('NEG 24-bit in S32 at 44.1 kHz into the 48')
    long_out = dom32(pattern(24, 43537, 2, 10), 24)  # 40000 frames of 44.1 kHz as 48 kHz
    assert control_ok(rate_case, compare(np.vstack([zeros, long_out]), big))[0], 'a 48/44.1 long sound passes'
    assert not control_ok(rate_case, compare(np.vstack([zeros, big + 1]), big))[0], 'the same length is not a rate change'
    assert not control_ok(rate_case, compare(np.vstack([zeros, big[:30000]]), big))[0], 'a short capture fails'
    assert not control_ok(case('NEG filter graph'), res_of(cut))[0], 'a cut is not a halving'
    assert not control_ok(case('NEG sink dither: S16 into S16LE, dither.method'), res_of(noise))[0], 'noise 8 steps is not a 1-step dither'


def selftest_capture():
    """The capture helpers: a stale prefix does not count toward the tail, the flip lands on the pattern whatever
    came before it, and the reader's drain empties a FIFO (in a temp dir of its own, no PipeWire)."""
    import tempfile
    for fmt, bits in (('S16LE', 16), ('S24LE', 24), ('S32LE', 32)):
        nb = SINK_BYTES[fmt]
        x = pattern(min(bits, 24), 3000, 2, 11)
        x = x if bits != 32 else x << 8
        enc = b''.join(int(v & ((1 << (8 * nb)) - 1)).to_bytes(nb, 'little') for v in x.flatten())
        stale = bytes(700 * 2 * nb)
        raw = stale + enc + bytes(600 * 2 * nb)
        assert has_tail(raw, fmt, 3000, 500) and not has_tail(raw, fmt, 3000, 700), 'tail counts from the first sound'
        assert not has_tail(stale + enc[:len(enc) // 2], fmt, 3000), 'half a pattern is not a tail'
        assert not has_tail(b'', fmt, 10), 'nothing is not a tail'
        before, after = decode(raw, fmt, 2), decode(flip_one(raw, fmt), fmt, 2)
        hit = np.argwhere(before != after)
        assert len(hit) == 1 and tuple(hit[0]) == (700 + 1000, 0), (fmt, hit)
        assert abs(int(before[tuple(hit[0])]) - int(after[tuple(hit[0])])) == 1 << (32 - 8 * nb) or nb == 3 and \
            abs(int(before[tuple(hit[0])]) - int(after[tuple(hit[0])])) == 256, fmt
    base = tempfile.mkdtemp(dir='/tmp/svolder') if os.path.isdir('/tmp/svolder') else tempfile.mkdtemp()
    try:
        path = base + '/t.pipe'
        os.mkfifo(path)
        r = Reader(path)
        w = os.open(path, os.O_WRONLY | os.O_NONBLOCK)
        os.write(w, b'stale' * 100)
        r.drain()
        os.write(w, b'fresh')
        r.start()
        t0 = time.monotonic()
        while r.total < 5 and time.monotonic() - t0 < 5:
            time.sleep(0.02)
        got = r.finish()
        os.close(w)
        assert got == b'fresh', got
    finally:
        for f in os.listdir(base):
            os.unlink(os.path.join(base, f))
        os.rmdir(base)


def set_setting(key, value):
    """The private `settings` metadata only (pw-metadata runs in the private environment)."""
    r = tool('pw-metadata', '-n', 'settings', '0', key, str(value))
    if r.returncode != 0:
        raise Fail('pw-metadata %s %s failed: %s' % (key, value, r.stderr.strip()))


def setting_now(key):
    """The `settings` metadata value as pw-dump lists it (None when the key is not there)."""
    for o in dump():
        if o['type'].endswith('Metadata') and (o.get('props') or {}).get('metadata.name') == 'settings':
            for m in o.get('metadata') or []:
                if m.get('key') == key:
                    return m.get('value')
    return None


SVOLDER = os.path.normpath(os.path.join(HERE, '../../target/debug/svolder'))
NULL_SINK = 'svolder_e2e_sink'


def default_now():
    """The `default` metadata's default.audio.sink as a node name (None when unset)."""
    for o in dump():
        if o['type'].endswith('Metadata') and (o.get('props') or {}).get('metadata.name') == 'default':
            for m in o.get('metadata') or []:
                if m.get('key') == 'default.audio.sink':
                    v = m.get('value')
                    if isinstance(v, str):
                        try:
                            v = json.loads(v)
                        except ValueError:
                            return None
                    return (v or {}).get('name') if isinstance(v, dict) else None
    return None


def set_default(name):
    """Make `name` the default sink in the private `default` metadata (both the configured and the live key
    are written, and the live one is read back)."""
    value = json.dumps({'name': name})
    for key in ('default.configured.audio.sink', 'default.audio.sink'):
        r = tool('pw-metadata', '-n', 'default', '0', key, value, 'Spa:String:JSON')
        if r.returncode != 0:
            raise Fail('pw-metadata %s failed: %s' % (key, r.stderr.strip()))
    wait_until('the default sink to read %s' % name, lambda: default_now() == name, 5)


def svolder_env(run):
    """The private instance's environment for the CLI: its own XDG homes, no fake dump, no daemon, no
    systemctl, no internet."""
    env = {k: v for k, v in os.environ.items() if not k.startswith('SVOLDER_')}
    script = run + '/no-systemctl'
    with open(script, 'w') as f:
        f.write('#!/bin/false\n')
    os.chmod(script, 0o755)
    env.update({'HOME': run, 'SVOLDER_TRANSPORT': 'fake', 'SVOLDER_ADAPTER': 'hci-none',
                'SVOLDER_SYSTEMCTL': script, 'SVOLDER_AUTOEQ_URL': 'http://127.0.0.1:9/'})
    return env


def host_check(run, player=None):
    """`svolder host check --json` (the built binary, never an installed one) on the private graph. Returns
    (parsed JSON, wall seconds); raises Fail when it does not exit 0 with JSON."""
    argv = [SVOLDER, 'host', 'check', '--json'] + (['--player', player] if player else [])
    t0 = time.monotonic()
    r = subprocess.run(argv, capture_output=True, text=True, timeout=30, env=svolder_env(run), cwd=run)
    took = time.monotonic() - t0
    if r.returncode != 0:
        raise Fail('svolder host check exited %d: %s%s' % (r.returncode, r.stdout.strip(), r.stderr.strip()))
    try:
        return json.loads(r.stdout), took
    except ValueError:
        raise Fail('svolder host check printed no JSON: %r' % r.stdout[:200])


def show_words(j, took):
    """One line a reader can check: the verdict, the path's words, the summary and each line's outcome."""
    path = j.get('path') or {}
    cfg = j.get('config') or {}
    lines = ' '.join('%s=%s' % (l['key'], l['outcome']) for l in cfg.get('lines') or [])
    say('  svolder: %s [%s] %s; %s; %s (%.2f s)' % (
        path.get('verdict'), ' | '.join(path.get('words') or []), j.get('sink'), cfg.get('summary'), lines, took))


def scrub(text, run):
    """Fixture text: user, host and paths replaced as pw-dump/README.md says."""
    me = os.environ.get('USER') or ''
    host = os.uname().nodename
    for old, new in ((run, '/run/user/1000/svolder-e2e'), (HERE + '/pipewire.conf', '/path/to/pipewire.conf'),
                     (HERE, '/path/to'), (run + '/bits.conf', '/path/to/bits.conf')):
        text = text.replace(old, new)
    for old, new in ((host, 'host'), (me, 'user')):
        if old:
            # whole words: the user name inside a core name, never a word that merely contains it
            text = re.sub(r'(?<![A-Za-z0-9])%s(?![A-Za-z0-9])' % re.escape(old), new, text)
    return text


def capture_fixture(run, outdir):
    """HOST_E2E_PWDUMP_DIR: one pw-dump of the S16LE+dither sink playing a `Fixture Player` S16 stream with
    dither.noise 2, next to the filter-graph sink, with allowed-rates [ 44100 48000 96000 ] and force-rate 96000."""
    say('capture: setup-pipe.json into ' + outdir)
    set_setting('clock.allowed-rates', '[ 44100 48000 96000 ]')
    set_setting('clock.force-rate', 96000)
    src = run + '/bits-capture.raw'
    with open(src, 'wb') as f:
        f.write(source_bytes(pattern(16, RATE * 4, 2, SEED), 16))
    reader = Reader(run + '/bits_s16_tri.pipe')
    reader.start()
    player = None
    try:
        player = spawn('pw-cat', '-p', '--raw', '--rate', str(RATE), '--channels', '2', '--format', 's16',
                       '--target', 'bits_s16_tri', '-P',
                       '{ node.name = bits_player application.name = "Fixture Player" dither.noise = 2 }', src)
        wait_until('the sink running with the player', lambda: sink_format('bits_s16_tri') and node('bits_player'), 5)
        text = tool('pw-dump').stdout
        json.loads(text)
        with open(os.path.join(outdir, 'setup-pipe.json'), 'w') as fh:
            fh.write(scrub(text, run))
    finally:
        reader.finish()
        if player is not None:
            stop(player)
        set_setting('clock.force-rate', 0)
        set_setting('clock.allowed-rates', '[ 44100 48000 ]')


def null_sink_phase(run):
    """The null sink, idle, as the default: its rates line follows `clock.allowed-rates` and its forced-rate
    line follows `clock.force-rate` (both in the private `settings` metadata, put back after), every other
    line passing."""
    say('case: the null sink, rates and forced rate from the private settings')
    set_default(NULL_SINK)
    full = '[ ' + ' '.join(str(r) for r in (44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000)) + ' ]'
    steps = [  # (what, allowed-rates, force-rate, rates line, forced-rate line, words the rates line holds)
        ('every standard rate allowed, no force', full, 0, 'pass', 'pass', None),
        ('allowed [ 44100 48000 ]', '[ 44100 48000 ]', 0, 'fail', 'pass', 'resampled (allowed: 44.1, 48)'),
        ('every rate allowed, clock.force-rate 96000', full, 96000, 'pass', 'fail', None),
        ('put back: allowed [ 48000 ], no force', '[ 48000 ]', 0, 'fail', 'pass', 'resampled (allowed: 48)'),
    ]
    try:
        for what, allowed, force, want_rates, want_forced, words in steps:
            set_setting('clock.allowed-rates', allowed)
            set_setting('clock.force-rate', force)
            ar, fr = setting_now('clock.allowed-rates'), setting_now('clock.force-rate')
            check(isinstance(ar, str) and ar.split() == allowed.split() and fr == force,
                  'settings read back: allowed-rates %r, force-rate %r (%s)' % (ar, fr, what))
            j, took = host_check(run)
            show_words(j, took)
            cfg = j.get('config') or {}
            lines = {l['key']: l for l in cfg.get('lines') or []}
            bad = []
            if (j.get('path') or {}).get('verdict') is None or j.get('sink') != 'svolder e2e sink':
                bad.append('the sink is %r' % j.get('sink'))
            for key in SETUP_KEYS:
                want = {'rates': want_rates, 'forced_rate': want_forced}.get(key, 'pass')
                if (lines.get(key) or {}).get('outcome') != want:
                    bad.append('line %s is %s, expected %s' % (key, (lines.get(key) or {}).get('outcome'), want))
            if words and words not in (lines.get('rates') or {}).get('words', ''):
                bad.append('the rates line does not say %r' % words)
            if want_forced == 'fail' and '96 kHz forced' not in (lines.get('forced_rate') or {}).get('words', ''):
                bad.append('the forced-rate line does not say 96 kHz forced')
            check(not bad, 'null sink, %s%s' % (what, '' if not bad else ': ' + '; '.join(bad)))
    finally:
        for key, value in (('clock.force-rate', 0), ('clock.allowed-rates', '[ 44100 48000 ]')):
            try:
                set_setting(key, value)
            except Fail as e:
                say('  note: %s' % e)


def run_case(run, c, idx):
    frames = c.rate * SECONDS
    x = pattern(c.bits, frames, c.channels, SEED + idx)
    src_path = '%s/bits-src-%d.raw' % (run, idx)
    with open(src_path, 'wb') as f:
        f.write(source_bytes(x, c.bits))
        # Silence after the pattern: a player that exits as soon as it has sent its last sample can have
        # the stream destroyed before the sink has pulled it (a busy machine shows this), so the pattern
        # is never the last thing in the stream.
        f.write(bytes(8192 * c.channels * (2 if c.bits == 16 else 4)))
    fifo = '%s/%s.pipe' % (run, c.sink)
    sink_fmt, sink_rate = SINKS[c.sink]
    forced_ok = None
    if c.force is not None:
        set_setting('clock.force-rate', c.force)
        forced_ok = setting_now('clock.force-rate')
    set_default(c.sink)
    reader = Reader(fifo)
    reader.drain()
    reader.start()
    player = None
    fmt = vol = pprops = hc = None
    try:
        props = 'node.name = bits_player application.name = "Fixture Player"' + (' ' + c.props if c.props else '')
        argv = ['pw-cat', '-p', '--raw', '--rate', str(c.rate), '--channels', str(c.channels),
                '--format', STREAM_FMT[c.bits], '--target', c.sink, '--volume', '1.0',
                '-P', '{ ' + props + ' }', src_path]
        player = spawn(*argv)
        t0 = time.monotonic()
        while time.monotonic() - t0 < 1.5 and player.poll() is None:
            fmt = sink_format(c.sink)
            pprops = params_of(node('bits_player'))
            if fmt and (c.noise is None or 'dither.noise' in pprops):
                break
            time.sleep(0.05)
        if player.poll() is None:
            try:
                hc = host_check(run, 'Fixture Player')
            except (Fail, subprocess.TimeoutExpired) as e:
                hc = str(e)
        vol = sink_volume(c.sink)
        try:
            player.wait(SECONDS + 15)
        except subprocess.TimeoutExpired:
            raise Fail('pw-cat did not end for ' + c.label)
        # The sink goes on writing silence after the pattern: wait (a condition, not a fixed time, so a busy
        # machine cannot cut the tail) until what was read holds the pattern and a margin after its first sound.
        t0 = time.monotonic()
        while time.monotonic() - t0 < 8 and not has_tail(b''.join(reader.chunks), sink_fmt, frames):
            time.sleep(0.2)
    finally:
        raw = reader.finish()
        if player is not None:
            stop(player)
        if c.force is not None:
            try:
                set_setting('clock.force-rate', 0)
            except Fail as e:
                say('  note: could not reset clock.force-rate: %s' % e)
    if FLIP:
        raw = flip_one(raw, sink_fmt)
    out = decode(raw, sink_fmt, 2)
    res = compare(out, dom32(x, c.bits))
    return res, fmt, vol, pprops, forced_ok, hc


def control_ok(c, res):
    """A negative control passes only with full-length sound AND its own difference signature."""
    if res['exact']:
        return False, 'it came out exact'
    if res['why'] is not None:
        return False, 'the capture is not a full-length sound (%s)' % res['why']
    return c.sig(res)


def main():
    run = private_env()
    if not os.access(SVOLDER, os.X_OK):
        refuse('no %s (make host-e2e-bits builds it)' % SVOLDER)
    proof = tool('bash', LAUNCHER, 'proof')
    if proof.returncode != 0:
        refuse('proof failed, nothing plays: ' + proof.stdout + proof.stderr)
    say(proof.stdout.strip())
    say('host-e2e-bits: pw-cat and pipewire as the private instance; seed %d, %d s patterns' % (SEED, SECONDS))
    with open(os.path.join(HERE, 'bits.conf')) as f:
        conf = f.read().replace('@RUN@', run)
    with open(run + '/bits.conf', 'w') as f:
        f.write(conf)
    spawn('pipewire', '-c', run + '/bits.conf', log=run + '/bits-pipewire.log')
    wait_until('the pipe sinks', lambda: all(node(n) for n in SINKS), 15)
    say('the %d pipe sinks are up (one client, its own pid)' % len(SINKS))
    try:
        check(setting_now('clock.force-rate') == 0, 'clock.force-rate starts unforced (%s)' % setting_now('clock.force-rate'))
        # the graph follows the stream's rate between these two; read back below
        set_setting('clock.allowed-rates', '[ 44100 48000 ]')
        ar = setting_now('clock.allowed-rates')
        check(isinstance(ar, str) and ar.split() == ['[', '44100', '48000', ']'], 'clock.allowed-rates is [ 44100 48000 ] (%r)' % (ar,))
        for idx, c in enumerate(CASES):
            say('case: ' + c.label)
            res, fmt, vol, pprops, forced_ok, hc = run_case(run, c, idx)
            sink_fmt, sink_rate = SINKS[c.sink]
            say('  start frame %s, %d of %d samples differ, largest gap %d steps of 24 bits%s'
                % (res['start'], res['differing'], res['total'], res['max_lsb'], (', ' + res['why']) if res['why'] else ''))
            check(fmt == (sink_fmt, sink_rate, 2), 'the sink read back %s at %d Hz while playing (%s)' % (sink_fmt, sink_rate, fmt))
            check(vol[0] == [1.0, 1.0] and vol[1] is False, 'the sink volume is 1.0 and not muted (%s)' % (vol,))
            if c.noise is not None:
                check(pprops.get('dither.noise') == c.noise, 'the player\'s own converter reads dither.noise %s (%s)' % (c.noise, pprops.get('dither.noise')))
            if c.force is not None:
                check(forced_ok == c.force, 'clock.force-rate read back as %s (%s)' % (c.force, forced_ok))
            if c.exact:
                check(res['exact'], 'exact: %s' % c.label)
            else:
                ok, why = control_ok(c, res)
                check(ok, 'NOT exact, in its own way (%s): %s' % (why, c.label))
            if hc is None:
                check(False, 'the player ended before svolder host check could read it: %s' % c.label)
            elif isinstance(hc, str):
                check(False, 'svolder host check failed (%s): %s' % (hc, c.label))
            else:
                show_words(*hc)
                bad = judge_words(c, res['exact'], hc[0])
                check(not bad, 'svolder host check says it as it is%s: %s' % ('' if not bad else ' (' + '; '.join(bad) + ')', c.label))
        null_sink_phase(run)
        if os.environ.get('HOST_E2E_PWDUMP_DIR'):
            capture_fixture(run, os.environ['HOST_E2E_PWDUMP_DIR'])
    finally:
        # leave the private settings as found, and say if that failed
        for key, value in (('clock.force-rate', 0), ('clock.allowed-rates', '[ 48000 ]')):
            try:
                set_setting(key, value)
            except Fail as e:
                say('  note: %s' % e)
    check(setting_now('clock.force-rate') == 0, 'clock.force-rate is back to 0 (%s)' % setting_now('clock.force-rate'))
    ar = setting_now('clock.allowed-rates')
    check(isinstance(ar, str) and ar.split() == ['[', '48000', ']'], 'clock.allowed-rates is back to [ 48000 ] (%r)' % (ar,))
    if STATE['failed']:
        raise Fail('%d check(s) failed: %s' % (len(STATE['failed']), '; '.join(STATE['failed'])))
    say('host-e2e-bits: %d rows, %d exact and %d negative controls, all as expected' % (
        len(CASES), sum(1 for c in CASES if c.exact), sum(1 for c in CASES if not c.exact)))


def _on_term(*_):
    raise KeyboardInterrupt


if __name__ == '__main__':
    if sys.argv[1:] == ['--selftest']:
        selftest()
        sys.exit(0)
    signal.signal(signal.SIGTERM, _on_term)
    code = 0
    try:
        main()
    except Fail as e:
        print('host-e2e-bits: FAIL: %s' % e, file=sys.stderr)
        code = 1
    except KeyboardInterrupt:
        print('host-e2e-bits: interrupted', file=sys.stderr)
        code = 130
    finally:
        cleanup()
    sys.exit(code)
