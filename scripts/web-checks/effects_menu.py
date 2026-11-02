# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The frame's menu (Ember's effects rows in it), the levels, the theme picker and the key T, in one
browser. Usage: python3 effects_menu.py chromium|firefox
Runs against a fake DAC and a fake amp (and a lost-link DAC for the out-of-band swap) and a DAC with
a fake host (the Visualizer switch row: shown only where a screen has the card's band, a menu
item the arrows and the keys reach, never a level). Exits 1 on any failed check; prints one line
per check."""
import json
import sys
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
fails = []
passed = 0


def check(name, cond, detail=''):
    global passed
    if cond:
        passed += 1
        print('ok  ', name)
    else:
        fails.append(name)
        print('FAIL', name, detail)


def make(**kw):
    # Smoke cannot run in these pages (WebGL refused), as on a machine with no WebGL2: the menu's
    # rules with an unavailable Smoke. smoke_core.py covers the menu with Smoke available.
    if browser == 'chromium':
        return common.Chromium(block_cookies=kw.pop('block_storage', False),
                               args=['--disable-3d-apis'], **kw)
    return common.Firefox(extra_prefs=['user_pref("webgl.disabled", true);'], **kw)


def errors(b):
    return b.errors if browser == 'chromium' else b.collect_errors()


def state(b):
    return b.ev("""(() => {
      const fx = document.querySelector('.fx');
      const pop = document.querySelector('.fxpop');
      const at = document.activeElement;
      let stored = 'unreadable';
      try { stored = localStorage.getItem('svolder.effects'); } catch (e) {}
      return {
        level: fx && fx.dataset.level, open: pop && !pop.hidden,
        expanded: document.querySelector('.fxbtn')
          && document.querySelector('.fxbtn').getAttribute('aria-expanded'),
        focus: at && (at.id || (at.dataset && (at.dataset.level || at.dataset.switch))
          || at.tagName),
        checked: [...document.querySelectorAll('.fxpop [role=menuitemradio]')]
          .filter(i => i.getAttribute('aria-checked') === 'true').map(i => i.dataset.level),
        glowShown: fx && getComputedStyle(fx.querySelector('.glow')).display !== 'none',
        stored,
      };
    })()""")


# The ids (or levels or switches) of the rows the arrows reach in the open menu, in order.
REACHABLE = """[...document.querySelectorAll('.fxpop [role^=menuitem]')]
  .filter(i => i.getAttribute('aria-disabled') !== 'true' && !i.hidden)
  .map(i => i.id || i.dataset.level || i.dataset.switch || i.dataset.device)"""


# The page's core, once it is built (a module the page itself loaded: `page` reaches its realm).
CORE = "import(new URL('assets/core/boot.js', document.baseURI)).then((m) => m.core)"
TOASTS = "[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"
# The picker as the page shows it, or null.
PICKER = """(() => { const p = document.getElementById('picker');
  if (!p) return null;
  const at = document.activeElement;
  return { role: p.getAttribute('role'), modal: p.getAttribute('aria-modal'),
    title: p.querySelector('h2').textContent, note: p.querySelector('.picker-note').textContent,
    foot: p.querySelector('.picker-foot').textContent,
    cards: [...p.querySelectorAll('.tcard')].map((c) => [c.dataset.theme,
      c.getAttribute('aria-current'), c.querySelector('b').textContent,
      c.querySelector('small').textContent, !!c.querySelector('.tprev svg')]),
    menuOpen: !document.getElementById('fx-menu').hidden,
    focus: at && (at.dataset.theme || at.id), inert: document.getElementById('theme').inert }; })()"""


def rect(b, sel):
    return b.ev(f"(() => {{ const r = document.querySelector({json.dumps(sel)})"
                f".getBoundingClientRect();"
                f" return [r.left, r.top, r.right, r.bottom]; }})()")


def click_el(b, sel):
    x, y = common.center(b, sel)
    b.click(x, y)


def open_by_click(b):
    click_el(b, '.fxbtn')


dac = amp = gone = host = two = guest = None
try:
    dac = common.Daemon('gungnir2-v2')
    amp = common.Daemon('fake-amp', name='Amp', env={'SVOLDER_FAKE_SIGNAL': '1'})
    # ---- levels by click, stored, reload ----
    b = make()
    b.goto(dac.url)
    s = state(b)
    check(
        'starts on Glow (Smoke cannot run here (WebGL refused))',
        s['level'] == 'glow' and s['glowShown'],
        s,
    )
    check('nothing stored by a plain visit', s['stored'] is None, s)
    smoke_item = b.ev("(() => { const i = document.querySelector('[data-level=smoke]');"
                      " return [i.getAttribute('aria-disabled'), i.title]; })()")
    check('Smoke is disabled with its reason as title',
          smoke_item == ['true', 'This browser cannot draw the smoke (no WebGL2)'], smoke_item)
    check('the menu checks Glow', s['checked'] == ['glow'], s)
    open_by_click(b)
    s = state(b)
    check('click opens the menu, focus on the checked item',
          s['open'] and s['expanded'] == 'true' and s['focus'] == 'glow', s)
    click_el(b, '[data-level=off]')
    s = state(b)
    check('picking Off: level off, glow hidden, stored, closed, focus on the button',
          s['level'] == 'off' and not s['glowShown'] and s['stored'] == 'off' and not s['open']
          and s['expanded'] == 'false' and s['focus'] == 'fx-button' and s['checked'] == ['off'], s)
    b.goto(dac.url)
    s = state(b)
    check('the stored choice survives a reload', s['level'] == 'off' and s['checked'] == ['off'], s)
    open_by_click(b)
    click_el(b, '[data-level=glow]')
    s = state(b)
    check(
        'picking Glow again',
        s['level'] == 'glow' and s['glowShown'] and s['stored'] == 'glow',
        s,
    )
    # a disabled item cannot be picked
    open_by_click(b)
    click_el(b, '[data-level=smoke]')
    s = state(b)
    check('a disabled item cannot be picked; the menu stays open',
          s['level'] == 'glow' and s['open'] and s['stored'] == 'glow', s)
    # button again closes
    click_el(b, '.fxbtn')
    s = state(b)
    check(
        'the button again closes it, focus on the button',
        not s['open'] and s['focus'] == 'fx-button',
        s,
    )
    # outside click
    open_by_click(b)
    b.click(5, 400)
    s = state(b)
    check('a click outside closes it', not s['open'] and s['expanded'] == 'false', s)
    check('  and the focus is back on the button', s['focus'] == 'fx-button', s)

    # ---- keyboard ----
    b.goto(dac.url)
    reach = None
    for i in range(30):
        b.press('Tab')
        at = b.ev("document.activeElement && document.activeElement.id")
        if at == 'fx-button':
            reach = i + 1
            break
    check('Tab reaches the button', reach is not None, reach)
    b.press('Enter')
    s = state(b)
    check('Enter opens with focus on the checked item', s['open'] and s['focus'] == 'glow', s)
    reached = b.ev(REACHABLE)
    check('the rows the arrows reach with Smoke disabled: the Theme row, Glow, Off, Arrange (the '
          'Device row is disabled with one device, and there is no Sign out with auth none)',
          reached == ['menu-theme', 'glow', 'off', 'menu-arrange'], reached)
    order = []
    for k in ('ArrowDown', 'ArrowDown', 'ArrowDown', 'ArrowUp', 'ArrowRight', 'ArrowLeft', 'End',
              'Home'):
        b.press(k)
        order.append(state(b)['focus'])
    check('arrows wrap, skip the disabled Smoke and the Device row (one device); Home/End jump; '
          'the Arrange row is the last of them',
          order == ['off', 'menu-arrange', 'menu-theme', 'menu-arrange', 'menu-theme',
                    'menu-arrange', 'menu-arrange', 'menu-theme'], order)
    b.press('ArrowDown')
    b.press('ArrowDown')
    b.press('Enter')
    s = state(b)
    check('Enter picks Off and closes, focus on the button',
          s['level'] == 'off' and not s['open'] and s['focus'] == 'fx-button'
          and s['stored'] == 'off', s)
    b.press(' ')
    s = state(b)
    check('Space opens, focus on the checked item (Off)', s['open'] and s['focus'] == 'off', s)
    b.press('ArrowUp')
    b.press(' ')
    s = state(b)
    check(
        'Space picks (Glow)',
        s['level'] == 'glow' and not s['open'] and s['focus'] == 'fx-button',
        s,
    )
    b.press('Enter')
    b.press('Escape')
    s = state(b)
    check('Escape closes, focus back on the button', not s['open'] and s['focus'] == 'fx-button', s)
    b.press('Enter')
    b.press('q')
    s = state(b)
    check('q closes, focus back on the button', not s['open'] and s['focus'] == 'fx-button', s)
    b.press('q')
    s = state(b)
    check('q on the closed button opens nothing', not s['open'], s)
    b.press('Enter')
    b.press('Tab')
    s = state(b)
    check('Tab out of the menu closes it', not s['open'] and s['expanded'] == 'false', s)
    # a Tab that never moves the focus inside the page (it left the page, or came back) must not
    # leave the menu primed to close on the next focus move: a Tab keydown, then an arrow
    b.ev("document.querySelector('.fxbtn').focus()")
    b.press('Enter')
    b.ev("document.activeElement.dispatchEvent(new KeyboardEvent('keydown', "
         "{ key: 'Tab', bubbles: true, cancelable: true }))")
    b.press('ArrowDown')
    s = state(b)
    check('a Tab that leaves the page, then an arrow inside the menu: it stays open',
          s['open'] and s['focus'] == 'off', s)
    b.press('Escape')

    # ---- the page's own keys stay out of the menu ----
    b.goto(dac.url)
    b.ev("""(() => { window.__posts = []; const f = window.fetch;
      window.fetch = (...a) => { window.__posts.push(String(a[0])); return f(...a); };
      const x = XMLHttpRequest.prototype.open;
      XMLHttpRequest.prototype.open = function (m, u) { window.__posts.push(m + ' ' + u);
        return x.apply(this, arguments); };
    })()""")
    b.ev("document.querySelector('.fxbtn').focus()")
    b.press('Enter')
    for k in ('ArrowRight', 'ArrowLeft', 'm'):
        b.press(k)
    posts = [p for p in b.ev('window.__posts') if 'controls' in p or 'volume' in p]
    check('volume and mute keys in the menu send nothing', posts == [], posts)
    b.press('Escape')
    b.ev("document.activeElement.blur()")
    b.press('?')
    shown = b.ev("!document.getElementById('keys').hidden")
    check('? on the page opens the key list', shown is True, shown)
    b.ev("document.querySelector('.fxbtn').focus()")
    b.press('q')
    shown = b.ev("!document.getElementById('keys').hidden")
    check('a key on the closed button reaches the page: the ? list closes', shown is False, shown)
    b.press('?')
    shown = b.ev("!document.getElementById('keys').hidden")
    check('  and ? on the closed button toggles it open', shown is True, shown)
    b.press('Enter')
    b.press('Escape')
    shown = b.ev("!document.getElementById('keys').hidden")
    check('  nor does Escape in the open menu', shown is True, shown)
    errs = errors(b)
    check('no page error', errs == [], errs)
    b.close()

    # ---- the closed button leaves the page's keys alone (a writes-enabled page, the fake DAC) ----
    b = make()
    b.goto(dac.url)
    check('the page allows writes (the fake DAC, no auth)',
          b.ev("document.body.classList.contains('writes')") is True)
    b.ev("""(() => { window.__posts = []; const f = window.fetch;
      window.fetch = (...a) => { window.__posts.push(String(a[0])); return f(...a); };
    })()""")
    open_by_click(b)
    click_el(b, '[data-level=off]')
    s = state(b)
    check('a pick returns the focus to the closed button', s['focus'] == 'fx-button'
          and not s['open'], s)
    raw = b.ev("document.querySelector('sv-knob').dataset.raw")
    b.press('ArrowLeft')
    b.sleep(1.0)
    posts = [p for p in b.ev('window.__posts') if ('controls/' in p or '/volume' in p)]
    now = b.ev("document.querySelector('sv-knob').dataset.raw")
    check('<- on the closed button steps the volume', len(posts) >= 1, (raw, now, posts))
    before = len(posts)
    b.press('m')
    b.sleep(0.5)
    posts = [p for p in b.ev('window.__posts') if ('controls/' in p or '/volume' in p)]
    check('m on the closed button mutes', len(posts) > before, posts)
    b.press('ArrowDown')
    s = state(b)
    check('Down on the closed button still opens the menu', s['open'], s)
    posts_open = len([p for p in b.ev('window.__posts') if ('controls/' in p or '/volume' in p)])
    b.press('ArrowLeft')
    b.press('m')
    b.sleep(0.5)
    posts = [p for p in b.ev('window.__posts') if ('controls/' in p or '/volume' in p)]
    check("<- and m in the open menu stay the menu's", len(posts) == posts_open, posts)
    b.press('Escape')
    errs = errors(b)
    check("no page error from the closed button's keys", errs == [], errs)
    b.close()

    # ---- ?effects= ----
    b = make()
    b.goto(dac.url + '?effects=off')
    s = state(b)
    check('?effects=off shows Off and stores nothing',
          s['level'] == 'off' and s['stored'] is None and s['checked'] == ['off'], s)
    open_by_click(b)
    click_el(b, '[data-level=glow]')
    s = state(b)
    check('  the menu still works and stores', s['level'] == 'glow' and s['stored'] == 'glow', s)
    b.goto(dac.url + '?effects=banana')
    s = state(b)
    check('an unknown ?effects= is ignored (the kept Glow stands)', s['level'] == 'glow', s)
    b.ev("localStorage.setItem('svolder.effects', 'smoke')")
    b.goto(dac.url)
    s = state(b)
    check('a kept smoke that cannot run shows Glow and stays kept',
          s['level'] == 'glow' and s['stored'] == 'smoke' and s['checked'] == ['glow'], s)
    b.goto(dac.url + '?effects=smoke')
    s = state(b)
    check('?effects=smoke shows Glow here', s['level'] == 'glow', s)
    b.ev("localStorage.removeItem('svolder.effects')")
    b.close()

    # ---- reduced motion ----
    b = make(reduced=True)
    b.goto(dac.url)
    s = state(b)
    check('reduced motion: the default is Glow', s['level'] == 'glow' and s['glowShown'], s)
    lvl = b.ev("getComputedStyle(document.querySelector('.fx')).getPropertyValue('--lvl')")
    anim = b.ev("getComputedStyle(document.querySelector('.glow')).animationName")
    check(
        'reduced motion: the glow is not pinned or stilled (reduced_motion.py reads it closely)',
        anim == 'glow-turn',
        (lvl, anim),
    )
    b.close()

    # ---- storage refused ----
    b = make(block_storage=True)
    b.goto(dac.url)
    readable = b.ev("(() => "
                    "{ try { localStorage.getItem('x'); return true; } catch (e) {"
                    " return false; } })()")
    check('storage really is refused in this profile', readable is False, readable)
    s = state(b)
    check('storage refused: the page starts', s['level'] == 'glow', s)
    open_by_click(b)
    click_el(b, '[data-level=off]')
    s = state(b)
    check(
        'storage refused: the menu still switches for the page life',
        s['level'] == 'off' and s['checked'] == ['off'],
        s,
    )
    errs = errors(b)
    check('storage refused: no error', errs == [], errs)
    if browser == 'chromium':
        setup = [w for w in b.warnings if 'effects threw' in w]
        check('storage refused: the effects set up whole (no setup warning)', setup == [], setup)
    b.close()

    # ---- amp page and guest-like pages carry the menu ----
    b = make()
    b.goto(amp.url)
    s = state(b)
    check(
        'the amp page has the layer and the menu',
        s['level'] == 'glow' and s['checked'] == ['glow'],
        s,
    )
    b.close()

    # ---- the Visualizer switch row ----
    b = make()
    b.goto(dac.url)
    open_by_click(b)
    row = b.ev("(() => { const r = document.querySelector('.fxpop [data-switch=viz]');"
               " return [r.hidden, document.querySelector('.fxsep').hidden]; })()")
    check('a page with no Now playing band (no host) hides the Visualizer row and its rule',
          row == [True, True], row)
    b.close()
    host = common.Daemon('gungnir2-v2', host='fake')
    b = make()
    b.goto(host.url)
    b.sleep(0.8)
    open_by_click(b)
    row = b.ev("""(() => { const r = document.querySelector('.fxpop [data-switch=viz]');
      return { hidden: r.hidden, role: r.getAttribute('role'),
        checked: r.getAttribute('aria-checked'), text: r.textContent.trim(),
        tabindex: r.getAttribute('tabindex'),
        radios: document.querySelectorAll('.fxpop [role=menuitemradio]').length,
        sep: !document.querySelector('.fxsep').hidden }; })()""")
    check('a page with the band shows it: a checkbox row "Visualizer", on, under a rule, and '
          'the levels are still three radios',
          row == {'hidden': False, 'role': 'menuitemcheckbox', 'checked': 'true',
                  'text': 'Visualizer', 'tabindex': '-1', 'radios': 3, 'sep': True}, row)
    b.press('Escape')
    b.ev("document.querySelector('.fxbtn').focus()")
    b.press('Enter')
    order = []
    for k in ('ArrowDown', 'ArrowDown', 'ArrowDown', 'ArrowUp', 'End', 'Home'):
        b.press(k)
        order.append(state(b)['focus'])
    check('the arrows reach the switch after the levels, then the Arrange row, and wrap (Smoke is '
          'disabled here)',
          order == ['off', 'viz', 'menu-arrange', 'viz', 'menu-arrange', 'menu-theme'], order)
    b.ev("document.querySelector('.fxpop [data-switch=viz]').focus()")
    b.press(' ')
    s = state(b)
    after = b.ev("document.querySelector('.fxpop [data-switch=viz]').getAttribute('aria-checked')")
    check('Space on the switch turns it off: the menu closes, the focus is back on the button, '
          'the level and its stored choice are untouched',
          not s['open'] and s['focus'] == 'fx-button' and after == 'false'
          and s['level'] == 'glow' and s['stored'] is None, [s, after])
    b.press('Enter')
    s = state(b)
    check('  reopened, the focus is on the checked level, not on the switch',
          s['open'] and s['focus'] == 'glow', s)
    b.ev("document.querySelector('.fxpop [data-switch=viz]').focus()")
    b.press('Enter')
    after = b.ev("document.querySelector('.fxpop [data-switch=viz]').getAttribute('aria-checked')")
    check('  and Enter on the switch turns it on again', after == 'true', after)
    errs = errors(b)
    check('no page error from the switch row', errs == [], errs)
    b.close()

    # ---- the pill's out-of-band swap leaves the menu alone ----
    gone = common.Daemon('gungnir2-v2')
    b = make()
    b.goto(gone.url)
    word = b.ev("document.getElementById('part-link').textContent.trim()")
    check('the device is connected before it is dropped', word == 'connected', word)
    b.ev("(() => { document.querySelector('.fxbtn').__same = 1; "
         "document.querySelector('.fxpop').__same = 1; })()")
    open_by_click(b)
    before = rect(b, '.fxbtn')
    pill_before = rect(b, '#part-link')
    gap_before = pill_before[0] - before[2]
    gone.drop()
    changed = False
    for _ in range(40):
        b.sleep(0.25)
        now = b.ev("document.getElementById('part-link').textContent.trim()")
        if now != word:
            changed = True
            break
    b.sleep(0.5)
    s = state(b)
    after = rect(b, '.fxbtn')
    pill_after = rect(b, '#part-link')
    same = b.ev("[!!document.querySelector('.fxbtn').__same, "
                "!!document.querySelector('.fxpop').__same]")
    check('the pill changed (a link drop)', changed, word)
    check(
        'the button and the open menu are the same nodes after the swap',
        same == [True, True],
        same,
    )
    check('the menu is still open and consistent',
          s['open'] and s['expanded'] == 'true' and s['checked'] == ['glow'], s)
    check(
        'the button keeps its row (no vertical jump)',
        abs(after[1] - before[1]) < 1,
        (before, after),
    )
    gap_after = pill_after[0] - after[2]
    check(
        'the button stays beside the pill (same gap)',
        abs(gap_after - gap_before) < 1.5,
        (gap_before, gap_after),
    )
    popr = rect(b, '.fxpop')
    check('the popover is inside the viewport', popr[0] >= 0 and popr[2] <= 1280, popr)
    errs = errors(b)
    check('no page error through the swap', errs == [], errs)
    b.close()

    # ---- the frame: the picker, the key T, the fade, the fallback, the button's place ----
    b = make()
    b.goto(dac.url)
    check('the_button_is_on_top_and_not_covered',
          b.ev("""(() => { const b = document.getElementById('fx-button').getBoundingClientRect();
            const at = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
            return at === document.getElementById('fx-button')
              || document.getElementById('fx-button').contains(at); })()""") is True)
    own = b.ev("""(() => ({ n: document.querySelectorAll('.fxbtn').length,
      inside: document.getElementById('theme').contains(document.getElementById('fx-button')),
      before: !!(document.getElementById('theme').compareDocumentPosition(
        document.getElementById('frame')) & Node.DOCUMENT_POSITION_PRECEDING) }))()""")
    check('the frame draws one button, before the theme root in the page (the first Tab stop) and '
          'not inside it', own == {'n': 1, 'inside': False, 'before': True}, own)
    # a theme's own rules for the button, added after every sheet of the page
    b.ev("""(() => { const sheet = new CSSStyleSheet();
      sheet.replaceSync('#frame .fxbtn.fxbtn, #frame .fxmenu.fxmenu, #frame#frame {'
        + ' display: none !important; visibility: hidden !important;'
        + ' pointer-events: none !important; position: static !important;'
        + ' opacity: 0 !important; }');
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]; })()""")
    kept = b.ev("""(() => { const cs = getComputedStyle(document.getElementById('fx-button'));
      return [cs.display, cs.visibility, cs.pointerEvents, cs.position, cs.opacity]; })()""")
    check('the frame keeps the button displayed, visible, clickable and in place even against a '
          'later, more specific !important rule', kept[0] != 'none' and kept[1] == 'visible' and kept[2] != 'none'
          and kept[3] == 'relative' and kept[4] == '1', kept)
    b.goto(dac.url)
    b.page(CORE + ".then((core) => { core.frame.anchor(null); return true; })")
    b.sleep(0.3)
    r = rect(b, '.fxbtn')
    width = b.ev('document.documentElement.clientWidth')
    check('without an anchor the button sits top right with 16 px insets',
          abs(r[1] - 16) <= 1 and abs((width - r[2]) - 16) <= 1, [r, width])
    b.goto(dac.url)

    # the Theme row opens the picker with Ember marked
    open_by_click(b)
    click_el(b, '#menu-theme')
    p = b.ev(PICKER)
    check('the_theme_row_opens_the_picker_with_ember_marked',
          p and p['role'] == 'dialog' and p['modal'] == 'true' and p['title'] == 'Theme'
          and p['cards'] == [['ember', 'true', 'Ember', 'warm, smoke and glow', True],
                             ['paper', None, 'Paper', 'light, ink and notes', True]]
          and not p['menuOpen'] and p['focus'] == 'ember' and p['inert'] is True, p)
    check('  the sheet says the choice is per browser and how to get back',
          p and p['note'] == "Each browser keeps its own choice. Your sections' order stays with "
          "each theme." and p['foot'] == 'Press T in any theme to open this or add ?theme=ember '
          'to the address', p)
    b.press('Escape')
    p = b.ev(PICKER)
    s = state(b)
    check('  Escape closes it and the focus goes back to the button',
          p is None and s['focus'] == 'fx-button'
          and not b.ev("document.getElementById('theme').inert"), [p, s])
    b.press('T')
    check('  T opens it again', b.ev(PICKER) is not None)
    b.click(5, 5)
    check('  a click outside the sheet closes it', b.ev(PICKER) is None)
    b.press('T')
    click_el(b, '#picker .tcard')
    check('  a pick of the theme in use closes it and changes nothing',
          b.ev(PICKER) is None and b.ev("document.documentElement.dataset.theme") == 'ember')
    b.page("""(() => { window.__sent = []; const f = window.fetch;
      window.fetch = (...a) => { window.__sent.push(String(a[0])); return f(...a); };
      return true; })()""")
    b.press('T')
    b.press('m')
    b.press('ArrowLeft')
    b.sleep(0.6)
    sent = [u for u in b.page('window.__sent') if '/controls' in u or '/volume' in u]
    check('  the keys do not reach the page under it (an m would mute the DAC, an arrow step it)',
          sent == [] and b.ev(PICKER) is not None, sent)
    b.press('T')
    check('  and T closes it', b.ev(PICKER) is None)

    # T and the fade
    b.page(CORE + ".then((core) => core.frame.fade(true))")
    b.sleep(2.8)
    faded = b.ev("getComputedStyle(document.getElementById('fx-button')).opacity")
    b.press('T')
    p = b.ev(PICKER)
    shown = b.ev("getComputedStyle(document.getElementById('fx-button')).opacity")
    check('t_opens_the_picker_when_faded', faded == '0' and p is not None and shown == '1',
          [faded, p, shown])
    b.press('Escape')
    b.page(CORE + ".then((core) => core.frame.fade(false))")
    b.sleep(0.3)
    kept = b.ev("getComputedStyle(document.getElementById('fx-button')).opacity")
    check('  fade(false) keeps it shown', kept == '1', kept)
    b.page(CORE + ".then((core) => core.frame.fade(true))")
    b.sleep(1.2)
    b.click(5, 5)
    b.sleep(1.5)
    check('  a pointer press restarts the 2 s wait: 1.5 s after it the button is still shown',
          b.ev("getComputedStyle(document.getElementById('fx-button')).opacity") == '1')
    b.page(CORE + ".then((core) => core.frame.fade(false))")

    # T in the preset name field types a t
    b.ev("document.querySelector('.psave [data-open]') && "
         "document.querySelector('.psave [data-open]').click()")
    if b.ev("!!document.querySelector('.psave input[name=name]')"):
        b.ev("document.querySelector('.psave input[name=name]').focus()")
        b.press('t')
        typed = b.ev("document.querySelector('.psave input[name=name]').value")
        check('t_in_the_preset_name_field_types_a_t', typed == 't' and b.ev(PICKER) is None,
              [typed, b.ev(PICKER)])
        b.press('A')
        typed = b.ev("document.querySelector('.psave input[name=name]').value")
        check('a_in_the_preset_name_field_types_an_a: no Arrange mode',
              typed == 'tA' and not b.ev("!!document.querySelector('.arr-bar')"), typed)
    else:
        check('t_in_the_preset_name_field_types_a_t', False, 'no preset name field on the page')
    b.ev("document.activeElement.blur()")

    # a theme that throws falls back to Ember with a toast
    b.page(CORE + """.then((core) => { core.registry.add({ id: 'probe', label: 'Probe',
      mount() { throw new Error('x'); } }); return true; })""")
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=probe]')
    b.sleep(1.5)
    toasts = b.ev(TOASTS)
    check('a_theme_that_throws_falls_back_to_ember_with_a_toast',
          'Probe could not start here; showing Ember' in toasts
          and b.ev("document.documentElement.dataset.theme") == 'ember'
          and b.ev("document.querySelectorAll('.fxbtn').length") == 1
          and b.ev("!!document.querySelector('#screen sv-knob')") is True,
          [toasts, b.ev("document.documentElement.dataset.theme")])
    cookie = b.ev("document.cookie")
    check('  and the choice is Ember again', 'svolder_theme=ember' in cookie, cookie)

    # a second theme and back: no page error, one button, the menu follows the theme
    b.page(CORE + """.then((core) => { core.registry.add({ id: 'second', label: 'Second',
      blurb: 'one line of text', needs: [], schemes: ['dark'],
      mount(root) { root.innerHTML = '<p id="second-line">Second theme</p>';
        return () => root.replaceChildren(); },
      menuRows: () => [{ kind: 'heading', text: 'Second' }],
      preview(box) { box.innerHTML = '<svg viewBox="0 0 4 4"><rect width="4" height="4"/></svg>'; }
    }); return true; })""")
    b.press('T')
    cards = b.ev("[...document.querySelectorAll('#picker .tcard')].map(c => c.dataset.theme)")
    check('the picker has a card for each listed theme, the test ones too',
          cards == ['ember', 'paper', 'probe', 'second'], cards)
    click_el(b, '#picker .tcard[data-theme=second]')
    b.sleep(1.0)
    on = b.ev("""({ theme: document.documentElement.dataset.theme,
      line: !!document.getElementById('second-line'), screen: !!document.getElementById('screen'),
      buttons: document.querySelectorAll('.fxbtn').length,
      sheets: [...document.querySelectorAll('link[data-sheet]')].map(l => l.dataset.sheet),
      layers: document.querySelectorAll('.fx').length })""")
    check('a second theme is mounted in place of Ember: its line, one button, Ember\'s sheet out',
          on == {'theme': 'second', 'line': True, 'screen': False, 'buttons': 1, 'sheets': [],
                 'layers': 0}, on)
    open_by_click(b)
    rows = b.ev("""[...document.querySelectorAll('.fxpop > *')].map(
      e => e.id || e.textContent.trim())""")
    check('  its menu is the frame\'s rows and its own: the Theme row, its heading, no effects',
          rows[0] == 'menu-theme' and 'Second' in rows and not any('Smoke' in r for r in rows),
          rows)
    check('  the Theme row says which theme',
          b.ev("document.getElementById('menu-theme').textContent.trim()") == 'Theme: Second ›')
    b.press('Escape')
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    b.sleep(1.5)
    back = b.ev("""({ theme: document.documentElement.dataset.theme,
      line: !!document.getElementById('second-line'),
      knob: !!document.querySelector('#screen sv-knob'),
      buttons: document.querySelectorAll('.fxbtn').length,
      layers: document.querySelectorAll('.fx').length,
      sheets: [...document.querySelectorAll('link[data-sheet]')].map(l => l.dataset.sheet),
      styled: getComputedStyle(document.body).backgroundColor })""")
    check('and back to Ember: drawn again, one button, one layer, its sheet loaded again',
          back['theme'] == 'ember' and not back['line'] and back['knob'] and back['buttons'] == 1
          and back['layers'] == 1 and back['sheets'] == ['ember']
          and back['styled'] == 'rgb(15, 13, 11)', back)
    open_by_click(b)
    click_el(b, '[data-level=off]')
    s = state(b)
    check('  and its effects menu works again', s['level'] == 'off' and s['checked'] == ['off'], s)
    # a page whose first paint was another theme: Ember's sheet, meta and scripts are not in it
    # (the second theme being on show, its own are all that is left); picking Ember loads them
    def strip_ember_head():
        b.page("""(() => { for (const el of document.querySelectorAll(
            'script[src*="themes/ember/htmx"], script[src*="themes/ember/viz-pref"]')) el.remove();
          for (const el of document.querySelectorAll('link[data-sheet=ember], meta[data-sheet=ember]'))
            el.remove();
          delete window.htmx;
          return true; })()""")

    EMBER_HEAD = """({ theme: document.documentElement.dataset.theme,
      body: getComputedStyle(document.body).backgroundColor,
      htmx: typeof window.htmx, meta: !!document.querySelector('meta[name=htmx-config]'),
      scripts: document.querySelectorAll('script[src*="themes/ember/htmx"]').length,
      knob: !!document.querySelector('#screen sv-knob'), buttons: document.querySelectorAll('.fxbtn').length })"""
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=second]')
    b.sleep(1.0)
    strip_ember_head()
    # the focus is on a node of the theme on show, which the switch takes out of the page
    b.ev("(() => { const p = document.getElementById('second-line'); p.tabIndex = 0; p.focus(); })()")
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    b.sleep(2.0)
    head = b.page(EMBER_HEAD)
    check('a pick of Ember from a page without its sheet, meta and htmx loads them: styled, '
          'htmx there once, one button',
          head == {'theme': 'ember', 'body': 'rgb(15, 13, 11)', 'htmx': 'object', 'meta': True,
                   'scripts': 1, 'knob': True, 'buttons': 1}, head)
    b.page("(() => { window.__raw = document.querySelector('sv-knob').raw; return true; })()")
    b.press('ArrowLeft')
    b.sleep(1.2)
    moved = b.page("({ was: window.__raw, now: document.querySelector('sv-knob').raw })")
    check('  and a write\'s answer swaps its part: the knob moves a step', moved['was'] != moved['now'],
          moved)
    check('  the focus is on the menu button after the switch',
          b.ev("document.activeElement && document.activeElement.id") == 'fx-button')
    # and the fallback of a theme that throws loads them too
    b.page(CORE + """.then((core) => { core.registry.add({ id: 'probe2', label: 'Probe two',
      mount() { throw new Error('x'); } }); return true; })""")
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=second]')
    b.sleep(1.0)
    strip_ember_head()
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=probe2]')
    b.sleep(2.0)
    head = b.page(EMBER_HEAD)
    toasts = b.ev(TOASTS)
    check('a theme that throws, picked from a page without Ember\'s files, falls back to a styled '
          'Ember with htmx and a toast',
          head == {'theme': 'ember', 'body': 'rgb(15, 13, 11)', 'htmx': 'object', 'meta': True,
                   'scripts': 1, 'knob': True, 'buttons': 1}
          and 'Probe two could not start here; showing Ember' in toasts, [head, toasts])
    b.page("(() => { window.__raw = document.querySelector('sv-knob').raw; return true; })()")
    b.press('ArrowRight')
    b.sleep(1.2)
    moved = b.page("({ was: window.__raw, now: document.querySelector('sv-knob').raw })")
    check('  and its knob still moves', moved['was'] != moved['now'], moved)

    # a mute that is still being written when the theme unmounts reaches nothing of the old mount
    # (the closed-button check above left the DAC muted: a climb is only stopped by a mute that
    # turns it on)
    dac.write('Gungnir 2', 'mute', False)
    b.sleep(1.0)
    b.page("""(() => { window.__fetch = window.fetch;
      window.fetch = (u, o) => String(u).includes('/controls')
        ? new Promise((r) => setTimeout(r, 1800)).then(() => window.__fetch(u, o))
        : window.__fetch(u, o);
      const knob = document.querySelector('sv-knob');
      knob.target = knob.raw + 10;
      knob.mute();
      return true; })()""")
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=second]')
    b.sleep(2.6)
    check('a mute awaiting its write when the theme unmounts leaves no error and no old mount',
          b.ev("document.documentElement.dataset.theme") == 'second'
          and b.ev("document.querySelectorAll('.fx').length") == 0)
    b.press('T')
    click_el(b, '#picker .tcard[data-theme=ember]')
    b.sleep(1.5)
    check('  and Ember mounts again', b.ev("!!document.querySelector('#screen sv-knob')") is True)
    errs = errors(b)
    check('no page error through the theme switches', errs == [], errs)
    b.close()

    # the Device row lists the devices when there are two
    two = common.Daemon(more=[('Second', '1112131415161718')])
    b = make()
    b.goto(two.url)
    open_by_click(b)
    row = b.ev("""(() => { const r = document.getElementById('menu-device');
      return [r.textContent.trim(), r.getAttribute('aria-disabled')]; })()""")
    check('with two devices the Device row names the shown one and is enabled',
          row == ['Device: Gungnir 2 ›', None], row)
    click_el(b, '#menu-device')
    radios = b.ev("""[...document.querySelectorAll('.fxpop [data-device]')].map(
      r => [r.dataset.device, r.getAttribute('aria-checked'), r.getAttribute('role')])""")
    check('  it opens in place into one radio per device, the shown one checked',
          [r[1:] for r in radios] == [['true', 'menuitemradio'], ['false', 'menuitemradio']]
          and len(radios) == 2, radios)
    check('  the menu stays open on the list', state(b)['open'] is True)
    click_el(b, '.fxpop [data-device]:not([aria-checked=true])')
    b.sleep(1.2)
    now = b.ev("""({ shown: document.querySelector('.devs .dev[aria-current=page] b').textContent,
      open: !document.getElementById('fx-menu').hidden })""")
    check('  choosing the other shows it and closes the menu',
          now == {'shown': 'Second', 'open': False}, now)
    errs = errors(b)
    check('no page error from the Device row', errs == [], errs)
    b.close()

    # a guest's menu offers Sign in
    guest = common.Daemon(web='guest = "read"')
    b = make()
    b.goto(guest.url)
    open_by_click(b)
    link = b.ev("""(() => { const a = document.getElementById('menu-sign-in');
      return a && [a.getAttribute('href'), a.getAttribute('role'), a.textContent.trim(),
        !!document.getElementById('menu-sign-out')]; })()""")
    check('a guest\'s menu has a Sign in link to ./?sign-in and no Sign out',
          link == ['./?sign-in', 'menuitem', 'Sign in', False], link)
    errs = errors(b)
    check('no page error on the guest page', errs == [], errs)
    b.close()
finally:
    for d in (dac, amp, gone, host, two, guest):
        if d:
            d.stop()
print(f'effects_menu.py {browser}: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
