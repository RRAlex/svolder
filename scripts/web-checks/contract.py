# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The contract every theme passes (design section 8), in one browser. One short pass for each theme
the page lists (`svolder-themes`), driven only through the theme's test driver (`theme.driver`:
reach, operate, shown, state), judged by what the fake daemon holds and what the theme shows, never
by pixels or positions:
  volume_up_and_down, mute, a_band_change, a_preset_applied, the_device_lost_and_back,
  the_link_state_is_in_words_beside_the_name (the driver's `linkWords()`),
  a_refused_write (a write the daemon refuses, its words shown), resting_and_waking,
  the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade,
  theme_query_switches_and_is_kept, and the same pass as a guest (every operate sends no request
  and toasts "Sign in to change this").
Also: the core's members are pinned (the core object has these and no others), and the menu button
is shown, big enough, on top and inside the window in every theme and on every kind of device, at
a desktop and a phone size.
Usage: python3 contract.py chromium|firefox"""
import json
import os
import sys
import time

import common

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


# While iterating on one theme: WEB_CHECK_THEMES=paper limits the pass to the named themes.
ONLY = set(filter(None, os.environ.get('WEB_CHECK_THEMES', '').split(',')))
FLEET = [('fake-amp', 'Amp', '1112131415161718'), ('fake-five', 'Five', '2122232425262728'),
         ('fake-mono', 'Mono', '3132333435363738'), ('lokimax', 'Loki Max', '4142434445464748')]
# the members of `core`, which every theme may rely on and none may find missing
CORE = ['actions', 'analysis', 'ask', 'climb', 'clock', 'computer', 'curve', 'drag', 'frame', 'hold',
        'keys', 'lifecycle', 'model', 'prefs', 'registry', 'scheme', 'stream', 'theme', 'title',
        'toneNote', 'words']
COMPUTER_TARGETS = ("import(new URL('assets/core/contract.js', document.baseURI))"
                    ".then((c) => c.computerTargetsOf(__core.model.page()))")
HOOK = ("window.__posts = []; const f = window.fetch; window.fetch = (...a) => {"
        " if (a[1] && a[1].method === 'POST') window.__posts.push([String(a[0]), a[1].body]);"
        " return f(...a); };")
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; window.__driver = m.core.theme.driver;
  return m.core.theme.id; })"""
# the page's visibility is a switch the check holds (the stream rests when it is hidden)
HIDE = """(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
  Object.defineProperty(document, 'visibilityState', { configurable: true,
    get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); })()"""
SHOW = """(() => { delete document.hidden; delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange')); })()"""
BUTTON = """(() => { const b = document.getElementById('fx-button');
  const box = b.getBoundingClientRect(); const css = getComputedStyle(b);
  const at = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
  return { display: css.display, visibility: css.visibility, opacity: css.opacity,
    faded: b.classList.contains('faded'), size: [box.width, box.height],
    top: at === b || b.contains(at), count: document.querySelectorAll('#fx-button').length,
    inside: box.left >= 0 && box.top >= 0 && box.right <= innerWidth + 0.5
      && box.bottom <= innerHeight + 0.5 }; })()"""


def poll(read, secs=5.0):
    """The first truthy answer of `read()` within `secs`, else the last one."""
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def make(size=(1280, 900), **kw):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size, **kw)
    b.preload(HOOK)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def listed(b):
    return [t['id'] for t in b.page('__core.registry.list()')]


def start(b, daemon, theme, query=''):
    """Load the page with `?theme=` and wait for the theme to be on show."""
    b.goto(f'{daemon.url}?theme={theme}&effects=off{query}', settle=1.0)
    mounted = b.page(BOOT)
    check(f'{theme}: the page mounts the theme', mounted == theme, mounted)
    time.sleep(0.5)


