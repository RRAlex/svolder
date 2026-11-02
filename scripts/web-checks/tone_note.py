# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Tone pane's Flat and Loudness notes: they take no room (the band table and the presets
stay where they are across the three modes) and show as a message when the mode changes into
Flat or Loudness, from a click or from another client; a reload, and a change into EQ, show
none. Numbers from the page, no pictures. Usage: python3 tone_note.py chromium|firefox"""
import atexit
import json
import sys
import urllib.parse
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
FLAT = ('Flat bypasses the bands; the faint line is what EQ would do. Step a band and the mode '
        'switches to EQ.')
LOUD = ('Loudness follows the volume: it adds more the lower you listen, and nothing at full '
        'volume.')
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


TOASTS = "[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"
# page positions (a shorter page clamps the scroll, so the viewport's would move)
TOPS = """(() => ({
  bands: document.getElementById('part-bands').getBoundingClientRect().top + scrollY,
  presets: document.getElementById('part-presets').getBoundingClientRect().top + scrollY,
  notes: document.querySelectorAll('.gnote').length }))()"""


def api_mode(d, mode):
    """A mode change made by another client, through the web API."""
    req = urllib.request.Request(
        f'{d.url}api/v1/devices/{urllib.parse.quote("Gungnir 2")}/eq/mode',
        data=json.dumps({'mode': mode}).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    urllib.request.urlopen(req, timeout=10).read()


def api_loudness(d, raw):
    """A loudness strength set by another client, through the web API (the mode must be
    Loudness)."""
    req = urllib.request.Request(
        f'{d.url}api/v1/devices/{urllib.parse.quote("Gungnir 2")}/eq/loudness',
        data=json.dumps({'raw': raw}).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    urllib.request.urlopen(req, timeout=10).read()


def waiting(b, expression, secs=5.0):
    """Poll `expression` (a page expression) until it is truthy, for at most `secs`; its last value.
    A change goes from the click through the daemon and the stream to the page, which a loaded
    machine slows far past any fixed sleep."""
    got = b.ev(expression)
    for _ in range(int(secs / 0.2)):
        if got:
            break
        b.sleep(0.2)
        got = b.ev(expression)
    return got


NOT_THERE_FIT = {'inside': False, 'clipped': True, 'oneLine': False, 'clear': False,
                 'box': [0, 0]}


def measure(b, script, missing):
    """What `script` measures once the Tone part has what it measures (the strength's words and
    the round buttons: the model has no strength for a moment after a change, and the part is
    drawn without them), else `missing`, which fails the check that reads it."""
    waiting(b, script)
    return b.ev(script) or missing


def bring(b, selector):
    """Scroll the Tone pane to the top of the window and wait until `selector` is inside it (what
    sits above the pane settles late on a loaded machine, and a click or a hit test lands where
    the pane no longer is)."""
    b.ev("document.getElementById('part-tone').scrollIntoView({ block: 'start' })")
    waiting(b, f"(() => {{ const r = document.querySelector({json.dumps(selector)})"
            ".getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; })()", 2.0)


def tap(b, selector):
    """A click on `selector`, the Tone pane first brought into view."""
    bring(b, selector)
    x, y = common.center(b, selector)
    b.click(x, y)


def press(b, mode):
    """Click the mode's button (the pane scrolled into view first) and wait up to 8 s for the page to
    show it pressed. One click: a click that came to nothing is a failure, not tried again."""
    button = f'#part-tone [data-mode={mode}]'
    pressed = f"document.querySelector('{button}').getAttribute('aria-pressed') === 'true'"
    tap(b, button)
    check(f'the {mode} button shows pressed after one click', waiting(b, pressed, 8.0))


WORDS = """(() => { const w = document.getElementById('loud-strength');
  return w ? [w.textContent.trim(), w.querySelector('.n').textContent,
    document.querySelector('#part-tone .seg').dataset.strength ?? null] : null; })()"""
FIT = """(() => { const o = document.querySelector('#part-tone [data-mode=loudness]');
  if (!o || !o.querySelector('.lv') || o.parentElement.querySelectorAll('.rb').length < 2)
    return null;
  const box = o.getBoundingClientRect(), lv = o.querySelector('.lv').getBoundingClientRect();
  const range = document.createRange(); range.selectNodeContents(
    [...o.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim()));
  const lab = range.getBoundingClientRect();
  const rb = [...o.parentElement.querySelectorAll('.rb')].map((e) => e.getBoundingClientRect());
  const inside = (r) => r.left >= box.left - 0.5 && r.right <= box.right + 0.5
    && r.top >= box.top - 0.5 && r.bottom <= box.bottom + 0.5;
  return { box: [box.width, box.height], inside: inside(lab) && inside(lv),
    clipped: o.scrollWidth > o.clientWidth + 1 || o.scrollHeight > o.clientHeight + 1,
    oneLine: lab.height < 24 && lv.height < 16,
    clear: lab.left >= rb[0].right - 0.5 && lab.right <= rb[1].left + 0.5
      && lv.left >= rb[0].right - 0.5 && lv.right <= rb[1].left + 0.5 }; })()"""
GEO = """(() => { const r = (e) => { const b = e.getBoundingClientRect();
    return [b.left, b.right, b.width]; };
  const w = [...document.querySelectorAll('#part-tone .optw')];
  const rb = [...document.querySelectorAll('#part-tone .optw > .rb')].map(r);
  const o = document.querySelector('#part-tone [data-mode=loudness]');
  return { boxes: w.map(r), rb, loud: r(w[2]),
    fonts: [...document.querySelectorAll('#part-tone .opt')].map(
      (e) => getComputedStyle(e).fontSize),
    pressed: [...document.querySelectorAll('#part-tone .opt')].findIndex(
      (e) => e.getAttribute('aria-pressed') === 'true'), lw: o.scrollWidth, cw: o.clientWidth }; })()"""
HITS = """(() => { const w = document.querySelector('#part-tone .optw:last-child');
  if (!w || w.querySelectorAll('.rb').length < 2) return null;
  const o = w.getBoundingClientRect(), rb = [...w.querySelectorAll('.rb')];
  const at = (x, y) => document.elementFromPoint(x, y)?.closest('.rb') ?? null;
  const tap = parseFloat(getComputedStyle(document.querySelector('#part-tone .opt'))
    .minHeight);
  const a = rb[0].getBoundingClientRect(), z = rb[1].getBoundingClientRect();
  return { tap, height: o.height, ends: [a.left - o.left, o.right - z.right],
    corners: [at(o.left + 1, o.top + 2) === rb[0], at(o.left + 1, o.bottom - 2) === rb[0],
      at(o.right - 1, o.top + 2) === rb[1], at(o.right - 1, o.bottom - 2) === rb[1]],
    beside: [at(o.left + 1, o.top + o.height / 2) === rb[0],
      at(o.right - 1, o.top + o.height / 2) === rb[1]],
    outside: [at(o.left - 1.5, o.top + o.height / 2), at(o.right + 1.5, o.top + o.height / 2)]
      .map((e) => e !== null),
    label: at(a.right + 6, o.top + o.height / 2) !== null
      || at(z.left - 6, o.top + o.height / 2) !== null }; })()"""
HOLD_DOWN = ("(() => { document.querySelector('#part-tone .rb:%s').dispatchEvent("
             "new PointerEvent('pointerdown', { bubbles: true, button: 0 })); return 1; })()")
HOLD_UP = "(() => { window.dispatchEvent(new Event('pointerup')); return 1; })()"


def loudness_words(b, tag):
    """The strength's words on the Loudness button, with the page in Loudness: they show, are
    what the round buttons are described by, change after a + and a -, follow a held - or +
    while it is held, and the label and words fit the button."""
    got = waiting(b, WORDS) and b.ev(WORDS)
    check(f'{tag}: the strength\'s words show on the Loudness button', bool(got)
          and got[0] == f'strength {got[1]}' and got[1] == got[2], got)
    if not got:
        return
    check(f'{tag}: both round buttons are described by them',
          b.ev("[...document.querySelectorAll('#part-tone .rb')].map((e) => ["
               "e.getAttribute('aria-describedby'), document.getElementById("
               "e.getAttribute('aria-describedby'))?.textContent.trim()].join('=')).join('|')")
          == f'loud-strength=strength {got[1]}|loud-strength=strength {got[1]}', got)
    fit = measure(b, FIT, NOT_THERE_FIT)
    check(f'{tag}: the label and the words fit inside the button, on one line each, clear of '
          'the round buttons', fit['inside'] and not fit['clipped'] and fit['oneLine']
          and fit['clear'], fit)
    start = int(got[1])
    tap(b, '#part-tone .rb:last-child')
    b.sleep(0.8)
    up = b.ev(WORDS)
    check(f'{tag}: the words change after a +', up and int(up[1]) == start + 1 and up[1] == up[2],
          [start, up])
    tap(b, '#part-tone .rb:first-child')
    b.sleep(0.8)
    down = b.ev(WORDS)
    check(f'{tag}: and after a -', down and int(down[1]) == start and down[1] == down[2],
          [start, down])
    b.ev(HOLD_DOWN % 'last-child')
    b.sleep(1.2)
    held = b.ev(WORDS)
    b.ev(HOLD_UP)
    b.sleep(1.0)
    after = b.ev(WORDS)
    check(f'{tag}: the words follow a held + while it is held (no freeze until release)',
          held and int(held[1]) >= start + 3 and held[1] == held[2], [start, held])
    check(f'{tag}: and agree with the reading after the release', after
          and int(after[1]) >= int(held[1]) and after[1] == after[2], [held, after])
    b.ev(HOLD_DOWN % 'first-child')
    b.sleep(1.2)
    b.ev(HOLD_UP)
    b.sleep(1.0)
    check(f'{tag}: a held - lowers them again', int(b.ev(WORDS)[1]) < int(after[1]), after)
    fit = measure(b, FIT, NOT_THERE_FIT)
    check(f'{tag}: the label and the words still fit after the steps', fit['inside']
          and not fit['clipped'] and fit['oneLine'] and fit['clear'], fit)


def one_width(tag, seen, equal):
    """`seen` is every step of a round trip, as (mode, geometry). The Loudness button's box (its
    optw with the round buttons) has one width at every step, the Flat and EQ boxes keep their left
    edges (every Flat step against the first), their labels keep their font size, and in Loudness
    the - sits at the box's left end and the + at its right end. With `equal` the three boxes are
    one width in every mode (within 1 px); without it the Loudness box is wider than Flat's."""
    widths = [g['loud'][2] for _, g in seen]
    print(f'{tag}: Loudness box widths', [(m, round(g['loud'][2], 2)) for m, g in seen])
    check(f'{tag}: the Loudness box is one width at every step (within 1 px)',
          max(widths) - min(widths) <= 1, widths)
    if equal:
        check(f'{tag}: the three buttons are an equal third in every mode (within 1 px)',
              all(max(b3[2] for b3 in g['boxes']) - min(b3[2] for b3 in g['boxes']) <= 1
                  for _, g in seen), [g['boxes'] for _, g in seen])
    else:
        check(f'{tag}: the Loudness button is wider than the Flat button in every mode (by 8 px '
              'or more)', all(g['loud'][2] - g['boxes'][0][2] >= 8 for _, g in seen),
              [g['boxes'] for _, g in seen])
    lefts = [[g['boxes'][i][0] for _, g in seen] for i in (0, 1)]
    check(f'{tag}: the Flat and EQ boxes keep their left edges at every step (within 1 px)',
          all(max(v) - min(v) <= 1 for v in lefts), lefts)
    fonts = [[g['fonts'][i] for _, g in seen] for i in (0, 1)]
    check(f'{tag}: the Flat and EQ labels keep their font size in every mode',
          all(len(set(v)) == 1 for v in fonts), fonts)
    g = next(g for m, g in seen if m == 'loudness')
    loud = g['loud']
    check(f'{tag}: in Loudness the - sits at the box\'s left end and the + at its right end '
          '(within 6 px)', len(g['rb']) == 2 and 0 <= g['rb'][0][0] - loud[0] <= 6
          and 0 <= loud[1] - g['rb'][1][1] <= 6, g)


