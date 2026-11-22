<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The PipeWire config check

What the route reads of the PipeWire config, and the seven-line check built on it. The route and
the path line are in [host-audio.md](host-audio.md#the-route); this page is the config facts and
the check. The check judges the PipeWire config itself, whatever plays: whether a single player
reaching the DAC through this config would come out untouched.

## The facts

- **The config facts** (`RouteFacts.setup`, a `SetupFacts` that every route of this build carries
  and an older daemon's lacks) come out of the same one dump, with no further process. They are the
  sink's `EnumFormat` offer (formats with the default first, rates as one, a list or a range,
  channels), a format or an allowed-rates list set on the sink's node, the `settings` metadata
  (`clock.rate`, `clock.allowed-rates`, `clock.force-rate`, where 0 reads as none), the negotiated
  layouts of the sink and of the player's stream, the sink converter's dither (`dither.method`,
  `dither.noise`), the `audioconvert.filter-graph` of the sink and of the player's stream when one
  is set and not disabled, and the ALSA hop. A missing fact stays missing: the judges read it as
  unknown, never as clean.
- **The ALSA hop** is read only for a sink whose `device.api` is `alsa`. Its device name is the
  node's `api.alsa.path`, else the follower's `device` prop (`hw:0,0`, `plug:...`, `dmix:...`). Its
  card is `api.alsa.pcm.card`; its device number is `api.alsa.pcm.device`, else `alsa.device` (real
  sinks carry only the latter). What the kernel says of the card is read from `/proc/asound`
  (`SVOLDER_ASOUND_DIR` names another directory, for tests): `cardN/stream{D}` for the formats and
  rates the card takes (the PCM index `D`, `stream0` when the card has only one file), and
  `cardN/pcmDp/sub{S}/hw_params` for what an open card was handed (`S` from the node's
  `alsa.subdevice`, else 0). Reading these files starts no process. A `Rates: a - b (continuous)`
  line adds no rate to the card's list, so a continuous range reads as unknown rather than a list.

## The check

The path line judges the sound that is playing. The config check judges the PipeWire config
itself, whatever plays: whether a single player reaching the DAC through this config would come out
untouched, as seven pass, fail or unknown lines. Both are pure functions of one `RouteFacts` in
`svolder-ipc` (`host/path.rs` and `host/setup.rs`), and every front end shows the words it is
given and judges nothing: `computer_line` and `path_line` word the path, `setup_check` the config.

The seven lines, in their fixed order (the JSON key first, then the name the lists show):

| Key | Name | Fails when |
|---|---|---|
| `rates` | rates | a rate the DAC takes (the kernel's list, else the sink's offer) is not in the allowed rates, so PipeWire resamples it |
| `forced_rate` | forced rate | `clock.force-rate` is set, since every other rate is then resampled |
| `dither` | dither | the sink converter's dither method is not `none`, or its noise is not 0 |
| `format` | 24-bit output | the format in use (the negotiated one, else the forced one, else the offer's default) is narrower than 24 bits |
| `plug` | no plug | the ALSA device is `plug` (converts) or `dmix` (mixes), or the card's `hw_params` show a conversion |
| `stereo` | stereo to stereo | the sink has other than two channels |
| `filter` | no filter in the sink | an `audioconvert` filter graph runs inside the sink |

A line that could not be read is `unknown`, never `pass`. A line's words say what was found
(`44.1-192 kHz all allowed`, `S16LE, S32LE available`, `a filter inside the sink`). The decisions
behind them: an open card's `hw_params` beat the device name, except that `dmix` fails whatever the
card shows (other ALSA clients can mix in) and `plug` stays a failure when the card's format cannot
be read; the profile names Analog and Digital are never a verdict; rates above 384 kHz are neither
judged nor listed; mono into stereo is not a change; a forced rate fails its own line always, while
the path line names it only when it causes a resample (`resampled 44.1 → 48 kHz (rate forced)`,
whenever the force differs from the stream's rate, and `forced rate not read` when the clock
settings could not be read). The `24-bit output` line judges the sink's format, not the stream's, so
an S16LE sink fails it even when a 16-bit stream through it is exact.

The summary is one of five (`SetupSummary::KEYS`, the keys JSON carries): `untouched` ("PipeWire
config lets it through untouched"), `changes` ("PipeWire config changes the sound: N", N lines
fail), `unread` ("PipeWire config not fully read: N", no line fails and N are unknown), `not_read`
("PipeWire config not read": no sink) and `other_version` ("PipeWire config not read (svolderd's
version differs)": a route with a sink and no config facts this build reads, from an older or a
newer svolderd). The Path row draws it as a `config N` tag only while N lines fail; a check that
passes, is not fully read or is not read shows nothing on the row (the `k` box and the web
popup keep every line). The path verdict's keys (`Verdict::KEYS`) are `untouched`, `changed`,
`not_this_dac`, `other_input` and `unknown`; `end_to_end` takes the same five words and adds
the DAC's own stage to the verdict (`PathLine::end_to_end`); a line's
outcome keys (`SetupOutcome::KEYS`) are `pass`, `fail` and `unknown`. Every JSON surface (the web
card, MCP, `svolder host check --json`, Home Assistant) uses these keys; `words` is an array
everywhere, and `config.limit` carries `SETUP_LIMIT`, the sentence of what the check cannot judge:
svolder judges the config from the player's stream on; it cannot see inside a player, nor past the
sink's converter, and judges at most 24 bits; it only reads.

A route that was not read has one word per surface because the surfaces differ in what they can
hold: `unknown` on the web card (it has no `not_read` verdict), `not_read` on the Home Assistant
path sensor, and `path: null` in MCP `get_host`; `svolder host check` has no such state, since it
reads the graph itself or exits 6.

What the check proves is bounded by the e2e: a private PipeWire shows that the stream converter,
the one-input mixer, the sink volume at 1.0 and the sink converter with dither off are
transparent for 16- and 24-bit samples, and that each of the faults above breaks that
([host-e2e.md](host-e2e.md) phases B and C). It does not prove what ALSA does past the converter
(`hw` hands the bytes on, `plug` converts), the USB driver or the DAC.

Where it shows: `svolder host check` and `svolder doctor`'s PipeWire config row, the TUI's Path row
tag and its `k` box, the web page's tag and popup, MCP `get_host.config`, and the Home Assistant
`computer_config` problem sensor. The judge gives `not_read` only for a route with no sink; which
sink is judged is each surface's choice. The CLI and MCP judge whatever sink the computer's default
is. The web page, the TUI and Home Assistant judge it only when that sink is a configured DAC (the
first published device of the DAC family whose name matches the sink): for an amp or another sink
the web card and the TUI show no tag, and Home Assistant's sensor has no state. A DAC that has not
connected since svolderd started is not recognised as the sink's DAC until it does, so those
surfaces judge nothing meanwhile, never a false pass.

**The Path row's label.** One model, `svolder_ipc::host::path_row`, builds the row from the path
line and the config check of its sink, and every front end draws it as it is: none re-judges,
diffs two texts or picks a colour. The label is green (`good`) only when `end_to_end` is
`untouched` and the config check does not fail, yellow (`warn`) when `end_to_end` is `changed` or
the check fails, and plain for what could not be read, another sink and another input. A failing
check outranks a path that could not be read (the label is yellow), and a known change at the
DAC outranks something not read (`end_to_end` reads `changed`). Yellow words come from the judge's
change list, never from comparing two texts: `S16LE` into `S32LE` stays plain. The values line
says a resample as `player rate → sink rate` (`44.1 → 48 kHz`), and the forced rate only when the
sink still reads the stream's rate. A row with no hops joins the words of a port mismatch with
`; ` in its label's words. The row serializes as `{label: {tone, words}, hops: [{label, words:
[{text, tone}]}], values: [{text, tone}], words}`, a tone being `plain`, `dim`, `good` or `warn`
and `words` the one plain line of a row with no hops. `verdict` keeps its meaning on every JSON
surface; `end_to_end` is beside it in the web page's model (`playing.path`, with the row as
`playing.path.row`), MCP `get_host.path` and `svolder host check --json`. Home Assistant has no
`end_to_end` in 0.38.1.

**One rule for the path verdict of a sink that is not a DAC.** The CLI (`host check`) and MCP
judge the computer's side for any sink: `path.dac` is null and the verdict may read `untouched`
up to that sink (`computer_line`). The TUI, the web and Home Assistant judge against svolder's
DACs only: a sink that is none of them reads `not_this_dac` and is named (`path_line` with the
DACs' facts, or none). The "same keys" of the JSON surfaces are the same keys, not always the same
verdict for the same route. The config's own rule is the one above.

## Reads

The route is read from one `pw-dump`, while somebody holds a `Watcher` or for one request. A fast
watcher (a page, a joined TUI) reads every 2 s, and a fast hold that joins reads at once only when
the last read is more than 2 s old; a slow one, which only publishes (the MQTT client while it is
connected and the computer's entities are published), reads every 30 s, and slow watchers count for
neither the volume follower nor the volume keeper. A one-shot `host` request (`svolder host check`,
`svolder doctor`, MCP `get_host`, a script's `host`) reads the route anew within 1 s unless a fast
watcher already holds one; requests that arrive while such a read runs share it (one `pw-dump` for
all of them), and one that arrives after it ended reads again. A stalled PipeWire gives no route and
nothing else is lost. `svolder host check` with `--player`, or with no daemon, reads the graph
itself, once. With none of these there is no read. A test greps the files that read the route
(`route.rs`, `route/**`, `path.rs`, `setup.rs`, `sample.rs`, `host_check.rs` and the TUI's
`run/setup.rs`) for `pw-metadata`, `pw-cli set`, `wpctl set-` and config writes: the check only
reads.

## Fakes

The fakes of host audio at large live here for want of room in [host-audio.md](host-audio.md):
`crates/svolder-host/src/fake.rs` holds a stand-in for each source that answers from what a test
gave it and records what it was asked; a clone shares its original's state, so a test keeps one and
hands the other to the code under test. This is what lets the TUI's buffer tests cover every state,
including each part missing. `FakePlayers::answering` makes the players answer a press as a real
player would, which the daemon's fake host and its tests use.

`SVOLDER_FAKE_ROUTE=changed` gives the fake route a 16-bit sink under a 32-bit stream and
triangular dither besides its other changes, and `SVOLDER_FAKE_CONFIG=fail` fails the fake route's
`rates`, `forced rate` and `filter` lines, so a look can see a failing check. Unset, every line
passes.
