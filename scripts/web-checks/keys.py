# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A listed key is bound and a bound global key is listed, in every theme. Each theme the page
lists gives the frame a key table (`core.frame.keyTable()`, core/keys.js): this check reads it in
the browser and holds the page to it, on a fake DAC with a player and a second device beside it (a
writer), on a DAC with a player for a guest, and on one with no player (Ember then has no
visualizer card).

  the_table_is_the_mounted_instances_and_every_entry_is_whole   the frame's table is the one the
        theme's mount returned (the theme object gives none), each entry with an id, its keycaps, a
        zone and words
  every_page_key_that_applies_is_really_bound    each key of each `run` entry of zone `page` that
        `core.keys.listed` gives is pressed with the real keyboard: its `run` is called once (a
        spy laid over the entry's `run`: the binding calls it through the entry), and where the
        page's answer is cheap to read it is read too (REACTS below says which keys got which)
  every_own_key_of_the_page_is_really_answered   each key of each entry of zone `page` marked
        `own` (a listener of the theme's own answers it, not the binding: no theme has one now)
        is pressed and the page's answer is read (OWNED below); an `own` key of a widget is pressed in the
        state that shows the widget (Ember's Escape in Arrange mode, and its Escape that ends a
        drag, with the mouse really pressed on a point of the curve and held)
  a_key_with_no_entry_does_nothing_global        keys no entry of zone `page` names are pressed:
        no `run` is called and the page stays as it was (the volume, the mute, the tone mode,
        the focus, what stands over the page, the address)
  an_entry_whose_when_is_false_is_neither_bound_nor_listed   a guest's keys that write, and
        Ember's `v` on a page with no visualizer card: not in `listed`, and pressing it calls
        no `run` and leaves the page as it was
  every_widget_entrys_zone_matches_an_element    every entry of a widget's zone is seen on the
        page once, listed and with an element its selector matches: on the page as it opens, or
        in one of the states STATES below puts it in (a list open, the sheet open, Arrange mode),
        so a renamed class cannot orphan a line of the Help popup
  the_key_listeners_on_the_window_and_the_document_are_the_allowed_ones   `addEventListener` of
        the window and of the document is wrapped before the page's scripts run, and every
        listener for `keydown`, `keyup` or `keypress` is filed under the script that added it
        (the caller in its stack) for as long as it is on. Through the whole run of a theme (its
        mount, every press, every state of STATES) a file of the theme never has more of them
        on at once than `LISTENERS` of `scripts/theme-css-check.py` gives it lines for that
        event, `core/keys.js` has the binding's one for a table with a `run` entry and none
        for a table without, and none is on whose adder is unknown (no script of the page in
        its stack: added from a timer or through `bind`). The method wrapped is
        `EventTarget.prototype.addEventListener`, which the window's and the document's own
        name, `.call(window, ...)` and `Reflect.apply` all end in. So a listener the static
        rule cannot see fails here. What this cannot see: a handler property (`window.onkeydown
        = ...`; the static rule reads those); a listener added with another realm's method (an
        iframe's `EventTarget.prototype.addEventListener.call(window, ...)`: no theme makes a
        frame); a listener on another node that hears every key of the page all the same
        (`document.body`, `document.documentElement`: only the window and the document are
        counted, as `LISTENERS` lists only those); and a listener a theme adds only in a state
        the check never enters

  help_shows_the_tables_and_nothing_else         the Help popup cannot drift from the tables
        the cases above hold: on each theme's page as it opens (Help opened with `?`, the real
        key, for a writer and a guest) and in every state of STATES (opened through the router,
        since a field or a theme's own sheet may own the key there), every row of Here names an
        entry `core.keys.listed` gives now, of the theme's table or of the frame's list; the
        rows under one heading are every listed entry of the zones that hold the control with
        the focus (`core.keys.zonesOf` of `document.activeElement`, read before the open and
        after it: a widget that closes when it loses the focus is gone by then), every `own` entry of another widget, and the page's keys with the
        frame's; each heading is the theme's name for the zone and no selector; and Everywhere
        is the page's keys with the frame's

The static half is `make theme-css-check`, rule 5: a key a theme's script compares a key press
with is in the theme's table.
Usage: python3 keys.py chromium|firefox"""
import importlib.util
import json
import os
import sys
import time

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()

# The lines a theme's files may have a key listener on the window or the document for.
_spec = importlib.util.spec_from_file_location(
    'theme_css_check', os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'theme-css-check.py'))
_rules = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_rules)
ALLOWED = {}
for _file, _event, _entry, _why in _rules.LISTENERS:
    ALLOWED[f'{_file} {_event}'] = ALLOWED.get(f'{_file} {_event}', 0) + 1

# Before the page's scripts: every key listener added to the window or the document is filed
# under the script that added it (the first script of the page in the adder's stack) while it is
# on, and `__keyListeners.peak` keeps the most each file had on at once, by `<file> <event>`.
# The method wrapped is the one every way of adding a listener in this page's realm ends in,
# `EventTarget.prototype.addEventListener` (the window and the document have none of their own),
# so `window.addEventListener(...)`, a bare `addEventListener(...)`, `.call(window, ...)` and
# `Reflect.apply` are all counted; a listener added with no script of the page in its stack (from
# a timer, through `bind`) is filed under `unknown`.
COUNT = """(() => { const KEYS = ['keydown', 'keyup', 'keypress'];
  const live = new Map(); const peak = {}; const on = [];
  const file = () => { const at = /\\/assets\\/([^\\s:?)]+\\.js)/.exec(new Error().stack ?? '');
    return at ? at[1] : 'unknown'; };
  const bump = (name, by) => { const now = (live.get(name) ?? 0) + by; live.set(name, now);
    peak[name] = Math.max(peak[name] ?? 0, now); };
  const mine = (target) => target == null || target === window || target === document;
  {
    const proto = EventTarget.prototype;
    const add = proto.addEventListener; const remove = proto.removeEventListener;
    proto.addEventListener = function counted(type, fn, options) {
      if (mine(this) && KEYS.includes(type) && fn) {
        const capture = options === true || options?.capture === true;
        if (!on.some((one) => one.type === type && one.fn === fn && one.capture === capture)) {
          const one = { type, fn, capture, name: `${file()} ${type}`, off: false };
          const drop = () => { if (one.off) return; one.off = true; on.splice(on.indexOf(one), 1);
            bump(one.name, -1); };
          on.push(one); bump(one.name, 1);
          one.drop = drop;
          options?.signal?.addEventListener('abort', drop);
          if (options?.once === true) {
            const run = fn; const wrapped = function once(...args) { drop();
              return (typeof run === 'function' ? run : run.handleEvent).apply(this, args); };
            one.wrapped = wrapped;
            return add.call(this, type, wrapped, options);
          }
        }
      }
      return add.call(this, type, fn, options);
    };
    proto.removeEventListener = function counted(type, fn, options) {
      const capture = options === true || options?.capture === true;
      const one = !mine(this) ? null
        : on.find((each) => each.type === type && each.fn === fn && each.capture === capture);
      if (one) { one.drop(); if (one.wrapped) return remove.call(this, type, one.wrapped, options); }
      return remove.call(this, type, fn, options);
    };
  }
  window.__keyListeners = { peak, now: () => Object.fromEntries([...live].filter(([, n]) => n > 0)) };
})();"""
PEAK = "({ peak: window.__keyListeners?.peak ?? null, now: window.__keyListeners?.now() ?? null })"
# whether the table the theme gave has an entry the binding dispatches
BINDS = "__core.frame.keyTable().some((e) => typeof e.run === 'function' || e.__spied === true)"

BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
LISTED = "__core.registry.list().map((t) => t.id)"
# The table as the frame gives it, each entry with whether it applies now and whether an element
# of the page is in its zone.
TABLE = """(() => { const table = __core.frame.keyTable(); const now = __core.keys.listed(table);
  const text = (v) => typeof v === 'string' && v !== '';
  const seen = (zone) => { try { return document.querySelector(zone) !== null; } catch (e) { return 'bad selector'; } };
  return { mine: __core.mounted !== null && __core.mounted.keys === table && __core.theme.keys === undefined,
    entries: table.map((e, i) => ({ i, id: e.id ?? null, key: e.key, keys: e.keys ?? [].concat(e.key),
      zone: e.zone, words: e.words, run: typeof e.run === 'function', own: e.own === true,
      whole: text(e.id) && [].concat(e.key).every(text) && text(e.zone) && text(e.words),
      listed: now.includes(e),
      here: e.zone === 'page' ? true : seen(e.zone) })) }; })()"""
# Lay a spy over every `run` of the table (once a page): `__ran` holds the entries called.
SPY = """(() => { window.__ran = []; const table = __core.frame.keyTable();
  table.forEach((e, i) => { if (typeof e.run !== 'function' || e.__spied) return;
    const run = e.run; e.__spied = true;
    e.run = function spied(event) { window.__ran.push(i); return run.call(this, event); }; });
  return table.length; })()"""
RAN = "(() => { const ran = window.__ran; window.__ran = []; return ran; })()"
# What a key of the whole page could change, read before and after a press.
STATE = """(() => { const d = __core.model.device(); const a = document.activeElement;
  return { raw: d?.volume?.raw ?? null, muted: d?.volume?.muted ?? null, mode: d?.tone?.mode ?? null,
    focus: a && a !== document.body ? (a.id || a.getAttribute('class') || a.tagName) : null,
    over: document.querySelectorAll('[aria-modal="true"], #picker').length,
    menu: !document.getElementById('fx-menu').hidden, address: location.search,
    help: !!document.getElementById('popup-help'),
    held: !!document.querySelector('.plot.grabbing'),
    arranging: !!document.querySelector('.view.arranging') }; })()"""
BLUR = "(() => { document.activeElement?.blur?.(); window.scrollTo(0, 0); return 1; })()"

# The keys pressed to find one that acts without an entry: letters and digits no theme lists, the
# other themes' own page keys, and keys a widget has that the page must not. `T`, `?` and Space
# are the frame's (frame-keys.mjs, space_key.py, modal.py hold those).
STRAY = ['q', 'x', 'j', 'k', 'z', '9', 'm', 'f', 'p', 'v', 'a', '0', 'ArrowUp', 'ArrowDown',
         'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown', 'Enter']

# The page's answer to a key where it is cheap to read: `(theme, entry id, key)` to a function of
# the state before and after. Every `run` entry is held by the spy; these are held by both.
REACTS = {
    ('ember', 'volume', 'ArrowRight'): lambda was, now: now['raw'] == was['raw'] + 1,
    ('ember', 'volume', 'ArrowLeft'): lambda was, now: now['raw'] == was['raw'] - 1,
    ('ember', 'mute', 'm'): lambda was, now: now['muted'] is (not was['muted']),
    ('ember', 'preset', 'p'): lambda was, now: 'pre' in (now['focus'] or '').split(),
    ('ember', 'arrange', 'A'): lambda was, now: now['arranging'] and not was['arranging'],
    ('ember', 'arrange', 'a'): lambda was, now: now['arranging'] and not was['arranging'],
    ('nightdrive', 'volume', 'ArrowUp'): lambda was, now: now['raw'] == was['raw'] + 1,
    ('nightdrive', 'volume', 'ArrowDown'): lambda was, now: now['raw'] == was['raw'] - 1,
    ('voidgarden', 'volume', 'ArrowUp'): lambda was, now: now['raw'] == was['raw'] + 1,
    ('voidgarden', 'volume', 'ArrowDown'): lambda was, now: now['raw'] == was['raw'] - 1,
}
# The page's answer to an `own` key of the page: `(theme, entry id, key)` to a function of the
# state before and after. An `own` page entry with no line here fails the check.
OWNED = {}
# An `own` key of a widget, pressed in the state of STATES that shows the widget: `(theme, state)`
# to the entry's id, the key and the answer.
OWNED_IN = {
    ('ember', 'Arrange mode'): ('arrange-leave', 'Escape',
                                lambda was, now: was['arranging'] and not now['arranging']),
    ('ember', 'a point of the curve held'): ('curve-end', 'Escape',
                                             lambda was, now: was['held'] and not now['held']),
}
# What undoes a key's answer before the next key is pressed.
UNDO = {
    ('ember', 'arrange'): 'Escape',
    ('ember', 'mute'): 'm',
}

# The states that bring a widget onto the page, per theme: a name and the steps that enter it,
# each a selector of the control a person would use (clicked once it is there), with `js:` an
# expression (truthy when it did its part), or with `hold:` a selector of what the mouse is
# pressed on and held (let go when the state is left). A third part is an expression that puts the daemon
# back as it was (awaited); the page is loaded again after every state.
MENU = 'js:(() => { __core.frame.openMenu(); return 1; })()'
STATES = {
    'ember': [
        ('Arrange mode', [MENU, '#menu-arrange']),
        ('a band\'s value typed', ['#part-bands .band', '#part-bands .bedit .v']),
        # the Escape that ends a drag applies only while a point is held: the mouse is pressed on
        # a point of the device's curve and let go once the state was seen
        ('a point of the curve held', ['hold:#part-tone .plot .mk']),
        ('a cell of the computer\'s points typed', ['#part-switch .opt[data-side="computer"]',
                                                    '.pts-table .cell']),
    ],
    'paper': [
        ('a paper list', ['#p-mode']),
        ('a question', ['#p-preset-save']),
        ('the computer\'s side', ['#p-side-computer']),
    ],
    'signal': [
        ('a question', ['#sg-preset-save']),
        ('the list of devices', ['#sg-line [aria-haspopup=listbox] b']),
        ('the Add to the chain sheet', [MENU, '#menu-add-device']),
        ('the computer\'s hop', ['#sg-line [data-hop="computer"]']),
        # a hop the person declared has Edit; the hop goes again once the state was seen
        ('a hop\'s edit form',
         ["js:(() => { window.__hop ??= __core.actions.hops.add("
          "{ name: 'HD 650', kind: 'headphones', after: 'Gungnir 2' }); return 1; })()",
          '#sg-line [data-hop="hop:HD 650"]', '#sg-hop-edit-open'],
         "__core.actions.hops.remove('HD 650').then((done) => done.ok === true)"),
    ],
    'nightdrive': [
        ('a list', ['#nd-dash [aria-haspopup=listbox], .nd-k.chip[aria-haspopup=listbox]']),
        ('the sheet', ['#nd-sheet']),
        ('a question in the sheet', ['#nd-sheet', '#nd-preset-save']),
    ],
}
# What Help must show now: the entries that apply (the theme's, then the frame's), the zones that
# hold the control that had the focus before Help was opened, while it is still drawn (`__had`,
# kept by HAD from the document's own focus, not from the frame's record of the last one, which
# is what Help reads) and the theme's names for its zones.
HELP_WANT = """(() => { const table = __core.keys.listed(__core.frame.keyTable());
  const frame = __core.keys.listed(__core.keys.frame);
  return { table: table.map((e) => ({ id: e.id, zone: e.zone, own: e.own === true })),
    frame: frame.map((e) => e.id),
    zones: __core.keys.zonesOf(table, window.__had?.isConnected
      && (window.__had.checkVisibility?.({ visibilityProperty: true }) ?? true) ? window.__had : null),
    names: __core.frame.zoneNames() }; })()"""
# Keep the control of the theme that has the focus now, or nothing.
HAD = ("(() => { const at = document.activeElement;"
       " window.__had = document.getElementById('theme').contains(at) ? at : null; return 1; })()")
# What the open Help shows: per list of its page pane the heading over it and its rows' ids and
# zones; `null` until the popup is drawn.
HELP_SHOWN = """(() => { const pop = document.getElementById('popup-help');
  if (pop?.querySelector('.fp-box')?.dataset.state !== 'ready') return null;
  const out = []; let heading = null;
  for (const el of pop.querySelector('.fp-page').children) {
    if (el.matches('.fp-heading')) heading = el.textContent;
    else if (el.matches('.fp-rows')) out.push({ heading, rows: [...el.querySelectorAll('[data-key]')]
      .map((row) => [row.dataset.key, row.dataset.zone]) }); }
  return { at: __core.router.state().page, groups: out }; })()"""

CLICK = "(() => { const el = document.querySelector(%s); if (!el) return false; el.click(); return true; })()"

def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make():
    if browser != 'chromium':
        return common.Firefox(size=(1280, 900))
    # no 3D APIs: Void Garden stands as its still (a live scene on software rendering blocks the page for seconds at
    # every draw); no other theme's keys read a scene
    b = common.Chromium(size=(1280, 900), args=['--disable-3d-apis'])
    # a headless page that no key or pointer has touched does not count as focused, and Chromium
    # then sends no focus event for a `focus()` by script: a state entered by script would leave
    # the frame no last focus, which no person's page does
    b.send('Emulation.setFocusEmulationEnabled', {'enabled': True})
    return b


def poll(read, secs=4.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def press(b, name):
    """Press `name` (an `event.key`) with the real keyboard, whatever key it is: the harness's own
    `press` knows a short list."""
    if browser == 'chromium':
        if name in common.KEYS:
            key, code, vk, text = common.KEYS[name]
        elif len(name) == 1 and name.isalpha():
            key, code, vk, text = name, f'Key{name.upper()}', ord(name.upper()), name
        elif len(name) == 1 and name.isdigit():
            key, code, vk, text = name, f'Digit{name}', ord(name), name
        else:
            raise KeyError(name)
        base = {'key': key, 'code': code, 'windowsVirtualKeyCode': vk, 'nativeVirtualKeyCode': vk}
        # a capital and `?` come with Shift, as a person types them
        if (len(key) == 1 and key != key.lower()) or key == '?':
            base['modifiers'] = 8
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyDown', **({'text': text} if text else {})})
        b.send('Input.dispatchKeyEvent', {**base, 'type': 'keyUp'})
    else:
        k = common.WD_KEYS.get(name, name if len(name) == 1 else None)
        if k is None:
            raise KeyError(name)
        b._post('/actions', {'actions': [{'type': 'key', 'id': 'kb', 'actions': [
            {'type': 'keyDown', 'value': k}, {'type': 'keyUp', 'value': k}]}]})
    b.sleep(0.15)


def mount(b, daemon, theme):
    b.goto(f'{daemon.url}assets/core/frame.css', settle=0.2)
    b.page("(() => { try { localStorage.clear(); } catch (e) {} "
           "document.cookie = 'svolder_theme=; Max-Age=0; Path=/'; return 1; })()")
    b.goto(f'{daemon.url}?theme={theme}&effects=off', settle=1.0)
    mounted = b.page(BOOT)
    time.sleep(0.8)
    return mounted


def listeners(b, theme, who, when):
    """Hold the key listeners on the window and the document, as the page has had them on since
    it loaded, to the lines the theme's files are allowed and to the binding's one."""
    got = b.page(PEAK)
    tag = f'the_key_listeners_on_the_window_and_the_document_are_the_allowed_ones: {theme} for {who}, {when}'
    if got['peak'] is None:
        check(f'{tag}: the count was taken', False)
        return
    mine = {name: n for name, n in got['peak'].items() if name.startswith('themes/')}
    over = {name: [n, ALLOWED.get(name, 0)] for name, n in mine.items() if n > ALLOWED.get(name, 0)}
    other = [name for name in mine if not name.startswith(f'themes/{theme}/')]
    check(f'{tag}: no file of the theme has more on at once than LISTENERS allows it ({sum(mine.values())} seen)',
          over == {} and other == [], [over, other, got])
    want = 1 if b.page(BINDS) else 0
    bound_now = got['now'].get('core/keys.js keydown', 0)
    check(f'{tag}: the binding has {want} listener on the document', bound_now == want
          and got['peak'].get('core/keys.js keydown', 0) == want, got)
    nameless = sorted(name for name in got['peak'] if name.startswith('unknown '))
    check(f'{tag}: every key listener has a script of the page that added it', nameless == [], nameless)


def judged(want, shown):
    """What is wrong with Help's Here as `shown`, held to the tables as `want` reads them: a list
    of `[what, detail]`, empty when Here is right."""
    wrong = []
    page = [e['id'] for e in want['table'] if e['zone'] == 'page'] + want['frame']
    known = {e['id'] for e in want['table']} | set(want['frame'])
    rows = [row for group in shown['groups'] for row in group['rows']]
    if not (len(rows) > 0 and all(row[0] in known for row in rows) and len({row[0] for row in rows}) == len(rows)):
        wrong.append(['a row is no entry that applies now, or stands twice', rows])
    seen = set()
    for group in shown['groups']:
        zones = {row[1] for row in group['rows']}
        ids = [row[0] for row in group['rows']]
        seen |= zones
        if 'page' in zones:
            ok = ids == page and group is shown['groups'][-1]
            named = bool(group['heading'])
        else:
            focused = bool(zones & set(want['zones']))
            ok = ids == [e['id'] for e in want['table'] if e['zone'] in zones and (focused or e['own'])]
            named = group['heading'] in {want['names'].get(zone) for zone in zones}
        if not ok or not named or any(c in (group['heading'] or '') for c in '.#[]'):
            wrong.append(['a heading does not hold every listed entry of its zones under their name', group])
    held = [zone for zone in want['zones'] if any(e['zone'] == zone for e in want['table'])]
    owned = [e['zone'] for e in want['table'] if e['own'] and e['zone'] != 'page']
    if not all(zone in seen for zone in held + owned):
        wrong.append(['a zone that holds the last focus, or a widget with a key of its own, is missing',
                      [held, owned, sorted(seen)]])
    return wrong


def helped(b, theme, who, when, by_key):
    """Help's Here and Everywhere against the tables, on the page as it stands. Here is read as
    Help is shown: before the focus moves into the popup once its code is loaded, after it on the
    first open of a page, when the code arrives later. A widget that closes when it loses the
    focus (a list, a typed cell) is gone by then, so the tables are read both before the open
    and after it, each time for the control that had the focus before the open while it is
    still drawn, and Here is held to one of the two: a control that stays on the page must have
    its group in both."""
    tag = f'help_shows_the_tables_and_nothing_else: {theme} for {who}, {when}'
    if by_key:
        b.page(BLUR)
    b.page(HAD)
    before = b.page(HELP_WANT)
    if by_key:
        press(b, '?')
    else:
        b.page("(__core.router.open('help'), 1)")
    shown = poll(lambda: b.page(HELP_SHOWN), 6.0)
    if not shown:
        check(f'{tag}: Help opens', False, b.page("__core.router.state()"))
        return
    after = b.page(HELP_WANT)
    wrong = min(judged(before, shown), judged(after, shown), key=len)
    rows = sum(len(group['rows']) for group in shown['groups'])
    check(f'{tag}: every row of Here is an entry that applies now, under each heading every listed entry of '
          f'its zones, the page\'s keys with the frame\'s last ({rows} rows)', wrong == [], [wrong, before, after])
    page = [e['id'] for e in after['table'] if e['zone'] == 'page'] + after['frame']
    b.page("(document.getElementById('popup-help-entry-everywhere').click(), 1)")
    every = poll(lambda: (lambda got: got if got and got['at'] == 'everywhere' else None)(b.page(HELP_SHOWN)), 4.0)
    check(f'{tag}: Everywhere is the page\'s keys with the frame\'s',
          bool(every) and [row[0] for group in every['groups'] for row in group['rows']] == page, [every, page])
    b.page('(__core.router.close(), 1)')
    gone = poll(lambda: b.page("!document.getElementById('popup-help')"), 4.0)
    time.sleep(0.4)
    check(f'{tag}: Help closes', gone is True)


def hold(b, selector, down=True):
    """Press the mouse on the middle of `selector` and keep it down, or (`down` false) let go."""
    at = b.page(f"(() => {{ const el = document.querySelector({json.dumps(selector)}); if (!el) return null;"
                " el.scrollIntoView({ block: 'center' }); const r = el.getBoundingClientRect();"
                " return [r.left + r.width / 2, r.top + r.height / 2]; })()")
    if at is None:
        return False
    if browser == 'chromium':
        if down:
            b.send('Input.dispatchMouseEvent', {'type': 'mouseMoved', 'x': at[0], 'y': at[1]})
        b.send('Input.dispatchMouseEvent', {'type': 'mousePressed' if down else 'mouseReleased', 'x': at[0],
                                            'y': at[1], 'button': 'left', 'buttons': 1 if down else 0,
                                            'clickCount': 1})
    else:
        move = {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': int(at[0]), 'y': int(at[1])}
        acts = [move, {'type': 'pointerDown', 'button': 0}] if down else [{'type': 'pointerUp', 'button': 0}]
        b._post('/actions', {'actions': [{'type': 'pointer', 'id': 'mouse',
                                          'parameters': {'pointerType': 'mouse'}, 'actions': acts}]})
    b.sleep(0.3)
    return True


def step_in(b, step):
    """Do one step of a state: whether it did its part."""
    if step.startswith('js:'):
        return b.page(step[3:])
    if step.startswith('hold:'):
        return hold(b, step[5:])
    return b.page(CLICK % repr(step))


def settled(b, was, secs=2.5):
    """The state once a press has had its answer: the first state that differs from `was`, else
    the state after `secs`."""
    end = time.time() + secs
    now = b.page(STATE)
    while now == was and time.time() < end:
        time.sleep(0.1)
        now = b.page(STATE)
    return now


def page_keys(entries):
    """The `event.key` values the entries of zone `page` name."""
    return {key for e in entries if e['zone'] == 'page' for key in e['keys']}


def bound(b, theme, who, entries, writer=True):
    """Every key of every `run` entry that applies is bound: pressed, its `run` is called once;
    for a viewer who may write the page's answer is read too, where REACTS has one."""
    runs = [e for e in entries if e['run'] and e['zone'] == 'page' and e['listed']]
    for e in runs:
        for key in e['keys']:
            b.page(BLUR)
            b.page(RAN)
            was = b.page(STATE)
            press(b, key)
            reacts = REACTS.get((theme, e['id'], key)) if writer else None
            now = settled(b, was) if reacts else b.page(STATE)
            ran = b.page(RAN)
            check(f'every_page_key_that_applies_is_really_bound: {theme} for {who}: `{key}` '
                  f'({e["words"]}) calls its run once', ran == [e['i']], [key, ran])
            if reacts:
                check(f'  and the page answers ({theme}, `{key}`)', bool(reacts(was, now)), [was, now])
            undo = UNDO.get((theme, e['id']))
            if undo:
                press(b, undo)
                time.sleep(0.4)
    return len(runs)


