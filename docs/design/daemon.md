<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# The daemon

`svolderd` keeps one Bluetooth session to every configured device and lets any number of front ends
share them. This page is how it is built: the process and its start-up, the actor that owns a
device's link, the scanner that finds devices, the writer every write goes through, and what reload
and shutdown do. It sits one level below [the map](../architecture.md), whose life of a volume
change walks the same code from a key press; the words (actor, writer, slot, fleet) are in the
[glossary](../glossary.md). How the socket and the configuration file work is in
[ipc-and-configuration.md](ipc-and-configuration.md), and the profile a device runs under is in
[profiles.md](profiles.md).

## The process

`main` in `bins/svolderd/src/main.rs` starts things in an order that matters:

1. It asks for the sockets a service manager handed over (`listen_fds`) before it opens any file or
   bus of its own, so a stale `LISTEN_FDS` cannot name a descriptor the daemon opened itself.
2. It registers the SIGHUP handler (`hangup_stream`) before it awaits anything, so a hangup that
   arrives while it starts reloads instead of killing it.
3. It reads the configuration and the profiles. A file it cannot read ends the daemon (exit 1)
   before any device is touched.
4. It takes the one-daemon-per-socket lock (`take_lock`, an exclusive `flock` on `<socket>.lock`)
   and builds the transport (`transport::make`: `ble` or `fake`).
5. It checks that nobody answers on the socket, then binds it, or adopts the handed-over one.
6. Only then does `Daemon::reconcile` start an actor for each configured device, followed by
   `host_to` (the host service), `bridge_to` (MQTT), `mcp_to` and `web_to`.

A daemon that finds the socket taken exits 3 and has opened no link, so a second daemon never steals
devices from the first. While it serves, `stop_or_signal` ends the accept loop on SIGINT, SIGTERM, a
`stop` request, or the socket file being unlinked or replaced under the listener (`socket_taken`
compares the inode it bound, every second). Then `Daemon::shutdown` stops the HTTP servers, the host service
(after them, so no open page is told on the way down that the computer is no longer watched), the MQTT
bridge (which says `offline` first), and asks every actor to close at once.

`Daemon` (`bins/svolderd/src/handler.rs`) is the one object everything reaches: the set of device
handles, the transport, the shared scanner, the registry, the host service (the computer's player,
cover and route, kept once for every front end; see
[Host audio](host-audio.md#the-daemons-host-service)) and the slots for the web UI, the MCP server
and the MQTT bridge. Its `handle_from(origin, request)` answers every socket request;
`DaemonFleet` (`bins/svolderd/src/fleet.rs`) lets the web and MCP servers ask the same questions
in-process, each with its own `Origin`.

## One actor per device

`device::spawn` (`bins/svolderd/src/device.rs`) starts a tokio task, the `Actor`, and returns a
`DeviceHandle`: the command channel, the event broadcast, the `LiveInfo` the `devices` verb reads, a
`Feed` watch channel (profile, firmware, link state, every confirmed reading) that the MQTT bridge,
the web UI and the writer read, and the device's `Writer`.

The actor's life is a loop of two states (`device/link.rs`):

- **Disconnected.** `ensure_connected` runs a discover-and-connect attempt (`device/attempt.rs`) in
  a task of its own and keeps answering commands while it runs. A failed attempt waits the next step
  of `RETRY_SCHEDULE`: 2, 5, 15, 30, then 60 seconds, repeating the last. A command that needs the
  link, a `Wake`, or a changed configuration entry ends the wait early and restarts the schedule
  (`Command::ends_the_wait`); a bare profile reload does not.
- **Connected.** `serve_connected` selects between the command channel and `Session::recv`. Every
  status is folded into the session's `DeviceState`, published as an `Event::Status` to every
  subscriber, and added to the ring of the last half hour of readings (`device/ring.rs`,
  `device/publish.rs`). A lost link publishes `Event::Connection` and returns to disconnected; the
  readings stay, flagged stale.

Two things are stamped with the daemon's wall clock (`wall_ms`, unix milliseconds, the clock the
history and readings answers use). The signal strength comes from the advertisement a listen of
the daemon's hands an actor when it hears its device: `discover` (`device/attempt.rs`) returns the
advertisement with the moment the listen handed it over, and `update_identity`
(`device/publish.rs`) keeps its `rssi` in `LiveInfo.signal` as an `Advert` signal stamped with that
moment, so `devices` reports it as `DeviceInfo.signal`. An advertisement taken from the adapter's
held list (when the listen heard nothing) comes with no moment and sets no signal, since its
strength is a cached value, not a hearing. An advertisement with no strength leaves the held signal
as it is, and a lost link leaves it too, with its own source and stamp: it is always the strength
from before the connect, because BlueZ keeps `Device1.RSSI` only while it is discovering and an
unprivileged user cannot ask the kernel for a connected link's (`SignalSource::Live` is on the wire
for a daemon that can, and this one never produces it). Each reading's stamp is kept in
`Feed.heard_at` where the actor accepts it (`Actor::hear`; at a connect, `record_state` stamps the
readings `DeviceState::heard_since_stale` says the device reported on that link), and every
`ValueObject` the daemon builds carries it as `heard_at_ms`: `state`, `get`, a set's echo and the
`status` events. A reading kept after the link goes, or one a reconnect's burst did not repeat,
keeps its stamp.

