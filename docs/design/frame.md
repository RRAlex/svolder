<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The frame

The web page is a frame, the core and a theme. The frame is what the page draws over every theme
and never hands to one: the menu's button, its menu, the picker of themes, the toasts, and the
reserved keys `T` and `?`. A theme is drawn into `#theme`; the frame lives in `#frame`, first in
the page and above the theme by `z-index`. This page says what the frame does, how a theme is
chosen and loaded, and what a theme gives it. The code is `bins/svolderd/web/core/` (`frame.js`,
`menu.js`, `menu-rows.js`, `picker.js`, `modal.js`, `router.js`, `phone.js`, `switch.js`,
`frame.css`) and `bins/svolderd/src/web/themes.rs`. What the theme Ember
adds to the frame (its menu rows) is in [themes-and-effects.md](themes-and-effects.md); the page
around it is in [web-ui.md](web-ui.md).

## The button and its guard

The button is one node, drawn by the frame, dressed from the `--frame-*` tokens the theme sets on
`:root` (size, background, colour, border, radius; frame.css has defaults). A theme says where it
sits by anchoring it to an element (`core.frame.anchor(el)`: Ember's empty `.fxslot`, pinned to the
window's corner), and the frame places it over that element in page coordinates, again at a
resize, a font's arrival and `realign()`; with no anchor it sits at the top right, 16 px in.
`anchor(el, { fixed })` (a boolean, or a function asked at each placing) places it against the
window instead of the page, for a slot that is pinned at the top and never moves as the page
scrolls (every theme's slot is, so the button is 8 px from the window's top right corner in all
of them); the frame then carries the class `fixed`. A theme may ask it to fade (`fade(true)`):
after 2 s without a pointer move, touch or key it goes to opacity 0, and any of those brings it
back at once; it never fades while the menu or a modal is open.

A theme cannot hide, cover or move the button. Its displayed, visible, clickable, positioned and
opaque declarations are `!important` in a cascade layer declared before every other (`@layer
frame-guard` in frame.css): an important declaration in an earlier layer beats every important
declaration of a later layer or of none, whatever the selectors. `make theme-css-check` (part of
`make check`) fails when a theme's style sheet names `#frame`, `.fxbtn`, `.fxmenu`, `.fxpop`,
`#fx-button`, `#fx-menu` or `#picker`. `effects_menu.py` adds a more specific `!important` sheet and
finds the button shown.

## The menu

The rows are drawn when a theme is mounted and again each time the menu opens (the theme's optional
`menuOpening()` runs first, for work done only for a person who looks), and kept in step by
`realign()`. In order: the **Theme** row (`#menu-theme`, "Theme: Ember ›", with the theme's
picture from `preview(box)`), the rows the theme gives (`menuRows(core)` of its mounted instance:
headings, radios, checks, actions and rules, each with the fields `menu.js` describes), a rule,
the popups' rows (`#menu-popup-devices`, *Devices…*, for a viewer who may write, then
`#menu-popup-help`, *Help…*) and **Sign out**
(`#menu-sign-out`) or, for a guest, **Sign in** (`#menu-sign-in`, `./?sign-in`). The arrows reach
every enabled row round the ends, Home and End jump, Escape and `q` close, and the focus opens on
the checked radio. After a pick the focus goes back to the button.

