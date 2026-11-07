# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The core's computer side in a real page (design: the computer flip), through the core's own API,
on a fake host (`SVOLDER_HOST=fake`, whose EQ and sink live in memory: no PipeWire, no sound):
  the_host_event_carries_the_computer   the page's host holds the computer's volume, EQ points,
                                        presets and the curve, for a DAC's page and an amp's page,
                                        and `computer.switchOf` names the shown device
  each_action_reaches_its_route         every `actions.computer` call posts its route with its
                                        body (the request log) and the model shows the change
  a_refusal_toasts_and_rereads          a refused write is toasted with the daemon's words and the
                                        host is read again
  every_point_write_carries_its_slot    a set and a remove name the point by its slot
  a_stale_place_still_lands_on_the_point_dragged   another viewer's add moves the places while this
                                        page still holds the old table: the write lands on the point
                                        dragged, its neighbour is untouched; a point that is gone is a
                                        409, one toast, no more writes, the points read again
  the_side_is_kept_and_a_flip_writes_nothing   the side survives a reload, and neither the load
                                        nor flips post anything
  unavailable_hides_nothing_but_says_why   `SVOLDER_FAKE_HOST_EQ=off`: the switch carries the whole
                                        sentence, the DAC side is shown whatever was kept, the DAC's
                                        own volume is still there
  the_question_is_asked_with_keys_and_buttons   `core.ask` is a card with Yes and No (the focus on
                                        No, 44 px tall), Escape and `n` answer No, `y` and a click
                                        on Yes answer Yes
  the_same_preset_on_both_asks_and_yes_flattens_the_dac   recalling on the computer a preset the DAC
                                        plays asks; no sends nothing; yes repeats with confirm and
                                        the DAC is flat
