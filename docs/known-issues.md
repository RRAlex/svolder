# Known issues

What svolder's code does on purpose but imperfectly, or could do better but does not yet: real
limitations kept as they are, and findings judged larger than worth fixing right now. Each entry is
a present-tense statement of what the code does and why, naming the file it lives in.

The file has two parts:

- **Things you may run into** is for people who use the CLI, the TUI, the daemon or Home Assistant:
  something shown or printed wrong, a wait, a key that does nothing, a config the tool refuses or
  reads wrong. It is grouped by area: Bluetooth and devices, Daemon, CLI, TUI, Web UI.
- **Maintainer notes** is for people who change the code: code shape, missing tests, and races or
  edges no user meets outside contrived cases. It is grouped by the part of the code it concerns.

An entry reads ``- [kind] **Title.** Text (`files`).`` The kind is one of:

- `bug`: wrong or surprising behaviour we can fix;
- `limit`: imposed by hardware, BlueZ, a dependency or the protocol;
- `observed`: seen on real hardware or in real runs, and not checkable by reading code;
- `trade-off`: chosen on purpose, with the reason;
- `debt`: code shape, such as duplication, size or visibility;
- `test`: a missing or weak test.

An entry that belongs to upcoming work carries a tag after a comma in its kind:
`with Bifrost 3 testing`, as in `[test, with Bifrost 3 testing]`, or `with hardware` for one that
waits on a device svolder has not met.

To add an entry, put it under its area, in kind order (bug, limit, observed, trade-off, debt, test),
in the part whose reader would meet it; when unsure between the parts, use Maintainer notes. Drop an
entry once the code it describes changes. A cheap issue found in a branch's own diff is fixed in
that branch rather than listed here.

## Things you may run into

### Bluetooth and devices

- [limit] **An adapter with no `Modalias` cannot be opened.** The Bluetooth library svolder opens
  adapters through (btleplug 0.13 over bluez-async 0.9) leaves an adapter that exposes no
  `Modalias` property out of its list, and some on-board and UART controllers expose none, so a
  session cannot open it: naming it with `--adapter` exits 3, and so does naming none on a host
  with no other controller. `doctor` and the setup wizard read BlueZ directly and still list it;
  the wizard's Adapter step grades it 0 out of 5 with the open error as its note
  (`crates/svolder-ble/src/bluez/transport.rs`, documented in `docs/usage/bluetooth.md`).
- [limit] **The fresh reading after a refusal can wait out a scan window and a second connect.**
  The drop listens for at most one scan window (5 seconds), split between two listens when the
  first advertisement heard repeats the refused code: the second is a guard for a transport that
  reports a device when something about it changes, not only when it advertises. The connect that
  follows can itself be refused for several seconds by an adapter that has just released the
  device. A refusal is therefore reported seconds after the first refused answer, and a device that
  another central takes in the window is not read at all (`crates/svolder-ble/src/episode.rs`).
- [limit] **Discovery cannot be limited to LE.** svolder starts discovery with a service filter
  only, and btleplug 0.13 asks BlueZ for discovery over both transports, so a dual-mode controller
  runs BR/EDR inquiry alongside LE scanning for the whole window (seen on a TP-Link UB500 pair);
  an LE-only discovery tried by hand did not reduce missed windows
  (`crates/svolder-ble/src/bluez/transport.rs`).
- [limit] **The signal strength is the advertisement's from before the connect.** A device shows
  the strength its advertisement carried when the daemon last listened for it, with its age, and
  it does not refresh while the link is up: BlueZ publishes `Device1.RSSI` only while it is
  discovering and drops it when discovery ends, and an unprivileged user cannot ask the kernel for a
  connected link's strength (the management call is refused with permission denied)
  (`bins/svolderd/src/device/publish.rs`, `crates/svolder-ipc/src/socket/signal.rs`).
- [limit, with Bifrost 3 testing] **Recently-off devices still find their way back one at a
  time.** The daemon's shared listen finds every device it is looking for in one window, but when
  it answers nothing for a device, that device's connect falls back to the devices the adapter
  holds, and each cached device's `find` takes the transport's scan lock for its own window in
  turn (`bins/svolderd/src/scanner.rs`, `bins/svolderd/src/device/attempt.rs`,
  `crates/svolder-ble/src/bluez/transport.rs`).
- [observed, with hardware] **Amp meter scales are unconfirmed on a real amp.** Each amp's meter
  reads on a logarithmic scale built from a zero, a full scale and a class-A block, and the
  profiles carry those constants; none has been checked against an amp. Open questions: where
  each amp's zero and full scale really sit, whether the Wotan's two channels have different full
  scales, as their profile has them, how far above a reported floor the zero belongs on the Tyr
  and the Wotan, how often an amp sends readings, what an amp does with
  its meters when they are switched off (whether readings stop or go on), whether it reports the
  switch and its floors at connect and after a refresh (until a floor arrives a Tyr or Wotan meter
  reads from a zero of 0, so an idle amp fills much of the bar), and what physical quantity the
  readings measure (`profiles/`, `crates/svolder-profiles/src/meter.rs`).
- [observed, with Bifrost 3 testing] **The onboard MediaTek adapter scans poorly.** The onboard
  MediaTek adapter (`hci0`) as a central, against a device that advertises every 1.28 s, misses
  about half of its 5-second scan windows, where a TP-Link UB500 Plus catches about five in six; a
  USB dongle is preferred as central for this reason
  (`crates/svolder-ble/src/bluez/transport.rs`).
- [observed, with Bifrost 3 testing] **A 1.28 s advertiser is missed by whole scan windows.** A
  device that advertises every 1.28 s, BlueZ's default when an advertiser names no interval, is
  missed by a whole 5-second `svolder scan` window about a quarter of the time on a TP-Link UB500
  central, and by several windows in a row now and then; svolder cannot change a device's
  interval, and whether Forkbeard devices advertise that slowly is unknown.
- [observed, with Bifrost 3 testing] **Whether a unit echoes a write of the value it already holds
  is unverified.** svolder's fake device and simulator answer a repeated tone-control mode with no
  Status and every other repeat with one (all of them silent with `--quiet-same` or
  `SVOLDER_FAKE_QUIET_SAME=1`); the official app needs the silence on the mode, or it writes the
  mode forever. A write svolder makes of the value a device holds and gets no echo for is answered
  with that value after the set timeout, not with an error, but a unit that does echo a repeat
  answers at once. A waited Loki Max band write of the held value lands as a value, so a unit
  that echoes repeats more than 2 s late and stays silent after a recall could have that late echo
  read as the post-recall report. The Bifrost 3 settles which it does
  (`crates/svolder-fake/src/device.rs`, `crates/svolder-ble/src/session.rs`).