def owned(b, theme, who, entries):
    """Every key of every `own` entry of the page that applies is answered: pressed, the page
    reacts as OWNED says, and no `run` is called (the binding does not dispatch it)."""
    mine = [e for e in entries if e['own'] and e['zone'] == 'page' and e['listed']]
    for e in mine:
        for key in e['keys']:
            b.page(BLUR)
            b.page(RAN)
            was = b.page(STATE)
            press(b, key)
            reacts = OWNED.get((theme, e['id'], key))
            now = settled(b, was)
            ran = b.page(RAN)
            check(f'every_own_key_of_the_page_is_really_answered: {theme} for {who}: `{key}` '
                  f'({e["words"]}) is answered by the theme\'s own listener and calls no run',
                  reacts is not None and bool(reacts(was, now)) and ran == [], [key, ran, was, now])
            undo = UNDO.get((theme, e['id']))
            if undo:
                press(b, undo)
                time.sleep(0.4)
    return len(mine)


def stray(b, theme, who, entries):
    """A key no entry of zone `page` names does nothing on the page."""
    named = page_keys(entries)
    tried = [key for key in STRAY if key not in named]
    acted = []
    for key in tried:
        b.page(BLUR)
        b.page(RAN)
        was = b.page(STATE)
        press(b, key)
        time.sleep(0.25)
        now = b.page(STATE)
        ran = b.page(RAN)
        if ran or now != was:
            acted.append([key, ran, {k: [was[k], now[k]] for k in was if was[k] != now[k]}])
    check(f'a_key_with_no_entry_does_nothing_global: {theme} for {who}: {len(tried)} keys with no '
          f'entry of the page ({" ".join(tried)}) call no run and leave the page as it was',
          len(tried) >= 10 and acted == [], acted)