The signal is advert-only because nothing else reaches an unprivileged service. A spike against two
dongles found no `Device1.RSSI` while a device is connected (BlueZ reports it during discovery and
drops it when discovery ends) and a refusal when the kernel's connection-information command is
asked by a user; raw HCI, `btmgmt` and capabilities are out of bounds for a user service, so no
live path exists. The daemon does not start a listen while a device is connected just to refresh
the number either: that would take the radio from the link, and a weak scanner would make the
number unreliable. The value is therefore as old as the last listen that heard the device, and it
is shown with its source and age however old, since a number that says how old it is never
misleads and a cutoff would hide something true. A reading's stamp is on the wire for the same
reason: an age has to survive a page that reconnects, so the daemon says when it heard a reading
and the front end words the age.

While the link is down every command is still answered: reads come from the last known state
(`last_state`), writes that need the link are refused with a recovery hint, and a changed
configuration entry is stored for the next attempt. Link changes over the last day are kept in
`device/history.rs` for the `history` request.

### The session underneath

`Session` (`crates/svolder-ble/src/session.rs`) is the protocol's life on one `Link`: `connect`
subscribes and writes function 1 to ask for every value, then reads until a burst ends. The burst
ends when every function the profile names has reported, on a quiet gap (`drain_idle`), or at
`drain_bound`, whichever comes first. `set` encodes a message, writes it, and returns when the echo
for that function arrives or `set_timeout` (2 s) passes; `set_no_wait` writes and returns. Writes
are spaced (`write_spacing`, 20 ms) because a write without response reports no completion over
BlueZ. A connect deadline (20 s) covers the connect attempts and the short backoffs between them.
`identify` writes function 200 with no refresh, at most once per `IDENTIFY_GAP` (2 s) per link and
on a refused link too (`set_no_wait` refuses there); the gap lives in the session so a daemon and a
direct session pace alike, and a new link starts with none. The actor answers `Command::Identify`
with a usage error (`NAME offers no identify`) where the profile's `identify` is false, link or not,
and otherwise with the not-connected error while it holds no link, and the command is deliberately
not on the `needs_link` list, so asking a waiting device never ends its wait. After the refresh the
session asks one `210.2.<n>` command (Data, empty) at a time for the subsystems the profile's
`firmware_subsystems` lists, in order, as the official app does: the first
`SessionConfig::firmware_delay` (300 ms) after the refresh, each next one that long after the
previous one's report, and no more on a link whose report never comes. `recv` writes each ask when
its time falls due, waking for it beside the notification stream, so a reading or a `set` is never
held up by the wait, and the daemon and a direct session both ask. Once per link: a refresh the
device's power-on owes does not ask again, and a refused link asks nothing. After a corrected code
lifts a refusal the ask waits for the refresh that code owes and for its burst to prove it
(`code_accepted`), so a correction refused again sends one refresh and nothing else, and the ask is
spent only when it goes out. The answers are ordinary statuses out of `recv`, stamped and published
like any other. `fold` rewrites each status's id once, with `Profile::read_as`, to the control that
reads it (a Loki Max band reported on `6.1` or `6.1.7` is the band `6.1.3`), and only when some
control of the profile has `reads`, so state, the feed and every front end see one id and a `set` is
confirmed by a report on any of the control's ids. A command on an action (a Loki Max recall or
save) has no echo: `set` writes it as `set_no_wait` does and returns at once, and the actor sends it
that way whatever the command asked, publishing nothing of it. An action that has `changes` arms a
guard: a report of any control it changes cancels it, and if none comes within
`SessionConfig::recall_guard` (1 s) of the command, function 1 is written once, by `recv` (waking
for it with the firmware ask) or by a waited `set` that is then awaiting its echo; a save arms
nothing, and a dropped link forgets the guard. `close` releases the radio so bluetoothd hands the
device back. A session refuses a device in firmware-update mode and can never encode the update
functions; the actor refuses them again in `Actor::send` (`device/commands.rs`, `refuse`).

