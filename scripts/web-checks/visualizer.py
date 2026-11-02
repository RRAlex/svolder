# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The visualizer card on the fake host: it draws the fake host's synthetic music (non-blank pixels;
a fresh waterfall's oldest columns are transparent, silence has no colour); a fresh browser gets
lumiBars soft with the graph off and a tap goes from it to the spectrogram, and an earlier card's
`svolder.viz` is ignored (the hint keeps only that it was shown); the card and Now playing are
translucent like the tone panel and the canvas paints no ground; a tap cycles spectrogram,
lumiBars, lumiBars soft and the chip names each; the corner toggle shows and hides the graph
overlay; the choice survives a reload; `v` cycles; a hidden page stops drawing at once and closes
the frames stream after the 2 s grace, and opening it again draws again; a page hidden for more
than 2 s lets go of its device stream too (a rest, not a fault), is caught up by the new stream's
snapshot within 2 s of being shown, a quick hide and show opens nothing, and a device switch or a
history step while it rests opens no stream until it is shown, then one on the device shown; a
card below the fold opens no stream until it is scrolled to; with SVOLDER_FAKE_CAPTURE=off the
card says so quietly and keeps its box; the effects hear the frames: the glow's level is over the
volume's while the card runs and the layer says `hearing`, the volume's again once it stops or
without a capture, and the smoke puffs on the beats and only then; the card sits in the band's
right half beside Now playing on a desktop (820 and 1280 px) and under the volume on a phone
(390 px), and nothing moves when it starts; the Device fold-out shows the delay row while the card
runs; an amp page shows the card with a sink label; a guest uses the card too; a card shown again
after a rest draws nothing and the page waits (dimmed, inert, no notice, a key or tap sends
nothing) until the new stream's first reading; frames that stop while the capture reads running
let the waterfall drift out to transparent; a woken page keeps the glow in colour and a screen with
nothing to read is ready at its first event; on a phone the card follows the volume (or the
meters) in the page's order as well as on screen, and goes back into the band on a desktop; the
effects menu's Visualizer switch (on by default, kept in svolder.viz.2, in the menu on every page
with a card, reachable by the keyboard) takes the card out of the page with its canvas loop and its
frames stream, leaves Now playing the whole band at the lower fixed height (the page says so before
the first paint), moves nothing but once, survives a reload, brings the card back, and leaves the
page whole without storage; on a phone there is no card. Numbers from the
page, no pictures (VIZ_SHOTS=dir keeps stills for looking).
Usage: python3 visualizer.py chromium|firefox"""
import json
import os
import signal
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
shots = os.environ.get('VIZ_SHOTS')

HINT = 'tap to change the look'
LOOKS = ['spectrogram', 'lumibars', 'lumibars-soft']
NAMES = {'spectrogram': 'Spectrogram', 'lumibars': 'lumiBars', 'lumibars-soft': 'lumiBars soft'}


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# Every page: the page's visibility is a switch the check holds (`window.__hidden`; a page that
# starts hidden opens nothing), and every EventSource is noted with whether it was closed.
PRELOAD = """window.__hidden = %s;
%s
Object.defineProperty(Document.prototype, 'visibilityState',
  { configurable: true, get() { return window.__hidden ? 'hidden' : 'visible'; } });
Object.defineProperty(Document.prototype, 'hidden',
  { configurable: true, get() { return window.__hidden; } });
window.__es = [];
window.__raf = new Set();
const request = window.requestAnimationFrame.bind(window);
const cancel = window.cancelAnimationFrame.bind(window);
window.requestAnimationFrame = (fn) => {
  if (fn.name !== 'animate') return request(fn);
  const id = request((t) => { window.__raf.delete(id); fn(t); });
  window.__raf.add(id);
  return id;
};
window.cancelAnimationFrame = (id) => { window.__raf.delete(id); cancel(id); };
const Native = window.EventSource;
window.EventSource = class extends Native {
  constructor(url, options) {
    super(url, options);
    const note = { url: String(url), closed: false };
    window.__es.push(note);
    const close = this.close.bind(this);
    this.close = () => { note.closed = true; close(); };
  }
};"""

BOXES = """(() => {
  const q = (s) => document.querySelector(s);
  const box = (el) => { if (!el) return null; const b = el.getBoundingClientRect();
    if (b.width === 0 && b.height === 0) return null;
    return [b.left + scrollX, b.top + scrollY, b.width, b.height]
      .map((v) => Math.round(v * 10) / 10); };
  return { band: box(q('.p-band')), playing: box(q('.p-playing')), viz: box(q('.p-viz')),
    knob: box(q('sv-knob')), volume: box(q('#part-volume')), controls: box(q('#part-controls')),
    col: box(q('.view .col')), view: box(q('.view')), facts: box(q('#part-facts')),
    meters: box(q('#part-meters')), height: document.documentElement.scrollHeight }; })()"""

STATE = """(() => {
  const r = document.querySelector('.p-viz'); if (!r) return null;
  const q = (s) => r.querySelector(s);
  const chip = q('.viz-chip'); const tog = q('.viz-graph'); const note = q('.viz-note');
  const sink = q('.viz-sink');
  return { look: r.dataset.look, graph: r.dataset.graph, state: r.dataset.state,
    chip: chip.textContent, chipShown: chip.classList.contains('show'),
    chipTransition: getComputedStyle(chip).transitionDuration,
    pressed: tog.getAttribute('aria-pressed'), note: note.textContent.trim(),
    noteShown: !note.hidden && getComputedStyle(note).display !== 'none',
    sink: sink.textContent.trim(), sinkShown: !sink.hidden,
    stored: (() => { try { return JSON.parse(localStorage.getItem('svolder.viz.2')); }
      catch { return null; } })(),
    canvas: [q('canvas').width, q('canvas').height],
    delay: (() => { const row = document.querySelector('#part-facts [data-viz-delay]');
      return row ? [row.querySelector('dt').textContent, row.querySelector('dd').textContent]
        : null; })() }; })()"""

PIXELS = """(() => {
  const c = document.querySelector('.p-viz canvas'); const W = c.width; const H = c.height;
  const d = c.getContext('2d').getImageData(0, 0, W, H).data;
  let non = 0; let hash = 0; let cyan = 0; let orange = 0;
  for (let i = 0; i < d.length; i += 4) {
    const r = d[i]; const g = d[i + 1]; const b = d[i + 2]; const a = d[i + 3];
    if (a !== 0) non++;
    hash = (hash * 31 + r + g * 3 + b * 7 + a * 11) | 0;
    if (a > 200 && Math.abs(r - 80) + Math.abs(g - 230) + Math.abs(b - 255) <= 12) cyan++;
    if (a > 200 && Math.abs(r - 255) + Math.abs(g - 150) + Math.abs(b - 50) <= 12) orange++;
  }
  let left = true;
  for (let y = 0; y < H && left; y++) for (let x = 0; x < 4; x++) {
    if (d[(y * W + x) * 4 + 3] !== 0) left = false;
  }
  return { W, H, non, hash, cyan, orange, leftClear: left }; })()"""


ORDER = """(() => {
  const anchor = document.querySelector('#part-volume, #part-meters');
  const card = document.querySelector('.p-viz');
  return { after: !!(anchor.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING),
    inBand: card.parentElement.classList.contains('p-band') }; })()"""


def state(b):
    return b.page(STATE)


def boxes(b):
    return b.page(BOXES)


def pixels(b):
    return b.page(PIXELS)


def frames_streams(b):
    return [e for e in b.page('window.__es') if 'host/frames' in e['url']]


def device_streams(b):
    return [e for e in b.page('window.__es') if '/events' in e['url']]


def stream_device(url):
    """The device a page's event stream was opened for, as its address names it."""
    return url.split('device=')[1].split('&')[0]


