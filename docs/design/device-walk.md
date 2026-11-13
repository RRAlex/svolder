<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The device walk

Adding a device is one walk, whoever drives it: listen for devices, pick one (and a profile for a
product no profile selects), name it, test its passphrase, and write it into the configuration.
The daemon runs it as a job, which the web's *+ Add a device*, the socket, `svolder setup`,
`svolder add` with no unit id and the TUI's `n` drive; only with no daemon do those three run it in
their own process. The words a person reads are the walk's, so every front end says the same.

## The crate

`crates/svolder-walk` holds the walk and depends on `svolder-ipc`, `svolder-profiles` and
`svolder-proto`, never on `svolder-ble`. `DeviceWalk<E: WalkEffects>` is one state machine:
`prompt()` renders the current step as a `Prompt`, `answer(choice, text)` takes an answer and does
what it asks for through the effects, `back()` steps back. Its steps are `Linked`, `Scan`, `Pick`
(with the profile question), `Name` and `Passphrase`; `answer` reports `Walked::On`, `Added`,
`Skipped` (a quiet scan given up on) or `Cancelled` (at the linked step), or a `WalkError`.

`Prompt` and `Step` are serde types. `Step` names every step of a guided run, the front end's own
(`Health`, `Fixes`, `Adapter`, `Daemon`, `Done`) too, so one vocabulary covers what a person is
shown. A prompt says what answer it takes (`takes`: `choice`, `text` or `secret`), so no front end
guesses from the step; a prompt that takes text has no choices, and its last body line is what an
empty line answers.

`WalkEffects` is what the walk asks of the machine: `scan`, `scan_window`, `try_connect` (answering
`Connect`), `write_device`, the names and units configured already, the registry, and `linked`
(the configured devices that hold a link). Its slow methods are futures without a `Send` bound:
the front end's run on one thread, and the daemon's are `Send` because its types are, which the
compiler sees through the concrete `DeviceWalk` a job runs.

`WalkError` keeps refusals apart: an effect that did not happen, an answer the step does not take,
a name taken, a configured unit picked (naming its device), a unit another block took at the
write, a code turned away, a test that got no answer (`Connect::Failed` with its kind), no device
picked. A write that a file cannot take, and a scan that could not run three times in a row, stop
the walk (`DeviceWalk::stopped`); every other refusal leaves the step to be asked again. A name
taken at the write sends the walk back to its name step, a unit taken back to its pick step.

A passphrase is held as a `Secret`: `{:?}` prints `<hidden>`, it has no `Display` and no serde. It
is never in a prompt, a state, a refusal, a log line or `Debug` output.

## The front ends over a daemon

A front end asks `DaemonInfo::supports` for the feature the work needs (`walk`, `device_crud`,
`server_switch`) through `bins/svolder/src/reach.rs`, and the answer has three shapes. No socket, or
nothing listening on it: no daemon, and the front end's own writer and its local walk stand
(`svolder setup` before any daemon exists). A daemon with the feature: the work goes over the
socket, and the file is never written from this side. A daemon without it, one that does not say
what it is, or a socket that cannot be tried: a refusal in words (`svolder daemon restart` for the
first), and nothing is written behind it. A front end writes `[[device]]` itself only in the first
case.

`bins/svolder/src/wizard/remote.rs` (`RemoteWalk`) is the walk's client: `start` on a connection of
its own, the states from a `follow` on another, and each `answer`, `back` and `cancel` on a
connection per request, so a request given up on never leaves a half line on one the next needs. It
acts on the job its own start answer named and no other: states of other jobs in the replay are
skipped, and leaving cancels only that job (a dropped `RemoteWalk` does too, and so do SIGINT,
SIGHUP and SIGTERM while a walk runs: `wizard/signals.rs`, exit 130). An answer is settled by a
state that shows it was taken (the walk working, a new prompt, the end), never by the state it was
given against sent again. The wizard skips the Adapter step with a daemon, as the daemon opened its
adapter at start, and tells it nothing of the device: the daemon wrote it. A refused start (`busy`)
shows the daemon's words, which say where the running walk was started; a terminal never cancels
another front end's walk. `--name` stands for an empty line at the daemon's name step; `--profile`
and `--passphrase` (when not the default `auto`) answer the daemon's profile question and passphrase
step by themselves, once; a product with no profile question gets the profile, and a device that
took `auto` the code, as an `update` after the walk's end, a refusal said on the last step.