def hit_areas(b, tag):
    """In Loudness the - and + hit areas fill the button's height and its ends (a point in the
    corner outside the drawn circle hits the round button), never reach past the box, and never
    cover the label."""
    bring(b, '#part-tone .optw:last-child')
    h = measure(b, HITS, None)
    if h is None:
        check(f'{tag}: the round buttons are there to measure', False)
        return
    check(f'{tag}: the round buttons\' hit areas fill the button\'s height and end padding',
          h['height'] >= h['tap'] - 0.5 and all(h['corners']) and all(h['beside']), h)
    check(f'{tag}: they stop at the box\'s edge and keep off the label',
          not any(h['outside']) and not h['label'], h)


def strength_127(b, tag):
    """The strongest strength the device takes, set by another client, in the Loudness mode: the
    label and the words fit the button, on one line each, clear of the round buttons."""
    api_loudness(d, 127)
    waiting(b, "document.getElementById('loud-strength')?.textContent.trim() === 'strength 127'")
    b.sleep(0.3)
    got = b.ev(WORDS)
    check(f'{tag}: the words read strength 127', bool(got) and got[0] == 'strength 127', got)
    fit = measure(b, FIT, NOT_THERE_FIT)
    print(f'{tag}: Loudness box with strength 127', [round(v, 1) for v in fit['box']])
    check(f'{tag}: the label and "strength 127" fit inside the button, on one line each, clear '
          'of the round buttons', fit['inside'] and not fit['clipped'] and fit['oneLine']
          and fit['clear'], fit)
    api_loudness(d, 20)
    b.sleep(1.2)


