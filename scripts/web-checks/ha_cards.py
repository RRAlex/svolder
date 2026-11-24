# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""svolder's Home Assistant cards in a real browser over a mocked `hass` (ha/page.html), dark (the
svolder theme's values) and light (Home Assistant's default), 400 px (phone) and 800 px (tablet),
served from the worktree by a loopback server on a free port. Numbers, not pixels.
  the_cards_fit_and_read          every card inside the width; text against its card: main >= 7, secondary >= 3
  touch_targets                   keys, chips, rows and band boxes are at least 30 px high
  faceplate_bands                 the input and preset chips are one column at 400 and two at 800; a dial
  chain_columns                   the four modules are two across at 400, four at 800, all numbered
  chain_bypass                    the chain card's line over the modules says the daemon's words ("plays −14.8 dB in all"); the Bypass key
                                  is one switch.turn_on and one turn_off per press; commands off it is disabled and writes nothing
  tone_mode_chips                 the EQ card's mode chips (flat, eq, loudness) sit above the plot, "eq" marked
  balance_row                     the faceplate's balance slider is under the readouts, wider than 60 px
  eq_one_line                     a short plot and the five band boxes on ONE line
  eq_fields                       the selected band's Hz, dB and Q are three number fields inside the card, >= 30 px high, readable;
                                  typing a value and pressing Enter writes that entity once; commands off shows no fields
  scenes_column                   the common chips start at one x in every row; the extra chip is under its row, in that column
  one_write_per_gesture           a dial drag is one number.set_value; an EQ point drag is one per moved value
  commands_off                    the same cards over sensors: no write controls move anything, 0 calls
