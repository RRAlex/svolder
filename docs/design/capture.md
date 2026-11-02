<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The capture file

`svolder capture` records everything svolder can learn about one device into a single text file
that its owner can send to the developers. The reason it exists is a new product: before anyone
owns one, nobody knows what it advertises, what it reports or what its buttons send, and a file
that shows all three lets a profile be written without the device. This page is what the file
holds and why, how it is scrubbed, and how the verb treats a daemon that holds the device. What a
user types and reads is in the [usage guide](../usage/cli.md#capture); the verb's code is
`bins/svolder/src/commands/capture.rs` and its children `capture/file.rs`, `capture/scrub.rs` and
`capture/handover.rs`. It is a command-line tool only, with no way in from the TUI or the web page:
it may have to stop the daemon that holds the device, and it is for an owner helping a developer,
not for everyday use.

## What the file holds

The sections come in the order the run gathers them, each under a heading:

- **`== doctor ==`**: the host checks as `svolder doctor --json` prints them. They say whether
  Bluetooth, the permissions and the daemon are in the state a device needs, which is the first
  question when a device does not show up. They are collected before the daemon is stopped, so they
  describe the host as the owner has it, and nothing is fixed.
- **`== adverts ==`**: every advertisement one 5 s scan heard, each with its manufacturer record in
  hex, then what it parses to (product, unit, whether a passphrase is present, firmware). BlueZ
  reports one record per peripheral per window, so this is each device's latest record, not every
  packet on the air. A neighbour's Forkbeard is in it too, and scrubbed the same way. The record
  is where a new product's code first shows, so it is kept whole.
- **`== inspect ==`**: the device chosen as every direct verb chooses it (`-d`, the one
  configured, the one in range), connected, and every reading it reports, as `inspect` lists them.
  That is what the profile says is there; the frames below show what is really sent. The names
  the owner gave the device's inputs are not read with it: the readings carry the profile's own
  names, which are the ones the developers know.
- **`== frames ==`**: every frame of the session from the connect on, each with its time since the
  tap started, its direction and its bytes in hex, then what `svolder decode` makes of them. The
  readings of the previous section are in it, and so, for the `--seconds` the owner asks for, is
  everything the device sends while its buttons are pressed and its knob turned. A line
  `-- frame window from +12.345 s --` stands before the first frame of that window, so what
  the connect and the readings said and what the buttons said are told apart. `--seconds 0`
  skips the window. A frame that does not decode is printed in hex with the reason it did not,
  never dropped: it is the most useful line the file can hold.

The header says the version, the time (UTC), the length of the frame window and, when Ctrl-C
ended the window early, how long it really lasted. Before the frames start a capture spends
time on doctor's device rows and adapter grades (several seconds each), on stopping a daemon that
holds the device (up to about half a minute) and on two scans, one for the advertisements and one
for the device selection; it says on stderr when it is checking the host, when it has stopped the
daemon and when the daemon is running again.

## The scrub

A capture is meant to be sent to strangers, so by default it holds none of the four things that
name the owner's device or could be used to talk to it; `--keep-ids` turns the scrub off for the
owner who wants the real values. The default is the safe side because the owner reads the file
once, tired, before sending it, and an option that must be remembered is one that gets forgotten.
Four things are taken out:

- **Unit ids.** A unit becomes `00000000000000NN`, where `NN` is its place in the file: the
  configured devices in the order of the configuration, then a unit id given with `-d`, then the
  units heard in the scan in the order heard; a unit id that stands as a word of its own in the line
  that says why a capture ended is hidden as well. The same placeholder stands everywhere, in a
  record's bytes and in text, so a scrubbed advertisement still parses and two lines about one
  device still read as one device.
- **Passphrase bytes.** The passphrase field of a frame the owner sent (its first four bytes) is
  zeroed; so is the passphrase in an advertisement (bytes 14 to 18 of a record); so is the value of
  every message of the passphrase function (250), sent or received. A sent frame that carried a
  passphrase is described as `passphrase field set (scrubbed)`, so the zeros do not read as "no
  passphrase was sent"; a field that really was zero stays `0x00000000`, which is information and
  not the secret. The decoded line of a passphrase message ends `(scrubbed)` for the same reason:
  its value reads as zero, and that is not what the device said.
- **The home directory**, which doctor's text prints in paths, becomes `~`.
- **Bluetooth addresses**, which doctor prints for the adapters, become `00:00:00:00:00:NN`, again
  stable within the file. A BlueZ object path that names a device, `dev_XX_XX_XX_XX_XX_XX`, loses
  its address the same way.

What stays is everything else: the names the owner gave to devices (doctor and the line of a
device that was not found say them), listen addresses of the MCP server and the web UI in doctor's
rows, adapter names, the time, signal strengths and every reading of a function other than 250. The
owner reads the file before sending it; the usage guide says so.

The walk over a frame does not depend on the frame decoding. Every frame, a sent or a received one,
decodable or not, is read as length-prefixed messages, and the value of every passphrase message
found is zeroed. A checksum is recomputed only where it matched before, so a damaged frame stays
damaged; a bad checksum that covered a zeroed value is replaced rather than left, so the file keeps
neither the secret nor a marker that points at it. A message cut short loses everything from its
value on, and the last two bytes of such a frame, which are not a checksum, are left alone. What
the walk cannot place as a message stays as heard, except that a sent frame always loses its first
four bytes: a received frame that does not parse has no boundaries to find a passphrase by, and a
device does not send one that way. A record too short to hold a unit id loses everything from byte
6 on, and one too short to hold the whole passphrase loses the part it has.

The tests that guard this, the ones that a leaked passphrase or unit id would break, are in
`scripts/safety-tests.list`, so deleting one is a visible act.

## When a daemon holds the device

One central holds a Forkbeard at a time, so a capture over the radio needs the daemon that holds
it out of the way. `handover.rs` finds who that is without waking anything, and where systemd
holds the socket with its service idle the doctor is told not to connect either:

- when systemd's units own the socket and the socket unit or the service is running, the holder is
  **systemd**. A connect to that socket would start the service, so this case reads the units'
  state and never connects;
- else a daemon that answers on the socket is **spawned** (one a front end started, or the owner
  by hand). With the units idle nothing is started by asking, so the socket is asked;
- else nobody holds it. With `--transport fake` nobody does either: the fake is the process's own.

The owner is asked before anything is stopped (`--yes` answers for them); a no stops nothing, and
captures nothing. Systemd's units are stopped by one `systemctl stop` that names the service and the
socket unit, so nothing starts the service again, and waited for until they are inactive; a spawned
daemon is asked to stop over its socket and waited for until the socket and the lock beside it are
gone. What was there is put back afterwards: the units that were running are started again, the
socket first, and a spawned daemon comes back through svolder's own autospawn, which means a daemon
started by hand with other flags returns the way svolder starts one. A restore that fails prints the
exact command that brings the daemon back and makes the exit code the daemon's, even after a good
capture.

The restore runs whenever the stop ran, even a stop that failed part way, and on every path out of
the stop and the body: a good capture, a failure, Ctrl-C and a panic, which is caught around the
stop and the body together and resumed after the restore. The `systemctl` calls run in a process
group of their own, so the terminal's Ctrl-C does not reach one that is part way through. A host
left without its daemon after a capture is the one outcome the verb may not have.

## Ctrl-C

Ctrl-C, SIGTERM and SIGHUP end the capture where it stands, from the moment the owner has
answered the question on; a Ctrl-C at the question itself ends the process as it does anywhere
else, with nothing stopped. In the frame window that is the normal
early end: the file is written, the exit code is 0 and the header records the window's real
length. Before the window, the file holds the sections finished so far and the exit code is 130. A
failure part way (nothing in range, a refused passphrase) writes the sections gathered so far with
the failure as the last line and exits with the failure's code. Presses while the daemon is being
put back are ignored: it always comes back.

## The file

The output file is checked at the start, before any daemon is touched: `-o` names a file that does
not exist yet in a directory that does, and the default name,
`svolder-capture-<product>-<date>-<time>.txt` in the current directory, is written with
`create_new`, readable by its owner alone (mode 0600). If the write fails at the end, the whole text
is printed to stdout so the sitting is not lost. On success stdout holds only the file's path.

## What it does not do

A capture never applies a doctor fix and never changes a setting on the device. It has no row in
the terminal UI yet: a "Save a capture" row in Settings waits for the next milestone, since the
question about the daemon needs a screen of its own there.

## Where to look

- The verb: `bins/svolder/src/commands/capture.rs`; the text: `capture/file.rs`; the scrub:
  `capture/scrub.rs`; the daemon hand-over: `capture/handover.rs`.
- The live path, both with and without a daemon, runs in `scripts/e2e.sh`, with a runtime directory
  and a `systemctl` of its own so that it can never touch the host's daemon.
