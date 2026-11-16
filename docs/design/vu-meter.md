<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The VU meter

Two meters draw the level of what the computer plays, one needle for each channel: Signal's two
dials on the web ([Signal](web-signal.md#the-computers-level-meters)) and the `VU L` and `VU R`
rows of the terminal UI. They share one set of rules, written twice (Rust in
`crates/svolder-host/src/vu.rs`, JavaScript in `bins/svolderd/web/core/vu-model.js`) and held
together by a golden file. The levels come from the daemon's capture ([the capture](host-capture.md))
or, in a TUI with no daemon, from its own.

## What is measured

The capture records the default sink's monitor. A level is therefore the sound as that monitor
carries it: after the computer EQ's chain when one runs (a preamp or a boost raises it), and after
the sink's volume and mute only where the sink applies them to its monitor (PipeWire's
`monitor.channel-volumes`). Where the sink does not, the meters show the mix whatever the volume
says and stay lit on mute; the meters follow the volume only where the sink's monitor does. When
the graph runs at 44.1 kHz the tap resamples to 48 kHz, which moves a sine's peak by under 0.04 dB
and can lift a square or clipped master by up to 1.9 dB.

The needle is driven by RMS, not by the average-rectified level of a classic meter. The reading is
in VU, a ratio to the reference.

## The scale and the reference

The scale runs from -20 VU to +3 VU and is linear in amplitude between them: the deflection is
`(ratio - 0.1) / (10^(3/20) - 0.1)`, 0 at the left stop, 1 at +3 VU and about 0.69 at 0 VU, held
at 1.03 (the pin) beyond. `ratio` is the channel's RMS divided by the amplitude of the **reference**,
the level in dBFS that reads 0 VU: a whole number from -30 to -6, -18 by default. A value outside
the range is held at the nearest end. A sine whose RMS is the reference reads 0 VU, which is a
sine of peak 3 dB higher (peak -15 dBFS for -18). A reading below -20 VU is written `< -20`. The
reference is a display choice of each viewer: the web page keeps it per browser, the TUI in
`[tui] vu_reference`.

## Ballistics

The needle is a second-order system, `x'' + 2ζω x' + ω² x = ω² u`, with `ω = 14.2` rad/s and
`ζ = 0.83`, `u` being the latest ratio held until the next frame. It is integrated in fixed 1 ms
steps (semi-implicit Euler) on a whole-millisecond clock the caller passes in, so its path depends
only on `u(t)` and never on the frame rate or on how often the screen is drawn. The fit is to an
oracle: a step from 0 to 1 first reaches 0.99 at 300 ms (within 15 ms) and overshoots by about
0.85 %; a step from 1 to 0 is under 0.01 at 300 ms. The state is held between 0 and the pin's
ratio, and speed is zeroed on a stop, so the needle leaves a stop at once. The first call starts the
clock with the needle at rest; a gap over 2000 ms between two calls puts the needle at its steady
state instead of integrating through it. The model holds each channel's RMS amplitude, so a change
of reference acts on the level at once.

## The over lamp

A lamp for each channel lights at any frame whose sample peak is at or above full scale of a 16-bit
signal (`OVER`, 32767/32768) and stays lit for 1200 ms after the last such frame. The wire carries
it as the top value of the peak byte ([the capture](host-capture.md#the-levels-and-the-over-rule)).
The rule is `>=` on the same constant at every step, as a 16-bit full-scale square arrives at the
tap exactly at `OVER`.

## The golden file and the oracle

`vu::golden()` is the needle's path for a fixed script (silence, a -18 dBFS-RMS sine, a +6 dB
burst, decay, frame times at 30 Hz and irregular), sampled every 10 ms for 8 s, with the model's
constants. `crates/svolder-host/examples/vu_golden.rs` writes it to
`scripts/web-checks/fixtures/vu-ballistics.json`; `make vu-golden` rewrites the file and
`make vu-golden-check`, part of `make check`, fails when it is not what the engine gives. The node
check `scripts/web-checks/node/vu.mjs` holds the page's model to the same file within 1e-4 at every
sample, and asserts the model's constants against it, so the two engines cannot drift. The oracle
(the 300 ms rise and fall) is asserted by the engine's own tests.

## On the web

`core/vu-model.js` is pure: `decodeLevels` reads a full or a levels-only frame event (four integer
bytes: RMS and peak for left and right), `VuModel` has `push`, `step`, `settled` (a reading with
no easing, for reduced motion), `setRef` and `clear`. The core holds the levels-only stream as
`core.levels`, apart from `core.analysis`. Signal runs the model on the page's own clock.

## In the terminal UI

From a terminal of 100 columns by 45 rows with host audio on, the Status pane draws the rows. The
needle is the engine's, on the loop's real time: a block (or a daemon frame, through
`Frame::block()`) lands its levels, and a draw reads the needle at the moment of the draw. A needle
that has no block for 500 ms reads its input as 0, the stream having stalled. The capture runs while
the rows show (not under a Settings box, help or a menu) and the loop redraws every 50 ms while a
needle is off rest or a lamp is lit, never otherwise. A joined TUI takes the daemon's full frames
stream (its Spectrum row wants the bands), not the levels-only form.
