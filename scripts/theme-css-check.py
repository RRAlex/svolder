#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make theme-css-check`: three rules on the style sheets, two on the themes' scripts and one on
# the popups' wires, read as text.
#
# 1. No style sheet of a theme (bins/svolderd/web/themes/*/*.css) names the frame's own parts. The
#    frame draws the menu button, the menu, the picker and the layer they sit in (core/frame.css);
#    a theme dresses them only through the `--frame-*` tokens. A theme's rule on `#frame`,
#    `.fxmenu`, `.fxbtn`, `.fxpop`, `#fx-button`, `#fx-menu` or `#picker` could hide or move what
#    must always be reachable.
# 2. Every theme sets every colour token of the frame in each scheme it lists. The colour tokens are
#    the `--frame-*` names whose default in core/frame.css holds a colour (a shadow counts). A theme
#    sets each one in its root block: a rule on `:root` alone or on `:root[data-theme="<its id>"]`
#    alone, outside any media query (a rule on the root with anything more, in a media query or
#    for another theme's name sets it for some pages only and does not count). A theme that lists
#    two schemes (bins/svolderd/src/web/themes.rs) has a block for the other scheme under the
#    system's setting (`@media (prefers-color-scheme: <scheme>)`) and one under the person's
#    choice (`[data-scheme="<scheme>"]`), and each of those sets again every token the root block
#    gives a value of its own (a value that is only `var(...)` follows the theme's own token,
#    which the block sets; that the block does set it is not checked).
# 3. The frame's sheet turns to its phone layout at the core's one phone width: every width a
#    media query of core/frame.css names is `max-width: <PHONE_MAX_PX of core/phone.js>px`.
#
# 4. A theme reaches the core's services only through `core`. A theme file (bins/svolderd/web/
#    themes/**/*.js) imports a core module directly only if the module, or the name it takes, is on
#    PURE below: the pure modules and names, which hold no state and reach no service. Anything else
#    (the model, the actions, the menu, the lifecycle, the stream, ...) is `core.<member>`.
# 5. Every key a theme's script compares a key press with is in the theme's key table. The table is
#    themes/<id>/key-table.js: the names it lists are its `key: '<name>'` values and the strings of
#    its `key: [...]` and `keys: [...]` lists (and of a list it names once, `const ARROWS =
#    [...]`). A theme file that says `event.key === '<name>'` (or `!==`; the event may be called
#    `event`, `evt`, `ev`, `e` or `key`), `case '<name>':` inside a `switch` over such a key, or
#    `key === '<name>'` after `const key = event.key`, with a name the table does not
#    list, fails: a key the page answers to and the Help popup would not show. KEY_ALLOWED below
#    holds the names that need no entry, each with its reason. This half is a pattern, not a
#    parser: a key looked up in a map (`STEPS[event.key]`) is not seen, which is fine on a widget
#    (its keys are its zone's entries, by hand) and is why the other half does not read the
#    handler at all:
#    no theme file listens for a key on the window or the document. A key of the whole page is a
#    `run` entry of the table, which `core.keys.bind` dispatches, so these fail, however the
#    handler reads the key:
#    - a `keydown`, `keyup` or `keypress` listener whose receiver holds a global name anywhere in
#      it (GLOBALS: `window`, `document`, `globalThis`, `self`, `win`, `doc`, `ownerDocument`,
#      `defaultView`, `body`, `documentElement`, `getRootNode`, `parentNode`, ...; so
#      `(window)`, `[window][0]`, `root.getRootNode()`, `document.body.parentNode`), or a name
#      the file binds to an expression that holds one: the whole initialiser is read (`x ??
#      window`, `x || document`, a ternary, an object that holds the window), and so are an
#      assignment, a member set (`this.place = window` makes every `.place` global), a parameter's
#      default and `{ ownerDocument: place } = root`. An element looked up (`querySelector`,
#      `getElementById`, `closest`, `createElement`) is an element whatever it was looked up
#      on, unless the selector is `html`, `body` or `:root`;
#    - a listener on a parameter that a call in the same file hands a global (`wire(window)`);
#    - a listener on a parameter the check cannot follow: the function is exported, has no name,
#      is handed on as a value (`.forEach(wire)`, `wire.call(...)`), or takes the parameter apart
#      from an object; and a listener on a bare `this`. HANDED below allows one such listener a
#      line, with what its callers hand it;
#    - any listener on the window or the document whose event name is put together (`'key' +
#      'down'`, a template, a call, or a name the file binds to one of those; a name that stands
#      for a string or a list of strings is read as those strings, which the last line sees);
#    - an `onkeydown =` on anything, the string `'onkeydown'`, and one of the three names anywhere
#      but as the first argument of `addEventListener` or of `new KeyboardEvent` (in a list of
#      event names, handed to a helper).
#    LISTENERS below allows a listener on the window or the document: one line each, with its
#    file, its event, the table entry it answers for and the reason. The entry is one marked
#    `own: true` (a `keyup` may name any entry: the release of a key `bind` dispatched; a
#    listener that reads no key names none, and its file reads no `.key`, `.code`, `.keyCode`
#    or `.which` anywhere), and every `own` entry of a table has a line, so
#    `own` is no way around the binding either. A file with more such listeners than lines, or
#    fewer, fails.
#    This half too is a pattern, not a parser, and it reads one file at a time. What it does not
#    see:
#    - a global that reaches the file under a name of its own from another file: an import, a
#      member of an object built elsewhere (`opts.place`), what a function returns (`pick()`);
#    - a HANDED line whose reason is no longer true (people read the reason, nothing checks it);
#    - an element lookup whose selector is put together and gives the page's root;
#    - an event name put together elsewhere and handed in (`on('key' + 'down')` with
#      `window.addEventListener(name, ...)` inside), or put together on a receiver the check does
#      not know for global;
#    - a listener added by other means (`Reflect.apply`, `EventTarget.prototype`, `eval`, an
#      `on...` attribute whose name is put together).
#    The browser check scripts/web-checks/keys.py presses what the table lists and 21 stray keys,
#    and counts the key listeners on the window and the document against LISTENERS below, by the
#    script that added each: it sees the last three of this list (its docstring says what it
#    does not).
#
# 6. A popup names its device. A popup is about the device its page is about, which need not be
#    the one on show, and the core's defaults are the shown one. So in a popup's wire
#    file (every `.js` under a folder of bins/svolderd/web/core/popups/, its draw and any helper
#    beside its wire included):
#    - a call of a device action has an object literal among its arguments that names `device`
#      (`{ device }`, `{ device: url }`; not `{ device: undefined }`). The actions are read from
#      core/actions.js: every member that hands its `options` on, through `aimed(options)`
#      (`setControl`, `identify`, the preset calls of `actions.computer`, ...) or as they are
#      (the configuration's writes, which have no default: `renameDevice`, `setProfile`,
#      `setPassphrase`, `makeDefault`, `removeDevice`), so a new one is covered as it is written;
#    - `.part(` of the store (a receiver that ends in `store` or `model`) has a second argument,
#      `.device(` and `.refresh(` have one: the device's address, not `undefined` or `null`;
#    - both by another name the file gives the actions or the store (`const s = core.model`), and
#      by a quoted name (`core.actions['identify'](...)`);
#    - neither is taken off its object: `const { part } = store`, `actions.identify.call(...)`, an
#      action handed on as a value and an action called by a computed name (`actions[name]()`)
#      fail as they stand, since the call could not be read;
#    - `store.page().device`, the shown device's model, is not read.
#    `store.shown()` is allowed: a wire reads it to say which device is the shown one, and to
#    name it in such a call. A pattern, not a parser. What it does not see: an options object
#    built before the call (`actions.setControl(k, v, to)` fails: write the object in the call),
#    a `{ device }` that sits deeper inside an object argument, an alias made by anything but a
#    plain `const x = …store` (a parameter, an import, a member), a store or the actions reached
#    from another file, and whether the device named is the right one; the wires' node cases
#    run with another device on show than the page's and read each request.
#
# `--self-test` runs the same rules on copies of the sheets with one thing broken in each and
# fails when a copy passes.

import os
import pathlib
import re
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tmp_base  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
WEB = ROOT / "bins" / "svolderd" / "web"
LISTED = ROOT / "bins" / "svolderd" / "src" / "web" / "themes.rs"
NAMES = re.compile(r"#frame\b|#fx-button\b|#fx-menu\b|#picker\b|\.fxbtn\b|\.fxmenu\b|\.fxpop\b")
COMMENT = re.compile(r"/\*.*?\*/", re.S)
COLOUR = re.compile(
    r"#[0-9a-f]{3,8}\b|\b(rgba?|hsla?|color-mix)\(|\btransparent\b|\bcurrentcolor\b",
    re.I)
DECLARED = re.compile(r"(--frame-[\w-]+)\s*:\s*([^;}]+)")
ROOT_RULE = re.compile(r':root(?:\[data-theme="(?P<theme>[\w-]+)"\])?(?P<rest>.*)$')
SCHEME_REST = re.compile(r'(?::not\(\[data-scheme="\w+"\]\)|\[data-scheme="\w+"\])$')
SCHEME_MEDIA = re.compile(r"\(prefers-color-scheme: \w+\)$")


