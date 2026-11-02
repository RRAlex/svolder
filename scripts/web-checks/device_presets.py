# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""A Loki Max's presets part: it draws the three slots and the five built-in settings where a DAC
page has its presets; a built-in's click posts its name and its lamp lights when the stream says
the bands match (and when another client applies one); a slot's click posts its number; the save
icon opens its question in place, Keep closes it with no request made, Save posts the slot with
`confirm: true`, and what was saved comes back on a recall; no console error. Numbers from the
page, no pictures. Usage: python3 device_presets.py chromium|firefox"""
import json
import sys
import urllib.parse
import urllib.request

import common

browser = sys.argv[1] if len(sys.argv) > 1 else 'chromium'
NAME = 'Loki Max'
SLOTS = ['Preset 1', 'Preset 2', 'Preset 3']
BUILTINS = ['Flat', 'Bass Boost', 'Treble Boost', 'Smiley', 'Loudness']
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


# every request the page makes through fetch: the core's writes and its reads of the model
HOOK = """(() => { window.__posts = []; window.__gets = [];
  const fetched = window.fetch;
  window.fetch = function (url, init) {
    const method = (init && init.method) || 'GET';
    if (method === 'GET') window.__gets.push(String(url));
    else window.__posts.push({ url: String(url), body: init && init.body ? String(init.body) : null });
    return fetched.apply(this, arguments); };
  return 1; })()"""
PART = "document.getElementById('part-device-presets')"
SHAPE = f"""(() => {{ const part = {PART};
  const names = (sel) => [...part.querySelectorAll(sel)].map((e) => e.dataset.name);
  return {{ heading: part.querySelector('h3').textContent,
    sub: part.querySelector('h4').textContent,
    slots: names('.prow.slot .pre'), builtins: names('.prow:not(.slot) .pre'),
    saves: part.querySelectorAll('.ico[data-open=save]').length,
    forms: part.querySelectorAll('form[data-form=save]').length,
    tone: !!document.getElementById('part-tone'),
    dacPresets: !!document.getElementById('part-presets') }}; }})()"""
LIT = f"""[...{PART}.querySelectorAll('.prow:not(.slot) .pre[aria-pressed=true]')]
  .map((e) => e.dataset.name)"""
POSTS = "window.__posts.map((p) => [p.url, p.body])"
TOASTS = "[...document.querySelectorAll('#toasts .toast')].map((t) => t.textContent)"


def form_open(slot):
    return (f"(() => {{ const f = [...{PART}.querySelectorAll('form[data-form=save]')]"
            f".find((f) => f.querySelector('p').textContent.includes('{slot}?'));"
            f" return !!f && !f.hidden; }})()")


def api(d, path, body):
    """A write made by another client, through the web API."""
    req = urllib.request.Request(
        f'{d.url}api/v1/devices/{urllib.parse.quote(NAME)}/device-presets/{path}',
        data=json.dumps(body).encode(), method='POST',
        headers={'Content-Type': 'application/json', 'Origin': d.url.rstrip('/')})
    return json.load(urllib.request.urlopen(req, timeout=15))


def until(b, expression, secs=6):
    """Whether `expression` turns truthy within `secs` seconds."""
    for _ in range(int(secs * 10)):
        if b.ev(expression):
            return True
        b.sleep(0.1)
    return bool(b.ev(expression))


def press(b, selector):
    """A click on the element, scrolled to the middle of the view first."""
    b.ev(f"document.querySelector({json.dumps(selector)}).scrollIntoView({{ block: 'center' }})")
    b.sleep(0.2)
    x, y = common.center(b, selector)
    b.click(x, y)


d = common.Daemon(profile='lokimax', name=NAME)
b = (common.Chromium if browser == 'chromium' else common.Firefox)(size=(1280, 900))
try:
    b.goto(d.url)
    b.page(HOOK)
    check('the page has the presets part', b.ev(f'!!{PART}'))
    shape = b.ev(SHAPE)
    check('the part draws a Presets heading, three slots, a Built in subheading and five '
          'built-in settings',
          shape['heading'] == 'Presets' and shape['sub'] == 'Built in'
          and shape['slots'] == SLOTS and shape['builtins'] == BUILTINS, shape)
    check('a writer has a save icon and a question for each slot',
          shape['saves'] == 3 and shape['forms'] == 3, shape)
    check('there is no tone part and no DAC presets part on a Loki Max',
          not shape['tone'] and not shape['dacPresets'], shape)
    check('the bands start flat: the Flat lamp is the one lit',
          until(b, f'JSON.stringify({LIT}) === \'["Flat"]\''), b.ev(LIT))

    # a built-in
    press(b, f'#part-device-presets .pre[data-name="Bass Boost"]')
    check('a built-in click posts its name to apply',
          until(b, f'{POSTS}.length === 1'), b.ev(POSTS))
    posts = b.ev(POSTS)
    check('  to the device-presets/apply route, with the name only',
          posts and posts[0][0].endswith('device-presets/apply')
          and json.loads(posts[0][1]) == {'name': 'Bass Boost'}, posts)
    check('  its lamp lights and Flat goes out',
          until(b, f'JSON.stringify({LIT}) === \'["Bass Boost"]\''), b.ev(LIT))
    check('  a toast says what was sent',
          until(b, f"{TOASTS}.includes('Bass Boost on')"), b.ev(TOASTS))

    # another client's built-in: the stream lights the lamp, no click here
    before = len(b.ev(POSTS))
    api(d, 'apply', {'name': 'Treble Boost'})
    check('a built-in applied by another client lights its lamp through the stream',
          until(b, f'JSON.stringify({LIT}) === \'["Treble Boost"]\''), b.ev(LIT))
    check('  and this page made no request for it', len(b.ev(POSTS)) == before, b.ev(POSTS))

    # a slot's recall
    press(b, '#part-device-presets .pre[data-name="Preset 2"]')
    check('a slot click posts its number to recall',
          until(b, f'{POSTS}.length === {before + 1}'), b.ev(POSTS))
    posts = b.ev(POSTS)
    check('  to the device-presets/recall route',
          posts[-1][0].endswith('device-presets/recall')
          and json.loads(posts[-1][1]) == {'slot': 2}, posts)
    check('  a toast says what was sent',
          until(b, f"{TOASTS}.includes('Recalled Preset 2')"), b.ev(TOASTS))
    check('  the slot was never saved, so the device recalls Flat and its lamp lights',
          until(b, f'JSON.stringify({LIT}) === \'["Flat"]\''), b.ev(LIT))
    check('  no slot is ever lit',
          b.ev(f"{PART}.querySelectorAll('.prow.slot .pre[aria-pressed]').length") == 0)

    # save: the question, Keep, then Save
    api(d, 'apply', {'name': 'Smiley'})
    check('the bands are Smiley', until(b, f'JSON.stringify({LIT}) === \'["Smiley"]\''),
          b.ev(LIT))
    posts_now = len(b.ev(POSTS))
    save_icon = '#part-device-presets .ico[aria-label="Save the bands to Preset 2"]'
    press(b, save_icon)
    check('the save icon opens the question in place', until(b, form_open('Preset 2')))
    text = b.ev(f"{PART}.querySelector('form[data-form=save]:not([hidden]) p').textContent")
    check('  it names the slot and says what it replaces',
          text == 'Save the bands to Preset 2? What the device holds there is replaced.', text)
    press(b, '#part-device-presets form[data-form=save]:not([hidden]) [data-close]')
    check('Keep closes it', until(b, f'!{form_open("Preset 2")}'))
    b.sleep(0.5)
    check('  and no request was made', len(b.ev(POSTS)) == posts_now, b.ev(POSTS))
    press(b, save_icon)
    check('the question opens again', until(b, form_open('Preset 2')))
    api(d, 'apply', {'name': 'Smiley'})
    b.sleep(0.8)
    check('  and stays open while the stream has parts to send', b.ev(form_open('Preset 2')))
    press(b, '#part-device-presets form[data-form=save]:not([hidden]) button.danger')
    check('Save posts the slot to the save route',
          until(b, f'{POSTS}.length === {posts_now + 1}'), b.ev(POSTS))
    posts = b.ev(POSTS)
    check('  with confirm: true',
          posts[-1][0].endswith('device-presets/save')
          and json.loads(posts[-1][1]) == {'slot': 2, 'confirm': True}, posts)
    check('  a toast says what was saved',
          until(b, f"{TOASTS}.includes('Saved to Preset 2')"), b.ev(TOASTS))
    check('  and the question is closed again',
          until(b, f"{PART}.querySelectorAll('form[data-form=save]:not([hidden])').length === 0"))

    # what was saved comes back
    api(d, 'apply', {'name': 'Flat'})
    check('the bands go to Flat', until(b, f'JSON.stringify({LIT}) === \'["Flat"]\''), b.ev(LIT))
    press(b, '#part-device-presets .pre[data-name="Preset 2"]')
    check('recalling the slot that was saved brings Smiley back',
          until(b, f'JSON.stringify({LIT}) === \'["Smiley"]\''), b.ev(LIT))

    errors = b.errors if browser == 'chromium' else b.collect_errors()
    check('no page error', errors == [], errors)

    # a refused apply: the page says why and reads the part again, so no lamp is left stale
    b.ev(f"""{PART}.querySelector('.pre[data-name="Smiley"]').dataset.json = '{{"name":"Nope"}}'""")
    gets = len(b.ev('window.__gets'))
    posts_now = len(b.ev(POSTS))
    press(b, '#part-device-presets .pre[data-name="Smiley"]')
    check('a refused apply is a 409 the page says so for',
          until(b, f"{TOASTS}.some((t) => t.includes('Nope'))"), b.ev(TOASTS))
    check('  and the device is read again',
          until(b, f"window.__gets.slice({gets}).some((u) => u.endsWith('/model'))"),
          b.ev('window.__gets'))
    check('  after one request that was refused', len(b.ev(POSTS)) == posts_now + 1, b.ev(POSTS))
    errors = b.errors if browser == 'chromium' else b.collect_errors()
    check('  and the refusal is the only thing the console reports',
          all('409' in e for e in errors), errors)

finally:
    b.close()
    d.stop()
print(f'device_presets.py: {passed} passed, {len(fails)} failed')
sys.exit(1 if fails else 0)