Which rows there are and in what order is data, in `core/menu-rows.js` (`menuRows`, `frameRows`; no
page); `menu.js` draws and runs them. A popup's row is not written there: it is
part of the popup's entry in the page's list of popups (`core/popups/listed.js`: its words and
who sees it, everyone or a signed-in writer), and `frameRows` puts the rows of that list before
Sign out ([popups.md](popups.md#the-list-of-popups)). `node/core-menu-rows.mjs` pins the
order for each kind of viewer.

## The picker and the key `T`

The picker is a modal dialog with a card for each theme the page lists: a picture drawn by the theme
(160 by 104), its name and blurb, the one in use marked. `T` (or `t`, no Ctrl, Alt or Meta, not in
an input, a text area, a select or an editable element) toggles it, in the capture phase on the
window, before any theme sees the key. It is a modal of the core's stack (below): while it is open
the page under it is `inert` and the keys that move in it are its own; Escape, a click outside or a
pick closes it. From the open menu `T` opens it too, and the menu closes; under a sheet a theme
draws itself (Night drive's sheet, Signal's Add to the chain) the key is the sheet's and opens
nothing.

`?` is reserved under the same rule (alone or with Shift, not in a field, nothing of the theme's own
or of the stack over the page): it opens the Help popup through the router (`core.router.open`, the
key being the person's gesture), in every theme, and no key table may bind it. From the open menu it
closes the menu first. With Help open the key is the popup's, as every key is the top modal's, and
does nothing. Space, which plays and pauses the computer's player, is the frame's too. The three are
`RESERVED` in `core/keys.js`, and the frame's own list of them with their words is `core.keys.frame`
([key-table.md](key-table.md)); its `?` entry applies once the page has a Help popup.

## Modals

`core/modal.js` is one stack of dialogs, `core.modal`. The picker, the question (`core.ask`) and
every popup stand on it, so a popup brings no modal code of its own.

- `open({ el, labelledBy, focus, returnTo, onClose, onKey, stays })` puts `el` on top and returns
  `{ close(), opened() }` for it; `close()` closes the top; `closeAll()` closes every one, the top
  first; `opened()` says whether any is open. An `el` that is on the stack already stays where it
  is, with the options it was opened with.
- `el` becomes a child of the body with `role="dialog"`, `aria-modal="true"` and, with
  `labelledBy`, `aria-labelledby`, and leaves the page when it closes. An `el` that something else
  takes out of the page is off the stack the next time the stack is asked or a key is pressed.
- Every other child of the body but the toasts (`#toasts`) is `inert`, the modals under the top
  included.
- **The top owns the keyboard.** The frame's listener (capture phase, on the window) hands every
  keydown to the stack first and the key stops there: no other listener hears it, of a theme, of
  the frame or inside the modal, and `T` and `?` do nothing.
- **A part that lives inside a modal attaches no key listener of its own. It exports
  `key(event)`, and the modal's `onKey` hands it the key.** `onKey(event)` gets each key first and
  returns `true` for one it took: an Escape that shuts a list inside the modal, an Enter that
  sends its form, the arrows of its list. A modal with several parts asks each in turn
  (`onKey: (event) => list.key(event) || form.key(event)`).
- Of the keys `onKey` leaves, Escape closes the top and Tab goes round, also from outside the top:
  from its last control to its first, with Shift back, past one that is disabled or not rendered.
  A radio group is one stop, as it is for the browser: its checked radio, else the one Tab enters
  it at (the first radio; the last with Shift), so a group at the end of a popup does not hide the
  end from the stack.
- **The browser's own behaviour inside the top is never prevented by the stack**: a letter typed
  in a field, the caret's keys, Enter and Space on the focused button, Tab from one control of the
  top to the next, a native select. A key of a text composition (an input method) is the field's
  alone: `onKey`, Escape and Tab do not act on it.
- The focus opens on `focus`, else on the first control, else on `el`. At the close it goes to
  `returnTo` (an element, or an id looked up then, so a control drawn again meanwhile is found),
  else to what had it at the opening, or to its twin by id when it was drawn again. With no
  `returnTo` named and no opener that takes the focus (a row of a menu that has closed), it goes to
  the menu button; a modal opened with the focus nowhere leaves it nowhere. `onClose()` runs once,
  however the modal closed; one that throws is said in the console and stops nothing.
- `closeAll()` runs when another theme is switched to. When another device comes on show the core
  calls `closeAll({ device: true })`, which closes every modal but one opened with `stays: true`:
  a question about one device is answered No rather than left over another, while a popup that is
  about the devices and not about the one on show (Devices: `stays: true` in
  `core/popups/listed.js`) stands, with its page and its focus.
  A device the fleet drops from under the page moves it to the fleet's first device, unless the
  popup that renamed it said so with `core.stream.renamed(from, to)`: then the page moves to the
  renamed device.

`core.frame.covered()` says whether something stands over the page and owns its keys: a modal of
the stack, the open menu, or a sheet a theme draws itself and marks `aria-modal="true"` (Night
drive's sheet and Signal's Add to the chain sheet are such sheets: they stay the themes' own, inside
`#theme`, with their own Escape and Tab). A theme asks it before it acts on a key or a wheel of the
whole page; it reads no selector of the frame.

## Parts a popup shares with the themes

The core holds the parts a popup and a theme's page both need, so none is built twice. A theme
imports `html`, `inline-ask`, `list` and `act` directly and reaches the others through `core`
(`core.clock.tickAges`, `core.freeze`, `core.eq.table.editCell`): `PURE` in
`scripts/theme-css-check.py` is the list of what a theme may import, and every service of the
core is `core`'s. A part that draws takes the look as `classes` and the words as `words` (the
themes pass their own; the defaults are dressed by the `--frame-*` tokens in `frame.css`), and
every part follows the key rule above: it attaches no key listener, and one that has keys returns
`key(event)` and a `listen` helper for a page outside a modal.

- `core/html.js`: `esc(text)` makes a word safe in markup (none is empty) and `attr(name, value)`
  writes one attribute, quoted and escaped. Every string a draw builds goes through them.
- `core/inline-ask.js`: `askForm` (a name field with a save or rename word and cancel) and
  `askQuestion` (a sentence, a `yes` that asks once and a `no` that has the focus) draw in a place
  the caller keeps (`within`), one at a time in its scope (below). An answer that is refused
  leaves the ask open with what was typed. `between` is the markup between the two words and
  `scroll` brings the ask into view. Each ask returns `{ close, key, box }`: `key` takes Escape
  (the `no`), and `listen(ask)` gives a page's box its keys. The question is read by focus, not
  as an alert: it has an id of its own and both answers are described by it
  (`aria-describedby`), so a screen reader reads it with the answer the focus lands on. The
  name field carries `aria-invalid`: `false` as it opens (an empty required field is not said
  to be wrong before the person acted), `true` on its `invalid` event (a send with no name;
  the browser's own bubble still shows), and from the first typing on what the browser finds
  (`false` with a name in it, `true` emptied again). A theme's classes
  get the same attributes; the frame's own field (`.fi-input`) also takes the `--frame-danger`
  border while it is `:user-invalid`. The modal question `core.ask` is a
  different part: it is a modal.
- `core/list.js`: `openList` draws the options of a chip as a panel next to it, `place: 'below'`
  or `'auto'` (over the opener when it stands in the lower half of the box). `key` takes the
  arrows, Home, End and Escape; Enter and a tap choose through the browser's own click. One list
  is open at a time in its scope. In a popup the panel goes into the page pane, the popup's
  place for lists ([popups.md](popups.md)); a panel in the body would be inert under the stack,
  so `openList` throws for a list in a modal whose panel would land outside it. The panel is
  measured at its box's top left corner, not shown, and shown once it is placed, so measuring
  it never adds a scrollbar to the box.
- `core/gestures.js`, `core/place.js`, `core/volume-ask.js`: the hands every theme shares: dragging
  along a track and the slider keys; a mark's place as a fraction of its box (set through the
  style object, never a `style` attribute); the step the page asked the volume for, which stands
  for the reading until the device reports it (`ASK_MS`). Signal and Paper import all three; Night
  drive places its sheet itself; Ember's knob uses `volume-ask.js` and its `menuRows` is per mount.
- `core/clock.js`: the daemon's clock on the page. `daemonNow(now_ms, localMs)` is keyed by the
  daemon's `now_ms` value at this page's **first local sight** of it, never by the time of a draw:
  a redraw with no new player reading counts the track on from the same start in every theme, and
  a new `now_ms` re-bases it.
- `core/act.js`: `createAct(core, { now })` sends a write through the core's actions and logs the
  last fifty requests, which is how a page knows a change is its own (`MINE_MS`).
- `tickAges` (`core.clock`): the ticker of the ages on a device's page (the Devices popup calls
  it). It finds the section under the root it is given at every tick, writes each age and the
  signal's on the daemon's clock, runs only while the page is on show and stops with the `signal`
  it is given. It listens to no key.
- `core/freeze.js` (`core.freeze`): a section of a page that is under the person's hand is frozen,
  so a redraw leaves it as it is and what came meanwhile is drawn from the newest model when it is
  thawed. `keeper(live, name)` counts the freezes on one section (a gesture's apart from an open
  list's or form's); a popup that shows readings while a field is being typed in takes a freeze
  for the field's section, never a "hold" of its own, unless it puts its own places by id (the
  Devices popup keeps what is typed itself, around its puts: devices-popup.md).
  `createRelease(doc, keep, signal)` gives a freeze back once the press that closed the list is
  over; `createLive({ core, signal, view, sections, device })` draws sections from the store's
  events and keeps the frozen ones as they are. It follows the device its caller names: `device()`
  is asked at every event, a page passes the device it shows (`() => core.model.shown()`) and a
  popup the device it was opened for, so a popup about one device is not redrawn by, and does not
  let go for, the device on show. It never chooses the device itself. It listens to no key.
