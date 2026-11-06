# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Shared helpers for the browser checks: a throwaway fake daemon, headless Chromium over CDP,
headless Firefox over geckodriver's W3C WebDriver HTTP. Profiles and copies of the web dir live
under target/web-checks/ (both browsers are snaps and cannot see /tmp). Nothing here touches the
user's browser, the installed binaries or ports 8737/8738. Env: SMOKE_WEB (the web dir to serve,
default the tree's), SVOLDERD (a daemon binary, default target/debug/svolderd), SMOKE_EMBEDDED=1
(serve the binary's own copies of the files)."""
import asyncio
import atexit
import base64
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

# the browsers are snaps here, found on PATH when they are there
CHROMIUM = shutil.which('chromium') or '/snap/bin/chromium'
GECKODRIVER = shutil.which('geckodriver') or '/snap/bin/geckodriver'
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CHECKS = f'{REPO}/target/web-checks'
WEB = os.environ.get('SMOKE_WEB', f'{REPO}/bins/svolderd/web')
SVOLDERD = os.environ.get('SVOLDERD', f'{REPO}/target/debug/svolderd')
SVOLDER = os.environ.get('SVOLDER', f'{REPO}/target/debug/svolder')
# Read the page's files from the tree (the developer's switch), unless the run asks for the
# binary's own copies (SMOKE_EMBEDDED=1).
LIVE_ASSETS = os.environ.get('SMOKE_EMBEDDED') != '1' and os.environ.get('SVOLDERD') is None

os.makedirs(CHECKS, exist_ok=True)

# every browser started, closed at exit even when a check dies
_live = []
atexit.register(lambda: [b.close() for b in list(_live)])
# a TERM (the runner's, or a timeout's) runs the atexit above instead of leaving browsers and
# daemons behind
signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))


def free_port():
    s = socket.socket()
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
    s.close()
    return port


def scratch(prefix):
    return tempfile.mkdtemp(prefix=prefix, dir=CHECKS)


def remove_tree(path):
    """Delete a scratch directory, trying again while something still writes into it (a browser's
    helper process that outlives its parent for a moment)."""
    for _ in range(50):
        shutil.rmtree(path, ignore_errors=True)
        if not os.path.exists(path):
            return
        time.sleep(0.1)


def kill_using(path):
    """Kill every process that has `path` on its command line (a browser's helper processes, which
    leave the browser's process group, carry its profile's path), and wait until they are gone."""
    for _ in range(50):
        found = []
        for pid in os.listdir('/proc'):
            if not pid.isdigit() or int(pid) == os.getpid():
                continue
            try:
                with open(f'/proc/{pid}/cmdline', 'rb') as f:
                    if path.encode() in f.read():
                        found.append(int(pid))
            except OSError:
                continue
        if not found:
            return
        for pid in found:
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass
        time.sleep(0.1)


def reap(proc, secs=10):
    """Ask a process to end, kill it when it does not, and wait for it either way; its whole
    process group goes with it when it was started as a group leader."""
    proc.terminate()
    try:
        proc.wait(secs)
    except Exception:
        proc.kill()
        proc.wait()
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except OSError:
        pass


