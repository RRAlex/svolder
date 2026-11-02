# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The smoke's notes, in Chromium or Firefox, numbers only (readPixels and float
readbacks, never a picture of Smoke). Usage: python3 notes.py chromium|firefox
Optional second argument `look`: only the look section (a calm volume step each, several notes);
`settle`: only the page's wiring of the settle rule.
Node proves the rules (node/director.mjs: every source's odds, the gate, the cap, away and lost,
idle, the amps' meters, the phone width; node/notes.mjs: the pointer's thresholds); this check
keeps what needs a GPU or a real page. Notes come as a stir settles; every settle rolls the odds of
its stir (a check-only hook, `effects.roll(v)`, makes every roll come out as `v`). Checks: the
atlas texture's three cells hold red and green, upright, at the glyph height the model says; one
end-to-end note (a real short flick forms none until its stir settles, then one at its place, a
second right after none); the stamp's look in the dye the draw reads (the glyph toward the tint,
the halo thinner, the right cell read); a large volume step forms one at the dot; a control's
click, a big rise and a one-step change reach the director with their odds, the header's
controls never count, a click puffs, a stir pending when the level changes is forgotten; a music
peak on an amp forms one at a meter; a failed atlas leaves the smoke running without notes; stop
frees the atlas; Glow and Off keep no note; no page error. The coverage map is COVERAGE.md."""
import json
import math
import os
import sys
import time
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
ONLY = sys.argv[2] if len(sys.argv) > 2 else None
TRIES = 15  # the most real tries per source (a miss of all is under 1 % at odds 1/3)
MIN_SETTLES = 3  # the settles a source must show before its first note ends its tries
SETTLE_WAIT = 6.0  # the longest wait for a try's settle to be rolled (s)
NOTE_COOLDOWN_SLACK = 0.2  # a little over the time between two notes, on the page's own cooldown (s)
NOTE_COOLDOWN = None  # TUNING.noteCooldown + the slack; read from the page by `load`
DENSE_MIN = 0.005  # the least mean dye in a note's cell when it forms (amount)
GAP_KEEP = 0.5  # the most of its drawn opacity the smoke keeps inside a note's glyph (share)
RING_KEEP = 0.95  # the least of its drawn opacity the smoke ringing the glyph keeps (share)
CARVE_LO, CARVE_HI = -1.2, -0.35  # the glyph's fitted opacity change per unit of mask
HALO_LO, HALO_HI = -0.6, -0.02  # the halo's fitted opacity change per unit of halo weight
LOOK_NOTES = 3  # the calm notes of the look section (each is a stamp read on the GPU)
fails = []
passed = 0
shown = 0


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(**kw):
    if browser == 'chromium':
        return common.Chromium(**kw)
    return common.Firefox(**kw)


def pg(b, expr):
    """An expression's value in the page's own realm. Firefox's WebDriver scripts run in a sandbox
    with module maps of their own, so there the expression goes through WebDriver BiDi
    (script.evaluate in the page's realm, the value passed as JSON)."""
    if browser == 'chromium':
        return b.ev(expr)
    import asyncio
    import urllib.request as rq
    import websockets
    ctx = json.load(rq.urlopen(f'http://127.0.0.1:{b.gport}/session/{b.sid}/window'))['value']
    wrapped = ('Promise.resolve(' + expr + ').then((v) => JSON.stringify(v === undefined ? null : '
                                           'v))')

    async def go():
        async with websockets.connect(b.bidi_url, max_size=None) as ws:
            await ws.send(json.dumps({'id': 1, 'method': 'script.evaluate', 'params': {
                'expression': wrapped, 'target': {'context': ctx}, 'awaitPromise': True,
                'resultOwnership': 'none'}}))
            reply = json.loads(await ws.recv())
            if reply.get('type') == 'error' or reply['result'].get('type') != 'success':
                raise RuntimeError(json.dumps(reply)[:400])
            return json.loads(reply['result']['result']['value'])
    return asyncio.run(go())


def do(b, js):
    """Run statements in the page's realm."""
    return pg(b, f'(() => {{ {js} }})()')


def val(b, expr, default=None):
    """An expression's value, or `default` when it throws (a missing feature is a FAIL, not a
    crash)."""
    try:
        return pg(b, expr)
    except Exception as e:  # noqa: BLE001
        global shown
        if shown < 3:
            shown += 1
            print('      (error:', str(e)[:300].replace('\n', ' '), ')')
        return default


def errors(b):
    """Page errors: Chromium's CDP events; Firefox's BiDi preload collector, read in the page's
    realm (the collector lives there)."""
    if browser == 'chromium':
        return b.errors
    try:
        return b.errors + (pg(b, 'window.__errs || []') or [])
    except Exception as e:  # noqa: BLE001
        return b.errors + [f'collector unreadable: {e}']


PAGE = """
window.__fx = null;
import(new URL('assets/themes/ember/effects.js', document.baseURI).href).then((m) => { window.__fx = m; });
import(new URL('assets/themes/ember/smoke-model.js', document.baseURI).href).then((m) => { window.__M = m; });
window.__st = () => (window.__fx && window.__fx.stats ? window.__fx.stats() : null);
// The stamp's own effect, from the dye grids: every frame the page keeps the dye as it left the
// frame before (a frame callback registered after the smoke's own), and the frame in which a note
// is stamped gives the pair (before, after) of the stamping step. Numbers only.
window.__tr = { prev: null, n: 0, hit: null, on: false };
window.__trStart = (H) => {
  H = H || window.__fx;
  const tr = window.__tr; tr.on = true; tr.hit = null; tr.prev = null;
  tr.n = H.stats().notes.stamped;
  const tick = () => {
    if (!tr.on) return;
    const rb = H.readback(), st = H.stats();
    if (rb && st) {
      if (st.notes.stamped > tr.n && tr.prev) {
        tr.hit = { prev: tr.prev, cur: rb.dye, note: st.notes.last };
        tr.n = st.notes.stamped; tr.on = false; return;
      }
      tr.prev = rb.dye;
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
};
// least squares: the coefficients of `ys` on the rows of `X` (Gauss elimination, normal equations)
window.__ols = (X, ys) => {
  const p = X[0].length, A = Array.from({ length: p }, () => new Array(p + 1).fill(0));
  for (let k = 0; k < X.length; k++) for (let i = 0; i < p; i++) {
    A[i][p] += X[k][i] * ys[k]; for (let j = 0; j < p; j++) A[i][j] += X[k][i] * X[k][j]; }
  for (let i = 0; i < p; i++) {
    let m = i; for (let r = i + 1; r < p; r++) if (Math.abs(A[r][i]) > Math.abs(A[m][i])) m = r;
    [A[i], A[m]] = [A[m], A[i]];
    for (let r = 0; r < p; r++) if (r !== i) { const f = A[r][i] / A[i][i];
      for (let c = i; c <= p; c++) A[r][c] -= f * A[i][c]; }
  }
  return A.map((row, i) => row[p] / row[i]);
};
// What the stamp did, by the note's masks (the atlas read back from the GPU), fitted against the
// smoke's own smooth change over the note's square (a quadratic in the position): the opacity
// ratio (drawn, 1 - exp(-gain * amount), after over before) on the halo's thinning weight
// (kHalo) and on the glyph's own mask (kCarve), the colour's share of the way from the dye's
// colour to the tint on the mask (kMask), and the same share on all three cells' masks together
// (joint).
window.__fit = (atlas, gain) => {
  const hit = window.__tr.hit; if (!hit) return null;
  const { prev, cur, note } = hit, cell = atlas.w / 3, t = note.tilt * Math.PI / 180;
  const cs = Math.cos(t), sn = Math.sin(t), cv = document.querySelector('canvas.smoke');
  const Lt = (note.tint[0] + note.tint[1] + note.tint[2]) / 3, R = [];
  let dye = 0, dn = 0;
  for (let gy = 0; gy < cur.h; gy++) for (let gx = 0; gx < cur.w; gx++) {
    const x = (gx + 0.5) / cur.w * cv.clientWidth, y = (1 - (gy + 0.5) / cur.h) * cv.clientHeight;
    const dx = x - note.x, dy = -(y - note.y);
    const qx = (cs * dx + sn * dy) / note.cell + 0.5, qy = (-sn * dx + cs * dy) / note.cell + 0.5;
    if (qx < 0 || qx >= 1 || qy < 0 || qy >= 1) continue;
    const i = (gy * cur.w + gx) * 4, a0 = prev.data[i + 3], a1 = cur.data[i + 3];
    if (!(a0 > 1e-3)) continue;
    const at = (g, ch) => atlas.data[
      (Math.floor(qy * cell) * atlas.w + Math.floor((g + qx) * cell)) * 4 + ch] / 255;
    const m = [at(0, 0), at(1, 0), at(2, 0)], own = m[note.glyph], H = at(note.glyph,
      1) * (1 - own) ** 2;
    const l0 = (prev.data[i] + prev.data[i + 1] + prev.data[i + 2]) / 3 / a0;
    const l1 = a1 > 1e-4 ? (cur.data[i] + cur.data[i + 1] + cur.data[i + 2]) / 3 / a1 : l0;
    const u = dx / note.cell, v = dy / note.cell;
    R.push({ b: [1, u, v, u * u, u * v, v * v], H, m, own,
      op: (1 - Math.exp(-gain * a1)) / (1 - Math.exp(-gain * a0)),
      s: Math.abs(Lt - l0) > 0.08 ? (l1 - l0) / (Lt - l0) : null });
    if (own > 0.5) { dye += a0; dn++; }
  }
  const S = R.filter((r) => r.s !== null), last = (c, n) => c.slice(c.length - n);
  const f1 = (rows, ys, n) => last(window.__ols(rows, ys), n);
  return { note, px: R.length, glyphPx: dn,
    drawnBefore: dn ? 1 - Math.exp(-gain * dye / dn) : null,
    kHalo: f1(R.map((r) => [...r.b, r.H, r.own]), R.map((r) => r.op), 2)[0],
    kCarve: f1(R.map((r) => [...r.b, r.H, r.own]), R.map((r) => r.op), 2)[1],
    kMask: f1(S.map((r) => [...r.b, r.m[note.glyph]]), S.map((r) => r.s), 1)[0],
    joint: f1(S.map((r) => [...r.b, r.m[0], r.m[1], r.m[2]]), S.map((r) => r.s), 3) };
};
"""


def roll_to(x):
    """A forced roll, kept inside the roll's range and to two places."""
    return round(min(max(x, 0.0), 0.99), 2)


def tuning(b, key):
    """One number of the page's own `TUNING`: the approved look is the page's, not this file's."""
    v = val(b, f'window.__M.TUNING.{key}', None)
    if v is None:
        raise RuntimeError(f'TUNING.{key} is not readable from the page')
    return v


def load(b, url, wait=3.0, roll=0):
    """Load a page and wait for its effects module; every settle's roll comes out as `roll` (0:
    always a note, `None`: random draws)."""
    b.goto(url, settle=0.8)
    do(b, PAGE)
    for _ in range(50):
        if val(b, 'window.__fx !== null && window.__M !== undefined', False):
            break
        b.sleep(0.1)
    global NOTE_COOLDOWN
    NOTE_COOLDOWN = tuning(b, 'noteCooldown') + NOTE_COOLDOWN_SLACK
    if roll is not None:
        do(b, f'window.__fx.roll({roll})')
    b.sleep(wait)


def notes(b):
    """The smoke's note counters: stamped, live, last (or {} before the first)."""
    return (val(b, 'window.__st()', None) or {}).get('notes') or {}


def stamped(b):
    return notes(b).get('stamped', -1)


def wait_stamped(b, n, secs):
    """Wait up to `secs` for `n` notes in all; the counters then."""
    end = time.time() + secs
    while stamped(b) < n and time.time() < end:
        b.sleep(0.1)
    return notes(b)


def sweep(b, x0, y0, x1, y1, n, ms, short=900):
    """One unpressed straight pointer sweep in `n` moves `ms` apart (adds no dye); the time it took
    and its speed in short sides (`short` px, the viewport's) a second, measured on the wall
    clock."""
    t0 = time.time()
    stroke(b, line(x0, y0, x1, y1, n), False, ms=ms)
    secs = time.time() - t0
    return secs, math.hypot(x1 - x0, y1 - y0) / secs / short


def flick(b, cy=420, x0=40, x1=1240, n=6, ms=20):
    """A real short flick: 1200 px in about 0.2 s (5 short sides a second or more)."""
    return sweep(b, x0, cy, x1, cy, n, ms)


def stroke(b, pts, pressed, ms=16):
    """A pointer stroke through `pts` (viewport px): real pointer events, a button held when
    `pressed`."""
    if browser == 'chromium':
        b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': pts[0][0], 'y': pts[0][1]})
        if pressed:
            b.send('Input.dispatchMouseEvent', {
                'type': 'mousePressed', 'x': pts[0][0], 'y': pts[0][1],
                'button': 'left', 'buttons': 1, 'clickCount': 1})
        for x, y in pts[1:]:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': x, 'y': y,
                                                'buttons': 1 if pressed else 0,
                                                **({'button': 'left'} if pressed else {})})
            b.sleep(ms / 1000)
        if pressed:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseReleased', 'x': pts[-1][0],
                                                'y': pts[-1][1], 'button': 'left', 'buttons': 0,
                                                'clickCount': 1})
        return
    acts = [{'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(pts[0][0]),
             'y': int(pts[0][1])}]
    if pressed:
        acts.append({'type': 'pointerDown', 'button': 0})
    for x, y in pts[1:]:
        acts.append({'type': 'pointerMove', 'duration': ms, 'origin': 'viewport', 'x': int(x),
                     'y': int(y)})
    if pressed:
        acts.append({'type': 'pointerUp', 'button': 0})
    b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                      'parameters': {'pointerType': 'mouse'}, 'actions': acts}]})


