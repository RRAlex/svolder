<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's loudness

Below full volume the ear hears the bass and the top of the band less than the middle. The
computer's loudness gives that back by taking the middle and the top down as the computer's volume
falls, so it never raises any frequency. svolder runs it as one fixed stage inside the computer
EQ's filter chain and moves it live with the computer's volume. This page is the graph, the maths
with its constants, why the stage can never be louder than its input, how the volume is read and
followed, the lifecycle, and the state and contract. What a user sees and does is in
[eq.md](../usage/eq.md#loudness); the chain it sits in is in [host-control.md](host-control.md),
the level rule in [host-control-preamp.md](host-control-preamp.md), the other stages in
[crossfeed.md](crossfeed.md) and [virtual-speakers.md](virtual-speakers.md). The code is
`crates/svolder-host/src/eq/loudness.rs` (pure: settings, the maths, the view),
`eq/live.rs` (the graph and the `set-param`s) and `eq/control/ld.rs` (the controller).

This is the computer's loudness. A device's own Loudness (a mode of its tone control, set in its own
steps) is a different thing, covered in [tone-control-and-presets.md](tone-control-and-presets.md);
the screens tell the two apart by their place, the computer's side or the device's.

## The graph

After the sixteenth EQ slot each channel holds two first-order `bq_raw` sections, `ld1` (the low
shelf) and `ld2` (the high shelf), named `lld1`, `lld2`, `rld1`, `rld2` in the graph:

```text
C16 --> Cld1 --> Cld2 --> the crossfeed stage (or out)
```

The crossfeed stage, and the virtual speakers after it, take `Cld2`'s output, after the sixteenth
slot. The order is the filter chain's: EQ, loudness, crossfeed, virtual speakers. Every stage is
linear and time-invariant, so the order changes headroom only, and loudness adds none. It is not
exclusive with crossfeed or the speakers.

The configuration on disk always holds the two sections as the identity
`[b0, b1, b2, a1, a2] = [1, 0, 0, 0, 0]` (`a0` is 1), whatever loudness is set to: no render
parameter carries the stage. Turning loudness on or moving it is one `set-param` of the stage's 24
values (2 channels, 2 sections, 6 ports), never a rebuild. A chain without these ports (one built
before the loudness stage) is rebuilt once when loudness is turned on, and the view says so.

## The maths

With `v` the computer's volume on the cubic scale a volume control shows (0 to 1), `c = v³` the
gain on the samples, `r` the reference (the volume at which loudness is flat) and `s` the strength
(both 0 to 1):

```text
V = 20·log10(c) = 60·log10(v)     R = 60·log10(r)     d = clamp(R − V, 0, 40) dB     d_e = s·d
G_lo = 0.53 · d_e                 G_hi = min(0.15 · d_e, 6) dB
```

`d` is how far the volume sits below the reference, counted to 40 dB, and `d_e` that depth scaled by
the strength. At the reference and above, and at strength 0, `d_e` is 0 and the stage is the exact
identity: loudness never plays an inverse curve, and a volume past 100 % is flat too. The depth the
controller plays is held to a 0.1 dB grid.

The stage is a low shelf (its zero at 289 Hz, `G_lo` of gain at DC) followed by a high shelf (its
zero at 7 760 Hz, `G_hi` of gain at Nyquist). The constants are the best joint fit of the two
shelves to the difference between the equal-loudness contours of ISO 226:2003 at 80 phon and at 80
phon less the depth; `make loudness-fit` prints them from the standard's formula and its table
(`scripts/loudness-fit.py`, with the 29 frequencies from 20 Hz to 12.5 kHz). The gains are linear in
the depth because the difference curve is close to linear in it (within 0.25 dB at 40 dB of depth
and half strength), so strength is the depth scaled and one set of four constants serves every
setting. The fit's worst error against the standard is 0.7 dB at 5 dB of depth, 1.2 at 10, 1.4 at
20, 2.1 at 30 and 2.7 at 40; no four constants do better at 40 dB under the 6 dB treble cap.

Digitally each section is `k·(s + Kz)/(s + Kp)` in the bilinear domain, with `Kz = tan(π·f/rate)`
for the zero's corner:

```text
low:  Kp = Kz / 10^(G_lo/20)    k = 1
high: Kp = Kz · 10^(G_hi/20)    k = Kp / Kz
b0 = k·(1 + Kz)/(1 + Kp)   b1 = k·(Kz − 1)/(1 + Kp)   b2 = 0   a1 = (Kp − 1)/(1 + Kp)   a2 = 0
```

The pole is set from the gain, not from a second prewarped frequency. With the pole prewarped too, a
stage at 44.1 or 48 kHz would rise 14 to 18 dB at Nyquist and the maximum would move there; set
from the gain, the stage's gain at DC is `G_lo` and at Nyquist `G_hi` at every rate (before the cut
below), 705.6 and 768 kHz included. Each section's magnitude is monotonic from DC to Nyquist (one
real pole and one real zero inside the unit circle), and a first-order section is well conditioned
in f32: at the deepest stage the low pole sits 4·10⁻⁴ inside the circle at 384 kHz and about 2·10⁻⁴
at 768 kHz, against an f32 step of 6·10⁻⁸. No second-order sections are used: two poles near
`z = 1` are what f32 cannot hold.

