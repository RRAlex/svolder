# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The tone graph's overlay of an imported preset (the target, the gap and the fit) in a browser,
in every theme: what no node harness can see (the theme's own colours, the computed strokes, a
pointer that still reaches the points, the canvas of Night drive, the budget's order). The
drawing's logic is node's (plot.mjs, ember-parts.mjs, signal-draw.mjs, paper-draw.mjs,
nightdrive-*.mjs). The daemon is a fake (`Gungnir 2`, AutoEq at nothing: the preset is uploaded
as text) with a preset that ten narrow filters make, which the device's five bands cannot follow,
so the daemon serves a target, a fit and several gap spans for it. Every number drawn is
compared with the model route's own `tone.overlay`.

Per theme (Ember, Signal and Paper at 1280 px and again at 390 px, Paper also in its dark scheme):
  the_overlay_is_the_daemons_numbers     one shaded shape per gap span the daemon serves, the
                                        target's line, the fit in words as the daemon's RMS
  the_overlay_wears_the_themes_tokens    the target is a dashed stroke in `--frame-graph-target`,
                                        the gap a fill in `--frame-graph-gap`, neither takes a
                                        pointer
  a_pointer_still_reaches_the_points     (Ember, Signal, Paper) the element at a band's point is
                                        the point, not the overlay
  no_overlay_without_an_imported_preset  recalling a built-in preset takes every shape and the
                                        words away; recalling the imported one brings them back
  the_overlay_has_words_for_a_reader     a title and a hidden sentence say the fit, the daemon's worst miss and
                                        what the dotted line and the shade are
  an_edited_band_moves_the_overlay       (Ember, Signal, Paper) twelve gain steps on a band: the shapes and the fit
                                        words are the daemon's new numbers (the fit follows the curve, not the
                                        preset's own fit)
  nothing_scrolls_sideways               the page is no wider than the window
Night drive instead:
  the_fit_stands_in_the_hud              `fit X dB` in the EQ line, the daemon's RMS
  the_ghost_ridge_and_the_tint_are_painted   the road canvas has more of the target's colour, and of the
                                        ground tinted by the gap, than with `nd-debug=nooverlay`
  the_hud_keeps_the_units_case           the computed text-transform of the fit is none (`fit 0.9 dB`, not FIT 0.9 DB)
  the_budget_drops_the_overlay_second    sixty slow draws drop the wet road, sixty more the
                                        overlay, the density is still whole
With WEB_CHECK_SHOTS=<folder> `<theme>.png` (the desktop, the imported preset on) and
`nightdrive-phone.png` are written there.
Usage: python3 tone_overlay.py chromium|firefox"""
import json
import os
import sys
import time

import common
import modal
from modal import check, load, poll
from eq_library import api
from popups import DESK, PHONE

browser = modal.browser
DEVICE = 'Gungnir 2'
NAME = 'Wide'

# ten narrow filters of strong gains: the five bands cannot make them, so the fit misses by more
# than the daemon's 2 dB in several places
FILTERS = [(60, 8, 3), (120, -7, 4), (300, 7, 5), (700, -8, 6), (1500, 8, 6), (3000, -7, 7),
           (5000, 8, 8), (8000, -8, 9), (11000, 7, 8), (14000, -6, 7)]
TEXT = '\n'.join(['Preamp: -6 dB'] + [
    f'Filter {i + 1}: ON PK Fc {f} Hz Gain {g}.0 dB Q {q}.00' for i, (f, g, q) in enumerate(FILTERS)])

# what each theme names its parts: the graph's box, the target, the gap, the fit's words, a point
PARTS = {
    'ember': dict(box='#part-tone .eqg', target='.tgt', gap='.gap', fit='.fitt', point='.mk'),
    'signal': dict(box='#sg-graph', target='.sg-target', gap='.sg-gap', fit='.sg-fit', point='.sg-pt'),
    'paper': dict(box='#p-graph', target='.p-target', gap='.p-gap', fit='.p-fit', point='.pt'),
}


def put_wide_on(d, device=DEVICE):
    """Upload the preset and recall it: the device's tone view carries its overlay from now on.
    Returns the overlay the model route serves."""
    code, said = api(d, 'api/v1/autoeq/upload', {'text': TEXT, 'name': NAME})
    assert code == 200, said
    code, said = d.post(device, 'presets/recall', {'name': NAME})
    assert code == 200, said
    return d.model(device)['tone']['overlay']


def words_of(overlay):
    return f"fit {overlay['rms_db']:.1f} dB"


def overlay_now(d):
    return d.model(DEVICE)['tone'].get('overlay')


def desktop(b, theme, d, overlay, shots, where=''):
    p = PARTS[theme]
    tag = f'({theme}{where})'
    ready = poll(lambda: b.page(f"!!document.querySelector({json.dumps(p['box'] + ' ' + p['target'])})"), 8.0)
    spans = len(overlay['gap'])
    seen = b.page(f"""(() => {{ const box = document.querySelector({json.dumps(p['box'])});
      const t = box.querySelector({json.dumps(p['target'])});
      return {{ gaps: box.querySelectorAll({json.dumps(p['gap'])}).length,
        d: t?.getAttribute('d')?.length ?? 0,
        words: (box.parentElement.querySelector({json.dumps(p['fit'])}) ?? box.querySelector({json.dumps(p['fit'])}))
          ?.textContent ?? null }}; }})()""")
    check(f'the_overlay_is_the_daemons_numbers {tag}: one shape per gap span ({spans}), the target a line',
          ready is True and spans > 1 and seen['gaps'] == spans and seen['d'] > 100, [spans, seen])
    check(f'the_overlay_is_the_daemons_numbers {tag}: the fit is the daemon\'s RMS in words',
          seen['words'] == words_of(overlay), [seen['words'], words_of(overlay)])
    css = b.page(f"""(() => {{ const box = document.querySelector({json.dumps(p['box'])});
      const probe = document.createElement('i');
      probe.style.cssText = 'position:fixed;left:-99px;color:var(--frame-graph-target);fill:var(--frame-graph-gap)';
      document.body.append(probe); const want = getComputedStyle(probe);
      const t = getComputedStyle(box.querySelector({json.dumps(p['target'])}));
      const g = getComputedStyle(box.querySelector({json.dumps(p['gap'])}));
      const out = {{ stroke: t.stroke, wantStroke: want.color, dash: t.strokeDasharray, tev: t.pointerEvents,
        fill: g.fill, wantFill: want.fill, gev: g.pointerEvents, gstroke: g.stroke }};
      probe.remove(); return out; }})()""")
    check(f'the_overlay_wears_the_themes_tokens {tag}: the target is a dashed stroke in the target token',
          css['stroke'] == css['wantStroke'] and css['dash'] not in (None, 'none')
          and css['tev'] == 'none', css)
    check(f'the_overlay_wears_the_themes_tokens {tag}: the gap is a fill in the gap token, no stroke',
          css['fill'] == css['wantFill'] and css['gstroke'] == 'none' and css['gev'] == 'none', css)
    b.page(f"(document.querySelector({json.dumps(p['box'])}).scrollIntoView({{ block: 'center' }}), 1)")
    time.sleep(0.4)
    hit = b.page(f"""(() => {{ const pt = document.querySelector({json.dumps(p['box'] + ' ' + p['point'])});
      const r = pt.getBoundingClientRect(); const e = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
      return [e === pt || pt.contains(e), e?.getAttribute('class') ?? e?.tagName]; }})()""")
    check(f'a_pointer_still_reaches_the_points {tag}', hit[0] is True, hit)
    said = b.page(f"""(() => {{ const box = document.querySelector({json.dumps(p['box'])});
      const w = box.querySelector({json.dumps(p['fit'])}); const sr = box.querySelector('[class$="sr"]');
      return [w?.getAttribute('title') ?? null, sr?.textContent ?? null]; }})()""")
    worst = overlay['worst_hz']
    at = f'{worst / 1000:.1f} kHz' if worst >= 1000 else f'{round(worst)} Hz'
    note = (f"{words_of(overlay)}, worst {overlay['worst_db']:.1f} dB at {at}. The dotted line is the target; "
            f"the shade is where the bands miss it by more than {overlay['gap_db']:g} dB.")
    check(f'the_overlay_has_words_for_a_reader {tag}: the title and the hidden sentence', said == [note, note], [said, note])
    hidden = b.page(f"document.querySelector({json.dumps(p['box'] + ' ' + p['fit'])})?.getAttribute('aria-hidden')")
    check(f'the_overlay_has_words_for_a_reader {tag}: the visible words are hidden from a screen reader (the sentence says them)',
          hidden == 'true', hidden)
    if shots and not where:
        b.page(f"(document.querySelector({json.dumps(p['box'])}).scrollIntoView({{ block: 'center' }}), 1)")
        time.sleep(0.6)
        with open(os.path.join(shots, f'{theme}.png'), 'wb') as out:
            out.write(b.shot())


def without(b, theme, d, overlay):
    p = PARTS[theme]
    tag = f'({theme})'
    count = (f"document.querySelectorAll({json.dumps(p['box'] + ' ' + p['gap'])}).length"
             f" + document.querySelectorAll({json.dumps(p['box'] + ' ' + p['target'])}).length"
             f" + (document.querySelector({json.dumps(p['fit'])}) ? 1 : 0)")
    high = 'document.documentElement.scrollHeight'
    with_it = b.page(high)
    code, _ = d.post(DEVICE, 'presets/recall', {'name': 'Warm'})
    gone = poll(lambda: b.page(count) == 0, 8.0)
    time.sleep(0.4)
    # the recall itself adds a `last change` note, which can open a new row of notes; that is the
    # note's doing, not the overlay's, so the height is read with that note taken out
    after = b.page("(document.querySelectorAll('[data-note=\"change\"]').forEach((e) => e.remove()), "
                   + high + ')')
    check(f'no_overlay_without_an_imported_preset {tag}: the page keeps its height when the overlay goes (no jump)',
          abs(after - with_it) <= 1 or theme == 'ember', [with_it, after])
    check(f'no_overlay_without_an_imported_preset {tag}: a built-in preset takes every shape and the words away',
          code == 200 and gone is True, b.page(count))
    code, _ = d.post(DEVICE, 'presets/recall', {'name': NAME})
    back = poll(lambda: b.page(count) > 2, 8.0)
    check(f'no_overlay_without_an_imported_preset {tag}: the imported preset brings them back',
          code == 200 and back, b.page(count))


def edited(b, theme, d, overlay):
    """Twelve gain steps on band 3: the daemon's overlay now is the one of the bands on the device, and the page draws
    those numbers, not the preset's own fit."""
    p = PARTS[theme]
    tag = f'({theme})'
    for _ in range(12):
        code, _ = d.post(DEVICE, 'eq/bands/3', {'param': 'g', 'step': 1})
        assert code == 200, code
    now = overlay_now(d)
    count = f"document.querySelectorAll({json.dumps(p['box'] + ' ' + p['gap'])}).length"
    words = f"document.querySelector({json.dumps(p['box'] + ' ' + p['fit'])})?.textContent ?? null"
    moved = now['gap'] != overlay['gap'] or now['rms_db'] != overlay['rms_db']
    followed = poll(lambda: b.page(count) == len(now['gap']) and b.page(words) == words_of(now), 8.0)
    check(f'an_edited_band_moves_the_overlay {tag}: the daemon\'s numbers changed with the band', moved,
          [overlay['rms_db'], now['rms_db']])
    check(f'an_edited_band_moves_the_overlay {tag}: the shapes and the words are the new numbers',
          followed is True, [b.page(count), len(now['gap']), b.page(words), words_of(now)])
    d.post(DEVICE, 'presets/recall', {'name': NAME})
    poll(lambda: b.page(count) == len(overlay['gap']) and b.page(words) == words_of(overlay), 8.0)


def sideways(b, tag, query):
    wide = b.page('[document.documentElement.scrollWidth, innerWidth]')
    check(f'nothing_scrolls_sideways {tag} {query}', wide[0] <= wide[1] + 1, wide)


def ground(b):
    """How many pixels of the road canvas are near the target's colour (255, 179, 236)."""
    return b.page("""(() => { const c = document.getElementById('nd-road'); const g = c.getContext('2d');
      const px = g.getImageData(0, 0, c.width, c.height).data; let n = 0;
      for (let i = 0; i < px.length; i += 4) {
        if (px[i + 3] > 40 && Math.abs(px[i] - 255) < 10 && Math.abs(px[i + 1] - 179) < 25 && Math.abs(px[i + 2] - 236) < 18) n++; }
      return n; })()""")


def tinted(b):
    """How many fills of the tint's colour (`rgb(255, 209, 102)`) the scene makes in one second: the road canvas's own
    2D context is asked, so what the floor's heat lays over the tint cannot hide it."""
    b.page("""(() => { if (!window.__tint) { window.__tint = { n: 0 };
      const real = CanvasRenderingContext2D.prototype.fill;
      CanvasRenderingContext2D.prototype.fill = function (...a) {
        if (this.fillStyle === 'rgb(255, 209, 102)' || this.fillStyle === '#ffd166') window.__tint.n++;
        return real.apply(this, a); }; }
      window.__tint.n = 0; return 1; })()""")
    time.sleep(1.0)
    return b.page('window.__tint.n')


def nightdrive(b, d, overlay, shots, make):
    tag = '(nightdrive)'
    q = '?theme=nightdrive&effects=off'
    b.goto(f'{d.url}{q}', settle=1.0)
    b.page("import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => { await m.mounted;"
           " window.__core = m.core; return 1; })")
    time.sleep(2.0)
    words = poll(lambda: b.page("document.querySelector('.nd-eq .nd-fit')?.textContent ?? null"), 8.0)
    check(f'the_fit_stands_in_the_hud {tag}: `fit X dB` is the daemon\'s RMS', words == words_of(overlay), words)
    perf = b.page('__core.theme.driver.perf()')
    on = ground(b)
    tint_on = tinted(b)
    unit = b.page("getComputedStyle(document.querySelector('.nd-eq .nd-fit')).textTransform")
    sentence = b.page("document.querySelector('.nd-eq .nd-sr')?.textContent ?? null")
    check(f'the_fit_stands_in_the_hud {tag}: a hidden sentence says the worst miss and the key, the visible words are hidden',
          isinstance(sentence, str) and sentence.startswith(words_of(overlay)) and ', worst ' in sentence
          and b.page("document.querySelector('.nd-eq .nd-fit').getAttribute('aria-hidden')") == 'true', sentence)
    check(f'the_hud_keeps_the_units_case {tag}: the fit is not upper-cased', unit == 'none', unit)
    check(f'the_budget_drops_the_overlay_second {tag}: a fresh scene draws the overlay', perf['overlay'] is True, perf)
    b.page('__core.theme.driver.forceStepDown()')
    first = b.page('__core.theme.driver.perf()')
    b.page('__core.theme.driver.forceStepDown()')
    second = b.page('__core.theme.driver.perf()')
    check(f'the_budget_drops_the_overlay_second {tag}: sixty slow draws drop the wet road, sixty more the overlay, '
          'the density stays', first['wet'] is False and first['overlay'] is True
          and second['overlay'] is False and second['step'] == 0, [first, second])
    # the overlay's own query: the same scene without the layer
    b.goto(f'{d.url}?theme=nightdrive&effects=off&nd-debug=nooverlay', settle=1.0)
    b.page("import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => { await m.mounted;"
           " window.__core = m.core; return 1; })")
    time.sleep(2.0)
    off = ground(b)
    tint_off = tinted(b)
    perf_off = b.page('__core.theme.driver.perf()')
    check(f'the_ghost_ridge_and_the_tint_are_painted {tag}: the road holds more of the target\'s colour than without',
          on > off + 20 and perf_off['overlay'] is False, [on, off, perf_off])
    check(f'the_ghost_ridge_and_the_tint_are_painted {tag}: the floor is filled with the tint only with the overlay (fills of its colour in a second)',
          tint_on > 20 and tint_off == 0, [tint_on, tint_off])
    sideways(b, tag, 'desktop')
    if shots:
        b.goto(f'{d.url}?theme=nightdrive&effects=off', settle=1.0)
        time.sleep(2.5)
        with open(os.path.join(shots, 'nightdrive.png'), 'wb') as out:
            out.write(b.shot())
        small = make(PHONE)
        try:
            small.goto(f'{d.url}?theme=nightdrive&effects=off', settle=1.0)
            time.sleep(2.5)
            with open(os.path.join(shots, 'nightdrive-phone.png'), 'wb') as out:
                out.write(small.shot())
        finally:
            small.close()


def main():
    """Every case, in every theme; exits 1 on a failure."""
    make = lambda size: (common.Chromium if browser == 'chromium' else common.Firefox)(size=size)  # noqa: E731
    shots = os.environ.get('WEB_CHECK_SHOTS')
    d = common.Daemon(host='fake', env={'SVOLDER_AUTOEQ_URL': common.NO_AUTOEQ})
    try:
        overlay = put_wide_on(d)
        check('the daemon serves an overlay with several gap spans', len(overlay['gap']) > 1 and overlay['rms_db'] > 0,
              {k: v for k, v in overlay.items() if not isinstance(v, list)})
        for theme in modal.THEMES:
            b = make(DESK)
            try:
                b.preload(modal.HOOK)
                if theme == 'nightdrive':
                    nightdrive(b, d, overlay, shots, make)
                else:
                    load(b, d, theme)
                    desktop(b, theme, d, overlay, shots)
                    without(b, theme, d, overlay)
                    edited(b, theme, d, overlay)
                    sideways(b, f'({theme})', 'desktop')
                    if theme == 'paper':
                        b.page("__core.scheme.set('dark')")
                        time.sleep(0.6)
                        desktop(b, theme, d, overlay, None, ' dark')
                        dark = b.page("""(() => { const box = document.querySelector('#p-graph');
                          return [getComputedStyle(box.querySelector('.p-target')).stroke,
                                  getComputedStyle(box.querySelector('.p-gap')).fill]; })()""")
                        check('the_overlay_wears_the_themes_tokens (paper dark): the dark scheme\'s own colours',
                              dark == ['rgb(236, 232, 223)', 'rgba(214, 178, 106, 0.3)'], dark)
                        b.page("__core.scheme.set('light')")
                        time.sleep(0.4)
                    b.resize(*PHONE)
                    time.sleep(0.8)
                    desktop(b, theme, d, overlay, None, ' phone')
                    sideways(b, f'({theme})', 'phone')
            finally:
                b.close()
    finally:
        d.stop()
    print(f'tone_overlay.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