Why identify and the firmware ask are shaped this way:

- **Identify rides the held link.** The official app opens a short connection of its own for it.
  The daemon already holds every configured device's link and runs the one reconnect loop with its
  backoff, so a second connect path beside it would compete with that loop for the radio and
  bypass its schedule. A device that is down answers "not connected" with the wait to its next
  attempt, and asking never wakes it. A refused link carries it, because the unit shows itself for
  a request under a wrong code as well. A device that is not configured has no held link, and a
  link of its own would need a code that may not be known, so it cannot be identified.
- **The gap is in the session.** One request arrives from a held TUI key, a repeated Home
  Assistant press, an MCP call and several front ends on one daemon. With the gap in the session
  one frame goes out whoever asks, and every front end answers "just sent" for the rest. The two
  seconds is a choice, not a measurement: what a unit does on receiving the request (one blink, a
  timed cue, a repeat that extends it) has not been seen on a real unit.
- **The firmware ask follows each connect.** The advertisement carries one build number; the main
  and DSP builds are readings the device sends only when asked, and the official app asks, so
  without the ask they would stay unknown on a real unit. The ask goes once per link, after the
  refresh, and a refresh that a power-on owes does not repeat it. It goes one subsystem at a time
  with the app's 300 ms before each, so a device that drops a request arriving while it answers
  the one before is never given two at once; the wait is a deadline `recv` honours, not a sleep,
  so it holds nothing else up.
- **After a corrected code the ask waits until the code is proved.** A code that has just been
  corrected may be wrong again. Spending the ask on the first correction would leave a second,
  right correction (X, then a wrong Y, then Z) with no ask for the whole link, so the ask goes out
  only once a burst or a set's echo has proved the code, and a refused link asks nothing.

## Finding devices: the scanner

`Scanner` (`bins/svolderd/src/scanner.rs`) is the one place that listens for advertisements. An
actor that wants its device calls `Scanner::sight(unit)` and is answered by the listen that is
running, or by the next. A listen lasts `SCAN_WINDOW` (5 s), answers each waiting device the moment
it is heard, and stops when nobody is waiting. The `scan` verb and the listen after a refused code
take turns on the same radio through one mutex (`radio`).