PRE = ("const O = window.EventSource; window.EventSource = class extends O {"
       " constructor(u, o) { super(u, o); const es = this;"
       " document.addEventListener('svx-model',"
       " (e) => es.dispatchEvent(new MessageEvent('model', { data: e.detail }))); } };")
d = common.Daemon()
atexit.register(d.stop)
b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(1280, 1100))
b.preload(PRE)
try:
    api_mode(d, 'eq')
    b.goto(d.url)
    b.sleep(1.0)
    check('a load in EQ shows no message', b.ev(TOASTS) == [], b.ev(TOASTS))
    base = b.ev(TOPS)
    check('no gnote paragraph in EQ', base['notes'] == 0, base)
    # the EQ graph's background is a color-mix(): the open test must read its alpha (0.7 of the
    # card) and count it as covering the smoke, whatever form the browser computes it to
    graph = ("import(new URL('assets/themes/ember/effects.js', document.baseURI).href).then((m) => ["
             "getComputedStyle(document.querySelector('.eqg')).backgroundColor, "
             "m.backgroundAlpha(getComputedStyle(document.querySelector('.eqg'))"
             ".backgroundColor)])")
    colour, alpha = (b.ev if browser == 'chromium' else b.page)(graph)
    check('the EQ graph\'s colour-mix background counts as covering the smoke (alpha 0.35 or '
          'more)', alpha >= 0.35, [colour, alpha])

    def same(label):
        got = b.ev(TOPS)
        check(f'{label}: band table and presets within 1 px of EQ, no gnote',
              abs(got['bands'] - base['bands']) <= 1 and abs(got['presets'] - base['presets']) <= 1
              and got['notes'] == 0, [got, base])

    seen = []
    press(b, 'flat')
    b.sleep(0.8)
    seen.append(('flat', b.ev(GEO)))
    check('a click on Flat shows the Flat words', b.ev(TOASTS) == [FLAT], b.ev(TOASTS))
    same('Flat')
    b.sleep(3.0)
    check('the message goes after its time', b.ev(TOASTS) == [], b.ev(TOASTS))
    press(b, 'eq')
    b.sleep(0.8)
    seen.append(('eq', b.ev(GEO)))
    check('a click on EQ shows no message', b.ev(TOASTS) == [], b.ev(TOASTS))
    same('EQ again')
    press(b, 'loudness')
    b.sleep(0.8)
    seen.append(('loudness', b.ev(GEO)))
    check('a click on Loudness shows the Loudness words', b.ev(TOASTS) == [LOUD], b.ev(TOASTS))
    same('Loudness')
    strength = "+(document.querySelector('#part-tone .seg').dataset.strength ?? -1)"
    check('Loudness: no strength row, a round - and + on the Loudness button',
          b.ev("document.querySelectorAll('#part-tone .r-loud').length") == 0
          and b.ev("[...document.querySelectorAll('#part-tone .optw > .rb')]"
                   ".map((e) => e.getAttribute('aria-label')).join('|')")
          == 'Lower the loudness|Raise the loudness')
    check('the round buttons are not nested in a button',
          b.ev("document.querySelectorAll('#part-tone button button').length") == 0)
    b.sleep(0.5)
    start = b.ev(strength)
    for _ in range(2):
        tap(b, '#part-tone .rb:last-child')
        b.sleep(0.8)
    up = b.ev(strength)
    check('a click on + raises the strength one step each time', up == start + 2, [start, up])
    tap(b, '#part-tone .rb:first-child')
    b.sleep(0.8)
    down = b.ev(strength)
    check('a click on - lowers the strength one step', down == up - 1, [up, down])
    check('a click on a round button shows no message and keeps Loudness',
          b.ev(TOASTS) in ([LOUD], []) and b.ev(
              "document.querySelector('#part-tone [data-mode=loudness]')"
              ".getAttribute('aria-pressed')") == 'true')
    same('Loudness after the steps')
    loudness_words(b, 'at 1280 x 1100')
    same('Loudness after the held steps')
    hit_areas(b, 'at 1280 x 1100')
    strength_127(b, 'at 1280 x 1100')
    press(b, 'flat')
    b.sleep(0.8)
    seen.append(('flat', b.ev(GEO)))
    one_width('at 1280 x 1100', seen, True)
    press(b, 'loudness')
    b.sleep(0.8)
    b.sleep(3.0)
    api_mode(d, 'flat')
    b.sleep(1.2)
    check('a change made by another client into Flat shows the Flat words',
          b.ev(TOASTS) == [FLAT], b.ev(TOASTS))
    same('Flat by the API')
    b.sleep(3.0)
    api_mode(d, 'eq')
    b.sleep(1.2)
    check('another client changing into EQ shows no message', b.ev(TOASTS) == [], b.ev(TOASTS))
    # a whole model from the stream that shows no mode (the page forgets what it had seen), then a
    # click on a mode: the click is the person's own change, so it shows the mode's words
    b.sleep(1.0)
    api_mode(d, 'eq')
    b.sleep(1.5)
    model = b.page("import(new URL('assets/core/boot.js', document.baseURI)).then((m) => {"
                 " const page = JSON.parse(JSON.stringify(m.core.model.page()));"
                 " page.device.tone.mode = null;"
                 " page.device.tone.modes.forEach((mode) => { mode.on = false; });"
                 " return JSON.stringify(page); })")
    b.ev(f"document.dispatchEvent(new CustomEvent('svx-model', {{ detail: {json.dumps(model)} }}))")
    b.sleep(0.8)
    check('a model with no mode shows no message', b.ev(TOASTS) == [], b.ev(TOASTS))
    press(b, 'flat')
    b.sleep(0.8)
    check('a click on Flat right after a model that forgot the mode shows the Flat words',
          b.ev(TOASTS) == [FLAT], b.ev(TOASTS))
    b.sleep(3.0)
    api_mode(d, 'flat')
    b.goto(d.url)
    b.sleep(0.8)
    check('a reload in Flat shows no message', b.ev(TOASTS) == [], b.ev(TOASTS))
    same('Flat after a reload')
    check('no page error', (b.errors if browser == 'chromium' else b.collect_errors()) == [],
          b.errors if browser == 'chromium' else '')
