<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer EQ's end-to-end check

`make host-e2e` runs the computer EQ end to end, through the real daemon, on a private PipeWire
server with its own WirePlumber and a null sink (no sound card), started by
`scripts/host-e2e/private-pipewire.sh`; the person's session is never touched. It is part of
[the gates](testing.md), opt-in and not part of `make check`. Run it after a change to the computer
EQ, its chain or the daemon's computer side.

**Run it alone.** The run ends by comparing the processes that exist then with those that existed
before it started, and fails with "the process lists differ" when they differ. A `make test` (or any
other run that starts and stops processes) beside it makes them differ, so no other `make test` may
run while it does; the web checks hold the gate lock for the same reason.

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
- the volume and mute with `wpctl`, and the daemon ending with its chain.

It prints `host-e2e: PASS`, and fails on a process it started that is still there.

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
