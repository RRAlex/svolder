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
No device is opened. The file holds a `Preamp:` line and
`Filter n: ON PK|LSC|HSC Fc … Hz Gain … dB Q …` lines; a BOM, CRLF line ends, `OFF` filters,
comments and keywords in any case are accepted, a filter of another kind (a low-pass, a slope-given
shelf, a bandwidth in octaves) is skipped and counted, and a line that cannot be read refuses the
file with its line number.

While a daemon reads the same configuration file it is the one writer, so `eq import FILE` (like
`eq save`, `eq rename` and `eq delete`) reads and parses the file itself and asks the daemon to
save the preset (a daemon older than 0.34.0 is refused with `svolder daemon restart`); with no
daemon, or one that reads another file, the command writes the file itself. `--transport ble` and
`fake` never ask a daemon to write, so they write the file too, and then ask a daemon that reads
the same file to reload it, as every transport does.

While the daemon says the file it reads is gone, `eq save` and `eq import` are refused in its words
(`PATH is missing: the daemon keeps its running configuration; restore the file, then try again`,
exit 1), as a write would start a new file from the template and the reload after it would drop
every device.

The preset is named after the file without its extension and without a trailing ` ParametricEQ`,
unless `--name` says another; the name keeps the rules of `eq save`, a longer default is cut at 40
characters, and a name already in use is refused unless `--force` replaces that user preset. A
built-in's name is refused even with `--force`.

`--autoeq HEADPHONE` takes the correction from AutoEq's published results instead of a file (see "eq
search" below). Only the daemon reaches AutoEq: the command asks it to search, then to download and
save, so it needs `svolderd` running. With none, it says `AutoEq needs svolderd, which fetches it:
no daemon answers on SOCKET` and names `svolder daemon restart`; a daemon older than this svolder
says to restart it and exits 7; `--transport ble` and `fake` open no daemon, so they refuse it too.
Importing a file needs no network and works either way.

The name is looked for among the headphones the daemon's search finds, in this order: the one with
a measurement named as the query is written (blanks round it dropped), then ignoring case, so
`Melomania 1` and `Melomania 1+` are each importable by their full name; then the one whose name
reads the same as the query compared as letters and digits; and last every headphone the search
matches. Names that differ only in case (`DT 770 Pro (32 Ohm)`, `(32 ohm)`) are one headphone whose
measurements are merged; a name that differs otherwise (`Melomania 1`, `Melomania 1+`, or `Koss UR
20`, `UR20`, which `eq search` lists as one headphone) is a headphone of its own, and a query that
is not exactly one spelling lists them and asks for the full name.

`--measured-by` first narrows several candidates to the headphones that source measured; when a
headphone's name is exactly the query, or else exactly one headphone is left, the measurement of the
source `--measured-by SOURCE` names (in any case; the recommended one by default) is downloaded by
the daemon, read like a file, and saved under the headphone's name, with the origin
`AutoEq · <source> · <form>[ · <rig>]`. When several headphones match, they are listed and the
command exits with the usage code, saving nothing. A download that fails (no network, a measurement
the index lists but the server lacks, a file over 64 KiB; `--measured-by` is suggested only when
another source measured the headphone) says what happened, in the daemon's words, and names
`svolder eq import FILE` as the way out. The preset is saved for the library, which no side owns:
the daemon is told the computer as the side, so a correction of more than 16 filters is refused.
Exactly one of `FILE` and `--autoeq` is given, and `--measured-by` goes with `--autoeq` only.
Ctrl-C or a closed terminal during `--autoeq` cancels the daemon's job and leaves with 130.

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
`i` to import a file, and `Enter` on the computer's side to apply on the computer).

#### What the DAC can and cannot do

