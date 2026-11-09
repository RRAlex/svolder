# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A controllable clock for a page, for the asserts that count what the page's own timers and frames
do in a stretch of time (draws in 5 s, writes in 3 s): with the real clock the count depends on how
loaded the machine is, with this one it depends only on the page's code.

`install(b)` runs a script in every page the browser loads from then on (Chromium:
Page.addScriptToEvaluateOnNewDocument, Firefox: a WebDriver BiDi preload script). Until `take(b)`
the page runs on the real clock, its timers and frames wrapped but passed through, so it boots
and settles as usual. `take(b)` freezes the page's clock where it stands: performance.now(),
Date.now(), setTimeout, setInterval and requestAnimationFrame become virtual, the timers and frames
already waiting move over with the time they had left. `advance(b, ms)` then moves the clock `ms`
virtual milliseconds, running every timer in its order and a frame every 1000/60 ms, and lets the page's
promises and the daemon's messages run between the steps (the real work of a draw still takes real
time, only the count of them in the stretch is fixed). Fetches and sockets stay real; a case that
depends on the daemon's own pace in real time does not use this clock.

Use a browser of its own for these cases: the wrapped timers are in every page that browser loads."""

FRAME_MS = 1000 / 60

CLOCK_JS = """(() => {
  if (window.__clock) return;
  const real = { st: setTimeout.bind(window), ct: clearTimeout.bind(window), si: setInterval.bind(window), ci: clearInterval.bind(window),
    raf: requestAnimationFrame.bind(window), caf: cancelAnimationFrame.bind(window),
    pnow: performance.now.bind(performance), dnow: Date.now };
  const chan = new MessageChannel();
  const yielded = () => new Promise((resolve) => { chan.port1.onmessage = () => resolve(); chan.port2.postMessage(0); });
  const table = new Map();
  let id = 1000000;
  let frozen = false, vnow = 0, vdate = 0, vnow0 = 0, nextFrame = 0;

  const arm = (e) => {
    if (e.kind === 'timeout') e.handle = real.st(() => { table.delete(e.id); e.fn(...e.args); }, e.delay);
    else if (e.kind === 'interval') e.handle = real.si(() => { e.due = real.pnow() + e.delay; e.fn(...e.args); }, e.delay);
    else e.handle = real.raf((ts) => { table.delete(e.id); e.fn(ts); });
    if (e.kind !== 'raf') e.due = real.pnow() + e.delay;
  };
  const add = (kind, fn, delay, args) => {
    const e = { id: ++id, kind, fn, delay: Math.max(0, Number(delay) || 0), args };
    table.set(e.id, e);
    if (frozen) e.due = vnow + e.delay; else arm(e);
    return e.id;
  };
  const drop = (kind) => (h) => {
    const e = table.get(h);
    if (!e) return;
    table.delete(h);
    if (!frozen) (kind === 'raf' ? real.caf : kind === 'interval' ? real.ci : real.ct)(e.handle);
  };
  window.setTimeout = (fn, delay, ...args) => add('timeout', fn, delay, args);
  window.setInterval = (fn, delay, ...args) => add('interval', fn, Math.max(1, Number(delay) || 0), args);
  window.requestAnimationFrame = (fn) => add('raf', fn, 0, []);
  window.clearTimeout = drop('timeout');
  window.clearInterval = drop('interval');
  window.cancelAnimationFrame = drop('raf');
  performance.now = () => (frozen ? vnow : real.pnow());
  Date.now = () => (frozen ? vdate + (vnow - vnow0) : real.dnow());

  window.__clock = {
    take() {
      if (frozen) return vnow;
      const at = real.pnow();
      for (const e of table.values()) {
        (e.kind === 'raf' ? real.caf : e.kind === 'interval' ? real.ci : real.ct)(e.handle);
        if (e.kind !== 'raf') e.due = at + Math.max(0, e.due - at);
      }
      vnow = vnow0 = at;
      vdate = real.dnow();
      nextFrame = at + %(frame)s;
      frozen = true;
      return vnow;
    },
    now: () => (frozen ? vnow : real.pnow()),
    async advance(ms) {
      const end = vnow + ms;
      for (;;) {
        let t = nextFrame;
        for (const e of table.values()) if (e.kind !== 'raf' && e.due < t) t = e.due;
        if (t > end) break;
        vnow = Math.max(vnow, t);
        const due = [...table.values()].filter((e) => e.kind !== 'raf' && e.due <= vnow).sort((a, b) => a.due - b.due || a.id - b.id);
        for (const e of due) {
          if (!table.has(e.id)) continue;
          if (e.kind === 'timeout') table.delete(e.id); else e.due = vnow + e.delay;
          try { e.fn(...e.args); } catch (err) { console.error(err); }
        }
        if (vnow >= nextFrame) {
          const frames = [...table.values()].filter((e) => e.kind === 'raf');
          for (const e of frames) {
            table.delete(e.id);
            try { e.fn(vnow); } catch (err) { console.error(err); }
          }
          nextFrame += %(frame)s;
        }
        await yielded();
      }
      vnow = end;
      return vnow;
    },
  };
})();""" % {'frame': FRAME_MS}


def install(b):
    """Before the page loads: put the clock in every page this browser loads from now on."""
    b.preload(CLOCK_JS)


def take(b):
    """Freeze the page's clock where it stands (the page loaded and settled on the real one)."""
    return b.page('window.__clock.take()')


def advance(b, ms):
    """Move the page's clock `ms` virtual milliseconds, its timers and frames running as they come due."""
    return b.page(f'window.__clock.advance({ms})')