def line(x0, y0, x1, y1, n):
    return [(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n) for i in range(n + 1)]


def dial(b):
    return b.ev("(() => { const k = document.querySelector('sv-knob').geometry();"
                " return [k.cx, k.cy, k.radius]; })()")


def knob(b):
    return b.ev("(() => { const k = document.querySelector('sv-knob');"
                " return { raw: k.raw, lo: k.lo, hi: k.hi }; })()")


def dot_of(b):
    """Where the knob's dot is now, from its own angle (as the page computes it)."""
    return b.ev("(() => { const k = document.querySelector('sv-knob'), g = k.geometry();"
                " const a = k.angle(k.raw) * Math.PI / 180;"
                " return [g.cx + g.radius * Math.sin(a), g.cy - g.radius * Math.cos(a)]; })()")


OWN = """const c = document.createElement('canvas');
  for (const [k, v] of [['position', 'fixed'], ['left', '0'], ['top', '0'], ['width', '100%'],
    ['height', '100lvh'], ['pointer-events', 'none']]) c.style.setProperty(k, v);
  c.className = 'smoke'; document.body.append(c);"""
START = """import(new URL('assets/themes/ember/smoke.js', document.baseURI).href).then((m) => {
    window.__h = m.start(c, { glow: ['#8b5cf6', '#6d3fd1', '#b9a0ff'], bg: '#08070c' });
    window.__h.level(0.8); window.__h.anchor({ x: 640, y: 450, radius: 100, meters: [] }); });"""


