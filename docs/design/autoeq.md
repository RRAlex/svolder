<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# AutoEq in the daemon

`svolderd` is the one process that reaches AutoEq's repository. It holds the index, downloads a
measurement's correction, fits it to a tone control and saves it as a preset; every front end
(the web page, the TUI and the CLI) asks it. The feature name is `autoeq`
(`FEATURE_AUTOEQ`, listed in `FEATURES`; see [ipc-and-configuration.md](ipc-and-configuration.md)).

## The crates

`svolder-autoeq` has two halves. `index` parses the index text, searches it and holds the words
and rules both sides share: `Entry::group_key` (the name as lowercase letters and digits; equal
keys are one headphone, whoever measured it), `Entry::measurement` (`source · form[ · rig]`),
`Entry::origin` and `Refresh::of` / `Refresh::is_stale` (when the index is fetched again). It has no
network dependency. `client` (ureq, rustls) is behind the feature `net`, off by default;
`svolderd` enables it. `make network-check` fails while `cargo tree -p svolder -e normal -i ureq`
or `-i rustls` prints anything: the `svolder` binary must not link an HTTP client, since only the
daemon reaches the internet. Tests that need a repository use the feature `test-server`
(`svolder_autoeq::test_server`, a loopback server on a port the system picks); no test reaches a
network.

## The ops

`Request::AutoEq { op }` (wire name `auto_eq`), ops in `socket/protocol/autoeq.rs`:

| op | kind | answer |
|---|---|---|
| `search { query, refresh }` | plain request | `Response::AutoEqSearch`: groups, then the count of the rest |
| `preview { variant, target, compare? }` | job | the result in `done`: `AutoEqPreview` |
| `import { variant, name?, force, target }` | job | the result in `done`: `AutoEqSaved` |
| `upload { text, name, force }` | plain request | `Response::AutoEqSaved` |
| `cancel { job }`, `follow` | the family's | as in [jobs.md](jobs.md) |

`search` runs on every key of a search box, so it is not a job (a job would take one of the four
slots and need a cancel per key). `preview` and `import` download a file (at most 20 s, 64 KiB) and
then fit or save, so they are jobs: cancellable, a fifth start refused `busy`. `upload` reaches no
network. A query over 60 characters is refused (`invalid`).

A **variant** names one measurement as the index lists it (`name`, `source`, `form`, `rig`). The
daemon looks it up in the index it holds, so only a listed measurement is ever fetched (a name or
URL a client makes up is never followed); an unlisted one is `not_listed`. A **target** is
`{"side":"device","device":NAME}` or `{"side":"computer"}`; an op that depends on a side names it
and there is no default side (leaving it out is a 400). A device's band count comes from its live
profile, or, while it has not been found, from the profile the configuration names for it; with
neither the op is refused `not_found_yet`.

A search answer groups the matches by headphone (`group_key`), each source under it in the
index's own order, so the first is AutoEq's recommended one; at most 200 groups and about 900 KiB
of them (the rest is counted in `more`), with the index's date (`fetched_unix`), its size
(`measurements`) and its `note` (a refresh failed and the cached copy was used).

A preview is `svolder_profiles::tone::fit::Preview` on the web's 181 log-spaced points: the target
(the correction without its preamp), and for a device the fit's response, the gap spans (runs
where the fit misses the target by more than `GAP_DB`, 2 dB), the RMS and worst miss and where,
and the peak (the volume to lower by). The computer's preview has no fit: the correction as it
plays, refused with `invalid` past 16 filters. `compare` gives a second measurement's preview on
the same grid. The numbers are worked out once, in the daemon: front ends draw them. The miss is
read on those 181 points for a fit's own bands too, so the preview, the overlay and the TUI give
one figure for one fit; the preset lists' fit line keeps the fit's own finer grid (the fit as
found), and the TUI shades the gap per column at its own width, so an edge can differ by a column.

An import saves `Preset::imported` with `Entry::origin()` through the configuration writer
(`Daemon::save_preset`), so a refusal carries the writer's own reasons (`taken`, `bad_name`,
`read_only`, `file_busy`, `stopping`). A computer target refuses a correction of more than 16
filters. A refusal that is not the writer's: `not_listed` (not in the index, or a 404),
`unreachable` (no index, a failed or timed-out fetch, a file too large or not text), `invalid` (not
a parametric EQ, a query too long, a computer over its limit), `missing` (no such device),
`internal`.

## The service