class Pass:
    """One theme on one page of a daemon, driven by its test driver."""

    def __init__(self, b, daemon, theme, name='Gungnir 2'):
        self.b, self.d, self.theme, self.name = b, daemon, theme, name

    def tag(self, check_name):
        return f'{check_name} ({self.theme})'

    def reach(self, target):
        return self.b.page(f'__driver.reach({json.dumps(target)})')

    def operate(self, target, change):
        self.b.page(f'__driver.operate({json.dumps(target)}, {json.dumps(change)})')

    def shown(self, target):
        return self.b.page(f'__driver.shown({json.dumps(target)})')

    def state(self):
        return self.b.page('__driver.state()')

    def link_words(self):
        return self.b.page('__driver.linkWords()')

    def model(self):
        return self.d.model(self.name)

    def posts(self):
        return self.b.page('window.__posts.length')


def volume_and_mute(p):
    vol = p.model()['volume']
    check(p.tag('volume_up_and_down: the volume is reached'),
          p.reach({'control': 'volume'}) is True)
    p.operate({'control': 'volume'}, {'step': 1})
    up = poll(lambda: p.model()['volume']['raw'] == vol['raw'] + 1)
    check(p.tag('volume_up_and_down: a step up raises the daemon\'s volume.raw by one'), up,
          [vol['raw'], p.model()['volume']['raw']])
    p.operate({'control': 'volume'}, {'step': -1})
    down = poll(lambda: p.model()['volume']['raw'] == vol['raw'])
    check(p.tag('volume_up_and_down: and a step down lowers it again'), down,
          [vol['raw'], p.model()['volume']['raw']])
    check(p.tag('volume_up_and_down: the theme shows the daemon\'s words'),
          poll(lambda: p.shown({'control': 'volume'}) == p.model()['volume']['text']),
          [p.shown({'control': 'volume'}), p.model()['volume']['text']])
    check(p.tag('mute: the mute is reached'), p.reach({'control': 'mute'}) is True)
    p.operate({'control': 'mute'}, {'on': True})
    check(p.tag('mute: it mutes at the daemon'), poll(lambda: p.model()['volume']['muted']))
    poll(lambda: p.b.page('__core.model.device().volume.muted'))
    time.sleep(0.2)
    p.operate({'control': 'mute'}, {'on': False})
    check(p.tag('mute: and unmutes'), poll(lambda: not p.model()['volume']['muted']))


def tone(p):
    check(p.tag('a_band_change: band 2 gain is reached'),
          p.reach({'band': 2, 'param': 'g'}) is True)
    before = p.model()['tone']['readings']['bands'][1][1]
    p.operate({'band': 2, 'param': 'g'}, {'step': 1})
    check(p.tag('a_band_change: a step of band 2\'s gain moves it by one at the daemon'),
          poll(lambda: p.model()['tone']['readings']['bands'][1][1] == before + 1),
          [before, p.model()['tone']['readings']['bands'][1][1]])
    p.operate({'band': 2, 'param': 'g'}, {'step': -1})
    check(p.tag('a_band_change: and a step down brings it back'),
          poll(lambda: p.model()['tone']['readings']['bands'][1][1] == before),
          [before, p.model()['tone']['readings']['bands'][1][1]])
    check(p.tag('a_band_change: the theme shows the band table\'s words'),
          poll(lambda: p.shown({'band': 2, 'param': 'g'})
               == p.model()['tone']['bands']['rows'][1]['g']),
          [p.shown({'band': 2, 'param': 'g'}), p.model()['tone']['bands']['rows'][1]['g']])
    check(p.tag('a_preset_applied: Warm is reached'), p.reach({'preset': 'Warm'}) is True)
    p.operate({'preset': 'Warm'}, {'apply': True})
    check(p.tag('a_preset_applied: Warm is on at the daemon'),
          poll(lambda: p.model()['presets']['status']['name'] == 'Warm'),
          p.model()['presets']['status'])


