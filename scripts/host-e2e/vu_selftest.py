# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Checks vu.py alone (no PipeWire, no daemon): python3 -I scripts/host-e2e/vu_selftest.py"""
import http.server
import json
import os
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np  # noqa: E402

import vu  # noqa: E402


def to_byte(db):
    return int(round((db + 72.0) / 72.0 * 255.0))


def rms_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x ** 2)))


def main():
    # the byte maths: -18 dB is byte 191 and reads back within a byte's step
    assert to_byte(-18.0) == 191 and abs(vu.byte_db(191) + 18.0) < 0.3
    assert vu.byte_db(0) == -72.0 and vu.byte_db(255) == 0.0
    # the signals
    s = vu.sine(rms_dbfs=-18.0)
    assert abs(rms_db(s) + 18.0) < 0.001 and abs(np.abs(s).max() - 10 ** (-15 / 20)) < 1e-3
    s = vu.sine(peak_dbfs=-0.1)
    assert abs(np.abs(s).max() - 10 ** (-0.1 / 20)) < 1e-9
    q = vu.square(0.0)
    assert set(np.unique(q)) == {-1.0, 1.0} and q[:24].min() == 1.0 and q[24] == -1.0
    # an int16 square at 0 dBFS is full scale 32767 and at -0.1 dBFS stays under the over line
    import tempfile
    import wave
    os.makedirs('/tmp/svolder', exist_ok=True)
    with tempfile.TemporaryDirectory(dir='/tmp/svolder') as d:
        for name, x, want in (('a', vu.square(0.0), 32767), ('b', vu.square(-0.1), 32392)):
            p = d + '/' + name + '.wav'
            vu.write_wav(p, x, secs=1)
            w = wave.open(p)
            a = np.frombuffer(w.readframes(w.getnframes()), '<i2')
            assert w.getnchannels() == 2 and a.max() == want and a.min() == -want, (name, a.max())
    # the SSE parser
    text = ['retry: 2000\n', '\n', 'event: hello\n', 'data: {"now_ms":1}\n', '\n', ': keep\n', '\n',
            'event: frame\n', 'data: {"t":5,"lv":[1,2,3,4]}\n', '\n']
    got = list(vu.sse_events(text))
    assert got == [('hello', '{"now_ms":1}'), ('frame', '{"t":5,"lv":[1,2,3,4]}')], got
    # the statistics and the shape check
    fr = [{'t': i, 'lv': [191, 190, 200 + i, 255]} for i in range(5)]
    st = vu.window_stats(fr)
    assert st['rms'] == (191.0, 190.0) and st['peak_max'] == (204, 255) and st['peak_min'] == (200, 255)
    assert vu.only_levels_shape(fr) and not vu.only_levels_shape(fr + [{'t': 1, 'lv': [1, 2, 3, 4], 'l': 'x'}])
    assert not vu.only_levels_shape([]) and not vu.only_levels_shape([{'t': 1, 'lv': [1, 2, 3]}])

    # Levels against a stub server: login with an Origin, the cookie sent back, frames read
    seen = {}

    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_POST(self):
            n = int(self.headers['Content-Length'])
            seen['login'] = (json.loads(self.rfile.read(n)), self.headers.get('Origin'))
            self.send_response(200)
            self.send_header('Set-Cookie', 'svolder_web_1=abc; HttpOnly')
            self.send_header('Content-Length', '2')
            self.end_headers()
            self.wfile.write(b'{}')

        def do_GET(self):
            seen['get'] = (self.path, self.headers.get('Cookie'))
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            self.wfile.write(b'retry: 2000\n\nevent: hello\ndata: {"now_ms":1}\n\n'
                             b'event: frame\ndata: {"t":9,"lv":[10,11,12,13]}\n\n')
            self.wfile.flush()
            time.sleep(0.5)

    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    port = srv.server_address[1]
    t0 = time.monotonic()
    lv = vu.Levels(port, 'tok')
    for _ in range(50):
        if lv.frames:
            break
        time.sleep(0.05)
    lv.close()
    srv.shutdown()
    assert seen['login'] == ({'token': 'tok'}, 'http://127.0.0.1:%d' % port), seen
    assert seen['get'] == ('/api/v1/host/frames?only=levels', 'svolder_web_1=abc'), seen
    assert lv.since(t0) == [{'t': 9, 'lv': [10, 11, 12, 13]}], lv.frames
    print('vu_selftest: ok')


main()