Usage: python3 computer_core.py chromium|firefox"""
import json
import os
import sys
import time

import common

os.environ.setdefault('SVOLDER_FAKE_COMPUTER_VOLUME', '42')

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


FLEET = [('fake-amp', 'Amp', '1112131415161718')]
# every write the page posts and every read of the host, with the status of the write
HOOK = ("window.__posts = []; window.__gets = []; const f = window.fetch; window.fetch = (...a) => {"
        " const post = a[1] && a[1].method === 'POST';"
        " if (!post) { if (String(a[0]).startsWith('api/v1/host')) window.__gets.push(String(a[0]));"
        " return f(...a); }"
        " const entry = [String(a[0]), a[1].body, null]; window.__posts.push(entry);"
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""


def make(size=(1280, 900)):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)
    b.preload(HOOK)
    return b


def errors(b):
    """The page's errors, but for the console's lines about the refusals a check makes on purpose."""
    found = b.errors if browser == 'chromium' else b.collect_errors()
    return [e for e in found if 'status of 409' not in e and 'status of 422' not in e]


def poll(read, secs=6.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d):
    b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
    b.page(BOOT)
    time.sleep(0.5)


COMPUTER = '__core.model.page().host?.computer'


def host_event(b, d):
    load(b, d)
    got = poll(lambda: b.page(f'!!({COMPUTER}?.volume && {COMPUTER}?.eq?.points?.length)'))
    check('the_host_event_carries_the_computer: volume and points', got,
          b.page(f'JSON.stringify({COMPUTER})')[:300])
    c = b.page(f'({COMPUTER})')
    check('the_host_event_carries_the_computer: the curve has 181 points', len(c['eq']['response']) == 181)
    check('the_host_event_carries_the_computer: the presets are listed with what plays',
          len(c['presets']) > 0 and all('playable' in p for p in c['presets']), c['presets'])
    sw = b.page('__core.computer.switchOf(__core.model.page())')
    check('the_host_event_carries_the_computer: the switch names the shown device',
          sw == {'dac': 'Gungnir 2', 'computer': 'This computer', 'side': 'dac', 'unavailable': None}, sw)
    tone = b.page('(() => { const t = __core.computer.toneOf(__core.model.page());'
                  ' return { line: t.line, verified: t.verified, canAdd: t.canAdd, modes: t.modes.map((m) => m.label) }; })()')
    check('the_host_event_carries_the_computer: the tone reads its line and modes',
          tone['line'].endswith('points · preamp 0.0 dB') and tone['modes'] == ['Off', 'EQ'], tone)
    b.page("__core.actions.showDevice('Amp').then(() => new Promise((r) => setTimeout(r, 1200)))")
    amp = b.page('({ drawable: __core.computer.drawable(__core.model.page()),'
                 ' name: __core.computer.switchOf(__core.model.page())?.dac,'
                 ' volume: __core.computer.volumeOf(__core.model.page())?.text,'
                 ' presets: __core.computer.toneOf(__core.model.page())?.presets.filter((p) => p.built_in).length })')
    check('the_host_event_carries_the_computer: an amp page has the computer side, and no built-ins',
          amp['drawable'] is True and amp['name'] == 'Amp' and amp['volume'] not in (None, '—')
          and amp['presets'] == 0, amp)


def posted(b):
    return b.page('window.__posts.map((p) => [p[0], JSON.parse(p[1]), p[2]])')


def actions(b, d):
    load(b, d)
    poll(lambda: b.page(f'!!{COMPUTER}?.volume'))
    run = lambda call: b.page(f'(async () => JSON.stringify(await __core.actions.computer.{call}))()')  # noqa: E731
    changed = lambda expr: poll(lambda: b.page(expr) is True)  # noqa: E731
    run('setVolume(33)')
    check('each_action_reaches_its_route: the volume', changed(f'{COMPUTER}.volume.pct === 33'),
          b.page(f'{COMPUTER}.volume'))
    run('setMute(true)')
    check('each_action_reaches_its_route: the mute', changed(f'{COMPUTER}.volume.muted === true'))
    run('setMute(false)')
    run("setMode('eq')")
    check('each_action_reaches_its_route: the mode', changed(f'{COMPUTER}.eq.on === true'))
    before = b.page(f'{COMPUTER}.eq.points.length')
    run("addPoint({ type: 'LSC', f: 120, g: 3, q: 0.7 })")
    check('each_action_reaches_its_route: add reads back one more point, a shelf',
          changed(f'{COMPUTER}.eq.points.length === {before + 1}'
                  # the points are listed by frequency: the 120 Hz shelf is listed before the 1 kHz peak
                  f' && {COMPUTER}.eq.points[0].type === "LSC"'), b.page(f'{COMPUTER}.eq.points'))
    run('setPoint(1, { g: 2.5 }, { done: false })')
    run('setPoint(1, {}, { done: true })')
    check('each_action_reaches_its_route: a point moves', changed(f'{COMPUTER}.eq.points[0].g === 2.5'),
          b.page(f'{COMPUTER}.eq.points[0]'))
    check('each_action_reaches_its_route: what the page draws is what the model holds once released',
          changed(f'JSON.stringify(__core.computer.toneOf(__core.model.page()).points.map((p) => [p.f, p.g, p.q]))'
                  f' === JSON.stringify({COMPUTER}.eq.points.map((p) => [p.f, p.g, p.q]))'))
    run('removePoint(1)')
    check('each_action_reaches_its_route: remove', changed(f'{COMPUTER}.eq.points.length === {before}'))
    name = 'Warm'
    run(f'recallPreset({json.dumps(name)})')
    check('each_action_reaches_its_route: a preset recalled', changed(f'{COMPUTER}.eq.preset === {json.dumps(name)}'),
          [name, b.page(f'{COMPUTER}.eq.preset')])
    run("savePreset('Computer mine')")
    check('each_action_reaches_its_route: saved from the computer, listed',
          changed(f"{COMPUTER}.presets.some((p) => p.name === 'Computer mine')"),
          b.page(f'{COMPUTER}.presets.map((p) => p.name)'))
    log = posted(b)
    paths = [(p[0], p[2]) for p in log]
    want = ['api/v1/host/volume', 'api/v1/host/mute', 'api/v1/host/mute', 'api/v1/host/eq',
            'api/v1/host/eq/points', 'api/v1/host/eq/points/1', 'api/v1/host/eq/points/1',
            "api/v1/host/eq/points/1/remove", 'api/v1/host/eq/preset',
            'api/v1/devices/Gungnir%202/presets']
    check('each_action_reaches_its_route: the request log is these routes, all answered 200',
          [p[0] for p in paths] == want and all(p[1] == 200 for p in paths), paths)
    bodies = {p[0]: p[1] for p in log}
    check('each_action_reaches_its_route: the bodies (done, one-based n, the shape)',
          bodies['api/v1/host/volume'] == {'pct': 33} and bodies['api/v1/host/eq'] == {'on': True}
          and bodies['api/v1/host/eq/points'] == {'type': 'LSC', 'f': 120, 'g': 3, 'q': 0.7}
          and [{k: v for k, v in p[1].items() if k != 'slot'} for p in log if p[0] == 'api/v1/host/eq/points/1']
          == [{'g': 2.5, 'done': False}, {'g': 2.5, 'done': True}], bodies)
    sets = [p[1] for p in log if p[0] == 'api/v1/host/eq/points/1']
    removes = [p[1] for p in log if p[0] == 'api/v1/host/eq/points/1/remove']
    check('every_point_write_carries_its_slot: each set and the remove name the point by its slot (1 to 16)',
          len(sets) == 2 and len(removes) == 1
          and all(isinstance(x.get('slot'), int) and 1 <= x['slot'] <= 16 for x in sets + removes)
          and sets[0]['slot'] == sets[1]['slot'] == removes[0]['slot'], [sets, removes])
    check('no page error', errors(b) == [], errors(b)[:3])


def slots(b, d):
    load(b, d)
    poll(lambda: b.page(f'!!{COMPUTER}?.eq'))
    run = lambda call: b.page(f'(async () => JSON.stringify(await __core.actions.computer.{call}))()')  # noqa: E731
    pts = lambda: b.page(f'JSON.stringify({COMPUTER}.eq.points)')  # noqa: E731
    run("setMode('eq')")
    run("addPoint({ type: 'PK', f: 120, g: 0, q: 1 })")
    run("addPoint({ type: 'PK', f: 5000, g: 0, q: 1 })")
    poll(lambda: b.page(f'{COMPUTER}.eq.points.length') == 3)
    stale = poll(lambda: b.page(f'JSON.stringify({COMPUTER})'))
    seen = json.loads(stale)['eq']['points']
    first, second = seen[0], seen[1]
    # another viewer adds a 50 Hz point: every place below it moves down, and this page has not
    # read that yet (the old model is put back in the page)
    b.page("(async () => { await fetch('api/v1/host/eq/points', { method: 'POST', headers: {"
           " 'Content-Type': 'application/json' }, body: JSON.stringify({ type: 'PK', f: 50, g: 0, q: 1 }) }); })()")
    poll(lambda: b.page(f'{COMPUTER}.eq.points.length') == 4)
    time.sleep(0.4)
    b.page(f"__core.model.emit('host', {{ ...__core.model.page().host, computer: {stale} }})")
    mark = len(posted(b))
    run('setPoint(1, { g: 5 }, { done: false })')
    run('setPoint(1, {}, { done: true })')
    time.sleep(0.5)
    sent = posted(b)[mark:]
    after = {p['slot']: p for p in json.loads(pts())}
    check('a_stale_place_still_lands_on_the_point_dragged: the posts name the dragged point\'s slot',
          sent and all(p[1].get('slot') == first['slot'] for p in sent), sent)
    check('a_stale_place_still_lands_on_the_point_dragged: that point has the new gain',
          after[first['slot']]['g'] == 5, after.get(first['slot']))
    check('a_stale_place_still_lands_on_the_point_dragged: its neighbour (now first in the table) is untouched',
          all(p['g'] == 0 for slot, p in after.items() if slot != first['slot']), after)
    # a slot nobody holds: the page's write is a 409, the toast says so, nothing more is sent
    gone = second['slot']
    b.page("(async () => { await fetch('api/v1/host/eq/points/1/remove', { method: 'POST', headers: {"
           f" 'Content-Type': 'application/json' }}, body: JSON.stringify({{ slot: {gone} }}) }}); }})()")
    poll(lambda: b.page(f'{COMPUTER}.eq.points.length') == 3)
    time.sleep(0.4)
    b.page(f"__core.model.emit('host', {{ ...__core.model.page().host, computer: {stale} }})")
    mark = len(posted(b))
    b.page("document.querySelectorAll('#toasts .toast').forEach((t) => t.remove())")
    run("setPoint(2, { g: 4 }, { done: false })")
    time.sleep(0.4)
    run("setPoint(2, { g: 6 }, { done: false })")
    run("setPoint(2, {}, { done: true })")
    time.sleep(0.6)
    sent = posted(b)[mark:]
    check('a_moved_point_is_a_409_that_ends_the_drag: one request, answered 409, nothing more',
          len(sent) == 1 and sent[0][2] == 409, sent)
    words = b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.dataset.text)")
    check('a_moved_point_is_a_409_that_ends_the_drag: the toast says the point moved',
          words == ['That point moved. Try again.'], words)
    check('a_moved_point_is_a_409_that_ends_the_drag: the points are read again',
          poll(lambda: b.page(f'{COMPUTER}.eq.points.length') == 3), pts())


def refusal(b, d):
    load(b, d)
    poll(lambda: b.page(f'!!{COMPUTER}?.eq'))
    b.page('window.__gets.length = 0')
    answer = b.page("(async () => JSON.stringify(await __core.actions.computer.addPoint("
                    "{ type: 'ZZ', f: 100, g: 0, q: 1 })))()")
    words = json.loads(answer).get('error')
    check('a_refusal_toasts_and_rereads: the daemon refuses with words', bool(words), answer)
    shown = poll(lambda: b.page("[...document.querySelectorAll('#toasts .toast')].map((t) => t.dataset.text)"))
    check('a_refusal_toasts_and_rereads: the words are the toast', shown == [words], [shown, words])
    reread = poll(lambda: b.page("window.__gets.some((g) => g.startsWith('api/v1/host'))"))
    check('a_refusal_toasts_and_rereads: the host is read again', reread is True, b.page('window.__gets'))
    check('a_refusal_toasts_and_rereads: no value is held after it',
          b.page("__core.computer.toneOf(__core.model.page()).points.length")
          == b.page(f'{COMPUTER}.eq.points.length'))


def kept_and_silent(b, d):
    load(b, d)
    b.page("localStorage.setItem('svolder.side', 'computer')")
    b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
    b.page(BOOT)
    poll(lambda: b.page(f'!!{COMPUTER}?.eq'))
    check('the_side_is_kept_and_a_flip_writes_nothing: a reload keeps the computer side',
          b.page('__core.computer.side()') == 'computer')
    b.page("(__core.computer.flip('dac'), __core.computer.flip('computer'), __core.computer.flip('dac'), 1)")
    time.sleep(0.8)
    check('the_side_is_kept_and_a_flip_writes_nothing: the load and three flips post nothing',
          b.page('window.__posts.length') == 0, b.page('window.__posts'))
    check('the_side_is_kept_and_a_flip_writes_nothing: the side is kept in storage',
          b.page("localStorage.getItem('svolder.side')") == 'dac')


def unavailable(b, d):
    load(b, d)
    poll(lambda: b.page(f'!!{COMPUTER}'))
    why = b.page(f'{COMPUTER}.unavailable')
    check('unavailable_hides_nothing_but_says_why: the model carries the whole sentence',
          isinstance(why, str) and why.startswith('computer controls unavailable:'), why)
    b.page("__core.computer.flip('computer')")
    sw = b.page('__core.computer.switchOf(__core.model.page())')
    check('unavailable_hides_nothing_but_says_why: the switch says why and the DAC side is shown',
          sw['unavailable'] == why and sw['side'] == 'dac' and b.page('__core.computer.side()') == 'dac', sw)
    check('unavailable_hides_nothing_but_says_why: the DAC\'s own volume is still read',
          b.page('__core.model.device().volume !== null'))
    check('unavailable_hides_nothing_but_says_why: its targets are the switch alone',
          b.page("import(new URL('assets/core/contract.js', document.baseURI))"
                 ".then((c) => c.computerTargetsOf(__core.model.page()))") == [{'computer': 'flip'}])
    check('unavailable_hides_nothing_but_says_why: a flip posted nothing', b.page('window.__posts.length') == 0)


CARD = "document.querySelector('.ask')"


def key(b, name, extra=''):
    b.page(f"document.activeElement.dispatchEvent(new KeyboardEvent('keydown', {{ key: {json.dumps(name)}, bubbles: true, cancelable: true{extra} }}))")


def question(b, d):
    load(b, d)
    b.page("window.__answer = null, (__core.ask('Warm is on the dac too: flatten the dac and apply on the computer?')"
           ".then((a) => { window.__answer = a; }), 1)")
    card = poll(lambda: b.page(f'!!{CARD}'))
    check('the_question_is_asked_with_keys_and_buttons: the card shows the words and two buttons', card is True)
    info = b.page(f'({{ words: {CARD}.querySelector(".ask-words").textContent,'
                  f' buttons: [...{CARD}.querySelectorAll("button")].map((x) => [x.textContent, x.getBoundingClientRect().height]),'
                  f' focus: document.activeElement.textContent, role: {CARD}.getAttribute("role"),'
                  f' modal: {CARD}.getAttribute("aria-modal"),'
                  f' inert: [...document.body.children].filter((el) => el !== {CARD} && el.id !== "toasts").every((el) => el.inert) }})')
    check('the_question_is_asked_with_keys_and_buttons: words, Yes then No, 44 px, the focus on No',
          info['words'].startswith('Warm is on the dac too') and [x[0] for x in info['buttons']] == ['Yes', 'No']
          and all(x[1] >= 44 for x in info['buttons']) and info['focus'] == 'No'
          and info['role'] == 'dialog' and info['modal'] == 'true' and info['inert'] is True, info)
    key(b, 'Escape')
    check('the_question_is_asked_with_keys_and_buttons: Escape answers No and the card goes',
          poll(lambda: b.page('window.__answer') is False) and b.page(f'!{CARD}'), b.page('window.__answer'))
    check('the_question_is_asked_with_keys_and_buttons: the page is not inert any more',
          b.page('[...document.body.children].every((el) => !el.inert)') is True)
    b.page("(document.body.insertAdjacentHTML('afterbegin', '<button id=\"was\">was</button>'),"
           " document.getElementById('was').focus(), window.__answer = null,"
           " __core.ask('Back?').then((a) => { window.__answer = a; }), 1)")
    poll(lambda: b.page(f'!!{CARD}'))
    key(b, 'n')
    check('the_question_is_asked_with_keys_and_buttons: the focus returns to where it was',
          poll(lambda: b.page("document.activeElement.id") == 'was'), b.page('document.activeElement.id'))
    for how, script, want in [('n', lambda: key(b, 'n'), False), ('y', lambda: key(b, 'y'), True),
                              ('a click on Yes', lambda: b.page(f'{CARD}.querySelector(".ask-yes").click()'), True),
                              ('a click on No', lambda: b.page(f'{CARD}.querySelector(".ask-no").click()'), False)]:
        b.page("window.__answer = null, (__core.ask('Question?').then((a) => { window.__answer = a; }), 1)")
        poll(lambda: b.page(f'!!{CARD}'))
        script()
        got = poll(lambda: b.page('window.__answer') is not None)
        check(f'the_question_is_asked_with_keys_and_buttons: {how} answers {want}',
              got and b.page('window.__answer') is want, b.page('window.__answer'))


def both_sides(b, d):
    load(b, d)
    status, _ = d.post('Gungnir 2', 'presets/recall', {'name': 'Warm'})
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the DAC plays Warm', status == 200, status)
    time.sleep(1.0)
    playing = lambda: b.page(  # noqa: E731
        "JSON.stringify(__core.model.device().presets?.rows?.filter((r) => r.on).map((r) => r.name))")
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: it is on', 'Warm' in (playing() or ''), playing())
    b.page("window.__posts.length = 0, window.__result = null,"
           " (__core.actions.computer.recallPreset('Warm').then((r) => { window.__result = r; }), 1)")
    card = poll(lambda: b.page(f'!!{CARD}'))
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: it asks',
          card is True and 'flatten' in (b.page(f'{CARD}?.querySelector(".ask-words").textContent') or ''), card)
    b.page(f'{CARD}.querySelector(".ask-no").click()')
    poll(lambda: b.page('window.__result') is not None)
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: no sends nothing more and says nothing',
          len(posted(b)) == 1 and posted(b)[0][2] == 409 and b.page('window.__result.declined') is True
          and b.page("document.querySelectorAll('#toasts .toast').length") == 0, posted(b))
    b.page("window.__result = null,"
           " (__core.actions.computer.recallPreset('Warm').then((r) => { window.__result = r; }), 1)")
    poll(lambda: b.page(f'!!{CARD}'))
    b.page(f'{CARD}.querySelector(".ask-yes").click()')
    poll(lambda: b.page('window.__result') is not None)
    log = posted(b)
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: yes repeats with confirm and lands',
          len(log) == 3 and log[2][1].get('confirm') is True and log[2][2] == 200, log)
    flat = poll(lambda: 'Warm' not in (playing() or '') and b.page(f'{COMPUTER}.eq.preset') == 'Warm')
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the DAC is flat, the computer plays it',
          flat is True, [playing(), b.page(f'{COMPUTER}.eq.preset')])
    dac = d.model('Gungnir 2')
    check('the_same_preset_on_both_asks_and_yes_flattens_the_dac: the daemon agrees',
          not any(r.get('on') and r['name'] == 'Warm' for r in dac['presets']['rows']), dac['presets'])


def one(name, fn, env=None, fleet=FLEET):
    d = common.Daemon('gungnir2-v2', host='fake', fleet=fleet, env=env or {})
    b = make()
    try:
        fn(b, d)
        check(f'{name}: no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


one('the_host_event_carries_the_computer', host_event)
one('each_action_reaches_its_route', actions)
one('a_stale_place_still_lands_on_the_point_dragged', slots)
one('a_refusal_toasts_and_rereads', refusal)
one('the_side_is_kept_and_a_flip_writes_nothing', kept_and_silent)
one('unavailable_hides_nothing_but_says_why', unavailable, env={'SVOLDER_FAKE_HOST_EQ': 'off'})
one('the_question_is_asked_with_keys_and_buttons', question)
one('the_same_preset_on_both_asks_and_yes_flattens_the_dac', both_sides)
print(f'computer_core.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