ALL = None
# What a theme may import from the core directly (rule 4): `module file -> None (every export)` or
# `-> the names it may take`. None reaches a service, and each is a function of its arguments or a
# constant but for the three that say otherwise where they stand: the two shared widgets (which
# list or question is open in each scope) and `registerTheme` (it adds to the registry's list).
# The list is the one place; docs/design/web-themes.md states the rule.
PURE = {
    # whole modules
    "act.js": ALL,  # createAct(core) over the core it is given; MINE_MS
    "contract.js": ALL,  # the lists the checks and the drivers ask for
    "gestures.js": ALL,  # dragAlong, sliderKeys: listen on the element they are handed (the check no
    # longer follows `sliderKeys`'s receiver: its callers, `*/controls-wire.js`, `*/volume-wire.js`
    # and `*/computer-wire.js`, all pass a section's box)
    "html.js": ALL,
    "phone.js": ALL,
    "place.js": ALL,
    "plot.js": ALL,
    "tone-curve.js": ALL,
    "viz-model.js": ALL,
    "vu-model.js": ALL,
    "volume-ask.js": ALL,  # the ask's rules; `askExpiry` holds one timer, its own, per call
    "words.js": ALL,
    # the shared widgets: each draws into the place it is given and reaches no service. Its one
    # piece of state is which list (or question) is open in each scope (a modal of the stack,
    # else the page), so a theme's and a popup's do not close each other
    "inline-ask.js": ALL,
    "list.js": ALL,
    # some exports of a module that holds a service beside them
    "api.js": {"newest"},  # one write in flight and the newest waiting, per call
    # `daemonNow` keeps one page-wide table of the first sight of each `now_ms`, shared on purpose
    "clock.js": {"AGES_EVERY_MS", "clockOffset", "daemonClock", "daemonNow",
                 "elapsedUs", "fraction",
                 "timeWords"},  # `tickAges` reads and writes the page: `core.clock.tickAges`
    "computer.js": {"POINT_TYPES", "ROWS_SHOWN", "THIS_COMPUTER", "markersOf"},
    # the key table's readers; binding a table is `core.keys.bind`
    "keys.js": {"FRAME_KEYS", "HELP", "RESERVED", "TYPING", "capsOf", "keysOf", "listed",
                "zoneOf", "zonesOf"},
    "old-readings.js": {"deviceOld"},  # a pure function of one device's model (the frame owns the badge)
    "registry.js": {"registerTheme"},  # how a theme tells the page it exists
    "repeat.js": {"REPEAT_AFTER_MS", "REPEAT_EVERY_MS"},
    "table.js": {"isLong"},
    "title.js": {"TITLE", "statusOf"},
    "tone-drag.js": {"BAND_PARAM", "LABEL_GAP_PX", "REACH", "grab", "labelAt", "wheelSteps"},
    "write.js": {"mayWrite"},  # a pure function of the page
}
# The key names a theme may compare a key press with and leave out of its key table.
KEY_ALLOWED = {
    # the focus kept inside a sheet the theme draws itself: the browser's own key, which goes on
    # doing what it does everywhere, so there is nothing to tell a person
    "Tab": "the focus trap of a theme's own sheet",
}
# The listeners for a key on the window or the document that stay outside `core.keys.bind`: `(file
# under bins/svolderd/web, event, id of the entry of the theme's key table it answers for or None
# for a listener that reads no key, reason)`, one line a listener.
LISTENERS = [
    ("themes/ember/arrange.js", "keydown", "arrange-leave",
     "Escape leaves Arrange mode from a field and from the menu's button too, where `bind` "
     "leaves a key alone"),
    ("themes/ember/tone-graph.js", "keydown", "curve-end",
     "Escape ends a drag on the device's curve in the capture phase, whatever has the focus"),
    ("themes/ember/computer.js", "keydown", "curve-end",
     "Escape ends a drag on the computer's curve in the capture phase, whatever has the focus"),
    ("themes/nightdrive/volume-keys.js", "keyup", "volume",
     "the release of `↑` or `↓`, which ends the repeat of the held key `bind` dispatched"),
    ("themes/nightdrive/dash-fade.js", "keydown", None,
     "any key press ends the deaf time after the press that woke the dash; no key is read"),
    ("themes/nightdrive/hud-wire.js", "keydown", None,
     "any input wakes the faded HUD (the list of waking events); no key is read"),
]
# The key listeners on a receiver the check cannot follow: `(file under bins/svolderd/web, the
# function, its parameter, reason)`. The receiver is a parameter of a function whose callers the
# check cannot all read (it is exported, taken apart from an object, handed on as a value or has
# no name), so nothing here proves it is not the window: the reason says what the callers hand it.
HANDED = [
    ("themes/signal/strip-wire.js", "wireStrip", "band",
     "the strip of hops (`#sg-line`), which signal.js looks up under the theme's root"),
]
# A key event's name as a string (`on` before it is the attribute's name), and what may stand
# before it: a listener's receiver, or a key event the test driver makes.
KEY_EVENT_NAME = re.compile(r"(?P<q>['\"`])(?P<on>on)?(?P<event>keydown|keyup|keypress)(?P=q)")
KEY_HANDLER = re.compile(r"\.on(?P<event>keydown|keyup|keypress)\s*=(?!=)")
LISTENS = re.compile(r"\??\.addEventListener\(\s*$")
LISTENER = re.compile(r"\??\.addEventListener\(")
MADE = re.compile(r"\bnew\s+KeyboardEvent\(\s*$")
# The names that are the window or the document, or lead to one: a receiver or an initialiser
# that holds one of them is taken for global.
GLOBALS = {"window", "document", "globalThis", "self", "top", "parent", "win", "doc",
           "ownerDocument", "defaultView", "body", "documentElement", "getRootNode", "parentNode",
           "parentElement", "offsetParent"}
COMPUTED = "(a computed name)"
READS_KEY = re.compile(r"\.(?:key|code|keyCode|which)\b")
JS_STRING = re.compile(r"'(?:[^'\\\n]|\\.)*'|\"(?:[^\"\\\n]|\\.)*\"|`(?:[^`\\]|\\.)*`", re.S)
# A call that gives an element whatever it is called on, unless it asks for the page's root.
LOOKUP = re.compile(r"\??\.\s*(?:querySelector(?:All)?|getElementById|getElementsBy\w+|"
                    r"createElement(?:NS)?|closest)\((?P<arg>[^()]*)\)")
PAGE_ROOTS = {"html", "body", ":root"}
# `name = ...` (a declaration, an assignment, a member set, a parameter's default) and
# `const name of ...`; `{ ownerDocument: name } = ...`.
BOUND = re.compile(r"(?<![\w$])(?P<name>[\w$]+)\s*=(?![=>])|"
                   r"\b(?:const|let|var)\s+(?P<each>[\w$]+)\s+of\b")
TAKEN_APART = re.compile(r"\{(?P<parts>[^{}]*)\}\s*=(?![=>])")
RENAMED = re.compile(r"(?<![\w$])(?P<key>[\w$]+)\s*:\s*(?P<name>[\w$]+)")
FUNCTION_BEFORE = re.compile(r"\bfunction\s*\*?\s*$")
NOT_A_FUNCTION = {"if", "for", "while", "switch", "catch", "function", "return", "await",
                  "typeof", "new", "with", "super"}
TABLE_ENTRY = re.compile(r"\bid:\s*(?P<q>['\"])(?P<id>[^'\"]+)(?P=q)")
# A key press compared with a literal: `event.key === 'x'`, `'x' !== e.key`.
KEY_EVENT = r"\b(?:event|evt|ev|e|key)\.key\b"
KEY_LITERAL = r"(?P<q>['\"])(?P<name>(?:(?!(?P=q)).)+)(?P=q)"
KEY_COMPARED = re.compile(
    rf"{KEY_EVENT}\s*[!=]==?\s*{KEY_LITERAL}"
    rf"|{KEY_LITERAL.replace('<q>', '<q2>').replace('=q)', '=q2)').replace('<name>', '<name2>')}"
    rf"\s*[!=]==?\s*{KEY_EVENT}")
# The same through a local: `const key = event.key` ... `key === 'x'`.
KEY_LOCAL = re.compile(rf"\bconst\s+key\s*=\s*{KEY_EVENT}")
KEY_LOCAL_COMPARED = re.compile(rf"(?<![.\w])key\s*[!=]==?\s*{KEY_LITERAL}")
# A `switch` over a key press, and a `case` of it.
KEY_SWITCH = re.compile(rf"\bswitch\s*\(\s*(?:{KEY_EVENT}|key)\s*\)\s*\{{")
KEY_CASE = re.compile(rf"\bcase\s+{KEY_LITERAL}\s*:")
# What a key table lists: `key: 'x'`, the strings of `key: [...]` and `keys: [...]` and those of a
# list the file names once for several entries (`const ARROWS = [...]`).
TABLE_KEY = re.compile(rf"\bkey:\s*{KEY_LITERAL}")
TABLE_KEYS = re.compile(r"(?:\bkeys?:|\bconst\s+\w+\s*=)\s*\[(?P<list>[^\]]*)\]")
TABLE_STRING = re.compile(r"'((?:[^'\\]|\\.)*)'|\"((?:[^\"\\]|\\.)*)\"")

