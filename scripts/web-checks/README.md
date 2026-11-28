<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# The web UI's checks

Node harnesses (`node/*.mjs`) and browser checks (`*.py`, in headless Chromium and Firefox) for the
web UI's effects layer: the effects menu and its levels, the Glow level, the smoke behind the page
and the notes that form in it, the volume knob, the Tone pane and the Now playing card. The smoke
is read with `readPixels` and float readbacks, as numbers; no check takes a picture of it. Each
check runs against a throwaway fake daemon (never the installed binaries, never ports 8737 or 8738)
and keeps its whole output in `target/web-checks/out/`. The drivers' ports (geckodriver's
WebDriver and BiDi ports, Chromium's debugging port) are picked free, so two runs can share a
machine.

[COVERAGE.md](COVERAGE.md) maps each rule to the check that proves it.
[docs/design/testing.md](../../docs/design/testing.md) says where these checks sit among svolder's
other tests, and [docs/design/themes-and-effects.md](../../docs/design/themes-and-effects.md)
describes the layer they cover.

## Run it

```sh
make web-checks-task    # the quick gate with the logic-only checks in Chromium only: a task or a lane end
make web-checks-quick   # about 38 minutes at 6 jobs (much longer with WEB_CHECKS_JOBS=1): the page changed, the smoke did not
make web-checks         # 47 minutes in 0.32.0, over an hour on a busy machine: everything, before merging any change to the web page
```

One check on its own, from the repository root (build the daemon first with `cargo build -p
svolderd`; the browser checks start `target/debug/svolderd`):

```sh
node scripts/web-checks/node/director.mjs     # a node harness: no browser, a few seconds
python3 scripts/web-checks/haze.py chromium   # a browser check: chromium or firefox
python3 scripts/web-checks/notes.py firefox look   # some take a section name; see the file's header
```

Every check prints one line per assertion, `ok` or `FAIL` with the numbers it read, and exits 1 on
any failure. `run-all.sh` is the entry point of both make targets: it runs every check, prints one
line per check with its time and last output line, then a total.

## Requirements

`node` with `node-jsdom`, `python3` with `python3-websockets`, `python3-numpy` and `python3-pil` (apt packages),
`chromium`, `firefox` and `geckodriver` (the browsers may be snaps), and a Rust toolchain to build
`svolderd`. `run-all.sh` names anything missing and stops before it runs a check.

## Which gate runs what

- `make web-checks`: every node harness, every browser check in both browsers, the gate refusal;
  before a merge, and whenever a smoke file or one of the smoke's own checks changed.
- `make web-checks-task`: the quick gate's set with the logic-only checks in Chromium only; the
  rung for a task or a lane end (the ladder is below).
