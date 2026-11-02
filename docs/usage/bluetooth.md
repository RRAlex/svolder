<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Transports and Bluetooth

How a verb chooses between the daemon and a direct session, the Bluetooth transport with its adapter
choice and timing, and the simulator used to try svolder without a device. Back to the
[usage index](README.md).

## Transport and Defaults

Every command's transport defaults to `auto` (env `SVOLDER_TRANSPORT`), which walks a chain before
falling back to a direct session: connect to the resolved socket; with nothing answering but the
systemd units installed, `systemctl --user start svolderd.socket` and connect again; with no systemd
or no units, start `svolderd` itself, detached, and wait up to two seconds for it to answer
(`[daemon] autospawn`, on by default, turns this step off); and only then a direct BlueZ session, as
before. `--transport daemon` walks the same chain but never falls back — with nothing answering at
the end of it, the command exits 3 rather than opening a direct session.

`--transport ble` forces a direct BlueZ session without ever consulting a daemon; `--transport fake`
forces the in-process fake device, likewise without consulting a daemon, so the fake is never
shadowed by a daemon on the same machine. `-v` names the choice on stderr:
`svolder: daemon at <socket>`, or `svolder: direct (ble)` / `svolder: direct (fake)`. `devices` and
`doctor` open no transport of their own, so `-v` names where they looked instead:
`svolder: daemon at <socket>` for a daemon that answered, and, for `devices` reading the
configuration file, `svolder: from the configuration`.