JS_COMMENT = re.compile(r"/\*.*?\*/|(?<![:'\"`\\])//[^\n]*", re.S)
# A static import or re-export of a core module (a query string or a fragment after the file is
# still that file), or a dynamic one: `import(` of a quoted string or a template literal, bare or
# inside `new URL(`, that names `core/`. A dynamic path that is put together (`'../../core/' +
# name`, `${name}.js`) has no file to look up and is flagged as it stands. This is a pattern, not a
# parser: a path built wholly elsewhere is not seen.
CORE_IMPORT = re.compile(
    r"""\b(?:import|export)\s*(?P<what>[^'";()]*?)\s*(?:from\s*)?['"](?P<path>[^'"\n]*/core/"""
    r"""(?P<file>[\w.-]+))(?:[?#][^'"\n]*)?['"]"""
    r"""|\bimport\s*\(\s*(?:new\s+URL\s*\(\s*)?['"`](?P<dpath>[^'"`\n]*?\bcore/(?P<dfile>[\w.-]*))"""
    r"""(?P<dend>[^'"`\n]*['"`]\s*[),]?)""",
    re.S)


def core_imports(path):
    """`(line, core file, names)` for each import of a core module in the theme file `path`; the
    names are `{"*"}` for a namespace or a dynamic import."""
    text = path.read_text()
    text = JS_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    for found in CORE_IMPORT.finditer(text):
        line = text.count("\n", 0, found.start()) + 1
        if found.group("dpath") is not None:
            # the file, unless the path goes on being put together after it
            whole = re.fullmatch(r"""(?:[?#][^'"`]*)?['"`]\s*[),]""", found.group("dend"))
            file = found.group("dfile")
            yield line, file if whole and file and "$" not in file else "(a computed path)", {"*"}
            continue
        what = found.group("what").strip()
        if what.startswith("*") or not what:
            names = {"*"} if what else set()
        else:
            names = {part.split(" as ")[0].strip() for part in what.strip("{} \n").split(",")
                     if part.strip()}
        yield line, found.group("file"), names


def direct_imports(web):
    """What rule 4 finds wrong in the theme files under `web`, one line each."""
    for path in sorted((web / "themes").rglob("*.js")):
        for line, file, names in core_imports(path):
            allowed = PURE.get(file, set())
            if file in PURE and allowed is ALL:
                continue
            bad = sorted(names - allowed) if file in PURE else sorted(names) or ["(nothing)"]
            if bad:
                yield (f"{shown(path)}:{line}: imports {', '.join(bad)} of core/{file} directly; "
                       "reach it through `core` (PURE in scripts/theme-css-check.py lists what a "
                       "theme may import)")


def table_keys(path):
    """The key names the key table at `path` lists."""
    text = JS_COMMENT.sub("", path.read_text())
    names = {found.group("name") for found in TABLE_KEY.finditer(text)}
    for found in TABLE_KEYS.finditer(text):
        names.update(a or b for a, b in TABLE_STRING.findall(found.group("list")))
    return names


def compared_keys(path):
    """`(line, name)` for each key name the theme file `path` compares a key press with."""
    text = path.read_text()
    text = JS_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    line = lambda at: text.count("\n", 0, at) + 1
    for found in KEY_COMPARED.finditer(text):
        yield line(found.start()), found.group("name") or found.group("name2")
    if KEY_LOCAL.search(text):
        for found in KEY_LOCAL_COMPARED.finditer(text):
            yield line(found.start()), found.group("name")
    for found in KEY_SWITCH.finditer(text):
        depth, at = 1, found.end()
        while depth and at < len(text):
            depth += {"{": 1, "}": -1}.get(text[at], 0)
            at += 1
        for case in KEY_CASE.finditer(text, found.end(), at):
            yield line(case.start()), case.group("name")


def hollow(text):
    """`text` with what its strings hold blanked out, every position kept."""
    return JS_STRING.sub(lambda m: m.group(0)[0] + " " * (len(m.group(0)) - 2) + m.group(0)[-1],
                         text)


def span_end(flat, at, stops=";,"):
    """Where the expression that starts at `at` ends: at one of `stops` outside every bracket, or
    at the bracket that closes what holds it."""
    depth = 0
    while at < len(flat):
        char = flat[at]
        if char in "([{":
            depth += 1
        elif char in ")]}":
            if not depth:
                return at
            depth -= 1
        elif char in stops and not depth:
            return at
        at += 1
    return at


def receiver_start(flat, end):
    """Where the receiver that ends at `end` starts: names, dots and whole brackets."""
    at = end
    while at > 0:
        char = flat[at - 1]
        if char in ")]":
            depth = 0
            while at > 0:
                depth += {")": 1, "]": 1, "}": 1, "(": -1, "[": -1, "{": -1}.get(flat[at - 1], 0)
                at -= 1
                if not depth:
                    break
        elif re.match(r"[\w$.?]", char):
            at -= 1
        else:
            break
    return at


def holds_global(expr, names):
    """Whether `expr` may be the window or the document: one of `names` stands in it, outside a
    string and not as what an element was looked up on."""
    for part in re.split(r"\?\?|\|\||&&|\?(?!\.)|:", expr):
        found = list(LOOKUP.finditer(part))
        if found:
            arg = found[-1].group("arg").strip()
            if len(arg) > 1 and arg[0] in "'\"`" and arg[-1] == arg[0] \
                    and arg[1:-1].strip() in PAGE_ROOTS:
                return True
            part = part[found[-1].end():]
        if names & set(re.findall(r"[\w$]+", hollow(part))):
            return True
    return False


def split_top(flat, start, end):
    """The `(start, end)` of each piece of `flat[start:end]` between commas outside brackets."""
    at = start
    while at < end:
        stop = min(span_end(flat, at, ","), end)
        yield at, stop
        at = stop + 1


def functions(flat):
    """Every function of the hollowed text: `{name, params, start, body, end, exported}`. `params`
    is a list of `(names, simple)`: one name for a plain parameter, each name of one taken apart."""
    heads = []
    for found in re.finditer(r"\bfunction\b\s*\*?\s*(?P<name>[\w$]*)\s*\(", flat):
        heads.append((found.start(), found.group("name"), found.end() - 1, None))
    for found in re.finditer(r"(?<![\w$.])(?P<name>[\w$]+)\s*\(", flat):
        if found.group("name") in NOT_A_FUNCTION \
                or FUNCTION_BEFORE.search(flat, max(0, found.start() - 40), found.start()):
            continue
        close = span_end(flat, found.end(), "")
        if re.compile(r"\s*\{").match(flat, close + 1):
            heads.append((found.start(), found.group("name"), found.end() - 1, None))
    for found in re.finditer(r"=>", flat):
        before = found.start()
        while before and flat[before - 1].isspace():
            before -= 1
        if flat[before - 1:before] == ")":
            open_at = receiver_start(flat, before)
            heads.append((open_at, "", open_at, found.end()))
        else:
            one = re.search(r"[\w$]+$", flat[max(0, before - 80):before])
            if one:
                heads.append((before - len(one.group(0)), "", None, found.end()))
    for start, name, open_at, arrow in heads:
        if open_at is None:
            pieces = [(start, arrow - 2)]
            after = arrow
        else:
            close = span_end(flat, open_at + 1, "")
            pieces = list(split_top(flat, open_at + 1, close))
            after = arrow if arrow is not None else close + 1
        params = []
        for a, b in pieces:
            piece = flat[a:b].strip()
            plain = re.fullmatch(r"([\w$]+)(\s*=.*)?", piece, re.S)
            params.append(([plain.group(1)], True) if plain
                          else (re.findall(r"[\w$]+", piece), False))
        body = after
        while body < len(flat) and flat[body].isspace():
            body += 1
        end = span_end(flat, body + 1, "") + 1 if flat[body:body + 1] == "{" \
            else span_end(flat, body)
        lead = flat[max(0, start - 120):start]
        if not name:
            given = re.search(r"(?<![\w$.])([\w$]+)\s*=\s*(?:async\s+)?$", lead)
            name = given.group(1) if given else ""
        exported = bool(name) and bool(
            re.search(r"\bexport\s+(?:default\s+)?(?:async\s+)?(?:(?:const|let|var)\s+[\w$]+\s*=\s*"
                      r"(?:async\s+)?)?(?:function\s*\*?\s*)?$", lead)
            or re.search(r"\bexport\s*(?:default\s+%s\b|\{[^}]*\b%s\b)" % (name, name), flat))
        yield {"name": name, "params": params, "start": start, "body": body, "end": end,
               "exported": exported}


def calls(flat, function):
    """`(calls, followed)` of a function of the file: the `(start, end)` of each call's
    arguments, and whether those are all its calls as far as this file can tell (it has a name,
    is not exported, is called here and is never named but to call it)."""
    name = function["name"]
    if not name:
        return [], False
    found, loose = [], function["exported"]
    for one in re.finditer(r"(?<![\w$])%s(?![\w$])" % re.escape(name), flat):
        if one.start() == function["start"] or function["start"] <= one.start() < function["body"] \
                or re.compile(r"\s*=(?![=>])").match(flat, one.end()):
            continue
        opened = re.compile(r"\s*\(").match(flat, one.end())
        defined = FUNCTION_BEFORE.search(flat, max(0, one.start() - 40), one.start())
        if not opened or defined:
            loose = loose or not defined
            continue
        at = opened.end()
        found.append((at, span_end(flat, at, "")))
    return found, bool(found) and not loose


