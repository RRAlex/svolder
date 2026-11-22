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
  lie outside 20 Hz to 20 kHz, so a shelf preset gets a deeper preamp cut than that window gives: a
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
note says which point and at which rate. Nothing is sent at the new rate. The note goes at the next
successful apply. A chain that plays flat (crossfeed or the virtual speakers alone, or a
pass-through) plays none of the points, so they are not checked and it goes on, an apply of it (a
set chosen there) included; turning the EQ on at a rate they cannot play at is refused, and the
state keeps the preamp they were saved with. The rate is forgotten with the chain, so an apply with
none there reads the default output's rate (`pw-dump`) and checks and writes at it; when it cannot
be read it is the default 48 kHz and the first look at the new chain checks it at the rate the sink
runs at.
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
  anything is sent to it. The preamp it holds then bounds every preamp sent until an apply, or a
  state file read at start or on a reload, says what it was started from: the bound suits the slots
  the chain starts from. After a reload the bound is taken again from the node at the next look. A
  preamp at the -120 dB floor bounds nothing: such a chain plays nothing (an older svolder could
  leave one so), and it is sent the preamp the view says. A graph that has not run (nothing has
  played through the chain yet) lists every control at 0, which all of a slot's `b0` at 0 gives
  away, and it starts from its config when it runs, dropping what was sent before; so a look at it
  bounds nothing, checks no send and makes no foreign resend, and the first look that finds it run
  takes the bound and sends everything again (every slot, the stage and the preamp). Up to one watch
  period (1 s) of the saved config plays first.
