<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer EQ's end-to-end check

`make host-e2e` runs the computer EQ end to end, through the real daemon, on a private PipeWire
server with its own WirePlumber and a null sink (no sound card), started by
`scripts/host-e2e/private-pipewire.sh`; the person's session is never touched. It needs PipeWire, WirePlumber and their tools, `paplay` and
`pactl` (the `pulseaudio-utils` package) and python3 with numpy; `run.sh` says so and exits 2
without them. It is part of
[the gates](testing.md), opt-in and not part of `make check`. Run it after a change to the computer
EQ, its chain or the daemon's computer side.

**Run it alone.** The run ends by comparing the processes that exist then with those that existed
before it started, and fails with "the process lists differ" when they differ. Only the user's session
services count (`pipewire`, `pipewire-pulse`, `wireplumber` whose cgroup is under
the user manager's `session.slice`), so the fake `pipewire` scripts a unit test starts in its own scratch
home are not flagged. Other runs that start and stop real session processes must still not run beside it,
and the web checks hold the gate lock for the same reason.

## What it does

A 1 kHz tone at -20 dBFS plays into the null sink and a recorder linked by hand to the sink's own
monitor hears it after the filter chain. `scripts/host-e2e/run.sh` runs `svolderd` (the repo-built
one, a child chain, no `systemctl`, a fake device, the sets' and AutoEq's addresses on a closed
port) and `scripts/host-e2e/analyse.py` drives it over the socket:

- the EQ on (with the Off to EQ gap), and a drag of one point to +6 dB at 20 updates a second, with
  the peak off the tone so a bypass cannot pass for a boost (no gap, no click, the preamp at or
  under unity);
- a move, a type change, and a forced sample-rate change followed by the controller sending every
  slot again, Off, and the crossfeed (X3e kills the chain three times: restarted, the second death
  ridden out, the third leaving the EQ off);
- the virtual speakers, phases V0 to V11 below;
- phase U, the VU levels: the daemon runs with its web page on a free loopback port and a run token,
  and the levels-only frames (`?only=levels`, events `hello`, `capture` and `frame` with just `t`
  and `lv`) are read with the sink at 100 % (an earlier phase leaves it at 50 %): a sine at -18 dBFS
  RMS reads -18 dB (within 0.3 dB) in the RMS byte, a 0 dBFS square gives peak byte 255 in every
  frame, and a sine and a square at -0.1 dBFS stay at 254 or less. The pieces are
  `scripts/host-e2e/vu.py` and its self-check, `python3 -I scripts/host-e2e/vu_selftest.py` (no
  make target);
- the computer loudness, phases L1 to L6 below (after the V phases);
- the volume and mute with `wpctl`, and the daemon ending with its chain.

It runs phase B (below, with phase C's check of svolder's words on the same rows) first, then the
phases above, and prints `host-e2e: PASS` as its last line, so a PASS
means every phase passed; it fails on a process it started that is still there.

## The loudness phases

They run after the V phases, before phase U. Three tones play at once (50 Hz, 1 kHz and 12.5 kHz, each at -26 dBFS,
whole cycles in 20 ms) and a recorder on the sink's monitor measures each. **The monitor is after the sink's volume**
(at 35 % every tone reads 27.36 dB lower, 60 log10 0.35), so a tone reads the stage's response plus the volume's own
cut; "dry" below is that cut alone. The baseline is what the flat EQ chain plays with loudness off at full volume;
every number is re that baseline. The oracle is the stage's response (`loudness::response`) worked out again in Python
from the pinned constants (289 Hz, 0.53 dB per dB, 7760 Hz, 0.15 dB per dB, cap 6 dB; no `svolder` helper prints it)
and the depth `strength x clamp(reference - volume, 0, 40)` of the cubic volume `wpctl` reports (35 % at strength 50
gives 13.7 dB; at 48 kHz the 1 kHz tone is -6.96 dB, the view's `mids_db` of -7.3 is the shelf gain, not a response).
Each shape check waits for the volume's glide to settle; the glide can leave a brief extra cut of up to about 0.15 dB
near its end, which is why the tolerance against the oracle is 0.3 dB.

- **L1**: loudness on with the volume at the reference (100 %): the view reads flat and every tone is the EQ-only
  baseline within 0.1 dB.
- **L2**: the volume reaches 35 % (checked), reference 100, strength 50 (depth 13.7 dB): each tone within 0.3 dB of the
  oracle, and the view's `volume_pct` within 1 of `wpctl`.
- **L3**: back to 100 %. The sink's own volume change is an instant 27 dB step on the monitor; it is found in the
  recording and only the windows across it are skipped. From the command on: 20-60 ms after the edge every tone is still
  within 0.5 dB of the L2 shape (a stage that jumps to flat in one line shows here); no step over 1 dB between
  neighbouring milliseconds of the 1 kHz envelope; no tone above its dry level (0 dB after the edge) by more than 0.15 dB on
  any 20 ms window; a rise with no fall back over 0.3 dB (on the median of three readings); every tone flat within 0.1 dB 3 s after
  the command, and the 1 kHz tone within 0.15 dB of flat from 3 s at the latest.
- **L4**: 48 to 96 kHz with loudness on: one reading from 3 s after the sink reports the new rate is the oracle's at
  96 kHz within 0.3 dB; back at 48 kHz the 48 kHz shape returns.
- **L5**: the EQ off (a chain for loudness alone stays and cuts), then a graceful stop of the daemon: the tones are at
  the dry level afterwards (0.1 dB) and no tone is above it by more than 0.15 dB on any 20 ms window; loudness is still
  on in the restarted daemon. The daemon runs here as a child manager, so the chain ends with the daemon; the service
  case (the chain kept, its stage sent flat) is covered by the controller's unit test, not by this phase.
- **L6**: loudness on again, then the daemon killed (SIGKILL) with the chain cutting: no tone is above the dry level by
  more than 0.15 dB on any 20 ms window, before, during or after the chain's end.

Under `HOST_E2E_BYPASS=1` (the recorder hears the tone before the chain) L2 to L6 fail; L1 is a null check against the
baseline and cannot. `HOST_E2E_ONLY=L make host-e2e` runs the loudness phases alone while they are worked on: it exits 3
and never prints `host-e2e: PASS`.

## The virtual speakers' phases

- **V0**: a set chosen while the speakers are off is only kept; the chain is not rebuilt and loads
  no WAV.
- **V1**: on rebuilds the chain once with the stage in it, and the config on disk holds the stage
  off while it plays; off keeps the convolvers loaded and idle, and the chain is a bit-exact bypass
  (the null test).
- **V2, V3, V4**: a tone on the left speaker alone, on the right alone, and mono, against the
  engine's response of the stored WAV (level and phase at both ears); the mono one never passes
  0 dBFS at any frequency, since a set is scaled so its worst case is -0.1 dB.
- **V5**: a rate change; the convolvers resample again and the speakers play at the new rate.
- **V6**: the set's WAV deleted under the chain; the next rebuild leaves the stage out with a note,
  the chain lives and the dry sound is exact.
- **V7**: exclusivity with crossfeed, each turning the other off in one update.
- **V8**: an output change (the default sink moves to a second null sink); the speakers turn
  themselves off with a note.
- **V9**: the speakers on with a chain for them alone (EQ off), then a graceful stop and a kill of
  the daemon; each restart finds them off, the choice kept, no chain, and nothing in the config or
  the graph playing a WAV.
- **V10**: the chain process's CPU while a tone plays through the speakers, recorded and not a gate.
- **V11**: off on a chain that runs for the speakers alone, with a tone playing; the chain stays as
  a pass-through.

The example `vs_expect` writes a synthetic 4-channel room (a Dirac per path with its gain and lag,
plus a decaying noise tail) and prints the engine's four complex gains of a WAV; the controller's
tests and this check use it as the oracle (`eq::speakers_response`).

## Phase B: bit-exact samples through a sink converter

Phase B (`scripts/host-e2e/bits.py`, config `bits.conf`) asks one question of PipeWire alone (phase C, below, then
runs the CLI on the same rows): do known 16- and 24-bit samples come out of a sink's converter exactly as they went
in? `make host-e2e` runs it first, before the EQ phases, so that `host-e2e: PASS` stays the last line of a full run;
`make host-e2e-bits` (`run.sh bits`) runs it alone in about 45 s on a quiet machine (about a
minute with the builds) and prints `host-e2e-bits: PASS`.
`HOST_E2E_ONLY=bits` is refused (exit 2): no environment variable makes a partial run pass for the whole check. Like
`host-e2e` it runs under the exclusive gate lock and needs a quiet machine.

**Rig.** One `pipewire -c` client of the private instance provides seven `pipe-tunnel` sinks (stereo; each is a sink
whose converter writes what a device would receive into a FIFO): S16LE, S24LE and S32LE at 48 kHz and S24LE at
44.1 kHz with nothing in the converter, and three S16LE sinks that each carry one fault (`dither.method =
triangular`; `dither.noise = 4` with method none; an `audioconvert.filter-graph.0` builtin mixer at gain 0.5). The
private `settings` metadata holds `clock.allowed-rates [ 44100 48000 ]` for the run, so the graph follows the
stream's rate between the two (it is put back to `[ 48000 ]`, with `clock.force-rate` 0, and both read back at the
end). A pattern of seeded random stereo samples (the extremes first, no zero sample, 2 s) is played with
`pw-cat --raw` at stream volume 1.0; the sink's volume is read back as 1.0 and unmuted, and its negotiated `Format`
as the format and rate asked for. A thread reads each FIFO (no `cat`), the first non-zero frame is the pattern's
first (the sink writes silence before it), and every sample from there is compared in a common 32-bit domain, so
any two widths compare. `pw-cat --format s24` is not used: it does not carry packed 3-byte samples, so 24-bit
content is sent in the top bits of S32, as players send it.

**The rows**, each a check with its expected outcome:

| Stream | Sink | Result |
|---|---|---|
| S16 | S16LE, S24LE (<<8), S32LE (<<16) | exact |
| 24-bit in S32 | S24LE, S32LE (<<8) | exact |
| S16 mono | S16LE stereo | exact, both channels are copies |
| S16 / 24-bit, stream `dither.noise = 4` (read back from the player's Props while it plays) | S16LE / S24LE | exact (a playback stream's converter does not quantise) |
| 24-bit at 44.1 kHz | the 44.1 kHz S24LE sink, no force | exact |
| 24-bit at 48 kHz, `clock.force-rate 48000` (read back) | S24LE | exact (the stream's own rate) |
| 24-bit in S32 | S16LE | NOT exact: every sample cut below bit 16 |
| S16 | S16LE with triangular dither | NOT exact: one 16-bit step on 30-70 % of the samples, never more |
| S16 | S16LE with `dither.noise = 4` | NOT exact: off by at most 8 steps of 16 bits |
| S16 | S16LE with a filter graph | NOT exact: every sample halved |
| 24-bit at 44.1 kHz, `clock.force-rate 48000` (read back) | the 44.1 kHz S24LE sink | NOT exact: resampled, yet a copy of the source (correlation over 0.5 at the best lag); the same stream is exact without the force, the row above |
| 24-bit at 44.1 kHz | the 48 kHz S24LE sink, no force | NOT exact: nearly every sample differs and the sound lasts 48/44.1 as long |

A negative control passes only with a full-length sound AND its own difference signature (the right-hand
column); silence, a short capture or another kind of difference is a setup accident and fails the run. The two
resampling rows are the loosest: nearly every sample must differ, and the forced one must still correlate with the
source (over 0.5 at the best lag; a resample falls between two source frames, so the best whole-frame lag sees a
floor of about 0.64 of white noise, measured 0.70 live, against about 0 for unrelated noise) while the sink-rate one
must last 48/44.1 as long; the forced row is also held up
by its partner (the same stream and sink exact without the force) and by the read-back of the metadata.
The pipe is drained before each player starts and the tail is waited for from the capture's first sound, so an
earlier case's silence never counts toward it. `python3 -I bits.py --selftest` checks the comparison and each
signature on made-up data (a flipped bit, a shifted frame, silence, a cut pattern and each control's accident must
not pass). `HOST_E2E_BITS_FLIP=1` flips the lowest bit of one written sample of every capture before it is
decoded, one thousand frames after the capture's first sound: every exact row must then fail (the run fails by design; a proof that one wrong bit is seen at each
sink's own width).

`HOST_E2E_PWDUMP_DIR=<dir>` also writes `setup-pipe.json`, one `pw-dump` of the S16LE+dither sink playing a
`Fixture Player` S16 stream with `dither.noise = 2`, next to the filter-graph sink, with allowed-rates
`[ 44100 48000 96000 ]` and force-rate 96000 (user, host and paths replaced): the real capture the route's
setup reader is tested against.

**What it proves**: PipeWire's stream converter (integer to F32), the mixer with one input, the volume at 1.0 and the
sink converter (F32 to integer, dither none) are transparent for 16- and 24-bit samples, and each fault in the table
breaks that. **What it does not prove**: what ALSA does past the converter (`hw:` hands the bytes on, `plug:` converts),
the USB driver and the DAC; that an ALSA sink's adapter is the same converter object as the pipe sink's (both load
`audioconvert/libspa-audioconvert`: the same code, not the same object); anything above 24 bits (PipeWire mixes in
32-bit float); and what a player does to its samples before its stream.

## Phase C: svolder's words against the bits

Phase C runs inside the same `bits.py`, on the same rows: it asks whether `svolder host check` says what phase B
measured. For each row the row's sink is made the default in the private `default` metadata (`default.audio.sink` and
`default.configured.audio.sink`, read back), and while the player plays, `target/debug/svolder host check --json
--player "Fixture Player"` reads the private graph: temp XDG homes inside the run directory, `SVOLDER_TRANSPORT=fake`
(no daemon), `SVOLDER_PW_DUMP` unset, no systemctl, never an installed binary. Only `pw-dump` runs on svolder's side;
the metadata writes are the script's, on the private instance.

- **Soundness and completeness of the verdict.** An exact row must read `untouched`; a NOT-exact row must read `changed`
  and name its cause (`cut to 16 bits`, `dither on (triangular)`, `dither noise 4`, `a filter inside the sink`,
  `(rate forced)` for the forced rate, `resampled 44.1 → 48 kHz` for the sink's rate). A NOT-exact row that reads `untouched`
  is the failure that matters: the check would have called a changed sound untouched.
- **The seven config lines.** Each fault fails its own line and the others pass: `forced rate` with a force,
  `dither` on the two dither sinks, `no filter in the sink` on the filter-graph sink. The `24-bit output` line fails on
  the S16LE sinks, exact or not: it judges the sink's format, not this stream. The rows' words and the check's wall time
  are printed, one line each.
- **The null sink.** With the null sink as default and idle, the `rates` line follows `clock.allowed-rates` (every
  standard rate allowed passes; `[ 44100 48000 ]` fails naming the rates that would be resampled) and the `forced rate`
  line follows `clock.force-rate` (96000 fails, 0 passes), each read back from the `settings` metadata and put back after.

`make host-e2e-bits` builds `target/debug/svolder` first; the script refuses (exit 2) without it. What phase C does not
prove: the ALSA lines (`no plug`, the card's `hw_params`): the private instance has no card.
