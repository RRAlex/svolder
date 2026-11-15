# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Closing the terminal during a daemon walk: `svolder add` is at the walk's name step under a
pty, the pty's master is closed (a hangup, as a closed window or a dropped ssh session does), and
the client must leave as a cancelled walk (exit 130, or SIGHUP's own death), never by SIGABRT or a
panic, which is what restoring a dead terminal with a printing call does. The daemon is a fake
with no device configured, in temp homes; nothing real is touched.

    python3 scripts/tui-hangup-check.py [rounds]    (make tui-hangup-check)

Also checked: the daemon shows the walk `cancelled`, and a SIGTERM with the terminal alive still
leaves with 130."""
import os
import pty
import select
import shutil
import signal
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tmp_base  # noqa: E402
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), 'web-checks'))
import common  # noqa: E402

OK_EXITS = (130, -signal.SIGHUP)


def read_until(fd, needle, seconds=15):
    seen = b''
    end = time.time() + seconds
    while time.time() < end:
        if select.select([fd], [], [], 0.2)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            seen += chunk
            if needle in seen:
                return seen
    raise RuntimeError(f'never saw {needle!r}; the screen ended: {seen[-300:]!r}')


def to_the_name_step(daemon):
    pid, fd = pty.fork()
    if pid == 0:
        # the daemon's own environment alone (its temp homes), never the caller's
        env = dict(daemon._environ, TERM='xterm-256color')
        os.execve(common.SVOLDER, [common.SVOLDER, '--socket', f'{daemon.dir}/run/svolder.sock',
                                   'add', '--name', 'Desk'], env)
    import fcntl
    import struct
    import termios
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
    read_until(fd, b'Setup')
    os.write(fd, b'\r')
    read_until(fd, b'dBm')
    os.write(fd, b'\r')
    read_until(fd, b'takes the name')
    return pid, fd


def code_of(pid):
    for _ in range(100):
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.1)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    raise RuntimeError('svolder did not leave')


def main():
    # A socket path holds 108 bytes, and a deep checkout's target/ passes them: the daemons' homes
    # go under a short directory of their own.
    short = tmp_base.mkdtemp('svh')
    common.CHECKS = short
    try:
        run(short)
    finally:
        shutil.rmtree(short, ignore_errors=True)


def run(short):
    rounds = int(sys.argv[1]) if len(sys.argv) > 1 else 3
    failed = []
    for n in range(rounds):
        daemon = common.Daemon(blank=True)
        try:
            pid, fd = to_the_name_step(daemon)
            os.close(fd)
            code = code_of(pid)
            states = sorted(s['kind'] for s in daemon.walk_states().values())
            ok = code in OK_EXITS and states == ['cancelled']
            print(f'hangup {n + 1}: exit {code}, walk {states}: {"ok" if ok else "FAIL"}')
            if not ok:
                failed.append(n)
        finally:
            daemon.stop()
    daemon = common.Daemon(blank=True)
    try:
        pid, fd = to_the_name_step(daemon)
        os.kill(pid, signal.SIGTERM)
        code = code_of(pid)
        os.close(fd)
        ok = code == 130
        print(f'sigterm: exit {code}: {"ok" if ok else "FAIL"}')
        if not ok:
            failed.append('sigterm')
    finally:
        daemon.stop()
    sys.exit(1 if failed else 0)


main()
