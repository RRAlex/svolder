# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the theme Signal's page: each mutation edits a copy of the page's web dir
(never the repo), runs signal_theme.py (Chromium) against it and must make a check the mutant
names FAIL: a failure of any other check is not a kill (a flake must not pass for one). The mutants
the node harnesses kill are in node/mutate_signal.sh.
Usage: python3 mutate_signal.py [name ...]   (default: all). One worker, by hand, slow."""
import os
import shutil
import subprocess
import sys

import common

HERE = os.path.dirname(os.path.abspath(__file__))
S = 'themes/signal/'
# name: ([the checks that must fail, by the start of their names], edits)
MUTATIONS = {
    'an amp control is written without the amp\'s url': (['an_amp_control_writes_to_the_amps_url'], [
        (S + 'amp-wire.js', "JSON.parse(target.json), { device: amp.url })", "JSON.parse(target.json), {})"),
    ]),
    'the dots ignore reduced motion': (['the_dots_stand_still_under_reduced_motion'], [
        (S + 'signal.js', "matchMedia: (query) => window.matchMedia(query),",
         "matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {},"
         " addListener() {}, removeListener() {} }),"),
    ]),
    'the slim bar never pins': (['the_slim_bar_pins_when_the_line_scrolls_away'], [
        (S + 'slim.js', "bar.hidden = last.isIntersecting;", "bar.hidden = true;"),
    ]),
    'the Add device row opens the sheet for a guest': (['a_guest_cannot_add_or_edit_a_hop'], [
        (S + 'hops-wire.js', "  function add() {\n    if (inert()) {", "  function add() {\n    if (false) {"),
    ]),
    'a held band stepper never repeats': (['holding_a_band_stepper_repeats'], [
        (S + 'controls-wire.js', "      live: () => !view.device()?.inert && view.state !== 'lost' && view.state !== 'waiting',",
         "      live: () => false,"),
    ]),
    'the menu button follows the page': (['the_menu_button_is_fixed_on_a_phone'], [
        (S + 'signal.js', "{ fixed: true }", "{ fixed: false }"),
    ]),
    'the mode is not under the curve on a phone': (['the_mode_has_its_place'], [
        (S + 'signal.js', "${view.phone() ? modeRow(device) : ''}", "${''}"),
    ]),
    'the tab keeps its plain title': (['the_tab_title_names_the_device_and_the_connection'], [
        ('core/title.js', "    document.title = titleOf({ name, status: statusOf(page, store.state()), more });", "    void more;"),
    ]),
    'a held plus never repeats': (['holding_plus_repeats'], [
        (S + 'volume-wire.js', "      live: () => !inert() && !quiet(),", "      live: () => false,"),
    ]),
    'a blur does not stop a hold': (['holding_plus_repeats'], [
        ('core/hold.js', "  globalThis.window.addEventListener('blur', end, own);\n", ""),
    ]),
    'a hold does not hold the open area': (['holding_plus_repeats'], [
        (S + 'volume-wire.js', "      onStart: () => keep.take(),", "      onStart: () => {},"),
        (S + 'volume-wire.js', "        if (give) keep.drop();", "        if (give && false) keep.drop();"),
    ]),
    'a pinch narrows when the fingers spread': (['a_pinch_sets_the_q'], [
        (S + 'tone-wire.js', "fingers.entry.drag.spread(spacing(a, b));",
         "fingers.entry.drag.spread(1e6 / spacing(a, b));"),
    ]),
    'a tall window stretches the open area': (['a_tall_window_keeps_the_block_together'], [
        (S + 'style.css', "  .sg-open { flex: none; }\n", ""),
    ]),
    'the shelf has three columns again': (['the_shelf_has_two_columns'], [
        (S + 'style.css', "grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 34px 40px;",
         "grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) minmax(0, 1fr); gap: 34px 40px;"),
    ]),
    'the strip does not centre the chosen hop': (['the_strip_centres_the_chosen_hop'], [
        (S + 'draw/line.js', "translateX(${drag - at * HOP_PX}px)", "translateX(${drag}px)"),
    ]),
}


def run(name):
    web = common.scratch('mut-web-')
    shutil.copytree(common.WEB, web + '/w')
    named, edits = MUTATIONS[name]
    for f, old, new in edits:
        p = f'{web}/w/{f}'
        s = open(p).read()
        assert old in s, f'{name}: {old[:50]!r} not in {f}'
        open(p, 'w').write(s.replace(old, new, 1))
    env = dict(os.environ, SMOKE_WEB=web + '/w')
    r = subprocess.run([sys.executable, f'{HERE}/signal_theme.py', 'chromium'], env=env,
                       capture_output=True, text=True, timeout=1800)
    shutil.rmtree(web, ignore_errors=True)
    # a check's own line names it; the indented lines after it are parts of it
    failed, fails, current = [], [], ''
    for line in r.stdout.splitlines():
        if not line.startswith(('ok  ', 'FAIL')):
            continue
        text = line[5:]
        if not text.startswith('  '):
            current = text
        if line.startswith('FAIL'):
            failed.append(line)
            # only a check the mutant names counts; any other failure is listed and is not a kill
            if any(current.startswith(n) for n in named):
                fails.append(line)
    if not fails and r.returncode and not failed:
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:200]
        return name, None, '(error, not a kill) ' + tail
    other = [x for x in failed if x not in fails]
    return name, fails, ('(failed only other checks, not a kill) ' + other[0][:120] if other and not fails else '')


names = sys.argv[1:] or list(MUTATIONS)
bad = 0
for name in names:
    name, fails, err = run(name)
    print('ERROR   ' if fails is None else 'KILLED  ' if fails else 'SURVIVED', name, flush=True)
    for f in (fails or [])[:3]:
        print('    ', f[:160])
    if err:
        print('    ', err)
    bad += not fails
print(f'{len(names) - bad} of {len(names)} mutations killed')
sys.exit(1 if bad else 0)
