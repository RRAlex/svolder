# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The computer's level meters in the theme Signal, in a real page on the fake host. The page's
EventSource for the frames streams is the check's own fake (the visualizer checks' method), so the
levels the needles and lamps hear are exactly the ones the check sends; nothing waits on a clock but
the needle's own settling.
  the_computer_opens_with_meters_and_two_dial_buttons   Meters shown, two dial face buttons, the levels
                                        stream held and the full one not
  the_needles_settle_at_the_rotation_the_model_says     both needles within 0.5 degrees of the scale's
                                        angle for the level sent; the readings say it
  the_lamp_lights_on_a_255_peak_and_is_off_later        lit at a peak byte 255 on its channel only, off
                                        1.3 s after the last
  the_reference_steps_by_one_db_and_stops_at_its_ends   +/- move the reading 1 dB, stop at -6 and -30,
                                        Reset shows only off -18 and restores
  the_face_cycles_the_glow_by_click_enter_and_space     W3 -> W1 -> W2 -> W3 (class and label)
  the_choices_survive_a_reload                          glow, view and reference come back
  refused_storage_leaves_the_defaults_and_no_error      a browser whose storage throws: the defaults
  the_spectrum_view_swaps_the_streams_and_stops_the_needles   canvas drawn, no needle write and no
                                        animation frame for 1 s, the
                                        levels stream closed and the full one held, and back
  a_hidden_page_holds_no_stream_after_the_grace         hidden: none after 2.5 s, shown: held again
  the_capture_state_is_said_and_the_needles_rest        unavailable: a note and the left stop
  the_markup_has_no_style_attribute                     the drawn section carries none
  the_layout_holds_at_390_900_1020_1239_1240_1241_1280   no horizontal scroll, dials inside the area and side by
                                        side, the curve at least 300 px wide from 720 px, the third column from
                                        1240 px, beside the volume below it
  reduced_motion_jumps_the_needle_to_the_settled_value   prefers-reduced-motion: one frame and two animation
                                        frames later the needles stand at the model's angle (an eased needle would
                                        not); the lamp lights; the Spectrum line is drawn when bands arrive
  leaving_the_computer_side_closes_the_streams_and_stops_the_needles   flip to the device hop: no stream, no write
  Reset_hands_the_focus_to_the_plus_button_when_it_hides   the focus does not fall to the page
  a_frame_writes_two_needle_transforms_at_most          the style writes are at most two per animation frame
  the_needle_loop_stops_in_silence                      no animation frame asked for by the needles' loop with
                                        nothing fed; a frame, or a reference change, wakes it; a settled
                                        constant level writes nothing and asks for no frame
  a_stored_reference_that_is_not_a_whole_number_is_refused   the default applies
  a_guest_turns_the_glow_too                            display choices need no write right