- [observed, with hardware] **Loki Max band reports are unverified.** Which id a real Loki Max
  reports a band on is unknown. svolder reads a band from a report on `6.<b>` or on any
  `6.<b>.<n>` as well as on `6.<b>.3`, so any of them lands; a report on a form outside those
  leaves the band unread (`profiles/lokimax.toml`).
- [observed, with hardware] **A Loki Max's answers are unknown.** Nothing says which form a unit
  reports its bands on after a recall or a built-in setting; whether it reports them at all
  (svolder asks for a refresh one second after a recall that no report follows, once); whether
  a `6.<b>` write behaves as a `6.<b>.3` write; whether a `6.<b>` write is echoed (if not, a
  built-in setting stops after its first write with a timeout); whether the device answers a save;
  and what it does with a slot outside 1 to 3, which svolder never sends
  (`profiles/lokimax.toml`, `crates/svolder-ble/src/session.rs`).
- [limit, with hardware] **A preset recall waits up to 2 s for a band's echo.** The TUI sends
  every band write without waiting, and the daemon goes on to the next write at once; but a device
  slot's recall asked right after a band write goes out only once that band's echo is in, up to 2 s
  on a unit that never echoes a band write, and every write asked behind the recall waits with it.
  A built-in setting or a preset waits the same way before it starts, and only the writes to the
  tone control, to the bands it holds and to a slot's recall or save wait behind it
  (`bins/svolderd/src/writer.rs`).
- [limit, with hardware] **An echo more than 2 s late can count as a report after a second recall.**
  A band whose echo has not come 2 s after its write is unreported after the next recall of a device
  slot, and that recall ends the mark: if the echo comes later still, after a second recall, the
  second recall takes it for the device's report, and a built-in setting planned then can skip that
  band. A band written without waiting that the device's actor leaves unanswered for 60 s gets no
  mark at all. Whether a real Loki Max ever echoes that late is unknown
  (`bins/svolderd/src/writer.rs`).
- [observed, with hardware] **On a unit that stays silent after a recall, a band echo can cancel
  the refresh.** The echo of any band write made within the second after a recall (from the
  TUI, the web page or the CLI, over the daemon or not) counts as the device reporting what the
  recall changed, so it clears the one-second refresh guard on a unit that stays silent after a
  recall: no refresh follows and the other bands keep their values from before the recall until
  something else reports. This matters only on a unit that pushes nothing after a recall. The
  daemon puts a recall behind the echoes of the band writes in flight when it goes out, which
  spares only those writes (`crates/svolder-ble/src/session.rs`, `bins/svolderd/src/writer.rs`).
- [observed, with Bifrost 3 testing] **Remote lock's meaning is a best guess.** The app shows
  nothing for Stjarna's and Hati's `201`; "Remote lock" is the likeliest reading, and nothing the
  device or the app does confirms it (`profiles/stjarna.toml`, `profiles/hati.toml`).
- [observed, with Bifrost 3 testing] **Tyr's over-temperature channel is assumed.** svolder takes
  Tyr's over-temperature fault to come with a channel id and listens on `230.2.1` only: a report on
  another channel, or a plain `230.2`, raises no fault (`profiles/tyr.toml`).
- [observed, with Bifrost 3 testing] **Tyr's over-current fault id is assumed.** svolder listens
  for Tyr's over-current on a plain `230.1` only, as `amp-common` defines it, so a Tyr that reports
  it with a channel id, as Gjallarhorn and Wotan do, raises no fault (`profiles/tyr.toml`,
  `profiles/amp-common.toml`).
- [observed] **The Skoll's headphone status has no known meaning.** The Skoll reports a boolean on
  `5` that the app shows nothing for; svolder shows it as the "Headphone output" readout, whose
  meaning is unconfirmed (`profiles/skoll.toml`).
- [observed, with Bifrost 3 testing] **What a unit does on identify is unverified.** svolder sends
  function 200 with no payload, at most once every 2 seconds, and it goes out on a refused link
  too; what a unit does with it (a blink, a timed cue), whether a repeat extends the cue, and
  whether a unit under a wrong passphrase answers it at all have not been seen on real hardware
  (`crates/svolder-ble/src/session.rs`, `crates/svolder-proto/src/function.rs`).
- [observed, with Bifrost 3 testing] **A silent device holds a connect to the drain bound.** The
  refresh waits for every id a profile names, so a device that never reports `21.1`, `201` or a
  three-fid fault holds a connect until the 2 s drain bound (`crates/svolder-ble/src/session.rs`
  `awaited`). A `set` made just after a device comes back on waits for the refresh's burst the
  same way, up to that bound, before it goes out.
- [trade-off] **A refusal is handled differently from the app.** svolder keeps the link when the
  device refuses the passphrase and reports the refusal, where the app disconnects when the
  advertised passcode does not match the stored one. A device on `auto` is the one exception, once
  per refusal: svolder releases the link, reads the code from the device's next advertisement and
  connects again, and keeps the link only if that is refused too (the entry below). A code typed as
  digits, and `none`, keep the link at the first refusal. Only a status of exactly `250.255` counts
  as a refusal, whatever its direction; a `250.255.<n>` does not. The `250.255` itself is handed to
  callers and published like any other status (`crates/svolder-ble/src/session.rs`,
  `crates/svolder-ble/src/state.rs`).
- [trade-off] **Temperature reporting is switched off, as the app does.** What an amp streams while
  it is on is unknown (`profiles/amp-common.toml`, `crates/svolder-ble/src/session.rs`).
- [trade-off] **Freya 2's gain can be sent in Passive.** The app refuses it there; svolder sends it
  and the device decides (`profiles/freya2.toml`).
- [trade-off] **A Mesh Card is offered no identify.** The app offers none on a Mesh Card, so svolder
  does not either: its profile sets `identify = false`, the daemon and a direct session refuse the
  request in words, and the TUI dims `b`, the web hides the button and Home Assistant gets none.
  `svolder raw 200` can still send the function (`profiles/meshcard.toml`,
  `crates/svolder-proto/src/function.rs`).
