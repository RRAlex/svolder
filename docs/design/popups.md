<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Popups

A popup is a dialog of the shared core over whatever theme is on show: it stands on the core's
stack of modals and the router puts it there, both described in [the frame](frame.md) ("Modals",
"The history", "Parts a popup shares with the themes"). This page says how a popup is laid out in
files and how it is checked. The code is `bins/svolderd/web/core/popups/` and the `fp-*` classes
of `core/frame.css`.

The expensive direction for checks is themes times browsers times popups. So a popup's content
is a pure string draw checked in node, its behaviour is the stack plus a thin wire, also checked
in node, and only what no node harness can see goes into one browser check that loops over the
four themes. There is no popup check per theme.

## Why not `<dialog>`

A popup is an element on the core's own stack (`core/modal.js`), with `role="dialog"` and
`aria-modal="true"`, and not a native `<dialog>` shown with `showModal()`:

- Android's Back on a native modal goes to the dialog's close watcher first: the popup closes
  before any `popstate`, the router's entry is left behind, and `cancel` cannot be prevented
  without a fresh tap. Back must step page → list → closed.
- A native modal does not stop the window's key listeners, so `T` and `?` would still act under
  it: the stack's capture listener is needed either way.
- The focus returns to the twin by id of a control a theme drew again meanwhile, which a native
  dialog does not do.
- One stack serves the picker, the question and every popup, and its node and browser checks
  prove it in both browsers.

## A popup's files

A popup is three parts, under `core/popups/`:

- `popups/<name>/draw.js`: the model in, a string out. Pure: no page, no core. Every word goes
  through `esc` and `attr` of `core/html.js`, and the markup is dressed only by the `fp-*` and
  `fi-*` classes of `frame.css`, which read the `--frame-*` tokens: a popup has no colour of its
  own. Its cases are strings in a node harness.
- `popups/<name>/wire.js`: thin. `wire({ core, frame })` returns the popup's part for the router,
  `{ show(page), hide(), onKey(event) }`. `show` draws into the frame; `onKey` is where the keys
  come from (the key rule: it attaches no key listener, and hands the key to the parts inside it,
  `(event) => list?.key(event) === true || ask?.key(event) === true`); the stream wants are the
  popup's own
  (`core.stream.want('popup:<name>', …)` on the first `show`, released in `hide()`), and a live
  line names the device it follows, which is the device the popup was opened for.
- `popups/frame.js`: what every popup shares. `popupHtml({ name, title, entries, page, body, state
  })` is the box as a string: the head (the title, whose id `popup-<name>-title` is the modal's
  `labelledBy`, and the close button), the side list of `entries` with the current one marked, and
  the page pane holding `body`. `createPopupFrame({ document, router, name, title, entries })`
  gives `{ el, labelledBy, focus(), state(), titleWords(page), draw({ title, entries, page, body
  }), put(id, html), list(entries), loading(page), failed(page) }`: the close button and a click
  on the veil are the router's `close()`, an entry is `step(id)`, the way back is `step(null)`.
  `draw` draws the whole box and `put` one place in it by its id (a live line), so a reading
  never draws over an open ask; a `put` of what the place holds already changes nothing, so a
  control in it keeps its element and the focus (a control in a place that is put has an id).
  `list(entries)` draws the side list alone: the pane stays, with what is typed, asked or
  scrolled there. A draw may name a `title` (a popup whose title names what it is about) and
  `entries`; each stands from then on, a draw that names neither keeps both, and so do the loading
  and failed lines. `draw`, `loading` and `failed` draw the whole box again, so they first close the
  inline ask and the list that are open in the popup: a wire closes neither before it draws. `draw`
  and `put` keep the focus: it goes to the element that has the same id after the redraw; when there
  is none, or it takes no focus because it is not rendered, to the first of these that is: the
  current entry of the side list, the way back from a page, the close button, the popup's element.
  So it is never on the page under the popup. The focus opens on the first of the same list:
  `focus()` is a function, which the stack asks once the popup is in the page. A page may hold no
  control at all (a list of keys): while the page pane holds more than it shows and no control that
  is shown, the frame makes it a stop of Tab (`tabindex="0"`) and a region named after the entry
  whose page it holds, else after the title, so the keyboard scrolls it. A pane that shows
  everything, or that has a control (the keyboard scrolls from it), is no stop. The frame looks
  after each draw and `put` and when its box changes size; a page that grows by other means calls
  `put` or `draw`.