Stills (each glow at 1280, 900 and 390) go under target/web-shots/signal/vu/, or VU_STILLS=dir (Chromium run).
Usage: python3 signal_vu.py chromium|firefox"""
import json
import os
import sys
import time

import common

os.environ.setdefault('SVOLDER_FAKE_COMPUTER_VOLUME', '42')

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0
t0 = time.time()
SHOTS = f'{common.REPO}/target/web-shots/signal'

FLEET = [('fake-amp', 'Amp', '1112131415161718')]
HOOK = """window.__es = [];
const Real = window.EventSource;
window.EventSource = class extends EventTarget {
  constructor(url) {
    super();
    if (!String(url).includes('host/frames')) return new Real(url);
    this.url = String(url); this.readyState = 1; this.closed = false;
    window.__es.push(this);
    setTimeout(() => this.dispatchEvent(new Event('open')), 0);
  }
  close() { this.readyState = 2; this.closed = true; }
};
window.EventSource.CLOSED = 2;
window.__emit = (match, type, data) => {
  for (const s of window.__es) {
    if (!s.closed && s.url.includes(match)) s.dispatchEvent(Object.assign(new Event(type), { data: JSON.stringify(data) }));
  }
};
window.__open = () => window.__es.filter((s) => !s.closed).map((s) => (s.url.includes('only=levels') ? 'levels' : 'full'));
window.__beat = 0; window.__lv = null; window.__feeding = 0;
window.__feed = (lv) => {
  window.__lv = lv;
  if (window.__feeding) return;
  window.__feeding = setInterval(() => {
    if (!window.__lv) return;
    const bands = btoa(String.fromCharCode(...Array.from({ length: 240 }, (_, i) => 60 + (i % 80))));
    window.__beat += 33;
    window.__emit('host/frames', 'frame', { t: Date.now(), l: bands, r: bands, lv: window.__lv, beat: 0, bpm: null, phase: 0 });
  }, 33);
};
window.__quiet = () => { clearInterval(window.__feeding); window.__feeding = 0; window.__lv = null; };
window.__vis = 'visible';
Object.defineProperty(document, 'visibilityState', { get: () => window.__vis });
const nativeRaf = window.requestAnimationFrame.bind(window);
window.__raf = 0; (function count() { window.__raf++; nativeRaf(count); })();
window.__vuRaf = 0;
window.requestAnimationFrame = (cb) => {
  if (new Error().stack.includes('vu-wire')) window.__vuRaf++;
  return nativeRaf(cb);
};"""
NO_STORAGE = "Object.defineProperty(window, 'localStorage', { get() { throw new Error('denied'); } });"
BOOT = """import(new URL('assets/core/boot.js', document.baseURI)).then(async (m) => {
  await m.mounted; window.__core = m.core; return m.core.theme.id; })"""
HOP = '#sg-line [data-hop="computer"]'
DEVICE_HOP = '#sg-line [data-hop="device:Gungnir%202"]'
AREA = '#sg-cvu'
LEFT = f'{AREA} .sg-vneedle[data-needle="L"]'
RIGHT = f'{AREA} .sg-vneedle[data-needle="R"]'


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(size=(1280, 1100), storage=True, reduced=False):
    b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=size, reduced=reduced)
    b.preload(HOOK)
    if not storage:
        b.preload(NO_STORAGE)
    return b


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def poll(read, secs=6.0):
    got = read()
    end = time.time() + secs
    while not got and time.time() < end:
        time.sleep(0.1)
        got = read()
    return got


def load(b, d, side='computer'):
    b.goto(f'{d.url}?theme=signal&effects=off', settle=1.0)
    assert b.page(BOOT) == 'signal'
    poll(lambda: b.page('!!(__core.model.page().host?.computer?.volume)'))
    time.sleep(0.4)
    if side == 'computer':
        flip(b)


def tap(b, selector):
    b.page(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    time.sleep(0.15)
    x, y = common.center(b, selector)
    b.click(x, y)
    time.sleep(0.3)


def flip(b):
    tap(b, HOP)
    poll(lambda: b.page("__core.computer.side() === 'computer'"))
    poll(lambda: b.page(f"!!document.querySelector('{AREA}')"))
    time.sleep(0.5)


def click(b, selector):
    """A click on the element itself (the section's own handler), without moving a pointer."""
    b.page(f"document.querySelector({json.dumps(selector)}).click()")
    time.sleep(0.12)


def angle(b, selector):
    """The needle's rotation in degrees, from the style the page wrote."""
    text = b.page(f"document.querySelector({json.dumps(selector)}).style.transform")
    return float(text.replace('rotate(', '').replace('deg)', '')) if text else None


def expected(byte, ref=-18):
    """The scale's angle for an RMS byte: amplitude over the reference's, then the needle's span."""
    amp = 10 ** ((byte / 255 * 72 - 72) / 20)
    f = (amp / 10 ** (ref / 20) - 0.1) / (10 ** (3 / 20) - 0.1)
    return -42 + 84 * max(0.0, min(1.03, f))


def vu_of(words):
    """A reading's text as a number (the byte for 0 VU rounds a tenth away)."""
    return float(words.replace('\u2212', '-').replace('+', ''))


def text(b, selector):
    return b.page(f"document.querySelector({json.dumps(selector)})?.textContent ?? null")


def run(name, fn, **kw):
    web = kw.pop('web', 'auth = "none"')
    storage = kw.pop('storage', True)
    size = kw.pop('size', (1280, 1100))
    reduced = kw.pop('reduced', False)
    d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET, web=web)
    b = make(size, storage, reduced)
    try:
        fn(b, d)
        check(f'{name}: no page error', errors(b) == [], errors(b)[:3])
    finally:
        b.close()
        d.stop()