def until(b, cond, secs=8.0, every=0.2):
    """The viz state once `cond` holds of it, or the last one when `secs` are up."""
    end = time.time() + secs
    s = state(b)
    while not (s and cond(s)) and time.time() < end:
        b.sleep(every)
        s = state(b)
    return s


# What a run keeps before the page starts (only when nothing is kept yet, so a reload keeps what
# the page chose): the spectrogram, with the graph on or off. The checks below were written from
# that start; a run with `fresh` keeps nothing and sees the page's own defaults.
SEED = """try { if (localStorage.getItem('svolder.viz.2') === null) {
  localStorage.setItem('svolder.viz.2', JSON.stringify({ look: 'spectrogram', graph: %s,
    hinted: false })); } } catch (e) {}"""
GRAPH_OFF = SEED % 'false'
# What an earlier 0.29.0 card kept (its look and graph, the hint seen): the page never reads it.
EARLIER = """try { localStorage.setItem('svolder.viz', JSON.stringify({ look: 'spectrogram',
  graph: true, hinted: true })); } catch (e) {}"""


def make(daemon, size=(1280, 900), hidden=False, graph=True, effects='off', fresh=False,
         earlier=False, kept=None, extra='', **kw):
    """A page on `daemon`. `kept` is what the page starts with in storage (a script; the default
    is the seed above), `extra` a script that runs before the page's own."""
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size, **kw)
    if kept is None:
        kept = EARLIER if earlier else '' if fresh else SEED % ('true' if graph else 'false')
    b.preload(PRELOAD % ('true' if hidden else 'false', kept + extra))
    b.goto(daemon.url + f'?effects={effects}', settle=1.2)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def set_hidden(b, hidden):
    b.page("(() => { window.__hidden = %s; document.dispatchEvent("
           "new Event('visibilitychange')); return 1; })()" % ('true' if hidden else 'false'))


def tap(b, selector='.p-viz .viz-face'):
    x, y = common.center(b, selector)
    b.click(x, y)


def shoot(b, name):
    if shots:
        os.makedirs(shots, exist_ok=True)
        with open(f'{shots}/{browser}-{name}.png', 'wb') as f:
            f.write(b.shot())


def settle_running(b, secs=8.0):
    return until(b, lambda s: s['state'] == 'running', secs)