- [trade-off] **An absent device fails at the connect timeout, and a stale advertisement under a
  typed code is not corrected while its link is up.** When a window hears nothing of the device,
  device selection (`select_device`) and the daemon's `discover` fall back to the advertisement
  the adapter holds (`Transport::held`), and `BluezTransport::find` to the peripheral it holds, so
  a held advertisement can feed profile and passphrase resolution stale data; the cost is an
  absent device failing at the connect timeout rather than at device selection. A device a
  central holds does not advertise, and the daemon reads an advertisement again only at its next
  connect attempt, which comes when the link drops. Under `auto` a refusal of a code resolved from
  a stale advertisement is therefore answered once: svolder releases the link, listens for the
  device's next advertisement for one scan window and connects again with the code it carries.
  When that advertisement carries the same code, or none is heard, the refused link is kept
  (above), nothing is written to it, and the user power-cycles the device. The same refusal earns
  no second drop while the code svolder would use is the one that was refused: the episode ends
  when the device shows another code works, the entry changes, or a connect reads another code
  from the device's advertisement. A code configured as digits is never dropped over: it gets the
  passphrase note, and a corrected entry ends it. A direct session reads one advertisement at its
  start, again once per refusal, and under `auto` again before each reconnect after its link
  drops (`crates/svolder-ble/src/bluez/transport.rs`, `crates/svolder-ble/src/episode.rs`,
  `bins/svolderd/src/device/attempt.rs`, `bins/svolder/src/backend/direct.rs`,
  `bins/svolder/src/commands/device/select.rs`).
- [trade-off, with Bifrost 3 testing] **A verb against an absent device takes 20 seconds or more.**
  A session's `connect` keeps trying for `SessionConfig::connect_deadline`, 20 seconds, so a verb
  run without the daemon against a device the adapter still holds but that is gone fails after the
  selection's 5-second window, those 20 seconds, and up to `CLOSE_GRACE` (8 seconds) more while
  the transport takes the abandoned attempt down. A device the adapter does not hold fails at
  selection instead, after `WINDOWS` windows of `WINDOW` (15 seconds), and through the daemon a
  write to a disconnected device is refused at once (`crates/svolder-ble/src/session.rs`,
  `bins/svolder/src/scanning.rs`).
- [trade-off] **A recall does not ask.** The TUI, the CLI, the web page and Home Assistant send a
  Loki Max's slot recall at once, as the app does, though the slot's bands are the device's to say
  and a recalled band can be up to 12 dB above 0. The MCP server asks first (below). The save
  asks wherever it is offered (`bins/svolder/src/commands/device/device_presets.rs`,
  `bins/svolderd/src/mcp/tools.rs`).

### Daemon

- [limit] **An MCP client that cannot ask the user gets `confirm`.** A volume rise of more than
  6 dB, a higher gain or mode, an unmute, a power-on and any change of a control that asks on every
  change (the Lyr 5's pre-out, the Loki Max's mode), a Loki Max built-in setting that raises a band
  more than 6 dB and any recall of a Loki Max slot ask the user through MCP form elicitation,
  which only a client on protocol 2026-07-28 or newer that declared form elicitation can answer;
  any other client gets a tool error telling the model to ask the user and repeat the call with
  `"confirm": true`, which the daemon then honours without being able to check that the user was
  asked. Leave `[mcp] writes` off, or set `max_volume_db`, for such a client
  (`bins/svolderd/src/mcp/tools.rs`).
- [trade-off] **MCP judges a built-in setting against the floors it read at call
  time.** A band that another front end lowers after the call and before the writer applies the
  setting is raised back, and was judged against the earlier floor, so a rise can land larger than
  the one asked about, by up to the band's whole range, 24 dB on bands 1 and 6 (the floor at
  +12 dB, the band lowered to -12 dB within the window, a setting that targets +12 dB). It is the
  window `set_control` has (`bins/svolderd/src/mcp/tools.rs`).
- [trade-off] **Home Assistant's mute, power, gain and Preset are not guarded.** With
  `commands = true`, a `mute OFF`, a `power ON`, a gain select set higher or a Loki Max's Preset
  select (a slot, or a built-in setting that raises a band) from Home Assistant goes to the device
  at once: the bridge clamps the volume to `max_volume_db` and climbs it above three quarters of its
  range, but it asks nothing, having no one to ask, where the MCP server asks first. The web page
  does the same for a signed-in browser with `[web] writes` on, and so does the TUI. The rule
  that names those changes is shared (`lets_sound_through`), so a front end that can ask has it to
  hand (`bins/svolderd/src/mqtt_bridge/commands.rs`, `bins/svolderd/src/web/writes.rs`,
  `crates/svolder-profiles/src/safety.rs`).
- [trade-off] **MCP judges its question and its cap on its own reading of the volume.** The 6 dB
  question and the `max_volume_db` cap are decided on the volume the MCP server read when the tool
  was called; a knob turned down between that reading and the write can make an allowed change
  land as a larger rise than the one asked about. Above three quarters of the range the daemon's
  climb paces the rise (`bins/svolderd/src/mcp/tools.rs`).
- [trade-off] **A timed-out MCP glide or recall keeps running.** When the daemon's deadline
  passes, the tool answers with an error, but the glide or recall stays in the writer until it
  ends: a write is bounded by its own deadline and a climb by its own end, so the device may
  still move after the error (`bins/svolderd/src/handler.rs`, `bins/svolderd/src/writer.rs`).
- [trade-off] **The MCP listener has no TLS and no OAuth.** It speaks plain HTTP with a bearer
  token, so a LAN client reaches it through a reverse proxy that terminates TLS, and a single
  sign-on cannot validate tokens yet: `McpAuth` is the seam a later `auth = "oidc"` would fill
  (`bins/svolderd/src/mcp.rs`).
- [trade-off] **A mode and a band sent together can leave the band unheard.** A band command from
  Home Assistant or the web page, answered while the tone control reads `flat`, switches it to
  `eq` unless a mode command arrived after the band. When a `flat` sent before the band is still
  on its way to the device as the band is answered — the two held during a preset recall and
  released together, or published back to back by a script or a scene — the switch is skipped:
  the device ends in that `flat`, with the band written but not heard. Queuing the switch behind
  that mode write would close this; it is left because this way round never overrides a mode
  asked for later (`bins/svolderd/src/writer.rs`, `bins/svolderd/src/web/writes/tone.rs`).
- [trade-off, 0.28.0] **A player that rewrites one cover file for every track keeps the first
  picture.** The daemon reads a cover once for each `mpris:artUrl` and keeps it while the address
  stays the same, so a player that writes every track's art to the same path shows the first
  track's picture for the whole session. The common players change the address per track or per
  album; keying the cache on the track as well would cost one local read per track
  (`bins/svolderd/src/host.rs`).
- [limit, 0.32.0] **Low, sharp filters play with f32 noise.** PipeWire's biquads run in f32 (and
  with fused multiply-adds on arm64), so a filter below about 40 Hz with a high Q plays with
  rounding noise 40 to 55 dB under the music at 48 kHz and more, worse as the rate rises, and its
  response can differ from the curve drawn. The computer EQ refuses only a filter that is not
  stable in those coefficients, never one that is merely noisy
  (`crates/svolder-host/src/eq/manage.rs`).

### CLI

