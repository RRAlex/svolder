# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Night drive's timing cases in a browser: the checks that read the CPU or the wall clock (a draw's
cost, draws or floor rows a second, the budget's rate rule), so they need the machine to themselves and run
in the gate's serial lane (run-all.sh), not beside the other checks. The rest of the theme's cases are in
nightdrive_theme.py; what both share is in nightdrive_base.py.
  no_frame_while_hidden                             no draw while the page is hidden, draws again once shown
  reduced_motion_starts_on_calm_and_full_can_be_picked   draws keep coming under Full while reduced
  a_paused_player_on_a_desktop_keeps_its_levels_own_pace   draws go on at their rate when the player pauses
  what_a_draw_costs                                 drawMs p95; a draw with every cell lit at 4.5 ms or less (median of 5, Chromium)
  a desktop starts with the wet road allowed by the budget   the budget's answer to the real draws: the wet road is allowed
  the_road_cruises_with_the_song_and_keeps_the_tempo   floor rows a second, loud, silent and with the mood off
  a_slow_page_steps_down_on_the_rate                a page that draws fast and seldom steps down; left alone it keeps its density and rate
  a_phone_starts_at_30_a_second                     Chromium: a coarse pointer begins capped, the draws keep to 21 to 34 a second
  the_readout_rate_is_the_loops_own                 the readout's draws a second within 10 of the loop's own count
