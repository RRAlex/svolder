<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's stages in the terminal UI

How the tone screen's computer side is built; what each key does for a user is in the
[TUI guide](../usage/tui.md#the-computers-stages), and the rest of the TUI is in [tui.md](tui.md).

## The strip and the panel

On the computer's side the tone screen is the **strip** and one **panel**, not the band table
(`screens/tone/stages.rs`, state and keys in `app/stages.rs`). The strip is the one row over the
panel: a stop per stage the controller serves (`Stage`: EQ, Loudness, Space, Bypass), each with its
dot and level, the open one reversed, then the total. Every number and word comes from the view
(`EqView::levels`, `bypass`, `loudness`, `crossfeed`, `speakers`): the TUI works no level out. The
panel is the open stage's rows: the Points table, the Loudness rows, the Space rows or the Bypass
row, drawn with the row builder of `screens/tone/crossfeed.rs` (`Built`) and painted by `put_rows`.

Where the keys are is `tone.row`, one value: the header (`Mode`), `Strip(stage)`, or a row of the open
panel, which reuses the rows the stages already had (`Point(i)`, `Ld(row)`, `Crossfeed(row)`) and
adds `Bypass`. `App::open_stage` says which panel shows: the stage of the row the keys are on,
else the one they were last in (`tone.stage`, a `Cell` the getter keeps, so a frame can ask), the EQ's
once a stage is gone from the view. `stage_key` (`app/stages.rs`) is tried before the screen's own
keys in `App::on_screen`: it takes `f`, `L` and `X`, the strip's keys and the panel's, and leaves
every other key to the screen. A panel's keys send the ops the older section keys sent (`xf_*`,
`ld_*`, `space_*`, `computer_mode`), plus `HostEqOp::Bypass` for `f`; `Tab` is `cycle_stage`. The
Plays row's `←→` move a highlight (`tone.plays`) that `Enter` applies, so a stray arrow never
rebuilds the chain. The keys' meaning is in the [TUI guide](../usage/tui.md#the-computers-stages);
its mockups are tests (`screens/tone/tests/docs.rs`).

The layout is fixed by the window's height: the panel is 9 rows inside its border below 40 rows, 13
from 40 and 17 from 48, the strip is the row above it, and the last row stays blank
(`stages::panel_height`, `screens/tone.rs::rows`). The Response block takes what is left, so the
graph never loses a row to the panel, and `render::draw` draws a toast as one line on the blank row
(`toast::draw_row`) instead of a box that would cover the panel. The panel's border is heavy
(`border::THICK`) in `device::border_style`, the frame the keys are in, while the keys are in a row of
it, and thin otherwise. The device screen's Tone pane keeps its own rows: the Loudness row and, with
crossfeed playing, Model and Preset rows (`screens/device/tone_pane.rs`); `f` there is the bypass too.

## Keys with a side

A key whose meaning depends on the side (`f` is the flat mode on the device's side and the bypass
on the computer's; `Tab` steps the panes or the stages) has a hint for each (`f`: `When::DacEq` and
`When::OnComputer`; `Tab`: `When::HasEq` and `When::OnComputer`), and the one that does not apply
is hidden or dimmed. `keys::this_screen` adds the keys that act where the keys are now on the
computer's side: the help overlay lists them first, under **This screen**. The uppercase `L` and
`X` are named `toggle ... panel`: pressed in the panel they open they go back to the strip, so the
rule that uppercase keys toggle holds.
