# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""WCAG 2.2 contrast of the page and its popups, measured in a browser on the colours the page really
draws, in every theme and every scheme the theme has (the page's own list). The browser resolves the
tokens (`var()` chains, `color-mix()`, alpha); the script reads the computed colours, composites each
element's background over the ones behind it and computes the ratios itself (a node check over the token
files would have to resolve the same chains and colour functions a second time, so none is made).

Per theme and scheme, in every state of sweep_states.py (the page, the frame's menu, each listed popup on
each entry, a device's page with its forms and lists open, a dimmed device, the add walk's pages, the
config-changed line, the EQ library's preview, a guest's and a read-only page, no device at all):
  text          every visible text on its ground: 4.5:1, 3:1 for large text (24 px, or 18.66 px bold); a
                disabled control 3:1 (it is exempt in WCAG; its reason is text and is held to 4.5:1)
  edge          a control's border or fill against the ground it stands on: 3:1 (the better of the two)
  focus         the ring of the control that holds the focus after a Tab (outline, or box-shadow): 3:1 against
                its ground
  graph         the strokes and fills of a graph (series, target, compare, gap) against the plot's ground:
                3:1; the theme's own tone graph carries the imported preset's overlay
A ground that is a picture (a canvas or an image behind translucent layers: Night drive's scene) is
sampled from a screenshot with the text hidden, at its darkest and lightest in the element's box, and the
text is held against the worse of the two. A ground that cannot be reached (offscreen, a gradient) is
listed as not measurable with the reason.
The baseline (contrast-baseline.json) holds the pairs that fail today, with their ratio: a pair that is not in
it and fails, or one in it that got worse (by more than 0.02), fails the check. `--update-baseline` rewrites
it from this run (Chromium). What the check cannot see: text drawn into a canvas (Night drive's HUD, Ember's
smoke notes: no element holds it), an image's own pixels, forced-colours mode, a colour that changes on hover
or by an animation between two samples.
With WEB_CHECK_REPORT=<file> the palette report's numbers are written there as JSON (see palettes.py).
Usage: python3 contrast.py chromium|firefox [--update-baseline]"""
import io
import re
import json
import os
import sys
import time

import common
import modal
import sweep_states as S
from modal import check

HERE = os.path.dirname(os.path.abspath(__file__))
TEXT_MIN, LARGE_MIN, NON_TEXT_MIN = 4.5, 3.0, 3.0
SLACK = 0.02


# ---- the arithmetic (self-tested: `python3 contrast.py --self-test`) ----

def lin(v):
    v /= 255.0
    return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4


def lum(rgb):
    return 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])


def ratio_of(l1, l2):
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def over(fg, bg):
    """`fg` ([r, g, b, a]) over the opaque `bg`: [r, g, b]."""
    a = fg[3]
    return [fg[i] * a + bg[i] * (1 - a) for i in range(3)]


def ratio(fg, bg):
    """The contrast of the colour `fg` (alpha allowed) over the opaque `bg`."""
    return ratio_of(lum(over(fg, bg)), lum(bg))


def ratio_over_range(fg, lo, hi):
    """The worse of the contrasts of `fg` over the darkest (`lo`) and the lightest (`hi`) of a ground's luminance."""
    worst = None
    for g in (lo, hi):
        grey = 255 * (g ** (1 / 2.2))  # a grey of that luminance, close enough for the composite
        text = lum(over(fg, [grey, grey, grey]))
        r = ratio_of(text, g)
        worst = r if worst is None else min(worst, r)
    return worst


def hexed(rgb):
    return '#%02x%02x%02x' % tuple(max(0, min(255, round(v))) for v in rgb)


def self_test():
    bad = []

    def near(what, got, want, tol=0.01):
        if abs(got - want) > tol:
            bad.append(f'{what}: got {got}, want {want}')
    near('black on white', ratio([0, 0, 0, 1], [255, 255, 255]), 21.0)
    near('#767676 on white (the 4.54 edge)', ratio([118, 118, 118, 1], [255, 255, 255]), 4.54, 0.02)
    near('#777 on white', ratio([119, 119, 119, 1], [255, 255, 255]), 4.48, 0.02)
    near('half white over black on black', ratio([255, 255, 255, .5], [0, 0, 0]), ratio([127.5] * 3 + [1], [0, 0, 0]))
    near('same colour', ratio([10, 20, 30, 1], [10, 20, 30]), 1.0)
    near('mid grey over black..white is the worse end', ratio_over_range([128, 128, 128, 1], lum([0, 0, 0]), lum([255, 255, 255])), ratio([128, 128, 128, 1], [255, 255, 255]), 0.1)
    near('white over a dark range', ratio_over_range([255, 255, 255, 1], lum([0, 0, 0]), lum([40, 40, 40])), ratio([255, 255, 255, 1], [40, 40, 40]), 0.2)
    if hexed([255, 0, 128.4]) != '#ff0080':
        bad.append('hexed')
    # the verdict against a baseline: new failures and worse ones fail, a known one passes
    base = {'k|text': {'ratio': 3.0, 'need': 4.5}}
    cases = [(verdict('k|text', 3.0, 4.5, base), 'known'), (verdict('k|text', 2.9, 4.5, base), 'worse'),
             (verdict('j|text', 3.0, 4.5, base), 'new'), (verdict('j|text', 4.6, 4.5, base), 'ok'),
             (verdict('k|text', 4.6, 4.5, base), 'ok'), (verdict('j|text', 4.3, 4.5, base, True), 'new'),
             (verdict('j|text', 4.5, 4.5, base, True), 'ok'), (verdict('k|text', 2.8, 4.5, base, True), 'known'),
             (verdict('k|text', 2.5, 4.5, base, True), 'worse')]
    # a theme's other palette is held to the default palette's pairs
    pal = {'t.dark|s|b|edge': {'ratio': 2.0, 'need': 3.0}}
    cases += [(verdict('t.dark.blue|s|b|edge', 2.0, 3.0, pal), 'known'), (verdict('t.dark.blue|s|b|edge', 1.5, 3.0, pal), 'worse'),
              (verdict('t.dark.blue|s|c|edge', 2.0, 3.0, pal), 'new'), (verdict('t.dark|s|c|edge', 2.0, 3.0, pal), 'new'),
              (baseline_key('t.dark.blue|s|b|edge'), 't.dark|s|b|edge'), (baseline_key('t.dark|s|b|edge'), 't.dark|s|b|edge')]
    for got, want in cases:
        if got != want:
            bad.append(f'verdict: got {got}, want {want}')
    for line in bad:
        print('self-test:', line)
    print('contrast self-test:', 'FAILED' if bad else 'ok')
    return 1 if bad else 0


PICTURE_ROOM = 0.35


PALETTE_TAG = re.compile(r'^([a-z0-9_-]+\.[a-z]+)\.[a-z0-9_-]+\|')


def baseline_key(key):
    """The key a pair is compared with in the baseline: a pair of a theme's other palette (`signal.dark.phosphor|...`)
    is held to the same pair of the theme's default palette (`signal.dark|...`): a palette may fail where the default
    fails today and no worse, and nowhere else."""
    return PALETTE_TAG.sub(r'\1|', key)


def verdict(key, got, need, baseline, picture=False):
    """`ok` (at the WCAG threshold, whatever the ground), `known` (below it, in the baseline, not worse), `worse` or
    `new`. Only the comparison with a baseline pair has room: a pair read off a picture is drawn a little
    differently by the two engines, and the baseline is written from one of them."""
    if got >= need:
        return 'ok'
    base = baseline_key(key)
    if base not in baseline:
        return 'new'
    key = base
    room = PICTURE_ROOM if picture else 0.0
    return 'known' if got >= baseline[key]['ratio'] - SLACK - room else 'worse'


# ---- what the page is asked ----

# Every visible text, field, control edge, the focused control's ring and every graph in the scope: the colours as
# the browser computes them, with the layers behind each. The ratios are made in Python.
MEASURE = r"""((scopeSel) => {
  const scope = scopeSel ? document.querySelector(scopeSel) : document.body;
  if (!scope) return null;
  const cv = document.createElement('canvas'); cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  const num = (s) => (String(s).endsWith('%') ? parseFloat(s) / 100 : Number(s));
  const parse = (css) => {
    let m = css.match(/^rgba?\(([^)]+)\)$/);
    if (m) { const p = m[1].split(/[ ,\/]+/).filter(Boolean); return [num(p[0]), num(p[1]), num(p[2]), p.length > 3 ? num(p[3]) : 1]; }
    m = css.match(/^color\(srgb ([^)]+)\)$/);
    if (m) { const p = m[1].split(/[ \/]+/).filter(Boolean); return [num(p[0]) * 255, num(p[1]) * 255, num(p[2]) * 255, p.length > 3 ? num(p[3]) : 1]; }
    if (css === 'transparent') return [0, 0, 0, 0];
    const at = (bg) => { cx.globalCompositeOperation = 'source-over'; cx.clearRect(0, 0, 1, 1);
      cx.fillStyle = bg; cx.fillRect(0, 0, 1, 1); cx.fillStyle = css; cx.fillRect(0, 0, 1, 1);
      return cx.getImageData(0, 0, 1, 1).data; };
    const w = at('#fff'), k = at('#000'); const a = 1 - (w[0] - k[0]) / 255;
    return a > 0.001 ? [k[0] / a, k[1] / a, k[2] / a, a] : [0, 0, 0, 0];
  };
  const opacityOf = (e) => { let o = 1; for (let n = e; n && n.nodeType === 1; n = n.parentElement) o *= Number(getComputedStyle(n).opacity); return o; };
  const root = parse(getComputedStyle(document.documentElement).backgroundColor);
  // the ground behind `e` from its own box outward: the layers up to the first opaque one
  const ground = (e, from) => {
    const layers = []; let cover = 1, image = null, stop = null;
    for (let n = from; n && n.nodeType === 1; n = n.parentElement) {
      const cs = getComputedStyle(n);
      const c = parse(cs.backgroundColor); const a = c[3] * opacityOf(n);
      if (cs.backgroundImage && cs.backgroundImage !== 'none' && !image) image = cs.backgroundImage.slice(0, 30);
      if (a > 0.001) { layers.push([c[0], c[1], c[2], a]); cover *= 1 - a; }
      if (cover < 0.004) { stop = n; break; }
    }
    let base = cover < 0.004 ? null : (root[3] > 0.5 ? root : null);
    let rgb = base ? [base[0], base[1], base[2]] : [255, 255, 255];
    for (let i = layers.length - 1; i >= 0; i--) { const l = layers[i];
      rgb = [l[0] * l[3] + rgb[0] * (1 - l[3]), l[1] * l[3] + rgb[1] * (1 - l[3]), l[2] * l[3] + rgb[2] * (1 - l[3])]; }
    return { rgb, open: cover >= 0.004 && !base, image, cover, stop };
  };
  // something else stands over the middle of `e` at this scroll offset (a fixed bar, a sheet): a shot here shows that, not the ground
  const swid = (e) => { window.__sw = (window.__sw || 0) + 1; e.dataset.swid = String(window.__sw); return window.__sw; };
  const coveredAt = (e) => {
    const r = e.getBoundingClientRect();
    if (r.top < 0 || r.bottom > innerHeight) return false;
    const t = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!t && t !== e && !e.contains(t) && !t.contains(e);
  };
  // why a ground is read off a picture: a gradient or image painted on it, nothing opaque under it, or a canvas under it
  const why = (g) => (g.image ? 'a gradient or image on it' : g.open ? 'no opaque layer under it' : 'a canvas, video or image under it');
  const visible = (e) => {
    if (e.checkVisibility && !e.checkVisibility({ visibilityProperty: true, opacityProperty: true })) return false;
    const r = e.getBoundingClientRect();
    return r.width >= 3 && r.height >= 3;
  };
  const sig = (e) => {
    const tag = e.tagName.toLowerCase();
    const id = e.id && !/\d{3,}/.test(e.id) ? '#' + e.id : '';
    const cls = id ? '' : [...e.classList].filter((c) => !/\d/.test(c)).sort().join('.');
    return tag + id + (cls ? '.' + cls : '') + (e.matches('[disabled],[aria-disabled=true]') ? '[off]' : '');
  };
  // a canvas, video or image that paints under `e`: above the opaque layer that ended the walk (a descendant of the
  // element that gave it), or anywhere under it when no layer was opaque
  const behind = (e, g) => {
    const r = e.getBoundingClientRect();
    const hits = document.elementsFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return hits.some((h) => h !== e && !e.contains(h) && !h.contains(e) && /^(CANVAS|VIDEO|IMG)$/.test(h.tagName)
      && h.getBoundingClientRect().width > 8 && (g.cover > 0.004 || (g.stop && g.stop.contains(h))));
  };
  const out = { text: [], edge: [], graph: [], skipped: [] };
  const seen = new Set();
  for (const e of scope.querySelectorAll('*')) {
    if (!visible(e)) continue;
    const cs = getComputedStyle(e);
    const svg = e instanceof SVGElement;
    let own = [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim() !== '');
    let kind = 'text';
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName) && e.type !== 'checkbox' && e.type !== 'radio' && e.type !== 'hidden') {
      own = (e.value || e.placeholder || e.tagName === 'SELECT'); kind = 'field';
    }
    if (own && svg && e.ownerSVGElement && [...e.ownerSVGElement.children].some((c) => c.tagName === 'rect' && c.getAttribute('fill'))) {
      out.skipped.push({ sig: sig(e), why: 'text in an svg picture that paints its own ground (a theme\'s card in the picker)' });
      continue;
    }
    if (own && svg && /^(url\(|none|context-)/.test(cs.fill)) {
      out.skipped.push({ sig: sig(e), why: 'an svg text painted with ' + cs.fill.slice(0, 14) + ' (a paint server or none), not a colour' });
      continue;
    }
    if (own && !(svg && !(e instanceof SVGTextElement || e instanceof SVGTSpanElement))) {
      const r = e.getBoundingClientRect();
      const fg = parse(svg ? cs.fill : (e.value === '' && e.placeholder ? getComputedStyle(e, '::placeholder').color : cs.color));
      const g = ground(e, e);
      const size = parseFloat(cs.fontSize), bold = Number(cs.fontWeight) >= 700;
      out.text.push({ sig: sig(e), kind, fg: [fg[0], fg[1], fg[2], fg[3] * opacityOf(e)], bg: g.rgb, image: g.image,
        picture: g.open || g.image !== null || behind(e, g), large: size >= 24 || (size >= 18.66 && bold),
        off: !!e.closest('[disabled],[aria-disabled=true]'), what: (e.value || e.textContent || '').trim().slice(0, 24),
        rect: [r.left, r.top, r.width, r.height], covered: coveredAt(e), id: swid(e), why: why(g) });
    }
    // a control's edge: its border, or its fill, against the ground outside it
    if (/^(BUTTON|INPUT|TEXTAREA|SELECT)$/.test(e.tagName) || e.getAttribute('role') === 'button') {
      if (e.type === 'hidden' || e.type === 'checkbox' || e.type === 'radio') continue;
      // an inactive control is exempt from the edge rule (WCAG 1.4.11); its words are still read, at 3:1
      if (e.matches('[disabled],[aria-disabled=true]')) continue;
      const bw = parseFloat(cs.borderTopWidth); const bs = cs.borderTopStyle;
      const fill = parse(cs.backgroundColor); const outside = ground(e, e.parentElement);
      const border = bw > 0 && bs !== 'none' ? parse(cs.borderTopColor) : null;
      if (border || /^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName)) {
        out.edge.push({ covered: coveredAt(e), id: swid(e), why: why(outside), rect: [e.getBoundingClientRect().left, e.getBoundingClientRect().top, e.getBoundingClientRect().width, e.getBoundingClientRect().height], sig: sig(e), border, fill: fill[3] > 0.02 ? fill : null, bg: outside.rgb, picture: outside.open || outside.image !== null || behind(e, outside),
          what: (e.value || e.textContent || e.getAttribute('aria-label') || '').trim().slice(0, 20) });
      }
    }
  }
  // graphs: the strokes and fills of the popup graphs and of the theme's tone graph, over the plot's ground
  const PARTS = [['.fp-graph', { series: ['.fp-graph-series', 'stroke'], target: ['.fp-graph-target', 'stroke'],
      compare: ['.fp-graph-compare', 'stroke'], gap: ['.fp-graph-gap', 'fill'] }],
    ['#part-tone .eqg', { target: ['.tgt', 'stroke'], gap: ['.gap', 'fill'] }],
    ['#sg-graph', { target: ['.sg-target', 'stroke'], gap: ['.sg-gap', 'fill'] }],
    ['#p-graph', { target: ['.p-target', 'stroke'], gap: ['.p-gap', 'fill'] }]];
  for (const [box, parts] of PARTS) {
    const b = scope.querySelector(box) || (scopeSel ? null : document.querySelector(box));
    if (!b || !visible(b)) continue;
    const g = ground(b, b);
    for (const [name, [q, prop]] of Object.entries(parts)) {
      const p = b.querySelector(q);
      if (!p) continue;
      const c = parse(getComputedStyle(p)[prop]);
      out.graph.push({ sig: box + ' ' + name, fg: [c[0], c[1], c[2], c[3] * opacityOf(p)], bg: g.rgb, picture: g.open || g.image !== null || behind(b, g), rect: [b.getBoundingClientRect().left, b.getBoundingClientRect().top, b.getBoundingClientRect().width, b.getBoundingClientRect().height], id: swid(b), why: why(g) });
    }
  }
  // the control that holds the focus, when the browser shows it a ring
  const a = document.activeElement;
  out.focus = null;
  if (a && a !== document.body && a.matches(':focus-visible') && visible(a)) {
    const cs = getComputedStyle(a);
    const outside = ground(a, a.parentElement);
    out.focus = { id: swid(a), why: why(outside), rect: [a.getBoundingClientRect().left, a.getBoundingClientRect().top, a.getBoundingClientRect().width, a.getBoundingClientRect().height], sig: sig(a), outline: [cs.outlineStyle, parseFloat(cs.outlineWidth), parse(cs.outlineColor)],
      shadow: cs.boxShadow, bg: outside.rgb, picture: outside.open || outside.image !== null || behind(a, outside), inside: parse(cs.backgroundColor) };
  }
  return out;
})"""

# make every text transparent, so a screenshot is the ground alone
# (a constructed sheet: the page's CSP allows no inline style element)
HIDE = """(() => { const sheet = new CSSStyleSheet();
  sheet.replaceSync('*, *::before, *::after { color: transparent !important; text-shadow: none !important; text-decoration: none !important; caret-color: transparent !important; -webkit-text-fill-color: transparent !important; outline-color: transparent !important; box-shadow: none !important; border-color: transparent !important; } svg text, svg tspan { fill: transparent !important; stroke: none !important; }');
  window.__hide = sheet; document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]; return 1; })()"""
UNHIDE = "(document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== window.__hide), 1)"


def parse_shadow(shadow):
    """The first outer shadow's colour in a computed `box-shadow` (`rgb(..) 0px 0px 0px 2px`), as [r, g, b, a] and its
    spread; None if there is none."""
    import re
    for part in re.split(r',\s*(?![^(]*\))', shadow or ''):
        if 'inset' in part or part.strip() in ('', 'none'):
            continue
        m = re.match(r'\s*(rgba?\([^)]*\)|color\([^)]*\))\s*(.*)', part)
        if not m:
            continue
        nums = [float(x) for x in re.findall(r'-?[\d.]+(?=px)', m.group(2))]
        if len(nums) >= 4 and nums[3] > 0 or (len(nums) >= 3 and nums[2] == 0 and nums[0] == nums[1] == 0):
            nums = (nums + [0, 0, 0, 0])[:4]
            return m.group(1), nums[3]
    return None


class Run:
    """The measurements of one browser run: the pairs it found, the verdict on each against the baseline, what it could
    not measure."""

    def __init__(self, baseline):
        self.baseline = baseline
        self.pairs = {}       # key -> {ratio, need}
        self.unmeasured = {}  # key -> reason
        self.seen_states = set()
        self.last = None
        self.palette = {}
        self.rings = {}
        self.ground = {}
        self.gaps = {}        # key -> (computed ratio, sampled ratio, why it is read off a picture, state)

    def add(self, key, got, need, note=''):
        if key not in self.pairs or got < self.pairs[key]['ratio']:
            self.pairs[key] = {'ratio': round(got, 3), 'need': need, 'note': note}

    def gap(self, key, computed, sampled, why, state):
        """Keep the widest gap between what the colours compute to and what the screenshot gave."""
        if computed - sampled > 0 and (key not in self.gaps or computed - sampled > self.gaps[key][0] - self.gaps[key][1]):
            self.gaps[key] = (round(computed, 3), round(sampled, 3), why, state)

    def skip(self, key, why):
        self.unmeasured.setdefault(key, why)


def to_rgba(css):
    """A css colour string from a computed style as [r, g, b, a] (rgb(), rgba() and color(srgb ..) only)."""
    import re
    m = re.match(r'rgba?\(([^)]+)\)', css) or re.match(r'color\(srgb ([^)]+)\)', css)
    if not m:
        return None
    p = [x for x in re.split(r'[ ,/]+', m.group(1)) if x]

    def num(s):
        return float(s[:-1]) / 100 if s.endswith('%') else float(s)
    scale = 255 if css.startswith('color(') else 1
    return [num(p[0]) * scale, num(p[1]) * scale, num(p[2]) * scale, num(p[3]) if len(p) > 3 else 1.0]


# for each `[id, [x, y, w, h]]`: the boxes of the fixed or sticky elements that are painted over that box at the
# present scroll offset and are not ancestors of the element tagged `id`: what a screenshot shows there is not the
# element's ground. A fixed canvas that is painted under the element (a scene) is not one.
OVERLAYS = """((items) => {
  const fixed = [...document.querySelectorAll('*')].filter((f) => {
    const p = getComputedStyle(f).position; if (p !== 'fixed' && p !== 'sticky') return false;
    const r = f.getBoundingClientRect(); return r.width >= 2 && r.height >= 2; });
  return items.map(([id, [bx, by, bw, bh]]) => {
    const e = document.querySelector('[data-swid="' + id + '"]'); const out = [];
    if (!e) return out;
    for (const f of fixed) {
      if (f === e || f.contains(e)) continue;
      const r = f.getBoundingClientRect();
      const x0 = Math.max(r.left, bx), y0 = Math.max(r.top, by), x1 = Math.min(r.right, bx + bw), y1 = Math.min(r.bottom, by + bh);
      if (x1 <= x0 || y1 <= y0) continue;
      const hits = document.elementsFromPoint((x0 + x1) / 2, (y0 + y1) / 2);
      const iF = hits.findIndex((h) => h === f || f.contains(h));
      let iE = hits.findIndex((h) => h === e || e.contains(h));
      if (iE < 0) iE = hits.findIndex((h) => h.contains(e));
      if (iF >= 0 && (iE < 0 || iF < iE)) out.push([r.left, r.top, r.width, r.height]);
    }
    return out; }); })"""


def sample_ground(b, rects):
    """The luminance range of the screenshot's pixels in each box `(x, y, w, h[, ring[, covered[, id]]])` (css px in the
    window now; the browser is at dpr 1), with the text hidden. Each box is read in one shot only, where it stands on
    screen: the shot at the window's present scroll offset when the box is in the window (twice, a moment apart, for a
    ground that animates), else one at an offset that brings it in. Shots are never stitched. In each shot the pixels
    of any fixed or sticky element painted over the box (and not its ancestor) are left out (OVERLAYS), at the offset
    the shot was taken at. With a `ring` the pixels sampled are the ring of that many px around the box, its own box
    left out (a negative ring: the box less that many px on each side, its border out). `[(lo, hi)]`, None for a box
    no shot shows (inside a pane that scrolls on its own, say) or that is wholly covered."""
    import numpy as np
    from PIL import Image
    b.page(HIDE)
    time.sleep(0.15)
    top, height, total = b.page("[scrollY, innerHeight, document.documentElement.scrollHeight]")
    reach = max(total - height, 0)
    plan = []
    for box in rects:
        x, y, rw, rh = box[:4]
        ring = box[4] if len(box) > 4 else 0
        covered = len(box) > 5 and box[5]
        if not covered and y - max(ring, 0) >= 0 and y + rh + max(ring, 0) <= height:
            plan.append((top, y))
            continue
        centred = int(min(max(top + y + rh / 2 - height / 2, 0), reach))
        want = centred // 100 * 100
        if not (0 <= top + y - want and top + y + rh <= want + height):
            want = centred
        plan.append((want, top + y - want))

    def lum_of():
        img = np.asarray(Image.open(io.BytesIO(b.shot())).convert('RGB'), dtype=float) / 255.0
        lin_px = np.where(img <= 0.04045, img / 12.92, ((img + 0.055) / 1.055) ** 2.4)
        return lin_px @ np.array([0.2126, 0.7152, 0.0722])

    shots, masks = {}, {}
    for pos in sorted({p for p, _ in plan}):
        if pos != b.page('scrollY'):
            b.page(f'(window.scrollTo(0, {pos}), 1)')
            time.sleep(0.12)
        shots[pos] = [lum_of()]
        if pos == top:
            time.sleep(0.4)
            shots[pos].append(lum_of())
        items = []
        for i, (box, (p, ry)) in enumerate(zip(rects, plan)):
            if p != pos:
                continue
            ring = max(box[4] if len(box) > 4 else 0, 0)
            ident = box[6] if len(box) > 6 else None
            if ident is not None:
                items.append((i, [ident, [box[0] - ring, ry - ring, box[2] + 2 * ring, box[3] + 2 * ring]]))
        if items:
            got = b.page(f'({OVERLAYS})({json.dumps([it for _, it in items])})')
            for (i, _), rs in zip(items, got):
                masks[i] = rs
    b.page(f'(window.scrollTo(0, {top}), 1)')
    b.page(UNHIDE)
    out = []
    for i, (box, (pos, ry)) in enumerate(zip(rects, plan)):
        x, _, rw, rh = box[:4]
        ring = box[4] if len(box) > 4 else 0
        lo, hi = 2.0, -1.0
        for sheet in shots[pos]:
            rows, cols = sheet.shape
            x0, y0, x1, y1 = (max(0, int(x) - ring), max(0, int(ry) - ring), min(cols, int(x + rw) + 1 + ring),
                              min(rows, int(ry + rh) + 1 + ring))
            if x1 - x0 < 2 or y1 - y0 < 2:
                continue
            part = sheet[y0:y1, x0:x1]
            keep = np.ones(part.shape, dtype=bool)
            if ring > 0:
                # everything but the control's own box
                keep[max(0, int(ry) - y0):int(ry + rh) + 1 - y0, max(0, int(x) - x0):int(x + rw) + 1 - x0] = False
            for mx, my, mw, mh in masks.get(i, []):
                keep[max(0, int(my) - y0):max(0, int(my + mh) + 1 - y0), max(0, int(mx) - x0):max(0, int(mx + mw) + 1 - x0)] = False
            if keep.any():
                # the 2nd and 98th percentile: a thin border, a rule or an underline in the box is not the ground
                vals = part[keep]
                lo, hi = min(lo, float(np.percentile(vals, 2))), max(hi, float(np.percentile(vals, 98)))
        out.append((lo, hi) if hi >= 0 else None)
    return out


def analyse(b, run, tag, state, scope=None, only_focus=False):
    """Measure the page in its present state and record each pair under `tag|state|signature|kind`."""
    group = state.split('/')[0]
    if scope is None:
        # with a popup open the page behind it is under the veil and inert: the popup is what is read; the same
        # for the open menu
        scope = b.page("document.querySelector('.fp') ? '.fp' : (document.getElementById('fx-menu')?.hidden === false"
                       " ? '#fx-menu' : null)")
    got = b.page(f'({MEASURE})({json.dumps(scope)})')
    if not got:
        return set()
    if only_focus:
        got = {'text': [], 'edge': [], 'graph': [], 'skipped': [], 'focus': got.get('focus')}
    for k in got.get('skipped', []):
        run.skip(f'{tag}|{group}|{k["sig"]}|text', k['why'])
    pictures = []
    for t in got['text']:
        need = LARGE_MIN if t['large'] else TEXT_MIN
        if t['off']:
            need = NON_TEXT_MIN
        key = f'{tag}|{group}|{t["sig"]}|{"text-large" if t["large"] else t["kind"]}{"-disabled" if t["off"] else ""}'
        if t['picture']:
            pictures.append((key, t, need))
            continue
        if t['image']:
            run.skip(key, f'a gradient or image under it ({t["image"]})')
            continue
        run.add(key, ratio(t['fg'], t['bg']), need, f'{t["what"]!r} at {state}')
    if pictures:
        ranges = sample_ground(b, [tuple(t['rect']) + (-3 if min(t['rect'][2:]) > 12 else 0, t['covered'], t['id']) for _, t, _ in pictures])
        for (key, t, need), rng in zip(pictures, ranges):
            if rng is None:
                run.skip(key, 'a picture ground, the text is outside the window')
            else:
                got_ratio = ratio_over_range(t['fg'], rng[0], rng[1])
                run.add(key, got_ratio, need, f"{t['what']!r} at {state} (picture, worse of lightest and darkest)")
                if got_ratio < need:
                    run.gap(key, ratio(t['fg'], t['bg']), got_ratio, t['why'], state)
    ring_jobs = []
    for e in got['edge']:
        key = f'{tag}|{group}|{e["sig"]}|edge'
        if e['picture']:
            ring_jobs.append((key, e, 'edge'))
            continue
        best = max([ratio(c, e['bg']) for c in (e['border'], e['fill']) if c] or [1.0])
        run.add(key, best, NON_TEXT_MIN, f"{e['what']!r} at {state}")
    found = set()
    for g in got['graph']:
        key = f'{tag}|{group}|{g["sig"]}|graph'
        found.add(g['sig'])
        if g['picture']:
            ring_jobs.append((key, g, 'graph'))
            continue
        run.add(key, ratio(g['fg'], g['bg']), NON_TEXT_MIN, f'at {state}')
    f = got.get('focus')
    if f:
        key = f'{tag}|{group}|{f["sig"]}|focus'
        ring = None
        style, width, colour = f['outline']
        if style != 'none' and width > 0 and colour[3] > 0:
            ring = colour
        else:
            shadow = parse_shadow(f['shadow'])
            if shadow:
                ring = to_rgba(shadow[0])
        if f['picture'] and ring is not None:
            ring_jobs.append((key, dict(f, ring=ring), 'focus'))
        elif f['picture']:
            run.add(key, 1.0, NON_TEXT_MIN, f'no outline and no box-shadow at {state}')
        elif ring is None:
            run.add(key, 1.0, NON_TEXT_MIN, f'no outline and no box-shadow at {state}')
        else:
            run.add(key, ratio(ring, f['bg']), NON_TEXT_MIN, f'ring at {state}')
            run.rings.setdefault(tag, (over(ring, f['bg']), ratio(ring, f['bg'])))
    if ring_jobs:
        # a control on a picture: the picture's lightest and darkest just outside the control
        ranges = sample_ground(b, [tuple(j['rect']) + (6, j.get('covered', False), j['id']) for _, j, _ in ring_jobs])
        for (key, j, kind), rng in zip(ring_jobs, ranges):
            if rng is None:
                run.skip(key, 'the control stands on a picture and no picture shows around it')
                continue
            colours = ([j['ring']] if kind == 'focus' else [j['fg']] if kind == 'graph'
                       else [c for c in (j['border'], j['fill']) if c])
            best = max([ratio_over_range(c, rng[0], rng[1]) for c in colours] or [1.0])
            run.add(key, best, NON_TEXT_MIN, f'{kind} at {state} (picture around it, worse of lightest and darkest)')
            flat = ([ratio(j['ring'], j['bg'])] if kind == 'focus' else [ratio(j['fg'], j['bg'])] if kind == 'graph'
                    else [ratio(c, j['bg']) for c in (j['border'], j['fill']) if c])
            if best < NON_TEXT_MIN:
                run.gap(key, max(flat or [1.0]), best, j['why'], state)
    return found


# ---- the palette report (WEB_CHECK_PALETTE=<notes folder>) ----

TOKENS = {'surface': '--frame-surface', 'text': '--frame-ink', 'muted text': '--frame-dim', 'faint text': '--frame-faint',
          'accent': '--frame-accent', 'text on accent': '--frame-on-accent', 'control edge': '--frame-line-strong',
          'graph series': '--frame-graph-series', 'graph target': '--frame-graph-target', 'graph gap': '--frame-graph-gap',
          'graph compare': '--frame-graph-compare'}
PALETTE_JS = """((tokens) => { const out = {};
  const p = document.createElement('i'); p.style.cssText = 'position:fixed;left:-99px';
  document.body.append(p);
  for (const [name, token] of Object.entries(tokens)) { p.style.background = 'var(' + token + ')'; out[name] = getComputedStyle(p).backgroundColor; }
  p.remove();
  const box = document.querySelector('.fp .fp-box');
  out['popup'] = box ? getComputedStyle(box).backgroundColor : null;
  return out; })"""


def to_oklab(rgb):
    def f(v):
        v /= 255.0
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = f(rgb[0]), f(rgb[1]), f(rgb[2])
    l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    return (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)


def oklab_distance(a, b):
    p, q = to_oklab(a), to_oklab(b)
    return sum((x - y) ** 2 for x, y in zip(p, q)) ** 0.5


def ground_of(b, tag, run):
    """The page's ground: the screenshot's top left pixel with the text hidden (the page, nothing open)."""
    from PIL import Image
    b.page(HIDE)
    time.sleep(0.15)
    shot = Image.open(io.BytesIO(b.shot())).convert('RGB')
    b.page(UNHIDE)
    run.ground[tag] = list(shot.getpixel((3, 3)))


def palette_of(b, tag, run):
    """The colours of this theme and scheme as the page draws them (a popup open): the tokens and the popup's box; the
    ground is the one `ground_of` took at the page."""
    got = b.page(f'({PALETTE_JS})({json.dumps(TOKENS)})')
    colours = {'ground (page corner, nothing open)': run.ground.get(tag, [0, 0, 0])}
    surface = to_rgba(got['surface'])
    surface_rgb = over(surface, [255, 255, 255]) if surface else [255, 255, 255]
    for name, css in got.items():
        c = to_rgba(css) if css else None
        if c:
            label = name if c[3] >= 0.99 else name + ' (alpha ' + format(c[3], '.2f') + ', over the surface)'
            colours[label] = over(c, surface_rgb) if name != 'surface' else surface_rgb
    run.palette[tag] = {'colours': colours, 'surface': surface_rgb}


def write_palette(run, folder):
    from PIL import Image, ImageDraw
    os.makedirs(f'{folder}/p5-shots', exist_ok=True)
    lines = ['# 5.5 palettes: every theme and scheme, as the page draws them', '',
             'Computed by `scripts/web-checks/contrast.py` (WEB_CHECK_PALETTE) in Chromium: the frame tokens the theme sets, read through a probe',
             'with the Help popup open; the ground is the screenshot pixel at the window\'s top left with the page\'s text hidden and nothing open (Night drive\'s',
             'scene and Signal\'s gradient are pictures: a single sample). A token with alpha is shown composited over the surface. Ratios are WCAG',
             '2.2 contrast against the surface unless a column says otherwise. The focus colour is the ring of the control that holds the focus',
             'after a Tab (outline, or box-shadow) where one was seen. The distances are OKLab (Euclidean; 0.02 is about a just-noticeable',
             'difference, 0.1 is plainly another colour).', '']
    names = list(run.palette)
    for tag in names:
        data = run.palette[tag]
        cols = data['colours']
        surface = data['surface']
        lines += [f'## {tag}', '', '| part | hex | contrast |', '|---|---|---|']
        for name, rgb in cols.items():
            against, ref = ('the accent', cols.get('accent')) if name == 'text on accent' else ('the surface', surface)
            r = ratio_of(lum(rgb), lum(ref))
            lines.append(f'| {name} | {hexed(rgb)} | {r:.2f}:1 against {against} |')
        ring = run.rings.get(tag)
        if ring:
            lines.append(f'| focus ring (seen) | {hexed(ring[0])} | {ring[1]:.2f}:1 (against its ground) |')
        lines.append('')
        strip = Image.new('RGB', (len(cols) * 90, 110), (128, 128, 128))
        d = ImageDraw.Draw(strip)
        for i, (name, rgb) in enumerate(cols.items()):
            d.rectangle([i * 90, 0, i * 90 + 89, 89], fill=tuple(int(round(v)) for v in rgb))
            d.text((i * 90 + 3, 93), name.split(' (')[0][:14], fill=(255, 255, 255))
        theme, scheme = tag.split('.')
        strip.save(f'{folder}/p5-shots/{theme}-{scheme}.png')
    lines += ['## Distances between themes (OKLab)', '',
              'Each pair of themes in their dark scheme (every theme has one), then Paper light against each: the ground, the accent and the text.', '',
              '| pair | ground | accent | text |', '|---|---|---|---|']
    dark = [t for t in names if t.endswith('.dark')]
    order = sorted(dark, key=lambda t: (0 if t.startswith(('ember', 'signal')) else 1, t))
    pairs = [(a, c) for i, a in enumerate(order) for c in order[i + 1:]]
    pairs.sort(key=lambda p: (0 if {p[0].split('.')[0], p[1].split('.')[0]} == {'ember', 'signal'} else 1))
    pairs += [(a, c) for a in names if a.startswith('paper.light') for c in order]
    for a, c in pairs:
        ca, cc = run.palette[a]['colours'], run.palette[c]['colours']
        g = oklab_distance(ca['ground (page corner, nothing open)'], cc['ground (page corner, nothing open)'])
        ac = oklab_distance(ca['accent'], cc['accent'])
        tx = oklab_distance(ca['text'], cc['text'])
        lines.append(f'| {a} and {c} | {g:.3f} | {ac:.3f} | {tx:.3f} |')
    lines.append('')
    with open(f'{folder}/5.5-palettes.md', 'w') as f:
        f.write('\n'.join(lines) + '\n')


# pairs read off a picture that FAIL on the sample and whose sample is more than 1.0 below what the colours compute to
# (the artefact class: a sample that manufactures a failure), each with what was found there (a name and a reason: the
# guard fails on any other)
ALLOWED_GAPS = [   # [(regex on the key, what was found and why the sample is right)]
    (r'^nightdrive\.dark\|(page|guest|off|config)\|text\|text-large$',
     'the rotated pink dB label stands over the scene\'s pink neon line (same colour): 1.0:1 where they cross'),
    (r'\|button#(p|sg)-pt-\d\|edge$',
     'a 12 to 14 px point marker on a tone graph: the plot\'s own curve and grid run through its 6 px ring'),
    (r'^paper\.dark\|(page|config)\|span\.pn\.red\|text$',
     'the red pencil ellipse drawn around the note is a background in the text\'s own colour and runs through its box'),
    (r'^signal\.dark\|(guest|off)\|button#sg-mode-flat\[off\]\|text-disabled$',
     'the selected mode button is filled with the accent (cyan in the default Blueprint; in the other palettes orange, green or brass) under the page\'s gradient: its dark words at .55 opacity are legible in a guest still of Blueprint and sit at the computed 2.3:1, an inactive control read off a sample that includes the glow around the chip (a palette\'s pair uses the default palette\'s key here)'),
]
GAP = 1.0

# the box each theme draws its tone graph in: its target and gap must be among the pairs measured on the page
THEME_GRAPH = {'ember': '#part-tone .eqg', 'signal': '#sg-graph', 'paper': '#p-graph'}
POPUP_GRAPH = ['.fp-graph series', '.fp-graph target', '.fp-graph gap']


def read_baseline():
    with open(f'{HERE}/contrast-baseline.json') as f:
        return {k: v for k, v in json.load(f).items() if not k.startswith('licence_')}


def canary(b, rig, theme, tag):
    """The sweep can fail: Help's title painted in the popup's own colour must read as no contrast at all."""
    scratch = Run({})
    S.open_page(b, rig.writer, theme)
    if S.open_to(b, 'help', 'here', lambda label: None, 'canary/help') is not True:
        return
    b.page("(() => { const t = document.querySelector('.fp .fp-title');"
           " t.style.color = getComputedStyle(document.querySelector('.fp .fp-box')).backgroundColor; return 1; })()")
    analyse(b, scratch, tag, 'canary/help')
    worst = min([p['ratio'] for k, p in scratch.pairs.items() if 'title' in k.split('|')[2]] or [99.0])
    check(f'contrast ({tag}): a planted title in the popup\'s own colour is read as unreadable (the sweep can fail)',
          worst < 1.5, worst)
    S.shut(b, 'help')


def canary_sampler(b, rig, theme):
    """The sampler reads a box's own surroundings, never a fixed bar's: a box is planted below the fold of a tall page and
    sampled; then a fixed orange bar is planted at the top (over the rows where a stitched sheet would paste it onto
    the box) and another at the bottom (over the ring of a box at the page's end, at the offset the shot is taken
    at). The range must not move (the page behind may be anything: the check is the difference)."""
    S.open_page(b, rig.writer, theme)
    for case, bar, where in (('a bar at the top', 'top:0;height:80px', 'innerHeight + 20'),
                             ('a bar over the ring', 'bottom:0;height:37px', 'document.documentElement.scrollHeight - 60')):
        b.page("(() => { const s = document.createElement('div'); s.id = 'canary-s'; s.style.cssText = 'position:absolute;left:0;top:0;width:10px;height:3000px;pointer-events:none'; document.body.append(s);"
               " const p = document.createElement('div'); p.id = 'canary-box'; p.dataset.swid = 'canary';"
               " p.style.cssText = 'position:absolute;left:100px;width:100px;height:20px;background:#101010;top:' + (" + where + ") + 'px'; document.body.append(p); return 1; })()")
        time.sleep(0.3)
        r = b.page("(() => { const r = document.getElementById('canary-box').getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()")
        before = sample_ground(b, [tuple(r) + (6, False, 'canary')])[0]
        b.page("(() => { const bar = document.createElement('div'); bar.id = 'canary-bar';"
               " bar.style.cssText = 'position:fixed;left:0;width:100%;z-index:2147483000;background:#f0a73a;" + bar + "'; document.body.append(bar); return 1; })()")
        time.sleep(0.3)
        got = sample_ground(b, [tuple(r) + (6, False, 'canary')])[0]
        b.page("(['canary-s', 'canary-bar', 'canary-box'].forEach((i) => document.getElementById(i).remove()), 1)")
        moved = None if before is None or got is None else max(abs(got[0] - before[0]), abs(got[1] - before[1]))
        check(f'contrast ({theme}): a fixed bar does not move what the sampler reads for a box ({case})',
              moved is not None and moved < 0.05, [before, got])


def child(theme, out):
    """One theme: every scheme, every visitor, in one browser; the result goes to `out` as JSON."""
    rig = S.Rig()
    run = Run({})
    palette_dir = os.environ.get('WEB_CHECK_PALETTE')
    b = S.browser_of()
    try:
        b.preload(modal.HOOK + ' ' + S.WARNS)
        S.open_page(b, rig.writer, theme)
        schemes = dict(S.themes_of(b))[theme]
        # a theme with palettes (Signal's) is swept once in each: its own list, read from the page
        palettes = S.palettes_of(b, theme)
        for n, (scheme, palette) in enumerate((s, p) for s in schemes for p in palettes):
            # the first palette is the theme's default, whose pairs keep the theme's plain tag
            tag = f'{theme}.{scheme}' if palette == palettes[0] else f'{theme}.{scheme}.{palette}'
            for who, daemon, visit in S.visitors(rig):
                S.open_page(b, daemon, theme)
                S.set_scheme(b, scheme, schemes)
                S.set_palette(b, theme, palette)

                def probe(state, who=who, tag=tag, palette=palette):
                    run.last = state
                    if palettes[0] is not None and palette != palettes[0]:
                        check(f'contrast ({tag}): the page still wears its palette at {state}',
                              S.palette_held(b, palette), state)
                    found = analyse(b, run, tag, state)
                    if who == 'writer' and state == 'page' and theme in THEME_GRAPH:
                        box = THEME_GRAPH[theme]
                        want = {f'{box} target', f'{box} gap'}
                        check(f'contrast ({tag}): the tone graph\'s target and gap are among the pairs',
                              want <= found, sorted(want - found))
                    if state == 'eq-library/autoeq/preview':
                        check(f'contrast ({tag}): the EQ library\'s graph has its series, target and gap',
                              set(POPUP_GRAPH) <= found, sorted(set(POPUP_GRAPH) - found))
                    if palette_dir and state == 'page' and palette == palettes[0]:
                        ground_of(b, tag, run)
                    if palette_dir and state == 'help/here' and palette == palettes[0] and tag not in run.palette:
                        palette_of(b, tag, run)
                    # a Tab, so the control that holds the focus shows its ring
                    b.press('Tab')
                    time.sleep(0.15)
                    analyse(b, run, tag, state, only_focus=True)

                try:
                    visit(b, probe)
                except Exception as error:  # noqa: BLE001
                    check(f'contrast ({tag}, {who}): the visit ran to its end', False,
                          f'after {run.last}: ' + S.words_of(error))
            if n == 0:
                canary(b, rig, theme, tag)
                canary_sampler(b, rig, theme)
            print(f'  {tag}: {len(run.pairs)} pairs')
    finally:
        b.close()
        rig.stop()
    with open(out, 'w') as f:
        json.dump({'pairs': run.pairs, 'unmeasured': run.unmeasured, 'palette': run.palette, 'rings': run.rings,
                   'ground': run.ground, 'gaps': run.gaps, 'passed': modal.passed, 'fails': modal.fails}, f)


def main():
    if '--self-test' in sys.argv:
        sys.exit(self_test())
    if '--child' in sys.argv:
        child(sys.argv[sys.argv.index('--child') + 1], sys.argv[sys.argv.index('--out') + 1])
        return
    update = '--update-baseline' in sys.argv
    run = Run(read_baseline())
    palette_dir = os.environ.get('WEB_CHECK_PALETTE')
    started = time.time()
    d = common.Daemon(blank=True)
    b = S.browser_of()
    try:
        b.preload(modal.HOOK + ' ' + S.WARNS)
        S.open_page(b, d, 'ember')
        themes = S.themes_of(b)
    finally:
        b.close()
        d.stop()
    wanted = [t for t, _ in themes if t in modal.THEMES]
    for theme, got in S.run_children(os.path.abspath(__file__), wanted).items():
        run.pairs.update(got['pairs'])
        run.unmeasured.update(got['unmeasured'])
        run.palette.update(got['palette'])
        run.rings.update({k: tuple(v) for k, v in got['rings'].items()})
        run.ground.update(got['ground'])
        run.gaps.update({k: tuple(v) for k, v in got['gaps'].items()})
        modal.passed += got['passed']
        modal.fails.extend(got['fails'])
    if palette_dir:
        write_palette(run, palette_dir)
    report(run, update, time.time() - started)


def report(run, update, wall):
    fails, known, ok, improved = [], [], 0, []
    run.unmeasured = {k: v for k, v in run.unmeasured.items() if k not in run.pairs}
    for key, p in sorted(run.pairs.items()):
        v = verdict(key, p['ratio'], p['need'], run.baseline, '(picture' in p['note'])
        if v == 'ok':
            ok += 1
            if key in run.baseline:
                improved.append(key)
        elif v == 'known':
            known.append(key)
        else:
            fails.append((v, key, p))
    if update:
        fails = []
    for v, key, p in fails:
        modal.check(f'contrast: {key}: {p["ratio"]}:1, needs {p["need"]}:1 ({v}; {p["note"]})', False)
    check('contrast: no pair is new or worse than the baseline', not fails, fails[:3])
    check('contrast: the sweep measured pairs in every state it visits', len(run.pairs) > 100, len(run.pairs))
    check('contrast: no more than eight pairs are not measurable (six today) (a ground that turned unreadable adds some)',
          len({baseline_key(k) for k in run.unmeasured}) <= 8, sorted(run.unmeasured)[:5])
    print(f'contrast: {len(run.pairs)} pairs in {int(wall)} s: {ok} pass, {len(known)} known failures in the baseline, '
          f'{len(fails)} new or worse; {len(run.unmeasured)} not measurable')
    for key, why in sorted(run.unmeasured.items()):
        print(f'  not measurable: {key}: {why}')
    for key in improved:
        print(f'  now passes, can leave the baseline: {key}')
    wide = {k: v for k, v in run.gaps.items() if v[0] - v[1] > GAP}
    for key, (computed, sampled, why, state) in sorted(wide.items()):
        named = next((why_ for pattern, why_ in ALLOWED_GAPS if re.search(pattern, baseline_key(key))), None)
        print(f'  sampled far below computed: {key}: computed {computed}, sampled {sampled} ({why}; at {state}; '
              f'{run.pairs.get(key, {}).get("note", "")})' + (f'; allowed: {named}' if named else ''))
    unexplained = sorted(k for k in wide if not any(re.search(pattern, baseline_key(k)) for pattern, _ in ALLOWED_GAPS))
    check('contrast: no pair reads more than 1.0 lower off its picture than the colours compute to, without a named reason',
          not unexplained, unexplained[:5])
    narrowed = any(os.environ.get(v) for v in ('WEB_CHECK_THEMES', 'WEB_CHECK_WHO'))
    if not narrowed:
        for key in run.baseline:
            if key not in run.pairs:
                print(f'  in the baseline and not seen now: {key}')
    if update and modal.fails:
        print('contrast: the baseline was NOT written: a check failed (see FAIL lines above); fix that first')
    elif update:
        out = {'licence_1': 'SPDX-FileCopyrightText: 2026 the svolder authors',
               'licence_2': 'SPDX-License-Identifier: GPL-3.0-or-later'}
        out.update({k: {'ratio': p['ratio'], 'need': p['need']} for k, p in sorted(run.pairs.items())
                    if p['ratio'] < p['need'] and baseline_key(k) == k})
        with open(f'{HERE}/contrast-baseline.json', 'w') as f:
            json.dump(out, f, indent=1)
            f.write('\n')
        print(f'contrast: baseline written, {len(out) - 2} pairs')
    print(f'contrast.py: {modal.passed} passed, {len(modal.fails)} failed')
    sys.exit(1 if modal.fails else 0)


if __name__ == '__main__':
    main()
