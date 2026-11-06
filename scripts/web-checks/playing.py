# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Now playing card on the fake host: the band across a desktop page and the line on a phone
that a tap opens; the fake track, its cover, its path ("untouched" for the fake route on the fake
DAC) and its time counting; the transport posting to api/v1/host/press and the card following the
answer; a guest and a page with writes off seeing the card without buttons; the card's box
unchanged when the cover goes and when the player goes; an amp page with the same band; with the
visualizer off, a changed path's every chip stands inside the band at its lower fixed height (820,
1100 and 1280 px) with the verdict first; a path that does not fit its card ends in a chip of three
dots, the verdict first and the hops cut kept in the page, and comes back whole when the card
grows; on a phone the folded line (72 px) at 341, 426 and 784 px: a round play and a bare next, the
fold arrow small in the bottom-right corner with the bar stopping short of it, the title never
under the buttons, and a cut title looping slowly (30 px a second, a copy of the text a 40 px gap
behind it so the lap wraps with no jump, 2 s of rest at the start of each lap) for as long as the
line is folded and the page shows, whatever the system's motion setting, under a fade. Numbers
from the page, no pictures. Usage: python3 playing.py chromium|firefox"""
import json
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


# every fetch is noted, so a press and a guest's silence can be read off the page
HOOK = """(() => { window.__posts = []; const f = window.fetch;
  window.fetch = (...a) => { if (a[1] && a[1].method === 'POST') {
    window.__posts.push([String(a[0]), String(a[1].body)]); } return f(...a); }; })()"""
# what the card is, by the numbers: boxes, words, the cover, the buttons, the time
STATE = """(() => {
  const q = (s) => document.querySelector(s);
  const box = (el) => { if (!el) return null; const b = el.getBoundingClientRect();
    return [b.left, b.top, b.width, b.height].map((v) => Math.round(v * 10) / 10); };
  const text = (s) => q(s)?.textContent.trim() ?? null;
  const root = q('#part-playing'); const wide = q('.p-band');
  const img = q('#part-playing .np-cover img');
  const np = q('#part-playing .np'); const shown = (s) => {
    const el = q(s); return !!el && getComputedStyle(el).display !== 'none'; };
  return { band: box(root), wide: box(wide), cover: box(q('#part-playing .np-cover')),
    col: box(q('.view .col')),
    knob: box(q('sv-knob')), view: box(q('.view')), barBox: box(q('.np-bar')),
    title: text('.np-title'), artist: text('.np-artist'), source: text('.np-source'),
    none: text('.np-none'), path: text('.np-path'), verdict: text('.np-verdict'),
    chips: [...document.querySelectorAll('.np-path > *')].map((el) => [box(el), el.className]),
    pathClass: q('.np-path')?.className ?? null, hops: document.querySelectorAll('.np-hop').length,
    img: img ? { loaded: img.complete && img.naturalWidth > 0, src: img.getAttribute('src'),
                 hidden: img.hidden } : null,
    coverBg: q('#part-playing .np-cover') ? getComputedStyle(q('#part-playing .np-cover'))
      .backgroundColor : null,
    presses: [...document.querySelectorAll('#part-playing [data-press]')]
      .map((b) => b.dataset.press),
    status: np?.dataset.status ?? null, go: q('.np-btn.go')?.dataset.press ?? null,
    el: text('.np-el'), len: text('.np-len'), bar: q('.np-bar')?.style.getPropertyValue('--p'),
    open: root?.hasAttribute('data-expanded') ?? null,
    expanded: q('.np-fold')?.getAttribute('aria-expanded') ?? null,
    prevShown: shown('.np-btn.prev'), nextShown: shown('.np-btn.next'),
    pathShown: shown('.np-path'), timesShown: shown('.np-el'), sourceShown: shown('.np-source'),
    foldShown: shown('.np-fold'),
    posts: window.__posts.filter((p) => /host\\/press/.test(p[0])) }; })()"""


def state(b):
    return b.ev(STATE)


def until(b, cond, secs=8.0, every=0.25):
    """The state once `cond` holds of it, or the last one when `secs` are up."""
    end = time.time() + secs
    s = state(b)
    while not cond(s) and time.time() < end:
        b.sleep(every)
        s = state(b)
    return s


# What a browser that switched the visualizer off keeps.
OFF_KEPT = """try { localStorage.setItem('svolder.viz.2',
  JSON.stringify({ on: false, hinted: true })); } catch (e) {}"""


def make(daemon, size=(1280, 900), viz_off=False, **kw):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size, **kw)
    if viz_off:
        b.preload(OFF_KEPT)
    b.goto(daemon.url + '?effects=off', settle=1.0)
    b.ev(HOOK)
    b.sleep(0.4)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def seconds(clock):
    """`m:ss` or `h:mm:ss` as a number of seconds."""
    parts = [int(p) for p in clock.split(':')]
    return sum(p * 60 ** i for i, p in enumerate(reversed(parts)))


# ------------------------------------------------------------------ the fake host on a desktop
dac = common.Daemon(host='fake')
b = make(dac)
try:
    s = until(b, lambda s: s['img'] is not None and s['img']['loaded'])
    check('the card shows the fake title, artist, and album with the player',
          [s['title'], s['artist'], s['source']] ==
          ['Test Pressing', 'The Fakes', 'Demo Tape · Fake Player'], s)
    check('the cover is the daemon\'s, asked for by its hash, and it drew',
          s['img'] and s['img']['loaded'] and
          s['img']['src'].startswith('api/v1/host/cover?v='), s)
    check('the band stands above the two columns, across the view, Now playing its left half',
          s['band'] and s['col'] and s['wide'] and s['band'][1] + s['band'][3] <= s['col'][1] + 0.5
          and s['band'][0] == s['col'][0] and s['wide'][2] > 1100
          and abs(s['band'][2] - (s['wide'][2] - 24) / 2) <= 2, s)
    check('the transport is three buttons, pause among them (it plays)',
          s['presses'] == ['previous', 'pause', 'next'] and s['status'] == 'playing', s)
    check('on a desktop the whole card shows: the album line, prev and next, the times, the path',
          s['sourceShown'] and s['prevShown'] and s['nextShown'] and s['timesShown']
          and s['pathShown'] and not s['foldShown'], s)
    check('the path says untouched, with its three hops, for the fake route on the fake DAC',
          s['verdict'] == 'untouched' and s['hops'] == 3 and 'ok' in s['pathClass'].split(), s)
    band_playing = s['band']
    first = seconds(s['el'])
    check('the length reads 20:00 and the time reads m:ss',
          s['len'] == '20:00' and ':' in s['el'], s)
    b.sleep(2.3)
    later = state(b)
    check('the time counts forward while it plays', seconds(later['el']) >= first + 2,
          [s['el'], later['el']])
    check('the bar fills with it', float(later['bar'].rstrip('%')) > 0, later)

    # a track shorter than the time since its read stops at its length (the fake runs on past it)
    b.ev("document.querySelector('#part-playing .np').dataset.length = '1000000'")
    clamped = until(b, lambda s: s['el'] == '0:01', secs=3)
    check('the time stops at the length, the bar full',
          clamped['el'] == '0:01' and clamped['bar'] == '100.0%', clamped)

    # a hidden tab counts nothing, and counts again at once when it shows
    b.ev("""(() => { Object.defineProperty(document, 'hidden', { configurable: true,
        get: () => true }); document.dispatchEvent(new Event('visibilitychange'));
        document.querySelector('#part-playing .np').dataset.length = '1200000000'; })()""")
    b.sleep(0.3)
    hidden_a = state(b)['el']
    b.sleep(2.3)
    hidden_b = state(b)['el']
    b.ev("""(() => { delete document.hidden;
        document.dispatchEvent(new Event('visibilitychange')); })()""")
    b.sleep(0.3)
    shown_again = state(b)['el']
    check('a hidden tab does not count; it counts again when shown',
          hidden_a == hidden_b and seconds(shown_again) > seconds(hidden_b),
          [hidden_a, hidden_b, shown_again])

    # the pause button posts a press and the card follows the player's answer
    x, y = common.center(b, '.np-btn.go')
    b.click(x, y)
    s = until(b, lambda s: s['status'] == 'paused')
    check('pause posts api/v1/host/press with the button named',
          s['posts'] == [['api/v1/host/press', '{"button":"pause"}']], s['posts'])
    check('the card follows the answer: paused, and the button now says play',
          s['status'] == 'paused' and s['go'] == 'play', s)
    frozen = s['el']
    b.sleep(2.3)
    check('a paused track does not count', state(b)['el'] == frozen, [frozen, state(b)['el']])
    check('the band is the same box, playing or paused', s['band'] == band_playing,
          [band_playing, s['band']])
    check('no page error on the desktop', errors(b) == [], errors(b))
    # a cover the daemon no longer holds (a real 404, which the browser logs) is hidden, its box
    # stays
    box_before = state(b)['cover']
    b.ev("""document.querySelector('#part-playing .np-cover img')
        .setAttribute('src', 'api/v1/host/cover?v=deadbeefdeadbeef')""")
    b.sleep(1.0)
    gone = b.ev("""(() => { const i = document.querySelector('#part-playing .np-cover img');
        return i && [i.hidden, getComputedStyle(i).display]; })()""")
    check('a cover that answers 404 is hidden, not drawn broken, and its box stays',
          gone == [True, 'none'] and state(b)['cover'] == box_before, [gone, box_before])
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the phone's line
dac = common.Daemon(host='fake')
b = make(dac, size=(390, 800))
try:
    s = until(b, lambda s: s['img'] is not None and s['img']['loaded'])
    check('on a phone the compact line sits above the volume, 72 px tall',
          s['band'] and s['knob'] and s['band'][1] + s['band'][3] <= s['knob'][1]
          and s['band'][3] == 72, s)
    check('  folded: play and next, no previous, no times, no path, no album line',
          s['presses'] == ['previous', 'pause', 'next'] and not s['prevShown']
          and s['nextShown'] and not s['timesShown'] and not s['pathShown']
          and not s['sourceShown'] and s['foldShown'] and s['go'] == 'pause', s)
    check('  folded: a thin bar runs along its foot, inside the line',
          s['barBox'] and s['barBox'][3] == 3 and s['barBox'][2] > 280
          and s['band'][1] + s['band'][3] - 12 <= s['barBox'][1] + 3 <= s['band'][1] + s['band'][3]
          and s['bar'] != '', s)
    folded = s['band']
    x, y = common.center(b, '.np-title')
    b.click(x, y)
    s = state(b)
    check('a tap opens it into the whole card: cover, transport, times, path',
          s['open'] and s['expanded'] == 'true' and s['prevShown'] and s['nextShown']
          and s['timesShown'] and s['pathShown'] and s['sourceShown'] and s['band'][3] > 72
          and s['verdict'] == 'untouched', s)
    opened = s['band']
    check('  opened: the line grew down, its top and width stand',
          opened[1] == folded[1] and opened[2] == folded[2], [folded, opened])
    # the card is swapped by the stream; the open line stays open
    x, y = common.center(b, '.np-btn.go')
    b.click(x, y)
    s = until(b, lambda s: s['status'] == 'paused')
    check('a swap of the card keeps it open', s['status'] == 'paused' and s['open']
          and s['expanded'] == 'true', s)
    x, y = common.center(b, '.np-head')
    b.click(x, y)
    s = state(b)
    check('a second tap folds it again, to the same box',
          not s['open'] and s['expanded'] == 'false' and s['band'] == folded, [folded, s])
    check('no page error on the phone', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the folded line at real widths
# The widths of the user's phone: Firefox 341 px (125 % text zoom), Chrome 426 px, Firefox in
# desktop mode 784 px (under the 820 px band, so still the phone's line).
BOXES = """(() => {
  const r = (s) => { const e = document.querySelector(s); if (!e) return null;
    const b = e.getBoundingClientRect(); return { l: b.left, t: b.top, r: b.right, b: b.bottom,
      w: b.width, h: b.height }; };
  const shown = (s) => { const e = document.querySelector(s);
    return !!e && getComputedStyle(e).display !== 'none'; };
  const fold = document.querySelector('.np-fold');
  return { card: r('#part-playing'), who: r('.np-who'), title: r('.np-title'),
    go: r('.np-btn.go'), next: r('.np-btn.next'), fold: r('.np-fold'), bar: r('.np-bar'),
    prev: shown('.np-btn.prev'), nextShown: shown('.np-btn.next'),
    tag: fold?.tagName ?? null, label: fold?.getAttribute('aria-label') ?? null,
    nextRing: getComputedStyle(document.querySelector('.np-btn.next')).borderTopColor,
    nextRadius: getComputedStyle(document.querySelector('.np-btn.next')).borderTopLeftRadius,
    goRadius: getComputedStyle(document.querySelector('.np-btn.go')).borderTopLeftRadius,
    vw: document.documentElement.clientWidth }; })()"""
# a title that is cut, and what its loop is doing
LOOP = """(() => {
  const t = document.querySelector('.np-title'); const s = t.firstElementChild;
  const c = t.querySelector('.np-copy');
  const a = s.getAnimations(); const ca = c ? c.getAnimations() : [];
  const cs = getComputedStyle(t);
  const faded = (v) => !!v && v !== 'none';
  const tb = t.getBoundingClientRect(); const sb = s.getBoundingClientRect();
  const cb = c ? c.getBoundingClientRect() : null;
  const timing = a[0] ? a[0].effect.getTiming() : null;
  return { cut: t.classList.contains('cut'), anims: a.length, canims: ca.length,
    copies: t.querySelectorAll('.np-copy').length, hidden: c?.getAttribute('aria-hidden') ?? null,
    select: c ? getComputedStyle(c).userSelect : null,
    over: sb.width - t.clientWidth, w: sb.width, x: sb.left - tb.left,
    cx: cb ? cb.left - tb.left : null, gap: cb ? cb.left - sb.right : null,
    dur: timing ? timing.duration : null,
    iter: timing ? (timing.iterations === Infinity ? 'infinite' : timing.iterations) : null,
    easing: timing ? timing.easing : null, time: a[0] ? a[0].currentTime : null,
    ctime: ca[0] ? ca[0].currentTime : null, state: a[0] ? a[0].playState : null,
    fade: faded(cs.maskImage) || faded(cs.webkitMaskImage), overflow: cs.textOverflow,
    text: s.textContent, now: performance.now(),
    reduced: matchMedia('(prefers-reduced-motion: reduce)').matches }; })()"""
# both animations of the title taken to a time in the lap, as a person watching could not tell
SEEK = """((ms) => { const t = document.querySelector('.np-title');
  for (const a of t.getAnimations({ subtree: true })) a.currentTime = ms; return 1; })"""
# every frame's positions for `ms` milliseconds, to see what the wrap does between two frames
FRAMES = """((ms) => new Promise((done) => { const t = document.querySelector('.np-title');
  const s = t.firstElementChild; const c = t.querySelector('.np-copy'); const out = [];
  const t0 = performance.now(); const left = t.getBoundingClientRect().left;
  const frame = () => { const a = s.getAnimations()[0];
    out.push({ time: a.currentTime, total: a.effect.getTiming().duration,
      x: s.getBoundingClientRect().left - left, cx: c.getBoundingClientRect().left - left,
      w: s.getBoundingClientRect().width });
    if (performance.now() - t0 < ms) requestAnimationFrame(frame); else done(out); };
  requestAnimationFrame(frame); }))"""
# a part swapped in for another track, as the stream's out-of-band swap does it
SWAP = """((T, idle) => { const old = document.getElementById('part-playing');
  let el;
  if (idle) { window.__stash = old; el = document.createElement('section'); el.id = 'part-playing';
    el.className = 'p-playing idle'; el.innerHTML = '<p class="np-none">No player</p>'; }
  else { el = (window.__stash || old).cloneNode(true); window.__stash = null;
    el.querySelector('.np-title span').textContent = T; }
  const fragment = document.createDocumentFragment(); fragment.append(el);
  document.body.dispatchEvent(new CustomEvent('htmx:oobBeforeSwap',
    { bubbles: true, detail: { target: old, fragment } }));
  old.replaceWith(el); document.body.dispatchEvent(new CustomEvent('htmx:oobAfterSwap',
    { bubbles: true, detail: { target: el } })); return 1; })"""
# the page's visibility is a switch the check holds
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true,
  get: () => true }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
MID = 'Everything In Its Right Place'
LONG = ('A Very Long Track Title That Cannot Possibly Fit In The Room Of A Phone Line, Not Even '
        'At Seven Hundred And Eighty Four Pixels, Remastered In Stereo, Deluxe Edition (Live)')


def loop(b):
    return b.ev(LOOP)


def waiting(b, cond, secs=8.0, every=0.1):
    """The loop's state once `cond` holds of it, or the last one when `secs` are up."""
    end = time.time() + secs
    g = loop(b)
    while not cond(g) and time.time() < end:
        b.sleep(every)
        g = loop(b)
    return g