# ---------------------------------------------------------------------------------------- cases

def opens(b, d):
    load(b, d)
    time.sleep(0.4)
    got = b.page(f"""({{ view: document.querySelector('{AREA}').dataset.view,
      faces: document.querySelectorAll('{AREA} button.sg-vface').length, open: __open(),
      glow: document.querySelector('{AREA}').className,
      label: document.querySelector('#sg-vface-L').getAttribute('aria-label'),
      spec: document.querySelector('[data-pane="spectrum"]').hidden }})""")
    check('the_computer_opens_with_meters_and_two_dial_buttons',
          got['view'] == 'meters' and got['faces'] == 2 and got['open'] == ['levels'] and got['spec'], got)
    check('  and the first glow is the filament one', 'sg-w3' in got['glow']
          and got['label'] == 'Meter glow: filament. Change', got)


def needles(b, d):
    load(b, d)
    b.page('__feed([191, 150, 200, 160])')
    time.sleep(1.6)
    left, right = angle(b, LEFT), angle(b, RIGHT)
    check('the_needles_settle_at_the_rotation_the_model_says',
          left is not None and right is not None and abs(left - expected(191)) <= 0.5
          and abs(right - expected(150)) <= 0.5, (left, right, expected(191), expected(150)))
    reads = (text(b, '#sg-vnum-L'), text(b, '#sg-vnum-R'))
    check('  and the readings say it', abs(vu_of(reads[0])) <= 0.2 and reads[1].startswith('−'), reads)
    b.page('__quiet()')


def lamp(b, d):
    load(b, d)
    b.page('__feed([191, 191, 255, 200])')
    time.sleep(0.4)
    on = b.page("[...document.querySelectorAll('.sg-vlamp')].map((l) => l.classList.contains('on'))")
    check('the_lamp_lights_on_a_255_peak_and_is_off_later: lit on its channel only', on == [True, False], on)
    spoken = b.page("[...document.querySelectorAll('.sg-vlamp .sg-sr')].map((l) => l.textContent)")
    check('  and the lit lamp is spoken: over on the left, off on the right', spoken == [', over', ', off'], spoken)
    b.page('__feed([191, 191, 200, 200])')
    time.sleep(0.5)
    still = b.page("document.querySelector('#sg-vlamp-L').classList.contains('on')")
    time.sleep(1.2)
    off = b.page("document.querySelector('#sg-vlamp-L').classList.contains('on')")
    check('  still lit within the hold, off after it', still is True and off is False, (still, off))
    words = b.page("[...document.querySelectorAll('.sg-vlamp')].map((l) => l.textContent)")
    check('  the reading rows name their channel, the lamp text goes with the light',
          words == ['Peak, off', 'Peak, off'] and b.page("[...document.querySelectorAll('.sg-vrd')].map((r) => r.getAttribute('aria-label'))")
          == ['Left level', 'Right level'], words)
    b.page('__quiet()')


