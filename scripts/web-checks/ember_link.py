# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Ember's connection state is a dot beside the shown device's name, not a pill by the menu button.
Connected: a green dot right after the name, its word kept for a screen reader and not drawn; the dot
is not a tab stop. Any other state (the device taken off the air): the dot changes colour AND the
state's word is drawn beside the name, so trouble is never shown by colour alone. A desktop and a
phone.
  the_link_is_a_green_dot_beside_the_name           connected: the dot after the name, the word unseen
  the_dot_is_named_and_not_focusable                an image named by the state's words (the computed
                                                    name in Chromium); no tab stop in it
  trouble_shows_its_word_and_changes_the_dot        drop the device: another colour and the word beside the name
  every_state_shows_its_word                        connected, not connected, connecting: each state's word
  the_dot_follows_the_device_shown                  two devices: after a switch the dot is beside the new name
  a_dot_stays_with_a_long_name                      a long device name keeps its dot on the name's own row
  the_muted_volume_reads_minus_infinity             muted the knob reads `−∞ dB`, unmuted the device's step again
Usage: python3 ember_link.py chromium|firefox"""
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
READ = """(() => { const l = document.getElementById('part-link'); if (!l) return null;
  const r = (e) => { const q = e.getBoundingClientRect(); return [q.left, q.right, q.top, q.bottom, q.width, q.height]; };
  const dev = document.querySelector('.devs .dev[aria-current=page], .devs .dev.one');
  const dot = l.querySelector('i'); const word = l.querySelector('.lw');
  const before = dev ? !!(dev.compareDocumentPosition(l) & Node.DOCUMENT_POSITION_FOLLOWING) : null;
  const gap = dev ? l.getBoundingClientRect().left - dev.getBoundingClientRect().right : null;
  const centre = dev ? (l.getBoundingClientRect().top + l.getBoundingClientRect().bottom) / 2 : null;
  const dr = dev ? dev.getBoundingClientRect() : null;
  return { inDevs: !!l.closest('.devs'), after: before, gap, centre, nameRows: dr && [dr.top, dr.bottom],
    state: l.className, text: l.textContent.trim(), dotColour: getComputedStyle(dot).backgroundColor,
    dot: r(dot), wordBox: r(word), wordShown: word.getBoundingClientRect().width > 8
      && getComputedStyle(word).display !== 'none',
    dotHidden: dot.getAttribute('aria-hidden'), tabindex: l.getAttribute('tabindex'),
    inside: [...l.querySelectorAll('*')].some((e) => e.tabIndex >= 0 || e.hasAttribute('tabindex')),
    tabbable: l.tabIndex >= 0 }; })()"""


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(size):
    return common.Chromium(size=size) if browser == 'chromium' else common.Firefox(size=size)


def is_green(colour):
    r, g, b = (int(x) for x in colour[colour.index('(') + 1:colour.index(')')].split(',')[:3])
    return g > r and g > b


def ax(b):
    """The dot's role and computed accessible name: from Chromium's accessibility tree; Firefox has no
    way in from here, so its role and label attributes stand for them."""
    if browser == 'chromium':
        root = b.send('DOM.getDocument', {'depth': 0})['root']['nodeId']
        node = b.send('DOM.querySelector', {'nodeId': root, 'selector': '#part-link'})['nodeId']
        first = b.send('Accessibility.getPartialAXTree', {'nodeId': node, 'fetchRelatives': False})['nodes'][0]
        role = first['role']['value']
        return ['img' if role == 'image' else role, first.get('name', {}).get('value')]
    return b.page("[document.getElementById('part-link').getAttribute('role'),"
                  " document.getElementById('part-link').getAttribute('aria-label')]")


def poll(read, secs=8.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.15)
        got = read()
    return got


for label, size in (('a desktop 1280x800', (1280, 800)), ('a phone 390x844', (390, 844))):
    d = common.Daemon('gungnir2-v2', host='fake')
    b = make(size)
    try:
        b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
        check(f'{label}: Ember mounts', b.page(BOOT) == 'ember')
        time.sleep(0.5)
        up = b.page(READ)
        check(f'the_link_is_a_green_dot_beside_the_name: on {label} the dot follows the shown name, in the name\'s row, '
              'green, and the word `connected` is kept but not drawn',
              up and up['inDevs'] and up['after'] and -1 <= up['gap'] <= 30
              and up['nameRows'][0] <= up['centre'] <= up['nameRows'][1]
              and up['text'] == 'connected' and not up['wordShown'] and up['state'] == 'link on'
              and is_green(up['dotColour']), up)
        good = up['dotColour']
        check(f'the_dot_is_named_and_not_focusable: on {label} its words are its name, the dot is hidden from the '
              'tree and nothing in the link is a tab stop',
              ax(b) == ['img', 'connected'] and up['dotHidden'] == 'true' and up['tabindex'] is None
              and not up['inside'] and not up['tabbable'], [up, ax(b)])
        d.drop()
        down = poll(lambda: (lambda s: s if s and s['text'] != 'connected' else None)(b.page(READ)), 15.0)
        time.sleep(0.4)
        down = b.page(READ)
        check(f'trouble_shows_its_word_and_changes_the_dot: on {label} not connected shows its word beside the name '
              'and the dot is another colour',
              down and down['text'] != 'connected' and down['wordShown'] and down['dotColour'] != good
              and down['after'] and down['gap'] <= 30, [good, down])
        check(f'trouble_shows_its_word_and_changes_the_dot: on {label} the name follows the state', ax(b) == ['img', down['text']], ax(b))
        errs = b.errors if browser == 'chromium' else b.collect_errors()
        check(f'  no page error on {label}', [e for e in errs if 'favicon' not in e] == [], errs[:3])
    finally:
        b.close()
        d.stop()

STATES = [('off', 'not connected'), ('wait', 'connecting\u2026'), ('on', 'connected')]
SET = """(() => { const url = __core.model.shown(); const d = structuredClone(__core.model.page().device);
  __core.model.applyPart(url, 'link', { state: %s, word: %s }); return 1; })()"""
FLEET = [('fake-amp', 'Amp', '1112131415161718')]
PLACE = """(() => { const l = document.getElementById('part-link');
  const row = document.querySelector('.devs .dev[aria-current=page], .devs .dev.one');
  const r = (e) => { const q = e.getBoundingClientRect(); return [q.left, q.right, q.top, q.bottom]; };
  return { sameRow: !!row && l.parentElement === row.parentElement, name: row && row.querySelector('b').textContent,
    link: r(l), dev: row && r(row), word: l.textContent.trim(),
    shown: !!l.querySelector('.lw') && l.querySelector('.lw').getBoundingClientRect().width > 8 }; })()"""
import json  # noqa: E402

d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET)
b = make((1280, 800))
try:
    b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
    b.page(BOOT)
    time.sleep(0.5)
    # every state's word, and the dot's colour against the connected one
    colours = {}
    for state, word in STATES:
        b.page(SET % (json.dumps(state), json.dumps(word)))
        got = poll(lambda: (lambda g: g if g and g['text'] == word else None)(b.page(READ)), 5.0)
        shown_ok = bool(got) and (got['wordShown'] is (state != 'on'))
        check(f'every_state_shows_its_word: `{word}` is {"unseen" if state == "on" else "drawn"} beside the name, '
              f'class link {state}', shown_ok and got['state'] == f'link {state}', got)
        if got:
            colours[state] = got['dotColour']
        check(f'  `{word}` is the dot\'s accessible name', ax(b) == ['img', word], ax(b))
    check('every_state_shows_its_word: the three states have three colours or the two troubles share one that '
          'is not the connected green', colours.get('off') == colours.get('wait') != colours.get('on'), colours)
    # two devices: the dot follows the one shown
    before = b.page(PLACE)
    check('the_dot_follows_the_device_shown: with Gungnir 2 shown the dot is beside its name',
          before['sameRow'] and before['name'] == 'Gungnir 2', before)
    b.page("__core.actions.showDevice('Amp')")
    after = poll(lambda: (lambda g: g if g['name'] == 'Amp' and g['sameRow'] else None)(b.page(PLACE)), 8.0)
    check('the_dot_follows_the_device_shown: after a switch the dot is in the row of the new name, level with it',
          bool(after) and after['dev'][0] <= after['link'][0] and after['link'][2] >= after['dev'][2] - 2
          and after['link'][3] <= after['dev'][3] + 2, [before, after])
    b.page(SET % (json.dumps('off'), json.dumps('not connected')))
    trouble = poll(lambda: (lambda g: g if g['word'] == 'not connected' and g['shown'] else None)(b.page(PLACE)), 5.0)
    check('the_dot_follows_the_device_shown: and the word of its state is drawn beside the new name',
          bool(trouble) and trouble['sameRow'] and trouble['name'] == 'Amp', trouble)
    b.page("__core.actions.showDevice('Gungnir%202')")
    back = poll(lambda: (lambda g: g if g['name'] == 'Gungnir 2' and g['sameRow'] else None)(b.page(PLACE)), 8.0)
    check('the_dot_follows_the_device_shown: and back to the first', bool(back), back)
    # muted reads `−∞ dB`, as Signal and Night drive say it
    big = "document.querySelector('sv-knob .big').textContent"
    step = poll(lambda: b.page(big) != '', 3.0) and b.page(big)
    d.post('Gungnir 2', 'controls/mute', {'value': True})
    muted = poll(lambda: b.page(big) == '\u2212\u221edB', 6.0)
    check('the_muted_volume_reads_minus_infinity: muted, the knob reads `\u2212\u221e dB`', bool(muted), b.page(big))
    d.post('Gungnir 2', 'controls/mute', {'value': False})
    back = poll(lambda: b.page(big) == step, 6.0)
    check('  unmuted, it reads the device\'s step again', bool(back), [step, b.page(big)])
    errs = b.errors if browser == 'chromium' else b.collect_errors()
    check('  no page error with two devices', [e for e in errs if 'favicon' not in e] == [], errs[:3])
finally:
    b.close()
    d.stop()
d = common.Daemon('gungnir2-v2', host='fake', name='A device with a very long name that keeps going and going')
b = make((360, 780))
try:
    b.goto(f'{d.url}?theme=ember&effects=off', settle=1.0)
    b.page(BOOT)
    time.sleep(0.5)
    got = b.page(PLACE)
    check('a_dot_stays_with_a_long_name: at 360 px a long name keeps its dot in the name\'s own row, beside it',
          got['sameRow'] and got['link'][2] >= got['dev'][2] - 2 and got['link'][3] <= got['dev'][3] + 2, got)
finally:
    b.close()
    d.stop()
print(f'ember_link.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