def pace(b, secs=1.0):
    """The pace of the text in pixels a second over `secs`, by the page's own clock."""
    g1 = loop(b)
    b.sleep(secs)
    g2 = loop(b)
    return (g1['x'] - g2['x']) / ((g2['now'] - g1['now']) / 1000), g1, g2


def swap(b, title, idle=False):
    b.ev(SWAP + f'({json.dumps(title)}, {json.dumps(idle)})')


dac = common.Daemon(host='fake')
b = make(dac, size=(341, 800))
try:
    until(b, lambda s: s['img'] is not None and s['img']['loaded'])
    first = None
    for width in (341, 426, 784):
        b.resize(width, 800)
        x = b.ev(BOXES)
        words = f'({width} px)'
        check(f'the folded line is 72 px at the page width the phone reports {words}',
              x['vw'] == width and x['card']['h'] == 72, x)
        check(f'  a round 40 px play and a bare next (no ring) with a 32 x 44 px tap area, no '
              f'previous {words}',
              abs(x['go']['w'] - 40) < .5 and abs(x['go']['h'] - 40) < .5
              and x['goRadius'] != '0px' and not x['prev'] and x['nextShown']
              and abs(x['next']['w'] - 32) < .5 and abs(x['next']['h'] - 44) < .5
              and x['nextRing'] == 'rgba(0, 0, 0, 0)' and x['nextRadius'] == '0px'
              and x['next']['l'] >= x['go']['r'], x)
        check(f'  the title stays clear of the buttons and inside the card {words}',
              x['who']['r'] <= x['go']['l'] + .5 and x['title']['r'] <= x['go']['l'] + .5
              and x['title']['l'] > x['card']['l'] and x['title']['w'] > 90, x)
        check(f'  the fold arrow is a button in the card\'s bottom-right corner, small and quiet '
              f'{words}',
              x['tag'] == 'BUTTON' and x['fold']['w'] <= 24 and x['fold']['h'] <= 18
              and x['card']['r'] - x['fold']['r'] <= 12 and x['card']['b'] - x['fold']['b'] <= 2
              and x['fold']['t'] > x['card']['b'] - 20 and x['fold']['t'] >= x['next']['b'] - 1,
              x)
        check(f'  the thin bar stops short of the arrow, still along the foot {words}',
              x['bar']['r'] <= x['fold']['l'] and x['bar']['h'] == 3
              and x['card']['b'] - x['bar']['b'] <= 12 and x['bar']['w'] > 200, x)
        if first is None:
            first = x
        folded_card = x['card']
        # a tap on the line opens it, a tap on the arrow too, and the arrow takes the keyboard
        fx, fy = common.center(b, '.np-fold')
        b.click(fx, fy)
        s = state(b)
        check(f'  a tap on the arrow opens the card {words}',
              s['open'] and s['expanded'] == 'true' and s['prevShown'] and s['band'][3] > 72, s)
        opened = b.ev(BOXES)
        check(f'  opened: the card grew down only, its top, left and width stand {words}',
              opened['card']['t'] == folded_card['t'] and opened['card']['l'] == folded_card['l']
              and opened['card']['w'] == folded_card['w'], [folded_card, opened['card']])
        fx, fy = common.center(b, '.np-fold')
        b.click(fx, fy)
        s = state(b)
        back = b.ev(BOXES)
        check(f'  a second tap on the arrow folds it, every box back where it was {words}',
              not s['open'] and back['card'] == folded_card and back['go'] == x['go']
              and back['next'] == x['next'] and back['fold'] == x['fold']
              and back['title'] == x['title'], [x, back])
        tx, ty = common.center(b, '.np-title')
        b.click(tx, ty)
        check(f'  a tap on the line opens it {words}', state(b)['open'], state(b))
        b.click(*common.center(b, '.np-title'))
        b.ev("document.querySelector('.np-fold').focus()")
        focused = b.ev("document.activeElement === document.querySelector('.np-fold')")
        check(f'  the arrow takes the keyboard focus {words}', focused is True, focused)
        # a title that fits is shown whole with its ellipsis rule, and does not move
        g = loop(b)
        check(f'  the fake\'s short title is not cut and ends clear of the fade: no loop, no copy '
              f'{words}',
              not g['cut'] and g['over'] <= -14 and g['anims'] == 0 and g['copies'] == 0
              and g['fade'] and g['overflow'] == 'clip', g)
        # a title far too long for its room: cut, faded, looping, never under the buttons
        swap(b, LONG)
        g = waiting(b, lambda g: g['anims'] == 1, secs=3)
        check(f'  a long title is cut: a fade at its right edge in place of the dots, and it '
              f'loops {words}',
              g['cut'] and g['fade'] and g['overflow'] == 'clip' and g['anims'] == 1
              and g['canims'] == 1 and g['iter'] == 'infinite' and g['over'] > 100, g)
        check(f'  its copy is aria-hidden, not selectable, and a 40 px gap behind the text '
              f'{words}',
              g['copies'] == 1 and g['hidden'] == 'true' and g['select'] == 'none'
              and abs(g['gap'] - 40) < 1, g)
        g = waiting(b, lambda g: g['x'] < -20, secs=4)
        under = b.ev(BOXES)
        check(f'  mid-lap the title is still clear of the buttons and the card stands {words}',
              under['title']['r'] <= under['go']['l'] + .5 and under['card'] == folded_card,
              under)
        rate, g1, g2 = pace(b)
        check(f'  it moves at 30 px a second, 30 +- 3 {words}', 27 <= rate <= 33, [rate, g1, g2])
        g = loop(b)
        check(f'  and it is still looping, the same animation, a few seconds on {words}',
              g['anims'] == 1 and g['state'] == 'running' and g['x'] < -20, g)
        swap(b, LONG)
        g = loop(b)
        check(f'  a part swapped in for the same track carries the loop on, no snap to the start '
              f'{words}', g['anims'] == 1 and g['x'] < -20 and g['time'] > 1500, g)
        # the page back to what the fake says for the next width
        b.goto(dac.url + '?effects=off', settle=1.0)
        b.ev(HOOK)
    check('no page error at the measured widths', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the title's loop, closely
dac = common.Daemon(host='fake')
b = make(dac, size=(341, 800))
try:
    until(b, lambda s: s['img'] is not None and s['img']['loaded'])
    g = loop(b)
    check('a title that fits is not cut and ends clear of the fade: no loop, no copy',
          not g['cut'] and g['over'] <= -14 and g['fade'] and g['overflow'] == 'clip'
          and g['anims'] == 0 and g['copies'] == 0, g)
    # no ellipsis and no script needed for the fade: without the mark the style still clips it
    swap(b, LONG)
    g = b.ev("(() => { document.querySelector('.np-title').classList.remove('cut'); return ("
             + LOOP + '); })()')
    check('the fade and the clip come from the style alone, before the script marks the title: '
          'no "..." on first paint', not g['cut'] and g['fade'] and g['overflow'] == 'clip', g)
    swap(b, 'Test Pressing')
    b.sleep(0.3)
    swap(b, MID)
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('a new track whose title is cut loops, with the fade in place of the dots',
          g['cut'] and g['fade'] and g['overflow'] == 'clip' and g['anims'] == 1
          and g['canims'] == 1 and 40 < g['over'] < 120, g)
    check('  forever (iterations Infinity), linearly, one lap = 2 s of rest and the text plus '
          'the gap at 30 px a second',
          g['iter'] == 'infinite' and g['easing'] == 'linear'
          and abs(g['dur'] - (2000 + (g['w'] + 40) / 30 * 1000)) < 5, g)
    check('  the copy is aria-hidden, not selectable, the only one, a 40 px gap behind the text',
          g['copies'] == 1 and g['hidden'] == 'true' and g['select'] == 'none'
          and abs(g['gap'] - 40) < 1, g)
    check('  the loop starts with its rest: the title at its start, whole from its first letter',
          g['time'] < 2000 and abs(g['x']) < 1, g)
    g = waiting(b, lambda g: g['x'] < -1, secs=4)
    check('  the rest lasts about 2 s, then the text moves to the left',
          2000 <= g['time'] < 3000 and g['x'] < -1, g)
    rate, g1, g2 = pace(b)
    check('  at 30 px a second, 30 +- 3, from the start of the move', 27 <= rate <= 33,
          [rate, g1, g2])
    rate2, g3, g4 = pace(b)
    check('  and at the same pace a second later: no easing', 27 <= rate2 <= 33, [rate2, g3, g4])
    # a sync with nothing to change leaves the running animation alone, not a new one in its place
    b.ev("window.__lap = document.querySelector('.np-title span').getAnimations()[0]")
    b.ev("document.dispatchEvent(new Event('visibilitychange'))")
    b.ev("document.fonts.dispatchEvent(new Event('loadingdone'))")
    b.sleep(0.3)
    same = b.ev("window.__lap === document.querySelector('.np-title span').getAnimations()[0]")
    check('syncing again while nothing changed keeps the very same animation going',
          same is True and loop(b)['state'] == 'running', same)
    # the wrap: seek to 1.5 s before the lap's end, then watch every frame across it
    total = g['dur']
    b.ev(SEEK + f'({total - 1500})')
    frames = b.ev(FRAMES + '(3500)')
    laps = [f['time'] % total for f in frames]
    wrapped = [i for i in range(1, len(laps)) if laps[i] < laps[i - 1]]
    check('the lap wraps while it is watched', len(wrapped) == 1, [len(frames), laps[:3]])
    near = [f for f in frames if abs(f['cx']) < 60 or abs(f['x']) < 60]
    pos = [f['x'] if abs(f['x']) <= abs(f['cx']) else f['cx'] for f in near]
    jump = max(abs(pos[i] - pos[i - 1]) for i in range(1, len(pos)))
    check('  the wrap does not jump: the copy reaches the start, the text goes on from it '
          '(under 3 px between two frames)', jump < 3, [jump, len(near)])
    check('  the gap between the text and its copy stands in every frame (40 px +- 1)',
          all(abs(f['cx'] - f['x'] - f['w'] - 40) < 1 for f in frames), frames[:2])
    b.ev(SEEK + f'({total - 1})')
    g = loop(b)
    check('  at the lap\'s end the copy stands where the text started (+- 1 px)',
          abs(g['cx']) < 1 and g['x'] < -(g['w'] + 38), g)
    b.ev(SEEK + f'({2 * total + 300})')
    g = loop(b)
    check('  still looping after two laps, at rest at the start of the third',
          g['anims'] == 1 and g['state'] == 'running' and abs(g['x']) < 1, g)
    b.ev(SEEK + f'({2 * total + 3000})')
    g = loop(b)
    check('  and moving in it', g['x'] < -20 and g['x'] > -g['w'], g)
    # the same track in a swapped part carries the loop on; another track starts at its rest
    b.ev(SEEK + '(3500)')
    before = loop(b)
    swap(b, MID)
    g = loop(b)
    check('the same track swapped in again carries the loop on, no snap to the start',
          g['anims'] == 1 and g['canims'] == 1 and g['x'] <= before['x'] + 1 and g['x'] < -20
          and g['time'] >= before['time'] - 50, [before, g])
    g1 = loop(b)
    b.sleep(0.5)
    g2 = loop(b)
    check('  the copy goes along with it (the gap holds)', abs(g2['gap'] - 40) < 1
          and g2['x'] < g1['x'], [g1, g2])
    swap(b, MID + ' (Live)')
    g = loop(b)
    check('the next track, cut, starts its loop from its start, resting first',
          g['anims'] == 1 and g['cut'] and g['time'] < 1500 and abs(g['x']) < 1, g)
    # the page hides: the loop stops at once, the title at its start and the copy still
    waiting(b, lambda g: g['x'] < -10, secs=4)
    b.ev(HIDE)
    g = waiting(b, lambda g: g['anims'] == 0, secs=1.5)
    check('the page hiding stops the loop at once, the title back at its start',
          g['anims'] == 0 and g['canims'] == 0 and abs(g['x']) < 1, g)
    swap(b, MID + ' (Mono)')
    b.sleep(1.0)
    g = loop(b)
    check('a track that comes while the page is hidden does not loop', g['anims'] == 0
          and g['text'].endswith('(Mono)'), g)
    b.ev(SHOW)
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('the page showing again starts the loop, from its rest',
          g['anims'] == 1 and g['canims'] == 1 and g['cut'] and g['time'] < 1500, g)
    # open: the title is whole (it wraps), no fade, no copy and no loop; folded again: it starts
    b.click(*common.center(b, '.np-title'))
    g = loop(b)
    check('opened, the title is not cut: no fade, no copy, no loop, at its start',
          not g['cut'] and not g['fade'] and g['anims'] == 0 and g['copies'] == 0
          and abs(g['x']) < 1, g)
    b.sleep(1.2)
    check('  and it stays still while the card is open',
          loop(b)['anims'] == 0 and loop(b)['copies'] == 0, loop(b))
    swap(b, MID + ' (Live In Paris)')
    b.sleep(0.5)
    g = loop(b)
    check('a track that comes while the card is open has no loop and no copy',
          g['anims'] == 0 and g['copies'] == 0, g)
    b.click(*common.center(b, '.np-title'))
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('the line folding starts the loop, from its rest, with the copy',
          g['anims'] == 1 and g['canims'] == 1 and g['cut'] and g['copies'] == 1
          and g['time'] < 1500, g)
    b.click(*common.center(b, '.np-title'))
    b.sleep(0.3)
    b.click(*common.center(b, '.np-title'))
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('  and folding again for the same track starts it again',
          g['anims'] == 1 and g['time'] < 2000, g)
    # the player quits and comes back with the same track: it loops again; no title, no copy
    swap(b, '', idle=True)
    b.sleep(0.5)
    check('with no player there is no title and no copy',
          b.ev("document.querySelectorAll('.np-title, .np-copy').length") == 0)
    swap(b, MID + ' (Live In Paris)')
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('the player returning with the same track gets its loop',
          g['anims'] == 1 and g['cut'] and g['copies'] == 1, g)
    # a tap acts in the same task, not a frame later when the card's size is seen to change
    tap = ("(() => { document.querySelector('#part-playing .np-head').click(); return ("
           + LOOP + '); })()')
    g = b.ev(tap)
    check('a tap that opens the card stops the loop and takes the copy away at once',
          not g['cut'] and g['anims'] == 0 and g['copies'] == 0, g)
    g = b.ev(tap)
    check('  a tap that folds it starts the loop, with its copy, at once',
          g['cut'] and g['anims'] == 1 and g['canims'] == 1 and g['copies'] == 1, g)
    # a font that comes in later changes the text's width: the loop is fitted to it again
    b.ev("document.querySelector('.np-title span').style.fontSize = '4px'")
    b.ev("document.fonts.dispatchEvent(new Event('loadingdone'))")
    g = loop(b)
    check('a font coming in that makes the text fit: no longer cut, no copy, no loop',
          not g['cut'] and g['copies'] == 0 and g['anims'] == 0, g)
    b.ev("document.querySelector('.np-title span').style.fontSize = ''")
    b.ev("document.fonts.dispatchEvent(new Event('loadingdone'))")
    g = loop(b)
    check('  and one that makes it long again: cut, copied and looping again',
          g['cut'] and g['copies'] == 1 and g['anims'] == 1 and g['canims'] == 1, g)
    # the card's own size changes with no redraw of the screen: the observer fits the title again
    b.ev("document.getElementById('part-playing').style.width = '900px'")
    b.sleep(0.4)
    g = loop(b)
    check('a card that grows till the title fits: not cut, no copy, no loop',
          not g['cut'] and g['copies'] == 0 and g['anims'] == 0, g)
    b.ev("document.getElementById('part-playing').style.width = ''")
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('  and back to its size: cut, copied and looping from its rest',
          g['cut'] and g['copies'] == 1 and g['anims'] == 1 and g['time'] < 2000, g)
    # the card made wider: the title fits, the loop stops; narrower again: it starts
    b.resize(784, 800)
    g = loop(b)
    check('at 784 px the same title fits: not cut, no copy, no loop',
          not g['cut'] and g['copies'] == 0 and g['anims'] == 0, g)
    b.resize(341, 800)
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('back at 341 px it is cut again, copied and looping from its rest',
          g['cut'] and g['fade'] and g['copies'] == 1 and g['anims'] == 1 and g['time'] < 2000, g)
    # a desktop's band: a title that is long is whole there, with no copy and no loop
    b.resize(1280, 800)
    swap(b, LONG)
    b.sleep(0.5)
    g = loop(b)
    check('on a desktop the band shows the title whole: not cut, no copy, no loop',
          not g['cut'] and g['copies'] == 0 and g['anims'] == 0, g)
    check('no page error with the loop', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ a long title from the daemon
# SVOLDER_FAKE_TITLE gives the fake track a title the folded line cuts: the real path, no simulated
# swap (the page's first paint, then the stream's own swap on a press).
dac = common.Daemon(host='fake', env={'SVOLDER_FAKE_TITLE': MID})
b = make(dac, size=(341, 800))
try:
    g = waiting(b, lambda g: g['anims'] == 1, secs=5)
    check('the daemon\'s own long title is the card\'s title, cut, and it loops from the first '
          'paint', g['text'] == MID and g['cut'] and g['fade'] and g['anims'] == 1
          and g['canims'] == 1 and g['iter'] == 'infinite' and g['copies'] == 1, g)
    g = waiting(b, lambda g: g['x'] < -15, secs=4)
    before = g['x']
    x, y = common.center(b, '.np-btn.go')
    b.click(x, y)
    s = until(b, lambda s: s['status'] == 'paused')
    g = loop(b)
    check('a real press swaps the part for the same track mid-lap: the loop goes on, no snap',
          s['status'] == 'paused' and g['anims'] == 1 and g['x'] < -10 and g['x'] <= before + 1,
          [before, g])
    check('  and the card\'s accessible title is still the track once, not twice',
          b.ev("document.querySelector('.np-title').firstElementChild.textContent") == MID
          and b.ev("document.querySelectorAll('.np-copy[aria-hidden=true]').length") == 1)
    check('no page error with the daemon\'s long title', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ reduced motion: it still loops
dac = common.Daemon(host='fake')
b = make(dac, size=(341, 800), reduced=True)
try:
    until(b, lambda s: s['img'] is not None and s['img']['loaded'])
    swap(b, MID)
    g = waiting(b, lambda g: g['anims'] == 1, secs=3)
    check('with the system asking for less motion the cut title still loops, forever',
          g['reduced'] is True and g['anims'] == 1 and g['iter'] == 'infinite', g)
    g = waiting(b, lambda g: g['x'] < -25, secs=6)
    check('  and it moves', g['x'] < -25, g)
    check('no page error under reduced motion', errors(b) == [], errors(b))
finally:
    b.close()
    dac.stop()

# ------------------------------------------------------------------ the cover goes
gone = common.Daemon(host='fake')
b = make(gone)
try:
    s = state(b)
    with_cover = until(b, lambda s: s['img'] is not None)
    box = [with_cover['band'], with_cover['cover']]
    check('the fake cover shows first', with_cover['img'] is not None, with_cover)
    gone.drop_cover()
    after = until(b, lambda s: s['img'] is None, secs=10)
    check('with the cover gone the card shows no picture and the title still', after['img'] is None
          and after['title'] == 'Test Pressing', after)
    check('  the card\'s box and the cover\'s box are exactly where they were',
          [after['band'], after['cover']] == box, [box, [after['band'], after['cover']]])
    check('  the space is given back to the background (nothing drawn there)',
          after['coverBg'] in ('rgba(0, 0, 0, 0)', 'transparent'), after)
    check('no page error with the cover gone', errors(b) == [], errors(b))
finally:
    b.close()
    gone.stop()

# ------------------------------------------------------------------ the player goes
quit_ = common.Daemon(host='fake')
b = make(quit_)
try:
    playing = until(b, lambda s: s['title'] is not None)
    check('the player is there first', playing['title'] == 'Test Pressing', playing)
    quit_.drop_player()
    after = until(b, lambda s: s['title'] is None, secs=10)
    check('with the player gone the card is one quiet line, no buttons, no stale title or art',
          after['none'] == 'No player' and after['presses'] == [] and after['title'] is None
          and after['img'] is None and after['path'] is None, after)
    check('  the card\'s box is exactly where it was', after['band'] == playing['band'],
          [playing['band'], after['band']])
    b.resize(390, 800)
    s = state(b)
    check('  on a phone the quiet line is as tall as the folded line (72 px)',
          s['band'][3] == 72, s)
    check('no page error with the player gone', errors(b) == [], errors(b))
finally:
    b.close()
    quit_.stop()

# ------------------------------------------------------------------ the open card, the player quits
quit_ = common.Daemon(host='fake')
b = make(quit_, size=(390, 800))
try:
    x, y = common.center(b, '.np-title')
    b.click(x, y)
    s = state(b)
    check('the card is open when the player quits', s['open'] and s['band'][3] > 72, s)
    quit_.drop_player()
    s = until(b, lambda s: s['title'] is None, secs=12)
    check('an open card whose player quits folds to the quiet line, 72 px, not expanded',
          s['none'] == 'No player' and s['open'] is False and s['band'][3] == 72, s)
    check('no page error when an open card folds', errors(b) == [], errors(b))
finally:
    b.close()
    quit_.stop()

# ------------------------------------------------------------------ a changed path, in the band
# The band is 240 px tall beside the visualizer; with it off Now playing has the whole band, 168 px
# below 1100 px and 188 px from there (the cover grows from 64 to 84 px), the least that holds the
# changed path's two rows of chips with the room under them.
PATH = """(() => {
  const p = document.querySelector('#part-playing').getBoundingClientRect();
  const kids = [...document.querySelectorAll('.np-path > *')];
  const shown = kids.filter((k) => !k.classList.contains('np-cut'));
  const r = shown.map((k) => k.getBoundingClientRect());
  return { cut: kids.length - shown.length,
    more: shown.filter((k) => k.classList.contains('np-more')).length,
    first: shown[0].className, last: shown[shown.length - 1].className,
    inside: r.every((c) => c.left >= p.left && c.right <= p.right && c.bottom <= p.bottom - 12),
    title: document.querySelector('.np-more')?.title ?? null,
    kept: kids.filter((k) => k.classList.contains('np-cut')).map((k) => k.textContent.trim()),
    band: p.height }; })()"""
for viz_off, width in [(False, 820), (False, 1100), (False, 1280),
                       (True, 820), (True, 1100), (True, 1280)]:
    height = 240 if not viz_off else 168 if width < 1100 else 188
    width_words = f'{width} px' + (', visualizer off' if viz_off else '')
    changed = common.Daemon(host='fake', env={'SVOLDER_FAKE_ROUTE': 'changed'})
    b = make(changed, size=(width, 900), viz_off=viz_off)
    try:
        s = until(b, lambda s: s['verdict'] is not None)
        check(f'the changed route reads changed, naming each change ({width_words})',
              'changed' in (s['pathClass'] or '').split() and all(
                  word in s['verdict'] for word in
                  ['player volume 91 %', 'resampled 44.1 → 48 kHz', 'computer volume 54 %',
                   'host EQ on', '2 other sounds mixing']), s['verdict'])
        check('  the verdict is the first chip of the path', s['chips'][0][1] == 'np-verdict'
              and s['hops'] == 3 and len(s['chips']) == 4, s['chips'])
        band = s['band']
        # the band clips at its padding: every chip must sit inside, with the padding to spare
        inside = all(c[0][0] >= band[0] and c[0][0] + c[0][2] <= band[0] + band[2]
                     and c[0][1] >= band[1] and c[0][1] + c[0][3] <= band[1] + band[3] - 12
                     for c in s['chips'])
        check(f'  every chip of the path sits inside the band\'s box ({width_words})', inside,
              [band, s['chips']])
        check(f'  the band is {height} px tall' + ('' if viz_off else ', as for an untouched path'),
              band[3] == height and (not viz_off or abs(band[2] - s['wide'][2]) <= 1),
              [band, s['wide']])
        fit = b.ev(PATH)
        check('  and no chip was cut: no dots, the row whole', fit['cut'] == 0 and fit['more'] == 0
              and fit['first'] == 'np-verdict', fit)
        if not viz_off and width == 1100:
            # a path that does not fit: the card is made lower (what a longer verdict or a longer
            # name would do at the same height), and the row ends in a chip of three dots
            b.ev("document.querySelector('.p-band').style.setProperty('--band-h', '220px')")
            b.sleep(0.5)
            fit = b.ev(PATH)
            check('a path that does not fit ends in a chip of three dots, not a cut chip: the '
                  'verdict first, every chip inside the card',
                  fit['more'] == 1 and fit['cut'] >= 1 and fit['first'] == 'np-verdict'
                  and fit['last'] == 'np-more' and fit['inside'], fit)
            check('  the hops put out of sight stay in the page, and the dots name them',
                  len(fit['kept']) == fit['cut'] and fit['title'] is not None
                  and all(text in fit['title'] for text in fit['kept']), fit)
            b.ev("document.querySelector('.p-band').style.setProperty('--band-h', '204px')")
            b.sleep(0.5)
            fit = b.ev(PATH)
            check('  a lower card cuts more, the verdict still first', fit['more'] == 1
                  and fit['cut'] >= 2 and fit['first'] == 'np-verdict' and fit['inside'], fit)
            # so low that the verdict alone overflows: every hop is cut and the dots say nothing
            b.ev("document.querySelector('.p-band').style.setProperty('--band-h', '120px')")
            b.sleep(0.5)
            fit = b.ev(PATH)
            check('  a card too low even for the verdict drops the dots too: nothing is left to '
                  'name', fit['more'] == 0 and fit['cut'] == 3 and fit['first'] == 'np-verdict',
                  fit)
            b.ev("document.querySelector('.p-band').style.removeProperty('--band-h')")
            b.sleep(0.5)
            fit = b.ev(PATH)
            check('  and the row comes back whole when the card does', fit['cut'] == 0
                  and fit['more'] == 0 and fit['inside'], fit)
    finally:
        b.close()
        changed.stop()

# ------------------------------------------------------------------ a guest, and writes off
for label, web in [('a guest', 'guest = "read"'), ('a page with writes off',
                                                    'auth = "none"\nwrites = false')]:
    ro = common.Daemon(host='fake', web=web)
    b = make(ro)
    try:
        s = until(b, lambda s: s['title'] is not None)
        check(f'{label} sees the card without buttons',
              s['title'] == 'Test Pressing' and s['presses'] == [] and s['verdict'] == 'untouched',
              s)
        check(f'  and the card posts nothing', s['posts'] == [], s['posts'])
        # the key list is theirs too: the keys they have (v, A, T and ?), none that write
        b.press('?')
        keys = b.ev("""(() => { const k = document.getElementById('keys');
          return k ? { shown: !k.hidden, keys: [...k.querySelectorAll('dt')]
            .map((d) => d.textContent.trim()), hint: !!document.querySelector('.kh2'),
            vShown: getComputedStyle(k.querySelector('dt.kviz')).display !== 'none' }
            : null; })()""")
        check(f'  and ? opens a list of the keys {label} has: v, A, T and ?, none that write',
              keys is not None and keys['shown'] and keys['keys'] == ['v', 'A', 'T', '?']
              and keys['hint'],
              keys)
        check('  with the v row shown, since the screen has the card', keys['vShown'], keys)
    finally:
        b.close()
        ro.stop()

# ------------------------------------------------------------------ an amp page
amp = common.Daemon('fake-amp', host='fake', name='Fake amp')
b = make(amp)
try:
    s = until(b, lambda s: s['title'] is not None)
    check('an amp page has the same band above its columns',
          s['title'] == 'Test Pressing' and s['band'] and s['col']
          and s['band'][1] + s['band'][3] <= s['col'][1] + 0.5, s)
    check('  with the transport but no path (nothing it plays comes over the computer)',
          s['presses'] == ['previous', 'pause', 'next'] and s['path'] == '', s)
    check('no page error on the amp page', errors(b) == [], errors(b))
finally:
    b.close()
    amp.stop()

# ------------------------------------------------------------------ the host is off
off = common.Daemon()
b = make(off)
try:
    s = state(b)
    check('with no host the page has no card at all', s['band'] is None and s['title'] is None, s)
    b.press('?')
    shown = b.ev("""(() => { const k = document.getElementById('keys');
      return { open: !k.hidden, v: [...k.querySelectorAll('dt.kviz, dd.kviz')]
        .map((e) => getComputedStyle(e).display) }; })()""")
    check('the key list of a page with no card leaves out the v row',
          shown['open'] and shown['v'] == ['none', 'none'], shown)
finally:
    b.close()
    off.stop()
print(f'playing.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