- [limit] **The process manager's pid file is written after the old chain stops.** `host eq apply`
  tries the runtime directory before anything of the chain that is on is touched, but the pid it
  keeps there is only known once the new chain starts; a runtime directory that fills up between
  the two fails the apply with the old chain already stopped
  (`crates/svolder-host/src/eq/manage.rs`, `start_process`).
- [limit] **`doctor` grades adapters against a held device.** `doctor`'s Adapter grade row, shown
  when there are two or more adapters, grades each against the first configured device, which a
  running daemon may be holding, and a held device stops advertising: on such a host the grades
  read low across the board and the row's recommendation is worth less than it looks. `doctor`
  observes and starts nothing, so it cannot put the daemon down to measure properly; the Devices
  row beside it says outright when the daemon holds a device, which is the reader's cue
  (`bins/svolder/src/commands/doctor.rs`).
- [limit] **Two Ctrl-Cs a millisecond apart can count as one.** Signals that arrive before
  `watch`'s receiving task has read the one before are delivered once (Tokio coalesces them, as
  the kernel does a standard signal still pending), so two presses within about a millisecond
  can stop the loop and leave the release waited for, until a third press gives up on it
  (`bins/svolder/src/commands/device/watch.rs`).
- [trade-off] **`svolder set` is outside the volume's glide.** `svolder set volume max`, a raw
  number above three quarters of the range and `svolder raw` on the volume write that step at
  once, as does `svolder set volume up --step N` that lands above three quarters with N of 4 or
  more: an explicit command is taken as meant. The TUI's bar, keys and wheel, and Home
  Assistant's slider, climb above three quarters at a tenth of the range a second instead
  (`bins/svolder/src/commands/device/values.rs`). The keys' and wheel's pace keeps its own credit,
  apart from the bar's glide, so bar drags interleaved with wheel notches can approach twice the
  glide's rate (`bins/svolder/src/tui/app/glide.rs`).
- [trade-off] **A plain-prompt wizard stopped by Ctrl-C leaves the daemon unreloaded.** The
  wizard's plain prompts, which a walk falls back to where the TUI cannot claim the terminal,
  leave Ctrl-C to end the process, so a walk stopped that way on the Adapter step, its device
  written, does not tell a running daemon to re-read the file as every other way out of a walk
  does; the daemon learns of the device at its next reload or restart
  (`bins/svolder/src/wizard/cli.rs`).
- [trade-off] **The wizard's Adapter step takes seconds per adapter.** The wizard's Adapter step
  grades the adapters one after another, so that each count is what that adapter heard alone, for
  `GRADE_WINDOWS × GRADE_WINDOW` (five 1-second windows) each: on a host with three dongles,
  fifteen seconds between naming the device and seeing the next prompt; announced
  (`Wizard::busy`) but not shortened (`bins/svolder/src/wizard.rs`, `bins/svolder/src/scanning.rs`).
- [trade-off] **`daemon install` waits five seconds for a stuck daemon.** A daemon that will not let
  its socket go holds `daemon install` for `STOP_WAIT` (five seconds) and then fails the verb, with
  the unit files written and nothing enabled; running the verb again once the daemon is dealt with
  finishes the install (`bins/svolder/src/commands/daemon/install.rs`, with the wait in
  `bins/svolder/src/commands/daemon/lifecycle.rs`).
- [trade-off] **Move to computer flattens the DAC first.** Putting the preset that is on the DAC
  on the computer, with `svolder host eq apply NAME` or `Enter` in the presets box on the computer's side, asks first and on
  yes flattens the DAC before the apply, so the correction is never on both; a failed apply leaves
  the DAC flat, and the message says how to put the curve back (`svolder eq load NAME`, or `Enter`
  on it in the presets box) (`bins/svolder/src/commands/host_eq/verbs.rs`,
  `bins/svolder/src/tui/boxes/presets/computer.rs`).

### TUI

- [limit] **Over SSH in a VTE terminal a pairing code and the MCP token cannot reach the local
  clipboard.** VTE (GNOME Terminal, Ptyxis) ignores OSC 52, and only the Web UI box's link is a
  terminal hyperlink (OSC 8), so a code or token copied in a session with no display has no route
  to the clipboard of the computer in front of you; the box says the sequence was sent, never that
  it was copied. The ways round it are `ssh -X` (the copy then pipes to `xclip` or `xsel` and
  lands on your display's clipboard), a Shift+drag selection of the text on screen, or running
  `svolder mcp token` or `svolder web pair` in a terminal on the local computer
  (`bins/svolder/src/tui/clipboard.rs`, `bins/svolder/src/tui/run/web_box.rs`).
- [limit] **A session that inherits a local `DISPLAY` over SSH copies to that display.** A shell
  that keeps the desktop's `DISPLAY` or `WAYLAND_DISPLAY` (a tmux started at the desktop and
  attached over SSH) makes the copy tools run against that session's clipboard, which is the
  computer's, not the one you sit at, and the note says `copied with xclip` without naming it
  (`bins/svolder/src/tui/clipboard.rs`).
- [trade-off] **A tone edit waits out another front end's preset recall.** While a recall from
  Home Assistant or MCP is running on the daemon, a tone-control edit from the TUI waits for it to
  finish, and the TUI waits with it, redrawing and reading keys again only once the write is
  answered, because it waits for each write's reply; the wait ends at the daemon's 60 s deadline
  at the latest (`bins/svolderd/src/writer.rs`, `bins/svolder/src/tui/run/events.rs`,
  `bins/svolder/src/backend.rs`).
- [trade-off] **Imported presets are fitted on the UI thread.** An imported preset is fitted the
  first time the tone screen's header, the device screen's Tone pane or the preset list draws it,
  or when the list opens (`presets::reread`), and `fit_for` keeps each fit for the rest of the
  process; with ten or more imports the screen pauses for about 0.3 s once
  (`bins/svolder/src/tui/boxes/presets.rs`, `crates/svolder-profiles/src/tone/preset.rs`).
- [trade-off] **Bands past the ninth have no digit key.** The tone screen's digit keys reach a band
  by `1`-`9` only, so a custom profile with more than nine bands (every built-in tone control has
  three or five) has bands past the ninth that only `↑↓` and the mouse can focus
  (`bins/svolder/src/tui/app/tone.rs`, `bins/svolder/src/tui/keys.rs`).
- [trade-off] **A recalled preset's name lives as long as the daemon.** The name a recall put up
  (for the `*` the tone screen's header and the device screen's Tone pane show) is kept by the
  daemon in memory, per device and for every front end, and written nowhere: restarting the
  daemon starts back at whatever the readings alone say, and a TUI session with no daemon keeps
  the name for that session only (`bins/svolderd/src/writer.rs`,
  `bins/svolder/src/tui/boxes/presets.rs`, `PresetsBox::loaded_preset`).
