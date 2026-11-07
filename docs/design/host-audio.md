<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Host audio

A DAC is fed by a computer, and the computer has audio of its own: a volume, a media player, a
signal that is about to play. svolder reaches that side too: the TUI shows and sets the default
sink's volume, follows the media player that is playing, draws a spectrum and a level of what the
sink plays, and puts a parametric correction (a headphone's AutoEq curve, say) in front of the sink,
live. This page is how that is built and what it promises to leave alone. The crate is
`svolder-host`, placed in [the map](../architecture.md#the-crates); the TUI and `svolder host eq`
use it, and so does `svolderd`, whose host service follows the player, reads its cover and reads the
route for every front end ([below](#the-daemons-host-service)). The parametric maths and the presets
a host EQ applies are in [tone-control-and-presets.md](tone-control-and-presets.md); the
commands and keys are in the [EQ reference](../usage/eq.md) and the [TUI
reference](../usage/tui.md).

## No PipeWire library

Nothing here links PipeWire. The sink runs `wpctl`, the route runs `pw-dump`, the capture runs
`pw-record`, the EQ runs `pipewire`, `pw-cli` and `systemctl`, and MPRIS goes over the session bus through the
D-Bus bindings the workspace already links for BlueZ. A distribution package therefore carries no
new native dependency, and a machine without any of them still runs svolder. Each part goes on
quietly without its source: a missing tool, a missing bus, a player that went away, a call that
times out reads as an absent answer (`None`), never an error to handle or a wait. `SVOLDER_HOST=off`
turns every part off for a process (`host_off`, `crates/svolder-host/src/lib.rs`), and the program
names (`SVOLDER_WPCTL`, `SVOLDER_PW_DUMP`, `SVOLDER_PW_RECORD`, `SVOLDER_PIPEWIRE`,
`SVOLDER_WIREPLUMBER`, `SVOLDER_PW_CLI`, `SVOLDER_SYSTEMCTL`) can be pointed at fakes. Tests that
could reach the user's session set the first or do the second.

## The sink

`Sink` (`crates/svolder-host/src/sink.rs`) is a trait; `Wpctl` implements it by running `wpctl`
against `@DEFAULT_AUDIO_SINK@`. The default sink is the one read and changed, whichever device it
is, and its name comes along so a screen can say which one that is. Each call runs `wpctl` once
under `TIMEOUT` (1 s). A read asks `get-volume` every time and `inspect` only when the sink may have
changed: on the first read, after a failed one, when the volume or the mute moved, and once the name
it holds is `INSPECT_EVERY` (5 s) old, which catches a new default sink that happens to read the
same volume. A write sets the volume capped to 100 % (never a boost) or toggles mute.

## The players

`Mpris` (`crates/svolder-host/src/players.rs`) lists the `org.mpris.MediaPlayer2.*` names on the
session bus and follows one: the one playing (the one that started last, if several), else the one
that played last, else the first by name. The choice is remade on a `NameOwnerChanged` (a player
joins or leaves) and on a `PropertiesChanged` naming a status change, never by polling. A player
that does not answer when asked is kept by name with what it said before and asked again at its next
signal, so one slow answer never loses it or its place. Nothing is ever sent to a player except one
of the four presses (`Button`: previous, play, pause, next) a caller asks for; reading is `GetAll`
and `Get` and the bus's own signals and calls, nothing else.

`NowPlaying` carries the followed player's title, artists, genres, album (`xesam:album`), cover
address as published (`mpris:artUrl`), track id (`mpris:trackid`), length in microseconds
(`mpris:length`), and its process id (the bus's `GetConnectionUnixProcessID` on the player's unique
name, asked again twice if it fails). `Progress` is the player's `Position` (microseconds) and
`Rate` (thousandths, 1000 when the player gives none or a rate that is not positive) with the Unix
millisecond stamp of the read, so a reader works the position out as position plus elapsed time
times the rate while the status is `Playing`. The position is read when the player is followed and
again on every signal that moves it: a status change, a new track, a rate change, and the `Seeked`
signal, and once more 500 ms after a new track. A player that does not answer `Position` has no
progress. Lengths and positions stay integers so `NowPlaying` keeps `Eq`.

## The covers

`cover` (`crates/svolder-host/src/cover.rs`) turns the `mpris:artUrl` a player published into a
picture, **from the disk only**: the module has no HTTP client and opens no socket, so an address
a player chose can cost at most a bounded read of a local file. It returns the bytes and a mime
type (`image/jpeg`, `image/png` or `image/webp`, told by the first bytes, never by a name) or
nothing; a miss is "no cover", never an error. The daemon calls it for each new address (again
1 s and 3 s later while that gives nothing) and keeps the picture it finds as its one current
cover ([below](#the-daemons-host-service)).

- **`file://` addresses.** The path is percent-decoded and is the one path the address names
  (`localhost` is accepted as the host, any other host is not). It is resolved with symbolic links
  followed *before* the root check, and must be a regular file under one of the roots in
  `CoverPlaces.roots`: `$HOME`, `/tmp` and `$XDG_RUNTIME_DIR`. A link out of the roots, a `..` out
  of them, a directory, a missing file and a file that is not a picture are all no cover. Once
  opened, the file is checked again for what it is (`/proc/self/fd`: a regular file, still under a
  root), so a path swapped for a link between the check and the open gives nothing; Spotify's cache
  entries get the same check against their cache directory.
- **Size.** At most `MAX_COVER_BYTES` (4 MiB), checked on the file's metadata and again on the read
  itself (at most one byte more than the cap is read, since a file can grow).
- **Spotify.** Only `https://i.scdn.co/image/<id>` is looked up, in the picture cache Spotify's
  embedded Chromium keeps on disk (`Browser/Cache/Cache_Data` under the snap, deb and Flatpak
  cache directories, in `CoverPlaces.spotify`). The entry is `<name>_0`, where `<name>` is the
  first 8 bytes of the SHA-1 of the key `1/0/_dk_https://spotify.com https://spotify.com <artUrl>`
  as a little-endian number in 16 lowercase hex digits (`spotify_cache_name`; SHA-1 only names a
  file, the `ring` legacy digest is enough). The file is a 24-byte header (the key's length at
  offset 12), the key, the picture, then Chromium's end marker and its other records. The key
  stored in the file must equal the expected key byte for byte; the picture is the bytes up to the
  first end marker and is taken only if it is a JPEG, PNG or WebP of at most 4 MiB. Any other
  `http(s)` address is no cover. The layout was checked against the snap install; the deb and
  Flatpak ones are expected to match.

## The route

`route` (`crates/svolder-host/src/route.rs`) answers where the followed player's sound goes: which
sink is the default, which stream is the player's, what else plays into the sink and whether the
computer EQ's filter sits in front of it. `PwDump` runs `pw-dump` once with a 2 s timeout (a missing
tool, a failure or a timeout is `None`, like `Wpctl`; `SVOLDER_PW_DUMP` names another program), and
`route(dump, player_pid, player_identity)` is a pure function of its text, returning `RouteFacts`
(`None` only when the text is not a JSON array). It reads the array as plain JSON, with no PipeWire
library: nodes, ports, links, clients, devices and the `default` metadata.

- **The sink** is the node named by `default.audio.sink` in the `default` metadata. When that is
  the filter's own sink (`svolder_eq`, the `NODE_NAME` the EQ's config uses), the sink described is
  the real one the filter's `svolder_eq.output` stream feeds, so the facts are always about the
  device the sound ends in. `SinkFacts` carries its name and description, the rate and format of
  its negotiated `Format` (a suspended sink has none, and then no rate, whatever its `node.rate`
  says), mute (hard or soft) and the computer volume.
- **The volume** is `volume_milli`, 1000 = 100 %, on the scale `wpctl` prints: the cube root of the
  node's master `volume` times its lowest channel's linear `channelVolumes` (0.013822 linear is the
  0.24 `wpctl status` shows). Behind the computer EQ it is the sink's times the filter node's,
  because the default-route EQ makes the filter node the one the system volume moves. A stream
  has its own, read the same way, with its mute: `StreamFacts` is the rate and format the stream
  negotiated, before the sink converts it, and the volume and mute the player set on its own
  stream, which a path that claims to be untouched has to account for.
- **The player's stream** is looked for among all the `Stream/Output/Audio` nodes, running or
  idle, first by process id (the stream's client's `application.process.id` against the pid of the
  player's D-Bus connection, `PlayerMatch::Pid`), else by name (the stream's or its client's
  `application.name`, binary, desktop entry or node name against the MPRIS identity, equal or
  containing in either direction and in any case, `Name`). Of the streams found, one that plays
  into the sink (or into the filter that feeds it) is taken, a running one first, then the lowest
  id. If they all play into another sink the match is `Elsewhere { sink }`, with that sink's
  description (else its node name) and no stream facts, so the line can say where the player plays
  and give no verdict (a stream into the EQ's filter names the sink behind it; a linked stream
  beats an unlinked one). If they are linked to no sink it is `NotFound`. Only when the player has
  no stream of its own anywhere, and exactly one stream is running into the sink, that one is taken
  (`Assumed`); otherwise `NotFound`. No followed player (no pid, no identity) is `NoPlayer`.
- **Others mixing** counts the running streams (a corked or idle one is not) that reach the sink
  and are not the player's chosen stream, so a second running stream of the player's own counts;
  with no chosen stream (`NotFound`, `NoPlayer`, `Elsewhere`) it counts all of them.
- **The host EQ** is `host_eq` when the filter's output feeds the sink and the player's stream goes
  through the filter (never for `Elsewhere`).
- **A USB sink** gives `usb_vendor_id` and `usb_product` from its device object
  (`device.vendor.id`, `device.product.name`, on `device.bus = "usb"`); any other bus gives none.

The tests run against `crates/svolder-host/tests/fixtures/pw-dump/`: real `pw-dump` output from
the development machine (anonymised, trimmed to the objects a route reads) and files derived from
it for what that machine cannot show; the folder's README says which is which.

## The capture

`PwRecord` (`crates/svolder-host/src/capture.rs`) records the default sink's monitor and hands it on
as `Block`s: sixteen bands and two levels. `pw_record_args` is the whole of what `pw-record` is run
with, the stream properties `stream.capture.sink = true`, `node.passive = true` and `node.rate = 0`
and `--target` naming the sink, so PipeWire links it to that sink's monitor without keeping it awake
or pinning its rate, and without a sink name nothing starts. It can never open a microphone, and a
test pins the exact argument list. Samples stay in memory only while a block of 1024 frames at 48
kHz becomes a `Block`; nothing is written, kept or sent. `fft.rs` is a hand-rolled radix-2 transform
and the `BANDS` (16) log-spaced bands from `LOW_HZ` (40) to `HIGH_HZ` (16 000), with the RMS level,
both on one dB scale that puts `FLOOR_DB` (-60) at 0.0 and full scale at 1.0. The TUI starts the
capture only while host audio shows and either the Spectrum row is drawn or something wants a level
that no meter of the device supplies (`App::capture_target`, `bins/svolder/src/tui/app/meters.rs`),
and only for a sink that named its own node.

`PwFrames` records the same way and hands on `Frame`s instead
(`crates/svolder-host/src/analysis.rs`), through the `FrameCapture` trait that mirrors `Capture`;
both share one recorder, so the process, its thread and the ways it ends are the same. An `Analyser`
turns the interleaved samples into 30 frames a second, however they are cut up: each frame is the
latest 8192 samples of each channel through a Hann window and `fft::fft`. Per channel there are 240
bands at 1/24 octave from 20 Hz (the last ends at 20 480 Hz), one byte each: the band's amplitude in
dB, 0 at -72 dBFS and 255 at 0 dBFS. The range is a constant (`BYTE_FLOOR_DB`) that suits how music
looks, not a promise to a front end. A band is the power of the bins that centre inside it, so a
sine reads its amplitude in the band that holds it; below about 200 Hz a band is narrower than a
bin, and reads the power at its centre, interpolated between the two bins around it. The frame also
carries the RMS and peak of each channel over its own 33 ms, as bytes on the same scale.

The beat comes from the 20-150 Hz range alone and is found in the time domain, since a transform of
8192 samples smears an onset over about 170 ms and would put every beat late: each channel through a
20 Hz high-pass and a four-pole 150 Hz low-pass (the channels' energies add, so bass that is out of
phase between them does not cancel), the RMS of what comes through per frame, and the rise of that
RMS from one frame to the next as the onset curve. A frame is a beat when its rise is over a small
floor and stands 1.2 deviations above the last second's mean, no sooner than 8 frames (0.27 s) after
the last beat; the first three frames are not counted, as the filters start from rest. The tempo is
the strongest lag from 60 to 180 BPM in the autocorrelation of the last 8 s of the curve (after 3 s,
and only when the peak is a clear share of the curve's energy), refined by a parabola through the
peak and held while it moves by less than 2 BPM; the double tempo wins when its peak is within 80 %
of the strongest, so onsets on the half beat read at double; the phase is the time since the last
beat over the beat period, 0 to 255, and 0 until a first beat has been seen. When the range stays
silent for a second the curve, the tempo and the phase are cleared, so a front end that schedules
beats from them stops pulsing over silence and finds a new tempo from what plays next. The reading
thread stamps each frame with `unix_ms` as it makes it.

`Frame::block()` turns a frame back into the TUI's `Block`: each of the sixteen bands takes the
loudest of the 240 whose centre falls inside it, each the mean power of the two channels (a mono
signal reads as it would mixed, a hard-panned one 3 dB lower), and the levels are the RMS bytes on
the -60 dB drawing scale, so a joined TUI lands a daemon's frames through the paths that draw its
own captures.

## The daemon's host service

`HostService` (`bins/svolderd/src/host.rs`) is the one place the daemon touches the host, so a web
page, a joined TUI and a script all read the same thing. While `[host] enabled` is true (and
`SVOLDER_HOST` is not `off`), one task follows the players through `Players` and publishes a
`HostView` (`crates/svolder-ipc/src/host.rs`) on a watch channel: `available`, the followed
player's `PlayingView` (title, artist, album, player, status, length, progress, cover hash) and the
route. The view is sent on change only. A bus that cannot be reached, or one that goes away, is
`available = false`; one that went away or has not answered is looked for every 10 s.

- **The cover** is read once per new `art_url`, on the blocking pool so a slow disk never holds up
  the task that follows the player, and kept as the one current cover, named by the first 16
  lowercase hex characters of its SHA-256. `PlayingView.cover` holds that hash only when a picture
  was read, and `HostService::cover(hash)` serves the current one and no other, so a stale hash is
  a miss. A new address clears the old cover at once. An address that gave no picture is read
  again 1 s and then 3 s after the first read (a player may name a picture a moment before it has
  written it), then left. A player that quits clears the cover with the track.
- **The route** is read out of `pw-dump` only while a `Watcher` is held: by each `Subscribe`
  connection that asked for the host (`host: true`) for its life and by the event stream of each
  web page of a DAC (an amp's page shows no path, so its stream holds none); a page hidden for 2 s
  lets its stream go, so a background tab holds no route reads. The first watcher gets a read at
  once, then one every 2 s, never two closer: a player change does not force a read inside the 2 s
  (a player that quits is dropped at once), and a sink switch shows on the next read. With nobody
  watching there is no `pw-dump` run and no route kept; the in-process readers that never show the
  host (the MQTT bridge, the MCP server) read `Daemon::device_events`, which holds no watcher. The
  web model names the applied preset from `host-eq.toml`, read only when the filter is in the path.
- **The presses** go to the followed player through `Players::press`; `Request::HostPress` names the
  button as a word, and answers `Ack` when the player took it, an error for an unknown word, a host
  that is off, no player, or a player that refused.
- **`SVOLDER_HOST=fake`** runs a fixed fake player (it answers presses: play and pause turn its
  status over), a fake route on a Schiit sink named for the fake device with the player matched at
  the rate the fake device reads (48 kHz, `svolder_fake::SAMPLE_RATE_HZ`), and the bundled picture
  as the cover, through the same cover path. Knobs: `SVOLDER_FAKE_COVER_GONE_AFTER_MS` and
  `SVOLDER_FAKE_PLAYER_GONE_AFTER_MS` drop its art or its player after a delay, `..._GONE_FILE`
  (what the checks use) when the named file appears; `SVOLDER_FAKE_ROUTE=changed` makes the route
  alter the signal in every way (a quieter player and sink, a resample, the host EQ, two other sounds), for a long changed
  path line; `SVOLDER_FAKE_TITLE` names the track, for a title too long for a phone's folded line.
- **The capture** is held, not followed: one capture runs in the daemon while a page's frames
  stream or a joined TUI's frames connection lives, and nothing runs without one
  ([host-capture.md](host-capture.md)).
- **A reload** calls `HostService::reconfigure`: the same configuration leaves a running host alone,
  any other starts the new one behind the same channels, so a stream that is open hears the host go
  off or come back.

**Why the daemon, and what stays out.** The daemon is the per-user service on the computer that
plays, so it is the one to know the player and the sink: a phone's page and a TUI joined from
another terminal see the same computer, and a second watcher is never needed for them. That is why
`[host] enabled` is on by default; a daemon that runs somewhere with no session bus or none of the
tools (a container, a service started outside the session) reports `available = false`, says so
plainly, and every front end goes on as it did without it. The computer's volume and the computer EQ
are the daemon's too, for the web page, the TUI, MCP and MQTT ([host-control.md](host-control.md)).
The systemd unit carries no sandbox that would hide the home directory, since the
covers and Spotify's cache live there; a unit that is ever hardened has to keep the cover roots and
the cache directories readable.

**What it leaves out, and what comes later.** No audio travels over the network, nothing seeks,
lists or looks up lyrics, no cover is looked up anywhere, and Home Assistant gets the computer EQ's
switch and preset only. The TUI's direct mode still captures for itself.

## A joined TUI

Who speaks for the player depends on the session. `Host.daemon` (`bins/svolder/src/tui/host.rs`) is
set while a joined daemon's `HostView` says `available`: its player replaces the one the loop's own
watcher follows, its route feeds the Path row (`App::host_path`: a DAC only, present while the
daemon speaks so the Host group keeps its height: `nothing playing` with no player, `route not read`
before the first read, else `path_line` judged against the device's name, input and sample-rate
readings: a DAC on an input the computer does not feed reads `the DAC listens to Optical` with no
verdict, and one whose input is not read is never `untouched`; one that reads each USB port is on it
only when its own port alone reads connected; a DAC whose profile has no `input` control, the Eitr 2
and the Mesh Card, takes USB alone and counts as on `USB`, built once in `DacFacts::read` for both
front ends), and `HostLink` (`run/host.rs`) leaves the players alone: not looked for when the daemon
spoke first, held and ignored when it speaks later, taken back (`HostLink::follow`) when it stops:
when it says the host is unavailable, and when its feed ends (`Watching::wait_for_daemon`, `gone`),
until a reopen on a daemon that is back brings an available view.

The view comes from `Backend::host_view` (a `Request::Host`, asked when a device opens) and then the
update feed, which `Backend::open_host_feed` opens with `Subscribe { host: true }`; only a TUI that
shows the host asks, since a subscription that asks keeps the daemon reading the computer's audio
graph. `match_host_feed` (`run/events.rs`) keeps the feed asking for it exactly while
`App::asks_host` holds, so a Settings switch of `host_audio` re-subscribes at once (off without the
host; on with it, and a fresh `Request::Host`, asked off the loop); the opener's choice for later
opens is the session's (`Kept`), not the table the TUI started with. A press goes to the daemon as
`Request::HostPress`, and what it refuses is said in a toast. A daemon that does not know the
request, or says it has no host, leaves everything as in a direct session. The sink, the host EQ
and the capture are the TUI's own in both. The Path row puts the verdict first and then the hops,
cut at the right edge, as the web line does, so a short row keeps the answer, and it stays in the
Host group while the daemon speaks for the host, so the group keeps its height as a track starts and
stops; the TUI draws no elapsed time or progress bar, which is the web card's. `[tui]
host_audio = false` wins: nothing is asked for, and a view that arrives is dropped.

## Fakes

`crates/svolder-host/src/fake.rs` holds a stand-in for each source that answers from what a test
gave it and records what it was asked; a clone shares its original's state, so a test keeps one and
hands the other to the code under test. This is what lets the TUI's buffer tests cover every state,
including each part missing. `FakePlayers::answering` makes the players answer a press as a real
player would, which the daemon's fake host and its tests use.

## The computer EQ

The computer EQ (the manager in `crates/svolder-host/src/eq/manage.rs`) puts a correction in front
of the sink as a PipeWire filter chain; its live controls and front ends are in
[host-control.md](host-control.md). `apply` writes the chain's config and starts it; `remove` stops
it and deletes what `apply` wrote; `status` reads the state file and checks that the chain runs.

**The config.** `render_live_conf` (`crates/svolder-host/src/eq/live.rs`) renders a complete
standalone PipeWire config for `pipewire -c <file>`: a preamp and sixteen biquad slots per channel
(see host-control.md), with the FL and FR ports named. It differs by `Route`, chosen from the
WirePlumber version (`wireplumber_version`, `Route::for_wireplumber`):

- **Smart** (WirePlumber 0.5 and newer): the chain is a smart filter targeting the sink. WirePlumber
  links every stream to the filter and the filter to the sink, and again when the sink goes away and
  returns. `node.dont-fallback` and `node.linger` ship together: with `node.dont-fallback` alone the
  filter's nodes are destroyed when the sink is unplugged and never come back while the process
  lives.
- **Default** (older WirePlumber): the chain is a sink of its own that plays into the target through
  `target.object` and is made the default output; the previous default is remembered and restored by
  `remove`. When the target is unplugged the filter stays the default and plays on, filtered, into
  the next sink.

**Keeping it running.** Something has to keep the chain alive, and `Manager` says what: a systemd
user unit (`render_unit`, modelled on PipeWire's own filter-chain service so it follows the PipeWire
service and restarts on failure) when the user manager answers, else a detached `pipewire` process
with an autostart entry (`render_desktop`) and a pid file in the private runtime directory.
`SVOLDER_HOST_EQ_MANAGER` chooses.

**What it writes.** Apart from PipeWire's and WirePlumber's own runtime state, svolder writes four
kinds of file and nothing else: `svolder/host-eq.conf` in the configuration directory, the unit
`svolder-host-eq.service` or the autostart entry `svolder-host-eq.desktop`, the state file
`svolder/host-eq.toml` (`HostEqState`) and, for a process, its pid file. `remove` deletes what
`apply` wrote. PipeWire's own configuration directories, its `filter-chain.service` and every other
unit are left alone.

**What it applies.** `host_correction` turns a preset into a `Correction`: an imported preset's
filters and stated preamp; a band preset's bands as peaking filters with a preamp of minus the
curve's peak rounded up to 0.1 dB. A band preset the DAC keeps flat or in loudness mode has no curve
to play and is refused with the reason. `safe_preamp` is applied to every correction whatever it
states: the lower of the stated preamp and minus the peak, so the computer EQ never goes above
unity, and `lowered_note` says so when it lowered it.

**Apply is transactional.** `Host::apply` checks the correction, the paths and the sink, tries every
directory it will delete from or write to, and writes the config and the unit or entry beside their
places (`Staged`) before it stops anything, so a request that cannot be served, or whose files
cannot be written, leaves a computer EQ that is on exactly as it was.

## Principles

**Absent is an answer.** Every source returns "nothing" instead of failing: no tool, no bus, no
player, a timeout. A feature that threw would make a laptop without PipeWire unusable for everything
else svolder does.

**Explicit control, never on its own.** The host's volume, mute, EQ and a player's transport change
only on a key, a click, a drag or a wheel the user makes in a front end, or an MCP or MQTT command; nothing is sent on start, on focus or on a timer.
A program that moved the system volume by itself would surprise the person whose speakers those are.

**One daemon speaks for the computer.** A front end that has a daemon asks it; a second watcher
beside it would read the bus and the graph twice and could disagree. The TUI keeps its own only
when the daemon has none, and it takes the daemon's back the moment one speaks.

**A cover never costs a network request.** The picture a player names is read from a local file or
Spotify's own cache, or there is none. A hostile or odd address can cost a bounded read of one file
under the user's directories, and nothing else: the module cannot open a socket.

**Never say clean when it is not.** The path says `untouched` only when every hop is known to leave
the signal alone, and names the hop that does not; what could not be read is said as unknown, and
the words never claim the bits are exact, since svolder cannot see the bits. A path that cannot be
judged (another sink, another input) gives no verdict at all.

**Watching changes nothing, and costs nothing when nobody looks.** The route is only read, and only
while a page or a TUI that shows it is connected, at most once in two seconds; the player is only
asked, and sent a press the user made.

**Capture only an output, keep nothing.** The argument list is fixed and tested, no microphone is
reachable, and samples never leave memory. This is a privacy promise a user can read in one
function.

**One transport rule for the volume.** The host volume is capped at 100 %, never a boost, on the web
page, in the TUI, MCP and the CLI alike, for the same reason the DAC's volume has a rule.

**Leave the user's audio configuration alone.** The computer EQ writes four named files and removes
exactly them. It never edits PipeWire's or WirePlumber's own configuration, so removing svolder
leaves the machine as it was.

**Check everything before changing anything.** `apply` stages its files and validates before
stopping a chain that is on. A half-applied EQ would leave the user with no sound or an unfiltered
one at the moment they asked for a change.

**Never above unity.** The preamp is lowered to cover the correction's peak. A boost with no
headroom clips in the computer's own mix, before the DAC ever hears it.

**Test against fakes, never the session.** Real `wpctl set-volume` or an MPRIS `Play` against the
developer's own session is never run; the traits and the environment variables are the seam.

## Where the code and tests live

- The crate: `crates/svolder-host/src/lib.rs`, `sink.rs`, `players.rs`, `route.rs`, `capture.rs`,
  `cover.rs`, `fft.rs`, `eq.rs`, `fake.rs`. Tests are in `crates/svolder-host/src/eq/tests.rs`,
  `crates/svolder-host/src/players/tests.rs`, `crates/svolder-host/src/cover/tests.rs`,
  `crates/svolder-host/src/route/tests.rs` and `crates/svolder-host/tests/` (`sink.rs`,
  `players.rs`, `capture.rs`, `route.rs`), which run against fake `wpctl`, `pw-dump` and
  `pw-record` scripts and a private session bus.
- The computer EQ: the manager in `crates/svolder-host/src/eq/` (`manage.rs`, `process.rs`, tests
  in `manage/tests.rs`), the verbs in `bins/svolder/src/commands/host_eq.rs`.
- The daemon's host service: `bins/svolderd/src/host.rs`; tests in
  `bins/svolderd/src/host/tests.rs`, `bins/svolderd/src/handler/host_tests.rs` and
  `bins/svolderd/tests/host.rs`.
- The TUI's use: `bins/svolder/src/tui/host.rs`, and the host EQ on the tone screen in
  `bins/svolder/src/tui/boxes/presets/computer.rs`.
