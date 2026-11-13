<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Streams, jobs and long requests

The socket's other half besides request and response (see
[ipc-and-configuration.md](ipc-and-configuration.md)): requests answered with events, work that
takes seconds or more, and what the daemon does with it as it stops.

## One stream path

`Request::is_stream()` says which requests are answered with events (`Subscribe`, `HostFrames` and
the `follow` op of the `Walk` and `AutoEq` families). The server asks `Handler::stream(request)`
for the events or for the `Response` that refuses it (written as any answer, the connection serving
on); the client opens one with `Client::stream(request)`. Neither goes by the request's kind.
`Handler::subscribe` stays the device stream the default `stream` serves; the default refuses the
rest. A stream request is sent without the default request bound.

A family's stream opens with a `Response::Ack` under its id (`Request::stream_acks()`), so a client
knows it is open while no event comes. The two older streams keep their wire, with no ack, and
`Client::stream` takes their first event, or an error, as the opening (`Request::stream_waits()`
is the one table for which streams a client waits on). A refused stream is
`IpcError::Refused(answer)`: the daemon's whole answer, code and reason included, so `busy` and
`no_such_job` are told apart by reason. A daemon that does not know a family answers code 7 (1
before 0.33.0). The wait for the first line has no bound of its own; a caller wraps the call.

## The families

`Request::Walk { op: WalkOp }` and `Request::AutoEq { op: AutoEqOp }` follow `HostEq` (wire names
`walk` and `auto_eq`), each in its own file under `crates/svolder-ipc/src/socket/protocol/`
(`walk.rs`, `autoeq.rs`; `jobs.rs` holds what they share). Every family has `cancel { job }` (an
acknowledgement meaning nothing runs under that id now, also for a job that ended; `no_such_job`
for an id the daemon does not hold) and `follow`. The walk adds `start`, `answer` and `back`, under
the feature `walk` ([device-walk.md](device-walk.md)); AutoEq's ops that start work are in
[autoeq.md](autoeq.md), under the feature `autoeq`. Both features are listed, each once its ops are
usable. An op a daemon does not know inside a family it does know does not parse and is answered
code 1, never code 7, which names a request kind: a front end gates an op on a feature name, so an
op added after a family's first release is listed under its own feature. New refusals carry typed
reasons: `no_such_job`, `busy`, `not_waiting` (an answer to a walk that does not wait for one),
`internal` and `stopping`.

## Jobs

A job is daemon-owned (`bins/svolderd/src/handler/jobs.rs`). A request that starts long work asks
its family's registry to start it and answers at once with the job's id, as `Response::Job { job }`
(`{"result":"job","job":4}`; `Jobs::start_answer` builds it, and a refusal is the usual error with
its reason). The work is a task the registry owns. Each change is an `Event::Walk` or
`Event::AutoEq` carrying a `JobEvent`: the `job` id and a `state` (`kind` `started`, `progress`
with an opaque `payload` the family fills, `done`, `failed` with a `message` and a `Reason`, or
`cancelled`). Ids never repeat within a daemon run.

A job's result is in `done`: the value the job's work returns (`()` for none) is its `payload`,
opaque to the mechanism and left out of the line when `null`. It is part of the job's last state,
the one `follow` replays to a subscriber that joins after the end, lags past the backlog, or
reconnects, so a result never depends on having caught a `progress`. It may weigh at most
`JOB_RESULT_MAX_BYTES` (512 KiB as JSON); a larger one ends the job as `failed` with the reason
`internal`. The registry remembers the last state of the newest 16 ended jobs, as many of them as
fit in `REPLAY_BYTES_MAX` (1 MiB, two full results; the newest is always kept), so a result is
there for that long: a front end reads it when it sees the `done`. A progress payload over
`PROGRESS_MAX_BYTES` (64 KiB) is dropped and logged: progress says where the work stands, the
result carries data. Both bounds keep what a follower that opens or lags is sent small (it was
up to 8 MiB). Both additions are new on the
wire (an older front end never sends a start), and `done` without a `payload` still reads.