def bindings(flat):
    """`(name, start, end)` of each expression the hollowed text binds a name to."""
    for one in BOUND.finditer(flat):
        stops = ";," if one.group("name") else ""
        yield (one.group("name") or one.group("each"), one.end(),
               span_end(flat, one.end(), stops))


def global_names(text, flat, found):
    """The names of the file that may be the window or the document: GLOBALS, what the file binds
    to an expression that holds one, and a parameter a call in the file hands one."""
    names = set(GLOBALS)
    bound = list(bindings(flat))
    handed = []
    for function in found:
        sites, _followed = calls(flat, function)
        for at, end in sites:
            for (names_of, simple), (a, b) in zip(function["params"], split_top(flat, at, end)):
                if simple:
                    handed.append((names_of[0], a, b))
    while True:
        more = {name for name, a, b in bound + handed if holds_global(text[a:b], names)}
        for apart in TAKEN_APART.finditer(flat):
            more |= {one.group("name") for one in RENAMED.finditer(apart.group("parts"))
                     if one.group("key") in names}
        if more <= names:
            return names
        names |= more


def handed_in(flat, found, recv, at):
    """`(function, parameter)` when the receiver `recv` of the listener at `at` is a parameter the
    check cannot follow to its callers, or a bare `this`; else None."""
    words = re.findall(r"[\w$]+", hollow(recv))
    if not words:
        return None
    around = [one for one in found if one["body"] <= at < one["end"]]
    if words == ["this"]:
        named = [one for one in around if one["name"]]
        return (max(named, key=lambda one: one["start"])["name"] if named else "", "this")
    base = words[0]
    holders = [one for one in around if any(base in names for names, _simple in one["params"])]
    if not holders:
        return None
    function = max(holders, key=lambda one: one["start"])
    if re.search(r"\b(?:const|let|var)\s+(?:[\[{][^=;]*)?\b%s\b" % re.escape(base),
                 flat[function["body"]:at]):
        return None
    simple = any(simple and names == [base] for names, simple in function["params"])
    if simple and calls(flat, function)[1]:
        return None
    return function["name"], base


def global_listeners(path):
    """`(line, event, handed)` for each key listener of the theme file `path` on the window or the
    document (`handed` None) or on a parameter the check cannot follow (`handed` its `(function,
    parameter)`), each key event's name that is not a listener's own first argument, and each
    listener on the window or the document whose event name is put together (the event is
    COMPUTED)."""
    text = path.read_text()
    text = JS_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)
    flat = hollow(text)
    line = lambda at: text.count("\n", 0, at) + 1
    events = list(KEY_EVENT_NAME.finditer(text))
    # a file that names no key event has no listener to follow a parameter for
    found = list(functions(flat)) if events else []
    known = []

    def names():
        if not known:
            known.append(global_names(text, flat, found))
        return known[0]

    receiver = lambda end: text[receiver_start(flat, end):end]
    for one in KEY_HANDLER.finditer(text):
        yield line(one.start()), one.group("event"), None
    for one in events:
        if one.group("on"):
            yield line(one.start()), one.group("event"), None
            continue
        if MADE.search(text[max(0, one.start() - 200):one.start()]):
            continue
        listens = LISTENS.search(flat, max(0, one.start() - 200), one.start())
        recv = receiver(listens.start()) if listens else ""
        if not recv or holds_global(recv, names()):
            yield line(one.start()), one.group("event"), None
            continue
        handed = handed_in(flat, found, recv, one.start())
        if handed:
            yield line(one.start()), one.group("event"), handed
    plain = lambda a, b: re.fullmatch(r"[\s\w$.,\[\]'\"]*", flat[a:b])
    bound = []
    for one in LISTENER.finditer(flat):
        end = span_end(flat, one.end(), ",")
        # a name's value is the binding nearest before the listener
        first = flat[one.end():end].strip()
        if re.fullmatch(r"['\"] *['\"]", first):
            continue
        if not bound:
            bound.extend(bindings(flat))
        last = [(a, b) for name, a, b in bound if name == first and a < one.start()][-1:]
        if plain(one.end(), end) and all(plain(a, b) for a, b in last):
            continue
        recv = receiver(one.start())
        if not recv or not holds_global(recv, names()):
            continue
        yield line(one.start()), COMPUTED, None


def table_entries(path):
    """`{id: whether the entry says own: true}` for the key table at `path`."""
    text = JS_COMMENT.sub("", path.read_text())
    found = list(TABLE_ENTRY.finditer(text))
    ends = [one.start() for one in found[1:]] + [len(text)]
    return {one.group("id"): bool(re.search(r"\bown:\s*true\b", text[one.end():end]))
            for one, end in zip(found, ends)}


def unbound_listeners(web):
    """What the listener half of rule 5 finds wrong under `web`, one line each."""
    for theme in sorted(path for path in (web / "themes").iterdir() if path.is_dir()):
        table = theme / "key-table.js"
        entries = table_entries(table) if table.exists() else {}
        lines = [one for one in LISTENERS if one[0].startswith(f"themes/{theme.name}/")]
        for path in sorted(theme.rglob("*.js")):
            name = path.relative_to(web).as_posix()
            every = sorted(global_listeners(path), key=lambda one: one[:2])
            found = [(line, event) for line, event, handed in every if handed is None]
            allowed = sorted(event for file, event, _id, _why in lines if file == name)
            passes = {(function, param) for file, function, param, _why in HANDED if file == name}
            seen = set()
            for line, event, handed in every:
                if handed is None:
                    continue
                seen.add(handed)
                if handed in passes:
                    continue
                function, param = handed
                whose = f"`{function}`" if function else "a function with no name"
                yield (f"{shown(path)}:{line}: listens for `{event}` on `{param}`, which {whose} "
                       "is handed by callers the check cannot read (the function is exported, "
                       "handed on as a value or has no name, or the parameter is taken apart; a "
                       "bare `this` is the same): it may be the window. Listen on an element the "
                       "file looks up, or add a line to HANDED in scripts/theme-css-check.py "
                       "with what the callers hand it")
            for function, param in sorted(passes - seen):
                yield (f"{shown(path)}: HANDED allows a key listener on `{param}` of "
                       f"`{function}` here and none was found: take the line out")
            if sorted(event for _line, event in found) == allowed:
                continue
            for line, event in found:
                yield (f"{shown(path)}:{line}: listens for `{event}` on the window or the document "
                       "outside `core.keys.bind` (or names the event outside a listener): a key "
                       "of the whole page is a `run` entry of the theme's key table; LISTENERS in "
                       f"scripts/theme-css-check.py allows {len(allowed)} here, {len(found)} found")
            if not found:
                yield (f"{shown(path)}: LISTENERS allows {len(allowed)} key listeners on the "
                       "window or the document here and none was found: take the lines out")
        for file, _function, _param, why in HANDED:
            if file.startswith(f"themes/{theme.name}/") and not (web / file).exists():
                yield f"{shown(web / file)}: HANDED names a file that is not there"
            if file.startswith(f"themes/{theme.name}/") and not why:
                yield f"{shown(web / file)}: a line of HANDED gives no reason"
        for file, event, entry, why in lines:
            if not (web / file).exists():
                yield f"{shown(web / file)}: LISTENERS names a file that is not there"
            if not why:
                yield f"{shown(web / file)}: a line of LISTENERS gives no reason"
            if entry is None:
                text = JS_COMMENT.sub("", (web / file).read_text()) if (web / file).exists() else ""
                if READS_KEY.search(text):
                    yield (f"{shown(web / file)}: LISTENERS says its listener reads no key, and "
                           "the file reads `.key`, `.code`, `.keyCode` or `.which`: name the "
                           "entry the listener answers for")
                continue
            if entry not in entries:
                yield (f"{shown(web / file)}: LISTENERS names the entry `{entry}`, which "
                       f"{shown(table)} does not hold")
            elif not entries[entry] and event != "keyup":
                yield (f"{shown(web / file)}: LISTENERS names the entry `{entry}`, which does not "
                       "say `own: true`: a key press of the page is bound by its `run`")
        named = {entry for _file, _event, entry, _why in lines}
        for entry, own in entries.items():
            if own and entry not in named:
                yield (f"{shown(table)}: the entry `{entry}` says `own: true` and no line of "
                       "LISTENERS in scripts/theme-css-check.py names the listener that answers it")


def unlisted_keys(web):
    """What rule 5 finds wrong in the theme files under `web`, one line each."""
    for theme in sorted(path for path in (web / "themes").iterdir() if path.is_dir()):
        table = theme / "key-table.js"
        if not table.exists():
            yield (f"{shown(theme)}: the theme has no key table (key-table.js: every key it "
                   "answers to, core/keys.js says how)")
            continue
        listed = table_keys(table)
        if not listed:
            yield f"{shown(table)}: no `key: '<name>'` was read from the key table"
        for path in sorted(theme.rglob("*.js")):
            if path == table:
                continue
            for line, name in sorted(set(compared_keys(path))):
                if name not in listed and name not in KEY_ALLOWED:
                    yield (f"{shown(path)}:{line}: the key `{name}` is compared with a key press "
                           f"and is not in {shown(table)} (an entry, or a name of its `keys`)")