def refused_write(p):
    """The daemon refuses a write (the device is out of reach and the page, its stream closed, does
    not know yet): the theme shows the daemon's own words and nothing changes."""
    vol = p.model()['volume']
    check(p.tag('a_refused_write: the volume is reached'),
          p.reach({'control': 'volume'}) is True)
    # the page does not hear that the link went, so the step is sent and refused
    p.b.page('__core.stream.close()')
    p.d.drop()
    words = None
    for _ in range(60):
        status, body = p.d.post(p.name, 'volume', {'raw': vol['raw'] - 1})
        if status == 409:
            words = body['error']
            break
        time.sleep(0.1)
    check(p.tag('a_refused_write: the daemon refuses the write with words'), words, [status, body])
    vol = p.model()['volume']
    p.operate({'control': 'volume'}, {'step': -1})
    got = poll(lambda: words in p.state()['toasts'])
    check(p.tag('a_refused_write: the daemon\'s words are shown'), got, [words, p.state()])
    check(p.tag('a_refused_write: nothing changed'), p.model()['volume']['raw'] == vol['raw'],
          [vol['raw'], p.model()['volume']['raw']])


def resting_and_waking(p):
    want = p.model()['volume']['raw'] - 3
    p.b.page(HIDE)
    check(p.tag('resting_and_waking: the page rests'), poll(lambda: p.state()['resting'], 6.0),
          p.state())
    status, _ = p.d.post(p.name, 'volume', {'raw': want})
    check(p.tag('resting_and_waking: a change is made meanwhile'),
          status == 200 and poll(lambda: p.model()['volume']['raw'] == want), status)
    p.b.page(SHOW)
    woke = time.time()
    shown = poll(lambda: p.shown({'control': 'volume'}) == p.model()['volume']['text'], 2.0)
    check(p.tag('resting_and_waking: the change shows within 2 s of waking'), shown,
          [p.shown({'control': 'volume'}), p.model()['volume']['text'], time.time() - woke])
    check(p.tag('resting_and_waking: and the page is no longer resting'),
          poll(lambda: not p.state()['resting']) and not p.state()['waiting'], p.state())


def computer_pass(p):
    """The computer's side, through the driver only: every target of `computerTargetsOf` is reached
    (the switch first, then the side flipped to the computer so the rest can be), and a flip writes
    nothing."""
    targets = poll(lambda: p.b.page(COMPUTER_TARGETS))
    check(p.tag('the computer targets are listed (the switch, the volume, the tone)'),
          bool(targets) and targets[0] == {'computer': 'flip'} and len(targets) > 5, targets)
    posts = p.posts()
    p.operate({'computer': 'flip'}, {'side': 'computer'})
    time.sleep(0.4)
    check(p.tag('a flip posts nothing'), p.posts() == posts, p.posts())
    # the list is read again before each target: a remove or a preset changes what follows it
    done = []
    while True:
        target = next((t for t in p.b.page(COMPUTER_TARGETS) if t not in done), None)
        if target is None:
            break
        done.append(target)
        check(p.tag(f'computer {json.dumps(target)}: reached'), p.reach(target) is True)
        # a preset that two sides would play twice asks: the driver's answer here is No
        p.b.page("document.querySelector('.ask .ask-no')?.click() ?? null")
    p.operate({'computer': 'flip'}, {'side': 'dac'})
    check(p.tag('and flipping back posts nothing either'), p.posts() == posts, p.posts())