A chain that started a daemon of its own and it did not answer in time prints
`svolder: the daemon did not answer in 2s; using a direct session — see <log>` on stderr before the
command's own output, whether or not `-v` was given. A daemon the chain starts is handed
`SVOLDER_ADAPTER` as this command has it, and `SVOLDER_TRANSPORT` only when that names a transport
the daemon itself opens — `ble` or `fake`: `auto` and `daemon` are the two ways a *front end* asks
for a daemon, and the daemon has neither, so a script that exports one of them to insist on the
daemon still gets one started. See ["daemon"](daemon.md#daemon) for the six verbs that manage the
units and the autospawned daemon directly.

The fake device's profile, unit id, firmware version, and passphrase can be changed with environment
variables:

| Variable | Default | Example |
|----------|---------|---------|
| `SVOLDER_FAKE_PROFILE` | `gungnir2-v2` | `bifrost3` |
| `SVOLDER_FAKE_UNIT` | `0102030405060708` | `aabbccddeeff0011` |
| `SVOLDER_FAKE_FIRMWARE` | `3` | `1` |
| `SVOLDER_FAKE_PASSPHRASE` | `1234` | `none` |
| `SVOLDER_FAKE_FAULTS` | (unset) | `1` (activates all fault controls) |
| `SVOLDER_FAKE_SIGNAL` | (unset) | `1` (plays a signal on the meters; needs a profile whose meters have a scale, such as `fake-amp`) |

See ["Environment variables"](environment.md#environment-variables) near the end of this reference
for these six alongside every other `SVOLDER_*` variable the workspace reads.

A value that does not parse is an error, not a silent fallback: a non-numeric
`SVOLDER_FAKE_FIRMWARE`, an unparsable `SVOLDER_FAKE_UNIT`, or a `SVOLDER_FAKE_PASSPHRASE` that is
neither `none` nor four digits all exit 1 with a message naming the variable; an unknown
`SVOLDER_FAKE_PROFILE` exits 1 with a message naming the unknown profile instead.

Each CLI invocation runs its own fresh fake device: changes made by one command do not persist to
the next. To see the built-in profiles, run `svolder profiles list`. A `Meter` control's `scale`
TOML table, and how the TUI draws it, are documented under ["tui"](tui.md#tui)'s
["Device screen"](tui.md#device-screen).

Two optional fields apply to a `Readout` control with `value_type = "bool"`. A device may send such
a readout as the integer 1 rather than as a boolean; 1 reads true and any other integer reads false.
`only_while = "<key>"` names a boolean control, and the readout then reads true only while that
control also reads true: an amplifier's "Warming up" readout is shown only while the amplifier is
awake. The Status line, the MQTT state, MCP's `get_state`, the daemon's own answers,
`svolder status`, `get` and every line `svolder watch` prints apply it; with `--json` the `raw`
field stays the value the device sent and `display` carries the derived one.

A profile whose `only_while` names a key that is absent or not a boolean fails to load; a base
profile may name a key only its children define, so the check runs on each concrete profile (one
with a `product_id`) once it is fully resolved. A profile with no `product_id` of its own or
inherited is not checked, so a bad `only_while` there never reads true. When the control a readout
depends on changes, `svolder watch` prints the readout again with its new derived value and the MQTT
state follows, each only when the derived value changes. `auto_off = true` makes the session write
the readout false whenever the device reports it true, as the official app does with the amplifiers'
temperature reporting; such a readout cannot be set by hand.

`output_gain = true` marks a `Select` whose values, ordered by raw value from low to high, set how
loud the output is (a gain, or a mode with gain), and `mutes_output = true` marks a `Toggle` whose
true silences the output, and `powers_output = true` one whose true powers the device on and lets
sound through (every writable `power` toggle); the MCP tools ask before a higher gain, an unmute or
a power-on. `asks_on_change = true` marks a `Select` or `Toggle` whose every change asks, where no
order or polarity says which way is louder (the Lyr 5's pre-out, the Loki Max's mode). Each applies
only to its kind of control.

Every example in this reference that talks to the fake device passes `--transport fake` explicitly,
since the default is now `auto`.

## Real Bluetooth

`--transport ble` (or `SVOLDER_TRANSPORT=ble`) talks to a Forkbeard device directly over a real
BlueZ adapter, bypassing any running daemon. `--adapter hciN` (or `SVOLDER_ADAPTER`) picks which
controller to use; without it, a configured `adapter` (see
["Daemon and config"](daemon.md#daemon-and-config)) or the first adapter BlueZ reports is used.

```
      --transport <TRANSPORT>  Where a device verb runs: `auto` (the daemon when it is running, else a direct BlueZ session), `ble` (direct BlueZ), or `fake` (in-process) [env: SVOLDER_TRANSPORT=] [default: auto]
      --adapter <ADAPTER>      Bluetooth adapter for a direct BlueZ session (e.g. hci0). Defaults to the configured `adapter`, else the first one [env: SVOLDER_ADAPTER=]
```

Using `--transport ble` requires the invoking user to be in the `bluetooth` group and the chosen
adapter to be powered. A missing or unpowered adapter, or a permission failure from BlueZ, exits 3.

An adapter that exposes no `Modalias` property — some on-board and UART controllers — is rejected by
the BlueZ backend and never offered as a choice, so naming it with `--adapter` (or having no other
controller) exits 3.

Example:

```
$ svolder --transport ble scan --timeout 2s
$ svolder --transport ble --adapter hci1 status
```

### make e2e

`make e2e` runs `svolder-sim` as a peripheral on one Bluetooth controller and drives
`svolder --transport ble`, then `svolderd`, against it on another. `SVOLDER_E2E_ADV_ADAPTER` and
`SVOLDER_E2E_SCAN_ADAPTER` name the peripheral's and the central's controller — set both or neither
— and win over auto-detection. Left unset, `make e2e` auto-detects: it prefers a matched pair of
identical USB radios (the same USB vendor:product id, read from sysfs), then any two USB dongles
over another kind of controller, then whichever two qualifying controllers BlueZ reports first; it
prints which rule chose the pair.

Both controllers must support LE extended advertising (Bluetooth 5.0 or newer). The simulator's
27-byte Forkbeard manufacturer payload needs 34 bytes once BlueZ adds its own mandatory flags, and
`btvirt` virtual controllers report `MaxAdvLen = 31` in every mode, so a virtual controller cannot
advertise it; a real extended-advertising adapter reports `MaxAdvLen = 251`. The scanning controller
needs it too: a legacy-only controller cannot receive extended advertising packets, so it never sees
the peripheral. Two TP-Link UB500 Plus dongles (Realtek family) on one host are verified: `make e2e`
passes end to end against the fake device over both. A "CSR 4.0" dongle is Bluetooth 4.0 and will
not work. Without two qualifying controllers `make e2e` prints a SKIP message and exits 0. After
plugging in a dongle, each qualifying controller shows a `MaxAdvLen` above 31:

```
for h in $(busctl --system tree org.bluez | grep -oE 'hci[0-9]+' | sort -u); do
  v=$(busctl --system get-property org.bluez /org/bluez/$h \
        org.bluez.LEAdvertisingManager1 SupportedCapabilities 2>/dev/null \
      | grep -oE 'MaxAdvLen" y [0-9]+' | grep -oE '[0-9]+$')
  echo "$h MaxAdvLen=${v:-none}"
done
```

BlueZ 5.85 as first released, on Linux kernel 7.0, sizes the Add Extended Advertising Data command
with the legacy struct, so the kernel rejects every LE advertisement as `Invalid Parameters`; the
simulator needs a `bluez` build with the fix, which Ubuntu's 5.85-4ubuntu0.2 and later carry.

The run goes through `scan`, `inspect`, `set volume`, the live transport tests normally left
`--ignored` (a scan, a connect that writes a command and reads its status, and a connect the test
itself cuts), a test that kills the peripheral mid-link and checks the disconnect is seen, a scan
with the peripheral gone, a scan with it back, and then a daemon section: `svolderd --transport ble`
started against a temporary `XDG_CONFIG_HOME` naming the sim's unit, then `status`, `set`, `watch`,
a SIGHUP reload, and a kill that is expected to take the socket with it.

A last section restarts the sim as `fake-amp` with `--signal` and asserts that a direct `watch` on
the scanning adapter sees at least three `Meter L` lines within the section's 25-second budget — a
window wide enough for the scan that learns the device before the connect — and then ends that watch
with Ctrl-C the way a user would, so the link is released rather than left held: the signal reaching
a real central over the air.

Each run keeps the sim's and the daemon's logs in a temporary directory of its own. A passing run
removes it; a failing one keeps it and names it on stderr.

`SVOLDER_E2E_KEEP_SIM=1` changes what a failing run leaves behind: instead of stopping the
peripheral, it leaves the sim running and prints its pid to kill before the next run — the daemon,
if the run got that far, is stopped either way. `SVOLDER_E2E_UNIT` overrides the peripheral's unit
id (default `0102030405060708`).

Run the workspace's tests through `make test` or `make check`: both set `SVOLDER_TEST_PREBUILT=1`,
so the daemon-spawning test helper in `bins/svolder/tests/common/mod.rs` uses the `svolderd` binary
that same invocation already built. A bare `cargo test` builds the daemon into `target/test-daemon`
instead, apart from `target/debug/svolderd`, which another suite may have running; the first such
run compiles the daemon's dependencies, and `make clean` removes the directory. `make test` also
runs the suite with its own temporary directory and fails, naming them, if a test leaves anything in
it.

### The simulator's repeats and its advertising

`svolder-sim --quiet-same` has the simulated device report a change only: a Command equal to the
value a control holds gets no Status, on every control. Without it only the tone-control mode is
silent on a repeat, since the official app answers every report of the mode with a write of it, and
every other control reports a repeat as it reports any write. The in-process fake takes the same
knob as `SVOLDER_FAKE_QUIET_SAME=1`.

The simulator advertises again after a link to it closes, in either layout, with the same payload
and passcode field, so a central that scans again finds it. `make e2e` shows it with a connect, a
scan and a second connect; `SVOLDER_E2E_LEGACY=1` adds the same for `--layout legacy`.

### The simulator's signal

`svolder-sim --signal` plays a signal on the meters, ten readings a second, following a music-like
envelope — a slow loudness contour with a faster tremble on top, every channel its own phase apart
from the others, quiet passages included — deterministic from a fixed seed. Each reading sits on its
meter's own scale, so a channel's bar plays across the live part of the bar the way an amplifier's
would. A simulated amplifier with a Power control (an Aegir 2, say) starts in standby, where its
meters read `off`: switch it on, from the TUI's Power row or with `svolder set power on`, to see
them move.

The signal needs a profile with a scaled meter: given any other profile the simulator refuses before
touching a radio, with exit 2 and the line `svolder-sim: --signal needs a profile whose meters have
a scale, such as fake-amp or aegir2`, and once it starts playing it says so on stderr. Every
amplifier profile has scaled meters, and three more exist only for the demo: `fake-amp`
(`--profile fake-amp`), a stereo amplifier that reads like an Aegir 2, with no standby, product id
65534 so nothing on the air can be taken for it; `fake-mono` (`--profile fake-mono`), one meter,
channel `M`, on a Tyr's scale with its floor reading on `20.2`, product id 65533; and `fake-five`
(`--profile fake-five`), five meters, channels `1`-`5`, on a Kraken's scale, product id 65532 —
between them a front end has one, two and more than two channels to draw against.

`SVOLDER_FAKE_SIGNAL=1` plays the same signal on the in-process fake (`--transport fake`, both in
`svolder` and `svolderd`), which likewise needs a profile with a scaled meter —
`SVOLDER_FAKE_PROFILE=fake-amp`, `fake-mono` or `fake-five` (see the environment-variables table
above). The fake and the simulator report the `Power meters` switch on from the start. `watch` shows
the signal as ordinary meter events, through a direct session or the daemon alike.

### Tracing a connect

`SVOLDER_BLE_TRACE=1` prints one line to stderr per connect step, naming the step and its elapsed
time: `sighted` (taking the device a scan heard in the last 10 seconds, which is how a device verb
connects to what its selection just heard without scanning again), `find` (scanning for the device,
when nothing sighted it), `known` (falling back to a device the adapter is already holding, when the
scan saw nothing), `events` (subscribing to the adapter's disconnect events before the connect
itself), `connect`, `discover` (GATT service discovery), `characteristics` (locating the TX and RX
characteristics), `subscribe` (the RX notification subscription), and `notifications` (opening the
notification stream).

A scan traces what it heard the same way: `scan listed N after T ms` for one that waits out its
window, `sight listed N after T ms` for one that ends at the device it is listening for.

### Host requirements for a same-host rig

Running the peripheral and the central on the same host needs two things, or a peer adapter exposing
the BLE MIDI GATT service makes a MIDI client on the host demand pairing with it and the link drops:

- `bluetoothd` started with `--noplugin=midi`, through a systemd drop-in — BlueZ 5.85 has no
  `DisablePlugins` key in `main.conf` for this.
- No WirePlumber BLE MIDI monitor running in any user session: a system-wide
  `/etc/wireplumber/wireplumber.conf.d/` drop-in setting `monitor.bluez-midi = disabled`, with every
  session's WirePlumber restarted afterwards.

`scripts/e2e.sh` prints a hint naming this when it saw the peer exposing that service.

### The simulator against the official phone app

`svolder-sim` can stand in for a Forkbeard device towards the Schiit app on a phone, which exercises
this project's protocol code from the other side. The app scans legacy advertising PDUs only,
filtering on the Forkbeard service UUID, so the sim has to lay its advertisement out the way a real
device does: `svolder-sim --adapter hci1 --layout legacy` puts the flags and the service UUID in the
31-byte advertisement and the 27-byte manufacturer payload in the scan response, registering its own
advertisement object with bluetoothd because bluer's advertisement has no scan-response fields. The
default layout, `extended`, sends everything in one extended PDU and is what `make e2e` uses.

The legacy layout needs two more things on the host, both in the same systemd drop-in as the MIDI
exclusion, so that the `ExecStart` line reads
`bluetoothd --experimental --noplugin=midi,vcp,micp,bass,csip,mcp,ccp`:

- `--experimental`, because the scan-response properties of the advertisement interface are
  experimental in BlueZ 5.85.
- `--noplugin=` extended with `vcp,micp,bass,csip,mcp,ccp`: `--experimental` also starts BlueZ's LE
  Audio server plugins, whose characteristics require authenticated encryption. A phone probes them
  on every connection, is told "insufficient authentication", and bluetoothd then demands pairing,
  which the phone shows as a pairing prompt and the link drops when it fails. With the plugins
  excluded the app never pairs, which matches a real Forkbeard.

`--trace` prints every frame the phone writes, decoded through this project's codec, and every
notification the fake device answers with, so a session reads as `Command 10 64` followed by
`Status 10 64`. `--accept-pairing` registers a Just Works agent for a central that insists on
bonding; it answers pairing requests for every adapter on the host while the sim runs, so it is off
by default and not needed for the app.

Never combine it with a BlueZ central on the same host: if that central probes an authenticated
service on the sim's adapter, the two adapters bond, and every later connection between them fails
until the bond is removed on both sides (`busctl --system call org.bluez /org/bluez/hciN
org.bluez.Adapter1 RemoveDevice o /org/bluez/hciN/dev_<address>` for each). A phone caches the GATT
database of an unbonded peer, so after restarting the sim turn the phone's Bluetooth off and on
before reconnecting.

### Adapter choice

Prefer a USB Bluetooth dongle as the central (scanning) adapter over an on-board controller. On this
host, against a device that advertises every 1.28 s, the on-board MediaTek adapter misses about half
of its 5-second scan windows, where a TP-Link UB500 Plus catches about five in six.
`svolder scan --timeout` lists only what advertised during the window it listened, never a device
the adapter merely remembers from earlier (not even while another program keeps the adapter
discovering, which leaves a dead device's last signal strength in place), so a quiet window can
leave an advertising device off the list; a device verb given `-d` also consults the devices its
adapter is already holding from an earlier scan, so it can still connect to a device a quiet window
missed.

A `-d` that is a full 16-digit unit id — or a configured device, which is selected by its unit id —
ends each window at that device's first advertisement instead of waiting it out, while a unit-id
prefix or a product name waits out the window, since it has to hear every device it could name;
either way the connect opens its link on the device the selection just heard, so reaching a device
takes one scan, not two.

The daemon reaches its devices the same way. Per device, svolder remembers which adapter last
connected it (`$XDG_STATE_HOME/svolder/state.toml`) and tries that one first, so a multi-adapter
host settles on the adapter that actually hears the device rather than retrying whichever one the
config or the command line named. `svolder doctor` grades every adapter present against the first
configured device and warns when the configured `adapter` grades worse than another one on the host;
the setup wizard's Adapter step does the same grading for a device it is adding and writes the best
one into `adapter`.

### Timing

Real connects on this host, over BlueZ, take 2–7 seconds. A device verb against a device that never
answers fails after `connect_deadline`, 20 seconds — that bound covers every retry and backoff a
connect attempt makes, not just the first. Inside it, a single connect to btleplug is itself bounded
at 15 seconds, and the operations around it — a write, a disconnect, GATT discovery, subscribing for
notifications, the stop that ends a scan svolder waited out, and reading the devices an adapter
already holds — are bounded at 5 seconds each.

What has no bound of svolder's own is opening the adapter, starting a scan, taking the adapter's
event stream, the per-device reads the events of a scan window cost, and the stop that follows a
scan whose caller walked away mid-window: each of those waits out BlueZ's own D-Bus reply timeout of
30 seconds, which is also why a scan can take a little longer than the window it listened for.
Quitting during a reconnect can take up to 8 seconds (`close_grace`) for the attempt itself to be
given up on, plus up to another 8 seconds — the same grace again — for the release (the disconnect)
it leaves behind to finish, since that release cannot start until the connect it belongs to gives
up, and a connect is bounded at 15. So 16 seconds in all.

A direct session's release that has not finished after half a second says `svolder: releasing NAME…`
on stderr, once, when `svolder tui` quits (after the terminal is restored) and when `svolder watch`
is ended with Ctrl-C, and is then waited for as before (in `watch` a Ctrl-C during the release stops
waiting, exit 130); a quick release, and any quit over a daemon, print nothing.

The refresh a connect takes — the snapshot `status`, `inspect`, and `watch` all open with — ends as
soon as every function the profile lists has reported, falling back to a 300 ms gap with nothing
new, and after that to a 2-second bound whatever the device is still saying. A device that reports
everything its profile names, such as `fake-amp` playing a signal (see "The simulator's signal"
above), answers in tens of milliseconds rather than waiting out the gap; one whose meters never stop
talking leaves no quiet gap at all and is still bounded at 2 seconds.

A device with a power state (the Freya 2, Stjarna, Hati, Saga 2, Aegir 2, Vidar 2, Kraken, Tyr and
Wotan) is refreshed again each time it reports coming back on: from standby (or, on the Saga 2, off)
to on, or, on the amplifiers, from warming up to on. At most one refresh goes out per change, so a
report that leaves the device where it was sends nothing, and neither does one that takes it out of
on. Only a true or false report counts, and a device is followed once it has reported its power
function; on an amplifier, a warming-up reading that arrives first is remembered and judged with it.

The power state lives as long as the session, so a device that was in standby when the link dropped
and is on when it is back gets this refresh after the connect's own. A `set` made meanwhile waits
for the refresh's replies, for the end of the burst or a quiet gap of 300 ms or the 2-second bound,
before it goes out, and its own timeout starts after that. No such refresh is sent while the device
has refused the passphrase (a reconnect still sends the connect's own). The statuses the refresh
brings reach `watch` and every subscriber like any other.
