<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Testing

svolder is mostly testable without a radio, a broker or a sound server, and the few things that are
not have a gate of their own. This page is what each gate proves and when to run which, how the
fake device and the simulator stand in for a real one, how the end-to-end run uses two Bluetooth
controllers, what the safety list guards, what the documentation check enforces and how the web
page is checked. It sits one level below [the map](../architecture.md), whose "Where the tests
live" lists where each module's tests are.

## The gates

- **`make check`**: the whole ordinary gate: build tools present, format, clippy with warnings
  denied, the safety list, the documentation check, dependency licences, source headers, the
  network boundary, the build-deps self-test and every test. Run it before every commit meant to
  stay and before a merge.
- **`make test`**: the workspace's tests alone, in a scratch environment; run it while iterating.
- **`make safety-tests`**: every test on the safety list still exists; run it after touching one.
- **`make gnome-extension-check`**: the indicator's [gjs tests, zip and headless load](gnome-indicator.md).
- **`make docs-check`**: links, anchors, process tags, code spans that name repo paths and page
  sizes (below). Run it after editing any Markdown; it is part of `make check`.
- **`make tui-harness`**: the real `svolder tui --demo` in a pseudo-terminal, read through a screen
  parser: start-up bytes, the Web UI box, the mouse, a resize, quit. About 5 s, opt-in; run it
  after a change to the TUI's terminal handling or loop. See [the harness](tui-pty-harness.md).
- **`make web-checks-task`**: the quick gate with the logic-only checks in Chromium only; the
  rung for a task or a lane end (the ladder is in the web checks' README).
- **`make web-checks-quick`**: the node harnesses, the browser checks that leave the smoke alone
  and one short smoke check per browser. Run it after a change to the page, its styles or its
  templates that does not touch the smoke; it takes about 38 minutes with the default number of
  checks at a time, about six on 24 cores, and much longer with `WEB_CHECKS_JOBS=1`, one by one.
- **`make web-checks`**: everything the quick gate runs plus every smoke check, in headless
  Chromium and Firefox. Run it before merging any change to the web page and whenever a smoke file
  changed; it takes over an hour at about six checks at a time (24 cores).
- **`make e2e`**: the daemon, the CLI and a web write against a simulator over real Bluetooth. Run
  it after a change to the transport, the session, the scanner, the daemon's reconnect or the
  simulator; it is not part of `make check`.
- **`make host-e2e`**: the computer EQ end to end, through the real daemon, on a private PipeWire
  with a null sink, never the session; the whole run is on [its page](host-e2e.md). Run it after a
  change to the computer EQ, its chain or the daemon's computer side, alone: no other `make test`
  may run beside it. Not part of `make check`.
- **`make preamp-lab`**: the computer EQ's preamp lab, the long run (`SEED` is the first seed and
  `RUNS` how many of each profile; 5000 by default). The controller is driven through
  seeded random sequences of hand edits, preset loads, command-line applies, node restarts, rate
  changes, `pw-cli` deaths and the rest, against a model of what PipeWire plays, and after every
  event and tick the curve played times the preamp played must not pass unity. A failing seed
  prints the shrunk sequence. Run it after any change to the controller, the session or the chain's
  config; a short run of 300 seeds of each profile is part of `make check`. How the model works is
  in [host-control-preamp.md](host-control-preamp.md#the-preamp-lab).
- **`make preamp-lab-outcomes OUT=<file>`**: every seed's outcome in the lab (each profile, both models
  of ids; `SEED`, `RUNS` (400 by default) and `THREADS`), one line each. A change that must not
  change what the lab finds (a faster evaluator, say) runs it before and after, and the two files
  are the same. `TRACE=1` adds each run's trace (every `set-param` the controller sends): the
  stricter check for a change to the controller.
- **The soak**: a long run of the smoke's frame rate, haze density and fluid size. Run it before a
  merge that touches a smoke file; it is part of neither web gate.