- `editCell` (`core.eq.table.editCell`) follows the rule too: `flow.key(event)` takes Enter (write
  the cell) and Escape (put the old text back); `flow.listen()` listens on the field for a page.

An inline ask and a list are open one at a time **per scope**. The scope is the modal of the stack
the ask's `within` (the list's opener) sits in, else the page. It is found by identity, not by
where the element stands (`modalOf` in `core/modal.js`): the nearest ancestor that is an element
the stack was given. `open` marks its `el`, and the router marks a popup's `el` when it is
registered (`claim(el)`), so the scope is the popup's before the stack has put it in the page
(what `show(page)` draws on opening) and after it has taken it out (`onClose`, `hide()`). An
element no stack was given is the page's: a theme's section that is not in the page, a sheet a
theme draws inside `#theme`. So:

- opening an ask or a list closes only the one open in the same scope: a question in a popup
  leaves a form on the page open with what was typed, and the other way round;
- a modal that leaves the stack, however it closed, closes the ask and the list inside it (their
  `onClose` runs, the focus is the modal's to give back);
- `closeAsk()` and `closeList()` close the page's, which is what a theme calls when the device
  switches or a stream event takes the place away; they never reach into a popup. A popup closes
  its own with `closeAsk(el)` or `closeList(el)`, `el` being its element or any element inside
  it, from anywhere: `show`, `onKey`, `onClose` or `hide()`.

Each module keeps one small map for this (scope to the handle open in it; an entry goes when its
ask or list closes) and nothing else.

## What a popup's frame dresses

`core/frame.css` dresses a popup's classes from the `--frame-*` tokens alone, so a theme sets no
colour of them. The box, side list, pane and shared pieces (`fp-box`, `fp-entry`, `fp-page`,
`fp-key`, `fp-dot`, `fp-tag`, `fp-heading`, `fp-row`, `fp-graph`) are in [popups.md](popups.md);
these are the ones around a page:

- `fp-head-tools`: the header's tools, between the title and the close button (a switch); drawn
  only when the popup's `head` is not empty.
- `fp-bar`: a strip under the header that does not scroll, ruled by `--frame-line`. It holds
  `fp-tabs` and, on a tab with pages, `fp-pills`.
- `fp-tabs`, `fp-tab`: `role="tab"` buttons in `--frame-dim`; the one shown (`.on`) is
  `--frame-ink` over a 3 px edge in `--frame-accent`. The pane is its tab panel (popups.md).
- `fp-pills`, `fp-pill`: a tab's pages, edged and worded in `--frame-dim`; the current one (`.on`)
  in `--frame-accent`. The focus ring of tabs, pills and switch is `--frame-accent`.
- `fpx-*`: the Crossfeed tab. The switch (`fpx-switch`, in the header tools: `--frame-dim` off,
  `--frame-accent` on), the menus and steppers (`fpx-menu`, `fpx-step`, edged in `--frame-dim`, a
  custom preset in `--frame-accent`) and the readings (`fpx-row`: `--frame-dim` label,
  `--frame-ink` value, `fpx-grey` in `--frame-faint`). The picture draws the same ear's level
  curve (`fpx-same`) in `--frame-ink` and the other ear's (`fpx-other`) in `--frame-accent`, over
  the grid (`--frame-graph-grid`), the leak's gap (`--frame-graph-gap`) and the crossover
  (`--frame-graph-target`); with crossfeed off (`fpx-off`) curves and dot are `--frame-faint`.

## The last focus

The frame records the element inside `#theme` that had the focus last (`focusin`, capture phase)
and gives it as `core.frame.lastFocus()`. The focus moving into the menu, onto its button or into
a modal does not overwrite it, so a popup opened from the menu still knows where the person was.
It is `null` before anything in the theme had the focus, once the element has left the page (a
redraw took it), while the element is hidden (`hidden` or `display: none` on it or on what holds
it: a control inside a sheet that has closed), after a theme switch, and once the focus left the
element for nothing (`focusout` with nowhere to go: a click on empty ground), so Help does not
show the keys of a widget the person has left. A window that goes behind another keeps it: the
element still has the focus.
`core.keys.zonesOf(core.frame.keyTable(), el)` turns it into the zones of the theme's key table
that hold it, the outermost first (a scale in a sheet: the sheet's zone, then the scale's), which
is how the Help popup knows which widgets' keys to show under Here; an empty list is the page
alone.

## The key table

Each theme has one key table: every key it answers to, as data, which its global keys are bound
from and the Help popup is drawn from. [key-table.md](key-table.md) has the entry's fields,
`core.keys` and the checks that hold a table to its page.

## The history

`core/router.js` (`core.router`) owns what an entry of the browser's history says: `{ device,
popup, page }`. A device shown pushes an entry (`./?device=<url>`), an opened popup pushes one
with the same address, and so does the first page entered from the popup's list; a page changed
from another page replaces its entry. Back therefore steps page → the popup's list → closed → the
device before, and:

- a step that changes only the popup part never reads or changes the shown device;
- a step to an entry that names another device shows that device, then the entry's popup;
- an entry with no device (not one of the router's) keeps the shown device;
- Escape and a popup's close button are `close()`: one step back to the entry the popup was opened
  from, whatever page it was on. Forward after that opens nothing (the entries ahead become plain
  ones as they are reached), while Forward after Back shows the popup again;
- a device shown while a popup is open closes the popup, and its entry takes the place of the
  popup's entries;
- under a popup that stays, a device shown is no entry at all, whoever shows it (*Show on the
  page*, a device the fleet dropped, `pushState`): `shown(device, address)` writes the device
  and its address into the entry the page is on and keeps its popup part. The popup's other
  entries and the entry it was opened from take that device as a step reaches them, so Back
  still steps page → list → closed and Escape lands on the device now shown, never on the one
  before. What was carried is kept in the tab's session store (`svolder-carried`, the newest 16
  runs), so the same holds after the page was loaded again in the same chain of loads; a page
  loaded on an entry that is no router's (a bookmark, a typed address) starts a new chain and
  takes no base of the chain before;
- a device shown with no push and no popup (the fleet dropped the shown one) names itself in the
  entry the page is on the same way, so the address never names a device that is gone.

A popup is a modal of the stack that the router puts there and takes away, so what the history
says and what stands on the page are one thing; [popups.md](popups.md) has a popup's files. It
registers once, `register(name, { el, labelledBy, focus, returnTo, onKey, show(page), hide() })`:
`el` and the next four are the stack's options for it (`focus` may be a function, asked once the
popup is in the page), `show(page)` draws it at a page (`null`: its list), and `hide()`, if
given, runs once it has left. On opening, `show(page)` runs before the stack puts `el` in the
page, so the control that takes the focus is drawn when the stack looks for it; on a step it
runs with `el` in the page. A popup whose code loads at its first opening registers `load()`,
`loading(page)` and `failed(page)` instead ([popups.md](popups.md#the-lazy-loader)). It is
opened, stepped and closed through `open(name, page)`, `step(page)` (`null`: back to its list)
and `close()`, which its close button calls. Escape and `closeAll()` reach `close()` by
themselves, and Back takes the popup off the stack, so the three leave the same history and a
popup closed any way opens again. A popup's page is the popup's own word and may name a device
that is not the shown one.

An entry that names a popup opens it only when this load of the page opened it: after the page is
loaded again, Back or Forward onto a popup's entry of the load before opens nothing and makes it a
plain entry. Chromium's Back button skips an entry that was pushed without a tap or a key of the
person's, so a popup is opened from one, and so is the step from its list to a page. Where the
browser says whether there was one (`navigator.userActivation`), the router says in the console
when it pushes a popup's entry without (`svolder: the popup <name> was opened with no tap or
key of the person's`); the push is made all the same, and `popups.py` fails on the words. The
router's `changed()` runs each time the entry the page is on has changed: the core makes the
tab's title again, which names the open popup and its page ([popups.md](popups.md)).