An entry of the side list is `{ id, label, dot?, dotWords?, tag?, group? }`. Beside its label
it may carry a state dot (`dot`: `'on'`, `'warn'` or `'off'`; `dotWords` are read by a screen
reader when the label does not say the state), a tag after the label (`tag`: its words, as
`Shown`) and a break before it (`group: true`: the entry starts a group). The head holds the
title and the close button and nothing else: it is no place for lists, so a chip that opens one
stands in the page pane.

What every popup draws has one class each in `frame.css`, dressed by the `--frame-*` tokens
alone, and a pure draw in `popups/frame.js`; a popup adds classes only under its own prefix.

| Draw | Class | What it is |
| --- | --- | --- |
| `keycap(name)` | `fp-key` | a key's name as a keycap |
| `dot(state, words)` | `fp-dot` with `on`, `warn` or `off` | a state dot: full for on and warn, a ring for off |
| `tag(words)` | `fp-tag` | one or two words in a pill |
| `heading(words)` | `fp-heading` | the heading of a section of a page |
| `rows(list)` | `fp-rows` holding `fp-row`, `fp-row-label`, `fp-row-value` | labels and their values as one description list (`<dl>`): each `{ label, value, attrs? }`, both markup; `fp-rows` has no look of its own |
| (markup of the popup's own) | `fp-graph`; `fp-graph-grid`, `fp-graph-series`, `fp-graph-target` | a graph's box and the three strokes a graph is drawn with |

A click on the veil closes the popup only when the press and the release were both on it: a
drag that starts in the box (text selected in a field) and ends on the veil leaves the popup and
what was typed. The veil is a scroll container that scrolls nothing (`overflow: hidden` with
`overscroll-behavior: contain`), so a wheel or a swipe over it does not scroll the page under
it, in the browsers that keep the rule on a box that scrolls nothing (Chromium from 144,
Firefox from 150). The oldest browsers the page is built for (Chromium 142, Firefox 140) do not:
there the page under the veil still scrolls, and nothing else differs. `html` and `body` keep
their overflow. While a popup stands, the browser tab's title names
it after the device's words, with the page's label at a page (`Bifrost 3 · connected ·
Devices · Kara — svolder`): a `core.title` part that reads the router and the frame's
`titleWords(page)`, so the entries the history holds for a popup and its page read apart in the
browser's list, and the words go however the popup closes.

How the box comes in is a theme's to say: `.fp-box` takes `animation: var(--frame-popup-in,
none)`, and no theme sets the token, so the box stands at once. A theme sets it in its own
sheet, to an animation of its own `@keyframes`, **inside `@media (prefers-reduced-motion:
no-preference)`**:

```css
@media (prefers-reduced-motion: no-preference) {
  :root { --frame-popup-in: my-theme-popup-in .16s ease-out; }
}
```

The box is one element for as long as the popup stands: a draw writes its classes, its state
and what it holds again and keeps the element. So the animation runs once each time the popup
enters the page, the first open included, where the loading line is drawn first and the
popup's module arrives a moment later; a step or a redraw neither cuts it nor starts it again.
There is no exit animation: the stack takes the element out at once.

The box says where the router is: `fp-at-list` at the popup's list (page `null`) and `fp-at-page`
at a page. A wide window shows the side list and the page side by side either way; at the list
the pane holds the first entry's page. At `PHONE_MAX_PX` and below the popup fills the screen and
shows one of the two: the list at the list, the page at a page, with a button at the top of the
page that goes back to the list (`step(null)`, as Back does). There the focus follows the pane:
a step from the list to a page puts it on the way back (the entry is no longer shown), and the
way back puts it on the list's current entry. The turn is the sheet's media query alone; no
script reads the width, and the frame asks the browser whether a control is rendered. In a wide
window a click on the entry whose page the pane already shows (the first entry, at the list)
steps nowhere and pushes no entry of the history; on a phone it opens that page.

A list in a popup (`openList` of `core/list.js`) needs no `within`: the page pane is the popup's
place for lists (`data-list-place`), and the panel of a list opened from a control in the pane
goes into it. The pane is positioned and scrolls, so the panel is placed in what the pane
scrolls and goes with its chip. It opens where the pane shows it whole: under the chip, else
over it, else against the bottom of what the pane shows; the sheet keeps it no taller than the
pane, and a longer list scrolls in its panel. The panel is measured at the pane's top left
corner, not shown, and shown once it is placed: measured where the flow would put it, under the
pane's bottom, it would give a pane that did not scroll a scrollbar for that one layout. The
wire hands the list its keys from `onKey`,
before its asks, so Escape closes the list alone.

`core/popups/sample/` is the sample popup that fills this skeleton once: two entries, a paragraph,
a question and a form of `core/inline-ask.js`, a chip that opens a list, and a live line. The
page never registers it, so
no menu row, key or list reaches it and `core.router.open('sample')` opens nothing; a check
adds it to the popups of the core it holds (`core.popups.add`). `node/core-popups.mjs` holds
the draws and the wire. The sample is not in the binary: the daemon serves it only from a
`SVOLDER_WEB_ASSETS` directory, the browser checks' default.

## The list of popups

`core/popups/listed.js` is the one place a popup of the page is added: `POPUPS`, a list of `{
name, title, entries?, stays?, load, row: { label, sees } }`. `stays: true` is a popup that is
about the devices and not about the one on show: it stands through a change of the shown device,
where every other popup closes ([frame.md](frame.md#modals); Devices is the one such popup,
[devices-popup.md](devices-popup.md)). An entry is the popup and its row in the
frame's menu together: `row.label` is the row's words and `row.sees` who has the popup,
`'everyone'` or `'writer'` (a viewer who may write: no guest, and not on a page whose writes
are off; the core's `mayWrite`). Devices is `'everyone'`: it reads `mayWrite` itself and draws
read-only for a reader. `createCore` adds every entry
to `core.popups` (a list of its own may be handed in as `popups`, which the node harnesses do),
`core.popups.listed()` gives the entries back, and `frameRows` of `core/menu-rows.js` reads the
menu's popup rows from them, in the list's order, before Sign out (`#menu-popup-<name>`;
a tap opens the popup through the router). So a row cannot stand without its popup: no row is
written in `menu-rows.js`, and an entry with no `row`, no `load` or a name listed twice is
refused as the core is made. The sample popup is not in the list. `listed()` is the list as
the core was made with it: a copy, frozen with each entry and its `row`. `HELP` of `listed.js`
is the Help popup's name and the id of the frame's `?` entry. An entry's `title` and `entries`
are written in `listed.js`, never imported from the popup's folder, which loads at the first
opening.

