<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Paper

Paper is the page's second theme, shipped in 0.31.2. It draws the same model as Ember as a sheet of
squared paper: a printed serif for the device's own words, a pencil for what you may change and
for what needs care, and blue ink for handwritten notes about what the numbers mean. It has a
light paper and a dark one. It is plain ES modules with no build step, in
`bins/svolderd/web/themes/paper/`, and one entry in `bins/svolderd/src/web/themes.rs`. It is built
on the core alone: no file of Ember's was touched, and the few gaps it found in the core were
fixed in the core (the last section). [Web themes](web-themes.md) says what a theme is; [the
frame](frame.md) says what the page keeps for itself.

## The files

| File | What it holds |
| --- | --- |
| `paper.js` | the registration, the menu rows, the mount: wires the sections to the store |
| `live.js` | `createLive`: draws the sections, keeps them current, holds one while a hand is on it |
| `draw/` | one file per section: `sheet.js` (the skeleton, title, footer, `SECTIONS`), `volume.js`, `tone.js`, `controls.js`, `presets.js`, `playing.js`, `amp.js`, `device.js`, `notes.js`, and the small `word.js`, `esc.js` |
| `*-wire.js`, `graph.js`, `lists.js`, `ask.js`, `gestures.js` | the hands: what a tap, a drag, a key, a list or a question does |
| `ruler.js`, `changes.js`, `notes.js`, `marks.js`, `words.js`, `place.js` | pure maths and words: the ruler, the last change, the notes, where they sit, every fixed word |
| `act.js` | `createAct`: sends a write through the core and logs what this page asked |
| `driver.js` | the test driver |
| `preview.js` | the picture on Paper's picker card |
| `style.css` | the tokens, the sheet, every section, the phone |

Pure modules take the model and return text or numbers; the drawing modules return markup; the
wiring modules are the only ones that listen. Paper reaches a device only through `core.actions`.

## The layout

The sheet `#paper` is at most 1180 px wide and drawn on a grid of two faint 1 px lines. It has a
title, a card, the volume, two columns, the notes band, the device and a footer.

- **Title.** The device's name in the print face at 26 px; under or beside it, in italics, its
  kind, link, input and sample rate (`DAC, connected over USB 1 at 48 kHz`). A guest's line adds
  `read only: sign in to change this`; a page with writes off says why in the daemon's words.
  With two or more devices, `also here: Amp` is an underlined word that shows the other one. The
  menu button is the ink circle at the right end of the title line.
- **Card.** Empty unless the device is out of reach or faulted (see States).
- **Volume.** The ruler, the needle and the written controls (below).
- **Columns.** Left: the tone, the amp's power, the meters. Right: the controls, Now playing, the
  presets, the device's presets. Each starts with a small mono label in capitals.
- **Notes band.** The handwritten notes (below).
- **The device.** A heading that is a button (underlined like the other written words, with
  `aria-expanded` and `aria-controls`), and under it, folded until the person opens it, a table of
  facts (product, unit, firmware, DSP, profile, link, signal, passphrase), every reading with the
  age it was heard, and `identify`. The folded part stays in the page (`hidden`), so the ages keep
  ticking. The last choice is held in memory (`fold.js`), so it works when storage is refused, and
  is also the theme's preference `deviceOpen` in the core's store for Paper (the same one that
  keeps the scheme), so it survives redraws, device switches and reloads. A guest may fold it too,
  since it changes nothing on the device. The test driver unfolds it before it reaches a fact.
- **Footer.** `svolder <version> · <day month>`, `T for themes` taken from the core's key list, and
  `sign out · Paper` (a guest sees `sign in · Paper`).

At 720 px and wider the columns are 1.4 to 1 with a 40 px gap. Under 720 px the page is one
column: the volume is a large italic number over a shorter ruler, the notes follow the section
each is about, and the type is smaller. Nothing scrolls sideways at 390 px, with a 40-character
preset name, a long track title, a device named with a space and a slash, nine presets or six Loki
Max sliders. Paper keeps no print stylesheet, ignores the strip and Ember's visualizer, and draws
no effects.

## Written controls