finally:
    b.close()



def at_size(w, h, equal):
    """The same at another width: the words fit, the band table stays put across Flat, EQ and
    Loudness and back to Flat, the Loudness button keeps one width, the round buttons' hit areas
    hold, and the strongest strength fits."""
    tag = f'at {w} x {h}'
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(w, h))
    b.preload(PRE)
    try:
        api_mode(d, 'eq')
        b.goto(d.url)
        b.sleep(1.0)
        # the Tone pane may be below the fold: scrolled to once, so the clicks land
        b.ev("document.getElementById('part-tone').scrollIntoView({ block: 'start' })")
        b.sleep(0.4)
        base = b.ev(TOPS)
        seen = []
        for mode in ('flat', 'eq', 'loudness', 'flat'):
            press(b, mode)
            b.sleep(0.8)
            seen.append((mode, b.ev(GEO)))
            got = b.ev(TOPS)
            check(f'{tag} in {mode}: band table and presets within 1 px of EQ, no gnote',
                  abs(got['bands'] - base['bands']) <= 1
                  and abs(got['presets'] - base['presets']) <= 1 and got['notes'] == 0,
                  [got, base])
            if mode == 'loudness':
                loudness_words(b, tag)
                got = b.ev(TOPS)
                check(f'{tag} after the steps: band table within 1 px of EQ',
                      abs(got['bands'] - base['bands']) <= 1, [got, base])
                hit_areas(b, tag)
                strength_127(b, tag)
        one_width(tag, seen, equal)
        check(f'{tag}: no page error', (b.errors if browser == 'chromium'
                                        else b.collect_errors()) == [])
    finally:
        b.close()