- `make web-checks-quick`: the node harnesses, the checks that leave the smoke alone (effects menu
  and layout, the Tone message, the ring click, the curve's drag, Now playing, the visualizer,
  devices, a Loki Max's presets, the Firefox preload) and `smoke_quick.py` per browser; after a
  change to the page, the styles or the templates that does not touch the smoke.

### The gate ladder

Which rung to run, from the smallest. The estimates are the targets of the speed work, the times
of a run on a quiet machine appear in `target/web-checks/timing.tsv`.

| Rung | What to run | About |
|---|---|---|
| A task (in the implementer's own worktree) | `make web-touched` names the checks the change affects (below), and at CODE DONE a web lane runs `make web-checks-affected` beside it (below); `cargo fmt` and `clippy` on the touched crates, `cargo test -p <crate>`, `make web-snippets`, the node harnesses (`node scripts/web-checks/node/NAME.mjs`, seconds each), and the browser checks the change touches by name, in Chromium: `python3 scripts/web-checks/NAME.py chromium`. For a whole web change `make web-checks-task` | 1 to 6 minutes by name; the task gate about 30 minutes; `make web-checks-affected` about 10 minutes for a small change, more for a change to the popup list |
| A lane end (the review) | `make check`, `make web-checks-task`; the checks that changed, in both engines by name | 10 to 20 minutes plus the task gate |
| A merge to main | `make check`, `make web-checks` (the full gate: every check, Firefox included; a pool check whose inputs did not change is `cached`, see the result cache), `make host-e2e` once, `make e2e` once when the Bluetooth path changed | a second gate in a row runs only the serial lane (about 12 minutes plus `nightdrive_timing`: the other serial entries took 728 s on a quiet machine, against 2100 s for the whole lane before the split) and the node harnesses; a cold gate 35 to 120 minutes of web gate, plus 12 of `make check` |
| A milestone merge | the merge rung, then `make host-e2e` five times, `make preamp-lab`, `nightdrive_timing.py` and the smoke's timing on a quiet machine | about 2 hours, unattended |
| A release (a tag) | `make web-checks-cold` (the full gate with nothing reused) before the version is tagged, then the milestone rung on the export tree with its own `CARGO_TARGET_DIR`, `make repro-check`, `make packages`, `make web-shots`, `make ha-shots` (needs the Home Assistant of the HA_* variables in its help), `make gnome-shots` (a headless GNOME Shell; no outside service) | about 1.5 hours plus the CI build |

Firefox: the checks of the page's layout, its pointer events, its canvas and its fonts run in both
engines on every web gate. The logic-only checks (`space_key`, `device_presets`, `core_rows`: the `logic_only` list in `run-all.sh`)
run in Chromium only on `make web-checks-task` and in both on the quick and the full gate, so
Firefox sees every check at least at the merge rung. A mutant run never blocks a task or a lane end.

### Running them

The gates run their browser checks `WEB_CHECKS_JOBS` at a time. The default is a quarter of the
cores (6 on 24), cut to two thirds, a half or 2 when the CPU is contended: the rule reads the
kernel's pressure stall information, `/proc/pressure/cpu`'s `some` line, `avg10` under 10 % keeps the
quarter, under 30 % two thirds, under 60 % a half, more than that 2 (never fewer than 2 nor more
than 12). It is not the load average, which counts tasks waiting on I/O and so reads high on idle
CPUs. The run says which number it chose and why; `WEB_CHECKS_JOBS=N` overrides it, `1` is one by one
(the serial lane's timing parts after the others). The checks run longest first (the `weight` table in
`run-all.sh`; refresh it from a run's times when the checks change). Every check has its own daemon
home, free ports, browser profile and output files, so any two can run side by side. A check that
fails beside others is run once more alone: passing alone is `flaky` (counted as passed, named in
the total line, both outputs kept in `target/web-checks/out/NAME.out` and `NAME.again.out`),
failing again is `FAIL`.

The gate lock, `/tmp/svolder/web-gate.lock` (`WEB_CHECKS_LOCK` names another file for a test of the
gate itself), is shared by every gate and lane on the machine: a browser check holds it shared
(`flock -s`), any number at once; a check that asserts a pace holds it exclusive (`flock -x`) and so
runs with no other browser check of any lane beside it. The re-run of a failed check holds it
exclusive too, so "failed alone" means alone on the machine's gate. A check that waits more than 10 s
for the lock prints `waiting for the gate lock (held by another gate)`, and its row ends with
`(waited N s for the gate lock)`: the seconds in a check's row and in its verdict are its own, the
wait is kept out of them (`lockwait.sh`, proved by `gate_wait.sh`); the lock is not fair
(shared holders keep arriving), so a long wait means another gate's pool is running. On both gates
these run one at a time in the exclusive lane after the parallel part: `nightdrive_timing.py`
(Night drive's draw cost, its draws and floor rows a second, the budget's rate rule, its phone budget and the tap under a slowed CPU; every case of it is a timing one), `notes.py` whole (its note cases depend on real input pace and smoke timing) and the timing part of each
script in `split_scripts` (`run-all.sh`): `smoke_core.py`, `smoke_page.py`, `haze.py`, `effects_cycle.py`,
`scroll.py`, `smoke_quick.py` and `sources.py`. Each of those runs twice: `--part logic` (the cases about state, geometry, dye sums and notes formed) in the
pool like any browser check, cached, label `NAME-ENGINE`; `--part timing` (the cases that read a frame rate, a wall-clock pace or a
frame gap) in the serial lane, cold, label `NAME_timing-ENGINE`. A script run by hand with no `--part` runs both. A timing check that fails is run once more alone at the end of the serial lane: passing is
`flaky` (listed in the total line, never cached), failing again is `FAIL`; the lane waits for no quiet
machine. A TERM or Ctrl-C stops every running check, and their browsers and daemons with them.

#### The serial lane's timing parts

`parts.py` takes `--part logic|timing|all` out of the command line (default `all`) and answers
`part_has(kind)` and `counts(timing)`; a check's `check(..., timing=True)` is counted only in the
timing part (and in `all`), every other case only in the logic part. No case is dropped and no limit moved:
the two parts' counts add up to the whole file's.

| Script | The timing part holds | Why it reads the clock |
| --- | --- | --- |
| `smoke_core.py` | the hidden-tab page: the test canvas, the calm rate band, no frames while hidden, the first frame's `dt` after showing, the loop running on; and `back from away: frames resume` (more than 5 frames in 1 s) | frame rate and frame gaps, with the hidden waits |
| `smoke_page.py` | `calm_rhythm`'s own-handle half: the calm rate below the red zone, more than 40 frames a second in it | frames a second against a band |
| `haze.py` | the wide page's calm rate (20 s untouched), and the stroke's pace against the recorded baseline | a rate, and dye along a stroke at set seconds |
| `effects_cycle.py` | the calm rate after ten rounds and the draw calls per simulated frame before and after | rates over 2 s windows |
| `scroll.py` | the `desktop` and `touch` sections: velocity after wheel steps and a pan a set time apart, the burst's peak speed, the calm rhythm after | velocity decays on the wall clock |
| `smoke_quick.py` | the calm rate | frames a second |
| `notes.py` (not split: the whole script is in the serial lane) | every case | notes form from real input pace and smoke timing; its "logic" cases (calm notes, thin-smoke stir, the 0.2 s amp peak, ordinary moves settling) failed in a loaded pool |
| `sources.py` | the 4 coalesced moves 12 ms apart (3 pushes), the 300 ms pause then a 20 ms move | the page's own clock against the browser's event times (a timer 90 ms late loses a push) |

Everything else of these files runs in the logic part. When unsure, a case was kept in the timing part.
`scroll.py`'s smear section reads the dye after waiting for two smoke frames (`two_frames`), so a
stalled frame cannot read before the scroll is applied. `smoke_page.py`'s `lost link: back, frames
resume` (more than 5 frames in 3 s) stays in the logic part: its floor is far under the loop's rate.

Cases per script and engine, passed + failed, main's whole file against the logic part + the timing part
(measured 2026-11-17; every pair is equal):

| Script | Chromium | Firefox | Timing part's seconds (Chromium / Firefox) |
| --- | --- | --- | --- |
| `smoke_core.py` | 79 = 70 + 9 | 77 = 68 + 9 | 33 / 36 |
| `smoke_page.py` | 60 = 57 + 3 | 55 = 52 + 3 | 10 / 12 |
| `haze.py` | 32 = 28 + 4 | 32 = 28 + 4 | 56 / 59 |
| `effects_cycle.py` | 20 = 18 + 2 | 20 = 18 + 2 | 24 / 26 |
| `scroll.py` | 24 = 10 + 14 | 20 = 10 + 10 | 20 / 16 |
| `smoke_quick.py` (quick and task gates) | 4 = 3 + 1 | 4 = 3 + 1 | 11 / 13 |
| `sources.py` | 45 = 43 + 2 | 41 = 39 + 2 | 6 / 8 |
| `notes.py` (whole, serial) | 69 | 69 | 200 / 222 |

### The fake clock

An assert that counts what the page's own timers and frames do in a stretch of time (draws in 5 s, writes
in 3 s) reads the machine's load as well as the page's code on the real clock. `fakeclock.py` gives such a
case a clock it moves itself: `install(b)` before the page loads (Chromium
`Page.addScriptToEvaluateOnNewDocument`, Firefox a WebDriver BiDi preload script), `take(b)` once the page
has booted and settled on the real clock (it then freezes `performance.now`, `Date.now`, `setTimeout`,
`setInterval` and `requestAnimationFrame`, the timers and frames already waiting move over with the time
they had left), and `advance(b, ms)` to move it, a frame every 1/60 s and every due timer in order, with the
page's promises and the daemon's messages let through between the steps. The count in 5 virtual seconds
is then the page's, the same on a quiet and a loaded machine (Night drive's Still: 5 draws in 5 s, the
readout's box: 6 writes in 3 s, in both engines). Fetches and sockets stay real, so a case that depends on
the daemon's pace in real time does not use it; its limits: `setTimeout(0)` chains, `new Date()` with no argument, `event.timeStamp`, CSS animations and transitions, and iframes stay on the real clock; the cases that use it run in a browser of their own, since
the wrapped timers are in every page that browser loads. The cases on it: `nightdrive_theme.py`
`the_pace_on_a_fake_clock` (`ND_ONLY=pace` runs just those, seconds).

What stays real-time, and why:

- A draw's cost, the budget's step-down and a tap under a slowed CPU are the real work, so they are real
  and exclusive: `nightdrive_timing.py` (it now holds Night drive's phone budget too:
  `a_phone_stays_usable`, the step-down at 30x and the tap that opens the menu within a second at 6x, moved
  there from `nightdrive_theme.py`), and the timing parts of `smoke_core.py`, `haze.py`, `smoke_page.py`,
  `smoke_quick.py`, `effects_cycle.py`, `scroll.py`, `notes.py` whole (not split) and
  `sources.py` (the 20 and 300 ms pointer pauses against the browser's own event times, which a page
  clock cannot move; the rest of `sources.py` is its logic part, in the pool on the full gate).
- Still in the pool, and a failure under load there is re-run exclusive: `nightdrive_theme.py`'s waits for
  the fade and the volume bubble (a sleep of the check's against the page's timer, the check's sleeps
  are lower bounds so only a page timer more than a second late fails them), `visualizer.py` ("the chip goes
  after about a second", the same shape), `signal_theme.py` (no rate assert, only `within 3 s` waits) and
  `paper.py` (its rate-a-second hits belong to the web-themes lane).

### Which checks a change touches

`make web-touched` (`BASE=<rev>`, default `main`: a lane cut from a release branch passes that
branch; `BASE=A TO=B` for the diff A to B alone) runs `touched.py`: it lists the checks the changed
files affect, both engines, each with the file that made it affected, and prints the
`WEB_CHECKS_ONLY="..." bash scripts/web-checks/run-all.sh` line that runs them. The rule is that it
never under-selects: when in doubt it lists more and says why (a `WARNING` line).

The map has two halves. A browser check records the files its browsers asked the daemon for on every
run, in `target/web-checks/deps/<script>-<engine>@<pid>.txt` (`common.py`; an asset path under
`/assets/` is a file of the web dir; one file per process, so the sweeps' child processes add up and
`touched.py` unions the files of a label). Those records are kept in the committed
`scripts/web-checks/load-map.json`, so a fresh worktree has them; a full gate run clears the
records at its start and rewrites the map at its end, when it passes (`touched.py --refresh` does
it by hand with `--refresh --replace`; plain `--refresh` only adds, so a partial run's records never shrink an entry; the merge commits the refreshed file after the full gate). Its format, which other tools may read:

```
{"licence_1": "...", "licence_2": "...",
 "web_files": ["core/a.js", ...],                 every file of the web dir when it was written
 "checks": {"nightdrive_theme-chromium": ["core/a.js", ...], ...}}   web-dir-relative paths
```

A browser check's files are the map's entry, this tree's records, its own script and the helpers
it imports. A node harness's files are its script and every file its string literals lead to
(relative to the script, under the web dir or under the repository root), followed through js
files: js, css, svg, json and rs files (the harness that reads `eq/control.rs` is listed when
it changes). A harness that names the web dir, or the root, as a base for computed paths depends on
every web file. A changed file lists every check whose files include it, and besides:

- a web file lists every browser check that loads anything from its area (`core/`, `fonts/`,
  `themes/<name>/`), so a line missing from a check's entry cannot drop it;
- a web file lists every browser check with no record (not in the map and not in this tree's
  records: a fresh worktree, a new check); the note says how many;
- a web file that no browser check has loaded (a new file, a stale map) lists every browser check;
- `common.py` lists every browser check; `parts.py` lists both parts of the seven split scripts;
- a file of the gate itself (`run-all.sh`, `fingerprint.py`, `gate_cache.sh`, `lockwait.sh`,
  `gate_wait.sh`) says the full gate is needed (`make web-checks`) and lists every check;
- another file here that no check reads and that is not a note, `touched.py` or the map prints a
  `WARNING` and lists every check.

A diff with no file under the web dir or here (a Rust-only diff) lists only the harnesses that name
a changed file, usually none: the daemon's Rust web code is not mapped. `make web-snippets` runs the
tool's own self-test on a fixture tree.

### Which checks name what a change touched

`make web-checks-affected` (`BASE=<rev>`, default `$WEB_CHECKS_BASE`, then the newest local
`release-X.Y.Z` branch behind HEAD, else `main`; `BASE=A TO=B` for the diff A to B alone; `LIST=1`
prints the labels and why and runs nothing; `TIMING=1` also runs the timing parts) runs
`scripts/web-affected.py`. It takes `web-touched`'s list and adds every
check whose script, helper or fixture NAMES something the diff adds or removes: an element id, a
popup's name or a menu row's words, an SSE event name, a quoted string of three words or twelve
characters (a note, a chip), in the files of the web dir and, for Rust, the page strings of
`src/web/` (never a test file) and the `send_json` event names. A change to a popup of
`core/popups/listed.js` also lists the checks that pin the popup list: a check whose own script
names two different popups of the list within 160 characters (it names the neighbours of the new
entry, not the entry). The words of a helper count for its importers only when three checks or fewer
import it. A label comes with its why (`token 'menu-popup-sound' in effects_menu.py`, the most
specific one). The timing parts are listed and left out of the run (`[timing]`, "owed to the gate
on a quiet machine"). The rest run in one `WEB_CHECKS_ONLY` run, both engines at once under the
gate's own lock; never wrap it in flock. Time: the replay of the 0.37.1 menu change lists about 25
labels, a 10-minute run on a quiet machine and over an hour while other lanes build; a daemon-only
change lists none. A change to a shared file such as `frame.css` brings `web-touched`'s list, which
is most of the gate.

What it never sees: a check that asserts a count or an order without naming the id (a loop to 7, a
length of 4), or a height, a band or a layout that moves without a name changing (0.37.1's note band
in tone_overlay). The lane still greps `scripts/web-checks/` for the old list by hand, as the rules
say. A token found in more than 10 check scripts is dropped as too common, and printed with its
count, so grep for it yourself. A change to a file of the gate itself (run-all.sh) still lists and
runs what the other files name and says the full gate is owed at merge.
`python3 -I scripts/web-affected.py --tokens-only BASE` lists what the tokens alone find.
`make gate-fails LOG=<gate.log>` lists every check still red at the end of a gate log with its FAIL
lines (triage starts from it).

### The result cache

Every gate (`make web-checks`, `make web-checks-quick`, `make web-checks-task`) skips a pool browser
check whose last pass is still good. A check's fingerprint (`fingerprint.py`) is a hash of: the files cargo's dep-info lists for the `svolderd` and
`svolder` binaries (`target/debug/*.d`: every crate's sources, `profiles/*.toml`, the templates) apart
from the web dir except the daemon's own `wordmark.svg` and icons, `Cargo.*` and the compiler's version
(taken once per gate, after the build; the binary's own bytes instead when `SVOLDERD`, `SVOLDER` or
`SMOKE_EMBEDDED=1` gives or serves one; a listed file newer than its binary, or no dep-info, means no
fingerprint and nothing is cached); `fixtures/`; the web files the check loads (its entry in `load-map.json` taken by
area, `touched.web_scope`: every file of the `core/`, `fonts/` and `themes/<T>/` areas the entry reaches,
plus every web file no check loads; a check with no entry, or with `SMOKE_WEB` set, hashes the whole web
directory), so a theme change re-runs the checks `make web-touched` names and no others; the check's own
file and every helper it really imports or opens here (`common.py` and the rest, followed through their
own imports; not file names in comments); the gate's own files (`run-all.sh`, `fingerprint.py`,
`touched.py`, `lockwait.sh`); the engine's name; the browsers' versions; the env that moves the daemon or
the web dir (`SVOLDERD`, `SVOLDER`, `SMOKE_WEB`, `SMOKE_EMBEDDED`) and the env that chooses a check's
cases (`ND_ONLY`, `ND_CASES`, `WEB_CHECK_THEMES`, `WEB_CHECK_POPUPS`, `HAZE_SECTIONS`,
`SCROLL_SECTIONS`, `HIDDEN_SECS`, `LOOK_*`, `SVOLDER_FAKE_*`, `SECS`, `REPS`, the stills switches). A
pass stored under the same fingerprint is reused: the check is not run, its line says `cached`, its row
in `timing.tsv` has the verdict `cached` and 0 s, and the total line names the cached checks. Any change
to any of those inputs, an edited web file included, changes the fingerprint and the check runs. The system fonts (the fallbacks `phone_sweep` and `contrast` measure) are not hashed: `make web-checks-cold` after a font package change.

The full gate runs COLD (nothing reused) when you ask (`make web-checks-cold`, or
`WEB_CHECKS_NO_CACHE=1`) and by itself when a smoke file, a file of the check tooling or the load map
changed since the last full gate that passed (`run-all.sh` keeps their hash in
`target/web-checks/cache/full-gate.state`; none yet means cold; the files are `smoke_files` plus
`touched.py`, and the message says why). A cold run is mandatory before a version is TAGGED for a
release build: the cache vouches for a merge, a tag takes nothing on trust. The serial lane (the timing parts and
`nightdrive_timing.py`, which hold the gate lock exclusive) and the node harnesses never use the cache. The
cache lives in `target/web-checks/cache`, so `make clean` drops it.

A pass is stored at the end of the gate, after the map refresh (so it carries the scope the next gate computes), and only when the Rust sources, tooling and browsers are still what they were after the build and the check's own files and the whole web dir are what they were when it started; never a pass that needed the alone retry (a flaky check is dropped from the cache), never when a subset env var is set (that run was not the whole check), and never when a browser's
version cannot be read. Records are written atomically, one file per check and engine, in
the `cache` directory under `target/web-checks`; a failed run drops its record; `rm -r` the directory to start over.
The quick gate's refusal stays by changed
files, not fingerprints: it is about what the quick set does not run (the smoke's checks read
`common.py` too, and a fingerprint cannot vouch for a check that is not in the set). `gate_cache.sh`
proves the fingerprint on a throwaway tree.

Every run writes `target/web-checks/timing.tsv`, one row per run of a check (the label, the engine,
the lane `node`, `pool`, `again` (the alone retry of a pool or a serial check) or `serial`, the verdict, the wall seconds, the CPU seconds of the
check's waited-for descendants and the peak resident memory in MB of its largest process, from GNU
`time`), and ends with the serial lane's wall time and the 15 slowest runs.

The quick and task gates refuse, with a message that names the file, says whether it is a smoke file
or the gate's own harness (`common.py`, `parts.py`, `run-all.sh`, `COVERAGE.md`, `fingerprint.py`, `gate_cache.sh`, `lockwait.sh`, `gate_wait.sh`) and names the rung to run
(`make web-checks`, the merge rung), and exit 1, when a file of the smoke or one of its own checks changed against `main`
(`WEB_CHECKS_BASE` names another base) in the commits, the working tree or the untracked files. The
list is `smoke_files` in `run-all.sh`: the smoke's scripts under `bins/svolderd/web/themes/ember/`,
`effects.js` (it starts and stops the smoke, and the quick gate does not cycle effects), the
smoke's checks and tools here, `COVERAGE.md` and the node harnesses of the smoke (`director`, `effects`, `look-values`, `model`, `notes`, `sources` and their `mutate_*.sh`; the other harnesses and the fixtures belong to the quick gate). `gate_refusal.sh` proves the refusal on a throwaway
repository. Outside a git checkout the gate runs and says it could not tell.

## What is here

- `snippets.py`: loads every `mutate_*.py` table and fails when an entry's `old` text does not occur
  exactly once in the file it names; needs only python3 (`make web-snippets`, part of `make check`).
- `node-list.py`: fails when a `node/*.mjs` harness is missing from `run-all.sh`'s node list or a listed
  name has no file (the helpers other harnesses import are named in it, each with its reason); run by
  `make web-snippets` too.
- `node/model.mjs`: the smoke's model (`smoke-model.js`): the rhythm, the sources' strength, the
  palette, the haze, the scroll push, bad input.
- `node/notes.mjs`: the notes' numbers: the agitation measure, the gate, the settle watch, volume
  bursts, placement, the three glyphs.
- `node/sources.mjs`: what stirs the smoke: the pointer's force and dye, the volume step's puff, the
  meters' emission.
- `node/director.mjs`: the director (`smoke-director.js`) on a fake clock: every rule above working
  together, away and lost, bad input.
- `node/effects.mjs`: the effects module: stored level, which level the page picks, the background
  alpha.
- `node/plot.mjs`: `core/plot.js`, the shared SVG strings of the response plot and the sparkline (frame
  geometry, curves, clipped spans, gaps, escaping), and Signal's and Paper's tone plots against their
  pre-move output in `node/fixtures/plot-move.json`.
- `node/tone-curve.mjs`: the page's copy of the tone control's curve maths (`tone-curve.js`) against
  `node/fixtures/tone-curve.json`, the core's own numbers that a Rust test writes and keeps current.
- `node/tone-drag.mjs`: the curve's gestures (`tone-drag.js`) over fake writes that answer when the
  test says: grab reach and ties, the axis hold, the snap past the plot's edges, the right drag's
  and the pinch's Q, the wheel's steps (a mouse's notch is one whatever its pixels), one write in
  flight per parameter (a value already sent is not written again), `settled`, the release, Esc, a
  lost stream, a refusal, the mode toast, the label's place, a coordinate map of the drawing's own
  that writes the same steps, and the sticky 0 dB turned off by a hold of 0.
- `node/ember-parts.mjs`: the theme Ember's drawing (`draw/parts.js`, `draw/meter-arch.js`,
  `draw/dial.js`, `draw/curve.js`, `draw/screen.js`, `draw/shell.js`, `order.js`) of the link, card,
  controls, power, meters, volume, tone, bands, presets, device presets, facts and playing parts,
  the whole screen and the page's shell from the page's JSON model, against
  `node/fixtures/ember-parts.json`, the golden copy of the DOM the server's templates drew for 45
  cases (frozen: no test writes it; its path models were edited by hand to the judge's keys in
  0.37.1 task 3.8, not re-captured). The same harness holds the claims the Rust tests
  keep of the model to the markup Ember draws (the cases named like those tests). `node/html-same.mjs` is the comparison (tags read in order; the attributes htmx
  reads dropped, attribute order and entities ignored, whitespace runs one space, JSON attributes
  and JSON data blocks as JSON, drawn numbers as written: no tolerance); the harness checks that comparison
  first.
- `node/keep.mjs`: the swap hold of the tone part while a point is dragged (`keep.js`), with and
  without `Element.moveBefore`, two overlapping holds, and the part drawn again once they go, once,
  from the newest copy the model holds (not from the swap that was held).
- `node/level-write.mjs`: a slider's writes (`level-write.js`): one in flight, the newest waiting,
  each for the device shown when it was asked, so a value that waits behind the write in flight is
  dropped when another device is shown or the slider has left, and never goes to the other device.
- `node/toast.mjs`: the popups (`core/toast.js`): the same words again leave the one popup standing
  and start its time over; a button, other words or a popup on its way out make a popup of their own.
- `node/frame-keys.mjs`: the frame's reserved key (`core/frame.js`): `T` is the frame's unless a
  field it would type into has the focus or Ctrl, Alt or Meta is held; and the two helpers Ember's keys
  ask the menu (`core/menu.js`: whose a key is, whether a node is inside it).
- `node/core-modal.mjs`: the core's modal layer (`core/modal.js`) and its two clients, the picker and the
  question, on a jsdom page (`node/dom.mjs`): the top of the stack owns every key (a theme's listeners on the
  document and the window hear none; `T` and `?` do nothing), Escape closes the top only, Tab stays in it, the
  page under the top is inert but the popups, the focus returns to `returnTo`, else the opener, else the frame's
  button, `closeAll` empties the stack, a field in a modal types and a list in it gets its keys through `onKey`,
  and the picker's and the question's own keys.
- `node/core-router.mjs`: the core's router (`core/router.js`) with the real stack and a popup on jsdom's
  history: the popup is on the stack and off it with the history whoever closes it, a page loaded again opens
  no popup of the load before, Back steps
  page → list → closed and shows no device, a step past the popup shows a device only when the entry names
  another, an entry without a device keeps the shown one, Escape steps back past the popup and Forward after
  it opens nothing, a push asked for during such a step waits for it.
- `node/dom.mjs`: not a harness: the jsdom page those two share (`page(html)` gives the window, a key sender
  and a wait for queued tasks). The apt package `node-jsdom`.
- `node/ember-order.mjs`: the order of Ember's sections (`order.js`): the default, a kept order that
  drops what the layout lacks and gives a new section its default zone, the arrow keys' moves, a
  section placed by index, and one order per layout.
- `node/contract.mjs`: what every theme must present (`core/contract.js`), read from the fixture's
  pages (every device kind, a guest's too): every control of the model is of a kind the core knows
  (`choice`, `toggle`, `slider`, `readout` and the volume, power, tone, presets and device presets
  objects), `targetsOf`, `deviceFactsOf` and `themeFactsOf` list what the model offers (the two lists
  of facts share no label and together are every fact in order), each target maps to one action
  request with today's path and body (a readout none, a slot's save and recall one each), a
  followed device's targets carry its name, and a guest's send nothing and say to sign in.
- `node/switch.mjs`: the theme on show and the switch (`core/switch.js`), over a fake head: whichever
  theme ends up mounted, the fallback included, has its sheet, metas and scripts in place first
  (metas first, scripts in order, none added twice); a sheet or script that fails or never answers
  leaves the theme on show and releases the switch; a theme and a fallback that both fail leave a
  popup that stays; a pick before the first mount and an empty `?theme=` do nothing.
- `node/facts.mjs`: the Device fold-out's page rules (`words.js`, `facts.js`): the ages as the daemon words them
  (90 s, 90 min and 36 h edges), the signal's words, the Identify toast with the daemon's gap, and
  when a read of the part is overtaken by a newer swap.
- `node/playing.mjs`: the Now playing card's arithmetic (`clock.js`, `playing.js`): the elapsed time from the
  player's position, rate and read time (none while paused, clamped to the length), the daemon-clock
  offset it is worked out against, the bar's fraction and the `m:ss` and `h:mm:ss` words.
- `node/core-api.mjs`: the page's calls to the daemon (`post`, `get`: the status with the
  JSON body, a 401 loading the sign-in page, no answer as status 0, a body that is no JSON).
- `node/core-store.mjs`, `core-stream.mjs`, `core-actions.mjs`: the core's model store (a model
  replaces everything and says so once, a part event changes one device, a reading folds in, a
  write's answer applies its parts), its one event stream (the address, the reopen after 2 s, the
  session question, the rest and the wait on waking, a device switched while it rests, a device no
  longer followed, the union of several owners' wants) and its actions (each posts today's
  request; a guest's or a read-only page's send nothing; a refusal is toasted and the model read again; the volume's cap and the raise that
  waits for a live stream; a device switch and its address), over fake streams, clocks and posts.
- `node/core-old-readings.mjs`: the frame's badge on readings from before a link dropped (the lost-link card
  or a lost stream shows it, the card going or the stream opening clears it; a resting page, a dim or fault
  card and no device shown do not).
- `node/core-title.mjs`: the tab's title the core keeps for every theme (the device, the status words,
  the parts a theme adds).
- `node/core-repeat.mjs`: the held `−` and `+` the core gives every theme (a step at the press, the
  repeat after 380 ms every 70, the ways it ends, a redraw that replaces the button, the 10 s cap),
  over a fake clock and fake events.
- `node/core-freeze.mjs`: the section keeper (`core/freeze.js`): the holds on a section counted, a
  list's apart from a gesture's, the give-back after the press, the reset, and `createLive` over a
  fake store, following the device its caller names.
- `node/core-names.mjs`: the core's pinned aliases (`core.hold`, `core.drag`): each member is the
  current name's own (`core.repeat`, `core.eq`), over a core made on a jsdom page.
- `node/core-html.mjs`: the core's markup helpers (`esc` on the five markup characters, `null` and
  `undefined` as empty; `attr` quoting, a bare true attribute, none for no value).
- `node/core-inline-ask.mjs`: the inline ask (a form for a name, a question that asks once): the classes
  and words passed are the ones drawn, typing, submit, a refused answer, cancel, one ask replacing the
  one before, and the keys handed in (`key`, `listen`, a modal of the real stack).
- `node/core-list.mjs`: the list of options next to a chip (the classes passed, `aria-expanded`, a tap
  chooses after the close, a press outside, one at a time, `below` and `auto` places, the keys handed in
  by `key`, `listen` and a modal of the real stack, the place in a popup's page pane, which scrolls).
- `node/core-volume-ask.mjs`: the volume ask Signal, Paper and Night drive share (`core/volume-ask.js`): the step
  shown, the step a change starts from, the expiry timer, and Night drive's dash in both orders of the answer.
- `node/core-clock.mjs`: the one ticker of a Device section's ages (`tickAges` of `core/clock.js`), on a
  jsdom page with a fake clock, fake timers and a fake watch of the page: the ages and the signal's age
  step with the clock on the daemon's time, a cell is written only when its words change, a row or a
  section that left is not touched, a section drawn again is aged from its own stamp, the timer runs
  only while the page is on show and goes with `stop()` and with the aborted signal of a theme's
  unmount, a gate (`when`) holds the writing but not the note of the part's arrival.
- `node/core-mounted.mjs`: the mounted instance (`core.mounted`, what a theme's `mount()` returns),
  on the real core over a jsdom page: the instance's `menuRows` are the ones the menu draws, before
  the theme object's; a theme with no instance fields, or one that returns a bare function, keeps
  the theme object's; after an unmount and the next mount the old instance's rows and key table are
  gone; a theme that throws as it mounts leaves none behind; the last focus (not the menu's, not
  a modal's, not a hidden element) and the frame's `?` (not under a sheet).
- `node/core-menu-rows.mjs`: the menu's rows as data (`core/menu-rows.js`, no page): the frame's own
  rows for a writer, a reader and a guest, in order (Device, then Sign out or Sign in); one device
  is a row that opens nothing and none is no row; the whole menu is the Theme row, the theme's own
  rows as given, a rule and the frame's; the device list is a heading and a radio for each device.
- `node/core-menu.mjs`: the keys of the frame's menu and of a modal over it, on the real core over a
  jsdom page: ArrowDown on the closed button opens the menu on the checked radio, the arrows go
  round the ends past a disabled and a hidden row, Home and End, Escape and `q` close it with the
  focus back on the button (Ctrl+q does not), a Tab out closes it and leaves the focus where it
  went, the Device row opens its list in place; a modal opened from the open menu closes the menu
  and owns every key (ArrowDown sent to the menu's button opens nothing), and Tab goes round
  inside the modal. These need a key that travels to the window and a real focus, which is why
  they are on jsdom (`node/dom.mjs`, whose header says when a harness uses it).
- `node/dom-skip.mjs`: where jsdom does not load, a harness that needs it prints `SKIPPED` as its
  last line and exits 77 (a failure for the gate); with `WEB_CHECK_ALLOW_NO_JSDOM=1` it prints
  the same line and passes, and `run-all.sh` starts without jsdom; a pure harness never skips.
  Its own case that needs jsdom is left out where jsdom does not load, and it then ends by the
  same rule, so a machine without jsdom passes the node list under the variable (the harnesses
  that need a page say `SKIPPED`, the others run) and fails it without. A run with the variable
  set is never stored in the gate's cache.
- `node/core-device-wire.mjs`: the settings of a device's page in the Devices popup, on the real core over a jsdom
  page with another device on show: each control's request, the refusals' words, the in-flight state, the focus, typed
  text kept through a put, a rename (and the shown device's), a setting changed elsewhere, the question on a playing
  device, and *Remove…*, the facts, Identify and All readings (35 cases). `node/core-device-facts.mjs` draws the
  facts and Identify from every fixture device. `devices_popup.py` is the same in a browser, in every theme, with
  one daemon per theme because Remove… leaves none of the configuration as it found it
  (`python3 devices_popup.py chromium|firefox`; `WEB_CHECK_SHOTS=<folder>` writes `<theme>-device.png`).
- `node/core-crossfeed.mjs`: the Sound popup's Crossfeed tab (`core/popups/crossfeed/`, the tabs of `core/popups/sound/`
  and the crossfeed lane) on the real core over a jsdom page and a fake host (28 cases). `crossfeed_popup.py` is the same
  in a browser, in every theme: the menu row, the tabs, geometry on each tab at 1280 and 390 px, a press -> a request ->
  the view, the keyboard, the frame tokens, a guest with no row. It lays a crossfeed view over the host model and answers
  the route in the page, so it holds with or without the daemon's route
  (`python3 crossfeed_popup.py chromium|firefox`; `WEB_CHECK_SHOTS=<folder>` writes the pictures).
- `node/core-loudness.mjs`: the Sound popup's Loudness tab (`core/popups/loudness/`, the tabs of `core/popups/sound/` and the
  loudness lane) on the real core over a jsdom page and a fake host (28 cases). `loudness_popup.py` is the same in a browser,
  in every theme: the tab order, geometry at 1280 and 390 px, a real drag of the slider, the keyboard, "Use current volume"
  and the switch, the frame tokens, reduced motion, and the daemon's own view and route
  (`python3 loudness_popup.py chromium|firefox`; `WEB_CHECK_SHOTS=<folder>` writes the pictures).
- `node/core-eq-library.mjs`: the EQ library popup (`core/popups/eq-library/`) on the real core over a jsdom page and
  a fake daemon: the search and its late answers, the preview, the import, the upload, the saved presets with
  *Overwrite with current…*, the computer's side (30 cases). `eq_library.py` is the same in a browser, in every theme,
  against a daemon whose AutoEq is a loopback server of the small fixture under `fixtures/autoeq/` (no check reaches
  AutoEq's servers; every daemon the checks start defaults `SVOLDER_AUTOEQ_URL` to a port nothing listens on)
  (`python3 eq_library.py chromium|firefox`; `WEB_CHECK_SHOTS=<folder>` writes `<theme>-preview.png`).
- `contrast.py` and `phone_sweep.py`: two sweeps over every state `sweep_states.py` visits (the page, the frame's menu,
  each popup the page lists on each entry, a device's page with its forms and lists open, a dimmed device, the add
  walk's pages, the config-changed line, the EQ library's preview, a guest's and a read-only page, a page with no
  device), in every theme and every scheme the page lists; `contrast.py` also runs once in each palette a theme offers
  (Signal's four: the first, Blueprint, keeps the tag `signal.dark`, the others are `signal.dark.<palette>`; the
  palette is the root's `data-palette`, which `sweep_states.set_palette` sets and keeps in the browser's setting; a palette's pair is held to the default palette's baseline pair and is never written to the baseline, and the not-measurable count and the allowed gaps use the default palette's key), `phone_sweep.py` once (a palette is
  colour alone, the geometry is the same). Each theme runs in a child process of its own with its own
  browser and daemons (`WEB_SWEEP_PAR` limits how many run at once). A visit fails when it does not reach a state it
  names. `contrast.py` reads the computed colours (the browser resolves the tokens) and computes WCAG 2.2 ratios: text
  4.5:1 (3:1 large), control edges, focus rings and graph strokes and fills 3:1; a ground that is a picture, a gradient or a
  canvas behind the text is sampled from a screenshot with the text, outlines and shadows hidden, at its darkest and
  lightest. `phone_sweep.py` loads the states with the browser already at 360 px (every theme, scheme and visitor) and
  at 390 and 768 px (the writer's and the guest's pages, first scheme), and reads the page's and the popup's sideways
  scroll, every box that scrolls sideways (the popup's pane and side list included), every element cut by the window or
  by an ancestor that clips, and controls under 24 px with no room around them (WCAG 2.5.8). Each has a baseline of
  what fails today (`contrast-baseline.json`, `phone-baseline.json`): a failure that is not in it, or one that got
  worse, fails the check; `python3 <check>.py chromium --update-baseline` rewrites it (a person looks at the diff
  first). Each also plants a fault it must see (a title in the popup's own colour; a 700 px element in Help's pane).
  They are in the full gate only. Run one on its own, from the repository root, with
  `python3 scripts/web-checks/contrast.py chromium`; `WEB_CHECK_THEMES=ember WEB_CHECK_WHO=writer` narrows it.
  `WEB_CHECK_PALETTE=<folder>` has `contrast.py` write the palette report (`5.5-palettes.md` and one swatch strip per
  theme and scheme in a folder `p5-shots`); `WEB_CHECK_SHOTS=<folder>` has `phone_sweep.py` write a picture of each page
  that scrolls sideways. `python3 contrast.py --self-test` and `phone_sweep.py --self-test` run the arithmetic and the
  verdict without a browser.
- `make routes-doc-check` (`scripts/routes-doc-check.py`, no browser, in `make check`): the routes the daemon registers against the
  routes' table of `docs/usage/web-ui.md`, the fields of the configured-devices, series, walk and AutoEq routes against their Rust
  types and the page's calls, and that those write routes are behind the writer gate.
- `tone_overlay.py`: the tone graph's overlay of an imported preset (the target dotted, the gap shaded, the fit in
  words; Night drive's ghost ridge, floor tint and `fit` in the HUD) in a browser, in every theme: a daemon with an
  uploaded preset the device's five bands cannot follow, every shape counted against the model route's own
  `tone.overlay`, the colours against the theme's tokens, a pointer that still reaches the points, none with a
  built-in preset, the budget dropping Night drive's layer second. `nightdrive_timing.py` holds its draw cost
  (`ND_ONLY=overlay`). `WEB_CHECK_SHOTS=<folder>` writes `<theme>.png` and `nightdrive-phone.png`.
- `node/core-device-config.mjs`: the writes to a configured device's settings
  (`core/device-config-actions.js`: rename, profile, passphrase, default, remove), through the
  real `post` over a fetch the test answers: each request's method, path and body with another
  device on show than the one named, no request when none is named, the devices answer handed
  back, every refusal as a reason, a passphrase that is in the body and nowhere else.
- `node/core-setup.mjs`: the PipeWire config popup (`core/popups/setup/`) and each theme's config
  entry (Ember's label button and chip, Signal's and Paper's chips): the draw lists the sink, the
  summary, all seven lines (failing ones tagged), the `What changes the sound` section and the limit
  in the daemon's words, escaped, and says `config not read` for an older daemon; Ember's, Signal's
  and Paper's chips and the listed row; and, on the real core over jsdom, the wire (it follows the
  `playing` part and the model in either order). `setup_popup.py` is the same in a browser, wide and
  phone, with the fake host's passing and failing config (`python3 setup_popup.py
  chromium|firefox`).
- `node/core-help.mjs`: the Help popup (`core/popups/help/`): its draws as strings (a key's row,
  Here's groups, the line for a focus that is nowhere, Everywhere, About and its links), that Help
  shows the listed entries of the zones it names and no other on the four themes' real tables
  (a writer, a guest, the focus in each zone), that every zone has a name, and, on the real core
  over a jsdom page, its wire (Here read at each show, a guest, no stream want, no key listener)
  and the `?` key (through the router, not in a field, nothing while Help stands, the menu
  closed first).
- `node/core-devices.mjs`: the Devices popup (`core/popups/devices/`). Its draws as strings (the
  side list with dots and the Shown tag, a state in words, the graphs' order, a graph with its
  text alternative, one reading or none said in words, a device that is not connected dimmed by
  a class, the Overview, a device's page). Its wire on the real core over a jsdom page, with
  another device on show than the page is about: every request names the page's device, the
  stream's address and `store.forget` at the close, a device the fleet drops, a device that
  connects or appears (focus and selection kept), a rename, *Show on the page*, the timed
  re-read, a list or a series that cannot be read, no way in for a guest.
- `node/core-device-facts.mjs`: the facts and Identify of a device's page in the Devices popup
  (`core/popups/devices/facts.js`), from every fixture device: every fact of `deviceFactsOf` with the daemon's
  words, Unit and the passphrase note only when there are some, Identify offered or not, and off with its reason
  when the device is away, the stamps the ticker reads, escaping.
- `node/core-device-readings.mjs`: the All readings disclosure of a device's page
  (`core/popups/devices/readings.js`), as strings: closed and open, the table (name, value, age),
  the value through the converter, the ages in the core's words with what the ticker reads, a
  device that is not connected, a device with no reading, every string escaped, ids that hold
  across a rename.
- `node/core-device-settings.mjs`: the settings part of a device's page
  (`core/popups/devices/settings.js`), as strings: the words of every refusal at each setting, the
  name's field, the profile's button and its choices (no base profile but a pinned one), each
  passphrase kind and the form *Change* shows, *Make default* and the default's word, each
  setting idle, sending and refused, ids that hold across a rename, every string escaped, no
  passphrase anywhere.
- `node/core-stay.mjs`: a popup that stays through a change of the shown device (`stays: true`
  in `core/popups/listed.js`), on the real core over a jsdom page (store, stack, router, actions
  and stream together): it stands through `showDevice` with and without a push, no entry is
  pushed, the entry names the new device and its address, Back steps page → list → closed on
  the new device, Forward opens nothing that was closed; a popup that does not stay and a
  question over a staying one close; the shown device removed or renamed (the fleet event).
- `node/core-popups.mjs`: the popup skeleton (`core/popups/frame.js`) and the sample popup that
  fills it (`core/popups/sample/`). The draws as strings: a hostile title and device name escaped,
  the side list's current entry, the box's `fp-at-list` / `fp-at-page` class (what the phone layout
  turns on), the loading and failed lines. The wire on the real core over a jsdom page: opening
  draws and the focus opens on the current entry, a reading redraws the live line alone and leaves
  an open ask's field alone, a redraw keeps the focus by id, closing releases the stream want and
  gives the focus back, Escape in the inline ask cancels the ask and a second Escape closes the
  popup, Tab goes round past what a layout does not show, the list's panel is in the page pane
  and takes its keys from the popup, Back closes it, no device is shown by any of these; a core nothing registered the sample
  with opens nothing, and no shipped file outside the sample's folder names it. The lazy loading
  (`core/popups/popups.js`, `core/router.js`): the frame shows at once with the loading line and
  draws when the module arrives, the loader runs once for two opens, open and close before the
  import resolves draws nothing and holds no want, a rejected import (or a module with no wire,
  or a wire that throws) shows the failed line and Escape and Back still close it, and the next
  open loads again; a part that throws in `show`, at its first show, a later open or a step, is
  told to hide (its want is released), shows the failed line, closes like any popup and is kept
  (the wire is made once); a popup without a loader whose `show` throws stands and closes too.
- `node/core-keys.mjs`: a theme's key table and its binding (`core/keys.js`, `core.keys`), on a jsdom
  page with the real stack of modals in front: an entry with `run` fires once for its key (or each
  of its `keys`), one whose `when` is false is neither bound nor listed, nothing fires over a modal,
  in a field that takes typing, with Ctrl, Alt or Meta or for a key the menu owns, a held key fires
  only for an entry that repeats, the mount's signal unbinds, two tables do not hear each other
  across a theme switch, a table that binds a reserved key or holds a malformed entry is refused at
  bind time (a key of the page with neither a `run` nor `own: true` among them), `listed` keeps
  table order, `zonesOf` gives every zone that holds an element and `zoneOf` the innermost.
- `node/ember-keys.mjs`: Ember's keys over its key table (`themes/ember/keys.js`,
  `themes/ember/key-table.js`) with the core's real binding, on a jsdom page with stand-ins for the
  dial, the balance slider, the Flat option and the presets: what each key of the page does, when
  it does nothing (a guest, a lost link, a field, a modifier, the menu, a control with arrows of
  its own, a held key), that `?` is none of Ember's keys (no key card; the frame opens Help), and
  the table's shape.
- `node/theme-keys.mjs`: the four themes' key tables (`themes/<id>/key-table.js`): the core can read
  and bind each, every entry has an id of its own, a widget's zone is a selector and its entries
  stand together, every keycap is one a keyboard has, the words are sentence case with no full
  stop, a viewer who may not write has
  fewer keys and none that writes, and the keys, zones and words are the ones written in the
  harness (the list the Help popup shows: a change of wording shows there).
- `node/nightdrive-keys.mjs`: Night drive's `↑` and `↓` (`themes/nightdrive/volume-keys.js`) bound
  from its key table by the core's real binding: one step a press, no scroll, the system's repeat
  is no second press, nothing while a control has the focus, something stands over the page, with
  a modifier or once another listener took the key; the keys go with the mount.
- `node/core-driver.mjs`: the core's test driver for the device's facts (`core/driver.js`), over fake
  theme drivers: a label of `deviceFactsOf` of the device the caller names is handed to the mounted
  theme's driver; a theme's fact (Cover), an unknown label, a target, no device named, a device the
  page does not hold or does not show and a theme without a driver are refused (`false`, `null`, no
  throw); the theme mounted now is the one that answers.
- `node/core-prefs.mjs`, `core-registry.mjs`, `core-climb.mjs`, `core-loop.mjs`: the core's
  preferences and light or dark (storage that refuses, the choice and its cookie, the system's
  setting followed until overridden), the theme registry (a theme that throws or cannot run falls
  back to Ember with a toast), the climb's popup (and a climb a touch ended) and the tone mode's
  note, and the canvas loop (on show only, `reduced: 'still'` or `'theme'`, a pace waited on a
  timer with `dt` held to twice the pace) and the analysis frames shared by their holders.
- `node/viz.mjs`: the visualizer's pure rules (`viz-colour.js`, `core/viz-model.js`): the base colour
  scale and the stereo tint at the design's values, a frame decoded from the wire shape, the
  smoothing, the 2 s max hold, which frame is drawn (the newest waiting, a late one alone still
  drawn), the clock offset from `hello`, the delay's median over 2 s, the beats scheduled ahead
  from the tempo and phase (on time behind a steady delay, never twice, none in silence) and the
  band and pixel maps.
- `node/vu.mjs`: the VU meter's page model (`core/vu-model.js`) and `core.levels`: the needle
  against the engine's golden path (`fixtures/vu-ballistics.json`, within 1e-4 at every sample), the
  300 ms rise and fall, the stops, the scale, the reference's clamp and reset, a levels-only or full
  frame decoded, the 2000 ms snap and the lamp's 1200 ms hold each at their exact edges, a stalled
  stream or a clear at rest, and the levels stream opened on `?only=levels` apart from the full one.
- `smoke_core.py`: the level shown, the cloud and the haze, loss and refusal, switches, hidden and
  away, sizing.
- `smoke_page.py`: how the page drives the smoke: a lost stream, when WebGL is probed, the menu, the
  meters.
- `smoke_quick.py`: the quick gate's one short check: it starts, stays finite, rests at the calm
  rate.
- `sources.py`: what stirs the smoke on a real page: the pointer, the dial's drag, the volume puff,
  the meters.
- `notes.py`: the notes in a real browser: the atlas, one note end to end, the look, the page's
  settle wiring.
- `haze.py`: the haze across the page: its density, the calm rate, steadiness, the palette, the pace
  of a stroke.
- `scroll.py`: scrolling stirs the smoke, the dye a scroll moves out of view returns, the resting
  source rises not.
- `sweep.py`: four pages (DAC, amp, Loki Max, lost link) at three sizes and every level load with no
  error, no warning, no horizontal scroll.
- `effects_cycle.py`: the registry and the engine across level switches and a lost context.
- `effects_menu.py`, `effects_layout.py`, `effects_devices.py`: the frame's menu with Ember's
  levels, the picker (its sheet inside a window held sideways, its grid scrolling to the last theme under a
  finger), the key `T` and the button's fade, a theme that throws, a second theme and
  back, the Device and Sign in rows; the Glow layer's place and edges; a device switch with the
  layer.
- `contract.py`: the theme contract, one short pass for each theme the page lists, driven only by
  the theme's test driver (`theme.driver`): volume up and down, mute, a band change, a preset, a
  device out of reach and back (`Daemon.drop()` then `restart()`), a write the daemon refuses and
  the words shown, a page that rests and wakes, the link state in words beside the device's name (the driver's
  `linkWords()`), the menu button and `T` also after a fade, `?theme=`
  kept by its cookie, and the same as a guest (no request, "Sign in to change this"). It pins the
  members of `core` and holds the menu button shown, at least 24 px, on top and inside the window
  on every kind of device at 1280 and 390 px. `WEB_CHECK_THEMES` (a comma list, as
  `WEB_CHECK_THEMES=paper`) limits it and `completeness.py` to the named themes while one is being
  worked on; unset, both run every listed theme, and a name the page does not list fails the pass.
- `keys.py`: a listed key is bound and a bound global key is listed, in every theme the registry
  lists, from the key table the theme gives the frame (`core.frame.keyTable()`): each key of the
  page that applies is pressed with the real keyboard and its `run` is called once (a spy over the
  entry's `run`; Ember's volume, mute, preset and Arrange keys and Night drive's arrows are also
  read off the page), keys with no entry of the page call no `run` and leave the page as it was,
  an entry whose `when` is false (a guest's keys that write, Ember's `v` with no visualizer card)
  is neither listed nor bound, and every widget entry is seen once with an element in its zone, on
  the page as it opens or in a state the check puts it in (a list, a question, the sheet, Arrange
  mode, the computer's side). It counts the key listeners too: `addEventListener` of the window
  and the document is wrapped before the page's scripts (at `EventTarget.prototype`, which
  every way of adding one ends in), and no file of a theme has more key listeners on at once
  than `LISTENERS` of `scripts/theme-css-check.py` gives it, with the binding's one in
  `core/keys.js` and none whose adder is no script of the page. A key of the page that the theme's own listener answers (`own`;
  no theme has one now) is pressed and the page's answer read. It opens the Help popup on each
  page and in each state and holds its rows to `core.keys.listed`
  (`help_shows_the_tables_and_nothing_else`). The static half is `make theme-css-check`,
  rule 5: a key a script compares a key press with is in the table, and no theme file listens
  for a key on the window or the document outside the table's binding, as far as a pattern over
  one file reads (the script's header says what it cannot see).
- `frame_button.py`: the frame's menu button is the same in every theme the registry lists (Night drive
  too): 44 by 44 px, 8 px from the top and the right, at 1280x800, 768x1024, 412x915 and 360x780, for a
  signed-in viewer and a guest, and nothing the theme draws stands under it, with the scrolled case really
  scrolling where a page can. It also checks the frame's `old readings` badge in every theme (a lost-link
  card or a lost stream shows it, named `readings from before the link dropped`, fixed bottom left and clear
  of the button; the card going takes it away).
- `space_key.py`: the frame's Space key in all four themes: on the page body one press of the player's button
  is posted (relative `api/v1/host/press`) and the page does not scroll; on a focused button that button is
  activated and nothing is posted; an open menu owns the key; in a text field it types; with no player on the page
  nothing is posted.
- `modal.py`: the core's modal layer and router in a browser, in all four themes, with a probe modal and a probe
  popup: Space, Ember's ArrowLeft and Night drive's ArrowUp write with nothing open (the control) and no key
  writes or opens anything behind a modal of the stack or behind the question, Tab stays in it, `T` opens
  nothing under a theme's own sheet,
  Escape closes it with the focus back on its opener; a device switch and a theme switch close every modal;
  Back steps the popup page → list → closed and reads no device, Escape lands on the entry the popup was
  opened from and Forward after it opens nothing, two of the router's own steps in a row land where they
  should (a real history takes both, jsdom's does not); a toast is centred up to `PHONE_MAX_PX`.
- `popups.py`: a popup's frame (`core/popups/`) in a browser, in all four themes, at 1280 px and at
  390 px: one loop over the themes and over the popups of its `POPUPS` table (the sample and
  Help, which also has its menu row, `?`, a guest, the tab's title, Back's steps, a page with no
  control and a click on empty ground; `WEB_CHECK_SHOTS=<folder>` writes a picture of Here over
  each theme), so a popup is added
  as a row of data (`title`, `folder`, `add`, `entries`, `ask`, `form`, `list`, `extra`), never as a loop or a
  check of its own. It holds what no node harness can see: nothing under the popup's folder is
  requested before the first open; the box is on top at its centre and the veil at the window's
  corners (`elementFromPoint`, over Night drive's canvas and Ember's smoke); the page behind is
  inert; Tab stays inside, past a control the layout does not show; the list and the page stand
  side by side; the box's ground and the
  title's colour are the theme's `--frame-surface` and `--frame-ink` in each scheme the theme has,
  and the themes do not all give one ground; nothing scrolls sideways; a list's panel stands
  inside the box, on top, and goes with its chip when the pane is scrolled; Escape closes an open
  inline ask alone and then the popup, with the focus back where it was and the device the same;
  Back closes it and reads no device; a press in the name field let go on the veil keeps the
  popup and its text, and a click on the veil closes it; the veil contains its overscroll (in
  Chromium a wheel over it leaves the page where it was; in Firefox, whose WebDriver wheel does
  not pass through the compositor that keeps the rule, the sheet's values are read); the box
  opens with no animation, and with the token set by the check an entry animation starts once
  an opening and runs on over the module's arrival; the router says nothing in the console (the check opens a popup by a
  real click on a button it adds for that click, and steps by a real click), and does say it for
  an open by script once the last tap has worn off; on a phone the box is the window, shows the
  list or the page and the way back. Today's one row is the sample popup, which the check adds to the core
  it holds. It imports `modal.py`'s helpers (`check`, `poll`, `load`, the page expressions);
  `modal.py` keeps the stack's own rules (who owns a key, the history's steps, a device or a
  theme switch), on its probe modal and probe popup. `WEB_CHECK_POPUPS=sample` limits it to the
  named popups, `WEB_CHECK_THEMES` to the named themes.
- `ember_link.py`: Ember's connection state as a dot after the shown device's name (an image named by
  the state's words, not a tab stop): every state's word (`not connected`, `connecting…`, connected),
  its place after a switch of two devices, a long name, and the muted volume reading `−∞ dB`.
- `completeness.py`: every target and fact of `core/contract.js` (and the computer's `computerTargetsOf`, reached by every theme) on a Gungnir 2 with a player, an
  amp with a signal, a five-meter amp, a mono amp and a Loki Max (one daemon,
  `Daemon(fleet=[(profile, name, udid)])`), in Chromium, for each theme the page lists: reached,
  operated, and the change read back from the model route; every fact shown with the model's words, by
  two drivers: the device's facts (`deviceFactsOf`: the Device section and the reading rows) through the
  core's driver (`core.driver`, given the device by its url), the theme's facts (`themeFactsOf`: the
  track's cover, shown by every theme as the picture asked for by its version) through the theme's own
  driver (`core.theme.driver`). What there is to prove is held on every theme and device (the page and
  the computer's side list targets, the theme's list of facts is not empty, the Gungnir 2 has its player,
  its transport targets and its cover), and once per theme the ages step: every cell with `data-heard-at`
  has words and a reading's age that stayed in place says other words 2.2 s later.
- `node/core-computer.mjs`: the core's computer side on fakes for the post, the read, the clock and the
  question: the side kept per viewer, the switch, a flip that writes nothing, the shelves of the curve,
  the steps, the point a tap or `+ Add point` makes, a drag's pace and its release, the value held while
  a host update arrives, a guest, every route, the volume's cap and first paint, the both-sides question,
  the typed cell and the arrow step, `actions.refuse()`, the hand ranges against the Rust ones, the
  question's `returnTo`.
- `node/core-words.mjs`: the core's words: a typed number (`2.9k`, `+3,2 dB`) and a point's frequency,
  gain and Q as a table writes them.
- `node/core-table.mjs`: the points table's helpers (rows out of view counted, also for rows in a
  tbody of unequal heights, a row scrolled into view, the scroll kept across a draw, a typed cell's
  Enter, Escape and leaving), with fake elements.
- `node/core-point-drag.mjs`: the hand on the computer's curve (`core/point-drag.js`): a drag's
  paced writes and its one last value (also at a flip), the follow of a re-sort by slot, a point
  removed or a preset loaded meanwhile, the wheel and arrow bursts (N live and one last, a re-sort
  or a flip during one, a wide wheel), a tap that adds a point, pointer cancel, two fingers, a page
  that may not write, Escape, a lost stream, tearing down; a stepper's held burst (no timer, one
  last value at the hold's end), a begin for a pinch from a diamond, a second finger that drops the
  tap, a tap on a full table, the touch listeners that come off, the held values and the label's
  words, the point in hand after a remove, a burst whose point is removed or listed elsewhere.
- `computer_core.py`: the computer side in a real page (both browsers, `SVOLDER_HOST=fake`, no PipeWire):
  the host event, every action's route and read-back (a refusal in the same page), the side kept across a
  reload with no write, the bypass round trip (`setBypass`, `bypassOf`, `levelsOf`), a stale table (and a 409 for a point that moved), an unavailable computer
  (`SVOLDER_FAKE_HOST_EQ=off`), the question card's keys and buttons, and the same preset on both sides with
  its yes. Each theme's check also proves that Enter on an untouched cell writes nothing.
- `computer_ember.py`: Ember's computer side in a real page (both browsers, `SVOLDER_HOST=fake`, no PipeWire):
  the switch, the knob in %, the points' table, the curve under the mouse (tap, drag, wheel, right drag), a
  flip mid-drag, a guest, the side kept, the both-sides question, an amp page; a finger and a pinch in Chromium.
  It saves the computer side, desktop and phone, in Ember's folder of the web-shots pictures.
- `computer_signal.py`: Signal's computer side in a real page (both browsers, `SVOLDER_HOST=fake`, no PipeWire):
  the line's computer hop as the flip (and another hop flipping back, by tap and by keys with the focus on the
  chosen hop), the level in %, the points' table, the curve (a dot on it for each point; tap, drag, wheel), Save as,
  Rename and Delete of the presets, a flip mid-drag, a guest, the hop kept across a reload, the both-sides question,
  an amp page, a route that changes the sound; a finger and a pinch in Chromium. It saves the computer side
  (desktop, phone, the phone's table) in Signal's folder of the web-shots pictures.
- `computer_nightdrive.py`: Night drive's computer side in a real page (both browsers, `SVOLDER_HOST=fake`, no PipeWire):
  the two keys by the menu button, the volume figure and the dash's keys in %, the EQ line (OFF | EQ, no gains), the
  bubbles on the mountains (every dot on the curve, a drag, a tap on an empty spot, the wheel), the sheet's points
  table, a flip mid-drag, a guest, the side kept, the focus kept across a keyboard flip, the presets' actions, the
  both-sides question, an amp page, the fade; the phone's keys in the scene; the wheel and the arrows for the volume;
  `This computer` whole from 600 px up; a finger and a pinch in Chromium. It saves the
  computer side, desktop and phone, the sheet shut and open, in the web-shots folder `nightdrive`.
- `core_rows.py`: the core's rows Ember does not use, from a test page: a second device followed
  over one stream, a hop added, the menu's fade, a band moved by `core.eq.bands` from plain
  coordinates, light and dark following the system (Chromium), a canvas loop that pauses while the
  page is hidden, a theme unmounted and mounted again, the tab's title on Ember (connected,
  reconnecting, a device switch), and Ember mounted while the stream is lost.
- `node/paper-tokens.mjs`, `node/paper-words.mjs`, `node/paper-live.mjs`, `node/paper-ruler.mjs`, `node/paper-draw.mjs`, `node/paper-notes.mjs`: the theme Paper's tokens
  (AA on its sheet in light and dark paper, the two dark blocks alike, every frame colour token set,
  the planned values), its words (the title's line, the footer's date, only characters its fonts
  carry) and its sheet and what keeps it current (`draw/sheet.js`, `live.js`, `place.js`:
  which sections a change draws, a held section waiting for the newest model, the log of what the
  page asked, marks placed through the style object) and its volume's ruler (the steps' places,
  the labels at each 10 dB read off the device's table, the ticks, the zones, the drawn section) and
  its tone (the points at the bands' markers, the dashed Flat curve, the heading's words, the
  steppers, no `style`, no underlined word on a guest's sheet) and its controls and presets (every
  control written by its kind, the presets' save, rename and delete words only when editable, the
  device's slots, save and built-in settings) and its handwritten notes (which facts, their words,
  the one red note and its order, the last change and whose it is, the arrow drawn as a stroke).
- `paper.py`: the theme Paper in a browser: mounted from the address and the picker, its title,
  footer and menu button over the title's slot, Auto following the system, the menu's Auto, Light
  and Dark with the choice kept, dark paper painted dark from the first byte after a switch to Ember
  and back and a reload, contrast in both papers, no sideways scroll at 390 px, the volume (the needle's place, a drag,
  the written words, the keys, the cap, a reading from elsewhere during a drag) and the tone (a
  point dragged, the band table's steppers, the arrows on a point, the mode and preset lists, the
  loudness strength, a change from elsewhere during a point drag) and the controls (a choice, a
  switch, the balance scale), the presets with their forms and questions (a slot save posts
  `confirm: true` only after `save`), the device's built-in settings, a refused form, an open list
  kept through a change from elsewhere and a touch on the phone graph, Now playing's transport,
  an amp's power and needles, the device's spec table, the card for a lost device and the page's
  states in Paper's words, a second device one tap away, and no sideways scroll at 390 px with long
  names, titles and presets (a DAC, an amp, a Loki Max), and the notes (their words, the EQ's note
  and the track's time moving, `last change` and `from this browser`, each note in the band's third
  or after its section on a phone, one red note circled, ringed on a phone).
- `node/signal-line.mjs`: the theme Signal's line (`themes/signal/line.js`): the hops in order
  (player, computer, the shown device, the hops declared after it in file order), a hop after another
  DAC left off, a linked hop live only when listed, the line starting at the device with no host, an
  amp no hop links standing alone after a stand-in ring, the computer's stretch changing the sound
  from the path's flag, the AutoEq headphones ring and what replaces it, and a choice that falls
  back to the shown device (also the player that quits while chosen) and the shown hop's trouble,
  and a linked amp shown on its own page with the DAC its row names before it.
- `node/signal-draw.mjs`: what Signal draws of a DAC's first screen (`themes/signal/draw/`): the
  volume with its unit, the curve with a point on each band, the input chips and the mode row (the
  Loudness chip's round `−` and `+`; the mode under the curve on a phone, heading the shelf's middle
  column on a desktop, the input and presets first in a phone's shelf), the Add to the chain row of the
  menu, the Now playing bar with its progress, the shelf, a live amp's area (power, meters, Unlink), the player's and the computer's
  open areas (the computer read only), the card of a device in trouble and the broken line, what a guest's or a
  read-only viewer's controls owe (drawn inert), no `style` attribute and escaped words.
- `signal_theme.py`: the theme Signal in a browser: mounted from the address and the picker, the
  line's hops in order and evenly placed, its sizes (band 140 px, box 200 by 104, dots on y 60),
  rings for declared hops and dots for live ones, a click (even with the line drawn again between its
  press and its release) and the arrow, Home and End keys moving the choice and the box, the dots moving while the player plays and holding when it pauses, when
  the page is hidden and under reduced motion (the volume buttons in Ember's order, muted as `−∞ dB`, a held
  `−` or `+` repeating until it ends, a two-finger pinch on the curve setting a band's Q, the
  transport as svg icons, the page filling a window up to 1100 px and one centred block above it,
  the shelf in two columns, a held band or loudness stepper repeating across the shelf's redraws and
  stopping on a release, a blur or a hidden page, the mode's place at 1280 px and at 390 px, the
  tab's title), a hop that leaves and a device switch giving the
  choice back to the device, the line with no host and an amp standing alone, the menu's Palette row
  (four palettes by their own names, kept per browser, each recolouring the page while the hops keep their
  meaning and the dials their glow), contrast on the
  ground; the DAC's first screen and the Now playing bar; the shelf below it (the band table with
  typed and stepped values, presets saved, renamed and deleted in place, the switches, scales and
  loudness, a Loki Max's slots asked before they are written, the facts folded and the fold kept
  across a reload, Identify) and the slim bar that pins when the line scrolls away; a linked amp as a
  live dot with its power, switches and meters, its writes going to its own URL, the DAC's volume
  written under its name while the amp is chosen, the device list opened by the DAC's name, a chosen
  amp that leaves the fleet, Unlink asked once and the amp left standing alone, and a guest's taps; the
  menu's Add to the chain… row and the sheet that declares a hop (checked against `svolder hops list` on the same temp homes), a declared hop's
  Edit in place, Move earlier / later and Remove asked once, a hop removed elsewhere, and a guest's taps on them; the player
  chosen (its transport, the bottom bar hidden), the computer's read-only facts, a lost device's card with Try now and the
  seconds, a lost stream (`Reconnecting to svolder…`, faded, inert) and a refused write's toast; the
  phone (390 by 844, touch emulation): the strip with the chosen hop centred and boxed, a swipe or a tap
  choosing, the strip pinned when scrolled, 44 px targets, no sideways scroll with long hop names, the
  quiet lines not meeting, the menu button fixed against the window while the strip is pinned (and
  Paper and Ember placing it on the page, as before); a linked amp shown on its own page (the greyed way in, the DAC as a live
  hop, the thin stretch to the amp, an arrow onto the DAC boxing it in place with a Show button, a tap or Enter showing it); the middle layout (768 and 900 px:
  the curve and an amp's meters 300 px or wider, a point dragged; H6 whole from 1020 px).
  `contract.py` and `completeness.py` run Signal through its test driver (`themes/signal/driver.js`)
  like every listed theme.
- `signal_shots.py` (`make signal-shots`, Chromium): Signal's stills, a DAC at desktop width, with the
  linked amp chosen, with a declared hop chosen, with its link lost, the Add to the chain sheet open, the linked amp shown on its own page, a tablet (768 px), and a phone
  at rest and scrolled with the strip pinned, into `target/web-shots/signal`; no check, only pictures.
- `nightdrive_shots.py` (`make nightdrive-shots`, Chromium): Night drive's stills on the fake capture's
  music, a desktop at rest, with a bubble held, with the sheet open, with the device out of reach and with no
  capture, and a phone upright and sideways (each on the device's side and the computer's), into `target/web-shots/nightdrive`; no check, only pictures.
- `node/nightdrive-scene.mjs`: the theme Night drive's scene maths (`themes/nightdrive/`): the camera
  (the nearest row at the bottom, the far rows wider than F6's), the one frequency axis and the 24
  columns the 240 bands go to, the music's levels (the ridge's unit, a column's mean, the bass and the
  highs, the row rate) and the floor's 22 rows, newest at the horizon, still when it does not roll.
- `node/nightdrive-sky.mjs`: the theme Night drive's sky events and motion: the rays after a held
  bass, the shooting star on a transient and the shower on intense highs (each band's smoothed level read by its place in the
  last minute's range with a leaky count: kick- and hat-shaped music with dynamics fires, a flat drone and near silence do not), their limits (rays once a minute,
  a star in 5 s, a shower in 40 s) at 30, 60 and 144 steps a second, nothing when off, the log of 20, the
  levels readout of the driver, and the Full, Calm and Still levels with their pace.
- `node/nightdrive-hud.mjs`: the theme Night drive's HUD: the top line (device, input and rate, the
  path's verdict, the core title's trouble words in its place), the EQ line, the volume's words, the
  outrun volume's SVG (no `style`, prefixed ids, the minus and infinity as paths) and the fade on a
  fake clock (any input wakes it, trouble holds it off, a volume from elsewhere shows for 5 s), and the
  dash's markup (the keys in order, chips or one list chip, the transport as SVG, inert for a guest,
  the track's words and progress, the cover between the input and the title and none when the track has none),
  and the cursor's rest over the scene on a fake clock (2 s, any move, not over the dash, not while a button is held).
- `node/nightdrive-tune.mjs`: the theme Night drive's tuning on the mountains: the map between pixels and
  frequency and gain, the bubbles at each band's own gain, the held curve and label, the morph on a fake clock
  (the 1.5 s grace) and when a page may be tuned at all (a guest, writes off, no bands); and Q from a diamond
  (the half-gain points of a band's bell against the core's response, the Q step a diamond names, the diamonds on the curve, the resting pointer,
  a diamond drag landing on the step it names).
- `node/nightdrive-draw.mjs`: the theme Night drive's neon sheet as markup: its sections in order (Mode, Input, the
  band table, the presets beside it), the Loudness chip with its round `−` and `+` and the strength under its name,
  a field and two steppers for every band value, every key drawn inert for a guest and for a page whose writes are
  off, and no `style` attribute, inline script or unescaped name anywhere; and the thumbnail's sun stripes thick
  enough to show at the menu's size.
- `node/nightdrive-ridge.mjs`: the theme Night drive's ridge as heights: no sawtooth in silence, a notch a quarter
  octave and one a twelfth wide still there after smoothing, a ripple between bands gone, one bass band a
  rounded hill, the slow swell's drift (halved in Calm, frozen in Still) and the swell giving way to the music, and
  the ridge drawn as curves.
- `node/nightdrive-floor.mjs`: the theme Night drive's floor drawn as a heat grid on a recording canvas: a quiet row
  flat, a loud cell lifted and hotter than a quiet one, a near crossing lifted more than the same far one, the strips
  drawn far first (ground, heat, lines), the rows rolling toward the viewer and a floor that does not roll drawn the same
  twice, a lifted near edge with floor still under it to the bottom of the scene, the heat ramp leaning warm for bass and cool for highs, the wet road thinning the ground, the mood dimming the
  lines and the heat, and the wave of a moment running down the floor.
- `node/nightdrive-mood.mjs`: the theme Night drive's mood: the ear's level against the last minute's loudest, the
  medium and long views, quiet music at a low volume reaching long near 1, a drop after a breakdown firing one moment
  (and a steady song none after the first, at most one in 15 s), the balance and silence holding it, the embers
  of a peak, the same at 30, 60 and 144 steps a second and after a gap; the look made of it at each Motion level
  (mood, wet road, haze, heat lean, cruise speed, flare and wave); and the sparks (hot near cells, the cap by the
  canvas's area, births by the clock).
- `node/nightdrive-budget.mjs`: the theme Night drive's pixel budget on a fake clock and fake draw times: the density and
  2.5 MP caps, the steps down of 0.75 on a slow mean to one backing px per CSS px, the 30 a second cap after it, and
  the way back after 10 s comfortably under budget (a pause between draws restarts the clock), the step down on too few
  draws a second while every frame is asked for (and the way back shut for a minute after it), a phone starting at the cap,
  and the wet road: dropped first, before the scale and the rate, absent from a phone's start, back last and not for 3 minutes.
- `node/nightdrive-debug.mjs`: the theme Night drive's readout and bisect switches (`nomood`, `nowet`, `noembers` and `nohaze`
  among them): what an address asks for, the draws' rate and 95th percentile from the draws' own record, and the box's words
  (the ear's values in its sky line).
- `node/nightdrive-glow.mjs`: the theme Night drive's glow on a recording canvas: nothing in the scene is blurred (no
  `shadowBlur`), the sun's glow is a gradient round it, the ridge's edge and the bubbles glow with wider, fainter strokes
  under the line, and none of it is drawn when the glow is off; the wet road's slices sway as one wave (adjacent slices under 1 px apart at 1280 wide, all leaning the same way).
- `nightdrive_timing.py`: the theme Night drive's timing cases in a browser, which read the CPU or the wall clock and so run in the
  serial lane: no frame while the page is hidden, reduced motion starting on Calm with Full pickable, a paused player drawing at its
  level's own pace, a desktop starting with the wet road allowed by the budget, what a draw costs (printed; with every cell lit the median of five readings held to 4.5 ms in Chromium, the draws
  checked to have gone on between the readings), the floor's rows a second (loud, silent, mood off) and the budget's rate rule (a page
  that draws fast and seldom steps down, the same page left alone keeps its density and rate), a phone's start at the 30 a second cap
  and the readout's draws a second against the loop's own count.
- `nightdrive_theme.py`: the theme Night drive in a browser: mounted from the address and the picker,
  the scene's sampled pixels (the sky's top, the sun, a lit floor cell after 2 s, the ridge), the floor
  rolling with the host's synthetic capture, the analysis held only while the scene is on show (and
  let go on a hidden page and on a switch to another theme), the menu button in
  its 44 px slot and contrast on the scene, the Motion rows (kept per browser, Calm stopping the road,
  Still at about a frame a second) and the HUD: its words against the model, the fade after
  2 s and the wake on a move, a Tab, the volume shown for 5 s, trouble while faded, the menu button
  outside the fade, the two fonts and the words' places against the sun at 1280x800 and 1280x1600; and
the dash: the keys against the model, `+` `−` and MUTE, a held `+` (timed: the first repeat about 380 ms on,
then every 70 ms, ended by a release, a cancel, a blur, a hidden page and a lost stream), the cap held by
`[web] max_volume_db`, the input's chips and list, the transport, a guest's taps posting nothing, and
the dash outside the fade; and the tuning on the mountains: hovering turns them into the curve and
leaving lets it go, a bubble drag writes the steps nearest the drop (Flat becomes EQ), the curve stays 1.5 s
after the drag, a guest's tap says why, the value above a held bubble, a reading from elsewhere waiting for
the drag, a switch of device in the middle of a drag writing nothing to the new one, and a bubble grabbed
where it is drawn at a density of 2; the phone's top at 412x915 and 360x780 (no two of the device line, the dB, the EQ
readout and the menu button meet, the curve's top below the dB), the cursor hiding at rest and the cover as tall as the
track; and Q: a hovered bubble's pink stretch of the curve with its diamonds on it, the range of the map, a resting pointer letting the curve go, a diamond
dragged to a step's half point writing that step (mouse and touch), the wheel and the two-finger pinch on the
nearest band, a first touch only showing the curve, the curve staying 1.5 s after a notch or lifted fingers, and
a diamond grabbed where it is drawn at a density of 2; the neon sheet is `nightdrive_sheet.py`: opened by `DEVICE ▴` and by the menu's
Device… row with the focus inside, closed by the cross, Escape and a tap on the scene with the focus back, the scene
and the dash dimmed and out of reach while the scene goes on drawing, the band table stepped and typed (a held
stepper timed like the volume's and ended by a release, a cancel, a blur, a hidden page and a lost stream, let go at
10 s), the mode and the Loudness steppers, the presets saved, renamed and deleted in place, the sheet held while a
value is typed or a list is open, the phone's two layouts, a switch of device with the sheet open writing nothing,
and a guest's taps asked of the core and posting nothing, the device's slots (a save asks first), settings, controls (a toggle, a scale, the amp's power), facts (folded, the fold kept) and Identify, and an amp shown alone with its gauges. Both share `nightdrive_base.py` (the page, the request log, the bands reset); it is not run by itself.
- `nightdrive_phone.py`: the theme Night drive on a phone, in both browsers: held upright the scene is the top 52 % and the dash a
  panel under it (the volume large, `−` `MUTE` `+` in that order, the track, the transport and `DEVICE ▴`), held sideways the
  scene is the window with a slim dash, the sheet from the bottom or from the right and kept across a turn (the same control
  focused), every button and field 44 px (a 44 px box about the middle lands on it; the Loudness round keys by their hit area), the menu
  button fixed at 8 px, the mountains tuned from the scene's top half (touch in Chromium, the pointer in Firefox), a 40-character
  device name, a long track and a long preset with no sideways scroll at 390, 844 and 1100 px, and a tall window (1280x1600, 2560x1440) whole.
- `node/signal-controls.mjs`: the same on Signal's controls (`controls-wire.js`): one write in flight per scale of a device, a step after a device switch goes to the new device only.
- `node/signal-palette.mjs`: Signal's palettes (`palette.js`, the root tokens of `style.css`): Blueprint is the default (first in the list, no attribute, the bare root's tokens), a person who never chose gets it, and a kept choice of any palette, Warm amber included, stays.
- `node/nightdrive-controls.mjs`: the theme Night drive's controls on a stand-in page: a scale keeps one write in flight with the newest waiting, and a step asked after a device switch is written to the new device, never to the one shown before.
- `node/nightdrive-states.mjs` and `nightdrive_states.py`: the theme Night drive's states: the page's mood in each (`ok`,
  `unreachable`, `lost`, `wait`, `nocap`), the card of a device out of reach with Try now and the seconds, a fault that leaves the controls
  live, the quiet line's words; in a browser a dropped device (the road stops, the sun greys, the mountains flatten, the card, the dash out of
  reach, Try now posting `connect`), a lost stream, a page reading again and resting, a host with no capture (`SVOLDER_FAKE_CAPTURE=off`),
  a guest, a refused write and a card arriving while the top is faded.
- `voidgarden_theme.py`, `voidgarden_states.py`, `voidgarden_phone.py`, `voidgarden_timing.py` (shared helpers in `voidgarden_base.py`) and
  the layer checks `voidgarden_gpu.py`, `voidgarden_overlay.py`, `voidgarden_land.py`, `voidgarden_hero.py`, `voidgarden_fx.py`, `voidgarden_perf.py` (the post chain draws the scene once per frame; the `vg-debug` switches do what they say): the theme
  Void Garden in a browser. A cell is one engine at one tier: `none` (both 3D APIs off), `webgl2` (stock) and `webgpu` (Chromium: the four
  SwiftShader flags; Firefox: `dom.webgpu.enabled` in user.js); the tier checks run every cell, the page checks `none` and a few cases on
  `webgl2` with the Still motion level kept. `voidgarden_theme.py`: what mounts (layers in order, the slot last, the ground), the menu's rows and
  their kept choices (across a reload and with storage refused), `A`, the volume keys, the dash's keys and the driver, reduced motion on
  Calm, the fade by opacity only. `voidgarden_states.py`: the card, a lost stream, reading again and resting, no capture,
  a guest, a refused write, a card while faded. `voidgarden_phone.py`: at 360, 390, 768 and 844 wide, in each panel mode on each side, the
  page never scrolls sideways, the dash and its keys are inside the window and 24 px or more, the menu button is clear, the modules stand
  above the dash (a key past the window's edge passes inside a sideways scroller that brings it in); the phone layout of 0.40.0 on the `none` cell at 390, 412, 600, 844 sideways and 1280 (a compact see-through panel at the top, 44 px keys in one scrolling row, text over a bright scene at 4.5:1, one HUD module at a time, the style.css phone `@media` lines read as `hud.js` PHONE_QUERY, Tab to the Host module's last chip); Chromium's finger: the first tap on a faded dash only wakes, a tap opens the menu, a phone starts capped.
  `voidgarden_timing.py` (the serial lane): no draw while hidden, each level's pace as a ceiling, a held key's pace (the 15 s idle time is on a fake clock in `voidgarden_theme.py`).
  No case reads a colour, a place or a density of the scene (the look run moves those): they live in `contrast-baseline.json`,
  `phone-baseline.json` and the stills of `make voidgarden-shots` (`voidgarden_shots.py`). `contract.py`, `completeness.py` and `keys.py`
  run the theme. Every check that is not about the scene loads Void Garden with both 3D APIs off (Chromium: `--disable-3d-apis`, Firefox: the
  `webgl.disabled` pref; `modal.browser_for`, the sweeps' `browser_of`, contract, completeness, keys, frame_button): on software rendering a live
  scene blocks the page's main thread for seconds at every draw, even in Still (a driver call took 3.3 s), and the still stands for the scene.
  The live scene is `voidgarden_theme.py`'s webgl2 cell, the tier checks' and the stills'. Contract and `modal.HOOK` also keep the Still level for
  Firefox's live scene.
- `paper_shots.py` (`make paper-shots`, Chromium): Paper's stills, a DAC and an amp at desktop and
  phone size in light and dark paper, into `target/web-shots/paper`; no check, only pictures.
- `arrange.py`: Ember's Arrange mode in one browser (a fake DAC with a player, a DAC and an amp
  in one daemon, a DAC that goes away): the sections moved by the arrow keys and by a drag of the
  handle, Done, Reset and Escape, the order per layout kept across a reload and a screen swap, the
  page's own drags inert meanwhile, a swapped part landing in its moved section, `T`, a theme that
  unmounts mid-arrange, and a phone's order. `ARRANGE_SHOTS=<dir>` keeps four pictures.
- `tone_note.py`: the Tone pane's Flat and Loudness notes, and that no mode moves the layout.
- `knob_click.py`: a click on the knob's ring jumps there; a drag, the centre and the gaps do not; holding the `+` or `-` button repeats the step and a release ends it.
- `device_presets.py`: a Loki Max's presets part (`Daemon(profile='lokimax')`): three slots and five
  built-in settings, a built-in's click and its lamp (also lit by another client through the
  stream), a slot's recall, the save question opening in place, Keep closing it with no request and
  Save posting `confirm: true`, and what was saved coming back on a recall.
- `playing.py`: the Now playing card on the fake host (`Daemon(host='fake')`): the desktop band and
  the phone's line that a tap opens, the track, cover, path and counting time, the transport posting
  `api/v1/host/press`, a guest and a read-only page without buttons, the card's box unchanged when
  the cover goes (`SVOLDER_FAKE_COVER_GONE_FILE`, written by the check once the card shows the cover) and when the player goes
  (`SVOLDER_FAKE_PLAYER_GONE_FILE`), a cover address the daemon no longer holds (a real 404: the
  picture is hidden and its box stays), an open phone card whose player quits (it folds to the quiet
  line), the folded line at 341, 426 and 784 px (a round play, a bare next, the arrow in the corner,
  a cut title looping slowly, its copy a gap behind it), a changed path on the fake changed route
  (`SVOLDER_FAKE_ROUTE=changed`: every chip sits inside the band's fixed box at 820, 1100 and 1280
  px, the label first), a sweep from 240 to 420 px, one pixel apart, that no wrapped line of a chip
  starts with a dot or an arrow and no value is split (also a no-hop row with a long sink name), an
  amp page, a daemon with no host.
- `reduced_motion.py`: with reduced motion emulated, the small motions all run (the glow turns and
  follows the music, the dot pulses, the knob eases, a held peak falls by steps, the tone curve
  eases and a changed value flashes) and only the first default level (Glow) follows the setting;
  Off stills the page.
- `visualizer.py`: the visualizer card on the fake host: the canvas draws the synthetic music (a
  fresh waterfall starts as silence), a tap and `v` cycle the three looks with a chip, the graph
  toggle, the choice kept across a reload, a hidden page and a card out of view letting their frames
  stream go after 2 s, and a hidden page its device stream too, with no sign of a fault and a change
  made meanwhile on screen within 2 s of its being shown (the page's visibility is a switch the
  check holds, every EventSource noted), no capture (`SVOLDER_FAKE_CAPTURE=off`) keeping the box,
  the placement at 820, 1280 and 390 px with no box moving when it starts, the Device fold-out's
  delay row, an amp page's sink label, a guest and reduced motion, and the effects hearing the
  frames (the glow over the volume's level, the smoke's beat puffs). `VIZ_SHOTS=<dir>` keeps stills
  for looking.
- `tone_drag.py`: the Response graph under the mouse: a left drag, a drag straight across, a right
  drag for Q, the wheel (a burst that keeps its band as the pointer drifts), a guest; in Chromium
  also the swap hold, a device that goes away, and a drag let go of with its writes held (a new
  press, the wheel, a burst's base).
- `tone_touch.py`: the Response graph under a finger and a pinch, in Chromium under touch emulation
  (headless Firefox does not pan on touch): a drag from a point, a scroll away from every point, a
  touch between two points, a pinch for Q with a band picked and without, a second finger on a held
  point, a touchcancel mid-drag, a third finger in a pinch.
- `touch.py`: a finger stirs the smoke under touch emulation, and the page still scrolls.
- `firefox_preload.py`: Firefox's error collector sees an error thrown by a page's first script.
- `gate_refusal.sh`: the quick gate's refusal, on a throwaway repository.
- `fakeclock.py`: the controllable page clock (see "The fake clock").
- `fingerprint.py` and `gate_cache.sh`: the result cache's fingerprint and its record, and the proof of both on a throwaway tree.
- `parts.py`: the `--part logic|timing|all` selection of the seven split checks (see "The serial lane's timing parts").
- `common.py`, `smoke_probe.py`: the shared driver (fake daemon, Chromium over CDP, Firefox over
  geckodriver) and the in-page probe; not checks. A `Daemon` takes `fleet=[(profile, name, udid)]`
  (more fake devices, each with its `[[device]]` block), reads its model (`model(name)`), writes
  through the web API (`post`) and can take its first device off the air on the check's own cue
  (`drop()`, through `SVOLDER_FAKE_GONE_FILE`: no clock, so a loaded machine cannot reorder it).

## The approved look

The smoke's numbers live in `TUNING` (`bins/svolderd/web/themes/ember/smoke-model.js`). The numbers
the node scenarios are written for are copied once, in `node/look-values.mjs`. To retune on purpose, change
a number in `TUNING` and in that file together. While they differ, `node/model.mjs` fails its test
"the approved look" and names the numbers, and no other node check fails for it: `run-all.sh`
makes the tests written for the old numbers skip themselves (`LOOK_RETUNED_SKIPS=1`). Once the two
agree, those tests run against the new numbers and say which rule the new look breaks. The browser
checks that depend on a look number (`notes.py`: the note cooldown and the rolls around each odds;
`smoke_quick.py`, `effects_cycle.py` and `haze.py`: the calm rate) read it from the page's `TUNING`
and derive their brackets from it, so a retune never fails them for the number alone.

## Mutation tests

A check that cannot fail proves nothing. The mutation runners break the code in a scratch copy and
the checks must fail. Each runner holds a list of small edits (a guard removed, a number changed, a
sign flipped); for each it copies the page's files, applies the edit to the copy, points the check
at the copy and expects a failure. A mutant that passes is reported as `SURVIVED`: the check does
not cover that edit. An edit whose anchor text no longer exists in the source stops the runner with
`ANCHOR MISSING`: the list needs updating to the code.

They are slow, so they run by hand only and are never part of a gate. `make tests-check` runs
the node runners (a few at a time, `MUTANTS_JOBS`, default 4) and `make tests-check FULL=1` every browser
runner too, one runner after the other; it prints each runner's kills of its mutants and its wall time,
then a summary, and fails when a mutant survives. The browser runners share `mutpool.py`: the pool
(`MUT_WORKERS`, default from the cores and the CPU pressure; each mutant has its own scratch web copy,
daemon and browser), the mutants whose named check measures timing alone after the pool, every mutant the
pool did not kill run again alone (only the alone result counts; a pool kill stands), a baseline that
failed run again and not remembered, a run whose daemon lost the race for its port started again, and the
web gate lock (`/tmp/svolder/web-gate.lock`) held shared around a run and exclusive around a timing one.
`mutate_haze.py` and `mutate_scroll.py` run every mutant alone, and `mutate_smoke.py`, `mutate_notes.py`,
`mutate_sources.py` and `mutate_effects.py` one at a time: their timing bounds flake side by side.

- node: `bash scripts/web-checks/node/mutate_director.sh`, `mutate_model.sh`, `mutate_notes.sh`,
  `mutate_sources.sh`, `mutate_paper.sh` (27 mutants of Paper's ruler, notes, changes,
  log and words, run against a copy of the web dir by `node/paper-*.mjs`) and `mutate_signal.sh`
  (26 mutants of Signal's line, fold, computer hop, strip, linked amp and faults, volume and transport icons, the mode's place, the Loudness chip and the menu row, run by `node/signal-*.mjs`)
  `mutate_computer.sh` (140 mutants of the core's point writes and helpers: the slot
  dropped from a set or a remove, sent as the place, a 409 that does not end the drag or says the
  daemon's words, the cell's equal-value guards, the step's direction, the refusal, the question's
  return (also after the caller's work), the cell's rounding, the points' most, the words and the
  hand ranges, the table, the hand on the curve, run by `node/core-computer.mjs`, `node/core-words.mjs`,
  `node/core-table.mjs` and `node/core-point-drag.mjs`),
  and `mutate_nightdrive.sh` (62 mutants (a minute's fade and a ridge smoothed over 0.6 of a band among them, the wet road's slices swaying each by their own phase among them) of Night drive's camera, axis, levels, tempo, sky, ear, look, budget, floor drawing, tuning, words and dash, run by `node/nightdrive-*.mjs`);
- browser: `python3 scripts/web-checks/mutate_smoke.py [name ...]`, and the same for
  `mutate_notes.py`, `mutate_haze.py`, `mutate_scroll.py`, `mutate_sources.py`,
  `mutate_effects.py`, `mutate_webui.py`, `mutate_registry.py`, `mutate_ember.py` (29 mutants of Ember's computer
  side: the drawing through `node/ember-parts.mjs`, the wiring through `computer_ember.py`, each
  killed only by a case that names it), `mutate_signal.py` (26 mutants of Signal's page:
  the amp's write without the amp's url, the dots ignoring reduced motion, the slim bar never
  pinning, a held plus that never repeats, a blur that does not stop a hold, a pinch the wrong way, a tall window that stretches, a shelf of three columns, the Add to the chain row opening the sheet for a guest (the core refuses a guest's post, so the sheet
  is what can be mutated), the strip not centring, each killed only by the check it names; it drives `signal_theme.py` and `computer_signal.py`, the pool runs the mutants,
  then again alone each one it did not kill and the five whose check measures repeat timing)
  `mutate_nightdrive.py` (Night drive's page, Chromium, 55 mutants: a sky drawn twelve times in every draw, a loop each mount starts and nothing ends, a long press that presses on its release, a first tap on a faded dash let through, a wet-road copy made a tenth of a second apart, an amp's meters heard as a part, the fade not woken by focus, trouble fading, the analysis kept while hidden, reduced motion starting on Full, Still at the screen's rate, a dash hold that never repeats, a diamond drag that writes nothing, a pinch on band 1, hit tests in canvas pixels, a switch that lets a drag write, a guest's morph, a first touch that grabs, a question that sends early or on Keep, a band field or a list chip that writes when it should not, the core letting a guest's call go out, the sheet's focus not given back, its columns swapped, the menu button following the page on a phone, Space playing under the menu, a 30 px picker close key; each run names the cases of `nightdrive_theme.py`, `nightdrive_sheet.py`, `nightdrive_phone.py`, `nightdrive_states.py`, `space_key.py` or `effects_menu.py` it needs, runs them unmutated first and counts a kill only when a check the mutant names fails)
  and `mutate_paper.py` (15 mutants of Paper's page, each running only the cases of `paper.py` or `computer_paper.py` it names, after the same cases passed unmutated:
  `place` setting nothing, Auto ignoring the system, a held section drawn anyway, the log empty, a
  meter's needle at its class-A share, which drive `paper.py`, and ten of the computer's side, which drive `computer_paper.py`) (with no name they run every
  mutant; each file's header says which check it drives).

A worked example from `node/mutate_model.sh`: the line `Math.exp((-4 * dt) / secs)|Math.exp((-2 *
dt) / secs)` halves the speed of the easing in the copy; `model.mjs` must fail its test "ease:
converges, never overshoots, monotone". A line that survives means a rule without a test.

## Tuning aids

Not checks; run by hand when retuning the look.

- `tune.py key=val ...` serves a copy of the page with `TUNING` values changed and prints the
  resting cloud's numbers at several levels.
- `notes_tune.py [chromium|firefox]` prints the stamp's look in the canvas's own pixels over
  several volume steps.
- `glow_level.py` prints the Glow level's densest pixel as an equivalent alpha, to tune the smoke's
  densest cloud against.
- `soak.py chromium [minutes] [out.json]` runs the smoke untouched for 30 minutes by default and
  compares each five-minute sample with the same minute of `soak-baseline.json`: the frame rate
  within 1 frame a second and the mean alpha within 15 % must hold (exit 1 on a miss or a page
  error); the largest velocity within 25 % is only reported, as a noisy maximum. It is in neither
  gate; run it before a merge that touches a smoke file. It holds the long-run behaviour (haze
  density, frame rate, fluid size) that the gate's 20 s and 40 s reads do not.
- `smoke-baseline.json` is the recorded pace of a stroke that `haze.py` compares against: a stroke
  must last longer and travel further than in that recording. It holds one entry per browser.
  `SMOKE_WEB=<copy of the web dir whose pace is the reference> python3 haze.py chromium --baseline`
  (and `firefox`) records it again; do that only to move the reference on purpose.
  `soak-baseline.json` is the output file of a `soak.py` run (its fourth argument) kept as the
  reference.
- `computer_paper.py`: the theme Paper's computer side in a real page (both browsers, `SVOLDER_HOST=fake`, no
  PipeWire): the switch by the title and the ruler and tone it flips, one percent a tap, a point dragged,
  tapped in and removed, sixteen points in the same height, a flip mid drag (Chromium), a guest, the side kept
  across a reload, the same preset on both sides asked and answered, an amp's page, a phone. It writes the
  stills `target/web-shots/paper/computer-desktop.png` and `computer-phone.png`.