The slot policy is a parameter of the registry: one job daemon-wide for the walk (a second start is
refused as `busy`, naming whose job is in the way), a cap for AutoEq. `follow` first gives the last
state of each remembered job (the newest 16 that ended, and every running one), then each change.
The stream tells states, not deltas: a subscriber more than 64 events behind is not waited for and
is given where the jobs stand again, so a state may repeat and a reader never counts events; a slow
reader holds nothing up. Blocking work runs through `spawn_blocking` with a stop flag it checks
between steps.

Each family is one service (`walk.rs` and `autoeq.rs` under `bins/svolderd/src/handler/`) that
owns its registry; `Daemon` routes it in one line of `handle_from` and one of `stream`. The web and
MCP reach the families through `Fleet::request` and `Fleet::stream` only (the latter on the same
streams the socket serves); `Fleet` has no method per feature. Tests: `handler/jobs/tests.rs` and
`handler/families_tests.rs`.

## Long requests

A front end sends a long or streamed request on a connection of its own, as subscriptions always
were (`request_on_own_connection`; `daemon_long_request` and `daemon_stream` on the backend), so
the connection every read and write goes over is never held behind it. Each server connection is
its own task, so a long request holds up none on another (a test pins it).

## Shutdown

`Daemon::shutdown` runs in this order: the walk's and AutoEq's jobs (up to `JOB_STOP_BOUND`, 3 s;
a job still running is given up on and reported cancelled), the MCP and web servers, the host, the
MQTT bridge, every actor at once, the walk's probe among them (up to `SHUTDOWN_DEADLINE`, the
farewell bound and six seconds),
and last any configuration write already under way (up to `WRITE_SETTLE_BOUND`, 3 s). `svolderd`
builds its runtime by hand and ends it with `shutdown_timeout(RUNTIME_STOP_BOUND)` (3 s): dropping
a runtime waits for every blocking thread without a limit, which a stuck step of a job would turn
into a hung `svolder daemon stop`. Because writes are awaited before that, the bounded end cuts no
write; it can cut a read or a job's stuck thread.

After `Daemon::shutdown`, `main` settles the transport (`CLOSE_GRACE`, 8 s: connects and releases
still in flight) and then ends the runtime. The bounded steps are therefore: jobs (`JOB_STOP_BOUND`
3 s; the walk's and AutoEq's run at once), the MQTT bridge (`BRIDGE_STOP_BOUND`, 4 s), actors
(`FAREWELL_BOUND` 24 s and `ACTOR_STOP_MARGIN` 6 s; all asked at once), writes (`WRITE_SETTLE_BOUND`
3 s), the transport's settle (8 s) and the runtime's end (`RUNTIME_STOP_BOUND` 3 s): 51 s. The MCP
and web servers stop on a cancel and a join of their accept loop, and the host aborts its tasks;
none has a bound. `svolder daemon restart` waits `LOCK_WAIT` for all of it: that sum and two seconds
more (53 s), built from those same named constants. A test in the `svolder` binary pins the sum. A
test in svolderd reads the source of `Daemon::shutdown` and `main` as a tripwire for those two
bodies, not for what they call: a bound inside a called function is found by reading what each
step calls. The socket's lock is held to the end of the process (`run` leaks the file past the
runtime's end), so the sum measures the time a new daemon waits to take it. The service unit sets
`TimeoutStopSec=60`, above that sum, so it does not lean on a service manager's default (systemd's
is 90 s); `svolder daemon stop` waits only for the socket to go (5 s), which happens before any of
it. The wait for writes covers the ones the daemon started, not one a socket connection sends
after it. Two commands typed at once (stop, then start) can see the new daemon refused as already
running until the old process has ended, up to the stop's whole duration; a restart waits for it.