A worked example: reference 100 %, strength 50 %, volume 35 %. `d = 27.4`, `d_e = 13.7`, `G_lo =
7.3 dB`, `G_hi = 2.1 dB`: the bass is kept, the middle plays 7.3 dB under it and the top 2.1 dB
above the middle. That is what the readout says: the **shelf gains** (the bass `+7.3` above the
middle, the mids `−7.3` under the bass, the treble `+2.1` above the middle), not the response at
20 Hz, 1 kHz and 12.5 kHz.
The middle, on top of the volume's own cut, falls by `d + G_lo`, 1.5 times as fast as the knob at
strength 100 % and 1.27 times at 50 %: that is the volume cut taken in the middle and the top.

## Why it is never louder

Let `M` be the largest gain of the two sections together over DC to Nyquist, measured on the
f32-rounded coefficients with the preamp's own evaluator (a log grid refined by golden section).
The low section's `b0` and `b1` are divided by `M`, rounded to f32 and measured again, and any
excess the rounding leaves is trimmed one f32 step at a time until the f32 maximum is at most 0 dB.
So the stage `S = H_lo·H_hi / M` has `|S| ≤ 1` at every frequency, 0 dB at DC and about `−G_lo` in
the middle, and a depth of 0 is the exact identity. A coefficient is never beyond ±10 (the
largest is about 2), the f32 response matches the f64 design within 0.05 dB from 20 Hz to 20 kHz at
every rate, and `|a1| < 1 − 10⁻⁷`.

### The preamp pays nothing for it

The computer EQ's preamp is `min(stated preamp, −peak of the curve)`, lowered by what the stages
after the EQ could add at their worst frequency, then held to what the configuration allows, so that
no state made of whole sections plays over unity ([host-control-preamp.md](host-control-preamp.md)).
The node plays `P(w) = m · ΠH_k(w) · S(w) · X(w)` (`X` the crossfeed and speakers stages, `m` the
preamp), and the existing proof is `|P| ≤ m · peak(ΠH_k) · |S| · worst(X) ≤ 1`: loudness adds one
more factor that is at most 1. Its cost is therefore 0, and:

- the preamp never moves for a volume change, and a loudness line carries no preamp of its own;
  loudness never changes the preamp's value;
- loudness is outside every preamp sum: the cap a configuration puts on the live preamp, the total
  a recall wants, the cost of the stage a configuration holds, the test for lifting a held level and
  the preamp the state file saves all leave the stage out, and the two identity sections in a
  configuration are unity, so a configuration's own preamp is the one of the same configuration
  without the stage;