# The store's reads that default to the shown device, with how many arguments name the device:
# `part(name, url)`, `device(url)`, `refresh(url)` (rule 6).
STORE_ARGS = {"part": 2, "device": 1, "refresh": 1}
# an object literal that names the device, and one that names it as nothing
NAMES_DEVICE = re.compile(r"(?<![\w$.])device\s*(?::|,|\})")
NO_DEVICE = re.compile(r"(?<![\w$.])device\s*:\s*(?:undefined|null|void\b)")
# the side of the EQ library's calls (`autoEq*`) may be the computer, named as `computer: true`
NAMES_COMPUTER = re.compile(r"(?<![\w$.])computer\s*:\s*true\b")
NOTHING = {"undefined", "null", "void 0", ""}
# an object literal that names the walk's job, and one that names it as nothing
NAMES_JOB = re.compile(r"(?<![\w$.])job\s*(?::|,|\})")
NO_JOB = re.compile(r"(?<![\w$.])job\s*:\s*(?:undefined|null|void\b)")
# the calls of `actions.walk` that act on a walk already running: each names its job (`start` names
# none, it makes one)
WALK_ON_JOB = ("answer", "back", "cancel")
# a member of core/actions.js that hands its `options` on, as `aimed(options)` or as they are: an
# action on a device
AIMED = re.compile(r"^\s+(\w+):\s*\([^)\n]*\boptions\)\s*=>\s*\w+\.\w+\([^\n]*\boptions\)", re.M)


def device_actions():
    """The names of the actions that act on a device, read from core/actions.js: every member
    that hands its `options` on, through `aimed(options)` (the shown device when the caller names
    none) or as they are (no device when the caller names none)."""
    return tuple(sorted(set(AIMED.findall((WEB / "core" / "actions.js").read_text()))))


def aliases(flat, tail):
    """The names a file binds to an expression that ends in `tail` (a pattern): `s = core.model`."""
    found = re.finditer(r"(?:const|let|var)\s+([\w$]+)\s*=\s*[\w$.?]*\b(?:%s)\s*[;\n]" % tail, flat)
    return {one.group(1) for one in found}


def receiver(word, names):
    """A pattern for a receiver: a chain that ends in `word`, or one of `names` standing alone."""
    alone = "|".join(sorted(re.escape(name) for name in names))
    return r"(?:\b(?:%s)|(?<![\w$.])(?:%s))" % (word, alone) if alone else r"\b(?:%s)" % word


def unnamed_devices(web):
    """What rule 6 finds wrong in the files of the popups under `web`, one line each."""
    actions = device_actions()
    if len(actions) < 10:
        yield (f"{shown(WEB / 'core' / 'actions.js')}: only {len(actions)} device actions were read "
               "(the members that hand their `options` on): bring rule 6 in step with the file")
        return
    named = "|".join(actions)
    popups = web / "core" / "popups"
    folders = sorted(path for path in popups.iterdir() if path.is_dir()) if popups.is_dir() else []
    for path in sorted(file for folder in folders for file in folder.rglob("*.js")):
        plain = JS_COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), path.read_text())
        flat = hollow(plain)
        where = lambda at: f"{shown(path)}:{flat.count(chr(10), 0, at) + 1}"  # noqa: E731
        acts = receiver(r"actions(?:\s*\??\.\s*computer)?",
                        aliases(flat, r"actions(?:\s*\??\.\s*computer)?"))
        store = receiver("store|model", aliases(flat, "store|model"))
        member = r"(?:\s*\??\.\s*(?P<name>%s)\b|\s*\??\.?\s*\[\s*['\"](?P<quoted>%s)['\"]\s*\])"
        for found in re.finditer(acts + member % (named, named), plain):
            name = found.group("name") or found.group("quoted")
            opened = re.compile(r"\s*\(").match(flat, found.end())
            if not opened:
                yield (f"{where(found.start())}: `actions.{name}` is taken off the actions and not "
                       "called there (a `.call`, a value handed on): call it in place, with its "
                       "`{ device }`, so the check can read it")
                continue
            end = span_end(flat, opened.end(), "")
            args = [flat[a:b].strip() for a, b in split_top(flat, opened.end(), end)]
            computer = name.startswith("autoEq")
            if not any(arg.startswith("{") and not NO_DEVICE.search(arg)
                       and (NAMES_DEVICE.search(arg) or (computer and NAMES_COMPUTER.search(arg)))
                       for arg in args):
                yield (f"{where(found.start())}: `actions.{name}` is called with no `{{ device }}`: "
                       "it would act on the shown device or on none, and a popup acts on the "
                       "device its page is about (the object is written in the call)")
        for found in re.finditer(acts + r"\s*\??\.?\s*\[(?!\s*['\"])", plain):
            yield (f"{where(found.start())}: an action is called by a computed name: name it, so the "
                   "check can read whether it is one that acts on a device")
        for found in re.finditer(store + member % (("|".join(STORE_ARGS),) * 2), plain):
            name = found.group("name") or found.group("quoted")
            opened = re.compile(r"\s*\(").match(flat, found.end())
            if not opened:
                if re.compile(r"\s*\??\.\s*(?:call|apply|bind)\b").match(flat, found.end()):
                    yield (f"{where(found.start())}: the store's `{name}` is called with no device "
                           "the check can read: call it in place, with the device's address")
                continue
            end = span_end(flat, opened.end(), "")
            given = [plain[a:b].strip() for a, b in split_top(flat, opened.end(), end)]
            given = given[:STORE_ARGS[name]]
            if len(given) < STORE_ARGS[name] or given[-1] in NOTHING:
                yield (f"{where(found.start())}: the store's `{name}` is called with no device: it "
                       "would read the shown device, and a popup reads the device its page is "
                       "about")
        walk = receiver("actions", aliases(flat, "actions"))
        for found in re.finditer(walk + r"\s*\??\.?\s*\[\s*['\"]walk['\"]\s*\]", plain):
            yield (f"{where(found.start())}: `actions.walk` is reached by a quoted name: write it "
                   "plain, so the check can read its calls")
        for found in re.finditer(walk + r"\s*\??\.\s*walk\b", plain):
            opened = re.compile(r"\s*\??\.\s*(?P<name>\w+)\s*\(").match(flat, found.end())
            if not opened:
                yield (f"{where(found.start())}: `actions.walk` is taken off the actions and its call "
                       "is not made there: call `actions.walk.<call>(...)` in place, with its "
                       "`{ job }`, so the check can read it")
                continue
            name = opened.group("name")
            if name not in WALK_ON_JOB:
                continue
            end = span_end(flat, opened.end(), "")
            args = [flat[a:b].strip() for a, b in split_top(flat, opened.end(), end)]
            if not any(arg.startswith("{") and NAMES_JOB.search(arg) and not NO_JOB.search(arg)
                       for arg in args):
                yield (f"{where(found.start())}: `actions.walk.{name}` is called with no `{{ job }}`: "
                       "it would act on no walk, and a popup names the walk it drives (the object "
                       "is written in the call)")
        for found in re.finditer(r"\{(?P<parts>[^{}]*)\}\s*=\s*[\w$.?]*\b(?P<of>actions|store|model)"
                                 r"(?:\s*\??\.\s*computer)?\s*[;\n)]", flat):
            taken = {part.split(":")[0].strip() for part in found.group("parts").split(",")}
            risky = taken & (set(actions) | {"walk"} if found.group("of") == "actions"
                             else set(STORE_ARGS))
            for name in sorted(risky):
                what = f"`actions.{name}`" if found.group("of") == "actions" else f"the store's `{name}`"
                yield (f"{where(found.start())}: {what} is taken apart from its object: call it "
                       "there, with the device, so the check can read it")
        for found in re.finditer(r"\.\s*page\(\)\s*\??\.\s*device\b", flat):
            yield (f"{where(found.start())}: the page's `device` is the shown device's model: read "
                   "the device the page is about (`store.device(url)`)")


def shown(path):
    """`path` as a message names it: from the repository's root when it lies under it."""
    try:
        return path.relative_to(ROOT)
    except ValueError:
        return path


def bare(text):
    """`text` with its comments blanked out, the line numbers kept."""
    return COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def problems(path):
    text = bare(path.read_text())
    for number, line in enumerate(text.splitlines(), 1):
        for found in NAMES.finditer(line):
            yield number, found.group(0)


def rules(text, media=""):
    """Every style rule of `text` (comments already blanked) as `(media, selector, body)`; a rule
    inside `@media` carries the query, any other at-rule's rules are read as its own."""
    at = 0
    while True:
        open_at = text.find("{", at)
        if open_at < 0:
            return
        # an at-rule that is one statement (`@layer name;`) ends before the next rule's selector
        head = " ".join(text[at:open_at].split(";")[-1].split())
        depth, close_at = 1, open_at + 1
        while depth and close_at < len(text):
            depth += {"{": 1, "}": -1}.get(text[close_at], 0)
            close_at += 1
        body = text[open_at + 1:close_at - 1]
        if head.startswith("@media"):
            query = head[len("@media"):].strip()
            yield from rules(body, f"{media} and {query}" if media else query)
        elif head.startswith("@"):
            yield from rules(body, media)
        else:
            yield media, head, body
        at = close_at