Every control is a written word. An **underlined word acts**: tapping it does what it says. A
choice's current value has a 2 px pencil underline; the other values have a faint dotted one, so
they read as options. A word with a list behind it (the tone mode, a preset, a long choice, the
slot to save over) opens a paper list of the options; the arrows, Home and End move in it, Enter
picks, Escape closes and puts the focus back on the word. Only one list is open at a time.

For a guest, and for a page with writes off, the sheet draws the same words with no underline
and marks them `aria-disabled`. A tap sends nothing and shows the core's toast (`Sign in to
change this`, or the read-only words).

Paper binds no page keys except `T`. Every control is reached by Tab; the needle, the scales and
the graph's points are sliders with their own arrow keys.

| Kind | How Paper writes it |
| --- | --- |
| choice, few options | the options joined by dots, the current one underlined in pencil |
| choice, many | `label:` and the current value, which opens the list |
| toggle | `on` or `off` as a word that flips |
| slider | a printed scale with ticks, a pencil mark and its value; drag or keys; a level with a centre has a `centre` word |
| readout | text only, faint |
| action | the word itself |
| meter (amp) | a small printed scale, the class-A share hatched, a pencil needle at the reading, its words and state beside |
| power (amp) | `Power: on`, `off`, and `warming up` in italics while it warms; its switches as toggles |

## The ruler

The volume is a ruler with a pencil needle. The ruler is linear in the device's steps, so the
needle moves the same distance for each step, whether the device's decibel scale is even or not
(the Gungnir 2's runs from −96 to 0). Maths in `ruler.js`:

- `fractionOf(volume, raw)` is the needle's place, `(raw − lo) / (hi − lo)`; `rawAt(volume,
  fraction)` is the nearest step, rounded and held to the range.
- Labels sit where the decibels cross each multiple of 10 (20 when the ruler is under 600 px
  wide), found by interpolating the model's `volume.db` over the steps. The bottom mark of
  `volume.marks` is always labelled. A level with no decibels is labelled from `volume.texts`. A
  label is dropped when it would sit within 3 % of the one before, and never closer than 30 px.
- The risk zone (the model's red zone) and the cap are hatched on the ruler in pencil.

The needle is a slider. Dragging it writes one step at a time through `core.actions.setVolume`
and the daemon's volume rule, which holds the volume to the cap and glides a big jump. Paper
clamps the ask to the cap before it shows or sends it, so the needle stops at the cap. A touch
during a climb stops the climb first. The arrow keys move one step, PageUp and PageDown ten, Home
goes to the bottom; there is no End, so no key jumps to full volume. The written words under the
ruler are `− quieter`, `mute` and `louder +`.

While the hand is on the needle, a reading from elsewhere waits; it lands when the hand lets go.
After the device has answered an ask, a reading that differs from it is another hand's and is
shown at once.

## The graph

The tone section plots the EQ's curve in an SVG that uses the core's plot box
(`viewBox="0 0 1000 260"`), so the core's maps turn a pointer into a frequency and a gain. The
0 dB line is dashed, the curve is a 2 px print stroke, and each band is a round button, a point.

The drag is the core's: `core.drag` (`createDrag`, `grab`) with the write adapter in
`core/tone-drag.js` (`bandSteps`, `BAND_PARAM`, `bandWriter`). A drag moves the band's frequency
and gain; the wheel on a point steps its Q; the arrow keys step the frequency (left, right) and
the gain (down, up). A touch grabs a point only from its own 44 px area, so the page still
scrolls past the graph. A lost stream cancels a drag.

The band table under the graph writes every parameter: an underlined `−` and `+` around each
frequency, gain and Q, one device step a tap. It is the way to reach a Q without a pinch, and the
way the test driver reaches every band.

While a point is held, the tone section is drawn from the held graph and a tone update waits;
released, it is drawn from the newest state.

## The lists and the questions

Presets and the device's presets are written as names joined by dots. Your own are in the
current-name style; `save as…` and `rename or delete…` appear only for a viewer who may write,
and `rename or delete…` only when you have a preset of your own. The device's own slots are
listed, with `save the bands to…` when the device can store them, and the built-in settings.

Paper asks in its own words, drawn under the section, one at a time:

- a **form** for a name (`save as…`, rename): a ruled input of at most 40 characters, answers
  `save` or `rename`, and `cancel`. A refused answer leaves the words live and the typed text in
  place.
- a **question** for what loses something: `Delete “name”? The bands on the device stay as they
  are.` with `delete` and `keep`, and `Save the bands to 2? What the device holds there is
  replaced.` with `save` and `keep`. The safe answer, `keep`, has the focus, and Escape means it.

A slot save posts `"confirm": true` only when the question is answered `save`.

While a list, a form or a question is open its section is held, so a part update cannot close it
or take its focus. Every write goes to the device that was shown when the ask opened.

## The notes

A note is a short line in blue ink in the hand face, tilted a little (one of five tilts chosen
by a hash of its id, so it is steady from one draw to the next). Notes say what the numbers
mean. The words are written in `notes.js`; a note exists only when the model has the fact.

| Note | About | Its words |
| --- | --- | --- |
| volume notes | volume | the model's own (a cap, a risk), not the ones that are popups |
| peak | tone | the model's words for the clipping peak and where it clips, `peak +3.3 dB: above −3.3 dB, loud tracks may clip`; left out when the volume's own note about the peak is red, which says it (else that volume note is left out) |
| EQ | tone | by mode: `flat: the bands are bypassed`; `loudness, strength 3`; `EQ on, every band at 0 dB`; or the preset's name and the two biggest bands, `Warm: +3 dB at 60 Hz, −2 dB at 12 kHz` (`EQ:` in place of the name once the bands are edited; the tone heading then reads `Warm, edited`, as Presets does) |
| path | playing | the rates, `48 kHz · rate matches all the way` with an ink tick, or the rates with a drawn arrow between them (`44.1 kHz to 96 kHz`), then the path's words |
| track | playing | `0:30 of 20:00 · Album` (a part with no clock shows no time) |
| firmware | device | `firmware 11.2 · DSP 3` |
| link | device | `link` and the signal |
| power | power | one note per readout, `<label>: <text>` with the label lower-cased (`warming up: no`) |
| presets | presets | the model's presets note |
| passphrase | device | the model's passphrase note |
| last change | by its field | see below |

The notes' arrows are hand-drawn strokes in inline SVG, hidden from a screen reader, each
followed by a visually hidden ` to `, so a reader hears `44.1 kHz to 96 kHz`. No arrow character
is used anywhere, because the fonts lack it. A model's own arrow is turned into the same stroke.

**The red note.** At most one note is red, the most urgent one, in this order: the device's card
(a lost link or a fault), a red volume note (a risk or a cap), the EQ's clipping peak, a path
that changes the sound, the passphrase note. The path note carries its red words only when it is
the red one: `DAC: resampled, the only hop that changes the sound`, or `2 hops change the sound:
DAC and Amp`. The card's red note is drawn once, in the card under the title.

**Placement.** At 720 px and wider the notes sit in a band under the two columns, a grid of three.
Each note takes the third its section's centre is over, in note order. An ink arrow runs from the
EQ note to its section and a pencil arrow from the red note to its section, except that when both
are about the tone only the red note's is drawn; the red note is circled by a pencil ellipse, the
card's included. The volume stands above both columns, so the red note's pencil arrow to it leaves
the note's top, runs along the gap under the columns to the sheet's margin on the needle's side
(the right margin while the needle is in the right half of the ruler, else the left), climbs the
margin and turns in to end beside the written reading. It never crosses a section. Under 720 px
each note follows its section in the flow, the red note's first three words get a pencil ring, and
there are no arrows. Notes are placed again when the window or
the fonts change.

**The last change.** The volume, the mute, every choice and toggle, the tone mode and the
recalled preset are watched. A note says `last change: −23.5 dB to −20 dB, 14:32`, `tone flat to
EQ`, `preset Warm` and so on. The rules:

- the first reading and a replaced model (another device, a reload) are not changes;
- changes of one field within `RUN_MS` (3000 ms) are one change, so a drag reads as one;
- `, from this phone` or `, from this browser` is added only when this page sent that field that
  value and the change was seen within `MINE_MS` (3000 ms) of the request, answered or not
  (`phone` when the screen is coarse-pointer);
- when several fields change together one is told, in a fixed order: preset, mode, control,
  volume, mute.

The log of what this page asked is the last fifty requests in `act.js`, each with the time it was
asked and answered and whether the answer was good. svolder keeps no record of who changed what,
so a change from another tab within 3 s of the same ask can read `from this browser`.

## Light, dark and Auto

Paper has two papers, light and dark, and `schemes: ['light', 'dark']`. The menu's Paper rows are
a heading and radios `Auto`, `Light` and `Dark`. Auto follows the system; Light and Dark are
kept in this browser by the core. `?theme=paper.light` and `?theme=paper.dark` set the choice
from the address. The server paints the chosen scheme in the first byte, so dark paper never
flashes light: the cookie `svolder_theme` carries `paper.dark`, the page opens with
`data-scheme="dark"`, and the browser's own colours for the page are the dark ones.

The tokens are in `style.css` on `:root[data-theme="paper"]`: the light block, the same dark block
under `prefers-color-scheme: dark` guarded by `:not([data-scheme="light"])`, and again under
`[data-scheme="dark"]`.

| Token | Light | Dark |
| --- | --- | --- |
| sheet | `#f3efe6` | `#1e1f22` |
| print | `#23201b` | `#ece8df` |
| print, dim | `#6b6458` | `#a8a398` |
| print, soft (footer) | `#4a443a` | `#c9c3b8` |
| pencil (needle, red note, current underline) | `#b3261e` | `#f2786b` |
| ink (handwriting) | `#2b4a8a` | `#8fb0f5` |

