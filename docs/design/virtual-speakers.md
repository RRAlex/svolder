<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The computer's virtual speakers

Virtual speakers make the computer's sound on headphones seem to come from two speakers in front of
you, at about 30 degrees to either side (45 for a With room · BRIR set). There are two choices, each
always shown with its technical term: **With room · BRIR** (the speakers heard in a measured room)
and **No room · HRTF** (the same speakers in the open). svolder runs them as one fixed stage inside
the computer EQ's filter chain, at crossfeed's place. This page is the graph, how a measured set
becomes four impulse responses, the catalogue and the store, the lifecycle that keeps the stage from
being left on, the state and the contract. What a user sees and does is in
[eq.md](../usage/eq.md#virtual-speakers); the chain it sits in is in
[host-control.md](host-control.md), the sibling stage in [crossfeed.md](crossfeed.md). The code is
the crate `crates/svolder-speakers` (pure, no PipeWire; only its `net` feature reaches a network),
`crates/svolder-host/src/eq/speakers.rs` (what a set is, the oracle, the view), `eq/live.rs` (the
graph) and `eq/control/vs.rs` (the controller).

## The graph

The stage sits after crossfeed's mixers `lmix` and `rmix`, before the graph's outputs. For each ear
there is one mixer and for each of the four paths one `convolver` of PipeWire's builtin plugin
(`vLL`, `vLR`, `vRL`, `vRR`: the speaker, then the ear it is heard in), reading channel 0 to 3 of
one 4-channel WAV:

```text
lmix:Out --+--> lvs:In 1 (dry)         left ear  = LL · L + RL · R
           +--> vLL --> lvs:In 2
           +--> vLR --> rvs:In 2
rmix:Out --+--> rvs:In 1 (dry)         right ear = LR · L + RR · R
           +--> vRL --> lvs:In 3
           +--> vRR --> rvs:In 3     outputs: lvs, rvs
```

The mixers `lvs` and `rvs` are the switch: **off** is `Gain 1` 1 and `Gain 2`, `Gain 3` 0, **on** is
0, 1, 1. A mixer skips an input whose gain is 0, so off is a bit-exact bypass (proven by a null
test in `make host-e2e`). The convolvers' `config` is the WAV's absolute path, its `channel` (always
given) and `gain` 1; trim, cap and level are done at import, so `offset`, `length` and `delay` are
left 0, and the plugin's block and tail sizes are its defaults. A WAV at another rate is resampled
to the graph's rate when the chain is built, so one file serves every rate; a sink rate change
rebuilds the chain anyway, which resamples again (and adds about one source sample of delay,
which a test must not expect to the sample). The convolver adds no latency of its own.

The convolvers are in the config only while a set is chosen and the speakers are on (see "Rebuild
and the config" below). The EQ, crossfeed and this stage are linear and the EQ is the same on both
channels, so their order changes headroom only. No `sofa` plugin is used and no extra PipeWire
package is needed.

## One space choice

Crossfeed and the speakers are one choice, Off, Crossfeed or Virtual speakers: both feed one ear the
other speaker's sound, and stacking them would cross twice. The wire keeps two ops
(`HostEqOp::Crossfeed`, `HostEqOp::Speakers`); the controller turns the other one off in the same
update, so the Home Assistant crossfeed switch and `set_host_crossfeed` work as before and
the crossfeed switch goes OFF when the speakers go on. The speakers are global, like the EQ, and
reset on crossfeed's rules.

## From a file to four impulse responses

The crate turns any source into `Converted`: four impulse responses `LL, LR, RL, RR` at one rate,
the sidecar's numbers and the curve. All of it is pure and runs once, at fetch or import.

**Sources.**

- A **SOFA** file, read once with `sofar` (its `hdf` module, default features off, so no resampler
  and no FFT crate come with it). Only the shape every catalogue set has is read: `Data.IR` as
  measurements by two receivers (the ears, receiver 0 left) by samples, one `SourcePosition` per
  measurement, spherical (azimuth counter-clockwise from the front, elevation 0 on the horizon).
  A free-field set (one head, sources all around) and a room set (one speaker position per
  measurement) go through the same choice. The file's own rate (`Data.SamplingRate`) is kept: the
  sound server's resampler changes rate later, never a linear interpolation here. `Data.Delay` is
  applied as whole-sample shifts. A file whose HDF5 the parser refuses is refused in words.
- A **WAV**: four channels as `LL, LR, RL, RR`; fourteen channels by the common 7.1 layout (channels
  0, 1, 8, 7); a pair of two-channel files (the left speaker's, the right speaker's) through
  `convert_wav_pair`. PCM and float WAVs are read; any other channel count is refused.
- A **zip** member (a set that only comes in an archive), read by the `zip` crate, deflate only.

**The two speakers.** The measurement nearest the left speaker's azimuth (30 degrees, or 45 for the
room set, whose grid has no 30) and the right speaker's (330, or 315) on the horizon, within 10
degrees unless the recipe says otherwise; among those within half a degree of the nearest, the
one nearest 1.5 m. The angles and distance found are kept in the sidecar and shown ("left 30.0
deg, right 330.0 deg, 1.2 m"). `LL`, `LR` are the left speaker's two receivers, `RL`, `RR` the right
speaker's.

**What `prepare` does to the four** (`ir.rs`), in this order:

1. **Lead-in.** The earliest onset of the four (the first sample at a tenth of its path's peak, -20
   dB) is given exactly 1 ms in front of it, the same shift on all four, so the time and level
   differences between the ears stay: samples are taken off when there are more, zeros put in when
   there are fewer (`trimmed`, `padded` in the report). A measurement's time of flight (about 4 ms
   at 1.5 m) goes with it. The 1 ms keeps the resampler's pre-ringing at other sink rates.
2. **Length.** At most 0.5 s, with a 10 ms half-cosine fade at the end; a longer room set is cut and
   the view says "cut to 0.5 s". The length hardly changes the CPU (partitioned convolution).
3. **Level.** Every set is scaled so its worst gain `max over f of max(|LL|+|RL|, |LR|+|RR|)` is
   -0.1 dB, so the stage never boosts. The worst gain is searched over the whole band, 0 Hz to the
   Nyquist rate, on a 16-times zero-padded FFT of its own (the true worst is within 0.04 dB above
   -0.1), after rounding to the f32 that plays. The rule is per frequency, never a claim about the
   samples' peaks. No preamp is folded into the EQ for it.
4. **Numbers for a screen.** The loudness change against Off, the power of a centred pink-weighted
   signal in dB ("about 7 dB quieter", from a 1/48-octave grid, 20 Hz to 20 kHz), the length, and
   the curve: the four paths' magnitudes in dB at 61 points, 20 per decade from 20 Hz to 20 kHz.

A non-finite sample, a silent set, a rate outside 8 to 384 kHz or a source over 64 MiB is refused.
A panic inside the SOFA parser is caught and becomes a refusal. The parser inflates chunked data
without a total cap, which is why an import runs as a job (see known-issues).

**Timing** (`analysis.rs`). For the Arrivals and Decay pictures `analyse` works out, once per chosen
set, when each ear is reached with both speakers playing the same signal: arrivals in ms after the
earliest direct one and dB below the loudest (at most 12 reflections per ear within 50 ms and 30 dB;
a reflection within 1.5 ms of a stronger one, or of the direct sound, is not listed apart), and with
room the 50 ms envelope after the direct sound; and the decay in three bands (below 250 Hz, 250 Hz
to 4 kHz, above), log-spaced from 0.1 ms to the length with up to 100 points (fewer at low rates),
floored at -60 dB. A set with no room has direct arrivals only. The result is `SpeakersView.timing`;
it is `None` from an older daemon.

## The WAV and the sidecar

One float32 4-channel WAV (an own reader and writer, about 160 lines) and a `<id>.toml` sidecar: id,
room (`with` or `no`), label, licence (an SPDX id, `LicenseRef-` for words that have none), credit
words, source page, SHA-256 of the source and of the WAV, rate, taps, angles found, loudness change,
the scale applied, the worst gain, `cut`, `own`, and when it was added and last used. They live in
the state home's speakers folder next to `host-eq.toml`: they are state the chain depends on, not a
cache. The source download is deleted after conversion. A free-field set is about 4 to 9 KB, the
room set about 0.23 MB, and an own file at 192 or 384 kHz 1.5 to 3 MB.

The store holds at most 32 sets; past that the oldest unused one goes (a fetch's answer says which).
An id is 1 to 48 of lower-case letters, digits and dashes. Writes go through a temporary file and a
rename under one gate, so a fetch and an import never share a file. `Store::check(id)` reads the
sidecar, the WAV's header and its checksum: a missing or damaged set is "missing or damaged, fetch
it again". **A set in use is pinned:** `Store::set_in_use(&[ids])` names every set a running chain
was started with (and the one about to replace it), and `forget` and the eviction refuse or spare
those, because a convolver whose file cannot be opened does not stop the chain: it stays alive with
its graph never started, so all sound stops, dry included, with no exit code. A sink rate change
re-opens the file, which is why the pin lasts as long as the chain. Own sets are named `own-<8 hex
of the source's SHA-256>`. `list_sidecars` lists without reading the WAVs (the controller's
once-in-10-s refresh); `list` checks them all.

## The catalogue and the fetch

The catalogue is a table in code (`catalogue.rs`), not a live index. Each entry pins one URL with
its size and SHA-256, so a source that changes upstream is a refused checksum, never a silent other
file. Only sets whose licence allows any use are listed; a set under another licence can be imported
from a file the person has. The entries:

| id | Choice | Label | Licence | Credit words | Size |
|---|---|---|---|---|---|
| `mit-normal` | No room · HRTF | MIT, normal pinna | LicenseRef-MIT-Media-Lab-free | Gardner and Martin (1995) | 1.2 MB |
| `sadie-d2` | No room · HRTF | SADIE II, D2 | Apache-2.0 | SADIE II, University of York | 36.6 MB |
| `hutubs-pp1` | No room · HRTF | HUTUBS, subject pp1 | CC-BY-4.0 | HUTUBS, Audio Communication Group, TU Berlin | 1.7 MB |
| `ari-nh2` | No room · HRTF | ARI, subject NH2 | CC-BY-SA-3.0 | Acoustics Research Institute, Austrian Academy of Sciences | 3.0 MB |
| `cipic-003` | No room · HRTF | CIPIC, subject 003 | LicenseRef-CIPIC-any-purpose | The Regents of the University of California, CIPIC Interface Laboratory | 1.8 MB |
| `sadie-d2-brir` | With room · BRIR | SADIE II, D2 room | Apache-2.0 | SADIE II, University of York | 8.4 MB |

Labels name the dataset and the subject, never a head's or speaker's maker. The room set's speakers
are at 45 and 315 degrees, 1.5 m. The licence line (SPDX id, the credit words where the licence asks
for them, the source page) is stored with the set and shown wherever a set is chosen or shown. A
ShareAlike licence binds files derived from it: the converted WAV stays on the person's disk and
svolder redistributes nothing.

**Fetch** is the daemon's alone (`make network-check` keeps the HTTP client out of `svolder`): the
AutoEq client's pattern, `ureq` and `rustls` behind the `net` feature, system certificates. Only the
listed URLs are asked for, over HTTPS, 10 s to connect and 120 s in all, at most 64 MiB, streamed
to a temporary file in the speakers directory, checked against the entry's SHA-256, converted and
deleted whatever happens. Redirects are followed by hand, at most 5, each only to the entry's host
or one the catalogue lists. `SVOLDER_SPEAKERS_URL` replaces the scheme and host of the listed
addresses (the path stays), so a test serves the files from a loopback server; the address
`127.0.0.1:9` turns fetching off and the daemon says so in words that begin
`SPEAKERS_OFF_PREFIX`. Cancel is seen between reads.
`make speakers-catalogue-check` (network, not in `make check`) fetches each entry and checks size,
checksum and that the converter accepts it; run it before a release. Fixtures are synthetic: an
h5py-written SOFA from `make speakers-fixtures`, and WAVs the tests write; no dataset is committed.

## The controller

`EqControl` owns the speakers (`eq/control/vs.rs`), so the daemon and a TUI with no daemon behave
alike.

**Choose rebuilds while they play, on rebuilds when the chain lacks the set, off is live.** The WAV
path is the convolvers' config, so choosing or changing the set of a chain that plays them applies
the chain again, once (one short gap, as a rate change). A set chosen while the speakers are off is
only kept, so no chain loads a WAV that is not playing; the first on after it applies the chain
once (a flat chain when the EQ is off). On with the chain already holding the set, and off, are the
mixers' gains, sent live. Turning on with no set chosen, or choosing a
set that is not fetched, is refused in words, and a refusal leaves everything as it was.

**Rebuild and the config.** The config on disk holds the convolvers only while the speakers are
on, always with the stage off (the live sends turn it on), so a chain that starts from it loads a
WAV only after a session that crashed with them on. Turning off, or any reset, takes the stage out
live and then rewrites the config without the convolvers, with no restart: the running chain keeps
four idle convolvers (about 0.2 to 0.3 % of a core) until it next restarts. `speakers_on` in the
state makes the next owner start rewrite the config; a failed rewrite is tried again every second.

**The check before every apply.** The WAV is checked before any apply that names it (a choice, a
rate change, a restart). If it fails, the chain is applied without the stage, the choice goes
Off with the note "virtual speakers off: <set> is missing or damaged, fetch it again" and the reason
`set-missing`, and the list shows the set as not fetched. A rebuild that fails is tried again every
second. The controller never writes a WAV path it has not just verified; with live updates
unavailable the chain is not watched, so a WAV deleted by hand is noticed at the next apply only.

**Resets to off,** each with a note and a reason in the view, as crossfeed's: the owner (daemon or
TUI) starts and finds them on (`restart`); the output they were turned on for is not the output any
more (`output-changed`); the chain restarted (`chain-restarted`); a graceful stop (`stopped`); the
set is missing (`set-missing`). The choice is kept, only the switch goes off. A chain that runs for
the speakers alone (the EQ flat) is removed by every reset; any other has the stage turned off live.
A rate change is no reset: the stage is sent again, since the rebuilt graph starts from its config.
Above 384 kHz, the highest rate a stage runs at, the speakers turn off with a note and turning them
on is refused, as crossfeed is.
The same tests as crossfeed's hold: off after the owner starts, the output changes, the chain
restarts and a graceful stop; both EQ and the space choice off means no chain runs; two chains never
stack; the config never holds the stage on; a WAV that fails the check never reaches a config.

**Preamp.** The set is already scaled to never boost, so the speakers take no part in the EQ's
preamp and the preamp lab needs no speakers event.

## State and contract

`host-eq.toml` gains `speakers` (the chosen set's id, kept while off), `speakers_on` (crash
recovery) and `speakers_loaded` (the set the running chain holds, so the next owner pins it too,
kept while another is chosen with the speakers off). `flat` is reused: the chain may run for the
space alone with the EQ flat.

`HostEqOp::Speakers { on?, set?, done }`: every field is optional and an op that names nothing is
refused; `on` is applied last, so turning on plays the set sent with it. `EqView.speakers:
Option<SpeakersView>` is present only where the owner serves the speakers; every field has a default
so an older daemon's view parses: `on`, `set`, `room`, `room_name`, `term`, `label`, `licence`,
`credit`, `source_url`, `angles`, `taps_ms`, `loudness_db`, `cut`, `note`, `off_reason`,
`unavailable`, `curve` (the four paths at 61 points), `timing` and `sets` (the fetched sets with the
same words). The choice's words ("With room", "No room", "BRIR", "HRTF") are made in
`eq/speakers.rs` and nowhere else: a front end shows what the view carries.

The sets are a family of requests of their own, `Request::Speakers { op }`: `catalogue` (every
catalogue set and every stored one, fetched or not, `fetching`, `max_sets`), `fetch { id }` and
`import { path, label?, room? }` (jobs: cancellable, with progress, followed with `follow`, the
result a `SpeakersSaved` naming what was evicted), `import_dir`, `forget { id }` (refused `busy`
while the set plays or a job runs) and `cancel`. A fetch and an import share one slot: one at a
time, a second is refused busy. The daemon holds one `Arc<Store>` and gives it to the host service,
so the controller and the sets service see the same pins. Web routes `POST api/v1/host/speakers`
(the op) and `api/v1/speakers/{catalogue,fetch,import,forget}` are for signed-in writers only, as
AutoEq's: a guest never makes the daemon reach the internet ([web-ui.md](../usage/web-ui.md)). The
CLI is [`svolder host speakers`](../usage/cli.md#host-speakers), the MCP tools `get_host` and
[`set_host_speakers`](../usage/mcp.md#the-tools) (no fetch), Home Assistant a switch and a select of
the fetched sets, the TUI the Space choice and the set list; the web Sound popup's third tab is in
[popups.md](popups.md). The signal path line names the stage (`virtual speakers on`), taken from the
EQ view.

## The oracle and the proofs

`eq::speakers_response(irs, rate, hz)` gives the four complex gains of a stored WAV at a frequency,
the way the chain plays it (the engine ignores the resampler's delay). The controller's tests and
`make host-e2e` use it: the example `vs_expect` writes a synthetic 4-channel room (a Dirac per
path with its gain and lag, plus a decaying noise tail) and prints the oracle's gains for a WAV. The
phases V0 to V11 are in [host-e2e.md](host-e2e.md).