Usage: python3 nightdrive_timing.py chromium|firefox"""
import nightdrive_base as base
from nightdrive_base import *  # noqa: F401,F403 (the shared helpers, by their own names)

# five readings of the mean draw and of the draw count, a second apart (the median of the means is what a limit is held to)
FIVE = """new Promise((done) => { const ms = []; const draws = []; const tick = () => {
    const p = __core.theme.driver.perf(); ms.push(p.drawMs); draws.push(p.draws);
    if (ms.length < 5) setTimeout(tick, 1000); else done({ ms, draws }); }; tick(); })"""


SAMPLE = """new Promise((done) => { const out = []; const end = performance.now() + 4000;
  const tick = () => { out.push(__core.theme.driver.perf().drawMs);
    if (performance.now() < end) setTimeout(tick, 50); else done(out); }; tick(); })"""


WHOLE = '(() => DRIVER.then((m) => m.bound.scene.floor.whole))()'.replace('DRIVER', DRIVER)


def rows_a_second(b, secs=4.0):
    one = b.page(WHOLE)
    time.sleep(secs)
    return (b.page(WHOLE) - one) / secs


def hidden(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    time.sleep(0.3)
    b.page(HIDE)
    time.sleep(0.5)
    one = b.page(DRAWS)
    time.sleep(0.8)
    two = b.page(DRAWS)
    check('no_frame_while_hidden: no draw while the page is hidden', one == two, [one, two])
    b.page(SHOW)
    time.sleep(0.5)
    check('  and the draws go on once it shows', b.page(DRAWS) > two + 5, [two, b.page(DRAWS)])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_reduced_start(d):
    r = make(reduced=True)
    try:
        forget(r, d)
        load(r, d)
        poll(lambda: r.page(DRAWS) > 3)
        time.sleep(1.0)
        check('reduced_motion_starts_on_calm_and_full_can_be_picked: the system asks for less and the pace is Calm\'s',
              r.page('matchMedia(\'(prefers-reduced-motion: reduce)\').matches') is True
              and r.page(PERF)['pace'] == 100 and r.page(KEY) is None, [r.page(PERF), r.page(KEY)])
        open_menu(r)
        rows = r.page(MOTION_ROWS)
        check('  Calm is the one checked', [x['id'] for x in rows if x['checked'] == 'true'] == ['calm'], rows)
        r.press('Escape')
        pick(r, 'full')
        time.sleep(0.5)
        got = rate(r, 1.0)
        full = r.page(PERF)
        # a browser too slow to draw in 12 ms is held to 30 a second by the budget, never lower
        check('  and Full can be picked: the draws keep coming', got > 25 and (full['pace'] == 0 or (full['capped'] and full['pace'] < 34)),
              [got, r.page(PERF)])
        check('  no page error', errors(r) == [], errors(r)[:3])
    finally:
        r.close()


def the_paused_player(b, d):
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    playing = rate(b, 1.0)
    status = b.page(PAUSE)
    paused = poll(lambda: b.page("__core.model.page().host?.track?.status") == 'paused', 4.0)
    time.sleep(0.5)
    got = rate(b, 2.0)
    check('a_paused_player_on_a_desktop_keeps_its_levels_own_pace: the draws go on at the rate they had and asked is the level\'s',
          playing > 25 and paused and got > 25 and b.page(PERF)['pace'] < 125, [playing, status, paused, got, b.page(PERF)])


def the_wet_road_start(b, d):
    """The budget answers with what the real draws cost: a desktop that draws in time starts with the wet road allowed.
    A machine busy with other pages can draw slower than the budget wants and drop it, so this case runs alone."""
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    wet = poll(lambda: b.page('__core.theme.driver.perf().wet') is True, 3.0)
    check('a desktop starts with the wet road allowed by the budget', wet, b.page(PERF))


def what_a_draw_costs(b, d):
    """The mean of the last 60 draws, sampled for 4 s, its p95 (the numbers for the doc); with every cell lit the median of five
    readings is held to a limit, and the draws must have gone on between the readings."""
    forget(b, d)
    load(b, d)
    poll(lambda: b.page(DRAWS) > 3)
    # the budget is told every draw took 2 ms, so it does not answer a slow draw by dropping the wet road or the rate:
    # drawMs stays the real time, and the limit below sees a regression the budget would absorb
    b.page('__core.theme.driver.forceDrawMs(2)')
    time.sleep(1.0)
    plain = b.page(SAMPLE)
    print(f'INFO  {browser} 1280x800 dpr 1: drawMs p95 {p95(plain):.2f} ms (mean of 60, sampled 4 s), max {max(plain):.2f}')
    # the worst cases: every cell of the rows lit at 0.9 (the heat grid at its most lifted), and the quiet
    # (the wet road and the haze drawn, the floor flat)
    b.page(f'({FLOOR_AT})(0.9)')
    time.sleep(1.0)
    full = b.page(SAMPLE)
    five = b.page(FIVE)
    print(f'INFO  {browser} 1280x800 dpr 1, every cell lit: drawMs p95 {p95(full):.2f} ms, max {max(full):.2f}, '
          f'median of 5 {median(five["ms"]):.2f} ms {[round(x, 2) for x in five["ms"]]}')
    b.page(f'({HOLD})(0, 120)')
    b.page(f'({FLOOR_AT})(0.05)')
    time.sleep(1.5)
    wet = b.page(SAMPLE)
    seen = b.page('__core.theme.driver.perf()')
    print(f'INFO  {browser} 1280x800 dpr 1, quiet with the wet road and the haze: drawMs p95 {p95(wet):.2f} ms, max {max(wet):.2f}, '
          f'wet allowed {seen["wet"]}, look {b.page(LOOK)}')
    if browser == 'chromium':
        b.send('Emulation.setCPUThrottlingRate', {'rate': 4})
        time.sleep(1.0)
        slow = b.page(SAMPLE)
        b.send('Emulation.setCPUThrottlingRate', {'rate': 1})
        print(f'INFO  chromium CPU 4x slower: drawMs p95 {p95(slow):.2f} ms, max {max(slow):.2f}')
    check('what_a_draw_costs: measured', len(plain) > 10 and len(full) > 10 and len(wet) > 10)
    check('  the quiet sample is a quiet one: the ear hears less than 0.2 of the minute\'s loudest', b.page(EAR)['med'] < 0.2, b.page(EAR))
    if browser == 'chromium':
        # A slower machine is covered by the budget's step-down (budget.js), so this holds the median of five readings
        # to 4.5 ms: the per-draw copy of the wet road and the ridge's step leave about 10 % of margin at 4 ms.
        # drawMs keeps its last value when the loop stops, so the draws must have gone on between the readings.
        check('  the draws went on between the five readings (a stalled loop would keep its last mean)',
              min(five['draws'][i + 1] - five['draws'][i] for i in range(4)) >= 25, five['draws'])
        check('  a draw with every cell lit takes 4.5 ms or less at 1280x800 dpr 1 (headless Chromium, the median of 5 readings, each the mean of 60 draws)',
              median(five['ms']) <= 4.5, [median(five['ms']), five])


def the_cruise(b, d):
    """The road's speed is the tempo's rate times the song's cruise factor, never in its place: loud music
    (long near 1, a factor of 1.4) rolls the floor faster than the silence (0.6), with the mood off (a factor of 1)
    it is between; the tempo is the host's 120 beats a minute, four rows a second."""
    forget(b, d)
    load(b, d, '?theme=nightdrive&nd-debug=noridge')
    poll(lambda: b.page(DRAWS) > 3)
    time.sleep(1.0)
    b.page(f'({HOLD})(0.8, 60)')
    time.sleep(7.0)
    loud = rows_a_second(b)
    b.page(f'({HOLD})(0, 120)')
    time.sleep(8.0)
    quiet = rows_a_second(b)
    load(b, d, '?theme=nightdrive&nd-debug=noridge,nomood')
    poll(lambda: b.page(DRAWS) > 3)
    time.sleep(1.0)
    plain = rows_a_second(b)
    check('the_road_cruises_with_the_song_and_keeps_the_tempo: rows a second are about 5.6 when loud, 2.4 in the silence and 4 '
          'with the mood off (the tempo\'s rate times the factor)',
          4.6 <= loud <= 6.6 and 1.6 <= quiet <= 3.4 and 3.3 <= plain <= 4.7 and loud > 1.8 * quiet, [loud, quiet, plain])
    check('  no page error', errors(b) == [], errors(b)[:3])


