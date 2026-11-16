<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Signal

Signal is the page's third theme. It draws the sound's way from the player to the ears as one line
and makes the line the page: each hop on it is something the sound passes through, and choosing a
hop boxes it and opens what that hop offers underneath. It is dark only, on a warm near-black
ground, in the system's own type. It is plain ES modules with no build step, in
`bins/svolderd/web/themes/signal/`, and one entry in `bins/svolderd/src/web/themes.rs`. It is built
on the core alone: it shares no file with Ember or Paper, and what it needs of the core is in
the last section. [Web themes](web-themes.md) says what a theme is; [the frame](frame.md) says what
the page keeps for itself.

In this theme's words, **the signal path** is the line. The word *Signal* alone is the theme's name
and, in the device's facts, the strength of the Bluetooth link; the two are never mixed.

## What the line says

The page never pretends to know more than it does. A hop svolder reads is a solid dot with live
facts under its name. A hop the owner declared (`svolder hops`, or the menu's Add to the chain… row)
is a plain ring with the owner's own words in italics. After the DAC the sound is real but svolder
knows the next hops only because it was told, so the line runs on there, thinner, dimmer and slower,
and never stops.

The hops, left to right, are:

1. **The player**, when the computer's player is read: green. Its words are its stream's rate and
   its status.
2. **The computer** (PipeWire), whenever the daemon watches the computer: yellow. Its words are
   the sink's rate and volume and, when the host EQ is on, `host EQ on`. On a DAC's page, before the
   route is first read, it says `route not read`; on an amp's page, which holds no route, it is
   drawn without facts and without words.
3. **The shown device**, boxed when chosen: amber, as is every device svolder drives. Its words are
   the path's reading of its input and what the link from the computer does: `arrives as sent`
   when the DAC reads the rate the computer sends, else `changes the sound`. While the DAC's
   rate is not read it says neither, as nothing was checked.
4. **The hops declared after it**, in the order of the configuration file, each of the kind the
   owner gave (`your amp`, `your headphones`, `your speakers`, `your gear`) and the owner's note.
   A hop that links a Forkbeard amp the daemon lists is live: a solid amber dot, with the amp's own
   area behind it.
5. **A stand-in ring**, only while the host EQ is applied from an AutoEq preset and no headphones
   hop is declared: it carries the preset's name and says `from your AutoEq preset`. It is shown
   only while the correction is applied, and it is never written to the configuration; a
   headphones hop the owner declares takes its place.

With the computer not watched, the line starts at the DAC, with a faint stub on its left and one
quiet line, `the computer is not watched`. With more than seven hops a label is its name alone.

**An amp on its own.** An amp that no hop links has its own line: the player and the computer
greyed (no flow), a plain ring `a DAC svolder does not drive`, then the amp boxed. Linking it from a
DAC's Add to the chain… sheet puts it after that DAC; Unlink returns it to standing alone.

A linked amp shown on its own page (through the device list, the Devices popup's *Show on the page*
or `?device=`) does not claim a digital path from the computer to the amp. The line runs the player
and the computer greyed (no flow, as above), then the DAC the hop names, a live amber dot that shows
that device when chosen (as the device list does), then the amp boxed. The stretch from the DAC to
the amp is the thin one. When the DAC is not listed the amp stands alone.

**The colour of a stretch.** The stretch up to the DAC is thick, and its dots move 16 px
every 1.1 s; after the DAC it is thinner, smaller and slower, 16 px every 1.6 s. A stretch the host EQ
changes is yellow. The computer stretch counts as changing the sound when the shown device's path
has a `computer` hop that changes it, or, when there is no such path (an amp is shown), when the
route says the host EQ is on.

**The dots** are dashes of an SVG stroke moved by one loop on the core's `canvasLoop`, which runs
on the line's own wrapper. They move only while the track's status is `playing`. They stand still
while the page is hidden and, under reduced motion, never move (one still frame is drawn). No CSS
animation drives them, because it would keep running while hidden.

## The layout

On a desktop (720 px and wider) the page has three bands. A DAC's first screen is the volume and the
tone curve, in two columns from 1020 px; between 720 and 1019 px, where the curve would be narrower
than 300 px, it changes (below).

- **The line band**, 140 px high: the dots on y = 60, labels from y = 74, hops spaced evenly
  between 10 % and 90 % of the width. The chosen hop is a box 200 by 104 px with a 12 px notch
  pointing down at the open area, filled faintly amber with the page's only glow. The menu
  button is anchored to a slot at the band's top right. There is no `+` on the line: adding a
  device is a row of the frame's menu (below).
- **The open area**, under the band: what the chosen hop offers (below). A DAC's is the volume and
  the curve (`310px 1fr`, a 40 px gap, from 1020 px; the computer's adds a third column from
  1240 px, below). From 720 to 1019 px the tone curve (and an
  amp's meters) take a full-width row of their own under the volume, so the curve is never
  narrower than 300 px and a point can be dragged on a portrait tablet. The input, the mode and
  the presets are in the shelf (below), not here, except the mode on a phone.
- **The Now playing bar** at the bottom: transport, title and artist, progress. It is shown under
  every hop except the player, whose own area already is Now playing, and stays pinned at the
  bottom of the window.

On a window taller than 1100 px the page stops growing: the line, the open area, the shelf and the
Now playing bar stay together as one block, centred vertically in the window, instead of the open
area stretching and a gap growing under it. Up to 1100 px the page fills the window as it always
did. (CSS: from 720 px wide and 1100 px high, `.sg` centres its children and `.sg-open` stops
growing.)

Below the first screen is the **shelf**, in two columns from 900 px (one column from 720 to 899 px,
where the band table needs the width), for a DAC: the band table (typed values with a `−` and a `+`
for each, one device step a tap) with the input under it, beside the **mode** (it heads this
column), the presets (Save as, Rename and Delete with their questions in place), the controls
(balance and sliders as scales, toggles as pills) and a Loki Max's slots and settings; and under
both, across the full width, `Device settings…`, which opens the device's page in the Devices
popup (the facts and Identify are that page's); a viewer who may not write reads `Devices…`: the
same page, read-only. Every focusable control carries a
scroll margin so focus never lands under the slim bar or the Now playing bar.

**Mode and Loudness.** The mode is one chip for each mode the device has. While Loudness is on its
chip is one wrap: a round `−` and `+` over the chip's two ends and the strength under its name, so
the strength is stepped where Loudness is chosen. A device that is not a DAC keeps a Loudness box
in its shelf. There is no Preset chip: the presets are the buttons of the shelf.

**Holding a stepper.** A band table's `−` and `+` and Loudness's repeat like the volume's: a step at
the press and, after the pause, steps at the same pace. The shelf is drawn again from every answer,
so the hold follows the pointer, not the button: at each step it finds the button again by its id.
It ends at the release or a cancelled pointer anywhere, the window losing focus, the page hidden,
the stream lost or waiting, another device shown, or no such button left. A key press is one step.
A stepper takes no text selection, no panning and no context menu, so a long press on a phone
only holds.

**The slim bar.** When the line band leaves the window, a bar 64 px high pins at the top, under the
frame's layer (the menu button and toasts) and padded for the safe area. It holds the same hops as
names only, the chosen one in a ringed pill, the shown device's with its volume (`Bifrost 3 ·
−23.5 dB`). A tap chooses that hop and scrolls back to the top.

## Choosing

The line is one `listbox` (horizontal) and each hop an `option`. A click or tap on a hop's dot or
label chooses it; Left and Right move the choice and Home and End go to the ends. The shown device
is chosen on load and after a device switch, and the choice is not stored. The device's name has a
caret and opens a list of the other devices when there is more than one.

A chosen hop that goes away (an amp unplugged or unlisted, a declared hop removed from the
command line, the player quitting) gives the box back to the shown device, redraws the open area
and writes to nothing that is gone.

## What each hop opens

- **The DAC.** The volume large with `−`, Mute and `+` (Ember's order; a device with no mute key has
  `−` and `+`), and a thin track with its knob and the cap mark that can be dragged or keyed; the
  tone curve with a draggable point for each band (through `core.eq`); on a phone the mode is
  under the curve. The input and the mode are chips (a longer list of inputs is one chip that
  opens a list), the input under the band table and the mode heading the middle column of the
  shelf. The shelf is below. While the device is muted the large number reads `−∞` with
  its unit `dB`, the Mute button reads Unmute and is pressed, and the slider's value text says
  `muted`; the slim bar and the line's volume words say `−∞ dB` too. A held `−` or `+` steps at once
  and, after 380 ms, every 70 ms (the core's `core.repeat`, the same as Ember's knob). The
  hold ends at the release, a cancelled or lost pointer, the window losing focus, the page hidden,
  the stream lost, another device shown, or the button leaving the page; the open area is held
  while it lasts so a reading does not replace the button.
  Enter or Space on a focused button is one step. On the curve a two-finger pinch sets a band's Q as
  Ember's does (`tone-wire.js`, the gesture is the core's `tone-drag.js`): fingers apart widen the
  bell, together narrow it, twice as far apart halves the Q; a finger already on a point pinches
  that point's band, else the band nearest the fingers' midpoint; the page neither zooms nor scrolls
  while the two are down. The transport buttons (here and in the Now playing bar) are inline SVG
  shapes in the button's colour, so no font can swap them for a picture.
  While an imported preset is on and the EQ is on, the curve carries the daemon's `tone.overlay`
  (`core.plot.overlayShapes`): the target as a dotted line (`sg-target`, `--frame-graph-target`),
  under the curve the gap as a wash over the daemon's spans only (`sg-gap`, `--frame-graph-gap`),
  and the fit in words under the frequencies (`sg-fit`, `fit 0.9 dB`; its title and a hidden `sg-sr`
  sentence say the worst miss and what the shapes are). None in Flat or Loudness
  or without an imported preset; a held point drops the gap and the words. The computer's graph
  has none.
- **A live amp.** Its power, its switches, its meters with the class A share shaded, its
  faults (the fault family's rows, over-current and over-temperature), Identify and Unlink (its
  facts are the Devices popup's). Identify stays in the amp's area because that area is where the
  amp that is not the shown device is worked, and the popup is a separate place to reach it from.
  Every write names the amp's own URL
  (`{ device: url }`), so choosing a hop never switches the shown device, and the DAC's volume
  keeps being written to the DAC while the amp is chosen. The amp's readings come from the stream
  once it follows the amp; until the first reading the area says `Reading the amp…`.
- **A declared hop.** The dashed box, the owner's name and note, where it sits, Edit (the sheet's
  fields in place), Move earlier, Move later and Remove, which asks once in place. A move names
  the place in the whole `[[hop]]` list of the previous or next hop on the same line; the first
  and last on a line have the button off. A linked hop's Edit offers the note only, since its name
  and kind are the device's.
- **The player.** Now playing grown: title, artist, album, progress, the transport and the path in
  words with its verdict. The cover is a second column of the top (label to transport), square,
  `clamp(200px, 20cqw, 300px)` wide in the hop box's glow and top-aligned; under 600 px of the
  area (a container query) it is above the title at 240 px. No cover, one column. A long title with
  no break points wraps inside its column (`overflow-wrap: anywhere`), never under the cover. The
  box is sized by CSS and the daemon serves the player's picture as published (up to 4 MiB, no
  thumbnail), so its sharpness is the player's own art: a small picture is stretched by the browser
  to the box.
- **The computer.** Its hop is the flip (see `host-control.md`): chosen, the area is the DAC's
  screen acting on the computer (the level in %, the curve with its points, Off | EQ, the points'
  table, the presets), with the route's facts that are not controls under it: the sound card and its
  format, the other streams and the path's verdict. Where the controls are not drawn (not served,
  unavailable) the hop opens those facts alone. Choosing the hop writes nothing. Its area also holds
  the level meters (next section).

## The computer's level meters

The computer's area (`#sg-cvu`) shows two needle dials, left and right, of what the computer plays,
with the reading in VU under each, a peak lamp per channel and, under both, the reference. The rules of the
needle, the scale and the lamp are in [the VU meter](vu-meter.md); this is how Signal draws and
holds them. From 1240 px the area has a third column, 400 px, beside the volume and the curve
(`310px 1fr 400px`, the curve keeps 300 px or more); from 720 px to 1239 the dials sit beside the
volume in the first row and the curve takes its own row below; on a phone the order is volume,
meters, curve. The dials are side by side in their column.

- **Meters | Spectrum.** Two chips in the section's corner choose what it shows: the dials (the
  default) or **Spectrum**, a canvas line of the 240 smoothed bands (`core`'s `VizModel`, in
  `--sg-computer`; no beats, no max hold) in `draw/spectrum.js`. This is the only visualizer
  Signal has.
- **The glow.** Each dial face is one `<button>` named for the glow ("Meter glow: filament.
  Change"); a tap, Enter or Space turns both dials W1 (subtle: a faint warm wash on the face), W2
  (warm backlight: a lit face with a dark scale) and W3 (filament: the scale and needle glow and the face
  stays dark; the default) in turn. The glow is a class on `#sg-cvu` (`sg-w1`, `sg-w2`, `sg-w3`);
  the key is in Signal's key table.
- **The reference.** `−`, `+` and a reading ("0 VU = −18 dBFS") move it a dB at a time from -30 to
  -6; Reset appears only when it is not -18. It changes the needle at once.
- **Preferences.** Per browser, through `core.prefs`: `vu-glow` (`w1`, `w2` or `w3`), `vu-view`
  (`meters` or `spectrum`) and `vu-ref` (a whole number); each read is guarded and a bad value
  falls back, so the page is whole without storage.
- **What is held.** Only while the section is on the page and the tab is visible. Meters holds the
  levels-only stream (`core.levels`, 4 bytes a frame, [host-capture.md](host-capture.md)) and one
  animation loop that writes two `rotate()` transforms through the style object, the readings as
  text at most 10 times a second and a lamp class on change; Spectrum holds the full stream and its
  canvas loop instead. The face, glows and scale are drawn once with the section, and `live.js`
  puts the needles back after a redraw (`view.openDrawn`) rather than drawing per frame. Under
  reduced motion the needle takes the settled value of each reading. A capture that is `starting`
  or unavailable says so under the dials and the needles rest.
- **What it says.** "The level of what PipeWire plays, read at the sound card's monitor." The
  meters follow the volume only where the sink's monitor does.

**The Add to the chain… sheet.** The frame's menu has Signal's row, `Add to the chain…`, offered
while a device is shown; it opens a sheet that asks what comes after the device: Amp, Headphones,
Speakers or Other, a name and an optional note. Forkbeard amps the daemon lists that no hop links
come first as `<name> (connected)`; picking one links it. A name with a space and a slash is kept
whole. While the sheet stands the page behind it is `inert`, Tab stays inside it, and Escape or
Cancel sends nothing; when it closes the focus goes back to what had it, else the menu button. A
viewer who may not write gets the core's refusal instead of the sheet.

**The tab's title.** It is the core's (`core.title`, [web-themes.md](web-themes.md)), the same for
every theme: `<device> · connected — svolder`, or `reconnecting` while the stream is lost,
`reading again` or `resting` when the page waits or rests, the lowercased title of the trouble card
for a device out of reach (`fault` for a fault), and plain `svolder` with no device. Signal drew it
first and the core took it over.

## The states

- **Stream lost.** The line greys, the subtitle says `Reconnecting to svolder…`, and the page is
  inert.
- **A device out of reach** (a lost link, waiting, resting). The line dashes into a hollow ring at
  the DAC, red for a lost link and amber for waiting or resting. Everything after it is still and
  dimmed. A card in the open area says why, with `Try now` and `next try in 12 s` from the daemon's
  `retry_in`; the last heard volume and curve stay in view, dimmed and inert. The player and the
  computer keep flowing.
- **A fault on a connected device.** The ring at the device is red and the card's words are in the
  open area, but the controls stay live: the daemon's own refusals apply.
- **A refused write.** The core's toast, and the device is read again.
- **A guest or a read-only viewer.** Every control is drawn, inert, and a tap toasts the core's
  words. That includes the Add to the chain… sheet and every edit of a hop, which post nothing.

## The phone

Under 720 px the line is a strip 92 px high across the top. The chosen hop is centred and boxed
(112 by 74 px), neighbours are 110 px either side and fade at the edges. A swipe of 40 px or more
(pointer drag, with `touch-action: pan-y` so the page still scrolls) or a tap on a neighbour
chooses. The strip pins at the top when the page scrolls, so there is no slim bar on a phone. The
menu button is the frame's, at 44 px (Signal sets `--frame-btn-size`), and the strip leaves a
margin so the button and its touch area never cover a hop or a dot. Signal anchors the button with
`fixed`, so it is placed against the window at every width (8 px from the top and right, on a
solid backdrop of Signal's ground) and keeps its place as the page scrolls, in the pinned strip on a
phone. The first screen is the volume and the curve with the mode under it; the shelf follows in
one column, opening with the input, then the presets, then the band table, the rest and
`Device settings…`. A change across 720 px draws the page again in the other order. Nothing
scrolls sideways at 360, 390 or 719 px, with a 40-character declared hop, a device named with a
space and a slash, or six hops after the DAC, and every control is 44 px.

## Look

The tokens are in `style.css` on `:root[data-theme="signal"]`.

| Token | Value |
| --- | --- |
| ground, and its glow | `#0f0d0b`, `#17130f`: `radial-gradient(ellipse at 50% 70%, glow 0%, ground 70%)` |
| ink, mid, dim | `#f4eee5`, `#ada396`, `#897f73` |
| faint, thin (a thin stretch) | `#6b6258`, `#7a7064` |
| player, computer, device | `#7cc27e`, `#ead27a`, `#f0a73a` |
| bad (a lost link) | `#e0613c` |
| edge, box fill, box glow | `rgba(255,236,212,.14)`, `rgba(240,167,58,.06)`, `0 0 30px rgba(240,167,58,.1)` |

Text on the ground reads at 4.5 to 1 or better; the faint colour is for lines and never for text,
and a declared hop's words use the dim one. There is no scheme switch and no effects, and the only
picture of the music is the computer's Spectrum line, so the theme gives the menu no rows of its own. The menu button is the plain ink circle. The
preview is the line with five hops and the DAC boxed. Signal needs `meters` and `host`, and its
only scheme is `dark`. The fonts are the system's, so the page makes no new request.

Nothing Signal draws carries a `style=` attribute or an inline script or style: positions are set
through the style object of the element or as SVG attributes, which the page's content policy
allows. Browser storage is only through `core.prefs`, under `svolder.theme.signal.<name>`.

## The files

| File | What it holds |
| --- | --- |
| `signal.js` | the registration, the mount, what each kind of hop opens |
| `line.js` | the hop list from the model (pure), the choice that survives a redraw, the followed URLs |
| `live.js` | draws the sections, keeps them current, holds one while a hand is on it (the holds are counted by `core.freeze.keeper`) |
| `draw/` | one file per section: `line.js` (the wire, dots, labels, box, strip, the loop), `volume.js`, `tone.js`, `choices.js`, `playing.js`, `shelf.js`, `facts.js`, `amp.js`, `hop.js`, `player.js`, `computer.js`, `card.js`, `vu.js` (the dials), `spectrum.js` (the line), and the small `word.js`, `esc.js` |
| `*-wire.js` (`vu-wire.js`: the glow, the reference, the streams and the clock), `strip-wire.js`, `lists.js`, `ask.js` (the core's list and inline ask in Signal's classes and words), `device-list.js` | the hands: what a tap, a drag, a key, a list or a question does |
| `slim.js`, `place.js` (the core's `placeFractions` with Signal's fill class), `words.js`, `presets.js` | the slim bar, pure maths and fixed words; the drag and the keys of a track are the core's `gestures.js` |
| `driver.js`, `preview.js`, `style.css` | the test driver, the picker's picture, the tokens and every section |

Pure modules take the model and return text or numbers; drawing modules return markup; wiring
modules are the only ones that listen. Signal reaches a device only through `core.actions`, and
every action for a device that is not the shown one carries that device's URL. Safety stays where
it was: the volume rule and the cap, confirmations (a slot save posts `"confirm": true` only after
the question is answered), write pacing, gone-device handling, sign-in and guest limits are the
daemon's and the core's.

## The checks and the mutants

- **Node** (`scripts/web-checks/node/`): `signal-line.mjs` (the hop list: order, linked and
  declared, an amp on its own, the AutoEq ring, a choice that falls back, trouble) and
  `signal-draw.mjs` (every section from every fixture, guests quiet, the computer read only, the
  shelf's `Device settings…`, the card, no `style=`).
- **Browser** (`signal_theme.py`, Chromium and Firefox): the line's order, sizes and spacing,
  choosing by click and keys, the dots moving, holding and standing still, the first screen, the
  shelf, the mode's place at 1280 and 390 px, Loudness's round buttons, held steppers, the slim
  bar, a linked amp and where its writes go, the menu's Add to the chain… row and its sheet against
  `svolder hops list`, the tab's title, edits, moves and removes, the player and computer areas,
  the states, the phone strip, the menu button clear of it and fixed against the window (Paper
  and Ember keep page placement), no sideways scroll with long names, and contrast.
- **The contract and completeness passes** run Signal through its driver. The driver chooses the
  hop that holds a control before it reaches for it (an amp's control is written through the amp's
  hop) and reads values back from what the page draws. The completeness pass also asserts that every
  theme lists as many targets of each kind as the first theme listed, on every kind of device; the
  device's facts are reached through the core's driver on the Devices popup's page.
- **The meters**: `node/signal-vu.mjs` (the section's markup) and `signal_vu.py` (Chromium and
  Firefox: the needles' transforms per frame, the lamp, the glow cycle and its key, the reference,
  the chips and the streams held, the three widths); the needle itself is `node/vu.mjs` against
  the golden file.
- **Mutation runners**: `node/mutate_signal.sh` (twelve mutants of the sources, the last three
  the mode kept on a desktop's first screen, a Loudness chip without its round buttons and the menu
  row offered with no device; the first nine: the line dropping the
  hops after the DAC, a declared hop drawn live, a gone hop staying chosen,
  the computer hop gaining a write, a strip that does not centre, a linked amp losing its DAC, an
  amp's faults picked by their words, a muted volume's word, a transport icon shown to readers)
  and `mutate_signal.py` (fifteen mutants of the page: an amp write without the amp's URL, dots
  that ignore reduced motion, a slim bar that never pins, an Add to the chain row that opens for a
  guest, a held plus or band stepper that never repeats, a menu button that follows the page on a
  phone, a missing mode under the curve, a plain tab title, a strip that does not centre, and
  others; a mutant is killed only by a check it
  names, so a flake in another check cannot pass for a kill). A guest's
  post cannot be mutated in Signal because the core refuses it first, so that mutant opens the sheet
  for a guest instead. A full browser run takes about 75 minutes.
- **Stills**: `make signal-shots` draws a DAC at desktop width, with the linked amp chosen and with
  a declared hop chosen, a lost link, the Add to the chain sheet, the linked amp shown on its own
  page, a tablet at 768 px, and the phone at rest and scrolled, into ten PNGs.

## Known limits

- A fault card is covered by node tests only: the fake devices cannot report a fault.
- Signal keeps near-copies of Paper's gesture, list and preset modules; they are not shared through
  the core.

## What Signal needs of the core

Signal is built on the core alone. Two parts of the page model, one of the shutdown and one of the
frame exist for it:

1. **The model's `host.route` holds the route's facts**, read from the daemon's host view: `sink`
   (`words`, `rate_hz`, `format`, `volume_pct`, `muted`), `player_rate_hz`, `host_eq`, `eq_preset`,
   `eq_autoeq` and `others_mixing`. It is `null` while the route is not read, which the daemon does
   for an amp's page (no path, no hold on the computer).
2. **The daemon reads the computer EQ's saved state**, the file the computer EQ command writes,
   found through one function the command line and the daemon share, so a front end can name the
   applied AutoEq preset. `eq_preset` is the preset's name and is set only while the state is on,
   the route has the host EQ on, and the state's target is this route's sink; a host EQ put in by
   hand shows `host_eq` without a name. `eq_autoeq` is true only when the state came from AutoEq.
   Nothing is written to the configuration.
3. **`Daemon::shutdown` stops the servers (MCP, web) before the host.** The host's last act is to
   publish an empty view, which an open page would hear as `host: null`; because the servers are
   already down, a page keeps the last line it heard, dimmed, while the stream is lost, and no
   theme drops its Now playing card, nor Signal its player and computer hops.
4. **The frame's `anchor(el, { fixed })` takes a boolean, or a function asked at each placing**, and
   places the menu button against the window (the frame carries the class `fixed`; see
   [the frame](frame.md)). On a phone Signal's strip is pinned at the top, so the button stays over
   its slot at every scroll. Signal passes `fixed: () => view.phone()`; Ember and Paper call
   `anchor(el)` and place it in page coordinates.

[Back to web themes](web-themes.md).
