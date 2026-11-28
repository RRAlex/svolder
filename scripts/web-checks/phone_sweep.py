# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The page and every popup at phone widths, in a browser, in every theme and every scheme the theme has:
nothing scrolls sideways, nothing is cut, and every control is a target a thumb can hit. The states are
those of sweep_states.py (the page, the menu, each listed popup on each entry, a device's page with its
forms and lists open, a dimmed device, the add walk's pages, the config-changed line, the EQ library's
preview, a guest's and a read-only page, no device at all). The browser is put at the width first and the page
is loaded there (what the page and its popups place in script is laid out at the phone's width, never at a
desk's and then stretched): 360 px for every theme, scheme and visitor; 390 px and, as the one tablet width,
768 px for the writer's and the guest's pages in the theme's first scheme (the layout does not depend on the
scheme, and the wall time is the budget).
  page-scroll   the page is wider than the window (`scrollWidth` against `innerWidth`)
  popup-scroll  the open popup's box is wider than itself (its `scrollWidth` against `clientWidth`)
  pane-scroll   any box in the page or the popup that scrolls sideways (`overflow-x` auto or scroll) holds more
                than it shows: the popup's page pane and side list included, where a 700 px element would
                otherwise hide inside a box that "scrolls"; a carousel that scrolls on purpose is named in
                SCROLLS with its reason
  cut-viewport  an element's box runs past the window's left or right edge (and is not in a box that
                scrolls sideways)
  cut-ancestor  an element's box runs past an ancestor that clips (`overflow` hidden or clip) and so cannot be
                scrolled to
  target        a control under 24 px wide or high (WCAG 2.2, 2.5.8 minimum) that has no 24 px of hit area
                around its middle and has another control within 24 px of it
The baseline (phone-baseline.json) holds what fails today with its measure (the overshoot, the size): a
failure that is not in it, or one that got worse (by more than 1 px), fails the check; `--update-baseline`
rewrites it from this run (Chromium). A planted 700 px element in Help's pane is part of every run: the check
fails if the sweep does not report it. Each theme runs in a child process of its own (WEB_SWEEP_PAR limits
how many at once). With WEB_CHECK_SHOTS=<folder> one picture per failing page is written
there. What the check cannot see: text clipped by its own box with no element to blame (an ellipsis is on
purpose), content that wraps wrongly but fits, a control hidden behind another (`elementFromPoint` is read
at the middle only), the real on-screen keyboard, a phone's own safe areas and zoom.
Usage: python3 phone_sweep.py chromium|firefox [--update-baseline]"""
import json
import os
import sys
import time

import common
import modal
import sweep_states as S
from modal import check, poll

HERE = os.path.dirname(os.path.abspath(__file__))
TARGET_MIN = 24
# a box that scrolls sideways on purpose: `signature -> why` (every other one found is a failure)
SCROLLS = {
    'div.fp-tabs': 'the Sound popup\'s four tabs run in one line that scrolls sideways (the shown tab is scrolled into view)',
}
SLACK = 1.0

MEASURE = r"""(() => {
  const iw = innerWidth;
  const scopeSel = document.querySelector('.fp') ? '.fp' : (document.getElementById('fx-menu')?.hidden === false ? '#fx-menu' : null);
  const scope = scopeSel ? document.querySelector(scopeSel) : document.body;
  const visible = (e) => {
    if (e.checkVisibility && !e.checkVisibility({ visibilityProperty: true })) return false;
    const r = e.getBoundingClientRect();
    return r.width >= 3 && r.height >= 3;
  };
  const sig = (e) => {
    const tag = e.tagName.toLowerCase();
    const id = e.id && !/\d{3,}/.test(e.id) ? '#' + e.id : '';
    const cls = id ? '' : [...e.classList].filter((c) => !/\d/.test(c)).sort().join('.');
    return tag + id + (cls ? '.' + cls : '');
  };
  const sideways = (cs) => /^(auto|scroll)$/.test(cs.overflowX);
  const clips = (cs) => /^(hidden|clip)$/.test(cs.overflowX);
  const out = { scope: scopeSel, iw, pageScroll: document.documentElement.scrollWidth - iw, cuts: [], targets: [], popupScroll: 0 };
  const box = document.querySelector('.fp .fp-box');
  if (box) out.popupScroll = box.scrollWidth - box.clientWidth;
  for (const e of [scope, ...scope.querySelectorAll('*')]) {
    if (e instanceof SVGElement && e.tagName !== 'svg') continue;
    if (!visible(e)) continue;
    const r = e.getBoundingClientRect();
    const cs = getComputedStyle(e);
    if (sideways(cs) && e.scrollWidth > e.clientWidth + 1) out.cuts.push({ sig: sig(e), kind: 'pane-scroll', over: e.scrollWidth - e.clientWidth });
    if (e === scope) continue;
    if (cs.position === 'fixed' && (r.right < 0 || r.left > iw)) continue;
    // the nearest box that scrolls sideways holds what is past its edge; one that clips cuts it
    let scrolls = false, cutBy = null;
    for (let n = e.parentElement; n && n !== document.documentElement; n = n.parentElement) {
      const ns = getComputedStyle(n);
      const nr = n.getBoundingClientRect();
      if (sideways(ns)) { scrolls = true; break; }
      if (clips(ns) && nr.width > 0 && (r.right > nr.right + 1.5 || r.left < nr.left - 1.5) && n !== document.body) { cutBy = [sig(n), Math.max(r.right - nr.right, nr.left - r.left)]; break; }
    }
    if (cutBy) out.cuts.push({ sig: sig(e), kind: 'cut-ancestor', by: cutBy[0], over: cutBy[1] });
    else if (!scrolls && (r.right > iw + 1.5 || r.left < -1.5)) out.cuts.push({ sig: sig(e), kind: 'cut-viewport', over: Math.max(r.right - iw, -r.left) });
  }
  // the controls: a box 24 px wide and high, or 24 px of hit area around its middle, or no other control near
  const SEL = 'a[href], button, input:not([type=hidden]), select, textarea, summary, [role=button], [role=tab], [role=menuitem], [role=slider], [role=switch]';
  const controls = [...scope.querySelectorAll(SEL)].filter((e) => visible(e) && !e.disabled && getComputedStyle(e).display !== 'inline');
  const rectOf = (e) => {
    const rs = [e.getBoundingClientRect()];
    if (e.labels) for (const l of e.labels) rs.push(l.getBoundingClientRect());
    return rs.reduce((a, r) => ({ left: Math.min(a.left, r.left), top: Math.min(a.top, r.top), right: Math.max(a.right, r.right), bottom: Math.max(a.bottom, r.bottom) }),
      { left: 1e9, top: 1e9, right: -1e9, bottom: -1e9 });
  };
  const rects = controls.map((e) => [e, rectOf(e)]);
  for (const [e, r] of rects) {
    const w = r.right - r.left, h = r.bottom - r.top;
    if (w >= 23.5 && h >= 23.5) continue;
    const cx = (r.left + r.right) / 2, cy = (r.top + r.bottom) / 2;
    const hit = (x, y) => { const t = document.elementFromPoint(x, y); return !!t && (t === e || e.contains(t) || (e.labels && [...e.labels].some((l) => l.contains(t)))); };
    const wide = w >= 23.5 || (hit(cx - 11.5, cy) && hit(cx + 11.5, cy));
    const tall = h >= 23.5 || (hit(cx, cy - 11.5) && hit(cx, cy + 11.5));
    if (wide && tall) continue;
    // the spacing exception: a circle of 24 px around its middle that no other control touches
    const near = rects.some(([o, q]) => {
      if (o === e || e.contains(o) || o.contains(e)) return false;
      const dx = Math.max(q.left - cx, 0, cx - q.right), dy = Math.max(q.top - cy, 0, cy - q.bottom);
      if (Math.hypot(dx, dy) < 12) return true;
      // two undersized targets whose 24 px circles overlap (centres under 24 px apart)
      const small = (q.right - q.left < 23.5 || q.bottom - q.top < 23.5);
      return small && Math.hypot((q.left + q.right) / 2 - cx, (q.top + q.bottom) / 2 - cy) < 24;
    });
    if (near) out.targets.push({ sig: sig(e), w: Math.round(w * 10) / 10, h: Math.round(h * 10) / 10, what: (e.value || e.textContent || e.getAttribute('aria-label') || '').trim().slice(0, 20) });
  }
  return out;
})()"""


def verdict(key, got, baseline, worse_is_more=True):
    """`ok` (a measure of nothing), `known` (in the baseline, not worse), `worse` or `new`."""
    if key not in baseline:
        return 'new'
    base = baseline[key]
    # text widths differ a few px between the engines: 5 % and a px of room
    worse = got > base * 1.05 + SLACK if worse_is_more else got < base - 0.5
    return 'worse' if worse else 'known'


def self_test():
    bad = []
    base = {'a': 10.0}
    for got, want in [(verdict('a', 10.5, base), 'known'), (verdict('a', 12.0, base), 'worse'),
                      (verdict('b', 1.0, base), 'new'), (verdict('a', 20.0, {'a': 24.0}, False), 'worse'),
                      (verdict('a', 24.0, {'a': 24.0}, False), 'known')]:
        if got != want:
            bad.append(f'verdict: got {got}, want {want}')
    for line in bad:
        print('self-test:', line)
    print('phone_sweep self-test:', 'FAILED' if bad else 'ok')
    return 1 if bad else 0


class Run:
    def __init__(self, baseline):
        self.baseline = baseline
        self.found = {}   # key -> measure (px over, or the smaller side of a target)
        self.where = {}
        self.states = 0

    def add(self, key, measure, where):
        mine = self.found.get(key)
        worse = mine is None or (measure < mine if key.endswith('|target') else measure > mine)
        if worse:
            self.found[key] = measure
            self.where[key] = where


def analyse(b, run, tag, state, width):
    got = b.page(MEASURE)
    group = state.split('/')[0]
    base = f'{tag}|{group}|{width}'
    run.states += 1
    where = f'{state} at {width}'
    if got['pageScroll'] > 1:
        run.add(f'{base}|page|page-scroll', got['pageScroll'], where)
    if got['popupScroll'] > 1:
        run.add(f'{base}|{got["scope"]}|popup-scroll', got['popupScroll'], where)
    for c in got['cuts']:
        if c['kind'] == 'pane-scroll' and c['sig'] in SCROLLS:
            continue
        run.add(f'{base}|{c["sig"]}|{c["kind"]}', c['over'], where)
    for t in got['targets']:
        run.add(f'{base}|{t["sig"]}|target', min(t['w'], t['h']), f'{where} ({t["what"]!r})')
    return got


PASSES = [(360, 780, None, None), (390, 800, 1, 'writer,guest'), (768, 1024, 1, 'writer,guest')]


def set_width(b, w, h, tag):
    """The browser at `w` px wide, said so by the window itself (a browser that clamps the viewport must not sweep
    at another width unseen)."""
    b.resize(w, h)
    got = poll(lambda: b.page('innerWidth') == w, 4.0)
    check(f'phone ({tag}): the window is {w} px wide', got is True, b.page('innerWidth'))


def canary(b, rig, theme, run):
    """The sweep can fail: a 700 px element prepended to Help's pane at 360 px must be reported."""
    S.open_page(b, rig.writer, theme)
    if S.open_to(b, 'help', 'here', lambda label: None, 'canary/help') is not True:
        return
    b.page("(() => { const d = document.createElement('div'); d.id = 'canary-wide'; d.style.width = '700px';"
           " d.style.height = '10px'; document.querySelector('.fp .fp-page').prepend(d); return 1; })()")
    got = b.page(MEASURE)
    check(f'phone ({theme}): a 700 px element planted in Help\'s pane at 360 px is reported (the sweep can fail)',
          any(c['kind'] == 'pane-scroll' and c['over'] > 300 for c in got['cuts']), got['cuts'][:3])
    b.page("(document.getElementById('canary-wide').remove(), 1)")
    S.shut(b, 'help')


def child(theme, out):
    """One theme: the passes of PASSES in one browser; the result goes to `out` as JSON."""
    rig = S.Rig()
    run = Run({})
    shots = os.environ.get('WEB_CHECK_SHOTS')
    b = S.browser_of(theme=theme)
    try:
        b.preload(modal.HOOK + ' ' + S.WARNS)
        S.open_page(b, rig.writer, theme)
        schemes = dict(S.themes_of(b))[theme]
        for w, h, only_schemes, only_who in PASSES:
            for scheme in schemes[:only_schemes]:
                tag = f'{theme}.{scheme}'
                set_width(b, w, h, tag)
                for who, daemon, visit in S.visitors(rig, only_who):
                    S.open_page(b, daemon, theme)
                    S.set_scheme(b, scheme, schemes)

                    def probe(state, tag=tag, w=w):
                        run.last = state
                        got = analyse(b, run, tag, state, w)
                        if shots and (got['pageScroll'] > 1 or got['popupScroll'] > 1):
                            name = f'{shots}/phone-{tag}-{w}-{state.replace("/", "_").replace("%20", "-")}.png'
                            os.makedirs(shots, exist_ok=True)
                            with open(name, 'wb') as f:
                                f.write(b.shot())

                    try:
                        visit(b, probe)
                    except Exception as error:  # noqa: BLE001
                        check(f'phone ({tag}, {who}, {w}): the visit ran to its end', False,
                              f'after {run.last}: ' + S.words_of(error))
                if w == 360 and scheme == schemes[0]:
                    canary(b, rig, theme, run)
            print(f'  {theme} at {w}: {len(run.found)} failures so far')
    finally:
        b.close()
        rig.stop()
    with open(out, 'w') as f:
        json.dump({'found': run.found, 'where': run.where, 'states': run.states, 'passed': modal.passed,
                   'fails': modal.fails}, f)


def read_baseline():
    with open(f'{HERE}/phone-baseline.json') as f:
        return {k: v for k, v in json.load(f).items() if not k.startswith('licence_')}


def main():
    if '--self-test' in sys.argv:
        sys.exit(self_test())
    if '--child' in sys.argv:
        child(sys.argv[sys.argv.index('--child') + 1], sys.argv[sys.argv.index('--out') + 1])
        return
    update = '--update-baseline' in sys.argv
    run = Run(read_baseline())
    started = time.time()
    d = common.Daemon(blank=True)
    b = S.browser_of()
    try:
        S.open_page(b, d, 'ember')
        themes = S.themes_of(b)
    finally:
        b.close()
        d.stop()
    wanted = [t for t, _ in themes if t in modal.THEMES]
    for theme, got in S.run_children(os.path.abspath(__file__), wanted).items():
        for key, measure in got['found'].items():
            run.found[key] = measure
            run.where[key] = got['where'][key]
        run.states += got['states']
        modal.passed += got['passed']
        modal.fails.extend(got['fails'])
    report(run, update, time.time() - started)


def report(run, update, wall):
    fails, known = [], 0
    for key, measure in sorted(run.found.items()):
        v = verdict(key, measure, run.baseline, worse_is_more=not key.endswith('|target'))
        if v == 'known':
            known += 1
        else:
            fails.append((v, key, measure))
    if update:
        fails = []
    for v, key, measure in fails:
        modal.check(f'phone: {key}: {measure} ({v}; {run.where[key]})', False)
    check('phone: nothing is new or worse than the baseline', not fails, fails[:3])
    check('phone: the sweep measured states', run.states > 100, run.states)
    print(f'phone_sweep: {run.states} state measurements in {int(wall)} s: {known} known failures in the baseline, '
          f'{len(fails)} new or worse')
    if not any(os.environ.get(v) for v in ('WEB_CHECK_THEMES', 'WEB_CHECK_WHO')):
        for key in run.baseline:
            if key not in run.found:
                print(f'  in the baseline and not seen now: {key}')
    if update:
        out = {'licence_1': 'SPDX-FileCopyrightText: 2026 the svolder authors',
               'licence_2': 'SPDX-License-Identifier: GPL-3.0-or-later'}
        out.update(dict(sorted(run.found.items())))
        with open(f'{HERE}/phone-baseline.json', 'w') as f:
            json.dump(out, f, indent=1)
            f.write('\n')
        print(f'phone_sweep: baseline written, {len(out) - 2} entries')
    print(f'phone_sweep.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