- The session sends only to the node the controller looked at, told by its id and `object.serial`
  (PipeWire gives a freed id to a later object, never a serial). `pw-cli` does not refuse a line to
  a node that replaced the old one under its id, so the session reads the node again before every
  line: `pw-dump` of that object alone (about 13 ms and 9 ms of CPU on a private graph of 550
  objects, where the whole graph's dump takes about 105 ms and 99 ms), given at most 250 ms. That
  is one read a line, two when `pw-cli` is started for it and twice that when the line is tried
  again, so one send holds the controller at most a second on these reads. When the node it finds
  is another one, the line is not written; the controller looks at that node and sends once more.
- The config on disk is written with the preamp it needs at its own rate, so a chain that starts
  again from it is capped by that number and not by the preamp the points state. A state file that
  the command line wrote is checked at the running rate and sent whole on the first look after the
  daemon reads it, whatever the rate. No config is written whose points do not play at its rate
  (the EQ waiting for a rate its curve plays at, or points read from a state file the first look
  has not checked yet): it would hold coefficients the chain clamps to other filters, or an
  unstable one, and a chain that starts again would play them.

The price is plain: after a curve is softened live, the level stays at the applied curve's preamp
until the next apply. What applies is turning the EQ off and on, loading a preset softer than the
applied curve while nothing plays, an edit made while the EQ is off, an edit when live updates are
not available, and a restart of the chain (which loads the config on disk). A hand edit that
softens the curve, a point's new type, and a point added or removed all go live and do not apply.
The node bound has the same price: a user whose chain was sent to by a command-line apply before
the daemon's reload plays quieter than the points need, until the next apply.

## A softer preset while a stream plays

An apply restarts the chain: its sink is removed and made again, and a player that follows the sink
(a pulse client such as a desktop music player) may pause on the removal. So a preset that needs
more preamp than the cap allows, loaded while a stream plays through the chain, is not applied: it
goes live like a drag, the curve changes at once and the preamp stays at the cap (a flat preset
after one that needed -6.2 dB plays 6.2 dB quieter than it should, never over unity). The view's
note says `level held until the music pauses`. The state file and the config on disk keep the curve
with its own preamp. Once nothing has played through the chain for the idle time (30 s, the
pass-through's), the chain is applied again with the points as they stand, and the cap lifts; a
stream that plays at that check keeps the level held another period. With nothing playing the preset
applies at once, as before. A preset that needs less preamp than the cap goes live and holds nothing
(and a chain this controller applied, read back at the preamp its config holds, bounds nothing: the
read-back is f32 at six decimals, a hair off); the EQ turned off while the level is held keeps the
chain as a pass-through for the stream (off with nothing playing removes it, and the next on applies
the curve whole); a daemon that starts over a chain left holding a lower preamp than the saved state
needs (it stopped while the level was held) plays under that preamp and lifts it the same way once
idle. Rate changes, crossfeed and the speakers while the level is held go by the rules above.

## The crossfeed stage

The crossfeed stage sits after the EQ, and the preamp falls by what the stage could add at its worst
frequency, at any usual rate, since a line sent before the next look after a rate change plays the
stage worked out for the old rate (the preamp rule counts both). For Bauer's default that is about
0.04 dB at 100 Hz in a mono tone; for M/S bass at 50 % it is about 3.1 dB, because at the 120 Hz
crossover an anti-phase signal adds `|1 - a·LP/2| + |a·LP/2|`, about 1.41. So M/S bass plays about
3.1 dB under the input at every frequency: a tone at 2 kHz is unchanged apart from that preamp.
`make host-e2e` measures these levels on a private PipeWire.

A rate change puts the stage back to the one the chain's config holds, with the slots, and a line
carries the stage only when it moved, so a drag's line sent before the next look plays the points
as they stand under the config's stage. The preamp is therefore also never above what the points
need less what the config's stage costs. It bites only when the points need more preamp than the
config's curve and the config holds a dearer stage than the one sent live: a config holds the
stage only when its live send failed or live updates are off, and the stage sent live since is
cheaper (Meier with a 1 ms delay in the config, then its amount turned down to 7.5 %, then a
point dragged up: without the rule, +0.49 dB over unity). The level is then held down by the
difference, so the chain is applied again once nothing has played for the idle time, as after a
held recall, which takes the stage out of the config. A state file that says its config holds
the stage does not say the rate the stage was worked out at, so it is paid for at whichever usual
rate costs the most (Meier at 0.25 ms worked out at 44.1 kHz costs 0.37 dB more than at 48 kHz)
until the reset it owes takes the stage out. The lab's `stage_cost_table` (an ignored test,
`cargo test --release -p svolder-host --lib stage_cost_table -- --ignored --nocapture`) prints,
for every model's presets with and without a delay, the most the stage adds against its cost over
every rate it may be worked out at and played at.

## A line that changes points and a stage

A live line can land in torn quanta, and PipeWire plays a half-landed line, so a line that changes
the EQ's points and also a crossfeed or speakers stage that costs less (a crossfeed turned off or
made cheaper, crossfeed to speakers) could play over unity in between: the new points on the old,
dearer stage's preamp. A drag released while a crossfeed drag was still pending did it, up to +2.0
dB in the lab. So `send_dirty_once` puts the stage's ports before the slots in such a line, and
while the node may still hold the loaded config's slots (after a sink rate change nobody has seen)
and the new preamp would not pay for the new stage over them, it holds the preamp low for both
orders (`stage_hold`, in `cap_at`): the line goes live at the held level with `LEVEL_HELD_NOTE` and
the chain is applied again once nothing plays. With no stream it is one apply. A line that changes
no slot keeps the slots-first order, which is safe in every world, with no hold and no note. Each
recall holds for its own line only, and a recall that is refused restores the hold it found.

A stage the node plays and the controller has no record of (after a command-line apply replaced the
node under a stage) has an unknown cost, so no hold bound exists. The controller takes it to the
bypass in a line of its own before a line that changes slots: one extra live line, rare, costing
nothing and leaving the next line the single-kind order. It fires only for a stage the config does
not hold, since a config-held stage is paid for by `loaded_stage_db` on every line. The same rules
serve a scene recall ([scenes.md](scenes.md)), which is one such line; loudness does not count as a
slot change, since it is cut-only at every boundary.

## The loudness stage

The loudness stage ([loudness.md](loudness.md)) sits after the EQ's sixteenth slot and costs the
preamp nothing, and it is outside every preamp sum. Its f32 maximum over DC to Nyquist is at most
0 dB at every rate and every depth, it never raises any frequency, and its two sections in a
configuration are the identity, so:

- the preamp's value is never changed by loudness: not by the stage's depth, not by a volume
  change, and a loudness line carries no preamp of its own;
