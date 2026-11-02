<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Equaliser and presets

The `eq` verbs that read and write the tone control, and the preset library they share with the TUI.
Back to the [usage index](README.md).

## eq

Inspect or change the tone control. The tone control has a mode (flat, eq, or loudness) and three or
five parametric bands. Beside `show`, `set` and `mode`, `eq` also holds the preset verbs: a preset
is a named tone curve — a mode, a loudness strength and every band's frequency, gain and Q — kept in
the configuration file and offered on any device whose tone control has the same number of bands.
A Loki Max has no tone control and keeps presets of its own: [`preset`](cli.md#preset) lists and
applies them.

```
Inspect or change the tone control

Usage: svolder eq [OPTIONS] <COMMAND>

Commands:
  show     Print the tone control's current mode, loudness and band settings
  set      Set one band's frequency, gain, or Q
  mode     Select the tone-control mode
  list     List the presets, built-ins first
  load     Recall a preset onto the device
  save     Save the device's current curve as a preset
  import   Import an Equalizer APO parametric EQ file, or an AutoEq headphone, as a preset
  search   Search AutoEq's headphone index
  rename   Rename a preset
  delete   Delete a preset
  flatten  Put every band back on its centre frequency and centre Q at 0 dB
  help     Print this message or the help of the given subcommand(s)

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

### eq show

Print the tone control's current mode, loudness, band settings, which preset (if any) is on, and,
while a band boosts in EQ mode, the clipping-risk peak.

```
Print the tone control's current mode, loudness and band settings

Usage: svolder eq show [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per reading, then the preset and the peak, instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq show
mode flat
loudness 0
preset custom
band 1  40.0 Hz  +0.0 dB  Q 4.30
band 2  160 Hz  +0.0 dB  Q 4.30
band 3  632 Hz  +0.0 dB  Q 4.30
band 4  2515 Hz  +0.0 dB  Q 4.30
band 5  10000 Hz  +0.0 dB  Q 4.30
```

A band's value has one wording: the frequency with ` Hz` (`40.4 Hz`), the gain with a sign and ` dB`
(`+1.45 dB`, `-0.20 dB`) and Q bare (`0.50`; a line of prose writes `Q 0.50`, as `eq show` does).
`eq show`, the `display` of a band's reading on the socket (so `status --json` and `watch` too) and
the web page's band table all read a band through that one function. The fake device reports its
tone control at rest — flat, no loudness, every band at its centre frequency, 0 dB and its centre Q
— so every band has a value to show; a band the device has not reported yet prints `?`.

A frequency below 100 Hz prints to the tenth of a hertz, where one of a band's steps is less than a
hertz (`40.0`, `40.4`), and from 100 Hz up in whole hertz, so two neighbouring steps never read the
same; the tone-control screen's band table and edit line write it the same way. `preset` names the
preset that is on: every built-in is in EQ mode, so none of them is on while the device rests in
Flat mode (a device's Flat *mode* is not the same thing as the Flat *preset*, which is EQ mode with
every band at 0 dB), hence `custom` here.

With band 1 boosted to +6 dB and the mode switched to EQ (`eq set 1 g 6`, `eq mode eq`), a `peak`
line follows the bands:

```
mode eq
loudness 0
preset custom
band 1  40.0 Hz  +6.0 dB  Q 4.30
band 2  160 Hz  +0.0 dB  Q 4.30
band 3  632 Hz  +0.0 dB  Q 4.30
band 4  2515 Hz  +0.0 dB  Q 4.30
band 5  10000 Hz  +0.0 dB  Q 4.30
peak +6.0 dB — keep the volume at or below -6.0 dB to avoid clipping on loud masters
```

When an imported preset is on, a `fit` line follows the `preset` line:
`fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz`.

With `--json`, each of the tone control's readings is one object, as `status --json` prints them,
and two objects follow: the preset that is on, `custom` where none is, and the peak in dB to the
tenth — `null` wherever the text prints no `peak` line. The fake device at rest ends:

```
$ svolder --transport fake eq show --json | tail -n 3
{"id":"12.52","key":null,"label":"Loudness","value_type":"u8","raw":0,"display":"0","value":null,"unit":null}
{"preset":"custom"}
{"peak":null}
```

and the boosted band above ends `{"preset":"custom"}` then `{"peak":6.0}`.

### eq set

Set one band's frequency, gain, or Q. Each band covers its own frequency span, and a value outside
that span is clamped to it, so the confirmed reading can differ from what you asked for.

```
Set one band's frequency, gain, or Q

Usage: svolder eq set [OPTIONS] <BAND> <PARAM> <VALUE>

Arguments:
  <BAND>   Band number, starting at 1
  <PARAM>  Which parameter: `f`/`frequency`, `g`/`gain`, or `q`
  <VALUE>  The new value, in Hz, dB, or Q

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq set 1 f 50
Status 12.1.2 u8 85   EQ band 1 frequency = 50 Hz
```

### eq mode

Select the tone-control mode.

```
Select the tone-control mode

Usage: svolder eq mode [OPTIONS] <MODE>

Arguments:
  <MODE>  `flat`, `eq`, or `loudness`

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object instead of text
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq mode eq
Status 12.50 u8 2   Tone control mode = eq
```

### eq list

List the presets — the built-ins first, each marked `built-in` — with its band count. No session of
its own is opened, so the listing comes at once whether or not a device is in range. A device's
profile adds `fits` to every preset whose band count matches it; the profile is the one the
configured device names under `profile`, else the one it was last read through (kept in the state
file when a direct session connects to it), or the daemon's when one answers; a daemon that answers
without a device leaves the profile to the configuration, as with no daemon; a name that is no
profile is said on stderr and leaves `fits` unmarked.

With such a device the text listing shows only the presets that fit its band count, the names padded
to line up, and ends with one line counting the rest, `4 more for 3 bands; --all lists them`;
`--all` lists every preset instead. With no device to fit, or with `--all`, the presets are grouped
under a `5 bands` and a `3 bands` heading. Only a daemon that holds the device adds `on`, to the one
preset, if any, whose mode and every band (and, for a Loudness preset, its strength) already match
the device's readings. Without a daemon nothing is marked `on`, and a text listing ends with
`svolder eq show names the preset that is on`; `--json` carries no such line, and `on` is `null`.
With `--transport daemon` and no daemon running, `eq list` still lists the library, from the
configuration alone. For a device with no tone control but presets of its own, such as a Loki Max,
the library is still listed, and standard error says
`svolder: this device has no tone control; svolder preset list lists its presets`.

An imported preset (see `eq import`) is marked `imported` in place of a band count, `fits` on every
device whose tone control has bands, and `on` when the device matches its fit; without a device it
is listed under an `imported` heading. `--json` rows of imported presets carry `"bands":0` and
`"imported":true`; the field is left out of every other row.

```
List the presets, built-ins first

Usage: svolder eq list [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --json                   Print one JSON object per preset instead of text
      --all                    List the presets that do not fit the device's band count too
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, with no daemon, a configured Gungnir 2 (five bands, `profile = "gungnir2-v2"`) and one
saved preset:

```
$ svolder --transport fake eq list
Flat        5 bands  built-in  fits
Warm        5 bands  built-in  fits
Bass        5 bands  built-in  fits
Presence    5 bands  built-in  fits
HD650 warm  5 bands  fits
4 more for 3 bands; --all lists them
svolder eq show names the preset that is on
```

With `--all` the same device lists everything under headings:

```
$ svolder --transport fake eq list --all
5 bands
  Flat        built-in  fits
  Warm        built-in  fits
  Bass        built-in  fits
  Presence    built-in  fits
  HD650 warm  fits
3 bands
  Flat        built-in
  Warm        built-in
  Bass        built-in
  Presence    built-in
svolder eq show names the preset that is on
```

Over a daemon holding that device, after `eq load "HD650 warm"`, the last preset row reads
`HD650 warm  5 bands  fits  on` and the line `svolder eq show names the preset that is on` is not
printed (the `N more for B bands` line, where a device fits, still is).

`--json` prints one object per line instead:
`{"name":"Flat","bands":5,"built_in":true,"fits":true,"on":false}`, with `fits` and `on` both `null`
where there is no device to mark them against, and `on` `null` wherever no daemon read the device.

### eq load

Recall a preset onto the device: the writes that bring the tone control to the preset's mode,
loudness and every band, skipping whatever already matches, in an order that keeps a boost from
sounding where it is not meant to: gains coming down first, then frequency and Q, then gains going
up, with the mode last for an EQ preset and first otherwise, a Loudness preset's strength just
before it. Names are matched without regard to case. Recalling an imported preset first prints its
fit line, `svolder: fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz`, then writes the fitted bands as any
preset's, with no clamp.

A preset that is on the computer (`host eq apply`) is asked about first:
`NAME is on the computer. Move it to the DAC? [y/N]`; a yes removes the computer EQ and loads the
preset, a no or an Enter changes nothing and exits 0 with `nothing changed`, and without a terminal
the verb refuses unless `--yes` answers for it. Every clamped value is reported on its own line,
then the number of writes sent; a failure partway through says how many of them went through before
it. A device with no tone control but presets of its own is refused, exit 1, with
`this device has no tone control; svolder preset list lists its presets`.

```
Recall a preset onto the device

Usage: svolder eq load [OPTIONS] <NAME>

Arguments:
  <NAME>  The preset's name

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --yes                    Answer yes to the question asked when the preset is on the computer
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, recalling a curve saved earlier onto a device that had since moved away from it:

```
$ svolder eq load "HD650 warm"
svolder: loaded HD650 warm (2 writes)
```

Loading a preset the device already carries writes nothing: `svolder: loaded HD650 warm (0 writes)`.

### eq save

Save the device's current curve — its mode, loudness strength and every band's frequency, gain and Q
— as a new preset. The name is kept without the blanks round it, so `" HD650 warm "` saves
`HD650 warm`; a blank name, one longer than 40 characters and one holding a control character — a
newline, a tab — are refused before the device is reached, and so is a built-in's name; a name a
user preset already carries is refused unless `--force` says to overwrite it.

```
Save the device's current curve as a preset

Usage: svolder eq save [OPTIONS] <NAME>

Arguments:
  <NAME>  The name to save it under

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --force                  Overwrite a user preset of that name
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq save "HD650 warm"
svolder: saved HD650 warm (5 bands) to /home/user/.config/svolder/config.toml
```

A built-in's name:

```
$ svolder --transport fake eq save Flat
svolder: "Flat" is a built-in preset's name; save this curve under another name. See `svolder --help`
```

A name holding a newline:

```
$ svolder --transport fake eq save "$(printf 'HD650\nwarm')"
svolder: a preset's name cannot hold a control character, such as a newline or a tab. See `svolder --help`
```

### eq import

Import an Equalizer APO parametric EQ file — the text AutoEq writes as
`<headphone> ParametricEQ.txt` — as a preset that fits every tone control, whatever its band count.
Configuration only: no device is opened. The file holds a `Preamp:` line and
`Filter n: ON PK|LSC|HSC Fc … Hz Gain … dB Q …` lines; a BOM, CRLF line ends, `OFF` filters,
comments and keywords in any case are accepted, a filter of another kind (a low-pass, a slope-given
shelf, a bandwidth in octaves) is skipped and counted, and a line that cannot be read refuses the
file with its line number.

The preset is named after the file without its extension and without a trailing ` ParametricEQ`,
unless `--name` says another; the name keeps the rules of `eq save`, a longer default is cut at 40
characters, and a name already in use is refused unless `--force` replaces that user preset. A
built-in's name is refused even with `--force`.

`--autoeq HEADPHONE` takes the correction from AutoEq's published results instead of a file (see "eq
search" below): the name is looked for in this order: the query as written (blanks round it
dropped), then ignoring case, so `Melomania 1` and `Melomania 1+` are each importable by their full
name, and names that differ only in case (`DT 770 Pro (32 Ohm)`, `(32 ohm)`) are one headphone whose
measurements are merged; then a name that is exactly the query compared as letters and digits, and
last the way `eq search` matches it.

`--measured-by` first narrows several candidates to the headphones that source measured; when a
headphone's name is exactly the query, or else exactly one headphone is left, the measurement of the
source `--measured-by SOURCE` names (in any case; the recommended one by default) is downloaded,
read like a file, and saved under the headphone's name, with the origin
`AutoEq · <source> · <form>[ · <rig>]`. When several headphones match, they are listed and the
command exits with the usage code, saving nothing. A download that fails (no network, a measurement
the index lists but the server lacks, a file over 64 KiB; `--measured-by` is suggested only when
another source measured the headphone) says what happened and names `svolder eq import FILE` as the
way out. Exactly one of `FILE` and `--autoeq` is given, and `--measured-by` goes with `--autoeq`
only.

```
Import an Equalizer APO parametric EQ file, or an AutoEq headphone, as a preset

Usage: svolder eq import [OPTIONS] <FILE|--autoeq <HEADPHONE>>

Arguments:
  [FILE]  An Equalizer APO parametric EQ text file, as AutoEq writes them

Options:
      --autoeq <HEADPHONE>     Download this headphone's correction from AutoEq instead of reading a file
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --measured-by <SOURCE>   With --autoeq, the source to take the measurement from (the recommended one by default)
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --name <NAME>            The name to save it under; the file's name, or the headphone's, by default
      --force                  Overwrite a user preset of that name
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

The command says what it saved, then, for each distinct tone control among the configured devices
(told from their profiles, with no connection), how well the filters fit its bands:

```
$ svolder eq import "Sennheiser HD 650 ParametricEQ.txt"
svolder: imported "Sennheiser HD 650" (10 filters, preamp -6.1 dB, 1 skipped)
svolder: Gungnir 2 · 5 bands: fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz · lower volume 3.5 dB
```

The last part of a fit line, `lower volume 3.5 dB`, appears when the fitted bands boost enough to
clip at full volume. The fit is by frequency, gain and Q on the device's own steps, between 20 Hz
and 10 kHz, so a correction with more filters than the tone control has bands is approximated; the
file's preamp (within ±30 dB, or the file is refused) is kept in the preset, and the volume is never
lowered for you. To hear the correction exactly instead, put it on the computer with
`svolder host eq apply` (see ["host"](cli.md#host)); the TUI's preset list does both (`a` to search,
`i` to import a file, `h` to apply on the computer).

#### What the DAC can and cannot do

The tone control has 3 or 5 peaking bands, each free only inside its own window of frequencies, with
gains of −20 to +6 dB in steps of about 0.2 dB and a Q of 0.5 to 8 in steps of about 0.06. It has no
shelf filters and no preamp. A correction with more filters than the DAC has bands, or with shelves,
is therefore approximated, and `eq import` says how well in its fit line; on the computer
(`host eq`) every filter and the preamp apply exactly.

| Tone control | Band windows (Hz) |
|---|---|
| 5 bands (Gungnir 2, Yggdrasil, Bifrost 3) | 20–78, 82–312, 321–1232, 1276–4904, 5075–19500 |
| 3 bands (Mimir, Eitr 2, meshcard) | 31–128, 305–1285, 3056–12850 |

A 3-band fit is coarse: no band can sit between 128 and 305 Hz or between 1.3 and 3 kHz.

For a file you prepare yourself, svolder skips the fit and maps it directly when it has no more
filters than the DAC has bands, every filter is a peaking one (`PK`), the filters' frequencies in
ascending order each fall inside the window of the band of the same position, and every gain and Q
is within the ranges above: one filter per band, snapped to the device's steps, with any spare band
left flat on its window's centre. Anything else is fitted.

### eq search

Look a headphone up in AutoEq's index of corrections (`results/INDEX.md` of the project's
repository, about 850 KB; AutoEq is a separate project whose published results svolder reads only
when you ask it to, and svolder ships none of its data). Configuration and network only: no device
is opened. Names and query words compare as lowercase letters and digits, so `hd650` finds `HD 650`
and `athm50x` finds `ATH-M50x`; every word of the query must appear in the name.

Headphones are ranked exact name first, then names that start with the query, then names where the
query's first word starts one of the name's words (`Sennheiser HD 600` before `Denon AH-D600` for
`hd 600`), then the rest, each tier alphabetically; within one headphone, AutoEq's own order holds,
so the first measurement is the recommended one.

```
Search AutoEq's headphone index

Usage: svolder eq search [OPTIONS] [QUERY]...

Arguments:
  [QUERY]...  The words of the headphone's name, in any case and with or without spaces

Options:
      --all                    One line per measurement instead of one per headphone
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --refresh                Fetch the index again before searching; with no query, only that
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --json                   Print one JSON object per line instead of text
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Each line is a headphone, the recommended measurement as `source · form[ · rig]`, and `+N more` when
AutoEq has others, with the names padded to one column; `--all` prints one line per measurement
instead. A text listing stops at 30 lines with `… N more: narrow the search`; `--json` prints one
object per line (`name`, `source`, `form`, `rig`, `more`) and is not capped. `--refresh` with no
query only fetches the index again. A `--refresh` that fails while a cached copy exists still prints
the results from the copy, says so on standard error, and exits 1.

```
$ svolder eq search hd650
Sennheiser HD 650  oratory1990 · over-ear  +6 more
```

The index is cached in `$XDG_CACHE_HOME/svolder/autoeq/INDEX.md`
(`~/.cache/svolder/autoeq/INDEX.md`), written atomically, and fetched again when the file is older
than `[autoeq] refresh_weeks` (1 to 99 weeks, or `"never"`; 13 by default) or when `--refresh` says
so. When a refresh fails and a cached copy exists, the copy is used and standard error says
`using the index from 2026-07-01 (refresh failed: …)`; with no copy and no network the command
refuses and names `svolder eq import FILE`.

Transfers have a 10 s connect timeout, a 60 s limit for the index (8 MiB at most) and 20 s for a
file (64 KiB at most), follow at most 5 redirects, send `User-Agent: svolder/<version>`, honour the
proxy environment, refuse a redirect from https to http, and verify TLS against the operating
system's certificate store. Text from the index only ever becomes percent-encoded components of a
URL, never a local path. `SVOLDER_AUTOEQ_URL` replaces the base address
(`https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/`), for a mirror.

### eq rename

Rename a preset. Configuration only — no device is opened, since a preset's name carries between
every device it fits. The new name is kept and refused as `eq save` keeps and refuses one, and a
built-in's name is refused on either side.

```
Rename a preset

Usage: svolder eq rename [OPTIONS] <OLD> <NEW>

Arguments:
  <OLD>  Its current name
  <NEW>  Its new name

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq rename "HD650 warm" HD650
svolder: renamed "HD650 warm" to "HD650" in /home/user/.config/svolder/config.toml
```

### eq delete

Delete a preset. Configuration only, like `rename`. Without `--yes`, a terminal is asked to confirm,
in the words `passphrase` asks its own question in; a non-terminal stdin without `--yes` refuses
outright, so a script cannot delete a preset by accident.

```
Delete a preset

Usage: svolder eq delete [OPTIONS] <NAME>

Arguments:
  <NAME>  The preset's name

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --yes                    Skip the confirmation prompt
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example:

```
$ svolder --transport fake eq delete HD650 --yes
svolder: removed preset "HD650" from /home/user/.config/svolder/config.toml
```

Off a terminal without `--yes`:

```
$ svolder --transport fake eq delete HD650
svolder: refusing to delete a preset without confirmation; pass --yes
```

### eq flatten

Put every band back on its centre frequency and centre Q at 0 dB, as the built-in Flat has them,
leaving the mode and the loudness strength alone — the CLI's `0`. Only the steps that differ from
what the device reads are written: the gains that come down first, then the frequencies and Q, then
the gains that go up. In Flat mode, the mode is written to EQ right after the bands, so the change
can be heard, and the notice says so; in Loudness mode the mode is left alone, since the bands do
not apply there yet. A curve already on Flat's values — every band at 0 dB on its centre frequency
and centre Q — writes nothing, the mode included, as the tone-control screen's `0` writes nothing
there.

```
Put every band back on its centre frequency and centre Q at 0 dB

Usage: svolder eq flatten [OPTIONS]

Options:
  -d, --device <DEVICE>        Device selector: a unit-id prefix or a product name. Optional with one device
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `daemon` (the daemon, failing when none can be reached), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
      --socket <SOCKET>        Daemon socket to use. Defaults to the runtime directory's `svolder.sock` [env: SVOLDER_SOCKET=]
  -v, --verbose                Print the backend choice — a daemon, or a direct transport — to stderr
  -h, --help                   Print help
```

Example, against the fake device, which rests in Flat mode with every band at 0 dB on its centre, so
there is nothing to flatten:

```
$ svolder --transport fake eq flatten
svolder: the curve is already flat (0 writes)
```

With band 1 at +3 dB in Flat mode (`eq set 1 g 3` on a daemon's session, which keeps the device
between calls), the gain comes back and the mode switches after it:

```
svolder: switched to EQ
svolder: flattened (2 writes)
```

## Presets

A preset is a named tone curve: a mode (flat, eq or loudness), a loudness strength and every band's
frequency, gain and Q, kept in physical units rather than raw device steps — so the same preset
converts correctly on a profile with different band centres, and recalls to the same curve after a
firmware or profile change. Presets are one library, not one per device: a preset carries a `bands`
count, 3 or 5, and is offered on every device whose tone control has that many bands.

Four built-ins sit at the head of every list, read-only — `eq save`, `eq rename` and `eq delete`,
and the preset list's `u`, `c` and `x`, all refuse their names, in any case, for a preset of your
own too. So does `custom`, which names a curve no preset matches: a preset the file already calls
`custom` still loads, never reads as on, and can be renamed or deleted.

A preset's name is at most 40 characters, holds no control character — no newline, no tab — and is
kept without the blanks round it: every writer, `eq save` and `eq rename` and the preset list's
prompt, drops those blanks and refuses a name left blank, and a name is looked up in any case and
with or without blanks round it, so `eq delete " hd650 WARM "` finds `HD650 warm` and `eq load warm`
loads the built-in Warm. A block typed into the file with blanks round its name loads, and is found,
by the name without them. Every built-in is in EQ mode with loudness 0, and, Flat aside, boosts
somewhere, so the clipping-risk zone below applies to each of them too. On a five-band tone control
such as a Gungnir 2's:

| Preset | What it shapes |
|---|---|
| **Flat** | every band at 0 dB, on its own centre frequency and centre Q |
| **Warm** | 40 Hz +3 dB, 160 Hz +1.5 dB and 10 kHz +2 dB, each Q 0.7; the rest at 0 dB |
| **Bass** | 40 Hz +4 dB and 160 Hz +2 dB, each Q 0.7; the rest at 0 dB |
| **Presence** | the 2515 Hz band moved to 3 kHz, +2.5 dB, Q 1.4; the rest at 0 dB |

A three-band tone control's built-ins shape its own three bands (63/630/6300 Hz): **Warm** 63 Hz +3
dB and 6300 Hz +2 dB, each Q 0.7; **Bass** 63 Hz +4 dB, Q 0.7; **Presence** the 6300 Hz band moved
to 3 kHz, +2.5 dB, Q 1.0; the rest at 0 dB.

Your own presets live in the configuration file next to `[[device]]`, as `[[preset]]` blocks the
same byte-preserving writers keep intact around whatever else the file holds (see
["Configuration file"](daemon.md#configuration-file) for the file, and
["config template"](config.md#config-template) for a commented example block with a name, band
count, mode, loudness and band array to type straight into). `svolder eq save` writes one from the
device's current curve — here, with band 1 at +3.1 dB, band 2 at +1.4 dB and band 5 at +2.1 dB, set
by `eq set` first:

```toml
[[preset]]
name = "HD650 warm"
bands = 5
mode = "eq"
loudness = 0
band = [
  { f = 40.0, g = 3.1, q = 4.3 },
  { f = 160.0, g = 1.4, q = 4.3 },
  { f = 632.0, g = 0.0, q = 4.3 },
  { f = 2515.0, g = 0.0, q = 4.3 },
  { f = 10000.0, g = 2.1, q = 4.3 },
]
```

An imported preset (`svolder eq import`) is a `[[preset]]` block that holds the correction's own
filters instead of bands, so it fits whatever tone control it is recalled on:

```toml
[[preset]]
name = "Sennheiser HD 650"
origin = "AutoEq · oratory1990 · over-ear"
preamp = -6.1
filter = [
  { type = "LSC", f = 105.0, g = 6.0, q = 0.7 },
  { type = "PK", f = 180.0, g = -2.1, q = 0.9 },
]
```

`origin` and `preamp` are optional, and `preamp` is within ±30 dB; `filter` holds 1 to 63 entries,
`type` is `PK`, `LSC` or `HSC` (read in any case, written in upper case), `f` is 10 Hz to 24 kHz,
`g` within ±30 dB and `q` above 0 up to 20. A block holds `filter` or `bands`, `mode`, `loudness`
and `band`, never both: `preset "X": an imported preset holds filter, not band`.

Recalling an imported preset fits its filters to the tone control's bands first (the fit is kept for
the run of the program), then writes the fitted bands in EQ mode at loudness 0; it is on when the
device matches that fit. Rename and delete work as for any preset. Updating it with the device's
curve (`eq save --force NAME`, the list's `u`) is refused,
`Sennheiser HD 650 is imported: save the curve under a new name`, so the correction it was fitted
from is never lost.

A whole-number value always writes as a float (`40.0`, never `40`), since the writer always writes a
band value as one; typing the file by hand, or uncommenting the template's own example (see
["config template"](config.md#config-template)), can still write `40` and have it read the same.

The CLI's own preset verbs sit under `eq` above: `eq list` (built-ins first, with `fits` and which
one is `on`), `eq load NAME`, `eq save NAME [--force]`, `eq import FILE [--name NAME] [--force]`,
`eq search QUERY…`, `eq import --autoeq HEADPHONE [--measured-by SOURCE]` (see "eq import" and "eq
search" above), `eq rename OLD NEW`, `eq delete NAME [--yes]` and `eq flatten`, alongside
`eq show`'s `preset` and `peak` lines; the computer's own EQ is `svolder host eq` (see
["host eq apply"](cli.md#host-eq-apply)) — see "eq list" through "eq flatten" above for each one's
help text and examples.

The TUI reaches the same library through `p`, on the tone-control screen and on the device screen's
Tone pane; ["The preset list (`p`)"](tui.md#the-preset-list-p) covers its keys (`↑↓` `Enter` `n`
`u` `c` `x` `a` `i` `h` `Esc`), the name prompt and the `[y/N]` questions it shares in wording with
`eq delete`.

Loading a preset, from either front end, sends the same sequence: the gains that go down first, then
every frequency and Q change, then the gains that go up, and the mode last when it becomes EQ — so
the curve is complete before the device applies it — or first when it becomes Flat or Loudness, with
the loudness strength written alongside the mode and only for a preset in Loudness mode. Only what
differs from the device's current readings is written at all, so recalling the curve already on the
device sends nothing. A value outside this device's band window or range is clamped to the nearest
step and reported on its own line, `band 5 frequency 22000 Hz clamped to 19497 Hz`; the recall
carries on past a clamp, but stops at the first write that fails outright and says how many went
through.

A preset is **on** — the tone-control screen's header reading `Preset NAME` and the device screen's
Tone pane naming it, `eq show`'s `preset` line naming it, `eq list` marking it `on` — when the
device's mode is the preset's own and every band reads the step the preset's value converts to on
this device and, only for a preset in Loudness mode, when the loudness strength matches too, since
the strength has no effect in EQ or Flat mode. A band at 0 dB shapes nothing, so a band whose gain
is the preset's 0 dB matches whatever its frequency and Q: after `eq load Warm` and `eq flatten` (or
`0` in the TUI) the curve is Flat's in every step, since a flatten puts every band back on its
centre frequency and centre Q too, and `Flat` is the preset that is on.

A band with any other gain must match in frequency and Q too. So a curve with every gain at 0 dB in
EQ mode reads as Flat whatever its frequencies and Q, and a preset saved from such a curve —
`eq save` after Warm and then `eq set` of its three boosted gains to 0 dB, say — is on together with
Flat: `eq show` names Flat, `eq list` marks both `on`, and the TUI header keeps the name that was
loaded while it still matches. Loading a preset still writes every step that differs, a zero-gain
band's frequency and Q included.

After a change made from a loaded preset, the header reads `NAME*` until the curve matches it again
or another preset is loaded; with nothing matching and nothing loaded it reads `Preset custom`. The
daemon keeps one memory per device of the preset last recalled, by any front end — `eq load`, the
TUI, MQTT, MCP or the web page — since it started, and never writes it to the file: the TUI's header
and preset list follow a recall another front end made (`NAME*` and `Preset custom` as above), and a
recall the TUI makes itself tells the daemon its name, so the other front ends show it as the one
that is on. A session with no daemon (`--transport ble` or `fake`) keeps the name for that session
only.

Three more pieces of the same work are documented where the screen that draws them is, rather than
here: the clipping-risk zone the volume bar shows while a curve boosts in EQ mode — the red zone,
the warn-then-alert tick at its threshold step, and why svolder never lowers the volume itself —
under ["Device screen"](tui.md#device-screen); the tone control's own Flat *mode* (`f`, the device's
bypass, bands kept but not applied) against a *flat curve* (`0`, or the Flat preset, which also
centres every frequency and Q) and what the Response graph draws in each of the tone control's three
modes, including the Loudness note — a contour that tracks the DAC's own volume, doing nothing at
full volume and not drawn until it is measured on a real device — under
["What the graph draws"](tui.md#what-the-graph-draws); and typing a band's exact frequency, gain and
Q under ["The edit line (`Enter`)"](tui.md#the-edit-line-enter).