The tone control has 3 or 5 peaking bands, each free only inside its own window of frequencies, with
gains of −20 to +6 dB in steps of about 0.2 dB and a Q of 0.5 to 8 in steps of about 0.06. It has no
shelf filters and no preamp. A correction with more filters than the DAC has bands, or with shelves,
is therefore approximated, and `eq import` says how well in its fit line; on the computer
(`host eq`) every filter and the preamp apply exactly, up to 16 of them (a longer correction is
refused there with its count).

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
when you ask it to, and svolder ships none of its data). The daemon does the searching and the
fetching, so the command needs `svolderd` running: with none it says `AutoEq needs svolderd, which
fetches it: no daemon answers on SOCKET` and names `svolder daemon restart`, and `--transport ble`
and `fake`, which open no daemon, refuse it. No device is opened. Names and query words compare as
lowercase letters and digits, so `hd650` finds `HD 650` and `athm50x` finds `ATH-M50x`; every word
of the query must appear in the name. A query is at most 60 characters.

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
instead. Names that read the same as letters and digits are one headphone, whoever measured it. A
text listing stops at 30 lines with `… N more: narrow the search`; `--json` prints one object per
line (`name`, `source`, `form`, `rig`, `more`) and is capped only by the daemon, which answers
with at most 200 headphones (the text line says how many more there were). `--refresh` with no
query only fetches the index again, and says how many measurements it lists. A `--refresh` that
fails while a cached copy exists still prints the results from the copy, says so on standard
error, and exits 1.

```
$ svolder eq search hd650
Sennheiser HD 650  oratory1990 · over-ear  +6 more
```

The index is the daemon's: it caches it in `$XDG_CACHE_HOME/svolder/autoeq/INDEX.md` of the
daemon's own user (`~/.cache/svolder/autoeq/INDEX.md`), written atomically, and fetches it again
when the file is older than `[autoeq] refresh_weeks` (1 to 99 weeks, or `"never"`; 13 by default)
or when `--refresh` says so. The daemon reads `refresh_weeks` from its configuration at each
request, so a change takes effect with the next search; `SVOLDER_AUTOEQ_URL` and the cache
directory are read once, when the daemon starts, so a mirror needs a restart. When a refresh fails
and a cached copy exists, the copy is used and standard error says `using the index from
2026-07-01 (refresh failed: …)`; with no copy and no network the command refuses and names
`svolder eq import FILE`. Under the fake transport with no `SVOLDER_AUTOEQ_URL`, search and import
say `AutoEq is off for the fake device; set SVOLDER_AUTOEQ_URL to turn it on` instead.

The daemon's transfers have a 10 s connect timeout, a 60 s limit for the index (8 MiB at most) and
20 s for a file (64 KiB at most), follow at most 5 redirects, send `User-Agent: svolder/<version>`,
honour the proxy environment, refuse a redirect from https to http, and verify TLS against the
operating system's certificate store. Text from the index only ever becomes percent-encoded
components of a URL, never a local path. `SVOLDER_AUTOEQ_URL` replaces the base address
(`https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/`), for a mirror.

The same service answers the TUI's `a` search and the web page's AutoEq library (search, preview,
import): `svolderd` is the one process that reaches the internet, and the `svolder` binary links no
HTTP client. In a container the cache is `/config/cache/svolder/autoeq`, in the `/config` volume
(the image sets `XDG_CACHE_HOME=/config/cache`), and the image carries `ca-certificates` to verify
TLS.

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
`u` `c` `x` `a` `i` `Esc`), the name prompt and the `[y/N]` questions it shares in wording with
`eq delete`.