**`sees` is the popup's, not only the row's.** Every entry is added whoever the viewer is
(nothing loads before an open), and the gate is one place in the core: `core.popups` registers
a listed popup with the router with `allowed()`, which asks `seenBy(entry, viewer)`, and the
router asks it at every `open` and at every step of the history. A viewer who does not see the
row therefore opens the popup by no path: a key, a theme's call of `core.router.open`, Back or
Forward onto an entry that names it. Nothing is pushed, drawn or loaded. An open says what any
write of that viewer's says (`core.actions.refuse()`: the toast `Sign in to change this` with
the way to sign in, for a guest; the page's read-only words where writes are off); a step of
the history makes the entry a plain one and says nothing. An entry of an earlier load of the
page opens nothing in any case, so an address or a reload opens no popup. A viewer who loses
the right while such a popup stands (the page's model is replaced with another viewer; a
sign-out loads the page again) loses the popup: it closes. A popup added by hand
(`core.popups.add`, the checks' sample) has no such gate.

## The lazy loader

A popup's files load when a person first opens it, by `import()`, so a page whose viewer opens
none loads none, and a guest's page never loads a writer's popup: it cannot open one ("The
list of popups"). `core.popups` holds them:

- `core.popups.add(name, { title, load, entries, stays })` makes the popup's frame and registers
  it with the router (`stays`: it stands through a change of the shown device); `load` is `() =>
  import('./<name>/wire.js')`. `core.popups.names()` lists what was added, and
  `core.popups.listed()` the entries of the page's list. Nothing is fetched by `add`. `entries` is
  optional: a popup whose side list is the same for everyone gives it here, and the loading line
  then stands beside the list, with the focus on its current entry; without it the box has one pane
  until the module arrives.
- The router's `register` takes `load()`, `loading(page)` and `failed(page)` in place of `show`,
  `hide` and `onKey`. On opening, `loading(page)` draws the frame with its loading line before
  the stack puts the element in the page (the same order as a popup's `show`), so the popup is on
  the stack and in the history at once, with the focus on its close button (on the current
  entry, when the entries came with `add`).
- When the module arrives, its `wire({ core, frame })` gives the part and the part is shown at
  the page the router is on then, if the popup still stands; a focus that waited on the close
  button moves to the current entry. A popup opened and closed before that draws nothing and
  takes no stream want; opened again meanwhile, it waits for the same import. `load` runs once.
- A module that does not come or has no `wire` leaves the frame's failed line (`role="alert"`).
  The stack and the history are as for any popup: Escape, Back and the close button close it,
  and the next open asks for the module again.
- A part whose `show(page)` throws, at the first show or any later open or step, throws nothing
  on: the router tells it to `hide()` (so a want it took is given back), the frame shows the
  failed line, and the popup stands and closes as any other. The part is kept and its wire is
  made once: the next open or step calls `show` again.
- Every file under `core/popups/` is in `ASSETS` (`bins/svolderd/src/web/assets.rs`): a file
  that is not is a 404 in a release build, found only when the popup is opened. The assets test
  `every_popup_file_on_disk_is_served` reads the folder from disk and fails on one.

## What is checked where

In node, with no browser: every draw, as strings (`node/core-popups.mjs`), and the wire on the
real core, stack and router over a jsdom page (`node/dom.mjs`): what opening draws, the focus
after a redraw, the stream want taken and released, Escape inside an inline ask and then on the
popup, Back, that none of them shows a device, and the loader (once for two opens, closed
before it resolves, rejected). jsdom gives a real focus, keys that travel to the window, markup
parsed as a browser parses it and a history; it lays nothing out, knows no `inert` and runs no
media query, so those are the browser's to prove.

In a browser, one check for every popup and theme: `scripts/web-checks/popups.py` loops over the
four themes and over the rows of its `POPUPS` table (the sample, Devices, the EQ library and Help),
at 1280 px and at 390 px, in Chromium and Firefox. It proves what node cannot see: nothing under the
popup's folder is requested before the first open; the box is on top at its centre and the veil at
the window's corners (`elementFromPoint`, over Night drive's canvas and Ember's smoke); the page
behind is inert; Tab stays inside, past a control the layout does not show; the list and the page
stand side by side; the colours are the theme's tokens in each scheme; nothing scrolls sideways; a
list's panel stands inside the box, on top, and goes with its chip when the pane is scrolled; Escape
closes an inline ask and then the popup, with the focus back where it was; Back closes it; a press
in a field that is let go on the veil keeps the popup and a click on the veil closes it; the veil
contains its overscroll (in Chromium a wheel over it leaves the page where it was; Firefox keeps the
rule in its compositor, which a WebDriver wheel does not pass through, so there the sheet's values
are read); the box opens with no animation; on a phone the box is the window and shows the list or
the page. A popup is a row there (`title`, `folder`, `add`, `entries`, `ask`, `form`, `list`,
`extra`), never a loop or a check of its own. The check opens a popup by a real click and steps by
one, and fails when the router says in the console that a popup's entry was pushed with no tap or
key ([frame.md](frame.md#the-history)); at its end an open by script, with no tap behind it, is
said. Help's row has cases of its own (its `extra`): the menu's row and `?` by real clicks and keys,
for a writer and a guest, the tab's title, Back from a page to the list to closed, a page with no
control scrolled by the keyboard, and a click on empty ground. The stack's own rules (who owns a
key, the steps of the history, a device or theme switch) stay in `modal.py`, on its probe modal and
popup.

## Help

`core/popups/help/` is the popup a person reaches from the menu's last popup row, *Help…*, for
everyone, and the key `?`, which the frame's listener answers by `core.router.open(HELP)`
([frame.md](frame.md#the-picker-and-the-key-t)). It has three pages and opens on the first.

- **Here** is the keys that act where the person was. At each `show`, never when the popup is
  added, the wire reads `core.keys.listed` of the theme's table and of `core.keys.frame` (what
  applies now, not all a viewer may ever have) and `zonesOf(listed, core.frame.lastFocus())`.
  The draw makes a group for each widget that holds the focus, the innermost first, with every
  listed entry of its zones (two zones on one element are one group); a group for each other
  widget with an `own` key, holding those keys alone, since such a key acts on the whole page
  while its `when` holds (the Escape that leaves Arrange mode); then the page's keys with the
  frame's. A group's heading is the theme's name for the zone (`core.frame.zoneNames()`). With
  no widget holding the focus a line says so.
- **Everywhere** is the last of those groups alone: the theme's `'page'` zone and the frame's
  list.
- **About** is the version (`viewer.version`, which a guest's model does not carry: the page
  says why), the address the page was served from (the daemon's own web server, so no route is
  asked) and links to the project: the workspace's `repository` and two places under it that
  the README links. Each opens in a new tab with `rel="noopener"`.

Help holds no key list: its model is the tables' own entries, each row carries its entry's id
and zone (`data-key`, `data-zone`), and two checks hold it there (`node/core-help.mjs` on the
four tables, `keys.py` in a browser). It writes nothing, wants nothing of the stream and has no
key and no listener; no theme file imports from its folder.

## Devices

`core/popups/devices/` is everyone's popup (read-only for a reader), the row *Devices…* above
*Help…*: the configured devices as a side list, an Overview with every device's key graphs, and a
page per device with its settings (name, profile, passphrase, *Make default*, *Remove…*), its facts
and *Identify* and *Show on the page*. A theme opens a device's page with
`core.popups.openDevice(url)` from its *Device settings…* (a person's gesture), which a theme draws
for every viewer (*Devices…* for a reader). It has a live list (`frame.put` for
a dot, a tag and a state line; a draw only when the entries change), follows devices on the
stream and is the one popup that stays through a device switch. Its profile choice is a list opened
from a button in the page pane, as a list in a popup is (the pane is a place for lists); the modal's
`onKey` hands Escape and the arrows to it. [devices-popup.md](devices-popup.md) has it whole.

## The EQ library

`core/popups/eq-library/` is a writer's popup, the row *EQ library…* between *Devices…* and
*Help…*: a search of AutoEq with a preview of the chosen measurement for the device on show (or for
the computer), an import that only saves, an upload of a parametric EQ file and the saved presets
with *Overwrite with current…*. It is about the side the page shows and so is not `stays`: a change
of the shown device closes it, and its title names the side in every draw. It has a search field
whose answers arrive late (each is matched to its request and a stale one is dropped), a file input
and a disclosure of variants; its compare choice is a list opened from a chip in the page pane. It
wants nothing of the stream: the page behind follows the device. [eq-library.md](eq-library.md) has
it whole.

## Adding a popup

1. Write `core/popups/<name>/draw.js` (pure: a model in, a string out, `esc` and `attr` on every
   word, the `fp-*` and `fi-*` classes only) and its string cases in a node harness.
2. Write `core/popups/<name>/wire.js`: `export function wire({ core, frame })` returning `{
   show(page), hide(), onKey(event) }`; no key listener; its stream want taken on the first `show`
   and released in `hide()`. **A popup names its device**: every device action takes a `{ device }`
   and every `store.part`, `store.device` and `store.refresh` the device's address, since the core's
   default is the shown device and a popup's page may be about another (`make theme-css-check` rule
   6 fails a wire that leaves one out). Its node cases run with another device on show than the page
   is about and read each request. `hide()` is safe to call twice, and `show` may follow it while
   the popup stands: the router hides a part whose `show` threw and hides it again at the close. Its
   cases run on the real core over `node/dom.mjs`.
3. List every file of the folder in `ASSETS` (`bins/svolderd/src/web/assets.rs`); the assets test
   fails on one that is missing.
4. Add it to `POPUPS` of `core/popups/listed.js`, with its row: `{ name: '<name>', title,
   entries, load: () => import('./<name>/wire.js'), row: { label, sees } }`. The core adds it
   and the menu has its row. Open it from a tap or a key with `core.router.open('<name>')`:
   never on load, never from a stream event (the router says so in the console, and `popups.py`
   fails on it).
5. Give it a row in `POPUPS` of `scripts/web-checks/popups.py`, with `add: None` (the page adds
   it itself), and list the harness in `run-all.sh`, `COVERAGE.md` and `README.md`.