- [trade-off] **The Tone pane has no edit line.** `Enter`'s edit line for a band's frequency, gain
  and Q opens on the tone screen only; the device screen's Tone pane keeps `←→`'s one-step nudge and
  has no edit line or `Enter` hint of its own (`bins/svolder/src/tui/boxes/edit_line.rs`,
  `bins/svolder/src/tui/app/controls.rs`).
- [trade-off] **The 80-column edit line does not say what its arrows do.** At 80 columns the edit
  line writes its keys short, `Tab ←→ ↑↓ · ↵ writes · Esc cancels`, with no word for moving
  between the fields or stepping one, since the full form (`Tab/⇧Tab ←→ moves · ↑↓ steps ·
  Enter writes · Esc cancels`) does not fit beside the three fields at 78 inner cells
  (`bins/svolder/src/tui/screens/tone.rs`, `LINE_KEYS_SHORT`).
- [trade-off] **The clipping-risk note is shortened at 80 columns.** The clipping-risk note drops
  "on loud masters" at 80 columns (`peaks at +6.0 dB — keep the volume at or below -6.0 dB to avoid
  clipping`), since the whole sentence does not fit the Bands block's bottom border there; from
  about 92 columns it prints in full (`bins/svolder/src/tui/screens/tone.rs`).
- [trade-off] **A failed write does not stop a TUI preset load.** The TUI does not stop a preset
  load part-way on a failed write, unlike the CLI's `eq load`: every key's writes, presets included,
  are sent in order without waiting for the device's answer, and `take_intents` goes on past a
  write that failed, so a mid-plan failure is noticed (`App::write_failed`) but does not hold back
  the writes behind it (`bins/svolder/src/tui/run/input.rs`, `bins/svolder/src/tui/run/events.rs`).
- [trade-off] **A preset saved from the computer's points shows in the list when it is next
  opened.** Joined to a daemon, `n` on the computer's side sends the save and the list on screen
  does not refresh; closing and opening it (`p`) shows the new name
  (`bins/svolder/src/tui/run/computer.rs`).

### Web UI

- [limit] **Plain HTTP sends the sign-in over the network.** On a listen address off loopback the
  cookie, the token typed on the sign-in page and the pairing code cross the network in the
  clear; svolderd speaks no TLS, which a reverse proxy adds (`bins/svolderd/src/web.rs`).
- [limit] **A standalone window needs HTTPS.** Browsers install a web app only from a secure
  origin, so over plain HTTP "Add to Home Screen" gives an icon that opens a browser tab
  (`bins/svolderd/src/web/router.rs`).