def reference(b, d):
    load(b, d)
    b.page('__feed([191, 191, 100, 100])')
    time.sleep(1.4)
    check('the_reference_steps_by_one_db_and_stops_at_its_ends: starts at 0 VU = -18 with no Reset',
          text(b, '#sg-vref-out') == '0 VU = −18 dBFS'
          and b.page("document.querySelector('#sg-vref-reset').hidden") is True)
    click(b, '#sg-vref-up')
    time.sleep(1.4)
    check('  one step raises the reference and the reading falls 1 dB',
          text(b, '#sg-vref-out') == '0 VU = −17 dBFS' and abs(vu_of(text(b, '#sg-vnum-L')) + 1.0) <= 0.2
          and b.page("document.querySelector('#sg-vref-reset').hidden") is False,
          (text(b, '#sg-vref-out'), text(b, '#sg-vnum-L')))
    for _ in range(14):
        click(b, '#sg-vref-up')
    check('  it stops at -6', text(b, '#sg-vref-out') == '0 VU = −6 dBFS'
          and b.page("document.querySelector('#sg-vref-up').getAttribute('aria-disabled')") == 'true',
          text(b, '#sg-vref-out'))
    for _ in range(30):
        click(b, '#sg-vref-down')
    check('  and at -30', text(b, '#sg-vref-out') == '0 VU = −30 dBFS', text(b, '#sg-vref-out'))
    click(b, '#sg-vref-reset')
    check('  Reset restores -18 and hides itself', text(b, '#sg-vref-out') == '0 VU = −18 dBFS'
          and b.page("document.querySelector('#sg-vref-reset').hidden") is True)
    b.page('__quiet()')


def glow(b, d):
    load(b, d)
    seen = []
    for how in ('click', 'click', 'click', 'Enter', ' '):
        if how == 'click':
            click(b, '#sg-vface-L')
        else:
            b.page("document.querySelector('#sg-vface-R').focus()")
            b.press(how)
            time.sleep(0.2)
        seen.append(b.page(f"[document.querySelector('{AREA}').className.match(/sg-w\\d/)[0],"
                           " document.querySelector('#sg-vface-L').getAttribute('aria-label'),"
                           " document.querySelector('#sg-vface-R').getAttribute('aria-label')]"))
    names = [s[0] for s in seen]
    check('the_face_cycles_the_glow_by_click_enter_and_space',
          names == ['sg-w1', 'sg-w2', 'sg-w3', 'sg-w1', 'sg-w2'], seen)
    check('  and both faces name the glow', seen[0][1] == seen[0][2] == 'Meter glow: subtle. Change'
          and seen[1][1] == 'Meter glow: warm backlight. Change', seen[:2])


def kept(b, d):
    load(b, d)
    click(b, '#sg-vface-L')
    click(b, '#sg-vref-up')
    click(b, '#sg-vref-up')
    click(b, '#sg-vview-spectrum')
    b.goto(f'{d.url}?theme=signal&effects=off', settle=1.0)
    assert b.page(BOOT) == 'signal'
    poll(lambda: b.page(f"!!document.querySelector('{AREA}')"))
    got = b.page(f"""({{ cls: document.querySelector('{AREA}').className, view: document.querySelector('{AREA}').dataset.view,
      ref: document.querySelector('#sg-vref-out').textContent, open: __open() }})""")
    check('the_choices_survive_a_reload', 'sg-w1' in got['cls'] and got['view'] == 'spectrum'
          and got['ref'] == '0 VU = −16 dBFS' and got['open'] == ['full'], got)


def refused(b, d):
    load(b, d)
    got = b.page(f"""({{ cls: document.querySelector('{AREA}').className, view: document.querySelector('{AREA}').dataset.view,
      ref: document.querySelector('#sg-vref-out').textContent }})""")
    click(b, '#sg-vface-L')
    click(b, '#sg-vref-up')
    after = b.page(f"[document.querySelector('{AREA}').className, document.querySelector('#sg-vref-out').textContent]")
    check('refused_storage_leaves_the_defaults_and_no_error', 'sg-w3' in got['cls'] and got['view'] == 'meters'
          and got['ref'] == '0 VU = −18 dBFS' and 'sg-w1' in after[0] and after[1].endswith('−17 dBFS'), (got, after))


MUTES = """(() => { window.__writes = 0;
  new MutationObserver((records) => { for (const r of records) if (r.attributeName === 'style') window.__writes++; })
    .observe(document.querySelector('#sg-cvu'), { attributes: true, subtree: true, attributeFilter: ['style'] }); })()"""