The grid, the hatch and the frame's tokens (`--frame-*`: its button, surface, ink, lines,
accent, scrim, shadows) are set in both papers; every text colour reads at 4.5 to 1 or better on
its sheet. The dark paper is a neutral cool charcoal so the ink stays blue. The picker's card for
Paper is always light paper.

## The fonts

Three faces, all self-hosted from `bins/svolderd/web/fonts/`, so the page still makes no request
elsewhere:

- **Crimson Pro** Regular and Italic, the print. The Crimson Pro Project Authors' font, under the
  SIL Open Font License 1.1, cut into two Latin subsets (Latin 1 and Latin 2) of WOFF2.
- **Caveat** Medium, the hand. The Caveat Project Authors' font, under the same licence, cut the
  same way (instanced from the variable font at weight 500).
- **IBM Plex Mono**, for the ruler's numbers and the labels, already served for Ember.

Neither new face has a Reserved Font Name, so subsetting needs no renaming. Their licence texts are
`OFL-CrimsonPro.txt` and `OFL-Caveat.txt` beside the fonts, and in `THIRD-PARTY-LICENSES` for the
packages. `frame.css` declares the faces, so the picker can draw Paper's card while Ember is on
show. The faces carry no arrow, no tick, no `✓ ⏮ ⏭ ▾` and no caret, so Paper draws those as
strokes or words: a tick is an ink stroke, the transport is `back`, `pause` or `play`, `next`, and
an opener is told by its underline. The minus sign (U+2212) is in both faces.