Usage: python3 ha_cards.py chromium|firefox   (WEB_CHECK_SHOTS=<dir> also writes a screenshot per mode and width)"""
import http.server
import os
import sys
import threading
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


def make(size):
    return common.Chromium(size=size) if browser == 'chromium' else common.Firefox(size=size)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=common.REPO, **k)

    def log_message(self, *_):
        pass


port = common.free_port()
server = http.server.ThreadingHTTPServer(('127.0.0.1', port), Quiet)
threading.Thread(target=server.serve_forever, daemon=True).start()
BASE = f'http://127.0.0.1:{port}/scripts/web-checks/ha/page.html'

TAGS = ['svolder-faceplate-card', 'svolder-eq-card', 'svolder-chain-card', 'svolder-scenes-card']
MEASURE = """(() => {
  const ACC = { 'svolder-faceplate-card': ['.dial .val', 'stroke'], 'svolder-eq-card': ['.plot .curve', 'stroke'],
    'svolder-chain-card': ['.mod.on', 'borderTopColor'], 'svolder-scenes-card': ['.lamp.on', 'backgroundColor'] };
  const r = (e) => { const b = e.getBoundingClientRect(); return { l: b.left, r: b.right, t: b.top, b: b.bottom, w: b.width, h: b.height }; };
  const out = { vw: document.documentElement.clientWidth, sw: document.documentElement.scrollWidth, cards: {} };
  for (const tag of %s) {
    const host = document.getElementById(tag);
    const root = host.shadowRoot;
    const qa = (s) => [...root.querySelectorAll(s)];
    const card = root.querySelector('.card');
    const cs = (e) => getComputedStyle(e);
    const nm = root.querySelector('.nm');
    const dim = root.querySelector('.sub, .dim, .why, .leg, .rw');
    out.cards[tag] = {
      box: r(host), scrollW: card.scrollWidth, clientW: card.clientWidth, bg: cs(card).backgroundColor,
      nm: nm ? cs(nm).color : null, dim: dim ? cs(dim).color : null,
      acc: ACC[tag] ? ((e) => (e ? cs(e)[ACC[tag][1]] : null))(root.querySelector(ACC[tag][0])) : null,
      off: (() => { const e = root.querySelector('.part.off'); return e ? cs(e).color : null; })(),
      targets: qa('.key, .chip, button.row, .band, .mod .key, .mod .set, .read input').map((e) => ({ h: r(e).h, w: r(e).w, c: e.className })),
    };
  }
  return out; })()""" % str(TAGS)
FACE = """(() => { const root = document.getElementById('svolder-faceplate-card').shadowRoot;
  const r = (e) => { const b = e.getBoundingClientRect(); return { l: b.left, t: b.top, b: b.bottom, w: b.width, h: b.height }; };
  const groups = [...root.querySelectorAll('.chips[data-group]')].map((g) => r(g));
  return { groups, dial: r(root.querySelector('.dial')), keys: [...root.querySelectorAll('.key')].map(r),
    rows: root.querySelectorAll('.rw:not(.bal)').length, strip: !!root.querySelector('.strip') }; })()"""
CHAIN = """(() => { const root = document.getElementById('svolder-chain-card').shadowRoot;
  return [...root.querySelectorAll('.mod')].map((m) => { const b = m.getBoundingClientRect();
    return { l: b.left, t: b.top, no: m.querySelector('.no').textContent }; }); })()"""
HEAD = """(() => { const root = document.getElementById('svolder-chain-card').shadowRoot;
  const k = root.querySelector('.head [data-bypass]'); if (!k) return null; const b = k.getBoundingClientRect();
  return { total: root.querySelector('.head .total').textContent, key: k.textContent, pressed: k.getAttribute('aria-pressed'),
    disabled: k.disabled, h: b.height, r: b.right, bottom: root.querySelector('.head').getBoundingClientRect().bottom,
    modTop: root.querySelector('.mods').getBoundingClientRect().top, cardR: root.querySelector('.card').getBoundingClientRect().right }; })()"""
EQ = """(() => { const root = document.getElementById('svolder-eq-card').shadowRoot;
  const r = (e) => { const b = e.getBoundingClientRect(); return { l: b.left, t: b.top, b: b.bottom, w: b.width, h: b.height }; };
  return { plot: r(root.querySelector('.plot')), bands: [...root.querySelectorAll('.band')].map(r),
    head: r(root.querySelector('.eqh')), modes: [...root.querySelectorAll('.modes .chip')].map(r),
    on: [...root.querySelectorAll('.modes .chip[aria-pressed=true]')].map((c) => c.textContent), read: root.querySelector('.read').textContent }; })()"""
FIELDS = """(() => { const root = document.getElementById('svolder-eq-card').shadowRoot;
  const cardEl = root.querySelector('.card');
  const card = cardEl.getBoundingClientRect();
  const read = root.querySelector('.read');
  return { card: { l: card.left, r: card.right, bg: getComputedStyle(cardEl).backgroundColor }, scrollW: read.scrollWidth, clientW: read.clientWidth,
    inputs: [...root.querySelectorAll('.read input')].map((e) => { const b = e.getBoundingClientRect(); const c = getComputedStyle(e);
      return { fld: e.dataset.fld, type: e.type, l: b.left, r: b.right, h: b.height, color: c.color, bg: c.backgroundColor }; }) }; })()"""
BALANCE = """(() => { const root = document.getElementById('svolder-faceplate-card').shadowRoot;
  const r = (e) => { const b = e.getBoundingClientRect(); return { r: b.right, t: b.top, b: b.bottom, w: b.width }; };
  const rows = [...root.querySelectorAll('.rw:not(.bal)')];
  return { range: r(root.querySelector('.bal input')), lastRow: r(rows[rows.length - 1]), card: r(root.querySelector('.card')) }; })()"""
SCENES = """(() => { const root = document.getElementById('svolder-scenes-card').shadowRoot;
  const r = (e) => { const b = e.getBoundingClientRect(); return { l: b.left, t: b.top, b: b.bottom }; };
  return [...root.querySelectorAll('.row')].map((row) => ({ name: row.querySelector('.name').textContent, row: r(row),
    chips: [...row.querySelector('.parts').children].map(r),
    extra: row.querySelector('.parts.extra') ? [...row.querySelector('.parts.extra').children].map(r) : [] })); })()"""


def rgb(css):
    nums = css[css.index('(') + 1:css.index(')')].replace('/', ',').split(',') if ',' in css else css[css.index('(') + 1:css.index(')')].split()
    return [float(x) for x in nums[:3]], (float(nums[3]) if len(nums) > 3 else 1.0)


def over(top, bottom):
    """`top` (a css colour that may be translucent) painted over the opaque `bottom`, as a css colour."""
    (tc, ta), (bc, _) = rgb(top), rgb(bottom)
    return 'rgb(%s)' % ','.join(str(ta * a + (1 - ta) * b) for a, b in zip(tc, bc))


def lum(c):
    def f(v):
        v /= 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])


def contrast(fg, bg):
    (fc, fa), (bc, _) = rgb(fg), rgb(bg)
    mixed = [fa * a + (1 - fa) * b for a, b in zip(fc, bc)]
    hi, lo = sorted([lum(mixed), lum(bc)], reverse=True)
    return (hi + 0.05) / (lo + 0.05)


def load(b, mode, extra=''):
    b.goto(f'{BASE}?mode={mode}{extra}', settle=0.6)
    for _ in range(60):
        if b.page('window.__ready === true'):
            break
        time.sleep(0.2)
    b.sleep(0.4)


shots = os.environ.get('WEB_CHECK_SHOTS')
for width in (400, 800):
    b = make((width, 1400))
    try:
        for mode in ('dark', 'light', 'hadark'):
            tag = f'{mode} {width}'
            load(b, mode)
            m = b.page(MEASURE)
            inside = all(c['box']['l'] >= -0.5 and c['box']['r'] <= m['vw'] + 0.5 and c['scrollW'] <= c['clientW'] + 1 for c in m['cards'].values())
            check(f'the_cards_fit_and_read: {tag}: every card inside {width} px, no sideways scroll', inside and m['sw'] <= m['vw'], m)
            ratios = {t: (round(contrast(c['nm'], c['bg']), 1), round(contrast(c['dim'], c['bg']), 1)) for t, c in m['cards'].items()}
            check(f'  {tag}: main text >= 7 and secondary >= 3 against the card', all(a >= 7 and d >= 3 for a, d in ratios.values()), ratios)
            accents = {t: round(contrast(c['acc'], c['bg']), 1) for t, c in m['cards'].items() if c['acc']}
            check(f'  {tag}: the accent marks (arc, curve, lamp, module edge) are >= 3:1 against the card',
                  len(accents) == 4 and all(v >= 3 for v in accents.values()), accents)
            offs = [round(contrast(c['off'], c['bg']), 1) for c in m['cards'].values() if c['off']]
            check(f'  {tag}: the "not connected" chip text is >= 3:1', len(offs) == 1 and offs[0] >= 3, offs)
            small = [(t, x) for t, c in m['cards'].items() for x in c['targets'] if x['h'] < 30]
            check(f'touch_targets: {tag}: every key, chip, row and band box is >= 30 px high', not small, small[:3])
            f = b.page(FACE)
            g = f['groups']
            if width == 400:
                ok = len(g) == 2 and g[1]['t'] >= g[0]['b'] - 1
            else:
                ok = len(g) == 2 and abs(g[1]['t'] - g[0]['t']) <= 2 and g[1]['l'] > g[0]['l']
            check(f'faceplate_bands: {tag}: input and preset chips {"stacked" if width == 400 else "side by side"}', ok, g)
            check('  the dial sits beside the keys and the readouts', f['dial']['w'] > 100 and f['keys'][0]['l'] > f['dial']['l'] + f['dial']['w'] - 2
                  and f['rows'] == 4 and f['strip'], f)
            c = b.page(CHAIN)
            across = len([x for x in c if abs(x['t'] - c[0]['t']) <= 2])
            check(f'chain_columns: {tag}: {2 if width == 400 else 4} modules across, numbered 1 to 4',
                  across == (2 if width == 400 else 4) and [x['no'] for x in c] == ['1', '2', '3', '4'], c)
            hd = b.page(HEAD)
            check(f'chain_bypass: {tag}: the total line and the Bypass key sit over the modules, inside the card',
                  hd is not None and hd['total'] == 'plays \u221214.8 dB in all' and hd['key'] == 'Bypass off'
                  and hd['pressed'] == 'false' and hd['h'] >= 30 and hd['bottom'] <= hd['modTop'] and hd['r'] <= hd['cardR'] + 0.5, hd)
            q = b.page(EQ)
            bt = {round(x['t']) for x in q['bands']}
            check(f'eq_one_line: {tag}: five band boxes on one line under a short plot',
                  len(q['bands']) == 5 and len(bt) == 1 and q['plot']['h'] < 140 and q['bands'][0]['t'] > q['plot']['b'] - 1, q)
            check(f'tone_mode_chips: {tag}: three chips above the plot, eq marked',
                  len(q['modes']) == 3 and q['on'] == ['eq'] and q['modes'][0]['b'] <= q['plot']['t'] + 1, q)
            fl = b.page(FIELDS)
            ins = fl['inputs']
            check(f'eq_fields: {tag}: Hz, dB and Q are number fields, inside the card, no sideways scroll, >= 30 px high',
                  [i['fld'] for i in ins] == ['f', 'g', 'q'] and all(i['type'] == 'number' and i['h'] >= 30 for i in ins)
                  and all(i['l'] >= fl['card']['l'] - 0.5 and i['r'] <= fl['card']['r'] + 0.5 for i in ins)
                  and fl['scrollW'] <= fl['clientW'] + 1, fl)
            check(f'  {tag}: the field text is >= 4.5:1 against the field', all(
                  contrast(i['color'], over(i['bg'], fl['card']['bg'])) >= 4.5 for i in ins), ins)
            bal = b.page(BALANCE)
            check(f'balance_row: {tag}: the slider is under the last readout and wider than 60 px',
                  bal['range']['w'] > 60 and bal['range']['t'] >= bal['lastRow']['b'] - 1 and bal['range']['r'] <= bal['card']['r'] + 1, bal)
            s = b.page(SCENES)
            first = {round(r['chips'][0]['l']) for r in s}
            second = {round(r['chips'][1]['l']) for r in s}
            extra = [r for r in s if r['extra']]
            check(f'scenes_column: {tag}: the common chips start at one x in every row',
                  len(s) == 3 and len(first) == 1 and len(second) == 1, s)
            check('  the extra chip is under its row\'s chips, in the same column',
                  len(extra) == 1 and abs(extra[0]['extra'][0]['l'] - extra[0]['chips'][0]['l']) <= 1
                  and extra[0]['extra'][0]['t'] >= extra[0]['chips'][0]['b'] - 1, extra)
            if shots:
                os.makedirs(shots, exist_ok=True)
                with open(f'{shots}/ha-cards-{browser}-{mode}-{width}.png', 'wb') as out:
                    out.write(b.shot())
        # the fields never clip: the widest real values in band 1
        load(b, 'dark', '&wide=1')
        clip = b.page("""(() => [...document.getElementById('svolder-eq-card').shadowRoot.querySelectorAll('.read input')]
          .map((e) => ({ fld: e.dataset.fld, value: e.value, sw: e.scrollWidth, cw: e.clientWidth })))()""")
        check(f'eq_fields: {width} px: 19497 Hz, -20.0 dB and Q 10.00 fit their fields (scrollWidth <= clientWidth)',
              [c['value'] for c in clip] == ['19497', '-20.0', '10.00'] and all(c['sw'] <= c['cw'] for c in clip), clip)
        # one write per gesture, in dark at this width
        load(b, 'dark')
        dial = b.page("(() => { const r = document.getElementById('svolder-faceplate-card').shadowRoot.querySelector('.dial')"
                      ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2, r.width / 2]; })()")
        cx, cy, rad = dial
        b.drag(cx - rad * 0.7, cy + rad * 0.7, cx, cy - rad * 0.8)
        calls = b.page('window.__calls')
        check(f'one_write_per_gesture: {width} px: a dial drag is exactly one number.set_value',
              len(calls) == 1 and calls[0][1] == 'set_value' and calls[0][2]['entity_id'] == 'number.desk_volume'
              and -96.33 <= calls[0][2]['value'] <= 0, calls)
        b.page('window.__calls.length = 0')
        pt = b.page("(() => { const r = document.getElementById('svolder-eq-card').shadowRoot.querySelectorAll('.hit')[1]"
                    ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()")
        b.drag(pt[0], pt[1], pt[0] + 40, pt[1] - 10)
        calls = b.page('window.__calls')
        ids = sorted(c[2]['entity_id'] for c in calls)
        check('  an EQ point drag writes the band\'s frequency and gain once each',
              ids == ['number.desk_band_2_frequency', 'number.desk_band_2_gain'], calls)
        b.page('window.__calls.length = 0')
        SET = ("(() => { const i = document.getElementById('svolder-eq-card').shadowRoot.querySelector('.read input[data-fld=%s]');"
               " i.focus(); i.select(); i.value = '%s'; i.dispatchEvent(new Event('input', { bubbles: true })); return !!i; })()")
        b.page(SET % ('g', '3.25'))
        check('eq_fields: typing a gain writes nothing before Enter', b.page('window.__calls.length') == 0, b.page('window.__calls'))
        b.press('Enter')
        calls = b.page('window.__calls')
        check(f'  {width} px: Enter writes the typed gain once, snapped to the entity step',
              len(calls) == 1 and calls[0][1] == 'set_value' and calls[0][2]['entity_id'] == 'number.desk_band_2_gain'
              and abs(calls[0][2]['value'] - 3.3) < 1e-6, calls)
        b.page('window.__calls.length = 0')
        b.page(SET % ('f', '77'))
        b.page("document.getElementById('svolder-eq-card').shadowRoot.querySelector('.read input[data-fld=f]').blur()")
        b.sleep(0.2)
        calls = b.page('window.__calls')
        check('  leaving the frequency field writes it once', [c[2]['entity_id'] for c in calls] == ['number.desk_band_2_frequency']
              and calls[0][2]['value'] == 77, calls)
        b.page('window.__calls.length = 0')
        GAIN = "document.getElementById('svolder-eq-card').shadowRoot.querySelector('.read input[data-fld=g]').value"
        before = b.page(GAIN)
        b.page(SET % ('g', '9'))
        b.press('Escape')
        check('  Escape writes nothing and puts the gain back', b.page('window.__calls.length') == 0 and b.page(GAIN) == before,
              [before, b.page(GAIN), b.page('window.__calls')])
        b.page('window.__calls.length = 0')
        BYP = "document.getElementById('svolder-chain-card').shadowRoot.querySelector('.head [data-bypass]')?.click()"
        b.page(BYP)
        b.page(BYP)
        calls = b.page('window.__calls')
        check(f'chain_bypass: {width} px: each press of the Bypass key is one switch call (the entity, turn_on twice: the state is the daemon\'s)',
              [(c[0], c[1], c[2]['entity_id']) for c in calls] == [('switch', 'turn_on', 'switch.pc_bypass')] * 2, calls)
        b.page('window.__calls.length = 0')
        b.click(cx, cy)
        check('  a click on the dial writes nothing', b.page('window.__calls.length') == 0, b.page('window.__calls'))
        errs = b.errors if browser == 'chromium' else b.collect_errors()
        check(f'  no page error at {width} px', [e for e in errs if 'favicon' not in e] == [], errs[:3])

        # no chain: the daemon's words are empty and the card says nothing of the level
        load(b, 'dark', '&nochain=1')
        none = b.page("(() => { const root = document.getElementById('svolder-chain-card').shadowRoot;"
                      " return [!!root.querySelector('.head .total'), !!root.querySelector('.head [data-bypass]'), root.textContent.includes('Plays')]; })()")
        check(f'chain_bypass: {width} px: with no chain the card says nothing of the level and keeps the Bypass key', none == [False, True, False], none)

        # commands off: the same cards over sensors
        load(b, 'dark', '&commands=off')
        m = b.page(MEASURE)
        check(f'commands_off: {width} px: the four cards draw', len(m['cards']) == 4 and m['sw'] <= m['vw'], m)
        if shots:
            with open(f'{shots}/ha-cards-{browser}-commands-off-{width}.png', 'wb') as out:
                out.write(b.shot())
        dial = b.page("(() => { const r = document.getElementById('svolder-faceplate-card').shadowRoot.querySelector('.dial')"
                      ".getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2, r.width / 2]; })()")
        b.drag(dial[0] - dial[2] * 0.7, dial[1] + dial[2] * 0.7, dial[0], dial[1] - dial[2] * 0.8)
        b.drag(pt[0], pt[1], pt[0] + 40, pt[1] - 10)
        for sel in ("document.getElementById('svolder-faceplate-card').shadowRoot.querySelectorAll('.key, .chip')",
                    "document.getElementById('svolder-chain-card').shadowRoot.querySelectorAll('.key')",
                    "document.getElementById('svolder-scenes-card').shadowRoot.querySelectorAll('.row')"):
            n = b.page(f'[...{sel}].map((e) => e.click()).length')
        check('  dragging and clicking every control makes no service call', b.page('window.__calls.length') == 0, b.page('window.__calls'))
        hd = b.page(HEAD)
        check('chain_bypass: commands off the Bypass key is disabled and still says the state, and its click calls nothing',
              hd is not None and hd['disabled'] and hd['key'] == 'Bypass off' and b.page('window.__calls.length') == 0, hd)
        check('  the EQ card has no number field to type in', b.page(
            "document.getElementById('svolder-eq-card').shadowRoot.querySelectorAll('.read input').length") == 0)
        check('  no text field and no switch to press', b.page(
            "document.getElementById('svolder-scenes-card').shadowRoot.querySelectorAll('.saver, button').length") == 0)
    finally:
        b.close()
server.shutdown()
print(f'ha_cards.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