**The computer's side.** The web page and the TUI flip the tone control to the computer with one
switch; there the same presets, built-ins of the shown device and saved ones, play on the computer
EQ as written, the computer being able to hold any of them that has 16 filters or fewer. A preset
that was saved from the computer's points loads on a device by being fitted to its bands, as an
import is, and the fit line says how well. A preset that a device plays too is never put on both:
the TUI and the web page ask (`NAME is on the <device> too: flatten the <device> and apply on the computer?`)
and MCP asks for `confirm`; the CLI's `host eq apply` asks the same way. The points, the preamp and
the live editing are in ["The computer's side"](tui.md#the-computers-side-c) of the TUI reference and
["The switch to this computer"](web-ui.md#the-page) of the web page.

A preset loaded on the computer while music plays through the computer EQ never restarts its
chain, which would take the sink from under the player (some players pause then): it plays at
once. One that needs more headroom than the curve the chain started with plays at the earlier,
lower level, and the computer side says `level held until the music pauses`; about 30 s after the
music stops, the chain starts again with the preset at its own level. With nothing playing a preset
plays at its own level at once. Why the level is held:
[`host-control-preamp.md`](../design/host-control-preamp.md#a-softer-preset-while-a-stream-plays).

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

## Loudness

Loudness is a setting of the computer's sound, beside the computer EQ. Below full volume the ear
hears the bass and the top of the band less than the middle; loudness gives that back by taking the
middle and the top down as the computer's volume falls, so the bass stays where the volume left it.
It only cuts: it never raises any frequency and never moves the EQ's preamp, so turning it on can
make the sound quieter and never louder or clipped, apart from a rare PipeWire behaviour after a
live update (see [known issues](../known-issues.md)). It follows the computer's own volume (the
Volume row of the computer's side: the sink's volume, and the chain's while the chain is in the
path), not the volume of a device and not an application's own slider. It works from the TUI (the
computer's Tone pane and the tone screen), the web page's Sound… popup,
[`svolder host loudness`](cli.md#host-loudness), the MCP server's `set_host_loudness` and Home
Assistant; all of them change the same one setting, kept by the daemon (or by a TUI with no daemon),
so the last change wins. They offer it only while the daemon serves loudness; an older daemon has
none. The maths is in [the design page](../design/loudness.md).

The **computer's** loudness and a **device's own** Loudness (a mode of the device's tone control)
are different things. The screens keep them apart by place, the computer's side and the device's
side, and while both are on they show a dim "the DAC's loudness is on too" hint; nothing blocks
either.

- **Strength** is how much of the loudness curve plays, 0 to 100 % in steps of 5 (default 50 %).
- **Reference** is the volume at which loudness is flat, 1 to 100 % of the volume scale (default 100
  %: flat only at full volume). Lower the volume under it and the stage deepens, down to 40 dB below
  it. At the reference and above it plays nothing, and a volume boosted past 100 % is flat too. "Use
  the current volume" (`r` in the TUI, a button on the web page, `reference now` on the command
  line, `reference: "now"` for MCP) sets the reference to the volume as it is now, which is what to
  press after setting the volume you listen at most.
- **The readout** says what the stage plays at the volume it read: the depth below the reference in
  dB, the bass (how far it stands above the middle), the mids (how far the middle sits under the
  bass) and the treble (how far the top stands above the middle). At 35 % with the reference at 100
  % and strength 50 %, that is a depth of 13.7 dB, bass +7.3, mids −7.3, treble +2.1. These are the
  shelf gains of the stage, not the response at three frequencies. The mids fall faster than the
  volume knob does (1.27 times as fast at 50 %); that is the volume cut taken in the middle.

### On, off, and following the volume

Loudness has its own switch, apart from the EQ's and crossfeed's, and it plays with each on or off.
A step of the computer's volume made through svolder is followed on the next 50 ms tick, one made
elsewhere (a desktop key) within a second; the stage glides about 10 dB of depth a second, so a
jump in volume is not a jump in tone. Muted, or with the volume not read, it holds where it is, and
the view says it is not following.

Turned on with the EQ off, the chain plays flat for loudness alone. Turning the EQ off with loudness
on keeps the chain, and so does turning crossfeed or the virtual speakers off; with the EQ,
loudness, crossfeed and the virtual speakers all off no chain runs. Loudness plays at every rate,
705.6 and 768 kHz included (crossfeed and the speakers stop at 384 kHz).

Unlike crossfeed, loudness **stays on** across restarts (except from settings kept before any chain
was applied, which come back off): it can only make the sound quieter, so nothing needs turning off.
A graceful stop of the daemon (or a TUI with no daemon quitting) sends the stage flat and keeps a
chain that ran for loudness alone, playing flat until the next daemon or TUI starts and follows the
volume again. A crash leaves the last stage until then (quieter, never louder). Where loudness
cannot follow the volume (a chain from an older svolder, a chain held for a sample rate its curve
cannot play at, or a PipeWire that does not take live updates) it says it is unavailable and why
instead of rebuilding the chain on every step. A change made meanwhile, from any surface, is still
accepted and saved, and takes effect when loudness can follow again. A chain built before the
loudness stage is rebuilt once when loudness is turned on.

### The saved state

The computer EQ's state file (`~/.local/state/svolder/host-eq.toml`) keeps loudness as a table
beside the crossfeed's:

```toml
[loudness]
on = true
strength_pct = 50.0
reference_pct = 100.0
```

`on` is saved and survives a restart; settings kept in `host-eq-settings.toml` (no chain was ever
applied) come back with loudness off. The settings are kept across applies, and the chain's own
configuration always holds the stage flat: the volume-dependent stage is only ever sent live.
Settings changed before any chain was applied wait in `host-eq-settings.toml`, as crossfeed's do.

## Crossfeed

Crossfeed is a setting of the computer's sound, beside the computer EQ: it feeds a low-passed,
delayed copy of each channel into the other channel at a set level. Use it with headphones on the
computer's output; it does nothing for the DAC's own tone control, which it never touches. It
works from the TUI (`C`, then the tone screen `e` or the device screen's Tone pane), the web
page's Sound… popup, [`svolder host crossfeed`](cli.md#host-crossfeed), the MCP server's
`set_host_crossfeed` and Home Assistant; all of them change the same one setting, kept by the
daemon (or by a TUI with no daemon), so the last change wins.

### The four models

- - **Bauer** (the default) is the stereophonic-to-binaural crossfeed of the bs2b library. The other
  channel gets a low-passed copy of the sound, and the direct channel's highs are lifted a little to
  keep the whole level. It has three presets, named for the people whose settings they are, listed
  from the mildest to the strongest; choosing Bauer loads bs2b default.
- **Linkwitz** is a first-order low-pass cross path: the low frequencies of each channel are added
  to the other channel, and the direct sound is lowered by the same share.
- **Meier** crosses only what differs between the channels (the "side" signal, low-passed), so
  a sound in the centre stays untouched.
- **Mid/Side bass** blends the bass below the crossover toward mono and leaves the centre and the
  highs as they are. At 50 % the bass is fully mono. It adds no delay.

### Presets, Level, Crossover and Delay

| Model | Presets (crossover, level) | Crossover range | Level range | Delay |
| --- | --- | --- | --- | --- |
| Bauer | Jan Meier (650 Hz, 25 %), Chu Moy (700 Hz, 33 %), bs2b default (700 Hz, 37 %) | 300–2000 Hz, 10 Hz steps | 15–47 %, 1 % steps | own lag up to 1.00 ms, 0.01 ms steps |
| Linkwitz | Default (700 Hz, 30 %) | 300–2000 Hz, 10 Hz steps | 5–50 %, 1 % steps | own lag up to 1.00 ms |
| Meier | Default (650 Hz, 25 %), Rockbox (660 Hz, 24 %) | 300–2000 Hz, 10 Hz steps | 5–50 %, 1 % steps | own lag up to 1.00 ms |
| Mid/Side bass | Default (120 Hz, 50 %) | 40–500 Hz, 10 Hz steps | 5–50 %, 1 % steps | none |

- **Level** is the share of the other channel that is added at low frequencies, from 0 to
  50 % (50 % is mono). It is shown in percent and in dB (`37 % · −8.6 dB`, the level of the
  crossed sound against the input, `20·log10` of the share) by the TUI, the web popup and
  `host crossfeed` alike. The Bauer presets are defined by bs2b's own figure, the feed level
  (4.5 dB for the default: how far the crossed sound sits below the direct one); only
  `--json` and the MCP server carry it, as `feed_db`. More percent is more crossfeed.
- **Crossover** is where the crossed sound is cut: only what is lower than it crosses over
  (higher in Mid/Side bass is a wider blend).
- - **Delay** is how far the crossed copy is delayed, in ms. Every model has a lag of its own that
  follows the crossover (about 0.26 ms for Bauer at 700 Hz); the delay shown is the total, never
  below that, and `own` means no more than the model's own. It plays in whole samples, rounded down
  (21 µs at 48 kHz). Mid/Side bass has none to set (its low pass lags by itself, about 1.9 ms at 120
  Hz, which nothing changes).
- A **preset** loads its exact numbers. Changing the model loads the new model's default preset;
  picking the model that is already chosen changes nothing. Changing a Level, Crossover or Delay
  makes the values **Custom**, and the screens say what they were edited from (`Custom, edited from
  Chu Moy`). Every number is rounded to its step and held to its range, and a screen shows what
  was kept.

### On, off, and the level

Crossfeed has its own switch, apart from the computer EQ's. It plays with the EQ on or off: with the
EQ off and crossfeed on, the chain plays flat (every filter the identity) with the crossfeed after
it. With both off no chain runs, and turning one on starts it; but while a stream plays, where live
updates work, the chain stays as a flat pass-through (0 dB, or the lower level its loaded curve
needs, a boosted preset keeping its cut) so the player is not paused, and goes once nothing has
played through it for about 30 s; a graceful stop removes it at once, and turning either back on in
that time reuses it.

A chain that dies (killed, crashed) is noticed at once: the daemon starts it again with crossfeed
off and the note "the computer EQ chain stopped and was started again". The EQ turns off, with the
note "computer EQ turned off: the chain stopped (...)", only when the restarted chain dies quickly
(within 10 s) a second time; a restart that fails is tried again after 1, 2, 4 and 8 s. A PipeWire
restart never turns it off, a chain that ran for crossfeed alone is removed rather than restarted,
and a pass-through that dies is not restarted. A daemon that starts over a chain it did not start
sends it the saved state when it plays other values. The view never says live while no chain runs.
Turning the EQ off while crossfeed plays, or on while it is flat, changes nothing but the sound; no
chain is rebuilt.

A crossfeed adds sound, so its loudest point is looked at: the preamp is the EQ's less the
crossfeed's worst gain at any frequency and any usual rate (44.1 to 768 kHz), rounded up to 0.1 dB,
and follows every change of model, level, crossover or delay. An update lowers the preamp before the
crossfeed grows and raises it after the crossfeed shrinks, so no whole update plays above 0 dB at
any frequency (a half-applied PipeWire update can, rarely; see [known issues](../known-issues.md)),
and turning crossfeed on never asks. Meier with a delay plays
about 1 to 2 dB quieter for the rate rule (up to about 3.5 dB at extremes), so a change of the
output's rate (44.1 to 768 kHz) never takes it over 0 dB; the other models are unchanged. The rule
is per frequency, as the EQ's: a peak in the samples of a loud, wide mix can still be a few tenths
of a dB higher. Crossfeed and the virtual speakers run up to 384 kHz: an output above that turns
them off with a note, and turning them on there is refused. With crossfeed on and the EQ off, the
level stays at the lowest one the loaded chain allows until the chain restarts (a chain whose
loaded curve took 6 dB off keeps those 6 dB while it plays flat).

### It turns itself off

Crossfeed is **off** whenever any of these happens, so a filter is never left on that you do not
remember turning on:

- the daemon (or a TUI with no daemon) starts and finds it on;
- the output the computer plays into changes (read once a second from the sound server);
- the output's rate goes above 384 kHz (reason `output-changed`);
- the filter chain restarts;
- the daemon stops gracefully;
- a TUI with no daemon quits;
- a reload that changes `[host]` halts the computer's side and stops its chain; crossfeed goes
  off with the same note as a stop (`svolder stopped`), though the daemon keeps running.

Each reset leaves a note and a reason in the view (`restart`, `output-changed`, `chain-restarted`,
`stopped`): "crossfeed turned off: it is off after a restart", and the TUI shows it as a toast once.
The settings (model, numbers) are kept, only the switch goes off. A chain that runs for crossfeed
alone is removed by a reset (one that also runs for loudness stays, flat); a chain with an EQ goes
on playing the EQ. If the sound server refuses the change the reset is tried again once a second and
the note says so. A change of the sink's sample rate is no reset: the crossfeed is worked out again
for the new rate (or turned off, with a note, when it cannot play at it). When live updates are
unavailable, a change rebuilds the chain with the crossfeed in its configuration, and the next start
of the daemon takes it out again. A chain left by an older svolder cannot take crossfeed and is
rebuilt once when it is first turned on; a held curve the chain cannot carry says why in the view's
`unavailable` instead, and the controls dim.

### The saved state

The state file of the computer EQ (`~/.local/state/svolder/host-eq.toml`) keeps the model, preset,
level, crossover and delay under `crossfeed`, two flags: `crossfeed_on` (the stage was last on)
and `flat` (the chain was applied only for crossfeed), and `crossfeed_loaded`, the stage the
running chain's own configuration holds, if any (it holds one only when a live update failed).
After a crash or a restart the next start reads them and takes the stage out of the chain, or
removes the chain, so nothing stays on; until then the level pays for that stage. Settings you
change while the EQ was never applied and no state file exists (the crossfeed's settings, the
chosen speakers set and the loudness's settings) are kept in `~/.local/state/svolder/host-eq-settings.toml`
until a chain is applied, and read back at the next start. That file holds only those settings: no
curve, target or route, and starting from it applies nothing and turns nothing on (loudness comes
back off). The first state file carries the same settings and takes the file's place.

## Virtual speakers

Virtual speakers make the computer's sound on headphones seem to come from two speakers in front of
you, at about 30 degrees to either side (45 for a With room · BRIR set), instead of from inside your
head. Use them with headphones on the computer's output. There are two choices, each always shown
with its technical term:

- **With room · BRIR**: the speakers in a measured room, with its early reflections and its tail;
- **No room · HRTF**: the same speakers in the open, with no room at all.

Nothing is bundled. A set is a measurement: you **fetch** one once from a short catalogue (svolderd
downloads it, checks its checksum, converts it and keeps only the result, 4 KB to 0.25 MB; your own
file at a high rate up to about 3 MB), or **import** a file of your own (a SOFA file, or a WAV of 4
or 14 channels, which the daemon reads from its own machine). Fetching and importing need the
daemon, which is the one process that reaches the internet; a TUI with no daemon lists the sets
already fetched and chooses among them. A set's licence and credit line, where the licence asks for
one, show wherever the set is chosen. The sets, their licences and how a file becomes a set are in
[virtual-speakers.md](../design/virtual-speakers.md#the-catalogue-and-the-fetch).

It works from the TUI (`C`, then the tone screen `e` or the device screen's Tone pane, and `S` for
the set list), the web page's Sound... popup (its Virtual speakers tab), [`svolder host
speakers`](cli.md#host-speakers), the MCP server's `get_host` and
[`set_host_speakers`](mcp.md#the-tools) (which never fetch), and Home Assistant (a switch and a
select of the fetched sets); all of them change the same one setting, kept by the daemon (or by a
TUI with no daemon), so the last change wins. A daemon from before virtual speakers shows crossfeed
alone.

### One choice with crossfeed

Space is one choice: **Off**, **Crossfeed** or **Virtual speakers**. Both feed one ear the other
speaker's sound, so they never play together: turning one on turns the other off in the same
update (the crossfeed switch in Home Assistant goes OFF, `set_host_crossfeed` keeps working). The
virtual speakers are global, like the EQ, not per output.

### Choosing a set, and the level

Choosing a set, or another set while they play, applies the filter chain again once, with one short
gap in the sound, as a change of the output's rate does. Turning them on rebuilds it once too when
the running chain does not hold the set yet (the usual first on after a choice made while they were
off); turning them off is live.
Choosing a set while they are off only keeps the choice: turning them on with no set chosen is
refused with the reason in words. A set is shown with where its speakers were measured (`left 30.0
deg, right 330.0 deg, 1.5 m`; the room set's are at 45 degrees), the length of its responses (`cut
to 0.5 s` when it was shortened) and its **loudness change**: every set is scaled when it is
converted so that the stage never plays above 0 dB at any frequency, which makes it quieter than
Off (about 7 dB, say), and the view says how much. The rule is per frequency, as the EQ's, never a
claim about the peaks in the samples. A set's first impulse always has 1 ms of silence in front of
it, so the sound server's resampling at other rates cuts nothing.

### Virtual speakers turn themselves off

The virtual speakers are **off** whenever any of the four resets of
[crossfeed](#it-turns-itself-off) happens (the daemon or a TUI starts and finds them on, the output
changes, the filter chain restarts, the daemon stops gracefully), and also when the chosen set's
file is missing or damaged: the chain then plays without them, the note says `virtual speakers off:
<set> is missing or damaged, fetch it again` with the reason `set-missing`, and the list shows the
set as not fetched. The choice of set is kept, only the switch goes off. A chain that runs for the
speakers alone is removed by a reset. A change of the sink's rate is no reset.

### The saved sets and state

The state file of the computer EQ (`~/.local/state/svolder/host-eq.toml`) keeps the chosen set's id
as `speakers`, and two flags: `speakers_on` (the stage was last on) and `speakers_loaded` (the set
the running chain holds, so a daemon that starts over that chain keeps the set's file safe). After a
crash the next start reads them and takes the stage out of the chain, or removes the chain, so
nothing stays on. The sets themselves are in `~/.local/state/svolder/speakers/`, one `<id>.wav` and
one `<id>.toml` each, at most 32 (the oldest unused one goes when a new one comes); forgetting a set
that plays, or one a running chain still holds, is refused.

