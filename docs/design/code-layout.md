<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# How the code is laid out

The tree is arranged for someone who wants to change one thing: a key, how a screen looks, a
device's controls, a Home Assistant entity. From [the map](../architecture.md) and the folder
names alone they should reach the right file, and the files they never need (the decoration,
the fake device) should be visibly apart. The checklist a contributor follows is in
[CONTRIBUTING.md](../../CONTRIBUTING.md) ("Where new code goes"); this page says why the tree
is the shape it is.

## The rules

- **A file does one job.** A file that does several is split along them, so each file's
  opening paragraph can say what it is for in two sentences. A long file with one job stays
  whole: `bins/svolderd/src/writer.rs` is long because the door every write goes through has
  many cases, and splitting it would scatter one rule over several files.
- **A function does one job.** A function that does several is split along them, so its
  opening line can say what it does without an "and" that joins unrelated steps. A long
  function that is one linear list stays whole: a table of keys, a `match` over the intents a
  key can mean, each arm one case of the same decision.
- **A family lives in its folder.** The folder's name says what its files have in common, and
  a reader can skip a whole family they do not need.
- **The parent is the table of contents.** The module file of a folder (`x.rs` of `x/`, a
  crate's `lib.rs`, a program's `main.rs`) lists its children, one line each, so a reader learns
  a folder by opening one file. Rustdoc renders the same list.
- **One module-file style.** `x.rs` sits beside `x/`, never `x/mod.rs`; a module's tests are in
  `x/tests.rs`, or at the foot of the file when they are few. The one exception is an
  integration test's shared helper, `tests/common/mod.rs`, which Cargo would otherwise build as
  a test of its own.
- **A shared test helper sits as near as it can.** A helper several modules' tests use lives
  in the narrowest `test_support.rs` that covers all of them: a crate's, the TUI's, the
  loop's. It is not copied into each test module, and it is not moved up to a wider module than
  its users need.
- **A large test module is a parent listing its topic files.** When a module's tests outgrow
  one file, `x/tests.rs` holds the helpers several topics share and declares one topic file per
  subject, each with a line saying what it covers, and each topic file's opening paragraph says
  what its tests are about.
- **The narrowest visibility.** An item is visible to exactly the modules that use it, so the
  visibility itself says how far a change can reach.
- **One name for one thing.** There is no alias or re-export kept for an older path, and a
  module's type names, words in comments and test names agree with the module's name. The
  crates' root re-exports (`svolder_ipc::Client`, `svolder_profiles::Profile`) are their front
  doors, not aliases.

## The families

**The CLI.** `bins/svolder/src/main.rs` starts the program and dispatches; `cli.rs` is the
whole command line as clap types; `front_door.rs` is what bare `svolder` does; each verb group
is a module under `commands/`. The setup wizard's state machine is `wizard.rs` at the crate
root because two front ends drive it: a plain terminal (`wizard/cli.rs`) and the TUI
(`tui/wizard.rs`, with each step drawn by `tui/screens/wizard.rs`).

**The TUI.** Four layers, each a folder, and the same name may appear in several of them for
one thing:

- `app/` is the TUI's state with no terminal in it: what it has heard, what is focused, and the
  intents a key or a click turns into. The glide of the volume (`app/glide.rs`) is here and not
  among the widgets, because it is app logic paced by the safety rule: a press, a drag or a held
  key becomes intents only as fast as the glide allows.
- `run/` is the event loop and what it does for each part: connecting, reopening, the web box's
  codes.
- `screens/` draws: one module per screen or box.
- `widgets/` holds the drawing pieces several screens share: rows and bars, meters, the
  footer, text cells, scroll marks, graphs.

Besides them, `boxes/` holds the state of the boxes that open over a screen (the device editor,
the edit line, the presets, the Settings categories); a box is drawn by its screen, except the
toast and the tooltip, which draw themselves over whatever screen is up. `decoration/` holds
what the design calls decoration (the effects layer, the intro, the squirrel, the transitions,
the title meter, the wordmark). Its promise is that it never carries the only copy of a fact: a
frame's text is the same with every effect off. That is why `audio_level.rs` (how loud the
music is) is not in it: the Status pane and the Controls pane read it too.

**The daemon.** `bins/svolderd/src/main.rs` opens with the daemon at a glance, one line per
module. The MQTT bridge is `mqtt_bridge`, split by what its task does: the connection, the
discovery, the state it publishes and the commands it takes. `http_slot` holds one of the two
HTTP servers (the web UI or the MCP server) and is the one place the steps that bring it in line
with its configuration section are written, so a fix reaches both.

**The web page.** `bins/svolderd/web/` has two kinds of folder. `core/` is what every theme
shares: the stream, the model, the actions, the preferences and the frame (the menu button, the menu,
the picker, the popups). `themes/<id>/` is one theme: its module, style sheet, drawing, effects and
test driver, and nothing another theme imports. The core never imports a theme, and a theme
reaches the core through the object it is handed on mount. One theme is one folder and one entry in
`bins/svolderd/src/web/themes.rs`; [web-themes.md](web-themes.md) says what it exports.

**The crates.** `svolder-ipc` is three groups: `socket/` (what crosses the socket and both its
ends), `config/` (the configuration file, its parser and writers), `checks/` (the host checks
`doctor` runs), with the modules several of them share at its root. `svolder-profiles` keeps the
tone control's maths in `tone/` (presets, the response curve, fitting, Equalizer APO text). It
is called tone and not eq because "the tone control" is the device's and "the host EQ" is
PipeWire's, and "eq" would name both. Its conversions say what they are: `units` (a raw step in a
person's units), `db_tables` (the dB each raw value stands for), `codec` (a status to a named
reading, a user's words to a command); beside the volume rule in `safety`, `roles` says which
control plays which part.

**The fake device.** The library is `svolder-fake`: a device that answers real protocol frames
in memory, behind the fake transport (`SVOLDER_TRANSPORT=fake`, `make demo`) and the tests. The
program `svolder-sim` serves
it as a real Bluetooth peripheral; its `peripheral.rs` is the GATT service and the advertising,
and its `main.rs` the arguments.

## What keeps it true

- `make docs-check` fails when a Markdown file, or a comment in a Rust file, names a repository
  path that does not exist, so a move cannot leave a stale pointer behind (a span that is not a
  path in this repository goes on an allow list in `scripts/docs-check.py`, with its reason).
- The parents' tables are reviewed with every change to a folder; a new child is one more line.
- `scripts/safety-tests.list` names the tests that guard the volume rule by path, and
  `make safety-tests` fails when one goes missing, so a move of a safety test is always seen.