def med(xs):
    xs = sorted(xs)
    return xs[len(xs) // 2]


def look_section(runs=None):
    """The stamp's look from calm notes: on an amp, the two meters held at 0.9 build a smoke, then
    a music peak (0.99 on one meter) stamps a note; no pointer, no volume puff, so the flow is the
    meters' gentle lift. Each note is read from the stamping step's own before and after, the fit
    removing the smoke's smooth change over the square. Returns the per-note results."""
    amp = common.Daemon('fake-amp', name='Amp')
    b = make(size=(1280, 900))
    rows = []
    try:
        load(b, amp.url, wait=2.0)
        base = float(os.environ.get('LOOK_BASE', '0.9'))
        do(
            b,
            f"window.__L = [{base}, {base}];"
            " setInterval(() => window.__fx.meters(window.__L), 50);",
        )
        for rep in range(runs or LOOK_NOTES):
            b.sleep(4.5 if rep == 0 else 4.3)
            do(b, 'window.__trStart()')
            do(b, 'window.__L = %s' % (f'[0.99, {base}]' if rep % 2 == 0 else f'[{base}, 0.99]'))
            b.sleep(0.4)
            do(b, f'window.__L = [{base}, {base}]')
            for _ in range(500):
                if val(b, 'window.__tr.hit !== null', False):
                    break
                b.sleep(0.01)
            do(b, f'window.__L = [{base}, {base}]')
            th = val(b, 'window.__fit(window.__fx.readback().atlas, 25)', None)
            if th:
                rows.append(th)
                n = th['note']
                print('      note %d at (%d, %d), glyph %d, glyph %d px high, tint %s: drawn %.2f '
                      'of its '
                      'ceiling before; halo coefficient %+.3f, colour toward the tint %+.3f, joint '
                      'masks %s'
                      % (rep + 1, n['x'], n['y'], n['glyph'], n['size'],
                         [round(t, 2) for t in n['tint']], th['drawnBefore'] or 0, th['kHalo'],
                         th['kMask'], [round(j, 2) for j in th['joint']]))
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()
        amp.stop()
    return rows


def look_checks(rows, want):
    n = len(rows)
    check(f'{want} calm notes were stamped and read', n == want, n)
    if not rows:
        return
    kh = med([t['kHalo'] for t in rows])
    kc = med([t['kCarve'] for t in rows])
    km = med([t['kMask'] for t in rows])
    wrong = med([t['joint'][t['note']['glyph']] - max(j for i, j in enumerate(t['joint'])
                                                      if i != t['note']['glyph']) for t in rows])
    print('      medians: halo coefficient %+.3f, glyph carve %+.3f, colour toward the tint %+.3f, '
          'stamped glyph\'s joint coefficient over the best other %+.3f' % (kh, kc, km, wrong))
    check('  the glyph is carved out (the median fitted change of the drawn opacity per unit of '
          f'the glyph\'s mask is between {CARVE_LO:g} and {CARVE_HI:g}: the settings remove 85 % '
          'of the dye)', CARVE_LO <= kc <= CARVE_HI, [t['kCarve'] for t in rows])
    check('  and the halo thins the smoke round it a little (the median fitted change per unit of '
          f'halo weight is between {HALO_LO:g} and {HALO_HI:g})', HALO_LO <= kh <= HALO_HI,
          [t['kHalo'] for t in rows])
    check('  the glyph moves toward the lighter tint (the median fitted share of the way from the '
          'dye\'s colour to the tint per unit of mask is at least 0.3; the settings give 0.7)',
          km >= 0.3, [t['kMask'] for t in rows])
    check('  the right cell was read (the median joint coefficient on the stamped glyph\'s mask '
          'is above the best of the other two by at least 0.1)', wrong >= 0.1,
          [t['joint'] for t in rows])


def counters(b, handle='window.__fx'):
    """The settles rolled and won so far, and the notes formed, of a handle."""
    n = val(b, f'{handle}.stats().notes', None) or {}
    return n.get('rolls', -1), n.get('wins', -1), n.get('stamped', -1)


def real_tries(b, act, max_tries=TRIES):
    """Real tries of one source: `act()` is one stroke or click; after it the page's own counters
    say when its settle was rolled (at most `SETTLE_WAIT` s). The tries stop at the first note
    once `MIN_SETTLES` settles are seen. Returns (notes formed, settles rolled, tries made)."""
    won = settled = made = 0
    while made < max_tries:
        before = counters(b)
        act()
        made += 1
        waited = 0.0
        while waited < SETTLE_WAIT and counters(b)[0] == before[0]:
            b.sleep(0.1)
            waited += 0.1
        after = counters(b)
        settled += after[0] - before[0]
        won += after[2] - before[2]
        if won >= 1 and settled >= MIN_SETTLES:
            break
    return won, settled, made


def tries(b, act, forced, secs, dac=None, reset=None, handle='window.__fx'):
    """One try per roll in `forced`: set that roll, run `act`, wait `secs`; the settles rolled and
    won by it. `reset` (a callable) puts the page back first and gets the time to settle."""
    out = []
    for roll in forced:
        if reset:
            reset()
            b.sleep(secs)
        do(b, f'window.__fx.roll({roll})')
        before = counters(b, handle)
        act()
        b.sleep(secs)
        after = counters(b, handle)
        out.append((roll, after[0] - before[0], after[1] - before[1]))
    return out


def puffs(b):
    """The puffs the smoke has applied so far (volume steps and clicks)."""
    return (val(b, 'window.__st().splats', None) or {}).get('puff', -1)


def set_level(b, level):
    """Choose an effects level through the page's own menu."""
    b.ev("(() => { document.querySelector('#fx-button').click(); "
         f"document.querySelector('#fx-menu [data-level={level}]').click(); }})()")
    b.sleep(0.8)


def settle_section(dac):
    """The page's wiring of the settle rule on its own handle. Node proves every source's odds (a
    roll that wins tells them apart); here a click on a control, a big rise and a one-step
    change reach the director with the odds the page gives them, the header's controls never count,
    a click puffs, and a stir pending when the level changes is forgotten."""
    b = make(size=(1280, 900))
    try:
        dac.write('Gungnir 2', 'volume', -50)
        load(b, dac.url, wait=2.0)
        mute = common.center(b, '.btn.mute')
        click_mute = lambda: b.click(*mute)  # noqa: E731
        # The rolls sit the same distance either side of each odds as the literals did at the
        # approved look (a settle wins a roll under its odds), whatever TUNING holds.
        click_odds = tuning(b, 'clickOdds')
        win_click, lose_click = roll_to(click_odds - 0.10), roll_to(click_odds + 0.05)
        # ---- the odds of a click on a control: it wins a roll under clickOdds, not one over
        got = tries(b, click_mute, (win_click, lose_click), 2.4)
        print(f'      a click on Mute, forced rolls: {got}')
        check(f'a click on the Mute button settles once and wins a roll of {win_click}, not '
              f'{lose_click}: odds {click_odds:.3g}',
              got == [(win_click, 1, 1), (lose_click, 1, 0)], got)

        # ---- the odds of a volume change: a rise of 20 dB is a big one, a step is not
        def raise_to(db):
            return lambda: dac.write('Gungnir 2', 'volume', db)
        dac.write('Gungnir 2', 'volume', -50)
        b.sleep(2.4)
        big_db = 2 * tuning(b, 'bigRiseDb')  # twice what the page calls big
        big_odds, step_odds = tuning(b, 'bigRiseOdds'), tuning(b, 'settleOdds')
        win_big, lose_step = roll_to(big_odds - 7 / 60), roll_to(step_odds + 1 / 15)
        got = tries(b, raise_to(-50 + big_db), (win_big,), 2.4)
        print(f'      a {big_db:g} dB rise, forced roll: {got}')
        check(f'a rise of {big_db:g} dB settles once and wins a roll of {win_big}: odds '
              f'{big_odds:.3g} (the page says big)', got == [(win_big, 1, 1)], got)
        got = tries(b, raise_to(-49.5 + big_db), (lose_step,), 2.4)
        print(f'      one step up, forced roll: {got}')
        check(f'a one-step change settles once and loses a roll of {lose_step}: odds '
              f'{step_odds:.3g}', got == [(lose_step, 1, 0)], got)
        # ---- clicks that never count: the header (the device tab, the wordmark, the menu button)
        do(b, 'window.__fx.roll(0)')
        before = counters(b)
        p_before = puffs(b)
        done = []
        for sel in ('.devs .dev', '.top .wordmark', '#fx-button'):
            if b.ev(f'document.querySelector({json.dumps(sel)}) !== null'):
                b.click(*common.center(b, sel))
                done.append(sel)
                if sel == '#fx-button':
                    b.press('Escape')
        b.sleep(2.4)
        after = counters(b)
        p_header = puffs(b)
        print(f'      header clicks {done}: {before} -> {after}, puffs {p_before} -> {p_header}')
        check('a click on the header (the device tab, the wordmark, the effects menu button) '
              'rolls no settle and forms no note', len(done) >= 2 and after == before,
              [done, before, after])
        check('  and puffs nothing', p_header == p_before, [p_before, p_header])
        click_mute()
        b.sleep(0.6)
        p_click = puffs(b)
        check('a click on the Mute button puffs the smoke (the small puff of a click)',
              p_click > p_header, [p_header, p_click])
        # ---- a stir pending when the level changes is forgotten
        for level in ('glow', 'off'):
            flick(b)
            set_level(b, level)
            b.sleep(1.4)
            gone = val(b, 'window.__st()', 'missing')
            set_level(b, 'smoke')
            b.sleep(1.4)
            after = val(b, 'window.__st()', None)
            nn = (after or {}).get('notes') or {}
            check(f'a flick, then {level.capitalize()} before it settles: no smoke meanwhile and '
                  'no note after the smoke is back', gone is None and nn.get('stamped') == 0
                  and nn.get('rolls') == 0, [gone, nn])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()


CONTRAST = """(() => { const hit = window.__tr.hit; if (!hit) return null;
 const { prev, cur, note: n } = hit, at = window.__fx.readback().atlas, cell = at.w / 3;
 const t = n.tilt * Math.PI / 180, cs = Math.cos(t), sn = Math.sin(t);
 const cv = document.querySelector('canvas.smoke');
 const drawn = (A) => 1 - Math.exp(-25 * A);
 // the drawn opacity (share of the ceiling) before and after the stamping step: over the gap (the
 // glyph's own shape) and over the ring (the strip round the note's cell and what
 // is left of the cell's square beyond its halo); `dye` is the mean dye amount in the cell's box
 const sum = { gap: [0, 0], ring: [0, 0] };
 let dye = 0, dn = 0;
 for (let gy = 0; gy < cur.h; gy++) for (let gx = 0; gx < cur.w; gx++) {
  const x = (gx + 0.5) / cur.w * cv.clientWidth, y = (1 - (gy + 0.5) / cur.h) * cv.clientHeight;
  const dx = x - n.x, dy = -(y - n.y), k = (gy * cur.w + gx) * 4;
  const a0 = drawn(prev.data[k + 3]), a1 = drawn(cur.data[k + 3]);
  if (Math.abs(dx) < n.cell / 2 && Math.abs(dy) < n.cell / 2) { dye += prev.data[k + 3]; dn++; }
  const qx = (cs * dx + sn * dy) / n.cell + 0.5, qy = (-sn * dx + cs * dy) / n.cell + 0.5;
  let kind = null;
  if (qx < 0 || qx >= 1 || qy < 0 || qy >= 1) {
   if (Math.hypot(dx, dy) < n.cell * 1.2) kind = 'ring';
  }
  else {
   const i = (Math.floor(qy * cell) * at.w + Math.floor((n.glyph + qx) * cell)) * 4;
   const mr = at.data[i] / 255, mg = at.data[i + 1] / 255;
   if (mr > 0.5) kind = 'gap'; else if (mr < 0.02 && mg < 0.02) kind = 'ring';
  }
  if (kind) { sum[kind][0] += a0; sum[kind][1] += a1; }
 }
 const top = document.elementFromPoint(n.x, n.y);
 return { gap: sum.gap[1] / Math.max(sum.gap[0], 1e-9),
          ring: sum.ring[1] / Math.max(sum.ring[0], 1e-9), before: sum.gap[0],
          dye: dye / Math.max(dn, 1), cell: n.cell, x: n.x, y: n.y,
          covered: !!(top && (top.closest(window.__fx.CLICKABLE) || top.closest('.eqg'))) }; })()"""


def visible_section(dac):
    """Notes a person sees, from real input (WebDriver actions in Firefox, CDP input in Chromium):
    ordinary mouse moves and clicks on Mute each settle on every try and form a note within
    fifteen seeded tries (each source stops at its first note once three settles are seen; the
    rates are the node tests's), a click on the effects menu's button (the demo DAC has no device
    tab) rolls none, and a stamped note, off the control it came from, in smoke, is a gap carved
    out of it: the stamping step empties the glyph's shape and leaves the smoke ringing it."""
    b = make(size=(1280, 900))
    try:
        dac.write('Gungnir 2', 'volume', -50)
        load(b, dac.url, wait=2.0, roll=None)
        do(b, 'window.__fx.seed(11)')
        set_level(b, 'glow')
        set_level(b, 'smoke')
        b.sleep(3.0)
        # ---- ordinary moves: 700 px up the open left margin in 5 ticks of 28 ms,
        # (each tick also costs the input round trip) so the measured speed stays over the ruled 2
        # (arming starts at about 1.2); a note needs an open spot, and the page's bottom edge and
        # the cards on the right rarely leave one
        speeds = []

        def move():
            t0 = time.time()
            stroke(b, line(90, 850, 90, 150, 5), False, ms=28)
            speeds.append(700 / (time.time() - t0) / 900)

        won, settled, made = real_tries(b, move)
        print(f'      ordinary moves: {won} notes, {settled} settles in {made} tries, speeds '
              f'{min(speeds):.2f}-{max(speeds):.2f} short sides a second')
        check('real ordinary moves (2 short sides a second or more, measured on the wall clock) '
              f'settle every try and form a note within {TRIES} tries (the rate is the node '
              'tests\'s)', min(speeds) >= 2.0 and settled == made and won >= 1,
              [won, settled, made, speeds])
        # ---- real clicks on Mute
        b.sleep(NOTE_COOLDOWN)
        mute = common.center(b, '.btn.mute')
        won, settled, made = real_tries(b, lambda: b.click(*mute))
        print(f'      clicks on Mute: {won} notes, {settled} settles in {made} tries')
        check(f'real clicks on Mute settle every try and form a note within {TRIES} tries (the '
              'rate is the node tests\'s)', settled == made and won >= 1, [won, settled, made])
        # ---- the effects menu's button is never a control
        before = counters(b)
        for _ in range(6):
            b.click(*common.center(b, '#fx-button'))
            b.press('Escape')
            b.sleep(1.5)
        b.sleep(3.0)
        check('six real clicks on the effects menu\'s button roll no settle', counters(b) == before,
              [before, counters(b)])
        # ---- a note is a gap carved out of the smoke, off the control it came from: the drawn
        # opacity over its glyph, from the dye just before the stamping step to just after
        # the volume is raised first so the dial's cloud reaches over Mute (0.05 of dye there, a
        # place dense enough to carve in, under a control)
        dac.write('Gungnir 2', 'volume', -8)
        b.sleep(9.0)
        do(b, 'window.__fx.roll(0)')
        b.sleep(4.5)
        rows = []
        for _ in range(3):
            do(b, 'window.__trStart()')
            b.click(*mute)
            for _ in range(60):
                if val(b, 'window.__tr.hit !== null', False):
                    break
                b.sleep(0.1)
            got = val(b, CONTRAST, None)
            if got:
                rows.append(got)
            b.sleep(4.6)
        print(f'      carved gap, drawn opacity after over before: gap '
              f'{[round(r["gap"], 2) for r in rows]}, ring {[round(r["ring"], 2) for r in rows]}; '
              f'dye in the cell {[round(r["dye"], 3) for r in rows]}; '
              f'covered {[r["covered"] for r in rows]}')
        check('three clicks stamped three notes read', len(rows) == 3, len(rows))
        check('  none is under a control (the page shows the smoke at its centre)',
              all(not r['covered'] for r in rows), rows)
        check(f'  each forms in smoke (the mean dye in its cell at least {DENSE_MIN:g})',
              all(r['dye'] >= DENSE_MIN for r in rows), [r['dye'] for r in rows])
        check(f'  and each is carved out of it: the stamping step leaves the glyph '
              f'{GAP_KEEP:.0%} of its drawn opacity or less, and leaves the smoke ringing it at '
              f'{RING_KEEP:.0%} or more', all(r['gap'] <= GAP_KEEP for r in rows)
              and all(r['ring'] >= RING_KEEP for r in rows), rows)
        # ---- a stir in thin smoke forms its note in dense smoke: four passes up and down the
        # thin lower left (0.001 of dye) settle into a note, whose cell holds dye all the same
        do(b, 'window.__trStart()')
        pts = []
        for _ in range(4):
            pts += line(300, 880, 300, 700, 3) + line(300, 700, 300, 880, 3)
        stroke(b, pts, False, ms=15)
        for _ in range(80):
            if val(b, 'window.__tr.hit !== null', False):
                break
            b.sleep(0.1)
        thin = val(b, CONTRAST, None)
        at = thin and (round(thin['x']), round(thin['y']))
        print(f'      a stir in thin smoke: note at {at}, dye in its cell '
              f'{thin and round(thin["dye"], 3)}, covered {thin and thin["covered"]}')
        check('a stir in thin smoke (the lower left) forms a note, in smoke (the mean dye in its '
              f'cell at least {DENSE_MIN:g}), not under a control',
              bool(thin) and thin['dye'] >= DENSE_MIN and not thin['covered'], thin)
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()


PHONE_TRIES = 15  # the most real tries per source at phone width


def phone_section(dac):
    """At 390x844 (no margins, the cards fill the width) ordinary moves and clicks on Mute still
    form notes: each source settles (all but two of its tries) and forms at least one note within
    `PHONE_TRIES` seeded tries, stopping at its first note once three settles are seen. It does
    not check where the notes sit."""
    b = make(size=(390, 844))
    try:
        dac.write('Gungnir 2', 'volume', -50)
        load(b, dac.url, wait=2.0, roll=None)
        do(b, 'window.__fx.seed(21)')
        set_level(b, 'glow')
        set_level(b, 'smoke')
        b.sleep(3.0)
        won, settled, made = real_tries(
            b, lambda: stroke(b, line(40, 700, 350, 700, 6), False, ms=50), PHONE_TRIES)
        print(f'      phone, ordinary moves: {won} notes, {settled} settles in {made} tries')
        check(f'at 390x844 ordinary moves settle and form a note within {PHONE_TRIES} tries (the '
              'rate is the node tests\'s)', settled >= made - 2 and won >= 1, [won, settled, made])
        b.sleep(NOTE_COOLDOWN)
        mute = common.center(b, '.btn.mute')
        won, settled, made = real_tries(b, lambda: b.click(*mute), PHONE_TRIES)
        print(f'      phone, clicks on Mute: {won} notes, {settled} settles in {made} tries')
        check(f'at 390x844 clicks on Mute settle and form a note within {PHONE_TRIES} tries',
              settled >= made - 2 and won >= 1, [won, settled, made])
        check('  no page error', errors(b) == [], errors(b))
    finally:
        b.close()


def run():
    dac = common.Daemon('gungnir2-v2')
    if ONLY == 'visible':
        try:
            visible_section(dac)
        finally:
            dac.stop()
        return
    if ONLY == 'phone':
        try:
            phone_section(dac)
        finally:
            dac.stop()
        return
    if ONLY == 'settle':
        try:
            settle_section(dac)
        finally:
            dac.stop()
        return
    if ONLY == 'look':
        try:
            rows = look_section(int(os.environ.get('LOOK_NOTES', str(LOOK_NOTES))))
            look_checks(rows, int(os.environ.get('LOOK_NOTES', str(LOOK_NOTES))))
        finally:
            dac.stop()
        return
    b = make(size=(1280, 900))
    try:
        # ---- the atlas: three cells, each with red (the glyph) and green (the halo)
        load(b, dac.url, wait=2.0)
        has = val(b, "(() => { const r = window.__fx.readback(); return r && r.atlas ? "
                     "{ w: r.atlas.w, h: r.atlas.h } : null; })()", None)
        check(
            'the atlas is read back (readback().atlas) at 384 x 128',
            has == {'w': 384, 'h': 128},
            has,
        )
        cells = val(b, """(() => { const a = window.__fx.readback().atlas, out = [];
          for (let c = 0; c < 3; c++) { let red = 0, green = 0, other = 0, lo = 999, hi = -1;
            for (let y = 0; y < a.h; y++) for (let x = c * 128; x < c * 128 + 128; x++) {
              const i = (y * a.w + x) * 4, r = a.data[i], g = a.data[i + 1];
              if (r > 200) { red++; lo = Math.min(lo, y); hi = Math.max(hi, y); }
              else if (g > 60 && r < 30) green++;
              if (a.data[i + 2] !== 0) other++; }
            out.push({ red, green, blue: other, height: (hi - lo + 1) / 128 }); }
          return out; })()""", None) or []
        print('      atlas cells (red = glyph px, green = halo-only px, height = the glyph\'s '
              'share '
              'of the cell):', cells)
        check('each of the three cells holds a glyph (red > 400 px) and a halo (green > 800 px)',
              len(cells) == 3 and all(c['red'] > 400 and c['green'] > 800 for c in cells), cells)
        check('  the glyphs differ (the pair has the most red, the crotchet the least)',
              len(cells) == 3 and cells[1]['red'] > cells[0]['red'] > cells[2]['red'], cells)
        check('  nothing in the blue channel',
              len(cells) == 3 and all(c['blue'] == 0 for c in cells), cells)
        gh = val(b, 'window.__M.TUNING.noteGlyphHeight', None)
        check('  every glyph is as high as the model says (TUNING.noteGlyphHeight, within 0.04 of '
              'the cell)', gh is not None and len(cells) == 3
              and all(abs(c['height'] - gh) <= 0.04 for c in cells), [gh, cells])
        up = val(b, """(() => { const a = window.__fx.readback().atlas; let low = 0, high = 0;
          for (let y = 0; y < 128; y++) for (let x = 256; x < 384; x++)
            if (a.data[(y * a.w + x) * 4] > 200) { if (y < 42) low++; else if (y >= 86) high++; }
          return [low, high]; })()""", None) or [0, 0]
        check('  the uploaded crotchet is upright (its head, at the cell\'s bottom, has more than '
              'twice the red of the cell\'s top third; the rows are read from the bottom)',
              up[0] > 2 * up[1], up)
        n0 = notes(b)
        check('the handle reports notes (stats().notes: stamped 0, live 0, last null) at the start',
              n0.get('stamped') == 0 and n0.get('live') == 0 and n0.get('last') is None, n0)

        # ---- one end-to-end note: a real short flick forms no note until its stir settles, then
        # one at the flick's place; a second right after none (the gate). Node proves the rest of
        # the pointer's thresholds, the gap and the cap. The flick runs right to left, ending in the
        # open left margin where the smoke is dense: a stir that ends under a card has its note
        # moved to the nearest dense, open smoke, which node proves.
        secs, speed = flick(b, x0=1240, x1=40)
        t_flick = time.time()
        b.sleep(0.15)
        n0 = notes(b)
        print(f'      the flick: 1200 px in {secs:.2f} s = {speed:.2f} short sides a second; {n0}')
        check('the flick was a real one (at most 0.3 s, at least 5 short sides a second)',
              secs <= 0.3 and speed >= 5, [secs, speed])
        check('  it forms no note at once (a flick no longer stamps until its stir settles)',
              n0.get('stamped') == 0, n0)
        n1 = wait_stamped(b, 1, 3.5)
        t_note = time.time()
        last = n1.get('last') or {}
        print(f'      settled after {t_note - t_flick:.2f} s: {n1}')
        check('  once it settles it forms one note (rolled and won)',
              n1.get('stamped') == 1 and n1.get('live') == 1 and n1.get('wins') == 1, n1)
        check('  it is inside the page, its glyph 6-9 % of the short side (900 px) high, its cell '
              'the glyph over the model\'s share, its tilt within 25 degrees',
              bool(last) and 0 <= last['x'] <= 1280 and 0 <= last['y'] <= 900
              and 54 <= last['size'] <= 81 and abs(last['cell'] - last['size'] / gh) < 0.01
              and abs(last['tilt']) <= 25 and last['glyph'] in (0, 1, 2), [last, gh])
        check('  it is where the flick was (along its line, 40-1240 x 420, within the cell)',
              bool(last) and 40 - last['cell'] <= last['x'] <= 1240 + last['cell']
              and abs(last['y'] - 420) <= last['cell'], last)
        flick(b, cy=600)
        n2 = wait_stamped(b, 2, 2.5)
        check('a second flick right after forms none: its settle wins the roll but the 4 s gap '
              'holds', n2.get('stamped') == 1 and n2.get('wins') == 2, [n2, round(
                  time.time() - t_note, 1)])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- the stamp's look: calm notes (a volume step each), the fit
        rows = look_section()
        look_checks(rows, LOOK_NOTES)

        # ---- a volume step: a page load is not one, a big one stamps one at the dot
        dac.write('Gungnir 2', 'volume', -50)
        b = make(size=(1280, 900))
        load(b, dac.url, wait=2.0)
        check('a page load is not a step: no note', stamped(b) == 0, notes(b))
        dac.write('Gungnir 2', 'volume', -8)
        b.sleep(0.08)
        check('  a large volume step forms no note at once', stamped(b) == 0, notes(b))
        vn = wait_stamped(b, 1, 4.0)
        dot = dot_of(b)
        print(f'      volume -50 -> -8 dB: {vn}; the dot at {tuple(round(v) for v in dot)}')
        last = vn.get('last') or {}
        check('  once it settles it forms one note', vn.get('stamped') == 1, vn)
        check('  at the dial\'s dot (within 120 px of where the knob\'s dot is)',
              bool(last) and math.hypot(last['x'] - dot[0], last['y'] - dot[1]) < 120, [last, dot])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        settle_section(dac)

        # ---- a failed atlas: the smoke keeps running without notes (a handle of its own)
        failures = {
            'the atlas\'s 2D context is refused (OffscreenCanvas getContext gives null)':
                "OffscreenCanvas.prototype.getContext = function () { return null; };"
                " const cv = HTMLCanvasElement.prototype.getContext;"
                " HTMLCanvasElement.prototype.getContext = function (k, ...r) {"
                " return k === '2d' ? null : cv.call(this, k, ...r); };",
            'the atlas\'s upload throws (texImage2D with RG8)':
                "const ti = WebGL2RenderingContext.prototype.texImage2D;"
                " WebGL2RenderingContext.prototype.texImage2D = function (...a) {"
                " if (a[2] === 0x822B) throw new Error('refused'); return ti.apply(this, a); };",
        }
        for name, patch in failures.items():
            b = make(size=(1280, 900))
            load(b, dac.url + '?effects=off', wait=0.5)
            do(b, patch + OWN + START)
            b.sleep(1.2)
            flick(b)
            b.sleep(0.3)
            fs = val(b, "(() => "
                        "{ const r = window.__h.readback(); return { atlas: r ? r.atlas : 'no "
                        "readback', frames: window.__h.stats().frames, notes: "
                        "window.__h.stats().notes.stamped }; })()", None)
            print(f'      {name}: {fs}')
            check(f'{name}: the smoke runs (frames counted), no note, readback().atlas is null',
                  bool(fs) and fs['atlas'] is None and fs['frames'] > 5 and fs['notes'] == 0, fs)
            check('  no page error', errors(b) == [], errors(b))
            b.close()
        # an earlier GL error does not turn the notes off: the first getError says INVALID_ENUM once
        b = make(size=(1280, 900))
        load(b, dac.url + '?effects=off', wait=0.5)
        do(b, """const ge = WebGL2RenderingContext.prototype.getError; let once = true;
          WebGL2RenderingContext.prototype.getError = function () {
            if (once) { once = false; return 0x500; } return ge.call(this); };""" + OWN + START)
        b.sleep(1.2)
        has = val(b, "(window.__h.readback() || {}).atlas ? true : false", None)
        check('an earlier GL error (the first getError says INVALID_ENUM) does not turn the notes '
              'off: the atlas is there', has is True, has)
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- stop frees the atlas with every other GPU object (a handle of its own)
        b = make(size=(1280, 900))
        load(b, dac.url + '?effects=off', wait=0.5)
        do(b, """window.__live = { tex: 0, fbo: 0, prog: 0 };
          const P = WebGL2RenderingContext.prototype, L = window.__live;
          for (const [mk, rm, key] of [['createTexture', 'deleteTexture', 'tex'],
            ['createFramebuffer', 'deleteFramebuffer', 'fbo'], ['createProgram', 'deleteProgram',
              'prog']]) {
            const a = P[mk], d = P[rm];
            P[mk] = function (...x) { L[key]++; return a.apply(this, x); };
            P[rm] = function (o, ...x) { if (o) L[key]--; return d.call(this, o, ...x); };
          }""" + OWN + START)
        b.sleep(1.2)
        up_live = pg(b, 'JSON.parse(JSON.stringify(window.__live))')
        atlas_here = pg(b, '(window.__h.readback() || {}).atlas !== null')
        pg(b, 'window.__h.stop() || 0')
        dn_live = pg(b, 'JSON.parse(JSON.stringify(window.__live))')
        print(f'      GPU objects live while running {up_live}, after stop {dn_live}')
        check('the handle has the atlas texture while running (readback().atlas is there)',
              atlas_here is True, atlas_here)
        check('stop frees every texture, framebuffer and program it made, the atlas included',
              dn_live['tex'] == 0 and dn_live['fbo'] == 0 and dn_live['prog'] == 0
              and up_live['tex'] > 0, [up_live, dn_live])
        check('  no page error', errors(b) == [], errors(b))
        b.close()

        # ---- Glow and Off: no handle, no note, no canvas
        for level in ('glow', 'off'):
            b = make(size=(1280, 900))
            load(b, dac.url + f'?effects={level}', wait=0.8)
            flick(b)
            cnt = val(b, "document.querySelectorAll('canvas.smoke').length", -1)
            st = val(b, 'window.__st()', 'missing')
            check(f'on {level.capitalize()} there is no smoke, no counters and no note after a '
                  'flick', cnt == 0 and st is None, [cnt, st])
            check('  no page error', errors(b) == [], errors(b))
            b.close()

        # ---- an amp: a music peak into red stamps one note at a meter (node proves one, two and
        # five meters and a reading held below red; tyr and fake-five give their anchors below)
        amp = common.Daemon('fake-amp', name='Amp')
        try:
            b = make(size=(1280, 900))
            load(b, amp.url, wait=2.0)
            cs = b.ev("[...document.querySelectorAll('.meters figure.meter')].map((f) => {"
                      " const r = f.getBoundingClientRect();"
                      " return [r.left + r.width / 2, r.top + r.height / 2]; })")
            do(b, f"""const L = {json.dumps([0.99] * len(cs))}, Z = L.map(() => 0);
              window.__t = setInterval(() => window.__fx.meters(L), 50);
              setTimeout(() => {{ clearInterval(window.__t); window.__fx.meters(Z); }}, 200);""")
            b.sleep(1.0)
            mn = wait_stamped(b, 1, 3.5)
            last = mn.get('last') or {}
            print(f'      fake-amp ({len(cs)} meters): a peak to 0.99 for 0.2 s: {mn}')
            check('fake-amp has 2 meter(s) on the page', len(cs) == 2, len(cs))
            check('  a peak to 0.99 for 0.2 s forms one note once it settles',
                  mn.get('stamped') == 1, mn)
            check('  at one of the meters (within 120 px of its centre)',
                  bool(last) and any(math.hypot(last['x'] - x, last['y'] - y) < 120
                                     for x, y in cs), [last, cs])
            check('  no page error', errors(b) == [], errors(b))
            b.close()
        finally:
            amp.stop()
        # the anchor of a mono amp and a five-meter amp: the page gives the smoke one place per
        # meter, at the meter's own centre (node proves what a peak there forms)
        for profile, name, peak_n in (('tyr', 'Tyr', 1), ('fake-five', 'Five', 5)):
            amp = common.Daemon(profile, name=name)
            try:
                b = make(size=(1280, 900))
                load(b, amp.url, wait=1.5)
                cs = b.ev("[...document.querySelectorAll('.meters figure.meter')].map((f) => {"
                          " const r = f.getBoundingClientRect();"
                          " return [r.left + r.width / 2, r.top + r.height / 2]; })")
                at = val(b, '(window.__fx.anchorOf(document.querySelector("#screen .view"))'
                            ' || {}).meters', None) or []
                check(f'{profile} has {peak_n} meter(s) on the page and the smoke is given one '
                      'place for each', len(cs) == peak_n and len(at) == peak_n, [cs, at])
                check('  each place is its meter\'s centre (within 2 px)',
                      len(at) == len(cs) and all(abs(p['x'] - x) < 2 and abs(p['y'] - y) < 2
                                                 for p, (x, y) in zip(at, cs)), [cs, at])
                check('  no page error', errors(b) == [], errors(b))
                b.close()
            finally:
                amp.stop()
    finally:
        b.close()
    try:
        visible_section(dac)
        phone_section(dac)
    finally:
        dac.stop()


run()
print(f'{passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
