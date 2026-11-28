# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's timing cases in a browser: the checks that read the wall clock (the draws
a loop makes in a stretch of seconds, the pause and the pace of a held key), so they need the machine to themselves and run in the gate's serial lane
(run-all.sh) beside nightdrive_timing.py, not among the other checks. What both share with the
theme's other browser checks is in voidgarden_base.py. No case asserts a frame rate or a GPU time:
the loop on software rendering is as slow as it is. A pace is held as a ceiling (a loop may draw
less often than its level allows, never more) and a rule of the page as the time it takes.
  no_draw_while_hidden                        no draw while the page is hidden; draws again once shown
  each_level_keeps_under_its_pace             Still draws at most about once a second, Calm at most ten a second, and the loop's own pace says so; a scene that is alive draws at least once
  a_phone_stays_under_its_cap                 Chromium, a coarse pointer under Full: at most 30 draws a second (the cap), `capped` says so
  a_held_key_repeats_at_the_cores_pace        Chromium: ↑ held, one step at the press, the next after about 380 ms, then about every 70 ms; none after the release
Usage: python3 voidgarden_timing.py chromium|firefox"""
import time

import common
import voidgarden_base as vb
from voidgarden_base import check, load, make, poll

DRAWS = f'{vb.DRIVER}.perf().draws'
PERF = f'{vb.DRIVER}.perf()'
LEVEL = "try { localStorage.setItem('svolder.theme.voidgarden.motion', '\"%s\"'); } catch (e) {}"


def fresh(d, level='full', **kw):
    b = make('webgl2', **kw)
    b.preload(LEVEL % level)
    vb.forget(b, d)
    return b


def draws_in(b, secs):
    one = b.page(DRAWS)
    time.sleep(secs)
    return b.page(DRAWS) - one


def no_draw_while_hidden(d):
    b = fresh(d)
    try:
        load(b, d)
        check('no_draw_while_hidden: the scene draws while shown', poll(lambda: b.page(DRAWS) > 2, 20.0), b.page(PERF))
        b.page(vb.HIDE)
        time.sleep(1.0)
        held = draws_in(b, 3.0)
        check('  hidden for 3 s: no draw', held == 0, held)
        b.page(vb.SHOW)
        again = poll(lambda: draws_in(b, 1.0) > 0, 10.0)
        check('  shown again: it draws again', bool(again), b.page(PERF))
        check('  no page error', vb.logs(b) == [], vb.logs(b)[:3])
    finally:
        b.close()


def each_level_keeps_under_its_pace(d):
    for level, ceiling in (('still', 1.5), ('calm', 13.0)):
        b = fresh(d, level)
        try:
            load(b, d)
            poll(lambda: b.page(DRAWS) >= 1, 20.0)
            secs = 6.0
            got = draws_in(b, secs)
            p = b.page(PERF)
            check(f'each_level_keeps_under_its_pace: {level}: at most {ceiling} draws a second (a ceiling, over {secs:.0f} s) and at least one',
                  1 <= got <= ceiling * secs and p['motion'] == level and p['pace'] >= {'still': 1000, 'calm': 100}[level], [got, p['motion'], p['pace']])
            check('  no page error', vb.logs(b) == [], vb.logs(b)[:3])
        finally:
            b.close()


def a_phone_stays_under_its_cap(d):
    b = fresh(d, 'full', size=(390, 844), touch=True)
    try:
        load(b, d)
        poll(lambda: b.page(DRAWS) >= 1, 20.0)
        secs = 6.0
        got = draws_in(b, secs)
        p = b.page(PERF)
        check('a_phone_stays_under_its_cap: a coarse pointer under Full draws at most 30 a second (a little over for the window\'s edges)',
              p['capped'] is True and got <= 34 * secs and p['pace'] >= 1000 / 30 - 1, [got, p['capped'], p['pace']])
    finally:
        b.close()


def a_held_key_repeats_at_the_cores_pace(d):
    b = make('none')
    try:
        vb.forget(b, d)
        load(b, d)
        b.page(vb.REC)
        time.sleep(0.5)
        base = {'key': 'ArrowUp', 'code': 'ArrowUp', 'windowsVirtualKeyCode': 38, 'nativeVirtualKeyCode': 38}
        # a held key as the page sees it: the press, then the system's own repeats (the page ignores them and paces itself)
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyDown'})
        for _ in range(8):
            time.sleep(0.25)
            b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyDown', 'autoRepeat': True})
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyUp'})
        stamps = [p[2] for p in vb.posts(b, '/volume')]
        time.sleep(0.6)
        later = [p[2] for p in vb.posts(b, '/volume')]
        gaps = [b_ - a_ for a_, b_ in zip(stamps, stamps[1:])]
        rest = sorted(gaps[1:])
        median = rest[len(rest) // 2] if rest else None
        check('a_held_key_repeats_at_the_cores_pace: the press steps once, the next step comes after about 380 ms (not before 300)',
              len(stamps) >= 4 and gaps[0] >= 300, [len(stamps), [round(g) for g in gaps[:4]]])
        check('  then steps come about every 70 ms (the median gap between 50 and 160 ms)',
              median is not None and 50 <= median <= 160, [round(median) if median else None, [round(g) for g in gaps[:8]]])
        check('  and none after the release', len(later) == len(stamps), [len(stamps), len(later)])
    finally:
        b.close()


d = common.Daemon('gungnir2-v2', host='fake')
try:
    no_draw_while_hidden(d)
    each_level_keeps_under_its_pace(d)
    if vb.engine == 'chromium':
        a_phone_stays_under_its_cap(d)
        a_held_key_repeats_at_the_cores_pace(d)
finally:
    d.stop()
vb.done('voidgarden_timing.py')
