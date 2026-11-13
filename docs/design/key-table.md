<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The key table

Each theme has one key table: every key it answers to, as data. The page's global keys are bound
from it and the Help popup is drawn from it, so a key that is listed is bound and a global key
that is bound is listed. The code is `core/keys.js`; a theme reaches it as `core.keys` and gives
its table to the frame as `keys` of its mounted instance (`core.frame.keyTable()` reads it).
[The frame](frame.md) says which keys the frame keeps and how a modal owns the keyboard.

An entry is `{ id, key, zone, words, when?(ctx), run?(event), own?, keys?, repeat?, gesture? }`.
This shape is the Help popup's contract: the popup draws any theme's table from these fields alone.

| Field | What it is |
| --- | --- |
| `id` | the entry's name in its table; no other entry of the table has it |
| `key` | what a person reads on the key: one keycap's name (`'m'`, `'Home'`, `'Page Up'`), or a list of keycaps for an entry several keys share (`['←', '→']`, `['Page Up', 'Page Down']`). One string is one keycap, never several: a keycap's name holds no space, but for the two-word keycaps `Page Up` and `Page Down` (`TWO_WORD_CAPS` in `core/keys.js`), so `'Home End'` is refused. `capsOf(entry)` gives the list either way |
| `zone` | `'page'`, or a selector naming the widget the key belongs to: never `body`, `html`, `:root` or `*`, which are the page, and no space around it |
| `words` | what the key does, in the theme's words: sentence case, no full stop |
| `when(ctx)` | whether the entry applies now (the viewer's rights, what the page shows); always, without one |
| `run(event)` | with one, the entry is a global key of the page and `bind` calls it; its zone is `'page'`. Without one, the entry documents a key its widget handles itself |
| `own` | `true` for a key that a listener of the theme's own on the window or the document answers, because its rule is not `bind`'s (Ember's Escape that leaves Arrange mode, its Escape that ends a drag on a curve). `bind` does not dispatch it; it is listed like any other |
| `keys` | the `event.key` values the entry answers to when they are not its keycaps: `['ArrowLeft', 'ArrowRight']` for `['←', '→']`, `['A', 'a']` for `'A'`; one at least |
| `repeat` | `true` for a key that acts again while it is held; any other acts once a press |
| `gesture` | `true` for an entry that is no key (Ember's `curve`: what the pointer does on the curve). `key` is then its name, drawn as plain text and not as a keycap (any words); it has no `run` and no `keys`, in the zone `'page'` too |

A theme may add fields of its own, which are private to it and which the popup does not read.
Beside its table a theme gives its names for the table's zones, `ZONES` of its `key-table.js`
(`{ '<zone>': 'The volume dial' }`), as `zones` of its mounted instance
(`core.frame.zoneNames()` reads it): one name a zone, a person's words and no selector, and the
same name for two zones that can sit on one element (a slider, and a slider with a centre).

- `core.keys.bind(table, { signal, ctx })` binds the `run` entries until `signal` aborts; a theme
  passes its mount's signal, so its keys go with it and two themes never hear each other's.
  `ctx()` gives the theme's own facts for `when` (Ember: whether the screen has a visualizer
  card), laid over the core's (`{ canWrite }`); it is asked at each key.
- A bound key reaches its `run` only when nothing else owns it. The frame's listener comes first
  and stops every key while a modal is open, so a key does nothing on the page behind one. A key
  with Ctrl, Alt or Meta, a key in a field that takes typing (an input, a text area, a select, an
  editable element) and a key the menu owns are left alone. A widget that stops the key it takes
  keeps it (Ember's dial and its arrows). The first entry that names the key and applies takes
  it; a `run` that acts on a device names none, so the action's default target, the device on
  show, is used, as in the theme's other handlers. `bind` does not ask whether a sheet the theme
  draws itself stands over the page: a `run` that must not act under one asks
  `core.frame.covered()` (Night drive's arrows do).
- A table that binds a key the frame reserves (`RESERVED` in `core/keys.js`), or holds an entry
  the core cannot read (no `id`, or one another entry has; a `key` that is neither a keycap's
  name nor a list of them, or that packs several keycaps into one string; no `zone` or `words`,
  or a widget's zone that is the whole page; an empty `keys`; a `run` on a widget's zone or on
  a gesture; a `run` on a key that could never fire, its keycap being no `event.key` and no
  `keys` saying which is), is refused when it is bound, with an error that names the entry.
- **A key of the whole page has a `run` or says `own: true`**: an entry of zone `'page'` with
  neither would be listed and bound by nothing, and `bind` refuses it. An `own` entry may be
  of any zone (Ember's two are a widget's): such a key acts on the whole page while its `when`
  holds, so Help lists it wherever the focus is.
- **`core.keys.listed(table, ctx)` is what the Help popup draws**: the entries that apply now,
  the very objects of the table, in table order. Without `ctx` it uses the facts the table was
  bound with (the core's alone for a table that is not bound). Help reads `key` (through
  `capsOf`), `gesture`, `words` and `zone` of each: the entries of zone `'page'` are the page's
  keys everywhere, the others belong to the widget their selector names. An entry whose `when`
  is false is neither bound nor listed.
- **Help shows what applies now, not all a viewer may ever have.** Here and Everywhere are
  drawn from `listed(table)` with the page's facts as they stand when Help opens: Ember's
  Arrange keys are listed in Arrange mode alone, its Escape of a drag while a point is held, `v`
  where the screen has a visualizer card, a
  key that writes for a viewer who may write. A key that a viewer may not use is silent and
  unlisted (Night drive's `↑` `↓` for a guest, as Ember's arrows), never listed with a refusal.
  **Here** is the zones that hold `core.frame.lastFocus()`, then the `own` keys of other
  widgets, then the page's keys with the frame's; **Everywhere** is that last group. A focus
  that left for nothing (a click on empty ground) is no last focus
  ([frame.md](frame.md#the-last-focus)): Here then shows the page's keys and says that nothing
  with keys of its own has the focus.
- `core.keys.zonesOf(table, el)` is every zone that holds `el` (its selector matches `el` or an
  ancestor), each once, the outermost first; `core.keys.zoneOf(table, el)` is the innermost of
  them, else `'page'`. The entries of those zones are the keys that act where `el` is. Both
  read the table they are given: Help's Here calls `zonesOf(listed(table), el)`, since the whole
  table also names a zone whose entries do not apply now (a guest on a scale).
- `core.keys.frame` is the frame's own keys as a table (`T`, Space, `?`) with their words, which
  Paper's footer names, every one; `FRAME_KEYS` is the same list for a theme that draws it without
  the core. It is never bound: the frame's listener answers its keys. Space applies for a viewer who
  may write and `?` on a page that has a Help popup (`ctx.help`, one of the core's facts), so
  neither is listed where it does nothing. `core.keys` itself is a plain object (`frame`, `bind`,
  `listed`, `zoneOf`, `zonesOf`). `listed`, `zonesOf`, `zoneOf`, `capsOf`, `keysOf`, `FRAME_KEYS`
  and `RESERVED` are pure and a theme may import them.

Three checks hold a table to its page:

- `make theme-css-check`, rule 5, reads the themes' scripts. A key that a script compares a key
  press with is in the theme's table. No theme file listens for `keydown`, `keyup` or `keypress`
  on the window or the document, however its handler reads the key: a key of the whole page is a
  `run` entry. The list `LISTENERS` in `scripts/theme-css-check.py` allows such a listener a
  line: its file, the entry it answers for if it reads a key, and the reason; every `own` entry
  has a line, so `own` is no way around the binding. The rule is a pattern over one file. It
  takes a receiver for the window or the document when it holds a global name (`window`, `doc`,
  `ownerDocument`, `getRootNode()`, ...), a name the file binds to an expression that holds
  one (`x ?? window`, a ternary, an object), or a parameter a call in the file hands one. A key
  listener on a parameter it cannot follow (the function is exported, has no name or takes the
  parameter apart) fails unless the list `HANDED` gives it a line. It does not see a global
  that comes from another file under a name of its own (an import, a member of an object built
  elsewhere, what a function returns), nor a listener added by other means (`Reflect.apply`,
  an event name put together elsewhere); a `HANDED` reason is read by people, not checked.
  The script's header has the whole list. `keys.py` counts the listeners in the browser.
- `scripts/web-checks/keys.py` presses, in each theme: every key of the page that applies (its `run`
  is called once), every `own` key of the page (the page answers; no theme has one now), keys with
  no entry (nothing happens) and the keys of an entry whose `when` is false (not listed, nothing
  happens); and it finds an element for every widget's zone, on the page or in a state of it. It
  also counts: `EventTarget.prototype.addEventListener` is wrapped before the page's scripts run
  (every way of adding a listener to the window or the document ends in it), each key listener on
  those two is filed under the script that added it while it is on, and through a theme's whole run
  no file of the theme has more of them on at once than `LISTENERS` gives it lines, `core/keys.js`
  has the binding's one (none for a table with no `run`) and none is on whose adder is no script of
  the page (added from a timer or through `bind`). A listener the pattern cannot see fails there; a
  handler property (`window.onkeydown`) is the pattern's. The count does not see a listener added
  with another realm's method (an iframe's `addEventListener` called on the window) nor one on
  `document.body` or the root element, which hear the page's keys too: only the window and the
  document are counted.
- `node/theme-keys.mjs` pins the four tables' keycaps, zones and words, and `node/core-keys.mjs`
  what `bind` dispatches and refuses.

Two more hold the Help popup to the tables. `node/core-help.mjs` draws Here and Everywhere
from each theme's real table, for a writer and a guest, with the focus in each zone in turn:
every row is an entry `listed` gives, every listed entry of a zone Help names is a row, and
every zone has a name. `keys.py` opens Help in each theme, on the page as it opens (with `?`)
and in every state it enters, and reads the rows against `core.keys.listed` in the page.