def menu_and_picker(p):
    b = p.b
    b.page('window.scrollTo(0, 0)')
    button = b.page(BUTTON)
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'one button'), button['count'] == 1, button)
    b.press('T')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'T opens the picker'), poll(lambda: b.page("!!document.getElementById('picker')")))
    b.press('Escape')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'Escape closes it'), poll(lambda: not b.page("!!document.getElementById('picker')")))
    # scrolled down, the button out of view (or pinned): T still opens the picker
    b.page('window.scrollTo(0, document.documentElement.scrollHeight)')
    time.sleep(0.3)
    away = b.page("(() => { const r = document.getElementById('fx-button').getBoundingClientRect();"
                  " return { y: scrollY, scrolls: document.documentElement.scrollHeight > innerHeight,"
                  " out: r.bottom <= 0 || r.top >= innerHeight,"
                  " pinned: Math.abs(r.top - 8) <= 1 }; })()")
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'the page is scrolled and the button is out of view, or pinned to the window'),
          (away['y'] > 0 or not away['scrolls']) and (away['out'] or away['pinned']), away)
    b.press('T')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'T opens the picker mid-page'),
          poll(lambda: b.page("!!document.getElementById('picker')")))
    b.press('Escape')
    poll(lambda: not b.page("!!document.getElementById('picker')"))
    b.page('window.scrollTo(0, 0)')
    # a theme that fills the screen asks for the fade: the button goes after 2 s, T still works
    b.page('__core.frame.fade(true)')
    faded = poll(lambda: b.page(BUTTON)['faded'], 5.0)
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'the button fades'), faded, b.page(BUTTON))
    b.press('T')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'T opens the picker with the button faded'),
          poll(lambda: b.page("!!document.getElementById('picker')")))
    b.press('Escape')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'and the key brought the button back'), not b.page(BUTTON)['faded'],
          b.page(BUTTON))
    b.page('__core.frame.fade(false)')
    check(p.tag('the_menu_button_is_present_and_t_opens_the_picker_also_after_a_fade: '
                'no page error'), errors(b) == [], errors(b)[:3])


def the_query(p, themes):
    """`?theme=` mounts the theme and its cookie keeps it for the page without the query."""
    b = p.b
    for theme in themes:
        b.goto(f'{p.d.url}?theme={theme}&effects=off', settle=1.0)
        mounted = b.page(BOOT)
        cookie = b.page('document.cookie')
        check(f'theme_query_switches_and_is_kept: ?theme={theme} mounts it and keeps '
              f'svolder_theme={theme}', mounted == theme and f'svolder_theme={theme}' in cookie,
              [mounted, cookie])
        b.goto(f'{p.d.url}?effects=off', settle=1.0)
        check(f'theme_query_switches_and_is_kept: the page without the query shows {theme}',
              b.page(BOOT) == theme and b.page('document.documentElement.dataset.theme') == theme)


def button_everywhere(b, d, theme):
    """The menu button on every kind of device, desktop and phone: shown, big enough, on top, in
    the window."""
    for size in ((1280, 900), (390, 844)):
        b.resize(*size)
        for name in ['Gungnir 2', 'Amp', 'Five', 'Mono', 'Loki Max']:
            url = name.replace(' ', '%20')
            b.page(f"__core.actions.showDevice('{url}').then("
                   "() => new Promise((r) => setTimeout(r, 700)))")
            # a person arrives at the top of the page (Ember's button sits in its header)
            b.page('window.scrollTo(0, 0)')
            time.sleep(0.2)
            got = b.page(BUTTON)
            check(f'{theme} / {name} / {size[0]} x {size[1]}: the menu button is shown, at least '
                  '24 px, on top and inside the window',
                  got['display'] != 'none' and got['visibility'] == 'visible'
                  and got['opacity'] == '1' and not got['faded'] and min(got['size']) >= 24
                  and got['top'] and got['inside'] and got['count'] == 1, got)
    b.resize(1280, 900)


def lost_and_back(theme):
    """The device goes out of reach and comes back: the theme shows the lost state, then the
    reading again."""
    b = make()
    daemon = common.Daemon('gungnir2-v2')
    try:
        b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
        b.page(BOOT)
        time.sleep(0.5)
        p = Pass(b, daemon, theme)
        check(p.tag('the_device_lost_and_back: not lost to begin with'), not p.state()['lost'])
        before = p.link_words()
        daemon.drop()
        # the state is told in words beside the device's name (a mark alone, or a colour, is not enough)
        word = poll(lambda: (p.model().get('link') or {}).get('word') not in (None, 'connected')
                    and p.model()['link']['word'], 10.0)
        check(p.tag('the_link_state_is_in_words_beside_the_name: a device out of reach has words beside its '
                    'name that name its state (the model\'s link word)'),
              bool(word) and poll(lambda: word.lower() in (p.link_words() or '').lower(), 10.0)
              and word.lower() not in (before or '').lower(), [before, p.link_words(), word])
        check(p.tag('the_device_lost_and_back: the theme shows the device lost'),
              poll(lambda: p.state()['lost'], 10.0), p.state())
        daemon.restart()
        check(p.tag('the_device_lost_and_back: and not lost again when it is back'),
              poll(lambda: not p.state()['lost'], 30.0), p.state())
        check(p.tag('the_device_lost_and_back: the volume is shown again with the daemon\'s '
                    'words'), poll(lambda: p.shown({'control': 'volume'})
                                   == p.model()['volume']['text'], 10.0),
              [p.shown({'control': 'volume'}), p.model()['volume']['text']])
    finally:
        b.close()
        daemon.stop()