def tokens(body):
    """The `--frame-*` declarations of a rule's body, `name -> value`."""
    return {name: " ".join(value.split()) for name, value in DECLARED.findall(body)}


def colour_tokens(web):
    """The frame's colour tokens: those whose default in the frame's sheet holds a colour."""
    found = {}
    for media, selector, body in rules(bare((web / "core" / "frame.css").read_text())):
        if selector == ":root" and not media:
            found.update(tokens(body))
    return sorted(name for name, value in found.items() if COLOUR.search(value))


def listed_schemes():
    """`{theme id: [scheme, ...]}` as the server lists them."""
    text = LISTED.read_text()
    return {theme: re.findall(r'"(\w+)"', schemes) for theme, schemes in re.findall(
        r'\bid:\s*"([\w-]+)".*?\bschemes:\s*\[([^\]]*)\]', text, re.S)}


def unset(web, theme, schemes, colours):
    """What the theme `theme` leaves unset: one line for each token and place."""
    base = {}
    others = []
    for sheet in sorted((web / "themes" / theme).glob("*.css")):
        for media, selector, body in rules(bare(sheet.read_text())):
            for part in (one.strip() for one in selector.split(",")):
                found = ROOT_RULE.match(part)
                if not found or found.group("theme") not in (None, theme):
                    continue
                rest = found.group("rest")
                if not rest and not media:
                    base.update(tokens(body))
                elif (SCHEME_REST.match(rest) and (not media or SCHEME_MEDIA.match(media))) or (
                        not rest and SCHEME_MEDIA.match(media)):
                    others.append((sheet, f"{media} {part}".strip(), tokens(body)))
    where = f"bins/svolderd/web/themes/{theme}"
    for name in colours:
        if name not in base:
            yield f"{where}: {name} is not set in the theme's root block"
    for scheme in schemes[1:]:
        system = [o for o in others if f"prefers-color-scheme: {scheme}" in o[1]]
        chosen = [o for o in others if f'[data-scheme="{scheme}"]' in o[1]
                  and "prefers-color-scheme" not in o[1]]
        if not system or not chosen:
            yield (f"{where}: lists the scheme {scheme} but has no block for it under the "
                   "system's setting and under the person's choice")
    for sheet, place, have in others:
        for name in colours:
            if name in base and "var(" not in base[name] and name not in have:
                yield (f"{shown(sheet)}: {name} is not set in `{place}` "
                       f"(the root block's {base[name]} is one scheme's value)")


def phone_width(web):
    """What is wrong with the frame's phone width, one line each."""
    phone = web / "core" / "phone.js"
    frame = web / "core" / "frame.css"
    text = phone.read_text() if phone.exists() else ""
    found = re.search(r"export const PHONE_MAX_PX = (\d+);", text)
    if not found:
        yield f"{shown(phone)}: no `export const PHONE_MAX_PX = <number>;`"
        return
    want = f"max-width: {found.group(1)}px"
    widths = sorted({" ".join(feature.split())
                     for media, _selector, _body in rules(bare(frame.read_text()))
                     for feature in re.findall(r"\(([^()]*width[^()]*)\)", media)})
    if not widths:
        yield f"{shown(frame)}: no media query names a width (the phone layout)"
    for feature in widths:
        if feature != want:
            yield (f"{shown(frame)}: a media query turns at `{feature}`, the phone layout's is "
                   f"`{want}` (PHONE_MAX_PX)")


def findings(web, schemes):
    """`(lines, themes, colours)`: what rules 2 and 3 find wrong under `web`."""
    colours = colour_tokens(web)
    themes = sorted(path.name for path in (web / "themes").iterdir() if path.is_dir())
    lines = []
    if len(colours) < 15:
        lines.append(f"{shown(web / 'core' / 'frame.css')}: only {len(colours)} colour tokens "
                     "were read")
    for theme in themes:
        if theme not in schemes:
            lines.append(f"{shown(LISTED)}: no schemes are listed for the theme {theme}")
            continue
        lines.extend(unset(web, theme, schemes[theme], colours))
    lines.extend(phone_width(web))
    lines.extend(direct_imports(web))
    lines.extend(unlisted_keys(web))
    lines.extend(unbound_listeners(web))
    lines.extend(unnamed_devices(web))
    return lines, themes, colours


def edit(path, old, new):
    """Replace `old`, which is in the file once, with `new`."""
    text = path.read_text()
    if text.count(old) != 1:
        sys.exit(f"theme-css-check --self-test: `{old}` is in {shown(path)} {text.count(old)} "
                 "times, not once: bring the self-test in step with the sheets")
    path.write_text(text.replace(old, new))


def take(path, token):
    """Take `token`'s declaration out of the root block of the theme sheet at `path`, and give
    back its value."""
    text = path.read_text()
    found = re.search(r"\n\s*%s:\s*([^;]+);" % re.escape(token), text)
    if not found:
        sys.exit(f"theme-css-check --self-test: {token} is not in {shown(path)}")
    path.write_text(text[:found.start()] + text[found.end():])
    return found.group(1)


