<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The daemon's capture

The daemon captures what the computer plays so that a web page's visualizer and a joined TUI can
draw it from one place. The analysis itself (`Analyser`, `Frame`, the 240 bands, the beat) is
described in [host-audio.md](host-audio.md#the-capture); this page is how the daemon runs the
capture and hands the frames on. Nothing here runs unless someone looks.

## Holds

`HostService::frames()` (`bins/svolderd/src/host.rs`) gives a `FramesHold`, RAII like the route's
`Watcher`, and `None` when the host is off. A hold exists for as long as a web page's frames stream
or a joined TUI's frames connection lives. The first hold starts one capture of the default sink
through `PwFrames` and the `Analyser`; the capture stops 2 s after the last hold is let go, and a
hold taken inside those 2 s keeps it. With no hold nothing is captured, analysed or read: no
`wpctl`, no `pw-record`.

The capture is a helper of the current run (`host/capture.rs`), so a reconfigure, a stop or the
generation gate ends it, and dropping it kills its `pw-record`; a step of a run that was halted
publishes nothing. While it runs the helper reads the default sink every 2 s on a task of its own,
so a slow `wpctl` never holds up the frames, and puts the capture on the node the sink names, so a
switch of the default sink moves it. It only reads (`Sink::read`): the daemon never sets the sink's
volume or mute. `pw-record` is passive (`node.passive = true`), so the tap never keeps the sink
awake or the graph busy and watching does not change the sound; a sink that suspends gives it no
samples, and the page and the TUI then show silence. It also sets `node.rate = 0`: measured on
PipeWire 1.6.2 with several allowed rates, a passive capture alone still started the graph at
48 kHz, because `pw-record` suggests a rate of its own, and a non-passive one held it there,
while with both properties a 44.1 kHz and a 96 kHz stream set the graph's rate and the capture
kept receiving samples. `--rate 48000` stays, so only our stream is resampled. `make rate-check`
repeats the measurement on a host (the real session, silent streams, never part of `make check`);
what `make check` does run is `make rate-check-test`, the script's parsers on saved tool output
(`crates/svolder-host/tests/fixtures/rate-check/` and the `pw-dump` fixtures).

A hold that outlives a reconfigure sees the capture go `Unavailable` ("host audio is off") and gets
the new run's capture when the host comes back.

## What a holder hears

Frames go to every holder on one broadcast channel of 8, so a holder that reads slowly skips to the
newest and never slows the capture or the others. `CaptureState` rides a watch channel:

- `Starting`: nothing runs yet, or it waits for a first holder;
- `Running { sink, target }`: `sink` is the sink's name for people (its `node.description`, else
  its node name), which the card shows as a label; `target` is the node the capture runs on;
- `Unavailable { why }`: a plain reason, tried again at the next look (every 2 s while held): no
  `pw-record`, no sink to read, a capture that ended on its own, a host that is off, or a host with
  no capture tools at all, which says so from the start.

`FramesHold::events` is the stream both front ends read: the state now, each change of it, and the
frames, only while the capture runs. The stream owns the hold, so dropping it lets go.

## When the capture fails

A capture that cannot start, or that ends within 10 s of starting, is a failure, and the next try
waits: 2 s after the first, then 4, 8 and so on to 30 s, so a missing `pw-record` or a sink that
will not take it is not run again every look. A capture that has run for 10 s is a good run and
clears the count, whether it ended or was moved to another sink. One failed read of the sink keeps
a running capture; two in a row give it up, since one `wpctl` that is slow or busy is not a sink
that went. A page that comes back after the capture lingered out starts at once and does not wait
out a back-off from before it left. The reasons are the card's quiet line, and the next try is
the next look.

## The sources

`Parts::frames` is `Result<Frames, String>`: the capture and the sink it follows, or why there is
none. The real host builds a `PwFrames` (the `pw-record` `SVOLDER_PW_RECORD` names) and a `Wpctl`;
the fake host builds `SyntheticFrames` over a `FakeSink`, and tests fill it with `FakeFrames` and
`FakeSink`.

The fake host's capture (`host/synthetic.rs`) is synthetic music made as samples at real time and
run through the same `Analyser`, only while started: a 120 BPM kick, a bass line that changes every
two seconds, off-beat hats from a fixed noise seed and a pad that pans slowly from left to right,
the same every run. `SVOLDER_FAKE_CAPTURE=off` gives the fake no capture, so a check can see the
unavailable card.

## Over the web