`handler/autoeq.rs` owns the job slots and routes the ops; `handler/autoeq/core.rs` holds what they
share. `main` reads `SVOLDER_AUTOEQ_URL` and the cache directory (`cache_home()/svolder/autoeq`)
once and hands the `Client` to the daemon (`Daemon::with_autoeq`); a daemon given none answers
`internal`, so a test that forgot reaches nothing.

- The index loads one at a time, in `spawn_blocking`. Requests that arrive while it loads share its
  outcome, a copy or the failure; a copy the policy (`[autoeq] refresh_weeks` from the snapshot)
  still takes is returned as it is, kept in memory for the run.
- Downloaded corrections are kept by measurement (32, the oldest goes), so a preview and the
  import after it fetch the file once.
- Fits run in `spawn_blocking` behind a semaphore of two.
- A cancelled job stops waiting at once; a download already under way finishes on its thread,
  bounded by the 20 s limit.

## The CLI and the TUI

`bins/svolder/src/autoeq.rs` is the one place a front end sends the ops: `search`, `import` and
`upload`, each over a connection of its own, each ending in the daemon's answer or a `Refused`
(its words and its reason). `import` starts the job, opens the follow stream and waits for the
job's end by its id (the stream tells each job's last state first, so a job that ended before the
stream opened is read all the same); a call dropped while its job runs cancels the job. A daemon
from before the feature answers the request as an unknown one, which reads as "restart it".

`svolder` links no HTTP client (`make network-check`), so with no daemon there is no AutoEq: the
CLI's `eq search` and `eq import --autoeq` say AutoEq needs `svolderd` and name `svolder daemon
restart`, and so do they under `--transport ble` and `fake`, which open none. The CLI asks for the
`autoeq` feature as `device_crud` callers do (`reach::daemon_runs`). `eq import FILE` goes through
`upload` while a daemon runs, so the daemon stays the one writer, and writes the file itself with
none. The CLI's `import --autoeq` names the computer as the side: the preset goes to the library,
which no side owns, and the side only bounds a correction to 16 filters.

The TUI's search box (`tui/boxes/presets/search.rs`) holds no index. Each change of its query is
a numbered `Ask`; the loop waits 200 ms for the typing to pause (`Jobs::due`, one `select!` arm)
and sends the last one on a task of its own, stopping the one still in flight; the answer comes
back as `Done::Searched(ask, result)`, and the box takes it only when `ask.seq` is its last one,
so an answer that arrives after a newer question, or after the box closed or opened again (a
switch of device or side closes the list), is dropped. The box opens with an empty-query ask, so
the daemon loads its index while the status line says `fetching the index…`. `Enter` settles the
name first (a taken one opens the name prompt with `NAME (source)`), then has the daemon import
for the side the list shows (`Device { name }` or `Computer`); the answer has the presets read
from the file again, which says it in a toast and focuses the new row.

## The web routes

`api/v1/autoeq/{search,preview,import,upload}` (`web/writes/autoeq.rs`) take the op's own keys as
the JSON body and answer once. A route that starts a job follows it on the daemon's own stream
and waits for its end (at most 150 s), and cancels the job when the HTTP client goes. Every route
is a signed-in writer's, search included (401 for a guest, 403 with `[web] writes` off): a guest
never makes the daemon reach the internet. The upload's body may be 128 KiB (the text is at most
64 KiB; the rest is JSON escapes); the other routes' 8 KiB; larger is 413. A refusal is
`{"error", "reason"}`, `not_listed` 404, `unreachable` 502 (`router::status_of`).

`POST api/v1/devices/{d}/presets` takes `force`: it writes the device's bands over the user's
preset of that name. A built-in, an imported preset and the computer's side are refused.

The tone view's curve carries the overlay while an imported preset is on the device, or was
recalled and the bands differ (the TUI's rule): the preset's correction against the bands standing
on the device now (`Preview::of_bands`), so an edited band moves the fit, the gap and the RMS as it
moves the curve, as `tone.overlay` in the page model (absent otherwise, and while a band is unread).

## Shipping

The daemon verifies HTTPS against the system's certificates (`/etc/ssl/certs`): the packages
depend on `ca-certificates`, and the container image installs it (its Ubuntu base has none). The
opt-in filesystem drop-in makes `CacheDirectory=svolder` writable for the index. In a container
the image sets `XDG_CACHE_HOME=/config/cache`, so the cache is in the `/config` volume and survives
a recreate.