def spectrum(b, d):
    load(b, d)
    b.page('__feed([191, 150, 200, 160])')
    time.sleep(2.5)
    b.page('window.__vuRaf = 0')
    click(b, '#sg-vview-spectrum')
    time.sleep(0.8)
    got = b.page("({ open: __open(), meters: document.querySelector('[data-pane=\"meters\"]').hidden,"
                 " pressed: document.querySelector('#sg-vview-spectrum').getAttribute('aria-pressed') })")
    check('the_spectrum_view_swaps_the_streams_and_stops_the_needles: the full stream is held, the levels one closed',
          got == {'open': ['full'], 'meters': True, 'pressed': 'true'}, got)
    drawn = b.page("""(() => { const c = document.querySelector('#sg-vspec'); const x = c.getContext('2d');
      const px = x.getImageData(0, 0, c.width, c.height).data; let n = 0;
      for (let y = 0; y < c.height * 0.85; y++) for (let i = 0; i < c.width; i++) if (px[(y * c.width + i) * 4 + 3] > 150) n++;
      return { n, w: c.width, h: c.height }; })()""")
    check('  the canvas draws the line from the bands sent (strong pixels well above the floor)', drawn['n'] > 200 and drawn['w'] > 100, drawn)
    b.page(MUTES)
    time.sleep(1.0)
    writes = b.page('window.__writes')
    check('  no needle is written for a second', writes == 0, writes)
    check('  and the needles\' animation loop is stopped from the tap on (no frame asked for)',
          b.page('window.__vuRaf') == 0, b.page('window.__vuRaf'))
    click(b, '#sg-vview-meters')
    time.sleep(0.8)
    check('  back to Meters: the levels stream again, the full one closed', b.page('__open()') == ['levels'], b.page('__open()'))
    b.page('__quiet()')


def hidden(b, d):
    load(b, d)
    b.page('window.__vuRaf = 0')
    b.page("(() => { window.__vis = 'hidden'; document.dispatchEvent(new Event('visibilitychange')); })()")
    time.sleep(2.6)
    gone = b.page('__open()')
    still = b.page('window.__vuRaf')
    b.page("(() => { window.__vis = 'visible'; document.dispatchEvent(new Event('visibilitychange')); })()")
    time.sleep(0.6)
    back = b.page('__open()')
    check('a_hidden_page_holds_no_stream_after_the_grace: and its needle loop is stopped', gone == [] and back == ['levels'] and still == 0,
          (gone, back, still))


def capture_state(b, d):
    load(b, d)
    b.page('__feed([191, 191, 200, 200])')
    time.sleep(1.2)
    before = angle(b, LEFT)
    b.page('__quiet()')
    b.page("__emit('only=levels', 'capture', { capture: 'unavailable', why: 'no sink' })")
    time.sleep(0.9)
    note = text(b, '#sg-vstate')
    rest = angle(b, LEFT)
    check('the_capture_state_is_said_and_the_needles_rest',
          before > -30 and 'no sink' in (note or '') and rest is not None and rest <= -41.5, (before, note, rest))


def markup(b, d):
    load(b, d)
    html = b.page("import(new URL('assets/themes/signal/draw/vu.js', document.baseURI)).then((m) => m.vuArea({"
                  " glow: 'w3', view: 'meters', ref: -18, defaults: -18, min: -30, max: -6, note: '' }))")
    check('the_markup_has_no_style_attribute', 'style=' not in html and '<script' not in html and len(html) > 500, html[:80])