`GET api/v1/host/frames` is described in [web-ui.md](web-ui.md#now-playing): server-sent `hello`,
`capture` and `frame` events, the frame's bytes in base64. It is a stream of its own and not part of
the device's, because frames are the one thing that costs the daemon work in proportion to who
looks: the device stream is open for every page, and only the page whose card is on show opens
this one, closes it when the card is not, and so holds the capture only then. The frame is JSON
with base64 bytes because a page decodes that with no code of ours; the byte scale is a tuning that
may change and nothing is promised about it, and the encoder is a small function of ours, since
the base64 crate is in the tree only as another crate's dependency.

## Over the socket

`Request::HostFrames` turns its connection into an event-only stream, as `Subscribe` does, and holds
a `FramesHold` for the connection's life. Its events are `Event::HostCapture(CaptureView)`
(`{"event":"host_capture","state":"starting|running|unavailable","sink":…,"why":…}`, the last two
only when they apply), once at the start and on each change of the capture, and
`Event::HostFrame(FrameView)` (`{"event":"host_frame","t":<unix ms>,"left":[240 bytes],
"right":[240 bytes],"levels":[rmsL,rmsR,peakL,peakR],"beat":bool,"bpm":n,"phase":n}`) about 30
times a second while it runs. `FrameView::to_frame` gives the `Frame` back, and `Frame::block()`
the TUI's own block.

A daemon that cannot give them (one from before the request, or a host that is off) answers with
an error `Response` and the connection goes on serving; `Client::host_frames` returns that as an
error, which a TUI reads as "capture on my own". The request is not a `Subscribe` option, so a
front end that wants frames only while its spectrum shows opens and drops a third connection
without the device snapshot being sent again. A client from before the events skips them as events
it does not know.

## A joined TUI

A TUI joined to a daemon whose host side is available reads the frames instead of recording the
host itself (`bins/svolder/src/tui/run/host/frames.rs`). The connection is a third one, beside the
request and `Subscribe` connections, opened by `HostLink::steer_frames` after each drawn frame
while three things hold: the session is on a daemon, the daemon's host side is available
(`Host::daemon_owns`) and `App::capture_target` wants a capture (a Spectrum row on the screen, or a
level for the title meter, the breathing borders, the hue drift or the focus runner). When any
stops holding the connection is dropped, which lets the daemon's hold go; a device switch, the
add-device walk and the loop's end drop it too, and the first frame after restarts it where it is
still wanted.

Only the TUI's own `wpctl` read of the sink says there is one to record, so a TUI with no `wpctl`
never asks the daemon. In direct mode the TUI records for itself and bins the audio into its 16
bands as before; joined, it bins the daemon's 240 down to them. While the connection is opening the
loop's own `pw-record` keeps feeding the row, and it stops when the daemon's first frame lands, so a
slow daemon never leaves the row without data and the host is recorded once afterwards (both
captures are passive, `node.rate = 0`, so the short overlap holds nothing of the graph). A frame
lands through `Frame::block()` and `App::heard_frame`, as a block of the loop's own capture does;
its beat flag starts `Host::pulse` (200 ms), which draws the Spectrum row bold and lifts the
breathing borders' level for that long (and puts the loop on its fast tick for it; the row pulses
whatever the level's source is, the borders only where the host's capture is it). The capture's
`Running` state marks the capture running; `Unavailable` drops the connection and asks again after
30 s, as does a daemon that does not answer within 3 s (the loop's own capture feeds the row
meanwhile), the connection ending after 5 s, a bar for a time keeping until then however briefly the
frames are not wanted, and a refusal (an old daemon, or one with no host audio) holding until
another daemon speaks for the host. In each case the loop's own capture feeds the row meanwhile, and
direct mode never asks.

## Principles

**Nothing runs when nobody looks.** No `wpctl`, no `pw-record`, no analysis and no frame is made
for a daemon that has no hold. The capture is held, not followed: a front end that wants the
spectrum says so by holding, and lets go by dropping the stream.

**Watching never changes the sound.** The daemon only reads the sink and only listens to its
monitor, passively; it never sets a volume or a mute, never links itself in front of anything and
never keeps the graph awake. A front end that would show the music must not alter it.

**One capture, however many look.** Two pages and a TUI share one `pw-record` and one analysis, and
a slow one skips frames and never slows the others. The frames are made once and encoded per
connection, only when that connection has room for them.

**The picture is about the music, not the DAC.** The capture follows the computer's default sink
whatever it is, and the card says which sink it is when it is not the page's DAC; with no capture
possible it says why, quietly, and keeps its box.

**An old peer is never broken.** The frames are a request and a stream of their own, not an option
of `Subscribe`: a daemon from before them answers a bad request and a TUI falls back to its own
capture; a client from before them skips the events.

## Where the code and tests live

`bins/svolderd/src/host.rs` and `host/capture.rs`, `host/synthetic.rs`; tests in
`bins/svolderd/src/host/capture/tests.rs` and `bins/svolderd/src/handler/host_tests.rs`. The web
route is `bins/svolderd/src/web/host/frames.rs` with its tests beside it; the wire types are in
`crates/svolder-ipc/src/host.rs` and the request in `crates/svolder-ipc/src/socket/`. The TUI's
side is `frames.rs` beside `bins/svolder/src/tui/run/host.rs`, tested in `host/joined.rs` against
the stub daemon (`test_support/stub_daemon.rs`, `frames(..)`) and in `host/tests.rs`.