class Daemon:
    """A fake daemon with its web UI on a free loopback port, homes under one temp dir. `host` is
    the daemon's SVOLDER_HOST: `off` (the default: no player, no card on the page) or `fake` (a
    fake player with a cover, and a route on a Schiit sink named for the device). `drop()` takes
    the first device off the air on the check's own cue."""

    def __init__(self, profile='gungnir2-v2', env=None, name='Gungnir 2', web='auth = "none"',
                 more=(), host='off', fleet=()):
        self.dir = scratch('daemon-')
        self.port = free_port()
        home = self.dir
        # `fleet` is `[(profile, name, udid)]`: devices beside the first, each a `[[device]]` block
        # and a fake of its profile (SVOLDER_FAKE_MORE); `more` is `[(name, udid)]` for blocks
        # alone
        more = list(more) + [(n, u) for _, n, u in fleet]
        for sub in ('config/svolder', 'state', 'data', 'cache', 'run'):
            os.makedirs(f'{home}/{sub}', exist_ok=True)
        os.chmod(f'{home}/run', 0o700)
        with open(f'{home}/config/svolder/config.toml', 'w') as f:
            others = ''.join(f'[[device]]\nname = "{n}"\nudid = "{u}"\npassphrase = "1234"\n\n'
                             for n, u in more)
            f.write(f'schema = 1\n\n[[device]]\nname = "{name}"\nudid = "0102030405060708"\n'
                    f'passphrase = "1234"\n\n{others}[web]\nenabled = true\n'
                    f'listen = "127.0.0.1:{self.port}"\n{web}\n')
        environ = {
            'PATH': os.environ['PATH'], 'HOME': os.environ['HOME'],
            'XDG_CONFIG_HOME': f'{home}/config', 'XDG_STATE_HOME': f'{home}/state',
            'XDG_DATA_HOME': f'{home}/data', 'XDG_CACHE_HOME': f'{home}/cache',
            'XDG_RUNTIME_DIR': f'{home}/run', 'SVOLDER_TRANSPORT': 'fake',
            'SVOLDER_FAKE_PROFILE': profile, 'SVOLDER_HOST': host,
        }
        if LIVE_ASSETS:
            environ['SVOLDER_WEB_ASSETS'] = WEB
        if fleet:
            environ['SVOLDER_FAKE_MORE'] = ','.join(f'{p}@{u}' for p, _, u in fleet)
        # the fake takes the first device off the air when this file appears (`drop`)
        self.gone_file = f'{home}/gone'
        environ['SVOLDER_FAKE_GONE_FILE'] = self.gone_file
        # the fake host's player loses its art, or quits, when these files appear (`drop_cover`,
        # `drop_player`)
        self.cover_gone_file = f'{home}/cover-gone'
        self.player_gone_file = f'{home}/player-gone'
        environ['SVOLDER_FAKE_COVER_GONE_FILE'] = self.cover_gone_file
        environ['SVOLDER_FAKE_PLAYER_GONE_FILE'] = self.player_gone_file
        environ.update(env or {})
        self.log = open(f'{home}/daemon.log', 'w')
        self._cmd = [SVOLDERD, '--socket', f'{home}/run/svolder.sock', '--config',
                     f'{home}/config/svolder/config.toml']
        self._environ = environ
        self.proc = subprocess.Popen(self._cmd, env=environ, stdout=self.log,
                                     stderr=subprocess.STDOUT)
        # stopped at exit even when the rest of this constructor fails
        _live.append(self)
        self.url = f'http://127.0.0.1:{self.port}/'
        for _ in range(100):
            try:
                urllib.request.urlopen(f'{self.url}assets/themes/ember/style.css', timeout=1).read()
                break
            except Exception:
                time.sleep(0.1)
        else:
            with open(f'{home}/daemon.log') as seen:
                tail = seen.read()[-400:]
            self.stop()
            raise RuntimeError(f'daemon did not answer; its log ended: {tail}')
        # wait for a reading in the device's model: a volume step, or a meter with a fraction
        dev = urllib.request.quote(name)
        for _ in range(100):
            try:
                body = urllib.request.urlopen(f'{self.url}api/v1/devices/{dev}/model').read()
                if re.search(rb'"raw":-?[0-9]', body) or re.search(
                        rb'"meters":\[[^\]]*"fraction":[0-9]', body):
                    break
            except Exception:
                pass
            time.sleep(0.1)

    def write(self, device, control, value):
        """A control write through the web API from the page's own origin."""
        req = urllib.request.Request(
            f'{self.url}api/v1/devices/{urllib.request.quote(device)}/controls/{control}',
            data=json.dumps({'value': value}).encode(), method='POST',
            headers={'Content-Type': 'application/json', 'Origin': self.url.rstrip('/')})
        urllib.request.urlopen(req, timeout=10).read()

    def model(self, device='Gungnir 2'):
        """What the model route reports of `device` now: its device model, as the page reads it."""
        url = f'{self.url}api/v1/devices/{urllib.request.quote(device)}/model'
        return json.load(urllib.request.urlopen(url, timeout=10))['device']

    def post(self, device, path, body):
        """A write through the web API from the page's own origin: `path` under the device's route
        (`eq/mode`, `controls/band1`, ...); the answer's status and JSON."""
        req = urllib.request.Request(
            f'{self.url}api/v1/devices/{urllib.request.quote(device)}/{path}',
            data=json.dumps(body).encode(), method='POST',
            headers={'Content-Type': 'application/json', 'Origin': self.url.rstrip('/')})
        try:
            with urllib.request.urlopen(req, timeout=10) as answer:
                return answer.status, json.load(answer)
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b'{}')

    def cli(self, *args):
        """The repo-built `svolder` run with this daemon's homes and config (never the installed one):
        its stdout, or the failure's words."""
        run = subprocess.run([SVOLDER, *args], env=self._environ, capture_output=True, text=True, timeout=30)
        return run.stdout if run.returncode == 0 else f'exit {run.returncode}: {run.stderr}'

    def drop(self):
        """Take the first device off the air now, as a device out of reach: its link is cut and it
        is not heard or reached again (a restart brings it back)."""
        open(self.gone_file, 'w').close()

    def drop_cover(self):
        """The fake host's player loses its cover art now, the track staying (`host='fake'`)."""
        open(self.cover_gone_file, 'w').close()

    def drop_player(self):
        """The fake host's player quits now (`host='fake'`)."""
        open(self.player_gone_file, 'w').close()

    def restart(self):
        """Stop the daemon and start it again on the same port and homes (a reconnect); a device
        that was dropped is on the air again."""
        if os.path.exists(self.gone_file):
            os.remove(self.gone_file)
        self.proc.terminate()
        try:
            self.proc.wait(5)
        except Exception:
            self.proc.kill()
        self.proc = subprocess.Popen(self._cmd, env=self._environ, stdout=self.log,
                                     stderr=subprocess.STDOUT)
        for _ in range(100):
            try:
                urllib.request.urlopen(f'{self.url}assets/themes/ember/style.css', timeout=1).read()
                return
            except Exception:
                time.sleep(0.1)
        raise RuntimeError('daemon did not come back')

    def stop(self):
        if self not in _live:
            return
        _live.remove(self)
        self.proc.terminate()
        try:
            self.proc.wait(5)
        except Exception:
            self.proc.kill()
        self.log.close()
        shutil.rmtree(self.dir, ignore_errors=True)

    close = stop