def layout(b, d):
    load(b, d)
    # the widths: the phone, the two-row layout, the breakpoint of the third column and a step either side
    for w, h in ((390, 844), (900, 1000), (1020, 1000), (1239, 1100), (1240, 1100), (1241, 1100), (1280, 1100)):
        b.resize(w, h)
        time.sleep(0.6)
        got = b.page("""(() => { const area = document.querySelector('#sg-open').getBoundingClientRect();
          const faces = [...document.querySelectorAll('.sg-vface')].map((f) => f.getBoundingClientRect());
          const g = document.querySelector('#sg-cgraph').getBoundingClientRect();
          const v = document.querySelector('#sg-cvol, #sg-cvolume').getBoundingClientRect();
          return { scroll: document.documentElement.scrollWidth - window.innerWidth,
            inside: faces.every((r) => r.left >= area.left - 1 && r.right <= area.right + 1 && r.width > 100),
            curve: g.width, side: Math.abs(faces[0].top - faces[1].top) < 4 && faces[1].left > faces[0].right - 1,
            third: faces[0].left > g.right - 1, beside: faces[0].left > v.right - 1 && faces[0].top < v.bottom }; })()""")
        ok = got['scroll'] <= 0 and got['inside'] and (got['curve'] >= 300 or w < 720)
        check(f'the_layout_holds_at_{w}: no scroll, dials inside, curve {got["curve"]:.0f} px', ok, got)
        check(f'  the dials are side by side at {w}', got['side'], got)
        if w >= 1240:
            check(f'  and they are the third column at {w}', got['third'], got)
        elif w >= 720:
            check(f'  and they are beside the volume at {w}', got['beside'], got)


def reduced_motion(b, d):
    load(b, d)
    # one frame, then two animation frames: an eased needle is still near its stop, a settled one is at the angle
    left, right = b.page(f"""new Promise((done) => {{
      __emit('only=levels', 'frame', {{ t: Date.now(), lv: [191, 150, 255, 160] }});
      requestAnimationFrame(() => requestAnimationFrame(() => done(['{LEFT}', '{RIGHT}'].map((q) => {{
        const t = document.querySelector(q).style.transform; return t ? Number(t.replace('rotate(', '').replace('deg)', '')) : null; }}))));
    }})""")
    lit = b.page("[...document.querySelectorAll('.sg-vlamp')].map((l) => l.classList.contains('on'))")
    check('reduced_motion_jumps_the_needle_to_the_settled_value: no easing, the needles stand at the model\'s angle',
          left is not None and abs(left - expected(191)) <= 0.5 and abs(right - expected(150)) <= 0.5,
          (left, right, expected(191), expected(150)))
    check('  and the lamp still lights on a 255 peak', lit == [True, False], lit)
    b.page('__quiet()')
    click(b, '#sg-vview-spectrum')
    b.page('__feed([191, 150, 200, 160])')
    time.sleep(1.2)
    drawn = b.page("""(() => { const c = document.querySelector('#sg-vspec'); const x = c.getContext('2d');
      const px = x.getImageData(0, 0, c.width, c.height).data; let n = 0;
      for (let y = 0; y < c.height * 0.85; y++) for (let i = 0; i < c.width; i++) if (px[(y * c.width + i) * 4 + 3] > 150) n++;
      return n; })()""")
    check('  and the Spectrum line is drawn again when the bands arrive', drawn > 200, drawn)
    b.page('__quiet()')


def leaving(b, d):
    load(b, d)
    b.page('__feed([191, 150, 200, 160])')
    time.sleep(2.5)
    b.page(MUTES)
    b.page('window.__vuRaf = 0')
    tap(b, DEVICE_HOP)
    poll(lambda: b.page("__core.computer.side() === 'dac'"))
    time.sleep(0.6)
    gone = b.page('__open()')
    time.sleep(0.8)
    check('leaving_the_computer_side_closes_the_streams_and_stops_the_needles: no stream, no needle loop',
          gone == [] and b.page('window.__vuRaf') == 0 and b.page(f"!document.querySelector('{AREA}')"),
          (gone, b.page('window.__vuRaf')))
    b.page('__quiet()')


def at_rest(b, d):
    load(b, d)
    time.sleep(0.6)
    b.page(MUTES)
    b.page('window.__vuRaf = 0')
    time.sleep(1.0)
    check('the_needle_loop_stops_in_silence: no frame asked for with nothing fed', b.page('window.__vuRaf') == 0,
          b.page('window.__vuRaf'))
    b.page('__feed([191, 150, 200, 160])')
    time.sleep(0.5)
    moved = b.page('window.__vuRaf')
    check('  and a levels frame starts it again', moved > 0, moved)
    time.sleep(2.5)
    b.page('(window.__writes = 0, window.__vuRaf = 0)')
    time.sleep(1.2)
    got = b.page('({ writes: window.__writes, raf: window.__vuRaf })')
    check('  a constant level, settled: zero needle writes and the loop stopped', got == {'writes': 0, 'raf': 0}, got)
    first = text(b, '#sg-vnum-L')
    check('  and the reading shows the settled level, not a stale one', abs(vu_of(first)) <= 0.2, first)
    click(b, '#sg-vref-up')
    time.sleep(0.4)
    check('  a reference change wakes it: the reading falls by 1 dB', abs(vu_of(text(b, '#sg-vnum-L')) + 1.0) <= 0.2,
          text(b, '#sg-vnum-L'))
    b.page('__quiet()')


