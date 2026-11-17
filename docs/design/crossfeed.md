<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's crossfeed

A crossfeed feeds a low-passed, delayed copy of each channel into the other channel at a set
level. svolder runs it as one fixed stage inside the computer EQ's
filter chain. This page is the graph, the maths of the four models, the lifecycle that keeps the
stage from being left on, the state and the contract. What a user sees and does is in
[eq.md](../usage/eq.md#crossfeed); the chain it sits in is in [host-control.md](host-control.md), the
level rule in [host-control-preamp.md](host-control-preamp.md). Crossfeed and the
[virtual speakers](virtual-speakers.md) are one choice: turning crossfeed on turns the speakers off
in the same update, and the other way round. The code is
`crates/svolder-host/src/eq/crossfeed.rs` (pure: settings, models, maths, the view),
`eq/live.rs` (the graph and the `set-param`s) and `eq/control/xf.rs` (the controller).

## The graph

After the sixteenth EQ slot each channel `C` (with `O` the other channel) has six nodes, the same
whatever the model:

```text
C16 --+--> Cdir ------------------------------------> Cmix.In 1 --> out
      +--> Cs  --> Csum.In 2 --+
O16 -------> Cx --> Csum.In 1 -+--> Cdel --> Cmix.In 2
```

`Cdir`, `Cx` and `Cs` are `bq_raw` biquads (the EQ slots' own kind), `Csum` a two-input mixer
(`Gain 1` for the cross path, `Gain 2` for the side path), `Cdel` a `delay` node built for 2 ms
(`DELAY_BUFFER_S`) and `Cmix` the last mixer whose `Gain 2` is 1 when the stage plays and 0 for the
bypass. The output of channel `C` is

```text
out_C = H_dir * C  +  D * (H_s * C  +  H_x * O)
```

with `H_dir` the direct path, `H_x` the cross path, `H_s` a side-subtract path on the channel's own
signal and `D` the added delay (a pure phase). The mixer cannot take a negative gain, so a path that
subtracts carries its sign in its numerator.

The configuration on disk always has the stage OFF: a bypass that is bit-exact (`dir` the identity,
`x` and `s` silent, `Cmix` `Gain 2` 0, so the output is the input). Turning it on is a `set-param`
of `crossfeed_params` (the three paths' coefficients, the sum's gains, the delay in seconds and
`Cmix`'s `Gain 2`), one call for both channels, with no rebuild and no gap. A chain that cannot be
set live has the stage written into its configuration instead (`render_live_conf_with`).

## The models' maths

A section is `[b0, b1, b2, a1, a2]` with `a0` 1, the transfer function
`(b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2)`, the convention of the EQ's slots. `A` is the
share of the other channel at low frequencies (0 to 0.5, 0.5 being mono), `fc` the crossover, `rate`
the sink's rate (the crossover is held under 0.499 of it).

- `LP1(fc) = (1 - p) / (1 - p z^-1)` with `p = exp(-2 pi fc / rate)`, a first-order low pass of DC
  gain 1.
- `LP2(fc)` is the second-order Butterworth (Q 0.7071) low pass of DC gain 1, the cookbook form.

**Linkwitz**: `H_dir = 1 - A`, `H_x = A * LP1(fc)`. The direct path gives up what the cross path
adds, so the DC level is 1.

**Meier**: `H_dir = 1`, `H_x = A * LP1(fc)`, `H_s = -A * LP1(fc)`. For a signal common to both
channels the cross and side terms cancel at low frequencies, so the centre is untouched; only the
difference between the channels is crossed. The Meier preset "Rockbox" (24 %, 660 Hz, no added
delay) comes from the coefficients of the Rockbox project's open-source `crossfeed.c`: its
single-pole side path has a corner of about 657 Hz and crosses 24 % of the difference.

**Mid/Side bass**: `H_dir = 1`, `H_x = A * LP2(fc)`, `H_s = -A * LP2(fc)`. Below `fc` each channel
is `(1 - A)` of itself plus `A` of the other: at 50 % the bass of both is the mid signal, the centre
and the highs pass untouched.

**Bauer** is the stereophonic-to-binaural crossfeed as the bs2b library sets it, from a feed level
`F` in dB (how far the crossed signal sits below the direct one at DC) and the low-pass corner `fc`:

```text
Gb_lo = F * (-5/6) - 3          G_lo = 10^(Gb_lo / 20)
Gb_hi = F / 6 - 3               G_hi = 1 - 10^(Gb_hi / 20)
Fc_hi = fc * 2^((Gb_lo - 20 log10(G_hi)) / 12)
gain  = 1 / (1 - G_hi + G_lo)
```

The cross path is `gain * G_lo * LP1(fc)`. The direct path is bs2b's high boost scaled by `gain`:
with `x = exp(-2 pi Fc_hi / rate)`, `[1 - G_hi (1 - x), -x, 0, -x, 0] * gain` on the numerator. The
direct path's highs are lifted by 6 dB per octave so that the two together stay level, and `gain`
makes direct plus cross 1 at DC.

**Level and the feed level.** Level is the share `A` of the other channel at low frequencies.
For Bauer the feed level and `A` are one number in two units, `F = 20 log10((1 - A) / A)` and
`A = 1 / (1 + 10^(F / 20))`: the presets keep bs2b's own `F` (4.5, 6.0 and 9.5 dB, so they stay
exact) and the percent is derived (37 %, 33 %, 25 %). The view carries both: `amount_pct`, `feed_db`
and `amount_db = 20 log10(A)`, the level of the crossed signal against the input, which is what a
screen shows beside the percent (more percent, a higher dB). Bauer's level is held to 15-47 %
(bs2b's feed level runs 1 to 15 dB; 15 % is 15.06 dB, just past it, harmless).

**The lag and its floor.** The cross path is a low pass, so it arrives later than the direct path.
`own_lag_ms` is the difference of the two paths' group delays at 20 Hz, read from the phase of `H_x
/ H_dir` around the probe (about 0.26 ms for Bauer at 700 Hz, falling as the crossover rises). The
delay the user sets is the total lag of the other side: the stage adds `total - own` to reach it,
and a value at or below `own` is stored as "own" (`delay_ms` absent), so the floor is the model's
own lag and moves with the crossover. The added delay plays in whole samples, truncated (21 us at 48
kHz): `delay_samples = floor((total - own) * rate)`, and the value sent to the node is the middle of
that step, `(n + 0.5) / rate`, so PipeWire's own truncation cannot land a sample short. Mid/Side
bass plays no added delay (its low pass lags by itself, about 1.9 ms at 120 Hz, which no control
changes); its view shows 0 for the delay and the floor.

**The worst gain.** One output is `H_dir C + D (H_s C + H_x O)`, which for the two inputs aligned to
add up is at most `|H_dir + D H_s| + |H_x|` at a frequency. `worst_gain_db` is the maximum of that
over a grid of 1 Hz steps from 1 Hz to Nyquist for the stage at the running rate, in dB, never below
0. For Meier it depends on the played delay (0.5 dB at the model's own lag, 2.0 dB at 0.5 ms total,
2.9 dB at 1 ms), so the preamp is re-folded when the delay changes. Those are the worst gain at the
running rate, not the preamp: the preamp counts the most of it over the usual rates (below). The
rule it serves, "nothing plays above unity", is per frequency, as the EQ's preamp rule is; it is not
a bound on the samples: the time-domain bound `sum |h_dir| + sum |h_x|` of Bauer's default is +2.45
dB at 48 kHz, so the page never claims a sample-peak guarantee.

**Precision.** The chain's control type is f32, so every coefficient is rounded to f32 before the
worst gain, the curves and the stability test are worked out: they are the values sent. A section is
refused when a coefficient is past the chain's limit of 10 or its poles are not strictly inside the
unit circle in f32 (`check`), with the words "lower its crossover". Mid/Side bass at 120 Hz and 192
kHz keeps its DC share within 2e-4.

## Settings, ranges and the table

`CrossfeedSettings` holds the model, the preset (absent is Custom, with `edited_from` the preset the
values left), the level as a share, the crossover in Hz and the delay (absent is "own"). The table
`SPECS` is the one place that holds the models' words, presets and ranges; a front end reads it from
`CrossfeedView.models` and hard-codes none. `CrossfeedSettings::change` is the one function every
front end's request goes through:

1. an unknown model word or preset, a number that is not finite and a delay for a model that plays
   none are refused in words;
2. a model other than the current one loads that model's default preset; naming the current one
   changes nothing, so it does not wipe Custom values;
3. a preset loads its exact numbers and the model's own delay;
4. a level, crossover or delay makes the values Custom, each rounded to the model's step and held
   to its range; a delay at or below the model's own lag is "own".

`held` holds any settings to their ranges (a state file edited by hand cannot make an impossible
stage). A crossover that moves the model's own lag below a stored delay turns that delay into
"own".

## The view and the contract

`EqView.crossfeed` is `CrossfeedView` (every field defaulted, so a view from an older daemon
parses): `on`, `model`, `preset`, `edited_from`, `amount_pct`, `amount_db`, `feed_db`,
`crossover_hz`, `delay_ms`, `delay_floor_ms`, `delay_settable`, `delay_own`, `output`, `note`,
`off_reason`, `unavailable`, `curve` and `models`. The curve is two 61-point series on a log grid
(20 per decade, 20 Hz to 20 kHz) in dB floored at -120: the same ear (direct plus side path) and the
other ear (cross path), at the running rate with the played delay, so no front end works out the
maths. `unavailable` says why the controls are dim (a chain of an older svolder; a curve the chain
cannot hold) and every front end shows it instead of failing a press.

The one write is `HostEqOp::Crossfeed { on?, model?, preset?, amount_pct?, crossover_hz?,
delay_ms?, done? }`; the controller owns every rule, and every front end (the TUI, the web route
`api/v1/host/crossfeed`, `svolder host crossfeed`, the MCP tool `set_host_crossfeed`, Home
Assistant) sends it and reads the view. One `svolder_ipc::host::run_eq_op` carries a request out for
the daemon and for a TUI with no daemon alike. The preamp the view shows is the played one (the
EQ's less the stage's); the state file keeps the EQ's own.

## The lifecycle

The controller keeps two facts apart: whether a chain runs (`runs`) and whether the EQ is on
(`eq_on()`). Crossfeed has its own switch.

- Turning it on with no chain applies a chain with the EQ flat (`flat`: slots the identity, preamp 0
  dB) and sends the stage. The EQ turned off while crossfeed is on makes the chain flat live; the EQ
  turned on while it is flat sends the slots live; both off removes the chain, except that while a
  stream plays through it the chain stays as a flat pass-through (preamp 0 dB, lower only if the
  loaded configuration forces it, live with no sink swap, so the player is not paused) until nothing
  has played through it for about 30 s (`IDLE_KEEP`; `SVOLDER_EQ_IDLE_TICKS` shortens it for tests);
  "playing" is a `running` stream node linked into the chain's sink, never a monitor capture. A
  graceful stop, an owner restart or a death removes it at once, and the view note says "chain kept
  as a pass-through until idle". None of the first two applies or removes anything. Two chains never
  stack.
- **The level.** The stage costs `stage_cost_db`: the worst gain rounded up to 0.1 dB, with float
  noise as the only tolerance (the EQ's preamp beside it forgives 0.008 dB), subtracted from the
  EQ's preamp and re-folded whenever model, level, crossover or delay move it. One update lowers the
  preamp before the stage turns on or grows and raises it after the stage shrinks or turns off, so
  no step plays above unity. The live stage costs what a configuration-held one does, the most of
  its worst gain over the usual rates (44.1, 48, 88.2, 96, 176.4, 192, 352.8 and 384 kHz, and the
  sink's own) for the stage worked out at the rate last looked at, because a line sent between a
  rate change and the next look (a crossfeed drag, the 1 s repeat, the session's full repair line)
  plays the old stage with its delay in other whole samples. Only Meier with an added delay pays: up
  to about 2 dB more preamp (0.5 ms at 44.1 or 48 kHz: -3.6 to -5.9 dB at 50 %, -2.0 to -3.4 dB at
  the default level; up to about 3.5 dB at extremes); Bauer, Linkwitz, Mid/Side bass and Meier at
  its own lag are unchanged. A curve its chain holds that cannot play at 352.8 or 384 kHz is applied
  again with the points on screen, and the EQ turns off with a note only when that fails. With
  crossfeed on and the EQ off the flat chain plays at the lowest level its loaded configuration
  allows until it restarts.
- **Four resets to off**, each with a note and a reason: the owner starts and finds it on
  (`restart`), the output the stage was turned on for is not the output any more (`output-changed`,
  from the 1 s look of the same `pw-dump` that reads the node), the chain restarted, a new node
  (`chain-restarted`, told by its `object.serial`: PipeWire may list it under the old id), and a
  graceful stop (`stopped`). A chain that runs for crossfeed alone is removed by every reset; any
  other has the stage turned off live, or is applied again without it when its configuration may
  hold the stage or live updates fail. A reset whose chain part fails is retried once a second and
  the note says so. A change of the sink's rate is no reset: the stage is sent again for the new
  rate, or turned off with a note when it cannot play there (no rate in the engine's ranges reaches
  that branch, so it has no test).
- **Reverts are seen.** A look that finds any port (the preamp, the stage's mixer or a slot) at a
  value other than the one the look before held at its sent value, at the same node and rate, sends
  everything again (at most 4 times in 5 s, then crossfeed turns off with a note). A port the look
  before did not yet hold at its sent value (a slow `pw-cli`) is no revert, so a line that is merely
  late is not sent twice.
- **A dead chain is noticed** through `Host::exited` (`control/exit.rs`), one task of the controller
  for the chain's life that only waits and then signals it: it follows the chain (an apply or remove
  is not an exit), resolves when an EQ is on and no process runs it (a unit: only when systemd
  answers inactive or failed), and never while no EQ is on or when the chain cannot be looked at
  until the next apply or remove looks again. The first death starts the chain again with crossfeed
  off; a second within 10 s of that restart is ridden out and starts it again too (a PipeWire
  restart stops it once more), and only a third, again within 10 s, turns the EQ off with a note.
  It is never recreated per tick, and after it resolves the controller applies or removes before
  waiting again.
- **After a crash** the state's `crossfeed_on` and `flat` make the next owner start take the stage
  out of the chain, or remove it. Settings changed while no state file exists are kept in memory only
  until a chain is applied.

Every crossfeed claim above has a test over `FakeChain` and `FakeParams` (the fakes of
`svolder-host`); the real PipeWire is only touched by `make host-e2e`, on a private one.

## What is not verified

- - The numbers of **Linkwitz**, **Meier** and **Mid/Side bass** (names, ranges, defaults) are
  svolder's own choices from the models' published forms, not checked against any maker's figures.
  Only the maths of Bauer's presets is exact to bs2b's.
- - The stage's graph has been seen loading on a private PipeWire with the fixture's property names;
  its played level has not been measured yet, and no model has been judged by ear; the curves in the
  view are the maths, not a measurement of the played sound.
- Live updates were tried on one PipeWire and one WirePlumber version (see
  [host-control.md](host-control.md)).
