<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's controls: the flip in each theme

The second page of [the computer's controls](host-control.md): how each web theme shows the flip
between the device's controls and the computer's, and what each theme draws for the computer.


## Ember

The switch is `#part-switch`, a part of its own beside the shown device's name in the header: the
device's name and `This computer` in the mode track's style (`.seg`), swapped out of band as the
link's dot is. Where the computer cannot be used it carries the reason. It is drawn for any shown
device, an amp too, while the model has the computer's controls (`draw/computer.js`).

On the computer side `core.computer.view()` hands the drawing the core's `volumeOf` and `toneOf`;
the screen is the DAC page's layout, an amp's too, and flipping back restores its own.

- **Volume.** The same `sv-knob` over a model of 0 to 100 % (labels 0, 25, 50, 75, 100; no zone, no
  cap tick); `data-computer` sends its steps and mute through `actions.computer`. The first paint
  reads `—`; a level above 100 % shows as read with the dial at its end.
- **Tone control.** Mode `Off | EQ`. The curve's points are tapped on (an empty spot adds a PK),
  dragged (frequency and gain) and set for Q by the wheel, a right drag or a pinch: the core's hand
  (`core.eq.points`) makes the gestures and a flip ends them; `computer.js` forwards the events.
  While a point is dragged the part's swap waits (`keep.js`) and `computer-page.js` moves the curve
  and cells; the part is drawn again from the host's model when the writes are answered. The point
  in hand has a `-` and `+` per value (`computer-steps.js`): a hold repeats, a final write ends.
- **The order** of the points is the daemon's: `HostEqState.filters` is kept sorted by frequency (a
  stable sort) when an edit ends (a drag's last value, a typed value, an add, a preset recalled or
  loaded) and never during a drag. The wire's 1-based point numbers are that order; the point's
  `slot` is its identity: a front end that holds a point across an edit, its own or another's, finds
  it by the slot, and sends it with every write. The web core keeps a drag's holds by slot, looks
  the number up again before every write and ends the drag with no write when the point is gone; it
  tells the themes where a held point went with the `moved` event (`from`, `to`). The TUI does the
  same for a pointer drag and for its focus.
- **The table** shows six rows of Type, Frequency, Gain, Q and `×` (labelled `Remove point N`; on a
  lone point it sets the point to 0 dB and keeps its frequency, type and Q) and scrolls; `N more ↓
  scroll` counts the rows below. The dragged point's row is lit and scrolled into view. A value is
  typed into its cell (`2.5k`, `+3,2`), the type is a choice of PK, LSC, HSC. `+ Add point` adds a
  PK at the widest gap; the line reads `7 of 16 points · preamp −6.0 dB`. 16 rows are as tall as 7.
- **Presets** are the shared list, with `Save as preset` (the points on show), and rename and delete
  for a viewer's own; one the EQ cannot play is disabled with its reason. A preset a connected
  device plays too is put as the core's question (`core.ask`).
- The switch's halves have stable ids (`side-dac`, `side-computer`): a flip from the keyboard keeps
  the focus on the chosen half across the redraw. A point's marker sits on the summed curve at its
  frequency (`toneOf`, shared by every theme), so a shelf's dot is on the line.
- Every computer control carries `data-comp` and no `data-json`, so the DAC side's handlers leave it
  alone; for a guest each is `aria-disabled`, and the switch alone works.
- **The stages line and Bypass** sit under the mode track in `.stages`: `p.chain` holds each stage
  with its level (`EQ −7.8 dB · Loudness 60 % · Space speakers −7.0 dB`, a stage that is off says
  `off`) and the daemon's total (`levels.words`), each item carrying its trailing dot so a phone's
  line breaks at a ` · `. `Bypass` is a `.seg`-style toggle (`data-comp="bypass"`, `aria-pressed`,
  written by `actions.computer.setBypass`), `aria-disabled` for a guest; while nothing plays it is
  dim (class `idle`) and a writer's tap on it is still sent, and the daemon's refusal words are
  shown the usual way. A stage word (`data-comp="stage"`) opens the Sound popup at its tab: EQ the
  library, Loudness, Space the virtual speakers while they play and the crossfeed otherwise. A
  guest sees plain words. The levels come from `core.computer.levelsOf` and `bypassOf`, laid over
  the computer's view by `withStages`. The driver's `findComputer` has `{computer: 'bypass'}`.