# ---------------------------------------------------------------- Chromium over CDP

KEYS = {
    'Enter': ('Enter', 'Enter', 13, '\r'), 'Escape': ('Escape', 'Escape', 27, ''),
    'ArrowDown': ('ArrowDown', 'ArrowDown', 40, ''), 'ArrowUp': ('ArrowUp', 'ArrowUp', 38, ''),
    'ArrowLeft': ('ArrowLeft', 'ArrowLeft', 37, ''),
    'ArrowRight': ('ArrowRight', 'ArrowRight', 39, ''),
    'PageUp': ('PageUp', 'PageUp', 33, ''), 'PageDown': ('PageDown', 'PageDown', 34, ''),
    'Home': ('Home', 'Home', 36, ''), 'End': ('End', 'End', 35, ''), 'Tab': ('Tab', 'Tab', 9, ''),
    ' ': (' ', 'Space', 32, ' '), 'm': ('m', 'KeyM', 77, 'm'), '?': ('?', 'Slash', 191, '?'),
    'a': ('a', 'KeyA', 65, 'a'), 'A': ('A', 'KeyA', 65, 'A'), 'q': ('q', 'KeyQ', 81, 'q'), 'v': ('v', 'KeyV', 86, 'v'),
    't': ('t', 'KeyT', 84, 't'), 'T': ('T', 'KeyT', 84, 'T'),
}