def guest_pass(theme):
    """A guest's operate sends nothing and says to sign in."""
    b = make()
    daemon = common.Daemon('gungnir2-v2', web='guest = "read"')
    began = time.time()
    try:
        b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
        b.page(BOOT)
        time.sleep(0.5)
        p = Pass(b, daemon, theme)
        steps = [({'control': 'volume'}, {'step': 1}), ({'control': 'mute'}, {'on': True}),
                 ({'band': 2, 'param': 'g'}, {'step': 1}), ({'preset': 'Warm'}, {'apply': True}),
                 ({'mode': 'eq'}, {})]
        for target, change in steps:
            reached = p.reach(target)
            empty = poll(lambda: p.state()['toasts'] == [], 8.0)
            check(p.tag(f'guest {json.dumps(target)}: the toasts are empty before it'), empty,
                  p.state()['toasts'])
            if not empty:
                continue
            posts = p.posts()
            p.operate(target, change)
            said = poll(lambda: 'Sign in to change this' in p.state()['toasts'], 3.0)
            check(p.tag(f'guest {json.dumps(target)}: reached, operate sends no request and toasts '
                        '"Sign in to change this"'),
                  reached is True and said and p.posts() == posts, [reached, p.state(), posts,
                                                                    p.posts()])
        check(p.tag('guest: nothing changed at the daemon'),
              p.model()['volume']['muted'] is False)
        check(p.tag('the guest pass takes under 5 minutes'), time.time() - began < 300,
              time.time() - began)
    finally:
        b.close()
        daemon.stop()


def refused_pass(theme):
    """A write the daemon refuses."""
    b = make()
    daemon = common.Daemon('gungnir2-v2')
    try:
        b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
        b.page(BOOT)
        time.sleep(0.5)
        refused_write(Pass(b, daemon, theme))
    finally:
        b.close()
        daemon.stop()


fleet = None
try:
    fleet = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET, env={'SVOLDER_FAKE_SIGNAL': '1'})
    b = make()
    b.goto(f'{fleet.url}?effects=off', settle=1.0)
    b.page(BOOT)
    themes = [t for t in listed(b) if not ONLY or t in ONLY]
    check('WEB_CHECK_THEMES names only listed themes', ONLY <= set(listed(b)), sorted(ONLY - set(listed(b))))
    check('the page lists a theme', len(themes) > 0, themes)
    members = sorted(b.page('Object.keys(__core)'))
    check('the core has these members and no others', members == CORE, members)
    for theme in themes:
        began = time.time()
        start(b, fleet, theme)
        p = Pass(b, fleet, theme)
        check(p.tag('the theme has a test driver'),
              b.page("['reach', 'operate', 'shown', 'state'].every("
                     "(name) => typeof __driver[name] === 'function')") is True)
        volume_and_mute(p)
        tone(p)
        computer_pass(p)
        menu_and_picker(p)
        button_everywhere(b, fleet, theme)
        start(b, fleet, theme)
        resting_and_waking(Pass(b, fleet, theme))
        check(p.tag('no page error'), errors(b) == [], errors(b)[:3])
        lost_and_back(theme)
        refused_pass(theme)
        guest_pass(theme)
        check(p.tag('the pass takes under 5 minutes'), time.time() - began < 300,
              time.time() - began)
    if themes:
        the_query(Pass(b, fleet, themes[0]), themes)
    b.close()
finally:
    if fleet:
        fleet.stop()
print(f'contract.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