## Paper

The switch is a line by the title, `showing: the DAC · this computer` (`the amp` on an amp's page),
the side shown underlined in pencil as Paper marks every chosen word, the other dotted. When the
computer cannot be used the line is replaced by the sentence of why. The ruler reads 0 to 100 %
(labels at 0, 25, 50, 75, 100, a tick every 2 %, no zone and no cap) with the percent beside the
needle; before the first read the number is `—` and the needle is not drawn. The tone is written
`TONE · off · EQ` with the mode chosen underlined; an EQ whose state is only what its file says is
drawn with a dashed underline and a line saying so. The curve has a pencil point for each point;
under it a table (`#`, `type`, `frequency`, `gain`, `Q`, `×`) shows six rows and a half and scrolls
(`N more below`, counting what is not fully in view); below it `+ Add point` and the line `7 of 16
points · preamp −6.0 dB`. The table keeps its height, so sixteen points never grow the sheet: the
add word's place stays when it goes.

The hands are `computer-wire.js` (the drawing is `draw/computer.js`, `draw/volume.js` reuses the
DAC's ruler through `percentVolume` in `ruler.js`). A point is dragged on the graph through the
core's `core.eq.points` hand; a tap on an empty spot adds a PK at that place (on a full table it
says `16 is the most`); the arrow keys step a focused point and the wheel its Q, each a burst with
one last value; a cell takes typed text (held to the hand ranges) or the up and down keys; the type
word opens Paper's list. The row and the mark of the point in hand are drawn selected. A flip ends a
drag through `computer.track` (the last value is sent, nothing else) and runs the same closing of
lists and holds as another device shown.

On an amp's page the switch is drawn too, and the computer's side is the DAC page's own volume, tone
and presets sections (the amp's power and meters stay); the device's notes are left out. Paper's `×`
and `−` are in its faces, and the table's down hint is a drawn stroke, not an arrow glyph.

The chain line sits under the tone's heading, in pencil words: `CHAIN · EQ −7.8 · loudness 60 % ·
space speakers −7.0 · plays −14.8 dB in all · bypass` (a stage that is off says `EQ off`, `loudness
off`; the total is the daemon's `levels.words`). Each part carries its dot in one unbreakable span,
so a wrapped line never starts with a dot. Each stage word opens the Sound popup at its tab (Space:
the virtual speakers while they play, else the crossfeed); a guest sees plain words. `bypass` is
the toggle (`aria-pressed`, underlined while the bypass is on, written by
`actions.computer.setBypass`), quiet for a guest; dim (class `idle`) while nothing plays, where a
writer's tap on it is still sent and the daemon's refusal words are shown the usual way. The
driver's `findComputer` has `{computer: 'bypass'}`.

## Signal

The flip is the line's own hop: the computer's hop is named `This computer`, says `PipeWire ·
<rate>` and, while it is the hop chosen, `controls below` (or why the controls cannot be used).
Choosing it is the flip: the side is the computer's exactly while that hop is chosen, any other hop
writes `dac`, and a load with the computer kept (and usable) chooses the hop (`keepChoice`, `sideOf`
in `line.js`). On an amp's page the line has the hop too while the computer is served.

The area below is the DAC's usual screen acting on the computer (`draw/computer.js`,
`computer-wire.js`): the level in % with `−`, Mute, `+` and the track (`—` until read), the curve
with a dot on each point (on the curve, at the summed response at the point's frequency), the mode
`Off | EQ` as chips, the points' table (six rows in view, `×` labelled `Remove point N`, `+ Add
point`, the line `7 of 16 points · preamp −6.0 dB`), the presets with Save as, Rename and Delete,
and, last, the facts of the route that are not controls: the sound card and its format, the other
streams and the verdict on the sound's way (the daemon's words name PipeWire there, not the
computer). Nothing below the line says `computer`, but for the path's own words (its `computer: …`
hop line, `computer volume N %`, `computer muted`). A preset the DAC plays too is asked in Signal's
own card, through its `asker`.

The stage strip sits under the mode chips: each stage with its level (`EQ −7.8`, `Loudness 60 %`,
`Space speakers −7.0`; a stage that is off reads `off`) and the daemon's own total words (`plays
−14.8 dB in all`, `pass-through …`); with no words, no dot and no total. A stage word opens the
Sound popup at its tab (the Space word at Virtual speakers while they play, else Crossfeed); a
viewer who may not write sees plain text. A `Bypass` chip sits with Off and EQ: pressed while the
bypass is on, dim while nothing plays, and a writer's tap on it still asks the daemon, whose refusal
shows in the usual place. The strip's pieces never break inside a stage; on a phone the line wraps
at the dots. The driver's `findComputer` has `{computer: 'bypass'}`.

## Night drive

The switch is two keys, the shown device's name and `This computer`, in a slot of their own left of
the menu button (`#nd-switch`, by `#nd-slot`); on a phone it is in the scene's top column, under the
device line (held sideways, at the left). They are drawn for any shown device, an amp too, while the
model has the computer's controls, and where the computer cannot be used the sentence stands under
them, in the switch's own place. They are not the dash's: the dash fades to a fifth with the HUD
after 15 s on a desktop and on a phone held sideways, and the keys stay whole; the right of the top
line (the verdict, the status words) stands under the keys and the EQ line under it. The keys keep
their ids (`nd-side-dac`, `nd-side-computer`), so a keyboard flip keeps its focus across the redraw.

The core's `computer.view()` hands the drawing its `volumeOf` and `toneOf` on the
computer side only. The scene, the top line, the dash's transport and the fade do not flip; the rest
reads the computer's, in the DAC page's own parts, so an amp page flipped to the computer has the
volume keys, figure, mode, points and presets of a DAC page and its own power key and gauges still.

- **Volume.** The outrun figure reads `42` with `%`; muted keeps the number, dimmed; before the
  first read it is a dash and the keys are quiet, never a live 0 %. The dash's `−` `MUTE` `+` step
  and mute the computer (`actions.computer`), a held key repeats as the device's does.
- **EQ line.** `OFF`, or `EQ` with the preset's name, and no gains list; a state only the EQ's file
  says is marked `unconfirmed`.
- **Stages and Bypass.** The EQ line reads `EQ Bass · SPACE −7.0 · −14.8 dB` (the Space stage while
  it plays, then the total; in a pass-through, a climb or untouched, the daemon's words in place of
  the total). A `BYPASS` key stands in the dash right after the EQ key (pressed while on, dim while
  nothing plays, disabled for a guest; on an upright phone on the dash's second row). The EQ and
  SPACE words open the Sound popup at their tab for a mouse press or the keyboard; under a finger
  the line stays click-through (the Sound popup stays one menu row away). The driver's
  `findComputer` has `{computer: 'bypass'}`.
- **Mountains.** The bubbles are the points, each on the curve the points sum to (a shelf's too);
  the lit point has the bell's diamonds, a drag of one sets its Q. A press grabs a bubble and moves
  its frequency and gain, the wheel and two fingers set the Q, and a tap on an empty spot adds a PK
  while the curve is up (the morph at one half or more); a first touch only shows the curve. The
  held bubble is on the curve all along a drag; a flip ends the drag and writes only its last value.
  With the EQ off the curve, the bubbles and the diamonds are drawn at 45 %: the points it would
  play, not playing. The computer's tone is worked out once per model change or held value
  (`tune-wire.js`), not per frame.
- **Sheet.** Mode (`Off | EQ`), Points (the table of six rows that scrolls, `N more below ↓`, each
  value between a `−` and a `+` that step it (a held one repeats) and a field that takes a typed
  one, the type as a choice, `×` labelled `Remove point N`, `+ Add point`, the line `7 of 16 points
  · preamp −6.0 dB`) and the shared presets with `Save as…`, rename and delete. A phone shows a
  point as a block, each value named, with 44 px keys. A preset a connected device plays too is
  asked in the presets' own place in the sheet (`Apply`, `Keep`).
- Nothing below the switch says whose controls these are. For a guest every control is quiet and
  `aria-disabled`; the switch alone works.
