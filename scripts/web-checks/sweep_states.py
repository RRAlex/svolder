# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The pages and popups the two sweeps (contrast.py, phone_sweep.py) visit, shared so that both see the same
states. A visit is `visit_*(b, probe, ...)`: it puts the browser in each state in turn and calls
`probe(label)` there; what a probe measures is the sweep's. The popups come from the page's own list
(`__core.popups.listed()`, listed.js) and each popup's entries from that list or, where the list is
drawn per viewer (the Devices popup), from the side list the popup drew; so a popup or an entry added
later is visited with no edit here. The states a bare fake does not show are made on purpose (a
device's page with its facts, readings, passphrase form, profile list, a remove question and a refused
setting; a device that never connected; the add walk's first pages; the config-changed line; the EQ
library's preview with a second measurement's list closed and its upload and saved pages; the tone
graph's overlay), and every device a step opens or acts on is named (the fleet's, from the fake).

Usage: imported by contrast.py and phone_sweep.py."""
import json
import os
import signal
import time

import common
import modal
from devices_popup import (ADD_ENTRY, BIFROST, WALK, at_step, hand_edit, press_on, press_sel, put_text, sid, to_add,
                           click_summary, walk_page)
from eq_library import api, pid, search, variant
from modal import BODY, BOOT, HOOK, check, load, poll
from popups import WARNS, close_popup, el, open_popup, ready
from tone_overlay import put_wide_on

browser = modal.browser
DESK = (1280, 800)
NO_AUTOEQ = {'SVOLDER_AUTOEQ_URL': 'http://127.0.0.1:9/'}
SECOND = ('Second', '1112131415161718')
THIRD = ('Third', '2122232425262728')
DEVICE = 'Gungnir 2'


class Rig:
    """The daemons of a sweep: a writer's (two more configured devices, a unit on the air that is not
    configured for the add walk, the AutoEq fixture, a preset whose overlay the tone graph draws, the
    config file edited by hand so the Overview carries its line), a guest's, a page with writes off and one
    with no device configured. Every one is a fake on a free port in a temp home of its own."""

    def __init__(self):
        self.fx = common.AutoEqFixture()
        self.parts = [self.fx]
        env = {'SVOLDER_AUTOEQ_URL': self.fx.url, 'SVOLDER_FAKE_MORE': f'bifrost3@{BIFROST}'}
        self.writer = self._make(common.Daemon, host='fake', more=[SECOND, THIRD], env=env)
        self.overlay = put_wide_on(self.writer)
        hand_edit(f'{self.writer.dir}/config/svolder/config.toml', 5)
        self.guest = self._make(common.Daemon, host='fake', web='guest = "read"', env=NO_AUTOEQ,
                                more=[SECOND, THIRD])
        self.off = self._make(common.Daemon, host='fake', web='auth = "none"\nwrites = false', env=NO_AUTOEQ,
                              more=[SECOND, THIRD])
        self.blank = self._make(common.Daemon, blank=True,
                                env={**NO_AUTOEQ, 'SVOLDER_FAKE_MORE': f'bifrost3@{BIFROST}'})

    def _make(self, make, **kw):
        d = make(**kw)
        self.parts.append(d)
        return d

    def stop(self):
        for part in reversed(self.parts):
            try:
                part.stop()
            except Exception:  # noqa: BLE001 (stopping the rest matters more)
                pass
        for part in self.parts:
            home = getattr(part, 'dir', None)
            if home:
                common.remove_tree(home)


def browser_of(size=DESK):
    return (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)


def open_page(b, d, theme):
    """The page of daemon `d` in `theme`, loaded and settled, nothing open."""
    b.goto(f'{d.url}?theme={theme}&effects=off', settle=1.5)
    b.page(BOOT)
    time.sleep(0.5)
    b.page(BODY)


def themes_of(b):
    """`[(id, [schemes])]` as the page lists them."""
    return [(t[0], t[1] or ['dark']) for t in b.page(
        "__core.registry.list().map((e) => [e.id, e.schemes])")]


def set_scheme(b, scheme, schemes):
    if len(schemes) > 1:
        b.page(f"(__core.scheme.set({json.dumps(scheme)}), 1)")
        poll(lambda: b.page('document.documentElement.dataset.scheme') == scheme, 4.0)
    time.sleep(0.4)


def listed(b):
    """`[(name, [entry ids])]` of the page's popups, in the menu's order."""
    return [(p['name'], [e['id'] for e in (p.get('entries') or [])])
            for p in b.page("__core.popups.listed().map((p) => ({ name: p.name, entries: p.entries ?? [] }))")]


def focus_button(b):
    b.page("(document.getElementById('fx-button').focus(), 1)")


def open_to(b, name, page, probe, label):
    """Open the popup `name` at `page` by a real click and probe it; false when it did not draw."""
    b.page(BODY)
    focus_button(b)
    if open_popup(b, name, page) is not True:
        check(f'sweep: {label} opens', False, '')
        return False
    time.sleep(0.5)
    probe(label)
    return True


def wait_for(b, ident, secs=8.0):
    """The element `ident` is drawn (a device's page draws a moment after the popup is ready)."""
    return poll(lambda: b.page(f"!!document.getElementById({json.dumps(ident)})"), secs)


def shut(b, name):
    close_popup(b, name)
    b.page(BODY)


def menu_state(b, probe):
    b.page(BODY)
    b.click(*common.center(b, '#fx-button'))
    if poll(lambda: b.page("!document.getElementById('fx-menu').hidden"), 3.0) is True:
        time.sleep(0.3)
        probe('menu')
        b.press('Escape')
        time.sleep(0.3)
    else:
        check('sweep: the menu opens', False, '')


def entry_ids(b, name):
    return b.page(f"[...document.querySelectorAll('#popup-{name} .fp-entry')].map((e) => e.dataset.popupPage)")


class Tracker:
    """A probe that remembers each state it was called for, so a visit can say which of the states it must reach it
    did not: a step that is not reached (a wording changed, a fixture stopped matching) fails the check and is not
    left out in silence."""

    def __init__(self, probe, expected):
        self.probe, self.expected, self.seen = probe, set(expected), set()

    def __call__(self, label):
        self.seen.add(label)
        self.probe(label)

    def done(self, who):
        missing = sorted(self.expected - self.seen)
        check(f'sweep ({who}): every state the visit names was reached', not missing, missing)


def visit_listed(b, probe, tag, expected, skip=()):
    """Every listed popup on each of its entries. Where the entries are drawn per viewer (Devices) the
    ones in the side list are visited."""
    for name, static in listed(b):
        if name in skip:
            continue
        first = static[0] if static else None
        if not open_to(b, name, first, probe, f'{tag}{name}/{first or "first"}'):
            continue
        pages = entry_ids(b, name) or static
        for page in pages:
            if page == 'add':
                continue
            expected.add(f'{tag}{name}/{page}')
            b.page(f"(__core.router.step({json.dumps(page)}), 1)")
            time.sleep(0.5)
            probe(f'{tag}{name}/{page}')
        shut(b, name)


def fleet_of(b):
    return b.page('__core.model.page().fleet.devices.map((x) => x.url)')


def visit_writer(b, probe, d, rig):
    """The writer's page: the page, the menu, every popup's entries, then the states of the Devices popup's
    device pages, the add walk and the EQ library that need a hand."""
    urls = fleet_of(b)
    first = urls[0]
    listing = listed(b)
    expected = {'page', 'menu'}
    for name, static in listing:
        for page in static:
            expected.add(f'{name}/{page}')
    for u in urls:
        expected.add(f'devices/d-{u}')
    expected |= {f'devices/d-{first}/{x}' for x in ('top', 'readings-open', 'passphrase-form', 'profile-list',
                                                    'refused-name')}
    expected |= {'devices/d-Second/dimmed', 'devices/d-Second/remove-ask', 'devices/add/warning',
                 'devices/add/scan', 'devices/add/pick', 'eq-library/autoeq/empty', 'eq-library/autoeq/preview'}
    probe = Tracker(probe, expected)
    b.page(BODY)
    probe('page')
    menu_state(b, probe)
    visit_listed(b, probe, '', probe.expected)
    # ---- a device's page, a form open, a list open, a question, a refused setting ----
    if open_to(b, 'devices', f'd-{first}', probe, f'devices/d-{first}/top'):
        wait_for(b, sid('passphrase', 'change'))
        time.sleep(0.4)
        click_summary(b)
        time.sleep(0.3)
        probe(f'devices/d-{first}/readings-open')
        click_summary(b)
        press_on(b, sid('passphrase', 'change'))
        press_on(b, sid('passphrase', 'kind-code'))
        probe(f'devices/d-{first}/passphrase-form')
        press_on(b, sid('passphrase', 'cancel'))
        press_on(b, sid('profile', 'open'))
        probe(f'devices/d-{first}/profile-list')
        b.press('Escape')
        time.sleep(0.3)
        put_text(b, 'popup-devices-set-name-field', 'Second')
        press_on(b, sid('name', 'save'))
        poll(lambda: b.page(f"document.getElementById({json.dumps(sid('name', 'words'))})?.textContent") != '', 6.0)
        probe(f'devices/d-{first}/refused-name')
        put_text(b, 'popup-devices-set-name-field', DEVICE)
        shut(b, 'devices')
    # the second device never connected: its page is dimmed; a remove question over it
    if open_to(b, 'devices', 'd-Second', probe, 'devices/d-Second/dimmed'):
        wait_for(b, sid('remove', 'open'))
        press_on(b, sid('remove', 'open'))
        time.sleep(0.3)
        probe('devices/d-Second/remove-ask')
        b.press('Escape')
        shut(b, 'devices')
    # ---- the add walk: the warning, the scan prompt, the pick ----
    b.page(BODY)
    focus_button(b)
    if to_add(b) is True:
        if at_step(b, 'A device is linked', 12.0) is True:
            probe('devices/add/warning')
            press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
            if at_step(b, 'Find the device', 6.0) is True:
                probe('devices/add/scan')
                press_sel(b, f'#{WALK}-controls [data-walk-choice="0"]')
                if at_step(b, 'Which device', 20.0) is True:
                    probe('devices/add/pick')
        b.press('Escape')
        time.sleep(0.4)
        shut(b, 'devices')
    # ---- the EQ library: a search, a preview with its graph, the other pages ----
    if open_to(b, 'eq-library', 'autoeq', probe, 'eq-library/autoeq/empty'):
        if search(b, 'hd 650'):
            press_sel(b, variant(0, 0))
            if poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph-target')"), 15.0):
                time.sleep(0.4)
                probe('eq-library/autoeq/preview')
        shut(b, 'eq-library')
    probe.done('writer')


def visit_reader(b, probe, daemon_kind):
    """A guest's page (`guest`), or a signed-in viewer's of a page with writes off (`off`): the page, the
    Overview and the device's page, read-only."""
    first = fleet_of(b)[0]
    expected = {f'{daemon_kind}/page', f'{daemon_kind}/devices/overview', f'{daemon_kind}/devices/overview-drawn',
                f'{daemon_kind}/devices/d-{first}'}
    if daemon_kind == 'guest':
        expected.add('guest/help/here')
    probe = Tracker(probe, expected)
    b.page(BODY)
    probe(f'{daemon_kind}/page')
    if open_to(b, 'devices', 'overview', probe, f'{daemon_kind}/devices/overview'):
        drawn = poll(lambda: b.page("document.querySelectorAll('#popup-devices .fpd-device .fpd-graph').length") > 0, 8.0)
        check(f'sweep ({daemon_kind}): the Overview drew its graphs', drawn is True, '')
        probe(f'{daemon_kind}/devices/overview-drawn')
        b.page(f"(__core.router.step('d-{first}'), 1)")
        facts = poll(lambda: b.page("!!document.querySelector('#popup-devices-facts dd[data-fact=Firmware]')"), 8.0)
        check(f'sweep ({daemon_kind}): the device page drew its facts', facts is True, '')
        probe(f'{daemon_kind}/devices/d-{first}')
        shut(b, 'devices')
    if daemon_kind == 'guest':
        if open_to(b, 'help', 'here', probe, 'guest/help/here'):
            shut(b, 'help')
    probe.done(daemon_kind)


def visit_blank(b, probe):
    """A page with no device configured: its page, the Overview with no device and the add walk's first page (no
    warning: nothing is linked)."""
    probe = Tracker(probe, {'blank/page', 'blank/devices/overview', 'blank/devices/overview-drawn',
                            'blank/devices/add'})
    b.page(BODY)
    probe('blank/page')
    if open_to(b, 'devices', 'overview', probe, 'blank/devices/overview'):
        time.sleep(0.4)
        probe('blank/devices/overview-drawn')
        shut(b, 'devices')
    b.page(BODY)
    focus_button(b)
    if to_add(b) is True:
        time.sleep(0.6)
        probe('blank/devices/add')
        b.press('Escape')
        time.sleep(0.3)
        shut(b, 'devices')
    probe.done('blank')


def words_of(error):
    """What a failed page call said: the browser's description of the exception, else the error's text."""
    import re
    text = str(error)
    m = re.search(r"'description': '((?:[^'\\]|\\.)*)'", text)
    return (m.group(1) if m else text)[:400]


# ---- one child process per theme ----

def visitors(rig, who=None):
    """`[(name, daemon, visit)]` of a Rig, `who` (a comma list, or the env WEB_CHECK_WHO) narrowing them."""
    every = [('writer', rig.writer, lambda b, probe: visit_writer(b, probe, rig.writer, rig)),
             ('guest', rig.guest, lambda b, probe: visit_reader(b, probe, 'guest')),
             ('off', rig.off, lambda b, probe: visit_reader(b, probe, 'off')),
             ('blank', rig.blank, lambda b, probe: visit_blank(b, probe))]
    keep = os.environ.get('WEB_CHECK_WHO', 'writer,guest,off,blank').split(',')
    if who:
        keep = [v for v in who.split(',') if v in keep]
    return [v for v in every if v[0] in keep]


def run_children(script, themes, par=None):
    """Run `script chromium|firefox --child THEME --out FILE` once per theme, `par` at a time (default all of
    them: each has a browser and four daemons of its own, in homes of its own), and read what each wrote: a list
    of the children's JSON. Their output is printed after, theme by theme; the temp files go even on a failure."""
    import subprocess
    import sys
    par = par or int(os.environ.get('WEB_SWEEP_PAR', len(themes)))
    folder = common.scratch('sweep-')
    results, running, queue = {}, [], list(themes)
    try:
        def start(theme):
            out, log = f'{folder}/{theme}.json', f'{folder}/{theme}.log'
            proc = subprocess.Popen([sys.executable, script, browser, '--child', theme, '--out', out],
                                    stdout=open(log, 'w'), stderr=subprocess.STDOUT)
            running.append((theme, proc, out, log))
        while queue or running:
            while queue and len(running) < par:
                start(queue.pop(0))
            theme, proc, out, log = running.pop(0)
            code = proc.wait()
            print(open(log).read(), end='')
            if code != 0 or not os.path.exists(out):
                check(f'sweep: the child for {theme} ran to its end (exit {code})', False, '')
                continue
            with open(out) as f:
                results[theme] = json.load(f)
    finally:
        for _, proc, _, _ in running:
            proc.terminate()
        common.remove_tree(folder)
    return results