The TUI's device editor and the CLI's `add` and `remove` send `device_config` ops by the same rule.
`clear_default` leaves no default (the TUI's "no default" is its own op under `device_crud`, which
is unreleased and needs no feature of its own); the TUI's Web UI and MCP on/off rows are the
daemon's `server_switch` (the other keys of those boxes are still written to the file and reloaded).
The live code on the link (`Request::Passphrase`) is separate from `update { passphrase }`, which is
the file's.

## The front end's wizard

`bins/svolder/src/wizard.rs` keeps `Health`, `Fixes`, `Adapter` and `Daemon` and embeds the walk
with `Local` effects over its own (`wizard/local.rs`: the radio, `Config::add_device`, and a
refusal of the file as the stop it always was). It leaves a configured unit out of the pick, with a
line saying so (`Configured::LeaveOut`), and never warns: its effects name no linked device. It
maps a `WalkError` to the `Failure` and exit code its kind has.

## The daemon's walk

`bins/svolderd/src/handler/walk.rs` runs one walk daemon-wide over `DaemonEffects`
(`handler/walk/effects.rs`):

- **Scan**: one listen of the shared scanner's that hears every device for `WALK_SCAN_WINDOW`
  (10 s), each row on the walk's state as it is heard, the first 64 (`ROWS_MAX`) offered.
- **Pick**: a configured unit is offered marked with its configured name
  (`Configured::Mark`); picking it is refused, naming that device.
- **Passphrase test**: a probe actor (below), `auto` first, then the code typed.
- **Write**: the `device_config` add, through `edit_then_reload` and its refusals
  (`ConfigWriteError::answer`): `taken` keeps the walk on its name, `invalid` for a unit the
  snapshot now holds sends it to its pick, and any other `invalid` (the file holds one unit twice,
  a profile went away), `read_only`, `file_busy` or `stopping` stop it. The name step judged the
  name by the device writers' own rule (`device_name_refusal`: blank, over 40 characters, a
  control character), so a write is never refused for the name's form.

The walk ends `done` with the device as `device_config` answers it (`ConfiguredDevice`, no
passphrase), `done` with nothing when a quiet scan is given up on, `cancelled` by a cancel, the
linked step's Cancel, the daemon's stop, or `WALK_IDLE_BOUND` (10 min) with no answer, and
`failed` when an effect stops it. The reload the write sets off connects the device.

### The warning

While any configured device holds a link, the walk's first prompt (`Linked`) names those devices
and says the scan and the passphrase test may briefly disturb them, with Go on and Cancel. With
nothing linked the walk starts at its scan. Whether a held link survives a scan is for hardware to
answer; the warning stands either way.

### Ops, inbox and state

`WalkOp` (`crates/svolder-ipc/src/socket/protocol/walk.rs`) has `start`, `answer { job, choice,
text }`, `back { job }`, `cancel { job }` and `follow`; every op but `start` and `follow` names its
job. The op is read by hand, so a value of the wrong type is refused naming its key and never the
value, and `{:?}` hides an answer's text. The feature `walk` says a daemon has them.

`start` answers `Response::Job`, or `busy` while another walk runs; the busy refusal adds no field:
the running walk and its origin are in the follow stream's states. A job has no inbox, so the
service keeps one channel per walk: `answer` and `back` are acknowledged when the walk waits for an
answer, refused `not_waiting` while it works or once it has ended, and `no_such_job` for an id the
daemon never had.

Each progress state is the walk's whole state (`svolder_walk::WalkState`): its `origin`, `phase`
(`waiting` or `working`, with `working` saying `scanning`, `testing`, `writing` or `answering`),
the `prompt`, the `rows` heard (each a `ScanEntry` with `configured` for a marked unit),
`can_go_back`, the `refusal` the last answer met (`message` and `kind`) and the last test's
`connect`. A follower that comes late or reconnects reads everything from the replay of the last
state; nothing arrives as a delta.

## The configured devices