An actor that asks within `JOIN_GRACE` of a listen's start counts as having had the whole window;
one that asks later and goes unheard is answered at the end of the next listen, which it then has
whole. Without this a device that advertises slowly would be reported absent from a window it had
only seen the tail of. A device is answered the moment it is heard, unless it is the last one
wanted: that answer is held until the listen has stopped, because the connect it starts may pause or
end the adapter's scan, and a device still wanted would then hear nothing more from the window.

## The passphrase and its refusals

The entry's `passphrase` is `auto`, four digits or `none` (see the [usage
guide](../usage/README.md)). With `auto` the daemon reads the code from the advertisement
(`resolve_passphrase`, `device/passphrase.rs`). The advertisement can be older than the device's
code, so `judge_passphrase` only produces a note (`PassphraseNote`, shown by `devices` and `doctor`)
and never blocks a write; a session the device has answered without refusing shows the code works
whatever the note says.

A device that refuses its code keeps its link, because a refused link reports the refusal and a
second connect would only be refused again. The one exception is `auto`: `Episode`
(`crates/svolder-ble/src/episode.rs`) allows one drop of the link and one fresh reading of the code
from the next advertisement (`release_after_refusal`). A second refusal under the same code keeps
the link. The episode ends when the device shows a different code works, the entry changes, or a
connect reads another code. A wrong code therefore never loops.

## The writer

The writer (`bins/svolderd/src/writer.rs`, with its bookkeeping in `writer/slots.rs`) sits in front
of the actor's command channel. The socket, MQTT, the web UI and MCP each submit as an `Origin`;
reads, the passphrase change, reload and shutdown go straight to the actor.

- **One write in flight per function.** A write for a function waits behind the one in flight on
  that function. A `Mode::Latest` write (a slider, a climb step) replaces the one waiting, whose
  waiter gets `Outcome::Superseded`. A `Mode::Ordered` write (a typed command, a relative step) is
  never dropped and drops a waiting `Latest` write on its function.
- **Arrival order.** A waiting write is not overtaken by one that arrived after it, whatever its
  function, and a replacement keeps the place of the write it replaced. `submit` is synchronous, so
  the order a front end calls it in is the order the writer sees.
- **Relative steps are resolved at the writer.** `up` and `down` become absolute steps from where
  the function is heading (the step last asked, while fresh, against the reading), by
  `roles::next_step` (`crates/svolder-profiles/src/roles.rs`), and are resolved again as the write
  goes out (`Task::refreshed`). Two quick `down` presses before the first echo move two steps, and a
  lowering in flight is never undone by a raise.