def stored_ref(b, d):
    b.goto(f'{d.url}?theme=signal&effects=off', settle=0.5)
    b.page("localStorage.setItem('svolder.theme.signal.vu-ref', '-17.5')")
    load(b, d)
    got = text(b, '#sg-vref-out')
    check('a_stored_reference_that_is_not_a_whole_number_is_refused: the default applies', got == '0 VU = −18 dBFS', got)


def reset_focus(b, d):
    load(b, d)
    click(b, '#sg-vref-up')
    b.page("document.querySelector('#sg-vref-reset').focus()")
    click(b, '#sg-vref-reset')
    check('Reset_hands_the_focus_to_the_plus_button_when_it_hides',
          b.page("document.activeElement.id") == 'sg-vref-up', b.page("document.activeElement.id"))


def writes(b, d):
    load(b, d)
    b.page(MUTES)
    b.page('window.__raf = 0')
    # a level that moves every frame, so the needles never rest
    b.page("""(() => { let k = 0; window.__swing = setInterval(() => { k++; window.__feed([120 + (k % 100), 130 + ((k * 7) % 100), 200, 200]); }, 33); })()""")
    time.sleep(1.5)
    got = b.page('({ writes: window.__writes, raf: window.__raf })')
    b.page('(() => { clearInterval(window.__swing); __quiet(); })()')
    check('a_frame_writes_two_needle_transforms_at_most', got['raf'] > 10 and got['writes'] <= 2 * got['raf']
          and got['writes'] > 20, got)


def guest(b, d):
    load(b, d)
    click(b, '#sg-vface-L')
    check('a_guest_turns_the_glow_too', 'sg-w1' in b.page(f"document.querySelector('{AREA}').className"))


STILLS = os.environ.get('VU_STILLS', f'{SHOTS}/vu')


def pictures():
    """Stills for the user: each glow at 1280, 900 and 390 (the needles on a canned level, one lamp lit)."""
    os.makedirs(STILLS, exist_ok=True)
    d = common.Daemon('gungnir2-v2', host='fake', fleet=FLEET)
    try:
        for size in ((1280, 900), (900, 1000), (390, 1100)):
            b = make(size)
            try:
                load(b, d)
                b.page('__feed([191, 160, 200, 255])')
                for step in range(3):
                    time.sleep(1.6)
                    glow_now = b.page(f"document.querySelector('{AREA}').className.match(/sg-(w\\d)/)[1]")
                    shot = f'{STILLS}/vu-{size[0]}-{glow_now}.png'
                    with open(shot, 'wb') as f:
                        f.write(b.shot())
                    print('shot', shot)
                    click(b, '#sg-vface-L')
            finally:
                b.close()
    finally:
        d.stop()


run('opens', opens)
run('needles', needles)
run('lamp', lamp)
run('reference', reference)
run('glow', glow)
run('kept', kept)
run('refused', refused, storage=False)
run('spectrum', spectrum)
run('hidden', hidden)
run('capture', capture_state)
run('markup', markup)
run('layout', layout)
run('reduced', reduced_motion, reduced=True)
run('leaving', leaving)
run('rest', at_rest)
run('storedref', stored_ref)
run('reset', reset_focus)
run('writes', writes)
run('guest', guest, web='guest = "read"')
if browser == 'chromium':
    pictures()
print(f'signal_vu.py {browser}: {passed} passed, {len(fails)} failed in {time.time() - t0:.0f} s')
sys.exit(1 if fails else 0)
