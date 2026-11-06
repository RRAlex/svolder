# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A theme presents everything a device offers, on every kind of device (design 4.6), in Chromium.
One fake daemon holds the fleet: a Gungnir 2 with a fake player (a DAC), `fake-amp` with a signal,
`fake-five`, `fake-mono` and the Loki Max (slots and settings). For each device, shown by the
page's own device switch, and for every theme the page lists:
  - every target of `targetsOf(device)` (core/contract.js) is reached by the theme's test driver
    and operated, and the model route then reports the change (a readout is not operated: what
    the driver shows equals the model's words); each operation sends one write request;
  - every fact of `factsOf(device)` is reached and shown with the model's words; the cover of the
    track is one (shown as a picture whose address is `api/v1/host/cover?v=<version>`).
The Gungnir 2 is put into Loudness first, so the strength has its target; the Loki Max's slots are
saved over and recalled (a slot's effect is the bands it brings back), the amp's power is turned
off and on again.
Usage: python3 completeness.py [chromium]"""
import json
import os
import re
import sys
import time

import common

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


# While iterating on one theme: WEB_CHECK_THEMES=paper limits the pass to the named themes.
ONLY = set(filter(None, os.environ.get('WEB_CHECK_THEMES', '').split(',')))
FLEET = [('fake-amp', 'Amp', '1112131415161718'), ('fake-five', 'Five', '2122232425262728'),
         ('fake-mono', 'Mono', '3132333435363738'), ('lokimax', 'Loki Max', '4142434445464748')]
DEVICES = ['Gungnir 2', 'Amp', 'Five', 'Mono', 'Loki Max']
# the page records every write it posts
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (!(a[1] && a[1].method === 'POST')) return f(...a);"
        " const entry = [String(a[0]), a[1].body, null]; window.__posts.push(entry);"
        " return f(...a).then((r) => { entry[2] = r.status; return r; }); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; window.__driver = m.core.theme.driver;
  window.__contract = await import(new URL('assets/core/contract.js', document.baseURI));
  return m.core.theme.id; })"""
# how a kind of target is ordered: what changes the mode or the power comes after what needs them
RANK = {'control': 0, 'loudness': 1, 'band': 2, 'preset': 3, 'mode': 4, 'slot': 5, 'setting': 6,
        'transport': 7}


def kind_of(target):
    return next(iter(target)) if 'band' not in target else 'band'


def poll(read, secs=5.0):
    """The first truthy answer of `read()` within `secs`, else the last one."""
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.15)
        got = read()
    return got


def find(items, key, value):
    return next((i for i in items if i.get(key) == value), None)


class Run:
    """One device on the page, and the daemon's model of it."""

    def __init__(self, b, d, name, theme):
        self.b, self.d, self.name, self.theme = b, d, name, theme

    def label(self, target):
        return f'{self.theme} / {self.name} / {json.dumps(target)}'

    def model(self):
        return self.d.model(self.name)

    def reach(self, target):
        return self.b.page(f'__driver.reach({json.dumps(target)})')

    def shown(self, target):
        return self.b.page(f'__driver.shown({json.dumps(target)})')

    def page_shows(self, expression):
        """Wait until the page's own model says `expression`: a gesture reads what the page shows,
        and the page follows the model route by a moment."""
        poll(lambda: self.b.page(expression))
        time.sleep(0.15)

    def writes(self):
        return self.b.page('window.__posts.length')

    def operate(self, target, change):
        """The driver's gesture; the writes it made, from the page."""
        before = self.writes()
        self.b.page(f'__driver.operate({json.dumps(target)}, {json.dumps(change)})')
        return self.writes() - before

    def changes(self, target, change, read, expect, what, writes=1):
        """Operate and wait for the model route to say `expect(before, after)` of `read(model)`."""
        label = self.label(target)
        before = read(self.model())
        made = self.operate(target, change)
        after = poll(lambda: (lambda now: expect(before, now) and now)(read(self.model())))
        after = after or read(self.model())
        check(f'{label}: {what}', expect(before, after), [before, after, change])
        # a write is posted at once; the page may still be answering the last one
        check(f'{label}:   one write request', made == writes, made)


def control(run, target, dev):
    """A control of the device: the volume and its mute, a toggle, a choice, a slider, a readout,
    the power switch and what is under it."""
    key = target['control']
    vol = dev.get('volume')
    power = dev.get('power')
    if vol and key == vol['key']:
        step = 1 if vol['raw'] is not None and vol['raw'] < vol['hi'] and (
            vol['cap'] is None or vol['raw'] < vol['cap']) else -1
        run.changes(target, {'step': step}, lambda m: m['volume']['raw'],
                    lambda a, b: b == a + step, f'a step {step:+d} changes the volume')
        check(f'{run.label(target)}: shows the model\'s words',
              poll(lambda: run.shown(target) == run.model()['volume']['text']),
              [run.shown(target), run.model()['volume']['text']])
    elif vol and key == vol['mute']:
        run.changes(target, {'on': not vol['muted']}, lambda m: m['volume']['muted'],
                    lambda a, b: b is (not a), 'mutes')
        run.page_shows('__core.model.device().volume.muted === ' + json.dumps(not vol['muted']))
        run.changes(target, {'on': vol['muted']}, lambda m: m['volume']['muted'],
                    lambda a, b: b is (not a), 'and unmutes')
    elif power and key == power['key']:
        on = bool(power['on'])
        run.changes(target, {'on': not on}, lambda m: m['power']['on'],
                    lambda a, b: bool(b) is (not on), 'turns the power off' if on else 'turns on')
        run.page_shows('Boolean(__core.model.device().power.on) === ' + json.dumps(not on))
        run.changes(target, {'on': on}, lambda m: m['power']['on'],
                    lambda a, b: bool(b) is on, 'and back')
    else:
        found = find(dev['controls'], 'key', key) or find(
            (power or {}).get('toggles', []) + (power or {}).get('readouts', []), 'key', key)
        kind = found['kind']
        read = lambda m: find(  # noqa: E731
            m['controls'] + (m.get('power') or {}).get('toggles', [])
            + (m.get('power') or {}).get('readouts', []), 'key', key)
        if kind == 'readout':
            words = poll(lambda: run.shown(target) == read(run.model())['text'])
            check(f'{run.label(target)}: shows the model\'s words', words,
                  [run.shown(target), read(run.model())['text']])
        elif kind == 'toggle':
            run.changes(target, {'on': not found['on']}, lambda m: bool(read(m)['on']),
                        lambda a, b: b is (not bool(found['on'])), 'flips')
        elif kind == 'choice':
            option = next(o for o in found['options'] if not o['on'])
            run.changes(target, {'value': option['label']},
                        lambda m: [o['label'] for o in read(m)['options'] if o['on']],
                        lambda a, b: b == [option['label']], f'picks {option["label"]}')
        elif kind == 'slider':
            step = 1 if found['raw'] < found['hi'] else -1
            run.changes(target, {'step': step}, lambda m: read(m)['raw'],
                        lambda a, b: b == a + step, f'a step {step:+d} moves it')
            check(f'{run.label(target)}: shows the model\'s words',
                  poll(lambda: run.shown(target) == read(run.model())['text']),
                  [run.shown(target), read(run.model())['text']])
        else:
            check(f'{run.label(target)}: a kind the checks know', False, kind)


def band(run, target, dev):
    n, param = target['band'], target['param']
    at = {'f': 0, 'g': 1, 'q': 2}[param]
    read = lambda m: m['tone']['readings']['bands'][n - 1][at]  # noqa: E731
    run.changes(target, {'step': 1}, read, lambda a, b: b == a + 1, 'a step +1 moves it')
    check(f'{run.label(target)}: shows the band table\'s words',
          poll(lambda: run.shown(target) == run.model()['tone']['bands']['rows'][n - 1][param]),
          [run.shown(target), run.model()['tone']['bands']['rows'][n - 1][param]])


def loudness(run, target, dev):
    read = lambda m: int(m['tone']['strength'])  # noqa: E731
    run.changes(target, {'step': 1}, read, lambda a, b: b == a + 1, 'a step +1 moves it')
    check(f'{run.label(target)}: shows the strength',
          poll(lambda: run.shown(target) == str(read(run.model()))),
          [run.shown(target), read(run.model())])


def slot(run, target, dev):
    """Save the bands over the slot, change them, recall it: the saved bands are back."""
    n = target['slot']
    mine = f'band{1 + n}'
    key = lambda m: find(m['controls'], 'key', mine)['raw']  # noqa: E731
    base = key(run.model())
    saved, other = base + n, base + n + 5
    check(f'{run.label(target)}: the bands are set for the slot',
          run.d.post(run.name, f'controls/{mine}', {'raw': saved})[0] == 200
          and poll(lambda: key(run.model()) == saved))
    made = run.operate(target, {'save': True})
    check(f'{run.label(target)}: a save is one write request', made == 1, made)
    check(f'{run.label(target)}: the bands are changed by someone else',
          run.d.post(run.name, f'controls/{mine}', {'raw': other})[0] == 200
          and poll(lambda: key(run.model()) == other))
    run.changes(target, {'recall': True}, key, lambda a, b: b == saved, 'a recall brings back '
                'what was saved')


def setting(run, target, dev):
    name = target['setting']
    read = lambda m: find(m['device_presets']['builtins'], 'name', name)['on']  # noqa: E731
    run.changes(target, {'apply': True}, read, lambda a, b: b is True, 'applies')


def preset(run, target, dev):
    name = target['preset']
    read = lambda m: find(m['presets']['rows'], 'name', name)['on']  # noqa: E731
    run.changes(target, {'apply': True}, read, lambda a, b: b is True, 'applies')


def mode(run, target, dev):
    name = target['mode']
    read = lambda m: find(m['tone']['modes'], 'name', name)['on']  # noqa: E731
    run.changes(target, {}, read, lambda a, b: b is True, 'is on')


def transport(run, target, dev):
    """Previous and next change nothing the model holds; the press is the one request, answered."""
    button = target['transport']
    made = run.operate(target, {})
    got = poll(lambda: run.b.page('window.__posts.at(-1)')[2:] != [None] and
               run.b.page('window.__posts.at(-1)'))
    check(f'{run.label(target)}: is one request, answered', made == 1 and got
          and got[0].endswith('api/v1/host/press') and json.loads(got[1]) == {'button': button}
          and got[2] == 200, [made, got])
    if button in ('pause', 'play'):
        want = 'paused' if button == 'pause' else 'playing'
        check(f'{run.label(target)}: the player is {want}',
              poll(lambda: run.model()['playing']['track']['status'] == want),
              run.model()['playing']['track']['status'])


DO = {'control': control, 'band': band, 'loudness': loudness, 'slot': slot, 'setting': setting,
      'preset': preset, 'mode': mode, 'transport': transport}


def shape(words):
    """`words` with every number as `#`: what a reading that moves keeps."""
    return re.sub(r'\d+(\.\d+)?', '#', words)


def facts(run, dev):
    """Every fact is reached and shown with the model's words (a moment is given for a device that
    is still reporting what it was asked to do; a reading that moves on its own, a meter, is held
    to its shape)."""
    labels = run.b.page('__contract.factsOf(__core.model.device())')
    unwritten = {'Product', 'Unit', 'Firmware', 'DSP', 'Profile', 'Link', 'Signal', 'Passphrase'}

    def words(label, f):
        """The words the model gives for the label, and how the page may differ in showing them."""
        if label == 'Product':
            return f['product'] or f'product {f["product_id"]}'
        if label == 'Signal':
            return (f.get('signal_age') or {}).get('head', f['signal'])
        if label == 'Cover':
            return f"api/v1/host/cover?v={run.model()['playing']['track']['cover']}"
        known = {'Unit': f.get('unit'), 'Firmware': f['firmware'],
                 'DSP': '—' if f['dsp'] is None else f['dsp'], 'Profile': f['profile'],
                 'Link': f['link'], 'Passphrase': f.get('passphrase_note')}
        if label in known:
            return known[label]
        return next(r['value'] for r in f['rows'] if r['id'] == label)

    def matches(label):
        f = run.model()['facts']
        want = str(words(label, f))
        shown = run.shown(label)
        if shown is None:
            return False
        if label in unwritten:
            # the page adds words to some (the product's number, the signal's age)
            return shown == want or shown.startswith(want)
        return shown == want or (label in volatile and shape(shown) == shape(want))

    # a reading that changes while the model is looked at is held to its shape
    samples = []
    for _ in range(4):
        samples.append({r['id']: r['value'] for r in run.model()['facts']['rows']})
        time.sleep(0.25)
    volatile = {i for i in samples[0] if len({sample.get(i) for sample in samples}) > 1}
    if dev.get('meters'):
        # an amp's meters move on their own, though they may hold still for a moment
        volatile |= {r['id'] for r in dev['facts']['rows'] if r['label'].startswith('Meter')}
    bad = []
    for label in labels:
        if not run.reach(label):
            bad.append((label, 'not reached'))
        elif not poll(lambda: matches(label), 4.0):
            bad.append((label, run.shown(label), str(words(label, run.model()['facts']))))
    check(f'{run.theme} / {run.name}: all {len(labels)} facts are reached and shown with the '
          'model\'s words', not bad, bad[:6])


FIRST = {}  # per device: the per-kind target counts and the fact count of the first theme listed


def same_counts(run, name, theme, targets):
    """Every theme lists the same targets and facts as the first theme listed."""
    counts = {}
    for target in targets:
        counts[kind_of(target)] = counts.get(kind_of(target), 0) + 1
    counts['facts'] = len(run.b.page('__contract.factsOf(__core.model.device())'))
    first = FIRST.setdefault(name, (theme, counts))
    for kind in sorted(set(counts) | set(first[1])):
        check(f'{theme} / {name}: {kind} count equals {first[0]}\'s',
              counts.get(kind, 0) == first[1].get(kind, 0),
              f'{theme} {counts.get(kind, 0)}, {first[0]} {first[1].get(kind, 0)}')


def one_device(b, d, name, theme):
    url = name.replace(' ', '%20')
    b.page(f"__core.actions.showDevice('{url}').then(() => new Promise((r) => setTimeout(r, 900)))")
    run = Run(b, d, name, theme)
    dev = run.model()
    targets = b.page('__contract.targetsOf(__core.model.device())')
    targets.sort(key=lambda t: RANK[kind_of(t)])
    # the power switch last: a device in standby shows less
    targets.sort(key=lambda t: t.get('control') == (dev.get('power') or {}).get('key'))
    print(f'{theme} / {name}: {len(targets)} targets')
    check(f'{theme} / {name}: the page lists targets', len(targets) > 0)
    same_counts(run, name, theme, targets)
    if name == 'Gungnir 2':
        # the DAC with the host fake has a player to press: transport coverage must not drop
        # silently (the contract lists the buttons only with buttons and a track)
        playing = dev.get('playing') or {}
        shown = [t['transport'] for t in targets if 'transport' in t]
        check(f'{theme} / {name}: the player has buttons and a track', playing.get('buttons') is True
              and bool(playing.get('track')), playing)
        check(f'{theme} / {name}: the transport targets are listed (previous, play or pause, next)',
              len(shown) == 3 and shown[0] == 'previous' and shown[2] == 'next', shown)
        # the fake host carries a cover: the contract lists it, and every theme shows it
        check(f'{theme} / {name}: the cover is a fact (the host\'s picture for the track)',
              'Cover' in b.page('__contract.factsOf(__core.model.device())'), playing.get('track'))
    for target in targets:
        reached = run.reach(target)
        check(f'{run.label(target)}: reached', reached is True, reached)
        if not reached:
            continue
        try:
            DO[kind_of(target)](run, target, run.model())
        except Exception as error:  # a driver that throws fails the target, the run goes on
            check(f'{run.label(target)}: operated', False, str(error)[:300])
    facts(run, run.model())


d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET, env={'SVOLDER_FAKE_SIGNAL': '1'})
b = common.Chromium(size=(1280, 900))
try:
    b.preload(HOOK)
    b.goto(d.url, settle=1.0)
    b.page(BOOT)
    listed = [t['id'] for t in b.page('__core.registry.list()')]
    check('the page lists the themes the checks go through', listed == ['ember', 'paper', 'signal', 'nightdrive'], listed)
    check('WEB_CHECK_THEMES names only listed themes', ONLY <= set(listed), sorted(ONLY - set(listed)))
    for theme in [t for t in listed if not ONLY or t in ONLY]:
        # the Gungnir 2 starts in Loudness, where the strength has its target
        assert d.post('Gungnir 2', 'eq/mode', {'mode': 'loudness'})[0] == 200
        b.goto(f'{d.url}?theme={theme}&effects=off', settle=1.5)
        check(f'{theme}: the page mounts it', b.page(BOOT) == theme)
        for name in DEVICES:
            one_device(b, d, name, theme)
    check('no page error', b.errors == [], b.errors[:3])
finally:
    b.close()
    d.stop()
print(f'completeness.py: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