def unbound(b, theme, who, entries):
    """An entry whose `when` is false is not listed, and its key does nothing."""
    off = [e for e in entries if e['run'] and e['zone'] == 'page' and not e['listed']]
    still = page_keys([e for e in entries if e['listed']])
    acted = []
    for e in off:
        for key in e['keys']:
            if key in still:
                continue
            b.page(BLUR)
            b.page(RAN)
            was = b.page(STATE)
            press(b, key)
            time.sleep(0.25)
            now = b.page(STATE)
            ran = b.page(RAN)
            if ran or now != was:
                acted.append([key, ran, {k: [was[k], now[k]] for k in was if was[k] != now[k]}])
    return off, acted


def zones(b, daemon, theme, entries):
    """Every widget entry is seen once, listed and with an element in its zone: on the page as it
    is, or in one of the theme's states."""
    seen = {e['i'] for e in entries if e['zone'] != 'page' and e['listed'] and e['here'] is True}
    bad = [[e['key'], e['zone']] for e in entries if e['here'] == 'bad selector']
    for name, steps, *undo in STATES.get(theme, []):
        entered = all(poll(lambda step=step: step_in(b, step), 3.0) for step in steps)
        got = poll(lambda: [e['i'] for e in b.page(TABLE)['entries']
                            if e['zone'] != 'page' and e['listed'] and e['here'] is True
                            and e['i'] not in seen], 2.0) if entered else []
        check(f'  the state "{name}" of {theme} is entered and shows a widget not seen before',
              bool(entered) and bool(got), [entered, got])
        seen.update(got or [])
        if entered:
            helped(b, theme, 'a signed-in viewer', f'in the state "{name}"', by_key=False)
        if (theme, name) in OWNED_IN and entered:
            entry, key, reacts = OWNED_IN[(theme, name)]
            was = b.page(STATE)
            press(b, key)
            now = settled(b, was)
            check(f'every_own_key_of_the_page_is_really_answered: {theme}: `{key}` in the state '
                  f'"{name}" ({entry}) is answered', bool(reacts(was, now)), [was, now])
        for step in steps:
            if step.startswith('hold:'):
                hold(b, step[5:], down=False)
                still = [e['id'] for e in b.page(TABLE)['entries'] if e['own'] and e['zone'] != 'page'
                         and e['listed'] and (theme, name) in OWNED_IN and e['id'] == OWNED_IN[(theme, name)][0]]
                check(f'  once the mouse is let go in the state "{name}" of {theme}, its key no longer applies',
                      still == [], still)
        for expression in undo:
            check(f'  the state "{name}" of {theme} is undone', b.page(expression) is True)
        listeners(b, theme, 'a signed-in viewer', f'in the state "{name}"')
        # back to the page as it opened
        mount(b, daemon, theme)
    widgets = [e for e in entries if e['zone'] != 'page']
    missing = [[e['id'], e['key'], e['zone'], e['listed'], e['here']] for e in widgets
               if e['i'] not in seen]
    check(f'every_widget_entrys_zone_matches_an_element: {theme}: each of its {len(widgets)} '
          f'widget entries is listed with an element in its zone, on the page or in a state of it',
          # Void Garden's keys are all the page's (↑ ↓, A and the explore keys S N E): it has no widget zone
          (len(widgets) > 0 or theme == 'voidgarden') and missing == [] and bad == [], [missing, bad])


