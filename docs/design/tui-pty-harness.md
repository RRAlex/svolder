<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The TUI in a pseudo-terminal

The TUI's own tests drive its loop on a ratatui `Recorder` backend and rasterise `Buffer`s for the
screenshots. None of that runs the real `svolder tui` binary in a terminal, so raw mode, the
alternate screen, mouse reporting, what a terminal writes back and a resize were never exercised.
`make tui-harness` does: it starts `svolder tui --demo`, the fake device on a daemon of its own
with no host audio, in a pty of a given size and reads what it draws. It is one of
[the gates](testing.md#the-gates), opt-in and not part of `make check`.

## What it is made of

- **A pty** from `libc` alone (`posix_openpt`, `TIOCSCTTY`, `TIOCSWINSZ`), the dependency the
  workspace already has: `bins/svolder/tests/tui_pty/pty.rs`. The child leads a session of its own
  with the slave as its terminal.
- **A screen parser**, `vt100` 0.18.1 (MIT, a dev-dependency of `bins/svolder` only: nothing it
  adds is in a shipped build). `libghostty-vt` is not used.
- **A raw-byte tap** beside the parser (`tap.rs`): it keeps every byte the program writes, so a
  case can look for the kitty graphics APC (`ESC _ G`) and the sixel DCS (`ESC P … q`) the parser
  drops, and it recognises the queries a program may write and answers them as a **Responder**
  says. The parser is fed in step with the program: a synchronized update (`CSI ? 2026 h` … `l`)
  reaches it whole, as a terminal draws it, so a wait never reads half a frame.
- **The harness** (`harness.rs`): `Tui::start(cols, rows, Responder)`, `send_keys`, `send_bytes`,
  `send_mouse` (click, drag, wheel; SGR reports), `resize`, `wait_for` (polls the parsed screen,
  woken by every chunk, up to a deadline, never a fixed sleep), `screen_text`, `cell`, `find`,
  `raw`, `queries`, `answers` and `quit`, which proves a clean exit: status 0, the alternate screen
  left, mouse reporting off, the cursor shown, the demo's private directory gone.

The Responders are `None` (silence, as some tmux and SSH set-ups are), `Plain` (DA1 without
attribute 4, and `CSI 5n`), `Sixel` (DA1 with 4, the cell and window size for `CSI 16t` and
`CSI 14t`) and `Kitty` (the graphics queries answered `ESC _ G i=<id>;OK ESC \`, the sizes as `Sixel` gives them).
Both graphics responders also report the pixel size in the pty's window size, as the picker of
ratatui-image falls back on it; `Plain` and `None` report none. The queries it knows, among them
all those that picker writes (kitty probes, DA1, `CSI 16t`, `OSC 11 ; ?`, cursor reports, `CSI 5n`),
are in `Query`; the TUI's cover-art probe asks once at start-up, and
`start_up_draws_no_stray_bytes_and_reads_no_key_under_every_responder` pins the queries it writes
and that no answer is read as a key.

## Rules for a case

- Every case is `#[ignore]`d, so `cargo test` stays fast; the target runs them with `--ignored`.
  The tap's own tests are ordinary tests.
- Everything runs under one temporary directory (every XDG directory, `HOME`, `TMPDIR`), over the
  fake transport, with `SVOLDER_AUTOEQ_URL` at a dead address; `--host` is refused. `--web` makes the
  demo serve every interface on a port of its choosing, behind a token. The QR of the Web UI box
  is drawn only for that network reach, so that one case keeps it for its length; the other web
  case narrows it to loopback as soon as the box is open.
- Wait for the state, not the clock. Keys sent during the intro end its wait, so a case starts at
  `wait_ready`; a box the animation layer dissolves leaves rows of it for several frames after its
  last words are gone, so a case that reads rows back waits until they read as they did (the
  `static_rows` helper); a QR arrives over several frames.
- One wait rule: every wait is a condition on the screen or the program (the screen shows X, the
  program exited) under the one deadline `WAIT` (120 s, reached only by a failure), never a short
  deadline of its own and never a sleep that stands in for a condition. `find` waits for its text
  before it looks for it, so a case does not read the screen the instant after a key. The only
  short waits are the ones a case means: a text that must stay away (the tooltip that must not
  show), a retry of a gesture whose window is real time (the double-click), the quiet second that
  says a picture has stopped arriving (`settled_art_cells`), the timed samples of the squirrel's
  entrance and exit (what a case checks is that no stray cell changes in that window; that the
  squirrel has landed is a wait on the screen) and the 200 ms bound on the start-up medians
  (a measured time, the one assertion on the clock). A run's processes are found by the whole name
  of its home, so a run never stops a sibling's (`svolder-pty-7-2` is a prefix of
  `svolder-pty-7-27`).
- A key that the TUI may not read is sent again: `quit` sends `q` and, when no question shows
  within 8 s (`RESEND`), sends it again, all under `WAIT`. Only a key that is idempotent is
  resent this way: a second `q` on the open question quits, as Enter does, so a late first `q`
  does no harm. Keys that change state (a toggle, Enter on a row, a typed letter) are never
  resent; a case waits for the screen to show their effect instead. The TUI sometimes not reading
  a key sent just after another input is an open issue of the TUI.
- `finish` (what `quit` ends in) also checks the main screen holds no text of a terminal reply
  and that no process of the run (its daemon) is left.

## Wall time

The cases take about 30 s of wall time in the test binary (`make tui-harness`), about the same
beside a parallel cargo build.
