# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The VU phase's pieces: the signals it plays, the levels-only frames reader and the byte maths.

Pure Python and numpy with no PipeWire in sight, so `vu_selftest.py` checks them alone; analyse.py
imports this and does the playing and recording."""
import http.client
import json
import math
import socket
import threading
import time
import wave

import numpy as np

RATE = 48000
# The level bytes map -72..0 dBFS linearly over 0..255.
FLOOR_DB = 72.0


def byte_db(b):
    """The dBFS a level byte stands for."""
    return b * FLOOR_DB / 255.0 - FLOOR_DB


def sine(rms_dbfs=None, peak_dbfs=None, hz=1000.0, secs=1):
    """A sine at 48 kHz as float samples, of the given RMS or the given peak level (one of them)."""
    if (rms_dbfs is None) == (peak_dbfs is None):
        raise ValueError('give the rms or the peak, one of them')
    peak = 10 ** (peak_dbfs / 20) if peak_dbfs is not None else 10 ** (rms_dbfs / 20) * math.sqrt(2)
    n = np.arange(RATE * secs)
    return peak * np.sin(2 * np.pi * hz * n / RATE)


def square(peak_dbfs, hz=1000.0, secs=1):
    """A square wave at 48 kHz (whole samples per half cycle) at the given peak level."""
    half = int(RATE / hz / 2)
    n = np.arange(RATE * secs)
    return 10 ** (peak_dbfs / 20) * np.where((n // half) % 2 == 0, 1.0, -1.0)


def write_wav(path, x, secs=40):
    """A stereo 16-bit WAV of `x` (one second of both channels, repeated `secs` times): 1.0 is
    32767, the largest sample the format holds."""
    s = np.clip(np.round(x * 32767), -32767, 32767).astype('<i2')
    block = np.stack([s, s], 1).tobytes()
    w = wave.open(path, 'wb')
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(RATE)
    for _ in range(secs):
        w.writeframes(block)
    w.close()


def sse_events(lines):
    """(event name, data text) of each server-sent event in an iterable of decoded lines;
    comments and `retry:` lines are skipped."""
    name, data = 'message', []
    for raw in lines:
        line = raw.rstrip('\r\n')
        if line == '':
            if data:
                yield name, '\n'.join(data)
            name, data = 'message', []
        elif line.startswith(':') or line.startswith('retry:'):
            continue
        elif line.startswith('event:'):
            name = line[6:].strip()
        elif line.startswith('data:'):
            data.append(line[5:].lstrip(' '))


class Levels:
    """The daemon's levels-only frames (`GET api/v1/host/frames?only=levels`) read on a thread of
    its own; `frames` is a list of (monotonic time, the frame's JSON object)."""

    def __init__(self, port, token):
        self.port = port
        self.frames = []
        self.events = []
        origin = 'http://127.0.0.1:%d' % port
        c = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
        c.request('POST', '/login', json.dumps({'token': token}),
                  {'Content-Type': 'application/json', 'Origin': origin})
        r = c.getresponse()
        r.read()
        cookie = (r.getheader('Set-Cookie') or '').split(';')[0]
        if r.status != 200 or '=' not in cookie:
            raise RuntimeError('the web login answered %s' % r.status)
        c.close()
        self.conn = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
        self.conn.request('GET', '/api/v1/host/frames?only=levels', headers={'Cookie': cookie})
        self.sock = self.conn.sock
        self.resp = self.conn.getresponse()
        if self.resp.status != 200:
            raise RuntimeError('the frames stream answered %s' % self.resp.status)
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        try:
            lines = (b.decode('utf-8', 'replace') for b in self.resp)
            for name, data in sse_events(lines):
                try:
                    obj = json.loads(data)
                except ValueError:
                    continue
                now = time.monotonic()
                self.events.append((now, name, obj))
                if name == 'frame':
                    self.frames.append((now, obj))
        except (OSError, ValueError, AttributeError, http.client.HTTPException):
            return

    def since(self, t0):
        """The frames received at or after monotonic time `t0`."""
        return [f for t, f in list(self.frames) if t >= t0]

    def close(self):
        # shut the socket first: the reader thread wakes at once and the daemon sees the stream gone
        for end in (lambda: self.sock.shutdown(socket.SHUT_RDWR), lambda: self.thread.join(2),
                    self.resp.close, self.conn.close):
            try:
                end()
            except (OSError, ValueError):
                pass


def window_stats(frames):
    """Per channel over `frames` (levels-only objects): the median RMS byte and the largest and
    smallest peak byte: {'rms': (L, R), 'peak_max': (L, R), 'peak_min': (L, R)}."""
    lv = [f['lv'] for f in frames]
    if not lv:
        raise ValueError('no frames')
    a = np.array(lv, dtype=float)
    return {'rms': (float(np.median(a[:, 0])), float(np.median(a[:, 1]))),
            'peak_max': (int(a[:, 2].max()), int(a[:, 3].max())),
            'peak_min': (int(a[:, 2].min()), int(a[:, 3].min()))}


def only_levels_shape(frames):
    """Whether every frame has exactly `t` and `lv`, `lv` four bytes."""
    return bool(frames) and all(set(f) == {'t', 'lv'} and len(f['lv']) == 4
                                and all(isinstance(b, int) and 0 <= b <= 255 for b in f['lv'])
                                for f in frames)