## The choice, loading and the fallback

The choice is per browser: `localStorage` and the cookie `svolder_theme` (`<id>` or
`<id>.light` / `<id>.dark`), the cookie so the server paints the first frame in the right theme.
The server's `chosen(headers, query)` picks `?theme=<id>` when it names a shipped theme, else the
cookie's theme, else Ember (the first); a scheme the theme lacks is dropped. The page then keeps a
valid `?theme=`, says `There is no theme “x”.` for one that names nothing (an empty one is ignored),
and makes the rendered theme the choice when the stored one differs.

A theme's files are its style sheets, its meta tags and its scripts. The server lists them for each
theme (`Theme` in `themes.rs`: `styles`, `metas`, `scripts`, one source for the first paint and the
page's list), renders the chosen theme's at first paint, and `switch.js` adds the rest on a switch:
the metas first (a script may read them as it loads), then the sheets and the scripts not already
in the page, each awaited, with a bound of 5 s. Whichever theme ends up mounted has its files in
place first, the fallback included; the other themes' sheets and metas go only after the new theme
is mounted. Scripts are plain files of the page's own origin (the policy has no inline script).
A switch unmounts the theme on show and mounts the new one; a failed load leaves the theme on show
with a toast. A theme that does not load, cannot run or throws while it mounts is replaced by
Ember with the toast `<Label> could not start here; showing Ember` and the choice set to Ember;
when Ember cannot start either, a toast that stays says so, with a way to reload.

## Light, dark and the browser's colours

A theme with both schemes says so (`schemes`), and the core keeps the person's word (Auto, Light or
Dark) per browser. The server paints the chosen scheme in the first byte: the cookie says
`<id>.dark`, the page opens with `data-scheme="dark"` and no flash of the other scheme can come
first. At boot the scheme the server painted is the choice, over whatever is stored; a switch
rewrites the choice from the theme it mounts, never from the one it leaves. The browser's own
colours follow the scheme: a theme names `theme_color` and, for a dark scheme, `theme_color_dark`;
the page carries one `theme-color` meta for each scheme (one, once a scheme is painted), and the
core's `paintChrome` sets `theme-color` and `color-scheme` again on every change of scheme. The
toast's shadow is a token, `--frame-toast-shadow`, so a dark theme sets its own; a test fails when
the frame's sheet names a colour outside a token.