def self_test():
    """Each broken copy must fail with a line that names what was broken."""
    signal = pathlib.Path("themes") / "signal" / "style.css"
    paper = pathlib.Path("themes") / "paper" / "style.css"
    frame = pathlib.Path("core") / "frame.css"

    def moved(web, wrap):
        value = take(web / signal, "--frame-ok")
        with open(web / signal, "a") as out:
            out.write(wrap % f"--frame-ok: {value};")

    keys = pathlib.Path("themes") / "ember" / "keys.js"

    def imports(line):
        return lambda web: (web / keys).write_text(line + "\n" + (web / keys).read_text())

    signal_js = pathlib.Path("themes") / "signal" / "signal.js"
    paper_js = pathlib.Path("themes") / "paper" / "paper.js"

    sample = pathlib.Path("core") / "popups" / "sample" / "wire.js"

    def helper(line):
        """A file beside a popup's wire, holding `line`."""
        return lambda web: (web / "core" / "popups" / "sample" / "settings.js").write_text(line + "\n")

    def acts(line):
        return lambda web: (web / sample).write_text((web / sample).read_text() + "\n" + line + "\n")

    def listens(line, file=keys):
        return lambda web: (web / file).write_text((web / file).read_text() + "\n" + line + "\n")

    cases = {
        "the sheets as they are": (lambda web: None, None),
        "a popup's wire calls a device action with no device": (
            acts("core.actions.setControl('input', { value: 2 });"),
            "`actions.setControl` is called with no `{ device }`"),
        "a popup's wire calls a device action with no argument at all": (
            acts("core.actions.identify();"),
            "`actions.identify` is called with no `{ device }`"),
        "a popup's wire calls a preset action of the computer with no device": (
            acts("core.actions.computer.recallPreset('Flat');"),
            "`actions.recallPreset` is called with no `{ device }`"),
        "a popup's wire calls a device action that names its device": (
            acts("core.actions.setControl('input', { value: 2 }, { device });\n"
                 "core.actions.identify({ device: page });"), None),
        "a popup's wire previews for the computer": (
            acts("core.actions.autoEqPreview(variant, { computer: true, compare });\n"
                 "core.actions.autoEqImport(variant, { device, name });"), None),
        "a popup's wire previews and names no side": (
            acts("core.actions.autoEqPreview(variant, { compare });"),
            "`actions.autoEqPreview` is called with no `{ device }`"),
        "a popup's wire imports with the computer switched off and names no device": (
            acts("core.actions.autoEqImport(variant, { computer: false, name });"),
            "`actions.autoEqImport` is called with no `{ device }`"),
        "a popup's wire overwrites a preset and names no device": (
            acts("core.actions.overwritePreset(name);"),
            "`actions.overwritePreset` is called with no `{ device }`"),
        "a popup's wire sets a control for the computer, which only the EQ library may": (
            acts("core.actions.setControl('input', { value: 2 }, { computer: true });"),
            "`actions.setControl` is called with no `{ device }`"),
        "a popup's wire renames a device and names none": (
            acts("core.actions.renameDevice('Desk');"),
            "`actions.renameDevice` is called with no `{ device }`"),
        "a popup's wire sets a device's profile and names none": (
            acts("core.actions.setProfile('gungnir-2');"),
            "`actions.setProfile` is called with no `{ device }`"),
        "a popup's wire changes a device's passphrase and names none": (
            acts("core.actions.setPassphrase(typed);"),
            "`actions.setPassphrase` is called with no `{ device }`"),
        "a popup's wire makes a device the default and names none": (
            acts("core.actions.makeDefault();"),
            "`actions.makeDefault` is called with no `{ device }`"),
        "a popup's wire removes a device and names none": (
            acts("core.actions.removeDevice();"),
            "`actions.removeDevice` is called with no `{ device }`"),
        "a popup's wire calls the configuration's writes and names their device": (
            acts("core.actions.renameDevice('Desk', { device });\n"
                 "core.actions.setProfile(null, { device: url });\n"
                 "core.actions.setPassphrase(typed, { device });\n"
                 "core.actions.makeDefault({ device });\n"
                 "core.actions.removeDevice({ device: url });"), None),
        "a popup's wire reads a part of the shown device": (
            acts("const tone = store.part('tone');"),
            "the store's `part` is called with no device"),
        "a popup's wire reads the shown device": (
            acts("const held = core.model.device();"),
            "the store's `device` is called with no device"),
        "a popup's wire reads the model of the shown device again": (
            acts("store.refresh();"),
            "the store's `refresh` is called with no device"),
        "a helper beside a popup's wire calls a device action with no device": (
            helper("export const go = (core) => core.actions.identify();"),
            "settings.js:1: `actions.identify` is called with no `{ device }`"),
        "a popup's draw reads a part of the shown device": (
            lambda web: (web / "core" / "popups" / "sample" / "draw.js").write_text(
                (web / "core" / "popups" / "sample" / "draw.js").read_text()
                + "\nexport const tone = (store) => store.part('tone');\n"),
            "draw.js:"),
        "a popup's wire answers a walk and names no job": (
            acts("core.actions.walk.answer({ choice: 0 });"),
            "`actions.walk.answer` is called with no `{ job }`"),
        "a popup's wire steps a walk back and names no job": (
            acts("core.actions.walk.back();"),
            "`actions.walk.back` is called with no `{ job }`"),
        "a popup's wire cancels a walk with a job named as nothing": (
            acts("core.actions.walk.cancel({ job: undefined });"),
            "`actions.walk.cancel` is called with no `{ job }`"),
        "a popup's wire cancels a walk with a job named only inside another call": (
            acts("core.actions.walk.cancel(pick({ job }));"),
            "`actions.walk.cancel` is called with no `{ job }`"),
        "a popup's wire calls the walk's writes and names the job": (
            acts("core.actions.walk.start();\n"
                 "core.actions.walk.answer({ text }, { job });\n"
                 "core.actions.walk.back({ job: id });\n"
                 "core.actions.walk.cancel({ job });"), None),
        "a popup's wire takes the walk off the actions": (
            acts("const walk = core.actions.walk;\nwalk.cancel();"),
            "`actions.walk` is taken off the actions"),
        "a popup's wire takes the walk apart from the actions": (
            acts("const { walk } = core.actions;"),
            "`actions.walk` is taken apart from its object"),
        "a popup's wire reaches the walk by a quoted name": (
            acts("core.actions['walk'].cancel({ job });"),
            "`actions.walk` is reached by a quoted name"),
        "a device action by a quoted name": (
            acts("core.actions['identify']();"),
            "`actions.identify` is called with no `{ device }`"),
        "a device action under another name for the actions": (
            acts("const act = core.actions;\nact.setMode('eq');"),
            "`actions.setMode` is called with no `{ device }`"),
        "a store read under another name for the store": (
            acts("const held = core.model;\nconst tone = held.part('tone');"),
            "the store's `part` is called with no device"),
        "a device action through .call": (
            acts("core.actions.identify.call(null);"),
            "`actions.identify` is taken off the actions"),
        "a device action taken apart from the actions": (
            acts("const { identify, toast } = core.actions;"),
            "`actions.identify` is taken apart from its object"),
        "a store read taken apart from the store": (
            acts("const { part } = store;"),
            "the store's `part` is taken apart from its object"),
        "a device that is named as nothing": (
            acts("core.actions.identify({ device: undefined });"),
            "`actions.identify` is called with no `{ device }`"),
        "a store read with nothing for the device": (
            acts("const tone = store.part('tone', undefined);"),
            "the store's `part` is called with no device"),
        "a device named only inside another call": (
            acts("core.actions.setControl('input', pick({ device }));"),
            "`actions.setControl` is called with no `{ device }`"),
        "an action by a computed name": (
            acts("core.actions[name]({ device });"),
            "an action is called by a computed name"),
        "the shown device's model off the page": (
            acts("const held = store.page().device;"),
            "the page's `device` is the shown device's model"),
        "a popup's wire reads a part and a device by name, and asks which is shown": (
            acts("const tone = store.part('tone', device);\nconst held = core.model.device(device);\n"
                 "const on = store.shown();"), None),
        "a theme compares a key press with a key its table lacks": (
            listens("document.addEventListener('keydown', (event) => { "
                    "if (event.key === 'q') go(); });"),
            "the key `q` is compared with a key press"),
        "a key its table lacks, the literal first and under another name for the event": (
            listens("box.addEventListener('keydown', (e) => { if ('End' !== e.key) return; });"),
            "the key `End` is compared with a key press"),
        "a key its table lacks, in a switch over the key": (
            listens("function on(event) { switch (event.key) { case 'F2': go(); break; "
                    "default: } }", signal_js),
            "signal.js:"),
        "a key its table lacks, through a local": (
            listens("function on(event) { const key = event.key; if (key === 'z') go(); }",
                    signal_js),
            "the key `z` is compared with a key press"),
        "a key its table lists": (
            listens("box.addEventListener('keydown', (event) => { "
                    "if (event.key === 'm') go(); });"),
            None),
        "a key its table lists through `keys`": (
            listens("box.addEventListener('keydown', (event) => { "
                    "if (event.key === 'ArrowLeft') go(); });"),
            None),
        "a key its table lists as one keycap of a list": (
            listens("box.addEventListener('keydown', (event) => { "
                    "if (event.key === 'End') go(); });", signal_js),
            None),
        "a window listener for a key the table lists for a widget": (
            listens("window.addEventListener('keydown', (event) => { "
                    "if (event.key === 'Home') go(); });", paper_js),
            "listens for `keydown` on the window or the document"),
        "a document listener that takes the key apart in its parameter": (
            listens("document.addEventListener('keydown', ({ key }) => { "
                    "if (key === 'g') go(); });", signal_js),
            "listens for `keydown` on the window or the document"),
        "a window listener under another event and another name for it": (
            listens("window.addEventListener('keyup', (press) => { "
                    "if (press.key === 'g') go(); });", signal_js),
            "listens for `keyup` on the window or the document"),
        "a document listener that looks the key up in a list": (
            listens("document.addEventListener('keydown', (event) => { "
                    "if (['g', 'h'].includes(event.key)) go(); });", signal_js),
            "listens for `keydown` on the window or the document"),
        "a document listener that reads the key's code": (
            listens("document.addEventListener('keypress', (event) => { "
                    "if (event.code === 'KeyG') go(); });", signal_js),
            "listens for `keypress` on the window or the document"),
        "a window listener that looks the key up in a map": (
            listens("window.addEventListener('keydown', (event) => STEPS[event.key]?.());",
                    signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the document reached through the root": (
            listens("root.ownerDocument.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the document under a local name": (
            listens("const place = root.ownerDocument;\nplace.addEventListener('keydown', go);",
                    signal_js),
            "listens for `keydown` on the window or the document"),
        "a key event named in a list of events": (
            listens("for (const name of ['click', 'keydown']) window.addEventListener(name, go);",
                    signal_js),
            "listens for `keydown` on the window or the document"),
        "a key handler set as a property": (
            listens("document.onkeydown = go;", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on a receiver a function is handed, called with the window": (
            listens("function wire(target) {\n  target.addEventListener('keydown', (event) => "
                    "STEPS[event.key]?.());\n}\nwire(window);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on a local that falls back to the window": (
            listens("const view = root.ownerDocument.defaultView ?? window;\n"
                    "view.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on a local that is the document or something else": (
            listens("const place = root.closest('.x') || document;\n"
                    "place.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on a local a ternary sets to the document": (
            listens("const place = wide ? document : root;\n"
                    "place.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the document taken apart under another name": (
            listens("const { ownerDocument: place } = root;\n"
                    "place.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the window in brackets": (
            listens("(window).addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the root node of an element": (
            listens("root.getRootNode().addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the page's root element, looked up": (
            listens("document.querySelector('html').addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on what holds the document's body": (
            listens("document.body.parentNode.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the window held in an object": (
            listens("const places = { page: window };\n"
                    "places.page.addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on the window held in a member": (
            listens("function keep(me) { me.place = window; }\n"
                    "function on(me) { me.place.addEventListener('keydown', go); }",
                    signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on `this` in a function called on the window": (
            listens("function on() { this.addEventListener('keydown', go); }\non.call(window);",
                    signal_js),
            "listens for `keydown` on `this`"),
        "a listener on the window in a list": (
            listens("[window][0].addEventListener('keydown', go);", signal_js),
            "listens for `keydown` on the window or the document"),
        "a window listener whose event name is put together": (
            listens("window.addEventListener('key' + 'down', go);", signal_js),
            "listens for `(a computed name)` on the window or the document"),
        "a key handler set as an attribute": (
            listens("root.setAttribute('onkeydown', 'go()');", signal_js),
            "listens for `keydown` on the window or the document"),
        "a listener on a parameter of an exported function": (
            listens("export function wire(target) { target.addEventListener('keydown', go); }\n"
                    "wire(root);", signal_js),
            "listens for `keydown` on `target`, which `wire` is handed"),
        "a listener on a parameter of a function nothing names": (
            listens("for (const el of [root]) [el].forEach((one) => "
                    "one.addEventListener('keydown', go));", signal_js),
            "listens for `keydown` on `one`, which a function with no name is handed"),
        "a listener on a parameter of a function that is itself handed on": (
            listens("function wire(target) { target.addEventListener('keydown', go); }\n"
                    "wire(root);\n[window].forEach(wire);", signal_js),
            "listens for `keydown` on `target`, which `wire` is handed"),
        "a listener on a parameter taken apart": (
            listens("function wire({ target }) { target.addEventListener('keydown', go); }\n"
                    "wire({ target: root });", signal_js),
            "listens for `keydown` on `target`, which `wire` is handed"),
        "a listener on a parameter every call hands an element": (
            listens("function wire(target) { target.addEventListener('keydown', go); }\n"
                    "wire(root.querySelector('.box'));\nwire(root);", signal_js),
            None),
        "a listener on a local that is one element or another": (
            listens("const box = root.querySelector('.a') ?? root.querySelector('.b');\n"
                    "box.addEventListener('keydown', go);", signal_js),
            None),
        "a file whose listener is said to read no key reads one": (
            listens("const which = (event) => event.code;",
                    pathlib.Path("themes") / "nightdrive" / "dash-fade.js"),
            "dash-fade.js: LISTENERS says its listener reads no key"),
        "a listener HANDED allows is gone": (
            lambda web: edit(web / "themes" / "signal" / "strip-wire.js",
                             "band.addEventListener('keydown', () => { moved = false; }",
                             "band.addEventListener('click', () => { moved = false; }"),
            "strip-wire.js: HANDED allows a key listener on `band`"),
        "a second listener in a file LISTENERS allows one": (
            listens("document.addEventListener('keydown', go);",
                    pathlib.Path("themes") / "ember" / "arrange.js"),
            "LISTENERS in scripts/theme-css-check.py allows 1 here, 2 found"),
        "a listener LISTENERS allows is gone": (
            lambda web: edit(web / "themes" / "nightdrive" / "dash-fade.js",
                             "doc.addEventListener('keydown', () => { deaf = false; }",
                             "dash.addEventListener('keydown', () => { deaf = false; }"),
            "dash-fade.js: LISTENERS allows 1 key listeners"),
        "an entry that says own with no listener on the list": (
            lambda web: edit(web / "themes" / "paper" / "key-table.js",
                             "id: 'list-pick', key: 'Enter',",
                             "id: 'list-pick', own: true, key: 'Enter',"),
            "the entry `list-pick` says `own: true` and no line of LISTENERS"),
        "a listener whose entry no longer says own": (
            lambda web: edit(web / "themes" / "ember" / "key-table.js",
                             "words: 'Let go of the point where it is', own: true,",
                             "words: 'Let go of the point where it is',"),
            "LISTENERS names the entry `curve-end`, which does not say `own: true`"),
        "a key listener on an element": (
            listens("box.addEventListener('keydown', (event) => STEPS[event.key]?.());\n"
                    "document.querySelector('.box')?.addEventListener('keyup', go);", signal_js),
            None),
        "a key event the test driver makes": (
            listens("box.dispatchEvent(new KeyboardEvent('keydown', { key: 'g' }));", signal_js),
            None),
        "the Tab of a sheet's focus trap": (
            listens("box.addEventListener('keydown', (event) => { "
                    "if (event.key !== 'Tab') return; });"),
            None),
        "a control's key, which is no key press": (
            listens("const balance = (control) => control.key === 'balance';"), None),
        "a theme without a key table": (
            lambda web: (web / "themes" / "paper" / "key-table.js").unlink(),
            "paper: the theme has no key table"),
        "a token left out of a root block": (lambda web: take(web / signal, "--frame-ok"),
                                             "signal: --frame-ok is not set"),
        "a token set only for one state of the root": (
            lambda web: moved(web, '\n:root[data-theme="signal"][data-motion="calm"] { %s }\n'),
            "signal: --frame-ok is not set"),
        "a token set only in a media query": (
            lambda web: moved(
                web, '\n@media (min-width: 900px) { :root[data-theme="signal"] { %s } }\n'),
            "signal: --frame-ok is not set"),
        "a token set only for another theme's name": (
            lambda web: moved(web, '\n:root[data-theme="x"] { %s }\n'),
            "signal: --frame-ok is not set"),
        "a token left out of a second scheme's block": (
            lambda web: edit(web / paper, "\n    --frame-ok: #8fb0f5;", ""),
            "--frame-ok is not set in `(prefers-color-scheme: dark)"),
        "the frame's phone layout at another width": (
            lambda web: edit(web / frame, "@media (max-width: 719px)", "@media (max-width: 600px)"),
            "a media query turns at `max-width: 600px`"),
        "the frame's phone layout in another unit": (
            lambda web: edit(web / frame, "@media (max-width: 719px)",
                             "@media (max-width: 37.5em)"),
            "a media query turns at `max-width: 37.5em`"),
        "a second width in the frame's sheet": (
            lambda web: edit(web / frame, "@media (max-width: 719px)",
                             "@media (min-width: 400px) and (max-width: 719px)"),
            "a media query turns at `min-width: 400px`"),
        "a theme imports a service of the core": (
            imports("import { watchVisible } from '../../core/visible.js';"),
            "imports watchVisible of core/visible.js directly"),
        "a theme imports the one impure export of a module with pure ones": (
            imports("import { elapsedUs, tickAges } from '../../core/clock.js';"),
            "imports tickAges of core/clock.js directly"),
        "a theme imports a whole service module as a namespace": (
            imports("import * as api from '../../core/api.js';"),
            "imports * of core/api.js directly"),
        "a theme imports a service by a dynamic import": (
            imports("const m = import('../../core/menu.js');"),
            "imports * of core/menu.js directly"),
        "a theme imports a service by a dynamic import of a template literal": (
            imports("const m = import(`../../core/menu.js`);"),
            "imports * of core/menu.js directly"),
        "a theme imports a service by a dynamic import of a new URL": (
            imports("const m = import(new URL('assets/core/menu.js', document.baseURI));"),
            "imports * of core/menu.js directly"),
        "a theme imports a core module by a dynamic import of a computed path": (
            imports("const m = import('../../core/' + name + '.js');"),
            "imports * of core/(a computed path) directly"),
        "a theme imports a core module by a template literal with a computed file": (
            imports("const m = import(`../../core/${name}.js`);"),
            "imports * of core/(a computed path) directly"),
        "a theme imports a pure module by a dynamic import": (
            imports("const m = import('../../core/html.js');"), None),
        "a theme imports a service with a query string after the file": (
            imports("import { createActions } from '../../core/actions.js?x';"),
            "imports createActions of core/actions.js directly"),
        "a theme imports a service by a dynamic import with a query string": (
            imports("const m = import('../../core/menu.js?v=2');"),
            "imports * of core/menu.js directly"),
        "a theme re-exports a service": (
            imports("export { createActions } from '../../core/actions.js';"),
            "imports createActions of core/actions.js directly"),
    }
    schemes = listed_schemes()
    bad = 0
    for name, (breaks, wanted) in cases.items():
        scratch = tmp_base.mkdtemp("tcc")
        try:
            web = pathlib.Path(scratch) / "web"
            (web / "core").mkdir(parents=True)
            for file in ("frame.css", "phone.js"):
                shutil.copy(WEB / "core" / file, web / "core" / file)
            for sheet in (WEB / "themes").glob("*/*.css"):
                (web / "themes" / sheet.parent.name).mkdir(parents=True, exist_ok=True)
                shutil.copy(sheet, web / "themes" / sheet.parent.name / sheet.name)
            for script in [*(WEB / "themes").rglob("*.js"),
                           *(WEB / "core" / "popups").glob("*/**/*.js")]:
                copy = web / script.relative_to(WEB)
                copy.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(script, copy)
            breaks(web)
            lines = findings(web, schemes)[0]
        finally:
            shutil.rmtree(scratch, ignore_errors=True)
        fine = not lines if wanted is None else any(wanted in line for line in lines)
        if not fine:
            bad += 1
            print(f"theme-css-check --self-test: {name}: wanted "
                  f"{'no finding' if wanted is None else repr(wanted)}, got {lines or 'none'}")
    if bad:
        sys.exit(1)
    print(f"theme-css-check --self-test: {len(cases)} cases as expected")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    bad = 0
    for sheet in sorted((WEB / "themes").glob("*/*.css")):
        for number, name in problems(sheet):
            print(f"{sheet.relative_to(ROOT)}:{number}: names the frame's {name}; "
                  "dress it with the --frame-* tokens")
            bad += 1
    lines, themes, colours = findings(WEB, listed_schemes())
    for line in lines:
        print(line)
    bad += len(lines)
    if bad:
        sys.exit(1)
    print(f"theme-css-check: no theme sheet names the frame, {len(themes)} themes set the frame's "
          f"{len(colours)} colour tokens in every scheme, the frame's phone width is the core's, "
          "every theme file reaches the core's services only through `core`, every key a theme "
          "compares a key press with is in its key table, no theme listens for a key on the "
          f"window or the document outside its table's binding ({len(LISTENERS)} allowed, "
          f"{len(HANDED)} more on a receiver the check cannot follow), every device action and "
          f"store read in a popup's files names its device ({len(device_actions())} actions read "
          "from core/actions.js)")


if __name__ == "__main__":
    main()