- no order between a loudness line and any other line is needed, and a stage worked out at one rate
  and played at another (a line between a rate change and the next look) has the same maximum,
  because the maximum over the unit circle does not depend on the rate (no whole-sample delay here,
  unlike crossfeed's);
- the configuration's identity and a graph that has not run (its ports read zero) are at most 1.

Every state an interleaving of whole sections, a revert, a restart with the same id or a rate change
can leave is under unity, independently of how late the volume is read and of where the sink applies
its volume (a hardware mixer applies it after the digital stream, so a boost paid from the volume
would clip the stream). The alternatives were set aside for the same reason: boosting the bass and
lowering the preamp per step is the same sound but a preamp move with an ordering rule on every
volume event, and paying for a boost from the volume's own attenuation lets a jump to full volume
play over unity while the reading is late. svolder does not move the sink's own volume to give the
middle back; it would fight the desktop's volume keys.

## The volume

The controller needs the volume the ear hears: the sink's master volume times the lowest of its
channel volumes, and whether it is muted (`mute` or `softMute`), and, while the chain is in the
path, the chain node's own volume times it. That is the number the route line already shows
(`route::computer_level`, one function the route and the controller share); a node that lists no
volume counts as full. It equals the Volume row whenever the other node in the path sits at 100 %.

Two readings reach the controller, and the newest wins:

- **The look.** The one-second look at the chain reads a full `pw-dump` for its node anyway, and the
  volume comes out of that same dump: no extra spawn, no read while no chain runs.
- **The hint.** The daemon's and the TUI's volume writes read the volume back and hand it over
  through a `LevelHint`, a lock-free cell: a tick holds the controller's lock through a `pw-dump`,
  so a write never waits for it. The next tick takes the hint. A hint is the default sink's own
  reading; it replaces that sink's share and keeps the other node's share as the last look gave it
  (1.0 until a look has seen it).

So a volume change made through svolder is followed on the next 50 ms tick, and one made elsewhere
(a desktop key) within the second.

## Following it

On each tick with loudness on and a chain live, the target depth `d_e` is worked out from the newest
reading. The depth played moves toward it by at most 0.5 dB a tick, about 10 dB of depth a second,
on the 0.1 dB grid, and nothing is sent once it has arrived. Muted, or with no reading, it holds
where it is.

**One line per tick.** Every `pw-cli` line costs a `pw-dump <id>` read first (13 to 23 ms under the
controller's lock). A step marks every slot due, so it goes as an ordinary full line: the slots and
the preamp at the values they already hold, which changes nothing and leaves them confirmed, and the
stage's 24 values. The stage never takes a line of its own beside another on the same tick. A glide
is at most 20 reads a second, a drag's rate. The repair line and the first-run resend carry the
stage as they carry everything, and the line sent a second later repeats it once. A slow `pw-cli`
during a glide is no revert (a look that finds the node behind the newest line waits); the check is
the one that confirms live updates, and it counts a hit for the value it was armed with or any value
sent for that port since, so a node that lands every line one tick late is not taken for a node that
refuses live updates.

**The order inside a line.** Whatever order the sections land in, no state made of whole sections
plays over unity. (A section's own six values land one at a time in PipeWire, and a quantum can play
between two of them; see the limits below.) Every line sets the low section first, its coefficients
cut for the highest high section the node may still hold: the last two lines that went through, the
lines that failed since, and the look's stage until a look confirms one this controller sent. Only
the stop's identity, which no repair follows, goes high section first. The cost is a dip of at most
about 0.15 dB across the band at the end of a glide up (up to about 1.5 dB while a look finds the
node behind), quieter and never louder; the look that confirms the node caught up puts the exact
stage on.

**A new owner.** A daemon or a TUI that starts over a node still holding a stage plays from a depth
of 0 and glides back to the volume's depth: its first line steps the stage to 0.5 dB, cut for the
stage the node held, and the look settles it.

## The lifecycle

- **Turning it on** with no chain applies a flat chain and sends the stage. What keeps a chain is
  `keeps_chain`: crossfeed, the speakers or loudness on over a live chain. So the EQ off with
  loudness on goes flat live and keeps playing the stage; crossfeed or the speakers off with
  loudness on keeps the chain; and a chain that runs for loudness alone is never a pass-through.
  EQ, loudness, crossfeed and the speakers all off removes the chain.
- **It stays on** across restarts (unlike crossfeed, which starts off): it can only make the sound
  quieter, so nothing needs resetting. `on`, the strength and the reference are saved in
  `host-eq.toml`; a new owner follows the volume from its first look. Settings read from
  `host-eq-settings.toml` (kept before any chain was applied) come back with loudness off. A graph
  that has not run yet is sent nothing for loudness until the look that sees it run.
- **A graceful stop** sends the stage flat at once and keeps the chain, so a chain that ran for
  loudness alone plays the identity until the next owner finds it and follows the volume. Removing
  it would leave loudness on with no chain after every daemon or TUI restart, because starting the
  service applies nothing outside a container. A crash leaves the last cut until the next owner:
  quieter, never louder.
- **The configuration on disk never holds a non-identity stage.** A node found holding one while
  loudness is off is sent the identity.
- **An output change or a new chain** resets nothing: it follows the new sink's volume. A rate
  change, a chain restart (a new serial) and a revert are noticed at the look that sees them and
  send the stage again; the rebuilt chain played the identity meanwhile.
- **No cut-off by rate.** Crossfeed and the speakers stop at 384 kHz; loudness plays at 705.6 and
  768 kHz as well, and turning it on is never refused for the rate.
- **Giving up.** A chain that takes no live updates, a legacy chain, a chain held for a sample rate
  its curve cannot play at or one that gave up live updates makes loudness unavailable with a note;
  nothing is applied, because following the volume through configuration rewrites would restart the
  chain on every step. When the controller gives up after repeated reverts, loudness stays on and
  the next look's revert sends it again.
- **A state read at start** adopts loudness before crossfeed, so a flat state with loudness on owes
  no chain removal.

## State, view and contract

The settings are `LoudnessSettings` (`on`, `strength_pct`, `reference_pct`), saved as `[loudness]`
in `host-eq.toml` and, before any chain was applied, in `host-eq-settings.toml` beside it. Strength
is 0 to 100 % in steps of 5 (default 50 %), the reference 1 to 100 % in steps of 1 (default 100 %,
the one point that needs no calibration). One `LoudnessSettings::change` takes every front end's
request: values are snapped and held to their ranges, and a refusal comes in words.

`HostEqOp::Loudness { on?, strength_pct?, reference_pct?, reference_now?, done }` is the op every
front end sends, and `EqView.loudness` the view every front end reads: `on`, `strength_pct`,
`reference_pct`, `volume_pct` (absent until read), `depth_db` (the effective depth now playing),
`lift_db`, `mids_db` and `treble_db` (the shelf gains, to 0.1 dB), `following` (false while off,
muted, unread or unavailable), `note`, `unavailable`, `curve` (61 points, 20 Hz to 20 kHz, the stage
as played now) and `ranges`. The field is absent from a daemon without loudness, and front ends
offer loudness only while it is there. `reference_now` sets the reference to the volume as last
read. A change while loudness is unavailable is accepted and saved, and takes effect when it can
follow again. `done` is as crossfeed's: a slider's drag moves the target and is not saved, the
release saves. No front end works out a number, a range or a curve itself.

The route line names the stage ("loudness on", after "host EQ on" and before "crossfeed on").

## What is not verified, and limits

- The fit to ISO 226:2003 is within 2.7 dB at the deepest depth; the 2023 edition differs slightly
  and is not used.
- Loudness follows the computer's sink volume only; an application's own volume slider never reaches
  it. A change made outside svolder is followed within a second.
- PipeWire clears a filter's state when its coefficients change, so each step of a glide leaves a
  short low tick (see [known-issues.md](../known-issues.md)); a listening test on music did not
  hear it.
- **A half-applied filter can play for one quantum.** PipeWire's filter chain applies a live control
  update one value at a time, and nothing stops the audio thread from playing in between. Rarely
  (measured about 1 in 6,000 risky updates on PipeWire 1.6.2; at ten glide ends a day, roughly once
  every year or two) one quantum, about 1/50 s at 1024 samples and 48 kHz (a smaller quantum makes
  it likelier), plays a half-applied filter. A small step is harmless (a loudness glide step: about
  +0.3 dB). These are loud:
  - the daemon stopping gracefully, a TUI with no daemon quitting, a reconfigure that halts the
    computer's side, or loudness turned off, while loudness plays at a depth: +31 to +43 dB in the
    bass measured, up to +50 dB worked out;
  - the end of a loudness glide back to flat: +29 dB in the bass measured;
  - adding or removing a computer EQ point, a live preset recall that changes the number of points,
    the EQ turned on or off live: an unstable filter or full scale measured;
  - dragging an existing point: up to about +17 dB worked out, not measured;
  - switching the virtual speakers or crossfeed: up to about 6 dB worked out.

  After a bad quantum the chain recovers by itself. Live edits of the computer EQ have had this
  since they began (0.35), so v0.36.7 has it. svolder cannot close it without giving up live updates
  or a much larger redesign; the fix belongs in PipeWire. The proof above and the lab hold for whole
  sections only; see [known-issues.md](../known-issues.md).
- The preamp lab (`make preamp-lab`,
  [host-control-preamp.md](host-control-preamp.md#the-preamp-lab)) runs the stage in a profile of
  its own, holds the stage alone to unity, and asks that loudness follows the volume;
  `make host-e2e` measures the stage on a private PipeWire.