- the cap a configuration puts on the live preamp (`cap_at`, the configuration's own preamp), the
  total a recall wants (`wanted_total`), what a held level is capped at, the cost of the stage a
  configuration holds (`loaded_stage_db`, `config_stage_cost_db`), the test for lifting a held level
  and the preamp `save_state` keeps all leave the stage out. In particular a configuration's
  identity sections are unity, so the level rule of a recall and the held level's cap are the ones
  of a chain with no loudness stage, and a chain read back at its configuration's preamp bounds
  nothing;
- no order between a loudness line and any other line is needed for the level, and a rate change
  between a stage worked out and played gives the same maximum, because the maximum over the unit
  circle does not depend on the rate.

The stage has an order of its own inside its line, because the sections of one line may land in any
order and a line half-landed at a section boundary must stay under unity
([the proof](loudness.md#why-it-is-never-louder), the order in
[following it](loudness.md#following-it)). The proof holds for whole sections: see
[the limit below](#what-the-lab-cannot-see).

## A chain from an older svolder

A chain an older svolder built (a typed `param_eq`, with no preamp control) may still play after an
upgrade. The view says so in its note, shows the preamp it was applied with, and nothing is sent to
it and nothing written at start. Turning the EQ off and on, an edit that ends or a preset load
replaces it. A node listed before its controls are is taken for one at first and looked at again
until they appear.

## The preamp lab

`make preamp-lab` is the safeguard for all of the above: the controller driven through seeded random
sequences of events against a model of what PipeWire plays (`eq/lab.rs`). The model holds the config
on disk, the node (its id and serial, the config it started from, every port's value as an f32,
coefficients held to ±10), the sink's rate, a `pw-cli` that can die, the state file and a clock. It
takes from PipeWire what was measured on a private instance: a rate change puts the slots back to
the config the node started from and leaves the preamp as last set; a restart is a new node from the
config on disk, under a new serial and a new id, and every block of seeds runs a second time with
every other start given the old id, as PipeWire may (`SVOLDER_LAB_REUSE_IDS=1` picks that model for
a single sequence, such as `lab_trace`); a `set-param` to an id that is not the node's does nothing;
a line is applied whole or not at all. The events are hand edits (drags and releases), points added
and removed, preset loads harder and softer than the loaded curve, the EQ on and off, command-line
applies with the state file written before or after the daemon's look, state reloads, node restarts,
rate changes among 44.1 to 192 kHz, `pw-cli` deaths (also mid-line), refused sends, a write that
fails once, refusals the session reads zero to two reads late (its reader task sets the flag a
moment after `pw-cli` prints it), a failed Off or apply, nodes that are listed late, daemon
restarts, a stream that starts or stops playing through the chain, and ticks of 50 ms (the lab's
controllers idle after 60 ticks, so a sequence sees a held level lifted). A rate event with the rate
the sink already runs at models a rebuild of the graph, which puts the slots back without a change
of rate.

After every event and every tick the curve the node plays (its ports, as the model holds them)
times the preamp it plays must peak at most 0.01 dB over unity over the whole band at the current
rate, measured by an evaluator of its own, and each biquad must be stable. Also: no `set-param` to
a stale id after the controller's own apply, nothing written or sent while the daemon starts or
reads the state file (an event or a tick's reload), and no panic.

The rule that nothing plays over unity says nothing about whether the EQ still works, so a second
one asks after three seconds with no event, once, while nothing is armed to fail (a refused send,
a node not listed yet, a `pw-cli` that a refused send ended and that may not be started again for
up to a second) and the controller can tell what the node plays. A `pw-cli` that ended after
taking a line is started again at the next look, within a second, and the newest values are sent.
A graph rebuilt at the rate the controller last read puts every slot and the stage back and keeps
the preamp. A port known to have taken its sent value that a look finds at another value is a
revert, and everything is sent again. A port is known to have taken it when a look found it there,
or when a look saw a port move to the value of the same line or a later one: `pw-cli` applies each
line whole and in order, so a line whose preamp a look saw land put its slots in too. A line that
left the preamp where it was leaves nothing that survives a rebuild, so a look cannot tell a rebuild
before it from a `pw-cli` that has not applied the line yet: a port that a look still finds off its
value although its line went before the look before is sent again, once for that line (the newest
values, not counted as a revert). So the slots are put right within about two seconds of one or two
rebuilds. A third rebuild that lands between that resend and the next look leaves the slot at the
config's value until the next edit, under unity, while the view says live (a known limit). A daemon
that starts over a node it did not start compares its slots and preamp with the saved points at the
first look and, where they differ, sends the saved state through the normal update (preamp first
when it falls, never above the node's own preamp unless that is the -120 dB floor): the saved file
wins. A controller that says it is on and live must have a node that plays the points it shows,
at the rate the sink runs at, under the preamp it shows, within 0.05 dB at every frequency of a
grid. One that is off, with a curve that plays at the sink's rate, must turn on when asked.

The short run is six blocks of 300 seeds. The first draws every kind of event with its usual weight.
The second, the targeted profile, takes nearly half of its events from a command-line apply, a node
that starts again, a refusal read late, a write that fails once and drags: the compound that can
leave a chain replaced while a line is refused or lost. Against a copy that sends a line to a node
other than the one it looked at, it fails 132 of 5000 seeds where the first block fails 58. The
third, the speakers profile, makes one event in eight a change of the virtual speakers (a set
chosen, on, off): the model's controller has a store of two sets, the model renders the speakers'
stage in a config with a set and reads its mixers back, the unity rule adds what those mixers make
of a set scaled to -0.1 dB, and the liveness rule asks that the speakers play as the view says. The
fourth, the high-rate profile, starts the sink at any of the usual rates or 705.6 or 768 kHz and
spends about a third of its events on rate changes among them, a quarter on crossfeed changes (drags
in flight and their ends) and the rest on ticks, drags of the points and the ordinary mix: against
the controller before the stage was priced at the rate it was worked out at, it fails 2 of 3000
seeds (+1.9 and +2.4 dB at 768 kHz). The fifth, the loudness profile, spends about 40 % of its
events on the computer's volume (a key's step, a jump to 0 or 100 %, from inside svolder or from
outside), 5 % on mutes, 13 % on loudness changes (on, off, strength, reference as a number or as the
volume now, drags and releases), 8 % on rate changes up to 768 kHz, 2 % on daemon takeovers (half of
them with a first line that does not land) and 12 % on ticks. The model holds the loudness stage's
ports, the unity rule counts the stage and holds the stage alone to unity, and a liveness rule asks
that the played depth follows the volume. The sixth, the scene profile (`generate_scene`, its own
stream, so no older profile's seeds move), mixes scene recalls with random parts (the curve,
loudness, crossfeed and speakers, playing or idle) among the faults of `event_for`, under both id
models, and the unity rule is asked after every event and every tick. The lab also checks every line
boundary for every profile, and the whole rule at the end of every landed line; the repair line
(`Outbox::line`, sent after a `pw-cli` died with a line unseen) carries every port in the caller's
order and the rest after, so its preamp placement is not the controller's and the check skips it by
name (`repair_owed`; `SVOLDER_LAB_REPAIR_LINES=1` makes the check see it).

Each profile draws from a stream of its own, so the first block's seeds do not depend on the others.
Every sequence ends with a quiet tail: the faults armed and not used are disarmed, the daemon reads
the state file and the world is left alone for 90 ticks, so the liveness rule is asked of the state
the sequence ended in. A failing seed prints the shrunk sequence; `SVOLDER_LAB_SEED=<seed> cargo
test -p svolder-host --lib lab_trace -- --ignored --nocapture` prints what happened step by step.
The short run is part of `make check`; `make preamp-lab SEED=<first> RUNS=<count>` runs the long one
(5000 sequences of each profile by default).

## What the lab cannot see

The lab, the unity rule and the proofs above work at the granularity of whole filter updates and
sections: a line's sections land in some order, and every state made of whole sections stays under
unity. They do not see a tear inside one section.

PipeWire's filter chain applies a live control update one value at a time, and nothing stops the
audio thread from playing in between. Rarely (measured about 1 in 6,000 risky updates on PipeWire
1.6.2; at ten glide ends a day, roughly once every year or two) one quantum, about 1/50 s at 1024
samples and 48 kHz (a smaller quantum makes it likelier), plays a half-applied filter. A small step
is harmless (a loudness glide step: about +0.3 dB). These are loud:

- the daemon stopping gracefully, a TUI with no daemon quitting, a reconfigure that halts the
  computer's side, or loudness turned off, while loudness plays at a depth: +31 to +43 dB in the
  bass measured, up to +50 dB worked out;
- the end of a loudness glide back to flat: +29 dB in the bass measured;
- adding or removing a computer EQ point, a live preset recall that changes the number of points,
  the EQ turned on or off live: an unstable filter or full scale measured;
- dragging an existing point: up to about +17 dB worked out, not measured;
- switching the virtual speakers or crossfeed: up to about 6 dB worked out.

After a bad quantum the chain recovers by itself. Live edits of the computer EQ have had this since
they began (0.35), so v0.36.7 has it. svolder cannot close it without giving up live updates or a
much larger redesign; the fix belongs in PipeWire. See [known-issues.md](../known-issues.md).