def the_rate_trigger(d):
    """A page that draws fast and seldom (here: its main thread busy 45 ms in every 50, between the draws)
    steps down on the rate, though the mean draw time stays far under the slow mark; the same page left alone
    does not."""
    for busy in (False, True):
        b = make(size=(412, 915), dpr=3)
        try:
            forget(b, d)
            load(b, d)
            if busy:
                b.page("""(() => { setInterval(() => { const t = performance.now(); while (performance.now() - t < 45); }, 50); return 1; })()""")
            stepped = poll(lambda: b.page(PERF)['scale'] < 2 or b.page(PERF)['capped'], 14.0) if busy else False
            if not busy:
                time.sleep(10.0)
            seen = b.page(PERF)
            if busy:
                check('a_slow_page_steps_down_on_the_rate: 20 draws a second at 3 ms each steps the scale down or caps the rate '
                      'within 14 s, the mean draw time under the slow mark', stepped and seen['drawMs'] < 12, seen)
            else:
                check('  the same page left alone keeps its density and its rate', seen['scale'] == 2 and seen['capped'] is False
                      and seen['step'] == 0, seen)
            check('  no page error', errors(b) == [], errors(b)[:3])
        finally:
            try:
                forget(b, d)
            finally:
                b.close()


def the_readout_rate(d):
    b = make(size=(412, 915))
    try:
        forget(b, d)
        load(b, d, '?theme=nightdrive&nd-debug')
        time.sleep(1.5)
        text = b.page("document.querySelector('.nd-debug')?.textContent || ''")
        fps = float(text.split(' draws/s')[0].strip())
        check('the_readout_rate_is_the_loops_own: the draws a second the readout says are within 10 of the loop\'s own count',
              abs(fps - rate(b, 1.0)) <= 10, [fps, text[:60]])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()


def the_phone_cap(d):
    """Chromium, a phone at density 3: a coarse pointer begins capped and the draws keep to the cap."""
    b = common.Chromium(size=(390, 844), dpr=3, touch=True)
    try:
        forget(b, d)
        load(b, d)
        time.sleep(1.5)
        first = b.page(PERF)
        check('a_phone_starts_at_30_a_second: a coarse pointer begins capped, and the cap is kept by the draws '
              '(21 to 34 a second where the screen gives 60)', first['capped'] is True and 21 <= rate(b, 3.0) / 3.0 <= 34,
              [first['capped'], first['fps']])
        check('  no page error', errors(b) == [], errors(b)[:3])
    finally:
        try:
            forget(b, d)
        finally:
            b.close()


d = common.Daemon('gungnir2-v2', host='fake')
# the cases that make a browser of their own run first, with no other page drawing beside them
the_readout_rate(d)
if browser == 'chromium':
    the_phone_cap(d)
b = make()
try:
    hidden(b, d)
    the_reduced_start(d)
    the_paused_player(b, d)
    the_wet_road_start(b, d)
    the_cruise(b, d)
    the_rate_trigger(d)
    b.resize(1280, 800)
    what_a_draw_costs(b, d)
    check('no page error in the whole run', errors(b) == [], errors(b)[:3])
finally:
    try:
        forget(b, d)
    finally:
        b.close()
        d.stop()
base.finish('nightdrive_timing.py')
