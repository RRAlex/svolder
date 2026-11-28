<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The Device controls popup

`core/popups/controls/` is the controls of the page by kind, for a theme that draws a small dash
and no sheet of its own (a visual theme that has the volume, the mute and the player's buttons in
reach and nothing more). The popup is everyone's: the router opens it for a guest and for a reader
too, and it draws every control inert for them. Its title is *Device controls*. It has no row in
the menu: it is listed with `row.menu: false` ([popups.md](popups.md), "The list of popups"), and
a theme opens it from a person's gesture with `core.router.open('controls')`. It closes with a
change of the shown device (no `stays`: it is about the one on show), through Escape, Back and its
close button as any popup does, and it wants nothing of the stream.

## What it draws

There is no side list. The page pane holds a switch and the sections of one side, each in a place
of its own that the wire puts alone (`frame.put`), so a control keeps its element and the focus:

- **The switch** *Device | This computer*. It only chooses what is looked at
  (`core.computer.flip`; nothing is written). It is drawn where the page has a computer's side. When
  the computer cannot be used, its pill is inert and the page says why in the daemon's words.
- **The device's side**, exactly what `targetsOf(device)` of `core/contract.js` lists, by kind. The
  power key with its switches and readouts; the volume with `−` `MUTE` `+` (a press and hold
  repeats through `core.repeat`: one step at the press, then after 380 ms every 70 ms, up to 10 s;
  muted reads `−∞ dB`); each control of the model by its kind (a choice's pills, a toggle, a
  slider's `−` and `+`, a readout as a row); the tone mode; the loudness strength while the mode is
  loudness; each band's frequency, gain and Q between a `−` and a `+`; the tone presets (recall;
  *Save as…*, *Rename…* and *Remove…* ask in place through `core/inline-ask.js`); the device's own
  slots (recall, or *Save…* the bands over one after a question) and its built-in settings; and the
  player's buttons while there is a track. A section the model does not offer is left out; a device
  with nothing to operate says so.
- **The computer's side**, exactly what `computerTargetsOf(page)` lists: the volume with `−` `MUTE`
  `+`; the mode (Off, EQ); the points' table (a type menu, a field each for frequency, gain and Q, a
  `×`, *Add point*) with the line that counts the points and the preamp; the presets the computer
  can play; the bypass; and the levels of the filter chain, read-only, in the daemon's words. For a
  writer a *Sound…* button opens the Sound popup: loudness, crossfeed, virtual speakers and the EQ
  library are its tabs, and this popup never redraws them.

Every word is the model's (the daemon's); the popup writes only its labels. The look is the
`fp-*`, `fi-*` and `fpx-*` classes of `core/frame.css` and the `--frame-*` tokens, so it is the same
under every theme and both schemes, wide and on a phone.

## Writes

Every write goes through `core.actions` (`core.actions.computer` on the computer's side), each call
naming the device the popup is about (`make theme-css-check`, rule 6, holds the wire to it).

- A viewer who may not write (a guest, or a page whose writes are off) sees every control drawn
  inert: `aria-disabled="true"`, never `disabled`, so a tap still reaches the wire; a point's field
  is read-only. A tap calls `core.actions.refuse()`, which toasts the core's words (`Sign in to
  change this`, or the page's read-only words), and the same words stand in a line of the popup.
- A refused write puts the daemon's words in that line (`role="alert"`); a write that landed clears
  them. A write that was dropped (a raise sent while the page was not live) or declined says
  nothing new.
- A stepper pressed again before the model has answered the last press goes on from the step asked,
  not from the stale reading; the step is held to the control's range and the volume's cap, and it
  starts from the model again once the model has moved.
- A point's field is sent on Enter (the core rounds it and holds it to the hand range); Escape puts
  back what was typed and leaves the popup standing; leaving the field puts back what was not sent.

The popup's wire is thin. A change of the model is drawn section by section; the whole page is drawn
again only when the set of sections changes (a side flipped, a mode that has a strength). While an
ask stands, a stepper is held or a point's field holds what was typed, the model's changes wait and
are drawn once the hand is away.

## The core's driver for it

`core.driver.controls` (`core/driver.js`) is test surface for the popup, as `core.driver` is for the
device's facts: no theme and no module of the core calls it at run time. It does what a person does there and
takes the targets of `targetsOf` and `computerTargetsOf` in the shapes a theme's own driver is
asked about:

| Call | What it does |
| --- | --- |
| `reach(target)` | opens the popup through the router, chooses the side the target needs, scrolls to it; `true` when it is drawn and in the window |
| `operate(target, how)` | taps, types or picks: `how` is `{ step: ±1 }`, `{ raw }`, `{ value }`, `{ on }`, `{ side }`, `{ save }` or `{}` |
| `shown(target)` | the words the popup shows for it (a switch reads `on` or `off`) |
| `state(target)` | `{ pressed, inert, said, toasts }`: whether a switch is on, whether it is drawn inert, the popup's line of words and the toasts standing |

It refuses, without a throw, with `false` or `null`: no device shown, a target the page does not
list, and a viewer the router turns away. A theme that leaves the controls to this popup keeps its
own `driver` as the one the checks call; only that test driver hands the targets it does not
draw to `core.driver.controls` (no theme calls it at run time). The driver lives in the core and not in the popup's folder, so nothing
under `core/popups/controls/` is requested before the popup is first opened.

## Files and checks

- `draw.js`: the model in, a string out. Pure. Its header names the attributes a control carries
  (`data-act`, `data-key`, `data-band`, ...), which the wire and the driver find it by.
- `wire.js`: `wire({ core, frame })`. No key listener of its own: the stack hands the keys to
  `onKey`.
- Both are in `ASSETS` of `bins/svolderd/src/web/assets.rs`.

`node/core-popups-controls.mjs` holds the draws (every target of `targetsOf` and of
`computerTargetsOf` has its control; inert markup; escaping), the wire over jsdom with the core's
actions faked and recorded (each tap calls the right action naming the device; the asks; the flip
writes nothing; a guest's tap and a refused write say their words; Escape and Back; a change of the
shown device; a stepper that goes on from the step asked; a model change that waits for an ask) and
the driver. `node/core-path-words.mjs` holds `core/path-words.js`, the pure module that names
what the path says about the sound (the stages of the filter chain, the computer's flag, the DAC's
own stage), which Night drive's top line reads. `popups.py` has a row for the popup in `POPUPS` for
the cases every popup shares, in the four themes at 1280 px and 390 px. Its extra pass reaches every target of the
fake DAC through the driver and reads what the fake daemon holds after each write (its own model, not what
left the page), then a volume step the loop did not set; and in every theme, on This computer with three
points, the pane does not scroll sideways and every point field lies inside it at 1280 px and 390 px (a point
is a wrapping `fi-form` row, never a table).