- **The volume rule.** `Writer::glide` asks `safety::judge` with `Rules::writer` whether the level
  lands at once or climbs. Those rules have no cap and ask no question: the cap and the question
  belong to the front end, which applies them before it submits (MQTT and the web page clamp to
  their cap, MCP refuses and asks; the front ends' own `Rules` are `Rules::mqtt`, `web` and `mcp`).
  A climb belongs to the origin that asked: its own next glide steers it, and any other write to the
  volume, a mute, a power-off, a reading the climb did not write, a lost link or a changed profile
  ends it. `Writer::climb` publishes it on a watch channel the actor's `forward` task turns into
  `Event::Climb`. The rule itself, and why a raise above three quarters climbs, is in [the
  map](../architecture.md#where-the-volume-safety-rule-lives-and-why).
- **A silent device is not climbed on.** When the device reads itself muted or off (`silent`,
  `silences`), a glide writes only the step that may land at once, and a running climb ends.
- **Recall as a batch.** `Writer::recall` plans a tone-control preset from the device's readings
  when it starts (`recall::plan`, `tone::preset::recall_named`), holds every write to the
  functions it rewrites that arrives meanwhile, and sends the plan in order. A later recall
  replaces one that has not started. `Writer::apply` puts a device's built-in setting
  (`recall::plan_setting`) in the same line: one waited raw write per band, every band going down
  before any goes up, none for a band already on its value, and no effect on the recall memory.
  A setting holds the writes to the bands it sets, to the tone control, and to a device slot's
  recall or save (both are actions); a save therefore never stores a half-applied setting. It
  drops only the held `Latest` writes to its own bands. A slot's recall goes out only once no
  held write is in flight and no echo is awaited for a control it changes (`Queued::barrier`),
  so the echoes from before it are in first: a band written without waiting is answered at its
  `Ack` and the next write on the band goes at once, while its echo is awaited on the side
  until it comes or the session's set timeout passes (`Task::echoing`). A band written again
  while an echo is awaited is awaited until the timeout after its latest write, since the first
  echo cannot be told from the second. A band whose echo is late is marked unreported for the
  recall that goes out meanwhile (`Task::unsure`). After it goes out the band readings are from before
  it: a setting treats each band as unread until that band's own report arrives
  (`Task::unreported`), and writes it whatever it reads. Presets are described in
  [tone-control-and-presets.md](tone-control-and-presets.md).
- **One line per function as the profile reads it.** `key_of` keys a raw write on `read_as` of
  its id, so a band written as `6.1` waits behind the same band written as `6.1.3`; an action
  (`set recall 2`, or a raw `7.2.2`) is keyed on the command its slot makes.
- **Actions have no echo.** The actor sends any action without waiting and answers `Ack`, so the
  writer and the `Fleet` take `Ack` as success for a recall or a save, and nothing of an action is
  published or read back as a status.
- **Flat to EQ.** A band write that goes through while the tone control reads Flat may ask
  (`Options::flat_to_eq`) to be followed by a write of the mode EQ, so the edit is heard. MQTT and
  the web ask; the socket does not, because the TUI switches the mode itself.
- **Every waiter is answered.** When the actor stops, each waiting write is told (`STOPPED`), and a
  write waits at most `WRITE_DEADLINE` (60 s).

## Reload

`Daemon::reload_all` (SIGHUP, or a `reload` request, which the CLI and the web UI send after they
edit the file) re-reads the configuration and the user profile directory before acting on either, so
a profile that does not parse leaves every device as it was. It holds one lock for the whole reload,
then:

- `reconcile` matches devices by name, case-insensitively. A new name gets an actor; a lost name has
  its actor stopped; a name whose unit id changed has the old actor stopped before the new one
  starts, so the radio is free. Everything else (passphrase, profile, input labels) goes to the
  running actor as `Command::Configure`, so a device whose settings changed keeps its link; a rename
  costs nothing (`DeviceHandle::renamed`).
- `bridge_to`, `mcp_to` and `web_to` bring each service in line with its section through a slot
  (`Slot` in `bins/svolderd/src/http_slot.rs` for web and MCP, `BridgeSlot` in `handler.rs` for the
  MQTT bridge): a server already serving the section is kept, a changed one is
  stopped and replaced. Every slot releases its old server before any starts a new one, so two
  servers that swap ports find them free.
- `host_to` brings the host service in line with `[host]` and `SVOLDER_HOST` (a player change
  does not force a route read inside the 2 s spacing; the next timed read picks it up): the same
  configuration leaves it running, a change restarts it behind the same channels, so an open
  `Subscribe` stream that asked for the host hears it go off or come back.
- `reload` hands every actor the new registry; each re-resolves its profile, and a changed profile
  ends a climb.

The device set is swapped in whole, so a request in flight resolves against one complete set or the
other.

## Principles

**One process owns a link.** A Bluetooth connection has one owner, and several front ends (a TUI, a
phone, Home Assistant) must share it. The daemon owns it and everyone else asks. Make a second path
to the radio and two front ends fight over a connection one of them cannot see.

**Every front end's write goes through the writer.** Ordering, supersession, relative steps and the
volume rule live in one place, so a front end added later inherits them and cannot bypass them. A
write that reaches the actor another way loses the climb's cancellation and can undo a lowering
still in flight.

**What is drawn is what the device said.** A write is acknowledged by the device's echo, and the
readings are folded only from device notifications. A front end that shows its own guess shows a
value the device may have refused.

**Reads never need the link; writes that need it say so.** The actor answers every command with the
link down, with a recovery sentence instead of a hang. A command that waited for a link that is not
coming would hold the front end until its deadline.

**A refusal is reported, not obeyed blindly.** A refused code keeps the link and sets a flag; only
`auto` earns one fresh reading. A loop of drop and reconnect against a wrong code would keep the
device's radio busy and lock out the app.

**Retries back off.** The schedule grows to a minute, because scanning for a device that is off
spends power and airtime every other user of the radio shares; a command or a configuration change
that suggests the device is reachable restarts it.

**Cancel-safe selects.** Both arms of the actor's `select!` lose nothing when dropped, and
reconnection runs outside it in a task, so a cancelled branch never orphans a radio connection. Keep
any new arm cancel-safe.

**A poisoned lock is taken as it is.** `locks::locked`, `read` and `written` recover the guard after
a panic. What sits behind the daemon's locks is a report or a slot the next write puts right, and a
panic in one holder must not freeze every later caller.

**The computer's side is read once, and only for those who look.** The host service follows the
player for every front end, and reads the audio graph only while a page or a TUI that shows the path
is connected, at most every two seconds; a front end that never shows it (the MQTT bridge, the MCP
server, `svolder watch`) never holds the read on. A daemon with no session bus says it has no host
side and nothing else breaks; a bus that was there and goes away (a user's session bus restarting)
is looked for every 10 s, and the host comes back with the first look that finds it; so is a bus
that took the connection and has not answered yet (a slow start, or a look refused because an
earlier one still waits on it), and it is looked for until it answers, whatever the looks in
between find (a socket that vanishes meanwhile included). One that was never there is tried again
on a config reload. What the computer plays is captured on the same terms: the daemon captures and
analyses it only while a page's frames stream or a joined TUI's frames connection is open, and 2 s
after the last one goes the capture stops.

**The daemon never holds a socket it cannot be reached on.** The lock, the pre-bind check and the
inode watch exist so that two daemons never hold the same devices and a daemon whose socket file was
replaced stops rather than serving nobody.

## Where the code and tests live

- Start-up, lock, signals: `bins/svolderd/src/main.rs`; tests in `bins/svolderd/tests/daemon.rs`.
- Request routing and reload: `bins/svolderd/src/handler.rs`; tests in
  `bins/svolderd/src/handler/tests.rs` and `bins/svolderd/tests/reload.rs`.
- The actor: `bins/svolderd/src/device.rs` and `bins/svolderd/src/device/`; tests in
  `bins/svolderd/src/device/tests.rs` and `bins/svolderd/src/device/tests/refusal.rs`.
- The scanner: `bins/svolderd/src/scanner.rs`; tests in `bins/svolderd/src/scanner/tests.rs`.
- The writer: `bins/svolderd/src/writer.rs` and `bins/svolderd/src/writer/slots.rs`; tests in
  `bins/svolderd/src/writer/tests.rs`, and the ones `scripts/safety-tests.list` names.
- The host service: `bins/svolderd/src/host.rs`; its capture helper and the fake host's synthetic
  music in `bins/svolderd/src/host/capture.rs` and `host/synthetic.rs`; tests in
  `bins/svolderd/src/host/tests.rs`, `host/capture/tests.rs` and
  `bins/svolderd/src/handler/host_tests.rs`.
- Slots and the fleet: `bins/svolderd/src/http_slot.rs`, `bins/svolderd/src/fleet.rs`; tests in
  `bins/svolderd/src/http_slot/tests.rs`.
- The session and the refusal episode: `crates/svolder-ble/src/session.rs` and `episode.rs`; tests
  in `crates/svolder-ble/tests/session.rs` and `crates/svolder-ble/tests/episode.rs`.
- The volume rule: `crates/svolder-profiles/src/safety.rs` and `roles.rs`; tests in
  `crates/svolder-profiles/src/safety/tests.rs`.