class Chromium:
    """Headless Chromium driven over CDP with python's websockets."""
    name = 'chromium'

    def __init__(self, size=(1280, 900), reduced=False, block_cookies=False, dpr=1, args=(),
                 touch=False):
        import websockets  # noqa: F401
        self.profile = scratch('chromium-')
        if block_cookies:
            os.makedirs(f'{self.profile}/Default', exist_ok=True)
            with open(f'{self.profile}/Default/Preferences', 'w') as f:
                json.dump({'profile': {'default_content_setting_values': {'cookies': 2}}}, f)
        self.proc = subprocess.Popen(
            [CHROMIUM, '--headless', '--remote-debugging-port=0',
             f'--user-data-dir={self.profile}', '--no-first-run', '--no-default-browser-check',
             '--hide-scrollbars',
             f'--window-size={size[0]},{size[1]}', *args, 'about:blank'],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        # From here close() can reap the process, so an exit (or a TERM) while the port is awaited,
        # or a failure while it connects, leaves nothing behind.
        self.size, self.reduced, self.dpr, self.touch = size, reduced, dpr, touch
        self.errors = []
        self.warnings = []
        _live.append(self)
        try:
            port_file = f'{self.profile}/DevToolsActivePort'
            for _ in range(200):
                if os.path.exists(port_file) and open(port_file).read().count('\n') >= 1:
                    break
                time.sleep(0.1)
            self.port = int(open(port_file).read().split('\n')[0])
            self.loop = asyncio.new_event_loop()
            self.loop.run_until_complete(self._connect())
        except BaseException:
            self.close()
            raise

    async def _connect(self):
        import websockets
        for _ in range(100):
            try:
                pages = json.load(urllib.request.urlopen(f'http://127.0.0.1:{self.port}/json'))
                page = next(p for p in pages if p['type'] == 'page')
                break
            except Exception:
                await asyncio.sleep(0.1)
        self.ws = await websockets.connect(page['webSocketDebuggerUrl'], max_size=None)
        self.next_id = 0
        self.waiting = {}
        self.events = []
        self.reader = asyncio.ensure_future(self._read())
        for domain in ('Page', 'Runtime', 'Log', 'Network'):
            await self._send(f'{domain}.enable')
        await self._apply_metrics()

    async def _apply_metrics(self):
        await self._send('Emulation.setDeviceMetricsOverride', {
            'width': self.size[0], 'height': self.size[1],
            'deviceScaleFactor': self.dpr, 'mobile': self.touch})
        await self._send('Emulation.setTouchEmulationEnabled',
                         {'enabled': True, 'maxTouchPoints': 5} if self.touch
                         else {'enabled': False})
        await self._send('Emulation.setEmulatedMedia', {
            'features': [{'name': 'prefers-reduced-motion',
                          'value': 'reduce' if self.reduced else 'no-preference'}]})

    async def _read(self):
        async for raw in self.ws:
            msg = json.loads(raw)
            if 'id' in msg:
                fut = self.waiting.pop(msg['id'], None)
                if fut:
                    fut.set_result(msg)
            else:
                self.events.append(msg)
                m, p = msg['method'], msg.get('params', {})
                if m == 'Runtime.exceptionThrown':
                    d = p['exceptionDetails']
                    self.errors.append('exception: ' + (d.get('exception', {}).get('description')
                                                        or d.get('text', '')))
                elif m == 'Log.entryAdded' and p['entry']['level'] == 'error':
                    self.errors.append(
                        'log: ' + p['entry']['text'] + ' ' + p['entry'].get('url', ''),
                    )
                elif m == 'Log.entryAdded' and p['entry']['level'] == 'warning':
                    self.warnings.append(p['entry']['text'])
                elif m == 'Runtime.consoleAPICalled' and p['type'] == 'warning':
                    self.warnings.append(json.dumps(p['args'])[:300])
                elif m == 'Runtime.consoleAPICalled' and p['type'] == 'error':
                    self.errors.append('console.error: ' + json.dumps(p['args'])[:200])

    async def _send(self, method, params=None):
        self.next_id += 1
        fut = self.loop.create_future()
        self.waiting[self.next_id] = fut
        await self.ws.send(
            json.dumps({'id': self.next_id, 'method': method, 'params': params or {}}),
        )
        msg = await asyncio.wait_for(fut, 60)
        if 'error' in msg:
            raise RuntimeError(f'{method}: {msg["error"]}')
        return msg.get('result', {})

    def send(self, method, params=None):
        return self.loop.run_until_complete(self._send(method, params))

    def page(self, expression):
        """An expression's value in the page's own realm (see Firefox.page)."""
        return self.ev(expression)

    def preload(self, source):
        """Run `source` (statements) in every page from now on, before the page's own scripts."""
        self.send('Page.addScriptToEvaluateOnNewDocument', {'source': source})

    def goto(self, url, settle=0.8):
        self.events.clear()
        self.send('Page.navigate', {'url': url})
        for _ in range(200):
            self.loop.run_until_complete(asyncio.sleep(0.1))
            if any(e['method'] == 'Page.loadEventFired' for e in self.events):
                break
        self.sleep(settle)

    def sleep(self, secs):
        self.loop.run_until_complete(asyncio.sleep(secs))

    def ev(self, expression):
        r = self.send('Runtime.evaluate', {'expression': expression, 'returnByValue': True,
                                          'awaitPromise': True})
        if 'exceptionDetails' in r:
            raise RuntimeError(r['exceptionDetails'])
        return r['result'].get('value')

    def press(self, name):
        key, code, vk, text = KEYS[name]
        base = {'key': key, 'code': code, 'windowsVirtualKeyCode': vk, 'nativeVirtualKeyCode': vk}
        self.send('Input.dispatchKeyEvent', {**base, 'type': 'keyDown' if not text else 'keyDown',
                                             **({'text': text} if text else {})})
        self.send('Input.dispatchKeyEvent', {**base, 'type': 'keyUp'})
        self.sleep(0.15)

    def click(self, x, y):
        for kind in ('mouseMoved', 'mousePressed', 'mouseReleased'):
            self.send('Input.dispatchMouseEvent', {'type': kind, 'x': x, 'y': y,
                                                   'button': 'left', 'clickCount': 1})
        self.sleep(0.2)

    def resize(self, w, h):
        self.size = (w, h)
        self.loop.run_until_complete(self._apply_metrics())
        self.sleep(0.4)

    def shot(self):
        r = self.send('Page.captureScreenshot', {'format': 'png'})
        return base64.b64decode(r['data'])

    def close(self):
        if self not in _live:
            return
        # Taken off _live only once the process is reaped: a TERM during the close leaves it for the
        # exit handler to find.
        try:
            self.loop.run_until_complete(self.ws.close())
        except Exception:
            pass
        reap(self.proc)
        kill_using(self.profile)
        if self in _live:
            _live.remove(self)
        remove_tree(self.profile)


# ---------------------------------------------------------------- Firefox over geckodriver

WD_KEYS = {
    'Enter': '', 'Escape': '', 'ArrowDown': '', 'ArrowUp': '',
    'ArrowLeft': '', 'ArrowRight': '', 'Home': '', 'End': '',
    'PageUp': '', 'PageDown': '',
    'Tab': '', ' ': ' ', 'm': 'm', '?': '?', 'a': 'a', 'A': 'A', 'q': 'q', 'v': 'v',
    't': 't', 'T': 'T',
}


class Firefox:
    """Headless Firefox through geckodriver, plain W3C WebDriver over HTTP. Page errors: a
    collector installed as a BiDi preload script, so it sees errors from the first script on."""
    name = 'firefox'

    def __init__(self, size=(1280, 900), reduced=False, block_storage=False, dpr=1, extra_prefs=()):
        self.profile = scratch('firefox-')
        # `size` is in CSS pixels; headless Firefox will not make a window under 500 px wide, so
        # the viewport is set through WebDriver BiDi (browsingContext.setViewport)
        self.dpr = dpr
        # overlay scrollbars, as a phone has (Chromium gets --hide-scrollbars)
        prefs = ['user_pref("ui.useOverlayScrollbars", 1);',
                 'user_pref("widget.gtk.overlay-scrollbars.enabled", true);']
        if reduced:
            prefs.append('user_pref("ui.prefersReducedMotion", 1);')
        prefs.extend(extra_prefs)
        if block_storage:
            prefs.append('user_pref("dom.storage.enabled", false);')
        with open(f'{self.profile}/user.js', 'w') as f:
            f.write('\n'.join(prefs) + '\n')
        self.gport = free_port()
        # its BiDi websocket port is its own too (the default, 9222, is one for the whole machine);
        # 0 lets the browser pick a free one and report it in the session's webSocketUrl (this needs
        # a geckodriver that reports the real port there, as 0.37 does): a port
        # chosen here and bound later can be taken meanwhile (the log named
        # NS_ERROR_SOCKET_ADDRESS_IN_USE, and the connection then met whoever held it)
        # geckodriver's (and so the browser's) own messages go to a file in the profile, printed
        # when the session or its first BiDi call fails
        self.gd_log = f'{self.profile}/geckodriver.log'
        with open(self.gd_log, 'w') as log:
            self.gd = subprocess.Popen([GECKODRIVER, '--port', str(self.gport),
                                        '--websocket-port', '0'],
                                       stdout=log, stderr=subprocess.STDOUT,
                                       start_new_session=True)
        # From here close() can reap geckodriver and the profile's Firefox, so a failure while the
        # session is made (or the BiDi socket opened) leaves nothing behind.
        self.sid = None
        self.errors = []
        _live.append(self)
        for _ in range(100):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{self.gport}/status', timeout=1)
                break
            except Exception:
                time.sleep(0.1)
        try:
            r = self._post('/session', {'capabilities': {'alwaysMatch': {
                'browserName': 'firefox', 'webSocketUrl': True,
                'moz:firefoxOptions': {'args': ['-headless', '-profile', self.profile]}}}},
                session=False)
            self.sid = r['value']['sessionId']
            self.bidi_url = r['value']['capabilities']['webSocketUrl']
            self.size = size
            self._viewport(*size)
            self._preload()
        except Exception:
            self._report_start_failure()
            raise

    def _report_start_failure(self):
        """Print the tail of geckodriver's log and whether the browser is still there."""
        try:
            with open(self.gd_log) as f:
                tail = f.read()[-1500:]
        except OSError:
            tail = '(no log)'
        alive = self.gd.poll() is None
        print(f'firefox did not start: geckodriver {"running" if alive else "gone"}; its log ended:\n'
              f'{tail}', file=sys.stderr, flush=True)

    def _post(self, path, body=None, session=True, method='POST'):
        url = f'http://127.0.0.1:{self.gport}' + (f'/session/{self.sid}' if session else '') + path
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(url, data=data,
                                     method=method, headers={'Content-Type': 'application/json'})
        try:
            return json.load(urllib.request.urlopen(req, timeout=120))
        except urllib.error.HTTPError as e:
            raise RuntimeError(e.read().decode()[:500])

    def _preload(self):
        """Install the error collector as a WebDriver BiDi preload script (script.addPreloadScript),
        so it runs before the first script of every page from now on."""
        self.preload("window.__errs = [];"
                     " addEventListener('error', e => window.__errs.push('error: ' + e.message));"
                     " addEventListener('unhandledrejection',"
                     " e => window.__errs.push('rejection: ' + e.reason));")

    def preload(self, source):
        """Run `source` (statements) in every page from now on, before the page's own scripts
        (WebDriver BiDi, script.addPreloadScript)."""
        import websockets

        async def go():
            async with websockets.connect(self.bidi_url) as ws:
                await ws.send(json.dumps({'id': 1, 'method': 'script.addPreloadScript',
                                          'params': {'functionDeclaration':
                                                     '() => { ' + source + ' }'}}))
                reply = json.loads(await ws.recv())
                if reply.get('type') == 'error':
                    raise RuntimeError(reply)
        asyncio.run(go())

    def goto(self, url, settle=0.8):
        self._post('/url', {'url': url})
        self.sleep(settle)

    def _viewport(self, w, h):
        """Set the page's viewport (CSS pixels) over WebDriver BiDi."""
        import websockets
        ctx = json.load(urllib.request.urlopen(
            f'http://127.0.0.1:{self.gport}/session/{self.sid}/window'))['value']

        async def go():
            async with websockets.connect(self.bidi_url) as ws:
                await ws.send(json.dumps({'id': 1, 'method': 'browsingContext.setViewport',
                                          'params': {'context': ctx,
                                                     'viewport': {'width': w, 'height': h},
                                                     'devicePixelRatio': self.dpr}}))
                reply = json.loads(await ws.recv())
                if reply.get('type') == 'error':
                    raise RuntimeError(reply)
        asyncio.run(go())

    def sleep(self, secs):
        time.sleep(secs)

    def ev(self, expression):
        v = self._post('/execute/sync', {'script': f'return ({expression});', 'args': []})['value']
        return v

    def page(self, expression):
        """An expression's value in the page's own realm. WebDriver scripts run in a sandbox with
        module maps of their own, so a module the page itself imported is reached through
        WebDriver BiDi (script.evaluate in the page's realm, the value passed as JSON)."""
        import websockets
        ctx = json.load(urllib.request.urlopen(
            f'http://127.0.0.1:{self.gport}/session/{self.sid}/window'))['value']
        wrapped = (f'Promise.resolve({expression}).then('
                   f'(v) => JSON.stringify(v === undefined ? null : v))')

        async def go():
            async with websockets.connect(self.bidi_url, max_size=None) as ws:
                await ws.send(json.dumps({'id': 1, 'method': 'script.evaluate', 'params': {
                    'expression': wrapped, 'target': {'context': ctx}, 'awaitPromise': True,
                    'resultOwnership': 'none'}}))
                reply = json.loads(await ws.recv())
                if reply.get('type') == 'error' or reply['result'].get('type') != 'success':
                    raise RuntimeError(json.dumps(reply)[:400])
                return json.loads(reply['result']['result']['value'])
        return asyncio.run(go())

    def collect_errors(self):
        try:
            self.errors += self.ev('window.__errs') or []
            self.ev('window.__errs = []')
        except Exception:
            pass
        return self.errors

    def press(self, name):
        k = WD_KEYS[name]
        self._post('/actions', {'actions': [{'type': 'key', 'id': 'kb', 'actions': [
            {'type': 'keyDown', 'value': k}, {'type': 'keyUp', 'value': k}]}]})
        self.sleep(0.15)

    def click(self, x, y):
        self._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                             'parameters': {'pointerType': 'mouse'}, 'actions': [
            {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(x), 'y': int(y)},
            {'type': 'pointerDown', 'button': 0}, {'type': 'pointerUp', 'button': 0}]}]})
        self.sleep(0.2)

    def resize(self, w, h):
        self.size = (w, h)
        self._viewport(w, h)
        self.sleep(0.4)

    def shot(self):
        url = f'http://127.0.0.1:{self.gport}/session/{self.sid}/screenshot'
        return base64.b64decode(json.load(urllib.request.urlopen(url))['value'])

    def close(self):
        if self not in _live:
            return
        if self.sid is not None:
            try:
                self._post('', method='DELETE')
            except Exception:
                pass
        reap(self.gd)
        kill_using(self.profile)
        if self in _live:
            _live.remove(self)
        remove_tree(self.profile)


def center(b, selector):
    """The viewport centre of an element, from the page."""
    r = b.ev(f"(() => {{ const r = document.querySelector({json.dumps(selector)})"
             f".getBoundingClientRect();"
             f" return [r.left + r.width / 2, r.top + r.height / 2]; }})()")
    return r