def held_without_move():
    """A browser with no `Element.moveBefore`: the Tone part's swap waits while a round button is
    held (keep.js), so the strength's words move only by the stepper's own write of the answer's
    text, and they agree with the reading after the release."""
    tag = 'without moveBefore'
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(860, 900))
    b.preload(PRE + ' delete Element.prototype.moveBefore;')
    try:
        api_mode(d, 'loudness')
        api_loudness(d, 20)
        b.goto(d.url)
        b.sleep(1.0)
        b.ev("document.getElementById('part-tone').scrollIntoView({ block: 'start' })")
        b.sleep(0.4)
        check(f'{tag}: the page has no moveBefore',
              b.ev("typeof Element.prototype.moveBefore") == 'undefined')
        start = b.ev(WORDS)
        check(f'{tag}: the strength\'s words show', bool(start) and start[1] == start[2], start)
        b.ev(HOLD_DOWN % 'last-child')
        b.sleep(1.2)
        held = b.ev(WORDS)
        b.ev(HOLD_UP)
        b.sleep(1.0)
        after = b.ev(WORDS)
        check(f'{tag}: the words follow a held + while it is held (the swap waits)',
              held and int(held[1]) >= int(start[1]) + 3 and held[1] == held[2], [start, held])
        check(f'{tag}: and agree with the reading after the release', after
              and int(after[1]) >= int(held[1]) and after[1] == after[2], [held, after])
        check(f'{tag}: no page error', (b.errors if browser == 'chromium'
                                        else b.collect_errors()) == [])
    finally:
        b.close()


# equal thirds where the words fit, a wider Loudness button where they do not
for size, equal in (((600, 900), True), ((390, 844), False), ((360, 800), False),
                    ((860, 900), False)):
    at_size(*size, equal)
held_without_move()
print(f'tone_note.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