While a daemon runs it is the one writer of the `[[device]]` blocks and of `default_device`. The
`device_config` request (`DeviceConfigOp`, feature `device_crud`) carries the ops: `list`, `add`
(unit, name, and profile and passphrase where wanted), `update` (`name`, `profile`, `passphrase`,
each changed only when the op carries it; `profile: null` takes the pin off), `remove` and
`set_default`. Every op that acts on a device names it, by configured name or whole unit id, neither
case-sensitive: no op means "the shown device". The handler (`handler/device_config.rs`) makes the
edit through `edit_then_reload` and the core's writers that take a selector (`Config::patch_device`,
`remove_selected_device`, `default_to_device`), which find and write the device under the file's one
lock, so two writers at once take turns and neither loses a change. It answers
`Response::DeviceConfig`: one `ConfiguredDevice` per block, in the file's order (unit, name, pinned
profile, kind of passphrase (`auto`, `none` or `set`), default mark, whether a link is up and, when
none is, since when), and `profiles` (`ProfileChoice`: name, display, base mark).

A refusal goes through `ConfigWriteError::answer` and reads `taken` (the name), `missing` (no such
device), `invalid` (a name that is none, a bad unit id or passphrase, a unit another device has, a
profile the answer's `profiles` do not list, the one list), `read_only`, `file_busy` or `stopping`.
The device writers refuse a blank name, one over 40 characters and one with a control character, and
write a name without the blanks round it; a block keeps the name it has. The change reaches the
actors through the reload the write asks for: a changed passphrase or profile goes to the running
actor on its link, a removed device's actor closes its session, a renamed one is stopped and started
under the new name. Until that reload has run, `devices` and the streams show the device as it was.
The web's routes send the same request through `Fleet::request`.

An op may carry a passphrase, so nothing of one is shown back. `DeviceConfigOp` is read by hand
(serde quotes a value of the wrong type): an op that does not parse is refused naming the key at
fault, never what it held, on the socket and the web alike, and `{:?}` prints a passphrase as
`<hidden>`. A refusal of a well-formed op names its device or profile, never a passphrase.

## The probe

The passphrase test is a short-lived device actor (`handler/probe.rs`): `device::spawn_probe` with
a temporary `DeviceConfig` for the unit, the same discovery, profile, passphrase (with `auto`'s
re-read after a refusal) and connect as a configured device's actor, which reports how its first
attempt ended. It answers `Connect::{Accepted, Refused, Failed { kind }}` (`not_in_range`, `link`,
`held`, `stopping`), and is closed with `Command::Shutdown` once the answer is in, before the walk
writes. `PROBE_BOUND` (75 s) bounds the wait for that first attempt.

It lives in the device set's `probe` slot beside `starting`, entered under the set's lock: it is
never started after the stop's mark, `Daemon::shutdown`'s actor step closes it with the other
actors at once (so `LOCK_WAIT` holds), `reconcile` starts no actor for the unit it holds, and it is
refused a unit an actor holds. A walk cancelled mid-test closes its probe before it ends; one cut
short by the daemon's stop leaves it to the stop.

## The scanner

The `scan` verb and the walk's scan are one listen (`Scanner::scan_each`) rather than a raw scan:
it hears every device for its window, hands each unit's first advertisement over as it is heard,
and answers the actors waiting for a device it hears once it has stopped, so their connects run on
an adapter that no longer scans. The shared listen, its join grace and its held-back answers are
unchanged. A configured device such a listen hears takes the hearing as its signal
(`DeviceHandle::heard_on_air`, through `Daemon::listen_all`); the shared listen feeds an actor's
signal through its own attempt. Every scan row, the daemon's and a front end's, is built by
`ScanEntry::from_advert`.

## The web

`POST api/v1/walk` and `api/v1/walk/{job}/answer|back|cancel` (`bins/svolderd/src/web/writes/
walk.rs`) send the ops through `Fleet::request` as the web's; the states come on the page's events
stream as `walk` events with `needs=walk`, one follow per page. Signed-in writer only. The routes
are in [the web UI usage page](../usage/web-ui.md).

## Tests

`crates/svolder-walk/src/tests.rs` (the walk's own rules), `bins/svolder/src/wizard/tests.rs` (the
wizard over it), `handler/probe/tests.rs`, `handler/walk/tests.rs` (the whole walk on the fake
transport, busy, a late follower, cancel, back, the idle bound on a paused clock, a stop mid-test),
`web/writes/walk/tests.rs`, and the walk section of `scripts/e2e.sh` on two controllers, where the
sim advertises a code it does not hold (`svolder-sim --advertised-passphrase`) so `auto` is
refused.