## The tokens

A theme dresses the frame through these custom properties, set on `:root`; `core/frame.css` holds
a default for each.

| Token | What it dresses |
| --- | --- |
| `--frame-btn-size`, `--frame-btn-radius` | the menu button's size and corner |
| `--frame-btn-bg`, `--frame-btn-fg`, `--frame-btn-border` | the button at rest (the picker's close button too) |
| `--frame-btn-fg-hover`, `--frame-btn-border-hover` | the button under the pointer |
| `--frame-surface`, `--frame-hover` | the ground of the menu, the picker, a popup and a question; a row under the pointer |
| `--frame-ink`, `--frame-dim`, `--frame-faint` | words, lesser words (and headings), the least (disabled rows) |
| `--frame-line`, `--frame-line-strong` | a border, a stronger one |
| `--frame-accent`, `--frame-on-accent` | the chosen thing and the focus ring; words on the accent |
| `--frame-scrim` | the veil behind a sheet |
| `--frame-shadow`, `--frame-toast-shadow` | the menu's and the picker's shadow; a toast's |
| `--frame-radius`, `--frame-tap` | the menu's corner; the least height of a row under a finger |
| `--frame-font`, `--frame-mono` | the type, and the type of a key's name |
| `--frame-toast-lift` | how far above the page's bottom a toast stands |
| `--frame-field-bg`, `--frame-field-border` | an input field's ground and border |
| `--frame-danger`, `--frame-ok`, `--frame-warn` | a thing that removes; connected or fine; not connected or to mind |
| `--frame-graph-series`, `--frame-graph-grid`, `--frame-graph-target` | a graph's line, its grid and its target curve |
| `--frame-graph-gap`, `--frame-graph-compare` | the shaded gap between a fit and its target, and a second measurement's target curve |
| `--frame-offline` | the words of a device that is not there |
| `--frame-popup-in` | how a popup's box comes in: an animation, set inside `@media (prefers-reduced-motion: no-preference)`; no default, and no theme sets it ([popups.md](popups.md)) |

The colour tokens are those whose default holds a colour (the shadows count). Every theme sets each
of them in its root block, and a theme with two schemes sets again, in the block of the other scheme
under the system's setting and in the one under the person's choice, every token its root block
gives a value of its own (one that is only `var(...)` of the theme's own token follows that token).
`make theme-css-check` fails when a theme leaves one unset. The defaults of the four rows from
`--frame-field-bg` to `--frame-offline` are no theme's: they read on a light page and on a dark
one. The popups' shared classes (`fp-key`, `fp-dot`, `fp-tag`, `fp-heading`, `fp-row`,
`fp-graph`; [popups.md](popups.md)) are where `--frame-ok`, `--frame-warn`, `--frame-offline`
and the five `--frame-graph-*` tokens are used.

## The phone width

A window 719 px wide or narrower is a phone: `PHONE_MAX_PX` in `core/phone.js`. The frame's sheet
turns at it (a toast is centred from there down; `make theme-css-check` holds the number in
`frame.css` to the export), and Paper, Signal and Night drive import it for their phone layouts.
Ember's change of layout at 820 px is its own two-column desktop, not a phone's.

## What a theme gives the frame

`menuButton` (`{ icon: 'waves' | 'ink', label }`), `menuOpening()` (optional), `preview(box)`, the
`--frame-*` tokens in its style sheet, and, from the instance its `mount()` returns
(`core.mounted`), `menuRows(core)`, its key table, `keys`, and its names for the table's zones,
`zones` ([web-themes.md](web-themes.md#the-key-table)). The frame reads the mounted instance first
and the theme object when the instance has no such field
([web-themes.md](web-themes.md#what-a-theme-exports)). A theme never receives the button or the
menu, and it binds none of the reserved keys `T`, `?` and Space.
