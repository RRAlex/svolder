# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mutation checks for the effects menu, levels and layer: each mutation edits a copy of the page's
web dir (never the repo), runs its check against the copy and must make at least one check FAIL
(a FAIL line in the output; a crash without one is an error, not a kill). One worker: the checks'
timing-based bounds flake under two.
Usage: python3 mutate_effects.py [name-part ...]   (default: all; needs a built svolderd for the
browser checks)"""
import os
import shutil
import subprocess
import sys

import common

HERE = os.path.dirname(os.path.abspath(__file__))
S = HERE + '/'
# the node harness imports the page's modules by a path relative to itself, so it runs from a
# copy laid out like the repo; the browser checks serve the copy through SMOKE_WEB
NODE = ['node', 'scripts/web-checks/node/effects.mjs']
MENU = ['python3', S + 'effects_menu.py', 'chromium']
LAYOUT = ['python3', S + 'effects_layout.py']
DEVS = ['python3', S + 'effects_devices.py']
ORDER = ['node', 'scripts/web-checks/node/ember-order.mjs']
ARRANGE = ['python3', S + 'arrange.py', 'chromium']
CONTRACT_NODE = ['node', 'scripts/web-checks/node/contract.mjs']
CONTRACT = ['python3', S + 'contract.py', 'chromium']
COMPLETE = ['python3', S + 'completeness.py', 'chromium']
ROWS = ['python3', S + 'core_rows.py', 'chromium']

M = [
    (
        'pick: kept before asked',
        'themes/ember/effects.js',
        'const chosen = asked ?? kept ??',
        'const chosen = kept ?? asked ??',
        NODE
    ),
    (
        'pick: smoke not downgraded',
        'themes/ember/effects.js',
        "chosen === 'smoke' && !can.ok ? 'glow'",
        "false ? 'glow'",
        NODE
    ),
    (
        'keep: does not write',
        'themes/ember/effects.js',
        '    storage.setItem(KEY, level);\n    return true;',
        '    return true;',
        NODE
    ),
    (
        'stored: accepts anything',
        'themes/ember/effects.js',
        'return LEVELS.includes(value) ? value : null;',
        'return value ?? null;',
        NODE
    ),
    (
        'menu: arrows land on a disabled item',
        'core/menu.js',
        'const list = choices();',
        "const list = [...pop.querySelectorAll('[role^=menuitem]')];",
        MENU
    ),
    (
        'menu: no focusout close',
        'core/menu.js',
        "document.addEventListener('focusout'",
        "document.addEventListener('xfocusout'",
        MENU
    ),
    (
        'menu: Escape does nothing',
        'core/menu.js',
        "(key === 'Escape' || (key.toLowerCase() === 'q'",
        "(false || (key.toLowerCase() === 'q'",
        MENU
    ),
    (
        'menu: focus not returned',
        'core/menu.js',
        'pop.contains(at))) button.focus();',
        'pop.contains(at))) {}',
        MENU
    ),
    (
        'menu: outside click ignored',
        'core/menu.js',
        '} else if (!wrap.contains(target)) {',
        '} else if (false) {',
        MENU
    ),
    ('menu: choose does not store', 'themes/ember/effects.js', '        keep(storage, id);\n', '', MENU),
    (
        'menu: Smoke never disabled',
        'themes/ember/effects.js',
        'disabled: () => unavailable.get(id) ?? null,',
        'disabled: () => null,',
        MENU
    ),
    (
        'page keys read inside the menu',
        'themes/ember/keys.js',
        'if (typingIn(event) || inEffectsMenu(event)) return;',
        'if (typingIn(event)) return;',
        MENU
    ),
    (
        'closed button swallows the page keys',
        'core/menu.js',
        "  return ['Enter', ' ', 'ArrowDown', 'ArrowUp'].includes(event.key);",
        '  return true;',
        MENU
    ),
    (
        'open menu lets the page keys through',
        'core/menu.js',
        "return !target.closest('.fxpop').hidden;",
        'return false;',
        MENU
    ),
    (
        'menu click closes the ? list',
        'themes/ember/keys.js',
        "if (kind === 'click' && event.detail === 0 && inMenu(event.target)) return;",
        '',
        MENU
    ),
    (
        'menu key closes the ? list',
        'themes/ember/keys.js',
        "    if (inEffectsMenu(event)) return;\n    if (!['Shift',",
        "    if (!['Shift',",
        MENU
    ),
    (
        'menu: the Theme row does not open the picker',
        'core/menu.js',
        '      run: openPicker,\n',
        '      run() {},\n',
        MENU
    ),
    (
        'picker: Escape does nothing',
        'core/picker.js',
        "    if (event.key === 'Escape') {",
        '    if (false) {',
        MENU
    ),
    (
        'picker: the keys reach the page',
        'core/picker.js',
        '    event.stopPropagation();\n  }\n\n  return { open',
        '  }\n\n  return { open',
        MENU
    ),
    (
        'frame: T types into a field',
        'core/frame.js',
        '  return !(target?.closest?.(TYPING) ?? target?.matches?.(TYPING));',
        '  return true;',
        MENU
    ),
    (
        'frame: a faded button stays faded',
        'core/frame.js',
        "    button.classList.remove('faded');\n",
        '',
        MENU
    ),
    (
        'frame: the guard is not layered',
        'core/frame.css',
        '@layer frame-guard {\n  #frame {',
        '@media all {\n  #frame {',
        MENU
    ),
    (
        'frame: the button is not guarded',
        'core/frame.css',
        '    visibility: visible !important; pointer-events: auto !important; opacity: 1 !important; }',
        '    visibility: visible !important; pointer-events: auto !important; }',
        MENU
    ),
    (
        'frame: the layer is after the theme in the page',
        'core/frame.js',
        '  document.body.prepend(root);',
        '  document.body.append(root);',
        MENU
    ),
    (
        'switch: the scripts of the theme are not loaded',
        'core/switch.js',
        'for (const src of entry.scripts ?? []) {',
        'for (const src of []) {',
        MENU
    ),
    (
        'switch: the metas of the theme are not added',
        'core/switch.js',
        'for (const { name, content } of entry.metas ?? []) {',
        'for (const { name, content } of []) {',
        MENU
    ),
    (
        'switch: the fallback is not prepared',
        'core/switch.js',
        'toast: core.frame.toast, prepare: load,',
        'toast: core.frame.toast,',
        MENU
    ),
    (
        'switch: the focus is left where it was',
        'core/switch.js',
        "        document.getElementById('fx-button')?.focus();\n",
        '',
        MENU
    ),
    (
        'urlparam ignored',
        'themes/ember/effects.js',
        'asked: LEVELS.includes(asked) ? asked : null,',
        'asked: null,',
        MENU
    ),
    (
        'storage throw not caught',
        'themes/ember/effects.js',
        '  try {\n    storage = window.localStorage;\n  } catch {\n    storage = null;\n  }',
        '  storage = window.localStorage;',
        MENU
    ),
    (
        'Off does not hide',
        'themes/ember/style.css',
        '.fx[data-level=smoke] .glow, .fx[data-level=off] .glow { display: none; }',
        '',
        MENU
    ),
    (
        'away not applied',
        'themes/ember/effects.js',
        "  layer?.classList.toggle('away', isAway);\n  handle?.away",
        '  handle?.away',
        DEVS
    ),
    ('fallback place lost', 'themes/ember/effects.js', 'box.top + 170', 'box.top + 50', DEVS),
    (
        'column width not carried',
        'themes/ember/effects.js',
        "  layer.style.setProperty('--fx-col', `${view.clientWidth}px`);\n",
        '',
        LAYOUT
    ),
    (
        'layout not redone on scroll',
        'themes/ember/effects.js',
        "window.addEventListener('scroll', later, { passive: true, signal });",
        '',
        LAYOUT
    ),
    (
        'layer clipped to the column again',
        'themes/ember/style.css',
        ('  position: fixed; top: 0; left: 0; width: 100%; height: 100vh; height: 100lvh; '
         'overflow: hidden;'),
        ('  position: absolute; top: 0; left: 400px; width: 600px; height: 100vh; overflow: '
         'hidden;'),
        LAYOUT
    ),
    (
        'arrange: an order kept for one layout is every layout\'s',
        'themes/ember/order.js',
        ("if (!stored || typeof stored !== 'object' || !Object.hasOwn(stored, layout)) return null;\n"
         '  const own = stored[layout];'),
        "if (!stored || typeof stored !== 'object') return null;\n  const own = stored.dac;",
        ORDER
    ),
    (
        'arrange: the keys that write reach a page being arranged',
        'themes/ember/keys.js',
        '    if (arranging()) return;\n',
        '',
        ARRANGE
    ),
    (
        'arrange: the sections\' children are not inert',
        'themes/ember/arrange.js',
        "        child.setAttribute('inert', '');\n",
        '',
        ARRANGE
    ),
    (
        'arrange: Escape keeps the order',
        'themes/ember/arrange.js',
        "if (how === 'done') {",
        "if (how !== 'away') {",
        ARRANGE
    ),
    (
        'arrange: an unmount keeps the order',
        'themes/ember/arrange.js',
        "const leave = () => end('away');",
        "const leave = () => end('done');",
        ARRANGE
    ),
    (
        'arrange: a swapped section is not given its handle again',
        'themes/ember/arrange.js',
        '    if (ended) return;\n    decorate();',
        '    if (ended) return;',
        ARRANGE
    ),
    (
        'arrange: a swapped handle loses the focus',
        'themes/ember/arrange.js',
        '      handleOf(focused.id)?.focus({ preventScroll: true });\n',
        '',
        ARRANGE
    ),
    (
        'arrange: Reset does not move the sections',
        'themes/ember/arrange.js',
        '      liveOrder.order = order;\n      lay(view, order, true);\n      placeEffects();\n      live.text',
        '      liveOrder.order = order;\n      placeEffects();\n      live.text',
        ARRANGE
    ),
    (
        'arrange: the default order is kept as an order',
        'themes/ember/arrange.js',
        'if (isDefault(order, layout)) delete all[layout];\n      else all[layout] = order;',
        'all[layout] = order;',
        ARRANGE
    ),
    (
        'arrange: one order for every layout',
        'themes/ember/arrange.js',
        'else all[layout] = order;',
        'else all.dac = order;',
        ARRANGE
    ),
    (
        'arrange: a drag lands nowhere',
        'themes/ember/arrange.js',
        'if (keep && place) apply(',
        'if (false && place) apply(',
        ARRANGE
    ),
    (
        'arrange: the visualizer is placed on an arranged screen',
        'themes/ember/visualizer.js',
        "if (!band || view.classList.contains('arranged')) return;",
        'if (!band) return;',
        ARRANGE
    ),
    (
        'arrange: the phone order rules reach an arranged screen',
        'themes/ember/style.css',
        '  .view:not(.arranged) .p-viz { order: 1; }',
        '  .view .p-viz { order: 1; }',
        ARRANGE
    ),
    (
        'arrange: the glow does not follow a moved dial',
        'themes/ember/arrange.js',
        '    lay(view, order, true);\n    placeEffects();\n    if (id) {',
        '    lay(view, order, true);\n    if (id) {',
        ARRANGE
    ),
    (
        'arrange: a drag ends only over the screen',
        'themes/ember/arrange.js',
        "  window.addEventListener('pointerup', (event) => {",
        "  view.addEventListener('pointerup', (event) => {",
        ARRANGE
    ),
    (
        'arrange: the visualizer switch seats the card by the kept order',
        'themes/ember/bind.js',
        'export const orderOf = (device) => live.order\n  ?? orderFor(',
        'export const orderOf = (device) => orderFor(',
        ARRANGE
    ),
    (
        'arrange: a locked device is offered for arranging',
        'themes/ember/arrange.js',
        'view !== null && !locked(view)',
        'view !== null',
        ARRANGE
    ),
    (
        'arrange: the mode starts on a locked device',
        'themes/ember/arrange.js',
        'if (!view || locked(view) || !mount',
        'if (!view || !mount',
        ARRANGE
    ),
    (
        'arrange: a narrow Now playing cell loses its previous button',
        'themes/ember/style.css',
        '    .p-playing .np-trans { gap: 4px; }\n',
        '    .p-playing .np-trans { gap: 4px; }\n    .p-playing .np-btn.prev { display: none; }\n',
        ARRANGE
    ),
    (
        'arrange: the menu row is offered while arranging',
        'themes/ember/ember.js',
        '    available: canArrange,',
        '    available: () => true,',
        ARRANGE
    ),
    (
        'contract: a band lacks its Q',
        'core/contract.js',
        "const PARAMS = ['f', 'g', 'q'];",
        "const PARAMS = ['f', 'g'];",
        CONTRACT_NODE
    ),
    (
        'contract: the Signal fact is not listed',
        'core/contract.js',
        "labels.push('Firmware', 'DSP', 'Profile', 'Link', 'Signal');",
        "labels.push('Firmware', 'DSP', 'Profile', 'Link');",
        CONTRACT_NODE
    ),
    (
        'contract: the unit is not listed',
        'core/contract.js',
        "if (facts.unit) labels.push('Unit');",
        '',
        CONTRACT_NODE
    ),
    (
        'contract: the transport button is the other one',
        'core/contract.js',
        "playing ? 'pause' : 'play'",
        "playing ? 'play' : 'pause'",
        CONTRACT_NODE
    ),
    (
        'contract: the slots are not listed',
        'core/contract.js',
        'device.device_presets?.slots ?? []',
        '[]',
        CONTRACT_NODE
    ),
    (
        'contract: the power readouts are not listed',
        'core/contract.js',
        '[...(power.toggles ?? []), ...(power.readouts ?? [])]',
        '[...(power.toggles ?? [])]',
        CONTRACT_NODE
    ),
    (
        'contract: a transport button without a track',
        'core/contract.js',
        "if (device.playing?.buttons && track) {\n    const playing = track.status === 'playing';",
        "if (device.playing?.buttons) {\n    const playing = track?.status === 'playing';",
        CONTRACT_NODE
    ),
    (
        'contract: the core loses a member',
        'core/core.js',
        '    clock,\n    frame: null,',
        '    frame: null,',
        CONTRACT
    ),
    (
        'driver: a step down raises',
        'themes/ember/driver.js',
        'data-step="${change.step < 0 ? -1 : 1}"',
        'data-step="1"',
        CONTRACT
    ),
    (
        'driver: a band step down is a step up',
        'themes/ember/driver.js',
        "[change.step < 0 ? 0 : 1]);\n      }\n    } else if ('loudness' in target) {",
        "[1]);\n      }\n    } else if ('loudness' in target) {",
        CONTRACT
    ),
    (
        'driver: the lost card is not the lost state',
        'themes/ember/driver.js',
        "'#screen .view .card.warn'",
        "'#screen .view .card.none'",
        CONTRACT
    ),
    (
        'driver: the toasts are never shown',
        'themes/ember/driver.js',
        '.filter((box) => !box.dataset.leaving)',
        '.filter(() => false)',
        CONTRACT
    ),
    (
        'driver: the page never rests',
        'themes/ember/driver.js',
        "resting: core.model.state() === 'resting'",
        "resting: core.model.state() === 'xresting'",
        CONTRACT
    ),
    (
        'driver: the volume shows its number only',
        'themes/ember/driver.js',
        'return unit ? `${num} ${unit}` : num;',
        'return num;',
        CONTRACT
    ),
    (
        'driver: the Device fold-out is not opened',
        'themes/ember/driver.js',
        "press(details.querySelector('summary'));",
        'void details;',
        COMPLETE
    ),
    (
        'driver: a slot save does not answer its question',
        'themes/ember/driver.js',
        "press(form.querySelector('button.danger'));",
        'void form;',
        COMPLETE
    ),
    (
        'driver: the loudness steppers swapped',
        'themes/ember/driver.js',
        "'-1': 'Lower the loudness', 1: 'Raise the loudness'",
        "'-1': 'Raise the loudness', 1: 'Lower the loudness'",
        COMPLETE
    ),
    (
        'frame: the button hidden',
        'core/frame.css',
        '#frame .fxbtn { position: relative !important; display: grid !important;',
        '#frame .fxbtn { position: relative !important; display: none !important;',
        CONTRACT
    ),
    (
        'frame: the button too small',
        'core/frame.css',
        'width: var(--frame-btn-size); height: var(--frame-btn-size); padding: 0; flex: none;',
        'width: 12px; height: 12px; padding: 0; flex: none;',
        CONTRACT
    ),
    (
        'frame: the button never fades',
        'core/frame.js',
        "button.classList.add('faded');",
        "button.classList.remove('faded');",
        ROWS
    ),
    (
        'stream: a followed device is not asked for',
        'core/stream.js',
        "if (follow.length > 0) query.push(`follow=${follow.join(',')}`);",
        '',
        ROWS
    ),
    (
        'actions: a hop is added by the edit route',
        'core/actions.js',
        "add: (hop) => hopWrite('api/v1/hops', hop),",
        "add: (hop) => hopWrite('api/v1/hops/edit', hop),",
        ROWS
    ),
    (
        'scheme: the root is not told',
        'core/scheme.js',
        'if (schemes.length > 1) root.dataset.scheme = now;',
        'if (false) root.dataset.scheme = now;',
        ROWS
    ),
    (
        'scheme: the word is not kept for the server',
        'core/scheme.js',
        "onCookie?.(mode === 'auto' ? null : mode);",
        '',
        ROWS
    ),
    (
        'lifecycle: the loop ignores a hidden page',
        'core/lifecycle.js',
        'shown = on;',
        'shown = true;',
        ROWS
    ),
    (
        'tone-drag: the gain read from the x',
        'core/tone-drag.js',
        'const db = map.dbAt(at.y);',
        'const db = map.dbAt(at.x);',
        ROWS
    ),
    (
        'ember: its listeners outlive its mount',
        'themes/ember/bind.js',
        'export const mine = (options = {}) => ({ ...options, signal: ember.signal ?? undefined });',
        'export const mine = (options = {}) => ({ ...options });',
        ROWS
    ),
]
def run(name, f, old, new, cmd):
    """One mutation in a fresh copy: `(killed, detail)`, `killed` None on a crash."""
    root = common.scratch('mut-effects-')
    try:
        web = f'{root}/bins/svolderd/web'
        shutil.copytree(common.WEB, web)
        os.makedirs(f'{root}/scripts/web-checks/node')
        for harness in ('effects', 'ember-order', 'contract'):
            shutil.copy(f'{S}node/{harness}.mjs', f'{root}/scripts/web-checks/node/{harness}.mjs')
        os.makedirs(f'{root}/scripts/web-checks/node/fixtures')
        shutil.copy(f'{S}node/fixtures/ember-parts.json',
                    f'{root}/scripts/web-checks/node/fixtures/ember-parts.json')
        src = open(f'{web}/{f}').read()
        if old not in src:
            return None, 'NO-MATCH'
        open(f'{web}/{f}', 'w').write(src.replace(old, new, 1))
        node = cmd[0] == 'node'
        r = subprocess.run(
            [cmd[0], f'{root}/{cmd[1]}'] if node else cmd, capture_output=True, text=True,
            timeout=600, env=dict(os.environ, SMOKE_WEB=web))
    finally:
        common.remove_tree(root)
    fails = [x for x in r.stdout.splitlines() if x.startswith('FAIL')]
    if fails:
        return True, fails[0][:110]
    if r.returncode != 0 and not node:
        # no FAIL line: an infrastructure crash is an error, never a kill
        tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:110]
        return None, tail
    if r.returncode != 0:
        # the node harness fails by an assertion, not a FAIL line; any other crash is an error
        lines = [x for x in r.stderr.splitlines() if 'Error' in x] or ['no output']
        return ('AssertionError' in r.stderr or None), lines[0][:110]
    return False, (r.stdout.splitlines() or [''])[-1][:110]


only = sys.argv[1:]
bad = 0
for name, f, old, new, cmd in M:
    if only and not any(o in name for o in only):
        continue
    killed, detail = run(name, f, old, new, cmd)
    label = 'ERROR   ' if killed is None else 'killed  ' if killed else 'SURVIVED'
    print(label, name, '|', detail)
    bad += 0 if killed else 1
print('survivors/errors:', bad)
sys.exit(1 if bad else 0)