# ------------------------------------------------------------------ a desktop, 1280 px
dac = common.Daemon(host='fake')
b = make(dac, hidden=True, graph=False)
try:
    before = boxes(b)
    s = state(b)
    check('a card is on the page, in the band beside Now playing',
          s is not None and before['band'] and before['viz'] and before['playing'], before)
    check('a page that starts hidden opens no frames stream and has not started the card',
          frames_streams(b) == [] and s['state'] != 'running', [s, frames_streams(b)])
    set_hidden(b, False)
    s = settle_running(b)
    fresh = pixels(b)
    check('the card runs once shown: a frames stream is open and the state reads running',
          s['state'] == 'running' and len(frames_streams(b)) == 1
          and not frames_streams(b)[0]['closed'], [s, frames_streams(b)])
    check('a fresh spectrogram\'s oldest columns are transparent, none white',
          fresh['leftClear'], fresh)
    check('the hint reads "tap to change the look" the first time',
          s['chip'] == HINT, s['chip'])
    b.sleep(2.5)
    px = pixels(b)
    check('the card draws: non-blank pixels, and they change', px['non'] > 1000
          and px['hash'] != fresh['hash'], [px['non'], px['hash'], fresh['hash']])
    after = boxes(b)
    check('nothing moves when the visualizer starts (every box as before)', after == before,
          [before, after])
    check('the band holds Now playing on the left and the card on the right, both 240 px tall',
          after['playing'][0] < after['viz'][0]
          and after['playing'][0] + after['playing'][2] <= after['viz'][0] + 0.5
          and after['playing'][1] == after['viz'][1]
          and after['playing'][3] == 240 and after['viz'][3] == 240, after)
    check('at 1280 px the two are halves of the band',
          abs(after['playing'][2] - after['viz'][2]) <= 2
          and after['viz'][0] + after['viz'][2] <= after['band'][0] + after['band'][2] + 0.5
          and after['band'][1] + after['band'][3] <= after['col'][1] + 0.5, after)
    check('a page that shows its own DAC names no sink', not s['sinkShown'] and s['sink'] == '', s)
    shoot(b, 'desktop-spectrogram')

    # a tap cycles the looks; the chip names each
    s = state(b)
    order = []
    for expect in LOOKS[1:] + LOOKS[:1]:
        tap(b)
        s = state(b)
        order.append((s['look'], s['chip'], s['chipShown']))
        b.sleep(0.5)
        shoot(b, f'desktop-{expect}')
    check('a tap cycles spectrogram, lumiBars, lumiBars soft, spectrogram, the chip naming each',
          order == [(look, NAMES[look], True) for look in LOOKS[1:] + LOOKS[:1]], order)
    b.sleep(1.5)
    s = state(b)
    check('the chip goes after about a second', not s['chipShown'], s)

    # the graph overlay: off here (the run started with it off), on by the corner toggle
    off = pixels(b)
    check('the graph starts as it was kept: off, no curve colours on the waterfall',
          s['graph'] == 'off' and s['pressed'] == 'false' and off['cyan'] == 0
          and off['orange'] == 0, [s, off['cyan'], off['orange']])
    x, y = common.center(b, '.p-viz .viz-graph')
    b.click(x, y)
    b.sleep(0.5)
    s = state(b)
    on = pixels(b)
    check('the corner toggle shows the graph over the spectrogram and leaves the look alone',
          s['graph'] == 'on' and s['pressed'] == 'true' and s['look'] == 'spectrogram'
          and on['orange'] > 20, [s, on['cyan'], on['orange']])
    shoot(b, 'desktop-spectrogram-graph')
    b.click(x, y)
    b.sleep(0.5)
    s = state(b)
    off = pixels(b)
    check('and hides it again', s['graph'] == 'off' and s['pressed'] == 'false'
          and off['cyan'] == 0 and off['orange'] == 0, [s, off['cyan'], off['orange']])
    b.click(x, y)

    # the delay row of the Device fold-out
    b.page("(() => { document.querySelector('.device-facts').open = true; return 1; })()")
    b.sleep(1.6)
    s = state(b)
    check('the Device fold-out shows "Visualizer delay N ms" while the card runs',
          s['delay'] is not None and s['delay'][0] == 'Visualizer delay'
          and s['delay'][1].endswith(' ms') and s['delay'][1][:-3].isdigit(), s['delay'])

    # the keys: v cycles; the list says so
    b.press('v')
    s = state(b)
    check('v cycles the look', s['look'] == 'lumibars' and s['chip'] == 'lumiBars', s)
    b.press('v')
    b.press('v')
    s = state(b)
    check('  and wraps', s['look'] == 'spectrogram', s)
    b.page("(() => { document.querySelector('.device-facts').open = false; return 1; })()")

    # the choice survives a reload
    b.press('v')
    b.press('v')
    x, y = common.center(b, '.p-viz .viz-graph')
    b.click(x, y)
    s = state(b)
    check('the choice is kept in svolder.viz.2',
          s['stored'] and s['stored']['look'] == 'lumibars-soft'
          and s['stored']['graph'] is False and s['stored']['hinted'] is True, s['stored'])
    b.goto(dac.url + '?effects=off', settle=1.2)
    set_hidden(b, False)
    s = settle_running(b)
    b.sleep(1.0)
    s = state(b)
    soft = pixels(b)
    check('after a reload the look and the graph are as left, with no hint again',
          s['look'] == 'lumibars-soft' and s['graph'] == 'off' and s['pressed'] == 'false'
          and s['chip'] != HINT, s)
    check('  lumiBars soft draws', soft['non'] > 500, soft['non'])
    shoot(b, 'desktop-after-reload-soft')
    b.press('v')

    # a hidden page: nothing is drawn at once, the stream goes after the grace, and it comes back
    s = state(b)
    shown_a = pixels(b)['hash']
    b.sleep(0.4)
    check('a shown page keeps drawing', pixels(b)['hash'] != shown_a)
    set_hidden(b, True)
    b.sleep(0.3)
    h1 = pixels(b)['hash']
    b.sleep(0.6)
    h2 = pixels(b)['hash']
    check('a hidden page draws nothing at once', h1 == h2, [h1, h2])
    check('  its frames stream is still open inside the 2 s grace',
          not frames_streams(b)[-1]['closed'], frames_streams(b))
    b.sleep(1.9)
    check('  and closed after it', frames_streams(b)[-1]['closed'], frames_streams(b))
    s = state(b)
    check('  the delay row goes with it', s['delay'] is None, s['delay'])
    opened = len(frames_streams(b))
    set_hidden(b, False)
    s = settle_running(b)
    b.sleep(0.5)
    check('a shown page opens the stream again and draws',
          len(frames_streams(b)) == opened + 1 and not frames_streams(b)[-1]['closed']
          and s['state'] == 'running' and pixels(b)['hash'] != h2,
          [opened, frames_streams(b), s])
    # shown again inside the grace: the stream is never closed
    set_hidden(b, True)
    b.sleep(1.0)
    set_hidden(b, False)
    b.sleep(2.6)
    check('a page shown again inside the grace keeps its stream',
          len(frames_streams(b)) == opened + 1 and not frames_streams(b)[-1]['closed'],
          frames_streams(b))
    check('no page error on the desktop', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ 820 px
dac = common.Daemon(host='fake')
b = make(dac, size=(820, 900))
try:
    s = settle_running(b)
    m = boxes(b)
    check('at 820 px the card sits in the right of the band beside Now playing, 240 px tall',
          m['playing'][0] + m['playing'][2] <= m['viz'][0] + 0.5 and m['playing'][1] == m['viz'][1]
          and m['viz'][3] == 240 and m['playing'][3] == 240
          and m['viz'][0] + m['viz'][2] <= m['band'][0] + m['band'][2] + 0.5
          and m['band'][1] + m['band'][3] <= m['col'][1] + 0.5, m)
    check('  Now playing has the larger share there (a changed path must fit)',
          m['playing'][2] > m['viz'][2] and m['viz'][2] > 200, m)
    check('no page error at 820 px', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ a phone, 390 px
dac = common.Daemon(host='fake')
b = make(dac, size=(390, 600))
try:
    m = boxes(b)
    check('on a phone the card sits under the volume and above the controls, 180 px tall',
          m['viz'] and m['viz'][1] >= m['volume'][1] + m['volume'][3] - 0.5
          and m['viz'][1] + m['viz'][3] <= m['controls'][1] + 0.5 and m['viz'][3] == 180
          and m['viz'][2] >= 330 and m['viz'][2] <= 390, m)
    check('  and the compact Now playing line stays above the volume',
          m['playing'][1] + m['playing'][3] <= m['volume'][1] and m['playing'][3] == 72, m)
    order = b.page(ORDER)
    check('  and the card is after the volume in the page\'s order too (a keyboard or a screen '
          'reader meets the volume first), outside the band', order['after']
          and not order['inBand'], order)
    check('  the card is below the fold, so no stream is open for it',
          m['viz'][1] > 600 and frames_streams(b) == [], [m['viz'], frames_streams(b)])
    b.page("(() => { window.scrollTo(0, %d); return 1; })()" % (m['viz'][1] - 200))
    s = settle_running(b)
    check('scrolled into view the stream opens and the card runs',
          s['state'] == 'running' and len(frames_streams(b)) == 1, [s, frames_streams(b)])
    check('nothing moves when it starts', boxes(b) == m, [m, boxes(b)])
    shoot(b, 'phone-spectrogram')
    for look in LOOKS[1:]:
        tap(b)
        b.sleep(0.5)
        shoot(b, f'phone-{look}')
    tap(b)
    b.page("(() => { window.scrollTo(0, 0); return 1; })()")
    b.sleep(2.6)
    check('scrolled out of view again, the stream closes after the grace',
          frames_streams(b)[-1]['closed'], frames_streams(b))
    # the page is widened and narrowed again: the card follows in the page's order
    b.resize(1280, 900)
    b.sleep(0.5)
    order = b.page(ORDER)
    wide = boxes(b)
    check('  back on a desktop width the card is in the band again, beside Now playing',
          not order['after'] and order['inBand'] and wide['viz'][0] > wide['playing'][0]
          and wide['viz'][1] == wide['playing'][1], [order, wide])
    b.resize(390, 600)
    b.sleep(0.5)
    order = b.page(ORDER)
    check('  and on a phone width again, after the volume', order['after'] and not order['inBand'],
          order)
    check('no page error on the phone', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ a browser with nothing kept
GROUND = """(() => {
  const alpha = (el) => { const m = getComputedStyle(el).backgroundColor
    .match(/\\/\\s*([\\d.]+)\\)|,\\s*([\\d.]+)\\)$/); return m ? Number(m[1] || m[2]) : 1; };
  const c = document.querySelector('.p-viz canvas');
  const d = c.getContext('2d').getImageData(0, 0, c.width, 1).data;
  let top = 0; for (let i = 3; i < d.length; i += 4) top += d[i];
  return { viz: alpha(document.querySelector('.p-viz')),
    playing: alpha(document.querySelector('.p-playing')), top: top / (c.width * 255),
    folded: document.querySelector('.device-facts').open === false }; })()"""
dac = common.Daemon(host='fake')
b = make(dac, earlier=True)
try:
    s = settle_running(b)
    b.sleep(2.0)
    s = state(b)
    px = pixels(b)
    g = b.page(GROUND)
    check('a browser with nothing kept starts on lumiBars soft with the graph off',
          s['look'] == 'lumibars-soft' and s['graph'] == 'off' and s['pressed'] == 'false', s)
    check('  an earlier card\'s svolder.viz (spectrogram, graph on, hint seen) is ignored',
          s['chip'] == HINT, s['chip'])
    check('  the hint keeps only that it was shown: no look, no graph in svolder.viz.2',
          s['stored'] == {'hinted': True}, s['stored'])
    check('  the card draws, and it paints no ground (the soft look fades out at the top)',
          px['non'] > 500 and g['top'] < 0.02, [px['non'], g['top']])
    check('Now playing and the visualizer are translucent, at the tone panel\'s 70 %',
          abs(g['viz'] - 0.7) < 0.02 and abs(g['playing'] - 0.7) < 0.02, g)
    check('the Device fold-out is folded on a fresh load', g['folded'], g)
    shoot(b, 'fresh-default')
    order = []
    for _ in range(3):
        tap(b)
        order.append(state(b)['look'])
    check('a tap goes from soft to the spectrogram, then lumiBars, then soft again',
          order == ['spectrogram', 'lumibars', 'lumibars-soft'], order)
    s = state(b)
    check('  a tap keeps the look it chose and still no graph (not a choice yet)',
          s['stored'] == {'look': 'lumibars-soft', 'hinted': True}, s['stored'])
    check('no page error from a fresh browser', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ swaps while it runs
RUNNING = """(() => ({ cards: document.querySelectorAll('.p-viz').length,
  open: window.__es.filter((e) => e.url.includes('host/frames') && !e.closed).length,
  loops: window.__raf.size, device: document.querySelector('.view')?.dataset.device }))()"""
two = common.Daemon(host='fake', more=[('Second', '1112131415161718')])
b = make(two)
try:
    s = settle_running(b)
    b.sleep(0.5)
    one = b.page(RUNNING)
    check('before any swap: one card, one open frames stream, one animation loop',
          one['cards'] == 1 and one['open'] == 1 and one['loops'] == 1, one)
    first = one['device']
    # a device change: the header's link swaps the whole screen in
    b.page("(() => { document.querySelector('.devs a:not([aria-current=page])').click(); "
           "return 1; })()")
    b.sleep(1.5)
    s = settle_running(b)
    h1 = pixels(b)['hash']
    b.sleep(0.5)
    after = b.page(RUNNING)
    check('a device change leaves one card, one open frames stream and one animation loop',
          after['device'] != first and after['cards'] == 1 and after['open'] == 1
          and after['loops'] == 1, [first, after])
    check('  and the new card draws', s['state'] == 'running' and pixels(b)['hash'] != h1
          and pixels(b)['non'] > 500, s)
    # the daemon restarts: the device stream reconnects and sends the whole page again
    for _ in range(3):
        b.page("import(new URL('assets/core/boot.js', document.baseURI)).then((m) => "
               "m.core.actions.showDevice(m.core.model.shown(), { push: false })).then(() => 1)")
        b.sleep(0.4)
    check('no page error across the device change and the swaps', errors(b) == [], errors(b))
    streams = len(device_streams(b))
    two.restart()
    # the page's device stream opens anew (a stream is only counted once it was made after the
    # restart) and the link is back
    end = time.time() + 15.0
    while time.time() < end:
        seen = b.page("({ lost: document.body.classList.contains('lost'),"
                      " knob: document.querySelector('sv-knob')?.raw ?? null })")
        if len(device_streams(b)) > streams and not seen['lost'] and seen['knob'] is not None:
            break
        b.sleep(0.25)
    s = settle_running(b)
    h2 = pixels(b)['hash']
    b.sleep(0.5)
    again = b.page(RUNNING)
    check('swaps of the screen and a daemon restart leave one card, one open stream, one loop',
          again['cards'] == 1 and again['open'] == 1 and again['loops'] == 1, again)
    check('  and the card still draws', pixels(b)['hash'] != h2 and s['state'] == 'running', s)
finally:
    b.close()
    two.stop()

# ------------------------------------------------------------------ a stream the browser gave up on
# A stand-in frames stream that fails for good at once: the page asks again after 3 s, then 6 s.
GIVEN_UP = """window.__tries = [];
const Real = window.EventSource;
window.EventSource = class extends EventTarget {
  constructor(url) {
    super();
    if (!String(url).includes('host/frames')) return new Real(url);
    this.readyState = 0;
    window.__tries.push(performance.now());
    setTimeout(() => { this.readyState = 2; this.dispatchEvent(new Event('error')); }, 10);
  }
  close() { this.readyState = 2; }
};
window.EventSource.CLOSED = 2;"""
gone = common.Daemon(host='fake')
b = common.Chromium(size=(1280, 900)) if browser == 'chromium' else common.Firefox(size=(1280, 900))
try:
    b.preload(PRELOAD % ('false', GRAPH_OFF))
    b.preload(GIVEN_UP)
    b.goto(gone.url + '?effects=off', settle=1.0)
    b.sleep(10.5)
    tries = b.page('window.__tries')
    gaps = [round(b2 - a) for a, b2 in zip(tries, tries[1:])]
    check('a stream the browser gave up on is asked for again after 3 s, then 6 s (back-off)',
          len(gaps) >= 2 and 2700 <= gaps[0] <= 3600 and 5500 <= gaps[1] <= 6800, gaps)
finally:
    b.close()
    gone.stop()

# ------------------------------------------------------------------ no capture possible
keep = {}
dac = common.Daemon(host='fake')
b = make(dac)
try:
    settle_running(b)
    keep = boxes(b)
finally:
    b.close()
    dac.stop()
off = common.Daemon(host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
b = make(off)
try:
    s = until(b, lambda s: s['state'] == 'unavailable')
    m = boxes(b)
    flat = pixels(b)
    x, y = common.center(b, '.p-viz .viz-graph')
    b.click(x, y)
    px = pixels(b)
    check('with no capture the card says so quietly, with the daemon\'s reason',
          s['state'] == 'unavailable' and s['noteShown'] and 'switched off' in s['note'], s)
    check('  and keeps its box exactly where it was with the capture on',
          m['viz'] == keep['viz'] and m['playing'] == keep['playing'] and m == keep, [keep, m])
    check('  nothing is drawn (with the graph on, only its flat curves)',
          px['non'] == 0 and 0 < flat['non'] < 2 * flat['W'] * 4, [px['non'], flat['non']])
    b.sleep(0.5)
    tap(b)
    check('  and a tap still cycles the look', state(b)['look'] == 'lumibars', state(b))
    shoot(b, 'desktop-unavailable')
    check('no page error with no capture', errors(b) == [], errors(b))
finally:
    b.close()
    off.stop()

# ------------------------------------------------------------------ an amp page
amp = common.Daemon('fake-amp', host='fake', name='Fake amp')
b = make(amp)
try:
    s = settle_running(b)
    m = boxes(b)
    check('an amp page shows the card in the band, running',
          s['state'] == 'running' and m['viz'] and m['viz'][3] == 240
          and m['playing'][0] + m['playing'][2] <= m['viz'][0] + 0.5, [s, m])
    check('  with the sink named (the sound goes to the computer\'s sink, not to the amp)',
          s['sinkShown'] and s['sink'].startswith('Playing to ') and len(s['sink']) > 12, s)
    shoot(b, 'desktop-amp')
    b.resize(390, 800)
    m = boxes(b)
    check('  on a phone the card follows the meters',
          m['viz'][1] >= m['meters'][1] + m['meters'][3] - 0.5 and m['viz'][3] == 180, m)
    order = b.page(ORDER)
    check('  and follows them in the page\'s order too', order['after'] and not order['inBand'],
          order)
    check('no page error on the amp page', errors(b) == [], errors(b))
finally:
    b.close()
    amp.stop()

# ------------------------------------------------------------------ the Visualizer switch
# The effects menu's Visualizer row: on by default, kept as `on` in svolder.viz.2. Off takes the
# card out of the page (no canvas loop, no frames stream), leaves Now playing the whole band at the
# lower fixed height, moves the page once, and is told to the style sheet before the first paint.
OFF_KEPT = """try { localStorage.setItem('svolder.viz.2',
  JSON.stringify({ on: false, hinted: true })); } catch (e) {}"""
# what the page looked like when the parser had finished and no script had run yet (the frame
# carries no card), and the card's computed display at the moment the theme inserts the screen:
# `data-viz` is on the root before any module runs, so a card kept off is not in the layout when
# it first exists.
FIRST = """document.addEventListener('readystatechange', () => {
  if (document.readyState !== 'interactive') return;
  const card = document.querySelector('.p-viz');
  window.__first = { viz: document.documentElement.dataset.viz ?? null,
    card: card ? getComputedStyle(card).display : null }; });
(() => { const html = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
  Object.defineProperty(Element.prototype, 'innerHTML', { ...html, set(value) {
    html.set.call(this, value);
    const card = window.__inserted ? null : this.querySelector?.('.p-viz');
    if (card) window.__inserted = { viz: document.documentElement.dataset.viz ?? null,
      card: getComputedStyle(card).display }; } }); })();"""
SWITCH = """(() => { const row = document.querySelector('.fxpop [data-switch=viz]');
  const at = document.activeElement;
  return { hidden: row.hidden, role: row.getAttribute('role'),
    checked: row.getAttribute('aria-checked'), text: row.textContent.trim(),
    open: !document.querySelector('.fxpop').hidden,
    focus: at.dataset?.switch ?? at.id ?? null }; })()"""
GONE = """(() => ({ cards: document.querySelectorAll('.p-viz').length,
  data: document.documentElement.dataset.viz ?? null, loops: window.__raf.size,
  open: window.__es.filter((e) => e.url.includes('host/frames') && !e.closed).length,
  frames: window.__es.filter((e) => e.url.includes('host/frames')).length,
  first: window.__first ?? null, inserted: window.__inserted ?? null,
  stored: (() => { try { return JSON.parse(localStorage.getItem('svolder.viz.2')); }
    catch { return 'unreadable'; } })() }))()"""


def menu_open(b):
    x, y = common.center(b, '.fxbtn')
    b.click(x, y)


def menu_row(b):
    x, y = common.center(b, '.fxpop [data-switch=viz]')
    b.click(x, y)


dac = common.Daemon(host='fake')
b = make(dac, graph=False, extra=FIRST)
try:
    settle_running(b)
    b.sleep(0.5)
    on = boxes(b)
    g = b.page(GONE)
    check('by default the visualizer is on: the card, a canvas loop and a frames stream',
          g['cards'] == 1 and g['data'] is None and g['loops'] == 1 and g['open'] == 1
          and g['first']['viz'] is None, g)
    menu_open(b)
    row = b.page(SWITCH)
    check('the effects menu has a Visualizer row, a switch that is on',
          row['open'] and not row['hidden'] and row['role'] == 'menuitemcheckbox'
          and row['checked'] == 'true' and row['text'] == 'Visualizer', row)
    shoot(b, 'menu-switch-on')
    b.press('Escape')
    # the keyboard reaches it and turns it off
    b.ev("document.querySelector('.fxbtn').focus()")
    b.press('Enter')
    b.ev("document.querySelector('.fxpop [data-switch=viz]').focus()")
    row = b.page(SWITCH)
    check('  the switch takes the focus in the open menu (a menu item the keys reach)',
          row['open'] and row['focus'] == 'viz', row)
    b.press('Enter')
    b.sleep(0.8)
    g = b.page(GONE)
    off = boxes(b)
    row = b.page(SWITCH)
    check('off from the keyboard: the card is gone from the page, with its canvas loop and its '
          'frames stream (the daemon holds no capture for this page)',
          g['cards'] == 0 and g['loops'] == 0 and g['open'] == 0 and g['data'] == 'off'
          and state(b) is None, g)
    check('  the choice is kept in svolder.viz.2, the menu closed, the switch off, the focus on '
          'the menu\'s button', g['stored'] and g['stored']['on'] is False
          and row['checked'] == 'false' and not row['open'] and row['focus'] == 'fx-button',
          [g['stored'], row])
    check('  Now playing has the whole band, 188 px tall at 1280 px',
          off['viz'] is None and abs(off['playing'][2] - off['band'][2]) <= 1
          and off['playing'][3] == 188 and off['band'][3] == 188, off)
    check('  the page moved once, by what the band lost, and no more',
          off['knob'][1] == on['knob'][1] - (240 - 188), [on['knob'], off['knob']])
    b.sleep(1.5)
    check('  then nothing moves', boxes(b) == off, [off, boxes(b)])
    shoot(b, 'desktop-card-off')
    before = g['stored']
    b.press('v')
    b.sleep(0.3)
    g = b.page(GONE)
    check('  v does nothing with the card off (the look is as it was kept)',
          g['stored'] == before and g['cards'] == 0 and errors(b) == [], [before, g['stored']])
    # a reload: off before the first paint, no frames stream ever opened
    b.goto(dac.url + '?effects=off', settle=1.5)
    g = b.page(GONE)
    again = boxes(b)
    check('a reload keeps it off: the page said so before any script ran, the frame holds no '
          'card, the card was not in the layout when the theme first inserted it, and no frames '
          'stream was ever opened',
          g['first'] == {'viz': 'off', 'card': None}
          and g['inserted'] == {'viz': 'off', 'card': 'none'}
          and g['cards'] == 0 and g['frames'] == 0 and g['loops'] == 0, g)
    check('  the same band, the page where it was', again == off, [off, again])
    menu_open(b)
    check('  and the menu shows the switch off', b.page(SWITCH)['checked'] == 'false')
    # on again, by click
    menu_row(b)
    s = settle_running(b)
    b.sleep(1.0)
    g = b.page(GONE)
    back = boxes(b)
    check('on again by click: the card is back, runs, with one canvas loop and one frames stream',
          g['cards'] == 1 and g['data'] is None and g['loops'] == 1 and g['open'] == 1
          and s['state'] == 'running' and pixels(b)['non'] > 500, [g, s])
    check('  kept on, and the band is as it was at the start',
          g['stored'] and g['stored']['on'] is True and back == on, [on, back])
    shoot(b, 'desktop-card-back')
    b.goto(dac.url + '?effects=off', settle=1.5)
    g = b.page(GONE)
    check('  and a reload keeps it on', g['cards'] == 1 and g['data'] is None, g)
    check('no page error from the switch', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# the page whole without storage: the switch works for the page's life
dac = common.Daemon(host='fake')
b = make(dac, kept='', **({'block_cookies': True} if browser == 'chromium'
                          else {'block_storage': True}))
try:
    settle_running(b)
    readable = b.page("(() => { try { localStorage.getItem('x'); return true; }"
                      " catch (e) { return false; } })()")
    menu_open(b)
    menu_row(b)
    b.sleep(0.6)
    g = b.page(GONE)
    check('with storage refused the switch still turns the card off for the page, no error',
          readable is False and g['cards'] == 0 and g['open'] == 0 and g['data'] == 'off'
          and errors(b) == [], [readable, g, errors(b)])
    menu_open(b)
    menu_row(b)
    s = settle_running(b)
    check('  and on again', state(b) is not None and s['state'] == 'running'
          and b.page(GONE)['data'] is None, s)
finally:
    b.close()
    dac.stop()

# a phone has no card with it off, and the page keeps the volume where it was
dac = common.Daemon(host='fake')
b = make(dac, size=(390, 600), kept=OFF_KEPT, extra=FIRST)
try:
    b.sleep(1.0)
    g = b.page(GONE)
    m = boxes(b)
    check('with the visualizer off a phone has no card and opens no frames stream',
          g['cards'] == 0 and g['frames'] == 0 and g['first']['viz'] == 'off'
          and m['viz'] is None and m['playing'][3] == 72, [g, m])
    b.page("(() => { window.scrollTo(0, 300); return 1; })()")
    b.sleep(0.5)
    shoot(b, 'phone-card-off')
    check('  and no page error', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# an amp page has the switch too
amp = common.Daemon('fake-amp', host='fake', name='Fake amp')
b = make(amp)
try:
    settle_running(b)
    menu_open(b)
    row = b.page(SWITCH)
    menu_row(b)
    b.sleep(0.6)
    g = b.page(GONE)
    m = boxes(b)
    check('an amp page has the switch, and off leaves Now playing the band',
          row['checked'] == 'true' and not row['hidden'] and g['cards'] == 0 and g['open'] == 0
          and m['viz'] is None and abs(m['playing'][2] - m['band'][2]) <= 1
          and m['playing'][3] == 188, [row, g, m])
    check('no page error on the amp page with the card off', errors(b) == [], errors(b))
finally:
    b.close()
    amp.stop()

# ------------------------------------------------------------------ a hidden page rests
dac = common.Daemon(host='fake')
b = make(dac)


def knob_raw(b):
    return b.page("document.querySelector('sv-knob')?.raw ?? null")


try:
    b.sleep(0.5)
    streams = device_streams(b)
    check('a shown page has one device stream open, and one frames stream',
          len(streams) == 1 and not streams[0]['closed'] and len(frames_streams(b)) == 1,
          [streams, frames_streams(b)])
    # a quick hide and show inside the grace: no stream is closed or opened
    set_hidden(b, True)
    b.sleep(1.0)
    set_hidden(b, False)
    b.sleep(2.6)
    check('a hide and show inside the grace opens and closes nothing',
          len(device_streams(b)) == 1 and not device_streams(b)[0]['closed']
          and len(frames_streams(b)) == 1 and not frames_streams(b)[0]['closed'],
          [device_streams(b), frames_streams(b)])
    # hidden for longer than the grace: both streams are closed, and it does not read as a fault
    set_hidden(b, True)
    b.sleep(1.5)
    check('  both streams are still open inside the 2 s grace',
          not device_streams(b)[-1]['closed'] and not frames_streams(b)[-1]['closed'],
          [device_streams(b), frames_streams(b)])
    b.sleep(1.5)
    check('a page hidden for more than 2 s closes its device stream and its frames stream',
          device_streams(b)[-1]['closed'] and frames_streams(b)[-1]['closed'],
          [device_streams(b), frames_streams(b)])
    rest = b.page("({ lost: document.body.classList.contains('lost'),"
                  " inert: document.getElementById('screen').hasAttribute('inert'),"
                  " said: document.getElementById('part-link')?.dataset.said ?? null })")
    check('  a rest does not read as a fault: no lost state, the screen is not inert, no notice',
          rest == {'lost': False, 'inert': False, 'said': None}, rest)
    b.sleep(2.5)
    check('  nothing opens again while it stays hidden (no retry after the 2 s wait)',
          len(device_streams(b)) == 1, device_streams(b))
    # a change made while hidden is not read by the page, and is on screen soon after it is shown
    shown = knob_raw(b)
    dac.write('Gungnir 2', 'volume', -20)
    b.sleep(1.0)
    check('a change made while hidden is not on the page yet', knob_raw(b) == shown,
          [shown, knob_raw(b)])
    set_hidden(b, False)
    t0 = time.time()
    after = shown
    while after == shown and time.time() - t0 < 4.0:
        b.sleep(0.1)
        after = knob_raw(b)
    took = time.time() - t0
    check('shown again, the change made while hidden is on screen within 2 s',
          after != shown and took < 2.0, [shown, after, round(took, 2)])
    b.sleep(2.5)
    streams = device_streams(b)
    check('  one new device stream was opened (its snapshot catches the page up), no storm',
          len(streams) == 2 and streams[0]['closed'] and not streams[1]['closed'], streams)
    lost = b.page("document.body.classList.contains('lost')")
    check('  and the page is not lost', lost is False, lost)
    # a second rest and show: the same, and the frames stream comes back too
    set_hidden(b, True)
    b.sleep(2.6)
    set_hidden(b, False)
    b.sleep(2.0)
    check('a second rest: one more device stream and one more frames stream, no more',
          len(device_streams(b)) == 3 and len(frames_streams(b)) == 3
          and not device_streams(b)[-1]['closed'] and not frames_streams(b)[-1]['closed'],
          [device_streams(b), frames_streams(b)])
    check('no page error through the rests', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ a rest across device changes
two = common.Daemon(host='fake', more=[('Second', '1112131415161718')])
b = make(two)
try:
    two.write('Gungnir 2', 'volume', -50)
    b.sleep(1.0)
    first = b.page("document.querySelector('.view').dataset.device")
    raw_first = knob_raw(b)

    def open_streams(b):
        return [e for e in device_streams(b) if not e['closed']]

    def settled(b, device, secs=4.0):
        """The page once it shows `device` with exactly one device stream open, or the last
        reading when `secs` are up. The second device is never found by the fake: its screen is the
        "Looking for" card, the first has the knob."""
        end = time.time() + secs
        while True:
            seen = b.page("({ device: document.querySelector('.view')?.dataset.device,"
                          " raw: document.querySelector('sv-knob')?.raw ?? null,"
                          " looking: document.querySelector('#screen .card.dim h2')"
                          "?.textContent ?? null })")
            seen['open'] = [e['url'] for e in open_streams(b)]
            if (seen['device'] == device and len(seen['open']) == 1) or time.time() > end:
                return seen
            b.sleep(0.1)

    # a device switch while the page rests
    set_hidden(b, True)
    b.sleep(2.6)
    check('a resting page has no device stream open', open_streams(b) == [], device_streams(b))
    b.page("(() => { document.querySelector('.devs a.dev:not([aria-current=page])').click();"
           " return 1; })()")
    b.sleep(1.5)
    other = b.page("document.querySelector('.view').dataset.device")
    check('  a device switch while it rests swaps the screen and opens no stream',
          other != first and open_streams(b) == [], [first, other, device_streams(b)])
    set_hidden(b, False)
    seen = settled(b, other)
    check('shown after a device switch: one device stream, on the device shown, with its screen',
          seen['device'] == other and len(seen['open']) == 1
          and stream_device(seen['open'][0]) == seen['device']
          and seen['raw'] is None and seen['looking'] == 'Looking for Second…',
          [first, other, seen])
    b.sleep(2.5)
    check('  and it stays one stream, no page error',
          len(open_streams(b)) == 1 and errors(b) == [], [open_streams(b), errors(b)])
    # a history step while the page rests
    set_hidden(b, True)
    b.sleep(2.6)
    b.page("(() => { history.back(); return 1; })()")
    b.sleep(2.0)
    check('a history step while it rests opens no stream', open_streams(b) == [],
          device_streams(b))
    set_hidden(b, False)
    seen = settled(b, first)
    check('shown after a history step: one device stream, on the device it went back to, '
          'with its screen',
          seen['device'] == first and len(seen['open']) == 1
          and stream_device(seen['open'][0]) == seen['device']
          and seen['raw'] == raw_first is not None and seen['looking'] is None,
          [first, seen, raw_first])
    # and forward again
    set_hidden(b, True)
    b.sleep(2.6)
    b.page("(() => { history.forward(); return 1; })()")
    b.sleep(2.0)
    set_hidden(b, False)
    seen = settled(b, other)
    check('shown after a history step forward: one stream, on the device it went forward to',
          seen['device'] == other and len(seen['open']) == 1
          and seen['looking'] == 'Looking for Second…', seen)
    # a woken page whose screen has nothing to read (the "Looking for" card) is ready at its first
    # event, `parts`: it does not wait out the 5 s that a stream that says nothing gets
    set_hidden(b, True)
    b.sleep(2.6)
    set_hidden(b, False)
    t0 = time.time()
    lost = True
    while lost and time.time() - t0 < 4.0:
        lost = b.page("document.body.classList.contains('lost')")
        b.sleep(0.05)
    took = time.time() - t0
    seen = settled(b, other)
    check('a woken page on a screen with nothing to read is ready at the first event, not after '
          'the 5 s wait', not lost and took < 2.0 and seen['looking'] == 'Looking for Second…',
          [lost, round(took, 2), seen])
    check('no page error across the rests and the device changes', errors(b) == [], errors(b))
finally:
    b.close()
    two.stop()

# ------------------------------------------------------------------ a page that starts hidden
dac = common.Daemon(host='fake')
b = make(dac, hidden=True)
try:
    b.sleep(2.5)
    check('a page that starts hidden opens no device stream, and is not lost',
          device_streams(b) == [] and b.page("document.body.classList.contains('lost')") is False,
          device_streams(b))
    set_hidden(b, False)
    b.sleep(1.5)
    check('  shown, it opens one and has its screen',
          len(device_streams(b)) == 1 and knob_raw(b) is not None, device_streams(b))
    check('no page error for a page that starts hidden', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ stopped frames, a woken page
# A daemon that is stopped (SIGSTOP) keeps its connections open and says nothing: the frames stop
# while the capture still reads running, as with a passive capture on a suspended sink, and a
# stream the page opens meanwhile gets no `hello` and no snapshot.


def freeze(daemon, on):
    try:
        os.kill(daemon.proc.pid, signal.SIGSTOP if on else signal.SIGCONT)
    except ProcessLookupError:
        pass  # the daemon is gone: nothing to hold or let go


# every POST the page makes, by its URL
POSTS = """(() => { window.__posts = []; const f = window.fetch.bind(window);
  window.fetch = (...a) => { if ((a[1] || {}).method === 'POST') window.__posts.push(String(a[0]));
    return f(...a); }; return 1; })()"""
WAITING = """({ lost: document.body.classList.contains('lost'),
  waking: document.body.classList.contains('waking'),
  glow: getComputedStyle(document.querySelector('.glow')).filter,
  inert: document.getElementById('screen').hasAttribute('inert'),
  said: document.getElementById('part-link')?.dataset.said ?? null,
  loops: window.__raf.size, posts: window.__posts.slice() })"""

dac = common.Daemon(host='fake')
b = make(dac, graph=False, effects='glow')
try:
    settle_running(b)
    b.sleep(2.0)
    check('it draws the music first', pixels(b)['non'] > 1000, pixels(b)['non'])
    b.page(POSTS)
    set_hidden(b, True)
    b.sleep(2.6)
    freeze(dac, True)
    set_hidden(b, False)
    b.sleep(1.2)
    s = state(b)
    h1 = pixels(b)['hash']
    b.sleep(0.6)
    h2 = pixels(b)['hash']
    w = b.page(WAITING)
    check('a card shown again after a rest draws nothing before the new stream speaks',
          s['state'] == 'idle' and h1 == h2 and w['loops'] == 0, [s['state'], h1, h2, w])
    check('a woken page waits: dimmed and inert as a lost link is, with no notice',
          w['lost'] and w['inert'] and w['said'] is None, w)
    check('  and the glow stays in colour: the wait dims the controls only',
          w['waking'] and w['glow'] == 'none', w)
    b.press('ArrowRight')
    x, y = common.center(b, 'sv-knob .step[data-step="1"]')
    b.click(x, y)
    b.sleep(0.4)
    check('  a key and a tap on + while it waits send nothing', b.page('window.__posts') == [],
          b.page('window.__posts'))
    freeze(dac, False)
    end = time.time() + 6.0
    while time.time() < end and b.page(WAITING)['lost']:
        b.sleep(0.1)
    w = b.page(WAITING)
    check('the first reading of the new stream ends the wait', not w['lost'] and not w['inert']
          and not w['waking'] and w['said'] is None, w)
    s = settle_running(b)
    b.sleep(1.0)
    check('  and the card draws again', s['state'] == 'running' and pixels(b)['non'] > 500, s)
    b.press('ArrowLeft')
    b.sleep(0.4)
    posts = b.page('window.__posts')
    check('  and a key step is sent again', len(posts) == 1 and posts[0].endswith('/volume'), posts)

    # frames that stop while the capture reads running: the waterfall keeps drifting and ends in
    # transparent (a waterfall crosses the card in 12 s)
    b.sleep(1.0)
    check('before they stop the waterfall holds music', pixels(b)['non'] > 1000, pixels(b)['non'])
    freeze(dac, True)
    end = time.time() + 20.0
    px = pixels(b)
    while time.time() < end and px['non'] > 0:
        b.sleep(0.5)
        px = pixels(b)
    s = state(b)
    loops = b.page('window.__raf.size')
    check('with the frames stopped and the capture still running, the waterfall drifts out to '
          'transparent and the animation goes on', px['non'] == 0
          and s['state'] == 'running' and loops == 1, [px['non'], s['state'], loops])
    freeze(dac, False)
    end = time.time() + 10.0
    while time.time() < end and pixels(b)['non'] == 0:
        b.sleep(0.25)
    check('  and the music comes back when the frames do', pixels(b)['non'] > 100,
          pixels(b)['non'])
    check('no page error with the daemon stopped and started', errors(b) == [], errors(b))
    # a lost link still greys the glow: the wait's exception is the wait's own
    dac.stop()
    end = time.time() + 8.0
    while time.time() < end and not b.page(WAITING)['lost']:
        b.sleep(0.2)
    w = b.page(WAITING)
    check('a lost link greys the glow, as before', w['lost'] and not w['waking']
          and 'grayscale' in w['glow'], w)
finally:
    freeze(dac, False)
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the effects hear the frames
# The glow's level is the volume's (the view's data-glow) plus the music's share while the card
# runs, and the volume's again once it stops; the smoke puffs on the beats and only then.
GLOW = """(() => {
  const fx = document.querySelector('.fx');
  const view = document.querySelector('#screen .view');
  return { lvl: parseFloat(fx.style.getPropertyValue('--lvl')),
    hearing: fx.classList.contains('hearing'),
    volume: Number(view && view.dataset.glow), level: fx.dataset.level }; })()"""
PUFFS = "(window.__fx.stats() ? window.__fx.stats().splats.puff : null)"


def glow(b):
    return b.page(GLOW)


dac = common.Daemon(host='fake')
b = make(dac, effects='glow')
try:
    s = settle_running(b)
    b.sleep(1.0)
    g = glow(b)
    check('with the glow on and the card running, the level is over the volume\'s and the layer '
          'says it hears music',
          g['level'] == 'glow' and g['hearing'] and g['lvl'] > g['volume'] + 0.01, g)
    seen = set()
    for _ in range(12):
        seen.add(glow(b)['lvl'])
        b.sleep(0.15)
    check('  and it moves with the music, not fixed', len(seen) >= 3, sorted(seen))
    set_hidden(b, True)
    b.sleep(0.3)
    g = glow(b)
    check('a hidden page takes the music off at once: the volume\'s level and no hearing',
          not g['hearing'] and abs(g['lvl'] - g['volume']) < 0.006, g)
    set_hidden(b, False)
    b.sleep(1.5)
    g = glow(b)
    check('  shown again, it follows the music again', g['hearing'] and g['lvl'] > g['volume'],
          g)
    check('no page error with the glow on', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

dac = common.Daemon(host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
b = make(dac, effects='glow')
try:
    b.sleep(2.0)
    g = glow(b)
    check('with no capture the glow is the volume\'s level and the layer hears nothing',
          not g['hearing'] and abs(g['lvl'] - g['volume']) < 0.006, g)
finally:
    b.close()
    dac.stop()

dac = common.Daemon(host='fake')
b = make(dac, effects='smoke')
try:
    b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
           ".then((m) => { window.__fx = m; return 1; })")
    settle_running(b)
    p0 = b.page(PUFFS)
    check('Smoke runs on the page', p0 is not None and b.page(
        "document.querySelectorAll('canvas.smoke').length") == 1, p0)
    end = time.time() + 14
    while time.time() < end and b.page(PUFFS) - p0 < 3:
        b.sleep(0.25)
    gained = b.page(PUFFS) - p0
    check('the smoke puffs on the music\'s beats while the card runs', gained >= 3, gained)
    set_hidden(b, True)
    b.sleep(0.5)
    p1 = b.page(PUFFS)
    b.sleep(2.0)
    check('a hidden page makes no more beat puffs', b.page(PUFFS) == p1, [p1, b.page(PUFFS)])
    check('no page error with the smoke on', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

dac = common.Daemon(host='fake', env={'SVOLDER_FAKE_CAPTURE': 'off'})
b = make(dac, effects='smoke')
try:
    b.page("import(new URL('assets/themes/ember/effects.js', document.baseURI).href)"
           ".then((m) => { window.__fx = m; return 1; })")
    b.sleep(1.0)
    s = until(b, lambda s: s['state'] == 'unavailable', 8.0)
    p0 = b.page(PUFFS)
    # an observation window, not a wait: several beats' worth of the fake's 120 BPM
    end = time.time() + 3.0
    while time.time() < end and b.page(PUFFS) == p0:
        b.sleep(0.25)
    check('with no frames the smoke makes no beat puffs',
          s['state'] == 'unavailable' and p0 is not None and b.page(PUFFS) == p0,
          [s['state'], p0, b.page(PUFFS)])
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ a guest, reduced motion
guest = common.Daemon(host='fake', web='guest = "read"')
b = make(guest, reduced=True)
try:
    s = settle_running(b)
    b.sleep(1.5)
    px = pixels(b)
    check('a guest sees the card run and draw', s['state'] == 'running' and px['non'] > 500, s)
    check('  with reduced motion it still draws, and the chip fades as for anyone',
          s['chipTransition'] in ('0.25s', '0.25s, 0.25s'), s['chipTransition'])
    b.press('v')
    check('  a guest\'s v cycles the look too', state(b)['look'] == 'lumibars', state(b))
    check('no page error for a guest', errors(b) == [], errors(b))
finally:
    b.close()
    guest.stop()

print(f'{browser}: {passed} checks passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
