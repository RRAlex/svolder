# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The EQ library popup (core/popups/eq-library/) in a browser, in every theme: what no node
harness can see (a real click, a real Enter, the focus, the layout, the colours the theme sets).
The wire's logic is node's (node/core-eq-library.mjs); popups.py holds the popup's frame. The
daemon is a fake (`Gungnir 2` shown, the computer's side fake too) whose AutoEq is a loopback
server of three headphones' files under scripts/web-checks/fixtures/autoeq/ (the daemon's
SVOLDER_AUTOEQ_URL); nothing reaches the internet. Every preset a case makes it removes.

Per theme, at 1280 px unless said:
  opens_by_its_menu_row_and_names_the_device   a real click on the menu's button and on `EQ
                                        library…`; the title and the tab's title name the shown
                                        device; the side list holds AutoEq, Upload a file, Saved
                                        presets
  the_search_finds_headphones_grouped    typing waits for the pause; the answer groups the
                                        headphones, each measurement of one headphone under it,
                                        the recommended first; Enter sends at once; the index's
                                        date is told
  the_preview_is_the_daemons_numbers     a real click on a measurement: the graph has the target,
                                        the fit and the gap in the colours of the theme's
                                        tokens, and the fit and worst miss the page words are the
                                        numbers the daemon's own preview route gives
  compare_with_is_a_list_in_the_pane     the chip opens a list inside the box, a pick draws the
                                        second target in its own class with its numbers and Stop
                                        comparing takes it away
  a_measurement_that_is_not_there_says_so   an index entry with no file: the daemon's refusal as
                                        words
  import_saves_and_a_taken_name_is_worded_at_the_field   Save adds an imported preset (the
                                        daemon's model lists it) and says it is not on; saving
                                        again is refused at the field with the suggestion, which
                                        a click takes
  saved_presets_say_where_they_live_and_overwrite_asks_once   the marks, the reason for each
                                        preset that cannot be overwritten, one question with
                                        Keep focused, Keep sends nothing, Overwrite sends `force`
  upload_reads_a_file_in_the_page        Save is disabled with its reason before a file; a text
                                        the daemon cannot read is refused in its words; a good
                                        one is saved
  the_computer_side                      the title names the computer, the preview is the
                                        correction with no fit, Overwrite is disabled
  nothing_scrolls_sideways               at 1280 and at 390 px; on a phone the list, then a page
                                        and the way back
  a_guest_and_a_read_only_page_have_no_way_in   on a guest's daemon and on one with `writes =
                                        false`: the Devices row (the read-only popup) but no EQ
                                        library row, an open by the router draws no popup,
                                        and no request goes to the AutoEq routes
With WEB_CHECK_SHOTS=<folder> `<theme>-preview.png`, a preview with a second measurement, is
written there.
Usage: python3 eq_library.py chromium|firefox"""
import json
import os
import sys
import time
import urllib.error
import urllib.request

import common
import modal
from modal import AT, BODY, POSTS, STATE, THEMES, check, load, poll
from popups import DESK, PHONE, by_the_menu, centre, close_popup, el, ready, rects

browser = modal.browser
DEVICE = 'Gungnir 2'
NAME = 'eq-library'
HD650 = 'Sennheiser HD 650'
SOURCES = ['oratory1990', 'Rtings', 'Innerfidelity']


def pid(part):
    return f'popup-{NAME}-{part}'


def api(d, path, body):
    """A POST to the daemon's own route as the page would make it: the status and the JSON."""
    req = urllib.request.Request(
        f'{d.url}{path}', data=json.dumps(body).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    try:
        with urllib.request.urlopen(req, timeout=60) as answer:
            return answer.status, json.load(answer)
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b'{}')


def put_text(b, ident, text):
    """Type `text` into the field: its value and the events a typist's keys make."""
    b.page(f"(() => {{ const f = document.getElementById({json.dumps(ident)}); f.focus();"
           f" f.value = {json.dumps(text)};"
           " f.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()")


def press_on(b, selector, wait=0.3):
    """A real click on the control: scrolled into view first."""
    b.page(f"(document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.15)
    b.click(*centre(b, selector))
    time.sleep(wait)


def text_of(b, ident):
    return b.page(f"document.getElementById({json.dumps(ident)})?.textContent ?? null")


def variant(group, which):
    return f'#{pid(f"v-{group}-{which}")}'


def rows(d):
    return {row['name']: row for row in d.model(DEVICE)['presets']['rows']}


def expected_words(preview):
    """The fit's words as the page words them, from the daemon's own numbers."""
    fit = preview['fit']
    hz = fit['worst_hz']
    where = f'{hz / 1000:.1f} kHz' if hz >= 1000 else f"{float(f'{hz:.{1 if hz < 100 else 0}f}'):g} Hz"
    return f"{fit['rms_db']:.1f} dB", f"{fit['worst_db']:.1f} dB at {where}"


def open_library(b, tag):
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    return by_the_menu(b, NAME)


def search(b, text):
    put_text(b, pid('query'), text)
    return poll(lambda: b.page("document.querySelectorAll('.fpe-group').length"), 8.0)


def case(b, d, fx, theme):
    tag = f'({theme})'
    b.page(BODY)
    opened = open_library(b, tag)
    title = b.page(f"[document.getElementById('{pid('title')}').textContent, document.title,"
                   " [...document.querySelectorAll('#popup-eq-library .fp-entry')].map((e) => e.textContent)]")
    check(f'opens_by_its_menu_row_and_names_the_device {tag}',
          opened is True and title[0] == f'EQ library · {DEVICE}' and 'EQ library' in title[1]
          and DEVICE in title[1] and title[2] == ['AutoEq', 'Upload a file', 'Saved presets'], title)

    # ---- the search ----
    asked = len(fx.requests)
    found = search(b, 'hd 6')
    names = b.page("[...document.querySelectorAll('.fpe-group-name')].map((e) => e.firstChild.textContent)")
    check(f'the_search_finds_headphones_grouped {tag}: typing waits for the pause and the answer groups '
          'the headphones', found == 2 and sorted(names) == ['Sennheiser HD 600', HD650], [found, names])
    status = text_of(b, pid('status'))
    check(f'the_search_finds_headphones_grouped {tag}: the index and its date are told',
          'AutoEq\'s index lists 4 measurements' in status and 'was fetched on 20' in status, status)
    check(f'the_search_finds_headphones_grouped {tag}: the daemon read the index from the fixture',
          any(p.endswith('/INDEX.md') for p in fx.requests[asked:]) or any(p.endswith('/INDEX.md') for p in fx.requests),
          fx.requests)
    group = [i for i, n in enumerate(names) if n == HD650][0]
    words = b.page(f"[...document.querySelectorAll('.fpe-group')[{group}].querySelectorAll('.fpe-variant')]"
                   ".map((e) => e.textContent.replace(/\\s+/g, ' ').trim())")
    check(f'the_search_finds_headphones_grouped {tag}: each measurement under its headphone, the '
          'recommended first', words[0].endswith('Recommended') and len(words) == 3
          and words[0].startswith('oratory1990 · over-ear'), words)
    marker = b.page("(() => { const s = document.querySelector('.fpe-group-name');"
                    " return [getComputedStyle(s, '::before').content, getComputedStyle(s, '::before').width]; })()")
    check(f'the_search_finds_headphones_grouped {tag}: a disclosure has a marker of its own (the summary is flex)',
          marker[0] not in ('none', 'normal') and marker[1] != 'auto', marker)
    put_text(b, pid('query'), 'hd 650')
    b.press('Enter')
    after = poll(lambda: b.page("document.querySelectorAll('.fpe-group').length") == 1, 8.0)
    check(f'the_search_finds_headphones_grouped {tag}: Enter sends at once and the form never navigates',
          after is True and b.page('location.search').count('eq-query') == 0, b.page('location.href'))
    group = 0

    # ---- the preview ----
    press_on(b, variant(group, 0))
    shown = poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph')"), 15.0)
    code, daemon = api(d, 'api/v1/autoeq/preview', {
        'variant': {'name': HD650, 'source': 'oratory1990', 'form': 'over-ear'},
        'target': {'side': 'device', 'device': DEVICE}})
    preview = daemon['preview']
    fit, worst = expected_words(preview)
    text = text_of(b, pid('preview'))
    check(f'the_preview_is_the_daemons_numbers {tag}: the fit and the worst miss are the daemon\'s numbers',
          shown is True and code == 200 and f'Fit{fit}'.replace(' ', '') in text.replace(' ', '')
          and worst.replace(' ', '') in text.replace(' ', ''), [fit, worst, text])
    graph = b.page("""(() => { const p = document.getElementById('popup-eq-library-preview');
      const probe = document.createElement('i');
      probe.style.cssText = 'position:fixed;left:-99px;stroke:var(--frame-graph-series);color:var(--frame-graph-target);'
        + 'fill:var(--frame-graph-gap)';
      document.body.append(probe); const want = getComputedStyle(probe);
      const css = (q, prop) => { const e = p.querySelector(q); return e ? getComputedStyle(e)[prop] : null; };
      const out = { series: css('.fp-graph-series', 'stroke'), wantSeries: want.stroke,
        target: css('.fp-graph-target', 'stroke'), wantTarget: want.color,
        gap: css('.fp-graph-gap', 'fill'), wantGap: want.fill,
        dash: css('.fp-graph-target', 'strokeDasharray'),
        box: p.querySelector('.fp-graph').getBoundingClientRect().width,
        d: p.querySelector('.fp-graph-series')?.getAttribute('d')?.length ?? 0 };
      probe.remove(); return out; })()""")
    has_gap = bool(preview['fit']['gap'])
    check(f'the_preview_is_the_daemons_numbers {tag}: the target is dashed, the fit a line, both in the '
          'theme\'s tokens', graph['series'] == graph['wantSeries'] and graph['target'] == graph['wantTarget']
          and graph['dash'] not in (None, 'none') and graph['d'] > 100 and graph['box'] > 200, graph)
    check(f'the_preview_is_the_daemons_numbers {tag}: the gap is shaded exactly where the daemon says one is '
          f'({len(preview["fit"]["gap"])} span(s))',
          (graph['gap'] is not None) == has_gap and (not has_gap or graph['gap'] == graph['wantGap']), graph)

    # ---- compare ----
    press_on(b, '#' + pid('compare-open'))
    listed = poll(lambda: b.page("[...document.querySelectorAll('.fi-opt')].map((e) => e.textContent)"), 4.0)
    inside = b.page("!!document.querySelector('.fi-list')?.closest('.fp-page')")
    check(f'compare_with_is_a_list_in_the_pane {tag}: the list stands in the box with the other measurements',
          inside is True and len(listed) == 2 and 'Rtings · over-ear · GRAS 43AG-7' in listed, [inside, listed])
    b.click(*centre(b, '.fi-opt'))
    drawn = poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph-compare')"), 15.0)
    other = b.page("document.getElementById('popup-eq-library-preview').textContent")
    check(f'compare_with_is_a_list_in_the_pane {tag}: a pick draws the second target in its own class with its '
          'numbers', drawn is True and 'Compared: ' in other and other.count('Worst miss') == 2, other)
    check(f'compare_with_is_a_list_in_the_pane {tag}: the focus is on Stop comparing after a pick',
          b.page('document.activeElement.id') == pid('compare-clear'), b.page(AT))
    shots = os.environ.get('WEB_CHECK_SHOTS')
    if shots:
        time.sleep(0.5)
        with open(os.path.join(shots, f'{theme}-preview.png'), 'wb') as out:
            out.write(b.shot())
        print(f'note a picture of the preview over {theme} is in {shots}')
    press_on(b, '#' + pid('compare-clear'))
    gone = poll(lambda: b.page("!document.querySelector('.fp-graph-compare')"), 15.0)
    check(f'compare_with_is_a_list_in_the_pane {tag}: Stop comparing takes it away, the focus on Compare with…',
          gone is True and b.page('document.activeElement.id') == pid('compare-open'), b.page(AT))

    # ---- a measurement with no file ----
    put_text(b, pid('query'), 'hd 650')
    b.press('Enter')
    poll(lambda: b.page("document.querySelectorAll('.fpe-variant').length") == 3, 6.0)
    press_on(b, variant(0, 2))
    said = poll(lambda: b.page("document.querySelector('#popup-eq-library-preview .fp-failed')?.textContent"), 15.0)
    check(f'a_measurement_that_is_not_there_says_so {tag}', bool(said) and 'does not list' in said, said)

    # ---- import ----
    made = []
    press_on(b, variant(0, 0))
    poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph')"), 15.0)
    posts = len(b.page(POSTS))
    press_on(b, '#' + pid('import-save'), 1.0)
    saved = poll(lambda: 'Saved' in (text_of(b, pid('import-words')) or ''), 15.0)
    listed = rows(d)
    made.append(HD650)
    words = text_of(b, pid('import-words'))
    check(f'import_saves_and_a_taken_name_is_worded_at_the_field {tag}: Save adds an imported preset, saved '
          'only', saved and HD650 in listed and listed[HD650]['imported'] is True
          and f'not on {DEVICE} yet' in words and any(p[0].endswith('/autoeq/import') for p in b.page(POSTS)[posts:]),
          [saved, list(listed), words])
    press_on(b, '#' + pid('import-save'), 1.0)
    poll(lambda: b.page("document.getElementById('popup-eq-library-import-name').getAttribute('aria-invalid')") == 'true', 15.0)
    field = b.page("(() => { const f = document.getElementById('popup-eq-library-import-name');"
                   " return [f.getAttribute('aria-invalid'), f.getAttribute('aria-describedby'),"
                   " document.activeElement === f, f.value]; })()")
    suggest = b.page("document.querySelector('[data-act=suggest]')?.textContent")
    check(f'import_saves_and_a_taken_name_is_worded_at_the_field {tag}: saving again is refused at the field with '
          'the suggestion', field == ['true', pid('import-words'), True, HD650]
          and suggest == f'Use “{HD650} (oratory1990)”', [field, suggest])
    press_on(b, '[data-act=suggest]')
    taken = b.page("document.getElementById('popup-eq-library-import-name').value")
    press_on(b, '#' + pid('import-save'), 1.0)
    saved = poll(lambda: HD650 + ' (oratory1990)' in rows(d), 15.0)
    made.append(f'{HD650} (oratory1990)')
    check(f'import_saves_and_a_taken_name_is_worded_at_the_field {tag}: the suggestion is taken and saved',
          taken == f'{HD650} (oratory1990)' and saved is True, [taken, list(rows(d))])

    # ---- the saved presets ----
    mine = b.page(f"__core.actions.savePreset('Mine', {{ device: __core.model.shown() }}).then((a) => a.ok)")
    time.sleep(0.5)
    made.append('Mine')
    press_on(b, '#' + f'popup-{NAME}-entry-saved')
    got = poll(lambda: b.page("document.querySelectorAll('.fpe-row').length") >= 3, 6.0)
    state = b.page("""[...document.querySelectorAll('.fpe-row')].map((r) => [
      r.querySelector('.fpe-row-name').textContent, r.querySelector('.fpe-where').textContent,
      r.querySelector('[data-act=overwrite]').getAttribute('aria-disabled'),
      r.querySelector('.fpe-why')?.textContent ?? null, r.textContent.includes('Imported'),
      r.textContent.includes('Built in')])""")
    by_name = {row[0]: row for row in state}
    check(f'saved_presets_say_where_they_live_and_overwrite_asks_once {tag}: the marks and the reasons',
          mine is True and by_name[HD650][4] and by_name[HD650][2] == 'true'
          and 'is imported' in (by_name[HD650][3] or '') and by_name['Mine'][2] is None
          and by_name['Mine'][1] in ('In the library', f'On {DEVICE} now')
          and any(row[5] and row[2] == 'true' and 'is built in' in (row[3] or '') for row in state), state)
    check(f'saved_presets_say_where_they_live_and_overwrite_asks_once {tag}: nothing says "saved on the device"',
          'saved on the device' not in text_of(b, 'popup-eq-library'))
    index = [i for i, row in enumerate(state) if row[0] == 'Mine'][0]
    posts = len(b.page(POSTS))
    press_on(b, f'#popup-{NAME}-overwrite-{index}')
    asked = b.page("[document.activeElement.dataset.answer, document.querySelectorAll('.fi-askbox').length]")
    press_on(b, '[data-answer=keep]')
    kept = [p for p in b.page(POSTS)[posts:] if p[0].endswith('/presets')]
    press_on(b, f'#popup-{NAME}-overwrite-{index}')
    press_on(b, '[data-answer=overwrite]', 1.0)
    sent = [json.loads(p[1]) for p in b.page(POSTS)[posts:] if p[0].endswith('/presets')]
    said = poll(lambda: text_of(b, pid('saved-words')), 6.0)
    check(f'saved_presets_say_where_they_live_and_overwrite_asks_once {tag}: one question with Keep focused, Keep '
          'sends nothing, Overwrite sends force', asked == ['keep', 1] and kept == []
          and sent == [{'name': 'Mine', 'force': True}] and 'now holds the curve that is on' in (said or ''),
          [asked, kept, sent, said])

    # ---- the upload ----
    press_on(b, '#' + f'popup-{NAME}-entry-upload')
    up = b.page("(() => { const s = document.getElementById('popup-eq-library-upload-save');"
                " return [s.getAttribute('aria-disabled'), s.disabled,"
                " document.getElementById('popup-eq-library-upload-words').textContent]; })()")
    check(f'upload_reads_a_file_in_the_page {tag}: Save is disabled with its reason before a file',
          up == ['true', False, 'Choose a file first.'], up)

    def choose(text, name):
        b.page("(() => { const dt = new DataTransfer();"
               f" dt.items.add(new File([{json.dumps(text)}], {json.dumps(name)}, {{ type: 'text/plain' }}));"
               " const input = document.getElementById('popup-eq-library-file'); input.files = dt.files;"
               " input.dispatchEvent(new Event('change', { bubbles: true })); return 1; })()")
        return poll(lambda: b.page("document.getElementById('popup-eq-library-upload-save')"
                                   ".getAttribute('aria-disabled')") is None, 4.0)

    choose('this is not a parametric eq', 'Bad file.txt')
    press_on(b, '#' + pid('upload-save'), 1.0)
    refused = poll(lambda: text_of(b, pid('upload-words')) not in (None, '', 'Bad file read.'), 6.0)
    words = text_of(b, pid('upload-words'))
    check(f'upload_reads_a_file_in_the_page {tag}: a text the daemon cannot read is refused in its words',
          refused and 'cannot read' in words and len(rows(d)) == len(listed) + 2, words)
    choose('Preamp: -2.5 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2 dB Q 1.0\n', 'Good file.txt')
    name = b.page("document.getElementById('popup-eq-library-upload-name').value")
    press_on(b, '#' + pid('upload-save'), 1.0)
    saved = poll(lambda: 'Good file' in rows(d), 6.0)
    made.append('Good file')
    check(f'upload_reads_a_file_in_the_page {tag}: a good file is named after itself and saved',
          name == 'Good file' and saved is True and 'Saved' in text_of(b, pid('upload-words')), [name, list(rows(d))])

    # ---- the layout ----
    press_on(b, '#' + f'popup-{NAME}-entry-autoeq')
    r = rects(b, NAME)
    check(f'nothing_scrolls_sideways {tag}: at 1280 px', r['over'] == [0, 0], r['over'])
    close_popup(b, NAME)
    for name in made:
        d.post(DEVICE, 'presets/delete', {'name': name})
    time.sleep(0.5)
    left = [n for n in rows(d) if n in made]
    check(f'every preset the case made is removed {tag}', left == [], left)


def phone(b, d, theme):
    tag = f'({theme})'
    b.resize(*PHONE)
    time.sleep(0.4)
    b.page(BODY)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    opened = by_the_menu(b, NAME)
    r0 = rects(b, NAME)
    list_only = r0['side'] and r0['side']['shown'] and (not r0['page'] or not r0['page']['shown'])
    check(f'nothing_scrolls_sideways {tag}: on a phone the popup opens at its list, the page pane not shown',
          opened is True and list_only and r0['over'] == [0, 0], r0)
    press_on(b, f'#popup-{NAME}-entry-autoeq')
    put_text(b, pid('query'), 'hd 650')
    b.press('Enter')
    poll(lambda: b.page("document.querySelectorAll('.fpe-group').length") == 1, 8.0)
    press_on(b, variant(0, 0))
    poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph')"), 15.0)
    r1 = rects(b, NAME)
    page_only = r1['page'] and r1['page']['shown'] and (not r1['side'] or not r1['side']['shown'])
    graph = b.page("document.querySelector('.fp-graph').getBoundingClientRect().width")
    check(f'nothing_scrolls_sideways {tag}: on a phone a page shows alone, its graph fits and nothing '
          'scrolls sideways', page_only and r1['over'] == [0, 0] and 150 < graph <= PHONE[0], [r1, graph])
    press_on(b, f'#popup-{NAME}-back')
    back = rects(b, NAME)
    check(f'nothing_scrolls_sideways {tag}: the way back is the list again',
          back['side'] and back['side']['shown'], back)
    close_popup(b, NAME)
    b.resize(*DESK)
    time.sleep(0.4)


def computer(b, d, theme):
    tag = f'({theme})'
    b.page(BODY)
    side = b.page("(__core.computer.flip('computer'), __core.computer.side())")
    time.sleep(0.6)
    b.page("(document.getElementById('fx-button').focus(), 1)")
    opened = by_the_menu(b, NAME)
    named = b.page(f"document.getElementById('{pid('title')}').textContent")
    check(f'the_computer_side {tag}: the title names the computer', side == 'computer' and opened is True
          and named == 'EQ library · This computer', [side, named])
    search(b, 'hd 650')
    press_on(b, variant(0, 0))
    shown = poll(lambda: b.page("!!document.querySelector('#popup-eq-library-preview .fp-graph')"), 15.0)
    pane = b.page("[document.getElementById('popup-eq-library-preview').textContent,"
                  " !!document.querySelector('#popup-eq-library-preview .fp-graph-series')]")
    check(f'the_computer_side {tag}: the preview is the correction as the computer plays it, with no fit',
          shown is True and 'as the computer plays it' in pane[0] and 'Worst miss' not in pane[0]
          and pane[1] is False, pane)
    press_on(b, '#' + f'popup-{NAME}-entry-saved')
    poll(lambda: b.page("document.querySelectorAll('.fpe-row').length") >= 1, 6.0)
    state = b.page("[...document.querySelectorAll('[data-act=overwrite]')].map((e) => e.getAttribute('aria-disabled'))")
    note = text_of(b, pid('saved-note'))
    check(f'the_computer_side {tag}: Overwrite is disabled on every row, and says why',
          state and all(one == 'true' for one in state) and 'not over one' in (note or ''), [state, note])
    close_popup(b, NAME)
    b.page("(__core.computer.flip('dac'), 1)")
    time.sleep(0.4)


def no_way_in(b, theme, who):
    tag = f'a_guest_and_a_read_only_page_have_no_way_in ({theme}): {who}'
    b.page('(__core.frame.openMenu(), 1)')
    time.sleep(0.3)
    rows = b.page("[...document.querySelectorAll('#fx-menu [id^=\"menu-popup-\"]')].map((e) => e.id)")
    b.page('(__core.frame.closeMenu(), 1)')
    b.page("(__core.router.open('eq-library'), 1)")
    time.sleep(0.5)
    posts = [p for p in b.page(POSTS) if '/autoeq/' in p[0]]
    check(f'{tag}: the Devices row only (the read-only popup), no EQ library row, no popup and no request',
          rows == ['menu-popup-devices', 'menu-popup-help'] and b.page("!document.getElementById('popup-eq-library')") is True
          and posts == [], [rows, posts])


def main():
    """Every case, in every theme; exits 1 on a failure."""
    fx = common.AutoEqFixture()
    d = common.Daemon(host='fake', env={'SVOLDER_AUTOEQ_URL': fx.url})
    try:
        for theme in THEMES:
            b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
            try:
                b.preload(modal.HOOK)
                load(b, d, theme)
                case(b, d, fx, theme)
                phone(b, d, theme)
                computer(b, d, theme)
            finally:
                b.close()
        check('nothing but the fixture was asked for', all(p.startswith('/results/') for p in fx.requests),
              fx.requests)
        for who, web in (('a guest', 'guest = "read"'), ('writes off', 'auth = "none"\nwrites = false')):
            other = common.Daemon(host='fake', web=web, env={'SVOLDER_AUTOEQ_URL': fx.url})
            try:
                for theme in THEMES:
                    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=DESK)
                    try:
                        b.preload(modal.HOOK)
                        load(b, other, theme)
                        b.page(BODY)
                        no_way_in(b, theme, who)
                    finally:
                        b.close()
            finally:
                other.stop()
    finally:
        d.stop()
        fx.stop()
    print(f'eq_library.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
