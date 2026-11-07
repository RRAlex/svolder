<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer EQ's preamp

The preamp of the computer EQ (`played_preamp` in `eq/manage.rs`) is the lower of a preset's stated
preamp and minus the curve's peak, floored at 0 and rounded up to a tenth of a dB, forgiving
0.008 dB, so the curve as played never passes unity by more than 0.008 dB. This page is how that
peak is measured and what the chain refuses; the controls are in [host-control.md](host-control.md).

## What is measured

- **The filters as they play.** Each is held to what its biquad is worked out for (1 Hz to just
  under Nyquist, Q 0.01 to 100, within ±60 dB) and read at the running rate.
- **The coefficients as PipeWire runs them.** PipeWire takes its control values as f32, so the
  coefficients are rounded to f32 first: they are the very values sent.
- **The whole band the chain plays, DC to Nyquist.** A shelf's asymptote and a resonant shelf's bump
  lie outside 20 Hz to 20 kHz, so a shelf preset gets a lower preamp than that window gives: a
  resonant shelf of +12 dB at Q 10 needs about −30 dB where the window gives −6. The DAC's red zone
  keeps the app's window.
- **Narrow peaks.** A peaking filter near Nyquist is a few hertz wide, so the samples are a log grid
  in frequency from 0.1 Hz, a second grid log-spaced in the distance from Nyquist and each filter's
  own centre; every local maximum of them is refined by a golden-section search. One call is about a
  millisecond for 16 filters, and the newest answer is remembered, since a drag asks for the same
  curve several times an update.

## What is refused

A curve is refused, with the words of the refusal, nothing applied and the EQ unchanged (an edit
while the EQ is off is refused before it turns on), when:

- a filter's f32 coefficients are not stable at the running rate: the second Jury condition,
  `1 + a2 − |a1|`, must stay above 1e-7, which is about the f32 step of the coefficients. A low
  shelf of +30 dB at 10 Hz and Q 20 at 96 kHz or more is one. Every point a hand can set, at every
  rate up to 192 kHz, passes. The margin guards stability only, not fidelity or noise (see the
  known issue on f32 noise). A filter of 0 dB is the identity and is never refused, so a lone point
  can always be removed;
- a coefficient is past ±10: PipeWire clamps a `bq_raw`'s coefficient ports to that, either way
  (read back from a private instance: 10.5 reads 10 and −31.6 reads −10), so a high shelf of more
  than about 14 dB at a low corner, or a wide peak of +30 dB, would play as another filter. What a
  hand can set stays inside (the largest is 7.96);
- the curve needs a preamp below −120 dB: it is silence with a resonance on top. The floor is a
  policy, the same as a typed biquad's gain minimum; `Mult` itself holds far lower.

A refusal names the point as the table does ("point 3 …"). One bad saved point therefore blocks
edits to the others until it is mended or removed.

## When the rate changes

The coefficients are worked out for the rate the sink runs at, so a rate change or a restart of the
chain checks the curve again at the new rate before anything is sent. A curve that is not playable
there turns the EQ off: the chain leaves the path, the points are kept, the state says off, and the
note says which point and at which rate. The note goes at the next successful apply. Nothing is
sent at the new rate. The rate is forgotten with the chain, so an apply with none there reads the
default output's rate (`pw-dump`) and checks and writes at it; when it cannot be read it is the
default 48 kHz and the first look at the new chain checks it at the rate the sink runs at.
If the chain cannot be taken out, the note says so, nothing is sent, and the chain is looked at
again: it plays on again once the sink's rate plays the curve, or goes off once it can be removed.

## What a rate change does to the level

A rate change rebuilds the graph: the slots go back to the coefficients of the curve the chain was
applied with, but the preamp keeps the value last sent (measured on a private PipeWire: a +6 dB
peak on the tone, applied and then dragged to 0 dB live, played +5.8 dB over unity for 0.1 to
0.2 s at each change while the preamp had risen to 0). So no preamp is ever sent above the one the
applied curve needs, at the running rate; the controller also keeps the loaded curve apart from
the state it saves. The sound after a rate change is shifted for up to about 2 s, never over unity.

Three more rules keep that true when something else changes the chain:

- Every live update carries all sixteen slots with the preamp, not only the slot that moved. A rate
  change reverts the slots without telling anyone, and a line with one slot would leave the others
  as the config has them under a preamp worked out for the points as they stand.
- A chain the controller did not start (it was running when the daemon started, it started again,
  the command line replaced it, or it is the first looked at after an apply) is looked at before
  anything is sent to it. The preamp it holds then bounds every preamp sent until an apply or an adopt says what it was
  started from: it was safe for the slots the chain starts from. A reload does not lift the bound
  for good: it is taken again from the node at the next look.
- The session sends only to the node id the controller looked at. When the node it finds is under
  another id (a refusal dropped the kept node, or a write failed and it was found again), the line
  is not written; the controller looks at that node and sends once more.
- The config on disk is written with the preamp it needs at its own rate, so a chain that starts
  again from it is capped by that number and not by the preamp the points state. A state file that
  the command line wrote is checked at the running rate and sent whole on the first look after the
  daemon reads it, whatever the rate.

The price is plain: after a curve is softened live, the level stays at the applied curve's preamp
until the next apply, which is turning the EQ on, loading a preset softer than the applied curve,
a change of a point's type, a point added or removed, or a restart of the chain (which loads the
config on disk). A hand edit that softens the curve does not apply.

## A chain from an older svolder

A chain an older svolder built (a typed `param_eq`, with no preamp control) may still play after an
upgrade. The view says so in its note, shows the preamp it was applied with, and nothing is sent to
it and nothing written at start. Turning the EQ off and on, an edit that ends or a preset load
replaces it. A node listed before its controls are is taken for one at first and looked at again
until they appear.

## The preamp lab

`make preamp-lab` is the safeguard for all of the above: the controller driven through seeded
random sequences of events against a model of what PipeWire plays (`eq/lab.rs`). The model holds
the config on disk, the node (its id, the config it started from, every port's value as an f32,
coefficients held to ±10), the sink's rate, a `pw-cli` that can die, the state file and a clock.
It takes from PipeWire what was measured on a private instance: a rate change puts the slots back
to the config the node started from and leaves the preamp as last set; a restart is a new node
from the config on disk; a `set-param` to an id that is not the node's does nothing; a line is
applied whole or not at all. The events are hand edits (drags and releases), points added and
removed, preset loads harder and softer than the loaded curve, the EQ on and off, command-line
applies with the state file written before or after the daemon's look, state reloads, node
restarts, rate changes among 44.1 to 192 kHz, `pw-cli` deaths (also mid-line), refused sends, a
write that fails once, refusals the session reads zero to two reads late (its reader task sets
the flag a moment after `pw-cli` prints it), a failed Off or apply, nodes that are listed late, daemon restarts and ticks of 50 ms.

After every event and every tick the curve the node plays (its ports, as the model holds them)
times the preamp it plays must peak at most 0.01 dB over unity over the whole band at the current
rate, measured by an evaluator of its own, and each biquad must be stable. Also: no `set-param` to
a stale id after the controller's own apply, nothing written or sent while the daemon starts, and
no panic. A failing seed prints the shrunk sequence; `SVOLDER_LAB_SEED=<seed> cargo test -p
svolder-host --lib lab_trace -- --ignored --nocapture` prints what happened step by step. A short
run of 300 seeds is part of `make check`; `make preamp-lab SEED=<first> RUNS=<count>` runs the
long one (5000 sequences by default, about a minute and a half).