- [limit] **A link's address on a wildcard listen is a guess.** Without `[web] url`, `svolder web
  --qr` and the TUI's Web box use the first up LAN IPv4 address, which may not be the one a phone
  reaches; and a phone signed in to an IP address loses its sign-in when that address changes,
  since the cookie belongs to the address. With `[web] hosts` set and no `[web] url`, the first LAN
  address is not among the names the server answers to, so the phone is refused (403): set `[web]
  url` (`bins/svolder/src/commands/web.rs`).
- [limit] **More than five tabs of one daemon queue their streams.** Browsers open at most six
  HTTP/1.1 connections to one host, and each tab holds one for its event stream
  (`bins/svolderd/src/web/events.rs`).
- [limit] **Guests' streams count against the server's connections.** With `guest = "read"`,
  anyone who reaches the page may hold an event stream open, and 64 of them fill the listener's
  connections until they close; keep guest read to a network you trust
  (`bins/svolderd/src/http.rs`, `bins/svolderd/src/web/events.rs`).
- [limit] **A guest's read view on a wildcard listen can be read through DNS rebinding.** With
  `guest = "read"` on `0.0.0.0` or `::` and no `[web] hosts`, the server answers any `Host`, so a
  web page a visitor on the network opens can make their browser read the guest view (device
  names, values, preset names); the config warns, and `[web] hosts` closes it
  (`crates/svolder-ipc/src/config.rs`, `bins/svolderd/src/http.rs`).
- [limit] **A pinch whose first finger already began to scroll cannot be stopped.** On the web
  page's graph, a finger put down away from every point is the page's, and once the browser has
  begun to scroll for it a second finger cannot take the touch back; put both fingers down
  together, or the first on a point, to pinch the Q (`bins/svolderd/web/themes/ember/tone-graph.js`).
- [trade-off] **The web UI's endpoints are private to its page.** Everything under `api/v1/` is
  shaped for the page and may change in any release; scripts use the CLI, the socket, MQTT or MCP
  (`bins/svolderd/src/web/api.rs`).
- [trade-off] **The sign-in page wears Ember's look in every theme.** The sign-in page is a
  template of its own that links Ember's style sheet, and no theme draws into it, so a person who
  chose another theme sees Ember's look until they are signed in
  (`bins/svolderd/templates/signin.html`).
- [trade-off] **A narrow Now playing cell in an arranged band drops its cover.** When Arrange puts
  three sections in the top band on a desktop, a Now playing cell under 400 px keeps the title, the
  artist, the buttons and the bar and leaves the cover out, since the cover and the transport do
  not both fit (`bins/svolderd/web/themes/ember/style.css`).
- [trade-off] **`svolder web` and a click on the TUI's Web UI link open the browser with the
  one-use link on `xdg-open`'s command line.** For the second or two before the page spends it,
  the code in the link is readable by other local users, so on a computer that others log into
  use `svolder web --qr` or `svolder web pair` (`bins/svolder/src/commands/web.rs`,
  `bins/svolder/src/tui/run/web_box.rs`).
- [limit] **A browser on software WebGL runs Smoke on the CPU.** A browser whose WebGL2 is drawn in
  software (llvmpipe, SwiftShader) passes the page's check for Smoke, so the page offers it and
  runs it, and the simulation then loads a processor core; pick Glow in the effects menu there
  (`bins/svolderd/web/themes/ember/smoke.js`).

## Maintainer notes

### Bluetooth and sessions

- [test, with Bifrost 3 testing] **The release bookkeeping is proven over a fake peer.** The BlueZ
  transport records each connect's task and each release's per device until it ends, so
  `settle_for` waits for its own device's work only and two settles both wait; a link nobody took
  is taken down once, through itself; a caller that leaves mid-open has the open stopped and the
  connection taken down at once; a panic in the open steps still takes it down; a link whose
  `disconnect` failed is asked again by its drop, on a recorded task. The bookkeeping is
  unit-tested over a fake peer, and the stop of a scan start whose caller left over a fake
  discovery. Only `make e2e` exercises the transport's wiring (the release closure choosing
  `link.disconnect()` or the peripheral's own take-down, `Peer for Peripheral`,
  `Discovery for Adapter`, `open` building the `Hold` with its device, the `settle_for` calls in
  `connect`), and what only an adapter shows is untested: that BlueZ's `Disconnect` cancels a
  connect still in progress, which the release of an open the caller left relies on
  (`crates/svolder-ble/src/bluez/releases.rs`, `crates/svolder-ble/src/bluez/transport.rs`).

### Daemon and IPC

- [trade-off] **`Client::request` skips a mismatched response id.** `Client::request`'s reply loop
  skips a `Reply::Response` whose id does not match the pending request (`_ => continue`) rather
  than reporting a protocol desync. Every caller today awaits each request to its end on its
  `Client`, so this fires only on a confused server, or for a caller that dropped a request future
  after its line went out, whose late answer it then passes over. An id-0 `Error` reply, which the
  server sends for a request line it cannot read, is taken as the answer to the pending request
  (`crates/svolder-ipc/src/socket/client.rs`).
- [trade-off] **`accept_loop` retries a failed `accept` forever.** `accept_loop`'s 100 ms
  retry-and-continue after a failed `accept` has no cap, so a permanently broken listener retries
  forever instead of eventually reporting itself dead; the comment beside it explains why retrying
  at all is intentional for a long-lived daemon (`crates/svolder-ipc/src/socket/server.rs`).
- [trade-off] **Link changes are kept twice.** The link `History` each device's actor answers
  `history` and `readings` with, and the `history::Link` the TUI's graphs hold (seeded from the
  `readings` answer and extended by what the TUI sees itself), are kept apart; merging them is
  debt (`bins/svolderd/src/device/link.rs`, `crates/svolder-profiles/src/history.rs`).
- [trade-off, 0.28.0] **The position and the rate are asked for with the player's own timeout.**
  The follower asks a player for `Position` and `Rate` one after the other when it follows it, on
  each signal and once more 500 ms after a new track, each bounded by the one-second call timeout,
  so a player that hangs adds up to two seconds to the stall its `Identity` read already causes
  (`crates/svolder-host/src/players.rs`).
- [trade-off, 0.28.0] **A track is first sent with no cover, then with its hash.** The cover is
  read off the host task, so a front end sees a new track with `cover: null` and then the same
  track with the picture's hash; the cover's box is fixed, so nothing moves, and waiting for the
  read would delay every track change (`bins/svolderd/src/host.rs`).
- [debt] **The web page's live stream reads `config.toml` on the daemon's runtime.** A turn of the
  stream that sends the parts or the whole model reads the file for the presets that fit and for
  whether it can be written, on an async worker instead of a blocking one, so a slow file system
  holds that stream's turn. The fleet turn does the same once a second per page: a `Devices`
  request to the daemon and a read of the file's modification time
  (`bins/svolderd/src/web/events/device.rs`, `bins/svolderd/src/web/events/page.rs`,
  `bins/svolderd/src/web/api.rs`).
- [test, with Bifrost 3 testing] **The shared listen is proven on fakes and on one device.** Every
  actor looks for its device through one `Scanner`: the listen that runs hears every device
  looked for while it runs, answers each the moment it is heard (the one that leaves nobody
  waiting once the scan has stopped) and stops once nobody is left; a device asked for more than
  a quarter second into a listen (or as it stops) and not heard in it is answered at the end of
  the next one. The scanner is tested over scripted transports. `BluezTransport::listen`, and the
  transport's own scan lock that makes a connect's `find` wait for a running scan instead of
  meeting BlueZ `InProgress`, run only in `make e2e`, with one peripheral: two Forkbeards on one
  adapter are the test still to make: check with two devices coming back in one window, since an
  early answer's connect may end the adapter's LE scan and leave the other device unheard in that
  window (`bins/svolderd/src/scanner.rs`,
  `crates/svolder-ble/src/bluez/transport.rs`).

### CLI

- [limit] **A unit's chain and a process chain look the same.** The computer EQ finds its
  PipeWire chain by its exact command line, which is the same under svolder's user unit and under
  the background-process manager (only the process's cgroup would tell them apart, and svolder
  does not read it); while the unit is active, a process-manager chain started by hand with the
  same command line is left running, so two could stack. svolder's own apply and remove never
  produce this (`crates/svolder-host/src/eq/manage.rs`).
- [limit] **The computer EQ's chain can be signalled by pid.** On a kernel without `pidfd_open`
  (before Linux 5.3), or where it is refused for a reason other than "no such process", the
  chain is signalled by pid after its command line is checked, which leaves the short window in
  which that pid could be given to another process (`crates/svolder-host/src/eq/process.rs`).
- [trade-off] **The computer EQ's saved state is re-applied as written.** Turning the computer EQ
  on again (Mode `EQ`, on the computer's side), while it is off, puts the last one back from what
  `host-eq.toml` kept: its filters are validated and a preamp that is not a number or is above
  +30 dB is refused, but a hand-edited preamp far below the safe value applies as written, only
  quieter (`crates/svolder-host/src/eq/manage.rs`,
  `bins/svolder/src/tui/boxes/presets/computer.rs`).
- [trade-off] **The computer EQ reads PipeWire's graph once a second while it is on.** The
  controller runs one `pw-dump` every 20 ticks for as long as the chain is on and live, whether
  or not anyone watches: it is how a change of the sink's rate or a restart of the chain is seen
  within about a second and every slot sent again at the new rate. On a graph with many nodes the
  dump is large. A narrower read (a dump on the sink's format change only) would need a PipeWire
  subscription the code does not have (`crates/svolder-host/src/eq/control.rs`).
- [trade-off] **The status line hardcodes the word "Status".** `format.rs`'s status line hardcodes
  the word "Status", dropping the wire's Command/Status distinction; harmless today because
  `ValueObject`, the only thing this renders, represents a device's reported readings and never
  carries a Command (`bins/svolder/src/format.rs`).

### TUI: device screen

- [trade-off] **A history graph's columns are recomputed every frame.** `graph::columns` walks
  every sample inside the span for each graph on each draw, and `Link::columns` copies up to 4096
  changes. Measured in release: about 115 µs per series at the cap (14,400 samples, 60-minute
  span), 4.6 ms for 40 such graphs; a Kraken's five meters at 60 minutes cost about 0.6 ms per
  frame, a tenth of that at the default 5 minutes. A cache would have to be keyed on the sample
  count and a column boundary of a window that slides with every instant, which changes what a
  frame draws between boundaries, so it is left until a frame budget asks for it
  (`bins/svolder/src/tui/widgets/graph.rs`).

### TUI: effects

- [trade-off] **The trace starts one frame after its screen appears.** The animation layer is
  armed from the frame that has just been drawn (`arm_effects`), so under the `trace` transition
  (the default) a new screen's trace, or its body's sweep, starts one frame after the frame that
  first shows the screen: the screen is visible untraced for that frame. The `noise` and `melt`
  transitions start before that frame (`Effects::on_navigation`) and cover it. Arming the trace
  before the draw would also need its first frame held at its start while everything else running
  advances by the time since the frame before, which the one `process_effects` call a frame makes
  does not offer (`bins/svolder/src/tui/run.rs`, `bins/svolder/src/tui/run/look.rs`,
  `bins/svolder/src/tui/decoration/effects/passage.rs`).
- [trade-off] **Two narrow effects edges stay by design.** A terminal shrunk below 80×24 while an
  overlay is open and grown back before it closes traces nothing when the overlay lifts: the frames
  under an overlay are never handed to `Effects::appeared`, so the ones recorded before it count as
  still drawn. The light running round a focused frame can paint a wider catch-up flash than its
  usual three-cell band on the lap that also turns `active()` false, which can sit on the contour
  for up to one slow tick; noted as a bounded, self-correcting edge in `lap`'s doc
  (`bins/svolder/src/tui/decoration/effects/keyed.rs`, `bins/svolder/src/tui/run/look.rs`).

### TUI: event loop and opening devices

- [trade-off] **`render::frame_areas` derives the layout the screens drew.** `frame_areas` calls
  the same pure layout functions (`curve_of`, `fault_line`, `logo_of`, each screen's `frames_of`)
  that the screens' `draw` functions use, rather than reading it back from them: the alternative
  changes five `draw` signatures for a function that cannot disagree with itself, and `render`'s
  `assert_bordered` cases compare what `frame_areas` reports with the cells drawn
  (`bins/svolder/src/tui/render.rs`).

### Profiles and presets

- [limit] **The peak search assumes one peak per half octave.** `response`'s peak refinement runs
  a golden-section search over the two sample intervals either side of the best of 1024 samples
  and over a half-octave window round each filter's centre, and a golden-section search finds one
  maximum per window: two narrow boosts almost on top of each other could hide a peak the grid
  only covers coarsely, leaving the computer EQ's preamp, and the clipping-risk mark the `eq` verbs
  and the tone screen show, a fraction of a decibel short
  (`crates/svolder-profiles/src/tone/response.rs`).
- [trade-off] **Built-in presets are keyed by band count.** `tone::preset::builtins` gives Warm,
  Bass and Presence to a five- or three-band tone control by its band count alone (`shaped`), as
  fixed Hz, dB and Q chosen for the mesh-eq profiles' own bands: a future five- or three-band
  profile with other centres would still get a correct Flat (on its own centres) but a Warm, Bass
  and Presence shaped for mesh-eq's frequencies rather than its own. Harmless today: only
  `mesh-eq-5` and `mesh-eq-3` carry an `[eq]` block (`crates/svolder-profiles/src/tone/preset.rs`).
- [trade-off] **Fits match only within one build.** A fit is a floating-point optimisation whose
  exponentials, logarithms and sines come from the C library the binary is linked with: a CLI
  built against glibc (`make install`) beside a static musl daemon (the release packages', the
  container's) may land a fitted step one apart, so the two could disagree on whether an imported
  preset is `on`. Routing those calls through the pure-Rust `libm` crate would make every build
  agree, but no test could show it short of running the suite under musl
  (`crates/svolder-profiles/src/tone/fit.rs`, `crates/svolder-profiles/src/tone/response.rs`).

### Web page

- [debt, with Ember] **The band helpers exist twice.** `bandSteps`, `BAND_PARAM` and `bandWriter`
  moved to the core for Paper, and Ember's `tone-graph.js` keeps its own copies, because this
  release changes no file of Ember's (`bins/svolderd/web/core/tone-drag.js`,
  `bins/svolderd/web/themes/ember/tone-graph.js`).
- [bug] **After no theme can start, the menu still draws the failed theme's rows.** When a theme
  fails to load and Ember fails as well, the page shows a popup with a reload link, but `core.theme`
  still names the theme that was bound last, so the menu button opens that theme's rows, and a pick
  in the picker closes it with no word (`bins/svolderd/web/core/switch.js`).
- [trade-off] **The `ember-parts` fixture is 4.25 MB.** It holds the markup of every part of every
  device kind, so the node check compares the browser-drawn page with the daemon's own drawing
  byte for byte; a smaller fixture would prove less (`scripts/web-checks/node/fixtures/ember-parts.json`).
- [debt] **A raise the sender drops overwrites the run's last answer.** A volume raise that waited
  behind a refused write and is dropped sets `state.last` to its own `dropped` answer, so the call
  that started the run resolves with the dropped raise's word instead of the write that was
  refused (`bins/svolderd/web/core/actions.js`).
- [debt, with Path] **The fleet handler prunes followed devices without opening the stream
  again.** A `fleet` event that no longer lists a followed device forgets it and takes it off the
  list, but the open stream is not opened again without it, so it goes on asking for the device
  until the next reopen (`bins/svolderd/web/core/stream.js`).
- [debt, with Paper] **The theme style check is name-based, and a theme script that timed out may
  still run.** `theme-css-check.py` finds the frame's selectors by their names, so a theme that
  styles one through another selector is not seen; a theme script that timed out is given up on,
  but the browser can still run it when it arrives late
  (`scripts/theme-css-check.py`, `bins/svolderd/web/core/switch.js`).
- [bug, with Paper] **`?theme=paper` resets a stored Dark to Auto when the cookie names another
  theme.** The page paints the scheme the server sent and takes it as the person's choice at boot,
  so a link that switches the theme while another theme's cookie is set loses a stored Dark; the
  alternative is a flash of the wrong scheme (`bins/svolderd/web/core/core.js`).
- [debt, with Paper] **While a list or a form holds a section, a lost or waking sheet stays
  clickable.** The sheet goes inert only when nothing is held, and an open list or form holds its
  section; the core still drops raises while the page is not open, so no volume is written
  (`bins/svolderd/web/themes/paper/live.js`).
- [debt, with Paper] **A tap that cannot write goes through a turned-away `setMode`.** Paper's
  `refuse()` asks the core to set the mode it is already in, so that the core turns the tap away
  and says why; the core has no public helper that only says it
  (`bins/svolderd/web/themes/paper/tone-wire.js`).
- [debt] **The page rounds ties up where the daemon rounds half to even.** The page's `toFixed`
  takes a value that falls on a tie up, and the daemon's formatter takes it to the even digit; the
  node fixture's tolerance hides the difference (`bins/svolderd/web/themes/ember/draw/`).
- [debt] **A hop answer is stale after a failed reload, and an edit reads the hop before the
  lock.** After a failed reload of the configuration a hop write answers the list it last held,
  and an edit reads the hop it changes before it takes the file's lock. The CLI's `hops` verbs do
  the same (`bins/svolderd/src/web/writes/config_edit.rs`,
  `bins/svolder/src/commands.rs`).
- [test] **A stream's `close()` and its wait while lost are untested.** That `close()` removes its
  listeners, and that a woken page keeps waiting through a lost link, have no check of their own
  (`bins/svolderd/web/core/stream.js`).
- [test] **The hand-run `ember-parts` mutants are not a re-runnable check.** The mutants listed in
  the coverage notes were run by hand and each was killed; no script runs them again
  (`scripts/web-checks/COVERAGE.md`).

### Simulator and fake transport

- [trade-off] **`FakeDevice::handle` panics on a restricted frame.** `FakeDevice::handle` panics
  on a firmware-update frame (the `210` family but the `210.2` version query), which svolder never
  sends, so a test whose code sends one fails on the spot. In `svolder-sim` the panic lands in a
  GATT write callback while the device mutex is held: the process stays up, but the poisoned
  mutex makes every later write, and the `--signal` ticks, panic too, so the simulator stops
  answering until it is restarted. A panic hook that ends the process would show that sooner; it
  is left out because only a frame the codec already refuses to build can reach the panic
  (`crates/svolder-fake/src/device.rs`, `bins/svolder-sim/src/peripheral.rs`).
- [trade-off] **`drop_links` disconnects every device on its adapter.** `svolder-sim`'s `drop_links`
  disconnects every device connected on its adapter, which is fine for a dedicated e2e controller
  and not for one shared with anything else (`bins/svolder-sim/src/peripheral.rs`).
- [trade-off] **The devices `SVOLDER_FAKE_MORE` adds take few knobs.** The other `SVOLDER_FAKE_*`
  knobs (a fault, a severed or vanished link, the code, the firmware) describe the first fake
  device alone: each added device is seeded for its profile with the code `1234` and firmware `3`
  and never drops its link. `SVOLDER_FAKE_SIGNAL` is the exception: it plays on the meters of
  every device that has them (`crates/svolder-ble/src/fake/fleet.rs`).

### Tests and tooling

- [observed] **The `playing` check of Ember fails now and then in Firefox.** In the runs of
  0.31.2 it failed three times, with a 2 px overflow of the title marquee under load, a
  geckodriver websocket 405 and a websockets `InvalidMessage`, and each time passed when run alone;
  a failed full run is rerun with `python3 scripts/web-checks/playing.py firefox`
  (`scripts/web-checks/playing.py`).
- [observed] **`make e2e` fails after about fifty runs until bluetoothd restarts.** Once
  bluetoothd has handed out GATT handles past 0x3ff on the peripheral adapter, about fifty runs
  after it started, `make e2e` fails at the first reconnect after a simulator restart
  (`ATT error: 0x01`): BlueZ 5.85 stops updating its GATT Database Hash there, so the central
  keeps a stale cache. Restart bluetoothd (`scripts/e2e.sh`).
- [observed] **A few `haze.py` checks fail about one run in ten.** The haze's side sources wander
  from random phases that the wide and resize sections do not seed, so a side band or the column
  is read at an unlucky moment now and then; running `haze.py` again in that browser passes. The
  phone section is seeded through `effects.seed`; seeding the others needs the haze's random drawn
  from the seeded generator too (`scripts/web-checks/haze.py`, `bins/svolderd/web/themes/ember/smoke.js`).
- [observed] **`sources-chromium` failed once on a bound of the dye sum.** One full `make
  web-checks` failed its "40 full-size steps, one per frame: finite, max |v| < 60, the dye sum
  under 2.2 x the resting cloud" check with a dye sum of 5071.67 against a bound of 2.2 x 2305.26
  = 5071.56, 0.002 % over; the same check passed 45 of 45 three times run alone, and no smoke
  file had changed. The bound is not loosened; running `sources.py` again in that browser passes
  (`scripts/web-checks/sources.py`).
- [trade-off] **Firefox's Content Security Policy reports are not seen.** `make web-shots` fails on
  any uncaught error Chromium or Firefox logs while a page loads, and `make web-checks` on every
  error a script or a click raises after the load in both (`scripts/web-checks/`); both fail on any
  Content Security Policy report Chromium logs, and Firefox writes none in this mode. The policy is
  the daemon's one header, so Chromium's check stands for both (`scripts/web-shots.sh`).
- [test] **Two `backend` tests fail inside a container.** `make check` in a container passes but
  for these two: `daemon_install_writes_where_it_is_told_and_runs_the_systemctl_it_is_given` meets
  `daemon install`'s refusal to write user units in a container, and
  `the_chain_prints_nothing_of_its_own_when_a_daemon_will_not_come_up` expects BlueZ's answer for
  an adapter that does not exist and gets no system D-Bus to ask instead
  (`bins/svolder/tests/backend.rs`).
- [test] **Animated-path helpers pair a capable layer with a no-colour theme on purpose.**
  `animated` draws on the monochrome theme so that what the layer paints stands out, which
  `shows_effects` would refuse (it is named `capable_over_no_colour`); a non-monochrome theme the
  layer would never run on is refused by `animated_with`
  (`bins/svolder/src/tui/run/look/tests.rs`).
- [test] **An unnamed run takes BlueZ's first adapter, which no dongle test can steer.** With no
  `--adapter`, no configured `adapter` and no remembered one, the transport opens the first adapter
  BlueZ lists, which on a machine with a built-in controller need not be the one that hears the
  device (`bins/svolder/src/backend/direct.rs`).
- [test] **The happy path of `tui::run` and `tui::wizard::setup` through a real terminal is
  untested.** The refusals and the choice of front end are tested over a `Claim` seam; a real
  `Guard` cannot be built in a unit test (`bins/svolder/src/tui/terminal.rs`).
- [test] **No loop-level test moves a preset from the DAC to the computer and back.** The guard that
  turns the computer EQ off before a DAC load has a unit test, but nothing runs the whole `Intent`
  from the preset list through the loop to the DAC's writes (`bins/svolder/src/tui/run/computer.rs`).
- [test] **The joined write queue's bound of 64 is untested.** The TUI's queue of computer edits
  keeps the newest live value per point and every `done`, and caps at 64 by dropping only the
  oldest live values; `Other` and `Last` items are never dropped. Each joined request times out,
  so the queue drains even behind a daemon that does not answer, and a test can fill it with a
  never-ending first item (`bins/svolder/src/tui/run/computer.rs`).
- [test] **Night drive's mouse hover over the tone band has no browser check.** The touch band of
  the curve (±12 dB and a fingertip) is checked for touch and tap; the hover cursor that follows the
  same band is not (`bins/svolderd/web/themes/nightdrive/`).
