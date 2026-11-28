<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Web themes

The web page is a frame, a core and a theme. A theme is a whole experience of the same device, not
a colour scheme: it draws every control and every fact from one JSON model, in its own way. This
page is the map of that arrangement: what each of the three owns, what a theme must export, what
the core gives it, how it is loaded and chosen, how the checks hold every theme to the same
contract, and how to add one. The details sit in the pages it points to: [the frame](frame.md),
[the page and its stream](web-ui.md), [Ember's effects and Arrange mode](themes-and-effects.md),
[the checks](testing.md). It sits below [the map](../architecture.md).

The page ships five themes. **Ember** is the test theme: it is complete and it looks and behaves as the page does, so it proves the core against every kind of device. **Paper** is a sheet of squared paper in light and dark, with written controls and handwritten notes ([web-paper.md](web-paper.md)); building it on the core alone found seven gaps in the core, which that page lists. **Signal** is the sound's way from the player to the ears drawn as one line, dark only, with the chosen hop opened below it ([web-signal.md](web-signal.md)); it found three gaps: two in the page model's computer facts and the order in which the daemon shuts the web server and the host service down. **Night drive** is the music drawn as an outrun road, dark only, with the volume as a large neon number and the rest of the device on a sheet ([web-nightdrive.md](web-nightdrive.md)); it found gaps in the canvas loop, the analysis rules, the menu and the picker, and in the reserved keys, which that page lists. Its computer side is two keys by the menu button (the panel's top row on a phone) that turn the volume figure, the dash's keys and the curve on the mountains into the computer's ([host-control-themes.md](host-control-themes.md#night-drive)). **Void Garden** is a night garden drawn on the GPU that grows with the music, dark only, on three.js ([web-voidgarden.md](web-voidgarden.md), [its checks](web-voidgarden-checks.md)): the one theme with a frame budget, a choice of WebGPU or WebGL2 and a library of its own, and the one that opens the core's Device controls popup for what its dash does not draw ([controls-popup.md](controls-popup.md)).

## Who owns what

| Layer | Folder | It owns |
| --- | --- | --- |
| The frame | `web/core/` (`frame.js`, `menu.js`, `picker.js`, `modal.js`, `router.js`, `switch.js`, `frame.css`) | the sign-in page, the menu button and its menu, the picker of themes, the popups, the stack of modals, the history, the reserved keys `T` and Space, the empty theme root `#theme` |
| The core | `web/core/` | the stream, the model, the actions, the sender and its refusal words, the climb's toast, the per-theme preferences, light and dark, the lifecycle of a canvas, the analysis stream, the tone curve's maths and gestures, the registry, the hands every theme shares (`gestures.js`: dragging along a track and the keys; `place.js`: a mark's place as a fraction of its box; `volume-ask.js`: the step the page asked for, which stands for the reading until the device reports it, whichever of the write's answer and the stream's reading comes first: `noteReading`, `ackAsk`; and `createVolumeAsker`, the one ask every theme shares: the range and cap clamp, the "same ask" rule, the send and the landed fold; a theme keeps only its gestures, its drawing and the write it hands in), the daemon's clock (`clock.js`) |
| A theme | `web/themes/<id>/` | everything drawn in `#theme`: its markup or canvas, its style sheet, its effects, its menu rows, the sections it lets a person arrange, its test driver |

The frame sits above the theme by `z-index`, in a cascade layer that no theme's sheet can outrank,
so no theme can hide the way back to the picker ([frame.md](frame.md)). A theme draws the header,
footer, device list and sign-out its own way, and the menu carries a Sign out row as well, so a
theme that leaves it out never strands anyone from signing out (another device is shown from the
Devices popup).

## What a theme exports

A theme's module calls `registerTheme(theme)` (`core/registry.js`) when it is evaluated. The
object has:

- `id`, `label`: the folder's name and the name a person reads.
- `needs`: the live feeds it wants, from `meters`, `host` and `strip` (sent on the event stream;
  `strip` is the shown device's composite bar, which only Ember draws: a theme that does not list
  it neither reads it with the device's model nor hears its events) and `analysis` (the music's
  frames, which the core opens and closes). The model is always sent.
- `schemes`: `dark`, `light` or both. A theme with one scheme gets no switch.
- `supported()` (optional): `{ ok, why }`; a theme that says no is replaced by Ember.
- `mount(root, core, { signal })`: draws into `root` and returns the **mounted instance**,
  `{ unmount(), menuRows?(core), keys? }`. The signal is aborted when the theme unmounts or
  throws, so every listener the theme made with it goes too. Unmounting leaves nothing behind.
- `menuButton`, `menuOpening()` (optional), `preview(box)`: [what it gives the
  frame](frame.md#what-a-theme-gives-the-frame).
- `sections` (optional): the sections Arrange mode may move, if the theme arranges.
- `driver`: the test driver of the contract checks (below).

The mounted instance is what one mount gives the frame, built from that mount's own scope and
gone with it: `unmount()` takes the theme off the page, `menuRows(core)` (optional) are its rows
in the frame's menu and `keys` is its key table. The frame reads `core.mounted` (the instance on
show, `null` between two themes) before the theme object; a theme that returns only
`{ unmount() }`, the unmount function or nothing is still a theme, and the frame then reads
`menuRows` and `keys` from the theme object, as Paper's rows are.

### The key table

Every theme has one, in `themes/<id>/key-table.js`: `KEYS`, a list of entries `{ id, key, zone,
words, … }` ([key-table.md](key-table.md) has each field), pure data with no DOM. The mount binds it
(`core.keys.bind(table, { signal, ctx })`) and returns the bound table as `keys`. It must hold:

- **every key of the whole page the theme answers to**, with zone `'page'`; the mount gives each
  its `run` (Ember seven, Night drive `↑` `↓`, Paper and Signal none, their table bound all the
  same), or the entry says `own: true` for a listener of the theme's own;
- **every key a widget handles itself**, with the widget's selector as its zone and no `run`,
  a zone's entries together;
- **words for the Help popup** (sentence case, short, no full stop), a `when` on an entry that
  does not apply to every viewer or page, and `ZONES`, each zone's name, returned as `zones`.

[key-table.md](key-table.md) says which checks hold a table to its page.

A theme may draw with DOM, SVG, canvas or WebGL. It owns its style sheet, which must not name the
frame's elements (`make theme-css-check` fails when it does).

## The model and its events

The daemon builds the model from the same view models the TUI's words come from, so a value reads
the same everywhere and a guest sees what a guest may. The page carries the first snapshot in a
JSON block, so a theme draws at once with no blank screen; the stream then sends `model`, `part`,
`volume`, `level`, `meters`, `strip`, `fleet` and `host` events ([web-ui.md](web-ui.md#live-updates)).
`core.model` is the store that keeps them:

- `page()` is the whole model: `viewer` (`signed_in`, `guest`, `writes`), `fleet` (every device,
  its link and the declared hops), `shown`, `device`, `followed`, `card`, `host`, `strip`.
- `page().host` is the computer's player (`track`, `path`) and `route`, the route's facts read-only:
  `sink` (`words`, `rate_hz`, `format`, `volume_pct`, `muted`), `player_rate_hz`, `host_eq`,
  `eq_preset` (the applied preset's name, from the saved computer EQ state, only while the filter
  is in the player's path, on, and in front of this sink), `eq_autoeq`, `others_mixing`; `null`
  while the route is not read.
- `shown()`, `device(url?)` and `part(name, url?)` read a device and one of its parts: `link`,
  `card`, `volume`, `controls`, `meters`, `power`, `tone`, `presets`, `device_presets`, `facts`,
  `playing`.
- `on(event, listener)` hears `model`, `part`, `volume`, `level`, `meters`, `strip`, `fleet`,
  `host` and `state` (`open`, `lost`, `resting`, `waiting`).

The model describes a control by its kind: a level, a choice, a toggle, a slider, an action, a band
set, a meter, a power state. Each carries its words, range and state. A theme draws the kinds, not
the devices, so a control a profile adds later appears in every theme without a change to it. The
model holds no geometry. The stream follows the device shown and, when an owner asks, others, so a
theme may show several devices at once. Every owner of the stream (the theme mounted, and each popup
that shows devices live) says what it wants with `core.stream.want(owner, { needs, follow })`, which
returns its `release`: the stream's address is the union of every owner's wants, the stream opens
again only when the union changes as a set (the same feeds in another order change nothing), and a
release takes away only the owner's own wants. A want is kept as the owner asked: a followed device
the fleet does not list is left off the address while it is gone, and carried again when the fleet
lists it, with no new `want`. An owner name is the owner's own: two owners with one name replace
each other, and `theme` is the core's (set as a theme mounts; `core.stream.follow(urls)` sets the
devices the theme follows beside its `needs`). A popup releases its want when it closes; an owner
that never releases is carried in every address for the page's life. `core.stream.batch()` batches
changes: the stream opens at most once, when the returned function ends the batch (a theme switch
batches from the old theme's unmount to the new theme's mount). The stream's state is the page's, not
a device's: a wait after a wake drops a raise for every device until the first reading.

## What the core gives a theme

`core` is one object; `contract.py` pins its members and those of the parts themes and popups use.

| Member | What it is |
| --- | --- |
| `model` | the store above |
| `actions` | every write, as one call: `setVolume`, `stopClimb`, `setControl`, `setMode`, `setBand`, `setLoudness`, `recallPreset`, `savePreset`, `renamePreset`, `deletePreset`, `recallSlot`, `saveSlot`, `applySetting`, `connect`, `identify`, `renameDevice`, `setProfile`, `setPassphrase`, `makeDefault`, `removeDevice`, `press`, `signOut`, `signInPrompt`, `showDevice`, `hops` (`add`, `edit`, `remove`, `move`) and `walk` (`start`, `answer`, `back`, `cancel`) |
| `computer` | the computer's side of the page, by device: its view (`view`, `side`, `tone`, `volumeOf`, `switchOf`), the point picked (`pick`, `picked`, `repick`) and where a new point goes (`addAt`, `nextFree`); its writes are `actions.computer` |
| `prefs` | the mounted theme's own preferences, kept in `localStorage` under `svolder.theme.<id>.<key>`, every access guarded |
| `scheme` | light or dark for the theme: follows the system, with the person's word kept |
| `lifecycle` | `watchVisible` and `canvasLoop` (a loop that pauses when hidden, follows the size and the pixel density, and draws one still frame under reduced motion; `reduced: 'theme'` keeps it running under reduced motion for a theme that draws its own, and `pace()` gives the least milliseconds between draws, waited on a timer from 100 ms) |
| `analysis`, `levels` | the host's analysis frames, shared by every holder, and `viz-model.js` that decodes and smooths them and schedules the beat; `levels` is the host's levels alone (`?only=levels`), shared by its holders and apart from `analysis`, read by `vu-model.js` (needle, scale, reference, lamp; held to the golden file by `node/vu.mjs`, [the VU meter](vu-meter.md)) |
| `curve`, `plot`, `eq` | the tone control's maths; the plots' SVG as strings from the daemon's numbers (`frame`, `zeroLine`, `gridLines`, `curveD`, `fillD`, `path`, `pathThrough`, `bandSpans`, `gapSpans`, `overlayShapes`, `fitWords`, `sparkline`, `freqAt`, `FREQ_MARKS`; a target and a fit are `[{ hz, db }]` on one grid, a gap span `{ from_hz, to_hz }`, a sample `{ t, v }`; every colour is a class, and `plot` computes no fit); and the tone control's hands: `eq.bands` is the band drag (`createDrag`, `grab`, the steps and the writer, as plain coordinates, so any drawing can edit a band), `eq.points(options)` makes the hand on the computer's curve (with `pointValues`, `markersIn`, `POINT_KEYS`, `touchWatch`) and `eq.table(options)` makes the points' table (with `editCell`, `labelWords`, `POINT_TYPES`, `isLong`). `drag` is the old name of `eq`, kept as a pinned alias (`drag.pointDrag` is `eq.points`, `drag.table` is `eq.table`, the rest of `drag` is `eq.bands` and the helpers above): the current name is `eq` |
| `words`, `clock` | the daemon's words for ages, the signal and the Identify answer, and `stepWords(level, raw)`, the model's words for a step of a level; the Now playing card's arithmetic against the daemon's clock (`daemonNow` is keyed by the daemon's `now_ms` value at the page's first local sight of it, never by the time of a draw: frame.md), and `clock.tickAges(root, { stamp, ages, signal }, options)`, the one ticker that keeps a Device section's ages current (below) |
| `freeze` | the section keeper: `keeper(live, name)` counts the holds on a section a person has a hand on (a value typed, a list open, a question asked, a stepper held), `createRelease(doc, keep, signal)` gives one back after the press that closed what held it, `createLive({ core, signal, view, sections, device })` draws the sections of the device `device()` names (a page passes `() => core.model.shown()`) from the store's events and keeps the held ones as they are until the last hold goes |
| `frame` | `anchor(el, { fixed })`, `realign()` and `fade(on)` for the menu button; `covered()`: whether a modal, the open menu or a theme's own modal sheet stands over the page, asked before a key or a wheel of the whole page acts; `inMenu(node)` and `menuOwns(event)`: whether a node is in the menu and whether a key press is the menu's own; `keyTable()` and `zoneNames()`: the key table of the theme on show and its names for the table's zones; `lastFocus()`: the element inside the theme that had the focus last |
| `modal`, `ask` | the one stack of modal dialogs (`open`, `close`, `closeAll`, `opened`; `closeAll({ device: true })` leaves a modal opened with `stays`): the top owns the keyboard, the page under it is inert ([frame.md](frame.md#modals)); `key(event)` is the frame's, which hands the stack every key. `ask(text, options)` is the modal question on that stack: a promise of yes or no, with `ask.close()` and `ask.opened()` |
| `router` | the history state `{ device, popup, page }`: a popup opens, steps and closes through it, and Back steps a popup without changing the shown device; `shown(device, address)` is a device shown with no entry pushed ([frame.md](frame.md#the-history)) |
| `popups` | the popups of the page, each loaded when it is first opened: `add(name, { title, load, entries, stays })`, `names()`, `listed()` (the page's list of popups, each with its menu row) and, for a theme's foot (*Device settings…*, *Devices…* for a reader), `openDevice(url)` ([popups.md](popups.md)); the frame's to fill, not a theme's |
| `keys` | `frame`, the frame's own keys as a table (`T`, Space, which Paper's sheet names, and `?`), and the key table's hands: `bind(table, { signal, ctx })`, `listed(table, ctx)`, `zonesOf(table, el)`, `zoneOf(table, el)` ([key-table.md](key-table.md)) |
| `mounted` | what the theme on show returned from `mount()`, `{ unmount, menuRows?, keys?, zones? }`, or `null`: the frame's to read, not a theme's |
| `title` | the browser tab's title for every theme: `<device> · connected — svolder` (`reconnecting`, `reading again`, `resting`, the lowercased title of a card for a device out of reach, `fault`); `add(fn)` puts a theme's words after the status and returns the function that takes them away (the open popup's name stands there the same way; with no device shown the title is those words alone, `Devices — svolder`, and `svolder` without any); `statusOf(page, state)` is the status word for a theme's own trouble line |
| `repeat` | `repeat(event, { root, step, … })`: a pressed `−` or `+` steps once and, held, repeats after `REPEAT_AFTER_MS` (380) every `REPEAT_EVERY_MS` (70), up to a cap of 10 s; it follows the button of the pressed one's id through a redraw and ends at the pointer's release anywhere, the window's blur, the page hidden and the other ends the node harness lists. `core.hold` (`hold.repeat`, `hold.AFTER_MS`, `hold.EVERY_MS`) is the old name, kept as a pinned alias: the current name is `core.repeat` |
| `registry`, `theme`, `climb`, `toneNote`, `stream` | the registry of themes and the entry of the one mounted (`theme.id`, its `schemes`), the raised-volume toast, the tone mode's note, the stream |
| `driver` | the checks' driver for the device's facts (`reach`, `shown`; see "Every theme is complete") and, as `driver.controls` (`reach`, `operate`, `shown`, `state`), for the Device controls popup ([controls-popup.md](controls-popup.md)): only a theme's test driver hands the targets it does not draw to it (a theme with no sheet of its own for the controls); test surface, no theme calls it at run time |

A theme reaches the core's services only through `core`; it imports a core module directly only for
what holds no state and reaches no service. `PURE` in `scripts/theme-css-check.py` is the one list
(`make theme-css-check` fails on a theme file that imports anything else, a namespace or dynamic
import included; it reads the imports by pattern, so a path put together elsewhere and handed to
`import()` is not seen). Whole modules: `act`, `contract`, `html`, `path-words`, `phone`, `plot`,
`tone-curve`, `viz-model` and `words`, and the two shared widgets `list` and `inline-ask`: they
reach no service, and the one thing each keeps is which list or question is open in each scope (a
modal of the stack, else the page; [frame.md](frame.md), "Parts a popup shares with the themes"), so
a theme and a popup can both import them without closing each other's. Some exports of a module that
holds a service beside them: `clock` (every export but `tickAges`, which reads and writes the page
and holds a timer: the Devices popup calls it as `core.clock.tickAges`), `computer` (`POINT_TYPES`,
`ROWS_SHOWN`, `THIS_COMPUTER`, `markersOf`), `api` (`newest`), `write` (`mayWrite`), `repeat`
(`REPEAT_AFTER_MS`, `REPEAT_EVERY_MS`), `registry` (`registerTheme`, which adds the theme to the
registry's list: how a theme exists), `table` (`isLong`), `title` (`TITLE`, `statusOf`) and
`tone-drag` (the constants and the pure functions of a point). The model, the actions, the menu and
its keys (`core.frame.menuOwns`), the page's visibility (`core.lifecycle.watchVisible`) and the
drags (`core.eq.bands.createDrag`) are `core`'s.

The words each name one thing, so a new part does not add a sixth meaning to "hold":

| Name | What it is | Pinned alias |
| --- | --- | --- |
| `core.repeat` | a pressed `−` or `+` repeats while held | `core.hold` (`core.hold.repeat`) |
| `pending` (`createPending`) | the values this viewer is changing, until the daemon's model is read after them | none |
| `freeze` (`core.freeze`, `live.freeze`, `live.frozen`, `live.thaw`) | a section kept from a redraw under the hand | none |
| `core.eq.bands`, `core.eq.points`, `core.eq.table` | the band drag, the point drag and the table with `editCell`, `labelWords`, `POINT_TYPES` | `core.drag` (every member of the three, with `pointDrag` and `table`) |
| `core.stream.batch()` | changes of wants batched, the stream opening once (a theme switch batches from the old theme's unmount to the new theme's mount) | none |

An alias is one line of `core.js` (`hold`, `drag`) whose members are the current name's own;
`contract.py` pins both names and `node/core-names.mjs` holds each member of an alias to the current
one, so a theme written against the old name still mounts and does the same. Ember's `keep.js` and
`holdTone` keep what is typed or dragged across a swap of its own parts and are not part of the
contract.

`core.clock.tickAges` is the only code that ages a reading. A device's page in the Devices popup
carries the daemon's clock when its part was made (`data-now` on the element `stamp` selects) and
each reading's stamp (`data-heard-at`); every second while the page is on show, and at once when
it comes back, the ticker writes each age (`ages`) and the Bluetooth strength's (`signal`) again
where the words changed, looking the section up under `root` each time, so a redrawn section and a
removed row need no bookkeeping (a stamp is aged from its first sight). The popup calls it once,
with its own `signal` (the popup closing stops the timer) and, if a caller's section is hidden,
`when`; the returned `tick()` writes at once after a draw.

Every action toasts a refusal in the daemon's words and reads the device again, so what the
device holds is shown, not what was chosen. A guest's call sends nothing and says why. The volume
goes one at a time with the newest waiting and is held to the cap.

`core/actions.js` joins one file per domain: `write.js` (the turn-away and the finish every write
shares), `device-actions.js` (the thin device writes), `volume-actions.js` (the volume's climb),
`computer-actions.js` (`actions.computer`), `scenes-actions.js` (the Scenes popup's routes),
`host-actions.js` (the player's buttons, the hops), `navigation.js` (showing a device, signing out)
and `device-config-actions.js` (its settings).

Every action names the device it acts on. A call to a device takes a last `{ device }` (the model's
URL segment of the device), and the page's `shown` device is only the default target: the public
actions put it there for a caller that named none (one helper, `aimed` in `core/actions.js`), and
the actions under them read no device but the one named. So a theme that passes no device acts on
the shown one, and a popup that lists every device acts on any of them, shown or not, with the same
calls. The computer's preset calls (`recallPreset`, `savePreset`, `renamePreset`, `deletePreset` of
`actions.computer`) are the same: they name the device whose list they act on. A theme that one day
shows and controls several devices on one page passes each control's own device. A device's
settings have no default and toast no refusal ([devices-popup.md](devices-popup.md)).

## Safety stays in the daemon

A theme gets no way round a safety rule, because the rules are not in the page. The volume rule
and its three-quarters glide, the confirmations (a slot's save needs `"confirm": true`), write
pacing, gone-device handling and the sign-in and guest limits are the daemon's ([daemon.md](daemon.md),
[web-ui.md](web-ui.md#writes)). A theme reaches a device only through the core's actions, which
wrap the JSON writes. What a widget does (Ember's ring click on the knob, the sticky 0 dB on a
curve drag) is that theme's; a rule that protects the listener is the core's.

## Loading and the choice

The choice is per browser: `localStorage` and the cookie `svolder_theme`, which lets the server
paint the right frame on the first request. `?theme=<id>` sets it and is kept. The server lists the
themes in `THEMES` (`bins/svolderd/src/web/themes.rs`), which carries each theme's id, label,
blurb, module, style sheets, head scripts and metas, schemes, `color-scheme`, `theme-color` and the
files it serves. `switch.js` loads a chosen theme's files and mounts it; a theme that fails to
load, cannot run or throws is replaced by Ember with a toast that names it. The whole account is in
[frame.md](frame.md#the-choice-loading-and-the-fallback).

## The menu, the picker and `T`

The menu button opens one Theme row, then the theme's own rows, then Device and Sign out. The Theme
row, or `T`, opens the picker: a card for each listed theme, drawn by the theme's `preview(box)`,
the one in use marked, a close button at its top right and a foot that says how to get back (`T`
and `?theme=ember`). `T` opens it when nothing covers the page; over an open question, a popup or
a theme's own sheet it does nothing, and in the picker it closes it. Escape, a click outside the
sheet and the close button close it. No theme binds `T`, `?` or Space: a key table that does is
refused when it is bound. The menu and the picker are as tall as the visible window (the visual
viewport, `dvh`) and scroll inside themselves, so on a phone every theme and every row can be
reached. The menu never grows with the number of themes ([frame.md](frame.md)).

## Arrange mode

Arrange mode is Ember's and not the core's: Paper's order, for example, is part of its look. Ember
lists its sections, keeps an order per layout and per browser, and offers the mode from its menu
and the key `A` ([themes-and-effects.md](themes-and-effects.md#arrange-mode)). A theme that
wants the same exports `sections` and builds its own mode.

## Every theme is complete

A theme presents every control and every fact of every kind of device (DAC, amp, preamp) and never
sends a person to another theme for a missing piece. What stays a theme's choice is decoration:
effects, a visualizer, how things are worded and laid out.

`core/contract.js` makes that checkable. `targetsOf(device)` lists everything to operate (the
volume and mute, each control, the power switch, each band's frequency, gain and Q, the mode, the
loudness, a preset, a slot, a built-in setting, a transport button when there is a track). The
facts to show are two lists with no label in common:

| List | What it holds | Who must show it | Which driver answers |
| --- | --- | --- | --- |
| `themeFactsOf(device)` | Cover, when the player has one for the track | every theme | the theme's (`core.theme.driver`) |
| `deviceFactsOf(device)` | Product, Unit, Firmware, DSP, Profile, Link, Signal, Passphrase when noted, and each reading row's id | the core, on the device's page of the Devices popup (no theme draws them) | the core's (`core.driver`) |

The link words beside the device's name are not a fact of either list: a theme writes them while
the device is out of reach, and `contract.py` holds every theme to that through `linkWords()`.

`core.driver` (`core/driver.js`) is the checks' way to the device's facts:
`reach(label, { device })` and `shown(label, { device })`, with `device` the url of the device
meant, named at every call (the driver never takes the shown device for a caller that named
none). It opens the Devices popup at that device's page through the router, opens *All readings*
for a reading row, and reads the cell's words, whichever theme is mounted; it refuses with `false`
and `null` a label that is not a fact of that device (Cover, a target, an unknown word), a missing
device and a viewer the router turns away. No theme and no module of the core calls it; it is test
surface, like a theme's driver. A theme owes only its list and its foot (*Device settings…*).

A theme's `driver` lets a check do what a person does:

- `reach(target)`: show it (scroll, open a fold) and say whether it is there;
- `operate(target, change)`: press it, key it or pick its value;
- `shown(target)`: the words the theme shows for it (a fact is named by its label);
- `state()`: `{ lost, waiting, resting, toasts }`;
- `linkWords()`: the words beside the shown device's name that tell its link state, or nothing.

A driver acts on the theme's own DOM and never calls an action itself, so what the check sees at
the daemon went through the theme's handlers.

## What every theme does the same way

Completeness says what a theme shows; these lines say how the same thing is done wherever a person
learns it once. Each line names its check, or says which theme does not keep it yet.

- **The frame's menu button is the same in every theme**: 44 by 44 px, 8 px from the window's top
  edge and 8 px from its right edge, at every width, pinned to the window (it stays there when the
  page scrolls) on an opaque backdrop in the theme's own colour (a plain background, no blur), so it
  reads cleanly over scrolling content. Each theme's header keeps that corner clear: nothing the theme draws stands under
  it, for a guest too (Ember's Sign in stands left of it). The menu scrolls inside a short window.
  Checked by `frame_button.py` (every listed theme, signed in, guest and with a long device name, at
  1280x800, 768x1024, 412x915 and 360x780: the exact rectangle, nothing under it at the top of the
  page, the same rectangle and an opaque backdrop after a scroll) and, for the menu's scrolling,
  `effects_menu.py`. Kept by all five.
- **Space plays and pauses the computer's player in every theme.** The frame's second reserved key:
  Space alone, not a repeat, with the focus neither in a field it would type into nor on a button, a
  link, a summary, a select or an element with a role Space activates (button, slider, switch,
  checkbox, tab, option, menu items), and no menu, picker or modal dialog open, posts one press of
  `pause` (while the player plays) or `play` through `core.actions.press`, and the page does not
  scroll. Night drive's sheet counts as a modal dialog. Nothing where the page has no player or the player has no buttons. Checked by
  `space_key.py` (every theme) and `node/frame-keys.mjs`.
- **The device's connection state is a mark beside its name, and every state but connected is also
  in words**, never by colour alone. The words are checked in every theme by
  `contract.py` (`the_link_state_is_in_words_beside_the_name`, through the driver's `linkWords()`):
  a device taken off the air has words beside its name that it did not have while connected. Ember
  keeps the whole line, a dot after the shown device's name (named for a screen reader, not a tab
  stop, following the device after a switch), the word drawn unless connected (`ember_link.py`).
  Paper says the state in the words under its title (always, `, connected`), with no mark yet.
  Signal says it under the device's name on the line (the card's title) with the hop's dot, not the
  link's own word. Night drive says it in its top line, beside the device's name, while the link is
  not on (`not connected`).
- **The volume's `−` `Mute` `+` in that order, held repeats (the core's hold: after 380 ms, every
  70 ms, a 10 s cap), muted reads `−∞ dB`.** Signal keeps all of it (`signal_theme.py`:
  `the_volume_buttons_are_in_embers_order`, `holding_plus_repeats`, the muted words). Ember keeps
  all of it (`knob_click.py` for the order and the hold; `ember_link.py`
  `the_muted_volume_reads_minus_infinity`). Paper's exception: it writes `− quieter`, `mute`,
  `louder +` as words, says `muted` when muted and does not use the core's hold; this is a decision
  of its style, not a gap to close. Night drive's dash keeps all of it (`nightdrive_theme.py`,
  `the_dash_runs_minus_mute_plus_input_track_transport_device` and
  `holding_plus_repeats_and_the_release_ends_it`).
- **Now playing: the title, the artist, the transport as inline SVG, and the cover when there is
  one**, each theme styling it (Paper draws it in monochrome). The cover is a fact in
  `themeFactsOf(device)`, so `completeness.py` fails a theme that does not show it. That pass runs
  at 1280 by 900 in Chromium with Ember's sections in their usual order; Ember drops its cover in a
  Now playing cell under 400 px wide (an arranged band of three sections), where the fact is not
  shown and the check does not look. With no cover Paper and Signal draw no box and leave no gap; Ember keeps the
  box empty so its card does not move. Each theme's own check covers the cover going and a new
  track (`signal_theme.py`, `paper.py`, `playing.py`). Transport as inline SVG: Ember, Signal and Night drive.
  Night drive shows the cover in its dash, between the input and the title, and draws no box when
  there is none (`nightdrive_theme.py`, `the_dash_shows_the_cover_when_there_is_one`).
  Not yet: Paper's transport is written words, by its design.
- **The tone curve: drag a point, the wheel or a pinch for Q.** Kept by Ember, Signal and Night
  drive, Paper without the pinch ([checks](web-curve-dragging.md#which-themes-keep-it)).
- **The overlay of an imported preset** (`tone.overlay`, [autoeq.md](autoeq.md#the-web-routes)),
  while the EQ is on: Ember, Signal and Paper draw the target dotted, the gap spans shaded and the
  fit in words; Night drive has `fit` in its EQ line and a ghost ridge (`tone_overlay.py`).
- **The tab title comes from the core** (`core.title`: `<device> · <state> — svolder`): no theme
  writes its own. Checked by `node/core-title.mjs` and `core_rows.py` (`the_tab_title_follows`, on
  Ember: the title is the core's, so another theme adds nothing; Signal's lost and back is
  `signal_theme.py`). Kept by all.

## The checks

One set of checks runs against every listed theme ([testing.md](testing.md)):

- `contract.mjs` (node): the lists above and the actions' bodies.
- `contract.py`: a short pass per theme in each browser, under five minutes: volume up and down,
  mute, a band, a preset, a device lost and back, a refused write, resting and waking, the menu
  button present and `T` opening the picker, also after a fade, `?theme=` switching, and the same as
  a guest. It checks what the fake daemon received and what the theme showed, never pixels.
- `completeness.py`: every target and fact on a DAC, an amp, a five-meter amp, a mono amp and the
  Loki Max (the track's cover is a fact, shown by every theme).
- `frame_button.py`: the menu button is the same rectangle in every theme, with nothing under it.
- `ember_link.py`: Ember's connection dot, its words for every state, its place after a switch, and
  the muted volume's `−∞ dB`.
- `core_rows.py`: what the core holds for themes that need it: a second device followed, a declared
  hop, the menu's fade, a band moved from plain coordinates, light and dark, a canvas loop that
  pauses, a theme that unmounts clean.
- Ember keeps every check; they prove the browser-drawn page is the page. A theme's own
  browser pass stays short. Paper's own are in [web-paper.md](web-paper.md#the-checks-and-the-mutants), Signal's in
  [web-signal.md](web-signal.md#the-checks-and-the-mutants) and Night drive's in
  [web-nightdrive-checks.md](web-nightdrive-checks.md#the-checks-and-the-mutants).
- `contract.py` and `completeness.py` run every listed theme; `WEB_CHECK_THEMES` (a comma list)
  limits them while one is being worked on, and an unlisted name fails the pass.

## How to add a theme

1. Make `bins/svolderd/web/themes/<id>/` with a module `<id>.js` that calls `registerTheme` with
   the object above, its style sheet, and a `driver`.
2. Add one entry to `THEMES` in `bins/svolderd/src/web/themes.rs`, with every file of the folder
   in `files` (a name not listed there is never served).
3. Keep the style sheet off the frame's elements, use only the page's own origin (the policy is
   `default-src 'none'` with `'self'`), write no inline script or `style=` attribute and add no
   build step.
4. Draw every control by its kind from `core.model` and write through `core.actions`.
5. Put the theme's look in tokens, set every colour token of the frame (`--frame-*`,
   [frame.md](frame.md#the-tokens); `make theme-css-check` fails on one left unset) so the button,
   the menu and the popups match, and give `preview(box)` a small picture.
6. Write its key table, bind it in `mount()`, return it as `keys`; add it to `theme-keys.mjs`.
7. Run `make check` and the contract checks: they pick the theme up from the page's list. The
   completeness check lists the themes it expects (`listed` in `completeness.py`); add the new id.
   A light and a dark theme sets its `--frame-*` tokens for both and gives `theme_color_dark`.
8. A gap in the core is fixed in the core, not worked round in the theme. A theme that cannot fix
   it in the core says so in its page's "found missing" section (each theme's page has one).

[Back to the map](../architecture.md).