`make check` does not run the web checks, `make e2e`, `make host-e2e`, the long preamp lab or the
soak: they need browsers, two controllers, a PipeWire install or an hour, and a machine without
them must still be able to run the ordinary gate. The quick web gate refuses (exit 1, naming the
file) when a smoke file or one of the smoke's checks changed since `main`, so it cannot stand in
for the full one. `make web-touched` names the checks a diff touches, `make web-checks-affected`
also those that name what it changes (a lane's CODE DONE); a tag needs `make web-checks-cold`. See
[scripts/web-checks/README.md](../../scripts/web-checks/README.md).

### What `make test` does

`make test` runs `cargo test --workspace --all-features` once, so every binary is built by the same
invocation and a test that needs a sibling binary runs the one on disk (`SVOLDER_TEST_PREBUILT=1`;
`T='-p <crate>'`, another set, leaves it unset). It links with lld (`ld.lld` or `ld.lld-NN` on the
PATH; `NO_LLD=1` turns it off; never in release builds or CI) through `COMPILER_PATH`, which is in
cargo's fingerprint: every dev-profile `make` target sets it and they agree; a hand `cargo` run
without it rebuilds ring, psm and their dependents once. `NEXTEST=1` runs cargo-nextest (166-272 s,
`cargo test` 196 s): opt-in; its concurrent processes fail real-time tests on a loaded machine.
It runs in a scratch environment and fails naming anything a test leaves in its TMPDIR. `make check`
runs with no BlueZ: only a hand run takes the BlueZ branch of `backend.rs`:179, `device_cli.rs`:507.

The in-process MQTT broker (`rumqttd`, about 175 crates svolder-mqtt would otherwise build) is an
optional dependency behind the `broker-tests` feature of `svolder-mqtt`, `svolderd` and `svolder`.
Four groups of tests need it and run only with `--features broker-tests`: svolder-mqtt's `client`
tests, svolderd's `mqtt` integration tests, two unit tests in svolderd's `mqtt_bridge/tests.rs` and
the one `daemon status` test in svolder's `backend.rs`. `make test`, `make check` and both CI jobs
pass `--all-features`, so they run them. A plain `cargo test` builds without the broker and passes
without running them, so it can pass with a broken broker test: finish with `make test`.

### What `make audit` does

`make audit` runs `cargo audit --deny warnings` against `Cargo.lock`, allowing the two known
unmaintained warnings by id (paste, RUSTSEC-2024-0436; ttf-parser, RUSTSEC-2026-0192) and failing
on any other advisory or warning. It needs the network and `cargo-audit`, writes the advisory
database to `~/.cargo/advisory-db`, and is not part of `make check`.

### The minimum-compiler job

`Cargo.toml` promises `rust-version = "1.93"`, and the local `make check` runs on it. CI's `test`
job in `check.yml` runs `make check` on Rust 1.100.0, the compiler the release builds use. Its
`minimum` job runs `make test` on 1.93.1: it proves the workspace builds and its tests pass on the
minimum compiler, with every feature on. It runs no lints: clippy's lints move with the compiler.

## The fake device and the transport

`svolder-fake`'s `FakeDevice` (`crates/svolder-fake/src/device.rs`) answers real protocol frames in
memory: it decodes a command with `svolder-proto` and returns the notifications a real device
would send, for any built-in profile. It is synchronous and holds no I/O. `Signal`
(`crates/svolder-fake/src/signal.rs`) plays a music-like level on the fake's meters, a function of
the clock and a seed, so a front end that draws to the music has something to draw.

A profile's actions are the fake's too. With the Loki Max profile it keeps three slots in memory: a
save copies the bands into slot `p` and answers nothing, a recall sets the bands from slot `p` (a
slot never saved gives every band its 0 dB step, which is Flat) and reports every band in one
notification, changed or not, as a unit that pushes its bands after a recall does. A band written
on `6.<band>` or `6.<band>.3` sets one band, and the reading is reported on the band's own id, or
with the report form switched to short on its two-id form, so the alias read is tested end to end.
A silent recall reports nothing, so the session's refresh after a recall is tested.

`FakeTransport` (`crates/svolder-ble/src/fake.rs`) wraps a `FakeDevice` as a `Transport`. With
`SVOLDER_TRANSPORT=fake` (or `--transport fake`) a front end or the daemon talks to it in-process
with no radio. The fake's behaviour is set by environment variables the tests and demos use:
`SVOLDER_FAKE_PROFILE`, `SVOLDER_FAKE_UNIT`, `SVOLDER_FAKE_FIRMWARE`, `SVOLDER_FAKE_PASSPHRASE`,
`SVOLDER_FAKE_FAULTS`, `SVOLDER_FAKE_SIGNAL`, `SVOLDER_FAKE_LOKI_REPORTS` (`full` or `short`, the id
a Loki Max band is reported on), `SVOLDER_FAKE_SILENT_RECALL` (`1`, a recall reports nothing),
`SVOLDER_FAKE_QUIET_SAME` (`1`, a Command equal to the held value reports nothing, on every
control), and three that play hard cases, `SVOLDER_FAKE_SEVER_AFTER_MS` (cut the first link, to
watch a reconnect), `SVOLDER_FAKE_GONE_AFTER_MS` (take the device off the air) and
`SVOLDER_FAKE_GONE_FILE` (the same, the moment a file appears at that path, for a check that must
drop it on its own cue). `FakeTransport` also counts connects and releases and can advertise
differently, go quiet or push readings, for tests of the scanner and the session.
`docs/usage/environment.md` lists the variables.

`make demo-safe` and `svolder tui --demo` run a fake device behind a throwaway daemon in a directory
of their own, touching nothing of the user's; the daemon's host is the fake one
(`SVOLDER_HOST=fake`), so the demo's web page shows a Now playing card, and the TUI's own host is
off. `--host` (which `make demo` adds, with `--web`) leaves the host on instead: the user's real
volume, player and computer EQ, which no test ever runs with.
`bins/svolder/tests/demo.rs` proves every way the TUI can end leaves neither daemon nor directory.

## The simulator

`svolder-sim` (`bins/svolder-sim`) serves the fake device as a real BLE peripheral: it advertises
the service and manufacturer data and answers writes over GATT, so the CLI and the daemon can be
driven over a radio with no hardware. Its flags choose the device (`--profile`, `--unit`,
`--firmware`, `--passphrase`, and `--advertised-passphrase`, which is opt-in and changes only the
code the device advertises, not the one it checks), seed faults (`--faults`), play a signal
(`--signal`, for a profile whose meters have a scale such as `fake-amp`), report a Loki Max's bands
on the short id form (`--loki-reports short`) or not at all after a recall (`--silent-recall`),
report a change only (`--quiet-same`), start with USB, S/PDIF lock and a sample rate of its own and
change them from stdin (`--usb`, `--spdif-locked`, `--sample-rate`; `usb 1`, `spdif 0`, `rate
44100`), print every decoded frame (`--trace`), lay the advertisement out as an older device does
(`--layout legacy`) and answer pairing (`--accept-pairing`, which must never run on a host that also
has a Bluetooth central of its own on the same adapters). It needs an adapter (`--adapter`).

The device reports the tone-control mode on a change only: a Command equal to the mode it holds is
stored and answered with nothing, found by the profile's tone-control role so every parametric-EQ
profile has it. The official app answers every report of the mode with a write of it, so a device
that answered each write would have it write forever. `--quiet-same` (or
`SVOLDER_FAKE_QUIET_SAME=1` on the fake) extends the silence to every control, which is how a unit
that reports changes only is played; firmware asks, identify, refresh, actions and the passphrase
are never silenced. A session answers a write of the value a device holds, left without an echo
past the set timeout, with that value instead of an error.

## End to end over two controllers

`make e2e` (`scripts/e2e.sh`) starts `svolder-sim` on one controller and drives the CLI and then
`svolderd` against it from another over BlueZ. It needs two controllers that support extended
advertising: it picks them (two of the same USB model if the host has a pair, else USB dongles
before any other), or `SVOLDER_E2E_ADV_ADAPTER` and `SVOLDER_E2E_SCAN_ADAPTER` name them. It never
needs root. By default it builds `svolder` and `svolderd` with cargo and drives those. With
`SVOLDER_E2E_BIN_DIR` set to a directory holding both it drives those prebuilt binaries instead,
which is how the arm64 release builds run under QEMU on an x86_64 host:
`SVOLDER_E2E_BIN_DIR=target/dist/aarch64 make e2e`. QEMU's default CPU accepts instructions a
Raspberry Pi 3 or 4 lacks; `QEMU_CPU=cortex-a72` in the environment runs the arm64 binaries as a
Pi 4's core. A missing or non-executable binary fails at once, before any controller is touched.
The simulator and the ignored `bluez_e2e` tests stay native builds.
Its sections run in order, and each says what it proves:

- scan, inspect, the adapter is recorded, set a volume: the direct BlueZ path;
- the ignored live transport tests of `svolder-ble` (`bluez_e2e`), one at a time because each drives
  the same two controllers;
- `capture`: a scrubbed file with its four sections, then the same with `--keep-ids`; these run
  with a runtime directory and a `systemctl` of their own so they never see the host's daemon;
- the link ends when the peripheral goes away; a scan with it gone and back;
- a connect, a scan and a second connect, with the simulator's log showing it advertised again
  after the first link closed. The default layout reconnects without the restart, so this run
  proves nothing new for it; `SVOLDER_E2E_LEGACY=1` repeats the step for `--layout legacy`, the
  layout a phone scans, and that run is the proof for the case a phone needs;
- the daemon starts, connects, answers `status`, sets a volume, and takes a volume write through
  the web page's own route;
- `capture` over the running daemon: it is stopped, svolder starts it again, and it reconnects;
  the script stops the new one over its socket;
- `watch`, a reload on SIGHUP, a clean stop;
- the signal: a `--signal` simulator's readings reach `watch`;
- a Loki Max simulator that reports a band on its two-id form: the alias read, a built-in setting,
  a save and a recall (one frame on `7.2.2`), then a simulator that stays silent after a recall:
  the daemon's reading catches up and the simulator sees one refresh.

A passing run prints `e2e: PASS`; a failing one prints the simulator's and the daemon's logs and
keeps its directory. The host needs a Bluetooth stack that does not answer the simulator's services
with pairing attempts of its own: [the Bluetooth guide](../usage/bluetooth.md#make-e2e) lists the
setup and the hints the script prints. This is the only place the BlueZ transport's wiring is
exercised, which is why a change there is not finished until it has run.

## The safety list

`scripts/safety-tests.list` has one `path|function` line per test that guards the volume rule
(the three-quarters glide, a climb's cancellation, the cap), the refusal of the firmware-update
functions at every layer, or the host capture's command line. `make safety-tests` fails if a listed
file no longer defines the function, so none of these tests can be removed or renamed without
someone noticing. To rename one, change its line in the same commit; to remove one, say why in the
commit message. The list is a tripwire for deletion; running the tests is `make test`'s work. The
rule itself is in [the map](../architecture.md#where-the-volume-safety-rule-lives-and-why).

## The documentation check

`scripts/docs-check.py` (`make docs-check`, part of `make check`) keeps the docs in a shape a reader
can use:

- every relative Markdown link in the README and the Markdown under `docs/`, `profiles/` and
  `scripts/` reaches an existing file, and every `#anchor` an existing heading;
- no process tag, and no "bit-perfect" or "bit-exact", in shipped words (see the script);
- a code span that looks like a repo path, in a shipped Markdown file or a `//` comment of the Rust
  sources, names a tracked file or folder; the `PATH_ALLOW` list in `scripts/docs-check.py` takes a
  span that is not a repo path, each with its reason;
- `docs/architecture.md` stays within 300 lines and each `docs/design/*.md` within 400, so a page
  that outgrows its budget is split into two rather than squeezed;
- with `--merge` (`make docs-check-merge`, run before a merge to main), every entry of `docs/` is a
  shipped doc.

Some tests read the documentation too, so it follows the build: the TUI's mockups in
`docs/usage/tui.md` are compared with what the renderer draws
(`bins/svolder/src/tui/test_support.rs`, `screens/menu/tests.rs`, `screens/settings/tests/look.rs`),
and `docs/usage/environment.md`'s table is compared with `svolder help env --markdown`
(`bins/svolder/tests/env_vars_doc.rs`); `make docs-env` rewrites that table from the built binary.
`crates/svolder-ipc/src/config/tests/tui.rs` reads the template blocks of `docs/usage/config.md`
twice: the configuration template and that page carry the same intro lines.

## The web page's checks

The Rust side of the page is tested like any module (`bins/svolderd/src/web/*/tests.rs`, against a
fake daemon). The script, the styles and the smoke need a real browser and are checked from
`scripts/web-checks/`:

- **Node harnesses** (`node/*.mjs`) prove the smoke's rules with a fake clock: the rhythm, the haze,
  the palette, the scroll floor, the notes' odds, gap and cap and placement, the registry. The tone
  curve's maths is held to a fixture of the core's own numbers, and the curve's gestures
  (`tone-drag.js`) and the swap hold (`keep.js`) are models with no page in them, so node proves
  grab reach, the axis hold, the wheel's steps, the order of the writes and the hold's rules too.
  They run in seconds and are the first place a rule change is proved.
- **Browser checks** (`*.py`, headless Chromium and Firefox against throwaway fake daemons, never
  the installed binaries and never the default ports) keep what needs a GPU or a real page: finite
  and bounded fields, the calm frame rate, the haze's density on screen, legibility against Glow,
  touch panning, the wheel, resize, screen swaps, a theme that fails to load, the CSP and the
  fallbacks. What a browser alone does to the tone curve's points is checked with real pointer and
  touch events: a drag, a right drag, the wheel, a pinch, the page still scrolling away from a
  point, the swap that waits for a drag (`tone_drag.py` in both browsers, `tone_touch.py` in
  Chromium, where touch pans). They read the smoke with `readPixels` and float readbacks, no
  picture. - **The theme contract** holds every theme the page lists to one set of checks:
  `node/contract.mjs` for the lists of what a theme must reach and show, `contract.py` for a short
  pass per theme in each browser (volume, mute, a band, a preset, a device lost and back, a refused
  write, resting and waking, the menu button and `T`, `?theme=`, a guest), `completeness.py` for
  every control and fact on every kind of device in the fake fleet, and `core_rows.py` for what the
  core holds for themes to come. They go through each theme's test driver and judge what the fake
  daemon received and what the theme showed, never a pixel. [Web themes](web-themes.md) says why.
  The core's stack of modals and its router of the history are held by `node/core-modal.mjs` and
  `node/core-router.mjs` on a jsdom page (the apt package `node-jsdom`) and by `modal.py` in each
  browser, in every theme: no key acts behind a modal, and Back steps a popup without changing the
  shown device. A harness that needs jsdom skips loudly where it is missing (`SKIPPED`, exit 77, a
  failure for the gate) and passes only under `WEB_CHECK_ALLOW_NO_JSDOM=1`, under which the whole
  node list passes on a machine without jsdom; `node/dom.mjs` says when a harness uses a jsdom page.
  Paper adds six node harnesses (`paper-*.mjs`: tokens and contrast, words, the live sheet, the
  ruler, every section from every fixture, the notes), `paper.py` in both browsers (dark from the
  first byte, the needle's value against its place, drags, the cap, a held section, the questions,
  no sideways scroll at 390 px, the notes), and two mutation runners;
  [web-paper.md](web-paper.md#the-checks-and-the-mutants) lists them. `WEB_CHECK_THEMES` limits the
  contract passes to the themes it names and fails on a name the page does not list. - **The
  sweeps** hold what the other checks leave to the eye. `contrast.py` computes WCAG 2.2 contrast
  from the colours the browser draws for the page, the menu and every listed popup, in every theme
  and scheme (a picture ground is sampled); `phone_sweep.py` loads the same states at 360, 390 and
  768 px and reads sideways scroll, cut elements and small controls. Each keeps a baseline of
  today's failures (a new or worse one fails) and fails on a state it did not reach or a planted
  fault it misses. Full gate only; blind to canvas text, hover, animation, forced colours. `make
  routes-doc-check` ties the daemon's routes, the fields of the devices, series, walk and AutoEq
  routes and their writer gate to the table in [web-ui.md](../usage/web-ui.md). - **Arrange mode**
  has `node/ember-order.mjs` (the order's rules) and `arrange.py` (every section into every zone,
  keys, touch, Reset, Escape, the order across a reload). - **Mutation runners** (`mutate_*.py`,
  `node/mutate_*.sh`) edit a copy of the page's files and must make a check fail, so a check that
  cannot fail is found. They are slow and run by hand, one worker. `snippets.py` (`make
  web-snippets`, part of `make check`) needs no browser: it loads each runner's table and fails when
  a mutation's text no longer occurs exactly once in its file. - **The coverage map**
  (`COVERAGE.md`) says which check proves each rule. - **The approved look**
  (`node/look-values.mjs`) holds the smoke's tuned numbers the node scenarios are written for. A
  number retuned in `TUNING` alone fails one check, `node/model.mjs` "the approved look", which
  names the number and the place to update. The browser checks that depend on a look number read it
  from the page's `TUNING`, so they follow it. - **The gate's refusal** (`gate_refusal.sh`) proves
  on a throwaway repository that the quick gate refuses after a smoke file changed.

The soak (`soak.py`) runs the smoke for half an hour and compares its frame rate and mean alpha
with a recorded run; its numbers, not a picture, are what is trusted. The detail of every check, the
tools needed (node, two browsers, geckodriver, a few Python packages) and the timings are in
[scripts/web-checks/README.md](../../scripts/web-checks/README.md).

## Other gates `make check` runs

- `make license-check` (`scripts/check-licenses.py`) fails on a licence GPL-3.0-or-later cannot use.
- `make speakers-fixtures` rewrites the synthetic SOFA files (made with `python3-h5py`, committed)
  and tests them. `make speakers-catalogue-check` (network; before a release, not part of `make
  check`) fetches each catalogue entry and checks its size, its checksum and that the converter
  accepts it.
- `make spdx-check` (`scripts/spdx-check.sh`) fails when a source file lacks its SPDX header; a
  test (`bins/svolder/tests/spdx_check.rs`) proves the checker on small trees.
- `make network-check` fails when `cargo tree -p svolder -e normal -i ureq` or `-i rustls` prints
  anything: only `svolderd` may link an HTTP client or TLS (the CLI and the TUI ask the daemon for
  AutoEq). It fails closed: only "nothing to print" or "did not match any packages" is clean.
  `make network-check-test` runs it on `svolderd` (`NETWORK_CHECK_PKG=svolderd`), which links both.
- `make unit-check` runs `systemd-analyze --user verify` on the filled-in `svolderd.service` and
  `svolderd.socket`, plain and with `svolderd-filesystem.conf` as a drop-in; output naming our units
  fails it. It also compares the drop-in block in `docs/usage/daemon.md` with that file, and prints
  `SKIPPED` for the verify half where `systemd-analyze` is missing.
- `make deps-test` (`scripts/build-deps-test.sh`) self-tests the script that names missing tools.
- `make release-dry-run` (`scripts/release-dry-run.sh`; before a tag, not part of `make check`)
  builds the packages, installs the amd64 `.deb` in a clean container, then starts the container
  image under the compose file a release attaches, on the fake device, and checks its log, `add`,
  `status`, `web off` then `on` and the log size cap. Its containers and image are named
  `svolder-dryrun-<random>` and removed at exit.
- `bins/svolder/tests/makefile.rs` reads the Makefile's choices off a dry run; nothing is built.

## Principles

**No test touches a radio, except `make e2e`.** The fake device answers real frames, so session,
daemon, TUI and web behaviour are proved in the ordinary gate on any machine; the one gate that
needs a radio says so and stays out of `make check`. A test that needs hardware cannot run in CI or
on a laptop without it.

**No check plays sound or touches the user's PipeWire session.** `make host-e2e` is the only one
that plays anything, and it does so on a private server: its own run directory as
`XDG_RUNTIME_DIR` and `PIPEWIRE_REMOTE`, its own WirePlumber, a null sink and no session bus. Every
PipeWire tool and the daemon run in that environment, `private-pipewire.sh proof` must pass before
the tone starts, the script exits 2 without the environment, a trap ends what it started, and it
lists your own PipeWire and WirePlumber processes before and after and fails if they differ (so
leave your session's audio alone while it runs, about a minute). It never runs `systemctl --user`
and never an installed svolder binary.

**A test never reaches the user's files.** `make test` runs in scratch homes and fails on a
leftover; a binary run by hand is given temporary XDG homes and `SVOLDER_TRANSPORT=fake`, never the
installed one. A test that wrote to a real configuration would change a person's setup.

**What is safety-critical is listed.** The volume rule, the firmware-update refusals and the
capture's argv are in `scripts/safety-tests.list`, so deleting or renaming a guard is a visible
act. A test no one can lose by accident protects the people the rule protects.

**A check must be able to fail.** The web checks have mutation runners, and a new test is seen
failing before it is trusted. A check that cannot go red is decoration.

**The gate fits the change.** Rust and docs run the ordinary gate; a page change runs the quick web
gate; a smoke change runs the full one; a transport change runs `make e2e`. A gate that costs half
an hour on every edit is skipped, and one that cannot say it was too small to stand in is trusted
when it should not be, which is why the quick gate refuses.

**Rules are proved where they are cheapest.** The smoke's rules are proved in node in seconds; only
what needs a GPU or a real page is proved in a browser. Moving a rule's proof is recorded in the
coverage map so it is never silently dropped.

**Fixtures match the real thing.** A fixture of an external tool's output (a sound server's, a
service manager's) is checked once against the real tool; a parser tested only against an invented
fixture can pass and fail on the real output.

**Numbers, not pictures.** The smoke's checks read pixels and float fields and compare them with
bounds; screenshots are for looking at, not for proving.

**Names are sentences.** A test is named for the behaviour it pins, so searching for a behaviour's
words finds its test, and a failing name reads as what broke.

**Docs that quote the build are tested against it.** The TUI mockups and the environment table are
compared with what the binary draws and prints; a doc that drifts fails a test.

## Where the code and tests live

- The gates: `Makefile` (`check`, `test`, `safety-tests`, `docs-check`, `web-checks`,
  `web-checks-quick`, `web-checks-task`, `e2e`, `host-e2e`, `demo`).
- The fake device and signal: `crates/svolder-fake/src/device.rs`, `signal.rs`; the fake transport:
  `crates/svolder-ble/src/fake.rs`; the simulator: `bins/svolder-sim/src/main.rs`, `peripheral.rs`.
- The end-to-end script: `scripts/e2e.sh`, with the live tests in
  `crates/svolder-ble/tests/bluez_e2e.rs`.
- The computer EQ's end-to-end check: `scripts/host-e2e/` (`private-pipewire.sh`, `run.sh`,
  `analyse.py`).
- The preamp lab: `crates/svolder-host/src/eq/lab.rs` (the model, the events, the driver) and
  `crates/svolder-host/src/eq/lab/tests.rs` (the short run, the long run, the trace, the
  regression sequences).
- The safety list: `scripts/safety-tests.list`; the documentation check: `scripts/docs-check.py`.
- The web checks: `scripts/web-checks/`; Void Garden's own are in [its checks page](web-voidgarden-checks.md), with `make voidgarden-measure`, which prints the cost of its frame budget on a real GPU and asserts nothing.
- The programs' tests that run built binaries against the fake: `bins/svolder/tests/` and
  `bins/svolderd/tests/`; the helper that spawns the daemon is `bins/svolder/tests/common/mod.rs`.
  The TUI in a pty: `bins/svolder/tests/tui_pty/`, with [its page](tui-pty-harness.md).
- The one way a test writes a script it then runs: `install_script` in
  `crates/svolder-host/tests/common/script.rs`, included by each crate whose tests stand a program
  in for a real one. A `sh` writes the file, so no thread of the test process holds it open when
  another thread forks; the run never fails with "text file busy".

[Back to the map](../architecture.md).
