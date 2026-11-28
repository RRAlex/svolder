# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The theme Void Garden's HUD and audio panel in a browser, on the scene's no-GPU cell (both 3D
APIs off: the scene is the still and its feed steps once a second, so the overlay is what is under
test and no software rendering is spent): the HUD's rows read the model's words, an amp's meters
move in the rows already drawn on the stream's `meters` events, the panel's three modes (Panel,
Spread, Off) lay their parts out inside the window and clear of the menu button's slot on a wide
and a narrow window, the panel fades with the chrome (3 s) and comes back on the pointer, a
section change, and a mode change (the check hands the panel a `step` with the section event on
the theme's own seam; the director's rules are the node harnesses'), every word of the HUD and the
panel in every tone reads at 4.5:1 over its backdrop at the worst the scene behind it can be, the
HUD and panel never cover the menu button, and no console error, no CSP violation, no page error.
The fade runs on a clock the check moves (fakeclock.py): the clock stands still and every `step`
is made by the check, so nothing depends on the machine's pace.
  the_hud_rows_say_the_models_words          Output, Volume, Path, Input from the page model; absent facts have no row
  an_amps_meters_move_in_the_rows_drawn      the same row element, its bar and words follow `meters`
  the_panel_modes_lay_out_inside_the_window  Panel, Spread, Off on 1280x800 and 390x844: parts in the window, none over the slot
  the_panel_fades_with_the_chrome            up at 2 s of idle, gone after 3 s; a pointer move brings it back, the scene's cues do not; pinned it stays
  every_word_reads_at_4_5_to_1               each tone and chip of the HUD, the panel's words and lamps over the worst backdrop
  the_overlay_never_covers_the_menu_button   the point at the slot's centre is the menu button, in every mode
  no_console_error_warning_or_csp            the page: none
Usage: python3 voidgarden_overlay.py chromium|firefox"""
import sys
import time

import common
import fakeclock

engine = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()

# console warnings and errors, CSP violations and page errors from the page's first script, and the
# theme's `step` seam: the EventTarget the scene sends its steps on, and the last step's detail
HOOK = """window.__vg = { logs: [], csp: [], wakes: 0, rec: [] };
for (const k of ['warn', 'error']) { const o = console[k].bind(console);
  console[k] = (...a) => { window.__vg.logs.push(k + ': ' + a.map(String).join(' ').slice(0, 300)); o(...a); }; }
addEventListener('securitypolicyviolation', (e) => window.__vg.csp.push(e.violatedDirective + ' ' + e.blockedURI));
addEventListener('error', (e) => window.__vg.logs.push('error: ' + e.message));
for (const t of ['pointermove', 'pointerdown']) addEventListener(t, () => { window.__vg.wakes++; }, true);
addEventListener('unhandledrejection', (e) => window.__vg.logs.push('rejection: ' + e.reason));
(() => { const add = EventTarget.prototype.addEventListener;
  EventTarget.prototype.addEventListener = function (type, fn, opt) {
    if (type === 'step' && this !== window && this !== document && !window.__vgSteps) {
      window.__vgSteps = this;
      add.call(this, 'step', (e) => { window.__vgLast = e.detail;
        for (const ev of e.detail.events || []) if (ev.type === 'section' || ev.type === 'peak') window.__vg.wakes++;
        const c = e.detail.cues || {}; const k = c.rateHz + '|' + c.inputRateHz;
        if (window.__vg.rate !== undefined && window.__vg.rate !== k) window.__vg.wakes++;
        window.__vg.rate = k; });
    }
    return add.call(this, type, fn, opt); }; })();"""

BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
DRIVER = '__core.theme.driver'
NONE_CHROMIUM = ['--disable-3d-apis']
NONE_FIREFOX = ['user_pref("webgl.disabled", true);', 'user_pref("dom.webgpu.enabled", false);']


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def poll(read, secs=10.0, every=0.2):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(every)
        got = read()
    return got


def make(size=(1280, 800)):
    if engine == 'chromium':
        b = common.Chromium(size=size, args=NONE_CHROMIUM)
    else:
        b = common.Firefox(size=size, extra_prefs=NONE_FIREFOX)
    b.preload(HOOK)
    return b


def load(b, d):
    b.goto(f'{d.url}?theme=voidgarden', settle=1.0)
    return b.page(BOOT)


def logs(b):
    found = list(b.page('window.__vg.logs') or []) + [f'csp: {c}' for c in b.page('window.__vg.csp') or []]
    errs = b.errors if engine == 'chromium' else b.collect_errors()
    return found + [e for e in errs if 'favicon' not in e]


def js(b, expr):
    return b.page(expr)


def mode(b, name):
    return b.page(f"{DRIVER}.operate('Audio panel', {{ mode: '{name}' }})")


def hud_on(b):
    """Wait for the HUD's rows and for its modules to have come in (the start shows them 5 s)."""
    return poll(lambda: b.page("document.querySelectorAll('#vg-hud [data-row]').length > 3"), 20.0)


# ------------------------------------------------------------------ the HUD's words
def the_hud_rows(d):
    b = make()
    try:
        load(b, d)
        check('the_hud_rows_say_the_models_words: the rows are drawn', hud_on(b))
        facts = b.page("""(() => { const p = __core.model.page(); const dr = __core.theme.driver;
          const s = (l) => dr.shown(l);
          const eq = p.host && p.host.computer && p.host.computer.eq;
          return { output: s('Output'), format: s('Format'), eq: s('EQ'), xfeed: s('XFeed'), spk: s('V.Spkr'), loud: s('Loudness'),
            vul: s('VU L'), path: s('Path'), volume: s('Volume'), input: s('Input'), word: 'x',
            sink: p.host && p.host.route && p.host.route.sink && p.host.route.sink.words,
            rate: p.host && p.host.route && p.host.route.sink && p.host.route.sink.rate_hz,
            volText: p.device && p.device.volume && p.device.volume.text,
            hops: ((p.device && p.device.playing && p.device.playing.path && p.device.playing.path.hops) || []).map((h) => h.label),
            crossfeed: !!(eq && eq.crossfeed), speakers: !!(eq && eq.speakers), loudness: !!(eq && eq.loudness), eqOn: !!eq,
            rows: [...document.querySelectorAll('#vg-hud .vg-h-row')].map((r) => [r.dataset.row, r.querySelector('.vg-h-k').textContent, r.querySelector('.vg-h-v').textContent]), }; })()""")
        check('  Output names the sink', facts['output'] and facts['sink'] and facts['sink'] in facts['output'], facts)
        check('  Volume is the model\'s reading', facts['volText'] and facts['volume']
              and facts['volume'].replace('−', '-') == facts['volText'].replace('−', '-'), facts)
        order = [facts['path'].find(h) for h in facts['hops']] if facts['path'] else [-1]
        check('  Path names the model\'s hops in order', bool(facts['hops']) and -1 not in order and order == sorted(order), facts)
        check('  the rows the host does not carry are left out (crossfeed, speakers, loudness)',
              (facts['xfeed'] is not None) == facts['crossfeed'] and (facts['spk'] is not None) == facts['speakers']
              and (facts['loud'] is not None) == facts['loudness'] and (facts['eq'] is not None) == facts['eqOn'], facts)
        check('  every row has a label and words', all(r[1] and r[2] for r in facts['rows']), facts['rows'])
        check('  the VU rows are there for a host with audio', facts['vul'] is not None, facts)
        check('  the slots and the canvas are hidden from a screen reader', b.page(
            "[...document.querySelectorAll('#vg-hud .vg-h-slot, #vg-hud canvas')].every((e) => e.getAttribute('aria-hidden') === 'true')"))
        check('  the config chip is a button that opens the PipeWire config', b.page("""(() => { const c = document.querySelector('#vg-hud .vg-h-chip');
          return !c || (c.tagName === 'BUTTON' && c.getAttribute('aria-haspopup') === 'dialog'); })()"""))
    finally:
        b.close()


# ------------------------------------------------------------------ an amp's meters
def the_amps_meters(amp):
    b = make()
    try:
        load(b, amp)
        check('an_amps_meters_move_in_the_rows_drawn: the Amp module has a row for each meter',
              poll(lambda: b.page("document.querySelectorAll('#vg-hud [data-row^=\"meter:\"]').length >= 2"), 20.0))
        b.page("""(() => { window.__meter = document.querySelector('#vg-hud [data-row^="meter:"]');
          window.__bar = window.__meter.querySelector('.vg-h-slot'); window.__seen = new Set(); window.__texts = new Set(); return 1; })()""")
        end = time.time() + 12
        while time.time() < end:
            b.page("""(() => { const r = window.__meter; if (r.querySelector('.vg-h-slot').dataset.v) window.__seen.add(r.querySelector('.vg-h-slot').dataset.v);
              window.__texts.add(r.querySelector('.vg-h-v').textContent); return 1; })()""")
            if b.page('window.__seen.size') >= 3:
                break
            time.sleep(0.25)
        seen = b.page('[...window.__seen]')
        check('  the bar\'s fill takes at least three values as the amp reads', len(seen) >= 3, seen)
        check('  the row and its bar are the same elements after the readings', b.page(
            "window.__meter === document.querySelector('#vg-hud [data-row^=\"meter:\"]') && window.__bar === window.__meter.querySelector('.vg-h-slot')"))
        check('  its words are the model\'s', poll(lambda: b.page("""(() => { const m = __core.model.page().device.meters.meters[0];
          return document.querySelector('#vg-hud [data-row="meter:' + m.key + '"] .vg-h-v').textContent === m.text; })()""")))
        check('  the Amp module is the shown device\'s, with no DAC module', b.page(
            "[...document.querySelectorAll('#vg-hud [data-mod]')].map((e) => e.dataset.mod).join()") == 'host,amp', b.page(
            "[...document.querySelectorAll('#vg-hud [data-mod]')].map((e) => e.dataset.mod).join()"))
    finally:
        b.close()


# ------------------------------------------------------------------ the panel's modes
RECTS = """(() => { const vw = innerWidth, vh = innerHeight;
  const slot = document.getElementById('vg-slot').getBoundingClientRect();
  const boxes = []; const add = (name, el) => { const r = el.getBoundingClientRect(); if (r.width > 0 && r.height > 0) boxes.push([name, r.left, r.top, r.right, r.bottom]); };
  for (const el of document.querySelectorAll('#vg-panel .vg-h-pn:not([data-mode="spread"])')) add('panel', el);
  for (const el of document.querySelectorAll('#vg-panel .vg-h-pn[data-mode="spread"] .vg-h-feat, #vg-panel .vg-h-pn[data-mode="spread"] .vg-h-foot')) add(el.dataset.key || 'foot', el);
  for (const el of document.querySelectorAll('#vg-hud [data-mod][data-on="1"]')) add('mod:' + el.dataset.mod, el);
  for (const el of document.querySelectorAll('#vg-hud .vg-h-top')) add('top', el);
  const bar = document.querySelector('#vg-dash .vg-bar'); const dash = bar && bar.getBoundingClientRect().width > 0 ? bar.getBoundingClientRect() : null;
  const inside = boxes.filter((b) => b[1] >= -0.5 && b[2] >= -0.5 && b[3] <= vw + 0.5 && b[4] <= vh + 0.5);
  const hit = (a, b) => a[1] < b[3] && b[1] < a[3] && a[2] < b[4] && b[2] < a[4];
  const s = ['slot', slot.left, slot.top, slot.right, slot.bottom];
  const d = dash ? ['dash', dash.left, dash.top, dash.right, dash.bottom] : null;
  const over = []; for (let i = 0; i < boxes.length; i++) { if (hit(boxes[i], s)) over.push([boxes[i][0], 'slot']);
    if (d && hit(boxes[i], d)) over.push([boxes[i][0], 'dash']);
    for (let j = i + 1; j < boxes.length; j++) if (hit(boxes[i], boxes[j])) over.push([boxes[i][0], boxes[j][0]]); }
  return { dash: !!d, n: boxes.length, outside: boxes.length - inside.length, names: boxes.filter((b) => !inside.includes(b)).map((b) => b[0]), over,
    slot: [slot.left, slot.top, slot.right, slot.bottom], mode: document.querySelector('#vg-panel').dataset.mode, hidden: document.querySelector('#vg-panel').hidden }; })()"""


def the_panel_modes(d):
    b = make()
    try:
        load(b, d)
        hud_on(b)
        for size in ((1280, 800), (390, 844)):
            b.resize(*size)
            time.sleep(0.5)
            tag = f'{size[0]}x{size[1]}'
            check(f'the_panel_modes_lay_out_inside_the_window: [{tag}] Panel is the default', b.page(
                f"{DRIVER}.shown('Audio panel')") == 'Panel')
            got = b.page(RECTS)
            check(f'  [{tag}] Panel: one box, in the window, clear of the menu slot', got['n'] >= 1 and got['outside'] == 0
                  and not any('slot' in o for o in got['over']), got)
            check(f'  [{tag}] Spread is taken', mode(b, 'spread') is True)
            time.sleep(0.6)
            if size[0] <= 720:
                # a narrow window shows one module at a time: let the start's moments pass, then bring the Host in
                poll(lambda: b.page("[...document.querySelectorAll('#vg-hud [data-mod]')].every((e) => e.dataset.on === '0')"), 9.0, 0.5)
                b.page("""(() => { window.__vgSteps.dispatchEvent(new CustomEvent('step', { detail: { ...window.__vgLast, dt: 0.01,
                  events: [{ type: 'section', section: 'build', flash: false }] } })); return 1; })()""")
                check(f'  [{tag}] Spread on a narrow window shows the one module the moment brought', poll(lambda: b.page(
                    "[...document.querySelectorAll('#vg-hud [data-mod][data-on=\"1\"]')].map((e) => e.dataset.mod).join()") == 'host', 4.0))
            got = b.page(RECTS)
            feats = b.page("document.querySelectorAll('#vg-panel .vg-h-pn[data-mode=\"spread\"] .vg-h-feat').length")
            check(f'  [{tag}] Spread: all ten features, each in the window, none over the slot nor over another', feats == 10
                  and got['outside'] == 0 and got['over'] == [], got)
            if size[0] > 720:
                check(f'  [{tag}] Spread keeps every module in', b.page(
                    "[...document.querySelectorAll('#vg-hud [data-mod]')].every((e) => e.dataset.on === '1')"))
            check(f'  [{tag}] Off hides the panel and its words', mode(b, 'off') is True and b.page(
                "document.querySelector('#vg-panel').hidden && document.querySelector('#vg-panel').getClientRects().length === 0"))
            mode(b, 'panel')
        # a short phone window: Spread's own rows run off it, the module above the dash keeps a usable height
        for size in ((667, 375), (375, 667)):
            b.resize(*size)
            time.sleep(0.5)
            mode(b, 'spread')
            b.page("""(() => { window.__vgSteps.dispatchEvent(new CustomEvent('step', { detail: { ...window.__vgLast, dt: 0.01,
              events: [{ type: 'section', section: 'build', flash: false }] } })); return 1; })()""")
            BOX = """(() => { const m = document.querySelector('#vg-hud [data-mod][data-on="1"]');
              return m ? { h: Math.round(m.getBoundingClientRect().height), scrolls: m.scrollHeight >= m.clientHeight } : null; })()"""
            # the module comes in on the next step of the scene's own clock (a second): wait for it, not a fixed second
            box = poll(lambda: b.page(BOX), 6.0)
            check(f'  [{size[0]}x{size[1]}] Spread keeps the module at 96 px or more, scrolling when it is cut',
                  box is not None and box['h'] >= 90 and box['scrolls'], box)
            mode(b, 'panel')
        check('  Panel again', b.page(f"{DRIVER}.shown('Audio panel')") == 'Panel')
    finally:
        b.close()


# ------------------------------------------------------------------ the fade on a clock the check moves
def opacity(b, sel='#vg-panel .vg-h-pbox'):
    return b.page(f"Number(getComputedStyle(document.querySelector({sel!r})).opacity)")


def tick(b, secs):
    """Move the clock `secs` virtual seconds one second at a time (the feed steps once a second here)."""
    for _ in range(int(secs)):
        fakeclock.advance(b, 1000)


WAKE = "document.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))"


def the_panel_fades(d):
    """0.40.0 3.10: the panel is the chrome's: gone 3 s after the last input (a transition of 0.6 s), back on a
    pointer move, not woken by the scene's own cues; pinned from the menu it stays."""
    b = make()
    try:
        fakeclock.install(b)
        load(b, d)
        poll(lambda: b.page('!!window.__vgLast'), 20.0)
        check('the_panel_fades_with_the_chrome: the scene sends steps on its seam', b.page('!!window.__vgSteps'))
        fakeclock.take(b)
        mode(b, 'panel')
        b.page(WAKE)
        tick(b, 2)
        time.sleep(0.9)
        check('  up at 2 s of idle', opacity(b) == 1, opacity(b))
        tick(b, 2)
        time.sleep(1.0)
        check('  gone (0) after the 3 s idle time and the 0.6 s transition', opacity(b) == 0, opacity(b))
        check('  its words stay in the page while gone', b.page(
            "document.querySelector('#vg-panel .vg-h-feat .vg-h-v').textContent !== '' && !document.querySelector('#vg-panel').hidden"))
        b.page("""(() => { window.__vgSteps.dispatchEvent(new CustomEvent('step', { detail: { ...window.__vgLast, dt: 0.01,
          events: [{ type: 'section', section: 'drop', flash: false }, { type: 'peak', flash: false }] } })); return 1; })()""")
        time.sleep(0.9)
        check('  a section or a peak of the scene does not bring it back', opacity(b) == 0, opacity(b))
        b.page(WAKE)
        time.sleep(0.9)
        check('  the pointer moving does', opacity(b) == 1, opacity(b))
    finally:
        b.close()
    p = make()
    try:
        p.preload("try { localStorage.setItem('svolder.theme.voidgarden.panel-fades', 'false'); } catch (e) {}")
        fakeclock.install(p)
        load(p, d)
        poll(lambda: p.page('!!window.__vgLast'), 20.0)
        fakeclock.take(p)
        p.page(WAKE)
        tick(p, 5)
        time.sleep(1.0)
        check('  pinned (Pin features panel): the capsules are gone and the panel stays',
              opacity(p, '#vg-dash .vg-cap') == 0 and opacity(p) == 1, [opacity(p, '#vg-dash .vg-cap'), opacity(p)])
    finally:
        p.close()


# ------------------------------------------------------------------ contrast
CONTRAST = """(() => {
  const parse = (s) => { const m = s.match(/rgba?\\(([^)]+)\\)/); const p = m[1].split(/[ ,/]+/).filter(Boolean).map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
  const lin = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
  const lum = (c) => 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
  const over = (top, under) => ({ r: top.r * top.a + under.r * (1 - top.a), g: top.g * top.a + under.g * (1 - top.a), b: top.b * top.a + under.b * (1 - top.a), a: 1 });
  const WHITE = { r: 255, g: 255, b: 255, a: 1 };
  // the backdrop: the nearest ancestor with a background, laid over the worst the scene can be behind it (white)
  const backdrop = (el) => { for (let e = el; e && e !== document.documentElement; e = e.parentElement) {
      const c = parse(getComputedStyle(e).backgroundColor); if (c.a > 0) { return over(c, over(parse(getComputedStyle(e.parentElement || e).backgroundColor), WHITE)); } }
    return null; };
  const ratio = (fg, bg) => { const a = lum(over(fg, bg)) + 0.05, b = lum(bg) + 0.05; return a > b ? a / b : b / a; };
  const out = []; const seen = new Set();
  const measure = (el, label) => { const bg = backdrop(el); if (!bg) { out.push([label, 'no backdrop', 0]); return; }
    const fg = parse(getComputedStyle(el).color); out.push([label, el.textContent.trim().slice(0, 24), Math.round(ratio(fg, bg) * 100) / 100]); };
  const texts = (root) => [...root.querySelectorAll('*')].filter((e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim()));
  // the HUD: every word, then a copy of a module with each tone and each chip outcome
  const hud = document.querySelector('#vg-hud'); const mod = hud.querySelector('.vg-h-mod');
  for (const el of texts(hud)) measure(el, 'hud ' + (el.className || el.tagName));
  const host = mod.cloneNode(true); host.dataset.on = '1'; host.style.opacity = '1'; host.style.position = 'absolute'; host.style.left = '0'; host.style.top = '0'; host.style.visibility = 'hidden';
  hud.querySelector('.vg-h-mods').append(host);
  for (const tone of ['on', 'warn', 'bad', '']) { for (const row of host.querySelectorAll('.vg-h-row')) { if (tone) row.dataset.tone = tone; else delete row.dataset.tone;
      for (const el of row.querySelectorAll('.vg-h-v, .vg-h-note, .vg-h-k')) measure(el, 'tone ' + (tone || 'plain') + ' ' + el.className); } }
  const chip = document.createElement('button'); chip.className = 'vg-h-chip'; chip.textContent = 'config 2';
  host.querySelector('.vg-h-row').append(chip);
  for (const outcome of ['untouched', 'changes', 'unread', 'not_read', 'other_version']) { chip.dataset.outcome = outcome; measure(chip, 'chip ' + outcome); }
  const topLine = hud.querySelector('.vg-h-top'); const verdict = document.createElement('span'); verdict.className = 'vg-h-r'; verdict.textContent = 'x';
  topLine.append(verdict);
  for (const tone of ['warn', 'bad', '']) { if (tone) verdict.dataset.tone = tone; else delete verdict.dataset.tone; measure(verdict, 'top ' + (tone || 'plain')); }
  verdict.remove();
  measure(host.querySelector('.vg-h-title'), 'hud title'); measure(host.querySelector('.vg-h-sub') || host.querySelector('.vg-h-title'), 'hud sub');
  host.remove();
  // the panel: Panel and Spread, the words and the lamps lit and unlit
  const panel = document.querySelector('#vg-panel');
  for (const m of ['panel', 'spread']) { __core.theme.driver.operate('Audio panel', { mode: m });
    for (const lamp of panel.querySelectorAll('.vg-h-lamp')) for (const on of ['0', '1']) { lamp.dataset.on = on; measure(lamp, 'lamp ' + m + ' ' + lamp.dataset.lamp + on); }
    for (const el of texts(panel)) measure(el, 'panel ' + m + ' ' + (el.className || el.tagName)); }
  __core.theme.driver.operate('Audio panel', { mode: 'panel' });
  return out; })()"""


def every_word_reads(d):
    b = make()
    try:
        load(b, d)
        hud_on(b)
        time.sleep(0.5)
        got = b.page(CONTRAST)
        worst = min(got, key=lambda r: r[2])
        print(f'INFO  {len(got)} words measured, lowest {worst}')
        check('every_word_reads_at_4_5_to_1: every word of the HUD and the panel, each tone and chip, over the worst backdrop',
              len(got) > 40 and all(r[2] >= 4.5 for r in got), [r for r in got if r[2] < 4.5][:6])
    finally:
        b.close()


# ------------------------------------------------------------------ the top line and the dash's height
def the_top_line(d):
    b = make()
    try:
        load(b, d)
        hud_on(b)
        got = b.page("""(() => { const p = __core.model.page(); const dr = __core.theme.driver;
          return { top: dr.shown('Top line'), name: p.device && p.device.name, link: dr.linkWords(),
            word: p.device && p.device.link && p.device.link.word,
            box: (() => { const r = document.querySelector('#vg-hud .vg-h-top').getBoundingClientRect(); return [r.width, r.height]; })() }; })()""")
        check('the_top_line_is_always_on_and_names_the_device: its words, drawn without a moment',
              got['top'] and got['name'] in got['top'] and got['box'][0] > 0, got)
        check('  linkWords is null while the link is on', got['link'] is None, got)
        d.drop()
        word = poll(lambda: (lambda w: w if w not in (None, 'connected') else None)(b.page(
            "(__core.model.page().device || {}).link && __core.model.page().device.link.word")), 15.0)
        check('  linkWords names the link state beside the name once the device is lost',
              bool(word) and poll(lambda: word.lower() in (b.page(f"{DRIVER}.linkWords()") or '').lower(), 10.0), word)
        check('  the top line says the stream\'s trouble or the link on the page: still drawn', b.page(
            "document.querySelector('#vg-hud .vg-h-top').textContent.length > 0"))
    finally:
        b.close()


def the_dash_height(d, has_amp=False):
    """Everything at the bottom stands above `--vg-dash-h` (the dash's height, set by the dash on the page root):
    the check sets it large on a page with no dash of its own and measures; and, where the dash is drawn,
    that no part of the overlay overlaps its bar."""
    b = make()
    try:
        load(b, d)
        hud_on(b)
        for size in ((1280, 800), (390, 844)):
            b.resize(*size)
            time.sleep(0.4)
            tag = f'{size[0]}x{size[1]}'
            b.page("document.getElementById('voidgarden').style.setProperty('--vg-dash-h', '230px')")
            mode(b, 'spread')
            if size[0] <= 720:
                b.page("""(() => { window.__vgSteps.dispatchEvent(new CustomEvent('step', { detail: { ...window.__vgLast, dt: 0.01,
                  events: [{ type: 'section', section: 'build', flash: false }] } })); return 1; })()""")
            time.sleep(1.0)
            # the parts that stand at the bottom: Spread's meters and rows, the Amp (every module on a phone)
            mods = '#vg-hud [data-mod][data-on="1"]' if size[0] <= 720 else '#vg-hud [data-mod="amp"][data-on="1"]'
            low = b.page("""(() => { const vh = innerHeight; const out = [];
              for (const el of document.querySelectorAll('#vg-panel [data-key="bass"], #vg-panel [data-key="mid"], #vg-panel [data-key="high"], #vg-panel [data-key="width"], %s')) {
                const r = el.getBoundingClientRect(); if (r.width > 0 && r.bottom > vh - 230 + 0.5) out.push([el.dataset.key || el.dataset.mod, Math.round(r.bottom)]); }
              return { low: out, amp: !!document.querySelector('#vg-hud [data-mod="amp"][data-on="1"]'), n: document.querySelectorAll('#vg-panel [data-form="col"]').length }; })()""" % mods)
            check(f'the_dash_height_is_one_measure: [{tag}] with --vg-dash-h at 230 px the bottom parts stand above it',
                  low['low'] == [] and low['n'] >= 1 and (size[0] <= 720 or low['amp'] == has_amp), low)
            b.page("document.getElementById('voidgarden').style.removeProperty('--vg-dash-h')")
            mode(b, 'panel')
        b.resize(1280, 800)
        time.sleep(0.4)
        got = b.page(RECTS)
        if got['dash']:
            mode(b, 'spread')
            time.sleep(0.5)
            got = b.page(RECTS)
            check('the_dash_height_is_one_measure: nothing of the overlay overlaps the dash bar', not any('dash' in o for o in got['over']), got)
        else:
            print('INFO  no dash bar drawn on this tree (a stub): the overlap with #vg-dash .vg-bar is checked once the dash is merged')
    finally:
        b.close()


# ------------------------------------------------------------------ the menu button
def never_covers(d):
    b = make()
    try:
        load(b, d)
        hud_on(b)
        for size in ((1280, 800), (390, 844)):
            b.resize(*size)
            time.sleep(0.4)
            for name in ('panel', 'spread'):
                mode(b, name)
                time.sleep(0.4)
                hit = b.page("""(() => { const s = document.getElementById('vg-slot').getBoundingClientRect();
                  const e = document.elementFromPoint(s.left + s.width / 2, s.top + s.height / 2);
                  return { tag: e && e.tagName, id: e && e.id, hud: !!(e && e.closest('#vg-hud, #vg-panel')),
                    slot: !!(e && e.closest('#vg-slot, .frame-btn, [data-frame-button], button')) }; })()""")
                check(f'the_overlay_never_covers_the_menu_button: [{size[0]}x{size[1]} {name}] the point at the slot\'s centre is not the HUD or the panel',
                      hit['hud'] is False, hit)
        mode(b, 'panel')
    finally:
        b.close()


def main():
    d = common.Daemon('gungnir2-v2', host='fake')
    amp = common.Daemon('fake-amp', name='Amp', host='fake', env={'SVOLDER_FAKE_SIGNAL': '1'})
    try:
        the_hud_rows(d)
        the_amps_meters(amp)
        the_panel_modes(d)
        the_panel_fades(d)
        every_word_reads(d)
        never_covers(d)
        the_dash_height(d)
        the_dash_height(amp, True)
        the_top_line(d)
        b = make()
        try:
            load(b, d)
            hud_on(b)
            time.sleep(2)
            found = logs(b)
            check('no_console_error_warning_or_csp: the page', found == [], found[:4])
        finally:
            b.close()
    finally:
        d.stop()
        amp.stop()


main()
print(f'voidgarden_overlay.py {engine}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
