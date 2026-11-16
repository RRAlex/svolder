<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Night drive: the sheet

The third page of [Night drive](web-nightdrive.md): the sheet, the dialog over the scene that holds
everything about the device. Words such as the scene, the dash and the mountains are the first
page's.


`DEVICE ▴` or the menu's `Device…` row opens the sheet, a dialog over the scene. While it stands the
scene and the dash under it are `inert` and dimmed (the scene goes on drawing), the focus goes in,
and the ✕ button, Escape or a tap on the scene closes it and gives the focus back to what opened it.

- **On a desktop** (720 px wide and more and 500 px high and more) it is two columns, `1.15fr 1fr`,
  at most 1100 px or 94 % of the width, with `--nd-sheet-top` and `--nd-sheet-bottom` margins above
  and below it. Row one: **Mode** (a chip for each mode the device has; while Loudness is on its
  chip carries a round `−` and `+` and the strength under its name) and **Input**. Row two: the
  **band table** (typed values with `−` and `+`, one device step a tap, a held one repeats) and the
  **presets** (chips; Save as, Rename and Delete ask their question in place), the device's slots
  and its controls (balance and sliders as scales, toggles as pills), an amp's meters with their
  zones. *Device settings…* stands across both columns and opens the device's page in the Devices
  popup (its facts and Identify are that page's); a viewer who may not write sees `Devices…`,
  read-only.
- **On a phone held upright** the sheet slides up from the dash to 30 % from the top, one column:
  Mode, Input, presets, bands, slots, controls, Device settings…, scrolling inside.
- **On a phone held sideways** (under 500 px high) it slides in from the right, `min(420px, 60vw)`
  wide, the same column.

The dash on a phone held upright is a translucent panel over the foot of the scene
(`rgba(8,3,16,.7)`; the road shows through, and its text keeps 4.5:1 over the brightest of what is
behind it) in two rows: `−` `MUTE` `+` `EQ` and the transport on one, then the track with the input
and `DEVICE ▴`. Its keys are 36 px tall with a 46 px hit area. At 360 by 780 the panel is 111 px, 14
% of the window, and the scene above it 86 %; the checks hold it under 30 %. The scene runs behind
the panel the whole height. The volume figure is in the scene, `min(60vw, 20dvh)` wide, under the
device line and the EQ line, which shows its values there. The switch is one pill in the scene's top
left corner, mirroring the menu button at the top right: `DAC │ PC` (an amp's `AMP │ PC`), 36 px
tall in the button's 44 px row, each segment 44 px to the hand, the side shown lit; each segment
keeps its whole name (the device's, `This computer`) as its accessible name, and the keys, the
arrows and the core's `switchOf` are the desktop's. The device line starts to its right (at 360 px:
the pill 8 to 103, the line 111 to 296, the button from 308); a verdict (`computer controls
unavailable: …`) stands under the pill and the HUD makes room for it. Held sideways the strip is one
row, under 25 % of the height, and the pill is in the same corner with the device line to its right.
A desktop keeps its two keys left of the menu button, with the device's whole name and `This
computer`.

**The volume from the scene** (`volume-keys.js`). The wheel over the big number steps the volume as
`+` and `−` do (the device's own step, 1 % on the computer side; up is louder); anywhere else on the
scene the wheel is the mountains' (the Q), and the first wheel over a number that has faded only
wakes the HUD. `↑` and `↓` step it when nothing has the keyboard focus and no sheet, question or
menu is open; held, they repeat after the core's pause at its pace, and they wake a faded dash like
any key. `←` and `→` are not read. The arrows are the one key of the whole page in Night drive's
key table (`key-table.js`): the mount binds the table through `core.keys.bind` and
`volume-keys.js` holds what a press does.

The presets are what the model offers: a preset made by AutoEq is one of the chips. There is no
AutoEq picker in this theme; adding one is a new feature for every front end.

Every write goes through `core.actions`; a question that asks before it writes (delete a preset,
replace what a slot holds) has the safe answer focused, and an answer the daemon refuses leaves the
question open with what was typed. A slot save posts `"confirm": true` only after the question is
answered. The sheet is drawn again from every answer, so a section in use is frozen (`core.freeze`:
`createLive`'s `freeze`/`thaw`, counted by `keeper`): a value being typed, a list open, a question
asked, a stepper held, and what came meanwhile is drawn from the newest model when it is let go.

The bubbles are not focusable. The band table is the keyboard path to every band value.