## The states

- **Stream lost.** The title's line becomes `Reconnecting to svolder…` in pencil, the sheet fades
  to 45 % and is inert.
- **Waiting** (a page shown again after being hidden). The line reads `reading the device again…`,
  the sheet is at 70 % and inert.
- **Resting.** Nothing visible.
- **A device out of reach, or a fault.** The card's title is the red note under the title, in
  pencil; its detail is in print, then the faults with their advice, `try now` (underlined, when
  the daemon offers it) and `trying again in 12 s` from the daemon's `retry_in` (`trying again
  now` at 0). A page with no device draws only the card.
- **A refused write.** The core's toast shows, and the value springs back from the store's re-read.

A section held by a hand is not made inert under it, so a drag is not cut off; the stillness is
applied when the hand lets go.

## The menu rows, the button and the preview

The button is the ink circle (`{ icon: 'ink', label: 'Menu' }`) anchored to the title line's end.
The rows are the heading `Paper` and the radios above. The preview is a small sheet of lines and
a pencil needle, drawn by `preview.js` and always light. Paper needs `meters` and `host` only: no
visualizer, no analysis, no strip.

Browser colour and the scheme's paint belong to the frame and the core, not to Paper; the page
meta for each scheme and the toast's shadow token are described in [the frame](frame.md).

## The checks and the mutants

- **Node** (`scripts/web-checks/node/`): `paper-tokens.mjs` (contrast of every token pair in both
  papers, the two dark blocks agree, every frame colour is set), `paper-words.mjs` (the title, the
  date, only the characters the faces carry), `paper-live.mjs` (the skeleton, the store's events,
  held sections, the log, `place`), `paper-ruler.mjs` (the ruler's maths, the volume section),
  `paper-draw.mjs` (every section from every fixture, guests quiet, the transport, the amp, the
  facts, the card), `paper-notes.mjs` (the notes, the red order, the last change), `paper-fold.mjs`
  (the fold of The device, with storage that refuses).
- **Browser** (`paper.py`, Chromium and Firefox): the button, the menu and Auto, dark from the first
  byte, the needle's value against its place, drags and keys, the cap, a reading from elsewhere
  during a drag, the controls, the presets and their questions, the transport, the amp, the
  states, a second device, no sideways scroll at 390 px with long words, the notes' placement
  and the red ring, and contrast of real notes in both papers.
- **The contract and completeness passes** run Paper through its driver. The driver
  (`themes/paper/driver.js`) reaches a target by its written word, operates it by a tap, arrow
  presses on the needle or scale, or a pick in the paper list, and reads the words back. Both
  passes take `WEB_CHECK_THEMES`, a comma list of themes; unset they run every listed theme, and
  an unlisted name fails the pass.
- **Mutation runners**: `node/mutate_paper.sh` (13 mutants of the pure sources: a ruler not
  linear in the steps, a place that rounds down, a place not held to the range, labels that may
  touch, two red volume notes, the peak redder than the volume, a run of changes that is no run,
  a request that is this page's for a minute, the first reading counted, two changing hops told as
  one, the EQ's words smallest first, a rate that loses its decimals, a retry at nought that
  waits) and `mutate_paper.py` (5 mutants of the page: `place` setting nothing, the system's dark
  overriding a person's Light, a held section drawn anyway, a log that stays empty, a meter's
  needle at the class-A share).
  Each must make a check fail.
- **Stills**: `make paper-shots` draws the DAC and an amp at desktop and phone width, light and
  dark, into eight PNGs.

## What Paper found missing in the core

Paper was built on the core alone, and building it found seven gaps in 0.31.0's core. They were
fixed in the core, in their own commits, before Paper's sections used them:

1. **The track's album was only inside `source`.** An album may contain the separator, so Paper
   could not split it out. `album` is now a field of the host's track and of the model's `track`.
2. **One browser colour per theme, and `color-scheme` ignored a chosen scheme.** A dark paper
   needs a dark browser bar. A theme has `theme_color_dark`, the page carries a `theme-color` for
   each scheme, and the core's `chromeOf` and `paintChrome` keep the metas in step with the
   scheme.
3. **A switch wrote the cookie with the old theme's scheme,** so dark paper reloaded light first.
   A switch now rewrites the choice from the mounted theme's own scheme.
4. **At boot the stored scheme overrode the scheme the server painted** (`?theme=paper.dark`, a
   cookie). The painted scheme is now the choice at boot.
5. **The toast's shadow was a value made for a dark page.** It is now `--frame-toast-shadow`, and a
   test fails if the frame draws with anything but tokens.
6. **The band steps and the write adapter a drag needs lived in Ember's `tone-graph.js`.**
   `bandSteps`, `BAND_PARAM` and `bandWriter` are now in `core/tone-drag.js`. Ember keeps its own
   copies, as no Ember file was touched this release (a known issue).
7. **No font for a serif or a hand.** Crimson Pro and Caveat are served from `fonts/` and
   declared in `frame.css`.

Building the rest of Paper (the ruler, the notes, the controls, the checks) found no further gap
in the core.

[Back to web themes](web-themes.md).