def run(who, daemon, themes, writer=True, card=True):
    b = make()
    b.preload(COUNT)
    listed_here = False
    try:
        for theme in themes or ['ember']:
            mounted = mount(b, daemon, theme)
            if themes is None:
                # the first run mounts Ember and reads which themes the page lists
                themes = b.page(LISTED)
                listed_here = True
            if mounted != theme:
                check(f'{theme} is mounted for {who}', False, mounted)
                continue
            got = b.page(TABLE)
            entries = got['entries']
            check(f'the_table_is_the_mounted_instances_and_every_entry_is_whole: {theme} for {who}: '
                  f'{len(entries)} entries, each with an id, its keycaps, a zone and words',
                  got['mine'] and len(entries) >= (2 if theme == 'voidgarden' else 8) and all(e['whole'] for e in entries),
                  [got['mine'], [e for e in entries if not e['whole']]])
            b.page(SPY)
            count = owned(b, theme, who, entries)
            expected = 0
            check(f'  {theme} answers {expected} keys of the page with a listener of its own for {who}',
                  count == expected, count)
            if writer and card:
                count = bound(b, theme, who, entries)
                expected = {'ember': 7, 'nightdrive': 1, 'voidgarden': 5}.get(theme, 0)
                check(f'  {theme} binds {expected} keys of the page for {who}', count == expected, count)
                stray(b, theme, who, entries)
                helped(b, theme, who, 'on the page as it opens', by_key=True)
                listeners(b, theme, who, 'after the presses')
                mount(b, daemon, theme)
                b.page(SPY)
                zones(b, daemon, theme, b.page(TABLE)['entries'])
            else:
                off, acted = unbound(b, theme, who, entries)
                ids = sorted(e['id'] for e in off)
                if not writer:
                    want = {'ember': ['centre', 'flat', 'mute', 'preset', 'volume'],
                            'nightdrive': ['volume'], 'voidgarden': ['volume']}.get(theme, [])
                    what = 'a guest has no key that writes'
                else:
                    want = {'ember': ['look']}.get(theme, [])
                    what = 'a page with no visualizer card has no `v`'
                check(f'an_entry_whose_when_is_false_is_neither_bound_nor_listed: {theme} for {who}: '
                      f'{what}: not listed {ids}, and pressing them calls no run and changes nothing',
                      ids == want and acted == [], [ids, acted])
                unlisted = [e['id'] for e in entries if not e['listed'] and e['zone'] != 'page']
                if not writer:
                    check(f'  and the widget keys that write are not listed for {who} ({theme})',
                          len(unlisted) >= 1 or theme == 'voidgarden', unlisted)
                bound(b, theme, who, entries, writer)
                stray(b, theme, who, entries)
                helped(b, theme, who, 'on the page as it opens', by_key=True)
                listeners(b, theme, who, 'after the presses')
            if listed_here:
                break
    finally:
        b.close()
    return themes


# a second device beside the DAC: Signal's line then has a list to open (the devices)
d = common.Daemon('gungnir2-v2', host='fake', fleet=[('fake-mono', 'Mono', '3132333435363738')])
g = common.Daemon('gungnir2-v2', host='fake', web='guest = "read"')
bare = common.Daemon('gungnir2-v2')
try:
    listed = run('a signed-in viewer', d, None)
    themes = listed if isinstance(listed, list) else ['ember']
    check('the page lists the five themes',
          sorted(themes) == ['ember', 'nightdrive', 'paper', 'signal', 'voidgarden'], themes)
    run('a signed-in viewer', d, [t for t in themes if t != 'ember'])
    run('a guest', g, themes, writer=False)
    run('a viewer of a page with no player', bare, ['ember'], card=False)
finally:
    d.stop()
    g.stop()
    bare.stop()
print(f'keys.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
